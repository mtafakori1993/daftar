//! State machine diagram parser and renderer.

use std::collections::HashMap;

use daftar_core::settings::TextAlign;

use crate::graph::{
    GraphDirection, GraphEdge, GraphNode, NodeShape, draw_edges_simple, layout,
};
use crate::scene::{Label, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene, unquote};

#[derive(Debug, Clone)]
struct StateDef {
    id: String,
    label: String,
    kind: StateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Normal,
    Start,
    End,
}

#[derive(Debug, Clone)]
struct Transition {
    from: usize,
    to: usize,
    label: String,
}

struct ParsedStates {
    states: Vec<StateDef>,
    index: HashMap<String, usize>,
    transitions: Vec<Transition>,
    direction: GraphDirection,
}

impl ParsedStates {
    fn new() -> Self {
        Self {
            states: Vec::new(),
            index: HashMap::new(),
            transitions: Vec::new(),
            direction: GraphDirection::TopBottom,
        }
    }

    fn state(&mut self, id: &str, label: Option<&str>, kind: StateKind) -> usize {
        let id = id.trim().to_string();
        if let Some(idx) = self.index.get(&id).copied() {
            if let Some(label) = label {
                if !label.trim().is_empty() {
                    self.states[idx].label = label.trim().to_string();
                }
            }
            return idx;
        }
        let idx = self.states.len();
        self.states.push(StateDef {
            id: id.clone(),
            label: label
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| id.clone()),
            kind,
        });
        self.index.insert(id, idx);
        idx
    }

    fn start(&mut self) -> usize {
        self.state("__start", Some(""), StateKind::Start)
    }

    fn end(&mut self) -> usize {
        self.state("__end", Some(""), StateKind::End)
    }
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let parsed = parse(source)?;
    if parsed.states.is_empty() {
        return Err(DiagramError::new("No states found in state diagram."));
    }
    let mut factory = TextFactory::new(engine);
    let labels: Vec<Label> = parsed
        .states
        .iter()
        .map(|s| match s.kind {
            StateKind::Normal => factory.layout(
                &s.label,
                13.0,
                500,
                220.0,
                TextAlign::Center,
                None,
            ),
            _ => factory.layout("", 13.0, 500, 10.0, TextAlign::Center, None),
        })
        .collect();
    let mut nodes: Vec<GraphNode> = parsed
        .states
        .iter()
        .enumerate()
        .map(|(i, state)| {
            let mut node = GraphNode::new(
                state.id.clone(),
                labels[i].clone(),
                NodeShape::Rounded,
            );
            match state.kind {
                StateKind::Start => {
                    node.width = 26.0;
                    node.height = 26.0;
                    node.invisible = true;
                }
                StateKind::End => {
                    node.width = 30.0;
                    node.height = 30.0;
                    node.invisible = true;
                }
                StateKind::Normal => {
                    node.width = (labels[i].width + 40.0).max(110.0);
                    node.height = (labels[i].height + 24.0).max(48.0);
                }
            }
            node
        })
        .collect();
    let edge_labels = parsed
        .transitions
        .iter()
        .map(|t| {
            if t.label.trim().is_empty() {
                None
            } else {
                Some(factory.layout(t.label.trim(), 11.5, 400, 150.0, TextAlign::Center, None))
            }
        })
        .collect::<Vec<_>>();
    drop(factory);
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (i, t) in parsed.transitions.iter().enumerate() {
        let mut edge = GraphEdge::new(t.from, t.to);
        edge.arrow_end = true;
        edge.label = edge_labels[i].clone();
        edges.push(edge);
    }
    let laid = layout(&nodes, &edges, parsed.direction, 58.0, 56.0);
    let margin = 34.0;
    let mut scene = padded_scene(theme, laid.width, laid.height, margin);
    draw_edges_simple(&mut scene, &nodes, &edges, &laid, theme, (margin, margin));

    for (i, state) in parsed.states.iter().enumerate() {
        let center = (laid.centers[i].0 + margin, laid.centers[i].1 + margin);
        match state.kind {
            StateKind::Start => {
                scene.ellipse(
                    center.0,
                    center.1,
                    11.0,
                    11.0,
                    Some(theme.text),
                    None,
                );
            }
            StateKind::End => {
                scene.ellipse(
                    center.0,
                    center.1,
                    13.0,
                    13.0,
                    Some(theme.background),
                    Some(SceneStroke::solid(theme.text, 2.0)),
                );
                scene.ellipse(center.0, center.1, 8.0, 8.0, Some(theme.text), None);
            }
            StateKind::Normal => {
                let w = nodes[i].width;
                let h = nodes[i].height;
                let x = center.0 - w * 0.5;
                let y = center.1 - h * 0.5;
                scene.rect(
                    x,
                    y,
                    w,
                    h,
                    12.0,
                    Some(theme.node_fill),
                    Some(SceneStroke::solid(theme.node_stroke, 1.2)),
                );
                let label = &labels[i];
                scene.text(
                    label.shaped.clone(),
                    x + (w - label.width) * 0.5,
                    y + (h - label.height) * 0.5,
                    theme.text,
                );
            }
        }
        let _ = state.id;
    }

    Ok(BuiltDiagram {
        kind: DiagramKind::State,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: Vec::new(),
    })
}

fn parse(source: &str) -> Result<ParsedStates, DiagramError> {
    let mut parsed = ParsedStates::new();
    let mut aliases: HashMap<String, String> = HashMap::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "statediagram" || lower == "statediagram-v2" {
            continue;
        }
        if lower.starts_with("direction ") {
            if let Some(dir) = line.split_whitespace().nth(1).and_then(GraphDirection::from_keyword) {
                parsed.direction = dir;
            }
            continue;
        }
        if lower.starts_with("note") {
            continue;
        }
        if lower.starts_with("state ") {
            let rest = line[6..].trim();
            if let Some(brace) = rest.find('{') {
                let name = unquote(rest[..brace].trim());
                if !name.is_empty() {
                    parsed.state(&name, Some(&name), StateKind::Normal);
                }
                continue;
            }
            if let Some((name_part, id_part)) = rest.split_once(" as ") {
                let label = unquote(name_part.trim());
                let id = id_part.trim().trim_end_matches('{').trim().to_string();
                if !id.is_empty() {
                    aliases.insert(id.clone(), label.clone());
                    parsed.state(&id, Some(&label), StateKind::Normal);
                }
            } else {
                let name = unquote(rest);
                if !name.is_empty() {
                    parsed.state(&name, Some(&name), StateKind::Normal);
                }
            }
            continue;
        }
        if line == "}" {
            continue;
        }
        if line.contains("-->") {
            let (head, label) = match line.split_once(':') {
                Some((h, l)) => (h, l.trim()),
                None => (line, ""),
            };
            let Some((from_raw, to_raw)) = head.split_once("-->") else {
                continue;
            };
            let from_raw = from_raw.trim();
            let to_raw = to_raw.trim();
            let from = if from_raw == "[*]" {
                parsed.start()
            } else {
                let id = unquote(from_raw);
                let label = aliases.get(&id).cloned();
                parsed.state(&id, label.as_deref(), StateKind::Normal)
            };
            let to = if to_raw == "[*]" {
                parsed.end()
            } else {
                let id = unquote(to_raw);
                let label = aliases.get(&id).cloned();
                parsed.state(&id, label.as_deref(), StateKind::Normal)
            };
            if from == to {
                // Ignore self-loops on the synthetic start/end nodes.
                if matches!(parsed.states[from].kind, StateKind::Start | StateKind::End) {
                    continue;
                }
            }
            parsed.transitions.push(Transition {
                from,
                to,
                label: clean_label(label),
            });
            continue;
        }
        if lower.starts_with("classdef") || lower.starts_with("class ") || lower.starts_with("style ") {
            continue;
        }
        if !line.is_empty() && !line.starts_with("--") {
            // A bare identifier is a state.
            let id = unquote(line.trim_end_matches(';'));
            if !id.is_empty() && !id.contains(' ') {
                parsed.state(&id, None, StateKind::Normal);
                continue;
            }
            if line_no < 100 {
                return Err(DiagramError::at(
                    line_no + 1,
                    format!("Could not parse state statement `{line}`."),
                ));
            }
        }
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_transitions_and_start_end() {
        let src = "stateDiagram-v2\n[*] --> Idle\nIdle --> Running : start\nRunning --> [*]\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.transitions.len(), 3);
        assert!(parsed.states.iter().any(|s| s.kind == StateKind::Start));
        assert!(parsed.states.iter().any(|s| s.kind == StateKind::End));
    }

    #[test]
    fn renders_states() {
        let src = "stateDiagram-v2\n[*] --> A\nA --> B : go\nB --> [*]\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 100.0);
        assert!(built.scene.items.len() > 5);
    }

    #[test]
    fn supports_aliases_and_persian() {
        let src = "stateDiagram-v2\nstate \"در انتظار\" as Idle\n[*] --> Idle\nIdle --> Done : پایان\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn composite_braces_do_not_error() {
        let src = "stateDiagram-v2\nstate Working {\n  [*] --> A\n  A --> [*]\n}\n[*] --> Working\n";
        let mut engine = TextEngine::new(None, "en-US");
        assert!(build(src, &DiagramTheme::light(), &mut engine).is_ok());
    }
}