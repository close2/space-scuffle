# Evidence: supply at the start of production, rally points and placed exits (B-59, B-60, B-66)

Plan: [`docs/plans/2026-11-23-production-queue.md`](../plans/2026-11-23-production-queue.md). Measured on the
package branch `production-plan` on top of `main` `4af9f51f`, 2026-11-23, on the shared VM. WP0 is `8807620e`.
Scratch material lives under `~/scratch/production-queue/` (`bin-wp0/` and `tree-wp0/` for the base,
`world-wp<N>/` for each package's world comparison, `impl/` for the golden and client logs, `scripts/` for the
commands).

## 0. The base and the comparison

- **Worlds.** The commands of `~/scratch/resource-footprints/scripts/measure.sh` (the matrix of every battle on its
  seeds of record and 16 seeds, battle 31 on 100 seeds, battle 33 on 64 seeds, the showcase on its default seed and
  seeds 1 to 8 for 6,000 ticks), with `route_trace` given `--assets <tree>/assets` explicitly
  (`scripts/measure.sh`): its default is the assets directory of the tree it was compiled in, so the base reads the
  WP0 assets exported to `tree-wp0/` and each package reads its own tree's. The base, `world-wp0/`, built from WP0's
  tree, has 420 files and empty error logs.
- **Comparison** (`scripts/compare.sh`): every output file byte for byte; the logs after replacing the output path.
- **Skirmish match.** The base is the stage log of `main` `4af9f51f` (the harvest-occupancy plan's WP3 tree, whose
  code `main` carries unchanged): Victory at tick 6,486; Mine 0, Raise 22, Site 550, Foundation 586, Train 642,
  Group 4,502, March 4,526, assaults 4,834, 5,098, 5,362 (evidence `2026-11-23-harvest-occupancy` §4).

## 1. WP1: the new state, the auto-rally and the schema (plan D-1, D-3, D-6, D-7; §8 rule 1)

`ProductionJob::in_production` is maintained (set at admission to an empty queue and when the front job first counts
down; supply still reserved for every job), `Entity::rally` and `sim_types::RallyTarget` exist, the seven town hall
rows author `auto_rally: Some((radius: 6.5, kinds: [Mineral]))` and the hall sets its rally at `spawn_structure` and
`place_foundation`.

### 1.1 The D-6 proof

| Step | Result (`impl/golden-proof-wp1/`, `impl/golden-wp1/`) |
|---|---|
| WP1's tree with both fields listed `unhashed` in the encoding and the schema at 33 | `golden_replay_tests` 4 of 4 on native debug and release and `wasm32-wasip1` release and debug (runner `wasmtime --dir=/home/spacescuffle/space-scuffle/.claude/worktrees/production-plan`, the real path): every recorded checkpoint of both goldens reproduced, with the halls' rallies set |
| The same build, `sim_fixtures`' `fixture_tests` | 3 passed: `DEFAULT_SANDBOX_CHECKSUM` `3_827_951_643_646_962_361` reproduced (the sandbox's halls rally too) |
| Both fields hashed (`in_production` after `reserved_supply`, `rally` after `captured_supply_provided`), schema 34 | every checkpoint changed (20 of `golden_replay`, 60 of `golden_replay_orders`); the new lists recorded with the plan's header, and 4 of 4 on all four builds agree on them; the sandbox checksum is `16_199_664_925_193_234_964` |

`main` was at schema 33 when WP1 was written, so WP1 takes 34 (plan D-6: the next free number at merge).

### 1.2 Worlds and tests

| Check | Result |
|---|---|
| Matrix, battles 31 and 33, the showcase on nine seeds (`world-wp1/`, binaries `bin-wp1/`) | 420 files, every output file and every log (path replaced) byte-identical to `world-wp0/` |
| `cargo test --release -p sim_types -p sim_core -p sim_fixtures` (`impl/tests-wp1.log`) | every target passes (101) |
| New, red on `main` | `checksum_state_tests::every_production_job_field_changes_the_world_checksum` and the three `rally` mutations of `every_public_entity_field_changes_the_world_checksum` (red with the fields unhashed, the proof build); `checksum::encoding::tests::rally_targets_encode_their_tag_and_payload`; `production_supply_tests::the_first_job_is_in_production_and_the_rest_wait_their_turn`; `rally_tests::a_town_hall_rallies_to_the_farthest_mineral_field_in_reach` and `a_hall_without_a_field_in_reach_has_no_rally_and_a_geyser_is_never_chosen`; `runtime_catalog_tests::an_edited_auto_rally_changes_the_halls_rally`; `catalog::validation::tests::an_auto_rally_is_refused_unless_its_radius_kinds_and_owner_are_valid`; `catalog_reference_values_tests::the_shipped_town_halls_auto_rally_as_5_0_14` |
| Clippy (`-p sim_types -p sim_core -p sim_fixtures -p client --all-targets -D warnings`), fmt, `workspace_checks` | clean; C7's count for `catalog.rs` 71 → 72 (the authored `auto_rally.radius`, an authoring boundary) |

The showcase's two halls rally to the node the test computes from the map; by hand (plan D-7) the slot-0 hall at
(6, 6) chooses the field anchored at (13, 13), whose centre (14, 13.5) is 7.43 from the hall's (8.5, 8.5).
