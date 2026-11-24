//! Sequence diagram parser and renderer.

use daftar_core::settings::TextAlign;

use crate::scene::{Label, PathOp, Rgba8, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParticipantKind {
    Participant,
    Actor,
}

#[derive(Debug, Clone)]
struct Participant {
    id: String,
    label: String,
    kind: ParticipantKind,
}

#[derive(Debug, Clone)]
enum SeqItem {
    Message {
        from: usize,
        to: usize,
        label: Label,
        dashed: bool,
        self_message: bool,
    },
    Note {
        over: Vec<usize>,
        label: Label,
    },
    Fragment {
        kind: String,
        label: Label,
        sections: Vec<SeqSection>,
    },
}

#[derive(Debug, Clone)]
struct SeqSection {
    label: Option<Label>,
    items: Vec<SeqItem>,
}

struct PendingFragment {
    kind: String,
    label: String,
    sections: Vec<(Option<String>, Vec<RawItem>)>,
}

enum RawItem {
    Message {
        from: String,
        to: String,
        text: String,
        dashed: bool,
    },
    Note {
        over: Vec<String>,
        text: String,
    },
    Fragment {
        kind: String,
        label: String,
        sections: Vec<(Option<String>, Vec<RawItem>)>,
    },
}

struct ParsedSequence {
    participants: Vec<Participant>,
    index: std::collections::HashMap<String, usize>,
    items: Vec<RawItem>,
    title: Option<String>,
    autonumber: bool,
}

impl ParsedSequence {
    fn new() -> Self {
        Self {
            participants: Vec::new(),
            index: std::collections::HashMap::new(),
            items: Vec::new(),
            title: None,
            autonumber: false,
        }
    }

    fn participant(&mut self, id: &str, alias: Option<&str>, kind: ParticipantKind) -> usize {
        let id = id.trim().to_string();
        if let Some(existing) = self.index.get(&id).copied() {
            if let Some(alias) = alias {
                if !alias.trim().is_empty() {
                    self.participants[existing].label = alias.trim().to_string();
                }
            }
            return existing;
        }
        let idx = self.participants.len();
        self.participants.push(Participant {
            id: id.clone(),
            label: alias
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| id.clone()),
            kind,
        });
        self.index.insert(id, idx);
        idx
    }

    fn add_message(&mut self, from: &str, to: &str, text: &str, dashed: bool) -> Result<(), DiagramError> {
        let from = self.participant(from, None, ParticipantKind::Participant);
        let to = self.participant(to, None, ParticipantKind::Participant);
        push_raw(
            &mut self.items,
            RawItem::Message {
                from: from.to_string(),
                to: to.to_string(),
                text: clean_label(text),
                dashed,
            },
            &self.index,
        );
        // The raw item stores string IDs; using indices would break when aliases appear later.
        // Store IDs instead for simplicity and resolve at layout time.
        Ok(())
    }
}

fn push_raw(items: &mut Vec<RawItem>, item: RawItem, _index: &std::collections::HashMap<String, usize>) {
    items.push(item);
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let parsed = parse(source)?;
    if parsed.participants.is_empty() {
        return Err(DiagramError::new(
            "No participants found in sequence diagram.",
        ));
    }
    let mut factory = TextFactory::new(engine);
    let participant_labels: Vec<Label> = parsed
        .participants
        .iter()
        .map(|p| {
            factory.layout(
                &p.label,
                13.0,
                600,
                180.0,
                TextAlign::Center,
                None,
            )
        })
        .collect();
    let items: Vec<SeqItem> = {
        let mut resolver = Resolver {
            factory: &mut factory,
            index: &parsed.index,
        };
        parsed
            .items
            .iter()
            .map(|item| resolver.item(item))
            .collect::<Result<_, _>>()?
    };
    let title = parsed
        .title
        .as_ref()
        .map(|t| factory.layout(t, 15.0, 600, 600.0, TextAlign::Center, None));

    // Column geometry.
    let n = parsed.participants.len();
    let gap = 205.0f32;
    let margin = 34.0f32;
    let xs: Vec<f32> = (0..n)
        .map(|i| margin + 60.0 + i as f32 * gap)
        .collect();
    let actor_h = participant_labels
        .iter()
        .map(|l| l.height)
        .fold(0.0f32, f32::max)
        + 24.0;
    let title_h = title.as_ref().map_or(0.0, |t| t.height + 18.0);
    let header_top = 20.0 + title_h;
    let lifeline_top = header_top + actor_h + 16.0;

    let content_h = measure_items(&items);
    let footer = 28.0;
    let total_h = lifeline_top + content_h + footer;
    let width = xs.last().copied().unwrap_or(0.0) + 60.0 + margin;
    let mut scene = padded_scene(theme, width, total_h, 0.0);
    scene.background = Some(theme.background);

    if let Some(title) = &title {
        let x = (scene.width - title.width) * 0.5;
        scene.text(title.shaped.clone(), x.max(10.0), 14.0, theme.text);
    }

    // Lifelines.
    for x in &xs {
        scene.path(
            vec![
                PathOp::Move(*x, lifeline_top - 6.0),
                PathOp::Line(*x, total_h - footer * 0.5),
            ],
            None,
            Some(SceneStroke {
                color: theme.node_stroke,
                width: 1.0,
                dash: Some((5.0, 5.0)),
            }),
        );
    }

    // Actor boxes.
    for (i, label) in participant_labels.iter().enumerate() {
        let w = (label.width + 34.0).max(90.0);
        let x = xs[i] - w * 0.5;
        scene.rect(
            x,
            header_top,
            w,
            actor_h,
            6.0,
            Some(theme.node_fill),
            Some(SceneStroke::solid(theme.node_stroke, 1.2)),
        );
        if parsed.participants[i].kind == ParticipantKind::Actor {
            draw_actor_icon(&mut scene, x + 14.0, header_top + actor_h * 0.5, theme.accent);
        }
        scene.text(
            label.shaped.clone(),
            xs[i] - label.width * 0.5,
            header_top + (actor_h - label.height) * 0.5,
            theme.text,
        );
    }

    let mut cursor_y = lifeline_top + 8.0;
    draw_items(
        &mut scene,
        &items,
        &xs,
        &mut cursor_y,
        theme,
        parsed.autonumber,
        &mut 0,
    );

    Ok(BuiltDiagram {
        kind: DiagramKind::Sequence,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: vec![],
    })
}

fn parse(source: &str) -> Result<ParsedSequence, DiagramError> {
    let mut parsed = ParsedSequence::new();
    let mut stack: Vec<PendingFragment> = Vec::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "sequencediagram" {
            continue;
        }
        if lower.starts_with("title ") {
            parsed.title = Some(line[6..].trim().to_string());
            continue;
        }
        if lower == "autonumber" || lower.starts_with("autonumber ") {
            parsed.autonumber = true;
            continue;
        }
        if lower.starts_with("participant ") || lower.starts_with("actor ") {
            let kind = if lower.starts_with("actor ") {
                ParticipantKind::Actor
            } else {
                ParticipantKind::Participant
            };
            let rest = line.splitn(2, ' ').nth(1).unwrap_or("").trim();
            let (id, alias) = split_alias(rest);
            parsed.participant(&id, alias.as_deref(), kind);
            continue;
        }
        if lower.starts_with("create participant") || lower.starts_with("destroy ") {
            continue;
        }
        if lower.starts_with("note ") {
            if let Some((over, text)) = parse_note(line) {
                let mut ids = Vec::new();
                for id in over {
                    ids.push(parsed.participant(&id, None, ParticipantKind::Participant).to_string());
                }
                push_raw(
                    &mut parsed.items,
                    RawItem::Note { over: ids, text },
                    &parsed.index,
                );
            }
            continue;
        }
        if let Some((kind, label)) = fragment_start(&lower, line) {
            stack.push(PendingFragment {
                kind,
                label,
                sections: vec![(None, Vec::new())],
            });
            continue;
        }
        if lower == "else" || lower.starts_with("else ") || lower.starts_with("and ") {
            if let Some(top) = stack.last_mut() {
                let label = line.splitn(2, ' ').nth(1).map(|s| s.trim().to_string());
                top.sections.push((label, Vec::new()));
            }
            continue;
        }
        if lower == "end" {
            if let Some(fragment) = stack.pop() {
                let raw = RawItem::Fragment {
                    kind: fragment.kind,
                    label: fragment.label,
                    sections: fragment.sections,
                };
                append_raw(&mut parsed.items, &mut stack, raw);
            }
            continue;
        }
        if lower.starts_with("activate ")
            || lower.starts_with("deactivate ")
            || lower.starts_with("box ")
            || lower.starts_with("rect ")
            || lower.starts_with("critical ")
            || lower.starts_with("break ")
            || lower.starts_with("par ")
        {
            // Containers without dedicated sequence semantics are ignored safely.
            continue;
        }
        // Message: find an arrow operator before the colon.
        if let Some((head, text)) = line.split_once(':') {
            if let Some((from, to, dashed)) = parse_arrow(head) {
                let from_idx = parsed.participant(&from, None, ParticipantKind::Participant);
                let to_idx = parsed.participant(&to, None, ParticipantKind::Participant);
                let raw = RawItem::Message {
                    from: from_idx.to_string(),
                    to: to_idx.to_string(),
                    text: clean_label(text),
                    dashed,
                };
                append_raw(&mut parsed.items, &mut stack, raw);
                continue;
            }
        }
        if stack.is_empty() {
            return Err(DiagramError::at(
                line_no + 1,
                format!("Could not parse sequence statement `{line}`."),
            ));
        }
    }
    // Unclosed fragments are tolerated but their content is preserved at the root.
    while let Some(fragment) = stack.pop() {
        let raw = RawItem::Fragment {
            kind: fragment.kind,
            label: fragment.label,
            sections: fragment.sections,
        };
        parsed.items.push(raw);
    }
    Ok(parsed)
}

fn append_raw(items: &mut Vec<RawItem>, stack: &mut [PendingFragment], raw: RawItem) {
    if let Some(top) = stack.last_mut() {
        if let Some(section) = top.sections.last_mut() {
            section.1.push(raw);
        }
    } else {
        items.push(raw);
    }
}

fn fragment_start(lower: &str, original: &str) -> Option<(String, String)> {
    for kind in ["loop", "alt", "opt", "par", "critical", "break", "rect", "box"] {
        if lower.starts_with(kind) {
            let rest = original[kind.len()..].trim();
            return Some((kind.to_string(), rest.to_string()));
        }
    }
    None
}

fn split_alias(rest: &str) -> (String, Option<String>) {
    if let Some((id, alias)) = rest.split_once(" as ") {
        (id.trim().to_string(), Some(alias.trim().to_string()))
    } else {
        (rest.trim().to_string(), None)
    }
}

fn parse_arrow(head: &str) -> Option<(String, String, bool)> {
    const OPS: &[(&str, bool)] = &[
        ("-->>", true),
        ("->>", false),
        ("-->", true),
        ("->", false),
        ("--x", true),
        ("-x", false),
        ("--)", true),
        ("-)", false),
        ("--)", true),
    ];
    for (op, dashed) in OPS {
        if let Some(pos) = head.find(op) {
            let from = head[..pos].trim().to_string();
            let to = head[pos + op.len()..].trim().to_string();
            if !from.is_empty() && !to.is_empty() {
                return Some((from, to, *dashed));
            }
        }
    }
    None
}

fn parse_note(line: &str) -> Option<(Vec<String>, String)> {
    let after = line.get(4..)?.trim_start();
    let (head, text) = after.split_once(':')?;
    let head_lower = head.trim().to_ascii_lowercase();
    let target = if head_lower.starts_with("note over ") {
        head.trim()[5..].to_string()
    } else if head_lower.starts_with("over ") {
        head.trim()[5..].to_string()
    } else if head_lower.starts_with("left of ") {
        head.trim()[8..].to_string()
    } else if head_lower.starts_with("right of ") {
        head.trim()[9..].to_string()
    } else {
        return None;
    };
    let ids: Vec<String> = target
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Some((ids, clean_label(text)))
}

struct Resolver<'a, 'b> {
    factory: &'a mut TextFactory<'b>,
    index: &'a std::collections::HashMap<String, usize>,
}

impl Resolver<'_, '_> {
    fn item(&mut self, raw: &RawItem) -> Result<SeqItem, DiagramError> {
        match raw {
            RawItem::Message {
                from,
                to,
                text,
                dashed,
            } => {
                let from = from.parse::<usize>().unwrap_or(0);
                let to = to.parse::<usize>().unwrap_or(0);
                let label = self.factory.layout(
                    text,
                    12.5,
                    400,
                    220.0,
                    TextAlign::Center,
                    None,
                );
                Ok(SeqItem::Message {
                    from,
                    to,
                    label,
                    dashed: *dashed,
                    self_message: from == to,
                })
            }
            RawItem::Note { over, text } => {
                let over: Vec<usize> = over
                    .iter()
                    .filter_map(|id| self.index.get(id).copied())
                    .collect();
                let label = self.factory.layout(
                    text,
                    12.0,
                    400,
                    240.0,
                    TextAlign::Left,
                    None,
                );
                Ok(SeqItem::Note { over, label })
            }
            RawItem::Fragment {
                kind,
                label,
                sections,
            } => {
                let label = self.factory.layout(
                    label,
                    12.0,
                    600,
                    320.0,
                    TextAlign::Left,
                    None,
                );
                let mut out_sections = Vec::new();
                for (sec_label, items) in sections {
                    let sec_label = sec_label.as_ref().map(|l| {
                        self.factory
                            .layout(l, 11.5, 600, 260.0, TextAlign::Left, None)
                    });
                    let mut out_items = Vec::new();
                    for item in items {
                        out_items.push(self.item(item)?);
                    }
                    out_sections.push(SeqSection {
                        label: sec_label,
                        items: out_items,
                    });
                }
                Ok(SeqItem::Fragment {
                    kind: kind.clone(),
                    label,
                    sections: out_sections,
                })
            }
        }
    }
}

fn measure_items(items: &[SeqItem]) -> f32 {
    let mut total = 0.0;
    for item in items {
        total += match item {
            SeqItem::Message { label, self_message, .. } => {
                label.height + if *self_message { 54.0 } else { 36.0 }
            }
            SeqItem::Note { label, .. } => label.height + 34.0,
            SeqItem::Fragment { sections, .. } => {
                32.0 + sections
                    .iter()
                    .map(|s| {
                        let header = s.label.as_ref().map_or(0.0, |l| l.height + 14.0);
                        header + measure_items(&s.items) + 12.0
                    })
                    .sum::<f32>()
                    + 20.0
            }
        };
    }
    total
}

fn draw_items(
    scene: &mut crate::Scene,
    items: &[SeqItem],
    xs: &[f32],
    cursor_y: &mut f32,
    theme: &DiagramTheme,
    autonumber: bool,
    message_no: &mut usize,
) {
    let left = xs.first().copied().unwrap_or(30.0) - 70.0;
    let right = xs.last().copied().unwrap_or(300.0) + 70.0;
    for item in items {
        match item {
            SeqItem::Message {
                from,
                to,
                label,
                dashed,
                self_message,
            } => {
                let x1 = xs.get(*from).copied().unwrap_or(left);
                let x2 = xs.get(*to).copied().unwrap_or(right);
                if *self_message {
                    let y = *cursor_y + label.height + 6.0;
                    let lw = 54.0;
                    scene.path(
                        vec![
                            PathOp::Move(x1, y),
                            PathOp::Cubic(x1 + lw, y, x1 + lw, y + 26.0, x1, y + 26.0),
                        ],
                        None,
                        Some(if *dashed {
                            SceneStroke::dashed(theme.line, 1.3)
                        } else {
                            SceneStroke::solid(theme.line, 1.3)
                        }),
                    );
                    scene.arrowhead((x1 + 18.0, y + 25.0), (x1 + 1.0, y + 26.0), theme.line, 8.0);
                    scene.text(
                        label.shaped.clone(),
                        x1 + lw + 10.0,
                        *cursor_y,
                        theme.text,
                    );
                    *cursor_y += label.height + 54.0;
                } else {
                    let center = (x1 + x2) * 0.5;
                    let text_x = center - label.width * 0.5;
                    scene.rect(
                        text_x - 4.0,
                        *cursor_y - 1.0,
                        label.width + 8.0,
                        label.height + 2.0,
                        2.0,
                        Some(theme.background),
                        None,
                    );
                    scene.text(label.shaped.clone(), text_x, *cursor_y, theme.text);
                    let y = *cursor_y + label.height + 12.0;
                    let direction = if x2 >= x1 { 1.0 } else { -1.0 };
                    let start_x = x1 + direction * 2.0;
                    let end_x = x2 - direction * 8.0;
                    let stroke = if *dashed {
                        SceneStroke::dashed(theme.line, 1.3)
                    } else {
                        SceneStroke::solid(theme.line, 1.4)
                    };
                    scene.line(start_x, y, end_x, y, stroke);
                    scene.arrowhead(
                        (end_x - direction * 10.0, y),
                        (end_x + direction * 2.0, y),
                        theme.line,
                        9.0,
                    );
                    *cursor_y += label.height + 36.0;
                }
            }
            SeqItem::Note { over, label } => {
                let first = over.first().and_then(|i| xs.get(*i)).copied().unwrap_or(left);
                let last = over.last().and_then(|i| xs.get(*i)).copied().unwrap_or(first);
                let w = (label.width + 28.0).max(120.0);
                let h = label.height + 22.0;
                let x = ((first + last) * 0.5 - w * 0.5).max(10.0);
                let y = *cursor_y;
                scene.rect(
                    x,
                    y,
                    w,
                    h,
                    2.0,
                    Some(theme.note_fill),
                    Some(SceneStroke::solid(theme.note_stroke, 1.1)),
                );
                let fold = 12.0;
                scene.line(x + w - fold, y, x + w - fold, y + fold, SceneStroke::solid(theme.note_stroke, 1.0));
                scene.line(x + w - fold, y + fold, x + w, y + fold, SceneStroke::solid(theme.note_stroke, 1.0));
                scene.text(label.shaped.clone(), x + 10.0, y + 11.0, theme.text);
                *cursor_y += h + 12.0;
            }
            SeqItem::Fragment {
                kind,
                label,
                sections,
            } => {
                let height = 32.0
                    + sections
                        .iter()
                        .map(|s| {
                            s.label.as_ref().map_or(0.0, |l| l.height + 14.0)
                                + measure_items(&s.items)
                                + 12.0
                        })
                        .sum::<f32>()
                    + 20.0;
                let x = left;
                let w = (right - left).max(200.0);
                let y = *cursor_y;
                scene.rect(
                    x,
                    y,
                    w,
                    height,
                    4.0,
                    Some(theme.surface),
                    Some(SceneStroke::solid(theme.muted, 1.0)),
                );
                scene.rect(
                    x,
                    y,
                    (label.width + 26.0).max(70.0),
                    label.height + 10.0,
                    3.0,
                    Some(theme.node_fill),
                    Some(SceneStroke::solid(theme.muted, 1.0)),
                );
                scene.text(label.shaped.clone(), x + 9.0, y + 5.0, theme.text);
                let _ = kind;
                *cursor_y += label.height + 18.0;
                for (i, section) in sections.iter().enumerate() {
                    if i > 0 {
                        scene.line(
                            x,
                            *cursor_y - 5.0,
                            x + w,
                            *cursor_y - 5.0,
                            SceneStroke {
                                color: theme.muted,
                                width: 1.0,
                                dash: Some((4.0, 4.0)),
                            },
                        );
                    }
                    if let Some(sec) = &section.label {
                        scene.text(sec.shaped.clone(), x + 12.0, *cursor_y, theme.muted);
                        *cursor_y += sec.height + 8.0;
                    }
                    draw_items(
                        scene,
                        &section.items,
                        xs,
                        cursor_y,
                        theme,
                        autonumber,
                        message_no,
                    );
                    *cursor_y += 8.0;
                }
                *cursor_y += 10.0;
            }
        }
    }
}

fn draw_actor_icon(scene: &mut crate::Scene, x: f32, y: f32, color: Rgba8) {
    scene.ellipse(x, y - 7.0, 3.5, 3.5, Some(color), None);
    scene.line(x, y - 3.0, x, y + 5.0, SceneStroke::solid(color, 1.4));
    scene.line(x - 5.0, y, x + 5.0, y, SceneStroke::solid(color, 1.4));
    scene.line(x, y + 5.0, x - 4.0, y + 11.0, SceneStroke::solid(color, 1.4));
    scene.line(x, y + 5.0, x + 4.0, y + 11.0, SceneStroke::solid(color, 1.4));
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_and_renders_basic_sequence() {
        let src = "sequenceDiagram\nparticipant A as Alice\nparticipant B as Bob\nA->>B: Hello\nB-->>A: Hi\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 200.0);
        assert!(!built.scene.items.is_empty());
    }

    #[test]
    fn supports_notes_and_fragments() {
        let src = "sequenceDiagram\nA->>B: start\nalt success\nA->>B: ok\nelse failure\nB-->>A: error\nend\nNote over A,B: done\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.height > 200.0);
    }

    #[test]
    fn supports_persian() {
        let src = "sequenceDiagram\nمشتری->>سرور: درخواست\nسرور-->>مشتری: پاسخ\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 100.0);
    }

    #[test]
    fn self_messages_render() {
        let src = "sequenceDiagram\nA->>A: think\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn exports_png() {
        let src = "sequenceDiagram\nA->>B: hi\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        let png = built.encode_png(&mut engine, 2.0).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
}