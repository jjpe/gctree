//! This module deals with arena allocation.

use crate::{
    error::{Error, Result},
    node::{Edge, Node, NodeCount, NodeIdx},
};
#[cfg(feature = "graphviz")] use crate::graphviz::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use std::collections::VecDeque;
use std::fmt::{self, Debug};

#[derive(Clone, Debug, Hash)]
pub(crate) struct Arena<D, P, C> {
    nodes: Vec<Node<D, P, C>>,
    garbage: VecDeque<NodeIdx>,
}

impl<D, P, C> Default for Arena<D, P, C> {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl<D, P, C> Arena<D, P, C> {
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            nodes: Vec::with_capacity(cap),
            garbage: VecDeque::with_capacity(cap),
        }
    }

    pub fn clear(&mut self) -> Result<()> {
        let non_garbage_nodes: Vec<NodeIdx> = self.nodes.iter()
            .filter(|node| !self.garbage.contains(&node.idx))
            .map(|node| node.idx)
            .collect();
        for idx in non_garbage_nodes {
            self.rm_node(idx)?;
        }
        debug_assert!(self.nodes.iter().all(|n| self.garbage.contains(&n.idx)));
        Ok(())
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

    #[inline]
    pub(crate) fn add_garbage(&mut self, nidx: NodeIdx) {
        self.garbage.push_back(nidx);
    }

    #[must_use]
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
            self.rm_edges(self[idx].parent_idxs().collect::<Vec<_>>(), [idx])?;
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

    /// Traverse the subtree rooted in `self[node_idx]`, but only recycle
    /// nodes that have become orphaned (i.e. parentless) by the time time
    /// they're visited.
    pub fn rm_orphan_node(&mut self, node_idx: NodeIdx) -> Result<()> {
        if node_idx.0 >= self.nodes.len() {
            return Err(Error::NodeNotFound(node_idx));
        }
        for idx in self.dfs(node_idx) {
            if self[idx].has_parents() { continue }
            self.rm_edges([idx], self[idx].child_idxs().collect::<Vec<_>>())?;
            // NOTE: Don't clear the  data field of `self[node_idx]`, for perf
            //       reasons.  We can get away with this because of the
            //       non-optionality of the `data` param of `Self::new_node()`
            //       i.e. the node's data field is statically guaranteed to be
            //       in a valid state by the time the node is usable again.
            self.garbage.push_back(idx);
        }
        Ok(())
    }

    pub fn parent_edge(
        &self,
        src: NodeIdx,
        dst: NodeIdx
    ) -> Option<Edge<NodeIdx, &P>> {
        self[src].parent_edges().find(|e| e.src == src && e.dst == dst)
    }

    pub fn child_edge(
        &self,
        src: NodeIdx,
        dst: NodeIdx
    ) -> Option<Edge<NodeIdx, &C>> {
        self[src].child_edges().find(|e| e.src == src && e.dst == dst)
    }

    /// Add a bidirectional edge between `self[pidx]` and `self[cidx]`.
    /// The former registers the latter as a child, while the latter
    /// registers the former as a parent.
    /// In addition, `pdata` is assigned to the parent edge `cidx -> pidx`
    /// while `cdata` is assigned to the child edge `pidx -> cidx`.
    pub fn add_edge(
        &mut self,
        (pidx, cidx): (NodeIdx, NodeIdx),
        pdata: P,
        cdata: C,
    ) {
        self[pidx].add_child(cidx, cdata);
        self[cidx].add_parent(pidx, pdata);
    }

    /// Insert a bidirectional edge between `self[pidx]` and `self[cidx]`.
    /// The former registers the latter as a child, while the latter
    /// registers the former as a parent.
    /// Parent `(pidx, pdata)` is inserted in `self[cidx].parents` at position
    /// `ppos`, or appended if `ppos` is `None`.
    /// Similarly, child `(cidx, cdata)` is inserted in `self[pidx].children`
    /// at position `cpos`, or appended if cpos` is `None`.
    pub fn insert_edge(
        &mut self,
        (pidx, pdata, ppos): (NodeIdx, P, Option<usize>),
        (cidx, cdata, cpos): (NodeIdx, C, Option<usize>),
    ) {
        if let Some(child_pos) = cpos {
            self[pidx].insert_child(cidx, cdata, child_pos);
        } else {
            self[pidx].add_child(cidx, cdata);
        }
        if let Some(parent_pos) = ppos {
            self[cidx].insert_parent(pidx, pdata, parent_pos);
        } else {
            self[cidx].add_parent(pidx, pdata);
        }
    }

    #[must_use]
    /// Remove all edges between each `self[parent_idx]` on the one
    /// hand, and each `self[child_idx]` on the other.
    /// Return an error if any of the `|parent_idxs| * |child_idxs|`
    /// edges do not exist.
    pub fn rm_edges(
        &mut self,
        parent_idxs: impl IntoIterator<Item = NodeIdx>,
        child_idxs: impl Clone + IntoIterator<Item = NodeIdx>,
    ) -> Result<()> {
        let child_idxs: Vec<NodeIdx> = child_idxs.into_iter().collect();
        for parent_idx in parent_idxs {
            for &child_idx in &child_idxs {
                self.rm_edge(parent_idx, child_idx)?;
            }
        }
        Ok(())
    }

    #[must_use]
    /// Remove the edge between `self[parent_idx]` and `self[child_idx]`.
    /// Return an error if no such edge exists.
    pub fn rm_edge(
        &mut self,
        parent_idx: NodeIdx,
        child_idx: NodeIdx
    ) -> Result<(P, C)> {
        let cdata: C = self[parent_idx].remove_child(child_idx)?;
        let pdata: P = self[child_idx].remove_parent(parent_idx)?;
        Ok((pdata, cdata))
    }

    pub fn self_or_ancestors_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> {
        type Layer = Vec<NodeIdx>;
        let mut layers: Vec<Layer> = vec![Layer::from([node_idx])];
        while let Some(previous) = layers.last() {
            let current: Layer = previous.iter()
                .flat_map(|&idx| self[idx].parent_idxs())
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
        self[node_idx].parent_idxs().flat_map(|pidx| self[pidx].child_idxs())
    }

    #[inline(always)]
    pub fn siblings_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self[node_idx].parent_idxs()
            .flat_map(|pidx| self[pidx].child_idxs())
            .filter(move |&cidx| cidx != node_idx)
    }

    #[inline(always)]
    pub fn children_of(
        &self,
        node_idx: NodeIdx,
    ) -> impl DoubleEndedIterator<Item = NodeIdx> + '_ {
        self[node_idx].child_idxs()
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
            stack.extend(self[node_idx].child_idxs().rev());
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
                .flat_map(|&idx| self[idx].child_idxs())
                .collect();
            if current.is_empty() {
                break;
            }
            layers.push(current);
        }
        layers.into_iter().flat_map(|layer| layer.into_iter())
    }

    #[allow(unused)]
    #[cfg(feature = "graphviz")]
    pub fn to_graphviz_graph(&self, root_idx: NodeIdx) -> DotGraph
    where
        D: std::fmt::Display,
        P: std::fmt::Display,
        C: std::fmt::Display,
    {
        let mut graph = DotGraph {
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

impl<D, P, C> std::ops::Index<NodeIdx> for Arena<D, P, C> {
    type Output = Node<D, P, C>;

    fn index(&self, idx: NodeIdx) -> &Self::Output {
        &self.nodes[idx.0]
    }
}

impl<D, P, C> std::ops::IndexMut<NodeIdx> for Arena<D, P, C> {
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Self::Output {
        &mut self.nodes[idx.0]
    }
}

impl<D, P, C> Serialize for Arena<D, P, C>
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
        let mut state = serializer.serialize_struct("Arena", NUM_FIELDS)?;
        state.serialize_field("nodes", &self.nodes)?;
        state.serialize_field("garbage", &self.garbage)?;
        state.end()
    }
}

#[rustfmt::skip]
impl<'de, D, P, C> Deserialize<'de> for Arena<D, P, C>
where
    D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
    P: Clone + Debug + Default + PartialEq + Deserialize<'de>,
    C: Clone + Debug + Default + PartialEq + Deserialize<'de>,
{
    fn deserialize<DE: Deserializer<'de>>(
        d: DE
    ) -> std::result::Result<Self, DE::Error> {
        #[derive(serde_derive::Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Nodes,
            Garbage,
        }

        #[rustfmt::skip]
        #[derive(
            Clone,
            Debug,
            PartialEq,
            serde_derive::Deserialize,
            serde_derive::Serialize
        )]
        struct ArenaVisitor<D, P, C>(
            std::marker::PhantomData<D>,
            std::marker::PhantomData<P>,
            std::marker::PhantomData<C>,
        );

        impl<'de, D, P, C> Visitor<'de> for ArenaVisitor<D, P, C>
        where
            D: Clone + Debug + Default + PartialEq + Deserialize<'de>,
            P: Clone + Debug + Default + PartialEq + Deserialize<'de>,
            C: Clone + Debug + Default + PartialEq + Deserialize<'de>,
        {
            type Value = Arena<D, P, C>;

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

        d.deserialize_map(ArenaVisitor(
            std::marker::PhantomData,
            std::marker::PhantomData,
            std::marker::PhantomData,
        ))
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
