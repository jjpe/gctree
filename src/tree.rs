//!

use crate::arena::Arena;
pub use crate::{
    error::{Error, Result},
    node::{Node, NodeCount, NodeIdx},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Debug};

#[derive(Clone, Debug, Hash)]
pub struct Tree<D> {
    arena: Arena<D>
}

impl<D> Default for Tree<D> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl<D> Tree<D> {
    pub const ROOT_IDX: NodeIdx = NodeIdx(0);

    pub fn with_capacity(cap: usize) -> Self {
        Self { arena: Arena::with_capacity(cap) }
    }

    #[inline]
    /// Get the logical size, which is defined as `physical size - garbage size`
    /// i.e. the number of allocated, non-garbage nodes in `self`.
    pub fn logical_size(&self) -> NodeCount {
        self.arena.logical_size()
    }

    #[inline]
    /// Get the physical size, which is defined as the number of nodes
    /// allocated in the tree, whether they are garbage or not.
    pub fn physical_size(&self) -> NodeCount {
        self.arena.physical_size()
    }

    /// Get the garbage size i.e. the number of garbage nodes in `self`.
    #[inline]
    pub fn garbage_size(&self) -> NodeCount {
        self.arena.garbage_size()
    }

    #[inline(always)]
    pub fn root_ref(&self) -> &Node<D> {
        &self[Self::ROOT_IDX]
    }

    #[inline(always)]
    pub fn root_mut(&mut self) -> &mut Node<D> {
        &mut self[Self::ROOT_IDX]
    }

    /// If there is a garbage `Node<D>`in `self`, recycle it.
    /// Otherwise, allocate a new one.
    pub fn add_node(
        &mut self,
        parent_idx: impl Into<Option<NodeIdx>>,
        data: D
    ) -> Result<NodeIdx> {
        let node_idx = self.arena.add_node(data);
        if let Some(parent_idx) = parent_idx.into() {
            self.arena.add_edge(parent_idx, node_idx);
        }
        Ok(node_idx)
    }

    #[inline]
    /// Recycle `self[node_idx]`.  Since a node conceptually owns
    /// its children, all descendant nodes and all edges between
    /// them are removed as well.
    pub fn rm_node(&mut self, node_idx: NodeIdx) -> Result<()> {
        self.arena.rm_node(node_idx)
    }

    /// Copy a `src` tree to `self`, making it a subtree of `self` in
    /// the process.  Specifically, `src[ROOT]` becomes a child node
    /// of `self[dst_node_idx]`.
    /// Note that the `NodeIdx`s of the nodes in `src` will *NOT* be
    /// valid in `self`.
    pub fn copy_subtree(
        &mut self,
        dst_node_idx: NodeIdx,
        src: &Self,
    ) -> Result<()>
    where
        D: Clone
    {
        type SrcTreeIdx = Option<NodeIdx>;
        type DstTreeIdx = NodeIdx;
        let mut map = HashMap::<SrcTreeIdx, DstTreeIdx>::new();
        map.insert(None, dst_node_idx);
        let (src, dst) = (src, self);
        for src_node_idx in src.dfs(Self::ROOT_IDX) {
            let src_parent_idx = src.parent_of(src_node_idx);
            let dst_parent_idx = map.get(&src_parent_idx).copied();
            let data = src[src_node_idx].data.clone();
            let dst_node_idx = dst.add_node(dst_parent_idx, data)?;
            map.insert(Some(src_node_idx), dst_node_idx);
        }
        Ok(())
    }

    #[inline]
    pub fn rm_subtree(&mut self, root_idx: NodeIdx) -> Result<()> {
        self.rm_node(root_idx)
    }

    /// Remove all descendant nodes of `self[node_idx]`, but
    /// not `self[node_idx]` itself.
    pub fn rm_descendants_of(&mut self, node_idx: NodeIdx) -> Result<()> {
        for child_idx in self[node_idx].children.clone() {
            self.rm_subtree(child_idx)?;
        }
        Ok(())
    }

    #[rustfmt::skip]
    /// Make `self[subroot_idx]` the last child node of `self[parent_idx]`.
    pub fn move_subtree(
        &mut self,
        subroot_idx: NodeIdx,
        parent_idx: NodeIdx,
    ) -> Result<()> {
        if let Some(old_parent_idx) = self.parent_of(subroot_idx) {
            self.arena.rm_edge(old_parent_idx, subroot_idx)?;
        };
        self.arena.add_edge(parent_idx, subroot_idx);
        Ok(())
    }

    /// Replace the subtree rooted @ `self[target_idx]` with the subtree
    /// rooted @ `self[subroot_idx]`.  This means that `self[target_idx]`
    /// is removed from `self` and `self[subroot_idx]` takes its place.
    pub fn replace_subtree(
        &mut self,
        target_idx: NodeIdx,
        subroot_idx: NodeIdx,
    ) -> Result<()> {
        if let Some(parent_idx) = self.parent_of(subroot_idx) {
            self.arena.rm_edge(parent_idx, subroot_idx)?;
        }
        if let Some(parent_idx) = self.parent_of(target_idx) {
            let pos = self[parent_idx].children.iter()
                .position(|&cidx| cidx == target_idx);
            self.arena.rm_edge(parent_idx, target_idx)?;
            self.arena.insert_edge((parent_idx, None), (subroot_idx, pos));
        }
        self.rm_subtree(target_idx)?;
        Ok(())
    }

    #[inline(always)]
    pub fn self_or_ancestors_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.self_or_ancestors_of(node_idx)
    }

    #[inline(always)]
    pub fn ancestors_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.ancestors_of(node_idx)
    }

    #[inline]
    pub fn parent_of(&self, node_idx: NodeIdx) -> Option<NodeIdx> {
        self[node_idx].parents.get(0).copied()
    }

    #[inline(always)]
    pub fn self_or_siblings_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.self_or_siblings_of(node_idx)
    }

    #[inline(always)]
    pub fn siblings_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.siblings_of(node_idx)
    }

    #[inline(always)]
    pub fn children_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.children_of(node_idx)
    }

    #[inline(always)]
    pub fn self_or_descendants_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.self_or_descendants_of(node_idx)
    }

    #[inline(always)]
    pub fn descendants_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.arena.descendants_of(node_idx)
    }

    #[inline(always)]
    pub fn dfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.arena.dfs(start_idx)
    }

    #[inline(always)]
    pub fn bfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.arena.bfs(start_idx)
    }
}

impl<D> std::ops::Index<NodeIdx> for Tree<D> {
    type Output = Node<D>;

    fn index(&self, idx: NodeIdx) -> &Self::Output {
        &self.arena[idx]
    }
}

impl<D> std::ops::IndexMut<NodeIdx> for Tree<D> {
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Self::Output {
        &mut self.arena[idx]
    }
}


impl<D> PartialEq<Self> for Tree<D>
where
    D: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        if self.logical_size() != other.logical_size() {
            return false;
        }
        let mut map = HashMap::new();
        for (sidx, oidx) in self.dfs(Self::ROOT_IDX).zip(other.dfs(Self::ROOT_IDX)) {
            map.insert(sidx, oidx);
            let (snode, onode) = (&self[sidx], &other[oidx]);
            match (&*snode.parents, &*onode.parents) {
                (&[], &[]) => { /*NOP*/ }
                (&[spidx], &[opidx]) if map[&spidx] == opidx => {
                    // NOP
                }
                _ => return false,
            }
            if snode.count_children() != onode.count_children() {
                return false;
            }
            if snode.data != onode.data {
                return false;
            }
        }
        true
    }
}

#[rustfmt::skip]
impl<D> Eq for Tree<D> where D: Eq {}

#[rustfmt::skip]
impl<D> PartialOrd<Self> for Tree<D>
where
    D: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().partial_cmp(&other.logical_size());
        if let Some(Ordering::Greater | Ordering::Less) = size_cmp {
            return size_cmp;
        }
        let snode_iter = self.dfs(Self::ROOT_IDX);
        let onode_iter = other.dfs(Self::ROOT_IDX);
        let mut map = HashMap::new();
        for (snode_idx, onode_idx) in snode_iter.zip(onode_iter) {
            map.insert(snode_idx, onode_idx);
            let (snode, onode) = (&self[snode_idx], &other[onode_idx]);
            let (sdata, odata) = (&**snode, &**onode);
            match (&*snode.parents, &*onode.parents) {
                (&[], &[]) => { /*NOP*/ }
                (&[spidx], &[opidx]) if map[&spidx] == opidx => {/*NOP*/}
                _ => return snode.parents.partial_cmp(&onode.parents),
            }
            let child_count_cmp = snode.count_children()
                .partial_cmp(&onode.count_children());
            if let Some(Ordering::Greater | Ordering::Less) = child_count_cmp {
                return child_count_cmp;
            }
            let data_cmp = sdata.partial_cmp(odata);
            if let Some(Ordering::Greater | Ordering::Less) = data_cmp {
                return data_cmp;
            }
        }
        Some(Ordering::Equal)
    }
}

#[rustfmt::skip]
impl<D> Ord for Tree<D>
where
    D: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().cmp(&other.logical_size());
        if let Ordering::Greater | Ordering::Less = size_cmp {
            return size_cmp;
        }
        let snode_iter =  self.dfs(Self::ROOT_IDX);
        let onode_iter = other.dfs(Self::ROOT_IDX);
        let mut map = HashMap::new();
        for (snode_idx, onode_idx) in snode_iter.zip(onode_iter) {
            map.insert(snode_idx, onode_idx);
            let (snode, onode) = (&self[snode_idx], &other[onode_idx]);
            let (sdata, odata) = (&**snode, &**onode);
            match (&*snode.parents, &*onode.parents) {
                (&[], &[]) => {/*NOP*/},
                (&[spidx], &[opidx]) if map[&spidx] == opidx => {/*NOP*/},
                _ => return snode.parents.cmp(&onode.parents),
            }
            let child_count_cmp = snode.count_children()
                .cmp(&onode.count_children());
            if let Ordering::Greater | Ordering::Less = child_count_cmp {
                return child_count_cmp;
            }
            let data_cmp = sdata.cmp(odata);
            if let Ordering::Greater | Ordering::Less = data_cmp {
                return data_cmp;
            }
        }
        Ordering::Equal
    }
}

impl<D> fmt::Display for Tree<D>
where
    D: std::fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // NOTE: This loop is `O(D * N)`, where:
        //       - D is the maximum depth of `self`
        //       - N is the number of nodes in `self`
        for node_idx in self.dfs(Self::ROOT_IDX) {
            for _ in self.ancestors_of(node_idx) {
                write!(f, "| ")?; // no newline
            }
            let Node { idx, data, .. } = &self[node_idx];
            writeln!(f, "{idx} {data}")?;
        }
        Ok(())
    }
}

// Manual impl to serialize a `Tree<D>` with relaxed requirements on `D`
#[rustfmt::skip]
impl<D: Serialize> Serialize for Tree<D>
where
    D: Serialize,
{
    fn serialize<S: Serializer>(
        &self,
        serializer: S
    ) -> std::result::Result<S::Ok, S::Error> {
        const NUM_FIELDS: usize = 1;
        let mut state = serializer.serialize_struct("Tree", NUM_FIELDS)?;
        state.serialize_field("arena", &self.arena)?;
        state.end()
    }
}

// Manual impl to deserialize a `Tree<D>` with relaxed requirements on `D`
#[rustfmt::skip]
impl<'de, D> Deserialize<'de> for Tree<D>
where
D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
    // D: Deserialize<'de>, // TODO
{
    fn deserialize<DE: Deserializer<'de>>(
        d: DE
    ) -> std::result::Result<Self, DE::Error> {
        #[derive(serde_derive::Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Arena
        }

        struct TreeVisitor<D>(std::marker::PhantomData<D>);

        impl<'de, D> Visitor<'de> for TreeVisitor<D>
        where
            D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
        {
            type Value = Tree<D>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct Tree<D>")
            }

            fn visit_seq<V>(
                self,
                mut seq: V
            ) -> std::result::Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let arena = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                Ok(Tree { arena })
            }

            fn visit_map<A>(
                self,
                mut map: A
            ) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut arena = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Arena if arena.is_some() => {
                            return Err(de::Error::duplicate_field("nodes"));
                        }
                        Field::Arena => { arena = Some(map.next_value()?); }

                    }
                }
                Ok(Tree {
                    arena: arena.ok_or_else(|| de::Error::missing_field("arena"))?,
                })
            }
        }

        d.deserialize_map(TreeVisitor(std::marker::PhantomData))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Data {
        tree: Tree<&'static str>,
        root: NodeIdx,
        bfs_order: Vec<NodeIdx>,
        dfs_order: Vec<NodeIdx>,
        subtree_bfs_order: Vec<NodeIdx>,
        subtree_dfs_order: Vec<NodeIdx>,
    }

    #[rustfmt::skip]
    fn make_data() -> Result<Data> {
        let mut tree: Tree<&str> = Tree::default();
        let root: NodeIdx = tree.add_node(None, "")?;
        let node0: NodeIdx = tree.add_node(root, "")?;
        let node00: NodeIdx = tree.add_node(node0, "")?;
        let node01: NodeIdx = tree.add_node(node0, "")?;
        let node1: NodeIdx = tree.add_node(root, "")?;
        let node10: NodeIdx = tree.add_node(node1, "")?;
        let node2: NodeIdx = tree.add_node(root, "")?;
        let node20: NodeIdx = tree.add_node(node2, "")?;
        let node200: NodeIdx = tree.add_node(node20, "")?;
        let node21: NodeIdx = tree.add_node(node2, "")?;
        Ok(Data {
            tree,
            root,
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
    fn dfs_traversal() -> Result<()> {
        let data = make_data()?;
        let dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
        assert_eq!(dfs_order, data.dfs_order);
        Ok(())
    }

    #[test]
    fn bfs_traversal() -> Result<()> {
        let data = make_data()?;
        let bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
        assert_eq!(bfs_order, data.bfs_order);
        Ok(())
    }

    #[test]
    #[allow(unused)]
    fn copy_subtree() -> Result<()> {
        let mut tree: Tree<&str> = Tree::default();
        let root: NodeIdx = tree.add_node(None, "root")?;
        let node0: NodeIdx = tree.add_node(root, "node0")?;
        let node1: NodeIdx = tree.add_node(root, "node1")?;
        let node10: NodeIdx = tree.add_node(node1, "node10")?;
        let node100: NodeIdx = tree.add_node(node10, "node100")?;
        let node2: NodeIdx = tree.add_node(root, "node2")?;
        let node20: NodeIdx = tree.add_node(node2, "node20")?;

        let mut subtree: Tree<&str> = Tree::default();
        let st_root: NodeIdx = subtree.add_node(None, "root (subtree)")?;
        let st_node0: NodeIdx = subtree.add_node(st_root, "node0 (subtree)")?;
        let st_node1: NodeIdx = subtree.add_node(st_root, "node1 (subtree)")?;
        let st_node10: NodeIdx = subtree.add_node(st_node1, "node10 (subtree)")?;
        let st_node100: NodeIdx = subtree.add_node(st_node10, "node100 (subtree)")?;
        let st_node2: NodeIdx = subtree.add_node(st_root, "node2 (subtree)")?;
        let st_node20: NodeIdx = subtree.add_node(st_node2, "node20 (subtree)")?;

        tree.copy_subtree(node20, &subtree)?;

        let mut expected: Tree<&str> = Tree::default();
        let e_root = expected.add_node(None, "root")?; //  0
        let e_node0 = expected.add_node(e_root, "node0")?; //  1
        let e_node1 = expected.add_node(e_root, "node1")?; //  2
        let e_node10 = expected.add_node(e_node1, "node10")?; //  3
        let e_node100 = expected.add_node(e_node10, "node100")?; //  4
        let e_node2 = expected.add_node(e_root, "node2")?; //  5
        let e_node20 = expected.add_node(e_node2, "node20")?; //  6
        let e_st_root = expected.add_node(e_node20, "root (subtree)")?; //  7
        let e_st_node0 = expected.add_node(e_st_root, "node0 (subtree)")?; //  8
        let e_st_node1 = expected.add_node(e_st_root, "node1 (subtree)")?; //  9
        let e_st_node10 = expected.add_node(e_st_node1, "node10 (subtree)")?; // 10
        let e_st_node100 = expected.add_node(e_st_node10, "node100 (subtree)")?; // 11
        let e_st_node2 = expected.add_node(e_st_root, "node2 (subtree)")?; // 12
        let e_st_node20 = expected.add_node(e_st_node2, "node20 (subtree)")?; // 13

        assert_eq!(tree, expected, "\n{} !=\n{}", tree, expected);
        Ok(())
    }

    #[test]
    #[allow(unused)]
    fn remove_subtree() -> Result<()> {
        let mut data = make_data()?;
        let node0 = NodeIdx(1);
        data.tree.rm_subtree(node0)?;
        let subtree_bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);
        let subtree_dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    fn remove_subtree_multiply() -> Result<()> {
        let mut data = make_data()?;
        let node0 = NodeIdx(1);

        println!("initial:\n{}\n{:#?}", data.tree, data.tree);
        // assert!(data.tree.garbage.is_empty());

        data.tree.rm_subtree(node0)?;
        println!("after removal 1:\n{}\n{:#?}", data.tree, data.tree);
        // assert_eq!(data.tree.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        data.tree.rm_subtree(node0)?;
        println!("after removal 2:\n{}\n{:#?}", data.tree, data.tree);
        // assert_eq!(data.tree.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        let subtree_bfs_order: Vec<_> = data.tree.bfs(data.root).collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);

        let subtree_dfs_order: Vec<_> = data.tree.dfs(data.root).collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn rm_descendants_of() -> Result<()> {
        let mut data = make_data()?;
        println!("tree 0:\n{}", data.tree);
        data.tree.rm_descendants_of(NodeIdx(6))?;
        println!("tree 1:\n{}", data.tree);
        assert!(data.tree[NodeIdx(6)].children.is_empty());
        let expected = &[NodeIdx(1), NodeIdx(4), NodeIdx(6)];
        assert_eq!(&data.tree.root_ref().children, expected);
        Ok(())
    }

    #[test]
    fn ancestors_of() -> Result<()> {
        let data = make_data()?;
        println!("tree:\n{}", data.tree);
        let node_idx = NodeIdx(8);
        let ancestors: Vec<_> = data.tree.ancestors_of(node_idx).collect();
        println!("ancestor indices:\n{:?}", ancestors);
        assert_eq!(ancestors, &[NodeIdx(7), NodeIdx(6), NodeIdx(0)]);
        Ok(())
    }

    #[test]
    fn siblings_of() -> Result<()> {
        let data = make_data()?;
        println!("tree:\n{}", data.tree);
        let node_idx = NodeIdx(4);
        let siblings: Vec<_> = data.tree.siblings_of(node_idx).collect();
        println!("sibling indices:\n{:?}", siblings);
        assert_eq!(siblings, &[NodeIdx(1), NodeIdx(6)]);
        Ok(())
    }

    #[test]
    fn children_of() -> Result<()> {
        let data = make_data()?;
        println!("tree:\n{}", data.tree);
        let node_idx = NodeIdx(1);
        let children: Vec<_> = data.tree.children_of(node_idx).collect();
        println!("child indices:\n{:?}", children);
        assert_eq!(children, &[NodeIdx(2), NodeIdx(3)]);
        Ok(())
    }

    #[test]
    fn descendants_of() -> Result<()> {
        let data = make_data()?;
        println!("tree:\n{}", data.tree);
        let node_idx = NodeIdx(6);
        let descendants: Vec<_> = data.tree.descendants_of(node_idx).collect();
        println!("descendant indices:\n{:?}", descendants);
        assert_eq!(descendants, &[NodeIdx(7), NodeIdx(8), NodeIdx(9)]);
        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn move_subtree() -> Result<()> {
        let data = make_data()?;
        let mut tree = data.tree.clone();

        let (subroot_idx, parent_idx) = (NodeIdx(6), NodeIdx(1));
        // Make `tree[subroot_idx]` a child of `tree[parent_idx]`,
        // i.e. move the subtree rooted in `tree[subroot_idx]`:
        tree.move_subtree(subroot_idx, parent_idx)?;
        let mut expected = Tree::<&str>::default();
        // The order in which the nodes are added is significant:
        let root_idx = expected.add_node(None, "")?;
        let _node1_idx = expected.add_node(root_idx, "")?;
        let _node2_idx = expected.add_node(_node1_idx, "")?;
        let _node3_idx = expected.add_node(_node1_idx, "")?;
        let _node4_idx = expected.add_node(root_idx, "")?;
        let _node5_idx = expected.add_node(_node4_idx, "")?;
        let _node6_idx = expected.add_node(_node1_idx, "")?;
        let _node7_idx = expected.add_node(_node6_idx, "")?;
        let _node8_idx = expected.add_node(_node7_idx, "")?;
        let _node9_idx = expected.add_node(_node6_idx, "")?;
        assert_eq!(
            tree, expected,
            "\ntree:\n{tree}\n    !=\n    expected:\n{expected}"
        );

        let (subroot_idx, parent_idx) = (NodeIdx(6), NodeIdx(0));
        // Make `tree[subroot_idx]` a child of its grandparent,
        // i.e. move the subtree back:
        tree.move_subtree(subroot_idx, parent_idx)?;
        let expected = data.tree;
        assert_eq!(
            tree, expected,
            "\ntree:\n{tree}\n    !=\n    expected:\n{expected}"
        );

        Ok(())
    }

    #[rustfmt::skip]
    #[test]
    fn replace_subtree() -> Result<()> {
        let data = make_data()?;
        let mut tree = data.tree;
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
        assert_eq!(tree.parent_of(NodeIdx(0)), None);
        assert_eq!(tree.parent_of(NodeIdx(1)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(4)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(6)), Some(NodeIdx(1)));
        assert_eq!(tree.parent_of(NodeIdx(3)), Some(NodeIdx(1)));
        assert_eq!(tree.parent_of(NodeIdx(7)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(9)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(8)), Some(NodeIdx(7)));
        assert_eq!(tree.parent_of(NodeIdx(5)), Some(NodeIdx(4)));

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
        assert_eq!(tree.parent_of(NodeIdx(0)), None);
        assert_eq!(tree.parent_of(NodeIdx(1)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(6)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(3)), Some(NodeIdx(1)));
        assert_eq!(tree.parent_of(NodeIdx(7)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(9)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(8)), Some(NodeIdx(7)));

        let _node02_idx = tree.add_node(NodeIdx(0), "")?;
        let _node020_idx = tree.add_node(_node02_idx, "")?;
        let _node0200_idx = tree.add_node(_node020_idx, "")?;
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
        assert_eq!(tree.parent_of(NodeIdx(0)), None);
        assert_eq!(tree.parent_of(NodeIdx(1)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(6)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(2)), Some(NodeIdx(0)));
        assert_eq!(tree.parent_of(NodeIdx(3)), Some(NodeIdx(1)));
        assert_eq!(tree.parent_of(NodeIdx(7)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(9)), Some(NodeIdx(6)));
        assert_eq!(tree.parent_of(NodeIdx(8)), Some(NodeIdx(7)));
        assert_eq!(tree.parent_of(NodeIdx(5)), Some(NodeIdx(2)));
        assert_eq!(tree.parent_of(NodeIdx(4)), Some(NodeIdx(5)));

        Ok(())
    }
}
