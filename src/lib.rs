//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.

mod error;

pub use crate::error::{TreeError, TreeResult};
use deltoid::{Apply, Core, Delta, DeltaError, DeltaResult, FromDelta, IntoDelta};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Debug};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArenaTree<D: Clone + Debug + Default + PartialEq> {
    nodes: Vec<Node<D>>,
    garbage: VecDeque<NodeIdx>,
}

#[allow(unused)]
impl<D> ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{
    #[inline(always)]
    pub fn new() -> Self {
        Self::with_capacity(64)
    }

    #[inline(always)]
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(node_count),
            garbage: VecDeque::new(),
        }
    }

    #[inline(always)]
    pub fn root_ref(&self) -> TreeResult<&Node<D>> {
        Ok(self.node_ref(NodeIdx::ROOT)?)
    }

    #[inline(always)]
    pub fn root_mut(&mut self) -> TreeResult<&mut Node<D>> {
        Ok(self.node_mut(NodeIdx::ROOT)?)
    }

    #[inline(always)]
    pub fn node_ref(&self, idx: NodeIdx) -> TreeResult<&Node<D>> {
        self.nodes
            .get(idx.0)
            .ok_or(TreeError::NoNodeForNodeIdx { idx })
    }

    #[inline(always)]
    pub fn node_mut(&mut self, idx: NodeIdx) -> TreeResult<&mut Node<D>> {
        self.nodes
            .get_mut(idx.0)
            .ok_or(TreeError::NoNodeForNodeIdx { idx })
    }

    pub fn new_node<P>(&mut self, parent: P) -> TreeResult<NodeIdx>
    where
        P: Into<Option<NodeIdx>>,
    {
        if let Some(cidx) = self.garbage.pop_front() {
            self[cidx].parent = parent.into();
            self[cidx].data = D::default();
            if let Some(pidx) = self[cidx].parent {
                self[pidx].add_child(cidx)
            }
            Ok(cidx)
        } else {
            let parent: Option<NodeIdx> = parent.into();
            let cidx = NodeIdx(self.nodes.len());
            self.nodes.push(Node {
                idx: cidx,
                parent,
                children: vec![],
                data: D::default(),
            });
            if let Some(pidx) = parent {
                self[pidx].add_child(cidx);
            }
            Ok(cidx)
        }
    }

    #[rustfmt::skip]
    fn destroy_node(&mut self, idx: NodeIdx) -> TreeResult<()> {
        if let Some(pidx) = self[idx].parent {
            // Filter out the NodeIdx from the parent's child indices
            self[pidx].children = self[pidx].children.drain(..)
                .filter(|&child_idx| child_idx != idx)
                .collect();
        }
        self[idx].parent = None;
        self[idx].children = vec![];
        self[idx].data = D::default();
        self.garbage.push_back(idx);
        Ok(())
    }

    /// Copy a `src` tree to `self`, making it a subtree of `self` in the process.
    /// Specifically, `src[ROOT]` becomes a child node of `self[dst_node_idx]`.
    /// Note that the `NodeIdx`s of the nodes in `src` will *NOT* be valid in `self`.
    pub fn add_subtree(&mut self, dst_node_idx: NodeIdx, src: &Self) -> TreeResult<()> {
        type SrcTreeIdx = Option<NodeIdx>;
        type DstTreeIdx = NodeIdx;
        let mut map = HashMap::<SrcTreeIdx, DstTreeIdx>::new();
        map.insert(None, dst_node_idx);
        let dst = self;
        for src_node_idx in src.dfs(NodeIdx::ROOT)? {
            let src_parent_idx = src[src_node_idx].parent;
            let dst_parent_idx = map.get(&src_parent_idx).map(|&idx| idx);
            let dst_node_idx = dst.new_node(dst_parent_idx)?;
            dst[dst_node_idx].data = src[src_node_idx].data.clone();
            map.insert(Some(src_node_idx), dst_node_idx);
        }
        Ok(())
    }

    pub fn remove_subtree(&mut self, start: NodeIdx) -> TreeResult<()> {
        for nidx in self.dfs(start)?.rev(/*from the leaves toward the root*/) {
            debug_assert!(self[nidx].children.is_empty());
            self.destroy_node(nidx)?;
        }
        Ok(())
    }

    #[rustfmt::skip]
    /// Make `self[subroot_idx]` a child node of `self[parent_idx]`
    // /// If `parent_idx` is `None`,
    pub fn move_subtree(
        &mut self,
        subroot_idx: NodeIdx,
        // TODO: Add support for multiple root nodes in ArenaTree
        //       before allowing `parent_idx` to be `None`:
        // parent_idx: impl Into<Option<NodeIdx>>,
        parent_idx: NodeIdx,
    ) -> TreeResult<()> {
        // Remove `subroot_idx` from the children of the old parent node
        // of `self[subroot_idx]`, if that parent node exists:
        if let Some(old_subroot_parent_idx) = self[subroot_idx].parent {
            let children = &mut self[old_subroot_parent_idx].children;
            *children = children.drain(..)
                .filter(|&cidx| cidx != subroot_idx)
                .collect();
        }
        // Update the parent idx of `self[subroot_idx]`:
        self[subroot_idx].parent = parent_idx.into();
        // If the parent node exists, push `subroot_idx`
        // to `self[parent_idx].children`:
        if let Some(parent_idx) = self[subroot_idx].parent {
            self[parent_idx].add_child(subroot_idx);
        }
        Ok(())
    }
    /// Return an iterator over the ancestors of `idx`, starting with
    /// the parent of `idx` and going toward the root of the tree.
    #[inline(always)]
    pub fn ancestors_of<'t>(
        &'t self,
        idx: NodeIdx,
    ) -> TreeResult<impl DoubleEndedIterator<Item = NodeIdx> + 't> {
        let mut ancestors = vec![];
        let mut current: NodeIdx = idx;
        while let Some(pidx) = self[current].parent {
            ancestors.push(pidx);
            current = pidx;
        }
        Ok(ancestors.into_iter())
    }

    #[inline(always)]
    pub fn children_of<'t>(
        &'t self,
        idx: NodeIdx,
    ) -> TreeResult<impl DoubleEndedIterator<Item = NodeIdx> + 't> {
        Ok(self[idx].children())
    }

    /// Return an iterator over the nodes, in DFS order,
    /// of the subtree rooted in `start`.
    pub fn dfs(&self, start: NodeIdx) -> TreeResult<impl DoubleEndedIterator<Item = NodeIdx>> {
        let mut output: Vec<NodeIdx> = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<NodeIdx> = vec![start];
        while let Some(idx) = stack.pop() {
            output.push(idx);
            stack.extend(self[idx].children().rev());
        }
        Ok(output.into_iter())
    }

    #[rustfmt::skip]
    #[allow(unused)]
    /// Return an iterator, in BFS order, over the `NodeIdx`s
    /// of the nodes of the the subtree rooted in `start`.
    pub fn bfs(
        &self,
        start: NodeIdx
    ) -> TreeResult<impl DoubleEndedIterator<Item = NodeIdx>> {
        type Layer = Vec<NodeIdx>;
        let start_layer: Layer = vec![start];
        let mut layers: Vec<Layer> = vec![start_layer];
        let mut current: Layer = vec![];
        while let Some(ref previous) = layers.last() {
            for &idx in previous.iter() {
                current.extend(self[idx].children());
            }
            if current.is_empty() {
                break;
            }
            layers.push(current);
            current = vec![];
        }
        Ok(layers.into_iter().flat_map(|layer| layer.into_iter()))
    }
}

impl<D> std::ops::Index<NodeIdx> for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{
    type Output = Node<D>;

    fn index(&self, idx: NodeIdx) -> &Self::Output {
        unsafe { self.nodes.get_unchecked(idx.0) }
    }
}

impl<D> std::ops::IndexMut<NodeIdx> for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Self::Output {
        unsafe { self.nodes.get_unchecked_mut(idx.0) }
    }
}

impl<D> fmt::Display for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // NOTE: This loop is `O(D * N)`, where:
        //       - D is the maximum depth of `self`
        //       - N is the number of nodes in `self`
        for node_idx in self.dfs(NodeIdx::ROOT).unwrap(/*TreeResult*/) {
            let num_ancestors: usize = self.ancestors_of(node_idx)
                .unwrap(/*TreeResult*/)
                .count();
            for _ in 0..num_ancestors {
                write!(f, "| ")?; /* no newline */
            }
            let Node { idx, data, .. } = &self[node_idx];
            writeln!(f, "{idx} {data:?}")?;
        }
        Ok(())
    }
}

impl<D: Serialize> Serialize for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq + Serialize,
{
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        const NUM_FIELDS: usize = 2;
        let mut state = serializer.serialize_struct("ArenaTree", NUM_FIELDS)?;
        state.serialize_field("nodes", &self.nodes)?;
        state.serialize_field("garbage", &self.garbage)?;
        state.end()
    }
}

impl<'de, D> Deserialize<'de> for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
{
    fn deserialize<DE: Deserializer<'de>>(d: DE) -> Result<Self, DE::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Nodes,
            Garbage,
        }

        #[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
        struct ArenaTreeVisitor<D>(std::marker::PhantomData<D>);

        impl<'de, D> Visitor<'de> for ArenaTreeVisitor<D>
        where
            D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
        {
            type Value = ArenaTree<D>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct ArenaTree<D>")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let nodes = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let garbage = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ArenaTree { nodes, garbage })
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut nodes = None;
                let mut garbage = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Nodes => {
                            if nodes.is_some() {
                                return Err(de::Error::duplicate_field("nodes"));
                            }
                            nodes = Some(map.next_value()?);
                        }
                        Field::Garbage => {
                            if garbage.is_some() {
                                return Err(de::Error::duplicate_field("garbage"));
                            }
                            garbage = Some(map.next_value()?);
                        }
                    }
                }
                Ok(ArenaTree {
                    nodes: nodes.ok_or_else(|| de::Error::missing_field("nodes"))?,
                    garbage: garbage.ok_or_else(|| de::Error::missing_field("garbage"))?,
                })
            }
        }

        d.deserialize_map(ArenaTreeVisitor(std::marker::PhantomData))
    }
}

#[rustfmt::skip]
impl<D> Core for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq
        + for<'de> Deserialize<'de> + Serialize
        + Core,
{
    type Delta = ArenaTreeDelta<D>;
}

#[rustfmt::skip]
impl<D> Apply for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq
        + for<'de> Deserialize<'de> + Serialize
        + Apply,
{
    fn apply(&self, delta: Self::Delta) -> DeltaResult<Self> {
        match delta.0 {
            Some(d) => Ok(d),
            None => Err(DeltaError::FailedToApplyDelta {
                reason: format!("Expected a `ArenaTreeDelta(Some(_))` value"),
            }),
        }
    }
}

#[rustfmt::skip]
impl<D> Delta for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq
        + for<'de> Deserialize<'de> + Serialize
        + Delta,
{
    fn delta(&self, rhs: &Self) -> DeltaResult<Self::Delta> {
        Ok(ArenaTreeDelta(if self == rhs {
            None
        } else {
            Some(rhs.clone()) // TODO: improve efficiency
        }))
    }
}

#[rustfmt::skip]
impl<D> FromDelta for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq
        + for<'de> Deserialize<'de> + Serialize
        + FromDelta,
{
    fn from_delta(delta: Self::Delta) -> DeltaResult<Self> {
        match delta.0 {
            Some(d) => Ok(d),
            None => Err(DeltaError::FailedToConvertFromDelta {
                reason: format!("Expected a `ArenaTreeDelta(Some(_))` value"),
            }),
        }
    }
}

#[rustfmt::skip]
impl<D> IntoDelta for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq
        + for<'de> Deserialize<'de> + Serialize
        + IntoDelta,
{
    fn into_delta(self) -> DeltaResult<Self::Delta> {
        Ok(ArenaTreeDelta(Some(self)))
    }
}

#[rustfmt::skip]
#[derive(
    Clone,
    Debug,
    PartialEq,
    serde_derive::Serialize,
    serde_derive::Deserialize
)]
pub struct ArenaTreeDelta<D>(Option<ArenaTree<D>>)
where
    D: Clone + Debug + Default + PartialEq + Core;

#[rustfmt::skip]
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    deltoid_derive::Delta,
    Deserialize,
    Serialize,
)]
pub struct Node<D> {
    pub idx: NodeIdx,
    pub parent: Option<NodeIdx>,
    pub children: Vec<NodeIdx>,
    pub data: D,
}

impl<D> Node<D> {
    #[inline(always)]
    pub fn node_idx(&self) -> NodeIdx {
        self.idx
    }

    #[deprecated]
    #[inline(always)]
    pub fn parent(&self) -> Option<NodeIdx> {
        self.parent
    }

    #[deprecated]
    #[inline(always)]
    pub fn set_parent(&mut self, parent: NodeIdx) {
        self.parent = Some(parent);
    }

    #[inline(always)]
    pub fn children<'n>(&'n self) -> impl DoubleEndedIterator<Item = NodeIdx> + 'n {
        self.children.iter().map(|&idx| idx)
    }

    pub fn count_children(&self) -> usize {
        self.children.len()
    }

    #[inline(always)]
    pub fn add_child(&mut self, child: NodeIdx) {
        self.children.push(child);
    }

    #[deprecated]
    #[inline(always)]
    pub fn data_ref(&self) -> &D {
        &self.data
    }

    #[deprecated]
    #[inline(always)]
    pub fn data_mut(&mut self) -> &mut D {
        &mut self.data
    }
}

impl<D> std::ops::Deref for Node<D> {
    type Target = D;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<D> std::ops::DerefMut for Node<D> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<D: Debug> fmt::Debug for Node<D> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut ds = f.debug_struct("Node");
        let ds = ds.field("idx", &self.idx);
        let ds = if let Some(parent) = self.parent {
            ds.field("parent", &parent)
        } else {
            ds.field("parent", &format_args!("None"))
        };
        let ds = ds.field("children", &self.children);
        let ds = ds.field("data", &self.data);
        ds.finish()
    }
}

#[rustfmt::skip]
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    deltoid_derive::Delta,
    Deserialize,
    Serialize,
)]
pub struct NodeIdx(pub(crate) usize);

impl NodeIdx {
    pub const ROOT: Self = Self(0);
}

impl fmt::Debug for NodeIdx {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "NodeIdx({})", self.0)
    }
}

impl fmt::Display for NodeIdx {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::ops::Add<usize> for NodeIdx {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: usize) -> Self {
        Self(self.0 + rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Data {
        tree: ArenaTree<()>,
        root: NodeIdx,
        ancestors_order: Vec<NodeIdx>,
        bfs_order: Vec<NodeIdx>,
        dfs_order: Vec<NodeIdx>,
        subtree_bfs_order: Vec<NodeIdx>,
        subtree_dfs_order: Vec<NodeIdx>,
    }

    fn make_data() -> TreeResult<Data> {
        let mut tree: ArenaTree<_> = ArenaTree::new();
        let root: NodeIdx = tree.new_node(None)?;
        let node0: NodeIdx = tree.new_node(root)?;
        let node00: NodeIdx = tree.new_node(node0)?;
        let node01: NodeIdx = tree.new_node(node0)?;
        let node1: NodeIdx = tree.new_node(root)?;
        let node10: NodeIdx = tree.new_node(node1)?;
        let node2: NodeIdx = tree.new_node(root)?;
        let node20: NodeIdx = tree.new_node(node2)?;
        let node200: NodeIdx = tree.new_node(node20)?;
        let node21: NodeIdx = tree.new_node(node2)?;
        Ok(Data {
            tree,
            root,
            ancestors_order: vec![node20, node2, root],
            bfs_order: vec![
                root, node0, node1, node2, node00, node01, node10, node20, node21, node200,
            ],
            dfs_order: vec![
                root, node0, node00, node01, node1, node10, node2, node20, node200, node21,
            ],
            subtree_bfs_order: vec![root, node1, node2, node10, node20, node21, node200],
            subtree_dfs_order: vec![root, node1, node10, node2, node20, node200, node21],
        })
    }

    #[test]
    fn dfs_traversal() -> TreeResult<()> {
        let data = make_data()?;
        let dfs_order: Vec<_> = data.tree.dfs(data.root)?.collect();
        assert_eq!(dfs_order, data.dfs_order);
        Ok(())
    }

    #[test]
    fn bfs_traversal() -> TreeResult<()> {
        let data = make_data()?;
        let bfs_order: Vec<_> = data.tree.bfs(data.root)?.collect();
        assert_eq!(bfs_order, data.bfs_order);
        Ok(())
    }

    #[test]
    fn add_subtree() -> TreeResult<()> {
        let mut tree: ArenaTree<String> = ArenaTree::new();
        let root: NodeIdx = tree.new_node(None)?;
        let node0: NodeIdx = tree.new_node(root)?;
        let node1: NodeIdx = tree.new_node(root)?;
        let node10: NodeIdx = tree.new_node(node1)?;
        let node100: NodeIdx = tree.new_node(node10)?;
        let node2: NodeIdx = tree.new_node(root)?;
        let node20: NodeIdx = tree.new_node(node2)?;
        let tree_pairs = [
            (root, "root"),
            (node0, "node0"),
            (node1, "node1"),
            (node10, "node10"),
            (node100, "node100"),
            (node2, "node2"),
            (node20, "node20"),
        ];
        for &(idx, data) in &tree_pairs {
            *tree[idx] = data.to_string();
        }

        let mut subtree: ArenaTree<String> = ArenaTree::new();
        let st_root: NodeIdx = subtree.new_node(None)?;
        let st_node0: NodeIdx = subtree.new_node(st_root)?;
        let st_node1: NodeIdx = subtree.new_node(st_root)?;
        let st_node10: NodeIdx = subtree.new_node(st_node1)?;
        let st_node100: NodeIdx = subtree.new_node(st_node10)?;
        let st_node2: NodeIdx = subtree.new_node(st_root)?;
        let st_node20: NodeIdx = subtree.new_node(st_node2)?;
        let subtree_pairs = [
            (st_root, "root (subtree)"),
            (st_node0, "node0 (subtree)"),
            (st_node1, "node1 (subtree)"),
            (st_node10, "node10 (subtree)"),
            (st_node100, "node100 (subtree)"),
            (st_node2, "node2 (subtree)"),
            (st_node20, "node20 (subtree)"),
        ];
        for &(idx, data) in &subtree_pairs {
            *subtree[idx] = data.to_string()
        }

        tree.add_subtree(node20, &subtree)?;

        let mut expected: ArenaTree<String> = ArenaTree::new();
        let e_root: NodeIdx = expected.new_node(None)?; //  0
        let e_node0: NodeIdx = expected.new_node(e_root)?; //  1
        let e_node1: NodeIdx = expected.new_node(e_root)?; //  2
        let e_node10: NodeIdx = expected.new_node(e_node1)?; //  3
        let e_node100: NodeIdx = expected.new_node(e_node10)?; //  4
        let e_node2: NodeIdx = expected.new_node(e_root)?; //  5
        let e_node20: NodeIdx = expected.new_node(e_node2)?; //  6
        let e_st_root: NodeIdx = expected.new_node(e_node20)?; //  7
        let e_st_node0: NodeIdx = expected.new_node(e_st_root)?; //  8
        let e_st_node1: NodeIdx = expected.new_node(e_st_root)?; //  9
        let e_st_node10: NodeIdx = expected.new_node(e_st_node1)?; // 10
        let e_st_node100: NodeIdx = expected.new_node(e_st_node10)?; // 11
        let e_st_node2: NodeIdx = expected.new_node(e_st_root)?; // 12
        let e_st_node20: NodeIdx = expected.new_node(e_st_node2)?; // 13
        let expected_pairs = [
            (e_root, "root"),
            (e_node0, "node0"),
            (e_node1, "node1"),
            (e_node10, "node10"),
            (e_node100, "node100"),
            (e_node2, "node2"),
            (e_node20, "node20"),
            (e_st_root, "root (subtree)"),
            (e_st_node0, "node0 (subtree)"),
            (e_st_node1, "node1 (subtree)"),
            (e_st_node10, "node10 (subtree)"),
            (e_st_node100, "node100 (subtree)"),
            (e_st_node2, "node2 (subtree)"),
            (e_st_node20, "node20 (subtree)"),
        ];
        for &(idx, data) in &expected_pairs {
            *expected[idx] = data.to_string()
        }

        assert_eq!(tree, expected, "{:#?} != {:#?}", tree, expected);
        Ok(())
    }

    #[test]
    fn remove_subtree() -> TreeResult<()> {
        let mut data = make_data()?;
        let node0 = NodeIdx(1);
        data.tree.remove_subtree(node0)?;
        let subtree_bfs_order: Vec<_> = data.tree.bfs(data.root)?.collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);
        let subtree_dfs_order: Vec<_> = data.tree.dfs(data.root)?.collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    fn ancestors() -> TreeResult<()> {
        let data = make_data()?;
        let node200 = NodeIdx(8);
        let ancestors: Vec<_> = data.tree.ancestors_of(node200)?.collect();
        assert_eq!(ancestors, data.ancestors_order);
        Ok(())
    }

    #[rustfmt::skip]
    #[test]
    fn move_subtree() -> TreeResult<()> {
        let data = make_data()?;
        let mut tree = data.tree.clone();

        let (subroot_idx, parent_idx) = (NodeIdx(6), NodeIdx(1));
        // Make `tree[subroot_idx]` a child of `tree[parent_idx]`,
        // i.e. move the subtree rooted in `tree[subroot_idx]`:
        tree.move_subtree(subroot_idx, parent_idx)?;
        let mut expected = ArenaTree::<()>::new();
        // The order in which the nodes are added is significant:
        let root_idx = expected.new_node(None)?;
        let _node1_idx = expected.new_node(root_idx)?;
        let _node2_idx = expected.new_node(_node1_idx)?;
        let _node3_idx = expected.new_node(_node1_idx)?;
        let _node4_idx = expected.new_node(root_idx)?;
        let _node5_idx = expected.new_node(_node4_idx)?;
        let _node6_idx = expected.new_node(_node1_idx)?;
        let _node7_idx = expected.new_node(_node6_idx)?;
        let _node8_idx = expected.new_node(_node7_idx)?;
        let _node9_idx = expected.new_node(_node6_idx)?;
        assert_eq!(
            tree, expected,
            "\ntree:\n{tree}\n    !=\n    expected:\n{expected}"
        );

        let (subroot_idx, parent_idx) = (NodeIdx(6), NodeIdx(0));
        // Make `tree[subroot_idx]` a child of its grandparent,
        // i.e. move the subtree back:
        tree.move_subtree(subroot_idx, parent_idx)?;
        let expected = data.tree.clone();
        assert_eq!(
            tree, expected,
            "\ntree:\n{tree}\n    !=\n    expected:\n{expected}"
        );

        Ok(())
    }
}
