//! Pie chart parser and renderer.

use std::f32::consts::TAU;

use daftar_core::settings::TextAlign;

use crate::scene::{PathOp, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, padded_scene, unquote};

struct Slice {
    label: String,
    value: f32,
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let (title, slices) = parse(source)?;
    if slices.is_empty() {
        return Err(DiagramError::new("Pie chart contains no data."));
    }
    let total: f32 = slices.iter().map(|s| s.value.max(0.0)).sum();
    if total <= 0.0 {
        return Err(DiagramError::new("Pie chart values must be positive."));
    }
    let mut factory = TextFactory::new(engine);
    let title_label = title
        .as_ref()
        .map(|t| factory.layout(t, 16.0, 600, 700.0, TextAlign::Center, None));
    let legend: Vec<_> = slices
        .iter()
        .map(|s| {
            factory.layout(
                &format!("{}  {:.0}%", s.label, s.value / total * 100.0),
                12.0,
                400,
                280.0,
                TextAlign::Left,
                None,
            )
        })
        .collect();
    let pct_labels: Vec<_> = slices
        .iter()
        .map(|s| {
            factory.layout(
                &format!("{:.0}%", s.value / total * 100.0),
                10.5,
                600,
                60.0,
                TextAlign::Center,
                None,
            )
        })
        .collect();
    drop(factory);

    let margin = 30.0;
    let radius = 118.0;
    let title_h = title_label.as_ref().map_or(0.0, |t| t.height + 16.0);
    let legend_w = legend.iter().map(|l| l.width).fold(150.0f32, f32::max) + 54.0;
    let width = margin * 2.0 + radius * 2.0 + legend_w;
    let height = (margin * 2.0 + radius * 2.0 + title_h).max(legend.len() as f32 * 26.0 + title_h + 40.0);
    let mut scene = padded_scene(theme, width, height, 0.0);
    scene.background = Some(theme.background);

    if let Some(title) = &title_label {
        let x = (width - title.width) * 0.5;
        scene.text(title.shaped.clone(), x, 10.0, theme.text);
    }

    let cx = margin + radius;
    let cy = title_h + radius + 8.0;
    let mut angle = -TAU / 4.0;
    for (i, slice) in slices.iter().enumerate() {
        let sweep = (slice.value.max(0.0) / total) * TAU;
        let color = theme.pie_palette[i % theme.pie_palette.len()];
        let steps = ((sweep / TAU) * 120.0).ceil().max(3.0) as usize;
        let mut points = vec![(cx, cy)];
        for step in 0..=steps {
            let a = angle + sweep * step as f32 / steps as f32;
            points.push((cx + radius * a.cos(), cy + radius * a.sin()));
        }
        scene.polygon(points, Some(color), Some(SceneStroke::solid(theme.background, 1.0)));
        if slice.value / total > 0.045 {
            let mid = angle + sweep * 0.5;
            let label = &pct_labels[i];
            let lx = cx + radius * 0.62 * mid.cos() - label.width * 0.5;
            let ly = cy + radius * 0.62 * mid.sin() - label.height * 0.5;
            scene.text(label.shaped.clone(), lx, ly, theme.background);
        }
        angle += sweep;
    }
    // Donut hole for a cleaner Spectrum-like look.
    scene.ellipse(
        cx,
        cy,
        radius * 0.38,
        radius * 0.38,
        Some(theme.background),
        None,
    );

    let legend_x = margin + radius * 2.0 + 22.0;
    let legend_y = title_h + 16.0;
    for (i, label) in legend.iter().enumerate() {
        let y = legend_y + i as f32 * 26.0;
        let color = theme.pie_palette[i % theme.pie_palette.len()];
        scene.rect(legend_x, y + 4.0, 10.0, 10.0, 2.0, Some(color), None);
        scene.text(label.shaped.clone(), legend_x + 18.0, y, theme.text);
    }
    let _ = PathOp::Close;

    Ok(BuiltDiagram {
        kind: DiagramKind::Pie,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: Vec::new(),
    })
}

fn parse(source: &str) -> Result<(Option<String>, Vec<Slice>), DiagramError> {
    let mut title = None;
    let mut slices = Vec::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "pie" || lower.starts_with("pie ") {
            // `pie showData` and `pie title ...`
            if let Some(pos) = lower.find("title ") {
                title = Some(line[pos + 6..].trim().to_string());
            }
            continue;
        }
        if lower.starts_with("title ") {
            title = Some(line[6..].trim().to_string());
            continue;
        }
        let Some((label, value)) = line.rsplit_once(':') else {
            return Err(DiagramError::at(
                line_no + 1,
                "Expected `\"Label\" : value` in pie chart.",
            ));
        };
        let label = unquote(label.trim());
        let value: f32 = value.trim().parse().map_err(|_| {
            DiagramError::at(line_no + 1, format!("Invalid pie value `{}`.", value.trim()))
        })?;
        if label.is_empty() {
            return Err(DiagramError::at(line_no + 1, "Pie slice label is empty."));
        }
        slices.push(Slice { label, value });
    }
    Ok((title, slices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_slices() {
        let (title, slices) = parse("pie title Pets\n\"Dogs\" : 3\n\"Cats\" : 2\n").unwrap();
        assert_eq!(title.as_deref(), Some("Pets"));
        assert_eq!(slices.len(), 2);
    }

    #[test]
    fn renders_pie() {
        let src = "pie\n\"Cats\" : 40\n\"Dogs\" : 60\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 300.0);
        assert!(built.scene.items.len() > 5);
    }

    #[test]
    fn supports_persian_labels() {
        let src = "pie title آمار\n\"گربه\" : 40\n\"سگ\" : 60\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.width > 300.0);
    }

    #[test]
    fn rejects_bad_values() {
        let mut engine = TextEngine::new(None, "en-US");
        assert!(build("pie\n\"A\" : many\n", &DiagramTheme::light(), &mut engine).is_err());
    }
}
