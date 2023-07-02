//!

use crate::{
    arena::Arena,
    error::Result,
    node::{Edge, Node, NodeIdx}, NodeCount,
};
use std::collections::VecDeque;

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
    serde_derive::Serialize
)]
/// A stack path through a `Gss`, starting at a top-of-stack.
pub struct StackPath(VecDeque<StackIdx>);

#[macro_export]
macro_rules! stackpath {
    ($($idx:expr),+ $(,)?) => {{
        let mut path = $crate::gss::StackPath::new();
        for idx in [$($idx),+] {
            path = path.append(idx);
        }
        path
    }}
}

impl StackPath {
    #[inline]
    pub fn new() -> Self {
        Self(VecDeque::with_capacity(16))
    }

    #[inline]
    pub fn head(&self) -> Option<StackIdx> {
        self.0.front().copied()
    }

    #[inline]
    pub fn tail(&self) -> Option<StackIdx> {
        self.0.back().copied()
    }

    #[inline]
    pub fn prepend(mut self, head: StackIdx) -> Self {
        self.0.push_front(head);
        self
    }

    #[inline]
    pub fn append(mut self, tail: StackIdx) -> Self {
        self.0.push_back(tail);
        self
    }

    #[inline]
    pub fn contains_node(&self, node: StackIdx) -> bool {
        self.nodes().any(|n| n == node)
    }

    #[inline]
    pub fn contains_edge(&self, edge: (StackIdx, StackIdx)) -> bool {
        self.edges().any(|e| e == edge)
    }

    #[inline]
    pub fn nodes(&self) -> impl DoubleEndedIterator<Item = StackIdx> + '_ {
        self.0.iter().copied()
    }

    pub fn edges(
        &self
    ) -> impl DoubleEndedIterator<Item = (StackIdx, StackIdx)> + '_ {
        let mapper = |window: &[StackIdx]| if let &[nidx, pidx] = window {
            (nidx, pidx)
        } else {
            unreachable!("[StackPath::edges] Malformed window: expected len == 2")
        };
        let mut edges = vec![];
        let (front, back) = self.0.as_slices();
        match (front, back) {
            ([], []) => {/*NOP*/},
            ([], [_first, ..]) => {
                edges.extend(back.windows(2).map(&mapper));
            },
            ([.., _last], []) => {
                edges.extend(front.windows(2).map(&mapper));
            },
            ([.., last], [first, ..]) => {
                edges.extend(front.windows(2).map(&mapper));
                edges.push((*last, *first));
                edges.extend(back.windows(2).map(&mapper));
            },
        }
        edges.into_iter()
    }
}

impl IntoIterator for StackPath {
    type Item = StackIdx;

    type IntoIter = std::collections::vec_deque::IntoIter<StackIdx>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl std::fmt::Display for StackPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, node) in self.nodes().enumerate() {
            if i == 0 {
                write!(f, "{node}")?;
            } else {
                write!(f, "->{node}")?;
            }
        }
        Ok(())
    }
}




#[derive(Debug)]
/// A arena-allocated `Graph-Structured Stack` implementation.
/// A Gss is an abstraction of the well-known `Stack` datastructure such that:
///   0. It can branch off i.e. in general it has a tree-like shape
///   1. Branches can merge, as long as doing so does not result in a cycle
///   2. When pushing a new node N to a stack, the top-of-stack T to which
///      N is pushed to becomes N's parent node i.e. pushing only results in
///      new child nodes.
///   3. Each node has associated data of type `N`
///   4. Each edge has associated data of type `E`
pub struct Gss<N, E> {
    arena: Arena<N, E, ()>,
    /// The tops-of-stack
    tops: Vec<StackIdx>,
}

impl<N, E> Default for Gss<N, E> {
    fn default() -> Self {
        Self::with_capacity(1024)
    }
}

impl<N, E> Gss<N, E> {
    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            arena: Arena::with_capacity(cap),
            tops: Vec::with_capacity(8),
        }
    }

    #[inline]
    pub fn clear(&mut self) -> Result<()> {
        self.arena.clear()?;
        self.tops.clear();
        Ok(())
    }

    #[inline]
    pub fn tops(&self) -> impl DoubleEndedIterator<Item = StackIdx> + '_ {
        self.tops.iter().copied()
    }

    /// Return an iterator over all `StackPath`s that:
    /// 1. Have a given `len`gth
    /// 2. Start at a given `top`
    ///
    /// That is, all `StackPath`s yielded by the iterator contain
    /// `length + 1` `StackIdx`s and `length` edges between them.
    /// Each path is top-inclusive i.e. it contains the `top`.
    /// For example, if `self` looks like follows:
    /// ``` text
    ///          /--4--\
    /// 0--1--2--3      6--7--8
    ///       \  \--5--/  /
    ///        \---------/
    /// ```
    /// If then `self.stacks(StackIdx(7), 3)` is executed, the returned
    /// iterator will iterate over the paths `stackpath![7, 6, 4, 3]`,
    /// `stackpath![7, 6, 5, 3]` and `stackpath![7, 2, 1, 0]`.
    pub fn stack_paths(
        &self,
        top: StackIdx,
        len: usize
    ) -> impl DoubleEndedIterator<Item = StackPath> {
        let mut stackpaths: Vec<StackPath> = vec![stackpath![top]];
        for _ in 0..len {
            stackpaths = stackpaths.into_iter()
                .flat_map(|path| {
                    let tail: StackIdx = path.tail().unwrap();
                    self[tail].parent_idxs().map(move |pidx| {
                        path.clone().append(StackIdx(pidx))
                    })
                })
                .collect();
        }
        stackpaths.into_iter().filter(move |path| {
            path.edges().count() == len
        })
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.arena.logical_size() == NodeCount::ZERO
    }

    #[inline]
    pub fn parent_edges(
        &self,
        node_idx: StackIdx
    ) -> impl DoubleEndedIterator<Item = Edge<StackIdx, &E>> + '_ {
        self[node_idx].parent_edges()
            .map(|Edge { src, dst, data }| Edge {
                src: StackIdx::from(src),
                dst: StackIdx::from(dst),
                data
            })
    }

    #[inline]
    pub fn edge(
        &self,
        src: StackIdx,
        dst: StackIdx
    ) -> Option<Edge<StackIdx, &E>> {
        self[src].parent_edges()
            .find(|edge| edge.dst == *dst)
            .map(|Edge { src, dst, data }| Edge {
                src: StackIdx::from(src),
                dst: StackIdx::from(dst),
                data
            })
    }

    #[inline]
    pub fn edge_mut(
        &mut self,
        src: StackIdx,
        dst: StackIdx
    ) -> Option<Edge<StackIdx, &mut E>> {
        self[src].parent_edges_mut()
            .find(|edge| edge.dst == *dst)
            .map(|Edge { src, dst, data }| Edge {
                src: StackIdx::from(src),
                dst: StackIdx::from(dst),
                data
            })
    }

    #[inline]
    pub fn add_edge(
        &mut self,
        node: StackIdx,
        parent: StackIdx,
        edata: E
    ) -> (StackIdx, StackIdx) {
        self.arena.add_edge((*parent, *node), edata, ());
        let edge = self.edge(node, parent)
            .unwrap_or_else(|| {
                panic!("expected edge ({node}, {parent}) to exist")
            });
        (edge.src, edge.dst)
    }

    #[must_use]
    /// Push a node on top of `self[pidx]` such that:
    ///   1. The node has associated data of type `N`
    ///   2. The parent edge `nidx->pidx` has associated data of type `E`
    ///   3. The child edge `pidx->nidx` has no associated data
    pub fn push(
        &mut self,
        parent: Option<(StackIdx, E)>,
        ndata: N,
    ) -> Result<StackIdx> {
        let nidx = StackIdx(self.arena.add_node(ndata));
        if let Some((pidx, pdata)) = parent {
            self.arena.add_edge((*pidx, *nidx), pdata, ());
            while let Some(pos) = self.tops.iter().position(|&top| top == pidx) {
                self.tops.remove(pos);
            }
        };
        self.tops.push(nidx);
        Ok(nidx)
    }

    #[must_use]
    pub fn pop(&mut self, top: StackIdx) -> Result<(N, Vec<E>)>
    where
        N: Default
    {
        self.ensure_is_top_of_stack(top)?;
        let parents: Vec<_> = self[top].parent_idxs().collect();
        let mut edata: Vec<E> = Vec::with_capacity(parents.len());
        for &pidx in &parents {
            let (data, _) = self.arena.rm_edge(pidx, *top)?;
            edata.push(data);
        }
        let ndata = std::mem::take(&mut self[top].data);
        self.arena.rm_node(*top)?;
        Ok((ndata, edata))
    }

    #[inline]
    fn ensure_is_top_of_stack(&self, node: StackIdx) -> Result<()> {
        if self.tops.contains(&node) && !self[node].has_children() {
            Ok(())
        } else {
            Err(Error::ExpectedTop(*node))?
        }
    }

    #[inline(always)]
    pub fn bfs(
        &self,
        start_idx: StackIdx,
    ) -> impl DoubleEndedIterator<Item = StackIdx> {
        self.arena.bfs(*start_idx).map(StackIdx)
    }
}

#[cfg(feature = "graphviz")]
pub type Layer = Vec<StackIdx>;

impl<N, E> std::ops::Index<StackIdx> for Gss<N, E> {
    type Output = Node<N, E, ()>;

    fn index(&self, idx: StackIdx) -> &Self::Output {
        &self.arena[*idx]
    }
}

impl<N, E> std::ops::IndexMut<StackIdx> for Gss<N, E> {
    fn index_mut(&mut self, idx: StackIdx) -> &mut Self::Output {
        &mut self.arena[*idx]
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
pub struct StackIdx(NodeIdx);

impl StackIdx {
    pub const ROOT: Self = Self(NodeIdx(0));
}

impl std::fmt::Debug for StackIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StackIdx({})", self.0)
    }
}

impl std::fmt::Display for StackIdx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StackIdx({})", self.0)
    }
}

impl From<usize> for StackIdx {
    fn from(idx: usize) -> Self {
        Self(NodeIdx::from(idx))
    }
}

impl std::ops::Deref for StackIdx {
    type Target = NodeIdx;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}




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
    displaydoc::Display,
    thiserror::Error,
)]
pub enum Error {
    /// Node {0} is not a top-of-stack
    ExpectedTop(NodeIdx)
}
