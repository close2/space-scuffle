# Blueprint: Contour Footprints and 5.0.14 Footprint Sizes (B-134, B-138)

This plan fixes two recorded bugs by following StarCraft II 5.0.14's footprint data:

- **B-134.** Our footprints are full squares of cells. In 5.0.14 the pathing shape of a footprint (`Shape` in `FootprintData.xml`) cuts the corners: the 5×5 by 1.5, the 4×4 by 1, the 3×3 and 2×2 by 0.5, and geysers by 0.75. Because of this, units keep further from our buildings' corners than they do in the game. Two buildings placed corner to corner also form a tight wall, where the game leaves a diagonal gap of `1/√2 ≈ 0.707` (Q0087, the owner's wall-off argument).
- **B-138.** Four footprints are the wrong size or shape. The radius floor of plan `2026-11-18-structure-radius` (its D-4) exists only because of these four, so it goes too:
  - Sensor Tower analogue: 2×2 here, 1×1 in the game.
  - Spire and Greater Spire analogues: 3×3 here, 2×2 in the game.
  - Spore Crawler analogue: its collision shape is its full 2×2 here; the game's is a 1×1 octagon inside the 2×2.

**Correction found while planning (§2.3).** The register's fifth B-138 item, "Hatchery, Lair and Hive 5×5 against 6×5", misreads the data. `Footprint6x5DropOffCreepSourceContour` has 5×5 `Check` and `Place` layers and the 5×5 octagon as its `Shape` (Footprint:2238–2264). Its placement twin `Footprint6x5DropOffCreepSource` is also 5×5 (Footprint:2210–2236). "6x5" appears only in the name. That item is withdrawn in WP0; it is not a deviation.

A full plan is required (charter §6). The change touches the navigation mesh, the hashed clearance field and the checksum schema. It moves structure worlds (the showcase, and `golden_replay` through its hashed clearance). It spans `sim_types`, `sim_map`, `sim_core`, the catalog, the client presentation and the evidence.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-18 against `main` at `10e747bf`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0087 (Zergling `InnerRadius`), closed: 0.375; the owner's diagonal wall-off argument | Closed. Its geometry is this plan's acceptance test | §2.2, D-14, WP3, WP6 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if written down | Standing rule | §10 |
| Q0083, Q0088 (GPU driver, fair lock queue) | Touch only WP7's renders, which follow the guide's 60 s rule | WP7 |
| **None new** | The data decides every rule (Q0051). D-5 (ground under a structure is walkable) and D-11 (the Spore dome's drawn size) are recorded scoping decisions, not owner choices | §10 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| What a structure occupies for **placement** (other buildings, construction sites, build and harvest reach, exit rings, knowledge mask, client ghost) | Its cells, `footprint: (w, h)` | Unchanged: the game's `Place` layer. The Sensor Tower analogue becomes 1×1 and the Spire analogues 2×2 (B-138) |
| What a structure occupies for **pathing and collision** (navigation mesh cut, routes, contact rule, centre rule, unit placement, production egress, clearance field) | Its cells | Its 5.0.14 `Shape`: a box centred on the cells with chamfered corners, authored as `footprint_shape` in `structures.ron` (required) |
| Mesh lattice | Whole cells | Quarter cells (the Spore and geyser shapes need 1/4). The mesh of every structure-free map, and of every map whose structures have full-square shapes, is identical |
| Two 2×2 or 3×3 contour buildings corner to corner | Tight wall | A 0.707 gap: workers (terrain radius 0.3125) pass, the Zergling analogue (0.375) does not |
| Structure radius rule (loader) | Floored at half the footprint's smaller side (structure-radius D-4) | Must cover the shape: `radius² ≥` the shape's farthest vertex distance². Every analogue authors its engine radius E. The four floors go (Sensor 0.75, Spore 0.875, Spire and Greater Spire 1.125) |
| Drawn dome | Half the footprint's smaller side | Half the shape's smaller extent. Unchanged for 45 structures; Spore 1.0 → 0.5, Sensor 1.0 → 0.5, Spires 1.5 → 1.0 |
| Hashed state | — | `PlacedStructure.shape` is hashed, so the checksum schema goes 29 → 30. Both goldens are re-recorded once with a control proof (WP3) |
| Expected impact | — | Structure-free worlds stay bit-identical: matrix, battle 33, `golden_replay_orders` (apart from the schema re-record). `golden_replay` changes from checkpoint 1 at the data step (the halls' clearance is hashed). The showcase moves: units round the halls' cut corners |
| Size | — | L: nine packages, two of them M |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`).
  - "Footprint:N" is line N of `mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`, read at `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml` (checked 2026-11-18).
  - "core Footprint:N" is the same file under `core.sc2mod`.
  - "liberty:N" is line N of the extracted `UnitData.xml` under `~/scratch/combat_wp3_data/liberty/`.
  - The later layers were checked: swarm, swarmmulti, void, voidmulti and libertymulti redefine none of the footprints used here. The one exception is void's `FootprintGeyserRoundedBuilt`, which changes only its resource ring rows, not its `Shape`. swarmmulti re-sets the Spine Crawler's `Footprint` to the same name.
- **[COMM]** Community or editor documentation.
- **[OWNER]** The owner's answer in Q0087.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Footprint:3–16, the `CFootprint` default | `Check` cells exclude `NoBuild`, `Creep` and `Masked`. `Place` cells apply `GroundNoSpawn` and `NoBuild`. **`Place` does not write pathing** [DATA] |
| S2 | SC2Mapster wiki, [Data/Footprints](https://sc2mapster.wiki.gg/wiki/Data/Footprints) (checked 2026-11-18) | "Placement Apply (Place) determines what placement flags are applied to cells". "Placement Check (Check) determines placement flag requirements". "Shape – Offsets: location of the points used to create the pathing mesh for the footprint". "Shape – Borders: used to determine the shape of the pathing layer … 2 = Building". "Footprints are stored as triangular geometries" [COMM] |
| S3 | Footprint:1923–1938 (`Footprint3x3Contour`), 1940–1953 (`2x2Contour`), 1955–1972 (`4x4Contour`), 1993–2018 (`5x5Contour`) | `Check` and `Place` are full squares of cells. `Shape` is an octagon with corners cut by 0.5 (3×3, 2×2), 1 (4×4) and 1.5 (5×5). Offsets are at :1936, :1951, :1970, :2016 [DATA] |
| S4 | Footprint:2189–2208 (`3x3CreepContour`), 2059–2076 (`2x2CreepContour`), 1226–1241 (`2x2IgnoreCreepContour`), 5352–5372 (`2x2ZergSpineCrawler`), 2238–2264 (`6x5DropOffCreepSourceContour`) | The Zerg contours equal the Protoss and Terran ones: 3×3 cut 0.5, 2×2 cut 0.5, 5×5 cut 1.5 on **5×5** cells [DATA] |
| S5 | Footprint:922–962 (`FootprintGeyserRoundedBuilt`) | Shape offsets ±1.5 and ±0.75 (:960): a 3×3 box cut by 0.75 [DATA] |
| S6 | Footprint:145–165 (`Footprint2x2Contour2`, Spore Crawler) | 2×2 `Place` and `Check` cells. Shape offsets ±0.5 and ±0.25 (:163): a 1×1 box cut by 0.25, centred in the 2×2 [DATA] |
| S7 | Footprint:1098–1110 (`Footprint1x1`) | One cell; shape ±0.5, uncut [DATA] |
| S8 | liberty UnitData `Footprint` and `PlacementFootprint` | Sensor Tower `Footprint1x1` (9454); Spire `2x2CreepContour` (9013); Greater Spire `2x2IgnoreCreepContour` (9084); Spore `2x2Contour2` (8751); Spine `2x2ZergSpineCrawler` (8617); Hatchery, Lair, Hive `6x5DropOffCreepSourceContour` (8210, 8292, 8372); Command Center family and Nexus `5x5Contour` (10379, 10525, 10663, 4186); Refinery, Assimilator, Extractor `GeyserRoundedBuilt` (5778, 4124, 8151); 3×3 and 2×2 contours per the radius plan's §2.3 [DATA] |
| S9 | Liquipedia, [Walling](https://liquipedia.net/starcraft2/Walling), section "Placement" (wikitext read 2026-11-18) | <ul><li>Buildings adjacent horizontally or vertically "completely block units".</li><li>Placed "diagonally so that only their corners meet, only small sized units can pass between them".</li><li>5×5 buildings: "a common border of length 1 results in a semi-wall, while corner-to-corner placement allows medium size units (e.g. Stalkers, Roaches, but not tanks) through".</li><li>Crawlers "allow small units … to pass … even when arranged next to each other".</li></ul> [COMM] |
| S10 | Q0087, answer | "Two structures placed diagonally corner-to-corner … have 45° chamfered footprint corners leaving a diagonal clearance of √2/2 ≈ 0.707 … blocks Zerglings (0.75 > 0.707) … workers (0.625 < 0.707)" pass [OWNER] |
| S11 | Footprint:1835–1860 (`5x5DropOff`), 128–144 (`3x3`), 1184–1197 (`2x2`), 2146–2165 (`3x3Creep`), 1212–1225 (`2x2Creep2`) | The `PlacementFootprint`s, used while placing. Their shapes are full squares for Terran and Protoss, and smaller for Zerg (±1.125 on 3×3) [DATA]. Their use is not documented (§10) |

### 2.2 The rules that follow

1. **Three layers, two roles.**
   - `Place` cells are what a placed structure takes from the placement grid: other buildings may not overlap them, and units do not spawn on them (S1).
   - `Shape` is what it cuts into the pathing mesh (S2). Units collide with that shape at their inner radius (contact plan S1, S2).
   - `Check` cells are what a placement tests. For every shipped analogue they equal the `Place` cells (S3 to S7). Their flag sets beyond buildability (`NearResources`, `DropOff`, `Creep`) are a separate mechanic (B-139, WP0).
2. **The contour is a centred box with 45° chamfers** for every shipped analogue (§2.3). Chamfer endpoints, for example 5×5 `(-1, -2.5)`, are measured from the unit's origin, which is the centre of its cells:
   - odd sizes: the centre of a cell (`Area="-1,-1,2,2"` for 3×3);
   - even sizes: a lattice point (`-2,-2,2,2` for 4×4).

   Our `top_left + (w, h)/2` is the same anchor.
3. **Walls (S9, S10) follow from the geometry** [INF, checked by hand]. Two chamfers of cuts `c₁` and `c₂` meeting at a shared Place corner leave a band of width `(c₁ + c₂)/√2`:
   - 2×2 or 3×3 pairs: 0.707. A disc passes iff `2t ≤ 0.707`, so a worker at 0.3125 passes and a Zergling at 0.375 does not (S10).
   - 5×5 with 3×3: 1.414. A Stalker passes and a tank does not (S9).
   - 5×5 sharing one cell of border with a 3×3: the nearest chamfer vertex is 0.707 from the other chamfer, so it is a semi-wall. Sharing two cells, the faces meet: tight (S9).
   - Face-adjacent buildings share a face: tight (S9).
4. **One S9 claim conflicts with the data and the data wins (Q0051).** "Spine Crawlers next to each other let small units pass" contradicts 5.0.14's `2x2ZergSpineCrawler` octagon (Footprint:5370), whose faces meet when the crawlers are adjacent. For Spores it holds: shapes 1 apart, so a Zealot passes. The Liquipedia passage likely predates a footprint change; recorded, not modelled.
5. **The engine radius `E = max(Radius, ⌈16·Shape.Radius⌉/16)`** (plan `2026-11-18-structure-radius` §2.2) covers every contour point by construction. With the true shapes no floor is needed for melee liveness (D-10).

### 2.3 Values (49 analogues)

`hw`, `hh` are the shape's half-extents and `c` its chamfer, in world units. "Place" is the footprint in cells.

| Group (analogues) | Place | `(hw, hh, c)` | Source | E (authored `radius`) |
|---|---|---|---|---|
| Command Center, Orbital, Planetary, Nexus, Hatchery, Lair, Hive (7) | 5×5 | (2.5, 2.5, 1.5) | 5x5Contour :2016; 6x5…Contour :2262 | 2.75 |
| Barracks, Engineering Bay, Bunker, Factory, Ghost Academy, Starport, Armory, Fusion Core, Gateway, Forge, Cybernetics Core, Twilight Council, Stargate, Robotics Facility, Templar Archives, Robotics Bay, Fleet Beacon (17) | 3×3 | (1.5, 1.5, 0.5) | 3x3Contour :1936 | 1.8125 |
| Spawning Pool, Evolution Chamber, Roach Warren, Baneling Nest, Hydralisk Den, Lurker Den, Infestation Pit, Nydus Network, Ultralisk Cavern (9) | 3×3 | (1.5, 1.5, 0.5) | 3x3CreepContour :2206 | 1.8125 |
| Refinery, Assimilator, Extractor (3) | 3×3 | (1.5, 1.5, 0.75) | GeyserRoundedBuilt :960 | 1.6875 |
| Supply Depot, Missile Turret, Tech Lab, Reactor, Pylon, Photon Cannon, Shield Battery, Dark Shrine, Spine Crawler (9) | 2×2 | (1, 1, 0.5) | 2x2Contour :1951; SpineCrawler :5370 | as today (1.25 Depot, 1.5 Shrine, 1.125 others) |
| **Spire, Greater Spire** (2) | **2×2** (was 3×3) | (1, 1, 0.5) | 2x2CreepContour :2074; IgnoreCreep :1239 | **1.125** (was 1.5, floor) |
| **Spore Crawler** (1) | 2×2 | **(0.5, 0.5, 0.25)** | 2x2Contour2 :163 | **0.875** (was 1.0, floor) |
| **Sensor Tower** (1) | **1×1** (was 2×2) | (0.5, 0.5, 0) | 1x1 :1106 | **0.75** (was 1.0, floor) |

The total is 49. Every vertex of every shape lies on the quarter-cell lattice once placed. The Spore and geyser shapes need quarter cells; the others need half cells.

---

## 3. Diagnosis

Today `PlacedStructure` (`sim_map/src/grid.rs`) is `top_left`, `width`, `height` and `prior_flags`. `place_structure` marks every cell `OCCUPIED_BY_STRUCTURE` and clears `BUILDABLE | GROUND_PATHABLE`. Every pathing consumer reads those cells:

| # | Consumer | File, symbol | Reads | After |
|---|---|---|---|---|
| M1 | Mesh constraints and labels | `navmesh/mod.rs` `classify_cell` (`structure` flag), `edge_kind` (Seam on every structure cell edge), `label_triangle` (centroid cell → `Structure(id)`), `collect_runs` | cells | Contour cut (D-3, D-4) |
| M2 | Mesh lattice | `navmesh/predicates.rs` `LatticePoint` (whole cells), `to_q`, `to_sim`; `segment.rs` `nearest_on_lattice_segment` (`<< 32`); `mod.rs` `unexplored_block` (bounding box in cells), `label_triangle` (`div_euclid(3)`) | whole cells | Quarter cells (D-3) |
| M3 | Contact early-out | `EdgeGrid::contact_free_cells` | cells | Unchanged (conservative: an occupied cell is never contact-free) |
| M4 | Clearance field (hashed) | `pathfinding.rs` `mark_structure_walls` → `mark_cell_wall` | cells | Conservative raster of the shape (D-6) |
| C1 | Centre rule | `sim_core/src/movement/flee.rs` `is_position_pathable` (an entered cell must not be occupied); read by `MotionPolicy::permits_segment`, `validate_origin` | cells | Shape (D-8) |
| C2 | Unit placement and egress | `movement/geometry.rs` `ground_footprint_connected` (open square envelope of half-side `t` against occupied cells); read by `validate_placement_terrain` (Blink, unload, Abduct) and `production/exits.rs` `egress_connected` | cells | Terrain unchanged; structures by the disc against the shape (D-8) |
| C3 | Contact, routes, steering probes, nearest reachable point | `contact.rs` → `Pathfinder::wall_contact`; `search.rs` | mesh | No code change; follows M1 |
| P1 | Construction sites, build and harvest reach, approach rings, exits ring, scenario rings, knowledge mask, `known_blocked_cells` | `production/construction.rs` `site_of`, `stands_on`; `economy` `within_reach`, `approach_point`; `exits.rs`; `scenario.rs`; `world/vision.rs` | Place cells | Unchanged (D-1) |
| P2 | Drawn dome, muzzle, projectile visual ends, shatter | `UnitArchetype::drawn_radius` | footprint ÷ 2 | Shape (D-11) |

**Hazard found (H-1).** `economy::approach_point` returns a 1×1 site's own centre when the cell is walkable. Construction uses it both to approach a site (`approach_footprint`) and to leave it (`leave_footprint`). Once the Sensor analogue is 1×1, the builder walks onto its site, `stands_on` refuses the construction, and leaving sends it back to the same centre. It never builds (D-9).

**Hazard found (H-2).** A mesh cut by chamfers has diagonal constraints. The CDT (`cdt.rs` `insert_constraint`, Sloan) accepts any segment direction, but it requires that "no vertex lies in the open segment and it crosses no constraint". Chamfer endpoints lie inside face runs of neighbouring structures and of terrain, so constraints must be split at every such T-junction (D-4).

**Hazard found (H-3).** `can_place_building` admits cells that are `BUILDABLE` but not `GROUND_PATHABLE` (map validation does not forbid them; `validation.rs` check 1 covers only Abyss). Under such a cell the cut corner would be labelled open ground that is not walkable (D-5).

---

## 4. Design decisions

### D-1. Two layers: `Place` cells and `Shape`

- **`footprint: (w, h)` stays the `Place` (and `Check`) cells.** Everything in P1 keeps reading it.
  - This is the game's split (S1, S2).
  - SC2's placement grid shows cells, not contours.
  - `PlacementFootprint` shapes (S11) are full squares for Terran and Protoss, so the construction body check (`stands_on`, at `t` from the cell box) matches them. The Zerg difference is unsourced in use (§10).
- **`footprint_shape` is the new pathing shape.** Everything in M1, M4, C1, C2 and C3 reads it.
- `Check` cells equal `Place` for every shipped analogue. Their flag sets are registered as B-139 (WP0), not modelled.

### D-2. Data model: a chamfered, centred box in quarter cells

- **Authored** (`StructureArchetypeDef`, `crates/sim_core/src/catalog.rs`): `pub footprint_shape: FootprintShapeDef { half_width: f64, half_height: f64, chamfer: f64 }`.
  - The field is **required**, like `radius` (structure-radius D-1). 46 of 49 rows differ from the full square, so a default would be silently wrong.
  - The name is the game's word (`Shape`).
  - The RON reads as `footprint_shape: (half_width: 1.5, half_height: 1.5, chamfer: 0.5)`, with a comment citing the Footprint line of its `Offsets`.
- **Resolved type** `sim_types::FootprintShape { half_width_q: u16, half_height_q: u16, chamfer_q: u16 }`, in quarter cells. It is `Copy, Eq, Hash, Serialize`.
  - It lives in `sim_types` because both `ShapeKind` (`sim_types`) and `PlacedStructure` (`sim_map`) carry it.
  - It has `FootprintShape::full(w, h)` (the full square of the cells, chamfer 0) and `inscribed_half_extent()`.
  - It has no geometry methods; those are in `sim_map` (D-7).
- **Carried** in `ShapeKind::BuildingDome { footprint_w, footprint_h, shape: FootprintShape }`. A sum type keeps "a dome has a shape, a unit none" in the type system.
  - 41 sites construct or destructure the variant with named fields. Patterns that do not read the shape gain `..`; constructors use `FootprintShape::full`.
  - Rejected: an `Option<FootprintShape>` on `UnitArchetype`, which would need a loader invariant the type could state.
- **Loader validation** (`catalog/validation.rs`, new `validate_footprint_shape`). Each failure is a typed `invalid(slug, "footprint_shape", reason)`:
  1. finite, and each value a multiple of 1/4 (exact in quarter units and on the mesh lattice);
  2. `0 < half_width ≤ w/2` and `0 < half_height ≤ h/2`: the shape lies in its cells;
  3. `0 ≤ chamfer < min(half_width, half_height)`: every face keeps a positive length.
- **Why a chamfered box and not a vertex list:** it represents all 49 analogues exactly (§2.3), and every rule below is a closed form of it (vertices, inscribed extent, bounding radius). A general convex polygon would need convexity, lattice and containment validation for no shipped need. The extension path is recorded (§10).
- **Placed** (`sim_map/src/grid.rs`): `PlacedStructure` gains `pub shape: FootprintShape`.
  - `place_structure(id, cell, footprint: StructureFootprint)`, where `StructureFootprint { width: u8, height: u8, shape: FootprintShape }` and `StructureFootprint::square(w, h)`, replaces `place_structure(id, x, y, w, h)`.
  - All 51 call sites move: the two production sites (`world.rs` `spawn_structure`, `construction.rs` `place_foundation`) pass the archetype's shape; tests and `examples/navmesh_measure.rs` pass `square`.
  - A shape that is outside its cells or off the lattice is a typed `MapError::InvalidFootprintShape`.

### D-3. The mesh lattice becomes quarter cells

- `LatticePoint` coordinates are quarter cells: `LATTICE_BITS = 2`, so `to_q` shifts by `Q_BITS − 2` and `to_sim` by `32 − 2`.
  - `LatticePoint::cell_corner(x, y)` replaces whole-cell construction.
  - `SquaredWidth` keeps the meaning "cells squared": widths built from lattice units carry `den × 16`.
- **Why quarter and not half:** the Spore (0.25) and geyser (0.75) shapes need it (§2.3). Every placed vertex is then `4·top_left + 2·(w, h) ± (half ± chamfer)·4`, an integer.
- **Margins.** WP1 re-derives every margin in `predicates.rs`'s module doc for coordinates up to 2^14:
  - `orient2d` differences are below 2^15 and products below 2^30;
  - `incircle` terms are below 2^62 in `i128`;
  - in `nearest_on_lattice_segment`, `along` is below 2^59 and `cross²` below 2^118;
  - for `admits_diameter`, the radius cap drops to 2^12 world units (the map's own limit), so `4·16·r²·den < 2^125`.

  `extremes_do_not_overflow` is extended to 2^14.
- **Purity.** Every predicate is scale-invariant in sign, and vertices and constraints are inserted in the same relative order (D-4). So the mesh of any map is the same triangulation with its coordinates scaled by 4.
- `NavMesh::digest` is first made unit-independent (WP1a): it eats vertex positions as `to_sim()` raw world values, and widths as fractions in cells² reduced by their gcd. It is re-pinned once on the old lattice; WP1b must leave it unchanged. That is the purity proof.

### D-4. The constraint graph with contours

- **Model.** Unit edges on the quarter lattice in eight directions: east, north and the two diagonals. Each edge has one kind, with precedence `Wall > Seam > none`.
  - **Terrain** contributes its whole-cell edges, as today's `edge_kind` computes them, but with `structure: false` for every cell (terrain only). An occupied cell is "usable" as today, through `OCCUPIED_BY_STRUCTURE` in `cardinal_edge_is_connected`.
  - **Each structure** contributes, as `Seam`:
    - the boundary of its shape;
    - every interior cell-grid line of its Place rectangle, clipped to the shape's open interior.

    The clipped lines keep the current invariant "a structure triangle lies in one cell". The knowledge rules (`Access::structure_known(label.cell)`, per-cell vision) therefore stay per cell, as today.
  - **Storage.** Terrain stays in the whole-cell `EdgeGrid`. Structure geometry, and terrain edges on or inside a Place rectangle, go into a sparse sorted map of quarter edges (a `Vec` sorted by `(y, x, direction)`; no hashed collection). Every structure edge lies in its closed Place rectangle.
- **Vertices and runs.**
  - A quarter-lattice point is a vertex iff its incident unit edges are not exactly one straight pair of one kind. That is today's `is_vertex`, generalised to eight directions.
  - Runs are maximal straight chains between vertices.
  - Vertices are inserted in row-major `(y, x)` order. Runs are inserted in order of `(from.y, from.x, direction)`, with directions ordered east, north, north-east, north-west. For whole-cell geometry this is today's order.
- **Lemma (no proper crossings, H-2).**
  - A diagonal unit quarter edge crosses no lattice line in its interior.
  - Axis-aligned unit edges meet only at lattice points.
  - So in the unit-edge model, two constraints share at most lattice points, and every such point is a vertex by the branch rule.
  - This holds for touching and adjacent structures, structures beside cliffs, and structures over terrain seams.
  - `NavMeshError::ConstraintRecovery` stays as the guard.
- **Labels.** A triangle's centroid cell is `⌊(a + b + c)/12⌋` per axis. The triangle is `Structure(id)` iff that cell's occupant's shape contains the centroid. The test runs on `3·centroid` against the shape scaled by 3: exact integers, open interior. A centroid is never on a constraint. Otherwise the label is the terrain class of the cell.
- **Equivalence for square shapes.** For `FootprintShape::full`, the boundary plus clipped lines are exactly every edge of every structure cell, all `Seam` unless terrain made one a `Wall`. That is today's set. So vertices, runs, order, labels, widths and the clearance (D-6) are bit-identical (WP3's stop rule 2).
- **Contact and routes need no change.** `segment.rs` and `funnel.rs` work on any segment direction. The inward normal of a diagonal edge is `SimVec2::from_i32(±1, ±1).normalize_or_zero()`, which is deterministic.

### D-5. The ground under a structure is walkable

- `can_place_building_where` refuses a cell that lacks `GROUND_PATHABLE`, with a new `PlacementFailureReason::UnpathableTerrain`, before the buildable check.
- So every Place cell is open ground of one level, and a cut corner is open ground [INF: in SC2 unpathable ground is never a valid building site; S1's `Check` excludes `NoBuild`].
- **Consequence for maps.** Validation check 13 (`StartFootprintBlocked`) calls `can_place_building`, so a start location on buildable but unwalkable cells becomes invalid. WP2 confirms that both shipped maps still validate (**verify**).
- **Rejected:** labelling the cut corner from `prior_flags`. That keeps placement unchanged, but it adds interior terrain walls under structures, breaks the square-shape equivalence on such maps, and leaves a never-intended case alive.

### D-6. The clearance field rasterises the shape conservatively

- `mark_structure_walls` marks the four corners of every half-cell whose open interior meets the shape's open interior.
  - For full squares that is exactly today's set of points (all half-lattice points in the closed cells).
  - For contours the field can only underestimate.
- The invariant in `pathfinding.rs` is reworded: "`clearance_at` is zero on every cell no unit may enter; a cell cut by a structure's contour may report zero although a disc reaches into it". Rationale: the field is hashed and diagnostic; routes do not read it. A sampled field that overestimated beside a 45° chamfer would claim discs that do not fit.
- Rejected: a quarter-cell raster. It would change every map's clearance, structure-free maps included (hashed), for diagnostics only.

### D-7. Exact shape predicates in `sim_map` (new `src/footprint.rs`)

Methods on `PlacedStructure` (via `FootprintShape` and its cells):

| Function | Use | Arithmetic |
|---|---|---|
| `contour(&self) -> ([LatticePoint; 8], usize)` | Mesh, tests | Counter-clockwise vertices; 4 when the chamfer is 0 |
| `contains_open(p: QPoint3)` | Labels, raster | Exact on `3·` coordinates |
| `segment_meets(a, b) -> bool` | Centre rule (C1) | Closed shape; Q16 `orient_q` and `segments_touch` |
| `capsule_meets(a, b, radius) -> bool` | Unit placement and egress (C2) | Open shape; Q16 `segments_within` with the radius rounded up (`radius_q`), the mesh segment predicate's arithmetic, so placement, routes and contact agree on the shape to the Q16 grid. Points on the Q16 grid and radii in 1/16 are exact |

`sim_map::GameMap::structure_occupants_touched(start, end, radius, visit)` iterates the distinct occupants of the cells a capsule's bounding box touches, in cell order.

### D-8. `sim_core` consumers (C1, C2)

- **Centre rule.** In `is_position_pathable`, a cell that holds a structure refuses the segment only if `placed.segment_meets(start, end)`. The shape is closed, which refuses more: a centre exactly on the contour is refused, but the contact rule keeps every `t > 0` disc off it anyway.
- **`ground_footprint_connected`.** Terrain is unchanged: the open square envelope of half-side `t` against unwalkable cells. For each distinct occupant except `exempt`, the segment is refused if `capsule_meets(start, end, t)`.
  - Rationale: the disc is the body model of contact (B-113) and routes. A point that placement admits is one the contact rule holds. The game's semi-walls (S9, S10) are geometry of discs.
  - At faces, square and disc agree. They differ only within `t(√2 − 1)` of a convex corner.
  - Structure-contact S-9, which pinned the square at structures, is rewritten (WP5).
- **Through the mesh with no code change:** contact (`settle_on_terrain`), routes, steering probes and slides, `nearest_reachable_point`. A cell centre in a 5×5's cut corner may now be a move goal: it is 0.354 from the chamfer, so a worker fits.

### D-9. Construction approaches the edge of a site (H-1)

`approach_point` is split:
- `approach_cell` keeps the one-cell shortcut, for resource-node and unload cells (`harvest.rs`, `transport/orders.rs`);
- `approach_edge` always uses the ring, for `approach_footprint` on construction sites and for `leave_footprint`.

This matches the game, where a worker builds from beside its site.

### D-10. B-138 and the radius rule

- Data (WP6):
  - Sensor analogue: `footprint (1, 1)`, `radius 0.75`;
  - Spire and Greater Spire analogues: `(2, 2)`, `1.125`;
  - Spore analogue: shape `(0.5, 0.5, 0.25)`, `0.875`.

  Each comment cites its UnitData and Footprint lines.
- **Loader rule D-2.3 of the radius plan is replaced** (`validate_radius`): "radius must cover the footprint shape", that is `radius² ≥ max(hw² + (hh − c)², (hw − c)² + hh²)`, exactly in 1/16 and 1/4 units.
  - This is the game's guarantee (§2.2.5). A unit touching any contour point is within `radius + r` of the centre, so a melee weapon connects.
  - It is stronger than the old floor, which left square corners out of reach (radius plan D-4).
  - The floor rule and the four floored values go.
  - Ordering: the rule is introduced together with the true shapes (WP6). With WP4's full-square shapes it would refuse the 5×5's 2.75 (corner 3.54).
- The radius plan's D-5 is unchanged: a structure's `terrain_radius` is half the Place's smaller side (Sensor 0.5 ≤ 0.75).

### D-11. Presentation

- `UnitArchetype::drawn_radius()` for a dome returns `shape.inscribed_half_extent()`. This is the radius plan's D-6 reason ("the dome covers what units meet") applied to the shape.
  - Unchanged for 45 analogues.
  - Spore 1.0 → 0.5: units now walk on its outer cells.
  - Sensor 1.0 → 0.5, Spires 1.5 → 1.0.
- The **client** draws:
  - the dome from the snapshot radius (no code change);
  - the placement ghost on the Place cells (`battle_input/command_mode.rs` `footprint`; no code change);
  - no contour overlay, as the game draws none.
- Media framing is per render shape (`asset_baker/src/framing.rs`), so no re-bake is needed. The muzzle (radius plan P3) follows `drawn_radius`.
- WP7's picture check records how a unit at a cut corner and the smaller domes read.

### D-12. Non-square footprints and anchors

- The Place rectangle is anchored by its minimum cell. The shape is centred on its centre `(4·left + 2w, 4·bottom + 2h)` in quarter units, always a lattice point:
  - odd sizes centre on a cell centre and even sizes on a lattice point, as SC2's `Area`;
  - the client's `placement_cell` already centres odd footprints on the clicked cell.
- `half_width ≠ half_height` is supported and tested with a scratch 3×2.
- Footprints are never rotated, as in the game.
- Shapes off-centre in their cells (add-on footprints such as `Footprint3x3AddOn2x2`) are out of scope; no shipped analogue has one.

### D-13. Hashed state

- `PlacedStructure.shape` is authoritative map state, so it is hashed (`fields!(PlacedStructure { …, shape })`, three `u16` values). `CHECKSUM_SCHEMA_VERSION` goes 29 → 30, documented as "Schema 30 adds each placed structure's footprint shape: half-width, half-height and chamfer in quarter cells (plan `2026-11-18-contour-footprints`)".
- The bump lands in WP3 with square shapes, so it changes checksums and not behaviour (D-15, rule 2).
- The clearance field is already hashed; its changes come only with true shapes (WP6).

### D-14. Measurement

`<base>` is the tree before a package and `<cand>` its tree.
- Release tools are built per tree into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` and copied aside (charter §7).
- Output goes to `~/scratch/contour-footprints/`.
- `docs/evidence/2026-11-18-contour-footprints.md` holds "How to reproduce".

| What | Command |
|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv` |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33`; `diff` of `seeds.csv` |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route` to the match's end, and seeds 1 to 8 with `--seed`; `units.csv`, deeper unit-ticks, reversals, give-ups |
| Goldens | `golden_replay_tests` native debug and release; `--target wasm32-wasip1 --release` under wasmtime; wasm debug once in WP3 and WP6 |
| `sim_map` goldens | `determinism_tests` (`EXPECTED_CHECKSUM`), `validation_golden_tests` (`VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `PATH_GOLDEN`, new `CONTOUR_PATH_GOLDEN`), `navmesh_tests` (`SHOWCASE_MESH_DIGEST`), each native and wasm |
| Entity-only replay digest | A scratch harness, not committed: replays a golden script and hashes per tick only the checksum's `entities`, `projectiles`, `players` and `effects` sections; reports the first tick whose entity state differs |
| Mesh build | `cargo run --release -p sim_map --example navmesh_measure`, extended in WP3 with a "showcase map plus 24 seeded structures" row, square vs contour |
| Throughput | `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate alternated, three rounds |

### D-15. Stop rules and acceptance

As in the contact, inner-radius, structure-contact and structure-radius plans: **a winner flip anywhere stops the package.** The agent commits nothing beyond it and reports the trace: the first divergent tick, the entity, the structure, and which consumer (M1 to M4, C1 to C3) caused it. A movement that is not a flip is explained before the package proceeds.

1. **WP1 (lattice) is bit-identical** everywhere:
   - every `sim_map` golden;
   - `SHOWCASE_MESH_DIGEST` after its WP1a re-pin;
   - the goldens, matrix, battle 33 and showcase `units.csv`.

   Any difference is a defect: stop, find it, never re-record.
2. **WP2 and WP3 change behaviour nowhere; WP3 changes checksums.**
   - `PATH_GOLDEN` and `VALIDATION_GOLDEN` may change in WP2 only through D-5. The agent counts the corpus placements and start footprints newly refused as `UnpathableTerrain`, re-records with that count in the constant's comment, and shows that a scratch build without the rule reproduces the old value.
   - WP3 keeps every `sim_map` golden unchanged (square shapes, D-4 equivalence).
   - WP3's schema 30 changes every checkpoint of both goldens. They are re-recorded once, with a header naming the schema bump and the proof: a scratch build that does not encode `shape` and keeps schema 29 reproduces every recorded checksum of both fixtures. The entity-only digest is unchanged on every tick.
   - Matrix and battle 33 are bit-identical; the showcase `units.csv` is byte-identical.
3. **WP4 (catalog wiring, square shapes) is bit-identical** after WP3, the goldens included.
4. **WP5 (consumer rules, square shapes) is expected bit-identical.** The disc rule departs from the square envelope only within `t(√2 − 1)` of a Place corner, and only for Blink, unload, Abduct and egress.
   - A difference is traced to such a placement or exit. It is accepted only if the trace shows one; otherwise it is a defect.
   - `golden_replay` must not move. If it does, it is re-recorded only with that trace.
5. **WP6 (data) moves structure worlds.**
   - **Structure-free worlds stay bit-identical:** matrix, battle 33, `golden_replay_orders`. A difference is a defect.
   - **`golden_replay`** (halls at cells (6, 6) and (52, 52)) changes from checkpoint 1, because the halls' clearance is hashed. The entity-only digest names the first tick at which entity state differs, if any. The header records both: "Re-recorded for plan 2026-11-18-contour-footprints WP6 (B-134): the halls' 5.0.14 contour (corners cut by 1.5) changes their hashed clearance from checkpoint 1; entity state first differs at tick T (entity E rounds the hall at cells (…) inside its old square corner) | entity state is unchanged through tick 1000". It must pass native debug and release, `wasm32-wasip1` release, and wasm debug once.
   - **Control runs** (scratch builds):
     - (a) the CC-analogue shape reverted to `(2.5, 2.5, 0)`: the showcase is byte-identical to WP5;
     - (b) the B-138 rows alone reverted: the showcase is byte-identical to WP6 (it holds none of them).

     These attribute the movement to the 5×5 contour.
   - **Showcase.** The base is WP5's own run; the structure-radius merge records player 1 winning at tick 2,191 (**verify**; if the base differs, its own run is the reference).
     - **Expected direction:** units leaving the spawn rings round the halls' cut corners, so the first divergence is within about 30 ticks, beside a hall corner. Lanes shorten by up to about 0.6 per corner rounded.
     - **Acceptance:** the same winner on the default seed; 0 deeper unit-ticks; stall give-ups ≤ base + 1; reversals ≤ base + 10 per owner; the match ends. Seeds 1 to 8 are reported with winners and end ticks.
     - **If the default-seed winner flips:** stop and trace from the spawn. Classify the cause as **rule** (a unit now passes inside the old square corner, S3) or **defect** (a stall or oscillation at a chamfer, a body wedged in a chamfer notch, a refused slide on a diagonal edge, or a funnel fallback in `mesh_funnel_fallbacks`). The orchestrator decides by amendment. The showcase has flipped three times before for rule-caused reasons. No constant is tuned.
   - **If the default-seed timeline moves at all,** WP7 re-checks the storyboard.
6. **Test edits.** A changed test is re-derived from a trace and stated in the commit; no bound is loosened. A change §3 does not explain is a finding and stops the package.
7. **No tuning constant** is invented. In particular `CORNER_SAFETY_MARGIN` stays 0.05 (R-2).

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `docs/plans/README.md` (row), `DESIGN.md` §4 (B-134, B-138 restated; B-139 new) | WP0 |
| Lattice | `crates/sim_map/src/navmesh/{predicates,mod,segment,cdt,search,funnel,lanes}.rs` (units, margins, `unexplored_block`, labels), `crates/sim_map/tests/navmesh_tests.rs`, `funnel_tests.rs`, `wall_contact_tests.rs` (lattice literals) | WP1 |
| Shape type, predicates, placement rule | `crates/sim_types/src/enums.rs` (or new `footprint.rs`: `FootprintShape`), `crates/sim_map/src/footprint.rs` (new), `grid.rs` (`PlacedStructure.shape`, `StructureFootprint`, `place_structure`, `UnpathableTerrain`), `error.rs`, `lib.rs`; every `place_structure` call site (51, listed by `grep -rn "place_structure(" crates`) | WP2 |
| Mesh cut, clearance, schema | `crates/sim_map/src/navmesh/mod.rs` (constraint graph, labels), `pathfinding.rs` (raster), `crates/sim_map/examples/navmesh_measure.rs`; `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs`; `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron` | WP3 |
| Catalog | `crates/sim_core/src/catalog.rs` (`footprint_shape`, `into_archetype`, `drawn_radius`), `catalog/validation.rs`, `assets/catalog/structures.ron` (49 rows, full squares), the 41 `BuildingDome {` sites across crates, `crates/sim_core/src/world.rs` and `production/construction.rs` (pass the shape) | WP4 |
| Consumers | `crates/sim_core/src/movement/flee.rs`, `movement/geometry.rs`, `economy/mod.rs` (`approach_cell`, `approach_edge`), `production/construction.rs`, `crates/sim_core/tests/structure_contact_tests.rs` (S-9), `terrain_radius_tests.rs` (if traced) | WP5 |
| Data and reference | `assets/catalog/structures.ron` (49 shapes; 4 B-138 rows), `catalog/validation.rs` (radius rule), `crates/sim_core/tests/catalog_reference_values_tests.rs`, `structure_radius_tests.rs` (R-5), new `crates/sim_core/tests/contour_footprint_tests.rs` | WP6 |
| Client and evidence | `crates/client/src/effects/catalog/units.rs` (muzzle doc), `assets/captures/showcase.ron` (if moved), `docs/evidence/2026-11-18-contour-footprints.md` | WP7 |
| Bookkeeping | `DESIGN.md` (Movement and Combat rows, register, §5 if figures move), this plan, pointer amendments in `2026-11-18-structure-contact.md` and `2026-11-18-structure-radius.md` | WP8 |

---

## 6. Work packages (commit order)

- **Branch:** `contour-footprints`.
- **Tests first:** each package writes its tests first and shows them red on its base. WP0 and tests marked *guard* are the exceptions.
- **Gates:** implementation agents hand back after `cargo gate quick` and the tests their change needs. WP1 to WP3 form the `sim_map` package and WP4 to WP6 the `sim_core` package; a verifier runs `cargo gate package` on each (Q0061).
- New test files are `fast` unless measured `slow`, in which case `crates/gate/suites.ron` gains a rule (C22).
- **Sizes:** S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md`, status `Proposed`.
- **B-134 restated:** "…fixed by authoring each structure's 5.0.14 `Shape` as `footprint_shape` and cutting it into the navigation mesh (plan `2026-11-18-contour-footprints`)". Open until WP8.
- **B-138 restated:** the Hatchery item is withdrawn, citing Footprint:2210–2264 (5×5 cells; "6x5" is only the name). The other four items stay, pointing here.
- **New B-139** (open, out of scope): "The flag sets of footprint layers are not modelled. A town hall analogue may be placed within reach of resources (`Footprint5x5Contour` `Check` excludes `NearResources`, Footprint:1997–2000; `FootprintGeyserRoundedBuilt` `Place` applies it on a ring, Footprint:937–957), and creep requirements (B-63) are not checked."
- **No question file:** Q0051 decides.
- **Gate:** docs-only `package` (fmt, `workspace_checks`, C17, C18).

### WP1: quarter-cell lattice (M, `sim_map`, pure)

- **WP1a, commit 1:** `NavMesh::digest` becomes unit-independent (D-3). `SHOWCASE_MESH_DIGEST` is re-pinned, with a comment: "re-pinned for plan 2026-11-18-contour-footprints WP1a: the digest now eats world coordinates and reduced widths; the mesh is unchanged". No other change.
- **Tests first (WP1b):**
  - unit (`predicates.rs`) `lattice_points_are_quarter_cells`: `cell_corner(3, 5).to_sim()` is (3, 5), and `LatticePoint::new(13, 7).to_sim()` is (3.25, 1.75);
  - unit `squared_widths_are_in_cells_squared`: two lattice points 4 apart admit radius 0.5 and refuse 0.5 + 2⁻³²;
  - `extremes_do_not_overflow` at 2^14.
- **Change:** D-3, plus every site that does arithmetic on a `LatticePoint`'s coordinates. The commit message carries the audit: the output of `grep -n "\.x\b\|\.y\b\|<< 32\|div_euclid(3)" crates/sim_map/src/navmesh/*.rs`, each hit classified as converted or unit-free.
- **Exit:** D-15, rule 1. `SHOWCASE_MESH_DIGEST`, `EXPECTED_CHECKSUM`, `PATH_GOLDEN` and every `sim_core` measurement are unchanged.

### WP2: shape type, predicates, walkable placement (S, `sim_map`)

- **Tests first** (unit tests in `footprint.rs` unless noted):
  - `a_full_shape_is_its_cells`;
  - `contour_vertices_are_on_the_quarter_lattice_for_odd_and_even_sizes`: 1×1, 2×2, 3×3, 5×5, and a scratch 3×2 with `(1.5, 1, 0.5)`;
  - `contains_open_is_exact_on_a_chamfer`;
  - `segment_meets_the_closed_shape`;
  - `capsule_meets_is_exact_at_a_chamfer`: for a point on the Q16 grid exactly `t` from a 5×5 chamfer, the capsule does not meet; at `t − 2⁻¹⁶` it does;
  - `a_shape_outside_its_cells_is_refused` (`MapError::InvalidFootprintShape`);
  - `wall_off_tests::placement_refuses_unwalkable_ground` (`UnpathableTerrain`).
- **Change:**
  - `FootprintShape` and `StructureFootprint`;
  - `PlacedStructure.shape`;
  - the new `place_structure` signature, with all 51 call sites moved to `StructureFootprint::square`;
  - D-5;
  - D-7's predicates.

  The mesh still cuts cells, which is correct while every shape is square.
- **`sim_core` encoding is not yet changed.** The field is added to the encoding in WP3, together with the mesh. **Verify** that `fields!` does not require completeness; if it does, the encoding moves here and WP3's schema proof applies here.
- **Exit:** D-15, rule 2 (the D-5 clause). Both shipped maps validate.

### WP3: the contour cut, the clearance raster, schema 30 (M, `sim_map` and checksum)

- **Tests first:**
  - **`wall_off_tests::a_diagonal_contour_wall_passes_a_worker_and_stops_a_zergling` (the owner's case, S10).**
    - Setup: the staircase map of `clearance_field_tests::f7_no_tier_passes_between_structures_touching_at_a_corner`, with two 2×2 structures at cells (10, 10) and (12, 12) placed with shape `(1, 1, 0.5)`.
    - Assertions:
      - radius 0.3125: `find_path` from centre (3, 18) to (18, 3) succeeds, and every leg passes `segment_is_passable` at 0.3125;
      - radius 0.375: `NoPathFound`;
      - the same holds for the layout mirrored across the anti-diagonal.
    - Red on WP2: both radii are refused, because the mesh cuts cells.
  - `wall_off_tests::a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` (property): for `t = k/256`, `k ∈ 64..=115`, on the same map, a route exists iff `8t² ≤ 1` (that is, `2t ≤ √2/2`).
  - `wall_off_tests::a_5x5_and_a_3x3_corner_to_corner_pass_medium_units` (S9): gap √2; `t = 0.6875` passes, `t = 0.75` is refused.
  - `wall_off_tests::a_5x5_sharing_one_cell_of_border_is_a_semi_wall_and_two_cells_is_tight` (S9): 0.3125 passes and 0.375 is refused for one cell; no route at 0.0625 for two cells.
  - `wall_off_tests::face_adjacent_contours_are_tight` (S9): two `(1.5, 1.5, 0.5)` 3×3s side by side and a staircase; no route at 0.0625.
  - Guards: `clearance_field_tests::f7_no_tier_passes_between_structures_touching_at_a_corner` (square shapes, unchanged) and `test_ramp_wall_off_geometry`.
  - `navmesh_tests::a_square_shape_cuts_the_mesh_as_cells_did`: seeded square structures on the showcase map; the digest equals the value the same test computes on WP2's tree, which is recorded in the test as a constant with that provenance.
  - `navmesh_tests::contour_structure_triangles_lie_in_one_cell`.
  - `navmesh_tests::a_cut_corner_is_open_ground`: the centre of a 5×5's corner cell locates in an `Open` triangle; the centre of the 3×3's corner cell locates in `Structure`.
  - `navmesh_tests::removing_a_contour_structure_restores_the_terrain_mesh` (digest equality).
  - `navmesh_tests::contour_meshes_build_for_seeded_layouts` (proptest, 64 cases): up to 12 seeded shaped structures drawn from §2.3's shapes, including touching, face-adjacent, diagonal and beside-cliff placements.
    - The build never errors.
    - Every `Structure(id)` triangle's centroid is inside `id`'s shape, and every `Open` triangle's centroid is outside every shape.
    - Every constraint edge lies on a shape boundary, a clipped cell line or a terrain edge.
  - `wall_contact_tests::contact_against_a_chamfer_is_exact`: the nearest point lies on the chamfer; the squared distance is exact; the normal is `(−1, −1)/√2` on the open side; the flood equals `wall_contact_exhaustive`.
  - `clearance_field_tests::a_contours_clearance_never_exceeds_the_true_clearance` and `square_shapes_keep_todays_clearance` (against a test-side cell-wall reference on seeded layouts).
  - `validation_golden_tests::find_path_with_vision_on_contours_matches_the_recorded_corpus_digest`: a new `CONTOUR_PATH_GOLDEN` with seeded shaped structures, pinned in this package.
  - `determinism_tests::contour_route_checksum`: 1,000 seeded routes on the default arena with eight contour structures, pinned.
  - `sim_core` `checksum_state_tests::a_placed_structures_shape_is_hashed`: two worlds that differ only in a structure's shape have different checksums.
- **Change:** D-4, D-6, D-13 (encoding and schema doc), and the measurement row in `navmesh_measure`.
- **Exit:** D-15, rule 2. Goldens are re-recorded with the scratch proof. Every pre-existing `sim_map` golden is unchanged. The new goldens pass on wasm.

### WP4: the catalog field and wiring (S, `sim_core`, pure)

- **Tests first** (`catalog/validation.rs` units over a minimal RON built from the shipped file, as for `radius`):
  - `a_footprint_shape_is_required` (a parse error naming `footprint_shape`);
  - `a_footprint_shape_must_be_on_the_quarter_lattice` (0.3 refused);
  - `a_footprint_shape_must_lie_in_its_cells` (half-width 1.25 on 2×2 refused);
  - `a_chamfer_must_leave_every_face` (chamfer 1 on `(1, 1)` refused);
  - `a_footprint_shape_is_read_into_the_dome` (quarter integers);
  - `catalog.rs` R-16 revised: `drawn_radius_is_the_shapes_smaller_half_extent`;
  - integration (`structure_contact_tests.rs`) `a_spawned_structure_places_its_catalog_shape`.
- **Change:**
  - D-2's catalog half;
  - `structures.ron` gets `footprint_shape: (half_width: w/2, half_height: h/2, chamfer: 0.0)` on all 49 rows, with the comment `// Replaced by the 5.0.14 Shape in plan 2026-11-18-contour-footprints WP6.`;
  - the 41 `BuildingDome` sites;
  - `spawn_structure` and `place_foundation` pass the shape;
  - D-11's `drawn_radius`.
- **Exit:** D-15, rule 3.

### WP5: consumer rules (S, `sim_core`)

- **Tests first** (`contour_footprint_tests.rs`, new). They use synthetic flat maps and a scratch catalog that edits one shipped structure's `footprint_shape` in its RON. Structures are chosen by `ShapeKind::BuildingDome` and size, and bodies are scratch-registered as in `terrain_radius_tests::register`, never by slug (C5). Every tick asserts no diagnostic.
  - C-1 `a_centre_may_cross_a_cut_corner_cell`: `is_position_pathable` admits a segment through a 5×5 contour's occupied corner cell outside the shape, and refuses one through the shape. Red on WP4.
  - C-2 `a_worker_builds_a_one_cell_structure` (H-1, D-9): a scratch 1×1 row and a harvester-capable builder; construction starts within 200 ticks. Red on WP4: `Unreachable`, or the builder never leaves the site.
  - S-9 rewritten (`structure_contact_tests.rs`) as `no_placement_is_refused_except_by_the_disc_against_the_shape` (property, 64 cases):
    - seeded full or contour shapes;
    - points on the Q16 grid;
    - a point is refused `Terrain` exactly when an exact test-side oracle (i128 squared distance from the point to the closed shape) is below `t²`, or the point lies inside the shape;
    - it is never refused `Occupied(structure)`.

    Red on WP4 at contour corners.
  - Guard: S-2 (face middle), unchanged.
- **Change:** D-8 and D-9, with docs (C4).
- **Exit:** D-15, rule 4.

### WP6: the 5.0.14 shapes and footprints (M, data)

- **Tests first:**
  - `catalog_reference_values_tests::b134_footprint_shapes_follow_starcraft`: `SC2_FOOTPRINT_SHAPE: [(&str, [f32; 5], &str); 49]`, holding `[w, h, hw, hh, c]` by analogue, with sources such as `"liberty UnitData:10379 Footprint5x5Contour; Footprint:2016 Offsets"`.
  - `b135_structure_radii_follow_starcraft`: the four floored rows become E.
  - `structure_radii_follow_the_engine_rule`: authored equals E for all 49, and E covers the authored shape.
  - Loader `a_structure_radius_must_cover_its_shape`, replacing the floor test:
    - 0.75 on a 1×1 full shape is admitted (bound √0.5) and 0.6875 refused;
    - 1.8125 on a 3×3 contour is admitted (bound 1.803) and 1.75 refused.
  - `structure_radius_tests` R-5 revised: `every_shipped_structure_contour_is_in_melee_reach` (bounding radius + `t ≤ R + r + range`).
  - `contour_footprint_tests.rs`:
    - C-3 `a_worker_passes_a_diagonal_depot_wall_and_a_zergling_does_not` (the owner's case end to end).
      - Setup: two shipped 2×2 structures with chamfer 0.5 (chosen by shape) corner to corner in the staircase map. A scratch body with `t` 0.3125 and `r` 0.375 is ordered from (3, 18) to (18, 3); so is a scratch body with `t = r = 0.375`.
      - Assertions: the first arrives within 400 ticks. The second never crosses `x + y = 24`, and its order ends without a diagnostic.
    - C-4 `a_pushed_zergling_never_enters_the_gap`: a hostile body holding at 45° pushes it. Its distance to each chamfer stays ≥ `t − 2⁻²⁰`, and it never crosses.
    - C-5 `a_unit_rounds_a_hall_corner_inside_its_old_square`: a slim body walks diagonally past a 5×5's cut corner. It passes inside the Place square, its distance to the chamfer stays ≥ `t − 2⁻²⁰`, and it arrives. On WP5 it detours outside the square.
    - C-6 `units_stand_on_a_spore_analogues_outer_cells`: a body rests at `t` from the `(0.5, 0.5, 0.25)` shape, inside the 2×2 cells.
    - C-7 `a_one_cell_sensor_analogue_is_placed_built_and_walked_round` (by shape and size).
    - C-8 data property `every_shipped_ground_archetype_meets_the_corner_gap_by_its_terrain_radius`: an archetype passes iff `8t² ≤ 1`, read from its `terrain_radius`; every harvester passes.
- **Change:** the 49 shapes and four B-138 rows of §2.3, each with a comment; D-10's loader rule.
- **Measure:** D-14, including control runs (a) and (b).
- **Stop rules:** D-15, 5 to 7.
- **Expected test edits** (**verify** by running; each re-derived):
  - tests that place or attack the Sensor, Spire, Greater Spire or Spore analogues at pinned cells or distances;
  - `structure_contact_tests` S-1 and S-4 if their chosen structure's corners now matter (they act at face middles, so none is expected);
  - `production_transactions_tests` exits;
  - `harvest_income_tests`, where trips past a hall's cut corner may shorten;
  - `full_match_tests` and `match_result_tests` end ticks.

### WP7: client, presentation, measurement (S)

- **Client tests:**
  - `effects::catalog::units` `a_structures_muzzle_is_its_drawn_dome` revised to the shape (the Spore analogue's muzzle 0.5);
  - `battle_input::command_mode` `a_one_cell_footprint_centres_on_the_clicked_cell`.
  - Both are red on WP6's tree only if the code reads the old size; **verify**.
- **No client code change is expected** beyond docs (D-11). **Verify** with `grep -rn "footprint_w\|drawn_radius\|BuildingDome {" crates/client/src`.
- **Picture check:** captures on the RX 580 under `with-gpu-lock` (charter §7, at least 60 s between holds) of:
  - a worker passing two 2×2 contour domes corner to corner;
  - a unit at a hall's cut corner;
  - the Spore, Sensor and Spire domes at their new sizes.

  These are recorded as observations. Anything that reads as a fault is registered under the next free number and left out of scope.
- **Storyboard:** if WP6 moved the default-seed timeline, re-check `assets/captures/showcase.ron` with real renders, one lock per take. `client_diagnostics/showcase_storyboard_tests` and `showcase_tests` must pass.
- **Throughput and mesh build** (D-14). Every scenario stays ≥ 25 ticks/s. The mesh rebuild of the 24-structure row is reported, square against contour. If it is more than 25 % slower, the agent profiles it and records the result; an optimisation needs an amendment.
- **Evidence:** §2.3 with the raw XML lines, the control runs, and the corner-gap property's table.

### WP8: bookkeeping (S)

- **Register:** B-134 and B-138 are removed as fixed, with commits. B-139 stays open.
- **`DESIGN.md` Movement row** gains: "…a structure meets units through its 5.0.14 footprint shape (a chamfered box authored as `footprint_shape`), cut into the mesh on a quarter-cell lattice; its cells remain its placement footprint (B-134, B-138 fixed, plan `2026-11-18-contour-footprints`)". Its tests gain `contour_footprint_tests.rs` and `wall_off_tests.rs`.
- **Combat row:** the floor sentence goes.
- **§2 `sim_map` bullet:** "footprint shapes".
- **Pointers:** amendments in `2026-11-18-structure-contact.md` (§10's B-134 item done) and `2026-11-18-structure-radius.md` (D-4's floors and D-2.3 replaced by D-10 here).
- This plan's row becomes `Done`, with commits and the evidence link.
- **Gate:** `merge` by the orchestrator.

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Lattice | Quarter units are exact and pure; margins hold | WP1 units, digest, every golden |
| Shape geometry | Vertices on the lattice for odd, even and non-square sizes; exact containment, segment and capsule tests | `footprint.rs` units (WP2) |
| Placement grid | Unwalkable ground refused; Place cells unchanged | WP2 |
| Mesh cut | Square shapes reproduce today's mesh; contour triangles per cell; cut corners open; no crossings for seeded layouts | `navmesh_tests` (WP3, property) |
| Walls (S9, S10) | Diagonal 2×2 gap: worker passes, Zergling stopped, exact threshold `8t² ≤ 1`; 5×5 cases; face-adjacent tight | `wall_off_tests` (WP3), `contour_footprint_tests` C-3, C-4, C-8 (WP6) |
| Contact | Exact against a chamfer; pushes never press through | `wall_contact_tests` (WP3), C-4 |
| Clearance | Conservative; square-identical | `clearance_field_tests` (WP3) |
| Consumers | Centre rule, disc placement, 1×1 construction | C-1, C-2, S-9 (WP5) |
| Data | 49 shapes and the radius-covers-shape rule sourced | reference tables, loader tests, R-5 (WP6) |
| Determinism | Pure packages bit-identical; schema 30 proven checksum-only; data step traced with control runs; native and wasm | D-14, D-15 |
| Charter | C4, C5 (shape and size, never slug), C7, C14, C17, C18, C22 | `workspace_checks`, `suite_catalog_tests` |

---

## 8. Determinism summary

- **Exact integers throughout.** Shapes are authored as `f64` and converted once at load, exactly (multiples of 1/4, validated; C7 "reads authored data"). The mesh, labels, raster and predicates are integer (`i64`/`i128`, Q16 where the mesh already is). No float and no hashed collection; the sparse edge store is a sorted `Vec`.
- **Order.** Vertex and run insertion is row-major with a fixed direction order (D-4), so the mesh is a pure function of the map, independent of structure insertion order (navmesh invariant kept and tested).
- **Hashed state.** `PlacedStructure.shape` is new (schema 30, WP3). The clearance values change only with true shapes (WP6). `NavMesh` is derived and unhashed.
- **Re-records.**
  - Both goldens are re-recorded in WP3 with a scratch-encoding proof.
  - `golden_replay` is re-recorded in WP6 with the entity-only digest naming the first behavioural tick.
  - `PATH_GOLDEN` and `VALIDATION_GOLDEN` change only by D-5's counted refusals in WP2.
  - `SHOWCASE_MESH_DIGEST` is re-pinned once in WP1a by definition.
  - New pins: `CONTOUR_PATH_GOLDEN`, `contour_route_checksum`.
  - All replay on `wasm32-wasip1` (release in CI, debug once locally); CI covers Windows and macOS ARM64 on the push.
- **Map format.** `.scmap.ron` is unchanged; maps hold no structures, and start locations keep `footprint`. `format_version` stays 1 and `DECODE_GOLDEN` is unchanged. Validation check 13 is stricter by D-5. `GameMap` is not persisted to files (**verify**: `grep -rn "PlacedStructure {" crates` finds only `grid.rs`).

### Throughput

- The bench scenarios spawn no structures, so they see only WP1's shift constants and are expected within noise.
- Mesh rebuilds per structure change gain at most eight boundary segments and a few clipped lines per structure. They are measured (WP7).
- The contact early-out is unchanged per cell, so floods near structures are as today.
- Every scenario must stay ≥ 25 ticks/s; `DESIGN.md` §5 is updated only if the figures move beyond noise.

### Charter implications

- **C5:** shapes are catalog data. Tests choose structures by `ShapeKind::BuildingDome`, size and shape, and bodies by capability or scratch registration. The reference table names analogues.
- **C7:** authored `f64` is converted at load; presentation `f32` appears only in the snapshot and client.
- **C4:** `footprint.rs` gets a `//!` doc. `navmesh/mod.rs` invariants are rewritten (quarter lattice, diagonal constraints, clipped interior lines). The clearance invariant is reworded (D-6).
- **C11:** a `PERFORMANCE RATIONALE` block on the sparse quarter-edge store (Why: a dense quarter-lattice grid is 16× the whole-cell one per rebuild; What: sparse edges only inside Place rectangles; Trade-off: a sort per rebuild).
- **C14:** `MapError::InvalidFootprintShape`, `PlacementFailureReason::UnpathableTerrain`.
- **C12:** no `let _ =`.
- **C17, C18:** `DESIGN.md` links and the plan row.
- **C22:** new test files are `fast` unless measured `slow`.

---

## 9. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | The showcase flips again | D-15, 5: control runs, trace, rule-or-defect classification, amendment. S3 and S9 are the cited reasons for a rule-caused flip |
| R-2 | The funnel fails a gap whose width exceeds `2t` by less than `2 × CORNER_SAFETY_MARGIN`. The worker in a 0.707 gap has 0.082 to spare against the margin's 0.1 | The same regime as `LARGE_RADIUS` in a 2-wide gap, which passes today through `shrink` and `pinned_retry`. WP3's threshold property finds any failure. A failure is a funnel defect, traced and fixed by amendment; the margin is not tuned |
| R-3 | Diagonal walls are new to locomotion (slides, probes, `forms_wall`, corner sidesteps) | Contact is general over edge directions (B-113 normal). C-4 and C-5 test it. Showcase liveness bounds apply; a defect stops WP6 |
| R-4 | A crossing the lemma missed aborts a mesh build (`ConstraintRecovery`) | Unit-edge model by construction; the seeded layout property (64 cases, adjacent and diagonal placements beside cliffs) |
| R-5 | D-5 refuses start footprints or seeded placements in the validation corpus | Counted and traced in WP2; shipped maps must validate |
| R-6 | The Spore analogue's outer cells are walkable while it occupies 2×2 for placement, which may read oddly | It is the game's data (S6). The drawn dome shrinks to the shape (D-11); WP7 picture check |
| R-7 | A 1×1 site breaks construction | D-9, C-2, C-7 |
| R-8 | Hidden cell-based pathing consumers | §3's table. WP5's commit message audits `grep -rn "OCCUPIED_BY_STRUCTURE\|occupying_structure\|GROUND_PATHABLE" crates/sim_core/src crates/client/src`, each hit classified as Place (P1) or Shape (C1, C2) |
| R-9 | The schema 30 re-record hides a behaviour change | The scratch-encoding proof and the entity-only digest (D-15, 2) |
| R-10 | Mesh rebuild cost in skirmish | Measured (WP7); optimisation only by amendment |
| R-11 | The Liquipedia crawler claim (S9) is read as a mismatch | §2.2.4: the 5.0.14 data wins; recorded |

---

## 10. Out of scope and decisions recorded (Q0002)

- **B-139, footprint flag sets** (`NearResources`, `DropOff`, `Creep`): a separate mechanic, registered in WP0.
- **`PlacementFootprint` shapes** (S11): what the game does with them while placing (blocking units, pushing them aside) is not sourced. Our construction body check stays on the Place cells, matching the Terran and Protoss placement squares. No register row until it is sourced.
- **General polygon shapes and off-centre shapes** (add-ons, rocks, `Footprint3x3AddOn2x2`): the model extends to a vertex list later. No shipped analogue needs it.
- **Geysers as structures** (S9, "Vespene Geysers act exactly as a normal building"): our geysers are resource nodes. Not changed.
- **Lowered Supply Depot** (`Footprint2x2Underground`) and **lifted structures:** not modelled.
- **Ranges to the contour polygon:** ranges stay on the engine radius (radius plan R-3).
- **Harvest, return and build reach:** stay on the Place cells (radius plan D-3).
- **A clearance field on quarter cells:** rejected (D-6).
- **Drawing the contour** (an overlay): the game draws none. Not added.

## Amendment 1 (orchestrator, 2026-11-18): the funnel takes a fitted retry before its fallback (R-2); B-140 registered

**Decision.** The Proposed Amendment of evidence §3 is adopted in its recommended form.

1. **Fitted retry.** When a channel's spare width is below one corner margin, so that both the shrunk and the
   pinned funnel pulls fail, the funnel pulls once more with the margin `min(CORNER_SAFETY_MARGIN, (w_min − 2t)/2)`
   and scales `ARC_CHORD_DIP` with it, before it falls back to portal midpoints. Reasons: route existence is
   already exact (k ≤ 90 routes, k ≥ 91 does not, matching `8t² ≤ 1`), so the defect is only the route's shape; a
   route that is legal today never reaches the retry, so it is unchanged; and the 0.05 constant is not tuned
   (alternative 3 is forbidden by the plan). Alternative 2 is rejected because it breaks the owner's wall-off
   argument (Q0087: workers pass the 0.707 gap); alternative 1 would leave an illegal route in the code.
2. **Tests.** `a_diagonal_contour_gap_admits_exactly_the_discs_that_fit` passes for every k as specified (every
   leg of every route passes the segment predicate). Add a unit test of the retry on a synthetic channel whose
   spare width is a quarter of the margin, and a guard that a channel with spare width ≥ the margin produces the
   same route bytes as before the change. The fallback, if still reachable, is counted and asserted zero in the
   proptest.
3. **Determinism.** With full-square shapes the retry is unreachable on shipped maps, so WP3 stays bit-identical
   to WP2 everywhere until the data WP; the agent shows this with the same runs as WP1b.
4. **B-140 registered** (finding of this plan, not fixed here): `sim_map` `validation_golden_tests`' three corpus
   digests differ between native and `wasm32-wasip1` on the base tree too (validation counts: unreachable 0
   against 4, warnings 2,487 against 2,362). Map validation is part of the simulation boundary that charter §3.1
   requires to be bit-identical on every target, so this is a determinism bug, not a test detail. It gets its own
   investigation; the WP0 agent of this plan adds the row with the evidence and does not change the pins.

WP3 resumes from `contour-footprints-wp3-wip` with the retry, then `CONTOUR_PATH_GOLDEN`, `contour_route_checksum`
and the `navmesh_measure` row, and continues to WP8 under the plan's stop rules.

## Amendment 2 (orchestrator, 2026-11-18): a direct pursuit stalled against a wall asks the mesh for a route; WP6 deviations confirmed

**Why.** WP6 stopped on D-15 (evidence §6). The seven hall contours alone move the showcase. Player 0's two workers
chase with no route, walk into a hall's 45° chamfer that faces their heading, and B-113's rule waits out the sliver
slide. Their pursuit is abandoned and re-acquired every two to four ticks: 345 to 469 `PursuitAbandoned` per seed,
and seed 7 flips. Against a square the same heading met a face at about 45°, so the slide was large. This is the
plan's defect class "a stall at a chamfer", not a rule-caused change. In StarCraft II an attacking unit always
follows the pathing of its target's position; a unit does not stand pressed against a building's corner.

**Decision (the agent's recommendation, adopted).**

1. When a direct (route-less) pursuer's step is waited out against terrain or a structure (a sliver slide, not a
   body), it asks the mesh for a route to its target, through the same request a move order's stall repath makes,
   instead of counting toward `PursuitAbandoned`. Route-following already rounds a chamfer at the unit's terrain
   radius (WP3, C-5). A body block keeps today's behaviour. If the mesh finds no route, today's abandon rule applies
   unchanged. No new constant: the trigger is B-113's existing sliver test, and the route is replanned under the
   existing stall-repath rules.
2. Before changing it, trace the pursuit band and stall counter in `movement/locomotion.rs` and record the trace in
   evidence §6. New test **C-9**: a worker chasing a target past a 5×5's cut corner reaches it, with zero
   `PursuitAbandoned`. It must be red on the WP6 side branch first.
3. Then WP6's measurements are repeated in full under D-15. That includes the matrix, battle 33, the showcase on the
   default seed and seeds 1 to 8 with controls, and the goldens. Re-record `golden_replay` only with the
   entity-only proof header, if it still holds through tick 1,000, or with a full trace if it does not. The stop
   rules stay as written. A further flip or a give-up count above base + 1 stops the package again, with a trace.
   An outcome change traced to units now rounding corners that they used to hug is rule-caused and is reported with
   its trace.

**Deviations confirmed.**

- **D-5 for a structure's terrain radius.** A structure's terrain radius is its shape's smaller half-extent, not
  half its footprint's smaller side. It is unchanged for every full-box shape and is 0.5 for the Spore analogue, so
  the invariant terrain ≤ collision radius holds. This supersedes structure-radius D-5 for structures; the agent
  records it in that plan too as a one-line cross-reference.
- **Amendment 1's zero-fallback proptest** is replaced by the deterministic `contours_add_no_funnel_fallbacks`. The
  fallback classes that exist with full squares are registered as B-141.
- **`w_min`** is measured across the channel's left and right obstacles.
- **B-140**: the path corpus is traced to `usize` range draws. The decode and validation corpora stay open under
  B-140 and are not part of this plan.
