//! Reusable Spectrum-inspired widgets built from the design tokens.

use egui::{
    Align, Color32, FontId, Layout, Rect, Response, RichText, Sense, Stroke, StrokeKind, TextEdit,
    Ui, Vec2, vec2,
};

use crate::icons::{Icon, draw_icon};
use crate::theme::Palette;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Quiet,
    Danger,
}

pub fn button(
    ui: &mut Ui,
    palette: &Palette,
    label: &str,
    icon: Option<Icon>,
    variant: ButtonVariant,
) -> Response {
    let label = crate::i18n::t(label);
    let icon = icon.map(crate::rtl::mirrored_icon);
    let font = FontId::proportional(13.0);
    let galley = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), palette.text));
    let icon_space = if icon.is_some() { 21.0 } else { 0.0 };
    let width = (galley.size().x + 24.0 + icon_space).max(64.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let hovered = response.hovered();
    let pressed = response.is_pointer_button_down_on();
    let (bg, border, fg) = match variant {
        ButtonVariant::Primary => {
            let bg = if pressed {
                palette.accent_pressed
            } else if hovered {
                palette.accent_hover
            } else {
                palette.accent
            };
            (bg, Color32::TRANSPARENT, palette.on_accent)
        }
        ButtonVariant::Secondary => {
            let bg = if pressed {
                palette.surface_sunken
            } else if hovered {
                palette.surface_hover
            } else {
                palette.surface
            };
            (bg, palette.border_strong, palette.text)
        }
        ButtonVariant::Quiet => {
            let bg = if pressed || hovered {
                palette.surface_hover
            } else {
                Color32::TRANSPARENT
            };
            (bg, Color32::TRANSPARENT, palette.text)
        }
        ButtonVariant::Danger => {
            let bg = if pressed {
                palette.danger.gamma_multiply(0.8)
            } else if hovered {
                palette.danger.gamma_multiply(0.9)
            } else {
                palette.danger
            };
            (bg, Color32::TRANSPARENT, palette.on_accent)
        }
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, bg);
    }
    if border != Color32::TRANSPARENT {
        ui.painter()
            .rect_stroke(rect, 6.0, Stroke::new(1.0, border), StrokeKind::Inside);
    }
    if variant == ButtonVariant::Primary && hovered {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            7.0,
            Stroke::new(1.0, palette.accent.gamma_multiply(0.35)),
            StrokeKind::Outside,
        );
    }
    let rtl = crate::rtl::is_rtl();
    let mut x = if rtl { rect.right() - 12.0 } else { rect.left() + 12.0 };
    if let Some(icon) = icon {
        let icon_x = if rtl { x - 16.0 } else { x };
        let icon_rect =
            Rect::from_min_size(egui::pos2(icon_x, rect.center().y - 8.0), Vec2::splat(16.0));
        draw_icon(ui.painter(), icon, icon_rect, fg);
        x = if rtl { x - 21.0 } else { x + 21.0 };
    }
    let galley = ui.ctx().fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font, fg));
    let label_x = if rtl { x - galley.size().x } else { x };
    ui.painter().galley(
        egui::pos2(label_x, rect.center().y - galley.size().y * 0.5),
        galley,
        fg,
    );
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn icon_button(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    tooltip: &str,
    selected: bool,
) -> Response {
    let tooltip = crate::i18n::t(tooltip);
    let icon = crate::rtl::mirrored_icon(icon);
    icon_button_sized(ui, palette, icon, tooltip, selected, 30.0)
}

pub fn icon_button_sized(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    tooltip: &str,
    selected: bool,
    size: f32,
) -> Response {
    let tooltip = crate::i18n::t(tooltip);
    let icon = crate::rtl::mirrored_icon(icon);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let bg = if selected {
        palette.accent_subtle
    } else if response.hovered() {
        palette.surface_hover
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, 6.0, bg);
    }
    if selected {
        ui.painter().rect_stroke(
            rect,
            6.0,
            Stroke::new(1.0, palette.accent.gamma_multiply(0.5)),
            StrokeKind::Inside,
        );
    }
    let color = if selected {
        palette.accent
    } else if response.hovered() {
        palette.text
    } else {
        palette.text_secondary
    };
    draw_icon(ui.painter(), icon, rect.shrink(size * 0.24), color);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.on_hover_text(tooltip)
}

pub fn nav_item(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    label: &str,
    selected: bool,
    badge: Option<u64>,
) -> Response {
    let label = crate::i18n::t(label);
    let icon = crate::rtl::mirrored_icon(icon);
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    let rtl = crate::rtl::is_rtl();
    let bar_x = if rtl {
        rect.right() - 2.5
    } else {
        rect.left()
    };
    if selected {
        ui.painter().rect_filled(rect, 6.0, palette.accent_subtle);
        ui.painter().rect_filled(
            Rect::from_min_size(
                egui::pos2(bar_x, rect.top() + 7.0),
                vec2(2.5, rect.height() - 14.0),
            ),
            1.5,
            palette.accent,
        );
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 6.0, palette.surface_hover);
    }
    let color = if selected {
        palette.accent
    } else {
        palette.text_secondary
    };
    let icon_x = if rtl { rect.right() - 26.0 } else { rect.left() + 10.0 };
    let icon_rect = Rect::from_min_size(
        egui::pos2(icon_x, rect.center().y - 8.0),
        Vec2::splat(16.0),
    );
    draw_icon(ui.painter(), icon, icon_rect, color);
    let text_color = if selected {
        palette.text
    } else {
        palette.text_secondary
    };
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(label.to_owned(), FontId::proportional(13.0), text_color)
    });
    let label_x = if rtl {
        rect.right() - 36.0 - galley.size().x
    } else {
        rect.left() + 36.0
    };
    ui.painter().galley(
        egui::pos2(label_x, rect.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );
    if let Some(badge) = badge {
        let text = badge.to_string();
        let badge_galley = ui.ctx().fonts_mut(|f| {
            f.layout_no_wrap(
                text,
                FontId::proportional(10.5),
                palette.text_secondary,
            )
        });
        let w = badge_galley.size().x + 12.0;
        let badge_x = if rtl {
            rect.left() + 8.0
        } else {
            rect.right() - w - 8.0
        };
        let badge_rect = Rect::from_min_size(
            egui::pos2(badge_x, rect.center().y - 9.0),
            vec2(w, 18.0),
        );
        ui.painter()
            .rect_filled(badge_rect, 9.0, palette.surface_sunken);
        ui.painter().galley(
            egui::pos2(
                badge_rect.center().x - badge_galley.size().x * 0.5,
                badge_rect.center().y - badge_galley.size().y * 0.5,
            ),
            badge_galley,
            palette.text_secondary,
        );
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn chip(ui: &mut Ui, palette: &Palette, label: &str, selected: bool) -> Response {
    let label = crate::i18n::t(label);
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(12.0),
            if selected {
                palette.accent
            } else {
                palette.text_secondary
            },
        )
    });
    let width = galley.size().x + 22.0;
    let (rect, response) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
    let bg = if selected {
        palette.accent_subtle
    } else if response.hovered() {
        palette.surface_hover
    } else {
        palette.surface_sunken
    };
    ui.painter().rect_filled(rect, 12.0, bg);
    if selected {
        ui.painter().rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0, palette.accent.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
    let color = if selected {
        palette.accent
    } else {
        palette.text_secondary
    };
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(label.to_owned(), FontId::proportional(12.0), color)
    });
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn toggle(ui: &mut Ui, palette: &Palette, value: &mut bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *value = !*value;
    }
    let bg = if *value {
        palette.accent
    } else {
        palette.border_strong
    };
    ui.painter().rect_filled(rect, 10.0, bg);
    let knob_x = if *value {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    ui.painter()
        .circle_filled(egui::pos2(knob_x, rect.center().y), 7.0, Color32::WHITE);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

pub fn progress_bar(ui: &mut Ui, palette: &Palette, fraction: f32, color: Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), height.max(4.0)),
        Sense::hover(),
    );
    ui.painter()
        .rect_filled(rect, height * 0.5, palette.surface_sunken);
    let f = fraction.clamp(0.0, 1.0);
    if f > 0.0 {
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(rect.width() * f, rect.height())),
            height * 0.5,
            color,
        );
    }
}

pub fn section_label(ui: &mut Ui, palette: &Palette, text: &str) {
    let text = crate::i18n::t(text);
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(
            text.to_ascii_uppercase(),
            FontId::proportional(10.5),
            palette.text_secondary,
        )
    });
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), galley.size().y + 8.0),
        Sense::hover(),
    );
    let x = if crate::rtl::is_rtl() {
        rect.right() - galley.size().x
    } else {
        rect.left()
    };
    ui.painter().galley(
        egui::pos2(x, rect.top() + 4.0),
        galley,
        palette.text_secondary,
    );
}

pub fn hint(ui: &mut Ui, palette: &Palette, text: &str) {
    ui.label(
        RichText::new(crate::i18n::t(text))
            .size(11.5)
            .color(palette.text_secondary),
    );
}

pub fn keycap(ui: &mut Ui, palette: &Palette, text: &str) {
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(
            text.to_owned(),
            FontId::proportional(10.5),
            palette.text_secondary,
        )
    });
    let (rect, _) =
        ui.allocate_exact_size(vec2(galley.size().x + 10.0, 20.0), Sense::hover());
    ui.painter().rect_filled(rect, 4.0, palette.surface_sunken);
    ui.painter().rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0, palette.border),
        StrokeKind::Inside,
    );
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        palette.text_secondary,
    );
}

pub fn text_field(
    ui: &mut Ui,
    palette: &Palette,
    value: &mut String,
    hint_text: &str,
    width: f32,
) -> Response {
    let hint_text = crate::i18n::t(hint_text);
    let width = if width <= 0.0 {
        ui.available_width()
    } else {
        width
    };
    let (rect, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 6.0, palette.surface_sunken);
    let inner = rect.shrink2(vec2(9.0, 4.0));
    let response = ui.put(
        inner,
        TextEdit::singleline(value)
            .frame(egui::Frame::NONE)
            .hint_text(hint_text)
            .font(FontId::proportional(13.0))
            .desired_width(inner.width()),
    );
    let border = if response.has_focus() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter()
        .rect_stroke(rect, 6.0, Stroke::new(1.0, border), StrokeKind::Inside);
    response
}

pub fn search_field(
    ui: &mut Ui,
    palette: &Palette,
    value: &mut String,
    hint_text: &str,
) -> Response {
    let hint_text = crate::i18n::t(hint_text);
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 6.0, palette.surface_sunken);
    // The magnifier sits on the leading edge: left in LTR, right in RTL.
    if crate::rtl::is_rtl() {
        draw_icon(
            ui.painter(),
            Icon::Search,
            Rect::from_min_size(
                egui::pos2(rect.right() - 23.0, rect.center().y - 7.0),
                Vec2::splat(14.0),
            ),
            palette.text_secondary,
        );
    } else {
        draw_icon(
            ui.painter(),
            Icon::Search,
            Rect::from_min_size(
                egui::pos2(rect.left() + 9.0, rect.center().y - 7.0),
                Vec2::splat(14.0),
            ),
            palette.text_secondary,
        );
    }
    let inner = if crate::rtl::is_rtl() {
        Rect::from_min_max(
            egui::pos2(rect.left() + 8.0, rect.top() + 4.0),
            egui::pos2(rect.right() - 30.0, rect.bottom() - 4.0),
        )
    } else {
        Rect::from_min_max(
            egui::pos2(rect.left() + 30.0, rect.top() + 4.0),
            egui::pos2(rect.right() - 8.0, rect.bottom() - 4.0),
        )
    };
    let response = ui.put(
        inner,
        TextEdit::singleline(value)
            .frame(egui::Frame::NONE)
            .hint_text(hint_text)
            .font(FontId::proportional(13.0))
            .desired_width(inner.width()),
    );
    let border = if response.has_focus() {
        palette.accent
    } else {
        palette.border
    };
    ui.painter()
        .rect_stroke(rect, 6.0, Stroke::new(1.0, border), StrokeKind::Inside);
    response
}

pub fn empty_state(ui: &mut Ui, palette: &Palette, icon: Icon, title: &str, body: &str) {
    let title = crate::i18n::t(title);
    let body = crate::i18n::t(body);
    ui.add_space(36.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
        ui.painter()
            .circle_filled(rect.center(), 23.0, palette.surface_sunken);
        draw_icon(
            ui.painter(),
            icon,
            rect.shrink(13.0),
            palette.text_secondary,
        );
        ui.add_space(10.0);
        ui.label(
            RichText::new(title)
                .size(15.0)
                .strong()
                .color(palette.text),
        );
        ui.add_space(3.0);
        ui.label(
            RichText::new(body)
                .size(12.0)
                .color(palette.text_secondary),
        );
    });
}

pub fn toolbar_divider(ui: &mut Ui, palette: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette.border);
}

pub fn meta_pill(ui: &mut Ui, palette: &Palette, text: &str, color: Color32) {
    let text = crate::i18n::t(text);
    let galley = ui.ctx().fonts_mut(|f| {
        f.layout_no_wrap(text.to_owned(), FontId::proportional(10.5), color)
    });
    let (rect, _) =
        ui.allocate_exact_size(vec2(galley.size().x + 14.0, 20.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 10.0, color.gamma_multiply(0.12));
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        ),
        galley,
        color,
    );
}

pub fn centered_text(ui: &mut Ui, palette: &Palette, text: &str) {
    let text = crate::i18n::t(text);
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.label(RichText::new(text).color(palette.text_secondary));
    });
}