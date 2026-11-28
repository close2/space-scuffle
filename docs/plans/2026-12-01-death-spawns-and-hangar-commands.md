# Blueprint: Death Spawns and Hangar Commands — Broodlings from Dying Zerg Structures (B-234) and the Carrier's Hangar Card (B-230)

This plan fixes two deferred rows of `DESIGN.md` §4:

- **B-234:** "Zerg structures spawn no Broodlings on death: 5.0.14 creates 6 or 9 when one dies
  (`ZergBuildingDies6`/`9`, `liberty` BehaviorData:1270–1285; `ZergBuildingSpawnBroodling6`/`9`, EffectData:1581–1592,
  2791–2808), each with the 8 Normal s `BroodlingFate`" (registered by plan
  [`2026-11-28-broodlings`](2026-11-28-broodlings.md) §14, which built the spawn and timed-life machinery for it).
- **B-230:** "The Carrier's hangar has no card commands: no manual interceptor build, no cancel
  (`HangarQueue5,CancelLast`), no auto-build toggle, no docked count shown (`liberty` UnitData:3521–3522); auto-build
  only" (registered by plan [`2026-11-28-carrier-interceptors`](2026-11-28-carrier-interceptors.md) D-12 A-2).

The design in one paragraph. **B-234:** an archetype may carry a **death spawn**
(`ArchetypeCapabilities::death_spawn: Option<DeathSpawnSpec>`, authored on structures as a top-level `death_spawn`
field): when a completed unit with one dies, a pending spawn is queued at its position (hashed world state, the 5.0.14
persistent's 0.5 Normal s delay), and when the delay runs out `count` units of the spawn archetype are created there
for the dead unit's owner, each with a timed life, through the broodlings plan's spawn transaction and clock. The 14
Zerg structure analogues that carry `ZergBuildingDies6`/`9` author it; the Broodling is the existing `brood_mite`. No
code names a unit, a structure or a faction. **B-230:** the hangar gains three player commands, `UnitOrder::HangarBuild`,
`UnitOrder::HangarCancelLast` and `UnitOrder::SetHangarAutoBuild { enabled }`, admitted through the economy's game
command path with typed refusals; the per-unit auto-build switch becomes hashed state on `HangarState`; `PlayerView`
gains a `hangars` list, from which the client derives three card entries on positional keys and a docked-count line in
the focused-unit panel. Neither half edits `movement/`, `steering.rs`, `movement_rules.ron` or
`target_acquisition.rs`. Each half steps the checksum schema once, to the next free number at merge.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `0c16a3a9` (checksum
schema 61).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** `main` is at schema 61. The Gated movement plan's WP2 (branch
  `gated-wp2`) may take 62 first; so WPA1 takes the next free number at its merge (**62** provisional, **63** if Gated
  WP2 lands first) and WPB1 the next free after that (**63** or **64** provisional). Every schema mention below reads
  "next free at merge". The next free bug number on `main` is **B-246** and the next free question **Q0138** (both
  verified on `main` `0c16a3a9` and on every local branch; branch `wall-clearance-wp1` names Q0138 only as a number its
  own draft would take "if it is ever filed", so the draft of §15 takes the next free number at filing). This plan
  registers no new bug row; B-246 stays free unless WPA0's census finds one.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S9, S16 |
| Q0045 (align times to SC2), Q0052 S-5 (25 ticks/s, half up) | Closed | §2.3 conversions |
| Q0020: visual questions are the agent's call | Standing rule | D-9, D-10 (card order, count line) |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0103 / owner 2026-11-24: natural movement outweighs close matchup flips | Standing guidance | §6.6 |
| Q0115 (B-214): a card entry's key is its position's key | Closed, (a) | D-9 |
| Q0129 (a), Q0130 (a, interceptors hover), Q0131 | Closed / built | B-230 builds on the hangar as built |
| Q0132 (a), Q0133 | Closed | `brood_mite` and its media exist |
| Gated coordination (`2026-11-27-gated-movement`): no other package edits `movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`' admission while a Gated package is open | Binding on this plan | D-5, D-12, stop rule 6 |
| **Draft, not filed:** "Cancelling an interceptor build while auto-build is on: re-queued at once (the data's reading)?" | Proposed (§15), default (a) followed | D-7, HC-6 |

---

## 1. Summary

| | Today (`0c16a3a9`) | After this plan |
|---|---|---|
| A Zerg structure analogue dies | Nothing is created | If it was complete: 9 ticks later, 9 (`ancient_tree_dome`, `canopy_grove_dome`, `primal_world_tree_dome`) or 6 (eleven others) `brood_mite` units appear at its centre for its owner, no order (they acquire), each living 143 + 0..5 ticks; none from the Extractor, Spine and Spore Crawler analogues, none from a foundation |
| Mechanism | — | `ArchetypeCapabilities::death_spawn` (`DeathSpawnSpec`), `SimWorld::pending_death_spawns` (hashed), reuse of the broodlings plan's spawn transaction, placement and `Entity::timed_life` |
| Carrier card | Attack, Stop, Hold | Attack (Q), **Build interceptor** (E, 15 minerals, to every selected Carrier), **Cancel build** (R, one Carrier, full refund), **Auto-build on/off** (T), Stop, Hold |
| Carrier panel | Name, health, shields | Plus a cached line: docked, out, building with progress, auto-build state |
| Simulation | Auto-build only (`HangarSpec::auto_build`, catalog) | Three instant `UnitOrder`s through game-command admission; `HangarState::auto_build` per unit (hashed); `HangarSpec::auto_build_toggle` (`AutoBuild` flag) |
| `PlayerView` | `carriers` (transports) | Plus `hangars: Vec<HangarView>` (viewer's own hangar units) |
| Worlds | — | Predicted byte-identical in every measured world (no measured world carries a Swarm structure or issues a hangar command; WPA0 and WPB0 measure it); goldens move by schema bytes only, re-recorded in each schema package |
| Size | — | M + M: WPA0 records, census, baseline; WPA1 mechanism (latent, schema); WPA2 data; WPA3 client; WPA4 confirmation. WPB0 baseline; WPB1 commands and state (schema); WPB2 client; WPB3 confirmation |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. `L Kind:N` is line N of layer L's file in SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy
at `~/scratch/combat_wp3_data/<layer>/`; layers apply in the order core, liberty, libertymulti, swarm, swarmmulti, void,
voidmulti, balancemulti, a later layer overriding a field. Conversions (Q0045, Q0052 S-5): a Normal time `t` is
`t × 25 / 1.4` ticks, rounded half up.

### 2.1 Sources

Every line below was read while planning; "no override" means a grep of all eight layers for the identifier found no
other definition and no later `CUnit` block of the structure touches its `BehaviorArray`.

**B-234**

| # | Source | What it gives |
|---|---|---|
| S1 | `ZergBuildingDies6`, `liberty BehaviorData:1270–1277`; `ZergBuildingDies9`, :1278–1285 (`CBehaviorBuff`): `InfoFlags Hidden`, **`BuffFlags DisableWhileUnderConstruction 1`**, `Modification DeathResponse Chance 1 Effect ZergBuildingSpawnBroodling6Delay` / `…9Delay`; no override | On death, with certainty, run the delayed spawn; not while the structure is under construction [DATA] |
| S2 | `SDeathResponse`, `core BehaviorData:19–21`: `Type index Remove value 0` | A death response does not fire on a `Remove` death (a unit removed without dying) [DATA] |
| S3 | `ZergBuildingSpawnBroodling6Delay`, `liberty EffectData:1581–1586`; `…9Delay`, :1587–1592 (`CEffectCreatePersistent`): `ValidatorArray CasterIsNotHidden`, **`ExpireDelay 0.5`**, `FinalEffect ZergBuildingSpawnBroodling6` / `9`; no override | The spawn happens 0.5 Normal s after the death (8.93 → **9 ticks**) [DATA] |
| S4 | `CasterIsNotHidden`, `core ValidatorData:269–272`: `WhichUnit Caster`, `Filters -;Hidden` | Refused if the dying structure is hidden [DATA]; no structure here can be hidden (D-11 A-4) |
| S5 | `ZergBuildingSpawnBroodling6`, `liberty EffectData:2791–2799`; `…9`, :2800–2808 (`CEffectCreateUnit`): `SpawnUnit Broodling`, **`SpawnCount 6`** / **`9`**, **`CreateFlags Placement 0`**, `SpawnEffect BroodlingTimedLife`, `WhichLocation TargetPoint`, `SpawnOwner Caster`; no `SetFacing`, no order effect; no override | 6 or 9 Broodlings at the death point, for the structure's owner, with no placement search and no order [DATA] |
| S6 | `BroodlingTimedLife`, `liberty EffectData:772–775` (`CEffectApplyBehavior`): `Behavior BroodlingFate`, no `UseDuration`; the Brood Lord's `BroodlingTimedLifeBroodLord` override (`voidmulti EffectData:2289–2294`) is a different effect and does not apply here | The behaviour's own duration applies [DATA] |
| S7 | `BroodlingFate`, `liberty BehaviorData:650–655`: parent `TimedLife`, **`Duration 8`**, `KillCredit Unknown`; `TimedLife`, `core BehaviorData:105–117`: **`DurationRandomMax 0.3`**, `ExpireEffect TimedLifeFate`, `Summoned`, `Permanent`; no override | 8 Normal s (142.86 → **143 ticks**) plus up to 0.3 (5.36 → **5 ticks**) at random; expiry kills with no credit [DATA] |
| S8 | `BehaviorArray Link="ZergBuildingDies9"`: `liberty UnitData:8251` (`Hatchery`, block from :8193), :8333 (`Lair`, :8274), :8412 (`Hive`, :8354). `ZergBuildingDies6`: :669 (`LurkerDenMP`, :631), :2070 (`InfestationPit`, :2032), :8478 (`EvolutionChamber`, :8430), :8920 (`SpawningPool`, :8873), :8983 (`HydraliskDen`, :8934), :9046 (`Spire`, :8996), :9117 (`GreaterSpire`, :9067), :9249 (`UltraliskCavern`, :9201), :9936 (`NydusNetwork`, :9885), :10000 (`BanelingNest`, :9952), :11799 (`RoachWarren`, :11751). None on `SpineCrawler` (`BehaviorArray` :8655–8656: `OnCreep`, `ZergBuildingNotOnCreep`), `SporeCrawler` (:8787–8790), `Extractor` (:8179), `CreepTumor`, `NydusCanal`. No later layer removes or adds either behaviour (grep of every layer's `UnitData` blocks of these units: only `AbilArray`, `LayoutButtons`, `TechAliasArray` removals and the Extractor's `WorkerPeriodicRefineryCheck`) | Which structures spawn, and how many [DATA] |
| S9 | Liquipedia, *Extractor (Legacy of the Void)* (<https://liquipedia.net/starcraft2/Extractor_(Legacy_of_the_Void)>), as quoted by a web search on 2026-11-26: "one of only three drone-made Zerg buildings that will not spawn any Broodlings when destroyed" (with the Spine and Spore Crawlers) | Agrees with S8 [EXT, medium: a search excerpt] |
| S10 | Broodling, `liberty UnitData:79–109` (`BroodlingDefault`), :11166–11193; `voidmulti UnitData:150–156` | The unit the broodlings plan already ships as `brood_mite` (that plan's S16–S18); no `Uncommandable`, `Unselectable`; `AbilArray stop`, `attack`, `move` [DATA] |

**B-230**

| # | Source | What it gives |
|---|---|---|
| S11 | Carrier `CardLayouts`, `liberty UnitData:3515–3523`: :3521 `LayoutButtons Face="Interceptor" Type="AbilCmd" AbilCmd="CarrierHangar,Ammo1" Row="2" Column="0"`; **:3522** `Face="Cancel" AbilCmd="HangarQueue5,CancelLast" Row="2" Column="4"`; :3523 `GravitonCatapult` (passive, research display); `AbilArray CarrierHangar` :3512, `HangarQueue5` :3513 | Two card commands: build one interceptor, cancel the last queued build [DATA] |
| S12 | `CarrierHangar` `InfoArray Ammo1`, `liberty AbilData:2088–2096`: `Button DefaultButtonFace="Interceptor" Requirements="ArmInterceptor"` with **`Flags ToSelection 1`**; **`Flags AutoBuild 1`**; `Charge Link CarrierInterceptor`; `Alert TrainComplete` (:2097) | The build button goes to the selection; auto-build is an autocast the player can switch [DATA for the flags; that `ToSelection` sends one build to every selected Carrier is INF, **verify**; `ArmInterceptor` is not in our data copy (no `RequirementData`), **verify** that it requires only the hangar's room] |
| S13 | `voidmulti AbilData:1294–1300`, `balancemulti :1131–1137`: `InfoArray Ammo1 Time 12`, **`Flags AutoBuildOn 1`**; `swarm AbilData:465–469`: `MaxCount 8` | Auto-build is on when the Carrier is created; 12 Normal s per build; 8 at most [DATA] |
| S14 | `HangarQueue5` (`CAbilQueue`), `liberty AbilData:2072–2076`: `QueueSize 5`, `Passive` | `CancelLast` is the queue's standard command: it removes the newest queued build [DATA for the command; that a queue cancel refunds in full, as every `CAbilQueue` train cancel does, is INF, **verify**] |
| S15 | Interceptor `CostResource Minerals 15`, `balancemulti UnitData:859–866` (carrier plan S10) | The manual build's price [DATA] |
| S16 | The docked count: not data. The game's unit panel shows a Carrier's interceptors as a row of icons (common knowledge of the game, no source quoted while planning) | Presentation; D-10 chooses a text line (Q0020) [EXT, **verify**] |

### 2.2 The rules that follow

**B-234**

1. **Who spawns** (S1, S8): the analogues of Hatchery, Lair and Hive spawn 9; of Spawning Pool, Evolution Chamber,
   Roach Warren, Baneling Nest, Hydralisk Den, Lurker Den, Spire, Greater Spire, Infestation Pit, Nydus Network and
   Ultralisk Cavern 6; of Extractor, Spine Crawler and Spore Crawler none.
2. **When** (S1–S3): on a death, if the structure was complete (not a foundation), not on a `Remove`. The spawn happens
   9 ticks after the death tick.
3. **Where and whose** (S5): at the dead structure's position, for its owner, with no placement search
   (`Placement 0`): all at one point, pushed apart by movement afterwards.
4. **What** (S5–S7, S10): Broodlings (`brood_mite`), no order, each with a timed life of 143 ticks plus 0 to 5 at
   random, expiring with no kill credit (as the broodlings plan's D-5).
5. **After that** they are ordinary units: they acquire by themselves, their owner may select and command them, they
   take no supply and cost nothing (the broodlings plan's rules 6 to 8).

**B-230**

6. **Build** (S11–S13, S15): one build per selected Carrier per press, if its docked, launched and queued members are
   below `max_count`, its queue below `queue_size`, and its owner can pay 15 minerals; paid at queueing, as auto-build
   pays.
7. **Cancel** (S11, S14): removes the newest queued build of a Carrier and refunds its cost in full; if it was the
   build in progress (the only one), its progress is lost.
8. **Auto-build** (S12, S13): a per-Carrier switch, on at creation; while on, the hangar queues builds by itself as
   today. Cancelling with auto-build on frees room that auto-build fills again on the next hangar phase if the owner
   can pay [INF: nothing in the data exempts a cancelled slot]; §15 drafts the question.
9. **Count** (S16): the owner sees each Carrier's docked, launched and building members.

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| The 14 structures and their counts (9 × 3, 6 × 11), none for Extractor and crawlers | High | S8 [DATA], S9 [EXT] |
| 9-tick delay after death | High | S3 [DATA] |
| No spawn from a foundation | High | S1 `DisableWhileUnderConstruction` [DATA] |
| No spawn on a `Remove` death | High for the data; no `Remove` death of a structure exists here | S2 [DATA]; §3 item 4 |
| All at the death point, no search | High for the flag; medium for what the engine does with a blocked point | S5 `Placement 0` [DATA]; D-4 |
| No order; they acquire | High | S5 (no `CEffectIssueOrder`), S10 [DATA] |
| 143 + 0..5 ticks | High (no `UseDuration` here, so the behaviour's own `Duration` and `DurationRandomMax` apply, unlike the Brood Lord's case) | S6, S7 [DATA] |
| Manual build to every selected Carrier | Medium, **verify** | S12 `ToSelection` [INF] |
| Cancel: newest build, full refund | Medium-high, **verify** the refund | S11, S14 [DATA, INF] |
| Auto-build switchable, on at creation | High | S12, S13 [DATA] |
| Cancel under auto-build is re-queued | Medium, **verify** | §2.2 rule 8 [INF]; draft question §15 |

---

## 3. Diagnosis (on `0c16a3a9`, read from the code)

1. **Structures have no capabilities block.** `StructureArchetypeDef` (`crates/sim_core/src/catalog.rs`) builds its
   `ArchetypeCapabilities` in `StructureArchetypeDef::capabilities` from top-level fields (`auto_rally`,
   `is_resource_dropoff`, `is_gas_extractor`, `hides_harvesters`, `abilities`); every unit capability is `None` there.
2. **Death.** `reap_dead_and_emit_shatter` (`world.rs`) runs `reap_hangar_members`, `release_housed_harvesters`, then
   `reap_transport_groups` (`transport/lifecycle.rs`), which groups each dead root with its passengers, prepares the
   group, removes a foundation's footprint from the map (`map.remove_structure`), removes the entities, recomputes
   clearance and pushes the shatter event. **No code runs on a death besides removal and the hangar's member kill.**
   A failed group preparation leaves the entity dead in the map, to be retried the next tick.
3. **Spawning** (broodlings plan, as built): `combat_resolution/impact_spawn.rs` owns `PendingSpawn`,
   `commit_one_spawn` (prepare, place, face, draw the timed life, commit, order) and `spawn_place` (the effect
   engine's ring search through `MotionPolicy`, terrain and structures only). The placement and clock draw are private
   to that module. `Entity::timed_life` counts down in `world/upkeep.rs` (`upkeep_entity`) and kills at 0.
4. **Construction and removal.** A foundation has `Entity::construction: Some(_)` and `is_completed: false`. A
   cancelled foundation is refunded and removed (`CancelConstruction`); morphs into Lair or Hive are not modelled
   (B-86: built whole). So a complete structure leaves the world only by dying.
5. **Measured worlds hold no Swarm structure.** Every start loadout (`start_loadouts.ron`) has the town hall
   `foundry_core_dome`; the four scenarios use those loadouts; the matrix fixture spawns squads; no fixture, golden,
   bench scenario or test outside `round2_visual_tests.rs` names a Swarm structure slug (grep on `0c16a3a9`). No Swarm
   structure has media (`assets/media/required_set.ron`), so no client match fields one. WPA0 confirms by census.
6. **The hangar** (`hangar/phase.rs`): `auto_build` reads `HangarSpec::auto_build` from the catalog every tick and
   queues while `queued < queue_size`, `total() < max_count` and the owner can pay; the debit is inline. Nothing
   per-unit records the switch.
7. **Commands.** `UnitOrder` (`sim_types/src/commands.rs`) carries instant game commands (`CancelProduction`,
   `CancelConstruction`, `UnloadAll`, `SetRally`) routed by `route_command` (`world/commands.rs`) to
   `dispatch_game_command` → `admit_game_command` (`economy/mod.rs`), whose outcomes carry `GameCommandError`.
   `UnitOrder` is encoded with tags 0 to 19 (`checksum/encoding.rs`); instant commands never enter an order queue, but
   the encoder's match is exhaustive.
8. **`PlayerView`** (`sim_types/src/player_view.rs`, filled by `SimWorld::write_player_view`) lists the viewer's
   transports as `CarrierView`; nothing about hangars. It is presentation, not hashed.
9. **The client card** (`client/src/battle_hud/card.rs`): `root_entries` emits Attack, modes, casts, Build, Return
   Cargo, Overclock, Unload, Cancel Build, train/research options, rallies; a slot's key is its position's
   (`COMMAND_CARD_KEYS`: Q E R T Y F G J K L Z C V B N; Stop X, Hold H fixed). `apply_card_action`
   (`battle_hud/actions.rs`) is the one function a click and a key reach. The focused-unit panel
   (`panels.rs::draw_focused_unit`) shows name, health and shield from `FocusedUnit`; strings are cached in the model.
   No right-click on card entries exists.
10. **The render-entity pool** (`client/src/render_pool.rs`) reserves `N₀ + P × (A + T)`; `T` counts hangar members
    and impact spawns with a timed life, per supply of the host. Death spawns are not in it, and structures have no
    supply, so a supply-based term cannot bound them (D-8).
11. **The tick** (`SimWorld::step_into`): commands → production and buffs (upkeep) → unit tasks → vision → hangars →
    lost swings → acquisition → attack dashes → movement → collisions → weapons and projectiles → cast locks → reap.

---

## 4. Design decisions

### D-1. B-234 is a capability on the archetype, authored on structures

On `ArchetypeCapabilities` (`crates/sim_types/src/commands.rs`):

```rust
/// Units created where this unit dies, after a delay (`StarCraft` II's `DeathResponse` that runs a
/// delayed `CEffectCreateUnit`: the Zerg structures' Broodlings; plan
/// `2026-12-01-death-spawns-and-hangar-commands` D-1). `None` for every unit that leaves nothing.
#[serde(default)]
pub death_spawn: Option<DeathSpawnSpec>,

/// What a dying unit leaves behind.
pub struct DeathSpawnSpec {
    /// The archetype created (RON: its slug).
    pub unit: UnitTypeId,
    /// Units created (`SpawnCount`), 1 to 16.
    pub count: u8,
    /// Ticks from the death to the creation (`ExpireDelay` of the persistent), 0 for the same tick's reap.
    pub delay_ticks: u16,
    /// World units around the death point searched for a valid place; 0 is no search (`Placement 0`).
    pub search_range: SimScalar,
    /// The timed life each created unit gets, if any (the broodlings plan's `TimedLifeSpec`).
    pub timed_life: Option<TimedLifeSpec>,
    /// Whether a unit that dies under construction leaves nothing (`DisableWhileUnderConstruction`).
    pub disabled_while_constructing: bool,
}
```

- **The RON form.** `StructureArchetypeDef` gains `#[serde(default)] death_spawn: Option<DeathSpawnSpecDef>` (slug,
  float range), resolved in `StructureArchetypeDef::capabilities`; `CapabilitiesDef` of units gains the same field, so
  the capability is the archetype's, whatever its kind (a later unit row may use it: §14). `TimedLifeSpec` moves from
  `sim_types::weapon` to a shared place only if a cyclic import requires it; otherwise it is reused where it is.
- **Rejected: a `CreateUnit` node in the spell effect graph, run from a death response.** It is the general home of
  5.0.14 effects, but the graph has no death hook, no persistent with a delay that outlives its caster, and no unit
  creation; the broodlings plan rejected the same node for the same reasons (its D-4). A capability whose fields are
  exactly S3 and S5's fields is the smaller, testable surface. Recorded in §14 as the future home.
- **Rejected: reuse `WeaponSpec::impact_spawn`.** A structure has no weapon that finishes; the spawn belongs to the
  death, not to a missile.

**Validation** (`catalog/validation.rs` or the broodlings plan's `catalog/strike_validation.rs`, typed `SimCoreError`
naming the slug and field `capabilities.death_spawn.<field>`): `1 ≤ count ≤ 16` (bounds the commit and the client
pool term, D-8); `search_range ≥ 0`; the unit exists, is mobile, is not a structure, has `supply_cost` 0, has no
hangar, strike magazine, impact spawn or death spawn of its own (no chains); it is in no `trains_units` (the hangar and
impact-spawn rule, as built). The host may be any archetype.

### D-2. The pending spawn (`world/death_spawns.rs`, new; hashed)

```rust
/// A spawn waiting for its delay after its host died (D-2). Hashed.
pub struct PendingDeathSpawn {
    /// Ticks left until the units are created; created when it reaches 0.
    pub due_in: u16,
    /// The dead host's archetype, whose `death_spawn` is read at creation.
    pub host: UnitTypeId,
    /// The dead host's owner, who owns the created units.
    pub owner: PlayerId,
    /// Where the host stood when it died.
    pub point: SimVec2,
    /// The dead host's heading, given to the created units (no `SetFacing` in S5: detail, D-11 A-3).
    pub heading: SimAngle,
}
```

`SimWorld` gains `pending_death_spawns: Vec<PendingDeathSpawn>`, in queue order, reserved at world creation to a
small capacity and kept at its high-water mark (PERFORMANCE RATIONALE in the code: it is drained in place, no
allocation in steady state). It is gameplay state that outlives a tick, so it is **hashed**: a new section
`"death-spawns"` after `"turret-tracking"` in `compute_simulation_checksum` (its length, then each entry's fields in
order). Reading the spec at creation through `host` keeps the entry small and makes the rule data, not a copy.

### D-3. When a spawn is queued and when it is created

- **Queued** in `reap_transport_groups`' commit branch (`transport/lifecycle.rs`), for each member id about to be
  removed, before `self.entities.remove(&id)`: if the member is dead, its archetype has a `death_spawn`, and not
  (`disabled_while_constructing` and `construction.is_some()`), push a `PendingDeathSpawn { due_in: delay_ticks, host,
  owner, point: position, heading }`. Inside the commit branch, so a group whose preparation fails (and is retried the
  next tick) queues nothing twice. Passengers dying with a carrier queue theirs too (a capability on a unit would hold;
  none is shipped). The order is the reap's: groups by root id, members by id.
- **Created** in `update_production_and_buffs` (`world/upkeep.rs`), after the per-entity upkeep loop: every entry's
  `due_in` is decremented, and an entry at 0 is committed and removed, in queue order (a stable in-place retain). A
  host that died on tick D with `delay_ticks` 9 is queued at D's reap; ticks D+1 … D+9 decrement it; it is created on
  tick **D + 9** during upkeep, before acquisition, so the created units acquire and act on that tick. A `delay_ticks`
  of 0 would be created on the next tick's upkeep (the reap is the tick's last phase); validation does not refuse 0,
  and the doc comment says so.
- **Creation** of one entry: for `count` units, `prepare_spawn(unit, owner, point, 0)`, place by the shared placement
  (D-4), heading, timed life drawn from the world's `ChaCha8Rng` (one draw per unit, in creation order, none when
  `random_extra_ticks` is 0), `commit_spawn`. **No order** (S5): the unit is idle and acquires through the existing
  acquisition. A refused transaction reports `SimulationDiagnostic::ProductionInvariant`; no place reports a new
  `SimulationDiagnostic::DeathSpawnBlocked { tick, unit, point }`; neither is silent.
- **The owner defeated meanwhile** (its last structure was the host): the spawn is still created, as production
  completes for a defeated owner and as the broodlings plan's D-9 decided; the match result is unchanged (structures
  only).

### D-4. Placement and the shared spawn helpers (`world/spawns.rs`, new)

`spawn_place` and the timed-life draw move from `combat_resolution/impact_spawn.rs` to a shared `world/spawns.rs`
(`pub(crate) fn spawn_place(&self, unit, centre, range)` and `pub(crate) fn draw_timed_life(&mut self, spec)`),
unchanged in behaviour (WPA1's latent proof covers it); `impact_spawn.rs` calls them. They read
`crate::movement::{MotionPolicy, MovementLayer}` and `movement::geometry::ground_footprint_connected` as they do today:
**read, no file under `movement/` is edited**.

- **`search_range: 0.0`** is authored (`Placement 0`, S5): the centre is the only point tried. The structure's
  footprint left the map at its reap (`map.remove_structure`, §3 item 2), 9 ticks before, and a structure stands only
  on buildable ground, so the centre is valid for a ground body (radius 0.375, at least 1.0 from any former edge of the
  smallest, 2 × 2, footprint). DS-6 proves it on a constructed map for every shipped host, found by property.
- **All `count` units at one point.** As in the game (S5), they start coincident; the existing collision phase
  separates coincident centres by its deterministic rule (`movement/solver.rs`: `+x` / `−x` by id). DS-7 checks that
  nine coincident `brood_mite` bodies on open ground are pairwise apart (edge distance ≥ −0.01) within 25 ticks with
  no movement invariant reported. **If they are not** (the rule stacks them on a line that never resolves, or a
  `MovementInvariant` fires), that is stop rule 6: no movement file is edited; the orchestrator chooses between an
  amendment that places each unit of one batch at the first ring point not taken by an earlier unit of the same batch
  (placement among units, today B-236's deferral) and waiting for Gated.

### D-5. Movement, acquisition and the Gated plan

The created units are ordinary `MoverType::Ground` entities with no order, created during upkeep at a terrain-valid
point; the next phases (acquisition, movement, collisions) handle them as any idle unit. The new code lives in
`sim_types` (`commands.rs`), the catalog and validation, `world/spawns.rs` (new), `world/death_spawns.rs` (new),
`world/upkeep.rs` (one call), `transport/lifecycle.rs` (one call), `combat_resolution/impact_spawn.rs` (calls the moved
helpers), `world.rs` (the field, its reservation), the checksum. **Not edited:** `movement/`, `steering.rs`,
`movement_rules.ron`, `target_acquisition.rs`. If any would need an edit, stop rule 6. Gated's measured worlds carry no
Swarm structure (§3 item 5), so this plan cannot move them, nor they this plan's (WPA0 re-baselines after any Gated
merge, as the broodlings and Carrier plans did).

### D-6. B-230: three instant commands (`sim_types`, `sim_core`)

`UnitOrder` gains, after `Follow` (serde appends; old replays and RON load unchanged):

```rust
/// Queue one member build in each selected hangar unit (5.0.14 `CarrierHangar,Ammo1`, a
/// `ToSelection` button; plan `2026-12-01-death-spawns-and-hangar-commands` D-6), paid now.
HangarBuild,
/// Cancel the newest queued member build of each selected hangar unit for a full refund
/// (`HangarQueue5,CancelLast`).
HangarCancelLast,
/// Switch automatic member builds of each selected hangar unit on or off (the `AutoBuild` autocast).
SetHangarAutoBuild {
    /// On or off.
    enabled: bool,
},
```

- **Routing.** `route_command` sends all three to `dispatch_game_command`, and `admit_game_command` to a new
  `admit_hangar_command` (`hangar/commands.rs`, new). They are instant: they never enter an order queue, cancel no
  order, and `queue` (Shift) is ignored, as for `CancelProduction`. The encoder gives them tags 20, 21 and 22 (22 then
  the `bool`) for exhaustiveness, as `CancelProduction` has 15; no state holds them, so no world's hash changes by the
  tags.
- **Recipients.** The usual admission (`MissingRecipient`, `ForeignRecipient`, `DeadRecipient`, `LoadedRecipient`,
  `UnselectableRecipient`) applies to each entity; recipients without a hangar are skipped; a command with no hangar
  recipient is refused with `GameCommandError::Hangar(HangarRejection::NoHangar)`.
- **Per recipient, in id order:**
  - `HangarBuild`: admitted if `queued < queue_size`, `total() < max_count`, and the owner can pay the member's cost;
    then debit and `queued += 1`. Refusals: `HangarRejection::{Full, QueueFull, Minerals, Gas}`.
  - `HangarCancelLast`: admitted if `queued > 0`; then `queued -= 1`, refund the member's cost in full; if `queued`
    becomes 0, `build_progress = 0`. Refusal: `HangarRejection::NothingQueued`.
  - `SetHangarAutoBuild { enabled }`: admitted if the spec's `auto_build_toggle`; then `state.auto_build = enabled`.
    Refusal: `HangarRejection::NoAutoBuildSwitch`.
- **Outcome.** A receipt counting the recipients changed; a refusal only when none changed, carrying the first
  recipient's reason in id order (so a group of Carriers where one is full still builds in the others). The debit
  and refund write `player_resources` exactly as the hangar phase's auto-build does: the queueing rule and debit move
  into one `HangarState`-level function, `try_queue_build(state, spec, resources, cost) -> Result<(), HangarRejection>`,
  used by both the command and auto-build, so the rule lives once.
- `HangarRejection` is a `thiserror` enum in `hangar/`, wrapped by `GameCommandError::Hangar(#[from] …)`; the client's
  feed gains its sentences (D-9).

### D-7. The auto-build switch (state, hashed; data)

- `HangarSpec` gains `#[serde(default)] auto_build_toggle: bool` (`AutoBuild`, S12: whether the player may switch it);
  `auto_build` keeps its meaning, now documented as the state at creation (`AutoBuildOn`, S13). Validation:
  `auto_build_toggle` false requires nothing; the Carrier row authors `auto_build_toggle: true` with the S12 citation.
- `HangarState` gains `pub auto_build: bool`, set from the spec at creation (`HangarState::with_capacity` takes it),
  read by the hangar phase instead of the spec. **Hashed** after `channel` (`fields!(HangarState { …, channel,
  auto_build })`).
- **Cancel under auto-build** (rule 8): nothing special; the next hangar phase's auto-build may queue again. The card
  tooltip of Cancel says so while auto-build is on (D-9). §15's draft asks the owner whether to keep the data's reading
  (default) or switch auto-build off on a cancel (a sanctioned deviation).

### D-8. Client: presentability and the pool bound for death spawns (WPA3)

1. **Presentability.** `UnitArchetype::created_units` (`catalog.rs`) also returns `capabilities.death_spawn.unit`, so a
   structure is presentable only if its spawn is (the rule the broodlings plan's WP5 built for impact spawns).
   `brood_mite` has media, so no lock changes today; the Swarm structures stay unpresentable for their own missing
   media.
2. **The pool bound.** `RenderPoolBound` gains a term `death: u32` and `total = start + death + players × (army +
   transient)`: `death` is, over the first presented snapshot's records whose archetype has a `death_spawn` with a
   timed life, the sum of their `count`, plus, per player, the largest `count` of any death-spawn archetype of the
   catalog (one structure built later dying). Structures have no supply, so no supply-based term can bound them; this
   covers every structure present at the start dying at once and one more per player, and beyond it the pool still
   spawns, counts `beyond_bound` and warns once (the pool's existing, non-silent rule). PERFORMANCE RATIONALE updated.
   Spawns without a timed life add nothing, as for impact spawns.
3. **Spawned units are ordinary units**: drawn, selectable, commandable; expiry plays the death look (as B-18).

### D-9. Client: the hangar card (WPB2)

- **Entries** (`card.rs::root_entries`), after Unload and Cancel Build and before the train and research options, for
  a selection whose completed members include a hangar unit:
  - `CardLabel::HangarBuild(member)` → `CardAction::Order(UnitOrder::HangarBuild)` (sent to the whole selection,
    `ToSelection`); price the member's cost; locked `CardLock::HangarFull` when no selected hangar unit has room in
    both its count and its queue (read from `PlayerView::hangars`); shortage of money is not a lock (the card's rule:
    the simulation's refusal explains it). Its caption names the member by the catalog (`Build <display name>`), so no
    identity is in code.
  - `CardLabel::HangarCancel` → new `CardAction::CancelHangarBuild`: `actions.rs` picks **one** recipient, the
    selected hangar unit with the most builds queued (ties to the lowest id), as UI policy beside "which structure
    trains" (5.0.14's cancel goes to one queue [INF]), and submits `HangarCancelLast` to it; locked
    `CardLock::NothingQueued` when no selected hangar unit has a build queued; its tooltip adds "Auto-build will queue
    again" while that unit's auto-build is on.
  - `CardLabel::HangarAutoBuild { on }` → `CardAction::Order(UnitOrder::SetHangarAutoBuild { enabled: !on })`, where
    `on` is true when every selected hangar unit that may switch has auto-build on; caption "Auto-build: On" / "Off".
    Shown only if some selected hangar unit's spec has `auto_build_toggle`.
- **Keys:** positional, as every entry (Q0115). For a Carrier alone: Attack **Q**, Build **E**, Cancel **R**,
  Auto-build **T**; Stop X, Hold H. 5.0.14 lays them out on its own grid (S11: row 2 columns 0 and 4); our card
  orders by entry, not by the game's grid, as for every unit (recorded, D-11 A-6).
- **Feed:** the `HangarRejection` sentences, in the feed's existing style.
- No right-click autocast: the client has none; the switch is its own entry with a key (D-11 A-7).

### D-10. Client: the docked count (WPB2)

- `PlayerView` gains `hangars: Vec<HangarView>`, sorted by entity, the viewer's own living hangar units:

```rust
/// One of the viewer's hangar units (plan `2026-12-01-death-spawns-and-hangar-commands` D-10).
pub struct HangarView {
    /// The hangar unit.
    pub entity: EntityId,
    /// Members docked, launched, and queued (the one building included).
    pub docked: u8, pub launched: u8, pub queued: u8,
    /// Members at most.
    pub max_count: u8,
    /// Ticks the build in progress has run, and ticks one build takes.
    pub build_progress: u16, pub build_ticks: u16,
    /// Whether it builds by itself, and whether the player may switch that.
    pub auto_build: bool, pub auto_build_toggle: bool,
}
```

  Written by `write_player_view` beside `CarrierView`, into the retained vector (`clear` + `push`).
- `FocusedUnit` gains `hangar: Option<HangarFocus>` (the same numbers, own units only), and `HudModel` a cached
  `hangar_line: String`, rewritten only when one of its numbers changes (the model's rule for every string): "<member
  display name>s: 6 docked, 2 out, building 1 (40 %) · auto-build on". `draw_focused_unit` draws it under the shield
  bar. An enemy Carrier shows no line: `PlayerView` holds no enemy hangar state (D-11 A-8).
- `HudModel::retained_buffers` lists `hud.view.hangars` and `hud.hangar_line`, so the steady-state test covers them.

### D-11. Approximations and deviations, each recorded

| # | What | Ours | Game | Record |
|---|---|---|---|---|
| A-1 | Coincident start | All `count` at the centre, separated by the solver's `±x` rule | All at the point (`Placement 0`), separated by the engine | None: same start; DS-7 checks the spread |
| A-2 | Placement when the centre is blocked | `DeathSpawnBlocked`, nothing created | The engine creates regardless | None expected (D-4: the centre is always free); a report would show it |
| A-3 | Facing | The dead structure's heading | Engine default (no `SetFacing`) | Detail |
| A-4 | `CasterIsNotHidden` | Not checked | Refuses a hidden caster | None: no structure can be hidden here |
| A-5 | Kill credit `Unknown`, `NoScore` | No kill accounting | — | None (broodlings plan §3 item 8) |
| A-6 | Card positions | Entry order, keys by position | Row 2, columns 0 and 4 | Detail (Q0115's rule for every card) |
| A-7 | Auto-build switch | Its own entry and key | Right-click autocast on the build button | Detail: the client has no autocast clicks |
| A-8 | Enemy Carrier's count | Not shown | Shown in the game's panel (**verify**) | Detail; a later `PlayerView` fact could add it |
| A-9 | Build with Shift | One build | Shift-click trains five for queues (**verify** for this button) | Detail |
| A-10 | `Alert TrainComplete` | No alert | An alert on each build | Detail (no build alerts exist in the client) |

---

## 5. Determinism, checksum and coordination

- **Integers only.** Counts are `u8`/`u16`; points and headings fixed point; one integer draw per created unit from the
  world's `ChaCha8Rng`. No float enters state; `HangarView` and the HUD line are presentation.
- **Order.** Death spawns queue in reap order (groups by root id, members by id) and are created in queue order; ids
  from `next_entity_id` in that order; draws in that order. Hangar commands act in command order, recipients in id
  order.
- **No new collection type, clock or global.** `pending_death_spawns` is a `Vec`; no map.
- **Hashed:** WPA1 adds the `"death-spawns"` section; WPB1 adds `HangarState::auto_build`. Catalog fields
  (`DeathSpawnSpec`, `auto_build_toggle`) are catalog data, not hashed. `checksum.rs` gains one schema line per
  package: "Schema N adds the world's pending death spawns, `SimWorld::pending_death_spawns` (a section
  `death-spawns`: its length, then each entry's countdown, host type, owner, point and heading), after the turret
  tracking (plan `2026-12-01-death-spawns-and-hangar-commands` D-2). The number is the next free at merge." and
  "Schema M adds each hangar's auto-build switch, `HangarState::auto_build` (a `bool`), after `channel`, and the
  `UnitOrder` tags 20 to 22 (… D-6, D-7). The number is the next free at merge."
- **The 4-build golden proof (each schema package, WPA1 and WPB1).** With `CHECKSUM_SCHEMA_VERSION` at `main`'s number
  and the package's new fields left out of the encoding in a proof copy (the broodlings plan's
  `~/scratch/broodlings-plan/scripts/proofcopy.sh` and `unhash.py`, copied to `~/scratch/death-spawns-plan/scripts/`
  and pointed at the new section or field), the package's tree reproduces every recorded checkpoint of
  `golden_replay.ron`, `golden_replay_orders.ron` and `DEFAULT_SANDBOX_CHECKSUM` (`sim_fixtures/tests/fixture_tests.rs`)
  on **native debug, native release, `wasm32-wasip1` release and `wasm32-wasip1` debug** (8 of 8 values). Then the
  schema steps to the next free number at merge, and **the same package re-records** the goldens and the sandbox under
  the header "Re-recorded for plan 2026-12-01-death-spawns-and-hangar-commands WPA1 (checksum schema M -> N): pending
  death spawns encoded; with the schema at M and the section unhashed every checkpoint reproduced. Behaviour: none in
  these worlds (no Swarm structure)" (WPB1 likewise for the switch), and proves the re-recorded values on the four
  builds. So every package leaves `main` green and merges alone. If `main` takes N first, the merge repeats the proof
  and the re-record with the next free number (the "next free at merge" rule); if Gated WP2 is on `main` before
  WPA1's merge, WPA1 merges `main` first and proves against `main`'s values.
- **Replays.** The new `UnitOrder` variants are appended, so every replay file and `TickInputBundle` written before
  loads unchanged and replays identically (DS-12, HC-10); a replay recorded under the older schema is refused by its
  schema, as for every step. Replays that use hangar commands are new; `golden_replay_orders.ron` is not extended (its
  world has no Carrier; adding one changes the golden's content, which is not this plan's purpose); instead HC-10
  records, serializes and replays a constructed fight with all three commands and checks the checksum of every tick,
  and the four-build proof runs it in release and on `wasm32-wasip1` through `golden_replay_tests.rs`' existing harness
  if that harness takes a replay path (**verify** in WPB1; otherwise HC-10 runs native only and the record says so).
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, the base and final trees interleaved, two
  rounds each: every scenario at **50 ticks per second or more**, instructions of the existing scenarios within
  **+0.5 %** (`perf stat -e instructions:u`, `DESIGN.md` §5's method). The new per-tick work is one empty-vector check
  in upkeep and one `Option` check per reaped entity. WPA4 adds a **`razed`** row to `throughput_scenarios.ron`: the
  200-unit `pulse_bot` battle plus 12 structures with a death spawn of 9 (found by property) owned by the second army
  and in the first army's path, so bursts of 9 creations and their expiries are measured; it must run at ≥ 50
  ticks/s, and its figure enters `DESIGN.md` §5. The hangar commands are not on a per-tick path (no row).
- **Gated coordination** as D-5; WPB touches `hangar/`, `world/commands.rs` (routing), `economy/mod.rs` (dispatch and
  the error), `player_view.rs`, the encoding, the client. `entity.rs`, `checksum.rs` and `encoding.rs` are also in
  Gated's file list; those are merge conflicts resolved by "next free at merge", not coordination breaches.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` and use shipped archetypes
from `sim_fixtures` found by property (a structure with a death spawn of the largest count; one with the smallest; a
structure without one; a hangar unit), never by slug, except the shipped-values tables. A scratch catalog copy supplies
values the shipped data lacks (WPA1, before the data exists). Every test is written red first against the previous
package's tree.

### 6.1 Death spawns, mechanism (WPA1; unit tests in `world/death_spawns.rs`, `world/spawns.rs`, validation; integration in new `crates/sim_core/tests/death_spawn_tests.rs`)

On a scratch catalog giving one complete structure `death_spawn: (unit: <the impact-spawn unit>, count: 9,
delay_ticks: 9, search_range: 0.0, timed_life: (143, 5), disabled_while_constructing: true)`:
- **DS-1 (timing and count).** Killed on tick D (damage through `PlayerCommand` attacks): exactly 9 units of the spawn
  archetype exist from tick D + 9, none on D + 8; owned by the structure's owner; with no order; at the dead
  structure's position (before separation); `pending_death_spawns` holds one entry from D's reap to D + 8, none after.
- **DS-2 (foundation).** A foundation killed: nothing queued, nothing created; with `disabled_while_constructing:
  false` (scratch): created.
- **DS-3 (cancel).** A cancelled foundation creates nothing (no death).
- **DS-4 (timed life).** Each created unit dies on tick D + 9 + 143 + d, d in [0, 5], with no damage source and its
  death event; over 200 created units every d of 0 to 5 occurs and no other; the same seed gives the same d's.
- **DS-5 (ordering and ids).** Two structures dying on one tick: ids in reap order (lower host id first); the same seed
  gives the same ids and draws; a structure dying on D and another on D + 3 create on D + 9 and D + 12.
- **DS-6 (placement).** For every shipped archetype with a death spawn (found by property; WPA2 adds them, so in WPA1
  on the scratch catalog with footprints 2 × 2, 3 × 3 and 5 × 5), placed on a constructed map and killed: all units
  are created at the centre, no `DeathSpawnBlocked`. A scratch host whose centre is made invalid (a wall placed in the
  delay) reports `DeathSpawnBlocked` and creates nothing.
- **DS-7 (separation).** Nine coincident created units on open ground are pairwise apart (edge distance ≥ −0.01)
  within 25 ticks with no `MovementInvariant` diagnostic (stop rule 6 if not, D-4).
- **DS-8 (behaviour).** The created units acquire and attack an enemy in their scan range on their creation tick or
  the next, and an enemy's automatic targeting attacks them; a `PlayerCommand` move to one of them is admitted for
  their owner.
- **DS-9 (owner defeated).** The host is its owner's last structure: the owner is defeated at D's reap, the units are
  still created on D + 9, and the match result does not change.
- **DS-10 (supply and economy).** Created units capture no supply; the supply ledger validates every tick of the fight.
- **DS-11 (validation).** Each D-1 rule refused with a typed error naming the field; an omitted `death_spawn` reads
  `None`; the shipped catalog validates.
- **DS-12 (encoding and replay).** Two worlds differing only in one field of one pending entry have different
  checksums; a constructed fight recorded, serialized to RON and replayed gives the same checksum on every tick.

### 6.2 Death spawns, data (WPA2; `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs`, `death_spawn_tests.rs`)

- **DS-13 (reference values).** A table per shipped structure, keyed by its `sc2_inspiration`, of `count` (9 for
  Hatchery, Lair, Hive; 6 for the eleven of S8; none for Extractor, Spine Crawler, Spore Crawler), `delay_ticks` 9,
  `search_range` 0, timed life 143 + 5, `disabled_while_constructing`, each row citing S1, S3, S5, S7 and its S8 line;
  and the unit is the archetype whose `sc2_analogue` names the Broodling.
- **DS-14 (zero recompilation).** In a catalog directory copy, changing a structure's `count` to 2 or `delay_ticks` to
  20 changes DS-1's count or creation tick in the same binary.
- **DS-15 (shipped fight).** A shipped structure with the largest count, found by property, killed by a constructed
  army: DS-1 holds with the shipped values; the created units fight the army.

### 6.3 Death spawns, client (WPA3; client tests)

- **DS-16 (presentable).** With the spawn's media absent from a fixture root, a structure with a death spawn is
  unpresentable (`NoMedia`); with both present (fixture atlases in a temporary root), presentable. Scratch media only,
  never under `assets/`.
- **DS-17 (bound).** `render_pool.rs` unit tests: `death` equals the start records' counts plus the per-player largest
  count; a catalog without death spawns gives 0 and leaves the shipped bound unchanged otherwise.
- **DS-18 (steady state).** `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` (valgrind) gain a
  `Razing` case: a constructed session (fixture media root) where one structure with a death spawn of 9 dies every 200
  ticks for 1,200 ticks; after the first death and its expiries, no allocation in the battle sync, no growth of any
  retained buffer, `beyond_bound` 0, every arrival reusing a parked entity.

### 6.4 Hangar commands, simulation (WPB1; `hangar_tests.rs` (extended), unit tests in `hangar/commands.rs`)

On the shipped Carrier analogue found by property (an archetype with a hangar and a launch weapon):
- **HC-1 (build).** `HangarBuild` with 4 docked: queued 1, 15 minerals debited, the member docks 214 ticks later; with
  auto-build off and repeated, queues to `min(queue_size, max_count − total)`, then `QueueFull` or `Full`.
- **HC-2 (to the selection).** Two Carriers selected, one full: the other queues one; the receipt counts 1. Both full:
  refused `Full`. Not enough minerals for the second: the first queues, the outcome counts 1.
- **HC-3 (cancel).** Three queued: `HangarCancelLast` → 2 queued, 15 refunded, progress kept; one queued and building:
  → 0 queued, progress 0, refunded; none queued: `NothingQueued`.
- **HC-4 (switch).** `SetHangarAutoBuild { enabled: false }`: no build is queued by itself afterwards (members killed,
  resources ample); `true` again: queues on the next hangar phase. On a scratch hangar with `auto_build_toggle: false`:
  refused `NoAutoBuildSwitch`.
- **HC-5 (creation).** A Carrier trained or spawned has `auto_build` equal to the spec's (`true` shipped).
- **HC-6 (cancel under auto-build).** Auto-build on, a build cancelled: refunded, and re-queued (and charged) on the
  next hangar phase if the owner can pay (the data's reading, §15 default).
- **HC-7 (admission).** A command naming only non-hangar units: `NoHangar`; a foreign, dead or unselectable recipient:
  the existing typed errors; `queue: true` behaves as `false`; a defeated player's or a closed match's command: the
  existing refusals.
- **HC-8 (view).** `write_player_view` lists each of the viewer's hangar units with its numbers, sorted, and none of
  another player's.
- **HC-9 (validation, encoding).** `auto_build_toggle` omitted reads `false`; the shipped catalog validates; two worlds
  differing only in one Carrier's `auto_build` have different checksums; the three orders encode distinct tags.
- **HC-10 (replay).** A constructed fight with every command recorded, serialized to RON, read back and replayed: the
  same checksum on every tick; a replay file written before WPB1 (the current `golden_replay_orders.ron`) loads
  unchanged.

### 6.5 Hangar commands, client (WPB2; `battle_card_tests.rs`, `battle_hud_tests.rs`, `battle_input_tests.rs`, `card.rs` unit tests, `battle_hud_window_tests.rs`, steady state)

- **HC-11 (card).** A Carrier selected (spawned for the viewer by property in the harness): the root shows Attack,
  Build, Cancel, Auto-build at positions 0 to 3 with keys Q, E, R, T; Build carries the member's price; Cancel is
  locked `NothingQueued` until a build is queued; Build locked `HangarFull` at the maximum; Auto-build reads "On".
- **HC-12 (keys and clicks).** Pressing E (and clicking the slot) submits one `HangarBuild` with the whole selection;
  R submits `HangarCancelLast` to the selected Carrier with the most queued; T submits `SetHangarAutoBuild { enabled:
  false }`, after which the entry reads "Off"; a refusal reaches the feed with its sentence.
- **HC-13 (mixed selection).** A Carrier with army units: the hangar entries appear once, keys by position; a selection
  without a hangar unit shows none.
- **HC-14 (count).** The focused Carrier's panel line reads docked, out, building and auto-build from the view, is
  rewritten only when a number changes (the string's pointer and capacity unchanged across 100 frames with no change),
  and is absent for an enemy Carrier.
- **HC-15 (GPU, window geometry).** `battle_hud_window_tests.rs` gains a child case (through `support/gpu_run.rs`,
  which takes the GPU lock itself, one child at a time): the production skirmish with a Carrier spawned for the viewer
  and selected before the first frame reports the rectangles of the three hangar slots and the count line; the parent
  asserts each lies inside the command panel and none overlaps another. Geometry, not pixels (the test file's stated
  limit: egui panels are flat on the capture path).
- **HC-16 (steady state).** The render-entity pool plan's `HangarCycle` pair (`sync_steady_state_tests`,
  `sync_allocation_count_tests` under valgrind) runs with a Carrier selected and focused, so the card and the count
  line are derived every refresh while members cycle: no allocation in steady state, no retained buffer grows.

### 6.6 World check and close battles (pre-registered)

Every world that differs from the previous package's is explained by its **first difference** (`firstdiff.py` and a
per-entity trace) and classed:
- **S (schema bytes):** the goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the encoding alone (the package's proof).
- **R (razing):** the first difference is a pending death spawn or a unit it created (WPA2), allowed only in a world
  WPA0's census lists as holding a structure with a death spawn that dies.
- **P (presentation):** a `PlayerView` or fog frame differs in presentation only (WPB1's `hangars` list).
- **Anything else:** a defect, stop rule 2. No package of this plan predicts any class but S and P.

The close-battle list is fixed by WPA0 from its baseline alone, by the rule of the broodlings and Carrier WP0s (every
matrix battle whose minority side wins at least 2 of its 20 baseline runs; battles 31 and 33 at the same share over
100 and 64 seeds; every battle a closed owner question or fixture comment names close or restated; no other; a
`known_bug` battle is not close), and frozen when WPA0 commits. WPB0 reuses it. **Any outcome move** (a run whose
winner changes, or an expectation that fails on a seed where it held) **is stop rule 4**: none is predicted, so none is
restated in advance; if one appears with a first difference of class R in a listed world, the orchestrator decides on
a §8 restatement with the trace and the S1–S8 source before the package continues.

### 6.7 Predictions (recorded before measuring, not thresholds)

| World | WPA1 | WPA2 | WPA3 | WPB1 | WPB2 |
|---|---|---|---|---|---|
| Matrix (seeds of record, 16 development seeds) | identical | identical | identical | identical | identical |
| Battles 31, 33 | identical | identical | identical | identical | identical |
| Showcase, showcase effects (default and 1–8), skirmish (6,000 ticks) | identical | identical (no Swarm structure; WPA0 measures) | identical | identical | identical |
| Goldens, sandbox | S, proved, re-recorded | identical | identical | S, proved, re-recorded | identical |
| `PlayerView` frames | identical | identical | identical | P (`hangars` filled where a Carrier exists; none measured) | identical |
| Client skirmish stage log | identical | identical | identical | identical | identical |

---

## 7. Measurement

### 7.1 WPA0 census and probe (scratch, not committed)

- **(c) Census.** `~/scratch/death-spawns-plan/census/`: the broodlings census script copied and extended to print, per
  layer with file and line, every element of S1–S8 (both behaviours, both persistents, both create effects,
  `BroodlingTimedLife`, `BroodlingFate`, `TimedLife`, `CasterIsNotHidden`, `SDeathResponse`, and every `CUnit` block of
  the 19 Zerg structures of §2.1 S8 in every layer, with any `BehaviorArray` entry or removal); and S11–S14 for the
  Carrier. Any difference from §2.1 is stop rule 9. A second listing prints every place a structure with a (planned)
  death spawn can enter a world: loadouts, scenarios, the matrix fixture, battles 31 and 33, both goldens, the sandbox,
  throughput scenarios, and every builder whose raise list names one (the AI and the card).
- **(p) Probe** (`~/scratch/death-spawns-plan/probe/`, from the broodlings probe; it may name slugs, being scratch):
  per world and seed, changing no state, every death of a Swarm structure (tick, type, complete or not) and every
  hangar unit present. **Predicted zero in every measured world.** A positive control (a scratch test killing a Swarm
  structure analogue) proves a zero is a measured zero.
- **Close-battle list** (§6.6, frozen), **baseline** on four builds (both goldens and `DEFAULT_SANDBOX_CHECKSUM`),
  **throughput and instructions**.

### 7.2 Commands

The broodlings plan's scripts, copied to `~/scratch/death-spawns-plan/scripts/` with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/death-spawns-plan
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
python3 -I $R/scripts/matrixdiff.py $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7). Each measured package compares its worlds with the previous package's; a package whose base on `main`
moved (a Gated merge in between) re-measures its base after merging `main`.

**How each world is measured:** a **world comparison against `main`** (every matrix battle at its seeds of record and
the 16 development seeds, battles 31 and 33 at their seeds, the showcase and showcase-effects worlds at the default
seed and seeds 1 to 8, the skirmish scenario for 6,000 ticks, both goldens and the sandbox on four builds, the client
skirmish stage log), byte for byte on the per-tick checksums and the outcome records; **a first-difference trace for
any world that moves** (`firstdiff.py` to the first differing tick, then `trace.sh` per entity at that tick, classed
by §6.6); **claims restated per §8** only under §6.6's rule, with the moved share, one traced seed and the S-source.

### 7.3 Held-out seeds (WPA4 and WPB3, confirmation only)

Once each, on `main` and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase and
showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12. Byte-identical expected, or class R only in
worlds the census lists; anything else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. A package's latent proof fails (a checkpoint differs with the schema at `main`'s number and the new section or
   field unhashed): stop; something other than the encoding moved.
2. A measured world's first difference is not of class S, R or P (§6.6), or R in a world the census did not list:
   stop, trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. Any outcome moves (§6.6): stop for review with the trace.
5. Throughput below 50 ticks/s in any scenario (the `razed` row included), or instructions above +0.5 % on the
   existing scenarios: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit, or DS-7 fails: stop and report to the orchestrator (D-4).
7. The client needs a change beyond D-8 to D-10, or a steady-state or allocation test fails (DS-18, HC-16): stop;
   never fix by excluding the case from the test.
8. A data change is needed beyond `structures.ron`'s `death_spawn` fields and the Carrier's `auto_build_toggle`: stop
   (the plan names no new unit, no media).
9. WPA0's census contradicts §2.1 (another count, delay, lifetime, structure list, or a layer that removes a
   behaviour): stop WPA0; amend the plan first.
10. §15's draft is answered other than (a) before WPB1 lands: stop WPB1 and amend D-7 and HC-6 first.

---

## 9. Client consequences

WPA3: presentability through `created_units`, the pool's `death` term, DS-16 to DS-18. WPB2: card entries, actions,
feed sentences, `HangarView` in the model, the count line, HC-11 to HC-16. No shader, renderer or new asset; the one
GPU test is HC-15's geometry child, which locks the GPU through `gpu_run.rs`. The diagnostics and GPU suites run in each
client package's gate through their catalogue triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 B-234, B-230 restated (WPA0, WPB0), fixed (WPA4, WPB3); §5 `razed` row; Combat row names death spawns | WPA0, WPA4, WPB0, WPB3 |
| `docs/evidence/2026-12-01-death-spawns-and-hangar-commands.md` | New: census, probe, close list, baselines, proofs, worlds, held-out, bench | all |
| `crates/sim_types/src/commands.rs` | `DeathSpawnSpec`, `ArchetypeCapabilities::death_spawn` (WPA1); `UnitOrder::{HangarBuild, HangarCancelLast, SetHangarAutoBuild}`, `HangarSpec::auto_build_toggle` (WPB1) | WPA1, WPB1 |
| `crates/sim_types/src/player_view.rs` | `HangarView`, `PlayerView::hangars` | WPB1 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` (or `strike_validation.rs`) | Defs, resolution, `created_units`, D-1 rules | WPA1, WPA3 (`created_units`) |
| `crates/sim_core/src/world/spawns.rs` (new) | Shared placement and timed-life draw | WPA1 |
| `crates/sim_core/src/world/death_spawns.rs` (new) | `PendingDeathSpawn`, queue, creation | WPA1 |
| `crates/sim_core/src/combat_resolution/impact_spawn.rs` | Calls the moved helpers | WPA1 |
| `crates/sim_core/src/transport/lifecycle.rs` | Queue at the reap commit | WPA1 |
| `crates/sim_core/src/world/upkeep.rs`, `world.rs` | One call; the field and its reservation | WPA1 |
| `crates/sim_core/src/weapon_timing.rs` (diagnostics) | `DeathSpawnBlocked` | WPA1 |
| `crates/sim_core/src/hangar/{mod,phase}.rs`, `hangar/commands.rs` (new) | `auto_build` state, `try_queue_build`, admission, `HangarRejection` | WPB1 |
| `crates/sim_core/src/world/commands.rs`, `economy/mod.rs` | Routing; `GameCommandError::Hangar` | WPB1 |
| `crates/sim_core/src/player_view.rs` | `hangars` | WPB1 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema lines; section; field; tags | WPA1, WPB1 |
| `crates/sim_core/tests/death_spawn_tests.rs` (new) | DS-1 to DS-12, DS-15 | WPA1, WPA2 |
| `crates/sim_core/tests/{catalog_reference_values_tests,runtime_catalog_tests,checksum_state_tests,hangar_tests}.rs` and literals listing every `ArchetypeCapabilities`/`HangarSpec`/`UnitOrder` field or variant | DS-11, DS-13, DS-14, HC-1 to HC-10 | WPA1, WPA2, WPB1 |
| `assets/catalog/structures.ron` | `death_spawn` on 14 Swarm structures, with S-citations | WPA2 |
| `assets/catalog/units.ron` | Carrier `auto_build_toggle: true` with S12 | WPB1 |
| `crates/client/src/render_pool.rs`, `media.rs` (if `created_units` is not already its only source) | D-8 | WPA3 |
| `crates/client/src/battle_hud/{card,actions,model,panels,feed}.rs` | D-9, D-10 | WPB2 |
| `crates/client/tests/{sync_steady_state_tests,sync_allocation_count_tests,battle_card_tests,battle_hud_tests,battle_input_tests,battle_hud_window_tests}.rs`, support | DS-16 to DS-18, HC-11 to HC-16 | WPA3, WPB2 |
| `crates/sim_core/benches/throughput_scenarios.ron` | `razed` row | WPA4 |
| Goldens, `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded in each schema package | WPA1, WPB1 |
| `crates/gate/suites.ron` | `death_spawn_tests` only if not selected as a default fast target of `sim_core` (the broodlings plan found it needs no entry) | WPA1 |
| `docs/plans/README.md` | Row: Proposed (plan), Active (WPA1), Done (WPB3) | plan, WPA1, WPB3 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`effects/`, `catalog/spells.rs`, every shader, every asset under `assets/textures/` and `assets/videos/`.

---

## 11. Work packages (commit order)

Each package is red first, then the change, then `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). **Each package
leaves `main` green and can merge alone**: the schema packages re-record their own goldens (§5), the data package
needs only the mechanism, the client packages only their simulation package. B-234's packages come first (no UI); B-230
may start once WPA1 has merged (both edit the encoding and the schema line). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/death-spawns-plan/`. GPU tests lock the GPU themselves; `cargo test` is never wrapped in `with-gpu-lock`.

### WPA0: records, census, probe, baseline (XS, docs and scratch)

§7.1. `DESIGN.md` §4 B-234 restated with S1–S8 and this plan. **Acceptance:** stop rule 9 holds; the evidence lists the
census, the probe's zeros with the control, the frozen close list, the four-build baseline, throughput and
instructions. Commit `docs(design): restate B-234 (plan 2026-12-01-death-spawns-and-hangar-commands WPA0)`.

### WPA1: death spawns, latent, schema (M, `sim_types`, `sim_core`)

D-1 to D-5. Red first: DS-1 to DS-12 on the scratch catalog. **Acceptance:** they pass; every measured world
byte-identical to WPA0's; latent proof 8 of 8; schema stepped to the next free at merge; goldens and sandbox
re-recorded and proved on four builds; stop rules 1 and 6 hold. Commit `feat(sim_core): death spawns with a delay and
a timed life (B-234; latent)`.

### WPA2: the Swarm structures' Broodlings (S, data and tests)

`structures.ron`: on each of the 14 structures, after `abilities`:

```ron
// 5.0.14 ZergBuildingDies9 (liberty BehaviorData:1278-1285, DisableWhileUnderConstruction) on Hatchery
// (liberty UnitData:8251): ZergBuildingSpawnBroodling9Delay ExpireDelay 0.5 = 9 ticks (liberty
// EffectData:1587-1592), ZergBuildingSpawnBroodling9 SpawnCount 9, Placement 0, TargetPoint, Caster
// (:2800-2808); BroodlingTimedLife -> BroodlingFate Duration 8 = 143 ticks (liberty EffectData:772-775,
// BehaviorData:650-655), TimedLife DurationRandomMax 0.3 = 5 ticks (core BehaviorData:105-117);
// plan 2026-12-01-death-spawns-and-hangar-commands D-1 (B-234).
death_spawn: Some((unit: "brood_mite", count: 9, delay_ticks: 9, search_range: 0.0,
                   timed_life: Some((ticks: 143, random_extra_ticks: 5)),
                   disabled_while_constructing: true)),
```

(9 for `ancient_tree_dome`, `canopy_grove_dome`, `primal_world_tree_dome`; 6, citing `ZergBuildingDies6` and its
lines, for `spawning_hollow_dome`, `metamorphic_chamber_dome`, `carapace_mound_dome`, `caustic_nursery_dome`,
`spine_den_dome`, `burrow_spire_dome`, `chitin_spire_dome`, `apex_spire_dome`, `infestation_nursery_dome`,
`subterranean_network_dome`, `behemoth_den_dome`; none for `spore_vent_dome`, `thorn_lasher_dome`, `spore_spire_dome`).
Red first: DS-13 to DS-15. Worlds against WPA1's. **Acceptance:** tests pass; worlds byte-identical (no measured world
kills a Swarm structure); stop rules 2, 4, 8 hold. Commit `fix(catalog): Zerg structure analogues leave 6 or 9
Broodlings when they die (B-234)`.

### WPA3: client for death spawns (S, `client`)

D-8. Red first: DS-16 to DS-18. **Acceptance:** they pass; stop rule 7 holds. Commit `feat(client): death-spawned units
are presentable and inside the render pool's bound (B-234)`.

### WPA4: confirmation and records for B-234 (XS)

Held-out seeds (§7.3); the `razed` throughput row and the comparison (§5); `DESIGN.md` §4 B-234 **fixed**, §5 row,
Combat row; `## As built` (part A). **Acceptance:** stop rules 3 and 5 hold. Commit `docs: close B-234 (plan
2026-12-01-death-spawns-and-hangar-commands WPA4)`.

### WPB0: baseline for B-230 (XS, docs and scratch)

After WPA1 is on `main`: the four-build baseline and the world baseline re-taken on `main`; `DESIGN.md` §4 B-230
restated with S11–S16. Commit `docs(design): restate B-230 (plan … WPB0)`.

### WPB1: hangar commands and the switch, schema (M, `sim_types`, `sim_core`, data)

D-6, D-7, D-10's `PlayerView` half; the Carrier's `auto_build_toggle: true`. Red first: HC-1 to HC-10. **Acceptance:**
they pass; worlds byte-identical (class P only in `PlayerView` frames of worlds with a Carrier: none measured); latent
proof 8 of 8 with `auto_build` unhashed and the tags present (tags never reach state); schema stepped and goldens
re-recorded and proved; stop rules 1, 2, 10 hold. Commit `feat(sim_core): hangar build, cancel and auto-build commands
(B-230)`.

### WPB2: the card and the count (M, `client`)

D-9, D-10. Red first: HC-11 to HC-16. **Acceptance:** they pass, HC-15 in its GPU child; stop rule 7 holds. Commit
`feat(client): the hangar's card commands and docked count (B-230)`.

### WPB3: confirmation and records for B-230 (XS)

Held-out seeds; throughput comparison (no new row); `DESIGN.md` §4 B-230 **fixed**; carrier plan D-12 A-2 gains the
pointer; `## As built` (part B). Commit `docs: close B-230 (plan … WPB3)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `commands.rs`, `player_view.rs` | RON forms and defaults of `DeathSpawnSpec`, `auto_build_toggle`, the new orders and `HangarView` |
| Unit | `sim_core` `world/death_spawns.rs`, `world/spawns.rs`, `hangar/commands.rs`, validation, encoding | Countdown and drain order, placement, rejections, D-1 rules, tags |
| Integration | `death_spawn_tests.rs` (new), `hangar_tests.rs` | DS-1 to DS-12, DS-15; HC-1 to HC-10 through `SimWorld` and `PlayerCommand` |
| Integration | `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs`, `checksum_state_tests.rs` | DS-11, DS-13, DS-14, HC-9 |
| Determinism | goldens, sandbox, four builds | §5 proofs, one re-record per schema package |
| Client | `render_pool.rs` units, `battle_card_tests.rs`, `battle_hud_tests.rs`, `battle_input_tests.rs`, `card.rs` units | DS-16, DS-17, HC-11 to HC-14 |
| GPU | `battle_hud_window_tests.rs` (child through `gpu_run.rs`) | HC-15 |
| Steady state | `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) | DS-18 (`Razing`), HC-16 (`HangarCycle` with the card focused) |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s (the `razed` row included), ≤ +0.5 % instructions |

---

## 13. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 B-234 | Restated (S1–S9, this plan); **fixed** with commits and figures | WPA0, WPA4 |
| `DESIGN.md` §4 B-230 | Restated (S11–S16, this plan); **fixed** | WPB0, WPB3 |
| `DESIGN.md` §5, Combat row | `razed` row; death spawns and hangar commands named | WPA4, WPB3 |
| `docs/plans/2026-11-28-broodlings.md` §14, `2026-11-28-carrier-interceptors.md` D-12 A-2 | "*Taken up by plan `2026-12-01-death-spawns-and-hangar-commands`.*" | WPA4, WPB3 |
| New evidence file | §7 | all |
| `docs/questions/open/…` | §15's draft filed by the orchestrator if it accepts it (number next free at filing) | plan acceptance |
| `docs/plans/README.md` | Row `Proposed`, `Active`, `Done` | plan, WPA1, WPB3 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **Other death responses**: the Baneling's `VolatileBurst` (`liberty` BehaviorData:1146) is modelled by its own
  weapon today; Infested Terran eggs (`swarm` BehaviorData:915), Adept shades, Locusts, Auto-Turrets: none of our rows
  needs them; `death_spawn` is general enough for a unit row that spawns on death.
- **A `CreateUnit` node in the spell graph** with death and finish hooks: the future home of both `impact_spawn` and
  `death_spawn` when a third creator needs it (D-1).
- **Placement among units** (B-236): kept terrain-only; D-4's fallback is an amendment, not part of the plan.
- **Shift-queued builds, build alerts, the enemy's hangar count** (A-8 to A-10): details.
- **Hangar upgrades** (Graviton Catapult, capacity, leash): as the Carrier plan's §14.
- **A matrix battle with a Swarm base or a Carrier**: needs a sourced expectation; left to the matrix capability work.

---

## 15. Owner question (filed as [Q0138](../questions/open/0138-hangar-cancel-auto-build.md))

```markdown
# Q0138: Cancelling an interceptor build while auto-build is on: let auto-build queue it again?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (a); a cancelled build is refunded, and auto-build queues (and charges) a new one on the
  next tick if the player can pay

## Question

The Carrier analogue builds its interceptors by itself while "auto-build" is on (StarCraft II 5.0.14 turns it on for
every new Carrier). The player will be able to build one by hand, cancel the newest build for a full refund, and switch
auto-build off. The game data says nothing about what a cancel does to auto-build, so with auto-build on a cancel frees
a slot that auto-build fills again at once: the cancel only helps after switching auto-build off. (Plan
`2026-12-01-death-spawns-and-hangar-commands` D-7.)

- (a) **Follow the data's reading:** cancel refunds; auto-build may queue again. **Recommended**: it is what the data
  implies, and the cancel button's tooltip says "Auto-build will queue again" while auto-build is on.
- (b) **A cancel also switches auto-build off** for that Carrier: friendlier, a recorded deviation that allows more.

## Answer

```

(The number is the next free at filing; branch `wall-clearance-wp1` may file its own draft as Q0138 first.)

---

## 16. Material (scratch, not committed)

- `~/scratch/combat_wp3_data/`: the 5.0.14 data of §2.1, every line re-read on 2026-11-26.
- `~/scratch/broodlings-plan/scripts/`: the measurement, proof and bench scripts this plan copies.
- Code read on `0c16a3a9`: `catalog.rs` (`StructureArchetypeDef`, `capabilities`, `created_units`), `sim_types`
  `commands.rs` (`UnitOrder`, `ArchetypeCapabilities`, `HangarSpec`), `weapon.rs` (`TimedLifeSpec`),
  `combat_resolution/impact_spawn.rs`, `world.rs` (`step_into`, `reap_dead_and_emit_shatter`),
  `transport/lifecycle.rs`, `world/upkeep.rs`, `world/commands.rs`, `economy/mod.rs`, `hangar/{mod,phase}.rs`,
  `checksum.rs`, `checksum/encoding.rs`, `match_state.rs`, `player_view.rs`; client `battle_hud/{card,actions,panels,
  model}.rs`, `input_bindings.rs`, `render_pool.rs`; tests `battle_hud_window_tests.rs`, `battle_card_tests.rs`;
  `structures.ron`, `start_loadouts.ron`, `scenarios/*.ron`, `required_set.ron`. `movement/solver.rs` was read for its
  coincident-centre rule only.
- Web search 2026-11-26 (S9): Liquipedia's Extractor page excerpt.

## Amendment 1 (orchestrator, 2026-11-27): the `razed` row is judged on instructions while the host is loaded

**What happened.** WPA4 stopped on stop rule 5. The new `razed` bench row ran 67 and 45 ticks/s, then 61 and 65 when re-run. The host load average was 87 to 96 on 16 cores during the measurements. With every death spawn cut to one unit, the row ran 46 and 48 ticks/s. So the low round is load noise on a costly scenario, not the mechanism. Instructions on the existing rows moved +0.010 %.

**Decision.** Following the rule the wall-clearance and `Gated` packages used on this host (`Gated` C-0: no wall-clock figure of record above load 8), the `razed` row is judged by callgrind instruction count. That count is converted to ticks/s with the instructions-per-second rate of the existing rows, measured in the same session.
- If the projection is at least 50 ticks/s, WPA4 completes: records, B-234 marked fixed, `DESIGN.md` §5 row marked "projected, loaded host".
- If it is below 50, WPA4 stops, and the row is re-designed before merge, keeping its 12 structures and shrinking the army.

CI's throughput job, run on a quiet runner, remains the arbiter after merge.

## As built

### Part A (B-234): WPA0 to WPA4

| Package | Commit | What landed |
|---|---|---|
| WPA0 | `03e174b4` | B-234 restated; census (stop rule 9 holds), probe and control (no measured world kills a Swarm structure), frozen close list (14, 26, 31, 33, 35, 38a, 38b), four-build baseline 8 of 8 at schema 61, throughput. Committed after WPA1 to WPA3, measured on `a7c03026` |
| WPA1 | `62e6dd5e`, `88d286e6` | `ArchetypeCapabilities::death_spawn` (`DeathSpawnSpec`), validation, `SimWorld::pending_death_spawns` (hashed section `death-spawns`, schema 62, provisional), queue at the reap commit, creation in upkeep after the per-entity loop, `DeathSpawnBlocked`; `world::spawns` shares the placement search and clock draw with impact spawns; C7 allowlist for the authored range. Latent proof 8 of 8; re-recorded goldens and sandbox proved 8 of 8 |
| WPA2 | `6a311368` | The 14 Swarm structure analogues author 9 or 6 `brood_mite` (S8's lines); DS-13 to DS-15 |
| WPA3 | `827a5f81` | `created_units` lists the death spawn; `RenderPoolBound::death`; DS-16 to DS-18 (steady state, and valgrind 0 allocations) |
| WPA4 | `c82467b3`, `3f82dd04`, the records commit | The `razed` bench row (`c82467b3` added it at 200 units despite its subject; `3f82dd04` halved the army per Amendment 1); held-out seeds byte-identical; records |

Deviations and findings:
- DS-3: a cancel goes through the reap like a death, as 5.0.14's `Cancel` is no `Remove`. With the authored
  `disabled_while_constructing: true`, a foundation never leaves units.
- Two tests that loaded `structures.ron` without units now load the units first: a death spawn names its unit by
  slug.
- DS-7 held (nine coincident units apart within 25 ticks): no movement file edited.
- WPA0's probe did not count hangar units; WPB0 does.
- The `razed` row was re-designed from 200 to 100 units. It is judged on instructions per measured tick against the
  400-unit row (38.09 M against 54.44 M), not on wall-clock ticks/s on the loaded host.

### Part B (B-230): WPB0 to WPB3

| Package | Commit | What landed |
|---|---|---|
| WPB0 | `b64e35a5` | B-230 restated with S11–S16; baseline on `main` `cd093e03`: hangar probe (no hangar unit in any measured world, golden or the sandbox; control positive), four builds 8 of 8 at schema 62, bench |
| WPB1 | `eaf4b968` | `UnitOrder::{HangarBuild, HangarCancelLast, SetHangarAutoBuild}` through the economy's admission (`hangar/commands.rs`, `HangarRejection`), `try_queue_build` shared with auto-build, hashed `HangarState::auto_build`, `HangarSpec::auto_build_toggle` (the Carrier authors it), `PlayerView::hangars`, tags 20 to 22, schema 63 (provisional); HC-1 to HC-10 |
| WPB2 | `1fd8f638` | The card's hangar entries (Build <member>, Cancel last build with the re-queue tooltip, Auto-build On/Off; keys by position), the focused Carrier's members line; HC-11 to HC-16 (GPU geometry child, steady state and valgrind with the card focused) |
| WPB3 | the records commit | Merge of `main` `edbac56b` (`a5e46785`: goldens re-recorded at 63 over main's), held-out seeds, bench; B-230 fixed; Q0138 closed (`6ada543e`) |

Deviations and findings:
- `HangarView` carries `queue_size` beside the plan's fields: the Build entry's lock needs room in both the count
  and the queue (D-9), and the view is the card's only source.
- `FocusedUnit::hangar` holds a `HangarView` (the same numbers) rather than a separate `HangarFocus` type.
- The members line omits "building …" while nothing is queued.
- The feed's hangar sentences landed in WPB1, with the commands, because the client's refusal table is exhaustive.
- HC-10 runs natively only: `golden_replay_tests.rs` takes no replay path (§5 **verify**).
- A command's build progresses on its own queueing tick (the command phase precedes the hangar phase), so it docks
  `build_ticks` − 1 ticks after the command's tick; auto-build's docks `build_ticks` after its queueing tick.
- With auto-build on, a cancelled build is re-queued in the same tick's hangar phase, progress 0 (Q0138 (a)).
- HC-16's steady-state case runs H-20's stage under the player's profile, since the sandbox profile runs no HUD.
