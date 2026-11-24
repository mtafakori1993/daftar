//! Gantt chart parser and renderer.

use chrono::{Datelike, Duration, NaiveDate};
use daftar_core::settings::TextAlign;

use crate::scene::{PathOp, SceneStroke, TextFactory};
use crate::{BuiltDiagram, DiagramError, DiagramKind, DiagramTheme, clean_label, padded_scene};

#[derive(Debug, Clone)]
struct Task {
    name: String,
    section: String,
    start: NaiveDate,
    end: NaiveDate,
    done: bool,
    crit: bool,
    milestone: bool,
}

struct ParsedGantt {
    title: Option<String>,
    date_format: String,
    tasks: Vec<Task>,
    warnings: Vec<String>,
}

pub fn build(
    source: &str,
    theme: &DiagramTheme,
    engine: &mut daftar_text::TextEngine,
) -> Result<BuiltDiagram, DiagramError> {
    let parsed = parse(source)?;
    if parsed.tasks.is_empty() {
        return Err(DiagramError::new("No tasks found in Gantt chart."));
    }
    let mut factory = TextFactory::new(engine);
    let title = parsed
        .title
        .as_ref()
        .map(|t| factory.layout(t, 16.0, 600, 900.0, TextAlign::Center, None));
    let labels: Vec<_> = parsed
        .tasks
        .iter()
        .map(|t| factory.layout(&t.name, 12.0, 400, 240.0, TextAlign::Left, None))
        .collect();
    let section_labels: Vec<_> = {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::new();
        for t in &parsed.tasks {
            if seen.insert(t.section.clone()) {
                out.push((
                    t.section.clone(),
                    factory.layout(&t.section, 12.5, 600, 260.0, TextAlign::Left, None),
                ));
            }
        }
        out
    };
    drop(factory);

    let min_date = parsed.tasks.iter().map(|t| t.start).min().unwrap();
    let max_date = parsed.tasks.iter().map(|t| t.end).max().unwrap();
    let total_days = (max_date - min_date).num_days().max(1) as f32;
    let label_width = labels
        .iter()
        .map(|l| l.width)
        .fold(0.0f32, f32::max)
        .clamp(110.0, 300.0);
    let timeline_width = (total_days * 24.0).clamp(600.0, 1600.0);
    let day_width = timeline_width / total_days;
    let margin = 30.0;
    let title_h = title.as_ref().map_or(0.0, |t| t.height + 14.0);
    let axis_h = 34.0;
    let row_h = 30.0;
    let header_h = title_h + axis_h;
    let sections = section_labels.len().max(1) as f32;
    let content_h = parsed.tasks.len() as f32 * row_h + sections * 26.0;
    let width = margin * 2.0 + label_width + 24.0 + timeline_width;
    let height = header_h + content_h + margin + 16.0;
    let mut scene = padded_scene(theme, width, height, 0.0);
    scene.background = Some(theme.background);

    if let Some(title) = &title {
        let x = (scene.width - title.width) * 0.5;
        scene.text(title.shaped.clone(), x.max(margin), 10.0, theme.text);
    }

    let grid_x = margin + label_width + 24.0;
    let grid_top = header_h;
    let grid_bottom = height - margin;
    // Alternating section backgrounds and grid.
    let mut y = grid_top;
    let step_days = if total_days > 400.0 {
        60
    } else if total_days > 120.0 {
        30
    } else if total_days > 45.0 {
        14
    } else {
        7
    };
    let mut day = 0i64;
    while day <= (max_date - min_date).num_days() {
        let x = grid_x + day as f32 * day_width;
        scene.line(
            x,
            grid_top,
            x,
            grid_bottom,
            SceneStroke {
                color: theme.grid,
                width: 1.0,
                dash: Some((3.0, 4.0)),
            },
        );
        let date = min_date + Duration::days(day);
        let text = date.format("%b %d").to_string();
        // Axis labels need a text factory; reuse a temporary engine-less path by drawing with
        // a fresh factory at the end is complex, so grid labels are drawn after layout below.
        let _ = text;
        day += step_days;
    }
    // Axis labels via a second factory.
    let mut factory = TextFactory::new(engine);
    let mut day = 0i64;
    while day <= (max_date - min_date).num_days() {
        let x = grid_x + day as f32 * day_width;
        let date = min_date + Duration::days(day);
        let label = factory.layout(
            &date.format("%b %d").to_string(),
            10.5,
            500,
            100.0,
            TextAlign::Center,
            None,
        );
        if x + label.width <= grid_x + timeline_width {
            scene.text(label.shaped.clone(), x, header_h - axis_h + 8.0, theme.muted);
        }
        day += step_days;
    }

    let mut cursor_y = grid_top;
    let mut current_section: Option<&String> = None;
    let mut task_index = 0usize;
    for (section_name, section_label) in &section_labels {
        if current_section != Some(section_name) {
            current_section = Some(section_name);
            scene.rect(
                margin,
                cursor_y,
                width - margin * 2.0,
                26.0,
                0.0,
                Some(theme.surface),
                None,
            );
            scene.text(
                section_label.shaped.clone(),
                margin + 6.0,
                cursor_y + 5.0,
                theme.muted,
            );
            cursor_y += 26.0;
        }
        while task_index < parsed.tasks.len() && parsed.tasks[task_index].section == *section_name {
            let task = &parsed.tasks[task_index];
            let label = &labels[task_index];
            scene.text(
                label.shaped.clone(),
                margin,
                cursor_y + (row_h - label.height) * 0.5,
                theme.text,
            );
            let start_day = (task.start - min_date).num_days() as f32;
            let end_day = (task.end - min_date).num_days() as f32 + 1.0;
            let x = grid_x + start_day * day_width;
            let w = ((end_day - start_day) * day_width).max(4.0);
            let bar_y = cursor_y + row_h * 0.5 - 8.0;
            if task.milestone {
                let cx = x + w * 0.5;
                let cy = bar_y + 8.0;
                scene.polygon(
                    vec![
                        (cx, cy - 9.0),
                        (cx + 9.0, cy),
                        (cx, cy + 9.0),
                        (cx - 9.0, cy),
                    ],
                    Some(if task.crit { theme.pie_palette[3] } else { theme.accent }),
                    None,
                );
            } else {
                let fill = if task.crit {
                    theme.pie_palette[3]
                } else if task.done {
                    theme.accent.mix(theme.node_fill, 0.35)
                } else {
                    theme.accent
                };
                scene.rect(x, bar_y, w, 16.0, 4.0, Some(fill), None);
            }
            cursor_y += row_h;
            task_index += 1;
        }
    }
    let _ = PathOp::Close;

    Ok(BuiltDiagram {
        kind: DiagramKind::Gantt,
        width: scene.width,
        height: scene.height,
        scene,
        warnings: parsed.warnings,
    })
}

fn parse(source: &str) -> Result<ParsedGantt, DiagramError> {
    let mut parsed = ParsedGantt {
        title: None,
        date_format: "%Y-%m-%d".to_string(),
        tasks: Vec::new(),
        warnings: Vec::new(),
    };
    let mut section = "Tasks".to_string();
    let mut ids: std::collections::HashMap<String, NaiveDate> = std::collections::HashMap::new();
    for (line_no, raw) in source.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "gantt" {
            continue;
        }
        if lower.starts_with("title ") {
            parsed.title = Some(line[6..].trim().to_string());
            continue;
        }
        if lower.starts_with("dateformat ") {
            parsed.date_format = map_date_format(line[11..].trim());
            continue;
        }
        if lower.starts_with("axisformat")
            || lower.starts_with("tickinterval")
            || lower.starts_with("todaymarker")
            || lower.starts_with("excludes")
            || lower.starts_with("includes")
            || lower.starts_with("weekday")
        {
            continue;
        }
        if lower.starts_with("section ") {
            section = line[8..].trim().to_string();
            continue;
        }
        let Some((name, rest)) = line.split_once(':') else {
            if line_no < 100 {
                parsed.warnings.push(format!("Ignored Gantt line {}.", line_no + 1));
            }
            continue;
        };
        let tokens: Vec<String> = rest.split(',').map(|t| t.trim().to_string()).collect();
        let mut start: Option<NaiveDate> = None;
        let mut end: Option<NaiveDate> = None;
        let mut done = false;
        let mut crit = false;
        let mut milestone = false;
        let mut id: Option<String> = None;
        for token in &tokens {
            let tl = token.to_ascii_lowercase();
            match tl.as_str() {
                "" => continue,
                "done" => {
                    done = true;
                    continue;
                }
                "active" => continue,
                "crit" => {
                    crit = true;
                    continue;
                }
                "milestone" => {
                    milestone = true;
                    done = true;
                    continue;
                }
                _ => {}
            }
            if let Some(dep) = tl.strip_prefix("after ") {
                if let Some(prev_end) = ids.get(dep.trim()) {
                    start = Some(*prev_end + Duration::days(1));
                }
                continue;
            }
            if let Some(date) = parse_date(token, &parsed.date_format) {
                if start.is_none() {
                    start = Some(date);
                } else if end.is_none() {
                    end = Some(date);
                }
                continue;
            }
            if let Some(days) = parse_duration(token) {
                if let Some(s) = start {
                    end = Some(s + Duration::days(days.max(0)));
                }
                continue;
            }
            if id.is_none() {
                id = Some(token.clone());
            }
        }
        let start = start.unwrap_or_else(|| chrono::Local::now().date_naive());
        let end = end.unwrap_or(start);
        let end = if end < start { start } else { end };
        if milestone {
            // zero-length milestone
        }
        if let Some(id) = &id {
            ids.insert(id.to_ascii_lowercase(), end);
        }
        parsed.tasks.push(Task {
            name: clean_label(name.trim()),
            section: section.clone(),
            start,
            end,
            done,
            crit,
            milestone,
        });
    }
    Ok(parsed)
}

fn map_date_format(input: &str) -> String {
    input
        .replace("YYYY", "%Y")
        .replace("YY", "%y")
        .replace("MM", "%m")
        .replace("DD", "%d")
        .replace(' ', " ")
}

fn parse_date(token: &str, format: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(token.trim(), format).ok()
}

fn parse_duration(token: &str) -> Option<i64> {
    let t = token.trim();
    let (number, unit) = t.split_at(t.find(|c: char| !c.is_ascii_digit())?);
    let value: i64 = number.parse().ok()?;
    match unit.trim().to_ascii_lowercase().as_str() {
        "d" => Some(value),
        "w" => Some(value * 7),
        "h" => Some((value / 24).max(1)),
        "m" => Some(value * 30),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    #[test]
    fn parses_tasks_and_durations() {
        let src = "gantt\n dateFormat YYYY-MM-DD\n title Plan\n section Design\n Wireframe :a1, 2024-01-01, 10d\n Mockup :after a1, 5d\n";
        let parsed = parse(src).unwrap();
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].end, NaiveDate::from_ymd_opt(2024, 1, 11).unwrap());
        assert_eq!(parsed.tasks[1].start, NaiveDate::from_ymd_opt(2024, 1, 12).unwrap());
    }

    #[test]
    fn renders_chart() {
        let src = "gantt\n dateFormat YYYY-MM-DD\n section Build\n A :2024-01-01, 5d\n B :2024-01-07, 3d\n";
        let mut engine = TextEngine::new(None, "en-US");
        let built = build(src, &DiagramTheme::light(), &mut engine).unwrap();
        assert!(built.width > 500.0);
        assert!(built.scene.items.len() > 5);
    }

    #[test]
    fn supports_persian_tasks() {
        let src = "gantt\n dateFormat YYYY-MM-DD\n section برنامه\n طراحی :2024-01-01, 5d\n";
        let mut engine = TextEngine::new(None, "fa-IR");
        let built = build(src, &DiagramTheme::dark(), &mut engine).unwrap();
        assert!(built.height > 100.0);
    }

    #[test]
    fn formats_are_mapped() {
        assert_eq!(map_date_format("YYYY-MM-DD"), "%Y-%m-%d");
        assert_eq!(parse_duration("3w"), Some(21));
    }
}
