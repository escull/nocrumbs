# NoCrumbs roadmap

Last reviewed: 2026-11-20 | Last updated: 2026-11-20 | Change: initial roadmap (spec only; no code yet; estimates are first guesses)

One-page summary of where NoCrumbs is, what comes next, and what could go wrong. NoCrumbs is a native Rust/egui single-exe file explorer driven by one address bar (Windows first, then Linux, then macOS). The *what* is in [DESIGN_BRIEF.md](DESIGN_BRIEF.md); this file is the *where we are*.

| Document | Holds |
|---|---|
| `DESIGN_BRIEF.md` | The spec: principles, address bar, completion, commands, keys, acceptance criteria (§15), tests (§16) |
| `ROADMAP.md` (this file) | Stage, milestones, ranked upcoming work, risks, progress log |
| `spec/requirements.toml` *(planned, M1)* | Every requirement with a stable ID, mapped to the keystroke-script tests that prove it |
| `docs/gaps.md` *(planned, M1)* | Every known shortfall against the brief, ranked: the work list |
| `archive/` | Superseded briefs and the original HTML concept sketch |

Keep it current: every working session, update the milestone row and add one line to the progress log; when a gap closes, remove it from `docs/gaps.md`; re-measure the headline numbers when a milestone lands and at least monthly. Keep this file to one page of status: detail belongs in the docs above, not here.

## Stage

**Stage: pre-alpha (spec stage)** · next: **spike passed**, then **alpha** (core passes its spec tests headless).

| Stage | Gate |
|---|---|
| Concept | Brief written and reviewed *(done)* |
| Spike | Milestone-0 questions answered: ghost-text bar, 100k-row list, IME, screen reader on egui |
| Alpha | Core logic passes all keystroke-script tests headless; Windows shell does real navigation, rename, delete-to-Recycle-Bin |
| Beta | All §15 acceptance criteria met on Windows; async I/O, undo, clipboard interop with Explorer, single signed exe within budgets |
| 1.0 | Beta + accessibility pass + performance budgets (§12) measured on real hardware |

## Headline numbers

| | Value | Kind |
|---|---|---|
| Acceptance criteria met (§15) | 0 of 12 | measured by hand until M1 |
| Requirements with a passing test | 0 of *n/a* (IDs assigned in M1) | measured by `cargo xtask spec` once it exists |
| Remaining to alpha | ≈ 50–95 focused hours | estimated, uncalibrated |
| Remaining to Windows 1.0 | ≈ 120–230 focused hours | estimated, uncalibrated |
| Binary size / cold start | not yet built (targets: ≤ 10 MB, ≤ 150 ms) | measured from M2 |

"Estimated" numbers are guesses until the first milestone provides a velocity to calibrate against. They will be revised, not defended.

## Milestones

| # | Milestone | Status | Estimate (h) | Exit criteria |
|---|---|---|---|---|
| M0 | **Spike**: egui bar with ghost text, 100k-row virtual list, IME, AccessKit pass | not started | 8–16 | Go/no-go note in `docs/spike.md`; the three risks in brief §3.2 each answered with evidence |
| M1 | **Core**: `nocrumbs-core` + `nocrumbs-testfs` + keystroke-test harness; `spec/requirements.toml` | not started | 40–80 | Brief §4–§8 pass headless; every bug from the HTML concept is a regression test (§16) |
| M2 | **Windows shell**: real `Fs`/`Platform`, async listing, Recycle Bin, clipboard interop, watcher, config/history | not started | 40–70 | §15 items 8–11 met on Windows; portable mode works |
| M3 | **UI polish, accessibility, signed release** | not started | 30–60 | All §15 met; accessibility checks in §13; **v1.0 (Windows)** |
| M4 | **Linux port** | not started | 30–60 | Core unchanged; platform crate only |
| M5 | **macOS port** | not started | 40–80 | Cmd-key map, Trash API, notarisation |

## Upcoming

Ranked; the next thing to do is first.

| Rank | Work | Estimate (h) |
|---|---|---|
| 0 | M0 spike, starting with ghost-text selection in the egui `TextEdit` (the riskiest piece) | 8–16 |
| 1 | Assign stable requirement IDs to brief §4–§15 and create `spec/requirements.toml` | 4–8 |
| 2 | Keystroke-script harness and the first 20 cases: completion, Backspace-on-ghost, command arguments, case, Tab descent | 16–30 |
| 3 | Bar parser: path / alias / glob / command resolution order, quoting, name validation | 16–30 |
| 4 | Selection model, multi-select, clipboard and undo stack in core | 20–40 |

## Risks

| Risk | Effect | Mitigation |
|---|---|---|
| egui `TextEdit` can't render selected ghost text cleanly, or IME conflicts with completion | The central feature (§5) is compromised | M0 spike first; fall back to a custom-painted bar, then to iced/slint (brief §3.2) |
| Windows path semantics (drive letters, UNC, case-insensitive, reserved names, long paths) leak into core | Linux/macOS ports become rewrites | Platform traits from day one; no `/` or `\` literals in core (brief §3.3) |
| Real-filesystem edge cases (locked files, network shares, huge folders) break the "zero latency" promise | Feels slow exactly where users notice | Async listing with cancellation and virtualised list in M2, budgets in brief §12 |
| Scope creep from the non-goals (tabs, preview pane, drag-and-drop) | v1 never ships | Brief §14 is binding; new ideas go to `docs/gaps.md`, not the milestone |
| Estimates are uncalibrated | Planning is fiction | Recalibrate after M0 and M1 from actual hours |

## Open decisions

Tracked in the brief, §18 (toolkit is decided: egui/eframe). Next ones to settle: Windows root view (list drives?), frecency store format, dotfile default, command-prefix configurability.

## Progress log

Newest first. One line per entry: date (milestone, what changed); estimates unchanged/changed.

2026-11-20 (project): Renamed from OneBar to NoCrumbs (OneBar is an existing company and product name, onebar.io; NoCrumbs had no clash found). Files renamed: DESIGN_BRIEF.md, archive/concept-sketch.html, archive/DESIGN_BRIEF_v1_web_concept.md. Crates will be nocrumbs-*. Estimates unchanged.

2026-11-20 (project): Git repository initialised on `main`; licensed GPL-3.0-or-later; added README, CONTRIBUTING (DCO sign-off), .gitignore, .gitattributes. Local only; not yet on GitHub. Estimates unchanged.

2026-11-20 (M0): Switched toolkit from "egui recommended" to egui/eframe decided; added §19 Project Management to the brief and created this roadmap. Estimates unchanged.

2026-11-20 (concept): Brief rewritten as v2 for a native Rust single-exe app (Windows → Linux → macOS); the v1 web-concept brief archived. The HTML file is now a concept sketch only. Behaviour bugs found in the sketch while reviewing are to become regression cases in M1 (placeholder overwrite, Backspace re-ghosting, case rewrite, Tab descent, folder-into-self, path hidden while filtering, filter with no keyboard exit). Estimates set; uncalibrated.
