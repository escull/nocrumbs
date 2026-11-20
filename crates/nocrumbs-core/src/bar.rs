// SPDX-License-Identifier: GPL-3.0-or-later
//! The address-bar state machine: everything that happens between a keystroke and the
//! screen, with no UI and no OS calls.
//!
//! A front end owns the actual text field (caret, mouse selection, clipboard, IME) and
//! reports to [`Bar`] what happened:
//!
//! * [`Bar::edited`] — the user changed the text (typed, pasted, deleted);
//! * [`Bar::observe_selection`] — the field's selection, once per frame, so a ghost the user
//!   has moved off is committed as typed text;
//! * [`Bar::key`] — Tab, Esc, Enter, Up, Down;
//! * [`Bar::set_composing`] — an IME is composing.
//!
//! In return the front end renders [`Bar::text`] in the field and obeys the requests the bar
//! queues: [`Bar::take_selection_request`] (select the ghost, select all, collapse the caret),
//! [`Bar::take_scroll_request`] and [`Bar::take_blur_request`].
//!
//! `sim::Session` is a headless text field that does exactly this, so the whole behaviour spec
//! can be tested as keystroke scripts.

use std::fmt;
use std::sync::Arc;

use crate::complete::{Analysis, BarContext, Mode, analyze, display_path, resolve};
use crate::fs::{Entry, Fs};
use crate::glob::Glob;

/// A text selection in the bar, in characters. `anchor == caret` is just a caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sel {
    pub anchor: usize,
    pub caret: usize,
}

impl Sel {
    pub fn caret(at: usize) -> Self {
        Self { anchor: at, caret: at }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.caret)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.caret)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.caret
    }
}

/// What the user just did to the text. Completion is not re-offered after a deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
}

/// Keys the bar itself handles. Text editing keys (Backspace, Delete, arrows, Home, End)
/// belong to the front end's text field and arrive as [`Bar::edited`] / [`Bar::observe_selection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Tab,
    Escape,
    Enter,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Good,
    Bad,
}

/// Cache key for the listing: folder, glob pattern, hidden files shown.
type ViewKey = (String, Option<String>, bool);

pub struct Bar {
    fs: Arc<dyn Fs>,
    home: String,
    sep: char,
    cwd: String,
    show_hidden: bool,

    /// The bar text, including any ghost text at the end.
    text: String,
    /// How many chars at the end of `text` are ghost text (offered, not typed).
    ghost_chars: usize,
    composing: bool,

    analysis: Analysis,
    analysed_for: Option<String>,

    view_dir: String,
    entries: Arc<[Entry]>,
    rows: Vec<u32>,
    view_key: Option<ViewKey>,
    hl_row: Option<usize>,
    selected: Option<usize>,

    status: (String, Tone),
    errors: u32,

    sel_request: Option<Sel>,
    scroll_request: Option<usize>,
    blur_request: bool,
}

impl fmt::Debug for Bar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bar")
            .field("cwd", &self.cwd)
            .field("text", &self.text)
            .field("ghost_chars", &self.ghost_chars)
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

impl Bar {
    /// `start` is the initial folder (falls back to `home` if it does not exist); `sep` is the
    /// separator appended after a completed directory when the user has not typed one.
    pub fn new(fs: Arc<dyn Fs>, home: &str, start: &str, sep: char) -> Self {
        let cwd = fs.canonical_dir(start).or_else(|| fs.canonical_dir(home)).unwrap_or_else(|| "/".to_string());
        let mut bar = Self {
            fs,
            home: home.to_string(),
            sep,
            view_dir: cwd.clone(),
            cwd,
            show_hidden: false,
            text: String::new(),
            ghost_chars: 0,
            composing: false,
            analysis: Analysis::idle(),
            analysed_for: None,
            entries: Arc::from(Vec::new()),
            rows: Vec::new(),
            view_key: None,
            hl_row: None,
            selected: None,
            status: (String::new(), Tone::Info),
            errors: 0,
            sel_request: None,
            scroll_request: None,
            blur_request: false,
        };
        bar.go_idle();
        bar.scroll_request = None;
        bar
    }

    // ----- state the front end renders -----

    /// The bar text, including ghost text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// What the user has actually typed: [`Bar::text`] without the ghost.
    pub fn typed(&self) -> &str {
        let keep = self.char_len() - self.ghost_chars;
        let end = self.text.char_indices().nth(keep).map_or(self.text.len(), |(i, _)| i);
        &self.text[..end]
    }

    /// Number of ghost characters at the end of [`Bar::text`].
    pub fn ghost_chars(&self) -> usize {
        self.ghost_chars
    }

    /// The current folder (canonical absolute path).
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// The current folder as the bar shows it when idle (`~/Documents`).
    pub fn display_cwd(&self) -> String {
        display_path(&self.cwd, &self.ctx())
    }

    /// The folder the list shows: the current folder, or the folder being typed toward.
    pub fn view_dir(&self) -> &str {
        &self.view_dir
    }

    pub fn view_dir_display(&self) -> String {
        display_path(&self.view_dir, &self.ctx())
    }

    pub fn mode(&self) -> Mode {
        self.analysis.mode
    }

    pub fn analysis(&self) -> &Analysis {
        &self.analysis
    }

    /// Rows shown (after the glob filter and the hidden-files setting).
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Entries in the shown folder, before filtering.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn row(&self, i: usize) -> &Entry {
        &self.entries[self.rows[i] as usize]
    }

    /// The row carrying the completion highlight: what Tab would go to.
    pub fn highlighted_row(&self) -> Option<usize> {
        self.hl_row
    }

    pub fn highlighted_name(&self) -> Option<&str> {
        self.hl_row.map(|r| self.row(r).name.as_str())
    }

    /// The selected row: the only target of destructive operations (distinct from the highlight).
    pub fn selected_row(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.selected.map(|r| self.row(r).name.as_str())
    }

    pub fn select_row(&mut self, row: Option<usize>) {
        self.selected = row.filter(|r| *r < self.rows.len());
    }

    pub fn status(&self) -> (&str, Tone) {
        (&self.status.0, self.status.1)
    }

    pub fn set_status(&mut self, msg: impl Into<String>, tone: Tone) {
        self.status = (msg.into(), tone);
    }

    /// How many errors have been reported (a front end flashes the bar when this grows).
    pub fn error_count(&self) -> u32 {
        self.errors
    }

    pub fn show_hidden(&self) -> bool {
        self.show_hidden
    }

    pub fn set_show_hidden(&mut self, on: bool) {
        self.show_hidden = on;
        self.analysed_for = None;
        self.reanalyze();
    }

    // ----- requests to the front end -----

    /// A selection the text field should adopt (ghost, select-all, caret at end).
    pub fn take_selection_request(&mut self) -> Option<Sel> {
        self.sel_request.take()
    }

    /// A list row to scroll into view.
    pub fn take_scroll_request(&mut self) -> Option<usize> {
        self.scroll_request.take()
    }

    /// The bar should give up focus (third Esc).
    pub fn take_blur_request(&mut self) -> bool {
        std::mem::take(&mut self.blur_request)
    }

    // ----- input from the front end -----

    /// An IME started or finished composing. No completion is offered while composing.
    pub fn set_composing(&mut self, composing: bool) {
        self.composing = composing;
    }

    /// The user changed the text. `caret` is the caret position (chars) after the edit.
    pub fn edited(&mut self, text: String, caret: usize, kind: EditKind) {
        self.text = text;
        // Whatever was selected (the ghost) has been replaced or removed by this edit.
        self.ghost_chars = 0;
        self.reanalyze();
        if kind == EditKind::Insert && !self.composing && caret == self.char_len() {
            self.offer_ghost();
        }
        let more = self.analysis.completion.as_ref().map_or(0, |c| c.matches.saturating_sub(1));
        if self.analysis.mode == Mode::Path && more > 0 {
            self.set_status(format!("+{more} more match{}", if more == 1 { "" } else { "es" }), Tone::Info);
        }
    }

    /// Report the text field's selection (once per frame when nothing was edited). The ghost
    /// stays only while it is still exactly the selected tail; moving the caret commits it.
    pub fn observe_selection(&mut self, sel: Sel) {
        if self.ghost_chars == 0 {
            return;
        }
        let n = self.char_len();
        if !(sel.start() == n - self.ghost_chars && sel.end() == n) {
            self.ghost_chars = 0;
        }
    }

    pub fn key(&mut self, key: Key) {
        match key {
            Key::Tab if !self.composing => self.accept(),
            Key::Enter if !self.composing => self.submit(),
            Key::Escape => self.escape(),
            Key::Up => self.move_selection(-1),
            Key::Down => self.move_selection(1),
            Key::Tab | Key::Enter => {}
        }
    }

    // ----- internals -----

    fn ctx(&self) -> BarContext<'_> {
        BarContext { fs: &*self.fs, cwd: &self.cwd, home: &self.home, sep: self.sep, show_hidden: self.show_hidden }
    }

    fn char_len(&self) -> usize {
        self.text.chars().count()
    }

    /// Recompute the analysis (and the list) for the typed text, if it changed.
    fn reanalyze(&mut self) {
        let typed = self.typed().to_string();
        if self.analysed_for.as_deref() == Some(typed.as_str()) {
            return;
        }
        let idle = typed == self.display_cwd();
        self.analysis = if idle { Analysis::idle() } else { analyze(&typed, &self.ctx()) };
        self.analysed_for = Some(typed);
        self.refresh_view();
    }

    fn refresh_view(&mut self) {
        let dir = self.analysis.dir.clone().unwrap_or_else(|| self.cwd.clone());
        let glob = self.analysis.glob.clone();
        let key = (dir.clone(), glob.clone(), self.show_hidden);
        if self.view_key.as_ref() != Some(&key) {
            if dir != self.view_dir || self.entries.is_empty() {
                self.entries = self.fs.entries(&dir).unwrap_or_else(|| Arc::from(Vec::new()));
            }
            let g = glob.as_deref().map(Glob::new);
            self.rows = self
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| (self.show_hidden || !e.hidden) && g.as_ref().is_none_or(|g| g.is_match(&e.name)))
                .map(|(i, _)| i as u32)
                .collect();
            self.view_dir = dir;
            self.view_key = Some(key);
            self.selected = None;
        }
        // Completion highlight: only for path completions, on the row it would navigate to.
        let new_hl = match (&self.analysis.mode, &self.analysis.completion) {
            (Mode::Path, Some(c)) => self.rows.iter().position(|&i| self.entries[i as usize].name == c.name),
            _ => None,
        };
        if new_hl != self.hl_row {
            self.hl_row = new_hl;
            if new_hl.is_some() {
                self.scroll_request = new_hl;
            }
        }
    }

    /// Append the completion's ghost text, selected, so the next keystroke overwrites it.
    fn offer_ghost(&mut self) {
        let Some(c) = &self.analysis.completion else { return };
        if c.ghost.is_empty() || !matches!(self.analysis.mode, Mode::Path | Mode::Command) {
            return;
        }
        let typed_chars = self.char_len() - self.ghost_chars;
        let ghost = c.ghost.clone();
        self.text.truncate(self.typed().len());
        self.text.push_str(&ghost);
        self.ghost_chars = ghost.chars().count();
        self.sel_request = Some(Sel { anchor: typed_chars, caret: self.char_len() });
    }

    fn clear_ghost(&mut self) {
        let n = self.typed().len();
        self.text.truncate(n);
        self.ghost_chars = 0;
        self.sel_request = Some(Sel::caret(self.char_len()));
    }

    /// Tab: accept the completion (canonical casing), then offer the next level.
    fn accept(&mut self) {
        self.reanalyze();
        let Some(c) = self.analysis.completion.clone() else {
            // Nothing to complete: commit any ghost as typed text, keep focus.
            self.ghost_chars = 0;
            self.sel_request = Some(Sel::caret(self.char_len()));
            return;
        };
        self.text = c.accepted;
        self.ghost_chars = 0;
        self.reanalyze();
        self.sel_request = Some(Sel::caret(self.char_len()));
        self.offer_ghost();
    }

    /// Back to the idle bar: the current folder, everything selected, ready for the next path.
    fn go_idle(&mut self) {
        self.text = self.display_cwd();
        self.ghost_chars = 0;
        self.analysed_for = None;
        self.reanalyze();
        self.selected = None;
        self.sel_request = Some(Sel { anchor: 0, caret: self.char_len() });
    }

    /// Enter: act on the whole bar text, ghost included (as a browser's URL bar does).
    fn submit(&mut self) {
        self.analysed_for = None;
        self.reanalyze();
        let input = self.text.clone();
        let mode = analyze(&input, &self.ctx()).mode;
        match mode {
            Mode::Command => {
                self.set_status(format!("Command `{}` is not implemented yet", input.trim()), Tone::Info);
                return;
            }
            Mode::Glob => {
                self.set_status("Filter active. Esc clears it.", Tone::Good);
                return;
            }
            Mode::Empty | Mode::Path => {}
        }
        match resolve(&input, &self.ctx()) {
            Some(r) if r.is_dir => {
                self.cwd = r.path;
                self.go_idle();
                let shown = self.display_cwd();
                self.set_status(format!("Now in {shown}"), Tone::Good);
            }
            Some(r) => {
                let (dir, name) = r.path.rsplit_once('/').unwrap_or(("", &r.path));
                self.cwd = if dir.is_empty() { "/".into() } else { dir.into() };
                let name = name.to_string();
                self.go_idle();
                self.selected = self.rows.iter().position(|&i| self.entries[i as usize].name == name);
                self.scroll_request = self.selected;
                self.set_status(format!("Selected {name}"), Tone::Good);
            }
            None => {
                self.errors += 1;
                self.set_status(format!("No such path: {}", input.trim()), Tone::Bad);
            }
        }
    }

    /// Staged Esc: remove the ghost, then revert the bar, then give up focus.
    fn escape(&mut self) {
        if self.ghost_chars > 0 {
            self.clear_ghost();
        } else if self.typed() != self.display_cwd() {
            self.go_idle();
        } else {
            self.blur_request = true;
        }
    }

    fn move_selection(&mut self, dir: i32) {
        let n = self.rows.len();
        if n == 0 {
            return;
        }
        let next = match (self.selected, dir) {
            (None, d) if d > 0 => 0,
            (None, _) => n - 1,
            (Some(i), d) => ((i as i64 + i64::from(d)).rem_euclid(n as i64)) as usize,
        };
        self.selected = Some(next);
        self.scroll_request = Some(next);
    }
}
