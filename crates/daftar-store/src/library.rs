//! Libraries, categories, collections, documents, tags and full-text indexing.

use std::collections::HashMap;

use daftar_core::direction::Direction;
use daftar_core::model::{
    Category, Collection, Document, Id, Library, LibraryStats, Tag, native_to_rel,
};
use daftar_core::now_ms;
use rusqlite::{Row, params};

use crate::db::{Result, Store, StoreError};

const DOCUMENT_COLUMNS: &str = "id, library_id, collection_id, category_id, rel_path, title, \
     direction, created_at, updated_at, file_mtime_ms, file_size, content_hash, word_count, \
     reading_progress, scroll_progress, last_opened_at, is_missing";

impl Store {
    pub fn upsert_library(&self, library: &Library) -> Result<()> {
        self.conn.execute(
            "INSERT INTO libraries(id, name, root_path, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, root_path=excluded.root_path,
                 updated_at=excluded.updated_at",
            params![
                library.id,
                library.name,
                library.root_path,
                library.created_at,
                library.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn libraries(&self) -> Result<Vec<Library>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, root_path, created_at, updated_at FROM libraries ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Library {
                id: row.get(0)?,
                name: row.get(1)?,
                root_path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn library(&self, id: &str) -> Result<Option<Library>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, root_path, created_at, updated_at FROM libraries WHERE id=?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Library {
                id: row.get(0)?,
                name: row.get(1)?,
                root_path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn library_by_root(&self, root: &str) -> Result<Option<Library>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, root_path, created_at, updated_at FROM libraries WHERE root_path=?1",
        )?;
        let mut rows = stmt.query([root])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Library {
                id: row.get(0)?,
                name: row.get(1)?,
                root_path: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_library(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM libraries WHERE id=?1", [id])?;
        self.conn
            .execute("DELETE FROM documents_fts WHERE doc_id NOT IN (SELECT id FROM documents)", [])?;
        Ok(())
    }

    pub fn categories(&self, library_id: &str) -> Result<Vec<Category>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, library_id, name, color, sort_order FROM categories
             WHERE library_id=?1 ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok(Category {
                id: row.get(0)?,
                library_id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
                sort_order: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn upsert_category(&self, category: &Category) -> Result<()> {
        self.conn.execute(
            "INSERT INTO categories(id, library_id, name, color, sort_order)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, color=excluded.color,
                 sort_order=excluded.sort_order",
            params![
                category.id,
                category.library_id,
                category.name,
                category.color,
                category.sort_order
            ],
        )?;
        Ok(())
    }

    pub fn delete_category(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM categories WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn collections(&self, library_id: &str) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, library_id, name, description, color, sort_order, created_at
             FROM collections WHERE library_id=?1 ORDER BY sort_order, name",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok(Collection {
                id: row.get(0)?,
                library_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                color: row.get(4)?,
                sort_order: row.get(5)?,
                created_at: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn upsert_collection(&self, collection: &Collection) -> Result<()> {
        self.conn.execute(
            "INSERT INTO collections(id, library_id, name, description, color, sort_order, created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description,
                 color=excluded.color, sort_order=excluded.sort_order",
            params![
                collection.id,
                collection.library_id,
                collection.name,
                collection.description,
                collection.color,
                collection.sort_order,
                collection.created_at
            ],
        )?;
        Ok(())
    }

    pub fn delete_collection(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM collections WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn upsert_document(&self, document: &Document) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO documents({DOCUMENT_COLUMNS})
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)
                 ON CONFLICT(id) DO UPDATE SET
                    collection_id=excluded.collection_id,
                    category_id=excluded.category_id,
                    rel_path=excluded.rel_path,
                    title=excluded.title,
                    direction=excluded.direction,
                    updated_at=excluded.updated_at,
                    file_mtime_ms=excluded.file_mtime_ms,
                    file_size=excluded.file_size,
                    content_hash=excluded.content_hash,
                    word_count=excluded.word_count,
                    reading_progress=excluded.reading_progress,
                    scroll_progress=excluded.scroll_progress,
                    last_opened_at=excluded.last_opened_at,
                    is_missing=excluded.is_missing"
            ),
            params![
                document.id,
                document.library_id,
                document.collection_id,
                document.category_id,
                document.rel_path,
                document.title,
                document.direction.as_db(),
                document.created_at,
                document.updated_at,
                document.file_mtime_ms,
                document.file_size as i64,
                document.content_hash,
                document.word_count as i64,
                document.reading_progress,
                document.scroll_progress,
                document.last_opened_at,
                document.is_missing as i64,
            ],
        )?;
        Ok(())
    }

    pub fn documents(&self, library_id: &str) -> Result<Vec<Document>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE library_id=?1 ORDER BY title COLLATE NOCASE"
        ))?;
        let rows = stmt.query_map([library_id], row_to_document)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn document(&self, id: &str) -> Result<Option<Document>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {DOCUMENT_COLUMNS} FROM documents WHERE id=?1"))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn document_by_rel_path(&self, library_id: &str, rel_path: &str) -> Result<Option<Document>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents WHERE library_id=?1 AND rel_path=?2"
        ))?;
        let mut rows = stmt.query(params![library_id, rel_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_document(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn recent_documents(&self, library_id: &str, limit: u32) -> Result<Vec<Document>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {DOCUMENT_COLUMNS} FROM documents
             WHERE library_id=?1 AND last_opened_at IS NOT NULL
             ORDER BY last_opened_at DESC LIMIT ?2"
        ))?;
        let rows = stmt.query_map(params![library_id, limit], row_to_document)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn delete_document(&self, id: &str) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM documents_fts WHERE doc_id=?1", [id])?;
        tx.execute("DELETE FROM documents WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_document_missing(&self, id: &str, missing: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET is_missing=?2, updated_at=?3 WHERE id=?1",
            params![id, missing as i64, now_ms()],
        )?;
        Ok(())
    }

    pub fn set_document_direction(&self, id: &str, direction: Direction) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET direction=?2, updated_at=?3 WHERE id=?1",
            params![id, direction.as_db(), now_ms()],
        )?;
        Ok(())
    }

    pub fn update_document_title(&self, id: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET title=?2, updated_at=?3 WHERE id=?1",
            params![id, title, now_ms()],
        )?;
        Ok(())
    }

    pub fn organize_document(
        &self,
        id: &str,
        collection_id: Option<&str>,
        category_id: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET collection_id=?2, category_id=?3, updated_at=?4 WHERE id=?1",
            params![id, collection_id, category_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn update_reading_progress(&self, id: &str, progress: f32, scroll: f32) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET reading_progress=?2, scroll_progress=?3, last_opened_at=?4,
                updated_at=?5 WHERE id=?1",
            params![
                id,
                progress.clamp(0.0, 1.0),
                scroll.clamp(0.0, 1.0),
                now_ms(),
                now_ms()
            ],
        )?;
        Ok(())
    }

    pub fn touch_document(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET last_opened_at=?2 WHERE id=?1",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    /// Update file metadata without touching library-assigned fields.
    pub fn update_file_metadata(
        &self,
        id: &str,
        mtime_ms: i64,
        size: u64,
        hash: &str,
        words: u64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE documents SET file_mtime_ms=?2, file_size=?3, content_hash=?4,
                 word_count=?5, updated_at=?6 WHERE id=?1",
            params![id, mtime_ms, size as i64, hash, words as i64, now_ms()],
        )?;
        Ok(())
    }

    pub fn index_document(
        &self,
        doc_id: &str,
        title: &str,
        body: &str,
        headings: &str,
        rel_path: &str,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM documents_fts WHERE doc_id=?1", [doc_id])?;
        tx.execute(
            "INSERT INTO documents_fts(doc_id, title, body, headings, path)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![doc_id, title, body, headings, rel_path],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_from_index(&self, doc_id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM documents_fts WHERE doc_id=?1", [doc_id])?;
        Ok(())
    }

    pub fn indexed_body(&self, doc_id: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT body FROM documents_fts WHERE doc_id=?1 LIMIT 1")?;
        let mut rows = stmt.query([doc_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn tags(&self, library_id: &str) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, library_id, name, color FROM tags WHERE library_id=?1 ORDER BY name",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                library_id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn find_or_create_tag(&self, library_id: &str, name: &str) -> Result<Tag> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Data("Tag name cannot be empty.".into()));
        }
        if let Some(tag) = self.tags(library_id)?.into_iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
            return Ok(tag);
        }
        let tag = Tag::new(library_id.to_string(), name);
        self.conn.execute(
            "INSERT INTO tags(id, library_id, name, color) VALUES(?1,?2,?3,?4)",
            params![tag.id, tag.library_id, tag.name, tag.color],
        )?;
        Ok(tag)
    }

    pub fn set_document_tags(&self, document_id: &str, library_id: &str, names: &[String]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM document_tags WHERE document_id=?1", [document_id])?;
        for name in names {
            if name.trim().is_empty() {
                continue;
            }
            let tag_id: String = match tx.query_row(
                "SELECT id FROM tags WHERE library_id=?1 AND name=?2 COLLATE NOCASE",
                params![library_id, name.trim()],
                |row| row.get(0),
            ) {
                Ok(id) => id,
                Err(rusqlite::Error::QueryReturnedNoRows) => {
                    let tag = Tag::new(library_id.to_string(), name.trim());
                    tx.execute(
                        "INSERT INTO tags(id, library_id, name, color) VALUES(?1,?2,?3,?4)",
                        params![tag.id, tag.library_id, tag.name, tag.color],
                    )?;
                    tag.id
                }
                Err(e) => return Err(e.into()),
            };
            tx.execute(
                "INSERT OR IGNORE INTO document_tags(document_id, tag_id) VALUES(?1,?2)",
                params![document_id, tag_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn document_tags(&self, document_id: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name FROM tags t JOIN document_tags dt ON dt.tag_id=t.id
             WHERE dt.document_id=?1 ORDER BY t.name",
        )?;
        let rows = stmt.query_map([document_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn all_document_tags(&self, library_id: &str) -> Result<HashMap<Id, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT dt.document_id, t.name FROM document_tags dt
             JOIN tags t ON t.id=dt.tag_id
             JOIN documents d ON d.id=dt.document_id
             WHERE d.library_id=?1 ORDER BY t.name",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut map: HashMap<Id, Vec<String>> = HashMap::new();
        for row in rows {
            let (doc, tag) = row?;
            map.entry(doc).or_default().push(tag);
        }
        Ok(map)
    }

    pub fn tag_counts(&self, library_id: &str) -> Result<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name, count(dt.document_id) FROM tags t
             LEFT JOIN document_tags dt ON dt.tag_id=t.id
             WHERE t.library_id=?1 GROUP BY t.id ORDER BY count(dt.document_id) DESC, t.name",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn stats(&self, library_id: &str) -> Result<LibraryStats> {
        let documents: i64 = self.conn.query_row(
            "SELECT count(*) FROM documents WHERE library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let collections: i64 = self.conn.query_row(
            "SELECT count(*) FROM collections WHERE library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let categories: i64 = self.conn.query_row(
            "SELECT count(*) FROM categories WHERE library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let highlights: i64 = self.conn.query_row(
            "SELECT count(*) FROM highlights h JOIN documents d ON d.id=h.document_id
             WHERE d.library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let bookmarks: i64 = self.conn.query_row(
            "SELECT count(*) FROM bookmarks b JOIN documents d ON d.id=b.document_id
             WHERE d.library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let flashcards: i64 = self.conn.query_row(
            "SELECT count(*) FROM flashcards",
            [],
            |r| r.get(0),
        )?;
        let roadmaps: i64 = self.conn.query_row(
            "SELECT count(*) FROM roadmaps WHERE library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        let total_words: i64 = self.conn.query_row(
            "SELECT coalesce(sum(word_count),0) FROM documents WHERE library_id=?1",
            [library_id],
            |r| r.get(0),
        )?;
        Ok(LibraryStats {
            documents: documents.max(0) as u64,
            collections: collections.max(0) as u64,
            categories: categories.max(0) as u64,
            highlights: highlights.max(0) as u64,
            bookmarks: bookmarks.max(0) as u64,
            flashcards: flashcards.max(0) as u64,
            roadmaps: roadmaps.max(0) as u64,
            total_words: total_words.max(0) as u64,
        })
    }

    /// Convert an absolute path to the stored slash-separated relative path.
    pub fn relative_path(&self, library_root: &std::path::Path, path: &std::path::Path) -> String {
        path.strip_prefix(library_root)
            .map(native_to_rel)
            .unwrap_or_else(|_| native_to_rel(path))
    }
}

fn row_to_document(row: &Row<'_>) -> rusqlite::Result<Document> {
    Ok(Document {
        id: row.get(0)?,
        library_id: row.get(1)?,
        collection_id: row.get(2)?,
        category_id: row.get(3)?,
        rel_path: row.get(4)?,
        title: row.get(5)?,
        direction: Direction::from_db(&row.get::<_, String>(6)?),
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        file_mtime_ms: row.get(9)?,
        file_size: row.get::<_, i64>(10)?.max(0) as u64,
        content_hash: row.get(11)?,
        word_count: row.get::<_, i64>(12)?.max(0) as u64,
        reading_progress: row.get::<_, f64>(13).unwrap_or(0.0) as f32,
        scroll_progress: row.get::<_, f64>(14).unwrap_or(0.0) as f32,
        last_opened_at: row.get(15)?,
        is_missing: row.get::<_, i64>(16).unwrap_or(0) != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;

    fn store_with_library() -> (Store, Library) {
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("Test", "/tmp/test-library");
        store.upsert_library(&library).unwrap();
        (store, library)
    }

    #[test]
    fn library_round_trip() {
        let (store, library) = store_with_library();
        let found = store.library(&library.id).unwrap().unwrap();
        assert_eq!(found.name, "Test");
        assert_eq!(store.libraries().unwrap().len(), 1);
        assert_eq!(
            store.library_by_root("/tmp/test-library").unwrap().unwrap().id,
            library.id
        );
    }

    #[test]
    fn documents_round_trip() {
        let (store, library) = store_with_library();
        let mut doc = Document::new(library.id.clone(), "notes/a.md", "A");
        doc.word_count = 100;
        store.upsert_document(&doc).unwrap();
        let loaded = store.document(&doc.id).unwrap().unwrap();
        assert_eq!(loaded.rel_path, "notes/a.md");
        assert_eq!(loaded.word_count, 100);
        assert_eq!(
            store
                .document_by_rel_path(&library.id, "notes/a.md")
                .unwrap()
                .unwrap()
                .id,
            doc.id
        );
        store.update_reading_progress(&doc.id, 0.4, 0.2).unwrap();
        let loaded = store.document(&doc.id).unwrap().unwrap();
        assert!((loaded.reading_progress - 0.4).abs() < 0.01);
        assert!(loaded.last_opened_at.is_some());
        store.delete_document(&doc.id).unwrap();
        assert!(store.document(&doc.id).unwrap().is_none());
    }

    #[test]
    fn fts_index_and_remove() {
        let (store, library) = store_with_library();
        let doc = Document::new(library.id.clone(), "a.md", "A");
        store.upsert_document(&doc).unwrap();
        store
            .index_document(&doc.id, "A", "سلام دنیا hello", "Heading", "a.md")
            .unwrap();
        assert!(store.indexed_body(&doc.id).unwrap().unwrap().contains("سلام"));
        store.remove_from_index(&doc.id).unwrap();
        assert!(store.indexed_body(&doc.id).unwrap().is_none());
    }

    #[test]
    fn tags_round_trip() {
        let (store, library) = store_with_library();
        let doc = Document::new(library.id.clone(), "a.md", "A");
        store.upsert_document(&doc).unwrap();
        store
            .set_document_tags(&doc.id, &library.id, &["rust".into(), "study".into()])
            .unwrap();
        let tags = store.document_tags(&doc.id).unwrap();
        assert_eq!(tags, vec!["rust", "study"]);
        let counts = store.tag_counts(&library.id).unwrap();
        assert_eq!(counts.len(), 2);
        assert_eq!(store.all_document_tags(&library.id).unwrap()[&doc.id].len(), 2);
    }

    #[test]
    fn categories_and_collections() {
        let (store, library) = store_with_library();
        let mut category = Category::new(library.id.clone(), "Programming", 0);
        category.color = "#2680EB".into();
        store.upsert_category(&category).unwrap();
        let collection = Collection::new(library.id.clone(), "Reading", 0);
        store.upsert_collection(&collection).unwrap();
        assert_eq!(store.categories(&library.id).unwrap().len(), 1);
        assert_eq!(store.collections(&library.id).unwrap().len(), 1);
        assert_eq!(store.stats(&library.id).unwrap().categories, 1);
    }

    #[test]
    fn recent_documents_are_ordered() {
        let (store, library) = store_with_library();
        let mut a = Document::new(library.id.clone(), "a.md", "A");
        let mut b = Document::new(library.id.clone(), "b.md", "B");
        store.upsert_document(&a).unwrap();
        store.upsert_document(&b).unwrap();
        a.last_opened_at = Some(now_ms() - 1000);
        b.last_opened_at = Some(now_ms());
        store.upsert_document(&a).unwrap();
        store.upsert_document(&b).unwrap();
        let recent = store.recent_documents(&library.id, 10).unwrap();
        assert_eq!(recent[0].id, b.id);
    }
}
