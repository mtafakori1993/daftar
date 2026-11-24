//! Library-wide search view.

use daftar_core::model::SearchHit;
use daftar_store::{SearchFilters, SearchKind};
use egui::Ui;

use crate::app::{DaftarApp, View};
use crate::theme::Palette;
use crate::{Icon, shaped::{shape, ShapedStyle}, widgets};

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub kind: Option<SearchKind>,
    pub category: Option<String>,
    pub collection: Option<String>,
    pub tag: Option<String>,
    pub results: Vec<SearchHit>,
    pub error: Option<String>,
    pub last_signature: String,
}

impl SearchState {
    pub fn run(&mut self, store: &daftar_store::Store, library: Option<&daftar_core::model::Library>) {
        let Some(library) = library else { return };
        let signature = format!(
            "{}|{:?}|{:?}|{:?}|{:?}",
            self.query, self.kind, self.category, self.collection, self.tag
        );
        if signature == self.last_signature {
            return;
        }
        self.last_signature = signature;
        if self.query.trim().is_empty() {
            self.results.clear();
            self.error = None;
            return;
        }
        let mut filters = SearchFilters::new(&library.id, self.query.trim());
        filters.kind = self.kind;
        filters.category_id = self.category.clone();
        filters.collection_id = self.collection.clone();
        filters.tag = self.tag.clone();
        match store.search(&filters) {
            Ok(results) => {
                self.results = results;
                self.error = None;
            }
            Err(error) => {
                self.results.clear();
                self.error = Some(error.to_string());
            }
        }
    }
}

impl DaftarApp {
    pub(crate) fn show_search(&mut self, ui: &mut Ui, palette: &Palette) {
        let Some(library) = self.library.clone() else { return };
        ui.add_space(10.0);
        ui.heading(crate::i18n::t("Search"));
        ui.label(
            egui::RichText::new(crate::i18n::t("Search documents, headings, highlights, bookmarks, flashcards and roadmaps."))
                .size(11.5)
                .color(palette.text_secondary),
        );
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            widgets::search_field(ui, palette, &mut self.search.query, crate::i18n::t("Search the library…"));
            if widgets::button(ui, palette, "Search", Some(Icon::Search), widgets::ButtonVariant::Primary).clicked() {
                self.search.last_signature.clear();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if widgets::chip(ui, palette, crate::i18n::t("All"), self.search.kind.is_none()).clicked() {
                self.search.kind = None;
                self.search.last_signature.clear();
            }
            for kind in [
                SearchKind::Documents,
                SearchKind::Highlights,
                SearchKind::Flashcards,
                SearchKind::Bookmarks,
                SearchKind::Roadmaps,
            ] {
                if widgets::chip(ui, palette, kind.label(), self.search.kind == Some(kind)).clicked() {
                    self.search.kind = if self.search.kind == Some(kind) { None } else { Some(kind) };
                    self.search.last_signature.clear();
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for collection in self.collections.clone() {
                let selected = self.search.collection.as_deref() == Some(collection.id.as_str());
                if widgets::chip(ui, palette, &collection.name, selected).clicked() {
                    self.search.collection = if selected { None } else { Some(collection.id.clone()) };
                    self.search.last_signature.clear();
                }
            }
            for category in self.categories.clone() {
                let selected = self.search.category.as_deref() == Some(category.id.as_str());
                if widgets::chip(ui, palette, &category.name, selected).clicked() {
                    self.search.category = if selected { None } else { Some(category.id.clone()) };
                    self.search.last_signature.clear();
                }
            }
            for tag in self.tags.iter().take(14).cloned() {
                let selected = self.search.tag.as_deref() == Some(tag.name.as_str());
                if widgets::chip(ui, palette, &tag.name, selected).clicked() {
                    self.search.tag = if selected { None } else { Some(tag.name.clone()) };
                    self.search.last_signature.clear();
                }
            }
        });
        ui.separator();
        self.search.run(&self.store, self.library.as_ref());
        if let Some(error) = &self.search.error {
            ui.colored_label(palette.danger, error);
            return;
        }
        if self.search.query.trim().is_empty() {
            widgets::empty_state(ui, palette, Icon::Search, "Search your knowledge", "Type a word or phrase. Filters compose with the query.");
            return;
        }
        if self.search.results.is_empty() {
            widgets::empty_state(ui, palette, Icon::Search, "No results", "Try a different word or remove a filter.");
            return;
        }
        let results = self.search.results.clone();
        egui::ScrollArea::vertical()
            .id_salt("search-results")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for hit in results {
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 64.0),
                        egui::Sense::click(),
                    );
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
                    }
                    let icon = match hit.kind.as_str() {
                        "highlight" => Icon::Note,
                        "flashcard" => Icon::Flashcards,
                        "bookmark" => Icon::Bookmark,
                        "roadmap" => Icon::Roadmap,
                        _ => Icon::FileText,
                    };
                    crate::icons::draw_icon(
                        ui.painter(),
                        icon,
                        egui::Rect::from_min_size(rect.min + egui::vec2(12.0, 22.0), egui::Vec2::splat(18.0)),
                        palette.text_secondary,
                    );
                    let (title, _) = shape(
                        &mut self.engine,
                        &hit.title,
                        &ShapedStyle::title().with_size(13.0).with_weight(600),
                        ui.available_width() - 80.0,
                    );
                    let text_colors = daftar_text::TextColors {
                        text: palette.text,
                        link: palette.accent,
                        code: palette.text,
                        code_background: None,
                        selection: egui::Color32::TRANSPARENT,
                    };
                    self.engine.paint(ui.ctx(), ui.painter(), &title, rect.min + egui::vec2(40.0, 9.0), &text_colors, None, &[]);
                    let (snippet, _) = shape(
                        &mut self.engine,
                        &hit.snippet,
                        &ShapedStyle::body().with_size(11.5),
                        ui.available_width() - 80.0,
                    );
                    let text_colors = daftar_text::TextColors {
                        text: palette.text_secondary,
                        link: palette.accent,
                        code: palette.text,
                        code_background: None,
                        selection: egui::Color32::TRANSPARENT,
                    };
                    self.engine.paint(ui.ctx(), ui.painter(), &snippet, rect.min + egui::vec2(40.0, 31.0), &text_colors, None, &[]);
                    ui.painter().text(
                        egui::pos2(rect.right() - 10.0, rect.top() + 10.0),
                        egui::Align2::RIGHT_TOP,
                        hit.kind.to_uppercase(),
                        egui::FontId::proportional(9.5),
                        palette.text_secondary,
                    );
                    if response.clicked() {
                        match hit.kind.as_str() {
                            "document" => self.open_document(&hit.id),
                            "highlight" | "bookmark" => {
                                if let Some(doc) = hit.document_id.clone() {
                                    self.open_document(&doc);
                                }
                            }
                            "flashcard" => {
                                if let Some(doc) = hit.document_id.clone() {
                                    self.open_document(&doc);
                                } else {
                                    self.navigate(View::Flashcards, crate::app::NavSection::Flashcards);
                                }
                            }
                            "roadmap" => {
                                self.navigate(View::Roadmaps, crate::app::NavSection::Roadmaps);
                                self.roadmaps.select(&hit.id);
                            }
                            _ => {}
                        }
                    }
                }
            });
        let _ = library;
    }
}
