//! Library-wide search with SQLite FTS5 for document content and indexed lookups for
//! study metadata. Filters compose with the query and a result-kind restriction.

use daftar_core::model::SearchHit;
use rusqlite::types::Value;
use rusqlite::params_from_iter;
use serde::{Deserialize, Serialize};

use crate::db::{Result, Store};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Documents,
    Highlights,
    Flashcards,
    Bookmarks,
    Roadmaps,
}

impl SearchKind {
    pub fn label(self) -> &'static str {
        match self {
            SearchKind::Documents => "Documents",
            SearchKind::Highlights => "Highlights",
            SearchKind::Flashcards => "Flashcards",
            SearchKind::Bookmarks => "Bookmarks",
            SearchKind::Roadmaps => "Roadmaps",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            SearchKind::Documents => "documents",
            SearchKind::Highlights => "highlights",
            SearchKind::Flashcards => "flashcards",
            SearchKind::Bookmarks => "bookmarks",
            SearchKind::Roadmaps => "roadmaps",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub library_id: String,
    pub query: String,
    pub category_id: Option<String>,
    pub collection_id: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<SearchKind>,
    pub limit: u32,
}

impl SearchFilters {
    pub fn new(library_id: impl Into<String>, query: impl Into<String>) -> Self {
        Self {
            library_id: library_id.into(),
            query: query.into(),
            category_id: None,
            collection_id: None,
            tag: None,
            kind: None,
            limit: 60,
        }
    }
}

impl Store {
    pub fn search(&self, filters: &SearchFilters) -> Result<Vec<SearchHit>> {
        let query = filters.query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        let limit = filters.limit.clamp(1, 300) as i64;
        if filters.kind.is_none() || filters.kind == Some(SearchKind::Documents) {
            hits.extend(self.search_documents(filters, limit)?);
        }
        if filters.kind.is_none() || filters.kind == Some(SearchKind::Highlights) {
            hits.extend(self.search_highlights(filters, limit.min(30))?);
        }
        if filters.kind.is_none() || filters.kind == Some(SearchKind::Flashcards) {
            hits.extend(self.search_flashcards(filters, limit.min(30))?);
        }
        if filters.kind.is_none() || filters.kind == Some(SearchKind::Bookmarks) {
            hits.extend(self.search_bookmarks(filters, limit.min(30))?);
        }
        if filters.kind.is_none() || filters.kind == Some(SearchKind::Roadmaps) {
            hits.extend(self.search_roadmaps(filters, limit.min(20))?);
        }
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(filters.limit.clamp(1, 300) as usize);
        Ok(hits)
    }

    fn search_documents(&self, filters: &SearchFilters, limit: i64) -> Result<Vec<SearchHit>> {
        let Some(fts) = fts_query(&filters.query) else {
            return Ok(Vec::new());
        };
        let mut sql = String::from(
            "SELECT d.id, d.title, documents_fts.body, d.rel_path
             FROM documents_fts JOIN documents d ON d.id = documents_fts.doc_id
             WHERE documents_fts MATCH ?1 AND d.library_id = ?2",
        );
        let mut values: Vec<Value> = vec![Value::Text(fts), Value::Text(filters.library_id.clone())];
        push_document_filters(&mut sql, &mut values, filters, "d");
        sql.push_str(" ORDER BY bm25(documents_fts) LIMIT ?");
        values.push(Value::Integer(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let title: String = row.get(1)?;
            let body: String = row.get(2)?;
            Ok(SearchHit {
                kind: "document".to_string(),
                id: row.get(0)?,
                document_id: Some(row.get(0)?),
                title,
                snippet: make_snippet(&body, &filters.query),
                score: 1.0,
            })
        })?;
        let mut out = Vec::new();
        for (i, row) in rows.enumerate() {
            let mut hit = row?;
            hit.score = 1.0 - i as f32 * 0.0001;
            out.push(hit);
        }
        Ok(out)
    }

    fn search_highlights(&self, filters: &SearchFilters, limit: i64) -> Result<Vec<SearchHit>> {
        let pattern = like_pattern(&filters.query);
        let mut sql = String::from(
            "SELECT h.id, h.document_id, h.quote, h.note, d.title
             FROM highlights h JOIN documents d ON d.id = h.document_id
             WHERE d.library_id = ?1
               AND (h.quote LIKE ?2 ESCAPE '\\' OR h.note LIKE ?2 ESCAPE '\\'
                    OR h.tags_json LIKE ?2 ESCAPE '\\')",
        );
        let mut values: Vec<Value> = vec![
            Value::Text(filters.library_id.clone()),
            Value::Text(pattern.clone()),
        ];
        push_document_filters(&mut sql, &mut values, filters, "d");
        sql.push_str(" ORDER BY h.created_at DESC LIMIT ?");
        values.push(Value::Integer(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let quote: String = row.get(2)?;
            let note: String = row.get(3)?;
            Ok(SearchHit {
                kind: "highlight".to_string(),
                id: row.get(0)?,
                document_id: Some(row.get(1)?),
                title: row.get(4)?,
                snippet: make_snippet(&format!("{quote} {note}"), &filters.query),
                score: 0.8,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn search_flashcards(&self, filters: &SearchFilters, limit: i64) -> Result<Vec<SearchHit>> {
        let pattern = like_pattern(&filters.query);
        let requires_document = filters.collection_id.is_some() || filters.tag.is_some();
        let mut sql = String::from(
            "SELECT f.id, f.document_id, f.front, f.back, f.explanation,
                    coalesce(d.title, 'Flashcard')
             FROM flashcards f LEFT JOIN documents d ON d.id = f.document_id
             WHERE (f.front LIKE ?2 ESCAPE '\\' OR f.back LIKE ?2 ESCAPE '\\'
                    OR f.explanation LIKE ?2 ESCAPE '\\' OR f.tags_json LIKE ?2 ESCAPE '\\')",
        );
        let mut values: Vec<Value> = vec![
            Value::Text(filters.library_id.clone()),
            Value::Text(pattern),
        ];
        if requires_document {
            sql.push_str(" AND d.library_id = ?1");
        } else {
            sql.push_str(" AND (d.library_id = ?1 OR f.document_id IS NULL)");
        }
        if let Some(category) = &filters.category_id {
            sql.push_str(" AND (f.category_id = ? OR d.category_id = ?)");
            values.push(Value::Text(category.clone()));
            values.push(Value::Text(category.clone()));
        }
        if let Some(collection) = &filters.collection_id {
            sql.push_str(" AND d.collection_id = ?");
            values.push(Value::Text(collection.clone()));
        }
        if let Some(tag) = &filters.tag {
            let tag_pattern = format!("%\"{}\"%", escape_like(tag));
            sql.push_str(" AND f.tags_json LIKE ? ESCAPE '\\'");
            values.push(Value::Text(tag_pattern));
        }
        sql.push_str(" ORDER BY f.created_at DESC LIMIT ?");
        values.push(Value::Integer(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let front: String = row.get(2)?;
            let back: String = row.get(3)?;
            Ok(SearchHit {
                kind: "flashcard".to_string(),
                id: row.get(0)?,
                document_id: row.get(1)?,
                title: row.get(5)?,
                snippet: make_snippet(&format!("{front} {back}"), &filters.query),
                score: 0.7,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn search_bookmarks(&self, filters: &SearchFilters, limit: i64) -> Result<Vec<SearchHit>> {
        let pattern = like_pattern(&filters.query);
        let mut sql = String::from(
            "SELECT b.id, b.document_id, b.label, b.note, b.quote, d.title
             FROM bookmarks b JOIN documents d ON d.id = b.document_id
             WHERE d.library_id = ?1
               AND (b.label LIKE ?2 ESCAPE '\\' OR b.note LIKE ?2 ESCAPE '\\'
                    OR b.quote LIKE ?2 ESCAPE '\\')",
        );
        let mut values: Vec<Value> = vec![
            Value::Text(filters.library_id.clone()),
            Value::Text(pattern),
        ];
        push_document_filters(&mut sql, &mut values, filters, "d");
        sql.push_str(" ORDER BY b.created_at DESC LIMIT ?");
        values.push(Value::Integer(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let label: String = row.get(2)?;
            let note: String = row.get(3)?;
            Ok(SearchHit {
                kind: "bookmark".to_string(),
                id: row.get(0)?,
                document_id: Some(row.get(1)?),
                title: row.get(5)?,
                snippet: make_snippet(&format!("{label} {note}"), &filters.query),
                score: 0.6,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    fn search_roadmaps(&self, filters: &SearchFilters, limit: i64) -> Result<Vec<SearchHit>> {
        if filters.collection_id.is_some() || filters.tag.is_some() {
            return Ok(Vec::new());
        }
        let pattern = like_pattern(&filters.query);
        let mut sql = String::from(
            "SELECT r.id, r.title, r.description
             FROM roadmaps r WHERE r.library_id = ?1
               AND (r.title LIKE ?2 ESCAPE '\\' OR r.description LIKE ?2 ESCAPE '\\'
                    OR EXISTS(SELECT 1 FROM roadmap_nodes n WHERE n.roadmap_id = r.id
                        AND (n.title LIKE ?2 ESCAPE '\\' OR n.notes LIKE ?2 ESCAPE '\\')))",
        );
        let mut values: Vec<Value> = vec![
            Value::Text(filters.library_id.clone()),
            Value::Text(pattern),
        ];
        if let Some(category) = &filters.category_id {
            sql.push_str(" AND r.category_id = ?");
            values.push(Value::Text(category.clone()));
        }
        sql.push_str(" ORDER BY r.updated_at DESC LIMIT ?");
        values.push(Value::Integer(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), |row| {
            let description: String = row.get(2)?;
            Ok(SearchHit {
                kind: "roadmap".to_string(),
                id: row.get(0)?,
                document_id: None,
                title: row.get(1)?,
                snippet: make_snippet(&description, &filters.query),
                score: 0.5,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn push_document_filters(
    sql: &mut String,
    values: &mut Vec<Value>,
    filters: &SearchFilters,
    alias: &str,
) {
    if let Some(category) = &filters.category_id {
        sql.push_str(&format!(" AND {alias}.category_id = ?"));
        values.push(Value::Text(category.clone()));
    }
    if let Some(collection) = &filters.collection_id {
        sql.push_str(&format!(" AND {alias}.collection_id = ?"));
        values.push(Value::Text(collection.clone()));
    }
    if let Some(tag) = &filters.tag {
        sql.push_str(&format!(
            " AND EXISTS(SELECT 1 FROM document_tags dt JOIN tags t ON t.id=dt.tag_id
                         WHERE dt.document_id={alias}.id AND t.name = ? COLLATE NOCASE)"
        ));
        values.push(Value::Text(tag.clone()));
    }
}

/// Build a conservative FTS5 query: quoted terms with prefix matching, joined by AND.
fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .filter(|term| term.chars().any(|c| c.is_alphanumeric()))
        .take(8)
        .map(|term| {
            let cleaned = term.replace('"', "").replace('\'', "");
            format!("\"{cleaned}\"*")
        })
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" AND "))
    }
}

fn like_pattern(query: &str) -> String {
    format!("%{}%", escape_like(query.trim()))
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn make_snippet(text: &str, query: &str) -> String {
    let cleaned = text.replace(['\n', '\r'], " ");
    let lower = cleaned.to_lowercase();
    for term in query.split_whitespace() {
        let term = term.trim_matches(|c: char| !c.is_alphanumeric());
        if term.is_empty() {
            continue;
        }
        if let Some(byte_idx) = lower.find(&term.to_lowercase()) {
            let char_pos = lower[..byte_idx].chars().count();
            let chars: Vec<char> = cleaned.chars().collect();
            let start = char_pos.saturating_sub(70);
            let end = (char_pos + term.chars().count() + 110).min(chars.len());
            let mut snippet: String = chars[start..end].iter().collect();
            snippet = snippet.trim().to_string();
            if start > 0 {
                snippet.insert_str(0, "… ");
            }
            if end < chars.len() {
                snippet.push_str(" …");
            }
            return snippet;
        }
    }
    daftar_core::util::truncate_chars(cleaned.trim(), 180)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use daftar_core::model::{Category, Document, Flashcard, Highlight, Library, Roadmap, RoadmapNode, Tag};

    fn setup() -> (Store, Library, Document) {
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("L", "/tmp/L");
        store.upsert_library(&library).unwrap();
        let doc = Document::new(library.id.clone(), "rust.md", "Rust Notes");
        store.upsert_document(&doc).unwrap();
        store
            .index_document(
                &doc.id,
                "Rust Notes",
                "Ownership, borrowing and lifetimes are the core of Rust. سلام دنیا",
                "Ownership\nBorrowing",
                "rust.md",
            )
            .unwrap();
        (store, library, doc)
    }

    #[test]
    fn finds_document_content() {
        let (store, library, _doc) = setup();
        let hits = store.search(&SearchFilters::new(&library.id, "borrowing")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "document");
        assert!(hits[0].snippet.contains("borrowing"));
    }

    #[test]
    fn finds_persian_content() {
        let (store, library, _doc) = setup();
        let hits = store.search(&SearchFilters::new(&library.id, "سلام")).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("سلام"));
    }

    #[test]
    fn filters_by_tag_and_category() {
        let (store, library, doc) = setup();
        let category = Category::new(library.id.clone(), "Programming", 0);
        store.upsert_category(&category).unwrap();
        store.organize_document(&doc.id, None, Some(&category.id)).unwrap();
        store
            .set_document_tags(&doc.id, &library.id, &["systems".into()])
            .unwrap();
        let mut filters = SearchFilters::new(&library.id, "Rust");
        filters.category_id = Some(category.id.clone());
        assert_eq!(store.search(&filters).unwrap().len(), 1);
        let mut filters = SearchFilters::new(&library.id, "Rust");
        filters.tag = Some("systems".into());
        assert_eq!(store.search(&filters).unwrap().len(), 1);
        let mut filters = SearchFilters::new(&library.id, "Rust");
        filters.tag = Some("missing".into());
        assert!(store.search(&filters).unwrap().is_empty());
    }

    #[test]
    fn searches_highlights_flashcards_and_bookmarks() {
        let (store, library, doc) = setup();
        let mut highlight = Highlight::new(doc.id.clone(), "borrowed value".into(), "#FFD966");
        highlight.tags = vec!["rust".into()];
        store.upsert_highlight(&highlight).unwrap();
        let mut card = Flashcard::new("What is ownership?", "A set of rules");
        card.document_id = Some(doc.id.clone());
        card.tags = vec!["rust".into()];
        store.upsert_flashcard(&card).unwrap();
        let mut bookmark = daftar_core::model::Bookmark::new(doc.id.clone(), "Ownership chapter");
        bookmark.note = "review later".into();
        store.upsert_bookmark(&bookmark).unwrap();

        let hits = store.search(&SearchFilters::new(&library.id, "ownership")).unwrap();
        assert!(hits.iter().any(|h| h.kind == "flashcard"));
        assert!(hits.iter().any(|h| h.kind == "bookmark"));
        let mut filters = SearchFilters::new(&library.id, "ownership");
        filters.kind = Some(SearchKind::Flashcards);
        let hits = store.search(&filters).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, "flashcard");
    }

    #[test]
    fn searches_roadmaps() {
        let (store, library, _doc) = setup();
        let roadmap = Roadmap::new(library.id.clone(), "Learn Rust");
        store.upsert_roadmap(&roadmap).unwrap();
        let node = RoadmapNode::new(roadmap.id.clone(), daftar_core::model::NodeKind::Task, "Async runtimes", 0);
        store.upsert_roadmap_node(&node).unwrap();
        let hits = store.search(&SearchFilters::new(&library.id, "Async")).unwrap();
        assert!(hits.iter().any(|h| h.kind == "roadmap"));
    }

    #[test]
    fn empty_query_returns_nothing() {
        let (store, library, _doc) = setup();
        assert!(store.search(&SearchFilters::new(&library.id, "   ")).unwrap().is_empty());
    }

    #[test]
    fn fts_handles_punctuation() {
        let (store, library, _doc) = setup();
        for query in ["rust!", "\"rust\"", "rust AND", "***"] {
            let _ = store.search(&SearchFilters::new(&library.id, query)).unwrap();
        }
    }

    #[test]
    fn tags_are_searchable_in_flashcards() {
        let (store, library, doc) = setup();
        let mut card = Flashcard::new("Prompt", "Answer");
        card.document_id = Some(doc.id.clone());
        card.tags = vec!["interview".into()];
        store.upsert_flashcard(&card).unwrap();
        let hits = store.search(&SearchFilters::new(&library.id, "interview")).unwrap();
        assert!(hits.iter().any(|h| h.kind == "flashcard"));
        let _ = Tag::new(library.id.clone(), "x");
    }
}
