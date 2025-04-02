//! This module defines a low-level and cache-friendly tree
//! datastructure that can be newtyped for higher-level trees.

mod arena;
#[cfg(feature = "graphviz")] pub mod graphviz;
mod node;
pub mod error;
pub mod forest;
pub mod gss;

pub use crate::{
    arena::Phase,
    error::{Error, Result},
    gss::{Gss, StackIdx},
    node::{Edge, Node, NodeCount, NodeIdx},
    forest::{Forest, ForestIdx},
};
