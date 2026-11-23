// SPDX-License-Identifier: GPL-3.0-or-later
//! What persists between sessions (brief §6, §5.1): aliases and frecency. The core only knows
//! the shape and the [`StateStore`] trait; where and how it is stored (a TOML file, portable
//! or in the user's config folder) belongs to `nocrumbs-config` and the platform layers.

use std::fmt;

use crate::alias::Aliases;
use crate::frecency::Frecency;

/// Everything worth keeping across sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PersistedState {
    pub aliases: Aliases,
    pub frecency: Frecency,
}

/// The result of loading: the state, plus a message when something went wrong and was recovered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Loaded {
    pub state: PersistedState,
    /// e.g. "The settings file was damaged. A copy was kept as config.toml.bad; starting fresh."
    pub notice: Option<String>,
}

/// Why a store could not load or save. The message is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

/// Where persisted state lives. A missing store is not an error: `load` returns the default
/// state. `save` must be safe against a crash half-way (write elsewhere, then replace).
pub trait StateStore: Send + Sync {
    fn load(&self) -> Result<Loaded, StoreError>;
    fn save(&self, state: &PersistedState) -> Result<(), StoreError>;
}
