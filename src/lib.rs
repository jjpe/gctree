//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.
#![forbid(unsafe_code)]

mod arena;
mod node;
pub mod error;
pub mod forest;
pub mod gss;

pub use crate::{
    gss::{Gss, StackIdx},
    node::{Edge, Node, NodeCount, NodeIdx},
    forest::{Forest, ForestIdx},
};
