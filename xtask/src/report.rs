// SPDX-License-Identifier: GPL-3.0-or-later
//! What the spec says about the project: `spec/COVERAGE.md` and the console summary.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::run::{Outcome, Results};
use crate::scan::TestIndex;
use crate::spec::{Spec, Status, counts, outstanding};

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn referenced(spec: &Spec) -> BTreeSet<&str> {
    spec.requirement.iter().flat_map(|r| r.tests.iter().map(String::as_str)).collect()
}

fn acceptance_met(spec: &Spec) -> usize {
    spec.acceptance.iter().filter(|a| outstanding(spec, a).is_empty()).count()
}

/// The generated report. Deterministic (no dates), so CI can check it is current.
pub fn coverage_markdown(spec: &Spec, index: &TestIndex) -> String {
    let c = counts(spec);
    let refs = referenced(spec);
    let mut out = String::new();
    let _ = writeln!(out, "# Requirements coverage\n");
    let _ = writeln!(
        out,
        "Generated from [`requirements.toml`](requirements.toml) by `cargo xtask spec --write`. Do not edit by hand.\n\
         Each requirement maps to the tests that prove it. `cargo xtask spec` checks that those tests exist;\n\
         `cargo xtask spec --run` checks that they pass.\n"
    );
    let _ = writeln!(out, "## Summary\n");
    let _ = writeln!(out, "| | |\n|---|---|");
    let _ = writeln!(out, "| Requirements | {} |", c.total);
    let _ = writeln!(out, "| Implemented | {} |", c.by_status["implemented"]);
    let _ = writeln!(out, "| Partial | {} |", c.by_status["partial"]);
    let _ = writeln!(out, "| Manual (needs a human check) | {} |", c.by_status["manual"]);
    let _ = writeln!(out, "| Planned | {} |", c.by_status["planned"]);
    let _ = writeln!(
        out,
        "| Acceptance criteria met (brief §15) | {} of {} |",
        acceptance_met(spec),
        spec.acceptance.len()
    );
    let _ = writeln!(out, "| Tests in the workspace | {} ({} tied to a requirement) |\n", index.len(), refs.len());

    let _ = writeln!(out, "## Acceptance criteria (brief §15)\n");
    let _ = writeln!(out, "| # | Criterion | Met | Still needs |\n|---|---|---|---|");
    for a in &spec.acceptance {
        let todo = outstanding(spec, a);
        let met = if todo.is_empty() { "yes" } else { "no" };
        let _ = writeln!(out, "| {} | {} | {met} | {} |", a.n, cell(&a.text), todo.join(", "));
    }

    let _ = writeln!(out, "\n## Requirements");
    let mut sections: Vec<&str> = Vec::new();
    for r in &spec.requirement {
        if !sections.contains(&r.section.as_str()) {
            sections.push(&r.section);
        }
    }
    for section in sections {
        let _ = writeln!(out, "\n### {section}\n");
        let _ = writeln!(out, "| ID | Requirement | Status | Tests | Notes |\n|---|---|---|---|---|");
        for r in spec.requirement.iter().filter(|r| r.section == section) {
            let mut notes = Vec::new();
            if let Some(m) = &r.milestone {
                notes.push(format!("lands in {m}"));
            }
            if let Some(n) = &r.note {
                notes.push(n.clone());
            }
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} | {} |",
                r.id,
                cell(&r.title),
                r.status.word(),
                r.tests.len(),
                cell(&notes.join("; "))
            );
        }
    }
    out
}

pub fn console_summary(spec: &Spec, index: &TestIndex, results: Option<&Results>) -> String {
    let c = counts(spec);
    let refs = referenced(spec);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} requirements: {} implemented, {} partial, {} manual, {} planned",
        c.total, c.by_status["implemented"], c.by_status["partial"], c.by_status["manual"], c.by_status["planned"]
    );
    let _ = writeln!(
        out,
        "acceptance criteria (brief section 15) met: {} of {}",
        acceptance_met(spec),
        spec.acceptance.len()
    );
    let _ = writeln!(out, "tests: {} in the workspace, {} tied to a requirement", index.len(), refs.len());
    if let Some(r) = results {
        let passed = refs.iter().filter(|t| r.get(t) == Some(Outcome::Passed)).count();
        let _ = writeln!(out, "run: {passed} of {} referenced tests passed", refs.len());
    }
    let unlinked: Vec<&str> = index.iter().filter(|t| !refs.contains(t)).collect();
    if !unlinked.is_empty() {
        let _ = writeln!(
            out,
            "{} test(s) are not tied to any requirement (fine, but they prove nothing about the brief)",
            unlinked.len()
        );
    }
    let planned = spec.requirement.iter().filter(|r| r.status == Status::Planned).count();
    let _ = writeln!(out, "{planned} requirement(s) are planned and have no tests yet");
    out
}
