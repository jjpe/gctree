//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.
#![forbid(unsafe_code)]

pub mod dag;
pub mod error;
pub mod node_count;
pub mod node_idx;
pub mod tree;

#[rustfmt::skip]
pub use crate::{
    node_count::NodeCount,
    node_idx::NodeIdx
};
