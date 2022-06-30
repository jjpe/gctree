use crate::NodeIdx;
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
    /// Expected `tree[node_idx]` to have a parent node
    ParentNotFound { node_idx: NodeIdx },
}
