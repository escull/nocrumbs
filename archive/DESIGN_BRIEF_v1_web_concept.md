# OneBar — Design Brief
**A windowed file explorer driven entirely by one command-line-style address bar. No breadcrumb trails, ever.**

---

## 1. Vision

OneBar merges the speed and precision of a terminal with the detail-list clarity of a file explorer. The entire navigation model lives in a single address bar that accepts paths, aliases, glob filters, and colon-commands. There are no breadcrumb chips, no back/forward buttons, no sidebar tree. One bar, one list, one status line.

A working interactive prototype exists (`onebar-demo.html`, static single-file HTML with a mock filesystem). Use it as the behavioural reference for all interactions described below.

## 2. Core Principles

1. **The bar is the app.** Every navigation, filter, and operation is reachable by typing. Mouse use is optional, never required.
2. **No breadcrumbs.** The current path is shown in the bar itself and echoed in the status bar. Nothing else.
3. **Zero latency feel.** Completion, highlighting, and filtering update on every keystroke.
4. **Detail list view.** Rows show name, size, modified date, and kind. Sortable by clicking column headers. Folders sort before files (respecting the chosen direction).
5. **Speed of a terminal, safety of a GUI.** Destructive actions operate only on the explicit selection; no accidental modifiers.

## 3. The Address Bar

Single text input, monospace, top of the window. It interprets input in four modes, resolved in this order:

1. **Colon-commands** — input starts with `:` (see §7).
2. **Glob filters** — the last path segment contains `*` or `?` (see §6).
3. **Aliases** — the first path segment matches a saved alias (see §5).
4. **Literal paths** — everything else: absolute (`/etc`), home-relative (`~`, `~/Downloads`), and relative to the current folder (`Documents`, `..`, `projects/onebar`).

Empty input + Enter resets the bar to the current path.

## 4. Inline Completion (critical — this replaces the dropdown)

There is **no suggestion dropdown**. Completion works like a modern browser URL bar:

- As the user types, the best completion is **appended inside the bar as ghost text**, pre-selected (highlighted) so the next typed character overwrites it. The user's typed portion never moves.
- Simultaneously, the matching row in the file list is **highlighted** (accent-tinted row), so the user sees what Tab will do before pressing it.
- **Tab** accepts the completion and places the caret at the end. Completing a directory appends the trailing `/` and immediately offers the next level (progressive descent by Tab alone).
- Completion candidates: entries of the deepest existing ancestor folder, case-insensitive prefix match, alphabetically first candidate wins. Commands ghost-complete the same way (`:sor` → `:sort name`).
- Typing a name that exists nowhere → no completion, no highlight; Enter reports "No such path" (bar shakes; error shown in status line).
- Up/down arrows from the bar move the row selection in the list (wrapping), cycling through navigation history when nothing to select.

## 5. Aliases

- Pre-seedable and user-defined named shortcuts: `:alias proj ~/projects`, removed with `:unalias proj`.
- Typing an alias navigates to its target. Aliases compose with subpaths: `proj/onebar`.
- Active alias mode shows a small chip on the bar ("alias").
- Aliases persist across sessions (target must be validated at creation time; dangling targets handled gracefully at use time).

## 6. Glob Filtering

- Typing `*.pdf`, `report-*`, or `?2026?` in the bar filters the current list live, on every keystroke.
- A distinct chip ("filter") marks the mode; the status line shows match counts (`12 of 47 shown`).
- Esc or clearing the bar exits the filter and restores the full listing.
- Filtering composes with column sorting and hidden-file visibility.

## 7. Colon-Commands

| Command | Behaviour |
|---|---|
| `:sort name\|size\|date [asc\|desc]` | Resort list; header arrows reflect it |
| `:asc` / `:desc` | Flip current sort direction |
| `:hidden` | Toggle dotfile visibility |
| `:cd PATH` | Navigate (same as typing the path) |
| `:mkdir NAME` / `:touch NAME` | Create folder / empty file; names collisions rejected |
| `:rename NEWNAME` | Rename highlighted entry |
| `:rm` | Delete highlighted entry |
| `:copy` / `:cut` / `:paste` | Clipboard ops on the selection (§8) |
| `:alias NAME PATH` / `:unalias NAME` | Manage aliases |
| `:help` | One-line reminder of everything above |

Unknown commands produce a friendly status error, never a crash.

## 8. File Operations (GUI parity)

- **Selection:** click a row, or arrow keys from either the bar or the list.
- **Rename (F2):** inline edit in the name cell itself; Enter commits, Esc reverts, blur commits. Pre-selects the basename before the extension.
- **Delete (Del).**
- **Clipboard:** Ctrl+C copy, Ctrl+X cut, Ctrl+V paste. Paste into a folder containing a name collision auto-renames to `name (2)`. Cut+move removes the source. A persistent clipboard indicator in the status bar shows op + item name.
- Opening a file is application-dependent (real build: launch with OS default handler or user-configurable; prototype just reports).
- All destructive operations must be logged to the status bar and be undoable where feasible (single-step undo for delete/rename/paste is a stretch goal, flag it).

## 9. Keyboard Map (must all work)

- `Tab` — accept completion · `Enter` — navigate/run/open
- `/` or `Ctrl+L` — focus the bar from anywhere
- `↑`/`↓` — row selection (also from within the bar) · history recall when list exhausted
- `Backspace` (list focus) — up one folder
- `F2` rename · `Del` delete · `Ctrl+C/X/V` clipboard
- `Esc` — cancel completion / exit filter / deselect

## 10. Visual Direction

Dark theme, accent purple (#6d4aff family), monospace bar, compact dense rows, sticky column headers, subtle hover and selection tints. Mode chips (filter/alias/command) appear as small pills on the bar. Status bar carries: current path echo, counts and aggregate size, sort state, clipboard state, errors (red) and confirmations (green).

## 11. Non-Goals

- No breadcrumbs, no sidebar tree, no back/forward buttons, no ribbon/toolbars.
- No thumbnails/preview pane in v1 (grid preview mode is a possible future addition).
- No drag-and-drop in v1 (clipboard model is canonical); revisit later.

## 12. Acceptance Criteria

1. From launch to any nested folder requires only typing + Enter (+Tab), no mouse.
2. Ghost-text completion never alters already-typed characters; first keystroke after focusing the bar selects all.
3. Typing a partial name highlights the matching row in the list on the same frame.
4. Alias, filter, and command input are disambiguated deterministically per §3 ordering, with a visible mode chip.
5. Every command in §7 works; every key in §9 works; nothing crashes on invalid input.
6. Sorting is stable, folders-group-first, and survives filter and hidden toggles.
7. Cut/paste across folders moves the item; collisions auto-rename; clipboard indicator always reflects state.
8. The prototype runs fully offline as a single HTML file.

## 13. Deliverables

- Single-page app (HTML/CSS/JS or equivalent framework), local-first, no network dependency.
- Behavioural reference: `onebar-demo.html` (mock in-memory FS — safe to explore).
- Open decisions for the build: real-filesystem backend + permissions, persistence layer for aliases/history, undo support, multi-select (Shift/Ctrl-click) if desired.
