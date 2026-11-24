//! Native Mermaid-compatible diagram rendering for Daftar.
//!
//! No JavaScript, browser or WebView is involved. Mermaid source is parsed by native Rust
//! parsers and laid out into a vector scene that is rasterised by tiny-skia (PNG export) or
//! used as a texture in the egui viewer.
//!
//! Supported diagram types: flowchart/graph, sequenceDiagram, classDiagram, stateDiagram,
//! stateDiagram-v2, erDiagram, gantt, pie, mindmap and journey. Unsupported Mermaid directives
//! produce an explicit error instead of a fake rendering.

pub mod class_diagram;
pub mod er;
pub mod flowchart;
pub mod gantt;
pub mod graph;
pub mod journey;
pub mod mindmap;
pub mod pie;
pub mod scene;
pub mod sequence;
pub mod state_diagram;

use std::fmt;

use daftar_text::TextEngine;

pub use graph::{GraphDirection, GraphEdge, GraphLayout, GraphNode, NodeShape};
pub use scene::{
    Label, PathOp, Rgba8, Scene, SceneItem, ScenePath, SceneStroke, SceneText, TextFactory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagramKind {
    Flowchart,
    Sequence,
    Class,
    State,
    Er,
    Gantt,
    Pie,
    Mindmap,
    Journey,
}

impl DiagramKind {
    pub fn label(self) -> &'static str {
        match self {
            DiagramKind::Flowchart => "Flowchart",
            DiagramKind::Sequence => "Sequence diagram",
            DiagramKind::Class => "Class diagram",
            DiagramKind::State => "State diagram",
            DiagramKind::Er => "Entity relationship diagram",
            DiagramKind::Gantt => "Gantt chart",
            DiagramKind::Pie => "Pie chart",
            DiagramKind::Mindmap => "Mindmap",
            DiagramKind::Journey => "Journey",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DiagramTheme {
    pub dark: bool,
    pub background: Rgba8,
    pub surface: Rgba8,
    pub node_fill: Rgba8,
    pub node_stroke: Rgba8,
    pub text: Rgba8,
    pub muted: Rgba8,
    pub accent: Rgba8,
    pub line: Rgba8,
    pub note_fill: Rgba8,
    pub note_stroke: Rgba8,
    pub group_fill: Rgba8,
    pub group_stroke: Rgba8,
    pub grid: Rgba8,
    pub pie_palette: Vec<Rgba8>,
}

impl DiagramTheme {
    pub fn light() -> Self {
        Self {
            dark: false,
            background: Rgba8::from_hex(0xFFFFFF),
            surface: Rgba8::from_hex(0xFFFFFF),
            node_fill: Rgba8::from_hex(0xF8F8F8),
            node_stroke: Rgba8::from_hex(0xC6C6C6),
            text: Rgba8::from_hex(0x2C2C2C),
            muted: Rgba8::from_hex(0x6E6E6E),
            accent: Rgba8::from_hex(0x2680EB),
            line: Rgba8::from_hex(0x8A8A8A),
            note_fill: Rgba8::from_hex(0xFFF7D6),
            note_stroke: Rgba8::from_hex(0xD9C27A),
            group_fill: Rgba8::rgba(0xF5, 0xF5, 0xF5, 255),
            group_stroke: Rgba8::from_hex(0xD0D0D0),
            grid: Rgba8::from_hex(0xEAEAEA),
            pie_palette: vec![
                Rgba8::from_hex(0x2680EB),
                Rgba8::from_hex(0xE68619),
                Rgba8::from_hex(0x2D9D78),
                Rgba8::from_hex(0xE34850),
                Rgba8::from_hex(0x9256D9),
                Rgba8::from_hex(0x0FB5AE),
                Rgba8::from_hex(0xD6A100),
                Rgba8::from_hex(0x7A5AF8),
            ],
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            background: Rgba8::from_hex(0x1E1E1E),
            surface: Rgba8::from_hex(0x1E1E1E),
            node_fill: Rgba8::from_hex(0x2A2A2A),
            node_stroke: Rgba8::from_hex(0x5A5A5A),
            text: Rgba8::from_hex(0xE8E8E8),
            muted: Rgba8::from_hex(0xA0A0A0),
            accent: Rgba8::from_hex(0x6BB8FF),
            line: Rgba8::from_hex(0x9A9A9A),
            note_fill: Rgba8::from_hex(0x3A3524),
            note_stroke: Rgba8::from_hex(0x7A6A3A),
            group_fill: Rgba8::rgba(0x24, 0x24, 0x24, 255),
            group_stroke: Rgba8::from_hex(0x444444),
            grid: Rgba8::from_hex(0x333333),
            pie_palette: vec![
                Rgba8::from_hex(0x6BB8FF),
                Rgba8::from_hex(0xF0A04B),
                Rgba8::from_hex(0x55C9A0),
                Rgba8::from_hex(0xF07A80),
                Rgba8::from_hex(0xB18AF0),
                Rgba8::from_hex(0x3ED6D0),
                Rgba8::from_hex(0xE8C04B),
                Rgba8::from_hex(0x9B87F5),
            ],
        }
    }

    pub fn for_dark_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltDiagram {
    pub scene: Scene,
    pub kind: DiagramKind,
    pub width: f32,
    pub height: f32,
    pub warnings: Vec<String>,
}

impl BuiltDiagram {
    pub fn encode_png(&self, engine: &mut TextEngine, scale: f32) -> Result<Vec<u8>, String> {
        self.scene.encode_png(engine, scale, Some(self.scene.background.unwrap_or(Rgba8::from_hex(0xFFFFFF))))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramError {
    pub message: String,
    pub line: Option<usize>,
}

impl DiagramError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }

    pub fn at(line: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: Some(line),
        }
    }
}

impl fmt::Display for DiagramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "Line {line}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for DiagramError {}

/// Parse and lay out Mermaid source.
pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let trimmed = source.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(DiagramError::new("The diagram block is empty."));
    }
    let first = trimmed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("%%"))
        .unwrap_or("");
    let keyword = first.split_whitespace().next().unwrap_or("");
    let lower = keyword.to_ascii_lowercase();
    let mut diagram = match lower.as_str() {
        "graph" | "flowchart" => flowchart::build(trimmed, theme, engine)?,
        "sequencediagram" => sequence::build(trimmed, theme, engine)?,
        "classdiagram" => class_diagram::build(trimmed, theme, engine)?,
        "statediagram" | "statediagram-v2" => state_diagram::build(trimmed, theme, engine)?,
        "erdiagram" => er::build(trimmed, theme, engine)?,
        "gantt" => gantt::build(trimmed, theme, engine)?,
        "pie" => pie::build(trimmed, theme, engine)?,
        "mindmap" => mindmap::build(trimmed, theme, engine)?,
        "journey" | "userjourney" => journey::build(trimmed, theme, engine)?,
        _ => {
            return Err(DiagramError::at(
                1,
                format!(
                    "Unsupported Mermaid directive `{keyword}`. Supported: graph/flowchart, \
                     sequenceDiagram, classDiagram, stateDiagram-v2, erDiagram, gantt, pie, \
                     mindmap, journey."
                ),
            ));
        }
    };
    diagram.width = diagram.scene.width;
    diagram.height = diagram.scene.height;
    Ok(diagram)
}

/// Add a standard margin around freshly built content.
pub(crate) fn padded_scene(
    theme: &DiagramTheme,
    content_width: f32,
    content_height: f32,
    margin: f32,
) -> Scene {
    Scene::new(
        content_width.max(1.0) + margin * 2.0,
        content_height.max(1.0) + margin * 2.0,
        Some(theme.background),
    )
}

/// Strip Mermaid `<br>` variants into newlines and remove unsupported inline HTML tags.
pub(crate) fn clean_label(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(start) = rest.find('<') {
            out.push_str(&rest[..start]);
            let after = &rest[start..];
            let lower = after.to_ascii_lowercase();
            if lower.starts_with("<br>") {
                out.push('\n');
                rest = &after[4..];
                continue;
            }
            if lower.starts_with("<br/>") {
                out.push('\n');
                rest = &after[5..];
                continue;
            }
            if lower.starts_with("<br />") {
                out.push('\n');
                rest = &after[6..];
                continue;
            }
            if let Some(end) = after.find('>') {
                rest = &after[end + 1..];
                continue;
            }
            out.push_str(after);
            rest = "";
        } else {
            out.push_str(rest);
            break;
        }
    }
    out.trim().to_string()
}

/// Extract a quoted or bare token after a marker.
pub(crate) fn unquote(text: &str) -> String {
    let t = text.trim();
    if (t.starts_with('"') && t.ends_with('"') && t.len() >= 2)
        || (t.starts_with('\'') && t.ends_with('\'') && t.len() >= 2)
    {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_directive_errors() {
        let mut engine = TextEngine::new(None, "en-US");
        let err = build("gitGraph\n commit", &DiagramTheme::light(), &mut engine).unwrap_err();
        assert!(err.message.contains("Unsupported"));
    }

    #[test]
    fn empty_errors() {
        let mut engine = TextEngine::new(None, "en-US");
        assert!(build("   ", &DiagramTheme::light(), &mut engine).is_err());
    }

    #[test]
    fn clean_label_converts_breaks() {
        assert_eq!(clean_label("a<br/>b"), "a\nb");
        assert_eq!(clean_label("<b>bold</b>"), "bold");
    }

    #[test]
    fn themes_are_distinct() {
        assert_ne!(DiagramTheme::light().background, DiagramTheme::dark().background);
        assert!(DiagramTheme::dark().dark);
    }
}