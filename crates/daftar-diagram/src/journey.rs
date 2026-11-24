//! User journey diagram parser and renderer.

use daftar_core::settings::TextAlign;

use crate::scene::{Rgba8, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene};

#[derive(Debug, Clone)]
struct Step {
    label: String,
    score: u8,
    actor: String,
}

#[derive(Debug, Clone)]
struct Section {
    label: String,
    steps: Vec<Step>,
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let (title, sections) = parse(source)?;
    if sections.is_empty() {
        return Err(DiagramError::new("Journey has no steps."));
    }
    let mut factory = TextFactory::new(engine);
    let title_label = title
        .as_ref()
        .map(|t| factory.layout(t, 16.0, 600, 720.0, TextAlign::Center, None));
    let section_labels: Vec<_> = sections
        .iter()
        .map(|s| factory.layout(&s.label, 12.5, 600, 240.0, TextAlign::Left, None))
        .collect();
    let step_labels: Vec<Vec<_>> = sections
        .iter()
        .map(|s| {
            s.steps
                .iter()
                .map(|step| factory.layout(&step.label, 12.0, 500, 250.0, TextAlign::Left, None))
                .collect()
        })
        .collect();
    let actor_labels: Vec<Vec<_>> = sections
        .iter()
        .map(|s| {
            s.steps
                .iter()
                .map(|step| factory.layout(&step.actor, 11.0, 400, 140.0, TextAlign::Left, None))
                .collect()
        })
        .collect();
    drop(factory);

    let margin = 32.0;
    let title_h = title_label.as_ref().map_or(0.0, |t| t.height + 16.0);
    let row_h = 40.0;
    let section_h = 30.0;
    let width = 860.0;
    let content_h: f32 = sections
        .iter()
        .map(|s| section_h + s.steps.len() as f32 * row_h)
        .sum();
    let height = margin * 2.0 + title_h + content_h;
    let mut scene = padded_scene(theme, width, height, 0.0);
    scene.background = Some(theme.background);
    if let Some(title) = &title_label {
        let x = (width - title.width) * 0.5;
        scene.text(title.shaped.clone(), x, margin * 0.5, theme.text);
    }

    let label_x = margin + 8.0;
    let score_x = margin + 320.0;
    let actor_x = margin + 600.0;
    let mut y = margin + title_h;
    for (section_idx, section) in sections.iter().enumerate() {
        scene.rect(
            margin,
            y,
            width - margin * 2.0,
            section_h,
            0.0,
            Some(theme.accent.mix(theme.surface, 0.88)),
            None,
        );
        let section_label = &section_labels[section_idx];
        scene.text(
            section_label.shaped.clone(),
            label_x,
            y + (section_h - section_label.height) * 0.5,
            theme.text,
        );
        y += section_h;
        let score_center_y = y + row_h * 0.5;
        scene.line(
            score_x + 6.0,
            score_center_y,
            score_x + 5.0 * 26.0,
            score_center_y,
            SceneStroke::solid(theme.grid, 2.0),
        );
        for (step_idx, step) in section.steps.iter().enumerate() {
            if step_idx % 2 == 1 {
                scene.rect(
                    margin,
                    y,
                    width - margin * 2.0,
                    row_h,
                    0.0,
                    Some(theme.surface),
                    None,
                );
            }
            let label = &step_labels[section_idx][step_idx];
            scene.text(
                label.shaped.clone(),
                label_x,
                y + (row_h - label.height) * 0.5,
                theme.text,
            );
            for dot in 0..5u8 {
                let cx = score_x + dot as f32 * 26.0 + 8.0;
                let filled = dot < step.score;
                let color = score_color(dot + 1, theme);
                scene.ellipse(
                    cx,
                    score_center_y,
                    7.0,
                    7.0,
                    if filled { Some(color) } else { None },
                    Some(SceneStroke::solid(color, 1.4)),
                );
            }
            let actor = &actor_labels[section_idx][step_idx];
            if !step.actor.is_empty() {
                scene.text(
                    actor.shaped.clone(),
                    actor_x,
                    y + (row_h - actor.height) * 0.5,
                    theme.muted,
                );
            }
            scene.line(
                margin,
                y + row_h,
                width - margin,
                y + row_h,
                SceneStroke {
                    color: theme.grid,
                    width: 1.0,
                    dash: Some((3.0, 3.0)),
                },
            );
            y += row_h;
        }
    }
    let _ = Rgba8::rgb(0, 0, 0);

    Ok(BuiltDiagram {
        kind: DiagramKind::Journey,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: Vec::new(),
    })
}

fn score_color(score: u8, theme: &DiagramTheme) -> Rgba8 {
    let palette = &theme.pie_palette;
    match score {
        1 => palette[3 % palette.len()],
        2 => palette[1 % palette.len()],
        3 => palette[6 % palette.len()],
        4 => palette[2 % palette.len()],
        _ => palette[2 % palette.len()].mix(palette[0], 0.4),
    }
}

fn parse(source: &str) -> Result<(Option<String>, Vec<Section>), DiagramError> {
    let mut title = None;
    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<usize> = None;
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "journey" || lower == "userjourney" {
            continue;
        }
        if lower.starts_with("title ") {
            title = Some(line[6..].trim().to_string());
            continue;
        }
        if lower.starts_with("section ") {
            sections.push(Section {
                label: clean_label(line[8..].trim()),
                steps: Vec::new(),
            });
            current = Some(sections.len() - 1);
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, ':').collect();
        if parts.len() < 2 {
            return Err(DiagramError::at(
                line_no + 1,
                format!("Expected `Task: score: Actor` in journey, got `{line}`."),
            ));
        }
        let label = clean_label(parts[0].trim());
        let score: u8 = parts[1]
            .trim()
            .parse::<u8>()
            .map_err(|_| DiagramError::at(line_no + 1, "Journey score must be 1–5."))?
            .clamp(1, 5);
        let actor = parts.get(2).map(|s| clean_label(s.trim())).unwrap_or_default();
        if current.is_none() {
            sections.push(Section {
                label: "Journey".to_string(),
                steps: Vec::new(),
            });
            current = Some(0);
        }
        sections[current.unwrap()].steps.push(Step {
            label,
            score,
            actor,
        });
    }
    Ok((title, sections))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_journey_steps() {
        let (title, sections) = parse("journey\ntitle Day\nsection Work\nWake: 3: Me\nCode: 5: Me\n").unwrap();
        assert_eq!(title.as_deref(), Some("Day"));
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].steps.len(), 2);
        assert_eq!(sections[0].steps[1].score, 5);
    }

    #[test]
    fn renders_journey() {
        let src = "journey\nsection Morning\nCoffee: 4: Me\nWork: 2: Me\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 500.0);
        assert!(built.scene.items.len() > 10);
    }

    #[test]
    fn supports_persian() {
        let src = "journey\nsection صبح\nقهوه: 4: من\nکار: 2: من\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn rejects_bad_score() {
        let mut engine = TextEngine::new(None, "en-US");
        assert!(build("journey\nA: ten: me\n", &DiagramTheme::light(), &mut engine).is_err());
    }
}
