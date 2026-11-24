//! In-app file and folder picker.
//!
//! Daftar deliberately ships its own picker so it does not depend on platform GTK/portal
//! libraries and works identically from a USB drive on Linux and Windows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use daftar_text::TextEngine;
use egui::{Align2, Color32, Sense, Vec2, vec2};

use crate::shaped::{ShapedStyle, shaped_label};
use crate::theme::Palette;
use crate::widgets::{self, ButtonVariant};
use crate::{Icon, icons::draw_icon};
use crate::rtl::DirUi;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerMode {
    OpenFile,
    OpenFiles,
    OpenFolder,
    SaveFile,
}

#[derive(Debug, Clone)]
pub struct FilePicker {
    pub mode: PickerMode,
    pub title: String,
    current_dir: PathBuf,
    entries: Vec<Entry>,
    selected: HashSet<PathBuf>,
    filename: String,
    extensions: Vec<String>,
    show_hidden: bool,
    error: Option<String>,
    places: Vec<(String, PathBuf)>,
    confirm_overwrite: bool,
}

#[derive(Debug, Clone)]
struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickerResult {
    None,
    Picked(Vec<PathBuf>),
    Cancelled,
}

impl FilePicker {
    pub fn new(
        mode: PickerMode,
        title: impl Into<String>,
        start: impl AsRef<Path>,
        extensions: &[&str],
        default_name: &str,
    ) -> Self {
        let start = start.as_ref();
        let start = if start.is_dir() {
            start.to_path_buf()
        } else {
            start
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };
        let mut picker = Self {
            mode,
            title: title.into(),
            current_dir: start,
            entries: Vec::new(),
            selected: HashSet::new(),
            filename: default_name.to_string(),
            extensions: extensions.iter().map(|e| e.to_lowercase()).collect(),
            show_hidden: false,
            error: None,
            places: Vec::new(),
            confirm_overwrite: false,
        };
        picker.places = picker.quick_places();
        picker.refresh();
        picker
    }

    fn quick_places(&self) -> Vec<(String, PathBuf)> {
        let mut places = Vec::new();
        if let Some(home) = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
        {
            places.push((crate::i18n::t("Home").to_string(), home.clone()));
            for name in [crate::i18n::t("Documents"), crate::i18n::t("Desktop"), crate::i18n::t("Downloads")] {
                let p = home.join(name);
                if p.is_dir() {
                    places.push((name.to_string(), p));
                }
            }
        }
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        places.push((crate::i18n::t("Working folder").to_string(), cwd));
        places
    }

    fn refresh(&mut self) {
        self.entries.clear();
        self.error = None;
        match std::fs::read_dir(&self.current_dir) {
            Ok(iter) => {
                for entry in iter.flatten() {
                    let path = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !self.show_hidden && name.starts_with('.') {
                        continue;
                    }
                    let meta = entry.metadata().ok();
                    let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                    if !is_dir && !self.matches_filter(&path) {
                        continue;
                    }
                    self.entries.push(Entry {
                        path,
                        name,
                        is_dir,
                        size: meta.as_ref().map(|m| m.len()).unwrap_or(0),
                        modified: meta.and_then(|m| m.modified().ok()),
                    });
                }
            }
            Err(error) => self.error = Some(crate::i18n::tf("Cannot read folder: {}", &[&error.to_string()])),
        }
        self.entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        self.entries.truncate(5000);
    }

    fn matches_filter(&self, path: &Path) -> bool {
        if self.extensions.is_empty() {
            return true;
        }
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        self.extensions.iter().any(|e| e == &ext)
    }

    fn navigate(&mut self, path: PathBuf) {
        if path.is_dir() {
            self.current_dir = path;
            self.selected.clear();
            self.refresh();
        }
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.navigate(parent);
        }
    }

    fn create_folder(&mut self) {
        for n in 1..100 {
            let name = if n == 1 {
                "New folder".to_string()
            } else {
                crate::i18n::tf("New folder {}", &[&n.to_string()])
            };
            let path = self.current_dir.join(name);
            if !path.exists() {
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        self.refresh();
                        self.selected.clear();
                        self.selected.insert(path);
                    }
                    Err(error) => self.error = Some(crate::i18n::tf("Could not create folder: {}", &[&error.to_string()])),
                }
                return;
            }
        }
    }

    fn select_row(&mut self, path: PathBuf, multi: bool) {
        if multi {
            if !self.selected.remove(&path) {
                self.selected.insert(path);
            }
        } else {
            self.selected.clear();
            self.selected.insert(path);
        }
    }

    /// Draw the picker. Returns `None` while it remains open.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        engine: &mut TextEngine,
        palette: &Palette,
    ) -> Option<PickerResult> {
        let mut result = None;
        let mut open = true;
        egui::Window::new(&self.title)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(760.0, 520.0))
            .min_size(vec2(520.0, 360.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    // Path bar.
                    ui.horizontal(|ui| {
                        if widgets::icon_button(ui, palette, Icon::ArrowLeft, "Parent folder", false)
                            .clicked()
                        {
                            self.go_up();
                        }
                        if widgets::button(ui, palette, "New folder", Some(Icon::Folder), ButtonVariant::Quiet).clicked() {
                            self.create_folder();
                        }
                        let path_text = self.current_dir.to_string_lossy().to_string();
                        let max = (ui.available_width() - 140.0).max(120.0);
                        shaped_label(
                            ui,
                            engine,
                            &path_text,
                            &ShapedStyle::body().with_size(12.0),
                            palette.text_secondary,
                            max,
                            Sense::hover(),
                        );
                    });
                    if !self.places.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            for (name, path) in self.places.clone() {
                                if widgets::chip(ui, palette, &name, self.current_dir == path).clicked() {
                                    self.navigate(path);
                                }
                            }
                        });
                    }
                    ui.separator();
                    if let Some(error) = &self.error {
                        ui.label(egui::RichText::new(error).color(palette.danger).size(12.0));
                    }
                    let list_height = (ui.available_height() - 92.0).max(120.0);
                    egui::ScrollArea::vertical()
                        .id_salt("picker-list")
                        .max_height(list_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let mut navigate_to: Option<PathBuf> = None;
                            let multi = self.mode == PickerMode::OpenFiles;
                            for entry in self.entries.clone() {
                                let selected = self.selected.contains(&entry.path);
                                let (rect, response) = ui.allocate_exact_size(
                                    vec2(ui.available_width(), 30.0),
                                    Sense::click(),
                                );
                                if selected {
                                    ui.painter().rect_filled(rect, 4.0, palette.selected);
                                } else if response.hovered() {
                                    ui.painter().rect_filled(rect, 4.0, palette.surface_hover);
                                }
                                let icon = if entry.is_dir {
                                    Icon::Folder
                                } else {
                                    Icon::FileText
                                };
                                draw_icon(
                                    ui.painter(),
                                    icon,
                                    egui::Rect::from_min_size(
                                        rect.min + vec2(8.0, 7.0),
                                        Vec2::splat(16.0),
                                    ),
                                    if entry.is_dir { palette.warning } else { palette.text_secondary },
                                );
                                let (shaped, _) = crate::shaped::shape(
                                    engine,
                                    &entry.name,
                                    &ShapedStyle::body().with_size(13.0),
                                    0.0,
                                );
                                let text_colors = daftar_text::TextColors {
                                    text: palette.text,
                                    link: palette.accent,
                                    code: palette.text,
                                    code_background: None,
                                    selection: Color32::TRANSPARENT,
                                };
                                let clip = egui::Rect::from_min_max(
                                    egui::pos2(rect.left() + 30.0, rect.top()),
                                    egui::pos2(rect.right() - 160.0, rect.bottom()),
                                );
                                let painter = ui.painter().with_clip_rect(clip);
                                engine.paint(
                                    ui.ctx(),
                                    &painter,
                                    &shaped,
                                    egui::pos2(rect.left() + 30.0, rect.top() + 5.0),
                                    &text_colors,
                                    None,
                                    &[],
                                );
                                if !entry.is_dir {
                                    ui.painter().text(
                                        egui::pos2(rect.right() - 150.0, rect.center().y),
                                        Align2::RIGHT_CENTER,
                                        format_size(entry.size),
                                        egui::FontId::proportional(10.5),
                                        palette.text_secondary,
                                    );
                                }
                                if response.clicked() {
                                    self.select_row(entry.path.clone(), multi);
                                }
                                if response.double_clicked() {
                                    if entry.is_dir {
                                        navigate_to = Some(entry.path.clone());
                                    } else {
                                        result = Some(PickerResult::Picked(vec![entry.path.clone()]));
                                    }
                                }
                            }
                            if let Some(path) = navigate_to {
                                self.navigate(path);
                            }
                        });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if self.mode == PickerMode::SaveFile {
                            ui.label(crate::i18n::t("File name"));
                            let name_response = widgets::text_field(
                                ui,
                                palette,
                                &mut self.filename,
                                "name.md",
                                320.0,
                            );
                            if name_response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                if let Some(path) = self.save_path() {
                                    result = Some(PickerResult::Picked(vec![path]));
                                }
                            }
                        }
                        let mut hidden = self.show_hidden;
                        if widgets::toggle(ui, palette, &mut hidden).clicked() {
                            self.show_hidden = hidden;
                            self.refresh();
                        }
                        ui.label(
                            egui::RichText::new(crate::i18n::t("Show hidden"))
                                .size(11.5)
                                .color(palette.text_secondary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                                result = Some(PickerResult::Cancelled);
                            }
                            let label = match self.mode {
                                PickerMode::OpenFolder => "Use folder",
                                PickerMode::SaveFile => "Save",
                                _ => crate::i18n::t("Select"),
                            };
                            if widgets::button(ui, palette, label, Some(Icon::Check), ButtonVariant::Primary).clicked() {
                                match self.mode {
                                    PickerMode::OpenFolder => {
                                        result = Some(PickerResult::Picked(vec![self.current_dir.clone()]));
                                    }
                                    PickerMode::SaveFile => {
                                        if let Some(path) = self.save_path() {
                                            result = Some(PickerResult::Picked(vec![path]));
                                        }
                                    }
                                    _ => {
                                        if !self.selected.is_empty() {
                                            let mut paths: Vec<PathBuf> =
                                                self.selected.iter().cloned().collect();
                                            paths.sort();
                                            result = Some(PickerResult::Picked(paths));
                                        }
                                    }
                                }
                            }
                        });
                    });
                });
            });
        if let Some(PickerResult::Cancelled) = result {
            open = false;
            let _ = open;
            return Some(PickerResult::Cancelled);
        }
        if result.is_none() && !open {
            return Some(PickerResult::Cancelled);
        }
        // Overwrite guard for save mode.
        if let Some(PickerResult::Picked(paths)) = &result {
            if self.mode == PickerMode::SaveFile && paths.first().is_some_and(|p| p.exists()) && !self.confirm_overwrite {
                self.confirm_overwrite = true;
                self.error = Some(crate::i18n::t("That file already exists. Press Save again to replace it.").into());
                return Some(PickerResult::None);
            }
        }
        result
    }

    fn save_path(&self) -> Option<PathBuf> {
        let name = self.filename.trim();
        if name.is_empty() {
            return None;
        }
        let mut path = self.current_dir.join(name);
        if self.extensions.len() == 1 && path.extension().is_none() {
            path.set_extension(&self.extensions[0]);
        }
        Some(path)
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}