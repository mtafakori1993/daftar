//! Daftar core: domain models, Markdown parsing, text direction, anchoring and archive contracts.
//!
//! This crate contains no UI and no storage code, which keeps the domain testable and
//! reusable by the reader, exporter and importer.

pub mod anchor;
pub mod archive;
pub mod direction;
pub mod markdown;
pub mod model;
pub mod settings;
pub mod util;

pub use anchor::{AnchorStatus, TextAnchor, relocate_anchor};
pub use direction::{
    Direction, DirectionResolution, ResolvedDirection, detect_direction, paragraph_direction,
    resolve_direction,
};
pub use markdown::{
    Align, Block, BlockId, Footnote, ImageRef, ListItem, MdDocument, RichText, SpanStyle,
    StyledSpan, TocEntry, parse_markdown,
};
pub use model::*;
pub use settings::*;
pub use util::{now_ms, plain_text, slugify, word_count};