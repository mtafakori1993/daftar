//! Study-card layout shared by PNG export and vector PDF export.
//!
//! Card designs are rendered from a layout model, never by screenshotting the application.
//! Text is shaped with the reader's bidirectional engine, so Persian/Arabic joins, mixed
//! direction lines and RTL alignment are correct in both PNG and PDF output.

use daftar_core::direction::{detect_direction, ResolvedDirection};
use daftar_core::model::{Bookmark, Flashcard, Highlight};
use daftar_core::settings::TextAlign;
use daftar_text::{LayoutOptions, ShapedText, TextEngine};

use crate::error::{ExportError, Result};
use crate::Rgba8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKind {
    Flashcard,
    Highlight,
    Bookmark,
}

impl CardKind {
    pub fn label(self) -> &'static str {
        match self {
            CardKind::Flashcard => "FLASHCARD",
            CardKind::Highlight => "HIGHLIGHT",
            CardKind::Bookmark => "BOOKMARK",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CardContent {
    pub kind: CardKind,
    /// Front / question / selected quote.
    pub prompt: String,
    /// Back / answer / note.
    pub answer: Option<String>,
    /// Extra explanation, shown in smaller muted type.
    pub note: Option<String>,
    /// Source document title or relative path.
    pub source: Option<String>,
    pub tags: Vec<String>,
    /// Optional small label on the right of the header ("FRONT", "BACK").
    pub side_label: Option<String>,
    pub include_back: bool,
}

impl CardContent {
    pub fn flashcard(card: &Flashcard, source_title: Option<&str>) -> Self {
        Self {
            kind: CardKind::Flashcard,
            prompt: card.front.clone(),
            answer: Some(card.back.clone()),
            note: (!card.explanation.trim().is_empty()).then(|| card.explanation.clone()),
            source: source_title.map(|s| s.to_string()),
            tags: card.tags.clone(),
            side_label: Some("FRONT / BACK".to_string()),
            include_back: true,
        }
    }

    pub fn highlight(highlight: &Highlight, source_title: Option<&str>) -> Self {
        Self {
            kind: CardKind::Highlight,
            prompt: highlight.quote.clone(),
            answer: (!highlight.note.trim().is_empty()).then(|| highlight.note.clone()),
            note: None,
            source: source_title.map(|s| s.to_string()),
            tags: highlight.tags.clone(),
            side_label: Some("HIGHLIGHT".to_string()),
            include_back: !highlight.note.trim().is_empty(),
        }
    }

    pub fn bookmark(bookmark: &Bookmark, source_title: Option<&str>) -> Self {
        Self {
            kind: CardKind::Bookmark,
            prompt: if bookmark.quote.trim().is_empty() {
                bookmark.label.clone()
            } else {
                bookmark.quote.clone()
            },
            answer: (!bookmark.note.trim().is_empty()).then(|| bookmark.note.clone()),
            note: None,
            source: source_title.map(|s| s.to_string()),
            tags: Vec::new(),
            side_label: Some("BOOKMARK".to_string()),
            include_back: !bookmark.note.trim().is_empty(),
        }
    }
}

/// Visual card style. Dimensions are PDF points (1/72 inch); PNG export multiplies by `scale`.
#[derive(Debug, Clone)]
pub struct CardStyle {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub background: Rgba8,
    pub foreground: Rgba8,
    pub muted: Rgba8,
    pub accent: Rgba8,
    pub border: Rgba8,
    pub divider: Rgba8,
    pub font_family: String,
    pub heading_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub letter_spacing: f32,
    pub padding: f32,
    pub radius: f32,
    pub show_source: bool,
    pub show_brand: bool,
    pub dark: bool,
}

impl Default for CardStyle {
    fn default() -> Self {
        Self::light()
    }
}

impl CardStyle {
    pub fn light() -> Self {
        Self {
            width: 480.0,
            height: 320.0,
            scale: 2.0,
            background: Rgba8::from_hex(0xFFFFFF),
            foreground: Rgba8::from_hex(0x1F1F1F),
            muted: Rgba8::from_hex(0x6E6E6E),
            accent: Rgba8::from_hex(0x2680EB),
            border: Rgba8::from_hex(0xE1E1E1),
            divider: Rgba8::from_hex(0xE8E8E8),
            font_family: "Vazirmatn".to_string(),
            heading_family: "Vazirmatn".to_string(),
            font_size: 21.0,
            font_weight: 500,
            line_height: 1.45,
            letter_spacing: 0.0,
            padding: 34.0,
            radius: 14.0,
            show_source: true,
            show_brand: true,
            dark: false,
        }
    }

    pub fn dark() -> Self {
        Self {
            background: Rgba8::from_hex(0x1E1E1E),
            foreground: Rgba8::from_hex(0xEDEDED),
            muted: Rgba8::from_hex(0xA0A0A0),
            accent: Rgba8::from_hex(0x6BB8FF),
            border: Rgba8::from_hex(0x3A3A3A),
            divider: Rgba8::from_hex(0x333333),
            dark: true,
            ..Self::light()
        }
    }

    pub fn print() -> Self {
        Self {
            scale: 3.0,
            background: Rgba8::from_hex(0xFFFFFF),
            foreground: Rgba8::from_hex(0x111111),
            muted: Rgba8::from_hex(0x555555),
            accent: Rgba8::from_hex(0x1A1A1A),
            border: Rgba8::from_hex(0xDDDDDD),
            ..Self::light()
        }
    }

    pub fn validate(&mut self) {
        self.width = self.width.clamp(240.0, 1600.0);
        self.height = self.height.clamp(180.0, 1200.0);
        self.scale = self.scale.clamp(1.0, 6.0);
        self.font_size = self.font_size.clamp(10.0, 64.0);
        self.padding = self.padding.clamp(12.0, 120.0);
        self.radius = self.radius.clamp(0.0, 60.0);
        self.line_height = self.line_height.clamp(1.1, 2.4);
    }
}

#[derive(Debug, Clone)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub fill: Option<Rgba8>,
    pub stroke: Option<Rgba8>,
    pub stroke_width: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutLine {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub color: Rgba8,
    pub width: f32,
}

#[derive(Debug, Clone)]
pub struct LayoutText {
    pub shaped: ShapedText,
    /// Original text, retained for PDF ToUnicode maps.
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub color: Rgba8,
}

#[derive(Debug, Clone)]
pub struct CardLayout {
    pub width: f32,
    pub height: f32,
    pub background: Rgba8,
    pub rects: Vec<LayoutRect>,
    pub lines: Vec<LayoutLine>,
    pub texts: Vec<LayoutText>,
    pub direction: ResolvedDirection,
}

/// Lay out a card at 1x point dimensions.
pub fn layout_card(content: &CardContent, style: &CardStyle, engine: &mut TextEngine) -> CardLayout {
    let mut style = style.clone();
    style.validate();
    let width = style.width;
    let height = style.height;
    let padding = style.padding;
    let inner_width = (width - padding * 2.0).max(40.0);
    let direction = detect_direction(&format!(
        "{} {}",
        content.prompt,
        content.answer.clone().unwrap_or_default()
    ));
    let rtl = direction.is_rtl();

    let mut rects = Vec::new();
    let mut lines = Vec::new();
    let mut texts = Vec::new();

    rects.push(LayoutRect {
        x: 0.0,
        y: 0.0,
        width,
        height,
        radius: style.radius,
        fill: Some(style.background),
        stroke: Some(style.border),
        stroke_width: 1.0,
    });
    // Spectrum-like accent rule at the top.
    rects.push(LayoutRect {
        x: padding,
        y: padding * 0.55,
        width: 44.0,
        height: 3.0,
        radius: 1.5,
        fill: Some(style.accent),
        stroke: None,
        stroke_width: 0.0,
    });

    let header = build_text(
        engine,
        content.kind.label(),
        10.5,
        700,
        inner_width * 0.6,
        TextAlign::Start,
        rtl,
        &style.heading_family,
        style.letter_spacing.max(0.8),
        style.line_height,
    );
    texts.push(LayoutText {
        shaped: header,
        text: content.kind.label().to_string(),
        x: padding,
        y: padding * 0.55 + 10.0,
        color: style.accent,
    });

    if let Some(side) = &content.side_label {
        let side_text = build_text(
            engine,
            side,
            9.0,
            600,
            inner_width * 0.4,
            if rtl { TextAlign::Left } else { TextAlign::Right },
            rtl,
            &style.heading_family,
            0.6,
            1.2,
        );
        let x = width - padding - side_text.width;
        texts.push(LayoutText {
            shaped: side_text,
            text: side.clone(),
            x: x.max(padding),
            y: padding * 0.55 + 11.0,
            color: style.muted,
        });
    }

    let header_bottom = padding * 0.55 + 28.0;
    let footer_h = if style.show_source && content.source.is_some() {
        34.0
    } else if style.show_brand {
        26.0
    } else {
        0.0
    };
    let content_top = header_bottom + 10.0;
    let content_bottom = height - padding - footer_h;
    let available = (content_bottom - content_top).max(40.0);

    // Fit the prompt and answer by reducing type until it fits.
    let mut chosen = None;
    let mut size = style.font_size;
    while size >= 11.0 {
        let prompt = build_text(
            engine,
            &content.prompt,
            size,
            style.font_weight,
            inner_width,
            TextAlign::Start,
            rtl,
            &style.font_family,
            style.letter_spacing,
            style.line_height,
        );
        let answer = if content.include_back {
            content.answer.as_ref().map(|answer| {
                build_text(
                    engine,
                    answer,
                    size * 0.86,
                    400,
                    inner_width,
                    TextAlign::Start,
                    rtl,
                    &style.font_family,
                    style.letter_spacing,
                    style.line_height,
                )
            })
        } else {
            None
        };
        let note = content.note.as_ref().map(|note| {
            build_text(
                engine,
                note,
                size * 0.7,
                400,
                inner_width,
                TextAlign::Start,
                rtl,
                &style.font_family,
                style.letter_spacing,
                style.line_height,
            )
        });
        let divider_h = if answer.is_some() { 28.0 } else { 0.0 };
        let total = prompt.height
            + divisor_height(answer.as_ref())
            + note.as_ref().map_or(0.0, |n| n.height + 12.0);
        let total = total + divider_h;
        chosen = Some((prompt, answer, note, total, size));
        if total <= available {
            break;
        }
        size -= 1.0;
    }
    let (prompt, answer, note, total, _size) =
        chosen.unwrap_or_else(|| (fallback_text(engine, &content.prompt), None, None, 0.0, 0.0));
    let content_h = total.min(available);
    let mut y = content_top + ((available - content_h) * 0.42).max(0.0);
    texts.push(LayoutText {
        shaped: prompt.clone(),
        text: content.prompt.clone(),
        x: padding,
        y,
        color: style.foreground,
    });
    y += prompt.height;
    if let Some(answer) = &answer {
        let divider_y = y + 12.0;
        lines.push(LayoutLine {
            x1: padding,
            y1: divider_y,
            x2: width - padding,
            y2: divider_y,
            color: style.divider,
            width: 1.0,
        });
        y = divider_y + 16.0;
        texts.push(LayoutText {
            shaped: answer.clone(),
            text: content.answer.clone().unwrap_or_default(),
            x: padding,
            y,
            color: style.foreground,
        });
        y += answer.height;
    }
    if let Some(note) = &note {
        y += 8.0;
        texts.push(LayoutText {
            shaped: note.clone(),
            text: content.note.clone().unwrap_or_default(),
            x: padding,
            y,
            color: style.muted,
        });
    }

    if footer_h > 0.0 {
        let divider_y = height - padding - footer_h + 10.0;
        lines.push(LayoutLine {
            x1: padding,
            y1: divider_y,
            x2: width - padding,
            y2: divider_y,
            color: style.divider,
            width: 1.0,
        });
        if style.show_source {
            if let Some(source) = &content.source {
                let label = format!("Source · {source}");
                let source_text = build_text(
                    engine,
                    &label,
                    9.5,
                    400,
                    inner_width * 0.7,
                    TextAlign::Start,
                    rtl,
                    &style.font_family,
                    0.0,
                    1.3,
                );
                texts.push(LayoutText {
                    shaped: source_text,
                    text: label,
                    x: padding,
                    y: divider_y + 8.0,
                    color: style.muted,
                });
            }
        }
        if style.show_brand {
            let brand = build_text(
                engine,
                "DAFTAR",
                8.0,
                700,
                80.0,
                TextAlign::Right,
                rtl,
                &style.heading_family,
                1.4,
                1.2,
            );
            let x = width - padding - brand.width;
            texts.push(LayoutText {
                shaped: brand,
                text: "DAFTAR".to_string(),
                x: x.max(padding),
                y: height - padding - 10.0,
                color: style.muted,
            });
        }
    }

    CardLayout {
        width,
        height,
        background: style.background,
        rects,
        lines,
        texts,
        direction,
    }
}

fn divisor_height(answer: Option<&ShapedText>) -> f32 {
    if answer.is_some() {
        28.0
    } else {
        0.0
    }
}

fn fallback_text(engine: &mut TextEngine, text: &str) -> ShapedText {
    build_text(
        engine,
        text,
        14.0,
        400,
        300.0,
        TextAlign::Start,
        false,
        "Vazirmatn",
        0.0,
        1.4,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_text(
    engine: &mut TextEngine,
    text: &str,
    size: f32,
    weight: u16,
    width: f32,
    align: TextAlign,
    rtl: bool,
    family: &str,
    letter_spacing: f32,
    line_height: f32,
) -> ShapedText {
    let opts = LayoutOptions {
        width: width.max(20.0),
        family: family.to_string(),
        size,
        weight,
        line_height,
        letter_spacing,
        align,
        align_start_rtl: rtl,
        paragraph_direction: None,
        code_family: "JetBrains Mono".to_string(),
        code_size: size * 0.9,
        wrap: true,
        base_italic: false,
        base_code: false,
    };
    engine.layout_text(text, &opts, &daftar_core::markdown::SpanStyle::default())
}

/// Convert a layout into a raster scene and return encoded high-resolution PNG bytes.
pub fn render_card_png(
    content: &CardContent,
    style: &CardStyle,
    engine: &mut TextEngine,
) -> Result<Vec<u8>> {
    let layout = layout_card(content, style, engine);
    let mut scene = daftar_diagram::Scene::new(
        layout.width,
        layout.height,
        Some(layout.background),
    );
    for rect in &layout.rects {
        scene.rect(
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.radius,
            rect.fill,
            rect.stroke.map(|color| daftar_diagram::SceneStroke::solid(color, rect.stroke_width)),
        );
    }
    for line in &layout.lines {
        scene.line(
            line.x1,
            line.y1,
            line.x2,
            line.y2,
            daftar_diagram::SceneStroke::solid(line.color, line.width),
        );
    }
    for text in &layout.texts {
        scene.text(text.shaped.clone(), text.x, text.y, text.color);
    }
    scene
        .encode_png(engine, style.scale, Some(layout.background))
        .map_err(ExportError::Image)
}

pub fn save_card_png(
    path: impl AsRef<std::path::Path>,
    content: &CardContent,
    style: &CardStyle,
    engine: &mut TextEngine,
) -> Result<()> {
    let bytes = render_card_png(content, style, engine)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_text::TextEngine;

    fn engine() -> TextEngine {
        TextEngine::new(None, "fa-IR")
    }

    fn flashcard(front: &str, back: &str) -> CardContent {
        CardContent {
            kind: CardKind::Flashcard,
            prompt: front.to_string(),
            answer: Some(back.to_string()),
            note: Some("Extra context for review.".into()),
            source: Some("Rust Notes".into()),
            tags: vec!["rust".into()],
            side_label: Some("FRONT / BACK".into()),
            include_back: true,
        }
    }

    #[test]
    fn layouts_latin_card() {
        let mut engine = engine();
        let layout = layout_card(
            &flashcard("What is ownership?", "Rust's memory safety model."),
            &CardStyle::light(),
            &mut engine,
        );
        assert!(layout.texts.len() >= 5);
        assert!(layout.texts.iter().any(|t| t.text.contains("ownership")));
        assert!(layout.texts.iter().any(|t| t.text.contains("DAFTAR")));
    }

    #[test]
    fn layouts_rtl_card() {
        let mut engine = engine();
        let content = flashcard("مالکیت چیست؟", "مدل ایمنی حافظه در راست.");
        let layout = layout_card(&content, &CardStyle::dark(), &mut engine);
        assert_eq!(layout.direction, ResolvedDirection::Rtl);
        assert!(layout.texts.iter().any(|t| t.text.contains("مالکیت")));
    }

    #[test]
    fn renders_png_at_configured_scale() {
        let mut engine = engine();
        let mut style = CardStyle::light();
        style.width = 320.0;
        style.height = 200.0;
        style.scale = 2.0;
        let png = render_card_png(&flashcard("Q", "A"), &style, &mut engine).unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!(image.width(), 640);
        assert_eq!(image.height(), 400);
    }

    #[test]
    fn long_persian_text_fits_without_panicking() {
        let mut engine = engine();
        let long = "این یک متن بسیار طولانی فارسی است که باید در کارت جا شود. ".repeat(12);
        let layout = layout_card(&flashcard(&long, &long), &CardStyle::light(), &mut engine);
        assert!(!layout.texts.is_empty());
    }

    #[test]
    fn empty_content_is_handled() {
        let mut engine = engine();
        let content = CardContent {
            kind: CardKind::Highlight,
            prompt: String::new(),
            answer: None,
            note: None,
            source: None,
            tags: vec![],
            side_label: None,
            include_back: false,
        };
        let layout = layout_card(&content, &CardStyle::light(), &mut engine);
        assert!(!layout.texts.is_empty());
    }
}
