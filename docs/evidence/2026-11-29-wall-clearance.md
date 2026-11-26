# Evidence: wall clearance (plan `2026-11-29-wall-clearance`)

The measurements of plan [`2026-11-29-wall-clearance`](../plans/2026-11-29-wall-clearance.md), one section per work
package. The diagnosis it builds on is [`2026-11-29-wall-grazing`](2026-11-29-wall-grazing.md).

## WP0: the meter, the scenes, the base figures

**Tree.** Branch `wall-clearance-plan` at `c49082af` (`main` `404a5672` plus the plan and the Q0134 answer
`48ac2ff6`; checksum schema 61) plus WP0. WP0 changes no simulation code: it touches `crates/sim_fixtures`, two test
files, the `route_trace` example's doc comment, `DESIGN.md` §4 and this file. Measured on 2026-11-26 (session date)
on the shared VM. Scratch material is under `~/scratch/wall-clearance/` (`base/`, `cost/`, `scripts/`,
`mesher_check/`).

### Method

- **The meter** is `sim_fixtures::trace::route_clearance` (plan §6.1), wired into `run_route_trace` and on by
  default (`--clearance off` switches it off; `--clearance-box` sets its regions; the defaults are the evidence's
  ramp W channel, ramp E channel and central block). It writes `clearance.csv`, `clearance_episodes.csv`,
  `clearance_routes.csv` and `clearance_summary.md`. Definitions are the plan's: walking unit-tick, clearance at the
  terrain radius (reach 1.5), M-1 at clearance ≤ 2⁻¹⁴, M-2 below `CORNER_SAFETY_MARGIN`, episodes, M-3 (either tick
  M-2, split proposal / contact), M-4, M-5a (exact `segment_is_passable` at `r − 2⁻¹⁴` while the body is no deeper
  than 2⁻¹⁴, sampled depth beside it), M-5b by task, and the deepest body.
  - Task kinds are read from orders: a `Follow` order is a follow; an attack target (the order's, or the acquired
    `current_target`) is a chase; anything else a walk.
  - The meter's slack is `sim_core`'s `WALL_PROBE_SLACK` (2⁻¹⁴), which is not exported; it is named again in the
    meter, as the blocking meter names `STANDING_DRIFT`.
- **Tests.**
  - `crates/sim_fixtures/tests/trace_tests.rs`: the five of §6.1:
    `the_clearance_meter_counts_a_walk_along_a_wall_at_a_known_clearance`,
    `the_clearance_meter_finds_one_episode_and_its_ticks_lost_for_a_body_riding_a_wall`,
    `the_clearance_meter_counts_a_reversal_at_a_wall_and_not_one_in_open_ground`,
    `the_clearance_meter_finds_a_route_that_cuts_a_corner_and_not_one_that_grazes_it`,
    `the_clearance_meter_leaves_the_route_traces_other_outputs_byte_identical` (showcase, 40 ticks, meter on and
    off: `units.csv`, `passes.csv`, `overlap.csv`, `summary.md`, `reversals.csv`, `groups.md` equal).
  - Unit tests in the module: the probe's classes, an episode broken by a gap in a unit's ticks, a standing unit,
    and the CSV and summary formats.
- **Scenes** (`crates/sim_core/tests/route_following_tests.rs`): `rounding_depth_scene` (case D: a walker on the
  face y = 16 of `corner_world`, 2⁻²² inside it, beside its arc round (16, 16)) and `pushed_off_leg_scene` (case A:
  on the new `block_world`, a walker on the straight route (4, 15) → (28, 15) pushed to (8, 16.4), whose leg passes
  0.16 from the block's corner (16, 16)). `the_wall_clearance_scenes_are_what_they_claim` checks only their
  geometry, through the meter: case D's walker is M-1 at a depth below 2⁻¹⁴; case A's held route is M-5a from open
  ground. It asserts no route-following behaviour.
- **Runs.** `route_trace` (release) on `scenarios/showcase.ron` and `scenarios/showcase_effects.ron`, the default seed
  and seeds 1 to 32, played to the end or tick 3,000. Script `~/scratch/wall-clearance/scripts/measure_base.sh`,
  tables `scripts/base_tables.py` (outputs `base/base_tables.md`, `base/base_figures.csv`). The runs are
  deterministic and take about 2 s each; they were run without the gate lock, which was held by another agent's
  `cargo gate package` for over half an hour (plan §6.4 asks for the lock; the figures cannot depend on it).

### Identity (plan §8 WP0 acceptance)

The base binary (`c49082af`, built from a clean export) and the WP0 binary were run on both scenarios, the default
seed and seeds 1 to 8. Every file the base writes (`units.csv`, `passes.csv`, `summary.md`, `overlap.csv`,
`reversals.csv`, `groups.md`) and the standard output are byte-identical: **144 of 144** comparisons
(`base/identity.txt`). WP0 touches no simulation file, so worlds, goldens and `DEFAULT_SANDBOX_CHECKSUM` cannot move;
`cargo gate quick` ran the changed crates' fast tests, the goldens among them (see the hand-back).

### Agreement with the evidence's probe

- **Touching table.** Counted as the probe counted (clearance < 0.005, "moving" as `|dx| + |dy| > 0.005`, ticks below
  2,300), the meter's nine-seed classes are ramp W 39 / 378 / 35, ramp E 37 / 270 / 27, elsewhere 42 / 9 / 58: the
  evidence's figures exactly. The central block gives 107 / 1,121 / 1,022 against 107 / 1,110 / 1,023: the probe
  printed clearances to four decimals, so values near 0.005 round across the threshold.
- **Walking.** The meter's walking unit-tick also counts a body that proposes a step but does not move (plan §6.1).
  On the default seed that is 9,205 walking unit-ticks against the probe's 8,992 moving ones; under the probe's own
  definition its `ticks.csv` gives exactly 8,992 and 276 below 0.005. Under the meter's: 323 below 0.005 and 1,066
  within the margin. In the block over nine seeds this adds about 1,430 touching unit-ticks (3,684 against 2,250),
  bodies held at the face in a queue. The evidence's "1,102 inside the margin" on the default seed is not reproduced
  by the probe's stored data under its stated definition (1,016); the meter's 1,066 is the figure of record.
- **Cutting route updates.** M-5a on the default seed is 4: unit 22 at tick 10 (sampled depth −0.0048), and unit 21
  at ticks 687 (−0.3075), 694 (−0.0005) and 695 (−0.3043). The probe's `routes.csv` lists the same four below
  −2⁻¹⁴; the evidence counted the two deep ones. Seeds 1 and 6 give 3 each (the deep pairs and tick 10). **Case D is
  a chase**, not a walk: unit 21 has an acquired target. WP1 reaches it (`chase` calls `drop_passed_waypoints`).
- **Held cutting legs (new).** The opening of every showcase seed has two long M-5b walk episodes: unit 21 (player
  1), ticks 38 to 159 (122 ticks, the 22-unit leg along the ramp E side wall, least clearance 0.027) and rhino
  beetle 13 (player 0), ticks 60 to 126 (67 ticks: case A). Both are far above WP2's 19-tick bound.

### Base figures

`showcase.ron`, nine seeds. M-1 and M-2 per region in the order ramp W / ramp E / block / elsewhere.

| Seed | Ticks, outcome | M-1 | M-2 | M-3 whole (proposal) | M-4 whole | Longest episode (unit, last tick) | M-5a | M-5b walk ticks (longest) | M-5b chase ticks (longest) | Deepest |
|---|---|---|---|---|---:|---|---:|---|---|---|
| default | 2228, player 1 wins | 37 / 24 / 181 / 3 | 191 / 186 / 529 / 160 | 24 (0) | 220.0 | 92 (21, 777) | 4 | 213 (122) | 14 (9) | 2.1 × 10⁻⁶ |
| 1 | 2505, player 1 wins | 37 / 24 / 385 / 3 | 145 / 186 / 718 / 155 | 18 (0) | 361.3 | 90 (4, 518) | 3 | 213 (122) | 12 (8) | 3.7 × 10⁻¹⁰ |
| 2 | 3000, in progress | 37 / 24 / 216 / 3 | 133 / 186 / 452 / 160 | 23 (0) | 203.2 | 80 (22, 797) | 1 | 213 (122) | 3 (3) | 2.1 × 10⁻⁶ |
| 3 | 1875, player 1 wins | 37 / 24 / 459 / 3 | 169 / 186 / 721 / 160 | 29 (0) | 376.7 | 145 (8, 496) | 2 | 213 (122) | 1 (1) | 1.6 × 10⁻¹⁰ |
| 4 | 1912, player 1 wins | 37 / 24 / 444 / 3 | 161 / 186 / 682 / 155 | 62 (0) | 376.6 | 142 (7, 453) | 1 | 213 (122) | 0 | 1.6 × 10⁻¹⁰ |
| 5 | 2243, player 1 wins | 37 / 24 / 403 / 3 | 135 / 186 / 717 / 162 | 24 (0) | 383.0 | 161 (7, 472) | 1 | 213 (122) | 0 | 1.6 × 10⁻¹⁰ |
| 6 | 1943, player 1 wins | 37 / 24 / 447 / 3 | 153 / 186 / 743 / 160 | 22 (0) | 380.9 | 92 (7, 454) | 3 | 213 (122) | 8 (8) | 2.1 × 10⁻⁹ |
| 7 | 3000, in progress | 37 / 24 / 354 / 3 | 103 / 222 / 752 / 106 | 29 (0) | 513.8 | 183 (8, 536) | 1 | 213 (122) | 0 | 1.6 × 10⁻¹⁰ |
| 8 | 3000, in progress | 37 / 24 / 326 / 3 | 138 / 186 / 790 / 173 | 29 (0) | 520.5 | 150 (8, 503) | 1 | 213 (122) | 0 | 1.6 × 10⁻¹⁰ |

Nine-seed totals (walking / M-1 / < 0.005 / < 0.01 / M-2 / M-3 / M-4): ramp W 6,438 / 333 / 452 / 494 / 1,328 / 9 /
241.1; ramp E 4,680 / 216 / 334 / 381 / 1,710 / 0 / 105.5; block 19,615 / 3,215 / 3,684 / 4,155 / 6,104 / 251 /
2,842.0; elsewhere 51,071 / 27 / 109 / 260 / 1,391 / 0 / 147.6; whole map 81,804 / 3,791 / 4,579 / 5,290 / 10,533 /
260 / 3,336.0.

`showcase_effects.ron`, nine seeds: every seed plays to tick 3,000 in progress, and the figures barely depend on the
seed. M-1 12 to 16 / 62 / 104 / 7; M-2 228 / 203 / 774 / 51; M-3 3 (3 proposal); M-4 64.9 to 65.2; longest episode 88
(unit 22, tick 472); M-5a 7; M-5b walk 152 ticks (longest 118); no chase cutting; deepest 1.1 × 10⁻⁹.

Directional sample, 33 seeds (default and 1 to 32), per-seed mean and 95 % bootstrap interval of the mean (2,000
resamples, seed 7): the base side of the paired comparisons (§6.4). Per-seed values are in `base/base_figures.csv`.

| `showcase.ron` | M-1 | M-2 | M-3 | M-4 | Longest episode |
|---|---|---|---|---|---|
| ramp W | 37.0 [37.0, 37.0] | 144.3 [137.8, 150.8] | 1.0 [1.0, 1.0] | 26.0 [25.4, 26.7] | 47.0 [47.0, 47.0] |
| ramp E | 24.6 [24.0, 25.7] | 192.3 [187.0, 199.1] | 0.1 [0.0, 0.4] | 12.4 [11.5, 13.9] | 29.7 [29.0, 31.0] |
| block | 352.3 [323.1, 379.4] | 651.2 [612.2, 689.3] | 27.9 [22.7, 34.6] | 296.5 [266.5, 326.0] | 124.2 [114.0, 134.1] |
| elsewhere | 3.0 [3.0, 3.0] | 153.5 [147.0, 159.2] | 0.0 | 16.2 [14.4, 17.8] | 27.3 [26.3, 28.6] |
| whole map | 417.0 [387.5, 443.9] | 1,141.3 [1,099.8, 1,181.5] | 29.0 [23.8, 35.7] | 351.1 [321.2, 380.2] | 124.2 [114.0, 134.1] |

`showcase.ron`: M-5a mean 1.42 [1.18, 1.70], at least one on every seed (unit 22 at tick 10, in the deterministic
opening); M-5b longest walk episode 122 on every seed.

| `showcase_effects.ron` | M-1 | M-2 | M-3 | M-4 | Longest episode |
|---|---|---|---|---|---|
| ramp W | 13.1 [12.6, 13.7] | 228.0 | 0.0 | 22.7 [22.6, 22.7] | 32.0 |
| ramp E | 62.0 | 203.0 | 0.0 | 19.4 | 34.0 |
| block | 104.0 | 774.0 | 0.0 | 17.8 | 88.0 |
| elsewhere | 7.0 | 51.0 | 3.0 | 5.1 | 21.0 |
| whole map | 186.1 [185.6, 186.7] | 1,256.0 | 3.0 | 65.0 [64.9, 65.0] | 88.0 |

`showcase_effects.ron`: M-5a 7 and M-5b longest walk 118 on every seed.

**Findings for the later packages.**
- All 260 M-3 reversals of `showcase.ron`'s nine seeds are contact-caused; none is proposal-caused.
- `showcase_effects.ron` is almost seed-insensitive over its first 3,000 ticks: its paired differences will have
  next to no spread, so a directional claim there rests on one trajectory. The plan's paired test (§6.4) stays as
  written; the reader should know this.
- Both showcase scenarios play on the same map (`maps/showcase_canyon.scmap.ron`), which is also the throughput
  benchmark's canyon. WP2's property test therefore covers that map plus this plan's two scratch maps.
- H-W1 holds on the base: the deepest body is 2.1 × 10⁻⁶, below 2.4 × 10⁻⁵.

### Red tests (written on the plan's branch, ignored until window W)

In `route_following_tests.rs`, each `#[ignore]`d with its package and window, so no gate runs them. Their run on the
base (`cargo test -p sim_core --test route_following_tests -- --include-ignored`):

| Test | Package | Base | Why it fails |
|---|---|---|---|
| `a_body_resting_on_a_wall_at_rounding_depth_keeps_the_arc_round_the_corner` | WP1 | **red** | The arc (16.20, 16.37), (16.33, 16.27) is handed off on the first tick; the route is the goal alone |
| `a_wedged_body_hands_off_only_along_a_leg_its_fitting_disc_can_walk` | WP1 | **red** | Tick 2: from (15.88, 16.275) the leg to the goal (24.5, 6.5) goes deeper |
| `a_body_pushed_off_its_leg_replans_before_it_reaches_the_corner` | WP2 | **red** | The cutting route is held for 162 ticks (bound 8) |
| `a_refused_step_whose_leg_the_disc_cannot_walk_drops_the_route` | WP2 | **red** | The cutting route is kept after the refused step |
| `a_body_pushed_into_a_narrow_gap_keeps_a_route_it_can_follow` | WP2 | green | The intent WP2 must keep: holds on the base |
| `a_fresh_route_passes_its_own_held_leg_test` | WP2 | green | Every fresh first leg (showcase map, corner and block maps; radii 0.375 and 0.5; tangent, 2⁻²², 0.05 and 0.1 deep) passes the step rule on the base: no re-search loop is expected |

The step rule is mirrored from public predicates in the test file (`step_rule_permits`), since `probe_permits` is
crate-private.

### Cost attribution (plan §6.7)

Callgrind (`--dump-instr=yes`) on the base's canyon throughput scenario alone (400 pulse bots, 300 ticks; the
benchmark built with line tables, `CARGO_PROFILE_BENCH_DEBUG=line-tables-only`, from a clean export of `c49082af`).
27.37 G instructions in all. `route_is_usable` is inlined into `upkeep_route`, so it is attributed by source line.

| Item | Inclusive Ir | Share | Calls |
|---|---:|---:|---|
| `route_is_usable` (its two `segment_is_passable` lines) | 1,915 M | 7.0 % | about 12,800 route checks; 12,769 wedged tests and 144,730 leg tests: **11.3 legs per checked route**, 12,575 Ir per leg test |
| `drop_passed_waypoints` | 77 M | 0.28 % | 109,611 (one per walking draft); 3,114 leg and 1,938 wedged segment tests |
| `settle_on_terrain` | 4,174 M | 15.25 % | 1,668,677 (1,559,234 from the solver's settled step) |
| of which `clearance` (its `wall_contact`) | 3,406 M | 12.4 % | two per settle |
| all `wall_contact` | 3,708 M | 13.55 % | 3,449,837, about 1,075 Ir each |
| all `segment_is_passable` | 2,391 M | 8.73 % | 307,798, about 7,770 Ir each |
| `probe_permits` | 550 M | 2.0 % | 109,605 from `propose_step`, 22,085 from the slide view check |

Predictions this gives (each package restates its own before measuring):
- **WP1:** none beyond noise. `drop_passed_waypoints` is 0.28 %, and `probe_permits` asks the same first query as
  today's test.
- **WP2:** one disc segment test of the held leg per route check, about 12,800 × 12,600 Ir ≈ 0.16 G, **+0.6 %**,
  plus one per refused step (2,125 slides in the run, negligible), plus the re-searches of dropped routes (a search
  costs about 1 M Ir: `search_route` is 4.5 G over the run).
- **WP3:** one `wall_contact` at `r + m` per walking unit-tick, about 109,600 × 1,100 Ir ≈ 0.12 G, **+0.4 %**, unless
  shared with the settle.

### B-243 (mesher check)

See the hand-back of WP0 and the `DESIGN.md` §4 row, if filed.
