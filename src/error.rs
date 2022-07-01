use crate::node_idx::NodeIdx;
use serde_derive::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Deserialize,
    Serialize,
    displaydoc::Display,
    thiserror::Error,
)]
pub enum Error {
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
