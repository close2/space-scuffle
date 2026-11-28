# Guide: Tests and Test Gate Tiers

This guide holds the operating detail of charter §4 (`AGENTS.md`): the test subjects every crate covers, what each
`cargo gate` tier runs, and the full gate CI runs. The charter states the rules; this guide says how they are met,
and binds like the charter. The gate tool is `crates/gate`; its catalog is `crates/gate/suites.ron`. Check C15
reads the table of test subjects below, and check C16 reads the full gate list and every `cargo` command named here.

## Test subjects

Every crate has `#[cfg(test)]` unit tests and `tests/*.rs` integration tests that verify end-to-end subsystem
behaviour through public APIs. Each crate covers at least the subjects below, in the files named:

| Crate | Subject | Test files |
|---|---|---|
| `sim_types` | Fixed-point identities, overflow policy, trig, angle wrap, serialization; property tests | `math_identities.rs`, `overflow_policy_tests.rs`, `trig_determinism.rs`, `angle_extremes_tests.rs`, `scalar_serialization.rs`, `snapshot_serialization.rs`, `math_properties.rs` |
| `sim_map` | High-ground line of sight, air reveal, ramps, structure wall-offs, clearance field, navigation mesh, triangle A* and radius funnel; property tests | `vision_tests.rs`, `ramp_vision_tests.rs`, `wall_off_tests.rs`, `clearance_field_tests.rs`, `navmesh_tests.rs`, `funnel_tests.rs`, `reachability_tests.rs`, `pathfinding_properties.rs`, `validation_tests.rs` |
| `sim_core` | Weapon timing with cancelable backswing, turrets, shield and armour spillover, push priority versus Hold Position, a whole match through `PlayerCommand`, cross-target golden replay | `stutter_step_tests.rs`, `turret_tracking_tests.rs`, `damage_and_shields_tests.rs`, `push_priority_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs` |
| `asset_baker` | Fixture flipbooks, frame validation, manifest round trip, media integrity, procedural output kept out of runtime paths | `procedural_tests.rs`, `bake_integrity_tests.rs`, `manifest_tests.rs`, `media_tests.rs` |
| `client` | Shader validation, uniform packing, camera, scene transitions, a skirmish won through input alone | `gpu_validation_tests.rs`, `round2_visual_tests.rs`, `camera_drag_tests.rs`, `lab_scene_tests.rs`, `skirmish_match_tests.rs` |
| `client_diagnostics` | Capture pipeline, one-way dependency on `client`, and the showcase storyboard, cards and a reproducible take | `runtime_capture_tests.rs`, `dependency_direction_tests.rs`, `showcase_tests.rs` |
| `sim_fixtures` | The embedded catalogs equal the shipped files; the battle sandbox is built whole | `fixture_tests.rs` |
| `workspace_checks` | The charter checks of charter §9 (listed in `docs/guides/charter-checks.md`) | `charter.rs` |
| `gate` | Selection, globs, scheduling, stamps, catalog consistency | `selection_tests.rs`, `suite_catalog_tests.rs` |

## What each tier runs

- `cargo gate quick`: optional, while working: fmt, clippy on the changed crates, their fast tests.
- `cargo gate package`: required once per package, after merging `main`, on the package's final tree. Clippy on the
  whole workspace, `workspace_checks`, every test of the changed crates (GPU and valgrind tests when the change
  touches their triggers), the tests the catalog's path rules and downstream contracts select, and the golden
  replays in release and on `wasm32-wasip1` for simulation changes. The implementation agent hands back after
  `quick` and the tests its change needs; a verifier agent on a cheap model then runs `package` and reports the
  stamp, or the failing targets, which go back to the implementation agent.
- `cargo gate merge <branch>`: the orchestrator, for each merge to `main`.
- `cargo gate full`: the full gate, on `main` before a push that contains a crate change, and on CI. Push only a
  commit whose tree has a green stamp. When it is red, fix forward or revert within an hour, and add the missing
  catalog rule.
- Docs-only changes: `cargo gate package` runs fmt and `workspace_checks` only.

## Test a tree once

A tier passes at once, without building or running anything, when its tree already has a green stamp of that tier
or a stricter one; a green `package` stamp counts for `merge` and `full` when the agent merged `main` right before
its gate and nothing landed since (the merged tree is the tested tree). Merge branches one at a time in that order,
batch pushes to a few a day, and run at most one or two building agents at once.

## The full gate

- **Full gate** (what CI runs on every push):
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo gate full`: every test executable and doc test of the workspace, each executable with its thread cap
    from `crates/gate/suites.ron` so that the run fits the runner's memory, and the golden replays.
- **Shards on CI.** CI runs the full gate as three parallel jobs, each `cargo gate full` with
  `SPACE_SCUFFLE_GATE_SHARD=<k>/3`. A target's `shard: Some(k)` in `crates/gate/suites.ron` assigns it to shard
  `k`; the last shard runs every unassigned target, clippy, the doc tests and the golden replays, so the shards
  together run the complete suite, each target once. A shard writes no stamp. Assign a long GPU or valgrind target
  to the lighter shard when it would push a shard past about 40 minutes of tests on CI.
