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

mod bar;
mod complete;
mod fs;
mod glob;
mod path;
pub mod sim;

pub use bar::{Bar, EditKind, Key, Sel, Tone};
pub use complete::{Analysis, BarContext, Completion, Mode, Resolved, analyze, display_path, resolve};
pub use fs::{Entry, Fs};
pub use glob::Glob;
pub use path::{fold_cmp, is_sep};
