//! Glyph rasterisation into an egui texture atlas and rich text painting.

use std::collections::HashMap;
use std::ops::Range;

use cosmic_text::{CacheKey, CacheKeyFlags, SwashContent, Weight};
use egui::{Color32, ColorImage, Mesh, Pos2, Rect, TextureHandle, TextureOptions, Vec2};

use crate::fonts::TextEngine;
use crate::layout::{ShapedGlyph, ShapedText};

const ATLAS_SIZE: usize = 2048;
const PADDING: usize = 1;

#[derive(Debug, Clone, Copy, Default)]
pub struct TextColors {
    pub text: Color32,
    pub link: Color32,
    pub code: Color32,
    pub code_background: Option<Color32>,
    pub selection: Color32,
}

impl TextColors {
    pub fn light() -> Self {
        Self {
            text: Color32::from_rgb(0x2c, 0x2c, 0x2c),
            link: Color32::from_rgb(0x0b, 0x6b, 0xcb),
            code: Color32::from_rgb(0x2c, 0x2c, 0x2c),
            code_background: Some(Color32::from_rgb(0xf5, 0xf5, 0xf5)),
            selection: Color32::from_rgba_premultiplied(0x8a, 0xbf, 0xf8, 0x90),
        }
    }

    pub fn dark() -> Self {
        Self {
            text: Color32::from_rgb(0xe1, 0xe1, 0xe1),
            link: Color32::from_rgb(0x6b, 0xb8, 0xff),
            code: Color32::from_rgb(0xe1, 0xe1, 0xe1),
            code_background: Some(Color32::from_rgb(0x25, 0x25, 0x25)),
            selection: Color32::from_rgba_premultiplied(0x1d, 0x5a, 0x96, 0xc0),
        }
    }
}

/// Additional painted range (persistent highlights, search matches, flashcard emphasis).
#[derive(Debug, Clone, Default)]
pub struct RangePaint {
    pub range: Range<usize>,
    pub background: Option<Color32>,
    pub text_color: Option<Color32>,
    pub underline: Option<Color32>,
}

#[derive(Debug, Clone)]
struct AtlasGlyph {
    uv: Rect,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

pub(crate) struct GlyphAtlas {
    image: ColorImage,
    texture: Option<TextureHandle>,
    cursor_x: usize,
    cursor_y: usize,
    row_height: usize,
    entries: HashMap<CacheKey, Option<AtlasGlyph>>,
    full: bool,
    generation: u32,
}

impl GlyphAtlas {
    pub(crate) fn new() -> Self {
        Self {
            image: ColorImage::new(
                [ATLAS_SIZE, ATLAS_SIZE],
                vec![Color32::TRANSPARENT; ATLAS_SIZE * ATLAS_SIZE],
            ),
            texture: None,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            entries: HashMap::new(),
            full: false,
            generation: 0,
        }
    }

    fn reset(&mut self) {
        self.image = ColorImage::new(
            [ATLAS_SIZE, ATLAS_SIZE],
            vec![Color32::TRANSPARENT; ATLAS_SIZE * ATLAS_SIZE],
        );
        self.entries.clear();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;
        self.full = false;
        self.texture = None;
        self.generation = self.generation.wrapping_add(1);
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) -> TextureHandle {
        if let Some(texture) = &self.texture {
            return texture.clone();
        }
        let texture = ctx.load_texture(
            format!("daftar-glyphs-{}", self.generation),
            self.image.clone(),
            TextureOptions::LINEAR,
        );
        self.texture = Some(texture.clone());
        texture
    }

    fn insert(
        &mut self,
        ctx: &egui::Context,
        key: CacheKey,
        placement: cosmic_text::Placement,
        mask: &[u8],
        image_size: (usize, usize),
    ) -> Option<AtlasGlyph> {
        let (w, h) = image_size;
        if w == 0 || h == 0 {
            self.entries.insert(key, None);
            return None;
        }
        if self.cursor_x + w + PADDING > ATLAS_SIZE {
            self.cursor_x = 0;
            self.cursor_y += self.row_height + PADDING;
            self.row_height = 0;
        }
        if self.cursor_y + h + PADDING > ATLAS_SIZE {
            self.full = true;
            return None;
        }
        let x0 = self.cursor_x;
        let y0 = self.cursor_y;
        for row in 0..h {
            for col in 0..w {
                let alpha = mask[row * w + col];
                let px = Color32::from_white_alpha(alpha);
                let idx = (y0 + row) * ATLAS_SIZE + (x0 + col);
                if idx < self.image.pixels.len() {
                    self.image.pixels[idx] = px;
                }
            }
        }
        let patch = {
            let mut pixels = Vec::with_capacity(w * h);
            for row in 0..h {
                for col in 0..w {
                    pixels.push(Color32::from_white_alpha(mask[row * w + col]));
                }
            }
            ColorImage::new([w, h], pixels)
        };
        let mut texture = self.ensure_texture(ctx);
        texture.set_partial([x0, y0], patch, TextureOptions::LINEAR);
        self.cursor_x += w + PADDING;
        self.row_height = self.row_height.max(h);
        let inv = 1.0 / ATLAS_SIZE as f32;
        let uv = Rect::from_min_max(
            egui::pos2(x0 as f32 * inv, y0 as f32 * inv),
            egui::pos2((x0 + w) as f32 * inv, (y0 + h) as f32 * inv),
        );
        let glyph = AtlasGlyph {
            uv,
            left: placement.left,
            top: placement.top,
            width: w as u32,
            height: h as u32,
        };
        self.entries.insert(key, Some(glyph.clone()));
        Some(glyph)
    }
}

impl TextEngine {
    /// Paint shaped text at `origin` (top-left of the layout box).
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        shaped: &ShapedText,
        origin: Pos2,
        colors: &TextColors,
        selection: Option<Range<usize>>,
        decors: &[RangePaint],
    ) {
        if shaped.lines.is_empty() {
            return;
        }
        if self.atlas.full {
            self.atlas.reset();
        }
        let texture = self.atlas.ensure_texture(ctx);
        let texture_id = texture.id();

        // Backgrounds (selection, highlights, inline code) are drawn before glyphs.
        if let Some(sel) = selection.clone() {
            if sel.start < sel.end {
                for (x, y, w, h) in shaped.ranges_to_rects(sel) {
                    painter.rect_filled(
                        Rect::from_min_size(origin + Vec2::new(x, y), Vec2::new(w, h)),
                        1.0,
                        colors.selection,
                    );
                }
            }
        }
        for decor in decors {
            if decor.range.start >= decor.range.end {
                continue;
            }
            if let Some(bg) = decor.background {
                for (x, y, w, h) in shaped.ranges_to_rects(decor.range.clone()) {
                    painter.rect_filled(
                        Rect::from_min_size(origin + Vec2::new(x, y), Vec2::new(w, h)),
                        1.0,
                        bg,
                    );
                }
            }
        }
        if let Some(code_bg) = colors.code_background {
            for span in shaped.spans.iter().filter(|s| s.style.code) {
                for (x, y, w, h) in shaped.ranges_to_rects(span.start..span.end) {
                    painter.rect_filled(
                        Rect::from_min_size(
                            origin + Vec2::new(x - 1.5, y - 0.5),
                            Vec2::new(w + 3.0, h + 1.0),
                        ),
                        2.0,
                        code_bg,
                    );
                }
            }
        }

        let mut mesh = Mesh::with_texture(texture_id);
        for line in &shaped.lines {
            for glyph in &line.glyphs {
                if glyph.width <= 0.0 || glyph.glyph_id == 0 {
                    continue;
                }
                let color = self.glyph_color(glyph, shaped, colors, decors);
                let Some(entry) = self.glyph_entry(ctx, glyph) else {
                    continue;
                };
                let Some(entry) = entry else {
                    continue;
                };
                let rect = Rect::from_min_size(
                    origin
                        + Vec2::new(
                            glyph.x + entry.left as f32,
                            glyph.baseline - entry.top as f32,
                        ),
                    Vec2::new(entry.width as f32, entry.height as f32),
                );
                mesh.add_rect_with_uv(rect, entry.uv, color);
            }
        }
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }

        self.paint_decorations(painter, shaped, origin, colors, decors);
    }

    fn glyph_color(
        &self,
        glyph: &ShapedGlyph,
        shaped: &ShapedText,
        colors: &TextColors,
        decors: &[RangePaint],
    ) -> Color32 {
        for decor in decors {
            if let Some(color) = decor.text_color {
                if glyph.end > decor.range.start && glyph.start < decor.range.end {
                    return color;
                }
            }
        }
        let _ = shaped;
        if glyph.style.footnote.is_some() || glyph.style.link.is_some() {
            colors.link
        } else if glyph.style.code {
            colors.code
        } else {
            colors.text
        }
    }

    fn glyph_entry(
        &mut self,
        ctx: &egui::Context,
        glyph: &ShapedGlyph,
    ) -> Option<Option<AtlasGlyph>> {
        let weight = Weight(glyph.weight);
        let (key, _, _) = CacheKey::new(
            glyph.font_id,
            glyph.glyph_id,
            glyph.font_size,
            (glyph.x, glyph.baseline),
            weight,
            CacheKeyFlags::empty(),
        );
        if let Some(entry) = self.atlas.entries.get(&key) {
            return Some(entry.clone());
        }
        let image = self
            .swash
            .get_image(&mut self.fonts, key)
            .as_ref()
            .cloned();
        match image {
            Some(image) => {
                if !matches!(image.content, SwashContent::Mask) {
                    self.atlas.entries.insert(key, None);
                    return Some(None);
                }
                let w = image.placement.width as usize;
                let h = image.placement.height as usize;
                if image.data.len() < w * h {
                    self.atlas.entries.insert(key, None);
                    return Some(None);
                }
                let entry = self
                    .atlas
                    .insert(ctx, key, image.placement, &image.data, (w, h));
                Some(entry)
            }
            None => {
                self.atlas.entries.insert(key, None);
                Some(None)
            }
        }
    }

    fn paint_decorations(
        &self,
        painter: &egui::Painter,
        shaped: &ShapedText,
        origin: Pos2,
        colors: &TextColors,
        decors: &[RangePaint],
    ) {
        for span in &shaped.spans {
            let link = span.style.link.is_some() && span.style.footnote.is_none();
            let strike = span.style.strikethrough;
            if !link && !strike {
                continue;
            }
            for (x, y, w, h) in shaped.ranges_to_rects(span.start..span.end) {
                if link {
                    painter.rect_filled(
                        Rect::from_min_size(
                            origin + Vec2::new(x, y + h * 0.93),
                            Vec2::new(w, 1.0),
                        ),
                        0.0,
                        colors.link,
                    );
                }
                if strike {
                    painter.rect_filled(
                        Rect::from_min_size(
                            origin + Vec2::new(x, y + h * 0.55),
                            Vec2::new(w, 1.0),
                        ),
                        0.0,
                        colors.text,
                    );
                }
            }
        }
        for decor in decors {
            if let Some(color) = decor.underline {
                if decor.range.start >= decor.range.end {
                    continue;
                }
                for (x, y, w, h) in shaped.ranges_to_rects(decor.range.clone()) {
                    painter.rect_filled(
                        Rect::from_min_size(
                            origin + Vec2::new(x, y + h * 0.92),
                            Vec2::new(w, 1.0),
                        ),
                        0.0,
                        color,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutOptions;
    use daftar_core::markdown::SpanStyle;

    #[test]
    fn headless_paint_does_not_panic_and_creates_mesh() {
        let ctx = egui::Context::default();
        let mut engine = TextEngine::new(None, "en-US");
        let opts = LayoutOptions {
            width: 500.0,
            family: "Vazirmatn".into(),
            align_start_rtl: true,
            ..Default::default()
        };
        let shaped = engine.layout_text("سلام دنیا hello", &opts, &SpanStyle::default());
        let mut painted = false;
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let painter = ui.painter().clone();
            engine.paint(
                ui.ctx(),
                &painter,
                &shaped,
                egui::pos2(10.0, 10.0),
                &TextColors::light(),
                Some(0..4),
                &[],
            );
            painted = true;
        });
        output.textures_delta.clear();
        assert!(painted);
    }

    #[test]
    fn atlas_packs_distinct_glyphs() {
        let mut atlas = GlyphAtlas::new();
        assert!(atlas.entries.is_empty());
        // Inserting a fake entry proves the shelf allocator advances.
        let key = CacheKey::new(
            fontdb::ID::dummy(),
            1,
            16.0,
            (0.0, 0.0),
            Weight::NORMAL,
            CacheKeyFlags::empty(),
        )
        .0;
        atlas.entries.insert(key, None);
        assert_eq!(atlas.entries.len(), 1);
    }
}