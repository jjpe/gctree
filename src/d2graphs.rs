use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Output};

#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct D2Graph {
    pub direction: D2Direction,
    pub nodes: BTreeSet<D2Node>,
    pub edges: BTreeSet<D2Edge>,
}

impl D2Graph {
    pub fn add_node(&mut self, node: D2Node) {
        self.nodes.insert(node);
    }

    pub fn add_edge(&mut self, edge: D2Edge) {
        self.edges.insert(edge);
    }

    /// Render `self` as `d2` diagram source code.
    pub fn to_d2_graph(&self) -> String {
        let mut buf = String::with_capacity(1024);
        buf.push_str(&match self.direction {
            D2Direction::Up    => format!("direction: up\n"),
            D2Direction::Down  => format!("direction: down\n"),
            D2Direction::Left  => format!("direction: left\n"),
            D2Direction::Right => format!("direction: right\n"),
        });
        buf.push_str("\n");
        for D2Node { id: D2NodeId(id), text } in &self.nodes {
            buf.push_str(&format!("{id}: {text}\n"));
        }
        buf.push_str("\n");
        for D2Edge {
            src: D2NodeId(src),
            dst: D2NodeId(dst),
            props: D2EdgeProps { label, style }
        } in &self.edges {
            buf.push_str(&format!("{src}->{dst}: {{\n"));

            // TODO: props
            if let Some(label) = label.as_ref() {
                buf.push_str(&format!("    label: {label}\n"));
            }
            if let Some(D2EdgeStyle { stroke: Some(stroke) }) = style.as_ref() {
                buf.push_str(&format!("    style.stroke: {stroke}\n"));
            }

            buf.push_str(&format!("}}\n"));
        }
        buf
    }

    /// Generate the following files for `self`:
    /// - `{dirpath}/{file_stem}.d2`
    /// - `{dirpath}/{file_stem}.svg`
    pub fn generate_files(
        &self,
        dirpath: &Path,
        file_stem: &str,
    ) -> std::io::Result<Output> {
        let  d2_filepath = dirpath.join(format!("{file_stem}.d2"));
        let svg_filepath = dirpath.join(format!("{file_stem}.svg"));
        std::fs::write(&d2_filepath, self.to_d2_graph())?;
        Command::new("d2")
            .args([
                "--layout".to_string(),
                "elk".to_string(),
                format!("{}", d2_filepath.display()),
                format!("{}", svg_filepath.display()),
            ])
            .output()
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum D2Direction {
    Up,
    #[default] Down,
    Left,
    Right
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct D2Node {
    pub id: D2NodeId,
    pub text: String,
}

#[rustfmt::skip]
#[derive(
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    derive_more::Deref,
    derive_more::From,
)]
pub struct D2NodeId(usize);

impl From<crate::node::NodeIdx> for D2NodeId {
    fn from(idx: crate::node::NodeIdx) -> Self {
        Self(idx.0)
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct D2Edge {
    pub src: D2NodeId,
    pub dst: D2NodeId,
    pub props: D2EdgeProps,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct D2EdgeProps {
    pub label: Option<String>,
    pub style: Option<D2EdgeStyle>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct D2EdgeStyle {
    pub stroke: Option<String>,
}
