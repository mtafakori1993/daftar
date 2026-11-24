//! Layout of Markdown documents for the reader.
//!
//! All text is shaped through `daftar-text` (Unicode bidi + HarfBuzz-class shaping). Elements are
//! flattened in document order with source ranges so selection, highlights and bookmarks can be
//! mapped back to the original `.md` file.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daftar_core::direction::{Direction, ResolvedDirection, detect_direction};
use daftar_core::markdown::{
    Align, Block, ImageRef, ListItem, MdDocument, QuoteKind, RichText,
};
use daftar_core::settings::TextAlign;
use daftar_text::{LayoutOptions, ShapedText, TextEngine};
use egui::TextureHandle;

use crate::code::CodeHighlighter;
use crate::images::image_dimensions;
use crate::theme::ReaderColors;

#[derive(Debug, Clone)]
pub struct ReaderConfig {
    pub family: String,
    pub code_family: String,
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
    pub paragraph_spacing: f32,
    pub letter_spacing: f32,
    pub heading_spacing: f32,
    pub reading_width: f32,
    pub text_align: TextAlign,
    pub code_font_size: f32,
    pub theme_id: String,
    pub dark: bool,
    pub doc_direction: Direction,
    pub resolved_direction: ResolvedDirection,
    pub max_diagram_height: f32,
    pub fit_diagrams: bool,
    pub show_diagram_edge_labels: bool,
}

impl Default for ReaderConfig {
    fn default() -> Self {
        Self {
            family: "Vazirmatn".into(),
            code_family: "JetBrains Mono".into(),
            font_size: 17.0,
            font_weight: 400,
            line_height: 1.85,
            paragraph_spacing: 14.0,
            letter_spacing: 0.0,
            heading_spacing: 18.0,
            reading_width: 760.0,
            text_align: TextAlign::Start,
            code_font_size: 13.5,
            theme_id: "spectrum-light".into(),
            dark: false,
            doc_direction: Direction::Auto,
            resolved_direction: ResolvedDirection::Ltr,
            max_diagram_height: 480.0,
            fit_diagrams: true,
            show_diagram_edge_labels: true,
        }
    }
}

impl ReaderConfig {
    pub fn key(&self) -> String {
        format!(
            "{}|{}|{:.2}|{}|{:.2}|{:.2}|{:.2}|{:.2}|{:.1}|{:?}|{:.2}|{}|{}|{:?}",
            self.family,
            self.code_family,
            self.font_size,
            self.font_weight,
            self.line_height,
            self.paragraph_spacing,
            self.letter_spacing,
            self.heading_spacing,
            self.reading_width,
            self.text_align,
            self.code_font_size,
            self.theme_id,
            self.dark,
            self.resolved_direction,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Ordered(u64),
    Task(bool),
}

#[derive(Debug, Clone)]
pub struct TableLayout {
    pub col_widths: Vec<f32>,
    pub header: Vec<ShapedText>,
    pub rows: Vec<Vec<ShapedText>>,
    pub header_height: f32,
    pub row_heights: Vec<f32>,
    pub aligns: Vec<Align>,
    pub total_height: f32,
}

#[derive(Debug, Clone)]
pub enum ElementKind {
    Heading {
        shaped: ShapedText,
        rt: RichText,
        level: u8,
        id: String,
    },
    Paragraph {
        shaped: ShapedText,
        rt: RichText,
    },
    ListItem {
        shaped: ShapedText,
        rt: RichText,
        marker: ListMarker,
        marker_left: f32,
        text_left: f32,
    },
    Code {
        shaped: ShapedText,
        code: String,
        language: Option<String>,
        tokens: Vec<(std::ops::Range<usize>, egui::Color32)>,
        header_height: f32,
    },
    Table {
        layout: TableLayout,
    },
    Image {
        path: PathBuf,
        alt: String,
        width: f32,
        height: f32,
        error: Option<String>,
    },
    Diagram {
        key: u64,
        width: f32,
        height: f32,
        error: Option<String>,
    },
    Math {
        shaped: ShapedText,
        tex: String,
    },
    Html {
        shaped: ShapedText,
        html: String,
    },
    Rule,
    FootnoteLabel {
        shaped: ShapedText,
        label: String,
    },
    Empty,
}

#[derive(Debug, Clone)]
pub struct Element {
    pub top: f32,
    pub height: f32,
    pub left: f32,
    /// Left edge for selectable text (markers sit to the left).
    pub text_left: f32,
    pub source_range: std::ops::Range<usize>,
    pub block_index: usize,
    pub kind: ElementKind,
}

impl Element {
    pub fn selectable_text(&self) -> Option<(&RichText, &ShapedText)> {
        match &self.kind {
            ElementKind::Heading { rt, shaped, .. }
            | ElementKind::Paragraph { rt, shaped }
            | ElementKind::ListItem { rt, shaped, .. } => Some((rt, shaped)),
            _ => None,
        }
    }

    pub fn text_top(&self) -> f32 {
        self.top
    }
}

#[derive(Debug, Clone)]
pub struct QuoteBand {
    pub top: f32,
    pub bottom: f32,
    pub depth: usize,
    pub kind: Option<QuoteKind>,
}

#[derive(Debug, Clone)]
pub struct ReaderLayout {
    pub column_width: f32,
    pub left: f32,
    pub total_height: f32,
    pub elements: Vec<Element>,
    pub quotes: Vec<QuoteBand>,
    pub headings: HashMap<String, f32>,
    pub footnotes: HashMap<String, f32>,
    pub diagram_keys: Vec<u64>,
    pub available_width: f32,
}

pub struct DiagramEntry {
    pub built: Option<Arc<daftar_diagram::BuiltDiagram>>,
    pub error: Option<String>,
    pub texture: Option<(u64, TextureHandle)>,
    pub source: String,
    pub dark: bool,
}

pub type DiagramCache = HashMap<u64, DiagramEntry>;

pub fn diagram_theme(colors: &ReaderColors) -> daftar_diagram::DiagramTheme {
    let to = |c: egui::Color32| daftar_diagram::Rgba8::rgba(c.r(), c.g(), c.b(), c.a());
    let mut theme = if colors.dark {
        daftar_diagram::DiagramTheme::dark()
    } else {
        daftar_diagram::DiagramTheme::light()
    };
    theme.background = to(colors.background);
    theme.surface = to(colors.background);
    theme.node_fill = if colors.dark {
        daftar_diagram::Rgba8::from_hex(0x2A2A2A)
    } else {
        daftar_diagram::Rgba8::from_hex(0xF8F8F8)
    };
    theme.node_stroke = to(colors.table_border);
    theme.text = to(colors.text);
    theme.muted = to(colors.muted);
    theme.accent = to(colors.link);
    theme.line = to(colors.muted);
    theme.note_fill = if colors.dark {
        daftar_diagram::Rgba8::from_hex(0x3A3524)
    } else {
        daftar_diagram::Rgba8::from_hex(0xFFF7D6)
    };
    theme.note_stroke = to(colors.quote_border);
    theme.group_fill = if colors.dark {
        daftar_diagram::Rgba8::from_hex(0x242424)
    } else {
        daftar_diagram::Rgba8::from_hex(0xF5F5F5)
    };
    theme.group_stroke = to(colors.table_border);
    theme.grid = to(colors.table_border);
    theme
}

pub fn diagram_key(source: &str, dark: bool, theme_id: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    dark.hash(&mut hasher);
    theme_id.hash(&mut hasher);
    hasher.finish()
}

pub fn build_layout(
    engine: &mut TextEngine,
    doc: &MdDocument,
    config: &ReaderConfig,
    available_width: f32,
    base_dir: &Path,
    code: &CodeHighlighter,
    diagrams: &mut DiagramCache,
    colors: &ReaderColors,
) -> ReaderLayout {
    let column_width = (config.reading_width)
        .min((available_width - 64.0).max(320.0))
        .max(280.0);
    let left = ((available_width - column_width) * 0.5).max(24.0);
    let mut builder = Builder {
        engine,
        config,
        doc,
        column_width,
        left,
        base_dir,
        code,
        diagrams,
        elements: Vec::new(),
        quotes: Vec::new(),
        headings: HashMap::new(),
        footnotes: HashMap::new(),
        diagram_keys: Vec::new(),
        y: 28.0,
        block_index: 0,
        colors: colors.clone(),
        current_block_range: 0..0,
    };
    let blocks = doc.blocks.clone();
    for (index, block) in blocks.iter().enumerate() {
        builder.block_index = index;
        builder.block(block, 0, 0, None);
    }
    builder.add_footnotes();
    let total_height = builder.y + 56.0;
    ReaderLayout {
        column_width,
        left,
        total_height,
        elements: builder.elements,
        quotes: builder.quotes,
        headings: builder.headings,
        footnotes: builder.footnotes,
        diagram_keys: builder.diagram_keys,
        available_width,
    }
}

impl ReaderColors {
    fn placeholder() -> Self {
        Self {
            background: egui::Color32::WHITE,
            text: egui::Color32::BLACK,
            muted: egui::Color32::GRAY,
            heading: egui::Color32::BLACK,
            link: egui::Color32::BLUE,
            selection: egui::Color32::LIGHT_BLUE,
            code_background: egui::Color32::from_gray(245),
            code_text: egui::Color32::BLACK,
            code_border: egui::Color32::GRAY,
            quote_background: egui::Color32::from_gray(248),
            quote_border: egui::Color32::GRAY,
            table_border: egui::Color32::GRAY,
            table_header_background: egui::Color32::from_gray(245),
            rule: egui::Color32::GRAY,
            highlight_palette: Vec::new(),
            dark: false,
        }
    }
}

struct Builder<'a> {
    engine: &'a mut TextEngine,
    config: &'a ReaderConfig,
    doc: &'a MdDocument,
    column_width: f32,
    left: f32,
    base_dir: &'a Path,
    code: &'a CodeHighlighter,
    diagrams: &'a mut DiagramCache,
    elements: Vec<Element>,
    quotes: Vec<QuoteBand>,
    headings: HashMap<String, f32>,
    footnotes: HashMap<String, f32>,
    diagram_keys: Vec<u64>,
    y: f32,
    block_index: usize,
    colors: ReaderColors,
    current_block_range: std::ops::Range<usize>,
}

impl Builder<'_> {
    fn block(&mut self, block: &Block, depth: usize, quote_depth: usize, marker: Option<ListMarker>) {
        let indent = depth as f32 * 22.0 + quote_depth as f32 * 18.0;
        match block {
            Block::Heading {
                level,
                id,
                text,
                source_range,
            } => {
                let (size, weight, before, after) = match level {
                    1 => (self.config.font_size * 1.75, 700, 20.0, 12.0),
                    2 => (self.config.font_size * 1.45, 700, 24.0, 10.0),
                    3 => (self.config.font_size * 1.24, 600, 20.0, 8.0),
                    4 => (self.config.font_size * 1.1, 600, 16.0, 6.0),
                    _ => (self.config.font_size, 600, 14.0, 6.0),
                };
                if !self.elements.is_empty() {
                    self.y += if level <= &2 { self.config.heading_spacing.max(before) } else { before };
                }
                let shaped = self.shape(
                    text,
                    size,
                    weight,
                    self.column_width - indent - 20.0,
                    TextAlign::Start,
                );
                let height = shaped.height + after;
                self.push(
                    indent,
                    indent,
                    height,
                    source_range.clone(),
                    ElementKind::Heading {
                        shaped,
                        rt: text.clone(),
                        level: *level,
                        id: id.clone(),
                    },
                );
                self.headings.insert(id.clone(), self.elements.last().map(|e| e.top).unwrap_or(self.y));
            }
            Block::Paragraph { text, source_range } => {
                let rtl = self.config.resolved_direction.is_rtl();
                let has_marker = marker.is_some();
                let text_left = indent + if has_marker && !rtl { 24.0 } else { 0.0 };
                let marker_gutter = if has_marker && rtl { 28.0 } else { 0.0 };
                let shaped = self.shape(
                    text,
                    self.config.font_size,
                    self.config.font_weight,
                    self.column_width - text_left - 12.0 - marker_gutter,
                    self.config.text_align,
                );
                let height = shaped.height + self.config.paragraph_spacing;
                let kind = if let Some(marker) = marker {
                    ElementKind::ListItem {
                        shaped,
                        rt: text.clone(),
                        marker,
                        marker_left: if rtl {
                            self.left + self.column_width - 24.0
                        } else {
                            self.left + indent
                        },
                        text_left: self.left + text_left,
                    }
                } else {
                    ElementKind::Paragraph {
                        shaped,
                        rt: text.clone(),
                    }
                };
                self.push(text_left, text_left, height, source_range.clone(), kind);
            }
            Block::List {
                items,
                ordered,
                start,
                ..
            } => {
                let mut number = *start;
                for item in items {
                    self.list_item(item, depth, quote_depth, *ordered, number);
                    if *ordered {
                        number += 1;
                    }
                }
            }
            Block::Code {
                language,
                code,
                source_range,
            } => {
                if language
                    .as_deref()
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid"))
                {
                    self.diagram(code, indent, source_range.clone());
                    return;
                }
                let mut rt = RichText::new(0..code.len());
                rt.push(
                    code,
                    &daftar_core::markdown::SpanStyle {
                        code: true,
                        ..Default::default()
                    },
                    0..code.len(),
                );
                let shaped = self.shape_plain(
                    &rt,
                    self.config.code_font_size,
                    400,
                    self.column_width - indent - 40.0,
                    TextAlign::Left,
                    Some(ResolvedDirection::Ltr),
                    true,
                );
                let highlighted = self
                    .code
                    .highlight(code, language.as_deref(), self.config.dark);
                let header_height = 30.0;
                let height = header_height + shaped.height + 26.0;
                self.push(
                    indent,
                    indent,
                    height,
                    source_range.clone(),
                    ElementKind::Code {
                        shaped,
                        code: code.clone(),
                        language: language.clone(),
                        tokens: highlighted
                            .tokens
                            .into_iter()
                            .map(|t| (t.range, t.color))
                            .collect(),
                        header_height,
                    },
                );
            }
            Block::Quote {
                kind,
                blocks,
                source_range,
            } => {
                let start_y = self.y;
                let label_space = if kind.is_some() { 18.0 } else { 0.0 };
                self.y += label_space;
                let start_index = self.elements.len();
                for inner in blocks {
                    self.block(inner, depth, quote_depth + 1, None);
                }
                let bottom = self.y.max(start_y + 30.0);
                self.quotes.push(QuoteBand {
                    top: start_y - 4.0,
                    bottom,
                    depth: quote_depth,
                    kind: *kind,
                });
                let _ = (source_range, start_index);
            }
            Block::Table {
                align,
                header,
                rows,
                source_range,
            } => {
                let layout = self.table(align, header, rows, indent);
                let height = layout.total_height + 18.0;
                self.push(
                    indent,
                    indent,
                    height,
                    source_range.clone(),
                    ElementKind::Table { layout },
                );
            }
            Block::Rule { source_range } => {
                self.push(
                    indent,
                    indent,
                    26.0,
                    source_range.clone(),
                    ElementKind::Rule,
                );
            }
            Block::Image { image, source_range } => {
                self.image(image, indent, source_range.clone());
            }
            Block::Math { tex, source_range } => {
                let display = tex_to_display(tex);
                let shaped = self.shape_plain(
                    &rich_text(&display),
                    self.config.font_size + 1.0,
                    400,
                    self.column_width - indent - 40.0,
                    TextAlign::Center,
                    None,
                    false,
                );
                let height = shaped.height + 20.0;
                self.push(
                    indent,
                    indent,
                    height,
                    source_range.clone(),
                    ElementKind::Math {
                        shaped,
                        tex: tex.clone(),
                    },
                );
            }
            Block::Html { html, source_range } => {
                let text = strip_html(html);
                let shaped = self.shape_plain(
                    &rich_text(&text),
                    self.config.font_size * 0.85,
                    400,
                    self.column_width - indent - 20.0,
                    TextAlign::Start,
                    None,
                    false,
                );
                let height = shaped.height + 12.0;
                self.push(
                    indent,
                    indent,
                    height,
                    source_range.clone(),
                    ElementKind::Html {
                        shaped,
                        html: html.clone(),
                    },
                );
            }
        }
    }

    fn list_item(
        &mut self,
        item: &ListItem,
        depth: usize,
        quote_depth: usize,
        ordered: bool,
        number: u64,
    ) {
        let marker = if let Some(checked) = item.task {
            ListMarker::Task(checked)
        } else if ordered {
            ListMarker::Ordered(number)
        } else {
            ListMarker::Bullet
        };
        let mut first = true;
        for block in &item.blocks {
            if first {
                self.block(block, depth, quote_depth, Some(marker));
                first = false;
            } else {
                match block {
                    Block::List { .. } => self.block(block, depth + 1, quote_depth, None),
                    _ => self.block(block, depth, quote_depth, None),
                }
            }
        }
        if first {
            let indent = depth as f32 * 22.0 + quote_depth as f32 * 18.0;
            let shaped = self.shape_plain(
                &rich_text(" "),
                self.config.font_size,
                400,
                40.0,
                TextAlign::Start,
                None,
                false,
            );
            self.push(
                indent,
                indent + 24.0,
                22.0,
                item.source_range.clone(),
                ElementKind::ListItem {
                    shaped,
                    rt: rich_text(""),
                    marker,
                    marker_left: self.left + indent,
                    text_left: self.left + indent + 24.0,
                },
            );
        }
    }

    fn diagram(&mut self, source: &str, indent: f32, source_range: std::ops::Range<usize>) {
        let key = diagram_key(source, self.config.dark, &self.config.theme_id);
        self.diagram_keys.push(key);
        let theme = diagram_theme(&self.colors);
        let entry = self.diagrams.entry(key).or_insert_with(|| {
            match daftar_diagram::build(source, &theme, self.engine) {
                Ok(built) => DiagramEntry {
                    built: Some(Arc::new(built)),
                    error: None,
                    texture: None,
                    source: source.to_string(),
                    dark: self.config.dark,
                },
                Err(error) => DiagramEntry {
                    built: None,
                    error: Some(error.to_string()),
                    texture: None,
                    source: source.to_string(),
                    dark: self.config.dark,
                },
            }
        });
        let max_width = (self.column_width - indent - 32.0).max(160.0);
        let max_height = self.config.max_diagram_height.max(180.0);
        if let Some(built) = &entry.built {
            let fit = (max_width / built.width)
                .min(max_height / built.height)
                .min(if self.config.fit_diagrams { 1.0 } else { 1.5 })
                .max(0.2);
            let width = built.width * fit;
            let height = built.height * fit;
            self.push(
                indent,
                indent,
                height + 20.0,
                source_range,
                ElementKind::Diagram {
                    key,
                    width,
                    height,
                    error: None,
                },
            );
        } else {
            let error = entry
                .error
                .clone()
                .unwrap_or_else(|| crate::i18n::t("Diagram could not be rendered.").to_string());
            self.push(
                indent,
                indent,
                104.0,
                source_range,
                ElementKind::Diagram {
                    key,
                    width: max_width,
                    height: 84.0,
                    error: Some(error),
                },
            );
        }
    }

    fn image(&mut self, image: &ImageRef, indent: f32, source_range: std::ops::Range<usize>) {
        let path = resolve_path(self.base_dir, &image.src);
        let max_width = (self.column_width - indent - 32.0).max(120.0);
        match path {
            None => {
                self.push_image_error(
                    indent,
                    image.alt.clone(),
                    crate::i18n::t("Images from the web are not loaded. Save the image locally and use a relative path.").into(),
                    source_range,
                );
            }
            Some(path) => {
                let dims = image_dimensions(&path);
                match dims {
                    Some((w, h)) if w > 0 && h > 0 => {
                        let mut width = w as f32;
                        let mut height = h as f32;
                        if width > max_width {
                            let scale = max_width / width;
                            width *= scale;
                            height *= scale;
                        }
                        let max_height = self.config.max_diagram_height.max(240.0);
                        if height > max_height {
                            let scale = max_height / height;
                            width *= scale;
                            height = max_height;
                        }
                        self.push(
                            indent,
                            indent,
                            height + 20.0,
                            source_range,
                            ElementKind::Image {
                                path,
                                alt: image.alt.clone(),
                                width,
                                height,
                                error: None,
                            },
                        );
                    }
                    _ => self.push_image_error(
                        indent,
                        image.alt.clone(),
                        crate::i18n::tf("Could not load image: {}", &[&path.display().to_string()]),
                        source_range,
                    ),
                }
            }
        }
    }

    fn push_image_error(
        &mut self,
        indent: f32,
        alt: String,
        error: String,
        source_range: std::ops::Range<usize>,
    ) {
        self.push(
            indent,
            indent,
            86.0,
            source_range,
            ElementKind::Image {
                path: PathBuf::new(),
                alt,
                width: 0.0,
                height: 0.0,
                error: Some(error),
            },
        );
    }

    fn table(
        &mut self,
        aligns: &[Align],
        header: &[RichText],
        rows: &[Vec<RichText>],
        indent: f32,
    ) -> TableLayout {
        let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        let padding = 10.0f32;
        let available = self.column_width - indent - 16.0;
        let cell_max = 320.0;
        let mut naturals = vec![60.0f32; cols];
        for (c, cell) in header.iter().enumerate() {
            let s = self.shape_plain(
                cell,
                self.config.font_size * 0.82,
                600,
                0.0,
                TextAlign::Left,
                None,
                false,
            );
            naturals[c] = naturals[c].max(s.lines.iter().map(|l| l.width).fold(0.0, f32::max));
        }
        for row in rows {
            for (c, cell) in row.iter().enumerate() {
                if c >= cols {
                    continue;
                }
                let s = self.shape_plain(
                    cell,
                    self.config.font_size * 0.82,
                    400,
                    0.0,
                    TextAlign::Left,
                    None,
                    false,
                );
                naturals[c] = naturals[c].max(s.lines.iter().map(|l| l.width).fold(0.0, f32::max));
            }
        }
        let total_natural: f32 = naturals.iter().map(|w| w + padding * 2.0).sum();
        let widths: Vec<f32> = if total_natural > available && cols > 0 {
            naturals
                .iter()
                .map(|w| {
                    ((w + padding * 2.0) / total_natural * available)
                        .max(56.0)
                })
                .collect()
        } else {
            naturals.iter().map(|w| w + padding * 2.0).collect()
        };
        let widths: Vec<f32> = {
            let sum: f32 = widths.iter().sum();
            if sum > available && sum > 0.0 {
                let scale = available / sum;
                widths.iter().map(|w| (w * scale).max(44.0)).collect()
            } else {
                widths
            }
        };
        let header_cells: Vec<ShapedText> = (0..cols)
            .map(|c| {
                let cell = header.get(c).cloned().unwrap_or_else(|| rich_text(""));
                self.shape_plain(
                    &cell,
                    self.config.font_size * 0.82,
                    600,
                    (widths[c] - padding * 2.0).max(30.0),
                    align_to_text(aligns.get(c).copied().unwrap_or(Align::None)),
                    None,
                    true,
                )
            })
            .collect();
        let header_height = header_cells
            .iter()
            .map(|s| s.height)
            .fold(0.0f32, f32::max)
            .max(24.0)
            + 14.0;
        let mut row_cells = Vec::new();
        let mut row_heights = Vec::new();
        for row in rows {
            let mut cells = Vec::new();
            let mut height = 20.0f32;
            for c in 0..cols {
                let cell = row.get(c).cloned().unwrap_or_else(|| rich_text(""));
                let shaped = self.shape_plain(
                    &cell,
                    self.config.font_size * 0.82,
                    400,
                    (widths[c] - padding * 2.0).max(30.0),
                    align_to_text(aligns.get(c).copied().unwrap_or(Align::None)),
                    None,
                    true,
                );
                height = height.max(shaped.height + 12.0);
                cells.push(shaped);
            }
            row_cells.push(cells);
            row_heights.push(height);
        }
        let total_height = header_height + row_heights.iter().sum::<f32>() + 8.0;
        TableLayout {
            col_widths: widths,
            header: header_cells,
            rows: row_cells,
            header_height,
            row_heights,
            aligns: aligns.to_vec(),
            total_height,
        }
    }

    fn add_footnotes(&mut self) {
        if self.doc.footnotes.is_empty() {
            return;
        }
        self.y += 24.0;
        let heading = self.shape_plain(
            &rich_text(crate::i18n::t("Footnotes")),
            self.config.font_size * 1.2,
            700,
            self.column_width - 32.0,
            TextAlign::Start,
            Some(self.config.resolved_direction),
            false,
        );
        let heading_y = self.y;
        self.push(
            0.0,
            0.0,
            heading.height + 14.0,
            0..0,
            ElementKind::Heading {
                shaped: heading,
                rt: rich_text(crate::i18n::t("Footnotes")),
                level: 2,
                id: "__footnotes".into(),
            },
        );
        self.headings.insert("__footnotes".into(), heading_y);
        for footnote in &self.doc.footnotes {
            let label_text = format!("[{}]", footnote.label);
            let label = self.shape_plain(
                &rich_text(&label_text),
                self.config.font_size * 0.78,
                600,
                90.0,
                TextAlign::Start,
                None,
                false,
            );
            let top = self.y;
            self.push(
                12.0,
                12.0,
                label.height + 4.0,
                self.current_block_range.clone(),
                ElementKind::FootnoteLabel {
                    shaped: label,
                    label: footnote.label.clone(),
                },
            );
            self.footnotes.insert(footnote.label.clone(), top);
            for block in &footnote.blocks {
                self.block(block, 1, 0, None);
            }
            self.y += 4.0;
        }
    }

    fn push(
        &mut self,
        left: f32,
        text_left: f32,
        height: f32,
        source_range: std::ops::Range<usize>,
        kind: ElementKind,
    ) {
        let top = self.y;
        self.elements.push(Element {
            top,
            height,
            left: self.left + left,
            text_left: self.left + text_left,
            source_range,
            block_index: self.block_index,
            kind,
        });
        self.y += height;
    }

    fn shape(
        &mut self,
        rt: &RichText,
        size: f32,
        weight: u16,
        width: f32,
        align: TextAlign,
    ) -> ShapedText {
        self.shape_plain(rt, size, weight, width, align, None, true)
    }

    fn shape_plain(
        &mut self,
        rt: &RichText,
        size: f32,
        weight: u16,
        width: f32,
        align: TextAlign,
        direction: Option<ResolvedDirection>,
        wrap: bool,
    ) -> ShapedText {
        let manual = direction.or(match self.config.doc_direction {
            Direction::Ltr => Some(ResolvedDirection::Ltr),
            Direction::Rtl => Some(ResolvedDirection::Rtl),
            Direction::Auto => None,
        });
        let opts = LayoutOptions {
            width: width.max(40.0),
            family: self.config.family.clone(),
            size,
            weight,
            line_height: self.config.line_height,
            letter_spacing: self.config.letter_spacing,
            align,
            align_start_rtl: self.config.resolved_direction.is_rtl(),
            paragraph_direction: manual,
            code_family: self.config.code_family.clone(),
            code_size: self.config.code_font_size,
            wrap,
            base_italic: false,
            base_code: false,
        };
        self.engine.layout_rich(rt, &opts)
    }
}

fn rich_text(text: &str) -> RichText {
    let mut rt = RichText::new(0..text.len());
    rt.push(text, &Default::default(), 0..text.len());
    rt
}

fn align_to_text(align: Align) -> TextAlign {
    match align {
        Align::None => TextAlign::Start,
        Align::Left => TextAlign::Left,
        Align::Center => TextAlign::Center,
        Align::Right => TextAlign::Right,
    }
}

pub fn resolve_path(base: &Path, src: &str) -> Option<PathBuf> {
    let src = src.trim();
    if src.is_empty() {
        return None;
    }
    if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
        return None;
    }
    let src = src
        .strip_prefix("file://")
        .unwrap_or(src)
        .split('#')
        .next()
        .unwrap_or(src)
        .split('?')
        .next()
        .unwrap_or(src);
    let decoded = urldecode(src);
    let path = Path::new(&decoded);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        Some(base.join(path))
    }
}

fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tex_to_display(tex: &str) -> String {
    daftar_core::util::tex_to_unicode(tex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_core::parse_markdown;

    fn config() -> ReaderConfig {
        ReaderConfig::default()
    }

    #[test]
    fn lays_out_paragraphs_and_headings() {
        let doc = parse_markdown("# Title\n\nSome text here.\n\n## Sub\n\nMore text.\n");
        let mut engine = TextEngine::new(None, "en-US");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &config(),
            900.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        assert!(layout.total_height > 100.0);
        assert_eq!(layout.headings.len(), 2);
        assert!(layout.elements.iter().any(|e| matches!(e.kind, ElementKind::Heading { .. })));
    }

    #[test]
    fn lays_out_persian_document_rtl() {
        let doc = parse_markdown("# سلام\n\nاین یک متن فارسی است که باید درست چیده شود.\n");
        let mut cfg = config();
        cfg.doc_direction = Direction::Auto;
        cfg.resolved_direction = ResolvedDirection::Rtl;
        let mut engine = TextEngine::new(None, "fa-IR");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &cfg,
            900.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        let element = layout
            .elements
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Paragraph { .. }))
            .unwrap();
        let (_, shaped) = element.selectable_text().unwrap();
        assert!(shaped.lines.iter().any(|l| l.rtl));
    }

    #[test]
    fn lays_out_lists_tasks_and_code() {
        let md = "- [x] done\n- [ ] todo\n\n```rust\nfn main() {}\n```\n";
        let doc = parse_markdown(md);
        let mut engine = TextEngine::new(None, "en-US");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &config(),
            800.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        assert!(layout
            .elements
            .iter()
            .any(|e| matches!(e.kind, ElementKind::ListItem { marker: ListMarker::Task(true), .. })));
        assert!(layout.elements.iter().any(|e| matches!(e.kind, ElementKind::Code { .. })));
    }

    #[test]
    fn lays_out_tables() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n";
        let doc = parse_markdown(md);
        let mut engine = TextEngine::new(None, "en-US");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &config(),
            800.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        let table = layout
            .elements
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Table { layout } => Some(layout),
                _ => None,
            })
            .unwrap();
        assert_eq!(table.col_widths.len(), 2);
        assert_eq!(table.rows.len(), 1);
    }

    #[test]
    fn lays_out_footnotes_and_math() {
        let md = "Text[^1]\n\n[^1]: Footnote.\n\n$$x^2 + y^2$$\n";
        let doc = parse_markdown(md);
        let mut engine = TextEngine::new(None, "en-US");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &config(),
            800.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        assert!(!layout.footnotes.is_empty());
        assert!(layout.elements.iter().any(|e| matches!(e.kind, ElementKind::Math { .. })));
    }

    #[test]
    fn lays_out_mermaid_diagram() {
        let md = "```mermaid\nflowchart LR\nA[One] --> B[Two]\n```\n";
        let doc = parse_markdown(md);
        let mut engine = TextEngine::new(None, "en-US");
        let code = CodeHighlighter::new();
        let mut cache = DiagramCache::new();
        let layout = build_layout(
            &mut engine,
            &doc,
            &config(),
            800.0,
            Path::new("/tmp"),
            &code,
            &mut cache,
            &ReaderColors::placeholder(),
        );
        // The code element is replaced by a diagram element.
        assert!(layout
            .elements
            .iter()
            .any(|e| matches!(e.kind, ElementKind::Diagram { .. })));
        assert!(!cache.is_empty());
    }
}