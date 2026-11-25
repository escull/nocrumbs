# Requirements coverage

Generated from [`requirements.toml`](requirements.toml) by `cargo xtask spec --write`. Do not edit by hand.
Each requirement maps to the tests that prove it. `cargo xtask spec` checks that those tests exist;
`cargo xtask spec --run` checks that they pass.

## Summary

| | |
|---|---|
| Requirements | 137 |
| Implemented | 105 |
| Partial | 10 |
| Manual (needs a human check) | 3 |
| Planned | 19 |
| Acceptance criteria met (brief §15) | 5 of 12 |
| Tests in the workspace | 262 (223 tied to a requirement) |

## Acceptance criteria (brief §15)

| # | Criterion | Met | Still needs |
|---|---|---|---|
| 1 | From launch to any nested folder requires only typing + Enter (+Tab), no mouse, including after using a filter | yes |  |
| 2 | Ghost text never alters typed characters; Backspace on ghost deletes it without re-offering; the first keystroke after focusing the bar replaces all | yes |  |
| 3 | Typing a partial name highlights the matching row in the same frame; Tab on a directory offers the next level immediately | yes |  |
| 4 | Alias, filter, command and path input are disambiguated deterministically, with a visible mode chip | no | VIS-03 |
| 5 | Command arguments (:mkdir "a b", :alias p ~/x, :cd C:\Windows) can be typed in full without placeholder interference | no | BAR-05 |
| 6 | Every command in section 8 works; every key in section 10 works; nothing panics on invalid input (fuzz-tested parser) | no | CMD-11, KEY-07 |
| 7 | Sorting is stable, folders-first, and survives filter and hidden toggles | yes |  |
| 8 | Cut/paste across folders moves the item; collisions auto-rename; moving a folder into itself is rejected; the clipboard interoperates with Explorer in both directions | no | CLP-07 |
| 9 | Delete goes to the Recycle Bin and Ctrl+Z restores it | no | TRS-01 |
| 10 | The current path is visible at all times (bar, status line, title) | no | VIS-05 |
| 11 | Single nocrumbs.exe, no installer, runs with no config present, at most 10 MB, within the startup and keystroke budgets | no | DST-06, PRF-03, PRF-06 |
| 12 | Behaviour in sections 4-10 is covered by automated keystroke-script tests against the core and the in-memory filesystem | yes |  |

## Requirements

### 3 Platform and distribution

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| DST-01 | The core is UI-free and testable headless against an in-memory filesystem | implemented | 2 |  |
| DST-02 | All filesystem access goes through the Fs trait; a read-only Fs is valid and says what it cannot do | implemented | 2 |  |
| DST-03 | Portable mode: a nocrumbs.toml next to the exe holds all state | implemented | 1 |  |
| DST-04 | Otherwise state lives in the user's config folder | implemented | 2 |  |
| DST-05 | Fully functional with no config file present | implemented | 1 |  |
| DST-06 | One statically linked nocrumbs.exe, no installer, at most 10 MB, cold start at most 150 ms | planned | 0 | lands in M3 |
| DST-07 | Release builds are code-signed | planned | 0 | lands in M3 |
| DST-08 | Linux (X11 and Wayland) port | planned | 0 | lands in M4 |
| DST-09 | macOS port (Cmd keys, Trash API) | planned | 0 | lands in M5 |

### 4 The address bar

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| BAR-01 | Input is resolved in a fixed order: command, then glob, then alias, then literal path | implemented | 1 |  |
| BAR-02 | Empty input plus Enter resets the bar to the current path | implemented | 1 |  |
| BAR-03 | Literal paths: absolute, home (~), relative, and .. | implemented | 3 |  |
| BAR-04 | Both / and \ are accepted as separators on input, and the typed one is kept | implemented | 1 |  |
| BAR-05 | Windows drive letters (C:\) and UNC paths (\\server\share) | planned | 0 | lands in M2 |
| BAR-06 | A file whose name starts with a colon is reachable as .\:name | planned | 0 | lands in M2 |

### 5 Inline completion

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| CMP-01 | The best completion is appended in the bar as selected ghost text | implemented | 2 |  |
| CMP-02 | The user's typed characters never move or change while ghosting (case included) | implemented | 5 |  |
| CMP-03 | The row Tab would go to is highlighted, distinct from the selection | implemented | 2 |  |
| CMP-04 | Tab accepts the completion and the completed segment takes the filesystem's casing | implemented | 3 |  |
| CMP-05 | Completing a folder appends the separator and immediately offers the next level (descent by Tab alone) | implemented | 3 |  |
| CMP-06 | Backspace or Delete on ghost text removes it and does not re-offer it | implemented | 3 |  |
| CMP-07 | Prefix matching is case-insensitive (Windows); case-sensitive first with a case-insensitive fallback elsewhere | implemented | 2 |  |
| CMP-08 | Dotfiles and hidden entries are candidates when a dot is typed or hidden files are shown | implemented | 2 |  |
| CMP-09 | Commands ghost-complete the command word and fixed keywords only, never placeholders | implemented | 3 |  |
| CMP-10 | Alias names are completion candidates for the first path segment | implemented | 1 |  |
| CMP-11 | A name that exists nowhere gets no ghost and no highlight; Enter reports No such path | implemented | 3 |  |
| CMP-12 | Enter acts on the completed text, as a browser's URL bar does | implemented | 2 |  |
| CMP-13 | Candidates are ranked by frecency (use and recency), then alphabetically | implemented | 5 |  |
| CMP-14 | While typing toward another folder the list previews it without navigating | implemented | 1 |  |
| CMP-15 | Other matching candidates are signalled (+N more, fainter tint on their rows) | implemented | 6 |  |
| CMP-16 | No completion is offered while an IME is composing | implemented | 2 |  |
| CMP-17 | Moving the caret off the ghost commits it as typed text; no ghost when the caret is not at the end | implemented | 3 |  |
| CMP-18 | :cd PATH and the path of :alias NAME PATH complete like a path, appending only | implemented | 3 |  |
| CMP-19 | The ghost is cut by the name's own characters, even when lower-casing changes how many there are | implemented | 4 |  |

### 6 Aliases

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| ALI-01 | :alias NAME PATH creates, :unalias NAME removes, :alias alone lists | implemented | 4 |  |
| ALI-02 | Aliases compose with subpaths and globs (proj/sub, dl/*.pdf) | implemented | 2 |  |
| ALI-03 | An alias in use shows a visible alias mode chip | partial | 1 | the core reports the alias in use; the chip itself is drawn by the egui front end and not asserted by a test |
| ALI-04 | Alias names are validated (one segment; no * ? . spaces; no leading : or ~) | implemented | 2 |  |
| ALI-05 | An alias target is validated at creation: it must be an existing folder | implemented | 1 |  |
| ALI-06 | A dangling alias target at use time gets its own message naming :unalias | implemented | 1 |  |
| ALI-07 | An alias beats a real folder of the same name, and the bar says so | implemented | 1 |  |
| ALI-08 | Aliases persist across sessions | implemented | 3 |  |
| ALI-09 | A name that is exactly an alias previews its target as you type | implemented | 1 |  |

### 7 Glob filtering

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| FLT-01 | * and ? filter the folder live, case-insensitively, on every keystroke | implemented | 3 |  |
| FLT-02 | A path before the glob selects the folder (~/Downloads/*.pdf) | implemented | 2 |  |
| FLT-03 | A filter chip marks the mode; the status shows the path and the match counts | partial | 2 | path and counts are asserted; the chip is drawn by the egui front end |
| FLT-04 | Esc clears the filter and restores the full listing | implemented | 1 |  |
| FLT-05 | Filtering composes with sorting and hidden-file visibility | implemented | 1 |  |
| FLT-06 | Ctrl+A selects all filtered rows | implemented | 1 |  |
| FLT-07 | Enter with a filter keeps the filter active | implemented | 1 |  |
| FLT-08 | A literal * or ? in a file name can be escaped with backslash (Unix) | planned | 0 | lands in M4 |

### 8 Commands

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| CMD-01 | :sort name\|size\|date [asc\|desc], folders first, ties by name | implemented | 4 |  |
| CMD-02 | :asc and :desc flip the direction | implemented | 1 |  |
| CMD-03 | :hidden toggles hidden-file visibility | implemented | 1 |  |
| CMD-04 | :cd PATH navigates (aliases and unquoted spaces work) | implemented | 1 |  |
| CMD-05 | :back and :fwd walk the history (also Alt+Left and Alt+Right) | implemented | 4 |  |
| CMD-06 | :mkdir NAME and :touch NAME create in the current folder; collisions are rejected | implemented | 3 |  |
| CMD-07 | :rename NEWNAME renames the one selected entry | implemented | 4 |  |
| CMD-08 | :rm moves the selection to the trash | implemented | 3 |  |
| CMD-09 | :rm! deletes permanently, and always asks first (Enter confirms, Esc cancels) | implemented | 3 |  |
| CMD-10 | :copy, :cut and :paste work on the selection | implemented | 4 |  |
| CMD-11 | :open opens the selection with the OS default handler | partial | 2 | the core enters folders and asks the front end to open files; launching the default handler comes with the Windows shell (M2) |
| CMD-12 | :undo and :redo | implemented | 2 |  |
| CMD-13 | :help gives a one-line reminder of every command | implemented | 1 |  |
| CMD-14 | :log shows recent operations | implemented | 1 |  |
| CMD-15 | Arguments: whitespace-separated, double quotes group, backslash-quote escapes, other backslashes literal | implemented | 3 |  |
| CMD-16 | Commands with one free-text argument (:cd :mkdir :touch :rename) take the rest of the line | implemented | 2 |  |
| CMD-17 | Unknown commands and bad arguments give a friendly error with usage, never a crash | implemented | 4 |  |
| CMD-18 | A successful command returns the bar to the idle path; an error keeps the text so it can be fixed | implemented | 2 |  |
| CMD-19 | Command words are case-insensitive | implemented | 1 |  |
| FUZ-01 | The bar parser is fuzz-tested: no input panics | implemented | 7 | randomised property tests (proptest) with shrinking, not coverage-guided fuzzing |
| SRT-01 | Folders always sort before files; the direction applies within each group | implemented | 3 |  |
| SRT-02 | Clicking a column header sorts by it, and clicking again flips the direction | implemented | 1 |  |

### 8.2 Name validation

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| NAM-01 | Empty names, . and .., separators and NUL are rejected everywhere | implemented | 1 |  |
| NAM-02 | Windows: no < > : " \| ? *, no control characters, no trailing space or dot | implemented | 2 |  |
| NAM-03 | Windows: reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9), with or without an extension | implemented | 1 |  |
| NAM-04 | Unix allows what Windows forbids, except separators and dot names | implemented | 2 |  |
| NAM-05 | Collision checks follow the filesystem's case rule, with a specific message each | implemented | 3 |  |

### 9 File operations

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| SEL-01 | Destructive operations act only on the explicit selection, never on the completion highlight | implemented | 2 |  |
| SEL-02 | Multi-select: Ctrl+click toggles, Shift+click and Shift+arrows extend, Ctrl+A takes everything shown | implemented | 2 |  |
| SEL-03 | The selection clears when the list changes | implemented | 3 |  |
| SEL-04 | The status line states the selection count and total size | implemented | 3 | the size is that of the selected files; folders are counted but their contents are not measured |
| SEL-05 | Typing the path of a file selects it in its folder | implemented | 1 |  |
| HIS-01 | The current folder always exists: history skips folders that are gone, and removing the current folder moves to the nearest survivor | implemented | 5 |  |
| OPN-01 | Enter opens the selected row (folder: navigate; file: the platform opens it), from the bar too when its text is unchanged since the arrow keys or a click selected it | implemented | 10 |  |
| REN-01 | F2 renames inline in the name cell (basename pre-selected; Enter commits, Esc reverts) | planned | 0 | lands in M3 |
| CLP-01 | Pasting into a folder with a colliding name renames the copy: name (2) before the extension | implemented | 2 |  |
| CLP-02 | Cut then paste moves the item and uses up the clipboard; a copy can be pasted again | implemented | 3 |  |
| CLP-03 | A folder can never be copied or moved into itself or one of its descendants | implemented | 2 |  |
| CLP-04 | Renaming a cut or copied item updates the clipboard instead of duplicating or losing it | implemented | 2 |  |
| CLP-05 | A source that was deleted after copying is reported, never resurrected | implemented | 1 |  |
| CLP-06 | A persistent status-bar indicator shows the clipboard operation and item | partial | 2 | the core reports the summary (Copied x, Cut 3 items); the chip is drawn by the egui front end and not asserted |
| CLP-07 | The clipboard is the real system clipboard (CF_HDROP): copy in NoCrumbs, paste in Explorer, and back | planned | 0 | lands in M2 |
| CLP-08 | Ctrl+C/X/V act on files when the list has focus and on text when the bar has focus | implemented | 3 |  |
| TRS-01 | Delete uses the real Recycle Bin (trash) on the platform | planned | 0 | lands in M2 |
| UND-01 | Create, rename, move, paste and trash are undoable and redoable | implemented | 4 |  |
| UND-02 | The undo stack is at least 20 deep | implemented | 1 |  |
| UND-03 | Undo never deletes for good, and one undo reverses a whole multi-item action | implemented | 2 |  |
| UND-04 | If the world changed underneath an undo, it says so and leaves the step undoable | implemented | 2 |  |
| UND-05 | Permanent deletes are not undoable | implemented | 1 |  |
| ASY-01 | Copies, moves and deletes run on a worker thread with progress and cancel | planned | 0 | lands in M2 |
| ASY-02 | Failures are reported per item with a retry or skip choice; long paths are supported | planned | 0 | lands in M2 |

### 10 Keyboard

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| KEY-01 | / or Ctrl+L focuses the bar from anywhere | implemented | 2 |  |
| KEY-02 | The first keystroke after focusing the bar replaces its whole content | implemented | 3 |  |
| KEY-03 | Tab accepts the completion and never moves focus out of the bar | implemented | 2 |  |
| KEY-04 | Up and Down move the row selection: wrapping from the bar, stopping at the ends in the list; Shift extends without wrapping | implemented | 4 |  |
| KEY-05 | Alt+Left and Alt+Right go back and forward in the history | implemented | 2 |  |
| KEY-06 | Backspace in the list goes up one folder | implemented | 2 |  |
| KEY-07 | F2 rename | planned | 0 | lands in M3 |
| KEY-08 | Del recycles; Shift+Del deletes permanently after confirming | implemented | 6 | on Windows egui delivers Shift+Del as a Cut event; the tests send that event, the physical key has not been pressed in a test |
| KEY-09 | Ctrl+Z and Ctrl+Y undo and redo; Ctrl+A selects all (in the list) | implemented | 2 |  |
| KEY-10 | Esc is staged: remove the ghost, then clear the filter and revert the bar, then deselect, then blur | implemented | 6 |  |

### 11 Visual direction

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| VIS-01 | Three distinct row states: selection, completion highlight, hover | partial | 1 | selection and highlight are separate states in the core; checked by eye on 2026-11-23 that a selected row and a highlighted row are distinguishable; hover is egui's and was not specifically reviewed |
| VIS-02 | The current path is visible at all times: in the bar when idle and always in the status line | implemented | 2 |  |
| VIS-03 | Mode chips (filter, alias, command) appear on the bar | manual | 0 | drawn by the egui front end; check by eye that the chip matches the mode while typing (spike checklist) |
| VIS-04 | Dark theme with accent purple; light theme follows the OS | partial | 1 | the spike is dark only; light-follows-OS is not built |
| VIS-05 | Window title is NoCrumbs - <current path>; native window chrome | planned | 0 | lands in M2 |
| VIS-06 | Colour is never the only signal; a dismissible key-hint strip replaces the permanent hint | planned | 0 | lands in M3 |

### 12 Performance

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| PRF-01 | Directory listings are read asynchronously with cancellation; stale results are discarded | planned | 0 | lands in M2 |
| PRF-02 | A 100,000-entry folder lists, scrolls, completes and filters | implemented | 5 |  |
| PRF-03 | Keystroke to repaint takes at most 16 ms for folders up to 10k entries | manual | 0 | measured by `cargo test -p m0-ghost-bar --profile perf --test bar perf_100k -- --ignored --nocapture`: 6-9 ms typing and about 13 ms per glob keystroke at 100k, headless layout only; real-GPU numbers are not measured |
| PRF-04 | Filtering works over the cached listing, incrementally, without re-reading the folder | partial | 1 | filters over the cached listing; narrowing the previous result instead of rescanning is not done |
| PRF-05 | Directories are watched for changes and the list updates live, keeping the selection | planned | 0 | lands in M2 |
| PRF-06 | Cold start to first painted listing within 150 ms; navigation to a cached folder within 50 ms | planned | 0 | lands in M3 |

### 13 Accessibility and input

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| ACC-01 | The bar, list and status line are exposed to screen readers (AccessKit) | manual | 0 | PARKED: check with Narrator (installed) or NVDA: the bar is announced as an edit field with its text, rows are reachable and announced, the status line is read. Not yet run; a gate before the bar is declared accessible and before any release (docs/spike.md) |
| ACC-02 | Everything is operable by keyboard alone, with a visible focus indicator | partial | 4 | navigation, filtering, opening and file operations work without the mouse; F2 inline rename is not built (use :rename), and the focus indicator has not been reviewed |
| ACC-03 | IME composition works in the bar | partial | 2 | completion is suppressed during composition (tested through egui's IME events); a real IME on a real window was skipped on 2026-11-23 for lack of an installed IME and is still unchecked |
| ACC-04 | Full Unicode names are matched, shown and never rewritten on disk | partial | 2 | matching and storage handle Unicode; the default egui fonts draw tofu for scripts they lack, so a font fallback stack is needed (M2) |
| ACC-05 | High-contrast mode, OS text scaling and reduced motion are respected | planned | 0 | lands in M3 |

### 16 Testing

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| TST-01 | Behaviour is specified as keystroke scripts against the headless core and the in-memory filesystem | implemented | 4 |  |
| TST-02 | Every bug found in the HTML concept sketch is a regression test | implemented | 6 |  |

### Persistence and settings

| ID | Requirement | Status | Tests | Notes |
|---|---|---|---|---|
| CFG-01 | Settings are saved atomically (temporary file, then replace) and the folder is created | implemented | 3 |  |
| CFG-02 | A damaged settings file is set aside as .bad, never overwritten, and the user is told | implemented | 3 |  |
| CFG-03 | A settings file from a newer version is refused and left untouched | implemented | 2 |  |
| CFG-04 | A failing disk is reported once and the bar keeps working in memory | implemented | 1 |  |
| CFG-05 | Frecency survives restarts, follows renames, is capped, and never offers what no longer exists | implemented | 5 |  |
| CFG-06 | Unknown keys are ignored and invalid alias names skipped when loading; a missing version means 1 | implemented | 2 |  |
