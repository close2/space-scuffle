# Blueprint: Resource Node Footprints (B-147)

This plan gives resource nodes their StarCraft II 5.0.14 footprints:

- **Mineral fields** become 2×1 obstacles (`FootprintMineralsRounded`, its `Shape` a box of ±1 × ±0.5), and
  **Vespene geysers** 3×3 ones (`FootprintGeyserRounded`, its 5.0.14 `Shape` the 16-vertex outline of the
  Legacy of the Void layer). Today each node is one cell that blocks no path (B-147).
- **Workers harvest from the footprint's edge.** A worker gathers when its body touches the field and delivers
  when it touches the town hall, as the game's harvest ability does. Today it stops up to 0.75 short at both ends
  (`TASK_REACH`), and our mineral income is 92 per worker-minute where the game's is 55 to 61.
- **A mined-out mineral field leaves the map** (`RemoveWhenEmpty`), and the navigation mesh is rebuilt. The
  register's **verify** is resolved: the flag is in the data. Geysers stay when empty, as in the game.
- **The mineral `NearResources` reach** becomes the game's own 8 × 7 rows on the 2×1 field. That removes plan
  `2026-11-19-footprint-flags` D-3's recorded minor difference.

A full plan is required (charter §6). It is a new mechanic for the navigation mesh, it changes the checksum
schema, it moves economy worlds, the showcase's timeline and both goldens, it re-lays both shipped maps, and it
spans `sim_types`, `sim_map`, `sim_core`, the catalog, the maps, `client` and the evidence.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-20 against `main` at
`fb66d85e`. Every measurement of §6 is on that tree. `main` has since gained plan
`2026-11-19-partial-knowledge-funnel` (`e58f1af1`, routing under partial knowledge); each package's agent re-measures the
base figures of §6.3 and §6.6 on its own base before applying any stop rule (crowd-slide D-3).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations S-1 to S-6 | Closed. S-1 (RON data instead of editor data) covers authoring footprints and shapes in RON; S-5 (25 ticks per Faster second) converts the harvest times; none of the others applies | D-1, §2.4 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §10 |
| Q0002: scoping is the agent's call if written down | Standing rule | §13 |
| Q0083, Q0088 (GPU driver, fair lock queue) | Touch only WP7's renders, which follow the guide's lock rules | WP7 |
| **None new** | The data decides every rule. The map layouts (D-10), the public removal (D-9) and the items of §13 are recorded scoping decisions | §4, §13 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Mineral field | One cell; blocks no path; units may stand and spawn on it | A 2×1 footprint (the anchor cell and the cell east of it) and a 2×1 box shape, cut into the navigation mesh; nothing spawns, stands or builds on it |
| Vespene geyser | One cell (the centre of the extractor's 3×3) | A 3×3 footprint centred on the anchor cell, its pathing shape the 16-vertex 5.0.14 outline (§2.2); an extractor is placed exactly over it |
| Footprint data | None (the `NearResources` rings only) | `assets/catalog/placement_rules.ron`: per resource kind, the game's `Place` rows (`x` own cells, `o` ring), the anchor, the shape and `remove_when_empty` |
| Harvest reach | Within 0.75 of the node's cell and of the depot's cells (`TASK_REACH`, B-67) | Contact: the body touches the node's shape, and the depot's shape (D-7). `TASK_REACH` stays for building, construction and loading |
| Mined-out mineral field | Stays on the map | Leaves the map and the mesh in the tick it empties (D-8); its id stays, with `remaining` 0 |
| Mineral `NearResources` reach | Anchored on our one cell (footprint-flags D-3, a recorded minor difference) | The game's 8 × 7 rows around the 2×1 field (52 cells) |
| Income, one worker per field, showcase start | 91.9 per minute (8 fields) | 62.8 (prototype); the game's 55 to 61, 62.1 once its unmodelled `ReturnDelay` is allowed for (§6.2) |
| Shipped maps | Minerals in adjacent cells (`showcase_canyon`) or a 3-row block (`default_arena`) that would overlap or enclose fields once they are 2×1; two geyser pairs that would sit on the army's exit or under the goldens' halls | Both maps re-laid (D-10): ladder-like two-row mineral lines with exposed faces, the showcase's geysers at the back of each base |
| Hashed state | Node positions | Also each node's placed footprint and each cell's resource occupant; schema 31 → 32 |
| Worlds that move | — | Matrix, battles 31 and 33: none (no resource nodes). Goldens: hashed state only, entity state identical at every checkpoint (prototype). Showcase: winners unchanged on nine seeds, end ticks +0 to +74, reversals 174 → 205 (prototype, §6.3). Economy tests: income falls by a third |
| Size | — | L: nine packages, three of them M |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`).
  - "Footprint:N" is line N of `mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`, read as data from
    `~/scratch/footprint-flags/dl-footprint/FootprintData-liberty.xml`; "void Footprint:N" is the same file under
    `void.sc2mod` (`FootprintData-void.xml`). `libertymulti`, `swarm`, `swarmmulti` and `voidmulti` redefine no
    resource footprint (plan `2026-11-19-footprint-flags` §2).
  - "liberty:N", "liberty Behavior:N", "liberty Abil:N" and "balancemulti Behavior:N" are lines of `UnitData.xml`,
    `BehaviorData.xml` and `AbilData.xml` under `~/scratch/combat_wp3_data/<mod>/`.
- **[COMM]** Community documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | liberty:36–75 (`MineralFieldDefault`, the parent of every mineral field: `MineralField`, `MineralField750`, `RichMineralField` …, liberty:2094–2110), its `Footprint` at liberty:68 | `Footprint value="FootprintMineralsRounded"`; `Radius 0.75`; `Collide` Ground, Structure, Small …; `FlagArray CreateVisible 1` (liberty:42) [DATA] |
| S2 | Footprint:1061–1096 (`FootprintMineralsRounded`) | `Place` (:1077–1091), `Area="-4,-3,4,4"`, rows `.oooooo.`, `oooooooo`, `oooooooo`, `oooxxooo` (:1087), `oooooooo`, `oooooooo`, `.oooooo.`: the field's own cells are the two `x`, a 2×1. `Shape` (:1092–1096): `Offsets -1,-0.5;-1,0.5;1,0.5;1,-0.5` (:1094), a 2 × 1 box, `Radius 1.1181` [DATA] |
| S3 | liberty:2116–2145 (`VespeneGeyser`), 2147–2177 (`RichVespeneGeyser`); `Footprint` at liberty:2127 | `Footprint value="FootprintGeyserRounded"`, `Radius 1.5` [DATA] |
| S4 | Footprint:1535–1576 (`FootprintGeyserRounded`) | `Place` (:1554–1570), `Area="-4,-4,5,5"`, own cells the 3×3 `xxx` (:1564–1566). Its liberty `Shape` is a ±1 square (`Offsets` :1573, `Radius 1.4`) [DATA] |
| S5 | void Footprint:3–21 (`FootprintGeyserRounded`) | Legacy of the Void replaces the geyser's `Shape` (:16–20): 16 `Offsets` (:18) joined by `Borders` (:19) into one outline: the ±1 square whose four corners are pushed out into diagonal lobes reaching ±1.25 (vertices `(±0.5, ±1)`, `(±0.75, ±1.25)`, `(±1.25, ±0.75)`, `(±1, ±0.5)`), `Radius 1.4577`. It also rounds the ring rows 0, 1, 7 and 8 (:5–14) [DATA] |
| S6 | Footprint:922–962 (`FootprintGeyserRoundedBuilt`, the extractors' footprint) | `Shape` ±1.5 with corners cut by 0.75 (:960). It contains S5's outline [DATA, INF: every S5 vertex satisfies `|x| ≤ 1.5`, `|y| ≤ 1.5`, `|x| + |y| ≤ 2.25`] |
| S7 | liberty Behavior:743–756 (`MineralFieldMinerals`); balancemulti Behavior:251–256 | `HarvestTime 2.786` (:747), `HarvestAmount 5` (:748), `Flags RemoveWhenEmpty 1` (:749), `ReturnDelay 0.5` (:750). balancemulti sets `Contents 1800` [DATA] |
| S8 | liberty Behavior:801–807 (`RawVespeneGeyserGas`), 817–828 (`HarvestableVespeneGeyserGas`) | No `RemoveWhenEmpty`: a geyser stays when empty. Gas `HarvestTime 1.981`, `HarvestAmount 4` [DATA] |
| S9 | core Abil:353–380 (the `CAbilHarvest` default); liberty Abil:1094–1099, 2241–2243 (`SCVHarvest`, `ProbeHarvest`, `DroneHarvest`); liberty Abil:703–714 (`MULEGather`) | The workers' harvest abilities set no `Range`; the MULE's sets `Range 0.5` (:706). The engine default of `Range` is not in the data (**verify**: taken to be 0, so a worker gathers and returns on contact) [DATA, INF] |
| S10 | Liquipedia, [Resources](https://liquipedia.net/starcraft2/Resources) (wikitext read through the MediaWiki API on 2026-11-20) | "Once a mineral field has no more minerals left it is depleted and disappears from the map." "A Vespene Geyser is depleted once it has no more gas, but the geyser as well as the building on top of it remain on the map." "A worker occupies a mineral field for 1.99 seconds. It then pauses for approximately 0.3571 seconds before returning at full speed." Measurements: "One worker on one mineral field gathers 55-60 minerals per minute … depending on distance"; two workers 110–120; three 143; "With two workers per mineral field, a base with 8 mineral fields will harvest about 925 minerals per minute" [COMM] |
| S11 | Liquipedia, [MULE (Legacy of the Void)](https://liquipedia.net/starcraft2/MULE_(Legacy_of_the_Void)) (same date and method) | "SCVs average 61.22 minerals per minute on near blue mineral patches and 53.57 minerals per minute on far blue mineral patches (for two or less workers per patch)" [COMM] |
| S12 | Liquipedia, [Mining Minerals](https://liquipedia.net/starcraft2/Mining_Minerals) (same date and method) | Fields differ in travel distance; "any patch with 3 workers will yield exactly the same rate regardless of distance"; a worker that finds its field occupied looks for another (B-61) [COMM] |

### 2.2 The rules that follow

1. **Two layers, as for structures.** A resource's `Place` `x` cells are what it takes from the placement grid:
   nothing is built on them (they inherit the default `NoBuild` and `GroundNoSpawn`, core Footprint:3–17) and no
   unit spawns there. Its `Shape` is what it cuts into the pathing mesh (contour plan §2.2 rule 1).
2. **Anchoring.** The game's `Area` is relative to the cell holding the unit's position, and the `Shape` to the
   position itself [INF from S2, S4 and the contour plan's §2.2 rule 2]:
   - a mineral field's position is a lattice point in `x` and a cell centre in `y`. Its two cells are the cell
     holding the position and the one west of it, and its box is centred on the position;
   - a geyser's position is a cell centre; its 3×3 is centred on that cell.
3. **The pathing shapes (5.0.14, the void layer applied).**
   - Mineral field: the full 2 × 1 box of its cells (S2).
   - Geyser: the 16-vertex outline of S5, in world units from the geyser's centre, in order round the outline
     (`Borders`, void Footprint:19): `(−0.5, −1)`, `(−0.75, −1.25)`, `(−1.25, −0.75)`, `(−1, −0.5)`,
     `(−1, 0.5)`, `(−1.25, 0.75)`, `(−0.75, 1.25)`, `(−0.5, 1)`, `(0.5, 1)`, `(0.75, 1.25)`, `(1.25, 0.75)`,
     `(1, 0.5)`, `(1, −0.5)`, `(1.25, −0.75)`, `(0.75, −1.25)`, `(0.5, −1)`. It is not convex: each side is
     recessed to ±1 over its middle cell. Every vertex is on the quarter-cell lattice once placed, and every edge
     runs along an axis or a diagonal, so it fits the mesh's unit-edge model (contour plan D-4) [INF, checked by
     hand].
4. **A geyser under an extractor.** The extractor's footprint is the same 3×3 (`3x3CappedGeyser`, plan
   footprint-flags S11), and its shape contains the geyser's (S6). While it stands the union is the extractor's
   shape; when it goes, the geyser's outline is left [INF].
5. **Depletion.** A mineral field is removed when it empties (S7 `RemoveWhenEmpty`, S10). A geyser is not (S8,
   S10). With the field gone, its `Place` flags (`NearResources`, `NoBuild`) and its pathing go with it [INF: a
   removed unit's footprint is removed].
6. **Workers mine from the edge.** The harvest ability has no range of its own for workers (S9), so a worker walks
   until it touches the field's shape, gathers for `HarvestTime`, waits `ReturnDelay`, walks until it touches the
   town hall and delivers [INF from S9 and S10's timings, **verify** the engine default]. The figures of S10 and
   S11 are consistent with this: §6.2 reproduces them within 2 % once `ReturnDelay` is allowed for.
7. **Visibility.** Mineral fields and geysers are created visible to every player (`CreateVisible`, S1) and seen
   through fog as snapshots. Whether a player's units path round a field that has gone in fog is not sourced
   (**verify**); D-9 takes routes to follow the truth.

### 2.3 Values

| Kind | `Place` cells | Anchor | Shape (world units from the footprint's centre) | `NearResources` ring | Removed when empty |
|---|---|---|---|---|---|
| Mineral | 2×1: the anchor and the cell east of it | The west cell (column 3 of the 8 × 7 rows) | Box ±1 × ±0.5, uncut | The game's rows (S2): 52 cells, the field's 2 included | Yes (S7) |
| Gas | 3×3 centred on the anchor | The centre cell (column 4, row 4 of the 9 × 9) | S5's 16 vertices (§2.2 rule 3) | Unchanged: the void-rounded 9×9 (69 cells) | No (S8) |

### 2.4 Harvest timings (for §6.2's band)

At 25 ticks per Faster second (S-5) and the game's 1.4 Faster factor: `HarvestTime 2.786` is 1.99 s, 49.75
ticks (ours: `harvest_rate_ticks: 50`); `ReturnDelay 0.5` is 0.357 s, 8.93 ticks (ours: none; registered by WP0,
§13). Gas `HarvestTime 1.981` and `HarvestAmount 4` belong to the resource in the game and to the worker here
(ours: 50 ticks and 5 for gas too; registered by WP0, §13).

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | A node is `ResourceNode { pos, kind, remaining, cell }`, `pos` the cell's centre. The map file's `MapResource { id, kind, cell, amount }` is decoded one to one | `crates/sim_map/src/grid.rs`, `map_format.rs` `to_game_map` |
| T2 | Nodes mark no cell. The mesh, the clearance field, the centre rule, unit placement, spawn rings and approach rings ignore them | `navmesh/mod.rs`, `pathfinding.rs`, `movement/flee.rs`, `movement/geometry.rs`, `scenario.rs`, `scenario/placement.rs`, `economy/mod.rs` |
| T3 | A site may not cover a node's cell (`CoversResource`), and an extractor must cover a gas node's cell (`ExtractorNeedsGas`) | `production/construction.rs` `check_site` |
| T4 | The `NearResources` reach of every node counts, depleted ones too (B-147) | `construction.rs` `check_flags` |
| T5 | A worker gathers within `radius + TASK_REACH` (0.75) of the node's cell, walking to the cell's centre when it is walkable (`approach_cell`), and delivers within the same reach of the depot's cells | `economy/mod.rs` `within_reach`, `approach_cell`; `economy/harvest.rs` `step_gather`, `step_delivery` |
| T6 | An emptied node stays; `next_node` sends its workers on; the client hides an exhausted node it knows of (`node_present`) | `harvest.rs`; `client/src/battle_pipeline/fog.rs`, `runtime_battle/scene.rs` |
| T7 | No shipped scenario, matrix battle, golden or bench row issues a `Harvest` order (`grep` of `assets/scenarios`, `crates/sim_core/tests/fixtures`, `crates/sim_fixtures`, `crates/sim_core/benches`). Harvesting is exercised by `harvest_income_tests`, `full_match_tests`, `player_view_tests` and client tests | — |
| T8 | `golden_replay` places its halls at cells (6, 6) and (52, 52) of `default_arena`, over that map's mineral lines and, with a 3×3, over the geyser at (11, 11) and (55, 56) | `crates/sim_core/tests/fixtures/golden_replay.ron` |

**Finding F-1 (income).** Today a worker earns 91.9 minerals a minute on a showcase start against the game's 55
to 61 (S10, S11): 50 % more. Most of it is `TASK_REACH`: with 2×1 obstacles and the old reach the prototype
earns 82.5, with contact 62.8 (§6.2). B-67 classed task reach as a tolerance with "no observable game
counterpart"; for harvesting it is observable through income, so D-7 replaces it there.

**Finding F-2 (map layouts).** `showcase_canyon`'s eight fields per start are adjacent cells, so 2×1 fields would
overlap. `default_arena`'s are a three-row block, so the middle field would be enclosed. Both maps must be re-laid
(D-10).

**Finding F-3 (geysers at the exit, §6.3).** With nodes as obstacles and the shipped geyser cells, the two
geysers east of each showcase hall leave a gap of one cell (1.5 between their outlines) on the army's first
waypoint at tick 0 (unit 6: `(14.55, 8.13)`). The armies funnel through it: reversals rise from 174 to 7,189 over
nine seeds, stall give-ups from 0 to 31, and four seeds flip. Geysers alone, spread to a three-cell gap, still
flip all nine seeds and raise reversals to 1,027. With both kinds, the geysers spread and the scenario's targets
moved off the bases, one unit reverses 345 times beside a geyser between ticks 714 and 1,356, in runs of up to 12
consecutive ticks (seed 6, unit 4, at `(46.3, 56.2)`). Minerals alone keep every winner. So the
geysers are re-laid behind the bases (D-10), and the oscillation is registered (WP0) as a locomotion finding
beside rounded obstacles, out of this plan's scope.

---

## 4. Design decisions

### D-1. Resource footprints are catalog data

`assets/catalog/placement_rules.ron`'s `resource_footprints` rows become the game's `Place` layers:

```ron
// 5.0.14 FootprintMineralsRounded (liberty Footprint:1061-1096; liberty UnitData:68). Place rows
// :1083-1089, read with rows[0] the lowest y: `x` the field's own 2x1, `o` the NearResources ring.
// The anchor is the field's west cell, the map file's `cell`. Shape Offsets :1094 (a 2 x 1 box).
// RemoveWhenEmpty: liberty BehaviorData:749.
(kind: Mineral, applies: [NearResources], anchor: (3, 3), rows: [
    ".oooooo.", "oooooooo", "oooooooo", "oooxxooo", "oooooooo", "oooooooo", ".oooooo.",
], shape: Box(half_width: 1.0, half_height: 0.5, chamfer: 0.0), remove_when_empty: true),
// 5.0.14 FootprintGeyserRounded (liberty Footprint:1535-1576, rows 0, 1, 7, 8 and Shape as void
// Footprint:3-21; liberty UnitData:2127). The anchor is the centre cell. Shape: void Footprint:18-19.
// No RemoveWhenEmpty (liberty BehaviorData:801-807).
(kind: Gas, applies: [NearResources], anchor: (4, 4), rows: [
    "..ooooo..", ".ooooooo.", "ooooooooo", "oooxxxooo", "oooxxxooo",
    "oooxxxooo", "ooooooooo", ".ooooooo.", "..ooooo..",
], shape: Outline([(-0.5, -1.0), (-0.75, -1.25), (-1.25, -0.75), (-1.0, -0.5), (-1.0, 0.5), (-1.25, 0.75),
    (-0.75, 1.25), (-0.5, 1.0), (0.5, 1.0), (0.75, 1.25), (1.25, 0.75), (1.0, 0.5), (1.0, -0.5),
    (1.25, -0.75), (0.75, -1.25), (0.5, -1.0)]), remove_when_empty: false),
```

- `x` cells are the node's own cells (its footprint), `o` cells its ring; both carry `applies`. This is the game's
  notation (S2, S4), so the rows are copied, not re-encoded. The old `x`-only masks are replaced; the loader
  refuses a row with no `x`.
- The mineral row is the game's 8 × 7, anchored on the field's west cell. That removes footprint-flags D-3's
  recorded minor difference: the reach is 3 cells beyond the 2×1 on every side, with one corner cell cut (52
  cells).
- `shape` is the pathing shape in world units from the centre of the `x` cells: `Box` is the contour plan's
  chamfered box, `Outline` a vertex list (D-2).
- **Resolved** at load into `sim_map::ResourceFootprint { cells: (dx, dy, w, h) from the anchor, shape:
  FootprintShape, remove_when_empty: bool }` per kind, held in a `sim_map::ResourceFootprints` table that
  `UnitCatalog::placement_rules().resource_footprints()` returns. `sim_map` owns the type because it places the
  footprint (D-4); the catalog only fills it (charter §3.3, C6: read at run time, nothing embedded).
- **Loader validation** (`catalog/validation.rs`, typed `invalid(...)` errors), added to footprint-flags D-2's:
  1. the `x` cells form one rectangle of at most 4 × 4 cells, and the anchor is one of them;
  2. the shape lies in the `x` rectangle, every vertex is a multiple of 1/4, and (D-2) the outline is simple, has
     3 to 16 vertices, and every edge runs along an axis or a diagonal;
  3. both kinds have a row (a node of a kind with no footprint could not be placed).
- `ResourceKind` names a mechanic class, not a catalog identity (charter §3.3, C5), as in footprint-flags D-2.

### D-2. Shapes: the chamfered box gains an outline form

The geyser's 5.0.14 shape is a non-convex 16-gon (S5). The contour plan's `FootprintShape` is a chamfered box,
and its §10 names the extension: "the model extends to a vertex list later".

- `sim_types::FootprintShape` becomes an enum: `Box { half_width_q, half_height_q, chamfer_q }` (today's fields
  and meaning) and `Outline(QuarterOutline)`, where `QuarterOutline` is up to 16 vertices in quarter cells from
  the footprint's centre, stored as `[(i16, i16); 16]` with a `u8` count (`Copy, Eq, Hash`, no allocation).
- `sim_map` places every shape as one outline. `PlacedFootprint { top_left, width, height, outline }`, with the
  outline as lattice points in counter-clockwise order, holds the geometry of D-7 of the contour plan
  (`contour`, `contains_open_scaled`, `meets_open_half_cell`, `segment_meets`, `capsule_meets`) for both
  structures and nodes. `PlacedStructure` keeps its fields and gains `fn footprint(&self) -> PlacedFootprint`.
- **The predicates become general and exact.** Containment is the crossing-number test on `3·` coordinates (a
  centroid is never on a constraint); a half-cell meets the open outline when an outline vertex lies strictly
  inside it, a cell corner lies strictly inside the outline, or an edge properly crosses it; segment and capsule
  tests are per edge, as today. The mesh's interior cell lines are clipped to the outline's open interior as a
  sorted list of intervals, which is one interval for every convex shape.
- **Equivalence.** For every `Box` shape the outline is today's 4 or 8 vertices in today's order, and each
  predicate returns today's answer. WP1 is bit-identical everywhere (§7 rule 1), with the general predicates
  property-tested against the box closed forms.
- **Rejected:** the geyser as the chamfered hull ±1.25 cut by 0.5 (it blocks a band up to 0.25 wide in the middle
  of each side), and as liberty's ±1 square (S4, not 5.0.14). Both would be recorded differences where the data
  can be followed exactly. The prototype used the hull; §6 measures it as the upper bound of what the geyser
  blocks, and the square as the lower one.

### D-3. Anchoring and the map format

- `MapResource.cell` stays and becomes "the node's anchor cell; the catalog's resource footprint places the
  node's cells relative to it" (doc and `//!` invariant changes only). For gas the anchor is today's meaning (the
  centre, where the extractor is centred, T3); for minerals it is the west cell of the 2×1.
- **No format change.** No field is added and `format_version` stays 1: the file still says which kind stands
  where. `DECODE_GOLDEN`'s code path is unchanged; its value moves only with the maps (WP3).
- A v1 file written for one-cell nodes is still read; if its fields overlap once they are 2×1, D-4's placement
  refuses it with a typed error naming both nodes. A silent reinterpretation cannot pass: every shipped map is
  guarded (WP3).
- `ResourceNode.pos` becomes the footprint's centre: `(x + 1, y + ½)` for a field anchored at `(x, y)`, the cell
  centre for a geyser. It is hashed (as today).

### D-4. Footprints are placed when a world is built

- `sim_map::GameMap::place_resource_footprints(&mut self, table: &ResourceFootprints) -> Result<(), MapError>`
  places every node in id order. Typed errors (`MapError::ResourceFootprint { node, reason }`): out of bounds;
  overlap with another node (both named); a cell that is not walkable, not buildable, a ramp, or on another cliff
  level than the anchor; already placed.
- `SimWorld::with_catalogs` calls it before building the pathfinder and becomes fallible: it returns
  `Result<Self, SimCoreError>` with `SimCoreError::ResourceFootprint(MapError)`. Its 40 call sites (listed by
  `grep -rn "with_catalogs(" crates`) propagate the error; `sim_fixtures::world` and the scenario loader return
  it. No world exists without its node footprints.
- **Map validation** gains `sim_map::validation::validate_resource_footprints(doc, table) -> Vec<MapValidationError>`
  with the same checks plus: no node cell under a start footprint; no start footprint in any node's ring (the
  footprint-flags WP4 guard, now with the game's mineral rows); every node has a walkable cell beside its
  footprint. `validate_map`'s signature and its 53 callers are unchanged; the scenario loader and the editor call
  the new function too.
- `ResourceNode` gains `placed: Option<PlacedFootprint>`: `None` before placement and after removal (D-8), `Some`
  while the node stands. A node blocks exactly when it is placed.

### D-5. Occupancy and its consumers

- **Cells.** Placing a node sets `CellFlags::OCCUPIED_BY_RESOURCE` and `MapCell.occupying_resource:
  Option<ResourceNodeId>` on its cells, clears `BUILDABLE | GROUND_PATHABLE`, and keeps the prior flags on the
  `PlacedFootprint`, as `place_structure` does. Removal restores them. The flag is runtime state, so
  `MapDocument::from_game_map` refuses it (`RuntimeStateInCell`), as it refuses structures.
- **Consumers** (the contour plan's §3 table, extended; the WP4 and WP5 commit messages audit
  `grep -rn "OCCUPIED_BY_STRUCTURE\|occupying_structure\|GROUND_PATHABLE" crates/*/src`, each hit classified):

| # | Consumer | Change |
|---|---|---|
| M1 | Mesh (`navmesh/mod.rs` `classify_cell`, `edge_kind`, `label_triangle`) | A node's outline and its clipped interior cell lines are `Seam` constraints, as a structure's. A triangle whose centroid is inside a standing node's outline is `TriangleKind::Resource(ResourceNodeId)`; that test runs before the structure test, so a geyser under an extractor stays closed to a player who does not know the extractor |
| M2 | Request passability (`open`) | `Resource(_)` is closed to every request: nodes are public (S1, D-9) |
| M3 | Lane cuts (`lanes.rs`) | Unchanged: built once from terrain, as for structures; a node is not a lane obstacle (recorded, §13) |
| M4 | Clearance field (`pathfinding.rs` `mark_structure_walls`) | Nodes are rasterised in the same pass, by the general half-cell test (D-2). The static (terrain) field is unchanged |
| C1 | Centre rule (`flee.rs` `is_position_pathable`) | A resource cell refuses a segment that meets the node's closed outline |
| C2 | Unit placement and egress (`geometry.rs` `ground_footprint_connected`) | Node outlines are tested with the disc as structures are; `GameMap::structure_occupants_touched` becomes `footprint_occupants_touched`, yielding structures and nodes |
| C3 | Contact, routes, nearest reachable point | No change: they follow the mesh |
| P1 | Placement (`grid.rs` `can_place_building_where`, `place_structure`) | A resource cell refuses with `PlacementFailureReason::OccupiedByResource(ResourceNodeId)`; D-6 is the only exception |
| P2 | Site rule (`construction.rs` `check_site`) | `CoversResource` when a site covers any standing node's cell; an extractor must cover exactly one standing gas node's 3×3. These keep their reasons: they run before the grid check reports the occupant |
| P3 | Flags (`check_flags`) | Only standing nodes apply their ring (S7, §2.2 rule 5) |
| P4 | Spawn and approach rings (`scenario.rs` two ring finders and `standable_cell_centre`, `scenario/placement.rs` `admits` and the hall region, `economy/mod.rs` `cell_is_walkable`, `traversal.rs`) | A resource cell is not standable |
| P5 | Vision (`world/vision.rs`), knowledge (`known_blocked_cells`) | Unchanged: nodes are not structures and are always known |

### D-6. An extractor over a geyser

`GameMap::place_structure_over_resource(id, node, footprint)` places a structure exactly on a standing gas node's
cells and refuses anything else (`MapError::NotOverResource`). The cells then hold both occupants:
`OCCUPIED_BY_STRUCTURE` is added, the node's occupancy stays, and the structure's prior flags are the
resource-occupied ones, so removing the extractor leaves the geyser. Construction (`place_foundation`) and
`spawn_structure` call it for a gas-extractor archetype; every other structure goes through `place_structure`.

### D-7. Workers harvest from the footprint's edge

- **Arrival** for a harvest leg (to a node) and for a delivery leg (to a depot) is contact: the distance from the
  worker's centre to the target's shape (the node's outline, the depot structure's outline) is at most the
  worker's collision radius. The distance is the exact squared distance on the mesh's Q16 grid, as
  `capsule_meets` computes it.
- **The goal** of a leg is the point at the worker's terrain radius from the target's shape, on the side of the
  walkable cell next to the footprint that is nearest the worker (today's `approach_edge` ring choice). The
  terrain radius is the closest a body may stand (contour plan D-8), and terrain radius ≤ collision radius is a
  loader invariant, so a leg that ends at its goal has arrived.
- **No new constant.** Both distances are the worker's own radii; the 0.0625 between them (workers: collision
  0.375, terrain 0.3125) is the arrival slack.
- `TASK_REACH` stays for `Build`, `Construct`, `Load` and unloading (B-67's tolerance (ii)). B-67's row is
  narrowed: task reach no longer applies to harvesting.
- Gas is gathered from the extractor's edge, as today, now by contact with its outline.
- Sources: S9 (no worker range; the MULE's 0.5 shows ranges are edge to edge), S10's timings, and §6.2, where
  contact reproduces S10 and S11 within 2 % once `ReturnDelay` is allowed for. Without D-7 the obstacles alone
  leave income at 82.5 a minute.

### D-8. A mined-out field leaves the map

- In `step_gather`, when a take empties a node whose kind has `remove_when_empty`, the node is removed in the same
  phase: `GameMap::remove_resource_footprint(node)` restores its cells, sets `placed` to `None`, and the world
  calls `pathfinder.recompute_clearance` (the mesh and clearance rebuild every structure change triggers).
- The node stays in `resources()` with `remaining` 0, so identifiers hold. Admission of `Harvest` to it is
  refused when the player sees it (today's rule). A worker sent to it in fog walks to its anchor and finds it
  gone (today's rule).
- A geyser is never removed (S8).
- **Order.** Tasks run in entity-id order (phase 2b), so the worker that empties a field is the lowest-id one
  gathering from it on that tick; workers whose legs end later re-target with `next_node`.
- **Cost.** One full mesh and clearance rebuild per emptied field, at most 16 per shipped map per match. §6.5
  measures it.

### D-9. Nodes are public; so is their removal

Nodes are created visible (S1) and are map data every player has. Every route request treats a standing node as
closed and a removed one as open, whatever the player has seen. A player can therefore route through a field that
emptied in fog before seeing it gone. The game's rule for pathing round a fogged, removed field is not sourced
(**verify**, §2.2 rule 7). Recorded as a minor difference under B-147's successor row (WP0): per-player knowledge
of node removal would need a known-node mask in every request, which nothing else needs.

### D-10. The shipped maps are re-laid

Prototype measurements decided these (§6.3, §6.4). Each mineral cell is the field's anchor (its west cell); gas
cells are centres. `golden_replay`'s structures and every start footprint are clear of every node and of every
ring.

| Map | Slot | Minerals (anchor cells) | Gas | Why |
|---|---|---|---|---|
| `showcase_canyon` | 0 | Front row `y = 14`: `x` 3, 6, 9, 12; back row `y = 15`: `x` 4, 7, 10; corner field (13, 13) | (1, 1), (10, 1) (Amendment 2; was (1, 10), (10, 1)) | A ladder-like two-row line: four close fields 3 cells from the hall, three far fields mined through 1-cell notches, a corner field that the 8 × 7 ring's cut corner admits. Geysers behind the base (F-3) |
| `showcase_canyon` | 1 | The point mirror: anchor `(62 − x, 63 − y)` | (62, 62), (53, 62) (Amendment 2; was (62, 53), (53, 62)) | The map's point symmetry (a 2×1's mirrored anchor is one cell west of the mirrored cell) |
| `default_arena` | 0 | Front column `x = 4`: `y` 3, 5, 7, 9; back column `x = 2`: `y` 2, 4, 6, 8 | (4, 11) unchanged, (11, 11) → (13, 12) | Clear of `golden_replay`'s hall at (6, 6) (T8) |
| `default_arena` | 1 | Front row `y = 60`: `x` 39, 42, 45, 48; back row `y = 61`: `x` 40, 43, 46, 49 | (50, 56) unchanged, (55, 56) → (58, 59) | Clear of the hall at (52, 52); north of the start, 3 cells from its face |

Mineral amounts (1,500) and gas amounts (2,500) are unchanged (§13).

### D-11. Hashed state

- New: `ResourceNode.placed` (the footprint's cells and outline) and `MapCell.occupying_resource`.
  `CHECKSUM_SCHEMA_VERSION` 31 → 32: "Schema 32 adds each resource node's placed footprint and each cell's
  resource occupant (plan `2026-11-20-resource-footprints`)". `FootprintShape` becoming an enum changes the
  encoding of `PlacedStructure.shape`; it is encoded as before for `Box` (its three values) with a leading
  variant tag, also under schema 32.
- The clearance field and `nav_version` are hashed already; they change with the obstacles and with each removal.
- The schema lands in WP5 with the obstacles. Every re-record carries the proof header of §7.

### D-12. Client

- **Positions.** A field's billboard and its pick point move to the footprint centre (from `ResourceNode.pos`; no
  code change in `scene.rs`). Picking stays a disc of `NODE_PICK_RADIUS` 1.0 round it, which covers the 2×1's
  cells to 0.12 of their far corners; WP7 checks it on the shipped maps and records the result.
- **Sprites.** The billboard art is not resized; media is out of scope (charter §1.2) and the art already reads at
  field size. WP7's picture check records how it reads over a 2×1.
- **Editor.** The mineral and gas tools place a node by its anchor; symmetric placement mirrors the footprint, not
  the anchor (a 2×1 mirrored across a vertical axis moves its anchor one cell); the document is checked with
  `validate_resource_footprints` and a refused placement is reported, as other refused edits are.
- The placement ghost (`placement_cell`, `nearest_gas_cell`) is unchanged: gas anchors are still centres.

### D-13. Measurement

`<base>` is the tree before a package and `<cand>` its tree. Release tools are built per tree into
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` and copied aside; output goes to
`~/scratch/resource-footprints/`. The tools read the assets of the tree they were built in, or `--assets`.

| What | Command |
|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix` |
| Battle 31 | `matchup_trace --battle 31_15_marauders_vs_15_stalkers --seeds 100 --no-unit-csv --out <dir>/b31` |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33` |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 6000 --out <dir>/route-default`, and seeds 1 to 8 with `--seed`; winners, end ticks, reversals, stall give-ups, deeper unit-ticks |
| Placebos | The three one-raw-unit patches of plan `2026-11-19-crowd-edge-steering-2` (`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`) rebuilt on `<base>`: the showcase's nine seeds, and battle 31 if a survivor range moves (crowd-slide D-2, D-3) |
| Income | A scratch example (`~/scratch/resource-footprints/proto/harvest_measure.rs`, not committed): per mineral field within 12 cells of a start, a fresh world with the hall and `k` workers (1 and 2) ordered to it; income over ticks 750 to 3,750, per worker per minute; both showcase starts and `default_arena` slot 1 |
| Goldens | `golden_replay_tests` native debug and release, `--target wasm32-wasip1 --release` and debug with `CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"` |
| Entity-only digest | A scratch switch (not committed) that ends the checksum after the `route-groups` section (entities, projectiles, players, effects, route groups); both goldens replayed on `<base>` and `<cand>`, lists compared per checkpoint |
| `sim_map` pins | `cargo test -p sim_map`: `EXPECTED_CHECKSUM`, `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`, `contour_route_checksum`, `SHOWCASE_MESH_DIGEST`, each native and `wasm32-wasip1` |
| Mesh rebuild | `perf stat -e instructions:u` of a scratch example that rebuilds the showcase's and the arena's mesh 41 times with and without placed nodes; and `navmesh_measure` gains a "showcase with its 20 nodes" row in WP4 |
| Throughput | `flock ~/scratch/gate.lock perf stat -e instructions:u` of the `throughput` bench binary, `<base>` and `<cand>` alternated, two rounds; every scenario ≥ 25 ticks per second (instructions retired decide on a loaded host, funnel-fallbacks Amendment 1) |

---

## 5. Determinism and checksum

- **Integer geometry throughout.** Shapes are authored as `f64` multiples of 1/4 and converted once at load,
  exactly (C7: reads authored data). Outlines, containment, rasters and distances are exact integer arithmetic on
  the lattice and the Q16 grid, as for structures. No float, no hashed collection; the table is a `Vec` by kind.
- **Order.** Nodes are placed in id order; the mesh is a pure function of the cells, the structures and the
  standing nodes, independent of insertion order (navmesh invariant, extended and tested in WP4). Removal restores
  the terrain mesh exactly (tested).
- **Hashed state:** D-11. Schema 32.
- **Re-records:** both goldens in WP3 (node positions; proof by the entity-only digest) and in WP5 (schema 32 and
  the obstacles; the same proof); `VALIDATION_GOLDEN` and `DECODE_GOLDEN` in WP3 (the corpus is drawn from
  `default_arena`). Each on native debug and release and `wasm32-wasip1` release and debug. CI covers Windows and
  macOS ARM64 on the push.
- **wasm.** No new dependency; the simulation crates still build for `wasm32-unknown-unknown`.

---

## 6. Prototype and measurements

Prototype: a scratch worktree of `fb66d85e` (`.claude/worktrees/resource-footprints-proto`,
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/resource-footprints-proto`, removed at hand-back). The diff is
kept at `~/scratch/resource-footprints/proto/proto.diff`. It:

- placed every node in `SimWorld::with_catalogs` as a pseudo-structure (ids from `0x7000_0000`) through
  `place_structure`: minerals 2×1 on their anchor and the cell east, full box; geysers 3×3 centred, as the hull
  `(1.25, 1.25, 0.5)` (switch `PROTO_GEYSER=square` for ±1); and moved each field's `pos` to its centre;
- closed those triangles to every request (`open`);
- gathered minerals from the 2×1 (`gather_site`); with `PROTO_REACH=contact`, arrived on contact (gap to the
  cells' rectangle ≤ collision radius) and walked to a goal 0.34375 from the rectangle;
- removed a field and rebuilt the mesh when a take emptied it (`PROTO_REMOVE=off` to keep it);
- `PROTO_RES=off` turned all of it off (the control); `PROTO_NOGAS` and `PROTO_NOMIN` placed one kind only;
  `PROTO_ENTITY_ONLY` ended the checksum after the route groups.

It did not implement D-2's outline, D-6's extractor placement, the occupancy flag or the client. The mesh, the
clearance raster, the centre rule, placement, contact and spawn rings all saw the nodes through the structure
machinery, so its worlds are D-5's worlds up to the geyser's exact outline.

### 6.1 Worlds without resources

| Run (base `fb66d85e` against the prototype, layout of D-10) | Result |
|---|---|
| Matrix, every battle, seeds of record and 16 | Byte-identical (`matrix.csv`, `matrix.md`, logs) |
| Battle 31, 100 seeds | Byte-identical |
| Battle 33, 64 seeds | Byte-identical |
| Throughput bench, instructions retired (two rounds each) | 200.585 G base, 201.740 G prototype (+0.58 %, all in the `showcase_canyon` row, which marches 400 units past the new obstacles); every scenario above 25 ticks per second on a host at load 100 |

The matrix's worlds are generated arenas with no nodes, so nothing could move them; they are the control that the
code paths add nothing where no node stands.

### 6.2 Income

Mineral income per worker per minute, `k` workers on each field in turn (§4 D-13). The reference is S10 and S11.

| Tree | Map, start | `k` = 1: mean (fields) | `k` = 2 |
|---|---|---|---|
| Base | `showcase_canyon` 0 and 1 | 91.9 (87.5 to 92.5) | 91.9 |
| Base | `default_arena` 1 | 74.7 | — |
| Prototype, obstacles, old reach (`TASK_REACH`) | `showcase_canyon` 0, D-10's minerals | 82.5 (65.0 to 92.5) | 82.7 |
| Prototype, obstacles, contact (D-7) | `showcase_canyon` 0 and 1 (D-10) | **62.8** (60.0 to 67.5) | **62.8** |
| Prototype, obstacles, contact | `default_arena` 1 (D-10) | 60.3 (50.0 to 65.0) | — |
| Prototype, control (`PROTO_RES=off`) | `showcase_canyon` 0 (D-10) | 85.6 | 85.6 |

Per field on a showcase start (contact): close fields 62.5 to 65.0, far fields 60.0, the corner field 67.5.
`default_arena` slot 0's line is 13 cells from its start, beyond the measure's 12; that is the authored arena.

**The income acceptance band (derived and sourced).**
1. **Reference trip.** A worker's trip is `7,500 / rate` ticks (5 minerals per trip, 1,500 ticks per minute). S11
   gives 122.5 ticks near and 140.0 far; S10's base (925 per minute with 16 workers) gives 57.8 per worker, a
   129.8-tick trip.
2. **What we do not model.** `ReturnDelay` (8.93 ticks per trip, §2.4) is a recorded difference (WP0). Our trip is
   shorter by it, so our expected rates are `7,500 / (T − 8.93)`: **66.0 near, 57.2 far, 62.1 base mean**.
3. **Tolerance ±5 %.** S10 states one worker's rate as a range, 55 to 60 (±4.3 % round its middle), and the base
   as "about 925"; the measure's window counts whole deliveries (one delivery is 2.5 a minute, 4 % of 62).
4. **The band.** On each showcase start (the skirmish map's ladder-like bases), with `k` = 1 and with `k` = 2:
   the mean over its eight fields lies in **[59.0, 65.2]**, and every field in **[54.3, 69.3]** (far × 0.95 to
   near × 1.05).
5. **Prototype:** means 62.8 and 62.8, fields 60.0 to 67.5. Inside the band. `k` = 3 is B-61's (the game's 143 per
   field needs occupancy) and is not banded.

The band is in force from WP6, when contact lands. It is re-derived if `ReturnDelay` is ever modelled (the
reference itself, 61.22 / 53.57 / 57.8, becomes the band's centre).

### 6.3 The showcase

Winner at end tick, reversals per owner (p0/p1), stall give-ups (g0/g1), default seed then seeds 1 to 8.

| Variant | Default | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | Reversals | Give-ups |
|---|---|---|---|---|---|---|---|---|---|---:|---:|
| Base | 1@1935 8/9 | 1@2141 9/18 | 1@2170 8/14 | 1@2542 8/9 | 1@1939 12/11 | 1@1939 8/7 | 0@3437 9/7 | 1@2175 12/10 | 1@1937 8/7 | 174 | 0 |
| Control: maps of D-10, nodes off | identical to base on every seed (winners, ticks, reversals) | | | | | | | | | 174 | 0 |
| Nodes, geysers where they are today | 1@2325 113/184 g3/0 | **0**@2159 | **0**@2159 | 1@2245 | 1@2451 | 1@2400 | **1**@2280 | **0**@2663 | 1@2271 | 7,189 | 31 |
| Minerals only (geysers spread, not placed) | 1@1970 8/14 | 1@2182 9/24 | 1@2182 8/16 | 1@2579 8/13 | 1@2013 12/20 | 1@1985 8/12 | 0@3437 9/7 | 1@2179 12/10 | 1@1963 8/7 | 205 | 0 |
| Geysers only, spread to a 3-cell gap at the exit | **0**@1207 | **0**@1196 | **0**@1207 | **0**@1191 | **0**@1201 | **0**@1192 | 0@1194 | **0**@1198 | **0**@1205 | 1,027 | 4 |
| Both, geysers spread at the exit | **0**@1280 30/317 | **0** | **0** | **0** | **0** | **0** | 0 | **0** | **0** | 1,377 | 5 |
| **D-10 (geysers behind the base)** | **1@1971 8/12** | 1@2209 9/26 | 1@2207 8/16 | 1@2579 8/13 | 1@2013 12/20 | 1@1985 8/12 | 0@3437 9/7 | 1@2190 12/10 | 1@1963 8/7 | **205** | **0** |
| Layout A (back field at the line's east end), today's geysers | 0@1555 | 0@1560 | 1@1509 | 0@1538 | 1@1634 | 1@1649 | 0@1380 | 1@1507 | 1@1632 | 1,107 | 9 |
| Geyser as the ±1 square, today's geysers | 1@2965 | 1@2241 | 1@1952 | 0@2959 | 0@2314 | 0@2964 | 1@2961 | 1@2966 | 0@2971 | 3,180 | 25 |

Bold winners differ from the base. Every variant had 0 deeper unit-ticks.

- **The control** (D-10's maps, nodes not placed) is identical to the base, so the node moves alone change no
  play; only the obstacles do.
- **D-10** keeps all nine winners, delays the default seed's end by 36 ticks (seeds 1 to 8: +0 to +74), has no
  give-up, and raises reversals by 31 over nine seeds (+18 %), at most +9 per owner on one seed (seed 4, player 1:
  11 → 20). Player 0's reversals are unchanged on every seed. The added reversals are player 1's attackers
  rounding the mineral lines in player 0's base; they lie above the placebo band and are traced as rule-caused in
  §6.6.
- With the geysers on the exit (F-3) the battle is decided by the funnel; any layout that leaves them there flips
  seeds. Minerals alone never flip one.

### 6.4 Goldens and `sim_map` pins

| Run | Result |
|---|---|
| Both goldens, D-10's maps, nodes off (`PROTO_RES=off`) | Differ from checkpoint 0 (node positions are hashed) |
| Both goldens, D-10's maps, nodes on | Differ from checkpoint 0 (positions, clearance, `nav_version`) |
| Entity-only digest: base (original maps, nodes off) against the prototype (D-10, nodes on) | **Identical on every checkpoint**: `golden_replay` 20 of 20 (1,000 ticks), `golden_replay_orders` 60 of 60 (600 ticks). No unit's behaviour moves |
| The prototype build with `PROTO_RES=off` on the original maps | Reproduces both recorded goldens (4 of 4 tests pass): the switch is a clean control |
| `golden_replay` with the first arena layout (fields under its halls) | `InvalidBuildingPlacement { x: 6, y: 6 }`: the fixture's halls stood on the old mineral lines (T8); D-10 moves the lines and two geysers |
| `cargo test -p sim_map --release` on the prototype | All pass except `VALIDATION_GOLDEN` and `DECODE_GOLDEN` (both drawn from `default_arena`), as in footprint-flags Amendment 1. `SHOWCASE_MESH_DIGEST`, `EXPECTED_CHECKSUM`, `PATH_GOLDEN`, `CONTOUR_PATH_GOLDEN`, lanes: unchanged (they place no nodes) |

### 6.5 Mesh rebuild

A full rebuild with the 20 nodes placed, 42 rebuilds per run, instructions retired per rebuild: showcase 17.8 M
without nodes, 46.6 M with them (×2.6); arena 13.9 M and 40.1 M (×2.9). Wall clock on the loaded host: 2.2 to
6.2 ms without, 14.0 to 21.0 ms with (showcase). A depletion costs one such rebuild, and every structure change
now pays for the nodes too. The contour plan's 24-structure row was 11 ms. This is R-4.

### 6.6 Placebo band (showcase)

The three one-raw-unit placebos of plan `2026-11-19-crowd-edge-steering-2` rebuilt on `fb66d85e` (a first build
by mistake on `e58f1af1` is kept apart in `placebo-e58f1af1/` and not used):

| Build | Default | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | Reversals | Give-ups |
|---|---|---|---|---|---|---|---|---|---|---:|---:|
| Base | 1@1935 8/9 | 1@2141 9/18 | 1@2170 8/14 | 1@2542 8/9 | 1@1939 12/11 | 1@1939 8/7 | 0@3437 9/7 | 1@2175 12/10 | 1@1937 8/7 | 174 | 0 |
| Placebo 1 (`PUSH_RELAXATION` + 1 raw) | as base | as base | 1@2165 8/15 | as base | as base | as base | as base | as base | as base | 175 | 0 |
| Placebo 2 (`FIXED_BODY_TANGENT_WEIGHT` + 1 raw) | as base | | | | | | | | | 174 | 0 |
| Placebo 3 (`STEERING_LOOKAHEAD` + 1 raw) | as base | | | | | | | | | 174 | 0 |
| D-10 prototype | 1@1971 8/12 | 1@2209 9/26 | 1@2207 8/16 | 1@2579 8/13 | 1@2013 12/20 | 1@1985 8/12 | 0@3437 9/7 | 1@2190 12/10 | 1@1963 8/7 | 205 | 0 |

**The showcase's placebo band on `fb66d85e` is [174, 175] reversals over nine seeds**, and no placebo changes a
winner. The prototype's 205 lies above it, so §7 rule 4's trace clause applies, and the prototype meets it:

- 30 of the 31 added reversals lie within 1.5 of a mineral line (cells `y` 13 to 15 by the slot-0 hall, `y` 48 to
  50 by slot 1's); all 30 are player 1's attackers, proposal-caused, stepping round the line's ends and notches in
  player 0's base. Player 0's reversals are unchanged on every seed.
- No unit reverses on more than 3 consecutive ticks (the base's own longest run is 3), so the repeated
  oscillation of F-3 does not occur with D-10's layout.

So the rise is rule-caused: units rounding fields they used to walk through.

### 6.7 Tests on the prototype

`cargo test -p sim_core --release` with contact: 1,002 passed, 8 failed. Each failure is one this plan predicts:

| Test | Why | WP |
|---|---|---|
| `golden_replay_tests` (2) | Hashed state (§6.4) | WP3, WP5 |
| `harvest_income_tests::gas_needs_a_completed_extractor_of_ones_own` | The prototype has no extractor-over-geyser placement (D-6) | WP5 |
| `full_match_tests::stim_is_locked_until_researched_through_the_whole_tech_path` | The same; its site (14, 5) and gas cell (15, 6) also move with D-10 | WP3, WP5 |
| `footprint_flag_tests::the_geyser_reach_is_the_void_ring` | Its 1×1 test row is refused on the geyser's own cells (`OccupiedBy…`) before the ring is consulted: the expected verdict there becomes `CoversResource` | WP5 |
| `player_view_tests::preview_judges_public_terrain_and_nodes_at_any_range`, `the_view_and_admission_agree` | Extractor placement (D-6) and a site search that now meets the 2×1 fields | WP5 |
| `scenario_placement_properties::placement_keeps_the_oracle_and_fills_outer_rings_validly` | Its generated maps put nodes where a hall is then spawned; the generator must keep starts clear of footprints (D-4's validation) | WP5 |

Client tests were not run. `battle_card_path_tests`, `battle_context_order_tests`, `battle_fog_tests` and
`battle_hud_tests` order harvests or name node cells; WP7 re-derives them.

---

## 7. Stop rules and acceptance

As in the contour, flags and crowd plans: **a winner flip anywhere stops the package.** The agent commits nothing
beyond it and reports the trace: the first divergent tick, the unit, the node, and which consumer (M1 to P5)
caused it. A movement that is not a flip is explained before the package proceeds.

1. **WP1 (outline geometry) and WP2 (data) are bit-identical everywhere:** matrix, battles 31 and 33, the
   showcase's nine `units.csv`, both goldens on all four builds, every `sim_map` pin. A difference is a defect;
   never re-record. WP2 re-derives only the footprint-flags tests of the mineral reach (45 → 52 cells, F-2's
   corner case), each from the new rows.
2. **WP3 (maps) moves hashed state only.**
   - Both goldens are re-recorded once. The header names this plan and WP3 and holds the proof: the entity-only
     digest equals the base's on every checkpoint (prototype: 20 of 20, 60 of 60).
   - `VALIDATION_GOLDEN` and `DECODE_GOLDEN` are re-recorded with a header recording that a scratch build reading
     the old `default_arena` reproduces the old values, and that native debug and release and `wasm32-wasip1`
     release and debug agree. Every required error variant is still raised.
   - The showcase's nine `units.csv` are byte-identical (prototype control); matrix and battles 31 and 33 too.
   - Tests that name a moved node cell are fixed in the WP3 commit with their trace (footprint-flags Amendment 1's
     reading of rule 4); any other failure stops the package.
3. **WP4 (`sim_map` occupancy and mesh) changes no `sim_map` pin** (no pin places a node) and no world (nothing
   in `sim_core` places a node yet). New pins are added with their provenance.
4. **WP5 (obstacles in the world, schema 32) moves the expected worlds and nothing else.**
   - **Matrix, battles 31 and 33:** byte-identical (§6.1). A difference is a defect.
   - **Goldens:** re-recorded once, with the header "Re-recorded for plan 2026-11-20-resource-footprints WP5
     (B-147): schema 32 hashes each node's footprint and each cell's resource occupant, and the nodes' outlines
     change the hashed clearance and `nav_version`. Entity state is unchanged on every checkpoint." The
     entity-only digest must hold on every checkpoint (prototype: it does). If it does not, the first differing
     checkpoint is traced to a unit and a consumer, and the package stops for an amendment.
   - **Showcase**, base WP4's own run:
     - the same winner on all nine seeds (prototype: all nine);
     - stall give-ups at most base + 1 per seed (prototype: 0);
     - 0 deeper unit-ticks;
     - reversals: the nine-seed total within the placebo band of §6.6, re-measured on the package's base
       (crowd-slide D-3); if above it (the prototype is: 205 against [174, 175]), the package proceeds only if
       at least 90 % of the added reversals lie within 1.5 of a node's footprint (rule-caused; prototype 30 of
       31), the rise over the base is at most the prototype's 31 plus a tenth of the base total (48 on
       `fb66d85e`; above it, an amendment decides), and no unit reverses on more than 3
       consecutive ticks (the base's longest run; a longer run is F-3's oscillation, a defect);
     - survivors: the showcase records only winners; battle 31's survivor ranges are byte-identical, so the
       placebo-aware survivor rule (crowd-slide D-2) cannot be reached unless the matrix moves, which is a stop.
   - **Control runs** (scratch builds): (a) geysers not placed: the showcase is unchanged against the full
     package on all nine seeds (prototype: identical, because D-10's geysers are off the battle's path); (b) the
     geyser as the hull `(1.25, 1.25, 0.5)` instead of the outline: reported, with any difference traced to a
     unit passing a recessed side.
   - **If the default-seed winner flips:** stop and trace from tick 0. Classify the cause as **rule** (a unit
     rounds a node's 5.0.14 footprint it used to walk through) or **map** (a node on an army's path because of
     D-10's layout) or **defect** (a stall, an oscillation or a wedge at a node). The orchestrator decides by
     amendment. No constant is tuned and no layout is changed without one.
   - **If the timeline moves at all** (prototype: +36 ticks on the default seed), WP7 re-checks the storyboard.
5. **WP6 (contact harvest and removal) moves economy worlds only.**
   - Matrix, battles 31 and 33, the showcase and both goldens are byte-identical to WP5 (none harvests, T7).
   - **Income** lies in §6.2's band on both showcase starts, `k` = 1 and `k` = 2. A field outside its band is
     traced (its route and its arrival points) before anything is changed; a band miss stops the package.
   - Every changed harvest test is re-derived from a trace; no bound is loosened.
6. **Test edits.** A changed test is re-derived from a trace and stated in the commit. A failure §6.7 does not list
   is a finding and stops the package.
7. **No tuning constant** is introduced. Every cell, vertex and flag comes from §2 with its line; every distance
   is a unit's own radius.

---

## 8. Client consequences

D-12. Tests:
- `crates/client/src/battle_input` `a_mineral_field_is_picked_anywhere_on_its_footprint`: the pick at each of a
  2×1's two cell centres names the node.
- `crates/client/src/editor/tools.rs` `mirrored_minerals_mirror_the_footprint`: under each symmetry, the mirrored
  field's cells are the image of the original's cells; `an_overlapping_field_is_refused`.
- `battle_card_path_tests`, `battle_context_order_tests`, `battle_fog_tests`, `battle_hud_tests`: re-derived for
  the moved nodes and the contact harvest, each from a trace.
- GPU targets that read the shipped maps: `runtime_battle_tests`, `editor_app_tests`, `terrain_tests`,
  `mesher_golden_tests` (footprint-flags Amendment 1), each run under its own lock.
- The billboard is not re-rendered by code change; WP7's picture check records the 2×1 fields and the moved
  geysers on the RX 580.

---

## 9. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `docs/plans/README.md` (row), `DESIGN.md` §4 (B-147 restated, B-67 narrowed, new rows), this plan | WP0 |
| Outline geometry | `crates/sim_types/src/footprint.rs` (or `enums.rs`: `FootprintShape`, `QuarterOutline`), `crates/sim_map/src/footprint.rs` (`PlacedFootprint`, general predicates), `navmesh/mod.rs` (contour and clipping from outlines), `pathfinding.rs` (raster), `crates/sim_core/src/checksum/encoding.rs` only if the enum needs a type change there (encoding unchanged until WP5), the 45 sites that construct a `FootprintShape` (`grep -rn "FootprintShape::full\|FootprintShape {" crates`) | WP1 |
| Data and loader | `assets/catalog/placement_rules.ron`, `crates/sim_core/src/catalog/placement_rules.rs`, `catalog/validation.rs`, `crates/sim_core/tests/runtime_catalog_tests.rs`, `catalog_reference_values_tests.rs`, `footprint_flag_tests.rs` (mineral reach) | WP2 |
| Maps | `assets/maps/showcase_canyon.scmap.ron`, `assets/maps/default_arena.scmap.ron`, `crates/sim_core/tests/fixtures/golden_replay*.ron`, `crates/sim_core/tests/scenario_tests.rs` (guards), `crates/sim_map/tests/validation_golden_tests.rs` (two pins), `full_match_tests.rs` and client tests that name moved cells | WP3 |
| `sim_map` occupancy | `crates/sim_map/src/grid.rs` (`ResourceNode.placed`, `OCCUPIED_BY_RESOURCE`, `occupying_resource`, `place_resource_footprints`, `remove_resource_footprint`, `place_structure_over_resource`, `OccupiedByResource`), `error.rs`, `map_format.rs` (doc, runtime-state check), `validation.rs` (`validate_resource_footprints`), `navmesh/mod.rs` (`TriangleKind::Resource`, labels, constraints), `pathfinding.rs`, `footprint.rs` (`footprint_occupants_touched`), `traversal.rs`, `examples/navmesh_measure.rs`, new `crates/sim_map/tests/resource_footprint_tests.rs` | WP4 |
| `sim_core` wiring | `crates/sim_core/src/world.rs` (`with_catalogs` fallible), its 40 call sites, `error.rs`, `movement/flee.rs`, `movement/geometry.rs`, `scenario.rs`, `scenario/placement.rs`, `production/construction.rs`, `checksum.rs`, `checksum/encoding.rs`, `crates/sim_fixtures/src/lib.rs`, new `crates/sim_core/tests/resource_footprint_tests.rs` | WP5 |
| Harvest | `crates/sim_core/src/economy/mod.rs` (contact arrival and goal), `economy/harvest.rs` (removal), `harvest_income_tests.rs`, new `crates/sim_core/tests/harvest_edge_tests.rs` | WP6 |
| Client and evidence | `crates/client/src/battle_input/*` (picking test), `crates/client/src/editor/tools.rs`, `editor/document.rs`, client tests, `assets/captures/showcase.ron` (if the storyboard needs it), `docs/evidence/2026-11-20-resource-footprints.md` | WP7 |
| Bookkeeping | `DESIGN.md` (Economy and Movement rows, register, §5 if figures move), this plan | WP8 |

---

## 10. Work packages (commit order)

- **Branch:** `resource-footprints`.
- **Tests first:** each package writes its tests first and shows them red on its base (WP0 and *guard* tests
  excepted).
- **Gates:** implementation agents hand back after `cargo gate quick` and the tests their change needs; a verifier
  runs `cargo gate package` once per package on its final tree (Q0061). WP1 and WP4 form the `sim_map` packages;
  WP2, WP3, WP5 and WP6 the `sim_core` ones. New test files are `fast` unless measured `slow`, in which case
  `crates/gate/suites.ron` gains a rule (C22).
- **Coordination.** The pathfinding branch `partial-knowledge-funnel-plan` may move `sim_map` pins. Whichever
  merges second re-measures its pins on the merged tree and re-records only with its own proof.
- **Sizes:** S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row: `Proposed` → `Active`.
- `DESIGN.md` §4:
  - **B-147** restated: "…fixed by authoring each resource kind's 5.0.14 footprint in `placement_rules.ron` and
    cutting it into the navigation mesh; workers harvest by contact; a mined-out field is removed (plan
    `2026-11-20-resource-footprints`)". Its **verify** is resolved: `RemoveWhenEmpty` (liberty
    BehaviorData:749). Open until WP8.
  - **B-67** narrowed: task reach no longer governs harvesting (D-7, F-1).
  - New rows, taking the next free numbers when WP0 lands (B-150 onward at the time of writing: B-149 was taken by
    plan `2026-11-19-partial-knowledge-funnel`, merged as `e58f1af1` while this plan was measured; if another plan
    takes numbers first, WP0 takes the next free ones and adds a pointer line here):
    1. "No `ReturnDelay`: a worker leaves a mineral field as soon as it has gathered; the game waits 0.5 game
       seconds (0.357 s, 8.93 ticks; liberty BehaviorData:750). Income is about 7 % high (plan
       `2026-11-20-resource-footprints` §6.2)."
    2. "Gas harvest time and amount are the worker's (`harvest_rate_ticks` 50, `harvest_capacity` 5); in the game
       they are the resource's: `HarvestTime 1.981` (35.4 ticks) and `HarvestAmount 4` (liberty
       BehaviorData:817–828). Gas income per trip is 25 % high and each trip 41 % long."
    3. "A unit beside rounded obstacles at a narrow gap can reverse its step on up to 12 consecutive ticks, hundreds of
       times (prototype of plan `2026-11-20-resource-footprints`, F-3: geysers spread at the showcase's exit, seed 6,
       unit 4, ticks 714 to 1,356, 345 reversals)." Out of this plan's scope; it is why D-10 keeps geysers off
       the armies' path.
    4. "A route treats a node removed in fog as gone for every player (D-9); the game's rule is not sourced."
- **Gate:** docs-only `package`.

### WP1: outline geometry (M, `sim_types`, `sim_map`, pure)

- **Tests first** (`sim_map/src/footprint.rs` units unless noted):
  - `a_box_outline_is_its_contour`: for every §2.3 contour shape and odd, even and non-square sizes, the outline
    of `Box` is today's `contour()`;
  - `general_predicates_agree_with_the_box_closed_forms` (property, 256 cases): seeded boxes, points, segments
    and capsules on the Q16 grid; `contains_open_scaled`, `meets_open_half_cell`, `segment_meets`,
    `capsule_meets` equal the box closed forms (kept as the test oracle);
  - `the_geyser_outline_is_contained_exactly`: the 16-gon's recessed side (the centre of the side's middle cell,
    0.25 outside the recess) is outside, the lobe tips are on the boundary, the centre is inside;
  - `an_outline_must_be_simple_axis_or_diagonal_and_on_the_lattice` (`MapError::InvalidFootprintShape`);
  - `navmesh_tests::an_outline_shape_cuts_the_mesh` (a structure with the 16-gon placed in a test: build never
    errors, labels by containment, no crossings);
  - `navmesh_tests::box_shapes_cut_the_mesh_as_before` (*guard*): `SHOWCASE_MESH_DIGEST` and the contour
    corpus pins unchanged.
- **Change:** D-2. `FootprintShape` callers build `Box`.
- **Exit:** §7 rule 1.

### WP2: footprint data (S, catalog)

- **Tests first:**
  - loader: `a_resource_row_needs_own_cells`, `own_cells_must_be_one_rectangle_with_the_anchor`,
    `a_resource_shape_must_lie_in_its_cells`, `an_outline_edge_must_be_axis_or_diagonal`, `both_kinds_need_a_row`;
  - reference (`catalog_reference_values_tests.rs`): `the_shipped_resource_footprints_are_5_0_14`: mineral cells
    `(0, 0, 2, 1)` from the anchor, box ±1 × ±0.5, ring 52 cells, removed when empty; gas cells `(−1, −1, 3, 3)`,
    the 16 vertices of §2.2 rule 3, ring 69 cells, never removed;
  - `runtime_catalog_tests`: a changed footprint in a temp copy of `assets/catalog` changes the loaded table (C6);
  - `footprint_flag_tests` F-2 re-derived: the mineral reach is the game's 8 × 7 on the 2×1; a hall whose face is
    3 cells east of the field's east cell is admitted, 2 cells refused (red on the base, where the reach is
    anchored on one cell).
- **Change:** D-1 and the RON. Nothing places a footprint yet.
- **Exit:** §7 rule 1 (worlds bit-identical; the one re-derived test family).

### WP3: maps (S, data)

- **Tests first** (*guards*, `scenario_tests.rs`, read from the shipped catalog's table):
  - `every_shipped_resource_footprint_fits`: `validate_resource_footprints` reports nothing on both maps (red on
    the base: the showcase's fields overlap, the arena's middle field is enclosed);
  - `every_shipped_start_clears_every_resource_ring` (the footprint-flags guard on the new rings);
  - `no_golden_structure_stands_on_a_node` (reads both fixtures);
  - `every_shipped_gas_node_takes_an_extractor` (kept).
- **Change:** D-10's cells, with each map's comment naming this plan. The two golden re-records and the two
  `sim_map` digest re-records under §7 rule 2. `full_match_tests` and any test that names a moved cell, each with
  its trace.
- **Exit:** §7 rule 2.

### WP4: occupancy, placement and the mesh in `sim_map` (M)

- **Tests first** (new `crates/sim_map/tests/resource_footprint_tests.rs`, with a test table: a 2×1 box kind and
  the 16-gon kind, no catalog):
  - `placing_marks_the_cells_and_removal_restores_them` (flags, occupants, prior flags);
  - `placement_refuses_overlap_bounds_terrain_and_a_second_placement` (each typed error);
  - `a_node_is_closed_to_every_request` (with and without vision, with a known-structure mask);
  - `routes_round_a_mineral_line_and_through_its_notches`: a worker-sized disc reaches a back field's notch; a
    disc wider than the notch does not;
  - `the_geyser_outline_admits_a_disc_in_its_recess`: a disc of radius 0.3125 centred 0.3125 + 2⁻¹⁶ outside the
    recessed side's middle is placeable; at the hull's line it is still placeable (the hull would refuse it);
  - `an_extractor_over_a_geyser_keeps_the_geyser`: `place_structure_over_resource`, then removal of the
    structure; the geyser's triangles stay `Resource`;
  - `removing_a_node_restores_the_terrain_mesh` (digest equality);
  - `the_mesh_does_not_depend_on_placement_order` (nodes and structures, shuffled);
  - `validate_resource_footprints_reports_each_error`;
  - `navmesh_measure` row "showcase with its 20 nodes", and a new pin `SHOWCASE_RESOURCE_MESH_DIGEST` with
    provenance.
- **Change:** D-4's `sim_map` half, D-5 rows M1 to M4 and P1, D-6's placement call, D-9's closed label.
- **Exit:** §7 rule 3.

### WP5: obstacles in the world (M, `sim_core`; schema 32)

- **Tests first** (new `crates/sim_core/tests/resource_footprint_tests.rs`, shipped catalog, nodes chosen by kind):
  - `a_world_places_its_nodes` and `a_world_with_overlapping_nodes_is_refused` (`SimCoreError::ResourceFootprint`);
  - `nothing_spawns_on_a_node` (scenario rings, hall region, production exits);
  - `a_centre_never_enters_a_field` and `a_blinked_or_unloaded_unit_is_never_placed_on_a_field` (C1, C2);
  - `a_site_on_a_field_says_covers_resource` and `an_extractor_must_cover_exactly_one_geyser` (P2);
  - `checksum_state_tests::a_nodes_footprint_is_hashed`;
  - the §6.7 tests re-derived (each with its trace).
- **Change:** D-4's world half, D-5's `sim_core` rows, D-6's callers, D-11.
- **Measure:** D-13 in full, controls (a) and (b), the placebo band on the package's base.
- **Exit:** §7 rule 4.

### WP6: contact harvest and removal (M, `sim_core`)

- **Tests first** (new `crates/sim_core/tests/harvest_edge_tests.rs`):
  - `a_worker_gathers_touching_the_field`: when gathering starts, the worker's distance to the field's box is at
    most its collision radius and at least its terrain radius;
  - `a_worker_delivers_touching_the_hall`: the same against the hall's outline, at a cut corner too;
  - `a_back_field_is_mined_through_its_notch`;
  - `income_lies_in_the_band` (*reference*): the showcase start of slot 0 with one worker per field over 3,000
    ticks after 750, per-field and mean bounds of §6.2 (red on the base: 91.9);
  - `a_mined_out_field_leaves_the_map`: on the take that empties it the node is unplaced, its cells walkable,
    the mesh rebuilt (`nav_version` changes), a unit can then walk through; `a_geyser_stays_when_empty`;
  - `an_emptied_field_no_longer_applies_its_ring` (P3);
  - `harvest_income_tests` re-derived (its trip timings), each from a trace.
- **Change:** D-7, D-8.
- **Exit:** §7 rule 5.

### WP7: client, presentation, measurement (S)

- §8's tests first.
- **Change:** D-12 (the editor's mirror and validation; picking checked).
- **Picture check** on the RX 580 (`with-gpu-lock`, one capture per hold): a showcase start's mineral line and
  geysers, a worker mining a back field, a field before and after it empties. Recorded as observations; anything
  that reads as a fault is registered under the next free number and left out of scope.
- **Storyboard:** the default-seed timeline moved (+36 ticks in the prototype), so `assets/captures/showcase.ron`
  is re-checked with real renders on radv, one lock per take; `client_diagnostics` `showcase_storyboard_tests`
  and `showcase_tests` must pass.
- **Throughput** (D-13): every scenario ≥ 25 ticks per second; instructions retired reported against the base.
  The mesh rebuild row is reported; if a rebuild with the shipped nodes costs more than 3× the node-free rebuild
  (prototype: 2.6 to 2.9×), the agent profiles it and records the result; an optimisation needs an amendment.
- **Evidence:** `docs/evidence/2026-11-20-resource-footprints.md`: §2's lines, §6's tables re-measured on the
  merged tree, "How to reproduce".

### WP8: bookkeeping (S)

- **Register:** B-147 removed as fixed, with commits; B-67's row as narrowed; the WP0 rows stay open.
- **`DESIGN.md` Economy row:** "…resource nodes have their 5.0.14 footprints (a mineral field 2×1, a geyser
  3×3 with its void outline), authored in `placement_rules.ron` and cut into the navigation mesh; workers gather
  and deliver by contact; a mined-out field leaves the map (B-147 fixed, plan `2026-11-20-resource-footprints`)".
  Its tests gain `harvest_edge_tests.rs` and `resource_footprint_tests.rs`.
- **Movement row:** "…and through resource nodes' footprints".
- **§5:** updated only if the throughput figures move beyond noise.
- **Pointer:** a one-line amendment in `2026-11-19-footprint-flags.md` (D-3's difference removed here).
- This plan's row `Done`.
- **Gate:** `merge` by the orchestrator.

---

## 11. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Geometry | Box outlines unchanged; general predicates exact; the 16-gon | WP1 |
| Data | Footprints, shapes and removal read at run time with their sources | WP2 |
| Maps | Every shipped footprint fits; starts clear of rings; goldens' halls clear | WP3 guards |
| Mesh and placement | Occupancy, removal, order independence, notches, the recess, extractors over geysers | WP4 |
| World | Spawning, centre rule, placement, sites, hashing | WP5 |
| Economy | Contact at both ends, the sourced income band, removal and its ring | WP6 |
| Client | Picking, the editor's mirror and refusal | WP7 |
| Determinism | Pure packages bit-identical; WP3 and WP5 re-records proven by the entity-only digest on four builds | §7 |

---

## 12. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The showcase flips when the exact outline replaces the prototype's hull, or on the implementation's own details | A stop in WP5 | D-10 keeps the geysers off the battle's path (control (a): the showcase does not read them); control (b) compares hull and outline; §7 rule 4's classification |
| R-2 | The reversal rise (+31 on nine seeds) lies above the placebo band | A stop in WP5 | §7 rule 4: traced; rule-caused only if units round a node; the oscillation class of F-3 is a defect and is out of scope (WP0 row 3) |
| R-3 | The engine default harvest range is not 0 (S9, **verify**) | Income off by the difference | §6.2 shows contact reproduces the game's figures within 2 % with `ReturnDelay` allowed for; a sourced range would be a data change of D-7 |
| R-4 | Mesh rebuilds cost 2.6 to 2.9× with the shipped nodes (§6.5), once per depletion and per structure change | Tick spikes in skirmish | Measured in WP7 against a 3× bar; an incremental rebuild is out of scope and needs its own plan |
| R-5 | `with_catalogs` becoming fallible touches 40 call sites | A mechanical WP5 diff | `grep` lists them; tests already return `Result` |
| R-6 | The general predicates differ from the box closed forms somewhere | WP1 not bit-identical | The property test against the closed forms; every pin as the guard |
| R-7 | A player routes through a field removed in fog (D-9) | A minor knowledge leak | Recorded (WP0 row 4) |
| R-8 | The parallel `partial-knowledge-funnel-plan` moves `sim_map` pins | Conflicting re-records | §10's coordination rule |
| R-9 | The client's sprites read oddly over 2×1 fields | Presentation | WP7's picture check; media is not faked (charter §1.2) |

---

## 13. Out of scope and decisions recorded (Q0002)

- **B-61, mineral-field occupancy** (one worker at a time; a third worker waits or moves on, S12). Workers still
  walk through each other while mining. `k` = 3 income is not banded.
- **`ReturnDelay`** and **gas harvest time and amount per resource**: registered by WP0 (rows 1 and 2), each its own
  change of economy worlds.
- **The repeated oscillation beside rounded obstacles** (F-3): registered (row 3).
- **Per-player knowledge of node removal** (D-9): registered (row 4).
- **Mineral and gas amounts.** The shipped maps keep 1,500 per field and 2,500 per geyser; ladder bases hold
  1,800 and 900 per field (balancemulti Behavior:251–256, S10) and 2,250 per geyser (balancemulti Behavior:323–326).
  Map balance is the map's, not a unit mechanic; a later map change may adopt them.
- **Rich fields and rich geysers**, **`MineralField750`**-style variants: no such kinds exist here.
- **Lane cuts round nodes** (M3): nodes are not lane obstacles, as structures are not; group lanes stay terrain
  homotopy classes (S-7).
- **An incremental mesh update** on removal (R-4).
- **Node art at 2×1** (D-12, R-9).
- **Rocks and other neutral footprints** (`FootprintRocks…`): no such units exist here.

---

## 14. Prototype material (scratch, not committed)

- Game data: `~/scratch/footprint-flags/dl-footprint/FootprintData-{core,liberty,void}.xml`,
  `~/scratch/combat_wp3_data/{core,liberty,libertymulti,balancemulti}/{UnitData,BehaviorData,AbilData}.xml`
  (5444e12f13), read as data, never executed. Liquipedia wikitext: `Resources`, `Mining Minerals`,
  `MULE (Legacy of the Void)`, fetched through the MediaWiki API on 2026-11-20.
- Scripts in `~/scratch/resource-footprints/scripts/`: `migrate.py` (D-10's mineral cells), `scan_reach.py`
  (overlaps, rings against starts, exposed faces), `view.py` (ASCII map windows), `measure.sh` (matrix, battles
  31 and 33, showcase), `showcase.sh` and `summ.sh` (showcase variants).
- Prototype diff: `~/scratch/resource-footprints/proto/proto.diff` (with `harvest_measure.rs` and
  `rebuild_measure.rs`); maps of D-10 in `~/scratch/resource-footprints/maps/`.
- Results: `base/world`, `proto/world` (matrix, b31, b33, showcase), `proto/sc-*` (showcase variants),
  `placebo/p{1,2,3}`, `proto/harvest/`, `proto/golden/`, `proto/sim_map_tests.log`, `proto/sim_core_tests.log`,
  `proto/rebuild*.txt`, `bench/`, `showcase-variants.txt`.
- The scratch worktrees (`resource-footprints-proto`, `rf-placebo`) and their target directories were removed
  at hand-back; the diff rebuilds the prototype on `fb66d85e`.

## Amendment 1 (orchestrator, 2026-11-20): slot 1's geysers re-laid away from the army's spawn; the reversal clause is reported, not a stop

**Why.** WP5 stopped on §7 rule 4. The proposed amendment, with its controls and trace, is on
`resource-footprints-wp5-wip`. Measured on today's base (the WP4 tree), placing the geysers makes showcase seed 4 a
draw and flips seed 6 to player 0. The controls attribute both changes to slot 1's geysers. Their footprint changes
the mesh beside the ground where the scenario spawns slot 1's army, so the routes out of the base change from
tick 1. Minerals alone keep every winner, and a far node changes nothing. The plan's prototype was measured on
`fb66d85e`, before plan `2026-11-19-partial-knowledge-funnel` moved every seed, so its figures no longer apply.

**Decision: option 1.**

1. **Re-lay the geysers.** Slot 1's geysers, and their mirror in slot 0, are moved so that no geyser footprint or
   its mesh constraint lies between a town hall and the cells where the scenario spawns that slot's army, nor on
   the army's exit. Use the same rule that D-10 states for "behind the base", made concrete:
   - the geyser lies on the far side of the hall from the army's spawn ring and exit;
   - the map stays mirror-symmetric;
   - the D-10 guards hold.

   Record the new cells in D-10 and in Q0091's context; Q0091's default (a) still applies.
2. **Re-measure WP5** against the WP4 base and its placebo band. Rule 4's winner part stands as written: all nine
   showcase winners, the matrix, and battles 31 and 33. A further winner change stops the package again, with
   controls.
3. **The reversal clause becomes a report, not a stop.** The 90 %-near-a-node fraction and the +31 + 13 allowance
   were set from the `fb66d85e` prototype. That base no longer exists, and even minerals alone on today's base rise
   by 50. The stop rules for oscillation stay:
   - give-ups at most base + 1;
   - deeper unit-ticks 0;
   - no unit reversing on more than 3 consecutive ticks;
   - the F-3 oscillation absent.

   The total and its split by distance from a node are reported per seed. B-152 (the oscillation beside rounded
   obstacles) records the measured rise as its baseline. I decide on the figures at hand-back, without loosening
   any other rule.
4. Then WP6 to WP8 as written.

## Amendment 2 (orchestrator, 2026-11-20): the geysers go to the base corners

**Why.** Amendment 1's cells, slot 1 at (59, 62) and its mirror at (4, 1), still change the mesh beside the route
slot 1's army takes round the north of its hall. The geysers alone flip the default seed and make seeds 1, 3 and 8
draws; minerals alone keep every winner (proposed amendment 2 on `resource-footprints-wp5-wip`).

The probe with the geysers in the base corners keeps all nine winners. Its seeds end within a few ticks of the
"geysers not placed" control, so in the corners the geysers stay out of every army's route. That is the property
D-10's "behind the base" meant.

**Decision: option 1, the corner cells.**

- **Slot 1's geysers** go to (53, 62) and (62, 62); **slot 0's** to (10, 1) and (1, 1). The map stays
  point-symmetric.
- **Proof the corners keep geysers out of the routes:** the geysers-not-placed control is part of WP5's
  measurement, and it must stay within a few ticks of the full run on every seed. Record that comparison as the
  evidence.
- **Unchanged:**
  - the full WP5 re-measure against the WP4 base;
  - the D-10 guards;
  - the `SHOWCASE_RESOURCE_MESH_DIGEST` re-pin with its proof;
  - the tests' training site moved off (1, 1)'s 3×3.
- **The winner rule stands.** If it breaches again, land WP5 with minerals only (option 2), and give the geysers
  their own layout study in a later package. Stop and report rather than trying further cells.
- Q0091's context names the final cells.

**As applied (implementation agent, 2026-11-20).** Slot 1's geyser at (62, 53) moves to (62, 62), the base's
north-east corner, beside the geyser at (53, 62); its mirror (1, 10) moves to (1, 1), beside (10, 1). The D-10 guards
pass. The arena is unchanged, so no golden moves; `SHOWCASE_RESOURCE_MESH_DIGEST` (WP4) is re-recorded with its proof.
The tests' training site (2, 2), under (1, 1)'s 3×3, moves to (12, 2).

## As built (implementation agent, 2026-11-20)

| WP | Commit | Notes |
|---|---|---|
| WP0 | `6549d1b3` | B-150 to B-153 registered |
| WP1 | `8fcda359` | Interior cell lines clipped per unit edge by its midpoint; the half-cell test adds the square's centre |
| WP2 | `7280da8c`, `f254816a` | C7 allowlist for the authored shapes |
| WP3 | `8a130efe`, `bec14d5d` | Maps carry no comment (the showcase must round-trip); golden_replay's hall stands in a ring, not on a node |
| WP4 | `fb285e2d` | `ResourceNode.placed: Option<PlacedResource>`; `validate_resource_footprints` takes the ring as a predicate |
| WP5 | `6efc053c` | Schema 32; `sim_fixtures::world` still returns a world (about 300 callers), `try_world` returns the error |
| WP6 | `fb615fdd` | Contact goal one Q16 step past the terrain radius (the route's fit test is strict) |
| WP7 | `1d41731f` | Picture check without harvest captures (no capture path orders harvests) |
| WP8 | this commit | Register and rows |

Figures are in [`docs/evidence/2026-11-20-resource-footprints.md`](../evidence/2026-11-20-resource-footprints.md):
showcase winners hold with the corner geysers; income 60.3 per worker-minute; throughput +2.25 % instructions
retired; the arena's rebuild with its nodes at 3.04× the node-free rebuild, above the 3× bar, profiled and recorded.


## Amendment 3 (orchestrator, 2026-11-20): the rebuild-cost bar missed on the arena is accepted and registered (B-155)

WP7 measured a navigation mesh rebuild at 3.04× with the shipped nodes on `default_arena` (2.74× on the showcase),
against the plan's 3× bar. The profile puts 68.5 % of the time in `ConstraintGraph::kind`, a per-edge lookup that
the nodes' extra constraints multiply. Throughput holds: +2.25 % instructions, every scenario above 25 ticks/s.

The miss is accepted for this package, and the cost is registered as **B-155**, with an indexed edge-kind lookup
as its direction. That is an optimisation with no behaviour change, for its own short plan. The reversal figure
reported under Amendment 1 (175 against 132, 34 within 1.5 of a node) is accepted as rule-caused, with B-152 as
its register entry.

## Amendment 4 (orchestrator, 2026-11-20): the three client failures that are not sites

The package gate's five failing client targets were traced (on `aa6f2a29` and `resource-footprints-wp7-wip`).
Three of them were sites on moved nodes and are fixed. The other three are decided here.

1. **`battle_flipbook_tests::frame_grouping_does_not_change_attack`: a client defect the new battle exposes. It is
   registered as B-156 and fixed in this package, option (b).**
   - An `Attack` record without a swing takes its flipbook lanes from the tick the client observed it start. So a
     unit that walks during its swing and then publishes `Attack` with no swing gets different lane origins at 1-tick
     and 5-tick frame grouping, with identical simulation records (entity 24, ticks 674 to 689).
   - The test states the right property: frame grouping must not change what is drawn. It is not weakened.
   - The client keeps the last swing's lanes for an `Attack` without a swing, or derives the origin from published
     records only, whichever the flipbook plan's D-5 supports. Then the lanes depend on the records, not on which
     ticks were observed.
   - This is a presentation change only. It is recorded with before and after evidence, and the other flipbook tests
     must stay green.
2. **`area_link_gpu_tests`.**
   - The depth-first spot search from `resource-footprints-wp7-wip` is adopted. It is test support and finds a full
     spot set where the greedy search could not.
   - The fungal-growth child is re-staged so that no viewer unit's sight crosses the measured fog edge, with the
     edge and the sight radii stated in the test. Its thresholds are unchanged.
3. **`weapon_battle_gpu_tests::weapon_looks_fire_in_a_staged_battle`: a staging change first.**
   - Wave 6's spots are chosen away from the re-laid nodes and the hall, on open ground like the other waves. The
     bar (twice the blank plus the margin) is unchanged.
   - If needle_volley + needle_hit still misses on open ground, stop. That would be a look too faint for this
     shooter: a media item to register, not a test change.

After these, run every client and `client_diagnostics` target that loads the shipped maps or stages a battle, one
at a time, as asked before. All must pass.

## Amendment 5 (orchestrator, 2026-11-20, short note): B-155, a faster edge-kind lookup in the mesh rebuild

**What.** `ConstraintGraph::kind` (`crates/sim_map/src/navmesh/mod.rs`) takes 68.5 % of a mesh rebuild with the
shipped resource nodes, which makes the arena rebuild 3.04× its cost without the nodes. Replace its per-edge search
(`StructureEdges::contains` and `terrain_kind`) with a lookup that is O(1), or close to it, per unit edge. One option
is a dense per-rebuild table keyed by canonical edge, built once from the structure edges and the terrain cell
lines. The design is the implementer's, with a `PERFORMANCE RATIONALE` block.

**Why.** It is a performance fix with no change in behaviour. The meshes must be bit-identical.

**Tests and acceptance.**

- **Bit-identity:** a property test compares the new lookup with the old one, kept as a test oracle, on random
  layouts. Every `sim_map` pin and both goldens must stay unchanged.
- **Cost:** the mesh rebuild instructions on the arena with nodes fall to at most 2× the no-node figure, the plan's
  original 3× bar with margin. Measure on instructions retired.
- **Memory:** the steady state allocates nothing per rebuild beyond today's.
- B-155 is marked fixed.
