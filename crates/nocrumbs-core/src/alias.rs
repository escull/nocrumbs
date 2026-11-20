// SPDX-License-Identifier: GPL-3.0-or-later
//! Saved named shortcuts (brief §6): `:alias proj ~/projects`, then `proj/nocrumbs`.

use std::collections::BTreeMap;

use crate::names::{AliasNameError, validate_alias_name};
use crate::path::fold_cmp;

/// The user's aliases. Names are matched case-insensitively when the filesystem is
/// (the caller passes that in), and targets are absolute canonical folder paths.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aliases {
    map: BTreeMap<String, String>,
}

impl Aliases {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from `(name, target)` pairs, skipping any with an invalid name.
    pub fn from_pairs<'a>(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut a = Self::new();
        for (n, t) in pairs {
            let _ = a.set(n, t);
        }
        a
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// `(name, target)` in name order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Look an alias up. `fold` ignores case (case-insensitive filesystems).
    pub fn get(&self, name: &str, fold: bool) -> Option<(&str, &str)> {
        if fold {
            self.map.iter().find(|(k, _)| fold_cmp(k, name).is_eq()).map(|(k, v)| (k.as_str(), v.as_str()))
        } else {
            self.map.get_key_value(name).map(|(k, v)| (k.as_str(), v.as_str()))
        }
    }

    /// Add or replace an alias. Returns `true` if one with that name already existed.
    pub fn set(&mut self, name: &str, target: &str) -> Result<bool, AliasNameError> {
        validate_alias_name(name)?;
        Ok(self.map.insert(name.to_string(), target.to_string()).is_some())
    }

    /// Remove an alias; `true` if it existed.
    pub fn remove(&mut self, name: &str, fold: bool) -> bool {
        let key = self.get(name, fold).map(|(k, _)| k.to_string());
        key.is_some_and(|k| self.map.remove(&k).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove() {
        let mut a = Aliases::new();
        assert_eq!(a.set("proj", "/home/user/projects"), Ok(false));
        assert_eq!(a.set("proj", "/home/user/other"), Ok(true));
        assert_eq!(a.get("proj", false), Some(("proj", "/home/user/other")));
        assert_eq!(a.get("PROJ", false), None);
        assert_eq!(a.get("PROJ", true), Some(("proj", "/home/user/other")));
        assert!(a.remove("PROJ", true));
        assert!(a.is_empty());
        assert!(!a.remove("proj", false));
    }

    #[test]
    fn invalid_names_are_rejected() {
        let mut a = Aliases::new();
        assert!(a.set("a/b", "/x").is_err());
        assert!(a.set("", "/x").is_err());
        assert!(a.is_empty());
        let b = Aliases::from_pairs([("ok", "/a"), ("no way", "/b"), ("fine", "/c")]);
        assert_eq!(b.len(), 2);
    }
}
