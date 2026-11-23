// SPDX-License-Identifier: GPL-3.0-or-later
//! The requirements file: its shape, and the rules it must obey.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;

use crate::run::{Outcome, Results};
use crate::scan::TestIndex;

#[derive(Debug, Deserialize)]
pub struct Spec {
    pub version: u32,
    #[serde(default)]
    pub requirement: Vec<Requirement>,
    #[serde(default)]
    pub acceptance: Vec<Acceptance>,
}

#[derive(Debug, Deserialize)]
pub struct Requirement {
    /// `AREA-NN`, stable forever: never renumber, retire instead.
    pub id: String,
    /// Where it comes from, e.g. "5 Inline completion". Requirements are grouped by this text.
    pub section: String,
    pub title: String,
    pub status: Status,
    /// Tests that prove it: `file.rs::test_name` (integration) or `unit:module::tests::name`.
    #[serde(default)]
    pub tests: Vec<String>,
    /// Required for `partial` and `manual`: what is missing, or what a human must check.
    #[serde(default)]
    pub note: Option<String>,
    /// Required for `planned`: the milestone it lands in.
    #[serde(default)]
    pub milestone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// Built, and every test listed passes.
    Implemented,
    /// Built in part: tests exist for what is built, the note says what is not.
    Partial,
    /// Not built yet.
    Planned,
    /// Cannot be proven by an automated test (hardware, a screen reader, a measurement): a
    /// human checks it, and the note says how.
    Manual,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Self::Implemented => "implemented",
            Self::Partial => "partial",
            Self::Planned => "planned",
            Self::Manual => "manual",
        }
    }
}

/// One of the acceptance criteria of brief section 15, met when all the requirements it needs are.
#[derive(Debug, Deserialize)]
pub struct Acceptance {
    pub n: u32,
    pub text: String,
    #[serde(default)]
    pub requires: Vec<String>,
}

pub fn load(path: &Path) -> Result<Spec, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}

/// Everything wrong with the file, as sentences. Empty means valid.
pub fn validate(spec: &Spec, index: &TestIndex) -> Vec<String> {
    let mut problems = Vec::new();
    if spec.version != 1 {
        problems.push(format!("requirements.toml has version {}, expected 1", spec.version));
    }

    let mut seen = BTreeSet::new();
    for r in &spec.requirement {
        if !valid_id(&r.id) {
            problems.push(format!("{}: ids look like AREA-NN (2-4 capital letters, a dash, two digits)", r.id));
        }
        if !seen.insert(r.id.as_str()) {
            problems.push(format!("{}: duplicate id", r.id));
        }
        if r.title.trim().is_empty() {
            problems.push(format!("{}: empty title", r.id));
        }
        match r.status {
            Status::Implemented if r.tests.is_empty() => {
                problems.push(format!("{}: implemented, but no test proves it", r.id));
            }
            Status::Partial if r.tests.is_empty() || r.note.is_none() => {
                problems.push(format!("{}: partial needs at least one test and a note saying what is missing", r.id));
            }
            Status::Manual if r.note.is_none() => {
                problems.push(format!("{}: manual needs a note saying what a human must check", r.id));
            }
            Status::Planned if r.milestone.is_none() => {
                problems.push(format!("{}: planned needs a milestone", r.id));
            }
            _ => {}
        }
        if r.status == Status::Planned && !r.tests.is_empty() {
            problems.push(format!("{}: planned, but it already lists tests: is it partial?", r.id));
        }
        let mut refs = BTreeSet::new();
        for t in &r.tests {
            if !refs.insert(t.as_str()) {
                problems.push(format!("{}: lists {t} twice", r.id));
            }
            if !index.contains(t) {
                problems.push(format!("{}: test {t} does not exist", r.id));
            }
        }
    }

    let ids: BTreeSet<&str> = spec.requirement.iter().map(|r| r.id.as_str()).collect();
    for (i, a) in spec.acceptance.iter().enumerate() {
        if a.n as usize != i + 1 {
            problems.push(format!("acceptance criterion {} is out of order (expected {})", a.n, i + 1));
        }
        if a.requires.is_empty() {
            problems.push(format!("acceptance criterion {} requires nothing", a.n));
        }
        for id in &a.requires {
            if !ids.contains(id.as_str()) {
                problems.push(format!("acceptance criterion {} requires unknown {id}", a.n));
            }
        }
    }
    problems
}

fn valid_id(id: &str) -> bool {
    let Some((area, num)) = id.split_once('-') else { return false };
    (2..=4).contains(&area.len())
        && area.chars().all(|c| c.is_ascii_uppercase())
        && num.len() == 2
        && num.chars().all(|c| c.is_ascii_digit())
}

/// With real test results: every referenced test must have run and passed.
pub fn check_results(spec: &Spec, results: &Results) -> Vec<String> {
    let mut problems = Vec::new();
    for r in &spec.requirement {
        for t in &r.tests {
            match results.get(t) {
                Some(Outcome::Passed) => {}
                Some(Outcome::Failed) => problems.push(format!("{}: test {t} FAILED", r.id)),
                Some(Outcome::Ignored) => problems.push(format!("{}: test {t} is ignored, so it proves nothing", r.id)),
                None => problems.push(format!("{}: test {t} did not run", r.id)),
            }
        }
    }
    problems
}

pub struct Counts {
    pub by_status: BTreeMap<&'static str, usize>,
    pub total: usize,
}

pub fn counts(spec: &Spec) -> Counts {
    let mut by_status = BTreeMap::new();
    for s in [Status::Implemented, Status::Partial, Status::Manual, Status::Planned] {
        by_status.insert(s.word(), 0);
    }
    for r in &spec.requirement {
        *by_status.entry(r.status.word()).or_insert(0) += 1;
    }
    Counts { by_status, total: spec.requirement.len() }
}

/// Is this acceptance criterion met? Only if every requirement it needs is implemented.
/// Returns the ids still outstanding.
pub fn outstanding<'a>(spec: &'a Spec, a: &'a Acceptance) -> Vec<&'a str> {
    a.requires
        .iter()
        .filter(|id| spec.requirement.iter().find(|r| &r.id == *id).is_none_or(|r| r.status != Status::Implemented))
        .map(String::as_str)
        .collect()
}
