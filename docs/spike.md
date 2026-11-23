# Milestone 0 spike: can egui do the NoCrumbs bar?

Status: **automated checks pass; two human checks (IME, screen reader) still open.** Go/no-go is
therefore *provisionally go*; see [Verdict](#verdict).

## Run it

```powershell
cargo run -p m0-ghost-bar                      # 100,000-entry /big folder, starts in ~
cargo run -p m0-ghost-bar -- --start /big      # starts inside the 100k folder
cargo run -p m0-ghost-bar -- --small           # 1,000 entries instead
cargo run -p m0-ghost-bar -- --state spike.toml  # keep aliases and frecency in spike.toml between runs
cargo test -p m0-ghost-bar                     # headless keystroke tests
cargo test -p nocrumbs-testfs                  # completion cases against the in-memory FS
cargo test -p m0-ghost-bar --profile perf --test bar perf_100k -- --ignored --nocapture
```

The filesystem is an in-memory mock (`nocrumbs-testfs`): nothing real is read or changed.
Things to try in the bar: `Doc` then Tab (then Tab again: it descends), `~/Down`, `doc`
(typed case is kept while ghosting, canonical once accepted), `/big/rep`, `*.pdf`,
`~/Downloads/*.pdf`, `:mk` Tab, `:mkdir foo` (nothing overwrites your arguments),
Backspace on ghost text, Esc three times, Up/Down.

## What was built

| Crate | Role |
|---|---|
| `crates/nocrumbs-core` | UI-free: `Bar` (the whole bar state machine), `analyze()`/`resolve()` (command > glob > alias > path), the `:command` parser, name validation, aliases, `Glob`, the `Fs` trait, and `sim::Session` (a simulated text field for tests) |
| `crates/nocrumbs-testfs` | In-memory `Fs` (full write side, folder subtrees, trash) with a deterministic 100k-entry folder; completion cases, 106 keystroke scripts and 12 filesystem tests that run headless in the core |
| `spikes/m0-ghost-bar` | egui/eframe front end (a thin renderer over `Bar`) and 14 headless tests that drive the real widget |

The split: egui's `TextEdit` still does the text editing (caret, mouse, clipboard, IME). The front
end reports `edited(text, caret, kind)`, the field's selection each frame, and the keys Tab / Esc
/ Enter / Up / Down; the bar answers with the text to show and requests to select, scroll or
blur. `sim::Session` plays the front end's part in tests, so behaviour is specified once, in the
core, and the UI tests only prove the egui widget forwards events faithfully.

Persistence and the real filesystem are **not** in the spike. Aliases, `:cd`, `:sort`, `:hidden`, history and, as of the
file-operations step, `:mkdir`/`:touch`/`:rename`/`:rm`/`:rm!`/`:copy`/`:cut`/`:paste`/`:undo`/`:redo` on the in-memory
filesystem are. In the window, press Esc to blur the bar, then Del, Backspace, Ctrl+C/X/V/Z/A and Shift+arrows act on the list.

## Findings

### Question 1: ghost text in a `TextEdit` — works

The ghost is real text at the end of the `TextEdit`, selected via `TextEditState`
(`cursor.set_char_range` + `store`). Typing replaces the selection, so typed characters never
move. The app tracks `ghost_chars` and drops it the moment the selection is no longer exactly
that tail. Headless tests confirm: ghost appears selected; typing over it keeps typed text and
case; Backspace removes the ghost and does **not** re-offer it; Tab accepts with canonical
casing and immediately offers the next level; staged Esc (ghost, then revert, then blur);
Enter navigates to the completed text; command arguments are never overwritten.

Things egui does that you have to design around:

1. **`TextEdit` re-sets its own focus-lock filter every frame.** A filter set *before*
   `show()` is overwritten, so Tab moved focus and Esc blurred. The fix: set our
   `EventFilter { tab, arrows, escape }` *after* `show()`.
2. **Selection state must be written through `TextEditState` and takes effect next frame.**
   Set it before `show()` when acting on a key; after `show()` when reacting to an edit.
3. **Whether an edit was a deletion comes from the input events**, not from text lengths
   (typing over a selected ghost can also shorten the text).
4. **Cursor positions are `CharIndex` (a `usize` newtype)**, not byte offsets.

### Question 2: 100,000 rows — works

`egui_extras::TableBuilder` virtualises; the 100k folder shows at ~1 ms per frame in an
optimised build (the debug build's first frames are slower: 17 ms avg / 57 ms max). Headless
layout cost per keystroke (optimised, no GPU, two frames):

| Action in the 100k folder | ms |
|---|---|
| Enter the folder / type next character (scan 100k for the prefix) | 6–9 |
| Backspace | 1 |
| Tab (accept + next level) | 6 |
| Glob filter, per keystroke (`*`, `.`, `pdf`) | 12–20, ~13 typical |

This meets the brief's ≤ 16 ms keystroke budget (§12) for up to 10k entries comfortably and
for 100k narrowly; the glob filter is the slowest because it folds the case of every name.
Easy wins when needed: lower-case names once per listing, and incremental filtering (narrow
the previous result when the new pattern extends the old one).

### Other findings

- **Default fonts lack `⇥ ↵ →`.** In the app that draws tofu; in `egui_kittest` the missing
  glyph policy is *panic*, which makes the test harness a useful guard against tofu in our own
  strings. **Real file names will contain anything**, so the font fallback stack (system fonts
  and CJK/Arabic/Thai faces) is a real requirement, not polish: put it in milestone 2.
- The theme follows the OS unless forced; the spike forces dark (`ThemePreference::Dark`).
  The brief's "light follows the OS" is a later setting.
- `Id::new` is deprecated in egui 0.37 (`Id::unique` replaces it).
- Windows `PrintWindow` can snapshot the app for visual checks, but synthetic `WM_CHAR`/
  `WM_KEYDOWN` posting is not faithful (characters doubled or ignored), so real-window typing
  has to be done by a person. That is why the headless tests carry the weight.

## Still to verify by hand (human)

- [ ] **Ghost text on screen.** Type `Doc`: the rest of `Documents/` shows selected in accent
      purple, the next typed letter replaces it, no flicker or caret jump.
- [ ] **IME.** With the Microsoft Japanese or Pinyin IME, compose text in the bar. Expected:
      no ghost text while composing, candidate window appears at the caret, committed text
      completes normally afterwards. (The headless test only covers the egui event path.)
- [ ] **Screen reader.** With Narrator (Win+Ctrl+Enter) or NVDA: the bar is announced as an
      edit field with its text; the list is reachable and rows are announced; the status line
      is read. egui provides AccessKit, but a list of 100k virtualised rows and a custom
      ghost selection are exactly where it can fall short. Record what is and isn't spoken.
- [ ] **Mouse.** Click in the bar with ghost text showing; click a row; check selection
      colours are distinguishable from the completion highlight.
- [ ] **High DPI / 150%+ scaling**, and a window narrower than 700 px.

## Verdict

**Provisionally go on egui/eframe.** The two riskiest technical questions (ghost-text
selection semantics and a 100k-row list) are answered by passing tests and measurements. The
open risk is accessibility (screen reader on a virtualised list and a custom selection) and
real-IME behaviour; neither is a reason to switch toolkits yet, but each could force a custom
widget, so do the checks above before M1 builds on the bar. Fallbacks stay as in the brief
(§3.2): custom-painted bar, then `iced`/`slint`.
