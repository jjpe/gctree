//!
#![allow(unused)]

use crate::{
    error::{Error, Result},
    node::{NodeCount, NodeIdx},
};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
/// An arena-allocated Directed Acyclic Graph (DAG) implementation.
pub struct ArenaDag<D> {
    /// Root nodes
    roots: VecDeque<NodeIdx>,
    /// Nodes allocated within the arena
    nodes: Vec<Node<D>>,
    /// A FIFO cache for garbage nodes
    garbage: VecDeque<NodeIdx>,
}

#[allow(unused)]
impl<D> ArenaDag<D> {
    #[inline(always)]
    pub fn new() -> Self {
        Self::with_capacity(128)
    }

    #[inline(always)]
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            roots: VecDeque::with_capacity(4),
            nodes: Vec::with_capacity(node_count),
            garbage: VecDeque::with_capacity(16),
        }
    }

    /// Get the logical size, which is defined as `physical size - garbage size`
    /// i.e. the number of allocated, non-garbage nodes in `self`.
    #[inline(always)]
    pub fn logical_size(&self) -> NodeCount {
        self.physical_size() - self.garbage_size()
    }

    /// Get the physical size, which is defined as the number of nodes
    /// allocated in the tree, whether they are garbage or not.
    #[inline(always)]
    pub fn physical_size(&self) -> NodeCount {
        NodeCount::from(self.nodes.len())
    }

    /// Get the garbage size i.e. the number of garbage nodes in `self`.
    #[inline(always)]
    pub fn garbage_size(&self) -> NodeCount {
        NodeCount::from(self.garbage.len())
    }

    #[inline(always)]
    pub fn roots(&self) -> &VecDeque<NodeIdx> {
        &self.roots
    }

    #[inline(always)]
    pub fn roots_mut(&mut self) -> &mut VecDeque<NodeIdx> {
        &mut self.roots
    }

    /// Remove `root_ids` from `self.roots`.
    /// Does not remove the actual nodes themselves.
    #[inline(always)]
    pub fn remove_roots(&mut self, root_idxs: HashSet<NodeIdx>) {
        self.roots = self.roots.drain(..)
            .filter(|idx| !root_idxs.contains(&idx))
            .collect();
    }

    #[inline(always)]
    pub fn roots_iter(&self) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.roots.iter().copied()
    }

    #[inline(always)]
    pub fn add_root(&mut self, data: D) -> Result<NodeIdx> {
        self.add_node(data, [])
    }

    /// Add a node to the graph
    pub fn add_node(
        &mut self,
        data: D,
        parent_idxs: impl IntoIterator<Item = NodeIdx>,
    ) -> Result<NodeIdx> {
        let node_idx = self.alloc_node(data);
        // Fix up the parents of the allocated node:
        self[node_idx].add_parents(parent_idxs);
        // If it's a root node, register that:
        if self[node_idx].is_root_node() {
            self.roots.push_back(node_idx);
        }
        // Fix up the children of the parents of `self[node_idx]`:
        for parent_idx in self[node_idx].parents.clone() {
            self[parent_idx].add_children([node_idx]);
        }
        // The new node is assigned to one layer after its furthest parent:
        self[node_idx].layer = self[node_idx].parents.iter()
            .map(|&parent_idx| 1 + self[parent_idx].layer)
            .max()
            .unwrap_or(0);
        // The new node shouldn't have any children:
        debug_assert!(self[node_idx].children.is_empty());
        Ok(node_idx)
    }

    fn alloc_node(&mut self, data: D) -> NodeIdx {
        self.garbage.pop_front().unwrap_or_else(|| {
            let node_idx = NodeIdx::from(self.nodes.len());
            self.nodes.push(Node::new(node_idx, data));
            node_idx
        })
    }

    /// Remove the sub-DAG rooted in `start_idx`.
    pub fn remove_subdag(&mut self, start_idx: NodeIdx) -> Result<()> {
        for desc_idx in self.dfs(start_idx).rev() {
            self.recycle_node(desc_idx)?;
        }
        Ok(())
    }

    fn recycle_node(&mut self, node_idx: NodeIdx) -> Result<()> {
        self.ensure_node_is_leaf(node_idx)?;
        // Filter out the `node_idx` from each parents' children:
        for parent_idx in self[node_idx].parents.clone() {
            self[parent_idx].remove_children([node_idx]);
        }
        self[node_idx].clear();
        self.garbage.push_back(node_idx);
        Ok(())
    }

    #[inline]
    pub fn ensure_node_is_branch(&self, node_idx: NodeIdx) -> Result<()> {
        if self[node_idx].is_branch_node() {
            Ok(())
        } else {
            Err(Error::ExpectedBranchNode(node_idx))
        }
    }

    #[inline]
    pub fn ensure_node_is_leaf(&self, node_idx: NodeIdx) -> Result<()> {
        if self[node_idx].is_leaf_node() {
            Ok(())
        } else {
            Err(Error::ExpectedLeafNode(node_idx))
        }
    }

    #[inline]
    pub fn ensure_node_is_root(&self, node_idx: NodeIdx) -> Result<()> {
        if self[node_idx].is_root_node() {
            Ok(())
        } else {
            Err(Error::ExpectedRootNode(node_idx))
        }
    }

    // /// Ensure that `self` contains no cycles
    // // TODO: Make this method incremental. Currently the method traverses
    // //       the entire graph, which can quickly become expensive.
    // pub fn ensure_no_cycles(&self) -> Result<()> {
    //     macro_rules! set {
    //         ($($elt:expr),* $(,)?) => {{
    //             let mut set = std::collections::HashSet::new();
    //             $(
    //                 set.insert($elt);
    //             )*;
    //             set
    //         }}
    //     }

    //     let mut path: Vec<NodeIdx> = vec![
    //         // TODO
    //     ];

    //     let mut reached = <Vec<HashSet<_>>>::with_capacity(self.roots().len());
    //     for (i, &root_idx) in self.roots().iter().enumerate() {
    //         reached[i] = set!{root_idx};
    //         for node_idx in self.bfs(root_idx).filter(|&idx| idx != root_idx) {
    //             // if self[node_idx].parents.contains()

    //             if self.roots().contains(&node_idx) {
    //                 return Err(Error::CycleDetected { path });
    //             }

    //         }
    //     }

    //     // todo!("ArenaDag::ensure_no_cycles()") // TODO
    //     Ok(())
    // }

    /// Return an iterator over the nodes, in DFS order,
    /// of each of the sub-GSS's rooted in `start_idx`.
    pub fn dfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        let mut output = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![start_idx];
        while let Some(node_idx) = stack.pop() {
            output.push(node_idx);
            stack.extend(self[node_idx].children.iter().rev());
        }
        output.into_iter()
    }

    /// Return an iterator, in BFS order, over the `NodeIdx`s
    /// of the nodes of all the sub-GSS's rooted in `start_idx`.
    pub fn bfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        type Layer = Vec<NodeIdx>;
        let mut layers: Vec<Layer> = vec![Layer::from([start_idx])];
        let mut current: Layer = vec![];
        while let Some(previous_layer) = layers.last() {
            for &idx in previous_layer {
                current.extend(&self[idx].children);
            }
            if current.is_empty() {
                break;
            }
            layers.push(current);
            current = vec![];
        }
        layers.into_iter().flat_map(|layer| layer.into_iter())
    }

    ///
    pub fn descendants_of(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.dfs(start_idx).filter(move |&node_idx| node_idx != start_idx)
    }

    /// Return `true` iff. some path `self[src_idx] -> ... -> self[dst_idx]`
    /// exists i.e. iff. `dst` is reachable from `src`.
    #[inline]
    pub fn is_reachable(&self, src_idx: NodeIdx, dst_idx: NodeIdx) -> bool {
        self.bfs(src_idx).any(|idx| idx == dst_idx)
    }

    /// Make the nodes identified by `root_idxs` (i.e. the root nodes of
    /// sub-DAGs allocated in `self`) the children of a newly added parent node
    /// carrying `data`, which becomes a new root node instead of its children.
    /// The children are added to the parent in iteration order.
    /// Return the parent's `NodeIdx`.
    pub fn merge_subdags<I>(
        &mut self,
        data: D,
        root_idxs: I,
    ) -> Result<NodeIdx>
    where
        I: IntoIterator<Item = NodeIdx>,
        I::IntoIter: Clone
    {
        let child_idxs = root_idxs.into_iter();
        // Verify "rootness" *BEFORE* allocating a new parent root node:
        for child_idx in child_idxs.clone() {
            self.ensure_node_is_root(child_idx)?;
        }
        let parent_idx = self.add_root(data)?;
        for child_idx in child_idxs.clone() { // Update parent-child links
            self[parent_idx].add_children([child_idx]);
            self[child_idx].add_parents([parent_idx]);
        }
        self.remove_roots(child_idxs.collect());
        // Increment the layer of all descendants of the new root:
        for desc_idx in self.dfs(parent_idx).filter(|&idx| idx != parent_idx) {
            self[desc_idx].layer += 1;
        }
        Ok(parent_idx)
    }

    /// Ensure that the nodes corresponding to the
    /// passed `node_idxs` are root nodes.
    fn ensure_roots(
        &mut self,
        node_idxs: impl IntoIterator<Item = NodeIdx>
    ) -> Result<()> {
        for node_idx in node_idxs {
            if !self[node_idx].is_root_node() {
                return Err(Error::ExpectedRootNode(node_idx));
            }
        }
        Ok(())
    }
}

impl<D> std::ops::Index<NodeIdx> for ArenaDag<D> {
    type Output = Node<D>;

    fn index(&self, idx: NodeIdx) -> &Self::Output {
        &self.nodes[idx.0]
    }
}

impl<D> std::ops::IndexMut<NodeIdx> for ArenaDag<D> {
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Self::Output {
        &mut self.nodes[idx.0]
    }
}

#[rustfmt::skip]
#[derive(
    Clone,
    Debug, // TODO: use something like GraphViz to visualize instances?
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::Deref,
    derive_more::DerefMut,
)]
pub struct Node<D> {
    pub idx: NodeIdx,
    pub layer: usize,
    pub parents: Vec<NodeIdx>,
    pub children: Vec<NodeIdx>,
    #[deref]
    #[deref_mut]
    pub data: D,
    _prevent_direct_instantiation_: (),
}

impl<D> Node<D> {
    #[inline(always)]
    fn new(node_idx: NodeIdx, data: D) -> Self {
        Self {
            idx: node_idx,
            layer: 0,
            parents: Vec::with_capacity(2),
            children: Vec::with_capacity(4),
            data,
            _prevent_direct_instantiation_: (),
        }
    }

    #[inline(always)]
    fn clear(&mut self) {
        self.layer = 0;
        self.parents.clear();
        self.children.clear();
        // Don't change `self.data` - changing it to some dummy value would
        // require a `where D: Default` bound on this method, which would
        // mean that GC could only take place for values of such types `D`.
    }

    #[rustfmt::skip]
    #[inline(always)]
    pub fn is_root_node(&self) -> bool { self.parents.is_empty() }

    #[rustfmt::skip]
    #[inline(always)]
    pub fn is_leaf_node(&self) -> bool { self.children.is_empty() }

    #[rustfmt::skip]
    #[inline(always)]
    pub fn is_branch_node(&self) -> bool { !self.is_leaf_node() }

    #[inline(always)]
    pub fn add_parents(
        &mut self,
        parent_idxs: impl IntoIterator<Item = NodeIdx>
    ) {
        for parent_idx in parent_idxs {
            if !self.parents.contains(&parent_idx) {
                self.parents.push(parent_idx);
            }
        }
    }

    /// Remove from `self.parents` any index in `parent_idxs`.
    /// If `self.parents` doesn't contain any index in
    /// `parent_idx`, this fn Has no effect.
    #[inline]
    #[rustfmt::skip]
    pub fn remove_parents(
        &mut self,
        parent_idxs: impl IntoIterator<Item = NodeIdx>
    ) {
        for parent_idx in parent_idxs {
            if self.parents.contains(&parent_idx) {
                self.parents = self.parents.drain(..)
                    .filter(|&pidx| pidx != parent_idx)
                    .collect();
            }
        }
    }

    #[inline(always)]
    pub fn add_children(
        &mut self,
        child_idxs: impl IntoIterator<Item = NodeIdx>
    ) {
        for child_idx in child_idxs {
            if !self.children.contains(&child_idx) {
                self.children.push(child_idx);
            }
        }
    }

    /// Remove from `self.children` any index in `child_idxs`.
    /// If `self.children` doesn't contain any index in
    /// `child_idx`, this fn Has no effect.
    #[inline]
    #[rustfmt::skip]
    pub fn remove_children(
        &mut self,
        child_idxs: impl IntoIterator<Item = NodeIdx>
    ) {
        for child_idx in child_idxs {
            if self.children.contains(&child_idx) {
                self.children = self.children.drain(..)
                    .filter(|&pidx| pidx != child_idx)
                    .collect();
            }
        }
    }

    /// Assuming that `parent_idx` is a member of `self.parents`, return the
    /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    /// for `parent_idx` e.g. the leftmost parent is the `zeroth` parent of
    /// `self`.
    /// Return `None` if `parent_idx` is not a member of `self.parents`.
    #[inline]
    #[rustfmt::skip]
    pub fn parent_ordinal(&self, parent_idx: NodeIdx) -> Option<usize> {
        self.parents.iter().enumerate()
            .filter(|(_, &cidx)| cidx == parent_idx)
            .map(|(ordinal_numeral, _)| ordinal_numeral)
            .next()
    }

    /// Assuming that `child_idx` is a member of `self.children`, return the
    /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    /// for `child_idx` e.g. the leftmost child is the `zeroth` child of
    /// `self`.
    /// Return `None` if `child_idx` is not a member of `self.children`.
    #[inline]
    #[rustfmt::skip]
    pub fn child_ordinal(&self, child_idx: NodeIdx) -> Option<usize> {
        self.children.iter().enumerate()
            .filter(|(_, &cidx)| cidx == child_idx)
            .map(|(ordinal_numeral, _)| ordinal_numeral)
            .next()
    }
}

// impl<D: Debug> fmt::Debug for Node<D> {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         let mut ds = f.debug_struct("Node");
//         let ds = ds.field("idx", &self.idx);
//         let ds = if let Some(parent) = self.parent {
//             ds.field("parent", &parent)
//         } else {
//             ds.field("parent", &format_args!("None"))
//         };
//         let ds = ds.field("children", &self.children);
//         let ds = ds.field("data", &self.data);
//         ds.finish()
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_subdags() -> Result<()> {
        let mut dag = ArenaDag::<&str>::new();
        // subtree 0:
        let  root0_idx = dag.add_node("Old Root", [/*no parents*/])?;
        let child1_idx = dag.add_node("Child 1", [root0_idx])?;
        let child2_idx = dag.add_node("Child 2", [root0_idx])?;
        // subtree 1:
        let  root3_idx = dag.add_node("Old Root", [/*no parents*/])?;
        let child4_idx = dag.add_node("Child 4", [root3_idx])?;
        let child5_idx = dag.add_node("Child 5", [root3_idx])?;
        let child6_idx = dag.add_node("Child 6", [root3_idx])?;
        dag.merge_subdags("New Root", [root0_idx, root3_idx])?;

        let mut expected = ArenaDag::<&str>::new();
        let     root_idx = expected.add_node("New Root", [/*no parents*/])?;
        // subtree 0:
        let   child0_idx = expected.add_node("Old Root", [root_idx])?;
        let gchild01_idx = expected.add_node("Child 1", [child0_idx])?;
        let gchild02_idx = expected.add_node("Child 2", [child0_idx])?;
        // subtree 1:
        let   child1_idx = expected.add_node("Old Root", [root_idx])?;
        let gchild10_idx = expected.add_node("Child 4", [child1_idx])?;
        let gchild11_idx = expected.add_node("Child 5", [child1_idx])?;
        let gchild12_idx = expected.add_node("Child 6", [child1_idx])?;

        /// Return true iff. `l` and `r` are structurally equivalent
        /// in terms of the data of their nodes.
        /// In particular this means that the `NodeIdx`s in each `ArenaDag<D>`
        /// are not compared against one another, only the data of the nodes
        /// that those `NodeIdx`s point to.
        fn eq<D>(l: &ArenaDag<D>, r: &ArenaDag<D>) -> bool
        where
            D: Eq
        {
            if l.roots.len() != r.roots.len() {
                return false;
            }
            if l.logical_size() != r.logical_size() {
                return false;
            }
            for (lroot, rroot) in l.roots_iter().zip(r.roots_iter()) {
                for (lidx, ridx) in l.dfs(lroot).zip(r.dfs(rroot)) {
                    if l[lidx].data != r[ridx].data {
                        return false;
                    }
                }
            }
            true
        }

        assert!(eq(&dag, &expected), "{dag:#?}\n!=\n{expected:#?}");



        Ok(())
    }
}
