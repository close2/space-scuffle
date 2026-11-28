# Evidence: the remaining rally rows and the per-unit smart command (B-163, B-167, B-214, B-215, B-217, B-222)

Plan: [`docs/plans/2026-12-01-rallies-and-smart-command.md`](../plans/2026-12-01-rallies-and-smart-command.md).
Measured on the package branch `rallies`, cut from `main` `b2ba4621`, from 2026-11-27, on the shared VM. Scratch
material lives under `~/scratch/rallies/smart/` (`scripts/`, `bin-<wp>/`, `tree-<wp>/`, `world-<wp>/`,
`client-<label>/`, `red/`, `valgrind-<label>/`); the scripts are adapted from the follow-order plan's
(`~/scratch/follow-order/scripts/`).

## 1. WP0: records, probe and baseline

### 1.1 Records

- **Numbers.** `main` ends its register at B-245. The open branches claim B-246 and B-250 (`diagonal-cliffs`), and
  the minimap plan proposes B-248 and B-249 without filing them. B-247 occurs on `main` and on every open branch only
  in this plan's own text, so the plan's provisional B-247 stands and nothing is renumbered. Q0140 and Q0141 were
  already filed on `main` (`0140-unit-exit-placement.md`, `0141-rally-waypoint-cap.md`), so they keep their
  numbers.
- `DESIGN.md` §4: B-163's status is **fixed** (follow order WP3 `0a797bab` and WP4 `27604c11`), with "open;" and
  "stays open under …" struck; B-214 is removed (sanctioned below); B-247 (a transport's right click does not pick a
  unit up, deferred to WP-D1) is registered; B-167, B-215, B-217 and B-222 point to the plan.
- Q0052 gains the addendum **S-rally-keys** (D-5), from Q0115 (a).

### 1.2 Worlds (`world-wp0/`)

`scripts/wp.sh wp0 b2ba4621` exports the commit (WP0 changes only documents, so its code is `b2ba4621`'s) to
`tree-wp0/`, builds `matchup_trace` and `route_trace` in release in a separate target directory
(`/home/spacescuffle/cargo-target/rallies-measure`) and runs `scripts/measure.sh` (the follow-order plan's: the matrix
with every battle on its seeds of record and 16 seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase and
showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks): 483 files, the reference for every
later comparison by `scripts/compare.sh` (every output file byte for byte; logs after removing the output path). The
matrix log reads "1220 runs: expectation failures 2 on the seeds of record, 26 over all seeds", as on `main`.

### 1.3 P-0: no skirmish right click is resolved differently per member

`scripts/probe.sh b2ba4621` exports the commit twice. It runs the client skirmish match
(`skirmish_match_tests::a_match_is_won_through_real_input_alone`, release) on the unpatched copy (`client-wp0/`), and
on a copy patched by `scripts/patch_probe.py` (`client-probe/`, never committed). The patch adds one line to
`issue_pointer_order`, after today's `context_order` and rally choice, that compares, member by member, the order
today's single-order rule gives (the members `any_recipient` accepts for it, when the unit submission happens at all)
with the order D-1's per-member table would give (the first row each member takes, by the same member rule), and
prints whether they differ. Today's behaviour is unchanged.

| Lines | Count |
|---|---:|
| `P0PROBE click` (right clicks) | 8 |
| `differs=true` | **0** |

The eight clicks are the pilot's six workers sent to minerals at tick 0, one at tick 594, and the army's six units
attacking an enemy at tick 5,542: selections of workers alone or of the army alone, as plan §6 expected. P-0 is
zero, so stop rule 1 does not apply and §7's skirmish row stands.

### 1.4 The skirmish match (`client-wp0/`, `client-probe/`)

On WP0's tree the match is won at tick 6,342 (1,645 frames), the pilot's stages at ticks 0, 0, 0, 22, 550, 586, 642,
4,502, 4,526, 4,822, 4,834, 5,086, 5,098, 5,350, 5,362 and 6,334, the figures of the follow-order evidence. The
probe's tree gives the identical stage log.

## 2. WP1: the per-unit smart command (`b8547023`)

### 2.1 Red runs (`red/wp1-unit.log`, `red/wp1-red/`, `red/wp1-red2/`, `red/wp1-red3/`)

`smart_plan` does not exist before WP1, so the red tree is WP1's tree with `scripts/red_stub.py` applied: the stub
returns today's single order (the first row some member takes, an enemy always attacked) as a one-entry plan of the
members that take it, which through `issue_pointer_order` and `submit_smart` behaves exactly as `main` does.

| Test | Red (stub) | Green |
|---|---|---|
| `context_order` unit tests (P-1) | 10 of 22 fail: the enemy, enemy structure, foundation, own carrier, mobile friend, carrier alone, dropoff, extractor, node and whole-or-nothing tests | 22 pass |
| `a_mixed_selection_right_clicking_its_transport_boards_and_follows_it` (P-2) | fails: only `Load` is queued, the air unit gets nothing | pass |
| `a_worker_and_a_soldier_right_clicking_a_mineral_harvest_and_walk` (P-2) | fails: one order queued | pass |
| `an_unarmed_member_right_clicking_an_enemy_follows_it_while_the_armed_attack` (P-2) | fails: one order queued | pass |
| `frames_after_a_mixed_right_click_create_modify_and_remove_nothing` (P-6) | fails: 2 orders queued, not 3 | pass |
| `smart_mixed_live_frames`, run directly with `SPACE_SCUFFLE_STEADY_FRAMES=64` (P-6) | fails: "the mixed click planned [Load …]" | pass |

The first red run of the transport and steady-state tests failed in their setup (bodies spawned at unchecked offsets
outside the movement domain; bubbles not yet published while paused). After the setup was fixed, both were run red
again and failed on their behaviour assertions, as listed.

### 2.2 Worlds

`git diff --stat 54852435 b8547023` touches no `sim_*` crate and no asset. `scripts/wp.sh wp1 b8547023`, then
`compare.sh world-wp0 world-wp1`: 483 files, **0 differing**. No golden is reachable from a client-only change.

### 2.3 The skirmish match (`client-wp1/`, `client-wp2/`)

`scripts/chain_wp12.sh` exports `b8547023` and `e32c744a` and runs the skirmish match on each: both are won at tick
6,342 (1,645 frames), with a stage log identical to WP0's but for the wall time. §7's WP1 and WP2 skirmish cells
hold.

### 2.4 Tests on the final tree

Debug, from the worktree, one target at a time (`final/`): `client` lib 577 passed; `battle_context_order_tests` 34;
`battle_input_tests` 41; `battle_card_tests` 35; `sync_steady_state_tests` 17; the `smart_mixed` children run
directly (2); `sim_core` `rally_tests` 40 and `catalog_reference_values_tests` 58. None failed, so stop rule 4 does
not apply. `cargo clippy --workspace --all-targets -- -D warnings` exits 0.

### 2.5 Allocation under valgrind (`valgrind-wp1/`)

`scripts/valgrind.sh wp1` runs `sync_allocation_count_tests::smart_mixed_frames_allocate_nothing` alone under the gate
lock. Blocks allocated inside `measured_steady_frames` after 64 warm frames, windows of 64 and 128 frames:

| Child | 64 frames | 128 frames | Difference |
|---|---:|---:|---:|
| `smart_mixed_live_frames` | 3 | 5 | 2 |
| `smart_mixed_baseline_frames` | 3 | 5 | 2 |

The live battle sync adds **0** allocations over 64 running frames in which two boarders walk to the transport and an
air unit follows it; each child checked that the orders were still running when its window ended.

## 3. WP2: rally records proved (`e32c744a`)

### 3.1 Red runs by named mutation (`red/wp2/`, `red/wp2-extra/`; `scripts/red_wp2.sh`)

Each mutation is applied to its own copy of the worktree and never committed.

| Mutation | Test | Result |
|---|---|---|
| `fallthrough`: in `child_rally_slot`, an empty chosen slot falls through to the first slot holding a route | `a_worker_gets_no_order_while_only_the_army_rally_is_set` | fails (the worker takes the army rally); `the_rally_order_table_is_as_documented` passes |
| `extra_slot`: the oracle run on a catalog copy whose Hatchery analogue gains an `All` slot | `no_shipped_product_is_taken_by_two_rally_slots` | fails: the worker and a non-worker product taken by 2 slots, the Queen analogue by 1 |
| `snapshot`: `issue_waypoint_order`'s mobile branch disabled (B-163's old snapshot move) | `the_rally_order_table_is_as_documented` | fails (rows 5 and 8 move instead of following) |

The plan's wording for P-4's mutation, "`slot_used_by` changed to skip an empty slot", cannot fail the worker test on
the shipped data: only slot 1 accepts a worker, so skipping it when empty leaves no slot at all, which is the very
point of §2.2 rule 4. The fall-through mutation above, which ignores the use rule once the chosen slot is empty, is
the rule the test guards against.

### 3.2 Worlds

`git diff --stat b8547023 e32c744a` touches one `sim_*` source file, `production/rally.rs`, in a doc comment only.
`scripts/wp.sh wp2 e32c744a`, then `compare.sh world-wp0 world-wp2`: 483 files, **0 differing**.

## 4. WP3: the owner's rows under the defaults (`202dddb5`)

Q0140 and Q0141 are open, so their defaults (b) apply: B-167 and B-215 stay open, reworded with S8, S10 and S11.
Docs only; no world is touched.

## 5. WP4: bookkeeping

`DESIGN.md` §4's rows carry their commits, and the "Selection and input" row says right clicks are resolved per unit.
The follow-order plan's §14 (B-222) and the rallies plan's D-9 (B-167) point to this plan. The plan gains its
`## As built`, and its README row reads `Partly done`, naming WP3 under an answer and WP-D1 (B-247). Before hand-back,
`cargo gate quick` ran on the head (§2.4 lists the targeted tests).


## 6. WP3 under the owner's answers (Amendment 1, `b1ec5f0d`)

Built on branch `rallies-wp3` from `3d729617` (the `rallies` head with `main` `2516a068` merged), after the owner
answered Q0140 (a sanctioned symmetric deviation, S-exit-order) and Q0141 (c).

### 6.1 Red runs (`wp3-red.log`)

On `3d729617` with the three new `production_transactions_tests` overlaid, all three fail on their position
assertions (the unit appears on the ring's minimum corner, (9.5, 9.5), and the mirrored producer's at (16.5, 16.5)
instead of the reflection (22.5, 22.5)); on `b1ec5f0d` they pass. Nine existing tests pinned the old start and moved
to the new rule (the commit message lists them); the whole of `sim_core`, `sim_types` and `sim_fixtures` passes
(`wp3-simcore2.log`), the golden replays included.

### 6.2 Worlds (`world-base3/`, `world-wp3/`)

No golden, matrix battle, route-trace scenario, bench scenario or harness world trains a unit (no `Produce` in any
fixture, scenario or example), so S-exit-order cannot reach them. `scripts/wp_wp3.sh` measures `3d729617` and
`b1ec5f0d` with the usual `measure.sh`: 483 files, **0 differing**. The goldens are not re-recorded, and the
checksum format is unchanged (no hashed state is added). The exit order runs only when a ready job places its unit,
never on a tick without one, so no per-tick hot path changed and the throughput bench (which trains nothing) was not
re-run.

### 6.3 The skirmish match (`client-base3/`, `client-wp3/`, `client-tbase3/`, `client-twp3/`)

The skirmish trains seven units at its Barracks analogue with no rally, so it moves, as the owner dictated
(restated here under AGENTS.md §8):

| Tree | Won at tick | Frames | Stages |
|---|---:|---:|---|
| `3d729617` | 6,342 | 1,645 | 0, 0, 0, 22, 550, 586, 642, 4,502, 4,526, 4,822, 4,834, 5,086, 5,098, 5,350, 5,362, 6,334 |
| `b1ec5f0d` | **6,338** | 1,644 | identical up to 5,362, then 6,330 |

First-difference trace (`scripts/patch_spawn_trace.py`, a scratch print of each trained unit's spawn, never
committed; the patched trees give the same stage logs as the unpatched ones): the first difference is at **tick 2,264,
entity 16** (the first unit the Barracks analogue, entity 15, trains), which appears at **(14.5, 6.5)**, the ring
corner nearest the map's centre, instead of (12.5, 2.5), the ring's minimum corner. Every one of the seven trained
units appears on the centre-facing side (WP3: (14.5, 6.5), (13.5, 6.5), (12.5, 6.5), (11.5, 6.5), (10.5, 5.5),
(10.5, 4.5), (14.5, 6.5); before: (12.5, 2.5), (13.5, 2.5), (14.5, 2.5), (14.5, 3.5), (14.5, 4.5), (14.5, 5.5),
(12.5, 2.5)), so the army gathers on the side facing the enemy and the match ends 4 ticks sooner. No unit sticks at its
exit, and the match is won within `TICK_LIMIT`, stage for stage. The skirmish is not a matchup claim of
`DESIGN.md` §5, so no claim is restated.

### 6.4 Q0141 (c)

`max_waypoints: 32` on all 22 producers of `structures.ron`. A Shift-click past the cap keeps today's refusal
path, `RallyWaypointsFull`: the route is unchanged (the click is ignored, as answered); the client still shows the rally
marker for the click, since it submitted a valid order, and the next tick's status line reports "the rally holds its
most waypoints". `catalog_reference_values_tests::the_shipped_rallies_are_5_0_14s` pins 32, and
`rally_tests::shift_appends_waypoints_up_to_the_cap` fills 32 distinct points and is refused the 33rd.

### 6.5 Checks

Client targets reading the harness change: `battle_input_tests` 41, `battle_card_tests` 35,
`battle_context_order_tests` 34, `rally_gpu_tests` 1 (one ignored by design), `runtime_battle_tests` 18, all passing.
The client's source is unchanged, so the valgrind and steady-state suites are not re-run. Workspace clippy exits 0.


## 7. WP-D1, built as WP-T1 of plan `2026-12-03-movement-work-on-solver` (`6efa92df`, `132d2489`)

Built on branch `transport-load` from `main` `cd11ec75` (schema 63, hangar commands merged), under that plan's WP-T1
and this plan's Amendment 2. Scratch material: `~/scratch/transport-load/`.

### 7.1 Sources re-read

- `CAbilTransport` (`core` AbilData:1192–1212): `Range 1`, `MaxUnloadRange 3`, `AllowSmartCmd 1`,
  `AllowPassengerSmartCmd 1`. The load range is 1, as `BOARDING_REACH` (1.0, edge to edge) already is, so no bug row
  is filed (movement plan §3.4). `MedivacTransport` (`liberty` AbilData:1359–1379) sets no range of its own, and no
  later layer overrides it.
- Whether the passenger keeps its orders while the transport comes is still not in the data; it stays an inference
  [INF], and the tests pin the built rule (the passenger keeps them until it boards).

### 7.2 Red runs

On the stub (the variant present, its admission `UnsupportedOrder`, the client's row off), every new test failed:
`red-sim-core-transport-pickup.log` (10 of 10 in `transport_pickup_tests`; the refusal test was split in two after
the red run, for the function-length lint) and `red-client-smart-row.log` (`context_order`'s
`a_carrier_picks_up_an_own_walker_and_follows_an_own_flyer` and `battle_context_order_tests`'
`a_transport_right_clicking_an_own_soldier_picks_it_up`: the carrier was planned a `Follow`).

### 7.3 Schema (movement plan §4.6, stop rule T-2)

`6efa92df` with the `LoadUnit` arm encoding nothing, at schema 63, reproduces both goldens at every checkpoint and
the sandbox's `7_726_200_156_173_230_158` on native debug and release and `wasm32-wasip1` release and debug: 8 of 8
plus the sandbox on four builds (`golden-latent63/summary.txt`). `132d2489` then takes schema **66** (provisional:
64 and 65 are the missiles and diagonal-walkability branches'), encodes tag 23 and the passenger, and re-records.

WORLDS_AND_SKIRMISH

THROUGHPUT

VALGRIND
