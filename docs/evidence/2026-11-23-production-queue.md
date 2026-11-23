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

### 1.3 Skirmish match (release, radv; `impl/client-wp1/`)

`skirmish_match_tests` 7 passed, 1 ignored by design (the windowed child, launched by its parent): Victory, every stage
on the base's tick (Mine 0, Raise 22, Site 550, Foundation 586, Train 642, Group 4,502, March 4,526, assaults 4,834,
5,098, 5,362, Done 6,486).

## 2. WP2: supply at the start (plan D-1, D-2; B-59; §8 rule 2)

Only the job in production holds supply. A job admitted to an empty queue starts at once and takes its supply with
its price (refused at the cap, as before); one admitted behind others pays minerals and gas, holds 0 and is not
checked against supply. At the front, phase 2 starts a job whose supply fits (`EntityId` order); otherwise it waits,
its countdown not started, and its producer is listed as `ProductionBlock::SupplyWait`, which the player view
publishes as `StructureQueueView::supply_blocked`. The queue invariants add "a job that has not started holds no
supply", and `validate_unit_job` checks the front's supply against `in_production`.

| Check (`impl/verify-wp2/`) | Result |
|---|---|
| New tests (`production_supply_tests`: `queued_jobs_hold_no_supply`, `the_next_job_takes_its_supply_when_it_starts`, `a_front_job_waits_for_supply_and_starts_when_it_fits`, `admission_to_an_idle_producer_needs_supply_and_behind_a_job_does_not`, `cancelling_a_waiting_job_returns_only_its_price`, `cancelling_the_running_job_releases_its_supply_and_the_next_starts`, `two_producers_waiting_for_the_last_supply_start_in_entity_order`, `supply_lost_during_production_does_not_stop_the_running_job`, `jobs_complete_on_the_same_ticks_when_supply_never_binds`, `corrupt_queue_states_are_invariant_failures`; `player_view_tests::only_the_structure_waiting_for_supply_is_marked`) | red on WP1 (they name `SupplyWait` and `supply_blocked`), green on WP2 |
| P-2 on WP1's tree | `jobs_complete_on_the_same_ticks_when_supply_never_binds`, built alone against WP1's sources, passes with the same expected ticks (`k × build_time` for the k-th job of each of two queues of five): the completion ticks are WP1's |
| Re-derived from D-2 | `production_transactions_tests::selection_order_and_reservations_are_exact` (20 admissions behind a running job are accepted and hold no supply; the supply refusal is now that of an idle producer at the cap); `research_and_cancel_tests::cancelling_training_refunds_payment_and_supply` (two admissions hold one product's supply); `production_supply_tests::the_first_job_is_in_production_and_the_rest_wait_their_turn` (one product's supply, not three) |
| Unchanged | `production_timer_tests` (4), `production_transactions_tests` otherwise, `replay_keeps_transaction_and_supply_state_identical`; no other test moved |
| `cargo test --release -p sim_types -p sim_core -p sim_fixtures` | every target passes (101) |
| Goldens | 4 of 4 on native debug and release, `wasm32-wasip1` release and debug, unchanged |
| Worlds (`world-wp2/`) | 420 files byte-identical to `world-wp0/`, logs included |
| Clippy, fmt, `workspace_checks` | clean |
| Skirmish match (release, radv; `impl/client-wp2/skirmish.log`) | 7 passed, 1 ignored by design; every stage on the base's tick, Done 6,486 |

## 3. WP3: the rally command and the new unit's order (plan D-3 to D-5; B-60; §8 rule 3)

`UnitOrder::SetRally { target }` (appended last; encoded with tag 18 in order queues, though it never enters one) is
admitted by the economy dispatcher (`admit_set_rally`) for every recipient that is alive, completed, owned, unloaded
and trains units; a rally on the recipient itself clears it; refusals are `NoCapableProducer`, `RallyOutsideMap`,
`InvalidRallyTarget` (absent, dead or foreign unit) and `UnknownResourceNode`. `SimWorld::rally_point` resolves a
rally; after `commit_spawn` the child takes its rally's order through the ordinary admission (`Move`, `Harvest`,
`Load`, or a `Move` to the resolved point), a dead target clears the rally and sends no order, a carried one sends
none. `PlayerView::rallies` lists the viewer's own resolved rallies as `RallyView { entity, point: [f32; 2] }`.

| Check (`impl/verify-wp3/`, `impl/client-wp3/`) | Result |
|---|---|
| New tests, red on WP2 (they name `SetRally`, `rally_point` and `rallies`) | `rally_tests`: `set_rally_is_admitted_only_for_producers`, `a_rally_on_the_producer_itself_clears_it`, `a_unit_moves_to_a_point_rally`, `a_worker_rallied_to_a_mineral_field_harvests_it`, `a_soldier_rallied_to_a_field_walks_to_it`, `a_worker_rallied_to_a_geyser_without_an_extractor_walks_to_it`, `a_unit_rallied_to_a_friend_goes_where_the_friend_stood`, `a_dead_rally_target_sends_no_order_and_clears_the_rally`, `a_unit_rallied_to_its_carrier_boards_it`, `a_town_halls_auto_rally_sends_its_workers_to_harvest`, `a_world_without_a_rally_is_unchanged`; `player_view_tests::the_view_lists_only_the_viewers_own_resolved_rallies` |
| A `Move` to a node | movement admission moves a target inside the node's footprint to the nearest point the child can stand on (S-4): (25.5, 5.5) for the field whose centre is (25, 6.5) in the first draft of the test; the tests assert a `Move` within the footprint's half-diagonal plus one of the centre, not the centre itself |
| `cargo test --release -p sim_types -p sim_core -p sim_fixtures` | every simulation target passes; `sim_types`' own unit test fixture (`populated()`) lacked the new field on the first run and was completed |
| Goldens | 4 of 4 on native debug and release, `wasm32-wasip1` release and debug, unchanged |
| Worlds (`world-wp3/`) | 420 files byte-identical to `world-wp0/`, logs included |
| Skirmish match | 7 passed, 1 ignored by design; every stage on the base's tick, Done 6,486 |
| Client, release, radv (`impl/client-wp3/`) | `sync_steady_state_tests` 8, `battle_hud_tests` 34, `battle_context_order_tests` 24, `battle_card_path_tests` 4, `battle_fog_tests` 25, `battle_flipbook_tests` 9 pass. `client --lib battle_` first failed to compile: `status/tests.rs`' exhaustive table of `GameCommandError` lacked the three rally refusals; added (slots 22 to 24), then 102 pass |
| Client arms needed to compile | `capable` and `order_subject` arms for `SetRally` and the three refusal messages of `write_game_reason`, which plan §10 gives to WP5, land here because the new variants make the client's exhaustive matches fail to compile (a deviation of file ownership only; WP5 adds the input, display and tests) |
| `PlayerView` | loses `Eq` (keeps `PartialEq`): `RallyView::point` is `[f32; 2]`, presentation output (plan D-10; the plan's `Vec2` is a Bevy type `sim_types` cannot name). C7 gains `crates/sim_types/src/player_view.rs`, 1 float token |
