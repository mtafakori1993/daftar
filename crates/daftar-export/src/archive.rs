//! Full-library backup and restore.
//!
//! Export writes a documented ZIP: `manifest.json`, ordinary Markdown files, attachments, a
//! consistent SQLite snapshot and optional settings. Import validates the archive and every
//! path, stages and hashes files, moves only after validation, merges metadata inside a single
//! database transaction, and rolls files back if the transaction fails.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use daftar_core::archive::{
    ARCHIVE_FORMAT, ARCHIVE_FORMAT_VERSION, ArchiveEntry, ArchiveFileKind, ArchiveLibraryInfo,
    ArchiveManifest, ArchiveStatistics, DATABASE_PATH, DATABASE_SCHEMA_VERSION, DOCUMENTS_PREFIX,
    ATTACHMENTS_PREFIX, DuplicateStrategy, ImportOptions, MANIFEST_PATH, README_PATH,
    SETTINGS_PATH, is_zip_slip, readme_text,
};
use daftar_core::model::{Document, Library, native_to_rel, rel_to_native};
use daftar_core::util::sha256_hex;
use daftar_core::util::now_ms;
use daftar_store::archive_import::ImportSummary;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{ExportError, Result};

#[derive(Clone, Default)]
pub struct ExportOptions {
    pub include_settings: bool,
    pub settings_toml: Option<String>,
    pub cancel: Option<Arc<AtomicBool>>,
    pub progress: Option<Arc<dyn Fn(ExportProgress) + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub struct ExportProgress {
    pub done: u64,
    pub total: u64,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExportReport {
    pub path: PathBuf,
    pub files: u64,
    pub bytes: u64,
    pub documents: u64,
    pub attachments: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviewManifest {
    pub library_name: String,
    pub app_version: String,
    pub format_version: u32,
    pub exported_at: i64,
    pub documents: u64,
    pub attachments: u64,
    pub total_words: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportPreview {
    pub ok: bool,
    pub manifest: Option<PreviewManifest>,
    pub documents: u64,
    pub attachments: u64,
    pub new_documents: u64,
    pub duplicate_documents: u64,
    pub missing_archive_entries: Vec<String>,
    pub total_bytes: u64,
    pub fatal_errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImportReport {
    pub summary: ImportSummary,
    pub settings_toml: Option<String>,
    pub warnings: Vec<String>,
}

struct PlannedFile {
    staged: PathBuf,
    target: PathBuf,
    backup: Option<PathBuf>,
    original_rel: String,
    remapped_rel: Option<String>,
}

/// Export an entire library as a validated ZIP with a consistent database snapshot.
pub fn export_library(
    store: &daftar_store::Store,
    library: &Library,
    output: impl AsRef<Path>,
    options: &ExportOptions,
) -> Result<ExportReport> {
    let output = output.as_ref();
    let root = Path::new(&library.root_path);
    if !root.exists() {
        return Err(ExportError::Data(format!(
            "Library folder does not exist: {}",
            root.display()
        )));
    }
    let temp = tempfile::Builder::new()
        .prefix("daftar-export-")
        .tempdir()
        .map_err(ExportError::Io)?;
    let snapshot = store.backup_to(temp.path())?;
    prune_snapshot_to_library(&snapshot, &library.id)?;

    let mut entries: Vec<ArchiveEntry> = Vec::new();
    let mut files_on_disk: Vec<(PathBuf, String, ArchiveFileKind)> = Vec::new();
    let mut total_bytes = 0u64;
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !skip_export_entry(entry.path(), root, output))
    {
        let entry = entry.map_err(|e| ExportError::Other(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if is_database_artifact(path) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(native_to_rel)
            .unwrap_or_else(|_| native_to_rel(path));
        let kind = if matches!(
            path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
            Some(ext) if ext == "md" || ext == "markdown" || ext == "mdown"
        ) {
            ArchiveFileKind::Markdown
        } else {
            ArchiveFileKind::Attachment
        };
        let (size, hash) = hash_file(path)?;
        total_bytes += size;
        let zip_path = match kind {
            ArchiveFileKind::Markdown => format!("{DOCUMENTS_PREFIX}{rel}"),
            _ => format!("{ATTACHMENTS_PREFIX}{rel}"),
        };
        entries.push(ArchiveEntry {
            path: zip_path,
            kind,
            size,
            sha256: hash,
            original_rel_path: Some(rel.clone()),
        });
        files_on_disk.push((path.to_path_buf(), rel, kind));
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));

    // Database snapshot entry.
    let (db_size, db_hash) = hash_file(&snapshot)?;
    entries.push(ArchiveEntry {
        path: DATABASE_PATH.to_string(),
        kind: ArchiveFileKind::Database,
        size: db_size,
        sha256: db_hash,
        original_rel_path: None,
    });
    if options.include_settings {
        if let Some(settings) = &options.settings_toml {
            entries.push(ArchiveEntry {
                path: SETTINGS_PATH.to_string(),
                kind: ArchiveFileKind::Settings,
                size: settings.len() as u64,
                sha256: sha256_hex(settings.as_bytes()),
                original_rel_path: None,
            });
        }
    }
    let readme = readme_text(env!("CARGO_PKG_VERSION"));
    entries.push(ArchiveEntry {
        path: README_PATH.to_string(),
        kind: ArchiveFileKind::Readme,
        size: readme.len() as u64,
        sha256: sha256_hex(readme.as_bytes()),
        original_rel_path: None,
    });

    let stats = store.stats(&library.id)?;
    let document_count = entries
        .iter()
        .filter(|e| e.kind == ArchiveFileKind::Markdown)
        .count() as u64;
    let attachment_count = entries
        .iter()
        .filter(|e| e.kind == ArchiveFileKind::Attachment)
        .count() as u64;
    total_bytes += db_size + readme.len() as u64;

    let mut metadata = BTreeMap::new();
    metadata.insert("app".to_string(), "Daftar".to_string());
    metadata.insert("generator".to_string(), "daftar-export".to_string());
    let manifest = ArchiveManifest {
        format: ARCHIVE_FORMAT.to_string(),
        format_version: ARCHIVE_FORMAT_VERSION,
        app_name: "Daftar".to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: now_ms(),
        database_schema_version: DATABASE_SCHEMA_VERSION,
        library: ArchiveLibraryInfo {
            id: library.id.clone(),
            name: library.name.clone(),
            root_path_at_export: library.root_path.clone(),
            document_count,
            attachment_count,
            total_words: stats.total_words,
        },
        statistics: ArchiveStatistics {
            documents: document_count,
            attachments: attachment_count,
            collections: stats.collections,
            categories: stats.categories,
            tags: store.tags(&library.id)?.len() as u64,
            highlights: stats.highlights,
            bookmarks: stats.bookmarks,
            flashcards: stats.flashcards,
            roadmaps: stats.roadmaps,
            reading_sessions: 0,
            study_sessions: 0,
        },
        files: entries,
        metadata,
    };
    let manifest_json = manifest.to_json().map_err(|e| ExportError::Other(e.to_string()))?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let partial = output.with_extension("daftar-partial");
    if partial.exists() {
        let _ = std::fs::remove_file(&partial);
    }
    let total_work = files_on_disk.len() as u64 + 5;
    let mut done = 0u64;
    let result = (|| -> Result<ExportReport> {
        let file = File::create(&partial)?;
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(MANIFEST_PATH, opts)?;
        zip.write_all(manifest_json.as_bytes())?;
        done += 1;
        report_progress(options, done, total_work, "Writing manifest");
        check_cancel(options)?;

        zip.start_file(README_PATH, opts)?;
        zip.write_all(readme.as_bytes())?;
        done += 1;
        if options.include_settings {
            if let Some(settings) = &options.settings_toml {
                zip.start_file(SETTINGS_PATH, opts)?;
                zip.write_all(settings.as_bytes())?;
            }
        }
        zip.start_file(DATABASE_PATH, opts)?;
        std::io::copy(&mut File::open(&snapshot)?, &mut zip)?;
        done += 1;
        report_progress(options, done, total_work, "Writing library database");
        check_cancel(options)?;

        for (path, rel, kind) in &files_on_disk {
            check_cancel(options)?;
            let zip_path = match kind {
                ArchiveFileKind::Markdown => format!("{DOCUMENTS_PREFIX}{rel}"),
                _ => format!("{ATTACHMENTS_PREFIX}{rel}"),
            };
            zip.start_file(zip_path, opts)?;
            std::io::copy(&mut File::open(path)?, &mut zip)?;
            done += 1;
            report_progress(
                options,
                done,
                total_work,
                &format!("Adding {rel}"),
            );
        }
        zip.finish()?;
        Ok(ExportReport {
            path: output.to_path_buf(),
            files: files_on_disk.len() as u64 + 3,
            bytes: total_bytes,
            documents: document_count,
            attachments: attachment_count,
        })
    })();
    match result {
        Ok(report) => {
            if output.exists() {
                std::fs::remove_file(output)?;
            }
            std::fs::rename(&partial, output)?;
            Ok(report)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            Err(error)
        }
    }
}

fn prune_snapshot_to_library(snapshot: &Path, library_id: &str) -> Result<()> {
    let conn = Connection::open(snapshot)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute("DELETE FROM libraries WHERE id != ?1", [library_id])?;
    conn.execute(
        "DELETE FROM documents_fts WHERE doc_id NOT IN (SELECT id FROM documents)",
        [],
    )?;
    conn.execute(
        "DELETE FROM import_journal",
        [],
    )?;
    conn.execute("VACUUM", [])?;
    Ok(())
}

fn skip_export_entry(path: &Path, root: &Path, output: &Path) -> bool {
    if path == output || path == root {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if path.is_dir() {
        if matches!(name, ".git" | ".daftar" | "backups" | "node_modules" | "target")
            || name.starts_with(".daftar-import")
        {
            return true;
        }
    }
    matches!(name, ".DS_Store" | "Thumbs.db")
}

fn is_database_artifact(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    name.ends_with(".db-wal")
        || name.ends_with(".db-shm")
        || name == "daftar.db"
        || (name.starts_with("daftar-") && name.ends_with(".db"))
}

fn check_cancel(options: &ExportOptions) -> Result<()> {
    if options
        .cancel
        .as_ref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        return Err(ExportError::Other("Export cancelled.".into()));
    }
    Ok(())
}

fn report_progress(options: &ExportOptions, done: u64, total: u64, message: &str) {
    if let Some(callback) = &options.progress {
        callback(ExportProgress {
            done,
            total,
            message: message.to_string(),
        });
    }
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    let mut hash = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write as _;
        let _ = write!(hash, "{byte:02x}");
    }
    Ok((size, hash))
}

fn open_archive(path: &Path) -> Result<ZipArchive<File>> {
    let file = File::open(path)?;
    ZipArchive::new(file).map_err(ExportError::Zip)
}

/// Inspect an archive without modifying the library.
pub fn preview_import(
    store: &daftar_store::Store,
    archive_path: impl AsRef<Path>,
    library_id: &str,
) -> Result<ImportPreview> {
    let archive_path = archive_path.as_ref();
    let mut archive = open_archive(archive_path)?;
    let mut manifest_text = String::new();
    archive
        .by_name(MANIFEST_PATH)
        .map_err(|_| {
            ExportError::Data("Archive is missing manifest.json.".into())
        })?
        .read_to_string(&mut manifest_text)?;
    let manifest = ArchiveManifest::from_json(&manifest_text)
        .map_err(|e| ExportError::Data(format!("Invalid manifest: {e}")))?;
    let validation = manifest.validate();
    let mut fatal = validation.fatal_errors.clone();
    let mut warnings = validation.warnings.clone();
    if manifest.database_schema_version > DATABASE_SCHEMA_VERSION {
        fatal.push(format!(
            "Archive database schema v{} is newer than this app (v{}).",
            manifest.database_schema_version, DATABASE_SCHEMA_VERSION
        ));
    }
    let names: HashSet<String> = archive
        .file_names()
        .filter_map(|s| s.ok().map(|s| s.into_owned()))
        .collect();
    let mut missing = Vec::new();
    let mut documents = 0u64;
    let mut attachments = 0u64;
    let mut total_bytes = 0u64;
    let mut duplicates = 0u64;
    for entry in &manifest.files {
        if !names.contains(&entry.path) {
            missing.push(entry.path.clone());
        }
        total_bytes += entry.size;
        match entry.kind {
            ArchiveFileKind::Markdown => {
                documents += 1;
                if let Some(rel) = &entry.original_rel_path {
                    if store
                        .document_by_rel_path(library_id, rel)?
                        .is_some()
                    {
                        duplicates += 1;
                    }
                }
            }
            ArchiveFileKind::Attachment => attachments += 1,
            _ => {}
        }
    }
    if !missing.is_empty() {
        fatal.push(format!(
            "{} file(s) listed in the manifest are missing from the archive.",
            missing.len()
        ));
    }
    if total_bytes > 8 * 1024 * 1024 * 1024 {
        fatal.push("Archive is larger than the 8 GiB safety limit.".into());
    }
    if manifest.database_schema_version < DATABASE_SCHEMA_VERSION {
        warnings.push(format!(
            "Archive uses database schema v{}; it will be migrated after import.",
            manifest.database_schema_version
        ));
    }
    Ok(ImportPreview {
        ok: fatal.is_empty(),
        manifest: Some(PreviewManifest {
            library_name: manifest.library.name.clone(),
            app_version: manifest.app_version.clone(),
            format_version: manifest.format_version,
            exported_at: manifest.exported_at,
            documents,
            attachments,
            total_words: manifest.library.total_words,
        }),
        documents,
        attachments,
        new_documents: documents.saturating_sub(duplicates),
        duplicate_documents: duplicates,
        missing_archive_entries: missing,
        total_bytes,
        fatal_errors: fatal,
        warnings,
    })
}

/// Validate, stage, apply and roll back on failure.
pub fn apply_import(
    store: &daftar_store::Store,
    archive_path: impl AsRef<Path>,
    library_id: &str,
    library_root: impl AsRef<Path>,
    options: &ImportOptions,
) -> Result<ImportReport> {
    let archive_path = archive_path.as_ref();
    let root = library_root.as_ref();
    let preview = preview_import(store, archive_path, library_id)?;
    if !preview.ok {
        return Err(ExportError::Data(format!(
            "Archive validation failed: {}",
            preview.fatal_errors.join(" ")
        )));
    }
    let mut archive = open_archive(archive_path)?;
    let mut manifest_text = String::new();
    archive
        .by_name(MANIFEST_PATH)
        .map_err(|_| ExportError::Data("Archive is missing manifest.json.".into()))?
        .read_to_string(&mut manifest_text)?;
    let manifest = ArchiveManifest::from_json(&manifest_text)
        .map_err(|e| ExportError::Data(format!("Invalid manifest: {e}")))?;
    let library = store
        .library(library_id)?
        .ok_or_else(|| ExportError::Data("Target library does not exist.".into()))?;

    std::fs::create_dir_all(root)?;
    let staging = root.join(format!(".daftar-import-{}", uuid::Uuid::new_v4()));
    let staged_files = staging.join("files");
    std::fs::create_dir_all(&staged_files)?;
    let staged_db = staging.join("library.db");
    let backup_root = staging.join("backup");

    let result = (|| -> Result<ImportReport> {
        // Extract and verify everything first.
        let mut settings_toml = None;
        for entry in &manifest.files {
            if crate::archive::is_zip_slip(&entry.path) {
                return Err(ExportError::Data(format!(
                    "Unsafe path in archive: {}",
                    entry.path
                )));
            }
            if entry.kind == ArchiveFileKind::Manifest {
                continue;
            }
            let mut file = archive
                .by_name(&entry.path)
                .map_err(|_| ExportError::Data(format!("Missing archive entry {}", entry.path)))?;
            match entry.kind {
                ArchiveFileKind::Readme => continue,
                ArchiveFileKind::Database => {
                    copy_and_verify(&mut file, &staged_db, entry)?;
                }
                ArchiveFileKind::Settings => {
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)?;
                    if sha256_hex(&bytes) != entry.sha256 {
                        return Err(ExportError::Data(
                            "Settings file failed checksum validation.".into(),
                        ));
                    }
                    settings_toml = Some(
                        String::from_utf8(bytes)
                            .map_err(|_| ExportError::Data("Settings are not valid UTF-8.".into()))?,
                    );
                }
                ArchiveFileKind::Markdown | ArchiveFileKind::Attachment => {
                    let rel = entry.original_rel_path.as_deref().ok_or_else(|| {
                        ExportError::Data(format!(
                            "Manifest entry {} has no original path.",
                            entry.path
                        ))
                    })?;
                    if is_zip_slip(rel) {
                        return Err(ExportError::Data(format!(
                            "Unsafe original path: {rel}"
                        )));
                    }
                    let target = safe_join(&staged_files, rel)?;
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    copy_and_verify(&mut file, &target, entry)?;
                }
                ArchiveFileKind::Manifest => {}
            }
        }

        // Plan duplicate handling and path remapping for the database merge.
        let mut path_remap: HashMap<String, String> = HashMap::new();
        let mut plan: Vec<PlannedFile> = Vec::new();
        let mut warnings = Vec::new();
        for entry in &manifest.files {
            let (ArchiveFileKind::Markdown | ArchiveFileKind::Attachment) = entry.kind else {
                continue;
            };
            let Some(rel) = entry.original_rel_path.clone() else {
                continue;
            };
            let staged = safe_join(&staged_files, &rel)?;
            let target = safe_join(root, &rel)?;
            if target.exists() {
                match options.duplicate_strategy {
                    DuplicateStrategy::Skip => {
                        warnings.push(format!("Skipped existing file {rel}."));
                        continue;
                    }
                    DuplicateStrategy::Replace => {
                        let backup = safe_join(&backup_root, &rel)?;
                        if let Some(parent) = backup.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::copy(&target, &backup)?;
                        plan.push(PlannedFile {
                            staged,
                            target,
                            backup: Some(backup),
                            original_rel: rel.clone(),
                            remapped_rel: None,
                        });
                    }
                    DuplicateStrategy::Rename => {
                        if entry.kind == ArchiveFileKind::Markdown {
                            let new_rel = unique_rel_path(root, &rel);
                            path_remap.insert(rel.clone(), new_rel.clone());
                            plan.push(PlannedFile {
                                staged,
                                target: safe_join(root, &new_rel)?,
                                backup: None,
                                original_rel: rel.clone(),
                                remapped_rel: Some(new_rel),
                            });
                        } else {
                            warnings.push(format!(
                                "Attachment {rel} already exists; kept the existing file."
                            ));
                        }
                    }
                }
            } else {
                plan.push(PlannedFile {
                    staged,
                    target,
                    backup: None,
                    original_rel: rel.clone(),
                    remapped_rel: None,
                });
            }
        }

        // Move files. Each move is atomic within the library filesystem.
        let mut moved: Vec<PathBuf> = Vec::new();
        for item in &plan {
            if let Some(parent) = item.target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if item.target.exists() {
                std::fs::remove_file(&item.target)?;
            }
            if std::fs::rename(&item.staged, &item.target).is_err() {
                std::fs::copy(&item.staged, &item.target)?;
                let _ = std::fs::remove_file(&item.staged);
            }
            moved.push(item.target.clone());
        }

        // Database merge. Any failure rolls back newly moved files and restores backups.
        let merge_result = if staged_db.exists() {
            daftar_store::archive_import::merge_snapshot(
                store,
                &staged_db,
                library_id,
                options.duplicate_strategy,
                options.restore_flashcards,
                options.restore_roadmaps,
                &path_remap,
            )
        } else {
            import_without_database(store, &library, &plan, &mut warnings)
        };
        let summary = match merge_result {
            Ok(summary) => summary,
            Err(error) => {
                rollback(&plan, &moved);
                return Err(ExportError::Store(error));
            }
        };

        let reindex_warnings = reindex_library(store, &library, root);
        warnings.extend(reindex_warnings);
        let _ = store.journal_import("library_import", &format!("{:?}", summary));
        let settings = if options.restore_settings {
            settings_toml
        } else {
            None
        };
        Ok(ImportReport {
            summary,
            settings_toml: settings,
            warnings,
        })
    })();

    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    } else {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn import_without_database(
    store: &daftar_store::Store,
    library: &Library,
    plan: &[PlannedFile],
    warnings: &mut Vec<String>,
) -> std::result::Result<ImportSummary, daftar_store::StoreError> {
    let mut summary = ImportSummary::default();
    for item in plan {
        if !is_markdown(&item.target) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&item.target) else {
            warnings.push(format!("Could not read {}", item.original_rel));
            continue;
        };
        let parsed = daftar_core::parse_markdown(&source);
        let rel = item
            .remapped_rel
            .clone()
            .unwrap_or_else(|| item.original_rel.clone());
        let title = parsed
            .title
            .clone()
            .unwrap_or_else(|| file_stem(&item.target));
        let mut doc = Document::new(library.id.clone(), rel, title);
        doc.word_count = parsed.word_count as u64;
        doc.direction = parsed
            .direction_hint
            .unwrap_or(parsed.detected_direction);
        doc.updated_at = now_ms();
        store.upsert_document(&doc)?;
        store.index_document(
            &doc.id,
            &doc.title,
            &parsed.plain_text(),
            &parsed
                .toc
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            &doc.rel_path,
        )?;
        summary.documents_imported += 1;
    }
    Ok(summary)
}

fn reindex_library(store: &daftar_store::Store, library: &Library, root: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    let docs = match store.documents(&library.id) {
        Ok(docs) => docs,
        Err(error) => return vec![format!("Could not list documents for indexing: {error}")],
    };
    for mut doc in docs {
        let path = root.join(rel_to_native(&doc.rel_path));
        if !path.exists() {
            let _ = store.set_document_missing(&doc.id, true);
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let hash = sha256_hex(&bytes);
        let source = String::from_utf8_lossy(&bytes).to_string();
        let parsed = daftar_core::parse_markdown(&source);
        let mtime = std::fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        if doc.content_hash != hash {
            doc.title = parsed
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_else(|| doc.title.clone());
            doc.word_count = parsed.word_count as u64;
            let _ = store.update_document_title(&doc.id, &doc.title);
        }
        if let Err(error) = store.update_file_metadata(
            &doc.id,
            mtime,
            bytes.len() as u64,
            &hash,
            parsed.word_count as u64,
        ) {
            warnings.push(format!("Could not update metadata for {}: {error}", doc.rel_path));
        }
        let headings = parsed
            .toc
            .iter()
            .map(|t| t.title.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if let Err(error) = store.index_document(
            &doc.id,
            &doc.title,
            &parsed.plain_text(),
            &headings,
            &doc.rel_path,
        ) {
            warnings.push(format!("Could not index {}: {error}", doc.rel_path));
        }
        let _ = store.set_document_missing(&doc.id, false);
    }
    warnings
}

fn rollback(plan: &[PlannedFile], moved: &[PathBuf]) {
    for target in moved {
        let _ = std::fs::remove_file(target);
    }
    for item in plan {
        if let Some(backup) = &item.backup {
            if let Some(parent) = item.target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(backup, &item.target);
        }
    }
}

fn copy_and_verify(reader: &mut impl Read, target: &Path, entry: &ArchiveEntry) -> Result<()> {
    let mut file = File::create(target)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > entry.size.saturating_add(1024 * 1024) {
            return Err(ExportError::Data(format!(
                "Archive entry {} is larger than declared.",
                entry.path
            )));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])?;
    }
    file.flush()?;
    if size != entry.size {
        return Err(ExportError::Data(format!(
            "Archive entry {} size mismatch (declared {}, actual {}).",
            entry.path, entry.size, size
        )));
    }
    let hash = {
        let mut out = String::with_capacity(64);
        for byte in hasher.finalize() {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
        }
        out
    };
    if !entry.sha256.is_empty() && hash != entry.sha256 {
        return Err(ExportError::Data(format!(
            "Archive entry {} failed checksum validation.",
            entry.path
        )));
    }
    Ok(())
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    if is_zip_slip(rel) {
        return Err(ExportError::Data(format!("Unsafe relative path: {rel}")));
    }
    let mut path = root.to_path_buf();
    for component in Path::new(rel).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => {
                return Err(ExportError::Data(format!(
                    "Unsafe relative path: {rel}"
                )));
            }
        }
    }
    Ok(path)
}

fn unique_rel_path(root: &Path, rel: &str) -> String {
    let path = Path::new(rel);
    let parent = path.parent().map(native_to_rel).unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for counter in 1..1000 {
        let name = format!("{stem} (imported {counter}){ext}");
        let candidate = if parent.is_empty() {
            name
        } else {
            format!("{parent}/{name}")
        };
        if !root.join(rel_to_native(&candidate)).exists() {
            return candidate;
        }
    }
    format!("imported-{}-{}", crate::archive::timestamp(), rel)
}

fn timestamp() -> String {
    chrono::Utc::now().format("%Y%m%d%H%M%S").to_string()
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported document")
        .to_string()
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
        Some(ext) if ext == "md" || ext == "markdown" || ext == "mdown"
    )
}

pub fn default_staging_dir(library_root: &Path) -> PathBuf {
    library_root.join(".daftar-import")
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_core::model::{Category, Collection, Flashcard, Highlight, Library, Roadmap, RoadmapNode};
    use daftar_store::Store;

    fn fixture(root: &Path) -> (Store, Library) {
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::write(
            root.join("notes/english.md"),
            "# English\n\nA **test** document with [a link](https://example.com).\n",
        )
        .unwrap();
        std::fs::write(
            root.join("notes/persian.md"),
            "# سلام دنیا\n\nاین یک متن فارسی برای آزمون است.\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("images")).unwrap();
        std::fs::write(root.join("images/pic.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();

        let store = Store::open_in_memory().unwrap();
        let library = Library::new("Test Library", root.to_string_lossy().to_string());
        store.upsert_library(&library).unwrap();
        for (rel, title, body) in [
            ("notes/english.md", "English", "A test document with a link."),
            ("notes/persian.md", "سلام دنیا", "این یک متن فارسی برای آزمون است."),
        ] {
            let doc = Document::new(library.id.clone(), rel, title);
            store.upsert_document(&doc).unwrap();
            store.index_document(&doc.id, title, body, "", rel).unwrap();
        }
        let english = store
            .document_by_rel_path(&library.id, "notes/english.md")
            .unwrap()
            .unwrap();
        let mut highlight = Highlight::new(english.id.clone(), "test".into(), "#FFD966");
        highlight.tags = vec!["rust".into()];
        highlight.start_offset = 11;
        highlight.end_offset = 15;
        store.upsert_highlight(&highlight).unwrap();
        let mut card = Flashcard::new("What is tested?", "The integration");
        card.document_id = Some(english.id.clone());
        store.upsert_flashcard(&card).unwrap();
        let category = Category::new(library.id.clone(), "Programming", 0);
        store.upsert_category(&category).unwrap();
        let collection = Collection::new(library.id.clone(), "Reading", 0);
        store.upsert_collection(&collection).unwrap();
        let roadmap = Roadmap::new(library.id.clone(), "Plan");
        store.upsert_roadmap(&roadmap).unwrap();
        let node = RoadmapNode::new(
            roadmap.id.clone(),
            daftar_core::model::NodeKind::Task,
            "Read the docs",
            0,
        );
        store.upsert_roadmap_node(&node).unwrap();
        (store, library)
    }

    #[test]
    fn round_trip_export_import() {
        let source_dir = tempfile::tempdir().unwrap();
        let (source, library) = fixture(source_dir.path());
        let export_dir = tempfile::tempdir().unwrap();
        let zip_path = export_dir.path().join("library.daftar.zip");
        let report = export_library(
            &source,
            &library,
            &zip_path,
            &ExportOptions::default(),
        )
        .unwrap();
        assert_eq!(report.documents, 2);
        assert_eq!(report.attachments, 1);
        assert!(zip_path.exists());
        let preview = preview_import(&source, &zip_path, &library.id).unwrap();
        assert!(preview.ok, "{:?}", preview.fatal_errors);
        assert_eq!(preview.documents, 2);
        assert_eq!(preview.duplicate_documents, 2);

        // Import into a fresh library/store.
        let target_dir = tempfile::tempdir().unwrap();
        let target = Store::open_in_memory().unwrap();
        let target_library = Library::new("Target", target_dir.path().to_string_lossy().to_string());
        target.upsert_library(&target_library).unwrap();
        let preview = preview_import(&target, &zip_path, &target_library.id).unwrap();
        assert!(preview.ok, "{:?}", preview.fatal_errors);
        assert_eq!(preview.new_documents, 2);
        let report = apply_import(
            &target,
            &zip_path,
            &target_library.id,
            target_dir.path(),
            &ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(report.summary.documents_imported, 2);
        assert_eq!(report.summary.highlights, 1);
        assert_eq!(report.summary.flashcards, 1);
        assert_eq!(report.summary.roadmaps, 1);
        assert!(target_dir.path().join("notes/english.md").exists());
        assert!(target_dir.path().join("images/pic.svg").exists());
        let docs = target.documents(&target_library.id).unwrap();
        assert_eq!(docs.len(), 2);
        let hits = target
            .search(&daftar_store::SearchFilters::new(&target_library.id, "test"))
            .unwrap();
        assert!(!hits.is_empty());
        let hits = target
            .search(&daftar_store::SearchFilters::new(&target_library.id, "فارسی"))
            .unwrap();
        assert!(!hits.is_empty());
        assert_eq!(target.all_highlights(&target_library.id).unwrap().len(), 1);
        assert_eq!(target.flashcards().unwrap().len(), 1);
        assert_eq!(target.roadmaps(&target_library.id).unwrap().len(), 1);
    }

    #[test]
    fn skip_then_rename_on_duplicate_import() {
        let source_dir = tempfile::tempdir().unwrap();
        let (source, library) = fixture(source_dir.path());
        let export_dir = tempfile::tempdir().unwrap();
        let zip_path = export_dir.path().join("lib.zip");
        export_library(&source, &library, &zip_path, &ExportOptions::default()).unwrap();

        let target_dir = tempfile::tempdir().unwrap();
        let target = Store::open_in_memory().unwrap();
        let target_library = Library::new("Target", target_dir.path().to_string_lossy().to_string());
        target.upsert_library(&target_library).unwrap();
        apply_import(
            &target,
            &zip_path,
            &target_library.id,
            target_dir.path(),
            &ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(target.documents(&target_library.id).unwrap().len(), 2);
        // Default Skip should keep two documents and create no duplicates.
        apply_import(
            &target,
            &zip_path,
            &target_library.id,
            target_dir.path(),
            &ImportOptions::default(),
        )
        .unwrap();
        assert_eq!(target.documents(&target_library.id).unwrap().len(), 2);
        // Rename should keep both copies.
        let options = ImportOptions {
            duplicate_strategy: DuplicateStrategy::Rename,
            ..Default::default()
        };
        apply_import(&target, &zip_path, &target_library.id, target_dir.path(), &options).unwrap();
        let docs = target.documents(&target_library.id).unwrap();
        assert_eq!(docs.len(), 4);
        assert!(docs.iter().any(|d| d.rel_path.contains("imported")));
    }

    #[test]
    fn rejects_corrupt_archive() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("bad.zip");
        std::fs::write(&zip_path, b"not a zip").unwrap();
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("L", dir.path().to_string_lossy().to_string());
        store.upsert_library(&library).unwrap();
        assert!(preview_import(&store, &zip_path, &library.id).is_err());
    }

    #[test]
    fn settings_round_trip() {
        let source_dir = tempfile::tempdir().unwrap();
        let (source, library) = fixture(source_dir.path());
        let export_dir = tempfile::tempdir().unwrap();
        let zip_path = export_dir.path().join("with-settings.zip");
        let settings = "version = 1\n[general]\nlanguage = \"fa\"\n";
        export_library(
            &source,
            &library,
            &zip_path,
            &ExportOptions {
                include_settings: true,
                settings_toml: Some(settings.to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        let target_dir = tempfile::tempdir().unwrap();
        let target = Store::open_in_memory().unwrap();
        let target_library = Library::new("T", target_dir.path().to_string_lossy().to_string());
        target.upsert_library(&target_library).unwrap();
        let report = apply_import(
            &target,
            &zip_path,
            &target_library.id,
            target_dir.path(),
            &ImportOptions {
                restore_settings: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(report.settings_toml.as_deref(), Some(settings));
    }

    #[test]
    fn unsafe_paths_are_rejected() {
        assert!(safe_join(Path::new("/tmp"), "../evil").is_err());
        assert!(safe_join(Path::new("/tmp"), "/abs").is_err());
    }
}
