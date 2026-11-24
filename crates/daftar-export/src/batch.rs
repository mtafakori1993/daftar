//! Batch PNG export as individual files or a ZIP archive.

use std::io::Write;
use std::path::{Path, PathBuf};

use daftar_core::util::safe_file_stem;
use daftar_text::TextEngine;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::cards::{CardContent, CardStyle, render_card_png};
use crate::error::{ExportError, Result};

#[derive(Debug, Clone)]
pub struct BatchReport {
    pub files: u64,
    pub bytes: u64,
    pub path: PathBuf,
}

/// Write each card as a high-resolution PNG into a directory.
pub fn export_cards_png_directory(
    cards: &[CardContent],
    style: &CardStyle,
    engine: &mut TextEngine,
    directory: impl AsRef<Path>,
    prefix: &str,
) -> Result<BatchReport> {
    if cards.is_empty() {
        return Err(ExportError::Data("No cards selected for export.".into()));
    }
    let directory = directory.as_ref();
    std::fs::create_dir_all(directory)?;
    let mut files = 0u64;
    let mut bytes = 0u64;
    for (index, card) in cards.iter().enumerate() {
        let name = format!(
            "{}-{:03}-{}.png",
            safe_file_stem(prefix),
            index + 1,
            safe_file_stem(&card.prompt.chars().take(40).collect::<String>())
        );
        let png = render_card_png(card, style, engine)?;
        std::fs::write(directory.join(&name), &png)?;
        files += 1;
        bytes += png.len() as u64;
    }
    Ok(BatchReport {
        files,
        bytes,
        path: directory.to_path_buf(),
    })
}

/// Write cards as PNGs inside a ZIP archive.
pub fn export_cards_png_zip(
    cards: &[CardContent],
    style: &CardStyle,
    engine: &mut TextEngine,
    zip_path: impl AsRef<Path>,
    prefix: &str,
) -> Result<BatchReport> {
    if cards.is_empty() {
        return Err(ExportError::Data("No cards selected for export.".into()));
    }
    let zip_path = zip_path.as_ref();
    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(zip_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut files = 0u64;
    let mut bytes = 0u64;
    for (index, card) in cards.iter().enumerate() {
        let name = format!(
            "{}-{:03}.png",
            safe_file_stem(prefix),
            index + 1
        );
        let png = render_card_png(card, style, engine)?;
        zip.start_file(name, options)?;
        zip.write_all(&png)?;
        files += 1;
        bytes += png.len() as u64;
    }
    zip.finish()?;
    Ok(BatchReport {
        files,
        bytes,
        path: zip_path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::CardKind;
    use daftar_text::TextEngine;

    fn card(front: &str) -> CardContent {
        CardContent {
            kind: CardKind::Flashcard,
            prompt: front.into(),
            answer: Some("Answer".into()),
            note: None,
            source: None,
            tags: vec![],
            side_label: None,
            include_back: true,
        }
    }

    #[test]
    fn writes_png_directory_and_zip() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = TextEngine::new(None, "en-US");
        let cards = vec![card("One"), card("Two")];
        let style = CardStyle::light();
        let report =
            export_cards_png_directory(&cards, &style, &mut engine, dir.path().join("pngs"), "deck")
                .unwrap();
        assert_eq!(report.files, 2);
        assert!(dir.path().join("pngs").read_dir().unwrap().count() >= 2);
        let zip_path = dir.path().join("deck.zip");
        let report = export_cards_png_zip(&cards, &style, &mut engine, &zip_path, "deck").unwrap();
        assert_eq!(report.files, 2);
        let archive = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 2);
    }

    #[test]
    fn rejects_empty_batches() {
        let dir = tempfile::tempdir().unwrap();
        let mut engine = TextEngine::new(None, "en-US");
        assert!(export_cards_png_zip(
            &[],
            &CardStyle::light(),
            &mut engine,
            dir.path().join("x.zip"),
            "x"
        )
        .is_err());
    }
}
