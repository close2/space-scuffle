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
