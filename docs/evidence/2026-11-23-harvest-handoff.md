# Evidence: the hand-off between two gatherers (B-162)

Plan: [`docs/plans/2026-11-23-harvest-handoff.md`](../plans/2026-11-23-harvest-handoff.md). Measured on the package
branch `harvest-handoff-plan` on top of `main` `e8cc3f80`, 2026-11-23, on the shared VM. Scratch material lives under
`~/scratch/harvest-handoff/` (`scripts/`, `tool/`, `bin-<wp>/`, `tree-<wp>/`, `world-<wp>/`, `income/`,
`golden-<label>/`, `client-<wp>/`).

## 1. WP0: baseline

### 1.1 Commands

The splash-cliff scripts, copied to `scripts/measure.sh` and `compare.sh`; `scripts/wp.sh <wp>` builds
`matchup_trace`, `route_trace` and the scratch income tool from the worktree's `HEAD` in release, copies them to
`bin-<wp>/` and `HEAD`'s `assets/` to `tree-<wp>/`, runs `measure.sh` into `world-<wp>/` (route_trace with `--assets`
given explicitly), compares it with `world-wp0/`, and runs `scripts/income.sh`. The income tool is the occupancy
plan's `harvest_occ.rs` as it was used for that plan's WP2 to WP3 (`tool/harvest_occ.rs`, moves counted as changes of
a worker's `Harvest` target between ticks); it is copied into `crates/sim_core/examples/` only for the build and never
committed. Line mode: one world, `k` workers sent to each node of the line, 1,500 ticks of warm-up, a 3,000-tick
window, the take-period rate per node summed. Node mode: a fresh world per node, 750 ticks of warm-up.

### 1.2 T6 re-checked

`grep -rln Harvest assets/scenarios crates/sim_core/tests/fixtures crates/sim_fixtures crates/sim_core/benches`
finds nothing: no shipped scenario, golden, matrix battle, sandbox or bench issues a `Harvest` order.

### 1.3 Worlds (`world-wp0/`)

The matrix (every battle, seeds of record and 16: 1,180 runs, 0 expectation failures on the seeds of record, 4 over
all seeds), battle 31 on 100 seeds, battle 33 on 64, the showcase and showcase effects scenarios on their default seed
and seeds 1 to 8 for 6,000 ticks: 483 files, the reference for every later comparison.

### 1.4 Income (`income/wp0-*`)

Every figure reproduces the occupancy evidence (`docs/evidence/2026-11-23-harvest-occupancy.md` §2) exactly, so the
derivation of plan §6.1 stands on the measured `T_f` and needs no amendment.

One-worker take periods `T_f` (line mode, `k` = 1, ticks):

| Line | Fields in node order | Line, a minute |
|---|---|---:|
| Showcase slot 0 | 137, 121, 123, 137, 147, 135, 147, 133 | 446.44 |
| Showcase slot 1 | 137, 121, 121, 137, 147, 135, 147, 135 | 446.61 |
| `default_arena` slot 1 | 129, 121, 123, 143, 139, 135, 153, 176 | 434.69 |

Lines, a minute (take-period rate summed; moves in the window and in the whole run all 0):

| Line | `k` = 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|
| Showcase slot 0 | 446.44 | 892.62 | 1,200.00 | 1,200.00 |
| Showcase slot 1 | 446.61 | 890.78 | 1,200.00 | 1,200.00 |
| `default_arena` slot 1 | 434.69 | 871.76 | 1,174.80 | 1,200.00 |

Geysers (node mode, take-period rate a minute):

| Geyser | `k` = 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|
| Showcase near (10, 1) | 60.61 | 121.21 | 171.43 | 171.43 |
| Showcase corner (1, 1) | 44.44 | 88.89 | 133.33 | 171.43 |
| `default_arena` slot 0 (13, 12) | 47.24 | 95.44 | 141.73 | 171.43 |
| `default_arena` slot 1 (50, 56) | 60.61 | 121.21 | 171.43 | 171.43 |

Node mode, one worker on each showcase slot-0 field: worker 0's mean trip 137, 121, 123, 137, 147, 135, 147, 133
ticks, the line's `T_f`.

### 1.5 Records

`DESIGN.md` §4: B-162 points to this plan and Q0100; B-161 restated as plan D-7 says (not presentation only; open and
deferred to its own plan).

## 2. WP1: the release tick, latent (plan §7.3 rule 1)

### 2.1 Red run

The WP1 tests were written first (`placement_rules` unit tests, `economy::lines` unit tests, the release-tick
mutation of `checksum_state_tests::player_world_map_and_rng_state_are_observable`,
`harvest_handoff_tests::a_zero_handoff_is_todays_rule`): on WP0's tree they do not build (`red-wp1.log`:
`unresolved import lines::ResourceLines`, `ResourceHarvest` has no `handoff_ticks`).

### 2.2 The golden proof (plan §5)

| Step | Tree | Native debug | Native release | `wasm32-wasip1` release | `wasm32-wasip1` debug |
|---|---|---|---|---|---|
| 1 | WP1 with `resource_lines` left out of the encoding, schema 37 (`golden-wp1-unhashed/`) | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 |
| 2 | WP1 hashed, schema 38, re-recorded (`golden-wp1-hashed/`) | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 | 4 of 4; sandbox 3 of 3 |

"4 of 4" is `golden_replay_tests` (both fixtures reproduce every checkpoint); "sandbox 3 of 3" is `sim_fixtures`'
`fixture_tests`, whose `DEFAULT_SANDBOX_CHECKSUM` was `18_404_512_489_137_112_834` at step 1 and is
`13_576_003_452_353_276_199` at step 2. The `wasm32-wasip1` runs used
`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<the worktree's real path>"`. Schema 38 was the next free number on
`origin/main` (`e8cc3f80`, schema 37) when WP1 was built.

### 2.3 Worlds and income

| Check | Result |
|---|---|
| Matrix, battles 31 and 33, both showcases on nine seeds (`world-wp1/`) | 483 files, every file byte-identical to `world-wp0/`, logs included once the output path is removed |
| Income, every case of §1.4 (`income/wp1-*`) | every output file byte-identical to WP0's |
| `cargo test --release -p sim_core` on the harvest, checksum, catalog and footprint targets | pass, the occupancy tests unchanged (`the_next_worker_starts_on_the_tick_of_the_take` among them) |
| Clippy, `sim_core` and `sim_fixtures`, all targets | clean |
