# Evidence: Footprint Placement Flags (B-139)

Measurements and test results of plan [`2026-11-19-footprint-flags`](../plans/2026-11-19-footprint-flags.md), one
section per work package, re-measured on the implementation tree against its base `41fa340d` (the plan's tree; its
code is `main` at `e969dc75`).

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside (charter §7). The tools read
the assets of the tree they were built in (`CARGO_MANIFEST_DIR` is compiled in), so the base tools read the base
maps and catalog:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

`~/scratch/footprint-flags/impl/scripts/measure.sh <bin> <tree root> <dir>` then runs, from the tree root:

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 6000 --out <dir>/route-default
for s in 1 2 3 4 5 6 7 8; do
  <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --ticks 6000 --out <dir>/route-s$s
done
```

and `scripts/compare.sh <base dir> <tree dir>` compares every output file byte for byte (logs without their
`wrote <dir>` line). Goldens: `scripts/goldens.sh` runs `cargo test -p sim_core --test golden_replay_tests` native
debug and `--release`, and `--target wasm32-wasip1` release (and debug for WP4) with
`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<tree real path>"`.

Sources: the 5.0.14 data were read as data from `~/scratch/footprint-flags/dl-footprint/` (`FootprintData.xml` of
`core`, `liberty`, `void` at `5444e12f13`) and `~/scratch/combat_wp3_data/*/UnitData.xml`. Each structure row's
placement footprint and its `UnitData` line are in its `structures.ron` comment; `scripts/fp.py` resolves a
footprint's flag sets with inheritance.

## WP1: vocabulary, data and loader

| Run | Outputs | Result |
|---|---:|---|
| Matrix, every battle, seeds of record and 16 (1,160 runs) | `matrix.csv`, `matrix.md`, logs | Byte-identical; 0 expectation failures on the seeds of record (4 over all seeds, as on the base) |
| Battle 33, 64 seeds | `seeds.csv`, log | Byte-identical |
| Showcase, default seed and seeds 1 to 8, 6,000 ticks | `units.csv`, `reversals.csv`, `passes.csv`, `overlap.csv`, `groups.md`, `summary.md` | Byte-identical on all nine seeds |
| All of the above | 205 files | 0 differ |
| `golden_replay_tests` | native debug, release, `wasm32-wasip1` release | Pass, unmoved |

Tests: the loader's validation and the shipped rings (`catalog::validation::placement_flag_tests`, six tests), the
reference flag sets (`catalog_reference_values_tests::the_shipped_flag_sets`) and the run-time read
(`runtime_catalog_tests::an_edited_placement_rule_changes_the_loaded_reach`).

## WP2: the rule

Measured exactly as WP1: 205 of 205 outputs byte-identical to the base, goldens pass on the three targets. All 81
`sim_core` test targets pass.

| Test change | Trace |
|---|---|
| `player_view_tests::can_raise_matches_admission_for_every_catalog_pair` | The one failure the prototype predicted (plan §6.1): the shared site (100, 2) is out of every unit's sight, so the two crawler rows were refused `FlaggedSite { flag: Fogged, cell: (100, 2) }`. A sighted unit now stands beside the site and vision is measured once before the orders |

New: `footprint_flag_tests.rs` (F-1 to F-8, F-10 guard) and `player_view_tests::the_preview_reports_near_resources`
(F-9). F-6 judges 338 town hall sites (13 × 13 around a mineral and a gas node): 64 admitted, 224 refused
`NearResources`, 50 `CoversResource`; preview and admission agree on every one, every admitted order is built on
arrival, and a node moved beside an admitted site ends the order on arrival with `FlaggedSite`.

## WP3: client

`battle_card_tests` 29 of 29 (with `a_town_hall_near_resources_shows_refused_and_says_why`), the status tests 14 of
14. No simulation file changed; no GPU capture (the ghost's colours and geometry are unchanged, plan §8).

## WP4: maps

`every_shipped_start_clears_every_resource_reach` was red on WP3 (`default_arena` slot 1: hall cell (45, 53) in the
ring of the gas node at (48, 56)) and passes after D-8's five moves; `every_shipped_gas_node_takes_an_extractor`
passes before and after.

WP4 first stopped on §7 rule 4 (trace: [`2026-11-19-footprint-flags-wp4-stop.md`](2026-11-19-footprint-flags-wp4-stop.md)).
Amendment 1 fixed the three tests that name or digest the moved cells in WP4 and added a re-measurement on the merged
tree against `main`'s own run, and the three GPU targets that read the shipped maps.

| Run | Result |
|---|---|
| Before the merge: matrix, battle 33, showcase nine seeds against `41fa340d` | 205 of 205 outputs byte-identical |
| After the merge (`main` `bf4252cf`, with the funnel-fallbacks changes): the same against `main`'s own tools and maps | 205 of 205 outputs byte-identical: no winner, survivor count or `units.csv` row moved, so no placebo build was needed (§7 rule 3) |
| `golden_replay`, `golden_replay_orders` with the moved map | Diverge at checkpoint 0 (node positions are hashed) |
| Scratch build hashing each moved node at its old cell (`FF_OLD_RES`, diff `~/scratch/footprint-flags/impl/proof/proof.diff`) | Both fixtures reproduce every recorded checksum |
| Scratch build without the resources section (`FF_SKIP_RES`), per-tick checksum, old map against moved map | Identical on all 3,280 logged ticks of the four golden tests (`ticks-old-map.txt`, `ticks-moved-map.txt`) |
| Re-recorded goldens, on the merged tree (`main` changed neither fixture) | Pass native debug and release, `wasm32-wasip1` release and debug |
| `sim_map/validation_golden_tests`, previous map files on the merged tree | Reproduce the recorded digests `0x3527_8c7f_a705_ba2c` and `0x4c32_25d2_ef3a_fc87` |
| The same with the moved maps | Validation `0x78f4_a8ea_2deb_b63b`, decode `0x10db_ea43_6aba_f292`; the variant set and counts (13 and 14 together in 15 reports, unreachable in 1, 2,496 warnings) and the decode failure set unchanged; the two path digests unchanged. Re-recorded; pass native debug and release, `wasm32-wasip1` release and debug |
| `full_match_tests::stim_is_locked_until_researched_through_the_whole_tech_path` | Stalled at gas 0 (it looked the slot-0 gas node up at (14, 6)); with the site `(14, 5)` and the cell `(15, 6)` it passes |
| `battle_card_tests::a_town_hall_on_its_own_start_shows_legal` | With the starting hall removed, the town hall ghost on the player's own start is green |
| GPU targets `runtime_battle_tests`, `editor_app_tests`, `terrain_tests` | Pass through the harness of `gpu_run.rs` (its default driver, a GPU lock per child): `editor_app_tests` 40, `runtime_battle_tests` 18 (1 child ignored, launched by its parent), `terrain_tests` 14 |

The showcase's timeline did not move, so the storyboard (`assets/captures/showcase.ron`) was not re-checked.
