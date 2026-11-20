// SPDX-License-Identifier: GPL-3.0-or-later
//! The building blocks of file operations (brief §9): collision-free names, the clipboard, and
//! an undo stack made of reversible steps. None of it touches the filesystem except through
//! [`Fs`].

use crate::fs::{Fs, FsError, TrashToken};
use crate::path::fold_cmp;

/// `dir` + `name`, with `/` as the root.
pub fn join(dir: &str, name: &str) -> String {
    if dir == "/" { format!("/{name}") } else { format!("{dir}/{name}") }
}

/// The folder part of a full path (`/a/b` -> `/a`, `/a` -> `/`).
pub fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) | None => "/",
        Some(i) => &path[..i],
    }
}

/// Is `dir` the folder `ancestor`, or somewhere inside it? (Pasting a folder there is refused.)
pub fn is_inside(dir: &str, ancestor: &str, case_insensitive: bool) -> bool {
    let norm = |s: &str| {
        let s = s.trim_end_matches('/');
        if case_insensitive { s.to_lowercase() } else { s.to_string() }
    };
    let (d, a) = (norm(dir), norm(ancestor));
    d == a || d.starts_with(&format!("{a}/"))
}

/// Split `name.ext` at the last dot; a leading dot (`.zshrc`) or trailing dot is not an extension.
pub fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

/// `name`, or `name (2)`, `name (3)`, ... before the extension, until `taken` says it is free.
pub fn unique_name(name: &str, is_dir: bool, taken: &dyn Fn(&str) -> bool) -> String {
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = if is_dir { (name, "") } else { split_ext(name) };
    (2..)
        .map(|i| format!("{stem} ({i}){ext}"))
        .find(|candidate| !taken(candidate))
        .expect("an unbounded range always yields")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipOp {
    Copy,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipItem {
    /// Full path, kept current if the item is renamed (see [`Clipboard::renamed`]).
    pub path: String,
    pub name: String,
    pub is_dir: bool,
}

/// What `:copy` / `:cut` put aside for `:paste`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipboard {
    pub op: ClipOp,
    pub items: Vec<ClipItem>,
}

impl Clipboard {
    /// A short description for the status bar: `Copied notes.md`, `Cut 3 items`.
    pub fn summary(&self) -> String {
        let verb = match self.op {
            ClipOp::Copy => "Copied",
            ClipOp::Cut => "Cut",
        };
        match self.items.as_slice() {
            [one] => format!("{verb} {}", one.name),
            many => format!("{verb} {} items", many.len()),
        }
    }

    /// An item was renamed: follow it, so a later paste does not silently duplicate or lose it.
    pub fn renamed(&mut self, from: &str, to: &str) {
        for item in &mut self.items {
            if item.path == from {
                item.path = to.to_string();
                item.name = to.rsplit('/').next().unwrap_or(to).to_string();
            } else if let Some(rest) = item.path.strip_prefix(&format!("{from}/")) {
                item.path = format!("{to}/{rest}");
            }
        }
    }
}

/// One reversible action. A step knows whether its result currently exists, so undo and redo
/// are the same two operations in opposite order.
#[derive(Debug, Clone)]
pub enum Step {
    /// Something was created (a folder, a file, or a pasted copy). Undo trashes it.
    Created {
        path: String,
        trashed: Option<TrashToken>,
    },
    /// Something was moved to the trash. Undo restores it.
    Trashed {
        path: String,
        token: Option<TrashToken>,
    },
    Renamed {
        dir: String,
        from: String,
        to: String,
    },
    Moved {
        from_dir: String,
        from_name: String,
        to_dir: String,
        to_name: String,
    },
}

impl Step {
    /// Undo this step.
    pub fn revert(&mut self, fs: &dyn Fs) -> Result<(), FsError> {
        match self {
            Self::Created { path, trashed } => {
                *trashed = Some(fs.trash(path)?);
                Ok(())
            }
            Self::Trashed { token, .. } => restore(fs, token),
            Self::Renamed { dir, from, to } => fs.rename(dir, to, from),
            Self::Moved { from_dir, from_name, to_dir, to_name } => {
                fs.move_entry(&join(to_dir, to_name), from_dir, from_name)
            }
        }
    }

    /// Redo this step (after it was reverted).
    pub fn apply(&mut self, fs: &dyn Fs) -> Result<(), FsError> {
        match self {
            Self::Created { trashed, .. } => restore(fs, trashed),
            Self::Trashed { path, token } => {
                *token = Some(fs.trash(path)?);
                Ok(())
            }
            Self::Renamed { dir, from, to } => fs.rename(dir, from, to),
            Self::Moved { from_dir, from_name, to_dir, to_name } => {
                fs.move_entry(&join(from_dir, from_name), to_dir, to_name)
            }
        }
    }
}

fn restore(fs: &dyn Fs, token: &mut Option<TrashToken>) -> Result<(), FsError> {
    let t = token.ok_or_else(|| FsError::Other("Nothing to restore".to_string()))?;
    fs.restore(t)?;
    *token = None;
    Ok(())
}

/// One undoable action: a label for the status line and the steps that make it up.
#[derive(Debug, Clone)]
pub struct UndoEntry {
    pub label: String,
    pub steps: Vec<Step>,
}

/// Undo and redo history (brief §9.5: depth of at least 20; cleared on exit).
#[derive(Debug, Clone)]
pub struct UndoStack {
    done: Vec<UndoEntry>,
    undone: Vec<UndoEntry>,
    limit: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(100)
    }
}

impl UndoStack {
    pub fn new(limit: usize) -> Self {
        Self { done: Vec::new(), undone: Vec::new(), limit: limit.max(1) }
    }

    /// Record something just done. A new action ends the redo history.
    pub fn push(&mut self, label: impl Into<String>, steps: Vec<Step>) {
        if steps.is_empty() {
            return;
        }
        self.undone.clear();
        self.done.push(UndoEntry { label: label.into(), steps });
        if self.done.len() > self.limit {
            self.done.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.done.last().map(|e| e.label.as_str())
    }

    /// Undo the last action. `Ok(None)` means there was nothing to undo. If a step fails, the
    /// steps already reverted become redoable and the rest stay undoable, so nothing is lost.
    pub fn undo(&mut self, fs: &dyn Fs) -> Result<Option<String>, FsError> {
        let Some(mut entry) = self.done.pop() else { return Ok(None) };
        let mut reverted: Vec<Step> = Vec::new();
        while let Some(mut step) = entry.steps.pop() {
            match step.revert(fs) {
                Ok(()) => reverted.insert(0, step),
                Err(e) => {
                    entry.steps.push(step);
                    self.done.push(entry.clone());
                    if !reverted.is_empty() {
                        self.undone.push(UndoEntry { label: format!("{} (partly)", entry.label), steps: reverted });
                    }
                    return Err(e);
                }
            }
        }
        let label = entry.label.clone();
        self.undone.push(UndoEntry { label: entry.label, steps: reverted });
        Ok(Some(label))
    }

    /// Redo the last undone action.
    pub fn redo(&mut self, fs: &dyn Fs) -> Result<Option<String>, FsError> {
        let Some(mut entry) = self.undone.pop() else { return Ok(None) };
        let mut applied: Vec<Step> = Vec::new();
        let mut remaining = std::mem::take(&mut entry.steps).into_iter();
        while let Some(mut step) = remaining.next() {
            match step.apply(fs) {
                Ok(()) => applied.push(step),
                Err(e) => {
                    let mut rest = vec![step];
                    rest.extend(remaining);
                    self.undone.push(UndoEntry { label: entry.label.clone(), steps: rest });
                    if !applied.is_empty() {
                        self.done.push(UndoEntry { label: format!("{} (partly)", entry.label), steps: applied });
                    }
                    return Err(e);
                }
            }
        }
        let label = entry.label.clone();
        self.done.push(UndoEntry { label: entry.label, steps: applied });
        Ok(Some(label))
    }
}

/// Is `a` the same name as `b` on this filesystem?
pub fn same_name(a: &str, b: &str, case_insensitive: bool) -> bool {
    if case_insensitive { fold_cmp(a, b).is_eq() } else { a == b }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        assert_eq!(join("/", "a"), "/a");
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(parent_of("/a/b"), "/a");
        assert_eq!(parent_of("/a"), "/");
        assert!(is_inside("/a/b/c", "/a/b", false));
        assert!(is_inside("/a/b", "/a/b", false));
        assert!(!is_inside("/a/bc", "/a/b", false), "a sibling with the same prefix is not inside");
        assert!(is_inside("/A/B/c", "/a/b", true));
        assert!(!is_inside("/A/B/c", "/a/b", false));
    }

    #[test]
    fn extensions() {
        assert_eq!(split_ext("a.tar.gz"), ("a.tar", ".gz"));
        assert_eq!(split_ext(".zshrc"), (".zshrc", ""));
        assert_eq!(split_ext("name."), ("name.", ""));
        assert_eq!(split_ext("README"), ("README", ""));
    }

    #[test]
    fn unique_names() {
        let taken = |n: &str| ["a.txt", "a (2).txt", "dir", "dir (2)", ".zshrc"].contains(&n);
        assert_eq!(unique_name("fresh.txt", false, &taken), "fresh.txt");
        assert_eq!(unique_name("a.txt", false, &taken), "a (3).txt");
        assert_eq!(unique_name("dir", true, &taken), "dir (3)");
        assert_eq!(unique_name(".zshrc", false, &taken), ".zshrc (2)");
        assert_eq!(unique_name("holiday (1).jpg", false, &|n| n == "holiday (1).jpg"), "holiday (1) (2).jpg");
    }

    #[test]
    fn clipboard_follows_renames() {
        let mut c = Clipboard {
            op: ClipOp::Cut,
            items: vec![
                ClipItem { path: "/a/old".into(), name: "old".into(), is_dir: true },
                ClipItem { path: "/a/old/inner.txt".into(), name: "inner.txt".into(), is_dir: false },
                ClipItem { path: "/a/other".into(), name: "other".into(), is_dir: false },
            ],
        };
        c.renamed("/a/old", "/a/new");
        assert_eq!(c.items[0].path, "/a/new");
        assert_eq!(c.items[0].name, "new");
        assert_eq!(c.items[1].path, "/a/new/inner.txt");
        assert_eq!(c.items[2].path, "/a/other");
        assert_eq!(c.summary(), "Cut 3 items");
        c.items.truncate(1);
        assert_eq!(c.summary(), "Cut new");
    }
}
