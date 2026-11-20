// SPDX-License-Identifier: GPL-3.0-or-later
use std::sync::Arc;

use crate::names::NameRules;

/// One directory entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    /// Hidden by the platform's rules (Windows: attribute; Unix: leading dot).
    pub hidden: bool,
    pub size: u64,
    /// Seconds since the Unix epoch.
    pub mtime: i64,
}

/// What the core needs from a filesystem.
///
/// Paths passed to and returned from this trait are absolute, `/`-separated and
/// normalised (no `.`/`..`). The platform layer is responsible for mapping real
/// paths (drive letters, UNC, ...) onto this shape; the core never assumes a
/// particular separator beyond that.
pub trait Fs: Send + Sync {
    /// The canonically-cased form of `dir` if it exists and is a directory.
    fn canonical_dir(&self, dir: &str) -> Option<String>;
    /// Entries of a directory given by its canonical path. Sorted folders-first, then by name.
    fn entries(&self, dir: &str) -> Option<Arc<[Entry]>>;
    /// Whether names compare case-insensitively (Windows, default macOS).
    fn case_insensitive(&self) -> bool;
    /// Which naming rules apply to files created here (brief §8.2). Defaults to Unix rules.
    fn name_rules(&self) -> NameRules {
        NameRules::Posix
    }
}
