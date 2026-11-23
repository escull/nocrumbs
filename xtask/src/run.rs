// SPDX-License-Identifier: GPL-3.0-or-later
//! Running the workspace's tests and reading what they report.

use std::collections::BTreeMap;
use std::fs::File;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Ignored,
}

/// Test results keyed like the spec refers to tests (`file.rs::name`, `unit:module::tests::name`).
#[derive(Debug, Default)]
pub struct Results(BTreeMap<String, Outcome>);

impl Results {
    pub fn get(&self, name: &str) -> Option<Outcome> {
        self.0.get(name).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

pub fn run_tests(root: &Path) -> Result<Results, String> {
    let log = std::env::temp_dir().join(format!("nocrumbs-xtask-{}.log", std::process::id()));
    let out = File::create(&log).map_err(|e| format!("creating {}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| format!("duplicating log handle: {e}"))?;
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    eprintln!("running cargo test --workspace (this takes a moment)...");
    // Both streams go to one file so the "Running ..." lines (stderr) stay in order with the
    // results (stdout).
    let status = Command::new(cargo)
        .args(["test", "--workspace", "--no-fail-fast"])
        .current_dir(root)
        .stdout(out)
        .stderr(err)
        .status()
        .map_err(|e| format!("running cargo test: {e}"))?;
    let text = std::fs::read_to_string(&log).map_err(|e| format!("reading test log: {e}"))?;
    let _ = std::fs::remove_file(&log);
    let results = parse(&text);
    if results.len() == 0 && !status.success() {
        return Err(format!("cargo test failed before running any test:\n{text}"));
    }
    Ok(results)
}

enum Context {
    Unit,
    Integration(String),
    Other,
}

pub fn parse(text: &str) -> Results {
    let mut results = BTreeMap::new();
    let mut ctx = Context::Other;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Running ") {
            let target = rest.split(" (").next().unwrap_or(rest);
            ctx = if target.starts_with("unittests") {
                Context::Unit
            } else {
                let file = target.rsplit(['\\', '/']).next().unwrap_or(target);
                Context::Integration(file.to_string())
            };
        } else if line.starts_with("Doc-tests") {
            ctx = Context::Other;
        } else if let Some(rest) = line.strip_prefix("test ")
            && let Some((name, outcome)) = rest.split_once(" ... ")
        {
            let outcome = if outcome.starts_with("ok") {
                Outcome::Passed
            } else if outcome.starts_with("FAILED") {
                Outcome::Failed
            } else if outcome.starts_with("ignored") {
                Outcome::Ignored
            } else {
                continue;
            };
            let key = match &ctx {
                Context::Unit => format!("unit:{name}"),
                Context::Integration(file) => format!("{file}::{name}"),
                Context::Other => continue,
            };
            results.insert(key, outcome);
        }
    }
    Results(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_unit_and_integration_results() {
        let log = "\
     Running unittests src\\lib.rs (target\\debug\\deps\\nocrumbs_core-1.exe)
test command::tests::sort ... ok
test glob::tests::detects_glob ... FAILED
     Running tests\\keystrokes.rs (target\\debug\\deps\\keystrokes-2.exe)
test ghost_text_is_appended_and_selected ... ok
test perf_100k_keystrokes ... ignored, timing probe
   Doc-tests nocrumbs_core
test src/lib.rs - (line 1) ... ok
";
        let r = parse(log);
        assert_eq!(r.get("unit:command::tests::sort"), Some(Outcome::Passed));
        assert_eq!(r.get("unit:glob::tests::detects_glob"), Some(Outcome::Failed));
        assert_eq!(r.get("keystrokes.rs::ghost_text_is_appended_and_selected"), Some(Outcome::Passed));
        assert_eq!(r.get("keystrokes.rs::perf_100k_keystrokes"), Some(Outcome::Ignored));
        assert_eq!(r.len(), 4, "doc-tests are not tracked");
    }
}
