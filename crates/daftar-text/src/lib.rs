//! Bidirectional shaping, typography, glyph painting and vector outlines for Daftar.
//!
//! The engine is built on cosmic-text (HarfBuzz-class shaping via `harfrust`, UAX #9 bidi) and
//! swash/tiny-skia. It supports Persian/Arabic joining, mixed RTL/LTR runs, advanced font
//! fallback, letter spacing, superscripts, custom fonts and both screen and export rendering.

pub mod fonts;
pub mod layout;
pub mod paint;
pub mod raster;

pub use fonts::{BundledFont, FaceMetrics, FontBlob, TextEngine, bundled_fonts};
pub use layout::{
    LayoutOptions, ShapedGlyph, ShapedLine, ShapedText, shape_rich, shape_text, span_for_all,
};
pub use paint::{RangePaint, TextColors};
pub use raster::OutlineCache;