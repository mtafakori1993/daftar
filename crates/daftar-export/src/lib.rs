//! Export and import: study-card PNG images, print-quality PDF with real text and font
//! embedding, ZIP batch exports, and full-library archive backup/restore.

pub mod archive;
pub mod batch;
pub mod cards;
pub mod error;
pub mod pdf;

pub use archive::{
    apply_import, export_library, preview_import, ExportOptions, ExportReport, ImportPreview,
    ImportReport,
};
pub use batch::{BatchReport, export_cards_png_directory, export_cards_png_zip};
pub use cards::{
    CardContent, CardKind, CardLayout, CardStyle, layout_card, render_card_png, save_card_png,
};
pub use error::ExportError;
pub use pdf::{CardsPerPage, PaperSize, PdfOptions, PdfReport, export_cards_pdf};

pub use daftar_diagram::Rgba8;
