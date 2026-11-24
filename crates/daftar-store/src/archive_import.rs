//! Merge a library database snapshot into an existing store.
//!
//! Imported rows receive fresh IDs where relationships require remapping. The caller stages
//! files on disk first and invokes this inside a single transaction so a crash cannot leave a
//! half-imported library.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use daftar_core::archive::DuplicateStrategy;

use crate::db::{Result, Store, StoreError};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ImportSummary {
    pub libraries: u64,
    pub documents_imported: u64,
    pub documents_skipped: u64,
    pub documents_replaced: u64,
    pub collections: u64,
    pub categories: u64,
    pub tags: u64,
    pub highlights: u64,
    pub bookmarks: u64,
    pub flashcards: u64,
    pub roadmaps: u64,
    pub warnings: Vec<String>,
}

/// Import metadata from a SQLite snapshot. Files are expected to have been staged into the
/// target library root already, with relative paths preserved. If `library_id` is provided,
/// documents are attached to that library and duplicated paths handled per `strategy`.
pub fn merge_snapshot(
    store: &Store,
    snapshot_path: &Path,
    library_id: &str,
    strategy: DuplicateStrategy,
    restore_flashcards: bool,
    restore_roadmaps: bool,
    path_remap: &HashMap<String, String>,
) -> Result<ImportSummary> {
    let source = Connection::open_with_flags(
        snapshot_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(StoreError::Db)?;
    // Validate it really is a Daftar database snapshot.
    let version: i64 = source
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(StoreError::Db)?;
    if version <= 0 || version as usize > crate::db::MIGRATIONS.len() {
        return Err(StoreError::Data(format!(
            "Unsupported library database schema version {version}."
        )));
    }
    let mut summary = ImportSummary::default();
    let tx = store.conn.unchecked_transaction()?;

    // Map source category/collection IDs to target IDs (created or matched by name).
    let mut category_map: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = source
            .prepare("SELECT id, name, color, sort_order FROM categories")
            .map_err(StoreError::Db)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i32>(3)?,
                ))
            })
            .map_err(StoreError::Db)?;
        for row in rows {
            let (old_id, name, color, sort_order) = row.map_err(StoreError::Db)?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM categories WHERE library_id=?1 AND name=?2 COLLATE NOCASE",
                    params![library_id, name],
                    |r| r.get(0),
                )
                .ok();
            let new_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if category_map.values().all(|v| v != &new_id) {
                tx.execute(
                    "INSERT OR IGNORE INTO categories(id, library_id, name, color, sort_order)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![new_id, library_id, name, color, sort_order],
                )?;
                summary.categories += 1;
            }
            category_map.insert(old_id, new_id);
        }
    }
    let mut collection_map: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = source
            .prepare("SELECT id, name, description, color, sort_order, created_at FROM collections")
            .map_err(StoreError::Db)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .map_err(StoreError::Db)?;
        for row in rows {
            let (old_id, name, description, color, sort_order, created_at) =
                row.map_err(StoreError::Db)?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM collections WHERE library_id=?1 AND name=?2 COLLATE NOCASE",
                    params![library_id, name],
                    |r| r.get(0),
                )
                .ok();
            let new_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if !collection_map.values().any(|v| v == &new_id) {
                tx.execute(
                    "INSERT OR IGNORE INTO collections(id, library_id, name, description, color,
                        sort_order, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![new_id, library_id, name, description, color, sort_order, created_at],
                )?;
                summary.collections += 1;
            }
            collection_map.insert(old_id, new_id);
        }
    }
    let mut tag_map: HashMap<String, String> = HashMap::new();
    {
        let mut stmt = source
            .prepare("SELECT id, name, color FROM tags")
            .map_err(StoreError::Db)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(StoreError::Db)?;
        for row in rows {
            let (old_id, name, color) = row.map_err(StoreError::Db)?;
            let existing: Option<String> = tx
                .query_row(
                    "SELECT id FROM tags WHERE library_id=?1 AND name=?2 COLLATE NOCASE",
                    params![library_id, name],
                    |r| r.get(0),
                )
                .ok();
            let new_id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            if !tag_map.values().any(|v| v == &new_id) {
                tx.execute(
                    "INSERT OR IGNORE INTO tags(id, library_id, name, color) VALUES(?1,?2,?3,?4)",
                    params![new_id, library_id, name, color],
                )?;
                summary.tags += 1;
            }
            tag_map.insert(old_id, new_id);
        }
    }

    // Documents: match by relative path.
    let mut document_map: HashMap<String, String> = HashMap::new();
    let mut doc_rows = Vec::new();
    {
        let mut stmt = source
            .prepare(
                "SELECT id, collection_id, category_id, rel_path, title, direction, created_at,
                        updated_at, file_mtime_ms, file_size, content_hash, word_count,
                        reading_progress, scroll_progress, last_opened_at, is_missing
                 FROM documents",
            )
            .map_err(StoreError::Db)?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, f64>(12)?,
                    row.get::<_, f64>(13)?,
                    row.get::<_, Option<i64>>(14)?,
                    row.get::<_, i64>(15)?,
                ))
            })
            .map_err(StoreError::Db)?;
        for row in rows {
            doc_rows.push(row.map_err(StoreError::Db)?);
        }
    }
    for (
        old_id,
        collection,
        category,
        rel_path,
        title,
        direction,
        created_at,
        updated_at,
        mtime,
        size,
        hash,
        words,
        progress,
        scroll,
        last_opened,
        missing,
    ) in doc_rows
    {
        let rel_path = path_remap.get(&rel_path).cloned().unwrap_or(rel_path);
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM documents WHERE library_id=?1 AND rel_path=?2",
                params![library_id, rel_path],
                |r| r.get(0),
            )
            .ok();
        if let Some(existing_id) = existing {
            match strategy {
                DuplicateStrategy::Skip => {
                    document_map.insert(old_id, existing_id);
                    summary.documents_skipped += 1;
                    continue;
                }
                DuplicateStrategy::Replace | DuplicateStrategy::Rename => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    let target_path = if strategy == DuplicateStrategy::Rename {
                        format!("imported-{}", rel_path)
                    } else {
                        rel_path.clone()
                    };
                    if strategy == DuplicateStrategy::Replace {
                        tx.execute("DELETE FROM documents WHERE id=?1", [&existing_id])?;
                        tx.execute("DELETE FROM documents_fts WHERE doc_id=?1", [&existing_id])?;
                    }
                    tx.execute(
                        "INSERT INTO documents(id, library_id, collection_id, category_id, rel_path,
                            title, direction, created_at, updated_at, file_mtime_ms, file_size,
                            content_hash, word_count, reading_progress, scroll_progress,
                            last_opened_at, is_missing)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
                        params![
                            new_id,
                            library_id,
                            collection.and_then(|id| collection_map.get(&id).cloned()),
                            category.and_then(|id| category_map.get(&id).cloned()),
                            target_path,
                            title,
                            direction,
                            created_at,
                            updated_at,
                            mtime,
                            size,
                            hash,
                            words,
                            progress,
                            scroll,
                            last_opened,
                            missing
                        ],
                    )?;
                    document_map.insert(old_id, new_id);
                    summary.documents_imported += 1;
                    if strategy == DuplicateStrategy::Replace {
                        summary.documents_replaced += 1;
                    }
                }
            }
            continue;
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO documents(id, library_id, collection_id, category_id, rel_path, title,
                direction, created_at, updated_at, file_mtime_ms, file_size, content_hash,
                word_count, reading_progress, scroll_progress, last_opened_at, is_missing)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                new_id,
                library_id,
                collection.and_then(|id| collection_map.get(&id).cloned()),
                category.and_then(|id| category_map.get(&id).cloned()),
                rel_path,
                title,
                direction,
                created_at,
                updated_at,
                mtime,
                size,
                hash,
                words,
                progress,
                scroll,
                last_opened,
                missing
            ],
        )?;
        document_map.insert(old_id, new_id);
        summary.documents_imported += 1;
    }

    // Document tags.
    if let Ok(mut stmt) = source.prepare("SELECT document_id, tag_id FROM document_tags") {
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                if let (Some(doc), Some(tag)) = (document_map.get(&row.0), tag_map.get(&row.1)) {
                    tx.execute(
                        "INSERT OR IGNORE INTO document_tags(document_id, tag_id) VALUES(?1,?2)",
                        params![doc, tag],
                    )?;
                }
            }
        }
    }

    // Highlights and bookmarks.
    if let Ok(mut stmt) = source.prepare(
        "SELECT id, document_id, quote, prefix, suffix, start_offset, end_offset, block_index,
                color, note, tags_json, created_at, updated_at FROM highlights",
    ) {
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
            ))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                if let Some(doc) = document_map.get(&row.1) {
                    let already: bool = tx
                        .query_row(
                            "SELECT 1 FROM highlights WHERE document_id=?1 AND quote=?2
                             AND start_offset=?3 LIMIT 1",
                            params![doc, row.2, row.5],
                            |_| Ok(()),
                        )
                        .is_ok();
                    if already {
                        continue;
                    }
                    tx.execute(
                        "INSERT OR IGNORE INTO highlights(id, document_id, quote, prefix, suffix,
                            start_offset, end_offset, block_index, color, note, tags_json,
                            created_at, updated_at)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                        params![
                            uuid::Uuid::new_v4().to_string(),
                            doc,
                            row.2,
                            row.3,
                            row.4,
                            row.5,
                            row.6,
                            row.7,
                            row.8,
                            row.9,
                            row.10,
                            row.11,
                            row.12
                        ],
                    )?;
                    summary.highlights += 1;
                }
            }
        }
    }
    if let Ok(mut stmt) = source.prepare(
        "SELECT document_id, label, quote, prefix, suffix, start_offset, end_offset,
                block_index, note, created_at, updated_at FROM bookmarks",
    ) {
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
            ))
        });
        if let Ok(rows) = rows {
            for row in rows.flatten() {
                if let Some(doc) = document_map.get(&row.0) {
                    let already: bool = tx
                        .query_row(
                            "SELECT 1 FROM bookmarks WHERE document_id=?1 AND label=?2
                             AND start_offset=?3 LIMIT 1",
                            params![doc, row.1, row.5],
                            |_| Ok(()),
                        )
                        .is_ok();
                    if already {
                        continue;
                    }
                    tx.execute(
                        "INSERT INTO bookmarks(id, document_id, label, quote, prefix, suffix,
                            start_offset, end_offset, block_index, note, created_at, updated_at)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                        params![
                            uuid::Uuid::new_v4().to_string(),
                            doc,
                            row.1,
                            row.2,
                            row.3,
                            row.4,
                            row.5,
                            row.6,
                            row.7,
                            row.8,
                            row.9,
                            row.10
                        ],
                    )?;
                    summary.bookmarks += 1;
                }
            }
        }
    }

    // Flashcards.
    if restore_flashcards {
        let mut highlight_map: HashMap<String, String> = HashMap::new();
        if let Ok(mut stmt) = source.prepare("SELECT id, quote FROM highlights") {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    highlight_map.insert(row.0, row.1);
                }
            }
        }
        if let Ok(mut stmt) = source.prepare(
            "SELECT id, document_id, highlight_id, front, back, explanation, tags_json,
                    category_id, difficulty, status, created_at, updated_at, last_reviewed_at,
                    review_count FROM flashcards",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, i64>(13)?,
                ))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let doc = row.1.and_then(|d| document_map.get(&d).cloned());
                    let already: bool = tx
                        .query_row(
                            "SELECT 1 FROM flashcards WHERE front=?1 AND back=?2 LIMIT 1",
                            params![row.3, row.4],
                            |_| Ok(()),
                        )
                        .is_ok();
                    if already {
                        continue;
                    }
                    let new_id = uuid::Uuid::new_v4().to_string();
                    // Remap highlight by quote is unsafe; keep link only if a highlight with the
                    // same document and quote exists in the target.
                    let highlight_id: Option<String> = row.2.as_ref().and_then(|old| {
                        let quote = highlight_map.get(old)?;
                        let doc_id = doc.as_ref()?;
                        tx.query_row(
                            "SELECT id FROM highlights WHERE document_id=?1 AND quote=?2 LIMIT 1",
                            params![doc_id, quote],
                            |r| r.get(0),
                        )
                        .ok()
                    });
                    tx.execute(
                        "INSERT INTO flashcards(id, document_id, highlight_id, front, back,
                            explanation, tags_json, category_id, difficulty, status, created_at,
                            updated_at, last_reviewed_at, review_count)
                         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                        params![
                            new_id,
                            doc,
                            highlight_id,
                            row.3,
                            row.4,
                            row.5,
                            row.6,
                            row.7.and_then(|c| category_map.get(&c).cloned()),
                            row.8,
                            row.9,
                            row.10,
                            row.11,
                            row.12,
                            row.13
                        ],
                    )?;
                    summary.flashcards += 1;
                }
            }
        }
    }

    // Roadmaps with fresh IDs and remapped parents/documents.
    if restore_roadmaps {
        let mut roadmap_map: HashMap<String, String> = HashMap::new();
        if let Ok(mut stmt) = source.prepare(
            "SELECT id, title, description, category_id, color, created_at, updated_at FROM roadmaps",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO roadmaps(id, library_id, title, description, category_id,
                            color, created_at, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            new_id,
                            library_id,
                            row.1,
                            row.2,
                            row.3.and_then(|c| category_map.get(&c).cloned()),
                            row.4,
                            row.5,
                            row.6
                        ],
                    )?;
                    roadmap_map.insert(row.0, new_id);
                    summary.roadmaps += 1;
                }
            }
        }
        let mut node_map: HashMap<String, String> = HashMap::new();
        if let Ok(mut stmt) = source.prepare(
            "SELECT id, roadmap_id, parent_id, kind, title, notes, status, estimated_minutes,
                    actual_minutes, order_index, created_at, updated_at FROM roadmap_nodes",
        ) {
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i32>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            });
            if let Ok(rows) = rows {
                for row in rows.flatten() {
                    let Some(roadmap) = roadmap_map.get(&row.1) else { continue };
                    let new_id = uuid::Uuid::new_v4().to_string();
                    let parent = row
                        .2
                        .and_then(|p| node_map.get(&p).cloned());
                    tx.execute(
                        "INSERT INTO roadmap_nodes(id, roadmap_id, parent_id, kind, title, notes,
                            status, estimated_minutes, actual_minutes, order_index, created_at,
                            updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                        params![
                            new_id,
                            roadmap,
                            parent,
                            row.3,
                            row.4,
                            row.5,
                            row.6,
                            row.7,
                            row.8,
                            row.9,
                            row.10,
                            row.11
                        ],
                    )?;
                    if row.3 == "task" || row.3 == "milestone" {
                        summary.flashcards += 0;
                    }
                    node_map.insert(row.0, new_id);
                }
            }
        }
    }

    tx.commit()?;
    Ok(summary)
}
