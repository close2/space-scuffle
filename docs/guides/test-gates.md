# Guide: Test Gate Tiers

This guide holds the operating detail of charter §4.3 (`AGENTS.md`). The charter states the rules; this guide says
how the tiers work, and binds like the charter. It was moved out of `AGENTS.md` word for word (Q0084, Q0085). The
tool is `crates/gate`; its catalog is `crates/gate/suites.ron`.

## What each tier runs

- `cargo gate quick`: optional, while working: fmt, clippy on the changed crates, their fast tests.
- `cargo gate package`: required once per package, after merging `main`, on the package's final tree. Clippy on the
  whole workspace, `workspace_checks`, every test of the changed crates (GPU and valgrind tests when the change
  touches their triggers), the tests the catalog's path rules and downstream contracts select, and the golden
  replays in release and on `wasm32-wasip1` for simulation changes. The implementation agent hands back after
  `quick` and the tests its change needs; a verifier agent on a cheap model then runs `package` and reports the
  stamp, or the failing targets, which go back to the implementation agent (owner, Q0061).
- `cargo gate merge <branch>`: the orchestrator, for each merge to `main`.
- `cargo gate full`: the full gate, on `main` before a push that contains a crate change, and on CI. Push only a
  commit whose tree has a green stamp. When it is red, fix forward or revert within an hour, and add the missing
  catalog rule.
- Docs-only changes: `cargo gate package` runs fmt and `workspace_checks` only.

## Test a tree once

A tier passes at once, without building or running anything, when its tree already has a green stamp of that tier
or a stricter one; a green `package` stamp counts for `merge` and `full` when the agent merged `main` right before
its gate and nothing landed since (the merged tree is the tested tree). Merge branches one at a time in that order,
batch pushes to a few a day, and run at most one or two building agents at once (owner, Q0055).

## The full gate

What CI runs on every push:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo gate full`: every test executable and doc test of the workspace, each executable with its thread cap from
  `crates/gate/suites.ron` so that the run fits the runner's memory, and the golden replays.

The charter's own **Full gate** list is the one check C16 reads; this copy only explains it.
