//! Background indexing, file watching and long-running export/import jobs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::SystemTime;

use daftar_core::archive::ImportOptions;
use daftar_core::model::{Document, Library, native_to_rel};
use daftar_core::util::sha256_hex;
use daftar_export::{ExportOptions, ExportReport, ImportReport};
use daftar_store::Store;
use walkdir::WalkDir;

#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub added: u64,
    pub updated: u64,
    pub missing: u64,
    pub skipped: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum JobEvent {
    Progress {
        job: String,
        done: u64,
        total: u64,
        message: String,
    },
    IndexDone(ScanSummary),
    ExportDone(std::result::Result<ExportReport, String>),
    ImportDone(std::result::Result<ImportReport, String>),
    Failed {
        job: String,
        error: String,
    },
}

#[derive(Debug, Clone)]
pub struct JobProgress {
    pub job: String,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

pub struct Jobs {
    tx: Sender<JobEvent>,
    rx: Receiver<JobEvent>,
    cancel: Option<Arc<AtomicBool>>,
    pub active: Option<String>,
    pub progress: Option<JobProgress>,
    pub last_message: String,
}

impl Jobs {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            tx,
            rx,
            cancel: None,
            active: None,
            progress: None,
            last_message: String::new(),
        }
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub fn cancel(&mut self) {
        if let Some(flag) = &self.cancel {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn poll(&mut self) -> Vec<JobEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            match &event {
                JobEvent::Progress { job, done, total, message } => {
                    self.progress = Some(JobProgress {
                        job: job.clone(),
                        done: *done,
                        total: *total,
                        message: message.clone(),
                    });
                }
                JobEvent::IndexDone(_) | JobEvent::ExportDone(_) | JobEvent::ImportDone(_) => {
                    self.active = None;
                    self.progress = None;
                    self.cancel = None;
                }
                JobEvent::Failed { .. } => {
                    self.active = None;
                    self.progress = None;
                    self.cancel = None;
                }
            }
            events.push(event);
        }
        events
    }

    pub fn start_index(&mut self, db_path: PathBuf, library: Library) -> bool {
        if self.active.is_some() {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.active = Some(crate::i18n::t("Indexing").to_string());
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("daftar-index".into())
            .spawn(move || {
                let result = (|| -> Result<ScanSummary, String> {
                    let store = Store::open(&db_path).map_err(|e| e.to_string())?;
                    let summary = scan_library(&store, &library, Some(&tx), Some(&cancel));
                    Ok(summary)
                })();
                match result {
                    Ok(summary) => {
                        let _ = tx.send(JobEvent::IndexDone(summary));
                    }
                    Err(error) => {
                        let _ = tx.send(JobEvent::Failed {
                            job: crate::i18n::t("Indexing").into(),
                            error,
                        });
                    }
                }
            })
            .ok();
        true
    }

    pub fn start_sync_paths(
        &mut self,
        db_path: PathBuf,
        library: Library,
        paths: Vec<PathBuf>,
    ) -> bool {
        if self.active.is_some() || paths.is_empty() {
            return false;
        }
        self.active = Some(crate::i18n::t("Updating library").to_string());
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("daftar-sync".into())
            .spawn(move || {
                match Store::open(&db_path) {
                    Ok(store) => {
                        let summary = sync_paths(&store, &library, &paths);
                        let _ = tx.send(JobEvent::IndexDone(summary));
                    }
                    Err(error) => {
                        let _ = tx.send(JobEvent::Failed {
                            job: "Updating library".into(),
                            error: error.to_string(),
                        });
                    }
                }
            })
            .ok();
        true
    }

    pub fn start_export(
        &mut self,
        db_path: PathBuf,
        library: Library,
        output: PathBuf,
        include_settings: bool,
        settings_toml: Option<String>,
    ) -> bool {
        if self.active.is_some() {
            return false;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        self.active = Some(crate::i18n::t("Exporting library").to_string());
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("daftar-export".into())
            .spawn(move || {
                let result = (|| -> Result<ExportReport, String> {
                    let store = Store::open(&db_path).map_err(|e| e.to_string())?;
                    let tx_progress = tx.clone();
                    let options = ExportOptions {
                        include_settings,
                        settings_toml,
                        cancel: Some(cancel),
                        progress: Some(Arc::new(move |progress| {
                            let _ = tx_progress.send(JobEvent::Progress {
                                job: crate::i18n::t("Exporting library").into(),
                                done: progress.done,
                                total: progress.total,
                                message: progress.message,
                            });
                        })),
                    };
                    daftar_export::export_library(&store, &library, &output, &options)
                        .map_err(|e| e.to_string())
                })();
                let _ = tx.send(JobEvent::ExportDone(result));
            })
            .ok();
        true
    }

    pub fn start_import(
        &mut self,
        db_path: PathBuf,
        archive: PathBuf,
        library_id: String,
        library_root: PathBuf,
        options: ImportOptions,
    ) -> bool {
        if self.active.is_some() {
            return false;
        }
        self.active = Some(crate::i18n::t("Importing library").to_string());
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("daftar-import".into())
            .spawn(move || {
                let result = (|| -> Result<ImportReport, String> {
                    let store = Store::open(&db_path).map_err(|e| e.to_string())?;
                    daftar_export::apply_import(
                        &store,
                        &archive,
                        &library_id,
                        &library_root,
                        &options,
                    )
                    .map_err(|e| e.to_string())
                })();
                let _ = tx.send(JobEvent::ImportDone(result));
            })
            .ok();
        true
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan the whole library: add new Markdown files, update metadata and the FTS index, and mark
/// documents whose files disappeared. Files and metadata are never deleted by a scan.
pub fn scan_library(
    store: &Store,
    library: &Library,
    progress: Option<&Sender<JobEvent>>,
    cancel: Option<&AtomicBool>,
) -> ScanSummary {
    let mut summary = ScanSummary::default();
    let root = Path::new(&library.root_path);
    if !root.is_dir() {
        summary
            .warnings
            .push(crate::i18n::tf("Library folder is missing: {}", &[&root.display().to_string()]));
        return summary;
    }
    let docs = store.documents(&library.id).unwrap_or_default();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut paths: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if entry.file_type().is_dir()
                && (matches!(name.as_ref(), ".git" | ".daftar" | "backups" | "node_modules" | "target")
                    || name.starts_with(".daftar-import"))
            {
                return false;
            }
            true
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && is_markdown(entry.path()))
        .map(|entry| entry.into_path())
        .collect();
    paths.sort();
    let total = paths.len() as u64;
    for (index, path) in paths.iter().enumerate() {
        if cancel.is_some_and(|f| f.load(Ordering::Relaxed)) {
            summary.warnings.push(crate::i18n::t("Indexing cancelled.").into());
            break;
        }
        let rel = path
            .strip_prefix(root)
            .map(native_to_rel)
            .unwrap_or_else(|_| native_to_rel(path));
        seen.insert(rel.clone());
        match index_file(store, library, path, &rel, doc_title_hint(path)) {
            Ok(true) => summary.added += 1,
            Ok(false) => summary.updated += 1,
            Err(error) => {
                summary.skipped += 1;
                if summary.warnings.len() < 20 {
                    summary.warnings.push(format!("{rel}: {error}"));
                }
            }
        }
        if let Some(tx) = progress {
            if index % 15 == 0 || index as u64 + 1 == total {
                let _ = tx.send(JobEvent::Progress {
                    job: crate::i18n::t("Indexing").into(),
                    done: index as u64 + 1,
                    total,
                    message: crate::i18n::tf("Indexing {}", &[&rel]),
                });
            }
        }
    }
    for doc in docs {
        if !seen.contains(&doc.rel_path) && !doc.is_missing {
            if store.set_document_missing(&doc.id, true).is_ok() {
                summary.missing += 1;
            }
        }
    }
    summary
}

/// Incrementally update a small set of filesystem paths.
pub fn sync_paths(store: &Store, library: &Library, paths: &[PathBuf]) -> ScanSummary {
    let mut summary = ScanSummary::default();
    let root = Path::new(&library.root_path);
    for path in paths {
        if !is_markdown(path) {
            continue;
        }
        let rel = match path.strip_prefix(root) {
            Ok(rel) => native_to_rel(rel),
            Err(_) => continue,
        };
        if path.exists() {
            match index_file(store, library, path, &rel, doc_title_hint(path)) {
                Ok(true) => summary.added += 1,
                Ok(false) => summary.updated += 1,
                Err(error) => summary.warnings.push(format!("{rel}: {error}")),
            }
        } else if let Ok(Some(doc)) = store.document_by_rel_path(&library.id, &rel) {
            if store.set_document_missing(&doc.id, true).is_ok() {
                summary.missing += 1;
            }
        }
    }
    summary
}

/// Index one file. Returns `Ok(true)` when a new document row was created.
fn index_file(
    store: &Store,
    library: &Library,
    path: &Path,
    rel: &str,
    fallback_title: String,
) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let source = String::from_utf8_lossy(&bytes).to_string();
    let parsed = daftar_core::parse_markdown(&source);
    let hash = sha256_hex(&bytes);
    let mtime = std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let headings = parsed
        .toc
        .iter()
        .map(|t| t.title.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let body = parsed.plain_text();
    if let Ok(Some(mut doc)) = store.document_by_rel_path(&library.id, rel) {
        if doc.content_hash != hash {
            if let Some(title) = &parsed.title {
                if !title.trim().is_empty() && (doc.title.trim().is_empty() || doc.title == fallback_title) {
                    doc.title = title.clone();
                    let _ = store.update_document_title(&doc.id, &doc.title);
                }
            }
            let _ = store.update_file_metadata(
                &doc.id,
                mtime,
                bytes.len() as u64,
                &hash,
                parsed.word_count as u64,
            );
        }
        store
            .index_document(&doc.id, &doc.title, &body, &headings, rel)
            .map_err(|e| e.to_string())?;
        let _ = store.set_document_missing(&doc.id, false);
        Ok(false)
    } else {
        let mut doc = Document::new(library.id.clone(), rel.to_string(), fallback_title);
        if let Some(title) = &parsed.title {
            if !title.trim().is_empty() {
                doc.title = title.clone();
            }
        }
        doc.word_count = parsed.word_count as u64;
        doc.direction = parsed
            .direction_hint
            .unwrap_or(parsed.detected_direction);
        doc.file_mtime_ms = mtime;
        doc.file_size = bytes.len() as u64;
        doc.content_hash = hash;
        store.upsert_document(&doc).map_err(|e| e.to_string())?;
        store
            .index_document(&doc.id, &doc.title, &body, &headings, rel)
            .map_err(|e| e.to_string())?;
        Ok(true)
    }
}

fn doc_title_hint(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace(['-', '_'], " ")
        .trim()
        .to_string()
}

pub fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()),
        Some(ext) if ext == "md" || ext == "markdown" || ext == "mdown"
    )
}

// ----------------------------------------------------------------------------------------
// File watching
// ----------------------------------------------------------------------------------------

pub struct LibraryWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<PathBuf>,
}

impl LibraryWatcher {
    pub fn start(root: &Path) -> Result<Self, String> {
        use notify::Watcher;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            for path in event.paths {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.starts_with(".daftar") || name == ".git" {
                    continue;
                }
                let _ = tx.send(path);
            }
        })
        .map_err(|e| crate::i18n::tf("Could not start file watcher: {}", &[&e.to_string()]))?;
        watcher
            .watch(root, notify::RecursiveMode::Recursive)
            .map_err(|e| crate::i18n::tf("Could not watch library folder: {}", &[&e.to_string()]))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Non-blocking drain with de-duplication, cap 200 paths per frame.
    pub fn drain(&self) -> Vec<PathBuf> {
        let mut paths = std::collections::BTreeSet::new();
        for _ in 0..200 {
            match self.rx.try_recv() {
                Ok(path) => {
                    paths.insert(path);
                }
                Err(_) => break,
            }
        }
        paths.into_iter().collect()
    }
}