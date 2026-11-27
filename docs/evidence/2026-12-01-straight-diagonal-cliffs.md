# Evidence: Straight Diagonal Cliffs on the High Side (B-246, Q0136 (c), Q0139)

Measurements of plan [`2026-12-01-straight-diagonal-cliffs`](../plans/2026-12-01-straight-diagonal-cliffs.md): the
base of record, the WP0 measurements, the red runs of every new test, and (from WP1 on) the green runs, the golden
re-records and the review captures. Scratch material (logs, captures, the scratch mesher check) is under
`~/scratch/straight-diagonals/`; none of it is committed.

## 1. WP0

### 1.1 Base of record

- `main` at `b2ba4621`, branch `diagonal-cliffs` in its worktree.
- WP0 changes no drawn terrain: `mesher::full_cut` is a stub that holds nowhere and `mesher::chamfer_mitre` returns
  today's square-nook mitre `n₁ + n₂`, which no production path calls yet. The mesher goldens pass unchanged.

### 1.2 The scratch mesher check (WP0 step 1)

`~/scratch/straight-diagonals/mesher_check/` calls the client's `dual_corner_levels` and the production `full_cut`,
and the independent D-1 analysis of `crates/client/tests/support/terrain_mesh.rs`, over every cell, corner and
threshold. Output `result_wp0.txt`, the planning table of §1.5 measured:

| | `showcase_canyon` | `default_arena` | synthetic | diagonal fixture |
|---|---:|---:|---:|---:|
| convex corner instances (cell, corner, threshold) | 236 | 20 | 413 | 263 |
| full cuts (instances) | 24 | 0 | 11 | 105 |
| cut cells / runs / void runs | 24 / 2 / 4 | 0 / 0 / 0 | 7 / 7 / 2 | 105 / 14 / 3 |
| rectangle corners, half cut kept | 32 | 20 | 242 | 86 |
| void-only run corners, half cut kept | 180 | 0 | 154 | 48 |
| fallbacks: opposite corner not interior / ramp in the 3×3 block | 0 / 0 | 0 / 0 | 1 / 5 | 23 / 1 |
| concave dual cells: ground, 2 cuts through / 1 / none | 22 / 0 / 2 | 0 | 0 / 7 / 32 | 91 / 12 / 13 |
| concave dual cells: void, cuts through / none | 0 / 86 | 0 | 5 / 189 | 1 / 35 |

The planning figures of `showcase_canyon` hold exactly. Its four cut centres on a chunk seam are, in `GameMap`
coordinates, `(24, 15)`, `(15, 24)`, `(47, 40)` and `(40, 47)`: the plan's analysis read the map file's rows top
down, so its list is the same four cells mirrored. The synthetic map has seven cut cells, so its mesher hash moves in
WP2 as well as in WP1. The fixture's 23 "opposite corner not interior" corners are its one- and two-cell ridges
(D-7).

### 1.3 The diagonal fixture

`support/terrain_mesh.rs::diagonal_fixture`, 64×48, holds every row of plan §4.0 in its own box (the layout is
documented on `DIAGONAL_FIXTURE_ROWS`). One change from a first layout: the run whose low side changes from void to
ground (row 10) was moved one cell east, off the chunk seam `x = 31.5`. On that seam, a column of abyss beside two
plateau levels made `main`'s mesher put a hidden level-0 floor corner under the level-1 plateau on one side of the
seam only, so the seam comparison of `terrain_tests.rs` failed on `main` for a reason that has nothing to do with
diagonals (the comparison's own documentation describes the same effect for the map's edge dual cells).

### 1.4 Red runs on the WP0 tree

Logs under `~/scratch/straight-diagonals/red/`.

| Test | Result on WP0 | Why |
|---|---|---|
| T-S1 `diagonal_runs_are_straight_showcase_canyon`, `_diagonal_fixture` | red | the sawtooth's toes stand up to `0.354` beyond the line through the low cells' corners (largest deviation `0.462` on every ground run); the void runs' square nooks put rows on axis lines (`0.502`) |
| T-S1 `diagonal_runs_are_straight_default_arena` | green | no runs (asserted) |
| T-S2 `full_cut_follows_the_block` (unit) | red | the stub holds nowhere: the run cell, the run end and the saddle expect a cut |
| T-S2 `full_cut_holds_once_per_cell_and_never_through_a_saddle` | red | the stub disagrees with the analysis on all 105 fixture cuts |
| T-A3 `junction_rows_stay_behind_both_lines` (unit) | red | the stub mitre `n₁ + n₂` meets a chamfer's normal at 2, not 1 (40 of 1,171,968 rows, toes or mitres) |
| T-S3 `chamfer_floors_and_walls_share_their_edges_*` | red | no floor vertex on any cut's rim-top line; on the fixture, floor corners at cut centres on the foot are no wall vertex |
| T-S4 `chunk_seams_through_a_run_are_watertight` | green (guard) | |
| T-K `brink_drop_under_a_body_is_bounded_*` | green (guard) | see §1.5 |
| T-B1, T-B2 `..._diagonal_fixture` | green | `main` draws no rock inside a walkable cell of the fixture |

### 1.5 T-K on `main`, and its sampling rule

The plan states that T-K passes on `main` with an uncut maximum of `0.077`. Measured with the sampling rule as
written (every lattice centre that keeps its terrain radius from every cliff edge), it does not: at every **convex**
corner whose low side is ground, a body at clearance 0 from both edges stands over the foot of the 45° half cut,
`2.84` below its plateau for `r = 0.3125` (`5.84` on a two-level cliff of the fixture). That half cut is the mesher's
convex-corner drawing since plan 21b and is not touched by this plan; the terrain plan's brink limit was measured on
straight walls (its scratch check computes `RIM_DROP·(0.37 − r)/0.15`). T-K therefore reports the quadrants an uncut
cell's convex half cut draws (a lone high corner of their dual cell) beside the limit instead of holding them to it,
and holds every other uncut sample to `0.2`. On `main` (WP0 tree), per terrain radius:

| Map | `r` | uncut (limit 0.2) | cut cells | convex half-cut quadrants (reported) |
|---|---:|---:|---:|---:|
| `showcase_canyon` | 0.3125 | 0.1600 | 2.8409 | 5.8409 |
| | 0.375 | 0.0767 | 0.1600 | 0.1600 |
| | ≥ 0.5 | 0 | 0 | 0 |
| fixture | 0.3125 | 0.1931 | 3.0000 | 5.8409 |
| | 0.375 | 0.0767 | 0.1600 | 3.0000 (beside the ramp of row 12) |
| | ≥ 0.5 | 0 | 0 | 0 |

The catalog's ground unit spheres: 28 archetypes in 7 terrain radii (3 at 0.3125, 13 at 0.375, 8 at 0.5, one each at
0.5625, 0.75 and 0.875, one at 1.0), as plan D-8 lists.

## 2. WP1: void diagonals through the midpoints again (D-6)

Commits `a786f902` (code), `ba9c159f` (golden re-record). Logs `~/scratch/straight-diagonals/logs/wp1/`.

- T-S1's void runs are straight: largest deviation `9.7e-7` (τ `1.22e-4`) on the 4 void runs of `showcase_canyon`
  and the 3 of the fixture. The void runs' continuity bound is `1/√2 + √2·0.06 = 0.792`, not `1/√2`: the two profiles
  of one 45° cut stand at its midpoints, each moved by the void setback along its own axis, so consecutive bottom
  rows are `0.622` and `0.792` apart along the line (derived, recorded as a deviation of T-S1).
- T-B1 and T-B2 green on five maps; `terrain_tests` (14, including the 4×4 counts, the seams and the preview on radv)
  and `ramp_geometry_tests` green.
- Mesher goldens: `showcase_canyon` mesh `0x2fab86d971733ced` → `0x97118e48b41d7893`, ramp-free chunks
  `0x9ade734dd7dd651c` → `0x051496086fec6fa6`; synthetic mesh `0x45d30dce9779890c` → `0x029596f22649d218`, ramp-free
  chunk `0x7155c56ab6ce594b` → `0x4d48d3b0e0ad5713`. `default_arena` (`0x8b6b69ceed8bcf97`,
  `0xed16db8a30059794`) and every pick hash and hit count unchanged.
- Review capture `~/scratch/straight-diagonals/review/contact_sheet_wp1.png`, column `diagonals_wp1/`: the abyss
  canyons are straight again; the ground diagonals and the player-0 close-ups are identical to `main`'s, pixel for
  pixel.

## 3. WP2: straight chamfers on the high side (D-1 to D-5, D-7, D-9)

Commits `1fe78950` (code), `058435c4` (cheaper cut reads, same bits), `8649ce2d` (golden re-record). Logs
`~/scratch/straight-diagonals/logs/{wp2,wp2b}/`.

### 3.1 Tests

| Test | Result |
|---|---|
| T-S1 (three maps) | green; largest chamfer deviation `2.7e-6` on 16 ground runs, `9.7e-7` on 7 void runs, τ `1.22e-4` |
| T-S2 (unit table, fixture half) | green; the production predicate agrees with the independent analysis on every fixture cell, one direction per cell, no foot end at a saddle; the scratch check finds no disagreement on any map |
| T-A3 | green; every junction mitre meets both normals at 1 within `8·ulp(2)`; the square nook's mitre is `d₁ + d₂` bit for bit |
| T-S3 | green; 552 (`showcase_canyon`) and 2,418 (fixture) floor vertices on a cut's rim-top or foot line, each a wall vertex bit for bit; no floor triangle of a touched dual cell turned over |
| T-S4 | green |
| T-K | green, figures in §4 |
| T-B1, T-B2 (five maps), T-A, T-A2 | green |
| `terrain_tests` (4×4 counts, seams with four cut centres on `showcase_canyon`'s seams, incremental rebuild, winding, normals, preview on radv) | green |
| `ramp_geometry_tests`, G-1 `cliff_contact_gpu_tests` (radv: band 3184/3184 changed), `smog_golden_tests`, `round2_visual_tests`, `effect_slice_gpu_tests` (effect stills) | green, unchanged |

### 3.2 What the implementation needed beyond the plan's text

- **The wall between the two crossing columns is the `O` dual cell's.** With the centre column in both `P` dual
  cells' strips, a cut centre on a chunk seam put three vertices on one side of the seam at the centre (the `C` dual
  cell's floor corner, a `P` dual cell's floor corner and its strip's toe) and two on the other, and the unchanged
  `test_chunk_borders_are_watertight` compares vertex multisets. The `O` dual cell now emits the strip crossing
  column → centre column → crossing column once, with shared middle vertices, and each `P` dual cell's strip starts at
  its crossing column. Every dual cell round the centre then holds one vertex there. Crossing columns exist at every
  cut (at the rim-top line of the cut run's top), except where two cuts meet at the 90° apex.
- **The low region is fanned from the low cell's corner.** A low region with a chamfer's triangle added is
  star-shaped from `L`'s corner, not always from the table's first vertex (T-S3 caught turned-over fan triangles at
  every run end on a straight wall).
- **Flat dual cells read 4×4 too, behind a filter** (D-9 said they would not pay). A flat dual cell can be a cut's
  `O`, whose floor must stop at the rim top. It first reads its corners' 8 outward edge neighbours and skips the 4×4
  read unless some corner has two lower, non-ramp ones.

### 3.3 Cost (D-9)

Full mesh build, scratch check at opt-level 2, best of 300, WP1 tree → final tree: `showcase_canyon` 4.93 → 5.90 ms
(1.20×), `default_arena` 4.39 → 4.90 ms, synthetic 3.86 → 4.54 ms, fixture 4.67 → 5.85 ms. Before the filter of
`058435c4` the figures were 1.66× and 1.74×. Nothing new allocates: `DualCuts`, `Refs` and `FloorPolygon` are on the
stack; the profile `Vec`s are the rebuild path's, as before.

### 3.4 Goldens

`showcase_canyon` mesh `0x97118e48b41d7893` → `0x7d2d666737296669`, ramp-free chunks `0x051496086fec6fa6` →
`0x70700bdb731bc5d3`; synthetic mesh `0x029596f22649d218` → `0xc7cf28f9598ea15e`, ramp-free chunk
`0x4d48d3b0e0ad5713` → `0xebb43f3f7c63f8e8`. `default_arena` and every pick hash and hit count unchanged.

## 4. Review capture and the trade-off (D-8, Q0139)

`~/scratch/straight-diagonals/review/` (`README.txt` names every file and commit): `contact_sheet.png` has the
columns `main_before`, `wp1` (the terrain plan's), `main` and `diagonals` (WP2), the rows of the terrain plan's wp3
sheet, the brink rows `brink_0.3125.png` and `brink_0.5.png`, and `straightness_overlay.png`.

**High ground drawn below its surface** (walkable plateau more than 0.01 below its level, 1/32 lattice): within a
cell of a cut on `showcase_canyon`, 19.36 cells under (a) and 25.65 under (c), +0.262 per step (plan: 18.7 → 24.6,
+0.243).

**T-K, the drop under a body's centre** (final tree; `main` in brackets):

| `r` | Archetypes | Cut cells | Uncut cells (limit 0.2) |
|---:|---:|---:|---:|
| 0.3125 | 3 | 3.0000 [2.84] | 0.1987 [0.160] |
| 0.375 | 13 | 3.0000 [0.160] | 0.1398 [0.077] |
| 0.5 | 8 | 3.0000 [0] | 0.0219 [0] |
| 0.5625 | 1 | 2.8875 [0] | 0 |
| 0.75 | 1 | 0.0219 [0] | 0 |
| 0.875, 1.0 | 2 | 0 | 0 |

The cut-cell figures are plan D-8's table. The largest uncut drop, 0.1987, is a backing cell's corner at a concave
corner `V`, where the rim band dips into it, at the radius' distance from `V`. The convex half-cut quadrants, reported
not limited (§1.5), are unchanged.

**Acceptance 5:** the terrain plan's scratch mesher check, re-pointed at the final tree
(`~/scratch/straight-diagonals/terrain_mesher_check/result_final.txt`), counts 0 rows on the low side of their edge
on both shipped maps.
