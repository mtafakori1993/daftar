//! Study roadmap management with sections, tasks, milestones and progress.

use std::collections::HashMap;

use daftar_core::model::{NodeKind, Roadmap, RoadmapNode, RoadmapRef, TaskStatus, RefKind};
use daftar_core::util::format_minutes;
use daftar_store::RoadmapProgress;
use egui::Ui;

use crate::app::{DaftarApp, PromptAction, TextPrompt};
use crate::theme::Palette;
use crate::{Icon, shaped::{shape, ShapedStyle}, widgets};
use crate::rtl::DirUi;

#[derive(Debug, Clone, Default)]
pub struct RoadmapsState {
    pub roadmaps: Vec<(Roadmap, RoadmapProgress)>,
    pub selected: Option<String>,
    pub nodes: Vec<RoadmapNode>,
    pub refs: HashMap<String, Vec<RoadmapRef>>,
    pub progress: RoadmapProgress,
    pub title_edit: String,
    pub description_edit: String,
    pub focus_node: Option<String>,
    pub editor: Option<NodeEditor>,
    pub drag_node: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeEditor {
    pub node: RoadmapNode,
    pub is_new: bool,
    pub ref_url: String,
    pub ref_label: String,
    pub ref_document: Option<String>,
    pub error: Option<String>,
}

impl RoadmapsState {
    pub fn reload(&mut self, store: &daftar_store::Store, library: Option<&daftar_core::model::Library>) {
        let Some(library) = library else { return };
        self.roadmaps = store.all_roadmaps_with_progress(&library.id).unwrap_or_default();
        if let Some(id) = self.selected.clone() {
            if !self.roadmaps.iter().any(|(r, _)| r.id == id) {
                self.selected = None;
            }
        }
        if let Some(id) = self.selected.clone() {
            self.nodes = store.roadmap_nodes(&id).unwrap_or_default();
            self.progress = store.roadmap_progress(&id).unwrap_or_default();
            self.refs.clear();
            for node in &self.nodes {
                self.refs.insert(node.id.clone(), store.roadmap_refs(&node.id).unwrap_or_default());
            }
            if let Some((roadmap, _)) = self.roadmaps.iter().find(|(r, _)| r.id == id) {
                if self.title_edit.is_empty() {
                    self.title_edit = roadmap.title.clone();
                    self.description_edit = roadmap.description.clone();
                }
            }
        } else {
            self.nodes.clear();
            self.progress = RoadmapProgress::default();
        }
    }

    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
        self.title_edit.clear();
        self.description_edit.clear();
        self.focus_node = None;
    }

    /// Flatten the node tree into display order with depth.
    pub fn flat_nodes(&self) -> Vec<(usize, RoadmapNode)> {
        let mut children: HashMap<Option<String>, Vec<RoadmapNode>> = HashMap::new();
        for node in &self.nodes {
            children.entry(node.parent_id.clone()).or_default().push(node.clone());
        }
        for list in children.values_mut() {
            list.sort_by_key(|n| (n.order_index, n.created_at));
        }
        let mut out = Vec::new();
        fn walk(
            children: &HashMap<Option<String>, Vec<RoadmapNode>>,
            parent: Option<String>,
            depth: usize,
            out: &mut Vec<(usize, RoadmapNode)>,
        ) {
            if let Some(list) = children.get(&parent) {
                for node in list {
                    out.push((depth, node.clone()));
                    walk(children, Some(node.id.clone()), depth + 1, out);
                }
            }
        }
        walk(&children, None, 0, &mut out);
        out
    }
}

impl DaftarApp {
    pub(crate) fn show_roadmaps(&mut self, ui: &mut Ui, palette: &Palette) {
        self.roadmaps.reload(&self.store, self.library.as_ref());
        ui.add_space(10.0);
        if self.roadmaps.selected.is_none() {
            self.show_roadmap_list(ui, palette);
        } else {
            self.show_roadmap_editor(ui, palette);
        }
        self.show_node_editor(ui.ctx(), palette);
    }

    fn show_roadmap_list(&mut self, ui: &mut Ui, palette: &Palette) {
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::t("Study roadmaps"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, palette, "New roadmap", Some(Icon::Plus), widgets::ButtonVariant::Primary).clicked() {
                    self.prompt = Some(TextPrompt {
                        title: "New roadmap".into(),
                        label: crate::i18n::t("Roadmap title").into(),
                        value: String::new(),
                        action: PromptAction::NewRoadmap,
                    });
                }
            });
        });
        ui.label(egui::RichText::new(crate::i18n::t("Plan technical learning with sections, tasks, milestones, prerequisites and progress.")).size(11.5).color(palette.text_secondary));
        ui.add_space(8.0);
        if self.roadmaps.roadmaps.is_empty() {
            widgets::empty_state(ui, palette, Icon::Roadmap, "No roadmaps yet", "Create a roadmap for a language, framework, documentation project or research plan.");
            return;
        }
        egui::ScrollArea::vertical().id_salt("roadmaps-list").auto_shrink([false,false]).show(ui, |ui| {
            for (roadmap, progress) in self.roadmaps.roadmaps.clone() {
                egui::Frame::new().fill(palette.surface).stroke(egui::Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (title, _) = shape(&mut self.engine, &roadmap.title, &ShapedStyle::title().with_size(16.0), ui.available_width() - 160.0);
                        let colors = daftar_text::TextColors { text: palette.text, link: palette.accent, code: palette.text, code_background: None, selection: egui::Color32::TRANSPARENT };
                        let pos = ui.cursor().min;
                        self.engine.paint(ui.ctx(), ui.painter(), &title, pos, &colors, None, &[]);
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), title.height), egui::Sense::click());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button_sized(ui, palette, Icon::Trash, crate::i18n::t("Delete roadmap"), false, 26.0).clicked() {
                                self.confirm = Some(crate::app::ConfirmDialog {
                                    title: crate::i18n::t("Delete roadmap").into(),
                                    message: crate::i18n::tf("Delete `{}` and all of its tasks? This cannot be undone.", &[&roadmap.title]),
                                    confirm_label: crate::i18n::t("Delete").into(),
                                    action: crate::app::ConfirmAction::DeleteRoadmap(roadmap.id.clone()),
                                });
                            }
                            if widgets::button(ui, palette, "Open", Some(Icon::ArrowRight), widgets::ButtonVariant::Secondary).clicked() {
                                self.roadmaps.select(&roadmap.id);
                                self.roadmaps.reload(&self.store, self.library.as_ref());
                                self.roadmaps.title_edit = roadmap.title.clone();
                                self.roadmaps.description_edit = roadmap.description.clone();
                            }
                        });
                    });
                    if !roadmap.description.trim().is_empty() {
                        ui.label(egui::RichText::new(&roadmap.description).size(11.5).color(palette.text_secondary));
                    }
                    ui.add_space(6.0);
                    widgets::progress_bar(ui, palette, progress.percent / 100.0, palette.accent, 6.0);
                    ui.label(egui::RichText::new(crate::i18n::tf("{}% · {}/{} tasks · {} estimated", &[&format!("{:.0}", progress.percent), &progress.completed.to_string(), &progress.tasks.to_string(), &format_minutes(progress.estimated_minutes as u32)])).size(10.5).color(palette.text_secondary));
                });
                ui.add_space(6.0);
            }
        });
    }

    fn show_roadmap_editor(&mut self, ui: &mut Ui, palette: &Palette) {
        let Some(id) = self.roadmaps.selected.clone() else { return };
        let Some((roadmap, _)) = self.roadmaps.roadmaps.iter().find(|(r, _)| r.id == id).cloned() else {
            self.roadmaps.selected = None;
            return;
        };
        ui.horizontal(|ui| {
            if widgets::icon_button(ui, palette, Icon::ArrowLeft, "Back to roadmaps", false).clicked() {
                self.roadmaps.selected = None;
            }
            let mut title = self.roadmaps.title_edit.clone();
            if widgets::text_field(ui, palette, &mut title, crate::i18n::t("Roadmap title"), 360.0).changed() {
                self.roadmaps.title_edit = title;
            }
            if widgets::button(ui, palette, "Save", Some(Icon::Save), widgets::ButtonVariant::Secondary).clicked() {
                let mut updated = roadmap.clone();
                updated.title = self.roadmaps.title_edit.clone();
                updated.description = self.roadmaps.description_edit.clone();
                updated.updated_at = daftar_core::now_ms();
                let _ = self.store.upsert_roadmap(&updated);
                self.roadmaps.reload(&self.store, self.library.as_ref());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, palette, "Add task", Some(Icon::Plus), widgets::ButtonVariant::Primary).clicked() {
                    self.new_node(NodeKind::Task);
                }
                if widgets::button(ui, palette, "Add milestone", Some(Icon::Star), widgets::ButtonVariant::Quiet).clicked() {
                    self.new_node(NodeKind::Milestone);
                }
                if widgets::button(ui, palette, "Add section", Some(Icon::List), widgets::ButtonVariant::Quiet).clicked() {
                    self.new_node(NodeKind::Section);
                }
            });
        });
        ui.add_space(4.0);
        let mut description = self.roadmaps.description_edit.clone();
        if ui.add(egui::TextEdit::multiline(&mut description).desired_rows(2).desired_width(ui.available_width()).hint_text(crate::i18n::t("Description and learning objectives…"))).changed() {
            self.roadmaps.description_edit = description;
        }
        ui.add_space(6.0);
        widgets::progress_bar(ui, palette, self.roadmaps.progress.percent / 100.0, palette.accent, 7.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::i18n::tf("{}% complete", &[&format!("{:.0}", self.roadmaps.progress.percent)])).size(11.5).color(palette.text_secondary));
            ui.label(egui::RichText::new(crate::i18n::tf("{} completed · {} in progress · {} not started · {} skipped", &[&self.roadmaps.progress.completed.to_string(), &self.roadmaps.progress.in_progress.to_string(), &self.roadmaps.progress.not_started.to_string(), &self.roadmaps.progress.skipped.to_string()])).size(10.5).color(palette.text_secondary));
            ui.label(egui::RichText::new(crate::i18n::tf("Estimated {} · actual {}", &[&format_minutes(self.roadmaps.progress.estimated_minutes as u32), &format_minutes(self.roadmaps.progress.actual_minutes as u32)])).size(10.5).color(palette.text_secondary));
        });
        ui.separator();
        let flat = self.roadmaps.flat_nodes();
        if flat.is_empty() {
            widgets::empty_state(ui, palette, Icon::Roadmap, "Start your roadmap", "Add a section, then add tasks and milestones beneath it.");
            return;
        }
        let mut reorder: Option<(String, String)> = None;
        egui::ScrollArea::vertical().id_salt("roadmap-nodes").auto_shrink([false,false]).show(ui, |ui| {
            for (depth, node) in &flat {
                let id = node.id.clone();
                ui.horizontal(|ui| {
                    ui.add_space(*depth as f32 * 18.0);
                    let status_color = match node.status {
                        TaskStatus::Completed => palette.success,
                        TaskStatus::InProgress => palette.accent,
                        TaskStatus::Skipped => palette.text_disabled,
                        TaskStatus::NotStarted => palette.border_strong,
                    };
                    let (status_rect, status_resp) = ui.allocate_exact_size(egui::Vec2::splat(20.0), egui::Sense::click());
                    crate::icons::draw_icon(ui.painter(), Icon::Check, status_rect.shrink(3.0), status_color);
                    if status_resp.on_hover_text(crate::i18n::t("Cycle status")).clicked() {
                        let next = node.status.cycle();
                        let _ = self.store.set_roadmap_node_status(&id, next);
                        self.roadmaps.reload(&self.store, self.library.as_ref());
                    }
                    let (label, _) = shape(&mut self.engine, &node.title, &ShapedStyle::body().with_size(12.5).with_weight(if node.kind == NodeKind::Section { 600 } else { 400 }), ui.available_width() - 200.0);
                    let colors = daftar_text::TextColors { text: if node.status == TaskStatus::Skipped { palette.text_disabled } else { palette.text }, link: palette.accent, code: palette.text, code_background: None, selection: egui::Color32::TRANSPARENT };
                    let pos = ui.cursor().min;
                    self.engine.paint(ui.ctx(), ui.painter(), &label, pos, &colors, None, &[]);
                    let row = ui.allocate_exact_size(egui::vec2(ui.available_width() - 150.0, label.height), egui::Sense::click_and_drag());
                    if row.1.clicked() {
                        self.roadmaps.focus_node = Some(id.clone());
                    }
                    if row.1.drag_started() {
                        self.roadmaps.drag_node = Some(id.clone());
                    }
                    if row.1.hovered()
                        && self.roadmaps.drag_node.as_deref().is_some_and(|d| d != id)
                        && ui.input(|i| i.pointer.any_released())
                    {
                        if let Some(drag) = self.roadmaps.drag_node.clone() {
                            reorder = Some((drag, id.clone()));
                        }
                    }
                    widgets::meta_pill(ui, palette, node.kind.label(), match node.kind { NodeKind::Section => palette.text_secondary, NodeKind::Task => palette.accent, NodeKind::Milestone => palette.warning });
                    if node.estimated_minutes > 0 {
                        ui.label(egui::RichText::new(crate::i18n::tf("{} est", &[&format_minutes(node.estimated_minutes)])).size(9.5).color(palette.text_secondary));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button_sized(ui, palette, Icon::Trash, "Delete node", false, 22.0).clicked() {
                            self.confirm = Some(crate::app::ConfirmDialog {
                                title: crate::i18n::t("Delete roadmap item").into(),
                                message: crate::i18n::t("Delete this item and its children?").into(),
                                confirm_label: crate::i18n::t("Delete").into(),
                                action: crate::app::ConfirmAction::DeleteRoadmapNode(id.clone()),
                            });
                        }
                        if widgets::icon_button_sized(ui, palette, Icon::Edit, "Edit node", false, 22.0).clicked() {
                            self.edit_node(node.clone());
                        }
                        if node.kind == NodeKind::Section && widgets::icon_button_sized(ui, palette, Icon::Plus, "Add child task", false, 22.0).clicked() {
                            self.roadmaps.focus_node = Some(id.clone());
                            self.new_node(NodeKind::Task);
                        }
                    });
                });
                if let Some(refs) = self.roadmaps.refs.get(&id).cloned() {
                    for reference in &refs {
                        ui.horizontal(|ui| {
                            ui.add_space(*depth as f32 * 18.0 + 28.0);
                            let label = if reference.label.is_empty() { &reference.target } else { &reference.label };
                            if widgets::button(ui, palette, label, Some(match reference.kind { RefKind::Document => Icon::FileText, RefKind::Url => Icon::Link, RefKind::Note => Icon::Note }), widgets::ButtonVariant::Quiet).clicked() {
                                if reference.kind == RefKind::Document {
                                    self.open_document(&reference.target);
                                } else if reference.kind == RefKind::Url {
                                    open_url(&reference.target);
                                }
                            }
                        });
                    }
                }
                if node.kind == NodeKind::Section {
                    ui.separator();
                }
            }
        });
        self.roadmaps.drag_node = None;
        if let Some((dragged, target)) = reorder {
            self.reorder_nodes(&dragged, &target);
        }
    }

    fn reorder_nodes(&mut self, dragged: &str, target: &str) {
        let Some(dragged_node) = self.roadmaps.nodes.iter().find(|n| n.id == dragged).cloned() else { return };
        let Some(target_node) = self.roadmaps.nodes.iter().find(|n| n.id == target).cloned() else { return };
        if dragged_node.parent_id != target_node.parent_id {
            return;
        }
        let mut siblings: Vec<RoadmapNode> = self.roadmaps.nodes.iter().filter(|n| n.parent_id == dragged_node.parent_id).cloned().collect();
        siblings.sort_by_key(|n| (n.order_index, n.created_at));
        let dragged_pos = siblings.iter().position(|n| n.id == dragged);
        let target_pos = siblings.iter().position(|n| n.id == target);
        if let (Some(from), Some(to)) = (dragged_pos, target_pos) {
            let node = siblings.remove(from);
            let to = to.min(siblings.len());
            siblings.insert(to, node);
        }
        let ids: Vec<String> = siblings.into_iter().map(|n| n.id).collect();
        let _ = self.store.reorder_roadmap_nodes(&ids);
        self.roadmaps.reload(&self.store, self.library.as_ref());
    }

    fn new_node(&mut self, kind: NodeKind) {
        let Some(roadmap) = self.roadmaps.selected.clone() else { return };
        let parent = if kind == NodeKind::Section {
            None
        } else {
            self.roadmaps
                .focus_node
                .clone()
                .filter(|id| self.roadmaps.nodes.iter().any(|n| &n.id == id && n.kind == NodeKind::Section))
        };
        let mut node = RoadmapNode::new(roadmap, kind, "", self.roadmaps.nodes.len() as i32);
        node.parent_id = parent;
        self.roadmaps.editor = Some(NodeEditor {
            node,
            is_new: true,
            ref_url: String::new(),
            ref_label: String::new(),
            ref_document: None,
            error: None,
        });
    }

    fn edit_node(&mut self, node: RoadmapNode) {
        self.roadmaps.editor = Some(NodeEditor {
            node,
            is_new: false,
            ref_url: String::new(),
            ref_label: String::new(),
            ref_document: None,
            error: None,
        });
    }

    fn show_node_editor(&mut self, ctx: &egui::Context, palette: &Palette) {
        let Some(editor) = self.roadmaps.editor.as_mut() else { return };
        let mut save = false;
        let mut cancel = false;
        let mut add_ref = false;
        let mut delete_ref: Option<String> = None;
        egui::Window::new(if editor.is_new { "New roadmap item" } else { "Edit roadmap item" })
            .collapsible(false)
            .resizable(true)
            .default_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.direction(|ui| {
                    ui.label(crate::i18n::t("Title"));
                    widgets::text_field(ui, palette, &mut editor.node.title, "Title", 420.0);
                    ui.label(crate::i18n::t("Notes"));
                    ui.add(egui::TextEdit::multiline(&mut editor.node.notes).desired_rows(3).desired_width(420.0));
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("Kind"));
                        egui::ComboBox::from_id_salt("node-kind").selected_text(editor.node.kind.label()).show_ui(ui, |ui| {
                            for kind in [NodeKind::Section, NodeKind::Task, NodeKind::Milestone] {
                                ui.selectable_value(&mut editor.node.kind, kind, kind.label());
                            }
                        });
                        ui.label(crate::i18n::t("Status"));
                        egui::ComboBox::from_id_salt("node-status").selected_text(editor.node.status.label()).show_ui(ui, |ui| {
                            for status in [TaskStatus::NotStarted, TaskStatus::InProgress, TaskStatus::Completed, TaskStatus::Skipped] {
                                ui.selectable_value(&mut editor.node.status, status, status.label());
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("Estimated minutes"));
                        let mut est = editor.node.estimated_minutes as i32;
                        if ui.add(egui::DragValue::new(&mut est).range(0..=10000)).changed() {
                            editor.node.estimated_minutes = est.max(0) as u32;
                        }
                        ui.label(crate::i18n::t("Actual minutes"));
                        let mut actual = editor.node.actual_minutes as i32;
                        if ui.add(egui::DragValue::new(&mut actual).range(0..=100000)).changed() {
                            editor.node.actual_minutes = actual.max(0) as u32;
                        }
                    });
                    ui.separator();
                    ui.label(egui::RichText::new(crate::i18n::t("Reference")).size(11.5).color(palette.text_secondary));
                    ui.horizontal(|ui| {
                        let mut docs: Vec<(String, String)> = self.docs.iter().map(|d| (d.id.clone(), d.title.clone())).collect();
                        docs.sort_by(|a, b| a.1.cmp(&b.1));
                        let selected = editor.ref_document.clone();
                        let label = selected.as_ref().and_then(|id| docs.iter().find(|(d, _)| d == id)).map(|(_, t)| t.clone()).unwrap_or_else(|| crate::i18n::t("Link a document").into());
                        egui::ComboBox::from_id_salt("ref-doc").selected_text(label).show_ui(ui, |ui| {
                            for (id, title) in &docs {
                                if ui.selectable_label(editor.ref_document.as_deref() == Some(id), title).clicked() {
                                    editor.ref_document = Some(id.clone());
                                }
                            }
                        });
                        if widgets::button(ui, palette, "Add", Some(Icon::Plus), widgets::ButtonVariant::Quiet).clicked() {
                            add_ref = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        widgets::text_field(ui, palette, &mut editor.ref_url, "https://…", 300.0);
                        if widgets::button(ui, palette, "Add URL", Some(Icon::Link), widgets::ButtonVariant::Quiet).clicked() {
                            add_ref = true;
                        }
                    });
                    if let Some(refs) = self.roadmaps.refs.get(&editor.node.id) {
                        for reference in refs {
                            ui.horizontal(|ui| {
                                ui.label(if reference.label.is_empty() { reference.target.clone() } else { reference.label.clone() });
                                if widgets::icon_button_sized(ui, palette, Icon::Trash, "Remove reference", false, 22.0).clicked() {
                                    delete_ref = Some(reference.id.clone());
                                }
                            });
                        }
                    }
                    if let Some(error) = &editor.error {
                        ui.colored_label(palette.danger, error);
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if widgets::button(ui, palette, "Save", Some(Icon::Check), widgets::ButtonVariant::Primary).clicked() {
                            save = true;
                        }
                        if widgets::button(ui, palette, "Cancel", None, widgets::ButtonVariant::Quiet).clicked() {
                            cancel = true;
                        }
                    });
                });
            });
        if save {
            if editor.node.title.trim().is_empty() {
                editor.error = Some(crate::i18n::t("Title cannot be empty.").into());
            } else {
                let node = editor.node.clone();
                let new_ref = if let Some(doc) = editor.ref_document.clone() {
                    Some(RoadmapRef {
                        id: uuid::Uuid::new_v4().to_string(),
                        node_id: node.id.clone(),
                        kind: RefKind::Document,
                        target: doc,
                        label: crate::i18n::t("Linked document").into(),
                    })
                } else if !editor.ref_url.trim().is_empty() {
                    Some(RoadmapRef {
                        id: uuid::Uuid::new_v4().to_string(),
                        node_id: node.id.clone(),
                        kind: RefKind::Url,
                        target: editor.ref_url.trim().to_string(),
                        label: editor.ref_label.clone(),
                    })
                } else {
                    None
                };
                if self.store.upsert_roadmap_node(&node).is_ok() {
                    if let Some(reference) = new_ref {
                        let _ = self.store.upsert_roadmap_ref(&reference);
                    }
                    self.roadmaps.editor = None;
                    self.roadmaps.reload(&self.store, self.library.as_ref());
                } else {
                    editor.error = Some(crate::i18n::t("Could not save roadmap item.").into());
                }
            }
        } else if cancel {
            self.roadmaps.editor = None;
        }
        if let Some(id) = delete_ref {
            let _ = self.store.delete_roadmap_ref(&id);
            self.roadmaps.reload(&self.store, self.library.as_ref());
        }
    }
}

fn open_url(url: &str) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
}
