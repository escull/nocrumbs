// SPDX-License-Identifier: GPL-3.0-or-later
//! An in-memory [`StateStore`] for tests, with switches to simulate a damaged file, an
//! unreadable store and a disk that stops accepting writes.

use std::sync::Mutex;

use nocrumbs_core::{Loaded, PersistedState, StateStore, StoreError};

#[derive(Debug, Default)]
struct Inner {
    stored: Option<PersistedState>,
    notice: Option<String>,
    load_error: Option<String>,
    fail_saves: bool,
    saves: usize,
}

/// Remembers the last state saved to it.
#[derive(Debug, Default)]
pub struct MemStore {
    inner: Mutex<Inner>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start out holding `state`, as if saved by an earlier session.
    pub fn with_state(state: PersistedState) -> Self {
        let s = Self::new();
        s.inner.lock().expect("lock").stored = Some(state);
        s
    }

    /// The next `load` returns this notice (as when a damaged file was set aside).
    pub fn set_notice(&self, notice: &str) {
        self.inner.lock().expect("lock").notice = Some(notice.to_string());
    }

    /// `load` fails with this message (as for a file from a newer version).
    pub fn fail_load(&self, message: &str) {
        self.inner.lock().expect("lock").load_error = Some(message.to_string());
    }

    /// While on, every `save` fails (a read-only or full disk).
    pub fn fail_saves(&self, on: bool) {
        self.inner.lock().expect("lock").fail_saves = on;
    }

    /// The last state saved, if any.
    pub fn saved(&self) -> Option<PersistedState> {
        self.inner.lock().expect("lock").stored.clone()
    }

    /// How many saves succeeded.
    pub fn save_count(&self) -> usize {
        self.inner.lock().expect("lock").saves
    }
}

impl StateStore for MemStore {
    fn load(&self) -> Result<Loaded, StoreError> {
        let mut i = self.inner.lock().expect("lock");
        if let Some(e) = &i.load_error {
            return Err(StoreError(e.clone()));
        }
        Ok(Loaded { state: i.stored.clone().unwrap_or_default(), notice: i.notice.take() })
    }

    fn save(&self, state: &PersistedState) -> Result<(), StoreError> {
        let mut i = self.inner.lock().expect("lock");
        if i.fail_saves {
            return Err(StoreError("the disk is read-only".to_string()));
        }
        i.stored = Some(state.clone());
        i.saves += 1;
        Ok(())
    }
}
