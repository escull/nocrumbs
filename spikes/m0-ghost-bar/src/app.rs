// SPDX-License-Identifier: GPL-3.0-or-later
//! The egui front end: a thin renderer over [`nocrumbs_core::Bar`].
//!
//! egui's `TextEdit` still does the text editing (caret, mouse, clipboard, IME). This file
//! only forwards what happened to the bar and obeys what the bar asks for. All behaviour
//! (completion, ghost text, Esc stages, navigation, selection) lives in `nocrumbs-core`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Color32, Event, EventFilter, Id, ImeEvent, Key, RichText, Sense, TextStyle};
use egui_extras::{Column, TableBuilder};
use nocrumbs_core::{Aliases, Bar, EditKind, Key as BarKey, Mode, Sel, SortDir, SortKey, Tone};
use nocrumbs_testfs::MemFs;

use crate::format;

const HOME: &str = "/home/user";
const ACCENT: Color32 = Color32::from_rgb(0x6d, 0x4a, 0xff);
const ROW_H: f32 = 22.0;

/// Dark theme with the accent colour as the text selection (which is how ghost text looks).
pub fn setup(ctx: &egui::Context) {
    // Dark regardless of the OS theme for now; the brief wants light to follow the OS later.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.global_style_mut(|s| {
        s.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.75);
        s.visuals.selection.stroke = egui::Stroke::new(1.0, Color32::WHITE);
    });
}

#[derive(Debug, Default)]
struct Perf {
    frames: VecDeque<f32>,
    edit_ms: f32,
}

impl Perf {
    fn push(&mut self, ms: f32) {
        if self.frames.len() >= 120 {
            self.frames.pop_front();
        }
        self.frames.push_back(ms);
    }
    fn avg(&self) -> f32 {
        if self.frames.is_empty() { 0.0 } else { self.frames.iter().sum::<f32>() / self.frames.len() as f32 }
    }
    fn max(&self) -> f32 {
        self.frames.iter().copied().fold(0.0, f32::max)
    }
}

/// The spike application. [`App::show`] is one frame of the whole UI; `eframe::App::ui` calls
/// it, and so do the headless tests.
#[derive(Debug)]
pub struct App {
    bar: Bar,
    blurred: bool,
    first_frame: bool,
    scroll_to: Option<usize>,
    seen_errors: u32,
    error_until: Option<Instant>,
    perf: Perf,
}

impl App {
    /// `big_n` is the number of entries in `/big`.
    pub fn new(big_n: usize) -> Self {
        Self::with_start(big_n, HOME)
    }

    /// Like [`App::new`], but starting in `start` (falls back to home if it does not exist).
    pub fn with_start(big_n: usize, start: &str) -> Self {
        let fs = Arc::new(MemFs::sample(true, big_n));
        let mut bar = Bar::new(fs, HOME, start, '/');
        // Seed aliases (brief section 6); persistence comes with the Windows shell.
        bar.set_aliases(Aliases::from_pairs([
            ("dl", "/home/user/Downloads"),
            ("proj", "/home/user/projects"),
            ("pics", "/home/user/Pictures"),
        ]));
        bar.set_status(
            "Try: Doc then Tab · pi · proj/nocr · dl/*.pdf · :cd Doc · :sort size desc · :alias work ~/Documents · :help",
            Tone::Info,
        );
        Self {
            bar,
            blurred: false,
            first_frame: true,
            scroll_to: None,
            seen_errors: 0,
            error_until: None,
            perf: Perf::default(),
        }
    }

    // ----- test / inspection surface -----

    pub fn bar(&self) -> &Bar {
        &self.bar
    }
    /// The whole bar text, including ghost text.
    pub fn text(&self) -> &str {
        self.bar.text()
    }
    pub fn ghost_chars(&self) -> usize {
        self.bar.ghost_chars()
    }
    /// What the user has actually typed (the bar text without the ghost).
    pub fn typed(&self) -> &str {
        self.bar.typed()
    }
    pub fn cwd(&self) -> &str {
        self.bar.cwd()
    }
    pub fn view_dir(&self) -> &str {
        self.bar.view_dir()
    }
    pub fn shown_rows(&self) -> usize {
        self.bar.row_count()
    }
    /// Name of the row carrying the completion highlight.
    pub fn highlighted(&self) -> Option<&str> {
        self.bar.highlighted_name()
    }
    pub fn selected_name(&self) -> Option<&str> {
        self.bar.selected_name()
    }
    pub fn status_text(&self) -> &str {
        self.bar.status().0
    }
    pub fn bar_id() -> Id {
        Id::unique("nocrumbs-bar")
    }

    // ----- plumbing between the bar and egui -----

    fn set_selection(ctx: &egui::Context, sel: Sel) {
        let id = Self::bar_id();
        let mut st = TextEditState::load(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(sel.anchor), CCursor::new(sel.caret))));
        st.store(ctx, id);
        ctx.request_repaint();
    }

    /// Do what the bar has asked for: select, scroll, blur.
    fn apply_requests(&mut self, ctx: &egui::Context) {
        if let Some(sel) = self.bar.take_selection_request() {
            Self::set_selection(ctx, sel);
        }
        if let Some(row) = self.bar.take_scroll_request() {
            self.scroll_to = Some(row);
        }
        if self.bar.take_blur_request() {
            ctx.memory_mut(|m| m.surrender_focus(Self::bar_id()));
            self.blurred = true;
        }
    }

    // ----- the window -----

    /// One frame of the whole UI.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let t0 = Instant::now();
        let ctx = ui.ctx().clone();

        egui::Panel::top("bar-panel").show(ui, |ui| {
            ui.add_space(8.0);
            self.bar_row(ui, &ctx);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Tab complete · Enter go · Up/Down select · Alt+Left/Right history · Esc: ghost, then revert, then blur · / or Ctrl+L focus the bar")
                    .small()
                    .weak(),
            );
            ui.add_space(6.0);
        });
        egui::Panel::bottom("status-panel").show(ui, |ui| {
            ui.add_space(3.0);
            self.status_line(ui);
            ui.add_space(3.0);
        });
        egui::CentralPanel::default().show(ui, |ui| self.list(ui));

        self.perf.push(t0.elapsed().as_secs_f32() * 1000.0);
    }

    fn bar_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let id = Self::bar_id();
        if self.first_frame {
            self.first_frame = false;
            ctx.memory_mut(|m| m.request_focus(id));
        }
        let focused = ctx.memory(|m| m.has_focus(id));

        // Read this frame's input before the TextEdit consumes it.
        let events = ctx.input(|i| i.events.clone());
        let mut keys: Vec<BarKey> = Vec::new();
        let (mut deleted, mut refocus) = (false, false);
        for ev in &events {
            match ev {
                Event::Ime(ImeEvent::Preedit { text, .. }) => self.bar.set_composing(!text.is_empty()),
                Event::Ime(ImeEvent::Commit(_)) => self.bar.set_composing(false),
                Event::Cut => deleted = true,
                Event::Text(t) if t == "/" && !focused => refocus = true,
                Event::Key { key, pressed: true, modifiers, .. } => match key {
                    Key::Backspace | Key::Delete => deleted = true,
                    Key::Tab if !modifiers.shift => keys.push(BarKey::Tab),
                    Key::Escape => keys.push(BarKey::Escape),
                    Key::Enter => keys.push(BarKey::Enter),
                    Key::ArrowUp => keys.push(BarKey::Up),
                    Key::ArrowDown => keys.push(BarKey::Down),
                    Key::ArrowLeft if modifiers.alt => keys.push(BarKey::Back),
                    Key::ArrowRight if modifiers.alt => keys.push(BarKey::Forward),
                    Key::L if modifiers.command => refocus = true,
                    _ => {}
                },
                _ => {}
            }
        }
        // Alt+Left/Right are history, not caret movement: keep them from the TextEdit.
        for k in &keys {
            match k {
                BarKey::Back => ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::ArrowLeft)),
                BarKey::Forward => ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::ArrowRight)),
                _ => false,
            };
        }
        if refocus {
            self.blurred = false;
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if focused {
            for k in keys {
                self.bar.key(k);
            }
        }
        if self.bar.error_count() != self.seen_errors {
            self.seen_errors = self.bar.error_count();
            self.error_until = Some(Instant::now() + Duration::from_millis(400));
        }
        self.apply_requests(ctx);

        let flash = self.error_until.is_some_and(|t| t > Instant::now());
        if flash {
            ctx.request_repaint_after(Duration::from_millis(100));
        } else {
            self.error_until = None;
        }

        ui.horizontal(|ui| {
            let alias_in_use = self.bar.analysis().alias.is_some();
            let (label, color) = match self.bar.mode() {
                Mode::Command => ("command", ACCENT),
                Mode::Glob if alias_in_use => ("filter via alias", Color32::from_rgb(0xfe, 0xbc, 0x2e)),
                Mode::Glob => ("filter", Color32::from_rgb(0xfe, 0xbc, 0x2e)),
                Mode::Path if alias_in_use => ("alias", Color32::from_rgb(0x4a, 0xd2, 0x95)),
                Mode::Path => ("path", Color32::from_gray(150)),
                Mode::Empty => ("", Color32::from_gray(150)),
            };
            if !label.is_empty() {
                ui.label(RichText::new(label.to_uppercase()).small().strong().color(color));
            }

            // The bar owns the text; the TextEdit edits a copy and we report what changed.
            let mut buf = self.bar.text().to_owned();
            let out = egui::TextEdit::singleline(&mut buf)
                .id(id)
                .font(TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .hint_text("path, *.glob or :command")
                .show(ui);

            // TextEdit sets its own focus filter every frame, so ours must come after it:
            // Tab must not move focus and Esc must not blur (the bar stages Esc itself).
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    id,
                    EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true },
                );
            });
            if flash {
                ui.painter().rect_stroke(
                    out.response.rect.expand(2.0),
                    4.0,
                    egui::Stroke::new(2.0, Color32::from_rgb(0xff, 0x5d, 0x5d)),
                    egui::StrokeKind::Outside,
                );
            }
            if out.response.lost_focus() && !self.blurred {
                // Enter makes a singleline TextEdit surrender focus; the bar keeps it.
                out.response.request_focus();
            }

            let range = out.state.cursor.char_range();
            if out.response.changed() {
                let caret = range.map_or_else(|| buf.chars().count(), |r| r.primary.index.0);
                let kind = if deleted { EditKind::Delete } else { EditKind::Insert };
                let t0 = Instant::now();
                self.bar.edited(buf, caret, kind);
                self.perf.edit_ms = t0.elapsed().as_secs_f32() * 1000.0;
            } else if let Some(r) = range {
                let [a, b] = r.sorted_cursors();
                self.bar.observe_selection(Sel { anchor: a.index.0, caret: b.index.0 });
            }
            self.apply_requests(ctx);
        });
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            // The current path is always visible, even while the bar holds a filter.
            ui.label(RichText::new(self.bar.view_dir_display()).monospace().strong());
            ui.label(format!("· {} of {} shown", self.bar.row_count(), self.bar.entry_count()));
            let (msg, tone) = self.bar.status();
            let color = match tone {
                Tone::Info => ui.visuals().weak_text_color(),
                Tone::Good => Color32::from_rgb(0x4a, 0xd2, 0x95),
                Tone::Bad => Color32::from_rgb(0xff, 0x5d, 0x5d),
            };
            if !msg.is_empty() {
                ui.label(RichText::new(msg).color(color));
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "frame {:.2} ms avg / {:.2} max · last edit {:.2} ms",
                        self.perf.avg(),
                        self.perf.max(),
                        self.perf.edit_ms
                    ))
                    .small()
                    .weak(),
                );
            });
        });
    }

    fn list(&mut self, ui: &mut egui::Ui) {
        let mut table = TableBuilder::new(ui)
            .striped(false)
            .resizable(false)
            .sense(Sense::click())
            .column(Column::remainder().at_least(160.0).clip(true))
            .column(Column::exact(90.0))
            .column(Column::exact(140.0))
            .column(Column::exact(110.0));
        if let Some(row) = self.scroll_to.take() {
            table = table.scroll_to_row(row, Some(Align::Center));
        }
        let bar = &self.bar;
        let (selected, hl) = (bar.selected_row(), bar.highlighted_row());
        let sort = bar.sort();
        let mut clicked = None;
        let mut header_clicked = None;
        table
            .header(ROW_H, |mut h| {
                for (title, key) in [
                    ("Name", Some(SortKey::Name)),
                    ("Size", Some(SortKey::Size)),
                    ("Modified", Some(SortKey::Date)),
                    ("Kind", None),
                ] {
                    h.col(|ui| {
                        // The sort shows as text: egui's default fonts have no arrow glyphs.
                        let suffix = match (key, sort) {
                            (Some(k), (sk, SortDir::Asc)) if k == sk => " (asc)",
                            (Some(k), (sk, SortDir::Desc)) if k == sk => " (desc)",
                            _ => "",
                        };
                        let label = egui::Label::new(RichText::new(format!("{title}{suffix}")).strong().weak());
                        let r = ui.add(if key.is_some() { label.sense(Sense::click()) } else { label });
                        if r.clicked() {
                            header_clicked = key;
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_H, bar.row_count(), |mut row| {
                    let r = row.index();
                    let e = bar.row(r);
                    row.set_selected(selected == Some(r));
                    let is_hl = hl == Some(r);
                    let paint_hl = |ui: &mut egui::Ui| {
                        if is_hl {
                            let rect = ui.max_rect().expand2(egui::vec2(ui.spacing().item_spacing.x, 1.0));
                            ui.painter().rect_filled(rect, 0.0, ACCENT.gamma_multiply(0.22));
                        }
                    };
                    row.col(|ui| {
                        paint_hl(ui);
                        let name = if e.is_dir { format!("{}/", e.name) } else { e.name.clone() };
                        let t = RichText::new(name);
                        ui.label(if e.is_dir { t.color(Color32::from_rgb(0xbc, 0xaa, 0xff)) } else { t });
                    });
                    row.col(|ui| {
                        paint_hl(ui);
                        ui.label(RichText::new(if e.is_dir { "—".into() } else { format::size(e.size) }).weak());
                    });
                    row.col(|ui| {
                        paint_hl(ui);
                        ui.label(RichText::new(format::date(e.mtime)).weak());
                    });
                    row.col(|ui| {
                        paint_hl(ui);
                        ui.label(RichText::new(format::kind(&e.name, e.is_dir)).weak());
                    });
                    if row.response().clicked() {
                        clicked = Some(r);
                    }
                });
            });
        if clicked.is_some() {
            self.bar.select_row(clicked);
        }
        if let Some(key) = header_clicked {
            self.bar.sort_by_column(key);
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
