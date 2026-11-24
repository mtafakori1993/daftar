//! Bidirectional text layout with HarfBuzz-class shaping via cosmic-text.
//!
//! Every block is shaped with Unicode bidi reordering (UAX #9) and advanced shaping from
//! `harfrust`, which correctly joins Persian/Arabic letters. Forced document direction uses an
//! invisible directional mark to set the paragraph base direction, so mixed content is laid out
//! by the Unicode algorithm rather than by mirroring glyphs or right-aligning LTR runs.

use std::ops::Range;

use cosmic_text::{
    Align, Attrs, AttrsOwned, Buffer, Family, FontSystem, Metrics, Shaping, Style, Weight, Wrap,
};
use daftar_core::direction::{ResolvedDirection, detect_direction};
use daftar_core::markdown::{RichText, SpanStyle, StyledSpan};
use daftar_core::settings::TextAlign;

/// Text style and layout configuration for one block.
#[derive(Debug, Clone)]
pub struct LayoutOptions {
    /// Wrap width in points. `0.0` means unbounded.
    pub width: f32,
    pub family: String,
    pub size: f32,
    pub weight: u16,
    /// Multiplier of font size.
    pub line_height: f32,
    /// Extra letter spacing in pixels.
    pub letter_spacing: f32,
    pub align: TextAlign,
    /// Which way "start" alignment goes for the surrounding document.
    pub align_start_rtl: bool,
    /// Force a paragraph base direction. `None` uses the Unicode first-strong rule.
    pub paragraph_direction: Option<ResolvedDirection>,
    pub code_family: String,
    pub code_size: f32,
    pub wrap: bool,
    pub base_italic: bool,
    pub base_code: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            width: 0.0,
            family: "Vazirmatn".to_string(),
            size: 17.0,
            weight: 400,
            line_height: 1.8,
            letter_spacing: 0.0,
            align: TextAlign::Start,
            align_start_rtl: false,
            paragraph_direction: None,
            code_family: "JetBrains Mono".to_string(),
            code_size: 13.5,
            wrap: true,
            base_italic: false,
            base_code: false,
        }
    }
}

impl LayoutOptions {
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// Display byte range in the unwrapped block text.
    pub start: usize,
    pub end: usize,
    pub font_id: fontdb::ID,
    pub glyph_id: u16,
    pub font_size: f32,
    pub weight: u16,
    /// Pen X relative to the block's left edge.
    pub x: f32,
    /// Baseline Y relative to the block's top edge.
    pub baseline: f32,
    pub width: f32,
    /// True for right-to-left glyph runs.
    pub rtl: bool,
    pub style: SpanStyle,
}

#[derive(Debug, Clone)]
pub struct ShapedLine {
    pub index: usize,
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
    pub rtl: bool,
    /// Display byte range covered by this visual line.
    pub range: Range<usize>,
    pub glyphs: Vec<ShapedGlyph>,
}

#[derive(Debug, Clone, Default)]
pub struct ShapedText {
    pub lines: Vec<ShapedLine>,
    pub width: f32,
    pub height: f32,
    pub text_len: usize,
    pub direction: Option<ResolvedDirection>,
    pub spans: Vec<StyledSpan>,
    pub prefix_len_used: usize,
}

impl ShapedText {
    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|l| l.glyphs.is_empty())
    }

    pub fn line_at_y(&self, y: f32) -> Option<&ShapedLine> {
        if self.lines.is_empty() {
            return None;
        }
        self.lines
            .iter()
            .find(|line| y >= line.top && y < line.top + line.height.max(1.0))
            .or_else(|| {
                if y < 0.0 {
                    self.lines.first()
                } else {
                    self.lines.last()
                }
            })
    }

    /// Map a local point to a display byte offset. Returns an offset suitable for
    /// selection anchors. RTL glyph runs map visual left to the logical end.
    pub fn offset_at_point(&self, x: f32, y: f32) -> usize {
        let Some(line) = self.line_at_y(y) else {
            return 0;
        };
        if line.glyphs.is_empty() {
            return line.range.start.min(self.text_len);
        }
        let mut best: Option<&ShapedGlyph> = None;
        let mut best_dist = f32::MAX;
        for glyph in &line.glyphs {
            let (lo, hi) = (glyph.x, glyph.x + glyph.width.max(0.0));
            let dist = if x < lo {
                lo - x
            } else if x > hi {
                x - hi
            } else {
                0.0
            };
            if dist < best_dist {
                best = Some(glyph);
                best_dist = dist;
            }
        }
        let Some(glyph) = best else {
            return line.range.start;
        };
        let (lo, hi) = (glyph.x, glyph.x + glyph.width.max(0.0));
        let at_left = x < lo + (hi - lo) * 0.5;
        let offset = if glyph.rtl {
            if at_left {
                glyph.end
            } else {
                glyph.start
            }
        } else if at_left {
            glyph.start
        } else {
            glyph.end
        };
        offset.min(self.text_len)
    }

    /// Rectangle spans (relative to the block) covering a display byte range.
    pub fn ranges_to_rects(&self, range: Range<usize>) -> Vec<(f32, f32, f32, f32)> {
        let mut out = Vec::new();
        if range.start >= range.end {
            return out;
        }
        for line in &self.lines {
            let mut spans: Vec<(f32, f32)> = Vec::new();
            for glyph in &line.glyphs {
                if glyph.end > range.start && glyph.start < range.end {
                    let x0 = glyph.x;
                    let x1 = glyph.x + glyph.width.max(0.0);
                    if x1 > x0 {
                        spans.push((x0, x1));
                    }
                }
            }
            if spans.is_empty() {
                continue;
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (x0, x1) in spans {
                if let Some(last) = merged.last_mut() {
                    if x0 <= last.1 + 1.5 {
                        last.1 = last.1.max(x1);
                        continue;
                    }
                }
                merged.push((x0, x1));
            }
            for (x0, x1) in merged {
                out.push((
                    x0,
                    line.top + line.height * 0.14,
                    (x1 - x0).max(1.0),
                    line.height * 0.72,
                ));
            }
        }
        out
    }

    /// Caret x position for a display byte offset on its line.
    pub fn caret_position(&self, offset: usize) -> Option<(f32, f32, f32)> {
        let line = self
            .lines
            .iter()
            .find(|l| offset >= l.range.start && offset <= l.range.end)
            .or_else(|| self.lines.last())?;
        let mut x = if line.rtl {
            line.glyphs
                .iter()
                .map(|g| g.x + g.width)
                .fold(0.0f32, f32::max)
        } else {
            0.0
        };
        for glyph in &line.glyphs {
            if glyph.start <= offset && offset <= glyph.end {
                x = if glyph.rtl {
                    if offset == glyph.start {
                        glyph.x + glyph.width
                    } else {
                        glyph.x
                    }
                } else if offset == glyph.start {
                    glyph.x
                } else {
                    glyph.x + glyph.width
                };
                break;
            }
        }
        Some((x, line.top, line.height))
    }

    pub fn text_range_for_source_block(&self) -> Range<usize> {
        0..self.text_len
    }
}

/// Shape rich text into positioned glyph runs.
pub fn shape_rich(
    fonts: &mut FontSystem,
    rt: &RichText,
    opts: &LayoutOptions,
) -> ShapedText {
    let display_text = rt.text.as_str();
    if display_text.is_empty() {
        return ShapedText {
            width: opts.width.max(0.0),
            text_len: 0,
            direction: opts.paragraph_direction,
            spans: rt.spans.clone(),
            ..Default::default()
        };
    }
    let prefix = match opts.paragraph_direction {
        Some(ResolvedDirection::Rtl) => "\u{200F}",
        Some(ResolvedDirection::Ltr) => "\u{200E}",
        None => "",
    };
    let prefix_len = prefix.len();
    let text = format!("{prefix}{display_text}");
    let text_len = display_text.len();

    let metrics = Metrics::new(
        opts.size.max(1.0),
        (opts.size * opts.line_height).max(opts.size + 1.0),
    );
    let mut buffer = Buffer::new_empty(metrics);
    buffer.set_wrap(if opts.wrap {
        Wrap::WordOrGlyph
    } else {
        Wrap::None
    });
    if opts.wrap && opts.width > 1.0 {
        buffer.set_size(Some(opts.width), None);
    } else {
        buffer.set_size(None, None);
    }

    let align = resolve_align(opts);
    let default_attrs = base_attrs(opts, &SpanStyle::default());
    let default_owned = AttrsOwned::new(&default_attrs);

    let mut segments: Vec<(usize, usize, SpanStyle)> = Vec::new();
    let mut cursor = 0usize;
    for span in &rt.spans {
        let start = span.start.saturating_add(prefix_len).min(text.len());
        let end = span.end.saturating_add(prefix_len).min(text.len());
        if end <= cursor || start >= end {
            continue;
        }
        let start = start.max(cursor);
        if start > cursor {
            segments.push((cursor, start, SpanStyle::default()));
        }
        segments.push((start, end, span.style.clone()));
        cursor = end;
    }
    if cursor < text.len() {
        segments.push((cursor, text.len(), SpanStyle::default()));
    }
    if segments.is_empty() {
        segments.push((0, text.len(), SpanStyle::default()));
    }

    let span_data: Vec<(usize, usize, AttrsOwned)> = segments
        .into_iter()
        .filter(|(s, e, _)| *s < *e)
        .map(|(s, e, style)| {
            let attrs = base_attrs(opts, &style);
            (s, e, AttrsOwned::new(&attrs))
        })
        .collect();

    buffer.set_rich_text(
        span_data
            .iter()
            .map(|(s, e, attrs)| (&text[*s..*e], attrs.as_attrs())),
        &default_owned.as_attrs(),
        Shaping::Advanced,
        Some(align),
    );
    buffer.shape_until_scroll(fonts, false);

    let line_starts = line_starts(&text);
    let mut lines: Vec<ShapedLine> = Vec::new();
    for run in buffer.layout_runs() {
        let line_start = line_starts.get(run.line_i).copied().unwrap_or(0);
        let mut glyphs = Vec::with_capacity(run.glyphs.len());
        let mut min_start = usize::MAX;
        let mut max_end = 0usize;
        for glyph in run.glyphs {
            let raw_start = line_start.saturating_add(glyph.start);
            let raw_end = line_start.saturating_add(glyph.end);
            if raw_end <= prefix_len {
                continue;
            }
            let start = raw_start.saturating_sub(prefix_len).min(text_len);
            let end = raw_end.saturating_sub(prefix_len).min(text_len);
            if start >= end && glyph.w == 0.0 {
                continue;
            }
            min_start = min_start.min(start);
            max_end = max_end.max(end);
            let style = rt.style_at(start);
            let mut baseline = run.line_y + glyph.y - glyph.font_size * glyph.y_offset;
            if style.superscript {
                baseline -= glyph.font_size * 0.34;
            } else if style.subscript {
                baseline += glyph.font_size * 0.18;
            }
            glyphs.push(ShapedGlyph {
                start,
                end,
                font_id: glyph.font_id,
                glyph_id: glyph.glyph_id,
                font_size: glyph.font_size,
                weight: glyph.font_weight.0,
                x: glyph.x + glyph.font_size * glyph.x_offset,
                baseline,
                width: glyph.w,
                rtl: glyph.level.is_rtl(),
                style,
            });
        }
        if glyphs.is_empty() {
            // Keep empty visual lines so paragraph height remains stable.
            let range_start = line_start.saturating_sub(prefix_len).min(text_len);
            min_start = range_start;
            max_end = range_start;
        }
        let range = if min_start <= max_end {
            min_start..max_end
        } else {
            0..0
        };
        lines.push(ShapedLine {
            index: run.line_i,
            top: run.line_top,
            baseline: run.line_y,
            height: run.line_height,
            width: run.line_w,
            rtl: run.rtl,
            range,
            glyphs,
        });
    }

    let height = lines
        .iter()
        .map(|l| l.top + l.height)
        .fold(0.0f32, f32::max);
    let width = if opts.width > 1.0 {
        opts.width
    } else {
        lines.iter().map(|l| l.width).fold(0.0f32, f32::max)
    };
    let direction = if let Some(dir) = opts.paragraph_direction {
        Some(dir)
    } else {
        lines
            .iter()
            .find(|l| !l.glyphs.is_empty())
            .map(|l| ResolvedDirection::from_rtl(l.rtl))
            .or(Some(detect_direction(display_text)))
    };
    ShapedText {
        lines,
        width,
        height,
        text_len,
        direction,
        spans: rt.spans.clone(),
        prefix_len_used: prefix_len,
    }
}

/// Shape a uniform run of text (titles, buttons, card content).
pub fn shape_text(
    fonts: &mut FontSystem,
    text: &str,
    opts: &LayoutOptions,
    style: &SpanStyle,
) -> ShapedText {
    let mut rt = RichText::new(0..text.len());
    rt.push(text, style, 0..text.len());
    shape_rich(fonts, &rt, opts)
}

fn resolve_align(opts: &LayoutOptions) -> Align {
    match opts.align {
        TextAlign::Start => {
            if opts.align_start_rtl {
                Align::Right
            } else {
                Align::Left
            }
        }
        TextAlign::Left => Align::Left,
        TextAlign::Right => Align::Right,
        TextAlign::Center => Align::Center,
        TextAlign::Justify => Align::Justified,
    }
}

fn base_attrs<'a>(opts: &'a LayoutOptions, style: &SpanStyle) -> Attrs<'a> {
    let family_name: &'a str = if style.code {
        &opts.code_family
    } else if style.math {
        "DejaVu Serif"
    } else {
        &opts.family
    };
    let mut weight = opts.weight.max(if style.bold { 600 } else { 0 });
    if style.bold {
        weight = weight.max(700);
    }
    let mut attrs = Attrs::new()
        .family(Family::Name(family_name))
        .weight(Weight(weight))
        .style(if style.italic || opts.base_italic {
            Style::Italic
        } else {
            Style::Normal
        });
    let size = if style.code {
        opts.code_size
    } else {
        opts.size
    };
    let size = if style.superscript || style.subscript {
        size * 0.72
    } else {
        size
    };
    let line_height = (size * opts.line_height).max(size + 1.0);
    if style.superscript || style.subscript {
        attrs = attrs.metrics(Metrics::new(size, line_height));
    }
    if opts.letter_spacing.abs() > f32::EPSILON && !style.code {
        attrs = attrs.letter_spacing(opts.letter_spacing / size.max(1.0));
    }
    attrs
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, ch) in text.char_indices() {
        if ch == '\n' {
            starts.push(i + 1);
        }
    }
    starts
}

/// Build a span list covering `text` for callers that need to paint only part of it.
pub fn span_for_all(text: &str, style: SpanStyle) -> Vec<StyledSpan> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![StyledSpan {
            start: 0,
            end: text.len(),
            style,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::TextEngine;

    fn engine() -> TextEngine {
        TextEngine::new(None, "en-US")
    }

    fn shaped_persian() -> (TextEngine, ShapedText) {
        let mut engine = engine();
        let mut opts = LayoutOptions {
            width: 600.0,
            family: "Vazirmatn".into(),
            align_start_rtl: true,
            paragraph_direction: Some(ResolvedDirection::Rtl),
            ..Default::default()
        };
        opts.size = 20.0;
        let shaped = engine.layout_text("سلام دنیا", &opts, &SpanStyle::default());
        (engine, shaped)
    }

    #[test]
    fn shapes_persian_with_rtl_base() {
        let (_engine, shaped) = shaped_persian();
        assert!(!shaped.lines.is_empty());
        assert!(shaped.lines[0].rtl, "line should be RTL");
        assert!(shaped.lines[0].glyphs.len() >= 4);
        assert_eq!(shaped.direction, Some(ResolvedDirection::Rtl));
    }

    #[test]
    fn arabic_lam_alef_forms_a_ligature() {
        let mut engine = engine();
        let opts = LayoutOptions {
            width: 400.0,
            family: "Vazirmatn".into(),
            align_start_rtl: true,
            paragraph_direction: Some(ResolvedDirection::Rtl),
            ..Default::default()
        };
        let shaped = engine.layout_text("لا", &opts, &SpanStyle::default());
        let glyphs: usize = shaped.lines.iter().map(|l| l.glyphs.len()).sum();
        assert!(glyphs <= 2, "lam-alef should join, got {glyphs} glyphs");
    }

    #[test]
    fn mixed_bidi_contains_both_directions() {
        let mut engine = engine();
        let opts = LayoutOptions {
            width: 900.0,
            family: "Vazirmatn".into(),
            align_start_rtl: false,
            ..Default::default()
        };
        let shaped = engine.layout_text(
            "English then فارسی and English",
            &opts,
            &SpanStyle::default(),
        );
        let has_rtl = shaped
            .lines
            .iter()
            .flat_map(|l| &l.glyphs)
            .any(|g| g.rtl);
        let has_ltr = shaped
            .lines
            .iter()
            .flat_map(|l| &l.glyphs)
            .any(|g| !g.rtl);
        assert!(has_rtl && has_ltr);
    }

    #[test]
    fn forced_ltr_does_not_break_persian_shaping() {
        let mut engine = engine();
        let opts = LayoutOptions {
            width: 500.0,
            family: "Vazirmatn".into(),
            paragraph_direction: Some(ResolvedDirection::Ltr),
            ..Default::default()
        };
        let shaped = engine.layout_text("متن فارسی", &opts, &SpanStyle::default());
        assert!(shaped.lines[0].glyphs.len() >= 4);
    }

    #[test]
    fn wrapping_respects_width() {
        let mut engine = engine();
        let opts = LayoutOptions {
            width: 220.0,
            family: "Vazirmatn".into(),
            ..Default::default()
        };
        let shaped = engine.layout_text(
            "This is a longer sentence that must wrap onto several lines for the test.",
            &opts,
            &SpanStyle::default(),
        );
        assert!(shaped.lines.len() >= 2);
        assert!(shaped.height > shaped.lines[0].height);
    }

    #[test]
    fn hit_testing_finds_ltr_word() {
        let mut engine = engine();
        let opts = LayoutOptions {
            width: 600.0,
            family: "Vazirmatn".into(),
            align_start_rtl: false,
            ..Default::default()
        };
        let shaped = engine.layout_text("Hello world", &opts, &SpanStyle::default());
        let glyph = shaped.lines[0]
            .glyphs
            .iter()
            .find(|g| g.start >= 6)
            .unwrap();
        let offset = shaped.offset_at_point(
            glyph.x + glyph.width * 0.5,
            shaped.lines[0].top + 2.0,
        );
        assert!((6..=11).contains(&offset), "offset {offset}");
    }

    #[test]
    fn hit_testing_maps_rtl_visual_left_to_logical_end() {
        let (_e, shaped) = shaped_persian();
        let line = &shaped.lines[0];
        let right = line
            .glyphs
            .iter()
            .map(|g| g.x + g.width)
            .fold(0.0f32, f32::max);
        let left_offset = shaped.offset_at_point(0.0, line.top + 1.0);
        let right_offset = shaped.offset_at_point(right, line.top + 1.0);
        assert!(left_offset > right_offset, "RTL selection mirroring failed");
    }

    #[test]
    fn selection_rects_are_non_empty() {
        let (_e, shaped) = shaped_persian();
        let rects = shaped.ranges_to_rects(0..6);
        assert!(!rects.is_empty());
        assert!(rects.iter().all(|r| r.2 > 0.0 && r.3 > 0.0));
    }

    #[test]
    fn letter_spacing_changes_width() {
        let mut engine = engine();
        let mut opts = LayoutOptions {
            width: 0.0,
            family: "Vazirmatn".into(),
            ..Default::default()
        };
        let normal = engine.layout_text("ABCDEF", &opts, &SpanStyle::default());
        opts.letter_spacing = 2.0;
        let spaced = engine.layout_text("ABCDEF", &opts, &SpanStyle::default());
        assert!(spaced.lines[0].width > normal.lines[0].width + 4.0);
    }

    #[test]
    fn superscript_is_smaller_and_raised() {
        let mut engine = engine();
        let mut rt = RichText::new(0..20);
        rt.push("H", &SpanStyle::default(), 0..1);
        rt.push(
            "2",
            &SpanStyle {
                superscript: true,
                ..Default::default()
            },
            1..2,
        );
        rt.push("O", &SpanStyle::default(), 2..3);
        let opts = LayoutOptions {
            width: 100.0,
            family: "Vazirmatn".into(),
            ..Default::default()
        };
        let shaped = engine.layout_rich(&rt, &opts);
        let glyphs = &shaped.lines[0].glyphs;
        let normal = glyphs.iter().find(|g| !g.style.superscript).unwrap();
        let sup = glyphs.iter().find(|g| g.style.superscript).unwrap();
        assert!(sup.font_size < normal.font_size);
        assert!(sup.baseline < normal.baseline);
    }
}