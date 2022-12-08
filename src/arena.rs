//! This module deals with arena allocation.

use crate::{
    error::{Error, Result},
    node::{Node, NodeCount, NodeIdx}
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde_derive::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt::{self, Debug};

#[derive(Clone, Debug, Hash)]
pub(crate) struct Arena<D> {
    nodes: Vec<Node<D>>,
    garbage: VecDeque<NodeIdx>,
}

impl<D> Default for Arena<D> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl<D> Arena<D> {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(cap),
            garbage: VecDeque::with_capacity(cap),
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
        NodeCount::from(self.nodes.len())
    }

    /// Get the garbage size i.e. the number of garbage nodes in `self`.
    #[inline]
    pub fn garbage_size(&self) -> NodeCount {
        NodeCount::from(self.garbage.len())
    }

    /// If there is a garbage `Node<D>`in `self`, recycle it.
    /// Otherwise, allocate a new one.
    /// In either case, assign `data` to the node, and return its `NodeIdx`.
    pub fn add_node(&mut self, data: D) -> NodeIdx {
        if let Some(node_idx) = self.garbage.pop_front() {
            self[node_idx].data = data;
            node_idx
        } else {
            let node_idx = NodeIdx(self.nodes.len());
            self.nodes.push(Node::new(node_idx, data));
            node_idx
        }
    }

    /// Recycle `self[node_idx]`.  Since a node conceptually owns
    /// its children, all descendant nodes and all edges between
    /// them are removed as well.
    pub fn rm_node(&mut self, node_idx: NodeIdx) -> Result<()> {
        if node_idx.0 >= self.nodes.len() {
            return Err(Error::NodeNotFound(node_idx));
        }
        for idx in self.dfs(node_idx).rev(/* leaves -> ... -> node_idx */) {
            // NOTE: Don't reset the `idx` field of `self[node_idx]`, since
            //       `Node<_>` identity persists between allocations.
            debug_assert!(self[idx].is_leaf_node());
            self.rm_edges(self[idx].parents.clone(), [idx])?;
            debug_assert!(self[idx].is_root_node());
            // NOTE: Don't clear the  data field of `self[node_idx]`, for perf
            //       reasons.  We can get away with this because of the of the
            //       non-optionality of the `data` param of `Self::new_node()`
            //       i.e. the node's data field is statically guaranteed to be
            //       in a valid state by the time the node is usable again.
            self.garbage.push_back(idx);
        }
        Ok(())
    }

    /// Add an edge between `self[parent_idx]` and `self[child_idx]`.
    /// The former registers the latter as a child, while the latter
    /// registers the former as a parent.
    pub fn add_edge(&mut self, parent_idx: NodeIdx, child_idx: NodeIdx) {
        self[parent_idx].add_child(child_idx);
        self[child_idx].add_parent(parent_idx);
    }

    /// Insert an edge between `self[parent_idx]` and `self[child_idx]`.
    /// The former registers the latter as a child, while the latter
    /// registers the former as a parent.
    /// The `parent_idx` is inserted in `self[child_idx]` at position
    /// `p`, or appended if `p` is `None`.
    /// Similarly, the `child_idx` is inserted in `self[parent_idx]`
    /// at position `c`, or appended if c` is `None`.
    pub fn insert_edge(
        &mut self,
        (parent_idx, parent_pos): (NodeIdx, Option<usize>),
        (child_idx,   child_pos): (NodeIdx, Option<usize>),
    ) {
        if let Some(child_pos) = child_pos {
            self[parent_idx].insert_child(child_idx, child_pos);
        } else {
            self[parent_idx].add_child(child_idx);
        }
        if let Some(parent_pos) = parent_pos {
            self[child_idx].insert_parent(parent_idx, parent_pos);
        } else {
            self[child_idx].add_parent(parent_idx);
        }
    }

    /// Remove all edges between each `self[parent_idx]` on the one
    /// hand, and each `self[child_idx]` on the other.
    /// Return an error if any of the `|parent_idxs| * |child_idxs|`
    /// edges do not exist.
    pub fn rm_edges(
        &mut self,
        parent_idxs: impl AsRef<[NodeIdx]>,
        child_idxs: impl AsRef<[NodeIdx]>
    ) -> Result<()> {
        for &parent_idx in parent_idxs.as_ref() {
            for &child_idx in child_idxs.as_ref() {
                self.rm_edge(parent_idx, child_idx)?;
            }
        }
        Ok(())
    }

    /// Remove the edge between `self[parent_idx]` and `self[child_idx]`.
    /// Return an error if no such edge exists.
    pub fn rm_edge(
        &mut self,
        parent_idx: NodeIdx,
        child_idx: NodeIdx
    ) -> Result<()> {
        self[parent_idx].remove_child(child_idx)?;
        self[child_idx].remove_parent(parent_idx)?;
        Ok(())
    }

    pub fn self_or_ancestors_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        type Layer = Vec<NodeIdx>;
        let mut layers: Vec<Layer> = vec![Layer::from([node_idx])];
        while let Some(previous) = layers.last() {
            let current: Layer = previous.iter()
                .flat_map(|&idx| self[idx].parents())
                .collect();
            if current.is_empty() {
                break;
            }
            layers.push(current);
        }
        layers.into_iter().flat_map(|layer| layer.into_iter())
    }

    pub fn ancestors_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        self.self_or_ancestors_of(node_idx).filter(move |&aidx| aidx != node_idx)
    }

    #[inline(always)]
    pub fn self_or_siblings_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self[node_idx].parents()
            .flat_map(|pidx| self[pidx].children())
    }

    #[inline(always)]
    pub fn siblings_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self[node_idx].parents()
            .flat_map(|pidx| self[pidx].children())
            .filter(move |&cidx| cidx != node_idx)
    }

    #[inline(always)]
    pub fn children_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self[node_idx].children()
    }

    #[inline(always)]
    pub fn self_or_descendants_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.dfs(node_idx)
    }

    #[inline(always)]
    pub fn descendants_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self.dfs(node_idx)
            .filter(move |&didx| didx != node_idx)
    }

    pub fn dfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        let mut output = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![start_idx];
        while let Some(node_idx) = stack.pop() {
            output.push(node_idx);
            stack.extend(self[node_idx].children().rev());
        }
        output.into_iter()
    }

    pub fn bfs(
        &self,
        start_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        type Layer = Vec<NodeIdx>;
        let mut layers: Vec<Layer> = vec![Layer::from([start_idx])];
        while let Some(previous) = layers.last() {
            let current: Layer = previous.iter()
                .flat_map(|&idx| self[idx].children())
                .collect();
            if current.is_empty() {
                break;
            }
            layers.push(current);
        }
        layers.into_iter().flat_map(|layer| layer.into_iter())
    }

}

impl<D> std::ops::Index<NodeIdx> for Arena<D> {
    type Output = Node<D>;

    fn index(&self, idx: NodeIdx) -> &Self::Output {
        &self.nodes[idx.0]
    }
}

impl<D> std::ops::IndexMut<NodeIdx> for Arena<D> {
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Self::Output {
        &mut self.nodes[idx.0]
    }
}

impl<D: Serialize> Serialize for Arena<D> {
    fn serialize<S: Serializer>(
        &self,
        serializer: S
    ) -> std::result::Result<S::Ok, S::Error> {
        const NUM_FIELDS: usize = 2;
        let mut state = serializer.serialize_struct("Arena", NUM_FIELDS)?;
        state.serialize_field("nodes", &self.nodes)?;
        state.serialize_field("garbage", &self.garbage)?;
        state.end()
    }
}

#[rustfmt::skip]
impl<'de, D> Deserialize<'de> for Arena<D>
where
    D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
{
    fn deserialize<DE: Deserializer<'de>>(
        d: DE
    ) -> std::result::Result<Self, DE::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Nodes,
            Garbage,
        }

        #[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
        struct ArenaVisitor<D>(std::marker::PhantomData<D>);

        impl<'de, D> Visitor<'de> for ArenaVisitor<D>
        where
            D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
        {
            type Value = Arena<D>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("struct Arena<D>")
            }

            fn visit_seq<V>(
                self,
                mut seq: V
            ) -> std::result::Result<Self::Value, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let nodes = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let garbage = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(Arena { nodes, garbage })
            }

            fn visit_map<A>(
                self,
                mut map: A
            ) -> std::result::Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut nodes = None;
                let mut garbage = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Nodes if nodes.is_some() => {
                            return Err(de::Error::duplicate_field("nodes"));
                        }
                        Field::Nodes => { nodes = Some(map.next_value()?); }
                        Field::Garbage if garbage.is_some() => {
                            return Err(de::Error::duplicate_field("garbage"));
                        }
                        Field::Garbage => { garbage = Some(map.next_value()?); }
                    }
                }
                Ok(Arena {
                    nodes: nodes.ok_or_else(|| de::Error::missing_field("nodes"))?,
                    garbage: garbage.ok_or_else(|| de::Error::missing_field("garbage"))?,
                })
            }
        }

        d.deserialize_map(ArenaVisitor(std::marker::PhantomData))
    }
}



// #[cfg(test)]
// mod tests {
//     // use super::*;

//     #[test]
//     fn foo_test() {
//         todo!();
//     }
// }
