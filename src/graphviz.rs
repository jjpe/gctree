//! A module to make Graphviz graph definitions type-safe
#![allow(unused_imports)] // TODO: remove

use crate::{error::{Error, Result}, node::NodeIdx};
use dot_generator::*;
use dot_structures::*;
use graphviz_rust::{
    attributes::*,
    cmd::{CommandArg, Format},
    exec, parse,
    printer::{DotPrinter, PrinterContext},
};
use std::path::Path;


#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DotGraph {
    /// The background color
    pub bgcolor: Option<String>,
    /// The rank direction
    pub rankdir: Option<DotRankDir>,
    pub stmts: Vec<DotStmt>,
}

impl Default for DotGraph {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

impl DotGraph {
    const DEFAULT_BGCOLOR: &str = "#3c3c3c"; // dark gray

    pub fn new() -> Self {
        Self {
            bgcolor: Some(Self::DEFAULT_BGCOLOR.to_string()),
            rankdir: Some(DotRankDir::default()),
            stmts: vec![],
        }
    }

    pub fn add(&mut self, stmt: impl Into<DotStmt>) {
        self.stmts.push(stmt.into());
    }

    /// Write `self` to `{dirpath}/{stem}.svg`.
    pub fn write_to_svg(&self, dirpath: &Path, stem: &str) -> Result<()> {
        let dot = &format!("{self}");
        println!("dot:\n{}", dot.replace("\nstate", "\\nstate")); // HACK
        let g: Graph = parse(dot).map_err(Error::GraphvizParse)?;
        let mut pctx = PrinterContext::default();
        let svg: String = exec(g, &mut pctx, vec![Format::Svg.into()])?;
        let svg_filepath = dirpath.join(format!("{stem}.svg"));
        std::fs::write(&svg_filepath, svg)?;
        Ok(())
    }
}

impl std::fmt::Display for DotGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "digraph {{")?;
        if let Some(bg) = &self.bgcolor {
            writeln!(f, "  bgcolor=\"{bg}\";")?;
        }
        if let Some(rankdir) = &self.rankdir {
            writeln!(f, "  rankdir={rankdir};")?;
        }
        for stmt in &self.stmts {
            writeln!(f, "  {stmt}")?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}



#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DotSubGraph {
    pub is_cluster: bool,
    /// If `self.is_cluster` is `true`, then a background color can be provided
    pub bgcolor: Option<String>,
    pub stmts: Vec<DotStmt>,
}

impl DotSubGraph {
    pub fn add(&mut self, stmt: impl Into<DotStmt>) {
        self.stmts.push(stmt.into());
    }
}

impl std::fmt::Display for DotSubGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cluster = if self.is_cluster { "cluster" } else { "" };
        write!(f, "subgraph {cluster} {{")?;
        if let Some(bg) = &self.bgcolor {
            write!(f, "  bgcolor=\"{bg}\";")?;
        }
        for stmt in &self.stmts {
            write!(f, "  {stmt};")?;
        }
        write!(f, "}}")?;
        Ok(())
    }
}



#[rustfmt::skip]
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::From
)]
pub enum DotStmt {
    Node(DotNode),
    Edge(DotEdge),
    Subgraph(DotSubGraph),
    Overlap(DotOverlap),
    RankSeparation(usize),
    Rank { rank: DotRankStmt, nodes: Vec<NodeIdx> },
}

impl std::fmt::Display for DotStmt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Node(node) => write!(f, "{node};")?,
            Self::Edge(edge) => write!(f, "{edge};")?,
            Self::Subgraph(graph) => write!(f, "{graph}")?,
            Self::Overlap(DotOverlap::Scale) => write!(f, "overlap=scale;")?,
            Self::RankSeparation(sep) => write!(f, "ranksep={sep};")?,
            Self::Rank { rank, nodes } => {
                write!(f, "rank={rank};")?;
                for node in nodes.iter() {
                    write!(f, " {node};")?;
                }
            },
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, displaydoc::Display)]
/// {idx} [{attrs}]
pub struct DotNode {
    pub idx: NodeIdx,
    pub attrs: DotAttrs,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, displaydoc::Display)]
/// {src} -> {dst} [{attrs}]
pub struct DotEdge {
    pub src: NodeIdx,
    pub dst: NodeIdx,
    pub attrs: DotAttrs,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DotOverlap {
    Scale
}

#[rustfmt::skip]
#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    displaydoc::Display,
)]
pub enum DotRankDir {
    #[default]
    /// TB
    TopToBottom,
    /// BT
    BottomToTop,
    /// LR
    LeftToRight,
    /// RL
    RightToLeft,
}

#[rustfmt::skip]
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    displaydoc::Display
)]
pub enum DotRankStmt {
    /// min
    Min,
    /// max
    Max,
    /// same
    Same,
}

#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DotAttrs {
    /// A label that will be displayed within a pair of double quotes.
    pub label: Option<String>,
    /// A shape e.g. "square", "circle", "triangle" etc.
    pub shape: Option<String>,
    /// A color name or hex format e.g. "darkgreen", "#3c3c3c", etc.
    pub color: Option<String>,
    /// A color name or hex format e.g. "darkgreen", "#3c3c3c", etc.
    pub fontcolor: Option<String>,
    /// [edge-only] If `true`, draw a trace line from an
    /// edge label to the edge itself.
    pub decorate: Option<()>,
    /// The size of the font used for label text.
    pub fontsize: Option<usize>,
    /// [node-only] Minimum height
    pub height: Option<usize>,
    /// [node-only] Minimum width
    pub width: Option<usize>,
    /// [node-only] Node style
    pub style: Option<DotStyle>,
    /// [node-only] Sets the fill color when `style=filled`.  If not
    /// specified, the fillcolor when `style=filled` defaults to be
    /// the same as the outline color.
    pub fillcolor: Option<String>,
}

impl std::fmt::Display for DotAttrs {
    #[allow(unused_assignments)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut n = 0;
        if let Some(label) = &self.label {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "label=\"{label}\"")?;
            n += 1;
        }
        if let Some(shape) = &self.shape {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "shape={shape}")?;
            n += 1;
        }
        if let Some(color) = &self.color {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "color=\"{color}\"")?;
            n += 1;
        }
        if let Some(fontcolor) = &self.fontcolor {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "fontcolor={fontcolor}")?;
            n += 1;
        }
        if let Some(()) = &self.decorate {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "decorate=true")?;
            n += 1;
        }
        if let Some(fontsize) = &self.fontsize {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "fontsize={fontsize}")?;
            n += 1;
        }
        if let Some(height) = &self.height {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "height={height}")?;
            n += 1;
        }
        if let Some(width) = &self.width {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "width={width}")?;
            n += 1;
        }
        if let Some(style) = &self.style {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "style={style}")?;
            n += 1;
        }
        if let Some(fillcolor) = &self.fillcolor {
            if n > 0 { write!(f, ", ")?; }
            write!(f, "fillcolor={fillcolor}")?;
            n += 1;
        }
        Ok(())
    }
}

#[rustfmt::skip]
#[derive(
    Default,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    displaydoc::Display
)]
pub enum DotStyle {
    /// filled
    Filled,
    #[default]
    /// solid
    Solid,
    /// dashed
    Dashed,
    /// dotted
    Dotted,
    /// bold
    Bold,
    /// invis
    Invis,
}


#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> DotGraph {
        DotGraph {
            stmts: vec![
                DotStmt::Node(DotNode {
                    idx: NodeIdx::from(0), // A
                    attrs: DotAttrs {
                        label: "flippity\nfloppity".to_string().into(),
                        color: "darkred".to_string().into(),
                        ..DotAttrs::default()
                    },
                }),
                DotStmt::Edge(DotEdge {
                    src: NodeIdx::from(0), // A
                    dst: NodeIdx::from(1), // B
                    attrs: DotAttrs {
                        label: "hi tharr\n\"foo\"".to_string().into(),
                        fontcolor: "deepskyblue".to_string().into(),
                        ..DotAttrs::default()
                    },
                }),
                DotStmt::Edge(DotEdge {
                    src: NodeIdx::from(0), // A
                    dst: NodeIdx::from(2), // C
                    attrs: DotAttrs::default(),
                }),
                DotStmt::Edge(DotEdge {
                    src: NodeIdx::from(2), // C
                    dst: NodeIdx::from(3), // D
                    attrs: DotAttrs::default(),
                }),
                DotStmt::Edge(DotEdge {
                    src: NodeIdx::from(24), // X
                    dst: NodeIdx::from(25), // Y
                    attrs: DotAttrs::default(),
                }),
                DotStmt::Edge(DotEdge {
                    src: NodeIdx::from(3), // D
                    dst: NodeIdx::from(4), // E
                    attrs: DotAttrs::default(),
                }),

                // TODO

            ],
            ..DotGraph::default()
        }
    }
}
