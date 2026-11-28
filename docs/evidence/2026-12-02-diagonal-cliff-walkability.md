# Evidence: Diagonal Cliff Walkability, the Simulation Clipped to the Drawn Diagonal (B-251, Q0139)

Measurements of plan [`2026-12-02-diagonal-cliff-walkability`](../plans/2026-12-02-diagonal-cliff-walkability.md):
the census of WP0, the red runs of every new test, and the WP1 proof that the predicate moved without change. WP2's
green runs, latent proof, re-records, measured worlds and throughput go in §3 to §5 when it lands. Scratch material
(logs, the census probe) is under `~/scratch/diagonal-walk/`; none of it is committed.

## 1. WP0: the census (§7.4)

Base: `main` at `8ee093f1`, branch `diagonal-walk`. The probe (`~/scratch/diagonal-walk/census/diagonal_census.rs`,
run as a temporary `sim_fixtures` example and deleted) counts `GameMap::diagonal_cut` (the WP1 predicate, equal to the
independent rule by T-D2) over every map a measured set builds through its own constructor. Output `census.txt`.

| Measured set | Map | Cuts |
|---|---|---:|
| Golden replays (`golden_replay.ron`, `golden_replay_orders.ron`), the `default_arena` tests | `default_arena` | 0 |
| Showcase, effects showcase, skirmish and default-battle scenarios, the route traces of `measure.sh`, the canyon throughput row, the `sim_core` and `sim_map` tests that load the canyon | `showcase_canyon` | 24 |
| Matchup matrix (`sim_fixtures::matchup::arena`, 61 battles, 3 with terrain regions) | flat 0 plus rectangles | 0 |
| Sandbox (`DEFAULT_SANDBOX_CHECKSUM`), the flat throughput rows, the harness and held-out worlds, `mirror`, the crowd, clash, blocking and route-overlap traces | `GameMap::new` at one level (blocked cells by flags only) | 0 by construction |

- `showcase_canyon`: 24 cut cells, two runs of 12 at threshold 2, all `la 2, lb 1, t_hi 2`; 4 of them on a chunk seam
  (`(24, 15)`, `(15, 24)`, `(47, 40)`, `(40, 47)`), as the plan's §1.5 states.
- **Stop rule 5 holds.** No resource anchor, placed node cell, watchtower or start footprint of either shipped map
  lies on a cut cell, and neither shipped scenario order cell (`(52, 52)`, `(11, 11)`) is one. Map validation of both
  shipped maps reports 0 errors.
- Worlds with 0 cuts are predicted byte-identical by Lemma L-2 and are checked by digest in WP2. The worlds that may
  move are those on `showcase_canyon`.

## 2. WP0: red runs

Every new test was run alone on the WP0 tree (`4a90f380`, the predicate stubbed to `None` for T-D1 and T-D2; every
other test reads the cuts through the independent rule of `crates/sim_map/tests/support/diagonal.rs`, so the stub
does not affect it). Tests that need the clip are `#[ignore]`d with "diagonal-walk WP2" so the gate stays green until
WP2; the guards run. Logs: `~/scratch/diagonal-walk/red-*.log`.

| Test | Where | Result on `main` | Why red (or guard) |
|---|---|---|---|
| T-D1 `the_cut_follows_its_block` | `sim_map::diagonal` unit | red (stub), 8 expectations | the WP0 stub cuts nothing; green in WP1 |
| T-D2 `predicate_matches_the_plan` | `diagonal_walkability_tests` | red (stub): 105 fixture and 24 canyon cuts missing | guard after WP1 |
| T-W1 `no_disc_stands_beyond_the_foot_*` | same | red: fixture 427,665 of 2,784,810, canyon 97,752 of 636,528 centres closer than `r` to the foot stand free; 0 of the 118,650 / 27,120 centres that keep their radius from every wall are refused | the outer corner admits a body |
| T-W2 `no_route_lost_across_diagonals_*` | same | guard, recorded: fixture 501 samples in 21, 21, 21, 46, 87, 105, 105 classes for the seven radii; canyon 128 samples in 1 class each | — |
| T-W3 `routes_and_contact_agree_on_the_clip` | same | red: 387 goals in outer triangles answered with the cut cell's centre, on the foot; all 25,142 + 11,018 funnel legs pass | the cut cell's centre is reachable |
| T-M1 `the_foot_is_a_wall_and_beyond_is_blocked` | same | red: 510 failures (all 420 foot edges are no wall; the triangles beyond them are `Open`) | no diagonal terrain wall |
| T-L `lane_cuts_ignore_diagonal_walls` | same | guard, recorded (cut lists of the three maps, `max_edge_crossings` of `default_arena`) | — |
| T-T1 `segments_ending_beyond_the_foot_are_not_connected` | same | red: 387 segments ending in `T(X)` connected | no endpoint test |
| T-T1, guard half `segments_between_legal_points_answer_as_on_main` | same | guard, recorded: 1,977,462 fixture and 475,224 canyon segments | — |
| T-P1 `buildings_and_nodes_refuse_cut_cells` | same | red: 105 one-cell buildings and 105 mineral nodes on cut cells admitted; a canyon watchtower on cut cell `(25, 14)` adds no validation error | placement reads whole cells |
| T-C1 `cut_cells_never_overstate_clearance_*` | `clearance_field_tests` | red: all 105 + 24 cut cells claim 8 eighths, the closed half behind the foot holds at most 5 | the corner opposite `C` fits a disc of 1 on `main` |
| T-G1 `swept_bodies_refuse_the_outer_triangle` | `sim_core::movement::geometry` unit | red: 856 of 5,996 swept bodies wrongly admitted, 0 wrongly refused | the square envelope reads whole cells |
| T-G2 `a_unit_ordered_to_a_step_corner_stops_behind_the_foot` | `diagonal_cliff_movement_tests` | red: radii 0.3125, 0.375 and 0.5 stop at the cut cell's centre `(14.5, 25.5)`, on the foot | the order is clamped to the cut cell's centre |
| T-G3 `crowds_pressed_on_a_diagonal_stay_behind_it` | same | red: 1,029 body-ticks closer than `r − 2⁻²⁰` behind a foot | — |
| T-G4 `spawns_and_blinks_avoid_the_outer_triangle` | same | red: 407 violations (a Blink lands in `T(3, 8)`, the death spawn is placed there, egress units stand in it) | — |
| T-K′ `brink_drop_where_the_simulation_admits_a_body_*` | client `diagonal_cliff_tests` | red: canyon 1,920 and fixture 8,400 admitted centres over drops above 0.2, up to 3.0 | the simulation admits the drawn low floor |

Findings while writing the tests (none changes the plan's design):
- **T-W3, funnel fallbacks.** At radius 0.75 on the fixture, twelve legs of routes that fall back to portal midpoints
  fail the segment predicate on `main`: exact pinches of the diagonal bands, which B-141 keeps by design ("a diagonal
  exact pinch keeps the counted fallback"). T-W3 checks funnel routes only (`Pathfinder::mesh_funnel_fallbacks`
  unchanged across the search), and exempts legs from a point where the disc does not fit (pathfinding plan D-3).
- **T-G2's goal.** `C` is also the corner of the diagonal low cell, and a move to a blocked goal is clamped to the
  centre of the goal's cell (`nearest_reachable_point`), so an order to `C` exactly walks to the low side. The order
  goes an eighth of a cell inside `C`, in `T(X)`, which on `main` is clamped to the cut cell's centre.
- **T-K and T-K′.** The drawing plan's T-K stays as it is and keeps guarding the uncut cells until WP2; T-K′ is its
  measure with the simulation's admission and the 0.2 limit in cut cells, `#[ignore]`d until WP2 replaces T-K with it.
- **Radii in `sim_map`.** `sim_map` cannot depend on `sim_core`'s catalog loader (charter §5), so its tests read
  `assets/catalog/units.ron` through `ron` into the loader's fields, with its documented default (an omitted
  `terrain_radius` equals `collision_radius`): seven radii, 0.3125 to 1.0, the counts of the plan's table.

## 3. WP1: the predicate moved without change

- `sim_map::diagonal::DiagonalCut::of_block` is the client's `Cut::of_block` verbatim (same direction order, same
  `find_map`, same `t_hi` and `lb < t_hi`), on `i8` levels; `marching_level` is the client's `cut_level` with its
  `// PERFORMANCE RATIONALE:` block; `GameMap::diagonal_cut` reads the cell and its four 4-neighbours first and the
  3x3 block only when two perpendicular neighbours are lower (unit test `the_early_out_never_drops_a_cut`).
- The client builds its `Cut` from `DiagonalCut` (`Cut::from`), `DualCuts::read` hands each 3x3 window of its single
  4x4 read to `DiagonalCut::of_block`, and `cut_level` is gone; the drawing plan's T-S2 table moved to `sim_map` as
  T-D1.
- Green: T-D1, T-D2; `mesher_golden_tests` (6, hashes unchanged), `terrain_tests` (14, counts unchanged),
  `diagonal_cliff_tests` (9, and T-K′'s two ignored), `cliff_boundary_tests` (10), the mesher's unit tests (11). No simulation function reads
  the module yet, so no checksum and no world can change.
