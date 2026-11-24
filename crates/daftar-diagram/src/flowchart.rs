//! Flowchart / graph parser and renderer.

use std::collections::HashMap;

use daftar_core::settings::TextAlign;

use crate::graph::{
    GraphDirection, GraphEdge, GraphGroup, GraphNode, NodeShape, draw as draw_graph, layout,
};
use crate::scene::TextFactory;
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene, unquote};

#[derive(Debug, Clone)]
struct ParsedNode {
    id: String,
    label: String,
    shape: NodeShape,
    group: Option<usize>,
}

#[derive(Debug, Default, Clone)]
struct EdgeStyle {
    label: Option<String>,
    dashed: bool,
    thick: bool,
    bidir: bool,
    arrow_end: bool,
    arrow_start: bool,
}

#[derive(Debug, Clone)]
struct ParsedEdge {
    from: usize,
    to: usize,
    style: EdgeStyle,
}

#[derive(Debug, Clone)]
struct ParsedGroup {
    title: String,
    parent: Option<usize>,
    members: Vec<usize>,
}

struct Parser {
    nodes: Vec<ParsedNode>,
    index: HashMap<String, usize>,
    edges: Vec<ParsedEdge>,
    groups: Vec<ParsedGroup>,
    group_stack: Vec<usize>,
    warnings: Vec<String>,
    direction: GraphDirection,
}

impl Parser {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            index: HashMap::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            group_stack: Vec::new(),
            warnings: Vec::new(),
            direction: GraphDirection::TopBottom,
        }
    }

    fn node(&mut self, id: &str, label: Option<&str>, shape: Option<NodeShape>) -> usize {
        if let Some(existing) = self.index.get(id).copied() {
            let node = &mut self.nodes[existing];
            if let Some(label) = label {
                if !label.trim().is_empty() {
                    node.label = label.to_string();
                }
            }
            if let Some(shape) = shape {
                node.shape = shape;
            }
            return existing;
        }
        let idx = self.nodes.len();
        self.nodes.push(ParsedNode {
            id: id.to_string(),
            label: label
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| id.to_string()),
            shape: shape.unwrap_or(NodeShape::Rect),
            group: self.group_stack.last().copied(),
        });
        self.index.insert(id.to_string(), idx);
        if let Some(group) = self.group_stack.last().copied() {
            if let Some(group) = self.groups.get_mut(group) {
                group.members.push(idx);
            }
        }
        idx
    }

    fn parse_line(&mut self, line: &str, line_no: usize, is_header: bool) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            return;
        }
        if is_header {
            let mut parts = trimmed.split_whitespace();
            let _keyword = parts.next();
            if let Some(dir) = parts.next().and_then(GraphDirection::from_keyword) {
                self.direction = dir;
            }
            return;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("subgraph") {
            let rest = trimmed["subgraph".len()..].trim();
            let title = if let Some(open) = rest.find('[') {
                if let Some(close) = rest.rfind(']') {
                    unquote(&rest[open + 1..close])
                } else {
                    rest.to_string()
                }
            } else {
                rest.to_string()
            };
            let title = if title.is_empty() {
                format!("Group {}", self.groups.len() + 1)
            } else {
                clean_label(&title)
            };
            let parent = self.group_stack.last().copied();
            let idx = self.groups.len();
            self.groups.push(ParsedGroup {
                title,
                parent,
                members: Vec::new(),
            });
            self.group_stack.push(idx);
            return;
        }
        if lower == "end" {
            self.group_stack.pop();
            return;
        }
        for prefix in [
            "classdef",
            "class ",
            "style ",
            "linkstyle",
            "click ",
            "accTitle",
            "accDescr",
            "direction ",
        ] {
            if lower.starts_with(&prefix.to_ascii_lowercase()) {
                return;
            }
        }
        if lower.starts_with("graph ") || lower.starts_with("flowchart ") {
            return;
        }
        if !self.parse_statement(trimmed) && self.warnings.len() < 12 {
            self.warnings
                .push(format!("Line {line_no}: could not parse `{trimmed}`."));
        }
    }

    fn parse_statement(&mut self, statement: &str) -> bool {
        let bytes = statement.as_bytes();
        let mut i = 0usize;
        let mut previous: Option<usize> = None;
        let mut pending: Option<EdgeStyle> = None;
        let mut consumed_any = false;

        while i < bytes.len() {
            while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b';') {
                i += 1;
            }
            if i >= bytes.len() {
                break;
            }
            if previous.is_some() {
                if let Some((style, consumed)) = parse_edge_at(statement, i) {
                    pending = Some(style);
                    i += consumed;
                    continue;
                }
            }
            let Some((id, label, shape, consumed)) = parse_node_at(statement, i) else {
                return consumed_any;
            };
            let idx = self.node(&id, label.as_deref(), shape);
            consumed_any = true;
            i += consumed;
            if let Some(prev) = previous {
                let style = pending.take().unwrap_or_default();
                if style.label.is_none() {
                    // `A -->|x| B` stores the label on the arrow; handled by parse_edge_at.
                }
                self.edges.push(ParsedEdge {
                    from: prev,
                    to: idx,
                    style,
                });
            }
            previous = Some(idx);
            pending = None;
        }
        consumed_any
    }
}

/// Parse one node reference at byte `start`.
fn parse_node_at(
    s: &str,
    start: usize,
) -> Option<(String, Option<String>, Option<NodeShape>, usize)> {
    let bytes = s.as_bytes();
    let mut i = start;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    if matches!(bytes[i], b'|' | b'&' | b'[' | b']' | b'(' | b')' | b'{' | b'}' | b'>') {
        return None;
    }
    let id_start = i;
    let mut id_end = i;
    for (offset, c) in s[i..].char_indices() {
        if c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/') {
            // Do not swallow an arrow operator that starts with '-'.
            if c == '-' && id_end > id_start {
                let rest = &s[i + offset..];
                if rest.starts_with("--") || rest.starts_with("->") || rest.starts_with("-.") || rest.starts_with("-=") {
                    break;
                }
            }
            id_end = i + offset + c.len_utf8();
        } else {
            break;
        }
    }
    if id_end == id_start {
        return None;
    }
    let id = s[id_start..id_end].to_string();
    let mut after_id = id_end;
    while after_id < s.len() {
        let c = s[after_id..].chars().next().unwrap_or(' ');
        if c.is_whitespace() {
            after_id += c.len_utf8();
        } else {
            break;
        }
    }

    let _ = id;
    // Shape groups, longest delimiters first.
    let shape_specs: &[(&str, &str, NodeShape)] = &[
        ("([", "])", NodeShape::Stadium),
        ("((", "))", NodeShape::Circle),
        ("{{", "}}", NodeShape::Hexagon),
        ("[(", ")]", NodeShape::Cylinder),
        ("[[", "]]", NodeShape::Subroutine),
        ("[/", "/]", NodeShape::Parallelogram),
        ("[\\", "\\]", NodeShape::Parallelogram),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Rounded),
        ("{", "}", NodeShape::Diamond),
        (">", "]", NodeShape::Asymmetric),
    ];
    for (open, close, shape) in shape_specs {
        if !s[after_id..].starts_with(open) {
            continue;
        }
        let content_start = after_id + open.len();
        let rest = &s[content_start..];
        let label = if let Some(stripped) = rest.trim_start().strip_prefix('"') {
            let quote_offset = rest.len() - rest.trim_start().len();
            if let Some(end) = stripped.find('"') {
                let value = stripped[..end].to_string();
                // Find the closing delimiter after the quoted label.
                let after_quote = content_start + quote_offset + 1 + end + 1;
                if let Some(close_pos) = s[after_quote..].find(close) {
                    let consumed = after_quote + close_pos + close.len() - start;
                    return Some((
                        id,
                        Some(clean_label(&value)),
                        Some(*shape),
                        consumed,
                    ));
                }
            }
            rest.to_string()
        } else {
            rest.to_string()
        };
        if let Some(close_pos) = label.find(close) {
            let raw = label[..close_pos].to_string();
            let consumed = content_start + close_pos + close.len() - start;
            let label = clean_label(&unquote(&raw));
            return Some((id, Some(label), Some(*shape), consumed));
        }
        return None;
    }

    // A bare node reference.
    Some((id, None, None, after_id.saturating_sub(start)))
}

/// Parse an edge operator (and optional label) at `start`.
fn parse_edge_at(s: &str, start: usize) -> Option<(EdgeStyle, usize)> {
    let rest = &s[start..];
    // Direct operators, longest first. Some allow a `|label|` suffix.
    const DIRECT: &[(&str, bool, bool, bool, bool, bool)] = &[
        // (operator, dashed, thick, arrow_end, arrow_start, bidirectional)
        ("<-->", false, false, true, true, true),
        ("<---", false, false, false, true, false),
        ("-.->", true, false, true, false, false),
        ("-->", false, false, true, false, false),
        ("==>", false, true, true, false, false),
        ("---", false, false, false, false, false),
        ("-.-", true, false, false, false, false),
        ("===", false, true, false, false, false),
        ("--x", false, false, true, false, false),
        ("--o", false, false, true, false, false),
        ("x--x", false, false, true, true, true),
        ("o--o", false, false, true, true, true),
    ];
    for (op, dashed, thick, arrow_end, arrow_start, bidir) in DIRECT {
        if let Some(after) = rest.strip_prefix(op) {
            let mut consumed = op.len();
            let mut label = None;
            if let Some(tail) = after.strip_prefix('|') {
                if let Some(close) = tail.find('|') {
                    label = Some(unquote(&tail[..close]));
                    consumed += 1 + close + 1;
                }
            }
            return Some((
                EdgeStyle {
                    label,
                    dashed: *dashed,
                    thick: *thick,
                    bidir: *bidir,
                    arrow_end: *arrow_end,
                    arrow_start: *arrow_start,
                },
                consumed,
            ));
        }
    }

    // Labelled operators: `-- text -->`, `-. text .->`, `== text ==>`.
    let (open, closes): (&str, &[&str]) = if rest.starts_with("--") {
        ("--", &["-->", "---"])
    } else if rest.starts_with("-.") {
        ("-.", &[".->", "-.-"])
    } else if rest.starts_with("==") {
        ("==", &["==>", "==="])
    } else {
        return None;
    };
    let tail = &rest[open.len()..];
    let mut best: Option<(usize, &str)> = None;
    for pattern in closes {
        if let Some(pos) = tail.find(pattern) {
            if best.map_or(true, |(b, _)| pos < b) {
                best = Some((pos, pattern));
            }
        }
    }
    let (pos, pattern) = best?;
    let label = unquote(tail[..pos].trim());
    let consumed = open.len() + pos + pattern.len();
    let dashed = pattern.contains('.');
    let thick = pattern.contains('=');
    let plain = pattern.chars().all(|c| matches!(c, '-' | '.' | '='));
    let arrow_start = pattern.starts_with('<');
    Some((
        EdgeStyle {
            label: (!label.is_empty()).then_some(label),
            dashed,
            thick,
            bidir: arrow_start && pattern.ends_with('>'),
            arrow_end: !plain,
            arrow_start,
        },
        consumed,
    ))
}

fn is_edge_operator(op: &str) -> bool {
    matches!(
        op,
        "-->"
            | "---"
            | "<-->"
            | "<---"
            | "-.->"
            | "-.-"
            | "==>"
            | "==="
            | "--x"
            | "--o"
            | "x--x"
            | "o--o"
    ) || (op.len() >= 2 && op.chars().all(|c| c == '-' || c == '.' || c == '='))
        || (op.starts_with('<') && op.ends_with('>'))
}

fn edge_style_for(op: &str, label: Option<String>) -> EdgeStyle {
    let dashed = op.contains('.');
    let thick = op.contains('=');
    let arrow_start = op.starts_with('<') || op.starts_with("x--") || op.starts_with("o--");
    let plain = op.chars().all(|c| matches!(c, '-' | '.' | '='));
    EdgeStyle {
        label,
        dashed,
        thick,
        bidir: arrow_start && op.ends_with('>'),
        arrow_end: if plain { false } else { true },
        arrow_start,
    }
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let mut parser = Parser::new();
    let mut seen_header = false;
    for (line_no, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !seen_header {
            seen_header = true;
            parser.parse_line(line, line_no + 1, true);
            // Support a single-line graph: `graph TD; A-->B`.
            if let Some((_, rest)) = line.split_once(';') {
                for stmt in rest.split(';') {
                    parser.parse_line(stmt, line_no + 1, false);
                }
            }
            continue;
        }
        // Top-level semicolon-separated statements.
        for stmt in split_statements(line) {
            parser.parse_line(&stmt, line_no + 1, false);
        }
    }
    if parser.nodes.is_empty() {
        return Err(DiagramError::new(
            "No nodes found. Define nodes with `A[Label] --> B[Label]`.",
        ));
    }

    let mut factory = TextFactory::new(engine);
    let mut nodes: Vec<GraphNode> = Vec::with_capacity(parser.nodes.len());
    for parsed in &parser.nodes {
        let label = factory.layout(
            &parsed.label,
            13.0,
            500,
            210.0,
            TextAlign::Center,
            None,
        );
        let mut node = GraphNode::new(parsed.id.clone(), label, parsed.shape);
        node.group = parsed.group;
        nodes.push(node);
    }
    let mut edges: Vec<GraphEdge> = Vec::with_capacity(parser.edges.len());
    for parsed in &parser.edges {
        let mut edge = GraphEdge::new(parsed.from, parsed.to);
        edge.dashed = parsed.style.dashed;
        edge.thick = parsed.style.thick;
        edge.bidir = parsed.style.bidir;
        edge.arrow_end = parsed.style.arrow_end;
        edge.arrow_start = parsed.style.arrow_start;
        if let Some(text) = &parsed.style.label {
            if !text.trim().is_empty() {
                edge.label = Some(factory.layout(
                    text,
                    11.5,
                    400,
                    160.0,
                    TextAlign::Center,
                    None,
                ));
            }
        }
        edges.push(edge);
    }
    let mut groups: Vec<GraphGroup> = Vec::with_capacity(parser.groups.len());
    for parsed in &parser.groups {
        let title = factory.layout(&parsed.title, 12.0, 600, 260.0, TextAlign::Left, None);
        groups.push(GraphGroup {
            title,
            parent: parsed.parent,
            members: parsed.members.clone(),
        });
    }
    drop(factory);

    let laid = layout(&nodes, &edges, parser.direction, 58.0, 52.0);
    let margin = 30.0;
    let mut scene = padded_scene(theme, laid.width, laid.height, margin);
    draw_graph(
        &mut scene,
        &nodes,
        &edges,
        &laid,
        &groups,
        theme,
        (margin, margin),
    );

    Ok(BuiltDiagram {
        scene,
        kind: DiagramKind::Flowchart,
        width: laid.width + margin * 2.0,
        height: laid.height + margin * 2.0,
        warnings: parser.warnings,
    })
}

fn split_statements(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                current.push(ch);
            }
            '[' | '(' | '{' if !in_quote => {
                depth += 1;
                current.push(ch);
            }
            ']' | ')' | '}' if !in_quote => {
                depth -= 1;
                current.push(ch);
            }
            ';' if !in_quote && depth <= 0 => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                }
                current.clear();
                depth = 0;
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    fn theme() -> DiagramTheme {
        DiagramTheme::light()
    }

    #[test]
    fn parses_simple_flowchart() {
        let src = "flowchart TD\nA[Start] --> B{Choice}\nB -->|Yes| C[Do it]\nB -->|No| D[End]\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert_eq!(built.kind, DiagramKind::Flowchart);
        assert!(built.width > 100.0 && built.height > 100.0);
        assert!(!built.scene.items.is_empty());
    }

    #[test]
    fn parses_lr_direction() {
        let src = "graph LR\nA --> B --> C\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.width > built.height);
    }

    #[test]
    fn parses_shapes_and_labels() {
        let src = "graph TD\nA(Round) --> B((Circle))\nC{Diamond} --> D[(Cylinder)]\nE[[Sub]] --> F>Asym]\n";
        let mut parser = Parser::new();
        for (i, line) in src.lines().enumerate() {
            parser.parse_line(line, i + 1, i == 0);
        }
        assert_eq!(parser.nodes.len(), 6);
        assert_eq!(parser.nodes[0].shape, NodeShape::Rounded);
        assert_eq!(parser.nodes[1].shape, NodeShape::Circle);
        assert_eq!(parser.nodes[2].shape, NodeShape::Diamond);
        assert_eq!(parser.nodes[3].shape, NodeShape::Cylinder);
        assert_eq!(parser.nodes[4].shape, NodeShape::Subroutine);
        assert_eq!(parser.nodes[5].shape, NodeShape::Asymmetric);
    }

    #[test]
    fn parses_edge_labels_in_all_forms() {
        let src = "graph LR\nA -->|pipe| B\nB -- dash --> C\nC -. dot .-> D\nD ==>|thick| E\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.width > 300.0);
    }

    #[test]
    fn supports_persian_node_ids() {
        let src = "flowchart TD\nشروع[شروع] --> تصمیم{آیا ادامه؟}\nتصمیم --> پایان[پایان]\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.width > 100.0);
    }

    #[test]
    fn supports_persian_labels() {
        let src = "flowchart TD\nA[شروع] --> B{تصمیم}\nB --> C[پایان]\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.width > 100.0);
    }

    #[test]
    fn supports_subgraphs() {
        let src = "flowchart TB\nsubgraph One\nA --> B\nend\nsubgraph Two\nC\nend\nB --> C\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.scene.items.len() > 8);
    }

    #[test]
    fn unsupported_lines_do_not_panic() {
        let src = "flowchart TD\nA --> B\nclick A callback\nclassDef red fill:#f00\nstyle A fill:#f9f\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        assert!(built.warnings.is_empty());
    }

    #[test]
    fn empty_graph_errors() {
        let mut engine = TextEngine::new(None, "en-US");
        assert!(build("flowchart TD\n", &theme(), &mut engine).is_err());
    }

    #[test]
    fn renders_to_png() {
        let src = "flowchart LR\nA[One] --> B[Two]\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &theme(), &mut engine).unwrap();
        let png = built.encode_png(&mut engine, 2.0).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(png.len() > 1000);
    }
}