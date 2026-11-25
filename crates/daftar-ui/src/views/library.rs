//! Knowledge library browser.

use std::collections::HashMap;

use daftar_core::model::{Category, Collection, Document, Id};
use egui::Ui;

use crate::app::DaftarApp;
use crate::theme::Palette;
use crate::Icon;

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub query: String,
    pub collection: Option<Id>,
    pub category: Option<Id>,
    pub tag: Option<String>,
    pub recent_only: bool,
    pub selected: Option<Id>,
    pub sort: SortMode,
    pub editing_tags: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Title,
    RecentlyOpened,
    Progress,
}

impl SortMode {
    fn label(self) -> &'static str {
        match self {
            SortMode::Title => "Title",
            SortMode::RecentlyOpened => "Recently opened",
            SortMode::Progress => "Reading progress",
        }
    }
}

impl LibraryState {
    pub fn sync(&mut self, docs: &[Document], collections: &[Collection], categories: &[Category]) {
        if let Some(id) = &self.collection {
            if !collections.iter().any(|c| &c.id == id) {
                self.collection = None;
            }
        }
        if let Some(id) = &self.category {
            if !categories.iter().any(|c| &c.id == id) {
                self.category = None;
            }
        }
        if let Some(selected) = &self.selected {
            if !docs.iter().any(|d| &d.id == selected) {
                self.selected = None;
            }
        }
    }

    fn filters_active(&self) -> bool {
        !self.query.trim().is_empty()
            || self.collection.is_some()
            || self.category.is_some()
            || self.tag.is_some()
            || self.recent_only
    }

    fn clear_filters(&mut self) {
        self.query.clear();
        self.collection = None;
        self.category = None;
        self.tag = None;
        self.recent_only = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavFilter {
    All,
    Recent,
    Collection,
    Category,
}

impl DaftarApp {
    pub(crate) fn show_library(&mut self, ui: &mut Ui, palette: &Palette) {
        use crate::widgets::{button, chip, empty_state, search_field, ButtonVariant};

        // Header.
        ui.add_space(10.0);
        ui.horizontal_wrapped(|ui| {
            ui.heading(crate::i18n::t("Library"));
            ui.add_space(8.0);
            if button(
                ui,
                palette,
                "Import files",
                Some(Icon::Import),
                ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.open_picker(
                    crate::app::PickerPurpose::ImportMarkdown,
                    crate::picker::PickerMode::OpenFiles,
                    crate::i18n::t("Import Markdown files"),
                    &["md", "markdown", "mdown"],
                    "",
                );
            }
            if button(
                ui,
                palette,
                "Paste Markdown",
                Some(Icon::Note),
                ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.paste_markdown = Some(crate::app::PasteMarkdownDialog::default());
            }
            if button(
                ui,
                palette,
                "Add folder",
                Some(Icon::Folder),
                ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.open_picker(
                    crate::app::PickerPurpose::AddFolder,
                    crate::picker::PickerMode::OpenFolder,
                    "Add folder",
                    &[],
                    "",
                );
            }
        });
        ui.label(
            egui::RichText::new(crate::i18n::tf(
                "{} documents · {} collections · {} categories · {} words indexed",
                &[
                    &self.stats.documents.to_string(),
                    &self.stats.collections.to_string(),
                    &self.stats.categories.to_string(),
                    &self.stats.total_words.to_string(),
                ],
            ))
            .size(11.5)
            .color(palette.text_secondary),
        );
        ui.add_space(8.0);

        // Toolbar: structural actions, sort order and the filter field.
        ui.horizontal(|ui| {
            // The action buttons come first so the field can size itself to the remaining
            // width; otherwise the field would claim the full row and push the buttons out.
            if button(
                ui,
                palette,
                "New collection",
                Some(Icon::Collections),
                ButtonVariant::Quiet,
            )
            .clicked()
            {
                self.prompt = Some(crate::app::TextPrompt {
                    title: "New collection".into(),
                    label: crate::i18n::t("Collection name").into(),
                    value: String::new(),
                    action: crate::app::PromptAction::NewCollection,
                });
            }
            if button(
                ui,
                palette,
                "New category",
                Some(Icon::Categories),
                ButtonVariant::Quiet,
            )
            .clicked()
            {
                self.prompt = Some(crate::app::TextPrompt {
                    title: "New category".into(),
                    label: crate::i18n::t("Category name").into(),
                    value: String::new(),
                    action: crate::app::PromptAction::NewCategory,
                });
            }
            ui.add_space(4.0);
            let sort = self.library_state.sort;
            let mut picked: Option<SortMode> = None;
            let combo = egui::ComboBox::from_id_salt("library-sort")
                .selected_text(
                    egui::RichText::new(crate::i18n::tf(
                        "{}: {}",
                        &[crate::i18n::t("Sort"), crate::i18n::t(sort.label())],
                    ))
                    .size(12.5),
                )
                .width(206.0)
                .show_ui(ui, |ui| {
                    for mode in [
                        SortMode::Title,
                        SortMode::RecentlyOpened,
                        SortMode::Progress,
                    ] {
                        if ui
                            .selectable_label(
                                sort == mode,
                                egui::RichText::new(crate::i18n::t(mode.label())).size(12.5),
                            )
                            .clicked()
                        {
                            picked = Some(mode);
                        }
                    }
                });
            combo.response.on_hover_text(crate::i18n::t("Sort order"));
            if let Some(mode) = picked {
                self.library_state.sort = mode;
            }
            search_field(
                ui,
                palette,
                &mut self.library_state.query,
                crate::i18n::t("Filter documents by title or tag…"),
            );
        });
        // Filters.
        ui.horizontal_wrapped(|ui| {
            if chip(
                ui,
                palette,
                crate::i18n::t("All"),
                self.library_state.collection.is_none()
                    && self.library_state.category.is_none()
                    && !self.library_state.recent_only,
            )
            .clicked()
            {
                self.library_state.collection = None;
                self.library_state.category = None;
                self.library_state.recent_only = false;
            }
            if chip(ui, palette, "Recent", self.library_state.recent_only).clicked() {
                self.library_state.recent_only = !self.library_state.recent_only;
            }
            for collection in self.collections.clone() {
                let selected =
                    self.library_state.collection.as_deref() == Some(collection.id.as_str());
                if chip(ui, palette, &collection.name, selected).clicked() {
                    self.library_state.collection = if selected {
                        None
                    } else {
                        Some(collection.id.clone())
                    };
                    self.library_state.category = None;
                }
            }
            for category in self.categories.clone() {
                let selected = self.library_state.category.as_deref() == Some(category.id.as_str());
                if chip(ui, palette, &category.name, selected).clicked() {
                    self.library_state.category = if selected {
                        None
                    } else {
                        Some(category.id.clone())
                    };
                    self.library_state.collection = None;
                }
            }
        });
        if !self.tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for tag in self.tags.iter().take(20).cloned().collect::<Vec<_>>() {
                    let selected = self.library_state.tag.as_deref() == Some(tag.name.as_str());
                    if chip(ui, palette, &tag.name, selected).clicked() {
                        self.library_state.tag = if selected {
                            None
                        } else {
                            Some(tag.name.clone())
                        };
                    }
                }
            });
        }
        ui.separator();
        let query = self.library_state.query.to_lowercase();
        let mut docs: Vec<Document> = self
            .docs
            .iter()
            .filter(|doc| {
                if let Some(collection) = &self.library_state.collection {
                    if doc.collection_id.as_deref() != Some(collection.as_str()) {
                        return false;
                    }
                }
                if let Some(category) = &self.library_state.category {
                    if doc.category_id.as_deref() != Some(category.as_str()) {
                        return false;
                    }
                }
                if self.library_state.recent_only && !self.recents.iter().any(|r| r.id == doc.id) {
                    return false;
                }
                if let Some(tag) = &self.library_state.tag {
                    let tags = self.doc_tags.get(&doc.id);
                    if !tags.is_some_and(|t| t.iter().any(|x| x.eq_ignore_ascii_case(tag))) {
                        return false;
                    }
                }
                if !query.trim().is_empty() {
                    let tags = self.doc_tags.get(&doc.id);
                    let matched = doc.title.to_lowercase().contains(&query)
                        || doc.rel_path.to_lowercase().contains(&query)
                        || tags.is_some_and(|t| {
                            t.iter().any(|tag| tag.to_lowercase().contains(&query))
                        });
                    if !matched {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        match self.library_state.sort {
            SortMode::Title => {
                docs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            }
            SortMode::RecentlyOpened => {
                docs.sort_by_key(|d| std::cmp::Reverse(d.last_opened_at.unwrap_or(0)))
            }
            SortMode::Progress => {
                docs.sort_by(|a, b| b.reading_progress.total_cmp(&a.reading_progress))
            }
        }

        let total = self.docs.len();
        if total == 0 {
            empty_state(
                ui,
                palette,
                Icon::Library,
                "No documents",
                "Import Markdown files or add an existing folder to start your library.",
            );
            return;
        }
        if docs.is_empty() {
            empty_state(
                ui,
                palette,
                Icon::Search,
                "No matching documents",
                "Adjust the filters or search query.",
            );
            return;
        }

        // Keyboard navigation: arrows move the selection, Enter opens it.
        if !ui.ctx().text_edit_focused() {
            let (down, up, enter) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowDown),
                    i.key_pressed(egui::Key::ArrowUp),
                    i.key_pressed(egui::Key::Enter),
                )
            });
            if (down || up) && docs.len() > 1 {
                let current = docs
                    .iter()
                    .position(|d| self.library_state.selected.as_deref() == Some(d.id.as_str()));
                let next = match (current, down) {
                    (None, true) => Some(0),
                    (None, false) => Some(docs.len() - 1),
                    (Some(i), true) => Some((i + 1).min(docs.len() - 1)),
                    (Some(i), false) => Some(i.saturating_sub(1)),
                };
                if let Some(i) = next {
                    self.library_state.selected = Some(docs[i].id.clone());
                }
            }
            if enter {
                let target = self
                    .library_state
                    .selected
                    .clone()
                    .or_else(|| docs.first().map(|d| d.id.clone()));
                if let Some(id) = target {
                    self.open_document(&id);
                    return;
                }
            }
        }

        if docs.len() < total {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(crate::i18n::tf(
                        "Showing {} of {} documents",
                        &[&docs.len().to_string(), &total.to_string()],
                    ))
                    .size(11.5)
                    .color(palette.text_secondary),
                );
                if self.library_state.filters_active()
                    && button(
                        ui,
                        palette,
                        "Clear filters",
                        Some(Icon::Filter),
                        ButtonVariant::Quiet,
                    )
                    .clicked()
                {
                    self.library_state.clear_filters();
                }
            });
            ui.add_space(2.0);
        }

        let tag_colors: HashMap<String, egui::Color32> = self
            .tags
            .iter()
            .map(|tag| {
                let color = daftar_core::settings::ReadingTheme::parse_color(&tag.color)
                    .map(|c| egui::Color32::from_rgb(c[0], c[1], c[2]))
                    .unwrap_or(palette.accent);
                (tag.name.clone(), color)
            })
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("library-docs")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for doc in &docs {
                    let selected = self.library_state.selected.as_deref() == Some(doc.id.as_str());
                    let response = self
                        .library_row(ui, palette, doc, selected, &tag_colors)
                        .on_hover_text(crate::i18n::t("Double-click to open"));
                    if response.clicked() {
                        self.library_state.selected = Some(doc.id.clone());
                    }
                    if response.double_clicked() {
                        self.open_document(&doc.id);
                    }
                    response.context_menu(|ui| {
                        if ui.button(crate::i18n::t("Open")).clicked() {
                            self.open_document(&doc.id);
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("Rename")).clicked() {
                            self.prompt = Some(crate::app::TextPrompt {
                                title: crate::i18n::t("Rename document").into(),
                                label: "Title".into(),
                                value: doc.title.clone(),
                                action: crate::app::PromptAction::RenameDocument(doc.id.clone()),
                            });
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("Tags…")).clicked() {
                            self.prompt = Some(crate::app::TextPrompt {
                                title: "Add tag".into(),
                                label: crate::i18n::t("Tag").into(),
                                value: self.library_state.editing_tags.clone(),
                                action: crate::app::PromptAction::NewTag(doc.id.clone()),
                            });
                            ui.close();
                        }
                        if ui.button(crate::i18n::t("Remove from library")).clicked() {
                            self.confirm = Some(crate::app::ConfirmDialog {
                                title: crate::i18n::t("Remove document").into(),
                                message: crate::i18n::tf("Remove `{}` from the library? The Markdown file will remain on disk; highlights and bookmarks stored for it will be removed.", &[&doc.title]),
                                confirm_label: crate::i18n::t("Remove").into(),
                                action: crate::app::ConfirmAction::DeleteDocument(doc.id.clone()),
                            });
                            ui.close();
                        }
                    });
                    ui.add_space(3.0);
                }
            });
    }

    /// Paint one document row of the library list and return its click response.
    fn library_row(
        &mut self,
        ui: &mut Ui,
        palette: &Palette,
        doc: &Document,
        selected: bool,
        tag_colors: &HashMap<String, egui::Color32>,
    ) -> egui::Response {
        const ROW_H: f32 = 56.0;
        /// Leading block: icon tile plus padding on both sides.
        const LEADING: f32 = 56.0;
        /// Trailing block: reading progress (or the missing-file badge) plus padding.
        const TRAILING: f32 = 178.0;
        let rtl = crate::rtl::is_rtl();
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), ROW_H),
            egui::Sense::click(),
        );
        if selected {
            ui.painter().rect_filled(rect, 6.0, palette.selected);
        } else if response.hovered() {
            ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
        }
        if selected {
            // Leading accent bar, mirroring the sidebar's selected navigation item.
            let bar_x = if rtl { rect.right() - 3.0 } else { rect.left() };
            ui.painter().rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(bar_x, rect.top() + 9.0),
                    egui::vec2(3.0, ROW_H - 18.0),
                ),
                1.5,
                palette.accent,
            );
        }
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if !ui.is_rect_visible(rect) {
            return response;
        }

        // Icon tile.
        let tile = egui::Rect::from_min_size(
            egui::pos2(crate::rtl::leading_x(rect, 12.0), rect.center().y - 16.0),
            egui::Vec2::splat(32.0),
        );
        let missing = doc.is_missing;
        ui.painter().rect_filled(
            tile,
            8.0,
            if missing {
                palette.danger_subtle
            } else {
                palette.surface_sunken
            },
        );
        let (file_icon, icon_color) = if missing {
            (Icon::Warning, palette.danger)
        } else {
            (Icon::FileText, palette.text_secondary)
        };
        crate::icons::draw_icon(ui.painter(), file_icon, tile.shrink(8.0), icon_color);

        let text_colors = daftar_text::TextColors {
            text: palette.text,
            link: palette.accent,
            code: palette.text,
            code_background: None,
            selection: egui::Color32::TRANSPARENT,
        };
        let content_max = (rect.width() - LEADING - TRAILING).max(80.0);

        // Title (single line, truncated with an ellipsis when needed).
        let (title, title_w) = shaped_single_line(
            &mut self.engine,
            &doc.title,
            &crate::shaped::ShapedStyle::title()
                .with_size(13.5)
                .with_weight(600),
            content_max,
        );
        let title_x = crate::rtl::leading_x(rect, LEADING) - if rtl { title_w } else { 0.0 };
        self.engine.paint(
            ui.ctx(),
            ui.painter(),
            &title,
            egui::pos2(title_x, rect.min.y + 8.0),
            &text_colors,
            None,
            &[],
        );

        // Trailing zone: reading progress with a percentage, or a missing-file badge.
        if missing {
            let galley = ui.ctx().fonts_mut(|f| {
                f.layout_no_wrap(
                    crate::i18n::t("Missing file").to_owned(),
                    egui::FontId::proportional(10.5),
                    palette.danger,
                )
            });
            let pill_w = galley.size().x + 34.0;
            let pill_min_x = if rtl {
                rect.left() + 10.0
            } else {
                rect.right() - 10.0 - pill_w
            };
            let pill = egui::Rect::from_min_size(
                egui::pos2(pill_min_x, rect.min.y + 8.5),
                egui::vec2(pill_w, 18.0),
            );
            ui.painter().rect_filled(pill, 9.0, palette.danger_subtle);
            let icon_min_x = if rtl {
                pill.right() - 18.0
            } else {
                pill.left() + 6.0
            };
            crate::icons::draw_icon(
                ui.painter(),
                Icon::Warning,
                egui::Rect::from_min_size(
                    egui::pos2(icon_min_x, pill.center().y - 6.0),
                    egui::Vec2::splat(12.0),
                ),
                palette.danger,
            );
            let text_x = if rtl {
                pill.right() - 22.0 - galley.size().x
            } else {
                pill.left() + 22.0
            };
            ui.painter().galley(
                egui::pos2(text_x, pill.center().y - galley.size().y * 0.5),
                galley,
                palette.danger,
            );
        } else if doc.reading_progress > 0.001 {
            let fraction = doc.reading_progress.clamp(0.0, 1.0);
            let bar_w = 76.0;
            let bar_min_x = if rtl {
                rect.left() + 38.0
            } else {
                rect.right() - 38.0 - bar_w
            };
            let bar = egui::Rect::from_min_size(
                egui::pos2(bar_min_x, rect.min.y + 15.0),
                egui::vec2(bar_w, 4.0),
            );
            ui.painter().rect_filled(bar, 2.0, palette.surface_sunken);
            ui.painter().rect_filled(
                egui::Rect::from_min_size(bar.min, egui::vec2(bar_w * fraction, 4.0)),
                2.0,
                palette.accent,
            );
            let anchor = if rtl {
                egui::Align2::LEFT_CENTER
            } else {
                egui::Align2::RIGHT_CENTER
            };
            ui.painter().text(
                egui::pos2(crate::rtl::trailing_x(rect, 10.0), rect.min.y + 17.0),
                anchor,
                crate::i18n::tf("{}%", &[&format!("{:.0}", fraction * 100.0)]),
                egui::FontId::proportional(10.5),
                palette.text_secondary,
            );
        }

        // Meta line: relative path, reading time and tag pills.
        let meta_y = rect.min.y + 31.0;
        let path_font = egui::FontId::proportional(10.5);
        let (path, path_w) = truncated_text(ui, &doc.rel_path, path_font.clone(), content_max);
        ui.painter().text(
            egui::pos2(crate::rtl::leading_x(rect, LEADING), meta_y),
            crate::rtl::align2_leading_top(),
            &path,
            path_font,
            palette.text_secondary,
        );
        let limit = rect.width() - TRAILING;
        let mut cursor = LEADING + path_w;
        if doc.word_count > 0 && cursor + 10.0 < limit {
            let minutes = (doc.word_count / 200).max(1);
            let meta = crate::i18n::tf(
                "{} words · {} min read",
                &[&doc.word_count.to_string(), &minutes.to_string()],
            );
            let (shaped, w) = shaped_single_line(
                &mut self.engine,
                &meta,
                &crate::shaped::ShapedStyle::small().with_size(10.5),
                (limit - cursor - 10.0).max(40.0),
            );
            if cursor + 10.0 + w <= limit {
                let x = crate::rtl::leading_x(rect, cursor + 10.0) - if rtl { w } else { 0.0 };
                let meta_colors = daftar_text::TextColors {
                    text: palette.text_secondary,
                    link: palette.accent,
                    code: palette.text_secondary,
                    code_background: None,
                    selection: egui::Color32::TRANSPARENT,
                };
                self.engine.paint(
                    ui.ctx(),
                    ui.painter(),
                    &shaped,
                    egui::pos2(x, meta_y),
                    &meta_colors,
                    None,
                    &[],
                );
                cursor += 10.0 + w;
            }
        }
        if let Some(tags) = self.doc_tags.get(&doc.id) {
            for tag_name in tags.iter().take(3) {
                let (shaped, w) = shaped_single_line(
                    &mut self.engine,
                    tag_name,
                    &crate::shaped::ShapedStyle::small().with_size(10.0),
                    96.0,
                );
                let pill_w = w + 14.0;
                if cursor + 10.0 + pill_w > limit {
                    break;
                }
                cursor += 10.0;
                let pill_min_x = if rtl {
                    rect.right() - cursor - pill_w
                } else {
                    rect.left() + cursor
                };
                let pill = egui::Rect::from_min_size(
                    egui::pos2(pill_min_x, rect.min.y + 29.0),
                    egui::vec2(pill_w, 17.0),
                );
                let color = tag_colors
                    .get(tag_name.as_str())
                    .copied()
                    .unwrap_or(palette.text_secondary);
                ui.painter()
                    .rect_filled(pill, 8.0, color.gamma_multiply(0.13));
                let pill_colors = daftar_text::TextColors {
                    text: color,
                    link: color,
                    code: color,
                    code_background: None,
                    selection: egui::Color32::TRANSPARENT,
                };
                self.engine.paint(
                    ui.ctx(),
                    ui.painter(),
                    &shaped,
                    egui::pos2(pill.left() + (pill_w - w) * 0.5, rect.min.y + 30.5),
                    &pill_colors,
                    None,
                    &[],
                );
                cursor += pill_w;
            }
            let remaining = tags.len().saturating_sub(3);
            if remaining > 0 && cursor + 50.0 <= limit {
                ui.painter().text(
                    egui::pos2(crate::rtl::leading_x(rect, cursor + 10.0), meta_y),
                    crate::rtl::align2_leading_top(),
                    format!("+{remaining}"),
                    egui::FontId::proportional(10.5),
                    palette.text_secondary,
                );
            }
        }
        response
    }
}

/// Shape `text` as a single line, truncating with an ellipsis to fit `max_width`.
fn shaped_single_line(
    engine: &mut daftar_text::TextEngine,
    text: &str,
    style: &crate::shaped::ShapedStyle,
    max_width: f32,
) -> (daftar_text::ShapedText, f32) {
    let (shaped, width) = crate::shaped::shape(engine, text, style, 0.0);
    if width <= max_width || text.is_empty() {
        return (shaped, width);
    }
    let chars: Vec<char> = text.chars().collect();
    let avg = width / chars.len().max(1) as f32;
    let mut keep = (((max_width - 6.0) / avg).floor() as usize).min(chars.len());
    loop {
        let mut candidate: String = chars[..keep].iter().collect();
        candidate.push('…');
        let (shaped, width) = crate::shaped::shape(engine, &candidate, style, 0.0);
        if width <= max_width || keep == 0 {
            return (shaped, width);
        }
        let excess = (((width - max_width) / avg).ceil() as usize).max(1);
        keep = keep.saturating_sub(excess);
    }
}

/// Lay out `text` with egui's fonts, truncating with an ellipsis to fit `max_width`.
/// Returns the (possibly truncated) text and its measured width.
fn truncated_text(ui: &Ui, text: &str, font: egui::FontId, max_width: f32) -> (String, f32) {
    let width = |s: &str| {
        ui.ctx()
            .fonts_mut(|f| f.layout_no_wrap(s.to_owned(), font.clone(), egui::Color32::WHITE))
            .size()
            .x
    };
    let full = width(text);
    if full <= max_width || text.is_empty() {
        return (text.to_owned(), full);
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (0..chars.len()).rev() {
        let candidate: String = chars[..keep].iter().collect::<String>() + "…";
        let w = width(&candidate);
        if w <= max_width {
            return (candidate, w);
        }
    }
    let ellipsis = "…".to_owned();
    let w = width(&ellipsis);
    (ellipsis, w)
}
