//! User preferences: typography, reading themes, diagrams, library, keyboard shortcuts.
//!
//! Settings are stored as TOML next to the database (portable mode) or in the platform data
//! directory. Everything is serde-defaulted so older configs survive upgrades.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::direction::Direction;
use crate::util::now_ms;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeMode {
    pub fn label(self) -> &'static str {
        match self {
            ThemeMode::Light => "Light",
            ThemeMode::Dark => "Dark",
            ThemeMode::System => "Match system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Start,
    Left,
    Right,
    Center,
    Justify,
}

impl TextAlign {
    pub fn label(self) -> &'static str {
        match self {
            TextAlign::Start => "Document direction",
            TextAlign::Left => "Left",
            TextAlign::Right => "Right",
            TextAlign::Center => "Center",
            TextAlign::Justify => "Justify",
        }
    }
}

/// Reading typography, independent of chrome/UI typography.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Typography {
    /// Font family name resolved through the bundled and system font database.
    pub font_family: String,
    pub font_size: f32,
    /// 100–900.
    pub font_weight: u16,
    /// Multiplier of font size.
    pub line_height: f32,
    /// Extra pixels between paragraphs.
    pub paragraph_spacing: f32,
    /// Extra pixels between letters (converted to em internally).
    pub letter_spacing: f32,
    /// Heading gap in pixels.
    pub heading_spacing: f32,
    pub text_align: TextAlign,
    /// Maximum measure in pixels; used by [`WidthPreset::Custom`] and as a fine adjustment.
    pub reading_width: f32,
    /// Named reading width preset.
    #[serde(default)]
    pub width_preset: WidthPreset,
    pub code_font_family: String,
    pub code_font_size: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            font_family: "Vazirmatn".to_string(),
            font_size: 17.0,
            font_weight: 400,
            line_height: 1.85,
            paragraph_spacing: 14.0,
            letter_spacing: 0.0,
            heading_spacing: 18.0,
            text_align: TextAlign::Start,
            reading_width: 760.0,
            width_preset: WidthPreset::Comfortable,
            code_font_family: "JetBrains Mono".to_string(),
            code_font_size: 13.5,
        }
    }
}

impl Typography {
    /// The measure the reader should use for `available` pixels of reading area.
    ///
    /// Named presets never exceed the available width, so the document never overflows
    /// horizontally; [`WidthPreset::Full`] uses the whole reading area.
    pub fn effective_width(&self, available: f32) -> f32 {
        self.resolve_width(None, None, available)
    }

    /// Resolve the measure honouring per-document overrides.
    ///
    /// Precedence: per-document preset → per-document pixel width (treated as custom) →
    /// global preset / global custom width.
    pub fn resolve_width(
        &self,
        preset_override: Option<WidthPreset>,
        width_override: Option<f32>,
        available: f32,
    ) -> f32 {
        let available = available.max(160.0);
        let clamp = |px: f32| px.clamp(320.0, 2000.0).min(available);
        match (preset_override, width_override) {
            (Some(WidthPreset::Custom), _) => clamp(width_override.unwrap_or(self.reading_width)),
            (Some(preset), _) => match preset {
                WidthPreset::Full => available,
                other => clamp(other.nominal_width().unwrap_or(self.reading_width)),
            },
            (None, Some(px)) => clamp(px),
            (None, None) => match self.width_preset {
                WidthPreset::Full => available,
                WidthPreset::Custom => clamp(self.reading_width),
                preset => clamp(preset.nominal_width().unwrap_or(self.reading_width)),
            },
        }
    }

    pub fn validate(&mut self) {
        self.font_size = self.font_size.clamp(11.0, 34.0);
        self.font_weight = self.font_weight.clamp(100, 900);
        self.line_height = self.line_height.clamp(1.1, 3.0);
        self.paragraph_spacing = self.paragraph_spacing.clamp(0.0, 48.0);
        self.letter_spacing = self.letter_spacing.clamp(-1.0, 4.0);
        self.heading_spacing = self.heading_spacing.clamp(0.0, 64.0);
        self.reading_width = self.reading_width.clamp(320.0, 2000.0);
        self.code_font_size = self.code_font_size.clamp(9.0, 26.0);
    }
}

/// Reading width presets offered in the reader toolbar and in Settings → Reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidthPreset {
    /// 520 px — long-form, minimal eye travel.
    Narrow,
    /// 760 px — the default measure for comfortable reading.
    #[default]
    Comfortable,
    /// 1040 px — dense technical documents.
    Wide,
    /// The full width of the reading area.
    Full,
    /// Use [`Typography::reading_width`] exactly.
    Custom,
}

impl WidthPreset {
    pub const ALL: [WidthPreset; 5] = [
        WidthPreset::Narrow,
        WidthPreset::Comfortable,
        WidthPreset::Wide,
        WidthPreset::Full,
        WidthPreset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WidthPreset::Narrow => "Narrow",
            WidthPreset::Comfortable => "Comfortable",
            WidthPreset::Wide => "Wide",
            WidthPreset::Full => "Full width",
            WidthPreset::Custom => "Custom",
        }
    }

    /// Nominal measure in pixels, or `None` for [`WidthPreset::Full`] and [`WidthPreset::Custom`].
    pub fn nominal_width(self) -> Option<f32> {
        match self {
            WidthPreset::Narrow => Some(520.0),
            WidthPreset::Comfortable => Some(760.0),
            WidthPreset::Wide => Some(1040.0),
            WidthPreset::Full | WidthPreset::Custom => None,
        }
    }

    /// Storage representation used by the document table.
    pub fn as_db(self) -> &'static str {
        match self {
            WidthPreset::Narrow => "narrow",
            WidthPreset::Comfortable => "comfortable",
            WidthPreset::Wide => "wide",
            WidthPreset::Full => "full",
            WidthPreset::Custom => "custom",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "narrow" => WidthPreset::Narrow,
            "wide" => WidthPreset::Wide,
            "full" => WidthPreset::Full,
            "custom" => WidthPreset::Custom,
            _ => WidthPreset::Comfortable,
        }
    }

    /// The preset implied by a width, if it matches one of the named measures.
    pub fn from_width(width: f32) -> Self {
        for preset in [
            WidthPreset::Narrow,
            WidthPreset::Comfortable,
            WidthPreset::Wide,
        ] {
            if let Some(nominal) = preset.nominal_width() {
                if (nominal - width).abs() < 1.0 {
                    return preset;
                }
            }
        }
        WidthPreset::Custom
    }
}

/// A named, reusable typography + theme combination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingPreset {
    pub id: String,
    pub name: String,
    pub typography: Typography,
    pub theme_id: String,
}

impl ReadingPreset {
    pub fn new(name: impl Into<String>, typography: Typography, theme_id: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            typography,
            theme_id: theme_id.into(),
        }
    }
}

/// A complete reading colour theme. Colours are `#rrggbb` hex strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingTheme {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub background: String,
    pub text: String,
    pub muted: String,
    pub heading: String,
    pub link: String,
    pub selection: String,
    pub code_background: String,
    pub code_text: String,
    pub code_border: String,
    pub quote_background: String,
    pub quote_border: String,
    pub table_border: String,
    pub table_header_background: String,
    pub rule: String,
    pub highlight_palette: Vec<String>,
    /// Built-in themes are always available and cannot be removed.
    #[serde(default)]
    pub builtin: bool,
}

impl ReadingTheme {
    pub fn light_default() -> Self {
        Self {
            id: "spectrum-light".to_string(),
            name: "Spectrum Light".to_string(),
            dark: false,
            background: "#FFFFFF".to_string(),
            text: "#2C2C2C".to_string(),
            muted: "#6E6E6E".to_string(),
            heading: "#1A1A1A".to_string(),
            link: "#0B6BCB".to_string(),
            selection: "#CFE3FA".to_string(),
            code_background: "#F5F5F5".to_string(),
            code_text: "#2C2C2C".to_string(),
            code_border: "#E1E1E1".to_string(),
            quote_background: "#F8F8F8".to_string(),
            quote_border: "#CACACA".to_string(),
            table_border: "#E1E1E1".to_string(),
            table_header_background: "#F5F5F5".to_string(),
            rule: "#E1E1E1".to_string(),
            highlight_palette: vec![
                "#FFD966".to_string(),
                "#9BE7A8".to_string(),
                "#A8D4FF".to_string(),
                "#F5A8D0".to_string(),
                "#D5B8FF".to_string(),
            ],
            builtin: true,
        }
    }

    pub fn dark_default() -> Self {
        Self {
            id: "spectrum-dark".to_string(),
            name: "Spectrum Dark".to_string(),
            dark: true,
            background: "#1E1E1E".to_string(),
            text: "#E1E1E1".to_string(),
            muted: "#9A9A9A".to_string(),
            heading: "#F5F5F5".to_string(),
            link: "#6BB8FF".to_string(),
            selection: "#0B4F8A".to_string(),
            code_background: "#252525".to_string(),
            code_text: "#E1E1E1".to_string(),
            code_border: "#3A3A3A".to_string(),
            quote_background: "#242424".to_string(),
            quote_border: "#5A5A5A".to_string(),
            table_border: "#3A3A3A".to_string(),
            table_header_background: "#252525".to_string(),
            rule: "#3A3A3A".to_string(),
            highlight_palette: vec![
                "#8A6D1F".to_string(),
                "#2F6B3C".to_string(),
                "#285A8A".to_string(),
                "#7A3A60".to_string(),
                "#5A3F80".to_string(),
            ],
            builtin: true,
        }
    }

    pub fn sepia() -> Self {
        Self {
            id: "sepia".to_string(),
            name: "Sepia Paper".to_string(),
            dark: false,
            background: "#FBF6EC".to_string(),
            text: "#3A3226".to_string(),
            muted: "#7A6E5B".to_string(),
            heading: "#2A241B".to_string(),
            link: "#8A5A1B".to_string(),
            selection: "#EAD9B0".to_string(),
            code_background: "#F3EBDD".to_string(),
            code_text: "#3A3226".to_string(),
            code_border: "#E2D5C0".to_string(),
            quote_background: "#F6EFE2".to_string(),
            quote_border: "#C9B99C".to_string(),
            table_border: "#E2D5C0".to_string(),
            table_header_background: "#F3EBDD".to_string(),
            rule: "#E2D5C0".to_string(),
            highlight_palette: vec![
                "#F2D27A".to_string(),
                "#B8DFA8".to_string(),
                "#B6D2F0".to_string(),
                "#EFC0D8".to_string(),
                "#D8C2F0".to_string(),
            ],
            builtin: true,
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            id: "high-contrast".to_string(),
            name: "High Contrast".to_string(),
            dark: true,
            background: "#000000".to_string(),
            text: "#FFFFFF".to_string(),
            muted: "#C8C8C8".to_string(),
            heading: "#FFFFFF".to_string(),
            link: "#7CC4FF".to_string(),
            selection: "#004C99".to_string(),
            code_background: "#111111".to_string(),
            code_text: "#FFFFFF".to_string(),
            code_border: "#666666".to_string(),
            quote_background: "#0D0D0D".to_string(),
            quote_border: "#888888".to_string(),
            table_border: "#666666".to_string(),
            table_header_background: "#111111".to_string(),
            rule: "#666666".to_string(),
            highlight_palette: vec![
                "#B58B00".to_string(),
                "#157A33".to_string(),
                "#005EA8".to_string(),
                "#8A2A5A".to_string(),
                "#5A2A8A".to_string(),
            ],
            builtin: true,
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![
            Self::light_default(),
            Self::dark_default(),
            Self::sepia(),
            Self::high_contrast(),
        ]
    }

    pub fn parse_color(hex: &str) -> Option<[u8; 3]> {
        let hex = hex.trim().trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r, g, b])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    /// `en` or `fa`. UI strings fall back to English when a translation is missing.
    pub language: String,
    pub theme: ThemeMode,
    /// Empty means the portable/platform default.
    pub default_library_root: String,
    pub portable_mode: bool,
    /// `last` or a view key such as `library`.
    pub startup_view: String,
    pub reopen_last_document: bool,
    pub confirm_on_delete: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: ThemeMode::System,
            default_library_root: String::new(),
            portable_mode: false,
            startup_view: "library".to_string(),
            reopen_last_document: true,
            confirm_on_delete: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingSettings {
    pub default_direction: Direction,
    pub typography: Typography,
    /// Id into `AppSettings::reading_themes`.
    pub theme_id: String,
    pub active_preset_id: Option<String>,
    pub remember_scroll_position: bool,
    pub show_reading_progress: bool,
    pub show_toc: bool,
}

impl Default for ReadingSettings {
    fn default() -> Self {
        Self {
            default_direction: Direction::Auto,
            typography: Typography::default(),
            theme_id: "spectrum-light".to_string(),
            active_preset_id: None,
            remember_scroll_position: true,
            show_reading_progress: true,
            show_toc: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagramSettings {
    pub default_zoom: f32,
    pub fit_to_width: bool,
    pub max_height: f32,
    pub dark_mode_diagrams: bool,
    /// Add node labels for unlabeled diagram edges when possible.
    pub show_edge_labels: bool,
    pub pan_speed: f32,
}

impl Default for DiagramSettings {
    fn default() -> Self {
        Self {
            default_zoom: 1.0,
            fit_to_width: true,
            max_height: 480.0,
            dark_mode_diagrams: true,
            show_edge_labels: true,
            pan_speed: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LibrarySettings {
    pub watch_files: bool,
    pub auto_index: bool,
    pub index_debounce_ms: u64,
    pub backup_on_startup: bool,
    pub backups_to_keep: u32,
    pub backup_interval_minutes: u64,
    pub recents_limit: u32,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            watch_files: true,
            auto_index: true,
            index_debounce_ms: 800,
            backup_on_startup: true,
            backups_to_keep: 10,
            backup_interval_minutes: 60,
            recents_limit: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowSettings {
    pub width: f32,
    pub height: f32,
    pub left_panel_width: f32,
    pub right_panel_width: f32,
    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub maximized: bool,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            width: 1360.0,
            height: 860.0,
            left_panel_width: 248.0,
            right_panel_width: 288.0,
            left_panel_open: true,
            right_panel_open: true,
            maximized: false,
        }
    }
}

pub const SHORTCUT_ACTIONS: &[(&str, &str, &str)] = &[
    ("open_file", "Open Markdown file", "Ctrl+O"),
    ("import_folder", "Add folder to library", "Ctrl+Shift+O"),
    ("search", "Search library", "Ctrl+F"),
    ("command_palette", "Command palette", "Ctrl+K"),
    ("toggle_left", "Toggle library panel", "Ctrl+B"),
    ("toggle_right", "Toggle outline panel", "Ctrl+Shift+B"),
    ("theme", "Toggle light/dark theme", "Ctrl+Shift+T"),
    ("font_increase", "Increase reading font size", "Ctrl+="),
    ("font_decrease", "Decrease reading font size", "Ctrl+-"),
    ("direction", "Cycle document direction", "Ctrl+Shift+R"),
    ("highlight", "Highlight selection", "Ctrl+Shift+H"),
    ("bookmark", "Bookmark position", "Ctrl+D"),
    ("flashcard", "Create flashcard", "Ctrl+Shift+C"),
    ("study_flip", "Study: flip card", "Space"),
    ("study_next", "Study: next card", "Right"),
    ("study_prev", "Study: previous card", "Left"),
    ("study_learned", "Study: mark learned", "L"),
    ("export", "Export current item", "Ctrl+E"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Settings schema version, bumped when a migration is required.
    pub version: u32,
    pub general: GeneralSettings,
    pub reading: ReadingSettings,
    pub diagrams: DiagramSettings,
    pub library: LibrarySettings,
    pub window: WindowSettings,
    pub shortcuts: BTreeMap<String, String>,
    pub reading_themes: Vec<ReadingTheme>,
    pub presets: Vec<ReadingPreset>,
    pub updated_at: i64,
}

impl Default for AppSettings {
    fn default() -> Self {
        let mut shortcuts = BTreeMap::new();
        for (action, _, combo) in SHORTCUT_ACTIONS {
            shortcuts.insert((*action).to_string(), (*combo).to_string());
        }
        Self {
            version: 1,
            general: GeneralSettings::default(),
            reading: ReadingSettings::default(),
            diagrams: DiagramSettings::default(),
            library: LibrarySettings::default(),
            window: WindowSettings::default(),
            shortcuts,
            reading_themes: ReadingTheme::builtins(),
            presets: Vec::new(),
            updated_at: now_ms(),
        }
    }
}

impl AppSettings {
    pub fn validate(&mut self) {
        if !self.general.language.eq_ignore_ascii_case("fa") {
            self.general.language = "en".to_string();
        } else {
            self.general.language = "fa".to_string();
        }
        self.reading.typography.validate();
        self.diagrams.default_zoom = self.diagrams.default_zoom.clamp(0.25, 4.0);
        self.diagrams.max_height = self.diagrams.max_height.clamp(180.0, 1400.0);
        self.diagrams.pan_speed = self.diagrams.pan_speed.clamp(0.2, 3.0);
        self.library.backups_to_keep = self.library.backups_to_keep.clamp(1, 100);
        self.library.index_debounce_ms = self.library.index_debounce_ms.clamp(100, 10_000);
        self.library.backup_interval_minutes = self.library.backup_interval_minutes.max(5);
        self.library.recents_limit = self.library.recents_limit.clamp(3, 100);
        self.window.width = self.window.width.clamp(900.0, 8000.0);
        self.window.height = self.window.height.clamp(600.0, 8000.0);
        self.window.left_panel_width = self.window.left_panel_width.clamp(180.0, 480.0);
        self.window.right_panel_width = self.window.right_panel_width.clamp(200.0, 560.0);
        for theme in ReadingTheme::builtins() {
            if !self.reading_themes.iter().any(|t| t.id == theme.id) {
                self.reading_themes.push(theme);
            }
        }
        if !self
            .reading_themes
            .iter()
            .any(|t| t.id == self.reading.theme_id)
        {
            self.reading.theme_id = "spectrum-light".to_string();
        }
    }

    pub fn theme(&self) -> ReadingTheme {
        self.reading_themes
            .iter()
            .find(|t| t.id == self.reading.theme_id)
            .cloned()
            .unwrap_or_else(ReadingTheme::light_default)
    }

    pub fn shortcut(&self, action: &str) -> &str {
        self.shortcuts
            .get(action)
            .map(String::as_str)
            .unwrap_or_else(|| {
                SHORTCUT_ACTIONS
                    .iter()
                    .find(|(a, _, _)| *a == action)
                    .map(|(_, _, c)| *c)
                    .unwrap_or("")
            })
    }

    pub fn to_toml(&self) -> anyhow::Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        let mut settings: Self = toml::from_str(text)?;
        settings.validate();
        Ok(settings)
    }

    /// Generate a distinct name for user presets.
    pub fn unique_preset_name(&self, base: &str) -> String {
        if !self.presets.iter().any(|p| p.name == base) {
            return base.to_string();
        }
        for n in 2..100 {
            let candidate = format!("{base} {n}");
            if !self.presets.iter().any(|p| p.name == candidate) {
                return candidate;
            }
        }
        format!("{base} {}", now_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_toml_round_trip() {
        let mut settings = AppSettings::default();
        settings.reading.typography.font_size = 21.0;
        settings.library.backups_to_keep = 42;
        let text = settings.to_toml().unwrap();
        let restored = AppSettings::from_toml(&text).unwrap();
        assert_eq!(restored.reading.typography.font_size, 21.0);
        assert_eq!(restored.library.backups_to_keep, 42);
        assert!(restored.reading_themes.len() >= 4);
    }

    #[test]
    fn validation_clamps_unsafe_values() {
        let mut settings = AppSettings::default();
        settings.reading.typography.font_size = -100.0;
        settings.reading.typography.line_height = 0.0;
        settings.validate();
        assert!(settings.reading.typography.font_size >= 11.0);
        assert!(settings.reading.typography.line_height >= 1.1);
    }

    #[test]
    fn missing_theme_falls_back() {
        let mut settings = AppSettings::default();
        settings.reading.theme_id = "does-not-exist".into();
        settings.validate();
        assert_eq!(settings.reading.theme_id, "spectrum-light");
    }

    #[test]
    fn shortcut_lookup_has_defaults() {
        let settings = AppSettings::default();
        assert_eq!(settings.shortcut("search"), "Ctrl+F");
    }

    #[test]
    fn default_persian_font_is_available() {
        assert_eq!(Typography::default().font_family, "Vazirmatn");
    }
}
#[cfg(test)]
mod width_tests {
    use super::*;

    #[test]
    fn presets_resolve_to_their_nominal_measure() {
        let mut typography = Typography::default();
        typography.width_preset = WidthPreset::Narrow;
        assert_eq!(typography.effective_width(1200.0), 520.0);
        typography.width_preset = WidthPreset::Comfortable;
        assert_eq!(typography.effective_width(1200.0), 760.0);
        typography.width_preset = WidthPreset::Wide;
        assert_eq!(typography.effective_width(1200.0), 1040.0);
        typography.width_preset = WidthPreset::Full;
        assert_eq!(typography.effective_width(1200.0), 1200.0);
    }

    #[test]
    fn presets_never_overflow_the_available_width() {
        let mut typography = Typography::default();
        typography.width_preset = WidthPreset::Wide;
        assert_eq!(typography.effective_width(700.0), 700.0);
        assert_eq!(typography.effective_width(100.0), 160.0);
    }

    #[test]
    fn custom_preset_uses_the_configured_measure() {
        let mut typography = Typography::default();
        typography.width_preset = WidthPreset::Custom;
        typography.reading_width = 900.0;
        assert_eq!(typography.effective_width(1200.0), 900.0);
        // Clamped to the available area and to the supported range.
        typography.reading_width = 1900.0;
        assert_eq!(typography.effective_width(1000.0), 1000.0);
        typography.reading_width = 100.0;
        assert_eq!(typography.effective_width(1000.0), 320.0);
    }

    #[test]
    fn per_document_overrides_take_precedence() {
        let mut typography = Typography::default();
        typography.width_preset = WidthPreset::Comfortable;
        assert_eq!(typography.resolve_width(Some(WidthPreset::Narrow), None, 1200.0), 520.0);
        assert_eq!(typography.resolve_width(None, Some(980.0), 1200.0), 980.0);
        assert_eq!(typography.resolve_width(Some(WidthPreset::Full), None, 830.0), 830.0);
        assert_eq!(
            typography.resolve_width(Some(WidthPreset::Custom), Some(640.0), 1200.0),
            640.0
        );
        // No override falls back to the global preset.
        assert_eq!(typography.resolve_width(None, None, 1200.0), 760.0);
    }

    #[test]
    fn preset_storage_round_trips() {
        for preset in WidthPreset::ALL {
            assert_eq!(WidthPreset::from_db(preset.as_db()), preset);
        }
        assert_eq!(WidthPreset::from_db("unknown"), WidthPreset::Comfortable);
    }

    #[test]
    fn settings_toml_round_trips_the_width_preset() {
        let mut settings = AppSettings::default();
        settings.reading.typography.width_preset = WidthPreset::Wide;
        settings.reading.typography.reading_width = 1000.0;
        let text = settings.to_toml().expect("serialize");
        let parsed = AppSettings::from_toml(&text).expect("parse");
        assert_eq!(parsed.reading.typography.width_preset, WidthPreset::Wide);
        assert_eq!(parsed.reading.typography.reading_width, 1000.0);
    }
}
