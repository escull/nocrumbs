// SPDX-License-Identifier: GPL-3.0-or-later
//! Interpreting what is typed in the bar, and offering the inline completion.
//!
//! Resolution order (brief §4), deterministic and shown by a mode chip:
//!
//! 1. **command**: input starts with `:`;
//! 2. **glob**: the last path segment contains `*` or `?`;
//! 3. **alias**: the first path segment is a saved alias;
//! 4. **literal path**: everything else.
//!
//! An alias beats a real folder of the same name (it is step 3, before step 4); the bar says so.

use std::cmp::Ordering;
use std::fmt;
use std::sync::OnceLock;

use crate::alias::Aliases;
use crate::frecency::Frecency;
use crate::fs::Fs;
use crate::glob::Glob;
use crate::path::{fold_cmp, is_sep, normalize, starts_with};

/// Everything [`analyze`] needs to know about the world.
pub struct BarContext<'a> {
    pub fs: &'a dyn Fs,
    pub aliases: &'a Aliases,
    /// What the user has used, for ranking completions (brief section 5.1).
    pub frecency: &'a Frecency,
    /// The current time in seconds since the Unix epoch (the core has no clock of its own).
    pub now: i64,
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
            .field("aliases", &self.aliases.len())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    /// A file or folder in the folder being typed into.
    Entry,
    /// A saved alias (first path segment only).
    Alias,
    /// A command word or keyword.
    Command,
}

/// The best completion for what has been typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Canonical name of the winning entry, alias or command text.
    pub name: String,
    pub kind: CompletionKind,
    pub is_dir: bool,
    /// Text to show after what the user typed, as selected ghost text. May be empty.
    /// The user's typed characters are never altered while ghosting.
    pub ghost: String,
    /// The whole bar text after Tab: canonical casing, trailing separator on directories.
    pub accepted: String,
    /// How many candidates matched (so the UI can say `+N more`).
    pub matches: usize,
}

/// An alias the typed input goes through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasUse {
    pub name: String,
    pub target: String,
    /// A real entry of the current folder has the same name and is being shadowed.
    pub shadows: bool,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub mode: Mode,
    /// The folder the typed path points into (what the list should preview), if it exists.
    pub dir: Option<String>,
    /// The glob pattern, in `Mode::Glob`.
    pub glob: Option<String>,
    /// The alias in use, if the first segment is one (the "alias" chip).
    pub alias: Option<AliasUse>,
    pub completion: Option<Completion>,
}

impl Analysis {
    /// The analysis of the idle bar (showing the current folder): nothing typed, nothing offered.
    pub fn idle() -> Self {
        Self::of(Mode::Empty)
    }

    fn of(mode: Mode) -> Self {
        Self { mode, dir: None, glob: None, alias: None, completion: None }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    NoSuchPath,
    /// The alias exists but its target folder is gone.
    DanglingAlias {
        name: String,
        target: String,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoSuchPath => write!(f, "No such path"),
            Self::DanglingAlias { name, target } => {
                write!(f, "Alias {name} points to {target}, which no longer exists. :unalias {name} to remove it")
            }
        }
    }
}

impl std::error::Error for ResolveError {}

pub fn analyze(input: &str, ctx: &BarContext<'_>) -> Analysis {
    if input.is_empty() {
        return Analysis::idle();
    }
    if input.starts_with(':') {
        return analyze_command(input, ctx);
    }
    analyze_path(input, ctx)
}

fn analyze_path(input: &str, ctx: &BarContext<'_>) -> Analysis {
    let (head, partial) = split_last(input);
    if Glob::is_glob(head) {
        // A glob in a directory position (`*/x`) is not supported.
        return Analysis::of(Mode::Path);
    }
    let mut dir = resolve_head(head, ctx).ok();
    let mut alias = alias_in_head(head, ctx);

    if Glob::is_glob(partial) {
        let glob = Some(partial.to_string());
        return Analysis { mode: Mode::Glob, dir, glob, alias, completion: None };
    }

    // A whole first segment that is exactly an alias previews the alias target.
    if head.is_empty()
        && let Some(a) = exact_alias(partial, ctx)
    {
        if let Some(c) = ctx.fs.canonical_dir(&a.target) {
            dir = Some(c);
        }
        alias = Some(a);
    }
    let completion = dir.as_deref().and_then(|_| complete_entry(head, partial, ctx));
    Analysis { mode: Mode::Path, dir, glob: None, alias, completion }
}

fn analyze_command(input: &str, ctx: &BarContext<'_>) -> Analysis {
    // `:cd PATH` and the PATH of `:alias NAME PATH` complete like a path.
    if let Some((prefix, arg)) = command_path_arg(input) {
        if Glob::is_glob(arg) {
            return Analysis::of(Mode::Command);
        }
        let inner = analyze_path(arg, ctx);
        let completion = inner.completion.map(|mut c| {
            c.accepted = format!("{prefix}{}", c.accepted);
            c
        });
        return Analysis { mode: Mode::Command, dir: inner.dir, glob: None, alias: inner.alias, completion };
    }
    Analysis { completion: complete_command(input), ..Analysis::of(Mode::Command) }
}

/// For `:cd foo` and `:alias name foo`: the text up to the path argument, and the argument.
fn command_path_arg(input: &str) -> Option<(&str, &str)> {
    let body = input.strip_prefix(':')?;
    let (word, rest) = body.split_once(char::is_whitespace)?;
    let arg = match word.to_lowercase().as_str() {
        "cd" => rest.trim_start(),
        "alias" => {
            let rest = rest.trim_start();
            let (name, tail) = rest.split_once(char::is_whitespace)?;
            if name.is_empty() || name.starts_with('"') {
                return None;
            }
            tail.trim_start()
        }
        _ => return None,
    };
    if arg.starts_with('"') {
        return None;
    }
    Some((&input[..input.len() - arg.len()], arg))
}

/// Resolve a finished input (what Enter acts on) to an existing entry.
pub fn resolve(input: &str, ctx: &BarContext<'_>) -> Option<Resolved> {
    resolve_detailed(input, ctx).ok()
}

/// Like [`resolve`], but says *why* it failed (a dangling alias is not a typo).
pub fn resolve_detailed(input: &str, ctx: &BarContext<'_>) -> Result<Resolved, ResolveError> {
    let dir = |path: String| Resolved { path, is_dir: true };
    if input.is_empty() {
        return ctx.fs.canonical_dir(ctx.cwd).map(dir).ok_or(ResolveError::NoSuchPath);
    }
    if input == "~" {
        return ctx.fs.canonical_dir(ctx.home).map(dir).ok_or(ResolveError::NoSuchPath);
    }
    let (head, last) = split_last(input);
    if last.is_empty() {
        return resolve_head(head, ctx).map(dir);
    }
    if last == "." || last == ".." {
        return resolve_head(&format!("{input}{}", ctx.sep), ctx).map(dir);
    }
    // An alias beats a real entry of the same name (resolution order, brief §4).
    if head.is_empty()
        && let Some(a) = exact_alias(last, ctx)
    {
        return match ctx.fs.canonical_dir(&a.target) {
            Some(c) => Ok(dir(c)),
            None => Err(ResolveError::DanglingAlias { name: a.name, target: a.target }),
        };
    }
    let parent = resolve_head(head, ctx)?;
    let ci = ctx.fs.case_insensitive();
    let entries = ctx.fs.entries(&parent).ok_or(ResolveError::NoSuchPath)?;
    let e = entries
        .iter()
        .find(|e| if ci { fold_cmp(&e.name, last) == Ordering::Equal } else { e.name == last })
        .ok_or(ResolveError::NoSuchPath)?;
    let path = if parent == "/" { format!("/{}", e.name) } else { format!("{parent}/{}", e.name) };
    Ok(Resolved { path, is_dir: e.is_dir })
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

/// The alias that `name` is exactly (a whole first segment).
fn exact_alias(name: &str, ctx: &BarContext<'_>) -> Option<AliasUse> {
    if name.is_empty() {
        return None;
    }
    let ci = ctx.fs.case_insensitive();
    let (n, target) = ctx.aliases.get(name, ci)?;
    let shadows = ctx
        .fs
        .entries(ctx.cwd)
        .is_some_and(|es| es.iter().any(|e| if ci { fold_cmp(&e.name, name).is_eq() } else { e.name == name }));
    Some(AliasUse { name: n.to_string(), target: target.to_string(), shadows })
}

/// The alias that the first segment of `head` (which ends with a separator) is, if any.
fn alias_in_head(head: &str, ctx: &BarContext<'_>) -> Option<AliasUse> {
    if head.starts_with('~') || head.starts_with(is_sep) {
        return None;
    }
    exact_alias(head.split(is_sep).next()?, ctx)
}

/// Resolve the directory part of a path (`head` is empty or ends with a separator).
fn resolve_head(head: &str, ctx: &BarContext<'_>) -> Result<String, ResolveError> {
    let missing = || ResolveError::NoSuchPath;
    if head.is_empty() {
        return ctx.fs.canonical_dir(ctx.cwd).ok_or_else(missing);
    }
    let (base, rest, alias) = if head.starts_with('~') && head[1..].starts_with(is_sep) {
        (ctx.home.to_string(), &head[1..], None)
    } else if head.starts_with(is_sep) {
        ("/".to_string(), head, None)
    } else if let Some(a) = alias_in_head(head, ctx) {
        let first_len = head.split(is_sep).next().map_or(0, str::len);
        (a.target.clone(), &head[first_len..], Some(a))
    } else {
        (ctx.cwd.to_string(), head, None)
    };
    let norm = normalize(&base, rest.split(is_sep));
    match ctx.fs.canonical_dir(&norm) {
        Some(c) => Ok(c),
        None => match alias {
            Some(a) if ctx.fs.canonical_dir(&a.target).is_none() => {
                Err(ResolveError::DanglingAlias { name: a.name, target: a.target })
            }
            _ => Err(missing()),
        },
    }
}

/// Complete the last segment: entries of the folder `head` points into and, for the first
/// segment, alias names too.
fn complete_entry(head: &str, partial: &str, ctx: &BarContext<'_>) -> Option<Completion> {
    let dir = resolve_head(head, ctx).ok()?;
    let entries = ctx.fs.entries(&dir)?;
    let want_hidden = ctx.show_hidden || partial.starts_with('.');
    let ci = ctx.fs.case_insensitive();
    let first_segment = head.is_empty();
    let scores = ctx.frecency.scores_in(&dir, ctx.now);

    // Case-sensitive filesystems prefer exact-case matches, then fold.
    let mut best: Option<Cand<'_>> = None;
    let mut count = 0usize;
    for pass in 0..2 {
        let fold = ci || pass == 1;
        for e in entries.iter() {
            if (e.hidden && !want_hidden) || !starts_with(&e.name, partial, fold) {
                continue;
            }
            count += 1;
            let score = if scores.is_empty() { 0 } else { scores.get(e.name.as_str()).copied().unwrap_or(0) };
            let cand = Cand { name: e.name.as_str(), kind: CompletionKind::Entry, is_dir: e.is_dir, score };
            best = Some(pick(best, cand));
        }
        if first_segment {
            for (name, _) in ctx.aliases.iter() {
                if starts_with(name, partial, fold) {
                    count += 1;
                    best = Some(pick(best, Cand { name, kind: CompletionKind::Alias, is_dir: true, score: 0 }));
                }
            }
        }
        if count > 0 || ci {
            break;
        }
    }
    let Cand { name, kind, is_dir, .. } = best?;

    let sep = head.chars().last().filter(|c| is_sep(*c)).unwrap_or(ctx.sep);
    let mut ghost: String = name.chars().skip(partial.chars().count()).collect();
    let mut accepted = format!("{head}{name}");
    if is_dir {
        ghost.push(sep);
        accepted.push(sep);
    }
    Some(Completion { name: name.to_string(), kind, is_dir, ghost, accepted, matches: count })
}

/// One completion candidate, with how much the user has used it.
#[derive(Clone, Copy)]
struct Cand<'a> {
    name: &'a str,
    kind: CompletionKind,
    is_dir: bool,
    score: u32,
}

/// The better of two candidates (brief section 5.1): the one the user has used more, and more
/// recently (frecency); then alphabetical ignoring case; an alias before an entry of the same
/// name (the alias wins resolution); then exact spelling.
fn pick<'a>(best: Option<Cand<'a>>, cand: Cand<'a>) -> Cand<'a> {
    let Some(b) = best else { return cand };
    let rank = |k: CompletionKind| u8::from(k != CompletionKind::Alias);
    let ord = b
        .score
        .cmp(&cand.score)
        .then_with(|| fold_cmp(cand.name, b.name))
        .then_with(|| rank(cand.kind).cmp(&rank(b.kind)))
        .then_with(|| cand.name.cmp(b.name));
    if ord == Ordering::Less { cand } else { b }
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
        kind: CompletionKind::Command,
        is_dir: false,
        ghost: best.chars().skip(input.chars().count()).collect(),
        accepted: best.clone(),
        matches,
    })
}
