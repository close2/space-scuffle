# Blueprint: Footprint Placement Flags (B-139)

This plan models the placement flag sets of StarCraft II 5.0.14's footprint layers for structure placement:

- **`NearResources`.** Mineral fields and Vespene geysers apply it on a ring around themselves. A town hall's
  footprint refuses it, so a town hall analogue cannot be placed within reach of resources. Today it can (B-139).
- **`DropOff`** is examined and not modelled. A town hall applies it only to its own cells, which occupancy already
  closes to every other placement, and only the resources' own `Check` layers and the built geyser's `Footprint`
  refuse it; no structure's placement footprint does (§2.3, D-5). In a match it can never decide a placement. Its
  map-side meaning, that no resource stands near a town hall, becomes a guard test on the shipped maps (WP4).
- **`Fogged`.** The Spine Crawler and Spore Crawler placement footprints refuse a cell hidden by the fog of war
  (`Footprint2x2Creep2`). Our vision maps express it, so it is modelled.

The other flags of the shipped footprints are already modelled by today's placement code (`NoBuild`,
`GroundNoSpawn`), belong to B-63 (`Creep`), or are recorded and deferred (`Masked`, §4 D-6).

A full plan is required (charter §6): it is a new mechanic, it adds a catalog file and a required catalog field, it
changes the order admission of structures and the client's placement preview, and it spans `sim_types`, `sim_core`,
the catalog, the maps and `client`.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-19 against `main` at
`e969dc75`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations S-1 to S-6 | Closed. S-1 (RON data instead of editor data) covers authoring the flags in RON; none of the others applies | D-2 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §10 |
| Q0002: scoping is the agent's call if written down | Standing rule | §13 |
| Q0090 (splash stream frame budget) | Open, unrelated | none |
| **None new** | The data decides every rule. D-3 (a one-cell mineral keeps the game's reach), D-6 (`Masked`), D-7 (start halls are not checked) and D-8 (the shipped maps' geysers) are recorded scoping decisions | §4, §13 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| Town hall analogue (7 rows) beside resources | Refused only when its cells cover a node's cell (`CoversResource`) | Also refused when any of its 5×5 cells lies in a resource's `NearResources` reach: 3 cells beyond a mineral's cell, 3 cells beyond a geyser's 3×3, with the game's rounded corners (§2.2) |
| Spine and Spore Crawler analogues (2 rows) | Placeable in fog | Refused unless every site cell is visible to the placing player now |
| Every other structure | Unchanged | Unchanged |
| Data | None | New `assets/catalog/placement_rules.ron` (resource reach masks per resource kind, with their 5.0.14 rows). New required `placement_excludes` on every structure row of `structures.ron` (49 rows) |
| Refusal | — | `BuildRejection::FlaggedSite { flag, cell }`; `SiteRefusal::NearResources`, `SiteRefusal::NotVisible`; the client's ghost turns red and the status line names the reason |
| Shipped maps | The geysers of both maps stand inside the reach of their own start's town hall (§6.2) | Five geysers move by one or two cells (WP4), so a player can rebuild a town hall on its own start |
| Hashed state | — | None. The flags are derived from the catalog, the map, the placed structures and the vision maps; the schema stays 31 |
| Worlds that move | — | WP1 to WP3: none (no shipped world places a structure by order, §6.1). WP4 (maps): the hashed node positions only. Both goldens are re-recorded (every checkpoint); with the resources section left out, every checkpoint is unchanged. The showcase's play is byte-identical on nine seeds (§6.3) |
| Size | — | M: five packages, all S except WP2 |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`).
  - "Footprint:N" is line N of `mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`, read at
    `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml`
    (downloaded 2026-11-19 to `~/scratch/footprint-flags/dl-footprint/FootprintData-liberty.xml`).
  - "core Footprint:N" and "void Footprint:N" are the same file under `core.sc2mod` and `void.sc2mod`. `libertymulti`,
    `swarm` and `swarmmulti` define no footprint; `voidmulti` defines only Nydus creep and rock footprints, none of
    them used here.
  - "liberty:N" is line N of the extracted `UnitData.xml` under `~/scratch/combat_wp3_data/liberty/`.
- **[COMM]** Community or editor documentation.
- **[INF]** An inference of this plan.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | core Footprint:3–17, the `CFootprint` default | Set 0 (`x`) of `Check` refuses `NoBuild`, `Creep` and `Masked`. Set 0 of `Place` applies `GroundNoSpawn` and `NoBuild`. Every footprint inherits these unless it overrides them [DATA] |
| S2 | SC2Mapster wiki, [Data/Footprints](https://sc2mapster.wiki.gg/wiki/Data/Footprints) (read 2026-11-19) | `Check` "determines placement flag requirements for cells when a unit with this footprint is being placed"; `Place` "determines what placement flags are applied to cells". "Near Resources: used by the Invalid flags of the Placement Check layers of Resource Drop Off unit footprints." "Masked: is the terrain explored or not." "Fogged: is the terrain visible or hidden by fog of war." Characters: `x` is set one, `o` set two. `Area` is relative to the unit's origin [COMM] |
| S3 | Footprint:1993–2018 (`Footprint5x5Contour`), Check 1994–2003, Place 2004–2013 | Check set 0 adds `Negative NearResources` (:1996) over the 5×5. Place set 0 adds `Positive DropOff` (:2006) over the 5×5 [DATA] |
| S4 | Footprint:1835–1860 (`Footprint5x5DropOff`): :1838, :1848 | The Command Center family's and Nexus's `PlacementFootprint`: the same two flag sets [DATA] |
| S5 | Footprint:2210–2236 (`Footprint6x5DropOffCreepSource`): :2213, :2224; 2238–2264 (`…Contour`): :2241, :2252 | The Hatchery family's placement footprint and footprint: the same two flag sets, and `Creep` allowed (`value="0"`) [DATA] |
| S6 | liberty UnitData `Footprint` / `PlacementFootprint` | Command Center 10379/10380, Orbital 10525/10526, Planetary 10663/10664, Nexus 4186/4187 (`5x5Contour` / `5x5DropOff`); Hatchery 8210/8211, Lair 8292/8293, Hive 8372/8373 (`6x5DropOffCreepSourceContour` / `6x5DropOffCreepSource`); Spine 8617/8618, Spore 8751/8752 (… / `2x2Creep2`); Refinery 5778/5779, Assimilator 4124/4125, Extractor 8151/8152 (`GeyserRoundedBuilt` / `3x3CappedGeyser`). The other 37 analogues per contour plan S8 and the radius plan §2.3 [DATA] |
| S7 | Footprint:1061–1097 (`FootprintMineralsRounded`); liberty:68 (`MineralFieldDefault`, parent of every mineral field) | Area `-4,-3,4,4`. The field's own cells are `xx` (2×1, :1087). Place: set 0 and set `o` apply `NearResources` (:1079, :1082) on rows 1084–1090: `.oooooo.`, `oooooooo`, `oooooooo`, `oooxxooo`, `oooooooo`, `oooooooo`, `.oooooo.`. Check: set 0 and `o` refuse `DropOff` (:1064, :1067) on the same rows [DATA] |
| S8 | Footprint:1535–1576 (`FootprintGeyserRounded`); liberty:2127 (`VespeneGeyser`), 2158 (`RichVespeneGeyser`) | Area `-4,-4,5,5`, the geyser's own cells a 3×3 (`xxx`). Place: `NearResources` (:1556, :1559) on a 9×9 ring with one corner cell cut (rows 1561–1569). Check: `DropOff` refused on the same ring (:1538, :1542) [DATA] |
| S9 | void Footprint:3–21 (`FootprintGeyserRounded`), 22–35 (`FootprintGeyserRoundedBuilt`) | The Legacy of the Void layer rounds the geyser rings further: rows 0 and 8 become `..ooooo..`, rows 1 and 7 `.ooooooo.`, in both `Check` and `Place` (:5–8, :11–14, :24–27, :30–33). Rows 2 to 6 keep liberty's `ooooooooo` [DATA] |
| S10 | Footprint:922–962 (`FootprintGeyserRoundedBuilt`, the Refinery, Assimilator and Extractor footprint) | The same ring as the geyser: Place applies `NearResources` (:943, :946), Check refuses `DropOff` (:925, :929) [DATA] |
| S11 | Footprint:1515–1534 (`Footprint3x3CappedGeyser`, the extractors' placement footprint) | 3×3, only the default sets with `Creep` allowed (:1518). No `DropOff`, no ring [DATA] |
| S12 | Footprint:1212–1225 (`Footprint2x2Creep2`, parent `Footprint2x2`), the Spine and Spore Crawler placement footprint | Check set 0: `Positive Creep` (creep required, :1215), `Negative Fogged` (:1217) [DATA] |
| S13 | Footprint:1226–1232 (`Footprint2x2IgnoreCreepContour`, the Greater Spire's footprint) | Check refuses `Fogged` (:1230). The Greater Spire is a morph and is never placed [DATA] |
| S14 | Blizzard forums, [Coop: unable to build or root in explored fog of war](https://us.forums.blizzard.com/en/sc2/t/coop-unable-to-build-or-root-in-explored-fog-of-war/8750) (read through a search summary 2026-11-19) | Spine Crawler, Spore Crawler and similar defensive structures cannot be built or rooted in explored fog of war; the cause is named as a footprint flag [COMM] |
| S15 | SC2Mapster forum, [placement footprints of allies](https://www.sc2mapster.com/forums/development/data/161430-placement-footprints-of-allies?comment=3) and [unit altering pathing](https://www.sc2mapster.com/forums/development/data/164458-unit-altering-pathing-on-map?comment=4) (search summaries 2026-11-19) | The unit's "Pathing Footprint – Placement" is the one used while placing; "Footprint determines actual unit pathing". A reply calls the placement footprint's role in the check "a guess" [COMM] |

### 2.2 The rules that follow

1. **A placement is refused when a cell of the placed structure's `Check` layer carries a flag its set refuses** (S1,
   S2). A cell's flags are those applied by the `Place` layers of every unit whose footprint covers it, and the
   engine's per-player state flags (`Masked`, `Fogged`).
2. **Which layer is checked.** A unit with a `PlacementFootprint` is placed with it (S15) [COMM, **verify** in the
   editor]. For the flags of this plan the choice means:
   - `NearResources`: both of every town hall's footprints refuse it (S3 to S5). Unaffected.
   - `DropOff`: the extractors' `Footprint` (S10) refuses it on the geyser's ring, their `PlacementFootprint` (S11)
     does not. Under the placement footprint no shipped structure checks `DropOff`.
   - `Fogged`: the crawlers' placement footprint refuses it (S12); their footprints do not.
3. **The resources' reach.** Measured in cells from the resource's own cells, the `NearResources` ring reaches
   **3 cells** on every side. Its corners are cut:
   - mineral field (S7): one corner cell at each corner (rows of 8 × 7 around a 2×1);
   - geyser (S8 with S9, the 5.0.14 melee stack): the 9×9 around the 3×3, with row 0 and 8 holding the middle 5
     cells and rows 1 and 7 the middle 7. So 69 cells, the geyser's 9 included.
4. **What a town hall may not touch.** No cell of its 5×5 may lie in such a reach. A face of the hall therefore keeps
   at least 3 full cells from a mineral field's or geyser's cells along an axis [INF]: the well-known three-cell gap
   between a base and its mineral line.
5. **Row order.** `Rows` read from the top of the `Area` or its bottom is not documented (**verify**). Every ring
   used here is symmetric under both flips, so the order does not matter for this plan.
6. **A placed town hall applies `DropOff` to its own cells** (S3). Nothing that a player places in 5.0.14 melee
   checks it (rule 2). The resources' `Check` refusal of `DropOff` (S7, S8) acts when a resource is placed, which
   happens in the map editor, not in a match.
7. **`Fogged` is the engine's per-player flag for a cell hidden by fog of war** (S2, S14). The crawlers' rule is "every
   site cell visible to the placing player now" [INF from S2 and S12].
8. **Start-location town halls.** Melee initialisation creates each player's town hall at its start location; whether
   that creation checks placement flags is not sourced (**verify**). Blizzard's ladder maps keep the three-cell gap,
   so the question never arises there.

### 2.3 The flags of the shipped footprints

Resolved with inheritance from S1 by `~/scratch/footprint-flags/scripts/fp.py`. Only flags set to 1 are listed; `−`
means refused by `Check`, `+` applied by `Place` on the structure's own cells.

| Analogues (count) | Placement footprint | Check | Place |
|---|---|---|---|
| Command Center, Orbital, Planetary, Nexus (4) | `5x5DropOff` | −NoBuild −Creep −Masked **−NearResources** | +NoBuild +GroundNoSpawn **+DropOff** |
| Hatchery, Lair, Hive (3) | `6x5DropOffCreepSource` | −NoBuild −Masked **−NearResources** | +NoBuild +GroundNoSpawn **+DropOff** |
| Spine Crawler, Spore Crawler (2) | `2x2Creep2` | −NoBuild −Masked **−Fogged**, +Creep required | +NoBuild +GroundNoSpawn (footprints add +Creep) |
| Refinery, Assimilator, Extractor (3) | `3x3CappedGeyser` | −NoBuild −Masked | +NoBuild +GroundNoSpawn (footprint adds the ring: +NearResources; Check −DropOff on the ring) |
| Terran and Protoss 3×3, 2×2, 1×1, add-ons (26) | `3x3`, `2x2`, `1x1`, `3x3AddOn2x2` | −NoBuild −Creep −Masked | +NoBuild +GroundNoSpawn |
| Zerg 3×3 and 2×2 (11) | `3x3Creep`, `2x2Creep` | −NoBuild −Masked | +NoBuild +GroundNoSpawn |

| Flag | Where | In our game | This plan |
|---|---|---|---|
| `NoBuild` | Every Check and Place (S1) | `CellFlags::BUILDABLE`, cleared under a structure (`grid.rs` `can_place_building_where`, `place_structure`); a resource node's cell (`CoversResource`) | Already modelled; not authored (D-2) |
| `GroundNoSpawn` | Every Place (S1) | Spawn rules skip occupied cells (scenario rings, production exits) | Already modelled; not authored |
| `NearResources` | Town hall Check; resource Place | — | **Modelled** (D-3, D-4) |
| `DropOff` | Town hall Place (own cells); resource Check; extractor Footprint Check | — | Not modelled: it can decide no placement in a match (D-5); the WP4 guard covers its map side |
| `Fogged` | Crawler placement Check | Vision maps (`VisibilityState`) | **Modelled** (D-5) |
| `Masked` | Every Check (S1) | `VisibilityState::Unexplored` | Not modelled: D-6, registered |
| `Creep` | Terran and Protoss Check refuse it; Zerg crawlers require it; crawlers apply it | None | B-63, out of scope |

---

## 3. Diagnosis

| # | Today | File, symbol |
|---|---|---|
| T1 | A site is judged by `SimWorld::check_site`: terrain, occupancy and cliff through `GameMap::can_place_building_where` (judged by the player's knowledge of structures at admission and by the truth on arrival), remembered structures, and the node rule: no site covers a node's cell, except an extractor, which must cover a gas node | `crates/sim_core/src/production/construction.rs` `check_site` |
| T2 | `check_site` runs at admission (`admit_build` → `check_build`, `Knowledge::Player`), on the builder's arrival (`step_build` → `check_build`, `Knowledge::Full`) and for the client's preview (`player_view.rs` `preview_site`) | same, `player_view.rs` |
| T3 | A resource node is one cell (`ResourceNode { pos, kind, remaining, cell }`), placed at the cell's centre; it occupies no other cell, blocks no path and stays on the map at `remaining == 0` | `crates/sim_map/src/grid.rs`, `map_format.rs` `to_game_map` |
| T4 | The client centres an extractor's 3×3 on the gas node within 3 cells of the click | `crates/client/src/battle_input/command_mode.rs` `placement_cell`, `nearest_gas_cell` |
| T5 | Scenario setup places each start's town hall with `spawn_structure` → `place_structure`, which checks terrain and occupancy only | `crates/sim_core/src/scenario.rs` `place_town_hall` |
| T6 | The preview's refusal is `SiteRefusal`, mapped from `BuildRejection` by `site_refusal`; the ghost is one quad, green or red; `site_reason` gives the status text | `crates/sim_types/src/player_view.rs`, `crates/client/src/battle_hud/placement.rs`, `battle_pipeline/status.rs` |
| T7 | No shipped scenario, fixture, golden, matrix battle or showcase issues a `Build` order (`grep` of `assets/scenarios`, `assets/catalog/start_loadouts.ron`, `crates/sim_core/tests/fixtures`, `crates/client_diagnostics`); there is no AI that builds | — |

**Finding F-1 (registered, B-147).** Our resource nodes have no 5.0.14 footprint: a mineral field is a 2×1 obstacle
(S7, its `Shape` ±1 × ±0.5) and a geyser a 3×3 one (S8); ours are one cell that blocks nothing, and a mined-out
mineral node stays on the map where the game removes the field (**verify**). Out of scope; D-3 says how this plan
anchors the reach on our one cell.

**Finding F-2 (§6.2).** Both shipped maps place geysers inside the reach of their own start's town hall.

---

## 4. Design decisions

### D-1. The checked layer is the placement footprint

Placement uses each analogue's 5.0.14 `PlacementFootprint` `Check` layer (§2.2 rule 2, S15, **verify**). It decides
nothing for `NearResources`. It is why no shipped row checks `DropOff`, and why the crawlers check `Fogged`. Each
row's comment names the footprint and lines it was read from.

### D-2. Data model: authored flag sets, a closed vocabulary of modelled flags

- **Vocabulary.** `sim_types::PlacementFlag { NearResources, Fogged }` (new `crates/sim_types/src/placement.rs`,
  `Copy, Eq, Ord, Hash, Serialize, Deserialize`), with `PlacementFlags`, a `u8` bit set (`bitflags`, as `CellFlags`).
  - These are the game's flag names, mechanics and not catalog identities (charter §3.3, C5): no unit, structure,
    faction or class is named in Rust.
  - Only modelled flags are in the enum. An authored name outside it (`Creep`, `Masked`, `NoBuild`) fails to load
    with serde's typed unknown-variant error, wrapped in the catalog's `invalid(slug, "placement_excludes", …)`.
  - `NoBuild` and `GroundNoSpawn` stay where they are modelled today (§2.3). Authoring them would duplicate rules
    that every structure shares (S1), with no row that differs. `DropOff` is left out by D-5.
  - Only the `Check` side is authored on structures: no modelled flag is applied by a structure's `Place` layer.
- **Structures** (`StructureArchetypeDef`, `crates/sim_core/src/catalog.rs`): required
  `pub placement_excludes: Vec<PlacementFlag>`, the flags its placement footprint's `Check` refuses beyond the
  defaults every structure shares.
  - Required, like `footprint_shape` and `radius`: a catalog written before the field fails to load, so no row can
    silently lose a 5.0.14 fact. 40 of 49 rows author `placement_excludes: []`.
  - RON: `placement_excludes: [NearResources],` with a comment citing the placement footprint and its Footprint
    lines.
  - Resolved onto `UnitArchetype` as `pub placement_excludes: PlacementFlags`. A mobile unit's is empty.
- **Resource reach** (new `assets/catalog/placement_rules.ron`, loaded by `catalog/runtime.rs` beside
  `movement_rules.ron` into `UnitCatalog::placement_rules()`):

  ```ron
  (
      resource_footprints: [
          // 5.0.14 FootprintMineralsRounded Place (Footprint:1077–1091): NearResources on the field's cells and
          // 3 cells beyond them, one corner cell cut. Our node is one cell, not 2×1 (B-147; plan D-3).
          (kind: Mineral, applies: [NearResources], anchor: (3, 3), rows: [
              ".xxxxx.", "xxxxxxx", "xxxxxxx", "xxxxxxx", "xxxxxxx", "xxxxxxx", ".xxxxx.",
          ]),
          // 5.0.14 FootprintGeyserRounded Place (Footprint:1554–1570), rows 0, 1, 7, 8 as void Footprint:11–14.
          // The node is the geyser's centre cell (the extractor's 3×3 is centred on it).
          (kind: Gas, applies: [NearResources], anchor: (4, 4), rows: [
              "..xxxxx..", ".xxxxxxx.", "xxxxxxxxx", "xxxxxxxxx", "xxxxxxxxx",
              "xxxxxxxxx", "xxxxxxxxx", ".xxxxxxx.", "..xxxxx..",
          ]),
      ],
  )
  ```

  - `rows[0]` is the row of lowest `y` (our cell `y` grows with world `y`); `anchor` is `(column, row)` of the
    node's cell. §2.2 rule 5 makes the order immaterial for these masks.
  - Resolved to `ResourceReach { kind, applies: PlacementFlags, cells: Vec<(i8, i8)> }`, offsets from the node's
    cell, sorted `(dy, dx)`, built once at load.
  - `ResourceKind` is the existing `sim_types` enum the map format already uses; it names a mechanic class, not a
    catalog identity.
- **Loader validation** (`catalog/validation.rs`, typed `invalid(...)` errors):
  1. each `ResourceKind` at most once; a kind with no row applies nothing;
  2. `rows` non-empty, all the same length, at most 15 × 15, only `.` and `x`;
  3. `anchor` inside the rows and on an `x` (a resource applies its flags to its own cell, as `set 0` does in S7, S8);
  4. `applies` non-empty, no duplicates, and not `Fogged` (an engine state flag; S2 lists it only under Check);
  5. a structure's list without duplicates.

### D-3. A mineral node keeps the game's reach, not its size

Our mineral node is one cell (F-1). The authored mask keeps what a player sees of the rule: the reach of 3 cells
beyond the resource's own cells, with the corner cell cut (S7). The alternative, the game's 8 × 7 rows anchored on
one of the 2×1's cells, would make the reach one cell longer on one side, depending on which cell the anchor names.
Recorded as a minor difference under B-147. The geyser mask is the game's own (S8, S9), because our gas node is the
centre of the extractor's 3×3 (T4).

### D-4. The rule

`check_site` gains one step after the node rule, `check_flags(player, arch, site, knowledge)`:

1. If `arch.placement_excludes` is empty, return `Ok` (40 of 49 rows: no work).
2. For each site cell, in `y` then `x` order, its flags for `player` are the union of:
   - **resources:** `NearResources` (and any other `applies`) when the cell is in some node's reach. Nodes are
     public (T1), so every node counts, depleted ones too (B-147);
   - **state:** `Fogged` when `vision_maps[slot].visibility_at(x, y) != Visible` for the placing player.
3. The first cell whose flags meet `placement_excludes` refuses with
   `BuildRejection::FlaggedSite { flag: PlacementFlag, cell: (u32, u32) }`, the lowest flag in enum order.

- **Order of rules.** Terrain, occupancy and the node rule run first, so every rejection that exists today keeps
  its reason. A town hall on a mineral cell still says `CoversResource`.
- **Arrival.** The step runs in both modes. On arrival `Fogged` reads the same vision map, which the builder's sight
  normally keeps clear; a site beside the builder that is still fogged is refused on arrival, as the game re-checks
  a placement when the worker arrives (**verify**).
- **Cost.** Only for the nine rows that check a flag: at most 25 cells × nodes. The shipped maps have 20 nodes. No
  allocation: the reaches are slices built at load, and the loop reads the map's node list in place.
- **PERFORMANCE RATIONALE.** None needed: the step is a plain loop over a few hundred cell tests per order and per
  preview change, not per tick.

### D-5. `DropOff` and `Fogged`

- **`DropOff` is not modelled.** A town hall applies it to its own 5×5 (S3 to S5). Those cells are occupied, so
  every placement that could meet the flag is already refused for occupancy. The only refusals of `DropOff` are the
  resources' `Check` (S7, S8), which acts when a resource is placed, and the extractors' `Footprint` (S10), which is
  not their placement footprint (D-1). So the flag cannot decide a placement in a match, and modelling it would add
  a code path no world can reach. Its map-side meaning, that a resource's ring holds no town hall cell, is what the
  WP4 guard checks on every shipped map's starts. If D-1 proves wrong for extractors (R-1), the flag returns with
  that evidence.
- **`Fogged`** is authored on the Spine and Spore Crawler analogues' `placement_excludes` (S12, S14). The creep that
  the same layer requires stays out of scope (B-63).

### D-6. `Masked` is not modelled

Every 5.0.14 footprint refuses `Masked`, "is the terrain explored or not" (S1, S2). Our vision has
`VisibilityState::Unexplored`. It is not modelled here, for two reasons:
- Whether a 5.0.14 melee map has a black mask at all (the editor's "Unexplored Areas" option) is not sourced
  (**verify**). Without one, the flag is never set in a match.
- Players have seen nothing on tick 0, so the rule would refuse every order at tick 0 in tests and scenarios. It
  needs its own measurement.

Registered as B-148. A later plan sources the mask and models it.

### D-7. Start town halls are not checked

Scenario setup (T5) places each start's town hall without the flags, as melee initialisation creates it (§2.2
rule 8, **verify**). The rule applies only to orders: admission, arrival and the preview. A guard test checks that
every shipped map's start footprints clear every resource reach (WP4). So a map that breaks the rule fails a test,
not a match.

### D-8. The shipped maps' geysers move out of reach (WP4)

F-2 means a player cannot rebuild a town hall on its own start under the rule. That matters on the skirmish map. The
fix is map data:

| Map | Node | From | To | Why |
|---|---|---|---|---|
| `showcase_canyon` | Gas (slot 0) | (14, 6), (14, 10) | (15, 6), (15, 10) | 3 cells from the hall's face to the geyser's 3×3 |
| `showcase_canyon` | Gas (slot 1) | (49, 57), (49, 53) | (48, 57), (48, 53) | mirror |
| `default_arena` | Gas (slot 1) | (48, 56) | (50, 56) | the same gap; its minerals at (49..53, 52..54) already keep 3 cells |

The gaps are checked by `~/scratch/footprint-flags/scripts/scan_maps.py` (§6.2). The extractor's 3×3 around each new
cell must be placeable (WP4 test). Since resource nodes are hashed (`checksum.rs`: `world.map.resources()`), WP4
moves every checkpoint of `golden_replay` (it plays `default_arena`). It also moves the showcase's hashed state, and
its play only if a unit's behaviour reads the moved nodes (§6.3).

### D-9. Client

- `SiteRefusal` gains `NearResources` and `NotVisible`. `site_refusal` maps `FlaggedSite { flag: NearResources, .. }` to
  `NearResources` and `{ flag: Fogged, .. }` to `NotVisible`.
- `site_reason`: "too close to resources" and "the site must be in sight". `write_build_rejection` gives the
  same wording for an arrival refusal.
- The ghost already turns red through `preview_site` (T6). The status line shows the reason, as for every refusal
  today.
- A per-cell placement grid with the offending cells drawn (the game's) is out of scope (§13). The ghost stays one
  quad.
- **Knowledge.** The preview reads `Knowledge::Player`. Resource reach is public and `Fogged` is the viewer's own
  vision, so the preview reveals nothing.

---

## 5. Determinism and checksum

- **No new state; the schema stays 31.** Flags are derived on demand from the catalog, the map's nodes, the placed
  structures (`map.structures()` and their archetypes) and the vision maps. All are deterministic and already
  authoritative. Nothing is stored, so nothing is hashed.
- **Integer arithmetic only:** cell offsets `i8`, cells `u32`, compared with checked conversions. No float; the RON
  holds strings and integers only (C7 unaffected).
- **No hashed collection:** reaches are sorted `Vec`s (C9).
- **Order of evaluation** is fixed (cells by `y` then `x`, flags in enum order), so the reported flag and cell are
  deterministic. They reach the client only through `SiteRefusal`.
- **The catalog is not hashed** (`checksum.rs`); only outcomes are. A world in which no `Build` order meets a
  flagged site is bit-identical.
- **WP4's map edit** changes the hashed `resources` section (positions), so `golden_replay` is re-recorded once with
  a proof header (§7).

---

## 6. Which worlds move: prototype and measurements

Prototype: a scratch worktree of `e969dc75` (`.claude/worktrees/footprint-flags-proto`,
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/footprint-flags-proto`, removed at hand-back). It hard-coded D-4
for the seven drop-off rows (the two masks of D-2) and `Fogged` for the two crawler rows, returning
`CoversResource`. Under `PROTO_LOG` it logged every call of `check_site` and every flag refusal. The diff is kept at
`~/scratch/footprint-flags/proto/proto.diff`.

### 6.1 Rule only (WP1 to WP3)

| Run (prototype tree, `PROTO_LOG=1`) | `check_site` calls | Flag refusals | Result |
|---|---:|---:|---|
| `cargo test -p sim_core --no-fail-fast` (every target; under `flock ~/scratch/gate.lock`) | — | — | 986 passed, **1 failed** |
| …the same binaries re-run with `--nocapture` to count calls (`scripts/protolog.sh`) | 5,529 in 7 targets: `player_view_tests` 5,425, `construction_lifecycle_tests` 78, `full_match_tests` 14, `contour_footprint_tests` 6, `terrain_radius_tests` 2, `match_result_tests` 2, `faction_catalog_tests` 2 | 182, all `Fogged`, all in one test | — |
| Town hall sites among them | 671 (the seven drop-off rows) | **0** `NearResources` | No test places a town hall within reach of a node |
| Matrix, every battle, seeds of record (232 runs) | 0 | 0 | — |
| Battle 33, seeds of record and 64 more | 0 | 0 | — |
| Showcase, default seed and seeds 1 to 8 | 0 | 0 | Default seed: player 1 wins at tick 1,935 |
| `golden_replay`, `golden_replay_orders` | 0 | 0 | Pass, unmoved |

The failing test is `player_view_tests::can_raise_matches_admission_for_every_catalog_pair`. It admits every
structure at cell (100, 2), outside every unit's sight, so the two crawler rows are refused `Fogged` (91 checks each).
That is the rule working, not a defect: WP2 re-derives the test so that the crawlers' site is in sight (a sighted
unit beside it), and adds F-8 for the refusal itself.

Client tests were not run. Their placements were read instead: `battle_card_tests` and `battle_context_order_tests`
place the Barracks analogue (`assembly_bay_dome`), and `battle_card_path_tests` places the Refinery analogue and the
carrier path's structures. No town hall or crawler is placed, so none can meet the rule.

**So WP1 to WP3 move no shipped world.** No matrix battle, battle 33, showcase seed or golden calls `check_site`
(T7), so the rule cannot reach them. Only the one test above changes.

### 6.2 Scenarios and maps

`scan_maps.py` (start footprints against the D-2 reaches; output `~/scratch/footprint-flags/proto/map_scan.txt`):

| Map (scenarios) | Slot | Hall cells | Cells in a reach | Nearest node gaps (cells between hall face and node cell) |
|---|---|---|---|---|
| `default_arena` (`golden_replay`, `golden_replay_orders`, test worlds) | 0 | x 18–22, y 4–8 | none | gas 6, minerals 8 |
| | 1 | x 41–45, y 52–56 | **7** (gas (48, 56)) | gas **2**, minerals 3 |
| `showcase_canyon` (`showcase`, `showcase_effects`, `skirmish`, `default_battle`) | 0 | x 6–10, y 6–10 | **3 + 3** (gas (14, 6), (14, 10)) | gas 3 (one short of the geyser's 3×3 plus 3), minerals 3 |
| | 1 | x 53–57, y 53–57 | **3 + 3** (gas (49, 57), (49, 53)) | as slot 0 |

The minerals of both maps keep the game's three-cell gap. Every geyser near a start is one cell too close (two on
`default_arena` slot 1). With D-8's moves, every start clears every reach (`maps/` copies, same script).

### 6.3 The map move (WP4)

D-8's five moves, prototyped with the moved maps in `~/scratch/footprint-flags/maps/`:

| Run | Result |
|---|---|
| Showcase, default seed and seeds 1 to 8, `route_trace --assets <copy with the moved maps>` | Every output (`units.csv`, `reversals.csv`, `passes.csv`, `overlap.csv`, `groups.md`, `summary.md`) byte-identical to the base on all nine seeds. A control run with the map removed from the copy fails to load, so the copy was read |
| `golden_replay_tests` with the moved `default_arena` | Both fixtures (`golden_replay`, `golden_replay_orders`) diverge at checkpoint 0, the first one, because node positions are hashed (§5) |
| The same, both maps, with `world.map.resources()` left out of the checksum (scratch switch `PROTO_SKIP_RES`) | The 80 checkpoint values of both fixtures (original map and moved map) are **identical**. So nothing but the hashed positions moves |
| Matrix and battle 33 | Untouched: their worlds are generated 64-cell arenas (`map_cells: 64` in `matchup_matrix.ron`), not the shipped maps |

So WP4 moves hashed state only: both goldens are re-recorded once under the proof of §7 rule 3, and the
showcase's play and storyboard stay as they are. No worker in the showcase or the goldens reads the moved gas
nodes before the end of the run.

---

## 7. Stop rules and acceptance

As in the contour, contact and crowd plans: **a winner flip anywhere stops the package.** The agent commits nothing
beyond it and reports the trace.

1. **WP1 and WP2 (vocabulary, data, wiring with the rule) are bit-identical** on the matrix (all battles, seeds of
   record), battle 33 (64 seeds), the showcase (default seed and seeds 1 to 8, `units.csv` byte-identical) and both
   goldens (native debug and release, `wasm32-wasip1` release). The prototype shows why: none of them calls
   `check_site` (§6.1). A difference is a defect; never re-record.
2. **WP3 (client) changes no simulation file.**
3. **WP4 (maps) moves hashed state only, unless a trace says otherwise.**
   - `golden_replay` is re-recorded once. Its header names this plan and WP4, and holds the proof: a scratch build
     that hashes the resources section from the old map positions reproduces every recorded checksum. An
     entity-only digest (entities, projectiles, players, effects) is unchanged through tick 1,000, or the first
     differing tick is traced to a unit that reads a moved node.
   - `golden_replay_orders` follows the same rule (same map; prototyped, §6.3).
   - Matrix and battle 33: bit-identical (their worlds are generated arenas, §6.3).
   - **Showcase.** Expected byte-identical `units.csv`, the hashed node positions aside (§6.3). If it moves, the
     first divergent tick and unit are traced. A movement traced to a worker that reads a moved node is
     rule-caused; anything else is a defect.
   - **Survivors and placebos (crowd-slide D-2).** A survivor-range miss on a seed of record stops the package
     **unless a placebo build of the same base shows the same miss**. The placebos are the three one-raw-unit
     patches of plan `2026-11-19-crowd-edge-steering-2` (`~/scratch/crowd-edge-2/patches/patches-placebo{1,2,3}.diff`)
     re-built on this plan's base (crowd-slide D-3: a band is never carried across bases). A miss that only a
     placebo also shows is reported, not stopped on. A winner change that no placebo shows stops the package. Battle 26
     seed 33 is known to be decided by noise. The placebos are built only if WP4 moves a survivor range at all.
   - **If the showcase default-seed winner flips:** stop and trace from tick 0. The orchestrator decides by
     amendment. No constant is tuned.
4. **Test edits.** A changed test is re-derived from a trace and stated in the commit; no bound is loosened. A
   test that fails for a reason §6.1 does not list is a finding and stops the package.
5. **No tuning constant is introduced.** Every mask cell and flag comes from §2 with its line.

---

## 8. Client consequences

Covered by D-9. The tests:

- `crates/client/src/battle_pipeline/status/tests.rs`: every `SiteRefusal` has a reason; the two new strings.
- `crates/client/tests/battle_card_tests.rs` (beside the preview check at `preview_site`): in a session on
  `showcase_canyon`, a town hall analogue's ghost two cells from the mineral line is red and the status names "too
  close to resources". The same ghost on the player's own start (after WP4) is green.
- The placement ghost is not re-rendered: its colours and geometry do not change, so no GPU capture is needed.

---

## 9. File ownership

| Area | Files | WP |
|---|---|---|
| Records | `docs/plans/README.md` (row), `DESIGN.md` §4 (B-139 restated; B-147, B-148 new) | WP0 |
| Vocabulary and types | `crates/sim_types/src/placement.rs` (new), `lib.rs`, `player_view.rs` (`SiteRefusal`) | WP1 |
| Catalog | `crates/sim_core/src/catalog.rs` (`placement_excludes`), `catalog/placement_rules.rs` (new), `catalog/runtime.rs`, `catalog/validation.rs`, `assets/catalog/placement_rules.ron` (new), `assets/catalog/structures.ron` (49 rows), test catalogs under `crates/*/tests/fixtures` that hold structures (each gains the field) | WP1 |
| Rule | `crates/sim_core/src/production/construction.rs` (`check_flags`, `BuildRejection::FlaggedSite`), `player_view.rs` (`site_refusal`), new `crates/sim_core/tests/footprint_flag_tests.rs` | WP2 |
| Client | `crates/client/src/battle_pipeline/status.rs`, `status/tests.rs`, `crates/client/tests/battle_card_tests.rs` | WP3 |
| Maps | `assets/maps/showcase_canyon.scmap.ron`, `assets/maps/default_arena.scmap.ron`, `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`, `crates/sim_core/tests/scenario_tests.rs` (guard), `assets/captures/showcase.ron` (only if the timeline moves) | WP4 |
| Bookkeeping | `DESIGN.md` (register, the Economy row), this plan, evidence `docs/evidence/2026-11-19-footprint-flags.md` | WP5 |

---

## 10. Work packages (commit order)

- **Branch:** `footprint-flags`.
- **Tests first:** each package writes its tests first and shows them red on its base (WP0 and *guard* tests
  excepted).
- **Gates:** implementation agents hand back after `cargo gate quick` and the tests their change needs; a verifier
  runs `cargo gate package` once on the final tree (Q0061, charter §4). New test files are `fast` unless measured
  `slow`, in which case `crates/gate/suites.ron` gains a rule (C22).
- **Sizes:** S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- `DESIGN.md` §4: B-139 restated as "planned, `2026-11-19-footprint-flags`" with its sources corrected (the ring's
  5.0.14 rows are void Footprint:11–14 over liberty Footprint:1554–1570; the hall's refusal is in both its footprints).
- New rows (B-146 is proposed by plan `2026-11-19-funnel-fallbacks`; if another plan has taken these numbers when WP0 lands, WP0 takes the next free ones and updates this plan by a pointer line):
  - **B-147** (F-1): resource nodes have no 5.0.14 footprint. Minerals are 2×1 and geysers 3×3 obstacles in the
    game; ours are one cell that blocks nothing; a mined-out mineral node stays (**verify**). It includes D-3's
    recorded minor difference.
  - **B-148** (D-6): `Masked` (unexplored cells) is not checked when placing; whether melee maps set it is unsourced.
- This plan's row: `Proposed` → `Active`.

### WP1: vocabulary, data and loader (S, `sim_types`, `sim_core` catalog, data)

Tests first (`catalog/validation.rs` unit tests and `crates/sim_core/tests/runtime_catalog_tests.rs`):
- `placement_excludes_is_required`: the shipped structures with the field removed from one row fail to load and name
  it.
- `an_unknown_flag_is_refused` (`Creep`, `DropOff`), `fogged_is_never_applied` (by a resource),
  `duplicate_flags_are_refused`.
- `resource_reach_rows_are_validated`: ragged rows, a foreign character, an anchor off the rows or on `.`, a kind
  twice, an empty `applies`, `Fogged` applied: each a typed error.
- `the_shipped_reaches_are_the_5_0_14_rings`: the mineral reach is 45 cells (the 7 × 7 less its four corners); the
  gas reach is 69 cells (§2.2 rule 3). Both are symmetric under both flips.
- `the_shipped_flag_sets` (*reference*, `catalog_reference_values_tests.rs`): exactly the seven drop-off rows exclude
  `NearResources`; exactly the two crawler rows check `Fogged`; the other 40 are empty.
- `runtime_catalog_tests`: `placement_rules.ron` is read at run time (a changed mask in a temp copy of `assets/catalog`
  changes the loaded reach; C6).

Then the types (D-2), the loader, the RON file and the 49 rows with their comments. **Bit-identical** (nothing reads
it yet; §7 rule 1, goldens and matrix by test, showcase by `route_trace`).

### WP2: the rule (M, `sim_core`)

Tests first, new `crates/sim_core/tests/footprint_flag_tests.rs` (on `sim_fixtures` worlds with explicit nodes):
- F-1 `a_town_hall_is_refused_within_reach_of_minerals`: for each of the seven drop-off rows, the site whose face is
  2 cells from a mineral node is `FlaggedSite { NearResources, cell }` at admission, with the expected cell; at a
  3-cell gap it is admitted.
- F-2 `the_mineral_reach_has_its_corners_cut`: a hall whose corner cell is diagonal (3, 3) from a mineral node is
  admitted; (3, 2) is refused.
- F-3 `the_geyser_reach_is_the_void_ring`: every offset of the 9×9 around a gas node, tested with a 1×1 test row that
  checks `NearResources`: refused exactly on the 69 cells.
- F-4 `a_town_hall_on_a_node_still_says_covers_resource` (rule order).
- F-5 `other_structures_ignore_the_reach`: a 3×3 production row and an extractor on its gas node are admitted inside
  the reach.
- F-6 `admission_arrival_and_preview_agree`: for every site of a town hall analogue on a 13 × 13 grid around a
  mineral node and a gas node, `preview_site`, `admit_build` and the arrival check give the same verdict; an
  admitted order is built.
- F-7 `the_rule_reads_the_catalog`: a test catalog whose 3×3 production row excludes `NearResources` refuses it in
  reach, and a `placement_rules` copy with a 1-cell mineral mask admits a hall 1 cell from a mineral (charter §3.3:
  no code change).
- F-8 `a_crawler_needs_sight`: a Spine and a Spore analogue site with one cell fogged is refused
  (`FlaggedSite { Fogged, .. }`) and visible is admitted. The preview gives `SiteRefusal::NotVisible`.
- F-9 `the_preview_reports_near_resources` (`player_view_tests.rs`): `preview_site` gives
  `SiteRefusal::NearResources`.
- F-10 *guard* `no_shipped_world_orders_a_build`: no shipped scenario, start loadout, matrix battle or golden
  fixture holds a `Build` order (read from the files). This is the reason for §7 rule 1 (T7); a world that adds one
  must be measured under the rule.

Then `check_flags`, the variant, `site_refusal`. Measurements per §7 rule 1, with the evidence's commands (contour
plan D-14's table, `~/scratch/footprint-flags/`).

### WP3: client (S)

§8's tests first, then the two reasons. No simulation file changes.

### WP4: maps (S, data)

Tests first:
- *guard* `every_shipped_start_clears_every_resource_reach` (`scenario_tests.rs`): for every map under
  `assets/maps/`, each start footprint (5×5) has no cell in any node's reach, read from the shipped catalog. Red on
  the base (§6.2).
- `every_shipped_gas_node_takes_an_extractor`: for each gas node, the extractor analogue centred on it passes
  `check_site` on a fresh world.

Then D-8's five moves, and the measurements of §7 rule 3 with the re-record of `golden_replay` (and of
`golden_replay_orders`) under its proof. If the showcase timeline moves, the storyboard
(`assets/captures/showcase.ron`) is re-checked as in the contour plan's WP7.

### WP5: bookkeeping (S)

- `DESIGN.md`: B-139 fixed (with commits), the Economy row names the rule; §5 untouched (no throughput change: the
  rule is not in the tick).
- Evidence `docs/evidence/2026-11-19-footprint-flags.md`: §6's tables re-measured on the merged tree, "How to
  reproduce".
- This plan's `## As built`; README row `Done`.

---

## 11. Test plan (summary)

| Layer | Tests |
|---|---|
| Loader | WP1's validation and reference tests; `runtime_catalog_tests` |
| Rule | F-1 to F-10 |
| Client | status reasons; the ghost on `showcase_canyon` |
| Maps | the two WP4 guards |
| Worlds | matrix, battle 33, showcase nine seeds, both goldens on every target (§7) |

---

## 12. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The checked layer is the footprint, not the placement footprint (D-1 **verify**) | Extractors would refuse a geyser whose ring holds a town hall's cell; crawlers would not check `Fogged` | Both are a data change in `structures.ron` (D-2), no code. After WP4 no shipped map has a hall in a ring, so the extractor case cannot arise on shipped maps |
| R-2 | `Fogged` reads "not visible now"; the game might treat unexplored differently | A crawler site in black mask, if any | Recorded with B-148; melee has no black mask (**verify**) |
| R-3 | A test builds a town hall or crawler where the new rule refuses it | WP2 test edits | §6.1 lists every such test; each is re-derived (§7 rule 4) |
| R-4 | WP4 moves a world through workers that read nodes | Showcase or golden play moves | §7 rule 3: traced, rule-caused only with a trace |
| R-5 | The required field touches every test catalog with structures | Mechanical WP1 diff | The loader's error names the field; `grep -rln "footprint_shape:" crates` lists the files |
| R-6 | D-3's mineral mask is one cell short on one side against the game's 2×1 | A hall can stand one cell closer to one end of a mineral line than in the game | Recorded under B-147; it goes away when nodes get their footprints |

---

## 13. Out of scope and decisions recorded (Q0002)

- **Creep** (required by Zerg crawler placement, refused by Terran and Protoss, applied by crawlers and hatcheries) and
  **power**: B-63 (no larva, warp-in, creep or power).
- **`Masked`** (D-6): B-148.
- **Resource node footprints** (minerals 2×1, geysers 3×3 as obstacles; removing a mined-out mineral): B-147.
- **`DropOff`** (D-5) and the **placement of resources** (their `Check` refusal of `DropOff`): resources are map data,
  never placed in a match. The WP4 guard covers the map side.
- **Start town halls** are not checked at setup (D-7).
- **The extractor's built ring** (`GeyserRoundedBuilt` applies `NearResources` on the geyser's ring, S10): equal to
  the geyser's own ring, which stays applied while the extractor stands. Nothing to add.
- **A per-cell placement grid** in the client (the game draws the refused cells): the ghost stays one quad.
- **`PlacementFootprint` shapes** (contour plan §10): unchanged.

---

## 14. Prototype material (scratch, not committed)

- Game data: `~/scratch/footprint-flags/dl-footprint/FootprintData-{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti}.xml`
  (5444e12f13), never executed.
- Scripts in `~/scratch/footprint-flags/scripts/`: `fp.py` (resolves a footprint's flag sets with inheritance),
  `scan_maps.py` (start footprints against the reaches), `protolog.sh` (re-runs the test binaries counting
  `check_site` calls), `golden_proof.sh` (both goldens with the original and moved map, with and without the
  resources section).
- Prototype diff: `~/scratch/footprint-flags/proto/proto.diff` (the rule, the `PROTO_LOG` counters and the
  `PROTO_SKIP_RES` checksum switch); logs under `~/scratch/footprint-flags/proto/` (`sim_core_tests.log`,
  `protolog/`, `matrix/`, `route/`, `golden/`).
- Moved maps: `~/scratch/footprint-flags/maps/`.
- The scratch worktree and its target directory were removed at hand-back; the diff rebuilds it on `e969dc75`.
