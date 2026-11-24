//! Highlights, bookmarks, flashcards and study/read sessions.

use daftar_core::model::{
    Bookmark, CardStatus, Difficulty, Flashcard, Highlight, Id, ReadingSession, StudySession,
};
use daftar_core::now_ms;
use rusqlite::{Row, params};

use crate::db::{Result, Store};

fn tags_from_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
}

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

const HIGHLIGHT_COLUMNS: &str = "id, document_id, quote, prefix, suffix, start_offset, end_offset,
    block_index, color, note, tags_json, created_at, updated_at";

const BOOKMARK_COLUMNS: &str = "id, document_id, label, quote, prefix, suffix, start_offset,
    end_offset, block_index, note, created_at, updated_at";

const FLASHCARD_COLUMNS: &str = "id, document_id, highlight_id, front, back, explanation,
    tags_json, category_id, difficulty, status, created_at, updated_at, last_reviewed_at,
    review_count";

impl Store {
    pub fn highlights_for_document(&self, document_id: &str) -> Result<Vec<Highlight>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {HIGHLIGHT_COLUMNS} FROM highlights WHERE document_id=?1 ORDER BY start_offset"
        ))?;
        let rows = stmt.query_map([document_id], row_to_highlight)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn all_highlights(&self, library_id: &str) -> Result<Vec<Highlight>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT h.id, h.document_id, h.quote, h.prefix, h.suffix, h.start_offset,
                    h.end_offset, h.block_index, h.color, h.note, h.tags_json, h.created_at,
                    h.updated_at
             FROM highlights h JOIN documents d ON d.id=h.document_id
             WHERE d.library_id=?1 ORDER BY h.created_at DESC"
        ))?;
        let rows = stmt.query_map([library_id], row_to_highlight)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn highlight(&self, id: &str) -> Result<Option<Highlight>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {HIGHLIGHT_COLUMNS} FROM highlights WHERE id=?1"))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_highlight(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_highlight(&self, highlight: &Highlight) -> Result<()> {
        self.conn.execute(
            "INSERT INTO highlights(id, document_id, quote, prefix, suffix, start_offset,
                end_offset, block_index, color, note, tags_json, created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(id) DO UPDATE SET quote=excluded.quote, prefix=excluded.prefix,
                suffix=excluded.suffix, start_offset=excluded.start_offset,
                end_offset=excluded.end_offset, block_index=excluded.block_index,
                color=excluded.color, note=excluded.note, tags_json=excluded.tags_json,
                updated_at=excluded.updated_at",
            params![
                highlight.id,
                highlight.document_id,
                highlight.quote,
                highlight.prefix,
                highlight.suffix,
                highlight.start_offset,
                highlight.end_offset,
                highlight.block_index,
                highlight.color,
                highlight.note,
                tags_to_json(&highlight.tags),
                highlight.created_at,
                highlight.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_highlight(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM highlights WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn highlight_count(&self, library_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM highlights h JOIN documents d ON d.id=h.document_id
             WHERE d.library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn bookmarks_for_document(&self, document_id: &str) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {BOOKMARK_COLUMNS} FROM bookmarks WHERE document_id=?1 ORDER BY start_offset"
        ))?;
        let rows = stmt.query_map([document_id], row_to_bookmark)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn all_bookmarks(&self, library_id: &str) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT b.id, b.document_id, b.label, b.quote, b.prefix, b.suffix, b.start_offset,
                    b.end_offset, b.block_index, b.note, b.created_at, b.updated_at
             FROM bookmarks b JOIN documents d ON d.id=b.document_id
             WHERE d.library_id=?1 ORDER BY b.created_at DESC"
        ))?;
        let rows = stmt.query_map([library_id], row_to_bookmark)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn upsert_bookmark(&self, bookmark: &Bookmark) -> Result<()> {
        self.conn.execute(
            "INSERT INTO bookmarks(id, document_id, label, quote, prefix, suffix, start_offset,
                end_offset, block_index, note, created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, quote=excluded.quote,
                prefix=excluded.prefix, suffix=excluded.suffix, start_offset=excluded.start_offset,
                end_offset=excluded.end_offset, block_index=excluded.block_index,
                note=excluded.note, updated_at=excluded.updated_at",
            params![
                bookmark.id,
                bookmark.document_id,
                bookmark.label,
                bookmark.quote,
                bookmark.prefix,
                bookmark.suffix,
                bookmark.start_offset,
                bookmark.end_offset,
                bookmark.block_index,
                bookmark.note,
                bookmark.created_at,
                bookmark.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_bookmark(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM bookmarks WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn flashcards(&self) -> Result<Vec<Flashcard>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FLASHCARD_COLUMNS} FROM flashcards ORDER BY created_at DESC"
        ))?;
        let rows = stmt.query_map([], row_to_flashcard)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn flashcards_for_document(&self, document_id: &str) -> Result<Vec<Flashcard>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FLASHCARD_COLUMNS} FROM flashcards WHERE document_id=?1 ORDER BY created_at DESC"
        ))?;
        let rows = stmt.query_map([document_id], row_to_flashcard)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn flashcards_by_category(&self, category_id: &str) -> Result<Vec<Flashcard>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FLASHCARD_COLUMNS} FROM flashcards WHERE category_id=?1 ORDER BY created_at DESC"
        ))?;
        let rows = stmt.query_map([category_id], row_to_flashcard)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn flashcard(&self, id: &str) -> Result<Option<Flashcard>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {FLASHCARD_COLUMNS} FROM flashcards WHERE id=?1"))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_flashcard(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_flashcard(&self, card: &Flashcard) -> Result<()> {
        self.conn.execute(
            "INSERT INTO flashcards(id, document_id, highlight_id, front, back, explanation,
                tags_json, category_id, difficulty, status, created_at, updated_at,
                last_reviewed_at, review_count)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)
             ON CONFLICT(id) DO UPDATE SET document_id=excluded.document_id,
                highlight_id=excluded.highlight_id, front=excluded.front, back=excluded.back,
                explanation=excluded.explanation, tags_json=excluded.tags_json,
                category_id=excluded.category_id, difficulty=excluded.difficulty,
                status=excluded.status, updated_at=excluded.updated_at,
                last_reviewed_at=excluded.last_reviewed_at, review_count=excluded.review_count",
            params![
                card.id,
                card.document_id,
                card.highlight_id,
                card.front,
                card.back,
                card.explanation,
                tags_to_json(&card.tags),
                card.category_id,
                card.difficulty.as_db(),
                card.status.as_db(),
                card.created_at,
                card.updated_at,
                card.last_reviewed_at,
                card.review_count,
            ],
        )?;
        Ok(())
    }

    pub fn delete_flashcard(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM flashcards WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn review_flashcard(&self, id: &str, status: CardStatus) -> Result<()> {
        self.conn.execute(
            "UPDATE flashcards SET status=?2, last_reviewed_at=?3, review_count=review_count+1,
                updated_at=?3 WHERE id=?1",
            params![id, status.as_db(), now_ms()],
        )?;
        Ok(())
    }

    pub fn set_flashcard_difficulty(&self, id: &str, difficulty: Difficulty) -> Result<()> {
        self.conn.execute(
            "UPDATE flashcards SET difficulty=?2, updated_at=?3 WHERE id=?1",
            params![id, difficulty.as_db(), now_ms()],
        )?;
        Ok(())
    }

    /// Cards matching a free-form tag (case-insensitive) and optional status.
    pub fn flashcards_by_tag(&self, tag: &str, status: Option<CardStatus>) -> Result<Vec<Flashcard>> {
        let pattern = format!("%\"{}\"%", tag.replace('"', ""));
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {FLASHCARD_COLUMNS} FROM flashcards
             WHERE tags_json LIKE ?1 COLLATE NOCASE
             ORDER BY created_at DESC"
        ))?;
        let rows = stmt.query_map([pattern], row_to_flashcard)?;
        let cards = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(match status {
            Some(status) => cards.into_iter().filter(|c| c.status == status).collect(),
            None => cards,
        })
    }

    pub fn add_reading_session(&self, session: &ReadingSession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading_sessions(id, document_id, started_at, ended_at, progress_start,
                progress_end) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                session.id,
                session.document_id,
                session.started_at,
                session.ended_at,
                session.progress_start,
                session.progress_end
            ],
        )?;
        Ok(())
    }

    pub fn reading_sessions(&self, document_id: &str, limit: u32) -> Result<Vec<ReadingSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, document_id, started_at, ended_at, progress_start, progress_end
             FROM reading_sessions WHERE document_id=?1 ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![document_id, limit], |row| {
            Ok(ReadingSession {
                id: row.get(0)?,
                document_id: row.get(1)?,
                started_at: row.get(2)?,
                ended_at: row.get(3)?,
                progress_start: row.get::<_, f64>(4)? as f32,
                progress_end: row.get::<_, f64>(5)? as f32,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn add_study_session(&self, session: &StudySession) -> Result<()> {
        self.conn.execute(
            "INSERT INTO study_sessions(id, started_at, ended_at, cards_reviewed, cards_learned,
                deck_label) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                session.id,
                session.started_at,
                session.ended_at,
                session.cards_reviewed,
                session.cards_learned,
                session.deck_label
            ],
        )?;
        Ok(())
    }

    pub fn study_sessions(&self, limit: u32) -> Result<Vec<StudySession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, cards_reviewed, cards_learned, deck_label
             FROM study_sessions ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |row| {
            Ok(StudySession {
                id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                cards_reviewed: row.get::<_, i64>(3)?.max(0) as u32,
                cards_learned: row.get::<_, i64>(4)?.max(0) as u32,
                deck_label: row.get(5)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn card_counts(&self) -> Result<(u64, u64, u64)> {
        let total: i64 = self.conn.query_row("SELECT count(*) FROM flashcards", [], |r| r.get(0))?;
        let learned: i64 = self.conn.query_row(
            "SELECT count(*) FROM flashcards WHERE status='learned'",
            [],
            |r| r.get(0),
        )?;
        let review: i64 = self.conn.query_row(
            "SELECT count(*) FROM flashcards WHERE status='review'",
            [],
            |r| r.get(0),
        )?;
        Ok((total.max(0) as u64, learned.max(0) as u64, review.max(0) as u64))
    }

    pub fn seed_flashcard_for_highlight(&self, highlight: &Highlight, back: &str) -> Result<Flashcard> {
        let mut card = Flashcard::new(highlight.quote.clone(), back.to_string());
        card.document_id = Some(highlight.document_id.clone());
        card.highlight_id = Some(highlight.id.clone());
        card.tags = highlight.tags.clone();
        card.explanation = highlight.note.clone();
        self.upsert_flashcard(&card)?;
        Ok(card)
    }
}

fn row_to_highlight(row: &Row<'_>) -> rusqlite::Result<Highlight> {
    Ok(Highlight {
        id: row.get(0)?,
        document_id: row.get(1)?,
        quote: row.get(2)?,
        prefix: row.get(3)?,
        suffix: row.get(4)?,
        start_offset: row.get(5)?,
        end_offset: row.get(6)?,
        block_index: row.get(7)?,
        color: row.get(8)?,
        note: row.get(9)?,
        tags: tags_from_json(&row.get::<_, String>(10)?),
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn row_to_bookmark(row: &Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        id: row.get(0)?,
        document_id: row.get(1)?,
        label: row.get(2)?,
        quote: row.get(3)?,
        prefix: row.get(4)?,
        suffix: row.get(5)?,
        start_offset: row.get(6)?,
        end_offset: row.get(7)?,
        block_index: row.get(8)?,
        note: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

fn row_to_flashcard(row: &Row<'_>) -> rusqlite::Result<Flashcard> {
    Ok(Flashcard {
        id: row.get(0)?,
        document_id: row.get(1)?,
        highlight_id: row.get(2)?,
        front: row.get(3)?,
        back: row.get(4)?,
        explanation: row.get(5)?,
        tags: tags_from_json(&row.get::<_, String>(6)?),
        category_id: row.get(7)?,
        difficulty: Difficulty::from_db(&row.get::<_, String>(8)?),
        status: CardStatus::from_db(&row.get::<_, String>(9)?),
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        last_reviewed_at: row.get(12)?,
        review_count: row.get::<_, i64>(13)?.max(0) as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use daftar_core::model::{Category, Document, Library};

    fn setup() -> (Store, Library, Document) {
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("L", "/tmp/L");
        store.upsert_library(&library).unwrap();
        let doc = Document::new(library.id.clone(), "a.md", "A");
        store.upsert_document(&doc).unwrap();
        (store, library, doc)
    }

    #[test]
    fn highlight_round_trip() {
        let (store, _lib, doc) = setup();
        let mut h = Highlight::new(doc.id.clone(), "important text".into(), "#FFD966");
        h.note = "note".into();
        h.tags = vec!["a".into(), "b".into()];
        h.start_offset = 10;
        h.end_offset = 24;
        store.upsert_highlight(&h).unwrap();
        let loaded = store.highlight(&h.id).unwrap().unwrap();
        assert_eq!(loaded.tags, vec!["a", "b"]);
        assert_eq!(loaded.note, "note");
        assert_eq!(store.highlights_for_document(&doc.id).unwrap().len(), 1);
        store.delete_highlight(&h.id).unwrap();
        assert!(store.highlight(&h.id).unwrap().is_none());
    }

    #[test]
    fn bookmark_round_trip() {
        let (store, _lib, doc) = setup();
        let b = Bookmark::new(doc.id.clone(), "Chapter 1");
        store.upsert_bookmark(&b).unwrap();
        assert_eq!(store.bookmarks_for_document(&doc.id).unwrap().len(), 1);
        assert_eq!(store.all_bookmarks(&doc.library_id).unwrap().len(), 1);
        store.delete_bookmark(&b.id).unwrap();
        assert!(store.all_bookmarks(&doc.library_id).unwrap().is_empty());
    }

    #[test]
    fn flashcard_review_updates_status() {
        let (store, _lib, doc) = setup();
        let mut card = Flashcard::new("front", "back");
        card.document_id = Some(doc.id.clone());
        card.tags = vec!["study".into()];
        store.upsert_flashcard(&card).unwrap();
        store.review_flashcard(&card.id, CardStatus::Learned).unwrap();
        let loaded = store.flashcard(&card.id).unwrap().unwrap();
        assert_eq!(loaded.status, CardStatus::Learned);
        assert_eq!(loaded.review_count, 1);
        assert!(loaded.last_reviewed_at.is_some());
        assert_eq!(store.flashcards_by_tag("study", None).unwrap().len(), 1);
        assert_eq!(store.card_counts().unwrap().0, 1);
    }

    #[test]
    fn highlight_seeds_flashcard() {
        let (store, _lib, doc) = setup();
        let mut h = Highlight::new(doc.id.clone(), "term".into(), "#FFD966");
        h.tags = vec!["t".into()];
        store.upsert_highlight(&h).unwrap();
        let card = store.seed_flashcard_for_highlight(&h, "definition").unwrap();
        assert_eq!(card.highlight_id.as_deref(), Some(h.id.as_str()));
        assert_eq!(card.front, "term");
    }

    #[test]
    fn sessions_round_trip() {
        let (store, _lib, doc) = setup();
        let session = ReadingSession {
            id: uuid::Uuid::new_v4().to_string(),
            document_id: doc.id.clone(),
            started_at: now_ms() - 1000,
            ended_at: now_ms(),
            progress_start: 0.1,
            progress_end: 0.2,
        };
        store.add_reading_session(&session).unwrap();
        assert_eq!(store.reading_sessions(&doc.id, 10).unwrap().len(), 1);
        let study = StudySession {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: now_ms() - 1000,
            ended_at: now_ms(),
            cards_reviewed: 4,
            cards_learned: 2,
            deck_label: "All".into(),
        };
        store.add_study_session(&study).unwrap();
        assert_eq!(store.study_sessions(10).unwrap().len(), 1);
    }

    #[test]
    fn deleting_document_cascades_study_data() {
        let (store, _lib, doc) = setup();
        let h = Highlight::new(doc.id.clone(), "x".into(), "#FFD966");
        store.upsert_highlight(&h).unwrap();
        let card = Flashcard::new("f", "b");
        store.upsert_flashcard(&card).unwrap();
        store.delete_document(&doc.id).unwrap();
        assert!(store.all_highlights(&doc.library_id).unwrap().is_empty());
        assert!(store.flashcards().unwrap().len() == 1); // flashcards survive with NULL document
    }

    #[test]
    fn category_filter_for_cards() {
        let (store, library, _doc) = setup();
        let category = Category::new(library.id.clone(), "Cat", 0);
        store.upsert_category(&category).unwrap();
        let mut card = Flashcard::new("f", "b");
        card.category_id = Some(category.id.clone());
        store.upsert_flashcard(&card).unwrap();
        assert_eq!(store.flashcards_by_category(&category.id).unwrap().len(), 1);
    }
}
