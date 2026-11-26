# Blueprint: Straight Diagonal Cliffs on the High Side (Q0136 (c); B-246)

The owner answered [Q0136](../questions/closed/0136-concave-cliff-corners.md) with **option (c)**: keep the
straight 45° diagonal cliff look and draw each diagonal through the low cells' corners, so that it lies entirely on
high ground. Plan [`2026-11-30-terrain-and-air-indicators`](2026-11-30-terrain-and-air-indicators.md) (the
"terrain plan" below) built the default (a), square inner corners. Its WP3 (`6c47fc4d`, goldens `aca9dc86`) is on
`main`, and it draws every 45° staircase as a sawtooth of chamfered teeth and square nooks. This plan replaces that
look on diagonals and keeps the terrain plan's guarantees.

- **B-246 (new, provisional):** since the terrain plan's WP3, a 45° staircase cliff draws as a toothed staircase,
  not the straight diagonal of plan 21b §5.3, the look the owner chose in Q0136 (c). The void-side diagonals, where
  no cell is walkable, are toothed as well, with no W1 reason (§1.2).

The design in one paragraph. On a staircase, the only straight line that enters no low cell runs through the
staircase's concave corners, the low cells' corners. It is the anti-diagonal of every high step cell, and it
passes through that cell's centre (§1.3). A **full cut** of a convex corner `C` of high cell `X` replaces the
half-cell 45° cut of today with a cut along `X`'s anti-diagonal, between `X`'s two corners next to `C`. A cut is
made only where it continues a diagonal: at least one of those two corners must be a concave corner with ground
below. It is also made only where `X` is backed by plateau, so a ridge one or two cells wide and any cell beside a
ramp keep today's drawing (D-1). A chain of full cuts is one straight chamfer. Its foot lies on the line through
the nooks, and its wall, rim and jitter stand behind that line with the profile of the terrain plan's D-1,
translated along the diagonal normal. Concave corners with no cut through them stay square (WP3). Void diagonals
return to the 45° line through the edge midpoints that they had before WP3, because no walkable cell lies beside
them (D-6). The decision for a cut reads a 3×3 block of cells, so a dual cell reads a 4×4 block, still bounded and
local. W1 holds by construction: a full cut removes rock from its own high cell only. The cost is stated in
figures (D-8). Half of every step cell on a ground diagonal is drawn at the low floor's height, so a ground unit at
the step's outer corner, or anywhere on that cell's outer half, is drawn hanging over the cliff by up to the full
cliff height. The terrain plan's D-4 brink check (limit 0.2, today 0.077) cannot hold there for any straight
diagonal on the high side. The owner's option text names this ("units on the high plateau stand up to half a cell
over the drawn edge"), and draft question **Q0138** (§13) asks the owner to confirm its measured size.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `0c16a3a9` on branch
`diagonal-cliffs-plan`. It is a full plan, because it changes cliff look across maps and spans two work packages.

- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** The new bug row is **B-246** (`DESIGN.md` §4 ends at B-245, and no
  open branch names B-246). The draft question is **Q0138**. Branch `wall-clearance-wp1` names Q0138 as the number
  its own draft would take "if it is ever filed", so whichever question is filed first takes Q0138 and the other
  takes the next free number.
- **No simulation change.** No file under `crates/sim_*` or `assets/` changes (§6). A change there is a stop (§9).

---

## Contents

1. Diagnosis
2. Requirements and charter mapping
3. Design decisions
4. Tests
5. Goldens and the owner's review capture
6. Proof that the simulation does not change
7. Acceptance
8. Work packages
9. Stop rules
10. Files
11. Out of scope and recorded decisions
12. Bug row
13. Question draft

---

## 1. Diagnosis

### 1.1 What `main` draws

The mesher (`crates/client/src/terrain/mesher.rs`) is the dual-grid marching squares of plan 21b §5.3. Dual cell
`(di, dj)` is centred on the shared corner of four cells, and its corners are their centres. For each threshold run
`ta..=tb` it looks up `MARCHING_CASES[case]` from the four corner levels (`dual_corner_levels`) and emits three
things:

- the high regions (`regions`), with contour vertices inset by `floor_inset`;
- the walls along `segments`, from `generate_wall_profile` rows set back by `edge_setback` (terrain plan D-1:
  `FOOT_WIDTH` = 0.22 on a ground edge, `WALL_JITTER_AMPLITUDE` = 0.06 on a void edge, 0 on a ramp edge, with the
  toe on the edge);
- the low region (`low_regions`) at `H(lo)`.

The terrain plan's WP3 changed the table:

| Corner | Cases | `main` (WP3) | Before WP3 (21b) |
|---|---|---|---|
| lone high (convex) | 1, 2, 4, 8 | 45° cut through the two edge midpoints (`CUT_BL` …), removing 1/8 of the high cell | same |
| lone low (concave) | 7, 11, 13, 14 | square nook: `nook(start, end)`, contour through `Centre`, `mitred_corner_profile` with `ProfileFrame.offset = d₁ + d₂`, L region star-shaped from the corner diagonal to the nook | 45° cut through the midpoints, entering the low cell up to 0.354 (B-245) |
| saddle | 5, 10 | low-connected: two convex cuts | high-connected hexagon |

Every decision reads only the dual cell's own four corners (2×2 cells). A 45° staircase alternates lone-high and
lone-low dual cells. Each step therefore draws as a 45° tooth (0.707 long), two half-edges (0.5 each) and a square
nook. That is the sawtooth of `~/scratch/terrain-indicators/review/wp3/contact_sheet.png`, column 3. It shows on the
level-1/2 diagonals of `showcase_canyon` (rows 0 to 25 and 38 to 49 of `levels`) and also on the two diagonal abyss
canyons (columns 0 to 23 of rows 0 to 23, and the mirror), where every low cell is void.

### 1.2 The void diagonals did not need the change

W1 constrains walkable cells only. The nook of a lone-low corner whose low cell is void lies in the void. The
terrain plan's T-B2 heat map (`tb2_heatmap_wp1.png`) counts no violation on the canyons for that reason. WP3
nevertheless squares every lone-low case, void or not. So the canyons lost their straight look for no W1 gain. On
`showcase_canyon` 86 of the 110 concave dual cells are void-side (§1.5).

### 1.3 Why option (c) is one line, and why it cannot be drawn from one dual cell

Take a staircase whose high cells are `i + j ≤ n` and whose low cells are `i + j ≥ n + 1`. Cell `(i, j)` spans
`[i, i+1] × [j, j+1]`. Its boundary vertices alternate:

- convex corners `C` of the high region, at `x + y = n + 2` (the outer corner of each step cell);
- concave corners `V`, at `x + y = n + 1` (a lone low corner, the lower-left corner of a low cell).

**Claim: `x + y = n + 1` is the only straight line in the run's direction that enters no low cell. Proof:** every
`V` lies on the boundary of a low cell, so a line `x + y = c` with `c > n + 1` passes inside the low cell at `V`.
For `c ≤ n + 1`, the open half-plane `x + y < c` contains no point of any low cell, because a low cell's points
satisfy `x + y ≥ n + 1`. The line `c = n + 1` is the one closest to the simulation's boundary. ∎

That line is the **anti-diagonal** of each step cell `X = (i, j)`, from `(i, j+1)` to `(i+1, j)`, and it passes
through `X`'s centre. In dual-grid terms, the cut of `X` at `C`:

- crosses three dual cells: the one at `C`, where `X`'s quadrant lies wholly beyond the line, and the ones at `X`'s
  two corners next to `C`, called `P₁` and `P₂`, each of which holds half the cut, from `X`'s centre (a dual
  corner) to its own centre;
- has a wall band behind the foot (`FOOT_WIDTH + RIM_WIDTH` = 0.37 deep) that reaches the dual cell at `X`'s
  opposite corner `O`. A point of the band at depth `δ ≤ 0.37` lies within `δ/√2 ≤ 0.262` of the anti-diagonal on
  each axis, so the band lies in the dual cells at `P₁`, `P₂` and `O` and nowhere else.

Whether `X` is cut depends on `X`'s eight neighbours (§3 D-1), so a dual cell needs the cells of the 3×3 blocks
around its four corner cells: a **4×4 block**. Q0136 said "a wider rule than one dual cell". This is that rule, and
it stays bounded and local.

### 1.4 What a straight line costs on the high side

The foot runs through the centre of every step cell. Behind it, the wall reaches its rim top 0.37 deeper, at
`x + y = n + 1 − 0.37·√2`, which is local `u + v = 0.477` in `X`. So 88.6 % of each step cell is drawn below its
plateau: half is drawn at the low floor, and the rest is drawn as wall and rim. The simulation lets a body stand
anywhere in `X` at clearance 0 from its two cliff edges, so a body's centre can lie over the drawn low floor. §3 D-8
gives the figures. No straight diagonal on the high side avoids this, by the claim of §1.3.

### 1.5 Counts on the shipped maps

From a scratch analysis (`~/scratch/straight-diagonals/analysis/cuts.py`, not committed). It applies D-1's
predicate to `levels`, `terrain` and `ramps` of each map file, so its figures are planning figures. WP0 re-measures
them with the production mesher's functions.

| | `showcase_canyon` | `default_arena` |
|---|---:|---:|
| convex corner instances (cell, corner, threshold) | 236 | 20 |
| full cuts (D-1) | **24** (two runs of 12 at t = 2; 20 between two concave corners, 4 run ends at a straight wall) | 0 |
| rectangle corners, half cut kept | 32 | 20 |
| corners of void-only runs, `main`'s half cut kept (D-6) | 180 | 0 |
| fallbacks: corner opposite `C` not interior / ramp in the 3×3 block | 0 / 0 | 0 / 0 |
| concave dual cells: ground with 2 cuts through / ground with none (square) / void | 22 / 2 / 86 | 0 |
| full-cut cell centres on a chunk seam (`x` or `y` ≡ 15.5 mod 16) | 4: `(15,39)`, `(23,47)`, `(39,15)`, `(47,23)` | 0 |

`default_arena` has no concave corner and no run, so its mesh must not change (§5.1). No shipped diagonal comes
within a 3×3 block of a ramp. The goldens' synthetic map and a new diagonal fixture (§4) cover that case.

---

## 2. Requirements and charter mapping

| Requirement | Where met |
|---|---|
| Diagnose the current mesher and plan 21b §5.3 | §1 |
| Diagonals and saddles become straight chamfers whose foot passes through the low cells' corners | D-1, D-2 |
| No drawn rock in a walkable cell: W1, `cliff_boundary_tests.rs` T-B1, T-B2 with their derived τ, ε | D-1 (cut is inside `X`), D-3, D-4; T-B on five maps |
| Keep T-A, the seams test, the 4×4 counts, the star polygon, no mitre beside a ramp, `dual_floor_level_beside` | D-3 (T-A's monotone argument extended), D-5, D-7; T-C unchanged |
| Isolated inner corners, saddles, diagonals meeting ramps or narrow plateaus | D-2 table, D-7 |
| Trade-off in high-ground area and brink overhang, measured like D-4 (limit 0.2, now 0.077) | D-8, T-K, Q0138 |
| Red-first CPU test that runs are straight, with a derived tolerance | T-S1 |
| Mesher golden re-records with reasons | §5.1 |
| Review capture under `~/scratch/straight-diagonals/review/`, four columns, physical GPU through `with-gpu-lock` | §5.2 |
| No simulation change, proved | §6 |
| Charter §1.2 (real assets in tests), §2 (docs, short functions), §3.3 (no steady-state allocation), §7 (GPU lock) | T-K loads the real catalog; D-9; §5.2 |

---

## 3. Design decisions

Notation. At threshold `t`, a cell is **high** if its marching level (`dual_corner_levels`' rule, void −1) is
`≥ t`, else **low**. For a cell `X` and a corner direction `(dx, dy) ∈ {±1}²`, `C` is `X`'s corner in that direction,
`L₁ = X + (dx, 0)`, `L₂ = X + (0, dy)`, `K = X + (dx, dy)` (diagonal through `C`), `P₁` is `X`'s corner shared with
`L₁` other than `C`, `P₂` the one shared with `L₂`, and `O` is `X`'s corner opposite `C`. The unit normal
`n = (dx, dy)/√2` points from `X` towards `C`, the low side.

### D-1. The full cut, a predicate on the 3×3 block around `X`

`full_cut(map, X, (dx, dy), t)` is true when all of these hold:

1. **Convex:** `X` is high, and `L₁` and `L₂` are low (`K` may be high: a saddle).
2. **Backed:** `O` is interior, meaning the cells `X − (dx, 0)`, `X − (0, dy)` and `X − (dx, dy)` are high. This keeps
   the half of `X` behind the cut as plateau. It also rules out a second cut in the same cell, because a second
   convex corner of `X` would need one of those cells low. So a cell has at most one full cut.
3. **Continues a diagonal:** `P₁` is a ground concave corner (`X + (dx, −dy)` high, and `L₁`'s level `≥ 0`), or `P₂`
   is one (`X + (−dx, dy)` high, `L₂ ≥ 0`). With neither, `C` is a rectangle's corner and keeps the half cut.
4. **No ramp:** no cell of the 3×3 block around `X` is a ramp. This is a simple rule, and it keeps every dual cell
   with a ramp corner exactly as `main` draws it (D-7).

It is a pure function of nine cells' surface classes, so every dual cell that asks about the same `(X, C, t)` gets
the same answer. Chunks and stacked runs then agree by construction, the same argument as the terrain plan's
setback. Its **cut run** is the maximal interval of `t` over which it holds. Every threshold in it satisfies
`max(L₁, L₂) < t ≤ level(X)`, and the interval is computed from the same nine cells.

The cut replaces, at threshold `t`, the triangle `C, P₁, P₂` of `X`, which `X` alone covers, with a chamfer whose
foot is the segment `P₂ → P₁`. **W1 holds by construction:** the chamfer's foot and everything behind it lie in
`X`'s closure or behind its edges (D-3, D-4), and the region it gives to the low side is part of `X`, drawn lower
than `X`'s surface.

### D-2. What each dual cell draws

An **effective case** for a dual cell at threshold `t` is the table entry modified by the cuts that touch it. It is
built on the stack (`EffectiveCase`, D-9) only for a dual cell that some cut touches or whose lone low corner is
void. Every other dual cell uses `MARCHING_CASES` unchanged. The table has one row per kind of corner point:

| Dual cell centred on | At threshold `t` | Drawing |
|---|---|---|
| `C` of a cut `X` | lone-high `X`, or a saddle | `X`'s bit cleared: lone-high → case 0 (all low); saddle → the other high corner's half cut (or its own full cut, if it has one). |
| `P` with two cuts through it | ground lone-low (a `V` inside a run) | contour `corner(X₁) → centre → corner(X₂)`, one straight line (both cuts share `n`, §1.3); high region the triangle on the far side of it; low region the rest. The centre's profile has offset `n`. |
| `P` with one cut through it | ground or void lone-low (a `V` where a run meets a long wall) | contour `corner(X) → centre → midpoint` (a chamfer half and an axis half); a junction profile at the centre (D-3); the high region is the WP3 L minus the cut's triangle, a 5-gon with one reflex vertex at the centre, fanned from the corner diagonal to the nook as WP3's L is. |
| `P` with one cut through it | a straight case (3, 6, 9, 12): the end of a run at a straight wall | contour `midpoint → centre → corner(X)`; a convex 135° junction at the centre; the high region is the half quad minus the cut's triangle. |
| `P` with two cuts through it, different `n` | a straight case: two runs meeting at a point of a wall | contour `corner(X₁) → centre → corner(X₂)`; a 90° apex. It does not occur on the shipped maps. |
| `O` of a cut | case 15 (interior) | full quad, clipped by the cut's rim-top line (D-4). |
| ground lone-low, no cut through it | 7, 11, 13, 14 | `main` (WP3): square, mitred, the L star polygon. **Isolated concave corners stay square:** no chamfer of a concave corner can stay out of its low cell. |
| void lone-low, no cut through it, no ramp corner | 7, 11, 13, 14 | the 45° cut through the midpoints, as before WP3, with the void setback (D-6) |
| lone-high, not cut | 1, 2, 4, 8 | `main`: the half cut. This covers rectangle corners, void runs, 1- and 2-wide ridges, and ramps. |
| saddle, neither high corner cut | 5, 10 | `main`: low-connected, two half cuts |

One example of each kind is in the diagonal fixture (§4.0). The dual cells along a run alternate `C` (all low) and
`P = V` (one straight chamfer through the centre). Together with the `O` cells' clipped floors they draw the run as
one straight wall. At a run's end the chamfer meets the straight wall either at a 135° junction on the wall's line
(the shipped case, §1.5) or at a 225° junction at a concave corner. A **one-step notch** (`C V` between two long
walls) draws as one 45° segment `√2` long, which is the diagonal look applied to a single step.

**Saddles** get no special rule. A saddle's high corner is cut when D-1 holds for it. On a diagonal channel between
two plateaus, both sides are cut, and the channel is drawn as the band between the two lines through the low cells'
corners. Otherwise the saddle stays low-connected. No cut passes *through* a saddle point: a cut through a corner
`P` of `X` needs `O` interior, and `O` is one of the saddle's low cells (checked in T-S2).

### D-3. Profiles: one mitre for every junction, the toe on the foot

`ProfileFrame` (`p`, `offset`, `facing`) already carries the mitre. Today's `mitre()` returns `d₁ + d₂` for a square
nook. It becomes the general mitre of two unit low-side normals:

```
m(n₁, n₂) = (n₁ + n₂) / (1 + n₁·n₂)          so that  m·n₁ = m·n₂ = 1
```

That gives `d₁ + d₂` for a square nook (`n₁·n₂ = 0`, unchanged bit for bit, so WP3's corners do not move), `n` for
a straight chamfer, `(n_axis + n)/(1 + 1/√2)` for both 135° and 225° junctions, and `n₁ + n₂` for a 90° apex. A row
at offset `k ≤ 0` lies at `p + m·k`, so its depth behind **both** lines is exactly `−k` in real arithmetic. The
toe (`k = 0`) is `p` itself, bit for bit, as in the terrain plan. `facing = normalize(n₁ + n₂)`, and beside a ramp
the frame stays unmitred (WP3 as-built deviation 4; D-7 makes it unreachable for cuts).

- **The profile at a cut cell's centre** (`X`'s centre, a dual corner shared by the dual cells at `P₁` and `P₂`) has
  frame `(centre(X), n, n)`, jitter keyed by that point, and the **cut's levels**: `la = level(X)` and
  `lb = max(level(L₁), level(L₂))`. These come from the nine cells, so both dual cells build it alike. It is built
  for the whole cut run. A dual cell whose own threshold run is shorter emits the rows of that profile within its
  height range, sampled at its ends with `sample_profile_at_height`, so the column at `X`'s centre has the same row
  set in both dual cells (D-5).
- **The profile at a `P`'s centre** is used only by that dual cell. It has the junction's mitre. Its levels follow
  the cut's levels on the chamfer side, and `corner_levels`' rule (the lower high side) where an axis wall joins.
  **Verify** with T-S4 that the strips meeting at a run end share their union of row heights. If they do not, the
  axis half takes the cut's levels.
- **Setback.** `lb ≥ 0` (D-1 condition 3 makes at least one low side ground), so every chamfer has setback
  `FOOT_WIDTH`, and every canonical row is behind the foot by `0.22 − 0.06 = 0.16` or more. The jitter amplitude
  premise (T-A2) is unchanged.

### D-4. Floors: clipped by the rim-top line, sharing its vertices with the wall

The high floor at `H(t)` must stop at the chamfer's rim top. That is the line `n·x = n·P − (FOOT_WIDTH + RIM_WIDTH)`
when the rim is present (`la == tb`), else `n·x = n·P − FOOT_WIDTH`, as `floor_inset`. It crosses the dual cells at
`P₁`, `P₂` and `O` (§1.3).

1. A touched dual cell builds its effective region polygon as `main` does. Contour vertices on axis walls are inset
   by `floor_inset`, a junction centre is inset along its mitre, and a dual corner on a cut is placed at its exact
   position. The polygon is then clipped with `FloorPolygon::clip` against each rim-top half-plane of the cuts
   that touch the cell: at most two different lines, since a cell has at most one cut (D-1), and a dual cell's
   corners are four cells.
2. **Crossings are computed from the line, not interpolated.** Write the rim-top line as `dx·x + dy·y = c`, with
   `c` computed once per cut by one function (the foot's integer constant minus `√2` times the inset). It crosses
   an axis-aligned dual edge `x = e` at `y = (c − dx·e)·dy`, and `y = e` at `x = (c − dy·e)·dx`. The new helper
   `chamfer_rim_point(cut, axis, e)` computes it, and both dual cells on that edge, in any chunk, compute the same
   bits. `clip` already snaps axis-aligned crossings; the diagonal crossing gets this closed form instead.
3. **No T-junction.** The chamfer strip in a `P` dual cell gets a profile **column** at every point where the
   rim-top line crosses a dual edge between its two ends. The column's rim-top row is that same
   `chamfer_rim_point`, and its other rows are offset from it along `n`. The floor polygons insert the cut's
   rim-top anchors (the end profiles' rim-top rows, from the same function) when they lie on a clipped edge. The
   low floor's edge along the foot inserts each column's toe. So every floor vertex on a rim-top line or on a foot
   is a wall vertex bit for bit (T-S3). Within one cut the crossings are the band's two passages from the `P`
   cells into `O` (§1.3), so a strip has at most three columns.
4. Capacity: a region has at most 6 vertices, and two clips add one each, so the result stays within
   `FLOOR_POLYGON_CAPACITY` (12). The clipped polygons are convex, except the one-cut `V`'s 5-gon. That 5-gon is
   fanned from the corner diagonal to the nook, which sees all of it (checked in planning: every fan triangle has
   positive area). T-S3 checks the fan's area against the polygon's.

### D-5. Seams, stacked runs and the cut run

Three things are shared between dual cells: the profile at `X`'s centre, the rim-top crossings and anchors, and
the toes. Each is a pure function of `(X, C, cut run)` and the nine cells. A touched dual cell groups thresholds by
**effective case**, meaning the case index together with the cuts that touch the cell at each threshold, where
`main` grouped by case index alone. So a run never spans a change in the cuts. Stacked levels need no rule of their
own. At a threshold `t' < t` where `X` is not cut (`L₁` or `L₂` is not low there), the dual cells draw `main`'s
geometry at `t'`, so the part of `X` beyond the cut is drawn at the highest lower floor, which is below `X`'s
surface (W1). The terrain plan's §1.3 rule, that a profile is a property of its edge, extends to the cut. Four
full-cut centres lie on chunk seams of `showcase_canyon` (§1.5), so the unchanged `test_chunk_borders_are_watertight`
covers the shared column. The diagonal fixture adds a seam through a run of a stacked staircase (T-S4).

### D-6. Void diagonals return to the midpoint line (scoping decision)

A lone-low dual cell whose low corner is void, has no cut through it and no ramp corner draws the 45° cut through
its edge midpoints. That is the table row of the WP1 tree (`aaa5d450`, `MARCHING_CASES` cases 7, 11, 13, 14: high
region `NOT_x` reversed, segment midpoint to midpoint), with the void setback of 0.06 at both midpoints. The nook it
covers lies in the void cell, so W1 is untouched (T-B1 and T-B2 ignore void cells). The canyons get back their
straight look along the midpoint line, as on `main` before WP3. A run whose low side changes from void to ground
along its length steps by 0.354 where it changes: the ground part is cut (D-1) and the void part keeps the midpoint
line. This does not occur on the shipped maps (§1.5). **Rejected:** applying the full cut to void runs too. That
would be one uniform rule, but it widens every abyss canyon by `0.707` (two sides × 0.354) and moves the void-side
brink 0.354 further onto the high plateau, for no W1 gain. The rejected option stays in §11, so it can come back.

### D-7. Fallbacks: ramps and narrow plateaus

- **Ramps.** D-1 condition 4 keeps every cut's touched dual cells free of ramp corners, because their corners are
  cells of the 3×3 block. So R1 to R6, the ramp pass, `dual_floor_level_beside` (WP3 deviation 5) and the unmitred
  corner beside a ramp (deviation 4) are untouched. D-6 also excludes dual cells with a ramp corner, so a void nook
  beside a ramp stays square, as R3's rule expects. A run that approaches a ramp ends at the last cut whose block is
  free of ramps. Past it, the uncut step draws `main`'s half cut and square nook. That is a short toothed stretch
  beside the ramp, recorded in §11.
- **One-cell-wide diagonal ridge** (high cells `(i, i)` only; their shared corners are saddles). `O` is never
  interior, so there are no cuts, and the ridge keeps `main`'s half cuts, a chain of tiles joined at points. A cut
  would put both sides' feet on the ridge's centre line and draw nothing of it.
- **Two-cell-wide diagonal band.** `O` is a concave corner, not interior, so there are no cuts and the band stays
  toothed. With cuts, the two feet would lie `1/√2 = 0.707` apart and the rim tops `2·0.37 = 0.74` deep, which is
  more than the distance between them. Bands three cells wide or more are cut, and their drawn top is
  `1.414 − 0.74 = 0.67` wide.
- **One-cell plateau ends and rectangle corners:** D-1 conditions 2 and 3, so the half cut stays.

### D-8. The trade-off, in figures

**High ground drawn below its surface, per staircase step** (one cell along the diagonal, a run's interior). It is
the area between the simulation's boundary and the drawn rim top, with the rim top `FOOT_WIDTH + RIM_WIDTH = 0.37`
behind the foot. For a polygon offset inward by `w`, the band is `L·w`, plus `w²` per reflex corner and minus
`w²·tan(θ/2)` per convex turn `θ`.

| Option | Foot of a ground diagonal | Per step | Of which drawn at the low floor | Drawn inside a low cell |
|---|---|---:|---:|---:|
| (b) 45° through midpoints (WP1 tree) | `x + y = n + 1.5` | 0.523 | 0.125 − 0.125 (C cut, nook) | 0.125 (the nook, B-245) |
| (a) square nooks (`main`) | sawtooth | 0.780 | 0.125 | 0 |
| **(c) this plan** | `x + y = n + 1` | **1.023** | **0.500** | **0** |

On `showcase_canyon` that is about 24.6 cells of high ground on its two ground diagonals (24 cuts), against 18.7
under (a). WP0 measures the exact figure on the mesh. The void canyons go back to (b)'s figure (D-6).

**The brink check (the terrain plan's D-4, limit 0.2, today's maximum 0.077).** A body of terrain radius `r` on a
step cell `X` may stand at clearance 0 from both of `X`'s cliff edges. Its centre is then at local `(1 − r, 1 − r)`.
That is the deepest reachable point, because a body at a concave corner `V` stays `r` behind it. Its signed
distance beyond the foot is `(1 − 2r)/√2`. The drop is the cell's surface minus the drawn surface under the centre:
0 behind the rim top; down the rim chamfer (`RIM_DROP` over `RIM_WIDTH`); down the foot slope (`FOOT_RISE` over
`FOOT_WIDTH`); and the whole cliff, `CLIFF_LEVEL_HEIGHT` = 3.0, beyond the foot. The catalog's ground terrain radii
(`assets/catalog/units.ron`, terrain radius else collision radius, 28 archetypes):

| `r` | Archetypes | Distance beyond the foot | Drop under the centre |
|---:|---:|---:|---:|
| 0.3125 | 3 | +0.265 | **3.0** |
| 0.375 | 13 | +0.177 | **3.0** |
| 0.5 | 8 | 0.000 | **3.0** (on the toe) |
| 0.5625 | 1 | −0.088 | **2.89** (foot slope) |
| 0.75 | 1 | −0.354 | 0.022 (rim chamfer) |
| 0.875, 1.0 | 2 | ≤ −0.530 | 0 |

So for 25 of 28 ground archetypes the drop is above 0.2: such a unit is drawn standing over the low floor, a full
level below. The cell's own centre lies on the foot, so a unit standing in the middle of a step cell already hangs
over the cliff. The claim of §1.3 shows that no straight diagonal on the high side avoids this, and Q0136 (c)'s text
states it ("units on the high plateau stand up to half a cell over the drawn edge"). The plan treats it as follows:

- The 0.2 limit **stays** for every cell that is not cut. T-K checks this on the mesh, and stop rule 6 enforces it.
- In cut cells, the figure is **reported**, not limited: T-K prints the table above, measured from the mesh.
- The owner confirms the size through draft Q0138 (§13). Until then the plan builds (c) as answered.

**On the low side**, no unit is cut by rock (W1). A low unit pressed against a step's outer corner `C` stands
`0.707` from the drawn foot, where today it touches it.

**Picking** (`picking.rs`) raycasts the cell grid at the simulation's heights, not the mesh, so its goldens do not
move. On the low half of a cut cell, a ray from the RTS camera meets `X`'s simulation surface before it meets the
drawn floor, so a click there picks a point about `3.0 / tan 58° = 1.9` world units nearer the camera than the
floor the player sees. That is the same mismatch the nook had under (b), but larger. It is recorded in §11 and needs
no code here.

### D-9. Cost and allocation

The mesher runs on chunk rebuilds, not every frame. A touched dual cell reads its 4×4 block once into a stack
array, `[[i32; 4]; 4]` plus a ramp mask: 16 `surface_class` lookups instead of 4. That happens only for dual cells
whose 2×2 corners hold a lone-high, lone-low, saddle or straight case, so flat ground does not pay. Each threshold
then evaluates at most 16 `(cell, corner)` predicates on that array. `EffectiveCase` and `FloorPolygon` are fixed
size on the stack, so the change adds no heap allocation. The existing `Vec`s of `wall_profile_rows` and
`emit_wall_strip` are the rebuild path's and are unchanged. A `// PERFORMANCE RATIONALE:` block on the 4×4 read
states this (charter §2.3). `sync_steady_state_tests` and `sync_allocation_count_tests` are not affected (no system
changes) and are not re-run beyond the gate. WP2 records `terrain_tests::test_incremental_rebuild_equals_full_rebuild`'s
time before and after. A slowdown above 2× of the full `showcase_canyon` build is a stop (§9).

---

## 4. Tests

Every new test is written in WP0 and fails on `main` for the stated reason, unless it is marked as a guard. Test
names are final unless an amendment changes them.

### 4.0 Shared support

`crates/client/tests/support/terrain_mesh.rs` gains:

- `diagonal_fixture()`: a 64×48 map with one example of each row of D-2's table, all at levels 0 to 2 with abyss
  where named:
  1. A long ground staircase run with both ends on straight walls.
  2. A run ending at a concave corner (the 225° junction).
  3. A one-step notch.
  4. An isolated concave corner, ground.
  5. An isolated concave corner, void.
  6. A rectangle.
  7. A one-cell and a two-cell diagonal ridge, and a three-cell diagonal band.
  8. A diagonal low channel between two plateaus (saddles).
  9. A void canyon.
  10. A run whose low side changes from void to ground.
  11. A two-level stacked staircase, 0/1/2.
  12. A run passing within one cell of a ramp.
  13. Two runs meeting at a wall point (the 90° apex).
  14. A run crossing the chunk seam `x = 15.5` with a cut cell's centre on it.
- `diagonal_runs(map) -> Vec<Run>`: an **independent** re-implementation of D-1, written from this plan and not
  calling the production predicate. A run is a maximal chain of full cuts at one threshold, linked through shared
  ground concave corners. It records the cuts, the foot line `n·x = c` (computed in `f64` from integer corners),
  the cut levels and the run's end kinds. The same module gets `void_runs(map)`: chains of void concave corners
  and their convex neighbours, with the midpoint line.

### 4.1 CPU tests

**T-S1 (integration, new `crates/client/tests/diagonal_cliff_tests.rs`): `diagonal_runs_are_straight_<map>`** for
`showcase_canyon`, `default_arena` (no runs; it asserts there are none) and the diagonal fixture. For each run of
`diagonal_runs`:

- **Corridor `K`:** the points whose position along the run lies within the run's interval shrunk by ½ cell at each
  end, which excludes the junctions, and whose depth `n·x − c` lies in `[−1, 1]`. The test takes every vertex of
  every non-horizontal triangle of `MapMesh` whose three vertices lie in `K` and between `H(lb)` and `H(la)`.
- **The measure, chamfer deviation:**
  - *foot:* every vertex at `y = H(lb)` has `|n·v − c| ≤ τ`;
  - *rim top:* every vertex at `y = H(t)` (rim present) has `|n·v − (c − FOOT_WIDTH − RIM_WIDTH)| ≤ τ`;
  - *face:* every vertex in between lies in `[c − FOOT_WIDTH − RIM_WIDTH − τ, c + τ]`, and every canonical row (a
    vertex with a horizontal normal) has `|n·v − (c − FOOT_WIDTH)| ≤ WALL_JITTER_AMPLITUDE + τ`;
  - *continuity:* the foot vertices' positions along the line leave no gap wider than `1/√2 + τ` (the largest
    column spacing, D-4) across the corridor's interval.
- **Void runs** (`void_runs`) are checked the same way against the midpoint line moved by the void setback:
  - foot at `c_mid − WALL_JITTER_AMPLITUDE/√2` (the bottom rows at `ABYSS_WALL_BOTTOM_Y`);
  - rim top at `c_mid − (WALL_JITTER_AMPLITUDE + RIM_WIDTH)/√2`;
  - the same continuity bound.
- **Tolerance τ, derived:** a vertex coordinate is at most a few `f32` operations on values below `M = max(width,
  height)`. So its projection carries at most `16·ulp(M)`, with `ulp(v) = 2^(⌊log₂ v⌋ − 23)`, the T-B figure: for a
  64-cell map `τ = 1.22e-4`. The line constant is exact in `f64`. The test asserts its premise, that every vertex
  lies within `[0, M]`.
- **Red on `main`:** the sawtooth's axis-wall toes lie at depths from 0 to `+0.707` in the corridor, so the foot
  assertion fails on every ground run. The void runs' square nooks put bottom rows on axis lines, so the void
  assertion fails too.

**T-S2 (unit, `mesher.rs` `#[cfg(test)]`): `full_cut_follows_the_block`.** A table of 3×3 blocks against the
production predicate, with the expected result and reason for each:

- a run cell (two ground `V`s);
- a run end (one `V`, one straight);
- a rectangle corner;
- a void-only run;
- an `O` that is not interior (the 1- and 2-wide ridges);
- a ramp in each of the nine positions;
- a saddle `K`;
- levels that make the cut run `max(L₁, L₂) < t ≤ level(X)`.

It also checks **one cut per cell**: no cell of the fixture has two cuts. And it checks **no cut through a
saddle**: every `P` of every cut has three high cells. It is red on `main` because the predicate does not exist
there; WP0 writes it against a stub that returns `false`, so it compiles and fails.

**T-A3 (unit, `mesher.rs`): `junction_rows_stay_behind_both_lines`.** It extends T-A to the mitre of D-3. It sweeps
the normal pairs: straight `(n, n)`, 135° and 225° junctions, the square nook, and the 90° apex. For each pair it
takes the level and run sweep of T-A (ground and void low corners) and 64 points. For every row and both lines it
asserts `n_i·(row − p) ≤ 4·ulp(M)`, evaluated in `f64` from the `f32` row. For a row other than the toe on a ground
side it asserts `≤ −(FOOT_WIDTH − WALL_JITTER_AMPLITUDE) + 4·ulp(M)`, that is, behind by 0.16. **Tolerance,
derived:** `p + m·k` is one multiply and one add per component on values below `M`, so its rounding is at most
`2·ulp(M)` per component, and the projection onto a unit normal adds at most `√2` times that. The axis-only
pairs keep T-A's exact argument. It also asserts that the toe equals `p` bit for bit. For the square-nook pair it asserts `m == d₁ + d₂` bit for bit, so WP3's
corners do not move. Red on `main`: no general mitre.

**T-S3 (integration, `diagonal_cliff_tests.rs`): `chamfer_floors_and_walls_share_their_edges_<map>`.** For each cut,
it takes every floor vertex within `τ` of a rim-top line or a foot line inside the cut's band and asserts that it is
also a wall vertex, bit for bit, so there is no T-junction (D-4). For every touched dual cell it asserts that the
area of its emitted fan equals the area of its clipped region within `τ·perimeter`. Red on `main`: there are no
chamfers, so it fails on the first run's missing rim-top line.

**T-S4 (integration, `diagonal_cliff_tests.rs`): `chunk_seams_through_a_run_are_watertight`.** This is
`terrain_tests::test_chunk_borders_are_watertight`'s comparison on the diagonal fixture, at every chunk seam. The
fixture's run 14 puts a cut centre on `x = 15.5`, and run 11 stacks two levels across the seam. Red on `main`: it is
not red, because `main` is watertight. **It is a guard** that must stay green through WP2.

**T-K (integration, `diagonal_cliff_tests.rs`): `brink_drop_under_a_body_is_bounded_<map>`.** This is a guard, and
it reports the figures of D-8. It reads every ground archetype's terrain radius (terrain radius, else collision
radius) from the real catalog through the runtime loader, as `runtime_catalog_tests.rs` does (charter §3.3, C6, no
slug named). For each distinct radius `r`, and each walkable plateau cell of `showcase_canyon` and the fixture, it
samples centres on a 1/16 lattice that keep clearance `r` from every cliff edge of the cell (an edge to a
different-level, non-ramp neighbour, using the simulation's rule, not the mesh). The drop is the cell's level
height minus `MapMesh::top_at` under the centre.

- It asserts a drop of at most 0.2 in every cell that `diagonal_runs` does not cut.
- In cut cells it asserts a drop of at most `CLIFF_LEVEL_HEIGHT·(level(X) − lb)`.
- It prints the per-radius maximum in cut and uncut cells.

It passes on `main` (uncut maximum 0.077). WP2 must keep the uncut bound and report the cut figures. The test lives
in `client` because `client` may read the catalog at run time; the `sim_*` crates are not touched.

**T-B1, T-B2 (`cliff_boundary_tests.rs`, unchanged code):** they gain `..._diagonal_fixture` functions only.
Everything else stays as it is: the four maps, `τ`, `ε`, and the corner/profile classes. Under (c) both must
pass on all five maps. The nook classification stays, and it now catches any chamfer that strays into a low cell's
corner triangle.

**T-C, unchanged files that must stay green:**

- `terrain_tests.rs`: the 4×4 counts (only void border walls and rectangle corners, so nothing is cut and nothing
  reverts), heights, the seams on `showcase_canyon` (four cut centres on seams), winding, unit normals,
  incremental rebuild and capacity reuse;
- `ramp_geometry_tests.rs` T0 to T3;
- `cliff_contact_gpu_tests.rs` G-1 (its wall `x = 26`, row 31, is the rectangular central plateau, untouched);
- `mesher.rs` T-A and T-A2;
- `smog_golden_tests.rs` (it reads no mesh).

### 4.2 Not tested on the GPU

Straightness and W1 are geometric properties of the CPU mesh. The real-shader path is unchanged, and G-1 and the
review capture cover it. No new GPU test is added. The brink figure is shown on the real shaders in the review
capture instead (§5.2).

---

## 5. Goldens and the owner's review capture

No golden is re-baselined until its package's review capture exists under `~/scratch/straight-diagonals/review/`,
and the re-record commit names that capture. If the owner rejects the look, the package's commits are reverted,
not patched over.

### 5.1 Which goldens move, and why

| Golden | Moves? | Reason and record |
|---|---|---|
| `mesher_golden_tests.rs`: `showcase_canyon` and synthetic full-mesh and ramp-free chunk hashes | **yes, twice** | WP1: the void lone-low dual cells go back to the 45° midpoint cut (D-6; 86 on `showcase_canyon`). WP2: the full cuts (D-1 to D-4; 24 on `showcase_canyon`, the synthetic map's count from WP0). Each re-record extends the test module's `# Invariants` history: "re-recorded at `<sha>`, plan `2026-12-01-straight-diagonal-cliffs` WP1: void concave corners drawn through the midpoints again (D-6)", and the same for WP2: "ground diagonals drawn as straight chamfers through the low cells' corners (Q0136 (c))". |
| `mesher_golden_tests.rs`: `default_arena` hashes | **no** | no concave corner and no run (§1.5). A change is a stop. |
| pick hashes, hit counts, texture-array hash | no | the picker reads the grid. A change is a stop. |
| `terrain_tests` 4×4 counts | no | a change is a stop |
| `smog_golden_tests`, effect stills, `render_pool_gpu_tests`, showcase self-comparisons | no | they read no terrain mesh, or compare two renders of one tree |
| `terrain_tests::test_terrain_preview_renders` | thresholds only | must pass; its PNGs feed the review capture |

### 5.2 Review capture

Under `~/scratch/straight-diagonals/review/`, with the same poses and layout as
`~/scratch/terrain-indicators/review/wp3/`. There are four columns:

| Column | Tree | Source |
|---|---|---|
| `main_before` | `cbd8a8e4` (pre-terrain-plan production code) | copied from `~/scratch/terrain-indicators/review/wp3/main/` |
| `wp1` | `aaa5d450` (45° nooks, set back) | copied from `.../wp3/wp1/` |
| `main` | `0c16a3a9` (WP3, square nooks) | copied from `.../wp3/wp3/`: `git diff --stat 6c47fc4d 0c16a3a9 -- crates/client/src crates/client_diagnostics/src assets` is empty (checked while planning), so WP3's renders are `main`'s |
| `diagonals` | this plan's WP1 tree (`wp1/`), then its WP2 tree (`wp2/`) | rendered fresh |

The copies are named as copies in `README.txt`, with their commits. The fresh column uses the same commands as the
terrain plan's `run_visuals.sh` and `run_wp3_gpu.sh`, so the poses match pixel for pixel. Every GPU command runs
once per lock:

```
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock <command>
```

The release builds go into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`, and the large outputs go
under `~/scratch/straight-diagonals/`.

Views, rows of `contact_sheet.png` (`montage`, as in the terrain plan's `contact_sheet.sh`):

1. `staircase_closeup_p0_base.png`, `staircase_closeup_overview.png` (the two ground diagonals);
2. `preview_p0_base.png`, `preview_overview.png` (the canyons are visible in the overview: D-6);
3. `take_terrain_f200.png`;
4. **new:** `brink_<r>.png` for `r` = 0.3125 and 0.5. A ground unit of that radius, staged by property (the local
   player's ground archetype with the smallest drawn radius equal to `r` and with media, never by slug), stands at
   the outer corner of a cut cell, and the same unit stands at the cell's centre. These are rendered by a review
   child, `diagonal_cliff_review_gpu_tests.rs`. It is a separate Gpu target with an ignored parent, as the terrain
   plan's `air_indicator_review_gpu_tests.rs`, so the gate never runs it by default. The `main` column of this row
   is rendered fresh from a `main` worktree with the same child added on a scratch branch that is never merged. If
   that is impractical, the row shows the `diagonals` column only, and `README.txt` says so.
5. **new:** `straightness_overlay.png`: a top-down plot of `showcase_canyon`'s and the fixture's wall vertices,
   coloured by T-S1's chamfer deviation, for the WP2 tree. Ground runs and void runs are on separate panels.

`README.txt` holds:

- what each image shows and which decision it concerns;
- the commits of each column;
- the figures: cuts per map, high ground per step and per map (D-8), T-K's per-radius drops in cut and uncut cells,
  and T-S1's largest chamfer deviation per run.

The owner reviews the WP2 sheet and answers Q0138 from it if it is filed.

---

## 6. Proof that the simulation does not change

1. **No file changes** under `crates/sim_types`, `crates/sim_map`, `crates/sim_core`, `crates/sim_fixtures` or
   `assets/`. Acceptance runs `git diff --stat main -- crates/sim_* assets` on the final tree, and it must print
   nothing. A change there is stop rule 1.
2. **No data path from the mesh to the simulation.** The mesher is in `client::terrain`. Its output, `Mesh`
   buffers, feeds only rendering. The picker reads `GameMap` cells and their simulation heights (pick goldens
   unchanged, §5.1), and commands go through `PlayerCommand` (charter §3.2). The dependency direction (`client` →
   `sim_*`, check C1) makes a reverse path impossible to compile.
3. **Therefore the golden replays, checksums and throughput figures are not re-run** by this plan: nothing they
   hash or time changed. The verifier's `cargo gate package` selects by changed paths (`crates/client/**`, `docs/**`)
   and runs what the catalog says. If it selects the replays, they must pass unchanged.

---

## 7. Acceptance

1. T-S1 (three maps), T-S2, T-A3, T-S3 and T-S4 are green, and T-K is green with its figures in the evidence file.
2. T-B1 and T-B2 are green on all five maps with the derived `τ` and `ε`. T-A, T-A2 and every T-C test are green with
   no edit to their files beyond the new fixture functions in `cliff_boundary_tests.rs`.
3. The mesher golden hashes are re-recorded only as §5.1 says, each after its review capture. `default_arena`'s
   hashes, the pick hashes and counts, and every other golden are identical.
4. §6.1's diff is empty.
5. The terrain plan's scratch mesher check (`~/scratch/terrain-indicators/mesher_check/`), re-run on the final tree,
   still counts 0 rows on the low side of their edge. The chamfer profiles are checked against their own line by
   T-A3 and T-S1.
6. `cargo gate package` is green on the final tree, run by the verifier agent (charter §4).
7. Records:
   - `DESIGN.md` §4: B-246 (§12), and B-245's evidence column, which now cites this plan as the look it ships with;
   - plan 21b §5.3: a superseded note on the "45° cuts … straight 45° cliff" sentence;
   - the terrain plan: an amendment pointing here, which records that its D-3 (a) is replaced on diagonals and void
     nooks;
   - the evidence file `docs/evidence/2026-12-01-straight-diagonal-cliffs.md`;
   - this plan's `## As built`;
   - the README row.

---

## 8. Work packages

One package (crate `client`, plus records) on branch `straight-diagonals` in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/straight-diagonals`. Large outputs go under
`~/scratch/straight-diagonals/`. WP1 is small and independent of WP2, so the owner can keep one and drop the other.

**WP0: measurements and red tests.**
1. Port the scratch analysis to a scratch Rust check (`~/scratch/straight-diagonals/mesher_check/`, never
   committed). It calls the client's `dual_corner_levels` and counts, per shipped map and on the synthetic map, the
   rows of §1.5's table, so the planning figures become measured ones.
2. Add `diagonal_fixture`, `diagonal_runs` and `void_runs` to `support/terrain_mesh.rs`, and the stub predicate in
   `mesher.rs`: `pub(crate) fn full_cut(...) -> bool { false }`, documented as the WP0 stub, with no TODO marker
   (charter §1.1). T-S2 is red against it.
3. Write T-S1 to T-S4, T-A3, T-K and the `..._diagonal_fixture` T-B functions. Add the `suites.ron` entries:
   `client/diagonal_cliff_tests` is a CPU class with triggers on `crates/client/src/terrain/**` (**verify** the
   rule for CPU files), and `client/diagonal_cliff_review_gpu_tests` is Gpu. Run each new target alone and record
   its red output. T-K and T-S4 pass as guards. Never run a workspace test or `cargo gate`.
4. Evidence §1: the figures and red outputs.

**WP1: void diagonals return to the midpoint line (D-6).**
1. The void row of the lone-low cases: the 45° segment and the `NOT_x` high region of `aaa5d450`, selected when the
   lone low corner's level is −1 and the dual cell has no ramp corner. Update the module documentation's
   **Corners** paragraph.
2. Green: T-S1 on the void runs, T-B1 and T-B2 on five maps, and T-C unchanged (the 4×4 counts and the seams in
   particular).
3. Review capture §5.2, `wp1/` column.
4. Then re-record the mesher golden hashes (`showcase_canyon`, synthetic) in their own commit.

**WP2: full cuts (D-1 to D-5, D-7, D-9).**
1. `full_cut` (replacing the stub), the 4×4 block read with its performance rationale, `EffectiveCase` and the
   grouping by effective case, the general mitre `m(n₁, n₂)` in `mitre()`, the cut-centre profile over the cut run,
   the junction profiles, the floor clip with `chamfer_rim_point`, the strip columns, and the toe and anchor
   insertion. Update the module documentation:
   - W1's **Corners** paragraph states the full cut and its fallbacks;
   - a new **Diagonals** paragraph states the predicate, the 3×3 block and the shared functions;
   - the marching table's comment says which cases an effective case overrides.

   Split helpers so that every function stays within 100 counted lines (charter §2.4).
2. Green: T-S1 (all), T-S2, T-A3 and T-S3. Still green: T-S4, T-K's uncut bound, T-B1 and T-B2 (five maps), T-A,
   T-A2, G-1 (radv, through its harness) and T-C. Record the rebuild time (D-9).
3. Review capture §5.2, the `wp2/` column, the brink row and the overlay.
4. Then the second mesher golden re-record, in its own commit.

**WP3: records.** Everything listed in §7.7. The hand-off follows `cargo gate quick` and the targets above, then
the verifier runs `cargo gate package`.

---

## 9. Stop rules

Stop and report to the orchestrator, with the evidence, when:

1. any file under `crates/sim_*` or `assets/` would change;
2. `default_arena`'s mesher hashes, the pick hashes and counts, the 4×4 count fixture, `smog_golden_tests`, an
   effect still or a showcase self-comparison changes;
3. T-S1 needs a tolerance above the derived `τ`, T-B one above `τ` or `ε`, or T-S3 finds a T-junction that needs a
   tolerance;
4. a chunk seam or stacked run stops being watertight (T-S4, `terrain_tests` seams, ramp T3), or T-S4 shows that
   the shared column at a cut's centre cannot match without per-dual-cell knowledge beyond the 4×4 block;
5. a dual cell with a ramp corner changes, or `dual_floor_level_beside` needs a change;
6. T-K finds a drop above 0.2 in a cell that is **not** cut, or above the cliff height in a cut cell;
7. the full `showcase_canyon` mesh build becomes more than 2× slower (D-9), or a new path allocates per frame;
8. a GPU child cannot get the physical GPU or a real shader fails to load. Fail loudly (charter §1.2) and never fall
   back silently;
9. the review capture shows a look that departs from "straight 45° diagonal through the low cells' corners" on a
   shipped map. The orchestrator then asks the owner before any re-record.

---

## 10. Files

| File | Change | WP |
|---|---|---|
| `crates/client/src/terrain/mesher.rs` | WP0: the stub `full_cut`, T-S2, T-A3. WP1: the void lone-low row (D-6). WP2: the predicate, the 4×4 read, `EffectiveCase`, grouping, the general mitre, cut and junction profiles, the floor clip with `chamfer_rim_point`, strip columns; module documentation | 0, 1, 2 |
| `crates/client/tests/support/terrain_mesh.rs` | `diagonal_fixture`, `diagonal_runs`, `void_runs` | 0 |
| `crates/client/tests/diagonal_cliff_tests.rs` (new) | T-S1, T-S3, T-S4, T-K | 0 |
| `crates/client/tests/diagonal_cliff_review_gpu_tests.rs` (new) | the brink review child (ignored parent) | 0 |
| `crates/client/tests/cliff_boundary_tests.rs` | `..._diagonal_fixture` T-B functions | 0 |
| `crates/client/tests/mesher_golden_tests.rs` | golden constants and history | 1, 2 |
| `crates/gate/suites.ron` | the two new targets | 0 |
| `DESIGN.md`, `docs/plans/21b-plan-map-editor.md`, `docs/plans/2026-11-30-terrain-and-air-indicators.md`, `docs/evidence/2026-12-01-straight-diagonal-cliffs.md`, this plan, `docs/plans/README.md` | records | 0, 3 |

---

## 11. Out of scope and recorded decisions

- **Void diagonals with the full cut** (D-6). This is rejected for now and can be brought back by one predicate
  change, condition 3's "ground".
- **Toothed stretches beside ramps and on two-cell diagonal bands** (D-7). Both fall back to `main`'s drawing. A
  ramp-aware chamfer would need the ramp pass to share the chamfer's lines (R1 to R6), and a two-cell band cannot
  hold two rim bands. No shipped map has either.
- **A shallower chamfer profile** (setback 0.06 and no foot on chamfers, which would cut the band from 0.37 to 0.21).
  This is rejected. It changes the cliff's look between axis walls and chamfers, and the brink drop stays 3.0 for
  `r ≤ 0.5`, because the centre still lies beyond the foot.
- **Moving drawn units up onto the drawn surface.** This is rejected. The client would draw units where the
  simulation does not hold them (charter §3.2).
- **Picking on the drawn mesh** (D-8). The picker stays on the grid. A mesh-based picker is a separate change with
  its own goldens.
- **Diagonals at other angles** (1:2 staircases and so on). The predicate's condition 3 needs a unit step, so other
  slopes keep `main`'s drawing. A general "straight line through the concave corners" rule would need a run
  detector over unbounded windows.

---

## 12. Bug row

For `DESIGN.md` §4. The number is provisional, next free at merge.

```
| B-246 | Since plan `2026-11-30-terrain-and-air-indicators` WP3 (`6c47fc4d`, Q0136's default (a)), a 45° staircase cliff draws as a sawtooth of 45° teeth and square nooks (`MARCHING_CASES` cases 7, 11, 13, 14 squared), not the straight diagonal of plan 21b §5.3; the owner chose straight diagonals through the low cells' corners (Q0136 (c)). The void-side diagonals (the abyss canyons of `showcase_canyon`) are squared too, with no walkable cell beside them. Found by plan `2026-12-01-straight-diagonal-cliffs` §1 (number provisional, next free at merge) | fixed by that plan: WP1 (void diagonals through the midpoints again, D-6), WP2 (ground diagonals as straight chamfers on the high side, D-1 to D-5) |
```

---

## 13. Question draft

Not filed by this commit. Filing it is the orchestrator's call, and the default is what WP2 builds.

```
# Q0138: Straight diagonal cliffs draw high-ground units hanging over the cliff. Is that size acceptable?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (a) keep it; plan 2026-12-01-straight-diagonal-cliffs builds option (c) of Q0136 as
  answered

## Question

You chose straight 45° diagonal cliffs drawn through the low cells' corners (Q0136 (c)). Units on the low ground
are never drawn inside rock. The cost falls on the high side. The straight line runs through the middle of every
high step cell, so the outer half of each step is drawn as low floor, a full cliff level (3 world units) below
where the simulation holds units. A ground unit standing in the middle of such a cell, or at its outer corner, is
drawn hanging over the cliff edge. That is 25 of the 28 ground unit types: every type with a radius up to 0.56. The
other cliffs keep the 0.2 limit the terrain plan set (today 0.077). The review capture
`~/scratch/straight-diagonals/review/contact_sheet.png` (rows `brink_*`) shows a small unit at the corner and at the
centre of a step cell.

## Options

- **(a) Keep it (default).** The straight look as answered. The overhang is recorded as a sanctioned deviation of
  the drawn terrain.
- **(b) Square inner corners after all** (Q0136 (a), what `main` draws today): no overhang beyond 0.08, toothed
  diagonals.
- **(c) The 45° cut through the midpoints** (Q0136 (b)): straight diagonals half a cell further out. Units on the
  high side overhang by less, and units in the low nooks are drawn up to 0.2 inside the rock.

Recommendation: (a), since it is what you chose and the alternatives are the two looks you turned down; (b) if
units floating on diagonal plateaus look wrong in the capture.

## Answer
```

---
