// SPDX-License-Identifier: GPL-3.0-or-later
use std::cell::RefCell;

/// A compiled glob: `*` (any run of characters) and `?` (exactly one). Case-insensitive.
///
/// There is no `[..]`, `{..}` or `**` in v1 (brief §7).
#[derive(Debug, Clone)]
pub struct Glob {
    pattern: Vec<char>,
}

thread_local! {
    static BUF: RefCell<Vec<char>> = const { RefCell::new(Vec::new()) };
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

impl Glob {
    pub fn new(pattern: &str) -> Self {
        Self { pattern: pattern.chars().map(fold).collect() }
    }

    /// Does `s` contain glob metacharacters?
    pub fn is_glob(s: &str) -> bool {
        s.contains(['*', '?'])
    }

    pub fn is_match(&self, text: &str) -> bool {
        BUF.with(|b| {
            let mut buf = b.borrow_mut();
            buf.clear();
            buf.extend(text.chars().map(fold));
            self.match_chars(&buf)
        })
    }

    fn match_chars(&self, text: &[char]) -> bool {
        let pat = &self.pattern;
        let (mut p, mut t) = (0usize, 0usize);
        let (mut star, mut mark) = (None::<usize>, 0usize);
        while t < text.len() {
            if p < pat.len() && (pat[p] == '?' || pat[p] == text[t]) {
                p += 1;
                t += 1;
            } else if p < pat.len() && pat[p] == '*' {
                star = Some(p);
                mark = t;
                p += 1;
            } else if let Some(s) = star {
                p = s + 1;
                mark += 1;
                t = mark;
            } else {
                return false;
            }
        }
        while p < pat.len() && pat[p] == '*' {
            p += 1;
        }
        p == pat.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, t: &str) -> bool {
        Glob::new(p).is_match(t)
    }

    #[test]
    fn star_and_question() {
        assert!(m("*.pdf", "report.PDF"));
        assert!(!m("*.pdf", "report.pdf.bak"));
        assert!(m("report-*", "report-2026"));
        assert!(m("?2026?", "x2026y"));
        assert!(!m("?2026?", "2026y"));
        assert!(m("*", ""));
        assert!(m("a*b*c", "aXXbYYc"));
        assert!(!m("a*b*c", "aXXbYY"));
        assert!(m("*a*", "banana"));
    }

    #[test]
    fn unicode_folds() {
        assert!(m("ÄB*", "äbc"));
    }

    #[test]
    fn detects_glob() {
        assert!(Glob::is_glob("a*"));
        assert!(Glob::is_glob("a?"));
        assert!(!Glob::is_glob("abc"));
    }
}
