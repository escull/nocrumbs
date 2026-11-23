// SPDX-License-Identifier: GPL-3.0-or-later
//! Finding the tests that exist, without running anything.
//!
//! Integration tests are named `file.rs::fn_name` (the file is unique across the workspace).
//! Unit tests are named `unit:module::tests::fn_name`, as the test harness prints them.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Default)]
pub struct TestIndex {
    tests: BTreeSet<String>,
}

impl TestIndex {
    pub fn contains(&self, name: &str) -> bool {
        self.tests.contains(name)
    }

    pub fn len(&self) -> usize {
        self.tests.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.tests.iter().map(String::as_str)
    }
}

pub fn scan(root: &Path) -> io::Result<TestIndex> {
    let mut index = TestIndex::default();
    for group in ["crates", "spikes"] {
        let Ok(entries) = fs::read_dir(root.join(group)) else { continue };
        for entry in entries {
            let pkg = entry?.path();
            if !pkg.is_dir() {
                continue;
            }
            for file in rust_files(&pkg.join("tests"))? {
                let name = file.file_name().expect("file").to_string_lossy().into_owned();
                for f in test_fns(&fs::read_to_string(&file)?) {
                    index.tests.insert(format!("{name}::{f}"));
                }
            }
            let src = pkg.join("src");
            for file in rust_files(&src)? {
                let module = module_path(&src, &file);
                for f in test_fns(&fs::read_to_string(&file)?) {
                    index.tests.insert(if module.is_empty() {
                        format!("unit:tests::{f}")
                    } else {
                        format!("unit:{module}::tests::{f}")
                    });
                }
            }
        }
    }
    Ok(index)
}

fn rust_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return Ok(out) };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            out.extend(rust_files(&path)?);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// `src/bar/ops.rs` -> `bar::ops`; `lib.rs`, `main.rs` and `mod.rs` add nothing.
fn module_path(src: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(src).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".rs").to_string();
    }
    if matches!(parts.last().map(String::as_str), Some("lib" | "main" | "mod")) {
        parts.pop();
    }
    parts.join("::")
}

/// Names of functions marked `#[test]` (the attribute may be followed by others, such as
/// `#[ignore]`, before the `fn`).
fn test_fns(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let Some(rest) = t.strip_prefix("fn ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        // Walk up through the attribute and comment lines directly above the `fn`; it is a test
        // only if `#[test]` is among them (a helper right after a test is not).
        let mut is_test = false;
        for l in lines[..i].iter().rev() {
            let l = l.trim();
            if l == "#[test]" {
                is_test = true;
                break;
            }
            if !(l.starts_with("#[") || l.starts_with("//")) {
                break;
            }
        }
        if is_test {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_marked_functions_only() {
        let src = "#[test]\nfn one() {}\nfn helper() {}\n#[test]\n#[ignore = \"slow\"]\nfn two() {}\n";
        assert_eq!(test_fns(src), ["one", "two"]);
    }

    #[test]
    fn module_paths() {
        let src = Path::new("/p/src");
        assert_eq!(module_path(src, Path::new("/p/src/command.rs")), "command");
        assert_eq!(module_path(src, Path::new("/p/src/bar/ops.rs")), "bar::ops");
        assert_eq!(module_path(src, Path::new("/p/src/lib.rs")), "");
        assert_eq!(module_path(src, Path::new("/p/src/bar/mod.rs")), "bar");
    }
}
