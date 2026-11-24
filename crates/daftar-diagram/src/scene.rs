//! Diagram scene IR shared by the on-screen viewer and PNG export.
//!
//! Scenes contain positioned, already-shaped text so diagrams use the same Persian/Arabic
//! shaping as the document reader. Rasterisation happens through tiny-skia.

use daftar_text::{ShapedText, TextEngine};
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, StrokeDash, Transform,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba8 {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_hex(hex: u32) -> Self {
        Self {
            r: ((hex >> 16) & 0xff) as u8,
            g: ((hex >> 8) & 0xff) as u8,
            b: (hex & 0xff) as u8,
            a: 255,
        }
    }

    pub fn to_color(self) -> Color {
        Color::from_rgba8(self.r, self.g, self.b, self.a)
    }

    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self {
            r: lerp(self.r, other.r),
            g: lerp(self.g, other.g),
            b: lerp(self.b, other.b),
            a: lerp(self.a, other.a),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SceneStroke {
    pub color: Rgba8,
    pub width: f32,
    pub dash: Option<(f32, f32)>,
}

impl SceneStroke {
    pub fn solid(color: Rgba8, width: f32) -> Self {
        Self {
            color,
            width,
            dash: None,
        }
    }

    pub fn dashed(color: Rgba8, width: f32) -> Self {
        Self {
            color,
            width,
            dash: Some((6.0, 5.0)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PathOp {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone)]
pub struct ScenePath {
    pub ops: Vec<PathOp>,
    pub fill: Option<Rgba8>,
    pub stroke: Option<SceneStroke>,
}

#[derive(Debug, Clone)]
pub struct SceneText {
    pub shaped: ShapedText,
    pub x: f32,
    pub y: f32,
    pub color: Rgba8,
    pub opacity: f32,
}

#[derive(Debug, Clone)]
pub enum SceneItem {
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    },
    Ellipse {
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    },
    Polygon {
        points: Vec<(f32, f32)>,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    },
    Path(ScenePath),
    Text(SceneText),
}

#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub width: f32,
    pub height: f32,
    pub background: Option<Rgba8>,
    pub items: Vec<SceneItem>,
}

impl Scene {
    pub fn new(width: f32, height: f32, background: Option<Rgba8>) -> Self {
        Self {
            width: width.max(1.0),
            height: height.max(1.0),
            background,
            items: Vec::new(),
        }
    }

    pub fn rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    ) {
        self.items.push(SceneItem::Rect {
            x,
            y,
            width,
            height,
            radius,
            fill,
            stroke,
        });
    }

    pub fn ellipse(
        &mut self,
        cx: f32,
        cy: f32,
        rx: f32,
        ry: f32,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    ) {
        self.items.push(SceneItem::Ellipse {
            cx,
            cy,
            rx,
            ry,
            fill,
            stroke,
        });
    }

    pub fn polygon(
        &mut self,
        points: Vec<(f32, f32)>,
        fill: Option<Rgba8>,
        stroke: Option<SceneStroke>,
    ) {
        self.items.push(SceneItem::Polygon {
            points,
            fill,
            stroke,
        });
    }

    pub fn path(&mut self, ops: Vec<PathOp>, fill: Option<Rgba8>, stroke: Option<SceneStroke>) {
        self.items.push(SceneItem::Path(ScenePath { ops, fill, stroke }));
    }

    pub fn text(&mut self, shaped: ShapedText, x: f32, y: f32, color: Rgba8) {
        self.items.push(SceneItem::Text(SceneText {
            shaped,
            x,
            y,
            color,
            opacity: 1.0,
        }));
    }

    pub fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, stroke: SceneStroke) {
        self.path(
            vec![PathOp::Move(x1, y1), PathOp::Line(x2, y2)],
            None,
            Some(stroke),
        );
    }

    pub fn arrow(&mut self, from: (f32, f32), to: (f32, f32), stroke: SceneStroke, size: f32) {
        let dx = to.0 - from.0;
        let dy = to.1 - from.1;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let (ux, uy) = (dx / len, dy / len);
        let tip = (to.0, to.1);
        let base = (to.0 - ux * size, to.1 - uy * size);
        let perpendicular = (-uy, ux);
        let half = size * 0.38;
        let p1 = (
            base.0 + perpendicular.0 * half,
            base.1 + perpendicular.1 * half,
        );
        let p2 = (
            base.0 - perpendicular.0 * half,
            base.1 - perpendicular.1 * half,
        );
        self.line(from.0, from.1, base.0, base.1, stroke);
        self.polygon(vec![tip, p1, p2], Some(stroke.color), None);
    }

    /// Draw only an arrowhead whose tip is at `tip`, coming from `from`.
    pub fn arrowhead(&mut self, from: (f32, f32), tip: (f32, f32), color: Rgba8, size: f32) {
        let dx = tip.0 - from.0;
        let dy = tip.1 - from.1;
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let (ux, uy) = (dx / len, dy / len);
        let base = (tip.0 - ux * size, tip.1 - uy * size);
        let perpendicular = (-uy, ux);
        let half = size * 0.38;
        let p1 = (
            base.0 + perpendicular.0 * half,
            base.1 + perpendicular.1 * half,
        );
        let p2 = (
            base.0 - perpendicular.0 * half,
            base.1 - perpendicular.1 * half,
        );
        self.polygon(vec![tip, p1, p2], Some(color), None);
    }

    /// Rasterise at `scale` into a prepacked RGBA pixmap.
    pub fn rasterize(
        &self,
        engine: &mut TextEngine,
        scale: f32,
        background: Option<Rgba8>,
    ) -> Pixmap {
        let scale = scale.clamp(0.1, 8.0);
        let width = ((self.width * scale).ceil() as u32).max(1);
        let height = ((self.height * scale).ceil() as u32).max(1);
        let mut pixmap = Pixmap::new(width, height).unwrap_or_else(|| {
            Pixmap::new(1, 1).expect("1x1 pixmap")
        });
        let bg = background.or(self.background);
        if let Some(bg) = bg {
            pixmap.fill(bg.to_color());
        }
        for item in &self.items {
            match item {
                SceneItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    radius,
                    fill,
                    stroke,
                } => {
                    let path = rounded_rect_path(
                        x * scale,
                        y * scale,
                        width * scale,
                        height * scale,
                        radius * scale,
                    );
                    fill_and_stroke(&mut pixmap, &path, *fill, *stroke, scale);
                }
                SceneItem::Ellipse {
                    cx,
                    cy,
                    rx,
                    ry,
                    fill,
                    stroke,
                } => {
                    let path = ellipse_path(cx * scale, cy * scale, rx * scale, ry * scale);
                    fill_and_stroke(&mut pixmap, &path, *fill, *stroke, scale);
                }
                SceneItem::Polygon {
                    points,
                    fill,
                    stroke,
                } => {
                    let mut builder = PathBuilder::new();
                    if let Some((first, rest)) = points.split_first() {
                        builder.move_to(first.0 * scale, first.1 * scale);
                        for p in rest {
                            builder.line_to(p.0 * scale, p.1 * scale);
                        }
                        builder.close();
                        if let Some(path) = builder.finish() {
                            fill_and_stroke(&mut pixmap, &path, *fill, *stroke, scale);
                        }
                    }
                }
                SceneItem::Path(scene_path) => {
                    let mut builder = PathBuilder::new();
                    let mut valid = false;
                    for op in &scene_path.ops {
                        match *op {
                            PathOp::Move(x, y) => {
                                builder.move_to(x * scale, y * scale);
                                valid = true;
                            }
                            PathOp::Line(x, y) => {
                                builder.line_to(x * scale, y * scale);
                            }
                            PathOp::Quad(x1, y1, x, y) => {
                                builder.quad_to(
                                    x1 * scale,
                                    y1 * scale,
                                    x * scale,
                                    y * scale,
                                );
                            }
                            PathOp::Cubic(x1, y1, x2, y2, x, y) => {
                                builder.cubic_to(
                                    x1 * scale,
                                    y1 * scale,
                                    x2 * scale,
                                    y2 * scale,
                                    x * scale,
                                    y * scale,
                                );
                            }
                            PathOp::Close => builder.close(),
                        }
                    }
                    if valid {
                        if let Some(path) = builder.finish() {
                            fill_and_stroke(
                                &mut pixmap,
                                &path,
                                scene_path.fill,
                                scene_path.stroke,
                                scale,
                            );
                        }
                    }
                }
                SceneItem::Text(text) => {
                    engine.draw_shaped_skia(
                        &mut pixmap,
                        &text.shaped,
                        (text.x, text.y),
                        text.color.to_color(),
                        scale,
                    );
                }
            }
        }
        pixmap
    }

    pub fn encode_png(
        &self,
        engine: &mut TextEngine,
        scale: f32,
        background: Option<Rgba8>,
    ) -> Result<Vec<u8>, String> {
        let pixmap = self.rasterize(engine, scale, background);
        pixmap
            .encode_png()
            .map_err(|e| format!("Could not encode PNG: {e}"))
    }
}

fn fill_and_stroke(
    pixmap: &mut Pixmap,
    path: &tiny_skia::Path,
    fill: Option<Rgba8>,
    stroke: Option<SceneStroke>,
    scale: f32,
) {
    if let Some(fill) = fill {
        let mut paint = Paint::default();
        paint.set_color(fill.to_color());
        paint.anti_alias = true;
        pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    if let Some(stroke) = stroke {
        let mut paint = Paint::default();
        paint.set_color(stroke.color.to_color());
        paint.anti_alias = true;
        let mut ts = Stroke {
            width: (stroke.width * scale).max(0.5),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        if let Some((dash, gap)) = stroke.dash {
            if let Some(d) = StrokeDash::new(vec![dash * scale, gap * scale], 0.0) {
                ts.dash = Some(d);
            }
        }
        pixmap.stroke_path(path, &paint, &ts, Transform::identity(), None);
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.quad_to(x + w, y, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.quad_to(x + w, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.quad_to(x, y + h, x, y + h - r);
    b.line_to(x, y + r);
    b.quad_to(x, y, x + r, y);
    b.close();
    b.finish().expect("rounded rect path")
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> tiny_skia::Path {
    let k = 0.552_284_8;
    let mut b = PathBuilder::new();
    b.move_to(cx - rx, cy);
    b.cubic_to(
        cx - rx,
        cy - ry * k,
        cx - rx * k,
        cy - ry,
        cx,
        cy - ry,
    );
    b.cubic_to(cx + rx * k, cy - ry, cx + rx, cy - ry * k, cx + rx, cy);
    b.cubic_to(cx + rx, cy + ry * k, cx + rx * k, cy + ry, cx, cy + ry);
    b.cubic_to(cx - rx * k, cy + ry, cx - rx, cy + ry * k, cx - rx, cy);
    b.close();
    b.finish().expect("ellipse path")
}

/// A text label measured and shaped through the shared text engine.
#[derive(Debug, Clone)]
pub struct Label {
    pub shaped: ShapedText,
    pub width: f32,
    pub height: f32,
}

pub struct TextFactory<'a> {
    pub engine: &'a mut TextEngine,
    pub family: String,
    pub code_family: String,
}

impl<'a> TextFactory<'a> {
    pub fn new(engine: &'a mut TextEngine) -> Self {
        let family = engine.default_reading_family();
        let code_family = engine.default_code_family();
        Self {
            engine,
            family,
            code_family,
        }
    }

    pub fn layout(
        &mut self,
        text: &str,
        size: f32,
        weight: u16,
        max_width: f32,
        align: daftar_core::settings::TextAlign,
        family: Option<&str>,
    ) -> Label {
        let family_name = family.map(|f| f.to_string()).unwrap_or_else(|| self.family.clone());
        let prelim = self.layout_once(
            text,
            size,
            weight,
            0.0,
            daftar_core::settings::TextAlign::Left,
            &family_name,
        );
        if max_width > 1.0 && prelim.width > max_width {
            self.layout_once(text, size, weight, max_width, align, &family_name)
        } else {
            self.layout_once(
                text,
                size,
                weight,
                prelim.width.max(1.0),
                align,
                &family_name,
            )
        }
    }

    fn layout_once(
        &mut self,
        text: &str,
        size: f32,
        weight: u16,
        width: f32,
        align: daftar_core::settings::TextAlign,
        family: &str,
    ) -> Label {
        let opts = daftar_text::LayoutOptions {
            width,
            family: family.to_string(),
            size,
            weight,
            line_height: 1.32,
            letter_spacing: 0.0,
            align,
            align_start_rtl: daftar_core::direction::detect_direction(text).is_rtl(),
            paragraph_direction: None,
            code_family: self.code_family.clone(),
            code_size: size * 0.92,
            wrap: width > 1.0,
            base_italic: false,
            base_code: false,
        };
        let shaped = self
            .engine
            .layout_text(text, &opts, &daftar_core::markdown::SpanStyle::default());
        let measured = shaped
            .lines
            .iter()
            .map(|l| l.width)
            .fold(0.0f32, f32::max);
        let used_width = if width > 1.0 { width } else { measured };
        Label {
            width: used_width.max(1.0),
            height: shaped.height.max(size),
            shaped,
        }
    }

    pub fn layout_mono(
        &mut self,
        text: &str,
        size: f32,
        max_width: f32,
    ) -> Label {
        let opts = daftar_text::LayoutOptions {
            width: max_width,
            family: self.code_family.clone(),
            size,
            weight: 400,
            line_height: 1.3,
            letter_spacing: 0.0,
            align: daftar_core::settings::TextAlign::Left,
            align_start_rtl: false,
            paragraph_direction: Some(daftar_core::direction::ResolvedDirection::Ltr),
            code_family: self.code_family.clone(),
            code_size: size,
            wrap: max_width > 1.0,
            base_italic: false,
            base_code: false,
        };
        let shaped = self
            .engine
            .layout_text(text, &opts, &daftar_core::markdown::SpanStyle::default());
        let measured = shaped
            .lines
            .iter()
            .map(|l| l.width)
            .fold(0.0f32, f32::max);
        Label {
            width: if max_width > 1.0 { max_width } else { measured.max(1.0) },
            height: shaped.height.max(size),
            shaped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_core::markdown::SpanStyle;

    fn engine() -> TextEngine {
        TextEngine::new(None, "fa-IR")
    }

    #[test]
    fn rasterizes_shapes_and_persian_text() {
        let mut engine = engine();
        let mut scene = Scene::new(300.0, 160.0, Some(Rgba8::from_hex(0xFFFFFF)));
        let mut factory = TextFactory::new(&mut engine);
        let label = factory.layout(
            "سلام دنیا",
            18.0,
            600,
            240.0,
            daftar_core::settings::TextAlign::Center,
            None,
        );
        scene.rect(
            20.0,
            20.0,
            260.0,
            100.0,
            10.0,
            Some(Rgba8::from_hex(0xF5F5F5)),
            Some(SceneStroke::solid(Rgba8::from_hex(0xCCCCCC), 1.0)),
        );
        scene.text(label.shaped.clone(), 40.0, 50.0, Rgba8::from_hex(0x222222));
        scene.arrow(
            (20.0, 140.0),
            (280.0, 140.0),
            SceneStroke::solid(Rgba8::from_hex(0x666666), 1.5),
            9.0,
        );
        let pixmap = scene.rasterize(&mut engine, 2.0, None);
        assert_eq!(pixmap.width(), 600);
        assert_eq!(pixmap.height(), 320);
        let ink = pixmap.pixels().iter().filter(|p| p.alpha() > 0).count();
        assert!(ink > 1000);
        let png = scene.encode_png(&mut engine, 1.0, None).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn rtl_text_is_positioned_by_shaping() {
        let mut engine = engine();
        let mut factory = TextFactory::new(&mut engine);
        let label = factory.layout(
            "متن راست به چپ",
            16.0,
            400,
            200.0,
            daftar_core::settings::TextAlign::Start,
            None,
        );
        assert!(label.shaped.lines[0].rtl);
        let _ = SpanStyle::default();
    }

    #[test]
    fn rounded_rect_path_is_valid() {
        let p = rounded_rect_path(0.0, 0.0, 100.0, 50.0, 8.0);
        assert!(p.segments().count() > 0);
    }

    #[test]
    fn color_mixing_works() {
        let a = Rgba8::rgb(0, 0, 0);
        let b = Rgba8::rgb(255, 255, 255);
        let m = a.mix(b, 0.5);
        assert_eq!(m.r, 128);
    }
}