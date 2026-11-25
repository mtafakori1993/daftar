//! Settings: general, reading typography/theme presets, diagrams, library, shortcuts, about.

use daftar_core::direction::Direction;
use daftar_core::settings::{ReadingPreset, ReadingTheme, SHORTCUT_ACTIONS, TextAlign, ThemeMode};
use egui::Ui;

use crate::app::{DaftarApp, PickerPurpose};
use crate::picker::PickerMode;
use crate::theme::Palette;
use crate::{Icon, widgets};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Reading,
    Diagrams,
    Library,
    Keyboard,
    About,
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub tab: SettingsTab,
    pub new_preset: String,
    pub new_theme: String,
    pub font_filter: String,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            tab: SettingsTab::General,
            new_preset: String::new(),
            new_theme: String::new(),
            font_filter: String::new(),
        }
    }
}

fn settings_tab_label(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::General => "General",
        SettingsTab::Reading => "Reading",
        SettingsTab::Diagrams => "Diagrams",
        SettingsTab::Library => "Library",
        SettingsTab::Keyboard => "Keyboard",
        SettingsTab::About => "About",
    }
}

fn responsive_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    if ui.available_width() < 660.0 {
        ui.horizontal_wrapped(add)
    } else {
        ui.horizontal(add)
    }
}

impl DaftarApp {
    pub(crate) fn show_settings(&mut self, ui: &mut Ui, palette: &Palette) {
        ui.add_space(10.0);
        ui.heading(crate::i18n::t("Settings"));
        ui.add_space(6.0);
        let tabs = [
            (SettingsTab::General, "General", Icon::Settings),
            (SettingsTab::Reading, "Reading", Icon::Type),
            (SettingsTab::Diagrams, "Diagrams", Icon::Grid),
            (SettingsTab::Library, "Library", Icon::Library),
            (SettingsTab::Keyboard, "Keyboard", Icon::Keyboard),
            (SettingsTab::About, "About", Icon::Info),
        ];

        if ui.available_width() < 720.0 {
            egui::ComboBox::from_id_salt("settings-tab")
                .selected_text(crate::i18n::t(settings_tab_label(self.settings_state.tab)))
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for (tab, label, _) in tabs {
                        ui.selectable_value(
                            &mut self.settings_state.tab,
                            tab,
                            crate::i18n::t(label),
                        );
                    }
                });
            ui.add_space(8.0);
            self.show_settings_tab_content(ui, palette);
        } else {
            ui.horizontal_top(|ui| {
                ui.scope_builder(
                    egui::UiBuilder::new().layout(crate::rtl::root_layout()),
                    |ui| {
                        ui.set_width(180.0);
                        for (tab, label, icon) in tabs {
                            if widgets::nav_item(
                                ui,
                                palette,
                                icon,
                                label,
                                self.settings_state.tab == tab,
                                None,
                            )
                            .clicked()
                            {
                                self.settings_state.tab = tab;
                            }
                        }
                    },
                );
                ui.separator();
                ui.scope_builder(
                    egui::UiBuilder::new().layout(crate::rtl::root_layout()),
                    |ui| {
                        ui.set_width(ui.available_width());
                        self.show_settings_tab_content(ui, palette);
                    },
                );
            });
        }
    }

    fn show_settings_tab_content(&mut self, ui: &mut Ui, palette: &Palette) {
        match self.settings_state.tab {
            SettingsTab::General => self.settings_general(ui, palette),
            SettingsTab::Reading => self.settings_reading(ui, palette),
            SettingsTab::Diagrams => self.settings_diagrams(ui, palette),
            SettingsTab::Library => self.settings_library(ui, palette),
            SettingsTab::Keyboard => self.settings_keyboard(ui, palette),
            SettingsTab::About => self.settings_about(ui, palette),
        }
    }

    fn settings_general(&mut self, ui: &mut Ui, palette: &Palette) {
        widgets::section_label(ui, palette, "Appearance");
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Theme"));
            egui::ComboBox::from_id_salt("theme-mode")
                .selected_text(crate::i18n::t(self.settings.general.theme.label()))
                .show_ui(ui, |ui| {
                    for mode in [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark] {
                        if ui
                            .selectable_value(
                                &mut self.settings.general.theme,
                                mode,
                                crate::i18n::t(mode.label()),
                            )
                            .changed()
                        {
                            self.mark_dirty();
                            self.resolve_dark(ui.ctx());
                        }
                    }
                });
        });
        ui.add_space(6.0);
        widgets::section_label(ui, palette, "Language");
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Interface"));
            let mut language = crate::i18n::normalize(&self.settings.general.language).to_string();
            egui::ComboBox::from_id_salt("language")
                .selected_text(crate::i18n::language_name(&language))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_value(
                            &mut language,
                            "en".to_string(),
                            crate::i18n::t("English"),
                        )
                        .changed()
                    {
                        self.settings.general.language = language.clone();
                        crate::i18n::set_language(&language);
                        self.mark_dirty();
                    }
                    if ui
                        .selectable_value(&mut language, "fa".to_string(), "فارسی")
                        .changed()
                    {
                        self.settings.general.language = language.clone();
                        crate::i18n::set_language(&language);
                        self.mark_dirty();
                    }
                });
        });
        widgets::hint(
            ui,
            palette,
            "The interface is currently English. Markdown reading, selection, highlights, flashcards and exports fully support Persian/Arabic and mixed RTL content.",
        );
        ui.add_space(6.0);
        widgets::section_label(ui, palette, "Startup");
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Open"));
            egui::ComboBox::from_id_salt("startup")
                .selected_text(match self.settings.general.startup_view.as_str() {
                    "last" => crate::i18n::t("Last view"),
                    "search" => crate::i18n::t("Search"),
                    "roadmaps" => crate::i18n::t("Roadmaps"),
                    _ => crate::i18n::t("Library"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.settings.general.startup_view,
                        "library".into(),
                        crate::i18n::t("Library"),
                    );
                    ui.selectable_value(
                        &mut self.settings.general.startup_view,
                        "search".into(),
                        crate::i18n::t("Search"),
                    );
                    ui.selectable_value(
                        &mut self.settings.general.startup_view,
                        "roadmaps".into(),
                        crate::i18n::t("Roadmaps"),
                    );
                });
        });
        ui.checkbox(
            &mut self.settings.general.reopen_last_document,
            crate::i18n::t("Remember last document"),
        );
        ui.checkbox(
            &mut self.settings.general.confirm_on_delete,
            crate::i18n::t("Confirm destructive actions"),
        );
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Data");
        widgets::hint(
            ui,
            palette,
            &crate::i18n::tf(
                "Data directory: {}",
                &[&self.config.data_dir.display().to_string()],
            ),
        );
        widgets::hint(
            ui,
            palette,
            &crate::i18n::tf(
                "Library location: {}",
                &[&self.settings.general.default_library_root],
            ),
        );
        responsive_row(ui, |ui| {
            widgets::meta_pill(
                ui,
                palette,
                if self.config.portable {
                    "Portable mode"
                } else {
                    "Standard mode"
                },
                palette.accent,
            );
            if widgets::button(
                ui,
                palette,
                "Change library…",
                Some(Icon::FolderOpen),
                widgets::ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.open_picker(
                    PickerPurpose::AddLibrary,
                    PickerMode::OpenFolder,
                    crate::i18n::t("Choose library folder"),
                    &[],
                    "",
                );
            }
        });
    }

    fn settings_reading(&mut self, ui: &mut Ui, palette: &Palette) {
        let families = self.engine.families();
        let mut typography = self.settings.reading.typography.clone();
        widgets::section_label(ui, palette, "Typography");
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Reading font"));
            egui::ComboBox::from_id_salt("font-family")
                .width(240.0)
                .selected_text(typography.font_family.clone())
                .show_ui(ui, |ui| {
                    for family in &families {
                        ui.selectable_value(&mut typography.font_family, family.clone(), family);
                    }
                });
            if widgets::button(
                ui,
                palette,
                "Import font…",
                Some(Icon::Upload),
                widgets::ButtonVariant::Quiet,
            )
            .clicked()
            {
                self.open_picker(
                    PickerPurpose::ImportFont,
                    PickerMode::OpenFile,
                    crate::i18n::t("Import a font file"),
                    &["ttf", "otf", "ttc", "otc"],
                    "",
                );
            }
        });
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Code font"));
            egui::ComboBox::from_id_salt("code-font")
                .width(240.0)
                .selected_text(typography.code_font_family.clone())
                .show_ui(ui, |ui| {
                    for family in &families {
                        ui.selectable_value(
                            &mut typography.code_font_family,
                            family.clone(),
                            family,
                        );
                    }
                });
        });
        ui.add(
            egui::Slider::new(&mut typography.font_size, 11.0..=34.0)
                .text(crate::i18n::t("Font size")),
        );
        ui.add(
            egui::Slider::new(&mut typography.font_weight, 100..=900)
                .step_by(100.0)
                .text(crate::i18n::t("Font weight")),
        );
        ui.add(
            egui::Slider::new(&mut typography.line_height, 1.1..=3.0)
                .text(crate::i18n::t("Line height")),
        );
        ui.add(
            egui::Slider::new(&mut typography.paragraph_spacing, 0.0..=48.0)
                .text(crate::i18n::t("Paragraph spacing")),
        );
        ui.add(
            egui::Slider::new(&mut typography.letter_spacing, -1.0..=4.0)
                .text(crate::i18n::t("Letter spacing")),
        );
        ui.add(
            egui::Slider::new(&mut typography.heading_spacing, 0.0..=64.0)
                .text(crate::i18n::t("Heading spacing")),
        );
        // Reading width: presets, slider and numeric entry. The presets and the custom
        // width are the global default; individual documents can override them from the
        // reader toolbar.
        widgets::section_label(ui, palette, "Reading width");
        ui.horizontal_wrapped(|ui| {
            for preset in daftar_core::settings::WidthPreset::ALL {
                let selected = typography.width_preset == preset;
                if widgets::chip(ui, palette, preset.label(), selected).clicked() {
                    typography.width_preset = preset;
                    self.mark_dirty();
                }
            }
        });
        let custom = typography.width_preset == daftar_core::settings::WidthPreset::Custom;
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Custom width"));
            let mut width = typography.reading_width;
            let value = ui.add(
                egui::DragValue::new(&mut width)
                    .range(320.0..=1400.0)
                    .speed(4.0)
                    .suffix(" px"),
            );
            if value.changed() {
                typography.reading_width = width;
                typography.width_preset = daftar_core::settings::WidthPreset::Custom;
                self.mark_dirty();
            }
            let slider = ui.add_enabled(
                custom,
                egui::Slider::new(&mut typography.reading_width, 320.0..=1400.0).show_value(false),
            );
            if slider.changed() {
                typography.width_preset = daftar_core::settings::WidthPreset::Custom;
                self.mark_dirty();
            }
        });
        widgets::hint(
            ui,
            palette,
            "Presets apply to every document; the reader toolbar can override the width for one document.",
        );
        ui.add(
            egui::Slider::new(&mut typography.code_font_size, 9.0..=26.0)
                .text(crate::i18n::t("Code font size")),
        );
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Text alignment"));
            egui::ComboBox::from_id_salt("text-align")
                .selected_text(crate::i18n::t(typography.text_align.label()))
                .show_ui(ui, |ui| {
                    for align in [
                        TextAlign::Start,
                        TextAlign::Left,
                        TextAlign::Right,
                        TextAlign::Center,
                        TextAlign::Justify,
                    ] {
                        ui.selectable_value(
                            &mut typography.text_align,
                            align,
                            crate::i18n::t(align.label()),
                        );
                    }
                });
        });
        responsive_row(ui, |ui| {
            ui.label(crate::i18n::t("Default direction"));
            for direction in [Direction::Auto, Direction::Rtl, Direction::Ltr] {
                ui.radio_value(
                    &mut self.settings.reading.default_direction,
                    direction,
                    crate::i18n::t(direction.label()),
                );
            }
        });
        if typography != self.settings.reading.typography {
            self.settings.reading.typography = typography;
            self.mark_dirty();
        }
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Reading theme");
        responsive_row(ui, |ui| {
            let themes = self.settings.reading_themes.clone();
            let current = self.settings.theme().name;
            let mut selected = self.settings.reading.theme_id.clone();
            egui::ComboBox::from_id_salt("reading-theme")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for theme in &themes {
                        ui.selectable_value(&mut selected, theme.id.clone(), &theme.name);
                    }
                });
            if selected != self.settings.reading.theme_id {
                self.settings.reading.theme_id = selected;
                self.mark_dirty();
            }
            if widgets::button(
                ui,
                palette,
                "Duplicate theme",
                Some(Icon::Plus),
                widgets::ButtonVariant::Quiet,
            )
            .clicked()
            {
                let mut theme = self.settings.theme();
                theme.id = format!("custom-{}", uuid::Uuid::new_v4());
                theme.name = self
                    .settings
                    .unique_preset_name(crate::i18n::t("Custom theme"));
                theme.builtin = false;
                self.settings.reading.theme_id = theme.id.clone();
                self.settings.reading_themes.push(theme);
                self.mark_dirty();
            }
            if widgets::button(
                ui,
                palette,
                "Delete theme",
                Some(Icon::Trash),
                widgets::ButtonVariant::Quiet,
            )
            .clicked()
            {
                if !self.settings.theme().builtin {
                    let id = self.settings.reading.theme_id.clone();
                    self.settings.reading_themes.retain(|t| t.id != id);
                    self.settings.reading.theme_id = "spectrum-light".into();
                    self.mark_dirty();
                }
            }
        });
        // Colour editor (duplicates built-ins before editing so shipped themes stay canonical).
        let mut theme = self.settings.theme();
        let mut changed = false;
        let mut edit_color = |ui: &mut Ui, label: &str, value: &mut String| {
            let mut rgb = ReadingTheme::parse_color(value).unwrap_or([255, 255, 255]);
            responsive_row(ui, |ui| {
                ui.label(label);
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    *value = format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]);
                    changed = true;
                }
            });
        };
        egui::CollapsingHeader::new("Theme colours").show(ui, |ui| {
            edit_color(ui, crate::i18n::t("Background"), &mut theme.background);
            edit_color(ui, crate::i18n::t("Text"), &mut theme.text);
            edit_color(ui, crate::i18n::t("Muted"), &mut theme.muted);
            edit_color(ui, crate::i18n::t("Headings"), &mut theme.heading);
            edit_color(ui, crate::i18n::t("Links"), &mut theme.link);
            edit_color(ui, crate::i18n::t("Selection"), &mut theme.selection);
            edit_color(
                ui,
                crate::i18n::t("Code background"),
                &mut theme.code_background,
            );
            edit_color(ui, crate::i18n::t("Code text"), &mut theme.code_text);
            edit_color(
                ui,
                crate::i18n::t("Quote background"),
                &mut theme.quote_background,
            );
            edit_color(ui, crate::i18n::t("Quote border"), &mut theme.quote_border);
            edit_color(ui, crate::i18n::t("Table border"), &mut theme.table_border);
        });
        if changed {
            if theme.builtin {
                theme.id = format!("custom-{}", uuid::Uuid::new_v4());
                theme.name = self
                    .settings
                    .unique_preset_name(crate::i18n::t("Custom theme"));
                theme.builtin = false;
                self.settings.reading_themes.push(theme.clone());
            } else if let Some(existing) = self
                .settings
                .reading_themes
                .iter_mut()
                .find(|t| t.id == theme.id)
            {
                *existing = theme.clone();
            }
            self.settings.reading.theme_id = theme.id;
            self.mark_dirty();
        }
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Reading presets");
        responsive_row(ui, |ui| {
            widgets::text_field(
                ui,
                palette,
                &mut self.settings_state.new_preset,
                crate::i18n::t("Preset name"),
                220.0,
            );
            if widgets::button(
                ui,
                palette,
                "Save preset",
                Some(Icon::Save),
                widgets::ButtonVariant::Secondary,
            )
            .clicked()
            {
                let name = if self.settings_state.new_preset.trim().is_empty() {
                    self.settings
                        .unique_preset_name(crate::i18n::t("My preset"))
                } else {
                    self.settings
                        .unique_preset_name(self.settings_state.new_preset.trim())
                };
                let preset = ReadingPreset::new(
                    name,
                    self.settings.reading.typography.clone(),
                    self.settings.reading.theme_id.clone(),
                );
                self.settings.presets.push(preset);
                self.settings_state.new_preset.clear();
                self.mark_dirty();
            }
        });
        let presets = self.settings.presets.clone();
        for preset in presets {
            responsive_row(ui, |ui| {
                if widgets::button(
                    ui,
                    palette,
                    &preset.name,
                    Some(Icon::Type),
                    widgets::ButtonVariant::Quiet,
                )
                .clicked()
                {
                    self.settings.reading.typography = preset.typography.clone();
                    self.settings.reading.theme_id = preset.theme_id.clone();
                    self.mark_dirty();
                }
                if widgets::icon_button_sized(
                    ui,
                    palette,
                    Icon::Trash,
                    "Delete preset",
                    false,
                    22.0,
                )
                .clicked()
                {
                    self.settings.presets.retain(|p| p.id != preset.id);
                    self.mark_dirty();
                }
            });
        }
    }

    fn settings_diagrams(&mut self, ui: &mut Ui, palette: &Palette) {
        widgets::section_label(ui, palette, "Diagram rendering");
        ui.checkbox(
            &mut self.settings.diagrams.fit_to_width,
            crate::i18n::t("Fit diagrams to the reading column"),
        );
        ui.checkbox(
            &mut self.settings.diagrams.dark_mode_diagrams,
            crate::i18n::t("Use dark diagram palette in dark mode"),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.diagrams.max_height, 180.0..=1200.0)
                .text(crate::i18n::t("Maximum diagram height")),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.diagrams.default_zoom, 0.25..=4.0)
                .text(crate::i18n::t("Default zoom")),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.diagrams.pan_speed, 0.2..=3.0)
                .text(crate::i18n::t("Pan speed")),
        );
        ui.checkbox(
            &mut self.settings.diagrams.show_edge_labels,
            crate::i18n::t("Show edge labels where available"),
        );
        ui.add_space(6.0);
        widgets::hint(
            ui,
            palette,
            "Native Mermaid subset: flowchart/graph, sequence, class, state, ER, gantt, pie, mindmap and journey. Unsupported directives show an explicit error instead of a fake diagram.",
        );
        if ui
            .button(crate::i18n::t("Mark diagram settings dirty"))
            .clicked()
        {
            self.mark_dirty();
        }
        self.mark_dirty();
    }

    fn settings_library(&mut self, ui: &mut Ui, palette: &Palette) {
        widgets::section_label(ui, palette, "Monitoring");
        ui.checkbox(
            &mut self.settings.library.watch_files,
            crate::i18n::t("Watch library files for external edits"),
        );
        ui.checkbox(
            &mut self.settings.library.auto_index,
            crate::i18n::t("Index new and changed Markdown files"),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.library.index_debounce_ms, 100..=5000)
                .text(crate::i18n::t("File watcher debounce (ms)")),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.library.recents_limit, 3..=50)
                .text(crate::i18n::t("Recent documents shown")),
        );
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Backups");
        ui.checkbox(
            &mut self.settings.library.backup_on_startup,
            crate::i18n::t("Back up the database on startup"),
        );
        ui.add(
            egui::Slider::new(&mut self.settings.library.backups_to_keep, 1..=50)
                .text(crate::i18n::t("Backups to keep")),
        );
        responsive_row(ui, |ui| {
            if widgets::button(
                ui,
                palette,
                "Back up now",
                Some(Icon::Save),
                widgets::ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.backup_now(false);
            }
            widgets::hint(
                ui,
                palette,
                &format!("{}", self.config.backups_dir.display()),
            );
        });
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Archive");
        responsive_row(ui, |ui| {
            if widgets::button(
                ui,
                palette,
                "Export library…",
                Some(Icon::Export),
                widgets::ButtonVariant::Primary,
            )
            .clicked()
            {
                self.open_picker(
                    PickerPurpose::ExportLibrary,
                    PickerMode::SaveFile,
                    crate::i18n::t("Export library as ZIP"),
                    &["zip"],
                    "daftar-library.zip",
                );
            }
            if widgets::button(
                ui,
                palette,
                "Import archive…",
                Some(Icon::Import),
                widgets::ButtonVariant::Secondary,
            )
            .clicked()
            {
                self.open_picker(
                    PickerPurpose::ImportLibraryArchive,
                    PickerMode::OpenFile,
                    crate::i18n::t("Import Daftar library ZIP"),
                    &["zip"],
                    "",
                );
            }
        });
        widgets::hint(
            ui,
            palette,
            "Archives contain ordinary Markdown files, attachments, a SQLite snapshot, and a versioned manifest.",
        );
        self.mark_dirty();
    }

    fn settings_keyboard(&mut self, ui: &mut Ui, palette: &Palette) {
        widgets::section_label(ui, palette, "Keyboard shortcuts");
        widgets::hint(
            ui,
            palette,
            "Use Ctrl, Shift, Alt and a key name, for example `Ctrl+Shift+H`.",
        );
        for (action, label, default) in SHORTCUT_ACTIONS {
            responsive_row(ui, |ui| {
                ui.label(crate::i18n::t(label));
                let mut value = self.settings.shortcut(action).to_string();
                if widgets::text_field(ui, palette, &mut value, default, 180.0).changed() {
                    self.settings.shortcuts.insert((*action).to_string(), value);
                    self.mark_dirty();
                }
            });
        }
        if widgets::button(
            ui,
            palette,
            "Reset shortcuts",
            Some(Icon::Refresh),
            widgets::ButtonVariant::Quiet,
        )
        .clicked()
        {
            self.settings.shortcuts.clear();
            for (action, _, combo) in SHORTCUT_ACTIONS {
                self.settings
                    .shortcuts
                    .insert((*action).to_string(), (*combo).to_string());
            }
            self.mark_dirty();
        }
    }

    fn settings_about(&mut self, ui: &mut Ui, palette: &Palette) {
        widgets::section_label(ui, palette, "Daftar");
        ui.label(
            egui::RichText::new(crate::i18n::tf("Version {}", &[env!("CARGO_PKG_VERSION")]))
                .size(14.0)
                .strong(),
        );
        widgets::hint(
            ui,
            palette,
            "A native Rust desktop Markdown reader and knowledge studio. No JavaScript, WebView, Electron or Tauri.",
        );
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Implementation");
        widgets::hint(
            ui,
            palette,
            "egui/eframe · cosmic-text (HarfBuzz-class shaping + Unicode bidi) · rustybuzz · SQLite/FTS5 · syntect · tiny-skia · pdf-writer with embedded TrueType fonts.",
        );
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Honest limitations");
        for limitation in [
            "Mermaid support is a native subset; unsupported directives display an explicit error.",
            "Mathematical expressions use a practical TeX-to-Unicode fallback rather than a full MathJax engine.",
            "The interface language can be English or Persian; document content always keeps full RTL/LTR support.",
            "PDF export embeds full OFL fonts rather than subsetting them (larger files, correct shaping).",
            "Table text is rendered but selection/highlighting inside tables is not supported.",
        ] {
            ui.label(
                egui::RichText::new(crate::i18n::tf("• {}", &[crate::i18n::t(limitation)]))
                    .size(11.5)
                    .color(palette.text_secondary),
            );
        }
        ui.add_space(8.0);
        widgets::section_label(ui, palette, "Licenses");
        ui.label(egui::RichText::new(crate::i18n::t("Daftar is MIT licensed. Bundled fonts: Inter, Vazirmatn, JetBrains Mono (SIL OFL). See assets/fonts license files and LICENSE.")).size(11.0).color(palette.text_secondary));
        let _ = self.store.integrity_check().map(|v| {
            widgets::hint(
                ui,
                palette,
                &crate::i18n::tf("Database integrity: {}", &[&v.to_string()]),
            );
        });
    }
}
