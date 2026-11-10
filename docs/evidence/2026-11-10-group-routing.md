# Group-consistent routing: measurements per step

This file records the measurements of plan [`2026-11-10-group-routing`](../plans/2026-11-10-group-routing.md) (D-8):
the whole matchup matrix and battle 33 against the step before, the golden replays, the shipped showcase and the G-1
fixture through `route_trace`, and the throughput benchmark. The base is the deviation follow-ups tree `a2240e2`
(checksum schema 26) with the plan's WP1 and WP2 merged (`26f7578`), commit `e2e7e30`.

## How to reproduce

Every tool is a release build of an example of `sim_core`, copied once per step. The scripts used are in
`~/scratch/group-b/` (not committed); the commands they run are:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/group-b
cargo build --release --locked -p sim_core --example route_trace --example matchup_trace
# Shipped showcase, with each unit's first crossing of y = 32 (lanes.csv) and the groups (groups.md).
route_trace --scenario scenarios/showcase.ron --ticks 1800 --lane-line 32 --out <dir>/route
# G-1 fixture: the same with player 1's order before plan 2026-11-09-sc2-pathfinding Amendment 5, built in memory.
route_trace --scenario scenarios/showcase.ron --ticks 1800 --lane-line 32 \
    --order "AttackMoveToCell(player_slot: 0, cell: (52, 52))" \
    --order "AttackMoveToStart(player_slot: 1, target_player_slot: 0)" --out <dir>/g1
# --lane-tolerance <permille> replaces the catalog's tolerance for a run (WP4 measurements at 75 before WP5).
matchup_trace --all-battles --seeds 16 --jobs 6 --out <dir>/matrix --compare <previous>/matrix/matrix.csv
matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
```

The golden traces are per-tick `Debug` dumps of every entity, projectile, player's resources, the match state and the
structures after each tick of both golden scripts, written by a scratch observer appended to
`crates/sim_core/tests/golden_replay_tests.rs` in a copy of the tree (not committed), and compared with `cmp`.

A unit's lane is the x where it first crosses y = 32: the west lane crosses at about x 25.6 and the east lane at
about x 38.4.

## WP3: movement rules, groups, ordinals, schema 27 (`adfad28`)

Shipped tolerance 0: no group is formed.

| Measure | Result |
|---|---|
| Matrix, 16 extra seeds (1,000 runs) | bit-identical to the base `matrix.csv` |
| Battle 33, 68 seeds | every seed's end tick and survivors identical |
| Shipped showcase `units.csv`, 1,800 ticks | byte-identical |
| Goldens | re-recorded for schema 27; the per-tick dumps (golden_replay 2,310,074 lines, golden_replay_orders 1,835,834) equal the base's after removing the `route_orders_issued` lines |
| Sandbox checksum | re-recorded for schema 27; spawned state unchanged |

## WP4: the lane rule, route events, `route_trace` columns (shipped tolerance 0)

Bit-identical to WP3, as D-6 requires:

| Measure | Result |
|---|---|
| Matrix, 16 extra seeds (1,000 runs) | bit-identical to WP3's `matrix.csv` |
| Battle 33, 68 seeds | identical |
| Shipped showcase `units.csv` | byte-identical |
| Goldens and sandbox checksum | pass unchanged (debug) |

Measured with WP4's build and `--lane-tolerance 75` before the data change:

| Run | Tolerance | Player 1's lanes at y = 32 | Groups and `GroupLane` events | Winner | In range at tick 900 (p0 / p1) | Reversals (p0 / p1) |
|---|---:|---|---|---|---|---|
| G-1 fixture | 0 | 8 east (x 38.4), 2 west (the rhino beetles stop at the plateau's west foot) | none | player 1, tick 1,266 | 8 of 10 / 8 of 8 | 64 / 2 |
| G-1 fixture | 75 | all 10 east (x 38.4 to 38.5) | player 1's group: leader 22, lane word `[-1]`; 3 events (tick 0: both rhino beetles; tick 213: one pulse bot re-planning), all adopted, lane/own 1.0032 / 1.0037 / 1.0047 (min / mean / max) | player 0, tick 1,093 | 10 of 10 / 10 of 10 | 6 / 3 |
| Shipped showcase | 75 | (player 1 meets player 0 north of y = 32) | two groups, lane word `[]` each, no event | player 0, tick 1,693 | as at 0 | as at 0 |

The shipped showcase's `units.csv` at 75 is byte-identical to the run at 0: every member's own route is already in
its group's lane. G-1's winner returns to player 0, which is the fixture's purpose (§1 of the plan).

## WP5: tolerance 75 ‰ shipped

| Measure | Result |
|---|---|
| Matrix, 16 extra seeds (1,000 runs) | bit-identical to WP4's `matrix.csv` (no battle moved; the arenas have no lane cut) |
| Battle 33, 68 seeds | identical to WP4 |
| Shipped showcase `units.csv`, 1,800 ticks | byte-identical to WP4 (player 0 wins at tick 1,693); two groups, lane word `[]`, no `GroupLane` event |
| G-1 fixture | `units.csv` byte-identical to WP4's run at `--lane-tolerance 75` (table above): player 1's ten attackers all east, three adoptions (ratios 1.0032 to 1.0047), player 0 wins at tick 1,093 |
| Goldens | re-recorded. Cause: the hashed route groups alone. The per-tick dumps of both scripts at 75 are byte-identical to WP3's (and so to the base's but for `route_orders_issued`): `default_arena` has no lane cut, so every own route is already in its group's lane and no restricted search runs; the checksums differ from the first checkpoint because the squads' groups are hashed. Passed in debug, release and `wasm32-wasip1` (wasmtime) |
| Sandbox checksum | unchanged (no group at spawn) |
| `sim_core` and `sim_fixtures` tests | pass at 75 |

Own-to-lane ratios of every `GroupLane` event: the G-1 fixture's three (1.0032, 1.0032, 1.0047); none in the shipped
showcase and none in the goldens. Every adoption so far lies far below 7.5 %.

The storyboard (`assets/captures/showcase.ron`) was not re-rendered: the shipped showcase did not move.
