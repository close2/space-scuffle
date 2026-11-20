# Evidence: Free Portal Vertices in the Radius Funnel (B-146)

Measurements for plan [`2026-11-19-partial-knowledge-funnel`](../plans/2026-11-19-partial-knowledge-funnel.md). Each
work package appends its section. The plan's prototype figures (its §4) were taken on `3c2f4c65`; the figures here
are taken on the implementation tree and its base, named in each section.

## WP0: records

- `DESIGN.md` §4: B-146 restated with the plan's diagnosis and pointed at this plan; the TA* budget item the plan
  drafted as B-147 is registered as **B-149** (B-147 and B-148 went to plan `2026-11-19-footprint-flags`), open.
- Plan `2026-11-19-funnel-fallbacks` Amendment 3 points its §9 B-146 row here.

## WP1: regression cases, red on `main`

Six cases, each `#[ignore]`d with the work package that fixes it. Run with `--include-ignored` on the WP1 tree
(`main`'s funnel at `bf4252cf`), every one fails:

| Test | Case | On `main` |
|---|---|---|
| `a_route_crosses_an_unknown_footprint_without_zigzag` (WP2) | N1, r 0.5, 0.75, 1.0 | r 0.5: the free turn (9, 11) is rounded by an arc, not emitted as a waypoint |
| `an_unknown_neighbour_makes_no_pinch` (WP2) | N2, r 0.5 | the zero-room retry |
| `open_ground_is_not_a_gap` (WP2) | N3, r 0.5 | the zero-room retry, eight turns |
| `a_free_end_inside_the_clearance_is_clamped_out` (WP3) | N4, r 0.75 | a portal-midpoint fallback (the plan's table says "legal but 21.03 long"; on a flat 24 × 24 map, as the test builds it, `main` falls back) |
| `a_free_end_across_a_narrow_portal_is_passed_on_its_far_side` (WP3) | the corpus request (42.75, 49.5) → (26.25, 19.5) at r 0.75 and 1.0 | a fallback |
| `golden_corpora_fall_back_only_from_wedged_starts` (WP3) | both golden path corpora | 8 and 13 fallbacks from fitting starts |

**The far-side layout (plan WP1, "verify").** The anchors `R(35, 48.5) L(35, 46.5)[FREE] R(34.5, 46)` of §3.5 are on
the showcase, and every vertex there belongs to one structure: the corpus's 5 × 5 contour (`5, 10, 6`) at cell
(30, 45), known per cell. Its chamfer corner (35, 46.5) is free (its cells are unknown) and (34.5, 46), where the
chamfer crosses the line between the known cell (34, 45) and the unknown (34, 46), is an obstacle point; the other
anchors are the showcase's ramp and cliff corners. So the test is the showcase with that one structure and the
corpus's 25 knowledge bits, at the corpus request. At the corpus's own radius 0.625 the two points are 0.707 apart,
more than `rho` 0.675, so the free corner is a plain D-1 point bend there (the prototype's adopted build turns at
it); the far-side rule applies from r 0.75 (`rho` 0.8), which the test uses, with r 1.0. On the prototype both radii
fall back with the rules off, and route without an anchor at (35, 46.5) with D-1 alone and with D-1 to D-3.

**N4's expected length.** The plan gives 15.614 for N4 under D-1 to D-3. The prototype diff
(`~/scratch/pkf/patches/proto.diff`) with its adopted switches (`PKF_POINTS`, `PKF_WIDTH`, `PKF_CLAMP`, `PKF_NEAR`)
reproduces both of the plan's would-be pins (`0x4062_b0b6_83dd_1afb`, `0x1da8_6365_bcbd_3b37`), so it is the adopted
design, and it routes N4 in 15.5103 (turns at the known (13, 10.5) and (13.5, 12)), on 24 × 24 and 28 × 28 alike.
The test expects 15.510.

The golden counting test shares each corpus's generator with its digest test (`path_corpus`,
`contour_path_corpus`); both digests are unchanged by that refactor. The unit tests `free_vertices_are_points` and
`the_clamp_clears_every_disc` exercise the free list and the clamp, which do not exist before WP2 and WP3; they land
with those work packages.

## How to reproduce

The measurement tree is the implementation with a scratch patch, never committed
(`~/scratch/pkf/impl/measure.diff` for WP2, `measure3.diff` for WP3): environment switches that turn each rule off
(`PKF_OFF_D1`, `PKF_OFF_D2`, `PKF_OFF_D3`, `PKF_OFF_D5`, all at once with `PKF_OFF=1`), the planner's per-request
probes (`PKF_PROBE`: a fallback's class and anchors; `PKF_LOG`: one line per routed request), and the planner's
scratch tests (`zz_pkf_corpus.rs`, `zz_pkf_probe.rs`) with funnel-fallbacks' all-known corpus
(`zz_b141_probe.rs`), copied to `~/scratch/pkf/impl/`. `~/scratch/pkf/impl/variant.sh <tag> [env…]` runs every
`sim_map` corpus of D-8 for one variant; `cmpcorpus.py` compares two layout corpora route by route. The battle set is
the planner's `~/scratch/pkf/scripts/measure.sh` with release `matchup_trace` and `route_trace` built from the
measurement tree, compared with `summary.py` against the planner's base (`~/scratch/pkf/base`, `main`'s code at
`3c2f4c65`; `bf4252cf`, this branch's base, differs from it in documents only). Throughput is the bench with a scratch
`BENCH_ONLY=<index>` switch (`bench-only.diff`), built from `main` and from each work package's tree without the
measurement patch, under `perf stat -e instructions:u`.

**Control.** With every rule off (`PKF_OFF=1`) the measurement tree reproduces `main` exactly: both path digests
(`PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`), every per-request line of both golden corpora, the layout corpora (fitting-start
fallbacks 11, 14, 17, 32; oracle 1,711 of 1,809 within 1.01, worst 1.3959), the stress search (496), and the battle
set byte for byte (`matrix.csv`, battle 33 and battle 31 `seeds.csv`, the showcase `units.csv` on all nine seeds).

## WP2: D-1, D-4 and D-5

**Change.** `funnel.rs`: the free list (`FunnelScratch::free`, sorted, filled once per `radius_funnel` call over the
distinct portal and end-triangle vertices); `sigma`, `tangent` and the emission read it, so a free vertex is a point,
emitted once with no arc and no gate; `shrink` takes per-end flags; `deepest_violation` skips free candidates.
`search.rs`: `channel_width` keeps only portal vertices that block the request. The two path pins move; with the rules
off the tree reproduces the old values.

**Path pins.** `PATH_GOLDEN` `0x254e_64e8_49c1_5bd2`, `CONTOUR_PATH_GOLDEN` `0x01f9_a949_e9c0_77ef`, the same values as
the prototype's D-1 and D-5 switches. Re-recorded in this WP with proof headers (the four builds below).

**Golden corpora** (the expectation is the plan's §4.2 "D-1 alone" row):

| Corpus | Fallbacks | Fitting-start | Wedged (free anchor) | New fallbacks | Changed: fallback → legal | Legal → legal shorter / longer |
|---|---|---|---|---|---|---|
| `CONTOUR_PATH_GOLDEN`, `main` | 21 | 13 | 8 (5) | — | — | — |
| `CONTOUR_PATH_GOLDEN`, WP2 | 9 | 2 | 7 (2) | 0 | 12 (and 1 wedged fallback → fallback) | 30 (least 0.913, median 0.9989) / **0** |
| `PATH_GOLDEN`, `main` | 13 | 8 | 5 (3) | — | — | — |
| `PATH_GOLDEN`, WP2 | 2 | 0 | 2 (0) | 0 | 11 | 38 (least 0.823, median 0.9967) / **0** |

Every changed request is under partial knowledge.

**Partial-knowledge layouts (§4.3), fitting-start fallbacks:** whole-structure squares 11 → 0, contours 14 → 1;
per-cell squares 17 → 4, per-cell contours 32 → 10. **New against `main`:** 0, 0, 2 (layouts 71 q 4 and 143 q 6,
r 1.0) and 5 (layouts 67 q 6, 128 q 3, 128 q 14, 134 q 6, 145 q 5). **Stress search:** 496 → 29 fallbacks, 10 new.
The all-known corpora stay at 0 (150 layouts: 1,729 and 1,722 routes; 1,000: 11,932 and 11,838; 2,000 flush: 23,890
and 23,717).

**Reading against rule 9.** The new per-cell and stress fallbacks are those of D-1 without D-2 and D-3: a free end
pulled as a point inside an obstacle's clearance (the plan's §4.2 and §4.3 measured D-1 with the swallow variants,
which had 8 new stress fallbacks; D-2 and D-3 are the plan's answer to them). The plan gives no "D-1 alone" figure for
the layouts or the stress search, so WP2 is judged on the golden corpora's row (met: 0 new, 2 and 0 fitting-start
fallbacks) and every new fallback is carried to WP3, which must remove them (the WP3 tree does: 0 of the 7 layout and 0 of the 10
stress fallbacks remain new except the plan's 2 named ones and one per-cell case the plan's own prototype has, see the
WP3 stop below). **Flagged for the
orchestrator:** the WP2 tree on its own adds 7 layout fallbacks and 10 stress fallbacks that `main` did not have.

**Oracle (§4.4's corpus):** 1,805 of 1,820 within 1.01 (99.2 %), worst 1.1343, none shorter than the oracle. Against
`main` 235 routes get shorter and 2 longer, by 0.38 % (layout 86 q 10, r 0.5, (4.5, 40.5) → (49.5, 0.5),
65.521 → 65.773) and 0.04 % (layout 8 q 6, r 0.5, (46.5, 25.5) → (40.5, 61.5), 46.379 → 46.397). **Traces:** both
take another channel. Layout 86 q 10 turns at (30, 34) on `main` and at (30, 34), (34, 30) and (41, 20) with D-1;
layout 8 q 6 passes (35, 29) on its left on `main` and on its right with D-1 (147 expansions against 336). D-1 changes
the funnel lengths of the channels TA* compares, so its incumbent and the anytime budget's cut-off change; the
channel `main` took is not reached within the budget. This is B-149's class (TA*'s budget), within rule 8's 0.5 %.

**Battles.** The matrix (1,160 runs), battle 33 (68 runs), battle 31 (100 runs) and the showcase on nine seeds are
**bit-identical** to the base (`matrix.csv`, `seeds.csv`, `units.csv` on every seed): no winner flips, reversals of
record 4,659, give-ups 2 of record and 5 over all seeds, showcase 174 reversals. The control build (`PKF_OFF=1`) is
bit-identical too. Rules 1 to 5 hold.

**Path-search cost** (§4.6's corpus: the 150 partial-knowledge layouts, whole-knowledge squares and contours and
per-cell contours, one release binary of the measurement tree, `perf stat -e instructions:u`):

| Variant | Squares | Contours | Per-cell contours | Total | Against `main` |
|---|---|---|---|---|---|
| `main` (`PKF_OFF=1`) | 20.61 G | 33.68 G | 41.10 G | 95.39 G | — |
| WP2 (D-1, D-4, D-5) | 11.56 G | 12.99 G | 18.71 G | 43.26 G | −54.6 % |
| WP3 tree (D-1 to D-5) | 12.38 G | 14.49 G | 19.98 G | 46.85 G | −50.9 % |

Rule 7's path-cost bound (no worse than `main` by more than 10 %) holds.

**Throughput (rule 7, instructions retired).** Each scenario of `cargo bench -p sim_core --bench throughput` alone
(`BENCH_ONLY`), `perf stat -e instructions:u`, under `flock ~/scratch/gate.lock`, `main` (`bf4252cf`) against the WP2
tree:

| Scenario | `main` (G) | WP2 (G) | Change |
|---|---|---|---|
| 64×64, 0 units | 1.86041 | 1.86041 | −0.00002 % |
| 64×64, 100 units | 6.20228 | 6.20228 | +0.00001 % |
| 64×64, 400 units | 30.90618 | 30.90618 | +0.00001 % |
| 128×128, 400 units | 31.22870 | 31.22870 | +0.00001 % |
| 256×256, 0 units | 29.14122 | 29.14122 | −0.0000004 % |
| Showcase canyon, 400 units | 53.37711 | 53.50959 | **+0.25 %** |
| 64×64, 67 casters, 400 units | 48.06543 | 48.06543 | +0.000003 % |

Every scenario is within 5 %. The showcase's cost is the free-vertex walk on its channels; its structures make
channels with free vertices. (The WP3 tree, measured from its first build, is within 0.3 % on every scenario too.)

**Re-measured on the merged base.** After `main` gained plan `2026-11-19-footprint-flags` (`fb66d85e`, five geysers
moved), the battle set was measured again: base built from `fb66d85e`, candidate and control from this branch's merge
of it. Candidate and control are bit-identical to the new base on every output; the new base's matrix is byte-identical
to the old one. Both path pins and `VALIDATION_GOLDEN`/`DECODE_GOLDEN` (as `main` re-recorded them) pass on the merged
tree.

**Pins, four builds.** With `PATH_GOLDEN` `0x254e_64e8_49c1_5bd2` and `CONTOUR_PATH_GOLDEN` `0x01f9_a949_e9c0_77ef`,
`validation_golden_tests` passes in native debug, native release, `wasm32-wasip1` release and `wasm32-wasip1` debug
(runner `wasmtime --dir=<worktree>`); `VALIDATION_GOLDEN` and `DECODE_GOLDEN` are unchanged.

**Tests.** N1, N2, N3 and the far-side case are un-ignored (the far-side case passes with D-1 alone, as the prototype
showed, so it lands here rather than in WP3); `free_vertices_are_points` (unit) and the proptest
`free_vertices_carry_no_circle` (64 cases, 24 × 24 maps, two to nine full-square structures each known at random) are
added. The proptest's property is that a turn at a free vertex at least `rho` from every obstacle (the border and every
known structure cell) is emitted as the vertex itself, once; the plan's "no waypoint strictly inside `rho` of a free
anchor" would also catch another obstacle's arc points passing near a free corner, which is legal, so it is not used.
On `main`'s funnel (the prototype with its switches off) the proptest fails on its first case.

## WP3 stopped: the clean `Anchor::Pass` costs one stress-search route (trace and proposed amendment)

The WP3 tree (D-2, D-3, `Anchor::Pass` as the plan's §5 D-2 states it) is kept on the side branch
`partial-knowledge-funnel-wp3-wip`, not committed here (stop rule 9 and the hand-back rule).

**What the WP3 tree measures** (D-1 to D-5; the plan's "adopted" row is the expectation):

| Measure | Plan (prototype) | WP3 tree | Rule |
|---|---|---|---|
| `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN` | `0x4062_b0b6_83dd_1afb`, `0x1da8_6365_bcbd_3b37` | the same | — |
| Golden fallbacks, fitting-start / wedged (free) | 0 / 3 (1); 0 / 6 (2) | the same | 9: met |
| Golden changed routes longer | 0 | 0 | 8: met |
| Layouts, fitting-start fallbacks (squares, contours, per-cell squares, per-cell contours) | 0, 1, 1, 5 | 0, 1, 1, 5 | 9: met |
| Layouts, new against `main` | 0 (§4.3 does not list any) | 1: per-cell contours layout 58 q 13 (the prototype has it too) | 9 first bullet: see finding 2 |
| Stress search fallbacks (new) | 10 (2) | **11** (2, the plan's named ones) | 9: **at most 10, breached** |
| Oracle within 1.01 / worst / shorter | 1,804 of 1,820 / 1.134 / 0 | 1,804 of 1,820 / 1.1343 / 0 | 8: met |
| All-known corpora | 0 | 0 on 150, 1,000 and 2,000 flush layouts | 9: met |
| Battles (matrix, battle 33, battle 31, showcase nine seeds) | bit-identical | bit-identical to the base | 1 to 5: met |

**Finding 1 (the stop).** The eleventh stress fallback is K2/U4 offset (1, 1), r 0.75, (8.5, 8.5) → (20.5, 20.5)
(known 3 × 3 contour `6, 6, 3` at (10, 10), unknown 5 × 5 contour `10, 10, 6` at (13, 11), flat 28 × 28 map). It
falls back on `main` too, so it is not new; the prototype routes it legally through the zero-room retry (18.29,
waypoints round (12.25, 10), (13, 10.75), (13, 12.25)). The difference is the plan's clean form of D-2: a clamped
free end is an `Anchor::Pass`, which `approached_at_an_end` does not take for an end, where the prototype marked it
`Anchor::Point`. In the zero-room pulls that exemption covers every candidate (funnel-fallbacks D-2.2), so the clean
form inserts a vertex the prototype skipped there, and the last retry fails. Traced by switching: treating `Pass` as
an end in `approached_at_an_end` alone restores the plan's 10 (and leaves both pins, the per-cell layouts and the
golden corpora unchanged); treating it as an end in `emit_zero_room`'s inside test alone does not (11).

**Finding 2 (a plan inconsistency, not caused by the implementation).** Rule 9's first bullet says no request of the
§4.3 layouts falls back that did not on base, but the plan's adopted prototype (`~/scratch/pkf/v-clampnear`) has one
such request, per-cell contours layout 58 q 13, r 0.9375, (42.5, 44.5) → (52.5, 7.5), and the WP3 tree reproduces it.
Class: no free anchor; the route's anchors are R(36.5, 37), a clamped point (37.29, 36.41), L(34, 34) and L(34, 30):
the clamp moves a free portal end next to (36.5, 37), and no pull with a margin, nor the zero-room retry, passes the
segment predicate there. `main` routes it legally. It is R-2's class (a clamped point leaves no legal pull).

**Finding 3 (the clamp's ties).** Plan D-2 says ties of `|t|` go "to the positive", D-7 that the candidates are
sorted by `(|t|, t)` (ties to the negative); the prototype sorts by `|t|` stably, in the flood's vertex order with each
disc's positive exit first. Exact ties occur on the lattice. Measured: "positive first" moves two golden requests
(one wedged fallback, and one legal route 41.143 → 41.114), raises the per-cell contour layouts to 6 fitting-start
fallbacks and the stress search to 11; `(|t|, t)` gives 6 and 2 fitting-start per-cell fallbacks and 12 stress
fallbacks. The WP3 tree uses the prototype's order, documented on `clamp`, which reproduces the plan's pins and
figures.

**The other WP3 measures.** D-3 off (D-1, D-2, D-5): one golden request starts to fall back (`CONTOUR_PATH_GOLDEN`,
75.77 → fallback), one golden route gets 0.48 % longer, per-cell contour layouts gain 3 new fallbacks, stress search 8
fallbacks with 3 new. D-2 off (D-1, D-3, D-5): per-cell contours 7 fitting-start fallbacks (1 new), stress 21 (4 new).
So D-3 and D-2 each remove new fallbacks the other leaves, as the plan measured; the decision to keep D-3 holds. The
other new stress fallbacks of WP2 are gone; the remaining stress residuals are flush contour pairs where the route
must pass between a known contour's chamfer and an unknown one (all class N), the 2 new ones the plan's named
K2/U1 (1, 0) r 0.75 and K2/U4 (1, 1) r 0.75 (7.5, 12.5) → (22.5, 13.5). Unit test `the_clamp_clears_every_disc`
passes (1,800 seeded ends at the nine shipped radii); N4 routes in 15.510 and the far-side case passes;
`golden_corpora_fall_back_only_from_wedged_starts` passes.

### Proposed amendment

1. **D-2's anchor kind.** `Anchor::Pass` is an end for `approached_at_an_end` (the exemption from rounding a candidate
   that comes nearest a leg at that leg's end), as the prototype's point was; it stays no end for `emit_zero_room`'s
   inside test and for `MeshRoute::turns`. Measured: every figure of the plan's adopted row, both pins. Or, if the
   orchestrator prefers the plan's text as written, raise rule 9's stress bound to 11 with this route named (it
   falls back on `main` too).
2. **Rule 9's first bullet** names layout 58 q 13 (per-cell contours) as an accepted residual of the adopted design,
   traced above, or the plan says what removes it.
3. **D-2 and D-7's tie rule** read: candidates sorted stably by `|t|`, in the order of the obstacle vertices by index
   with each disc's positive exit first.

## WP3 under Amendment 1: D-2 and D-3 land

**Change.** `funnel.rs`: `Anchor::Pass`; D-3 (`keep_clearance_corners`, exact squared distances, judged against the
list before removal); D-2 (`clamp_free_end` and `clamp`, both pulls, candidates and discs in `FunnelScratch`, the
items copied only when a clamp happens); `CLAMP_PAD` (2⁻²⁴, derived); `Pass` counts as an end in
`approached_at_an_end` only (Amendment 1); the clamp's ties in the measured order. `search.rs`: `MeshRoute::turns`
skips `Pass`. Measured on the tree merged with `main` `fb66d85e`; the control (`PKF_OFF=1`) reproduces `main` on
every corpus.

| Measure | Plan (adopted) | WP3 | Rule |
|---|---|---|---|
| `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN` | `0x4062_b0b6_83dd_1afb`, `0x1da8_6365_bcbd_3b37` | the same, four builds | 6: met |
| Golden fallbacks: fitting-start / wedged (free anchor) | 0 / 3 (1); 0 / 6 (2) | the same | 9: met |
| Golden changed routes | path 49 (10 fallback → legal, 1 wedged fallback → fallback 57.93 → 64.72, 38 shorter); contours 42 (15 → legal, 27 shorter) | the same | 8, 9: none longer, none new |
| Layouts, fitting-start fallbacks (squares, contours, per-cell squares, per-cell contours) | 0, 1, 1, 5 | 0, 1, 1, 5 | 9: met |
| Layouts, new against `main` | — | layout 58 q 13 only (accepted residual, Amendment 1) | 9 |
| Stress search: fallbacks (new) | 10 (2) | **10 (2)**: K2/U1 (1, 0) r 0.75 and K2/U4 (1, 1) r 0.75 | 9: met |
| All-known corpora | 0 | 0 (150, 1,000, 2,000 flush) | 9: met |
| Oracle (§4.4): within 1.01 / worst / shorter | 1,804 of 1,820 / 1.134 / 0 | 1,804 of 1,820 / 1.1343 / 0 | 8: met |
| Oracle: changed routes longer than `main` | 2 | 2: 0.38 % and 0.04 % (the WP2 traces, TA*'s channel) | 8: met |
| Path-search cost | −52 % | 96.81 G → 46.54 G, **−51.9 %** | 7: met |

**Residual traces** (class probe on the WP3 tree): whole contours layout 59 q 9 r 1.0 and per-cell layouts 8 q 2,
59 q 9, 104 q 3 are class N on `main` too (a free anchor beside a known face where the disc has no room); 104 q 15 and
58 q 13 are class OTHER, a clamped point beside a known corner (R-2). The stress residuals are flush contour pairs.
**Observation outside rule 8's corpus:** per-cell squares layout 19 q 8 (a wedged start, r 1.0) is 31 % longer than
on `main` (19.95 → 26.20): with D-2 and D-3 together TA* settles on the channel south of a structure; with either
alone it does not. It is a wedged start, which rule 8's corpus excludes.

**D-3 on and off** (recorded as WP3 asks): off, one golden request starts to fall back and one golden route is
0.48 % longer, the per-cell contour layouts gain 3 new fallbacks and the stress search 3 new; D-3 stays.

**Battles** (base built from `main` `fb66d85e`): the matrix (1,160 runs), battle 33 (68), battle 31 (100) and the
showcase on nine seeds are **bit-identical** to the base, and so is the control build (`PKF_OFF=1`). Rules 1 to 5
hold; `golden_replay_tests` is unchanged. Throughput: the WP3 code differs from the first WP3 build measured under
WP2's table only on pulls with a clamp, which the battle scenarios other than the showcase never run; that build was
within 0.3 % on every scenario (showcase +0.29 %).

**Tests.** N4 (15.510), `golden_corpora_fall_back_only_from_wedged_starts` (0 and 0) and the unit test
`the_clamp_clears_every_disc` (200 seeded ends at each of the nine shipped radii) pass.

## WP4: tests

- `PATH_GOLDEN` and `CONTOUR_PATH_GOLDEN` were re-recorded in WP2 and WP3 with proof headers; nothing to re-record.
- `funnel_paths_are_legal_and_no_shorter_than_a_straight_line` now places up to four seeded full-square structures
  and, in half its cases, routes with a known mask that knows each structure or not as a whole. Its B-146 exemption
  is gone: a fallback is exempt only from a wedged start. 64 cases pass.
- `partial_knowledge_routes_agree_with_the_oracle`: §4.4's generator at 40 layouts (489 routes from fitting starts),
  the oracle on the known-only map at `r − 10⁻⁵` (reason in its doc comment). 485 of 489 within 1.01 (99.2 %), worst
  1.1343 (B-149's TA* residual, reported, not asserted against `MESH_RATIO_BOUND`), none shorter, no fallback. About
  15 s in a debug build.
- `crates/gate/suites.ron` needs no change: both tests are in an existing target.

## WP5: bookkeeping

`DESIGN.md` B-146 is fixed, with the figures above and its residuals; B-149 stays open. The plan's index row is Done.
The showcase did not move (bit-identical `units.csv` on nine seeds), so no storyboard check is needed. Open
confirmation item: a wall-clock throughput run on a quiet host (the decision rests on instructions retired, as
funnel-fallbacks Amendment 1 settled for this host).

**After merging `main` `fd750285`** (plan `2026-11-19-parked-body-aim`, showcase reversals 174 → 132): the battle set
built from this branch is bit-identical to one built from `fd750285` (matrix, battles 33 and 31, the showcase on nine
seeds), and `validation_golden_tests` passes with both path pins in native debug and release and `wasm32-wasip1`
release and debug.
