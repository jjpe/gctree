use crate::NodeIdx;
use serde_derive::{Deserialize, Serialize};

pub type TreeResult<T> = Result<T, TreeError>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(Deserialize, Serialize, deltoid_derive::Delta)]
pub enum TreeError {
    /// Couldn't find a node for the provided `NodeIdx`.
    NoNodeForNodeIdx { idx: NodeIdx },
}
