//! Entity-relationship diagram parser and renderer.

use std::collections::HashMap;

use daftar_core::settings::TextAlign;

use crate::graph::{GraphDirection, GraphEdge, GraphNode, NodeShape, draw_edges_simple, layout};
use crate::scene::{Label, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene, unquote};

#[derive(Debug, Clone)]
struct Attribute {
    type_name: String,
    name: String,
    key: Option<String>,
    comment: Option<String>,
}

#[derive(Debug, Clone)]
struct Entity {
    id: String,
    attributes: Vec<Attribute>,
}

impl Entity {
    fn new(id: String) -> Self {
        Self {
            id,
            attributes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct ErRelation {
    from: usize,
    to: usize,
    from_card: String,
    to_card: String,
    label: String,
    dashed: bool,
}

struct ParsedEr {
    entities: Vec<Entity>,
    index: HashMap<String, usize>,
    relations: Vec<ErRelation>,
    direction: GraphDirection,
}

impl ParsedEr {
    fn new() -> Self {
        Self {
            entities: Vec::new(),
            index: HashMap::new(),
            relations: Vec::new(),
            direction: GraphDirection::LeftRight,
        }
    }

    fn entity(&mut self, name: &str) -> usize {
        let name = unquote(name.trim());
        if let Some(idx) = self.index.get(&name).copied() {
            return idx;
        }
        let idx = self.entities.len();
        self.entities.push(Entity::new(name.clone()));
        self.index.insert(name, idx);
        idx
    }
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let parsed = parse(source)?;
    if parsed.entities.is_empty() {
        return Err(DiagramError::new("No entities found in ER diagram."));
    }
    let mut factory = TextFactory::new(engine);
    let name_labels: Vec<Label> = parsed
        .entities
        .iter()
        .map(|e| factory.layout(&e.id, 13.0, 600, 260.0, TextAlign::Center, None))
        .collect();
    let attr_labels: Vec<Vec<Label>> = parsed
        .entities
        .iter()
        .map(|e| {
            e.attributes
                .iter()
                .map(|a| {
                    let mut text = format!("{} {}", a.type_name, a.name);
                    if let Some(key) = &a.key {
                        text.push(' ');
                        text.push_str(key);
                    }
                    if let Some(comment) = &a.comment {
                        text.push_str("  ");
                        text.push_str(comment);
                    }
                    factory.layout_mono(&text, 11.0, 300.0)
                })
                .collect()
        })
        .collect();
    let relation_labels: Vec<Option<Label>> = parsed
        .relations
        .iter()
        .map(|r| {
            (!r.label.trim().is_empty()).then(|| {
                factory.layout(r.label.trim(), 11.0, 400, 140.0, TextAlign::Center, None)
            })
        })
        .collect();
    let card_labels: Vec<(Label, Label)> = parsed
        .relations
        .iter()
        .map(|r| {
            (
                factory.layout(&cardinality_text(&r.from_card), 10.0, 500, 60.0, TextAlign::Center, None),
                factory.layout(&cardinality_text(&r.to_card), 10.0, 500, 60.0, TextAlign::Center, None),
            )
        })
        .collect();
    drop(factory);

    let mut boxes = Vec::new();
    for (i, entity) in parsed.entities.iter().enumerate() {
        let name = &name_labels[i];
        let mut width = (name.width + 34.0).max(150.0);
        for attr in &attr_labels[i] {
            width = width.max(attr.width + 40.0);
        }
        let header_h = name.height + 18.0;
        let rows_h: f32 = attr_labels[i].iter().map(|a| a.height + 6.0).sum();
        let height = header_h + rows_h + if entity.attributes.is_empty() { 16.0 } else { 10.0 };
        boxes.push((width.min(400.0), height.max(56.0), header_h));
    }

    let nodes: Vec<GraphNode> = parsed
        .entities
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let mut node = GraphNode::new(e.id.clone(), name_labels[i].clone(), NodeShape::Rect);
            node.width = boxes[i].0;
            node.height = boxes[i].1;
            node
        })
        .collect();
    let mut edges = Vec::new();
    for (i, rel) in parsed.relations.iter().enumerate() {
        let mut edge = GraphEdge::new(rel.from, rel.to);
        edge.dashed = rel.dashed;
        edge.arrow_end = false;
        edge.label = relation_labels[i].clone();
        edges.push(edge);
    }
    let laid = layout(&nodes, &edges, parsed.direction, 82.0, 70.0);
    let margin = 34.0;
    let mut scene = padded_scene(theme, laid.width, laid.height, margin);
    draw_edges_simple(&mut scene, &nodes, &edges, &laid, theme, (margin, margin));

    // Cardinality labels near each endpoint.
    for (i, rel) in parsed.relations.iter().enumerate() {
        let (a, b) = (&card_labels[i].0, &card_labels[i].1);
        let c1 = laid.centers[rel.from];
        let c2 = laid.centers[rel.to];
        let dx = c2.0 - c1.0;
        let dy = c2.1 - c1.1;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let (ux, uy) = (dx / len, dy / len);
        let from_x = c1.0 + margin + ux * (boxes[rel.from].0 * 0.5 + 8.0);
        let from_y = c1.1 + margin + uy * (boxes[rel.from].1 * 0.5 + 8.0);
        let to_x = c2.0 + margin - ux * (boxes[rel.to].0 * 0.5 + 8.0);
        let to_y = c2.1 + margin - uy * (boxes[rel.to].1 * 0.5 + 8.0);
        scene.text(a.shaped.clone(), from_x - a.width * 0.5, from_y - a.height - 2.0, theme.muted);
        scene.text(b.shaped.clone(), to_x - b.width * 0.5, to_y + 3.0, theme.muted);
    }

    // Entity boxes.
    for (i, entity) in parsed.entities.iter().enumerate() {
        let center = laid.centers[i];
        let (w, h, header_h) = boxes[i];
        let x = center.0 + margin - w * 0.5;
        let y = center.1 + margin - h * 0.5;
        scene.rect(
            x,
            y,
            w,
            header_h,
            4.0,
            Some(theme.accent.mix(theme.node_fill, 0.84)),
            Some(SceneStroke::solid(theme.node_stroke, 1.1)),
        );
        scene.rect(
            x,
            y + header_h,
            w,
            h - header_h,
            0.0,
            Some(theme.node_fill),
            Some(SceneStroke::solid(theme.node_stroke, 1.1)),
        );
        let name = &name_labels[i];
        scene.text(
            name.shaped.clone(),
            x + (w - name.width) * 0.5,
            y + 8.0,
            theme.text,
        );
        let mut row_y = y + header_h + 7.0;
        for label in &attr_labels[i] {
            scene.text(label.shaped.clone(), x + 12.0, row_y, theme.text);
            row_y += label.height + 6.0;
        }
    }

    Ok(BuiltDiagram {
        kind: DiagramKind::Er,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: Vec::new(),
    })
}

fn cardinality_text(token: &str) -> String {
    match token.replace(' ', "").as_str() {
        "||" | "|" => "1".to_string(),
        "o|" | "|o" => "0..1".to_string(),
        "}|" | "|{" | "|}" | "{|" => "1..N".to_string(),
        "o{" | "{o" | "}o" | "o}" => "0..N".to_string(),
        other => other.to_string(),
    }
}

fn parse(source: &str) -> Result<ParsedEr, DiagramError> {
    let mut parsed = ParsedEr::new();
    let mut current: Option<usize> = None;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "erdiagram" {
            continue;
        }
        if lower.starts_with("direction ") {
            if let Some(dir) = line.split_whitespace().nth(1).and_then(GraphDirection::from_keyword) {
                parsed.direction = dir;
            }
            continue;
        }
        if line == "}" {
            current = None;
            continue;
        }
        if let Some(idx) = current {
            let attr = parse_attribute(line);
            if let Some(attr) = attr {
                parsed.entities[idx].attributes.push(attr);
            }
            continue;
        }
        if line.ends_with('{') {
            let name = line[..line.len() - 1].trim();
            if !name.is_empty() {
                let idx = parsed.entity(name);
                current = Some(idx);
            }
            continue;
        }
        if let Some(rel) = parse_relation(line) {
            let from = parsed.entity(&rel.0);
            let to = parsed.entity(&rel.2);
            parsed.relations.push(ErRelation {
                from,
                to,
                from_card: rel.1,
                to_card: rel.3,
                label: clean_label(&rel.4),
                dashed: rel.5,
            });
            continue;
        }
        if line_no < 200 && !line.contains(' ') {
            parsed.entity(line);
            continue;
        }
        return Err(DiagramError::at(
            line_no + 1,
            format!("Could not parse ER statement `{line}`."),
        ));
    }
    Ok(parsed)
}

fn parse_attribute(line: &str) -> Option<Attribute> {
    let (head, comment) = if let Some(start) = line.find('"') {
        if let Some(end) = line[start + 1..].find('"') {
            (
                line[..start].trim(),
                Some(line[start + 1..start + 1 + end].to_string()),
            )
        } else {
            (line, None)
        }
    } else {
        (line.trim(), None)
    };
    let tokens: Vec<&str> = head.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let key = tokens
        .iter()
        .skip(2)
        .find(|t| matches!(t.to_ascii_uppercase().as_str(), "PK" | "FK" | "UK" | "PK,FK"))
        .map(|s| s.to_string());
    Some(Attribute {
        type_name: tokens[0].to_string(),
        name: tokens[1].to_string(),
        key,
        comment,
    })
}

/// Returns (from, from_card, to, to_card, label, dashed).
fn parse_relation(line: &str) -> Option<(String, String, String, String, String, bool)> {
    let (body, label) = match line.split_once(':') {
        Some((b, l)) => (b.trim(), l.trim().to_string()),
        None => (line, String::new()),
    };
    let (delim, dashed) = if body.contains("--") {
        ("--", false)
    } else if body.contains("..") {
        ("..", true)
    } else {
        return None;
    };
    let (left, right) = body.split_once(delim)?;
    let left_tokens: Vec<&str> = left.split_whitespace().collect();
    let right_tokens: Vec<&str> = right.split_whitespace().collect();
    if left_tokens.len() < 2 || right_tokens.len() < 2 {
        return None;
    }
    let from = unquote(left_tokens[0]);
    let from_card = left_tokens[1].to_string();
    let to_card = right_tokens[0].to_string();
    let to = unquote(right_tokens[1]);
    Some((from, from_card, to, to_card, label, dashed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_entities_attributes_and_relations() {
        let src = "erDiagram\nCUSTOMER {\n  string name PK\n  int id\n}\nORDER {\n  int id\n}\nCUSTOMER ||--o{ ORDER : places\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.entities.len(), 2);
        assert_eq!(parsed.entities[0].attributes.len(), 2);
        assert_eq!(parsed.entities[0].attributes[0].key.as_deref(), Some("PK"));
        assert_eq!(parsed.relations.len(), 1);
        assert_eq!(parsed.relations[0].label, "places");
    }

    #[test]
    fn renders_er_diagram() {
        let src = "erDiagram\nA {\n  int id\n}\nB {\n  int id\n}\nA ||--o{ B : has\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 100.0);
        assert!(built.scene.items.len() > 5);
    }

    #[test]
    fn supports_persian_names() {
        let src = "erDiagram\nکاربر {\n  string نام\n}\nسفارش {\n  int شناسه\n}\nکاربر ||--o{ سفارش : ثبت\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn cardinalities_map_to_human_labels() {
        assert_eq!(cardinality_text("||"), "1");
        assert_eq!(cardinality_text("o{"), "0..N");
        assert_eq!(cardinality_text("}|"), "1..N");
    }
}