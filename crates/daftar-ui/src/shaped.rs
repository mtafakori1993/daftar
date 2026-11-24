//! Shaped text widgets for user-provided content (titles, notes, quotes) so Persian/Arabic
//! text is rendered with correct joining and bidi direction everywhere in the application.

use daftar_core::direction::detect_direction;
use daftar_core::settings::TextAlign;
use daftar_text::{LayoutOptions, ShapedText, TextColors, TextEngine};
use egui::{Color32, Response, Sense, Ui};

#[derive(Debug, Clone)]
pub struct ShapedStyle {
    pub family: String,
    pub size: f32,
    pub weight: u16,
    pub line_height: f32,
    pub align: TextAlign,
    pub letter_spacing: f32,
}

impl ShapedStyle {
    pub fn body() -> Self {
        Self {
            family: "Vazirmatn".to_string(),
            size: 13.5,
            weight: 400,
            line_height: 1.55,
            align: TextAlign::Start,
            letter_spacing: 0.0,
        }
    }

    pub fn title() -> Self {
        Self {
            size: 17.0,
            weight: 600,
            line_height: 1.35,
            ..Self::body()
        }
    }

    pub fn small() -> Self {
        Self {
            size: 11.5,
            weight: 400,
            line_height: 1.4,
            ..Self::body()
        }
    }

    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }
}

/// Lay out and paint shaped text. `max_width` ≤ 0 means unbounded single-line width.
pub fn shaped_label(
    ui: &mut Ui,
    engine: &mut TextEngine,
    text: &str,
    style: &ShapedStyle,
    color: Color32,
    max_width: f32,
    sense: Sense,
) -> Response {
    if text.is_empty() {
        return ui.allocate_response(egui::Vec2::ZERO, sense);
    }
    let measured = layout(engine, text, style, max_width);
    let width = if max_width > 1.0 {
        max_width.min(ui.available_width().max(1.0))
    } else {
        measured
            .lines
            .iter()
            .map(|l| l.width)
            .fold(0.0f32, f32::max)
            .min(ui.available_width().max(1.0))
    };
    let height = measured.height;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    if ui.is_rect_visible(rect) {
        let colors = TextColors {
            text: color,
            link: color,
            code: color,
            code_background: None,
            selection: Color32::TRANSPARENT,
        };
        engine.paint(
            ui.ctx(),
            ui.painter(),
            &measured,
            rect.min,
            &colors,
            None,
            &[],
        );
    }
    response
}

/// Lay out text and return both the shaped result and its measured width.
pub fn shape(
    engine: &mut TextEngine,
    text: &str,
    style: &ShapedStyle,
    max_width: f32,
) -> (ShapedText, f32) {
    let shaped = layout(engine, text, style, max_width);
    let measured = shaped
        .lines
        .iter()
        .map(|l| l.width)
        .fold(0.0f32, f32::max);
    (shaped, measured)
}

fn layout(engine: &mut TextEngine, text: &str, style: &ShapedStyle, max_width: f32) -> ShapedText {
    let unbounded = max_width <= 1.0;
    let opts = LayoutOptions {
        width: if unbounded { 0.0 } else { max_width },
        family: style.family.clone(),
        size: style.size,
        weight: style.weight,
        line_height: style.line_height,
        letter_spacing: style.letter_spacing,
        align: style.align,
        align_start_rtl: detect_direction(text).is_rtl(),
        paragraph_direction: None,
        code_family: "JetBrains Mono".to_string(),
        code_size: style.size * 0.9,
        wrap: !unbounded,
        base_italic: false,
        base_code: false,
    };
    engine.layout_text(text, &opts, &daftar_core::markdown::SpanStyle::default())
}

/// Paint a shaped title with a click response and hover cursor.
pub fn shaped_title(
    ui: &mut Ui,
    engine: &mut TextEngine,
    text: &str,
    size: f32,
    weight: u16,
    color: Color32,
    max_width: f32,
) -> Response {
    let style = ShapedStyle::title().with_size(size).with_weight(weight);
    let response = shaped_label(ui, engine, text, &style, color, max_width, Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}
