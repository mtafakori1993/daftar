//! Study roadmaps: sections, tasks, milestones, references and progress.

use daftar_core::model::{Id, Roadmap, RoadmapNode, RoadmapRef, TaskStatus};
use daftar_core::now_ms;
use rusqlite::{Row, params};
use serde::{Deserialize, Serialize};

use crate::db::{Result, Store};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RoadmapProgress {
    pub tasks: u64,
    pub completed: u64,
    pub in_progress: u64,
    pub not_started: u64,
    pub skipped: u64,
    pub percent: f32,
    pub estimated_minutes: u64,
    pub actual_minutes: u64,
}

const NODE_COLUMNS: &str = "id, roadmap_id, parent_id, kind, title, notes, status,
    estimated_minutes, actual_minutes, order_index, created_at, updated_at";

const REF_COLUMNS: &str = "id, node_id, kind, target, label";

impl Store {
    pub fn upsert_roadmap(&self, roadmap: &Roadmap) -> Result<()> {
        self.conn.execute(
            "INSERT INTO roadmaps(id, library_id, title, description, category_id, color,
                created_at, updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET title=excluded.title,
                description=excluded.description, category_id=excluded.category_id,
                color=excluded.color, updated_at=excluded.updated_at",
            params![
                roadmap.id,
                roadmap.library_id,
                roadmap.title,
                roadmap.description,
                roadmap.category_id,
                roadmap.color,
                roadmap.created_at,
                roadmap.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn roadmaps(&self, library_id: &str) -> Result<Vec<Roadmap>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, library_id, title, description, category_id, color, created_at, updated_at
             FROM roadmaps WHERE library_id=?1 ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([library_id], |row| {
            Ok(Roadmap {
                id: row.get(0)?,
                library_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                category_id: row.get(4)?,
                color: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn roadmap(&self, id: &str) -> Result<Option<Roadmap>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, library_id, title, description, category_id, color, created_at, updated_at
             FROM roadmaps WHERE id=?1",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Roadmap {
                id: row.get(0)?,
                library_id: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
                category_id: row.get(4)?,
                color: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn delete_roadmap(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM roadmaps WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn roadmap_nodes(&self, roadmap_id: &str) -> Result<Vec<RoadmapNode>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {NODE_COLUMNS} FROM roadmap_nodes WHERE roadmap_id=?1
             ORDER BY parent_id NULLS FIRST, order_index, created_at"
        ))?;
        let rows = stmt.query_map([roadmap_id], row_to_node)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn roadmap_node(&self, id: &str) -> Result<Option<RoadmapNode>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {NODE_COLUMNS} FROM roadmap_nodes WHERE id=?1"))?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row_to_node(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn upsert_roadmap_node(&self, node: &RoadmapNode) -> Result<()> {
        self.conn.execute(
            "INSERT INTO roadmap_nodes(id, roadmap_id, parent_id, kind, title, notes, status,
                estimated_minutes, actual_minutes, order_index, created_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id, kind=excluded.kind,
                title=excluded.title, notes=excluded.notes, status=excluded.status,
                estimated_minutes=excluded.estimated_minutes,
                actual_minutes=excluded.actual_minutes, order_index=excluded.order_index,
                updated_at=excluded.updated_at",
            params![
                node.id,
                node.roadmap_id,
                node.parent_id,
                node.kind.as_db(),
                node.title,
                node.notes,
                node.status.as_db(),
                node.estimated_minutes,
                node.actual_minutes,
                node.order_index,
                node.created_at,
                node.updated_at
            ],
        )?;
        self.conn.execute(
            "UPDATE roadmaps SET updated_at=?2 WHERE id=?1",
            params![node.roadmap_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn delete_roadmap_node(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM roadmap_nodes WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn set_roadmap_node_status(&self, id: &str, status: TaskStatus) -> Result<()> {
        self.conn.execute(
            "UPDATE roadmap_nodes SET status=?2, updated_at=?3 WHERE id=?1",
            params![id, status.as_db(), now_ms()],
        )?;
        Ok(())
    }

    pub fn add_actual_minutes(&self, id: &str, minutes: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE roadmap_nodes SET actual_minutes=actual_minutes+?2, updated_at=?3 WHERE id=?1",
            params![id, minutes, now_ms()],
        )?;
        Ok(())
    }

    /// Reorder siblings; `ordered_ids` are assigned increasing `order_index` values.
    pub fn reorder_roadmap_nodes(&self, ordered_ids: &[Id]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (index, id) in ordered_ids.iter().enumerate() {
            tx.execute(
                "UPDATE roadmap_nodes SET order_index=?2 WHERE id=?1",
                params![id, index as i32],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn roadmap_refs(&self, node_id: &str) -> Result<Vec<RoadmapRef>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {REF_COLUMNS} FROM roadmap_refs WHERE node_id=?1"
        ))?;
        let rows = stmt.query_map([node_id], |row| {
            Ok(RoadmapRef {
                id: row.get(0)?,
                node_id: row.get(1)?,
                kind: daftar_core::model::RefKind::from_db(&row.get::<_, String>(2)?),
                target: row.get(3)?,
                label: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn upsert_roadmap_ref(&self, reference: &RoadmapRef) -> Result<()> {
        self.conn.execute(
            "INSERT INTO roadmap_refs(id, node_id, kind, target, label)
             VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET kind=excluded.kind, target=excluded.target,
                label=excluded.label",
            params![
                reference.id,
                reference.node_id,
                reference.kind.as_db(),
                reference.target,
                reference.label
            ],
        )?;
        Ok(())
    }

    pub fn delete_roadmap_ref(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM roadmap_refs WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn roadmap_progress(&self, roadmap_id: &str) -> Result<RoadmapProgress> {
        let nodes = self.roadmap_nodes(roadmap_id)?;
        Ok(compute_progress(&nodes))
    }

    pub fn all_roadmaps_with_progress(&self, library_id: &str) -> Result<Vec<(Roadmap, RoadmapProgress)>> {
        let roadmaps = self.roadmaps(library_id)?;
        let mut out = Vec::with_capacity(roadmaps.len());
        for roadmap in roadmaps {
            let progress = self.roadmap_progress(&roadmap.id)?;
            out.push((roadmap, progress));
        }
        Ok(out)
    }
}

pub fn compute_progress(nodes: &[RoadmapNode]) -> RoadmapProgress {
    let mut progress = RoadmapProgress::default();
    let mut weighted = 0.0f32;
    for node in nodes.iter().filter(|n| n.kind != daftar_core::model::NodeKind::Section) {
        progress.tasks += 1;
        match node.status {
            TaskStatus::Completed => {
                progress.completed += 1;
                weighted += 1.0;
            }
            TaskStatus::InProgress => {
                progress.in_progress += 1;
                weighted += 0.5;
            }
            TaskStatus::Skipped => progress.skipped += 1,
            TaskStatus::NotStarted => progress.not_started += 1,
        }
        progress.estimated_minutes += node.estimated_minutes as u64;
        progress.actual_minutes += node.actual_minutes as u64;
    }
    progress.percent = if progress.tasks == 0 {
        0.0
    } else {
        (weighted / progress.tasks as f32 * 100.0).clamp(0.0, 100.0)
    };
    progress
}

fn row_to_node(row: &Row<'_>) -> rusqlite::Result<RoadmapNode> {
    Ok(RoadmapNode {
        id: row.get(0)?,
        roadmap_id: row.get(1)?,
        parent_id: row.get(2)?,
        kind: daftar_core::model::NodeKind::from_db(&row.get::<_, String>(3)?),
        title: row.get(4)?,
        notes: row.get(5)?,
        status: TaskStatus::from_db(&row.get::<_, String>(6)?),
        estimated_minutes: row.get::<_, i64>(7)?.max(0) as u32,
        actual_minutes: row.get::<_, i64>(8)?.max(0) as u32,
        order_index: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Store;
    use daftar_core::model::{Document, Library, NodeKind};

    fn setup() -> (Store, Library) {
        let store = Store::open_in_memory().unwrap();
        let library = Library::new("L", "/tmp/L");
        store.upsert_library(&library).unwrap();
        (store, library)
    }

    #[test]
    fn roadmap_and_nodes_round_trip() {
        let (store, library) = setup();
        let roadmap = Roadmap::new(library.id.clone(), "Rust");
        store.upsert_roadmap(&roadmap).unwrap();
        let mut section = RoadmapNode::new(roadmap.id.clone(), NodeKind::Section, "Basics", 0);
        store.upsert_roadmap_node(&section).unwrap();
        let mut task = RoadmapNode::new(roadmap.id.clone(), NodeKind::Task, "Ownership", 1);
        task.parent_id = Some(section.id.clone());
        task.estimated_minutes = 60;
        store.upsert_roadmap_node(&task).unwrap();
        assert_eq!(store.roadmap_nodes(&roadmap.id).unwrap().len(), 2);
        assert_eq!(store.roadmaps(&library.id).unwrap().len(), 1);
    }

    #[test]
    fn progress_weights_in_progress() {
        let (store, library) = setup();
        let roadmap = Roadmap::new(library.id.clone(), "R");
        store.upsert_roadmap(&roadmap).unwrap();
        let mut a = RoadmapNode::new(roadmap.id.clone(), NodeKind::Task, "A", 0);
        a.status = TaskStatus::Completed;
        a.estimated_minutes = 30;
        a.actual_minutes = 20;
        let mut b = RoadmapNode::new(roadmap.id.clone(), NodeKind::Task, "B", 1);
        b.status = TaskStatus::InProgress;
        let mut s = RoadmapNode::new(roadmap.id.clone(), NodeKind::Section, "S", 2);
        s.status = TaskStatus::Completed;
        store.upsert_roadmap_node(&a).unwrap();
        store.upsert_roadmap_node(&b).unwrap();
        store.upsert_roadmap_node(&s).unwrap();
        let p = store.roadmap_progress(&roadmap.id).unwrap();
        assert_eq!(p.tasks, 2);
        assert_eq!(p.completed, 1);
        assert!((p.percent - 75.0).abs() < 0.1);
        assert_eq!(p.estimated_minutes, 30);
        assert_eq!(p.actual_minutes, 20);
    }

    #[test]
    fn refs_and_reorder() {
        let (store, library) = setup();
        let roadmap = Roadmap::new(library.id.clone(), "R");
        store.upsert_roadmap(&roadmap).unwrap();
        let doc = Document::new(library.id.clone(), "a.md", "A");
        store.upsert_document(&doc).unwrap();
        let mut node = RoadmapNode::new(roadmap.id.clone(), NodeKind::Task, "T", 0);
        store.upsert_roadmap_node(&node).unwrap();
        let reference = RoadmapRef {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node.id.clone(),
            kind: daftar_core::model::RefKind::Document,
            target: doc.id.clone(),
            label: "Read".into(),
        };
        store.upsert_roadmap_ref(&reference).unwrap();
        assert_eq!(store.roadmap_refs(&node.id).unwrap().len(), 1);
        node.order_index = 4;
        store.upsert_roadmap_node(&node).unwrap();
        store.reorder_roadmap_nodes(&[node.id.clone()]).unwrap();
        assert_eq!(store.roadmap_node(&node.id).unwrap().unwrap().order_index, 0);
    }

    #[test]
    fn deleting_roadmap_cascades_nodes() {
        let (store, library) = setup();
        let roadmap = Roadmap::new(library.id.clone(), "R");
        store.upsert_roadmap(&roadmap).unwrap();
        let node = RoadmapNode::new(roadmap.id.clone(), NodeKind::Task, "T", 0);
        store.upsert_roadmap_node(&node).unwrap();
        store.delete_roadmap(&roadmap.id).unwrap();
        assert!(store.roadmap_nodes(&roadmap.id).unwrap().is_empty());
    }
}
