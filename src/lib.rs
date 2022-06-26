//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.

mod error;

pub use crate::error::{TreeError, TreeResult};
use deltoid::{Apply, Core, Delta, DeltaError, DeltaResult, FromDelta, IntoDelta};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_derive::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::fmt::{self, Debug};

#[derive(Clone, Debug, Hash)]
pub struct ArenaTree<D: Clone + Debug + Default + PartialEq> {
    nodes: Vec<Node<D>>,
    garbage: VecDeque<NodeIdx>,
}

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

    /// Get the logical size, which is defined as `physical size - garbage size`
    /// i.e. the number of allocated, non-garbage nodes in `self`.
    #[inline]
    pub fn logical_size(&self) -> NodeCount {
        self.physical_size() - self.garbage_size()
    }

    /// Get the physical size, which is defined as the number of nodes
    /// allocated in the tree, whether they are garbage or not.
    #[inline]
    pub fn physical_size(&self) -> NodeCount {
        NodeCount(self.nodes.len())
    }

    /// Get the garbage size i.e. the number of garbage nodes in `self`.
    #[inline]
    pub fn garbage_size(&self) -> NodeCount {
        NodeCount(self.garbage.len())
    }

    #[inline(always)]
    pub fn root_ref(&self) -> &Node<D> {
        &self[NodeIdx::ROOT]
    }

    #[inline(always)]
    pub fn root_mut(&mut self) -> &mut Node<D> {
        &mut self[NodeIdx::ROOT]
    }

    #[rustfmt::skip]
    pub fn new_node(
        &mut self,
        parent_idx: impl Into<Option<NodeIdx>>,
    ) -> TreeResult<NodeIdx> {
        if let Some(cidx) = self.garbage.pop_front() {
            self[cidx].clear();
            self[cidx].parent = parent_idx.into();
            if let Some(pidx) = self[cidx].parent {
                self[pidx].add_child(cidx)
            }
            Ok(cidx)
        } else {
            let child_idx = NodeIdx(self.nodes.len());
            self.nodes.push(Node {
                idx: child_idx,
                parent: parent_idx.into(),
                children: vec![],
                data: D::default(),
            });
            if let Some(parent_idx) = self[child_idx].parent {
                self[parent_idx].add_child(child_idx);
            }
            Ok(child_idx)
        }
    }

    #[rustfmt::skip]
    fn destroy_node(&mut self, node_idx: NodeIdx) -> TreeResult<()> {
        if let Some(parent_idx) = self[node_idx].parent {
            // Filter out the NodeIdx from the parent's child indices
            self[parent_idx].remove_child_idx(node_idx);
        }
        self[node_idx].clear();
        self.garbage.push_back(node_idx);
        Ok(())
    }

    #[rustfmt::skip]
    /// Copy a `src` tree to `self`, making it a subtree of `self` in
    /// the process.  Specifically, `src[ROOT]` becomes a child node
    /// of `self[dst_node_idx]`.
    /// Note that the `NodeIdx`s of the nodes in `src` will *NOT* be
    /// valid in `self`.
    pub fn add_subtree(
        &mut self,
        dst_node_idx: NodeIdx,
        src: &Self
    ) -> TreeResult<()> {
        type SrcTreeIdx = Option<NodeIdx>;
        type DstTreeIdx = NodeIdx;
        let mut map = HashMap::<SrcTreeIdx, DstTreeIdx>::new();
        map.insert(None, dst_node_idx);
        let dst = self;
        for src_node_idx in src.dfs(NodeIdx::ROOT) {
            let src_parent_idx = src[src_node_idx].parent;
            let dst_parent_idx = map.get(&src_parent_idx).map(|&idx| idx);
            let dst_node_idx = dst.new_node(dst_parent_idx)?;
            dst[dst_node_idx].data = src[src_node_idx].data.clone();
            map.insert(Some(src_node_idx), dst_node_idx);
        }
        Ok(())
    }

    pub fn remove_subtree(&mut self, start_idx: NodeIdx) -> TreeResult<()> {
        if self.garbage.contains(&start_idx) {
            return Ok(()); // Don't try to remove garbage
        }
        for node_idx in self.dfs(start_idx).rev(/*leaves -> ... -> root*/) {
            debug_assert!(self[node_idx].children.is_empty());
            self.destroy_node(node_idx)?;
        }
        Ok(())
    }

    #[rustfmt::skip]
    /// Make `self[subroot_idx]` the last child node of `self[parent_idx]`.
    /// Returns an error if `self[subroot_idx].parent` equals `None`.
    ///
    // /// If `parent_idx` is `None`, ...
    pub fn move_subtree(
        &mut self,
        subroot_idx: NodeIdx,
        // TODO: Add support for multiple root nodes in ArenaTree
        //       before allowing `parent_idx` to be `None`:
        // parent_idx: impl Into<Option<NodeIdx>>,
        parent_idx: NodeIdx,
    ) -> TreeResult<()> {
        let old_parent_idx = self[subroot_idx].parent
            .ok_or(TreeError::ParentNotFound { node_idx: subroot_idx })?;
        self[old_parent_idx].remove_child_idx(subroot_idx);
        self[subroot_idx].parent = parent_idx.into();
        if let Some(parent_idx) = self[subroot_idx].parent {
            self[parent_idx].add_child(subroot_idx);
        }
        Ok(())
    }

    #[rustfmt::skip]
    /// Replace the subtree rooted @ `self[target_idx]` with the subtree
    /// rooted @ `self[subroot_idx]`.  This means that `self[target_idx]`
    /// is removed from `self` and `self[subroot_idx]` takes its place.
    pub fn replace_subtree(
        &mut self,
        target_idx: NodeIdx,
        subroot_idx: NodeIdx,
    ) -> TreeResult<()> {
        let parent_idx = self[target_idx].parent
            // TODO: Remove the `.ok_or()` line below, as well as the
            //       `TreeError::ParentNotFound` enum variant used
            //       in it, after adding multiple root support in
            //       ArenaTree as well as in the self.move_subtree()
            //       method definition above:
            .ok_or(TreeError::ParentNotFound { node_idx: target_idx })?
            ;
        self.move_subtree(subroot_idx, parent_idx)?;
        // Rather than having the ordinal number of `self[target_idx]`,
        // `self[subroot_idx]` is the last child of `self[parent_idx]`.
        // Let's rectify that by swapping the 2:
        let ordinal = self[parent_idx].get_child_ordinal(target_idx)
            .unwrap(/*should be safe*/);
        let removed_node_idx = self[parent_idx].children.swap_remove(ordinal);
        assert_eq!(target_idx, removed_node_idx);
        self.remove_subtree(target_idx)?; // Clean up
        Ok(())
    }

    /// Return an iterator over the ancestors of `idx`, starting with
    /// the parent of `idx` and going toward the root of the tree.
    #[inline(always)]
    pub fn ancestors_of<'t>(
        &'t self,
        node_idx: NodeIdx,
    ) -> TreeResult<impl DoubleEndedIterator<Item = NodeIdx> + 't> {
        let mut ancestors = vec![];
        let mut current: NodeIdx = node_idx;
        while let Some(parent_idx) = self[current].parent {
            ancestors.push(parent_idx);
            current = parent_idx;
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

    #[rustfmt::skip]
    /// Return an iterator over the nodes, in DFS order,
    /// of the subtree rooted in `start_idx`.
    pub fn dfs(
        &self,
        start_idx: NodeIdx
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        let mut output: Vec<NodeIdx> = Vec::with_capacity(self.nodes.len());
        let mut stack: Vec<NodeIdx> = vec![start_idx];
        while let Some(node_idx) = stack.pop() {
            output.push(node_idx);
            stack.extend(self[node_idx].children().rev());
        }
        output.into_iter()
    }

    #[rustfmt::skip]
    /// Return an iterator, in BFS order, over the `NodeIdx`s
    /// of the nodes of the the subtree rooted in `start_idx`.
    pub fn bfs(
        &self,
        start_idx: NodeIdx
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        type Layer = Vec<NodeIdx>;
        let mut layers: Vec<Layer> = vec![Layer::from([start_idx])];
        let mut current: Layer = vec![];
        while let Some(previous_layer) = layers.last() {
            for &idx in previous_layer {
                current.extend(self[idx].children());
            }
            if current.is_empty() {
                break;
            }
            layers.push(current);
            current = vec![];
        }
        layers.into_iter().flat_map(|layer| layer.into_iter())
    }
}

#[rustfmt::skip]
impl<D> PartialEq<Self> for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        if self.logical_size() != other.logical_size() {
            return false;
        }
        let snode_iter =  self.dfs(NodeIdx::ROOT);
        let onode_iter = other.dfs(NodeIdx::ROOT);
        let mut map = HashMap::new();
        for (snode_idx, onode_idx) in snode_iter.zip(onode_iter) {
            map.insert(snode_idx, onode_idx);
            let (snode, onode) = (&self[snode_idx], &other[onode_idx]);
            let (sdata, odata) = (&**snode, &**onode);
            match (snode.parent, onode.parent) {
                (None, None) => {/*NOP*/},
                (Some(spidx), Some(opidx)) if map[&spidx] == opidx => {/*NOP*/},
                _ => return false,
            }
            if snode.count_children() != onode.count_children() {
                return false;
            }
            if sdata != odata {
                return false;
            }
        }
        true
    }
}

#[rustfmt::skip]
impl<D> Eq for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq,
{}

#[rustfmt::skip]
impl<D> PartialOrd<Self> for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq + PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().partial_cmp(&other.logical_size());
        if let Some(Ordering::Greater | Ordering::Less) = size_cmp {
            return size_cmp;
        }
        let snode_iter =  self.dfs(NodeIdx::ROOT);
        let onode_iter = other.dfs(NodeIdx::ROOT);
        let mut map = HashMap::new();
        for (snode_idx, onode_idx) in snode_iter.zip(onode_iter) {
            map.insert(snode_idx, onode_idx);
            let (snode, onode) = (&self[snode_idx], &other[onode_idx]);
            let (sdata, odata) = (&**snode, &**onode);
            match (snode.parent, onode.parent) {
                (None, None) => {/*NOP*/},
                (Some(spidx), Some(opidx)) if map[&spidx] == opidx => {/*NOP*/},
                _ => return snode.parent.partial_cmp(&onode.parent),
            }
            let child_count_cmp = snode.count_children()
                .partial_cmp(&onode.count_children());
            if let Some(Ordering::Greater | Ordering::Less) = child_count_cmp {
                return child_count_cmp;
            }
            let data_cmp = sdata.partial_cmp(&odata);
            if let Some(Ordering::Greater | Ordering::Less) = data_cmp {
                return data_cmp;
            }
        }
        Some(Ordering::Equal)
    }
}

#[rustfmt::skip]
impl<D> Ord for ArenaTree<D>
where
    D: Clone + Debug + Default + PartialEq + Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().cmp(&other.logical_size());
        if let Ordering::Greater | Ordering::Less = size_cmp {
            return size_cmp;
        }
        let snode_iter =  self.dfs(NodeIdx::ROOT);
        let onode_iter = other.dfs(NodeIdx::ROOT);
        let mut map = HashMap::new();
        for (snode_idx, onode_idx) in snode_iter.zip(onode_iter) {
            map.insert(snode_idx, onode_idx);
            let (snode, onode) = (&self[snode_idx], &other[onode_idx]);
            let (sdata, odata) = (&**snode, &**onode);
            match (snode.parent, onode.parent) {
                (None, None) => {/*NOP*/},
                (Some(spidx), Some(opidx)) if map[&spidx] == opidx => {/*NOP*/},
                _ => return snode.parent.cmp(&onode.parent),
            }
            let child_count_cmp = snode.count_children()
                .cmp(&onode.count_children());
            if let Ordering::Greater | Ordering::Less = child_count_cmp {
                return child_count_cmp;
            }
            let data_cmp = sdata.cmp(&odata);
            if let Ordering::Greater | Ordering::Less = data_cmp {
                return data_cmp;
            }
        }
        Ordering::Equal
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
        for node_idx in self.dfs(NodeIdx::ROOT) {
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
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde_derive::Serialize,
    serde_derive::Deserialize
)]
pub struct NodeCount(usize);

impl std::ops::Deref for NodeCount {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::Add<Self> for NodeCount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub<Self> for NodeCount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

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

    #[inline(always)]
    pub fn children<'n>(&'n self) -> impl DoubleEndedIterator<Item = NodeIdx> + 'n {
        self.children.iter().map(|&idx| idx)
    }

    pub fn count_children(&self) -> usize {
        self.children.len()
    }

    #[inline(always)]
    pub fn add_child(&mut self, child_idx: NodeIdx) {
        self.children.push(child_idx);
    }

    /// Filter out `child_idx` from `self.children`.
    /// Has no effect if `self.children` does not contain `child_idx`.
    #[inline]
    #[rustfmt::skip]
    pub fn remove_child_idx(&mut self, child_idx: NodeIdx) {
        if !self.children.contains(&child_idx) {
            return;
        }
        self.children = self.children.drain(..)
            .filter(|&cidx| cidx != child_idx)
            .collect();
    }

    #[inline(always)]
    pub fn clear(&mut self)
    where
        D: Default,
    {
        self.parent = None;
        self.children.clear();
        self.data = D::default();
    }

    /// Assuming that `child_idx` is a member of `self.children`, return the
    /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    /// for `child_idx` e.g. the leftmost child is the `zeroth` child of
    /// `self`.
    /// Return `None` if `child_idx` is not a member of `self.children`.
    #[inline]
    #[rustfmt::skip]
    pub fn get_child_ordinal(&self, child_idx: NodeIdx) -> Option<usize> {
        self.children.iter().enumerate()
            .filter(|(_, &cidx)| cidx == child_idx)
            .map(|(ordinal_numeral, _)| ordinal_numeral)
            .next()
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

    #[derive(Debug)]
    struct Data {
        tree: ArenaTree<()>,
        root: NodeIdx,
        ancestors_order: Vec<NodeIdx>,
        bfs_order: Vec<NodeIdx>,
        dfs_order: Vec<NodeIdx>,
        subtree_bfs_order: Vec<NodeIdx>,
        subtree_dfs_order: Vec<NodeIdx>,
    }

    #[rustfmt::skip]
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
                root,
                node0,
                node1,
                node2,
                node00,
                node01,
                node10,
                node20,
                node21,
                node200,
            ],
            dfs_order: vec![
                root,
                node0,
                node00,
                node01,
                node1,
                node10,
                node2,
                node20,
                node200,
                node21,
            ],
            subtree_bfs_order: vec![
                root,
                node1,
                node2,
                node10,
                node20,
                node21,
                node200
            ],
            subtree_dfs_order: vec![
                root,
                node1,
                node10,
                node2,
                node20,
                node200,
                node21
            ],
        })
    }

    #[test]
    fn dfs_traversal() -> TreeResult<()> {
        let data = make_data()?;
        let dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
        assert_eq!(dfs_order, data.dfs_order);
        Ok(())
    }

    #[test]
    fn bfs_traversal() -> TreeResult<()> {
        let data = make_data()?;
        let bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
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
        let subtree_bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);
        let subtree_dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    fn remove_subtree_multiply() -> TreeResult<()> {
        let mut data = make_data()?;
        let node0 = NodeIdx(1);

        println!("initial:\n{}\n{:#?}", data.tree, data.tree);
        assert!(data.tree.garbage.is_empty());

        data.tree.remove_subtree(node0)?;
        println!("after removal 1:\n{}\n{:#?}", data.tree, data.tree);
        assert_eq!(data.tree.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        data.tree.remove_subtree(node0)?;
        println!("after removal 2:\n{}\n{:#?}", data.tree, data.tree);
        assert_eq!(data.tree.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        let subtree_bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);

        let subtree_dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
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

    #[rustfmt::skip]
    #[test]
    fn replace_subtree() -> TreeResult<()> {
        let data = make_data()?;
        let mut tree = data.tree.clone();
        println!("0 tree:\n\n{tree}\n{tree:#?}\n");

        // Replace `self[target_idx]` with `self[subroot_idx]`:
        tree.replace_subtree(
            NodeIdx(2), // target_idx
            NodeIdx(6), // subroot_idx
        )?;
        println!("1 tree:\n\n{tree}\n{tree:#?}\n");

        assert_eq!(tree[NodeIdx(0)].children, [NodeIdx(1), NodeIdx(4)]);
        assert_eq!(tree[NodeIdx(1)].children, [NodeIdx(6), NodeIdx(3)]);
        assert_eq!(tree[NodeIdx(6)].children, [NodeIdx(7), NodeIdx(9)]);
        assert_eq!(tree[NodeIdx(7)].children, [NodeIdx(8)]);
        assert_eq!(tree[NodeIdx(4)].children, [NodeIdx(5)]);
        assert_eq!(tree[NodeIdx(0)].parent, None);
        assert_eq!(tree[NodeIdx(1)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(4)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(6)].parent, Some(NodeIdx(1)));
        assert_eq!(tree[NodeIdx(3)].parent, Some(NodeIdx(1)));
        assert_eq!(tree[NodeIdx(7)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(9)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(8)].parent, Some(NodeIdx(7)));
        assert_eq!(tree[NodeIdx(5)].parent, Some(NodeIdx(4)));

        // Replace `self[target_idx]` with `self[subroot_idx]`:
        tree.replace_subtree(
            NodeIdx(4), // target_idx
            NodeIdx(6), // subroot_idx
        )?;
        println!("2 tree:\n\n{tree}\n{tree:#?}\n");

        assert_eq!(tree[NodeIdx(0)].children, [NodeIdx(1), NodeIdx(6)]);
        assert_eq!(tree[NodeIdx(1)].children, [NodeIdx(3)]);
        assert_eq!(tree[NodeIdx(6)].children, [NodeIdx(7), NodeIdx(9)]);
        assert_eq!(tree[NodeIdx(7)].children, [NodeIdx(8)]);
        assert_eq!(tree[NodeIdx(0)].parent, None);
        assert_eq!(tree[NodeIdx(1)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(6)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(3)].parent, Some(NodeIdx(1)));
        assert_eq!(tree[NodeIdx(7)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(9)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(8)].parent, Some(NodeIdx(7)));

        let _node02_idx = tree.new_node(NodeIdx(0))?;
        let _node020_idx = tree.new_node(_node02_idx)?;
        let _node0200_idx = tree.new_node(_node020_idx)?;
        println!("3 tree:\n\n{tree}\n{tree:#?}\n");

        assert_eq!(
            tree[NodeIdx(0)].children,
            [NodeIdx(1), NodeIdx(6), NodeIdx(2)]
        );
        assert_eq!(tree[NodeIdx(1)].children, [NodeIdx(3)]);
        assert_eq!(tree[NodeIdx(6)].children, [NodeIdx(7), NodeIdx(9)]);
        assert_eq!(tree[NodeIdx(7)].children, [NodeIdx(8)]);
        assert_eq!(tree[NodeIdx(2)].children, [NodeIdx(5)]);
        assert_eq!(tree[NodeIdx(5)].children, [NodeIdx(4)]);
        assert_eq!(tree[NodeIdx(0)].parent, None);
        assert_eq!(tree[NodeIdx(1)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(6)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(2)].parent, Some(NodeIdx(0)));
        assert_eq!(tree[NodeIdx(3)].parent, Some(NodeIdx(1)));
        assert_eq!(tree[NodeIdx(7)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(9)].parent, Some(NodeIdx(6)));
        assert_eq!(tree[NodeIdx(8)].parent, Some(NodeIdx(7)));
        assert_eq!(tree[NodeIdx(5)].parent, Some(NodeIdx(2)));
        assert_eq!(tree[NodeIdx(4)].parent, Some(NodeIdx(5)));

        Ok(())
    }
}
