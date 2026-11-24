//! Centralised application layout direction.
//!
//! The interface is authored left-to-right. When the interface language is Persian the whole
//! layout is mirrored through this module instead of scattering `is_persian()` checks through
//! individual widgets: every container picks its layout from here, and egui's own
//! `ui.horizontal*` helpers mirror automatically because they honour the parent's layout
//! direction (`Layout::prefer_right_to_left`).
//!
//! Three concepts stay separate:
//! * *layout* direction — where panels, rows and controls sit (this module),
//! * *text* direction — handled by the text engine with Unicode BiDi (`daftar-text`),
//! * *text* alignment — `align_leading()`/`align_trailing()` map to the right edge in RTL.
//!
//! Technical content (code, URLs, file paths, identifiers) must keep its natural LTR
//! direction, so it is shaped and laid out by the text engine rather than by this module.

use egui::{vec2, Align, Align2, Layout, Rect, Ui, Vec2};

use crate::icons::Icon;

/// Text direction of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Ltr,
    Rtl,
}

impl Dir {
    /// Direction implied by the active interface language.
    pub fn current() -> Self {
        if crate::i18n::is_persian() {
            Dir::Rtl
        } else {
            Dir::Ltr
        }
    }

    pub fn is_rtl(self) -> bool {
        self == Dir::Rtl
    }

    /// Pick a value depending on the direction.
    pub fn pick<T>(self, ltr: T, rtl: T) -> T {
        if self.is_rtl() {
            rtl
        } else {
            ltr
        }
    }
}

/// Whether the application layout is right-to-left.
pub fn is_rtl() -> bool {
    Dir::current().is_rtl()
}

/// Current direction.
pub fn current() -> Dir {
    Dir::current()
}

// -------------------------------------------------------------------------------------- layouts

/// Layout for the content of a view, panel or dialog body.
///
/// Vertical with the cross axis on the leading edge: in RTL that is the right edge, so labels and
/// controls align to the right and nested horizontal groups are mirrored by egui itself.
pub fn root_layout() -> Layout {
    Layout::top_down(align_leading())
}

/// Layout used for title bars, toolbars and the status bar.
pub fn bar_layout() -> Layout {
    if is_rtl() {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    }
}

/// Layout used for toolbars that must not stretch their height.
pub fn bar_layout_top() -> Layout {
    if is_rtl() {
        Layout::right_to_left(Align::Min)
    } else {
        Layout::left_to_right(Align::Min)
    }
}

/// Force a left-to-right layout: used for technical content such as paths, URLs and code.
pub fn ltr_layout() -> Layout {
    Layout::left_to_right(Align::Center)
}

/// Cross-axis alignment of the leading edge (left in LTR, right in RTL).
pub fn align_leading() -> Align {
    if is_rtl() {
        Align::Max
    } else {
        Align::Min
    }
}

/// Cross-axis alignment of the trailing edge (right in LTR, left in RTL).
pub fn align_trailing() -> Align {
    if is_rtl() {
        Align::Min
    } else {
        Align::Max
    }
}

/// Painter text anchor at the leading edge.
pub fn align2_leading_top() -> Align2 {
    if is_rtl() {
        Align2::RIGHT_TOP
    } else {
        Align2::LEFT_TOP
    }
}

/// Painter text anchor at the trailing edge.
pub fn align2_trailing_top() -> Align2 {
    if is_rtl() {
        Align2::LEFT_TOP
    } else {
        Align2::RIGHT_TOP
    }
}

// ---------------------------------------------------------------------------------------- icons

/// Mirror directional icons so navigation keeps pointing "backwards" in RTL.
///
/// Search, settings, trash, bookmark, download, copy and similar non-directional icons are
/// returned unchanged.
pub fn mirrored_icon(icon: Icon) -> Icon {
    if !is_rtl() {
        return icon;
    }
    match icon {
        Icon::ArrowLeft => Icon::ArrowRight,
        Icon::ArrowRight => Icon::ArrowLeft,
        Icon::ChevronLeft => Icon::ChevronRight,
        Icon::ChevronRight => Icon::ChevronLeft,
        Icon::SidebarLeft => Icon::SidebarRight,
        Icon::SidebarRight => Icon::SidebarLeft,
        other => other,
    }
}

// ----------------------------------------------------------------------------------------- rects

/// The leading block of `container` (sidebar side): left in LTR, right in RTL.
pub fn leading_block(container: Rect, width: f32) -> Rect {
    let x = if is_rtl() {
        container.right() - width
    } else {
        container.left()
    };
    Rect::from_min_size(egui::pos2(x, container.top()), vec2(width, container.height()))
}

/// The trailing block of `container` (side panel side): right in LTR, left in RTL.
pub fn trailing_block(container: Rect, width: f32) -> Rect {
    let x = if is_rtl() {
        container.left()
    } else {
        container.right() - width
    };
    Rect::from_min_size(egui::pos2(x, container.top()), vec2(width, container.height()))
}

/// The central block between a leading and a trailing block.
///
/// The leading block (the sidebar) is inset from the leading edge: left in LTR, right in RTL.
pub fn central_block(container: Rect, leading_w: f32, trailing_w: f32) -> Rect {
    if is_rtl() {
        Rect::from_min_max(
            egui::pos2(container.left() + trailing_w, container.top()),
            egui::pos2(container.right() - leading_w, container.bottom()),
        )
    } else {
        Rect::from_min_max(
            egui::pos2(container.left() + leading_w, container.top()),
            egui::pos2(container.right() - trailing_w, container.bottom()),
        )
    }
}

/// Splitter rect sitting on the inner edge of a leading block, centred on that edge.
pub fn leading_splitter(container: Rect, leading_w: f32, thickness: f32) -> Rect {
    let edge = if is_rtl() {
        container.right() - leading_w
    } else {
        container.left() + leading_w
    };
    Rect::from_min_size(
        egui::pos2(edge - thickness * 0.5, container.top()),
        vec2(thickness, container.height()),
    )
}

/// Splitter rect sitting on the inner edge of a trailing block, centred on that edge.
pub fn trailing_splitter(container: Rect, trailing_w: f32, thickness: f32) -> Rect {
    let edge = if is_rtl() {
        container.left() + trailing_w
    } else {
        container.right() - trailing_w
    };
    Rect::from_min_size(
        egui::pos2(edge - thickness * 0.5, container.top()),
        vec2(thickness, container.height()),
    )
}

/// Width of a leading block implied by a pointer position (used while dragging its splitter).
pub fn leading_width_for(container: Rect, pointer_x: f32) -> f32 {
    if is_rtl() {
        container.right() - pointer_x
    } else {
        pointer_x - container.left()
    }
}

/// Width of a trailing block implied by a pointer position (used while dragging its splitter).
pub fn trailing_width_for(container: Rect, pointer_x: f32) -> f32 {
    if is_rtl() {
        pointer_x - container.left()
    } else {
        container.right() - pointer_x
    }
}

/// X coordinate of the edge of a leading panel that faces the central area.
pub fn leading_inner_edge(panel: Rect) -> f32 {
    if is_rtl() {
        panel.left()
    } else {
        panel.right()
    }
}

/// X coordinate of the edge of a trailing panel that faces the central area.
pub fn trailing_inner_edge(panel: Rect) -> f32 {
    if is_rtl() {
        panel.right()
    } else {
        panel.left()
    }
}

/// Offset an anchor by a distance along the leading direction (negative x in RTL).
pub fn advance(distance: f32) -> Vec2 {
    vec2(if is_rtl() { -distance } else { distance }, 0.0)
}

/// X coordinate `distance` inside `rect`, measured from the leading edge.
pub fn leading_x(rect: Rect, distance: f32) -> f32 {
    if is_rtl() {
        rect.right() - distance
    } else {
        rect.left() + distance
    }
}

/// X coordinate `distance` inside `rect`, measured from the trailing edge.
pub fn trailing_x(rect: Rect, distance: f32) -> f32 {
    if is_rtl() {
        rect.left() + distance
    } else {
        rect.right() - distance
    }
}

// ----------------------------------------------------------------------------------- extensions

/// Direction-aware replacements for egui's horizontal layout helpers.
///
/// The main application frame gets its direction from [`root_layout`], but dialogs, menus and
/// other floating surfaces are created by egui with a vertical left-to-right layout, so their
/// rows use these helpers.
pub trait DirUi {
    /// A row of widgets, laid out from the leading edge.
    fn hrow<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R>;
    /// A row aligned to the top of its bounding box.
    fn hrow_top<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R>;
    /// A vertically centred row filling the available height.
    fn hrow_centered<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R>;
    /// Run `add` in a child `Ui` with the application layout, sizing to the content.
    ///
    /// Dialog and menu bodies are created by egui with a left-to-right layout; this keeps them
    /// direction-aware without stretching them to the available space.
    fn direction<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R>;
}

impl DirUi for Ui {
    fn hrow<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        self.with_layout(bar_layout(), add)
    }

    fn hrow_top<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        self.with_layout(bar_layout_top(), add)
    }

    fn hrow_centered<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        let mut layout = bar_layout();
        layout.cross_align = Align::Center;
        self.with_layout(layout, add)
    }

    fn direction<R>(&mut self, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
        self.scope_builder(egui::UiBuilder::new().layout(root_layout()), add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_language(code: &str, f: impl FnOnce()) {
        crate::i18n::set_language(code);
        f();
        crate::i18n::set_language("en");
    }

    #[test]
    fn english_is_left_to_right() {
        with_language("en", || {
            assert!(!is_rtl());
            assert_eq!(align_leading(), Align::Min);
            assert_eq!(align_trailing(), Align::Max);
            assert!(matches!(root_layout().main_dir(), egui::Direction::TopDown));
            assert_eq!(root_layout().cross_align, Align::Min);
            assert_eq!(bar_layout().main_dir(), egui::Direction::LeftToRight);
        });
    }

    #[test]
    fn persian_mirrors_the_layout() {
        with_language("fa", || {
            assert!(is_rtl());
            assert_eq!(align_leading(), Align::Max);
            assert_eq!(align_trailing(), Align::Min);
            assert_eq!(root_layout().cross_align, Align::Max);
            assert_eq!(bar_layout().main_dir(), egui::Direction::RightToLeft);
            // Rows inside a view inherit the mirrored direction from egui.
            assert!(root_layout().prefer_right_to_left());
        });
    }

    #[test]
    fn panels_swap_sides() {
        let body = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 600.0));
        with_language("en", || {
            let leading = leading_block(body, 200.0);
            assert_eq!(leading.left(), 0.0);
            let trailing = trailing_block(body, 220.0);
            assert_eq!(trailing.right(), 1000.0);
            assert_eq!(leading_width_for(body, 250.0), 250.0);
            assert_eq!(trailing_width_for(body, 700.0), 300.0);
            assert_eq!(leading_inner_edge(leading), 200.0);
            assert_eq!(trailing_inner_edge(trailing), 780.0);
            assert_eq!(advance(10.0), vec2(10.0, 0.0));
        });
        with_language("fa", || {
            let leading = leading_block(body, 200.0);
            assert_eq!(leading.right(), 1000.0);
            assert_eq!(leading.left(), 800.0);
            let trailing = trailing_block(body, 220.0);
            assert_eq!(trailing.left(), 0.0);
            assert_eq!(leading_width_for(body, 750.0), 250.0);
            assert_eq!(trailing_width_for(body, 300.0), 300.0);
            assert_eq!(leading_inner_edge(leading), 800.0);
            assert_eq!(trailing_inner_edge(trailing), 220.0);
            assert_eq!(advance(10.0), vec2(-10.0, 0.0));
        });
    }

    #[test]
    fn central_area_is_between_the_panels() {
        let body = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1000.0, 600.0));
        with_language("en", || {
            let central = central_block(body, 200.0, 220.0);
            assert_eq!(central.left(), 200.0);
            assert_eq!(central.right(), 780.0);
        });
        with_language("fa", || {
            // Sidebar on the right, side panel on the left.
            let central = central_block(body, 200.0, 220.0);
            assert_eq!(central.left(), 220.0);
            assert_eq!(central.right(), 800.0);
        });
    }

    #[test]
    fn only_directional_icons_mirror() {
        with_language("fa", || {
            assert_eq!(mirrored_icon(Icon::ArrowLeft), Icon::ArrowRight);
            assert_eq!(mirrored_icon(Icon::ArrowRight), Icon::ArrowLeft);
            assert_eq!(mirrored_icon(Icon::ChevronLeft), Icon::ChevronRight);
            assert_eq!(mirrored_icon(Icon::SidebarLeft), Icon::SidebarRight);
            for icon in [
                Icon::Search,
                Icon::Settings,
                Icon::Trash,
                Icon::Bookmark,
                Icon::Download,
                Icon::Copy,
                Icon::Moon,
                Icon::Sun,
            ] {
                assert_eq!(mirrored_icon(icon), icon, "{icon:?} must not mirror");
            }
        });
    }

    #[test]
    fn text_alignment_follows_direction() {
        with_language("en", || {
            assert_eq!(align2_leading_top(), Align2::LEFT_TOP);
            assert_eq!(align2_trailing_top(), Align2::RIGHT_TOP);
        });
        with_language("fa", || {
            assert_eq!(align2_leading_top(), Align2::RIGHT_TOP);
            assert_eq!(align2_trailing_top(), Align2::LEFT_TOP);
        });
    }
}
