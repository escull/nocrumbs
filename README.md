# NoCrumbs

**A one-bar file manager. No breadcrumbs.** A native, single-binary file manager driven entirely by one command-line-style address bar.

NoCrumbs merges the speed of a terminal with the clarity of a detail-list file explorer. Paths, aliases, glob filters and `:commands` all go in one bar, with inline ghost-text completion; one list, one status line.

> **Status: pre-alpha, spec stage.** There is no code yet. The design is written; the first milestone is a UI spike. See [ROADMAP.md](ROADMAP.md) for where things stand.

## Plan

- **Rust**, **egui/eframe**, shipped as one `nocrumbs.exe` (no installer, no runtime, portable).
- **Windows first**, then Linux, then macOS.
- A UI-free core crate, tested with keystroke-script tests, behind thin platform and UI layers.

## Documents

| File | What it is |
|---|---|
| [DESIGN_BRIEF.md](DESIGN_BRIEF.md) | The specification: principles, address bar, completion, commands, keys, acceptance criteria |
| [ROADMAP.md](ROADMAP.md) | Stage, milestones, ranked upcoming work, risks, progress log |
| [archive/concept-sketch.html](archive/concept-sketch.html) | The original HTML concept sketch (from when the project was called OneBar). Not a behavioural reference; kept for history |
| `archive/` | Superseded versions of the brief and the concept sketch |

## Contributing

Issues and ideas are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first. The brief is binding on scope: new ideas that fall under its non-goals (§14) are recorded, not built.

## Licence

NoCrumbs is free software, licensed under the **GNU General Public License, version 3 or (at your option) any later version** (SPDX: `GPL-3.0-or-later`). See [LICENSE](LICENSE).

Copyright (C) 2026 NoCrumbs contributors.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for details.
