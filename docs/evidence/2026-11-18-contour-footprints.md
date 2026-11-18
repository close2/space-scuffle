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
