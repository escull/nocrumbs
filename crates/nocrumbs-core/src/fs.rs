// SPDX-License-Identifier: GPL-3.0-or-later
use std::fmt;
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

/// Why a filesystem operation failed. The `String` is the path or name concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsError {
    NotFound(String),
    AlreadyExists(String),
    NotADirectory(String),
    PermissionDenied(String),
    /// A folder cannot be copied or moved into itself.
    IntoItself(String),
    /// This filesystem cannot do that (a read-only `Fs`, or no trash on this platform).
    Unsupported(&'static str),
    Other(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(p) => write!(f, "{p} no longer exists"),
            Self::AlreadyExists(p) => write!(f, "{p} already exists"),
            Self::NotADirectory(p) => write!(f, "{p} is not a folder"),
            Self::PermissionDenied(p) => write!(f, "Permission denied: {p}"),
            Self::IntoItself(p) => write!(f, "Cannot put {p} inside itself"),
            Self::Unsupported(what) => write!(f, "This filesystem cannot {what}"),
            Self::Other(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for FsError {}

/// A handle to something moved to the trash, for restoring it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrashToken(pub u64);

/// What the core needs from a filesystem.
///
/// Paths passed to and returned from this trait are absolute, `/`-separated and
/// normalised (no `.`/`..`). The platform layer is responsible for mapping real
/// paths (drive letters, UNC, ...) onto this shape; the core never assumes a
/// particular separator beyond that.
///
/// The write methods take `&self`: a real filesystem is shared state, and the in-memory one
/// uses interior mutability. They default to [`FsError::Unsupported`], so a read-only `Fs`
/// is still a valid `Fs`.
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

    /// Create an empty folder `name` in `parent`.
    fn create_dir(&self, _parent: &str, _name: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("create folders"))
    }
    /// Create an empty file `name` in `parent`.
    fn create_file(&self, _parent: &str, _name: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("create files"))
    }
    /// Rename `from` to `to` inside `dir`. A change of case only is allowed on case-insensitive
    /// filesystems.
    fn rename(&self, _dir: &str, _from: &str, _to: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("rename"))
    }
    /// Copy the file or folder at `from` (a full path) into `to_dir` as `name`. Folders are
    /// copied recursively. Fails if `name` exists there, or a folder would go inside itself.
    fn copy_entry(&self, _from: &str, _to_dir: &str, _name: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("copy"))
    }
    /// Move the file or folder at `from` into `to_dir` as `name`.
    fn move_entry(&self, _from: &str, _to_dir: &str, _name: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("move"))
    }
    /// Move to the Recycle Bin / trash, recoverably.
    fn trash(&self, _path: &str) -> Result<TrashToken, FsError> {
        Err(FsError::Unsupported("use the trash"))
    }
    /// Put a trashed item back where it was; returns its path.
    fn restore(&self, _token: TrashToken) -> Result<String, FsError> {
        Err(FsError::Unsupported("restore from the trash"))
    }
    /// Delete for good. Not undoable.
    fn delete_forever(&self, _path: &str) -> Result<(), FsError> {
        Err(FsError::Unsupported("delete permanently"))
    }
}
