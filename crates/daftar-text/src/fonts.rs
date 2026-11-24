//! Font discovery and registration.
//!
//! Bundled fonts guarantee identical Persian/Arabic shaping on Linux and Windows even on a
//! machine with no Arabic system fonts installed. System fonts and user-imported fonts are
//! added on top.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cosmic_text::{FontSystem, SwashCache};
use fontdb::{Database, Source};
use ttf_parser::{Face, name_id};

use crate::layout::{LayoutOptions, ShapedText, shape_rich, shape_text};
use crate::paint::GlyphAtlas;
use daftar_core::markdown::{RichText, SpanStyle};

pub struct BundledFont {
    pub file: &'static str,
    pub bytes: &'static [u8],
}

macro_rules! font {
    ($file:literal) => {
        BundledFont {
            file: $file,
            bytes: include_bytes!(concat!("../../../assets/fonts/", $file)),
        }
    };
}

/// Fonts shipped inside the executable. All are licensed under the SIL Open Font License,
/// which permits redistribution and embedding (licence files live in `assets/fonts`).
pub fn bundled_fonts() -> &'static [BundledFont] {
    &[
        font!("Vazirmatn-Regular.ttf"),
        font!("Vazirmatn-Medium.ttf"),
        font!("Vazirmatn-SemiBold.ttf"),
        font!("Vazirmatn-Bold.ttf"),
        font!("Inter-Regular.ttf"),
        font!("Inter-Medium.ttf"),
        font!("Inter-SemiBold.ttf"),
        font!("Inter-Bold.ttf"),
        font!("Inter-Italic.ttf"),
        font!("Inter-BoldItalic.ttf"),
        font!("JetBrainsMono-Regular.ttf"),
        font!("JetBrainsMono-Bold.ttf"),
    ]
}

#[derive(Debug, Clone)]
pub struct FontBlob {
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    pub family: String,
    pub weight: u16,
    pub italic: bool,
}

pub struct TextEngine {
    pub fonts: FontSystem,
    pub(crate) swash: SwashCache,
    pub(crate) atlas: GlyphAtlas,
    pub(crate) blobs: HashMap<fontdb::ID, Arc<Vec<u8>>>,
    pub(crate) outline_cache: crate::raster::OutlineCache,
    pub(crate) family_cache: BTreeSet<String>,
    pub(crate) custom_dir: Option<PathBuf>,
    pub(crate) bundled_families: BTreeSet<String>,
}

impl TextEngine {
    /// Create the engine with bundled fonts, system fonts and optional user fonts.
    pub fn new(custom_font_dir: Option<PathBuf>, locale: &str) -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        let mut bundled_families = BTreeSet::new();
        for font in bundled_fonts() {
            if let Some(family) = font_family_from_bytes(font.bytes) {
                bundled_families.insert(family);
            }
            db.load_font_data(font.bytes.to_vec());
        }
        let fonts = FontSystem::new_with_locale_and_db(locale.to_string(), db);
        let mut engine = Self {
            fonts,
            swash: SwashCache::new(),
            atlas: GlyphAtlas::new(),
            blobs: HashMap::new(),
            outline_cache: HashMap::new(),
            family_cache: BTreeSet::new(),
            custom_dir: custom_font_dir,
            bundled_families,
        };
        engine.load_custom_fonts();
        engine.refresh_families();
        engine
    }

    pub fn load_custom_fonts(&mut self) {
        let Some(dir) = self.custom_dir.clone() else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc" | "otc")) {
                continue;
            }
            if let Ok(data) = std::fs::read(&path) {
                self.fonts.db_mut().load_font_data(data);
            }
        }
        self.refresh_families();
    }

    /// Copy and register a user font. The data is owned by the caller; persistence of the
    /// file into the app font folder is the UI's responsibility.
    pub fn register_font_bytes(&mut self, data: Vec<u8>) -> Result<String, String> {
        let family = font_family_from_bytes(&data)
            .ok_or_else(|| "The selected file is not a valid OpenType/TrueType font.".to_string())?;
        // Validate that it parses with the same parser used by shaping.
        let _face = Face::parse(&data, 0).map_err(|e| format!("Invalid font: {e}"))?;
        self.fonts.db_mut().load_font_data(data);
        self.refresh_families();
        Ok(family)
    }

    pub fn register_font_file(&mut self, path: &Path) -> Result<String, String> {
        let data = std::fs::read(path).map_err(|e| format!("Cannot read font: {e}"))?;
        self.register_font_bytes(data)
    }

    pub fn refresh_families(&mut self) {
        let db = self.fonts.db_mut();
        let mut set = BTreeSet::new();
        for face in db.faces() {
            if let Some((family, _)) = face.families.first() {
                if !family.trim().is_empty() {
                    set.insert(family.clone());
                }
            }
        }
        self.family_cache = set;
    }

    /// Sorted family names available for typography menus, bundled families first.
    pub fn families(&self) -> Vec<String> {
        let mut bundled: Vec<String> = self
            .family_cache
            .iter()
            .filter(|f| self.bundled_families.contains(*f))
            .cloned()
            .collect();
        let others: Vec<String> = self
            .family_cache
            .iter()
            .filter(|f| !self.bundled_families.contains(*f))
            .cloned()
            .collect();
        bundled.extend(others);
        bundled
    }

    pub fn bundled_families(&self) -> &BTreeSet<String> {
        &self.bundled_families
    }

    pub fn has_family(&self, name: &str) -> bool {
        self.family_cache
            .iter()
            .any(|f| f.eq_ignore_ascii_case(name))
    }

    /// Best default reading family that covers Persian/Arabic and Latin.
    pub fn default_reading_family(&self) -> String {
        if self.has_family("Vazirmatn") {
            "Vazirmatn".to_string()
        } else if self.has_family("Noto Naskh Arabic") {
            "Noto Naskh Arabic".to_string()
        } else {
            self.families()
                .first()
                .cloned()
                .unwrap_or_else(|| "SansSerif".to_string())
        }
    }

    pub fn default_code_family(&self) -> String {
        if self.has_family("JetBrains Mono") {
            "JetBrains Mono".to_string()
        } else if self.has_family("DejaVu Sans Mono") {
            "DejaVu Sans Mono".to_string()
        } else {
            "monospace".to_string()
        }
    }

    pub fn layout_rich(&mut self, rt: &RichText, opts: &LayoutOptions) -> ShapedText {
        shape_rich(&mut self.fonts, rt, opts)
    }

    pub fn layout_text(&mut self, text: &str, opts: &LayoutOptions, style: &SpanStyle) -> ShapedText {
        shape_text(&mut self.fonts, text, opts, style)
    }

    /// Raw font data for PDF/PNG export. Cached per face.
    pub fn font_blob(&mut self, id: fontdb::ID) -> Option<FontBlob> {
        if let Some(data) = self.blobs.get(&id) {
            let face_info = self
                .fonts
                .db_mut()
                .face(id)
                .map(|f| (f.families.first().map(|x| x.0.clone()).unwrap_or_default(), f.weight.0, f.style != fontdb::Style::Normal, f.index))
                .unwrap_or_default_tuple();
            return Some(FontBlob {
                data: data.clone(),
                index: face_info.3,
                family: face_info.0,
                weight: face_info.1,
                italic: face_info.2,
            });
        }
        let (source, index, family, weight, italic) = {
            let db = self.fonts.db_mut();
            let face = db.face(id)?;
            (
                face.source.clone(),
                face.index,
                face.families
                    .first()
                    .map(|(name, _)| name.clone())
                    .unwrap_or_default(),
                face.weight.0,
                face.style != fontdb::Style::Normal,
            )
        };
        let data: Vec<u8> = match source {
            Source::Binary(arc) => arc.as_ref().as_ref().to_vec(),
            Source::File(path) => std::fs::read(path).ok()?,
            Source::SharedFile(_, arc) => arc.as_ref().as_ref().to_vec(),
        };
        let data = Arc::new(data);
        self.blobs.insert(id, data.clone());
        Some(FontBlob {
            data,
            index,
            family,
            weight,
            italic,
        })
    }

    /// Face metadata needed for PDF font descriptors.
    pub fn face_metrics(&mut self, id: fontdb::ID) -> Option<FaceMetrics> {
        let blob = self.font_blob(id)?;
        let face = Face::parse(&blob.data, blob.index).ok()?;
        Some(FaceMetrics {
            units_per_em: face.units_per_em(),
            ascender: face.ascender(),
            descender: face.descender(),
            cap_height: face.capital_height().unwrap_or(face.ascender()),
            bbox: face.global_bounding_box(),
            italic_angle: face.italic_angle(),
            is_monospace: face.is_monospaced(),
            weight: blob.weight,
        })
    }

    pub fn is_bundled_family(&self, family: &str) -> bool {
        self.bundled_families.contains(family)
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new(None, "en-US")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FaceMetrics {
    pub units_per_em: u16,
    pub ascender: i16,
    pub descender: i16,
    pub cap_height: i16,
    pub bbox: ttf_parser::Rect,
    pub italic_angle: f32,
    pub is_monospace: bool,
    pub weight: u16,
}

trait DefaultTuple {
    fn unwrap_or_default_tuple(self) -> (String, u16, bool, u32);
}

impl DefaultTuple for Option<(String, u16, bool, u32)> {
    fn unwrap_or_default_tuple(self) -> (String, u16, bool, u32) {
        self.unwrap_or_else(|| (String::new(), 400, false, 0))
    }
}

pub fn font_family_from_bytes(data: &[u8]) -> Option<String> {
    let face = Face::parse(data, 0).ok()?;
    let mut fallback = None;
    for name in face.names() {
        if name.name_id == name_id::TYPOGRAPHIC_FAMILY || name.name_id == name_id::FAMILY {
            if let Some(value) = name.to_string() {
                if name.language_id == 0x0409 {
                    return Some(value);
                }
                fallback.get_or_insert(value);
            }
        }
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fonts_parse_and_have_families() {
        for font in bundled_fonts() {
            assert!(
                font_family_from_bytes(font.bytes).is_some(),
                "{} has no family",
                font.file
            );
        }
    }

    #[test]
    fn engine_finds_persian_family() {
        let engine = TextEngine::new(None, "fa-IR");
        let families = engine.families();
        assert!(families.iter().any(|f| f == "Vazirmatn"));
        assert!(families.iter().any(|f| f == "Inter"));
    }

    #[test]
    fn default_reading_family_supports_bundled_font() {
        let engine = TextEngine::new(None, "fa-IR");
        assert_eq!(engine.default_reading_family(), "Vazirmatn");
        assert_eq!(engine.default_code_family(), "JetBrains Mono");
    }

    #[test]
    fn font_blob_round_trip() {
        let mut engine = TextEngine::new(None, "en-US");
        let id = engine
            .fonts
            .db_mut()
            .faces()
            .find(|f| f.families.iter().any(|(n, _)| n == "Vazirmatn"))
            .map(|f| f.id)
            .expect("vazir face");
        let blob = engine.font_blob(id).expect("blob");
        assert!(blob.data.len() > 10_000);
        assert_eq!(blob.family, "Vazirmatn");
        let metrics = engine.face_metrics(id).expect("metrics");
        assert!(metrics.units_per_em > 0);
    }

    #[test]
    fn invalid_font_is_rejected() {
        let mut engine = TextEngine::new(None, "en-US");
        assert!(engine.register_font_bytes(vec![1, 2, 3, 4]).is_err());
    }
}