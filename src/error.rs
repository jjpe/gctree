//!

use crate::node::NodeIdx;

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
    /// GraphViz generation error: {0}
    GraphvizParse(String),

    /// An error originating in a Gss instance: {0:?}
    GssError(#[from] crate::gss::Error),

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
    NodeNotFound(NodeIdx)
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(ioe::IoError::from(err))
    }
}

impl deltoid::Core for Error {
    type Delta = ErrorDelta;
}

impl deltoid::Apply for Error {
    fn apply(&self, _delta: Self::Delta) -> deltoid::DeltaResult<Self> {
        unimplemented!("impl deltoid::Apply for Error") // TODO
    }
}

impl deltoid::Delta for Error {
    fn delta(&self, _rhs: &Self) -> deltoid::DeltaResult<Self::Delta> {
        unimplemented!("impl deltoid::Delta for Error") // TODO
    }
}

impl deltoid::FromDelta for Error {
    fn from_delta(_delta: Self::Delta) -> deltoid::DeltaResult<Self> {
        unimplemented!("impl deltoid::FromDelta for Error") // TODO
    }
}

impl deltoid::IntoDelta for Error {
    fn into_delta(self) -> deltoid::DeltaResult<Self::Delta> {
        unimplemented!("impl deltoid::INtoDelta for Error") // TODO
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
