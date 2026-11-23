// SPDX-License-Identifier: GPL-3.0-or-later
//! Project tasks: `cargo xtask <task>`.
//!
//! `cargo xtask spec` checks `spec/requirements.toml` (the requirements, each mapped to the tests
//! that prove it) against the tests that really exist, and reports how much of the brief is
//! implemented. See `spec.rs` for what is checked and `report.rs` for `spec/COVERAGE.md`.

mod report;
mod run;
mod scan;
mod spec;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "\
Usage: cargo xtask spec [--run] [--write] [--check-doc]

  (no flag)    validate spec/requirements.toml against the tests that exist, print a summary
  --run        also run `cargo test --workspace` and check that every referenced test passes
  --write      regenerate spec/COVERAGE.md
  --check-doc  fail if spec/COVERAGE.md is out of date (used by CI)
";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the workspace").to_path_buf()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("spec") => match spec_task(&args[1..]) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprint!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn spec_task(flags: &[String]) -> Result<ExitCode, String> {
    let (mut run_tests, mut write, mut check_doc) = (false, false, false);
    for f in flags {
        match f.as_str() {
            "--run" => run_tests = true,
            "--write" => write = true,
            "--check-doc" => check_doc = true,
            other => return Err(format!("unknown flag {other}\n{USAGE}")),
        }
    }
    let root = root();
    let spec = spec::load(&root.join("spec").join("requirements.toml"))?;
    let index = scan::scan(&root).map_err(|e| format!("scanning tests: {e}"))?;

    let mut problems = spec::validate(&spec, &index);

    let results = if run_tests { Some(run::run_tests(&root)?) } else { None };
    if let Some(r) = &results {
        problems.extend(spec::check_results(&spec, r));
    }

    let doc_path = root.join("spec").join("COVERAGE.md");
    let doc = report::coverage_markdown(&spec, &index);
    if write {
        std::fs::write(&doc_path, &doc).map_err(|e| format!("writing {}: {e}", doc_path.display()))?;
        println!("wrote {}", doc_path.display());
    }
    if check_doc {
        let on_disk = std::fs::read_to_string(&doc_path).unwrap_or_default();
        if on_disk.replace("\r\n", "\n") != doc {
            problems.push("spec/COVERAGE.md is out of date: run `cargo xtask spec --write`".to_string());
        }
    }

    print!("{}", report::console_summary(&spec, &index, results.as_ref()));
    if problems.is_empty() {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("\n{} problem(s):", problems.len());
        for p in &problems {
            eprintln!("  - {p}");
        }
        Ok(ExitCode::FAILURE)
    }
}
