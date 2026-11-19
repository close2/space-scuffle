# Evidence: Funnel Fallbacks (B-141)

Measurements for plan [`2026-11-19-funnel-fallbacks`](../plans/2026-11-19-funnel-fallbacks.md). Each work package
appends its section. The base is `main` at `6b922732`; its D-5 measurement is the planner's
(`~/scratch/funnel-fallbacks/base`), taken with release tools built from that tree.

## How to reproduce

The battle measurement is crowd-edge-steering-2's (`~/scratch/funnel-fallbacks/scripts/measure.sh <bindir> <outdir>
[<basedir>]`, run from the repository root, the matrix and battle runs serialised under
`flock ~/scratch/funnel-fallbacks/measure.lock`), with release builds of the two trace tools of `sim_core`:

```sh
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
```

It runs the matrix on 16 seeds, battle 33 on 64 seeds, battle 31 on 96 further seeds (100 with the matrix's four of
record), and the showcase to tick 3,000 on the default seed and seeds 1 to 8. `summary.py <run> <base>` compares two
runs. The fallback corpora use a scratch copy of the planner's probe test (`~/scratch/funnel-fallbacks/impl/
zz_b141_probe.rs`, `B141_N` layouts, `B141_FLUSH` for the flush variant), never committed.

## WP1: regression cases, red on `main`

Five deterministic cases in `crates/sim_map/tests/funnel_tests.rs`, each `#[ignore]`d with the work package that
fixes it. Run with `--ignored` on the WP1 tree, every one fails on its first assertion, a portal-midpoint fallback:

| Test | Case | On `main` |
|---|---|---|
| `an_exact_border_pinch_is_rounded_on_its_tangent` | E1 | fallback, 8 waypoints |
| `a_goal_in_a_pinch_is_reached_along_the_pinch` | E2 | fallback |
| `a_one_wide_zigzag_is_walked_down_its_middle` | E3, layout 23 rebuilt from its 12 structures | fallback |
| `an_outside_corner_is_rounded` | M1 | fallback, 34.30 long |
| `a_start_touching_a_corner_does_not_circle_it` | flush layout 1138 rebuilt from its 11 contour structures | fallback |

**Deviation.** The plan's two unit tests of WP1 (`zero_room_legs_keep_the_disc_radius`,
`gate_runs_keep_the_feature_across`) test `ZERO_ROOM_SLACK`, `GATE_RUN(r)` and the circumscribed emission, which do
not exist before WP3, so they land with WP3. The per-class diagnostic stays a scratch probe (the plan's default): no
`fallback_class` in `MeshRoute`.

## WP2: D-3, obstacle vertices near a leg

**Change.** `segment.rs`: `obstacle_vertices_near`, the segment predicate's flood around a leg, run to completion,
listing the ends of the blocking edges it meets within `rho` of the leg. `funnel.rs`: `deepest_violation` adds those
vertices to its candidates (after the portal and end-triangle vertices, so a channel vertex keeps its portal side),
and `refine`'s round bound counts the distinct outside vertices it has inserted. `search.rs` passes the access and
scratch (`FunnelScratch` in `MeshSearch`). Tests: `an_outside_corner_is_rounded` (M1) un-ignored;
`outside_vertices_match_the_exhaustive_scan` (proptest, 64 cases, seeded 24 × 24 maps with cliffs and structures,
partial knowledge in half the cases), which a mutation (a flood that skips half its neighbours) fails at once.

**Deviation 1 (trace below): the start/goal exemption of D-2.2 applies to D-3's outside vertices in every pull.**
With D-3 as written (no exemption outside the zero-room retry), `CONTOUR_PATH_GOLDEN`'s corpus turned two legal
routes into fallbacks, both from **wedged starts** (the disc does not fit at the start):

| Request (corpus of `find_path_with_vision_on_contours_matches_the_recorded_corpus_digest`) | D-3 off | D-3 as written |
|---|---|---|
| default arena, partial knowledge, r 1.0, (8.25, 56.5) → (59.25, 2.5) | legal, 5 turns, 75.91 | **fallback**, 87.39 |
| showcase, all known, r 0.75, (19.5, 20.5) → (36.75, 46.5) | legal, 6 turns, 53.72 | **fallback**, 97.27 |

The trace (scratch debug print of the anchors and the refused legs): in both, the refinement inserts obstacle
vertices that lie within `r` of the start itself, (8.5, 56.0) and (9.0, 55.5) in the first, (20.0, 21.0) in the
second, because the first leg passes within `rho` of them **at its start point**. Rounding a vertex the disc
already overlaps makes the route circle it, and the circle's legs fail the predicate. That is exactly D-2.2's case
("a vertex is not inserted when its nearest approach to the leg is the leg's start or goal point"), which the plan
scoped to the zero-room retry only. Applied to the outside vertices of D-3 (never to the channel's own candidates,
whose behaviour is unchanged), both routes stay legal (the second becomes 53.54, 5 turns) and no route of any
corpus falls back where it did not before. **Proposed amendment:** D-3.1 reads "...within `rho` of the leg, except a
vertex whose nearest approach to the leg is the leg's start or goal point (D-2.2)".

**Deviation 2: M1's side.** The plan's table says M1 rounds (40, 6) "on the right". The route travels east with the
corner north of it, which is the walker's left (`Side::Left`, as for every other turn): the test asserts
`Side::Left`, with the plan's slip noted in its doc comment. Length 13.350, as planned.

**Pins.** `contour_route_checksum` does not move. `CONTOUR_PATH_GOLDEN` moves; with D-3 switched off by the scratch
control flag the test reproduces its old value `0x9069_95c7_29b7_3498`. Requests whose outcome changes (D-3 on
against off, 500 requests):

| Request | Class | D-3 off | D-3 on |
|---|---|---|---|
| showcase, partial knowledge, r 0.625, (42.75, 49.5) → (26.25, 19.5) | B-146 (free portal vertices, partial knowledge): fallback both ways, another channel's midpoints | fallback, 75.98 | fallback, 76.77 |
| showcase, all known, r 0.75, (19.5, 20.5) → (36.75, 46.5) | wedged start, an outside vertex within the margin | legal, 53.72 | legal, 53.54 |

The re-pinned value `0x6637_d70a_80d8_a859` is the same in native debug, native release and `wasm32-wasip1` release
(wasmtime with the worktree mapped by `--dir`); `determinism_tests` passes on `wasm32-wasip1` release.
`golden_replay_tests` passes unchanged in native debug, native release and `wasm32-wasip1` release
(`CARGO_TARGET_WASM32_WASIP1_RUNNER=wasmtime`).

**D-5 battle set (release tools of this tree, `~/scratch/funnel-fallbacks/wp2`), against `main`: bit-identical.**

| What | `main` | WP2 |
|---|---|---|
| Matrix, 1,160 runs | `matrix.csv` | identical file; reversals of record 4,659, all seeds 22,666; give-ups 2 / 5; no flip |
| Battle 33, 68 runs | blue 39 / 68 | identical `seeds.csv` |
| Battle 31, 100 runs | blue 49 / red 17 / 34 | identical `seeds.csv` |
| Showcase, nine seeds | 174 reversals | identical `units.csv` on all nine |

Stop rules 1 to 4 hold trivially. Control (rule 5): with D-3 switched off by a scratch flag (never committed), the
same set is again identical to `main`, and so is `CONTOUR_PATH_GOLDEN`'s old value.

**Path-search cost** (scratch bench of `find_route_mesh`: the 150-layout corpus in both shapes and 2,000 seeded
showcase requests at the shipped radii, median of five passes, base and candidate alternated, three rounds; host load
25 to 47 on 16 CPUs, shared with another agent's builds):

| Corpus | `main`, µs per route (3 rounds) | WP2 | Change of the means |
|---|---|---|---|
| 150 layouts, contours (1,875 routes) | 96.0 / 97.9 / 103.4 | 101.2 / 101.9 / 102.3 | +3.0 % |
| 150 layouts, squares (1,871 routes) | 72.9 / 72.9 / 73.8 | 79.0 / 80.1 / 79.1 | +8.5 % |
| Showcase, 2,000 requests (1,283 routes) | 274.9 / 277.2 / 289.0 | 285.3 / 368.7 / 285.5 | +3.3 % (median of rounds) |

Instructions retired over the whole bench (`perf stat -e instructions:u`, independent of load): 35.85 G against
37.33 G, **+4.1 %**. Within the 10 % of stop rule 7.

**Throughput.** The wall-clock benchmark (`throughput --bench`, base and candidate alternated, eight rounds) was not
readable at this host's load (1-minute load 17 to 148 on 16 CPUs during the runs; the same binary varied by up to 45 %
between rounds), so stop rule 7 was decided on instructions retired per scenario (`perf stat -e instructions:u`,
each scenario alone through a scratch `BENCH_ONLY` switch of the bench, never committed; the work is fixed, 300 ticks
three times):

| Scenario | `main` (G instructions) | WP2 | Change |
|---|---|---|---|
| 64×64, 0 units | 1.8604 | 1.8604 | +0.00003 % |
| 64×64, 100 units | 6.2024 | 6.2024 | +0.000005 % |
| 64×64, 400 units | 30.9056 | 30.9056 | −0.00001 % |
| 128×128, 400 units | 31.2293 | 31.2293 | −0.00001 % |
| 256×256, 0 units | 29.1412 | 29.1412 | +0.0000004 % |
| Showcase canyon, 400 units | 53.0852 | 53.3741 | **+0.54 %** |
| 64×64, 67 casters, 400 units | 48.0651 | 48.0651 | +0.000007 % |

Every scenario is within 5 % of `main`; the wall-clock medians that could be taken (all above 25 ticks/s: lowest 62,
the showcase under load 91) are in `~/scratch/funnel-fallbacks/impl/throughput-wp2*.txt`. **Deviation:** the
decision rests on instructions, not on wall-clock time, because of the host's load; a quiet-host wall-clock re-run is
an open item.

## WP3 stopped: the derived gate run lengthens r = 1 routes past stop rule 8 (trace and proposed amendment)

**What was built** (branch `funnel-fallbacks-wp3-wip`, not on the plan's branch): D-2 as written.
`ZERO_ROOM_SLACK = 2⁻¹²` (16 Q16 steps), `gate_run(r) = 2^⌈log₂ √(2rε)⌉` (2⁻⁵ at r 1, 2⁻⁶ at r 0.3125 to 0.875),
`Emission::{Inscribed, ZeroRoom}` in `radius_funnel`, the circumscribed emission (tangent lines, gate lines,
bisectors above an eighth turn, redundant lines dropped, axis gates snapped by assignment, gate runs),
`segment::blocking_features_near` (the point flood from the vertex's open fan), `MeshSearch::zero_room_retry` after
`fitted_retry`, and the start/goal exemption for every candidate of the zero-room pulls. Tests: E1, E2, E3 and the
touching start un-ignored and passing (E1's waypoints beside the face have `x == 1` exactly, E2's last leg lies on
`y == 29.5`); `contours_add_no_funnel_fallbacks` asserts zero fallbacks for both shapes and passes;
`funnel_paths_are_legal_and_no_shorter_than_a_straight_line` keeps its exemption only for wedged starts and passes;
unit tests `zero_room_legs_keep_the_disc_radius`, `gate_runs_keep_the_feature_across` (legal at `GATE_RUN(r)`,
illegal at a quarter of it, every shipped radius) and `a_circumscribed_arc_passes_an_exact_pinch` pass.

**Fallback corpora (WP3 tree):** zero everywhere.

| Corpus | Routes (contour / square) | `main` | WP3 |
|---|---|---|---|
| 150 layouts (the test) | 1,729 / 1,722 | 11 / 14 | **0 / 0** |
| 1,000 layouts | 11,932 / 11,838 | 25 / 30 | **0 / 0** |
| 2,000 flush layouts | 23,890 / 23,717 | 42 / 94 | **0 / 0** |

No fixed route has an illegal leg; none is shorter than the oracle at `r − 10⁻⁵` (the nearest is 0.00075 longer).

**The stop: rule 8, at least 95 % of fixed routes within 1.01 of the oracle.**

| Square corpus (routes that fell back on `main`) | Within 1.01 | Max ratio | With the prototype's run 2⁻⁸ |
|---|---|---|---|
| 150 layouts | 14 / 14 | 1.0048 | 14 / 14, 1.0025 |
| 1,000 layouts (contains the 150) | 28 / 30 (93.3 %) | 1.0258 | 29 / 30, 1.0258 |
| 2,000 flush layouts | 89 / 94 (94.7 %) | 1.0126 | 94 / 94, 1.0078 |
| All three, distinct routes | 117 / 124 (94.4 %) | 1.0258 | 123 / 124 |

None is beyond `MESH_RATIO_BOUND` (1.10). The seven routes above 1.01, all at r 1.0:

| Corpus, layout, request | Ratio, derived run | Ratio, run 2⁻⁸ | Cause |
|---|---|---|---|
| flush 34 q 10 | 1.0126 | 1.0078 | gate runs |
| flush 455 q 13, flush 475 q 13 (same route) | 1.0114 | 1.0066 | gate runs |
| flush 671 q 9, flush 689 q 9 (same route) | 1.0102 | 1.0036 | gate runs |
| 1,000-layout 791 q 7 | 1.0155 | 1.0077 | gate runs |
| 1,000-layout 563 q 15 | 1.0258 | 1.0258 | the channel TA* picks (independent of the run) |

**Trace.** Six of the seven exceed 1.01 only because of the run: with the run set to 2⁻⁸ by a scratch switch,
everything else unchanged, they fall to 1.0036 to 1.0078. A run of 2⁻⁵ at r 1 replaces each meeting point
within 2⁻⁵ of a gate's touch point by the run's end, so the route walks 2⁻⁵ along the gate before it turns, at
every gate on both sides. Restricting the runs to gates across a point (an edge's end), where the derivation of
D-2.5 applies, changes nothing (scratch switch, same seven figures): every lengthened gate is across a vertex. The
derived bound is a worst case: in these routes the legs that the run replaces do not come within `ε` of the gate
near its touch point, which is why 2⁻⁸ was legal in every corpus. The seventh (563) is not a defect of this plan.
R-3 says a residual is traced, not tuned, and that a shortfall of the derived value is a stop, so WP3 stops here.

**Proposed amendment (for the owner of the plan to decide).**

1. **Conditional runs (recommended).** Keep `GATE_RUN(r)` as derived, but replace a meeting point within the run
   only when the leg leaving it would pass within `r` of the gate's feature (an exact Q16 check of that one leg
   against that one point, the predicate's own). The derived bound remains the guarantee where it is needed, and
   no route walks a run it does not need. Expected: the prototype's figures (123 / 124 within 1.01), to be
   measured.
2. Or restate stop rule 8 for this plan as "at least 94 % within 1.01, none beyond 1.03" (the derived runs as built:
   117 / 124, max 1.0258).
3. Not proposed: the prototype's 2⁻⁸ as a constant. It is a tuning value without a derivation (rule 9), and the
   unit test shows a quarter of the derived run is illegal against a point feature in the worst case.

**Also measured on the stopped tree** (`~/scratch/funnel-fallbacks/wp3`): the D-5 battle set is **bit-identical** to
`main` (matrix file, battle 33 and 31 `seeds.csv`, showcase `units.csv` on nine seeds); the retry fires in no battle.
The two pins would move to `contour_route_checksum` `0xf390_68c2_2c01_2b87` and `CONTOUR_PATH_GOLDEN`
`0xab3f_b3c9_d6ae_df40` (native release; with D-2 and D-3 off the tree reproduces both old values, with D-2 off
WP2's). Not re-recorded and not proven on other targets, since WP3 commits nothing; throughput not measured. The
branch `funnel-fallbacks-wp3-wip` (`97765727`, on WP2) holds the code; its two pin tests fail by design until an
amendment decides the run.

## WP3 resumed under Amendment 1: D-2 lands

**Change on top of the stopped tree** (Amendment 1, conditional gate runs): the zero-room emission records every gate
run's end it emits, and, in route order, drops each one whose two neighbouring waypoints are joined by a leg the
segment predicate passes for the disc (`drop_unneeded_runs`); a meeting point within a run is replaced by the run's
end only when the neighbouring leg through it would pass within `r` of the gate's feature (`leg_needs_run`, the
predicate's exact Q16 point test). Two arcs that share a gate (a pinch between two rounded vertices) no longer walk
it forth and back. Measuring the new test corpus of WP4 found two more rounding cases, fixed here as part of D-2.5's
"a run on the side of a start or goal point that lies on the gate stops at that point":

- a start or goal lying on an axis gate next to its own tangent line is that line's meeting point with the gate,
  not its rounded image (which lay 0.0002 beyond the start, towards the map border);
- an inner waypoint closer than `ZERO_ROOM_SLACK` to the start or the goal is dropped (a rounded meeting point of a
  line through that end, a hair beyond it).

E1's test is restated test-only for Amendment 1: without unneeded runs its leg on `x == 1` ends just beyond the face
(`y` 48.90 and 51.05), so the test asserts one leg on `x == 1` spanning the whole face instead of waypoints on it.

**Fallback corpora (final tree):** 0 / 0 on the 150, 1,000 and 2,000 flush layouts (1,729 / 1,722, 11,932 / 11,838,
23,890 / 23,717 routes), no illegal leg.

**Stop rule 8 (oracle at `r − 10⁻⁵`, routes that fell back on `main`, square layouts):**

| Corpus | Within 1.01 | Max ratio | Fixed shorter than the oracle |
|---|---|---|---|
| 150 layouts | 14 / 14 | 1.0023 | never |
| 1,000 layouts (contains the 150) | 29 / 30 | 1.0258 (layout 563, TA*'s channel, Amendment 1) | never |
| 2,000 flush layouts | 94 / 94 | 1.0071 | never |
| All, distinct | **123 / 124 (99.2 %)** | 1.0258 | never (nearest 0.00075 longer) |

**D-5 battle set (`~/scratch/funnel-fallbacks/wp3`): bit-identical to `main`** (matrix file, battles 31 and 33
`seeds.csv`, showcase `units.csv` on nine seeds); reversals of record 4,659, give-ups 2 / 5, no flip. Control: with D-2
switched off by a scratch flag the same set is identical again (it equals WP2's and `main`'s). Rules 1 to 5 hold.

**Pins (rule 6), with proof headers:**

| Pin | Old | New | Requests that change |
|---|---|---|---|
| `contour_route_checksum` | `0xc0d5_fbfd_292f_37a4` | `0xb426_7a12_8c3a_f1b1` | 2 of 1,000, class E at r 0.5, fallback to legal (95.67 → 63.46; 115.55 → 63.68) |
| `CONTOUR_PATH_GOLDEN` | `0x6637_d70a_80d8_a859` | `0xab3f_b3c9_d6ae_df40` | 1 of 500, class E at r 1.0 under partial knowledge, fallback to legal (91.45 → 51.42) |

Both new values are identical in native debug, native release, `wasm32-wasip1` release and `wasm32-wasip1` debug
(wasmtime with the worktree mapped by `--dir`). With D-2 off the tree reproduces WP2's values, with D-2 and D-3 off
`main`'s. `golden_replay_tests` passes unchanged in all four builds. `PATH_GOLDEN`, `VALIDATION_GOLDEN`,
`DECODE_GOLDEN`, `EXPECTED_CHECKSUM` and `SHOWCASE_MESH_DIGEST` do not move (the `sim_map` and `sim_core` suites pass).

**Rule 7 (instructions retired, `perf stat -e instructions:u`, the host still loaded):** throughput per scenario
against `main`: +0.0004 %, −0.002 %, +0.0001 %, −0.002 %, +0.00003 %, **+0.55 %** (showcase, as in WP2), +0.0005 %;
path-cost bench 37.48 G against 35.85 G, **+4.5 %** (WP2 +4.1 %). The retry fires in no battle.

**Unit tests** (WP1's two, plan §6): `zero_room_legs_keep_the_disc_radius`, `gate_runs_keep_the_feature_across`, and
`a_circumscribed_arc_passes_an_exact_pinch` (the diagonal pinch keeps its counted fallback, Amendment 1 point 4).

## WP4 stopped: the exact-fit oracle test fails as the plan states it (trace and proposed amendment)

**What was built** (branch `funnel-fallbacks-wp4-wip`, `af96746d`, on WP3; not on the plan's branch):

- `mesh_route_lengths_are_within_one_percent_of_the_oracle` asserts zero fallbacks (it was "at most 2 %"). It
  passes: 522 of 523 within 1 %, worst 1.0220, **0 fallbacks**.
- `exact_fit_routes_agree_with_the_oracle`: 60 seeded 16 × 16 maps, one cell in six blocked, r 0.5 and 1.0
  alternately, six pairs of cell centres where the disc fits per map, oracle at `r − 10⁻⁵` (comment naming §4.3's
  reason). 293 routes; existence agrees on every pair, **no fallback, no illegal leg, none shorter than the oracle**,
  worst ratio 1.0491 (within `MESH_RATIO_BOUND`). It **fails** the plan's "every route within 1.01 of the oracle":
  routes with room keep the corner margin, so a margin-free oracle sees them longer (that part the test restates by
  comparing them only from below); but the routes the zero-room retry produced fail it too.

**Trace.** Among the 293 routes, 159 fell back with D-2 switched off (scratch flag), i.e. are the zero-room
retry's routes. Of these, **128 (80.5 %) are within 1.01**, median 1.0046, worst 1.0491 (map 36, r 0.5,
(12.5, 14.5) → (12.5, 0.5)). Dense grids of one-cell corridors make routes that round many corners in a row, and
the circumscribed polygon is longer than the arc it replaces: with bisection above an eighth turn (D-2.4) each
corner's polygon is up to `8 tan 22.5° / π ≈ 1.055` times its arc. A scratch switch that bisects above a sixteenth
turn instead gives **153 of 159 (96.2 %)** within 1.01; the remaining six (1.0181 to 1.0451, all r 0.5) are not
explained by the polygon and are not traced further here (candidates: TA*'s channel, as for layout 563, or the gate
runs). In the layout corpora of stop rule 8 the same retry routes are 123 of 124 within 1.01: they round few corners.
Every route of the new test is legal; the defect is length only.

**Proposed amendment (for the plan's owner).**

1. Restate WP4's exact-fit test with stop rule 8's measure: the zero-room retry's routes at least 95 % within 1.01 of
   the oracle at `r − 10⁻⁵`, none beyond `MESH_RATIO_BOUND`, none shorter, no fallback; routes that keep the margin
   compared from below only. A committed test cannot tell the retry's routes apart through the public API, so either
   `MeshRoute` gains a flag for a route the zero-room retry produced, or the 95 % applies to every route of the test
   that only an exact fit allows (the oracle with the margin finds none; 190 of 293 here).
2. And/or bisect the zero-room arcs above a sixteenth turn instead of an eighth (D-2.4; polygon at most
   `16 tan 11.25° / π ≈ 1.013` times the arc), measured above at 96.2 %. This changes the retry's routes, so the
   pins and corpora would be measured again.
3. Trace the six remaining routes above 1.018 before deciding the bound.

WP5 (bookkeeping) waits for WP4.
