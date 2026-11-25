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

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use crate::alias::Aliases;
use crate::command::{Command, HELP, SortDir, SortKey, parse as parse_command};
use crate::complete::{
    Analysis, BarContext, CompletionKind, Mode, ResolveError, analyze, display_path, resolve_detailed,
};
use crate::fileops::{ClipOp, Clipboard, UndoStack, parent_of};
use crate::frecency::Frecency;
use crate::fs::{Entry, Fs};
use crate::glob::Glob;
use crate::path::{fold_cmp, starts_with};
use crate::state::{PersistedState, StateStore};

mod ops;
mod select;

pub use select::SelectionInfo;
use select::Step;

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
    /// History back (Alt+Left).
    Back,
    /// History forward (Alt+Right).
    Forward,
    /// Shift+Up / Shift+Down: grow the selection.
    ExtendUp,
    ExtendDown,
    /// Ctrl+A in the list: select everything shown.
    SelectAll,
    /// Del in the list: move the selection to the trash.
    Delete,
    /// Ctrl+C / Ctrl+X / Ctrl+V in the list (in the bar they edit text instead).
    Copy,
    Cut,
    Paste,
    /// Ctrl+Z / Ctrl+Y in the list.
    Undo,
    Redo,
    /// Backspace in the list: up one folder.
    ParentFolder,
    /// Up / Down in the list: like `Up` / `Down`, but stopping at the ends (they wrap from the bar).
    ListUp,
    ListDown,
    /// Enter in the list: answer a pending question, otherwise open the selected row.
    Open,
    /// Esc in the list: cancel a pending question, otherwise clear the selection.
    ListEscape,
    /// Shift+Del in the list: delete the selection for good (asks first).
    DeletePermanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Good,
    /// Needs attention but is not an error: a partial success, or a question awaiting an answer.
    Warn,
    Bad,
}

/// Cache key for the listing: what it shows and how it is ordered.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewKey {
    dir: String,
    glob: Option<String>,
    hidden: bool,
    sort: (SortKey, SortDir),
}

pub struct Bar {
    fs: Arc<dyn Fs>,
    home: String,
    sep: char,
    cwd: String,
    show_hidden: bool,
    aliases: Aliases,
    frecency: Frecency,
    store: Option<Arc<dyn StateStore>>,
    save_failing: bool,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    sort: (SortKey, SortDir),
    /// Visited folders, oldest first; `hist_pos` is the current one.
    history: Vec<String>,
    hist_pos: usize,

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
    /// The cursor row, and the set of selected rows (multi-select).
    selected: Option<usize>,
    selection: BTreeSet<usize>,
    anchor: Option<usize>,

    clip: Option<Clipboard>,
    undo: UndoStack,
    pending: Option<ops::Pending>,
    /// The rows were just chosen with the arrow keys or a click, and the bar text has not changed
    /// since: Enter opens the selected row instead of acting on the text (brief section 9.3).
    enter_opens: bool,
    /// A file the front end should open with the system's default handler.
    open_request: Option<String>,
    log: Vec<String>,

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
            history: vec![cwd.clone()],
            hist_pos: 0,
            cwd,
            show_hidden: false,
            aliases: Aliases::new(),
            frecency: Frecency::new(),
            store: None,
            save_failing: false,
            clock: Arc::new(|| 0),
            sort: (SortKey::Name, SortDir::Asc),
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
            selection: BTreeSet::new(),
            anchor: None,
            clip: None,
            undo: UndoStack::default(),
            pending: None,
            enter_opens: false,
            open_request: None,
            log: Vec::new(),
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

    /// The clock used to age frecency (seconds since the Unix epoch). The core has no clock of
    /// its own; tests inject a fixed one. Defaults to a constant 0.
    pub fn set_clock(&mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) {
        self.clock = clock;
    }

    /// Everything that persists between sessions.
    pub fn state(&self) -> PersistedState {
        PersistedState { aliases: self.aliases.clone(), frecency: self.frecency.clone() }
    }

    pub fn frecency(&self) -> &Frecency {
        &self.frecency
    }

    /// Load aliases and frecency from `store`, and save them back as they change. Problems
    /// (a damaged file that was set aside, a file from a newer version) appear in the status line
    /// and never stop the bar from working; with a store that cannot load, nothing is persisted.
    pub fn attach_store(&mut self, store: Arc<dyn StateStore>) {
        match store.load() {
            Ok(loaded) => {
                self.aliases = loaded.state.aliases;
                self.frecency = loaded.state.frecency;
                self.store = Some(store);
                self.save_failing = false;
                self.analysed_for = None;
                self.reanalyze();
                if let Some(notice) = loaded.notice {
                    self.set_status(notice, Tone::Warn);
                }
            }
            Err(e) => self.set_status(format!("Settings were not loaded and will not be saved: {e}"), Tone::Warn),
        }
    }

    /// Save to the attached store, if any. A failure is reported once, then kept quiet until a
    /// save works again, so a read-only disk does not nag on every keystroke.
    fn save_state(&mut self) {
        let Some(store) = self.store.clone() else { return };
        match store.save(&self.state()) {
            Ok(()) => self.save_failing = false,
            Err(e) => {
                if !self.save_failing {
                    self.save_failing = true;
                    self.set_status(format!("Could not save settings: {e}"), Tone::Warn);
                }
            }
        }
    }

    fn record_visit(&mut self, path: &str) {
        let now = (self.clock)();
        self.frecency.record(path, now);
    }

    pub fn aliases(&self) -> &Aliases {
        &self.aliases
    }

    /// Replace the aliases (e.g. loaded from the config at start-up).
    pub fn set_aliases(&mut self, aliases: Aliases) {
        self.aliases = aliases;
        self.analysed_for = None;
        self.reanalyze();
    }

    /// The current sort. Folders always come first; the direction applies within each group.
    pub fn sort(&self) -> (SortKey, SortDir) {
        self.sort
    }

    pub fn set_sort(&mut self, key: SortKey, dir: SortDir) {
        self.sort = (key, dir);
        self.analysed_for = None;
        self.reanalyze();
    }

    /// A column header was clicked: sort by it, or flip the direction if it is already the sort.
    pub fn sort_by_column(&mut self, key: SortKey) {
        let dir = match self.sort {
            (k, SortDir::Asc) if k == key => SortDir::Desc,
            _ => SortDir::Asc,
        };
        self.set_sort(key, dir);
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

    /// A file to open with the system's default handler (Enter or `:open` on a file). The core
    /// cannot launch anything itself: that is the platform's job.
    pub fn take_open_request(&mut self) -> Option<String> {
        self.open_request.take()
    }

    /// Does this row also match what has been typed, besides the one Tab would go to? A front
    /// end gives such rows a fainter tint (brief section 5), so an ambiguous completion is
    /// visible. Nothing is tinted when nothing has been typed of the name yet.
    pub fn is_other_match(&self, row: usize) -> bool {
        let Some(c) = &self.analysis.completion else { return false };
        c.kind != CompletionKind::Command
            && !c.partial.is_empty()
            && Some(row) != self.hl_row
            && row < self.rows.len()
            && starts_with(&self.row(row).name, &c.partial, c.folded)
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
        self.pending = None; // typing is not an answer to "delete permanently?"
        self.enter_opens = false; // the text changed: Enter acts on it, not on the selection
        self.reanalyze();
        if kind == EditKind::Insert && !self.composing && caret == self.char_len() {
            self.offer_ghost();
        }
        if let Some(a) = self.analysis.alias.as_ref().filter(|a| a.shadows) {
            let msg = format!("Alias {} shadows the folder of the same name here", a.name);
            self.set_status(msg, Tone::Info);
            return;
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
            Key::Enter if !self.composing => {
                if self.pending.is_some() {
                    self.confirm();
                } else if self.enter_opens && self.selection_len() > 0 {
                    self.open_selected();
                } else {
                    self.submit();
                }
            }
            Key::Open => {
                if self.pending.is_some() {
                    self.confirm();
                } else {
                    self.open_selected();
                }
            }
            Key::ListEscape => {
                if !self.cancel_pending() {
                    self.clear_selection();
                }
            }
            Key::DeletePermanent => self.remove_selected(true),
            Key::Escape => {
                if !self.cancel_pending() {
                    self.escape();
                }
            }
            Key::Up => self.move_selection(-1, Step::Wrap),
            Key::Down => self.move_selection(1, Step::Wrap),
            Key::ListUp => self.move_selection(-1, Step::Clamp),
            Key::ListDown => self.move_selection(1, Step::Clamp),
            Key::ExtendUp => self.move_selection(-1, Step::Extend),
            Key::ExtendDown => self.move_selection(1, Step::Extend),
            Key::SelectAll => self.select_all_shown(),
            Key::Delete => self.remove_selected(false),
            Key::Copy => self.set_clipboard(ClipOp::Copy),
            Key::Cut => self.set_clipboard(ClipOp::Cut),
            Key::Paste => self.paste(),
            Key::Undo => self.undo_last(),
            Key::Redo => self.redo_last(),
            Key::ParentFolder => self.go_up(),
            Key::Back => {
                self.history_step(-1);
            }
            Key::Forward => {
                self.history_step(1);
            }
            Key::Tab | Key::Enter => {}
        }
    }

    // ----- internals -----

    fn ctx(&self) -> BarContext<'_> {
        BarContext {
            fs: &*self.fs,
            aliases: &self.aliases,
            frecency: &self.frecency,
            now: (self.clock)(),
            cwd: &self.cwd,
            home: &self.home,
            sep: self.sep,
            show_hidden: self.show_hidden,
        }
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
        let key = ViewKey { dir: dir.clone(), glob: glob.clone(), hidden: self.show_hidden, sort: self.sort };
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
            self.sort_rows();
            self.view_dir = dir;
            self.view_key = Some(key);
            self.clear_selection();
        }
        // Completion highlight: the entry Tab would go to (path completions, including the
        // path argument of `:cd`). Aliases and command words have no row to highlight.
        let new_hl = match (&self.analysis.mode, &self.analysis.completion) {
            (Mode::Path | Mode::Command, Some(c)) if c.kind == CompletionKind::Entry => {
                self.rows.iter().position(|&i| self.entries[i as usize].name == c.name)
            }
            _ => None,
        };
        if new_hl != self.hl_row {
            self.hl_row = new_hl;
            if new_hl.is_some() {
                self.scroll_request = new_hl;
            }
        }
    }

    /// Order the rows: folders first, then by the sort key in its direction, ties by name.
    /// The filesystem already lists by name ascending, so that case is left alone.
    fn sort_rows(&mut self) {
        let (key, dir) = self.sort;
        if (key, dir) == (SortKey::Name, SortDir::Asc) {
            return;
        }
        let entries = &self.entries;
        self.rows.sort_by(|&a, &b| {
            let (ea, eb) = (&entries[a as usize], &entries[b as usize]);
            let by_key = match key {
                SortKey::Name => fold_cmp(&ea.name, &eb.name),
                SortKey::Size => ea.size.cmp(&eb.size),
                SortKey::Date => ea.mtime.cmp(&eb.mtime),
            };
            let by_key = if dir == SortDir::Desc { by_key.reverse() } else { by_key };
            eb.is_dir.cmp(&ea.is_dir).then(by_key).then_with(|| fold_cmp(&ea.name, &eb.name))
        });
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
        self.enter_opens = false;
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
        self.clear_selection();
        self.sel_request = Some(Sel { anchor: 0, caret: self.char_len() });
    }

    /// Enter: act on the whole bar text, ghost included (as a browser's URL bar does).
    fn submit(&mut self) {
        self.analysed_for = None;
        self.reanalyze();
        let input = self.text.clone();
        if input.starts_with(':') {
            match parse_command(&input) {
                Ok(cmd) => self.run_command(cmd),
                Err(e) => self.error(e.to_string()),
            }
            return;
        }
        if analyze(&input, &self.ctx()).mode == Mode::Glob {
            self.set_status("Filter active. Esc clears it.", Tone::Good);
            return;
        }
        self.go(&input);
    }

    /// Navigate to what `input` names: a folder (go there) or a file (go to its folder, select it).
    fn go(&mut self, input: &str) {
        match resolve_detailed(input, &self.ctx()) {
            Ok(r) if r.is_dir => {
                self.record_visit(&r.path);
                self.navigate(r.path);
                let shown = self.display_cwd();
                self.set_status(format!("Now in {shown}"), Tone::Good);
                self.save_state();
            }
            Ok(r) => {
                let (dir, name) = r.path.rsplit_once('/').unwrap_or(("", &r.path));
                let dir = if dir.is_empty() { "/".to_string() } else { dir.to_string() };
                let name = name.to_string();
                self.record_visit(&r.path);
                self.navigate(dir);
                let pos = self.rows.iter().position(|&i| self.entries[i as usize].name == name);
                self.select_row(pos);
                self.scroll_request = pos;
                self.set_status(format!("Selected {name}"), Tone::Good);
                self.save_state();
            }
            Err(ResolveError::NoSuchPath) => self.error(format!("No such path: {}", input.trim())),
            Err(e @ ResolveError::DanglingAlias { .. }) => self.error(e.to_string()),
        }
    }

    /// Make `path` the current folder, record it in the history, and reset the bar.
    fn navigate(&mut self, path: String) {
        if self.history.get(self.hist_pos) != Some(&path) {
            self.history.truncate(self.hist_pos + 1);
            self.history.push(path.clone());
            self.hist_pos = self.history.len() - 1;
        }
        self.cwd = path;
        self.go_idle();
    }

    /// Alt+Left / Alt+Right, `:back` / `:fwd`.
    fn history_step(&mut self, step: i32) -> bool {
        // Folders that have since been deleted, renamed or moved are skipped (but kept, since
        // an undo can bring them back).
        let mut at = self.hist_pos as i64;
        let (target, folder) = loop {
            at += i64::from(step);
            if at < 0 || at >= self.history.len() as i64 {
                let msg = if step < 0 { "Nothing to go back to" } else { "Nothing to go forward to" };
                self.set_status(msg, Tone::Info);
                return false;
            }
            if let Some(folder) = self.fs.canonical_dir(&self.history[at as usize]) {
                break (at as usize, folder);
            }
        };
        self.hist_pos = target;
        self.cwd = folder;
        self.go_idle();
        let shown = self.display_cwd();
        self.set_status(format!("Now in {shown}"), Tone::Good);
        true
    }

    /// `:back` / `:fwd`: like the keys, but the command line is finished either way.
    fn history_command(&mut self, step: i32) {
        if !self.history_step(step) {
            let msg = self.status.0.clone();
            self.info(msg);
        }
    }

    fn error(&mut self, msg: String) {
        self.errors += 1;
        self.set_status(msg, Tone::Bad);
    }

    /// Finish a successful command: back to the idle bar, with a confirmation.
    fn done(&mut self, msg: String) {
        self.finish_line();
        self.set_status(msg, Tone::Good);
    }

    /// Reset the bar to the idle current-folder text, unless it already is (so a key such as
    /// Ctrl+C in the list keeps the selection it acted on).
    fn finish_line(&mut self) {
        if !(self.ghost_chars == 0 && self.typed() == self.display_cwd()) {
            self.go_idle();
        }
    }

    /// If the current folder is gone (deleted, renamed or moved away), fall back to the
    /// nearest folder above it that still exists. Returns whether it had to.
    fn leave_missing_folder(&mut self) -> bool {
        if let Some(c) = self.fs.canonical_dir(&self.cwd) {
            self.cwd = c;
            return false;
        }
        let mut folder = self.cwd.clone();
        self.cwd = loop {
            folder = parent_of(&folder).to_string();
            if let Some(c) = self.fs.canonical_dir(&folder) {
                break c;
            }
            if folder == "/" {
                break folder;
            }
        };
        true
    }

    /// Backspace in the list: up one folder.
    fn go_up(&mut self) {
        let parent = parent_of(&self.cwd).to_string();
        if parent == self.cwd {
            self.set_status("Already at the top", Tone::Info);
            return;
        }
        self.record_visit(&parent);
        self.navigate(parent);
        let shown = self.display_cwd();
        self.set_status(format!("Now in {shown}"), Tone::Good);
        self.save_state();
    }

    /// Finish a command that did nothing wrong but nothing either: idle bar, neutral message.
    /// (Only errors keep the command text, so it can be fixed.)
    fn info(&mut self, msg: String) {
        self.finish_line();
        self.set_status(msg, Tone::Info);
    }

    fn run_command(&mut self, cmd: Command) {
        match cmd {
            Command::Cd(path) => self.go(&path),
            Command::Back => self.history_command(-1),
            Command::Fwd => self.history_command(1),
            Command::Sort { key, dir } => {
                let dir = dir.unwrap_or(if self.sort.0 == key { self.sort.1 } else { SortDir::Asc });
                self.set_sort(key, dir);
                self.done(format!("Sorted by {}, {}", key.word(), dir_word(dir)));
            }
            Command::Asc | Command::Desc => {
                let dir = if cmd == Command::Asc { SortDir::Asc } else { SortDir::Desc };
                self.set_sort(self.sort.0, dir);
                self.done(format!("Sorted by {}, {}", self.sort.0.word(), dir_word(dir)));
            }
            Command::Hidden => {
                self.set_show_hidden(!self.show_hidden);
                let msg = if self.show_hidden { "Hidden files shown" } else { "Hidden files hidden" };
                self.done(msg.to_string());
            }
            Command::Alias(None) => {
                let ctx = self.ctx();
                let list: Vec<String> =
                    self.aliases.iter().map(|(n, t)| format!("{n} = {}", display_path(t, &ctx))).collect();
                let msg = if list.is_empty() {
                    "No aliases yet. :alias NAME PATH creates one".to_string()
                } else {
                    format!("Aliases: {}", list.join(" | "))
                };
                self.done(msg);
            }
            Command::Alias(Some((name, path))) => self.set_alias(&name, &path),
            Command::Unalias(name) => {
                if self.aliases.remove(&name, self.fs.case_insensitive()) {
                    self.analysed_for = None;
                    self.done(format!("Removed alias {name}"));
                    self.save_state();
                } else {
                    self.error(format!("No such alias: {name}"));
                }
            }
            Command::Help => self.done(HELP.to_string()),
            Command::Mkdir(name) => self.make(&name, true),
            Command::Touch(name) => self.make(&name, false),
            Command::Rename(name) => self.rename_selected(&name),
            Command::Rm { permanent } => self.remove_selected(permanent),
            Command::Copy => self.set_clipboard(ClipOp::Copy),
            Command::Cut => self.set_clipboard(ClipOp::Cut),
            Command::Paste => self.paste(),
            Command::Undo => self.undo_last(),
            Command::Redo => self.redo_last(),
            Command::Open => self.open_selected(),
            Command::Log => self.show_log(),
        }
    }

    /// `:alias NAME PATH`: the target must be an existing folder, validated now (brief section 6).
    fn set_alias(&mut self, name: &str, path: &str) {
        let target = match resolve_detailed(path, &self.ctx()) {
            Ok(r) if r.is_dir => r.path,
            _ => return self.error(format!("Alias target is not a folder: {path}")),
        };
        match self.aliases.set(name, &target) {
            Ok(replaced) => {
                self.analysed_for = None;
                let shown = display_path(&target, &self.ctx());
                let verb = if replaced { "updated" } else { "created" };
                self.done(format!("Alias {name} {verb}: {shown}. Type {name} to jump there"));
                self.save_state();
            }
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Staged Esc (brief section 10): remove the ghost, then revert the bar (which clears a
    /// filter or preview), then deselect, then give up focus.
    fn escape(&mut self) {
        if self.ghost_chars > 0 {
            self.clear_ghost();
        } else if self.typed() != self.display_cwd() {
            self.go_idle();
        } else if self.selection_len() > 0 {
            self.clear_selection();
        } else {
            self.blur_request = true;
        }
    }
}

fn dir_word(dir: SortDir) -> &'static str {
    match dir {
        SortDir::Asc => "ascending",
        SortDir::Desc => "descending",
    }
}
