// SPDX-License-Identifier: GPL-3.0-or-later
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange};
use egui::text_edit::TextEditState;
use egui::{Align, Color32, Event, EventFilter, Id, ImeEvent, Key, RichText, Sense, TextStyle};
use egui_extras::{Column, TableBuilder};
use nocrumbs_core::{Analysis, BarContext, Entry, Glob, Mode, analyze, display_path, resolve};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Info,
    Good,
    Bad,
}

#[derive(Debug, Default)]
struct Perf {
    frames: VecDeque<f32>,
    complete_ms: f32,
    filter_ms: f32,
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

/// The spike application. All the logic that matters lives in [`App::show`] so that
/// headless tests can drive exactly what the window runs.
#[derive(Debug)]
pub struct App {
    fs: MemFs,
    cwd: String,
    show_hidden: bool,

    /// The bar's text, **including** any ghost text at the end.
    text: String,
    /// How many chars at the end of `text` are ghost text (selected, not yet typed).
    ghost_chars: usize,
    composing: bool,
    blurred: bool,
    first_frame: bool,

    /// Analysis of the typed text (ghost excluded), and the typed text it was computed for.
    analysis: Analysis,
    analysed_for: String,

    view_dir: String,
    entries: Arc<[Entry]>,
    view: Vec<u32>,
    view_key: (String, Option<String>, bool),
    hl_row: Option<usize>,
    selected: Option<usize>,
    scroll_to: Option<usize>,

    status: (String, Tone),
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
        use nocrumbs_core::Fs;
        let fs = MemFs::sample(true, big_n);
        let start = fs.canonical_dir(start).unwrap_or_else(|| HOME.to_string());
        let mut app = Self {
            fs,
            cwd: start.clone(),
            show_hidden: false,
            text: String::new(),
            ghost_chars: 0,
            composing: false,
            blurred: false,
            first_frame: true,
            analysis: Analysis { mode: Mode::Empty, dir: None, glob: None, completion: None },
            analysed_for: String::new(),
            view_dir: start,
            entries: Arc::from(Vec::new()),
            view: Vec::new(),
            view_key: (String::new(), None, false),
            hl_row: None,
            selected: None,
            scroll_to: None,
            status: (String::new(), Tone::Info),
            error_until: None,
            perf: Perf::default(),
        };
        app.text = app.shown_cwd();
        app.reanalyze();
        app.set_status(
            "Try: Doc then Tab · ~/Down · /big/report-09 · *.pdf · ~/Downloads/*.pdf · :sor · ../",
            Tone::Info,
        );
        app
    }

    // ----- test / inspection surface -----

    /// The whole bar text, including ghost text.
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Number of ghost characters at the end of [`App::text`].
    pub fn ghost_chars(&self) -> usize {
        self.ghost_chars
    }
    /// What the user has actually typed (the bar text without the ghost).
    pub fn typed(&self) -> &str {
        let keep = self.text.chars().count() - self.ghost_chars;
        let end = self.text.char_indices().nth(keep).map_or(self.text.len(), |(i, _)| i);
        &self.text[..end]
    }
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
    pub fn view_dir(&self) -> &str {
        &self.view_dir
    }
    pub fn shown_rows(&self) -> usize {
        self.view.len()
    }
    /// Name of the row carrying the completion highlight.
    pub fn highlighted(&self) -> Option<&str> {
        self.hl_row.map(|r| self.entries[self.view[r] as usize].name.as_str())
    }
    pub fn selected_name(&self) -> Option<&str> {
        self.selected.map(|r| self.entries[self.view[r] as usize].name.as_str())
    }
    pub fn status_text(&self) -> &str {
        &self.status.0
    }
    pub fn bar_id() -> Id {
        Id::unique("nocrumbs-bar")
    }

    // ----- helpers -----

    fn bar_ctx(&self) -> BarContext<'_> {
        BarContext { fs: &self.fs, cwd: &self.cwd, home: HOME, sep: '/', show_hidden: self.show_hidden }
    }

    fn shown_cwd(&self) -> String {
        display_path(&self.cwd, &self.bar_ctx())
    }

    fn set_status(&mut self, msg: impl Into<String>, tone: Tone) {
        self.status = (msg.into(), tone);
    }

    fn set_selection(ctx: &egui::Context, from: usize, to: usize) {
        let id = Self::bar_id();
        let mut st = TextEditState::load(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(from), CCursor::new(to))));
        st.store(ctx, id);
        ctx.request_repaint();
    }

    fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    /// Recompute the analysis (and the list view) for the typed text, if it changed.
    fn reanalyze(&mut self) {
        let typed = self.typed().to_string();
        if typed == self.analysed_for {
            return;
        }
        let t0 = Instant::now();
        let idle = typed == self.shown_cwd();
        self.analysis = if idle {
            Analysis { mode: Mode::Empty, dir: None, glob: None, completion: None }
        } else {
            analyze(&typed, &self.bar_ctx())
        };
        self.perf.complete_ms = t0.elapsed().as_secs_f32() * 1000.0;
        self.analysed_for = typed;
        self.refresh_view();
    }

    fn refresh_view(&mut self) {
        let dir = self.analysis.dir.clone().unwrap_or_else(|| self.cwd.clone());
        let glob = self.analysis.glob.clone();
        let key = (dir.clone(), glob.clone(), self.show_hidden);
        if key != self.view_key {
            let t0 = Instant::now();
            if dir != self.view_dir || self.entries.is_empty() {
                self.entries = self.fs_entries(&dir);
                self.selected = None;
            }
            let g = glob.as_deref().map(Glob::new);
            self.view = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| (self.show_hidden || !e.hidden) && g.as_ref().is_none_or(|g| g.is_match(&e.name)))
                .map(|(i, _)| i as u32)
                .collect();
            self.perf.filter_ms = t0.elapsed().as_secs_f32() * 1000.0;
            self.view_dir = dir;
            self.view_key = key;
            self.selected = None;
        }
        // Completion highlight: only for path completions, on the row it would navigate to.
        let new_hl = match (&self.analysis.mode, &self.analysis.completion) {
            (Mode::Path, Some(c)) => self.view.iter().position(|&i| self.entries[i as usize].name == c.name),
            _ => None,
        };
        if new_hl != self.hl_row {
            self.hl_row = new_hl;
            if new_hl.is_some() {
                self.scroll_to = new_hl;
            }
        }
    }

    fn fs_entries(&self, dir: &str) -> Arc<[Entry]> {
        use nocrumbs_core::Fs;
        self.fs.entries(dir).unwrap_or_else(|| Arc::from(Vec::new()))
    }

    // ----- ghost text -----

    /// Append the completion's ghost text, selected, so the next keystroke overwrites it.
    fn offer_ghost(&mut self, ctx: &egui::Context) {
        let Some(c) = &self.analysis.completion else { return };
        if c.ghost.is_empty() || !matches!(self.analysis.mode, Mode::Path | Mode::Command) {
            return;
        }
        let typed_chars = self.char_len() - self.ghost_chars;
        let ghost = c.ghost.clone();
        self.text.truncate(self.typed().len());
        self.text.push_str(&ghost);
        self.ghost_chars = ghost.chars().count();
        Self::set_selection(ctx, typed_chars, self.char_len());
    }

    fn clear_ghost(&mut self, ctx: &egui::Context) {
        let n = self.typed().len();
        self.text.truncate(n);
        self.ghost_chars = 0;
        let end = self.char_len();
        Self::set_selection(ctx, end, end);
    }

    /// Tab: accept the completion (canonical casing), then offer the next level.
    fn accept(&mut self, ctx: &egui::Context) {
        self.reanalyze();
        let Some(c) = self.analysis.completion.clone() else {
            // Nothing to complete: keep focus, and make sure any ghost is committed as typed.
            self.ghost_chars = 0;
            let end = self.char_len();
            Self::set_selection(ctx, end, end);
            return;
        };
        self.text = c.accepted;
        self.ghost_chars = 0;
        self.reanalyze();
        let end = self.char_len();
        Self::set_selection(ctx, end, end);
        self.offer_ghost(ctx);
    }

    fn go_idle(&mut self, ctx: &egui::Context) {
        self.text = self.shown_cwd();
        self.ghost_chars = 0;
        self.analysed_for = String::from("\u{0}");
        self.reanalyze();
        Self::set_selection(ctx, 0, self.char_len());
    }

    // ----- Enter -----

    fn submit(&mut self, ctx: &egui::Context) {
        self.reanalyze_full();
        let input = self.text.clone(); // ghost included, as a browser's URL bar does
        let a = analyze(&input, &self.bar_ctx());
        match a.mode {
            Mode::Command => {
                self.set_status(
                    format!("Command `{}` (commands are not implemented in the spike)", input.trim()),
                    Tone::Info,
                );
                return;
            }
            Mode::Glob => {
                self.set_status("Filter active. Esc clears it.", Tone::Good);
                return;
            }
            _ => {}
        }
        match resolve(&input, &self.bar_ctx()) {
            Some(r) if r.is_dir => {
                self.cwd = r.path.clone();
                self.go_idle(ctx);
                self.set_status(format!("Now in {}", self.shown_cwd()), Tone::Good);
            }
            Some(r) => {
                let (dir, name) = r.path.rsplit_once('/').unwrap_or(("", &r.path));
                self.cwd = if dir.is_empty() { "/".into() } else { dir.into() };
                let name = name.to_string();
                self.go_idle(ctx);
                self.selected = self.view.iter().position(|&i| self.entries[i as usize].name == name);
                self.scroll_to = self.selected;
                self.set_status(format!("Selected {name}"), Tone::Good);
            }
            None => {
                self.error_until = Some(Instant::now() + Duration::from_millis(400));
                self.set_status(format!("No such path: {}", input.trim()), Tone::Bad);
            }
        }
    }

    fn reanalyze_full(&mut self) {
        self.analysed_for = String::from("\u{0}");
        self.reanalyze();
    }

    fn move_selection(&mut self, dir: i32) {
        let n = self.view.len();
        if n == 0 {
            return;
        }
        let next = match (self.selected, dir) {
            (None, d) if d > 0 => 0,
            (None, _) => n - 1,
            (Some(i), d) => ((i as i64 + d as i64).rem_euclid(n as i64)) as usize,
        };
        self.selected = Some(next);
        self.scroll_to = Some(next);
    }

    // ----- the window -----

    /// One frame of the whole UI. `eframe::App::ui` calls this; so do the tests.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let t0 = Instant::now();
        let ctx = ui.ctx().clone();

        egui::Panel::top("bar-panel").show(ui, |ui| {
            ui.add_space(8.0);
            self.bar(ui, &ctx);
            ui.add_space(4.0);
            ui.label(
                RichText::new("Tab complete · Enter go · Up/Down select · Esc: ghost, then revert, then blur · / or Ctrl+L focus the bar")
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

    fn bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let id = Self::bar_id();
        if self.first_frame {
            self.first_frame = false;
            ctx.memory_mut(|m| m.request_focus(id));
            Self::set_selection(ctx, 0, self.char_len());
        }
        let focused = ctx.memory(|m| m.has_focus(id));

        // Read this frame's input before the TextEdit consumes it.
        let events = ctx.input(|i| i.events.clone());
        let (mut tab, mut esc, mut enter, mut up, mut down, mut deleted) = (false, false, false, false, false, false);
        let mut refocus = false;
        for ev in &events {
            match ev {
                Event::Ime(ImeEvent::Preedit { text, .. }) => self.composing = !text.is_empty(),
                Event::Ime(ImeEvent::Commit(_)) => self.composing = false,
                Event::Cut => deleted = true,
                Event::Text(t) if t == "/" && !focused => refocus = true,
                Event::Key { key, pressed: true, modifiers, .. } => match key {
                    Key::Backspace | Key::Delete => deleted = true,
                    Key::Tab if !modifiers.shift => tab = true,
                    Key::Escape => esc = true,
                    Key::Enter => enter = true,
                    Key::ArrowUp => up = true,
                    Key::ArrowDown => down = true,
                    Key::L if modifiers.command => refocus = true,
                    _ => {}
                },
                _ => {}
            }
        }
        if refocus {
            self.blurred = false;
            ctx.memory_mut(|m| m.request_focus(id));
        }

        // Keys the TextEdit must not act on: Tab (would move focus), Esc (would blur),
        // vertical arrows (list selection).
        if focused {
            if tab && !self.composing {
                self.accept(ctx);
            }
            if esc {
                self.escape(ctx);
            }
            if up {
                self.move_selection(-1);
            }
            if down {
                self.move_selection(1);
            }
            if enter && !self.composing {
                self.submit(ctx);
            }
        }

        let flash = self.error_until.is_some_and(|t| t > Instant::now());
        if !flash {
            self.error_until = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        ui.horizontal(|ui| {
            let (label, color) = match self.analysis.mode {
                Mode::Command => ("command", Color32::from_rgb(0x6d, 0x4a, 0xff)),
                Mode::Glob => ("filter", Color32::from_rgb(0xfe, 0xbc, 0x2e)),
                Mode::Path => ("path", Color32::from_gray(150)),
                Mode::Empty => ("", Color32::from_gray(150)),
            };
            if !label.is_empty() {
                ui.label(RichText::new(label.to_uppercase()).small().strong().color(color));
            }
            let out = egui::TextEdit::singleline(&mut self.text)
                .id(id)
                .font(TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .hint_text("path, *.glob or :command")
                .show(ui);
            // TextEdit sets its own focus filter every frame, so ours must be set after it:
            // Tab must not move focus and Esc must not blur (we stage Esc ourselves).
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
            if out.response.changed() {
                self.on_user_edit(ctx, deleted, &out.state);
            } else if self.ghost_chars > 0 {
                // The ghost stays only while it is still exactly the selection at the end.
                let n = self.char_len();
                let still = out.state.cursor.char_range().is_some_and(|r| {
                    let [a, b] = r.sorted_cursors();
                    a.index.0 == n - self.ghost_chars && b.index.0 == n
                });
                if !still {
                    self.ghost_chars = 0;
                }
            }
        });
    }

    fn on_user_edit(&mut self, ctx: &egui::Context, deleted: bool, state: &TextEditState) {
        // Whatever was selected (ghost) has been replaced or removed by this edit.
        self.ghost_chars = 0;
        self.reanalyze();
        let caret_at_end = state.cursor.char_range().is_some_and(|r| r.primary.index.0 == self.char_len());
        if !deleted && !self.composing && caret_at_end {
            self.offer_ghost(ctx);
        }
        let more = self.analysis.completion.as_ref().map_or(0, |c| c.matches.saturating_sub(1));
        if self.analysis.mode == Mode::Path && more > 0 {
            self.set_status(format!("+{more} more match{}", if more == 1 { "" } else { "es" }), Tone::Info);
        }
    }

    /// Staged Esc: remove the ghost, then revert the bar, then give up focus.
    fn escape(&mut self, ctx: &egui::Context) {
        if self.ghost_chars > 0 {
            self.clear_ghost(ctx);
        } else if self.typed() != self.shown_cwd() {
            self.go_idle(ctx);
        } else {
            ctx.memory_mut(|m| m.surrender_focus(Self::bar_id()));
            self.blurred = true;
        }
    }

    fn status_line(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            // The current path is always visible, even while the bar holds a filter.
            ui.label(RichText::new(display_path(&self.view_dir, &self.bar_ctx())).monospace().strong());
            ui.label(format!("· {} of {} shown", self.view.len(), self.entries.len()));
            let (msg, tone) = &self.status;
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
                        "frame {:.2} ms avg / {:.2} max · complete {:.2} ms · filter {:.2} ms",
                        self.perf.avg(),
                        self.perf.max(),
                        self.perf.complete_ms,
                        self.perf.filter_ms
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
        let (entries, view, selected, hl) = (&self.entries, &self.view, self.selected, self.hl_row);
        let mut clicked = None;
        table
            .header(ROW_H, |mut h| {
                for t in ["Name", "Size", "Modified", "Kind"] {
                    h.col(|ui| {
                        ui.label(RichText::new(t).strong().weak());
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_H, view.len(), |mut row| {
                    let r = row.index();
                    let e = &entries[view[r] as usize];
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
            self.selected = clicked;
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
