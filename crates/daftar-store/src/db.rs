//! SQLite connection management, migrations, integrity checks and backups.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 2;

/// Ordered migrations. Index + 1 is the schema version after applying it.
pub const MIGRATIONS: &[&str] = &[MIGRATION_V1, MIGRATION_V2];

/// v2: per-document reading width override (additive; existing rows keep `NULL`).
const MIGRATION_V2: &str = r#"
ALTER TABLE documents ADD COLUMN reading_width REAL;
ALTER TABLE documents ADD COLUMN reading_width_preset TEXT;
"#;

const MIGRATION_V1: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS libraries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE(library_id, name)
);
CREATE INDEX IF NOT EXISTS idx_categories_library ON categories(library_id);

CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    color TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    UNIQUE(library_id, name)
);
CREATE INDEX IF NOT EXISTS idx_collections_library ON collections(library_id);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    collection_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    rel_path TEXT NOT NULL,
    title TEXT NOT NULL,
    direction TEXT NOT NULL DEFAULT 'auto',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    file_mtime_ms INTEGER NOT NULL DEFAULT 0,
    file_size INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT NOT NULL DEFAULT '',
    word_count INTEGER NOT NULL DEFAULT 0,
    reading_progress REAL NOT NULL DEFAULT 0,
    scroll_progress REAL NOT NULL DEFAULT 0,
    last_opened_at INTEGER,
    is_missing INTEGER NOT NULL DEFAULT 0,
    UNIQUE(library_id, rel_path)
);
CREATE INDEX IF NOT EXISTS idx_documents_library ON documents(library_id);
CREATE INDEX IF NOT EXISTS idx_documents_collection ON documents(collection_id);
CREATE INDEX IF NOT EXISTS idx_documents_category ON documents(category_id);
CREATE INDEX IF NOT EXISTS idx_documents_recent ON documents(last_opened_at);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    color TEXT NOT NULL,
    UNIQUE(library_id, name)
);

CREATE TABLE IF NOT EXISTS document_tags (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY(document_id, tag_id)
);
CREATE INDEX IF NOT EXISTS idx_document_tags_tag ON document_tags(tag_id);

CREATE TABLE IF NOT EXISTS highlights (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    quote TEXT NOT NULL,
    prefix TEXT NOT NULL DEFAULT '',
    suffix TEXT NOT NULL DEFAULT '',
    start_offset INTEGER NOT NULL DEFAULT 0,
    end_offset INTEGER NOT NULL DEFAULT 0,
    block_index INTEGER,
    color TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_highlights_document ON highlights(document_id);

CREATE TABLE IF NOT EXISTS bookmarks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    quote TEXT NOT NULL DEFAULT '',
    prefix TEXT NOT NULL DEFAULT '',
    suffix TEXT NOT NULL DEFAULT '',
    start_offset INTEGER NOT NULL DEFAULT 0,
    end_offset INTEGER NOT NULL DEFAULT 0,
    block_index INTEGER,
    note TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_bookmarks_document ON bookmarks(document_id);

CREATE TABLE IF NOT EXISTS flashcards (
    id TEXT PRIMARY KEY,
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    highlight_id TEXT REFERENCES highlights(id) ON DELETE SET NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    explanation TEXT NOT NULL DEFAULT '',
    tags_json TEXT NOT NULL DEFAULT '[]',
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    difficulty TEXT NOT NULL DEFAULT 'medium',
    status TEXT NOT NULL DEFAULT 'new',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_reviewed_at INTEGER,
    review_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_flashcards_document ON flashcards(document_id);
CREATE INDEX IF NOT EXISTS idx_flashcards_category ON flashcards(category_id);

CREATE TABLE IF NOT EXISTS roadmaps (
    id TEXT PRIMARY KEY,
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category_id TEXT REFERENCES categories(id) ON DELETE SET NULL,
    color TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_roadmaps_library ON roadmaps(library_id);

CREATE TABLE IF NOT EXISTS roadmap_nodes (
    id TEXT PRIMARY KEY,
    roadmap_id TEXT NOT NULL REFERENCES roadmaps(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES roadmap_nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    notes TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'not_started',
    estimated_minutes INTEGER NOT NULL DEFAULT 0,
    actual_minutes INTEGER NOT NULL DEFAULT 0,
    order_index INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_roadmap_nodes_roadmap ON roadmap_nodes(roadmap_id);
CREATE INDEX IF NOT EXISTS idx_roadmap_nodes_parent ON roadmap_nodes(parent_id);

CREATE TABLE IF NOT EXISTS roadmap_refs (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES roadmap_nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_roadmap_refs_node ON roadmap_refs(node_id);

CREATE TABLE IF NOT EXISTS reading_sessions (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    progress_start REAL NOT NULL DEFAULT 0,
    progress_end REAL NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_reading_sessions_doc ON reading_sessions(document_id, started_at);

CREATE TABLE IF NOT EXISTS study_sessions (
    id TEXT PRIMARY KEY,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    cards_reviewed INTEGER NOT NULL DEFAULT 0,
    cards_learned INTEGER NOT NULL DEFAULT 0,
    deck_label TEXT NOT NULL DEFAULT ''
);

CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
    doc_id UNINDEXED,
    title,
    body,
    headings,
    path,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS import_journal (
    id TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT ''
);
"#;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("data error: {0}")]
    Data(String),
    #[error("library error: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) path: PathBuf,
}

impl Store {
    /// Open (or create) a store and apply migrations. Corrupt databases are quarantined and a
    /// backup is restored automatically when one is available.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match Self::open_once(&path) {
            Ok(store) => Ok(store),
            Err(first_error) => {
                log::error!("could not open database {}: {first_error}", path.display());
                let quarantine = path.with_extension(format!("corrupt-{}.db", crate::now_slug()));
                if path.exists() {
                    let _ = std::fs::rename(&path, &quarantine);
                }
                let _ = std::fs::remove_file(path.with_extension("db-wal"));
                let _ = std::fs::remove_file(path.with_extension("db-shm"));
                if let Some(backup) = newest_backup(&path) {
                    log::warn!("restoring database backup {}", backup.display());
                    if std::fs::copy(&backup, &path).is_ok() {
                        if let Ok(store) = Self::open_once(&path) {
                            return Ok(store);
                        }
                    }
                }
                log::warn!("creating a fresh database after recovery failure");
                Self::open_once(&path)
            }
        }
    }

    fn open_once(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        let check: String =
            conn.pragma_query_value(None, "quick_check", |row| row.get(0))?;
        if check != "ok" {
            return Err(StoreError::Data(format!(
                "database integrity check failed: {check}"
            )));
        }
        let mut store = Self {
            conn,
            path: path.to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let mut store = Self {
            conn,
            path: PathBuf::from(":memory:"),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> Result<u32> {
        let v: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))?;
        Ok(v.max(0) as u32)
    }

    fn migrate(&mut self) -> Result<()> {
        let mut version = self.schema_version()?;
        if version as usize > MIGRATIONS.len() {
            return Err(StoreError::Data(format!(
                "database schema v{version} is newer than this app supports (v{}).",
                MIGRATIONS.len()
            )));
        }
        while (version as usize) < MIGRATIONS.len() {
            let tx = self.conn.transaction()?;
            tx.execute_batch(MIGRATIONS[version as usize])?;
            version += 1;
            tx.pragma_update(None, "user_version", version as i64)?;
            tx.execute(
                "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [version.to_string()],
            )?;
            tx.commit()?;
            log::info!("applied database migration to v{version}");
        }
        Ok(())
    }

    /// Create a consistent snapshot with `VACUUM INTO`. Returns the snapshot path.
    pub fn backup_to(&self, directory: impl AsRef<Path>) -> Result<PathBuf> {
        let directory = directory.as_ref();
        std::fs::create_dir_all(directory)?;
        let stem = format!("daftar-{}", crate::now_slug());
        let mut target = directory.join(format!("{stem}.db"));
        let mut counter = 2;
        while target.exists() {
            target = directory.join(format!("{stem}-{counter}.db"));
            counter += 1;
        }
        let target_str = target.to_string_lossy().to_string();
        self.conn
            .execute("VACUUM INTO ?1", rusqlite::params![target_str])?;
        Ok(target)
    }

    pub fn prune_backups(directory: impl AsRef<Path>, keep: u32) -> Result<Vec<PathBuf>> {
        let directory = directory.as_ref();
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("db") {
                    if let Ok(meta) = entry.metadata() {
                        if let Ok(modified) = meta.modified() {
                            files.push((modified, path));
                        }
                    }
                }
            }
        }
        files.sort_by_key(|(t, _)| *t);
        let keep = keep.max(1) as usize;
        let mut removed = Vec::new();
        while files.len() > keep {
            let (_, path) = files.remove(0);
            if std::fs::remove_file(&path).is_ok() {
                removed.push(path);
            }
        }
        Ok(removed)
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn meta_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn journal_import(&self, kind: &str, detail: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO import_journal(id, created_at, kind, detail) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(),
                crate::now_ms(),
                kind,
                detail
            ],
        )?;
        Ok(())
    }

    pub fn integrity_check(&self) -> Result<String> {
        let value: String = self
            .conn
            .pragma_query_value(None, "integrity_check", |row| row.get(0))?;
        Ok(value)
    }
}

fn newest_backup(db_path: &Path) -> Option<PathBuf> {
    let dir = db_path.parent()?.join("backups");
    let mut candidates: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("db") {
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                candidates.push((modified, path));
            }
        }
    }
    candidates.sort_by_key(|(t, _)| *t);
    candidates.pop().map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_schema_and_reports_version() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        assert_eq!(store.integrity_check().unwrap(), "ok");
    }

    #[test]
    fn upgrades_a_v1_database_and_keeps_its_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        // Build a v1 database by hand: schema v1 plus a document row.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATION_V1).unwrap();
            conn.pragma_update(None, "user_version", 1i64).unwrap();
            conn.execute(
                "INSERT INTO libraries(id, name, root_path, created_at, updated_at)
                 VALUES('lib','L','/tmp',0,0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO documents(id, library_id, rel_path, title, direction, created_at,
                    updated_at, file_mtime_ms, file_size, content_hash, word_count,
                    reading_progress, scroll_progress, is_missing)
                 VALUES('doc','lib','a.md','A','auto',0,0,0,0,'',0,0,0,0)",
                [],
            )
            .unwrap();
        }
        // Opening the store migrates to the current schema without touching existing rows.
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        let document = store.document("doc").unwrap().unwrap();
        assert_eq!(document.title, "A");
        assert_eq!(document.reading_width, None);
        assert_eq!(document.reading_width_preset, None);
        // The new override can be written and read back on the migrated database.
        store
            .set_document_reading_width("doc", Some(daftar_core::settings::WidthPreset::Narrow), None)
            .unwrap();
        let document = store.document("doc").unwrap().unwrap();
        assert_eq!(
            document.reading_width_preset,
            Some(daftar_core::settings::WidthPreset::Narrow)
        );
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        {
            let store = Store::open(&path).unwrap();
            assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn fts5_is_available() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .execute(
                "INSERT INTO documents_fts(doc_id, title, body, headings, path) VALUES('x','t','سلام دنیا','h','p')",
                [],
            )
            .unwrap();
        let count: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM documents_fts WHERE documents_fts MATCH 'سلام'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn backup_creates_and_prunes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("lib.db")).unwrap();
        let backups = dir.path().join("backups");
        let first = store.backup_to(&backups).unwrap();
        assert!(first.exists());
        // VACUUM INTO needs distinct names.
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second = store.backup_to(&backups).unwrap();
        assert!(second.exists());
        let removed = Store::prune_backups(&backups, 1).unwrap();
        assert_eq!(removed.len(), 1);
    }

    #[test]
    fn meta_round_trip() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.meta_get("x").unwrap().is_none());
        store.meta_set("x", "y").unwrap();
        assert_eq!(store.meta_get("x").unwrap().as_deref(), Some("y"));
    }
}