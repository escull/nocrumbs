# Contributing to NoCrumbs

Thanks for your interest. NoCrumbs is in its spec stage, so the most useful contributions right now are **design feedback and spec bugs**, not code.

## Licence of contributions

NoCrumbs is licensed under `GPL-3.0-or-later`. By submitting a contribution you agree it is licensed under the same terms. We use the [Developer Certificate of Origin](https://developercertificate.org/): sign off every commit with `git commit -s` to certify you have the right to submit it.

New source files should carry this header (once code exists):

```rust
// SPDX-License-Identifier: GPL-3.0-or-later
```

## Ground rules

1. **The brief decides scope.** Read `DESIGN_BRIEF.md`, especially the principles (§2) and non-goals (§14). A change that contradicts them needs a brief change first, discussed in an issue.
2. **Behaviour is specified by tests.** Anything described in §4–§10 of the brief is verified by a keystroke-script test against the core crate. A bug fix includes a regression case; a feature includes its cases.
3. **Keep core UI-free.** `nocrumbs-core` has no UI and no direct OS calls; platform code sits behind the `Fs` and `Platform` traits.
4. **Windows first, but never Windows-only in core.** No `\` or `/` literals, drive-letter assumptions or case-sensitivity constants in core.
5. **Tie behaviour to a requirement.** A new behaviour or a bug fix adds or updates an entry in `spec/requirements.toml` (stable `AREA-NN` id, status, the tests that prove it), then run `cargo xtask spec --write` and commit `spec/COVERAGE.md`. CI runs `cargo xtask spec --check-doc` and fails if the mapping is invalid or the report is stale.
6. **Keep `ROADMAP.md` current.** Update the milestone row and add a one-line progress-log entry when your change lands.

## Reporting issues

Please include: the exact keys typed, what you expected, what happened, and your OS. For completion or filter problems, the bar text and cursor/selection state at each step is ideal; it is exactly what a keystroke-script test needs.
