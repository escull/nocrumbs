# NoCrumbs roadmap

Last reviewed: 2026-11-20 | Last updated: 2026-11-20 | Change: workspace and M0 spike built; automated checks pass; IME and screen-reader checks still open; estimates unchanged

One-page summary of where NoCrumbs is, what comes next, and what could go wrong. NoCrumbs is a native Rust/egui single-exe file explorer driven by one address bar (Windows first, then Linux, then macOS). The *what* is in [DESIGN_BRIEF.md](DESIGN_BRIEF.md); this file is the *where we are*.

| Document | Holds |
|---|---|
| `DESIGN_BRIEF.md` | The spec: principles, address bar, completion, commands, keys, acceptance criteria (§15), tests (§16) |
| `ROADMAP.md` (this file) | Stage, milestones, ranked upcoming work, risks, progress log |
| `docs/spike.md` | M0 findings, how to run the spike, the manual checks still to do, the verdict |
| `crates/`, `spikes/` | `nocrumbs-core` (UI-free), `nocrumbs-testfs` (in-memory FS), `spikes/m0-ghost-bar` (egui spike) |
| `spec/requirements.toml` *(planned, M1)* | Every requirement with a stable ID, mapped to the keystroke-script tests that prove it |
| `docs/gaps.md` *(planned, M1)* | Every known shortfall against the brief, ranked: the work list |
| `archive/` | Superseded briefs and the original HTML concept sketch |

Keep it current: every working session, update the milestone row and add one line to the progress log; when a gap closes, remove it from `docs/gaps.md`; re-measure the headline numbers when a milestone lands and at least monthly. Keep this file to one page of status: detail belongs in the docs above, not here.

## Stage

**Stage: pre-alpha (spike built, provisionally go)** · next: **spike passed** (two human checks), then **alpha** (core passes its spec tests headless).

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
| Automated tests passing | 176: 25 core units, 14 completion cases, 34 + 34 + 38 keystroke scripts (headless core), 12 in-memory filesystem, 18 egui UI, 1 format | measured, `cargo test --workspace` |
| 100k-row folder, keystroke cost (optimised, headless) | 6–9 ms typing, ≈ 13 ms per glob-filter keystroke; ≈ 1 ms/frame idle | measured, see `docs/spike.md` |
| Remaining to alpha | ≈ 50–95 focused hours | estimated, uncalibrated |
| Remaining to Windows 1.0 | ≈ 120–230 focused hours | estimated, uncalibrated |
| Binary size / cold start | not yet built (targets: ≤ 10 MB, ≤ 150 ms) | measured from M2 |

"Estimated" numbers are guesses until the first milestone provides a velocity to calibrate against. They will be revised, not defended.

## Milestones

| # | Milestone | Status | Estimate (h) | Exit criteria |
|---|---|---|---|---|
| M0 | **Spike**: egui bar with ghost text, 100k-row virtual list, IME, AccessKit pass | **in progress**: ghost text and 100k list proven by tests; IME and screen reader need a human | 8–16 | Go/no-go note in `docs/spike.md`; the three risks in brief §3.2 each answered with evidence |
| M1 | **Core**: `nocrumbs-core` + `nocrumbs-testfs` + keystroke-test harness; `spec/requirements.toml` | **in progress**: `Bar`, `sim::Session`, parser, aliases, names, sorting, history, file operations, multi-select and undo are in; `requirements.toml` and persistence are not | 40–80 | Brief §4–§8 pass headless; every bug from the HTML concept is a regression test (§16) |
| M2 | **Windows shell**: real `Fs`/`Platform`, async listing, Recycle Bin, clipboard interop, watcher, config/history | not started | 40–70 | §15 items 8–11 met on Windows; portable mode works |
| M3 | **UI polish, accessibility, signed release** | not started | 30–60 | All §15 met; accessibility checks in §13; **v1.0 (Windows)** |
| M4 | **Linux port** | not started | 30–60 | Core unchanged; platform crate only |
| M5 | **macOS port** | not started | 40–80 | Cmd-key map, Trash API, notarisation |

## Upcoming

Ranked; the next thing to do is first.

| Rank | Work | Estimate (h) |
|---|---|---|
| 0 | Finish M0: the human checklist in `docs/spike.md` (ghost on screen, IME, Narrator/NVDA, mouse, high DPI) | 1–3 |
| 1 | Assign stable requirement IDs to brief §4–§15 and create `spec/requirements.toml` | 4–8 |
| 2 | Persist aliases and history (config file, portable mode), frecency ranking for completion | 8–16 |
| 3 | Assign stable requirement IDs and create `spec/requirements.toml`; map the 176 tests to them | 4–8 |
| 4 | Windows shell (M2): real `Fs` with Recycle Bin, `CF_HDROP` clipboard interop, async operations with progress and cancel, file watcher, `:open` | 40–70 |
| 5 | Font fallback stack for arbitrary file names (CJK, Arabic, Thai, emoji): default egui fonts draw tofu | 6–12 |

## Risks

| Risk | Effect | Mitigation |
|---|---|---|
| egui `TextEdit` can't render selected ghost text cleanly, or IME conflicts with completion | The central feature (§5) is compromised | Ghost text proven headless (M0); IME and screen reader still open. Fall back to a custom-painted bar, then to iced/slint (brief §3.2) |
| egui draws tofu for characters its default fonts lack, and real file names contain anything | Unreadable names in the list | Font fallback stack in M2; the headless harness panics on missing glyphs so our own strings stay clean |
| Windows path semantics (drive letters, UNC, case-insensitive, reserved names, long paths) leak into core | Linux/macOS ports become rewrites | Platform traits from day one; no `/` or `\` literals in core (brief §3.3) |
| Real-filesystem edge cases (locked files, network shares, huge folders) break the "zero latency" promise | Feels slow exactly where users notice | Async listing with cancellation and virtualised list in M2, budgets in brief §12 |
| Scope creep from the non-goals (tabs, preview pane, drag-and-drop) | v1 never ships | Brief §14 is binding; new ideas go to `docs/gaps.md`, not the milestone |
| Estimates are uncalibrated | Planning is fiction | Recalibrate after M0 and M1 from actual hours |

## Open decisions

Tracked in the brief, §18 (toolkit is decided: egui/eframe). Next ones to settle: Windows root view (list drives?), frecency store format, dotfile default, command-prefix configurability.

## Progress log

Newest first. One line per entry: date (milestone, what changed); estimates unchanged/changed.

2026-11-20 (M1): File operations in the core. `Fs` gained a write side (create, rename, copy, move, trash, restore, delete) with typed errors; `MemFs` implements all of it including folder subtrees and the trash. `Bar` now runs `:mkdir`, `:touch`, `:rename`, `:rm`, `:rm!` (asks first), `:copy`, `:cut`, `:paste`, `:undo`, `:redo`, `:log`, with multi-select (Ctrl/Shift click, Shift+arrows, Ctrl+A), `name (2)` collision renaming, the folder-into-itself guard (the sketch's data-loss bug is a regression test), a clipboard that follows renames, and a reversible undo stack that never deletes for good. The egui app maps Del/Ctrl+C/X/V/Z/A/Backspace to the list once Esc has blurred the bar. 176 tests; 100k timings unchanged. Estimates unchanged.

2026-11-20 (M1): Bar parser work. `:commands` parse with the brief's quoting rules (`command.rs`); file-name validation for Windows and Unix rules incl. reserved device names (`names.rs`); aliases with validated names, first-segment completion, `proj/sub` and `dl/*.pdf` composition, shadowing and dangling-target messages (`alias.rs`, `complete.rs`); `:cd` and `:alias` path-argument completion; resolution order command > glob > alias > path; `:sort`/`:asc`/`:desc`/`:hidden`/`:alias`/`:unalias`/`:cd`/`:back`/`:fwd`/`:help` execute in the core, with history (Alt+Left/Right) and column-header sorting in the egui app; file operations parse and validate names but do not run yet. 120 tests; 100k timings unchanged. Estimates unchanged.

2026-11-20 (M1): Bar logic moved out of the egui app into `nocrumbs-core`: a headless `Bar` state machine (ghost text, staged Esc, Tab descent, Enter, list preview/highlight/selection, scroll and blur requests) plus `sim::Session`, a simulated text field. 34 keystroke scripts now run in the core with no UI; the 14 egui tests pass unchanged and 100k timings are unchanged. The egui app shrank to a thin renderer (~390 lines). `Fs` now requires `Send + Sync`. Estimates unchanged.

2026-11-20 (M0): Cargo workspace (`nocrumbs-core`, `nocrumbs-testfs`, `spikes/m0-ghost-bar`), CI workflow, and the egui spike. Ghost text, staged Esc, Tab descent with canonical casing, Backspace-without-reoffer, command arguments, glob-with-path, and a 100k-row virtual list are all covered by 14 headless UI tests and 14 completion cases; clippy clean. Found: egui's `TextEdit` overwrites the focus-lock filter every frame (set ours after `show`), and default fonts lack `⇥ ↵ →`. IME and screen-reader checks remain for a human. Estimates unchanged.

2026-11-20 (project): Published to github.com/escull/nocrumbs (public, GPL-3.0). Estimates unchanged.

2026-11-20 (project): Renamed from OneBar to NoCrumbs (OneBar is an existing company and product name, onebar.io; NoCrumbs had no clash found). Files renamed: DESIGN_BRIEF.md, archive/concept-sketch.html, archive/DESIGN_BRIEF_v1_web_concept.md. Crates will be nocrumbs-*. Estimates unchanged.

2026-11-20 (project): Git repository initialised on `main`; licensed GPL-3.0-or-later; added README, CONTRIBUTING (DCO sign-off), .gitignore, .gitattributes. Local only; not yet on GitHub. Estimates unchanged.

2026-11-20 (M0): Switched toolkit from "egui recommended" to egui/eframe decided; added §19 Project Management to the brief and created this roadmap. Estimates unchanged.

2026-11-20 (concept): Brief rewritten as v2 for a native Rust single-exe app (Windows → Linux → macOS); the v1 web-concept brief archived. The HTML file is now a concept sketch only. Behaviour bugs found in the sketch while reviewing are to become regression cases in M1 (placeholder overwrite, Backspace re-ghosting, case rewrite, Tab descent, folder-into-self, path hidden while filtering, filter with no keyboard exit). Estimates set; uncalibrated.
