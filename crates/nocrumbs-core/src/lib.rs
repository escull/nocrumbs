// SPDX-License-Identifier: GPL-3.0-or-later
//! NoCrumbs core: UI-free logic for the address bar.
//!
//! This crate has no UI and makes no direct OS calls. Everything it needs from the
//! outside world comes through the [`Fs`] trait, so the whole behaviour spec can be
//! tested headless against an in-memory filesystem (`nocrumbs-testfs`).
//!
//! **Status: M0 spike.** The bar state machine ([`Bar`]), path/command completion
//! ([`analyze`]), path resolution ([`resolve`]), glob matching ([`Glob`]) and a headless text
//! field for keystroke tests ([`sim::Session`]) exist. Aliases, real commands, history and file
//! operations do not yet.

mod alias;
mod bar;
mod command;
mod complete;
mod fileops;
mod fs;
mod glob;
mod names;
mod path;
pub mod sim;

pub use alias::Aliases;
pub use bar::{Bar, EditKind, Key, Sel, Tone};
pub use command::{Command, CommandError, HELP, SortDir, SortKey, parse as parse_command};
pub use complete::{
    AliasUse, Analysis, BarContext, Completion, CompletionKind, Mode, ResolveError, Resolved, analyze, display_path,
    resolve, resolve_detailed,
};
pub use fileops::{ClipItem, ClipOp, Clipboard, UndoStack, split_ext, unique_name};
pub use fs::{Entry, Fs, FsError, TrashToken};
pub use glob::Glob;
pub use names::{AliasNameError, NameError, NameRules, validate_alias_name, validate_name};
pub use path::{fold_cmp, is_sep};
