//!

use crate::arena::{Arena, TraversalType};
#[cfg(feature = "graphviz")] use crate::graphviz::*;
pub use crate::{
    error::{Error, Result},
    node::{Edge, Node, NodeCount, NodeIdx},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Debug};


#[macro_export]
/// Declaratively construct `Forest<D>` instances,
/// where the `$data` arguments all have type `D`
/// and no data is being stored in any edges.
macro_rules! forest {
    (
        $(
            ($data:expr $(, $($children:tt),+)?)
        ),*
        $(,)?
    ) => {{ #[allow(redundant_semicolons, unused)] {
        let mut forest = $crate::Forest::default();
        $(
            let root_idx = forest.push_root($data);
            $(
                $(
                    place_forest! { [in forest] root_idx; $children }
                )+
            )? ;
        )* ;
        forest
    }}};
}

pub use forest;

#[doc(hidden)]
#[macro_export]
// A "placement in" variant of the `forest!{}` macro.
// This internal version accommodates 3 things:
//   1. No `Forest` instance is created. Instead, one is passed
//      in as an additional macro argument named `$forest`.
//   2. No `Forest` instance is returned.
//   3. A `$parent_idx` is passed as a macro argument.
macro_rules! place_forest {
    (
        [in $arena:expr]
        $parent_idx:expr;
        ($data:expr $(, $($children:tt),+)?)
    ) => {{ #[allow(redundant_semicolons, unused)] {
        let forest: &mut $crate::Forest<_, (), ()> = &mut $arena;
        let node_idx: $crate::ForestIdx = forest.add_node($data);
        forest.add_edge(($parent_idx, node_idx), (), ());
        $(
            $(
                place_forest! { [in $arena] node_idx; $children }
            )+
        )? ;
    }}};
}



#[derive(Clone, Debug, Hash)]
pub struct Forest<D, P, C> {
    arena: Arena<D, P, C>,
    roots: Vec<ForestIdx>,
}

impl<D, P, C> Default for Forest<D, P, C> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl<D, P, C> Forest<D, P, C> {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            arena: Arena::with_capacity(cap),
            roots: vec![],
        }
    }

    #[inline]
    /// Get the logical size, which is defined as `physical size - garbage size`
    /// i.e. the number of allocated, non-garbage nodes in `self`.
    pub fn logical_size(&self) -> NodeCount {
        self.arena.logical_size()
    }

    #[inline]
    /// Get the physical size, which is defined as the number of nodes
    /// allocated in the forest, whether they are garbage or not.
    pub fn physical_size(&self) -> NodeCount {
        self.arena.physical_size()
    }

    /// Get the garbage size i.e. the number of garbage nodes in `self`.
    #[inline]
    pub fn garbage_size(&self) -> NodeCount {
        self.arena.garbage_size()
    }

    pub fn roots(&self) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.roots.iter().copied()
    }

    pub fn add_garbage(&mut self, fidx: ForestIdx) {
        self.arena.add_garbage(fidx.0);
    }

    #[inline]
    pub fn push_root(&mut self, data: D) -> ForestIdx {
        let root_idx = self.add_node(data);
        self.roots.push(root_idx);
        root_idx
    }

    #[inline]
    pub fn insert_root(&mut self, pos: usize, data: D) -> ForestIdx {
        let root_idx = self.add_node(data);
        self.roots.insert(pos, root_idx);
        root_idx
    }

    #[inline(always)]
    pub fn set_roots(&mut self, roots: Vec<ForestIdx>) {
        self.roots = roots;
    }

    pub fn root(&mut self, fidx: ForestIdx) {
        self.roots.push(fidx);
    }

    pub fn unroot(&mut self, fidx: ForestIdx) {
        if let Some(pos) = self.roots.iter().position(|&r| r == fidx) {
            self.roots.remove(pos);
        }
    }

    #[inline]
    pub fn clear_roots(&mut self) {
        self.roots.clear();
    }

    #[inline]
    pub fn count_roots(&self) -> usize {
        self.roots.len()
    }

    /// If there is a garbage `Node<D>`in `self`, recycle it.
    /// Otherwise, allocate a new one.
    pub fn add_node(&mut self, data: D) -> ForestIdx {
        ForestIdx(self.arena.add_node(data))
    }

    #[inline]
    #[must_use]
    /// Recycle `self[node_idx]`.  Since a node conceptually owns
    /// its children, all descendant nodes and all edges between
    /// them are removed as well.
    pub fn rm_node(&mut self, fidx: ForestIdx) -> Result<()> {
        self.arena.rm_node(*fidx)?;
        Ok(())
    }

    #[inline]
    pub fn add_edge(
        &mut self,
        (pidx, cidx): (ForestIdx, ForestIdx),
        pdata: P,
        cdata: C,
    ) {
        self.arena.add_edge((*pidx, *cidx), pdata, cdata)
    }

    #[inline]
    pub fn insert_edge(
        &mut self,
        (pidx, pdata, ppos): (ForestIdx, P, Option<usize>),
        (cidx, cdata, cpos): (ForestIdx, C, Option<usize>),
    ) {
        self.arena.insert_edge(
            (*pidx, pdata, ppos),
            (*cidx, cdata, cpos)
        )
    }

    #[inline]
    #[must_use]
    pub fn rm_edge(
        &mut self,
        parent_idx: ForestIdx,
        child_idx: ForestIdx
    ) -> Result<(P, C)> {
        self.arena.rm_edge(*parent_idx, *child_idx)
    }

    #[must_use]
    #[track_caller]
    /// Copy the subtree (rooted in `root_idx`) in `src` tree to `self`,
    /// making it a subtree of `self` in the process.  Specifically,
    /// `src[root_idx]` becomes a child node of `self[dst_node_idx]`.
    /// Note that the `ForestIdx`s of the nodes in `src` will *NOT* be
    /// valid in `self`.
    pub fn copy_subtree(
        &mut self,
        dst_node_idx: ForestIdx,
        (src, root_idx): (&Self, ForestIdx),
    ) -> Result<()>
    where
        D: Clone,
        P: Clone + Default,
        C: Clone + Default,
    {
        type SrcTreeIdx = Option<ForestIdx>;
        type DstTreeIdx = ForestIdx;
        let mut map = HashMap::<SrcTreeIdx, DstTreeIdx>::new();
        map.insert(None, dst_node_idx);
        let (src, dst) = (src, self);
        for src_node_idx in src.dfs_pre(root_idx) {
            let src_parent_idx = src.parent_of(src_node_idx);
            let dst_parent_idx = map[&src_parent_idx];
            let dst_node_idx = dst.add_node(src[src_node_idx].data.clone());
            let (pdata, cdata) = if src_node_idx == root_idx {
                // NOTE: `src[src_node_idx]` has no parent edge
                (P::default(), C::default())
            } else {
                // NOTE: `src[src_node_idx]` has a parent edge
                let src_parent_idx = src_parent_idx.unwrap();
                let parent_edge = src.arena
                    .parent_edge(*src_node_idx, *src_parent_idx)
                    .unwrap();
                let child_edge = src.arena
                    .child_edge(*src_parent_idx, *src_node_idx)
                    .unwrap();
                (parent_edge.data.clone(), child_edge.data.clone())
            };
            dst.add_edge((dst_parent_idx, dst_node_idx), pdata, cdata);
            map.insert(Some(src_node_idx), dst_node_idx);
        }
        Ok(())
    }

    #[must_use]
    #[rustfmt::skip]
    /// Make `self[subroot_idx]` the last child node of `self[parent_idx]`.
    pub fn move_subtree(
        &mut self,
        parent_idx: ForestIdx,
        subroot_idx: ForestIdx,
    ) -> Result<()>
    where
        C: Default,
        P: Default,
    {
        if let Some(old_parent_idx) = self.parent_of(subroot_idx) {
            self.arena.rm_edge(*old_parent_idx, *subroot_idx)?;
        };
        self.arena.add_edge(
            (*parent_idx, *subroot_idx),
            P::default(),
            C::default(),
        );
        Ok(())
    }

    #[must_use]
    /// Replace the subtree rooted @ `self[target_idx]` with the subtree
    /// rooted @ `self[subroot_idx]`.  This means that `self[target_idx]`
    /// is removed from `self` and `self[subroot_idx]` takes its place.
    pub fn replace_subtree(
        &mut self,
        target_idx: ForestIdx,
        subroot_idx: ForestIdx,
    ) -> Result<()>
    where
        P: Default,
        C: Default,
    {
        if let Some(parent_idx) = self.parent_of(subroot_idx) {
            self.arena.rm_edge(*parent_idx, *subroot_idx)?;
        }
        if let Some(parent_idx) = self.parent_of(target_idx) {
            let child_pos = self[parent_idx].get_child_ordinal(*target_idx);
            self.arena.rm_edge(*parent_idx, *target_idx)?;
            self.arena.insert_edge(
                (*parent_idx,  P::default(), None),
                (*subroot_idx, C::default(), child_pos),
            );
        }
        self.arena.rm_node(*target_idx)?;
        Ok(())
    }

    /// Remove a sub-tree rooted in `self[fidx]` from `self`.
    /// If `force`, the removal happens regardless of whether or not
    /// `self[fidx]` has any parents.
    /// If `!force`, the removal will only happen if `self[fidx]` is
    /// an orphan node, i.e. it respects the ownersip of the parent(s).
    #[inline(always)]
    pub fn rm_subtree(
        &mut self,
        fidx: ForestIdx,
        force: bool,
    ) -> Result<()> {
        if **fidx >= *self.physical_size() {
            return Err(Error::NodeNotFound(*fidx))?;
        }
        for nidx in self.dfs_pre(fidx).collect::<Vec<_>>() {
            if !force && self[nidx].has_parents() { continue }
            let parent_idxs = self[nidx].parent_idxs()
                .map(ForestIdx::from)
                .collect::<Vec<_>>(/*avoid borrowck*/);
            for &pidx in &parent_idxs {
                self.rm_edge(pidx, nidx)?;
            }
            self.unroot(nidx);
            self.arena.add_garbage(*nidx);
            let child_idxs = self[nidx].child_idxs()
                .map(ForestIdx::from)
                .collect::<Vec<_>>(/*avoid borrowck*/);
            for &cidx in &child_idxs {
                self.rm_edge(nidx, cidx)?;
                self.root(cidx);
            }
        }
        Ok(())
    }

    #[must_use]
    /// Remove all descendant nodes of `self[node_idx]`, but
    /// not `self[node_idx]` itself.
    pub fn rm_descendants_of(&mut self, fidx: ForestIdx) -> Result<()> {
        let children: Vec<_> = self[fidx].child_idxs().collect();
        for child_idx in children.into_iter().map(ForestIdx) {
            self.rm_edge(fidx, child_idx)?;
            self.arena.rm_node(*child_idx)?;
        }
        Ok(())
    }

    #[inline(always)]
    pub fn self_or_ancestors_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.self_or_ancestors_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn ancestors_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.ancestors_of(*fidx).map(ForestIdx)
    }

    #[inline]
    pub fn parent_of(&self, node_idx: ForestIdx) -> Option<ForestIdx> {
        self[node_idx].parents.get(0).map(|(pidx, _pdata)| ForestIdx(*pidx))
    }

    #[inline(always)]
    pub fn self_or_siblings_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.self_or_siblings_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn siblings_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.siblings_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn children_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.children_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn self_or_descendants_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.self_or_descendants_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn descendants_of(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> + '_ {
        self.arena.descendants_of(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn dfs_pre(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> {
        self.arena.dfs_pre(*fidx).map(ForestIdx)
    }

    #[inline(always)]
    pub fn dfs_post(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> {
        self.arena.dfs_post(*fidx).map(ForestIdx)
    }

    #[allow(unused)] // Intentionally not publicly exposed, for now
    #[inline(always)]
    pub(crate) fn dfs_base(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = (TraversalType, ForestIdx)> {
        self.arena.dfs_base(*fidx)
            .map(|(ttype, nidx)| (ttype, ForestIdx::from(nidx)))
    }

    #[inline(always)]
    pub fn bfs(
        &self,
        fidx: ForestIdx,
    ) -> impl DoubleEndedIterator<Item = ForestIdx> {
        self.arena.bfs(*fidx).map(ForestIdx)
    }

    #[inline]
    #[must_use]
    pub fn ensure_node_is_branch(&self, fidx: ForestIdx) -> Result<()> {
        if self[fidx].is_branch_node() {
            Ok(())
        } else {
            Err(Error::ExpectedBranchNode(*fidx))
        }
    }

    #[inline]
    #[must_use]
    pub fn ensure_node_is_leaf(&self, fidx: ForestIdx) -> Result<()> {
        if self[fidx].is_leaf_node() {
            Ok(())
        } else {
            Err(Error::ExpectedLeafNode(*fidx))
        }
    }

    #[inline]
    #[must_use]
    pub fn ensure_node_is_root(&self, fidx: ForestIdx) -> Result<()> {
        if self[fidx].is_root_node() {
            Ok(())
        } else {
            Err(Error::ExpectedRootNode(*fidx))
        }
    }


    #[cfg(feature = "graphviz")]
    pub fn to_graphviz_graph(&self, root_idx: ForestIdx) -> DotGraph
    where
        D: std::fmt::Display,
        P: std::fmt::Display,
        C: std::fmt::Display,
    {
        let mut graph = DotGraph {
            rankdir: Some(DotRankDir::TopToBottom),
            ..DotGraph::default()
        };
        for node_idx in self.bfs(root_idx) {
            let node @ Node { idx, data, .. } = &self[node_idx];
            graph.add(DotNode {
                idx: *idx,
                attrs: DotAttrs {
                    label: Some(format!("{data}")),
                    ..DotAttrs::default()
                },
            });
            for (pidx, pdata) in node.parents() {
                graph.add(DotEdge {
                    src: *idx,
                    dst: pidx,
                    attrs: DotAttrs {
                        label: Some(format!("{pdata}")),
                        color: Some("purple".to_string()),
                        ..DotAttrs::default()
                    },
                });
            }
            for (cidx, cdata) in node.children() {
                graph.add(DotEdge {
                    src: *idx,
                    dst: cidx,
                    attrs: DotAttrs {
                        label: Some(format!("{cdata}")),
                        color: Some("#36454F".to_string()), // charcoal
                        ..DotAttrs::default()
                    },
                });
            }
        }
        graph
    }
}

impl<D, P, C> std::ops::Index<ForestIdx> for Forest<D, P, C> {
    type Output = Node<D, P, C>;

    fn index(&self, idx: ForestIdx) -> &Self::Output {
        &self.arena[*idx]
    }
}

impl<D, P, C> std::ops::IndexMut<ForestIdx> for Forest<D, P, C> {
    fn index_mut(&mut self, idx: ForestIdx) -> &mut Self::Output {
        &mut self.arena[*idx]
    }
}


impl<D, P, C> PartialEq<Self> for Forest<D, P, C>
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
        let sroots: Vec<_> =  self.roots().collect();
        let oroots: Vec<_> = other.roots().collect();
        if sroots.len() != oroots.len() {
            return false;
        }
        let mut map = HashMap::new();
        for (&sroot, &oroot) in sroots.iter().zip(oroots.iter()) {
            // map.insert(sroot, oroot);
            for (sidx, oidx) in self.dfs_pre(sroot).zip(other.dfs_pre(oroot)) {
                map.insert(sidx, oidx);
                let (snode, onode) = (&self[sidx], &other[oidx]);
                match (&*snode.parents, &*onode.parents) {
                    (&[], &[]) => {/*NOP*/}
                    (&[(spidx, _)], &[(opidx, _)])
                        if map[&ForestIdx(spidx)] == ForestIdx(opidx) =>
                    {
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
        }
        true
    }
}

#[rustfmt::skip]
impl<D, P, C> Eq for Forest<D, P, C>
where
    D: Eq,
    P: Eq,
    C: Eq,
{}

#[rustfmt::skip]
impl<D, P, C> PartialOrd<Self> for Forest<D, P, C>
where
    D: PartialOrd,
    P: PartialOrd,
    // C: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().partial_cmp(&other.logical_size());
        if let Some(Ordering::Greater | Ordering::Less) = size_cmp {
            return size_cmp;
        }
        let sroots: Vec<_> =  self.roots().collect();
        let oroots: Vec<_> = other.roots().collect();
        let mut map = HashMap::new();
        for (&sroot, &oroot) in sroots.iter().zip(oroots.iter()) {
            // map.insert(sroot, oroot);
            for (sidx, oidx) in self.dfs_pre(sroot).zip(other.dfs_pre(oroot)) {
                map.insert(sidx, oidx);
                let (snode, onode) = (&self[sidx], &other[oidx]);
                match (&*snode.parents, &*onode.parents) {
                    (&[], &[]) => {/*NOP*/}
                    (&[(spidx, _)], &[(opidx, _)])
                        if map[&ForestIdx(spidx)] == ForestIdx(opidx) =>
                    {
                        // NOP
                    }
                    _ => return snode.parents.partial_cmp(&onode.parents),
                }
                let child_count_cmp = snode.count_children()
                    .partial_cmp(&onode.count_children());
                if let Some(Ordering::Greater | Ordering::Less) = child_count_cmp {
                    return child_count_cmp;
                }
                let data_cmp = snode.data.partial_cmp(&onode.data);
                if let Some(Ordering::Greater | Ordering::Less) = data_cmp {
                    return data_cmp;
                }
            }
        }

        Some(Ordering::Equal)
    }
}

#[rustfmt::skip]
impl<D, P, C> Ord for Forest<D, P, C>
where
    D: Ord,
    P: Ord,
    C: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        // NOTE: The idea is to do a logical comparison where:
        // 1. Garbage nodes are excluded from comparison
        // 2. Non-garbage nodes are compared in DFS order
        let size_cmp = self.logical_size().cmp(&other.logical_size());
        if let Ordering::Greater | Ordering::Less = size_cmp {
            return size_cmp;
        }
        let sroots: Vec<_> =  self.roots().collect();
        let oroots: Vec<_> = other.roots().collect();
        let mut map = HashMap::new();
        for (&sroot, &oroot) in sroots.iter().zip(oroots.iter()) {
            // map.insert(sroot, oroot);
            for (sidx, oidx) in self.dfs_pre(sroot).zip(other.dfs_pre(oroot)) {
                map.insert(sidx, oidx);
                let (snode, onode) = (&self[sidx], &other[oidx]);
                match (&*snode.parents, &*onode.parents) {
                    (&[], &[]) => {/*NOP*/}
                    (&[(spidx, _)], &[(opidx, _)])
                        if map[&ForestIdx(spidx)] == ForestIdx(opidx) =>
                    {
                        // NOP
                    }
                    _ => return snode.parents.cmp(&onode.parents),
                }
                let child_count_cmp = snode.count_children()
                    .cmp(&onode.count_children());
                if let Ordering::Greater | Ordering::Less = child_count_cmp {
                    return child_count_cmp;
                }
                let data_cmp = snode.data.cmp(&onode.data);
                if let Ordering::Greater | Ordering::Less = data_cmp {
                    return data_cmp;
                }
            }
        }
        Ordering::Equal
    }
}

impl<D, P, C> fmt::Display for Forest<D, P, C>
where
    D: std::fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // NOTE: This loop is `O(T * D * N)`, where:
        //       - T is the number of tree root nodes in `self`
        //       - D is the maximum depth of `self`
        //       - N is the number of nodes in `self`
        for root_idx in self.roots() {
            for node_idx in self.dfs_pre(root_idx) {
                for _ in self.ancestors_of(node_idx) {
                    write!(f, "| ")?; // no newline
                }
                let Node { idx, data, .. } = &self[node_idx];
                writeln!(f, "{idx} {data}")?;
            }
        }
        Ok(())
    }
}

// Manual impl to serialize a `Forest<D>` with relaxed requirements on `D`
#[rustfmt::skip]
impl<D, P, C> Serialize for Forest<D, P, C>
where
    D: Serialize,
    P: Serialize,
    C: Serialize,
{
    fn serialize<S: Serializer>(
        &self,
        serializer: S
    ) -> std::result::Result<S::Ok, S::Error> {
        const NUM_FIELDS: usize = 2;
        let mut state = serializer.serialize_struct("Forest", NUM_FIELDS)?;
        state.serialize_field("arena", &self.arena)?;
        state.serialize_field("roots", &self.roots)?;
        state.end()
    }
}

// Manual impl to deserialize a `Forest<D>` with relaxed requirements on `D`
#[rustfmt::skip]
impl<'de, D, P, C> Deserialize<'de> for Forest<D, P, C>
where
    D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
    P: Clone + Debug + Default + PartialEq + Deserialize<'de>,
    C: Clone + Debug + Default + PartialEq + Deserialize<'de>,

    // D: Deserialize<'de>, // TODO
    // P: Deserialize<'de>, // TODO
    // C: Deserialize<'de>, // TODO
{
    fn deserialize<DE: Deserializer<'de>>(
        d: DE
    ) -> std::result::Result<Self, DE::Error> {
        #[derive(serde_derive::Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Arena,
            Roots,
        }

        struct ForestVisitor<D, P, C>(
            std::marker::PhantomData<D>,
            std::marker::PhantomData<P>,
            std::marker::PhantomData<C>,
        );

        impl<'de, D, P, C> Visitor<'de> for ForestVisitor<D, P, C>
        where
            D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
            P: Clone + Debug + Default + PartialEq + Deserialize<'de>,
            C: Clone + Debug + Default + PartialEq + Deserialize<'de>,
        {
            type Value = Forest<D, P, C>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct Forest<D>")
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
                let roots = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(Forest { arena, roots })
            }

            fn visit_map<A>(
                self,
                mut map: A
            ) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut arena = None;
                let mut roots = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Arena if arena.is_some() => {
                            return Err(de::Error::duplicate_field("arena"));
                        }
                        Field::Arena => { arena = Some(map.next_value()?); }
                        Field::Roots if roots.is_some() => {
                            return Err(de::Error::duplicate_field("roots"));
                        }
                        Field::Roots => { roots = Some(map.next_value()?); }
                    }
                }
                Ok(Forest {
                    arena: arena.ok_or_else(|| de::Error::missing_field("arena"))?,
                    roots: roots.ok_or_else(|| de::Error::missing_field("roots"))?,
                })
            }
        }

        d.deserialize_map(ForestVisitor(
            std::marker::PhantomData,
            std::marker::PhantomData,
            std::marker::PhantomData,
        ))
    }
}


impl<D, P, C> deltoid::Core for Forest<D, P, C> {
    type Delta = ErrorDelta;
}

impl<D, P, C> deltoid::Apply for Forest<D, P, C>
where
    D: Clone + Debug + PartialEq,
    P: Clone + Debug,
    C: Clone + Debug,
{
    fn apply(&self, _delta: Self::Delta) -> deltoid::DeltaResult<Self> {
        unimplemented!("impl deltoid::Apply for Forest<D, P, C>") // TODO
    }
}

impl<D, P, C> deltoid::Delta for Forest<D, P, C>
where
    D: Clone + Debug + PartialEq,
    P: Clone + Debug,
    C: Clone + Debug,
{
    fn delta(&self, _rhs: &Self) -> deltoid::DeltaResult<Self::Delta> {
        unimplemented!("impl deltoid::Delta for Forest<D, P, C>") // TODO
    }
}

impl<D, P, C> deltoid::FromDelta for Forest<D, P, C>
where
    D: Clone + Debug + PartialEq,
    P: Clone + Debug,
    C: Clone + Debug,
{
    fn from_delta(_delta: Self::Delta) -> deltoid::DeltaResult<Self> {
        unimplemented!("impl deltoid::FromDelta for Forest<D, P, C>") // TODO
    }
}

impl<D, P, C> deltoid::IntoDelta for Forest<D, P, C>
where
    D: Clone + Debug + PartialEq,
    P: Clone + Debug,
    C: Clone + Debug,
{
    fn into_delta(self) -> deltoid::DeltaResult<Self::Delta> {
        unimplemented!("impl deltoid::INtoDelta for Forest<D, P, C>") // TODO
    }
}

#[derive(Clone, PartialEq, serde_derive::Serialize, serde_derive::Deserialize)]
pub struct ErrorDelta {
    // TODO
}

impl std::fmt::Debug for ErrorDelta {
    fn fmt(&self, _f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unimplemented!() // TODO
    }
}



#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::From,
)]
pub struct ForestIdx(NodeIdx);

impl std::fmt::Debug for ForestIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ForestIdx({})", self.0)
    }
}

impl std::fmt::Display for ForestIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<usize> for ForestIdx {
    fn from(idx: usize) -> Self {
        Self(NodeIdx::from(idx))
    }
}

impl std::ops::Deref for ForestIdx {
    type Target = NodeIdx;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}



#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Data {
        forest: Forest<&'static str, (), ()>,
        roots: Vec<ForestIdx>,
        bfs_order0: Vec<ForestIdx>,
        bfs_order1: Vec<ForestIdx>,
        dfs_order0: Vec<ForestIdx>,
        dfs_order1: Vec<ForestIdx>,
        subtree_bfs_order: Vec<ForestIdx>,
        subtree_dfs_order: Vec<ForestIdx>,
    }

    #[rustfmt::skip]
    fn make_data() -> Result<Data> {
        let mut forest: Forest<&str, (), ()> = Forest::default();
        let root0: ForestIdx = forest.push_root(""); // NOTE: <--
        let node0: ForestIdx = forest.add_node("");
        let node00: ForestIdx = forest.add_node("");
        let node01: ForestIdx = forest.add_node("");
        let node1: ForestIdx = forest.add_node("");
        let node10: ForestIdx = forest.add_node("");
        let node2: ForestIdx = forest.add_node("");
        let node20: ForestIdx = forest.add_node("");
        let node200: ForestIdx = forest.add_node("");
        let node21: ForestIdx = forest.add_node("");
        forest.add_edge((root0,  node0),   (), ());
        forest.add_edge((node0,  node00),  (), ());
        forest.add_edge((node0,  node01),  (), ());
        forest.add_edge((root0,  node1),   (), ());
        forest.add_edge((node1,  node10),  (), ());
        forest.add_edge((root0,  node2),   (), ());
        forest.add_edge((node2,  node20),  (), ());
        forest.add_edge((node20, node200), (), ());
        forest.add_edge((node2,  node21),  (), ());
        let root1: ForestIdx = forest.push_root(""); // NOTE: <--
        let node3: ForestIdx = forest.add_node("");
        let node30: ForestIdx = forest.add_node("");
        let node31: ForestIdx = forest.add_node("");
        let node4: ForestIdx = forest.add_node("");
        let node40: ForestIdx = forest.add_node("");
        let node5: ForestIdx = forest.add_node("");
        let node50: ForestIdx = forest.add_node("");
        let node500: ForestIdx = forest.add_node("");
        let node51: ForestIdx = forest.add_node("");
        forest.add_edge((root1,  node3),   (), ());
        forest.add_edge((node3,  node30),  (), ());
        forest.add_edge((node3,  node31),  (), ());
        forest.add_edge((root1,  node4),   (), ());
        forest.add_edge((node4,  node40),  (), ());
        forest.add_edge((root1,  node5),   (), ());
        forest.add_edge((node5,  node50),  (), ());
        forest.add_edge((node50, node500), (), ());
        forest.add_edge((node5,  node51),  (), ());
        Ok(Data {
            forest,
            roots: vec![root0, root1],
            bfs_order0: vec![
                root0,
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
            bfs_order1: vec![
                root1,
                node3,
                node4,
                node5,
                node30,
                node31,
                node40,
                node50,
                node51,
                node500,
            ],
            dfs_order0: vec![
                root0,
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
            dfs_order1: vec![
                root1,
                node3,
                node30,
                node31,
                node4,
                node40,
                node5,
                node50,
                node500,
                node51,
            ],
            subtree_bfs_order: vec![
                root0,
                node0,
                node1,
                node2,
                node00,
                node01,
                node10,
                node20,
                node21,
                node200
            ],
            subtree_dfs_order: vec![
                root0,
                node0,
                node00,
                node01,
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
    fn bfs_traversal_basic() -> Result<()> {
        let data = make_data()?;
        let bfs_order0: Vec<_> = data.forest.bfs(data.roots[0]).collect();
        assert_eq!(bfs_order0, data.bfs_order0);
        let bfs_order1: Vec<_> = data.forest.bfs(data.roots[1]).collect();
        assert_eq!(bfs_order1, data.bfs_order1);
        Ok(())
    }

    #[test]
    fn dfs_traversal_basic() -> Result<()> {
        let data = make_data()?;
        let dfs_order0: Vec<_> = data.forest.dfs_pre(data.roots[0]).collect();
        assert_eq!(dfs_order0, data.dfs_order0);
        let dfs_order1: Vec<_> = data.forest.dfs_pre(data.roots[1]).collect();
        assert_eq!(dfs_order1, data.dfs_order1);
        Ok(())
    }

    #[test]
    #[allow(unused)]
    fn copy_subforest() -> Result<()> {
        let mut forest = forest! {
            ("root",
             ("node0"),
             ("node1",
              ("node10",
               ("node100"))),
             ("node2",
              ("node20")))
        };
        let node20 = ForestIdx::from(6);

        let mut subtree = forest! {
            ("root (subtree)",
             ("node0 (subtree)"),
             ("node1 (subtree)",
              ("node10 (subtree)",
               ("node100 (subtree)"))),
             ("node2 (subtree)",
              ("node20 (subtree)")))
        };
        let subtree_root = ForestIdx::from(0);

        forest.copy_subtree(node20, (&subtree, subtree_root))?;

        let expected = forest! {
            ("root",
             ("node0"),
             ("node1",
              ("node10",
               ("node100"))),
             ("node2",
              ("node20",
               ("root (subtree)",
                ("node0 (subtree)"),
                ("node1 (subtree)",
                 ("node10 (subtree)",
                  ("node100 (subtree)"))),
                ("node2 (subtree)",
                 ("node20 (subtree)"))))))
        };
        assert_eq!(forest, expected, "\n{} !=\n{}", forest, expected);

        Ok(())
    }

    #[test]
    #[allow(unused)]
    fn remove_subtree() -> Result<()> {
        let mut data = make_data()?;
        let node0 = ForestIdx::from(1);
        data.forest.rm_subtree(node0, false)?;
        let subtree_bfs_order: Vec<_> = data.forest
            .bfs(data.roots[0])
            .collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);
        let subtree_dfs_order: Vec<_> = data.forest
            .dfs_pre(data.roots[0])
            .collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    fn remove_subtree_multiply() -> Result<()> {
        let mut data = make_data()?;
        let node0 = ForestIdx::from(1);

        println!("initial:\n{}\n{:#?}", data.forest, data.forest);
        // assert!(data.forest.garbage.is_empty());

        data.forest.rm_subtree(node0, false)?;
        println!("after removal 1:\n{}\n{:#?}", data.forest, data.forest);
        // assert_eq!(data.forest.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        data.forest.rm_subtree(node0, false)?;
        println!("after removal 2:\n{}\n{:#?}", data.forest, data.forest);
        // assert_eq!(data.forest.garbage, [NodeIdx(3), NodeIdx(2), NodeIdx(1)]);

        let subtree_bfs_order: Vec<_> = data.forest.bfs(data.roots[0]).collect();
        assert_eq!(subtree_bfs_order, data.subtree_bfs_order);

        let subtree_dfs_order: Vec<_> = data.forest
            .dfs_pre(data.roots[0])
            .collect();
        assert_eq!(subtree_dfs_order, data.subtree_dfs_order);
        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn move_subtree() -> Result<()> {
        let data = make_data()?;
        let mut forest = data.forest.clone();

        let (parent_idx, subroot_idx) = (ForestIdx::from(1), ForestIdx::from(6));
        // Make `tree[subroot_idx]` a child of `tree[parent_idx]`,
        // i.e. move the subtree rooted in `tree[subroot_idx]`:
        println!("0 forest:\n{forest}");
        forest.move_subtree(parent_idx, subroot_idx)?;
        println!("1 forest:\n{forest}");

        let (parent_idx, subroot_idx) = (ForestIdx::from(0), ForestIdx::from(6));
        // Make `tree[subroot_idx]` a child of its grandparent,
        // i.e. move the subtree back:
        forest.move_subtree(parent_idx, subroot_idx)?;
        let expected = data.forest;
        assert_eq!(
            forest, expected,
            "\nforest:\n{forest}\n    !=\n    expected:\n{expected}"
        );

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn replace_subtree() -> Result<()> {
        let data = make_data()?;
        let mut forest = data.forest;
        println!("0 forest:\n\n{forest}\n{forest:#?}\n");

        let (target_idx, subroot_idx) = (ForestIdx::from(2), ForestIdx::from(6));
        // Replace `self[target_idx]` with `self[subroot_idx]`:
        forest.replace_subtree(target_idx, subroot_idx)?;
        println!("1 forest:\n\n{forest}\n{forest:#?}\n");

        assert_eq!(
            forest[ForestIdx::from(0)].children,
            [(NodeIdx(1), ()), (NodeIdx(4), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(1)].children,
            [(NodeIdx(6), ()), (NodeIdx(3), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(6)].children,
            [(NodeIdx(7), ()), (NodeIdx(9), ())]
        );
        assert_eq!(forest[ForestIdx::from(7)].children, [(NodeIdx(8), ())]);
        assert_eq!(forest[ForestIdx::from(4)].children, [(NodeIdx(5), ())]);
        assert_eq!(forest.parent_of(ForestIdx::from(0)), None);
        assert_eq!(forest.parent_of(ForestIdx::from(1)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(4)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(6)), Some(ForestIdx::from(1)));
        assert_eq!(forest.parent_of(ForestIdx::from(3)), Some(ForestIdx::from(1)));
        assert_eq!(forest.parent_of(ForestIdx::from(7)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(9)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(8)), Some(ForestIdx::from(7)));
        assert_eq!(forest.parent_of(ForestIdx::from(5)), Some(ForestIdx::from(4)));

        let (target_idx, subroot_idx) = (ForestIdx::from(4), ForestIdx::from(6));
        // Replace `self[target_idx]` with `self[subroot_idx]`:
        forest.replace_subtree(target_idx, subroot_idx)?;
        println!("2 forest:\n\n{forest}\n{forest:#?}\n");

        assert_eq!(
            forest[ForestIdx::from(0)].children,
            [(NodeIdx(1), ()), (NodeIdx(6), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(1)].children,
            [(NodeIdx(3), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(6)].children,
            [(NodeIdx(7), ()), (NodeIdx(9), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(7)].children,
            [(NodeIdx(8), ())]
        );
        assert_eq!(forest.parent_of(ForestIdx::from(0)), None);
        assert_eq!(forest.parent_of(ForestIdx::from(1)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(6)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(3)), Some(ForestIdx::from(1)));
        assert_eq!(forest.parent_of(ForestIdx::from(7)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(9)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(8)), Some(ForestIdx::from(7)));

        let node02_idx = forest.add_node("");
        let node020_idx = forest.add_node("");
        let node0200_idx = forest.add_node("");
        forest.add_edge((ForestIdx::from(0),  node02_idx),   (), ());
        forest.add_edge((node02_idx,  node020_idx),  (), ());
        forest.add_edge((node020_idx, node0200_idx), (), ());
        println!("3 forest:\n\n{forest}\n{forest:#?}\n");

        assert_eq!(
            forest[ForestIdx::from(0)].children,
            [(NodeIdx(1), ()), (NodeIdx(6), ()), (NodeIdx(2), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(1)].children,
            [(NodeIdx(3), ())]
        );
        assert_eq!(
            forest[ForestIdx::from(6)].children,
            [(NodeIdx(7), ()), (NodeIdx(9), ())]
        );
        assert_eq!(forest[ForestIdx::from(7)].children, [(NodeIdx(8), ())]);
        assert_eq!(forest[ForestIdx::from(2)].children, [(NodeIdx(5), ())]);
        assert_eq!(forest[ForestIdx::from(5)].children, [(NodeIdx(4), ())]);
        assert_eq!(forest.parent_of(ForestIdx::from(0)), None);
        assert_eq!(forest.parent_of(ForestIdx::from(1)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(6)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(2)), Some(ForestIdx::from(0)));
        assert_eq!(forest.parent_of(ForestIdx::from(3)), Some(ForestIdx::from(1)));
        assert_eq!(forest.parent_of(ForestIdx::from(7)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(9)), Some(ForestIdx::from(6)));
        assert_eq!(forest.parent_of(ForestIdx::from(8)), Some(ForestIdx::from(7)));
        assert_eq!(forest.parent_of(ForestIdx::from(5)), Some(ForestIdx::from(2)));
        assert_eq!(forest.parent_of(ForestIdx::from(4)), Some(ForestIdx::from(5)));

        Ok(())
    }

    #[test]
    #[rustfmt::skip]
    fn rm_descendants_of() -> Result<()> {
        let mut data = make_data()?;
        println!("forest 0:\n{}", data.forest);
        data.forest.rm_descendants_of(ForestIdx::from(6))?;
        println!("forest 1:\n{}", data.forest);
        assert!(data.forest[ForestIdx::from(6)].children.is_empty());
        let root0_idx = data.forest.roots().nth(0).unwrap();
        assert_eq!(
            &data.forest[root0_idx].children,
            &[(NodeIdx(1), ()), (NodeIdx(4), ()), (NodeIdx(6), ())]
        );
        Ok(())
    }

    #[test]
    fn ancestors_of() -> Result<()> {
        let data = make_data()?;
        println!("forest:\n{}", data.forest);
        let node_idx = ForestIdx::from(8);
        let ancestors: Vec<_> = data.forest.ancestors_of(node_idx).collect();
        println!("ancestor indices:\n{:?}", ancestors);
        assert_eq!(
            ancestors,
            &[ForestIdx::from(7), ForestIdx::from(6), ForestIdx::from(0)]
        );
        Ok(())
    }

    #[test]
    fn siblings_of() -> Result<()> {
        let data = make_data()?;
        println!("forest:\n{}", data.forest);
        let node_idx = ForestIdx::from(4);
        let siblings: Vec<_> = data.forest.siblings_of(node_idx).collect();
        println!("sibling indices:\n{:?}", siblings);
        assert_eq!(siblings, &[ForestIdx::from(1), ForestIdx::from(6)]);
        Ok(())
    }

    #[test]
    fn children_of() -> Result<()> {
        let data = make_data()?;
        println!("forest:\n{}", data.forest);
        let node_idx = ForestIdx::from(1);
        let children: Vec<_> = data.forest.children_of(node_idx).collect();
        println!("child indices:\n{:?}", children);
        assert_eq!(children, &[ForestIdx::from(2), ForestIdx::from(3)]);
        Ok(())
    }

    #[test]
    fn descendants_of() -> Result<()> {
        let data = make_data()?;
        println!("forest:\n{}", data.forest);
        let node_idx = ForestIdx::from(6);
        let descendants: Vec<_> = data.forest.descendants_of(node_idx).collect();
        println!("descendant indices:\n{:?}", descendants);
        assert_eq!(
            descendants,
            &[ForestIdx::from(7), ForestIdx::from(8), ForestIdx::from(9)]
        );
        Ok(())
    }

    #[test]
    #[allow(non_snake_case)]
    fn dfs_base() {
        use crate::*;
        struct Data { name: String }

        let mut forest: Forest<Data, (), ()> = Forest::default();
        let node_A_idx = forest.push_root(Data { name: "A".into() });
        let node_B_idx = forest.add_node(Data { name: "B".into() });
        let node_C_idx = forest.add_node(Data { name: "C".into() });
        let node_D_idx = forest.add_node(Data { name: "D".into() });
        let node_E_idx = forest.add_node(Data { name: "E".into() });
        let node_F_idx = forest.add_node(Data { name: "F".into() });
        forest.add_edge((node_A_idx, node_B_idx), (), ());
        forest.add_edge((node_A_idx, node_E_idx), (), ());
        forest.add_edge((node_B_idx, node_C_idx), (), ());
        forest.add_edge((node_B_idx, node_D_idx), (), ());
        forest.add_edge((node_E_idx, node_B_idx), (), ());
        forest.add_edge((node_E_idx, node_F_idx), (), ());

        let dfs_base: Vec<(ForestIdx, _, _)> = forest.dfs_base(node_A_idx)
            .map(|(ttype, fidx)| (fidx, ttype, forest[fidx].data.name.clone()))
            .collect();
        assert_eq!(&*dfs_base, &[
            (ForestIdx::from(0), TraversalType::Pre,          "A".to_string()),
            (ForestIdx::from(1), TraversalType::Pre,          "B".to_string()),
            (ForestIdx::from(2), TraversalType::Pre,          "C".to_string()),
            (ForestIdx::from(2), TraversalType::Post,         "C".to_string()),
            (ForestIdx::from(1), TraversalType::Interspersed, "B".to_string()),
            (ForestIdx::from(3), TraversalType::Pre,          "D".to_string()),
            (ForestIdx::from(3), TraversalType::Post,         "D".to_string()),
            (ForestIdx::from(1), TraversalType::Post,         "B".to_string()),
            (ForestIdx::from(0), TraversalType::Interspersed, "A".to_string()),
            (ForestIdx::from(4), TraversalType::Pre,          "E".to_string()),
            (ForestIdx::from(1), TraversalType::Pre,          "B".to_string()),
            (ForestIdx::from(2), TraversalType::Pre,          "C".to_string()),
            (ForestIdx::from(2), TraversalType::Post,         "C".to_string()),
            (ForestIdx::from(1), TraversalType::Interspersed, "B".to_string()),
            (ForestIdx::from(3), TraversalType::Pre,          "D".to_string()),
            (ForestIdx::from(3), TraversalType::Post,         "D".to_string()),
            (ForestIdx::from(1), TraversalType::Post,         "B".to_string()),
            (ForestIdx::from(4), TraversalType::Interspersed, "E".to_string()),
            (ForestIdx::from(5), TraversalType::Pre,          "F".to_string()),
            (ForestIdx::from(5), TraversalType::Post,         "F".to_string()),
            (ForestIdx::from(4), TraversalType::Post,         "E".to_string()),
            (ForestIdx::from(0), TraversalType::Post,         "A".to_string()),
        ]);
    }

    #[test]
    #[allow(non_snake_case)]
    fn dfs_pre() {
        use crate::*;
        struct Data { name: String }

        let mut forest: Forest<Data, (), ()> = Forest::default();
        let node_A_idx = forest.push_root(Data { name: "A".into() });
        let node_B_idx = forest.add_node(Data { name: "B".into() });
        let node_C_idx = forest.add_node(Data { name: "C".into() });
        let node_D_idx = forest.add_node(Data { name: "D".into() });
        let node_E_idx = forest.add_node(Data { name: "E".into() });
        let node_F_idx = forest.add_node(Data { name: "F".into() });
        forest.add_edge((node_A_idx, node_B_idx), (), ());
        forest.add_edge((node_A_idx, node_E_idx), (), ());
        forest.add_edge((node_B_idx, node_C_idx), (), ());
        forest.add_edge((node_B_idx, node_D_idx), (), ());
        forest.add_edge((node_E_idx, node_B_idx), (), ());
        forest.add_edge((node_E_idx, node_F_idx), (), ());

        let dfs_pre: Vec<(ForestIdx, _)> = forest.dfs_pre(node_A_idx)
            .map(|fidx| (fidx, forest[fidx].data.name.to_string()))
            .collect();
        assert_eq!(&*dfs_pre, &[
            (ForestIdx::from(0), "A".to_string()),
            (ForestIdx::from(1), "B".to_string()),
            (ForestIdx::from(2), "C".to_string()),
            (ForestIdx::from(3), "D".to_string()),
            (ForestIdx::from(4), "E".to_string()),
            (ForestIdx::from(1), "B".to_string()),
            (ForestIdx::from(2), "C".to_string()),
            (ForestIdx::from(3), "D".to_string()),
            (ForestIdx::from(5), "F".to_string()),
        ]);
    }

    #[test]
    #[allow(non_snake_case)]
    fn dfs_post() {
        use crate::*;
        struct Data { name: String }

        let mut forest: Forest<Data, (), ()> = Forest::default();
        let node_A_idx = forest.push_root(Data { name: "A".into() });
        let node_B_idx = forest.add_node(Data { name: "B".into() });
        let node_C_idx = forest.add_node(Data { name: "C".into() });
        let node_D_idx = forest.add_node(Data { name: "D".into() });
        let node_E_idx = forest.add_node(Data { name: "E".into() });
        let node_F_idx = forest.add_node(Data { name: "F".into() });
        forest.add_edge((node_A_idx, node_B_idx), (), ());
        forest.add_edge((node_A_idx, node_E_idx), (), ());
        forest.add_edge((node_B_idx, node_C_idx), (), ());
        forest.add_edge((node_B_idx, node_D_idx), (), ());
        forest.add_edge((node_E_idx, node_B_idx), (), ());
        forest.add_edge((node_E_idx, node_F_idx), (), ());

        let dfs_post: Vec<(ForestIdx, _)> = forest.dfs_post(node_A_idx)
            .map(|fidx| (fidx, forest[fidx].data.name.to_string()))
            .collect();
        assert_eq!(&*dfs_post, &[
            (ForestIdx::from(2), "C".to_string()),
            (ForestIdx::from(3), "D".to_string()),
            (ForestIdx::from(1), "B".to_string()),
            (ForestIdx::from(2), "C".to_string()),
            (ForestIdx::from(3), "D".to_string()),
            (ForestIdx::from(1), "B".to_string()),
            (ForestIdx::from(5), "F".to_string()),
            (ForestIdx::from(4), "E".to_string()),
            (ForestIdx::from(0), "A".to_string()),
        ]);
    }

}
