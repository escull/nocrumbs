// SPDX-License-Identifier: GPL-3.0-or-later
//! Where NoCrumbs keeps what persists between sessions, and how (brief §3.4, §6, §5.1).
//!
//! * [`locate`] decides the file: **portable mode** if a `nocrumbs.toml` sits next to the exe
//!   (all state lives there), otherwise `config.toml` in the user's config folder.
//! * [`FileStore`] reads and writes it as TOML. Saves are atomic (write a temporary file, then
//!   replace), a damaged file is set aside as `*.bad` rather than overwritten, and a file from a
//!   newer version is refused and left alone.
//!
//! What is stored: aliases and frecency. Unknown keys are ignored on load and are therefore not
//! preserved when the file is rewritten. Two instances writing at once is last-writer-wins.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use nocrumbs_core::{Aliases, Frecency, Loaded, PersistedState, StateStore, StoreError, Visit};
use serde::{Deserialize, Serialize};

/// The file name that, next to the exe, switches on portable mode.
pub const PORTABLE_FILE: &str = "nocrumbs.toml";

/// The newest settings version this build understands (and writes).
pub const VERSION: i64 = 1;

/// The places the config could be, passed in so the rules are testable without touching the
/// real environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigEnv {
    /// The folder holding the running executable.
    pub exe_dir: Option<PathBuf>,
    /// The user's config folder: `%APPDATA%` on Windows, `$XDG_CONFIG_HOME` or `~/.config` on
    /// Linux, `~/Library/Application Support` on macOS.
    pub config_dir: Option<PathBuf>,
}

impl ConfigEnv {
    /// Read the real process environment.
    pub fn from_process() -> Self {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let config_dir = if cfg!(windows) {
            var("APPDATA")
        } else if cfg!(target_os = "macos") {
            var("HOME").map(|h| h.join("Library").join("Application Support"))
        } else {
            var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|h| h.join(".config")))
        };
        Self { exe_dir, config_dir }
    }
}

/// Where the settings file is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    /// Portable mode: the file sits next to the exe and *all* state is stored there.
    pub portable: bool,
}

/// Portable mode wins if `nocrumbs.toml` exists next to the exe; otherwise the user's config
/// folder (`NoCrumbs\config.toml` on Windows, `nocrumbs/config.toml` elsewhere). `None` if the
/// environment offers neither.
pub fn locate(env: &ConfigEnv) -> Option<Location> {
    if let Some(dir) = &env.exe_dir {
        let portable = dir.join(PORTABLE_FILE);
        if portable.is_file() {
            return Some(Location { path: portable, portable: true });
        }
    }
    let app = if cfg!(windows) { "NoCrumbs" } else { "nocrumbs" };
    env.config_dir.as_ref().map(|dir| Location { path: dir.join(app).join("config.toml"), portable: false })
}

#[derive(Serialize, Deserialize)]
struct Model {
    #[serde(default = "one")]
    version: i64,
    #[serde(default)]
    aliases: BTreeMap<String, String>,
    #[serde(default)]
    frecency: Vec<Row>,
}

#[derive(Serialize, Deserialize)]
struct Row {
    path: String,
    count: u32,
    last: i64,
}

fn one() -> i64 {
    1
}

impl Model {
    fn from_state(state: &PersistedState) -> Self {
        Self {
            version: VERSION,
            aliases: state.aliases.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect(),
            frecency: state
                .frecency
                .iter()
                .map(|(p, v)| Row { path: p.to_string(), count: v.count, last: v.last })
                .collect(),
        }
    }

    fn into_state(self) -> PersistedState {
        PersistedState {
            // Entries with an invalid alias name are skipped rather than failing the whole load.
            aliases: Aliases::from_pairs(self.aliases.iter().map(|(n, t)| (n.as_str(), t.as_str()))),
            frecency: Frecency::from_rows(
                self.frecency.into_iter().map(|r| (r.path, Visit { count: r.count, last: r.last })),
            ),
        }
    }
}

/// A settings file on disk.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn sidecar(&self, suffix: &str) -> PathBuf {
        let mut name = self.path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        name.push(suffix);
        self.path.with_file_name(name)
    }
}

impl StateStore for FileStore {
    fn load(&self) -> Result<Loaded, StoreError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Loaded::default()),
            Err(e) => return Err(StoreError(format!("Cannot read {}: {e}", self.path.display()))),
        };
        let damaged = |why: String| -> Result<Loaded, StoreError> {
            let bad = self.sidecar(".bad");
            fs::rename(&self.path, &bad).map_err(|e| {
                StoreError(format!("{} is damaged ({why}) and could not be set aside: {e}", self.path.display()))
            })?;
            Ok(Loaded {
                state: PersistedState::default(),
                notice: Some(format!(
                    "The settings file was damaged ({why}). A copy was kept as {}; starting fresh.",
                    bad.display()
                )),
            })
        };
        let table: toml::Table = match toml::from_str(&text) {
            Ok(t) => t,
            Err(e) => return damaged(e.message().to_string()),
        };
        // A file from a newer NoCrumbs may have a shape we cannot read. Refuse it and leave it
        // alone: it is not damaged, and overwriting it would lose the newer version's data.
        if let Some(v) = table.get("version").and_then(toml::Value::as_integer)
            && v > VERSION
        {
            return Err(StoreError(format!(
                "{} was written by a newer NoCrumbs (settings version {v}); leaving it alone",
                self.path.display()
            )));
        }
        match table.try_into::<Model>() {
            Ok(model) => Ok(Loaded { state: model.into_state(), notice: None }),
            Err(e) => damaged(e.message().to_string()),
        }
    }

    fn save(&self, state: &PersistedState) -> Result<(), StoreError> {
        let text = toml::to_string_pretty(&Model::from_state(state))
            .map_err(|e| StoreError(format!("Cannot encode settings: {e}")))?;
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(|e| StoreError(format!("Cannot create {}: {e}", dir.display())))?;
        }
        // Write beside the file and replace it, so a crash never leaves half a file.
        let tmp = self.sidecar(".tmp");
        let result = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, &self.path));
        if let Err(e) = result {
            let _ = fs::remove_file(&tmp);
            return Err(StoreError(format!("Cannot write {}: {e}", self.path.display())));
        }
        Ok(())
    }
}
