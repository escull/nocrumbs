// SPDX-License-Identifier: GPL-3.0-or-later
//! Interpreting what is typed in the bar, and offering the inline completion.
//!
//! Resolution order (brief §4): command, glob, alias, literal path. Aliases are not
//! implemented in the spike.

use std::cmp::Ordering;
use std::fmt;
use std::sync::OnceLock;

use crate::fs::{Entry, Fs};
use crate::glob::Glob;
use crate::path::{fold_cmp, is_sep, normalize, starts_with};

/// Everything [`analyze`] needs to know about the world.
pub struct BarContext<'a> {
    pub fs: &'a dyn Fs,
    /// Canonical absolute current folder.
    pub cwd: &'a str,
    /// Canonical absolute home folder (`~`).
    pub home: &'a str,
    /// Separator appended after a completed directory when the user has not typed one.
    pub sep: char,
    pub show_hidden: bool,
}

impl fmt::Debug for BarContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BarContext")
            .field("cwd", &self.cwd)
            .field("home", &self.home)
            .field("sep", &self.sep)
            .field("show_hidden", &self.show_hidden)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Empty,
    Command,
    Glob,
    Path,
}

/// The best completion for what has been typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Canonical name of the winning entry (or the command text).
    pub name: String,
    pub is_dir: bool,
    /// Text to show after what the user typed, as selected ghost text. May be empty.
    /// The user's typed characters are never altered while ghosting.
    pub ghost: String,
    /// The whole bar text after Tab: canonical casing, trailing separator on directories.
    pub accepted: String,
    /// How many candidates matched (so the UI can say `+N more`).
    pub matches: usize,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub mode: Mode,
    /// The folder the typed path points into (what the list should preview), if it exists.
    pub dir: Option<String>,
    /// The glob pattern, in `Mode::Glob`.
    pub glob: Option<String>,
    pub completion: Option<Completion>,
}

impl Analysis {
    /// The analysis of the idle bar (showing the current folder): nothing typed, nothing offered.
    pub fn idle() -> Self {
        Self { mode: Mode::Empty, dir: None, glob: None, completion: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub path: String,
    pub is_dir: bool,
}

pub fn analyze(input: &str, ctx: &BarContext<'_>) -> Analysis {
    let none = |mode| Analysis { mode, dir: None, glob: None, completion: None };
    if input.is_empty() {
        return none(Mode::Empty);
    }
    if input.starts_with(':') {
        return Analysis { completion: complete_command(input), ..none(Mode::Command) };
    }

    let (head, partial) = split_last(input);
    if Glob::is_glob(head) {
        // A glob in a directory position (`*/x`) is not supported.
        return none(Mode::Path);
    }
    let dir = resolve_head(head, ctx);
    if Glob::is_glob(partial) {
        return Analysis { mode: Mode::Glob, dir, glob: Some(partial.to_string()), completion: None };
    }
    let completion = dir.as_deref().and_then(|d| complete_entry(head, partial, d, ctx));
    Analysis { mode: Mode::Path, dir, glob: None, completion }
}

/// Resolve a finished input (what Enter acts on) to an existing entry.
pub fn resolve(input: &str, ctx: &BarContext<'_>) -> Option<Resolved> {
    if input.is_empty() {
        return Some(Resolved { path: ctx.fs.canonical_dir(ctx.cwd)?, is_dir: true });
    }
    if input == "~" {
        return Some(Resolved { path: ctx.fs.canonical_dir(ctx.home)?, is_dir: true });
    }
    let (head, last) = split_last(input);
    if last.is_empty() {
        return Some(Resolved { path: resolve_head(head, ctx)?, is_dir: true });
    }
    if last == "." || last == ".." {
        let with_sep = format!("{input}{}", ctx.sep);
        return Some(Resolved { path: resolve_head(&with_sep, ctx)?, is_dir: true });
    }
    let dir = resolve_head(head, ctx)?;
    let ci = ctx.fs.case_insensitive();
    let entries = ctx.fs.entries(&dir)?;
    let e = entries.iter().find(|e| if ci { fold_cmp(&e.name, last) == Ordering::Equal } else { e.name == last })?;
    let path = if dir == "/" { format!("/{}", e.name) } else { format!("{dir}/{}", e.name) };
    Some(Resolved { path, is_dir: e.is_dir })
}

/// How a path is shown in the status line and the idle bar: `~` for home.
pub fn display_path(path: &str, ctx: &BarContext<'_>) -> String {
    if path == ctx.home {
        return "~".to_string();
    }
    if ctx.home != "/"
        && let Some(rest) = path.strip_prefix(ctx.home)
        && rest.starts_with('/')
    {
        return format!("~{rest}");
    }
    path.to_string()
}

/// `a/b/c` -> (`a/b/`, `c`); `c` -> (``, `c`).
fn split_last(input: &str) -> (&str, &str) {
    match input.rfind(is_sep) {
        Some(i) => (&input[..=i], &input[i + 1..]),
        None => ("", input),
    }
}

/// Resolve the directory part of a path (`head` is empty or ends with a separator).
fn resolve_head(head: &str, ctx: &BarContext<'_>) -> Option<String> {
    if head.is_empty() {
        return ctx.fs.canonical_dir(ctx.cwd);
    }
    let (base, rest) = if head.starts_with('~') && head[1..].starts_with(is_sep) {
        (ctx.home, &head[1..])
    } else if head.starts_with(is_sep) {
        ("/", head)
    } else {
        (ctx.cwd, head)
    };
    let norm = normalize(base, rest.split(is_sep));
    ctx.fs.canonical_dir(&norm)
}

fn complete_entry(head: &str, partial: &str, dir: &str, ctx: &BarContext<'_>) -> Option<Completion> {
    let entries = ctx.fs.entries(dir)?;
    let want_hidden = ctx.show_hidden || partial.starts_with('.');
    let ci = ctx.fs.case_insensitive();

    // Case-sensitive filesystems prefer exact-case matches, then fall back to folding.
    let mut best: Option<&Entry> = None;
    let mut count = 0usize;
    for pass in 0..2 {
        let fold = ci || pass == 1;
        for e in entries.iter() {
            if (e.hidden && !want_hidden) || !starts_with(&e.name, partial, fold) {
                continue;
            }
            count += 1;
            let better = match best {
                None => true,
                Some(b) => fold_cmp(&e.name, &b.name).then_with(|| e.name.cmp(&b.name)) == Ordering::Less,
            };
            if better {
                best = Some(e);
            }
        }
        if count > 0 || ci {
            break;
        }
    }
    let best = best?;

    let sep = head.chars().last().filter(|c| is_sep(*c)).unwrap_or(ctx.sep);
    let tail: String = best.name.chars().skip(partial.chars().count()).collect();
    let mut ghost = tail;
    let mut accepted = format!("{head}{}", best.name);
    if best.is_dir {
        ghost.push(sep);
        accepted.push(sep);
    }
    Some(Completion { name: best.name.clone(), is_dir: best.is_dir, ghost, accepted, matches: count })
}

/// Command words and their fixed keyword arguments. Placeholders such as `NAME` are
/// never offered: a command that takes free text completes to `:name ` and stops.
fn commands() -> &'static [String] {
    static COMMANDS: OnceLock<Vec<String>> = OnceLock::new();
    COMMANDS.get_or_init(|| {
        let mut v: Vec<String> = [
            ":asc", ":desc", ":hidden", ":back", ":fwd", ":rm", ":rm!", ":copy", ":cut", ":paste", ":open", ":undo",
            ":redo", ":help", ":log",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for c in ["cd", "mkdir", "touch", "rename", "alias", "unalias"] {
            v.push(format!(":{c} "));
        }
        for k in ["name", "size", "date"] {
            v.push(format!(":sort {k}"));
            for d in ["asc", "desc"] {
                v.push(format!(":sort {k} {d}"));
            }
        }
        v.sort_by(|a, b| fold_cmp(a, b));
        v
    })
}

fn complete_command(input: &str) -> Option<Completion> {
    let mut matches = 0usize;
    let mut best: Option<&String> = None;
    for c in commands() {
        if starts_with(c, input, true) {
            matches += 1;
            best.get_or_insert(c); // sorted, so the first is the alphabetical winner
        }
    }
    let best = best?;
    Some(Completion {
        name: best.clone(),
        is_dir: false,
        ghost: best.chars().skip(input.chars().count()).collect(),
        accepted: best.clone(),
        matches,
    })
}
