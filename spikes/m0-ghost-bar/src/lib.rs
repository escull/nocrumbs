// SPDX-License-Identifier: GPL-3.0-or-later
//! Milestone-0 spike: can egui do the NoCrumbs address bar?
//!
//! Three questions (see ROADMAP.md and brief §3.2):
//!
//! 1. **Ghost text** — appended, pre-selected completion in a `TextEdit` that never
//!    alters what the user typed, that Backspace removes without re-offering, and
//!    that Tab accepts (with canonical casing) and then descends into.
//! 2. **100k-row list** — a virtualised detail list with live glob filtering and a
//!    completion-highlight state that is distinct from selection.
//! 3. **IME / screen reader** — composition suppresses completion; AccessKit comes
//!    from egui. These two need a human: see `docs/spike.md`.

mod app;
mod format;

pub use app::{App, other_match_tint, setup};
