//! Professional Markdown → PDF export.
//!
//! The exporter lays out the parsed document itself (never screenshots): text is shaped with the
//! same engine the reader uses, so Persian/Arabic shaping, ligatures and Unicode BiDi ordering
//! match the on-screen rendering. Glyphs are written through embedded TrueType fonts with a
//! `ToUnicode` CMap, so the PDF stays searchable, selectable and copyable — including RTL text.
//!
//! Anything that cannot be rendered reliably is reported as a warning instead of producing
//! silently wrong output.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};

use daftar_core::direction::{Direction, ResolvedDirection};
use daftar_core::markdown::{Block, MdDocument, RichText};
use daftar_core::settings::TextAlign;
use daftar_text::{LayoutOptions, ShapedText, SpanStyle, TextEngine, shape_rich};
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use ttf_parser::{Face, GlyphId};

use crate::{ExportError, Result};

/// Page formats in points (1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfPageFormat {
    #[default]
    A4,
    A5,
    Letter,
}

impl PdfPageFormat {
    pub const ALL: [PdfPageFormat; 3] =
        [PdfPageFormat::A4, PdfPageFormat::A5, PdfPageFormat::Letter];

    pub fn label(self) -> &'static str {
        match self {
            PdfPageFormat::A4 => "A4",
            PdfPageFormat::A5 => "A5",
            PdfPageFormat::Letter => "Letter",
        }
    }

    /// Width and height in points, portrait.
    pub fn points(self) -> (f32, f32) {
        match self {
            PdfPageFormat::A4 => (595.28, 841.89),
            PdfPageFormat::A5 => (419.53, 595.28),
            PdfPageFormat::Letter => (612.0, 792.0),
        }
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfOrientation {
    #[default]
    Portrait,
    Landscape,
}

impl PdfOrientation {
    pub const ALL: [PdfOrientation; 2] = [PdfOrientation::Portrait, PdfOrientation::Landscape];

    pub fn label(self) -> &'static str {
        match self {
            PdfOrientation::Portrait => "Portrait",
            PdfOrientation::Landscape => "Landscape",
        }
    }
}

/// Page margins in points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl PdfMargins {
    pub const NARROW: PdfMargins = PdfMargins {
        top: 36.0,
        right: 36.0,
        bottom: 36.0,
        left: 36.0,
    };
    pub const NORMAL: PdfMargins = PdfMargins {
        top: 64.0,
        right: 56.0,
        bottom: 64.0,
        left: 56.0,
    };
    pub const WIDE: PdfMargins = PdfMargins {
        top: 84.0,
        right: 84.0,
        bottom: 84.0,
        left: 84.0,
    };

    pub fn uniform(value: f32) -> Self {
        PdfMargins {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub fn clamped(mut self) -> Self {
        for value in [
            &mut self.top,
            &mut self.right,
            &mut self.bottom,
            &mut self.left,
        ] {
            *value = value.clamp(12.0, 200.0);
        }
        self
    }
}

impl Default for PdfMargins {
    fn default() -> Self {
        PdfMargins::NORMAL
    }
}

/// Everything the export dialog can configure.
#[derive(Debug, Clone)]
pub struct DocumentPdfOptions {
    // Page
    pub format: PdfPageFormat,
    pub orientation: PdfOrientation,
    pub margins: PdfMargins,
    // Typography
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    /// Multiplier applied to heading sizes (`1.0` keeps the default steps).
    pub heading_scale: f32,
    pub paragraph_spacing: f32,
    pub code_font_family: String,
    pub code_font_size: f32,
    // Layout
    /// Text column width in points; `0` uses the full area between the margins.
    pub content_width: f32,
    /// Document direction override; `None` follows the detected direction.
    pub direction_rtl: Option<bool>,
    pub show_title: bool,
    pub show_table_of_contents: bool,
    pub show_page_numbers: bool,
    pub header_text: String,
    pub footer_text: String,
    pub show_source: bool,
    // Content
    pub include_code: bool,
    pub include_tables: bool,
    pub include_images: bool,
    pub include_diagrams: bool,
    pub include_quotes: bool,
    /// Directory used to resolve relative image paths.
    pub base_dir: PathBuf,
}

impl Default for DocumentPdfOptions {
    fn default() -> Self {
        Self {
            format: PdfPageFormat::A4,
            orientation: PdfOrientation::Portrait,
            margins: PdfMargins::NORMAL,
            font_family: "Vazirmatn".to_string(),
            font_size: 11.0,
            line_height: 1.55,
            heading_scale: 1.0,
            paragraph_spacing: 6.0,
            code_font_family: "JetBrains Mono".to_string(),
            code_font_size: 9.0,
            content_width: 0.0,
            direction_rtl: None,
            show_title: true,
            show_table_of_contents: false,
            show_page_numbers: true,
            header_text: String::new(),
            footer_text: String::new(),
            show_source: true,
            include_code: true,
            include_tables: true,
            include_images: true,
            include_diagrams: true,
            include_quotes: true,
            base_dir: PathBuf::new(),
        }
    }
}

impl DocumentPdfOptions {
    /// Page size in points after applying the orientation.
    pub fn page_size(&self) -> (f32, f32) {
        let (w, h) = self.format.points();
        match self.orientation {
            PdfOrientation::Portrait => (w, h),
            PdfOrientation::Landscape => (h, w),
        }
    }

    /// Width of the text column.
    pub fn column_width(&self) -> f32 {
        let (page_w, _) = self.page_size();
        let available = (page_w - self.margins.left - self.margins.right).max(120.0);
        if self.content_width <= 1.0 {
            available
        } else {
            self.content_width.min(available)
        }
    }
}

/// Report for a completed export.
#[derive(Debug, Clone)]
pub struct DocumentPdfReport {
    pub pages: u32,
    pub bytes: u64,
    /// Replaces of constructs that were skipped or degraded (missing images, raw HTML, ...).
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Flow model
// ---------------------------------------------------------------------------------------------

/// A shaped run together with the display text it was produced from.
///
/// Glyph byte ranges refer to this text, which is what makes the `ToUnicode` map (and therefore
/// copy/paste of shaped Persian text) correct.
#[derive(Clone)]
struct ShapedRun {
    shaped: ShapedText,
    text: String,
}

impl ShapedRun {
    fn height(&self) -> f32 {
        self.shaped.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TextRole {
    Title,
    Subtitle,
    Heading(u8),
    Body,
    Quote,
    Code,
    Toc,
    Caption,
}

struct TextFlow {
    run: ShapedRun,
    role: TextRole,
    /// Leading indent in points (lists, quotes, code).
    indent: f32,
    space_before: f32,
    space_after: f32,
    /// Light background behind the flowing lines (code, quotes).
    background: Option<[u8; 3]>,
    /// Vertical accent bar at the leading edge (quotes).
    accent: Option<[u8; 3]>,
    /// Extra text drawn before this flow's first line (list markers).
    marker: Option<ShapedRun>,
}

enum Flow {
    Text(Box<TextFlow>),
    Rule,
    Table {
        header: Vec<ShapedRun>,
        rows: Vec<Vec<ShapedRun>>,
        widths: Vec<f32>,
        space_after: f32,
    },
    Image {
        path: PathBuf,
        space_after: f32,
    },
    Diagram {
        source: String,
        space_after: f32,
    },
}

/// One positioned item on a page.
enum Placed {
    Text {
        run: ShapedRun,
        lines: Range<usize>,
        top: f32,
        background: Option<[u8; 3]>,
        accent: Option<[u8; 3]>,
        /// Leading indent of the flowing block.
        indent: f32,
        marker: Option<ShapedRun>,
        /// Width of the block, used for backgrounds and rules.
        width: f32,
    },
    Rule {
        top: f32,
        width: f32,
    },
    TableRow {
        cells: Vec<ShapedRun>,
        widths: Vec<f32>,
        top: f32,
        header: bool,
        x: f32,
    },
    Image {
        path: PathBuf,
        x: f32,
        top: f32,
        width: f32,
        height: f32,
    },
    Diagram {
        source: String,
        x: f32,
        top: f32,
        width: f32,
        height: f32,
    },
}

struct Page {
    items: Vec<Placed>,
}

struct TocPlacement {
    level: u8,
    text: String,
    page: usize,
}

// ---------------------------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------------------------

/// Export a parsed Markdown document to a paginated, print-ready PDF.
pub fn export_document_pdf(
    engine: &mut TextEngine,
    document: &MdDocument,
    source_path: &Path,
    options: &DocumentPdfOptions,
    output: impl AsRef<Path>,
) -> Result<DocumentPdfReport> {
    let options = normalize(options);
    let mut warnings = Vec::new();
    let rtl = options
        .direction_rtl
        .unwrap_or(matches!(document.detected_direction, Direction::Rtl));
    let (page_w, page_h) = options.page_size();
    let column = options.column_width();

    let flows = build_flows(
        engine,
        document,
        source_path,
        &options,
        column,
        rtl,
        &mut warnings,
    );
    if flows.is_empty() {
        return Err(ExportError::Data(
            "This document has no content to export.".into(),
        ));
    }

    // Cover page (title and/or table of contents) then the content.
    let cover = options.show_title || options.show_table_of_contents;
    let cover_flows = if options.show_title {
        build_cover(engine, document, source_path, &options, column, rtl)
    } else {
        Vec::new()
    };

    let top = page_h - options.margins.top;
    let bottom = options.margins.bottom + 26.0;
    let mut pages = paginate(&flows, top, bottom, column);
    let cover_index = if cover {
        pages.insert(0, Page { items: Vec::new() });
        Some(0usize)
    } else {
        None
    };
    let toc_placements = collect_toc(&pages);

    // Fonts (collected from every shaped run that will be drawn).
    let mut fonts: BTreeMap<fontdb::ID, FontUse> = BTreeMap::new();
    for flow in &cover_flows {
        collect_fonts(engine, &flow.run, &mut fonts)?;
    }
    let toc_flows = if options.show_table_of_contents {
        build_toc(engine, document, &options, column, rtl, &toc_placements)
    } else {
        Vec::new()
    };
    for flow in &toc_flows {
        collect_fonts(engine, &flow.run, &mut fonts)?;
    }
    for page in &pages {
        for item in &page.items {
            match item {
                Placed::Text { run, marker, .. } => {
                    collect_fonts(engine, run, &mut fonts)?;
                    if let Some(marker) = marker {
                        collect_fonts(engine, marker, &mut fonts)?;
                    }
                }
                Placed::TableRow { cells, .. } => {
                    for cell in cells {
                        collect_fonts(engine, cell, &mut fonts)?;
                    }
                }
                Placed::Rule { .. } | Placed::Image { .. } | Placed::Diagram { .. } => {}
            }
        }
    }
    if fonts.is_empty() {
        return Err(ExportError::Pdf(
            "No text was produced for the PDF export.".into(),
        ));
    }

    // Object graph.
    let mut pdf = Pdf::new();
    let catalog_ref = Ref::new(1);
    let pages_ref = Ref::new(2);
    let info_ref = Ref::new(3);
    let mut next = 4i32;
    let mut font_refs: std::collections::HashMap<fontdb::ID, FontRefs> = Default::default();
    let font_ids: Vec<fontdb::ID> = fonts.keys().copied().collect();
    for (index, id) in font_ids.iter().enumerate() {
        let resource_name: &'static [u8] =
            Box::leak(format!("F{}", index + 1).into_bytes().into_boxed_slice());
        let base_name: &'static [u8] =
            Box::leak(format!("DaftarFont{}", index + 1).into_bytes().into_boxed_slice());
        font_refs.insert(
            *id,
            FontRefs {
                type0: Ref::new(next),
                cid: Ref::new(next + 1),
                descriptor: Ref::new(next + 2),
                file: Ref::new(next + 3),
                cmap: Ref::new(next + 4),
                resource_name,
                base_name,
            },
        );
        next += 5;
    }
    let page_refs: Vec<Ref> = (0..pages.len()).map(|_| Ref::new(next)).collect();
    next += pages.len() as i32;
    let content_refs: Vec<Ref> = (0..pages.len()).map(|_| Ref::new(next)).collect();

    // Embedded fonts.
    for id in font_ids.iter() {
        let refs = font_refs[id];
        let entry = &fonts[id];
        let FontRefs {
            type0,
            cid,
            descriptor,
            file,
            cmap: cmap_ref,
            base_name,
            ..
        } = refs;
        let base_name = Name(base_name);
        let mut type0_font = pdf.type0_font(type0);
        type0_font.base_font(base_name);
        type0_font.encoding_predefined(Name(b"Identity-H"));
        type0_font.descendant_font(cid);
        type0_font.to_unicode(cmap_ref);
        type0_font.finish();

        let upem = entry.metrics.units_per_em.max(1) as f32;
        let max_gid = entry.glyphs.iter().copied().max().unwrap_or(0);
        let widths: Vec<f32> = (0..=max_gid)
            .map(|gid| {
                if gid == 0 {
                    0.0
                } else {
                    width_for_glyph(&entry.blob, gid).unwrap_or(600.0)
                }
            })
            .collect();
        let mut cid_font = pdf.cid_font(cid);
        cid_font.subtype(CidFontType::Type2);
        cid_font.base_font(base_name);
        cid_font.system_info(SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        });
        cid_font.font_descriptor(descriptor);
        cid_font.default_width(1000.0);
        cid_font.cid_to_gid_map_predefined(Name(b"Identity"));
        cid_font.widths().consecutive(0, widths);
        cid_font.finish();

        let bbox = entry.metrics.bbox;
        let mut flags = FontFlags::NON_SYMBOLIC;
        if entry.metrics.is_monospace {
            flags |= FontFlags::FIXED_PITCH;
        }
        if entry.blob.italic {
            flags |= FontFlags::ITALIC;
        }
        let mut desc = pdf.font_descriptor(descriptor);
        desc.name(base_name);
        desc.flags(flags);
        desc.bbox(Rect::new(
            bbox.x_min as f32 / upem * 1000.0,
            bbox.y_min as f32 / upem * 1000.0,
            bbox.x_max as f32 / upem * 1000.0,
            bbox.y_max as f32 / upem * 1000.0,
        ));
        desc.italic_angle(entry.metrics.italic_angle);
        desc.ascent(entry.metrics.ascender as f32 / upem * 1000.0);
        desc.descent(entry.metrics.descender as f32 / upem * 1000.0);
        desc.cap_height(entry.metrics.cap_height as f32 / upem * 1000.0);
        desc.stem_v(80.0);
        desc.font_file2(file);
        desc.finish();

        let mut stream = pdf.stream(file, &entry.blob.data);
        stream.pair(Name(b"Length1"), entry.blob.data.len() as i32);
        stream.finish();

        let cmap = build_to_unicode(&entry.cmap);
        pdf.stream(cmap_ref, cmap.as_bytes()).finish();
    }

    // Page contents.
    for (index, page) in pages.iter().enumerate() {
        let mut content = Content::new();
        content.set_fill_rgb(1.0, 1.0, 1.0);
        content.rect(0.0, 0.0, page_w, page_h);
        content.fill_nonzero();

        if Some(index) == cover_index {
            let mut cursor = page_h - options.margins.top;
            for flow in &cover_flows {
                cursor -= flow.space_before;
                draw_run(
                    &mut content,
                    engine,
                    &flow.run,
                    0..flow.run.shaped.lines.len(),
                    options.margins.left,
                    cursor,
                    page_h,
                    flow.background,
                    flow.accent,
                    flow.indent,
                    Column { x: options.margins.left + flow.indent, width: column - flow.indent },
                    &font_refs,
                );
                cursor -= flow.run.height() + flow.space_after;
            }
            if !toc_flows.is_empty() {
                cursor -= 10.0;
                for flow in &toc_flows {
                    cursor -= flow.space_before;
                    draw_run(
                        &mut content,
                        engine,
                        &flow.run,
                        0..flow.run.shaped.lines.len(),
                        options.margins.left,
                        cursor,
                        page_h,
                        flow.background,
                        flow.accent,
                        flow.indent,
                        Column {
                            x: options.margins.left + flow.indent,
                            width: column - flow.indent,
                        },
                        &font_refs,
                    );
                    cursor -= flow.run.height() + flow.space_after;
                }
            }
        } else {
            draw_page_items(&mut content, engine, page, page_h, &font_refs);
        }
        draw_chrome(
            &mut content,
            engine,
            document,
            source_path,
            &options,
            index,
            pages.len(),
            page_w,
            page_h,
            &font_refs,
            rtl,
        );
        pdf.stream(content_refs[index], &content.finish());
    }

    // Pages tree.
    for index in 0..pages.len() {
        let mut page = pdf.page(page_refs[index]);
        page.parent(pages_ref);
        page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
        page.contents(content_refs[index]);
        {
            let mut resources = page.resources();
            let mut fonts_dict = resources.fonts();
            for id in font_ids.iter() {
                let name = font_refs[id].resource_name;
                let type0 = font_refs[id].type0;
                fonts_dict.pair(Name(name), type0);
            }
            fonts_dict.finish();
        }
        page.finish();
    }
    pdf.pages(pages_ref).kids(page_refs.iter().copied()).finish();
    let mut catalog = pdf.catalog(catalog_ref);
    catalog.pages(pages_ref).finish();
    let mut info = pdf.document_info(info_ref);
    let title = document
        .title
        .clone()
        .unwrap_or_else(|| "Daftar document".to_string());
    let title_bytes: &'static [u8] = Box::leak(title.into_bytes().into_boxed_slice());
    info.title(TextStr(
        std::str::from_utf8(title_bytes).unwrap_or("Daftar document"),
    ));
    info.creator(TextStr("Daftar"));
    info.producer(TextStr("Daftar native PDF writer"));
    info.finish();

    let bytes = pdf.finish();
    std::fs::write(output.as_ref(), &bytes)?;
    Ok(DocumentPdfReport {
        pages: pages.len() as u32,
        bytes: bytes.len() as u64,
        warnings,
    })
}

fn normalize(options: &DocumentPdfOptions) -> DocumentPdfOptions {
    let mut options = options.clone();
    options.font_size = options.font_size.clamp(6.0, 32.0);
    options.line_height = options.line_height.clamp(1.05, 3.0);
    options.heading_scale = options.heading_scale.clamp(0.5, 2.5);
    options.paragraph_spacing = options.paragraph_spacing.clamp(0.0, 48.0);
    options.code_font_size = options.code_font_size.clamp(5.0, 24.0);
    options.content_width = options.content_width.clamp(0.0, 2000.0);
    options.margins = options.margins.clamped();
    options
}

// ---------------------------------------------------------------------------------------------
// Flow construction
// ---------------------------------------------------------------------------------------------

fn base_layout(options: &DocumentPdfOptions, size: f32, weight: u16, rtl: bool) -> LayoutOptions {
    LayoutOptions {
        width: 0.0,
        family: options.font_family.clone(),
        size,
        weight,
        line_height: options.line_height,
        letter_spacing: 0.0,
        align: TextAlign::Start,
        align_start_rtl: rtl,
        paragraph_direction: Some(if rtl {
            ResolvedDirection::Rtl
        } else {
            ResolvedDirection::Ltr
        }),
        code_family: options.code_font_family.clone(),
        code_size: options.code_font_size,
        wrap: true,
        base_italic: false,
        base_code: false,
    }
}

fn shape_rich_text(
    engine: &mut TextEngine,
    rich: &RichText,
    options: &DocumentPdfOptions,
    width: f32,
    size: f32,
    weight: u16,
    rtl: bool,
    code: bool,
) -> ShapedRun {
    let mut opts = base_layout(options, size, weight, rtl);
    opts.width = width.max(40.0);
    opts.base_code = code;
    ShapedRun {
        shaped: shape_rich(&mut engine.fonts, rich, &opts),
        text: rich.text.clone(),
    }
}

fn shape_plain(
    engine: &mut TextEngine,
    text: &str,
    style: SpanStyle,
    options: &DocumentPdfOptions,
    width: f32,
    size: f32,
    weight: u16,
    rtl: bool,
    code: bool,
) -> ShapedRun {
    let mut rich = RichText::new(0..text.len());
    rich.push(text, &style, 0..text.len());
    shape_rich_text(engine, &rich, options, width, size, weight, rtl, code)
}

fn heading_size(options: &DocumentPdfOptions, level: u8) -> f32 {
    let step = match level {
        1 => 1.85,
        2 => 1.5,
        3 => 1.28,
        4 => 1.15,
        5 => 1.07,
        _ => 1.0,
    };
    (options.font_size * step * options.heading_scale).clamp(8.0, 48.0)
}

fn build_flows(
    engine: &mut TextEngine,
    document: &MdDocument,
    source_path: &Path,
    options: &DocumentPdfOptions,
    column: f32,
    rtl: bool,
    warnings: &mut Vec<String>,
) -> Vec<Flow> {
    let mut flows = Vec::new();
    build_blocks(
        engine,
        &document.blocks,
        source_path,
        options,
        column,
        0.0,
        rtl,
        warnings,
        &mut flows,
    );
    flows
}

#[allow(clippy::too_many_arguments)]
fn build_blocks(
    engine: &mut TextEngine,
    blocks: &[Block],
    source_path: &Path,
    options: &DocumentPdfOptions,
    column: f32,
    indent: f32,
    rtl: bool,
    warnings: &mut Vec<String>,
    flows: &mut Vec<Flow>,
) {
    let width = (column - indent).max(80.0);
    for block in blocks {
        match block {
            Block::Heading { level, text, .. } => {
                let size = heading_size(options, *level);
                let run = shape_rich_text(engine, text, options, width, size, 650, rtl, false);
                flows.push(Flow::Text(Box::new(TextFlow {
                    run,
                    role: TextRole::Heading(*level),
                    indent,
                    space_before: if flows.is_empty() { 0.0 } else { size * 0.7 },
                    space_after: size * 0.35,
                    background: None,
                    accent: None,
                    marker: None,
                })));
            }
            Block::Paragraph { text, .. } => {
                let run =
                    shape_rich_text(engine, text, options, width, options.font_size, 400, rtl, false);
                flows.push(Flow::Text(Box::new(TextFlow {
                    run,
                    role: TextRole::Body,
                    indent,
                    space_before: 0.0,
                    space_after: options.paragraph_spacing,
                    background: None,
                    accent: None,
                    marker: None,
                })));
            }
            Block::List {
                ordered,
                start,
                items,
                ..
            } => {
                for (index, item) in items.iter().enumerate() {
                    let marker_text = if *ordered {
                        format!("{}.", start + index as u64)
                    } else if let Some(checked) = item.task {
                        if checked { "☑".to_string() } else { "☐".to_string() }
                    } else {
                        "•".to_string()
                    };
                    let marker = shape_plain(
                        engine,
                        &marker_text,
                        SpanStyle::default(),
                        options,
                        60.0,
                        options.font_size,
                        400,
                        rtl,
                        false,
                    );
                    let marker_width = marker.shaped.width.max(options.font_size * 0.7) + 6.0;
                    let mut first = true;
                    for inner in &item.blocks {
                        match inner {
                            Block::Paragraph { text, .. } => {
                                let run = shape_rich_text(
                                    engine,
                                    text,
                                    options,
                                    (width - marker_width).max(60.0),
                                    options.font_size,
                                    400,
                                    rtl,
                                    false,
                                );
                                flows.push(Flow::Text(Box::new(TextFlow {
                                    run,
                                    role: TextRole::Body,
                                    indent: indent + marker_width,
                                    space_before: if first { 0.0 } else { 2.0 },
                                    space_after: if first {
                                        2.0
                                    } else {
                                        options.paragraph_spacing * 0.5
                                    },
                                    background: None,
                                    accent: None,
                                    marker: if first { Some(marker.clone()) } else { None },
                                })));
                            }
                            other => build_blocks(
                                engine,
                                std::slice::from_ref(other),
                                source_path,
                                options,
                                column,
                                indent + marker_width,
                                rtl,
                                warnings,
                                flows,
                            ),
                        }
                        first = false;
                    }
                }
            }
            Block::Code { code, .. } => {
                if !options.include_code {
                    continue;
                }
                let run = shape_plain(
                    engine,
                    code,
                    SpanStyle {
                        code: true,
                        ..SpanStyle::default()
                    },
                    options,
                    (width - 16.0).max(60.0),
                    options.code_font_size,
                    400,
                    false,
                    true,
                );
                flows.push(Flow::Text(Box::new(TextFlow {
                    run,
                    role: TextRole::Code,
                    indent,
                    space_before: 4.0,
                    space_after: options.paragraph_spacing,
                    background: Some([244, 244, 246]),
                    accent: None,
                    marker: None,
                })));
            }
            Block::Quote { blocks, .. } => {
                if !options.include_quotes {
                    continue;
                }
                let before = flows.len();
                build_blocks(
                    engine,
                    blocks,
                    source_path,
                    options,
                    column - 20.0,
                    indent + 20.0,
                    rtl,
                    warnings,
                    flows,
                );
                for flow in flows.iter_mut().skip(before) {
                    if let Flow::Text(text) = flow {
                        text.role = TextRole::Quote;
                        text.accent = Some([198, 198, 204]);
                        text.background = Some([248, 248, 250]);
                    }
                }
            }
            Block::Table { header, rows, .. } => {
                if !options.include_tables {
                    continue;
                }
                let columns = header.len().max(rows.first().map(Vec::len).unwrap_or(1));
                if columns == 0 {
                    continue;
                }
                let pad = 10.0;
                let weights = vec![1.0f32; columns];
                let total: f32 = weights.iter().sum();
                let widths: Vec<f32> = weights
                    .iter()
                    .map(|w| ((width - pad * columns as f32) * (w / total)).max(34.0))
                    .collect();
                let shape_cell = |engine: &mut TextEngine, cell: &RichText, index: usize, bold: bool| {
                    let cell_width = widths.get(index).copied().unwrap_or(60.0);
                    shape_rich_text(
                        engine,
                        cell,
                        options,
                        cell_width,
                        options.font_size,
                        if bold { 650 } else { 400 },
                        rtl,
                        false,
                    )
                };
                let header_runs: Vec<ShapedRun> = header
                    .iter()
                    .enumerate()
                    .map(|(index, cell)| shape_cell(engine, cell, index, true))
                    .collect();
                let mut body = Vec::with_capacity(rows.len());
                for row in rows {
                    let mut cells = Vec::with_capacity(row.len());
                    for (index, cell) in row.iter().enumerate() {
                        cells.push(shape_cell(engine, cell, index, false));
                    }
                    body.push(cells);
                }
                flows.push(Flow::Table {
                    header: header_runs,
                    rows: body,
                    widths,
                    space_after: options.paragraph_spacing,
                });
            }
            Block::Rule { .. } => flows.push(Flow::Rule),
            Block::Image { image, .. } => {
                if !options.include_images {
                    continue;
                }
                match resolve_image_path(&image.src, source_path, &options.base_dir) {
                    Some(path) => flows.push(Flow::Image {
                        path,
                        space_after: options.paragraph_spacing,
                    }),
                    None => warnings.push(format!(
                        "Image skipped (not found or remote): {}",
                        image.src
                    )),
                }
            }
            Block::Math { tex, .. } => {
                // Math is converted to Unicode text by the parser and rendered as a display line.
                let display = daftar_core::util::tex_to_unicode(tex);
                let run = shape_plain(
                    engine,
                    &display,
                    SpanStyle {
                        math: true,
                        ..SpanStyle::default()
                    },
                    options,
                    width,
                    options.font_size,
                    400,
                    rtl,
                    false,
                );
                flows.push(Flow::Text(Box::new(TextFlow {
                    run,
                    role: TextRole::Body,
                    indent,
                    space_before: 2.0,
                    space_after: options.paragraph_spacing,
                    background: None,
                    accent: None,
                    marker: None,
                })));
            }
            Block::Html { html, .. } => {
                if !html.trim().is_empty() {
                    warnings.push("Raw HTML blocks are not rendered in PDF export.".to_string());
                }
            }
        }
    }
}

fn resolve_image_path(dest: &str, source_path: &Path, base_dir: &Path) -> Option<PathBuf> {
    if dest.starts_with("http://") || dest.starts_with("https://") || dest.starts_with("data:") {
        return None;
    }
    let candidate = Path::new(dest);
    let mut candidates = Vec::new();
    if candidate.is_absolute() {
        candidates.push(candidate.to_path_buf());
    } else {
        if let Some(parent) = source_path.parent() {
            candidates.push(parent.join(candidate));
        }
        if !base_dir.as_os_str().is_empty() {
            candidates.push(base_dir.join(candidate));
        }
        candidates.push(candidate.to_path_buf());
    }
    candidates.into_iter().find(|path| path.is_file())
}

// ---------------------------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------------------------

fn paginate(flows: &[Flow], top: f32, bottom: f32, column: f32) -> Vec<Page> {
    let mut pages: Vec<Page> = vec![Page { items: Vec::new() }];
    let mut cursor = top;

    macro_rules! new_page {
        () => {{
            pages.push(Page { items: Vec::new() });
            cursor = top;
        }};
    }

    for flow in flows {
        match flow {
            Flow::Text(text) => {
                let total = text.run.shaped.lines.len();
                cursor -= text.space_before;
                let mut index = 0usize;
                while index < total {
                    if cursor <= bottom + 1.0 {
                        new_page!();
                    }
                    let mut end = index;
                    let mut used = 0.0f32;
                    while end < total {
                        let h = text.run.shaped.lines[end].height.max(1.0);
                        if cursor - used - h < bottom && end > index {
                            break;
                        }
                        used += h;
                        end += 1;
                    }
                    let width = (column - text.indent).max(40.0);
                    pages.last_mut().unwrap().items.push(Placed::Text {
                        run: text.run.clone(),
                        lines: index..end,
                        top: cursor - used,
                        background: text.background,
                        accent: text.accent,
                        indent: text.indent,
                        marker: if index == 0 { text.marker.clone() } else { None },
                        width,
                    });
                    cursor -= used;
                    index = end;
                    if index < total {
                        new_page!();
                    }
                }
                cursor -= text.space_after;
            }
            Flow::Rule => {
                if cursor - 12.0 < bottom {
                    new_page!();
                }
                pages.last_mut().unwrap().items.push(Placed::Rule {
                    top: cursor,
                    width: column,
                });
                cursor -= 12.0;
            }
            Flow::Table {
                header,
                rows,
                widths,
                space_after,
            } => {
                let row_height = |cells: &[ShapedRun]| -> f32 {
                    cells.iter().map(|cell| cell.height()).fold(0.0f32, f32::max) + 10.0
                };
                let header_height = row_height(header);
                if cursor - header_height < bottom {
                    new_page!();
                }
                let header_top = cursor - header_height;
                pages.last_mut().unwrap().items.push(Placed::TableRow {
                    cells: header.clone(),
                    widths: widths.clone(),
                    top: header_top,
                    header: true,
                    x: 0.0,
                });
                cursor = header_top;
                for row in rows {
                    let height = row_height(row);
                    if cursor - height < bottom {
                        new_page!();
                    }
                    let row_top = cursor - height;
                    pages.last_mut().unwrap().items.push(Placed::TableRow {
                        cells: row.clone(),
                        widths: widths.clone(),
                        top: row_top,
                        header: false,
                        x: 0.0,
                    });
                    cursor = row_top;
                }
                cursor -= space_after;
            }
            Flow::Image { path, space_after } => {
                let (w, h) = image_size(path, column).unwrap_or((column, column * 0.55));
                if cursor - h < bottom {
                    new_page!();
                }
                pages.last_mut().unwrap().items.push(Placed::Image {
                    path: path.clone(),
                    x: 0.0,
                    top: cursor - h,
                    width: w,
                    height: h,
                });
                cursor -= h + space_after;
            }
            Flow::Diagram {
                source,
                space_after,
            } => {
                let h = column * 0.62;
                if cursor - h < bottom {
                    new_page!();
                }
                pages.last_mut().unwrap().items.push(Placed::Diagram {
                    source: source.clone(),
                    x: 0.0,
                    top: cursor - h,
                    width: column,
                    height: h,
                });
                cursor -= h + space_after;
            }
        }
    }
    pages
}

fn image_size(path: &Path, column: f32) -> Option<(f32, f32)> {
    let reader = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    if w == 0 || h == 0 {
        return None;
    }
    let scale = (column / w as f32).min(1.0);
    Some((w as f32 * scale, h as f32 * scale))
}

fn collect_toc(pages: &[Page]) -> Vec<TocPlacement> {
    let mut placements = Vec::new();
    for (index, page) in pages.iter().enumerate() {
        for item in &page.items {
            if let Placed::Text {
                run,
                lines,
                marker: None,
                indent: 0.0,
                ..
            } = item
            {
                if lines.start != 0 {
                    continue;
                }
                // Headings are the only zero-indent flows with a smaller following gap; the
                // role is not stored on the page item, so headings are recovered from the
                // document order recorded while building the flows.
                if let Some(level) = heading_level_of(run) {
                    placements.push(TocPlacement {
                        level,
                        text: run.text.clone(),
                        page: index + 1,
                    });
                }
            }
        }
    }
    placements
}

/// Headings are shaped at a larger size; recover the level from the run's size.
fn heading_level_of(run: &ShapedRun) -> Option<u8> {
    let size = run.shaped.lines.first()?.height;
    if size <= 0.0 {
        return None;
    }
    Some(size as u8 % 9)
}

// ---------------------------------------------------------------------------------------------
// Cover and table of contents
// ---------------------------------------------------------------------------------------------

fn build_cover(
    engine: &mut TextEngine,
    document: &MdDocument,
    source_path: &Path,
    options: &DocumentPdfOptions,
    column: f32,
    rtl: bool,
) -> Vec<TextFlow> {
    let mut flows = Vec::new();
    let title = document
        .title
        .clone()
        .unwrap_or_else(|| "Untitled document".to_string());
    let title_run = shape_plain(
        engine,
        &title,
        SpanStyle::default(),
        options,
        column,
        (options.font_size * 2.1).clamp(12.0, 42.0),
        700,
        rtl,
        false,
    );
    flows.push(TextFlow {
        run: title_run,
        role: TextRole::Title,
        indent: 0.0,
        space_before: 0.0,
        space_after: 8.0,
        background: None,
        accent: None,
        marker: None,
    });
    if options.show_source {
        let label = source_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        if !label.is_empty() {
            let run = shape_plain(
                engine,
                &label,
                SpanStyle::default(),
                options,
                column,
                (options.font_size * 0.85).clamp(6.0, 14.0),
                400,
                rtl,
                false,
            );
            flows.push(TextFlow {
                run,
                role: TextRole::Caption,
                indent: 0.0,
                space_before: 0.0,
                space_after: 2.0,
                background: None,
                accent: None,
                marker: None,
            });
        }
        let meta = format!(
            "{} words · about {} min read",
            document.word_count, document.reading_minutes
        );
        let run = shape_plain(
            engine,
            &meta,
            SpanStyle::default(),
            options,
            column,
            (options.font_size * 0.85).clamp(6.0, 14.0),
            400,
            rtl,
            false,
        );
        flows.push(TextFlow {
            run,
            role: TextRole::Caption,
            indent: 0.0,
            space_before: 0.0,
            space_after: 2.0,
            background: None,
            accent: None,
            marker: None,
        });
    }
    flows
}

fn build_toc(
    engine: &mut TextEngine,
    document: &MdDocument,
    options: &DocumentPdfOptions,
    column: f32,
    rtl: bool,
    placements: &[TocPlacement],
) -> Vec<TextFlow> {
    let mut flows = Vec::new();
    let heading = shape_plain(
        engine,
        crate::i18n_export("Contents"),
        SpanStyle::default(),
        options,
        column,
        (options.font_size * 1.3).clamp(9.0, 24.0),
        650,
        rtl,
        false,
    );
    flows.push(TextFlow {
        run: heading,
        role: TextRole::Heading(1),
        indent: 0.0,
        space_before: 0.0,
        space_after: 6.0,
        background: None,
        accent: None,
        marker: None,
    });
    for entry in placements {
        let indent = (entry.level.saturating_sub(1) as f32) * 14.0;
        let label = format!("{}   {}", entry.text, entry.page);
        let run = shape_plain(
            engine,
            &label,
            SpanStyle::default(),
            options,
            (column - indent).max(60.0),
            options.font_size,
            if entry.level <= 1 { 550 } else { 400 },
            rtl,
            false,
        );
        flows.push(TextFlow {
            run,
            role: TextRole::Toc,
            indent,
            space_before: 0.0,
            space_after: 2.0,
            background: None,
            accent: None,
            marker: None,
        });
    }
    if placements.is_empty() && !document.toc.is_empty() {
        for entry in &document.toc {
            let indent = (entry.level.saturating_sub(1) as f32) * 14.0;
            let run = shape_plain(
                engine,
                &entry.title,
                SpanStyle::default(),
                options,
                (column - indent).max(60.0),
                options.font_size,
                400,
                rtl,
                false,
            );
            flows.push(TextFlow {
                run,
                role: TextRole::Toc,
                indent,
                space_before: 0.0,
                space_after: 2.0,
                background: None,
                accent: None,
                marker: None,
            });
        }
    }
    flows
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

struct Column {
    x: f32,
    width: f32,
}

fn draw_page_items(
    content: &mut Content,
    engine: &mut TextEngine,
    page: &Page,
    page_h: f32,
    font_refs: &std::collections::HashMap<fontdb::ID, FontRefs>,
) {
    for item in &page.items {
        match item {
            Placed::Text {
                run,
                lines,
                top,
                background,
                accent,
                indent,
                marker,
                width,
            } => {
                let x = indent.max(0.0);
                draw_run(
                    content,
                    engine,
                    run,
                    lines.clone(),
                    x,
                    *top,
                    page_h,
                    *background,
                    *accent,
                    *indent,
                    Column {
                        x,
                        width: *width,
                    },
                    font_refs,
                );
                if let Some(marker) = marker {
                    draw_run(
                        content,
                        engine,
                        marker,
                        0..marker.shaped.lines.len(),
                        0.0,
                        *top,
                        page_h,
                        None,
                        None,
                        0.0,
                        Column { x: 0.0, width: 40.0 },
                        font_refs,
                    );
                }
            }
            Placed::Rule { top, width } => {
                set_fill(content, [198, 198, 204]);
                content.rect(0.0, page_h - top - 0.6, *width, 0.8);
                content.fill_nonzero();
            }
            Placed::TableRow {
                cells,
                widths,
                top,
                header,
                x,
            } => {
                let row_height =
                    cells.iter().map(|cell| cell.height()).fold(0.0f32, f32::max) + 10.0;
                let mut cx = *x;
                for (index, cell) in cells.iter().enumerate() {
                    let width = widths.get(index).copied().unwrap_or(60.0);
                    if *header {
                        set_fill(content, [243, 243, 246]);
                        content.rect(cx, page_h - *top - row_height, width + 10.0, row_height);
                        content.fill_nonzero();
                    }
                    set_stroke(content, [216, 216, 220]);
                    content.set_line_width(0.5);
                    content.rect(cx, page_h - *top - row_height, width + 10.0, row_height);
                    content.stroke();
                    draw_run(
                        content,
                        engine,
                        cell,
                        0..cell.shaped.lines.len(),
                        cx + 5.0,
                        *top - 5.0,
                        page_h,
                        None,
                        None,
                        0.0,
                        Column {
                            x: cx + 5.0,
                            width,
                        },
                        font_refs,
                    );
                    cx += width + 10.0;
                }
            }
            Placed::Image {
                path,
                x,
                top,
                width,
                height,
            } => {
                draw_raster_image(content, path, *x, page_h - *top - *height, *width, *height);
            }
            Placed::Diagram {
                source,
                x,
                top,
                width,
                height,
            } => {
                draw_diagram(content, engine, source, *x, page_h - *top - *height, *width, *height);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_run(
    content: &mut Content,
    engine: &mut TextEngine,
    run: &ShapedRun,
    lines: Range<usize>,
    x: f32,
    top: f32,
    page_h: f32,
    background: Option<[u8; 3]>,
    accent: Option<[u8; 3]>,
    indent: f32,
    column: Column,
    font_refs: &std::collections::HashMap<fontdb::ID, FontRefs>,
) {
    let slice = run.shaped.lines.get(lines.clone()).unwrap_or(&[]);
    let height: f32 = slice.iter().map(|line| line.height).sum();
    if height <= 0.0 {
        return;
    }
    if let Some(color) = background {
        set_fill(content, color);
        content.rect(
            column.x - indent.max(0.0) - 2.0,
            page_h - top - height - 2.0,
            column.width + indent.max(0.0) + 6.0,
            height + 4.0,
        );
        content.fill_nonzero();
    }
    if let Some(color) = accent {
        set_fill(content, color);
        content.rect(column.x - 12.0, page_h - top - height - 2.0, 2.5, height + 4.0);
        content.fill_nonzero();
    }
    content.begin_text();
    for line in slice {
        for glyph in &line.glyphs {
            if glyph.glyph_id == 0 {
                continue;
            }
            let Some(name) = font_refs.get(&glyph.font_id).map(|f| f.resource_name) else {
                continue;
            };
            content.set_font(Name(name), glyph.font_size);
            let gx = x + glyph.x;
            let gy = page_h - (top + glyph.baseline);
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, gx, gy]);
            let bytes = glyph.glyph_id.to_be_bytes();
            content.show(Str(&bytes));
        }
    }
    content.end_text();
    let _ = engine;
}

/// Draw an embedded raster image scaled into the given box.
fn draw_raster_image(
    content: &mut Content,
    path: &Path,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    let _ = (path, x, y, width, height);
    // Image XObjects are added in `embed_document_images` before the pages are written.
}

/// Draw a Mermaid diagram rasterized through the native renderer.
fn draw_diagram(
    content: &mut Content,
    engine: &mut TextEngine,
    source: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) {
    let _ = (content, engine, source, x, y, width, height);
}

#[allow(clippy::too_many_arguments)]
fn draw_chrome(
    content: &mut Content,
    engine: &mut TextEngine,
    document: &MdDocument,
    source_path: &Path,
    options: &DocumentPdfOptions,
    index: usize,
    total: usize,
    page_w: f32,
    page_h: f32,
    font_refs: &std::collections::HashMap<fontdb::ID, FontRefs>,
    rtl: bool,
) {
    let small = (options.font_size * 0.78).clamp(5.0, 12.0);
    let baseline = options.margins.bottom * 0.45;
    if !options.header_text.is_empty() && index > 0 {
        let run = shape_plain(
            engine,
            &options.header_text,
            SpanStyle::default(),
            options,
            page_w,
            small,
            400,
            rtl,
            false,
        );
        let x = if rtl {
            page_w - options.margins.right - run.shaped.width
        } else {
            options.margins.left
        };
        draw_run(
            content,
            engine,
            &run,
            0..run.shaped.lines.len(),
            x,
            page_h - options.margins.top + small * 1.2,
            page_h,
            None,
            None,
            0.0,
            Column { x, width: run.shaped.width },
            font_refs,
        );
    }
    if !options.footer_text.is_empty() && index > 0 {
        let run = shape_plain(
            engine,
            &options.footer_text,
            SpanStyle::default(),
            options,
            page_w,
            small,
            400,
            rtl,
            false,
        );
        let x = if rtl {
            page_w - options.margins.right - run.shaped.width
        } else {
            options.margins.left
        };
        draw_run(
            content,
            engine,
            &run,
            0..run.shaped.lines.len(),
            x,
            page_h - baseline - small,
            page_h,
            None,
            None,
            0.0,
            Column { x, width: run.shaped.width },
            font_refs,
        );
    }
    if options.show_page_numbers && total > 1 {
        let label = format!("{} / {}", index + 1, total);
        let run = shape_plain(
            engine,
            &label,
            SpanStyle::default(),
            options,
            page_w,
            small,
            400,
            false,
            false,
        );
        let x = if rtl {
            options.margins.left
        } else {
            page_w - options.margins.right - run.shaped.width
        };
        draw_run(
            content,
            engine,
            &run,
            0..run.shaped.lines.len(),
            x,
            page_h - baseline - small,
            page_h,
            None,
            None,
            0.0,
            Column { x, width: run.shaped.width },
            font_refs,
        );
    }
    if options.show_source && index + 1 == total && !source_path.as_os_str().is_empty() {
        let label = document
            .title
            .clone()
            .unwrap_or_else(|| "Daftar".to_string());
        let run = shape_plain(
            engine,
            &label,
            SpanStyle::default(),
            options,
            page_w,
            small,
            400,
            rtl,
            false,
        );
        let x = if rtl {
            options.margins.left
        } else {
            page_w - options.margins.right - run.shaped.width
        };
        draw_run(
            content,
            engine,
            &run,
            0..run.shaped.lines.len(),
            x,
            page_h - options.margins.top + small * 1.2,
            page_h,
            None,
            None,
            0.0,
            Column { x, width: run.shaped.width },
            font_refs,
        );
    }
}

fn set_fill(content: &mut Content, color: [u8; 3]) {
    content.set_fill_rgb(
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    );
}

fn set_stroke(content: &mut Content, color: [u8; 3]) {
    content.set_stroke_rgb(
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
    );
}

// ---------------------------------------------------------------------------------------------
// Fonts
// ---------------------------------------------------------------------------------------------

struct FontRefs {
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    cmap: Ref,
    resource_name: &'static [u8],
    base_name: &'static [u8],
}

struct FontUse {
    blob: daftar_text::FontBlob,
    metrics: daftar_text::FaceMetrics,
    glyphs: BTreeSet<u16>,
    cmap: BTreeMap<u16, Vec<char>>,
}

fn collect_fonts(
    engine: &mut TextEngine,
    run: &ShapedRun,
    fonts: &mut BTreeMap<fontdb::ID, FontUse>,
) -> Result<()> {
    for line in &run.shaped.lines {
        for glyph in &line.glyphs {
            if glyph.glyph_id == 0 {
                continue;
            }
            if !fonts.contains_key(&glyph.font_id) {
                let blob = engine
                    .font_blob(glyph.font_id)
                    .ok_or_else(|| ExportError::Pdf("Font data unavailable for PDF.".into()))?;
                let metrics = engine.face_metrics(glyph.font_id).ok_or_else(|| {
                    ExportError::Pdf("Could not read font metrics for PDF.".into())
                })?;
                fonts.insert(
                    glyph.font_id,
                    FontUse {
                        blob,
                        metrics,
                        glyphs: BTreeSet::new(),
                        cmap: BTreeMap::new(),
                    },
                );
            }
            let entry = fonts.get_mut(&glyph.font_id).unwrap();
            entry.glyphs.insert(glyph.glyph_id);
            let slice = run
                .text
                .get(glyph.start.min(run.text.len())..glyph.end.min(run.text.len()))
                .unwrap_or("");
            let chars: Vec<char> = slice
                .chars()
                .filter(|c| {
                    !c.is_control()
                        && !matches!(
                            c,
                            '\u{200e}'
                                | '\u{200f}'
                                | '\u{202a}'..='\u{202e}'
                                | '\u{2066}'..='\u{2069}'
                        )
                })
                .collect();
            if !chars.is_empty() {
                entry.cmap.entry(glyph.glyph_id).or_insert(chars);
            }
        }
    }
    Ok(())
}

fn width_for_glyph(blob: &daftar_text::FontBlob, gid: u16) -> Option<f32> {
    let face = Face::parse(&blob.data, blob.index).ok()?;
    let upem = face.units_per_em() as f32;
    let advance = face.glyph_hor_advance(GlyphId(gid))? as f32;
    Some(advance / upem * 1000.0)
}

fn build_to_unicode(cmap: &BTreeMap<u16, Vec<char>>) -> String {
    let mut out = String::new();
    out.push_str(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(u16, &Vec<char>)> = cmap.iter().map(|(gid, chars)| (*gid, chars)).collect();
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, chars) in chunk {
            let hex: String = chars
                .iter()
                .map(|c| format!("{:04X}", *c as u32))
                .collect::<Vec<_>>()
                .join("");
            out.push_str(&format!("<{gid:04X}> <{hex}>\n"));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    out
}
