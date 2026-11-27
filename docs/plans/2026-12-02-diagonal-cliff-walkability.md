# Blueprint: Diagonal Cliff Walkability, the Simulation Clipped to the Drawn Diagonal (Q0139; B-251)

The owner answered [Q0139](../questions/closed/0139-diagonal-cliff-overhang.md) with a fourth option: keep the
straight 45° diagonal cliffs that plan [`2026-12-01-straight-diagonal-cliffs`](2026-12-01-straight-diagonal-cliffs.md)
(the "drawing plan" below) put on `main`, and clip the **walkable** high-ground half cell along the same 45° line, as
StarCraft II does, so that no high-ground unit floats over the low floor and no low-ground unit is drawn in rock. The
current overhang stays an open bug until this plan lands.

- **B-251 (new, provisional):** the simulation's walkable high ground of a 45° staircase step cell reaches the cell's
  outer corner, half a cell beyond the drawn diagonal. A ground body at the step's outer corner or centre stands
  over the drawn low floor, 3.0 world units below its plateau (25 of 28 ground archetypes; drawing plan D-8).

The design in one paragraph. The drawing plan's predicate D-1 already decides, from a cell's 3×3 block alone, which
step cells are cut and along which line: the cell's anti-diagonal through the staircase's concave corners, the
foot `dx·x + dy·y = k` with integer `k`. This plan moves that predicate, unchanged, into `sim_map`, so the drawing
and the simulation read one function (D-1). The simulation then treats the open triangle of a cut cell beyond its
foot as **terrain no ground unit enters**, from either level (D-2). It does so in the one place every ground rule
already shares: the navigation mesh gets the foot as a diagonal `Wall` constraint and labels the triangles beyond
it `Blocked` (D-3). The mesh already supports diagonal unit lattice edges for chamfered footprints, so routes, the
radius funnel, the segment predicate, wall contact and the nearest reachable point need no change of their own. The
clearance field, the grid traversal predicate and the placement checks learn the same triangle (D-4 to D-6). A body
then stands at least its terrain radius behind the drawn foot, which is exactly the position it holds at an
axis-aligned cliff: the brink drop under the smallest body becomes 0.077, the figure every other cliff has (D-7). No
cell changes level, and nothing on the low side changes. Maps with no cut (every flat test map, `default_arena`)
are byte-identical except for the checksum's new section and schema number (§7).

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `2516a068` on branch
`diagonal-walk-plan`. It is a full plan, because it changes authoritative state, moves showcase worlds and the
checksum format, and spans `sim_map`, `sim_core` and `client`.

- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.**
  - Bug row **B-251**: `main` ends at B-250; branches `rallies` and `minimap` hold B-247 to B-249, and
    `missile-throw-plan` names B-251 as the next free number but registers none. Whichever registers first takes
    B-251.
  - Question **Q0146**: Q0144 and Q0145 are taken by `missile-throw-plan`.
  - Checksum schema **65**: `main` is at 62, `hangar-commands` takes 63 and `missile-throw-plan` 64.

---

## Contents

1. Diagnosis
2. What StarCraft II does
3. Survey: how to represent a clipped half cell
4. Requirements and charter mapping
5. Design decisions
6. Tests
7. Determinism, checksum, goldens, measured worlds and throughput
8. Coordination with the movement work
9. Acceptance
10. Work packages
11. Stop rules
12. Files
13. Out of scope and recorded decisions
14. Bug rows
15. Owner question

---

## 1. Diagnosis

### 1.1 What the simulation holds on a diagonal

Take a staircase whose high cells are `i + j ≤ n` and whose low cells are `i + j ≥ n + 1` (the drawing plan's §1.3).
Every rule of the simulation that decides where a ground body may stand reads whole cells:

- **Navigation mesh** (`sim_map/src/navmesh/mod.rs`). `EdgeGrid::new` makes every cell edge between two levels a
  `Wall` (`edge_kind`), and `ConstraintGraph::terrain_kind` documents "diagonal edges are never terrain". Triangles
  are labelled by the cell of their centroid (`label_triangle`), so the whole step cell `X` is `Open`.
- **Segment predicate and wall contact** (`navmesh/segment.rs`). A disc keeps `r` from blocking edges only, so a
  body may stand anywhere in `X` at clearance 0 from `X`'s two cliff edges. Its deepest point is local
  `(1 − r, 1 − r)`, beyond the drawn foot by `(1 − 2r)/√2`.
- **Grid traversal** (`traversal.rs`): `terrain_step_is_connected` and `ground_segment_is_connected` decide by cell
  edges, so every point of `X` is connected to every other.
- **Clearance field** (`pathfinding.rs`, `ClearanceWorkspace::mark_terrain_walls`): walls on cell edges only.
- **Placement** (`sim_core/src/movement/geometry.rs`, `ground_footprint_connected`; `GameMap::can_place_building`):
  whole cells.

### 1.2 What is drawn

The drawing plan cuts `X` along its anti-diagonal: the foot of the chamfer is the line `x + y = n + 1`, through `X`'s
centre and the staircase's concave corners `V`. Its wall rises behind the foot (`FOOT_WIDTH` 0.22, then the rim,
`RIM_WIDTH` 0.15). The half of `X` beyond the foot is drawn at the low floor. Its T-K measured the consequence
(drawing plan D-8):

| `r` | Archetypes | Centre beyond the foot | Drop under the centre |
|---:|---:|---:|---:|
| 0.3125 | 3 | +0.265 | 3.0 |
| 0.375 | 13 | +0.177 | 3.0 |
| 0.5 | 8 | 0.000 | 3.0 |
| 0.5625 | 1 | −0.088 | 2.89 |
| 0.75 | 1 | −0.354 | 0.022 |
| 0.875, 1.0 | 2 | ≤ −0.530 | 0 |

### 1.3 Why the foot is the right line to clip at

At an axis-aligned cliff the drawing puts the toe of the foot exactly on the cell edge, and the simulation's wall is
that edge (the terrain plan's D-1, invariant W1). A body at clearance 0 stands with its centre `r` behind the toe:
for `r = 0.3125` that is on the rim chamfer, a drop of `RIM_DROP · (0.37 − r)/RIM_WIDTH = 0.2 · 0.0575/0.15 =
0.077`, the terrain plan's figure. If the simulation's wall on a diagonal is the drawn foot, a body at clearance 0
stands `r` behind the foot along the normal, with the same profile below it: **the same 0.077**, and 0 for
`r ≥ 0.37`. No other line gives that. A line further out leaves part of the overhang; a line further in keeps bodies
off ground that is drawn as plateau.

### 1.4 What is not wrong

- **The low side.** The triangle beyond the foot is part of `X`, which no low unit could enter before; the low cells'
  walls stay where they are. The drawing plan's W1 holds and nothing low changes.
- **Rectangle and void convex corners** (`main`'s 45° half cut, B-250). The drawing plan keeps them, and so does this
  plan; draft Q0146 (§15) asks whether the same mechanism should clip them too.
- **Ridges, two-cell bands, cells beside ramps.** The predicate makes no cut there (drawing plan D-7), so the drawing
  and the simulation already agree.

### 1.5 Counts

From the drawing plan's WP0, measured with the production mesher (its evidence
[`2026-12-01-straight-diagonal-cliffs`](../evidence/2026-12-01-straight-diagonal-cliffs.md)):

| | `showcase_canyon` | `default_arena` | flat test maps |
|---|---:|---:|---:|
| full cuts (cells clipped by this plan) | 24, two runs of 12 at threshold 2 | 0 | 0 |
| walkable area removed | 12 cells (half of each cut cell) | 0 | 0 |
| cut cells whose centre lies on a 16-cell chunk seam | 4 | 0 | 0 |

WP0 re-measures every map the measured sets build (§7.4), with the `sim_map` predicate.

---

## 2. What StarCraft II does

Tags as in the precedent plans: **[DOC]** Blizzard material; **[DATA]** SC2GameData 5.0.14 (`5444e12f13`, layer chain
`core → liberty → swarm → void → voidmulti`); **[COMM]** community or editor observation; **[2ND]** second-hand
account; **[INF]** this plan's inference; **verify** where nothing confirms it. Data files were fetched on 2026-11-27
from `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/<layer>.sc2mod/base.sc2data/GameData/`
into `~/scratch/diagonal-walk-plan/data/`.

| # | Source | What it says |
|---|---|---|
| S1 | `core` CliffData.xml:3–7; `liberty` CliffData.xml (191 lines) | `CCliff` carries only `CliffMesh`, `CliffMaterial` and `EditorIcon` [DATA]. No pathing field |
| S2 | `core` CliffMeshData.xml:3–21; `liberty` CliffMeshData.xml | `CCliffMesh` carries `ModelPath`, `WeldNormals` and `CliffHeights` (2.0 world units per level, index 2 = 2.0 to index 15 = 28.0) [DATA]. Cliff geometry is a model, and its pathing is not in data: the engine or the editor derives it [INF] |
| S3 | s2client-proto `raw.proto` (the pathfinding plan's S6), <https://github.com/Blizzard/s2client-proto> | The API exposes a 1-bit `pathing_grid` raster [DOC]; it is an export, not the navigation representation (pathfinding plan §2.1) |
| S4 | Y. Miyake, IGDA Japan GDC 2011 AI report, slides 39–40 (the pathfinding plan's S2) | The navmesh is a Delaunay triangulation whose regions are tagged ground, **cliff**, building or none [2ND] |
| S5 | s2editor-guides, "Custom Footprints", <https://s2editor-guides.readthedocs.io/New_Tutorials/04_Data_Editor/074_Custom_Footprints/> | Pathing types are None, No Building, Unpathable Terrain, Ground and **Cliff**; "multiple pathing types are supported within a single cell"; the brush sizes are "Triangle: 1 cell, Sub-cell: 4 cells, and Cell: 16 cells" [COMM]. So pathing is painted at sub-cell resolution with triangles: the engine's walkable set is not whole cells |
| S6 | sc2mapster forum, search snippet only (Terraining FAQ thread 147300 and a footprint thread) | Cliffs have natural pathing that custom pathing overwrites; cliff pathing lets cliff scalers over and is otherwise unpathable [COMM, **verify**: the pages themselves could not be fetched] |
| S7 | `liberty` FootprintData.xml (e.g. `Shape` with `Radius` and `Borders`, lines 22–26) | Footprints carry contour shapes finer than cells; plan `2026-11-18-contour-footprints` built them on the quarter lattice [DATA] |
| S8 | The owner's answer to Q0139 | StarCraft II's diagonal cliffs are straight "with neither floating high-ground units nor low-ground rock clipping" [owner] |

**Reading.** StarCraft II's cliff is a band of its own pathing class (S4, S5, S6) under the drawn face, with
sub-cell, triangular pathing (S5), so neither side's walkable ground reaches the drawn face [INF, **verify** in the
editor's pathing overlay]. Our cliffs have no thickness: a cliff is a cell edge, and the drawn wall stands wholly on
the high side (terrain plan D-1). The equivalent here is the one line of §1.3, the drawn foot, as the high side's
boundary. The low side keeps its cell edges, so it stays `0.707` from the foot at a step's outer corner, where the
drawing shows low floor. That gap is the analogue of StarCraft II's band on the low side, and it is recorded as a
minor deviation of detail (D-2, §13): no unit stands in rock or over a drop, which is what the owner asked for.

---

## 3. Survey: how to represent a clipped half cell

The owner's rule (Q0091 follow-up, 2026-11-20): survey the algorithms first, prefer a simpler sound design, and fit
nothing to a test set.

| Option | How | For | Against | Verdict |
|---|---|---|---|---|
| **A. Diagonal wall in the constrained Delaunay mesh** | The foot becomes a chain of diagonal unit lattice edges of kind `Wall`; triangles beyond it are `Blocked` | The mesh is the one geometry routes and movement share (pathfinding plan D-7). Demyen and Buro's TA* and radius funnel (pathfinding plan S11) and Kallmann's clearance triangulations (S12) take any polygonal obstacle. Diagonal unit edges already exist for chamfered footprints (contour-footprints D-3, D-4), with exact `i128` predicates. Comparable to Recast's convex-area marking (`rcMarkConvexPolyArea`), which marks a polygon unwalkable inside a cell-based heightfield | Every rule outside the mesh that reads whole cells must learn the triangle (D-4 to D-6) | **Chosen** |
| B. Sub-cell grid (quarter cells or triangles, as StarCraft II's editor paints, S5) | Walkability per triangle of each cell | Closest to the editor's data model | Rewrites every per-cell rule (vision, splash `SameCliff`, altitude, placement, fog, clearance, traversal) for an effect on 24 cells | Rejected: cost out of proportion |
| C. Give the outer triangle to the low level | The triangle beyond the foot becomes walkable low ground | Matches the drawing exactly: low units could walk to the foot | A cell with two levels: altitude, vision height, `SameCliff` splash, cliff-level vision rules and every `cell.cliff_level` reader would need sub-cell answers | Rejected; recorded in §13 so it can come back |
| D. Move drawn units onto the drawn surface | Presentation lifts or shifts sprites | No simulation change | Draws units where the simulation does not hold them (charter §3.2); rejected already by the drawing plan §11 | Rejected |
| E. Route margin near diagonals | Routes keep away from cut corners; contact unchanged | Small change | Not a guarantee: crowd pushes put bodies on walls (Q0134's evidence), and the corner stays reachable | Rejected: it does not meet the requirement |

---

## 4. Requirements and charter mapping

| Requirement | Where met |
|---|---|
| Clip the walkable high-ground half cell along the drawn 45° line (Q0139) | D-1, D-2, D-3 |
| Neither floating high-ground units nor low-ground rock clipping | D-7 (drop ≤ 0.077 under the smallest body in cut cells), D-2 (low side unchanged); T-K in cut cells, review capture |
| One rule for drawing and simulation | D-1: the predicate moves to `sim_map`, the mesher calls it; WP1 proves the mesh unchanged |
| Fixed-point determinism (charter §3.1) | D-8: integer predicate, exact `i128` half-plane tests, no float, no hashed collection |
| Golden re-record with the four-build proof; schema bump, next free at merge | §7.1, §7.2 |
| Matrix and harness effects | §7.3, §7.4 (census; worlds without cuts byte-identical by Lemma L-2) |
| Throughput by callgrind instructions against the 400-unit row (54.44 M per tick) | §7.5 |
| No fitting to the test set; simpler deviations allowed; algorithms surveyed | §3; acceptance on principled properties (§6), not on battle winners (§7.4) |
| Coordination with the contact-layer acceptance and wall-clearance WP1 | §8 |
| Charter §1.1 (no `unwrap`, typed errors), §2 (docs, 100-line functions, performance rationale), §3.3 (no per-frame allocation), §4 (tests), §7 (GPU lock) | D-8, D-9, §6, §10 |

---

## 5. Design decisions

Notation as in the drawing plan. For a cut of cell `X = (i, j)` with corner direction `(dx, dy) ∈ {±1}²`: `C` is
`X`'s corner towards the low side, `L₁ = X + (dx, 0)` and `L₂ = X + (0, dy)` are the low neighbours, and the
**foot** is the line `dx·x + dy·y = k`, with `k = dx·Cx + dy·Cy − 1` an integer (`Cut::foot_constant` today). The
**outer triangle** `T(X)` is the open triangle of `X` beyond the foot: `dx·x + dy·y > k` inside `X`. Its three sides
are the foot and the two cell edges `X|L₁` and `X|L₂`.

### D-1. One predicate, in `sim_map`

New module `crates/sim_map/src/diagonal.rs`, re-exported as `sim_map::diagonal`:

- `DiagonalCut { cell: (u32, u32), dir: (i8, i8), la: i8, lb: i8, t_hi: i8 }`, the fields of the client's `Cut`
  (levels are marching levels: plateau level, −1 for the void).
- `DiagonalCut::of_block(cell, block: &[[Option<i8>; 3]; 3]) -> Option<DiagonalCut>`: the client's `Cut::of_block`,
  moved verbatim (the same order of directions, the same `find_map`, the same `t_hi = min(la, backed, reach)` and
  `lb < t_hi` test). `None` in the block is a ramp cell, as today.
- `marching_level(map, x, y) -> Option<i8>`: the client's `cut_level`, moved with its `// PERFORMANCE RATIONALE:`
  block (abyss and off-map −1, ramp `None`, else the cliff level).
- `GameMap::diagonal_cut(x, y) -> Option<DiagonalCut>`: reads the 3×3 block through `marching_level` and calls
  `of_block`. It returns `None` early, after two reads, unless `X` is at least one level above some 4-neighbour
  (D-9).
- `DiagonalCut::foot_constant() -> i32`, and `DiagonalCut::beyond(p: SimVec2) -> bool`: whether `p` lies in the
  open half-plane beyond the foot, evaluated exactly as `dx·x_raw + dy·y_raw > k · 2³²` in `i128`.

**The client delegates.** `client::terrain::mesher::diagonal::Cut` keeps its presentation methods (`normal`,
`centre`, `rim_point`, …) and is built from `DiagonalCut` (`Cut::from(DiagonalCut)`); `DualCuts` keeps its single
4×4 read and hands each 3×3 sub-block to `DiagonalCut::of_block`. The unit test `cut_level_classifies_as_surface_class`
stays in the client and compares `sim_map::diagonal::marching_level` with `surface_class`. WP1 proves the move by
the mesher golden hashes, which must not change.

**Why `sim_map`.** The cut is terrain (levels, abyss, ramps), a pure function of nine cells. `client` depends on
`sim_map` (charter §5), so the drawing reads the simulation's rule, never the other way round, and the two cannot
drift apart.

**When it is evaluated.** On demand, never cached in `GameMap`: `GameMap::cells_mut` and `cell_mut` are public (the
editor and tests mutate terrain), and a stored table could go stale. The mesh computes its cut table in
`NavMesh::build` from the map it is given (D-3), so it stays "a pure function of the map's cells and structures".

### D-2. What a cut means: the outer triangle is a wall region for every ground unit

`T(X)` is terrain no ground unit stands on or crosses, at any level. **High side:** `X`'s walkable region becomes the
closed half of `X` behind the foot. **Low side:** unchanged; `T(X)` was part of `X` and closed to low units, and
stays so. **Air:** unchanged (air movement does not read ground terrain). **Per-cell rules stay per cell:** `X`
keeps its level, altitude, vision height, `SameCliff` splash membership and fog cell, because those rules describe
the cell, not where a body may stand, and no body stands in `T(X)`.

Recorded deviation (minor, detail): a low body at `C` stands 0.707 from the drawn foot, on drawn low floor that it
cannot enter. StarCraft II's cliff band keeps low units back from the face as well (§2), and the alternative
(option C of §3) splits a cell's level. It is in §13 so it can come back.

### D-3. The mesh: a diagonal `Wall` along the foot, `Blocked` beyond it

In `navmesh/mod.rs`:

1. `EdgeGrid` gains `cuts: Vec<u8>`, one code per cell (0 none, else the direction), filled in `EdgeGrid::new` from
   `GameMap::diagonal_cut`. `contact_free_cells` marks a cut cell not contact-free (its foot blocks).
2. `ConstraintGraph::terrain_kind` returns `Some(EdgeKind::Wall)` for a diagonal unit lattice edge that lies on the
   foot of the cut of the cell containing it: the edge's direction is `NorthWest` for `dx·dy = 1` and `NorthEast`
   for `dx·dy = −1`, and its midpoint lies in that cell's interior on the foot (an exact integer test on the quarter
   lattice: `dx·(2x + sx) + dy·(2y + sy) = 2·LATTICE_PER_CELL·k`). Its doc comment ("diagonal edges are never
   terrain") is replaced by the rule.
3. `is_vertex` needs no change: the foot's ends `P₁`, `P₂` are cell corners, which are candidates already, and
   three or more incident constraints make them vertices. The quarter points strictly inside the foot carry one
   straight pair of one kind and stay non-vertices.
4. **No crossing:** the foot lies in the open interior of `X` except its ends. No terrain constraint enters a cell's
   interior, and D-6 keeps structures and resource footprints off cut cells, so no other constraint meets the foot
   inside `X`. The module invariant "no two constraints cross" holds; `NavMeshError::ConstraintRecovery` still
   guards it.
5. `label_triangle`: after the structure and resource tests and before `Open`, a triangle whose centroid cell has a
   cut and whose centroid lies beyond the foot (exactly, on three times the centroid:
   `dx·sx + dy·sy > 3·LATTICE_PER_CELL·k`) is `Blocked`. A centroid never lies on a constraint, so the test is never
   on the line.
6. Unchanged and inherited: `width_at` and the TA* widths (diagonal walls are segments like any other), the radius
   funnel (its diagonal gate class already exists, `GateAxis::Diagonal`), `segment_clear`, `nearest_blocking_point`,
   `obstacle_vertices_near`, `locate` and `walk` (the cell-centre index is a hint; a cut cell's centre lies on the
   foot and the walk resolves either side), and the knowledge rules (a `Blocked` triangle is closed to every
   request).
7. **Lanes are unchanged by construction.** `LaneCuts::build` reads only `EdgeGrid::east`, `north` and
   `classify_cell(..).usable`; the foot's ends are already on the same wall component (both lie on the L-shaped
   wall `P₁ → C → P₂`), and `X` stays usable. T-L asserts equality.

So `Pathfinder::find_path*`, `segment_is_passable`, `wall_contact`, `wall_contact_exhaustive` and
`nearest_reachable_point` see the clip with no code of their own, and `sim_core`'s movement, contact and the Gated
contact layer, which reach terrain only through these, see it too (§8).

### D-4. The clearance field

`ClearanceWorkspace::mark_terrain_walls` marks, for every cut cell, the half-cell lattice point at its centre
`(2i + 1, 2j + 1)`. Every lattice point of the closed `T(X)` is then a wall: `C` and the two edge midpoints lie on
the cliff edges `X|L₁`, `X|L₂` (already marked), and `P₁`, the centre and `P₂` lie on the foot. The field keeps its
invariant (it may underestimate, never claim a disc that does not fit). Structures cannot stand on cut cells (D-6),
so `mark_structure_walls` is unchanged.

### D-5. The grid traversal predicate

**Lemma L-1.** Let `p` and `q` lie outside every `T(X)` (closed half of a cut cell, or any uncut point). Then the
segment `[p, q]` meets some `T(X)` only if it crosses a cell edge `X|L₁` or `X|L₂`. *Proof:* a segment enters the
open triangle `T(X)` through one of its sides. If it enters through the foot it lies, after the entry, in the open
half-plane beyond the foot, so it cannot leave through the foot again (a line crosses a line once); since `q` is not
in `T(X)` it leaves through `X|L₁` or `X|L₂`. If it enters through `X|L₁` or `X|L₂` it crosses that edge. ∎ Those
edges join levels `level(X) > lb ≥ level(L)` with no ramp in the block (D-1 condition 4), so
`cardinal_edge_is_connected` is false on them, and `ground_segment_is_connected` already refuses the segment.

So the predicate needs only an **endpoint test**: `ground_segment_is_connected` returns `false` when `start` or `end`
lies in some `T(X)` (the endpoint's cell has a cut and `DiagonalCut::beyond` holds). Interior points need no test by
L-1. This keeps `traversal.rs`'s invariant that navigation and physical movement share one walkable set. The test
costs the early-out of D-9 per endpoint on flat ground. `terrain_step_is_connected` is a cell predicate and is
unchanged.

### D-6. Placement

1. **Bodies** (`sim_core/src/movement/geometry.rs`, `ground_footprint_connected`): for each cell the square envelope
   touches, a cut cell also refuses the swept body when the segment `[start, end]` meets the open convex region
   `T(X) ⊕ (−r, r)²`. Since `T(X)`'s legs are axis-aligned, that region is exactly the open box `X ⊕ (−r, r)²`
   (the one `touches` already builds) intersected with the open half-plane `dx·x + dy·y > k − 2r`, the foot moved
   towards the plateau by the square's reach along the diagonal. The test is `segment_overlaps_open_box` with one
   more slab, the diagonal one, in the same exact rational form on raw `i128` coordinates; tangency is not overlap,
   as for the box. Blink, unload, spawns, production egress and effect placement
   reach terrain through this function, so they all inherit it. The function's `// PERFORMANCE RATIONALE:` block
   gains a line for the cut test.
2. **Structures and resource footprints** (`GameMap::can_place_building_where`, `place_one_resource_footprint`): a
   footprint that covers a cut cell is refused with the new `PlacementFailureReason::DiagonalCliff`. StarCraft II
   marks cliff edges unbuildable [COMM, S6]; here it also keeps the mesh free of crossings (D-3 item 4).
3. **Map validation** (`validation.rs`, `check_entity_placement` and `check_start_placement`): a resource node,
   watchtower or start footprint on a cut cell is an error of a new variant. WP0's census says whether a shipped map
   has one; if so, stop rule 5.
4. **Scenario order cells** (`scenario.rs`, `standable_cell_centre`): a cut cell's centre lies on the foot, so it is
   not standable for a body of any radius. It is refused with the existing `OrderCellNotStandable` when the cell has
   a cut. WP0's census lists any shipped scenario that names one (stop rule 5).

### D-7. The brink after the clip

A body of terrain radius `r` in a cut cell stands at least `r` behind the foot (D-3, the disc keeps `r` from blocking
edges). Under its centre the drawn surface is, by the drawing plan's profile (D-3 there: rows at `p + m·k`, depth
exactly `−k` behind the foot along the run): the rim chamfer for `0.22 ≤ r < 0.37`, the plateau for `r ≥ 0.37`.

| `r` | Archetypes | Depth behind the foot | Drop under the centre |
|---:|---:|---:|---:|
| 0.3125 | 3 | 0.3125 | 0.077 |
| 0.375 | 13 | 0.375 | 0 |
| ≥ 0.5 | 12 | ≥ 0.5 | 0 |

This is the axis-cliff figure, so the drawing plan's T-K limit of 0.2 now holds in cut cells too (T-K′, §6). At the
two kinds of run end (the 135° junction on a straight wall, the 225° junction at a concave corner) the drawing mitres
the two profiles with `m(n₁, n₂)`, so a point at depth at least `r` behind both lines has the same bound; T-K′
measures them and stop rule 6 enforces 0.2. **Verify** in WP0 on the diagonal fixture: the 225° junction is the
case where a body's centre can lie off both lines' normals (in the wedge round the junction vertex).

### D-8. Determinism

- The predicate reads `u8` levels and flags, compares integers, and runs in a fixed direction order.
- `beyond`, the mesh tests and the hexagon test are exact integer comparisons on Q32.32 raw values or lattice
  integers in `i128`, with the same overflow argument as `segment_overlaps_open_box` (raw values below `2⁶³`, products
  below `2¹²⁷`).
- No float, no hashed collection, no clock; `Vec`s in index order. The `sim_map` and `sim_core` crates build for
  `wasm32-unknown-unknown` as before (C2).

### D-9. Cost

- `GameMap::diagonal_cut` early-out: `X` must be higher than `L₁` and `L₂` for some direction, so the function first
  reads `X` and its four 4-neighbours' levels (five reads) and returns `None` unless some pair of perpendicular
  neighbours is lower. On flat ground that is five reads and no 3×3 block.
- `ground_segment_is_connected` (hot: body pairs in the solver, steering and locomotion): one early-out per endpoint,
  only when the endpoint's own cell is not on flat ground. It reads the cell it already reads first, so the flat-map
  cost is one comparison per endpoint after the first neighbour read. **Verify** by callgrind (§7.5); the fallback is
  stop rule 7's.
- `NavMesh::build`: one `diagonal_cut` per cell per rebuild (rebuilds follow structure changes), small against the
  triangulation. Recorded by T-P.
- No new allocation in steady state: `EdgeGrid` is built in the rebuild path that already allocates its grids; the
  per-tick predicates allocate nothing.

---

## 6. Tests

Every new test is written in WP0 and fails on `main` for the stated reason, unless it is marked as a guard.

**Support.** `crates/sim_map/tests/support/` gains `diagonal_fixture()`, a copy in the simulation's terms of the
client fixture's rows (runs with both end kinds, a one-step notch, ridges one and two cells wide, a three-cell band,
a saddle channel, a void run, a void-to-ground run, a stacked staircase, a run within one cell of a ramp, two runs
meeting at a wall point, a run crossing a chunk seam). It also gains `independent_cuts(map)`, a re-implementation of
the drawing plan's D-1 from the plan text, not calling `sim_map::diagonal` (it may share code with the client
support's `diagonal_runs` only by copying the rule, never by import, since `sim_map` cannot depend on `client`).

### 6.1 `sim_map`

- **T-D1 (unit, `diagonal.rs`): `the_cut_follows_its_block`.** The drawing plan's T-S2 table, moved with the
  predicate: a run cell, a run end, a rectangle corner, a void-only run, unbacked `O` (ridges), a ramp in each of
  nine positions, a saddle, and the cut run's levels. Plus `beyond` at the foot (false), one raw unit beyond (true)
  and one behind (false), in all four directions. Red on `main`: the module does not exist (WP0 writes it against a
  stub returning `None`).
- **T-D2 (integration, new `crates/sim_map/tests/diagonal_walkability_tests.rs`): `predicate_matches_the_plan`.**
  On the fixture, `showcase_canyon` and `default_arena`: `GameMap::diagonal_cut` equals `independent_cuts` cell by
  cell; `default_arena` has none. A guard.
- **T-W1: `no_disc_stands_beyond_the_foot_<map>`.** For each cut and each catalog ground terrain radius (read from
  the real catalog's distinct values through the runtime loader, charter §3.3, C6), samples centres on a 1/64
  lattice in `X` and asserts: `Pathfinder::wall_contact(map, request(r), p)` reports contact or
  `segment_is_passable(p, p)` is false for every centre closer than `r` to the foot on the high side or beyond it;
  and both report free for every centre at least `r + 1/64` behind the foot and `r` from `X`'s other walls. Red on
  `main`: the outer corner admits a body.
- **T-W2: `no_route_lost_across_diagonals_<map>`.** For every radius of T-W1 and every pair of walkable sample cells
  (cell centres, or the nearest legal point of a cut cell) on the fixture and `showcase_canyon`, whether a route
  exists is the same as on `main`. WP0 records `main`'s answers, from the same test in a record mode that is never
  the default, into `crates/sim_map/tests/fixtures/diagonal_reachability.ron`; the test compares against that file,
  so no production seam is needed. Expected: equal, because a diagonal band's narrowest pinch is between the
  concave corners on both sides, which the clip does not move (a three-cell band is `√2` wide before and after). A
  difference is stop rule 4. A guard that must stay green.
- **T-W3: `routes_and_contact_agree_on_the_clip`.** Every route of T-W2 passes `segment_is_passable` leg by leg, and
  `nearest_reachable_point` of a goal in `T(X)` lands behind the foot by at least `r`. Red on `main` for the second
  half.
- **T-M1: `the_foot_is_a_wall_and_beyond_is_blocked`.** On the fixture: every diagonal constraint run of kind `Wall`
  lies on a cut's foot and every foot is covered; every triangle with its centroid in some `T(X)` is `Blocked`;
  every other triangle carries the label `main`'s rule gives its centroid cell (a reference labelling written in
  the test from the pathfinding plan's D-2). Red on `main`.
- **T-L: `lane_cuts_ignore_diagonal_walls`.** `LaneCuts::build` on the fixture and `showcase_canyon` equals the
  recorded `main` value (cut list and `max_edge_crossings` on a map with no cut). A guard.
- **T-C1 (`clearance_field_tests.rs`): `cut_cells_never_overstate_clearance`.** For each cut cell, `clearance_at` is
  at most the largest disc (in eighths) inside its closed half behind the foot, by an exact enumeration on the
  1/64 lattice. Red on `main` where the corner quadrant admitted more.
- **T-T1 (`ramp_traversal_tests.rs` neighbour, new file section in `diagonal_walkability_tests.rs`):
  `segments_ending_beyond_the_foot_are_not_connected`.** Endpoints in `T(X)` refuse; segments between legal points
  agree with `main` (L-1) over a dense sample. Red on `main` for the first half.
- **T-P1: `buildings_and_nodes_refuse_cut_cells`.** `can_place_building` on a footprint covering a cut cell returns
  `DiagonalCliff`; resource placement and map validation refuse likewise. Red on `main`.
- **T-P (performance record, `#[ignore]`d measurement in an example, not a test):** `NavMesh::build` time of
  `showcase_canyon` before and after, recorded in the evidence (stop rule 7 at 1.2×).
- Existing files that must stay green unedited, except for pinned values on maps with cuts (re-recorded with a
  reason line, §7.1): `navmesh_tests`, `funnel_tests`, `wall_contact_tests`, `lane_tests`, `ramp_*`,
  `reachability_tests`, `pathfinding_properties`, `validation_tests`, `validation_golden_tests`, `map_gate_tests`,
  `wall_off_tests`, `resource_footprint_tests`, `determinism_tests`.

### 6.2 `sim_core`

- **T-G1 (`movement/geometry.rs` unit): `swept_bodies_refuse_the_outer_triangle`.** The hexagon test against a
  brute-force reference on a sample of segments and radii, in all four directions, including tangency (not overlap)
  on the foot. Red on `main`.
- **T-G2 (integration, new `crates/sim_core/tests/diagonal_cliff_movement_tests.rs`):
  `a_unit_ordered_to_a_step_corner_stops_behind_the_foot`.** On `showcase_canyon`, a ground unit of each distinct
  catalog radius (found by property, never by slug) ordered to a cut cell's outer corner ends at least `r` behind
  the foot (exact, `beyond` of the shifted line false) and never stands beyond it in any tick. Red on `main`.
- **T-G3: `crowds_pressed_on_a_diagonal_stay_behind_it`.** Twenty units of the smallest radius attack-move along a
  ground diagonal of the fixture map in a crowd; every tick, every body's centre is at least `r − ε` behind every
  foot, with `ε` the contact layer's documented penetration bound of the default pipeline (the same bound the
  existing wall tests use; **verify** its name in `wall_contact_tests.rs`). Red on `main`.
- **T-G4: `spawns_and_blinks_avoid_the_outer_triangle`.** Production egress, a death spawn and a Blink aimed at
  `C` place no body beyond the foot. Red on `main`.

### 6.3 `client`

- **T-K′ (`diagonal_cliff_tests.rs`, edited): `brink_drop_under_a_body_is_bounded_<map>`.** The sampling rule becomes
  the simulation's own: a centre is admitted when `sim_map`'s wall contact for that radius reports it free (the
  `Pathfinder` of the map). The test then asserts a drop of at most 0.2 in cut cells too, keeps 0.2 elsewhere, and
  keeps reporting the convex half-cut quadrants (B-250) as the drawing plan's Amendment 1 does. Red on `main` (3.0 in
  cut cells). The drawing plan's independent `diagonal_runs` stays the test's reference for where cuts are.
- **Mesher goldens (`mesher_golden_tests.rs`) unchanged** through WP1 and WP2: the proof that the predicate moved
  without change and that the simulation change reaches no drawing.
- **Review capture** (§7.6): the `brink_<r>` rows re-rendered with the unit at its deepest legal point.

---

## 7. Determinism, checksum, goldens, measured worlds and throughput

### 7.1 Checksum section and schema

- **New section `diagonal-cuts`**, after the map section and before the clearance field: the number of cuts, then
  per cut in row-major order its cell index (`u32`), direction (two `i8`) and `lb`, `t_hi` (`i8`). The cuts are a
  pure function of the hashed cells, as the clearance field is of cells and structures, and the checksum already
  hashes exposed navigation state; the section makes the walkable rule a hashed fact, so a replay recorded under the
  whole-cell rule fails at its first checkpoint with a schema mismatch instead of diverging mid-battle, and a
  predicate disagreement between two builds shows at tick 0.
- **Schema 65**, provisional, next free at merge. The `CHECKSUM_SCHEMA_VERSION` doc gains: "Schema 65 adds the
  map's diagonal cuts, a section `diagonal-cuts` (their count, then each cut's cell index, direction, foot level
  and highest threshold), after the map (plan `2026-12-02-diagonal-cliff-walkability` D-1, §7.1). The number is the
  next free at merge."

### 7.2 Goldens and the four-build proof

Both golden replays (`golden_replay.ron`, `golden_replay_orders.ron`) fight on `default_arena`, which has no cut
(§1.5), and `DEFAULT_SANDBOX_CHECKSUM`'s map is flat (**verify** in the census).

**Lemma L-2.** On a map with no cut, every function this plan changes returns what it returned on `main`: the edge
grid has no diagonal wall, no label changes, the clearance marks nothing new, the endpoint test and the placement
tests are false, and the section is the count 0. So such a world's state evolves byte-identically, and only the
checksum's bytes change (the schema number and the empty section).

The procedure of the vision plan's §5, as the death-spawns plan ran it:

1. **Latent proof.** With the section left out of the encoding and the schema at `main`'s number, both goldens and the
   sandbox reproduce every recorded checkpoint on native debug and release and on `wasm32-wasip1` debug and release
   (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`): **8 of 8**, plus the sandbox on four.
2. Then the section goes in, the schema steps to the next free number, and the checkpoints are re-recorded under
   "Re-recorded for plan 2026-12-02-diagonal-cliff-walkability WP2 (checksum schema M -> N): the checksum also
   encodes the map's diagonal cuts (empty on this map), so every checkpoint changed. No behaviour did: with the
   section left out at schema M, both fixtures reproduce as recorded for <the previous header>." The four builds
   reproduce the new values.
3. If `main` takes the number first, the merge repeats step 2 with the next one.

A golden that moves in step 1 is stop rule 2.

### 7.3 Matrix

`sc2_who_wins_matchup_matrix_tests` and `sc2_battle_benchmarks_tests` build flat 64×64 maps (`GameMap::new` with one
level); `sim_fixtures::matchup` adds rectangular level regions, which have no concave corner and so no cut
(**verify** in the census). By L-2 their outcomes are byte-identical; the matrix is run once in WP2 to show it.

### 7.4 Measured worlds and the harness census

**Census (WP0).** A scratch probe (`~/scratch/diagonal-walk/census/`, never committed) prints the cut count of every
map that a measured set builds, through the same constructors: the harness battles and held-out worlds
(`sim_fixtures::harness`, `mirror`), the traces (`trace::*`, including wall-clearance's crowd scenes if merged), the
sandbox, the matchup regions, the throughput rows, the golden maps, `sim_core` and `sim_map` tests that load
`showcase_canyon`, and the client scenarios (`showcase`, `showcase_effects`, `skirmish`, `default_battle`, all on
`showcase_canyon`). Worlds with 0 cuts are predicted byte-identical (L-2) and checked by digest in WP2.

**Worlds that may move** are those on `showcase_canyon` whose units reach a cut cell's half: the showcase seeds, the
effects showcase, the skirmish run, and the `showcase_canyon` throughput row. For each, WP2 records the first tick
whose world differs and the first body that stood in some `T(X)` on `main` at that tick (a trace probe reading
`beyond` per body). A move without such a body is unexplained and stops the package (stop rule 3). A battle whose
winner flips through this is restated under the owner's rule of 2026-11-24 (movement over matchup claims) and is not
a bug unless a unit's behaviour is broken (charter §8). No layout or rule is chosen to keep a winner (owner,
2026-11-20).

### 7.5 Throughput

Measured as the death-spawns plan's Amendment 1 did: `valgrind --tool=callgrind` instructions at 0 and at 300
measured ticks per row, the difference over 300, on `main` and on the branch in **one** session under one hold of
`~/scratch/gate.lock`, release benchmark binaries, one scenario each. Bars:

- **B1.** The 64×64, 400-unit row (54.44 M per tick at `08f73156`; re-measured on `main` in the session) rises by at
  most 0.5 %. It has no cut, so this measures only the endpoint test's early-out (D-9).
- **B2.** Every flat row rises by at most 0.5 %.
- **B3.** The `showcase_canyon` row rises by at most 2 %, and its instructions per tick, converted with the session's
  instructions per second of the 400-unit row, project at least 50 ticks per second (Q0093's bar).
- **B4.** Native ticks per second of every row stay at least 25 (CI's `--floor`), reported with the load average.

A bar missed is stop rule 7.

### 7.6 Review capture

Under `~/scratch/diagonal-walk/review/`, the drawing plan's layout (`contact_sheet.png`, `README.txt`):

- the `brink_0.3125` and `brink_0.5` rows: the same staging by property, the unit now at its deepest legal point
  in a cut cell (the corner direction, `r` behind the foot) and at the nearest legal point to the cell's centre;
  columns `main` (the drawing plan's capture, copied and named as a copy) and `wp2`;
- one overview of each `showcase_canyon` diagonal with a crowd pressed against it (T-G3's scene on the shipped map).

Every GPU command runs once per lock:
`WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock <command>`.

---

## 8. Coordination with the movement work

Two movement packages are in flight on 2026-11-27:

- **Wall clearance WP1** (branch `wall-clearance-wp1`, plan `2026-11-29-wall-clearance`): edits
  `movement/locomotion.rs`, `locomotion/follow.rs`, `sim_fixtures::trace` and the golden fixtures.
- **The contact layer's acceptance** (Q0143 (c), plan not yet written): ships the Gated contact layer C′
  (merged latent at `a5a3df79`) without WP2's gated intent, after a full acceptance run.

**Files.** This plan edits terrain and geometry queries only: `sim_map` (`diagonal.rs`, `navmesh/mod.rs`,
`pathfinding.rs`, `traversal.rs`, `grid.rs`, `validation.rs`), `sim_core/src/movement/geometry.rs`, `scenario.rs`,
`checksum.rs` and `checksum/encoding.rs`, and the client mesher's `diagonal.rs`. It edits no file under
`movement/` other than `geometry.rs`, no steering, solver, contact or locomotion file. Neither movement branch
touches those files (checked at `2516a068`); `checksum.rs` conflicts only in the schema line and doc, as every schema
package does.

**Interfaces.** Movement reaches the clip through `Pathfinder::{segment_is_passable, wall_contact,
nearest_reachable_point, find_path*}` and `GameMap::ground_segment_is_connected`, whose signatures do not change. A
diagonal wall gives wall contact a diagonal normal, which chamfered footprints already give, so the contact
solver's and C′'s terrain transfer (B-220) need no change. **Verify** in WP2 that C′'s terrain rule has no axis
assumption (its tests on chamfered footprints, if any, or T-G3 under the Gated pipeline value).

**Order.**

1. **WP0 and WP1 may land at any time.** WP1 changes no behaviour (mesher goldens unchanged, no simulation file's
   behaviour changes, no schema).
2. **WP2 lands after wall-clearance WP1 and after the contact-layer acceptance package** merge to `main`, one at a
   time, and re-bases on them. Reason: both measure showcase worlds and re-record goldens. Landing WP2 first would
   move their showcase baselines mid-acceptance; landing it after makes WP2's own census and measured-world
   comparison run against the movement that ships. Their measured worlds without cuts are unaffected either way
   (L-2).
3. **If either package stalls for more than a week**, WP2 may land first. The census then hands that package the list
   of its measured worlds with cuts, which must be re-baselined on the merged tree before its acceptance runs.
   The orchestrator decides.
4. WP3 follows WP2.

---

## 9. Acceptance

1. T-D1, T-D2, T-W1, T-W3, T-M1, T-C1, T-T1, T-P1 (`sim_map`), T-G1 to T-G4 (`sim_core`) and T-K′ (`client`) green;
   guards T-W2 and T-L green.
2. Every existing test green; pinned values move only on maps with cuts, each with a reason line naming this plan.
3. Mesher golden hashes, pick hashes and `terrain_tests` counts unchanged through WP1 and WP2.
4. The latent proof 8 of 8 (plus the sandbox), the re-record with its header, and the four builds reproduce it.
5. The census lists every measured world; those with 0 cuts byte-identical; those with cuts explained per §7.4.
6. Throughput bars B1 to B4 hold, figures in the evidence file.
7. `cargo gate package` green on the final tree, run by the verifier agent (charter §4).
8. Records: `DESIGN.md` §4 (B-251 fixed; B-246's status points here; B-250 unchanged unless Q0146 says otherwise),
   §5 if a throughput figure is restated, the evidence file
   `docs/evidence/2026-12-02-diagonal-cliff-walkability.md`, the drawing plan's Amendment 2 (T-K's cut cells now
   bounded; the predicate lives in `sim_map`), this plan's `## As built`, the README row.

---

## 10. Work packages

One package per branch in its own worktree, `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`, large
outputs under `~/scratch/diagonal-walk/`. Never run a workspace test outside `cargo gate`; the verifier runs
`cargo gate package` on each package's final tree.

**WP0: census and red tests** (branch `diagonal-walk-wp0`; may merge with WP1).
1. The census probe (§7.4), output in evidence §1.
2. `sim_map::diagonal` with the stub `of_block` returning `None` (documented as the WP0 stub, no TODO marker,
   charter §1.1) and `beyond`; the fixture and `independent_cuts` in `sim_map` test support.
3. T-D1, T-D2, T-W1 to T-W3, T-M1, T-L, T-C1, T-T1, T-P1, T-G1 to T-G4, T-K′; new `suites.ron` entries:
   `sim_map/diagonal_walkability_tests` and `sim_core/diagonal_cliff_movement_tests`, CPU class, triggered by
   `crates/sim_map/src/**` and `crates/sim_core/src/movement/**` (**verify** the rule form against neighbouring
   entries). Run each new target alone; record red outputs in evidence §2. Guards pass.

**WP1: the predicate moves to `sim_map`** (no behaviour change).
1. `DiagonalCut::of_block`, `marching_level`, `GameMap::diagonal_cut` with the early-out (D-9) and its
   `// PERFORMANCE RATIONALE:` block; module `//!` docs (responsibility, how it fits, invariants: pure function of
   nine cells, at most one cut per cell, no ramp in the block).
2. The client's `Cut` built from `DiagonalCut`; `cut_level` removed from the client; its module docs point to
   `sim_map::diagonal`.
3. Green: T-D1, T-D2; mesher golden hashes, `terrain_tests`, `diagonal_cliff_tests`, `cliff_boundary_tests`
   unchanged. No simulation file changes behaviour; no checksum changes (golden replays run unchanged).

**WP2: the clip** (after §8's order).
1. D-3 (mesh), D-4 (clearance), D-5 (endpoint test), D-6 (placement, validation, scenario order cells), D-8; split
   helpers so every function stays within 100 counted lines; update module docs of `navmesh/mod.rs` (Invariants:
   constraints now include each cut's foot; triangles beyond it are `Blocked`), `traversal.rs`, `pathfinding.rs`'s
   clearance model and `geometry.rs`.
2. Green: everything of §9.1. Matrix run once (§7.3).
3. Checksum section and schema (§7.1); the latent proof, re-record and four builds (§7.2), in their own commit.
4. Census comparison of measured worlds (§7.4) and throughput (§7.5); evidence §3 to §5.

**WP3: client review and records.**
1. Review capture §7.6; T-K′ figures in the evidence.
2. Records of §9.8.

---

## 11. Stop rules

Stop and report to the orchestrator, with the evidence, when:

1. The mesher golden hashes, pick hashes or `terrain_tests` counts change in WP1 or WP2.
2. A golden or the sandbox moves in the latent step (§7.2), or a world with 0 cuts is not byte-identical.
3. A measured world moves with no body in an outer triangle on `main` before its first difference (§7.4).
4. T-W2 finds a route lost or gained for some radius.
5. A shipped map, scenario or start loadout places a node, watchtower, structure or order cell on a cut cell, or
   map validation of a shipped map fails.
6. T-K′ finds a drop above 0.2 in a cut cell (a run end included), or T-W1 needs a tolerance.
7. A throughput bar of §7.5 fails, or `NavMesh::build` on `showcase_canyon` is more than 1.2× slower. The fallback
   for B1 and B2 (the endpoint test on the hot path) is to move the endpoint test from
   `ground_segment_is_connected` into the callers whose endpoints are not body positions, listed by audit, with L-1
   as the argument for the rest; that is an amendment, not an agent's choice.
8. The mesh fails `ConstraintRecovery` on any map, or a diagonal wall crosses another constraint.
9. A movement file beyond `geometry.rs` would need an edit.
10. A GPU child cannot get the physical GPU or a real shader fails to load (charter §1.2).

---

## 12. Files

| File | Change | WP |
|---|---|---|
| `crates/sim_map/src/diagonal.rs` (new), `lib.rs` | `DiagonalCut`, `of_block`, `marching_level`, `GameMap::diagonal_cut`, `beyond`; T-D1 | 0, 1 |
| `crates/sim_map/src/navmesh/mod.rs` | cut table in `EdgeGrid`, diagonal `Wall`s, `Blocked` labels, contact-free cells, docs | 2 |
| `crates/sim_map/src/pathfinding.rs` | clearance marks the cut cell's centre; docs | 2 |
| `crates/sim_map/src/traversal.rs` | endpoint test in `ground_segment_is_connected`; docs | 2 |
| `crates/sim_map/src/grid.rs`, `error.rs` | `PlacementFailureReason::DiagonalCliff`; building and resource placement | 2 |
| `crates/sim_map/src/validation.rs` | node, watchtower and start footprints off cut cells | 2 |
| `crates/sim_map/tests/diagonal_walkability_tests.rs` (new), `support/` | §6.1 | 0 |
| `crates/sim_map/tests/clearance_field_tests.rs` | T-C1 | 0 |
| `crates/sim_core/src/movement/geometry.rs` | the hexagon test; T-G1 | 0, 2 |
| `crates/sim_core/src/scenario.rs` | order cells on cut cells refused | 2 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | section `diagonal-cuts`, schema | 2 |
| `crates/sim_core/tests/diagonal_cliff_movement_tests.rs` (new) | T-G2 to T-G4 | 0 |
| `crates/sim_core/tests/fixtures/golden_replay*.ron`, sandbox constant | re-record (§7.2) | 2 |
| `crates/client/src/terrain/mesher/diagonal.rs` | `Cut` from `DiagonalCut`; `cut_level` removed | 1 |
| `crates/client/tests/diagonal_cliff_tests.rs` | T-K′ | 0 |
| `crates/gate/suites.ron` | two new targets | 0 |
| `DESIGN.md`, `docs/evidence/2026-12-02-diagonal-cliff-walkability.md`, drawing plan, this plan, `docs/plans/README.md` | records | 0, 3 |

---

## 13. Out of scope and recorded decisions

- **The outer triangle walkable by the low side** (§3 option C). Rejected for now: it splits a cell's level for every
  per-cell rule. A low body stands 0.707 from the drawn foot at a step's outer corner (D-2), recorded as a minor
  deviation of detail. It can come back as its own plan.
- **Convex half-cut corners (B-250), ground and void.** Not clipped here; draft Q0146 asks the owner. The mechanism
  generalises: the half cut's foot through the edge midpoints is a diagonal of the quarter lattice, so a second cut
  kind in `sim_map::diagonal` would clip it the same way. It would move `default_arena` (20 rectangle corners) and
  so the goldens' behaviour, which is why it is a question and not a scoping decision.
- **Picking** stays on the grid (drawing plan §11). On the low half of a cut cell a click now lands on terrain no unit
  can reach; the order's goal is clamped by `nearest_reachable_point`, as for any wall.
- **Cliff movers.** No ground mover crosses cliffs today (`MoverType::CliffWalker` moves as ground). A future cliff
  mover would treat the foot as it treats a cliff edge, since both are mesh `Wall`s with level tags on either side.
- **Diagonals at other slopes** keep whole-cell walkability, as their drawing keeps `main`'s geometry (drawing plan
  §11).

---

## 14. Bug rows

Registered in `DESIGN.md` §4 by this commit (owner: "track the current overhang as an open bug until it lands"):

```
| B-251 | The simulation's walkable high ground of a 45° staircase step cell reaches the cell's outer corner, half a cell beyond the straight diagonal cliff drawn since plan `2026-12-01-straight-diagonal-cliffs` (B-246, Q0136 (c)): a ground body at the step's outer corner or centre stands over the drawn low floor, 3.0 below its plateau for terrain radius up to 0.5 (25 of 28 ground archetypes; that plan's D-8 and T-K). The owner answered Q0139 with a fourth option: keep the straight look and clip the walkable half cell along the same line, as StarCraft II shows straight diagonal cliffs with neither floating high-ground units nor low-ground rock clipping. On `showcase_canyon`, 24 cells (number provisional, next free at merge) | open; planned by [`2026-12-02-diagonal-cliff-walkability`](docs/plans/2026-12-02-diagonal-cliff-walkability.md): the cut predicate moves to `sim_map`, the navigation mesh walls each cut's foot and blocks the triangle beyond it, and clearance, traversal and placement follow |
```

B-246's status column ends "a unit at a step's outer corner or centre is drawn over the low floor (owner question
Q0139, default keep)"; it now ends "…over the low floor: B-251 (Q0139 answered: clip the simulation to the drawn
line)". B-250 is unchanged, with a pointer to Q0146.

---

## 15. Owner question (filed as [Q0146](../questions/open/0146-convex-cliff-corner-walkability.md))

```
# Q0146: Should units also keep off the cut corners of square cliffs (B-250)?

- **Asked:** 2026-12-02
- **Blocking:** no
- **Default until answered:** (a) only diagonals are clipped; B-250 stays open

## Question

Following your answer to Q0139, plan 2026-12-02-diagonal-cliff-walkability clips the walkable ground of 45°
diagonal cliffs to the drawn line, so no unit floats there. The same kind of overhang exists, smaller, at every
outer corner of a square plateau (B-250): the drawing trims each such corner with a short 45° cut (one eighth of
the cell), and a small unit standing right in the corner is drawn hanging 2.84 world units over the cut (5.84 on a
two-level corner). Both maps have these corners (20 on default_arena).

## Options

- **(a) Leave square corners as they are (default).** Only diagonals are clipped. B-250 stays an open presentation
  bug.
- **(b) Clip the corners in the simulation too**, the same mechanism: units keep their radius from the short 45°
  line, so they stop about 0.09 cells earlier in a corner. This changes movement near every plateau corner, so
  battles on default_arena (the golden replays) move; it would get its own package after the diagonal one.
- **(c) Draw square corners sharp instead** (no 45° trim on convex corners). No simulation change; plateaus get hard
  right-angled corners.

Recommendation: (b), since it is the rule you chose for diagonals applied to the remaining case, after the diagonal
package has landed; (c) if you prefer to keep movement untouched.

## Answer
```
