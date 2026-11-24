# Blueprint: Vision and Detection — Lost Missiles, the Attack Reveal, Detection Radius and Sight-Blocker Edges (B-54 to B-57)

This plan settles the four rows that the deviation-fixes plan grouped as its WP14 vision half (B-54 to B-57), against
StarCraft II 5.0.14:

- **B-54: a missile whose target is lost flies on to the target's last position.** Every attack missile keeps the
  `CEffectLaunchMissile` default `PointFallback 1` (`core` EffectData:97). When its target fails the weapon's filters
  in flight, because it dies, is loaded, turns invulnerable or cloaks undetected, the missile goes on to the point and
  lands there. A single-target weapon then strikes nobody, as ours does. A splash weapon still strikes its area around
  the point, which ours does not, because our missile vanishes (`ProjectileTargetLossBehavior::Fizzle`).
- **B-55: an air attacker hidden from its victim is revealed to it.** A damage effect's `Visibility` defaults to
  `Snapshot` (`core` EffectData:62). Every damage effect of a unit on the air plane (air units and the Colossus)
  sets it to `Visible`. The struck player can then target the attacker for 3 game seconds (`AttackReveal`, `core`
  BehaviorData:237–240). A ground attacker on high ground or in fog is shown only as a snapshot and cannot be
  targeted, which our simulation already matches. Ours never reveals an air attacker either.
- **B-56: detection has its own radius.** Each detector's `Detect` comes from a `Detector11`-style behaviour, not
  from its `Sight`. For every modelled detector the two values are equal (11), so no world moves. The model still
  needs the field, so that the data can say what the game says.
- **B-57: a sight blocker hides its edge cells from outside.** "Units inside obstructing terrain can't be seen by
  units or structures on the outside" (Blizzard's map guide). Today a ground ray that enters a blocker from outside
  reveals the first blocker cell, so a unit standing in a blocker's edge can be seen and shot from outside.

All four become rules over catalog data. B-54 reuses the existing `target_loss_behavior` field and flips its default
to the engine's. B-55 adds `attack_visibility` on `WeaponSpec` plus a per-player reveal counter on each entity. B-56
replaces `is_detector: bool` with `detection_radius`. B-57 changes the map's ray cast. No code names a unit (charter
§3.3, C5).

Four neighbouring differences are registered and deferred (§2.5): B-182 (a missile follows a target that teleports),
B-183 (the snapshot of a hidden ground attacker), B-184 (detectors under construction, **verify**) and B-185 (a unit
keeps tracking and firing at a target that left its owner's vision).

A full plan is required (charter §6): new rules in the simulation, a change of hashed encoding (B-55), golden replays
that move (B-54 and B-57 by behaviour), and work across `sim_map`, `sim_types`, `sim_core`, the catalog and the client.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`e212ca44` (checksum schema 39).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-2 splash hits cloaked units; S-5 25 ticks per Faster second, whole-tick rounding) | Closed | D-1 (a lost missile's splash), D-5 (54 ticks) |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S14 to S17 (Liquipedia, Blizzard's guide, a data author's forum answer) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.5, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| **Draft, not filed:** "Sight blockers, detection through terrain, detectors under construction, and who sees an attack reveal" | Proposed for the orchestrator to file as Q0101 (§15); defaults followed meanwhile | D-3, D-4, D-5, B-184 |

---

## 1. Summary

| | Today (`e212ca44`) | After this plan |
|---|---|---|
| Missile whose target is lost (dies, loaded, invulnerable, cloaked or burrowed undetected) | Vanishes (`Fizzle`, the serde default); no strike, no area | Flies on to the last tracked position and lands (`ImpactLastKnownPosition`, the new default; 5.0.14 `PointFallback`). It strikes no unit there, but a splash weapon's area strikes around the point. `Fizzle` stays expressible (`NeuralParasiteLaunchMissile` clears the flag) |
| Air or Colossus attacker over high ground or in fog | Never visible to the victim's owner; victims cannot acquire it and flee as helpless | Revealed to the struck unit's owner for 54 ticks after each damaging hit (3 Normal s), refreshed by every hit. During the reveal it is acquirable, alerts allies and counts as visible for retaliation; concealment still needs detection |
| Ground attacker hidden by high ground or fog | Never visible | Unchanged in the simulation (5.0.14 shows a snapshot only, no targeting); the snapshot presentation is B-183, deferred |
| Detection radius | The sight radius (`is_detector: bool`) | `detection_radius: Option<f32>` per archetype and structure, cast along the detector's line of sight; every shipped detector 11 = its sight (no world moves) |
| Ground ray entering a sight blocker from outside | Reveals the first blocker cell, then stops | Stops before it; air units and watchtowers unchanged (**verify**, §15 part 1) |
| Hashed state | — | `Entity::attack_reveal_ticks` (per player) and `WeaponSpec::attack_visibility`; schema **next free at merge** (40 on `e212ca44`, 41 if the `turret-wind-up-plan` branch merges first) |
| Worlds that move | — | B-54: goldens by projectile lifetime only, no measured trace. B-56: none. B-57: goldens and showcase-map worlds only where a ground ray reaches a blocker. B-55: battle 41 (new) and only those measured worlds where WP0's probe counts a hidden air attacker striking a side that can hit air |
| Size | — | L: eight packages, each S or M |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`. `liberty EffectData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CEffectLaunchMissile` default, `core EffectData:95–109`: `Flags PointFallback 1` (:97), `ValidateTeleport 1` (:100), `ValidateWeapon 1` (:101), `ImpactLocation TargetUnit` | Every launched missile validates the weapon's filters and falls back to a point when they fail |
| S3 | The only missiles that clear these flags: `NeuralParasiteLaunchMissile` (`liberty EffectData:1002–1013`, `PointFallback 0`), the Brood Lord's escort missiles (`liberty :2664–2668`, `:2741–2749`, `:2774–2780`, `ValidateWeapon 0`), `ReaperKD8KnockbackPHLM` (`void :3324–3332`) | No modelled attack missile clears `PointFallback` [DATA] |
| S4 | Weapon `TargetFilters`: `CWeapon` default `Visible;-` (`core WeaponData:29`); typical rows `Visible;Missile,Stasis,Dead,Hidden,Invulnerable` (`liberty WeaponData:9`, `:19`, `:51`, …) | What "lost" means: not visible to the owner (cloaked undetected; also out of vision, see B-185), dead, hidden (loaded), invulnerable or in stasis |
| S5 | `CEffectDamage` default, `core EffectData:61–69`: `Visibility Snapshot` (:62), `ImpactLocation TargetUnitOrPoint` (:66) | A missile landing on a point damages through its area only [INF]; the default attack visibility is a snapshot |
| S6 | `Visibility Visible` overrides (survey, §2.3): `ThermalLancesMU` (`liberty EffectData:2421`), `ThermalLancesMUAir` (`void :1571`), `LanzerTorpedoesDamage` (`liberty :1715`), `GlaiveWurmU1–U3` (`:2229`, `:2234`, `:2239`), `ParasiteSporeDamage` (`:2589`), `BroodlingEscortDamage` (`:2788`), `BacklashRocketsU` (`:2566`), `IonCannonsU` (`:1984`), `PrismaticBeamMUBase` (`:125`), `VoidRaySwarmDamage` (`void :1807`, `swarmmulti :567`), `LiberatorMissileDamage` (`void :2935`), `LiberatorAGDamage` (`void :3010`), `InterceptorBeamAADamage`/`AGDamage` (`voidmulti :2264`, `:2280`), `MothershipBeamDamage` (`liberty :959`), `ATALaserBatteryU`/`ATSLaserBatteryU` (`:1726`, `:1735`), `TempestDamage` (`swarm :1422`), `OracleWeaponDamage` (`void :1312`) | Every damage effect of an air-plane attacker is `Visible`; no ground attacker's is [DATA] |
| S7 | `Visibility Hidden`: `PsiStormDamage` (`liberty EffectData:2099`), `90mmCannonsDummy` (`voidmulti :1195`, a dummy), `KillRemove` (`swarm :1296`) | A third value, used by no modelled weapon damage |
| S8 | `CBehaviorReveal id="AttackReveal"`, `core BehaviorData:237–240`: `InfoFlags Hidden`, `Duration 3`. No layer of S1 links it to a unit, so the engine applies it | The reveal lasts 3 Normal s [DATA]; how it is triggered is the engine's (S15) |
| S9 | `CBehaviorBuff default id="Detector"`, `core BehaviorData:58–61` (`DetectFilters -;Missile`); `Detector11`, `liberty BehaviorData:1175–1178` (`Detect 11`); also `Detector10`, `Detector12`, `Detector20` (`:1169–1191`), `Detector9` (`swarm :337–339`), `Detector7` (`void :1032–1034`), `Detector13p75` (`voidmulti :678–683`), `Detector35` (`voidmulti :1061–1065`, the Xel'Naga healing shrine) | The detection radius is a behaviour's `Detect`, separate from `Sight` [DATA] |
| S10 | The detectors (S1 `UnitData`, `Sight` / behaviour): Observer `liberty :3442` / `:3446`; Raven `:5607` / `:5613`; Missile Turret `:6062` / `:6067`; Overseer `:7511` / `:7515`; Spore Crawler `:8782` / `:8787`; Photon Cannon `:11081` / `:11085`, each `Sight 11`, `Detector11`; Observer and Overseer Siege Mode `voidmulti :1144` / `:1146`, `:1224` / `:1227`, each `Sight 13.75`, `Detector13p75`. No layer changes these `Sight` values, and no 5.0.14 upgrade changes a `Sight` or a `Detect` (`UpgradeData` of every layer, only commented-out Liberator rows) | For every modelled detector, detection radius = sight radius [DATA] |
| S11 | Scanner Sweep, `liberty EffectData:1861–1867`: `RevealRadius 13`, `ExpireDelay 12.2775`, `RevealFlags Unfog, Detect`; Oracle Revelation, `swarm EffectData:3222–3227` (`AreaArray Radius 6`), buff `OracleRevelation` (`void BehaviorData:170–175`, `SuppressCloak`; `voidmulti :169–171` Duration 28). The `Revelation` detector buff (`swarm BehaviorData:185–189`, `Detect 11`) is linked only inside a commented-out unit (`swarm UnitData:829–902`) | Area detection by abilities; out (§2.5) |
| S12 | `PowerUserQueue…` (`liberty BehaviorData:896–960`): an unpowered Photon Cannon gets `SuppressDetection` and disables `Detector11` | Power, B-63; out |
| S13 | `LayoutButtons Face="Detector" … Requirements="NotUnderConstruction"` on the Missile Turret, Spore Crawler and Photon Cannon (`liberty UnitData:6074`, `:8796`, `:11092`) | The button hides under construction; whether detection does is **verify** (B-184) |
| S14 | Blizzard, *Game Guide: Map Elements* (news.blizzard.com/en-us/article/4546768): "Units inside obstructing terrain can't be seen by units or structures on the outside." Caption: "A protoss player places a pylon to watch the smoke area." | B-57: no blocker cell is seen from outside [EXT]; a source inside sees its blocker |
| S15 | A Battle.net forum answer by a long-time map-data author (us.forums.blizzard.com/en/sc2/t/missile-turrets-can-spot-high-ground-units/26375): "All units that use the air plane act like this, so all air units and the Colossus. In the editor, the damage effects of these units' attacks had to be adjusted to make the unit visible … When these 'air' units attack something, they are targetable for 3 game seconds (= 2.1 seconds). Ground units do not become targetable, they only appear in fog and on minimap." | Links S6 to S8: `Visible` gives the 3 s targetable reveal; `Snapshot` gives a fog and minimap snapshot only [EXT, consistent with S6 and S8] |
| S16 | Liquipedia, *High Ground and Low Ground* (search abstract; the page refused fetching, HTTP 429): "Units on the high ground that attack will reveal their position (but not necessarily grant vision)"; "Air unit movement can be hidden by keeping them over high ground, but they will be revealed if they attack"; the Colossus counts as air for this | Same rule [EXT] |
| S17 | Liquipedia, *Distance* (search abstract): range "is also the distance units can see … as well as reveal cloaked or burrowed units if they are a Detector" | Detection is a circle like sight [EXT]; whether terrain blocks it is not stated (§15 part 2) |

### 2.2 The rules that follow

1. **A lost missile lands on the point** (S2, S3). A missile homes on its target while the target passes the weapon's
   filters (S4). Once the target fails them, the missile continues to the last point it held and lands. Its damage
   effect then has a point and no unit (S5), so a single-target weapon strikes nobody and an area weapon strikes
   around the point [INF]. The fallback is permanent: a target that becomes valid again before impact is not struck
   [INF; the flag names a fallback, not a pause].
2. **When the filters are checked** (`ValidateWeapon`) is the engine's. "In flight" and "at impact" differ only for a
   target that is lost and found again before impact. The plan keeps our per-tick check (§3 item 1) [INF].
3. **An air-plane attacker is revealed to the player it damages** for `AttackReveal`'s 3 Normal s, refreshed by each
   damaging hit (S6, S8, S15, S16). During that time it is targetable by that player. Because the trigger is the
   damage effect's `Visibility`, every unit the effect strikes reveals the attacker to its owner, including units
   struck by an area. A reveal grants visibility, not detection: a cloaked attacker stays cloaked [INF; a Banshee
   attacking from cloak stays untargetable without detection].
4. **A ground attacker's damage is a snapshot** (S5, S15): the struck player sees it in fog and on the minimap but
   cannot target it. Our simulation already never makes it targetable. The snapshot is presentation (B-183).
5. **Detection is its own radius** (S9, S10, S17); for every modelled detector it equals the sight radius.
6. **Nothing in a sight blocker is seen from outside** (S14); a unit inside sees out and sees its blocker (S14's
   caption; our current rule). Whether an air unit or a Xel'Naga tower sees into a blocker is **verify** (§15 part 1).

### 2.3 Survey: attack visibility of every shipped weapon

| Ours (slug) | 5.0.14 damage effect | `Visibility` | Source |
|---|---|---|---|
| `tripod_annihilator` (Colossus, `CliffWalker`) | `ThermalLancesMU` | `Visible` | S6 |
| `vector_interceptor` (Viking, air) | `LanzerTorpedoesDamage` | `Visible` | S6 |
| `sonic_bat` (Mutalisk) | `GlaiveWurmU1` (and U2, U3) | `Visible` | S6 |
| `sky_leviathan` (Corruptor) | `ParasiteSporeDamage` | `Visible` | S6 |
| `spore_matriarch` (Brood Lord) | `BroodlingEscortDamage` | `Visible` | S6 |
| `cloak_gunship` (Banshee) | `BacklashRocketsU` | `Visible` | S6 |
| `flying_saucer` (Phoenix) | `IonCannonsU` | `Visible` | S6 |
| `prismatic_saucer` (Void Ray) | `VoidRaySwarmDamage` / `PrismaticBeamMUBase` | `Visible` | S6 |
| `sky_bastion` (Liberator, anti-air) | `LiberatorMissileDamage` | `Visible` | S6 |
| `mothership_carrier` (Carrier/Mothership) | `InterceptorBeamAADamage`/`AGDamage`, `MothershipBeamDamage` | `Visible` | S6 |
| every other shipped weapon (ground units, armed structures, the Thor analogue's both weapons) | their `DU_WEAP*` children without an override | `Snapshot` | S5 |

The rule this table follows is "a weapon of an archetype whose `mover_type` is `Air` or `CliffWalker` is `Visible`,
every other weapon is `Snapshot`". WP6's test reads it as a property and cites the table; the data authors each row
explicitly (D-5).

### 2.4 Survey: detectors

| Ours (slug) | 5.0.14 | `Sight` | `Detect` | Source |
|---|---|---|---|---|
| `stealth_eye_orb` | Observer | 11 | 11 (`Detector11`) | S10 |
| `omni_probe_drone` | Raven | 11 | 11 | S10 |
| `flak_array_dome` | Missile Turret | 11 | 11 | S10 |
| `tesla_sentinel_dome` | Photon Cannon | 11 | 11 | S10 |
| `spore_spire_dome` | Spore Crawler | 11 | 11 | S10 |
| (none) | Overseer, Observer/Overseer Siege Mode | 11; 13.75 | 11; 13.75 | S10; no analogue or mode shipped |

### 2.5 Neighbouring rules: in or out

| Rule | Verdict | Why |
|---|---|---|
| B-54, B-55 (air plane), B-56, B-57 | **In** (D-1 to D-6) | |
| A missile drops a target that teleports (`ValidateTeleport 1`, S2): a Stalker analogue that blinks dodges missiles in flight — **B-182, registered here** | Deferred (§14 F-1) | What counts as a teleport for the flag is the engine's (**verify**). Blink is an effect-tree displacement here and needs a "teleported this tick" mark that the missile step can read; that is its own small plan |
| The snapshot of a hidden ground attacker (S5, S15): the struck player sees the attacker in fog and on the minimap — **B-183, registered here** | Deferred (§14 F-2) | Presentation (`PlayerView` and the client's minimap and fog); no simulation decision depends on it |
| Detectors under construction detect (S13) — **B-184, registered here, verify** | Deferred (§15 part 3) | Not in the data; the owner question asks |
| A unit keeps tracking, chasing and firing at a non-concealed target outside its owner's vision. The weapons' `Visible` filter (S4) drops it, and with `ValidateWeapon` (S2) a missile whose target walks into fog lands on the point — **B-185, registered here** | Deferred (§14 F-3) | `is_entity_targetable` requires vision only for a concealed target. Changing it reaches pursuit in `crates/sim_core/src/movement/locomotion.rs` (owned by another branch) and moves the matrix widely (kiting, chases into fog). Its own plan. It bounds B-55: after a reveal ends, a victim that acquired the attacker keeps it until B-185 lands (R-4) |
| Scanner Sweep and Oracle Revelation (S11) | Out | Abilities not shipped (B-69) |
| Power for the Photon Cannon (S12) | Out | B-63 |
| Effect-tree missiles (spells, `effects::ActiveMissile`) on target loss | Out (§14 F-4) | Spell missiles have their own flags per effect; no shipped spell missile carries an area |
| Lifetime of attack projectiles (B-53) | Unchanged | Classified (ii) by the deviation-fixes plan; a missile flying on to a point arrives well within 125 ticks |

---

## 3. Diagnosis (on `e212ca44`, read from the code)

1. **Lost missiles.** `advance_projectile` → `track_projectile_target` (`crates/sim_core/src/combat_resolution.rs`)
   checks `is_entity_available_for_combat(owner, target)` each tick. When that fails it follows
   `weapon_spec.target_loss_behavior`. The serde default `Fizzle` returns `ProjectileStep::Disjointed`: the missile
   is removed, with no impact and no area. `ImpactLastKnownPosition` already keeps the last tracked `target_pos`,
   clears `target_id` and lands there, and `resolve_projectile_impact` applies only the splash (with no primary) for a
   target-less impact. That is S2's point fallback. No shipped row authors `target_loss_behavior`, so all take
   `Fizzle`; the enum's doc calls `Fizzle` "Default SC2 homing projectile behavior", which S2 contradicts. The
   availability predicate is S4's filter without its fog half (B-185).
2. **No attack reveal.** Four places decide whether a player sees a non-concealed enemy, each by "its cell is
   `Visible`": `scan_candidates` (`target_acquisition.rs`), `is_attacker_visible_to` (retaliation, helpless flee and
   ally alerts), `is_seen_by` (`world/casts.rs`) and `is_visible_enemy_target` (`world/attack_dash.rs`). The client
   decides the same way (`BattleFog::shows`, `crates/client/src/battle_pipeline/fog.rs`, from
   `PlayerView::visibility`). Nothing records that an attack revealed a unit, and `WeaponSpec` has no visibility
   field.
3. **Detection rides on sight.** `collect_sights` (`world/vision.rs`) builds one `VisionSource` per entity with
   `sight_radius` (with sight upgrades) and `is_detector`. `PlayerVisionMap::cast` (`crates/sim_map/src/vision.rs`)
   increments `detector_counter` on every cell that it marks visible. So the detection radius is the sight radius,
   upgrades included, and it follows sight's cliff and blocker rules. The catalog's `is_detector: bool` (units and
   structures) has no radius.
4. **The blocker edge.** In `cast`, a ground ray from outside that steps onto an `LOS_BLOCKER` cell marks that cell
   ("Entering blocker from outside: reveal edge cell, then terminate ray") and stops. `vision_tests.rs`
   `test_los_blocker_smoke_vent` asserts the edge cell `Visible`. Both shipped maps carry 3×3 blockers:
   `default_arena` cells x 15–17, y 15–17 (the goldens' map), `showcase_canyon` x 19–21, y 43–45 and x 42–44,
   y 18–20 (the showcase, showcase effects and skirmish map).
5. **Hashed state.** `PlayerVisionMap` states and `detector_counter` are hashed (`checksum.rs`). Projectiles are
   hashed with their `WeaponSpec`. A missile that lives longer (B-54), and edge cells that change state (B-57), move
   checksums without any new field. B-56 changes no hashed value. B-55 adds hashed state.

---

## 4. Design decisions

### D-1. B-54: the engine's point fallback is the default

- `ProjectileTargetLossBehavior`'s `#[default]` moves to `ImpactLastKnownPosition`. Its doc is rewritten: "5.0.14's
  `PointFallback` (`core` EffectData:97), the default of every attack missile: once the target fails the weapon's
  filters, the missile flies on to the last point it tracked and lands there; its strike reaches no unit, its area
  strikes around the point. Permanent: a target that becomes available again is not re-acquired." `Fizzle`'s doc
  says "`PointFallback 0` (e.g. `NeuralParasiteLaunchMissile`, `liberty` EffectData:1013): the missile is removed
  without landing". `RetargetNearest` is unchanged (no 5.0.14 attack missile; kept as expressible data).
- The sim code does not change: `track_projectile_target` and `resolve_projectile_impact` already do this. The
  landing point is where the target was on the last tick it passed, since tracking updates `target_pos` before the
  check fails.
- The fallback impact carries the weapon's area with `primary: None`, at the plane the missile was aimed at
  (`target_mover`). An area's cliff rule (`SplashSpec::cliff`) reads the point's level as for any other impact.
- **Area against a cloaked target** (S-2 in Q0052): the area strikes units whether or not they are detected, so a
  Liberator analogue's missile at a target that cloaks still splashes the point, and the cloaked unit too if it is
  inside the area. That is the game's rule and S-2's.
- **Data:** no row changes; the default is the data (no shipped row authors the field). The module doc of
  `combat_resolution.rs` gains the invariant: "A projectile whose target is lost lands at the last tracked point
  unless its weapon says `Fizzle` (plan `2026-11-23-vision-and-detection` D-1)."
- **Zero recompilation:** a catalog copy authoring `target_loss_behavior: Fizzle` on the Liberator analogue
  restores today's outcome in the same binary (P-3).

### D-2. B-56: `detection_radius` as catalog data

**Catalog** (`UnitArchetypeDef` and `StructureDef` in `crates/sim_core/src/catalog.rs`; the resolved
`UnitArchetype`): `is_detector: bool` is replaced by `#[serde(default)] detection_radius: Option<f32>` in RON,
resolved through the checked conversion that `sight_radius` uses into `Option<SimScalar>`. `None` means not a
detector. `UnitArchetype::is_detector()` stays as a method (`detection_radius.is_some()`) for the readers that only
ask whether a unit detects (presentation, `published_actions.rs`). Validation: `Some(r)` needs `0 < r ≤
MAX_SIGHT_RADIUS_CELLS` (the same bound as sight, named in the error). RON rows: the five shipped detectors author
`detection_radius: Some(11.0)` with "5.0.14 `Detector11` (`liberty` BehaviorData:1175–1178), linked at UnitData:N";
every other row drops `is_detector: false`. The RON is required to change in one commit, because an unknown
`is_detector` field must fail loudly (`deny_unknown_fields` on these defs, **verify**; otherwise validation refuses
it).

**Map** (`sim_map::VisionSource`): `is_detector: bool` becomes `detection_radius: SimScalar` (`ZERO` = none).
`watchtower_source` sets `ZERO`. `PlayerVisionMap::cast`:

```text
reach    := max(sight_radius, detection_radius)          # the rays are walked this far
r_int    := ceil(reach) clamped to MAX_PRECOMPUTED_RADIUS
for each ray cell at squared distance d2 (whole cells, as today):
    stop if d2 > reach²; apply the cliff and blocker rules as today (D-3 for blockers)
    sees    := d2 ≤ sight_radius²
    detects := d2 ≤ detection_radius²
    mark(cell, sees, detects, change)    # visible_counter only if sees; detector_counter only if detects
the source's own cell: sees (always, as today); detects if detection_radius > 0
```

- `mark` takes the two booleans; `Remove` mirrors `Add` with the same booleans, so incremental recasting stays exact
  (the module's invariant, P-5).
- **Detection follows the detector's line of sight**: the same cliff and blocker rules as sight. This keeps
  today's behaviour and is the default of §15 part 2 (**verify**). A detector whose detection radius exceeds its
  sight detects cells it does not see; a concealed unit there is targetable only if another source of its player
  sees the cell (`is_cloak_revealed` already needs both).
- **Sight upgrades do not change detection** (S10: no upgrade touches `Detect`). `collect_sights` passes
  `arch.detection_radius` unchanged, and `None` or a `suppress_detection` status gives `ZERO`.
- `sees_the_same` compares `detection_radius` instead of `is_detector`.
- **Equal radii are exactly today's cast**: with `detection_radius = sight_radius` the walked cells, the visible
  marks and the detector marks equal today's, cell for cell. So WP2 moves nothing and needs no schema (the
  counters are hashed, the source is not).

### D-3. B-57: a ground ray stops before a blocker it enters from outside

In `cast`, the branch "entering blocker from outside" stops the ray **without** marking the cell. Everything else is
unchanged:
- A source standing in a blocker cell (`source_in_blocker`) marks its rays through blocker cells and on out of them.
  Once a ray has left the blocker (`in_blocker = false`), entering any blocker cell stops it, as today.
- Air and `CliffWalker` sources (`is_air`) and watchtowers (`MoverType::Air`) ignore blockers, as today
  (**verify**, §15 part 1: S14 says "units or structures on the outside" without naming the air plane).
- Detection follows (D-2), so a detector outside a blocker detects nothing inside it.
- The comment block above the branch cites S14. `test_los_blocker_smoke_vent` is re-derived (the edge cell
  `(8, 9)` is no longer `Visible` from the west observer). The module doc's "units outside cannot see inside beyond
  1 cell" becomes "units outside a blocker see none of it (Blizzard, Map Elements)".

### D-4. B-55: `WeaponSpec::attack_visibility`

**Type** (`crates/sim_types/src/weapon.rs`, exported from `lib.rs`):

```rust
/// What a weapon's damage shows of its attacker to the player it strikes: `StarCraft` II's
/// `CEffectDamage` `Visibility` (`core` EffectData:62).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackVisibility {
    /// A snapshot in fog and on the minimap, never a target (the engine default). The
    /// simulation records nothing; the presentation is bug B-183.
    #[default]
    Snapshot,
    /// The attacker becomes visible, and so targetable, to each struck unit's owner for
    /// `reveal_ticks` (`AttackReveal`, `core` BehaviorData:237–240, 3 Normal s).
    Visible {
        /// How long one hit reveals the attacker, in whole ticks.
        reveal_ticks: u16,
    },
    /// Nothing is shown.
    Hidden,
}
```

`WeaponSpec` and its catalog def gain `#[serde(default)] pub attack_visibility: AttackVisibility`. Validation:
`Visible { reveal_ticks: 0 }` is refused, naming the field. The reveal duration is authored per weapon rather than
held in a global table. The game has one `AttackReveal` behaviour, but the data links `Visible` per effect, so the
catalog keeps the link where the data keeps it, at the cost of ten repeated values that one test checks are equal
(P-11).

### D-5. B-55: the reveal and who sees it

**State** (`crates/sim_core/src/entity.rs`): `pub attack_reveal_ticks: PerPlayer<u16>`, all zero at spawn. For each
player p it holds the ticks left during which this unit is revealed to p by its attacks. The method
`Entity::is_attack_revealed_to(p) -> bool` is `attack_reveal_ticks[p] > 0`.

**Set** (`combat_resolution.rs`): in the one helper every weapon strike goes through (`apply_weapon_hit` for the
target, `strike_area`'s per-unit strike, and the Baneling's fallback strike, which shares that body): when a strike
lands damage on a unit u owned by player p ≠ the attacker's owner and the weapon is `Visible { reveal_ticks: n }`, set
`attacker.attack_reveal_ticks[p] := max(current, n)`. This happens **before** the strike's retaliation runs, so the
victim's response in the same tick already sees the attacker. A strike that the barrier, shields or armour reduce to
zero still counts, because the effect ran (**verify**). A strike skipped because the target was unavailable does
not count. The attacker must still exist: a dead attacker (a Baneling) reveals nothing.

**Count down** (`world/vision.rs`, at the start of `update_vision`, phase 3, before acquisition): every entity's
nonzero counters decrease by one. A hit in phase 7 of tick t is therefore seen for the rest of tick t (retaliation)
and by acquisition on ticks t+1 to t+n−1, so the reveal is in force for exactly n ticks. Decrementing walks all
entities. If a retained list of entities with a nonzero counter is used instead, it needs a `// PERFORMANCE
RATIONALE:` block. Either way the counters end the same.

**Duration**: 3 Normal s = 3 × 25 / 1.4 = 53.571 ticks. The project's statuses keep fractional durations, but this
is a per-hit counter, so it is authored in whole ticks, rounded half up to 54 (S-5 in Q0052). The data comment states
the arithmetic.

**Who sees it**: the owner of each struck unit (S15: "targetable"). Allied players: **verify**, §15 part 4; the
simulation has no alliance relation that would decide it, so the default is the struck owner only.

**One predicate for "player p sees enemy e"** (`target_acquisition.rs`, `pub(crate)`):

```text
sees(p, e) := vision_maps.get(p) is None                     # a viewer without a map sees all (today)
           || cell(e.position) is Visible to p
           || e.is_attack_revealed_to(p)
```

All four readers of §3 item 2 call it in place of their cell test (`scan_candidates`, `is_attacker_visible_to`,
`is_seen_by`, `is_visible_enemy_target`). The concealment test after it is unchanged: a concealed unit still needs
`is_cloak_revealed` (a `Visible` cell and detection), so a revealed cloaked attacker stays untargetable without
detection (§2.2 rule 3). Nothing under `movement/` reads it.

**Module docs**: `combat_resolution.rs` gains the invariant "A strike by a `Visible` weapon reveals its attacker to
the struck unit's owner for the weapon's `reveal_ticks` (D-5)"; `world/vision.rs` names the countdown.

### D-6. B-55: what the simulation publishes, and the client

- `PlayerView` (`crates/sim_types/src/player_view.rs`) gains `#[serde(default)] pub attack_revealed:
  Vec<EntityId>`: the other players' living, uncarried entities that are attack-revealed to the viewer, sorted by
  ID. The doc says: "visible to the viewer although their cell may not be; concealed ones still need `revealed`".
  `write_player_view` fills it in entity-ID order. It is reused like the other lists and allocates only on growth.
- Client (`crates/client/src/battle_pipeline/fog.rs`): `BattleFog` copies the list as it copies `revealed`, and
  `presence` treats a listed unit as in sight even when its cell is not (`shows` stays the cell rule for things
  without an entity: nodes and events). The module doc's detection section gains one sentence. Picking, the
  selection panel and bars already ask `presence`, so they follow. The refresh stays allocation-free after warm-up
  (`fog.revealed` is checked by pointer and capacity in the steady-state test; `attack_revealed` joins that check).
- `RenderSnapshot` is unchanged.

### D-7. Hashed state and checksum

- `fields!(Entity { …, attack_reveal_ticks })` after the last field, `PerPlayer<u16>` by its existing encoding.
- `fields!(WeaponSpec { …, attack_visibility })` with a `Canonical` for `AttackVisibility` (tag 0 `Snapshot`, 1
  `Visible` followed by `reveal_ticks`, 2 `Hidden`). A `WeaponSpec` is hashed inside every in-flight
  `ActiveProjectile`.
- **Schema: next free at merge** (40 on `e212ca44`; 41 if `turret-wind-up-plan` merges first). The doc of
  `CHECKSUM_SCHEMA_VERSION`: "Schema N adds each entity's attack-reveal counters per player after `<last field>`, and
  each weapon's attack visibility (0 snapshot, 1 visible with its reveal ticks, 2 hidden) after `<last weapon
  field>` (plan `2026-11-23-vision-and-detection` D-7)."
- B-54, B-56 and B-57 change no encoding. B-54 and B-57 move checksums by behaviour (§5).

### D-8. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-54 to B-57 point to this plan (status "open; plan `2026-11-23-vision-and-detection`"), each restated with §2's sources: B-54 "the missile vanishes instead of landing on the point (`PointFallback`)"; B-55 "an air-plane attacker is never revealed to the player it strikes (`Visibility Visible`, `AttackReveal` 3 s)"; B-56 "detection has no radius of its own (`Detector11` …); every modelled detector's equals its sight"; B-57 with S14. Register **B-182** to **B-185** (§2.5) | WP0 |
| `DESIGN.md` §4 | B-54 to B-57 fixed with commits and evidence. The "Simulation vision and watchtowers" row gains detection radius and blocker edges. The "Combat" row gains the point fallback and the attack reveal. The "Smog fog presentation" row gains attack-revealed units | WP7 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP14's vision half and U-11 (cloak half), U-12, U-13, U-14: "taken up by plan `2026-11-23-vision-and-detection`" | WP7 |
| New `docs/evidence/2026-11-23-vision-and-detection.md` | §7's comparisons per package, the probes' counts (§7.2), the traced first differences, the golden traces | WP0 creates; WP1 to WP7 append |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP7) | plan, WP1, WP7 |

---

## 5. Determinism and checksum

- **Integer and fixed point only.** Distances in `cast` stay whole-cell integer squares compared with fixed-point
  radii squared, as today. Reveal counters are `u16` with saturating arithmetic. Catalog radii go through the
  existing checked `f32 → SimScalar` conversion, as `sight_radius` does, so C7's allowances do not change.
- **No new randomness, clocks or collections.** `PerPlayer` is a fixed array.
- **B-54 (WP1) moves goldens by behaviour, not encoding.** A missile at a dying target now lives until it lands, so
  the hashed projectile list differs from the first overkill onward. Both goldens have Roach analogues (missiles)
  and marines, so a move is expected. Procedure:
  1. Before the change, record on WP0's tree a debug dump per checkpoint of both goldens: entities (ID, position,
     HP, shields, order front), projectiles, and the RNG word. The dump uses a scratch example or test under
     `~/scratch/vision-detection/`, never committed.
  2. After the change, the same dump. **Acceptance:** at every checkpoint the entity lines are identical. The only
     differences are projectiles present after the change and absent before, each with `target_id = None` and each
     traceable to a target that died, was loaded or became unavailable in the last ticks. The evidence names the
     first one per golden.
  3. Re-record with the header "Re-recorded for plan 2026-11-23-vision-and-detection WP1 (B-54): a missile whose
     target is lost flies on to the last point and lands, so the projectile list differs from <first checkpoint>.
     No entity's state differs at any checkpoint (dump in the evidence)." Native debug and release and
     `wasm32-wasip1` release and debug reproduce the new values (`proof.sh`).
- **B-56 (WP2):** every checksum unchanged on the four builds (P-6).
- **B-57 (WP3) moves goldens only if a ground ray in a golden reaches the default arena's blocker** (x 15–17,
  y 15–17). The same dump procedure applies, extended with each player's vision states at the blocker cells.
  **Acceptance:** the first difference is a blocker cell's state or detector count. Entity differences are allowed
  only after a tick at which a unit stood in a blocker edge cell (the probe of §7.2 c), and are traced. Re-recorded
  under its own header.
- **B-55 latent (WP4) proof**, the procedure of the splash-cliff and harvest-handoff plans:
  1. With the two fields left out of the encoding and the schema at its current number, the tree reproduces every
     recorded checksum of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and on
     `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the fields go in, the schema becomes the next free number, and the checksums are re-recorded under "Re-
     recorded for plan 2026-11-23-vision-and-detection WP4 (checksum schema M -> N): each entity also encodes its
     attack-reveal counters and each weapon its attack visibility, so every checkpoint changed. No behaviour did:
     with the two fields left out of the encoding at schema M, both fixtures reproduce as recorded for <the previous
     header>." The four builds reproduce the new values.
  3. If `main` takes the number first, the merge repeats step 2 with the next one.
- **B-55 active (WP6):** the goldens field no air or Colossus analogue (the Observer analogue has no weapon), so no
  counter is ever set and the checksums stay as WP4 left them (P-13). A move stops the package.
- **wasm.** No dependency changes.
- **Cost.** D-2 walks rays to `max(sight, detect)`, equal today. D-5 adds one decrement pass per tick over entities
  (O(N), no allocation) and one `max` per strike. The throughput bench is checked in the package gates.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (a plateau with no ramp, a
3×3 blocker), read every radius, range and duration from the catalog, and find weapons and detectors by property
(`mover_type`, `attack_visibility`, `detection_radius`, `projectile_speed`, `splash`), never by slug.

**B-54**
- **P-1 (point fallback).** For a missile whose target is lost in flight by death, loading, invulnerability or
  undetected concealment: the missile keeps flying at its speed to the target's last tracked position and lands
  there. It strikes no unit directly. A target that becomes available again before the impact is not struck. With
  an area, the area strikes around the point by its zones, on the missile's aimed plane, with friendly fire and the
  cliff rule as for any impact. Each cause is tested separately.
- **P-2 (unchanged when tracked).** A missile whose target stays available lands on it exactly as today (same tick,
  same damage).
- **P-3 (zero recompilation).** A catalog copy authoring `Fizzle` on a splash missile weapon gives today's result
  (no area) in the same binary.

**B-56**
- **P-4 (radius of its own).** With `detection_radius = sight_radius` the cast's three outputs (states, visible
  counters, detector counters) equal today's cell for cell, on flat maps, plateaus, ramps and blockers. A scratch
  detector with detection > sight marks detection on cells it does not see. A concealed enemy there is targetable
  only when another source of the same player sees the cell. A detector with detection < sight detects only inside
  its detection circle. A sight upgrade leaves the detection circle unchanged.
- **P-5 (incremental equals recount).** Adding and removing sources with unequal radii, on maps with cliffs, ramps
  and blockers, leaves the counters and states exactly as a full recount (the existing incremental test, extended).
- **P-6 (latent).** WP2 changes no world, no golden and no sandbox checksum.
- **P-7 (shipped table).** §2.4: exactly the five detectors have a detection radius, each equal to its sight radius
  and to 5.0.14's `Detect`.

**B-57**
- **P-8 (edge hidden).** A ground source outside a blocker marks no blocker cell on any ray. A ground source inside
  marks the blocker cells its rays cross before leaving and the cells beyond, and stops at a second blocker. Air
  sources and watchtowers mark blocker cells as today. A ground unit standing in an edge cell is not acquired by an
  enemy whose player has only ground sight outside the blocker, and is acquired once an air source of that player
  sees it.
- **P-9 (confinement).** WP3 changes only worlds whose map has blocker cells. Within them, it changes vision state
  only at blocker cells, and entity state only after a tick at which a unit stood in a blocker cell that an outside
  ground source of an enemy player reached (§7.2 c).

**B-55**
- **P-10 (the reveal).** A strike by a `Visible` weapon on a unit of player p reveals the attacker to p for exactly
  `reveal_ticks` ticks: the same tick's retaliation sees it, and acquisition on the next `reveal_ticks − 1` ticks sees
  it. Each further strike refreshes the reveal to the full duration and never shortens it. An area strike reveals to
  the owner of each struck unit. A concealed attacker stays untargetable without detection. A `Snapshot` or
  `Hidden` weapon, and a strike skipped as unavailable, reveal nothing. A dead attacker reveals nothing. The
  attacker's own player and third players are unaffected.
- **P-11 (shipped table).** §2.3: every weapon of an `Air` or `CliffWalker` archetype is `Visible { reveal_ticks:
  54 }`, every other weapon `Snapshot`, and no shipped weapon is `Hidden`.
- **P-12 (the ground case unchanged).** A ground attacker on a plateau striking a unit below is never visible to the
  struck player, before and after this plan.
- **P-13 (latent, then confined).** WP4 changes no world (goldens by the schema bytes only, proved). WP6 changes
  only worlds in which WP0's probe (§7.2 b) counts a `Visible` strike whose attacker was not in a `Visible` cell of
  the struck player. It leaves the goldens and the sandbox unchanged.
- **P-14 (direction, a prediction recorded before measuring, not a threshold).** In battle 41 (§7.4), before WP6 the
  Hydra analogues deal no damage to the Void Ray analogue over the plateau. After WP6 they deal damage in reveal
  windows that open within a tick of each Void Ray strike, and no Hydra fires at it outside a reveal window or the
  B-185 tail (R-4). The Void Ray's time to win grows or it loses; which of the two is not predicted.
- **P-15 (client).** A unit listed in `PlayerView::attack_revealed` is drawn and pickable for the viewer while its
  cell is fogged, and is hidden again once the list drops it. A concealed one still needs `revealed`. The refresh
  allocates nothing after warm-up.
- **P-16 (zero recompilation).** Changing a weapon's `attack_visibility` in a catalog copy changes the outcome in the
  same binary.

---

## 7. Worlds that move

### 7.1 Commands

The harvest-handoff scripts (`~/scratch/harvest-handoff/scripts/measure.sh`, `compare.sh`, `client.sh`,
`goldens.sh`) and strike-delays' `proof.sh`, copied to `~/scratch/vision-detection/scripts/` with their paths
changed. Binaries are built once per tree and run from that tree's `assets/`:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/vision-detection
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, and the
showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run it in the
background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is compared by
its stage log (`client.sh`).

### 7.2 WP0's probes

On WP0's tree a scratch patch prints three kinds of event; it is never committed and is kept under
`~/scratch/vision-detection/probe/` with its diff. The probe binary plays every world of §7.1 and the two goldens
(through a scratch test). The evidence records each count per world and seed and the first event of each kind.

- **(a) Lost missiles:** every `track_projectile_target` call that leaves the tracked branch, with the cause (dead,
  loaded, invulnerable, concealed), and whether the weapon has a splash. *Expected:* many in the matrix (overkill)
  and the goldens. Of those with a splash, none: the only shipped splash missile is the Liberator analogue's
  (`sky_bastion`), which no measured world fields (the matrix has no `sky_bastion` row; the showcase loadouts are
  SCV, Marine, Roach, Siege Tank, Hellion and Lurker analogues).
- **(b) Hidden air strikes:** every strike by a weapon of an `Air` or `CliffWalker` archetype whose attacker's cell
  is not `Visible` to the struck unit's owner. Each is tagged with whether that owner had, within the reveal window,
  a unit that could attack the attacker's plane and had it within scan range. *Expected:* the flat matrix has a few
  where a long-range air unit outranges its victims' sight (battle 20, Brood Lord range 10 against Roach sight 9),
  but none tagged, since those victims cannot hit air. The showcase worlds have no air or Colossus analogue: zero.
- **(c) Blocker edges:** every tick at which a ground unit stands in a blocker cell that an enemy ground source
  outside the blocker reaches with a ray, and every cell of a blocker that some ground source sees from outside.
  *Expected:* zero in the matrix (flat arenas without blockers, `sim_fixtures`; **verify** that no battle's terrain
  sets the flag). In the showcase worlds, edge-cell sightings when units pass the blockers. Unit-in-edge events only
  where a unit walks through a blocker (the west blocker at x 19–21, y 43–45 lies near player 1's approach,
  **verify** from `units.csv`). In the goldens, sightings only if a unit comes within sight of x 15–17, y 15–17.
- **Rule:** if (b) has no tagged event in any measured world, WP5 adds battle 41 (§7.4) so that WP6's move is
  measured on a world. If (b) tags events, WP5 still runs, because battle 41 is the principled world for P-14, but
  P-13 is also judged on the tagged worlds.

### 7.3 Expected moves

| World | WP1 (B-54) | WP2 (B-56 latent) | WP3 (B-57) | WP4 (B-55 latent) | WP5 (battle 41) | WP6 (B-55 active) |
|---|---|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical (no splash missile, probe a) | byte-identical | byte-identical (no blocker) | byte-identical | byte-identical except the new battle | byte-identical except battle 41, and battles with tagged (b) events if any |
| Battles 31, 33 | byte-identical | byte-identical | byte-identical | byte-identical | byte-identical | byte-identical (no air) |
| Showcase, default and 1–8 | byte-identical | byte-identical | **moves only on seeds where (c) counts a unit in an edge cell**, from that tick | byte-identical | byte-identical | byte-identical (no air) |
| Showcase effects, default and 1–8 | byte-identical (the Lurker analogue's burrow loses Roach missiles, which strike nobody either way) | byte-identical | as the showcase | byte-identical | byte-identical | byte-identical |
| Client skirmish stage log | identical | identical | identical unless (c) counts there (**verify**) | identical | identical | identical unless its pilot builds air (**verify** from the stage log) |
| Goldens, sandbox | **goldens re-recorded** (projectile lists only, §5); sandbox **verify** unchanged | unchanged | re-recorded only if (c) counts in a golden (§5) | schema bytes only (proof) | unchanged | unchanged |

**What each moving package records** for every world that moved (in the evidence, all seeds, before and after):
the first differing tick and the probe event behind it, each side's survivors and the tick of the last death. For
battle 41, every Hydra shot at the Void Ray with the tick of the strike that opened its reveal.

**Direction for WP3 (a prediction, P-9):** a unit standing in a blocker edge cell is attacked later, or not at all,
by enemies outside that rely on ground sight. So the side that walks through a blocker loses no more units than
before and its enemies lose no fewer. Which side remains at the end should not change on a seed. A change is traced
(§8 rule 3), not accepted as measured.

### 7.4 Battle 41 (WP5)

Why a battle and not a scenario: the matrix is the world of record for combat rules, it plays 20 seeds, and its
expectations are checked on every gate. Why this one: the plateau of battle 40 (whose terrain support already exists
in `sim_fixtures::matchup::Battle::terrain`) and an air attacker over it. The only new element is the reveal.

- `41_void_ray_above_a_cliff_vs_hydras`, map 96, 600 ticks. Terrain: cells x ≥ 47 at level 1, no ramp. Red: one
  `prismatic_saucer` at (50.0, 44.0) on Hold Position. Blue: six `thorn_viper`s at (38.0, 44.5) with spacing 1.0,
  attack-moving to (46.0, 44.5), the foot of the cliff. The positions follow one rule: the Void Ray (range 6) reaches
  the Hydras at the foot, the Hydras (range 5, sight 9) reach it, and its cell is on a plateau that ground sight from
  below never sees.
- Claim: "A Void Ray over a plateau is revealed to the Hydras it strikes below (5.0.14 `Visibility Visible`,
  `AttackReveal` 3 s)". The expectation is recorded on WP5's tree (today: the Hydras never fire, so the Void Ray
  wins), and re-recorded in WP6 with the probe's trace as its evidence (P-14).
- **verify** in WP5 that the Hydras stop at the cliff foot and stay within reach. If the arena's pathing sends them
  elsewhere, the Hydras hold at (45.5, 44.5) instead (Hold Position, the battle-39b pattern), and the change is
  stated in an amendment.

---

## 8. Stop rules

1. **WP2 and WP4 are latent.** Every world of §7 byte-identical. WP2: goldens unchanged. WP4: goldens moved by the
   schema bytes only, proved (§5).
2. **WP1 moves only projectile lists.** Every measured trace byte-identical. Goldens re-recorded only after the dump
   shows identical entity state at every checkpoint (§5). Any entity difference stops the package.
3. **WP3 and WP6 move only the worlds their probe counted** (P-9, P-13). A difference elsewhere, or on a seed with a
   zero count, is a defect of the package, not a re-record. A move whose first differing tick is not a counted event
   stops the package until traced. If the trace shows a second cause, that cause is registered as a bug, and the
   package lands only if its rule's change can be separated from it. A change of which side remains is traced to its
   first counted event and reported. It is not by itself a stop, as the rules are sourced (the deviation-fixes stop
   rule's exception).
4. **No tuning constant.** Every number is the data's (11, 54 from 3 Normal s) or an existing engine rule's. The new
   rule values are the enum's three cases.
5. **A golden move outside WP1, WP3 and WP4** follows §5's tracing or stops the package.

---

## 9. Client consequences

- **B-54:** missiles at a lost target are now drawn until they land on the point. Today they vanish mid-air. The
  impact presentation of a target-less landing is whatever the effect catalog draws for a ground impact. No look
  changes are needed: a splash missile's blast at the point is drawn by its splash visual, as for any impact.
  `effect_snapshot_tests.rs` and `effect_catalog_tests.rs` are re-run in the package gate. If one counts projectile
  lifetimes, it is re-derived in WP1 and named in the commit.
- **B-56:** none (the editor's detector tools, `crates/client/src/editor/tools.rs`, read `is_detector`; they move to
  the method in WP2, mechanically).
- **B-57:** blocker edge cells are fogged for outsiders, so the smog field over a blocker stays closed from outside.
  `battle_fog_tests.rs` and `smog_golden_tests.rs` run in WP3's package gate. A smog golden that covers a blocker
  edge (**verify**: the goldens' maps and views) is re-derived with the reason in the commit. The GPU concealment and
  smog tests (`concealment_gpu_tests.rs`) read cell states from the view and need no change; they run on their
  catalog triggers.
- **B-55:** D-6. `battle_fog_tests.rs` gains P-15. `sync_steady_state_tests.rs` (the fog refresh) covers the new
  list's capacity. No shader, GPU path or capture changes. The showcase takes field no air unit, so no storyboard
  moves.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP7 | D-8 |
| `docs/plans/README.md` | plan, WP1, WP7 | Row |
| `docs/evidence/2026-11-23-vision-and-detection.md` (new) | WP0 to WP7 | §7, §5 dumps |
| `crates/sim_types/src/weapon.rs`, `crates/sim_types/src/lib.rs` | WP1, WP4 | D-1 default and docs; D-4 `AttackVisibility`, field |
| `crates/sim_core/src/combat_resolution.rs` | WP1 (module doc), WP6 | D-1 invariant; D-5 set |
| `crates/sim_core/tests/combat_retaliation_and_projectile_tests.rs`, `attack_move_simulation_tests.rs` | WP1 | Existing fizzle tests pinned to `Fizzle` explicitly, plus the P-1 to P-3 tests (new file `lost_missile_tests.rs`) |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP1, WP2, WP6 | P-3, P-7, P-16 |
| `crates/sim_map/src/vision.rs` | WP2, WP3 | D-2 cast and `VisionSource`; D-3 edge rule, module doc, unit tests |
| `crates/sim_map/tests/vision_tests.rs` | WP2, WP3 | P-4, P-5, P-8 (the `sim_map` line-of-sight subject); `test_los_blocker_smoke_vent` re-derived |
| `crates/sim_core/src/catalog.rs`, `crates/sim_core/src/catalog/validation.rs` | WP2, WP4 | `detection_radius`; `attack_visibility` def and validation |
| `crates/sim_core/src/world/vision.rs` | WP2, WP6 | `collect_sights`, `sees_the_same`; the countdown |
| `crates/sim_core/src/world/presentation.rs`, `crates/sim_core/src/published_actions.rs`, `crates/client/src/editor/tools.rs` | WP2 | `is_detector` readers to the method (mechanical) |
| `assets/catalog/units.ron`, `assets/catalog/structures.ron` | WP2, WP6 | D-2 rows; D-4 rows (§2.3) |
| `crates/sim_core/src/entity.rs` | WP4 | `attack_reveal_ticks`, `is_attack_revealed_to` |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP4 | D-7 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM`; the encoding unit tests' schema byte | WP1, WP3 (if moved), WP4 | §5 |
| `crates/sim_core/src/target_acquisition.rs`, `crates/sim_core/src/world/casts.rs`, `crates/sim_core/src/world/attack_dash.rs` | WP6 | D-5 `sees` and its four readers |
| `crates/sim_types/src/player_view.rs`, `crates/sim_core/src/player_view.rs` | WP4 (field, filled empty-latent), WP6 (filled) | D-6 |
| `crates/client/src/battle_pipeline/fog.rs`, `crates/client/src/battle_hud/fog.rs` | WP4 | D-6 plumbing (the list is empty until WP6) |
| `crates/client/tests/battle_fog_tests.rs` | WP6 | P-15 |
| `crates/sim_core/tests/attack_reveal_tests.rs` (new, `//!` doc) | WP4, WP6 | §12 |
| `crates/sim_core/tests/checksum_state_tests.rs` | WP4 | Both fields change the checksum |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP6 | `attack_visibility_follows_starcraft` (P-11) |
| `crates/sim_core/tests/player_view_tests.rs` | WP6 | `attack_revealed` filled and sorted |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP5, WP6 | Battle 41; its re-recorded expectation |
| `crates/gate/suites.ron` | WP1, WP4, only if needed | New targets (**verify**: an uncovered target may fall to its crate's default class) |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP7 | Pointers |

**Nothing under `crates/sim_core/src/movement/` changes**; the `contact-solver-plan` branch works there. B-185, the
one neighbouring rule that would reach it, is deferred. **Overlap with `turret-wind-up-plan`:** it adds fields to
`WeaponSpec`, its encoding and `target_acquisition.rs`. Whichever merges second rebases its `fields!` lists and
schema number (R-5).

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first). Write the failing tests first and paste their red run in the
hand-back. Run `cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). Run
§7's comparison in every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog
identity in production Rust (C5); tests find weapons and detectors by property. Check `docs/questions/open/` before
the last commit. All packages may run as one branch and one session, one commit each, with one `package` gate on the
final tree (charter §4.3). B-55 (WP4 to WP6) may run as a second branch after WP3 merges, if the orchestrator
prefers two merges.

### WP0: records, baseline and probes (XS, docs only)

- `DESIGN.md` §4: B-54 to B-57 restated and pointed here; B-182 to B-185 registered (D-8). Evidence file created.
- Measure WP0's tree (§7.1) for the baseline. Run §7.2's probes and record their counts and first events. Record the
  golden dumps (§5 step 1). State in the evidence whether battle 41 is the only B-55 world.
- **Gate:** docs-only.

### WP1: a lost missile lands on its point (S, `sim_types`, B-54)

Failing tests first (red on WP0's tree) in `crates/sim_core/tests/lost_missile_tests.rs`, with scratch weapons
copied from a shipped missile weapon found by property (`projectile_speed`), and one with a scratch splash:
- `a_missile_whose_target_dies_lands_on_the_last_point` (P-1: flight continues, impact tick = distance / speed,
  nobody struck).
- `…_loads`, `…_turns_invulnerable`, `…_cloaks_undetected` (P-1, one per cause).
- `a_lost_splash_missile_strikes_around_the_point` (P-1, with bystanders inside and outside the zones).
- `a_target_found_again_is_not_struck` (P-1, permanence).
- `a_tracked_missile_lands_as_before` (P-2).
- `runtime_catalog_tests`: `Fizzle` authored restores no area (P-3).

The existing fizzle tests author `Fizzle` explicitly (they test that variant). Then the change: D-1. Goldens per §5.
**Acceptance:** §8 rule 2. Plan row `Active`. Commit `fix(sim_types): a missile whose target is lost lands on the
last point, 5.0.14 PointFallback (vision-and-detection WP1, B-54)`.

### WP2: detection has its own radius, latent (S, `sim_map`, `sim_core`, data, B-56)

Failing tests first (red: they do not compile): `vision.rs` unit tests and `vision_tests.rs`
`detection_and_sight_are_separate_circles` (P-4), `an_upgraded_sight_leaves_detection` (P-4),
`incremental_casts_with_unequal_radii_equal_a_recount` (P-5); `validation.rs` unit tests (zero, negative and
oversized radius refused, naming the field); `runtime_catalog_tests` `detection_radius_follows_starcraft` (P-7,
§2.4) and a detector copy with radius 6 that loses a cloaked target at 8 (zero recompilation). Then D-2 and the
mechanical readers. **Acceptance:** §8 rule 1 (and goldens unchanged). Commit
`feat(sim_map): detection has its own radius, 11 on every shipped detector as in 5.0.14 (vision-and-detection WP2,
B-56)`.

### WP3: a sight blocker hides its edge (S, `sim_map`, B-57)

Failing tests first (red on WP2's tree): `vision_tests.rs` `a_blocker_hides_every_cell_from_outside`,
`a_source_inside_a_blocker_sees_it_and_out`, `air_and_towers_see_into_a_blocker` (P-8), and `sim_core`
`a_unit_in_a_blocker_edge_is_not_acquired_from_outside` (P-8 end to end, a scratch map with a 3×3 blocker). Then D-3;
`test_los_blocker_smoke_vent` re-derived. **Acceptance:** §8 rule 3 on §7.2 (c)'s counts; goldens per §5. Commit
`fix(sim_map): nothing in a sight blocker is seen from outside (vision-and-detection WP3, B-57)`, stating which
worlds moved, on which seeds, and why.

### WP4: attack visibility and reveal state, latent (M, `sim_types`, `sim_core`, client plumbing)

Failing tests first (red: they do not compile): `AttackVisibility` serde defaults and validation (`reveal_ticks: 0`
refused); `checksum_state_tests` extended to both fields; `attack_reveal_tests::a_fresh_unit_is_revealed_to_nobody`;
the client's fog refresh with an `attack_revealed` list (steady-state allocation). Then D-4, the entity field, D-7,
and D-6's plumbing (the list stays empty because nothing sets a counter). Every shipped weapon takes the default
`Snapshot`. The schema proof of §5. **Acceptance:** §8 rule 1. Commit `feat(sim_core): weapons carry their attack
visibility and units their attack reveals, latent (vision-and-detection WP4, schema N)`.

### WP5: battle 41 (S, fixture)

Battle 41 (§7.4) with its expectation recorded on this tree; the matrix test passes. **Acceptance:** §8 rule 1 for
every other world. Commit `test(sim_core): battle 41, a Void Ray over a plateau against Hydras below
(vision-and-detection WP5)`.

### WP6: an air attacker is revealed to the player it strikes (M, `sim_core`, data, client, B-55)

Failing tests first (red on WP5's tree), in `attack_reveal_tests.rs` on a plateau map with scratch copies found by
property:
- `a_visible_strike_reveals_the_attacker_for_its_ticks` (P-10: the exact window, the same-tick retaliation).
- `each_strike_refreshes_the_reveal`, `an_area_strike_reveals_to_each_struck_owner`,
  `a_cloaked_attacker_stays_hidden_without_detection`, `a_snapshot_or_hidden_weapon_reveals_nothing`,
  `a_dead_attacker_reveals_nothing` (P-10).
- `a_ground_attacker_above_stays_unseen` (P-12).
- `weapon_catalog_rules_tests::attack_visibility_follows_starcraft` (P-11); `runtime_catalog_tests` (P-16);
  `player_view_tests` (`attack_revealed` filled, sorted, without own units); `battle_fog_tests`
  `an_attack_revealed_enemy_is_drawn_and_pickable_in_fog` (P-15).

Then D-5's rule, the four readers through `sees`, `write_player_view` filling the list, and the data (§2.3, with S6
and S8 cited per row). Battle 41's expectation is re-recorded with its trace. **Acceptance:** §8 rule 3; P-13, P-14;
§7.3 column WP6. Commit `fix(sim_core): an air attacker is revealed to the player it strikes for 3 game seconds
(vision-and-detection WP6, B-55)`, stating which worlds moved and why.

### WP7: bookkeeping (XS)

D-8's records; the evidence completed; this plan's `## As built`; README row `Done`. Merge `main`; `cargo gate
package` once on the final tree (by the verifier); the schema re-recorded if `main` took the number (§5).

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Map | P-4, P-5, P-8 | WP2, WP3: `sim_map/src/vision.rs` unit tests, `vision_tests.rs` (the `sim_map` line-of-sight subject) |
| Types and validation | Defaults, refused values | WP2, WP4: `sim_types` unit tests, `catalog/validation.rs` unit tests |
| Rules | P-1, P-2, P-10, P-12 | WP1 `lost_missile_tests.rs`; WP6 `attack_reveal_tests.rs`; WP3 end to end |
| Encoding | Both new fields change the checksum | WP4: `checksum_state_tests.rs` |
| Data | §2.3, §2.4 against 5.0.14 | WP2 `runtime_catalog_tests.rs`; WP6 `weapon_catalog_rules_tests.rs` |
| Zero recompilation | P-3, P-7, P-16 | `runtime_catalog_tests.rs` |
| Published view and client | P-15; allocation-free refresh | WP4, WP6: `player_view_tests.rs`, `battle_fog_tests.rs`, `sync_steady_state_tests.rs` |
| Worlds | P-6, P-9, P-13, P-14: §7.3 | every package (measure scripts); goldens and sandbox in the package gate (release and `wasm32-wasip1`) |
| Charter subjects | `sim_map`, `sim_core` and `client` subjects of the guide (`ramp_vision_tests.rs`, `damage_and_shields_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, `skirmish_match_tests.rs`, …) keep passing | package gate |

Re-derived (expected to change): `vision_tests.rs` `test_los_blocker_smoke_vent` (WP3); the fizzle tests of
`combat_retaliation_and_projectile_tests.rs` and `attack_move_simulation_tests.rs` (WP1, `Fizzle` made explicit);
any test that counts projectiles right after a target's death (search `projectiles.is_empty()` in
`crates/sim_core/tests/`, WP1, each named in the commit).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | S15 is a forum answer, not data | The reveal's trigger and duration rest on an outside reading | It is consistent with two data facts (S6's `Visible` exactly on the air plane, S8's 3 s) and with S16; P-11 pins the table; the owner question (§15 part 4) asks for an editor check of the allies' half |
| R-2 | Detection through terrain is not what the game does (§15 part 2) | A low-ground detector would detect on high ground that an ally sees | One branch in `cast`; D-2 keeps today's reading; the tests name it |
| R-3 | Goldens move twice (WP1, WP3) plus a schema re-record (WP4) | Review load, churn | One cause per re-record, each with its dump (§5); a reader sees three headers, each saying what moved and what did not |
| R-4 | B-185 remains: a victim that acquired a revealed attacker keeps it after the reveal ends | Hydras in battle 41 may keep firing beyond the window | P-14 allows the B-185 tail and records it; B-185 is registered with this interaction named |
| R-5 | `turret-wind-up-plan` touches `WeaponSpec`, its encoding and `target_acquisition.rs` | Merge conflicts, schema number | "Next free at merge" (§5 step 3); the second branch rebases its `fields!` lists |
| R-6 | Sight blockers on the showcase map change the showcase effects or skirmish | A capture storyboard or the skirmish's stage log moves | Predicted by (c); traced; the skirmish is judged by its stage log (win through input still holds) |
| R-7 | `is_detector` removal breaks readers outside `sim_core` | Compile errors in client or tools | WP2 keeps `is_detector()` as a method; the RON schema change is checked by `fixture_tests` (embedded catalogs equal the shipped files) |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: B-182, a missile drops a target that teleports** (`ValidateTeleport 1`, S2). Needs a per-entity "teleported
  on tick t" mark set by Blink's displacement and read by `track_projectile_target`. Also, what counts as a
  teleport for the flag (**verify**: Blink, `Recall`, a transport unload). Its own small plan, after this one,
  using D-1's point fallback.
- **F-2: B-183, the snapshot of a hidden ground attacker** (S5, S15). A `PlayerView` list of attack snapshots (entity,
  position and type at the strike, held for the presentation time of a snapshot, **verify**) and the client's fog
  and minimap drawing. Presentation only; no simulation rule.
- **F-3: B-185, the `Visible` filter's fog half.** A target outside its attacker's vision is lost: no firing, no
  tracking (pursuit to its last seen position), and a missile in flight lands on the point. Needs
  `is_entity_targetable` to take vision and a pursuit rule under `movement/`; its own plan once the contact-solver
  branch has merged. It is expected to move the matrix widely.
- **F-4: effect-tree missiles on target loss.** Each spell's `CEffectLaunchMissile` carries its own flags (S3's
  `NeuralParasiteLaunchMissile` clears `PointFallback`); the effect engine's `ActiveMissile` would read them per
  node. No shipped spell missile carries an area, so no outcome differs today.
- **B-184, detectors under construction** (S13): kept detecting (today) until §15 part 3 is answered.
- **Overseer, Observer and Overseer Siege Modes (13.75), Scanner Sweep and Oracle Revelation** (S9 to S11): no
  analogue or ability shipped. The `detection_radius` field (D-2) and an area reveal persistent are their hooks.

---

## 15. Draft owner question (for the orchestrator to file as Q0101; not filed by this plan)

```markdown
# Q0101: Sight blockers, detection through terrain, detectors under construction, and who sees an attack reveal

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** air units and Xel'Naga towers see into sight blockers; a detector detects only what
  it could see itself (cliffs and blockers stop detection as they stop sight); detectors under construction detect;
  an air unit's attack reveals it to the struck player only, not to that player's allies (plan
  `2026-11-23-vision-and-detection` D-2, D-3, D-5, B-184).

## Question

Space Scuffle is adopting StarCraft II 5.0.14's rules for sight blockers (nothing inside is seen from outside), the
detection radius, and the attack reveal (an air unit or Colossus that damages a unit is targetable by that unit's
player for 3 game seconds). The game data do not settle four details:

1. **Sight blockers and the air.** Does an air unit (or a Xel'Naga tower) outside a smoke or grass blocker see the
   units inside it? (a) Yes (the default; our maps treat blockers as ground-level); (b) no.
2. **Detection through terrain.** A Missile Turret stands at the foot of a cliff; a cloaked Banshee hovers over the
   high ground within 11 of the turret, and an air unit of the turret's player gives vision there. Can the turret's
   side shoot the Banshee? (a) No: a detector detects only where it can see itself (the default, our current rule);
   (b) yes: detection is a plain circle and only the vision may come from another unit.
3. **Detectors under construction.** Does a Missile Turret, Photon Cannon or Spore Crawler detect while it is still
   being built? (a) Yes (the default, our current rule); (b) no, only once complete.
4. **Attack reveal and allies.** When a Mutalisk hits a unit of player A, can player A's ally B also target the
   Mutalisk for those 3 seconds? (a) Only A (the default); (b) A and its allies.

If you can, check in the editor or a custom game: a Marine inside a smoke blocker watched by a Viking outside it; the
turret, Banshee and Overlord of part 2; a turret under construction next to a cloaked Banshee; and part 4 in a 2v2.

## Answer

```

The question number is the next free at filing (0101 on `e212ca44`).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read with `grep`, `sed` and `awk` (the `Visibility` survey of §2.3: every `<Visibility` line of
  every layer's `EffectData.xml` mapped to its enclosing effect).
- Outside sources (Q0008): S14 (Blizzard's map-elements guide, fetched 2026-11-23); S15 (the Battle.net forum
  thread, fetched 2026-11-23); S16 and S17 (Liquipedia, from search abstracts; the pages answered HTTP 429).
- The blocker cells of both shipped maps: `flags` rows with glyph `7` (`BUILDABLE | GROUND_PATHABLE | LOS_BLOCKER`),
  rows stored north first (row = 63 − y).
- No prototype: WP2 and WP4 are latent by construction, WP1's and WP3's moves are traced from dumps, and WP6's
  direction is predicted (P-14) and measured on battle 41.

---

## As built

Implemented on branch `vision-detection-plan`, one commit per package, on `main` at `36b914fd` (the turret wind-up
merged, schema 40), merged into the branch as `393cf1fc` before WP0. Evidence:
[`docs/evidence/2026-11-23-vision-and-detection.md`](../evidence/2026-11-23-vision-and-detection.md).

| WP | Commit | What landed |
|---|---|---|
| WP0 | `c45a2747` | B-54 to B-57 restated, B-182 to B-185 registered; baseline; probes (a) to (c); golden dumps |
| WP1 | `c847f873` | D-1: the `PointFallback` default; `lost_missile_tests.rs`; P-3 |
| WP2 | `be33a440` | D-2: `detection_radius` (map, catalog, data); P-4, P-5, P-7 |
| WP3 | `e52c0c83` | D-3: the blocker edge; `golden_replay` re-recorded (vision of the blocker cells only) |
| WP4 | `cf846dc3` | D-4, D-6 plumbing, D-7: `AttackVisibility`, the counters, `attack_revealed`; schema 41 |
| WP5 | `cd7da623` | Battle 41 |
| WP6 | `4b67f9c5` | D-5: the reveal, `sees_enemy` and its four readers; the data of §2.3; battle 41 re-recorded |
| WP7 | this commit | Records |

**What differs from the blueprint** (no decision changed, so no amendment):

1. **WP1 moved no golden.** §5 expected the projectile lists to move. The dumps of WP1 equal WP0's, checksums
   included: `golden_replay`'s only lost missile (tick 224) lands before the next checkpoint, and no projectile is
   in flight at any checkpoint, so the new default, hashed with each in-flight projectile's weapon, is never
   encoded. B-54 does change an encoded value (`target_loss_behavior` 0 → 1 for in-flight projectiles), which §5
   did not say; it shows only where a projectile is in flight at a checkpoint.
2. **Probe (b)'s tag** is computed at the strike and on each of the next 53 ticks, for a unit of the struck side
   whose weapon hits the attacker's plane and flags, within its scan extent (scan range or weapon range plus both
   radii), while the attacker's cell is still not `Visible` to that side. No event was tagged; battles 26 and 35
   have untagged hidden air strikes, and both stayed byte-identical in WP6.
3. **Probe (a)** found no lost missile in the showcase effects scenario (§7.3 expected Roach missiles lost to the
   Lurker analogue's burrow) and only `dead` causes anywhere.
4. **WP2's mechanical readers** were not where §10 put them: `world/presentation.rs`, `published_actions.rs` and
   `editor/tools.rs` do not read `is_detector`. The readers that do, and the struct literals that author it, are
   tests in `sim_core`, `sim_map`, `sim_fixtures` and `client`, the client's `editor/los_preview.rs` and
   `battle_pipeline/commands/tests.rs`, and `sim_fixtures`' corpus row; all moved to the method or the radius in
   WP2. Structures author the radius as `Option<f64>` and units as `Option<f32>`, as each authors `sight_radius`.
5. **WP3** adds `crates/sim_core/tests/blocker_edge_tests.rs` for the end-to-end P-8 test, and re-records
   `sim_map`'s `test_cross_platform_determinism_checksum` reference (it hashes vision states on the default arena,
   whose blocker cells changed), neither in §10's table. No measured world moved: probe (c) counted no unit in an
   edge cell anywhere.
6. **WP4 fills `attack_revealed`** from the counters already (§10 had it filled in WP6). The list stays empty until
   WP6 sets a counter, so WP4 stays latent; as a consequence the client's P-15 tests pass on WP5's tree as well.
   `battle_hud/fog.rs` needed no change (it hands the whole view to `BattleFog::refill`). `PerPlayer` gained a
   `Canonical` encoding (each slot in order). The predicate D-5 calls `sees` is `SimWorld::sees_enemy`.
7. **Battle 41 before WP6** is a stalemate, not the Void Ray win §7.4 expected: the Hydra analogues stop at the foot
   within reach, as required, but flee as helpless out of the Void Ray's reach once struck, and never fire. After
   WP6 they answer within the reveal windows and win every seed with all six left (P-14: "it loses").
8. **Schema** 41, the next free number after the turret wind-up's 40; `main` has not moved since.
9. **C7's float allowances** grow by two in `catalog.rs` (the units' `Option<f32>` and the structures'
   `Option<f64>` detection radius) and two in `catalog/validation.rs` (the radius check), both authoring
   boundaries converted to `SimScalar` at load; §5 said they would not change. The allowance list
   (`crates/workspace_checks/tests/charter.rs`) is outside §10's table.

**Open:** Q0101 is unanswered; its defaults are built (air and towers see into blockers, detection needs the
detector's line of sight, detectors under construction detect, a reveal shows the attacker to the struck player
only). B-182 to B-185 stay deferred (§14).
