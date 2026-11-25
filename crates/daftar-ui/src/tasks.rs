//! Background indexing, file watching and long-running export/import jobs.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::SystemTime;
use std::{fs::OpenOptions, io::Write};

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
    pub unchanged: u64,
    pub missing: u64,
    pub skipped: u64,
    pub processed: u64,
    pub total: u64,
    pub cancelled: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum JobEvent {
    Started {
        operation_id: u64,
    },
    CancelStarted {
        operation_id: u64,
    },
    Progress {
        operation_id: u64,
        job: String,
        done: u64,
        total: u64,
        message: String,
    },
    IndexDone {
        operation_id: u64,
        summary: ScanSummary,
    },
    ExportDone {
        operation_id: u64,
        result: std::result::Result<ExportReport, String>,
        cancelled: bool,
    },
    ImportDone {
        operation_id: u64,
        result: std::result::Result<ImportReport, String>,
    },
    Failed {
        operation_id: u64,
        job: String,
        error: String,
    },
}

impl JobEvent {
    fn operation_id(&self) -> u64 {
        match self {
            Self::Started { operation_id }
            | Self::CancelStarted { operation_id }
            | Self::Progress { operation_id, .. }
            | Self::IndexDone { operation_id, .. }
            | Self::ExportDone { operation_id, .. }
            | Self::ImportDone { operation_id, .. }
            | Self::Failed { operation_id, .. } => *operation_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JobPhase {
    #[default]
    Idle,
    Initializing,
    Running,
    CancelRequested,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Default)]
pub struct JobState {
    pub operation_id: u64,
    pub job: Option<String>,
    pub phase: JobPhase,
    pub error: Option<String>,
    cancellable: bool,
}

impl JobState {
    fn is_busy(&self) -> bool {
        matches!(
            self.phase,
            JobPhase::Initializing | JobPhase::Running | JobPhase::CancelRequested | JobPhase::Cancelling
        )
    }

    fn can_cancel(&self) -> bool {
        self.cancellable && matches!(self.phase, JobPhase::Initializing | JobPhase::Running)
    }

    fn cancel_requested(&self) -> bool {
        matches!(self.phase, JobPhase::CancelRequested | JobPhase::Cancelling)
    }
}

#[derive(Debug, Clone)]
pub struct JobProgress {
    pub operation_id: u64,
    pub job: String,
    pub done: u64,
    pub total: u64,
    pub message: String,
}

impl JobProgress {
    pub fn fraction(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.done as f32 / self.total as f32).clamp(0.0, 1.0))
    }
}

pub struct Jobs {
    tx: Sender<JobEvent>,
    rx: Receiver<JobEvent>,
    cancel: Option<Arc<AtomicBool>>,
    next_operation_id: u64,
    pub state: JobState,
    pub progress: Option<JobProgress>,
}

impl Jobs {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self {
            tx,
            rx,
            cancel: None,
            next_operation_id: 0,
            state: JobState::default(),
            progress: None,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.state.is_busy()
    }

    pub fn can_cancel(&self) -> bool {
        self.state.can_cancel() && self.cancel.is_some()
    }

    pub fn cancel_requested(&self) -> bool {
        self.state.cancel_requested()
    }

    pub fn cancel(&mut self) {
        if self.can_cancel() {
            let Some(flag) = &self.cancel else { return };
            flag.store(true, Ordering::Relaxed);
            self.state.phase = JobPhase::CancelRequested;
        }
    }

    pub fn poll(&mut self) -> Vec<JobEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.rx.try_recv() {
            if event.operation_id() != self.state.operation_id {
                continue;
            }
            match event {
                JobEvent::Started { operation_id } => {
                    if self.state.phase == JobPhase::Initializing {
                        self.state.phase = JobPhase::Running;
                    }
                    events.push(JobEvent::Started { operation_id });
                }
                JobEvent::CancelStarted { operation_id } => {
                    if self.state.phase == JobPhase::CancelRequested {
                        self.state.phase = JobPhase::Cancelling;
                    }
                    events.push(JobEvent::CancelStarted { operation_id });
                }
                JobEvent::Progress { operation_id, job, done, total, message } => {
                    if !self.is_busy() || total == 0 {
                        continue;
                    }
                    let done = done.min(total.saturating_sub(1));
                    let done = match &self.progress {
                        Some(current) if current.operation_id == operation_id => {
                            if current.total != total {
                                continue;
                            }
                            done.max(current.done)
                        }
                        _ => done,
                    };
                    let unchanged = self.progress.as_ref().is_some_and(|current| {
                        current.operation_id == operation_id
                            && current.done == done
                            && current.total == total
                    });
                    if unchanged {
                        continue;
                    }
                    self.progress = Some(JobProgress {
                        operation_id,
                        job: job.clone(),
                        done,
                        total,
                        message: message.clone(),
                    });
                    events.push(JobEvent::Progress { operation_id, job, done, total, message });
                }
                JobEvent::IndexDone { operation_id, summary } => {
                    self.state.phase = if summary.cancelled { JobPhase::Cancelled } else { JobPhase::Completed };
                    self.state.cancellable = false;
                    self.cancel = None;
                    if summary.cancelled {
                        self.progress = None;
                    } else if let Some(progress) = self.progress.as_mut().filter(|p| p.operation_id == operation_id) {
                        progress.done = progress.total;
                        progress.message = crate::i18n::t("Indexing complete").into();
                    }
                    events.push(JobEvent::IndexDone { operation_id, summary });
                }
                JobEvent::ExportDone { operation_id, result, cancelled } => {
                    self.state.phase = if cancelled { JobPhase::Cancelled }
                    else if result.is_ok() { JobPhase::Completed }
                    else { JobPhase::Failed };
                    if let Err(error) = &result {
                        self.state.error = Some(error.clone());
                    } else if let Some(progress) = self.progress.as_mut().filter(|p| p.operation_id == operation_id) {
                        progress.done = progress.total;
                    }
                    self.state.cancellable = false;
                    self.cancel = None;
                    events.push(JobEvent::ExportDone { operation_id, result, cancelled });
                }
                JobEvent::ImportDone { operation_id, result } => {
                    self.state.phase = if result.is_ok() { JobPhase::Completed } else { JobPhase::Failed };
                    if let Err(error) = &result {
                        self.state.error = Some(error.clone());
                    }
                    self.state.cancellable = false;
                    events.push(JobEvent::ImportDone { operation_id, result });
                }
                JobEvent::Failed { operation_id, job, error } => {
                    self.state.phase = JobPhase::Failed;
                    self.state.error = Some(error.clone());
                    self.state.cancellable = false;
                    self.cancel = None;
                    self.progress = None;
                    events.push(JobEvent::Failed { operation_id, job, error });
                }
            }
        }
        events
    }

    fn begin(&mut self, job: &str, cancellable: bool) -> u64 {
        self.next_operation_id = self.next_operation_id.wrapping_add(1).max(1);
        let operation_id = self.next_operation_id;
        self.state = JobState {
            operation_id,
            job: Some(job.to_string()),
            phase: JobPhase::Initializing,
            error: None,
            cancellable,
        };
        self.progress = None;
        self.cancel = None;
        operation_id
    }

    fn fail_to_start(&mut self, operation_id: u64, job: &str, error: std::io::Error, tx: &Sender<JobEvent>) {
        if self.state.operation_id == operation_id {
            self.state.phase = JobPhase::Failed;
            self.state.error = Some(error.to_string());
            self.state.cancellable = false;
            self.cancel = None;
        }
        let _ = tx.send(JobEvent::Failed {
            operation_id,
            job: job.to_string(),
            error: error.to_string(),
        });
    }

    pub fn start_index(&mut self, db_path: PathBuf, library: Library) -> bool {
        if self.is_busy() {
            return false;
        }
        let job = crate::i18n::t("Indexing").to_string();
        let operation_id = self.begin(&job, true);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let tx = self.tx.clone();
        let fail_tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("daftar-index".into())
            .spawn(move || {
                let _ = tx.send(JobEvent::Started { operation_id });
                let result = (|| -> Result<ScanSummary, String> {
                    let store = Store::open(&db_path).map_err(|e| e.to_string())?;
                    scan_library(&store, &library, operation_id, Some(&tx), Some(&cancel))
                })();
                match result {
                    Ok(summary) => {
                        let _ = tx.send(JobEvent::IndexDone { operation_id, summary });
                    }
                    Err(error) => {
                        let _ = tx.send(JobEvent::Failed {
                            operation_id,
                            job,
                            error,
                        });
                    }
                }
            });
        match spawned {
            Ok(_) => true,
            Err(error) => {
                self.fail_to_start(operation_id, crate::i18n::t("Indexing"), error, &fail_tx);
                false
            }
        }
    }

    pub fn start_sync_paths(
        &mut self,
        db_path: PathBuf,
        library: Library,
        paths: Vec<PathBuf>,
    ) -> bool {
        if self.is_busy() || paths.is_empty() {
            return false;
        }
        let job = crate::i18n::t("Updating library").to_string();
        let operation_id = self.begin(&job, false);
        let tx = self.tx.clone();
        let fail_tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("daftar-sync".into())
            .spawn(move || {
                let _ = tx.send(JobEvent::Started { operation_id });
                match Store::open(&db_path) {
                    Ok(store) => {
                        let summary = sync_paths(&store, &library, &paths);
                        let _ = tx.send(JobEvent::IndexDone { operation_id, summary });
                    }
                    Err(error) => {
                        let _ = tx.send(JobEvent::Failed {
                            operation_id,
                            job,
                            error: error.to_string(),
                        });
                    }
                }
            });
        match spawned {
            Ok(_) => true,
            Err(error) => {
                self.fail_to_start(operation_id, crate::i18n::t("Updating library"), error, &fail_tx);
                false
            }
        }
    }

    pub fn start_export(
        &mut self,
        db_path: PathBuf,
        library: Library,
        output: PathBuf,
        include_settings: bool,
        settings_toml: Option<String>,
    ) -> bool {
        if self.is_busy() {
            return false;
        }
        let job = crate::i18n::t("Exporting library").to_string();
        let operation_id = self.begin(&job, true);
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Some(cancel.clone());
        let cancel_for_result = cancel.clone();
        let tx = self.tx.clone();
        let fail_tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("daftar-export".into())
            .spawn(move || {
                let _ = tx.send(JobEvent::Started { operation_id });
                let result = (|| -> Result<ExportReport, String> {
                    let store = Store::open(&db_path).map_err(|e| e.to_string())?;
                    let tx_progress = tx.clone();
                    let options = ExportOptions {
                        include_settings,
                        settings_toml,
                        cancel: Some(cancel),
                        progress: Some(Arc::new(move |progress| {
                            let _ = tx_progress.send(JobEvent::Progress {
                                operation_id,
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
                let cancelled = cancel_for_result.load(Ordering::Relaxed) && result.is_err();
                let _ = tx.send(JobEvent::ExportDone { operation_id, result, cancelled });
            });
        match spawned {
            Ok(_) => true,
            Err(error) => {
                self.fail_to_start(operation_id, crate::i18n::t("Exporting library"), error, &fail_tx);
                false
            }
        }
    }

    pub fn start_import(
        &mut self,
        db_path: PathBuf,
        archive: PathBuf,
        library_id: String,
        library_root: PathBuf,
        options: ImportOptions,
    ) -> bool {
        if self.is_busy() {
            return false;
        }
        let job = crate::i18n::t("Importing library").to_string();
        let operation_id = self.begin(&job, false);
        let tx = self.tx.clone();
        let fail_tx = self.tx.clone();
        let spawned = std::thread::Builder::new()
            .name("daftar-import".into())
            .spawn(move || {
                let _ = tx.send(JobEvent::Started { operation_id });
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
                let _ = tx.send(JobEvent::ImportDone { operation_id, result });
            });
        match spawned {
            Ok(_) => true,
            Err(error) => {
                self.fail_to_start(operation_id, crate::i18n::t("Importing library"), error, &fail_tx);
                false
            }
        }
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
    operation_id: u64,
    progress: Option<&Sender<JobEvent>>,
    cancel: Option<&AtomicBool>,
) -> Result<ScanSummary, String> {
    let mut summary = ScanSummary::default();
    let mut cancel_started = false;
    let root = Path::new(&library.root_path);
    if cancellation_requested(cancel, progress, operation_id, &mut cancel_started) {
        summary.cancelled = true;
        return Ok(summary);
    }
    if !root.is_dir() {
        return Err(crate::i18n::tf("Library folder is missing: {}", &[&root.display().to_string()]));
    }
    let docs = store.documents(&library.id).map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            !(entry.file_type().is_dir()
                && (matches!(name.as_ref(), ".git" | ".daftar" | "backups" | "node_modules" | "target")
                    || name.starts_with(".daftar-import")))
        })
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.file_type().is_file() && is_markdown(entry.path()) {
            paths.push(entry.into_path());
        }
    }
    paths.sort();
    let total = paths.len() as u64 + docs.len() as u64 + 1;
    summary.total = total;
    let mut last_progress_percent = None;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for path in &paths {
        if cancellation_requested(cancel, progress, operation_id, &mut cancel_started) {
            summary.cancelled = true;
            summary.warnings.push(crate::i18n::t("Indexing cancelled.").into());
            return Ok(summary);
        }
        let rel = path
            .strip_prefix(root)
            .map(native_to_rel)
            .unwrap_or_else(|_| native_to_rel(path));
        seen.insert(rel.clone());
        match index_file(store, library, path, &rel, doc_title_hint(path)) {
            Ok(IndexOutcome::Added) => summary.added += 1,
            Ok(IndexOutcome::Updated) => summary.updated += 1,
            Ok(IndexOutcome::Unchanged) => summary.unchanged += 1,
            Err(error) => {
                summary.skipped += 1;
                if summary.warnings.len() < 20 {
                    summary.warnings.push(format!("{rel}: {error}"));
                }
            }
        }
        summary.processed += 1;
        if let Some(tx) = progress {
            send_progress(
                tx,
                operation_id,
                summary.processed,
                total,
                crate::i18n::tf("Indexing {}", &[&rel]),
                &mut last_progress_percent,
            );
        }
    }
    if cancellation_requested(cancel, progress, operation_id, &mut cancel_started) {
        summary.cancelled = true;
        summary.warnings.push(crate::i18n::t("Indexing cancelled.").into());
        return Ok(summary);
    }

    let mut missing_ids = Vec::new();
    for doc in docs {
        if cancellation_requested(cancel, progress, operation_id, &mut cancel_started) {
            summary.cancelled = true;
            summary.warnings.push(crate::i18n::t("Indexing cancelled.").into());
            return Ok(summary);
        }
        if !seen.contains(&doc.rel_path) && !doc.is_missing {
            missing_ids.push(doc.id);
        }
        summary.processed += 1;
        if let Some(tx) = progress {
            send_progress(
                tx,
                operation_id,
                summary.processed,
                total,
                crate::i18n::t("Checking missing documents").into(),
                &mut last_progress_percent,
            );
        }
    }

    let missing_count = missing_ids.len() as u64;
    match store.set_documents_missing_cancellable(&missing_ids, true, || {
        cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
    }) {
        Ok(true) => summary.missing = missing_count,
        Ok(false) => {
            let _ = cancellation_requested(cancel, progress, operation_id, &mut cancel_started);
            summary.cancelled = true;
            summary.warnings.push(crate::i18n::t("Indexing cancelled.").into());
            return Ok(summary);
        }
        Err(error) => return Err(error.to_string()),
    }

    summary.processed = total;
    if let Some(tx) = progress {
        send_progress(
            tx,
            operation_id,
            total,
            total,
            crate::i18n::t("Indexing complete").into(),
            &mut last_progress_percent,
        );
    }
    Ok(summary)
}

fn cancellation_requested(
    cancel: Option<&AtomicBool>,
    progress: Option<&Sender<JobEvent>>,
    operation_id: u64,
    cancel_started: &mut bool,
) -> bool {
    let requested = cancel.is_some_and(|flag| flag.load(Ordering::Relaxed));
    if requested && !*cancel_started {
        if let Some(tx) = progress {
            let _ = tx.send(JobEvent::CancelStarted { operation_id });
        }
        *cancel_started = true;
    }
    requested
}

fn send_progress(
    tx: &Sender<JobEvent>,
    operation_id: u64,
    done: u64,
    total: u64,
    message: String,
    last_percent: &mut Option<u64>,
) {
    if total == 0 {
        return;
    }
    let percent = (u128::from(done.min(total)) * 100 / u128::from(total)) as u64;
    if percent < 100 && last_percent.is_some_and(|previous| percent <= previous) {
        return;
    }
    *last_percent = Some(percent);
    let _ = tx.send(JobEvent::Progress {
        operation_id,
        job: crate::i18n::t("Indexing").into(),
        done: done.min(total),
        total,
        message,
    });
}

/// Incrementally update a small set of filesystem paths.
pub fn sync_paths(store: &Store, library: &Library, paths: &[PathBuf]) -> ScanSummary {
    let mut summary = ScanSummary::default();
    let root = Path::new(&library.root_path);
    for path in paths {
        if !is_markdown(path) {
            continue;
        }
        summary.total += 1;
        let rel = match path.strip_prefix(root) {
            Ok(rel) => native_to_rel(rel),
            Err(_) => continue,
        };
        if path.exists() {
            match index_file(store, library, path, &rel, doc_title_hint(path)) {
                Ok(IndexOutcome::Added) => summary.added += 1,
                Ok(IndexOutcome::Updated) => summary.updated += 1,
                Ok(IndexOutcome::Unchanged) => summary.unchanged += 1,
                Err(error) => {
                    summary.skipped += 1;
                    summary.warnings.push(format!("{rel}: {error}"));
                }
            }
        } else {
            match store.document_by_rel_path(&library.id, &rel) {
                Ok(Some(doc)) => match store.set_document_missing(&doc.id, true) {
                    Ok(()) => summary.missing += 1,
                    Err(error) => {
                        summary.skipped += 1;
                        summary.warnings.push(format!("{rel}: {error}"));
                    }
                },
                Ok(None) => {}
                Err(error) => {
                    summary.skipped += 1;
                    summary.warnings.push(format!("{rel}: {error}"));
                }
            }
        }
        summary.processed += 1;
    }
    summary
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndexOutcome {
    Added,
    Updated,
    Unchanged,
}

/// Index one file and distinguish new, changed and unchanged documents.
fn index_file(
    store: &Store,
    library: &Library,
    path: &Path,
    rel: &str,
    fallback_title: String,
) -> Result<IndexOutcome, String> {
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
    let existing = store.document_by_rel_path(&library.id, rel).map_err(|error| error.to_string())?;
    if let Some(mut doc) = existing {
        let mut changed = doc.content_hash != hash
            || doc.file_mtime_ms != mtime
            || doc.file_size != bytes.len() as u64
            || doc.is_missing;
        if let Some(title) = &parsed.title {
            if !title.trim().is_empty() && (doc.title.trim().is_empty() || doc.title == fallback_title) && doc.title != *title {
                doc.title = title.clone();
                store.update_document_title(&doc.id, &doc.title).map_err(|error| error.to_string())?;
                changed = true;
            }
        }
        if changed {
            store.update_file_metadata(
                &doc.id,
                mtime,
                bytes.len() as u64,
                &hash,
                parsed.word_count as u64,
            ).map_err(|error| error.to_string())?;
        }
        if changed {
            store
                .index_document(&doc.id, &doc.title, &body, &headings, rel)
                .map_err(|e| e.to_string())?;
            store.set_document_missing(&doc.id, false).map_err(|e| e.to_string())?;
        }
        Ok(if changed { IndexOutcome::Updated } else { IndexOutcome::Unchanged })
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
        Ok(IndexOutcome::Added)
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

pub fn write_pasted_markdown(root: &Path, title: &str, source: &str) -> Result<PathBuf, String> {
    if source.trim().is_empty() {
        return Err(crate::i18n::t("Markdown text cannot be empty.").into());
    }
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;

    let inferred_title = source
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix('#').map(|heading| heading.trim_start_matches('#').trim())
        })
        .filter(|heading| !heading.is_empty())
        .unwrap_or("pasted-note");
    let safe_title = if title.trim().is_empty() { inferred_title } else { title.trim() };
    let mut stem = String::new();
    for ch in safe_title.chars() {
        if ch.is_alphanumeric() {
            stem.push(ch);
        } else if !stem.is_empty() && !stem.ends_with('-') {
            stem.push('-');
        }
    }
    while stem.ends_with('-') {
        stem.pop();
    }
    while stem.len() > 120 {
        stem.pop();
    }
    if stem.is_empty() {
        stem.push_str("pasted-note");
    }

    for suffix in 1u64.. {
        let filename = if suffix == 1 {
            format!("{stem}.md")
        } else {
            format!("{stem}-{suffix}.md")
        };
        let path = root.join(filename);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(source.as_bytes()).and_then(|()| file.flush()) {
                    let _ = std::fs::remove_file(&path);
                    return Err(error.to_string());
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    unreachable!("the filename suffix range is practically inexhaustible")
}

// ----------------------------------------------------------------------------------------
// File watching
// ----------------------------------------------------------------------------------------

pub struct LibraryWatcher {
    _watcher: notify::RecommendedWatcher,
    rx: Receiver<WatchChange>,
}

enum WatchChange {
    Markdown(PathBuf),
    Rescan,
}

#[derive(Debug, Default)]
pub struct WatchBatch {
    pub paths: Vec<PathBuf>,
    pub full_rescan: bool,
}

fn classify_watch_change(root: &Path, kind: notify::EventKind, path: &Path) -> Option<WatchChange> {
    use notify::event::{CreateKind, ModifyKind, RemoveKind};
    use notify::EventKind;

    if ignored_watch_path(root, path) {
        return None;
    }
    let full_scan = matches!(
        kind,
        EventKind::Create(CreateKind::Folder)
            | EventKind::Remove(RemoveKind::Folder)
            | EventKind::Modify(ModifyKind::Name(_))
    );
    let relevant = matches!(
        kind,
        EventKind::Any
            | EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Any | ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
    );
    if !relevant {
        return None;
    }
    if is_markdown(path) {
        Some(WatchChange::Markdown(path.to_path_buf()))
    } else if full_scan || path.is_dir() {
        Some(WatchChange::Rescan)
    } else {
        None
    }
}

impl LibraryWatcher {
    pub fn start(root: &Path) -> Result<Self, String> {
        use notify::Watcher;

        let root = root.to_path_buf();
        let callback_root = root.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            for path in event.paths {
                if let Some(change) = classify_watch_change(&callback_root, event.kind, &path) {
                    let _ = tx.send(change);
                }
            }
        })
        .map_err(|e| crate::i18n::tf("Could not start file watcher: {}", &[&e.to_string()]))?;
        watcher
            .watch(&root, notify::RecursiveMode::Recursive)
            .map_err(|e| crate::i18n::tf("Could not watch library folder: {}", &[&e.to_string()]))?;
        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    /// Non-blocking drain with de-duplication, cap 200 paths per frame.
    pub fn drain(&self) -> WatchBatch {
        let mut paths = std::collections::BTreeSet::new();
        let mut full_rescan = false;
        for _ in 0..200 {
            match self.rx.try_recv() {
                Ok(WatchChange::Markdown(path)) => {
                    paths.insert(path);
                }
                Ok(WatchChange::Rescan) => full_rescan = true,
                Err(_) => break,
            }
        }
        WatchBatch { paths: paths.into_iter().collect(), full_rescan }
    }
}

fn ignored_watch_path(root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        matches!(name.as_ref(), ".git" | ".daftar" | "backups" | "node_modules" | "target")
            || name.starts_with(".daftar-import")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use daftar_core::model::Library;
    use notify::event::{AccessKind, DataChange};
    use notify::EventKind;
    use tempfile::tempdir;

    fn test_library(root: &Path) -> (Store, Library) {
        std::fs::create_dir_all(root).unwrap();
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("Test", root.to_string_lossy());
        store.upsert_library(&library).unwrap();
        (store, library)
    }

    #[test]
    fn scan_progress_is_monotonic_and_counts_unchanged_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("library");
        let (store, library) = test_library(&root);
        for i in 0..4 {
            std::fs::write(root.join(format!("{i}.md")), format!("# Document {i}\nBody")).unwrap();
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let first = scan_library(&store, &library, 11, Some(&tx), None).unwrap();
        let updates: Vec<_> = rx.try_iter().collect();
        let fractions: Vec<_> = updates
            .iter()
            .filter_map(|event| match event {
                JobEvent::Progress { done, total, .. } => Some(*done as f32 / *total as f32),
                _ => None,
            })
            .collect();
        assert_eq!(first.added, 4);
        assert_eq!(first.updated, 0);
        assert_eq!(first.processed, first.total);
        assert!(fractions.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(fractions.last(), Some(&1.0));

        let second = scan_library(&store, &library, 12, None, None).unwrap();
        assert_eq!(second.added, 0);
        assert_eq!(second.updated, 0);
        assert_eq!(second.unchanged, 4);
        assert_eq!(second.processed, second.total);
    }

    #[test]
    fn empty_library_completes_without_dividing_by_zero() {
        let dir = tempdir().unwrap();
        let (store, library) = test_library(&dir.path().join("empty"));
        let (tx, rx) = std::sync::mpsc::channel();

        let summary = scan_library(&store, &library, 2, Some(&tx), None).unwrap();
        let progress: Vec<_> = rx.try_iter().filter_map(|event| match event {
            JobEvent::Progress { done, total, .. } => Some((done, total)),
            _ => None,
        }).collect();
        assert_eq!(summary.added, 0);
        assert_eq!(summary.total, 1);
        assert_eq!(progress, vec![(1, 1)]);
    }

    #[test]
    fn cancellation_does_not_mark_unvisited_files_missing() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("library");
        let (store, library) = test_library(&root);
        let file = root.join("existing.md");
        std::fs::write(&file, "# Existing\nBody").unwrap();
        scan_library(&store, &library, 1, None, None).unwrap();
        let doc = store.document_by_rel_path(&library.id, "existing.md").unwrap().unwrap();
        std::fs::remove_file(file).unwrap();

        let cancel = AtomicBool::new(true);
        let summary = scan_library(&store, &library, 2, None, Some(&cancel)).unwrap();
        let current = store.document_by_rel_path(&library.id, "existing.md").unwrap().unwrap();
        assert!(summary.cancelled);
        assert!(!current.is_missing);
        assert_eq!(current.id, doc.id);

        let checks = std::sync::atomic::AtomicUsize::new(0);
        let committed = store
            .set_documents_missing_cancellable(std::slice::from_ref(&doc.id), true, || {
                checks.fetch_add(1, Ordering::Relaxed) == 1
            })
            .unwrap();
        assert!(!committed);
        assert!(!store.document_by_rel_path(&library.id, "existing.md").unwrap().unwrap().is_missing);
    }

    #[test]
    fn job_progress_is_coalesced_monotonic_and_scoped_to_operation() {
        let mut jobs = Jobs::new();
        let operation_id = jobs.begin("Indexing", true);
        jobs.cancel = Some(Arc::new(AtomicBool::new(false)));
        jobs.tx.send(JobEvent::Started { operation_id }).unwrap();
        jobs.tx.send(JobEvent::Progress {
            operation_id, job: "Indexing".into(), done: 5, total: 10, message: "5".into(),
        }).unwrap();
        assert_eq!(jobs.poll().len(), 2);
        assert_eq!(jobs.progress.as_ref().unwrap().fraction(), Some(0.5));
        assert!(jobs.is_busy());

        jobs.tx.send(JobEvent::Progress {
            operation_id, job: "Indexing".into(), done: 5, total: 10, message: "duplicate".into(),
        }).unwrap();
        jobs.tx.send(JobEvent::Progress {
            operation_id, job: "Indexing".into(), done: 3, total: 10, message: "stale".into(),
        }).unwrap();
        assert!(jobs.poll().is_empty());
        assert_eq!(jobs.progress.as_ref().unwrap().done, 5);

        jobs.tx.send(JobEvent::Progress {
            operation_id, job: "Indexing".into(), done: 99, total: 10, message: "last item".into(),
        }).unwrap();
        jobs.poll();
        assert_eq!(jobs.progress.as_ref().unwrap().done, 9);
        assert_eq!(jobs.progress.as_ref().unwrap().fraction(), Some(0.9));
        assert!(jobs.is_busy());

        jobs.tx.send(JobEvent::IndexDone {
            operation_id, summary: ScanSummary { processed: 10, total: 10, ..ScanSummary::default() },
        }).unwrap();
        jobs.poll();
        assert_eq!(jobs.state.phase, JobPhase::Completed);
        assert_eq!(jobs.progress.as_ref().unwrap().fraction(), Some(1.0));

        let next_id = jobs.begin("Next", false);
        jobs.tx.send(JobEvent::Progress {
            operation_id, job: "Indexing".into(), done: 8, total: 10, message: "old".into(),
        }).unwrap();
        assert!(jobs.poll().is_empty());
        assert_eq!(jobs.state.operation_id, next_id);
        assert!(jobs.progress.is_none());
    }

    #[test]
    fn cancellation_state_is_explicit_and_sync_jobs_are_not_cancellable() {
        let mut jobs = Jobs::new();
        let operation_id = jobs.begin("Indexing", true);
        let cancel = Arc::new(AtomicBool::new(false));
        jobs.cancel = Some(cancel.clone());
        jobs.tx.send(JobEvent::Started { operation_id }).unwrap();
        jobs.poll();
        assert!(jobs.can_cancel());
        assert!(!jobs.cancel_requested());

        jobs.cancel();
        assert!(cancel.load(Ordering::Relaxed));
        assert_eq!(jobs.state.phase, JobPhase::CancelRequested);
        assert!(!jobs.can_cancel());
        assert!(jobs.cancel_requested());
        jobs.tx.send(JobEvent::CancelStarted { operation_id }).unwrap();
        jobs.poll();
        assert_eq!(jobs.state.phase, JobPhase::Cancelling);

        jobs.tx.send(JobEvent::IndexDone {
            operation_id,
            summary: ScanSummary { cancelled: true, ..ScanSummary::default() },
        }).unwrap();
        jobs.poll();
        assert_eq!(jobs.state.phase, JobPhase::Cancelled);
        assert!(!jobs.is_busy());
        assert!(jobs.progress.is_none());

        jobs.begin("Updating library", false);
        assert!(jobs.is_busy());
        assert!(!jobs.can_cancel());
    }

    #[test]
    fn failed_jobs_do_not_appear_completed() {
        let mut jobs = Jobs::new();
        let operation_id = jobs.begin("Indexing", true);
        jobs.tx.send(JobEvent::Failed {
            operation_id, job: "Indexing".into(), error: "store unavailable".into(),
        }).unwrap();
        jobs.poll();
        assert_eq!(jobs.state.phase, JobPhase::Failed);
        assert_eq!(jobs.state.error.as_deref(), Some("store unavailable"));
        assert!(!jobs.is_busy());
        assert!(jobs.progress.is_none());
    }

    #[test]
    fn watcher_ignores_access_and_non_markdown_file_events() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        assert!(classify_watch_change(
            root,
            EventKind::Access(AccessKind::Read),
            &root.join("doc.md"),
        ).is_none());
        assert!(classify_watch_change(
            root,
            EventKind::Create(notify::event::CreateKind::File),
            &root.join("daftar.db-wal"),
        ).is_none());
        assert!(matches!(
            classify_watch_change(
                root,
                EventKind::Modify(notify::event::ModifyKind::Data(DataChange::Content)),
                &root.join("doc.md"),
            ),
            Some(WatchChange::Markdown(_))
        ));
        assert!(matches!(
            classify_watch_change(
                root,
                EventKind::Remove(notify::event::RemoveKind::Folder),
                &root.join("old-folder"),
            ),
            Some(WatchChange::Rescan)
        ));
    }

    #[test]
    fn pasted_markdown_uses_safe_unique_filenames_and_preserves_content() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("library");
        let (store, library) = test_library(&root);
        let markdown = "# Persian title\n\n**Original Markdown**\n";
        let first = write_pasted_markdown(&root, "../notes/field notes", markdown).unwrap();
        let second = write_pasted_markdown(&root, "../notes/field notes", markdown).unwrap();

        assert_eq!(first.parent(), Some(root.as_path()));
        assert_eq!(first.file_name().unwrap(), "notes-field-notes.md");
        assert_eq!(second.file_name().unwrap(), "notes-field-notes-2.md");
        assert_eq!(std::fs::read_to_string(first).unwrap(), markdown);
        let summary = sync_paths(&store, &library, std::slice::from_ref(&second));
        assert_eq!(summary.added, 1);
        assert_eq!(store.documents(&library.id).unwrap().len(), 1);
        assert!(write_pasted_markdown(&root, "Empty", " \n ").is_err());
    }
}
