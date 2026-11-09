# Blueprint: Ramp Geometry in the Terrain Mesher (the Top Floor Ends Where the Ramp Begins)

**Owner report (chat, 2026-11-09):** "the ramps have artifacts. the top floor continues over the ramp." Seen in the
showcase video (<https://close2.github.io/space-scuffle/showcase.mp4>).

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-09 against `main` at `33a8e82`;
implemented the same day, WP1 to WP3 (§10).
Line numbers drift, so search for the quoted symbol. The numbers in §2.3 are derived from the code
(`corner_marching_level`, `emit_dual_cell`, `RampProfile::height_at`) and the shipped map files; the mesh probe of
§2.4 was written but its build was stopped under a server load of about 190 before it ran, so WP1's first step
ran it; the figures the plan marked **verify** are replaced below by the measured values (**measured**), and §10
records the implementation.

**Kind of change.** The cause is single (§2.3) and client-only, and the obvious test exists (§4, T1), so the bug
qualifies for a short note under charter §6 (Q0054) **if** the fix is limited to that cause. The complete fix is
not one line: it gives every ramp cell edge one owner, which also repairs hidden geometry and recessed ramps that
no shipped map has yet (§3). This plan is therefore written as a full plan, small enough to run as three work
packages in one package (`client`). The orchestrator may run WP1 alone as the short-note fix and defer WP2 (D-7).

**Questions.** None open bear on this plan. The simulation's ramp and vision rules do not change (§6); no owner
decision is needed.

---

## 1. Summary

1. **Cause.** The client's dual-grid marching squares classify the corners of ramp cells by **rounding** the ramp's
   height at that corner to the nearest cliff level (`corner_marching_level` in
   `crates/client/src/terrain/mesher.rs`). From the middle of a ramp onwards its corners round up to the upper
   level, so the dual-grid pass treats the upper half of every ramp as part of the upper plateau. It emits a flat
   upper-level floor polygon, and cliff walls with rims, over that half of the ramp; the sloped ramp quad is drawn
   underneath it. On the shipped maps the slab reaches 1.5 cells (showcase, run of 3) or 2.5 cells (default arena,
   run of 4) down the ramp and floats up to 1.5 or 1.875 world units above the slope (§2).
2. **Not shading, not map data, not the simulation.** The ramp surface itself is correct
   (`RampProfile::height_at`), the picker hits the ramp plane, and units walk on the slope: in the video they walk
   *under* the slab. The map data is valid. The fix is entirely in the client mesher.
3. **Rule.** A ramp cell's footprint belongs to the ramp: the dual-grid pass emits no floor and no wall inside it
   or on its edges, and the ramp cell emits every wall on its own edges, against the neighbour's actual surface
   (§3).
4. **Tests first.** Three CPU property tests over both shipped maps and a new synthetic ramp fixture: the top
   surface over every ramp footprint is the simulation's height, nothing but the ramp quad and its rails meets a
   ramp footprint, and every height step along a ramp edge is closed by a wall (§4). The mesher goldens are
   re-recorded once, with the cause stated. A GPU visual check through the harness renders a ramp close-up (C21).

---

## 2. Reproduction and Localisation

### 2.1 The video

Downloaded to `~/scratch/rampplan/showcase.mp4` (1280×720, 25 fps, 44 s). Stills were cut with ffmpeg:

| Still | Scene | What it shows |
|---|---|---|
| `~/scratch/rampplan/contact.png` | all, 1 fps | contact sheet; ramp shots at 5–11 s (`terrain`) and 19–22 s (`advance`) |
| `~/scratch/rampplan/full/t_6.png` | `terrain`, frame ~25 | the player-0 plateau; a jagged sliver at the plateau edge over the west ramp |
| `~/scratch/rampplan/full/t_7.5.png` | `terrain`, frame ~62 | **the defect**: a flat slab at the top level juts out of the metal plateau over the west ramp (cells x 26–28, y 8–11); one unit is visible *below* the slab on the slope |
| `~/scratch/rampplan/full/crop_7.5.png` | crop of the above, 3× | the slab, its sand texture (it is drawn over dirt ramp cells) and the dark ramp side walls under it |
| `~/scratch/rampplan/full/t_9.png` | `terrain`, frame ~100 | the same ramp from further down: the plateau edge continues past the ramp head |

The camera of the `terrain` scene (`assets/captures/showcase.ron`) moves from focus (12, 14) to (33, 13) over 75
frames, so at frame ~62 it looks at about (29, 13), just north-east of the west ramp of `showcase_canyon`.

### 2.2 The ramps of the shipped maps

From `assets/maps/*.scmap.ron` (row 0 is y = height − 1). Every shipped ramp is a **protruding** ramp: its lateral
neighbours are at the low level (or abyss), never at the upper level.

| Map | Ramp | Cells | Run length | Low → high | Lateral neighbours |
|---|---|---|---|---|---|
| `showcase_canyon` | east | x 35–37, y 52–55 | 3 | 1 → 2 (landing x 38) | level 1 |
| `showcase_canyon` | north | x 31–33, y 23–25 | 3 | 1 → 2 (landing y 26) | level 1 |
| `showcase_canyon` | west | x 26–28, y 8–11 | 3 | 1 → 2 (landing x 25) | level 1 |
| `default_arena` | north | x 30–33, y 30–33 | 4 | 1 → 2 (landing y 34) | abyss (x 28–29, 34–35) |

`sim_map::validation` constrains only the two landings (`RampUpperLandingMismatch`, `RampLowerLandingMismatch`),
not the lateral neighbours, so the map editor can author recessed ramps (lateral neighbours at the upper level),
half-recessed ramps and ramps beside higher plateaus. No shipped map has one.

### 2.3 The cause, with numbers

`CLIFF_LEVEL_HEIGHT` is 3 world units. A ramp of run length `n` from level `L` has, at the cell edge `k` steps up
the run (`k = 0..=n`), the height `3L + 3k/n`. `corner_marching_level` converts that to a marching level with
`(h / 3).round()`:

| Run | Edge `k` | Height (L = 1) | `h / 3` | Marching level today |
|---|---|---|---|---|
| 3 | 0, 1 | 3.0, 4.0 | 1.00, 1.33 | 1 |
| 3 | 2, 3 | 5.0, 6.0 | 1.67, 2.00 | **2** |
| 4 | 0, 1 | 3.0, 3.75 | 1.00, 1.25 | 1 |
| 4 | 2, 3, 4 | 4.5, 5.25, 6.0 | 1.50 (rounds away from zero), 1.75, 2.00 | **2** |

A dual cell is centred on a cell corner and covers a quarter of each of its four cells. Every dual cell whose four
corner samples are all level 2 emits a full flat quad at `H(2) = 6` (`emit_dual_cell`, the `hi == lo` branch), and
every dual cell where they are mixed emits a level-2 plateau polygon with cliff walls and rims. For the west ramp of
`showcase_canyon` (run 3, ascending west, landing at x 25–26):

- The ramp ascends west from the low landing at x = 29 (edge k = 0) to the high landing at x = 25 (edge x = 26 is
  k = 3). Dual cells centred on the cell corners at x = 27 (k = 2, height 5) and x = 26 (k = 3, height 6) have
  all-level-2 corner samples inside the ramp rows, so they emit flat quads at y = 6 covering x ∈ [26.5, 27.5] and
  [25.5, 26.5].
- Over x ∈ [26, 27.5] the ramp surface lies between 6.0 and 4.5, so the slab floats up to **1.5 world units** above
  it at x = 27.5 — half a cliff level, 1.5 cells long, across the full width of the ramp.
- Along the two lateral edges (y = 8 and y = 12) the mixed dual cells emit level-2 plateau polygons with rims and
  cliff walls from 6 down to 3 over the same 1.5 cells, in the plane of the ramp's own side walls.

For the run-4 ramp of `default_arena` the slab starts at edge k = 2 (y = 32), covers 2.5 of 4 cells
(y ∈ [31.5, 34]) and floats up to **1.875** above the slope (at y = 31.5 the ramp is at 4.125).

This matches the video: in `t_7.5.png` the slab leaves the plateau at the ramp head and ends about halfway down
the three-cell ramp, and a unit on the slope is drawn below it. **Measured** with the probe (§2.4) on `5accec2`
(64 vertical rays per ramp cell at the points `(i + 0.5) / 8`): every sample past the middle of each ramp is off,
32 of 64 in the cell of index 1 of a run of 3 and all 64 in index 2 (showcase), and 32 of 64 in index 1 and all
64 in indices 2 and 3 of the run of 4 (default arena); the highest hit is the flat floor at 6.0, at most **+1.44**
(showcase, at x = 27.44, where the analytic 1.5 lies at the cell boundary x = 27.5 between samples) and **+1.83**
(default arena, analytic 1.875) above `terrain_altitude_at`. No sample of the lower half is off except the guard
rail tops (+0.30) at the lateral samples 0.0625 cells from a side edge. Log:
`~/scratch/rampplan/probe_measured.log`.

The lower half of each ramp has the converse, hidden defect: its corners round to level 1, so a flat level-1 quad
lies under the slope (invisible, but drawn), and next to the abyss in `default_arena` the dual-grid cliff wall
from `H(1)` down to the abyss floor lies in the same plane as the ramp's own side wall over the band below
`H(1)` (overdraw and z-fighting there).

### 2.4 Probe and renders

- **Mesh probe** (uncommitted, in the planning worktree): `crates/client/tests/zz_ramp_probe.rs` builds every chunk
  of both shipped maps with `build_chunk_mesh`, casts 64 vertical rays per ramp cell (an 8×8 grid of cell-local
  points) and compares the highest triangle hit with `GameMap::terrain_altitude_at`; it also counts flat
  (floor-polygon) triangle hits above the ramp surface. Output: `~/scratch/rampplan/probe.log`.
- **Renders.** Three probe poses for `terrain_preview` (`probe_ramp_w`, `probe_ramp_n`, `probe_ramp_e`, camera 11
  units from each ramp's centre at the preview's 55° pitch) are kept as `~/scratch/rampplan/terrain_preview_probe_poses.diff`;
  the probe source is `~/scratch/rampplan/zz_ramp_probe.rs`. They were not rendered during planning (same build
  stop). WP1 renders `probe_ramp_w` on the RX 580 (`with-gpu-lock`) and on `lavapipe` before and after the fix;
  since the defect is in the CPU mesh, both drivers are expected to show it identically (**measured**: the two
  `probe_ramp_w` renders of `5accec2`, `~/scratch/rampplan/before_radv/` and `~/scratch/rampplan/before_lvp/`,
  show the same slab and give identical footprint statistics, §10).

### 2.5 Geometry, not shading

- The slab is emitted by `emit_floor_polygon` at `cliff_level_height(2) = 6` with an upward normal, so the artifact
  is in the mesh, not in a shader, decal or texture projection; the video comes from `lavapipe`, and nothing in the
  path is driver-specific. Its sand colour comes from the splat of the ramp cells under it; the shader does what it is
  told.
- The ramp quad is at the simulation's height (`emit_ramp_surface` samples `RampProfile::height_at`), and the picker
  (`terrain/picking.rs`) intersects ramp planes analytically, so clicks already land on the slope.
- `test_surface_heights_match_simulation` (`crates/client/tests/terrain_tests.rs`) never saw the defect because it
  samples only cells whose eight neighbours are uniform non-ramp cells; `test_ramp_surface_continuous_with_landings`
  checks only vertices on the two landing edges.

---

## 3. The Geometry Rule

### 3.1 Ownership

- **R1 (footprint).** The surface over a ramp cell's footprint `[x, x+1] × [y, y+1]` is the ramp quad alone. The
  dual-grid pass emits no floor area inside a ramp footprint, at any height (not even hidden under it), and no wall
  inside it or on its edges.
- **R2 (ramp edges).** Every edge of a ramp cell is owned by that ramp cell and is handled in two **half-edges**,
  split at the edge midpoint (each half lies in a different dual cell). For each half-edge the ramp cell compares
  its own edge line (from `RampProfile::height_at`) with the neighbour's surface along that half-edge and emits a
  vertical wall between them when they differ by more than `1e-4`, facing the lower side:
  - neighbour lower (plateau at a level ≤ `L`, abyss): the existing **side wall**, facing out of the ramp, down to
    the neighbour (abyss: `ABYSS_WALL_BOTTOM_Y`);
  - neighbour higher (plateau at a level ≥ `L + 1`): a new **flank wall**, facing into the ramp, from the
    neighbour's floor down to the ramp edge (the cliff face beside a recessed ramp);
  - neighbour a ramp cell: one wall between the two edge lines, emitted only by the cell whose half-edge midpoint
    is higher (none when equal, which covers parallel cells of one run and consecutive cells of a run);
  - landing edges are handled by the same comparison; on a valid map they are equal and emit nothing, on a map
    being edited they close the step.
  Because a ramp spans exactly one level and plateau levels are integers, the ramp edge and a plateau neighbour
  never cross strictly inside a half-edge, so one wall per half-edge suffices.
- **R3 (neighbour surface).** The neighbour's surface level along a half-edge comes from one function shared by
  both passes, `dual_floor_level_beside(cell, quadrant, edge)`, built on the same per-threshold classification
  `emit_dual_cell` uses (R4). Both passes then agree by construction; no tolerance hides a disagreement.

### 3.2 Marching classification of ramp corners

- **R4.** In a dual cell, a ramp corner is classified at threshold `t` from its non-ramp neighbours, never from its
  own height:
  - if one or both of its two edge-adjacent corners are non-ramp, it is high iff **all** of those non-ramp
    edge-adjacent corners are `>= t`;
  - else, if the diagonal corner is non-ramp, it takes the diagonal's classification;
  - else (four ramp corners) it is low, and the dual cell emits nothing (R1).
- **Why this rule.** A marching-squares contour enters the interior of a corner's quadrant only when that corner is
  *lone* (it differs from both edge-adjacent corners). Under R4 a ramp corner always equals one of its edge-adjacent
  corners, so no contour crosses a ramp quadrant; contours only run along ramp edges, where R1 drops them. "All"
  rather than "any" keeps plateau corners square where a plateau meets a ramp's head: at the head's lateral corner
  of a protruding ramp (corners: ramp, landing `L+1`, landing-row neighbour `L+1`, lateral neighbour `L`), "any"
  would chamfer the lateral neighbour's quadrant and leave a 0–0.5 unit step against the ramp edge; "all" yields a
  straight cliff on the landing-row boundary that meets the ramp's side wall in one vertical line.
- **R5 (clipping).** Floor polygons of a dual cell that has a ramp quadrant are clipped to the union of its non-ramp
  quadrants (Sutherland–Hodgman against the quadrant square, fixed-size arrays, no allocation). Dual cells without
  a ramp corner are emitted exactly as today, bit for bit.
- **R6 (walls and rims).** A contour segment of such a dual cell that is straight (it passes the dual-cell centre)
  is split at the centre; a half that lies on a ramp edge is dropped, the other half keeps its own profile and is
  emitted from its midpoint to the centre. A midpoint on a ramp edge gets no rim inset (it counts as suppressed in
  `DualCell::suppressed`, which today holds only `is_ramp_connected`).

### 3.3 Guard rails and diagonal ramps

- Guard rails keep today's rule (on side edges not shared with the same run). A rail beside a flank wall is kept;
  whether it should be is a style question, out of scope (§9).
- Diagonal ramps (not producible by map format v1, kept for legacy) follow R1 to R6 too: R2 is direction-free.
  Today they get only a surface; under this plan they also get their walls.

### 3.4 Effect on `sim_map` and the simulation

None. `RampProfile`, `terrain_altitude_at`, ramp traversal, vision and validation are untouched; no checksum,
golden replay or battle can move. The map data is valid and stays unchanged.

---

## 4. Tests (written first, red on `main`)

New file `crates/client/tests/ramp_geometry_tests.rs` (CPU only, no GPU, no Bevy `App`; `//!` header per C4). It runs
on `showcase_canyon`, `default_arena` and a new synthetic fixture (T0), building every chunk with
`build_chunk_mesh`.

- **T0 fixture.** A synthetic `GameMap` (built in the test, passing `sim_map::validation::validate_map` where the
  case is a valid map) with one ramp of each kind: protruding (lateral `L`), recessed (both lateral `L+1`),
  half-recessed (one side `L+1`, one `L`), beside abyss, beside a plateau two levels up (`L+2`), run lengths 1 to
  `MAX_RAMP_RUN_CELLS`, widths 1 to 4, all four cardinal directions, two runs side by side with different
  `low_level`, and a ramp whose head lies on a chunk border. The fixture map size is not a multiple of
  `CHUNK_DUAL_CELLS`.
- **T1 top surface.** For every ramp cell, at an 8×8 grid of points in the footprint (lateral margin
  `RAIL_WIDTH + 1e-3` so rails are excluded), the highest triangle hit of a vertical ray equals
  `terrain_altitude_at` within `1e-4`. On `main` it is expected to fail on every ramp cell of the upper half (**measured**: it fails on 22 of 36
  showcase_canyon cells, 12 of 16 default_arena cells and 74 fixture cells, exactly the cells past each ramp's middle; the red run is recorded in the
  WP1 hand-back).
- **T2 footprint exclusivity** (the owner's property, extended to walls). No triangle other than the ramp quad of
  that cell and its guard rails has an XZ projection (a triangle or, for a wall, a segment) that meets the open
  footprint shrunk by `1e-3` (laterally by `RAIL_WIDTH + 1e-3`). Ramp-quad triangles are those whose three vertices
  are footprint corners. Intersection by the separating-axis test, written out in the test. On `main` it fails on
  both the floating slab and the hidden floor under the lower half.
- **T3 closure.** For every ramp half-edge whose two sides differ in height (ramp edge vs neighbour surface, R2),
  at three points along the half-edge and at heights strictly between the two surfaces, a horizontal ray across
  the edge meets a vertical triangle within `1e-3` of the edge. Guards against R1 deleting walls without R2
  replacing them.
- **Existing tests.** `terrain_tests.rs` (watertight seams, winding, unit normals, incremental rebuild, capacity
  reuse) must stay green unchanged; `test_ramp_surface_continuous_with_landings` stays as is.
- **Goldens.** `crates/client/tests/mesher_golden_tests.rs` pins `build_chunk_mesh` bit for bit for both shipped maps
  and its synthetic map. All three hashes change in WP1 (cause: ramp corners no longer round into plateau levels;
  slabs, hidden floors and duplicated walls over ramp footprints removed) and the synthetic one again in WP2 (flank
  walls). Each re-record names this plan and the cause in the commit message and in the golden's doc comment
  ("re-recorded at `<sha>`, plan `2026-11-09-ramp-geometry` WPn: …"). The picker golden must **not** change (the
  picker is analytic); if it does, stop and report. Dual cells with no ramp corner must be bit-identical: WP1 adds a
  check that hashes only chunks containing no ramp cell and compares them to the hash recorded before the change.
- **GPU visual check (C21, through `crates/client/tests/support/gpu_run.rs`).** `terrain_preview` gains a fifth
  canonical pose, `ramp`, a close-up of the west ramp of `showcase_canyon` (target (27.5, 9.5), distance 11,
  look-at height 4.5). `test_terrain_preview_renders` asserts it like the other poses and adds one geometric pixel
  check: the projected footprint of ramp cell (27, 9) (projected from the known camera) shows no pixel of the
  plateau's metal-plate layer. **Measured**: the colours are separable on both drivers (about 92% of plateau
  pixels and at most 1.3% of dirt pixels are bright and neutral), but the slab carried the ramp's own dirt splat,
  so the pre-fix render has no metal pixel in the footprint either; the check therefore also requires that more
  than 90% of the footprint is lit dirt, which separates the pre-fix render (52% on radv and lavapipe) from the
  fixed one (99.98% on radv, 100% on lavapipe).

---

## 5. Work Packages and Commit Order

Each implementation agent works in its own worktree (`CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<wt>`),
hands back after `cargo gate quick`; a cheap verifier runs `cargo gate package` (Q0061).

| WP | Commits | Content | Done when |
|---|---|---|---|
| WP1 | (a) tests: `ramp_geometry_tests.rs` with T0–T3 and the ramp-free-chunk hash, T1 and T2 marked by expecting failure only through a red run recorded in the hand-back (no `#[ignore]` lands on `main`; the tests land together with the fix in one merge); (b) fix: R1, R4, R5, R6 in `mesher.rs`, side walls and abyss walls of R2 via the new half-edge loop, R3 shared function; (c) goldens re-recorded with cause | the fix of the owner's bug | T1, T2 green on both shipped maps and on the protruding, abyss and run-length cases of T0; T3 green; ramp-free chunk hash unchanged; `terrain_tests` green |
| WP2 | (a) T0 recessed, half-recessed and `L+2` cases enabled; (b) flank walls and ramp–ramp walls of R2; (c) synthetic golden re-recorded | recessed ramps | T0 complete and green |
| WP3 | (a) `terrain_preview` pose `ramp` and its assertions; (b) docs: `DESIGN.md` Terrain row (evidence column gains `ramp_geometry_tests.rs`), module doc of `mesher.rs` (R1–R6 as its ramp invariant), plan row `Done` with hashes, evidence file `docs/evidence/2026-11-09-ramp-geometry.md` with before and after stills | visual check and docs | GPU test green on radv and lavapipe; evidence shows the before still and the after render of the same pose |

Order: WP1 → WP2 → WP3. WP1 alone fixes everything visible on the shipped maps (D-7).

**Showcase re-render check (WP3).** Render the `battle` take locally on the RX 580
(`showcase take`, through `showcase render --driver radv` which takes the GPU lock per take, charter §7) and on
`lavapipe` (`WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`), cut the stills at 7.5 s
and 9 s as in §2.1, and put both pairs (before/after) in the evidence file. The published video updates by itself
after the next green CI on `main` (`.github/workflows/showcase.yml`); WP3 checks the new video once it is deployed.

---

## 6. Effect on Other Crates

- `sim_types`, `sim_map`, `sim_core`: none (§3.4). The orchestrator's merge check must show no file under those crates.
- `client_diagnostics`: none in code; the showcase output changes in pixels only. `showcase_tests` compares two
  renders of the same build with each other, so it is unaffected. `frame_budget` may move slightly (fewer hidden
  triangles); no budget assertion is expected to trip (verified by the package gate, which the verifier agent
  runs under Q0061; the implementation ran `quick` and the targeted client tests, §10).
- `gate`: the new test file is CPU-only and fast, so it needs no `suites.ron` class entry; C22 must stay green.

## 7. Decisions

- **D-1** The ramp owns its footprint and its edges (R1, R2) rather than the dual grid owning everything: the ramp
  is the only cell whose surface is not flat, so only it knows its edge heights exactly.
- **D-2** Ramp corners are classified from their neighbours (R4), not from their height: a height-based level can
  never be right for a cell that spans a whole level.
- **D-3** Clip only dual cells that touch a ramp (R5), so every other dual cell stays bit-identical and the change
  is provably local.
- **D-4** No hidden geometry under a ramp: hidden floors cost overdraw and cause z-fighting near the low edge and
  beside the abyss.
- **D-5** Goldens are re-recorded, never loosened; the ramp-free-chunk hash proves the rest of the terrain did not
  move.
- **D-6** The CPU properties are the arbiter, the GPU pose is the human-visible confirmation.
- **D-7** WP1 alone is a complete fix for the reported bug and the shipped maps; WP2 may be deferred by the
  orchestrator if recorded in this plan's status row.

## 8. Risks

| Risk | Mitigation |
|---|---|
| Clipping or segment splitting opens a crack at a ramp's corner (where ramp side wall, plateau cliff and floor meet) | T3 closure and the existing seam/watertight tests; T0 covers every corner configuration |
| A plateau cliff's jitter or rim pokes into a ramp footprint | Jitter displaces along the wall normal, which for walls ending at a ramp corner is parallel to the ramp edge; T2 would catch it |
| Chunk borders: a ramp edge's half-edges lie in two chunks | R2 is per half-edge in the chunk that contains the ramp cell; R3 is a pure function of the map, so both sides agree; T0 has a ramp head on a chunk border |
| Map editor shows a half-built ramp (invalid landings) | R2's comparison closes any step, so the editor never shows holes |
| Function length (clippy `too_many_lines`, C8) in `emit_dual_cell` | split the ramp-aware branch into its own function |
| Allocation in the rebuild path (`generate_wall_profile` returns a `Vec` today) | unchanged by this plan; the clip uses fixed arrays; `test_rebuild_reuses_capacity` must stay green |

## 9. Out of Scope

- Guard-rail styling beside flank walls, and rail ends at landings.
- `generate_wall_profile`'s per-call `Vec` (existing; a separate performance note if wanted).
- Any change to `sim_map` ramp rules, validation of lateral ramp neighbours, or map data.
- The `round()` semantics elsewhere (none other than `corner_marching_level` found).

---

## 10. Implementation Record (2026-11-09)

Implemented in one package on `main` at `5accec2`; measurements, renders and red runs are in
[`docs/evidence/2026-11-09-ramp-geometry.md`](../evidence/2026-11-09-ramp-geometry.md).

| WP | Commits | Result |
|---|---|---|
| WP1 | `6c3fe8a` tests (red), `3baab60` fix, `aec11e4` goldens | T1/T2 red on `main` (22/36, 12/16 and 74 fixture cells off; 33/36, 16/16 and 108 cells intruded), green after the fix; T3 green throughout; ramp-free chunk hash unchanged; picker hashes unchanged |
| WP2 | `9eab60d` tests (red), `ef26242` fix, `41f502c` synthetic golden | T3 on the full fixture: 470 open steps on 32 cells, then 0; shipped-map goldens unchanged |
| WP3 | `1105091` GPU pose, `cda8ff6` catalog, this commit (docs) | `ramp` pose green through the GPU harness on radv and lavapipe |

Golden hashes: `default_arena` mesh `0xd02323bb3431300e` → `0x0f0d39ef7f9758db`, `showcase_canyon`
`0x2b6a057b898da597` → `0x771ccfc198c0befd`, synthetic `0x90266ce659f753a4` → `0xe2a178698ccc7174` (WP1) →
`0xce785e16375bbd4c` (WP2); every pick hash and hit count unchanged.

Decisions taken during implementation (scoping is the agent's call if written down, charter §8, Q0002):

- **D-8** R4's classification reduces to one integer level per ramp corner (`dual_corner_levels`: the lowest
  non-ramp edge-adjacent level, else the diagonal's), so the existing threshold runs, rims and walls apply
  unchanged. R3's level beside a half-edge is the median of the neighbour's corner and its two edge-adjacent
  corners, which is the marching region at the dual cell's centre in every case (a lone corner's quadrant is cut,
  so the centre side follows its neighbours).
- **D-9** R5 cuts the union of non-ramp quadrants into convex pieces that keep whole dual-cell edges where both
  corners are non-ramp (one quadrant, a half, two diagonal quadrants, or two halves split along the diagonal
  through the ramp corner), so clipping adds no vertex on an edge a ramp-free neighbour meshes without one; the
  chunk-seam test stays bit-exact. Fan triangles below `1e-6` doubled area, which only clipping can leave, are
  skipped.
- **D-10** The kept half of a split straight contour ends at the dual cell's centre with no rim, foot or jitter;
  the centre is inserted into the floor polygon between the two opposite midpoints, so the floor edge and the
  wall's top edge meet (a taper where the kept midpoint has a rim, as at any ramp-suppressed midpoint before).
- **D-11** `corner_marching_level`, `is_ramp_connected`, `ramp_step_2d` and `to_cell_coords` are removed: they
  encoded the rounding rule and the old suppression, and nothing else used them.
- **D-12** The ramp-free chunk hash lives in `mesher_golden_tests.rs` (`ramp_free_chunks_are_bit_exact`), next to
  the goldens and their hashing code, rather than in `ramp_geometry_tests.rs`.
- **D-13** The GPU pose's pixel check also requires more than 90 % lit dirt in the footprint: the slab carried the
  ramp's dirt splat, so the metal check alone does not see it (§4).
- **D-14** `ramp_geometry_tests` is not in `crates/gate/suites.ron`: it is `fast` by default, and a catalog edit
  would select the complete suite for this client-only package. A `sim_map` change reaches the ramp geometry
  through `client/mesher_golden_tests`.
