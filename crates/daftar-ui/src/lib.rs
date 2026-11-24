//! Native egui interface for Daftar: Spectrum-inspired design system, reader and views.

pub mod app;
pub mod code;
pub mod i18n;
pub mod icons;
pub mod picker;
pub mod tasks;
pub mod views;
pub mod images;
pub mod reader;
pub mod shaped;
pub mod theme;
pub mod widgets;

pub use app::{AppConfig, DaftarApp, View};
pub use icons::Icon;
pub use theme::{Palette, ReaderColors, ToastKind};