# Blueprint: Funnel Fallbacks: Zero-Room Pinches and Vertices Outside the Channel (B-141)

This plan fixes bug **B-141** (`DESIGN.md` §4): the radius funnel falls back to portal midpoints for some routes
between points where the disc fits. The register names two classes:

1. **Exact fits.** The disc is exactly as wide as a bent channel, `(2r)² = w²`. TA* admits it, but no polyline
   rounds it under the strict segment predicate.
2. **Missed vertex.** A leg passes closer than `r` to a vertex outside the channel's portals and its start and goal
   triangles, and the funnel's refinement never considers that vertex.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`6b922732` (checksum schema 31). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.

**Why a full plan (charter §6).** The fix changes routes, so it can move battle outcomes and golden replays. It
re-pins two `sim_map` digests and adds a new retry to the route search. The prototype (§4) moved no battle, but the
stop rules must still hold in the clean implementation.

**Inputs.**
- Plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md): D-4 to D-7, the funnel and TA* design.
- Plan [`2026-11-18-contour-footprints`](2026-11-18-contour-footprints.md):
  - Amendment 1: the fitted retry, R-2.
  - Amendment 5: B-143, the oracle's smooth length.
- Its evidence [`2026-11-18-contour-footprints`](../evidence/2026-11-18-contour-footprints.md) §3b: the three
  fallback classes and `contours_add_no_funnel_fallbacks`.
- Plan [`2026-11-19-crowd-edge-steering-2`](2026-11-19-crowd-edge-steering-2.md): the placebo band, the 100-seed
  reading of near-tied battles (its Amendment 1) and the survivor-range stop (its Amendment 2).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14 | Standing rule | §2 |
| [Q0087](../questions/closed/0087-zergling-inner-radius.md) the owner's wall-off argument: workers pass a 0.707 gap (cited in contour-footprints Amendment 1) | Standing; D-1 keeps route existence | D-1 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| `docs/questions/open/` | Empty at planning time | — |

---

## 1. Summary

1. **Diagnosis (§3).** All of B-141's fallbacks between fitting endpoints fall into two disjoint classes, and the
   register's split is right with one correction:
   - **Class E, zero-room pinches** (the register's "exact fits"). Every one has a pinch of width exactly `2r`.
     Most of these pinches lie in the start or goal triangle, against a long wall edge of that triangle. The fitted
     retry measures channel width (`channel_width`) without those triangles, so it never runs for them, and the
     test's "exact fit" detector (a disc 1/4096 smaller) undercounts them. Every class-E fallback measured has
     `r = 0.5` or `r = 1.0`: whole-cell gaps of the terrain and of the map border.
   - **Class M, missed vertices.** A structure corner sits beyond an edge of the start or goal triangle. It is
     closer than `r` to the first or last leg. Neither the radius funnel's refinement nor the width test sees it.
2. **Decision on exact fits (D-1).** Exact fits stay admitted. The rule is one closed rule everywhere: a disc may
   touch an obstacle. TA* admits `w ≥ 2r`, the segment predicate refuses only `d < r`, and an endpoint fits at
   `d ≥ r`. The defect is the funnel's shape, not existence. A legal polyline exists through every exact pinch: it
   passes the pinch along the pinch's own tangent line, and that line is exact on our lattice.
3. **Design.**
   - **D-2, a zero-room retry.** It is the last retry before the fallback. It pulls the channel at `rho = r + ε`,
     where `ε = 2⁻¹²` is a rounding slack. Each arc is emitted as a circumscribed polygon: legs are tangent and the
     vertices lie outside the circle. Each pinch closer than `r + rho` becomes a **gate line** at half the pinch
     distance, followed for a short run on each side of the touch point.
   - **D-3, the refinement's candidate set.** It becomes every obstacle vertex within `rho` of a leg, found by a
     bounded flood around the leg. It applies in every pull.
4. **Prototype (§4, scratch).**
   - On the 150 layouts of `contours_add_no_funnel_fallbacks`, fallbacks drop to 0 of 1,729 with contours (from 11)
     and 0 of 1,722 with full squares (from 14).
   - On 1,000 such layouts, 55 fallbacks drop to 0. On 2,000 flush layouts, 136 drop to 0: 121 of class E and 15 of
     class M.
   - Against the exact oracle, the fixed routes are 1.0000 to 1.0078 times its length (94 square cases). The
     portal-midpoint fallbacks were 1.12 to 3.51 times, and 30 of 136 had an illegal leg.
   - The matrix, battle 33 (64 seeds), battle 31 (100 seeds), the showcase on nine seeds and both golden replays are
     **bit-identical**: there is no fallback in any of them on `main`.
   - Two `sim_map` pins move: `CONTOUR_PATH_GOLDEN` and `contour_route_checksum`.
5. **A third class, found while planning: B-146 (proposed).** Under partial knowledge, the funnel rounds portal
   vertices that are not obstacles, such as the corners of unknown structures. In `CONTOUR_PATH_GOLDEN`'s corpus 12
   of 22 fallbacks are of this kind, and 9 others are wedged starts (D-3 of the pathfinding plan). Both are out of
   scope (§9).
6. **Work packages (§6).**
   - WP0: records.
   - WP1: regression cases, red on `main`.
   - WP2: D-3.
   - WP3: D-2.
   - WP4: tests and pins.
   - WP5: measurement and bookkeeping.

---

## 2. What StarCraft II does

| Point | What is known | Tag |
|---|---|---|
| Navigation | A constrained Delaunay mesh, A* over triangles, a funnel. The unit's size is applied when the path is extracted, "a tunnel inside the expanded vertices" (pathfinding plan §2, S2 to S4) | 2ND |
| Width test | Demyen and Buro (S11): crossing a triangle is allowed when its width is at least `2r`, which is necessary and sufficient for a disc. They state `≥`; this plan's D-4 implements `4r² ≤ w²` | DOC (paper) |
| A unit that exactly fits | No source says whether StarCraft II lets a disc through a gap exactly as wide as itself, or how its funnel behaves there. The engine is understood to use floating point with tolerances (**verify**). A 1-cell gap between 5.0.14 footprints is wider than one cell because of the footprint `Shape` contours (contour-footprints §2), so the game's exact fits are rarer than our terrain's | — (**verify**) |
| Vertices outside the channel | S11's modified funnel rounds the channel's vertices. The width test bounds each crossed triangle, but not the start and goal triangles, which are entered from or left to a point. No source describes StarCraft II's handling (**verify**) | INF |
| Predicate shared by routes and movement | Ours (D-7 of the pathfinding plan). StarCraft II's movement is not documented at this level | — |

**What follows.** No source settles exact fits. The plan keeps S11's `≥` and makes the route's shape agree with it
(D-1). No sanctioned deviation of [Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) is touched.

---

## 3. Diagnosis

### 3.1 Method

The prototype worktree was a detached scratch tree at `6b922732`, since removed. Its diff is kept at
`~/scratch/funnel-fallbacks/patches/proto-E-M.diff`. It had four parts:

1. **Probe.** On a fallback, `search.rs` printed the channel, the anchors, every leg the segment predicate refuses,
   and the blocking edge nearest each such leg. Each endpoint of that edge was tagged `portal`, `endtri` or
   `OUTSIDE`, and the anchors that are not obstacle points were tagged `[FREE]`.
2. **Probe test.** `zz_b141_probe.rs` copied the generator of `navmesh_tests::fallback_counts`:
   - 150 or 1,000 layouts of random placements;
   - a "flush" variant of 2,000 layouts, which places the next structure against the previous one half the time,
     like `contour_meshes_build_for_seeded_layouts`.
3. **Variants, switched by environment variables.**
   - E: the zero-room retry, D-2.
   - M: the outside vertices in the retry.
   - MALL: the outside vertices in every pull, D-3.
4. **Instrumented release tools.** These counted fallbacks in battles.

Output is under `~/scratch/funnel-fallbacks/`.

**Reproduction on `main`.** The probe gives exactly the test's figures. With contours, 11 of 1,729 routes fall back
(6 "exact" by the test's detector). With full squares, 14 of 1,722 fall back (8 "exact").

### 3.2 Class E: zero-room pinches

**Mechanism.** The funnel rounds every vertex at `rho = r + CORNER_SAFETY_MARGIN` (0.05). Its refinement then inserts
every vertex closer than `rho` to a leg (`deepest_violation`).

- **Where the opposite obstacle is closer than `r + rho`,** rounding the vertex pushes the route into it.
- **The fitted retry was meant to shrink the margin there** (contour-footprints Amendment 1). Two things defeat it:
  - `fitted_margin` returns nothing when the spare is zero, so there is no retry at an exact fit.
  - `channel_width` measures left against right obstacles only across the fans between consecutive portals. The
    start and goal triangles are not included. A pinch against a long wall edge of those triangles is invisible, so
    the channel is reported several cells wide and no retry runs.
- **Even at margin zero, inscribed emission cannot work.** The current emission puts waypoints on the circle and
  lets chords dip inside it by half the margin. With zero margin every chord dips inside `r`, and the predicate is
  strict (`d < r` refuses).

**Concrete cases.** Each is reproduced on the default arena with one full-square structure, so it needs no seeded
layout. The prototype's `minimal` test checked all three.

| Case | Setup | Request | What happens on `main` |
|---|---|---|---|
| **E1, bend at a border pinch** | 2×2 at cell (2, 49): west face `x = 2` from (2, 49) to (2, 51) | r 1.0, (1.5, 56.5) → (3.5, 36.5) | The route turns round (2, 51) and (2, 49), both Left, between the face and the map border `x = 0`, a width of exactly 2 = 2r. It must run on `x = 1`. The shrunk pull puts it at `x = 0.95`, 0.05 inside `r`. **Fallback, 23.34 long; fixed 20.27** |
| **E2, goal in a pinch** | 3×2 at cell (7, 27), corner (10, 29) | r 0.5, (60.5, 45.5) → (9.5, 29.5) | The goal sits exactly between the corner (10, 29) and the cliff edge `y = 30`, which runs from (0, 30) to (30, 30) and is an edge of the goal triangle. `channel_width` says 4.0. The refinement inserts (10, 29) because the last leg passes within `rho` of it, rounds it at 0.55 and pushes the leg to `y = 29.55`, 0.45 from the cliff. **Fallback, 117.80 long; fixed 55.74** |
| **E3, a 1-wide zigzag** | Seeded layout 23 of the 150 (`fallback_counts` generator) | r 0.5, (25.5, 60.5) → (0.5, 0.5) | The corridor `x ∈ [27, 28]` between two structures is exactly 1 wide. The funnel alternates Left/Right anchors at `rho` 0.55 and each leg cuts 0.05 into the other wall. The fitted retry runs (width 1.0) but its margin is zero, so it is skipped |

**Why the test's detector undercounts.** `fallback_counts` calls a fallback an exact fit when a disc 1/4096 smaller
routes legally. For E2-type pinches in the start or goal triangle, the smaller disc also falls back, because the
fitted retry never runs there. By the probe, every fallback of the 150 layouts is a zero-room pinch: 11 of 11 with
contours and 14 of 14 with squares, not 6 and 8. Every class-E fallback in all corpora has `r` 0.5 or 1.0, the radii
whose `2r` is a whole cell.

**Can a legal polyline exist? Yes, and only along the pinch's tangent line.**

- **Setup.** Let `v` be a vertex and `F` the nearest point of the obstacle across, with `|F − v| = 2r`. The disc's
  centre must pass `M = (v + F)/2`, and at `M` the only legal directions are along the line `T` through `M`
  perpendicular to `F − v`.
- **Proof sketch.** A direction with a component towards `v` or towards `F` brings the centre closer than `r` to one
  of them.
- **Consequence.** Inscribed chords and tangent legs computed by square roots both fail, the first by geometry and
  the second by rounding. Only a run of the route that lies exactly on `T` is legal.
- **`T` is exact on our lattice.** Every exact pinch measured is axis-aligned: a lattice vertex against an axis edge
  or an axis-aligned vertex. So `T` is a line `x = c` or `y = c` with dyadic `c`. Distances to 45° chamfers are
  irrational multiples, `j / (4√2)`, so they are never exactly `2r` for a shipped radius. A diagonal exact pinch
  needs a Pythagorean pair of lattice points, for example offset (0.75, 1.0) for `r = 0.625`. There `M` is still
  exact (a midpoint), but `T`'s direction is not axis-aligned (**verify** in WP3's unit test, §6).

### 3.3 Class M: missed vertices

**Mechanism.** `deepest_violation` considers three kinds of vertex:
- the channel's portal vertices;
- the start and goal triangles' own vertices;
- nothing else.

A structure corner that lies beyond an edge of the start or goal triangle can still be closer than `r` to the first
or last leg. Inside the channel the width test (D-4) bounds what is across each crossed triangle. For the end
triangles there is no width test, only `reaches_edge`'s sampled straight sweeps, in the second pass.

**Concrete case M1.** The default arena with a 2×2 at cell (38, 6), r 0.625, (27.5, 2.5) → (40.5, 5.5).
- **The leg.** The channel has two portals and the funnel's route is the straight line. It passes 0.5996 from the
  structure's corner (40, 6), 0.025 inside `r`.
- **Why nobody sees the corner.** (40, 6) belongs to a triangle beside the goal triangle, outside the channel.
- **Result.** Fallback, 34.30 long, against a straight distance of 13.34; fixed 13.35, rounding (40, 6) on the right.

**Other cases found** (flush layouts):
- (49.5, 7.5) → (24.5, 47.5) at r 0.5: the corner (49, 7.5) is 0.415 from the first leg.
- (18.5, 43.5) → (38.5, 10.5) at r 0.5625: the corner (38, 11) is 0.537 from the last leg.
- (40.5, 5.5) → (23.5, 14.5) at r 0.5625, and (26.5, 1.5) → (55.5, 9.5) at r 0.625: the same pattern.

**Every class-M leg measured touches the start or the goal.** That agrees with the width test bounding the crossed
triangles. Even so, D-3 searches around every leg (§5), because no proof covers the funnel's legs inside the channel
(**verify**: no counterexample was found in 47,607 routes).

### 3.4 The classes are disjoint

On the 2,000 flush layouts (47,607 routes, 136 fallbacks):

| Variant | Contour | Square | Left |
|---|---|---|---|
| `main` | 42 | 94 | 136 |
| E only (D-2) | 6 | 9 | 15: all class M (r 0.5 to 0.875) |
| M only (outside vertices) | 36 | 85 | 121: all class E (r 0.5, 1.0) |
| E + M | **0** | **0** | 0 |

### 3.5 Where fallbacks happen on `main` in the game

Instrumented release tools on `main` (`~/scratch/funnel-fallbacks/base`) found **zero** fallbacks in all of these:
- the matrix: 1,160 runs;
- battle 33: 64 seeds;
- battle 31: 100 seeds;
- the showcase: nine seeds to tick 3,000;
- `golden_replay_tests`: both fixtures.

In `sim_map`'s pinned corpora, fallbacks occur in three places:

| Corpus | Fallbacks on `main` | What they are | What E + M fixes |
|---|---|---|---|
| The 150 layouts of `contours_add_no_funnel_fallbacks` | 11 contour, 14 square | All of class E | All 25 |
| `determinism_tests::contour_route_checksum` (1,000 routes) | 11 | 9 are wedged starts (`start_fits = false`), 2 are class E | The 2 |
| `CONTOUR_PATH_GOLDEN` (256 requests, half with partial knowledge) | 22 | 12 are class N (B-146), 9 are wedged starts, 1 is class E | The 1 |

---

## 4. Prototype results (scratch, not committed)

### 4.1 What was built

The prototype has three parts, all in `sim_map` and all switched off by default:
- the **zero-room retry** (D-2) after `fitted_retry`;
- the **outside vertices** in the refinement, either in that retry only (M) or in every pull (MALL);
- an **exemption**, in the zero-room retry, for a vertex whose nearest approach to a leg is the start or goal point
  itself. Without it, a start that touches a vertex at exactly `r` was rounded the long way round (one flush case).

In the prototype the outside vertices were all obstacle vertices of the mesh (brute force) and the pinches were found
by scanning every blocking edge. The plan replaces both scans with bounded floods (D-2.3, D-3).

### 4.2 Fallbacks

| Corpus | Routes (contour / square) | `main` | E + M |
|---|---|---|---|
| 150 layouts (the test) | 1,729 / 1,722 | 11 / 14 | **0 / 0** |
| 1,000 layouts | 11,932 / 11,838 | 25 / 30 | **0 / 0** |
| 2,000 flush layouts | 23,890 / 23,717 | 42 / 94 | **0 / 0** |

### 4.3 Route length against the exact oracle

The oracle is `tests/support/oracle.rs`, run at radius `r`, on full-square layouts, where its cell walls are exact.

| Corpus | Fixed route / oracle | Fallback / oracle | Fallbacks with an illegal leg |
|---|---|---|---|
| 150 layouts, 14 square cases | 1.0000 to 1.0025 | 1.109 to 1.926 | 1 of 14 square, 1 of 11 contour |
| 2,000 flush, 94 square cases | 1.0002 to 1.0078, none above 1.01 | 1.124 to 3.511, median 1.494 | 30 of 136 (both shapes) |

**Oracle caveat (WP4).** At radius exactly `r` the oracle missed the exact passage in 2 of the 14 cases, through its
float tolerance (`EPS` and one-degree arc samples). It then reported routes 0.976 and 0.899 times the mesh's. At
`r − 10⁻⁵` it finds them. A test that compares exact fits with the oracle must use that tolerance, the same lesson as
B-143.

### 4.4 Battles and goldens

Release tools ran the full measurement of crowd-edge-steering-2 (`measure.sh`). Retry-only (E + M) and MALL (E + M
in every pull) both reproduced `main` **byte for byte**:

| What | `main` | E + M (retry) | E + M (every pull) |
|---|---|---|---|
| Matrix, 1,160 runs | `matrix.csv` | identical; reversals of record 4,659; give-ups 2 / 5 | identical |
| Battle 33, 68 runs | blue 39 / 68 | identical `seeds.csv` | identical |
| Battle 31, 100 runs | blue 49 / red 17 / 34 | identical | identical |
| Showcase, nine seeds | player 1 at 1,935 on the default seed; 174 reversals | identical `units.csv` on all nine | identical |
| `golden_replay_tests`, release | pass | pass, unchanged | pass, unchanged |
| `sim_map`, every test | pass | `CONTOUR_PATH_GOLDEN` and `contour_route_checksum` change; all others pass, `PATH_GOLDEN` and `EXPECTED_CHECKSUM` included | the same two change, to other values |

Retries that fired in the battle runs: none. The difference between retry-only and every-pull, measured directly:

| Corpus | Requests whose route changes |
|---|---|
| 150 layouts | 7 of 4,800 |
| 500 flush layouts | 17 of 16,000 |

These are routes that are legal today but pass within the margin of an outside corner.

### 4.5 Cost

| Where | Cost |
|---|---|
| Fixed routes | 4 to 17 waypoints in the corpus. The polygon bisects arcs above 45°, against 2⁶ halvings in the inscribed emission |
| The retry | Runs only after three failed pulls: 136 of 47,607 corpus routes, none in any battle |
| Corpus runtime | Within the noise of a loaded machine (32.6 to 38.2 s for 47,607 routes, both variants) |
| The every-pull candidate set | The prototype's brute force made `sim_map`'s tests 5× slower, as expected, so it is not representative. D-3's flood is what WP2 benches |

---

## 5. Design decisions

### D-1. Exact fits are admitted: one closed rule

**Rule.** A disc may touch an obstacle. This holds for every check:
- TA*'s width test admits `4r² ≤ w²` (unchanged);
- the segment predicate refuses only `d < r` (unchanged);
- endpoint checks accept `d ≥ r` (unchanged).

**The two strict alternatives were considered and rejected.**

| Alternative | Why rejected |
|---|---|
| **Strict TA*** (`4r² < w²`) | It would refuse every whole-cell gap to r = 0.5 and r = 1.0 units (14 of the 42 catalog units, from `units.ron`). That changes route existence on shipped terrain. It contradicts S11, and it contradicts `a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` (`8t² ≤ 1`). It would also not fix E2, whose pinch is in the goal triangle, where the width test does not apply. And straight exact corridors would still pass through `route`'s straight check, so existence would depend on alignment |
| **Strict predicate** (refuse `d ≤ r`) | Contact resolution rests bodies exactly `r` from walls (contact plan D-5), so movement would refuse the positions bodies stand in |

**Consequence.** The funnel must produce a legal shape for every channel TA* admits whose endpoints fit (D-2). The
fallback remains only as a counted guard: it is no longer reachable in any measured corpus with fitting endpoints.

### D-2. A zero-room retry before the fallback

`MeshSearch::zero_room_retry` runs after `fitted_retry` fails, on the same channel. It pulls shrunk, then pinned,
with:

1. **Radius.** `rho = r + ZERO_ROOM_SLACK` and `ZERO_ROOM_SLACK = 2⁻¹²`.
   - This is a derived constant, not a tuning value: `2^(Q_BITS − 4)` Q16 units, so 16 grid steps.
   - It must exceed what the predicate's floor to Q16 and the radius's round-up can take from a tangent leg (2 grid
     steps), plus `SimScalar::sqrt`'s error, with room.
   - WP3's unit test proves the bound: tangent legs computed at `r + ε` pass the predicate at `r` for every shipped
     radius and every direction class.
2. **Refinement.** Candidates are those of D-3. A vertex is not inserted when its nearest approach to the leg is the
   leg's start or goal point (§4.1): that point already fits, so the approach is not the leg's to fix.
3. **Gates.** For each anchor `v`:
   - **Which obstacles.** Every blocking feature `F` not incident to `v` with `|F − v| < r + rho` gives a gate. The
     features are found by a flood from `v`'s fan across non-blocking edges within `r + rho` of `v`. This is the
     segment predicate's flood with a point as the segment. Bound: the triangles within `r + rho` of `v`.
   - **The gate.** Its unit normal is `u = (F − v)/|F − v|` and its offset `h = |F − v| / 2`, which is at least `r`
     because TA* admitted the channel. A gate whose normal lies outside the anchor's swept arc is ignored.
   - **Exactness.** For an axis edge, `F` is computed by clamping the coordinate, not by a projection, so `u` is
     exactly axis-aligned whenever the geometry is.
4. **Circumscribed emission.** Each anchor's arc becomes the boundary of the half-planes `(x − v)·n ≤ h` of these
   lines, in angular order (a diamond angle in fixed point; an exact cross-product order is acceptable too):
   - the incoming and outgoing tangent lines at `rho`;
   - bisecting lines at `rho` wherever two consecutive normals are more than 45° apart;
   - the gates.

   Three rules complete it:
   - **Redundant lines.** A line is dropped when its neighbours already meet inside its half-plane.
   - **Waypoints.** The waypoints are the consecutive intersections. An intersection on a gate line is snapped onto
     it by setting the gate's fixed coordinate exactly.
   - **Near-parallel lines.** These meet far away. That is the right answer (E2's last leg meets `y = 29.5` ten
     cells before the goal); only exactly parallel lines have no intersection.
5. **Gate runs.** A gate line is followed for at least `GATE_RUN(r)` on each side of its touch point `M`. Wherever a
   neighbouring intersection is closer to `M` than that, it is replaced by the exact point
   `M ± GATE_RUN · (gate direction)`.
   - **Why.** A neighbouring leg reaches the gate line at most `ε` off it. With a run `s` its dip against the feature
     across is bounded by `ε − s²/(2r)`. So `GATE_RUN(r) = 2^⌈log₂ √(2rε)⌉`, which is 2⁻⁵ for r = 1 and 2⁻⁶ for
     r = 0.5.
   - **The prototype.** It used 2⁻⁸ and passed every corpus. WP3 implements the derived value and re-measures; a
     shortfall is a stop (R-3).
   - **Where the run stops short.** A run on the side of a start or goal point that lies on the gate stops at that
     point (E2 ends at `M`).
   - **Point anchors.** A point anchor within `rho` of `v` gives the arc's end normal directly, as the direction to
     the point. The tangent to a point inside the circle is degenerate.
6. **What it does not change.**
   - A route legal under the shrunk, pinned or fitted pull never reaches this retry, so every such route is
     unchanged (§4.4).
   - If the retry fails, the portal-midpoint fallback and its counter remain. WP4 asserts the counter is zero on the
     corpora with fitting endpoints.
7. **Why a retry and not a new default.** The margin `CORNER_SAFETY_MARGIN` is the route's robustness against
   crowding (pathfinding Amendment 3). The zero-room shape keeps the margin wherever there is room, through the
   gates, and drops it only along the gate lines. It is the right shape only where the margin pulls failed.

**Alternatives rejected.**

| Alternative | Why rejected |
|---|---|
| Extend `channel_width` to the end triangles and let `fitted_margin` return 0 | It fixes nothing at zero spare: inscribed chords dip, and tangents computed by `sqrt` miss by rounding (prototype: refused by 5·10⁻⁸) |
| Per-vertex fitted margins with tangent circles of different radii | The tangents still touch circles of radius exactly `r`, with the same rounding failure. Gates give exactness only where it is needed |
| A tolerance in the predicate | The predicate is movement's (D-7) and promises "rounding can only refuse" |

### D-3. The refinement considers every obstacle vertex near a leg (class M)

1. **Candidates.** `deepest_violation` keeps its current candidates (portal vertices, the start and goal triangles'
   vertices) and adds every **obstacle vertex** (`NavMesh::vertex_blocks` under the request's access) within `rho`
   of the leg. The side is the side of the leg it lies on, as for the end-triangle vertices today.
2. **Search.** A new `segment.rs` query, `obstacle_vertices_near(mesh, access, scratch, a, b, rho, out)`, floods from
   the triangle of `a` across non-blocking edges within `rho` of the segment and collects the endpoints of blocking
   edges within `rho`. It reuses `SegmentScratch` and allocates nothing in steady state.
   - **Completeness** is the predicate's argument (module doc of `segment.rs`). The short path from the segment to a
     vertex within `rho` crosses only edges within `rho` of the segment, so the flood reaches it, or reaches a
     blocking edge nearer to it.
   - **Cost** is O(triangles within `rho` of the leg) per leg per refinement round. The rounds are already bounded
     by `4·items + 8`, which D-3 widens to count the added candidates.
3. **Scope: every pull.** One consistent rule: the margin belongs to obstacles, not to the channel's bookkeeping.
   - Measured cost to routes: 7 of 4,800 and 17 of 16,000 requests change, and no battle or golden replay.
   - The alternative, the retry only, keeps every legal route's bytes. It is the fallback decision if WP2's
     throughput stop rule fails (§7).
4. **Why not the vertices of the crossed triangles.** Class M's vertex belongs to no triangle the leg crosses; it
   belongs to a neighbour of the goal triangle (M1). A query against the obstacle set within `rho` is the smallest
   complete set.

### D-4. Determinism and state

- Fixed point only: Q32.32 with `SimScalar::sqrt` and exact lattice comparisons. No float and no `HashMap` (C7, C9).
- The gate and flood buffers live in `MeshSearch` and `SegmentScratch`. After warm-up they allocate nothing.
- **No hashed simulation state changes.** Routes are not hashed, only bodies, so the checksum schema stays 31.
- Iteration is in index order. The order of the gate lines is a total order: angle, then offset, then the index of
  the feature.

### D-5. Measurement method

The method is crowd-edge-steering-2's D-4 with Amendments 1 and 2 (`measure.sh`):
- the matrix on 16 seeds;
- battle 33 on 64 seeds;
- battle 31 on 100 seeds;
- the showcase to tick 3,000 on the default seed and seeds 1 to 8.

Two corpus measurements are added:

1. **Fallback corpus.** The 150-layout test, plus a scratch run on 1,000 layouts and 2,000 flush layouts, with
   fallback counts per class (the WP1 diagnostic, below).
2. **Oracle ratios.** The fixed routes of the square corpora against the oracle at `r − 10⁻⁵`.

**Stop rules.** A breach stops the WP. Nothing beyond the WP is committed. The trace names the first divergent
tick, the entity, the request and the class.

1. **Winner.** No overall winner flips in the matrix, on the showcase default seed or in battle 33. Battle 31, and
   any battle whose 20-run split is within one seed, is judged on 100 seeds. There it holds when the 100-seed
   winner is unchanged and the winner's share does not fall by more than the placebo band.
2. **Survivor ranges.** No seed of record leaves its battle's survivor range
   (`every_battle_keeps_its_recorded_outcome_on_every_seed`). This is decided by amendment, like a flip.
3. **Give-ups.** At most base + 1 on the matrix of record, on the matrix over all seeds, and on every showcase seed.
4. **Reversals.**
   - Expected: bit-identical (§4.4).
   - If the matrix moves at all, matrix reversals of record must stay within the placebo band of crowd-edge-2 §2.2,
     6,017 to 6,412, re-measured on WP1's tree if WP1's base moved.
   - A movement in the matrix, battle 31, battle 33 or the showcase is traced to a route that took the new retry or
     the new candidate set before the WP proceeds.
5. **Control runs.** Each new rule is switched off by a scratch flag, never committed, and must reproduce the
   previous tree byte for byte: the matrix, battle 33 `seeds.csv`, the showcase `units.csv` on nine seeds, and the
   `sim_map` pins.
6. **Goldens.**
   - `golden_replay_tests` is expected unchanged in debug, release and `wasm32-wasip1` release. A change is
     re-recorded only with a proof header: the first divergent tick, the entity, and the request that took D-2 or
     D-3, plus the control build reproducing the old value. Native debug, native release, `wasm32-wasip1` release,
     and `wasm32-wasip1` debug once.
   - The `sim_map` pins `CONTOUR_PATH_GOLDEN` and `contour_route_checksum` are re-recorded under the same rule. Each
     header counts the requests that changed and their class, and states that the old value is reproduced with both
     rules off. `PATH_GOLDEN`, `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `EXPECTED_CHECKSUM` and `SHOWCASE_MESH_DIGEST`
     must not move.
7. **Throughput** (`flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate
   alternated, three rounds):
   - every scenario at least 25 ticks/s;
   - every scenario within 5 % of base;
   - the path-search cost bench (WP2) within 10 % of base on the route corpora.
8. **Oracle.**
   - No fixed route is shorter than the oracle at `r − 10⁻⁵` by more than 10⁻³.
   - At least 95 % of fixed routes are within 1.01 of the oracle, and none is beyond `MESH_RATIO_BOUND`.
9. **Tests.** A test changed for a reason other than its WP's rule is restated first, test-only, with the trace. No
   bound is loosened and no tuning constant is added. `CORNER_SAFETY_MARGIN` stays 0.05.

---

## 6. Work packages (commit order)

Branch `funnel-fallbacks`. Implementation agents hand back after `cargo gate quick` and the tests their change needs.
A verifier runs `cargo gate package` on the final tree (Q0061). Each WP appends to a new evidence file,
`docs/evidence/2026-11-19-funnel-fallbacks.md`. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- `DESIGN.md` §4, B-141: the pointer becomes this plan, and the diagnosis is restated:
  - class E is zero-room pinches, most in the start or goal triangle;
  - class M is outside corners at the start or goal legs.
- `DESIGN.md` §4: register **B-146** (proposed, open): "under partial knowledge the radius funnel rounds portal vertices
  that are not obstacle points (corners of unknown structures), so routes zigzag and fall back (12 of 22 fallbacks
  of `CONTOUR_PATH_GOLDEN`'s corpus)". Pointer: this plan §9.
- The contour-footprints plan gets a one-line pointer amendment: its B-141 follow-up is this plan.
- **Gate:** docs-only `package`.

### WP1: regression cases, red on `main` (S, tests only)

**Tests first.** Each is deterministic, in `crates/sim_map/tests/funnel_tests.rs`, unless named otherwise. Each is
red on `main` (a portal-midpoint fallback) and passes when its WP lands. Until then each is `#[ignore]`d with the
reason "B-141, WP2" or "B-141, WP3", so `main` stays green.

| Test | Case | Asserts |
|---|---|---|
| `an_exact_border_pinch_is_rounded_on_its_tangent` (E1) | Default arena, full-square 2×2 at (2, 49), r 1.0, (1.5, 56.5) → (3.5, 36.5) | `fallback == false`; every leg passes the predicate; the waypoints between the two turns have `x == 1` exactly; length within 1.001 of 20.272 |
| `a_goal_in_a_pinch_is_reached_along_the_pinch` (E2) | Full-square 3×2 at (7, 27), r 0.5, (60.5, 45.5) → (9.5, 29.5) | No fallback; legal; the last leg lies on `y == 29.5`; length within 1.001 of 55.736 |
| `a_one_wide_zigzag_is_walked_down_its_middle` (E3) | A hand-built map in the test: two walls one cell apart with a bend, or layout 23 rebuilt by the `fallback_counts` generator | No fallback; legal; the corridor waypoints on its centre line |
| `an_outside_corner_is_rounded` (M1) | Full-square 2×2 at (38, 6), r 0.625, (27.5, 2.5) → (40.5, 5.5) | No fallback; legal; `turns` contains (40, 6) on the right; length within 1.001 of 13.350 |
| `a_start_touching_a_corner_does_not_circle_it` | Flush layout 1138's start (21.5, 48.5) at r 0.5, or a hand-built equivalent: a start exactly `r` from a structure corner | No fallback; no turn round that corner |
| `zero_room_legs_keep_the_disc_radius` (unit, `funnel.rs`) | Every shipped radius; tangent legs at `r + ZERO_ROOM_SLACK` in eight direction classes | Pass the predicate at `r` (D-2.1) |
| `gate_runs_keep_the_feature_across` (unit) | A gate line and neighbouring legs offset by `ε`, for r 0.3125 to 1.0 | Legal with `GATE_RUN(r)`; illegal with `GATE_RUN(r)/4` (shows the bound is needed) |

- **Diagnostic.** `MeshRoute` gains `fallback_class: Option<FallbackClass>` (`Wedged`, `Other`) only if the
  orchestrator wants it in production. Otherwise the per-class counts stay a scratch probe. The default is the probe,
  which adds no production surface.
- **Gate:** `quick`; `package` by the verifier.

### WP2: D-3, outside obstacle vertices (M, `sim_map`)

- **Change.**
  - `segment.rs`: `obstacle_vertices_near`, with a unit test against an exhaustive scan on seeded meshes.
  - `funnel.rs`: `deepest_violation` and `refine` take the near set, computed per leg per round; the round bound
    counts the candidates.
  - `search.rs`: pass the access and scratch.
- **Tests.** Un-ignore M1. Add `outside_vertices_match_the_exhaustive_scan` (proptest, 64 cases).
  `funnel_paths_are_legal_and_no_shorter_than_a_straight_line` and `mesh_routes_agree_with_the_exact_oracle` stay
  green.
- **Measure.**
  - The full D-5 set against `main`.
  - Path-search cost: a scratch criterion bench of `find_route_mesh` over the 150-layout corpus and the showcase
    route set, base and candidate alternated.
  - The `sim_map` pins: expected to move only `CONTOUR_PATH_GOLDEN` and `contour_route_checksum`, re-recorded per
    stop rule 6, or unchanged if WP2 alone does not touch their requests (the prototype measured them only with D-2
    too).
- **Exit.** The stop rules. **Throughput fallback decision:** if stop rule 7 fails, D-3 is restricted to the
  zero-room retry (§4.4: then legal routes keep their bytes) by amendment, and the measurement is repeated.

### WP3: D-2, the zero-room retry (M, `sim_map`)

- **Change.**
  - `funnel.rs`: gate lines, circumscribed emission, gate runs, the start/goal exemption. `radius_funnel` gains an
    `Emission` parameter (`Inscribed { dip }` today, `ZeroRoom { gates }`).
  - `search.rs`: `zero_room_retry` after `fitted_retry`.
  - `segment.rs`: `blocking_features_near(point, radius)` for the gates. It is the flood of D-3 with a point.
  - Constants `ZERO_ROOM_SLACK` and `GATE_RUN(r)`, each with its derivation in the doc comment (no magic numbers).
- **Tests.** Un-ignore E1, E2, E3 and the start case. The two unit tests. `navmesh_tests::contours_add_no_funnel_fallbacks`
  is strengthened to assert **zero** fallbacks for both shapes, which is the assertion Amendment 1 originally asked
  for. The proptest `funnel_paths_are_legal_and_no_shorter_than_a_straight_line` keeps its exemption only for
  wedged starts and partial knowledge (B-146), and counts them.
- **Measure.** The D-5 set; the pins per stop rule 6; throughput.
- **Exit.** The stop rules.

### WP4: tests and the oracle (S, tests only)

- `pathfinding_properties.rs`:
  - an exact-fit case in the oracle comparison, with the oracle at `r − 10⁻⁵` (§4.3) and a comment naming the
    reason;
  - `mesh_route_lengths_are_within_one_percent_of_the_oracle` asserts zero fallbacks among fitting endpoints
    instead of "at most 2 %".
- `pathfinding_properties::exact_fit_routes_agree_with_the_oracle`: seeded square layouts at r 0.5 and 1.0; every
  route within 1.01 of the oracle, none shorter.
- If a change to `crates/gate/suites.ron` is needed, it is measured (C22).

### WP5: measurement and bookkeeping (S)

- Evidence:
  - final D-5 figures;
  - the corpus tables of §4 recomputed on the final tree;
  - throughput;
  - the pins with their headers.
- `DESIGN.md` B-141 leaves the register as fixed, with the figures. B-146 stays open.
- `docs/plans/README.md`: this plan's row updated.
- No storyboard check is needed unless the showcase moved.

---

## 7. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `DESIGN.md` §4 (B-141, B-146), `docs/plans/README.md`, contour-footprints pointer amendment | WP0, WP5 |
| Evidence | `docs/evidence/2026-11-19-funnel-fallbacks.md` (new) | WP1 to WP5 |
| D-3 | `crates/sim_map/src/navmesh/segment.rs` (`obstacle_vertices_near`), `funnel.rs` (`deepest_violation`, `refine`), `search.rs` | WP2 |
| D-2 | `crates/sim_map/src/navmesh/funnel.rs` (emission, gates), `search.rs` (`zero_room_retry`), `segment.rs` (`blocking_features_near`) | WP3 |
| Tests | `crates/sim_map/tests/funnel_tests.rs`, `navmesh_tests.rs`, `pathfinding_properties.rs`, `support/oracle.rs` (tolerance only), unit tests in `funnel.rs` and `segment.rs` | WP1 to WP4 |
| Pins | `crates/sim_map/tests/validation_golden_tests.rs` (`CONTOUR_PATH_GOLDEN`), `determinism_tests.rs` (`contour_route_checksum`) | WP2, WP3 |

**Coexistence.** No `Active` plan in `docs/plans/README.md` owns `navmesh/` at planning time (**verify** at WP2's
start). `sim_core` is untouched.

---

## 8. Determinism, checksum and throughput

- Exact lattice comparisons and Q32.32 only; snapping to a gate's fixed coordinate is exact assignment.
- Gate lines are ordered totally (D-4).
- No new collection type, and no allocation in steady state: the scratch buffers live in `MeshSearch`.
- The checksum schema is unchanged: routes are not hashed.
- **Cross-platform.**
  - The two re-pinned `sim_map` digests are proven identical in native debug, native release, `wasm32-wasip1`
    release and `wasm32-wasip1` debug once.
  - `golden_replay_tests` runs on `wasm32-wasip1` release in WP2 and WP3.
- **Throughput.**
  - D-2 runs only on channels that fail three pulls: none in any measured battle.
  - D-3 adds one bounded flood per leg per refinement round in every pull. That is the risk, so stop rule 7 is
    tight (5 %) and has a fallback decision (WP2).
- **Charter.**
  - No identity in Rust (C5).
  - `// PERFORMANCE RATIONALE:` comments where a flood replaces a simpler scan, and where the gate search is limited
    to the anchor's neighbourhood.
  - Functions under 100 lines; the emission is split into gate collection, half-plane boundary and run insertion.
  - `thiserror` unchanged: no new error path; a failed retry is the existing fallback.

---

## 9. Out of scope and decisions recorded (Q0002)

| Item | State | Why not here |
|---|---|---|
| **B-146 (proposed): non-obstacle portal vertices rounded under partial knowledge** | Open (WP0 registers it) | 12 of `CONTOUR_PATH_GOLDEN`'s 22 fallbacks. The funnel treats every portal vertex as a circle, even a corner of an unknown structure that `vertex_blocks` calls free. `channel_width` then measures 0.5 to 0.707 for discs of 1.0 to 2.0. The fix (free portal vertices pulled as points, and `channel_width` restricted to obstacle points) changes routes under fog of war in battles, so it needs its own measurement. The direction is recorded here |
| **Wedged starts** (D-3 of the pathfinding plan) | Unchanged | Their legs are held to the centre only until the disc fits; a counted fallback remains there by design. 9 of `contour_route_checksum`'s 11 fallbacks, 9 of `CONTOUR_PATH_GOLDEN`'s 22 |
| Diagonal exact pinches (Pythagorean lattice pairs) | Handled by the predicate's verdict | None was found in 47,607 routes. Gates on a non-axis tangent are not snapped; WP3's unit test covers one (r 0.625, offset (0.75, 1.0)); if it fails, the counted fallback remains for that geometry and is recorded (**verify**) |
| Tuning `CORNER_SAFETY_MARGIN` | Not changed | Contour-footprints D-15 rule 7 |
| Local clearance triangulation (Kallmann, S12) | Not adopted | D-2 and D-3 close the measured classes without changing the mesh |
| A strict width test | Rejected | D-1 |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The clean D-3 flood costs more than the corpus suggests in crowded battles (many re-plans) | Throughput | Stop rule 7; the fallback decision of WP2 restricts D-3 to the retry |
| R-2 | A route in a battle reaches the new retry or candidate set in the clean implementation (none did in the prototype) | Battles move | Stop rules 1 to 5 with traces; the control flags attribute it |
| R-3 | The derived `GATE_RUN(r)` (2⁻⁵ to 2⁻⁶) changes outcomes where the prototype's 2⁻⁸ passed | A few corpus routes fall back or lengthen | WP3 re-measures all three corpora; the unit test pins the bound; a residual is traced, not tuned |
| R-4 | The zero-room shape hugs a wall along a gate line, and route following and crowding behave differently on it (no margin there) | Bodies scrape gate lines | Gates exist only where the room is below one margin; today those routes are portal midpoints, which are worse (1.12 to 3.5 times longer, 22 % with an illegal leg). The showcase and matrix have none |
| R-5 | The oracle's float tolerance hides exact passages (§4.3) | False test failures | WP4 runs the oracle at `r − 10⁻⁵` for exact-fit tests, with the reason stated |
| R-6 | B-146's free-vertex rounding interacts with D-3: a free vertex now added as a candidate | More zigzag under fog of war | D-3 adds only obstacle vertices (`vertex_blocks`); the existing free portal vertices are B-146's |
| R-7 | Pins re-recorded on one platform only | CI's determinism jobs fail | Stop rule 6 requires four builds with identical values |

---

## 11. Prototype material (scratch, not committed)

- Diff: `~/scratch/funnel-fallbacks/patches/proto-E-M.diff`. It contains:
  - the probe;
  - the environment switches `B141_E`, `B141_M`, `B141_MALL`, `B141_PROBE`, `B141_PROBE2` and `B141_STATS`, plus a
    `PROTO_DYN` override for tests;
  - the probe tests `probe`, `ratios`, `minimal` and `mall_diff` in `zz_b141_probe.rs`.
- Scripts: `~/scratch/funnel-fallbacks/scripts/measure.sh` and `summary.py`, copied from crowd-edge-2.
- Results:
  - `~/scratch/funnel-fallbacks/{base,retry,mall}/`: the matrix, battles 31 and 33, and the showcase on nine seeds;
  - `probe1.txt`, `probe1000.txt`, `probeflush.txt` and `residual.txt`: the probe dumps;
  - `ratios_flush.txt`: the oracle ratios.
- The prototype worktree and its target directory were removed after measurement.

## Amendment 1 (orchestrator, 2026-11-19): the gate run applies only where it is needed; WP2 deviations confirmed

**WP3 (stop rule 8).** The derived `GATE_RUN(r)` lengthens six of seven over-ratio routes at r = 1 (2⁻⁵). Only 117 of
124 fixed routes come within 1.01 of the oracle, where the rule needs at least 95 %.

**Decision:** adopt the agent's proposal from the evidence. The derived run stays as defined, but a gate line is
followed for it only where the leg leaving the gate would otherwise pass within `r` of the pinch's feature. That is
an exact check under the plan's one touching rule, so no constant is introduced, and 2⁻⁸ is not adopted. Stop rule
8 is not restated.

WP3 then re-measures all three corpora and the battle set. Its pins are re-recorded on native debug and release,
and on `wasm32-wasip1` release and debug, with proof headers. The seventh route (1,000-layout 563, ratio 1.0258)
comes from TA*'s channel choice, not from the funnel. It is reported and counts in the 95 % rule as measured.

**WP2 deviations, confirmed.**

1. D-2.2's start and goal exemption also applies to D-3's outside vertices in every pull, so a disc already
   overlapping a vertex at its start is not rounded around it. D-3.1 is amended accordingly, as the evidence
   proposes.
2. M1 rounds (40, 6) on `Side::Left`. The plan's "right" was a slip.
3. Throughput on a loaded host is judged on instructions retired per scenario. A wall-clock run on a quiet host
   stays an open confirmation item, not a blocker.
4. WP1's two unit tests land with WP3. The diagonal exact pinch keeps its counted fallback, as plan §9 anticipated.

## Amendment 2 (orchestrator, 2026-11-19): the zero-room arcs are bisected to a derived bound; the exact-fit test uses rule 8's measure

**Why.** WP4's `exact_fit_routes_agree_with_the_oracle` agrees with the oracle on existence for all 293 routes,
with no fallback, no illegal leg and none shorter. But only 128 of the 159 zero-room routes come within 1.01; the
worst is 1.049.

The cause is the arc bisection. A corner whose arc is bisected only above an eighth of a turn is emitted as a
circumscribed polygon. That polygon's length is `tan(θ/2)/(θ/2)` times the arc:

| Turn per segment | Length factor |
|---|---|
| 1/8 | 1.0548 |
| 1/16 | 1.0130 |
| 1/32 | 1.0032 |

A route of many consecutive corners therefore exceeds the oracle's 1.01 by construction.

**Decision.**

1. **The bisection bound is derived, not tuned.** In the zero-room retry only, the arc is bisected until each
   segment spans at most the largest power-of-two fraction of a turn whose circumscribed factor is at most the
   oracle bound 1.01. That is 1/32 of a turn. The derivation goes in the code's rationale block and in a unit test
   that computes it.
   - The other pulls are unchanged, so every route that is legal today keeps its bytes. Battles stay bit-identical,
     and the controls must show it.
   - Re-measure all four corpora and re-pin with proof headers on all four builds.
2. **The test is restated on stop rule 8's measure,** applied to every route that only an exact fit allows:
   - at least 95 % within 1.01;
   - none beyond `MESH_RATIO_BOUND`;
   - none shorter than the oracle;
   - existence agrees on every pair.
   Telling the retry's routes apart in the test may use a test-visible `MeshRoute` flag, if one is needed and
   documented.
3. **Trace whatever is still above 1.01** after item 1 (six routes were at 1.018 to 1.045 with a sixteenth turn).
   Record each trace in the evidence, and classify each as the TA* channel choice or a defect. A defect is a stop.
4. Then WP5.
