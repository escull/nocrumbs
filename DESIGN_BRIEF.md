# NoCrumbs — Design Brief (v2)
**A native, single-binary file explorer driven entirely by one command-line-style address bar. No breadcrumb trails, ever.**

> v2 changes: NoCrumbs is now a native Rust application shipped as one `.exe` (Windows first, Linux next, macOS later). The HTML sketch in `archive/concept-sketch.html` (from when the project was called OneBar) is a **concept sketch only** — it is not a behavioural reference, and where it disagrees with this document, this document wins. The previous brief is kept in `archive/`.

---

## 1. Vision

NoCrumbs merges the speed and precision of a terminal with the detail-list clarity of a file explorer. The entire navigation model lives in a single address bar that accepts paths, aliases, glob filters, and colon-commands. There are no breadcrumb chips, no back/forward buttons, no sidebar tree. One bar, one list, one status line.

## 2. Core Principles

1. **The bar is the app.** Every navigation, filter, and operation is reachable by typing. Mouse use is optional, never required — including after filtering or selecting.
2. **No breadcrumbs.** The current path is shown in the bar when idle, and **always** in the status line and window title (so it never disappears while the bar is being used for input).
3. **Zero latency feel.** Completion, highlighting, and filtering update on every keystroke. Disk and network I/O never block the UI thread (§12).
4. **Detail list view.** Rows show name, size, modified date, and kind. Sortable by clicking column headers or by command. Folders always sort before files; the chosen direction applies *within* each group.
5. **Speed of a terminal, safety of a GUI.** Destructive actions operate only on the explicit selection (never on the as-you-type highlight), go to the Recycle Bin / trash by default, and are undoable (§8).
6. **One file.** The product is a single executable: no installer, no runtime, no companion files. It runs from a USB stick.

## 3. Platform & Technology

### 3.1 Targets and phasing

| Phase | Target | Notes |
|---|---|---|
| 1 | **Windows 10/11 x64** | Primary. Everything in this brief must work here. |
| 2 | Linux (X11 + Wayland) | Case-sensitive FS, dotfile hiding, XDG trash. |
| 3 | macOS (Apple Silicon + Intel) | Cmd-key mappings, `.DS_Store`/resource-fork quirks, Trash API. |

Phase 1 must not paint later phases into a corner: all OS-specific behaviour sits behind a `Platform` trait (§3.3), and no core logic may assume `\` or `/`, drive letters, or case-sensitivity.

### 3.2 Language and UI toolkit

- **Rust** (stable), single Cargo workspace.
- **UI: `egui`/`eframe` (decided — already used on the author's other projects).** Immediate-mode rendering suits "re-evaluate everything on every keystroke", `egui_extras::TableBuilder` gives a virtualised detail list, it produces one self-contained exe on all three OSes, and it has AccessKit accessibility support. Known risks to validate in the milestone-0 spike (§17): custom ghost-text selection rendering inside a `TextEdit`, IME input, text crispness at small sizes, and screen-reader behaviour of the list.
- Fallbacks if the spike fails: `iced` or `slint` (same architecture, swap the `ui` crate only). A raw Win32 (`windows-rs`) front end is rejected for v1 because it would have to be rewritten for each later platform.
- **Not Tauri/Electron/webview**: they either need an external runtime (WebView2) or ship a browser; neither fits "one small exe".

### 3.3 Architecture (hard requirement)

The UI must be a thin layer over a UI-free core, so the entire behaviour spec is testable without a window.

```
nocrumbs-core    (no UI, no direct OS calls)
  ├─ bar        parse input → Mode {Command | Glob | Alias | Path}; completion engine; command parser
  ├─ model      listing, sort, filter, selection, clipboard, history, undo stack
  └─ traits     Fs (read_dir, metadata, rename, copy, move, trash, …)   Platform (paths, trash, open, clipboard, config dir)
nocrumbs-platform-windows / -linux / -macos   (implement the traits)
nocrumbs-ui      (egui front end: renders model, forwards key events)
nocrumbs-testfs  (in-memory Fs implementation for tests and demos)
```

- Core is driven by `(state, key_event) → (state, effects)`; effects (spawn listing, run op, open file) are executed by the shell layer. This gives deterministic tests and async I/O for free.
- Path handling uses a platform-aware path type (not `String` splitting on `/`). Case-sensitivity and separator come from the `Fs`/`Platform`, not constants.

### 3.4 Distribution

- One statically linked `nocrumbs.exe` (static CRT; no VC++ redistributable needed). Target ≤ 10 MB, cold start to first painted listing ≤ 150 ms on a typical SSD.
- **Portable by default.** Config and history live in `%APPDATA%\NoCrumbs\` — unless a file named `nocrumbs.toml` sits next to the exe, in which case *all* state is stored there (portable mode). The exe is fully functional with no config file present.
- Code-sign release builds (SmartScreen). Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, stripped.

## 4. The Address Bar

Single text input, monospace, top of the window. Input is interpreted in four modes, resolved in this order:

1. **Colon-commands** — input starts with `:` (see §8).
2. **Glob filters** — the last path segment contains `*` or `?` (see §7). A path prefix before the glob selects the folder: `~/Downloads/*.pdf` navigates to `~/Downloads` and filters it; `proj/nocrumbs/*.js` works via alias expansion.
3. **Aliases** — the first path segment matches a saved alias (see §6).
4. **Literal paths** — everything else.

Path syntax on Windows: drive-qualified (`C:\Users`, `c:/users`), UNC (`\\server\share`), home (`~`, `~/Downloads`; `~` = `%USERPROFILE%`), and relative to the current folder (`Documents`, `..`, `projects\nocrumbs`). **Both `\` and `/` are accepted as separators on input**; the bar displays the platform's native separator. A lone drive `C:` means that drive's root. Because colon-commands require a *leading* `:`, drive letters never collide with commands; a file literally named `:foo` is reachable as `.\:foo`.

Empty input + Enter resets the bar to the current path. Names containing spaces are typed literally in paths; in commands use quotes (§8.1).

## 5. Inline Completion (critical — this replaces the dropdown)

There is **no suggestion dropdown**. Completion works like a modern browser URL bar:

- As the user types, the best completion is appended inside the bar as **ghost text**, pre-selected so the next typed character overwrites it. **The user's typed characters never move or change while typing.**
- The matching row in the list is highlighted (accent tint, distinct from selection — §10) so the user sees what Tab will do. If other candidates exist, they get a fainter tint and the status line says `+2 more`.
- **Tab** accepts the completion and places the caret at the end. On accept, the completed segment takes the **canonical casing from the filesystem** (`doc` → `Documents`) — typed case may change at accept time only, never while ghosting.
- **Completing a directory** appends the separator and **immediately offers the next level** (first candidate), so descent works by Tab alone. Candidates for an empty partial are ordered as in §5.1.
- **Backspace/Delete** on ghost text removes the ghost *and does not re-offer it*; completion resumes on the next typed character. **Esc** with ghost text visible removes only the ghost (staged Esc, §10 keyboard map).
- **Case-insensitive prefix match** on Windows (and macOS default); case-sensitive-first with case-insensitive fallback on Linux. Dotfiles/hidden entries are candidates when the partial starts with `.` or when hidden files are shown.
- Commands ghost-complete **the command word and fixed keywords only** (`:sor` → `:sort`, `:sort s` → `:sort size`). Argument placeholders (`NAME`, `PATH`) are *never* inserted as text. Alias names are completion candidates for the first path segment.
- A name that exists nowhere → no ghost, no highlight; Enter reports "No such path" (bar shakes, error in status line).
- Enter with ghost text visible runs/navigates to the completed text (browser-URL-bar behaviour).

### 5.1 Candidate ranking
Prefix matches only. Ranking: (1) entries previously visited/used in this folder, most recent first (frecency, persisted per §9); (2) otherwise alphabetical. This makes `Do` resolve to the folder you actually use rather than always the alphabetically first.

### 5.2 Live preview of the target folder
When the typed path's parent folder differs from the current folder (e.g. `Documents\rec`), the list **previews that folder** with the matching row highlighted, without committing navigation. Esc reverts the preview.

## 6. Aliases

- User-defined named shortcuts: `:alias proj ~/projects`, removed with `:unalias proj`. A small seed set may be offered on first run (user can delete them).
- Typing an alias navigates to its target. Aliases compose with subpaths and globs: `proj/nocrumbs`, `proj/*.md`. The alias chip shows on the bar for any input whose first segment is an alias, with the resolved target in the status line.
- Alias names are validated: no separators, no leading `:` or `~`, no `* ? .`, non-empty. An alias that shadows a real folder in the current directory is allowed but the status line says so.
- Alias names are offered as completion candidates for the first path segment, alphabetically with the folders; an alias
  sorts before a folder of the same name. An alias **beats** a real folder with the same name (alias is step 3 of the
  resolution order, before literal paths) and the status line says so. A name that is exactly an alias previews its
  target folder as you type.
- Alias names are one path segment and may not contain `*`, `?`, `.`, whitespace or separators, or start with `:` or `~`.
- Aliases persist across sessions. The target is validated at creation; a **dangling target at use time** shows "alias `dl` → `D:\Downloads` no longer exists" (not a generic "No such path").

## 7. Glob Filtering

- `*.pdf`, `report-*`, `?2026?` filter the target folder live, on every keystroke. Syntax: `*` (any run, not crossing separators), `?` (one char); `*` and `?` are illegal in Windows filenames, so there is no ambiguity there (on Linux/macOS a literal can be escaped with `\`).
- A "filter" chip marks the mode; the status line shows the **path** and counts (`C:\Users\ed\Downloads · 12 of 47 shown`).
- **Keyboard flow must work without the mouse:** ↑/↓ moves selection within the filtered rows *from the bar*, and **Enter opens/navigates the selected row** (see §9.3). Esc (first press) clears the filter and restores the full listing; selection is preserved on the same entry where possible.
- Filtering composes with sorting and hidden-file visibility. `Ctrl+A` selects all filtered rows (§9.2).

## 8. Colon-Commands

| Command | Behaviour |
|---|---|
| `:sort name\|size\|date [asc\|desc]` | Resort; header arrows reflect it. Ties break by name. |
| `:asc` / `:desc` | Flip current sort direction |
| `:hidden` | Toggle hidden-file visibility (Windows: hidden *attribute*; dotfiles treated as hidden only if `dotfiles_hidden = true` in config, default false on Windows) |
| `:cd PATH` | Navigate (same as typing the path) |
| `:back` / `:fwd` | Navigate history (also Alt+←/Alt+→) |
| `:mkdir NAME` / `:touch NAME` | Create folder / empty file; collisions rejected |
| `:rename NEWNAME` | Rename the **selected** entry |
| `:rm` | Delete the **selection** to the Recycle Bin/trash. `:rm!` deletes permanently (always confirms with count) |
| `:copy` / `:cut` / `:paste` | Clipboard ops on the selection (§9.4) |
| `:open` | Open the selection with the OS default handler |
| `:undo` / `:redo` | Single/multi-step undo (§9.5) |
| `:alias NAME PATH` / `:unalias NAME` | Manage aliases |
| `:help` | One-line reminder of everything above |

A **successful** command returns the bar to the idle current-path text (selected, ready for the next input) with a
confirmation in the status line; an **error** keeps the command text so it can be fixed, flashes the bar, and says
what is wrong and how to use the command (`Usage: :cd PATH`, `Unknown command :foo. Try :help`).

### 8.1 Argument syntax
Arguments are whitespace-separated; **double quotes group** (`:mkdir "my folder"`) and `\"` escapes a quote. If a command takes exactly one free-text argument (`:mkdir`, `:touch`, `:rename`, `:cd`), the **rest of the line** is the argument, so unquoted spaces also work for those.

A quoted argument followed by more text (`:mkdir "a b" c`) is not guessed at: for the free-text commands the line is
taken literally. Command words are case-insensitive. `:rm!` is its own word, not `:rm` with an argument. `:sort KEY`
without a direction keeps the current direction if KEY is already the sort, otherwise starts ascending. Folders stay
first in every sort; the direction applies within each group, and ties break by name.

### 8.2 Name validation (all create/rename/paste operations)
Reject: empty names, `.`/`..`, path separators, and — on Windows — `< > : " | ? *`, control characters, trailing space or dot, and reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1–9`, `LPT1–9`). Collision checks follow the FS's case-sensitivity. Each rejection gives a specific message.

Unknown commands produce a friendly status error, never a crash or panic.

## 9. File Operations

### 9.1 Selection
Click, arrow keys (from the bar or the list), or by typing a path to a file (selects it). **Selection and the as-you-type highlight are different things**: only the selection is ever the target of `:rm`, `:rename`, `:cut`, `:copy`, F2 and Del.

### 9.2 Multi-select (in v1)
`Shift`+click / `Shift`+↑↓ extend; `Ctrl`+click toggles; `Ctrl+A` selects all rows currently shown (including filter results). `:rm`/`:cut`/`:copy` operate on the whole selection; `:rename` and F2 require exactly one. The status line always states the selection count and total size.

### 9.3 Open / Enter
Enter opens the selected row (folder → navigate; file → OS default handler via `ShellExecute`), whether focus is in the list **or in the bar** when the bar text is unchanged since the last arrow-key selection. If the bar text has been edited since, Enter acts on the bar text. A click or Ctrl+A counts like an arrow-key selection;
a selection made by typing a path does not. With several rows selected, the cursor row is opened. Opening a file does not
change folder, so a filter stays in place.

### 9.4 Rename and clipboard
- **Rename (F2):** inline edit in the name cell; Enter commits, Esc reverts, blur commits; pre-selects the basename before the final extension.
- **Clipboard uses the real system clipboard** (Windows `CF_HDROP` + preferred-drop-effect): copy in NoCrumbs → paste in Explorer, and vice versa. Ctrl+C/X/V act on files when the list has focus and on text when the bar has focus. A persistent status-bar indicator shows op + item name/count.
- Paste into a folder with a name collision auto-renames to `name (2)`. Cut+paste = move (same-volume rename, cross-volume copy-then-delete only after the copy verifies).
- **Guards:** pasting/moving a folder into itself or one of its descendants is rejected with a clear message. Renaming a cut/copied item updates the clipboard reference or invalidates it explicitly — it must never silently duplicate.

- **Pasting details (settled in the core):** a colliding name becomes `name (2)`, `name (3)`, ... *before the extension*
  for files (`notes (2).md`) and after the whole name for folders (`Documents (2)`). Cutting something into the folder it is
  already in does nothing ("Already in this folder"). A successful cut is used up; items that fail stay on the clipboard;
  a copy can be pasted again. A source that has since been deleted is reported ("... no longer exists"), never resurrected.
  Pasted items are selected afterwards. `:mkdir` and `:touch` always act on the current folder, whatever is selected.
  `:rename` checks the new name before it checks the selection, and needs exactly one selected entry.
- **`:rm!` asks first:** the status line says what will be deleted and that it cannot be undone; Enter confirms, Esc cancels,
  and typing anything else withdraws the question.

### 9.5 Undo
Delete (to Recycle Bin), rename, move, copy-paste, and create are undoable via `Ctrl+Z`/`:undo` (stack depth ≥ 20, cleared on exit; permanent deletes are not undoable and always confirm). Every operation is logged to the status bar and to an in-memory operation log viewable with `:log`.

Undo never deletes anything for good: undoing a create or a paste moves the created item to the trash, and redo restores it.
One undo reverses a whole multi-item action (a paste of three items is one step). If the world has changed underneath an
undo (the item is already gone), it says so, leaves the step undoable, and never half-applies silently.

### 9.6 Long operations
Copies/moves/deletes run on a worker thread, show progress + cancel in the status line, and never block navigation or typing. Failures (permission denied, file in use, path too long) are reported per item with a retry/skip choice; Windows long paths (`\\?\`) are supported.

## 10. Keyboard Map (must all work)

| Key | Action |
|---|---|
| `/` or `Ctrl+L` | Focus the bar from anywhere; first keystroke after focus replaces the whole content |
| `Tab` | Accept completion (never moves focus out of the bar) |
| `Enter` | Navigate / run / open (§9.3) |
| `↑` / `↓` | Move row selection (from bar or list), no wrap at the ends in the list, wrap in the bar |
| `Alt+←` / `Alt+→` | History back / forward |
| `Backspace` (list focus) | Up one folder |
| `F2` / `Del` / `Shift+Del` | Rename / recycle / permanent delete (confirm). On Windows `Shift+Del` reaches egui as a *Cut* event, so the front end must read it as permanent delete in the list, never as a cut |
| `Ctrl+C/X/V`, `Ctrl+Z/Y`, `Ctrl+A` | Clipboard, undo/redo, select all |
| `Esc` | **Staged:** (1) remove ghost text → (2) clear filter/preview and revert bar to current path → (3) deselect → (4) blur the bar |

## 11. Visual Direction

Dark theme (with a light theme following the OS setting), accent purple (#6d4aff family), monospace bar, compact dense rows, sticky column headers. Mode chips (filter/alias/command) appear as small pills on the bar.
- **Three visually distinct row states:** *selection* (solid accent tint + left bar), *completion highlight* (outline/lighter tint — "this is what Tab will do"), *hover* (subtle). Selection must be unmistakable because it is the only target of destructive operations.
- Status bar: current path (always), counts and aggregate size, selection summary, sort state, clipboard state, progress, errors (red) and confirmations (green). Colour is never the only signal (icon/prefix as well).
- Window title: `NoCrumbs — <current path>`. Native window chrome — no fake traffic lights.
- A one-time, dismissible key-hint strip replaces the permanent hint block; `:help` brings it back.

## 12. Performance & Responsiveness

- Directory listings are read **asynchronously** with cancellation; typing during a slow read (network share, huge folder) stays responsive and stale results are discarded.
- List is virtualised: 100,000 entries scroll and filter without dropped frames. Filtering is incremental over the cached listing, not a re-read.
- Metadata (size/date/kind) may arrive after names; rows render immediately and fill in.
- Directories are watched for changes (`ReadDirectoryChangesW`) and the list updates live; the selection survives.
- Budgets: keystroke → repaint ≤ 16 ms for folders ≤ 10k entries; navigation to a cached folder ≤ 50 ms.

## 13. Accessibility & Input

- AccessKit (or equivalent): list exposed as a table with row selection, status line as a live region, chips as labelled elements.
- Full keyboard operability (principle 1); visible focus indicator at all times; respect OS text scaling, high-contrast mode, and reduced motion (the shake becomes a colour flash).
- IME composition in the bar must work (no completion while composing).
- Full Unicode paths; display normalisation but never rewrite names on disk.

## 14. Non-Goals (v1)

- No breadcrumbs, sidebar tree, back/forward *buttons*, ribbon/toolbars.
- No thumbnails/preview pane (possible future grid mode).
- No drag-and-drop (clipboard model is canonical); revisit later.
- No tabs/split panes, no recursive search, no archive browsing, no shell-extension/context-menu integration, no plugin system.
- No cloud-provider-specific UI (OneDrive etc. are treated as ordinary folders).

## 15. Acceptance Criteria

1. From launch to any nested folder requires only typing + Enter (+Tab); no mouse, including after using a filter.
2. Ghost text never alters already-typed characters; Backspace on ghost text deletes it without re-offering; the first keystroke after focusing the bar replaces all.
3. Typing a partial name highlights the matching row in the same frame; Tab on a directory offers the next level immediately.
4. Alias, filter, command, and path input are disambiguated deterministically per §4, with a visible mode chip.
5. Command arguments (`:mkdir "a b"`, `:alias p ~/x`, `:cd C:\Windows`) can be typed in full without placeholder interference.
6. Every command in §8 works; every key in §10 works; nothing panics on invalid input (fuzz-tested bar parser).
7. Sorting is stable, folders-first, and survives filter and hidden toggles.
8. Cut/paste across folders moves the item; collisions auto-rename; moving a folder into itself is rejected; clipboard interoperates with Explorer in both directions.
9. Delete goes to the Recycle Bin and `Ctrl+Z` restores it.
10. The current path is visible at all times (bar, status line, title).
11. **Single `nocrumbs.exe`**, no installer, runs with no config present, ≤ 10 MB, passes startup/keystroke budgets in §3.4 and §12.
12. Behaviour in §4–§10 is covered by automated keystroke-script tests against `nocrumbs-core` + `nocrumbs-testfs` (see §16).

## 16. Testing Strategy

- **Keystroke-script tests** (primary): each case is an input sequence plus expected bar text, selection range, highlighted/selected rows, cwd, status line and chip. Run against the core with the in-memory FS, on every platform, in CI. Every bug found in the HTML concept (placeholder overwrite, Backspace re-ghosting, case rewrite, Tab descent, folder-into-self, path hidden during filter, filter without keyboard exit) becomes a regression case.
- **Property/fuzz tests** for the bar parser, glob matcher, name validator and path normaliser (no panics, round-trip display ↔ parse).
- **Platform tests** (Windows CI): real-FS operations in a temp dir — Recycle Bin, long paths, reserved names, locked files, UNC, case-insensitive collisions, clipboard interop.
- **UI smoke test**: launch, type, screenshot; plus the toolkit spike below.

## 17. Milestones

0. **Spike (1–2 days):** egui prototype proving ghost-text selection in the bar, a 100k-row virtual list, IME, and a screen-reader pass. Go/no-go on the toolkit (§3.2).
1. **Core:** `nocrumbs-core` + `nocrumbs-testfs` + keystroke-test harness; all of §4–§8 passing headless.
2. **Windows shell:** real `Fs`/`Platform`, async listing, trash, clipboard, watcher, config/history persistence.
3. **UI polish & accessibility;** signed single-exe release (v1.0 Windows).
4. Linux port (v1.x), then macOS.

## 18. Open Decisions

1. ~~Toolkit~~ **Decided: egui/eframe.** Revisit only if the milestone-0 spike fails on ghost-text selection, IME or accessibility (fallbacks in §3.2).
2. Whether `:` command prefix should be configurable (e.g. for users who prefer `>`), given drive letters are already safe.
3. Windows "root" view: should `\` or `This PC` list the drives as a pseudo-folder? (Recommended: yes, `\` at top level lists drives.)
4. ~~Frecency store format and retention (§5.1)~~ **Decided:** frecency and aliases live in `config.toml` (or the portable `nocrumbs.toml`), versioned, written atomically; at most 2,000 paths, the least-used dropped first; score = visits x a weight that falls with age (x16 within the hour, x8 day, x4 week, x1 after). The file records folder and file *paths you used*, so it is personal data: it never leaves the machine, and portable mode keeps it beside the exe.
5. Dotfile hiding default on Windows (recommended: off; hidden = attribute only).
6. Opening files: add a configurable per-extension handler table, or leave strictly to the OS default?
7. App name/branding check and code-signing certificate source.

## 19. Project Management

Progress is tracked in [ROADMAP.md](ROADMAP.md): a one-page status with stage, milestones, ranked upcoming work, risks and a dated progress log. This brief is the *what*; the roadmap is the *where we are*. Requirements have stable IDs (e.g. `BAR-05`, `CMD-12`) in [spec/requirements.toml](spec/requirements.toml), each mapped to the tests that prove it. `cargo xtask spec` checks the mapping, and [spec/COVERAGE.md](spec/COVERAGE.md) (generated) says what is implemented, partial, manual or planned and which of the §15 acceptance criteria are met, so progress is measured, not guessed. A change to behaviour adds or updates a requirement and its tests in the same commit.
