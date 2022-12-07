//!

use crate::error::{Error, Result};
use itertools::Itertools;

#[rustfmt::skip]
#[derive(
    Clone,
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
    pub parents: Vec<NodeIdx>,
    pub children: Vec<NodeIdx>,
    #[deref]
    #[deref_mut]
    pub data: D,
}

impl<D> Node<D> {
    pub fn new(idx: NodeIdx, data: D) -> Self {
        Node {
            idx,
            parents: Vec::with_capacity(2),
            children: Vec::with_capacity(4),
            data
        }
    }

    #[inline(always)]
    pub fn parents(&self) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.parents.iter().copied()
    }

    #[inline(always)]
    pub fn count_parents(&self) -> usize {
        self.parents.len()
    }

    #[inline(always)]
    pub fn add_parent(&mut self, parent_idx: NodeIdx) {
        self.parents.push(parent_idx);
    }

    #[inline]
    /// Insert `parent_idx` as the `pos`-th parent of `self`.
    pub fn insert_parent(&mut self, parent_idx: NodeIdx, pos: usize) {
        // pre: [0, pos)        post: [pos, len)
        let (pre, post) = self.parents.split_at(pos);
        self.children = std::iter::empty()
            .chain(pre.iter().copied())
            .chain([parent_idx])
            .chain(post.iter().copied())
            .unique()
            .collect();
    }

    /// Filter out `parent_idx` from `self.parents`.
    /// Return an error if `self.parents` does not contain `parent_idx`.
    #[inline]
    #[rustfmt::skip]
    pub fn remove_parent(&mut self, parent_idx: NodeIdx) -> Result<()> {
        if !self.parents.contains(&parent_idx) {
            return Err(Error::ParentNotFound {
                node_idx: self.idx,
                parent_idx: Some(parent_idx),
            });
        }
        self.parents = self.parents.drain(..)
            .filter(|&pidx| pidx != parent_idx)
            .collect();
        Ok(())
    }

    #[inline(always)]
    pub fn children(&self) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.children.iter().copied()
    }

    #[inline(always)]
    pub fn count_children(&self) -> usize {
        self.children.len()
    }

    #[inline(always)]
    pub fn add_child(&mut self, child_idx: NodeIdx) {
        self.children.push(child_idx);
        self.children = self.children.drain(..).unique().collect();
    }

    #[inline]
    /// Insert `child_idx` as the `pos`-th child of `self`.
    pub fn insert_child(&mut self, child_idx: NodeIdx, pos: usize) {
        let (pre, post) = self.children.split_at(pos);
        self.children = std::iter::empty()
            .chain(pre.iter().copied())
            .chain([child_idx])
            .chain(post.iter().copied())
            .unique()
            .collect();
    }

    #[inline]
    #[rustfmt::skip]
    /// Filter out `child_idx` from `self.children`.
    /// Return an error if `self.children` does not contain `child_idx`.
    pub fn remove_child(&mut self, child_idx: NodeIdx) -> Result<()> {
        if !self.children.contains(&child_idx) {
            return Err(Error::ChildNotFound { node_idx: self.idx, child_idx });
        }
        self.children = self.children.drain(..)
            .filter(|&cidx| cidx != child_idx)
            .collect();
        Ok(())
    }

    #[inline(always)]
    pub fn clear(&mut self)
    where
        D: Default,
    {
        self.parents.clear();
        self.children.clear();
        self.data = D::default();
    }

    // /// Assuming that `parent_idx` is a member of `self.parents`, return the
    // /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    // /// for `parent_idx` e.g. the leftmost parent is the `zeroth` parent of
    // /// `self`.
    // /// Return `None` if `parent_idx` is not a member of `self.parents`.
    // #[inline]
    // #[rustfmt::skip]
    // pub fn get_parent_ordinal(&self, parent_idx: NodeIdx) -> Option<usize> {
    //     self.parents.iter().enumerate()
    //         .filter(|(_, &pidx)| pidx == parent_idx)
    //         .map(|(ordinal_numeral, _)| ordinal_numeral)
    //         .next()
    // }

    // /// Assuming that `child_idx` is a member of `self.children`, return the
    // /// [ordinal numeral](https://en.wikipedia.org/wiki/Ordinal_numeral)
    // /// for `child_idx` e.g. the leftmost child is the `zeroth` child of
    // /// `self`.
    // /// Return `None` if `child_idx` is not a member of `self.children`.
    // #[inline]
    // #[rustfmt::skip]
    // pub fn get_child_ordinal(&self, child_idx: NodeIdx) -> Option<usize> {
    //     self.children.iter().enumerate()
    //         .filter(|(_, &cidx)| cidx == child_idx)
    //         .map(|(ordinal_numeral, _)| ordinal_numeral)
    //         .next()
    // }

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

impl<D: std::fmt::Debug> std::fmt::Debug for Node<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ds = f.debug_struct("Node");
        let ds = ds.field("idx", &self.idx);
        let ds = ds.field("parents", &self.parents);
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
    serde_derive::Deserialize,
    serde_derive::Serialize,
    derive_more::From
)]
pub struct NodeIdx(pub(crate) usize);

// impl NodeIdx {
//     pub const TREE_ROOT: Self = Self(0);
// }

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

impl std::ops::Add<usize> for NodeIdx {
    type Output = Self;

    #[inline(always)]
    fn add(self, rhs: usize) -> Self {
        Self(self.0 + rhs)
    }
}

impl std::ops::Sub<usize> for NodeIdx {
    type Output = Self;

    #[inline(always)]
    fn sub(self, rhs: usize) -> Self {
        Self(self.0 - rhs)
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
