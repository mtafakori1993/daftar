//! SQLite persistence for Daftar.
//!
//! Markdown stays on disk; this crate stores library metadata, study data and the full-text
//! index. All schema changes go through ordered migrations, and imports must use transactions.

pub mod archive_import;
pub mod db;
pub mod library;
pub mod roadmap;
pub mod search;
pub mod study;

pub use db::{MIGRATIONS, SCHEMA_VERSION, Store, StoreError};
pub use library::*;
pub use roadmap::RoadmapProgress;
pub use search::{SearchFilters, SearchKind};
pub use study::*;

pub fn now_ms() -> i64 {
    daftar_core::now_ms()
}

pub fn now_slug() -> String {
    daftar_core::util::timestamp_slug(now_ms())
}
