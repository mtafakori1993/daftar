//! Top bar, status bar and navigation sidebar.

use std::path::Path;

use egui::Ui;

use crate::app::{DaftarApp, NavSection, PickerPurpose, View};
use crate::icons::{draw_icon, Icon};
use crate::picker::PickerMode;
use crate::shaped::{shape, shaped_label, ShapedStyle};
use crate::theme::Palette;
use crate::widgets::{self, ButtonVariant};

impl DaftarApp {
    pub(crate) fn show_topbar(&mut self, ui: &mut Ui, palette: &Palette) {
        ui.horizontal_centered(|ui| {
            if widgets::icon_button(ui, palette, Icon::Menu, "Toggle navigation panel", self.show_left).clicked() {
                self.show_left = !self.show_left;
                self.settings.window.left_panel_open = self.show_left;
                self.mark_dirty();
            }
            let (mark, _) = ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::hover());
            if let Some(logo) = &self.logo {
                ui.painter().image(
                    logo.id(),
                    mark,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().rect_filled(mark, 6.0, palette.accent);
                ui.painter().rect_filled(egui::Rect::from_min_size(mark.min + egui::vec2(6.0, 6.0), egui::vec2(14.0, 3.0)), 1.5, palette.on_accent);
                ui.painter().rect_filled(egui::Rect::from_min_size(mark.min + egui::vec2(6.0, 12.0), egui::vec2(9.0, 3.0)), 1.5, palette.on_accent);
                ui.painter().rect_filled(egui::Rect::from_min_size(mark.min + egui::vec2(6.0, 18.0), egui::vec2(12.0, 3.0)), 1.5, palette.on_accent);
            }
            ui.label(egui::RichText::new(crate::i18n::t("Daftar")).size(16.0).strong().color(palette.text));
            ui.add_space(6.0);
            let current = self
                .library
                .as_ref()
                .map(|l| l.name.clone())
                .unwrap_or_else(|| crate::i18n::t("No library").into());
            let mut chosen: Option<String> = None;
            egui::ComboBox::from_id_salt("library-switcher")
                .selected_text(egui::RichText::new(current).size(12.5))
                .width(170.0)
                .show_ui(ui, |ui| {
                    for library in &self.libraries {
                        if ui.selectable_label(
                            self.library.as_ref().map(|l| l.id == library.id).unwrap_or(false),
                            egui::RichText::new(&library.name).size(12.5),
                        ).clicked() {
                            chosen = Some(library.id.clone());
                        }
                    }
                    ui.separator();
                    if ui.button(crate::i18n::t("Open another library…")).clicked() {
                        ui.close();
                    }
                });
            if let Some(id) = chosen {
                self.switch_library(&id);
            }
            if widgets::icon_button_sized(ui, palette, Icon::Plus, "Add library", false, 26.0).clicked() {
                self.open_picker(PickerPurpose::AddLibrary, PickerMode::OpenFolder, crate::i18n::t("Add library folder"), &[], "");
            }
            ui.add_space(8.0);
            ui.scope(|ui| {
                ui.set_max_width((ui.available_width() - 330.0).clamp(220.0, 520.0));
                let response = widgets::search_field(ui, palette, &mut self.search.query, crate::i18n::t("Search the library…  Ctrl+F"));
                if self.search_focus {
                    response.request_focus();
                    self.search_focus = false;
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    self.search.last_signature.clear();
                    if self.view != View::Reader {
                        self.navigate(View::Search, NavSection::Library);
                    }
                }
                if response.changed() {
                    // Live search while typing when already on the search view.
                    self.search.last_signature.clear();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, palette, Icon::Settings, "Settings", self.view == View::Settings).clicked() {
                    self.navigate(View::Settings, NavSection::Settings);
                }
                if widgets::icon_button(ui, palette, if self.dark { Icon::Sun } else { Icon::Moon }, "Toggle light/dark theme", false).clicked() {
                    self.toggle_theme(ui.ctx());
                }
                widgets::toolbar_divider(ui, palette);
                if widgets::button(ui, palette, "Import", Some(Icon::Import), ButtonVariant::Secondary).clicked() {
                    self.open_picker(PickerPurpose::ImportMarkdown, PickerMode::OpenFiles, crate::i18n::t("Import Markdown files"), &["md", "markdown", "mdown"], "");
                }
            });
        });
    }

    pub(crate) fn show_statusbar(&mut self, ui: &mut Ui, palette: &Palette, ctx: &egui::Context) {
        ui.horizontal_centered(|ui| {
            let busy = self.jobs.is_busy();
            let (icon, color) = if busy {
                (Icon::Refresh, palette.accent)
            } else {
                (Icon::Check, palette.success)
            };
            let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
            draw_icon(ui.painter(), icon, r, color);
            ui.label(egui::RichText::new(&self.status).size(10.5).color(palette.text_secondary));
            if busy {
                if let Some(progress) = &self.jobs.progress {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 4.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, palette.surface_sunken);
                    if progress.total > 0 {
                        let f = (progress.done as f32 / progress.total as f32).clamp(0.0, 1.0);
                        ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * f, 4.0)), 2.0, palette.accent);
                    }
                }
                if widgets::button(ui, palette, "Cancel", Some(Icon::Close), ButtonVariant::Quiet).clicked() {
                    self.jobs.cancel();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.config.portable {
                    widgets::meta_pill(ui, palette, "Portable", palette.accent);
                }
                if let Some(reader) = &self.reader {
                    ui.label(egui::RichText::new(crate::i18n::tf("{:.0}% read · {} min", &[&format!("{:.0}", reader.progress() * 100.0), &reader.reading_minutes().to_string()])).size(10.5).color(palette.text_secondary));
                } else if let Some(library) = &self.library {
                    ui.label(egui::RichText::new(crate::i18n::tf("{} docs · {} highlights · {} cards", &[&self.stats.documents.to_string(), &self.stats.highlights.to_string(), &self.stats.flashcards.to_string()])).size(10.5).color(palette.text_secondary));
                    let _ = library;
                }
                if self.jobs.is_busy() {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                }
                let _ = ctx;
            });
        });
    }

    pub(crate) fn show_sidebar(&mut self, ui: &mut Ui, palette: &Palette) {
        let library_name = self.library.as_ref().map(|l| l.name.clone()).unwrap_or_default();
        shaped_label(ui, &mut self.engine, &library_name, &ShapedStyle::title().with_size(15.0), palette.text, ui.available_width(), egui::Sense::hover());
        if let Some(library) = &self.library {
            let path = Path::new(&library.root_path).display().to_string();
            if widgets::button(ui, palette, "Library folder", Some(Icon::FolderOpen), ButtonVariant::Quiet).on_hover_text(path).clicked() {
                open_external_path(&library.root_path);
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::button(ui, palette, "Import", Some(Icon::Import), ButtonVariant::Quiet).clicked() {
                self.open_picker(PickerPurpose::ImportMarkdown, PickerMode::OpenFiles, crate::i18n::t("Import Markdown files"), &["md", "markdown", "mdown"], "");
            }
            if widgets::button(ui, palette, "Folder", Some(Icon::Folder), ButtonVariant::Quiet).clicked() {
                self.open_picker(PickerPurpose::AddFolder, PickerMode::OpenFolder, "Add folder", &[], "");
            }
        });
        ui.add_space(6.0);
        widgets::section_label(ui, palette, "Knowledge");
        let counts = (
            self.stats.documents,
            self.stats.collections,
            self.stats.categories,
        );
        if widgets::nav_item(ui, palette, Icon::Library, "Library", self.nav == NavSection::Library && self.view == View::Library, Some(counts.0)).clicked() {
            self.library_state.recent_only = false;
            self.library_state.collection = None;
            self.library_state.category = None;
            self.navigate(View::Library, NavSection::Library);
        }
        if widgets::nav_item(ui, palette, Icon::Collections, "Collections", self.nav == NavSection::Collections && self.view == View::Library, Some(counts.1)).clicked() {
            self.navigate(View::Library, NavSection::Collections);
        }
        if widgets::nav_item(ui, palette, Icon::Categories, "Categories", self.nav == NavSection::Categories && self.view == View::Library, Some(counts.2)).clicked() {
            self.navigate(View::Library, NavSection::Categories);
        }
        if widgets::nav_item(ui, palette, Icon::Roadmap, "Study roadmaps", self.view == View::Roadmaps, Some(self.stats.roadmaps)).clicked() {
            self.navigate(View::Roadmaps, NavSection::Roadmaps);
        }
        if widgets::nav_item(ui, palette, Icon::Bookmark, "Bookmarks", self.view == View::Highlights && self.highlights.tab == crate::views::highlights::MarksTab::Bookmarks, Some(self.stats.bookmarks)).clicked() {
            self.highlights.tab = crate::views::highlights::MarksTab::Bookmarks;
            self.navigate(View::Highlights, NavSection::Bookmarks);
        }
        if widgets::nav_item(ui, palette, Icon::Flashcards, "Flashcards", self.view == View::Flashcards || self.view == View::Study, Some(self.stats.flashcards)).clicked() {
            self.navigate(View::Flashcards, NavSection::Flashcards);
        }
        if widgets::nav_item(ui, palette, Icon::Recent, "Recent documents", self.nav == NavSection::Recent && self.view == View::Library, None).clicked() {
            self.library_state.recent_only = true;
            self.library_state.collection = None;
            self.library_state.category = None;
            self.navigate(View::Library, NavSection::Recent);
        }
        if widgets::nav_item(ui, palette, Icon::Note, "Highlights", self.view == View::Highlights && self.highlights.tab == crate::views::highlights::MarksTab::Highlights, Some(self.stats.highlights)).clicked() {
            self.highlights.tab = crate::views::highlights::MarksTab::Highlights;
            self.navigate(View::Highlights, NavSection::Bookmarks);
        }
        ui.add_space(4.0);
        widgets::section_label(ui, palette, "Collections");
        if self.collections.is_empty() {
            widgets::hint(ui, palette, "No collections yet.");
        }
        let collections = self.collections.clone();
        for collection in collections {
            let count = self.docs.iter().filter(|d| d.collection_id.as_deref() == Some(collection.id.as_str())).count() as u64;
            if widgets::nav_item(ui, palette, Icon::Collections, &collection.name, self.library_state.collection.as_deref() == Some(collection.id.as_str()) && self.view == View::Library, Some(count)).clicked() {
                self.library_state.collection = Some(collection.id.clone());
                self.library_state.category = None;
                self.library_state.recent_only = false;
                self.navigate(View::Library, NavSection::Collections);
            }
        }
        ui.add_space(4.0);
        widgets::section_label(ui, palette, "Categories");
        let categories = self.categories.clone();
        for category in categories {
            let count = self.docs.iter().filter(|d| d.category_id.as_deref() == Some(category.id.as_str())).count() as u64;
            if widgets::nav_item(ui, palette, Icon::Categories, &category.name, self.library_state.category.as_deref() == Some(category.id.as_str()) && self.view == View::Library, Some(count)).clicked() {
                self.library_state.category = Some(category.id.clone());
                self.library_state.collection = None;
                self.library_state.recent_only = false;
                self.navigate(View::Library, NavSection::Categories);
            }
        }
        if !self.tags.is_empty() {
            ui.add_space(4.0);
            widgets::section_label(ui, palette, "Tags");
            ui.horizontal_wrapped(|ui| {
                for tag in self.tags.iter().take(20).cloned().collect::<Vec<_>>() {
                    let selected = self.library_state.tag.as_deref() == Some(tag.name.as_str());
                    if widgets::chip(ui, palette, &tag.name, selected).clicked() {
                        self.library_state.tag = if selected { None } else { Some(tag.name) };
                        self.library_state.recent_only = false;
                        self.navigate(View::Library, NavSection::Library);
                    }
                }
            });
        }
        ui.add_space(8.0);
        ui.separator();
        if widgets::nav_item(ui, palette, Icon::Settings, "Settings", self.view == View::Settings, None).clicked() {
            self.navigate(View::Settings, NavSection::Settings);
        }
        // Keep portable mode visible at the bottom of the nav.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.add_space(4.0);
            let text = if self.config.portable {
                crate::i18n::t("Portable mode · data beside executable")
            } else {
                crate::i18n::t("Standard data directory")
            };
            ui.label(egui::RichText::new(text).size(9.5).color(palette.text_secondary));
        });
    }
}

fn open_external_path(path: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
}
