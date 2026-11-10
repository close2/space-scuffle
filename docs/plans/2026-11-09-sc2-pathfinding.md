# Blueprint: StarCraft II-Style Pathfinding (route following first, then a constrained Delaunay navigation mesh)

The owner wrote in chat on 2026-11-09: "the path finding algorithm is still suboptimal. this is visible in the
showcase video, where 2 units take strange paths at the bottom of the ramp (and later again don't take the direct
line) … for the path finding consider doing an internet research. maybe a algorithm which is equal or very close to
the SC2 algorithm has already been published".

This plan:

- records what is published about StarCraft II's pathfinding, keeping what is documented apart from what is
  inferred (§2);
- reproduces the two defects in the showcase battle and measures them (§3);
- chooses the target algorithm (§4).

The work comes in two parts.

- **Part A** fixes the route-following layer in `sim_core`, which causes both visible defects. It is small, and
  it lands first.
- **Part B** replaces the grid A* corridor in `sim_map` with the design closest to what StarCraft II is reported
  to use. That design is a constrained Delaunay triangulation (CDT) of the map, searched with A* over triangles,
  using Demyen and Buro's radius-aware triangle widths and funnel. One mesh serves every unit radius. The mesh is
  rebuilt from a cached terrain base whenever a structure is placed or removed. Part B is built latent next to the
  grid, measured against it, and then switched on.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-09 against `main` at `33a8e82`.
Line numbers drift, so search for the quoted symbol. The planning probe described in §3.1 was run on that commit and
was not committed. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8), unit mechanics follow StarCraft II | Standing rule | §2, D-1 |
| Q0061, implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002, scoping is the agent's call if written down | Standing rule | §9 |
| None new | — | The owner's request names the goal. Every choice below has a default recorded as a decision, and §9 lists what was deferred |

---

## 1. Summary

| WP | Title | Changes results? | Size | Depends on |
|---|---|---|---|---|
| WP0 | This plan and its row | No | — | — |
| WP1 | Route-quality harness: `route_trace` example, metrics, baseline evidence | No (tooling) | S | WP0 |
| WP2 | Part A1 and A2: corner waypoints are never rewritten for a parked body, and a corner is passed by position and terrain view, not by an empty next leg | Yes (terrain battles, goldens) | M | WP1 |
| WP3 | Part A3: a unit approaching a corner with a body parked on it aims past the body (steering only) | Yes | S | WP2 |
| WP4 | Part A4: corner offsets use the unit's own radius | Yes | S | WP3 |
| WP5 | Part B1: exact-integer constrained Delaunay mesh in `sim_map`, latent | No | L | WP0 (can run in parallel with WP1 to WP4) |
| WP6 | Part B2: triangle A* (TA*) with Demyen widths and the radius funnel, latent | No | L | WP5 |
| WP7 | Part B3: mesh segment predicate, nearest reachable point, player knowledge, latent | No | M | WP6 |
| WP8 | Part B4: differential and performance measurement; a terrain throughput scenario | No (bench added) | M | WP7 |
| WP9 | Part B5: the switch: routes come from the mesh at each unit's real radius | Yes (goldens, terrain battles) | M | WP4, WP8 |
| WP10 | Part B6: grid corridor code removed; documents | No | S | WP9 |
| WP11 | Bookkeeping | No | S | WP10 |

**Expected battle impact.** The matrix battles of `sim_fixtures::matchup` are fought on flat, open
`GameMap::new(n, n, default_ground)` arenas. On those, every route is the straight line, under both the grid and
the mesh: `find_path_with_vision` returns early when `segment_is_passable(start, goal)` holds. The matrix and battle
33 are therefore expected to stay **bit-identical** through every WP; any change there is a finding to explain.

What does move:

- The two golden replays, which run on `default_arena.scmap.ron` with terrain and structures. Route waypoints are
  hashed (`checksum/encoding.rs`, `current_funnel_waypoints`), and so is `nav_version`.
- The showcase battle.
- Any skirmish on a terrain map.

---

## 2. What is known about StarCraft II's pathfinding

Tags used in this section:

- **[DOC]**: stated by Blizzard staff or in official material.
- **[2ND]**: a secondhand account of the GDC 2011 talk.
- **[COMM]**: community observation.
- **[INF]**: inference made by this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | GDC Vault, "AI Navigation: It's Not a Solved Problem – Yet", James Anhalt (Blizzard), Alexander Kring, Nathan Sturtevant, GDC 2011 AI Summit, <https://www.gdcvault.com/play/1014514/AI-Navigation-It-s-Not> | The talk in which Anhalt described StarCraft II's navigation [DOC: panel and title]. No public slides or transcript were found |
| S2 | Y. Miyake, IGDA Japan GDC 2011 AI report, slides 39–40, <https://www.slideshare.net/youichiromiyake/igda-gdc2011ai> | A navmesh built by Delaunay triangulation; about 64,000 polygons for up to 2,000 units; regions tagged ground, cliff, building or none; "funnel algorithm" [2ND] |
| S3 | P. Czajecki, "Open Problems of A* for Any Angle Pathfinding", 2024, <https://blog-54a.pages.dev/open-problems-of-a-star-for-any-angle-pathfinding> | StarCraft II: CDT navmesh, A* on triangles, funnel smoothing; the static mesh is cached and re-run when a building is added or destroyed; units are not in the mesh, and boids-style steering handles moving units [2ND] |
| S4 | J. Dickison (jdxdev), "RTS Pathfinding 2: Dynamic Navmesh with Constrained Delaunay Triangles", 2021, <https://www.jdxdev.com/blog/2021/07/06/rts-pathfinding-2-dynamic-navmesh-with-constrained-delaunay-triangles/>, and part 3, "Variable agent size", <https://www.jdxdev.com/blog/2021/12/07/rts-pathfinding-3-variable-agent-size-smoke-tests-navmesh-fixes/> | "My understanding" is that StarCraft II reset to the static base mesh and reinserted every dynamic obstacle each time. Anhalt "demonstrated how Starcraft2 created paths along a tunnel inside the expanded vertices" (vertices expanded by unit radius) [2ND]. A Q48.16 fixed-point implementation reports that local repair was harder than generating the mesh [COMM] |
| S5 | Wicks Rivas & Souza, "Map Marker", SBGames 2021, <https://www.sbgames.org/proceedings2021/ComputacaoFull/217610.pdf> | Cites an email from Anhalt: StarCraft II runs one A* per agent, so each unit's path accounts for "its characteristics, such as size and ability to climb ledges", and the game "prioritized predictability in the behavior of its agents" [DOC, private communication as cited] |
| S6 | Blizzard s2client-proto (`raw.proto`, `query.proto`, `data.proto`), <https://github.com/Blizzard/s2client-proto> | The API exposes a 1-bit `pathing_grid` and `placement_grid`. `RequestQueryPathing` returns the engine's path `distance` (0 if there is no path). `footprint_radius` is "an estimation" [DOC]. The grid is a raster export, not the navigation representation [INF] |
| S7 | sc2mapster / Hive Workshop forum threads on unit Radius, Inner Radius, Separation Radius and Push Priority (seen as search snippets only) | Radius is the collision radius with units; Inner Radius sets how close a unit gets to buildings (effectively `min(inner, radius)`); higher push priority always pushes lower [COMM] |
| S8 | Liquipedia, "Walling", <https://liquipedia.net/starcraft2/Walling> (snippet) | Buildings adjacent edge to edge block completely; buildings touching only at corners let small units through [COMM]. That fits radius-aware geometric pathing, not a binary grid [INF] |
| S9 | P. Wyatt, "The StarCraft path-finding hack", 2013, <https://www.codeofhonor.com/blog/the-starcraft-path-finding-hack>; BWAPI `Region`, <https://bwapi.github.io/bwapi5/class_b_w_a_p_i_1_1_region.html> | Brood War paths on a tile grid with regions as search nodes, plus a large unstick state machine. This is the design StarCraft II replaced [DOC for Brood War] |
| S10 | TeamLiquid, "Broodwar and Starcraft 2 Pathing", 2013, <https://tl.net/blogs/429573-broodwar-and-starcraft-2-pathing>; "The Mechanics of SC2 [Part 1]", 2010, <https://tl.net/forum/starcraft-2/132171-the-mechanics-of-sc2-part-1> | StarCraft II groups clump through steering and flocking; Brood War units claim cells one at a time and jam [COMM] |
| S11 | D. Demyen & M. Buro, "Efficient Triangulation-Based Pathfinding", AAAI 2006, <https://cdn.aaai.org/AAAI/2006/AAAI06-148.pdf> | TA* and TRA*: A* over CDT triangles. A per-triangle width between entry and exit edge is necessary and sufficient for a disc of radius r, so one mesh serves all sizes. A modified funnel with circles of radius r around channel vertices gives the shortest path inside the channel in linear time. The search is anytime and converges to the optimum |
| S12 | M. Kallmann, "Shortest Paths with Arbitrary Clearance from Navigation Meshes", SCA 2010, <https://diglib.eg.org/handle/10.2312/SCA.SCA10.159-168>; "Dynamic and Robust Local Clearance Triangulations", ACM TOG 2014, <https://history.siggraph.org/?p=107908> | Local clearance triangulations: a refinement of the CDT under which the triangle-local clearance test is exact for any disc, with dynamic insertion and removal |
| S13 | Cui, Harabor & Grastien, "Compromise-free Pathfinding on a Navigation Mesh" (Polyanya), IJCAI 2017, <https://ijcai.org/proceedings/2017/70.md> | Optimal any-angle search on a polygon mesh with no preprocessing, for a point agent; a radius needs a mesh inflated per radius class |
| S14 | Harabor, Grastien, Öz & Aksakalli, "Optimal Any-Angle Pathfinding In Practice" (Anya), JAIR 56, 2016, <https://jair.org/index.php/jair/article/view/11004>; Nash, Koenig & Tovey, "Lazy Theta*", AAAI 2010, <https://ojs.aaai.org/index.php/AAAI/article/view/7566> | Any-angle search on grids: Anya is optimal for a point agent; Theta* and Lazy Theta* are near-optimal and use line-of-sight checks |
| S15 | M. Mononen, "Simple Stupid Funnel Algorithm", 2010, <https://digestingduck.blogspot.com/2010/03/simple-stupid-funnel-algorithm.html>; Recast/Detour (`rcErodeWalkableArea`) | String pulling for a point agent. Recast erodes the mesh per agent radius and uses floats; Mononen calls a radius-aware funnel "about 100x more complicated" |

### 2.2 What this means for us

**Documented or consistently reported (S2 to S5):**

- StarCraft II navigates ground units on a **triangulated navigation mesh**, built from terrain and building
  footprints and tagged by kind.
- Each unit runs its **own A\*** over the triangles, then a **funnel** pass.
- The path takes the unit's **size** into account. The best available account (S4) is a funnel in a "tunnel
  inside the expanded vertices": one mesh, with the radius applied when the path is extracted.
- Moving units are **not** in the mesh. Steering, flocking, separation and pushing handle them.
- Buildings change the mesh. S3 and S4 say it is rebuilt from a cached terrain base; S2 says "the mesh does not
  change at runtime", which may refer to the terrain base. This plan follows S3 and S4.

**Not documented, inferred by this plan [INF]:**

- That the A* is Demyen and Buro's TA* exactly.
- That paths are globally shortest. S5's "predictability" and S2's polygon count suggest plain A* with a funnel,
  which is not guaranteed optimal.
- How StarCraft II handles a corner occupied by a unit.

No source says StarCraft II rewrites a unit's route when a unit stands on its corner (our
`sidestep_occupied_corner`). By S3, bodies belong to steering, not to the route. No source puts unit radii into
three tiers (our `UnitClearance`). S5 and S7 say the size is the unit's own.

**Closest published algorithm:** CDT + TA* + Demyen width + radius funnel (S11). It is the only published method
that matches every account (S2 to S5): CDT, A* on triangles, one mesh for all radii, and a funnel in a tunnel of
radius-expanded vertices. S11 and S4 are its two references. S12 is the correction for narrow passages.

---

## 3. Diagnosis

### 3.1 Method

The planning probe was a throw-away `crates/sim_core/examples/path_probe.rs`, not committed. It loads
`scenarios/showcase.ron` from disk and gives the same attack-move orders as the client's
`BattleScenario::instantiate`: the approaches are (5.5, 5.5) and (52.5, 52.5). It steps the world with empty
bundles and writes every mobile unit's position and route per tick.

Metrics are taken per unit on its first contiguous pass through three boxes:

- the foot of player 0's ramp `W` (cells x 26–28, y 8–11), box x 26–34, y 7–14;
- the south-east corner of the central plateau, box x 35–41, y 21–29;
- the foot of ramp `E`, box x 31–39, y 49–57.

For each pass the probe reports:

- **walked/chord**: path length walked inside the box, divided by the straight distance between the pass's first
  and last point;
- **total turn**: the sum of absolute heading changes, sampled every 3 ticks;
- **first-waypoint variants**: how many different first waypoints the unit's route had during the pass.

A second probe mode compares `find_path` lengths (Small tier) on 400 random cell-centre pairs with a reference
shortest path. The reference is Dijkstra over a visibility graph whose nodes are every convex corner of the
showcase map, offset by 0.425 (tier radius + `CORNER_SAFETY_MARGIN`), with `segment_is_passable` as the
visibility test.

The video (<https://close2.github.io/space-scuffle/showcase.mp4>, 44 s) was downloaded to
`~/scratch/pathplan/video` and cut into frames. The "smog" scene (17 to 22 s) and the "advance" scene follow
player 0's group down ramp `W` and around the plateau. The probe's trajectories match the frames.

### 3.2 Defect 1: "two units take strange paths at the bottom of the ramp"

**Cause: `sidestep_occupied_corner` (`movement/locomotion.rs`) cascades.**

Every unit of player 0's group gets the same route, and its second waypoint is the ramp-foot corner (29.3, 11.7).
That point is the corner (29, 12) offset by 0.425, the same for a 0.375 Pulse Bot and a 0.5 Rhino Beetle. The
group walks in single file, so the corner is almost always occupied by the unit ahead.

`next_waypoint` calls `sidestep_occupied_corner` every tick. When a non-yielding body touches the first waypoint,
it replaces that waypoint with a point `r + r_occupant + 0.1` further out along `incoming − outgoing`. Here that
direction is south-east. The replacement point is then occupied by the previous sidestepper, so the next unit
steps out further. The probe's route log for unit 5 shows the cascade:

| Tick | Unit 5's first waypoint |
|---:|---|
| 0 | (29.30, 11.70) |
| 174 | (29.69, 10.94) |
| 184 | (29.85, 10.11) |
| 197 | the corner is dropped and the unit heads to (38.3, 25.7) |

Unit 8 goes (29.92, 11.12), then (30.36, 10.39). Both units walk south-east, away from their destination, and then
back north-east: the V seen in the video.

| Ramp W foot, player 0 (baseline) | Walked | Chord | Ratio | Total turn | First-waypoint variants |
|---|---:|---:|---:|---:|---:|
| Unit 5, `pulse_bot` | 9.49 | 6.86 | 1.383 | 358° | 4 |
| Unit 8, `pulse_bot` | 9.96 | 7.52 | 1.323 | 840° | 4 |
| Unit 4, `pulse_bot` | 7.78 | 6.13 | 1.270 | 323° | 4 |
| Unit 13, `rhino_beetle` (first through) | 5.54 | 4.98 | 1.113 | 52° | 2 |

The unit at the head of the column walks a ratio of about 1.11. The later ones walk up to 1.38, with up to 840° of
turning, in a box whose shortest route turns about 60°.

**Contributing cause: the handoff waits for an empty next leg.** `drop_passed_waypoints` passes a corner only when
the next waypoint is in terrain view **and** `leg_is_free_of_bodies`. In a column the next leg always holds a
flockmate. A unit that steering has pushed past the corner therefore keeps aiming back at the corner, and the
corner keeps generating sidesteps.

### 3.3 Defect 2: "later again don't take the direct line"

The same mechanism repeats at the plateau's south-east corner (38.3, 25.7), the next shared corner:

| Plateau SE corner, player 0 (baseline) | Walked | Chord | Ratio | Total turn | Variants |
|---|---:|---:|---:|---:|---:|
| Unit 7, `pulse_bot` | 12.70 | 8.47 | 1.500 | 1,071° | 7 |
| Unit 9, `pulse_bot` | 10.61 | 8.50 | 1.248 | 340° | 4 |
| Unit 5, `pulse_bot` | 10.29 | 8.61 | 1.195 | 239° | 4 |
| Unit 12, `rhino_beetle` (head of column) | 8.69 | 8.40 | 1.035 | 31° | 2 |

Player 1's units show the same pattern at the foot of their ramp `E` (unit 23: ratio 1.473, 5 variants; unit 26:
435° of turning).

### 3.4 What is not the cause

**The static route.** On 400 random pairs on the showcase map:

- the grid A* corridor plus funnel was more than 1 % longer than the corner-visibility reference once, at most
  2.7 % (pair (61.5, 24.5) → (16.5, 35.5): 50.05 against 48.73);
- the mean `find_path` cost was 337 µs on the loaded server (load average above 100 during the probe).

The reference is not complete: it misses turning points at ramp transitions, so on some pairs it is longer than
ours. That makes it an upper bound, not an exact optimum, and WP6 replaces it with an exact oracle. The routes in
§3.2 and §3.3 are themselves near-shortest. The defects come from following them.

**Ramp `E`'s right-angle turn.** Every player 0 unit walks a ratio of about 1.52 at the foot of ramp `E`, with
about 100° of turning, uniformly. The route goes to the ramp's south-west corner (34.7, 52.3) and then east. This
is correct under our terrain rules: `terrain_step_is_connected` makes a ramp's sides walls, and ramp `E` stands
proud of the plateau edge, so it can only be entered from its low end. StarCraft II's ramps are also bounded by
cliffs on their sides [INF]. This is not changed (§9).

### 3.5 Grid-design limits that Part B removes

These do not cause the reported defects, but they are deviations from S5, S7 and S11:

- **L-1. Three radius tiers.** `UnitClearance::for_radius` routes every radius up to 0.5 as Small (0.375). The
  Rhino Beetle (0.5) therefore gets corners offset by 0.425, less than its own radius, and its body crosses the
  corner by 0.075. StarCraft II paths each unit by its own size (S5, S7).
- **L-2. The route is limited to the A\* corridor.** Grid A* picks one cell corridor by octile cost, and the funnel
  can only shorten within it. Around the central plateau the east and west routes differ by about 0.4 units
  (43.6 against 43.2, 1 %). The octile cost chose the longer one for player 0.
- **L-3. Fallback smoothing through cell centres.** When the funnel's legs fail `segment_is_passable`,
  `smooth_connected_corridor` retains corridor cell centres. It allows a shortcut across a ramp transition only
  along a grid line, so it can add bends at ramp feet.
- **L-4. Cost.** The search is O(cells) per query (4,096 nodes on 64×64, 65,536 on 256×256). A CDT of the same
  maps has hundreds to low thousands of triangles.

### 3.6 Part A experiment (planning probe, not committed)

The probe was rebuilt with two switches inside `locomotion.rs`:

- `PROBE_NO_SIDESTEP`: `sidestep_occupied_corner` returns `None`;
- `PROBE_FREE_LEG`: `drop_passed_waypoints` drops the `leg_is_free_of_bodies` condition.

The results, over player 0's 10 units, are in §3.6.1. WP1's harness reproduces them as evidence.

#### 3.6.1 Results

Means and maxima over player 0's 10 units, ticks 0 to 700. Ratio is walked/chord; turn is total heading change.

| Variant | Ramp W foot: ratio mean / max | Turn mean / max | Plateau SE: ratio mean / max | Turn mean / max | Ramp E foot: ratio mean / max | Turn mean / max |
|---|---|---|---|---|---|---|
| Baseline (`33a8e82`) | 1.196 / 1.383 | 324° / 840° | 1.147 / 1.500 | 250° / 1,071° | 1.551 / 1.591 | 115° / 196° |
| No corner rewrite (A1 alone) | 1.171 / 1.385 | 546° / 1,392° | 1.041 / 1.082 | 42° / 137° | 1.546 / 1.580 | 112° / 162° |
| Handoff without the empty-leg condition (A2 alone) | 1.074 / 1.113 | 105° / 213° | 1.062 / 1.100 | 122° / 249° | 1.610 / 1.743 | 248° / 526° |
| A1 and A2 together | 1.063 / 1.113 | 92° / 146° | 1.045 / 1.078 | 67° / 148° | 1.557 / 1.635 | 213° / 416° |

Findings:

1. **Each switch alone helps one corner and hurts another.** Without the rewrite, the ramp W column aims at an
   occupied corner and jitters (546° mean). Without the empty-leg condition, ramp E jitters (248°).
2. **Together they remove both reported defects.** At ramp W the worst ratio drops from 1.383 to 1.113 and the
   worst turn from 840° to 146°. At the plateau corner the worst ratio drops from 1.500 to 1.078 and the worst turn
   from 1,071° to 148°.
3. **Together they leave ramp E with more turning (mean 115° → 213°).** The trajectories show units at the back of
   the column wobbling at the ramp's south-west corner (34.7, 52.3) while a body stands on it.

A1 and A2 therefore land as one package (WP2). The leftover corner wobble is WP3's subject: steering aims past a
parked body instead of at the corner under it. The probe was a planning instrument with environment switches, not
a design; WP1's harness replaces it.

---

## 4. Design decisions

### D-1. Target algorithm

The target is a CDT navigation mesh, TA* over its triangles with Demyen and Buro's radius widths, and the radius
funnel (S11). The mesh is rebuilt from a cached terrain base on every structure change (S3, S4). Bodies stay out
of the mesh and are handled by steering, as today.

| Candidate | Closeness to StarCraft II | Determinism in fixed point | Radius | Dynamic structures | Verdict |
|---|---|---|---|---|---|
| CDT + TA* + width + radius funnel (S11) | Matches every account (§2.2) | Exact integer predicates: every constraint vertex is a lattice point (D-2) | One mesh, every radius, exact width test | Rebuild from the cached base, O(n log n) | **Chosen** |
| Grid A* + funnel (today) + Theta* or Anya (S14) | Brood War's family (S9) | Already exact | One grid per tier, or tiers | Cheap | Rejected: not StarCraft II's design; tiers stay |
| Polyanya (S13) | Mesh-based, but optimal, which StarCraft II does not appear to be | Possible | Point agent: one inflated mesh per radius | Rebuild per radius class | Rejected: loses one mesh for every radius; kept as the exact oracle idea for tests |
| Recast/Detour (S15) | Not StarCraft II | Floats, voxels | Mesh eroded per agent size | Tile rebuild | Rejected: §3.1 forbids floats in authoritative state |
| Kallmann LCT (S12) | A refinement of the chosen design | Exact predicates | Exact for any disc | Local repair | Deferred: used only if WP6's narrow-passage tests show the Demyen width is unsound on our maps (R-2) |

### D-2. Exact integer geometry

**Vertices.**

- Every terrain constraint lies on a cell edge, so every mesh vertex is a lattice point `(i32, i32)` in cells.
- Predicates:
  - `orient2d` is an i64 cross product;
  - `incircle` is evaluated in i128.
- No Steiner points are needed. Constraints are axis-aligned unit edges merged into maximal collinear runs, and
  they never cross except at shared lattice vertices.

**Constraints.** Every cardinal cell edge becomes a constraint where any of the following holds:

- `cardinal_edge_is_connected` is false;
- one side is not walkable terrain;
- the edge is the map border;
- the two cells differ in the tag `(cliff_level, ramp)`.

The last rule gives every triangle exactly one tag, which ramp vision and future cliff movers need.

Structure footprints are inserted as constraint loops labelled with the structure's `EntityId`.

**Triangle labels.** Each triangle is `Walkable(tag)`, `Blocked` or `Structure(EntityId, tag)`. A request treats a
`Structure` triangle as blocked exactly when `is_cell_passable_for_request` would treat its cells as blocked, by
the same knowledge rules (known structures, vision). This keeps the fog-of-war contract of
`find_path_with_vision`'s invariant 2.

**Construction.**

1. Run Bowyer–Watson Delaunay insertion over vertices sorted by `(y, x)`.
2. Recover constraints by edge flipping (Sloan 1993).
3. Label triangles by a flood fill from constraint sides.

Iteration is in index order, so the mesh is a pure function of the map and the structure set, with no
`HashMap`.

### D-3. Each unit's real radius

`PathRequest` gains `radius: SimScalar`, the unit's `collision_radius`, in world units.

`UnitClearance` stays while the grid is live (WP2 to WP8). It is removed with the grid in WP10, together with the
"leave on the narrowest tier" fallback in `search_route`. The mesh has its own rule for a body standing where its
radius does not fit: the start triangle is allowed whatever its width, as today's fallback allows the start cell.

### D-4. The width test (S11)

Crossing triangle `t` from edge `a` to edge `b` is allowed when `width(t, a, b) ≥ 2r`. The width is Demyen and
Buro's: the distance from the vertex shared by `a` and `b` to the nearest obstacle across `t`, found by their
bounded search through unconstrained edges.

- Comparisons are on squares: `4r² ≤ d²`, with `r` in Q32.32 and `d²` an integer.
- Widths depend on the request's blocked set (D-2 labels), so they are computed during the search and cached per
  `(mesh version, knowledge mask version)`.
- Two caches are kept: all structures known, which is today's `clearance_grid` role, and the static mesh.
- A request with partial knowledge computes widths on demand. It is bounded by the search radius `2 × max
  radius`.

### D-5. Search (TA*)

- Nodes are triangles. `g` and `h` follow S11: `h` is the Euclidean distance from the goal to the entry edge, and
  `g` is the maximum of S11's lower bounds.
- Ties break on `(f, triangle index)`.
- When the goal triangle is first reached, the channel's funnel length becomes the incumbent. The search continues
  while some open node has `f <` incumbent (S11's anytime loop) and stops on a fixed expansion budget,
  `NAV_TA_STAR_EXTRA_EXPANSIONS`. WP8 measures the budget, and it is a named constant.
- Start and goal triangles are found by a jump-and-walk from a per-cell triangle index. The index is a flat
  `Vec<u32>` built with the mesh, so a lookup is O(1).

### D-6. The radius funnel

This is S11's modified funnel. Every channel vertex carries a circle of radius `r + CORNER_SAFETY_MARGIN`.

- An arc is replaced by its two tangent points when the turn exceeds 45°, and by one point on the bisector
  otherwise. The legs then keep at least `r` from every vertex they turn around.
- The fixed-point square root is `SimScalar::sqrt`, which is deterministic.
- The goal is the last waypoint, as today.

Every leg is then checked with the mesh segment predicate (D-7). This keeps today's guarantee that every leg
passes the predicate that movement uses. If a leg fails, which should not happen and is counted as a diagnostic,
the route falls back to the channel's portal midpoints.

### D-7. One predicate for routes and movement

`Pathfinder::segment_is_passable(request, a, b)` keeps its signature. Under the mesh it walks the triangles along
the segment and checks two things:

- each triangle is passable for the request (D-2);
- the segment keeps at least `r` from every blocking constraint edge met within `r`.

This is exact in integers and Q32.32. `route_is_usable` and `drop_passed_waypoints` use this predicate, so routes
and movement agree, as they do now.

`GameMap::ground_segment_is_connected` stays the terrain truth for physical motion (`MotionPolicy`). The mesh is
built from it, and WP7's tests prove they agree.

### D-8. Rebuild on structure change

`recompute_clearance` becomes `recompute_navigation`:

1. Clone the cached terrain-only constraint set.
2. Add the current structure loops.
3. Rebuild.
4. Increment `nav_version`, as today.

Its cost on a 256×256 map is measured in WP8 (R-3). Local repair (S4, S12) is out of scope until a measurement
needs it.

### D-9. Route following (Part A)

**A1 (WP2).** `sidestep_occupied_corner` is removed. A body parked on a corner is a steering obstacle, as for any
other point of the route:

- `compute_unified_local_steering` already deflects around obstacles (`OBSTACLE_WEIGHT`, about 40°);
- `step_toward` slides along blocked velocity.

**A2 (WP2, with A1).** A corner waypoint `w` with successor `s` is passed once two things hold:

- the unit is in the half-plane beyond `w` (`dot(pos − w, s − w) ≥ 0`), or within the existing handoff distance;
- the leg `pos → s` passes the terrain and route predicate.

`leg_is_free_of_bodies` no longer gates the handoff. Bodies are steering's job (S3).

A1 and A2 land together because §3.6.1 measured each alone making one corner worse.

M-5 (`movement_contact_tests.rs`, `route_continues_past_a_blocker_on_its_corner_waypoint`) must keep passing.

**A3 (WP3).** A unit whose current target is a corner waypoint with a non-yielding body touching it steers towards
the tangent point of the body's circle (radius `r_body + r_self + WAYPOINT_HANDOFF_MARGIN`) on the outside of the
turn. "Outside" is the side of the line from the unit to `s` away from the obstacle the corner turns around.

This is a steering aim, recomputed each tick from the tick-start snapshot. The route is never written. The side is
fixed by the sign of `cross(s − w, pos − w)` at first contact, held in `MovementMemory` until the corner is passed,
so the unit does not swap sides while rounding the body.

`MovementMemory` is hashed, so this adds one `i8` to the entity encoding. That is a checksum schema bump (20 → 21),
noted in WP3. If WP3 can instead derive the side from tick-start state without memory, it does so and the schema
stays at 20; the commit says which.

**A4 (WP4).** The corner offset is the unit's `collision_radius + CORNER_SAFETY_MARGIN`, passed in the request.
The tier still chooses the corridor until WP9. This fixes L-1 for corner offsets.

These are fixes to rules of our own, not StarCraft II values, so each is a bug fix under Q0051. Proposed register
IDs:

- B-98: corner rewrite and empty-leg handoff;
- B-99: wobble at an occupied corner;
- B-100: tier radius at corners;
- B-101: grid corridor in place of StarCraft II's triangle mesh.

**verify**: the next free number when WP1 lands. Plan `2026-11-09-client-mode-commands` proposes B-94 to B-97.

### D-10. Determinism

- No floats.
- No hashed collections: the mesh lives in `Vec`s indexed by `u32`.
- Every loop runs in index order.
- The mesh is derived state and is not hashed. `nav_version` and the waypoints stay hashed, so checksum schema 20
  does not change in Part B: no field is added to or removed from the encoding. Only WP3 may bump it (D-9 A3).
- WP5 to WP7 add tests that run on `wasm32-wasip1` through the existing determinism job: a mesh digest, and the
  routes of a fixed query list on both shipped maps.

### D-11. Measurement

| What | Command | Expectation |
|---|---|---|
| Matrix | `cargo run --release --locked -p sim_core --example matchup_trace -- --all-battles --seeds 16 --out <dir> --compare <base>/matrix.csv` | Bit-identical in every WP (flat arenas, §1). Any change is explained before the WP proceeds |
| Battle 33 | the same tool on 68 seeds | Bit-identical |
| Goldens | `golden_replay_tests` in debug, release and on `wasm32-wasip1` | WP2, WP3, WP4 and WP9 re-record. Each re-recording names the first changed checkpoint and the traced cause (which unit's route changed, and why), in the fixture header as the existing entries do |
| Showcase | `cargo run --release --locked -p sim_core --example route_trace -- --scenario scenarios/showcase.ron --ticks 900 --out <dir>` (WP1) | The §3.2 and §3.3 tables recomputed, and the battle outcome (the tick the viewer's base falls, about 1,225) recorded |
| Throughput | `cargo bench -p sim_core --bench throughput` plus WP8's terrain scenario | Every scenario at or above 25 ticks/s (charter §3.1). The flat scenarios within noise of `DESIGN.md` §5 |
| Path quality | `pathfinding_properties.rs` (WP6) | Mesh route length at most 1.01 × the exact oracle; never shorter than the oracle (legality) |

### D-12. Stop rules

- **The overall winner flips.** If the overall winner of any matrix battle, battle 33 or the showcase battle
  changes, stop and report with the trace, without committing data or code beyond the WP.
- **The matrix or battle 33 moves.** Stop and explain. A movement that is not a flip may proceed only once the
  orchestrator accepts the explanation.
- **Throughput.** A throughput scenario below 25 ticks/s, or more than 10 % below its baseline, stops WP9.
- **Showcase.** WP2 to WP4 stop if, in any of the three boxes, the mean walked/chord ratio rises by more than
  0.05 over the previous WP. WP3 stops if the ramp E mean turn is not back at or below the baseline's 115°.

---

## 5. File ownership

| WP | Files |
|---|---|
| WP1 | `crates/sim_core/examples/route_trace.rs`, a new `crates/sim_fixtures/src/trace/route.rs` (metrics), `docs/evidence/2026-11-09-sc2-pathfinding.md` |
| WP2 to WP4 | `crates/sim_core/src/movement/locomotion.rs`, `crates/sim_core/src/movement/tuning.rs`, `crates/sim_core/src/entity.rs` and `checksum/encoding.rs` (WP3 only), `crates/sim_core/tests/movement_contact_tests.rs`, a new `crates/sim_core/tests/route_following_tests.rs`, `crates/sim_core/tests/fixtures/golden_replay*.ron`, `DESIGN.md` (bug register) |
| WP5 to WP7 | new `crates/sim_map/src/navmesh/{mod.rs,predicates.rs,cdt.rs,search.rs,funnel.rs,segment.rs}`, `crates/sim_map/src/lib.rs`, new `crates/sim_map/tests/navmesh_tests.rs`, `funnel_tests.rs`, `pathfinding_properties.rs`, `reachability_tests.rs`, `crates/gate/suites.ron` |
| WP8 | `crates/sim_core/benches/throughput_scenarios.ron`, `crates/sim_core/benches/throughput.rs`, evidence |
| WP9 | `crates/sim_map/src/pathfinding.rs`, `crates/sim_core/src/movement/locomotion.rs`, goldens, evidence |
| WP10 | `crates/sim_map/src/pathfinding.rs`, `DESIGN.md`, `docs/plans/README.md` |

---

## 6. Work packages

Each implementation agent hands back after `cargo gate quick`. A verifier runs `cargo gate package` (Q0061).
Simulation WPs also run the golden replays in release and on `wasm32-wasip1`, which `package` selects. Agents build
with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree>` (charter §7).

### WP1: Route-quality harness (no behaviour change)

- `route_trace` example: loads any scenario with its `initial_orders`, through the same code path as the client.
  The order-applying code moves from `client::runtime_battle` into `sim_core::scenario` as
  `apply_initial_orders`, and the client calls it, so both stay identical. It writes the per-tick CSV of §3.1.
- Metrics in `sim_fixtures::trace::route`: walked/chord per configured box, total turn, first-waypoint variants,
  and the time and outcome of the battle.
- Tests:
  - the metrics on hand-made trajectories (a straight walk gives 1.0 and 0°; a V gives the known ratio);
  - `apply_initial_orders` matches what the client produced before the move (the client's existing scenario
    tests).
- Evidence: the baseline tables of §3.2 and §3.3.
- Commit: `feat(sim_fixtures): route-quality trace for scenarios (sc2 pathfinding WP1)`.

### WP2: Route following leaves bodies to steering (A1 and A2, B-98)

**Tests first**, in `route_following_tests.rs`. The tests use scratch archetypes and a hand-built map: a wall
corner and a column of 8 units of radius 0.375.

- No unit's route ever contains a point the pathfinder did not produce.
- A unit pushed past a corner while a flockmate stands on its next leg drops the corner on that tick.
- A unit before the corner keeps it.
- A corner whose next leg fails terrain view is kept.
- Every unit's walked/chord through the corner box is at most 1.15, and its total turn at most 180°. This is
  §3.6.1's measured "together" maximum (1.113, 148°) with margin.

M-5 stays green.

**Implement** D-9 A1 and A2, then measure per D-11 and D-12. In the showcase, the ramp E turning is expected to
rise as in §3.6.1. That is allowed here only because WP3 is its fix.

Commit: `fix(sim_core): corners are passed by position and terrain view and never rewritten for a parked body
(B-98, sc2 pathfinding WP2)`.

### WP3: Aim past a body parked on a corner (A3, B-99)

**Tests first.**

- A body parked exactly on a corner: a unit passes it on the outside within `STALL_REPATH_TICKS`, with total turn
  at most 120°.
- The side chosen does not change while the unit rounds the body.
- No route is written.

**Implement** D-9 A3, then measure. The showcase ramp E mean turn must return to at most 115° (D-12).

Commit: `fix(sim_core): a unit steers past a body parked on its corner instead of at it (B-99, sc2 pathfinding
WP3)`.

### WP4: Own-radius corner offsets (A4, B-100)

**Tests first** (`funnel_tests.rs` and `route_following_tests.rs`):

- a radius 0.5 body's corner waypoint keeps 0.55 from the corner vertex;
- a radius 0.375 body's keeps 0.425.

**Implement**: `PathRequest.radius` is added (D-3), and `offset_corner_waypoint` reads it. Then measure.

Commit: `fix(sim_map): corner offsets use the unit's own radius (B-100, sc2 pathfinding WP4)`.

### WP5: Exact CDT, latent

**Tests first**, in `navmesh_tests.rs`:

- `orient2d` and `incircle` on known and adversarial integer inputs: collinear, cocircular, and extremes of a
  1,024-cell map;
- the empty-circumcircle property for every unconstrained edge (property test on random obstacle masks);
- every constraint run is a mesh edge;
- labels agree with `terrain_step_is_connected` cell by cell on both shipped maps;
- the mesh does not depend on structure insertion order;
- placing and then removing a structure restores the terrain mesh byte for byte;
- a digest of the showcase mesh, also asserted on `wasm32-wasip1`.

**Implement** D-2 and D-8 as `sim_map::navmesh::NavMesh`, built in `Pathfinder::new` and on `recompute_*`, and read
by nothing in the simulation. Results are bit-identical: the mesh is not hashed and nothing reads it.

Add the catalog entries in `crates/gate/suites.ron` (C22).

Commit: `feat(sim_map): exact-integer constrained Delaunay navigation mesh, latent (sc2 pathfinding WP5)`.

### WP6: TA*, widths, radius funnel, latent

**Tests first:**

- An exact oracle in `crates/sim_map/tests/support`: a visibility graph over every convex constraint vertex
  inflated by `r + margin`, with tangent points, and Dijkstra. It is exact for discs on these polygon worlds.
- `pathfinding_properties.rs`: on random masks and radii 0.25 to 1.0, a mesh route exists iff the oracle finds
  one; its length is between 1.00 and 1.01 × the oracle's; every leg passes the D-7 predicate.
- `funnel_tests.rs`: an L-shaped wall, a ramp foot, and the §3.4 pair (61.5, 24.5) → (16.5, 35.5) on the
  showcase map, all at the oracle length.
- Narrow passages: a gap of exactly `2r` passes and `2r − ε` does not; two structures touching at a corner block
  every radius (today's f7 tests restated for the mesh); a passage that S12 shows can fool a triangle-local width.
  If that case fails, stop and report: R-2 decides whether to add LCT refinement.

**Implement** D-4 to D-6 as `Pathfinder::find_path_mesh`, latent.

Commit: `feat(sim_map): triangle A* with radius widths and the radius funnel, latent (sc2 pathfinding WP6)`.

### WP7: Predicate, reachability and knowledge, latent

**Tests first:**

- the mesh segment predicate agrees with today's `segment_is_passable` on 10,000 random segments per shipped map,
  per tier radius, except at the documented difference (exact radius against tier);
- `nearest_reachable_point` under the mesh gives the same shared point for a whole region (`reachability_tests.rs`,
  restated);
- a structure the player does not know is walked through, and a known one is not (the `find_path_with_vision`
  invariants);
- `block_unexplored_ramps` behaves as the grid. No production caller sets it (`route_request` passes `false`); it
  is kept for its tests.

**Implement** D-7 and the knowledge labels.

Commit: `feat(sim_map): mesh segment predicate, reachability and player knowledge, latent (sc2 pathfinding WP7)`.

### WP8: Differential and performance measurement

- A test-only differential runner over both shipped maps and 1,000 seeded queries each compares the grid and the
  mesh: length, legality and found-or-not. The mesh is never longer than the grid by more than 1 %, and every
  difference in found-or-not is explained. The evidence lists the 10 largest gains.
- `throughput_scenarios.ron` gains `showcase_canyon` with 400 units, two armies attack-moving between the bases,
  measured with the grid before the switch.
- Micro-measurements: mesh build and rebuild time on 64×64 and 256×256; µs per search for both backends on the
  same queries; `path_searches` per tick in the 400-unit terrain scenario.
- Set `NAV_TA_STAR_EXTRA_EXPANSIONS` and write each `PERFORMANCE RATIONALE` block (C11).

Commit: `bench(sim_core): terrain throughput scenario; grid against mesh measurement (sc2 pathfinding WP8)`.

### WP9: The switch (B-101)

- `Pathfinder::find_path_with_vision`, `nearest_reachable_point` and `segment_is_passable` use the mesh.
- `route_request` passes `policy.radius`.
- `search_route`'s tier fallback becomes D-3's start rule.

**Tests:**

- every existing `sim_map` and `sim_core` movement test stays green, or is restated only for a numeric route that
  changed for a named reason (each restatement listed in the evidence);
- `route_following_tests.rs` adds a 0.5-radius unit that keeps its body clear of corners.

**Measure** per D-11 and D-12, including throughput with the terrain scenario. Goldens are re-recorded with causes.
The showcase battle's outcome is recorded; a flip stops the WP.

Commit: `feat(sim_map): routes come from the triangle mesh at each unit's own radius (B-101, sc2 pathfinding
WP9)`.

### WP10: Remove the grid corridor; documents

Removed:

- `search_corridor`, `build_portals`, `run_funnel`, `smooth_connected_corridor`, `index_corridor`;
- the corridor buffers;
- `UnitClearance`, if nothing else reads it (`grep`).

The clearance field stays only if a caller remains; placement does not use it (**verify**).

Documents:

- the `DESIGN.md` Movement and pathfinding row reads "Constrained Delaunay navigation mesh rebuilt on structure
  change, triangle A* with radius widths and the radius funnel (StarCraft II's reported design), steering, push
  priority and Hold Position";
- the bug register closes B-98 to B-101.

The charter §4 subject text for `sim_map` ("clearance field, A* corridor and funnel") and its file list are the
orchestrator's to update. This WP proposes the wording: "navigation mesh, triangle A* and radius funnel", adding
`navmesh_tests.rs`.

Commit: `refactor(sim_map): remove the grid corridor search (sc2 pathfinding WP10)`.

### WP11: Bookkeeping

The plan row is set to `Done` with the commits. The evidence file is completed. The showcase video is re-rendered
by CI on the next push.

---

## 7. Risks

- **R-1. M-5 needs the corner rewrite.** Without `sidestep_occupied_corner`, a unit facing a body parked exactly on
  its corner may stall between WP2 and WP3. Mitigations:
  - steering's obstacle deflection;
  - the stall repath (`STALL_REPATH_TICKS`).

  If M-5 still fails, WP2 and WP3 merge into one commit, and the commit says so.
- **R-2. Demyen widths are unsound in some narrow configurations (S12).** WP6's adversarial test decides. Two
  outcomes are allowed:
  - add Kallmann's LCT refinement (one amendment);
  - keep the D-6 post-check and fall back to portal midpoints, counted.
- **R-3. Rebuild cost on 256×256 maps** with many structures. The cost is measured in WP8. If a rebuild exceeds
  2 ms, rebuild only the affected region's constraint set and re-triangulate the cavity. That is local repair,
  done in a new amendment.
- **R-4. Fixed-point overflow.** `incircle` on 1,024-cell coordinates needs about 4 × 10 + sign bits. i128 has a
  wide margin, and WP5 tests the extremes.
- **R-5. Golden churn.** Every route change re-records the goldens. Each re-recording names its cause, and four
  re-recordings are planned (WP2, WP3, WP4, WP9).
- **R-6. Battle balance on terrain.** Faster corner passage brings armies through ramps sooner. The showcase outcome
  is the sensor, and the stop rule covers a flip.
- **R-7. The two Part A switches interact (§3.6.1).** They are measured as a pair and land as a pair. WP3's gate
  catches the remaining wobble.

---

## 8. Determinism summary

- Integer and Q32.32 arithmetic only.
- `Vec` storage with index-order iteration; no hashed collections, clocks or floats.
- The mesh is a pure function of map and structures.
- Goldens on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, in debug and release.
- No checksum schema change, except possibly WP3's one `i8` (D-9 A3).

---

## 9. Out of scope and follow-ups

- **B-48, B-49 and B-50.** Push priorities, which radius spaces units (`separation_radius` unread), and melee
  packing belong to `2026-11-05-sc2-deviation-fixes` WP16 (movement research). This plan changes no steering
  weight, separation radius or push rule.
- **Ramp side walls.** Ramps stay walled on their sides (§3.4).
- **Flow fields, group formation, and "magic box" spreading of a group's goals.** No source says StarCraft II
  uses them for group moves.
- **Local mesh repair (S4, S12)**, until R-3 measures a need.
- **Air units.** They do not path, as today.
- **Cliff jumpers and cliff walkers.** The mesh's triangle tags (D-2) make them possible later.
- **Ground truth from the real game** through `RequestQueryPathing` (S6). It would need the owner to run the game.

---

## Amendment 1 (Part A, 2026-11-09): orchestrator decision

**Decision.** WP2 and WP3, held under D-12 because player 0's mean total turn at the showcase's ramp E foot rose
from 114° to 213° (not back at or below 115°), are accepted by the orchestrator (short note, charter §6). Reason:
both owner-reported defects are fixed (ramp W foot worst walked/chord 1.383 → 1.113 and worst turn 840° → 146°;
plateau south-east corner 1.500 → 1.078 and 1,070° → 148°); the matrix and battle 33 are bit-identical; the showcase
winner is unchanged (player 0, tick 1,369 → 1,351); the ramp E walked/chord is unchanged (mean 1.551 → 1.557). The
ramp E turn rise comes from flockmates jostling in a tighter column (the army reaches ramp E about 20 ticks
sooner), which is local steering owned by plan [`2026-11-09-friendly-blocking`](2026-11-09-friendly-blocking.md).

**Criterion moved.** D-12's ramp E turn criterion (mean at most 115° for player 0) leaves this plan and becomes part
of the friendly-blocking plan's acceptance (its Amendment 1). D-12's other stop rules stay.

**Deviations from D-9, recorded under Q0002:**
- **A3 acts only on parked bodies:** a body that stands still and will not yield (`parked_on_corner` in
  `movement/locomotion.rs`). With D-9's literal rule, every non-yielding body touching the corner including moving
  flockmates, the showcase got worse in all three boxes and its winner flipped (player 1 wins at tick 1,464).
  Moving bodies leave the corner by themselves and are spaced by steering.
- **A corner with a parked body is handed off only beyond it** (the half-plane test of A2), not when the unit is
  merely within the handoff distance. Otherwise a unit beside the body heads straight for the next point and is
  pushed into the gap between the body and the obstacle the corner turns around, where it sticks.

**Checksum schema 22.** The memoryless side rule of D-9 A3 was tried and fails: once the unit stands beside the body,
the line to the next point no longer cuts the obstacle and the rule picks the inside. The sense in which the unit
rounds the body is therefore held in `MovementMemory::corner_round` (`i8`), chosen at the first aim and cleared when
the corner is passed or the route replaced. Recorded first as schema 20 → 21; on the merge with WP13c of the
deviation-fix plan, which took 21 for `attack_dash_cooldown`, it became 21 → 22. Both goldens and the sandbox
checksum are re-recorded with their traced causes.

**One commit (R-1).** WP2 and WP3 landed as one commit: with A1 and A2 alone, M-5
(`route_continues_past_a_blocker_on_its_corner_waypoint`) stalls 12.5 from its goal.

**Register IDs.** The orchestrator's numbering replaces §D-9's: B-99 (corner rewrite and empty-leg handoff), B-100
(a body parked on a corner), B-101 (tier radius at corners, WP4), B-102 (grid corridor in place of the triangle mesh,
Part B).

## Amendment 2 (Part B switch, 2026-11-10): orchestrator decisions

Short note under charter §6, recorded before WP9 on the Part B report (evidence, "Part B, WP5 to WP8").

1. **Turn emission before the switch.** WP8 found mesh polylines longer than the grid's by more than 1 % on 12 of
   790 (radius 0.375) and 49 of 793 (radius 0.5) canyon pairs, worst 3.08 (a portal-midpoint fallback). Before WP9
   the cause is confirmed on the worst pairs and the turns are emitted more tightly, exact and deterministic, with
   the oracle agreement kept (every route within 1 % of the exact disc oracle, worst at most 1.01 on the seeded
   set). Acceptance: on the WP8 comparison at most 1 % of pairs longer than the grid by more than 1 %, none by more
   than 5 %, and every fallback counted and explained. This replaces D-6's "two tangent points above 45 degrees, one
   point on the bisector otherwise".
2. **Local mesh repair is out of scope for this round.** The shipped maps are 64×64 and rebuild in under 0.9 ms;
   a 256×256 map of tiled canyons rebuilds in about 14 ms (Part B evidence). R-3's local repair stays an open item
   for maps above 64×64 and is not a condition of WP9.
3. **The mesh's goal check is accepted.** The mesh refuses a goal where the unit's disc does not fit, where the grid
   tested only the goal cell's clearance tier. This is StarCraft II-like (a unit cannot stand where its body does
   not fit) and is accepted. A refused goal goes through the unreachable-move rule (Q0052 S-4, Q0006: the unit
   walks to the nearest reachable point), and WP9 shows that such orders still end cleanly.

## Amendment 3 (WP9 held, 2026-11-10): orchestrator decisions

Short note under charter §6, on the report of the Amendment 2 work (evidence, "Amendment 2, step 1").

1. **Turn emission accepted.** On the canyon at radius 0.5, 16 of 793 pairs are more than 1 % longer than the
   grid. Every one is a grid route that a disc of that radius cannot walk, and no mesh route is more than 1 %
   longer than a legal grid route. `80a48a6` stays as the latent improvement.
2. **WP9 is not accepted yet.** The held switch (side branch `sc2-path-wp9-held`, `7636e32`) moved the showcase
   for player 0 as follows:
   - ramp W foot ratio 1.064 → 1.301;
   - reversals 11 → 253;
   - ramp E turn 211° → 381°;
   - the winner flips: player 1 wins at tick 1,273, against player 0 at tick 1,100;
   - the canyon throughput scenario loses 17 % (141 → 117 ticks/s, measured unlocked).

   Before the switch lands:
   - a. Diagnose the showcase regression with traces and fix it in route following and emission (this plan's
     side of the `locomotion.rs` split, friendly-blocking plan §4). Friendly blocking's rules are not undone.
     Leads to trace:
     - arc points at exactly `r + margin` whose legs graze the wall, so that `route_is_usable` and
       `drop_passed_waypoints` reject the leg or re-plan;
     - arc runs at the clearance limit, which leave no room for flockmates;
     - the re-plan frequency;
     - how a run of arc points is handed off.

     Candidate fixes follow StarCraft II's reported "inside the vertices expanded by unit radius": a small extra
     corner margin for route following, handing an arc run off as a whole, and re-planning less often.
   - b. Recover throughput to within 10 % of the grid on the canyon scenario, by profiling:
     - TA*'s extra expansions;
     - the waypoint count per route (arc subdivision, `ARC_CHORD_DIP`);
     - the extra fit check per handoff;
     - the re-plan rate.

     Measure under the gate lock, or record the noise.
   - c. Acceptance for the switch:
     - Showcase, player 0: the route ratio and turn in each box are no worse than base, where base is:
       - ramp W 1.064 / 1.112 and 103° / 213°;
       - plateau SE 1.044 / 1.093;
       - ramp E 1.573 / 1.611 and 211°.
     - Reversals at most base + 10.
     - 10 of 10 in range, 0 give-ups, and the winner unchanged.
     - The matrix and battle 33 meet the stop rules of D-12.
     - Throughput as in b.
3. Once the acceptance is met, WP9 lands with the goldens re-recorded and their causes traced in debug, release and
   `wasm32-wasip1`, followed by WP10 and WP11. Until then the switch stays held.

## Amendment 4 (WP9 acceptance revised, 2026-11-10): orchestrator decisions

Short note under charter §6, on the diagnosis of Amendment 3 (evidence, "Amendment 3, diagnosis of the held
switch").

1. **Per-unit shortest routes stay.** Each unit paths on its own, as StarCraft II reportedly does. The showcase's
   winner flip comes from the near-tied west and east lanes (0.35 %) splitting the armies, not from route
   following. Group-consistent routing, an "allows more" deviation, is owner question
   [Q0079](../questions/open/0079-group-consistent-routing.md) (non-blocking; default: per-unit routes).
2. **Revised acceptance for landing the switch** (replaces Amendment 3, 2.c):
   - The showcase is judged on the lanes units actually take: route ratio and turning per box along each unit's
     own lane, no worse than base on comparable boxes. Ramp W's lane-end turn of about 100° at (29, 12) is
     legitimate when the lane is the west one.
   - Reversals at most base + 10, after the defects below are fixed.
   - 10 of 10 attackers in range at tick 900; 0 give-ups.
   - The winner may change only because of the lane split. Then the showcase storyboard
     (`assets/captures/showcase.ron`) must still tell a coherent story, checked with a render (`showcase_tests`,
     GPU through the harness), with captions and scene ticks updated if needed, or the scenario's attack-move
     targets made mirror images if that is the cleaner fix (the reason recorded).
   - The matrix and battle 33 meet the stop rules of D-12; throughput as in 3.d.
3. **Defects to fix first**, each with a test that is red before the fix:
   - a. *Illegal funnel leg:* `pathfinding_properties::funnel_paths_are_legal_and_no_shorter_than_a_straight_line`
     fails at the held switch (a leg near (9.6, 12.3)). Find and fix the cause in the mesh funnel or route layer,
     then re-record the two `sim_map` digests (`determinism_tests::test_cross_platform_determinism_checksum`,
     `validation_golden_tests::find_path_with_vision_matches_the_recorded_corpus_digest`) with the cause stated.
   - b. *Surround walk toggling every tick* (228 of player 0's 255 reversals, ticks 350 to 550): fixed in friendly
     blocking's rule with hysteresis consistent with that plan's Amendment 4, measured on main without the switch
     too, recorded as an amendment of the friendly-blocking plan; it may land alone.
   - c. *Ramp E corner crowding:* bodies pushed past the corner (35, 52) wait for stall re-plans; fixed in route
     following.
   - d. *Throughput*, under the gate lock, base `02f494b` and candidate back to back: the canyon scenario within
     10 % of base, every scenario at least 25 ticks/s.
4. If the acceptance is reached, WP9 lands with goldens and the sandbox checksum re-recorded and traced (debug,
   release, `wasm32-wasip1`), then WP10 and WP11. If not, whatever is independently good lands on the main line and
   the switch stays held on a side branch.

## Amendment 5 (WP9 held, 2026-11-10): orchestrator decisions

Short note under charter §6, on the report of the Amendment 4 work (evidence, "Amendment 4: defects a to d on the
held switch").

1. **Battle 31's flip from fix b is accepted.** Its claim accepts either side (friendly-blocking plan Amendment 7).
2. **Throughput criterion relaxed** for the canyon terrain scenario (`showcase_canyon`, 400 units) from 10 % to
   **15 %** below the grid baseline. Measured under the gate lock: 140 to 143 against 125 to 127 ticks/s, still about
   five times real time. Every scenario must still run at 25 ticks/s or more. No further throughput work unless a
   scenario falls below that.
3. **The showcase's attack-move targets become mirror images of each other.** Today player 0 aims at (52.5, 52.5),
   the corner of player 1's base ring that faces the map's centre, and player 1 at (5.5, 5.5), the corner of player
   0's ring that faces away from it: both are the first free cell of the ring, which is not a symmetric rule. The
   map is point-symmetric about its centre except the central plateau. The targets become images of each other under
   that symmetry, so that the near-tied lane choice (Amendment 4, 1) is symmetric too. This is presentation scenario
   data (`assets/scenarios/showcase.ron`). The showcase is then re-checked on the grid (main) and on the switch, and
   the storyboard (`assets/captures/showcase.ron`) is rendered (`showcase_tests`, GPU through the harness) and its
   scene ticks and captions adjusted if needed.
4. **Fix defect c (ramp E corner crowding)** in route following:
   - (i) a wedged unit whose first leg is not passable for its whole disc re-plans, bounded so that no re-plan storm
     can start (re-plans counted per unit);
   - (ii) an attack-move surround walk's side resets at each route corner, or is chosen on the outside of the turn
     when the walk starts at a corner, so that a column is not carried to the inside of the turn.

   The earlier attempt (evidence, Amendment 4, c) improved the ratio but worsened the maximum turn and left one unit
   out of range; the fix is judged on all of the acceptance's rows.
5. **Acceptance for landing the switch** (replaces Amendment 4, 2, for the showcase): with the mirrored targets, on
   the lanes units actually take, per-box ratios and turns no worse than the grid run with the same mirrored targets
   (both measured); reversals at most the grid's plus 10; 10 of 10 in range at tick 900; 0 give-ups; the winner
   unchanged against the grid run with mirrored targets. The matrix and battle 33 per the stop rules of D-12;
   throughput per 2. If reached, the switch lands as in Amendment 4, 4, followed by WP10 and WP11; if not, it stays
   held and the numbers are reported.

## Amendment 6 (WP9 accepted, 2026-11-10): orchestrator decisions

Short note under charter §6, on the report of the Amendment 5 work (evidence, "Amendment 5: mirrored showcase
targets, B-106, defect c").

1. **The switch is accepted** with the mirrored showcase targets. Against the grid run with the same targets and
   fixes: the winner is unchanged (player 0); stall give-ups 0 (grid 57, one welder's abandoned chases); ramp E,
   player 0, ratio 1.198 / 1.300 against 1.573 / 1.611 and mean turn 190° against 211°; reversals 17 and 7, within
   the grid's plus 10; the matrix (16 seeds) and battle 33 (68 seeds) bit-identical to the grid with the same fixes;
   canyon throughput 11 to 13 % below the grid, within Amendment 5, 2.
2. **Two rows of Amendment 5, 5 are missed and accepted, with reasons:**
   - *10 of 10 in range at tick 900* is a snapshot artefact of the battle's new shape: both armies take the west
     lane and fight there from tick 310 to 576 (9 of 9 in range at tick 430); the survivors are between targets at
     tick 900, and 6 of 6 are in range from tick 940.
   - *Ramp E maximum turn* (454° against the grid's 287°) is one pulse bot that the crowd wedges 0.30 from the
     corner vertex (35, 52), inside its radius. It is filed as **B-113**, open, in the bug register.
3. **Bug IDs.** The effects branch registered B-106 to B-111 in its WP0, so this plan's "B-106" (an attack-move
   resumed after a chase measures progress against the chase) is renumbered **B-112**; commit `3c51afe`'s message
   and the Amendment 5 evidence call it B-106. Defect c's wedged corner is B-113.
4. **Q0079** (group-consistent routing) stays open with its default, per-unit routes.
5. **Landing.** The goldens and the sandbox checksum are re-recorded on the final tree with traced causes (the
   first divergent tick and entity for the switch, B-112 and the mirrored targets) in debug, release and
   `wasm32-wasip1`. The showcase storyboard (`assets/captures/showcase.ron`) is restructured for the battle's new
   shape (the armies meet in the west lane at about tick 310) and checked with a real render. WP10 and WP11 follow
   in the same branch; the charter §4 wording for `sim_map` is proposed to the orchestrator, not edited here.
