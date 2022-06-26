use crate::NodeIdx;
use serde_derive::{Deserialize, Serialize};

pub type TreeResult<T> = Result<T, TreeError>;

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
    deltoid_derive::Delta,
    displaydoc::Display,
    thiserror::Error,
)]
pub enum TreeError {
    /// Expected `tree[node_idx]` to have a parent node
    ParentNotFound { node_idx: NodeIdx },
}
