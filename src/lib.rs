//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.
#![forbid(unsafe_code)]

mod arena;
mod node;
pub mod dag;
pub mod error;
pub mod forest;

pub use crate::{
    node::{Node, NodeCount, NodeIdx},
    forest::Forest,
};
