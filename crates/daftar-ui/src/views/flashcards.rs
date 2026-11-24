//! Flashcards and focused study mode.

use daftar_core::model::{Category, CardStatus, Flashcard, Tag};
use egui::Ui;

use crate::app::DaftarApp;
use crate::theme::Palette;

#[derive(Debug, Clone, Default)]
pub struct FlashcardsState {
    pub cards: Vec<Flashcard>,
    pub filter: String,
    pub tag: Option<String>,
    pub category: Option<String>,
    pub status: Option<CardStatus>,
    pub selected: std::collections::HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct StudyState {
    pub cards: Vec<Flashcard>,
    pub index: usize,
    pub flipped: bool,
    pub reviewed: u32,
    pub learned: u32,
    pub started_at: i64,
    pub deck_label: String,
}

impl FlashcardsState {
    pub fn reload(&mut self, store: &daftar_store::Store, categories: &[Category], tags: &[Tag]) {
        self.cards = store.flashcards().unwrap_or_default();
        let _ = (categories, tags);
    }
}

impl DaftarApp {
    pub(crate) fn show_flashcards(&mut self, ui: &mut Ui, palette: &Palette) {
        self.flashcards.reload(&self.store, &self.categories, &self.tags);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.heading(crate::i18n::t("Flashcards"));
            if crate::widgets::button(ui, palette, "New card", Some(crate::Icon::Plus), crate::widgets::ButtonVariant::Primary).clicked() {
                self.flashcard_editor = Some(crate::app::FlashcardEditor {
                    card: Flashcard::new(String::new(), String::new()),
                    is_new: true,
                    source_title: None,
                    error: None,
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::widgets::button(ui, palette, "Study selected", Some(crate::Icon::Play), crate::widgets::ButtonVariant::Secondary).clicked() {
                    let cards: Vec<Flashcard> = if self.flashcards.selected.is_empty() {
                        self.filtered_cards()
                    } else {
                        self.flashcards.cards.iter().filter(|c| self.flashcards.selected.contains(&c.id)).cloned().collect()
                    };
                    self.start_study(cards, crate::i18n::t("Selection"));
                }
                if crate::widgets::button(ui, palette, "Study all", Some(crate::Icon::Play), crate::widgets::ButtonVariant::Primary).clicked() {
                    let cards = self.filtered_cards();
                    self.start_study(cards, crate::i18n::t("All cards"));
                }
            });
        });
        let (total, learned, review) = self.store.card_counts().unwrap_or((0,0,0));
        ui.label(egui::RichText::new(crate::i18n::tf("{} cards · {} learned · {} marked for review", &[&total.to_string(), &learned.to_string(), &review.to_string()])).size(11.5).color(palette.text_secondary));
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            crate::widgets::search_field(ui, palette, &mut self.flashcards.filter, crate::i18n::t("Filter cards…"));
            for status in [CardStatus::New, CardStatus::Learning, CardStatus::Review, CardStatus::Learned] {
                if crate::widgets::chip(ui, palette, status.label(), self.flashcards.status == Some(status)).clicked() {
                    self.flashcards.status = if self.flashcards.status == Some(status) { None } else { Some(status) };
                }
            }
        });
        if !self.flashcards.selected.is_empty() {
            ui.horizontal(|ui| {
                ui.label(crate::i18n::tf("{} selected", &[&self.flashcards.selected.len().to_string()]));
                if crate::widgets::button(ui, palette, "Export PNG ZIP", None, crate::widgets::ButtonVariant::Secondary).clicked() {
                    self.start_batch_export(crate::app::BatchFormat::PngZip);
                }
                if crate::widgets::button(ui, palette, "Export PDF", None, crate::widgets::ButtonVariant::Secondary).clicked() {
                    self.start_batch_export(crate::app::BatchFormat::Pdf);
                }
            });
        }
        ui.separator();
        let cards = self.filtered_cards();
        if cards.is_empty() {
            crate::widgets::empty_state(ui, palette, crate::Icon::Flashcards, "No cards", "Create a card, or select text in the reader and choose Create flashcard.");
            return;
        }
        egui::ScrollArea::vertical().id_salt("cards-list").auto_shrink([false,false]).show(ui, |ui| {
            for card in cards {
                egui::Frame::new().fill(palette.surface).stroke(egui::Stroke::new(1.0, palette.border)).corner_radius(8.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut checked = self.flashcards.selected.contains(&card.id);
                        if ui.checkbox(&mut checked, "").changed() {
                            if checked { self.flashcards.selected.insert(card.id.clone()); } else { self.flashcards.selected.remove(&card.id); }
                        }
                        let (front, _) = crate::shaped::shape(&mut self.engine, &card.front, &crate::shaped::ShapedStyle::title().with_size(13.5), ui.available_width() - 160.0);
                        let colors = daftar_text::TextColors { text: palette.text, link: palette.accent, code: palette.text, code_background: None, selection: egui::Color32::TRANSPARENT };
                        self.engine.paint(ui.ctx(), ui.painter(), &front, ui.cursor().min, &colors, None, &[]);
                        ui.allocate_exact_size(egui::vec2(ui.available_width(), front.height), egui::Sense::hover());
                        crate::widgets::meta_pill(ui, palette, card.status.label(), match card.status { CardStatus::Learned => palette.success, CardStatus::Review => palette.warning, CardStatus::Learning => palette.accent, CardStatus::New => palette.text_secondary });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if crate::widgets::icon_button_sized(ui, palette, crate::Icon::Trash, crate::i18n::t("Delete"), false, 24.0).clicked() {
                                self.confirm = Some(crate::app::ConfirmDialog { title: crate::i18n::t("Delete flashcard").into(), message: crate::i18n::t("Delete this flashcard?").into(), confirm_label: crate::i18n::t("Delete").into(), action: crate::app::ConfirmAction::DeleteFlashcard(card.id.clone()) });
                            }
                            if crate::widgets::icon_button_sized(ui, palette, crate::Icon::Edit, "Edit", false, 24.0).clicked() {
                                self.flashcard_editor = Some(crate::app::FlashcardEditor { card: card.clone(), is_new: false, source_title: None, error: None });
                            }
                        });
                    });
                    ui.label(egui::RichText::new(&card.back).size(12.0).color(palette.text_secondary));
                    if !card.tags.is_empty() {
                        ui.label(egui::RichText::new(card.tags.join(" · ")).size(10.5).color(palette.text_secondary));
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    pub(crate) fn filtered_cards(&self) -> Vec<Flashcard> {
        let filter = self.flashcards.filter.to_lowercase();
        self.flashcards.cards.iter().filter(|c| {
            (self.flashcards.status.is_none() || self.flashcards.status == Some(c.status))
                && (filter.is_empty() || c.front.to_lowercase().contains(&filter) || c.back.to_lowercase().contains(&filter))
        }).cloned().collect()
    }

    pub(crate) fn start_study(&mut self, mut cards: Vec<Flashcard>, deck: &str) {
        if cards.is_empty() {
            self.toast(crate::i18n::t("No cards to study."), crate::theme::ToastKind::Warning);
            return;
        }
        use rand::seq::SliceRandom;
        cards.shuffle(&mut rand::thread_rng());
        self.study = Some(StudyState {
            cards,
            index: 0,
            flipped: false,
            reviewed: 0,
            learned: 0,
            started_at: daftar_core::now_ms(),
            deck_label: deck.to_string(),
        });
        self.view = crate::app::View::Study;
    }

    pub(crate) fn show_study(&mut self, ui: &mut Ui, palette: &Palette) {
        #[derive(Clone, Copy)]
        enum Action {
            Flip,
            Next,
            Prev,
            Learned,
            Review,
            Exit,
        }
        let Some(study) = self.study.as_ref() else {
            self.view = crate::app::View::Flashcards;
            return;
        };
        let total = study.cards.len();
        let index = study.index.min(total.saturating_sub(1));
        let flipped = study.flipped;
        let reviewed = study.reviewed;
        let learned = study.learned;
        let started_at = study.started_at;
        let deck_label = study.deck_label.clone();
        let card = study.cards[index].clone();

        let mut action: Option<Action> = None;
        ui.input(|i| {
            if i.key_pressed(egui::Key::Space) {
                action = Some(Action::Flip);
            }
            // Forward/back arrows follow the reading direction (left is forward in RTL).
            let forward_key = if crate::rtl::is_rtl() {
                egui::Key::ArrowLeft
            } else {
                egui::Key::ArrowRight
            };
            let backward_key = if crate::rtl::is_rtl() {
                egui::Key::ArrowRight
            } else {
                egui::Key::ArrowLeft
            };
            if i.key_pressed(forward_key) {
                action = Some(Action::Next);
            }
            if i.key_pressed(backward_key) {
                action = Some(Action::Prev);
            }
            if i.key_pressed(egui::Key::L) {
                action = Some(Action::Learned);
            }
            if i.key_pressed(egui::Key::R) {
                action = Some(Action::Review);
            }
            if i.key_pressed(egui::Key::Escape) {
                action = Some(Action::Exit);
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if crate::widgets::icon_button(ui, palette, crate::Icon::ArrowLeft, "Exit study", false).clicked() {
                action = Some(Action::Exit);
            }
            ui.label(egui::RichText::new(format!("{}/{}", index + 1, total)).size(12.0).color(palette.text_secondary));
            crate::widgets::progress_bar(ui, palette, (index + 1) as f32 / total.max(1) as f32, palette.accent, 4.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(crate::i18n::tf("{} reviewed · {} learned", &[&reviewed.to_string(), &learned.to_string()])).size(11.0).color(palette.text_secondary));
            });
        });
        ui.add_space(18.0);
        let available = ui.available_size();
        let card_size = egui::vec2(available.x.min(760.0), (available.y - 150.0).max(260.0));
        ui.vertical_centered(|ui| {
            let (rect, response) = ui.allocate_exact_size(card_size, egui::Sense::click());
            ui.painter().rect_filled(rect, 16.0, palette.surface);
            ui.painter().rect_stroke(rect, 16.0, egui::Stroke::new(1.0, palette.border), egui::StrokeKind::Inside);
            if response.clicked() {
                action = Some(Action::Flip);
            }
            let text = if flipped { &card.back } else { &card.front };
            let (shaped, _) = crate::shaped::shape(&mut self.engine, text, &crate::shaped::ShapedStyle::title().with_size(24.0).with_weight(500), rect.width() - 80.0);
            let colors = daftar_text::TextColors {
                text: palette.text,
                link: palette.accent,
                code: palette.text,
                code_background: Some(palette.surface_sunken),
                selection: egui::Color32::TRANSPARENT,
            };
            let x = rect.center().x - shaped.width * 0.5;
            let y = rect.center().y - shaped.height * 0.5;
            self.engine.paint(ui.ctx(), ui.painter(), &shaped, egui::pos2(x, y), &colors, None, &[]);
            ui.painter().text(
                egui::pos2(rect.right() - 20.0, rect.bottom() - 16.0),
                egui::Align2::RIGHT_BOTTOM,
                if flipped { crate::i18n::t("BACK") } else { crate::i18n::t("FRONT") },
                egui::FontId::proportional(9.5),
                palette.text_secondary,
            );
            if !card.tags.is_empty() {
                ui.painter().text(
                    egui::pos2(rect.left() + 20.0, rect.bottom() - 16.0),
                    egui::Align2::LEFT_BOTTOM,
                    card.tags.join(" · "),
                    egui::FontId::proportional(9.5),
                    palette.text_secondary,
                );
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if crate::widgets::button(ui, palette, "Previous", Some(crate::Icon::ArrowLeft), crate::widgets::ButtonVariant::Quiet).clicked() {
                action = Some(Action::Prev);
            }
            if crate::widgets::button(ui, palette, if flipped { "Show front" } else { "Flip card" }, Some(crate::Icon::Flip), crate::widgets::ButtonVariant::Secondary).clicked() {
                action = Some(Action::Flip);
            }
            if crate::widgets::button(ui, palette, "Needs review", Some(crate::Icon::Refresh), crate::widgets::ButtonVariant::Quiet).clicked() {
                action = Some(Action::Review);
            }
            if crate::widgets::button(ui, palette, "Learned", Some(crate::Icon::Check), crate::widgets::ButtonVariant::Primary).clicked() {
                action = Some(Action::Learned);
            }
            if crate::widgets::button(ui, palette, "Next", Some(crate::Icon::ArrowRight), crate::widgets::ButtonVariant::Quiet).clicked() {
                action = Some(Action::Next);
            }
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new(crate::i18n::t("Space flip · ←/→ navigate · L learned · R review · Esc exit")).size(10.5).color(palette.text_secondary));

        let Some(action) = action else { return };
        match action {
            Action::Flip => {
                if let Some(study) = self.study.as_mut() {
                    study.flipped = !study.flipped;
                }
            }
            Action::Next => {
                if let Some(study) = self.study.as_mut() {
                    study.index = (study.index + 1).min(total.saturating_sub(1));
                    study.flipped = false;
                }
            }
            Action::Prev => {
                if let Some(study) = self.study.as_mut() {
                    study.index = study.index.saturating_sub(1);
                    study.flipped = false;
                }
            }
            Action::Learned => {
                let _ = self.store.review_flashcard(&card.id, CardStatus::Learned);
                if let Some(study) = self.study.as_mut() {
                    study.reviewed += 1;
                    study.learned += 1;
                    study.index = (study.index + 1).min(total.saturating_sub(1));
                    study.flipped = false;
                }
                self.toast(crate::i18n::t("Marked as learned."), crate::theme::ToastKind::Success);
            }
            Action::Review => {
                let _ = self.store.review_flashcard(&card.id, CardStatus::Review);
                if let Some(study) = self.study.as_mut() {
                    study.reviewed += 1;
                    study.index = (study.index + 1).min(total.saturating_sub(1));
                    study.flipped = false;
                }
                self.toast(crate::i18n::t("Marked for review."), crate::theme::ToastKind::Info);
            }
            Action::Exit => {
                let session = daftar_core::model::StudySession {
                    id: uuid::Uuid::new_v4().to_string(),
                    started_at,
                    ended_at: daftar_core::now_ms(),
                    cards_reviewed: reviewed,
                    cards_learned: learned,
                    deck_label,
                };
                let _ = self.store.add_study_session(&session);
                self.study = None;
                self.view = crate::app::View::Flashcards;
            }
        }
    }
}
