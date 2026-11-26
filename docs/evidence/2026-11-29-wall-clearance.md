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

### B-243 (mesher check): confirmed and filed

A scratch program (`~/scratch/wall-clearance/mesher_check/`, never committed; source kept as
`scripts/mesher_check_main.rs`) links the client crate of `c49082af` and runs its public CPU mesher functions,
`dual_corner_levels` and `generate_wall_profile`, over every dual cell and threshold run of
`maps/showcase_canyon.scmap.ron` as `emit_dual_cell` and `emit_cliff_walls` do (edges with a ramp corner are left
out: their rows sit at the edge, with no jitter). No frame was rendered. For each profile row with the horizontal
normal `d` (the canonical rows, and plain wall bottoms at the edge itself) it measures the row's offset from the cell
edge along `d`, which points from the high cell to the low one, the side a body at the cliff's foot walks on.

Of 11,866 such rows, 5,501 are moved towards the low side; the largest offset is **0.0600** (edge (38.0, 34.5),
height 4.50), and the largest towards the high side 0.0599. The jitter is symmetric, as `WALL_JITTER_AMPLITUDE ·
(2h − 1)` says. So a body the simulation holds tangent to a cliff is drawn up to 0.06 inside the cliff's face.
B-243 is filed in `DESIGN.md` §4 as a deferred client follow-up, outside this plan (§11.1, §13).

### Crowd scenes (Amendment 1, the owner's answer to Q0134)

`sim_fixtures::trace::crowd`, run with `crowd_trace` (release) on seeds 1 to 8: 6 geometries × 3 group sizes × 8 seeds
= 144 scenes per arm, about 30 s an arm. Arms as `Gated` §6.1: **S** the shipped rules, **R** `--pipeline solver
--crowd-iterations 151`, **L** `--pipeline legacy`. Outputs `~/scratch/wall-clearance/crowd/<arm>/crowd.csv` (one row
per scene) and `crowd.md`. Each line below sums its 8 scenes; "Longest episode", the M-5b longest and "Deepest" are
the largest over them. Reported, not judged.

Known answers, every arm: **every move ends by the bound, none given up**, in 144 of 144 scenes; **no body deeper
than 2.4 × 10⁻⁵**, 144 of 144 (deepest 3.6 × 10⁻⁶, S, choke 2, 48 units). The first draft's answer "every unit ends
within 0.5 of its goal" did not hold and was withdrawn before the runs of record: every unit that ends outside 0.5 has
no order left, its move ended against friends resting at or near its goal (the arrival rule), so those are counted as
"stopped short", a figure.

Findings: held cutting walk legs (M-5b) last far beyond WP2's 19-tick bound in every geometry with 24 or more units
(up to 130 ticks); cutting route updates (M-5a) occur in every geometry; S and R differ only in the larger groups;
L leaves fewer units short but more M-2 and M-4 at the corner, hairpin and staircase.

**S**

| Geometry | Size | Scenes | Every move ends | Depth ok | Arrived (stopped short) | Walking | M-1 | M-2 | Longest episode | M-3 (proposal) | M-4 | M-5a | M-5b walk ticks (longest) | Deepest |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---|---:|---:|---|---|
| choke2 | 12 | 8 | 8 | 8 | 91 (5) | 20110 | 823 | 2390 | 71 | 28 (2) | 296.6 | 30 | 693 (44) | 3.6e-9 |
| choke2 | 24 | 8 | 8 | 8 | 136 (56) | 43648 | 2391 | 5362 | 126 | 172 (2) | 1201.8 | 51 | 2007 (88) | 4.5e-9 |
| choke2 | 48 | 8 | 8 | 8 | 208 (176) | 98344 | 7743 | 12579 | 130 | 539 (10) | 3674.1 | 101 | 5120 (110) | 3.6e-6 |
| choke3 | 12 | 8 | 8 | 8 | 96 (0) | 19307 | 260 | 1699 | 34 | 0 (0) | 21.2 | 8 | 145 (92) | 7.4e-10 |
| choke3 | 24 | 8 | 8 | 8 | 176 (16) | 40057 | 1345 | 3407 | 66 | 47 (1) | 458.1 | 25 | 686 (43) | 3.8e-9 |
| choke3 | 48 | 8 | 8 | 8 | 273 (111) | 87111 | 4418 | 7431 | 109 | 213 (1) | 1753.7 | 75 | 2885 (72) | 6.3e-7 |
| choke5 | 12 | 8 | 8 | 8 | 96 (0) | 19272 | 0 | 414 | 30 | 0 (0) | 0.1 | 1 | 91 (91) | 0.0e0 |
| choke5 | 24 | 8 | 8 | 8 | 192 (0) | 38704 | 636 | 2274 | 38 | 0 (0) | 71.1 | 34 | 207 (24) | 1.5e-9 |
| choke5 | 48 | 8 | 8 | 8 | 347 (37) | 80168 | 2553 | 4425 | 70 | 59 (1) | 712.0 | 48 | 1024 (37) | 3.4e-9 |
| corner90 | 12 | 8 | 8 | 8 | 96 (0) | 22251 | 245 | 433 | 20 | 0 (0) | 66.9 | 13 | 292 (19) | 6.3e-10 |
| corner90 | 24 | 8 | 8 | 8 | 171 (21) | 46259 | 795 | 1108 | 45 | 9 (0) | 336.2 | 28 | 979 (39) | 2.1e-9 |
| corner90 | 48 | 8 | 8 | 8 | 326 (58) | 96172 | 2094 | 2673 | 95 | 31 (0) | 997.0 | 71 | 1762 (83) | 3.6e-9 |
| hairpin | 12 | 8 | 8 | 8 | 59 (37) | 24473 | 719 | 1457 | 48 | 0 (0) | 317.8 | 40 | 421 (28) | 1.5e-9 |
| hairpin | 24 | 8 | 8 | 8 | 98 (94) | 52120 | 1707 | 2593 | 83 | 20 (4) | 783.0 | 102 | 1169 (58) | 9.1e-7 |
| hairpin | 48 | 8 | 8 | 8 | 187 (197) | 112707 | 4121 | 5410 | 118 | 45 (1) | 2142.8 | 176 | 2435 (106) | 3.2e-7 |
| staircase45 | 12 | 8 | 8 | 8 | 95 (1) | 23973 | 86 | 1043 | 28 | 0 (0) | 82.7 | 8 | 88 (9) | 4.5e-10 |
| staircase45 | 24 | 8 | 8 | 8 | 180 (12) | 48509 | 911 | 1822 | 28 | 11 (0) | 410.2 | 71 | 704 (21) | 2.0e-9 |
| staircase45 | 48 | 8 | 8 | 8 | 276 (108) | 101158 | 2377 | 3855 | 72 | 108 (1) | 1539.1 | 121 | 2085 (62) | 3.2e-9 |

**R**

| Geometry | Size | Scenes | Every move ends | Depth ok | Arrived (stopped short) | Walking | M-1 | M-2 | Longest episode | M-3 (proposal) | M-4 | M-5a | M-5b walk ticks (longest) | Deepest |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---|---:|---:|---|---|
| choke2 | 12 | 8 | 8 | 8 | 91 (5) | 20110 | 823 | 2390 | 71 | 28 (2) | 296.6 | 30 | 693 (44) | 3.6e-9 |
| choke2 | 24 | 8 | 8 | 8 | 137 (55) | 43360 | 2324 | 5290 | 103 | 193 (4) | 1158.4 | 44 | 2023 (66) | 4.5e-9 |
| choke2 | 48 | 8 | 8 | 8 | 231 (153) | 100773 | 7706 | 12911 | 164 | 630 (16) | 4176.2 | 95 | 5911 (130) | 2.5e-6 |
| choke3 | 12 | 8 | 8 | 8 | 96 (0) | 19307 | 260 | 1699 | 34 | 0 (0) | 21.2 | 8 | 145 (92) | 7.4e-10 |
| choke3 | 24 | 8 | 8 | 8 | 176 (16) | 40055 | 1315 | 3399 | 66 | 51 (1) | 454.4 | 24 | 695 (43) | 4.1e-9 |
| choke3 | 48 | 8 | 8 | 8 | 279 (105) | 86967 | 4623 | 7589 | 105 | 242 (6) | 1798.6 | 76 | 2999 (77) | 7.3e-7 |
| choke5 | 12 | 8 | 8 | 8 | 96 (0) | 19272 | 0 | 414 | 30 | 0 (0) | 0.1 | 1 | 91 (91) | 0.0e0 |
| choke5 | 24 | 8 | 8 | 8 | 192 (0) | 38704 | 636 | 2274 | 38 | 0 (0) | 71.1 | 34 | 207 (24) | 1.5e-9 |
| choke5 | 48 | 8 | 8 | 8 | 347 (37) | 80168 | 2553 | 4425 | 70 | 59 (1) | 712.0 | 48 | 1024 (37) | 3.4e-9 |
| corner90 | 12 | 8 | 8 | 8 | 96 (0) | 22251 | 245 | 433 | 20 | 0 (0) | 66.9 | 13 | 292 (19) | 6.3e-10 |
| corner90 | 24 | 8 | 8 | 8 | 172 (20) | 46270 | 797 | 1101 | 49 | 15 (2) | 339.3 | 26 | 954 (44) | 2.1e-9 |
| corner90 | 48 | 8 | 8 | 8 | 321 (63) | 96474 | 2039 | 2648 | 107 | 29 (0) | 987.6 | 66 | 1716 (84) | 3.6e-9 |
| hairpin | 12 | 8 | 8 | 8 | 59 (37) | 24473 | 719 | 1457 | 48 | 0 (0) | 317.8 | 40 | 421 (28) | 1.5e-9 |
| hairpin | 24 | 8 | 8 | 8 | 102 (90) | 51997 | 1717 | 2695 | 77 | 17 (2) | 819.2 | 101 | 1197 (53) | 9.1e-7 |
| hairpin | 48 | 8 | 8 | 8 | 179 (205) | 114011 | 4109 | 5542 | 129 | 52 (1) | 2304.7 | 187 | 2482 (113) | 1.3e-6 |
| staircase45 | 12 | 8 | 8 | 8 | 95 (1) | 23973 | 86 | 1043 | 28 | 0 (0) | 82.7 | 8 | 88 (9) | 4.5e-10 |
| staircase45 | 24 | 8 | 8 | 8 | 180 (12) | 48509 | 911 | 1822 | 28 | 11 (0) | 410.2 | 71 | 704 (21) | 2.0e-9 |
| staircase45 | 48 | 8 | 8 | 8 | 276 (108) | 101124 | 2383 | 3855 | 72 | 116 (1) | 1533.2 | 119 | 2054 (62) | 3.2e-9 |

**L**

| Geometry | Size | Scenes | Every move ends | Depth ok | Arrived (stopped short) | Walking | M-1 | M-2 | Longest episode | M-3 (proposal) | M-4 | M-5a | M-5b walk ticks (longest) | Deepest |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---|---:|---:|---|---|
| choke2 | 12 | 8 | 8 | 8 | 96 (0) | 19990 | 840 | 2389 | 47 | 2 (1) | 185.1 | 17 | 542 (25) | 8.3e-7 |
| choke2 | 24 | 8 | 8 | 8 | 189 (3) | 44395 | 2013 | 5450 | 62 | 28 (5) | 920.4 | 39 | 2595 (58) | 2.7e-10 |
| choke2 | 48 | 8 | 8 | 8 | 370 (14) | 104932 | 5850 | 11503 | 71 | 136 (8) | 3087.5 | 84 | 9589 (75) | 5.9e-7 |
| choke3 | 12 | 8 | 8 | 8 | 96 (0) | 19413 | 432 | 1719 | 37 | 0 (0) | 53.2 | 4 | 114 (12) | 1.6e-10 |
| choke3 | 24 | 8 | 8 | 8 | 192 (0) | 41123 | 880 | 3335 | 64 | 10 (0) | 395.3 | 22 | 1001 (53) | 7.1e-8 |
| choke3 | 48 | 8 | 8 | 8 | 381 (3) | 92304 | 2796 | 6859 | 73 | 48 (1) | 1488.4 | 39 | 3665 (77) | 3.1e-10 |
| choke5 | 12 | 8 | 8 | 8 | 96 (0) | 19200 | 0 | 397 | 29 | 0 (0) | 0.0 | 0 | 0 (0) | 0.0e0 |
| choke5 | 24 | 8 | 8 | 8 | 192 (0) | 39075 | 682 | 2387 | 40 | 0 (0) | 101.5 | 8 | 242 (24) | 1.8e-10 |
| choke5 | 48 | 8 | 8 | 8 | 378 (6) | 83198 | 2069 | 4966 | 67 | 27 (0) | 810.3 | 31 | 1682 (59) | 2.4e-10 |
| corner90 | 12 | 8 | 8 | 8 | 96 (0) | 23256 | 304 | 642 | 33 | 0 (0) | 273.8 | 9 | 524 (39) | 1.1e-10 |
| corner90 | 24 | 8 | 8 | 8 | 188 (4) | 51262 | 1009 | 1654 | 65 | 0 (0) | 821.9 | 27 | 1440 (73) | 2.3e-10 |
| corner90 | 48 | 8 | 8 | 8 | 352 (32) | 114397 | 2993 | 4406 | 98 | 16 (0) | 2290.7 | 58 | 3365 (75) | 1.2e-7 |
| hairpin | 12 | 8 | 8 | 8 | 85 (11) | 26642 | 527 | 1777 | 56 | 4 (1) | 561.1 | 13 | 581 (53) | 2.5e-10 |
| hairpin | 24 | 8 | 8 | 8 | 145 (47) | 61869 | 1899 | 3974 | 85 | 32 (3) | 1815.2 | 31 | 1972 (76) | 6.3e-7 |
| hairpin | 48 | 8 | 8 | 8 | 277 (107) | 141084 | 5932 | 9728 | 90 | 142 (1) | 5227.0 | 107 | 4729 (93) | 3.7e-7 |
| staircase45 | 12 | 8 | 8 | 8 | 96 (0) | 24458 | 115 | 1159 | 28 | 3 (0) | 191.2 | 3 | 217 (25) | 2.2e-10 |
| staircase45 | 24 | 8 | 8 | 8 | 171 (21) | 52651 | 812 | 2486 | 46 | 14 (2) | 913.2 | 48 | 1238 (47) | 2.6e-10 |
| staircase45 | 48 | 8 | 8 | 8 | 327 (57) | 113508 | 2528 | 5455 | 77 | 64 (11) | 2526.8 | 78 | 3179 (72) | 3.9e-10 |

### Left to the orchestrator

- **Moved worlds since `Gated`'s last re-base** (plan §5.3 item 1, §8 WP0 "Base figures"): not measured here. It
  needs `Gated`'s WP0b scripts and arms, and `Gated` WP1 was running its own heavy measurements; whether `Gated`
  recorded the Broodlings merge `0f0608cd` is to be verified with its orchestrator first.

## Window W: the base re-checked

**Tree.** Branch `wall-clearance-plan` at `c46b080d`: WP0 plus `main` `a5a3df79` (`Gated` WP1 merged latent; schema
61). Before any change of window W, the base binaries were built from a clean export of `c46b080d`
(`~/scratch/wall-clearance/trees/wcb`) and the meter re-run exactly as WP0 ran it (both showcase scenarios, default
seed and seeds 1 to 32, 3,000 ticks; crowd scenes under S, R and L, seeds 1 to 8). Against WP0's outputs on
`c49082af`: **726 of 726** meter and trace files byte-identical (the 66 standard outputs differ only in the output
path they print) and the three crowd tables identical. `Gated` WP1 moved no world the meter reads, so WP0's base
figures stand for window W (`window/base_recheck.txt`).

**Instruments for the window** (`~/scratch/wall-clearance/scripts/`):
- `build_tree.sh`: exports a tree and builds it with `Gated`'s own `build.sh` (by path), plus `crowd_trace`;
- `meter.sh`: the meter runs above; `compare.py`, `crowd_compare.py`, `matrix_diff.py`: the tables below;
- `measure_wc.sh`: the command lines of `Gated`'s WP0b `measure.sh`, parameterised by binary tag and output
  directory, which that script hard-codes (arm assets `~/scratch/gated-movement/assets/{S,R,L,G}-wc`, edited as
  `Gated`'s are);
- `trace_patch.py`: a scratch build of the base (`trees/wctr`, never committed) that keeps the base rules, prints
  one `WCTRACE` line wherever WP1's or WP2's rule would decide otherwise, and with `WCTRACE_APPLY=1` (WP1) or `=2`
  (WP1 and WP2) applies the new rules instead. It is the first-difference trace and the wiring proof (H-W4) in
  one: with the flag off it reproduces the base byte for byte, with it on the package's tree.

## WP1: the hand-off's view is the step's view (B-240)

**Code** (`crates/sim_core/src/movement/locomotion.rs`). `drop_passed_waypoints` passes a corner only when
`probe_permits` lets the body walk the leg to the next point (plan D-1). The wedged escape
`!segment_is_passable(p, p)` and its comment are gone; the function's doc cites the plan, contact D-3 and
`WALL_PROBE_SLACK` (§4.6). The function's route request parameter, which only the removed test read, is removed,
so its three callers lose one argument (one of them in `locomotion/follow.rs`; see "Deviations").

**Red runs** (`~/scratch/wall-clearance/red/window-base-c46b080d.log`, `cargo test -p sim_core --test
route_following_tests -- --include-ignored` on the unchanged `c46b080d`): both WP1 tests fail as WP0 recorded (case
D: the arc handed off on the first tick, line 671; the wedged body: a deeper leg on tick 2, line 714), both WP2
red tests fail, the two guard tests pass. With WP1 both WP1 tests pass, and
`a_unit_pushed_past_its_corner_drops_it_whoever_stands_on_the_next_leg` and `a_column_rounds_a_shared_corner_directly`
pass unchanged.

**One test assertion aligned with the plan.** WP0's case-D test also asked that the leg from the body to the kept
arc point pass the step rule. It does not, and should not be asked to: the funnel places that point 0.425 from the
vertex, 0.0023 below the face's tangent line, so from a body tangent to the face the leg dips about 6 × 10⁻⁴ into
the radius; the step then slides along the face (achieved step (0.126, 0)) and from tick 3 the leg is walkable, the
corner is rounded and no step reverses in 40 ticks. Plan §8 WP1 specifies the test as "the arc is not handed off;
the unit's next aim is the arc point; over the following ticks no step reverses". The assertion now checks that
the step rule refuses the leg *past* the kept point, which is why it is kept, and the doc says why the leg to the
point itself is not asked.

**Goldens** (§6.5). `golden_replay` first differs at checkpoint index 3 (ticks 151 to 200): on tick 151 rhino beetle
15 at (30.24, 29.71), at the foot of the default arena's ramp, keeps its corner (30.42, 29.96), which the base
handed off on the wedged escape. `golden_replay_orders` (16-unit script) first differs at checkpoint 46 (ticks 461 to 470): on tick
469 rhino beetle 26 at (33.98, 34.50), tangent to the lip at y = 34, keeps its arc (33.47, 34.15); in the 26-unit
script below, the rule first acts on tick 474 (pulse bot 30 at (33.80, 34.31)).
- **The orders fixture lost its mid-route repath.** Its only remaining one on the base (tick 515, rhino 27) was B-240
  itself: rhino 27, tangent to the lip, handed its arc off on the wedged escape (tick 512), was refused, dropped the
  route and was re-searched. With WP1 no unit's route empties mid-order, and the test's evidence check fails. The
  script's `blue_column` was raised from 16 to 26 pulse bots (four rows of eight, the same origin), chosen from a
  scripted search of eleven variants (`trace/orders_grid.log`) as the one in which a mid-route repath occurs under
  the base rules, WP1 and WP1 with WP2 (base: tick 511, entity 14; WP1: the same; WP2: tick 525, entity 37). Both
  situations of the fixture are found again; the phase-order mutation runs of WP5 were not repeated.
- **Proof.** The scratch build with the rule off (`WCTRACE_APPLY=0`) reproduces the previous checksums of both
  fixtures at every checkpoint (`trace/golden-apply0.log`); with the rule on it reproduces the re-recorded ones
  (`wp1/golden-proof-apply1.txt`). `DEFAULT_SANDBOX_CHECKSUM` does not move (`fixture_tests` green).

**First differences on the showcase** (the scratch build, rule off; `trace/sc/`). `showcase.ron`: default seed tick
688, pulse bot 21 at (25.94, 35.37) keeps its arc round the block's corner (26, 35) (predicted 686; the evidence's
route update at 687 is the same hand-off, read by the meter after the tick); seed 1 tick 718, unit 25 (predicted
717); seed 6 tick 631, unit 23 (predicted 630); and, not predicted, seed 3 tick 640 (unit 23, the same corner) and
seed 5 tick 1012: pulse bot 3 at (7.48, 5.69), touching the spawn exit's wall, is handed its corner (8.0, 5.64) along
the wall, which the base refused because its disc at the full radius did not fit (D-1's "a pushed-past body that
touches the wall is tested with its own disc less the slack"). Seeds 2, 4, 7 and 8 do not move. `showcase_effects.ron`:
tick 192 on every seed, pulse bot 18 at the ramp E foot corner (35, 52), tangent, keeps its arc.

**Showcase figures** (`wp1/compare.md`; base `window/wcb`, WP1 `wp1/meter`). Nine seeds of `showcase.ron`:

| Seed | M-5a | M-5b walk ticks (longest) | M-5b chase ticks (longest) | M-1 whole | M-2 whole | M-4 whole | Longest episode | Give-ups | Outcome |
|---|---|---|---|---|---|---|---|---|---|
| default | 4 → 1 | 213 (122) → 213 (122) | 14 (9) → 1 (1) | 245 → 324 | 1,066 → 1,068 | 220.0 → 205.9 | 92 → 81 | 0 → 0 | P1, 2228 → 2228 |
| 1 | 3 → 1 | 213 (122) → 213 (122) | 12 (8) → 4 (4) | 449 → 451 | 1,204 → 1,205 | 361.3 → 356.2 | 90 → 90 | 0 → 0 | P1, 2505 → 2504 |
| 2 | 1 → 1 | unchanged | 3 (3) → 3 (3) | unchanged | | | | 0 → 0 | 3000 |
| 3 | 2 → 1 | 213 (122) → 213 (122) | 1 (1) → 0 | 523 → 511 | 1,236 → 1,222 | 376.7 → 375.0 | 145 → 145 | 0 → 0 | P1, 1875 → 1874 |
| 4, 5, 7, 8 | 1 → 1 | unchanged | | unchanged | | | | 0 → 0 | unchanged |
| 6 | 3 → 2 | 213 (122) → 213 (122) | 8 (8) → 1 (1) | 511 → 495 | 1,242 → 1,257 | 380.9 → 375.1 | 92 → 92 | 0 → 0 | P1, 1943 → 1941 |

Seed 5's trajectory moves only after tick 1012 and none of its figures changes. `showcase_effects.ron`, every seed:
M-5a 7 → 2, M-5b walk 152 (118) → 139 (118), M-1 whole ≈ 186 → 168, M-2 1,256 → 1,261, M-4 65.0 → 62.7, longest
episode 88 → 81. H-W1: the deepest body 2.1 × 10⁻⁶ (unchanged). H-W2: the overlap meter's deeper unit-ticks equal
the base's on every seed (1 on the default and seed 2, from tick 363 before any difference; 0 elsewhere). H-W6: no
give-up on any seed.

**Target M-5a = 0: not met as counted.** Every cutting route update of case D (the 0.31-deep legs) is gone, and no
hand-off any more yields a leg that cuts: the scratch build counts, at decision time, hand-offs whose leg fails the
disc at `r − 2⁻¹⁴`, and finds **0** on all 18 showcase runs. M-5a still counts 1 per seed of `showcase.ron` (2 on seed 6)
and 2 per seed of `showcase_effects.ron`, each traced (`trace/r/`):
- `showcase.ron` tick 10, pulse bot 22 (every seed; sampled depth 0.0048): handed off on tick 11 from (57.38, 53.05),
  clearance 0.16, along a leg its full disc fits; the meter reads the leg from the position after the tick's step
  (57.27, 53.02), from which it dips 0.0048;
- seed 6 tick 631, pulse bot 23 (a chase; depth 0.0010): not a hand-off by view but `pass_reached_waypoint`, the step
  landing exactly on the arc point, after which the next arc leg dips 0.0010 from the tangent body;
- `showcase_effects.ron` tick 204, pulse bot 19 (depth below the sampling step) and tick 371, pulse bot 21 (clearance
  0.43, depth 0.0024): hand-offs whose leg was walkable from where the body stood, read after the step.

The plan set the target when the base was thought to hold only case D's pairs (2 on the default seed and seeds 1
and 6); WP0 found M-5a also counts these shallow updates (4, 3, 3 and the tick-10 one on every seed). They are not
B-240: no body at rounding depth hands off a cutting leg. Reported for the orchestrator's judgement against plan §7
rule 2; no rule was changed for it.

**33 paired seeds** (§6.4, "not worse beyond noise"): on `showcase.ron` every region's M-1 to M-4 and longest
episode lie within the base's 95 % interval. `showcase_effects.ron` is seed-insensitive (WP0's finding), so its
intervals have zero width and any change is "beyond noise": ramp W M-1 13.1 → 14.0, M-2 228 → 231, M-4 22.7 →
28.5, longest episode 32 → 41 (railgun crawler 24's 41-tick episode at ticks 584 to 624, a knock-on of the order
in which player 1's column reaches the ramp after the tick-192 difference), block M-2 774 → 780; better: ramp E
M-1, M-2, M-4, block M-1 104 → 87, M-4 17.8 → 11.5, whole-map M-1, M-4, longest episode.

**Route boxes** (default seed): unchanged except ramp W foot, owner 1: ratio 1.510 / 1.517 → 1.510 / 1.516, turn
110° / 115° → 112° / 117°. Reversals per owner on the default seed: [64, 121] → [64, 120].

**Crowd scenes** (`wp1/crowd_compare.md`; Amendment 1). Both known answers hold in 144 of 144 scenes under each arm
(every move ends, none given up; deepest 8.8 × 10⁻⁷). M-5a falls in every geometry and size (S: choke2 30 / 51 /
101 → 12 / 24 / 40, corner90 13 / 28 / 71 → 0 / 10 / 14, hairpin 40 / 102 / 176 → 15 / 20 / 51, staircase45 8 / 71 /
121 → 6 / 14 / 43; R and L alike). **Cutting hand-offs from rounding depth are gone**: the scratch build counts, over
the 144 scenes, 1,586 (S), 1,563 (R) and 1,138 (L) hand-offs on the base whose leg fails the disc at `r − 2⁻¹⁴` from
where the body stands (1,278, 1,261 and 435 of them by a body within 2⁻¹⁴ of the wall), and **0** under WP1 in
every arm (`trace/crowd*/`). The remaining M-5a are of the kinds traced on the showcase (fresh routes, group lane
joins, landing passes, legs read after the step). M-5b walk ticks fall in 16 of 18 cells (S). "Not worse beyond
noise" per geometry and size (8 seeds, the base's 95 % interval of the mean): under S nine of 108 cell-figures lie
above it (M-3 at corner90 12 and 24 and hairpin 12, 0.0 → 0.2 to 1.1 → 2.1 reversals a scene; M-2 at choke3 48 and
hairpin 24; M-4 at hairpin 48 and staircase45 24) and five below; under R eight above and three below; under L
eight above (M-3 at choke2 24, corner90 24, hairpin 12 and 48; hairpin 48 stopped short 13.4 → 15.5) and eight
below. Reported for the orchestrator's judgement (many comparisons; most moves are fractions of a reversal a scene).

**Matrix** (S, 24 seeds, `wp1/matrix_S.md`). One battle moved: **40** (sieged tanks above a ramp, Class D), 7 of 24
seeds, no winner flipped (red wins every one), give-ups 14 → 14, expectation failures 2 / 32 → 2 / 32. First
difference (seed of record 3,203,386,110): tick 43, unit 8 at (43.51, 47.08) on the ramp keeps its corner (43.85,
46.47); the battle ends at tick 296 instead of 350, red with 3 survivors instead of 2.

**H-W4.** The scratch build with WP1's rule on reproduces WP1's tree byte for byte on the 18 showcase runs and the
three crowd tables (`trace/r/`, `trace/crowd/`), and with the rule off the base's (`trace/sc/`, `trace/crowd0/`).

**Cost** (prediction, §6.7): none beyond noise; measured with window W's throughput below.
