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
    if !fold {
        return name.starts_with(prefix);
    }
    let mut n = name.chars().flat_map(char::to_lowercase);
    for p in prefix.chars().flat_map(char::to_lowercase) {
        if n.next() != Some(p) {
            return false;
        }
    }
    true
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
    fn prefix_folding() {
        assert!(starts_with("Documents", "doc", true));
        assert!(!starts_with("Documents", "doc", false));
        assert!(starts_with("Documents", "Doc", false));
        assert!(!starts_with("Do", "Doc", true));
    }
}
