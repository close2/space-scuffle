# StarCraft II-style pathfinding: measurements per step

This file records the measurements of plan [`2026-11-09-sc2-pathfinding`](../plans/2026-11-09-sc2-pathfinding.md)
(D-11, D-12): the showcase route metrics of §3.1, the whole matchup matrix and battle 33 compared with the step
before, and the golden replays. WP1 creates it with the baseline; each later package appends its own section in the
commit that makes the change.

## How to reproduce

Every tool is a release build of an example of `sim_core`. The binaries are copied once per step, so a later edit
cannot change a running measurement.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/path-a
cargo build --release --locked -p sim_core --example route_trace --example matchup_trace
# Showcase battle: units.csv (every mobile unit per tick), passes.csv (each unit's first pass through
# each box) and summary.md (per box and owner, mean and maximum walked/chord and total turn; outcome).
$CARGO_TARGET_DIR/release/examples/route_trace --scenario scenarios/showcase.ron --ticks 1500 \
    --out ~/scratch/path-a/route/<step>
# Whole matrix: the seeds of record plus 16 more; outcomes only.
$CARGO_TARGET_DIR/release/examples/matchup_trace --all-battles --seeds 16 --jobs 8 \
    --out ~/scratch/path-a/<step>/matrix --compare ~/scratch/path-a/<previous step>/matrix/matrix.csv
# Battle 33: the four seeds of record plus 64 more, 68 seeds; seeds.csv is compared with diff.
$CARGO_TARGET_DIR/release/examples/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 \
    --no-unit-csv --out ~/scratch/path-a/<step>/b33
```

`route_trace` (`crates/sim_core/examples/route_trace.rs`, metrics in `crates/sim_fixtures/src/trace/route.rs`)
starts the scenario with `sim_core::scenario::load_scenario_battle`, which applies the scenario's initial orders
through `apply_initial_orders`, the function the client's `BattleDefinition::instantiate` now calls. It steps the
world with empty bundles until the match ends or the tick limit. Its default boxes are those of plan §3.1, in world
units, bounds inclusive: `ramp_w_foot` x 26–34, y 7–14; `plateau_se` x 35–41, y 21–29; `ramp_e_foot` x 31–39,
y 49–57. The heading is sampled every 3 ticks.

The stop rule of D-12 is applied to player 0's means, the rows of plan §3.6.1. Player 1's rows are recorded too.

## WP1: baseline (`68d6fc1`)

The harness reproduces the planning probe of §3.2, §3.3 and §3.6.1 to the rounding of the last digit.

| Box | Owner | Units | Ratio mean / max | Turn mean / max |
|---|---:|---:|---|---|
| ramp_w_foot | 0 | 10 | 1.196 / 1.383 | 324° / 840° |
| ramp_w_foot | 1 | 10 | 1.369 / 1.380 | 91° / 99° |
| plateau_se | 0 | 10 | 1.147 / 1.500 | 250° / 1070° |
| ramp_e_foot | 0 | 10 | 1.551 / 1.591 | 114° / 196° |
| ramp_e_foot | 1 | 10 | 1.283 / 1.473 | 235° / 446° |

Outcome: player 0 wins at tick 1,369, when player 1's base falls (plan D-11 gave about 1,225 from the video; the
trace measures the tick the match ends).

Per unit, the rows of §3.2 and §3.3 (`passes.csv`):

| Box | Unit | Walked | Chord | Ratio | Total turn | First-waypoint variants |
|---|---|---:|---:|---:|---:|---:|
| ramp_w_foot | 5, `pulse_bot` | 9.49 | 6.86 | 1.383 | 358° | 4 |
| ramp_w_foot | 8, `pulse_bot` | 9.96 | 7.52 | 1.323 | 840° | 4 |
| ramp_w_foot | 4, `pulse_bot` | 7.78 | 6.13 | 1.270 | 323° | 4 |
| ramp_w_foot | 13, `rhino_beetle` | 5.54 | 4.98 | 1.113 | 52° | 2 |
| plateau_se | 7, `pulse_bot` | 12.70 | 8.47 | 1.500 | 1,070° | 7 |
| plateau_se | 9, `pulse_bot` | 10.61 | 8.50 | 1.248 | 340° | 4 |
| plateau_se | 5, `pulse_bot` | 10.29 | 8.61 | 1.195 | 239° | 4 |
| plateau_se | 12, `rhino_beetle` | 8.69 | 8.40 | 1.035 | 31° | 2 |
| ramp_e_foot | 23, `pulse_bot` (player 1) | 11.06 | 7.51 | 1.473 | 282° | 5 |
| ramp_e_foot | 26, `rhino_beetle` (player 1) | 11.03 | 8.96 | 1.231 | 435° | 4 |

Matrix (1,000 runs: 50 battles on 20 seeds): 0 expectation failures on the seeds of record and over all seeds. This
run is the comparison base of WP2. Battle 33 on 68 seeds: `seeds.csv` kept as the comparison base.

WP1 changes no behaviour: `apply_initial_orders` is the code the client ran, moved, and the golden replays do not
start from a scenario file.

## WP2+WP3: route following leaves bodies to steering (B-99, B-100)

One commit (plan R-1). Showcase, `route_trace --ticks 1500`:

| Box | Owner | Units | Ratio mean / max | Turn mean / max |
|---|---:|---:|---|---|
| ramp_w_foot | 0 | 10 | 1.063 / 1.113 | 92° / 146° |
| ramp_w_foot | 1 | 10 | 1.399 / 1.416 | 156° / 207° |
| plateau_se | 0 | 10 | 1.045 / 1.078 | 67° / 148° |
| ramp_e_foot | 0 | 10 | 1.557 / 1.635 | 213° / 415° |
| ramp_e_foot | 1 | 10 | 1.176 / 1.216 | 107° / 222° |

Outcome: player 0 wins at tick 1,351. A1 and A2 alone give the same table: no body is parked on a showcase corner,
so A3 does not act there. These are §3.6.1's "A1 and A2 together" numbers.

The ramp E turn rose from 114° to 213°: the column reaches ramp E about 20 ticks sooner and flockmates jostle each
unit sideways by about ±15° per 3-tick sample while it heads for the corner (34.70, 52.30). Under D-12 the package
was held; the orchestrator accepted it and moved the criterion to plan `2026-11-09-friendly-blocking` (Amendment 1
of both plans).

Experiments not kept:
- A3 with D-9's literal occupant rule (any non-yielding body, moving flockmates included): ramp W 1.185 / 1.371,
  350° / 720°; plateau 1.129 / 1.236, 309° / 583°; ramp E 1.562 / 1.593, 179° / 258°; **player 1 wins** at tick
  1,464.
- Handoff only beyond the corner (no "near" alternative): ramp E 1.570 / 1.593, 189° / 230°.

Matrix (1,000 runs) and battle 33 (68 seeds): bit-identical to WP1. Goldens: checksum schema 22 (21 before the merge with WP13c), re-recorded; first
behaviour change on tick 155 of `golden_replay` and tick 120 of `golden_replay_orders` (fixture headers).

## WP4: corner offsets use the unit's own radius (B-101)

`PathRequest` and `PathRequestWithVision` gain `radius` (the unit's `collision_radius`; `route_request` passes
`policy.radius`), and `offset_corner_waypoint` offsets a corner by `radius + CORNER_SAFETY_MARGIN`. The tier still
chooses the corridor (until WP9). Showcase:

| Box | Owner | Units | Ratio mean / max | Turn mean / max |
|---|---:|---:|---|---|
| ramp_w_foot | 0 | 10 | 1.067 / 1.112 | 99° / 182° |
| ramp_w_foot | 1 | 10 | 1.399 / 1.412 | 153° / 188° |
| plateau_se | 0 | 10 | 1.044 / 1.077 | 65° / 165° |
| ramp_e_foot | 0 | 10 | 1.573 / 1.598 | 206° / 292° |
| ramp_e_foot | 1 | 10 | 1.175 / 1.216 | 114° / 227° |

Outcome: player 0 wins at tick 1,355. Every mean ratio is within D-12's +0.05 of WP2+WP3 (largest rise: ramp E,
+0.016). Only the Rhino Beetles (radius 0.5) take new corners, 0.125 further out.

Matrix (1,000 runs) and battle 33 (68 seeds): bit-identical to WP2+WP3. Goldens re-recorded: `golden_replay` from
checkpoint 1 and `golden_replay_orders` from checkpoint 3, both because "blue_armor" (radius 0.5) turns its ramp
corner 0.55 from the corner instead of 0.425 (fixture headers); with the tier offset restored both reproduce as
recorded for WP2+WP3. Checksum schema unchanged by WP4.

## Merge with main `0e1a836` (WP13c, schema 21 for `attack_dash_cooldown`)

`corner_round` lands as schema 22, both fields hashed. Both goldens and the sandbox checksum are re-recorded on the
merged tree; the behaviour changes are those of WP2+WP3 and WP4 above on top of WP13c, which changed no behaviour in
the replays. Matrix (1,000 runs) and battle 33 (68 seeds) on the merged tree are bit-identical to a build of main's
simulation (`0e1a836`). The showcase numbers equal those of WP4 (player 0 wins at tick 1,355). Goldens verified in
debug, release and wasm32-wasip1.

## Part B, WP5 to WP8: the latent navigation mesh (2026-11-10)

Base `12ec668` (checksum schema 25). The mesh is built in `Pathfinder::new` and on every `recompute_clearance`,
and nothing in the simulation reads it: both goldens pass in debug, release and `wasm32-wasip1`, and the sandbox
checksum (`sim_fixtures` `fixture_tests`) passes, unchanged. The showcase mesh digest (`navmesh_tests.rs`,
`0x46d3_38ba_8cd1_c537`) is the same on x86_64 and `wasm32-wasip1` (wasmtime, release).

### Mesh size and build time

`cargo run --release -p sim_map --example navmesh_measure` (shared server, medians of 21 builds; "with structures"
places 3x3 structures on a 9-cell lattice of free footprints):

| Map | Vertices / triangles | Build (µs) | Structures | With structures: vertices / triangles | Rebuild (µs) | `recompute_clearance` (µs) |
|---|---:|---:|---:|---:|---:|---:|
| default_arena 64x64 | 12 / 14 | 300 | 6 | 108 / 206 | 438 | 1298 |
| showcase_canyon 64x64 | 256 / 494 | 728 | 6 | 351 / 684 | 890 | 1870 |
| canyon 4x4 tiled 256x256 | 4057 / 8048 | 12535 | 40 | 4691 / 9316 | 14002 | 30472 |

R-3: the 64x64 maps rebuild within the 2 ms target; a 256x256 map of four-by-four showcase canyons takes about
14 ms, so R-3's local repair is needed before the switch (WP9) for large maps. The structure cells are fully
gridded (every cell edge of a footprint is a constraint, so a structure triangle lies in one cell and per-cell
knowledge stays exact), which is part of the cost.

### Exact oracle (WP6)

`pathfinding_properties.rs` compares `find_route_mesh` with an exact disc oracle (`tests/support/oracle.rs`:
tangents between circles of radius `r + margin` round every wall vertex, arcs between them, Dijkstra). Radii are
drawn from 0.25 to 1.0 with no lattice gap `sqrt(k)` within 0.02 of `[2r, 2(r + margin)]`. On 120 seeded 16x16
maps (one cell in five blocked), 542 of 542 routes are within 1 % of the oracle (worst 1.0007, tangent-and-arc
length), none disagrees on existence, none fails a leg, none falls back; the proptest (24 cases per run; runs of
400 to 600 cases while developing) holds existence, legality and a hard 1.10 bound.

Narrow passages (`funnel_tests.rs`): a 1-cell gap admits radius 0.5 and refuses 0.5 + 2^-32; a 2-cell gap admits
1.0 and refuses 1.0 + 2^-32; a diagonal gap of sqrt(2) admits 0.7071 and refuses 0.7072; two structures touching at
a corner block radii 0.05 to 0.5 and are walked through when unknown. S12's adversarial case was not constructed
explicitly; the randomized oracle comparison found two families of failures while developing, both fixed in the
route layer, not in the width test: a narrow gap inside the start triangle (exempt from the width test by D-3), now
caught by a reach check of the start and goal triangles, and funnel turns at vertices of the start and goal
triangles that belong to no portal, now refined. Rare funnel legs that still fail the segment predicate fall back
to portal midpoints and are counted (`Pathfinder::mesh_funnel_fallbacks`, R-2's second outcome): 1 in about 3,600
grid-and-mesh queries below.

### Grid against mesh (WP8), 1,000 seeded queries per map and radius

| Map | Radius | Both | Grid only | Mesh only | Neither | Mean mesh/grid | Worst | > 1 % | Illegal mesh | Fallbacks | Grid µs (median) | Mesh µs (median) | Max expansions |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default_arena | 0.375 | 1000 | 0 | 0 | 0 | 1.0004 | 1.0062 | 0 | 0 | 0 | 3 | 2 | 8 |
| default_arena | 0.5 | 1000 | 0 | 0 | 0 | 1.0006 | 1.0078 | 0 | 0 | 0 | 3 | 2 | 8 |
| showcase_canyon | 0.375 | 790 | 0 | 0 | 210 | 1.0015 | 1.0230 | 12 | 0 | 0 | 172 | 201 | 293 |
| showcase_canyon | 0.5 | 793 | 0 | 0 | 207 | 1.0052 | 3.0755 | 49 | 0 | 1 | 170 | 211 | 295 |

Found-or-not agrees on every query (the 210 "neither" pairs start or end in a region cut off from the other).
The mesh is longer than the grid by more than 1 % on 12 and 49 of the pairs: the mesh polyline replaces each
turn above 45° by two points outside the circle of `r + margin` (D-6), where the grid keeps one offset point whose
legs are only checked against cell clearance; the worst case (3.08) is the one portal-midpoint fallback. D-11's
"never longer than the grid by more than 1 %" therefore does not hold for the polyline; WP9 has to decide whether
to accept it or emit arcs more tightly. Ten largest gains (grid minus mesh, world units): 2.713, 2.670, 2.644,
1.668, 1.601, 1.371 (showcase canyon), 1.295, 1.181, 1.138 (default arena), 1.094 (showcase canyon); the example
prints their endpoints. Search time: the mesh is about as fast as the grid on the open arena and about 20 % slower
on the canyon (TA*'s anytime loop, `NAV_TA_STAR_EXTRA_EXPANSIONS = 256`, `NAV_TA_STAR_TRIANGLE_VISITS = 4` per
triangle and entry edge; the oracle share above is the same with a budget of 100,000).

Segment predicate (WP7, `reachability_tests.rs`, 10,000 random segments per map and tier): every disagreement is
a segment the grid admits and the mesh refuses (the grid passes cells whose clearance admits the tier somewhere;
the mesh tests the swept disc), 4 % to 9 % of segments on the default arena; the mesh never admits a segment the
grid or `ground_segment_is_connected` refuses.

### Throughput

`cargo bench -p sim_core --bench throughput` (shared server, under the gate lock):

| Map | Units | Ticks/s (median of 3) |
|---|---:|---:|
| 64x64 | 0 | 3385 |
| 64x64 | 100 | 1029 |
| 64x64 | 400 | 169 |
| 128x128 | 400 | 205 |
| 256x256 | 0 | 208 |
| showcase_canyon.scmap.ron | 400 | 141 |

Every scenario is above 25 ticks/s. The flat rows are 2 % to 15 % below `DESIGN.md` §5 on the loaded shared
server; nothing in a flat battle reads the mesh, which is built once per world (a structure placement rebuilds it).
The new terrain row is the grid baseline WP9's stop rule compares against.

## Amendment 2, step 1: tighter turn emission and the wrap retry (2026-11-10)

Base `02f494b`. Latent: nothing in the simulation reads the mesh yet, so goldens, matrix and battle 33 cannot move.

**Cause, confirmed on the longest pairs.** A throw-away probe (not committed) set each WP8 canyon pair's grid
length beside the mesh polyline, the same route as tangents and arcs, and the exact oracle at `r` and at
`r + margin`. Two causes:

1. *Emission.* Each turn above 45 degrees became two points on tangents outside the circle of `r + margin`, one
   below became the tangent lines' intersection, so the polyline was up to 2 % longer than its own
   tangent-and-arc route (`(26.5, 37.5) -> (30.5, 27.5)` at r 0.5: polyline 24.284, arcs 24.131 = oracle 24.131,
   grid 23.779). Done exactly as arcs, 3 (r 0.375) and 16 (r 0.5) of the pairs stay more than 1 % longer, not 12
   and 49.
2. *The 3.08 fallback* (`(36.5, 51.5) -> (43.5, 49.5)`, r 0.5): the unit stands under ramp E's south side wall
   and must go round its western end (35, 52), up the ramp and out east, a turn of more than 180 degrees at
   (35, 52). The shrunk funnel shrinks that fan vertex towards each portal's other end, so its point orbits the
   circle and the apex beside it sees the side sweep past; and the refinement drops a turn whose tangent cross
   product has the "wrong" sign, which a turn above 180 degrees has. The route lost (35, 52), crossed the wall and
   fell back to portal midpoints.

**Fix** (`navmesh/funnel.rs`, `navmesh/search.rs`):

- Arcs are emitted on their circle: the two tangent points and, between them, the arc halved until every chord
  keeps at least `r + margin - ARC_CHORD_DIP` (`ARC_CHORD_DIP` = margin / 2 = 0.025) from the vertex, at most
  2^6 pieces. Exact Q32.32, bisection by normalised sums, no trigonometry. The polyline is never longer than the
  tangent-and-arc route; every leg keeps at least `r + 0.025` from the vertex it rounds and still passes the
  segment predicate (checked, as before).
- When the shrunk funnel's route fails the segment predicate, the same channel is pulled again with
  `PullStrategy::Pinned` (portal vertices themselves, and a turn above a half turn is kept when it reverses the
  travel direction and the anchors as points bend with the side); used if legal, else portal midpoints as
  before. Applying either rule to every channel instead produced 8 to 10 new fallbacks per 1,000 queries
  (slack spikes look like wraps locally), so it is only the retry.

**Grid against mesh after the fix** (`navmesh_measure`, the WP8 queries; new columns: routes the grid returns
that fail the exact disc predicate, and mesh routes more than 1 % longer than a grid route that passes it):

| Map | Radius | Both | Mean mesh/grid | Worst | > 1 % (WP8) | > 1 % | > 5 % | Illegal grid | > 1 % over a legal grid route | Fallbacks (WP8) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default_arena | 0.375 | 1000 | 1.0002 | 1.0037 | 0 | 0 | 0 | 358 | 0 | 0 (0) |
| default_arena | 0.5 | 1000 | 1.0004 | 1.0048 | 0 | 0 | 0 | 398 | 0 | 0 (0) |
| showcase_canyon | 0.375 | 790 | 1.0008 | 1.0137 | 12 | 3 | 0 | 505 | 0 | 0 (0) |
| showcase_canyon | 0.5 | 793 | 1.0017 | 1.0439 | 49 | 16 | 0 | 527 | 0 | 0 (1) |

Acceptance (Amendment 2, decision 1): no pair more than 5 % longer, no fallback, at most 1 % of pairs more than
1 % longer on three of the four rows. **The r 0.5 canyon row has 16 of 793 (2.0 %).** Every one of the 16 is a
grid route that the disc cannot walk: it fails the exact predicate (the grid checks cell clearance and offsets a
corner by `r + margin` on its bisector, so its legs pass inside `r` of the corner). In 6 of them even the exact
oracle at `r` itself, with no margin, is more than 1 % longer than the grid; the other 10 are within the
margin's cost (0.025 to 0.05 per turn). No mesh route is more than 1 % longer than a grid route that is legal.
Keeping the margin whole cost 2 more pairs (ARC_CHORD_DIP = 7/8 margin gave 14).

Oracle agreement unchanged: 542 of 542 seeded routes within 1 % of the exact oracle, worst 1.0007, no fallback
(`pathfinding_properties.rs`). `funnel_tests.rs` now holds the polyline at or below its arcs and at or above the
oracle at `r + margin / 2`, and adds the wrap pair above (turns (35, 52) and (38, 52) on the right, no fallback,
within 2 % of the oracle at `r`).

## Amendment 3, diagnosis of the held switch (2026-11-10)

Held switch `7636e32` on `c34955b`, simulation unchanged by this step (the showcase `units.csv` is byte-identical
to the held switch's). Probes were throw-away examples, not committed.

**Every showcase regression follows from which side of the central plateau each unit takes.** The two lanes
between the ramp feet, west (x 25.6) and east (x 38.4), differ by about 0.3 world units in 80 to 95 (0.35 %).
Initial routes (tick 0): all ten player 0 units west (the grid took them east); player 1's eight pulse bots east,
its two rhino beetles (radius 0.5, spawned north of the base) west. The rhinos meet player 0's whole army in the
west lane at tick about 330, at (22 to 26, 32 to 35), and hold it until about tick 525 (player 0's units have no
route and stand attacking or waiting behind the firing line), while the eight bots walk east to player 0's base.

- *Winner.* With a throw-away rule that refuses the west lane to player 1 only (so the armies pass each other, as
  on the grid), player 0 wins at tick 1,195 (base 1,100), ramp E foot 1.157 / 1.164 and 66° / 75°, reversals 66.
  The flip is the split army, not route following.
- *Reversals.* 228 of player 0's 255 (counted from `units.csv`) are in that fight, ticks 350 to 550: units behind
  the firing rhino toggle the surround walk (`flank_active`) every tick, friendly blocking's rule. With the armies
  passing, the remaining reversals are at player 1's base (47, 53), as in base.
- *Ramp W foot.* Units follow their routes cleanly (rhino 13: no re-plan, walked 5.80 in the box against about
  5.65 of route); the ratio rises because the west lane turns about 100° at the ramp's north wall end (29, 12)
  where the east lane turned about 35°. Plateau SE is not passed because the lane is the other one.
- *Ramp E foot turns.* The army arrives as a crowd after the fight; bodies pushed east of the corner (35, 52)
  under the ramp's south wall wait on stall repaths (rhino 12: 975°).
- *Lead 1, re-plans:* not supported. Whole match, player 0 routes adopted per unit 2 to 13; no re-plan storm at
  (25.575, 35.0) — the units there were stopped by the fight.
- *Search.* The side choices are TA*'s: for player 1's bots the first channel is east and the anytime loop spends
  its budget on east variants; raising the budget to 8,192 and the per-state visits to 32 changes no choice.

**Throughput** (bit-identical changes, measured unlocked: the gate lock was held by other agents' gates; canyon
scenario ticks/s, base `02f494b` and candidate back to back):

| Build | Canyon | 64x64, 400 |
|---|---|---|
| base | 134, 134, 137, 142, 141, 143 | 171 to 176 |
| held switch | 116 to 124 | 154 |
| one square root per edge distance (TA* bound) | 126, 127, 127 | 159 to 171 |
| + leg asked before the wedge check in `drop_passed_waypoints` | 127, 127, 128 | 157 to 172 |
| + open edges to reached triangles skip the capsule test | 116 / 104 against base 128 / 137 (machine loaded) | — |

The canyon scenario is 7 to 10 % below base after the first two changes; the noise between runs is of the same
size, so the 10 % criterion needs a measurement under the gate lock.

**Pre-existing red tests at `7636e32`** (unchanged by this step): `determinism_tests`
(`test_cross_platform_determinism_checksum`), `validation_golden_tests`
(`find_path_with_vision_matches_the_recorded_corpus_digest`) and `pathfinding_properties`
(`funnel_paths_are_legal_and_no_shorter_than_a_straight_line`: an illegal leg, recorded in the commit's
`pathfinding_properties.proptest-regressions`).

## Amendment 4: defects a to d on the held switch (2026-11-10)

Held switch `dd2e5e9` with main `d914917` merged. Output under `~/scratch/path-c/` (`sb/`, `c4/`; not committed).

**a. Illegal funnel leg: not a route defect.** Seed of record: start (9, 12), goal (14, 2), radius 0.625. The start
is wedged, 0.5 from the walls west and north of it. The route rounds the corner (9, 12) beside it; the arc's first
point, the tangent point (9.598, 12.313), is 0.598 from the west wall, still inside the disc's band, so the second
leg also starts where the disc does not fit. D-3 and the search's own legality check (`navmesh/search.rs`,
`is_legal`) hold every leg up to the first waypoint where the disc fits to the centre only; the property exempted
only the first leg. The property now applies D-3's rule exactly. The two `sim_map` digests: a build of the switch
commit `1f75ae0` itself produces `0x672c257d0fcacc1b` and `0xa1a3327e2a30f8b7`, the values of this tree; the switch
recorded them before its last edits. No route changed.

**b. Walk toggling:** friendly-blocking plan Amendment 7 (its evidence has the figures). On the switch, player 0's
showcase reversals 253 to 56, player 1's 24 to 2.

**c. Ramp E corner crowding: not fixed.** Trace: rhino 12 (and pulse bots 4, 6, 7) reach the corner (35, 52) as a
column and start a surround walk (attack-move without a target, a standing friend ahead) on the side kept from an
earlier walk, the inside of the turn, which carries them east under the ramp's south wall; there they lose sight
of the corner and wait for a stall re-plan. Pulse bot 7 ends wedged on the corner vertex itself with a route whose
first leg passes D-3's centre-only test, so no re-plan comes, and it jitters (1,364° in the box). Tried, not
kept: a walk side closed where the step loses terrain view of the next waypoint, and a walk round a crowd on a
route corner always on the outside of the turn. Together: ramp E ratio 1.295 to 1.247, but turn mean 352° to
339°, maximum 845° to 1,364°, 9 of 10 in range at tick 900 (from 10), reversals 56 to 61. Patch at
`~/scratch/path-c/c4.diff`.

**d. Throughput, under the gate lock**, base `02f494b` and candidate alternately, ticks/s:

| Scenario | Base | Candidate |
|---|---|---|
| showcase_canyon, 400 | 140, 142, 143 | 127, 126, 125 (-11 to -13 %) |
| 64x64, 400 | 171, 176, 174 | 169, 171, 171 |
| 128x128, 400 | 206, 212, 210 | 201, 204, 198 |

Every scenario is above 25 ticks/s; the canyon scenario misses the 10 % criterion.

**Showcase on the switch with b** (player 0 / base): ramp W 1.301 / 1.064 and 157° / 103° (the west lane, its
lane-end turn about 100°); plateau SE not passed by player 0 (the lane is the other one); ramp E 1.295 / 1.573 and
352° / 211°; reversals 56 / 11 (acceptance at most 21); 10 of 10 in range; 0 give-ups; player 1 wins at tick 1,266
(the lane split). **Acceptance not reached:** reversals, ramp E turning, canyon throughput. The switch stays held.

## Amendment 5: mirrored showcase targets, B-112, defect c (2026-11-10)

B-112 was registered here as B-106; the effects branch had already used B-106 to B-111, so plan Amendment 6
renumbers it (commit `3c51afe` keeps the old name in its message).

Held switch `0209b54` plus this amendment's commits. Grid runs: main `d914917` with friendly-blocking fix b
(`35aab04`), the mirrored targets and B-112, built from a source copy under `~/scratch/path-c/grid-src`. Outputs
under `~/scratch/path-c/` (`gridm2/`, `mir2/`, `cA/` to `cC/`; not committed). Outcome measurements ran without
the gate lock (other agents' gates held it; outcomes do not depend on timing).

**Mirrored targets.** The canyon map is point-symmetric about its centre, (x, y) to (63 − x, 63 − y) in cells,
apart from the central plateau (72 level cells, 42 ramp cells and 18 flag cells differ, all in or beside the
plateau; the anti-diagonal and the diagonal leave 54 to 342 cells unmatched per layer). The targets are now cells
(52, 52) and (11, 11), the centre-facing corners of the two base rings (scenario order `AttackMoveToCell`).

**B-112: an attack-move resumed after a chase gave up.** On the switch with mirrored targets four of player 0's
pulse bots gave the attack-move up at tick 828 (`NoProgress`, exactly 250 ticks after they resumed it at 578) while
walking freely toward player 1's base, and the match never ended. Cause: `prepare_pursuit` dropped a finished
attack order without restarting the movement memory, so the resumed attack-move measured its progress against the
chase's best distance (a few world units to the dead target). Fixed by restarting the memory as a new order does
(test red before, green after). On the grid the matrix (16 seeds) and battle 33 (68 seeds) are bit-identical with
and without the fix, and so is the grid showcase. The grid's `golden_replay` is not: with B-112 it diverges at its first checkpoint
(ticks up to 50) in debug and release, and passes without it; `golden_replay_orders` passes either way. Before
B-112 lands on main that list is re-recorded with the traced cause (not done in this step).

**Showcase with mirrored targets** (player 0 / player 1; ratio mean / max, turn mean / max):

| Box | Grid | Switch |
|---|---|---|
| ramp W foot, P0 | 1.064 / 1.112, 103° / 213° (east lane) | 1.301 / 1.473, 157° / 301° (west lane) |
| ramp W foot, P1 | 1.588 / 1.644, 194° / 241° (west lane) | not reached (army died in the lane fight) |
| plateau SE, P0 | 1.044 / 1.093, 66° / 191° | not passed (west lane) |
| ramp E foot, P0 | 1.573 / 1.611, 211° / 287° | 1.198 / 1.300, 190° / 454° (6 units) |
| ramp E foot, P1 | 1.170 / 1.214, 108° / 214° | 1.149 / 1.168, 81° / 152° |

| | Grid | Switch |
|---|---|---|
| Lanes (x where units cross y = 32) | P0 east (38.0 to 38.6), P1 west (25.8 to 25.9): the armies pass | both west (P0 22.5 to 26.0); they meet at (22 to 28, 31 to 38) from tick 310 to 576 |
| Winner | player 0, tick 1,102 | player 0, tick 1,693 (6 of 10 survive the fight) |
| In range at tick 900 | 10 of 10 / 9 of 10 | 0 of 6 / 0 of 0 (between the welders' death at 891 and the hall; 9 of 9 in the fight at tick 430, 6 of 6 from 940) |
| Reversals | 10 / 5 | 17 / 7 |
| Stall give-ups | 57 / 0 | 0 / 0 (4 / 0 before B-112) |

The grid's 57 give-ups are all one unit's: player 0's welder drone (entity 2), standing at its base, (6.4, 6.0),
abandons a chase of player 1's raiders (`PursuitAbandoned`, targets 17, 20 and 21 in turn) 57 times between ticks 624
and 814; the abandoned-target memory skips one target only, so it takes the next. Not a route defect; it is not
filed here.

The mirrored targets make the lanes symmetric on the grid only. The central plateau is not point-symmetric (its
northern corners, y 35 to 37, are low ground where the southern ones are plateau, and its one ramp faces south),
so on the mesh, which measures the lanes exactly, both armies find the west lane shorter and meet in it.

**Matrix and battle 33**, switch against the grid with the same fixes: bit-identical (whole matrix, 16 seeds; battle
33, 68 seeds). Fix b against base `02f494b`, for reference: battles 21, 26, 31 (the accepted flip) and 33 move;
battle 33 blue 48 → 47, red 9 → 16 of 68.

**Defect c, ramp E corner.** Trace of pulse bot 7 (454° in the box): at tick 756 the crowd has pushed it to 0.30
from the corner vertex (35, 52), inside its radius 0.375, so it is wedged; at 757 it passes the two arc points round
that corner by terrain view (the wedged exemption of `drop_passed_waypoints`), its leg to (35.00, 52.42) cuts the
corner, the step is refused at 762 and the route dropped; the re-plan leads back west to (34.58, 51.98), and it steps
back and forth until 769, then walks straight on. Three variants, each measured on the whole showcase:

| Variant | Ramp E P0 | Ramp W P0 | Ramp E P1 | Reversals | Kept |
|---|---|---|---|---|---|
| none (B-112 only) | 1.198 / 1.300, 190° / 454° | 1.301 / 1.473, 157° / 301° | 1.149 / 1.168, 81° / 152° | 17 / 7 | — |
| A: an attack-move's walk side resets at each route corner (ii) | bit-identical: no attack-move surround walk starts at a corner in this battle | | | | no (no effect) |
| B: a wedged body keeps an arc point where its disc fits until the disc fits along the leg past it | 1.209 / 1.341, 164° / 508° | 1.407 / 1.611, 270° / 636° | 1.174 / 1.288, 127° / 397° | 21 / 3 | no |
| C: a wedged unit whose first leg leads deeper into the wall (a half-radius disc fits where it stands, not along the leg) re-plans, at most once per re-plan cooldown and only while stalled (i) | 1.191 / 1.270, 239° / 739° (5 units) | 1.300 / 1.473, 155° / 303° | 1.149 / 1.168, 81° / 152° | 20 / 7 | no |

B makes the column at ramp W's lane-end corner walk back to arc points the crowd pushed it past; C's re-plan from
the wedge gives the same route back out (the one seen at tick 763), earlier, and changes the fight. Re-plans were
not counted, as neither variant is kept. Without a variant the ramp E means are better than the grid's on both
sides; the maximum turn of player 0 (454°, pulse bot 7) is not.

**Throughput** not re-measured: the gate lock was held by other agents' gates throughout, and nothing in this
amendment touches the route search. Amendment 4's figure stands: canyon 125 to 127 against 140 to 143 ticks/s
(−11 to −13 %), within the 15 % of Amendment 5, 2.

**Acceptance (Amendment 5, 5): not reached.** Met: per-box means on the lanes taken (ramp E better than the grid
for both players; ramp W's west-lane box better than the grid's west-lane box for player 1), reversals 17 and 7
(at most 20 and 15), 0 give-ups, winner unchanged (player 0), matrix and battle 33 bit-identical, throughput. Not
met: 10 of 10 in range at tick 900 (the battle has another shape: the armies meet, 6 of player 0's units survive, and
at tick 900 they are between targets), and the ramp E maximum turn (454° against 287°). The switch stays held.

## Amendment 6: landing the switch, WP10 and WP11 (2026-11-10)

Plan Amendment 6 accepts the switch with the mirrored showcase targets. This section records the landing on the tree
of main `0157437`, the switch, the mirrored targets, B-112 (registered above as B-106) and WP10. Tools: release builds
of `route_trace` and `matchup_trace` per tree; outputs under `~/scratch/path-c/` (`trace/`, `m/`, `wp10/`; not
committed).

**Goldens, traced.** Three source copies were built and replayed tick by tick, dumping every entity's state after
each tick (`Debug`) and the checksum: A, main `0157437` with the mirrored targets; B, A plus B-112; C, the branch
head (the switch, the mirrored targets, B-112 and WP10). Every copy builds into its own target directory: cargo
names workspace members by their path relative to the workspace, so copies sharing one directory reuse each other's
artefacts.

| Change | `golden_replay` | `golden_replay_orders` |
|---|---|---|
| Mirrored targets (A against main) | unchanged | unchanged |
| B-112 (B against A) | first state difference on tick 200: pulse bot 1's movement memory restarts when its target dies, where it kept the chase's best distance (0.977 to `None`); reconverges; lasting from tick 500, when psi grey 26 ends a chase the same way (positions from tick 536); checkpoint 11 (ticks 501 to 550) on | unchanged |
| The mesh (C against B) | from tick 5, the first tick "blue_infantry" walks: pulse bot 1's route rounds the corner near (30.3, 29.7) on two arc points of its own radius where the grid gave one corner waypoint; its first step differs by 0.0001; checkpoint 1 on | from tick 20, the column's first Move: pulse bot 9 rounds the corner near (33.6, 29.9) on two arc points where the grid's line ran straight; checkpoint 3 (ticks 21 to 30) on |

With the mesh, `golden_replay_orders` lost its mid-route repath: the column no longer stalls at the foot of the ramp
on its way to (32, 37), and the test's evidence check fails (0 repaths). Five goals for the column were tried, each
also with route upkeep and the arrival decision of `MotionDraft::walk` swapped (the mutation the fixture exists to
catch):

| Column goal | Repaths | Swapped: repaths | Swap diverges at checkpoint |
|---|---:|---:|---:|
| (32, 37), as recorded | 0 | 0 | none |
| (32, 35) | 2 | 4 | 34 |
| (31, 34) | 5 | 2 | 17 |
| **(32, 36)**, kept | 4 | 1 | 15 (ticks 141 to 150) |
| (32, 39) | 4 | 3 | 14 |
| (33, 38) | 1 | 3 | 20 |

(32, 36) is the smallest change: one cell south, still just past the top of the ramp. Units 19 and 22 stall at the
foot corner (34.0, 29.9) and are re-routed mid-order on ticks 140 and 172, unit 11 at the plateau corner on ticks 347
and 496; the swap diverges within ten ticks of the first. Both fixtures are re-recorded (`d9d210a`) and pass in debug,
release and on `wasm32-wasip1` (wasmtime). The sandbox checksum is unchanged: it hashes the spawned state only.

**Showcase battle** on the head (`route_trace`, 1,800 ticks): identical to Amendment 5's "none" row: ramp W foot P0
1.301 / 1.473, 157° / 301°; ramp E foot P0 1.198 / 1.300, 190° / 454° (6 units), P1 1.149 / 1.168, 81° / 152°;
reversals 17 / 7; give-ups 0 / 0; player 0 wins at tick 1,693. Timeline (units leaving the trace): the first death on
tick 333, the lane fight to tick 577 (player 0 loses two rhino beetles and two pulse bots, player 1 its ten
attackers), player 1's workers on ticks 868 and 890, the hall on tick 1,693.

**Matrix and battle 33** against a build of main `0157437`: the whole matrix (16 seeds) and battle 33 (68 seeds) are
bit-identical; battle 33 blue 47, red 16, other 5 of 68. The fix-b movements recorded in Amendment 5 are already on
main; B-112 and the switch move nothing.

**Storyboard.** With the switch the armies meet in the west lane at about tick 310, during the old `units` and
`smog` scenes, and the old `advance` (at 4x from tick 575, minimum 50 frames) would skip the lane fight. Restructured
(`assets/captures/showcase.ron`):

| Scene | Before | After | Render (frames, start tick) |
|---|---|---|---|
| title | paused, 125 | unchanged | 125, 0 |
| terrain | 2x, 150 | paused, 150 | 150, 0 |
| units | 1x, 150 | unchanged | 150, 1 |
| smog | 1x, 125 | 1x, 100 | 100, 151 |
| advance | 4x until combat, 50 to 500 | 1x until combat, 25 to 500 | 54, 251 |
| clash | 1x, 300 | 1x, 270 | 270, 305 |
| march (new) | | 4x until combat, 5 to 200, following one survivor; caption "Pathfinding" | 68, 578 |
| assault (new) | | 2x, 200; caption "Assault" | 200, 848 |
| closing | paused, 200 | unchanged | 200, 1,246 |

`march` follows one survivor rather than the army: the army's centroid includes player 0's workers at home, so it lies
on empty ground. The teaser loses the `advance` cut (its minimum is now 25 frames, below the old cut's 40) and gains a
75-frame `assault` cut. Rendered on the RX 580 (`showcase render --driver radv`, one GPU lock per take), 1,317 frames,
the site assembled within its budgets; `showcase_storyboard_tests` pass. Stills (640 wide) under
`~/scratch/path-c/wp10/`: after, `show2/stills/f560.png` (advance: the column at the west lane, the enemy base and
caption "Fog of war"), `f700.png` (clash in the lane), `f890.png` (march), `f950.png` and `f1100.png` (assault on the
hall), `f1200.png` (closing card, before the result); before (the old storyboard rendered on the same tree, take
only), `show-before/stills/f400.png` (the old `units` caption over the start of the lane fight, tick 426) and
`f600.png` (the old `advance` at 4x over empty ground after the fight, tick 778). An intermediate
render (`show1b`, advance at 4x with a 5-frame minimum and `march` following the army) showed the caption of
`advance` for 8 frames and `march` over empty ground; both were changed.

**WP10.** Removed from `crates/sim_map/src/pathfinding.rs` (1,780 to 499 lines): `find_path_grid`,
`search_corridor`, `reconstruct_corridor`, `build_portals`, `add_orthogonal_portal`, `run_funnel`,
`offset_corner_waypoint`, `smooth_connected_corridor`, `index_corridor`, `validate_endpoints`,
`nearest_reachable_point_grid`, `grid_segment_is_passable`, their helpers, `PathfinderBuffers`, `PortalEdge`,
`NO_HEAP_POS` and `UnitClearance`; `PathRequest` and `PathRequestWithVision` carry only the radius. The clearance
field
stays: `checksum.rs` hashes it. Placement does not read it (`grep`). `VALIDATION_PATH_CLEARANCE` becomes
`VALIDATION_PATH_RADIUS` (0.625, the old Medium tier). `navmesh_measure` keeps the mesh size, build and route
statistics; its grid-against-mesh columns are retired, and the WP8 table above is their last measurement. No
behaviour change: every `sim_map` test, including the two recorded digests, passes unchanged.

**Throughput** (`cargo bench -p sim_core --bench throughput`, release, each run under the gate lock, the head and
main `0157437` alternated, six rounds; rounds 1 to 3 at load average 9 to 17 while other agents built, rounds 4 to 6
at 5.5 to 6.5). Medians of rounds 4 to 6, head against main: 64x64 empty 3392 / 3383; 100 units 1031 / 1031; 400
units 173 / 177; 128x128 400 units 203 / 209; 256x256 empty 208 / 210; `showcase_canyon` 400 units 127 / 145
(−12 %, within Amendment 5's 15 %). Rounds 1 to 3 give the canyon 115 / 133 (one run at 83 under load). Every
scenario is above 25 ticks/s. `DESIGN.md` §5 now carries the table of rounds 4 to 6.

**Bookkeeping (WP11).** Register: B-102 closed (removed), B-113 added, open; B-112 was found and fixed within this
plan and never registered open. The deviation-fix plan's Amendment 2 table marks B-102 fixed and its counts follow.
The `DESIGN.md` Movement and pathfinding row names the mesh and lists `navmesh_tests.rs`. The plan's index row is
Done.
