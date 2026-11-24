//! Highlights and bookmarks browser.

use std::collections::HashSet;

use daftar_core::model::{Bookmark, Highlight};
use egui::Ui;

use crate::app::DaftarApp;
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarksTab {
    #[default]
    Highlights,
    Bookmarks,
}

#[derive(Debug, Clone, Default)]
pub struct HighlightsState {
    pub tab: MarksTab,
    pub filter: String,
    pub tag: Option<String>,
    pub selected: HashSet<String>,
    pub highlights: Vec<Highlight>,
    pub bookmarks: Vec<Bookmark>,
}

impl HighlightsState {
    pub fn reload(&mut self, store: &daftar_store::Store) {
        let library = store.libraries().ok().and_then(|l| l.into_iter().next());
        let Some(library) = library else { return };
        self.highlights = store.all_highlights(&library.id).unwrap_or_default();
        self.bookmarks = store.all_bookmarks(&library.id).unwrap_or_default();
    }
}

impl DaftarApp {
    pub(crate) fn show_highlights(&mut self, ui: &mut Ui, palette: &Palette) {
        let Some(library) = self.library.clone() else { return };
        self.highlights.highlights = self.store.all_highlights(&library.id).unwrap_or_default();
        self.highlights.bookmarks = self.store.all_bookmarks(&library.id).unwrap_or_default();
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::t("Highlights & Bookmarks"));
            ui.add_space(10.0);
            if crate::widgets::button(ui, palette, "Highlights", None, if self.highlights.tab == MarksTab::Highlights { crate::widgets::ButtonVariant::Secondary } else { crate::widgets::ButtonVariant::Quiet }).clicked() {
                self.highlights.tab = MarksTab::Highlights;
            }
            if crate::widgets::button(ui, palette, "Bookmarks", None, if self.highlights.tab == MarksTab::Bookmarks { crate::widgets::ButtonVariant::Secondary } else { crate::widgets::ButtonVariant::Quiet }).clicked() {
                self.highlights.tab = MarksTab::Bookmarks;
            }
        });
        ui.label(egui::RichText::new(crate::i18n::t("Anchors survive file edits; unresolved anchors are kept, never silently deleted.")).size(11.5).color(palette.text_secondary));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            crate::widgets::search_field(ui, palette, &mut self.highlights.filter, crate::i18n::t("Filter quotes and notes…"));
            if crate::widgets::button(ui, palette, "Select all", None, crate::widgets::ButtonVariant::Quiet).clicked() {
                match self.highlights.tab {
                    MarksTab::Highlights => self.highlights.selected = self.highlights.highlights.iter().map(|h| h.id.clone()).collect(),
                    MarksTab::Bookmarks => self.highlights.selected = self.highlights.bookmarks.iter().map(|b| b.id.clone()).collect(),
                }
            }
            if crate::widgets::button(ui, palette, "Clear", None, crate::widgets::ButtonVariant::Quiet).clicked() {
                self.highlights.selected.clear();
            }
        });
        if !self.highlights.selected.is_empty() {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tf("{} selected", &[&self.highlights.selected.len().to_string()]));
                if crate::widgets::button(ui, palette, "Export PNG ZIP", Some(crate::Icon::Archive), crate::widgets::ButtonVariant::Secondary).clicked() {
                    self.start_batch_export(crate::app::BatchFormat::PngZip);
                }
                if crate::widgets::button(ui, palette, "Export PDF", Some(crate::Icon::Pdf), crate::widgets::ButtonVariant::Secondary).clicked() {
                    self.start_batch_export(crate::app::BatchFormat::Pdf);
                }
            });
        }
        ui.separator();
        let filter = self.highlights.filter.to_lowercase();
        match self.highlights.tab {
            MarksTab::Highlights => {
                let items: Vec<Highlight> = self.highlights.highlights.iter().filter(|h| {
                    filter.is_empty() || h.quote.to_lowercase().contains(&filter) || h.note.to_lowercase().contains(&filter)
                }).cloned().collect();
                if items.is_empty() {
                    crate::widgets::empty_state(ui, palette, crate::Icon::Note, "No highlights yet", "Select text while reading and use the highlight button.");
                    return;
                }
                egui::ScrollArea::vertical().id_salt("highlights-list").auto_shrink([false,false]).show(ui, |ui| {
                    for highlight in items {
                        let checked = self.highlights.selected.contains(&highlight.id);
                        egui::Frame::new().fill(palette.surface).stroke(egui::Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let mut is_checked = checked;
                                if ui.checkbox(&mut is_checked, "").changed() {
                                    if is_checked { self.highlights.selected.insert(highlight.id.clone()); } else { self.highlights.selected.remove(&highlight.id); }
                                }
                                let color = daftar_core::settings::ReadingTheme::parse_color(&highlight.color).map(|c| egui::Color32::from_rgb(c[0],c[1],c[2])).unwrap_or(palette.warning);
                                let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(10.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 4.0, color);
                                let title = self.docs.iter().find(|d| d.id == highlight.document_id).map(|d| d.title.clone()).unwrap_or_else(|| crate::i18n::t("Document").into());
                                ui.label(egui::RichText::new(title).size(11.0).color(palette.text_secondary));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if crate::widgets::icon_button_sized(ui, palette, crate::Icon::Trash, crate::i18n::t("Delete"), false, 24.0).clicked() {
                                        self.confirm = Some(crate::app::ConfirmDialog {
                                            title: crate::i18n::t("Delete highlight").into(),
                                            message: crate::i18n::t("Delete this highlight? The source document is not modified.").into(),
                                            confirm_label: crate::i18n::t("Delete").into(),
                                            action: crate::app::ConfirmAction::DeleteHighlight(highlight.id.clone()),
                                        });
                                    }
                                    if crate::widgets::icon_button_sized(ui, palette, crate::Icon::Flashcards, "Create flashcard", false, 24.0).clicked() {
                                        self.open_flashcard_from_quote(&highlight.document_id, &highlight.quote, None);
                                    }
                                    if crate::widgets::icon_button_sized(ui, palette, crate::Icon::FileText, "Open document", false, 24.0).clicked() {
                                        self.open_document(&highlight.document_id);
                                    }
                                });
                            });
                            let (quote, _) = crate::shaped::shape(&mut self.engine, &daftar_core::util::plain_text(&highlight.quote), &crate::shaped::ShapedStyle::body(), ui.available_width());
                            let colors = daftar_text::TextColors { text: palette.text, link: palette.accent, code: palette.text, code_background: None, selection: egui::Color32::TRANSPARENT };
                            self.engine.paint(ui.ctx(), ui.painter(), &quote, ui.cursor().min, &colors, None, &[]);
                            let height = quote.height;
                            ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::hover());
                            if !highlight.note.trim().is_empty() {
                                ui.label(egui::RichText::new(&highlight.note).size(11.5).color(palette.text_secondary));
                            }
                        });
                        ui.add_space(4.0);
                    }
                });
            }
            MarksTab::Bookmarks => {
                let items: Vec<Bookmark> = self.highlights.bookmarks.iter().filter(|b| filter.is_empty() || b.label.to_lowercase().contains(&filter) || b.note.to_lowercase().contains(&filter)).cloned().collect();
                if items.is_empty() {
                    crate::widgets::empty_state(ui, palette, crate::Icon::Bookmark, "No bookmarks yet", "Select text and choose Bookmark.");
                    return;
                }
                egui::ScrollArea::vertical().id_salt("bookmarks-list").auto_shrink([false,false]).show(ui, |ui| {
                    for bookmark in items {
                        ui.horizontal(|ui| {
                            if crate::widgets::button(ui, palette, &bookmark.label, Some(crate::Icon::Bookmark), crate::widgets::ButtonVariant::Quiet).clicked() {
                                self.open_document(&bookmark.document_id);
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if crate::widgets::icon_button_sized(ui, palette, crate::Icon::Trash, crate::i18n::t("Delete"), false, 24.0).clicked() {
                                    self.confirm = Some(crate::app::ConfirmDialog {
                                        title: crate::i18n::t("Delete bookmark").into(),
                                        message: crate::i18n::t("Delete this bookmark?").into(),
                                        confirm_label: crate::i18n::t("Delete").into(),
                                        action: crate::app::ConfirmAction::DeleteHighlight(bookmark.id.clone()),
                                    });
                                }
                            });
                        });
                        ui.separator();
                    }
                });
            }
        }
    }
}
