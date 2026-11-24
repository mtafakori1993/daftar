//! Adobe Spectrum-inspired design tokens and egui styling for Daftar.
//!
//! The palette is deliberately neutral, with a single blue accent, 4px spacing rhythm,
//! 6px component radii and accessible contrast in both light and dark themes.

use std::collections::BTreeMap;
use std::sync::Arc;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke,
    StrokeKind, Style, TextStyle, Theme, ThemePreference, Visuals, style::{WidgetVisuals, Widgets},
};

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub canvas: Color32,
    pub surface: Color32,
    pub surface_sunken: Color32,
    pub surface_hover: Color32,
    pub surface_raised: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_disabled: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_pressed: Color32,
    pub accent_subtle: Color32,
    pub on_accent: Color32,
    pub selected: Color32,
    pub danger: Color32,
    pub danger_subtle: Color32,
    pub warning: Color32,
    pub warning_subtle: Color32,
    pub success: Color32,
    pub success_subtle: Color32,
    pub shadow: Color32,
    pub overlay: Color32,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            dark: false,
            canvas: Color32::from_rgb(0xF5, 0xF5, 0xF5),
            surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            surface_sunken: Color32::from_rgb(0xF0, 0xF0, 0xF0),
            surface_hover: Color32::from_rgb(0xF2, 0xF2, 0xF2),
            surface_raised: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            text: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            text_secondary: Color32::from_rgb(0x5C, 0x5C, 0x5C),
            text_disabled: Color32::from_rgb(0xA0, 0xA0, 0xA0),
            border: Color32::from_rgb(0xE1, 0xE1, 0xE1),
            border_strong: Color32::from_rgb(0xC6, 0xC6, 0xC6),
            accent: Color32::from_rgb(0x26, 0x80, 0xEB),
            accent_hover: Color32::from_rgb(0x14, 0x73, 0xE6),
            accent_pressed: Color32::from_rgb(0x0D, 0x66, 0xD0),
            accent_subtle: Color32::from_rgb(0xEA, 0xF3, 0xFF),
            on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            selected: Color32::from_rgb(0xE8, 0xF1, 0xFC),
            danger: Color32::from_rgb(0xD7, 0x37, 0x3F),
            danger_subtle: Color32::from_rgb(0xFD, 0xEC, 0xED),
            warning: Color32::from_rgb(0xE6, 0x86, 0x19),
            warning_subtle: Color32::from_rgb(0xFF, 0xF4, 0xE5),
            success: Color32::from_rgb(0x2D, 0x9D, 0x78),
            success_subtle: Color32::from_rgb(0xE7, 0xF6, 0xF0),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 26),
            overlay: Color32::from_rgba_premultiplied(0, 0, 0, 110),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            canvas: Color32::from_rgb(0x17, 0x17, 0x17),
            surface: Color32::from_rgb(0x1F, 0x1F, 0x1F),
            surface_sunken: Color32::from_rgb(0x15, 0x15, 0x15),
            surface_hover: Color32::from_rgb(0x2A, 0x2A, 0x2A),
            surface_raised: Color32::from_rgb(0x26, 0x26, 0x26),
            text: Color32::from_rgb(0xED, 0xED, 0xED),
            text_secondary: Color32::from_rgb(0xA8, 0xA8, 0xA8),
            text_disabled: Color32::from_rgb(0x6A, 0x6A, 0x6A),
            border: Color32::from_rgb(0x3A, 0x3A, 0x3A),
            border_strong: Color32::from_rgb(0x51, 0x51, 0x51),
            accent: Color32::from_rgb(0x6B, 0xB8, 0xFF),
            accent_hover: Color32::from_rgb(0x8B, 0xC8, 0xFF),
            accent_pressed: Color32::from_rgb(0x4A, 0xA3, 0xF0),
            accent_subtle: Color32::from_rgb(0x17, 0x2C, 0x42),
            on_accent: Color32::from_rgb(0x11, 0x11, 0x11),
            selected: Color32::from_rgb(0x1D, 0x2C, 0x3C),
            danger: Color32::from_rgb(0xF3, 0x7A, 0x80),
            danger_subtle: Color32::from_rgb(0x3A, 0x1E, 0x21),
            warning: Color32::from_rgb(0xF0, 0xA0, 0x4B),
            warning_subtle: Color32::from_rgb(0x3A, 0x2C, 0x19),
            success: Color32::from_rgb(0x55, 0xC9, 0xA0),
            success_subtle: Color32::from_rgb(0x17, 0x33, 0x2A),
            shadow: Color32::from_rgba_premultiplied(0, 0, 0, 90),
            overlay: Color32::from_rgba_premultiplied(0, 0, 0, 150),
        }
    }

    pub fn new(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }

    pub fn to_egui_visuals(&self) -> Visuals {
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        v.dark_mode = self.dark;
        v.panel_fill = self.canvas;
        v.window_fill = self.surface;
        v.extreme_bg_color = self.surface;
        v.faint_bg_color = self.surface_sunken;
        v.code_bg_color = self.surface_sunken;
        v.override_text_color = Some(self.text);
        v.weak_text_color = Some(self.text_secondary);
        v.hyperlink_color = self.accent;
        v.selection.bg_fill = self.selected;
        v.selection.stroke = Stroke::new(1.0, self.accent);
        v.window_stroke = Stroke::new(1.0, self.border);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.warn_fg_color = self.warning;
        v.error_fg_color = self.danger;
        v.window_shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: self.shadow,
        };
        v.popup_shadow = egui::epaint::Shadow {
            offset: [0, 6],
            blur: 18,
            spread: 0,
            color: self.shadow,
        };
        v.widgets = Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: self.surface,
                weak_bg_fill: self.surface,
                bg_stroke: Stroke::new(1.0, self.border),
                corner_radius: CornerRadius::same(6),
                fg_stroke: Stroke::new(1.0, self.text),
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: self.surface_sunken,
                weak_bg_fill: self.surface_raised,
                bg_stroke: Stroke::new(1.0, self.border),
                corner_radius: CornerRadius::same(6),
                fg_stroke: Stroke::new(1.0, self.text),
                expansion: 0.0,
            },
            hovered: WidgetVisuals {
                bg_fill: self.surface_hover,
                weak_bg_fill: self.surface_hover,
                bg_stroke: Stroke::new(1.0, self.border_strong),
                corner_radius: CornerRadius::same(6),
                fg_stroke: Stroke::new(1.0, self.text),
                expansion: 0.0,
            },
            active: WidgetVisuals {
                bg_fill: self.accent_subtle,
                weak_bg_fill: self.accent_subtle,
                bg_stroke: Stroke::new(1.0, self.accent),
                corner_radius: CornerRadius::same(6),
                fg_stroke: Stroke::new(1.0, self.text),
                expansion: 0.0,
            },
            open: WidgetVisuals {
                bg_fill: self.accent_subtle,
                weak_bg_fill: self.accent_subtle,
                bg_stroke: Stroke::new(1.0, self.accent),
                corner_radius: CornerRadius::same(6),
                fg_stroke: Stroke::new(1.0, self.text),
                expansion: 0.0,
            },
        };
        v
    }

    pub fn reader_theme(&self, theme: &daftar_core::settings::ReadingTheme) -> ReaderColors {
        let parse = |hex: &str, fallback: Color32| {
            daftar_core::settings::ReadingTheme::parse_color(hex)
                .map(|[r, g, b]| Color32::from_rgb(r, g, b))
                .unwrap_or(fallback)
        };
        ReaderColors {
            background: parse(&theme.background, self.surface),
            text: parse(&theme.text, self.text),
            muted: parse(&theme.muted, self.text_secondary),
            heading: parse(&theme.heading, self.text),
            link: parse(&theme.link, self.accent),
            selection: parse(&theme.selection, self.selected),
            code_background: parse(&theme.code_background, self.surface_sunken),
            code_text: parse(&theme.code_text, self.text),
            code_border: parse(&theme.code_border, self.border),
            quote_background: parse(&theme.quote_background, self.surface_sunken),
            quote_border: parse(&theme.quote_border, self.border_strong),
            table_border: parse(&theme.table_border, self.border),
            table_header_background: parse(&theme.table_header_background, self.surface_sunken),
            rule: parse(&theme.rule, self.border),
            highlight_palette: theme
                .highlight_palette
                .iter()
                .map(|hex| parse(hex, self.warning))
                .collect(),
            dark: theme.dark,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReaderColors {
    pub background: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub heading: Color32,
    pub link: Color32,
    pub selection: Color32,
    pub code_background: Color32,
    pub code_text: Color32,
    pub code_border: Color32,
    pub quote_background: Color32,
    pub quote_border: Color32,
    pub table_border: Color32,
    pub table_header_background: Color32,
    pub rule: Color32,
    pub highlight_palette: Vec<Color32>,
    pub dark: bool,
}

/// Install bundled UI fonts (Inter + Persian/Arabic fallback + monospace).
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
    };
    add("inter", include_bytes!("../../../assets/fonts/Inter-Regular.ttf"));
    add("inter-medium", include_bytes!("../../../assets/fonts/Inter-Medium.ttf"));
    add("inter-bold", include_bytes!("../../../assets/fonts/Inter-Bold.ttf"));
    add("vazirmatn", include_bytes!("../../../assets/fonts/Vazirmatn-Regular.ttf"));
    add("vazirmatn-bold", include_bytes!("../../../assets/fonts/Vazirmatn-Bold.ttf"));
    add("jetbrains", include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"));
    add("jetbrains-bold", include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"));

    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.push("vazirmatn".to_owned());
        list.push("vazirmatn-bold".to_owned());
        list.push("inter-medium".to_owned());
        list.push("inter-bold".to_owned());
    }
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jetbrains".to_owned());
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, dark: bool) {
    let palette = Palette::new(dark);
    ctx.set_theme(if dark {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    });
    let visuals = palette.to_egui_visuals();
    ctx.all_styles_mut(|style| {
        apply_style(style, &palette);
    });
    // Ensure both themes use this palette regardless of system preference.
    ctx.set_visuals_of(Theme::Dark, if dark { visuals.clone() } else { Palette::dark().to_egui_visuals() });
    ctx.set_visuals_of(Theme::Light, if dark { Palette::light().to_egui_visuals() } else { visuals.clone() });
}

fn apply_style(style: &mut Style, palette: &Palette) {
    style.visuals = palette.to_egui_visuals();
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.interact_size = egui::vec2(32.0, 28.0);
    style.spacing.indent = 18.0;
    style.spacing.slider_width = 160.0;
    style.spacing.combo_width = 180.0;
    style.spacing.scroll.bar_width = 9.0;
    style.spacing.scroll.floating = true;
    style.spacing.menu_margin = Margin::same(6);
    style.spacing.window_margin = Margin::same(12);
    let mut text_styles = BTreeMap::new();
    text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
    text_styles.insert(TextStyle::Body, FontId::new(13.0, FontFamily::Proportional));
    text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
    text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
    text_styles.insert(TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace));
    text_styles.insert(
        TextStyle::Name("H2".into()),
        FontId::new(16.0, FontFamily::Proportional),
    );
    text_styles.insert(
        TextStyle::Name("Label".into()),
        FontId::new(11.5, FontFamily::Proportional),
    );
    style.text_styles = text_styles;
}

/// Common panel frame for the sidebar/right dock.
pub fn sidebar_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .inner_margin(Margin::same(10))
}

pub fn content_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.canvas)
        .inner_margin(Margin::same(0))
}

pub fn card_frame(palette: &Palette) -> egui::Frame {
    egui::Frame::new()
        .fill(palette.surface)
        .stroke(Stroke::new(1.0, palette.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
}

pub fn separator(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, color),
        StrokeKind::Inside,
    );
}

pub fn toast_color(palette: &Palette, kind: ToastKind) -> Color32 {
    match kind {
        ToastKind::Info => palette.accent,
        ToastKind::Success => palette.success,
        ToastKind::Warning => palette.warning,
        ToastKind::Error => palette.danger,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

pub const RADIUS_SM: f32 = 4.0;
pub const RADIUS_MD: f32 = 6.0;
pub const RADIUS_LG: f32 = 10.0;