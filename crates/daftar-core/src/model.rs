//! Persistent domain entities.
//!
//! IDs are UUID strings and timestamps are Unix milliseconds, making the same records easy to
//! serialise to SQLite, JSON manifests and archives without platform-specific types.

use serde::{Deserialize, Serialize};

use crate::direction::Direction;
use crate::util::now_ms;

pub type Id = String;

/// A library is a folder of ordinary Markdown files plus its metadata database.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Library {
    pub id: Id,
    pub name: String,
    /// Absolute path on disk. Markdown files live beneath this folder.
    pub root_path: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Library {
    pub fn new(name: impl Into<String>, root_path: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            name: name.into(),
            root_path: root_path.into(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub id: Id,
    pub library_id: Id,
    pub name: String,
    pub color: String,
    pub sort_order: i32,
}

impl Category {
    pub fn new(library_id: Id, name: impl Into<String>, sort_order: i32) -> Self {
        Self {
            id: new_id(),
            library_id,
            name: name.into(),
            color: "#6E6E6E".to_string(),
            sort_order,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Collection {
    pub id: Id,
    pub library_id: Id,
    pub name: String,
    pub description: String,
    pub color: String,
    pub sort_order: i32,
    pub created_at: i64,
}

impl Collection {
    pub fn new(library_id: Id, name: impl Into<String>, sort_order: i32) -> Self {
        Self {
            id: new_id(),
            library_id,
            name: name.into(),
            description: String::new(),
            color: "#2680EB".to_string(),
            sort_order,
            created_at: now_ms(),
        }
    }
}

/// A Markdown document in the library. The authoritative content stays in `rel_path`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub id: Id,
    pub library_id: Id,
    pub collection_id: Option<Id>,
    pub category_id: Option<Id>,
    /// Path relative to the library root, always using `/` separators in storage.
    pub rel_path: String,
    pub title: String,
    pub direction: Direction,
    pub created_at: i64,
    pub updated_at: i64,
    pub file_mtime_ms: i64,
    pub file_size: u64,
    pub content_hash: String,
    pub word_count: u64,
    /// 0.0 to 1.0.
    pub reading_progress: f32,
    /// Vertical scroll fraction inside the document reader.
    pub scroll_progress: f32,
    pub last_opened_at: Option<i64>,
    pub is_missing: bool,
}

impl Document {
    pub fn new(library_id: Id, rel_path: impl Into<String>, title: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            library_id,
            collection_id: None,
            category_id: None,
            rel_path: rel_path.into(),
            title: title.into(),
            direction: Direction::Auto,
            created_at: now,
            updated_at: now,
            file_mtime_ms: 0,
            file_size: 0,
            content_hash: String::new(),
            word_count: 0,
            reading_progress: 0.0,
            scroll_progress: 0.0,
            last_opened_at: None,
            is_missing: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tag {
    pub id: Id,
    pub library_id: Id,
    pub name: String,
    pub color: String,
}

impl Tag {
    pub fn new(library_id: Id, name: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            library_id,
            name: name.into(),
            color: "#747474".to_string(),
        }
    }
}

/// A persistent text highlight. Anchors keep both offsets (fast path) and surrounding
/// context (recovery path after the Markdown file changes).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Highlight {
    pub id: Id,
    pub document_id: Id,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub start_offset: i64,
    pub end_offset: i64,
    pub block_index: Option<i64>,
    pub color: String,
    pub note: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Highlight {
    pub fn new(document_id: Id, quote: String, color: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            document_id,
            quote,
            prefix: String::new(),
            suffix: String::new(),
            start_offset: 0,
            end_offset: 0,
            block_index: None,
            color: color.into(),
            note: String::new(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn anchor(&self) -> crate::anchor::TextAnchor {
        crate::anchor::TextAnchor {
            quote: self.quote.clone(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
            start: self.start_offset.max(0) as usize,
            end: self.end_offset.max(0) as usize,
            block_index: self.block_index.map(|v| v.max(0) as usize),
        }
    }
}

/// A named position inside a document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    pub id: Id,
    pub document_id: Id,
    pub label: String,
    pub quote: String,
    pub prefix: String,
    pub suffix: String,
    pub start_offset: i64,
    pub end_offset: i64,
    pub block_index: Option<i64>,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Bookmark {
    pub fn new(document_id: Id, label: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            document_id,
            label: label.into(),
            quote: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            start_offset: 0,
            end_offset: 0,
            block_index: None,
            note: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn anchor(&self) -> crate::anchor::TextAnchor {
        crate::anchor::TextAnchor {
            quote: self.quote.clone(),
            prefix: self.prefix.clone(),
            suffix: self.suffix.clone(),
            start: self.start_offset.max(0) as usize,
            end: self.end_offset.max(0) as usize,
            block_index: self.block_index.map(|v| v.max(0) as usize),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Difficulty {
    #[default]
    Easy,
    Medium,
    Hard,
}

impl Difficulty {
    pub fn label(self) -> &'static str {
        match self {
            Difficulty::Easy => "Easy",
            Difficulty::Medium => "Medium",
            Difficulty::Hard => "Hard",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Medium => "medium",
            Difficulty::Hard => "hard",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "medium" => Difficulty::Medium,
            "hard" => Difficulty::Hard,
            _ => Difficulty::Easy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    #[default]
    New,
    Learning,
    Review,
    Learned,
}

impl CardStatus {
    pub fn label(self) -> &'static str {
        match self {
            CardStatus::New => "New",
            CardStatus::Learning => "Learning",
            CardStatus::Review => "Needs review",
            CardStatus::Learned => "Learned",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            CardStatus::New => "new",
            CardStatus::Learning => "learning",
            CardStatus::Review => "review",
            CardStatus::Learned => "learned",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "learning" => CardStatus::Learning,
            "review" => CardStatus::Review,
            "learned" => CardStatus::Learned,
            _ => CardStatus::New,
        }
    }
}

/// A study card created manually or from a highlight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Flashcard {
    pub id: Id,
    pub document_id: Option<Id>,
    pub highlight_id: Option<Id>,
    pub front: String,
    pub back: String,
    pub explanation: String,
    pub tags: Vec<String>,
    pub category_id: Option<Id>,
    pub difficulty: Difficulty,
    pub status: CardStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_reviewed_at: Option<i64>,
    pub review_count: u32,
}

impl Flashcard {
    pub fn new(front: impl Into<String>, back: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            document_id: None,
            highlight_id: None,
            front: front.into(),
            back: back.into(),
            explanation: String::new(),
            tags: Vec::new(),
            category_id: None,
            difficulty: Difficulty::Medium,
            status: CardStatus::New,
            created_at: now,
            updated_at: now,
            last_reviewed_at: None,
            review_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Roadmap {
    pub id: Id,
    pub library_id: Id,
    pub title: String,
    pub description: String,
    pub category_id: Option<Id>,
    pub color: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Roadmap {
    pub fn new(library_id: Id, title: impl Into<String>) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            library_id,
            title: title.into(),
            description: String::new(),
            category_id: None,
            color: "#2680EB".to_string(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    #[default]
    NotStarted,
    InProgress,
    Completed,
    Skipped,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::NotStarted => "Not started",
            TaskStatus::InProgress => "In progress",
            TaskStatus::Completed => "Completed",
            TaskStatus::Skipped => "Skipped",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            TaskStatus::NotStarted => "not_started",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Skipped => "skipped",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "in_progress" => TaskStatus::InProgress,
            "completed" => TaskStatus::Completed,
            "skipped" => TaskStatus::Skipped,
            _ => TaskStatus::NotStarted,
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            TaskStatus::NotStarted => TaskStatus::InProgress,
            TaskStatus::InProgress => TaskStatus::Completed,
            TaskStatus::Completed => TaskStatus::Skipped,
            TaskStatus::Skipped => TaskStatus::NotStarted,
        }
    }

    /// Fraction used for progress roll-ups.
    pub fn progress_weight(self) -> f32 {
        match self {
            TaskStatus::Completed => 1.0,
            TaskStatus::InProgress => 0.5,
            TaskStatus::Skipped => 0.0,
            TaskStatus::NotStarted => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Section,
    Task,
    Milestone,
}

impl NodeKind {
    pub fn label(self) -> &'static str {
        match self {
            NodeKind::Section => "Section",
            NodeKind::Task => "Task",
            NodeKind::Milestone => "Milestone",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            NodeKind::Section => "section",
            NodeKind::Task => "task",
            NodeKind::Milestone => "milestone",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "task" => NodeKind::Task,
            "milestone" => NodeKind::Milestone,
            _ => NodeKind::Section,
        }
    }
}

/// A roadmap section, learning task or milestone. The tree is stored as parent pointers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoadmapNode {
    pub id: Id,
    pub roadmap_id: Id,
    pub parent_id: Option<Id>,
    pub kind: NodeKind,
    pub title: String,
    pub notes: String,
    pub status: TaskStatus,
    pub estimated_minutes: u32,
    pub actual_minutes: u32,
    pub order_index: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl RoadmapNode {
    pub fn new(roadmap_id: Id, kind: NodeKind, title: impl Into<String>, order_index: i32) -> Self {
        let now = now_ms();
        Self {
            id: new_id(),
            roadmap_id,
            parent_id: None,
            kind,
            title: title.into(),
            notes: String::new(),
            status: TaskStatus::NotStarted,
            estimated_minutes: 0,
            actual_minutes: 0,
            order_index,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    Document,
    Url,
    Note,
}

impl RefKind {
    pub fn label(self) -> &'static str {
        match self {
            RefKind::Document => "Document",
            RefKind::Url => "Link",
            RefKind::Note => "Note",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            RefKind::Document => "document",
            RefKind::Url => "url",
            RefKind::Note => "note",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "url" => RefKind::Url,
            "note" => RefKind::Note,
            _ => RefKind::Document,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoadmapRef {
    pub id: Id,
    pub node_id: Id,
    pub kind: RefKind,
    /// Document id, URL, or note text.
    pub target: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingSession {
    pub id: Id,
    pub document_id: Id,
    pub started_at: i64,
    pub ended_at: i64,
    pub progress_start: f32,
    pub progress_end: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StudySession {
    pub id: Id,
    pub started_at: i64,
    pub ended_at: i64,
    pub cards_reviewed: u32,
    pub cards_learned: u32,
    pub deck_label: String,
}

/// A search hit with a context snippet, produced by SQLite FTS5 or an in-memory fallback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchHit {
    /// `document`, `highlight`, `flashcard`, `bookmark`, `roadmap`.
    pub kind: String,
    pub id: Id,
    pub document_id: Option<Id>,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

/// Summary used by the library home screen.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LibraryStats {
    pub documents: u64,
    pub collections: u64,
    pub categories: u64,
    pub highlights: u64,
    pub bookmarks: u64,
    pub flashcards: u64,
    pub roadmaps: u64,
    pub total_words: u64,
}

fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}

/// Convert a slash-separated relative path to the native separator.
pub fn rel_to_native(rel: &str) -> String {
    rel.replace('/', std::path::MAIN_SEPARATOR_STR)
}

/// Convert a native relative path to stored `/` separators.
pub fn native_to_rel(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
        .trim_start_matches("./")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips() {
        for status in [
            TaskStatus::NotStarted,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Skipped,
        ] {
            assert_eq!(TaskStatus::from_db(status.as_db()), status);
        }
        for kind in [NodeKind::Section, NodeKind::Task, NodeKind::Milestone] {
            assert_eq!(NodeKind::from_db(kind.as_db()), kind);
        }
    }

    #[test]
    fn native_rel_paths_are_slash_separated() {
        let p = std::path::Path::new("notes").join("nested").join("a.md");
        assert_eq!(native_to_rel(&p), "notes/nested/a.md");
    }

    #[test]
    fn timestamp_is_sane() {
        assert!(now_ms() > 1_700_000_000_000);
    }
}