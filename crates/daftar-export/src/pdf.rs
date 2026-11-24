//! Native PDF generation with real text operators and embedded TrueType fonts.
//!
//! Cards are laid out with the bidirectional shaper, then every glyph is written as a CID in an
//! Identity-H Type0 font. The original font is embedded (SIL OFL fonts from `assets/fonts`),
//! with a ToUnicode CMap for searching and copying. This is a genuine vector PDF; nothing is
//! rasterised from the application UI.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use daftar_text::TextEngine;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use ttf_parser::{Face, GlyphId};

use crate::cards::{CardContent, CardLayout, CardStyle, layout_card};
use crate::error::{ExportError, Result};
use crate::Rgba8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperSize {
    A4,
    Letter,
    /// Page size follows the card dimensions plus margins.
    Card,
}

impl PaperSize {
    /// Width and height in PDF points.
    pub fn points(self, card: (f32, f32), margin: f32) -> (f32, f32) {
        match self {
            PaperSize::A4 => (595.276, 841.89),
            PaperSize::Letter => (612.0, 792.0),
            PaperSize::Card => (card.0 + margin * 2.0, card.1 + margin * 2.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::Letter => "US Letter",
            PaperSize::Card => "Card size",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardsPerPage {
    One,
    Grid { cols: u32, rows: u32 },
}

impl CardsPerPage {
    pub fn label(self) -> String {
        match self {
            CardsPerPage::One => "One card per page".to_string(),
            CardsPerPage::Grid { cols, rows } => format!("{cols} × {rows} grid"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PdfOptions {
    pub paper: PaperSize,
    /// Page margin in points (1/72 inch).
    pub margin: f32,
    pub cards_per_page: CardsPerPage,
    pub page_numbers: bool,
    pub title: String,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            paper: PaperSize::A4,
            margin: 34.0,
            cards_per_page: CardsPerPage::Grid { cols: 2, rows: 3 },
            page_numbers: true,
            title: "Daftar Study Cards".to_string(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfReport {
    pub pages: u32,
    pub cards: u32,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy)]
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

/// Export cards to a multi-page, print-ready PDF.
pub fn export_cards_pdf(
    cards: &[CardContent],
    style: &CardStyle,
    engine: &mut TextEngine,
    options: &PdfOptions,
    output: impl AsRef<std::path::Path>,
) -> Result<PdfReport> {
    if cards.is_empty() {
        return Err(ExportError::Data("No cards selected for PDF export.".into()));
    }
    let mut style = style.clone();
    style.validate();
    style.scale = 1.0;

    let layouts: Vec<CardLayout> = cards
        .iter()
        .map(|card| layout_card(card, &style, engine))
        .collect();

    let (page_w, page_h) = options.paper.points((style.width, style.height), options.margin);
    let per_page = match options.cards_per_page {
        CardsPerPage::One => 1usize,
        CardsPerPage::Grid { cols, rows } => (cols.max(1) * rows.max(1)) as usize,
    };
    let page_count = layouts.len().div_ceil(per_page);

    // Collect fonts and ToUnicode mappings from every shaped run.
    let mut fonts: BTreeMap<fontdb::ID, FontUse> = BTreeMap::new();
    for layout in &layouts {
        for text in &layout.texts {
            for line in &text.shaped.lines {
                for glyph in &line.glyphs {
                    if glyph.glyph_id == 0 {
                        continue;
                    }
                    let entry = match fonts.get_mut(&glyph.font_id) {
                        Some(entry) => entry,
                        None => {
                            let blob = engine.font_blob(glyph.font_id).ok_or_else(|| {
                                ExportError::Pdf(format!(
                                    "Font data unavailable for {} (used in PDF export).",
                                    text.text
                                ))
                            })?;
                            let metrics = engine.face_metrics(glyph.font_id).ok_or_else(|| {
                                ExportError::Pdf("Could not read font metrics for PDF.".into())
                            })?;
                            fonts.entry(glyph.font_id).or_insert(FontUse {
                                blob,
                                metrics,
                                glyphs: BTreeSet::new(),
                                cmap: BTreeMap::new(),
                            })
                        }
                    };
                    entry.glyphs.insert(glyph.glyph_id);
                    let slice = text
                        .text
                        .get(glyph.start.min(text.text.len())..glyph.end.min(text.text.len()))
                        .unwrap_or("");
                    let chars: Vec<char> = slice
                        .chars()
                        .filter(|c| {
                            !c.is_control()
                                && !matches!(
                                    c,
                                    '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                                )
                        })
                        .collect();
                    if !chars.is_empty() {
                        entry.cmap.entry(glyph.glyph_id).or_insert(chars);
                    }
                }
            }
        }
    }
    if fonts.is_empty() {
        return Err(ExportError::Pdf(
            "No text glyphs were produced for PDF export.".into(),
        ));
    }

    let font_ids: Vec<fontdb::ID> = fonts.keys().copied().collect();
    let mut pdf = Pdf::new();
    let catalog_ref = Ref::new(1);
    let pages_ref = Ref::new(2);
    let info_ref = Ref::new(3);
    let mut next = 4i32;
    let mut font_refs: HashMap<fontdb::ID, FontRefs> = HashMap::new();
    let mut used_names: Vec<&'static [u8]> = Vec::new();
    for (index, id) in font_ids.iter().enumerate() {
        let resource_name: &'static [u8] = Box::leak(format!("F{}", index + 1).into_bytes().into_boxed_slice());
        let base_name: &'static [u8] = Box::leak(
            format!("DaftarFont{}", index + 1).into_bytes().into_boxed_slice(),
        );
        let refs = FontRefs {
            type0: Ref::new(next),
            cid: Ref::new(next + 1),
            descriptor: Ref::new(next + 2),
            file: Ref::new(next + 3),
            cmap: Ref::new(next + 4),
            resource_name,
            base_name,
        };
        next += 5;
        font_refs.insert(*id, refs);
        used_names.push(resource_name);
    }
    let helvetica_ref = Ref::new(next);
    next += 1;
    let mut page_refs = Vec::with_capacity(page_count);
    let mut content_refs = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        page_refs.push(Ref::new(next));
        content_refs.push(Ref::new(next + 1));
        next += 2;
    }

    // Fonts.
    for id in font_ids.iter() {
        let Some(refs) = font_refs.get(id) else {
            continue;
        };
        let FontRefs { type0, cid, descriptor, file, cmap: cmap_ref, base_name, .. } = *refs;
        let entry = &fonts[id];
        let base_name = Name(base_name);
        let system_info = SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        };

        // Type0 font.
        let mut type0_font = pdf.type0_font(type0);
        type0_font.base_font(base_name);
        type0_font.encoding_predefined(Name(b"Identity-H"));
        type0_font.descendant_font(cid);
        type0_font.to_unicode(cmap_ref);
        type0_font.finish();

        // CIDFont.
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
        cid_font.system_info(system_info);
        cid_font.font_descriptor(descriptor);
        cid_font.default_width(1000.0);
        cid_font.cid_to_gid_map_predefined(Name(b"Identity"));
        cid_font.widths().consecutive(0, widths);
        cid_font.finish();

        // Descriptor.
        let bbox = entry.metrics.bbox;
        let bbox = Rect::new(
            bbox.x_min as f32 / upem * 1000.0,
            bbox.y_min as f32 / upem * 1000.0,
            bbox.x_max as f32 / upem * 1000.0,
            bbox.y_max as f32 / upem * 1000.0,
        );
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
        desc.bbox(bbox);
        desc.italic_angle(entry.metrics.italic_angle);
        desc.ascent(entry.metrics.ascender as f32 / upem * 1000.0);
        desc.descent(entry.metrics.descender as f32 / upem * 1000.0);
        desc.cap_height(entry.metrics.cap_height as f32 / upem * 1000.0);
        desc.stem_v(80.0);
        desc.font_file2(file);
        desc.finish();

        // Embedded TrueType font.
        let mut stream = pdf.stream(file, &entry.blob.data);
        stream.pair(Name(b"Length1"), entry.blob.data.len() as i32);
        stream.finish();

        // ToUnicode CMap.
        let mut cmap = UnicodeCmap::new(Name(b"DaftarToUnicode"), system_info);
        for (gid, chars) in &entry.cmap {
            cmap.pair_with_multiple(*gid, chars.iter().copied());
        }
        let cmap_bytes = cmap.finish();
        pdf.cmap(cmap_ref, &cmap_bytes).finish();
    }

    // Base-14 font for page numbers.
    let mut helvetica = pdf.type1_font(helvetica_ref);
    helvetica.base_font(Name(b"Helvetica"));
    helvetica.encoding_predefined(Name(b"WinAnsiEncoding"));
    helvetica.finish();

    // Pages.
    let mut pages = pdf.pages(pages_ref);
    pages.kids(page_refs.iter().copied());
    pages.count(page_count as i32);
    pages.finish();

    let gap = 14.0f32;
    let (cols, rows) = match options.cards_per_page {
        CardsPerPage::One => (1u32, 1u32),
        CardsPerPage::Grid { cols, rows } => (cols.max(1), rows.max(1)),
    };
    let cell_w = (page_w - options.margin * 2.0 - gap * (cols.saturating_sub(1)) as f32)
        / cols as f32;
    let cell_h = (page_h - options.margin * 2.0 - gap * (rows.saturating_sub(1)) as f32)
        / rows as f32;

    for page_index in 0..page_count {
        let mut content = Content::new();
        // Page background.
        content.set_fill_rgb(1.0, 1.0, 1.0);
        content.rect(0.0, 0.0, page_w, page_h);
        content.fill_nonzero();

        for slot in 0..per_page {
            let card_index = page_index * per_page + slot;
            let Some(layout) = layouts.get(card_index) else {
                break;
            };
            let col = (slot as u32 % cols) as f32;
            let row = (slot as u32 / cols) as f32;
            let cell_x = options.margin + col * (cell_w + gap);
            let cell_y_from_top = options.margin + row * (cell_h + gap);
            let fit = (cell_w / layout.width)
                .min(cell_h / layout.height)
                .min(if matches!(options.cards_per_page, CardsPerPage::One) {
                    1.25
                } else {
                    1.0
                });
            let draw_w = layout.width * fit;
            let draw_h = layout.height * fit;
            let slot_x = cell_x + (cell_w - draw_w) * 0.5;
            let slot_y_top = cell_y_from_top + (cell_h - draw_h) * 0.5;
            draw_layout(
                &mut content,
                layout,
                slot_x,
                slot_y_top,
                fit,
                page_h,
                &font_ids,
                &font_refs,
                &fonts,
            );
        }

        if options.page_numbers && page_count > 1 {
            let label = format!("{} / {}", page_index + 1, page_count);
            content.begin_text();
            content.set_font(Name(b"FPAGE"), 9.0);
            content.set_fill_rgb(0.42, 0.42, 0.42);
            let width = label.len() as f32 * 4.7;
            content.set_text_matrix([1.0, 0.0, 0.0, 1.0, (page_w - width) * 0.5, options.margin * 0.42]);
            content.show(Str(label.as_bytes()));
            content.end_text();
        }

        let content_bytes = content.finish();
        pdf.stream(content_refs[page_index], &content_bytes);

        let mut page = pdf.page(page_refs[page_index]);
        page.parent(pages_ref);
        page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
        page.contents(content_refs[page_index]);
        {
            let mut resources = page.resources();
            let mut fonts_dict = resources.fonts();
            for (index, name) in used_names.iter().enumerate() {
                let id = font_ids[index];
                let type0 = font_refs[&id].type0;
                fonts_dict.pair(Name(name), type0);
            }
            fonts_dict.pair(Name(b"FPAGE"), helvetica_ref);
        }
        page.finish();
    }

    let mut catalog = pdf.catalog(catalog_ref);
    catalog.pages(pages_ref);
    catalog.finish();
    let mut info = pdf.document_info(info_ref);
    let title_bytes: &'static [u8] = Box::leak(options.title.clone().into_bytes().into_boxed_slice());
    let title_text = std::str::from_utf8(title_bytes).unwrap_or("Daftar Study Cards");
    info.title(TextStr(title_text));
    info.creator(TextStr("Daftar"));
    info.producer(TextStr("Daftar native PDF writer"));
    info.finish();

    let bytes = pdf.finish();
    std::fs::write(output.as_ref(), &bytes)?;
    Ok(PdfReport {
        pages: page_count as u32,
        cards: layouts.len() as u32,
        bytes: bytes.len() as u64,
    })
}

fn width_for_glyph(blob: &daftar_text::FontBlob, gid: u16) -> Option<f32> {
    let face = Face::parse(&blob.data, blob.index).ok()?;
    let upem = face.units_per_em() as f32;
    let advance = face.glyph_hor_advance(GlyphId(gid))? as f32;
    Some(advance / upem * 1000.0)
}

#[allow(clippy::too_many_arguments)]
fn draw_layout(
    content: &mut Content,
    layout: &CardLayout,
    slot_x: f32,
    slot_y_top: f32,
    scale: f32,
    page_h: f32,
    font_ids: &[fontdb::ID],
    font_refs: &HashMap<fontdb::ID, FontRefs>,
    fonts: &BTreeMap<fontdb::ID, FontUse>,
) {
    let y = |top: f32| page_h - (slot_y_top + top * scale);
    for rect in &layout.rects {
        let x = slot_x + rect.x * scale;
        let top = rect.y * scale;
        let w = rect.width * scale;
        let h = rect.height * scale;
        let bottom = y(rect.y + rect.height);
        if let Some(fill) = rect.fill {
            set_fill(content, fill);
            rounded_rect(content, x, bottom, w, h, rect.radius * scale);
            content.fill_nonzero();
        }
        if let Some(stroke) = rect.stroke {
            set_stroke(content, stroke);
            content.set_line_width((rect.stroke_width * scale).max(0.4));
            rounded_rect(content, x, bottom, w, h, rect.radius * scale);
            content.stroke();
        }
        let _ = top;
    }
    for line in &layout.lines {
        set_stroke(content, line.color);
        content.set_line_width((line.width * scale).max(0.4));
        content.move_to(slot_x + line.x1 * scale, y(line.y1));
        content.line_to(slot_x + line.x2 * scale, y(line.y2));
        content.stroke();
    }
    for text in &layout.texts {
        set_fill(content, text.color);
        content.begin_text();
        // The first glyph determines the font; mixed-font cards switch resources as needed.
        for line in &text.shaped.lines {
            for glyph in &line.glyphs {
                if glyph.glyph_id == 0 {
                    continue;
                }
                let Some(name) = font_refs.get(&glyph.font_id).map(|f| f.resource_name) else {
                    continue;
                };
                content.set_font(Name(name), glyph.font_size * scale);
                let gx = slot_x + (text.x + glyph.x) * scale;
                let gy = y(text.y + glyph.baseline);
                content.set_text_matrix([1.0, 0.0, 0.0, 1.0, gx, gy]);
                let gid = glyph.glyph_id;
                let bytes = gid.to_be_bytes();
                content.show(Str(&bytes));
            }
        }
        content.end_text();
    }
    let _ = (font_ids, fonts);
}

fn set_fill(content: &mut Content, color: Rgba8) {
    content.set_fill_rgb(color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0);
}

fn set_stroke(content: &mut Content, color: Rgba8) {
    content.set_stroke_rgb(color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0);
}

fn rounded_rect(content: &mut Content, x: f32, y: f32, w: f32, h: f32, r: f32) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    content.move_to(x + r, y);
    content.line_to(x + w - r, y);
    content.cubic_to(x + w, y, x + w, y, x + w, y + r);
    content.line_to(x + w, y + h - r);
    content.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    content.line_to(x + r, y + h);
    content.cubic_to(x, y + h, x, y + h, x, y + h - r);
    content.line_to(x, y + r);
    content.cubic_to(x, y, x, y, x + r, y);
    content.close_path();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardKind;
    use std::io::Cursor;
    use daftar_text::TextEngine;

    fn card(front: &str, back: &str) -> CardContent {
        CardContent {
            kind: CardKind::Flashcard,
            prompt: front.into(),
            answer: Some(back.into()),
            note: None,
            source: Some("Test document".into()),
            tags: vec![],
            side_label: Some("FRONT / BACK".into()),
            include_back: true,
        }
    }

    #[test]
    fn exports_valid_pdf_with_embedded_fonts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cards.pdf");
        let mut engine = TextEngine::new(None, "fa-IR");
        let cards = vec![
            card("What is ownership?", "Rust's memory safety model."),
            card("مالکیت چیست؟", "مدل ایمنی حافظه در راست."),
        ];
        let report = export_cards_pdf(
            &cards,
            &CardStyle::light(),
            &mut engine,
            &PdfOptions {
                paper: PaperSize::A4,
                cards_per_page: CardsPerPage::One,
                ..Default::default()
            },
            &path,
        )
        .unwrap();
        assert_eq!(report.cards, 2);
        assert_eq!(report.pages, 2);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.windows(9).any(|w| w == b"FontFile2"));
        assert!(bytes.windows(9).any(|w| w == b"ToUnicode"));
        // Parse with lopdf to ensure the document is structurally readable.
        let doc = lopdf::Document::load_mem(&bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 2);
        let text = doc.extract_text(&[1]).unwrap_or_default();
        assert!(!text.is_empty(), "page 1 should contain extractable text");
    }

    #[test]
    fn grid_layout_produces_one_page_for_two_cards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grid.pdf");
        let mut engine = TextEngine::new(None, "en-US");
        let cards = vec![card("A", "B"), card("C", "D")];
        let report = export_cards_pdf(
            &cards,
            &CardStyle::light(),
            &mut engine,
            &PdfOptions::default(),
            &path,
        )
        .unwrap();
        assert_eq!(report.pages, 1);
        let doc = lopdf::Document::load_mem(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
        // Both strings appear in extracted text (word order depends on shaping).
        let text = doc.extract_text(&[1]).unwrap_or_default();
        assert!(text.contains('A') && text.contains("D"));
    }

    #[test]
    fn empty_batch_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = TextEngine::new(None, "en-US");
        let err = export_cards_pdf(
            &[],
            &CardStyle::light(),
            &mut engine,
            &PdfOptions::default(),
            dir.path().join("x.pdf"),
        )
        .unwrap_err();
        assert!(matches!(err, ExportError::Data(_)));
    }

    #[test]
    fn paper_sizes_are_correct() {
        let (w, h) = PaperSize::A4.points((480.0, 320.0), 30.0);
        assert!((w - 595.276).abs() < 0.01);
        assert!((h - 841.89).abs() < 0.01);
    }

    #[test]
    fn pdf_starts_with_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("one.pdf");
        let mut engine = TextEngine::new(None, "en-US");
        export_cards_pdf(
            &[card("Q", "A")],
            &CardStyle::dark(),
            &mut engine,
            &PdfOptions::default(),
            &path,
        )
        .unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(b"%PDF-1.7") || bytes.starts_with(b"%PDF-1.6"));
        let _ = Cursor::new(bytes);
    }
}
