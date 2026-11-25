//! Application shell, state orchestration, navigation and dialogs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use daftar_core::archive::ImportOptions;
use daftar_core::model::{
    Bookmark, Category, Collection, Document, Flashcard, Id, Library, LibraryStats, Roadmap,
    RoadmapNode, SearchHit, Tag,
};
use daftar_core::settings::{AppSettings, ThemeMode};
use daftar_core::util::{now_ms, safe_file_stem};
use daftar_export::{CardContent, CardStyle};
use daftar_store::{SearchFilters, Store};
use daftar_text::TextEngine;
use eframe::CreationContext;
use egui::{
    Align2, Color32, Id as EguiId, Key, KeyboardShortcut, Modifiers, Order, RichText, Sense, Stroke,
    Ui, Vec2, vec2,
};

use crate::code::CodeHighlighter;
use crate::icons::{Icon, app_logo, draw_icon};
use crate::picker::{FilePicker, PickerMode, PickerResult};
use crate::reader::view::{ReaderAction, ReaderState};
use crate::reader::layout::ReaderConfig;
use crate::tasks::{JobEvent, Jobs, LibraryWatcher, is_markdown};
use crate::theme::{self, Palette, ToastKind};
use crate::views::flashcards::{FlashcardsState, StudyState};
use crate::views::highlights::HighlightsState;
use crate::views::library::{LibraryState, NavFilter};
use crate::views::roadmaps::RoadmapsState;
use crate::views::search::SearchState;
use crate::views::settings::{SettingsState, SettingsTab};
use crate::widgets::{self, ButtonVariant};
use crate::rtl::DirUi;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub library_root: PathBuf,
    pub settings_path: PathBuf,
    pub db_path: PathBuf,
    pub fonts_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub portable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Search,
    Highlights,
    Flashcards,
    Study,
    Roadmaps,
    Settings,
    Reader,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavSection {
    Library,
    Collections,
    Categories,
    Roadmaps,
    Bookmarks,
    Flashcards,
    Recent,
    Settings,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub kind: ToastKind,
    pub created: i64,
}

pub(crate) struct ConfirmDialog {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) confirm_label: String,
    pub(crate) action: ConfirmAction,
}

pub(crate) enum ConfirmAction {
    DeleteDocument(String),
    DeleteCollection(String),
    DeleteCategory(String),
    DeleteHighlight(String),
    DeleteFlashcard(String),
    DeleteRoadmap(String),
    DeleteRoadmapNode(String),
    RemoveTag {
        document_id: String,
        tag: String,
    },
}

pub(crate) struct TextPrompt {
    pub(crate) title: String,
    pub(crate) label: String,
    pub(crate) value: String,
    pub(crate) action: PromptAction,
}

pub(crate) enum PromptAction {
    NewCollection,
    NewCategory,
    RenameDocument(String),
    NewTag(String),
    NewRoadmap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerPurpose {
    ImportMarkdown,
    AddFolder,
    AddLibrary,
    ExportLibrary,
    ExportQuoteImage,
    ExportDiagram,
    ExportBatchZip,
    ExportBatchPdf,
    ExportBatchFolder,
    ImportLibraryArchive,
    ImportFont,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BatchFormat {
    PngZip,
    Pdf,
    PngFolder,
}

pub struct ImportDialogState {
    pub archive: PathBuf,
    pub preview: daftar_export::ImportPreview,
    pub strategy: daftar_core::archive::DuplicateStrategy,
    pub restore_settings: bool,
    pub restore_cards: bool,
    pub restore_roadmaps: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PasteMarkdownDialog {
    pub title: String,
    pub markdown: String,
    pub error: Option<String>,
}

pub struct FlashcardEditor {
    pub card: Flashcard,
    pub is_new: bool,
    pub source_title: Option<String>,
    pub error: Option<String>,
}

pub struct DaftarApp {
    pub config: AppConfig,
    pub store: Store,
    pub settings: AppSettings,
    pub settings_dirty: bool,
    pub(crate) last_settings_save: i64,
    pub engine: TextEngine,
    pub code: CodeHighlighter,
    pub libraries: Vec<Library>,
    pub library: Option<Library>,
    pub docs: Vec<Document>,
    pub doc_tags: HashMap<Id, Vec<String>>,
    pub collections: Vec<Collection>,
    pub categories: Vec<Category>,
    pub tags: Vec<Tag>,
    pub stats: LibraryStats,
    pub recents: Vec<Document>,
    pub view: View,
    pub nav: NavSection,
    pub reader: Option<ReaderState>,
    pub library_state: LibraryState,
    pub search: SearchState,
    pub highlights: HighlightsState,
    pub flashcards: FlashcardsState,
    pub study: Option<StudyState>,
    pub roadmaps: RoadmapsState,
    pub settings_state: SettingsState,
    pub(crate) picker: Option<FilePicker>,
    pub(crate) picker_purpose: Option<PickerPurpose>,
    pub jobs: Jobs,
    pub(crate) watcher: Option<LibraryWatcher>,
    pub(crate) pending_paths: Vec<PathBuf>,
    pub(crate) pending_full_scan: bool,
    pub(crate) last_fs_event: i64,
    pub(crate) toasts: Vec<Toast>,
    pub(crate) toast_seq: u64,
    pub(crate) confirm: Option<ConfirmDialog>,
    pub(crate) prompt: Option<TextPrompt>,
    pub import_dialog: Option<ImportDialogState>,
    pub(crate) paste_markdown: Option<PasteMarkdownDialog>,
    pub flashcard_editor: Option<FlashcardEditor>,
    pub(crate) dark: bool,
    pub(crate) logo: Option<egui::TextureHandle>,
    pub(crate) show_left: bool,
    pub(crate) show_right: bool,
    pub(crate) search_focus: bool,
    pub(crate) status: String,
    pub(crate) last_central_size: Vec2,
    pub(crate) pending_quote_export: Option<(String, String)>,
    pub(crate) pending_diagram_export: Option<(u64, String)>,
    pub(crate) pending_batch: Option<(Vec<daftar_export::CardContent>, BatchFormat)>,
    pub(crate) last_mouse_activity: i64,
}

impl DaftarApp {
    pub fn new(cc: &CreationContext<'_>, config: AppConfig) -> anyhow::Result<Self> {
        let ctx = &cc.egui_ctx;
        theme::install_fonts(ctx);
        let mut settings = load_settings(&config.settings_path);
        if settings.general.default_library_root.trim().is_empty() {
            settings.general.default_library_root = config.library_root.to_string_lossy().to_string();
        }
        settings.general.portable_mode = config.portable;
        crate::i18n::set_language(&settings.general.language);
        settings.validate();
        let dark = match settings.general.theme {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => ctx.system_theme().map(|t| t == egui::Theme::Dark).unwrap_or(false),
        };
        theme::apply(ctx, dark);

        let store = Store::open(&config.db_path)?;
        let mut libraries = store.libraries()?;
        let root = PathBuf::from(&settings.general.default_library_root);
        if !root.exists() {
            std::fs::create_dir_all(&root)?;
        }
        if libraries.is_empty() {
            let mut library = Library::new(library_display_name(&root), root.to_string_lossy());
            library.updated_at = now_ms();
            store.upsert_library(&library)?;
            libraries.push(library);
        }
        let library = libraries
            .iter()
            .find(|l| Path::new(&l.root_path) == root)
            .cloned()
            .unwrap_or_else(|| libraries[0].clone());
        settings.general.default_library_root = library.root_path.clone();

        let mut engine = TextEngine::new(Some(config.fonts_dir.clone()), "en-US");
        if !engine.has_family(&settings.reading.typography.font_family) {
            settings.reading.typography.font_family = engine.default_reading_family();
        }
        if !engine.has_family(&settings.reading.typography.code_font_family) {
            settings.reading.typography.code_font_family = engine.default_code_family();
        }
        let code = CodeHighlighter::new();

        let show_left = settings.window.left_panel_open;
        let show_right = settings.window.right_panel_open;
        let startup = startup_view(&settings.general.startup_view);
        let mut app = Self {
            config,
            store,
            settings,
            settings_dirty: false,
            last_settings_save: now_ms(),
            engine,
            code,
            libraries,
            library: Some(library),
            docs: Vec::new(),
            doc_tags: HashMap::new(),
            collections: Vec::new(),
            categories: Vec::new(),
            tags: Vec::new(),
            stats: LibraryStats::default(),
            recents: Vec::new(),
            view: View::Library,
            nav: NavSection::Library,
            reader: None,
            library_state: LibraryState::default(),
            search: SearchState::default(),
            highlights: HighlightsState::default(),
            flashcards: FlashcardsState::default(),
            study: None,
            roadmaps: RoadmapsState::default(),
            settings_state: SettingsState {
                tab: SettingsTab::General,
                ..Default::default()
            },
            picker: None,
            picker_purpose: None,
            jobs: Jobs::new(),
            watcher: None,
            pending_paths: Vec::new(),
            pending_full_scan: false,
            last_fs_event: 0,
            toasts: Vec::new(),
            toast_seq: 1,
            confirm: None,
            prompt: None,
            import_dialog: None,
            paste_markdown: None,
            flashcard_editor: None,
            dark,
            logo: app_logo(ctx),
            show_left,
            show_right,
            search_focus: false,
            status: crate::i18n::t("Ready").to_string(),
            last_central_size: vec2(1000.0, 800.0),
            pending_quote_export: None,
            pending_diagram_export: None,
            pending_batch: None,
            last_mouse_activity: 0,
        };
        app.reload();
        app.view = startup;
        app.nav = match app.view {
            View::Library => NavSection::Library,
            View::Roadmaps => NavSection::Roadmaps,
            View::Flashcards | View::Study => NavSection::Flashcards,
            View::Highlights => NavSection::Bookmarks,
            View::Settings => NavSection::Settings,
            _ => NavSection::Library,
        };
        if app.settings.library.backup_on_startup {
            app.backup_now(true);
        }
        if app.settings.library.auto_index {
            if let Some(library) = &app.library {
                let _ = app
                    .jobs
                    .start_index(app.config.db_path.clone(), library.clone());
            }
        }
        app.start_watcher();
        Ok(app)
    }

    pub(crate) fn toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toasts.push(Toast {
            id: self.toast_seq,
            message: message.into(),
            kind,
            created: now_ms(),
        });
        self.toast_seq += 1;
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.settings_dirty = true;
    }

    pub fn reload(&mut self) {
        let Some(library) = self.library.clone() else {
            return;
        };
        self.docs = self.store.documents(&library.id).unwrap_or_default();
        self.doc_tags = self.store.all_document_tags(&library.id).unwrap_or_default();
        self.collections = self.store.collections(&library.id).unwrap_or_default();
        self.categories = self.store.categories(&library.id).unwrap_or_default();
        self.tags = self.store.tags(&library.id).unwrap_or_default();
        self.stats = self.store.stats(&library.id).unwrap_or_default();
        self.recents = self
            .store
            .recent_documents(&library.id, self.settings.library.recents_limit)
            .unwrap_or_default();
        self.library_state.sync(&self.docs, &self.collections, &self.categories);
    }

    pub(crate) fn switch_library(&mut self, id: &str) {
        if self.library.as_ref().map(|l| l.id.as_str()) == Some(id) {
            return;
        }
        self.close_reader();
        if let Ok(Some(library)) = self.store.library(id) {
            self.settings.general.default_library_root = library.root_path.clone();
            self.library = Some(library);
            self.mark_dirty();
            self.reload();
            self.start_watcher();
            if self.settings.library.auto_index {
                if let Some(library) = &self.library {
                    let _ = self.jobs.start_index(self.config.db_path.clone(), library.clone());
                }
            }
        }
    }

    pub(crate) fn start_watcher(&mut self) {
        self.watcher = None;
        if !self.settings.library.watch_files {
            return;
        }
        let Some(library) = &self.library else { return };
        match LibraryWatcher::start(Path::new(&library.root_path)) {
            Ok(watcher) => self.watcher = Some(watcher),
            Err(error) => self.toast(error, ToastKind::Warning),
        }
    }

    fn close_reader(&mut self) {
        if let Some(mut reader) = self.reader.take() {
            reader.finish_session(&self.store);
        }
    }

    pub(crate) fn open_document(&mut self, id: &str) {
        self.close_reader();
        let doc = self
            .docs
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .or_else(|| self.store.document(id).ok().flatten());
        let Some(doc) = doc else {
            self.toast(crate::i18n::t("Document not found."), ToastKind::Error);
            return;
        };
        let Some(library) = &self.library else {
            return;
        };
        let width = (self.last_central_size.x - 80.0).max(480.0);
        let height = self.last_central_size.y.max(300.0);
        match ReaderState::open(
            &self.store,
            doc.clone(),
            &self.settings,
            &mut self.engine,
            &self.code,
            &Palette::new(self.dark),
            Path::new(&library.root_path),
            width,
            height,
        ) {
            Ok(reader) => {
                self.show_right = self.settings.window.right_panel_open;
                self.reader = Some(reader);
                self.view = View::Reader;
                self.nav = NavSection::Library;
                self.reload();
            }
            Err(error) => {
                self.toast(error, ToastKind::Error);
                self.reload();
            }
        }
    }

    pub(crate) fn navigate(&mut self, view: View, nav: NavSection) {
        if self.view == View::Reader && view != View::Reader {
            self.close_reader();
        }
        self.view = view;
        self.nav = nav;
        match view {
            View::Search => self.search.run(&self.store, self.library.as_ref()),
            View::Highlights => self.highlights.reload(&self.store),
            View::Flashcards => self.flashcards.reload(&self.store, &self.categories, &self.tags),
            View::Roadmaps => self.roadmaps.reload(&self.store, self.library.as_ref()),
            _ => {}
        }
    }

    pub(crate) fn backup_now(&mut self, silent: bool) {
        match self.store.backup_to(&self.config.backups_dir) {
            Ok(path) => {
                let _ = Store::prune_backups(
                    &self.config.backups_dir,
                    self.settings.library.backups_to_keep,
                );
                if !silent {
                    self.toast(
                        format!("Backup written to {}", path.display()),
                        ToastKind::Success,
                    );
                }
            }
            Err(error) => {
                if !silent {
                    self.toast(crate::i18n::tf("Backup failed: {}", &[&error.to_string()]), ToastKind::Error);
                } else {
                    log::warn!("startup backup failed: {error}");
                }
            }
        }
    }

    pub(crate) fn save_settings(&mut self, force: bool) {
        if !self.settings_dirty && !force {
            return;
        }
        if !force && now_ms() - self.last_settings_save < 1200 {
            return;
        }
        self.settings.updated_at = now_ms();
        match self.settings.to_toml() {
            Ok(text) => {
                if let Some(parent) = self.config.settings_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let temp = self.config.settings_path.with_extension("toml.tmp");
                if std::fs::write(&temp, text).is_ok() {
                    if std::fs::rename(&temp, &self.config.settings_path).is_err() {
                        let _ = std::fs::remove_file(&self.config.settings_path);
                        let _ = std::fs::rename(&temp, &self.config.settings_path);
                    }
                    self.settings_dirty = false;
                    self.last_settings_save = now_ms();
                }
            }
            Err(error) => self.status = crate::i18n::tf("Could not serialise settings: {}", &[&error.to_string()]),
        }
    }

    pub(crate) fn resolve_dark(&mut self, ctx: &egui::Context) {
        let dark = match self.settings.general.theme {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => ctx
                .system_theme()
                .map(|t| t == egui::Theme::Dark)
                .unwrap_or(self.dark),
        };
        if dark != self.dark {
            self.dark = dark;
            theme::apply(ctx, dark);
            if let Some(reader) = &mut self.reader {
                reader.diagrams.clear();
            }
        }
    }

    pub(crate) fn toggle_theme(&mut self, ctx: &egui::Context) {
        self.dark = !self.dark;
        self.settings.general.theme = if self.dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        };
        theme::apply(ctx, self.dark);
        if let Some(reader) = &mut self.reader {
            reader.diagrams.clear();
        }
        self.mark_dirty();
    }

    pub(crate) fn poll_jobs(&mut self, ctx: &egui::Context) {
        for event in self.jobs.poll() {
            match event {
                JobEvent::Started { .. } => {}
                JobEvent::CancelStarted { .. } => {
                    self.status = crate::i18n::t("Canceling…").into();
                }
                JobEvent::Progress { done, total, message, .. } => {
                    self.status = if total > 0 {
                        crate::i18n::tf("{}  ({}/{})", &[&message.to_string(), &done.to_string(), &total.to_string()])
                    } else {
                        message
                    };
                }
                JobEvent::IndexDone { summary, .. } => {
                    self.reload();
                    self.status = if summary.cancelled {
                        crate::i18n::t("Indexing cancelled.").into()
                    } else {
                        crate::i18n::tf(
                            "Library indexed: {} added, {} updated, {} unchanged, {} missing",
                            &[
                                &summary.added.to_string(),
                                &summary.updated.to_string(),
                                &summary.unchanged.to_string(),
                                &summary.missing.to_string(),
                            ]
                        )
                    };
                    if !summary.warnings.is_empty() {
                        self.toast(
                            format!("Indexing finished with {} warning(s).", summary.warnings.len()),
                            ToastKind::Warning,
                        );
                        for warning in summary.warnings.iter().take(3) {
                            log::warn!("index warning: {warning}");
                        }
                    }
                    if let Some(reader) = &mut self.reader {
                        // Reload annotations for external edits if the current doc changed.
                        if let Some(doc) = self.store.document(&reader.doc.id).ok().flatten() {
                            if doc.content_hash != reader.content_hash {
                                let id = doc.id.clone();
                                let was_reader = true;
                                if was_reader {
                                    self.close_reader();
                                    self.open_document(&id);
                                }
                            }
                        }
                    }
                }
                JobEvent::ExportDone { result, cancelled, .. } => if cancelled {
                    self.status = crate::i18n::t("Export cancelled.").into();
                } else { match result {
                    Ok(report) => {
                        self.status =
                            crate::i18n::tf("Export complete: {}", &[&report.path.display().to_string()]);
                        self.toast(
                            crate::i18n::tf(
                                "Library exported ({} documents, {} attachments).",
                                &[&report.documents.to_string(), &report.attachments.to_string()],
                            ),
                            ToastKind::Success,
                        );
                    }
                    Err(error) => {
                        self.status = crate::i18n::t("Export failed").into();
                        self.toast(crate::i18n::tf("Export failed: {}", &[&error.to_string()]), ToastKind::Error);
                    }
                }},
                JobEvent::ImportDone { result, .. } => match result {
                    Ok(report) => {
                        self.status = crate::i18n::t("Import complete").into();
                        let summary = &report.summary;
                        self.toast(
                            crate::i18n::tf(
                        "Imported {} documents, {} highlights, {} flashcards, {} roadmaps.",
                                &[
                                    &summary.documents_imported.to_string(),
                                    &summary.highlights.to_string(),
                                    &summary.flashcards.to_string(),
                                    &summary.roadmaps.to_string(),
                                ]
                            ),
                            ToastKind::Success,
                        );
                        for warning in report.warnings.iter().take(5) {
                            self.toast(warning.clone(), ToastKind::Warning);
                        }
                        if let Some(text) = report.settings_toml {
                            if let Ok(mut imported) = AppSettings::from_toml(&text) {
                                imported.general.default_library_root =
                                    self.settings.general.default_library_root.clone();
                                imported.general.portable_mode = self.settings.general.portable_mode;
                                imported.validate();
                                self.settings = imported;
                                self.mark_dirty();
                                theme::apply(ctx, self.dark);
                            }
                        }
                        self.import_dialog = None;
                        self.reload();
                    }
                    Err(error) => {
                        self.toast(crate::i18n::tf("Import failed: {}", &[&error.to_string()]), ToastKind::Error);
                        self.status = crate::i18n::t("Import failed").into();
                    }
                },
                JobEvent::Failed { job, error, .. } => {
                    self.toast(crate::i18n::tf("{} failed: {}", &[&job.to_string(), &error.to_string()]), ToastKind::Error);
                    self.status = crate::i18n::tf("{} failed", &[&job.to_string()]);
                }
            }
        }
    }

    fn poll_watcher(&mut self, ctx: &egui::Context) {
        let batch = self.watcher.as_ref().map(LibraryWatcher::drain).unwrap_or_default();
        if !batch.paths.is_empty() || batch.full_rescan {
            self.last_fs_event = now_ms();
            self.pending_paths.extend(batch.paths);
            self.pending_paths.sort();
            self.pending_paths.dedup();
            self.pending_full_scan |= batch.full_rescan;
        }
        if self.pending_paths.is_empty() && !self.pending_full_scan {
            return;
        }
        let elapsed = now_ms() - self.last_fs_event;
        if elapsed < 850 {
            ctx.request_repaint_after(Duration::from_millis((850 - elapsed).max(1) as u64));
            return;
        }
        if self.jobs.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
            return;
        }
        let paths = std::mem::take(&mut self.pending_paths);
        let full_rescan = std::mem::take(&mut self.pending_full_scan);
        let Some(library) = &self.library else { return };
        if !full_rescan && !paths.is_empty() && paths.len() <= 80 {
            let _ = self.jobs.start_sync_paths(
                self.config.db_path.clone(),
                library.clone(),
                paths,
            );
        } else {
            let _ = self.jobs.start_index(self.config.db_path.clone(), library.clone());
        }
    }

    fn handle_shortcuts(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            if self.picker.is_some() {
                self.picker = None;
                self.picker_purpose = None;
            } else if self.paste_markdown.is_some() {
                self.paste_markdown = None;
            } else if self.import_dialog.is_some() {
                self.import_dialog = None;
            } else if self.prompt.is_some() {
                self.prompt = None;
            } else if let Some(reader) = &mut self.reader {
                reader.selection = None;
            }
        }
        if self.picker.is_some()
            || self.paste_markdown.is_some()
            || self.import_dialog.is_some()
            || self.prompt.is_some()
        {
            return;
        }
        if shortcut(ui, self.settings.shortcut("search"))
            || shortcut(ui, self.settings.shortcut("command_palette"))
        {
            self.search_focus = true;
            if self.view != View::Reader {
                self.navigate(View::Search, NavSection::Library);
            }
        }
        if shortcut(ui, self.settings.shortcut("toggle_left")) {
            self.show_left = !self.show_left;
            self.settings.window.left_panel_open = self.show_left;
            self.mark_dirty();
        }
        if shortcut(ui, self.settings.shortcut("toggle_right")) {
            self.show_right = !self.show_right;
            self.settings.window.right_panel_open = self.show_right;
            self.mark_dirty();
        }
        if shortcut(ui, self.settings.shortcut("theme")) {
            self.toggle_theme(&ctx);
        }
        if shortcut(ui, self.settings.shortcut("open_file")) {
            self.open_picker(
                PickerPurpose::ImportMarkdown,
                PickerMode::OpenFiles,
                crate::i18n::t("Import Markdown files"),
                &["md", "markdown", "mdown"],
                "",
            );
        }
        if shortcut(ui, self.settings.shortcut("import_folder")) {
            self.open_picker(
                PickerPurpose::AddFolder,
                PickerMode::OpenFolder,
                crate::i18n::t("Add a folder to the library"),
                &[],
                "",
            );
        }
        if shortcut(ui, self.settings.shortcut("font_increase")) {
            self.adjust_font_size(1.0);
        }
        if shortcut(ui, self.settings.shortcut("font_decrease")) {
            self.adjust_font_size(-1.0);
        }
        if shortcut(ui, self.settings.shortcut("direction")) {
            self.cycle_direction();
        }
    }

    fn adjust_font_size(&mut self, delta: f32) {
        self.settings.reading.typography.font_size =
            (self.settings.reading.typography.font_size + delta).clamp(11.0, 34.0);
        self.mark_dirty();
        if let Some(reader) = &mut self.reader {
            reader.update_layout(
                &mut self.engine,
                &self.code,
                &Palette::new(self.dark),
                &self.settings,
                self.last_central_size.x - 60.0,
            );
        }
    }

    fn cycle_direction(&mut self) {
        let Some(reader) = &mut self.reader else {
            self.settings.reading.default_direction = self.settings.reading.default_direction.toggled();
            self.mark_dirty();
            return;
        };
        let next = reader.doc.direction.toggled();
        reader.doc.direction = next;
        let _ = self.store.set_document_direction(&reader.doc.id, next);
        self.mark_dirty();
    }

    pub(crate) fn open_picker(
        &mut self,
        purpose: PickerPurpose,
        mode: PickerMode,
        title: &str,
        extensions: &[&str],
        default_name: &str,
    ) {
        let start = self
            .library
            .as_ref()
            .map(|l| PathBuf::from(&l.root_path))
            .unwrap_or_else(|| self.config.data_dir.clone());
        self.picker = Some(FilePicker::new(mode, title, start, extensions, default_name));
        self.picker_purpose = Some(purpose);
    }

    fn handle_picker_result(&mut self, result: PickerResult, ctx: &egui::Context) {
        let Some(purpose) = self.picker_purpose else { return };
        match result {
            PickerResult::None => {}
            PickerResult::Cancelled => {
                self.picker = None;
                self.picker_purpose = None;
            }
            PickerResult::Picked(paths) => {
                self.picker = None;
                self.picker_purpose = None;
                match purpose {
                    PickerPurpose::ImportMarkdown => self.import_markdown(&paths),
                    PickerPurpose::AddFolder => {
                        if let Some(folder) = paths.first() {
                            self.add_folder_to_library(folder);
                        }
                    }
                    PickerPurpose::AddLibrary => {
                        if let Some(folder) = paths.first() {
                            self.add_library(folder);
                        }
                    }
                    PickerPurpose::ExportLibrary => {
                        if let Some(path) = paths.first() {
                            self.start_library_export(path.clone());
                        }
                    }
                    PickerPurpose::ExportQuoteImage => {
                        if let Some(path) = paths.first() {
                            self.write_quote_image(path);
                        }
                    }
                    PickerPurpose::ExportDiagram => {
                        if let Some(path) = paths.first() {
                            self.write_diagram_image(path);
                        }
                    }
                    PickerPurpose::ExportBatchZip | PickerPurpose::ExportBatchPdf | PickerPurpose::ExportBatchFolder => {
                        if let Some(path) = paths.first() {
                            self.write_batch_export(path);
                        }
                    }
                    PickerPurpose::ImportLibraryArchive => {
                        if let Some(path) = paths.first() {
                            self.open_import_preview(path);
                        }
                    }
                    PickerPurpose::ImportFont => {
                        if let Some(path) = paths.first() {
                            self.import_font(path);
                        }
                    }
                }
                self.resolve_dark(ctx);
            }
        }
    }

    fn import_markdown(&mut self, paths: &[PathBuf]) {
        let Some(library) = &self.library else { return };
        let root = PathBuf::from(&library.root_path);
        let target_dir = root.join("Imported");
        if let Err(error) = std::fs::create_dir_all(&target_dir) {
            self.toast(crate::i18n::tf("Could not create Imported folder: {}", &[&error.to_string()]), ToastKind::Error);
            return;
        }
        let mut copied = 0u64;
        for path in paths {
            if !is_markdown(path) {
                continue;
            }
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let target = unique_import_path(&target_dir, &name);
            match std::fs::copy(path, &target) {
                Ok(_) => copied += 1,
                Err(error) => self.toast(
                    crate::i18n::tf("Could not import {}: {}", &[&name.to_string(), &error.to_string()]),
                    ToastKind::Error,
                ),
            }
        }
        if copied > 0 {
            self.toast(
                crate::i18n::tf("Imported {} Markdown file(s). Indexing…", &[&copied.to_string()]),
                ToastKind::Success,
            );
            if let Some(library) = &self.library {
                let _ = self
                    .jobs
                    .start_index(self.config.db_path.clone(), library.clone());
            }
        }
    }

    fn add_folder_to_library(&mut self, folder: &Path) {
        let Some(library) = self.library.clone() else { return };
        let root = PathBuf::from(&library.root_path);
        let source = match folder.canonicalize() {
            Ok(p) => p,
            Err(error) => {
                self.toast(crate::i18n::tf("Cannot open folder: {}", &[&error.to_string()]), ToastKind::Error);
                return;
            }
        };
        let root_canonical = root.canonicalize().unwrap_or(root.clone());
        if source.starts_with(&root_canonical) {
            let _ = self.jobs.start_index(self.config.db_path.clone(), library);
            self.toast(crate::i18n::t("Folder is already inside the library. Re-indexing…"), ToastKind::Info);
            return;
        }
        let name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Imported folder")
            .to_string();
        let target = unique_directory(&root, &name);
        match copy_tree(folder, &target) {
            Ok(count) => {
                let _ = self.jobs.start_index(self.config.db_path.clone(), library);
                self.toast(
                    crate::i18n::tf("Added {} file(s) from `{}` into the library.", &[&count.to_string(), &name.to_string()]),
                    ToastKind::Success,
                );
            }
            Err(error) => self.toast(crate::i18n::tf("Could not copy folder: {}", &[&error.to_string()]), ToastKind::Error),
        }
    }

    fn add_library(&mut self, folder: &Path) {
        let root = folder.to_path_buf();
        if !root.exists() {
            let _ = std::fs::create_dir_all(&root);
        }
        let root_str = root.to_string_lossy().to_string();
        if let Ok(Some(existing)) = self.store.library_by_root(&root_str) {
            self.switch_library(&existing.id);
            return;
        }
        let mut library = Library::new(library_display_name(&root), root_str);
        library.updated_at = now_ms();
        match self.store.upsert_library(&library) {
            Ok(()) => {
                self.libraries.push(library.clone());
                self.settings.general.default_library_root = library.root_path.clone();
                self.library = Some(library);
                self.close_reader();
                self.reload();
                self.mark_dirty();
                self.start_watcher();
                let _ = self.jobs.start_index(self.config.db_path.clone(), self.library.clone().unwrap());
                self.toast(crate::i18n::t("Library added."), ToastKind::Success);
            }
            Err(error) => self.toast(crate::i18n::tf("Could not add library: {}", &[&error.to_string()]), ToastKind::Error),
        }
    }

    pub(crate) fn start_library_export(&mut self, path: PathBuf) {
        let Some(library) = self.library.clone() else { return };
        let settings_toml = self.settings.to_toml().ok();
        if self.jobs.start_export(
            self.config.db_path.clone(),
            library,
            path,
            true,
            settings_toml,
        ) {
            self.status = crate::i18n::t("Exporting library…").into();
        } else {
            self.toast(crate::i18n::t("Another background task is running."), ToastKind::Warning);
        }
    }

    fn write_quote_image(&mut self, path: &Path) {
        let Some((quote, source)) = self.pending_quote_export.take() else { return };
        self.write_card_png(path, &quote, &source);
    }

    fn write_card_png(&mut self, path: &Path, quote: &str, source: &str) {
        let mut style = if self.dark { CardStyle::dark() } else { CardStyle::light() };
        style.font_family = self.settings.reading.typography.font_family.clone();
        style.heading_family = style.font_family.clone();
        style.scale = 3.0;
        let content = CardContent {
            kind: daftar_export::CardKind::Highlight,
            prompt: quote.to_string(),
            answer: None,
            note: None,
            source: Some(source.to_string()),
            tags: Vec::new(),
            side_label: Some("HIGHLIGHT".into()),
            include_back: false,
        };
        match daftar_export::render_card_png(&content, &style, &mut self.engine) {
            Ok(bytes) => match std::fs::write(path, bytes) {
                Ok(()) => self.toast(
                    format!("Image exported to {}", path.display()),
                    ToastKind::Success,
                ),
                Err(error) => self.toast(crate::i18n::tf("Could not write image: {}", &[&error.to_string()]), ToastKind::Error),
            },
            Err(error) => self.toast(crate::i18n::tf("Image export failed: {}", &[&error.to_string()]), ToastKind::Error),
        }
    }

    fn write_diagram_image(&mut self, path: &Path) {
        let Some((key, _title)) = self.pending_diagram_export.take() else { return };
        let Some(reader) = &self.reader else { return };
        let Some(entry) = reader.diagrams.get(&key) else {
            self.toast(crate::i18n::t("Diagram is no longer available."), ToastKind::Warning);
            return;
        };
        let Some(built) = &entry.built else {
            self.toast(crate::i18n::t("Diagram could not be rendered."), ToastKind::Error);
            return;
        };
        match built.encode_png(&mut self.engine, 2.0) {
            Ok(bytes) => match std::fs::write(path, bytes) {
                Ok(()) => self.toast(
                    format!("Diagram exported to {}", path.display()),
                    ToastKind::Success,
                ),
                Err(error) => self.toast(crate::i18n::tf("Could not write image: {}", &[&error.to_string()]), ToastKind::Error),
            },
            Err(error) => self.toast(error, ToastKind::Error),
        }
    }

    pub(crate) fn start_batch_export(&mut self, format: BatchFormat) {
        let cards = self.batch_cards();
        if cards.is_empty() {
            self.toast(crate::i18n::t("Nothing selected to export."), ToastKind::Warning);
            return;
        }
        self.pending_batch = Some((cards, format));
        let (purpose, mode, title, exts, default_name) = match format {
            BatchFormat::PngZip => (
                PickerPurpose::ExportBatchZip,
                PickerMode::SaveFile,
                crate::i18n::t("Export selected cards as PNG ZIP"),
                &["zip"][..],
                "daftar-cards.zip",
            ),
            BatchFormat::Pdf => (
                PickerPurpose::ExportBatchPdf,
                PickerMode::SaveFile,
                crate::i18n::t("Export selected cards as PDF"),
                &["pdf"][..],
                "daftar-cards.pdf",
            ),
            BatchFormat::PngFolder => (
                PickerPurpose::ExportBatchFolder,
                PickerMode::OpenFolder,
                crate::i18n::t("Choose a folder for PNG cards"),
                &[][..],
                "",
            ),
        };
        self.open_picker(purpose, mode, title, exts, default_name);
    }

    fn batch_cards(&self) -> Vec<daftar_export::CardContent> {
        let mut cards = Vec::new();
        match self.view {
            View::Highlights => match self.highlights.tab {
                crate::views::highlights::MarksTab::Highlights => {
                    let filter = self.highlights.filter.to_lowercase();
                    for h in &self.highlights.highlights {
                        if !self.highlights.selected.is_empty() && !self.highlights.selected.contains(&h.id) {
                            continue;
                        }
                        if !self.highlights.selected.is_empty() || filter.is_empty()
                            || h.quote.to_lowercase().contains(&filter)
                            || h.note.to_lowercase().contains(&filter)
                        {
                            let title = self.docs.iter().find(|d| d.id == h.document_id).map(|d| d.title.as_str());
                            cards.push(daftar_export::CardContent::highlight(h, title));
                        }
                    }
                }
                crate::views::highlights::MarksTab::Bookmarks => {
                    for b in &self.highlights.bookmarks {
                        if !self.highlights.selected.is_empty() && !self.highlights.selected.contains(&b.id) {
                            continue;
                        }
                        let title = self.docs.iter().find(|d| d.id == b.document_id).map(|d| d.title.as_str());
                        cards.push(daftar_export::CardContent::bookmark(b, title));
                    }
                }
            },
            View::Flashcards => {
                for card in self.filtered_cards() {
                    if !self.flashcards.selected.is_empty() && !self.flashcards.selected.contains(&card.id) {
                        continue;
                    }
                    if !self.flashcards.selected.is_empty() || self.flashcards.selected.is_empty() {
                        let title = card
                            .document_id
                            .as_ref()
                            .and_then(|id| self.docs.iter().find(|d| &d.id == id))
                            .map(|d| d.title.as_str());
                        cards.push(daftar_export::CardContent::flashcard(&card, title));
                    }
                }
            }
            _ => {}
        }
        cards
    }

    fn write_batch_export(&mut self, path: &Path) {
        let Some((cards, format)) = self.pending_batch.take() else { return };
        let mut style = if self.dark { CardStyle::dark() } else { CardStyle::light() };
        style.font_family = self.settings.reading.typography.font_family.clone();
        style.heading_family = style.font_family.clone();
        style.scale = 3.0;
        let result = match format {
            BatchFormat::PngZip => daftar_export::export_cards_png_zip(
                &cards,
                &style,
                &mut self.engine,
                path,
                "daftar",
            )
            .map(|report| (report.files, report.bytes)),
            BatchFormat::PngFolder => daftar_export::export_cards_png_directory(
                &cards,
                &style,
                &mut self.engine,
                path,
                "daftar",
            )
            .map(|report| (report.files, report.bytes)),
            BatchFormat::Pdf => {
                let options = daftar_export::PdfOptions {
                    paper: daftar_export::PaperSize::A4,
                    cards_per_page: daftar_export::CardsPerPage::Grid { cols: 2, rows: 3 },
                    ..Default::default()
                };
                daftar_export::export_cards_pdf(
                    &cards,
                    &style,
                    &mut self.engine,
                    &options,
                    path,
                )
                .map(|report| (report.cards as u64, report.bytes))
            }
        };
        match result {
            Ok((count, bytes)) => self.toast(
                crate::i18n::tf(
                    "Exported {} card(s), {} KB.",
                    &[&count.to_string(), &format!("{:.1}", bytes as f64 / 1024.0)],
                ),
                ToastKind::Success,
            ),
            Err(error) => self.toast(crate::i18n::tf("Export failed: {}", &[&error.to_string()]), ToastKind::Error),
        }
    }

    fn import_font(&mut self, path: &Path) {
        let name = match self.engine.register_font_file(path) {
            Ok(name) => name,
            Err(error) => {
                self.toast(error, ToastKind::Error);
                return;
            }
        };
        let _ = std::fs::create_dir_all(&self.config.fonts_dir);
        let target = self
            .config
            .fonts_dir
            .join(format!("custom-{}", safe_file_stem(&name)));
        let _ = std::fs::copy(path, &target);
        self.settings.reading.typography.font_family = name.clone();
        self.mark_dirty();
        self.toast(crate::i18n::tf("Font `{}` imported.", &[&name.to_string()]), ToastKind::Success);
    }

    pub(crate) fn open_import_preview(&mut self, archive: &Path) {
        let Some(library) = &self.library else { return };
        match daftar_export::preview_import(&self.store, archive, &library.id) {
            Ok(preview) => {
                self.import_dialog = Some(ImportDialogState {
                    archive: archive.to_path_buf(),
                    preview,
                    strategy: daftar_core::archive::DuplicateStrategy::Skip,
                    restore_settings: false,
                    restore_cards: true,
                    restore_roadmaps: true,
                });
            }
            Err(error) => self.toast(crate::i18n::tf("Invalid archive: {}", &[&error.to_string()]), ToastKind::Error),
        }
    }

    pub(crate) fn apply_import(&mut self) {
        let Some(dialog) = &self.import_dialog else { return };
        let Some(library) = self.library.clone() else { return };
        if !dialog.preview.ok {
            self.toast(crate::i18n::t("Archive has validation errors and cannot be imported."), ToastKind::Error);
            return;
        }
        let options = ImportOptions {
            duplicate_strategy: dialog.strategy,
            restore_settings: dialog.restore_settings,
            restore_reading_progress: true,
            restore_flashcards: dialog.restore_cards,
            restore_roadmaps: dialog.restore_roadmaps,
        };
        if self.jobs.start_import(
            self.config.db_path.clone(),
            dialog.archive.clone(),
            library.id.clone(),
            PathBuf::from(&library.root_path),
            options,
        ) {
            self.status = crate::i18n::t("Importing library…").into();
        } else {
            self.toast(crate::i18n::t("Another background task is running."), ToastKind::Warning);
        }
    }

    pub(crate) fn execute_confirm(&mut self, action: ConfirmAction) {
        let library_id = self.library.as_ref().map(|l| l.id.clone());
        match action {
            ConfirmAction::DeleteDocument(id) => {
                if self.reader.as_ref().is_some_and(|r| r.doc.id == id) {
                    self.close_reader();
                    self.view = View::Library;
                }
                let _ = self.store.delete_document(&id);
                self.toast(crate::i18n::t("Document removed from the library. The file was not deleted."), ToastKind::Info);
            }
            ConfirmAction::DeleteCollection(id) => {
                let _ = self.store.delete_collection(&id);
            }
            ConfirmAction::DeleteCategory(id) => {
                let _ = self.store.delete_category(&id);
            }
            ConfirmAction::DeleteHighlight(id) => {
                let _ = self.store.delete_highlight(&id);
                if let Some(reader) = &mut self.reader {
                    reader.highlights.retain(|h| h.id != id);
                }
            }
            ConfirmAction::DeleteFlashcard(id) => {
                let _ = self.store.delete_flashcard(&id);
            }
            ConfirmAction::DeleteRoadmap(id) => {
                let _ = self.store.delete_roadmap(&id);
                self.roadmaps.reload(&self.store, self.library.as_ref());
            }
            ConfirmAction::DeleteRoadmapNode(id) => {
                let _ = self.store.delete_roadmap_node(&id);
                self.roadmaps.reload(&self.store, self.library.as_ref());
            }
            ConfirmAction::RemoveTag { document_id, tag } => {
                if let Some(library) = &self.library {
                    let mut tags = self.store.document_tags(&document_id).unwrap_or_default();
                    tags.retain(|t| !t.eq_ignore_ascii_case(&tag));
                    let _ = self.store.set_document_tags(&document_id, &library.id, &tags);
                }
            }
        }
        let _ = library_id;
        self.reload();
    }

    pub(crate) fn execute_prompt(&mut self, action: PromptAction, value: String) {
        let value = value.trim().to_string();
        if value.is_empty() {
            return;
        }
        let Some(library) = self.library.clone() else { return };
        match action {
            PromptAction::NewCollection => {
                let order = self.collections.len() as i32;
                let collection = Collection::new(library.id.clone(), value.clone(), order);
                if let Err(error) = self.store.upsert_collection(&collection) {
                    self.toast(crate::i18n::tf("Could not create collection: {}", &[&error.to_string()]), ToastKind::Error);
                }
            }
            PromptAction::NewCategory => {
                let order = self.categories.len() as i32;
                let category = Category::new(library.id.clone(), value.clone(), order);
                if let Err(error) = self.store.upsert_category(&category) {
                    self.toast(crate::i18n::tf("Could not create category: {}", &[&error.to_string()]), ToastKind::Error);
                }
            }
            PromptAction::RenameDocument(id) => {
                let _ = self.store.update_document_title(&id, &value);
                if let Some(reader) = &mut self.reader {
                    if reader.doc.id == id {
                        reader.doc.title = value;
                    }
                }
            }
            PromptAction::NewTag(document_id) => {
                let mut tags = self.store.document_tags(&document_id).unwrap_or_default();
                if !tags.iter().any(|t| t.eq_ignore_ascii_case(&value)) {
                    tags.push(value);
                }
                let _ = self.store.set_document_tags(&document_id, &library.id, &tags);
            }
            PromptAction::NewRoadmap => {
                let roadmap = Roadmap::new(library.id.clone(), value.clone());
                if let Err(error) = self.store.upsert_roadmap(&roadmap) {
                    self.toast(crate::i18n::tf("Could not create roadmap: {}", &[&error.to_string()]), ToastKind::Error);
                } else {
                    self.roadmaps.reload(&self.store, self.library.as_ref());
                    self.roadmaps.select(&roadmap.id);
                    self.roadmaps.reload(&self.store, self.library.as_ref());
                }
            }
        }
        self.reload();
    }

    pub(crate) fn handle_reader_actions(&mut self, actions: Vec<ReaderAction>, ctx: &egui::Context) {
        for action in actions {
            match action {
                ReaderAction::Back => {
                    self.close_reader();
                    self.view = View::Library;
                    self.nav = NavSection::Library;
                    self.reload();
                }
                ReaderAction::OpenDocument(id) => self.open_document(&id),
                ReaderAction::OpenExternal(url) => open_external(&url),
                ReaderAction::SetDirection(direction) => {
                    if let Some(reader) = &mut self.reader {
                        reader.doc.direction = direction;
                        let _ = self.store.set_document_direction(&reader.doc.id, direction);
                        reader.update_layout(
                            &mut self.engine,
                            &self.code,
                            &Palette::new(self.dark),
                            &self.settings,
                            self.last_central_size.x - 60.0,
                        );
                    }
                }
                ReaderAction::SetReadingWidth { preset, width } => {
                    if let Some(reader) = &mut self.reader {
                        reader.doc.reading_width_preset = preset;
                        reader.doc.reading_width = width;
                        let _ = self
                            .store
                            .set_document_reading_width(&reader.doc.id, preset, width);
                        let id = reader.doc.id.clone();
                        // Keep the library list in sync with the per-document override.
                        if let Some(doc) = self.docs.iter_mut().find(|doc| doc.id == id) {
                            doc.reading_width_preset = preset;
                            doc.reading_width = width;
                        }
                        reader.update_layout(
                            &mut self.engine,
                            &self.code,
                            &Palette::new(self.dark),
                            &self.settings,
                            self.last_central_size.x - 60.0,
                        );
                    }
                }
                ReaderAction::ChangeFontSize(delta) => self.adjust_font_size(delta),
                ReaderAction::ToggleTheme => self.toggle_theme(ctx),
                ReaderAction::ToggleRightPanel => {
                    self.show_right = !self.show_right;
                    self.settings.window.right_panel_open = self.show_right;
                    self.mark_dirty();
                }
                ReaderAction::CreateFlashcard { document_id, quote } => {
                    self.open_flashcard_from_quote(&document_id, &quote, None);
                }
                ReaderAction::ExportQuoteImage { quote, source } => {
                    self.pending_quote_export = Some((quote, source));
                    self.open_picker(
                        PickerPurpose::ExportQuoteImage,
                        PickerMode::SaveFile,
                        crate::i18n::t("Export selection as PNG"),
                        &["png"],
                        "daftar-highlight.png",
                    );
                }
                ReaderAction::ExportDiagram { key, title } => {
                    self.pending_diagram_export = Some((key, title));
                    self.open_picker(
                        PickerPurpose::ExportDiagram,
                        PickerMode::SaveFile,
                        crate::i18n::t("Export diagram as PNG"),
                        &["png"],
                        "daftar-diagram.png",
                    );
                }
                ReaderAction::Notice(message, kind) => self.toast(message, kind),
            }
        }
    }

    pub fn open_flashcard_from_quote(
        &mut self,
        document_id: &str,
        quote: &str,
        back: Option<String>,
    ) {
        let mut card = Flashcard::new(quote.to_string(), back.unwrap_or_default());
        card.document_id = Some(document_id.to_string());
        card.tags = self.store.document_tags(document_id).unwrap_or_default();
        let source_title = self
            .docs
            .iter()
            .find(|d| d.id == document_id)
            .map(|d| d.title.clone());
        self.flashcard_editor = Some(FlashcardEditor {
            card,
            is_new: true,
            source_title,
            error: None,
        });
    }

    fn theme_toggle_ctx(&mut self, ctx: &egui::Context) {
        self.toggle_theme(ctx);
    }

    // ------------------------------------------------------------------------------------
    // Overlays
    // ------------------------------------------------------------------------------------

    fn show_overlays(&mut self, ctx: &egui::Context) {
        let palette = Palette::new(self.dark);
        self.show_toasts(ctx, &palette);
        self.show_confirm(ctx, &palette);
        self.show_prompt(ctx, &palette);
        self.show_import_dialog(ctx, &palette);
        self.show_paste_markdown_dialog(ctx, &palette);
        self.show_flashcard_editor(ctx, &palette);
        if let Some(picker) = &mut self.picker {
            if let Some(result) = picker.show(ctx, &mut self.engine, &palette) {
                if result != PickerResult::None {
                    self.handle_picker_result(result, ctx);
                }
            }
        }
    }

    fn show_toasts(&mut self, ctx: &egui::Context, palette: &Palette) {
        let now = now_ms();
        self.toasts.retain(|t| now - t.created < 4800);
        let mut close: Option<u64> = None;
        let screen = ctx.content_rect();
        let mut y = screen.bottom() - 56.0;
        for toast in self.toasts.iter().rev() {
            egui::Area::new(EguiId::new(("toast", toast.id)))
            .layout(crate::rtl::root_layout())
                .order(Order::Tooltip)
                .fixed_pos(egui::pos2(
                    if crate::rtl::is_rtl() {
                        screen.left() + 20.0
                    } else {
                        screen.right() - 380.0
                    },
                    y,
                ))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(palette.surface)
                        .stroke(Stroke::new(1.0, palette.border))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::same(10))
                        .shadow(egui::epaint::Shadow {
                            offset: [0, 4],
                            blur: 16,
                            spread: 0,
                            color: palette.shadow,
                        })
                        .show(ui, |ui| {
                            ui.set_width(340.0);
                            ui.horizontal(|ui| {
                                let icon = match toast.kind {
                                    ToastKind::Info => Icon::Info,
                                    ToastKind::Success => Icon::Check,
                                    ToastKind::Warning => Icon::Warning,
                                    ToastKind::Error => Icon::Error,
                                };
                                let color = theme::toast_color(palette, toast.kind);
                                let (r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                draw_icon(ui.painter(), icon, r, color);
                                ui.label(RichText::new(&toast.message).size(12.0));
                                if widgets::icon_button_sized(ui, palette, Icon::Close, "Dismiss", false, 20.0).clicked() {
                                    close = Some(toast.id);
                                }
                            });
                        });
                });
            y -= 56.0;
        }
        if let Some(id) = close {
            self.toasts.retain(|t| t.id != id);
        }
    }

    fn show_confirm(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(dialog) = &self.confirm else { return };
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(&dialog.title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.set_max_width(420.0);
                    ui.label(RichText::new(&dialog.message).size(12.5).color(palette.text_secondary));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, palette, &dialog.confirm_label, Some(Icon::Trash), ButtonVariant::Danger).clicked() {
                            confirm = true;
                        }
                        if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if confirm {
            if let Some(dialog) = self.confirm.take() {
                self.execute_confirm(dialog.action);
            }
        } else if cancel {
            self.confirm = None;
        }
    }

    fn show_prompt(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(prompt) = self.prompt.as_mut() else { return };
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(&prompt.title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.set_max_width(420.0);
                    ui.label(RichText::new(&prompt.label).size(12.0).color(palette.text_secondary));
                    ui.add_space(4.0);
                    let response = widgets::text_field(ui, palette, &mut prompt.value, "", 360.0);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        save = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, palette, "Save", Some(Icon::Check), ButtonVariant::Primary).clicked() {
                            save = true;
                        }
                        if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if save {
            if let Some(prompt) = self.prompt.take() {
                self.execute_prompt(prompt.action, prompt.value);
            }
        } else if cancel {
            self.prompt = None;
        }
    }

    fn show_import_dialog(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(dialog) = self.import_dialog.as_mut() else { return };
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::t("Import library archive"))
            .collapsible(false)
            .resizable(true)
            .default_width(560.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.label(RichText::new(dialog.archive.display().to_string()).size(11.0).color(palette.text_secondary));
                    ui.separator();
                    if let Some(manifest) = &dialog.preview.manifest {
                        ui.label(RichText::new(&manifest.library_name).size(15.0).strong());
                        ui.label(crate::i18n::tf(
                            "Daftar {} · archive format v{} · {} documents · {} attachments · {} words",
                            &[
                                &manifest.app_version,
                                &manifest.format_version.to_string(),
                                &manifest.documents.to_string(),
                                &manifest.attachments.to_string(),
                                &manifest.total_words.to_string(),
                            ]
                        ));
                    }
                    ui.label(crate::i18n::tf(
                            "{} new document(s), {} already present. Archive size {}.",
                        &[
                            &dialog.preview.new_documents.to_string(),
                            &dialog.preview.duplicate_documents.to_string(),
                            &format_bytes(dialog.preview.total_bytes),
                        ]
                    ));
                    if !dialog.preview.fatal_errors.is_empty() {
                        ui.colored_label(palette.danger, crate::i18n::t("This archive cannot be imported:"));
                        for error in &dialog.preview.fatal_errors {
                            ui.label(RichText::new(format!("• {error}")).size(11.5).color(palette.danger));
                        }
                    }
                    for warning in dialog.preview.warnings.iter().take(4) {
                        ui.label(RichText::new(format!("• {warning}")).size(11.5).color(palette.warning));
                    }
                    ui.separator();
                    ui.label(RichText::new(crate::i18n::t("Duplicate documents")).strong());
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dialog.strategy, daftar_core::archive::DuplicateStrategy::Skip, crate::i18n::t("Skip existing"));
                        ui.radio_value(&mut dialog.strategy, daftar_core::archive::DuplicateStrategy::Replace, crate::i18n::t("Replace"));
                        ui.radio_value(&mut dialog.strategy, daftar_core::archive::DuplicateStrategy::Rename, crate::i18n::t("Keep both"));
                    });
                    ui.add_space(4.0);
                    ui.checkbox(&mut dialog.restore_settings, crate::i18n::t("Restore application settings"));
                    ui.checkbox(&mut dialog.restore_cards, crate::i18n::t("Restore flashcards"));
                    ui.checkbox(&mut dialog.restore_roadmaps, crate::i18n::t("Restore study roadmaps"));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let enabled = dialog.preview.ok && !self.jobs.is_busy();
                        ui.add_enabled_ui(enabled, |ui| {
                            if widgets::button(ui, palette, "Import", Some(Icon::Import), ButtonVariant::Primary).clicked() {
                                apply = true;
                            }
                        });
                        if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if apply {
            self.apply_import();
        } else if cancel {
            self.import_dialog = None;
        }
    }

    fn show_paste_markdown_dialog(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(dialog) = self.paste_markdown.as_mut() else { return };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(crate::i18n::t("Import Markdown text"))
            .collapsible(false)
            .resizable(true)
            .default_size([640.0, 500.0])
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.label(crate::i18n::t("Document title (optional)"));
                    ui.add(egui::TextEdit::singleline(&mut dialog.title).desired_width(ui.available_width()));
                    ui.label(crate::i18n::t(
                        "A Markdown heading sets the document title; this value names the file.",
                    ));
                    ui.add_space(6.0);
                    ui.label(crate::i18n::t("Markdown content"));
                    ui.add(
                        egui::TextEdit::multiline(&mut dialog.markdown)
                            .desired_width(ui.available_width())
                            .desired_rows(18)
                            .hint_text(crate::i18n::t("Paste Markdown content here…")),
                    );
                    if let Some(error) = &dialog.error {
                        ui.colored_label(palette.danger, error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!dialog.markdown.trim().is_empty(), |ui| {
                            if widgets::button(ui, palette, "Add to library", Some(Icon::Import), ButtonVariant::Primary).clicked() {
                                save = true;
                            }
                        });
                        if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });

        if cancel || !open {
            self.paste_markdown = None;
            return;
        }
        if !save {
            return;
        }

        let Some(library) = self.library.clone() else {
            if let Some(dialog) = &mut self.paste_markdown {
                dialog.error = Some(crate::i18n::t("Open a library before importing Markdown.").into());
            }
            return;
        };
        let (title, markdown) = self
            .paste_markdown
            .as_ref()
            .map(|dialog| (dialog.title.clone(), dialog.markdown.clone()))
            .unwrap_or_default();
        match crate::tasks::write_pasted_markdown(Path::new(&library.root_path), &title, &markdown) {
            Ok(path) => {
                self.paste_markdown = None;
                self.pending_paths.push(path);
                self.pending_paths.sort();
                self.pending_paths.dedup();
                self.last_fs_event = now_ms();
                self.status = crate::i18n::t("Markdown text added; indexing queued.").into();
                self.toast(crate::i18n::t("Markdown text added to the library."), ToastKind::Success);
            }
            Err(error) => {
                if let Some(dialog) = &mut self.paste_markdown {
                    dialog.error = Some(error);
                }
            }
        }
    }

    fn show_flashcard_editor(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(editor) = self.flashcard_editor.as_mut() else { return };
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(if editor.is_new { "Create flashcard" } else { "Edit flashcard" })
            .collapsible(false)
            .resizable(true)
            .default_width(480.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.label(RichText::new(crate::i18n::t("Front")).size(11.5).color(palette.text_secondary));
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.card.front)
                            .desired_rows(3)
                            .desired_width(440.0),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(crate::i18n::t("Back")).size(11.5).color(palette.text_secondary));
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.card.back)
                            .desired_rows(3)
                            .desired_width(440.0),
                    );
                    ui.add_space(4.0);
                    ui.label(RichText::new(crate::i18n::t("Explanation (optional)")).size(11.5).color(palette.text_secondary));
                    ui.add(
                        egui::TextEdit::multiline(&mut editor.card.explanation)
                            .desired_rows(2)
                            .desired_width(440.0),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("Tags"));
                        let mut tags = editor.card.tags.join(", ");
                        if widgets::text_field(ui, palette, &mut tags, crate::i18n::t("comma separated"), 320.0).changed() {
                            editor.card.tags = tags
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    });
                    if let Some(source) = &editor.source_title {
                        ui.label(RichText::new(crate::i18n::tf("Source · {}", &[&source])).size(10.5).color(palette.text_secondary));
                    }
                    if let Some(error) = &editor.error {
                        ui.colored_label(palette.danger, error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, palette, "Save card", Some(Icon::Check), ButtonVariant::Primary).clicked() {
                            save = true;
                        }
                        if widgets::button(ui, palette, "Cancel", None, ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if save {
            if editor.card.front.trim().is_empty() || editor.card.back.trim().is_empty() {
                editor.error = Some(crate::i18n::t("Front and back cannot be empty.").into());
            } else {
                if editor.is_new {
                    editor.card.created_at = now_ms();
                }
                editor.card.updated_at = now_ms();
                match self.store.upsert_flashcard(&editor.card) {
                    Ok(()) => {
                        self.toast(crate::i18n::t("Flashcard saved."), ToastKind::Success);
                        self.flashcards.reload(&self.store, &self.categories, &self.tags);
                        self.flashcard_editor = None;
                    }
                    Err(error) => editor.error = Some(crate::i18n::tf("Could not save card: {}", &[&error.to_string()])),
                }
            }
        } else if cancel {
            self.flashcard_editor = None;
        }
    }
}

impl eframe::App for DaftarApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.resolve_dark(&ctx);
        self.poll_jobs(&ctx);
        self.poll_watcher(&ctx);
        self.handle_shortcuts(ui);
        let palette = Palette::new(self.dark);
        let mut reader_actions = Vec::new();
        // eframe 0.37 hands us a root Ui whose layout can arrange children horizontally.
        // Create an explicit full-window, top-down root so panels nest as a desktop shell.
        let screen = ctx.content_rect();
        let mut root = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(screen)
                .layout(crate::rtl::root_layout()),
        );
        let ui = &mut root;
        let screen = ctx.content_rect();
        let top_h = 54.0;
        let bottom_h = 28.0;
        let mut left_w = if self.show_left {
            self.settings.window.left_panel_width.clamp(190.0, 380.0)
        } else {
            0.0
        };
        let mut right_w = if self.view == View::Reader && self.show_right && self.reader.is_some() {
            self.settings.window.right_panel_width.clamp(220.0, 520.0)
        } else {
            0.0
        };
        let top = egui::Rect::from_min_size(screen.min, vec2(screen.width(), top_h));
        let bottom = egui::Rect::from_min_max(
            egui::pos2(screen.left(), screen.bottom() - bottom_h),
            screen.max,
        );
        let body = egui::Rect::from_min_max(
            egui::pos2(screen.left(), top.bottom()),
            egui::pos2(screen.right(), bottom.top()),
        );
        let splitter_w = 5.0;
        if self.show_left {
            let splitter = crate::rtl::leading_splitter(body, left_w, splitter_w);
            let response = ui.interact(splitter, EguiId::new("left-splitter"), Sense::drag());
            if response.dragged() {
                if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                    left_w = crate::rtl::leading_width_for(body, pointer.x).clamp(190.0, 380.0);
                }
            }
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
        if right_w > 0.0 {
            let splitter = crate::rtl::trailing_splitter(body, right_w, splitter_w);
            let response = ui.interact(splitter, EguiId::new("right-splitter"), Sense::drag());
            if response.dragged() {
                if let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) {
                    right_w = crate::rtl::trailing_width_for(body, pointer.x).clamp(220.0, 520.0);
                }
            }
            if response.hovered() || response.dragged() {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        }
        if self.show_left && (left_w - self.settings.window.left_panel_width).abs() > 0.5 {
            self.settings.window.left_panel_width = left_w;
            self.mark_dirty();
        }
        if right_w > 0.0 && (right_w - self.settings.window.right_panel_width).abs() > 0.5 {
            self.settings.window.right_panel_width = right_w;
            self.mark_dirty();
        }
        let left_rect = crate::rtl::leading_block(body, left_w);
        let right_rect = crate::rtl::trailing_block(body, right_w);
        let central = crate::rtl::central_block(body, left_w, right_w);
        {
            let painter = ui.painter();
            painter.rect_filled(screen, 0.0, palette.canvas);
            painter.rect_filled(top, 0.0, palette.surface);
            painter.rect_filled(bottom, 0.0, palette.surface);
            if left_w > 0.0 {
                painter.rect_filled(left_rect, 0.0, palette.surface);
            }
            if right_w > 0.0 {
                painter.rect_filled(right_rect, 0.0, palette.surface);
            }
            painter.rect_filled(central, 0.0, palette.canvas);
            painter.rect_filled(
                egui::Rect::from_min_size(
                    egui::pos2(top.left(), top.bottom() - 1.0),
                    vec2(top.width(), 1.0),
                ),
                0.0,
                palette.border,
            );
            painter.rect_filled(
                egui::Rect::from_min_size(bottom.min, vec2(bottom.width(), 1.0)),
                0.0,
                palette.border,
            );
            if left_w > 0.0 {
                let edge = crate::rtl::leading_inner_edge(left_rect);
                let x = if crate::rtl::is_rtl() { edge } else { edge - 1.0 };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, left_rect.top()),
                        vec2(1.0, left_rect.height()),
                    ),
                    0.0,
                    palette.border,
                );
            }
            if right_w > 0.0 {
                let edge = crate::rtl::trailing_inner_edge(right_rect);
                let x = if crate::rtl::is_rtl() { edge - 1.0 } else { edge };
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, right_rect.top()),
                        vec2(1.0, right_rect.height()),
                    ),
                    0.0,
                    palette.border,
                );
            }
        }
        {
            let mut top_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(top.shrink2(vec2(12.0, 7.0)))
                    .layout(crate::rtl::bar_layout()),
            );
            top_ui.shrink_clip_rect(top);
            self.show_topbar(&mut top_ui, &palette);
        }
        {
            let mut bottom_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(bottom.shrink2(vec2(12.0, 4.0)))
                    .layout(crate::rtl::bar_layout()),
            );
            bottom_ui.shrink_clip_rect(bottom);
            self.show_statusbar(&mut bottom_ui, &palette, &ctx);
        }
        if left_w > 0.0 {
            let inner = left_rect.shrink2(vec2(10.0, 8.0));
            let mut sidebar_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(crate::rtl::root_layout()),
            );
            sidebar_ui.shrink_clip_rect(left_rect);
            egui::ScrollArea::vertical()
                .id_salt("sidebar-scroll")
                .auto_shrink([false, false])
                .show(&mut sidebar_ui, |ui| {
                    self.show_sidebar(ui, &palette);
                });
        }
        if right_w > 0.0 {
            let inner = right_rect.shrink2(vec2(10.0, 8.0));
            let mut side_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(inner)
                    .layout(crate::rtl::root_layout()),
            );
            side_ui.shrink_clip_rect(right_rect);
            if let Some(reader) = &mut self.reader {
                reader.show_side_panel(
                    &mut side_ui,
                    &mut self.engine,
                    &palette,
                    &self.settings,
                    &self.store,
                    &mut reader_actions,
                );
            }
        }
        let central_inner = central.shrink2(vec2(18.0, 10.0));
        self.last_central_size = central_inner.size();
        let mut central_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(central_inner)
                .layout(crate::rtl::root_layout()),
        );
        central_ui.shrink_clip_rect(central_inner);
        match self.view {
            View::Reader => {
                if let Some(reader) = &mut self.reader {
                    let actions = reader.show(
                        &mut central_ui,
                        &mut self.engine,
                        &self.code,
                        &palette,
                        &mut self.settings,
                        &self.store,
                        self.library
                            .as_ref()
                            .map(|l| Path::new(&l.root_path))
                            .unwrap_or_else(|| Path::new(".")),
                    );
                    reader_actions.extend(actions);
                } else {
                    self.view = View::Library;
                }
            }
            View::Library => self.show_library(&mut central_ui, &palette),
            View::Search => self.show_search(&mut central_ui, &palette),
            View::Highlights => self.show_highlights(&mut central_ui, &palette),
            View::Flashcards => self.show_flashcards(&mut central_ui, &palette),
            View::Study => self.show_study(&mut central_ui, &palette),
            View::Roadmaps => self.show_roadmaps(&mut central_ui, &palette),
            View::Settings => self.show_settings(&mut central_ui, &palette),
        }

        self.handle_reader_actions(reader_actions, &ctx);
        self.show_overlays(&ctx);
        let size = ctx.content_rect().size();
        if (size.x - self.settings.window.width).abs() > 2.0 {
            self.settings.window.width = size.x;
            self.settings.window.height = size.y;
            self.mark_dirty();
        }
        self.save_settings(false);
        if self.jobs.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(mut reader) = self.reader.take() {
            reader.finish_session(&self.store);
        }
        self.save_settings(true);
    }
}

// ----------------------------------------------------------------------------------------
// Small helpers
// ----------------------------------------------------------------------------------------

fn load_settings(path: &Path) -> AppSettings {
    match std::fs::read_to_string(path) {
        Ok(text) => match AppSettings::from_toml(&text) {
            Ok(settings) => settings,
            Err(error) => {
                log::warn!("could not parse settings, using defaults: {error}");
                AppSettings::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
        Err(error) => {
            log::warn!("could not read settings: {error}");
            AppSettings::default()
        }
    }
}

fn library_display_name(root: &Path) -> String {
    root.file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.trim().is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| crate::i18n::t("My Library").to_string())
}

fn startup_view(value: &str) -> View {
    match value {
        "search" => View::Search,
        "roadmaps" => View::Roadmaps,
        "settings" => View::Settings,
        _ => View::Library,
    }
}

fn parse_shortcut(combo: &str) -> Option<KeyboardShortcut> {
    let combo = combo.trim();
    if combo.is_empty() {
        return None;
    }
    let mut modifiers = Modifiers::NONE;
    let mut key_name = None;
    for part in combo.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => {
                modifiers.ctrl = true;
                modifiers.command = true;
            }
            "cmd" | "command" | "meta" => {
                modifiers.mac_cmd = true;
                modifiers.command = true;
            }
            "shift" => modifiers.shift = true,
            "alt" | "option" => modifiers.alt = true,
            "" => {}
            other => key_name = Some(other.to_string()),
        }
    }
    let key_name = key_name?;
    let key = if let Some(key) = Key::from_name(&key_name) {
        key
    } else if key_name == "=" || key_name == "plus" {
        Key::Equals
    } else if key_name == "-" {
        Key::Minus
    } else if key_name.chars().count() == 1 {
        Key::from_name(&key_name.to_ascii_uppercase())?
    } else {
        return None;
    };
    Some(KeyboardShortcut::new(modifiers, key))
}

fn shortcut(ui: &mut Ui, combo: &str) -> bool {
    let Some(shortcut) = parse_shortcut(combo) else {
        return false;
    };
    ui.input_mut(|i| i.consume_shortcut(&shortcut))
}

fn unique_import_path(directory: &Path, name: &str) -> PathBuf {
    let path = Path::new(name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("imported");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    for n in 2..1000 {
        let candidate = directory.join(format!("{stem} (imported {n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}{ext}", uuid::Uuid::new_v4()))
}

fn unique_directory(root: &Path, name: &str) -> PathBuf {
    let candidate = root.join(name);
    if !candidate.exists() {
        return candidate;
    }
    for n in 2..1000 {
        let candidate = root.join(format!("{name} (imported {n})"));
        if !candidate.exists() {
            return candidate;
        }
    }
    root.join(format!("{name}-{}", uuid::Uuid::new_v4()))
}

fn copy_tree(source: &Path, target: &Path) -> Result<u64, String> {
    use walkdir::WalkDir;
    let mut count = 0u64;
    for entry in WalkDir::new(source).follow_links(false).into_iter() {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.path() == source {
            continue;
        }
        let rel = entry.path().strip_prefix(source).map_err(|e| e.to_string())?;
        let dest = target.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    Ok(count)
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn open_external(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
}
