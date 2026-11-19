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
