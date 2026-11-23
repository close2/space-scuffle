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

## 3. WP2: the hand-off (plan §7.3 rules 2 to 5)

### 3.1 Red run

The WP2 tests were written first and run on WP1's tree (`red-wp2.log`): red are
`harvest_handoff_tests::the_next_worker_starts_handoff_ticks_after_the_take`,
`a_worker_arriving_during_the_handoff_waits_for_it`, `a_worker_arriving_at_a_free_node_starts_at_once`,
`the_handoff_is_shared_by_every_player`, `a_saturated_node_takes_every_time_plus_handoff_ticks` (each asserts first
that the shipped row hands off after the take), `runtime_catalog_tests::an_edited_handoff_changes_the_saturated_period`
(the same) and `catalog_reference_values_tests::the_shipped_handoffs_follow_the_stage_reading` (`(0, 0)` against
`(2, 1)`). `harvest_edge_tests::saturation_follows_the_derivation` and
`harvest_occupancy_tests::one_worker_at_a_time_in_an_extractor_and_gas_workers_wait`, re-derived with `H + h`, pass on
both trees, as they must with `h` = 0 on WP1's. On WP2's tree every one passes (`green-wp2.log`), and
`cargo test --release -p sim_core` passes every target (94, `wp2-simcore.log`), the goldens and the sandbox unchanged
from WP1's re-record.

### 3.2 Income against the derivation (A-1 to A-5)

`T_f` from WP1's one-worker runs (§1.4, identical on WP1); derived `R_f(k) = min(k × 7,500 / T_f, 7,500 / 52)` for
minerals and `min(k × 6,000 / T, 6,000 / 36)` for gas, summed over the line (`scripts/analyse.py wp1 wp2`).

| Line | `k` | Derived | Measured (WP2) | Difference | Worst field | Moves (window, run) | WP1 |
|---|---:|---:|---:|---:|---:|---|---:|
| Showcase slot 0 | 1 | 446.43 | 446.43 | 0.00 | 0.00 | 0, 0 | 446.44 |
| | 2 | 892.86 | 892.60 | −0.26 | 0.60 | 0, 0 | 892.62 |
| | 3 | 1,153.85 | **1,153.84** | −0.01 | 0.00 | 0, 0 | 1,200.00 |
| | 4 | 1,153.85 | 1,153.84 | −0.01 | 0.00 | 0, 0 | 1,200.00 |
| Showcase slot 1 | 1 | 446.60 | 446.60 | 0.00 | 0.00 | 0, 0 | 446.61 |
| | 2 | 893.20 | 890.68 | −2.52 | 2.01 | 0, 0 | 890.78 |
| | 3 | 1,153.85 | **1,153.84** | −0.01 | 0.00 | 0, 0 | 1,200.00 |
| | 4 | 1,153.85 | 1,153.84 | −0.01 | 0.00 | 0, 0 | 1,200.00 |
| `default_arena` slot 1 | 1 | 434.70 | 434.70 | 0.00 | 0.00 | 0, 0 | 434.69 |
| | 2 | 869.40 | 872.30 | +2.90 | 0.86 | 0, 0 | 871.76 |
| | 3 | 1,137.45 | **1,137.45** | 0.00 | 0.01 | 0, 0 | 1,174.80 |
| | 4 | 1,153.85 | 1,153.84 | −0.01 | 0.00 | 0, 0 | 1,200.00 |

(Differences of 0.01 are the per-field figures' two-decimal rounding: 8 × 144.23.) The arena line at `k` = 3 saturates
its 153-tick field at 144.23 and leaves the 176-tick field at 127.84, as §6.1 derived (R-3 did not occur).

| Geyser (`T`) | `k` = 1 derived / measured | 2 | 3 | 4 |
|---|---|---|---|---|
| Showcase near (10, 1), 99 | 60.61 / 60.61 | 121.22 / 121.21 | 166.67 / **166.67** | 166.67 / 166.67 |
| Showcase corner (1, 1), 135 | 44.44 / 44.44 | 88.88 / 88.89 | 133.32 / 133.33 | 166.67 / **166.67** |
| `default_arena` slot 0 (13, 12), 127 | 47.24 / 47.24 | 94.48 / 95.41 | 141.72 / 141.73 | 166.67 / **166.67** |
| `default_arena` slot 1 (50, 56), 99 | 60.61 / 60.61 | 121.22 / 121.21 | 166.67 / **166.67** | 166.67 / 166.67 |

- **A-1:** every line within 2.90 of its derivation (bound 20), every field within 2.01 (bound 2.5), every geyser
  within 0.93 (bound 2.0); every saturated node exact.
- **A-2:** no move in any window or warm-up.
- **A-3:** `harvest_handoff_tests::the_next_worker_starts_handoff_ticks_after_the_take` (both kinds, every hand-off in
  1,500 ticks) and `the_handoff_is_shared_by_every_player`.
- **A-4:** every `k` = 1 output file (lines, node mode and geysers) byte-identical to WP1's.
- **Direction:** no field's or geyser's count of takes in the window rose from WP1 to WP2 in any case. The unsaturated
  `k` = 2 lines and the arena slot-0 geyser at `k` = 2 move by phase only, within A-1: their take-period figures
  change by up to 0.54 a line (arena slot 1, 871.76 → 872.30, a period measured between a window's first and last
  take), while the loads drawn in the window fall or stay (870.0 → 867.5) and the loads banked stay (870.0).
- **A-5, the game's band** (stated in plan §6.3 before measuring): a field 144.23 in [138.7, 147.3]; a geyser 166.67
  in [158.1, 167.9]; the showcase line with three a field 1,153.84 in [1,105.8, 1,174.2]. All inside.

| | WP1 | WP2 | `StarCraft` II (Liquipedia, Resources) |
|---|---:|---:|---:|
| Three a field | 150.00 | 144.23 | 143 (theory 150.75) |
| Showcase line, three a field | 1,200.00 | 1,153.84 | about 1,140 |
| Near geyser, three | 171.43 | 166.67 | 163 (cap 169.61) |

### 3.3 Worlds that do not harvest

The matrix, battles 31 and 33 and both showcases on nine seeds (`world-wp2/`): 483 files, every one byte-identical to
`world-wp0/`, logs included once the output path is removed.

### 3.4 Client (release, radv, one target at a time under the gate lock; `client-wp0/`, `client-final/`)

The skirmish match was run on WP0's tree (an archive of `a5b7f281`, its own target directory) and on the final tree
(WP2 merged with `main` `d71f7153`); WP1's latent tree was not run separately.

| Target | WP0 | Final |
|---|---|---|
| `skirmish_match_tests` | 7 passed, 1 ignored by design | 7 passed, 1 ignored by design |
| `a_match_is_won_through_real_input_alone` | Victory at tick 6,486 | **Victory at tick 6,486** |

Every stage line is the same on both trees, at the same tick: Mine 0, Raise 22, Site 550, Foundation 586, Train 642,
Group 4,502, March 4,526, Command 4,822, Assault 4,834, Command 5,086, Assault 5,098, Command 5,350, Assault 5,362, Done
6,486; the windowed child's stages (Raise, Site, Resume, Foundation at 5, Done at 115) and the headless child's (Done at
111) too. The only difference in the captured output is the order in which the concurrently running tests' lines
interleave. So no stage moved, earlier or later (§7.2 expected identical or later).

On the final tree, `sync_steady_state_tests` (8), `battle_card_path_tests` (4), `battle_context_order_tests` (30),
`battle_fog_tests` (25), `battle_hud_tests` (35), `battle_flipbook_tests` (9) and the `client` library's
`battle_pipeline` tests pass.

## 4. Merge of `main` and schema 39

`main` took schema 38 for plan `2026-11-23-strike-delays` (`d71f7153`) before this branch handed back, so the merge
(`e028fc0b`) repeated §5 on the merged tree (`golden-merge-unhashed/`, `golden-merge-hashed/`):

| Step | Native debug | Native release | `wasm32-wasip1` release | `wasm32-wasip1` debug |
|---|---|---|---|---|
| `resource_lines` left out of the encoding, schema 38: main's recorded goldens and sandbox (`5_609_708_170_486_988_781`) | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 |
| Hashed, schema 39, re-recorded (sandbox `8_582_058_566_732_369_859`) | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 | 4 of 4; 3 of 3 |

`cargo test --release -p sim_core -p sim_fixtures` passes every target (99) on the merged tree.
