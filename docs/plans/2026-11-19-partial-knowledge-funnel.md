# Blueprint: Free Portal Vertices in the Radius Funnel (B-146)

This plan fixes bug **B-146** (`DESIGN.md` §4): under partial knowledge the radius funnel rounds portal vertices that
are not obstacle points, such as the corners of structures the player does not know. Routes zigzag through the
unknown footprint, pinches appear where there is room, and some routes fall back to the portal midpoints.

**Status of this document.** This is a Stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`3c2f4c65` (checksum schema 31). Line numbers drift, so search for the quoted symbol. Claims not confirmed while
planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): …`.
The TA* budget item was drafted as B-147; that number and B-148 went to the structure-placement plan
(`2026-11-19-footprint-flags`), so WP0 registers it as **B-149** and this text uses that number.

**Why a full plan (charter §6).** The fix changes routes under fog of war, so it can move battle outcomes and golden
replays. It re-pins two `sim_map` digests, one of which (`PATH_GOLDEN`) the previous funnel plan held fixed. The
prototype (§4) moved no battle, but the stop rules must still hold in the clean implementation.

**Inputs.**
- Plan [`2026-11-19-funnel-fallbacks`](2026-11-19-funnel-fallbacks.md): §3.5 and §9, where B-146 was found and its
  direction proposed; its stop rules (§5 D-5) and its Amendments 1 and 2.
- Plan [`2026-11-09-sc2-pathfinding`](2026-11-09-sc2-pathfinding.md): D-2 (triangle labels and knowledge), D-3
  (wedged starts), D-4 (the width test), D-5 (TA*), D-6 (the radius funnel).
- Plan [`2026-11-18-contour-footprints`](2026-11-18-contour-footprints.md) Amendment 1: the fitted retry and
  `channel_width`.
- Plan [`2026-11-19-crowd-slide`](2026-11-19-crowd-slide.md): D-2 (survivor misses read against the placebos) and
  §2.2 (the placebo band on `6b922732`).

**Questions.**

| Question | State | Where |
|---|---|---|
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II 5.0.14 | Standing rule | §2 |
| Q0061 hand back after `cargo gate quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002 scoping is the agent's call if written down | Standing rule | §9 |
| `docs/questions/open/` | Q0090 only (splash stream, answered); nothing on pathfinding | — |

---

## 1. Summary

1. **Diagnosis (§3).** A structure the player does not know is open ground for its routes (pathfinding D-2), but its
   footprint is still triangulated: its outline and its interior cell lines are mesh edges, and their ends are mesh
   vertices. Those vertices are **free**: no edge at them blocks the request (`NavMesh::vertex_blocks` is false).
   TA*'s width test already treats a free corner as unbounded (`width_at`); the funnel does not. It gives every
   portal vertex a circle of `rho = r + margin`, shrinks every portal by `rho` at both ends, lets the refinement
   insert free vertices, and measures `channel_width` between them. On `main`:
   - **Zigzag.** A route across an unknown 4×4 rounds five of its interior lattice points (case N1: 24 waypoints,
     20.90 long at r 0.75, where 17.19 suffices).
   - **False pinches.** A free column one cell from a real face looks like a one-cell gap: every pull with a margin
     fails and the zero-room retry zigzags along the "pinch" (cases N2, N3).
   - **Fallbacks.** In `CONTOUR_PATH_GOLDEN`'s corpus 13 of 21 fallbacks are from fitting starts and round a free
     vertex; 5 of its 8 wedged-start fallbacks round one too. In `PATH_GOLDEN`'s corpus: 8 of 13, and 3 of 5.
   - **Wedged starts.** The 8 wedged fallbacks that round a free vertex belong here (D-6); the 5 that round none are
     starts on a wall line, the pathfinding plan's D-3 by design, and stay out of scope.
   - **Battles.** On the measured battle set no route rounds a free vertex on `main` (the matrix maps have no
     structures; on the showcase 34 of 37 funnel routes of the default seed have a free portal vertex, none rounds
     one).
2. **Design (§5).**
   - **D-1, free portal vertices are pulled as points:** no circle, no shrink at a free end, the vertex itself
     emitted.
   - **D-2, a free end inside an obstacle's clearance is clamped out of it:** a free portal end closer than `rho` to
     an obstacle vertex slides along its portal's line to the nearest point at least `rho` from every such obstacle.
     Where the obstacle is the portal's other end, the route passes the free vertex on its far side.
   - **D-3, a free vertex within `rho` of an obstacle vertex not across its own portal keeps a circle**, as on
     `main` (it is then a corner of the obstacle's clearance region, not open ground).
   - **D-4, the refinement never inserts a free vertex.**
   - **D-5, `channel_width` measures obstacle points only.**
   - One rule for every request: under full knowledge no portal vertex of either shipped map is free (measured), so
     every all-known route keeps its bytes.
3. **Prototype (§4, scratch).**
   - Golden corpora: fitting-start fallbacks 13 → 0 and 8 → 0; wedged starts with a free vertex 8 → 3. No request
     starts to fall back; of the 65 routes that change and stay legal, every one gets shorter (to 0.83 of its
     length) and none longer.
   - Partial-knowledge layouts (150, half the structures unknown): whole structures, squares 11 → 0, contours
     14 → 1; per-cell knowledge, squares 17 → 1, contours 32 → 5.
   - A stress search of known and unknown contour pairs: 496 fallbacks → 10, of which 2 are new (§4.3, R-2).
   - Oracle (squares, whole knowledge): 94.6 % → 99.1 % within 1.01, worst 1.396 → 1.134, none shorter. The 16 routes
     still above 1.01 are TA*'s anytime budget over the many small triangles of an unknown footprint, out of scope
     (**B-149** proposed).
   - The matrix, battle 33 (68 runs), battle 31 (100 runs), the showcase on nine seeds and both golden replays are
     **bit-identical**.
   - Path-search cost on the partial-knowledge corpora: 97.4 G → 46.9 G instructions (−52 %): the zigzag and the
     retries cost more than the free-vertex test. The throughput benchmark retires 0.14 % more instructions (§4.6).
   - Two `sim_map` pins move, `PATH_GOLDEN` and `CONTOUR_PATH_GOLDEN`; every other `sim_map` test passes unchanged.
4. **Work packages (§6).** WP0 records; WP1 regression cases, red on `main`; WP2 D-1, D-4, D-5; WP3 D-2 and D-3;
   WP4 pins and tests; WP5 bookkeeping.

---

## 2. What StarCraft II does

| Point | What is known | Tag |
|---|---|---|
| Navigation | A constrained Delaunay mesh, A* over triangles, a funnel; the unit's size applied when the path is extracted, "a tunnel inside the expanded vertices" (pathfinding plan §2, S2 to S4) | 2ND |
| Which vertices are expanded | S11's modified funnel puts a circle of radius `r` round the channel's vertices. In S11 every mesh vertex is an obstacle vertex, because the triangulation is of the obstacles alone, so "channel vertex" and "obstacle vertex" coincide. S4's "expanded vertices" are obstacle vertices by the same reading. No source describes a mesh vertex that bounds nothing | DOC (paper), INF |
| Width test | S11 computes a corner's width only for obstacle corners; our `NavMesh::width_at` returns unbounded for a free corner ("a passage past it is checked where it narrows") | DOC (paper) |
| Unknown structures and pathing | Our knowledge rules (pathfinding D-2, the `find_path_with_vision` invariants): a structure the player has not seen is walked through until it is seen, and routes are re-planned then (`recheck_routes`). Whether StarCraft II paths through unseen enemy buildings is not documented in any source the pathfinding plan found (**verify**). This plan does not change the knowledge rules, only the shape of a route through open ground | — (**verify**) |
| Non-blocking constraints | Our mesh has `EdgeKind::Seam` constraints (cliff-level and ramp tags, structure interior lines). StarCraft II's tagged regions (S2) are not documented at this level | INF |

**What follows.** Rounding a vertex that bounds nothing is not part of S11's funnel; it is an artefact of a mesh that
carries non-obstacle vertices. D-1 restores S11's funnel on the obstacles the request actually has and makes it agree
with the width test TA* already applies. No sanctioned deviation of
[Q0052](../questions/closed/0052-sanctioned-sc2-deviations.md) is touched, and no new one is introduced.

---

## 3. Diagnosis

### 3.1 Method

The prototype was a detached scratch worktree at `3c2f4c65`, since removed. Its diff is kept at
`~/scratch/pkf/patches/proto.diff`. It had four parts:

1. **Probe** (`search.rs`, environment switches). On a fallback it prints the request, the failed route's anchors
   with each vertex tagged `[FREE]` when `vertex_blocks` is false for the request, a class (`WEDGED` when the disc
   does not fit at the start, else `N` when an anchor is free, else `OTHER`), and for each leg the predicate refuses
   the nearest wall point along it. A second switch prints one line per routed request (length, waypoints, whether a
   portal or anchor vertex is free), so two builds are compared request by request (`cmp.py`, `cmp2.py`).
2. **Variants** (switches, off by default): `POINTS` (D-1 and D-4), `WIDTH` (D-5), `CLAMP` (D-2), `NEAR` (D-3), and
   the rejected variants of §5 (`SWALLOW2`, `SWALLOW`, `SWALLOW3`, `LEGACY`).
3. **Scratch tests.** `zz_pkf_probe.rs`: the hand-built cases N1 to N3 and a stress search over pairs of known and
   unknown contours. `zz_pkf_corpus.rs`: the generator of `navmesh_tests::fallback_counts` with half the
   structures unknown (whole, or per cell at random as in the goldens), the oracle run on a map holding the known
   structures only.
4. **Release trace tools** (`matchup_trace`, `route_trace`) built from the prototype, so one binary measured base
   (switches off, `main`'s code paths) and candidate.

Output is under `~/scratch/pkf/`.

### 3.2 Free vertices under partial knowledge

- **Where they come from.** A structure's outline and its interior cell lines are constraints of the mesh
  (`navmesh/mod.rs` module doc), so a structure triangle lies in one cell and knowledge stays per cell. For a
  request that does not know a cell, its triangles are open (`Access::open`), and an edge blocks only when it is a
  `Wall` or one side is closed (`edge_blocks`). The outline and the interior lines of an unknown structure are
  therefore open, and their vertices are free.
- **Partly known structures.** Knowledge is per cell in battles too (`update_known_structures` marks the cells a
  player sees). The edge between a known and an unknown cell of one structure blocks, so its vertices are obstacle
  points; the free vertices are those wholly inside the unknown part.
- **Under full knowledge.** On both shipped maps, with every structure known, no portal vertex of any routed
  all-known request of the two golden corpora is free (173 routes). Seam vertices exist, but every one a channel
  meets lies on a wall.

### 3.3 What the funnel does with a free vertex

`radius_funnel` builds every portal as `(Anchor::Vertex(l, Left), Anchor::Vertex(r, Right))`. Then:

1. `string_pull`'s shrunk pull moves both ends of every portal inwards by `rho`; a portal of at most `2 rho` shrinks
   to its midpoint. Inside an unknown footprint every portal is a cell edge or diagonal, so for `r ≥ 0.5` the shrunk
   pull runs through portal midpoints.
2. `tangent` treats every vertex anchor as a circle of `rho` (`sigma` is ±1 for every vertex).
3. `deepest_violation` inserts any portal or end-triangle vertex within `rho` of a leg, free or not.
4. `channel_width` measures left against right portal vertices, free or not: a channel across an unknown footprint
   reports 0.5 to 0.707 wide.

TA* admitted the channel with free corners unbounded, so the funnel is held to a narrower channel than the search
admitted.

### 3.4 Concrete cases

Flat 24 × 24 map, full-square structures, a known mask with only the listed structures known (`zz_pkf_probe.rs`).
They become WP1's tests.

| Case | Setup | Request | `main` | Prototype (adopted design) |
|---|---|---|---|---|
| **N1, zigzag across an unknown footprint** | Known 2×8 at cell (6, 2); unknown 4×4 at (9, 9) | (3.5, 4.5) → (15.5, 14.5) | r 0.5: 17.665, 16 waypoints; r 0.75: 20.898, 24 waypoints, turns at (6, 10) and the free (9, 12), (9, 13), (10, 12), (10, 13), (11, 13); r 1.0: 21.935 | r 0.5: 16.975; r 0.75: 17.192 (a point bend at (9, 12)); r 1.0: 17.432. With the 4×4 known: 17.504 and 18.087 round it |
| **N2, a false pinch** | Known 3×3 at (8, 8); unknown 3×3 flush east at (11, 8) | (9.5, 5.5) → (9.5, 14.5) | r 0.5: zero-room retry, 8 turns zigzagging between the known face `x = 11` and the free column `x = 12`, 10.274 | r 0.5: full margin, round (8, 8) and (8, 11), 10.329 (the mirror route east is equally long) |
| **N3, open ground taken for a gap** | Known 3×3 at (8, 8); unknown 3×3 at (12, 8) | (9.5, 5.5) → (12.5, 14.5) | r 0.5: zero-room retry, 8 turns, 9.872 | r 0.5: one turn at (11, 8), 9.829. **r 1.0: 10.358 against 10.183** on `main` (D-3's cost, §4.3) |
| **N4, a free end inside the clearance** | Known 3×3 contour (half 6, chamfer 2 quarters) at (10, 10); unknown 2×2 contour (4, 2) at (13, 10) | (12.5, 7.5) → (12.5, 22.5), r 0.75 | legal but 21.03 long, 25 waypoints through the unknown contour | D-1 alone: **fallback** (the free corner (13.5, 10) is 0.707 from the known chamfer corner (13, 10.5)); D-1 to D-3: 15.61 |

### 3.5 The corpora on `main`

Fallbacks by class, from the probe (wedged takes precedence; "free" means an anchor of the failed route is free):

| Corpus | Requests | Fallbacks | N (fitting start, free anchor) | Wedged, free anchor | Wedged, no free anchor |
|---|---|---|---|---|---|
| `CONTOUR_PATH_GOLDEN` (`find_path_with_vision_on_contours_…`; half the requests with a 50 % per-cell known mask) | 500 | 21 | 13 | 5 | 3 (2 all-known) |
| `PATH_GOLDEN` (`find_path_with_vision_matches_…`; three quarters with vision or a known mask) | 500 | 13 | 8 | 3 | 2 |

**The register's "12 of 22".** That count was taken at `6b922732`, before funnel-fallbacks WP2 and WP3 moved two
requests of this corpus. The figures above are on `3c2f4c65`.

**Every class-N fallback rounds a free vertex next to a real obstacle.** Typical anchors (contours corpus, showcase,
r 0.75): `R(35, 48) L(35, 46.5)[FREE] R(34.5, 46)`. The free vertex is 0.707 from the obstacle across, less than
`rho` 0.8, so its circle pushes the route into the obstacle.

### 3.6 Wedged starts: which belong here

The funnel-fallbacks plan put the wedged-start fallbacks of this corpus out of scope. They are two kinds:

- **A wedged start whose route rounds a free vertex** (8 in the two golden corpora). The free vertex is B-146's
  defect; the start being wedged does not change it. **These belong here** (D-6).
- **A wedged start on a wall line** (5, two of them all-known). Each starts exactly on a cell line (x 38.0, 40.0,
  46.0, 19.0, or the border x 0.0), the disc's centre on or a hair from a wall. Their legs out of the wall are held
  to the centre only (pathfinding D-3), and the first leg at the full radius fails. This is D-3's counted fallback,
  unrelated to knowledge: **out of scope**. Whether a body's centre can stand on a wall line in the simulation
  (contact keeps `r`) is **verify**; if it cannot, these are corpus artefacts.

---

## 4. Prototype results (scratch, not committed)

### 4.1 What was built

All in `sim_map`, switched off by default:

- **D-1 and D-4** (`POINTS`): per pull, the free vertices among the channel's portal and end-triangle vertices
  (`vertex_blocks` under the request's access); `sigma` 0 for them, so tangents treat them as points and the
  emission emits the vertex once; the shrunk pull shrinks only obstacle ends; `deepest_violation` skips free
  candidates.
- **D-2** (`CLAMP`): for each free portal end, the obstacle vertices within `rho` (the segment predicate's flood,
  `obstacle_vertices_near`, with a point as the segment); if any, the end slides along the portal's line to the
  nearest point at least `rho` from all of them, and becomes a point anchor there.
- **D-3** (`NEAR`): a free vertex within `rho` of an obstacle vertex of the channel that is not its own portal
  partner is taken out of the free list (rounded as on `main`).
- **D-5** (`WIDTH`).

### 4.2 The golden corpora

| Corpus | Variant | Fallbacks | Fitting-start | Wedged (free anchor) | New fallbacks | Changed legal routes longer |
|---|---|---|---|---|---|---|
| `CONTOUR_PATH_GOLDEN` | `main` | 21 | 13 | 8 (5) | — | — |
| | D-1 alone | 9 | 2 | 7 | 0 | 0 |
| | D-1 + swallow across the portal only (`SWALLOW2`, rejected) | 8 | 1 | 7 (2) | 0 | 0 |
| | D-1 + D-2 (clamp alone) | 8 | 2 | 6 | **1** | 1 (0.5 %) |
| | **D-1 to D-5 (adopted)** | **6** | **0** | **6 (2)** | **0** | **0** |
| `PATH_GOLDEN` | `main` | 13 | 8 | 5 (3) | — | — |
| | D-1 alone | 2 | 0 | 2 | 0 | 0 |
| | `SWALLOW2` | 2 | 0 | 2 (0) | 0 | 0 |
| | **D-1 to D-5 (adopted)** | **3** | **0** | **3 (1)** | **0** | **0** |

D-5 changes no route of either corpus. It is kept because the fitted margin of contour-footprints Amendment 1 means
the room between obstacles.

**Requests that change** (adopted design against `main`, every routed request compared):

| Corpus | Routed | Changed | Fallback → legal | Fallback → fallback | Legal → legal | Of those, longer |
|---|---|---|---|---|---|---|
| `CONTOUR_PATH_GOLDEN` | 258 | 42 | 15 | 0 | 27 (median 0.9992 of `main`, least 0.913) | **0** |
| `PATH_GOLDEN` | 174 | 49 | 10 | 1 (wedged: 57.93 → 64.72) | 38 (median 0.9968, least 0.826) | **0** |

Every changed request is under partial knowledge; no all-known request changes. The would-be pins (native, release):
`PATH_GOLDEN` `0x4062_b0b6_83dd_1afb`, `CONTOUR_PATH_GOLDEN` `0x1da8_6365_bcbd_3b37`.

### 4.3 Partial-knowledge layouts and the stress search (scratch)

`fallback_counts`' generator (default arena, 2 to 13 structures, 16 requests per layout, the goal where the disc
fits), 150 layouts, half the structures unknown. Fallbacks from fitting starts:

| Knowledge | Shape | Routed | `main` | D-1 + `SWALLOW2` | D-1 to D-5 (adopted) |
|---|---|---|---|---|---|
| Whole structures | Squares | 1,964 | 11 | 0 | **0** |
| Whole structures | Contours | 1,971 | 14 | 1 | **1** |
| Per cell, 50 % | Squares | 1,920 | 17 | 4 | **1** |
| Per cell, 50 % | Contours | 1,927 | 32 | 10 | **5** |

Per-cell random knowledge (as in the golden corpora) is harsher than battles, where a player sees contiguous areas.

**Stress search** (`zz_pkf_probe::search`): a known contour of each of five 5.0.14 shapes at (10, 10), an unknown one
of each shape beside it at 16 offsets, r 0.5 to 1.0, five requests passing the pair; flat 28 × 28 map.

| | Fallbacks | New against `main` |
|---|---|---|
| `main` | 496 (and 99 zero-room routes) | — |
| D-1 + `SWALLOW2` | 26 | 8 |
| D-1 + D-2 + D-3 + `SWALLOW2` | 21 | 4 |
| D-1 + D-2 (clamp alone) | 8 | 3 |
| **D-1 to D-5 (adopted)** | **10** | **2** |

The two new fallbacks (K2/U1 offset (1, 0) r 0.75 and K2/U4 offset (1, 1) r 0.75, §3.1's naming) are flush contour
pairs where `main` found a legal zigzag through the unknown contour. No variant measured reaches zero new here; they
are R-2's cases, traced in WP3.

**D-3's cost (N3 at r 1.0).** The free corner (12, 8) of the unknown structure is 1.0 from the known corner (11, 8),
within `rho` 1.05, so D-3 rounds it and the route is 10.358 against `main`'s 10.183 (+1.7 %). No changed route of the
golden corpora gets longer; the oracle corpus has two (§4.4). D-3 is adopted because it removes 3 of 5 new stress
fallbacks of the clamp alone, the golden corpus's new fallback and its other residual, at this measured cost.

### 4.4 Route length against the exact oracle

Square layouts, whole-structure knowledge, routes between fitting points; the oracle runs on a map holding only the
known structures (the same free space) at `r − 10⁻⁵` (funnel-fallbacks §4.3):

| | Routes compared | Within 1.01 | Worst | Shorter than the oracle |
|---|---|---|---|---|
| `main` | 1,809 | 1,711 (94.6 %) | 1.396 | 0 |
| Adopted design | 1,820 | **1,804 (99.1 %)** | 1.134 | 0 |

Against `main`, 207 routes get shorter, 2 longer (by 0.38 % and 0.04 %: TA*'s incumbent changes with the funnel's
lengths; WP2 traces both), the rest are unchanged.

**The routes above 1.01 are TA*'s channel, not the funnel.** Routed on the map without the unknown structures, 14 of
the 15 traced come within 1.003 of the oracle (the fifteenth is 1.019 either way). Their extra turns are point bends
at free corners: TA* settled on a channel round the unknown footprint's outline instead of across it. With the
anytime budget unbounded and 64 visits per state (scratch), 1,817 of 1,820 are within 1.01 and the worst is 1.040.
The many small triangles of an unknown footprint exhaust `NAV_TA_STAR_EXTRA_EXPANSIONS` (256) and
`NAV_TA_STAR_TRIANGLE_VISITS` (4), which were measured on known terrain only (pathfinding WP8). Out of scope (§9),
proposed as **B-149**.

### 4.5 Battles and goldens

Release tools of the prototype, the full measurement of funnel-fallbacks D-5 (`~/scratch/pkf/scripts/measure.sh`):

| What | Base | `SWALLOW2` candidate | Adopted design |
|---|---|---|---|
| Matrix, 1,160 runs | reversals of record 4,659, all seeds 22,666; give-ups 2 / 5; expectation failures 0 / 4 | identical `matrix.csv` | **identical `matrix.csv`** |
| Battle 33, 68 runs | blue 39 / 68 | identical `seeds.csv` | identical |
| Battle 31, 100 runs | blue 49 / red 17 / 34 | identical `seeds.csv` | identical |
| Showcase, nine seeds | player 1 at 1,935 on the default seed; 174 reversals | identical `units.csv` on all nine | identical |

The base is bit-identical to crowd-slide's base on `6b922732` (`matrix.csv`, battle 33 `seeds.csv`, showcase
`units.csv`), so that plan's placebo band [4,633, 4,698] and its P1 finding on battle 26 seed 33 apply to this base
(crowd-slide D-3 asks for a re-measure on a new base; here the battle outputs are byte-identical, so the band is the
same). The base was measured with the prototype's first build, switches off; the adopted design with a later build
of the same diff. A flags-off control of the final build is WP2's rule 5.

### 4.6 Cost

**Path search** (instructions retired, `perf stat -e instructions:u`, the scratch layout corpus of §4.3 without the
oracle, whole-knowledge squares and contours and per-cell contours, 5,862 routed requests; the setup, map building
and mesh construction are the same in every variant):

| | Instructions | Against `main` |
|---|---|---|
| `main` | 97.44 G | — |
| D-1 + `SWALLOW2` | 43.72 G | −55 % |
| D-1 + D-3 + `SWALLOW2` | 45.26 G | −54 % |
| **D-1 to D-5 (adopted)** | **46.95 G** | **−52 %** |

The free-vertex test costs less than what it saves: the zigzag's refinement rounds, the retries after illegal pulls
and the second TA* pass that a failed route triggers. On all-known requests the cost is one `vertex_blocks` fan
walk per distinct portal vertex per pull, which WP2 measures.

**Throughput** (`cargo bench -p sim_core --bench throughput`, whole run under `perf stat`, base and candidate):
the whole benchmark run (all seven
scenarios, three repeats each) retires 200.72 G instructions on base and 201.01 G with the adopted design,
**+0.14 %**. The battle scenarios have no structures, so the cost there is the free-vertex test on channels with
none free. Per-scenario counts (a scratch selection switch, as funnel-fallbacks WP2 did) are WP2's. The wall-clock
figures of the same runs (base / candidate, ticks per second) are unreadable at this host's load (another agent's
gate ran alongside): for example the showcase 76 / 46 and 64 × 64 with 100 units 460 / 93, while the instructions
differ by 0.14 %; every scenario stayed above 25 ticks/s.

### 4.7 Tests

- `sim_map`, release, every test, with the adopted design's switches: only the two path pins fail (would-be values in
  §4.2). `contour_route_checksum`, `contours_add_no_funnel_fallbacks`, `SHOWCASE_MESH_DIGEST`, `VALIDATION_GOLDEN`,
  `DECODE_GOLDEN`, the restricted-route digest, the oracle tests, the proptests and the funnel tests pass unchanged.
- `sim_core`, release, with the adopted switches: `golden_replay_tests` (four tests), `fog_of_war_pathfinding_tests`
  and `wall_contact_tests` pass. The whole `sim_core` suite passed with the `SWALLOW2` build (80 test binaries).
- Not run: the `wasm32-wasip1` builds (D-8 rule 6 is WP2's and WP3's).

---

## 5. Design decisions

### D-1. Free portal vertices are pulled as points

**Rule.** For a request, a channel vertex `v` is **free** when `NavMesh::vertex_blocks(v, |t| access.open(t))` is
false (subject to D-3). The funnel treats a free vertex as a point:

1. **Offset.** Its signed offset is zero: `tangent` touches the vertex, not a circle round it. `radius_funnel`
   computes the free vertices among the portal and end-triangle vertices once per call into a sorted `Vec<u32>` in
   `FunnelScratch` (no allocation in steady state), and the geometry helpers (`sigma`, `tangent`) read it through an
   explicit parameter. The prototype's thread-local is not carried over (charter §1.1, no global mutable state).
2. **Shrink.** The shrunk pull moves only obstacle ends of a portal inwards by `rho`. A portal with two free ends is
   not shrunk; one with two obstacle ends is shrunk as today.
3. **Emission.** Both tangent points of a free anchor are the vertex, so `emit_waypoints` and `emit_zero_room` emit
   it once; no arc and no gate is built round it.
4. **Turns.** `turns_with_side` is unchanged: a point bend against its side is dropped like any other.
   `MeshRoute::turns` keeps free anchors with their side, since the route bends there.

**Why a point and not nothing.** The channel is TA*'s homotopy class, and its portals are where the route crosses.
A free vertex bounds nothing, but the shortest point path through the channel may still touch it, and a bend there
is the shortest route in that channel. Whether a better channel crosses the footprint is TA*'s question (§4.4).

**Alternatives rejected.**

| Alternative | Why rejected |
|---|---|
| A per-request mesh without the unknown structures | Exact, but a mesh per knowledge state is a rebuild per player per sighting; the pathfinding plan rejected per-request meshes (D-8) |
| Keep circles, smaller for free vertices | Any radius above zero keeps N2's false pinch at some width |
| After all retries fail, pull once more with `main`'s circles (`LEGACY`, measured) | The stress search's fallbacks went from 21 to 19 only; it keeps the defect as a rule |

### D-2. A free end inside an obstacle's clearance is clamped out of it

**Rule.** Let `F` be a free end of a portal and `O₁ … O_k` the obstacle vertices closer than `rho` to `F` (found by
the segment predicate's flood with the point `F` as segment, `obstacle_vertices_near`). If `k > 0`, the end is
replaced, in both pulls, by the point `P = F + t·u` on the portal's line (`u` the unit direction from the portal's
other end to `F`) with the least `|t|` such that `|P − O_i| ≥ rho` for every `i`. The candidates for `t` are 0 and
the two exits `t = (O_i − F)·u ± sqrt(rho² − d_i²)` of each disc (`d_i` the distance from `O_i` to the line); the
first candidate in order of `|t|` (ties to the positive) that clears every disc wins. `P` is a point anchor of the
pull, never rounded.

- **Where `O` is the portal's other end** (`|F − O| < rho`), `P` lies beyond `F` on the portal's line, `rho` from
  `O`: the disc cannot pass between them, so the route passes `F` on its far side.
- **Exactness.** `SimScalar::sqrt`, once per disc; the clamp is padded by one raw unit of `2⁻²⁴` so the point clears
  the disc after rounding (the prototype's `1 << 8` raw), a derived bound stated with its unit test.
- **Anchor kind.** The prototype reused `Anchor::Point`, which also marks the start and goal. The clean form adds
  `Anchor::Pass(SimVec2)`: a point the route passes, not an endpoint, so `approached_at_an_end` and
  `emit_zero_room`'s "point inside the circle" never take it for an end. `MeshRoute::turns` skips it like a `Point`.

**Alternatives measured and rejected (§4.2, §4.3).**

| Alternative | Result | Why rejected |
|---|---|---|
| Only the across case (`SWALLOW2`: the free end replaced when its own portal partner is within `rho`) | Goldens 2 + 8 left, no new fallback; stress search 26 left, 8 new | Misses a free vertex within `rho` of an obstacle on its own side (N4) |
| Drop a free anchor in the refinement when it lies in a neighbouring obstacle's circle (`SWALLOW`) | Contours 7, path 3 (one new wedged fallback); one route lost every turn after the drop and crossed the showcase's central plateau | Removing anchors in the refinement cascades: later anchors then turn against their side and are dropped too, and the refinement cannot see a wall more than `rho` from the new leg. The decision belongs to the pull, which keeps the channel |
| Drop the whole portal (`SWALLOW3`) | Contours 10 | Loses `O`'s constraint where no later portal carries it |

### D-3. A free vertex within `rho` of an obstacle not across its portal keeps its circle

**Rule.** A free vertex closer than `rho` to an obstacle vertex of the channel (a portal or end-triangle vertex)
that is not its partner in any portal is not treated as free: it keeps its circle, as on `main`. A free vertex whose
own partner is within `rho` stays free and is clamped (D-2).

**Why.** Such a vertex lies inside an obstacle's clearance disc. The clamp alone moves its portal end, but the next
portal at the same vertex may not see the obstacle, and the pull then alternates between the clamped point and the
vertex (the prototype's NEWFB2: anchors `L(13, 10.5) R(13.5, 11)[FREE] L(13, 11) R(13.5, 11)[FREE]`, a fallback).
Rounding it keeps the route outside the union of the two discs. This is a measured rule, not a derived one: it fixes
3 of the clamp's 5 stress cases (§4.3), the clamp's only new golden fallback and its other residual; its cost is a
longer route where the free vertex is a real gap's corner (N3 at r 1.0, +1.7 %). WP3 re-measures it against D-2
alone and states the choice in the evidence (§6).

### D-4. The refinement never inserts a free vertex

`deepest_violation` skips a free candidate: a portal vertex, a start or goal triangle's vertex (`extra`). D-3 of
funnel-fallbacks' outside vertices are obstacle vertices already. A free vertex near a leg is no collision; the
segment predicate judges the leg.

### D-5. `channel_width` measures obstacle points only

Left and right portal vertices enter `channel_width`'s pairs only when they are obstacle points; its blocking edges
are obstacles already. With free vertices it under-reports the room (0.5 to 0.707 across an unknown footprint),
shrinking or skipping the fitted margin for no obstacle. No corpus route changes with it; it is kept so the fitted
margin is the room between obstacles, as contour-footprints Amendment 1 defines it.

### D-6. Scope: wedged starts that round a free vertex

D-1 to D-5 apply to every pull, so to routes from wedged starts too. The 8 wedged fallbacks that round a free vertex
on `main` are this plan's (the prototype leaves 3); the 5 that round none stay the pathfinding plan's D-3 (§3.6).

### D-7. Determinism and state

- Fixed point only. The free test is `vertex_blocks`, exact; D-2's point uses `SimScalar::sqrt`, deterministic.
- The free list is sorted by vertex index; lookups are binary searches; candidates for `t` are sorted by `(|t|, t)`;
  no `HashMap` (C9).
- No hashed simulation state changes. The golden replays hash route waypoints (`current_funnel_waypoints`), so a
  route change would move them; none moved in the prototype. The schema stays 31.

### D-8. Measurement method and stop rules

The method is funnel-fallbacks D-5 with crowd-slide D-2: the matrix on 16 seeds, battle 33 on 64 seeds, battle 31 on
100 seeds, the showcase to tick 3,000 on the default seed and seeds 1 to 8. Added:

1. **Fallback corpora.** The two golden corpora with the class probe (§3.1); §4.3's partial-knowledge layouts (150,
   both shapes, whole and per-cell knowledge) and stress search; and the three all-known corpora of funnel-fallbacks
   (150, 1,000 and 2,000 flush layouts), which must stay at zero.
2. **Oracle ratios** on §4.4's corpus.
3. **Path-search cost** on §4.6's corpus, instructions retired.

**Stop rules.** A breach stops the WP; nothing beyond it is committed; the trace names the request (or the first
divergent tick, entity and request) and the rule that changed it.

1. **Winner.** No overall winner flips in the matrix, on the showcase default seed or in battle 33. Battle 31, and
   any battle whose 20-run split is within one seed, is judged on 100 seeds: the winner is unchanged and its share
   does not fall by more than the placebo band.
2. **Survivor ranges.** No seed of record leaves its battle's survivor range, read against the placebos (crowd-slide
   D-2): a miss that placebo P1, P2 or P3 of this base also shows (battle 26 seed 33 under P1) is reported, not
   stopped on.
3. **Give-ups.** At most base + 1 on the matrix of record, over all seeds, and on every showcase seed.
4. **Reversals.** Expected bit-identical (§4.5). If the matrix moves, its reversals of record stay at or below the
   placebo band's top (4,698), and every moved battle is traced to a route that took a rule of this plan.
5. **Control runs.** Each rule is switched off by a scratch flag, never committed, and must reproduce the previous
   tree byte for byte: the matrix, battle 33 `seeds.csv`, the showcase `units.csv` on nine seeds, and the `sim_map`
   pins.
6. **Goldens and pins.**
   - `golden_replay_tests` is expected unchanged. A change is re-recorded only with a proof header: the first
     divergent tick, the entity, the request and the rule, and the control build reproducing the old value.
   - `PATH_GOLDEN` and `CONTOUR_PATH_GOLDEN` are re-recorded with proof headers: the requests that changed, by
     class (fallback → legal, legal → shorter), and the old value reproduced with the rules off.
   - Every re-recorded value is identical in **native debug, native release, `wasm32-wasip1` release and
     `wasm32-wasip1` debug** (runner `wasmtime --dir=<real path of the worktree>`).
   - `contour_route_checksum`, `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `EXPECTED_CHECKSUM` and
     `SHOWCASE_MESH_DIGEST` must not move.
7. **Throughput**, judged on instructions retired (`perf stat -e instructions:u`, each scenario of
   `cargo bench -p sim_core --bench throughput` alone through a scratch selection switch, base and candidate, under
   `flock ~/scratch/gate.lock`), as funnel-fallbacks Amendment 1 point 3 settled for a loaded host:
   - every scenario within 5 % of base;
   - the path-cost measure no worse than base by more than 10 % (the prototype is −52 %);
   - every scenario at least 25 ticks/s in a wall-clock run, taken as the host allows (a quiet-host run stays an
     open confirmation item, not a blocker).
8. **Oracle** (§4.4's corpus):
   - no route shorter than the oracle by more than 10⁻³;
   - at least 95 % within 1.01;
   - no route this plan changes gets longer than on base by more than 0.5 %, except where D-3 rounds a free vertex
     that is a real gap's corner, each such case listed;
   - `MESH_RATIO_BOUND` (1.10) on routes this plan changes; the unchanged TA* residual above it (one route, 1.134,
     `main` 1.396) is B-149's and is reported, not stopped on.
9. **Fallbacks.**
   - No request of the golden corpora or of §4.3's layouts falls back that did not on base.
   - Fitting-start fallbacks: 0 in both golden corpora and in the whole-knowledge square layouts; at most 1 in the
     whole-knowledge contour layouts; the per-cell layouts at most the measured 1 and 5. Each residual is traced.
   - The stress search: at most the measured 10 fallbacks, of which at most the 2 named new ones, both traced (R-2).
   - The all-known corpora stay at 0.
10. **Tests.** A test changed for another reason than its WP's rule is restated first, test-only, with the trace. No
    bound is loosened, no tuning constant is added, `CORNER_SAFETY_MARGIN` stays 0.05, and the TA* budgets are not
    touched.

---

## 6. Work packages (commit order)

Branch `partial-knowledge-funnel`. Implementation agents hand back after `cargo gate quick` and the tests their
change needs; a verifier runs `cargo gate package` on the final tree (Q0061). Each WP appends to a new evidence file,
`docs/evidence/2026-11-19-partial-knowledge-funnel.md`. Sizes: S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- `DESIGN.md` §4, B-146: the pointer becomes this plan; the diagnosis is restated (free vertices rounded, shrunk,
  inserted and measured; zigzag, false pinches and fallbacks: 13 of 21 and 8 of 13 from fitting starts in the two
  golden corpora; the wedged starts that round a free vertex included).
- `DESIGN.md` §4: register **B-149** (proposed, open): "under partial knowledge TA* settles on a channel round an
  unknown footprint instead of across it: its many small triangles exhaust the anytime budget and the visit cap
  (`NAV_TA_STAR_EXTRA_EXPANSIONS`, `NAV_TA_STAR_TRIANGLE_VISITS`), which were measured on known terrain; 16 of 1,820
  routes of a partial-knowledge layout corpus above 1.01 of the oracle, worst 1.134". Pointer: this plan §4.4 and §9.
- The funnel-fallbacks plan gets a one-line pointer amendment: its §9 B-146 row is this plan.
- **Gate:** docs-only `package`.

### WP1: regression cases, red on `main` (S, tests only)

Each test is deterministic, in `crates/sim_map/tests/funnel_tests.rs` unless named otherwise, red on `main`, and
`#[ignore]`d with the reason "B-146, WP2" or "B-146, WP3" until its WP lands, so `main` stays green. A helper builds
the known mask from a list of known structure ids.

| Test | Case | Asserts |
|---|---|---|
| `a_route_crosses_an_unknown_footprint_without_zigzag` (N1) | §3.4 N1 at r 0.5, 0.75 and 1.0 | No fallback; legal; every waypoint at a free vertex equals it (no arc round it); length within 1.001 of 16.975, 17.192 and 17.432 |
| `an_unknown_neighbour_makes_no_pinch` (N2) | §3.4 N2 at r 0.5 | `zero_room == false`; legal; length within 1.001 of 10.329 |
| `open_ground_is_not_a_gap` (N3) | §3.4 N3 at r 0.5 | `zero_room == false`; one turn, at (11, 8); length within 1.001 of 9.829 |
| `a_free_end_inside_the_clearance_is_clamped_out` (N4) | §3.4 N4 | No fallback; legal; length within 1.001 of 15.614 (WP3) |
| `a_free_end_across_a_narrow_portal_is_passed_on_its_far_side` | The golden corpus's `R(35, 48) L(35, 46.5)[FREE] R(34.5, 46)` geometry rebuilt as a hand map (**verify** the minimal layout at WP1: a known structure with the corner (34.5, 46) and an unknown contour whose corner is (35, 46.5)) | No fallback; legal; no anchor at the free vertex (WP3) |
| `golden_corpora_fall_back_only_from_wedged_starts` (`validation_golden_tests.rs`) | The `PATH_GOLDEN` and `CONTOUR_PATH_GOLDEN` generators, counting fallbacks from fitting starts through `mesh_funnel_fallbacks` and `segment_is_passable` | 0 and 0 (on `main` 8 and 13) (WP3) |
| `free_vertices_are_points` (unit, `funnel.rs`) | A hand-built two-portal channel whose shared vertex is free | The route touches it; no arc point round it (WP2) |
| `the_clamp_clears_every_disc` (unit, `funnel.rs`) | Seeded free ends with one to three obstacles within `rho`, every shipped radius | The clamped point is at least `rho` from each obstacle and no other candidate with smaller `|t|` is (WP3) |

- **Diagnostic.** The per-class counts stay a scratch probe; no production surface is added for them.
- **Gate:** `quick`; `package` by the verifier.

### WP2: D-1, D-4 and D-5 (M, `sim_map`)

- **Change.**
  - `funnel.rs`: the free list in `FunnelScratch`, filled once per `radius_funnel` call; the geometry helpers take
    it; `shrink` takes per-end flags; `deepest_violation` skips free candidates. `// PERFORMANCE RATIONALE:` on the
    sorted list and its binary search instead of a mesh-sized flag array (a few dozen entries per channel, against
    an array cleared per pull).
  - `search.rs`: `channel_width` filters by `vertex_blocks`.
  - The module docs of `funnel.rs` and `search.rs` state the free-vertex invariant.
- **Tests.** Un-ignore N1, N2, N3 and `free_vertices_are_points`. A proptest `free_vertices_carry_no_circle` (64
  cases, seeded 24 × 24 maps with structures, half unknown): every route is legal or a counted fallback, and no
  emitted waypoint lies strictly inside `rho` of a free anchor other than the vertex itself.
  `funnel_paths_are_legal_and_no_shorter_than_a_straight_line` and `mesh_routes_agree_with_the_exact_oracle` stay
  green.
- **Measure.** The D-8 set against `main`; the pins (both path pins expected to move, nothing else); the two longer
  oracle routes of §4.4 traced; path cost and throughput.
- **Exit.** The stop rules, with the WP's own corpus figures (§4.2's "D-1 alone" row is the expectation).

### WP3: D-2 and D-3 (M, `sim_map`)

- **Change.** `Anchor::Pass(SimVec2)`; the clamp in `string_pull` for both pulls; D-3's exclusion when the free list
  is built; `emit_waypoints`, `emit_zero_room`, `approached_at_an_end` and `MeshRoute::turns` handle `Pass`.
- **Tests.** Un-ignore N4, the far-side case and `golden_corpora_fall_back_only_from_wedged_starts`; add
  `the_clamp_clears_every_disc`.
- **Decision recorded in the evidence.** D-3 is measured on and off (§4.3's table). It stays if it holds the stop
  rules and removes new fallbacks; dropping it, or changing its reach, is an amendment.
- **Trace.** The 2 new stress fallbacks and the residual fallbacks of §4.3, each named with its geometry and class.
- **Measure.** The D-8 set; the pins re-recorded on four builds with proof headers; throughput.
- **Exit.** The stop rules.

### WP4: tests (S, tests only)

- `validation_golden_tests.rs`: the two pins re-recorded if WP2 and WP3 did not, each header naming the requests that
  changed by class and the old value reproduced with the rules off.
- `pathfinding_properties.rs`: `funnel_paths_are_legal_and_no_shorter_than_a_straight_line` gains requests with a
  random known mask, and its doc comment's B-146 exemption is removed (wedged starts only).
- `pathfinding_properties.rs`: `partial_knowledge_routes_agree_with_the_oracle`: §4.4's corpus at a test-sized scale
  (40 layouts), oracle on the known-only map at `r − 10⁻⁵`, with a comment naming the reason; none shorter, at least
  95 % within 1.01, no fallback from a fitting start.
- If a change to `crates/gate/suites.ron` is needed, it is measured (C22).

### WP5: measurement and bookkeeping (S)

- Evidence: the final D-8 figures, the corpus tables of §4 recomputed, throughput, the pins with their headers.
- `DESIGN.md` B-146 leaves the register as fixed, with the figures and residuals (wedged starts on wall lines; the
  per-cell and stress residuals). B-149 stays open.
- `docs/plans/README.md`: this plan's row updated.
- No storyboard check is needed unless the showcase moved.

---

## 7. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `DESIGN.md` §4 (B-146, B-149), `docs/plans/README.md`, funnel-fallbacks pointer amendment | WP0, WP5 |
| Evidence | `docs/evidence/2026-11-19-partial-knowledge-funnel.md` (new) | WP1 to WP5 |
| D-1, D-4 | `crates/sim_map/src/navmesh/funnel.rs` (`FunnelScratch`, `sigma`, `tangent`, `shrink`, `deepest_violation`, emission) | WP2 |
| D-5 | `crates/sim_map/src/navmesh/search.rs` (`channel_width`) | WP2 |
| D-2, D-3 | `funnel.rs` (`Anchor::Pass`, `string_pull`, the free list), `search.rs` (`turns`) | WP3 |
| Tests | `crates/sim_map/tests/funnel_tests.rs`, `pathfinding_properties.rs`, `validation_golden_tests.rs`, unit tests in `funnel.rs` | WP1 to WP4 |
| Pins | `validation_golden_tests.rs` (`PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`) | WP2 to WP4 |

**Coexistence.** At planning time another agent works on structure placement (the `footprint-flags` worktree;
**verify** its file list at WP2's start). It is not expected to touch `navmesh/`'s funnel or search. If it re-pins
`PATH_GOLDEN` or `CONTOUR_PATH_GOLDEN` first, this plan re-records on top of it with both changes named in the
header. `sim_core` is untouched.

---

## 8. Determinism, checksum and throughput

- Exact lattice tests and Q32.32 only (D-7); no float, no `HashMap` (C7, C9).
- No new collection type in steady state: the free list and the clamp's candidates live in `FunnelScratch`.
- The checksum schema is unchanged (31).
- **Cross-platform.** The re-pinned digests are proven identical on four builds (D-8 rule 6); `golden_replay_tests`
  runs on `wasm32-wasip1` release in WP2 and WP3.
- **Throughput.** D-1 adds one `vertex_blocks` fan walk per distinct portal vertex per pull, and D-2 one point flood
  per free end. Measured: the path-search corpus is cheaper by half (§4.6); stop rule 7 bounds the battle scenarios.
- **Charter.** No identity in Rust (C5). `// PERFORMANCE RATIONALE:` where the free list replaces a simpler form.
  Functions under 100 lines; the clamp is its own function. No new error path: a failed pull is the existing retry
  chain and fallback. No global mutable state.

---

## 9. Out of scope and decisions recorded (Q0002)

| Item | State | Why not here |
|---|---|---|
| **B-149 (proposed): TA*'s channel under partial knowledge** | Open (WP0 registers it) | The anytime budget (256) and the visit cap (4) were measured on known terrain. An unknown footprint adds many small triangles, and TA* settles on a channel round it: 16 of 1,820 routes above 1.01 of the oracle, worst 1.134 (with an unbounded budget and 64 visits: 3, worst 1.040). Changing budgets changes every route's search; it needs its own measurement |
| **Wedged starts on a wall line** (5 in the golden corpora) | Unchanged | Pathfinding D-3: the counted fallback is by design (§3.6) |
| The stress search's 2 new fallbacks and per-cell residuals | Traced in WP3, recorded in B-146's row | No variant measured removed them without adding others; the shapes are flush contour pairs and ragged per-cell knowledge, harsher than vision |
| The knowledge model itself (whether unknown structures are walked through) | Unchanged | Pathfinding D-2; StarCraft II's behaviour **verify** |
| A per-request mesh without unknown structures | Rejected | D-1 alternatives |
| Tuning `CORNER_SAFETY_MARGIN` | Not changed | Contour-footprints D-15 rule 7 |

---

## 10. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | A battle route on another seed or a later map rounds a free vertex today, so battles move | Outcomes move | Stop rules 1 to 5 with traces and controls; the prototype measured none on the battle set |
| R-2 | The clamped point or a D-3 circle leaves no legal pull where `main`'s zigzag found one (the stress search's 2 new fallbacks) | A route falls back where it did not | Stop rule 9 bounds them; WP3 traces each; the fallback stays counted |
| R-3 | D-3 lengthens a route where a free corner is a real gap's corner (N3 at r 1.0, +1.7 %) | Slightly longer routes beside unknown structures | Stop rule 8's listed exception; WP3 measures D-3 on and off |
| R-4 | The clean `vertex_blocks` calls cost more than the prototype in crowded fog | Throughput | Stop rule 7 on instructions; the list is built once per pull |
| R-5 | Point bends let a route hug an unknown structure's outline; when the structure is seen, the route is re-planned (`recheck_routes`) | As today | Knowledge behaviour unchanged; on `main` the route was already through the footprint, only shaped worse |
| R-6 | `Anchor::Pass` is a public API change | Downstream matches | `Anchor` is used inside `sim_map` and its tests (**verify** with a grep at WP3); the variant is documented |
| R-7 | Pins re-recorded on one platform only | CI's determinism jobs fail | Stop rule 6 requires four builds |
| R-8 | The structure-placement work moves the same pins first | Merge conflict on the pin constants | §7 coexistence |

---

## 11. Prototype material (scratch, not committed)

- Diff: `~/scratch/pkf/patches/proto.diff`: switches `PKF_POINTS`, `PKF_WIDTH`, `PKF_CLAMP`, `PKF_NEAR`, the rejected
  `PKF_SWALLOW`, `PKF_SWALLOW2`, `PKF_SWALLOW3` and `PKF_LEGACY`, probes `PKF_PROBE`, `PKF_LOG`, `PKF_FT` and
  `PKF_DBG`; scratch tests `zz_pkf_probe.rs` and `zz_pkf_corpus.rs`.
- Scripts: `~/scratch/pkf/scripts/measure.sh` (funnel-fallbacks' with its own lock), `summary.py`, `variant.sh` (all
  corpora for one variant); `cmp.py` and `cmp2.py` compare two per-request logs.
- Results: `~/scratch/pkf/{base,cand,cand2}/` (battle set; `cand` is `SWALLOW2`, `cand2` the adopted design);
  `main-*.txt` and `log-PKF_X_0-*.txt` (golden corpora on `main`); `v-{near,clamp,clampnear,nearleg}/` (all corpora
  per variant); `corpus-*.txt`, `oracle-*.txt` (layouts); `cases-*.txt`, `search-*.txt` (hand cases); `thr-*.txt`
  (throughput instructions).
- The prototype worktree and its target directory were removed after measurement.

## Amendment 1 (orchestrator, 2026-11-20): `Anchor::Pass` is an end for approach; one residual accepted; the tie rule stated

**Why.** WP3 stopped on stop rule 9: the stress search has 11 fallbacks against the measured 10 (evidence, "WP3
stopped"). Switching each part on and off traced the cause to the plan's own text. D-2 keeps `Anchor::Pass` out of
`approached_at_an_end`, but the measured prototype used `Anchor::Point` there, which counts as an end. The extra
route (K2/U4 offset (1, 1), r 0.75) already falls back on `main`.

**Decision.**

1. `Anchor::Pass` counts as an end in `approached_at_an_end`, and only there. That restores the measured 10 and
   changes no other figure and neither pin. Rule 9's bound is not raised.
2. **Residual accepted.** The per-cell contours case, layout 58 q 13, is accepted as a residual under rule 9's first
   bullet. It is new against `main`, but the plan's adopted prototype has it too, so the rule and the measured
   design contradicted each other. It is recorded in the evidence and in B-146's row.
3. **Tie rule.** D-2 and D-7's tie wording is replaced by the measured order: a stable sort by `|t|` in vertex
   order, with each disc's positive exit first. Both alternative readings were measured and are worse.
4. **WP2's figures stand.** Its interim per-cell and stress-search fallbacks are recorded in the evidence. WP3
   removes them.

WP3 then lands from `partial-knowledge-funnel-wp3-wip`, with the four-build pin proof. WP4 and WP5 follow.
