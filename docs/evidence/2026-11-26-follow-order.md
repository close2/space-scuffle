# Evidence: the follow order (B-163, B-166's remaining half, B-187)

Plan: [`docs/plans/2026-11-26-follow-order.md`](../plans/2026-11-26-follow-order.md). Measured on the package branch
`follow-order-plan` on top of `main` `b4a99fae` (plan commits `a502d94a`, `cc1d0ecc`; merge `581ba63d`), from
2026-11-25, on the shared VM. Scratch material lives under `~/scratch/follow-order/` (`scripts/`, `bin-<wp>/`,
`tree-<wp>/`, `world-<wp>/`, `red/`, `golden-<label>/`, `client-<label>/`, `perf-<label>/`); the scripts are adapted
from the rallies plan's (`~/scratch/rallies/scripts/`).

## 1. WP0: records, probe and baseline

### 1.1 Records

`DESIGN.md` §4 registers B-221 (`FollowAcquireRange` not modelled), B-222 (per-unit smart resolution) and B-223 (the
Battlecruiser's slop), with provisional numbers. `main`'s register ends at B-218, and the friend-crowd branch in
flight takes B-219 and B-220, so the plan's provisional numbers stand and nothing is renumbered. B-163, B-166 and
B-187 point to this plan.

### 1.2 Worlds (`world-wp0/`)

`scripts/wp.sh wp0 581ba63d` exports the commit (WP0 changes only documents, so its code is `581ba63d`'s) to
`tree-wp0/`, builds `matchup_trace` and `route_trace` in release in a separate target directory
(`/home/spacescuffle/cargo-target/follow-order-measure`) and runs `scripts/measure.sh` (the rallies plan's: the matrix
with every battle on its seeds of record and 16 seeds, battle 31 on 100 seeds, battle 33 on 64, the showcase and
showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks): 483 files, the reference for every
later comparison by `scripts/compare.sh` (every output file byte for byte; logs after removing the output path).

### 1.3 P-0: no measured world gives a follow-type input

`scripts/probe.sh 581ba63d` patches an archive of the commit (`scripts/patch_probe.py`, never committed) so that
`step_into` prints one line when a world starts, `issue_waypoint_order` one line per rally spawn on a `Unit` waypoint
(both rows of D-9), `context_order` one line per right click on a friend that falls through to `Move` (D-10), and
`submit_attack_on_unit` one line per A-click on a unit with its attacker and walker counts (D-10). It builds the trace
binaries from that tree and runs `measure.sh` into `world-probe/`:

| Lines | Count |
|---|---:|
| `P0PROBE start` (worlds played) | 1,410 |
| `P0PROBE rallyunit` (rally spawns on a unit) | **0** |
| `P0PROBE friendmove` (right clicks on a friend falling through to `Move`) | **0** |
| `P0PROBE attackclick` (A-clicks on a unit) | **0** |
| Follow orders in the scenarios, `sim_fixtures` and the sim and client tests (`\bFollow *[{(]`) | **0** |

The probe's output files are byte-identical to `world-wp0` (483 files); the 21 differing logs differ only by the
probe's own lines, checked by removing them. So no measured world gives an input that becomes a follow, and plan §7's
predictions stand.

### 1.4 The skirmish match (`client-wp0/`, `client-probe/`)

`scripts/skirmish.sh` runs `skirmish_match_tests::a_match_is_won_through_real_input_alone` in release from an exported
tree in its own target directory (`follow-order-client`). On WP0's tree the match is won at tick 6,342 (1,645 frames),
the pilot's stages at ticks 0, 0, 0, 22, 550, 586, 642, 4,502, 4,526, 4,822, 4,834, 5,086, 5,098, 5,350, 5,362 and
6,334. With the probe's tree the stage log is identical (only the wall time differs), and the probe prints two world
starts and no `rallyunit`, `friendmove` or `attackclick` line: the pilot gives no input that becomes a follow.

## 2. WP1: the order, its state and admission (`b913f97e`, re-record `4e77c0b5`)

### 2.1 Red run (`red/wp1-red.log`)

`scripts/red_wp1.sh`: `main`'s code (`581ba63d`) with WP1's `follow_order_tests.rs` and `checksum_state_tests.rs`
overlaid fails to compile (`UnitOrder::Follow`, `FollowKind`, the four `FollowTarget*` refusals and
`MovementMemory::follow_rest` do not exist), which counts as red for WP1 (§6). The encoding unit tests are red on the
proof tree below, which keeps schema 49 (`the_schema_is_50` and the header tests). All pass on WP1's tree.

### 2.2 The §5 proof (`golden-proof-old/`, `golden-wp1-new/`)

`scripts/goldens.sh` git-archives a tree into scratch, optionally patches it, and runs `golden_replay_tests` and
`sim_fixtures`' `fixture_tests` on native debug and release and `wasm32-wasip1` release and debug
(`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`), in a fresh target directory removed
afterwards.

| Run | Tree | Result on all eight runs |
|---|---|---|
| `proof-old` | WP1's code (a snapshot equal to `b913f97e` but for one doc comment), patched by `scripts/old_encoding.py`: `follow_rest` unhashed (`fields!(... unhashed { follow_rest })`), schema 49 | pass: `main`'s lists of both fixtures and `DEFAULT_SANDBOX_CHECKSUM` (`2_504_113_823_383_333_194`) reproduced |
| raw lists | the worktree at schema 50 with the fixtures' `checksum_schema` set to 50 (native debug) | every checkpoint differs; the printed lists and the sandbox checksum `13_849_945_286_125_516_044` were recorded |
| `wp1-new` | `4e77c0b5` (WP1 plus the re-record) | pass |

### 2.3 Worlds

`compare.sh world-wp0 world-wp1` (`b913f97e`): 483 files, 0 differing.

## 3. WP2: following (`b5e9f218`)

### 3.1 Red run (`red/wp2-red.log`)

On WP2's tree with the follow made inert (`scripts/red_wp2.py`: `walking_goal`'s `Follow` arm removed, so a follower
coasts as in WP1, and `stands_idle` reduced to `is_idle`), six tests fail:
`a_follower_keeps_within_the_dead_band_of_a_walking_friend` (P-3),
`a_follower_rests_by_a_standing_target_and_resumes_only_past_the_slop` (P-4),
`a_follow_ends_when_its_target_dies_or_boards` and `a_follow_with_an_order_behind_it_ends_on_reaching_its_target`
(P-5), `twelve_followers_bunch_round_their_target_and_again_after_it_moves` (P-6) and
`resting_followers_yield_to_their_target_walking_back_through_them` (P-8). The five admission tests (P-2) pass there.
All pass on WP2's tree.

### 3.2 What the tests showed while building (plan As built, items 2 to 4)

P-6 failed on the text of D-4 in three ways, each traced in a scratch run and fixed by the rule named in the As built:

- With the glide of B-39 on coming to rest, twelve followers converging on a standing target pushed it 0.0149 world
  units, twice D-α's resting bound (1 % of the radius sum, 0.0075): a rest is now an arrival, with no glide.
- After the target walked 20, eleven of the twelve stayed behind: each follower that resumed came to rest again on a
  neighbour whose rest was still valid against a fresher anchor, so the group re-anchored in waves and never moved.
  With the witness's anchor inherited, the group resumes on one tick and bunches round the target again.
- Trailing followers walked into the resting group at speed and pushed the target 0.16 on: the walk now also brakes
  for the rest band of a resting follower ahead.

### 3.3 Worlds, solver diff and the simulation crates

`compare.sh world-wp0 world-wp2`: 483 files, 0 differing. `git diff 581ba63d b5e9f218 --stat` names no
`movement/solver*` file (P-8). `scripts/simall.sh wp2 b5e9f218` (`cargo test -p sim_types -p sim_core -p
sim_fixtures`, debug, from an archive of the commit, under the gate lock): 1,781 passed, 0 failed, 5 ignored.

### 3.4 Throughput (P-13)

User-space instructions of the whole benchmark (every scenario, three repeats of 300 ticks), `perf stat -e
instructions:u`, alternated, each run under the gate lock (`perf-wp2/`):

| Tree | Run 1 | Run 2 |
|---|---:|---:|
| WP0 (`581ba63d`) | 269,045,795,825 | 269,066,026,113 |
| WP2 (`b5e9f218`) | 275,733,074,100 | 275,733,180,611 |

WP2 is 2.48 % above WP0, within the 3 % of P-13. No measured world issues a follow, so the difference is the
code's, not a follower's: an instruction-sampled profile of both binaries (`profile/`) shows the inlining of the
locomotion module changed (`step_toward` is out of line in WP2), with no follow function among the hot symbols. The
profile's sample counts were unreliable under the server's load (a load average of 130 to 170), so it locates the
change and does not apportion it. Wall-clock ticks per second measured the load in this round (WP0's own casters
scenario ranged from 124 to 189 between runs); the floor is checked in §4.3 at a lower load.

## 4. WP3: enemies, acquisition and rallies (`0a797bab`)

### 4.1 Red run (`red/wp3-red.log`)

`scripts/red_wp3.sh`: WP2's commit with WP3's `follow_order_tests.rs` and `rally_tests.rs` overlaid. Eight tests fail:
`a_scan_move_follower_fights_on_the_way_and_follows_again` and
`a_follow_of_an_enemy_lost_to_fog_ends_at_its_last_seen_point` (P-7), `the_follow_works_through_player_commands_alone`
(P-10), and `a_unit_rallied_to_a_friend_follows_it`, `a_dead_waypoint_is_skipped_and_an_all_dead_route_is_cancelled`,
`a_unit_that_cannot_attack_follows_the_enemy`, `a_route_of_two_own_units_is_followed_in_turn` and
`a_new_unit_following_its_rally_unit_stops_when_it_dies` (P-9). `a_plain_follower_acquires_nothing` and
`a_rally_on_an_own_structure_stays_a_point_move` pass there, as they should: both pin behaviour WP3 keeps. All pass
on WP3's tree.

### 4.2 Worlds

`compare.sh world-wp0 world-wp3`: 483 files, 0 differing.

### 4.3 Throughput (P-13, the final simulation tree)

`perf-wp3/`, alternated as in §3.4: WP0 269,045,802,501 and 269,066,024,242 instructions; WP3 275,767,383,425 and
275,767,503,868 (+2.50 %, within 3 %). WP4 changes only the client, so WP3's simulation is the final one. Ticks per
second (median of three repeats) of the same runs, at a load average of about 24:

| Scenario | WP0 run 1 | WP3 run 1 | WP0 run 2 | WP3 run 2 |
|---|---:|---:|---:|---:|
| 64x64, 0 units | 3,062 | 3,064 | 3,122 | 3,013 |
| 64x64, 100 units | 765 | 794 | 733 | 758 |
| 64x64, 400 units | 60 | 106 | 104 | 104 |
| 128x128, 400 units | 74 | 177 | 170 | 174 |
| 256x256, 0 units | 189 | 195 | 140 | 185 |
| `showcase_canyon`, 400 units | 69 | 80 | 51 | 78 |
| 64x64, 67 casters, 400 units | 185 | 189 | 198 | 192 |

Every scenario of WP3 is at or above 50 ticks per second (78 at least).

### 4.4 The simulation crates

`scripts/simall.sh wp3 0a797bab`: 1,788 passed, 0 failed, 5 ignored (the goldens and the sandbox checksum at WP1's
lists among them: no golden moved after WP1).

## 5. WP4: client (`27604c11`, `75253b34`)

### 5.1 Red run (`red/wp4-red-lib.log`, `red/wp4-red-int.log`)

On WP4's tree with the follow row and the Scan Move follow switched off (`scripts/red_wp4.py`), four tests fail:
`context_order::tests::a_mobile_friend_is_followed_by_the_rest_of_the_selection`,
`commands::tests::an_attack_on_a_unit_splits_the_selection_into_attackers_and_walkers`,
`battle_context_order_tests::a_soldier_right_clicking_a_mineral_behind_a_friend_follows_the_friend` (T17) and
`battle_mode_tests::a_mixed_army_a_clicked_on_an_air_unit_follows_attacks_and_walks`. All pass on WP4's tree.

### 5.2 Client tests (`client-wp4/`)

Release, radv, each target under the gate lock, GPU children locking the GPU themselves (`GPU_LOCK_TIMEOUT=1200`):

| Target | Result |
|---|---|
| `battle_context_order_tests` | 32 passed |
| `battle_input_tests` | 40 passed |
| `battle_mode_tests` | 22 passed (after `75253b34`; 21 and 1 failure on `27604c11`, the own-unit A-click test) |
| `battle_card_tests` | 35 passed |
| `battle_hud_tests` | 35 passed |
| `sync_steady_state_tests` | 11 passed |
| `sync_allocation_count_tests` (valgrind) | 10 passed |
| `rally_gpu_tests` (radv) | 1 passed, 1 ignored |
| `skirmish_match_tests` | 8 passed, 2 ignored (the windowed children) |
| client `--lib` | 545 passed (after `75253b34`; 544 and 1 failure on `27604c11`, the idle-friend row test) |

The skirmish match is won at tick 6,342 in 1,645 frames, and every stage of WP0's run (§1.4) recurs at its tick.

## 6. The final tree (merge of `main` `263ef402`, `4f4ef72d`)

`main` moved by documents only (the blank-sheet movement round and Q0120 to Q0124); the merge conflicted only in the
plans README's last rows, both kept. On `4f4ef72d`:

- `scripts/goldens.sh final 4f4ef72d`: both fixtures and `DEFAULT_SANDBOX_CHECKSUM` pass on native debug and release
  and `wasm32-wasip1` release and debug, at WP1's lists (`golden-final/`).
- `scripts/skirmish.sh final` on an archive of the commit: won at tick 6,342 in 1,645 frames; the stage log is
  identical to WP0's (`client-final/`).
- `cargo gate quick`: green (fmt, clippy on `client`, `sim_core`, `sim_fixtures` and `sim_types`, 147 test targets);
  `cargo clippy --workspace --all-targets -- -D warnings`: clean.
