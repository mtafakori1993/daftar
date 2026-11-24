//! Versioned library archive contract.
//!
//! A Daftar library export is a ZIP with a documented layout:
//!
//! ```text
//! manifest.json                 machine-readable manifest
//! README.txt                    human-readable explanation
//! documents/<relative path>     ordinary Markdown files, directory structure preserved
//! attachments/<relative path>   images and other referenced assets
//! database/library.db           consistent SQLite snapshot (metadata, highlights, cards…)
//! settings/app.toml             exported application/reading settings (optional)
//! ```
//!
//! Import validates paths, hashes, manifest version and database integrity before changing
//! anything, then applies changes inside a transaction with a staged copy.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const ARCHIVE_FORMAT: &str = "daftar-library";
pub const ARCHIVE_FORMAT_VERSION: u32 = 1;
pub const MANIFEST_PATH: &str = "manifest.json";
pub const DATABASE_PATH: &str = "database/library.db";
pub const SETTINGS_PATH: &str = "settings/app.toml";
pub const README_PATH: &str = "README.txt";
pub const DOCUMENTS_PREFIX: &str = "documents/";
pub const ATTACHMENTS_PREFIX: &str = "attachments/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFileKind {
    Markdown,
    Attachment,
    Database,
    Settings,
    Readme,
    Manifest,
}

impl ArchiveFileKind {
    pub fn label(self) -> &'static str {
        match self {
            ArchiveFileKind::Markdown => "Markdown",
            ArchiveFileKind::Attachment => "Attachment",
            ArchiveFileKind::Database => "Database",
            ArchiveFileKind::Settings => "Settings",
            ArchiveFileKind::Readme => "Readme",
            ArchiveFileKind::Manifest => "Manifest",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveEntry {
    /// Path inside the ZIP, always `/` separated.
    pub path: String,
    pub kind: ArchiveFileKind,
    pub size: u64,
    pub sha256: String,
    /// Original path relative to the library root (documents/attachments only).
    pub original_rel_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveLibraryInfo {
    pub id: String,
    pub name: String,
    pub root_path_at_export: String,
    pub document_count: u64,
    pub attachment_count: u64,
    pub total_words: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveStatistics {
    pub documents: u64,
    pub attachments: u64,
    pub collections: u64,
    pub categories: u64,
    pub tags: u64,
    pub highlights: u64,
    pub bookmarks: u64,
    pub flashcards: u64,
    pub roadmaps: u64,
    pub reading_sessions: u64,
    #[serde(default)]
    pub study_sessions: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchiveManifest {
    pub format: String,
    pub format_version: u32,
    pub app_name: String,
    pub app_version: String,
    pub exported_at: i64,
    pub database_schema_version: u32,
    pub library: ArchiveLibraryInfo,
    pub statistics: ArchiveStatistics,
    pub files: Vec<ArchiveEntry>,
    /// Free-form key/value metadata for future extensions.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateStrategy {
    /// Keep the existing document; import metadata only when safe.
    #[default]
    Skip,
    /// Replace the existing file after staging and backing it up in the import journal.
    Replace,
    /// Import with a ` (imported N)` suffix.
    Rename,
}

impl DuplicateStrategy {
    pub fn label(self) -> &'static str {
        match self {
            DuplicateStrategy::Skip => "Skip existing documents",
            DuplicateStrategy::Replace => "Replace existing documents",
            DuplicateStrategy::Rename => "Keep both (rename imported)",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportOptions {
    pub duplicate_strategy: DuplicateStrategy,
    pub restore_settings: bool,
    pub restore_reading_progress: bool,
    pub restore_flashcards: bool,
    pub restore_roadmaps: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            duplicate_strategy: DuplicateStrategy::Skip,
            restore_settings: false,
            restore_reading_progress: true,
            restore_flashcards: true,
            restore_roadmaps: true,
        }
    }
}

/// Structural validation issues are collected rather than aborting on the first problem,
/// which lets the import preview explain every issue to the user.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManifestValidation {
    pub ok: bool,
    pub fatal_errors: Vec<String>,
    pub warnings: Vec<String>,
    pub unsupported_version: bool,
}

impl ArchiveManifest {
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(text: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(text)?)
    }

    /// Check manifest identity, version support and file path safety. Hash verification is
    /// performed by the importer because it requires reading the archive.
    pub fn validate(&self) -> ManifestValidation {
        let mut v = ManifestValidation::default();
        if self.format != ARCHIVE_FORMAT {
            v.fatal_errors.push(format!(
                "Not a Daftar library archive (format is `{}`).",
                self.format
            ));
        }
        if self.format_version == 0 {
            v.fatal_errors.push("Manifest has an invalid format version.".into());
        } else if self.format_version > ARCHIVE_FORMAT_VERSION {
            v.unsupported_version = true;
            v.fatal_errors.push(format!(
                "Archive format version {} is newer than this app supports ({}).",
                self.format_version, ARCHIVE_FORMAT_VERSION
            ));
        }
        if self.app_version.trim().is_empty() {
            v.warnings.push("Manifest is missing an application version.".into());
        }
        if self.files.is_empty() {
            v.fatal_errors.push("Manifest contains no files.".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut has_database = false;
        for entry in &self.files {
            if entry.path.starts_with('/') || entry.path.contains('\\') {
                v.fatal_errors
                    .push(format!("Unsafe archive path: {}", entry.path));
            }
            if is_zip_slip(&entry.path) {
                v.fatal_errors
                    .push(format!("Path traversal is not allowed: {}", entry.path));
            }
            if !seen.insert(entry.path.clone()) {
                v.warnings
                    .push(format!("Duplicate manifest entry: {}", entry.path));
            }
            if entry.path == DATABASE_PATH {
                has_database = true;
            }
            if entry.kind == ArchiveFileKind::Markdown && !entry.path.starts_with(DOCUMENTS_PREFIX)
            {
                v.warnings.push(format!(
                    "Markdown entry is outside documents/: {}",
                    entry.path
                ));
            }
        }
        if !has_database {
            v.warnings.push(
                "Archive has no library database; only files can be imported into a new library."
                    .into(),
            );
        }
        v.ok = v.fatal_errors.is_empty();
        v
    }

    pub fn document_entries(&self) -> impl Iterator<Item = &ArchiveEntry> {
        self.files
            .iter()
            .filter(|e| e.kind == ArchiveFileKind::Markdown)
    }

    pub fn attachment_entries(&self) -> impl Iterator<Item = &ArchiveEntry> {
        self.files
            .iter()
            .filter(|e| e.kind == ArchiveFileKind::Attachment)
    }
}

/// Reject absolute paths, drive letters and `..` traversal.
pub fn is_zip_slip(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return true;
    }
    path.split(['/', '\\']).any(|part| part == "..")
}

/// Version of the database schema expected by this build. Exposed here so manifests and
/// import validation agree with the store crate.
pub const DATABASE_SCHEMA_VERSION: u32 = 1;

pub fn readme_text(app_version: &str) -> String {
    format!(
        "Daftar library archive\n\
         ======================\n\n\
         Exported by Daftar {app_version}.\n\n\
         This ZIP contains ordinary Markdown files, their attachments, and a consistent SQLite\n\
         database with library metadata (collections, highlights, bookmarks, flashcards, study\n\
         roadmaps and reading progress).\n\n\
         You can open the Markdown files with any text editor or Markdown reader. The database\n\
         is used by Daftar to restore relationships and study data.\n\n\
         Import this archive through Daftar > Settings > Library > Import library. Importing\n\
         into a new, empty library is the safest way to restore a backup.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_manifest() -> ArchiveManifest {
        ArchiveManifest {
            format: ARCHIVE_FORMAT.into(),
            format_version: ARCHIVE_FORMAT_VERSION,
            app_name: "Daftar".into(),
            app_version: "0.1.0".into(),
            exported_at: 1_700_000_000_000,
            database_schema_version: DATABASE_SCHEMA_VERSION,
            library: ArchiveLibraryInfo {
                id: "lib-1".into(),
                name: "Test".into(),
                root_path_at_export: "/tmp/test".into(),
                document_count: 1,
                attachment_count: 0,
                total_words: 10,
            },
            statistics: ArchiveStatistics {
                documents: 1,
                attachments: 0,
                collections: 0,
                categories: 0,
                tags: 0,
                highlights: 0,
                bookmarks: 0,
                flashcards: 0,
                roadmaps: 0,
                reading_sessions: 0,
                study_sessions: 0,
            },
            files: vec![ArchiveEntry {
                path: format!("{DOCUMENTS_PREFIX}hello.md"),
                kind: ArchiveFileKind::Markdown,
                size: 12,
                sha256: "abc".into(),
                original_rel_path: Some("hello.md".into()),
            }],
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn manifest_round_trips() {
        let manifest = minimal_manifest();
        let json = manifest.to_json().unwrap();
        let restored = ArchiveManifest::from_json(&json).unwrap();
        assert_eq!(manifest, restored);
    }

    #[test]
    fn validates_ok_manifest() {
        let v = minimal_manifest().validate();
        assert!(v.ok, "{:?}", v.fatal_errors);
    }

    #[test]
    fn rejects_newer_format() {
        let mut manifest = minimal_manifest();
        manifest.format_version = ARCHIVE_FORMAT_VERSION + 1;
        let v = manifest.validate();
        assert!(!v.ok);
        assert!(v.unsupported_version);
    }

    #[test]
    fn detects_zip_slip() {
        assert!(is_zip_slip("../secret.md"));
        assert!(is_zip_slip("documents/../../secret.md"));
        assert!(is_zip_slip("/etc/passwd"));
        assert!(is_zip_slip("C:\\Windows\\file"));
        assert!(!is_zip_slip("documents/notes/a.md"));
    }

    #[test]
    fn rejects_unsafe_entry_paths() {
        let mut manifest = minimal_manifest();
        manifest.files[0].path = "documents/../../evil.md".into();
        let v = manifest.validate();
        assert!(!v.ok);
    }

    #[test]
    fn duplicate_strategy_labels_exist() {
        assert!(!DuplicateStrategy::Skip.label().is_empty());
        assert!(!DuplicateStrategy::Replace.label().is_empty());
        assert!(!DuplicateStrategy::Rename.label().is_empty());
    }
}