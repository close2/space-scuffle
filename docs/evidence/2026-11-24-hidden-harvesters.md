# Evidence: gas gatherers hidden inside the extractor (B-161)

Plan: [`docs/plans/2026-11-24-hidden-harvesters.md`](../plans/2026-11-24-hidden-harvesters.md). Measured on the package
branch `hidden-harvesters-plan` on top of `main` `e313268f` (plan commits `ee029d65`, `c0d679d8`), 2026-11-24, on the
shared VM. Scratch material lives under `~/scratch/hidden-harvesters/` (`scripts/`, `tool/`, `bin-<wp>/`,
`tree-<wp>/`, `world-<wp>/`, `income/`, `red/`, `golden-<label>/`, `client-<label>/`).

## 1. WP0: sources and baseline

### 1.1 The counts of plan §2.1

Read from `~/scratch/combat_wp3_data/<layer>/` (SC2GameData `5444e12f13`, all eight layers):

```bash
cat */WeaponData.xml | grep '<TargetFilters' | grep -c Hidden     # 144
cat */WeaponData.xml | grep -c '<TargetFilters'                   # 146
cat */EffectData.xml | grep 'SearchFilters' | grep -c Hidden      # 199
cat */EffectData.xml | grep -c 'SearchFilters'                    # 247
grep -n HideHarvesters */BehaviorData.xml
```

The last command lists `liberty` BehaviorData:97, :109, :121 (rich geysers), :824, :836, :848
(`HarvestableVespeneGeyserGas`, `…Protoss`, `…Zerg`) and `swarm` BehaviorData:857, :868, :879 (terrazine), each
`<Flags index="HideHarvesters" value="1"/>`; no later layer clears it. The Refinery, Assimilator and Extractor list
the three behaviours in their `BehaviorArray` at `liberty` UnitData:5806, :4151 and :8179. All match S1 to S5.

The TL.net "SC2 Mythbusters" page (§16 of the plan) was not fetched again; no default depends on it.

### 1.2 P-0: no measured world harvests

`grep` over the tree at `c0d679d8`:

| Where | `Harvest` or `ReturnCargo` |
|---|---|
| `assets/scenarios/` | none; `InitialOrder` variants used: `AttackMoveToStart` (3), `AttackMoveToCell` (3), `EnterModes` (2) |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron` | 0 lines each; their headers say "Nothing in this replay harvests" |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | 0 |
| `crates/sim_fixtures/src/` | 0 |
| `crates/sim_core/benches/` | 0 |

The scratch probe (`scripts/probe.sh`) patches an archive of `c0d679d8` so that `SimWorld::step_into` prints one line
when a world starts and one line on every tick on which any unit holds cargo or has a front order `Harvest` or
`ReturnCargo`, builds `matchup_trace` and `route_trace` from it and runs `measure.sh` (every world of plan §7):

| Lines | Count |
|---|---:|
| `P0PROBE start` (worlds played) | 1,390 |
| `P0PROBE tick … harvesting …` | **0** |

So nothing harvests in any measured world, and plan §7's predictions stand. The probe's world files are
byte-identical to `world-wp0` (`compare.sh`: 483 files, the 21 differing ones are the logs that carry the probe's own
lines).

### 1.3 Worlds (`world-wp0/`)

`scripts/wp.sh <wp> <commit> <housed-expr>` exports the commit to `tree-<wp>/`, adds the scratch income tool to
`crates/sim_core/examples/` of that export only, builds `matchup_trace`, `route_trace` and `harvest_occ` in release in
a separate target directory, runs `measure.sh` (the attack-orders script; `route_trace` gets `--assets <tree>/assets`)
into `world-<wp>/`, compares it with `world-wp0/` and runs `income.sh`. The matrix (every battle, seeds of record and
16: 1,200 runs, 0 expectation failures on the seeds of record, 4 over all seeds), battle 31 on 100 seeds, battle 33 on
64, the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks: 483 files, the
reference for every later comparison.

### 1.4 Income (`income/wp0-*`)

The handoff plan's tool (`tool/harvest_occ.rs`) with two probes added for this plan: every take of every node is
printed as `take,<node>,<tick>,<amount>`, and every trip of a worker (from one take to the next) as
`housed_trip,<kind>,<tick ends housed>,<trips>`. `HOUSED_PROBE` is replaced per tree: `false` before WP2 (no state
exists), `e.is_housed()` from WP2 on. The cases are the handoff plan's §6.1 (`income.sh`): mineral lines `k` = 1 to 4
on showcase slots 0 and 1 and `default_arena` slot 1, mineral nodes `k` = 1, 2 on showcase slot 0, and gas nodes
`k` = 1 to 4 on showcase slot 0 and `default_arena` slots 0 and 1. WP0: 26 files, 5,661 takes; every trip housed 0
tick ends.

## 2. WP1: the capability as data

### 2.1 Red run (`red/wp1-red.log`)

On the tree with `ArchetypeCapabilities::hides_harvesters` added (every row `false`) and the error variant declared,
before the RON field and the validation:

| Test | Result |
|---|---|
| `catalog_reference_values_tests::b161_structures_hide_harvesters_as_starcraft` | FAILED: `plasma_siphon_dome (Refinery): hides_harvesters[0] is 0, StarCraft II 1` |
| `runtime_catalog_tests::hiding_harvesters_is_required_and_refused_without_an_extractor` | FAILED (no row authors the field) |
| `runtime_catalog_tests::hiding_harvesters_is_read_at_run_time` | FAILED (no row authors the field) |

All three pass after D-1 (`red/wp1-green.log`), with the unit test
`catalog::validation::tests::only_a_gas_extractor_hides_its_harvesters`.

### 2.2 Worlds and income (`world-wp1/`, `income/wp1-*`)

Measured on `2fe30af0`: `compare.sh world-wp0 world-wp1`: 483 files, 0 differing. Every income file is
byte-identical to WP0's. The commit was then amended to `e659fe6b` with one attribute only
(`#[expect(clippy::struct_excessive_bools)]` on `ArchetypeCapabilities`, which the fourth bool trips); no code
changed.

## 3. WP2: the hidden state in the simulation

### 3.1 Red run (`red/wp2-red.log`, `red/wp2-red-hht.log`)

On WP1's tree plus the inert state (`UnitTask::housed_in`, never set and left unhashed, the predicates, and the
`HousedRecipient` variant), 12 of the 13 tests of `hidden_harvester_tests` fail and
`checksum_state_tests::every_public_entity_field_changes_the_world_checksum` fails on the `housed_in` mutation:

| Test (P) | Failure on the red tree |
|---|---|
| `a_gas_gatherer_is_inside_for_exactly_its_gather` (P-2) | inside on `[]`, expected ticks 41 to 75 |
| `inside_is_the_gas_gather_and_never_a_mineral_one` (P-2) | gathering but not housed |
| `an_enemy_acquires_the_control_and_never_the_housed_worker` (P-3) | the worker is not housed |
| `an_attack_on_a_worker_is_dropped_when_it_goes_inside` (P-3) | the attack holds after the grant |
| `splash_on_the_control_spares_the_housed_worker` (P-3) | the worker is not housed |
| `an_area_spell_finds_the_control_and_not_the_housed_worker` (P-3) | the storm took 10 from the worker (45 to 35) |
| `a_missile_at_a_worker_that_goes_inside_lands_on_the_point` (P-3) | the missile struck the worker (45 to 29) |
| `the_housed_worker_acquires_nothing` (P-3) | the worker is not housed |
| `a_housed_worker_sees_nothing` (P-4) | the worker is not housed |
| `no_order_reaches_a_housed_worker` (P-5) | no `HousedRecipient` refusal |
| `a_worker_survives_its_extractors_death_and_comes_out_where_it_went_in` (P-6, P-7) | not inside before the death |
| `the_housing_invariant_holds_over_a_long_run` (P-9) | tick 40: a gas gatherer gathers outside |
| `a_queued_order_waits_for_the_delivery` (P-5, other half) | ok (pins today's behaviour, a guard) |

P-7's position check sits in the P-6 test after the first take and held on the red tree too. All pass after the
change (`red/wp2-green.log`), with `runtime_catalog_tests::an_extractor_edited_not_to_hide_leaves_its_gatherer_seen`
(P-1's behaviour half) and P-8's mutation. `cargo test -p sim_core --release` on the unhashed tree: 1,328 passed, 1
failed (the P-8 mutation, expected before hashing); the matrix and full-match targets included.

### 3.2 Readers left on `is_loaded_in_transport` (R-1)

After WP2, `grep -rn is_loaded_in_transport crates --include=*.rs` outside tests lists: `entity.rs`
`takes_part_in_movement` (WP5) and `issue_order` (admission; housed units are refused at dispatch instead, §3.4),
`economy/mod.rs` `validate_selection` and `update_unit_tasks` (the gather must keep stepping), `player_view.rs` own
entities, `tech_tree.rs` (two), `production/mod.rs` (two) and `lifecycle.rs` (ownership, producers),
`production/rally.rs` (three, rallies), `transport/*` (carrier topology), `world/snapshot.rs` (cargo stays out of the
snapshot), `movement/mod.rs`, `movement/groups.rs`, `movement/flee.rs`, `steering.rs` (WP5 or orders), the client's
roster and `can_receive_order` (which also refuses housed units in WP3), `client_diagnostics` capture filters and
`sim_fixtures` traces (nothing traced harvests). Each is §3.3's classification.

### 3.3 Golden proof (`golden-wp2-unhashed/`, `golden-wp2-hashed/`)

`scripts/goldens.sh <label>` copies the worktree's files to `tree-<label>/` and runs `golden_replay_tests` and
`sim_fixtures`' `fixture_tests` on native debug and release and on `wasm32-wasip1` release and debug
(`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).

1. **Unhashed at schema 41** (the WP2 change complete, `housed_in` listed after `unhashed` in the encoding): all 8
   runs pass, so `golden_replay.ron` (20 checkpoints), `golden_replay_orders.ron` (60) and
   `DEFAULT_SANDBOX_CHECKSUM` `1_550_645_596_559_574_539` reproduce as recorded for plan
   `2026-11-23-vision-and-detection` WP4.
2. **Hashed at schema 44**: both fixtures re-recorded from the native release run with the header of plan §5, and
   the sandbox constant becomes `13_527_726_331_326_150_869`; all 8 runs pass with the new values.

### 3.4 Worlds and income (`world-wp2/`, `income/wp2-*`)

Measured on `ba096000` with the housed probe on (`e.is_housed()`), and measured again with every exported file touched
(the first run's export kept its commit-time file dates, so artifacts of an earlier tree in the shared target
directory could have been reused; `wp.sh` now touches the export; the second run gives the same results): `compare.sh world-wp0 world-wp2`: 483 files, 0
differing. Every income file equals WP0's line for line once the `housed_trip` lines are set aside: the same 5,661
takes on the same ticks with the same amounts, so P-11 holds. The probe: every one of 917 gas trips spent **35** tick
ends inside, and every one of 4,623 mineral trips 0. A whole-workspace `cargo check --all-targets` of `ba096000`
passes (the client's status message for `HousedRecipient` included).

### 3.5 The orders rule (Q0108's filed default)

The plan's D-8 let an order reach a housed worker and bring it out; the filed Q0108 default (a) says the player
cannot order it while it is inside. Built as filed: `dispatch_command` drops housed recipients from every command,
immediate or queued, and refuses a command left with none (`GameCommandError::HousedRecipient`). P-5 tests that rule
(`no_order_reaches_a_housed_worker`); the plan's As built records the decision.

## 4. WP3: the snapshot and the client

### 4.1 Red run (`red/wp3-red.log`)

On WP2's tree plus `BubbleRenderInstance::housed` (never set) and the client's gate:
`battle_fog_tests::a_worker_inside_its_extractor_is_not_drawn_picked_selected_or_ordered` FAILED ("entity 1 of
EntityId(2) would be drawn"); `sync_steady_state_tests::a_gas_trip_in_and_out_of_the_extractor_spawns_nothing` passed
(nothing hid the worker, so nothing changed). The unit tests of the gate (`a_housed_unit_is_hidden_with_fog_without_fog_and_from_its_owner`,
`leave_removes_a_unit_from_the_selection_and_keeps_its_groups`) were written with the gate and were green from the
start, and `the_snapshot_publishes_a_housed_worker_as_housed` was added with the publisher.

With the snapshot publishing the flag (`red/wp3-green.log`) the fog test passes and the steady-state test first
failed: 24 bubble materials modified against 23 redrawn (unit, frame) pairs. A per-frame trace showed the extra write
on the frame the worker goes inside: `effects::stance` writes the shell once when presence changes, as for a unit
that cloaks. The test now allows one write per presence change (two a trip), and passes; entities, render entity and
retained buffers are unchanged over the trip.

### 4.2 Client targets (`client-wp3/summary.txt`, release, radv)

| Target | Result |
|---|---|
| `battle_card_path_tests` | 4 passed (the gas test included) |
| `battle_input_tests` | 38 passed |
| `battle_hud_tests` | 35 passed |
| `battle_fog_tests` | 28 passed |
| `sync_steady_state_tests` | 9 passed |
| `sync_allocation_count_tests` (valgrind) | 10 passed; the running children carry a gas crew of three that goes inside and comes out during the window, and the live and baseline differences still agree |
| `skirmish_match_tests` | 7 passed, 1 ignored |
| `client --lib` | 542 passed |

The skirmish stage log (`client-wp3/stages.txt`) holds the same 42 lines as the WP0 tree's (`client-wp0/stages.txt`,
built from `tree-wp0`); only the order of three lines printed by concurrent test threads differs
(`diff <(sort …) <(sort …)` is empty).

## 5. The merge with `main` `7dc356fc` (weapon validation, schema 43)

- **Conflicts:** the schema constant and its schema-byte tests, both goldens and the sandbox constant, the bug
  register (B-194, B-195 before B-196), the plans README, a comment of `track_projectile_target`, and the tail of
  `runtime_catalog_tests.rs`. `passes_weapon_filters` (main) is `is_entity_targetable && sees_enemy`, and
  `is_entity_targetable` starts from `is_damageable`, which reads `is_hidden`, so every reader main switched honours
  a housed target. The missile that does not validate its weapon filters follows a target unless it is cargo; a
  housed target keeps its position and takes no damage at impact, so it stays on `is_loaded_in_transport`.
- **Golden proof on the merged tree** (`golden-merge-unhashed/`, `golden-merge-hashed/`): with `housed_in` unhashed at
  schema 43, `golden_replay`, `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` `17_697_741_365_715_682_877`
  reproduce main's values on native debug and release and `wasm32-wasip1` release and debug (8 of 8). Hashed at
  schema 44, both goldens re-recorded under one header (main's fixtures carry no earlier header of this plan) and the
  sandbox becomes `8_410_876_910_713_629_694`; 8 of 8 reproduce them.
- **Tests:** `cargo test -p sim_core --release` on the merged, unhashed tree: 1,359 passed, 1 failed (the P-8
  mutation, before hashing); hashed, `checksum_state_tests`, `hidden_harvester_tests` and `runtime_catalog_tests`
  pass. The client targets of §4.2 on the merged tree (`client-final/summary.txt`): all pass with the same counts,
  and the skirmish stage log has the same 42 lines as WP0's.
- **Worlds** (`world-main/` of `7dc356fc` against `world-merge/` of `18999b6f`, both exports touched): 483 files, 0
  differing; the matrix 1,220 runs, 0 expectation failures on the seeds of record, 4 over all seeds, on both.
- **Income** (`income/main-*`, `income/merge-*`): the same 5,661 takes on the same ticks; the housed probe gives 35
  tick ends inside for each of 917 gas trips and 0 for each of 4,623 mineral trips.

## 6. WP5: the movement phase reads `is_hidden` (Amendment 1)

Branch `hidden-harvesters-wp5` from `main` `708576ac` (the contact solver shipped, `Legacy` selectable). Scratch
outputs under `~/scratch/harvesters-wp5/`; the scripts are this evidence's §7 scripts with the worktree, target and
output paths changed (`scripts/`).

- **Red first** (`red/red-tests.log`, `red/red-legacy-first.log`, on the tree with the tests and no fix): the three
  new tests fail. `a_housed_worker_is_no_body_in_either_pipeline` fails under `Solver` (and, run with `Legacy`
  first, under `Legacy`): with `is_mineral_walking` cleared by hand, the housed worker pushes the idle friend standing
  in it and is pushed by the walker. `a_housed_worker_is_no_displacement_obstacle` returns `Occupied(housed)`.
  `a_housed_worker_is_no_push_partner` moves both units.
- **Green:** `hidden_harvester_tests`, 17 of 17.
- **Worlds** (`world-main/` of `708576ac` against `world-wp5/` of `78d68c64`, both exports touched, release): 483
  files, 0 differing. The matrix's 1,220 runs keep 2 expectation failures on the seeds of record and 25 over all
  seeds on both trees.
- **Income (P-11)** (`income/main-*`, `income/wp5-*`): 26 files byte-identical: the same 5,661 takes on the same
  ticks. The housed probe gives 35 tick ends inside for each of 917 gas trips and 0 for each of 4,623 mineral trips.
- **Goldens** (`golden-wp5/summary.txt`, on the exported WP5 tree, no re-recording): `golden_replay_tests` on native
  release and debug and `wasm32-wasip1` release and debug, and `sim_fixtures`' `fixture_tests` (the sandbox
  checksum) on native release and debug, all pass. No fixture file changed.
