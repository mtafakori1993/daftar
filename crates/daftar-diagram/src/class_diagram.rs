//! Class diagram parser and renderer (UML-style boxes).

use std::collections::HashMap;

use daftar_core::settings::TextAlign;

use crate::graph::{
    GraphDirection, GraphEdge, GraphNode, NodeShape, draw_edges_simple, layout,
};
use crate::scene::{Label, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene, unquote};

#[derive(Debug, Clone)]
struct ClassDef {
    id: String,
    name: String,
    stereotype: Option<String>,
    attributes: Vec<String>,
    methods: Vec<String>,
}

#[derive(Debug, Clone)]
struct Relation {
    from: usize,
    to: usize,
    dashed: bool,
    label: String,
}

struct ParsedClasses {
    classes: Vec<ClassDef>,
    index: HashMap<String, usize>,
    relations: Vec<Relation>,
    direction: GraphDirection,
}

impl ParsedClasses {
    fn new() -> Self {
        Self {
            classes: Vec::new(),
            index: HashMap::new(),
            relations: Vec::new(),
            direction: GraphDirection::TopBottom,
        }
    }

    fn class(&mut self, name: &str) -> usize {
        let name = unquote(name.trim());
        if let Some(idx) = self.index.get(&name).copied() {
            return idx;
        }
        let idx = self.classes.len();
        self.classes.push(ClassDef {
            id: name.clone(),
            name: name.clone(),
            stereotype: None,
            attributes: Vec::new(),
            methods: Vec::new(),
        });
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
    if parsed.classes.is_empty() {
        return Err(DiagramError::new("No classes found in class diagram."));
    }
    let mut factory = TextFactory::new(engine);
    let mut name_labels = Vec::new();
    let mut attr_labels = Vec::new();
    let mut method_labels = Vec::new();
    for class in &parsed.classes {
        let name = factory.layout(&class.name, 13.0, 600, 280.0, TextAlign::Center, None);
        let attrs = class
            .attributes
            .iter()
            .map(|m| factory.layout_mono(m, 11.0, 300.0))
            .collect::<Vec<_>>();
        let methods = class
            .methods
            .iter()
            .map(|m| factory.layout_mono(m, 11.0, 300.0))
            .collect::<Vec<_>>();
        name_labels.push(name);
        attr_labels.push(attrs);
        method_labels.push(methods);
    }
    let stereotype_labels: Vec<Option<Label>> = parsed
        .classes
        .iter()
        .map(|c| {
            c.stereotype.as_ref().map(|s| {
                factory.layout(
                    &format!("«{s}»"),
                    11.0,
                    500,
                    240.0,
                    TextAlign::Center,
                    None,
                )
            })
        })
        .collect();
    let relation_labels: Vec<Option<Label>> = parsed
        .relations
        .iter()
        .map(|r| {
            (!r.label.trim().is_empty()).then(|| {
                factory.layout(r.label.trim(), 11.5, 400, 160.0, TextAlign::Center, None)
            })
        })
        .collect();
    drop(factory);

    // Measure boxes.
    let mut boxes = Vec::with_capacity(parsed.classes.len());
    for (i, _) in parsed.classes.iter().enumerate() {
        let name = &name_labels[i];
        let stereotype_h = stereotype_labels[i]
            .as_ref()
            .map_or(0.0, |l| l.height + 4.0);
        let header_h = name.height + 16.0 + stereotype_h;
        let mut width = (name.width + 28.0).max(150.0);
        for label in attr_labels[i].iter().chain(method_labels[i].iter()) {
            width = width.max(label.width + 34.0);
        }
        let attr_h: f32 = attr_labels[i].iter().map(|l| l.height + 5.0).sum::<f32>();
        let method_h: f32 = method_labels[i]
            .iter()
            .map(|l| l.height + 5.0)
            .sum::<f32>();
        let attr_h = if attr_labels[i].is_empty() { 0.0 } else { attr_h + 10.0 };
        let method_h = if method_labels[i].is_empty() { 0.0 } else { method_h + 10.0 };
        let height = header_h + attr_h + method_h;
        boxes.push((width.min(380.0), height.max(50.0), header_h, attr_h));
    }

    let nodes: Vec<GraphNode> = parsed
        .classes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut node = GraphNode::new(c.id.clone(), name_labels[i].clone(), NodeShape::Rect);
            node.width = boxes[i].0;
            node.height = boxes[i].1;
            node
        })
        .collect();
    let mut edges: Vec<GraphEdge> = Vec::new();
    for (i, rel) in parsed.relations.iter().enumerate() {
        let mut edge = GraphEdge::new(rel.from, rel.to);
        edge.dashed = rel.dashed;
        edge.arrow_end = true;
        edge.label = relation_labels[i].clone();
        edges.push(edge);
    }
    let laid = layout(&nodes, &edges, parsed.direction, 70.0, 60.0);
    let margin = 34.0;
    let mut scene = padded_scene(theme, laid.width, laid.height, margin);
    draw_edges_simple(&mut scene, &nodes, &edges, &laid, theme, (margin, margin));

    for (i, class) in parsed.classes.iter().enumerate() {
        let center = laid.centers[i];
        let (w, h, header_h, _attr_h) = boxes[i];
        let x = center.0 + margin - w * 0.5;
        let y = center.1 + margin - h * 0.5;
        // Header.
        scene.rect(
            x,
            y,
            w,
            header_h,
            5.0,
            Some(theme.accent.mix(theme.node_fill, 0.82)),
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
        // Header text and stereotype.
        let mut ty = y + 8.0;
        if let Some(st) = &stereotype_labels[i] {
            let tx = x + (w - st.width) * 0.5;
            scene.text(st.shaped.clone(), tx, ty, theme.muted);
            ty += st.height + 4.0;
        }
        let name = &name_labels[i];
        scene.text(
            name.shaped.clone(),
            x + (w - name.width) * 0.5,
            ty,
            theme.text,
        );
        // Compartments.
        let mut row_y = y + header_h;
        if !attr_labels[i].is_empty() {
            scene.line(
                x,
                row_y,
                x + w,
                row_y,
                SceneStroke::solid(theme.node_stroke, 1.0),
            );
            row_y += 8.0;
            for label in &attr_labels[i] {
                scene.text(label.shaped.clone(), x + 12.0, row_y, theme.text);
                row_y += label.height + 5.0;
            }
            row_y += 5.0;
        }
        if !method_labels[i].is_empty() {
            scene.line(
                x,
                row_y,
                x + w,
                row_y,
                SceneStroke::solid(theme.node_stroke, 1.0),
            );
            row_y += 8.0;
            for label in &method_labels[i] {
                scene.text(label.shaped.clone(), x + 12.0, row_y, theme.text);
                row_y += label.height + 5.0;
            }
        }
        let _ = class.id;
    }

    Ok(BuiltDiagram {
        kind: DiagramKind::Class,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: vec![],
    })
}

fn parse(source: &str) -> Result<ParsedClasses, DiagramError> {
    let mut parsed = ParsedClasses::new();
    let mut current: Option<usize> = None;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "classdiagram" || lower.starts_with("direction ") {
            if let Some(dir) = line.split_whitespace().nth(1).and_then(GraphDirection::from_keyword) {
                parsed.direction = dir;
            }
            continue;
        }
        if lower.starts_with("note") || lower.starts_with("style ") || lower.starts_with("cssclass") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("class ") {
            let rest = rest.trim();
            if let Some(brace) = rest.find('{') {
                let name = rest[..brace].trim();
                let idx = parsed.class(name);
                current = Some(idx);
                continue;
            }
            let name = rest.trim_end_matches(';');
            parsed.class(name);
            current = None;
            continue;
        }
        if line == "}" {
            current = None;
            continue;
        }
        if let Some(idx) = current {
            let class = &mut parsed.classes[idx];
            if line.starts_with("<<") && line.ends_with(">>") {
                class.stereotype = Some(
                    line.trim_matches(|c| c == '<' || c == '>')
                        .replace("<<", "")
                        .replace(">>", "")
                        .trim()
                        .to_string(),
                );
            } else if line.contains('(') {
                class.methods.push(clean_label(line.trim_end_matches(';')));
            } else {
                class.attributes.push(clean_label(line.trim_end_matches(';')));
            }
            continue;
        }
        // Relation or `Class : member` shorthand.
        if let Some((op_pos, op)) = find_relation(line) {
            let left = line[..op_pos].trim();
            let right_full = &line[op_pos + op.len()..];
            let (right, label) = match right_full.split_once(':') {
                Some((r, l)) => (r.trim(), l.trim()),
                None => (right_full.trim(), ""),
            };
            let from_name = relation_entity(left);
            let to_name = relation_entity(right);
            if from_name.is_empty() || to_name.is_empty() {
                continue;
            }
            let from = parsed.class(&from_name);
            let to = parsed.class(&to_name);
            parsed.relations.push(Relation {
                from,
                to,
                dashed: op.contains('.'),
                label: clean_label(label),
            });
            continue;
        }
        if let Some((name, member)) = line.split_once(':') {
            let name = name.trim();
            if !name.is_empty() && !member.trim().is_empty() {
                let idx = parsed.class(name);
                let member = clean_label(member.trim().trim_end_matches(';'));
                let class = &mut parsed.classes[idx];
                if member.contains('(') {
                    class.methods.push(member);
                } else {
                    class.attributes.push(member);
                }
                continue;
            }
        }
        if parsed.classes.is_empty() && line_no == 0 {
            continue;
        }
    }
    Ok(parsed)
}

fn find_relation(line: &str) -> Option<(usize, &str)> {
    const OPS: &[&str] = &[
        "<|--", "--|>", "*--", "o--", "-->", "..>", "..|>", "<..", "--*", "--o", "--",
    ];
    for op in OPS {
        if let Some(pos) = line.find(op) {
            // Make sure a shorter `--` does not match inside a longer operator.
            return Some((pos, op));
        }
    }
    None
}

fn relation_entity(text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return String::new();
    }
    // Cardinalities may appear as `"1"` or `1` before the entity.
    let mut tokens: Vec<&str> = t.split_whitespace().collect();
    while tokens.len() > 1
        && (tokens[0].starts_with('"')
            || tokens[0].chars().all(|c| c.is_ascii_digit() || c == '*' || c == 'n' || c == 'm'))
    {
        tokens.remove(0);
    }
    unquote(tokens.first().copied().unwrap_or(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_class_members_and_relations() {
        let src = "classDiagram\nclass Animal {\n  +String name\n  +eat()\n}\nclass Duck\nAnimal <|-- Duck : inherits\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.classes.len(), 2);
        assert_eq!(parsed.classes[0].attributes.len(), 1);
        assert_eq!(parsed.classes[0].methods.len(), 1);
        assert_eq!(parsed.relations.len(), 1);
        assert_eq!(parsed.relations[0].label, "inherits");
    }

    #[test]
    fn renders_to_scene() {
        let src = "classDiagram\nclass A\nclass B\nA --> B : uses\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 200.0);
        assert!(built.scene.items.len() > 4);
    }

    #[test]
    fn supports_persian_class_names() {
        let src = "classDiagram\nclass دانشجو {\n  +نام\n  +ثبت‌نام()\n}\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.height > 80.0);
    }

    #[test]
    fn supports_shorthand_members() {
        let src = "classDiagram\nA : +int x\nA : +run()\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.classes[0].attributes.len(), 1);
        assert_eq!(parsed.classes[0].methods.len(), 1);
    }
}