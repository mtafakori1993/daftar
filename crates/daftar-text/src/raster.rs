//! Vector glyph outlines for PNG/PDF-quality export using tiny-skia.
//!
//! Export uses real glyph outlines from the same faces used for reading, so Persian and Arabic
//! joins and kerning match the on-screen reader. PDF generation reuses the same glyph run
//! data but writes actual text operators (see `daftar-export`).

use std::collections::HashMap;

use ttf_parser::{Face, GlyphId, OutlineBuilder};
use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, Transform};

use crate::fonts::TextEngine;
use crate::layout::ShapedText;

impl TextEngine {
    /// Outline for a glyph in em units with y pointing down, or `None` for blank glyphs.
    pub fn glyph_outline(&mut self, font_id: fontdb::ID, glyph_id: u16) -> Option<Path> {
        if let Some(cached) = self.outline_cache.get(&(font_id, glyph_id)) {
            return cached.clone();
        }
        let blob = self.font_blob(font_id)?;
        let face = Face::parse(&blob.data, blob.index).ok()?;
        let upem = face.units_per_em() as f32;
        if upem <= 0.0 {
            return None;
        }
        let mut sink = SkiaOutline {
            builder: PathBuilder::new(),
            upem,
        };
        let outline = face.outline_glyph(GlyphId(glyph_id), &mut sink);
        let path = if outline.is_some() {
            sink.builder.finish()
        } else {
            None
        };
        self.outline_cache
            .insert((font_id, glyph_id), path.clone());
        path
    }

    /// Paint shaped text into a pixmap. `origin` is the top-left of the text box;
    /// `scale` multiplies both positions and glyph size (2.0 = high-DPI export).
    pub fn draw_shaped_skia(
        &mut self,
        pixmap: &mut Pixmap,
        shaped: &ShapedText,
        origin: (f32, f32),
        color: Color,
        scale: f32,
    ) {
        let mut paint = Paint::default();
        paint.set_color(color);
        paint.anti_alias = true;
        for line in &shaped.lines {
            for glyph in &line.glyphs {
                if glyph.width <= 0.0 || glyph.glyph_id == 0 {
                    continue;
                }
                let Some(path) = self.glyph_outline(glyph.font_id, glyph.glyph_id) else {
                    continue;
                };
                let tx = (origin.0 + glyph.x) * scale;
                let ty = (origin.1 + glyph.baseline) * scale;
                let transform = Transform::from_row(
                    glyph.font_size * scale,
                    0.0,
                    0.0,
                    glyph.font_size * scale,
                    tx,
                    ty,
                );
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            }
        }
    }

    /// Draw a display-byte range of shaped text only (used by diagram labels and highlights).
    pub fn draw_range_skia(
        &mut self,
        pixmap: &mut Pixmap,
        shaped: &ShapedText,
        range: std::ops::Range<usize>,
        origin: (f32, f32),
        color: Color,
        scale: f32,
    ) {
        let filtered = crate::layout::ShapedText {
            lines: shaped
                .lines
                .iter()
                .map(|line| crate::layout::ShapedLine {
                    glyphs: line
                        .glyphs
                        .iter()
                        .filter(|g| g.end > range.start && g.start < range.end)
                        .cloned()
                        .collect(),
                    ..line.clone()
                })
                .collect(),
            ..shaped.clone()
        };
        self.draw_shaped_skia(pixmap, &filtered, origin, color, scale);
    }
}

struct SkiaOutline {
    builder: PathBuilder,
    upem: f32,
}

impl OutlineBuilder for SkiaOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder
            .move_to(x / self.upem, -(y / self.upem));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder
            .line_to(x / self.upem, -(y / self.upem));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(
            x1 / self.upem,
            -(y1 / self.upem),
            x / self.upem,
            -(y / self.upem),
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(
            x1 / self.upem,
            -(y1 / self.upem),
            x2 / self.upem,
            -(y2 / self.upem),
            x / self.upem,
            -(y / self.upem),
        );
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

/// Cache of outlines owned by the engine, populated lazily.
pub type OutlineCache = HashMap<(fontdb::ID, u16), Option<Path>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutOptions;
    use daftar_core::markdown::SpanStyle;
    use daftar_core::direction::ResolvedDirection;

    fn ink_count(pixmap: &Pixmap) -> usize {
        pixmap.pixels().iter().filter(|p| p.alpha() > 0).count()
    }

    #[test]
    fn ltr_text_produces_ink() {
        let mut engine = TextEngine::new(None, "en-US");
        let opts = LayoutOptions {
            width: 600.0,
            family: "Vazirmatn".into(),
            size: 32.0,
            ..Default::default()
        };
        let shaped = engine.layout_text("Hello export", &opts, &SpanStyle::default());
        let mut pixmap = Pixmap::new(700, 120).unwrap();
        engine.draw_shaped_skia(
            &mut pixmap,
            &shaped,
            (10.0, 10.0),
            Color::BLACK,
            1.0,
        );
        assert!(ink_count(&pixmap) > 100);
    }

    #[test]
    fn persian_rtl_text_produces_shaped_ink() {
        let mut engine = TextEngine::new(None, "fa-IR");
        let opts = LayoutOptions {
            width: 600.0,
            family: "Vazirmatn".into(),
            size: 32.0,
            align_start_rtl: true,
            paragraph_direction: Some(ResolvedDirection::Rtl),
            ..Default::default()
        };
        let shaped = engine.layout_text("سلام دنیا", &opts, &SpanStyle::default());
        let mut pixmap = Pixmap::new(700, 120).unwrap();
        engine.draw_shaped_skia(&mut pixmap, &shaped, (10.0, 10.0), Color::BLACK, 1.0);
        assert!(ink_count(&pixmap) > 100);
    }

    #[test]
    fn high_dpi_scaling_produces_more_ink() {
        let mut engine = TextEngine::new(None, "en-US");
        let opts = LayoutOptions {
            width: 400.0,
            family: "Vazirmatn".into(),
            size: 20.0,
            ..Default::default()
        };
        let shaped = engine.layout_text("Scale", &opts, &SpanStyle::default());
        let mut small = Pixmap::new(400, 80).unwrap();
        let mut large = Pixmap::new(800, 160).unwrap();
        engine.draw_shaped_skia(&mut small, &shaped, (5.0, 5.0), Color::BLACK, 1.0);
        engine.draw_shaped_skia(&mut large, &shaped, (5.0, 5.0), Color::BLACK, 2.0);
        assert!(ink_count(&large) > ink_count(&small));
    }
}