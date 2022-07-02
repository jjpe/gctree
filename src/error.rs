//!

use crate::node_idx::NodeIdx;

pub type Result<T> = std::result::Result<T, Error>;

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
    /// I/O error: {0}
    Io(ioe::IoError),

    /// Detected a cycle: {path:?}
    CycleDetected { path: Vec<NodeIdx> },
    /// Expected Node {0} to be a branch node
    NotABranch(NodeIdx),
    /// Expected Node {0} to be a leaf node
    NotALeaf(NodeIdx),
    /// Expected Node {0} to be a root node
    NotARoot(NodeIdx),
    /// Expected `tree[node_idx]` to have a parent node
    ParentNotFound { node_idx: NodeIdx },
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(ioe::IoError::from(err))
    }
}
