// SPDX-License-Identifier: GPL-3.0-or-later
use std::cmp::Ordering;

/// Both separators are accepted on input, on every platform.
pub fn is_sep(c: char) -> bool {
    c == '/' || c == '\\'
}

/// Case-insensitive comparison without allocating.
pub fn fold_cmp(a: &str, b: &str) -> Ordering {
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

/// Does `name` start with `prefix`? Optionally ignoring case. Allocation-free.
pub(crate) fn starts_with(name: &str, prefix: &str, fold: bool) -> bool {
    prefix_chars(name, prefix, fold).is_some()
}

/// If `name` starts with `prefix`, how many **characters of `name`** the prefix covers.
///
/// That is not always `prefix.chars().count()`: lower-casing can change the number of
/// characters (`İ` lower-cases to two), so the ghost text must be cut from the name by this
/// count. A prefix that would end halfway through one of the name's characters does not match,
/// because there is no way to show the rest of that character as ghost text.
pub(crate) fn prefix_chars(name: &str, prefix: &str, fold: bool) -> Option<usize> {
    if !fold {
        return name.starts_with(prefix).then(|| prefix.chars().count());
    }
    let mut want = prefix.chars().flat_map(char::to_lowercase).peekable();
    if want.peek().is_none() {
        return Some(0);
    }
    for (i, c) in name.chars().enumerate() {
        for lower in c.to_lowercase() {
            // `None` here means the prefix ran out in the middle of this character.
            if want.next() != Some(lower) {
                return None;
            }
        }
        if want.peek().is_none() {
            return Some(i + 1);
        }
    }
    None
}

/// Join `segments` onto absolute `base` (`/a/b`), resolving `.` and `..`.
pub(crate) fn normalize<'a>(base: &str, segments: impl Iterator<Item = &'a str>) -> String {
    let mut parts: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for s in segments {
        match s {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    let mut out = String::new();
    for p in &parts {
        out.push('/');
        out.push_str(p);
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes() {
        assert_eq!(normalize("/a/b", ["..", "c"].into_iter()), "/a/c");
        assert_eq!(normalize("/", ["..", ".."].into_iter()), "/");
        assert_eq!(normalize("/a", ["", ".", "b"].into_iter()), "/a/b");
    }

    #[test]
    fn covered_characters_are_counted_in_the_name_not_the_typed_text() {
        assert_eq!(prefix_chars("Documents", "doc", true), Some(3));
        // `İ` lower-cases to two characters: typing both covers one character of the name.
        assert_eq!(prefix_chars("\u{130}a", "i\u{307}", true), Some(1));
        assert_eq!(prefix_chars("\u{130}a", "i\u{307}a", true), Some(2));
        // Half of that character is not a match: there is nothing to show for the other half.
        assert_eq!(prefix_chars("\u{130}a", "i", true), None);
        // And the other way round: two typed characters of the name, one of them expanding.
        assert_eq!(prefix_chars("i\u{307}x", "\u{130}", true), Some(2));
        assert_eq!(prefix_chars("abc", "", true), Some(0));
        assert_eq!(prefix_chars("ab", "abc", true), None);
    }

    #[test]
    fn prefix_folding() {
        assert!(starts_with("Documents", "doc", true));
        assert!(!starts_with("Documents", "doc", false));
        assert!(starts_with("Documents", "Doc", false));
        assert!(!starts_with("Do", "Doc", true));
    }
}
