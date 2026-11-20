// SPDX-License-Identifier: GPL-3.0-or-later
//! Validation of file and alias names (brief §8.2 and §6).

use std::fmt;

use crate::path::is_sep;

/// Which filesystem's naming rules apply. Chosen by the platform layer ([`crate::Fs::name_rules`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NameRules {
    /// Linux and (for now) macOS: anything but `/`, NUL, empty, `.` and `..`.
    #[default]
    Posix,
    /// Windows: additionally no `< > : " | ? *`, no control characters, no trailing space or
    /// dot, and no reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`9`, `LPT1`–`9`),
    /// with or without an extension.
    Windows,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    /// `.` or `..`
    DotName(String),
    /// Contains `/` or `\`.
    Separator,
    ReservedChar(char),
    ControlChar,
    TrailingSpaceOrDot,
    ReservedDeviceName(String),
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "A name cannot be empty"),
            Self::DotName(n) => write!(f, "\"{n}\" is not a valid name"),
            Self::Separator => write!(f, "A name cannot contain / or \\"),
            Self::ReservedChar(c) => write!(f, "A name cannot contain {c}"),
            Self::ControlChar => write!(f, "A name cannot contain control characters"),
            Self::TrailingSpaceOrDot => write!(f, "A name cannot end with a space or a dot"),
            Self::ReservedDeviceName(n) => write!(f, "\"{n}\" is a reserved device name on Windows"),
        }
    }
}

impl std::error::Error for NameError {}

const WINDOWS_RESERVED_CHARS: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];

/// Check a single file or folder name for creation, rename or paste.
pub fn validate_name(name: &str, rules: NameRules) -> Result<(), NameError> {
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::DotName(name.to_string()));
    }
    if name.contains(is_sep) {
        return Err(NameError::Separator);
    }
    if name.contains('\0') {
        return Err(NameError::ControlChar);
    }
    if rules == NameRules::Posix {
        return Ok(());
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::ControlChar);
    }
    if let Some(c) = name.chars().find(|c| WINDOWS_RESERVED_CHARS.contains(c)) {
        return Err(NameError::ReservedChar(c));
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::TrailingSpaceOrDot);
    }
    // `CON`, `con.txt` and `Con.tar.gz` are all reserved: the part before the first dot counts.
    let stem = name.split('.').next().unwrap_or(name);
    if is_reserved_device(stem) {
        return Err(NameError::ReservedDeviceName(stem.to_string()));
    }
    Ok(())
}

fn is_reserved_device(stem: &str) -> bool {
    let up = stem.trim_end().to_ascii_uppercase();
    matches!(up.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|p| {
            up.strip_prefix(p).is_some_and(|n| matches!(n, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasNameError {
    Empty,
    Separator,
    LeadingColonOrTilde,
    /// Contains `*`, `?` or `.`: those would collide with globs and with `.`/`..`.
    Reserved(char),
    Whitespace,
}

impl fmt::Display for AliasNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "An alias name cannot be empty"),
            Self::Separator => write!(f, "An alias name cannot contain / or \\"),
            Self::LeadingColonOrTilde => write!(f, "An alias name cannot start with : or ~"),
            Self::Reserved(c) => write!(f, "An alias name cannot contain {c}"),
            Self::Whitespace => write!(f, "An alias name cannot contain spaces"),
        }
    }
}

impl std::error::Error for AliasNameError {}

/// Alias names must keep the resolution order of brief §4 deterministic: they cannot look like
/// a command (`:`), the home folder (`~`), a glob (`*`, `?`) or `.`/`..`, and are one path segment.
pub fn validate_alias_name(name: &str) -> Result<(), AliasNameError> {
    if name.is_empty() {
        return Err(AliasNameError::Empty);
    }
    if name.contains(is_sep) {
        return Err(AliasNameError::Separator);
    }
    if name.starts_with([':', '~']) {
        return Err(AliasNameError::LeadingColonOrTilde);
    }
    if name.chars().any(char::is_whitespace) {
        return Err(AliasNameError::Whitespace);
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '*' | '?' | '.')) {
        return Err(AliasNameError::Reserved(c));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use NameRules::{Posix, Windows};

    #[test]
    fn ordinary_names_pass() {
        for n in ["notes.md", "holiday-2026 (1).jpg", ".zshrc", "日本語", "a b"] {
            assert_eq!(validate_name(n, Windows), Ok(()), "{n}");
            assert_eq!(validate_name(n, Posix), Ok(()), "{n}");
        }
    }

    #[test]
    fn universal_rejections() {
        for rules in [Posix, Windows] {
            assert_eq!(validate_name("", rules), Err(NameError::Empty));
            assert_eq!(validate_name(".", rules), Err(NameError::DotName(".".into())));
            assert_eq!(validate_name("..", rules), Err(NameError::DotName("..".into())));
            assert_eq!(validate_name("a/b", rules), Err(NameError::Separator));
            assert_eq!(validate_name("a\\b", rules), Err(NameError::Separator));
            assert_eq!(validate_name("a\0b", rules), Err(NameError::ControlChar));
        }
    }

    #[test]
    fn windows_only_rejections() {
        for c in ['<', '>', ':', '"', '|', '?', '*'] {
            let n = format!("a{c}b");
            assert_eq!(validate_name(&n, Windows), Err(NameError::ReservedChar(c)));
            assert_eq!(validate_name(&n, Posix), Ok(()), "legal on Unix: {n}");
        }
        assert_eq!(validate_name("a\tb", Windows), Err(NameError::ControlChar));
        assert_eq!(validate_name("name ", Windows), Err(NameError::TrailingSpaceOrDot));
        assert_eq!(validate_name("name.", Windows), Err(NameError::TrailingSpaceOrDot));
        assert_eq!(validate_name("name ", Posix), Ok(()));
    }

    #[test]
    fn reserved_device_names() {
        for n in ["CON", "con", "Prn", "AUX", "nul", "COM1", "com9", "LPT3", "con.txt", "Aux.tar.gz"] {
            assert!(matches!(validate_name(n, Windows), Err(NameError::ReservedDeviceName(_))), "{n}");
            assert_eq!(validate_name(n, Posix), Ok(()), "{n}");
        }
        for n in ["COM0", "COM10", "LPT", "console", "nulls", "com", "xcon"] {
            assert_eq!(validate_name(n, Windows), Ok(()), "{n}");
        }
    }

    #[test]
    fn alias_names() {
        assert_eq!(validate_alias_name("proj"), Ok(()));
        assert_eq!(validate_alias_name("dl2"), Ok(()));
        assert_eq!(validate_alias_name(""), Err(AliasNameError::Empty));
        assert_eq!(validate_alias_name("a/b"), Err(AliasNameError::Separator));
        assert_eq!(validate_alias_name(":x"), Err(AliasNameError::LeadingColonOrTilde));
        assert_eq!(validate_alias_name("~"), Err(AliasNameError::LeadingColonOrTilde));
        assert_eq!(validate_alias_name("a*"), Err(AliasNameError::Reserved('*')));
        assert_eq!(validate_alias_name("a?"), Err(AliasNameError::Reserved('?')));
        assert_eq!(validate_alias_name(".."), Err(AliasNameError::Reserved('.')));
        assert_eq!(validate_alias_name("a b"), Err(AliasNameError::Whitespace));
    }
}
