//! Markdown reader: layout, selection, highlights and interactive document rendering.

pub mod layout;
pub mod view;

pub use layout::{
    DiagramCache, DiagramEntry, Element, ElementKind, ListMarker, ReaderConfig, ReaderLayout,
    build_layout, diagram_key, diagram_theme,
};
pub use view::ReaderState;
