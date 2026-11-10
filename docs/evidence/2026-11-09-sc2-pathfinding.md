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
