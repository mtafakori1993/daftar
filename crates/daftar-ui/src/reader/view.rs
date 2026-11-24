//! Interactive reader: selection, highlights, bookmarks, code copy, diagram viewer and TOC.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use daftar_core::anchor::{TextAnchor, relocate_anchor};
use daftar_core::direction::{Direction, resolve_direction};
use daftar_core::markdown::{MdDocument, QuoteKind, parse_markdown};
use daftar_core::model::{Bookmark, Document, Highlight};
use daftar_core::settings::{AppSettings, TextAlign};
use daftar_core::util::{now_ms, plain_text, truncate_chars};
use daftar_store::Store;
use daftar_text::TextEngine;
use egui::{
    Align2, Color32, Id, Order, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, TextureHandle, Ui,
    Vec2, vec2,
};

use crate::code::CodeHighlighter;
use crate::icons::{Icon, draw_icon};
use crate::images::load_image;
use crate::reader::layout::{
    DiagramCache, Element, ElementKind, ListMarker, ReaderConfig, ReaderLayout, TableLayout,
    build_layout, diagram_key, diagram_theme,
};
use crate::shaped::{ShapedStyle, shaped_label};
use crate::theme::{Palette, ReaderColors, ToastKind};
use crate::widgets::{self, ButtonVariant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    pub anchor: usize,
    pub focus: usize,
}

impl TextSelection {
    pub fn range(&self) -> std::ops::Range<usize> {
        if self.anchor <= self.focus {
            self.anchor..self.focus
        } else {
            self.focus..self.anchor
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.focus
    }
}

#[derive(Debug, Clone)]
pub enum ReaderAction {
    Back,
    OpenDocument(String),
    OpenExternal(String),
    SetDirection(Direction),
    ChangeFontSize(f32),
    ToggleTheme,
    ToggleRightPanel,
    CreateFlashcard {
        document_id: String,
        quote: String,
    },
    ExportQuoteImage {
        quote: String,
        source: String,
    },
    ExportDiagram {
        key: u64,
        title: String,
    },
    Notice(String, ToastKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SideTab {
    Outline,
    Marks,
    Info,
}

struct ImageTexture {
    texture: TextureHandle,
    width: u32,
    height: u32,
}

struct DiagramViewer {
    key: u64,
    zoom: f32,
    pan: Vec2,
    title: String,
}

struct NoteDialog {
    selection: TextSelection,
    text: String,
    highlight_id: Option<String>,
}

#[derive(Clone)]
struct HighlightMenu {
    id: String,
    pos: Pos2,
    fresh: bool,
}

pub struct ReaderState {
    pub doc: Document,
    pub source: String,
    pub parsed: MdDocument,
    pub content_hash: String,
    pub layout: ReaderLayout,
    layout_key: String,
    pub config: ReaderConfig,
    pub highlights: Vec<Highlight>,
    pub bookmarks: Vec<Bookmark>,
    pub orphaned: Vec<String>,
    pub selection: Option<TextSelection>,
    selecting: bool,
    press_pos: Option<Pos2>,
    scroll_offset: f32,
    pending_scroll: Option<f32>,
    restored_scroll: bool,
    pub diagrams: DiagramCache,
    image_textures: HashMap<PathBuf, Option<ImageTexture>>,
    side_tab: SideTab,
    diagram_viewer: Option<DiagramViewer>,
    note_dialog: Option<NoteDialog>,
    highlight_menu: Option<HighlightMenu>,
    last_progress_save: i64,
    session_start: i64,
    session_progress_start: f32,
    viewport_height: f32,
    pub missing: bool,
    library_root: Option<PathBuf>,
}

impl ReaderState {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        store: &Store,
        doc: Document,
        settings: &AppSettings,
        engine: &mut TextEngine,
        code: &CodeHighlighter,
        palette: &Palette,
        library_root: &Path,
        available_width: f32,
        viewport_height: f32,
    ) -> Result<Self, String> {
        let path = library_root.join(daftar_core::model::rel_to_native(&doc.rel_path));
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                let _ = store.set_document_missing(&doc.id, true);
                return Err(crate::i18n::tf("Could not open {}: {}", &[&path.display().to_string(), &error.to_string()]));
            }
        };
        let _ = store.set_document_missing(&doc.id, false);
        let _ = store.touch_document(&doc.id);
        let parsed = parse_markdown(&source);
        let hash = daftar_core::util::sha256_hex(source.as_bytes());
        let colors = palette.reader_theme(&settings.theme());
        let preference = if doc.direction == Direction::Auto {
            parsed.direction_hint.unwrap_or(Direction::Auto)
        } else {
            doc.direction
        };
        let resolved = resolve_direction(preference, &source).resolved;
        let config = ReaderConfig::from_settings(settings, palette, preference, resolved, &parsed);
        let mut state = Self {
            doc,
            content_hash: hash,
            source,
            parsed,
            layout_key: String::new(),
            layout: ReaderLayout {
                column_width: 700.0,
                left: 24.0,
                total_height: 1.0,
                elements: Vec::new(),
                quotes: Vec::new(),
                headings: HashMap::new(),
                footnotes: HashMap::new(),
                diagram_keys: Vec::new(),
                available_width,
            },
            config,
            highlights: Vec::new(),
            bookmarks: Vec::new(),
            orphaned: Vec::new(),
            selection: None,
            selecting: false,
            press_pos: None,
            scroll_offset: 0.0,
            pending_scroll: None,
            restored_scroll: false,
            diagrams: DiagramCache::new(),
            image_textures: HashMap::new(),
            side_tab: SideTab::Outline,
            diagram_viewer: None,
            note_dialog: None,
            highlight_menu: None,
            last_progress_save: 0,
            session_start: now_ms(),
            session_progress_start: settings
                .reading
                .remember_scroll_position
                .then_some(0.0)
                .unwrap_or(0.0),
            viewport_height,
            missing: false,
            library_root: Some(library_root.to_path_buf()),
        };
        state.load_annotations(store);
        state.rebuild_layout(engine, code, &colors, available_width, settings);
        state.scroll_offset = 0.0;
        state.pending_scroll = Some(
            (state.doc.scroll_progress.clamp(0.0, 1.0)
                * (state.layout.total_height - viewport_height).max(0.0))
            .max(0.0),
        );
        state.restored_scroll = true;
        state.session_progress_start = state.doc.reading_progress;
        Ok(state)
    }

    pub fn from_settings(
        settings: &AppSettings,
        palette: &Palette,
        preference: Direction,
        resolved: daftar_core::direction::ResolvedDirection,
        doc: &MdDocument,
    ) -> ReaderConfig {
        ReaderConfig::from_settings(settings, palette, preference, resolved, doc)
    }

    pub fn title(&self) -> &str {
        &self.doc.title
    }

    pub fn word_count(&self) -> usize {
        self.parsed.word_count
    }

    pub fn reading_minutes(&self) -> u32 {
        self.parsed.reading_minutes
    }

    fn load_annotations(&mut self, store: &Store) {
        self.orphaned.clear();
        self.highlights = store.highlights_for_document(&self.doc.id).unwrap_or_default();
        for highlight in &mut self.highlights {
            let status = relocate_anchor(&self.source, &highlight.anchor());
            if let Some(range) = status.range {
                let changed = highlight.start_offset != range.start as i64
                    || highlight.end_offset != range.end as i64;
                highlight.start_offset = range.start as i64;
                highlight.end_offset = range.end as i64;
                if changed && !status.exact {
                    let _ = store.upsert_highlight(highlight);
                }
            } else {
                self.orphaned.push(highlight.id.clone());
            }
        }
        self.bookmarks = store.bookmarks_for_document(&self.doc.id).unwrap_or_default();
        for bookmark in &mut self.bookmarks {
            let status = relocate_anchor(&self.source, &bookmark.anchor());
            if let Some(range) = status.range {
                let changed = bookmark.start_offset != range.start as i64
                    || bookmark.end_offset != range.end as i64;
                bookmark.start_offset = range.start as i64;
                bookmark.end_offset = range.end as i64;
                if changed && !status.exact {
                    let _ = store.upsert_bookmark(bookmark);
                }
            } else {
                self.orphaned.push(bookmark.id.clone());
            }
        }
    }

    fn rebuild_layout(
        &mut self,
        engine: &mut TextEngine,
        code: &CodeHighlighter,
        colors: &ReaderColors,
        available_width: f32,
        settings: &AppSettings,
    ) {
        let key = format!(
            "{}|{}|{}|{:.1}",
            self.doc.id,
            self.content_hash,
            self.config.key(),
            available_width
        );
        if key == self.layout_key {
            return;
        }
        let doc_dir = PathBuf::from(&self.doc.rel_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();
        let library_root = self
            .library_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("."));
        let base_path = library_root.join(doc_dir);
        let _ = settings;
        self.layout = build_layout(
            engine,
            &self.parsed,
            &self.config,
            available_width,
            &base_path,
            code,
            &mut self.diagrams,
            colors,
        );
        self.layout_key = key;
        let max = (self.layout.total_height - self.viewport_height).max(1.0);
        if !self.restored_scroll {
            self.pending_scroll = Some((self.doc.scroll_progress * max).clamp(0.0, max));
        } else {
            self.pending_scroll = Some(self.scroll_offset.clamp(0.0, max));
        }
    }

    pub fn update_layout(
        &mut self,
        engine: &mut TextEngine,
        code: &CodeHighlighter,
        palette: &Palette,
        settings: &AppSettings,
        available_width: f32,
    ) {
        let colors = palette.reader_theme(&settings.theme());
        let preference = if self.doc.direction == Direction::Auto {
            self.parsed.direction_hint.unwrap_or(Direction::Auto)
        } else {
            self.doc.direction
        };
        let resolved = resolve_direction(preference, &self.source).resolved;
        self.config =
            ReaderConfig::from_settings(settings, palette, preference, resolved, &self.parsed);
        self.rebuild_layout(engine, code, &colors, available_width, settings);
    }

    pub fn scroll_to_heading(&mut self, id: &str) -> bool {
        if let Some(y) = self.layout.headings.get(id).copied() {
            self.pending_scroll = Some((y - 24.0).max(0.0));
            true
        } else if let Some(y) = self.layout.footnotes.get(id).copied() {
            self.pending_scroll = Some((y - 24.0).max(0.0));
            true
        } else {
            false
        }
    }

    pub fn progress(&self) -> f32 {
        let max = (self.layout.total_height - self.viewport_height).max(1.0);
        (self.scroll_offset / max).clamp(0.0, 1.0)
    }

    pub fn save_progress(&mut self, store: &Store) {
        if now_ms() - self.last_progress_save < 1500 {
            return;
        }
        self.last_progress_save = now_ms();
        let progress = self.progress();
        let _ = store.update_reading_progress(&self.doc.id, progress, progress);
        self.doc.reading_progress = progress;
        self.doc.scroll_progress = progress;
    }

    pub fn finish_session(&mut self, store: &Store) {
        let end = now_ms();
        if end - self.session_start < 1000 {
            return;
        }
        let _ = store.update_reading_progress(&self.doc.id, self.progress(), self.progress());
        let session = daftar_core::model::ReadingSession {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: self.doc.id.clone(),
            started_at: self.session_start,
            ended_at: end,
            progress_start: self.session_progress_start,
            progress_end: self.progress(),
        };
        let _ = store.add_reading_session(&session);
    }

    // ------------------------------------------------------------------------------------
    // Main view
    // ------------------------------------------------------------------------------------

    pub fn show(
        &mut self,
        ui: &mut Ui,
        engine: &mut TextEngine,
        code: &CodeHighlighter,
        palette: &Palette,
        settings: &mut AppSettings,
        store: &Store,
        library_root: &Path,
    ) -> Vec<ReaderAction> {
        log::info!("DBG reader show layer={:?}", ui.layer_id());
        let mut actions = Vec::new();
        self.library_root = Some(library_root.to_path_buf());
        let colors = palette.reader_theme(&settings.theme());
        self.update_layout(engine, code, palette, settings, ui.available_width());

        // Header.
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, palette, Icon::ArrowLeft, "Back", false).clicked() {
                actions.push(ReaderAction::Back);
            }
            ui.add_space(6.0);
            let available = (ui.available_width() - 300.0).max(120.0);
            let title = shaped_label(
                ui,
                engine,
                &self.doc.title,
                &ShapedStyle::title().with_size(16.0),
                palette.text,
                available,
                Sense::hover(),
            );
            if title.hovered() {
                title.on_hover_text(&self.doc.rel_path);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, palette, Icon::SidebarRight, "Toggle outline panel", false)
                    .clicked()
                {
                    actions.push(ReaderAction::ToggleRightPanel);
                }
                if widgets::icon_button(ui, palette, Icon::Palette, "Toggle theme", false).clicked() {
                    actions.push(ReaderAction::ToggleTheme);
                }
                widgets::toolbar_divider(ui, palette);
                if widgets::icon_button(ui, palette, Icon::Plus, "Increase reading font size", false)
                    .clicked()
                {
                    actions.push(ReaderAction::ChangeFontSize(1.0));
                }
                if widgets::icon_button(ui, palette, Icon::Minus, "Decrease reading font size", false)
                    .clicked()
                {
                    actions.push(ReaderAction::ChangeFontSize(-1.0));
                }
                let direction_label = match self.doc.direction {
                    Direction::Auto => "AUTO",
                    Direction::Ltr => "LTR",
                    Direction::Rtl => "RTL",
                };
                if widgets::button(
                    ui,
                    palette,
                    direction_label,
                    Some(Icon::ArrowRight),
                    ButtonVariant::Quiet,
                )
                .on_hover_text(crate::i18n::t("Cycle text direction (automatic, RTL, LTR)"))
                .clicked()
                {
                    actions.push(ReaderAction::SetDirection(self.doc.direction.toggled()));
                }
            });
        });
        // Thin reading progress line.
        let progress = self.progress();
        if progress > 0.0 {
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
            ui.painter()
                .rect_filled(rect, 0.0, palette.surface_sunken);
            ui.painter().rect_filled(
                Rect::from_min_size(rect.min, vec2(rect.width() * progress, 2.0)),
                0.0,
                palette.accent,
            );
        }
        ui.add_space(2.0);

        if self.orphaned.len() > 0 {
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, palette.warning_subtle);
            draw_icon(
                ui.painter(),
                Icon::Warning,
                Rect::from_min_size(rect.min + vec2(8.0, 7.0), Vec2::splat(14.0)),
                palette.warning,
            );
            ui.painter().text(
                rect.min + vec2(28.0, 8.0),
                Align2::LEFT_TOP,
                format!(
                    "{} bookmark/highlight anchor(s) could not be relocated after the file changed. Nothing was deleted.",
                    self.orphaned.len()
                ),
                egui::FontId::proportional(11.5),
                palette.text_secondary,
            );
        }

        self.viewport_height = ui.available_height().max(200.0);
        let content_width = ui.available_width().max(320.0);
        let offset = self.pending_scroll.take().unwrap_or(self.scroll_offset);
        let mut open_diagram: Option<u64> = None;
        let mut context_screen_pos: Option<Pos2> = None;
        let mut forced_toolbar: Option<Pos2> = None;
        let mut clicked_link: Option<String> = None;
        let mut new_selection: Option<TextSelection> = None;
        let mut scroll_output = None;
        let mut copy_code: Option<String> = None;

        let output = egui::ScrollArea::vertical()
            .id_salt(("reader-scroll", &self.doc.id))
            .auto_shrink([false, false])
            .vertical_scroll_offset(offset)
            .show(ui, |ui| {
                let width = ui.available_width().max(320.0);
                let (content_rect, response) = ui.allocate_exact_size(
                    vec2(width, self.layout.total_height.max(ui.available_height())),
                    Sense::click_and_drag(),
                );
                let origin = content_rect.min;
                let layout = &self.layout;
                let colors_ref = &colors;

                // ---- pointer interaction ----
                let pointer = ui.ctx().pointer_interact_pos();
                let pressed = ui.input(|i| i.pointer.primary_pressed());
                let double_click = ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary));
                let released = ui.input(|i| i.pointer.any_released());
                let shift = ui.input(|i| i.modifiers.shift);
                if pressed {
                    if let Some(pos) = pointer {
                        if content_rect.contains(pos) {
                            if let Some((element_idx, source_offset)) =
                                hit_test(layout, pos, origin)
                            {
                                if double_click {
                                    let range = word_range(&self.source, source_offset);
                                    self.selection = Some(TextSelection {
                                        anchor: range.start,
                                        focus: range.end,
                                    });
                                    self.selecting = false;
                                    self.press_pos = None;
                                } else {
                                    let anchor = if shift {
                                        self.selection.map(|s| s.anchor).unwrap_or(source_offset)
                                    } else {
                                        source_offset
                                    };
                                    self.selection = Some(TextSelection {
                                        anchor,
                                        focus: source_offset,
                                    });
                                    self.selecting = true;
                                    self.press_pos = Some(pos);
                                }
                                let _ = element_idx;
                            }
                        }
                    }
                }
                if ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary)) {
                    if let Some(pos) = pointer {
                        if content_rect.contains(pos) {
                            if let Some((_idx, offset)) = hit_test(layout, pos, origin) {
                                let hit = self.highlights.iter().find(|h| {
                                    let start = h.start_offset.max(0) as usize;
                                    let end = h.end_offset.max(0) as usize;
                                    offset >= start && offset < end
                                });
                                if let Some(hit) = hit {
                                    self.highlight_menu = Some(HighlightMenu {
                                        id: hit.id.clone(),
                                        pos: egui::pos2(
                                            pos.x.min(ui.ctx().content_rect().right() - 200.0),
                                            pos.y,
                                        ),
                                        fresh: true,
                                    });
                                    self.selection = None;
                                } else if let Some(sel) = self.selection {
                                    let range = sel.range();
                                    if !range.is_empty()
                                        && offset >= range.start
                                        && offset <= range.end
                                    {
                                        forced_toolbar = Some(pos);
                                    }
                                }
                            }
                        }
                    }
                }
                if self.selecting {
                    if let Some(pos) = pointer {
                        if let Some((_idx, offset)) = hit_test_clamped(layout, pos, origin) {
                            if let Some(selection) = self.selection.as_mut() {
                                selection.focus = offset;
                            }
                        }
                    }
                    if released {
                        self.selecting = false;
                        let mut word_selected = false;
                        if double_click {
                            if let Some(pos) = pointer {
                                if let Some((_, offset)) = hit_test(layout, pos, origin) {
                                    let range = word_range(&self.source, offset);
                                    if !range.is_empty() {
                                        self.selection = Some(TextSelection {
                                            anchor: range.start,
                                            focus: range.end,
                                        });
                                        word_selected = true;
                                    }
                                }
                            }
                        }
                        let moved = self
                            .press_pos
                            .map(|p| pointer.map(|q| (p - q).length() > 3.5).unwrap_or(false))
                            .unwrap_or(false);
                        self.press_pos = None;
                        if !moved && !word_selected {
                            if let Some(sel) = self.selection {
                                let range = sel.range();
                                if range.is_empty() {
                                    clicked_link = link_at(layout, pointer, origin);
                                }
                            }
                        }
                    }
                } else if released {
                    if double_click {
                        if let Some(pos) = pointer {
                            if let Some((_, offset)) = hit_test(layout, pos, origin) {
                                let range = word_range(&self.source, offset);
                                if !range.is_empty() {
                                    self.selection = Some(TextSelection {
                                        anchor: range.start,
                                        focus: range.end,
                                    });
                                }
                            }
                        }
                    } else if let Some(sel) = self.selection {
                        if sel.is_empty() {
                            clicked_link = link_at(layout, pointer, origin);
                        }
                    }
                }

                // ---- backgrounds: quote bands ----
                for band in &layout.quotes {
                    let x = layout.left + band.depth as f32 * 18.0 - 14.0;
                    let rect = Rect::from_min_max(
                        origin + vec2(x, band.top),
                        origin + vec2(layout.left + layout.column_width - 8.0, band.bottom),
                    );
                    if ui.is_rect_visible(rect) {
                        ui.painter().rect_filled(
                            rect,
                            6.0,
                            colors_ref.quote_background,
                        );
                        let border_x = if self.config.resolved_direction.is_rtl() {
                            rect.right() - 3.0
                        } else {
                            rect.left()
                        };
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                egui::pos2(border_x, rect.top() + 6.0),
                                vec2(3.0, rect.height() - 12.0),
                            ),
                            1.5,
                            colors_ref.quote_border,
                        );
                        if let Some(kind) = band.kind {
                            ui.painter().text(
                                egui::pos2(
                                    if self.config.resolved_direction.is_rtl() {
                                        rect.right() - 12.0
                                    } else {
                                        rect.left() + 12.0
                                    },
                                    rect.top() + 7.0,
                                ),
                                if self.config.resolved_direction.is_rtl() {
                                    Align2::RIGHT_TOP
                                } else {
                                    Align2::LEFT_TOP
                                },
                                kind.label().to_uppercase(),
                                egui::FontId::proportional(9.5),
                                colors_ref.muted,
                            );
                        }
                    }
                }

                // ---- elements ----
                for (element_index, element) in layout.elements.iter().enumerate() {
                    let rect = Rect::from_min_size(
                        origin + vec2(0.0, element.top),
                        vec2(layout.column_width, element.height),
                    );
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    match &element.kind {
                        ElementKind::Heading { shaped, rt, level, .. } => {
                            self.paint_text_element(
                                ui,
                                engine,
                                colors_ref,
                                origin,
                                element,
                                rt,
                                shaped,
                                if *level <= 2 {
                                    colors_ref.heading
                                } else {
                                    colors_ref.heading
                                },
                                layout,
                                element_index,
                            );
                            if *level == 1 {
                                let y = element.top + shaped.height + 4.0;
                                ui.painter().rect_filled(
                                    Rect::from_min_size(
                                        origin + vec2(element.left, y),
                                        vec2(layout.column_width - element.left + layout.left - 20.0, 1.0),
                                    ),
                                    0.0,
                                    colors_ref.rule,
                                );
                            }
                        }
                        ElementKind::Paragraph { shaped, rt } => {
                            self.paint_text_element(
                                ui,
                                engine,
                                colors_ref,
                                origin,
                                element,
                                rt,
                                shaped,
                                colors_ref.text,
                                layout,
                                element_index,
                            );
                        }
                        ElementKind::ListItem {
                            shaped,
                            rt,
                            marker,
                            marker_left,
                            ..
                        } => {
                            self.paint_text_element(
                                ui,
                                engine,
                                colors_ref,
                                origin,
                                element,
                                rt,
                                shaped,
                                colors_ref.text,
                                layout,
                                element_index,
                            );
                            let marker_pos = origin + vec2(*marker_left, element.top);
                            match marker {
                                ListMarker::Bullet => {
                                    ui.painter().circle_filled(
                                        marker_pos + vec2(4.0, shaped.lines.first().map(|l| l.height * 0.45).unwrap_or(8.0)),
                                        2.6,
                                        colors_ref.muted,
                                    );
                                }
                                ListMarker::Ordered(n) => {
                                    let rtl = self.config.resolved_direction.is_rtl();
                                    ui.painter().text(
                                        marker_pos + vec2(if rtl { 24.0 } else { 0.0 }, 1.0),
                                        if rtl { Align2::RIGHT_TOP } else { Align2::LEFT_TOP },
                                        format!("{n}."),
                                        egui::FontId::proportional(12.0),
                                        colors_ref.muted,
                                    );
                                }
                                ListMarker::Task(checked) => {
                                    let box_rect = Rect::from_min_size(
                                        marker_pos + vec2(0.0, 2.0),
                                        Vec2::splat(14.0),
                                    );
                                    ui.painter().rect_stroke(
                                        box_rect,
                                        3.0,
                                        Stroke::new(1.0, if *checked { colors_ref.link } else { colors_ref.muted }),
                                        StrokeKind::Inside,
                                    );
                                    if *checked {
                                        ui.painter().rect_filled(box_rect, 3.0, colors_ref.link);
                                        draw_icon(
                                            ui.painter(),
                                            Icon::Check,
                                            box_rect.shrink(2.0),
                                            colors_ref.background,
                                        );
                                    }
                                }
                            }
                        }
                        ElementKind::Code {
                            shaped,
                            code,
                            language,
                            tokens,
                            header_height,
                        } => {
                            let code_rect = Rect::from_min_size(
                                origin + vec2(element.left, element.top),
                                vec2(
                                    layout.column_width - element.left + layout.left - 8.0,
                                    element.height - 10.0,
                                ),
                            );
                            ui.painter().rect_filled(
                                code_rect,
                                8.0,
                                colors_ref.code_background,
                            );
                            ui.painter().rect_stroke(
                                code_rect,
                                8.0,
                                Stroke::new(1.0, colors_ref.code_border),
                                StrokeKind::Inside,
                            );
                            let header_rect = Rect::from_min_size(
                                code_rect.min,
                                vec2(code_rect.width(), *header_height),
                            );
                            ui.painter().rect_filled(
                                header_rect,
                                egui::CornerRadius {
                                    nw: 8,
                                    ne: 8,
                                    sw: 0,
                                    se: 0,
                                },
                                colors_ref.table_header_background,
                            );
                            let lang = language.clone().unwrap_or_else(|| "text".into());
                            ui.painter().text(
                                header_rect.min + vec2(12.0, 8.0),
                                Align2::LEFT_TOP,
                                lang.to_lowercase(),
                                egui::FontId::monospace(10.5),
                                colors_ref.muted,
                            );
                            let copy_rect = Rect::from_min_size(
                                egui::pos2(header_rect.right() - 32.0, header_rect.top() + 4.0),
                                Vec2::splat(22.0),
                            );
                            let copy_resp = ui
                                .interact(
                                    copy_rect,
                                    Id::new(("copy-code", self.doc.id.clone(), element_index)),
                                    Sense::click(),
                                )
                                .on_hover_text(crate::i18n::t("Copy code"));
                            draw_icon(ui.painter(), Icon::Copy, copy_rect.shrink(4.0), colors_ref.muted);
                            if copy_resp.clicked() {
                                copy_code = Some(code.clone());
                            }
                            let mut decors: Vec<daftar_text::RangePaint> = Vec::new();
                            for (range, color) in tokens {
                                decors.push(daftar_text::RangePaint {
                                    range: range.clone(),
                                    background: None,
                                    text_color: Some(*color),
                                    underline: None,
                                });
                            }
                            let text_colors = daftar_text::TextColors {
                                text: colors_ref.code_text,
                                link: colors_ref.link,
                                code: colors_ref.code_text,
                                code_background: None,
                                selection: Color32::TRANSPARENT,
                            };
                            engine.paint(
                                ui.ctx(),
                                ui.painter(),
                                shaped,
                                origin + vec2(element.left + 12.0, element.top + header_height + 8.0),
                                &text_colors,
                                None,
                                &decors,
                            );
                        }
                        ElementKind::Table { layout: table } => {
                            self.paint_table(
                                ui,
                                engine,
                                colors_ref,
                                element,
                                layout,
                                table,
                                origin,
                            );
                        }
                        ElementKind::Image {
                            path,
                            alt,
                            width,
                            height,
                            error,
                        } => {
                            let x = origin.x + element.left + ((layout.left + layout.column_width - 8.0 - element.left) - width) * 0.5;
                            let y = origin.y + element.top;
                            let img_rect = Rect::from_min_size(egui::pos2(x, y), vec2(*width, *height));
                            if let Some(error) = error {
                                ui.painter().rect_filled(
                                    img_rect,
                                    6.0,
                                    colors_ref.code_background,
                                );
                                draw_icon(
                                    ui.painter(),
                                    Icon::Image,
                                    Rect::from_center_size(img_rect.center(), Vec2::splat(22.0)),
                                    colors_ref.muted,
                                );
                                ui.painter().text(
                                    img_rect.center() + vec2(0.0, 24.0),
                                    Align2::CENTER_TOP,
                                    error,
                                    egui::FontId::proportional(11.0),
                                    colors_ref.muted,
                                );
                            } else {
                                let texture = ensure_image_texture(
                                    ui.ctx(),
                                    &mut self.image_textures,
                                    path,
                                );
                                if let Some(texture) = texture {
                                    let uv = Rect::from_min_max(
                                        egui::pos2(0.0, 0.0),
                                        egui::pos2(1.0, 1.0),
                                    );
                                    ui.painter().image(
                                        texture.texture.id(),
                                        img_rect,
                                        uv,
                                        Color32::WHITE,
                                    );
                                    ui.painter().rect_stroke(
                                        img_rect,
                                        4.0,
                                        Stroke::new(1.0, colors_ref.table_border),
                                        StrokeKind::Inside,
                                    );
                                }
                            }
                            if !alt.is_empty() {
                                ui.painter().text(
                                    egui::pos2(x, y + height + 4.0),
                                    Align2::LEFT_TOP,
                                    alt,
                                    egui::FontId::proportional(10.5),
                                    colors_ref.muted,
                                );
                            }
                        }
                        ElementKind::Diagram {
                            key,
                            width,
                            height,
                            error,
                        } => {
                            let x = origin.x + element.left + ((layout.left + layout.column_width - 8.0 - element.left) - width) * 0.5;
                            let y = origin.y + element.top;
                            let d_rect = Rect::from_min_size(egui::pos2(x, y), vec2(*width, *height));
                            if let Some(error) = error {
                                ui.painter().rect_filled(
                                    d_rect,
                                    6.0,
                                    colors_ref.danger_subtle_placeholder(),
                                );
                                draw_icon(
                                    ui.painter(),
                                    Icon::Warning,
                                    Rect::from_min_size(d_rect.min + vec2(10.0, 10.0), Vec2::splat(16.0)),
                                    colors_ref.text,
                                );
                                ui.painter().text(
                                    d_rect.min + vec2(34.0, 11.0),
                                    Align2::LEFT_TOP,
                                    crate::i18n::t("Diagram could not be rendered"),
                                    egui::FontId::proportional(12.0),
                                    colors_ref.text,
                                );
                                ui.painter().text(
                                    d_rect.min + vec2(34.0, 30.0),
                                    Align2::LEFT_TOP,
                                    error,
                                    egui::FontId::proportional(10.5),
                                    colors_ref.muted,
                                );
                            } else {
                                if let Some(tex) = ensure_diagram_texture(
                                    ui.ctx(),
                                    engine,
                                    &mut self.diagrams,
                                    *key,
                                ) {
                                    ui.painter().image(
                                        tex,
                                        d_rect,
                                        Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                let open_rect = Rect::from_min_size(
                                    egui::pos2(d_rect.right() - 30.0, d_rect.top() + 6.0),
                                    Vec2::splat(24.0),
                                );
                                let resp = ui
                                    .interact(
                                        d_rect,
                                        Id::new(("diagram", self.doc.id.clone(), element_index)),
                                        Sense::click(),
                                    )
                                    .on_hover_text(crate::i18n::t("Open diagram viewer"));
                                let p = ui.painter();
                                p.rect_filled(open_rect, 4.0, colors_ref.background.gamma_multiply(0.85));
                                draw_icon(p, Icon::ZoomIn, open_rect.shrink(4.0), colors_ref.muted);
                                if resp.clicked() {
                                    open_diagram = Some(*key);
                                }
                            }
                        }
                        ElementKind::Math { shaped, .. } => {
                            let text_colors = daftar_text::TextColors {
                                text: colors_ref.text,
                                link: colors_ref.link,
                                code: colors_ref.text,
                                code_background: None,
                                selection: Color32::TRANSPARENT,
                            };
                            engine.paint(
                                ui.ctx(),
                                ui.painter(),
                                shaped,
                                origin + vec2(element.left, element.top + 6.0),
                                &text_colors,
                                None,
                                &[],
                            );
                        }
                        ElementKind::Html { shaped, .. } => {
                            let text_colors = daftar_text::TextColors {
                                text: colors_ref.muted,
                                link: colors_ref.link,
                                code: colors_ref.muted,
                                code_background: None,
                                selection: Color32::TRANSPARENT,
                            };
                            engine.paint(
                                ui.ctx(),
                                ui.painter(),
                                shaped,
                                origin + vec2(element.left, element.top),
                                &text_colors,
                                None,
                                &[],
                            );
                        }
                        ElementKind::Rule => {
                            let y = element.top + 12.0;
                            ui.painter().rect_filled(
                                Rect::from_min_size(
                                    origin + vec2(element.left, y),
                                    vec2(
                                        (layout.column_width - element.left + layout.left - 8.0).max(1.0),
                                        1.0,
                                    ),
                                ),
                                0.0,
                                colors_ref.rule,
                            );
                        }
                        ElementKind::FootnoteLabel { shaped, .. } => {
                            let text_colors = daftar_text::TextColors {
                                text: colors_ref.muted,
                                link: colors_ref.link,
                                code: colors_ref.muted,
                                code_background: None,
                                selection: Color32::TRANSPARENT,
                            };
                            engine.paint(
                                ui.ctx(),
                                ui.painter(),
                                shaped,
                                origin + vec2(element.left, element.top),
                                &text_colors,
                                None,
                                &[],
                            );
                        }
                        ElementKind::Empty => {}
                    }
                }

                // Selection toolbar screen position.
                if let Some(selection) = self.selection {
                    if !selection.is_empty() && !self.selecting {
                        if let Some(rect) = selection_screen_rect(&self.layout, selection, origin) {
                            context_screen_pos = Some(egui::pos2(
                                rect.center().x.clamp(160.0, ui.ctx().content_rect().right() - 160.0),
                                rect.top() - 8.0,
                            ));
                        }
                    }
                }

                if let Some(pos) = forced_toolbar {
                    context_screen_pos = Some(pos);
                }
                scroll_output = Some(ui.min_rect());
                let _ = response;
            });
        self.scroll_offset = output.state.offset().y;
        self.save_progress(store);
        let _ = scroll_output;

        if let Some(code) = copy_code {
            ui.ctx().copy_text(code);
            actions.push(ReaderAction::Notice(crate::i18n::t("Code copied to clipboard.").into(), ToastKind::Info));
        }

        if let Some(dest) = clicked_link {
            if dest.starts_with('#') {
                let id = dest.trim_start_matches('#');
                let id = id.strip_prefix("fn-").unwrap_or(id);
                if !self.scroll_to_heading(id) {
                    actions.push(ReaderAction::Notice(
                        crate::i18n::tf("No heading or footnote with anchor `{}`.", &[id]),
                        ToastKind::Warning,
                    ));
                }
            } else if dest.starts_with("http://")
                || dest.starts_with("https://")
                || dest.starts_with("mailto:")
            {
                actions.push(ReaderAction::OpenExternal(dest));
            } else if dest.ends_with(".md") || dest.ends_with(".markdown") {
                let base = PathBuf::from(&self.doc.rel_path)
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();
                let joined = base.join(dest);
                let rel = daftar_core::model::native_to_rel(&joined);
                let library = self.library_root.clone().unwrap_or_default();
                if let Ok(Some(doc)) = store.document_by_rel_path(&self.doc.library_id, &rel) {
                    let _ = library;
                    actions.push(ReaderAction::OpenDocument(doc.id));
                } else {
                    actions.push(ReaderAction::Notice(
                        crate::i18n::tf("Linked document `{}` is not in this library.", &[&rel]),
                        ToastKind::Warning,
                    ));
                }
            } else {
                actions.push(ReaderAction::Notice(
                    crate::i18n::tf("Unsupported link target: {}", &[&dest]),
                    ToastKind::Warning,
                ));
            }
            self.selection = None;
        }

        // Context toolbar.
        if let (Some(pos), Some(selection)) = (context_screen_pos, self.selection) {
            if !selection.is_empty() {
                self.context_toolbar(
                    ui.ctx(),
                    engine,
                    palette,
                    settings,
                    store,
                    pos,
                    selection,
                    &mut actions,
                );
            }
        }

        if let Some(key) = open_diagram {
            self.diagram_viewer = Some(DiagramViewer {
                key,
                zoom: 1.0,
                pan: Vec2::ZERO,
                title: crate::i18n::t("Mermaid diagram").into(),
            });
        }
        self.show_diagram_viewer(ui.ctx(), engine, palette, &mut actions);
        self.show_note_dialog(ui.ctx(), palette, store, &mut actions);
        self.show_highlight_menu(ui.ctx(), palette, settings, store, &mut actions);
        actions
    }

    fn paint_text_element(
        &self,
        ui: &mut Ui,
        engine: &mut TextEngine,
        colors: &ReaderColors,
        origin: egui::Pos2,
        element: &Element,
        rt: &daftar_core::markdown::RichText,
        shaped: &daftar_text::ShapedText,
        color: Color32,
        layout: &ReaderLayout,
        element_index: usize,
    ) {
        let mut decors: Vec<daftar_text::RangePaint> = Vec::new();
        for highlight in &self.highlights {
            let range = highlight.start_offset.max(0) as usize..highlight.end_offset.max(0) as usize;
            if range.start >= element.source_range.end || range.end <= element.source_range.start {
                continue;
            }
            if let Some(display) = display_range_for_source(rt, range.clone()) {
                let base = parse_hex_color(&highlight.color).unwrap_or(colors.highlight_palette.first().copied().unwrap_or(colors.selection));
                decors.push(daftar_text::RangePaint {
                    range: display,
                    background: Some(Color32::from_rgba_unmultiplied(
                        base.r(),
                        base.g(),
                        base.b(),
                        if colors.dark { 120 } else { 90 },
                    )),
                    text_color: None,
                    underline: Some(base.gamma_multiply(0.7)),
                });
            }
        }
        // Persistent bookmark markers are shown in the side panel; highlights are painted here.
        let selection_display = self.selection.and_then(|selection| {
            let range = selection.range();
            if range.start >= element.source_range.end || range.end <= element.source_range.start {
                return None;
            }
            display_range_for_source(rt, range)
        });
        let text_colors = daftar_text::TextColors {
            text: color,
            link: colors.link,
            code: colors.text,
            code_background: Some(colors.code_background),
            selection: Color32::from_rgba_unmultiplied(
                colors.selection.r(),
                colors.selection.g(),
                colors.selection.b(),
                150,
            ),
        };
        engine.paint(
            ui.ctx(),
            ui.painter(),
            shaped,
            origin + egui::vec2(element.text_left, element.top),
            &text_colors,
            selection_display,
            &decors,
        );
        let _ = (layout, element_index);
        // Footnote reference superscripts are already rendered as links by the shaper.
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_table(
        &self,
        ui: &mut Ui,
        engine: &mut TextEngine,
        colors: &ReaderColors,
        element: &Element,
        layout: &ReaderLayout,
        table: &TableLayout,
        origin: Pos2,
    ) {
        let x = origin.x + element.left;
        let y = origin.y + element.top;
        let width: f32 = table.col_widths.iter().sum();
        let total_h = table.total_height;
        let rect = Rect::from_min_size(egui::pos2(x, y), vec2(width, total_h));
        ui.painter()
            .rect_filled(rect, 6.0, colors.table_header_background);
        ui.painter().rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, colors.table_border),
            StrokeKind::Inside,
        );
        // Header background.
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(width, table.header_height)),
            egui::CornerRadius {
                nw: 6,
                ne: 6,
                sw: 0,
                se: 0,
            },
            colors.table_header_background,
        );
        let text_colors = daftar_text::TextColors {
            text: colors.text,
            link: colors.link,
            code: colors.text,
            code_background: None,
            selection: Color32::TRANSPARENT,
        };
        let mut cx = x;
        for (col, cell) in table.header.iter().enumerate() {
            engine.paint(
                ui.ctx(),
                ui.painter(),
                cell,
                egui::pos2(cx + 10.0, y + 7.0),
                &text_colors,
                None,
                &[],
            );
            cx += table.col_widths[col];
        }
        let mut row_y = y + table.header_height;
        for (row_index, row) in table.rows.iter().enumerate() {
            let h = table.row_heights[row_index];
            if row_index % 2 == 1 {
                ui.painter().rect_filled(
                    Rect::from_min_size(egui::pos2(x, row_y), vec2(width, h)),
                    0.0,
                    colors.quote_background,
                );
            }
            let mut cx = x;
            for (col, cell) in row.iter().enumerate() {
                engine.paint(
                    ui.ctx(),
                    ui.painter(),
                    cell,
                    egui::pos2(cx + 10.0, row_y + 6.0),
                    &text_colors,
                    None,
                    &[],
                );
                cx += table.col_widths[col];
            }
            ui.painter().rect_filled(
                Rect::from_min_size(egui::pos2(x, row_y), vec2(width, 1.0)),
                0.0,
                colors.table_border,
            );
            row_y += h;
        }
        // Column separators.
        let mut cx = x;
        for (col, w) in table.col_widths.iter().enumerate() {
            cx += w;
            if col + 1 < table.col_widths.len() {
                ui.painter().rect_filled(
                    Rect::from_min_size(egui::pos2(cx, y), vec2(1.0, total_h)),
                    0.0,
                    colors.table_border,
                );
            }
        }
        let _ = (layout, element);
    }

    // ------------------------------------------------------------------------------------
    // Context toolbar and dialogs
    // ------------------------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn context_toolbar(
        &mut self,
        ctx: &egui::Context,
        engine: &mut TextEngine,
        palette: &Palette,
        settings: &AppSettings,
        store: &Store,
        pos: Pos2,
        selection: TextSelection,
        actions: &mut Vec<ReaderAction>,
    ) {
        let range = selection.range();
        if range.is_empty() {
            return;
        }
        let quote = selected_display_text(&self.layout, selection)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| self.source[range.clone()].to_string());
        let source_quote = self.source[range.clone()].to_string();
        let anchor = TextAnchor::create(&self.source, range.clone());
        let block_index = element_for_offset(&self.layout, range.start)
            .map(|e| e.block_index as i64);
        let colors = palette.reader_theme(&settings.theme());
        let pos = egui::pos2(pos.x - 130.0, (pos.y - 44.0).max(8.0));
        let mut create_highlight: Option<Color32> = None;
        let mut create_bookmark = false;
        let mut create_flashcard = false;
        let mut add_note = false;
        let mut copy = false;
        let mut export = false;

        egui::Area::new(Id::new("selection-toolbar"))
            .order(Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(6))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 14,
                        spread: 0,
                        color: palette.shadow,
                    })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for color in colors.highlight_palette.iter().take(5) {
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::splat(22.0),
                                    Sense::click(),
                                );
                                ui.painter().circle_filled(rect.center(), 7.0, *color);
                                ui.painter().circle_stroke(
                                    rect.center(),
                                    7.0,
                                    Stroke::new(1.0, palette.border_strong),
                                );
                                if response.clicked() {
                                    create_highlight = Some(*color);
                                }
                                if response.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            }
                            widgets::toolbar_divider(ui, palette);
                            if widgets::icon_button_sized(ui, palette, Icon::Copy, "Copy selection", false, 26.0).clicked() {
                                copy = true;
                            }
                            if widgets::icon_button_sized(ui, palette, Icon::Bookmark, "Bookmark selection", false, 26.0).clicked() {
                                create_bookmark = true;
                            }
                            if widgets::icon_button_sized(ui, palette, Icon::Flashcards, "Create flashcard", false, 26.0).clicked() {
                                create_flashcard = true;
                            }
                            if widgets::icon_button_sized(ui, palette, Icon::Note, "Add note", false, 26.0).clicked() {
                                add_note = true;
                            }
                            if widgets::icon_button_sized(ui, palette, Icon::Image, "Export as image", false, 26.0).clicked() {
                                export = true;
                            }
                        });
                    });
            });

        if let Some(color) = create_highlight {
            let mut highlight = Highlight::new(self.doc.id.clone(), source_quote.clone(), hex_of(color));
            highlight.start_offset = range.start as i64;
            highlight.end_offset = range.end as i64;
            highlight.prefix = anchor.prefix.clone();
            highlight.suffix = anchor.suffix.clone();
            highlight.block_index = block_index;
            if store.upsert_highlight(&highlight).is_ok() {
                self.highlights.push(highlight);
                self.selection = None;
                actions.push(ReaderAction::Notice(crate::i18n::t("Highlight saved.").into(), ToastKind::Success));
            } else {
                actions.push(ReaderAction::Notice(crate::i18n::t("Could not save the highlight.").into(), ToastKind::Error));
            }
        } else if create_bookmark {
            let mut bookmark = Bookmark::new(self.doc.id.clone(), truncate_chars(&quote, 60));
            bookmark.quote = source_quote.clone();
            bookmark.prefix = anchor.prefix.clone();
            bookmark.suffix = anchor.suffix.clone();
            bookmark.start_offset = range.start as i64;
            bookmark.end_offset = range.end as i64;
            bookmark.block_index = block_index;
            if store.upsert_bookmark(&bookmark).is_ok() {
                self.bookmarks.push(bookmark);
                self.selection = None;
                actions.push(ReaderAction::Notice(crate::i18n::t("Bookmark saved.").into(), ToastKind::Success));
            }
        } else if create_flashcard {
            actions.push(ReaderAction::CreateFlashcard {
                document_id: self.doc.id.clone(),
                quote,
            });
            self.selection = None;
        } else if add_note {
            self.note_dialog = Some(NoteDialog {
                selection,
                text: String::new(),
                highlight_id: None,
            });
        } else if copy {
            ctx.copy_text(quote);
            actions.push(ReaderAction::Notice(crate::i18n::t("Selection copied.").into(), ToastKind::Info));
        } else if export {
            actions.push(ReaderAction::ExportQuoteImage {
                quote,
                source: self.doc.title.clone(),
            });
            self.selection = None;
        }
        let _ = engine;
    }

    fn show_note_dialog(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        store: &Store,
        actions: &mut Vec<ReaderAction>,
    ) {
        let Some(dialog) = self.note_dialog.as_mut() else {
            return;
        };
        let mut close = false;
        let mut save = false;
        let editing = dialog.highlight_id.is_some();
        egui::Window::new(if editing {
            crate::i18n::t("Edit note")
        } else {
            crate::i18n::t("Add note")
        })
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(crate::i18n::t("A highlight with your note will be saved at this selection."))
                        .size(12.0)
                        .color(palette.text_secondary),
                );
                ui.add_space(6.0);
                ui.add(
                    egui::TextEdit::multiline(&mut dialog.text)
                        .desired_rows(4)
                        .desired_width(380.0)
                        .hint_text(crate::i18n::t("Your note…")),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if widgets::button(
                        ui,
                        palette,
                        "Save note",
                        Some(Icon::Check),
                        ButtonVariant::Primary,
                    )
                    .clicked()
                    {
                        save = true;
                    }
                    if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                        close = true;
                    }
                });
            });
        if save {
            let text = dialog.text.clone();
            if let Some(id) = dialog.highlight_id.clone() {
                if let Some(existing) = self.highlights.iter_mut().find(|h| h.id == id) {
                    existing.note = text;
                    let _ = store.upsert_highlight(existing);
                }
                self.selection = None;
                close = true;
                actions.push(ReaderAction::Notice(crate::i18n::t("Note saved.").into(), ToastKind::Success));
            } else {
                let range = dialog.selection.range();
                let quote = self.source[range.clone()].to_string();
                let anchor = TextAnchor::create(&self.source, range.clone());
                let mut highlight = Highlight::new(
                    self.doc.id.clone(),
                    quote,
                    hex_of(palette.warning),
                );
                highlight.start_offset = range.start as i64;
                highlight.end_offset = range.end as i64;
                highlight.prefix = anchor.prefix;
                highlight.suffix = anchor.suffix;
                highlight.note = text;
                let _ = store.upsert_highlight(&highlight);
                self.highlights.push(highlight);
                self.selection = None;
                close = true;
                actions.push(ReaderAction::Notice(crate::i18n::t("Note saved.").into(), ToastKind::Success));
            }
        }
        if close {
            self.note_dialog = None;
        }
    }

    /// Right-click menu for an existing highlight: copy, annotate, recolor or remove it.
    fn show_highlight_menu(
        &mut self,
        ctx: &egui::Context,
        palette: &Palette,
        settings: &AppSettings,
        store: &Store,
        actions: &mut Vec<ReaderAction>,
    ) {
        let Some(menu) = self.highlight_menu.clone() else {
            return;
        };
        let Some(index) = self.highlights.iter().position(|h| h.id == menu.id) else {
            self.highlight_menu = None;
            return;
        };
        let colors = palette.reader_theme(&settings.theme());
        let fresh = self.highlight_menu.as_ref().is_some_and(|m| m.fresh);
        let has_note = !self.highlights[index].note.trim().is_empty();
        let quote = plain_text(&self.highlights[index].quote);
        let mut recolor: Option<Color32> = None;
        let mut add_note = false;
        let mut copy = false;
        let mut remove = false;
        let mut dismiss = false;
        log::info!(
            "DBG menu open pos={:?} fresh={} pointer={:?} layer_at={:?} area_layer={:?}",
            menu.pos,
            fresh,
            ctx.input(|i| i.pointer.interact_pos()),
            ctx.input(|i| i.pointer.interact_pos()).map(|p| ctx.layer_id_at(p)),
            Id::new("highlight-menu")
        );

        let menu_rect = egui::Area::new(Id::new("highlight-menu"))
            .order(Order::Foreground)
            .fixed_pos(menu.pos)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(palette.surface)
                    .stroke(Stroke::new(1.0, palette.border))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(6))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 14,
                        spread: 0,
                        color: palette.shadow,
                    })
                    .show(ui, |ui| {
                        ui.set_max_width(210.0);
                        ui.horizontal(|ui| {
                            for color in colors.highlight_palette.iter().take(5) {
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                                ui.painter().circle_filled(rect.center(), 7.0, *color);
                                ui.painter().circle_stroke(
                                    rect.center(),
                                    7.0,
                                    Stroke::new(1.0, palette.border_strong),
                                );
                                log::info!("DBG swatch {:?} hovered={} clicked={}", color, response.hovered(), response.clicked());
                                if response.clicked() {
                                    recolor = Some(*color);
                                }
                                if response.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                            }
                        });
                        widgets::toolbar_divider(ui, palette);
                        if ui
                            .button(if has_note {
                                crate::i18n::t("Edit note")
                            } else {
                                crate::i18n::t("Add note")
                            })
                            .clicked()
                        {
                            add_note = true;
                        }
                        let copy_r = ui.button(crate::i18n::t("Copy quote"));
                        log::info!("DBG copy btn hovered={} clicked={}", copy_r.hovered(), copy_r.clicked());
                        if copy_r.clicked() {
                            copy = true;
                        }
                        let remove_r = ui.button(crate::i18n::t("Remove highlight"));
                        log::info!("DBG remove btn hovered={} clicked={}", remove_r.hovered(), remove_r.clicked());
                        if remove_r.clicked() {
                            remove = true;
                        }
                    })
                    .response
                    .rect
            })
            .inner;

        log::info!("DBG actions recolor={:?} note={} copy={} remove={} menu_rect={:?}", recolor, add_note, copy, remove, menu_rect);
        if let Some(color) = recolor {
            let highlight = &mut self.highlights[index];
            highlight.color = hex_of(color);
            if store.upsert_highlight(highlight).is_ok() {
                actions.push(ReaderAction::Notice(crate::i18n::t("Highlight color updated.").into(), ToastKind::Success));
            }
            dismiss = true;
        } else if add_note {
            let highlight = &self.highlights[index];
            self.note_dialog = Some(NoteDialog {
                selection: TextSelection {
                    anchor: highlight.start_offset.max(0) as usize,
                    focus: highlight.end_offset.max(0) as usize,
                },
                text: highlight.note.clone(),
                highlight_id: Some(highlight.id.clone()),
            });
            dismiss = true;
        } else if copy {
            ctx.copy_text(quote);
            actions.push(ReaderAction::Notice(crate::i18n::t("Quote copied.").into(), ToastKind::Info));
            dismiss = true;
        } else if remove {
            let id = self.highlights[index].id.clone();
            let _ = store.delete_highlight(&id);
            self.highlights.retain(|h| h.id != id);
            actions.push(ReaderAction::Notice(crate::i18n::t("Highlight removed.").into(), ToastKind::Info));
            dismiss = true;
        } else if !fresh {
            if let Some(pos) = ctx.input(|i| i.pointer.interact_pos()) {
                if ctx.input(|i| i.pointer.any_click())
                    && !menu_rect.contains(pos)
                {
                    dismiss = true;
                }
            }
        }
        if dismiss {
            self.highlight_menu = None;
        } else if fresh {
            if let Some(menu) = self.highlight_menu.as_mut() {
                menu.fresh = false;
            }
        }
    }

    fn show_diagram_viewer(
        &mut self,
        ctx: &egui::Context,
        engine: &mut TextEngine,
        palette: &Palette,
        actions: &mut Vec<ReaderAction>,
    ) {
        let Some(viewer) = self.diagram_viewer.as_mut() else {
            return;
        };
        let Some(entry) = self.diagrams.get(&viewer.key) else {
            self.diagram_viewer = None;
            return;
        };
        let Some(built) = entry.built.clone() else {
            self.diagram_viewer = None;
            return;
        };
        let texture = ensure_diagram_texture(ctx, engine, &mut self.diagrams, viewer.key);
        let mut close = false;
        let mut export = false;
        egui::Area::new(Id::new("diagram-viewer"))
            .order(Order::Foreground)
            .fixed_pos(ctx.content_rect().min)
            .show(ctx, |ui| {
                let screen = ctx.content_rect();
                ui.painter().rect_filled(screen, 0.0, palette.overlay);
                let panel = Rect::from_center_size(screen.center(), vec2(screen.width() * 0.92, screen.height() * 0.9));
                ui.painter().rect_filled(panel, 10.0, palette.surface);
                let inner = panel.shrink(14.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&viewer.title).size(14.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, palette, Icon::Close, "Close", false).clicked() {
                                close = true;
                            }
                            if widgets::button(ui, palette, "Export PNG", Some(Icon::Export), ButtonVariant::Secondary).clicked() {
                                export = true;
                            }
                            if widgets::icon_button(ui, palette, Icon::Refresh, "Reset view", false).clicked() {
                                viewer.zoom = 1.0;
                                viewer.pan = Vec2::ZERO;
                            }
                            if widgets::icon_button(ui, palette, Icon::ZoomIn, "Zoom in", false).clicked() {
                                viewer.zoom = (viewer.zoom * 1.2).min(6.0);
                            }
                            if widgets::icon_button(ui, palette, Icon::ZoomOut, "Zoom out", false).clicked() {
                                viewer.zoom = (viewer.zoom / 1.2).max(0.2);
                            }
                        });
                    });
                    ui.separator();
                    let view_size = ui.available_size();
                    let (canvas, response) = ui.allocate_exact_size(view_size, Sense::click_and_drag());
                    ui.painter().rect_filled(canvas, 4.0, palette.canvas);
                    if response.dragged() {
                        viewer.pan += response.drag_delta();
                    }
                    if response.hovered() {
                        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll.abs() > 0.0 {
                            viewer.zoom = (viewer.zoom * (1.0 + scroll * 0.0015)).clamp(0.2, 6.0);
                        }
                    }
                    if let Some(texture) = &texture {
                        let base = (canvas.width() / built.width).min(canvas.height() / built.height);
                        let size = vec2(built.width * base, built.height * base) * viewer.zoom;
                        let draw = Rect::from_center_size(canvas.center() + viewer.pan, size);
                        ui.painter().image(
                            *texture,
                            draw,
                            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                });
            });
        if export {
            actions.push(ReaderAction::ExportDiagram {
                key: viewer.key,
                title: viewer.title.clone(),
            });
        }
        if close {
            self.diagram_viewer = None;
        }
    }

    // ------------------------------------------------------------------------------------
    // Side panel
    // ------------------------------------------------------------------------------------

    pub fn show_side_panel(
        &mut self,
        ui: &mut Ui,
        engine: &mut TextEngine,
        palette: &Palette,
        settings: &AppSettings,
        store: &Store,
        actions: &mut Vec<ReaderAction>,
    ) {
        let colors = palette.reader_theme(&settings.theme());
        ui.horizontal(|ui| {
            if widgets::button(ui, palette, "Outline", None, if self.side_tab == SideTab::Outline { ButtonVariant::Secondary } else { ButtonVariant::Quiet }).clicked() {
                self.side_tab = SideTab::Outline;
            }
            if widgets::button(ui, palette, "Marks", None, if self.side_tab == SideTab::Marks { ButtonVariant::Secondary } else { ButtonVariant::Quiet }).clicked() {
                self.side_tab = SideTab::Marks;
            }
            if widgets::button(ui, palette, "Info", None, if self.side_tab == SideTab::Info { ButtonVariant::Secondary } else { ButtonVariant::Quiet }).clicked() {
                self.side_tab = SideTab::Info;
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("reader-side-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| match self.side_tab {
                SideTab::Outline => {
                    if self.parsed.toc.is_empty() {
                        widgets::hint(ui, palette, "This document has no headings.");
                    }
                    let toc = self.parsed.toc.clone();
                    for entry in toc {
                        ui.horizontal(|ui| {
                            ui.add_space((entry.level.saturating_sub(1) as f32) * 12.0);
                            let response = shaped_label(
                                ui,
                                engine,
                                &entry.title,
                                &ShapedStyle::body().with_size(12.5).with_weight(if entry.level == 1 { 600 } else { 400 }),
                                palette.text_secondary,
                                ui.available_width() - 8.0,
                                Sense::click(),
                            );
                            if response.clicked() {
                                self.pending_scroll = self
                                    .layout
                                    .headings
                                    .get(&entry.id)
                                    .map(|y| (y - 24.0).max(0.0));
                            }
                            if response.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                        });
                    }
                }
                SideTab::Marks => {
                    widgets::section_label(ui, palette, "Highlights");
                    if self.highlights.is_empty() {
                        widgets::hint(ui, palette, "Select text in the document to create a highlight.");
                    }
                    let mut delete: Option<String> = None;
                    for highlight in self.highlights.clone() {
                        let range = highlight.start_offset.max(0) as usize..highlight.end_offset.max(0) as usize;
                        let quote = plain_text(&highlight.quote);
                        let color = parse_hex_color(&highlight.color).unwrap_or(colors.highlight_palette.first().copied().unwrap_or(colors.selection));
                        let frame = egui::Frame::new()
                            .fill(palette.surface_sunken)
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::same(8));
                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (chip, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                ui.painter().circle_filled(chip.center(), 4.0, color);
                                let resp = shaped_label(
                                    ui,
                                    engine,
                                    &quote,
                                    &ShapedStyle::body().with_size(12.0),
                                    palette.text,
                                    ui.available_width() - 30.0,
                                    Sense::click(),
                                );
                                if resp.clicked() {
                                    self.pending_scroll = Some((element_for_offset_public(&self.layout, range.start).map(|e| e.top).unwrap_or(0.0) - 24.0).max(0.0));
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if widgets::icon_button_sized(ui, palette, Icon::Trash, "Delete highlight", false, 24.0).clicked() {
                                        delete = Some(highlight.id.clone());
                                    }
                                    if widgets::icon_button_sized(ui, palette, Icon::Flashcards, "Create flashcard", false, 24.0).clicked() {
                                        actions.push(ReaderAction::CreateFlashcard {
                                            document_id: self.doc.id.clone(),
                                            quote: quote.clone(),
                                        });
                                    }
                                });
                            });
                            if !highlight.note.trim().is_empty() {
                                shaped_label(
                                    ui,
                                    engine,
                                    &highlight.note,
                                    &ShapedStyle::small(),
                                    palette.text_secondary,
                                    ui.available_width(),
                                    Sense::hover(),
                                );
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if let Some(id) = delete {
                        let _ = store.delete_highlight(&id);
                        self.highlights.retain(|h| h.id != id);
                    }
                    ui.add_space(8.0);
                    widgets::section_label(ui, palette, "Bookmarks");
                    if self.bookmarks.is_empty() {
                        widgets::hint(ui, palette, "Select text and choose Bookmark.");
                    }
                    let mut delete_bookmark: Option<String> = None;
                    for bookmark in self.bookmarks.clone() {
                        ui.horizontal(|ui| {
                            let resp = shaped_label(
                                ui,
                                engine,
                                &bookmark.label,
                                &ShapedStyle::body().with_size(12.0),
                                palette.text,
                                ui.available_width() - 30.0,
                                Sense::click(),
                            );
                            if resp.clicked() {
                                self.pending_scroll = Some((element_for_offset_public(&self.layout, bookmark.start_offset.max(0) as usize).map(|e| e.top).unwrap_or(0.0) - 24.0).max(0.0));
                            }
                            if widgets::icon_button_sized(ui, palette, Icon::Trash, "Delete bookmark", false, 24.0).clicked() {
                                delete_bookmark = Some(bookmark.id.clone());
                            }
                        });
                    }
                    if let Some(id) = delete_bookmark {
                        let _ = store.delete_bookmark(&id);
                        self.bookmarks.retain(|b| b.id != id);
                    }
                    if !self.orphaned.is_empty() {
                        ui.add_space(8.0);
                        widgets::section_label(ui, palette, "Unresolved anchors");
                        widgets::hint(ui, palette, "Some anchors could not be relocated after the file changed. They are kept in the database and shown here for review.");
                    }
                }
                SideTab::Info => {
                    widgets::section_label(ui, palette, "Document");
                    shaped_label(ui, engine, &self.doc.title, &ShapedStyle::title(), palette.text, ui.available_width(), Sense::hover());
                    widgets::hint(ui, palette, &self.doc.rel_path);
                    ui.add_space(6.0);
                    ui.label(crate::i18n::tf("{} words · {} min read", &[&self.parsed.word_count.to_string(), &self.parsed.reading_minutes.to_string()]));
                    ui.label(crate::i18n::tf("Headings: {}", &[&self.parsed.toc.len().to_string()]));
                    ui.label(crate::i18n::tf("Highlights: {} · Bookmarks: {}", &[&self.highlights.len().to_string(), &self.bookmarks.len().to_string()]));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(crate::i18n::tf(
                            "Direction: {} · Progress: {:.0}%",
                            &[
                                if self.config.resolved_direction.is_rtl() { "RTL" } else { "LTR" },
                                &format!("{:.0}", self.progress() * 100.0),
                            ]
                        ))
                        .size(12.0)
                        .color(palette.text_secondary),
                    );
                    widgets::progress_bar(ui, palette, self.progress(), palette.accent, 5.0);
                }
            });
    }

    // Missing field for layout base path, filled on first show.
    #[allow(dead_code)]
    fn unused(&self) {}
}

// Public helper needed by the side panel.
fn element_for_offset_public(layout: &ReaderLayout, offset: usize) -> Option<&Element> {
    layout
        .elements
        .iter()
        .find(|e| e.source_range.start <= offset && offset <= e.source_range.end)
}

// ----------------------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------------------

fn hit_test(layout: &ReaderLayout, pos: Pos2, origin: Pos2) -> Option<(usize, usize)> {
    let pos = pos - origin;
    layout.elements.iter().enumerate().find_map(|(index, element)| {
        let (rt, shaped) = element.selectable_text()?;
        if pos.y < element.text_top() || pos.y > element.top + element.height {
            return None;
        }
        if pos.x < element.text_left || pos.x > element.text_left + shaped.width + 12.0 {
            return None;
        }
        let local = Pos2::new(pos.x - element.text_left, pos.y - element.text_top());
        let display = shaped.offset_at_point(local.x, local.y);
        let source = rt.source_offset_at(display);
        Some((index, source))
    })
}

fn hit_test_clamped(layout: &ReaderLayout, pos: Pos2, origin: Pos2) -> Option<(usize, usize)> {
    if let Some(hit) = hit_test(layout, pos, origin) {
        return Some(hit);
    }
    let pos = pos - origin;
    let candidates: Vec<(usize, &Element)> = layout
        .elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.selectable_text().is_some())
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let (index, element) = candidates
        .iter()
        .min_by(|(_, a), (_, b)| {
            let da = (a.top - pos.y).abs();
            let db = (b.top - pos.y).abs();
            da.total_cmp(&db)
        })
        .copied()?;
    let (rt, shaped) = element.selectable_text()?;
    let local_x = (pos.x - element.text_left).clamp(0.0, shaped.width.max(1.0));
    let local_y = (pos.y - element.top).clamp(0.0, element.height.max(1.0));
    let display = shaped.offset_at_point(local_x, local_y);
    Some((index, rt.source_offset_at(display)))
}

fn link_at(layout: &ReaderLayout, pos: Option<Pos2>, origin: Pos2) -> Option<String> {
    let pos = pos?;
    let (index, _) = hit_test(layout, pos, origin)?;
    let pos = pos - origin;
    let element = layout.elements.get(index)?;
    let (rt, shaped) = element.selectable_text()?;
    let local = Pos2::new(pos.x - element.text_left, pos.y - element.text_top());
    let display = shaped.offset_at_point(local.x, local.y);
    let style = rt.style_at(display.min(rt.text.len().saturating_sub(1)));
    style.link
}

fn selection_screen_rect(
    layout: &ReaderLayout,
    selection: TextSelection,
    origin: Pos2,
) -> Option<Rect> {
    let range = selection.range();
    let mut best: Option<Rect> = None;
    for element in &layout.elements {
        let Some((rt, shaped)) = element.selectable_text() else {
            continue;
        };
        if range.start >= element.source_range.end || range.end <= element.source_range.start {
            continue;
        }
        let Some(display) = display_range_for_source(rt, range.clone()) else {
            continue;
        };
        for (x, y, w, h) in shaped.ranges_to_rects(display) {
            let rect = Rect::from_min_size(
                origin + vec2(element.text_left + x, element.top + y),
                vec2(w, h),
            );
            best = Some(match best {
                Some(existing) => existing.union(rect),
                None => rect,
            });
        }
    }
    best
}

fn selected_display_text(layout: &ReaderLayout, selection: TextSelection) -> Option<String> {
    let range = selection.range();
    let mut out = String::new();
    for element in &layout.elements {
        let Some((rt, _)) = element.selectable_text() else {
            continue;
        };
        if range.start >= element.source_range.end || range.end <= element.source_range.start {
            continue;
        }
        if let Some(display) = display_range_for_source(rt, range.clone()) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&rt.text[display]);
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn display_range_for_source(
    rt: &daftar_core::markdown::RichText,
    source: std::ops::Range<usize>,
) -> Option<std::ops::Range<usize>> {
    if let Some(range) = rt.display_range_for_source(source.clone()) {
        return Some(range);
    }
    // Fallback for selections that include Markdown syntax markers: search for trimmed text.
    let trimmed = rt.text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let _ = source;
    rt.text.find(trimmed).map(|_| 0..rt.text.len())
}

fn word_range(source: &str, offset: usize) -> std::ops::Range<usize> {
    let offset = offset.min(source.len());
    let mut start = offset;
    while start > 0 {
        let ch = source[..start].chars().next_back().unwrap_or(' ');
        if ch.is_alphanumeric() || ch == '_' {
            start -= ch.len_utf8();
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < source.len() {
        let ch = source[end..].chars().next().unwrap_or(' ');
        if ch.is_alphanumeric() || ch == '_' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    if start == end {
        start..end
    } else {
        start..end
    }
}

fn element_for_offset(layout: &ReaderLayout, offset: usize) -> Option<&Element> {
    layout
        .elements
        .iter()
        .find(|e| e.source_range.start <= offset && offset <= e.source_range.end)
}

fn parse_hex_color(hex: &str) -> Option<Color32> {
    let c = daftar_core::settings::ReadingTheme::parse_color(hex)?;
    Some(Color32::from_rgb(c[0], c[1], c[2]))
}

fn hex_of(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}

fn ensure_image_texture<'a>(
    ctx: &egui::Context,
    cache: &'a mut HashMap<PathBuf, Option<ImageTexture>>,
    path: &Path,
) -> Option<&'a ImageTexture> {
    if !cache.contains_key(path) {
        let loaded = load_image(path, 2400).ok().and_then(|img| {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            let texture = ctx.load_texture(
                format!("reader-image-{}", path.display()),
                color,
                egui::TextureOptions::LINEAR,
            );
            Some(ImageTexture {
                texture,
                width: img.width,
                height: img.height,
            })
        });
        cache.insert(path.to_path_buf(), loaded);
    }
    cache.get(path).and_then(|v| v.as_ref())
}

fn ensure_diagram_texture(
    ctx: &egui::Context,
    engine: &mut TextEngine,
    cache: &mut DiagramCache,
    key: u64,
) -> Option<egui::TextureId> {
    let generation = {
        let entry = cache.get(&key)?;
        if let Some((_, handle)) = &entry.texture {
            return Some(handle.id());
        }
        key
    };
    let built = cache.get(&key)?.built.clone()?;
    let pixmap = built.scene.rasterize(engine, 2.0, None);
    let mut rgba = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 4);
    for pixel in pixmap.pixels() {
        let c = pixel.demultiply();
        rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        &rgba,
    );
    let handle = ctx.load_texture(
        format!("diagram-{generation}"),
        image,
        egui::TextureOptions::LINEAR,
    );
    let id = handle.id();
    if let Some(entry) = cache.get_mut(&key) {
        entry.texture = Some((generation, handle));
    }
    Some(id)
}

// The reader needs to remember the library root for images and relative links.
impl ReaderState {
    fn library_root(&self) -> PathBuf {
        self.library_root.clone().unwrap_or_default()
    }
}

impl ReaderConfig {
    pub fn from_settings(
        settings: &AppSettings,
        palette: &Palette,
        preference: Direction,
        resolved: daftar_core::direction::ResolvedDirection,
        doc: &MdDocument,
    ) -> Self {
        let typography = &settings.reading.typography;
        Self {
            family: typography.font_family.clone(),
            code_family: typography.code_font_family.clone(),
            font_size: typography.font_size,
            font_weight: typography.font_weight,
            line_height: typography.line_height,
            paragraph_spacing: typography.paragraph_spacing,
            letter_spacing: typography.letter_spacing,
            heading_spacing: typography.heading_spacing,
            reading_width: typography.reading_width,
            text_align: typography.text_align,
            code_font_size: typography.code_font_size,
            theme_id: settings.reading.theme_id.clone(),
            dark: palette.dark,
            doc_direction: preference,
            resolved_direction: resolved,
            max_diagram_height: settings.diagrams.max_height,
            fit_diagrams: settings.diagrams.fit_to_width,
            show_diagram_edge_labels: settings.diagrams.show_edge_labels,
        }
    }
}

// Field added separately to keep the initializer readable.
impl ReaderState {
    fn _doc_dir(&self) -> PathBuf {
        PathBuf::from(&self.doc.rel_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default()
    }
}

// Extension used by the diagram error card.
trait DangerSubtle {
    fn danger_subtle_placeholder(&self) -> Color32;
}
impl DangerSubtle for ReaderColors {
    fn danger_subtle_placeholder(&self) -> Color32 {
        if self.dark {
            Color32::from_rgb(0x3A, 0x1E, 0x21)
        } else {
            Color32::from_rgb(0xFD, 0xEC, 0xED)
        }
    }
}