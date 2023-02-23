//!

use crate::error::{Error, Result};

#[rustfmt::skip]
#[derive(
    Clone,
    Debug,
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
pub struct Node<D, P, C> {
    pub idx: NodeIdx,
    pub parents: Vec<(NodeIdx, P)>,
    pub children: Vec<(NodeIdx, C)>,
    #[deref]
    #[deref_mut]
    pub data: D,
}

impl<D, P, C> Node<D, P, C> {
    pub fn new(idx: NodeIdx, data: D) -> Self {
        Node {
            idx,
            parents: Vec::with_capacity(2),
            children: Vec::with_capacity(4),
            data
        }
    }

    #[inline(always)]
    pub fn parents(&self) -> impl DoubleEndedIterator<Item = (NodeIdx, &P)> + '_ {
        self.parents.iter().map(|(idx, data)| (*idx, data))
    }

    #[inline(always)]
    pub fn parent_idxs(&self) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.parents.iter().map(|(idx, _)| *idx)
    }

    #[inline(always)]
    pub fn parent_edges(
        &self
    ) -> impl DoubleEndedIterator<Item = Edge<NodeIdx, &P>> + '_ {
        self.parents.iter().map(|(pidx, pdata)| Edge {
            src: self.idx,
            dst: *pidx,
            data: pdata
        })
    }

    #[inline(always)]
    pub fn parent_edges_mut(
        &mut self
    ) -> impl DoubleEndedIterator<Item = Edge<NodeIdx, &mut P>> + '_ {
        self.parents.iter_mut().map(|(pidx, pdata)| Edge {
            src: self.idx,
            dst: *pidx,
            data: pdata
        })
    }

    #[inline(always)]
    pub fn count_parents(&self) -> usize {
        self.parents.len()
    }

    #[inline(always)]
    pub fn has_parents(&self) -> bool {
        self.count_parents() != 0
    }

    #[inline(always)]
    pub fn add_parent(&mut self, pidx: NodeIdx, pdata: P) {
        self.parents.push((pidx, pdata));
    }

    #[inline]
    /// Insert `(pidx, pdata)` as the `pos`-th parent of `self`.
    pub fn insert_parent(&mut self, pidx: NodeIdx, pdata: P, pos: usize) {
        self.parents.insert(pos, (pidx, pdata));
    }

    #[inline]
    #[must_use]
    #[rustfmt::skip]
    /// Filter out `parent_idx` from `self.parents`.
    /// Return an error if `self.parents` does not contain `parent_idx`.
    pub fn remove_parent(&mut self, parent_idx: NodeIdx) -> Result<P> {
        let ordinal = self.get_parent_ordinal(parent_idx)
            .ok_or_else(|| Error::ParentNotFound {
                node_idx: self.idx,
                parent_idx: Some(parent_idx),
            })?;
        let (_, pdata) = self.parents.remove(ordinal);
        Ok(pdata)
    }

    #[inline(always)]
    pub fn children(&self) -> impl DoubleEndedIterator<Item = (NodeIdx, &C)> + '_ {
        self.children.iter().map(|(idx, data)| (*idx, data))
    }

    #[inline(always)]
    pub fn child_idxs(&self) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.children.iter().map(|(idx, _)| *idx)
    }

    #[inline(always)]
    pub fn child_edges(
        &self
    ) -> impl DoubleEndedIterator<Item = Edge<NodeIdx, &C>> + '_ {
        self.children.iter().map(|(cidx, cdata)| Edge {
            src: self.idx,
            dst: *cidx,
            data: cdata
        })
    }

    #[inline(always)]
    pub fn count_children(&self) -> usize {
        self.children.len()
    }

    #[inline(always)]
    pub fn has_children(&self) -> bool {
        self.count_children() != 0
    }

    #[inline(always)]
    pub fn add_child(&mut self, cidx: NodeIdx, cdata: C) {
        self.children.push((cidx, cdata));
    }

    #[inline]
    /// Insert `child_idx` as the `pos`-th child of `self`.
    pub fn insert_child(&mut self, cidx: NodeIdx, cdata: C, pos: usize) {
        self.children.insert(pos, (cidx, cdata));
    }

    #[inline]
    #[must_use]
    #[rustfmt::skip]
    /// Filter out `child_idx` from `self.children`.
    /// Return an error if `self.children` does not contain `child_idx`.
    pub fn remove_child(&mut self, child_idx: NodeIdx) -> Result<C> {
        let ordinal = self.get_child_ordinal(child_idx)
            .ok_or_else(|| Error::ChildNotFound {
                node_idx: self.idx,
                child_idx
            })?;
        let (_, cdata) = self.children.remove(ordinal);
        Ok(cdata)
    }

    #[inline]
    #[rustfmt::skip]
    /// Assuming that `parent_idx` is a member of `self.parents`, return the
    /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    /// for `parent_idx` e.g. the leftmost parent is the `zeroth` parent of
    /// `self`.
    /// Return `None` if `parent_idx` is not a member of `self.parents`.
    pub fn get_parent_ordinal(&self, parent_idx: NodeIdx) -> Option<usize> {
        self.parent_idxs().position(|pidx| pidx == parent_idx)
    }

    #[inline]
    #[rustfmt::skip]
    /// Assuming that `child_idx` is a member of `self.children`, return the
    /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    /// for `child_idx` e.g. the leftmost child is the `zeroth` child of
    /// `self`.
    /// Return `None` if `child_idx` is not a member of `self.children`.
    pub fn get_child_ordinal(&self, child_idx: NodeIdx) -> Option<usize> {
        self.child_idxs().position(|cidx| cidx == child_idx)
    }

    #[inline]
    pub fn is_root_node(&self) -> bool {
        self.parents.is_empty()
    }

    #[inline]
    pub fn is_branch_node(&self) -> bool {
        !self.is_leaf_node()
    }

    #[inline]
    pub fn is_leaf_node(&self) -> bool {
        self.children.is_empty()
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
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::From
)]
pub struct NodeIdx(pub(crate) usize);

impl std::fmt::Debug for NodeIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "NodeIdx({})", self.0)
    }
}

impl std::fmt::Display for NodeIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}



#[rustfmt::skip]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::Deref,
    derive_more::From,
)]
pub struct NodeCount(usize);

impl NodeCount {
    pub const ZERO: Self = Self(0);
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


#[derive(Debug)]
pub struct Edge<I, R> {
    pub src: I,
    pub dst: I,
    pub data: R,
}

impl<I, R> std::fmt::Display for Edge<I, R>
where
    I: std::fmt::Display,
    R: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { src, dst, data } = &self;
        write!(f, "{src}->{dst}, {data}")?;
        Ok(())
    }
}
