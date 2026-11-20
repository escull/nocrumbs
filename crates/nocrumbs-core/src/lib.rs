// SPDX-License-Identifier: GPL-3.0-or-later
//! NoCrumbs core: UI-free logic for the address bar.
//!
//! This crate has no UI and makes no direct OS calls. Everything it needs from the
//! outside world comes through the [`Fs`] trait, so the whole behaviour spec can be
//! tested headless against an in-memory filesystem (`nocrumbs-testfs`).
//!
//! **Status: M0 spike.** Only the pieces the spike needs exist so far: path/command
//! completion ([`analyze`]), path resolution ([`resolve`]) and glob matching
//! ([`Glob`]).

mod complete;
mod fs;
mod glob;
mod path;

pub use complete::{Analysis, BarContext, Completion, Mode, Resolved, analyze, display_path, resolve};
pub use fs::{Entry, Fs};
pub use glob::Glob;
pub use path::{fold_cmp, is_sep};
