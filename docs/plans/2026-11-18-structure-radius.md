# Blueprint: Ranges to a Structure Measure to Its Engine Radius (B-135)

This plan fixes one recorded bug by matching StarCraft II:

- **B-135.** A structure's `collision_radius` is half its footprint width (`StructureArchetypeDef::into_archetype`, `footprint.0 × 0.5`). Several things read it:
  - weapon and scan ranges, to and from a structure;
  - the cast range;
  - projectile flight time;
  - spell missile contact;
  - splash distance;
  - the Charge window;
  - boarding reach;
  - the pursuit band of an attacker chasing a structure.

  It is also the drawn dome's size. The register says the right value is the analogue's 5.0.14 `Radius`.

**Correction found while planning (§2).** The game measures to neither. The live 5.0.14 engine gives every structure the radius `max(Radius, ⌈16 · Shape.Radius⌉ ÷ 16)`, where `Shape.Radius` belongs to the footprint's contour in `FootprintData.xml`. This matches all 49 of our analogues. Examples:
- the Command Center, Nexus and Hatchery are 2.75, not 2.5, 2 and 2;
- every ordinary 3×3 contour building is 1.8125;
- 2×2 contour buildings are 1.125, unless their `Radius` is larger.

So 45 of our 49 structures reach and are reached about 0.0625 to 0.56 further than today. Four cannot take the game's value because our footprint is larger than the game's: the Sensor Tower, Spore Crawler, Spire and Greater Spire analogues. Their radius is floored at half their footprint (D-4), and the footprint mismatch is registered as B-138.

The drawn dome stays the footprint's size. It is presentation, and the engine radius is not a drawing size (D-6).

A full plan is required (charter §6): the change moves combat timing near structures, possibly `golden_replay` and the showcase, and spans the catalog, loader, simulation, client presentation and tooling evidence.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-18 against `main` at `c06d8f5c`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked **verify**. Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1. An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, D-1, D-4 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if written down | Standing rule | D-4, §10 |
| Q0087 (Zergling `InnerRadius`) | Closed: reading (a), 0.375. Affects which unit radius meets the footprint, not the structure's radius | — |
| Q0083, Q0086, Q0088 (GPU paths, lock queue) | None touches this plan except WP5's renders, which follow the guide's 60 s rule | WP5 |
| **None new** | The rule is sourced (§2). The floor for four analogues (D-4) follows from footprints we already know differ, and is registered as a bug (B-138), not asked | §10 |

---

## 1. Summary

| | Today | After this plan |
|---|---|---|
| A structure's radius for ranges: weapon, scan, cast, projectile, missile, splash, Charge window, boarding, pursuit band; as target and as attacker | `footprint.0 ÷ 2` | Authored `radius` in `structures.ron` (required) = the analogue's 5.0.14 engine radius `max(Radius, ⌈16·Shape.Radius⌉/16)`, floored at half the footprint's smaller side (D-4) |
| Harvest, return and build reach (`economy::within_reach`) | Footprint gap | Unchanged (footprint) |
| Unit against structure contact (B-131) | Footprint at the unit's terrain radius | Unchanged |
| A structure's `terrain_radius` (unread by any rule; kept for the motion-entry domain check) | = radius | Half the footprint's smaller side (D-5) |
| Drawn dome, selection ring, pick radius, struck glass, projectile visual endpoints, death shatter, client muzzle | collision radius (= footprint ÷ 2) | `UnitArchetype::drawn_radius()`: for a dome, half the footprint's smaller side, so it looks unchanged; for a unit, the collision radius (D-6) |
| Values changed | — | 45 of 49 structures, all upward; 4 floored, so unchanged (§2.3) |
| Hashed state, checksum schema | — | No new hashed state. Schema stays 29 |
| Expected impact | — | Matrix, battle 33 and `golden_replay_orders` bit-identical: no structures, and the catalog's largest-radius bound only widens a spatial query (D-9). `golden_replay` moves only if a unit engages a hall. The showcase moves only where units engage the enemy hall, most likely at the end, and the hall dies sooner |
| Size | — | M: one required catalog field, 49 data rows, one presentation helper at four sites, one client line, tests, measurement |

---

## 2. What StarCraft II does

Tags:
- **[DATA]** SC2GameData 5.0.14 (`5444e12f13`), layer chain `core → liberty → swarm → void → voidmulti`. `UnitData.xml` is read from the extraction under `~/scratch/combat_wp3_data/<layer>/`. No later layer sets `Radius` or `Footprint` on these structures, except swarmmulti's Spine Crawler footprint (same name). `FootprintData.xml` is read online at `https://raw.githubusercontent.com/SC2Mapster/SC2GameData/5444e12f13/mods/liberty.sc2mod/base.sc2data/GameData/FootprintData.xml` (checked 2026-11-18).
- **[LIVE]** Values read from the running game through the StarCraft II API.
- **[COMM]** Community or editor observation.
- **[INF]** An inference of this plan.

"liberty:4184" means the liberty `UnitData.xml`, line 4184. "Footprint:2015" means `FootprintData.xml`, line 2015.

### 2.1 Sources

| # | Source | What it says |
|---|---|---|
| S1 | `liberty` UnitData, every structure analogue (§2.3) | `Radius` is authored per structure and differs from the footprint's half-width. Examples: Nexus 2 (liberty:4184) and Command Center 2.5 (liberty:10377), both `Footprint5x5Contour`; Engineering Bay 1.25 (liberty:5970) on a 3×3 contour [DATA] |
| S2 | `FootprintData.xml` `<Shape><Radius>`: 5x5Contour 2.6926 (Footprint:2015), 6x5DropOffCreepSourceContour 2.6926 (:2261), 3x3Contour 1.803 (:1935), 3x3CreepContour 1.803 (:2205), 2x2Contour 1.1181 (:1950), 2x2CreepContour 1.1181 (:2073), 2x2IgnoreCreepContour 1.1181 (:1238), 2x2ZergSpineCrawler 1.1181 (:5369), GeyserRoundedBuilt 1.6772 (:959), 1x1 0.7072 (:1106), 2x2Contour2 0.559 (:162) | Each contour footprint has a bounding radius of its collision shape. Example: √(2.5² + 1²) = 2.6926 for the 5×5 octagon [DATA] |
| S3 | BurnySc2/sc2-techtree `data/data.json`, commit `c2c90e9b56` ("Update data.json for patch 5.0.14", 2025-01-02); same values at `edcefaff70`. The generator spawns each unit in a running game (`generate/collect.py:472`, `debug_create_unit`) and records `unit._proto.radius` (`:574`) | The engine's runtime radius. Structures: Command Center, Orbital, Planetary, Nexus, Hatchery, Lair and Hive 2.75; Barracks, Gateway, Engineering Bay, Bunker and every other 3×3 contour building 1.8125; Refinery, Assimilator and Extractor 1.6875; Pylon, Photon Cannon, Shield Battery, Tech Lab, Reactor, Missile Turret, Spine Crawler, Spire and Greater Spire 1.125; Supply Depot 1.25; Dark Shrine 1.5; Spore Crawler 0.875; Sensor Tower 0.75. Units equal their `UnitData` `Radius` (Zealot 0.5, Stalker 0.625, Queen 0.875, Siege Tank 0.875) [LIVE] |
| S4 | BurnySc2/python-sc2 `sc2/unit.py` at `80c18739`: `target_in_range` (lines 559–576), `in_ability_cast_range` (577–599), the `is_placeholder` docstring (≈499) | A weapon reaches when `distance ≤ self.radius + target.radius + range`, with the API radius, structures included. Cast range to a unit target uses the same sum. A Hatchery placeholder reports `radius: 2.75`. The code carries a TODO that immobile units (sieged tank, Planetary Fortress) have "a little lower range than this formula" [COMM] |
| S5 | SC2Mapster wiki, [Data/Weapons/Legacy](https://sc2mapster.wiki.gg/wiki/Data/Weapons/Legacy) | "Use Range To Center — the units radius field won't be added to the weapon range." By default, radii are added: range is edge to edge by radius [COMM] |
| S6 | Contact plan S1 (5.0.13 notes), S2 (SC2Mapster threads) | Units collide with a structure's footprint at their inner radius [DOC, COMM] |
| S7 | Q0057 / combat plan Amendment 7 | Our weapon and scan ranges are edge to edge by both radii, as in the game |

Two searches found no statement that range is measured to the footprint polygon: Liquipedia [Range](https://liquipedia.net/starcraft2/Range), and SC2Mapster [Data/Units](https://sc2mapster.wiki.gg/wiki/Data/Units).

### 2.2 The rule

- **The runtime radius of a structure is `E = max(Radius, ⌈16 · Shape.Radius⌉ ÷ 16)`** [INF from S1 to S3]. All 49 analogues match (table §2.3), including:
  - the ones where `Radius` wins: Supply Depot 1.25 > 1.125, Dark Shrine 1.5, Spore Crawler 0.875 > 0.5625, Sensor Tower 0.75 = 0.75;
  - the ones where the shape wins: everything else.
  - Rounding to 1/8 would give 1.875 for 3×3 contours and fails, so the step is 1/16.
- **Ranges use E, edge to edge** [COMM S4, S5; S7]. This applies to weapon, scan and cast ranges, both from and to a structure.
- **Why the engine needs this** [INF]. Every point of the contour lies within `Shape.Radius ≤ E` of the centre. A unit touching the contour at its inner radius `t ≤ r` is therefore within `E + r` of the centre, so a melee weapon (range ≥ 0) always connects. `Radius` alone would break this. Example: a Zealot at a Nexus's face middle is 2.5 + 0.375 = 2.875 from the centre, but `Radius 2 + 0.5 + 0.1 = 2.6`.
- **Units are unchanged.** Their runtime radius is their `Radius` (S3), as `SC2_UNIT_RADIUS` already pins.
- **Not known, recorded.** S4's note that immobile attackers reach a little less is unsourced in the data. It is not modelled (§10).
- **Harvest, return and build** are interactions with the footprint (our `within_reach`). No source ties them to E. Unchanged.

### 2.3 Values (49 analogues)

"Ours" is our footprint and today's radius. "Floor" is half our footprint's smaller side (D-4). The authored value is `max(E, floor)`.

| Analogue | Ours | `Radius` (liberty) | Game footprint, `Shape.Radius` | E (S3) | Authored |
|---|---|---:|---|---:|---:|
| Command Center | 5×5, 2.5 | 2.5 (10377) | 5x5Contour 2.6926 | 2.75 | 2.75 |
| Orbital Command | 5×5, 2.5 | 2.5 (10523) | 5x5Contour | 2.75 | 2.75 |
| Planetary Fortress | 5×5, 2.5 | 2.5 (10661) | 5x5Contour | 2.75 | 2.75 |
| Nexus | 5×5, 2.5 | 2 (4184) | 5x5Contour | 2.75 | 2.75 |
| Hatchery | 5×5, 2.5 | 2 (8208) | 6x5…Contour 2.6926 (8210) | 2.75 | 2.75 |
| Lair | 5×5, 2.5 | 2 (8290) | 6x5… | 2.75 | 2.75 |
| Hive | 5×5, 2.5 | 2 (8370) | 6x5… | 2.75 | 2.75 |
| Barracks | 3×3, 1.5 | 1.75 (5838) | 3x3Contour 1.803 | 1.8125 | 1.8125 |
| Engineering Bay | 3×3, 1.5 | 1.25 (5970) | 3x3Contour | 1.8125 | 1.8125 |
| Bunker | 3×3, 1.5 | 1.25 (6154) | 3x3Contour | 1.8125 | 1.8125 |
| Factory | 3×3, 1.5 | 1.625 (6238) | 3x3Contour | 1.8125 | 1.8125 |
| Ghost Academy | 3×3, 1.5 | 1.5 (9773) | 3x3Contour | 1.8125 | 1.8125 |
| Starport | 3×3, 1.5 | 1.625 (6371) | 3x3Contour | 1.8125 | 1.8125 |
| Armory | 3×3, 1.5 | 1.25 (6506) | 3x3Contour | 1.8125 | 1.8125 |
| Fusion Core | 3×3, 1.5 | 1.5 (10915) | 3x3Contour | 1.8125 | 1.8125 |
| Gateway | 3×3, 1.5 | 1.75 (4394) | 3x3Contour | 1.8125 | 1.8125 |
| Forge | 3×3, 1.5 | 1.5 (4536) | 3x3Contour | 1.8125 | 1.8125 |
| Cybernetics Core | 3×3, 1.5 | 1.5 (10817) | 3x3Contour | 1.8125 | 1.8125 |
| Twilight Council | 3×3, 1.5 | 1.5 (4604) | 3x3Contour | 1.8125 | 1.8125 |
| Stargate | 3×3, 1.5 | 1.75 (9640) | 3x3Contour | 1.8125 | 1.8125 |
| Robotics Facility | 3×3, 1.5 | 1.5 (9569) | 3x3Contour | 1.8125 | 1.8125 |
| Templar Archives | 3×3, 1.5 | 1.5 (4667) | 3x3Contour | 1.8125 | 1.8125 |
| Robotics Bay | 3×3, 1.5 | 1.5 (9837) | 3x3Contour | 1.8125 | 1.8125 |
| Fleet Beacon | 3×3, 1.5 | 1.5 (9708) | 3x3Contour | 1.8125 | 1.8125 |
| Spawning Pool | 3×3, 1.5 | 1.5 (8886) | 3x3CreepContour 1.803 | 1.8125 | 1.8125 |
| Evolution Chamber | 3×3, 1.5 | 1.5 (8444) | 3x3CreepContour | 1.8125 | 1.8125 |
| Roach Warren | 3×3, 1.5 | 1.5 (11765) | 3x3CreepContour | 1.8125 | 1.8125 |
| Baneling Nest | 3×3, 1.5 | 1.5 (9966) | 3x3CreepContour | 1.8125 | 1.8125 |
| Hydralisk Den | 3×3, 1.5 | 1.5 (8949) | 3x3CreepContour | 1.8125 | 1.8125 |
| Lurker Den | 3×3, 1.5 | 1.5 (676, `LurkerDenMP`) | 3x3CreepContour | 1.8125 | 1.8125 |
| Infestation Pit | 3×3, 1.5 | 1.5 (2077) | 3x3CreepContour | 1.8125 | 1.8125 |
| Nydus Network | 3×3, 1.5 | 1.5 (9899) | 3x3CreepContour | 1.8125 | 1.8125 |
| Ultralisk Cavern | 3×3, 1.5 | 1.5 (9215) | 3x3CreepContour | 1.8125 | 1.8125 |
| Refinery | 3×3, 1.5 | 1.5 (5776) | GeyserRoundedBuilt 1.6772 | 1.6875 | 1.6875 |
| Assimilator | 3×3, 1.5 | 1.5 (4122) | GeyserRoundedBuilt | 1.6875 | 1.6875 |
| Extractor | 3×3, 1.5 | 1.5 (8149) | GeyserRoundedBuilt | 1.6875 | 1.6875 |
| Supply Depot | 2×2, 1 | 1.25 (5663) | 2x2Contour 1.1181 | 1.25 | 1.25 |
| Missile Turret | 2×2, 1 | 0.75 (6033) | 2x2Contour | 1.125 | 1.125 |
| Tech Lab | 2×2, 1 | 1 (6642) | 2x2Contour | 1.125 | 1.125 |
| Reactor | 2×2, 1 | 1 (6579) | 2x2Contour | 1.125 | 1.125 |
| Pylon | 2×2, 1 | 1 (4335) | 2x2Contour | 1.125 | 1.125 |
| Photon Cannon | 2×2, 1 | 1 (11052) | 2x2Contour | 1.125 | 1.125 |
| Shield Battery | 2×2, 1 | 1 (1612) | 2x2Contour | 1.125 | 1.125 |
| Dark Shrine | 2×2, 1 | 1.5 (9510) | 2x2Contour | 1.5 | 1.5 |
| Spine Crawler | 2×2, 1 | 0.875 (8615) | 2x2ZergSpineCrawler 1.1181 | 1.125 | 1.125 |
| **Spore Crawler** | 2×2, 1 | 0.875 (8749) | 2x2Contour2 0.559 | 0.875 | **1 (floor)** |
| **Sensor Tower** | 2×2, 1 | 0.75 (9452) | **1x1** 0.7072 (9454) | 0.75 | **1 (floor)** |
| **Spire** | 3×3, 1.5 | 1 (9011) | **2x2**CreepContour 1.1181 (9013) | 1.125 | **1.5 (floor)** |
| **Greater Spire** | 3×3, 1.5 | 1 (9082) | **2x2**IgnoreCreepContour (9084) | 1.125 | **1.5 (floor)** |

WP0 re-checks each row against `sc2_inspiration` in `structures.ron` and S3's `data.json` before writing the register text.

---

## 3. Diagnosis

The archetype has one radius field. For a structure, `into_archetype` sets `collision_radius = terrain_radius = separation_radius = footprint.0 × 0.5`. Since B-131 (plan `2026-11-18-structure-contact`), a registered structure is no body:
- `MotionEntry::foundation` keeps it out of collision and steering;
- placement and exit occupancy skip it.

So a structure's `collision_radius` is now read only as a range edge and as a drawing size.

Readers, found by `grep -rn "collision_radius\|policy.radius\|\.radius\b"` over `crates/sim_core/src`, `crates/sim_fixtures/src` and `crates/client/src`:

| # | Reader | File, symbol | Kind | After this plan |
|---|---|---|---|---|
| A1 | Weapon reach from here; scan qualification and min range; spatial query bound `largest_radius` (catalog fold) | `target_acquisition.rs` `reaches_from_here`, `scan_candidates`, `acquire_targets` | Range | Engine radius (no code change) |
| A2 | Weapon band: `TargetSnapshot.radius` + attacker radius → `combined_radius` | `combat_resolution.rs` `capture_target_snapshots`, `advance_weapon_cycles`; `combat.rs` `in_engagement_band` | Range | Engine radius |
| A3 | Projectile flight: `eff_dist = dist − r_t − r_a` → `ticks_to_impact` | `combat_resolution.rs` `launch_projectile` | Range (authoritative) | Engine radius |
| A3v | Projectile visual endpoints `spawn_pos_2d`, `impact_pos_2d` | same | Presentation | **`drawn_radius`** (D-6) |
| A4 | Hit radius → splash `offset_by_unit_radius` | `ResolvedHit.radius`, `apply_splash` | Range | Engine radius |
| A5 | Splash distance `point.distance − other.collision_radius` | `strike_area` loop (≈ line 1015) | Range | Engine radius |
| A6 | Projectile retarget radius and live target radius | `projectile_target_radius`, `nearest_projectile_retarget` | Range | Engine radius |
| A7 | Spell missile stop-short radius | `effects/step.rs` `missile_aim` | Range | Engine radius |
| A8 | Cast range to a unit target | `world/casts.rs` `radius_of` | Range | Engine radius |
| A9 | Charge window gap | `world/attack_dash.rs` (`gap = distance − r_a − r_b`) | Range | Engine radius |
| A10 | Pursuit band of an attacker chasing a structure | `movement/locomotion.rs` `pursuit_band` (`MotionPolicy::resolve(target).radius`) | Range | Engine radius |
| A11 | Boarding reach to a carrier structure (Bunker analogue) | `transport/orders.rs` `step_load` | Interaction range (radius sum) | Engine radius |
| A12 | Motion-entry domain check of a foundation | `locomotion.rs` `motion_entry` → `MotionPolicy::contains` (terrain radius) | Domain | Structure `terrain_radius` = half the smaller side (D-5) |
| P1 | Snapshot radius (dome scale, selection ring, picking, struck glass, stance bindings) | `world/snapshot.rs` | Presentation | **`drawn_radius`** |
| P2 | Death shatter radius | `production/lifecycle.rs` `prepare_reap_with_resources` → `transport/lifecycle.rs` `shatter` | Presentation | **`drawn_radius`** |
| P3 | Client muzzle offset | `client/src/effects/catalog/units.rs` `resolve_unit` (`muzzle`) | Presentation | **`drawn_radius`** |
| P4 | Client projectile re-aim assumes "a bubble's radius is its collision radius" | `client/src/effects/struck.rs` `projectile_onto_struck` doc | Presentation | Holds again through A3v and P1 (both drawn); doc reworded |
| U1 | Harvest, return, build reach | `economy/mod.rs` `within_reach` (footprint gap + worker radius) | Footprint | Unchanged |
| U2 | Mobile-producer exit box; product radii | `production/exits.rs`, `production/mod.rs` | Units only (domes use the footprint) | Unchanged |
| U3 | Tooling: blocking trace edge gap, trace band | `sim_fixtures/src/trace/blocking.rs`, `trace/mod.rs` `within_band` | Tooling | Follows the field; recorded (D-8) |

**Hazard found (A12).** `motion_entry` runs `policy.contains` for every entry, foundations included. A 5×5 placed flush with the map edge has its centre 2.5 from the border. A structure terrain radius of 2.75 would raise `"actor is outside its current terrain-radius movement domain"`. D-5 removes it.

**Hazard found (melee liveness).** A floor-less authored value below the footprint's half-width would make some structures unhittable by melee:
- Sensor Tower analogue at 0.75 on our 2×2: a Zealot's best centre distance is 1 + 0.375 = 1.375 > 0.75 + 0.5 + 0.1;
- Spire analogues at 1.125 on 3×3;
- Spore analogue at 0.875 against Zerglings.

D-4's floor prevents it.

---

## 4. Design decisions

### D-1. One radius, the engine's, authored per structure

- **Authored** (`StructureArchetypeDef`, `crates/sim_core/src/catalog.rs`): `pub radius: f64`, **required** (no `serde(default)`), next to `footprint`.
  - Precedent: `start_energy` and `attack_target_priority` ("required, so a catalog written before the field fails to load").
  - Why required: 45 of 49 rows differ from any footprint formula, so a default would be silently wrong.
- **Resolved:** `collision_radius = SimScalar::from_num(radius)` and `separation_radius = collision_radius` (unread, B-49). `terrain_radius` is per D-5. The line `let radius = … footprint.0 * 0.5` is removed.
- **Name:** `radius` is the game's word. Not `collision_radius`: a structure is no body (B-131), so "collision" would mislead.
- **One field, every range reader.** S4 and S5 show weapon, scan and cast ranges all use the same runtime radius. No reader is split, and none branches on structure-ness.
- **The `UnitArchetype::collision_radius` doc** gains: "For a structure, which is no body (B-131), this is only the edge its ranges measure to: the 5.0.14 engine radius, authored as `radius` in `structures.ron` (plan `2026-11-18-structure-radius`)".
- **The `///` doc on `radius`** states: world units; the rule `max(Radius, ⌈16·Shape.Radius⌉/16)`; the floor; the readers (A1 to A11); the non-readers (U1, drawing).

### D-2. Loader validation (`catalog/validation.rs`, `StructureArchetypeDef::validate`, new `validate_radius`)

Each failure is a typed `invalid(slug, "radius", reason)`:

1. `scalar` (finite) and strictly positive.
2. **A multiple of 1/16:** "must be a multiple of 1/16 world unit". Every engine radius is (§2.2), and the value stays exact in Q32.32 and on the mesh's Q16 lattice.
3. **At least half the footprint's smaller side:** "must be at least half the footprint's smaller side, so a unit touching a face is within reach" (D-4).

A missing field is a RON parse error that names `radius`. **Verify** the error variant and assert on the field name, as the `start_energy` test does.

### D-3. What stays on the footprint

- Harvest, return and build reach (U1).
- Contact and routes (B-131, B-116).
- Placement, exits and construction sites.

The game's gather and build interactions are not sourced to E. Our footprint rule already ties them to where the worker stands.

### D-4. The floor, and the four analogues it touches

**Rule.** `radius ≥ min(w, h) ÷ 2`. A unit touching a face middle stands `h + t` from the centre. With `t ≤ r` and `R ≥ h`, it is within `R + r + range` for any range ≥ 0. That is the game's guarantee (§2.2) on our square footprints. The square corners stay outside melee reach for large footprints (5×5: lateral band ±1.72 on a face for a Zealot, against ±1.16 today). The game's contours cut those corners; that is B-134's subject.

**Four analogues** take the floor, because our footprint is larger than the game's collision shape:

| Analogue | Footprint, ours vs game | E | Authored | Difference |
|---|---|---:|---:|---:|
| Sensor Tower | 2×2 vs `Footprint1x1` | 0.75 | 1 | 0.25 |
| Spore Crawler | 2×2 vs `2x2Contour2`, shape ±0.5 | 0.875 | 1 | 0.125 |
| Spire | 3×3 vs 2×2 | 1.125 | 1.5 | 0.375 |
| Greater Spire | 3×3 vs 2×2 | 1.125 | 1.5 | 0.375 |

Ranges to and from these are longer than the game's by that amount. This is registered as **B-138** with the footprint mismatches (and the Hatchery family's 5×5 against 6×5, where E needs no floor). It is fixed with the footprints, not here. Changing footprints changes placement, the navigation mesh cut and media framing (B-134's plan).

**Rejected:**
- (a) Author E and accept unkillable structures: a liveness defect.
- (b) Change the four footprints here: scope (§10).
- (c) Use the half-diagonal of our squares for everything: 3.54 for a 5×5 overshoots the game by 0.79 for every ranged unit.

### D-5. A structure's terrain radius

`terrain_radius = min(w, h) ÷ 2`, which is at most `radius` by D-2.3, so `MotionPolicy::resolve`'s `0 < t ≤ r` holds.
- No rule reads it: a structure never moves, and units meet its footprint (B-131).
- It exists so that `motion_entry`'s per-tick domain check holds for a footprint inside the map (A12).
- The doc on the resolved field and in `into_archetype` says so.

Alternative rejected: skipping the domain check for foundations. That changes movement code for no gain and loses a corruption guard.

### D-6. The drawn dome stays the footprint's size

`UnitArchetype::drawn_radius(&self) -> SimScalar` (new, `catalog.rs`, `#[must_use]`):
- `ShapeKind::BuildingDome { w, h }` → `min(w, h) ÷ 2`;
- `UnitSphere` → `collision_radius`.

It is used by:
- P1, the snapshot `radius` (and with it dome scale, selection ring, picking, struck glass, stance bindings);
- A3v, the projectile visual's spawn and impact points, which are presentation only (the event is not hashed);
- P2, the shatter radius;
- P3, the client muzzle.

Reasons:
- **What units meet is the footprint (B-131).** Drawn at E (2.75 for a 5×5), a hall's dome would cover 0.25 beyond each face middle. A worker resting at `t` = 0.3125 from the face would sit with its bubble's centre 0.0625 inside the dome.
- **The engine radius is not a drawing size.** The game draws a model scaled by its actor. Its selection circle and minimap circle are separate fields (Nexus `MinimapRadius` 2.5 against `Radius` 2, liberty:4192).
- **Consistency of the client's re-aim.** `projectile_onto_struck` rebuilds the target centre from `arrival + direction · struck.radius`. Publishing the arrival at the *drawn* radius keeps that exact. Publishing it at E with a drawn snapshot would misplace hits on domes by `E − h`.
- **Withdrawn claim.** The B-135 text "and the drawn dome measure[s] to the wrong edge" is withdrawn in WP0. It is not a deviation.

The muzzle of an armed structure (cannon, turret, crawlers, Planetary Fortress) stays at its dome's rim. A spell-impact contact point (A7) and a splash centre (A4) on a structure follow E. They may land up to 0.25 outside a face middle; WP5's picture check records this (R-7).

### D-7. Mobile units unchanged

`drawn_radius` returns `collision_radius` for every unit, so the snapshot and every unit visual are bit-identical. Unit radii are unchanged (S3 confirms they equal `Radius`).

### D-8. Tooling

- `sim_fixtures` `blocking.rs` `edge_gap` and `trace/mod.rs` `within_band` read `collision_radius`. They follow the field, so the trace's bands now match the simulation's.
- The blocking trace's structure gap gets smaller, so units near a structure still read as touching (gap ≤ 0). Recorded, not changed.

### D-9. Determinism

- **No floats in state.** `radius` is converted once at load (`SimScalar::from_num(f64)`), exactly, being a multiple of 1/16.
- **No new hashed state.** The catalog and `MotionPolicy` are not hashed; `CHECKSUM_SCHEMA_VERSION` stays 29.
- **WP1 to WP3 are pure.** Every authored value equals today's `footprint.0 ÷ 2`, and `drawn_radius` equals it for every shipped (square) dome. So everything is bit-identical, including showcase `units.csv` and both goldens.
- **WP4 moves only structure worlds.** The catalog bound `CatalogBounds::largest_radius` (target acquisition) rises from 2.5 to 2.75 in every world. It only widens `query_radius_unordered`. Candidates are then filtered by exact distance and ranked by a key that ends in the entity id ("visiting order cannot change the winner"). So a world without structures is bit-identical: the matrix, battle 33 and `golden_replay_orders` (`structures: []`). Any difference there is a defect.

### D-10. Measurement

`<base>` is the tree before the WP and `<cand>` the WP's tree.
- Release tools are built per tree into `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` and copied aside (charter §7).
- Large output goes to `~/scratch/structure-radius/`.

| What | Command | WP1 to WP3 | WP4 (data) |
|---|---|---|---|
| Matrix | `matchup_trace --all-battles --seeds 16 --jobs 8 --out <dir>/matrix --compare <base>/matrix/matrix.csv` | Bit-identical | Bit-identical (D-9) |
| Battle 33 | `matchup_trace --battle 33_24_marines_vs_12_zealots --seeds 64 --no-unit-csv --out <dir>/b33`; `diff` of `seeds.csv` | Byte-identical | Byte-identical |
| Showcase | `route_trace --scenario scenarios/showcase.ron --ticks 1800 --out <dir>/route`, to the match's end, and seeds 1 to 8 with `--seed` | `units.csv` byte-identical | D-11, 4 |
| Overlap meter, reversals, give-ups | `route_trace` | Unchanged | 0 deeper unit-ticks; bounds of D-11, 4 |
| Goldens | `golden_replay_tests` native debug and release; `--target wasm32-wasip1 --release` under wasmtime; wasm debug once in WP4 | Pass unchanged | `golden_replay_orders` unchanged; `golden_replay` per D-11, 5 |
| Throughput | `flock ~/scratch/gate.lock cargo bench -p sim_core --bench throughput`, base and candidate alternated, three rounds | Within noise | Within noise (the scan query widens by 0.25; benches spawn no structures); every scenario ≥ 25 ticks/s |

### D-11. Stop rules and acceptance

General rule, as in the contact, inner-radius and structure-contact plans: **a winner flip anywhere stops the WP.** The agent commits nothing beyond the WP and reports the trace: first divergent tick, attacker, structure, and which reader (A1 to A11). A movement that is not a flip is explained before the WP proceeds.

1. **WP1 to WP3 are bit-identical** on everything in D-10. Any difference is a wiring defect: stop, find it, never re-record.
2. **Structure-free worlds are bit-identical in WP4** (matrix, battle 33, `golden_replay_orders`). A difference is a defect (D-9): stop.
3. **Control run.** For each moved structure world in WP4 (showcase, `golden_replay`), the agent runs the candidate with the CC-analogue row alone reverted to 2.5 (a scratch build, not committed). It also traces the first tick at which a reader first differs. The report names the reader.
4. **Showcase.**
   - **Base:** the WP4 base tree's own run. Structure-contact Amendment 1 gives player 1 winning at tick 2,193 on the default seed. If the base differs, its own run is the reference, and the evidence says so.
   - **Expected direction:** the first divergence is the first tick a unit scans, aims at, chases, or shoots a projectile at an enemy hall, now 0.25 further out. That is after the lane fight. The hall then dies sooner, so the end tick is earlier or equal on every seed that ends with a hall kill. Divergence before the armies meet is a finding and stops the WP.
   - **Acceptance:**
     - the same winner on the default seed;
     - 0 deeper unit-ticks;
     - stall give-ups ≤ base + 1;
     - reversals ≤ base + 10 per owner;
     - the match ends;
     - seeds 1 to 8 reported with winners and end ticks.
   - **If the default-seed winner flips:** stop and trace from the first divergence. Classify the cause:
     - **rule:** an engagement now begins at E (S3, S4) and decides the end;
     - **defect:** a pursuit that never reaches its band, a unit oscillating at a face, or a structure hit from beyond `E + r + range`.

     The orchestrator decides by amendment. A rule-caused flip has precedent (three earlier amendments); no constant is tuned.
   - **If the default-seed timeline moves at all,** WP5 re-checks the storyboard.
5. **`golden_replay`** (halls at cells (6, 6) and (52, 52), CC analogue 2.5 → 2.75).
   - **If it moves,** the re-recorded header names the first changed checkpoint and the traced cause, in the form: "Re-recorded for plan 2026-11-18-structure-radius WP4 (B-135): checkpoint N first changed; entity E at tick T scans (aims at, chases, shoots at) the hall at cells (…), whose radius is now its 5.0.14 engine radius 2.75". It must pass in debug and release native, in `wasm32-wasip1` release under wasmtime, and once in wasm debug.
   - **If it does not move,** that is a finding (no unit engages a hall). Record it.
6. **Test edits.** A changed test is re-derived from a trace and stated in the commit; no bound is loosened. A change not explained by §3's table is a finding and stops the WP.
7. **No tuning constant** is invented. If acceptance fails after the rule is shown correct by its tests, the WP stops and reports.

---

## 5. File ownership

| Area | Files | WP |
|---|---|---|
| Plan row, register | `docs/plans/README.md`; `DESIGN.md` §4 (B-135 restated, B-138 added) | WP0 |
| Catalog model and loader | `crates/sim_core/src/catalog.rs` (`StructureArchetypeDef::radius`, `into_archetype`, `UnitArchetype` docs, `drawn_radius`), `crates/sim_core/src/catalog/validation.rs` | WP1 |
| Data (today's values, then 5.0.14) | `assets/catalog/structures.ron` (49 rows) | WP1, WP4 |
| Inline structure RON in tests, if any | **verify**: `grep -rn "footprint: (" crates --include=*.rs` (catalog-shaped hits only; map files are unrelated), `catalog_serialization_compatibility.rs` | WP1 |
| Simulation presentation sites | `crates/sim_core/src/world/snapshot.rs`, `crates/sim_core/src/combat_resolution.rs` (`launch_projectile` visual), `crates/sim_core/src/production/lifecycle.rs` (shatter radius) | WP2 |
| Behavioural tests | `crates/sim_core/tests/structure_radius_tests.rs` (new; `fast` by default, so `crates/gate/suites.ron` needs a rule only if it measures `slow`, C22); unit tests in `snapshot.rs`, `combat_resolution.rs`, `casts.rs`, `effects/step.rs`, `catalog.rs` | WP1, WP2 |
| Client | `crates/client/src/effects/catalog/units.rs` (`muzzle`), `crates/client/src/effects/struck.rs` (doc of `projectile_onto_struck`) | WP3 |
| Reference table | `crates/sim_core/tests/catalog_reference_values_tests.rs` | WP4 |
| Goldens | `crates/sim_core/tests/fixtures/golden_replay.ron` (only if it moves) | WP4 |
| Showcase | `assets/captures/showcase.ron` (only if the timeline moved) | WP5 |
| Documents | `DESIGN.md` (Combat row, register, §5 if figures move), this plan, `docs/plans/README.md`, `docs/evidence/2026-11-18-structure-radius.md` (new), pointer amendment in `2026-11-18-structure-contact.md` | WP0 to WP6 |

---

## 6. Work packages (commit order)

- **Branch:** `structure-radius`.
- **Tests first:** each package writes its tests first and shows them red on its base, except WP0 and guards that are marked as such.
- **Gates:** implementation agents hand back after `cargo gate quick` and the tests their change needs. WP1 to WP4 form one simulation package, which a verifier runs through `cargo gate package` together (Q0061).
- **Evidence:** WP1 creates `docs/evidence/2026-11-18-structure-radius.md` with "How to reproduce" (D-10); each WP appends to it.
- **Sizes:** S ≤ 1 day, M 2 to 3 days.

### WP0: records (S, docs only)

- This plan's row in `docs/plans/README.md` (`Proposed`).
- **`DESIGN.md` B-135 restated** (open until WP6):

  > A structure's collision radius is half its footprint width, not its analogue's 5.0.14 engine radius `max(Radius, ⌈16·Shape.Radius⌉/16)` (live values: Command Center, Nexus and Hatchery families 2.75, 3×3 contour buildings 1.8125, geysers 1.6875, 2×2 contour buildings 1.125, Supply Depot 1.25, Dark Shrine 1.5). So weapon, scan and cast ranges, projectile and missile arrival, splash, the Charge window, boarding reach and the pursuit band to and from a structure measure to the wrong edge.

  It points to this plan. The earlier sentences about the dome and "Command Center analogues match at 2.5" are removed (D-6, §2.3). The footprint sentences move to B-138.
- **New row B-138** (open; package: with B-134's `sim_map` plan):

  > Footprints differ from 5.0.14 in size or collision shape: the Sensor Tower analogue is 2×2 against `Footprint1x1` (liberty:9454); the Spire and Greater Spire analogues 3×3 against 2×2 (liberty:9013, 9084); the Spore Crawler analogue's collision is its 2×2 cells against `Footprint2x2Contour2`'s ±0.5 shape (Footprint:145–162); the Hatchery, Lair and Hive analogues 5×5 against 6×5 (liberty:8210, 8292, 8372). For the first four, the structure radius is floored at half the footprint (plan `2026-11-18-structure-radius` D-4), 0.125 to 0.375 above the game's, so ranges to and from them are that much longer.
- WP0 checks every §2.3 row against `sc2_inspiration` and S3's `data.json` before writing. A mismatch is corrected in this plan by amendment.
- **No question file:** Q0051 decides the rule; D-4 is a recorded scoping decision.
- **Gate:** docs-only `package` (fmt, `workspace_checks`, C17 and C18).

### WP1: the field and the loader, data unchanged (S)

- **Tests first** (`catalog/validation.rs` unit tests over a minimal structure RON built from the shipped file, as `units_with_terrain_radius` does for units). On the base the field is ignored or absent, so they are red:
  - `a_structure_radius_is_read_into_the_collision_radius`: 1.8125 on a 3×3 → `collision_radius` 1.8125, `separation_radius` 1.8125;
  - `a_structures_terrain_radius_is_half_its_footprints_smaller_side`: 2.75 on 5×5 → `terrain_radius` 2.5 (D-5);
  - `a_structure_radius_is_required`: the row without `radius` fails to load, naming `radius`;
  - `a_structure_radius_must_be_positive_and_finite`: 0, −0.5, NaN;
  - `a_structure_radius_must_be_a_multiple_of_a_sixteenth`: 1.8 refused, 1.8125 admitted;
  - `a_structure_radius_may_not_be_below_half_the_footprint`: 1.4375 on 3×3 refused, 1.5 admitted; 0.75 on 2×2 refused;
  - the shipped catalogs load (existing `runtime_catalog_tests`, `asset_baker` `runtime_catalog_tests`).
- **Change.** D-1 and D-2, plus the `UnitArchetype` docs.
  - `structures.ron` gets `radius: <footprint.0 ÷ 2>` on all 49 rows, with the comment `// Replaced by the 5.0.14 engine radius in plan 2026-11-18-structure-radius WP4.`
  - Struct-literal or inline RON structures in tests are updated (**verify** the list).
- **Exit:** everything bit-identical (D-11, 1).

### WP2: simulation readers and presentation split (M)

- **Tests first** (`crates/sim_core/tests/structure_radius_tests.rs` unless noted).
  - **Setup:** synthetic flat maps, and a scratch catalog that changes one shipped structure's `radius` in its RON. The structure is chosen by `ShapeKind::BuildingDome` and footprint size; armed structures by weapon presence; transports by `transport_capacity > 0`; never by slug (C5).
  - **Control:** each test runs the same setup at `radius = h` (today) and at the larger value, and asserts the difference.
  - **Diagnostics:** every tick asserts none.
  - Items marked *guard* pass on WP1 (the readers already read the field) and pin the wiring. Items marked *red* fail on WP1.
  - R-1 *guard* `a_weapon_reaches_a_structure_at_its_radius`: a ranged ground archetype holding position with an explicit attack on a 3×3 at `R` = 1.8125. At centre distance `range + r + R − 2⁻¹⁶` it begins a swing within the weapon's period; at `+ 2⁻¹⁶` it never does. At `R` = 1.5 the first distance is out of range.
  - R-2 *guard* `a_structure_is_scanned_at_its_radius`: an idle armed unit acquires the structure at `scan + r + R − ε`, not at `+ ε` (no other targets).
  - R-3 *guard* `an_armed_structure_reaches_at_its_own_radius`: a 2×2 armed structure at `R` = 1.125 fires at a unit at `range + 1.125 + r − ε` and not at `+ ε`.
  - R-4 *guard* `a_melee_attacker_strikes_a_structure_along_its_face`: a melee ground archetype with `t < r`, attack-ordered from a seeded side onto a 3×3 at `R` = 1.8125, deals damage within 100 ticks of contact from lateral offsets 0, ±1.0 and ±1.4 of a face. At `R` = 1.5 the ±1.4 start fails to deal damage in that window (it slides toward the middle first). Pins the pursuit band (A10).
  - R-5 *guard* `every_shipped_structure_face_is_in_melee_reach`: for every shipped structure and every shipped ground melee weapon, `min(w,h)/2 + t ≤ R + r + range` (the D-4 floor on the data).
  - R-6 *guard* `a_projectile_to_a_structure_flies_to_its_radius`: `ticks_to_impact` equals `⌈max(dist − R − r, MIN_FLIGHT_DISTANCE) ÷ speed⌉` with the larger `R` (A3).
  - R-7 *guard* `splash_reaches_a_structure_by_its_radius`: a splash weapon's area at `R − ε` beyond the zone edge strikes the structure; at `R + ε` it does not (A5).
  - R-8 *guard* `a_bunker_boards_at_its_radius`: a passenger stops and loads at `R + r + BOARDING_REACH` (A11).
  - R-9 *guard* `a_charge_window_to_a_structure_uses_its_radius` (A9).
  - R-10 *guard* (unit, `world/casts.rs`) `a_cast_on_a_structure_is_in_range_at_its_radius`. Use a shipped unit-target effect only if one may target a structure (**verify**); otherwise a scratch ability in the test catalog.
  - R-11 *guard* (unit, `effects/step.rs`) `a_missile_stops_short_of_a_structure_by_its_radius` (A7), with the same **verify**.
  - R-12 *red* `a_structure_flush_with_the_map_edge_keeps_a_valid_motion_entry`: a 5×5 with `radius` 2.75 registered at cells (0, 0) and stepped 25 ticks with a unit ordered past it. No diagnostic, no `MovementInvariant`. Red if `terrain_radius = radius`. WP1 already sets D-5, so this test is written in WP1 if the order is kept. Record which.
  - R-13 *red* (unit, `world/snapshot.rs`) `a_dome_is_drawn_at_its_footprint_not_its_radius`: a 3×3 at `R` = 1.8125 publishes `radius` 1.5; a unit publishes its collision radius.
  - R-14 *red* (unit, `combat_resolution.rs`) `a_projectile_visual_to_a_dome_arrives_on_its_drawn_dome`: `pos + vel · duration` lies `drawn_radius` from the target centre towards the shooter (to 1e-4), while `ticks_to_impact` uses `R`.
  - R-15 *red* (unit, `production/lifecycle.rs` or `transport/lifecycle.rs`) `a_dying_structure_shatters_at_its_drawn_radius`.
  - R-16 (unit, `catalog.rs`) `drawn_radius_is_half_the_footprints_smaller_side_for_a_dome_and_the_radius_for_a_unit`.
- **Change:** `drawn_radius` at P1, A3v and P2 (D-6), with docs. No range reader changes; the audit shows why.
- **Audit in the commit message:** the output of `grep -rn "collision_radius\|policy.radius\|\.radius\b" crates/sim_core/src crates/sim_fixtures/src`, with each production hit classified as range (A1 to A11), domain (A12), presentation (P1, P2, A3v), footprint/units (U1, U2) or tooling (U3), with the reason.
- **Exit:** D-11, 1. Everything bit-identical, since `drawn_radius` equals today's value for every shipped dome.

### WP3: client (S)

- **Test first:** `effects::catalog::units` unit test `a_structures_muzzle_is_its_drawn_dome`. A scratch 2×2 armed structure with `radius` 1.125 has `muzzle` 1.0; a unit's muzzle is its collision radius. Red on WP2.
- **Change:**
  - `muzzle: archetype.drawn_radius().to_f32()`;
  - the doc of `projectile_onto_struck` now says the simulation publishes the arrival at the target's *drawn* radius (D-6);
  - `StruckBubble` docs follow.

  The client reads no other structure radius from the catalog. **Verify** with `grep -rn "collision_radius" crates/client/src`; the only other hit is the struck doc.
- **Client tests:** selected by the `crates/sim_core/**` and catalog rules; expected unchanged (bit-identical worlds). `weapon_battle_gpu_tests` spot clearance reads structure `collision_radius`, which is test-only and correct either way.
- **Exit:** bit-identical. `sync_allocation_count_tests` and `sync_steady_state_tests` pass (no new allocation; the method is a match on the shape).

### WP4: the 5.0.14 values (M)

- **Tests first** (`catalog_reference_values_tests.rs`), red on WP3:
  - `b135_structure_radii_follow_starcraft`: `SC2_STRUCTURE_RADIUS: [(&str, [f32; 1], &str); 49]`, the §2.3 "Authored" column by analogue. Sources look like `"liberty UnitData:5838 Radius 1.75; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)"`. The four floored rows cite "floor, D-4; B-138". Checked with `check_table("structure radius", true, …, &[], |a| [f(a.collision_radius)])`.
  - `structure_radii_follow_the_engine_rule`: a second table `SC2_STRUCTURE_ENGINE_RADIUS: [(&str, [f32; 3], &str); 49]` of `[Radius, Shape.Radius, E]`. It asserts `E == max(Radius, ceil(16·Shape)/16)` for every row, and that the authored value equals `max(E, min(w,h)/2)` read from the archetype's shape. This pins the rule, the floor and the data together; no unsourced value can be added.
- **Change:** the 49 `radius` values of §2.3 in `structures.ron`, each with a comment, e.g. `// 5.0.14 engine radius 1.8125 = max(Radius 1.75, liberty UnitData:5838; Footprint3x3Contour Shape 1.803, Footprint:1935).` The four floored rows say `// floor: half the footprint (B-138); the game's 0.75 on Footprint1x1.`
- **Measure:** D-10, WP4 column, including D-11, 3's control run.
- **Stop rules:** D-11, 2 to 7.
- **Goldens:** D-11, 5.
- **Expected test edits.** Each must be re-derived and stated; none is certain (**verify** by running):
  - tests that attack or scan a shipped structure at pinned distances: `combat_correctness_tests`, `target_selection_tests`, `stutter_step_tests` if they use domes, `attack_dash_tests`, `load_orders_tests`, `transport_ownership_tests` (Bunker analogue at 1.8125);
  - `full_match_tests`, `match_result_tests` end ticks;
  - `structure_contact_tests` and `production_transactions_tests` should not change (footprint paths).
  - `effect_snapshot_tests` lines 326 and 800 compute distances from `collision_radius`; check whether a structure is involved.
  - A test whose change §3 does not explain stops the WP.

### WP5: presentation check and evidence (S)

- **Picture check:** one capture on the RX 580 under `with-gpu-lock` (charter §7, `showcase render --driver radv` or the lab), at least 60 s between holds (readback plan Amendment 4). It shows:
  - a cannon-analogue shot and a unit's projectile striking a 3×3 dome;
  - a melee unit at a dome's face.

  Expected: the dome looks unchanged; projectiles arrive on its glass (D-6); a splash or spell impact on a structure may sit up to `E − h` off the dome at a face middle. This is recorded as an observation. If it reads as a fault, it is registered (next free number) and left out of scope.
- **Storyboard:** if WP4 moved the default-seed timeline, re-check `assets/captures/showcase.ron` with a real render, one GPU lock per take, and adjust scene ticks and captions as earlier plans did. `client_diagnostics/showcase_storyboard_tests` and `showcase_tests` must pass.
- **Throughput** (D-10) under the gate lock. `DESIGN.md` §5 is updated only if the figures move beyond noise.
- The evidence file gets the §2.3 table with S3's raw values and the control-run trace.

### WP6: bookkeeping (S)

- **Register:** B-135 removed as fixed, with commits. B-138 stays open (with B-134).
- **`DESIGN.md` Combat row** gains: "ranges to and from a structure (weapon, scan, cast, projectile, missile, splash, Charge, boarding) measure to its 5.0.14 engine radius, authored as `radius` in `structures.ron`, while its dome is drawn at its footprint (B-135 fixed, plan `2026-11-18-structure-radius`)", with `structure_radius_tests.rs` added to the row's tests.
- **Pointers:** a one-line amendment in `2026-11-18-structure-contact.md` (after "As built"). It corrects its §2.2 bullet "The `Radius` remains what the game measures ranges to" to the engine radius of this plan's §2.2.
- This plan's row becomes `Done`, with commits and the evidence link.
- **Gate:** `merge` by the orchestrator.

---

## 7. Test plan (summary)

| Layer | What it proves | Where |
|---|---|---|
| Loader | Field read, required, refused when ≤ 0, non-finite, off the 1/16 lattice, or below the floor; terrain radius per D-5 | `catalog/validation.rs` (WP1) |
| Ranges | Weapon, scan, own reach of armed structures, melee along a face, projectile flight, splash, boarding, Charge, cast, missile, all at `R` | R-1 to R-4, R-6 to R-11 |
| Liveness | Every face of every shipped structure is in melee reach of every shipped melee unit | R-5 (data property) |
| Domain | A flush 5×5 with `R` > half-width keeps a valid motion entry | R-12 |
| Presentation | Dome, projectile visual, shatter and muzzle at the drawn radius; units unchanged | R-13 to R-16, WP3 unit test, WP5 picture |
| Data | Every analogue equals `max(E, floor)`; E follows the engine rule; no unsourced row | `b135_structure_radii_follow_starcraft`, `structure_radii_follow_the_engine_rule` |
| Determinism | WP1 to WP3 bit-identical; structure-free worlds bit-identical in WP4; moved worlds traced with a control run; three targets | D-10, D-11 |
| Charter | C4, C5 (structures by shape, capability; tables by analogue), C7, C17, C18, C22 | `workspace_checks`, `suite_catalog_tests` |

---

## 8. Determinism summary

- **Exact fixed point.** Every authored value is a multiple of 1/16, enforced by the loader, and converted once at load. There are no floats in state. `drawn_radius` feeds only presentation outputs (snapshot, events), which are allowed under C7.
- **No new hashed state.** No new collections; the checksum schema stays 29. `sim_map` is untouched, so its digests are unchanged.
- **Wiring is proven pure** by bit-identity of WP1 to WP3 on the goldens, matrix, battle 33 and showcase. The data's effect on structure-free worlds is proven nil in WP4.
- **Re-recording.** `golden_replay` is re-recorded only with a traced first divergence and the control run. It is replayed in debug and release on Linux x86_64 and on `wasm32-wasip1` (release as CI does, debug once locally). CI covers Windows and macOS ARM64 on the push.

### Throughput

- Bench scenarios spawn no structures (no `spawn_structure` under `crates/sim_core/benches`).
- The only change they see is the catalog's largest radius, 2.5 → 2.75, which widens each scan's spatial query by 0.25: a few more neighbours visited, then filtered.
- Measured in WP2 (no change expected) and WP4/WP5, base and candidate alternated, three rounds under the gate lock. Every scenario must stay ≥ 25 ticks/s.

### Charter implications

- **C5:** no slug or identity in Rust. The reference tables name analogues. Tests choose structures by `ShapeKind::BuildingDome` and footprint, armed ones by weapon presence, carriers by `transport_capacity`.
- **C7:** `radius` is authored `f64` converted at load. `drawn_radius` is fixed point, and becomes `f32` only in the snapshot, event and client.
- **C4:** docs of `catalog.rs` (field docs), `snapshot.rs`, `struck.rs`.
- **C11:** no new PERFORMANCE RATIONALE.
- **C12:** no `let _ =`.
- **C22:** the new test file is `fast` unless measured slow.
- **C17:** `DESIGN.md` links stay true.

---

## 9. Risks

| # | Risk | Mitigation |
|---|---|---|
| R-1 | The showcase's near-tied fight flips | Expected engagement only at the enemy hall, late. D-11, 4: stop, trace, control run, amendment; S3 and S4 are the cited reasons for a rule-caused flip |
| R-2 | `golden_replay` moves | Traced header with first checkpoint, entity, hall and reader; `golden_replay_orders` must not move |
| R-3 | §2.2's rule is an inference that a later source contradicts (e.g. ranges measured to the contour polygon) | All 49 live values fit the rule; S4 uses the radius sum; the melee argument requires E ≥ contour reach. Should a source show polygon measurement, a later plan replaces the radius with a footprint distance; the field and tables still hold the game's E |
| R-4 | A structure becomes unhittable by melee | D-4 floor, the loader rule, R-4 and R-5 |
| R-5 | A foundation at the map edge trips the domain check | D-5, R-12 |
| R-6 | Client hits misplaced on domes | D-6: visual endpoints at the drawn radius; R-14; WP5 picture |
| R-7 | Splash centres and spell-impact contacts on structures sit off the dome by up to `E − h` | Game-correct (E); recorded in WP5, registered only if it reads as a fault |
| R-8 | Armed structures out-range units by 0.125 to 0.25 more than today (cannon, turret, spine, PF), changing skirmish defence | The game's values; skirmish outcomes are not pinned; trace if a pinned test moves |
| R-9 | The four floored analogues stay longer-ranged than the game | Recorded as B-138 with the footprint fix |
| R-10 | Hidden readers of the structure's radius (a future reader that expects a body size) | WP2 audit grep classifies every hit; the `collision_radius` doc says what it means for structures |
| R-11 | S4's note that immobile attackers reach slightly less | Unsourced in data; §10 |
| R-12 | Throughput | One wider query bound; measured |

---

## 10. Out of scope and decisions recorded (Q0002)

- **B-134, contour footprints**, and **B-138, footprint size and shape mismatches** (Sensor Tower 1×1, Spire and Greater Spire 2×2, Spore Crawler's small shape, Hatchery family 6×5): `sim_map` navigation cut and placement layers. Their fix removes D-4's four floors.
- **Measuring ranges to the footprint polygon** instead of a radius: not sourced (R-3).
- **S4's "immobile units have a little lower range"**: unsourced, not modelled.
- **Harvest, return and build reach**: stay on the footprint (D-3).
- **Lifted structures** (flying Barracks and others): not modelled.
- **`separation_radius` (B-49)**: set equal to `radius`, still unread.
- **Placing spell-impact and splash visuals on the dome's glass** (R-7): presentation, registered only if it reads as a fault.
