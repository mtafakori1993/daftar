//! Knowledge library browser.

use std::collections::HashMap;

use daftar_core::model::{Category, Collection, Document, Id};
use egui::Ui;

use crate::app::DaftarApp;
use crate::theme::Palette;

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
        use crate::widgets::empty_state;
        use crate::Icon;
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::t("Library"));
            ui.add_space(8.0);
            if crate::widgets::button(ui, palette, "Import files", Some(Icon::Import), crate::widgets::ButtonVariant::Secondary).clicked() {
                self.open_picker(
                    crate::app::PickerPurpose::ImportMarkdown,
                    crate::picker::PickerMode::OpenFiles,
                    crate::i18n::t("Import Markdown files"),
                    &["md", "markdown", "mdown"],
                    "",
                );
            }
            if crate::widgets::button(ui, palette, "Add folder", Some(Icon::Folder), crate::widgets::ButtonVariant::Secondary).clicked() {
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
                ]
            ))
            .size(11.5)
            .color(palette.text_secondary),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            // The action buttons come first so the field can size itself to the remaining
            // width; otherwise the field would claim the full row and push the buttons out.
            if crate::widgets::button(ui, palette, "New collection", Some(Icon::Collections), crate::widgets::ButtonVariant::Quiet).clicked() {
                self.prompt = Some(crate::app::TextPrompt {
                    title: "New collection".into(),
                    label: crate::i18n::t("Collection name").into(),
                    value: String::new(),
                    action: crate::app::PromptAction::NewCollection,
                });
            }
            if crate::widgets::button(ui, palette, "New category", Some(Icon::Categories), crate::widgets::ButtonVariant::Quiet).clicked() {
                self.prompt = Some(crate::app::TextPrompt {
                    title: "New category".into(),
                    label: crate::i18n::t("Category name").into(),
                    value: String::new(),
                    action: crate::app::PromptAction::NewCategory,
                });
            }
            ui.add_space(4.0);
            crate::widgets::search_field(ui, palette, &mut self.library_state.query, crate::i18n::t("Filter documents by title or tag…"));
        });
        // Filters.
        ui.horizontal_wrapped(|ui| {
            if crate::widgets::chip(ui, palette, crate::i18n::t("All"), self.library_state.collection.is_none() && self.library_state.category.is_none() && !self.library_state.recent_only).clicked() {
                self.library_state.collection = None;
                self.library_state.category = None;
                self.library_state.recent_only = false;
            }
            if crate::widgets::chip(ui, palette, "Recent", self.library_state.recent_only).clicked() {
                self.library_state.recent_only = !self.library_state.recent_only;
            }
            for collection in self.collections.clone() {
                let selected = self.library_state.collection.as_deref() == Some(collection.id.as_str());
                if crate::widgets::chip(ui, palette, &collection.name, selected).clicked() {
                    self.library_state.collection = if selected { None } else { Some(collection.id.clone()) };
                    self.library_state.category = None;
                }
            }
            for category in self.categories.clone() {
                let selected = self.library_state.category.as_deref() == Some(category.id.as_str());
                if crate::widgets::chip(ui, palette, &category.name, selected).clicked() {
                    self.library_state.category = if selected { None } else { Some(category.id.clone()) };
                    self.library_state.collection = None;
                }
            }
        });
        if !self.tags.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for tag in self.tags.clone() {
                    let selected = self.library_state.tag.as_deref() == Some(tag.name.as_str());
                    if crate::widgets::chip(ui, palette, &tag.name, selected).clicked() {
                        self.library_state.tag = if selected { None } else { Some(tag.name.clone()) };
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
                if self.library_state.recent_only
                    && !self.recents.iter().any(|r| r.id == doc.id)
                {
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
                        || tags.is_some_and(|t| t.iter().any(|tag| tag.to_lowercase().contains(&query)));
                    if !matched {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        match self.library_state.sort {
            SortMode::Title => docs.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
            SortMode::RecentlyOpened => docs.sort_by_key(|d| std::cmp::Reverse(d.last_opened_at.unwrap_or(0))),
            SortMode::Progress => docs.sort_by(|a, b| b.reading_progress.total_cmp(&a.reading_progress)),
        }

        if docs.is_empty() {
            empty_state(ui, palette, Icon::Library, "No documents", "Import Markdown files or add an existing folder to start your library.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("library-docs")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for doc in docs {
                    let selected = self.library_state.selected.as_deref() == Some(doc.id.as_str());
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 52.0),
                        egui::Sense::click(),
                    );
                    if selected {
                        ui.painter().rect_filled(rect, 6.0, palette.selected);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
                    }
                    let rtl = crate::rtl::is_rtl();
                    crate::icons::draw_icon(
                        ui.painter(),
                        Icon::FileText,
                        egui::Rect::from_min_size(
                            egui::pos2(crate::rtl::leading_x(rect, 30.0), rect.min.y + 16.0),
                            egui::Vec2::splat(18.0),
                        ),
                        palette.text_secondary,
                    );
                    let (title, _) = crate::shaped::shape(
                        &mut self.engine,
                        &doc.title,
                        &crate::shaped::ShapedStyle::title().with_size(13.5).with_weight(600),
                        0.0,
                    );
                    let text_colors = daftar_text::TextColors {
                        text: palette.text,
                        link: palette.accent,
                        code: palette.text,
                        code_background: None,
                        selection: egui::Color32::TRANSPARENT,
                    };
                    let title_width = title
                        .lines
                        .iter()
                        .map(|line| line.width)
                        .fold(0.0f32, f32::max);
                    let title_x = crate::rtl::leading_x(rect, 40.0)
                        - if rtl { title_width } else { 0.0 };
                    self.engine.paint(
                        ui.ctx(),
                        ui.painter(),
                        &title,
                        egui::pos2(title_x, rect.min.y + 8.0),
                        &text_colors,
                        None,
                        &[],
                    );
                    ui.painter().text(
                        egui::pos2(crate::rtl::leading_x(rect, 40.0), rect.min.y + 30.0),
                        crate::rtl::align2_leading_top(),
                        &doc.rel_path,
                        egui::FontId::proportional(10.5),
                        palette.text_secondary,
                    );
                    let trailing_anchor = if rtl {
                        rect.left() + 120.0
                    } else {
                        rect.right() - 120.0
                    };
                    if doc.is_missing {
                        ui.painter().text(
                            egui::pos2(trailing_anchor, rect.center().y),
                            if rtl { egui::Align2::LEFT_CENTER } else { egui::Align2::RIGHT_CENTER },
                            crate::i18n::t("Missing file"),
                            egui::FontId::proportional(10.5),
                            palette.danger,
                        );
                    } else if doc.reading_progress > 0.001 {
                        let bar = egui::Rect::from_min_size(
                            egui::pos2(trailing_anchor - 84.0, rect.center().y - 2.0),
                            egui::vec2(84.0, 4.0),
                        );
                        ui.painter().rect_filled(bar, 2.0, palette.surface_sunken);
                        ui.painter().rect_filled(
                            egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * doc.reading_progress, 4.0)),
                            2.0,
                            palette.accent,
                        );
                    }
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
                }
            });
    }
}
