//!

use crate::node::NodeIdx;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(serde::Deserialize, serde::Serialize)]
#[derive(displaydoc::Display, thiserror::Error)]
pub enum Error {
    /// I/O error: {0}
    Io(ioe::IoError),
    /// GraphViz generation error: {0}
    GraphvizParse(String),

    /// Detected a cycle: {path:?}
    CycleDetected { path: Vec<NodeIdx> },
    /// Expected Node {0} to be a branch node
    ExpectedBranchNode(NodeIdx),
    /// Expected Node {0} to be a leaf node
    ExpectedLeafNode(NodeIdx),
    /// Expected Node {0} to be a root node
    ExpectedRootNode(NodeIdx),

    /// Expected `tree[node_idx]` to have `tree[parent_idx]` as a parent node
    ParentNotFound { node_idx: NodeIdx, parent_idx: Option<NodeIdx> },
    /// Expected `tree[node_idx]` to have `tree[child_idx]` as a child node
    ChildNotFound { node_idx: NodeIdx, child_idx: NodeIdx },
    /// No node found with NodeIdx `{0}`
    NodeNotFound(NodeIdx),

    /// Expected a single parent node, but got {got} parents
    ExpectedSingleParent { fidx: NodeIdx, got: usize },

    /// Node {0} is not a top-of-gss
    ExpectedGssTop(NodeIdx),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(ioe::IoError::from(err))
    }
}
