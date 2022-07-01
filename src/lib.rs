//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.
#![forbid(unsafe_code)]

mod error;
mod node_count;
mod node_idx;
mod tree;

pub use crate::{node_count::NodeCount, node_idx::NodeIdx};
