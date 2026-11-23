// SPDX-License-Identifier: GPL-3.0-or-later
//! File operations driven from the bar (brief §8, §9): `:mkdir`, `:touch`, `:rename`, `:rm`,
//! `:copy`/`:cut`/`:paste`, `:undo`/`:redo`, `:log`.
//!
//! Every operation acts on the **selection** and nothing else, goes through [`Fs`], and is
//! undoable except `:rm!`, which always asks first.

use std::collections::HashSet;

use super::{Bar, Tone};
use crate::fileops::{ClipItem, ClipOp, Clipboard, Step, is_inside, join, parent_of, same_name, unique_name};
use crate::fs::FsError;
use crate::names::validate_name;

/// Something waiting for an explicit yes.
#[derive(Debug, Clone)]
pub(super) enum Pending {
    /// Full paths to delete for good.
    DeletePermanently(Vec<String>),
}

fn describe(names: &[String]) -> String {
    match names {
        [one] => one.clone(),
        many => format!("{} items", many.len()),
    }
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl Bar {
    pub fn clipboard(&self) -> Option<&Clipboard> {
        self.clip.as_ref()
    }

    /// For the status bar: `Copied notes.md`, `Cut 3 items`.
    pub fn clipboard_summary(&self) -> Option<String> {
        self.clip.as_ref().map(Clipboard::summary)
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// Every operation this session, oldest first.
    pub fn operation_log(&self) -> &[String] {
        &self.log
    }

    /// Re-read the shown folder (after a change, or when a watcher reports one).
    pub fn refresh(&mut self) {
        self.entries = std::sync::Arc::from(Vec::new());
        self.view_key = None;
        self.analysed_for = None;
        self.reanalyze();
    }

    fn log_line(&mut self, line: String) {
        self.log.push(line);
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn record(&mut self, label: String, steps: Vec<Step>) {
        self.log_line(label.clone());
        self.undo.push(label, steps);
    }

    /// Report a filesystem failure, phrased for the status line.
    fn fs_error(&mut self, e: FsError) {
        let msg = match &e {
            FsError::AlreadyExists(p) => format!("{} already exists here", base_name(p)),
            other => other.to_string(),
        };
        self.error(msg);
    }

    fn exists_in(&self, dir: &str, name: &str) -> bool {
        let ci = self.fs.case_insensitive();
        self.fs.entries(dir).is_some_and(|es| es.iter().any(|e| same_name(&e.name, name, ci)))
    }

    // ----- create and rename -----

    /// `:mkdir NAME` / `:touch NAME`, in the current folder.
    pub(super) fn make(&mut self, name: &str, is_dir: bool) {
        if let Err(e) = validate_name(name, self.fs.name_rules()) {
            return self.error(e.to_string());
        }
        let dir = self.cwd.clone();
        if self.exists_in(&dir, name) {
            return self.error(format!("{name} already exists here"));
        }
        let made = if is_dir { self.fs.create_dir(&dir, name) } else { self.fs.create_file(&dir, name) };
        if let Err(e) = made {
            return self.fs_error(e);
        }
        let what = if is_dir { "folder" } else { "file" };
        let msg = format!("Created {what} {name}");
        self.record(msg.clone(), vec![Step::Created { path: join(&dir, name), trashed: None }]);
        self.refresh();
        self.done(msg);
        self.select_names(&[name.to_string()]);
    }

    /// `:rename NEWNAME`: the one selected entry.
    pub(super) fn rename_selected(&mut self, new_name: &str) {
        // The name is checked first: it is the thing the user just typed.
        if let Err(e) = validate_name(new_name, self.fs.name_rules()) {
            return self.error(e.to_string());
        }
        let items = self.selected_entries();
        let old = match items.as_slice() {
            [] => return self.error("Select an entry first".to_string()),
            [(name, _)] => name.clone(),
            _ => return self.error("Select exactly one entry to rename".to_string()),
        };
        if old == new_name {
            return self.info(format!("{old} already has that name"));
        }
        let dir = self.view_dir.clone();
        if let Err(e) = self.fs.rename(&dir, &old, new_name) {
            return self.fs_error(e);
        }
        if let Some(clip) = self.clip.as_mut() {
            clip.renamed(&join(&dir, &old), &join(&dir, new_name));
        }
        self.frecency.renamed(&join(&dir, &old), &join(&dir, new_name));
        let msg = format!("Renamed {old} to {new_name}");
        self.record(msg.clone(), vec![Step::Renamed { dir, from: old, to: new_name.to_string() }]);
        self.refresh();
        self.done(msg);
        self.select_names(&[new_name.to_string()]);
        self.save_state();
    }

    // ----- delete -----

    /// `:rm` (to the trash, undoable) and `:rm!` (permanent, asks first).
    pub(super) fn remove_selected(&mut self, permanent: bool) {
        let names: Vec<String> = self.selected_entries().into_iter().map(|(n, _)| n).collect();
        if names.is_empty() {
            return self.error("Select an entry first".to_string());
        }
        let dir = self.view_dir.clone();
        let paths: Vec<String> = names.iter().map(|n| join(&dir, n)).collect();
        if permanent {
            let what = describe(&names);
            self.pending = Some(Pending::DeletePermanently(paths));
            self.set_status(
                format!("Delete {what} permanently? This cannot be undone. Enter confirms, Esc cancels"),
                Tone::Warn,
            );
            return;
        }
        let mut steps = Vec::new();
        let mut first_err = None;
        for p in &paths {
            match self.fs.trash(p) {
                Ok(token) => steps.push(Step::Trashed { path: p.clone(), token: Some(token) }),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        let moved = steps.len();
        let moved_names: Vec<String> = paths.iter().take(moved).map(|p| base_name(p).to_string()).collect();
        if moved > 0 {
            self.record(format!("Moved {} to the trash", describe(&moved_names)), steps);
        }
        self.refresh();
        match (moved, first_err) {
            (0, Some(e)) => self.fs_error(e),
            (_, Some(e)) => {
                self.set_status(format!("Moved {moved} of {} to the trash. {e}", paths.len()), Tone::Warn);
            }
            (_, None) => self.done(format!(
                "Moved {} to the trash. :undo brings {} back",
                describe(&moved_names),
                if moved == 1 { "it" } else { "them" }
            )),
        }
    }

    /// Enter on a pending confirmation: do it.
    pub(super) fn confirm(&mut self) {
        let Some(Pending::DeletePermanently(paths)) = self.pending.take() else { return };
        let (mut deleted, mut first_err) = (0usize, None);
        for p in &paths {
            match self.fs.delete_forever(p) {
                Ok(()) => deleted += 1,
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        if deleted > 0 {
            self.log_line(format!("Deleted {deleted} item(s) permanently"));
        }
        self.refresh();
        match first_err {
            Some(e) if deleted == 0 => self.fs_error(e),
            Some(e) => self.set_status(format!("Deleted {deleted} of {}. {e}", paths.len()), Tone::Warn),
            None => {
                let what = if deleted == 1 { "1 item".to_string() } else { format!("{deleted} items") };
                self.done(format!("Deleted {what} permanently"));
            }
        }
    }

    /// Esc on a pending confirmation: drop it. Returns whether there was one.
    pub(super) fn cancel_pending(&mut self) -> bool {
        if self.pending.take().is_some() {
            self.set_status("Cancelled. Nothing was deleted", Tone::Info);
            return true;
        }
        false
    }

    // ----- clipboard -----

    /// `:copy` / `:cut`: remember the selection for `:paste`. Nothing moves yet.
    pub(super) fn set_clipboard(&mut self, op: ClipOp) {
        let entries = self.selected_entries();
        if entries.is_empty() {
            return self.error("Select an entry first".to_string());
        }
        let dir = self.view_dir.clone();
        let items =
            entries.into_iter().map(|(name, is_dir)| ClipItem { path: join(&dir, &name), name, is_dir }).collect();
        let clip = Clipboard { op, items };
        let summary = clip.summary();
        self.clip = Some(clip);
        self.done(format!("{summary}. :paste drops it in the folder you are in"));
    }

    /// `:paste`: into the current folder. Collisions get `name (2)`; a folder cannot go inside
    /// itself; a cut moves, a copy leaves the original.
    pub(super) fn paste(&mut self) {
        let Some(clip) = self.clip.clone() else {
            return self.error("Clipboard is empty. :copy or :cut something first".to_string());
        };
        let dest = self.cwd.clone();
        let ci = self.fs.case_insensitive();
        let key = |n: &str| if ci { n.to_lowercase() } else { n.to_string() };
        let mut taken: HashSet<String> =
            self.fs.entries(&dest).map(|es| es.iter().map(|e| key(&e.name)).collect()).unwrap_or_default();

        let (mut steps, mut pasted, mut failed, mut errors) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut renamed = 0usize;
        let mut skipped_here = 0usize;
        for item in &clip.items {
            if item.is_dir && is_inside(&dest, &item.path, ci) {
                errors.push(format!("Cannot paste {} inside itself", item.name));
                failed.push(item.clone());
                continue;
            }
            let src_dir = parent_of(&item.path).to_string();
            if clip.op == ClipOp::Cut && is_inside(&dest, &src_dir, ci) && is_inside(&src_dir, &dest, ci) {
                skipped_here += 1; // cutting something into the folder it is already in
                continue;
            }
            let name = unique_name(&item.name, item.is_dir, &|n| taken.contains(&key(n)));
            if name != item.name {
                renamed += 1;
            }
            let done = match clip.op {
                ClipOp::Copy => self.fs.copy_entry(&item.path, &dest, &name),
                ClipOp::Cut => self.fs.move_entry(&item.path, &dest, &name),
            };
            match done {
                Ok(()) => {
                    taken.insert(key(&name));
                    steps.push(match clip.op {
                        ClipOp::Copy => Step::Created { path: join(&dest, &name), trashed: None },
                        ClipOp::Cut => Step::Moved {
                            from_dir: src_dir,
                            from_name: item.name.clone(),
                            to_dir: dest.clone(),
                            to_name: name.clone(),
                        },
                    });
                    pasted.push(name);
                }
                Err(e) => {
                    errors.push(e.to_string());
                    failed.push(item.clone());
                }
            }
        }

        if clip.op == ClipOp::Cut {
            // A successful cut is used up; items that failed stay on the clipboard.
            self.clip = if failed.is_empty() { None } else { Some(Clipboard { op: ClipOp::Cut, items: failed }) };
        }
        let verb = if clip.op == ClipOp::Cut { "Moved" } else { "Pasted" };
        let what = describe(&pasted);
        if !steps.is_empty() {
            self.record(format!("{verb} {what}"), steps);
        }
        self.refresh();
        if pasted.is_empty() {
            return match errors.into_iter().next() {
                Some(e) => self.error(e),
                None if skipped_here > 0 => self.info("Already in this folder".to_string()),
                None => self.info("Nothing to paste".to_string()),
            };
        }
        let note = match renamed {
            0 => String::new(),
            n => format!(" ({n} renamed to avoid a collision)"),
        };
        match errors.first() {
            Some(e) => {
                self.done(format!("{verb} {what}{note}"));
                self.set_status(format!("{verb} {what}{note}. {e}"), Tone::Warn);
            }
            None => self.done(format!("{verb} {what}{note}")),
        }
        self.select_names(&pasted);
    }

    // ----- undo, redo, log -----

    pub(super) fn undo_last(&mut self) {
        match self.undo.undo(&*self.fs) {
            Ok(Some(label)) => {
                self.log_line(format!("Undid: {label}"));
                self.refresh();
                self.done(format!("Undid: {label}"));
            }
            Ok(None) => self.info("Nothing to undo".to_string()),
            Err(e) => {
                self.refresh();
                self.error(format!("Undo failed: {e}"));
            }
        }
    }

    pub(super) fn redo_last(&mut self) {
        match self.undo.redo(&*self.fs) {
            Ok(Some(label)) => {
                self.log_line(format!("Redid: {label}"));
                self.refresh();
                self.done(format!("Redid: {label}"));
            }
            Ok(None) => self.info("Nothing to redo".to_string()),
            Err(e) => {
                self.refresh();
                self.error(format!("Redo failed: {e}"));
            }
        }
    }

    /// `:log`: the last few operations.
    pub(super) fn show_log(&mut self) {
        let recent: Vec<&str> = self.log.iter().rev().take(5).map(String::as_str).collect();
        let msg = if recent.is_empty() {
            "No operations yet".to_string()
        } else {
            let mut v = recent;
            v.reverse();
            format!("Recent: {}", v.join(" | "))
        };
        self.info(msg);
    }
}
