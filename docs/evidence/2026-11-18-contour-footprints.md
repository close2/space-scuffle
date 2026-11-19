# Evidence: Contour Footprints and 5.0.14 Footprint Sizes (B-134, B-138)

Measurements and test results of plan [`2026-11-18-contour-footprints`](../plans/2026-11-18-contour-footprints.md),
one section per work package.

## How to reproduce

Each tree builds its release tools into its own target directory and copies them aside, so later builds cannot
change an earlier step's binaries (charter §7: hot builds on the home disk, large output on `~/scratch`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/contour-footprints
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} <bin>/
```

Then, from the repository root (`<dir>` under `~/scratch/contour-footprints/`):

```bash
<bin>/matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv
<bin>/matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route
<bin>/route_trace --scenario scenarios/showcase.ron --ticks 6000 --out <dir>/route-end
for s in 1 2 3 4 5 6 7 8; do
  <bin>/route_trace --scenario scenarios/showcase.ron --seed $s --ticks 6000 --out <dir>/route-s$s
done
diff <base>/b33/33_24_marines_vs_12_zealots/seeds.csv <dir>/b33/33_24_marines_vs_12_zealots/seeds.csv
for d in route route-end route-s1 ... route-s8; do cmp <base>/$d/units.csv <dir>/$d/units.csv; done
```

Goldens: `cargo test -p sim_core --test golden_replay_tests` in debug and with `--release`, and
`cargo test -p sim_core --test golden_replay_tests --target wasm32-wasip1 --release` under wasmtime (CI's
`determinism-wasm` command). `sim_map` goldens: `cargo test -p sim_map --test determinism_tests --test
validation_golden_tests --test navmesh_tests`, native and on `wasm32-wasip1`. Throughput:
`flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, the trees' bench binaries alternated, three
rounds.

**Sources (§2.3).** The 5.0.14 data were read as data:

- `~/scratch/structure-radius/dl-footprint/FootprintData.xml`, downloaded by the structure-radius plan from
  `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`;
- `~/scratch/combat_wp3_data/liberty/UnitData.xml`.

`~/scratch/contour-footprints/scripts/check_shape.py <UnitData.xml> <FootprintData.xml> <rows.py>` prints, for every
analogue, its `Footprint` and line, the footprint's first line, its `Place` and `Check` cell counts, the line of its
`Shape` `Offsets`, and the shape's `(hw, hh, c)` read from the offsets, with a check that the offsets form a centred
box with equal 45° chamfers. Its output is `~/scratch/contour-footprints/shapes.txt`.

## 0. WP0: records

**Verified (§2.3, all 49 rows)** with `check_shape.py`. For every analogue, the footprint, its line in liberty
`UnitData.xml`, the `Offsets` line and `(hw, hh, c)` equal the plan's table, and every shape is a centred box with 45°
chamfers (8 offsets, or 4 for the uncut 1×1). Line-number notes, no value differs:

- `Footprint1x1`'s `Offsets` are at Footprint:1107 (the plan cites :1106, its `<Radius>` line).
- `Footprint2x2IgnoreCreepContour` (Greater Spire) has no `Place` rows of its own: it inherits them from its parent
  `Footprint2x2` (`parent="Footprint2x2"`, Footprint:1226; the parent's 2×2 rows at Footprint:1184–1197).
- The Lurker Den analogue's `Footprint` is at liberty:678 (`LurkerDenMP`).

| Analogue group | Footprint (liberty line) | Place = Check | `Offsets` | `(hw, hh, c)` |
|---|---|---|---|---|
| Command Center, Orbital Command, Planetary Fortress, Nexus | 5x5Contour (10379, 10525, 10663, 4186) | 5×5 | :2016 | (2.5, 2.5, 1.5) |
| Hatchery, Lair, Hive | 6x5DropOffCreepSourceContour (8210, 8292, 8372) | 5×5 | :2262 | (2.5, 2.5, 1.5) |
| 17 Terran and Protoss 3×3 buildings | 3x3Contour | 3×3 | :1936 | (1.5, 1.5, 0.5) |
| 9 Zerg 3×3 buildings | 3x3CreepContour | 3×3 | :2206 | (1.5, 1.5, 0.5) |
| Refinery, Assimilator, Extractor | GeyserRoundedBuilt (5778, 4124, 8151) | 3×3 | :960 | (1.5, 1.5, 0.75) |
| Supply Depot, Missile Turret, Tech Lab, Reactor, Pylon, Photon Cannon, Shield Battery, Dark Shrine | 2x2Contour | 2×2 | :1951 | (1, 1, 0.5) |
| Spine Crawler | 2x2ZergSpineCrawler (8617) | 2×2 | :5370 | (1, 1, 0.5) |
| Spore Crawler | 2x2Contour2 (8751) | 2×2 | :163 | (0.5, 0.5, 0.25) |
| Sensor Tower | 1x1 (9454) | 1×1 | :1107 | (0.5, 0.5, 0) |
| Spire | 2x2CreepContour (9013) | 2×2 | :2074 | (1, 1, 0.5) |
| Greater Spire | 2x2IgnoreCreepContour (9084) | 2×2 (inherited) | :1239 | (1, 1, 0.5) |

**The Hatchery item of B-138 is withdrawn** (plan §2.3 correction). `Footprint6x5DropOffCreepSource`
(Footprint:2210–2236) and `Footprint6x5DropOffCreepSourceContour` (Footprint:2238–2264) both have 5×5 `Check` and
`Place` layers (`Area="-2,-2,3,3"`, five rows of `xxxxx`); the contour's `Shape` is the 5×5 octagon cut by 1.5
(:2262). "6x5" is only the name.

**Records.** `DESIGN.md` §4: B-134 restated (pointing to this plan, open until WP8); B-138 restated with the Hatchery
item withdrawn; B-139 added (footprint layer flag sets; its citations are the `Check` layer of
`Footprint5x5Contour`, Footprint:1994–2003, whose `NearResources` exclusion is at :1996, and the `Place` layer of
`FootprintGeyserRoundedBuilt`, Footprint:941–957). B-139 was free: the register's highest number was B-138. The
plan's row in `docs/plans/README.md` was filed `Proposed` with the plan.

**Base.** The WP0 tree (`6758712a`) reproduces the structure-radius merge (**verified**): the default seed ends
with player 1 winning at tick 2,191 (reversals 25 / 10, stall give-ups 0 / 0, deeper unit-ticks 0 / 0); seeds 1 to
6 and 8 player 1 (at 2,200, 2,185, 1,886, 5,700, 1,580, 3,419 and 1,712), seed 7 player 0 at 2,552. Matrix: 1,160
runs, expectation failures 0 on the seeds of record, 4 over all seeds.

## 1. WP1: quarter-cell lattice

**WP1a** (`8626abbe`). `NavMesh::digest` eats each vertex as its raw Q32.32 world position and each width reduced to
lowest terms (`SquaredWidth::reduced_parts`). `SHOWCASE_MESH_DIGEST` re-pinned from `0x46d3_38ba_8cd1_c537` to
`0x4a14_9269_337f_3464`, native and `wasm32-wasip1`. No other change.

**WP1b** (`905ec057`). Lattice coordinates are quarter cells; widths built from lattice units carry `den × 16`; the
margins of `predicates.rs` are re-derived for 2^14 and `admits_diameter`'s radius cap drops to 2^12 world units.
The commit message carries the audit of every coordinate-arithmetic site (converted or unit-free). One site needed
a choice the plan did not spell out: `segment.rs`'s `inward_normal` reduces the edge direction by its gcd before
normalising, so a normal depends on the edge's direction only (unit-free, and exact for the diagonal edges WP3
adds).

| Test (red on WP1a) | WP1a | WP1b |
|---|---|---|
| `predicates::tests::lattice_points_are_quarter_cells` | Does not compile (no `cell_corner`); with a scale-1 stub, `(13, 7)` maps to (13, 7), expected (3.25, 1.75) | Passes |
| `predicates::tests::squared_widths_are_in_cells_squared` | As above; with the stub, points 4 apart are 16 cells² | Passes |
| `predicates::tests::extremes_do_not_overflow` (2^14) | As above; with the stub the limit is 4,096 | Passes |

Test edits (lattice literals only): `navmesh_tests`' area law counts squared quarter cells; `funnel_tests` and
`pathfinding_properties` read route turn vertices through `to_sim` and `cell_corner`.

**Bit-identity (D-15, 1)**, WP0 tools against WP1 tools:

| What | Result |
|---|---|
| `sim_map` goldens (`EXPECTED_CHECKSUM`, `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `PATH_GOLDEN`), `SHOWCASE_MESH_DIGEST` after WP1a | Unchanged; every `sim_map` test passes |
| Matrix (`--compare` WP0) | Bit-identical (1,160 runs) |
| Battle 33 `seeds.csv` | Byte-identical |
| Showcase `units.csv`, `summary.md`, `overlap.csv`: 1,800 ticks, to the end, seeds 1 to 8 | Byte-identical |
| `golden_replay_tests` | Pass unchanged: debug, release, `wasm32-wasip1` release under wasmtime |
| `sim_map` on `wasm32-wasip1` (wasmtime with the worktree mapped by `--dir`) | `determinism_tests` and `navmesh_tests` pass. `validation_golden_tests`' three corpus digests fail on wasm **on the base too**, with the same wasm values on both trees (decode `0xef65_8b0a_0d4e_7fe9`, validation `0x01e3_8765_0693_1dd7`, path `0xa243_924f_8125_f760`): those pins are native-only (finding, pre-existing; CI replays only `golden_replay` on wasm). Their wasm values are unchanged by WP1 |

## 2. WP2: shape type, predicates, walkable placement, schema 30

**Change.** `sim_types::FootprintShape` (quarter cells, `full`, `inscribed_half_extent`); `sim_map::StructureFootprint`
(`square`, `validate`); `PlacedStructure.shape`; `place_structure(id, cell, footprint)` with all 51 call sites moved
to `StructureFootprint::square` (the two production sites included; WP4 passes the catalog's shape); D-5
(`PlacementFailureReason::UnpathableTerrain`, checked before buildability; `sim_core`'s site refusal reports it as
`SiteRefusal::UnbuildableTerrain`, the client names it "the ground there cannot be walked on");
`MapError::InvalidFootprintShape`; D-7's predicates in `sim_map/src/footprint.rs` (`contour`,
`contains_open_scaled`, `meets_open_half_cell`, `segment_meets`, `capsule_meets`) and
`GameMap::structure_occupants_touched`. The mesh still cuts cells.

**Verified: `fields!` requires completeness.** The macro destructures `let Self { a, b, … } = self` without `..`, so
adding `PlacedStructure.shape` does not compile without encoding it (or naming it `unhashed`). Following the plan's
contingency, the encoding (`fields!(PlacedStructure { …, shape })`, `fields!(FootprintShape { … })`) and
`CHECKSUM_SCHEMA_VERSION` 29 → 30 move into WP2, and WP3's schema proof is made here.

**Tests, red on WP1** (WP1 tree with WP2's tests: none compiles, since `StructureFootprint`, `FootprintShape`,
`UnpathableTerrain` and `InvalidFootprintShape` do not exist there; on WP1 the D-5 site, a buildable but unwalkable
cell, is admitted): `footprint::tests::{a_full_shape_is_its_cells,
contour_vertices_are_on_the_quarter_lattice_for_odd_and_even_sizes, contains_open_is_exact_on_a_chamfer,
segment_meets_the_closed_shape, capsule_meets_is_exact_at_a_chamfer, a_shape_outside_its_cells_is_refused,
occupants_touched_are_distinct_and_in_cell_order}`, `sim_types::footprint::tests` (2),
`wall_off_tests::placement_refuses_unwalkable_ground`, `checksum_state_tests::a_placed_structures_shape_is_hashed`.
`capsule_meets_is_exact_at_a_chamfer` uses a point exactly 0.3125 (a 3-4-5 offset) from a 5×5 chamfer's end vertex
(no meet at 0.3125, meet at 0.3125 + 2⁻¹⁶) and a point √2/4 from the chamfer's face (no meet at 23,170 · 2⁻¹⁶, meet
at 23,171 · 2⁻¹⁶).

**Schema 30 proof (D-15, 2).** A scratch build with `shape` named `unhashed` and schema 29 passes
`golden_replay_tests` against the fixtures as recorded before WP2, so it reproduces every recorded checksum of both
fixtures. A scratch entity-only digest (entities, projectiles, players, effects; no schema header; a temporary
`compute_entity_only_checksum` and observer, applied by `~/scratch/contour-footprints/scripts/scratch_harness.py`,
not committed) is identical on all 1,000 ticks of `golden_replay` and all 600 of `golden_replay_orders` between that
build and schema 30. Schema 30 changes every checkpoint (20 of 20, 60 of 60); both fixtures are re-recorded with a
header naming the bump and the proof. Test edit: the four `checksum::encoding` unit tests that spell the schema byte
(29 → 30), as at every bump.

**D-5 count (D-15, 2).** `PATH_GOLDEN` is unchanged. `VALIDATION_GOLDEN` changes: instrumenting the corpus (scratch
`eprintln`s, not committed) with and without the rule, 17 start footprints the corpus already refused as
`UnbuildableTerrain` (8 reports of start 30, 9 of start 31) are refused as `UnpathableTerrain`, and 2 seeded path
corpus placements change reason the same way (both already refused). **No placement or start footprint is newly
refused**; the counts (13+14 in 15 reports, unreachable in 4, 2,362 warnings) are unchanged. A scratch build without
the rule reproduces `0xf028_1611_929c_52d1`; re-recorded as `0xc8fc_3cbd_495d_5177` with that count in its comment.
**Verified:** both shipped maps validate (`validation_tests::showcase_canyon_is_valid`,
`default_arena_file_is_valid`). **Verified:** `grep -rn "PlacedStructure {" crates` finds only `grid.rs` (and the
`fields!` line), so no file persists a `GameMap`.

Test edits, each by D-5: `wall_off_tests::test_building_placement_invariants` step 4 (a 3×3 across the arena's cliff
is now refused first as `UnpathableTerrain`, traced: `PlacementBlocked { cell_x: 10, cell_y: 29, reason:
UnpathableTerrain }`; the accepted reasons gain it); `map_gate_tests::f10_removing_a_structure_restores_the_cell_flags_it_found`
(its "pad over a chasm", buildable and not walkable, can no longer hold a structure: the test now asserts that
refusal, and checks the restore on a walkable cell with a non-default flag set).

**Behaviour (D-15, 2)**, WP1 tools against WP2 tools: matrix bit-identical (1,160 runs); battle 33 `seeds.csv`
byte-identical; showcase `units.csv`, `summary.md`, `overlap.csv` byte-identical at 1,800 ticks, to the end and on
seeds 1 to 8. Goldens (re-recorded) pass in debug, release and `wasm32-wasip1` release; `sim_map`'s
`determinism_tests` and `navmesh_tests` pass on `wasm32-wasip1`. The whole `sim_core` suite passes.

## 3. WP3: stopped at the threshold property (R-2), Proposed Amendment below

WP3 is **not committed** to `contour-footprints`. Its work in progress (the contour cut, the D-6 raster and all of
WP3's tests but the two new pinned goldens) is kept on the side branch `contour-footprints-wp3-wip`, one commit on
top of WP2's evidence, so nothing is lost.

**What the WIP does** (D-4, D-6): the whole-cell `EdgeGrid` is terrain only; a sorted `StructureEdges` store holds
each structure's contour and its interior cell lines clipped to the shape's open interior, as unit quarter-lattice
edges in four canonical directions; `ConstraintGraph` merges both (`Wall > Seam`), finds vertices by the
eight-direction branch rule over every cell corner and every quarter point of each closed footprint, and collects
runs in `(y, x)` order with directions east, north, north-east, north-west. Labels test the centroid against the
occupant's shape exactly (`contains_open_scaled(sum, 3)`). The clearance field marks the corners of every half-cell
whose open interior meets the shape.

**Tests on the WIP** (red on WP2 first, as the plan asks):

| Test | WP2 | WIP |
|---|---|---|
| `wall_off_tests::a_diagonal_contour_wall_passes_a_worker_and_stops_a_zergling` (both orientations) | Fails: the worker finds no route | Passes |
| `wall_off_tests::a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` (`k` = 64 to 115) | Fails at `k` = 64 | **Fails at `k` = 85** (below) |
| `wall_off_tests::a_5x5_and_a_3x3_corner_to_corner_pass_medium_units` | Fails: 0.6875 finds no route | Passes |
| `wall_off_tests::a_5x5_sharing_one_cell_of_border_is_a_semi_wall_and_two_cells_is_tight` | Fails: 0.3125 finds no route | Passes |
| `wall_off_tests::face_adjacent_contours_are_tight` | Passes (a guard: cells were tighter) | Passes |
| `navmesh_tests::a_square_shape_cuts_the_mesh_as_cells_did` (24 seeded square structures on the showcase map; digest `0xb220_7731_a3e3_10c8` computed by the same test on WP2's tree) | Passes (it pins WP2) | Passes: the square cut is the cell cut |
| `navmesh_tests::contour_structure_triangles_lie_in_one_cell` | Fails: whole cut corners labelled structure | Passes |
| `navmesh_tests::a_cut_corner_is_open_ground` | Fails | Passes |
| `navmesh_tests::removing_a_contour_structure_restores_the_terrain_mesh` | Passes (guard) | Passes |
| `navmesh_tests::contour_meshes_build_for_seeded_layouts` (proptest, 64 cases, flush, diagonal and seeded placements of the §2.3 shapes and a 3×2) | Fails: labels | Passes: no `ConstraintRecovery`, labels exact, every run on a contour, a clipped cell line or a cell edge |
| `wall_contact_tests::contact_against_a_chamfer_is_exact` | Fails | Passes: point (10.25, 10.25), `distance_sq` exactly 2^61, normal `(−1, −1)/√2`, equal to the exhaustive flood |
| `clearance_field_tests::square_shapes_keep_todays_clearance` (8 seeded layouts against the same cells as unwalkable terrain) | Passes | Passes |
| `clearance_field_tests::a_contours_clearance_never_exceeds_the_true_clearance` (8 seeded layouts, exact distances at the nine samples) | Passes (cells underestimate more) | Passes, and some cells underestimate |
| Guards `f7_no_tier_passes_between_structures_touching_at_a_corner`, `test_ramp_wall_off_geometry` | Pass | Pass |

Every pre-existing `sim_map` golden is unchanged on the WIP (`EXPECTED_CHECKSUM`, `VALIDATION_GOLDEN`,
`DECODE_GOLDEN`, `PATH_GOLDEN`, `SHOWCASE_MESH_DIGEST`) and every other `sim_map` test passes. Not yet written:
`CONTOUR_PATH_GOLDEN`, `determinism_tests::contour_route_checksum` and the `navmesh_measure` row; they pin routes
through contours, which the amendment below may change, so they are left for after it.

**Bit-identity of the WIP** (every production structure still has a full-box shape until WP4, so D-15 rule 2 holds
for the cut), WP2 tools against the WIP's tools: matrix bit-identical (1,160 runs); battle 33 `seeds.csv`
byte-identical; showcase `units.csv`, `summary.md`, `overlap.csv` byte-identical at 1,800 ticks, to the end and on
seeds 1 to 8; `golden_replay_tests` pass unchanged in debug, release and `wasm32-wasip1` release; `sim_map`'s
`determinism_tests` and `navmesh_tests` (with the new contour tests) pass on `wasm32-wasip1`. `cargo clippy -p
sim_map --all-targets` is clean on the WIP.

**The failure (R-2, traced).** On the staircase map with two 2×2 contours corner to corner (gap `1/√2 ≈ 0.7071`),
scanning `t = k/256` in both orientations (`~/scratch/contour-footprints` scratch test, not committed):

| `k` (t) | Route found | Funnel | Legs failing the segment predicate |
|---|---|---|---|
| 64 to 84 (0.25 to 0.328) | yes | legal (shrunk, or pinned for some `k`) | 0 |
| 85 to 90 (0.332 to 0.352) | yes | **shrunk and pinned both illegal; fallback to portal midpoints** | 4 of 16 |
| 91 to 115 | `NoPathFound` | — | — |

Route **existence** is exact: the A* width test admits exactly `8t² ≤ 1` (`k ≤ 90`) in both orientations. The
failure is the route's **shape**: for `2t < w < 2t + 2·CORNER_SAFETY_MARGIN` (here a spare `w − 2t` of 0.043 down to
0.004), every corner waypoint lies `t + 0.05` from the vertex it rounds, which is `w − (t + 0.05) < t` from the
parallel chamfer across the band, so every funnel leg through the gap fails the segment predicate; the pinned retry
has the same `rho`; the portal-midpoint fallback cuts the corners of the approach. The threshold seen, spare below
about 0.05 (one margin) rather than 0.1 (two), is the shrink and pinned retries absorbing up to one margin. This is
the plan's R-2 exactly; its mitigation says "a failure is a funnel defect, traced and fixed by amendment; the margin
is not tuned", so WP3 stops here.

**Which shipped units it reaches.** Empirically a route through such a band is legal while its spare `w − 2t` is at
least one margin (0.05; `k` = 84, spare 0.051, is legal) and illegal below. For corner-to-corner gaps of the §2.3
chamfers (`w = (c₁ + c₂)/√2`, `c` in 0.25, 0.5, 0.75, 1.5) the failing bands are (0.152, 0.177], (0.240, 0.265],
(0.329, 0.354] (2×2, 3×3 and Spore with geyser), (0.594, 0.619], (0.417, 0.442], (0.682, 0.707], (0.505, 0.530],
(0.771, 0.796] and (1.036, 1.061]; **no shipped terrain radius** (0.3125, 0.375, 0.5, 0.5625, 0.625, 0.75, 0.875,
0.9375, 1.0) lies in any of them, and the workers (0.3125, spare 0.082) get a legal route. The defect is still real:
other radii, and other contour geometries (chamfer against face, against a terrain corner, semi-walls), give other
widths that this scan did not enumerate, and a unit routed on the fallback walks legs the contact rule refuses.

### Proposed Amendment (for the orchestrator): a margin that fits the channel

**Decision asked.** How the radius funnel routes a disc through a channel whose narrowest width `w` leaves less than
two corner margins, `2t < w < 2(t + CORNER_SAFETY_MARGIN)`. Contours make such channels common (every diagonal gap
has a width that is not a whole number of cells), so R-2 is no longer a corner case.

**Proposal (recommended): a fitted retry before the fallback.** In `navmesh/search.rs`, when the shrunk route and the
pinned retry are both illegal, pull the channel once more (shrunk, then pinned) with
`rho' = t + m'`, `m' = min(CORNER_SAFETY_MARGIN, (w_min − 2t) / 2)`, where `w_min` is the narrowest admitting width
of the route's channel (the A* corner widths already computed, `SquaredWidth`, converted with a floor square root
in raw units so `m'` can only be smaller). `CORNER_SAFETY_MARGIN` stays 0.05 and stays the margin wherever it fits;
only a channel too narrow for it gets the room it has. The leg checks need the matching arc-chord allowance
(`ARC_CHORD_DIP` is half the margin; it would become `m' / 2`). Routes that are legal today are unchanged (the
retry runs only after both existing strategies fail), so every golden and every structure-free world stays
bit-identical; that is checked in WP3's exit as today.

- Acceptance: `a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` passes for every `k` (a legal route iff
  `8t² ≤ 1`), plus a property over the §2.3 gap widths (2×2, 3×3, 5×5, geyser pairs) asserting a legal route for
  every shipped terrain radius that fits; `mesh_funnel_fallbacks` stays 0 on the matrix and the showcase.
- Cost: one more funnel pull, only after two failures.

**Alternatives.**
1. Accept the fallback for now: the threshold property asserts existence for every `k` and legal legs only where
   the spare is at least one margin, and a register row records the narrow-channel funnel defect. Cheaper, and no
   shipped radius meets a failing band at the §2.3 corner gaps; but it weakens a test the plan specified, and other
   geometries are not enumerated.
2. Keep the margin and refuse such channels at the A* width test (`w ≥ 2(t + margin)`). Changes route existence
   (workers would no longer pass a 0.707 gap: 0.625 + 0.1 > 0.707), contradicting S10 and the owner's Q0087 argument.
3. Tune `CORNER_SAFETY_MARGIN`. Forbidden by D-15 rule 7.

On approval, WP3 resumes from `contour-footprints-wp3-wip`: the amendment's change and tests, then
`CONTOUR_PATH_GOLDEN`, `contour_route_checksum`, the `navmesh_measure` row, and WP3's exit measurements.

## 3b. WP3 resumed under Amendment 1

**Change** (on top of the work in progress of §3, brought from `contour-footprints-wp3-wip`):

- **Fitted retry** (`navmesh/search.rs` `fitted_retry`, Amendment 1). After the shrunk pull and the pinned retry
  both fail the segment predicate, the channel is pulled once more, shrunk then pinned, with the corner margin
  `min(CORNER_SAFETY_MARGIN, (w_min − 2t)/2)` and an arc-chord dip of half that margin (`radius_funnel` now takes
  the dip; every earlier pull passes `ARC_CHORD_DIP` as before). `fitted_margin` returns nothing when the full
  margin fits or no room is left.
- **`w_min` is measured across the channel, not from the TA* corner widths.** The amendment named "the A* corner
  widths already computed"; tracing a proptest failure showed they can miss the bottleneck: a Spore-sized shape's top
  face across from the long terrain edge y = 30 on the default arena is exactly 1.5 wide, but the triangles the route
  crosses measure their corners at (34, 30) (1.677) and (35.5, 28.25) (1.75), and no corner width pairs the face
  with the wall. `channel_width` therefore takes the least distance between the channel's left obstacles (left portal
  vertices, blocking edges of channel triangles on the left) and its right ones. It can only underestimate, so the
  margin can only be smaller (a `PERFORMANCE RATIONALE` explains the quadratic scan on this rare path).

**Tests** (Amendment 1, item 2):

| Test | Result |
|---|---|
| `wall_off_tests::a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` (k = 64 to 115) | Passes: a route iff `8t² ≤ 1`, every leg passes the segment predicate (k = 85 to 90 now take the fitted retry) |
| `wall_off_tests::the_fitted_retry_passes_a_channel_with_a_quarter_margin_to_spare` (both orientations; red before the retry: the portal-midpoint fallback) | Passes: no fallback, every leg legal |
| `navmesh::search::tests::the_fitted_margin_is_half_the_spare_width` (unit) | Passes |
| `wall_off_tests::a_channel_with_room_for_the_margin_keeps_its_route` (guard: k = 64 to 84, spare ≥ one margin, both orientations; digest `0xfeff_042e_30b7_7389` recorded with the retry disabled) | Passes: byte-identical routes |
| `navmesh_tests::contours_add_no_funnel_fallbacks` (deterministic, 150 seeded layouts, see below) | Passes |

**The fallback assertion: a deviation from Amendment 1's wording, for the orchestrator to confirm.** The amendment
asked for fallbacks to be "counted and asserted zero in the proptest". Asserting zero is not attainable there.
Stressing the property with up to 3,000 random cases found three classes of fallback that remain:

1. **Wedged starts**: the disc already overlaps a wall where it starts (D-3's regime, legs held to the centre). Full
   squares fall back there too (e.g. r = 0.75 beside a 3×2 at (17, 24)).
2. **Exact fits**: a disc exactly as wide as a bent channel (`(2r)² = w²`, which TA* admits by its existing `≤`
   rule), as r = 0.75 between that 1.5-wide face and wall. No polyline rounds such a bend under the strict segment
   predicate; a disc 1/4096 smaller now routes legally through the same channel (it fell back before `w_min` was
   measured across the channel).
3. **A vertex outside the channel**: a leg passes closer than `r` to a vertex that is neither a portal vertex nor a
   vertex of the start or goal triangle, which the funnel's refinement never considers (e.g. r = 0.875 from
   (41.5, 47.5): the first leg passes 0.765 from a geyser-shaped chamfer's end (40.75, 46)).

Classes 2 and 3 occur with **full-square structures as well**. On 600 seeded layouts (about 7,150 routes each),
contours: 14 fallbacks (12 exact fits); full squares: 35 (32 exact fits). Stressing the same property with
full-square layouts fails too (seed 12480478581175701913). So a zero assertion over random seeds would be a flaky
test of a pre-existing funnel limitation, now registered as **B-141**. Instead, `contours_add_no_funnel_fallbacks`
asserts deterministically, on 150 seeded layouts with the shipped terrain radii and endpoints where the disc fits,
that the contour layouts fall back no more often than the same layouts with full squares, in total and outside exact
fits: contour 11 of 1,729 (6 exact fits); square 14 of 1,722 (8 exact fits). The proptest
`contour_meshes_build_for_seeded_layouts` keeps the mesh laws (no `ConstraintRecovery`, labels, run geometry).

**New pins** (each passes native debug and release, and `wasm32-wasip1` release and debug):
`validation_golden_tests::find_path_with_vision_on_contours_matches_the_recorded_corpus_digest`
(`CONTOUR_PATH_GOLDEN` `0x9069_95c7_29b7_3498`; 256 cornered routes, 141 failures; every draw is a fixed-width range,
so it is the same corpus on 32- and 64-bit targets) and `determinism_tests::contour_route_checksum`
(`0xc0d5_fbfd_292f_37a4`, 1,000 routes among eight contours).

**B-140 registered** (Amendment 1, item 4), with a trace: the path corpus's radius index is drawn with
`rng.gen_range(0..3)` over `usize`; with that draw made `u32` (scratch, not committed) native produces exactly the
wasm digest `0xa243_924f_8125_f760` and wasm agrees, so that corpus differs because the test draws a different
corpus on 32-bit targets. The decode and validation corpora draw many `usize` ranges the same way (not traced). Pins
unchanged.

**Mesh build** (`navmesh_measure`, release, a new third table): the showcase with 24 seeded structures, full squares
573 / 1,123 vertices / triangles and a 11,125 µs rebuild; contours 671 / 1,321 and 11,341 µs (+2 %, below the
plan's 25 % bar). The output is in `~/scratch/contour-footprints/navmesh_measure_wp3.md`.

**Exit (D-15, 2; Amendment 1, item 3)**, WP2 tools against WP3 tools: matrix bit-identical (1,160 runs); battle 33
`seeds.csv` byte-identical; showcase `units.csv`, `summary.md`, `overlap.csv` byte-identical at 1,800 ticks, to the
end and on seeds 1 to 8; `golden_replay_tests` pass unchanged in debug, release, `wasm32-wasip1` release and once
in `wasm32-wasip1` debug; every pre-existing `sim_map` golden is unchanged, and `sim_map`'s determinism and navmesh
tests pass on `wasm32-wasip1`.

## 4. WP4: the catalog field and wiring (full-square shapes)

**Change.** `structures.ron` gains a required `footprint_shape: (half_width, half_height, chamfer)` on all 49 rows,
each the full box of its cells for now, with the comment `// Replaced by the 5.0.14 Shape in plan
2026-11-18-contour-footprints WP6.`. The loader checks it (`validate_footprint_shape`: finite multiples of 1/4, the
box inside the cells, the chamfer shorter than the smaller half-extent) and resolves it exactly to quarter cells.
`ShapeKind::BuildingDome` carries `shape: FootprintShape`. Every site that builds or destructures the variant was
moved: patterns gain `..`, constructors use `FootprintShape::full`; `ShapeKind::footprint()` reads a dome's cells
(used where adding a line would have pushed `validate_job_source` past 100 lines). `spawn_structure` and
`place_foundation` place the catalog's shape. `drawn_radius` is the shape's smaller half-extent (D-11). Charter C7:
the float allowlist of `catalog.rs` (67 → 71) and `catalog/validation.rs` (41 → 43) grows by the authored shape's
decimals and their range checks (authoring boundary, as for `radius`).

**Tests, red on WP3 first** (WP3 tree with WP4's tests): `a_footprint_shape_is_required`,
`a_footprint_shape_must_be_on_the_quarter_lattice` and `a_footprint_shape_must_lie_in_its_cells` fail ("loaded":
the field is ignored on WP3); `a_chamfer_must_leave_every_face` likewise; `a_footprint_shape_is_read_into_the_dome`,
R-16 `drawn_radius_is_the_shapes_smaller_half_extent` and `structure_contact_tests::a_spawned_structure_places_its_catalog_shape`
do not compile (no `shape` on the dome). All pass on WP4.

**Exit (D-15, 3)**, WP3 tools against WP4 tools: matrix bit-identical (1,160 runs); battle 33 `seeds.csv`
byte-identical; showcase `units.csv`, `summary.md`, `overlap.csv` byte-identical at 1,800 ticks, to the end and on
seeds 1 to 8; `golden_replay_tests` pass unchanged in debug, release and `wasm32-wasip1` release. The `sim_core`,
`sim_map`, `sim_types` and `workspace_checks` suites pass; the workspace builds and `cargo clippy --workspace
--all-targets` is clean.

## 5. WP5: consumer rules (full-square shapes)

**Change.** D-8: the centre rule (`SimWorld::is_position_pathable`) refuses a segment in a structure's cell only if the
segment meets the structure's closed shape; `ground_footprint_connected` (Blink, unload, Abduct, production egress)
keeps the square envelope against terrain, treats the ground under structures as walkable (D-5), and refuses a
segment whose disc of radius `t` meets a structure's open shape (`capsule_meets`, every distinct occupant of the
swept box except the exempt producer). D-9: `approach_point` is split into `approach_cell` (the one-cell shortcut,
for harvest targets and unload cells) and `approach_edge` (always the ring around the site, for construction's
approach and `leave_footprint`).

**Tests, red on WP4 first**:

| Test | WP4 | WP5 |
|---|---|---|
| C-1 `contour_footprint_tests::a_centre_may_cross_a_cut_corner_cell` | Fails: the segment through the 5×5 contour's corner cell, outside the shape, is refused | Passes |
| C-2 `contour_footprint_tests::a_worker_builds_a_one_cell_structure` | Fails for a builder **standing on** the one-cell site: `leave_footprint` sends it to the site's own centre, so it never gets off (H-1 reproduced). A builder walking from (12.5, 12.5) passes on WP4 too: it comes within reach before the centre, so the test checks both cases | Passes both |
| S-9 rewritten `structure_contact_tests::no_placement_is_refused_except_by_the_disc_against_the_shape` (64 cases, full and contour shapes of the 2×2, 3×3 and 5×5, Spore-sized and geyser contours; points on the Q16 grid; an independent `i128` oracle) | Fails at a full square's corner, where the square envelope refuses a point the disc admits | Passes |

Guards S-1 to S-8 and S-10 pass unchanged.

**Audit (R-8)** of `grep -rn "OCCUPIED_BY_STRUCTURE\|occupying_structure\|GROUND_PATHABLE" crates/sim_core/src
crates/client/src` (35 hits): Shape (C1, C2): `movement/flee.rs` 181–186, `movement/geometry.rs` 73 (both changed
here). Place (P1): `economy/mod.rs` 509–510 (`cell_is_walkable`, the approach ring), `economy/harvest.rs` 51 (node
cell's occupant), `production/lifecycle.rs` 263–266 and 363–364 (the producer's own foundation cells),
`scenario.rs` 371–374, 451–454, 899–908 and `scenario/placement.rs` 164–168 (scenario start rings and placement),
`world/vision.rs` 266 (`known_blocked_cells`), `client/src/battle_input/context_order.rs` 164 (picking a structure
by its cell). Map construction and flags only, no reader: `scenario.rs` 973, `scenario/placement.rs` 346,
`steering.rs` 1495 and 1576 (test maps), `checksum/encoding.rs` 630 (hashing), `client/src/editor/tools.rs`
(painting), `client/src/terrain/fields.rs` 200–201 (terrain texture, structures excluded).

**Exit (D-15, 4)**, WP4 tools against WP5 tools: matrix bit-identical (1,160 runs); battle 33 `seeds.csv`
byte-identical; showcase `units.csv`, `summary.md`, `overlap.csv` byte-identical at 1,800 ticks, to the end and on
seeds 1 to 8; `golden_replay_tests` pass unchanged in debug, release and `wasm32-wasip1` release. No Blink, unload,
Abduct or egress near a square corner occurred in these runs, so no difference needed a trace. The `sim_core`
suite passes.

## 6. WP6: stopped at the showcase acceptance (a stall at a chamfer), Proposed Amendment 2 below

WP6 is **not committed** to `contour-footprints`. Its work (the 49 shapes and four B-138 rows, D-10's loader rule,
the structure terrain radius change below, the WP6 tests and test edits, and WP7's two client tests) is kept on the
side branch `contour-footprints-wp6-wip`.

**Change on the branch.** The 49 `footprint_shape` values of §2.3 and the four B-138 rows (Sensor analogue 1×1 at
radius 0.75; Spire and Greater Spire analogues 2×2 at 1.125; Spore analogue shape (0.5, 0.5, 0.25) at 0.875), each
with its UnitData and Footprint lines (generated from the verified data by
`~/scratch/contour-footprints/scripts/wp6_data.py`). D-10: `validate_radius` requires the radius to cover the shape
instead of the floor.

**A conflict the plan did not foresee (decision recorded, Q0002).** The plan keeps structure-radius D-5 (a
structure's terrain radius is half its footprint's smaller side). For the Spore analogue that is 1.0 on its 2×2
cells, above its new radius 0.875, which breaks the invariant `terrain_radius ≤ collision_radius` (the first tick
reports `MovementInvariant … "terrain radius must be positive and at most the collision radius"` for the
structure). On the branch a structure's terrain radius is its shape's smaller half-extent: unchanged for every
full-box shape (all 45 others), 0.5 for the Spore analogue. It is still at most half the footprint (the map-edge
domain of R-12) and at most the radius (which covers the shape). No rule reads it.

**Tests on the branch, red on WP5 first**: `b134_footprint_shapes_follow_starcraft` (49 rows), the four B-138 rows of
`b135_structure_radii_follow_starcraft`, `structure_radii_follow_the_engine_rule` (authored = E and E covers the
shape), loader `a_structure_radius_must_cover_its_shape` (0.75 admitted and 0.6875 refused on the 1×1; 1.8125
admitted and 1.75 refused on a 3×3 cut by 0.5), R-5 `every_shipped_structure_contour_is_in_melee_reach`, and C-3 to
C-8 in `contour_footprint_tests.rs`; all pass with the data. C-3 asserts that the Zergling-sized body never reaches
x − y ≥ 0. The plan's "never crosses x + y = 24" names the anti-diagonal through the gap; the wall that separates
start and goal is the diagonal x = y.

**Test edits on the branch, each re-derived:**

- `structure_radius_tests`' scratch structures (R-1 to R-9) were written for full-square footprints at the control
  radius 1.5 and the engine radius 1.8125. The cover rule refuses both through the loader with the shipped
  contours, so the copy now keeps a full-square shape, loads with a covering radius, and gets the radius under test
  in memory. The tests' setups and assertions are unchanged, and all 10 pass.
- The catalog loader tests' RON helpers rewrote every other row to the old full-square radius or shape, which the
  cover rule refuses. They now keep every other row as shipped. `a_structures_terrain_radius_is_half_its_footprints_smaller_side`
  is renamed `…_is_its_shapes_smaller_half_extent`, with the same assertion for the 5×5.
- `catalog_reference_values_tests::only_ground_units_with_a_smaller_inner_radius_author_one`: a structure's terrain
  radius is its shape's smaller half-extent and at most its radius (was half its footprint).
- C7 allowlist: `catalog.rs` 70, `catalog/validation.rs` 42 (ratchet).

None of the plan's "expected test edits" (`production_transactions_tests`, `harvest_income_tests`,
`full_match_tests`, `match_result_tests`, `structure_contact_tests` S-1 and S-4) failed. The whole `sim_core` suite
otherwise passes.

**Measurements** (WP5 tools against WP6 tools):

| What | Result |
|---|---|
| Matrix (structure-free) | Bit-identical (1,160 runs) |
| Battle 33 `seeds.csv` | Byte-identical |
| `golden_replay_orders` | Passes unchanged |
| `golden_replay` | Changes from checkpoint 1 (20 of 20 checkpoints). The scratch entity-only digest (entities, projectiles, players, effects) is **identical on all 1,000 ticks** between WP5 and WP6: only the halls' hashed clearance differs. Not re-recorded, because the package stops |
| Control (a): WP6 with the seven hall contours reverted to (2.5, 2.5, 0), on a scratch build without the cover rule (which refuses a full 5×5 at 2.75) | Showcase `units.csv` byte-identical to WP5 on the default seed and seeds 1 to 8 |
| Control (b): the four B-138 rows' cells and radii reverted | Byte-identical to WP6 on all nine runs |

The hall contour alone moves the showcase.

| Seed | WP5 | WP6 | Reversals P0 / P1 | Stall give-ups P0 / P1 | Deeper |
|---|---|---|---|---|---|
| Default | P1 at 2,191 | P1 at 1,918 | 25 / 10 → 13 / 10 | 0 / 0 → **451** / 0 | 0 / 0 |
| 1 | P1 at 2,200 | P1 at 1,900 | 23 / 10 → 10 / 17 | 0 / 0 → **345** / 0 | 0 / 0 |
| 2 | P1 at 2,185 | P1 at 1,883 | 22 / 12 → 12 / 20 | 0 / 0 → **380** / 0 | 0 / 0 |
| 3 | P1 at 1,886 | P1 at 2,156 | 22 / 15 → 12 / 10 | 0 / 0 → **429** / 0 | 0 / 0 |
| 4 | P1 at 5,700 | P1 at 2,575 | 11 / 11 → 17 / 11 | 0 / 0 → **395** / 0 | 0 / 0 |
| 5 | P1 at 1,580 | P1 at 1,898 | 22 / 20 → 12 / 10 | 0 / 0 → **469** / 0 | 0 / 0 |
| 6 | P1 at 3,419 | P1 at 2,142 | 19 / 23 → 12 / 15 | 0 / 0 → **404** / 0 | 0 / 0 |
| 7 | **P0** at 2,552 | **P1** at 2,159 | 11 / 10 → 17 / 5 | 0 / 0 → **376** / 0 | 0 / 0 |
| 8 | P1 at 1,712 | P1 at 1,892 | 40 / 14 → 12 / 19 | 0 / 0 → **383** / 0 | 0 / 0 |

**Stop (D-15, 5).** The default-seed winner is unchanged, but acceptance fails: stall give-ups 451 against a bound of
base + 1. Seed 7's winner also flips (D-15: "a winner flip anywhere stops the package").

**Trace (default seed).**

- **First divergence, tick 0.** Player 0's starting pulse bots spawn on slightly different ring points beside the
  hall: entity 5 at (8.5664, 5.3929) instead of (8.6218, 5.5323). Scenario placement validates each point with
  the disc against the shape (C2, D-8), which admits points nearer the hall's cut corners. This is rule-caused.
- **The give-ups.** All 451 give-ups are `PursuitAbandoned` of player 0's two workers (`welder_drone`, entities 2
  and 3). A scratch `eprintln` in `BlockingMeter::observe` (not committed) names each one; the first is at tick 946.
- **What the workers do on WP5.** From tick ~932 they chase player 1's attackers east along the hall's south face
  (y ≈ 5.67, x 7.9 → 11.4 by tick 1,160), with no route (route length 0, a direct pursuit), and are killed at
  ticks 1,160 and 1,200.
- **What they do on WP6.** They walk north-east into the hall's cut south-west corner and stop pressed against the
  45° chamfer x + y = 13.5 (the hall covers cells 6 to 10), at (6.64, 6.36) and (7.20, 5.84). Their gap to the
  chamfer is ≈ 0.36 against a terrain radius of 0.3125.
- **Why they stall.** The chamfer faces almost straight along their heading to the targets at (14–16, 11–14).
  The wall-slide of their direct step is a sliver, and B-113's rule waits a sliver out. So they gain no ground, the
  pursuit is abandoned, they re-acquire, and this repeats about every two to four ticks until the match ends. They
  survive the match, and the hall falls earlier (1,918).
- **Classification.** Against the square, the same heading met the south face at about 45°, so the slide was large.
  This is the plan's **defect** class "a stall … at a chamfer", not a rule-caused change. Every seed shows the same
  give-ups (345 to 469), and seed 7's flip comes from the same stall.

### Proposed Amendment 2 (for the orchestrator): a pursuit that stalls against a wall takes a route

**Decision asked.** How a direct (route-less) pursuit leaves a wall that faces its heading. Contours put 45° walls
in front of units that chase past a building's corner; a square never did.

- **Recommended:** when a pursuer's direct step is waited out against terrain or a structure (a sliver slide, not a
  body), it asks the mesh for a route to its target, as a move order's stall repath does, instead of being counted
  toward `PursuitAbandoned`. Route-following already rounds the chamfer at the unit's terrain radius (WP3, C-5).
  It needs its own trace of the pursuit code (`movement/locomotion.rs` pursuit band and stall counter) and a test
  (C-9: a worker chasing a target across a 5×5's cut corner reaches it), then WP6's measurements again.
- **Alternative:** limit the sliver-wait of B-113 to walls whose normal is within some angle of the step. This
  invents an angle constant (D-15, 7 forbids tuning), so not recommended.
- **Not proposed:** reverting the hall contour. It is the game's data (S3).

On approval, WP6 resumes from `contour-footprints-wp6-wip` with that change, the `golden_replay` re-record (header:
entity state unchanged through tick 1000, if it still holds), and WP6's measurements; then WP7 and WP8.

**Addendum to WP2 (found by `cargo gate quick` after the merge of main).** One more pin carries the world checksum:
`sim_fixtures/tests/fixture_tests.rs`' `DEFAULT_SANDBOX_CHECKSUM` (the default battle sandbox, which places three
structures per side). Schema 30 changed it; WP2's runs did not cover `sim_fixtures`' tests. The same proof holds: a
scratch build that does not encode the shape and keeps schema 29 reproduces the recorded `1_650_891_098_077_756_775`.
Re-recorded as `13_611_770_232_129_032_728` with that header.

## 6b. Amendment 2 traced: the stalled pursuit already asks the mesh, and the leash abandons it

Amendment 2 asked for the pursuit band and stall counter to be traced before the change. The trace shows that its
premise does not hold, so it was not implemented. The WP6 work is brought onto the branch head (which includes
2dc77880 and Amendment 2) on the side branch `contour-footprints-wp6-wip2` (`4a176a35`).

**The pursuit path** (`movement/locomotion.rs`, `chase`): each tick the chaser checks its progress
(`check_pursuit_progress`: `PursuitAbandoned` after `STALL_ABANDON_TICKS` without progress), then steps
(`step_toward`). When terrain or a structure refuses the step (`after_wall_slide` reports `refused`, a slide below
half the step), a ground chaser off cooldown calls `reroute_pursuit`. That **already searches a mesh route to its
target**, as Amendment 2 asked. It abandons the target when:

- the search is `Blocked`;
- a retargeted point is beyond the weapon's band; or
- for an automatically acquired target, the route is longer than the leash: `scan_range ×
  PURSUIT_LEASH_SCAN_RANGES` (2), a rule of plan `2026-11-09-friendly-blocking`.

**The probe** (scratch `eprintln`s in `reroute_pursuit` and the stall abandon, not committed; WP6 data, default seed,
1,100 ticks): the two workers' steps against the chamfer were refused 80 times. Every time the mesh **found** a route
round the hall (length 11.15 to 12.17; for example tick 946, entity 3 at (7.109, 5.871), target at (16.090,
11.346), route 11.2031). Every time it was longer than the leash of 10.0 (scan range 5), so the target was
abandoned. The stall-abandon path never fired.

| What happens | Square hall (WP5) | Contour hall (WP6) |
|---|---|---|
| Workers' direct heading at the hall corner | Meets the south face at about 45° | Meets the chamfer nearly head-on |
| Wall slide | Large, so the step is not refused | A sliver, so the step is refused |
| Leash check | Never runs: the direct chase follows the face east | Runs; the 11.2 route exceeds the 10.0 leash |
| Outcome | Workers chase and are killed (ticks 1,160 and 1,200) | Pursuit abandoned, the next attacker re-acquired; repeats until the match ends |

**Conclusion.** The give-ups are the leash rule meeting the contour, not a missing route. A direct chase that hugged a
square face never had its route length checked. The same chase meeting a chamfer head-on has it checked, and the
detour round a 5×5 hall from its cut corner is longer than two scan ranges. Implementing Amendment 2 as written would
change nothing. C-9 as specified (a worker chasing past a 5×5's cut corner reaches it) passes already whenever the
target is within the leash, and fails, by abandonment, when it is not.

### Proposed Amendment 3 (for the orchestrator)

**Decision asked.** What the automatic-chase leash measures. It now measures the route length after a refused step,
while an unrefused direct chase is never measured. That asymmetry is what the contour exposes.

- **(a) Recommended: the leash measures the target's distance, not the route's length.** An automatic chase is given
  up when the target is farther than `scan_range × PURSUIT_LEASH_SCAN_RANGES` from the pursuer, whichever way it
  walks; a route found after a refused step is adopted. Unrefused direct chases are unchanged. This needs a trace
  of who else reaches the leash today (matrix, goldens) and C-9 with a target within leash distance but beyond leash
  route length (red now).
- **(b)** Leave the leash and accept the outcome as rule-caused. The workers now give up a chase that a square let
  them continue; give-ups stay at about 450 per showcase, which D-15's acceptance (base + 1) forbids, so the bound
  would need to change. Not recommended.
- **(c)** Count a leash abandonment only once per target (no give-up re-counted on re-acquire). This hides the loop
  rather than removing it. Not recommended.

In StarCraft II, a unit chasing an automatically acquired target is leashed by distance from where it acquired it,
not by its route's length (not sourced here; **verify** when the amendment is written).

## 6c. Amendment 3 measured: the straight-line leash does not end the stall (Proposed Amendment 4)

**Who reaches the leash today** (Amendment 3, item 2). A scratch probe (not committed;
`~/scratch/contour-footprints/scripts/leash_probe.py`) prints every abandonment in `reroute_pursuit`: a found route
longer than the leash, or no route. On the branch head (WP5 code, full-square data):

| Run | Route-length abandonments | No-route abandonments |
|---|---:|---:|
| Matrix, 1,160 runs | 0 | 0 |
| Battle 33, 64 seeds | 0 | 0 |
| Showcase, default seed to the end | 0 | 0 |
| `golden_replay`, `golden_replay_orders` | 0 | 0 |

The leash is reached nowhere today, so changing how it measures cannot move any of these.

**StarCraft II source** (Amendment 3, item 3). The 5.0.14 data has no per-unit leash for an automatic chase.
`CAbilAttack` (core AbilData:115) has none; the only `Leash` values are the Carrier's interceptors (AbilData:2086) and
wander behaviours. One game-wide setting exists: `CGame` `AcquireMovementLimit` 21, `AcquireLeashRadius` 21 and
`AcquireLeashResetRadius` 5 (core `GameData.xml`:8–10 at `5444e12f13`, downloaded to
`~/scratch/contour-footprints/dl-gamedata/`). Community posts describe the leash as measured from where the unit
acquired its target ([SC2Mapster 172487](https://www.sc2mapster.com/forums/development/miscellaneous-development/172487-changing-a-units-pursuit-distance?comment=2),
[SC2Mapster 162386](https://www.sc2mapster.com/forums/data/162386-weapon-level-3-but-what-does-it-do/previous));
the data alone does not confirm the measure. Ours is twice the scan range (10 for the worker analogue), measured
from the pursuer. Registered as **B-142**; not widened here.

**Option (a) implemented and measured.** The leash compares the pursuer's straight-line distance to the target,
not the route's length. The showcase on the default seed is **byte-identical to WP6**: player 1 wins at 1,918 with
451 give-ups for player 0. The reason is in §6b's probe: the attackers the workers respond to stand 10.5 to 10.6
from them in a straight line (for example tick 946: entity 3 at (7.109, 5.871), target at (16.090, 11.346)). That is
already beyond the leash of 10.0 (scan range 5.0), so a straight-line measure taken when the step is refused also
gives them up. With the square hall the leash was never consulted, because the direct chase was never refused.
C-9 was not written, since the rule it would pin does not cure the stall. The work (WP6 and option (a)) is on
`contour-footprints-wp6-wip3` (`f1817b66`).

### Proposed Amendment 4 (for the orchestrator)

**Decision asked.** What leash an automatic chase gets, given that the workers chase attackers beyond twice their
scan range. Option (a) only changes which rule applies at 10.0, and both measures end at 10.0.

- **(1) Recommended: adopt 5.0.14's leash (fix B-142 in this package).** An automatic chase is given up when the
  pursuer is farther than `AcquireLeashRadius` 21 from where it acquired the target. The radius is authored in the
  catalog's movement rules, not a constant; whether `AcquireLeashResetRadius` 5 resets the origin needs a source.
  The probe shows no leash abandonment in the matrix, battle 33, the showcase or the goldens on today's data, so
  those stay bit-identical; only the contour showcase moves. Needs a source check of the measure, and C-9 with a
  target 10 to 21 away behind a 5×5's cut corner.
- **(2)** Accept the give-ups as rule-caused. The workers now meet the existing leash where a square let them chase
  on. This needs a different acceptance bound than base + 1 (D-15, 5), and seed 7's flip has the same cause.
- **(3)** Measure the leash never, only on a refused step, for a target that acquired the pursuer itself
  (retaliation). Narrower and invented; not recommended.

## 6d. Amendment 4: 5.0.14's leash of an automatic chase (B-142)

**Prototype first** (Amendment 4, condition 1). A scratch build of the WP6 work (`contour-footprints-wp6-wip2`)
gave an automatic chase up once the pursuer stood farther than 21 from where the chase began, routed or direct, and
adopted every route found after a refused step. On the WP6 showcase:

| Seed | WP6 (old leash) | Prototype |
|---|---|---|
| Default | P1 at 1,918; give-ups P0 / P1 451 / 0 | P1 at 1,946; give-ups 0 / 0 |
| 7 | P1 at 2,159; give-ups 376 / 0 | P1 at 2,187; give-ups 0 / 0 |

The workers' give-ups fall to 0 (base 0), so the work went on. Seed 7's winner is still player 1 against WP5's
player 0; it is traced with WP6's measurements (§6e).

**Sources** (condition 2). `CGame` `AcquireLeashRadius` 21 (core `GameData.xml`:9 at `5444e12f13`). No unit in the
5.0.14 `UnitData.xml` sets a per-unit leash: the editor shows one, "Movement: Acquire Leash Radius", and a value of
-1 uses the game's ([SC2Mapster 162386](https://www.sc2mapster.com/forums/data/162386-weapon-level-3-but-what-does-it-do/previous)).
Community posts describe the leash as measured from where the unit acquired its target
([SC2Mapster 172487](https://www.sc2mapster.com/forums/development/miscellaneous-development/172487-changing-a-units-pursuit-distance?comment=2)).
What `AcquireLeashResetRadius` 5 and `AcquireMovementLimit` 21 do is **not sourced**: searches found only the editor
labels "Combat: Acquire Leash Radius" and "Combat: Acquire Movement Limit". They are not modelled, and B-142's row
records them as a minor difference.

**The rule.** `MovementMemory.leash_origin` (hashed) holds where the current automatic chase began. It is set on the
chase's first tick, and cleared when the chase is abandoned, on an ordered attack, or on a new order (the memory's
`restart`). `chase` gives the target up (`PursuitAbandoned`) when the pursuer stands farther than
`movement_rules.ron` `pursuit.acquire_leash_radius` (21, required, whole world units ≥ 1) from it. `reroute_pursuit`
adopts any route it finds. `PURSUIT_LEASH_SCAN_RANGES` and `Pursuit.scan_range` are removed. Ordered attacks are
unchanged. Schema stays 30 (not yet on main).

**Who it reaches** (condition 3). The leash-only tree (branch head plus the rule, full-square data), with the new
field left unhashed so behaviour is compared directly:

| Run | 21-leash abandonments | Against WP5 |
|---|---:|---|
| Battle 33, 64 seeds | 0 | `seeds.csv` byte-identical |
| Showcase, default seed and seeds 1 to 8 | 0 | `units.csv` byte-identical |
| `golden_replay`, `golden_replay_orders` | 0 | Pass unchanged |
| Matrix, 1,160 runs | 6 | Only `35_12_vikings_vs_6_colossi` moves |

Battle 35 moves on 4 of its 16 seeds: the seed of record, 9, 10 and 15. In each, Viking analogues follow the last
Colossus analogue as it walks off and give the chase up 21 from where they began; for example, tick 547, entity 3 at
(54.65, 58.88), origin (37.56, 46.48). The battle then runs to its 900-tick limit with that one Colossus alive,
instead of ending with its death at ticks 566 to 607. Blue keeps all 12 Vikings and the expectation is still met;
no winner changes. This follows from the 5.0.14 leash, so it is rule-caused (Amendment 4, item 3). Before, a direct
chase was never measured.

**Tests.**

| Test | Before (wip2 code) | After |
|---|---|---|
| C-9 `contour_footprint_tests::a_worker_called_from_a_cut_corner_reaches_an_attacker_behind_the_hall` (a shipped worker in a shipped 5×5's cut corner, called to help against a shooter 10 to 21 away beyond the hall) | Fails: 40 `PursuitAbandoned` | Passes: reached, 0 abandoned |
| C-10 `movement_contact_tests::an_automatic_chase_is_given_up_beyond_its_leash` (an enemy acquired beside a melee unit walks away) | Fails: the chase is never given up | Passes: given up within one step of 21 |
| `movement_rules::tests::the_leash_is_read_and_must_be_positive` and the missing-section cases | New | Pass |

C-10 was first written with the enemy across a long wall. Acquisition never chose that target, so the chaser never
moved and that setup could not test the leash. It was re-derived as an enemy that walks away. The existing
`movement_contact_tests::acquired_attack_without_a_short_route_is_dropped` passes unchanged.

**Test edits.** `group_routing_tests::g11_movement_rules_fail_loudly_with_the_path` writes `movement_rules.ron`
texts; they gain the now-required `pursuit` section, so the tolerance cases still reach their check.

**Pins re-recorded** with proof headers: `golden_replay`, `golden_replay_orders` (every checkpoint) and
`DEFAULT_SANDBOX_CHECKSUM` (`17_544_490_958_066_995_222`). A scratch build that leaves `leash_origin` unhashed
reproduces each previous value, and the 21-leash fires nowhere in them.

## 6e. WP6 redone under D-15 with the leash of Amendment 4

**Change** (WP6, on top of the leash commit `f0086939`): the 49 5.0.14 shapes and four B-138 rows of §6, D-10's
cover rule, a structure's terrain radius as its shape's smaller half-extent (confirmed by Amendment 2), the WP6
tests and test edits of §6, and the cross-reference of that terrain radius in plan `2026-11-18-structure-radius`.
The base for every comparison is the leash commit with full-square data. On full squares it equals WP5 everywhere
except the matrix's battle 35 (§6d).

| What | Result |
|---|---|
| Matrix (`--compare` the leash commit's matrix) | Bit-identical (1,160 runs) |
| Battle 33 `seeds.csv` | Byte-identical |
| `golden_replay_orders` | Passes unchanged |
| `golden_replay` | Changes from checkpoint 1. The scratch entity-only digest is **identical on all 1,000 ticks** between the leash commit and WP6. Re-recorded with the header the plan requires; passes debug, release, `wasm32-wasip1` release and debug |
| `sim_fixtures` `DEFAULT_SANDBOX_CHECKSUM` (the sandbox places a hall, a depot and a production building) | Changes. Every entity's position is identical; a scratch run with the full-square catalog (cover rule off) reproduces the previous `17_544_490_958_066_995_222`. Re-recorded as `15_355_001_956_314_655_944` with that header |
| Control (a): the hall contours reverted, scratch build without the cover rule | Showcase `units.csv` byte-identical to the base on the default seed and seeds 1 to 8 |
| Control (b): the four B-138 rows' cells and radii reverted | Byte-identical to WP6 on all nine runs |

**Showcase** (to the end; reversals and stall give-ups per owner, P0 / P1; deeper unit-ticks 0 / 0 on every run):

| Seed | Base | WP6 | Reversals | Stall give-ups |
|---|---|---|---|---|
| Default | P1 at 2,191 | P1 at 1,946 | 25 / 10 → 13 / 10 | 0 / 0 → 0 / 0 |
| 1 | P1 at 2,200 | P1 at 1,928 | 23 / 10 → 10 / 17 | 0 / 0 → 0 / 0 |
| 2 | P1 at 2,185 | P1 at 1,913 | 22 / 12 → 12 / 20 | 0 / 0 → 0 / 0 |
| 3 | P1 at 1,886 | P1 at 2,194 | 22 / 15 → 12 / 10 | 0 / 0 → 0 / 0 |
| 4 | P1 at 5,700 | P1 at 2,616 | 11 / 11 → 18 / 11 | 0 / 0 → 0 / 0 |
| 5 | P1 at 1,580 | P1 at 1,924 | 22 / 20 → 12 / 10 | 0 / 0 → 0 / 0 |
| 6 | P1 at 3,419 | P1 at 2,175 | 19 / 23 → 12 / 15 | 0 / 0 → 0 / 0 |
| 7 | P0 at 2,552 | P1 at 2,187 | 11 / 10 → 17 / 5 | 0 / 0 → 0 / 0 |
| 8 | P1 at 1,712 | P1 at 1,921 | 40 / 14 → 12 / 19 | 0 / 0 → 0 / 0 |
**Acceptance (D-15, 5) on the default seed holds.** The winner is the same (player 1), the match ends, deeper
unit-ticks are 0, stall give-ups are 0 / 0 (bound base + 1) and reversals are 13 / 10 (bound base + 10). Every
seed's give-ups are 0, and every seed's reversals are within base + 10.

**Expected direction, confirmed.** Every seed first diverges at tick 0. Player 0's starting pulse bots are placed on
slightly different ring points beside the hall (entity 5 at (8.5664, 5.3929) instead of (8.6218, 5.5323)): scenario
placement checks each point with the disc against the hall's contour (D-8). Matches end 27 to 3,084 ticks earlier or
later; on the default seed the hall falls at 1,946 instead of 2,191.

**Seed 7 changes winner (player 0 at 2,552 → player 1 at 2,187).**

- The cause is the same tick-0 placement, which is rule-caused (D-8, S3).
- Its lane fight is close. On the base, player 0 leads 10 to 7 at tick 500 and keeps 5 units to player 1's 2 by tick
  700. On WP6, the shifted starts give 8 to 8 at tick 500 and 2 to 6 by tick 700.
- There is no stall, wedge or fallback: give-ups 0 / 0, deeper 0.
- Seed 7 was player 0's only win of the nine. Reported, as D-15 rule 5 asks for seeds 1 to 8. The default seed's
  winner is unchanged.

**A finding outside this plan (B-143).** One full `sim_map` run failed
`pathfinding_properties::mesh_routes_agree_with_the_exact_oracle`: a random terrain mask, radius 0.26, route
(11.5, 12.5) to (6.5, 7.5), ratio 1.270 against `MESH_RATIO_BOUND`.

- It fails identically with the fitted retry disabled, and identically on `main` `739ca6ea`, so it predates the plan.
- The seed proptest appended to the tracked regressions file was taken out again (it would fail every run).
- Registered as B-143. Its shrunk input:

```text
cc d51620938f4e0748a6e9240a09b12b64150e7a508147ffce1700eb50640e16cb # shrinks to blocked = [false, false, false, false, false, false, true, false, false, false, false, true, false, false, false, false, false, true, false, false, false, false, false, false, true, true, false, false, false, false, true, false, false, false, false, false, false, false, false, true, true, false, false, true, true, false, false, false, false, true, false, false, false, false, false, true, false, false, false, true, false, false, false, false, false, false, true, false, false, false, false, false, true, true, false, false, false, false, false, false, true, false, false, true, false, false, false, true, false, false, false, false, false, true, false, false, false, true, true, false, false, true, false, false, false, false, true, false, false, false, false, true, true, false, false, false, false, true, false, true, false, false, false, false, true, true, false, false, false, false, false, false, false, true, false, false, true, false, false, false, false, false, false, true, false, true, true, false, false, true, true, false, false, false, false, false, false, false, true, false, false, false, false, false, false, false, false, true, false, false, false, true, false, false, false, false, true, false, true, false, false, false, true, false, false, false, false, true, true, false, false, true, false, false, true, false, false, false, false, false, false, false, false, false, false, false, false, false, true, true, false, false, false, false, false, false, false, false, false, false, false, true, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, false, true, false, true, false, false, false, false, false, false, false, false, true, false, true, false, false, true, true], radius_steps = 1, picks = [(1, 319), (967, 560), (956, 91), (796, 31)]
```

## 7. WP7: client, presentation, measurement

**Client tests.** `effects::catalog::units::a_structures_muzzle_is_its_drawn_dome` (revised to the shape: 1.0 for an
armed 2×2 contour, 0.5 for the Spore analogue) and `battle_input::command_mode::a_one_cell_footprint_centres_on_the_clicked_cell`
pass on WP6's tree. The client reads the drawn size through `drawn_radius` (the shape) and the cells through
`footprint_w/h`, so neither test needed client code. One test edit: the Spore analogue is armed only against air, so
the muzzle test now accepts an air or a ground weapon when it looks for an armed structure of a size. `grep -rn
"footprint_w\|drawn_radius\|BuildingDome {" crates/client/src` finds only the editor's placement size (cells) and
`matches!` on the dome kind; no client code changed (D-11). `drawn_radius`'s doc comment now names the shape.

**Picture check** (radv, RX 580, one `with-gpu-lock` hold per take, at least 60 s between holds). A scratch assets tree
(`~/scratch/contour-footprints/pic/assets`, symlinks to `assets/` plus a flat 48×48 map, three scenarios and three
loadouts) and the scratch storyboard `~/scratch/contour-footprints/pic/picture.ron` place, at tick 0, a 5×5 hall, a
2×2 dome (`capacitor_dome`) or the 1×1 sensor (`radar_beacon_dome`) with pulse bots on its first ring, seen from
above and from the side (frames in `~/scratch/contour-footprints/pic/frames/`). Observations:

- The hall's dome is drawn at 2.5; the ring's corner units stand outside it, in the cut corners, as the contour
  allows. The 2×2 dome is drawn at 1.0 with units at all four corners; the sensor at 0.5, its ring 0.125 from its
  glass. Nothing reads as a fault.
- **Not pictured, recorded.** A scenario cannot place two structures corner to corner (start locations must be 16
  cells apart, `MIN_START_SEPARATION_CELLS`), and no scenario order moves a worker, so the "worker between two 2×2
  domes" picture needs a build order in a scripted take. The gap itself is proven by `wall_off_tests` and C-3. The
  Spore and Spire analogues (`spore_spire_dome`, `chitin_spire_dome`, `apex_spire_dome`) have no admitted media, so
  they cannot be drawn without faking a dome (charter §1.2). Both are left out of scope (Q0002).

**Storyboard** (the default seed's timeline moved, §6e). The `battle` take rendered on radv (182 s, revision
`a7ef86fc`): `advance` 251 to 305 (first shot), `clash` 305 to 680, past the lane fight's last
death (tick 621), `march` 683 to 937 at 4x, ending on combat at the viewer's base, and `assault` from 937 at 2x
over the workers' deaths (1,010 and 1,038). The enemy's five surviving pulse bots fire on the viewer's hall, which
falls at 1,946. No scene length changed; the storyboard's comment gains the re-check.

**XML sources of §2.3** (`FootprintData.xml`, 5.0.14 liberty, the `Shape` offsets of each footprint):

```text
Footprint:2016  <Offsets value="-1,-2.5;-2.5,-1;-2.5,1;-1,2.5;1,2.5;2.5,1;2.5,-1;1,-2.5"/>
Footprint:2262  <Offsets value="1,-2.5;-1,-2.5;-2.5,-1;-2.5,1;-1,2.5;1,2.5;2.5,1;2.5,-1"/>
Footprint:1936  <Offsets value="-1.5,-1;-1.5,1;-1,1.5;1,1.5;1.5,1;1.5,-1;1,-1.5;-1,-1.5"/>
Footprint:2206  <Offsets value="-1,-1.5;-1.5,-1;-1.5,1;-1,1.5;1,1.5;1.5,1;1.5,-1;1,-1.5"/>
Footprint:960  <Offsets value="-0.75,-1.5;0.75,-1.5;-0.75,1.5;0.75,1.5;-1.5,-0.75;-1.5,0.75;1.5,-0.75;1.5,0.75"/>
Footprint:1951  <Offsets value="-1,0.5;-0.5,1;0.5,1;1,0.5;1,-0.5;0.5,-1;-0.5,-1;-1,-0.5"/>
Footprint:5370  <Offsets value="-0.5,-1;0.5,-1;-0.5,1;0.5,1;-1,-0.5;-1,0.5;1,-0.5;1,0.5"/>
Footprint:163  <Offsets value="-0.25,-0.5;0.25,-0.5;-0.25,0.5;0.25,0.5;-0.5,-0.25;-0.5,0.25;0.5,-0.25;0.5,0.25"/>
Footprint:1107  <Offsets value="-0.5,-0.5;-0.5,0.5;0.5,0.5;0.5,-0.5"/>
Footprint:2074  <Offsets value="0.5,-1;-0.5,-1;-1,-0.5;-1,0.5;-0.5,1;0.5,1;1,0.5;1,-0.5"/>
Footprint:1239  <Offsets value="1,-0.5;0.5,-1;-0.5,-1;-1,-0.5;-1,0.5;-0.5,1;0.5,1;1,0.5"/>
```

The control runs are in §6e (controls (a) and (b)) and the corner-gap property's table in §3b
(`a_diagonal_contour_gap_admits_exactly_the_discs_that_fit`, `8t² ≤ 1`). The mesh rebuild of the 24-structure row is
in §3b: 11,125 µs square against 11,341 µs contour (+2 %), below the 25 % bar, so no profile was needed.

**Throughput** (`cargo bench -p sim_core --bench throughput`, release, each run under `flock ~/scratch/gate.lock`,
base `main` `739ca6ea` against the candidate `a7ef86fc`, alternated over three rounds; ticks per second, each run's
median of 3, and the median of the three rounds). The shared machine was loaded by another agent's GPU tests
throughout: base round 1 ran under that load, and base round 3 (rerun after its first try lost its assets) shows it
on the showcase and caster rows.

| Scenario | Base rounds | Base | Candidate rounds | Candidate |
|---|---|---:|---|---:|
| 64x64, 0 units | 2,066 / 3,242 / 3,155 | 3,155 | 3,276 / 3,236 / 3,267 | 3,267 |
| 64x64, 100 | 472 / 947 / 745 | 745 | 944 / 955 / 949 | 949 |
| 64x64, 400 | 158 / 162 / 154 | 158 | 164 / 165 / 143 | 164 |
| 128x128, 400 | 91 / 192 / 185 | 185 | 192 / 195 / 184 | 192 |
| 256x256, 0 | 134 / 206 / 192 | 192 | 205 / 206 / 197 | 205 |
| showcase_canyon, 400 | 119 / 121 / 85 | 119 | 120 / 124 / 114 | 120 |
| 64x64, 67 casters, 400 | 219 / 233 / 164 | 219 | 238 / 237 / 216 | 237 |

Every scenario stays far above 25 ticks per second; the candidate is not slower than the base on any row.

## 8. Amendment 5: B-140 fixed in the test corpora; B-143 is a test defect

**Audit of every RNG draw (item 1).** Every `gen_range`, `gen`, `gen_bool`, `choose`, `shuffle`, `next_u32`,
`next_u64` and `sample` in `sim_types`, `sim_map`, `sim_core` and `sim_fixtures` (sources, tests, examples, benches)
was listed and typed. **No platform-width draw in simulation code.** The simulation draws twice, both over `i64`:
`sim_core/src/combat.rs` `charge_random_delay` (`min.raw()..=max.raw()`) and `sim_core/src/world/abilities.rs`
`mode_channel_ticks` (`0..=max.raw()`). `sim_fixtures` draws nothing. The `usize` draws were all in tests:

| File | `usize` draws | Pinned? |
|---|---|---|
| `sim_map/tests/validation_golden_tests.rs` | every index into a collection or a value table (cells, items, entity IDs, start locations, rows, columns, invalid characters, widths, heights, terrains, ramps, flags, player slots, tower radii, the path corpus's radius) | yes: validation, decode, path |
| `sim_map/tests/pathfinding_properties.rs` | `mesh_route_lengths_are_within_one_percent_of_the_oracle`, the pair's two centre indices | no (thresholds) |

Every other test draw was already a fixed width by inference (`i32` literals, `u32` map sizes, `u8` levels and
footprints, `i64`, `f64`).

**The fix (item 2).** Every `gen_range` and `gen` in those crates now names its type with a turbofish
(`rng.gen_range::<u32, _>(0..n)`), which changes no draw except the `usize` ones. An index is drawn by
`sim_map/tests/support/draw.rs` `draw_index`, a `u32` range converted after the draw. One addition, recorded as a
decision: on the fixed corpus no seeded document has unreachable starts (the old `wasm32` corpus had none either), so
the test's coverage assertion for check 22 failed. The validation corpus gains a fourth fixed document, the default
arena walled in two by an abyss band on rows 20 and 21, so check 22 is reached by construction rather than by a seed.

| Pin | Before | After | Native debug | Native release | `wasm32-wasip1` release |
|---|---|---|---|---|---|
| `VALIDATION_GOLDEN` | `0xc8fc_3cbd_495d_5177` | `0x3527_8c7f_a705_ba2c` | same | same | same |
| `DECODE_GOLDEN` | `0x56c4_3eea_8701_94bd` | `0x4c32_25d2_ef3a_fc87` | same | same | same |
| `PATH_GOLDEN` | `0xa1a3_327e_2a30_f8b7` | `0xa243_924f_8125_f760` | same | same | same |
| `CONTOUR_PATH_GOLDEN` (unchanged) | `0x9069_95c7_29b7_3498` | `0x9069_95c7_29b7_3498` | same | same | same |

Validation counts, identical on all three: 13 and 14 together in 15 reports, unreachable in 1, 2,496 warnings. Logs:
`~/scratch/sim-map-corpora/native-debug-3.log`, `native-release-3.log`, `wasm-release-3.log`.

**The old validation-count difference (item 3) was a different corpus.** Before the walled document was added, the
fixed-width corpus gave, natively, decode `0xef65_8b0a_0d4e_7fe9` and path `0xa243_924f_8125_f760`, exactly B-140's
`wasm32` values, and validation `0x2245_2ac5_06b6_d828` with unreachable in 0 reports and 2,487 warnings, exactly the
`wasm32` counts. The old test file run today on `wasm32-wasip1` release gives that same `0x2245_2ac5_06b6_d828`
(`wasm-old-validate.log`) and natively the old pin `0xc8fc_3cbd_495d_5177` with 4 and 2,362
(`native-old-validate.log`). So `wasm32` validated the corpus that `u32` draws produce, and the native counts (4
against 0, 2,362 against 2,487) came from the native corpus's different documents; no `validate_map` difference
remains. B-140's recorded `wasm32` validation value `0x01e3_8765_0693_1dd7` predates WP2's D-5 rule, which changes
the text of 17 reports (`UnbuildableTerrain` to `UnpathableTerrain`) but no count.

**Guard (item 4).** Charter check C25 (`crates/workspace_checks/src/rng_draws.rs`, listed in
`docs/guides/charter-checks.md`): every `gen_range` and `gen` call in `sim_types`, `sim_map`, `sim_core` and
`sim_fixtures`, tests, examples and benches included, names its type with a turbofish, and that type is not `usize` or
`isize`. Its unit tests plant untyped, `usize` and `isize` draws (reported) and typed draws, comments, literals,
definitions and other crates (passed). Its limit: it reads text, so a draw through a generic helper whose caller
picks the type, or a `rand::distributions` object, is a reviewer's call; neither is used in these crates.

**B-143 traced: a test defect, in the oracle's smooth length.** Scratch trace (the shrunk mask of §6e, radius 0.26,
(11.5, 12.5) to (6.5, 7.5), `rho` = 0.31):

| Quantity | Value |
|---|---|
| Mesh route turns | (11, 12), (8, 9), (7, 8), all on the left; the three corners lie on one line, `y = x + 1` |
| Mesh polyline | 7.2069 (not a fallback, 18 expansions; the reverse route is its mirror) |
| Oracle at `r + margin` / at `r` | 7.2093 / 7.1678 |
| `smooth_length` of the turns | 9.1571, ratio 1.270 |
| `smooth_length` − oracle | 1.9478 = 2π · 0.31, one whole circle |

The middle turn sweeps nothing: the route leaves the circle round (8, 9) along the same tangent it arrived on.
`smooth_length` computed that sweep as `atan2` of the outgoing tangent point minus that of the incoming one, which
rounding left a hair below zero, and `rem_euclid(2π)` turned it into a full circle. At radii 0.25, 0.27 and 0.30 the
same three corners give a ratio of 1.0000 (the rounding falls the other way), so the route and the routing are right;
only the measure was wrong. Fix (`sim_map/tests/support/oracle.rs` `turn_sweep`): an angle within 10⁻⁶ rad of a full
turn counts as no sweep. Deterministic regression test:
`pathfinding_properties::a_straight_pass_round_collinear_corners_sweeps_no_circle` (the §6e case; ratio 1 within
10⁻⁹, and the route still turns round the three collinear corners). With `turn_sweep` reverted it fails with ratio
1.2701775. `funnel_tests` (the other user of `smooth_length`) and every `pathfinding_properties` test pass with the
fix. Nothing in `sim_map/src` changed.
