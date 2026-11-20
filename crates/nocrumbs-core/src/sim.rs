// SPDX-License-Identifier: GPL-3.0-or-later
//! A headless text field wired to a [`Bar`], for keystroke-script tests.
//!
//! [`Session`] plays the part a real front end plays: it owns the caret and selection, applies
//! typed characters, Backspace/Delete and caret movement to the bar's text, reports each change
//! to the bar, and obeys the bar's selection requests. Nothing here draws anything.

use crate::bar::{Bar, EditKind, Key, Sel};

#[derive(Debug)]
pub struct Session {
    bar: Bar,
    sel: Sel,
    blurred: bool,
}

impl Session {
    pub fn new(bar: Bar) -> Self {
        let mut s = Self { bar, sel: Sel::caret(0), blurred: false };
        s.sync();
        s
    }

    pub fn bar(&self) -> &Bar {
        &self.bar
    }

    pub fn bar_mut(&mut self) -> &mut Bar {
        &mut self.bar
    }

    /// The text in the field, ghost included.
    pub fn text(&self) -> &str {
        self.bar.text()
    }

    /// The field's current selection, in characters.
    pub fn selection(&self) -> Sel {
        self.sel
    }

    /// Has the bar asked to give up focus (third Esc)?
    pub fn blurred(&self) -> bool {
        self.blurred
    }

    /// Type text one character at a time, as a person does.
    pub fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            self.insert(&c.to_string());
        }
    }

    /// Paste: insert all of `s` at once.
    pub fn paste(&mut self, s: &str) {
        self.insert(s);
    }

    /// Backspace: delete the selection, or the character before the caret.
    pub fn backspace(&mut self) {
        let (start, end) = (self.sel.start(), self.sel.end());
        let (from, to) = if start != end { (start, end) } else { (start.saturating_sub(1), start) };
        self.replace(from, to, "", EditKind::Delete);
    }

    /// Delete: delete the selection, or the character after the caret.
    pub fn delete(&mut self) {
        let (start, end) = (self.sel.start(), self.sel.end());
        let n = self.text().chars().count();
        let (from, to) = if start != end { (start, end) } else { (start, (start + 1).min(n)) };
        self.replace(from, to, "", EditKind::Delete);
    }

    pub fn key(&mut self, key: Key) {
        self.bar.key(key);
        self.sync();
    }

    pub fn left(&mut self) {
        let at = if self.sel.is_empty() { self.sel.caret.saturating_sub(1) } else { self.sel.start() };
        self.move_caret(at);
    }

    pub fn right(&mut self) {
        let n = self.text().chars().count();
        let at = if self.sel.is_empty() { (self.sel.caret + 1).min(n) } else { self.sel.end() };
        self.move_caret(at);
    }

    pub fn home(&mut self) {
        self.move_caret(0);
    }

    pub fn end(&mut self) {
        let n = self.text().chars().count();
        self.move_caret(n);
    }

    pub fn select_all(&mut self) {
        let n = self.text().chars().count();
        self.sel = Sel { anchor: 0, caret: n };
        self.bar.observe_selection(self.sel);
    }

    pub fn set_composing(&mut self, on: bool) {
        self.bar.set_composing(on);
    }

    fn move_caret(&mut self, at: usize) {
        self.sel = Sel::caret(at);
        self.bar.observe_selection(self.sel);
    }

    fn insert(&mut self, s: &str) {
        let (start, end) = (self.sel.start(), self.sel.end());
        self.replace(start, end, s, EditKind::Insert);
    }

    /// Replace chars `from..to` with `with`, put the caret after it, tell the bar.
    fn replace(&mut self, from: usize, to: usize, with: &str, kind: EditKind) {
        let chars: Vec<char> = self.text().chars().collect();
        let mut out: String = chars[..from].iter().collect();
        out.push_str(with);
        out.extend(&chars[to..]);
        let caret = from + with.chars().count();
        self.sel = Sel::caret(caret);
        self.bar.edited(out, caret, kind);
        self.sync();
    }

    /// Apply the bar's requests, then report the selection as a front end does every frame.
    fn sync(&mut self) {
        if let Some(sel) = self.bar.take_selection_request() {
            self.sel = sel;
        }
        if self.bar.take_blur_request() {
            self.blurred = true;
        }
        let n = self.text().chars().count();
        self.sel = Sel { anchor: self.sel.anchor.min(n), caret: self.sel.caret.min(n) };
        self.bar.observe_selection(self.sel);
    }
}
