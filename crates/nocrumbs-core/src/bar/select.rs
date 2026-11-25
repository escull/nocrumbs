// SPDX-License-Identifier: GPL-3.0-or-later
//! Row selection (brief §9.1, §9.2): a cursor row plus a set of rows, so `Shift`+click and
//! `Shift`+arrows extend, `Ctrl`+click toggles and `Ctrl+A` takes everything shown.
//!
//! The selection is the **only** target of destructive operations; the completion highlight
//! never is. Rows are indices into the shown list, so any change to the list clears it.

use super::Bar;

/// How an arrow key moves the cursor row (brief section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    /// From the bar: past one end comes out at the other.
    Wrap,
    /// In the list: stop at the ends.
    Clamp,
    /// With Shift: grow the selection, stopping at the ends (wrapping would select nearly all).
    Extend,
}

/// What the status line says about the selection (brief section 9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionInfo {
    pub count: usize,
    pub files: usize,
    pub folders: usize,
    /// Total size of the selected **files**. Folder sizes are not known without walking them.
    pub file_bytes: u64,
}

impl Bar {
    pub fn is_selected(&self, row: usize) -> bool {
        self.selection.contains(&row)
    }

    pub fn selection_len(&self) -> usize {
        self.selection.len()
    }

    /// Selected rows, in list order.
    pub fn selected_rows(&self) -> Vec<usize> {
        self.selection.iter().copied().collect()
    }

    pub fn selected_names(&self) -> Vec<&str> {
        self.selection.iter().map(|&r| self.row(r).name.as_str()).collect()
    }

    /// Count and total size of the selection, or `None` if nothing is selected.
    pub fn selection_info(&self) -> Option<SelectionInfo> {
        if self.selection.is_empty() {
            return None;
        }
        let mut info = SelectionInfo { count: self.selection.len(), files: 0, folders: 0, file_bytes: 0 };
        for &r in &self.selection {
            let e = self.row(r);
            if e.is_dir {
                info.folders += 1;
            } else {
                info.files += 1;
                info.file_bytes = info.file_bytes.saturating_add(e.size);
            }
        }
        Some(info)
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.selected = None;
        self.anchor = None;
        self.enter_opens = false;
    }

    /// Select just this row (or nothing).
    pub fn select_row(&mut self, row: Option<usize>) {
        self.clear_selection();
        if let Some(r) = row.filter(|r| *r < self.rows.len()) {
            self.selection.insert(r);
            self.selected = Some(r);
            self.anchor = Some(r);
        }
    }

    /// `Ctrl`+click: add the row, or remove it if already selected.
    pub fn toggle_row(&mut self, row: usize) {
        if row >= self.rows.len() {
            return;
        }
        if !self.selection.remove(&row) {
            self.selection.insert(row);
        }
        self.selected = Some(row);
        self.anchor = Some(row);
    }

    /// `Shift`+click or `Shift`+arrow: select from the anchor row to this one.
    pub fn extend_to(&mut self, row: usize) {
        if row >= self.rows.len() {
            return;
        }
        let anchor = self.anchor.or(self.selected).unwrap_or(row);
        let (lo, hi) = (anchor.min(row), anchor.max(row));
        self.selection = (lo..=hi).collect();
        self.selected = Some(row);
        self.anchor = Some(anchor);
    }

    /// `Ctrl+A`: everything shown, including filter results.
    pub fn select_all_shown(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.selection = (0..self.rows.len()).collect();
        self.enter_opens = true;
        self.selected.get_or_insert(0);
        self.anchor.get_or_insert(0);
        let n = self.selection.len();
        self.set_status(format!("{n} selected"), super::Tone::Info);
    }

    /// A mouse click on a row, with the modifier keys held.
    pub fn click_row(&mut self, row: usize, ctrl: bool, shift: bool) {
        match (ctrl, shift) {
            (_, true) => self.extend_to(row),
            (true, false) => self.toggle_row(row),
            (false, false) => self.select_row(Some(row)),
        }
        self.enter_opens = true;
    }

    /// Select the rows with these names (after a create, rename or paste).
    pub(super) fn select_names(&mut self, names: &[String]) {
        let ci = self.fs.case_insensitive();
        let rows: Vec<usize> = (0..self.rows.len())
            .filter(|&r| names.iter().any(|n| crate::fileops::same_name(&self.row(r).name, n, ci)))
            .collect();
        self.clear_selection();
        if let (Some(&first), Some(&last)) = (rows.first(), rows.last()) {
            self.selection = rows.into_iter().collect();
            self.selected = Some(last);
            self.anchor = Some(first);
            self.scroll_request = Some(first);
        }
    }

    /// `(name, is_dir)` of the selected rows, in list order. What operations act on.
    pub(super) fn selected_entries(&self) -> Vec<(String, bool)> {
        self.selection
            .iter()
            .map(|&r| {
                let e = self.row(r);
                (e.name.clone(), e.is_dir)
            })
            .collect()
    }

    /// Up/Down: move the cursor row, or with `Step::Extend` grow the selection to it.
    pub(super) fn move_selection(&mut self, dir: i32, step: Step) {
        let n = self.rows.len();
        if n == 0 {
            return;
        }
        let next = match (self.selected, dir) {
            (None, d) if d > 0 => 0,
            (None, _) => n - 1,
            (Some(i), d) if step == Step::Wrap => ((i as i64 + i64::from(d)).rem_euclid(n as i64)) as usize,
            (Some(i), d) => (i as i64 + i64::from(d)).clamp(0, n as i64 - 1) as usize,
        };
        if step == Step::Extend {
            self.extend_to(next);
        } else {
            self.select_row(Some(next));
        }
        self.enter_opens = true;
        self.scroll_request = Some(next);
    }
}
