# Blueprint: Cliff Feet on the Simulation's Boundary, and Air Indicators that Follow the Ground (B-243, B-238)

This plan fixes two client presentation rows of `DESIGN.md` §4 and two defects found while diagnosing them:

- **B-243** (filed on branch `wall-clearance-plan`, `1c146c83`, not yet on `main`): "The cliff mesh's wall-row jitter
  (`WALL_JITTER_AMPLITUDE`, up to 0.06, `crates/client/src/terrain/mesher.rs` `generate_wall_profile`) moves canonical
  wall rows towards the low, walkable side of their cliff edge, so a body the simulation holds tangent to the wall is
  drawn dipping into the cliff by up to 0.06." Measured by the wall-clearance WP0 mesher check
  (`~/scratch/wall-clearance/mesher_check/`): 5,501 of 11,866 canonical rows on `showcase_canyon` move towards the low
  side, the largest by 0.0600.
- **B-238** (on `main`): "The air shadow and tether keep their spawn height: `attach_bubble_indicators`
  (`indicators.rs`) places them from the record's altitude at spawn (now `IndicatorPose::of`, at arrival) and nothing
  moves them afterwards."
- **B-244 (new, provisional):** the air shadow and tether stop one bubble radius *above* the ground, never on it (§2.2).
- **B-245 (new, provisional):** the dual grid's 45° cut at a concave cliff corner draws the high plateau inside the low,
  walkable cell, up to 0.354 deep (§1.4). Its fix changes the look of staircase cliffs, so it waits on draft question
  **Q0136** (§13); the plan builds the default meanwhile.

The design in one paragraph. **Terrain:** the simulation's boundary between two plateaus is the cell edge, and both
sides are walkable. The mesher draws every cliff profile at that edge with a foot that spreads 0.22 *onto the low
floor* and canonical rows jittered ±0.06 *across* it. The fix keeps every profile's shape and translates it by
`FOOT_WIDTH` into the high cell wherever the edge's low side is ground, so the foot's toe lies exactly on the edge and
the jitter (0.06 ≤ 0.22) stays behind it. Concave corners (a lone low cell corner) get a square, mitred corner
instead of the 45° cut, and saddles resolve low-connected. The rule that results, **W1**, says that no terrain
geometry rises above a walkable cell's surface inside its footprint. A CPU test checks W1 triangle by triangle with
a tolerance derived from `f32` rounding, and a GPU test checks it on the real shaders, where a body drawn tangent to a
wall must not be cut by the cliff. **Air indicators:** the shadow and tether are placed by one pure function from
the record's ground height every frame, inside `update_indicators_system`. The shadow lies on the ground, and the
writes compare first, so nothing allocates and a steady frame changes nothing.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `e5ebc043`, with branch
`wall-clearance-plan` (`4be9009f`; B-243's row and its evidence `docs/evidence/2026-11-29-wall-clearance.md`) and the
merged plan [`2026-11-29-render-entity-pool`](2026-11-29-render-entity-pool.md) (`IndicatorPose`, D-6) read for
coordination.

- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-244**, and the draft question is
  **Q0136** (§13, not filed by this commit).
- **No simulation change.** No file under `crates/sim_*` changes, and no world, golden replay, checksum or throughput
  figure moves. A change there is a stop (§9).

---

## Contents

1. Diagnosis: the cliff profile (B-243, B-245)
2. Diagnosis: the air indicators (B-238, B-244)
3. Requirements and charter mapping
4. Design decisions
5. Tests
6. Goldens and the owner's review capture
7. Acceptance
8. Work packages
9. Stop rules
10. Files
11. Out of scope and recorded decisions
12. Bug rows
13. Question draft

---

## 1. Diagnosis: the cliff profile (B-243, B-245)

### 1.1 Where the boundary is

The simulation joins two ground cells only through a cardinal edge whose two sides are connected
(`GameMap::terrain_step_is_connected`, `crates/sim_map/src/traversal.rs`). A diagonal step needs both cardinal routes.
Between two plateaus of different level, with no ramp between them, the **unpathable boundary is the cell edge**. Both
sides are walkable, each at its own height, and a body on either side may stand tangent to the edge: the wall-grazing
evidence (`docs/evidence/2026-11-29-wall-grazing.md` case B) measures bodies riding walls at clearance 0.

The client draws cliffs with dual-grid marching squares (`mesher.rs`, plan `21b-plan-map-editor` §5.3). Dual cell
`(di, dj)` is centred on the shared corner of four cells, and its corners are their centres. Contour crossings are the
**edge midpoints**, `p`, which lie on the cell edges. So far the drawn cliff and the simulation agree. Three features
of the profile and one of the marching table depart from it.

### 1.2 The wall profile (`generate_wall_profile`)

At a contour midpoint `p`, `d = unit(B − A)` points from the high corner `A` to the low corner `B`, that is towards
the low side. The rows, from top to bottom:

| Row | Position | Side of the edge |
|---|---|---|
| rim top (if `level_a == tb`) | `p − RIM_WIDTH·d`, `y = H(tb)` | high cell, at its floor height |
| rim bottom | `p`, `H(tb) − RIM_DROP` | on the edge |
| canonical rows, every `k·WALL_ROW_SPACING` | `p + d·A(2h − 1)`, `h = hash(p, y) ∈ [0, 1]` | **either side, by up to `A = 0.06`** |
| foot top (if `level_b == ta − 1 ≥ 0`) | `p`, `H(ta−1) + FOOT_RISE` | on the edge |
| foot bottom (the toe) | `p + FOOT_WIDTH·d`, `H(ta−1)` | **low cell, 0.22 inside, at its floor** |
| wall bottom (no foot) | `p`, `H(ta−1)` | on the edge |

Every row of a profile at midpoint `p` is placed relative to `p` alone, and `d` is axis-aligned. No smoothing,
relaxation or normal offset moves vertices after this, and the crossings are never interpolated
(`dual_cell_corners`: `p_b`, `p_r`, `p_t`, `p_l` are exact half-integers). The candidate causes the task names are
therefore settled from the code:

- **Smoothing or relaxation:** none. `emit_wall_strip` samples both profiles at the union of their row heights by
  linear interpolation along each profile (`sample_profile_at_height`). That moves no row.
- **Normal offset:** none. Normals are computed per row and never added to positions.
- **Quantisation:** the hash key is `round(64·coord)`, which picks the jitter but moves no vertex.
- **Marching interpolation:** none on straight edges, because crossings are fixed at midpoints. The 45° cut of §1.4
  is the table's, not an interpolation.
- **The jitter is the cause B-243 names.** `disp = WALL_JITTER_AMPLITUDE · (2h − 1)` is symmetric about the edge, so
  about half the canonical rows move towards the low side. This matches the WP0 check: 5,501 of 11,866, largest
  0.0600, largest towards the high side 0.0599.
- **The foot is a deliberate bevel, and it lies on the walkable side.** Plan 21b §5.3 specifies it ("foot bottom:
  `p + FOOT_WIDTH·d`"), so it is not a coding slip. Its toe is 0.22 inside the low cell at floor height, and its slope
  rises to the edge at 0.28. The WP0 check counted only rows with a horizontal normal, so it did not see the foot. The
  wall-grazing evidence did: "a sphere of radius 0.375 tangent to the wall line meets the slope by 0.021". The foot
  is the larger part of B-243: it is present on every cliff whose low side is ground, and it reaches 0.22 where the
  jitter reaches 0.06.
- **The rim is on the safe side.** It removes solid from the high cell, below the high floor. A body on the high side
  can hang over it, but nothing is drawn *inside* that body.

### 1.3 Stacked runs share the edge

One edge can carry several threshold runs, for example corners at levels 2 and 0 when the dual cell's case changes at
t = 2. Their profiles stack at the same `p`: the lower run has the foot (`lb = ta − 1`), and the upper run has the rim
and no foot. The upper run's canonical rows lie above the **level-0** floor beside the edge, so its jitter also
reaches into the walkable low cell. Any fix must therefore be a property of the **edge** (its low corner is ground),
not of the run, or the stacked walls would no longer meet.

### 1.4 The 45° cut at a concave corner (B-245)

Plan 21b §5.3 writes that "outer/inner corners become 45° cuts of half a cell (area 1/8 cell, documented as the only
visual/sim deviation)". The two kinds of corner fall on opposite sides of the boundary:

- **Lone high corner** (cases 1, 2, 4, 8; a convex corner of the high plateau). The cut removes a triangle of the
  *high* cell, below its floor. That is safe by the same argument as the rim.
- **Lone low corner** (cases 7, 11, 13, 14; a concave corner of the high plateau, a nook in the low ground). The
  contour from one midpoint to the other runs through the **low** cell's corner quadrant. The high plateau polygon and
  its wall cover the triangle `(0,0), (0,−½), (−½,0)` of the walkable low cell, up to `½/√2 = 0.354` from the cell's
  corner. A body of radius 0.375 pressed into the nook (centre at `(r, r)` from the corner) is cut by
  `r − |2r − ½|/√2 = 0.198`.
- **Saddles 5 and 10** resolve high-connected, so the hexagon covers corner triangles of *both* low cells.

A staircase cliff alternates lone-high and lone-low dual cells, so the 45° line drawn through the midpoints lies half
in each. That is the "straight 45° cliff — the SC2 look" of plan 21b, and on `showcase_canyon` it is the long diagonal
canyon and the diagonal level-1/2 boundary (`levels` rows 0 to 25). StarCraft II paths along such diagonals. Our
simulation steps along the cells, so a body hugging the canyon wall from the low side presses into every nook.

WP0 measures the counts (lone-low dual cells, saddles, per map). They are not measured yet.

### 1.5 What is not wrong

- **Ramps.** Plan `2026-11-09-ramp-geometry` R1 to R6 put the ramp quad at the simulation's height and every ramp step
  wall on the ramp's edge. Contour midpoints on a ramp edge carry no rim, no foot and no jitter (R6), so their rows sit
  on the edge. The **guard rail** rides `RAIL_WIDTH` = 0.08 inside the ramp's footprint, up to `RAIL_HEIGHT` = 0.30
  above it. The ramp plan's T2 exempts that envelope by design; this plan keeps the exemption (D-7).
- **The picker** (`picking.rs`) raycasts the cell grid, not the mesh, so its goldens do not move.
- **The simulation and its data.** Nothing here is a simulation defect.

---

## 2. Diagnosis: the air indicators (B-238, B-244)

### 2.1 Where the pose is computed

`IndicatorPose::of(instance, style)` (`crates/client/src/indicators.rs`) is the one source of every indicator child's
values, for a fresh spawn (`attach_bubble_indicators`) and for a reused render entity (`IndicatorWriter::restyle`,
called by `reuse_live_instance` in `scene_controller.rs`). Pool plan D-5 and D-6 made it so. It computes:

```
height  = world_pos[2] − ground_z           (then max(AIRBORNE_HEIGHT))
inv_r   = 1 / max(radius, MIN_INDICATOR_RADIUS)
shadow  = Transform(0, −height·inv_r, 0), scale (inv_r, 1, inv_r)
tether  = Transform(0, −height·inv_r/2, 0), scale (inv_r, height·inv_r, inv_r)
air     = Inherited if height > AIRBORNE_HEIGHT else Hidden
```

These are local transforms under the bubble entity, whose transform the live synchroniser writes every frame
(`update_live_instance`): translation `(x, world_pos[2] + r_off, y)` with `r_off = radius` for a sphere (0 for a dome),
scale `radius`, rotation identity (**verify** that nothing rotates a bubble entity). `update_indicators_system` runs
every frame over every non-parked bubble. It writes the reticle's uniform and visibility, the bar's visibility and
scales, and the tether's **colour**, but never the shadow's or the tether's transform, and never their visibility.

### 2.2 What that does

- **B-238 as filed.** The shadow and tether keep the transform of the record that arrived. For a record whose height
  above its own ground changes after arrival, both stay at the old height, and a unit that becomes airborne or lands
  keeps its arrival visibility. Today's simulation gives every live record a constant height: `Placed::new`
  (`crates/sim_core/src/world/snapshot.rs`) sets `world_pos[2] = ground_z + AIR_UNIT_ALTITUDE` (2.5) for an air
  archetype and `ground_z` otherwise. A mode's `mover_type_override` does not reach it. Over a cliff, both `ground_z`
  and `world_pos[2]` jump by the cliff's height, and the bubble carries its children along. So the "spawn height" half
  of B-238 is **latent** for every shipped unit. It bites the first record whose altitude above ground varies, and
  the task asks for the shadow to follow altitude every frame regardless.
- **B-244 (new): the shadow floats.** The child's world height is `centre_y + radius · local_y`. For `r ≥ 0.4` that is
  `(ground_z + h + r) − h = ground_z + r`: the "ground" shadow is drawn **one radius above the ground**, and the tether
  ends there. For `r < MIN_INDICATOR_RADIUS` the offset mixes the size clamp into the position:
  `ground_z + h + r − h·r/0.4`, which is `ground_z + 2.5 − 5.25·r` at `h = 2.5`, for example 1.19 above the ground at
  `r = 0.25`. The reticle gets this right for ground units (`ground_y_offset = −0.98` in radius units puts it at the
  bubble's bottom). The shadow subtracts only the height and forgets the radius. Air drawn radii in the catalog range
  up to 1.25 (the Carrier analogue), so the offset is up to 1.25 world units, plainly visible from the RTS camera's
  58° pitch. This is the "wrong height" the B-238 row reports. WP0 lists the offset per air archetype from the
  catalog.

---

## 3. Requirements and charter mapping

| Requirement | Where met |
|---|---|
| Diagnose each bug from code | §1, §2 |
| The drawn cliff foot coincides with the simulation's boundary within a stated tolerance | D-1 (toe at `p`, exact), W1 (§4), T-A (tolerance 0), T-B (derived τ) |
| Keep the art's look; any bevel stays outside the walkable area | D-1 (profile translated, shape kept), D-3 (convex cuts kept), D-4 (trade-off measured), review capture §6 |
| Air indicators follow altitude every frame without allocating | D-5, D-6; U-1, U-2, I-1; steady-state and valgrind tests unchanged and green |
| Red first | every new test lands in WP0 and fails on `main` (§5, §8) |
| CPU wall-row test with derived tolerance | T-A, T-B |
| Mesher golden updates with a stated reason | §6.1 |
| GPU pixel test, real shaders, cliff foot against the drawn unit footprint, locked through `gpu_run.rs` | G-1 |
| Visual goldens re-baselined only with a review capture | §6 |
| No simulation change | §9 stop rule 1 |
| Charter §1.2 (real shaders, real assets), §3.2 (client draws from `RenderSnapshot`), §3.3 (no steady-state allocation) | G-1 boots the production client, D-6, §5.3 |
| Charter §2 (short functions, docs) | D-6 helper split; module docs gain W1 (D-2) |
| Charter §7 (GPU lock) | G-1 and review captures through `gpu_run.rs` and `with-gpu-lock` |

---

## 4. Design decisions

### D-1. The profile is set back by `FOOT_WIDTH` on every edge whose low side is ground (B-243)

`generate_wall_profile` gains an edge **setback**:

```
setback = FOOT_WIDTH   if level_b >= 0 and !suppressed     (the edge's low corner is ground)
        = 0            otherwise                            (void below, or a ramp edge: R6)
```

Every row's `xz` is translated by `−setback·d`, with one exception: when a foot is present, its bottom row (the toe)
is assigned `p` directly, not computed as `p − setback·d + FOOT_WIDTH·d`, so the toe is the edge **bit for bit**. The
profile's shape is kept: rim, canonical rows with today's hash and amplitude, foot top and toe are today's profile
moved `FOOT_WIDTH` into the high cell. Because the jitter amplitude is at most the setback (0.06 ≤ 0.22; T-A2 pins
it), no canonical row reaches the edge. The setback depends only on the edge's two corner levels and its ramp flag,
which both dual cells sharing the edge compute identically, so chunks and stacked runs stay watertight (§1.3).

The floor polygons follow the same rule: a region-polygon vertex that is a contour midpoint is inset by
`setback + RIM_WIDTH` when the rim is present (today `RIM_WIDTH`), and by `setback` otherwise (today 0), in
`emit_dual_cell`'s `region_polygon` closure. The low floor polygon still ends at `p`, where the toe now is.

`centre_profile` (the half-wall to a dual cell's centre beside a ramp, R6) keeps its rows at the centre. The half-wall
from a set-back midpoint to the centre then slants, but it stays behind the edge, because both ends are on the edge
or behind it.

*Rejected alternatives:*
- **One-sided jitter only** (`disp ∈ [−0.12, 0]`). This fixes the 0.06 but leaves the 0.22 foot, the larger part.
- **A battered face** (toe at `p`, face leaning out to `p` at the rim). This keeps the high lip at 0.15, but the cliff
  overhangs its own foot, which changes the look more than a translation does.
- **Dropping the foot.** This changes the look.

### D-2. Invariant W1, stated in the mesher's module documentation

> **W1 (walkable columns).** Inside the footprint of a walkable cell (a non-abyss plateau or a ramp), no terrain
> triangle rises above the cell's simulation surface, except a ramp's guard rail within `RAIL_WIDTH` of its side edge
> and `RAIL_HEIGHT` above the surface (D-7). Rims and convex 45° cuts remove solid below a plateau's floor and are
> allowed.

W1 is what "the drawn cliff foot coincides with the boundary" means for every feature at once. T-B checks it.

### D-3. Concave corners square and mitred; saddles low-connected (B-245; Q0136 default (a))

The marching table changes in two ways:

- **Lone low corner** (cases 7, 11, 13, 14). The contour runs from one midpoint to the dual cell's **centre** (the
  cell corner) and on to the other midpoint: two half-walls meeting at a right angle. The centre's profile is a
  **mitred corner**: each row's offset is applied along both edges' directions (`centre − off·d₁ − off·d₂`, with
  `off` the row's setback, rim inset or jitter), so both half-walls stay behind their edges. The corner's jitter is
  keyed by the centre point. No other dual cell uses that point, so no seam is shared. The high region becomes an L
  of three quadrants, which is not convex, so `MarchingCase::region` becomes a short list of convex parts
  (`regions: &[&[DualVertexRef]]`; two parts for an L) and `DualVertexRef` gains a `Centre` variant.
  `FloorPolygon`'s convex clipping is unchanged, because it clips each part.
- **Saddles 5 and 10** resolve **low-connected**: two lone-high cuts (the cases 1 + 4 and 2 + 8 contours). Each removes
  solid from its own high cell, and neither enters a low cell. The drawn plateaus meet only at the corner point, as
  in the simulation. The marching table's comment on saddles and the `DiagonalPinch` note of plan 21b §5.3 are
  updated.

Lone high corners (cases 1, 2, 4, 8) **keep** their 45° cut: it removes high solid only. A staircase cliff then draws
as 45° chamfered teeth with square nooks. This is the visible change the owner decides in Q0136. Option (b) keeps
the 45° nook as a sanctioned deviation, with an explicit exemption in T-B2; option (c) is a non-local
"high-biased" staircase that keeps the straight look by drawing the diagonal through the low cells' convex corners
(a later plan). WP3 builds (a) and is the last package, so the owner can drop it (§8).

### D-4. The high lip recedes by `FOOT_WIDTH`: measured, not hidden

D-1 moves the rim top from 0.15 to 0.37 behind the edge. A body on the **high** side, tangent to the edge, has its
centre `r` behind it. Its contact point (directly below the centre) lies on the drawn floor when `r ≥ 0.37`, and over
the rim chamfer otherwise. In the catalog only collision radius 0.25 is below 0.37 (one archetype; **verify** its
drawn radius). At the brink it would rest about `0.2·(0.37 − 0.25)/0.15 = 0.16` above the chamfer, an overhang and
not an intersection. WP0 measures this per ground archetype from the catalog. The review capture shows the case
(§6.3), and the evidence records the figure. It is the cost of D-1: on a cliff between two walkable plateaus, any
bevel lies on one side or the other, and W1 puts it where nothing is drawn inside a body.

### D-5. One pure function places the air shadow and tether, on the ground (B-244)

`IndicatorPose` takes its air fields from a new pure `AirPose::of(instance) -> AirPose { shadow, tether, air }`.
`IndicatorPose::of` calls it, so pool plan D-6's "one source" holds. With `r = instance.radius`,
`r_off = r` for a sphere and 0 for a dome, `h = world_pos[2] − ground_z`, and the bubble's parent transform as in §2.1:

```
span       = max(h + r_off − SHADOW_LIFT, SHADOW_LIFT)        world units, bubble centre down to the shadow
shadow     = Transform(0, −span / r, 0), scale (inv_r, 1, inv_r)          (sizes as today)
tether     = Transform(0, −span / (2r), 0), scale (inv_r, span / r, inv_r) (top at the bubble centre, as today)
air        = Inherited if h > AIRBORNE_HEIGHT else Hidden
```

The position uses the exact `1/r` of the parent's scale. The *sizes* keep today's `inv_r = 1/max(r,
MIN_INDICATOR_RADIUS)`, so the shadow disc and tether are as large as today. `SHADOW_LIFT` is a new named constant,
0.02 world units: the shadow disc (`Cylinder::new(0.9, 0.02)`, y-scale 1 in radius units) is `0.02·r` thick and
centred, so its underside is `SHADOW_LIFT − 0.01·r` above the ground, which is positive for every `r < 2` (largest
drawn radius 1.25). The `max` keeps a ground bubble's hidden tether from a zero scale, which is the role
`height.max(AIRBORNE_HEIGHT)` plays today. The result: the shadow's world height is `ground_z + SHADOW_LIFT` for every
radius, and the tether runs from the bubble's centre to it.

### D-6. The shadow and tether are written every frame, in `update_indicators_system`, compare-first (B-238)

`update_indicators_system` already visits every non-parked bubble's children every frame. It gains one query over the
shadow and tether, `(&mut Transform, &mut Visibility, Has<AirShadowMarker>)` filtered to
`Or<(With<AirShadowMarker>, With<AirTetherMarker>)>` and `Without` each of the ring, bar-root, HP-bar and shield-bar
markers. Those filters make it disjoint from the system's existing `&mut Transform` and `&mut Visibility` queries.
For each child it writes `AirPose::of(instance)` through `set_if_neq`. The work goes in a helper,
`follow_air_pose(instance, children, &mut air_query)`. The bar update becomes a helper too
(`update_overhead_bar`), so the system stays within 100 counted lines (charter §2.4).

*Why there and not in the live synchroniser:* the system is the existing per-frame owner of indicator state; it runs
after the synchroniser (`system_ownership_tests::bubble_consumers_run_after_the_live_synchroniser` already lists it);
it serves the Lab preview's bubbles as well; and the red test U-2 can drive the production system unchanged from WP0
to WP2.

*Allocation:* the helper allocates nothing by construction: query lookups, `Copy` values and compare-first writes; no
collection, no handle clone, no asset access. A steady frame (constant heights) writes
nothing and flags no change. The valgrind child (`sync_allocation_count_tests.rs`) counts the live synchroniser's
systems, which do not include `update_indicators_system`; it and `sync_steady_state_tests.rs` stay **unchanged** and
must pass. Adding the indicator system to the valgrind window is a separate decision (§11).

### D-7. The ramp guard rail stays as it is (out of scope)

The rail lies 0.08 inside the ramp's footprint and 0.30 high, and a body riding a ramp's side wall meets it. A rail
has width, and walkable ground lies on both sides of the edge, so any rail breaks W1 on one side. Moving it changes
the ramp's look and is not part of B-243. T-B exempts exactly the envelope that the ramp plan's T2 exempts
(`footprint_margins`), and §11 records the rail as a candidate follow-up.

---

## 5. Tests

All new tests are written in WP0 and fail on `main` for the stated reason. Each later package turns its tests green.
Test names are final unless an amendment changes them.

### 5.1 Terrain, CPU

**Shared support.** `crates/client/tests/support/terrain_mesh.rs` takes `MapMesh` (all chunk triangles, bucketed by
cell) out of `ramp_geometry_tests.rs`, and the seeded synthetic map out of `mesher_golden_tests.rs`, unchanged (the
golden's hash must not move by the move: a check in WP0). It adds a Sutherland-Hodgman clip of a 3D triangle against
an axis-aligned XZ box, interpolating `y` along the clipped edges.

**T-A (unit, `mesher.rs` `#[cfg(test)]`): `wall_rows_never_cross_an_edge_towards_ground`.** It sweeps
`level_a ∈ 0..=4`, `level_b ∈ −1..level_a`, every run `ta..=tb` within `(level_b, level_a]`, the four axis
directions `d`, and 64 midpoints `p` on a half-integer grid (to vary the hash). For every row with `level_b ≥ 0` it
asserts `(row.xz − p)·d ≤ 0` **exactly**, and that the toe, when present, equals `p` bit for bit. **Tolerance 0,
derived:** every row is `p + d·k` with `k ≤ 0` in exact arithmetic (`k = disp − setback`, `disp ≤ A ≤ setback`), and
`f32` addition is monotone, so `p.x + k ≤ p.x` for `d.x = 1` and `p.x − k ≥ p.x` for `d.x = −1`. On `main` it fails
(toe at `p + 0.22·d`, rows up to `+0.06`).

**T-A2 (unit): `jitter_amplitude_fits_behind_the_setback`.** `WALL_JITTER_AMPLITUDE ≤ FOOT_WIDTH`, the premise of D-1.
It passes on `main` and guards the premise.

**T-B (integration, new `crates/client/tests/cliff_boundary_tests.rs`): W1 triangle by triangle.** For each walkable
cell `(x, y)` (in bounds, not abyss), let `S` be its simulation surface: `cliff_level_height(level)` for a plateau,
`RampProfile::height_at` for a ramp, which is planar per cell. Its open column is
`Q = (x + τ, x + 1 − τ) × (y + τ, y + 1 − τ)`. Every triangle near the cell is clipped to `Q`. A non-empty clip whose
highest vertex exceeds `S` at that vertex by more than `ε` is a violation, except inside a ramp's rail envelope (D-7,
the margins of `ramp_geometry_tests::footprint_margins`). A violation is reported with its cell, its height above `S`
and its horizontal depth into the cell.

- **Tolerances, derived from `f32` rounding.** Coordinates are below `M = max(width, height)`, and heights below
  `H_max = max(|ABYSS_WALL_BOTTOM_Y|, H(max level) + RAIL_HEIGHT)`. One clip is one `a + t(b − a)`, a few roundings.
  So `τ = 16·ulp(M)` and `ε = 16·ulp(H_max)`, with `ulp(v) = 2^(⌊log₂ v⌋ − 23)`, computed in the test from the map:
  for a 64-cell map `τ = 1.2e-4` and for heights below 16 `ε = 3.1e-5`. The test asserts its own premise, that every
  vertex lies within `[0, M]`.
- **Classification.** A violation whose clipped part lies within the open nook triangle of a lone-low dual corner
  (`|x − cx| + |y − cy| < ½` around corner `(cx, cy)`), or of a saddle's low corner, is a **corner** violation
  (B-245). Any other is a **profile** violation (B-243).
- **Maps:** `showcase_canyon`, `default_arena` (through the real `MapFile` loader), the golden's synthetic map (all 16
  cases including saddles, stacked runs, abyss, ramps of eight directions, a size not a multiple of the chunk), and the
  ramp plan's fixture (recessed and half-recessed ramps).
- **T-B1 `no_profile_rises_inside_walkable_ground_<map>`**: no profile violation. Red on `main` (feet everywhere,
  jitter), green after WP1.
- **T-B2 `no_terrain_rises_inside_walkable_ground_<map>`**: no violation of either class. Red until WP3. Under Q0136
  (b), this test gains an explicit, named exemption of the nook triangles with the owner's answer cited.

**T-C (existing tests that must stay green unchanged):** `terrain_tests.rs` (the flat 4×4 count fixture has only
void-side border walls and lone-high map corners, so its counts must **not** change; altitude agreement; watertight
seams; winding; unit normals; incremental rebuild; capacity reuse), and `ramp_geometry_tests.rs` T1 to T3.

### 5.2 Terrain, GPU

**G-1 (new `crates/client/tests/cliff_contact_gpu_tests.rs`): a body drawn tangent to a wall is not cut by the
cliff.** One offscreen child of the GPU harness (`support/gpu_run.rs`: radv with the shared lock per child, lavapipe in
CI, never wrapped in `with-gpu-lock`). It boots the production client with a renderer as `render_pool_gpu_tests.rs`
does: `boot_on_gpu`, the offscreen canvas, 1280×720, smog volume off through `LabState::opt_smog_volume`.

- **Staging** (`support/cliff_contact_stage.rs`, setup only, no order under test). The map of the shipped battle is
  read for a straight cliff edge `x = c` along `y`: at least 8 cells where the cells either side are ground plateaus
  one level apart, with no ramp, structure or resource within 2 cells, and flat low ground for 4 cells across and 8
  along the camera side. The unit is the local player's ground archetype with the smallest drawn radius among those
  whose drawn radius equals their terrain radius and whose media exist, chosen by catalog property, never by slug
  (C5). It is written at `x = c ± r` on the low side (the simulation holds bodies at clearance 0, so this is a state the
  simulation itself produces), not selected, clock paused.
- **Camera.** `harness.look_at((c, y₀))` at `MIN_CAMERA_DISTANCE`. The RTS camera has no yaw, so the vertical plane
  `x = c` contains the eye and the view direction, and the whole edge projects onto the canvas's centre column `x_c`
  (`camera.rs`, `RtsCamera::compute_transform`).
- **Captures.** `B` with the unit at the wall, then `A` with the unit written 20 cells away, after the same settle
  frames, the clock stopped as in `render_pool_gpu_tests::capture`.
- **The footprint.** For each pixel, the camera ray through its centre (a harness helper over
  `Camera::viewport_to_world`) is tested against the drawn bubble's sphere, read from its `GlobalTransform`: centre,
  and radius equal to the scale. The pixels whose ray hits the sphere form the silhouette `Σ`.
- **Assertions.**
  1. *Tangency (precondition, CPU):* the drawn sphere's nearest point to the plane `x = c` lies on the low side within
     `1/4096` world units (one fixed-point step) of the plane.
  2. *No cut (the test):* in the band `Σ ∩ {x_c + 2 ≤ column ≤ x_c + w}`, where `w` is the projected width of 0.30
     world units at the sphere's depth (≥ `FOOT_WIDTH + WALL_JITTER_AMPLITUDE`), at least 99 % of pixels differ
     between `A` and `B` by at least 2/255 in some channel. A pixel that does not change is one where terrain in front
     of the bubble's near-wall side hides it. The 2-column margin covers rasterisation and MSAA at the plane, which
     projects to a column boundary.
  3. *Control:* in `Σ` away from the wall (`column > x_c + w`), at least 99 % differ too. A bubble that changes too
     few pixels anywhere would otherwise pass assertion 2 for the wrong reason.
- **Red on `main`:** with the camera at a 58° pitch, about 144 px per world unit at the contact (computed by the test),
  the foot (0.22, about 30 px) and the jitter (about 9 px) cover a band next to the wall, so assertion 2 fails. The
  numbers go into the WP0 evidence.
- **Captures are kept** under `~/scratch/terrain-indicators/g1/<driver>/{A,B}.png`, with a mask PNG of `Σ` and of the
  unchanged pixels, for the review capture (§6).
- `crates/gate/suites.ron`: `"client/cliff_contact_gpu_tests": (class: Gpu, triggers:
  ["crates/client/src/{terrain/**,scene_controller.rs,bubble_shader.rs}", "assets/shaders/terrain*"])` (**verify**
  the terrain shader's file name).

The nook case (B-245) has no GPU test. With the camera's fixed yaw, a nook's second wall either faces away (the
sphere's front hides the cut) or stands in front (the 3-unit plateau hides the body). T-B2 and the review capture
cover it.

### 5.3 Air indicators

- **U-1 (unit, `indicators.rs` `#[cfg(test)]`): `the_air_shadow_lies_on_the_ground_for_every_radius`.** Over heights
  `{0, 0.4, 0.6, 2.5, 5.0}`, radii `{0.15, 0.25, 0.4, 1.25}`, spheres and domes, and `ground_z ∈ {0, 3, 6}`, it
  composes `IndicatorPose::of(instance).shadow` and `.tether` with the parent transform the synchroniser writes, and
  asserts:
  - the shadow's world height is `ground_z + SHADOW_LIFT` within `1e-5`;
  - the tether's top is at the bubble's centre and its bottom at the shadow;
  - `air` is shown iff the height exceeds `AIRBORNE_HEIGHT`;
  - no scale is zero.

  Red on `main` (B-244: `ground_z + r`).
- **U-2 (integration, new `crates/client/tests/air_indicator_tests.rs`): `the_shadow_follows_a_change_of_altitude`.**
  A minimal app with `setup_indicator_assets_system` and the production `update_indicators_system`, as
  `round2_visual_tests.rs` builds one. A bubble is spawned through `attach_bubble_indicators` from an air record. Then,
  frame by frame, the test writes the record (`BubbleInstance`) and the parent transform as the synchroniser does,
  through:
  1. a cliff crossing (`ground_z` 3 → 0, height constant);
  2. a climb (height 2.5 → 4.0);
  3. a descent below the airborne threshold (height → 0.3, `air` hidden);
  4. a take-off again.

  After each step it asserts the shadow's and the tether's `GlobalTransform` against §D-5 within `1e-4`, and their
  visibility. A final steady frame asserts that no shadow or tether `Transform` or `Visibility` is flagged changed
  (Bevy change ticks). Red on `main`: steps 2 to 4 leave the arrival pose, and every step shows the B-244 offset.
- **I-1 (integration, `air_indicator_tests.rs`, battle harness, headless):
  `an_air_unit_crossing_a_cliff_keeps_its_shadow_on_the_ground`.** On the shipped skirmish (by property: the local
  player's air archetype with media and the smallest drawn radius), the unit is staged two cells from a cliff and
  ordered by right click across it. For every frame until it is three cells past, the test asserts that the shadow's
  world height is the record's `ground_z + SHADOW_LIFT` within `1e-4`, the tether's ends, and `air` shown. This is the
  production path end to end (snapshot → synchroniser → indicators). Red on `main` (B-244).
- **Unchanged and green:** `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind),
  `render_pool_tests.rs` and `render_pool_gpu_tests.rs` (a reused bubble still equals a fresh one: both now use the new
  pose), and `round2_visual_tests.rs` (a ground unit's air shadow and tether exist, hidden).
  `system_ownership_tests.rs` needs no new entry, because the system is already listed.

---

## 6. Goldens and the owner's review capture

No golden is re-baselined without the review capture of its package under
`~/scratch/terrain-indicators/review/` existing first, and the re-record commit names it. The owner reviews from
there. If the owner rejects a look, the package's commits are reverted, not patched over.

### 6.1 Which goldens move, and why

| Golden | Moves? | Reason, and how it is re-recorded |
|---|---|---|
| `mesher_golden_tests.rs`, three full mesh hashes and three ramp-free chunk hashes | **yes, twice** | WP1: D-1 moves every wall and plateau-inset vertex on edges whose low side is ground (map-border and abyss walls have setback 0 and keep their rows). WP3: D-3 changes the table for cases 5, 7, 10, 11, 13, 14. The test prints `{hash:#018x}` on failure; the implementer copies the new constants and extends the module's `# Invariants` history: "re-recorded at `<sha>`, plan `2026-11-30-terrain-and-air-indicators` WP1: cliff profiles set back by `FOOT_WIDTH` where the low side is ground (B-243, W1)" and the same for WP3 (B-245). The pick hashes and counts **must not** change (the picker reads the grid); a change is a stop. |
| `mesher_golden_tests.rs`, texture-array hash | no | not terrain geometry |
| `terrain_tests.rs` count fixture (4×4 flat) | no | only void-side border walls (setback 0) and lone-high corners; a change is a stop |
| `smog_golden_tests.rs` | **no** | the smog reads `TerrainState::map` sizes and records' `ground_z`, never the mesh (`smog/mod.rs` `live_battle_volume`). The task expected it to move; WP1 runs it to confirm, and a move is a stop to explain, never a re-record. |
| `round2_visual_tests.rs` | **no** | it holds no picture goldens (shader validation, layouts, framing, indicator visibility). B-238's change keeps visibility semantics. |
| `effect_slice_gpu_tests.rs` stills (per driver) | no | the effect lab draws no terrain mesh |
| `render_pool_gpu_tests.rs` | no | two children compared with each other, both on the new pose |
| showcase takes (`client_diagnostics/showcase_tests.rs`) | no stored golden | two renders compared with each other; the **published showcase video** changes look, so it is in the review capture |
| `terrain_tests.rs` `test_terrain_preview_renders` | thresholds only | must pass; its five PNGs feed the review capture |

### 6.2 Review capture, WP1 (B-243) and WP2 (B-238, B-244)

Under `~/scratch/terrain-indicators/review/wp1/` and `.../wp2/`, each with `before/` (a build of `main` at the
branch's base, in a separate worktree and target directory) and `after/`. Every GPU command goes through
`WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock …`, one capture per lock.

- the `terrain_preview` binary's five poses of `showcase_canyon` (1600×900) and of `default_arena`;
- G-1's `A`, `B` and mask images, before (red) and after (green);
- a close-up pose of a straight cliff with a unit at its foot, and one with a small unit on the high side at the brink
  (D-4). These are extra poses through the same harness child as G-1, enabled by an environment variable in the
  test (**verify** that the harness allows a second capture in one child; otherwise a second child);
- WP2: an air unit before, over and after a cliff from the I-1 staging, rendered by a GPU child of
  `air_indicator_tests.rs` gated to review runs (not part of the gate), and one shadow per air archetype's radius;
- the CI showcase take's `terrain` and `advance` scenes at CI size, run as `.github/workflows/ci.yml`'s video job runs
  it (**verify** the command);
- `contact_sheet.png`: before and after side by side per view (`montage` from ImageMagick, present on the server);
- `README.txt`: what each image shows, which bug it concerns, the commit of each side, and the measured figures (rows
  set back, toe offsets 0, shadow heights, D-4's brink figure).

### 6.3 Review capture, WP3 (B-245, Q0136)

Under `~/scratch/terrain-indicators/review/wp3/`: the canyon staircase and the diagonal level-1/2 cliff of
`showcase_canyon` from the `p0_base` and `overview` poses and one close-up of a staircase. There are three columns:
`main`, after WP1 (45° nooks, set back), and after WP3 (square nooks). A T-B2 heat map of the WP1 tree's corner
violations completes the set: one PNG with each violating cell coloured by depth. This is what the owner looks at to
answer Q0136.

---

## 7. Acceptance

1. T-A, T-A2, T-B1 and T-B2 (all four maps), G-1 (radv locally; lavapipe in CI), U-1, U-2 and I-1 are green.
2. Every test of §5.1 T-C and §5.3 "unchanged" is green with no edit to its file, apart from the extraction into
   `support/terrain_mesh.rs`, which must leave `mesher_golden_tests`' synthetic hash identical before its re-record.
3. The mesher golden hashes are re-recorded only as §6.1 says, with the review capture present. Pick hashes, counts
   and every other golden are identical.
4. The WP0 mesher check, re-run on the final tree and limited to edges whose low corner is ground, reports
   **0** rows towards the low side, down from 5,501.
5. No file under `crates/sim_*` or `assets/` changes. The golden replays are not re-run, because nothing they hash
   changed. `git diff --stat main -- crates/sim_* assets` is empty.
6. `cargo gate package` is green on the final tree, run by the verifier agent (charter §4).
7. Records: `DESIGN.md` §4 rows (§12), the evidence file `docs/evidence/2026-11-30-terrain-and-air-indicators.md`,
   this plan's `## As built`, and its `docs/plans/README.md` row.

---

## 8. Work packages

One package (crate `client`, plus records). It runs on branch `terrain-indicators` in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/terrain-indicators`, and large outputs go under
`~/scratch/terrain-indicators/`. WP2 does not depend on WP1 and may run first. WP3 is last, so that Q0136 (b) drops
it cleanly.

**WP0: measurements and red tests.**
1. Extend the scratch mesher check (copy it to `~/scratch/terrain-indicators/mesher_check/`, never committed) to
   count, per shipped map:
   - foot toes on edges with a ground low side;
   - canonical rows towards the low side, split by whether the low corner is ground or void;
   - lone-low dual cells and saddles, with their nook area.
2. From the catalog, list:
   - every air archetype's drawn radius and today's shadow height above the ground (§2.2);
   - every ground archetype's drawn radius against `FOOT_WIDTH + RIM_WIDTH` (D-4).
3. Move `MapMesh` and the synthetic map into `support/terrain_mesh.rs` and check that `mesher_golden_tests` still
   passes unchanged.
4. Write T-A, T-A2, T-B (both), G-1, U-1, U-2, I-1, and the `suites.ron` entries (the gate's catalog test requires
   every target to be listed; **verify** the rule for the new CPU files' classes). Run each new target alone and
   record its red output. G-1 runs through its harness; never run a workspace test or `cargo gate`.
5. Evidence §1 with all figures and the red outputs.

**WP1: B-243, the setback (D-1, D-2).**
1. `generate_wall_profile` setback and toe; the floor insets in `emit_dual_cell`; the module docs (W1, R6's note that
   ramp edges have setback 0); the constants' docs (`FOOT_WIDTH` now also the setback).
2. Green: T-A, T-B1 (four maps), G-1. T-C unchanged and green, and `smog_golden_tests` unchanged.
3. Review capture §6.2 (WP1 part).
4. Then re-record the mesher golden hashes (§6.1) in their own commit, naming the review directory.

**WP2: B-238 and B-244, the air pose (D-5, D-6).**
1. `AirPose::of`, `SHADOW_LIFT`, `IndicatorPose::of` delegating to it; the `follow_air_pose` and
   `update_overhead_bar` helpers in `update_indicators_system`; the module docs: "the shadow and tether follow the
   record's ground every frame".
2. Green: U-1, U-2, I-1. Unchanged and green: steady state, valgrind (`sync_allocation_count_tests`, run alone under
   `flock ~/scratch/gate.lock`), render pool headless and GPU, `round2_visual_tests`.
3. Review capture §6.2 (WP2 part).

**WP3: B-245, the corners (D-3; Q0136 default (a)).** If Q0136 is answered (b) before WP3 starts, WP3 instead
adds the named exemption to T-B2, records B-245 as a sanctioned deviation, and the table is untouched. If it is
answered (c), WP3 stops and the orchestrator commissions a plan.
1. `MarchingCase::regions`, `DualVertexRef::Centre`, the mitred corner profile, low-connected saddles; the table's doc
   comment.
2. Green: T-B2 (four maps). T-A, T-B1 and G-1 still green, and T-C unchanged (the 4×4 counts in particular).
3. Review capture §6.3.
4. Then the second mesher golden re-record, in its own commit.

**WP4: records.** `DESIGN.md` §4 (§12). If B-243's row has reached `main` through `wall-clearance-plan`, mark it
fixed; otherwise add it here as fixed, citing `1c146c83`. Then: the evidence file; this plan's `## As built`; the
README row status. Re-run the scratch mesher check (acceptance 4). Hand back after `cargo gate quick` and the targets
above; the verifier runs `cargo gate package`.

---

## 9. Stop rules

Stop and report to the orchestrator, with the evidence, when:

1. any file under `crates/sim_*`, any map, catalog or scenario file, or any simulation golden would change;
2. the pick hashes, the 4×4 count fixture, `smog_golden_tests`, an effect still, or a showcase self-comparison changes;
3. T-B needs a tolerance above the derived `τ`, `ε`, or G-1 a margin above 2 columns or a share below 99 %;
4. a stacked run, chunk seam or ramp edge stops being watertight (`terrain_tests` seams, ramp T3);
5. the steady-state or valgrind test needs an edit to pass, or any new code path allocates per frame;
6. D-4's brink figure for a shipped archetype exceeds 0.2, or the review capture shows the look departing from the
   art beyond the translation of D-1 (the orchestrator then asks the owner before any re-record);
7. a GPU child cannot get the physical GPU or a real shader fails to load. Fail loudly (charter §1.2); never fall back
   silently.

---

## 10. Files

| File | Change | WP |
|---|---|---|
| `crates/client/src/terrain/mesher.rs` | setback and toe (D-1), floor insets, W1 docs; T-A, T-A2; table regions, centre vertex, mitred corners, saddles (D-3) | 1, 3 |
| `crates/client/src/indicators.rs` | `AirPose`, `SHADOW_LIFT`, `follow_air_pose`, `update_overhead_bar`; U-1 | 2 |
| `crates/client/tests/support/terrain_mesh.rs` (new) | `MapMesh`, synthetic map, clip helper | 0 |
| `crates/client/tests/support/cliff_contact_stage.rs` (new) | G-1 staging | 0 |
| `crates/client/tests/cliff_boundary_tests.rs` (new) | T-B1, T-B2 | 0 |
| `crates/client/tests/cliff_contact_gpu_tests.rs` (new) | G-1 | 0 |
| `crates/client/tests/air_indicator_tests.rs` (new) | U-2, I-1 (and the review child) | 0 |
| `crates/client/tests/ramp_geometry_tests.rs`, `mesher_golden_tests.rs` | use the support module; golden constants and history | 0, 1, 3 |
| `crates/client/tests/support/battle_harness.rs` | a pixel-ray helper over `Camera::viewport_to_world` | 0 |
| `crates/gate/suites.ron` | the three new targets | 0 |
| `DESIGN.md`, `docs/evidence/2026-11-30-terrain-and-air-indicators.md`, this plan, `docs/plans/README.md` | records | 0, 4 |

---

## 11. Out of scope and recorded decisions

- **The ramp guard rail** (D-7): inside the ramp by 0.08 and 0.30 high, it meets a body riding the ramp's side wall.
  It is a candidate follow-up with its own look decision. Not filed as a bug by this plan; the orchestrator may.
- **A non-local staircase** that keeps straight 45° diagonals without entering low cells (Q0136 (c)): a plan of its
  own if the owner chooses it.
- **The valgrind window over `update_indicators_system`:** the live child counts the synchroniser's systems only. D-6
  adds no allocating call, and the steady-state test covers asset and entity churn, so extending the window is a
  separate improvement that touches a test the task keeps unchanged.
- **A mode's `mover_type_override` and altitude:** `Placed::new` uses the archetype's mover type, so a unit whose mode
  makes it airborne (none shipped) would be drawn on the ground. That is a simulation-side presentation question,
  noted for the orchestrator, not changed here.
- **The attack-snapshot images** carry no air indicators, so they are untouched.
- **The Lab preview and the swarm:** the Lab's air heroes get the corrected shadow for free through
  `update_indicators_system`, and the swarm has no indicators.

---

## 12. Bug rows

For `DESIGN.md` §4. Numbers are provisional, next free at merge.

```
| B-243 | (row as filed on `wall-clearance-plan`, `1c146c83`) | fixed by plan [`2026-11-30-terrain-and-air-indicators`](docs/plans/2026-11-30-terrain-and-air-indicators.md) WP1: profiles set back by `FOOT_WIDTH` where the low side is ground, toe on the edge (W1) |
| B-238 | (row as on `main`) | fixed by plan [`2026-11-30-terrain-and-air-indicators`](docs/plans/2026-11-30-terrain-and-air-indicators.md) WP2: `AirPose` written every frame by `update_indicators_system` |
| B-244 | The air shadow and tether stop one bubble radius above the ground: `IndicatorPose::of` (`indicators.rs`) offsets them from the bubble's centre by the record's height alone, while the centre sits a radius above the record's altitude, so the shadow is drawn at `ground_z + r` (for drawn radius below `MIN_INDICATOR_RADIUS`, `ground_z + h + r − h·r/0.4`, 1.19 at r = 0.25); up to 1.25 above the ground for the Carrier analogue. Found by plan `2026-11-30-terrain-and-air-indicators` §2.2 (number provisional, next free at merge) | fixed by that plan, WP2 |
| B-245 | The dual grid's 45° cut at a concave cliff corner (marching cases 7, 11, 13, 14) and the high-connected saddles (5, 10) draw the high plateau inside the walkable low cell, up to 0.354 from the cell corner (area 1/8 cell), so a body pressed into the nook is drawn cut by the cliff (0.198 at radius 0.375). Plan 21b §5.3 documented the cut as a deviation but not its side. Found by plan `2026-11-30-terrain-and-air-indicators` §1.4 (number provisional, next free at merge) | fixed by that plan, WP3 (Q0136 default (a)); or, under Q0136 (b), a sanctioned deviation |
```

---

## 13. Owner question (filed as [Q0136](../questions/closed/0136-concave-cliff-corners.md))

Not filed by this commit. The default is what WP3 builds; filing it is the orchestrator's call.

```
# Q0136: At a concave cliff corner, should the drawn cliff keep its 45° cut, or follow the cells?

- **Asked:** 2026-11-30
- **Blocking:** no
- **Default until answered:** (a) square nooks; WP3 of plan 2026-11-30-terrain-and-air-indicators builds it

## Background

The simulation's cliffs run along cell edges, and a unit on low ground may stand in the corner where two cliffs
meet. The client draws such an inner corner with a 45° cut (the StarCraft II look; a staircase of single-cell steps
becomes one straight diagonal cliff), so the cliff covers a triangle of the walkable cell, up to 0.35 deep at the
corner, and a unit pressed into the corner is drawn inside the rock. Outer corners keep their 45° cut either way,
because it removes rock instead of adding it. The review capture
`~/scratch/terrain-indicators/review/wp3/contact_sheet.png` shows the canyon of `showcase_canyon` in each option.

## Options

- **(a) Square inner corners (default).** The cliff follows the cells at inner corners and keeps the 45° cut at
  outer corners. No unit is ever drawn inside rock. Diagonal cliffs look toothed: chamfered steps with square nooks.
- **(b) Keep the 45° cut** as a sanctioned deviation. Diagonal cliffs stay straight; units in inner corners are drawn
  up to about 0.2 inside the rock.
- **(c) Straight diagonals moved onto the high side.** Keep the straight 45° look by drawing each diagonal through the
  low cells' corners, so it lies entirely in high ground. Low units are never cut; units on the high plateau stand
  up to half a cell over the drawn edge. It needs a wider rule than one dual cell and its own plan.

Recommendation: (a), because it is exact and local; (c) if the straight canyon look matters more than a later plan's
cost.

## Answer
```

---

## As built

Built on branch `terrain-indicators-plan` (worktree `.claude/worktrees/terrain-indicators-plan`, target
`~/cargo-target/terrain-indicators`), on `main` at `e5ebc043`. All figures are in the evidence file
[`2026-11-30-terrain-and-air-indicators`](../evidence/2026-11-30-terrain-and-air-indicators.md). The review captures
are under `~/scratch/terrain-indicators/review/{wp1,wp2,wp3}/`.

| Package | Commits | What |
|---|---|---|
| WP0 | `cbd8a8e4`, `f7955374` | Every new test, red on `main`'s production code. Support modules `terrain_mesh.rs`, `cliff_contact_stage.rs`, `gpu_battle.rs` and `air_crossing.rs`. The WP2 review child. Catalog entries. |
| WP1 | `aaa5d450`, `0c682640` (golden re-record) | `edge_setback`, the toe on the edge, the floor insets, W1 in the module documentation |
| WP2 | `8ced7c35` | `AirPose::of`, `SHADOW_LIFT`, `follow_air_pose`, `update_overhead_bar` with `OverheadBarParts` |
| WP3 | `6c47fc4d`, `aca9dc86` (golden re-record) | `regions` and `low_regions`, `DualVertexRef::Centre`, `ContourSegment::through_centre`, the mitred corner profile (`ProfileFrame`), low-connected saddles, R3's new `dual_floor_level_beside` |
| WP4 | this commit | records |

Green: T-A, T-A2, T-B1 and T-B2 on all four maps, G-1 on radv, U-1, U-2 and I-1. These are green and unchanged:
`terrain_tests` (the 4×4 counts and the seams), `ramp_geometry_tests`, `smog_golden_tests`,
`sync_steady_state_tests`, `sync_allocation_count_tests` (valgrind), `render_pool_tests`, `render_pool_gpu_tests`,
`round2_visual_tests`, `player_presentation_tests` and `system_ownership_tests`. The mesher's pick hashes, its hit
counts and its texture-array hash never moved. No file under `crates/sim_*` or `assets/` changed, and
`golden_replay_tests` and the matchup matrix pass. The scratch mesher check counts 0 rows on the low side of
their edge, down from 5,501 (acceptance 4).

Deviations, each recorded in its commit and in the evidence file:

1. **Void edges are set back too** (D-1). Edges whose low side is void are set back by `WALL_JITTER_AMPLITUDE`
   (0.06), not 0. Without it, T-B1 on the synthetic map caught a straight contour between a void-side and a
   ground-side profile crossing into the ground cell. Border and abyss walls move 0.06, and no wall row of any edge
   lies on its low side.
2. **The shadow's span is not clamped** (D-5). The shadow uses the exact span `h + r_off − SHADOW_LIFT`, and only the
   tether's length is floored at `SHADOW_LIFT`. D-5's clamp would put a grounded dome's hidden shadow below the
   ground and fail U-1.
3. **The L is one polygon** (D-3). A concave corner's L is one polygon, star-shaped from the corner diagonal to the
   nook. Two convex parts doubled a corner vertex on chunk seams and failed the unchanged seam test.
4. **No mitre beside a ramp** (D-3). Beside a ramp (only the corner diagonal to the nook can be one), the concave
   corner stands at the centre unmitred, as R6's centre profile does.
5. **R3 follows the table.** `dual_floor_level_beside` becomes `min(corner, max(edge neighbours))`, so the ramp pass
   matches the new table.
6. **G-1's silhouette is the drawn disc** (§5.2). `Σ` is the disc `bubble_dome.wgsl` draws, a screen-space circle of
   the radius at the centre's depth, not a ray-sphere hit. The shader discards the perspective sphere's ears, and
   with the ray-sphere `Σ` the control failed at 97 %.
7. **G-1's wall is shorter** (§5.2). The staged wall is straight 5 rows beyond the contact row and 2 rows before it
   (8 cells, the rows the camera sees). `showcase_canyon` has no wall with 8 flat rows on the camera side clear of
   ramps. The wall found is `x = 26`, row 31.
8. **The review child is its own target** (§6.2). The WP2 review child is `air_indicator_review_gpu_tests.rs`, a
   separate Gpu target with an ignored parent, because the gate classes any file that launches GPU children as Gpu.
9. **Review substitutes** (§6.2, §6.3):
   - *Before columns:* `before/` was built from WP0, which has `main`'s production code, in a separate worktree and
     target directory.
   - *`default_arena`:* it has no preview pose, because `terrain_preview` renders `showcase_canyon` only. WP1 moved
     its mesh only by the void setback, and WP3 not at all.
   - *Staircase close-ups:* they are crops of the preview poses.
   - *Showcase scenes:* the `terrain` and `advance` scenes come from `showcase take --take battle`, run the way
     `showcase render --driver radv` runs it.
10. **The support module holds the ramp fixture too** (§5.1), so that T-B can mesh it.

Records: `DESIGN.md` §4 (B-238 fixed; B-243 added as fixed, citing `1c146c83`, because `wall-clearance-plan` had
not merged; B-244 and B-245 added, fixed), plan 21b §5.3 (the saddle note marked superseded), this section, the
README row, and the evidence file. Q0136 stays open, and its default (a) is built. WP3 is the last package, so an
answer of (b) or (c) reverts `6c47fc4d` and `aca9dc86`.
