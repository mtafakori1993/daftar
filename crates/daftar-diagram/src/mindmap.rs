//! Mindmap parser and renderer with a left-to-right tree layout.

use daftar_core::settings::TextAlign;

use crate::scene::{Label, PathOp, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene};

#[derive(Debug, Clone)]
struct RawNode {
    label: String,
    indent: usize,
}

struct Node {
    label: Label,
    width: f32,
    height: f32,
    children: Vec<usize>,
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let (title, raw_nodes) = parse(source)?;
    if raw_nodes.is_empty() {
        return Err(DiagramError::new("Mindmap has no nodes."));
    }
    let mut factory = TextFactory::new(engine);
    let title_label = title
        .as_ref()
        .map(|t| factory.layout(t, 16.0, 600, 700.0, TextAlign::Center, None));
    let mut labels = Vec::new();
    for raw in &raw_nodes {
        labels.push(factory.layout(&raw.label, 12.5, 500, 190.0, TextAlign::Left, None));
    }
    drop(factory);

    // Build parent/children from indentation.
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); raw_nodes.len()];
    let mut parent = vec![None; raw_nodes.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, raw) in raw_nodes.iter().enumerate() {
        while let Some(&last) = stack.last() {
            if raw_nodes[last].indent >= raw.indent {
                stack.pop();
            } else {
                break;
            }
        }
        if let Some(&p) = stack.last() {
            children[p].push(i);
            parent[i] = Some(p);
        } else if i > 0 {
            // Additional top-level nodes attach to the first node as a pragmatic tree.
            children[0].push(i);
            parent[i] = Some(0);
        }
        stack.push(i);
    }
    let _ = parent;

    let mut nodes: Vec<Node> = raw_nodes
        .iter()
        .enumerate()
        .map(|(i, _)| Node {
            label: labels[i].clone(),
            width: (labels[i].width + 30.0).min(250.0),
            height: (labels[i].height + 20.0).max(42.0),
            children: children[i].clone(),
        })
        .collect();

    // Measure subtree heights bottom-up.
    let mut subtree: Vec<f32> = vec![0.0; nodes.len()];
    let order = postorder(&nodes, 0);
    for idx in order {
        let child_total: f32 = nodes[idx]
            .children
            .iter()
            .map(|c| subtree[*c])
            .sum::<f32>()
            + nodes[idx].children.len().saturating_sub(1) as f32 * 18.0;
        subtree[idx] = nodes[idx].height.max(child_total);
    }

    let margin = 34.0;
    let title_h = title_label.as_ref().map_or(0.0, |t| t.height + 16.0);
    let mut positions = vec![(0.0f32, 0.0f32); nodes.len()];
    place_tree(&nodes, &subtree, 0, margin, margin + title_h, 0, &mut positions);
    let max_x = positions
        .iter()
        .enumerate()
        .map(|(i, p)| p.0 + nodes[i].width)
        .fold(0.0f32, f32::max);
    let max_y = positions
        .iter()
        .enumerate()
        .map(|(i, p)| p.1 + nodes[i].height)
        .fold(0.0f32, f32::max);
    let mut scene = padded_scene(theme, max_x + margin, max_y + margin, 0.0);
    scene.background = Some(theme.background);
    if let Some(title) = &title_label {
        let x = (scene.width - title.width) * 0.5;
        scene.text(title.shaped.clone(), x.max(10.0), 10.0, theme.text);
    }

    // Connectors behind nodes.
    for (i, node) in nodes.iter().enumerate() {
        let (px, py) = positions[i];
        for child in &node.children {
            let (cx, cy) = positions[*child];
            let start = (px + node.width, py + node.height * 0.5);
            let end = (cx, cy + nodes[*child].height * 0.5);
            let mid = (start.0 + end.0) * 0.5;
            scene.path(
                vec![
                    PathOp::Move(start.0, start.1),
                    PathOp::Cubic(mid, start.1, mid, end.1, end.0, end.1),
                ],
                None,
                Some(SceneStroke::solid(theme.line, 1.4)),
            );
        }
    }

    for (i, node) in nodes.iter().enumerate() {
        let (x, y) = positions[i];
        let is_root = i == 0;
        let fill = if is_root { theme.accent } else { theme.node_fill };
        let stroke = if is_root { theme.accent } else { theme.node_stroke };
        scene.rect(
            x,
            y,
            node.width,
            node.height,
            8.0,
            Some(fill),
            Some(SceneStroke::solid(stroke, 1.1)),
        );
        let text_color = if is_root { theme.background } else { theme.text };
        scene.text(
            node.label.shaped.clone(),
            x + 15.0,
            y + (node.height - node.label.height) * 0.5,
            text_color,
        );
    }
    let _ = Label {
        shaped: daftar_text::ShapedText::default(),
        width: 0.0,
        height: 0.0,
    };

    Ok(BuiltDiagram {
        kind: DiagramKind::Mindmap,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: Vec::new(),
    })
}

fn postorder(nodes: &[Node], root: usize) -> Vec<usize> {
    let mut out = Vec::new();
    fn walk(nodes: &[Node], idx: usize, out: &mut Vec<usize>) {
        for child in &nodes[idx].children {
            walk(nodes, *child, out);
        }
        out.push(idx);
    }
    walk(nodes, root, &mut out);
    out
}

fn place_tree(
    nodes: &[Node],
    subtree: &[f32],
    idx: usize,
    x: f32,
    top: f32,
    depth: usize,
    positions: &mut [(f32, f32)],
) {
    let total = subtree[idx];
    let node_y = top + (total - nodes[idx].height) * 0.5;
    positions[idx] = (x, node_y);
    let child_gap = 18.0;
    let child_total: f32 = nodes[idx]
        .children
        .iter()
        .map(|c| subtree[*c])
        .sum::<f32>()
        + nodes[idx].children.len().saturating_sub(1) as f32 * child_gap;
    let mut child_top = top + (total - child_total) * 0.5;
    for child in &nodes[idx].children {
        place_tree(
            nodes,
            subtree,
            *child,
            x + nodes[idx].width + 52.0,
            child_top,
            depth + 1,
            positions,
        );
        child_top += subtree[*child] + child_gap;
    }
}

fn parse(source: &str) -> Result<(Option<String>, Vec<RawNode>), DiagramError> {
    let mut title = None;
    let mut nodes = Vec::new();
    for raw in source.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim().starts_with("%%") {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("mindmap") {
            continue;
        }
        if trimmed.to_ascii_lowercase().starts_with("title ") {
            title = Some(trimmed[6..].trim().to_string());
            continue;
        }
        let indent = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();
        let label = extract_label(trimmed);
        if label.is_empty() {
            continue;
        }
        nodes.push(RawNode {
            label: clean_label(&label),
            indent,
        });
    }
    Ok((title, nodes))
}

fn extract_label(text: &str) -> String {
    let mut text = text.trim();
    for suffix in ["::icon", ":::"] {
        if let Some(pos) = text.find(suffix) {
            text = text[..pos].trim();
        }
    }
    let first = text
        .find(['[', '(', '{'])
        .or_else(|| if text.starts_with(')') { Some(0) } else { None });
    if let Some(start) = first {
        let doubled = text[start..].starts_with("((") || text[start..].starts_with("{{");
        let open_len = if doubled { 2 } else { 1 };
        let inner = &text[start + open_len..];
        let trimmed = inner.trim_end();
        let close_len = if doubled {
            if trimmed.ends_with("))") || trimmed.ends_with("}}") {
                2
            } else {
                1
            }
        } else if trimmed.ends_with(')') || trimmed.ends_with(']') || trimmed.ends_with('}') {
            1
        } else {
            0
        };
        let candidate = if close_len > 0 {
            trimmed[..trimmed.len().saturating_sub(close_len)].trim()
        } else {
            trimmed.trim()
        };
        let candidate = candidate.trim_matches('"').trim();
        if !candidate.is_empty() {
            return candidate.to_string();
        }
    }
    text.trim_matches('"').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    fn engine() -> TextEngine {
        TextEngine::new(None, "en-US")
    }

    #[test]
    fn parses_indentation_tree() {
        let src = "mindmap\n  root((Knowledge))\n    Library\n      Markdown\n      Highlights\n    Study\n";
        let (_, nodes) = parse(src).unwrap();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0].label, "Knowledge");
        assert!(nodes[2].indent > nodes[1].indent);
    }

    #[test]
    fn renders_mindmap() {
        let src = "mindmap\n  root((Mind))\n    A[Branch A]\n      A1[Leaf]\n    B(Branch B)\n";
        let mut engine = engine();
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 200.0);
        assert!(built.scene.items.len() > 5);
    }

    #[test]
    fn supports_persian_nodes() {
        let src = "mindmap\n  root((دانش))\n    کتابخانه\n      مارک‌داون\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn extract_label_handles_shapes() {
        assert_eq!(extract_label("root((Hello))"), "Hello");
        assert_eq!(extract_label("A[Branch]"), "Branch");
        assert_eq!(extract_label("Plain"), "Plain");
    }
}
