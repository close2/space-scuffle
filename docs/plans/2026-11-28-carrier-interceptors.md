# Blueprint: Carrier Interceptors — a Hangar of Launched Units (B-22, B-228)

This plan carries out the owner's answer to
[Q0129](../questions/open/0129-mothership-carrier-identity.md), option (a): the Cosmic Enclave's `mothership_carrier`
is StarCraft II's **Carrier**. It fixes two rows of `DESIGN.md` §4:

- **B-22:** "Carrier interceptors are one 8-strike weapon" (plan
  [`2026-11-05-sc2-deviation-fixes`](2026-11-05-sc2-deviation-fixes.md) §3.3, WP18 "→ own plans"), with the Carrier
  clause of B-227 that plan [`2026-11-27-weapon-delivery`](2026-11-27-weapon-delivery.md) D-7 handed to it (the salvo's
  interim `launch_visibility: Hidden`).
- **B-228:** "The Mothership Carrier analogue mixes two units" (registered by the weapon-delivery plan's WP0): life and
  shields 350/350 are the Mothership's, everything else the Carrier's.

The design in one paragraph: a unit may carry a **hangar** (`ArchetypeCapabilities::hangar: Option<HangarSpec>`), a
magazine of up to `max_count` **members** of one other archetype, built one at a time from the owner's resources. Docked
members are records in the hangar, not entities. The hangar unit's weapon is a **launch weapon**
(`WeaponSpec::magazine_launch: Option<MagazineLaunch>`): its release deals no damage but starts a launch channel that
puts one docked member into the world every few ticks, aimed at the weapon's target. A launched member is an ordinary
unit entity with a link to its hangar (`Entity::hangar_link`); a small **hangar phase** gives it its orders through the
existing order types (attack the hangar's target, or follow the hangar home), docks it on contact, recalls it beyond the
leash, and kills it when its hangar dies. Its flight is the existing air movement, unedited. The Carrier row takes the
Carrier's vitals in a package of its own; a new interceptor row (`saucer_interceptor`) carries the 5.0.14 Interceptor's
vitals and `InterceptorBeam`. Members are unselectable and cannot be the target of a player's order, but automatic
targeting and splash hit them. Their look is a real Veo flipbook made through the existing `asset_baker` pipeline, in a
media batch that also gives the Carrier its own clips. Checksum schema: the next free number at merge (57
provisional).

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `39df7177` (checksum schema
53, Q0129 answered), with branch `weapon-delivery-plan` (`1943995d`, schema 56 provisional, about to merge) and plan
[`2026-11-27-gated-movement`](2026-11-27-gated-movement.md) (WP0b in progress on `gated-wp0b`) read for coordination.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-229**; the draft question is **Q0130**
  (§15, not filed by this commit). The checksum schema is the next free number at merge: `main` is at 53, the Gated
  plan reserves 54 and 55, the weapon-delivery plan takes 56, so this plan takes **57** if those land first. Every
  schema mention below reads "next free at merge".
- **Prerequisite.** The weapon-delivery plan is on `main` before WP0: its W-10 delivery census lists
  `mothership_carrier` with `known_bug: "B-22"`, its D-7 sets the salvo `Hidden`, and its B-22/B-228 rows are the
  ones this plan restates and closes.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0129 (a): the analogue is the Carrier; 300/150 and the interceptor hangar | Answered 2026-11-25; carried out here | everything; its `## Resolution` in WP6 |
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S18, S19 |
| Q0045 (align times to SC2), Q0052 S-5 (25 ticks/s, half up), Q0057 (ranges edge to edge) | Closed | §2.3 conversions, D-4 |
| Q0020: visual questions are the agent's call; Q0001/Q0024 and the owner's chat rules: Veo approved within the total budget ($230), audio on, loops without jumps, no sunburst backgrounds | Standing rules | D-11, WP5 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | §6.5 |
| Gated coordination (`2026-11-27-gated-movement` §7): no other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or `movement_rules.ron` while a Gated package is open | Binding on this plan | D-10, stop rule 6 |
| **Draft, not filed:** Q0130 "Interceptors hover in range instead of strafing until the movement redesign ends: accept as an interim deviation?" | Proposed (§15); default (a) followed | D-10, B-229 |

---

## 1. Summary

| | Today (`39df7177` + weapon delivery) | After this plan |
|---|---|---|
| `mothership_carrier` life / shields | 350 / 350 (Mothership) | 300 / 150 (Carrier); radius and separation 1.25 (Carrier; today 1.0 and 1.15) |
| Its attack | One missile carrying 8 strikes of 5, range 8, period 40 ticks, `Hidden` launch | A launch weapon (range 8, period 9 ticks, damage point 0): one docked interceptor launched every 7 ticks, up to 8, at the weapon's target |
| Interceptors | None | `saucer_interceptor` units: 40/40, armour 0, light + mechanical, air, speed 0.42 per tick, sight 7, `InterceptorBeam` 2 × 5 at range 2 every 54 ticks, `Visible`; 4 at the start, built to 8 at 214 ticks and 15 minerals each |
| Recall | — | Beyond leash 12 of the Carrier, or when the Carrier has no target: the member follows the Carrier home and docks on contact |
| Carrier death | — | Every launched member dies on the same tick; docked ones and builds are lost |
| Selection, orders | — | Members unselectable, never the target of a player's order or spell; automatic targeting and splash hit them |
| Movement code | — | Unedited: members fly the existing air movement under existing orders (D-10) |
| Client | Carrier unpresentable (no media) | Members drawn from their own flipbook; selection skips them; a hangar unit is presentable only with its member's media; real Veo clips for both identities (WP5) |
| Hashed state | — | `Entity::hangar` (Carrier), `Entity::hangar_link` (member), `WeaponSpec::magazine_launch`; schema next free at merge |
| Worlds | — | Predicted byte-identical in every measured world (none carries the unit; WP0 measures it); goldens move by schema bytes only and are re-recorded once |
| Records | B-22 open, B-228 open, Q0129 open (answered) | B-22, B-228 fixed; Q0129 closed; B-229 (strafe), B-230 (hangar card commands) registered, deferred |
| Size | — | L: WP0 (records, census, baseline), WP1 (Carrier vitals), WP2 (hangar mechanism, latent), WP3 (the data), WP4 (client), WP5 (media), WP6 (confirmation, re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. Conversions as everywhere in the catalog (Q0045, Q0052 S-5): a Normal time `t` is `t × 25 / 1.4` ticks,
rounded half up; a Normal speed `v` is `v × 0.056` world units per tick; a Normal acceleration `a` is `a × 0.003136`
per tick squared; a Normal turning rate `r` (degrees per second) is `r × 0.056` degrees per tick.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`; `liberty AbilData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CarrierHangar` (`CAbilArmMagazine`), `liberty AbilData:2083–2098`: `Leash 12`; `Flags Retarget 1`; `EffectArray Death InterceptorFate`; `InfoArray Ammo1 Time 8 CountStart 4 Manage Recall Unit Interceptor`, `Charge Link CarrierInterceptor`, `Flags AutoBuild 1`, `Flags LeashRetarget 1` | A magazine of `Interceptor` units, 4 at the start, recalled (`Manage Recall`), leash 12, retargeting [DATA] |
| S3 | `swarm AbilData:467–469`: `CarrierHangar MaxCount 8` | At most 8 [DATA] |
| S4 | `voidmulti AbilData:1296–1300`, `balancemulti AbilData:1133–1137`: `InfoArray Ammo1 Time 12`, `Flags AutoBuildOn 1` | 12 Normal s per interceptor, auto-build on by default [DATA] |
| S5 | `HangarQueue5` (`CAbilQueue`), `liberty AbilData:2072–2076`: `QueueSize 5`, `Passive`; on the Carrier, `liberty UnitData:3513` | At most 5 builds queued, one built at a time [DATA; one at a time is INF from a `CAbilQueue`, as for training] |
| S6 | `InterceptorFate`: named by `CarrierHangar` (`liberty AbilData:2088`) and `BroodLordHangar` (:3814) but defined in no layer's `EffectData` (grep of all eight) | No data effect runs on the Carrier's death; what happens to its interceptors is the engine's (S18) [DATA, INF] |
| S7 | `InterceptorLaunch` (`CWeaponLegacy`), `liberty WeaponData:601–614`: `TargetFilters Visible;Missile,Stasis,Dead,Hidden,Invulnerable`, `Range 8`, `Arc 360`, `Period 0.5`, `AllowedMovement Slowing`, `Options Hidden`, `Effect InterceptorLaunchPersistent`, `DamagePoint 0`, `Backswing 0`; no layer overrides it | The Carrier's weapon: range 8, period 0.5 → 9 ticks, no damage point or backswing [DATA] |
| S8 | `InterceptorLaunchPersistent`, `liberty EffectData:893–902`: `WhichLocation TargetUnit`, `Flags Channeled 1`, `PeriodCount 8`, `PeriodicValidator CasterNotDead`, `PeriodicEffectArray CarrierInterceptor`, period 0.5; `voidmulti EffectData:427–429` and `balancemulti :339–341`: period **0.375** | One launch attempt every 0.375 Normal s → 7 ticks, 8 attempts, while the Carrier lives; channelled [DATA] |
| S9 | `CarrierInterceptor` (`CEffectUseMagazine`), `liberty EffectData:1940–1942`, no fields | Each attempt spends one docked interceptor [DATA]; with none docked it does nothing [INF] |
| S10 | `Interceptor`, `liberty UnitData:3544–3604`: `LifeMax 40` (:3550), `ShieldsMax 40` (:3553), `ShieldRegenRate 2` (:3554), `ShieldRegenDelay 10` (:3601), no `LifeArmor` (0), `Collide FlyingEscorts` (:3556), `Mover Fly`, `Speed 7.5` (:3558), `Acceleration 1000` (:3559), `AttackTargetPriority 20` (:3561), `Radius 0.25`, `SeparationRadius 0.25` (:3565–3566), flags `Uncommandable`, `Unselectable`, `Untargetable` (:3571–3573), `PlaneArray Air`, `Sight 7` (:3581), `Light`, `Mechanical` (:3592–3593), turning 999.8437 (:3596–3597), no `Food`; `balancemulti UnitData:859–866`: `Uncommandable 0`, `DefaultAcquireLevel Offensive`, `Response Acquire`, `CostResource Minerals 15` | The member's vitals, flight, flags and cost [DATA] |
| S11 | `InterceptorBeam` (`CWeaponStrafe`), `liberty WeaponData:626–637`: `DisplayAttackCount 2`, `TargetFilters Visible;Missile,Stasis,Dead,Hidden,Invulnerable`, `Range 2`, `Arc 19.6875`, `Period 3`, `Effect InterceptorBeamPersistent`; no `DamagePoint` or `Backswing` (so the `CWeapon` defaults 0.167 and 0.5, `core WeaponData:17–18`) | Range 2, period 54 ticks, damage point 3, backswing 9 [DATA; that `CWeaponStrafe` keeps the `CWeapon` defaults is INF, **verify**] |
| S12 | `InterceptorBeamPersistent`, `liberty EffectData:1943–1952`: `InitialEffect InterceptorBeamDamage`, `PeriodCount 1`, `PeriodicPeriodArray 0`, `PeriodicEffectArray InterceptorBeamDamage`; `InterceptorBeamDamage` (:1953–1958): parent `DU_WEAP`, `Visibility Visible`, `Amount 5`, `Kind Ranged`. The `AA`/`AG` variants (`voidmulti EffectData:2262–2283`, `WeaponData:306–330`; `balancemulti` likewise) have the same values and are named by no unit's `WeaponArray` in any layer | Two strikes of 5 on the same tick, instant, `Visible` [DATA] |
| S13 | Upgrades: `libertymulti UpgradeData:47–49` (and :63–65, :79–81): each air weapons level `Effect,InterceptorBeamDamage,Amount +1`; `liberty UpgradeData:327–328` shields armour, :438–439 air armour, both naming `Interceptor` | +1 per strike per level; armour from the air armour and shields upgrades [DATA] |
| S14 | Carrier, `liberty UnitData:3469–3543`: `LifeMax 300` (:3474), `LifeArmor 2` (:3475), `ShieldsMax 150` (:3478), `ShieldRegenRate 2`, `ShieldRegenDelay 10` (:3539), `Speed 1.875`, `Acceleration 1.0625` (:3482–3483), `Radius 1.25`, `SeparationRadius 1.25` (:3491–3492), `Sight 12` (:3507), `Armored`, `Mechanical`, `Massive` (:3525–3527), 350/250 (:3533–3534), `Food -6`, `WeaponArray InterceptorLaunch` (:3538), `AbilArray CarrierHangar`, `HangarQueue5`; no other layer changes these (`balancemulti :134–142`, `voidmulti :174–185`, `libertymulti :146–154`, `swarm :1528–1536`, `swarmmulti :71–74`, `void :938–941`) | The Carrier's vitals and armour [DATA] |
| S15 | Mothership, `liberty UnitData:4244–4316`: radius 1.375, speed 1.4062, sight 14 (and 350/350, weapon-delivery S16) | What B-228's other half was [DATA] |
| S16 | `InterceptorLaunchUpgradedPersistent` (Graviton Catapult), `liberty EffectData:903+` (period 0.125); `CarrierCarrierCapacity`, `CarrierLeashRangeUpgrade`, `voidmulti UpgradeData:887–908` | Upgrades our catalog has no row for (§14) [DATA] |
| S17 | Plan [`2026-11-25-collision-classes`](2026-11-25-collision-classes.md) §2 S-1 and its table: `FlyingEscorts` is `Air3`; "Interceptor `FlyingEscorts`: touches only interceptors" | A collision class of the members' own is exact [DATA, via that plan] |
| S18 | Liquipedia, *Interceptor (Legacy of the Void)* (<https://liquipedia.net/starcraft2/Interceptor_(Legacy_of_the_Void)>), as quoted by a web search on 2026-11-25 (the page itself answered HTTP 429): "Interceptors are entirely dependent on their home Carrier for survival - they instantly explode if their Carrier is destroyed. Interceptors are not individually selectable, and they cannot be given orders directly." StarCraft Wiki, *Aiur carrier* (<https://starcraft.fandom.com/wiki/Aiur_carrier>), same search: interceptors self-destruct when their parent is destroyed and are not adopted by other Carriers | Carrier death kills its interceptors at once; members are unselectable and take no player orders [EXT; medium: search excerpts, not the pages] |
| S19 | Liquipedia, *Carrier (Legacy of the Void)* (<https://liquipedia.net/starcraft2/Carrier>), same search: "A Carrier launches Interceptors at range 8 of a target. After launching Interceptors, the Carrier can move back to a leash range of 12 before the Interceptors disengage and return to the Carrier." | The leash is measured from the Carrier to its interceptors; leashed interceptors return [EXT, medium] |

### 2.2 The rules that follow

1. **The hangar** (S2–S5). A Carrier starts with 4 interceptors (`CountStart`) at no cost; it builds more one at a
   time, 214 ticks each (12 Normal s), 15 minerals each (S10), up to 8 in all, with up to 5 builds queued. Auto-build is
   on (S4), so a Carrier below 8 queues builds by itself while its owner can pay. Interceptors take no supply (S10:
   no `Food`).
2. **The launch** (S7–S9). The Carrier's weapon has range 8 and fires every 9 ticks with no damage point. Its effect is
   a channelled persistent at the target that tries 8 launches, one each 7 ticks, while the Carrier lives; each try
   spends one docked interceptor and sends it out at the target. With none docked, a try launches nothing [INF].
3. **The interceptor** (S10–S13): 40 life, 40 shields (regenerating 0.112 per tick after 179 ticks without damage),
   life armour 0, shield armour 0, light and mechanical, air, radius 0.25, speed 0.42 per tick with instant
   acceleration (1000 → 3.136 per tick², above the speed) and instant turning (999.8 → 55.99° per tick), sight 7,
   attack target priority 20. Its weapon: range 2, firing arc 19.6875°, period 54 ticks, damage point 3, backswing 9,
   two strikes of 5 on the same tick, ranged, against air and ground, `Visible` (the struck player sees the
   interceptor, not the Carrier). +1 per strike per air weapons level.
4. **Orders** (S2, S10, S18, S19). The player cannot select or order an interceptor, nor target one with an order
   (`Unselectable`, `Untargetable`; S18). The Carrier directs them: they attack its target and switch when it switches
   (`Retarget`); in multiplayer they also acquire on their own (`Offensive`, `Response Acquire`, S10 `balancemulti`).
   An interceptor farther than 12 from its Carrier disengages and returns (S19); interceptors return and dock when the
   Carrier has nothing to attack (`Manage Recall`, S2) [INF from the flag name and S19].
5. **Targeted by enemies.** `Untargetable` keeps orders off them; enemy units still attack interceptors by automatic
   targeting, and splash hits them [EXT, common knowledge of the game, **verify** against a source quoting the flag;
   `AttackTargetPriority 20` equals the Carrier's, so a shooter in range of both picks by distance]. B-8's row cites
   Liquipedia for an Interceptor priority of 19; the data says 20 (S10). The data is followed; **verify**.
6. **Carrier death** (S6, S18): every launched interceptor dies at once; docked ones and queued builds go with the
   Carrier. Nothing is refunded [INF: the producer is gone].
7. **Collision** (S17): interceptors touch only interceptors, never the Carrier or other air units.
8. **The Carrier** (S14): 300 life, 150 shields, life armour 2, radius 1.25, separation radius 1.25, sight 12,
   armoured, mechanical, massive, 350/250, 6 supply; the row's other values already match (§3 item 5).

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| Hangar: max 8, start 4, 214 ticks and 15 minerals each, auto-build on, queue 5 | High | S2–S5, S10 [DATA] |
| Launch: weapon range 8, period 9 ticks, damage point 0; 8 tries, one each 7 ticks | High | S7, S8 [DATA] |
| A try with an empty magazine launches nothing | Medium | S9 [INF] |
| Interceptor vitals, armour 0, attributes, speed, sight, radius, cost 15 | High | S10 [DATA] |
| `InterceptorBeam`: range 2, period 54, 2 × 5 at once, `Visible` | High | S11, S12 [DATA] |
| Damage point 3 and backswing 9 for the strafe weapon (`CWeapon` defaults) | Medium, **verify** | S11 [INF]: `CWeaponStrafe` names neither |
| Leash 12, measured Carrier to interceptor | Medium-high | S2 [DATA] for the value; S19 [EXT] for what it is measured between |
| Recall when the Carrier has no target; dock on reaching it | Medium | S2 `Manage Recall` [INF], S19 [EXT] |
| `Retarget`: members follow the Carrier's new target | Medium | S2 flag name [INF] |
| What `LeashRetarget` does beyond returning leashed members | Low, **verify** | Flag name only; D-6 treats a leashed member as returning until docked |
| Carrier death kills launched interceptors at once | Medium-high | S18 [EXT, two wikis agree]; S6 shows it is not in the data |
| Unselectable; no player orders; not a valid target of an order | High (flags), medium (meaning) | S10 [DATA], S18 [EXT] |
| Enemies attack interceptors automatically; splash hits them | Medium-high, **verify** | Common knowledge of the game; no source quoted while planning |
| Interceptor `AttackTargetPriority` 20, not B-8's 19 | High for the data, **verify** the game | S10 [DATA] against B-8's Liquipedia citation |
| A docked interceptor keeps its life and shields; shields regenerate while docked under the unit's own rule; life does not | Low, **verify** | Not in the data; no source found (D-3 records the choice) |
| A Move or Stop given to the Carrier does not recall launched interceptors; only the leash and a lost target do | Medium, **verify** | S19 [EXT] (the Carrier "can move back to a leash range of 12 before the Interceptors disengage") |
| A channel continues while the Carrier moves (`AllowedMovement Slowing`) | Low, **verify** | S7; D-5 keeps the existing rule for moving while a weapon fires |
| The interceptor's attack is a strafe (fly through, turn, come back), not a hover in range | High for the class, unknown for the path | S11 `CWeaponStrafe`; not modelled here (D-10, B-229) |
| The Carrier's vitals and radius | High | S14 [DATA] |

---

## 3. Diagnosis (on `39df7177` plus the weapon-delivery branch, read from the code)

1. **The row.** `assets/catalog/units.ron`, `mothership_carrier` (`unit_type_id: 32`): `max_hp: 350.0`,
   `max_shields: 350.0`, `collision_radius: 1.0`, `separation_radius: 1.15`, `sc2_analogue: "Carrier / Mothership (…)"`;
   weapon "Interceptor Fleet Salvo": `attacks_count: 8`, `base_damage: 5.0`, `projectile_speed: Some(1.2)`,
   `period_ticks: 40`, `damage_point_ticks: 1`, `backswing_ticks: 4`, `max_range: 8.0`, `attack_visibility:
   Visible(54)`; after weapon delivery `launch_visibility: Hidden`. `effects`: travel `plasma_bolt`, impact
   `plasma_hit`. Trained by the Enclave's stargate analogue (`structures.ron`, `trains_units`).
2. **Reference tests exempt it.** `catalog_reference_values_tests.rs` lists `"Carrier / Mothership"` as an exemption
   in the vitals, damage, bonus-per-level, range and radius tables, and carries rows for its attributes (42), sight
   (12), arc (360) and acceleration (1.0625, "Carrier"). `weapon_catalog_rules_tests.rs` has the row `("Carrier /
   Mothership", None, &[], "none")`; the weapon-delivery census (its W-10) lists it with `known_bug: "B-22"`.
3. **No archetype owns other units.** The only relations between units are the transport link
   (`Entity::transport_carrier`, `crates/sim_core/src/transport/`: one authoritative passenger-side link; carried units
   take no part in movement, vision, targeting) and housed harvesters (`UnitTask::housed_in`). Neither fits a magazine:
   transport validation requires a `transport_capacity`, supply and unload rules, and a housed unit stays an entity.
4. **Spawning.** `SimWorld::spawn_unit_facing` (`world.rs`) → `prepare_spawn` / `commit_spawn` is the one way an
   entity enters the world outside production; production completion uses the same pair. Ids come from
   `next_entity_id` (`EntityId(pub u32)`) and are never reused.
5. **The Carrier's other values already match S14**: speed 0.1048 (the catalog's existing conversion of 1.875; it is
   not in any exemption), acceleration 0.003332, sight 12, attributes 42, life armour 2, shield regeneration 0.112
   after 179 ticks, cost 350/250, supply 6, build time 1,607 ticks.
6. **The tick** (`SimWorld::step_into`): commands → production and buffs → unit tasks → vision → lost swings →
   acquisition → attack dashes → movement → collisions → weapons and projectiles → cast locks → reap. Combat's
   `dispatch_shot` (`combat_resolution.rs`) chooses missile or instant by `projectile_speed`.
7. **Order types a member needs already exist**: `UnitOrder::AttackTarget { target, origin }` (with
   `AttackOrigin::Explicit`, "only impossibility ends it") and `UnitOrder::Follow { target, kind: FollowKind::Move }`
   (plan `2026-11-26-follow-order`: walk until touching the target, acquire nothing).
8. **Collision classes are authorable** (`collision_class: Some("…")` on an archetype, plan
   `2026-11-25-collision-classes`): bodies touch only bodies of the same class; a new name is a new class.
9. **Media.** Neither `mothership_carrier` nor any interceptor has a flipbook under `assets/textures/` or a clip under
   `assets/videos/`. `ClientMediaStatus::presentable` (`client/src/media.rs`) therefore refuses the Carrier, and the
   command card locks it (`CardLock::NoMedia`, `battle_hud/card.rs`): no client match can field a Carrier today.
10. **Measured worlds.** The weapon-delivery WP0 probe (`docs/evidence/2026-11-27-weapon-delivery.md`, Probe) found **no
    release** of the salvo in the matrix (1,220 runs), battles 31 and 33, the showcase and showcase-effects worlds, the
    skirmish scenario or either golden. It counted releases, not presence; this plan's WP0 counts presence (§7.1).

---

## 4. Design decisions

### D-1. The Carrier's vitals (WP1, its own package)

| Field | Today | After | Source |
|---|---|---|---|
| `max_hp` | 350.0 | **300.0** | S14 :3474 |
| `max_shields` | 350.0 | **150.0** | S14 :3478 |
| `collision_radius` | 1.0 | **1.25** | S14 :3491 |
| `separation_radius` | 1.15 | **1.25** | S14 :3492 |
| `sc2_analogue` | "Carrier / Mothership (Capital Flagship with Interceptor Fleet)" | "Carrier (Capital Flagship with Interceptor Fleet)" | Q0129 (a) |
| `display_name`, `slug`, `lore`, cost, supply, build time, speed, sight, armour, attributes | — | unchanged (already the Carrier's, §3 item 5; the name is an original name, Q0052 S-6) | — |

The weapon is untouched in WP1 (B-22 stays open until WP3). The reference tests' label `"Carrier / Mothership"`
becomes `"Carrier"` and leaves the vitals and radius exemption lists; its damage, bonus and range exemptions stay
until WP3. Each changed field carries a citation comment in the style of the row's existing comments.

### D-2. The hangar capability (data types, `sim_types` and `sim_core` catalog)

In `crates/sim_types/src/commands.rs`, on `ArchetypeCapabilities`:

```rust
/// A magazine of other units this unit builds, launches with its launch weapon and recalls
/// (`StarCraft` II `CAbilArmMagazine`; plan `2026-11-28-carrier-interceptors` D-2). `None` for
/// every unit without one.
#[serde(default)]
pub hangar: Option<HangarSpecDef>,
```

The resolved form, `sim_core::catalog::HangarSpec` (slug resolved to an id, times and lengths in fixed point):

| Field | Type | Meaning (units) | Carrier |
|---|---|---|---|
| `member` | `UnitTypeId` (RON: slug) | The archetype of every member | `saucer_interceptor` |
| `max_count` | `u8` | Members docked, launched and building together, at most (`MaxCount`) | 8 |
| `start_count` | `u8` | Members docked when the hangar unit is created (`CountStart`), free | 4 |
| `build_ticks` | `u16` | Ticks to build one member (`Time`) | 214 |
| `queue_size` | `u8` | Builds queued at most (`QueueSize` of the hangar's queue) | 5 |
| `auto_build` | `bool` | Whether builds are queued by themselves (`AutoBuild` + `AutoBuildOn`) | true |
| `leash` | `SimScalar` | Centre-to-centre distance, world units, beyond which a launched member returns (`Leash`) | 12.0 |
| `on_death` | `HangarDeath` | What happens to launched members when the hangar unit dies; one variant, `KillMembers` (S18), so the rule is authored, not assumed | `KillMembers` |

A member's cost is its own archetype's `mineral_cost` and `gas_cost` (15 / 0), not a hangar field, so one fact lives
in one place. The member archetype's `build_time_ticks` is not read (the hangar's `build_ticks` is the magazine's
time, as `Time` is the ability's, not the unit's).

**Validation** (`catalog/validation.rs`, each a typed `SimCoreError` naming the slug and field):
`max_count ≥ 1`; `start_count ≤ max_count`; `queue_size ≥ 1`; `build_ticks ≥ 1`; `leash > 0`; the member exists, is
mobile, is not a structure, has no hangar itself, is no transport and no harvester; the member is not in any
`trains_units`, start loadout or scenario spawn list (it exists only through a hangar); the member's `supply_cost` is
0; the hangar unit has exactly one launch weapon (D-4) and the member has its own weapon. A member's archetype may
serve several hangars (no rule needs it unique).

### D-3. The hangar state and the member link (`entity.rs`, hashed)

```rust
/// A hangar unit's magazine (plan `2026-11-28-carrier-interceptors` D-3). Present exactly on
/// entities whose archetype has a hangar; hashed in full.
pub struct HangarState {
    /// Docked members in docking order (oldest first); launches take the oldest.
    /// Capacity reserved for `max_count` at creation, so docking never allocates.
    pub docked: Vec<DockedMember>,
    /// Launched members alive in the world; equals the number of entities whose
    /// `hangar_link` names this unit (checked every hangar phase, D-8).
    pub launched: u8,
    /// Builds queued and paid for, the one in progress included.
    pub queued: u8,
    /// Ticks the build in progress has run; 0 when `queued` is 0.
    pub build_progress: u16,
    /// The target members are sent at: the target of the latest launch release, cleared
    /// when it stops being a valid target (D-6).
    pub target: Option<EntityId>,
    /// The launch channel of the latest release, if it has tries left (D-5).
    pub channel: Option<LaunchChannel>,
}

/// A docked member's vitals: what it takes back into the world at its next launch.
pub struct DockedMember { pub hp: SimScalar, pub shields: SimScalar, pub ticks_since_last_damage: u16 }

/// The tries left of a release's channel and the ticks until the next one.
pub struct LaunchChannel { pub tries_left: u8, pub next_try_in: u16 }

/// On a launched member: its hangar unit and whether it is on its way home (D-6).
pub struct HangarLink { pub hangar: EntityId, pub returning: bool }
```

`Entity` gains `pub(crate) hangar: Option<HangarState>` and `pub(crate) hangar_link: Option<HangarLink>`, each with a
read accessor. Created by `Entity::from_archetype`: a hangar archetype gets `start_count` docked members at full life
and shields; every other entity gets `None` for both.

**Docked members are not entities.** A docked interceptor is outside the world in 5.0.14 (it cannot be seen, hit or
counted on the map). A record keeps exactly what survives docking (life, shields, the regeneration clock) and needs no
exclusion from movement, vision, targeting, the snapshot or the client, which a hidden entity would (the transport and
housing predicates are read in `movement/mod.rs` and `movement/groups.rs`, files this plan must not edit, D-10).
**Each launch creates a new entity id** (D-7); reusing a docked member's old id would let stale references (a missile's
target, another unit's order) find a different life of it.

**Regeneration while docked** (§2.3, low confidence, **verify**): the hangar phase applies the member archetype's own
shield rule to each record (the same pure function the entity upkeep uses, moved to take the three fields if needed;
`world/upkeep.rs`), and life does not regenerate (the member's `hp_regen_per_tick` is 0). Recorded in D-12.

### D-4. The launch weapon (`sim_types/src/weapon.rs`, `combat_resolution.rs`)

`WeaponSpec` gains, after `missile_movers` (weapon delivery):

```rust
/// The launch channel this weapon starts instead of striking (`StarCraft` II's
/// `CEffectCreatePersistent` of `CEffectUseMagazine`; plan `2026-11-28-carrier-interceptors`
/// D-4). `None`, the default, for every weapon that strikes.
#[serde(default)]
pub magazine_launch: Option<MagazineLaunch>,

/// How a launch weapon's release spends its unit's hangar.
pub struct MagazineLaunch {
    /// Launch tries per release (`PeriodCount`), at least 1.
    pub tries: u8,
    /// Ticks between tries (`PeriodicPeriodArray`), at least 1; the first try is on the release tick.
    pub interval_ticks: u16,
}
```

**Validation:** a launch weapon has `base_damage` 0, no attribute bonuses, `attacks_count` 1, no splash, no
`projectile_speed`, no movers, `visual_kind: None`, `attack_visibility: Hidden` and `launch_visibility: Hidden` (the
launch is no missile and deals nothing, S7–S9, weapon-delivery §2.2 rule 7); its archetype has a hangar; no mode
override gives a hangar unit a striking weapon or a launch weapon to a unit without a hangar.

**Dispatch:** `dispatch_shot` gains one branch before the missile/instant choice: a weapon with `magazine_launch`
calls `start_launch_channel(attacker, target, launch)`: the hangar's `target` becomes the weapon's target, and
- if `channel` is `Some` and `target` is unchanged, nothing else happens (a release during a running channel does not
  start a second one; with period 9 and 8 tries of 7 ticks the weapon fires about six times per channel, and the game's
  `Channeled` persistent does not stack: **verify**);
- otherwise `channel = Some(LaunchChannel { tries_left: tries, next_try_in: 0 })` (a new target restarts the tries).

The weapon's cooldown, damage point (0), backswing (0), random delay and acquisition are the existing weapon rules,
unchanged. No damage, reveal, snapshot or launch image is recorded for the release (it is `Hidden` on both counts).

### D-5. The hangar phase (new module `crates/sim_core/src/hangar/`)

One phase, `SimWorld::step_hangars`, runs **after vision and before `end_lost_swings` and acquisition**, so a member
launched or redirected this tick is acquired, moved and fires through the ordinary phases of the same tick. A second
hook, `reap_hangar_members`, runs at the start of `reap_dead_and_emit_shatter`, before `reap_transport_groups`.
The phase is skipped entirely when the catalog has no hangar archetype (a catalog-level `bool`, not world state).

In order, each in ascending entity id:

1. **Members** (one pass over the entity map, D-8): for each living entity with a `hangar_link`, direct it (D-6),
   dock it if it is returning and touches its hangar unit (edge distance ≤ 0, the contact the follow order stops at;
   the dock removes the entity and pushes its `DockedMember`), and count it for its hangar's `launched` check.
2. **Hangars**: for each living entity with a `hangar`:
   - **Target check** (D-6): clear `target` and `channel` when the target is gone or invalid.
   - **Channel**: if `channel` is `Some`, decrement `next_try_in`; at 0, try a launch: if `docked` is not empty,
     launch the oldest (D-7); either way `tries_left -= 1`, `next_try_in = interval_ticks`; at 0 tries left the
     channel ends.
   - **Build**: if `queued > 0`, `build_progress += 1`; at `build_ticks` a member docks at full vitals,
     `queued -= 1`, `build_progress = 0`.
   - **Auto-build**: while `auto_build` and `queued < queue_size` and `docked + launched + queued < max_count` and the
     owner can pay the member's cost now, charge it and `queued += 1` (the production admission's resource check and
     debit, `production/`, reused).

The order (members, then hangars) means a member docking this tick is available to a try this tick, as a returning
interceptor in 5.0.14 can be relaunched once inside [INF].

### D-6. Directing a member (the interceptor's AI, as data and existing orders)

For a launched member `m` of hangar unit `c` (alive; a dead `c` is handled by D-9):

| Condition (first that holds) | Member's state | Order given (only when it differs from the member's current active order) |
|---|---|---|
| `centre_distance(c, m) > leash` | `returning = true` | `Follow { target: c, kind: Move }` |
| `returning` and `c.hangar.target` is `None` | returning | `Follow { target: c, kind: Move }` (kept) |
| `c.hangar.target = Some(t)`, `t` a valid target for `m` (below), and `m` not leashed | `returning = false` | `AttackTarget { target: t, origin: Explicit }` |
| otherwise (no target, or `t` invalid for `m`'s weapon) | `returning = true` | `Follow { target: c, kind: Move }` |

- **A leashed member returns** (`returning` is set while it is beyond the leash). Once back inside the leash, the
  third row sends it out again if `c` still has a valid target; otherwise it flies on home and docks. This is S19's
  "disengage and return"; what `LeashRetarget` adds is **verify** (§2.3).
- **Valid target for `m`**: alive, not loaded or housed, visible to `m`'s owner, and admitted by `m`'s weapon target
  flags (air/ground) — the same predicate a player's attack order is admitted by (`world/commands.rs`'s existing
  admission, read, not changed). `AttackOrigin::Explicit` means acquisition never replaces the hangar's target, which
  is `Retarget`: members follow the Carrier's choice, not their own.
- **The hangar's target is cleared** when it is no longer a valid target for the hangar unit's launch weapon (dead,
  loaded, housed, not visible to the owner, or outside its target flags). A Move, Stop or Hold Position given to the
  Carrier does **not** clear it (S19: the leash governs; **verify**). A new release at another target replaces it.
- **Orders are written through `Entity::replace_orders`** (or the existing equivalent that a non-queued command uses),
  never through `PlayerCommand`, so the selection and command rules of D-8 do not apply to the hangar itself. An order
  is written only on a change of row or target, so a member's route counters and wind-ups are not reset every tick.
- **The member's own acquisition** (S10 `balancemulti`: `Offensive`) is authored as its `acquire_level`; with an
  explicit attack or a follow always active while launched, it matters only in the tick between a target dying and the
  hangar phase of the next tick, as for any unit.

### D-7. Launching a member

`launch_member(c)`: pop the oldest `DockedMember`; spawn the member archetype through `prepare_spawn` /
`commit_spawn` with `owner = c.owner`, position `c.position` exactly (air bodies of another collision class, so no
placement conflict), heading `c.heading`, life, shields and `ticks_since_last_damage` from the record, `hangar_link =
Some(HangarLink { hangar: c.id, returning: false })`, and its first order from D-6 at once; `c.hangar.launched += 1`.
No supply is captured (member supply 0, D-2). The spawn's typed errors propagate as a `HangarInvariant` diagnostic
(D-8); a failed spawn puts the record back (the spawn is prepared before anything is popped, as the transport
transactions do).

### D-8. Invariants, errors and commands

- `HangarInvariant` (`thiserror`, in `hangar/`): a link to a missing or non-hangar entity, a link across owners, a
  `launched` count that differs from the links counted, `docked + launched + queued > max_count`, a docked record above
  the member's maxima. Checked in the members pass (the count is free there); reported through
  `SimulationDiagnostic`, as transport invariants are; the tick goes on with the hangar untouched.
- **Selection and orders (S10, S18).** `UnitArchetype` gains `pub selectable: bool` and `pub order_targetable: bool`
  (RON `selectable`, `order_targetable`, both default `true`; StarCraft II's `Unselectable` and `Untargetable` flags,
  general, not hangar-specific). Command admission (`world/commands.rs`) drops an entity whose archetype is not
  `selectable` from a `PlayerCommand`'s `entities` (a command left with none is refused as an empty selection is
  today), and refuses an `AttackTarget`, `Follow`, `Load` or `CastAbility` whose target entity's archetype is not
  `order_targetable`. Acquisition, splash and effect searches do not read either flag: automatic targeting still picks
  members (§2.2 rule 5), and `target_acquisition.rs` is not touched.
- Validation: a hangar's member must be `selectable: false` and `order_targetable: false` (S10); any other archetype
  may set them.

### D-9. Death of the hangar unit and of members

- **Hangar unit dies** (`on_death: KillMembers`): `reap_hangar_members` sets every living entity whose `hangar_link`
  names a dead hangar unit to zero life in the same tick, so the reap removes them with their own shatter event, before
  the hangar unit's entity is gone. Docked records and queued builds vanish with the hangar state. Nothing is refunded.
  Kills are credited to no one (no damage source), as an `InterceptorFate` with no data effect implies.
- **A member dies**: the members pass no longer counts it; `launched` is recomputed from the count before the hangars
  step (the count is the truth, `launched` its cached copy checked against it). Auto-build replaces it at cost.
- **A member's hangar is loaded or housed**: not possible for the shipped roster (an air capital ship); validation
  refuses a hangar on a unit with a `cargo_size` that a transport could take (**verify** the field name in WP2).

### D-10. Movement and the Gated plan: reuse, sequenced to stay off its files

**Decision: members fly the existing air movement, without editing any file the Gated plan protects; this plan may run
in parallel with Gated's packages.** Reasons:
- A member is an ordinary `MoverType::Air` entity with speed, acceleration and turning from its row; it moves under
  `AttackTarget` and `Follow`, both handled by today's movement for air units.
- Its collision class is authored data, `collision_class: Some("flying_escorts")` (S17), so members touch only members.
  The class table is built from `units.ron`; no movement code learns anything new.
- Docked members are not entities (D-3), so nothing in `movement/` needs a new exclusion.
- No file under `crates/sim_core/src/movement/`, nor `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`, is
  edited. If any would need an edit, that is stop rule 6.

**What this costs: interceptors hover, they do not strafe.** `InterceptorBeam` is a `CWeaponStrafe` (S11): in the game
an interceptor flies through its target, turns and comes back. Here a member stops in range 2 and fires every 54
ticks, as any ranged air unit. The damage rate is the game's; the position differs, which matters for splash and for
shooters with short range. A strafe is a movement behaviour and waits for the movement redesign: registered as
**B-229** (deferred until after Gated WP4, which owns the movement files), and drafted as **Q0130** (§15) so the owner
can accept it as interim or hold the Carrier back. Default: (a), ship with the recorded deviation.

**Gated coordination in practice.** Gated's switch-over (WP4) changes how every unit moves, members included. If a
Gated package merges to `main` while this plan is open, this plan merges `main` and re-takes its baseline before the
next measured package (§7.2); members are judged under whatever pipeline `main` ships. Gated's measured worlds carry no
Carrier (§3 item 10), so this plan cannot move Gated's acceptance figures.

### D-11. The client and the media (real assets, no stand-in)

**The look of a member is its own flipbook.** The client already draws every unit from its identity's flipbook atlas
and refuses an identity without media (`ClientMediaStatus::presentable`; `CardLock::NoMedia`); members are unit
entities in `RenderSnapshot`, so they are drawn by that path with no new renderer. What changes in `client/src`:
1. **Presentable hangar units:** a hangar unit is presentable only if its member is (`media.rs`, `presentable` reads
   the archetype's `capabilities.hangar.member`). Otherwise the Carrier could be trained and then launch units the
   client cannot draw.
2. **Selection** skips entities whose archetype is not `selectable` (click, box, double-click "select all of type",
   control groups); **picking for orders** does not return an entity whose archetype is not `order_targetable` (a click
   on a member falls through to what is beneath it).
3. **Docking is not dying:** a member that leaves the snapshot without a death event despawns without a shatter or
   death look (the existing despawn path; WP4 checks it, and the steady-state tests prove no allocation when members
   cycle, W-24).
4. **Weapon looks:** the Carrier's launch weapon has `visual_kind: None` and no `effects` entry (no travel, no impact:
   the interceptors are the visual); the member's beam takes an existing instant look of the Enclave from
   `effects.ron` (`visual_kind: LaserBeam`, WP4 picks it by delivery kind; effect looks are generic, not identity
   media, so reuse is not a stand-in). `effect_catalog_tests`' delivery helper learns that a `magazine_launch` weapon
   needs no travel or impact.

**The media decision: generate, do not reuse.** No small air unit of the Enclave has media; the only small air units
with real flipbooks (`vector_interceptor`, `cloak_gunship`, `omni_probe_drone`) belong to the Automaton faction and
would show an Automaton craft flying out of an Enclave saucer, which is exactly a faked identity (charter §1.2). So
**WP5 generates real Veo clips through `asset_baker`** for:
- `saucer_interceptor`: Idle, Walk, Attack (the publishable actions of a mobile armed unit,
  `published_actions.rs`), from the row's `veo_prompts` (D-13);
- `mothership_carrier`: Idle, Walk, Attack, from its existing `veo_prompts` (the Attack prompt already shows
  interceptors launching). Without the Carrier's own clips, members could never appear in a client match (item 1).

Budget: 6 clips at $0.40 (Veo 3.1 Fast, 720p, audio on) = $2.40; with regenerations, the batch is capped at
**25 generations, $10.00**, checked against the ledger's remaining total ($230 cap) by the free preflight before
anything is spent. Review with contact sheets in `docs/evidence/veo/review/`; loops must not jump; no radiating
stripes; the owner's chat rules apply (Q0001, Q0024, Q0047, 2026-11-18 raise). Baked into flipbooks by the existing
importer, provenance recorded, both identities added to `assets/media/required_set.ron` as `Expansion`, evidence in
`docs/evidence/veo/2026-11-28-batch-carrier.md`. **If Veo is blocked** (key missing, service disabled, quota, budget):
stop WP5 and ask the owner (charter §1.2); the simulation packages still land, and both identities stay locked by
`NoMedia` in the client, which is truthful.

### D-12. Approximations and deviations, each recorded

| # | What | Ours | Game | Record |
|---|---|---|---|---|
| A-1 | Attack path | Hover in range 2 | Strafe (`CWeaponStrafe`) | **B-229**, Q0130 draft; after Gated WP4 |
| A-2 | Hangar commands | None: auto-build only; no build button, no cancel, no auto-build toggle, no docked count on the card | Card buttons `CarrierHangar,Ammo1`, `HangarQueue5,CancelLast` (S14 :3521–3522) | **B-230**, deferred; data already allows `auto_build: false` |
| A-3 | Docked regeneration | Shields under the unit's rule; life none | Not in the data | **verify** (§2.3); no bug unless a source contradicts |
| A-4 | Dock point | Contact with the Carrier's edge | The model's dock, inside the Carrier | Detail (≤ 1.25 units of flight); not a bug |
| A-5 | Launch point | The Carrier's centre | The model's launch bay | Detail |
| A-6 | `LeashRetarget` | A leashed member returns until docked or back inside the leash with a valid target | Unknown | **verify** |
| A-7 | Upgrades Graviton Catapult, capacity, leash | Not modelled | S16 | Out of scope (§14): no upgrade row exists; a later row needs `MagazineLaunch` and `HangarSpec` values to be upgradable |

### D-13. The data rows (WP3)

`mothership_carrier`, in addition to D-1:

```ron
capabilities: (
    overclock: None,
    harvester: None,
    is_resource_dropoff: false,
    // 5.0.14 CarrierHangar (CAbilArmMagazine): liberty AbilData:2083-2098 (Leash 12, CountStart 4,
    // Manage Recall), swarm AbilData:467-469 (MaxCount 8), voidmulti AbilData:1296-1300 (Time 12 =
    // 214 ticks, AutoBuildOn); HangarQueue5 QueueSize 5 (liberty AbilData:2072-2076);
    // plan 2026-11-28-carrier-interceptors D-2.
    hangar: Some((member: "saucer_interceptor", max_count: 8, start_count: 4, build_ticks: 214,
                  queue_size: 5, auto_build: true, leash: 12.0, on_death: KillMembers)),
),
weapon: Some((
    name: "Interceptor Launch",
    // 5.0.14 InterceptorLaunch (liberty WeaponData:601-614): Range 8, Period 0.5 (9 ticks), DamagePoint 0,
    // Backswing 0; InterceptorLaunchPersistent 8 periods of 0.375 (voidmulti EffectData:427-429) = 7 ticks.
    attack_visibility: Hidden, launch_visibility: Hidden, visual_kind: None,
    max_range: 8.0, firing_arc_deg: 360.0, acquire_arc_deg: 360.0, target_flags: (bits: 255),
    base_damage: 0.0, attacks_count: 1, period_ticks: 9, damage_point_ticks: 0, backswing_ticks: 0,
    magazine_launch: Some((tries: 8, interval_ticks: 7)),
    // every other field as today's row (random delay, classes), projectile_speed: None
)),
effects: (weapons: []),
```

`saucer_interceptor` (new, `unit_type_id: 47`, next free; faction `cosmic_enclave`; `display_name: "Saucer
Interceptor"`; lore from the Carrier's "autonomous saucer interceptors"; `sc2_analogue: "Interceptor (Carrier
drone)"`; `shape: UnitSphere`; silhouette from the Enclave's small craft, chosen in WP3 by the visual rule Q0020):
`mover_type: Air`, `collision_class: Some("flying_escorts")`, attributes `LIGHT | MECHANICAL`, `max_hp: 40.0`,
`max_shields: 40.0`, `life_armor: 0.0`, `shield_armor: 0.0`, `shield_regen_per_tick: 0.112`,
`shield_regen_delay_ticks: Some(179)`, `max_speed: 0.42`, `acceleration` and `deceleration: 3.136` (1000 Normal),
`turn_rate_deg_per_tick: 55.99`, `collision_radius: 0.25`, `separation_radius: 0.25`, `sight_radius: 7.0`,
`scan_range` from the catalog's rule for an `Offensive` unit (WP3 reads how other rows derive it), `attack_target_priority:
20`, `acquire_level: Offensive`, `mineral_cost: 15`, `gas_cost: 0`, `supply_cost: 0`, `build_time_ticks` the hangar's
214 (validation does not read it; set for display), `selectable: false`, `order_targetable: false`,
`armor_class: Some("enclave_air_armor")`; weapon "Interceptor Beam": `visual_kind: LaserBeam`, `attack_visibility:
Visible(reveal_ticks: 54)`, `target_flags: (bits: 255)`, `max_range: 2.0`, `firing_arc_deg: 19.6875`,
`base_damage: 5.0`, `attacks_count: 2`, `strike_interval_ticks: 0`, `period_ticks: 54`, `damage_point_ticks: 3`,
`backswing_ticks: 9`, `random_delay_ticks: (-1.1161, 2.2321)` (the `CWeapon` default, as other rows),
`weapon_class: Some("enclave_air_weapons")`, `upgrade_bonus_per_level: 1.0`; `effects` one instant look (D-11);
`veo_prompts` for Idle, Walk, Attack written in the house style (a small golden saucer drone; Attack: twin short beams
on a strafing pass) in WP3, before WP5 generates from them. Every value carries its S10–S13 citation.

---

## 5. Determinism, checksum and coordination

- **Integers only.** Every new quantity is a `u8`, `u16` or `SimScalar`; distances use the existing fixed-point
  centre and edge distance helpers; no float enters state (RON floats convert once in the catalog, as today).
- **Order.** Members and hangars are visited in ascending entity id (the `BTreeMap` order). New entities take ids from
  `next_entity_id` in that order, so two runs of a seed create the same ids.
- **No new collection, randomness or clock.** `docked` is a `Vec` reserved to `max_count` at creation; nothing in the
  phase allocates in steady state except the entity a launch creates (as production does). The member weapon's random
  delay draws from the world's `ChaCha8Rng` like any weapon's.
- **Hashed** (`checksum/encoding.rs`): `Entity::hangar` (every field of `HangarState`, `DockedMember`,
  `LaunchChannel`), `Entity::hangar_link`, and `WeaponSpec::magazine_launch` (hashed with projectiles' weapon specs as
  every weapon field is). Archetype flags (`selectable`, `order_targetable`, `HangarSpec`) are catalog data, not
  hashed, as catalog data never is.
- **Latent proof (WP2).** With the three new fields left out of the encoding and the schema at `main`'s number, the
  tree reproduces every recorded checkpoint of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release
  and `wasm32-wasip1` debug and release (the weapon-delivery plan's `proofcopy.sh` and `unhash.py`, copied to
  `~/scratch/carrier-interceptors/scripts/`). Then the fields are encoded at the next free schema. WP2 to WP5 do not
  re-record; their golden tests fail by design, and each hand-back says so.
- **One re-record (WP6, final tree after merging `main`).** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM`, under the header "Re-recorded for plan 2026-11-28-carrier-interceptors (checksum schema
  M -> N): each entity encodes its hangar and hangar link, each weapon its magazine launch; with those left out at
  schema M every checkpoint reproduced. Behaviour: none in these worlds (no hangar unit appears)." Proved on the four
  builds. If `main` has taken N first, the merge repeats the re-record with the next free number.
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, on WP0's tree and the final tree: every
  scenario at **50 ticks per second or more**, instructions of the existing scenarios within **+0.5 %** (`perf stat -e
  instructions:u`, `DESIGN.md` §5's method; the hangar phase is skipped when no hangar archetype exists, and the shipped
  catalog has one, so the cost is one members pass per tick over entities with no link). WP6 adds one scenario to
  `throughput_scenarios.ron`, `hangar` (a 64×64 map, the first armies' hangar units, found by property, against an air
  and ground army of 200, about 100 members cycling), which must also run at 50 ticks/s or more, and gains a row in
  `DESIGN.md` §5.
- **Gated coordination** (D-10): code lives in `hangar/` (new), `world.rs` (two calls), `combat_resolution.rs`
  (`dispatch_shot`'s branch), `entity.rs`, `world/commands.rs` (admission of the two flags), the catalog, validation and
  the encoding. Nothing under `crates/sim_core/src/movement/`, nor `steering.rs`, `movement_rules.ron` or
  `target_acquisition.rs`. `entity.rs`, `checksum.rs` and `encoding.rs` are also in Gated's file list; those are merge
  conflicts handled by "next free at merge", not coordination breaches.
- **Weapon delivery.** This plan replaces that plan's interim (`mothership_carrier`'s salvo, `known_bug: "B-22"` in its
  W-10 census): WP3 changes the census row to a launch weapon with the S7–S9 citation and drops `known_bug`.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` and find shipped units by
property (an archetype with a hangar, a member, an air or ground enemy), never by slug, except the shipped-values
tables (H-12, H-13). WP2's tests author a hangar on a test catalog copy, because no shipped row has one until WP3.
Every test is written red first against the previous package's tree.

### 6.1 Vitals (WP1; `catalog_reference_values_tests.rs`)

- **H-1.** The Carrier's row reads 300/150, radius 1.25, separation 1.25, with S14's citations; the label `"Carrier"`
  leaves the vitals and radius exemption lists. Red on WP0's tree.

### 6.2 The mechanism (WP2; unit tests in `hangar/`, integration in new `crates/sim_core/tests/hangar_tests.rs`)

- **H-2 (creation).** A hangar unit spawned or trained has `start_count` docked members at full vitals, `launched` 0,
  `queued` 0; no member entity exists.
- **H-3 (build).** With resources, auto-build queues to `min(queue_size, max_count − docked − launched)`, charges each
  build's member cost at queueing, completes one per `build_ticks` exactly, never exceeds `max_count`; without
  resources it waits and queues on the first tick the owner can pay; `auto_build: false` queues nothing.
- **H-4 (launch).** A release at an enemy in range: the first member enters the world on the release tick at the hangar
  unit's centre, the next each `interval_ticks`, at most `tries` per channel and never more than are docked; a release
  during a running channel on the same target starts nothing; on a new target the channel restarts.
- **H-5 (attack).** Launched members attack the hangar's target with their own weapon (2 strikes of 5 on one tick
  every 54 ticks from range 2 in the shipped data); the struck player sees the member, not the hangar unit (`Visible`
  reveal on the member; the hangar unit gains no reveal, image or snapshot).
- **H-6 (retarget).** The hangar's target dies: the hangar unit's next release at a new target sends every launched
  member at it; with no new target, every member returns and docks, each docking on the tick it touches the hangar
  unit's edge, its vitals stored.
- **H-7 (leash).** The hangar unit moves away: a member beyond `leash` returns (`Follow`), docks or turns back once
  inside the leash with a valid target; a Move given to the hangar unit alone recalls nothing.
- **H-8 (death).** The hangar unit dies: every launched member dies on the same tick with a shatter event, docked and
  queued members are gone, no refund. A member dies: auto-build replaces it at cost.
- **H-9 (orders).** A player command naming a member drops it (a command naming only members is refused); an
  `AttackTarget`, `Follow`, `Load` or targeted cast at a member is refused with the typed reason; an enemy unit with no
  orders acquires and kills a member; splash damages members.
- **H-10 (validation).** Every rule of D-2, D-4 and D-8 refused with a typed error naming the field; omitted
  `hangar`, `magazine_launch`, `selectable`, `order_targetable` read as the defaults; the shipped catalog validates.
- **H-11 (encoding and invariants).** Two worlds differing only in one field of `HangarState`, `DockedMember`,
  `LaunchChannel`, `HangarLink` or `MagazineLaunch` have different checksums; a corrupted link or count is reported as
  a `HangarInvariant` diagnostic and leaves the hangar untouched; two runs of the same constructed fight are
  byte-identical (checksum per tick).

### 6.3 The data (WP3; `hangar_tests.rs`, `weapon_catalog_rules_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs`)

- **H-12 (reference values).** Tables for the member (life, shields, armour, attributes, speed, radius, sight, cost,
  priority) and its weapon (range, period, damage point, backswing, strikes, damage, arc), each citing S10–S13; the
  hangar's values citing S2–S5, S8; the Carrier leaves every remaining exemption list.
- **H-13 (delivery census).** The weapon-delivery W-10 table lists the Carrier's weapon as a launch weapon citing S7–S9,
  with no `known_bug`, and the member's beam as instant citing S11–S12.
- **H-14 (shipped fight).** The shipped Carrier against a constructed air target out of its range: it closes to 8,
  launches four members 7 ticks apart (start count), builds the other four at 214 ticks each, 15 minerals each.
- **H-15 (zero recompilation).** In a catalog directory copy, changing `max_count` to 4 or `leash` to 6 changes H-14's
  counts and recall tick in the same binary; removing `hangar` from the Carrier is refused (it has a launch weapon).

### 6.4 Client (WP4; client tests)

- **H-16 (presentable).** With the member's media absent from a fixture root, the hangar unit is unpresentable and its
  card entry is `NoMedia`; with both present (fixture atlases in a temporary root, the existing
  `support/battle_harness.rs::fixture_root` method), it is presentable. (`runtime_media_tests.rs` keeps its rule of
  finding an identity without media by asking.)
- **H-17 (selection).** In `battle_input_tests.rs`: a click, a box, a double-click on a member select nothing of it;
  a right click on a member under a selected army issues no order at the member.
- **H-18 (catalog looks).** `effect_catalog_tests` passes: the launch weapon needs no look; the member's beam is an
  instant weapon with its look.
- **H-19 (dock is not death).** In `battle_presentation_tests.rs`: a member docking leaves no shatter or death look; a
  member killed does.
- **H-20 (steady state).** `sync_steady_state_tests` and `sync_allocation_count_tests` (valgrind) with two Carriers
  cycling their members (launch, attack, dock, relaunch) for 600 ticks after warm-up: no allocation in steady state,
  no leaked visual.

### 6.5 Media (WP5)

- **H-21.** `asset_baker audit-media --strict` passes with both identities in the required set; `runtime_media_tests`
  and `runtime_assets_tests` load both flipbooks from the runtime paths; the review evidence names every accepted clip,
  its contact sheet and the spend.

### 6.6 World check (pre-registered)

Every world that differs from its base is explained by its **first difference** (`firstdiff.py` and a per-entity
trace) and classed:
- **S (schema bytes):** the goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the encoding alone (WP2's proof).
- **V (vitals):** the first difference is the Carrier's life, shields or radius (WP1). Allowed only in worlds WP0's
  census lists as carrying the Carrier.
- **H (hangar):** the first difference is a hangar state, a member entity, or the Carrier's launch release (WP3).
  Allowed only in worlds WP0's census lists as carrying the Carrier.
- **P (presentation):** a `PlayerView` or fog frame differs in images only.
- **Anything else:** a defect, stop rule 2.

### 6.7 Close battles

WP0 fixes the close-battle list from its baseline alone, by the rule the weapon-delivery WP0 applied (hold-facing
plan §6.4): every matrix battle whose minority side wins at least 2 of its 20 baseline runs; battles 31 and 33 at the
same share over their 100 and 64 seeds; every battle a closed owner question or fixture comment names close or
restated; no other; a `known_bug` battle is not close. (Weapon delivery's WP0 gave 14, 26, 31, 33, 35, 38a, 38b.) The
list is not edited after WP1 starts.

- **A battle whose outcome moves with a first difference of class V or H** moves because the Carrier now works like
  the Carrier: it is **restated under charter §8**, its claim or comment gaining the moved share, the first differing
  tick of one flipped seed traced, and the 5.0.14 source of the change (S10–S14). A battle off the close list moving
  this way is also reported to the orchestrator before WP6 merges (a decisive change, not a flip).
- **Any other outcome move** (class S, P or unexplained, on or off the list) **is stop rule 4.**
- WP0 predicts no measured world carries the Carrier (§3 item 10), so no outcome is expected to move at all.

### 6.8 Predictions (recorded before measuring, not thresholds)

| World | WP1 (vitals) | WP2 (latent) | WP3 (data) |
|---|---|---|---|
| Matrix (seeds of record and 16 development seeds) | byte-identical | byte-identical | byte-identical |
| Battles 31, 33 | byte-identical | byte-identical | byte-identical |
| Showcase, showcase effects (default and 1–8) | byte-identical unless WP0 finds the AI training the Carrier; then class V only | byte-identical | as WP1, class H |
| Skirmish scenario (6,000 ticks) | as the showcase | byte-identical | as the showcase |
| Goldens, sandbox | byte-identical | S, proved, not re-recorded | S; re-recorded once in WP6 |
| Client skirmish stage log | identical | identical | identical |

---

## 7. Measurement

### 7.1 WP0 census and probe (scratch, not committed)

- **(c) Census.** Every place the Carrier can enter a world: `matchup_matrix.ron` squads, battles 31 and 33, both
  goldens' squads, the sandbox, `scenarios/*.ron`, `start_loadouts.ron`, the throughput scenarios, and every
  structure whose `trains_units` names it (the AI and the client's card). Read from the files; printed with paths.
- **A scratch probe patch** (`~/scratch/carrier-interceptors/probe/`, from the weapon-delivery probe; it may name
  slugs, being scratch) logs per world and seed, changing no state: (p) Carrier entities present (spawned or trained),
  (r) salvo releases, (k) kills by the salvo, (d) damage taken by a Carrier. **Predicted zero in every matrix battle,
  battles 31 and 33, both goldens and the sandbox; the showcase, showcase effects and skirmish worlds are measured,
  not predicted.** A positive control (a scratch test spawning a Carrier beside an enemy for 300 ticks) proves a zero
  is a measured zero.
- **Close-battle list** (§6.7), **baseline** on four builds, **throughput and instructions**.

### 7.2 Commands

The weapon-delivery plan's scripts, copied to `~/scratch/carrier-interceptors/scripts/` (`measure.sh`, `compare.sh`,
`firstdiff.py`, `proof.sh`, `proofcopy.sh`, `unhash.py`, `heldout.sh`, `bench.sh`, `close.py`, `matrixdiff.py`) with
their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/carrier-interceptors-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/carrier-interceptors
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7). If `main` gains a Gated package between two measured packages, the next package's base is re-measured
after merging `main` (D-10), and the comparison is against that base.

### 7.3 Held-out seeds (WP6, confirmation only)

Once, on `main` and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase and
showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12. Byte-identical expected, or differences of
class V, H or P only in worlds WP0's census lists; anything else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. WP2's latent proof fails (a checkpoint differs with the new fields left out): stop; something other than the
   encoding moved.
2. A measured world's first difference is not of class S, V, H or P (§6.6), or is V or H in a world WP0's census did
   not list: stop, trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. An outcome moves that §6.7 does not allow (any move whose first difference is not V or H): stop for review, with the
   trace.
5. Throughput below 50 ticks/s in any scenario (the new `hangar` scenario included), or instructions of the existing
   scenarios above +0.5 %: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs` would
   need an edit (the Gated coordination rule): stop and report to the orchestrator; the strafe (B-229) is not a reason
   to edit them in this plan.
7. A steady-state or allocation test fails with members cycling (H-20): stop; the client's entity pool is fixed in its
   own amendment, never by excluding members from the test.
8. Veo is blocked, the preflight shows less than the batch cap remaining, or the batch would pass $10: stop WP5 and ask
   the owner (charter §1.2). No procedural, recoloured or borrowed media is committed in its place.
9. WP0's census finds the Carrier in a measured world the plan predicted empty, or the data contradicts §2.1 (a value
   of S2–S14 differs): stop WP0; the plan is amended before WP1.

---

## 9. Client consequences

D-11: `media.rs` (hangar units need their member's media), selection and picking (two archetype flags), the despawn
of a docked member, the effect catalog's launch weapon. New or extended tests H-16 to H-20. No renderer, shader or GPU
test is added: members are units drawn by the existing flipbook path, whose GPU tests run in WP4's package gate.
`PlayerView` gains nothing in this plan (the docked count on the card is B-230).

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (§13); §5 throughput row | WP0, WP6 |
| `docs/evidence/2026-11-28-carrier-interceptors.md` | New | WP0 to WP6 |
| `assets/catalog/units.ron` | `mothership_carrier` vitals (WP1); its hangar, launch weapon, `effects`; `saucer_interceptor` (WP3); the member's look (WP4) | WP1, WP3, WP4 |
| `crates/sim_types/src/commands.rs` | `ArchetypeCapabilities::hangar` (RON form) | WP2 |
| `crates/sim_types/src/weapon.rs` | `MagazineLaunch`, `WeaponSpec::magazine_launch`; unit tests | WP2 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | `HangarSpec`, `selectable`, `order_targetable`; D-2, D-4, D-8 validation | WP2 |
| `crates/sim_core/src/hangar/{mod,phase,launch}.rs` (new) | D-5 to D-9, `HangarInvariant`, unit tests | WP2 |
| `crates/sim_core/src/entity.rs` | `HangarState`, `DockedMember`, `LaunchChannel`, `HangarLink`; creation | WP2 |
| `crates/sim_core/src/world.rs` | `step_hangars`, `reap_hangar_members` calls; the catalog flag | WP2 |
| `crates/sim_core/src/world/commands.rs` | Admission of `selectable` and `order_targetable` | WP2 |
| `crates/sim_core/src/world/upkeep.rs` | The shield rule callable on a docked record (only if not already a pure function) | WP2 |
| `crates/sim_core/src/combat_resolution.rs` | `dispatch_shot`'s launch branch | WP2 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema line; the new `fields!` | WP2 |
| `crates/sim_core/tests/hangar_tests.rs` (new) | H-2 to H-11 (WP2), H-14, H-15 (WP3) | WP2, WP3 |
| `crates/sim_core/tests/{catalog_reference_values_tests,weapon_catalog_rules_tests,runtime_catalog_tests,checksum_state_tests}.rs` and test files whose literals list every `WeaponSpec` or `ArchetypeCapabilities` field | H-1, H-10 to H-13; `magazine_launch: None`, `hangar: None` in literals | WP1, WP2, WP3 |
| `crates/client/src/media.rs`, the selection and picking code under `crates/client/src/battle_pipeline/` | D-11 items 1 to 3 | WP4 |
| `crates/client/tests/{battle_input_tests,battle_presentation_tests,effect_catalog_tests,battle_card_tests,sync_steady_state_tests,sync_allocation_count_tests}.rs` | H-16 to H-20 | WP4 |
| `assets/videos/`, `assets/textures/` (new clips, atlases, provenance), `assets/media/required_set.ron`, `docs/evidence/veo/` | WP5 media and review | WP5 |
| `crates/sim_core/benches/throughput_scenarios.ron` | The `hangar` scenario | WP6 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded once (§5) | WP6 |
| `crates/gate/suites.ron` | `hangar_tests` as a target of the rules its files match, if not already a `fast` target | WP2 |
| `docs/questions/open/0129-…` → `closed/` | `## Resolution`, `git mv` | WP6 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md`, `docs/plans/2026-11-27-weapon-delivery.md` | Amendment pointers (B-22 fixed here) | WP6 |
| `docs/plans/README.md` | Row | plan, WP1, WP6 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`transport/`, every shader.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the code makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/carrier-interceptors/`. GPU tests lock the GPU themselves; `cargo test` is never wrapped in
`with-gpu-lock`. WP5 (media) depends only on WP3's prompts and may run beside WP4; every other package is sequential.

### WP0: records, census, probe, baseline (XS, docs and scratch)

After the weapon-delivery plan is on `main`. §13's WP0 rows (B-22 and B-228 restated with Q0129's answer and this
plan; B-229, B-230 registered); §7.1's census, probe, positive control, close-battle list, baseline on four builds,
throughput and instructions. **Acceptance:** stop rule 9 holds; the evidence lists every world's (p), (r), (k), (d).
Commit `docs(design): restate B-22 and B-228, register B-229 and B-230 (plan 2026-11-28-carrier-interceptors WP0)`.

### WP1: the Carrier's vitals (XS, data and tests)

D-1. Red first: H-1. **Acceptance:** H-1 passes; every measured world byte-identical to WP0's, or class V only in a
world WP0 listed; stop rules 2 and 4 hold. Commit `feat(catalog): the Mothership Carrier takes the Carrier's life,
shields and radius (B-228; Q0129 (a))`.

### WP2: the hangar mechanism, latent (L, `sim_types`, `sim_core`)

D-2 to D-9 and §5's encoding. Red first: H-2 to H-11 on a test catalog with an authored hangar. No shipped row
changes. **Acceptance:** H-2 to H-11 pass; every measured world byte-identical to WP1's; goldens differ by the schema
only (proved, not re-recorded); stop rules 1 and 6 hold. Commit `feat(sim_core): hangars, launch weapons and launched
members (B-22; latent)`.

### WP3: the data (M, catalog and `sim_core` tests)

D-13. Red first: H-12 to H-15 on WP2's tree. Then the Carrier's hangar and launch weapon, the `saucer_interceptor`
row (with its `veo_prompts`), the reference and census tables. Worlds measured against WP2's (§6.6 to §6.8).
**Acceptance:** H-12 to H-15 pass; every world's first difference classed; stop rules 2 and 4 hold. The member has no
media yet, so the client keeps the Carrier locked (`NoMedia`), which WP4's H-16 pins. Commit `feat(catalog): the
Carrier launches interceptors from a hangar (B-22)`.

### WP4: client (M, `client` and data)

D-11 items 1 to 4; H-16 to H-20. **Acceptance:** they pass; the GPU and diagnostics suites green in the package gate;
stop rule 7 holds. Commit `feat(client): hangar members are drawn, never selected or ordered`.

### WP5: media (S, paid; `asset_baker` runs, assets, evidence)

D-11's batch: preflight, generation, review, bake, provenance, required set; H-21. **Acceptance:** H-21 passes; the
spend is within the cap; stop rule 8 holds. Commit `feat(media): Veo clips and flipbooks for the Carrier and its
interceptor`.

### WP6: confirmation, re-record, records (S)

Held-out seeds (§7.3); the `hangar` throughput scenario and the throughput and instructions comparison (§5); goldens
and sandbox re-recorded once on the final tree after merging `main`, proved on four builds; §13's WP6 rows; Q0129's
`## Resolution` and its move to `closed/`; the amendments of the sc2-deviation-fixes and weapon-delivery plans;
`## As built`. **Acceptance:** stop rules 3 and 5 hold; B-22 and B-228 fixed. Commit `docs: close B-22 and B-228,
Q0129 (plan 2026-11-28-carrier-interceptors WP6)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `weapon.rs`, `commands.rs` | Defaults and RON forms of `MagazineLaunch` and the hangar |
| Unit | `sim_core` `hangar/`, `catalog/validation.rs` | D-5 steps in isolation, H-10 |
| Integration | `crates/sim_core/tests/hangar_tests.rs` (new) | H-2 to H-9, H-11, H-14, H-15 through `SimWorld` and `PlayerCommand` |
| Integration | `catalog_reference_values_tests.rs`, `weapon_catalog_rules_tests.rs`, `runtime_catalog_tests.rs`, `checksum_state_tests.rs` | H-1, H-10 to H-13, H-15 |
| Determinism | goldens, sandbox, four builds | §5: latent proof, one re-record |
| Client | `battle_input_tests.rs`, `battle_presentation_tests.rs`, `effect_catalog_tests.rs`, `battle_card_tests.rs` | H-16 to H-19 |
| Steady state | `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) | H-20 |
| Media | `runtime_media_tests.rs`, `runtime_assets_tests.rs`, `asset_baker audit-media --strict` | H-21 |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s (the new scenario included), ≤ +0.5 % instructions on the existing ones |

---

## 13. Records

| File | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | **B-22** restated: "the Carrier launches up to 8 interceptor units from a hangar (S2–S12); plan `2026-11-28-carrier-interceptors`; Q0129 (a)"; **B-228** restated: "Q0129 (a): the Carrier; vitals in WP1 of the same plan" | WP0 |
| `DESIGN.md` §4 | **B-229** (provisional): "Interceptors hover in range instead of strafing: 5.0.14's `InterceptorBeam` is a `CWeaponStrafe` (`liberty` WeaponData:626), whose user flies through its target and comes back; ours stops at range 2 as any ranged air unit. Damage rate unchanged, position differs. Deferred until after the Gated movement plan's WP4 (its files); Q0130" | WP0 |
| `DESIGN.md` §4 | **B-230** (provisional): "The Carrier's hangar has no card commands: no manual interceptor build, no cancel (`HangarQueue5,CancelLast`), no auto-build toggle, no docked count shown (`liberty` UnitData:3521–3522); auto-build only. Deferred" | WP0 |
| `DESIGN.md` §4 | B-22 and B-228 **fixed** with commits and figures; B-8 gains "the Interceptor's priority is 20 in the data (`liberty` UnitData:3561), not 19" | WP6 |
| `DESIGN.md` §5 | The `hangar` throughput row | WP6 |
| `DESIGN.md` Combat row | Names hangars and launch weapons | WP6 |
| `docs/questions/closed/0129-…` | `## Resolution`: "carried out by plan `2026-11-28-carrier-interceptors` (commits)" | WP6 |
| `docs/questions/open/0130-…` | Filed from §15 by the orchestrator when it accepts this plan (number next free at filing) | plan acceptance |

---

## 14. Out of scope and decisions recorded (Q0002)

- **The strafe** (B-229): a movement behaviour; after Gated WP4 (D-10). Q0130 lets the owner hold the Carrier back
  instead.
- **Hangar card commands** (B-230): manual build, cancel, auto-build toggle, docked count.
- **Graviton Catapult, hangar capacity and leash upgrades** (S16): our catalog has no such rows. A later row needs
  upgrade references to `MagazineLaunch::interval_ticks`, `HangarSpec::max_count` and `leash`, which `upgrades.ron`'s
  effect kinds do not reach today.
- **Release Interceptors** (a removed Legacy of the Void beta ability) and the Brood Lord's broodling hangar
  (`BroodLordHangar`, `liberty AbilData:3810+`, B-21): the hangar mechanism is general (a `HangarSpec` on a unit whose
  weapon launches), but the Brood Lord's escorts and broodling spawns differ (`External`, `Release` effects) and are
  B-21's plan.
- **A matrix battle with the Carrier** (for example Carriers against the Viking or Corruptor analogues): useful, but a
  battle needs a sourced expectation; left to the matrix capability work.
- **Teams:** unchanged (Q0101 4(b)).

---

## 15. Owner question (filed on main as [Q0130](../questions/open/0130-interceptors-hover-or-strafe.md))

````markdown
# Q0130: Interceptors hover instead of strafing until the movement work ends; is that acceptable for now?

- **Asked:** 2026-11-28
- **Blocking:** no
- **Default until answered:** (a); the Carrier ships with interceptors that stop in range and fire, and the strafe is
  recorded as bug B-229

## Question

In StarCraft II a Carrier's interceptors attack by strafing: each flies through its target, turns, and comes back for
another pass. Building the strafe means changing how units move, and the movement code is being rebuilt (the Gated
plan); no other work may change it until that ends. Everything else about interceptors can be built now: up to 8 per
Carrier, built one by one, launched, shot down, recalled, and destroyed with their Carrier. Until the strafe exists,
an interceptor would stop at its attack range (2) and fire from there, like any small ranged flyer. It deals the same
damage per second; it is easier to hit with splash and short-range weapons while it hovers.

- (a) **Ship interceptors now, hovering**, with the strafe recorded as bug B-229 and built after the movement work.
  **Recommended**: the Carrier works as a Carrier at once; the difference is one of position, and it is written down.
- (b) **Wait**: keep today's missile salvo until the strafe can be built too.

## Answer

````

---

## 16. Material (scratch, not committed)

- Game data `~/scratch/combat_wp3_data/` (`5444e12f13`): `liberty` AbilData:2072–2098, :3810–3820; `swarm`
  AbilData:467–469; `voidmulti` AbilData:1296–1300; `balancemulti` AbilData:1133–1137; `liberty` WeaponData:601–637;
  `balancemulti` WeaponData:279–310; `voidmulti` WeaponData:306–330; `liberty` EffectData:893–913, :1940–1958;
  `voidmulti` EffectData:427–429, :2262–2283; `balancemulti` EffectData:339–341, :2115–2146; `liberty`
  UnitData:3469–3604, :4244–4316; `balancemulti` UnitData:134–142, :859–866; the other layers' `Carrier` entries;
  `liberty` ValidatorData:1117–1120 (`CasterNotDead`); `libertymulti` UpgradeData:47–87; `liberty` UpgradeData:327–328,
  :438–439, :728; `voidmulti` UpgradeData:887–908; `units_dump_lotv.txt` for the Carrier and Interceptor. A grep of
  every layer's `EffectData.xml` for `InterceptorFate` finds no definition (S6).
- Outside sources (Q0008), 2026-11-25: web search excerpts of S18 and S19; the pages themselves answered HTTP 429
  (Liquipedia) and 402 (Fandom) to a fetch, hence medium confidence and the **verify** marks.
- Code read on `39df7177` and `weapon-delivery-plan` `1943995d`: `entity.rs` (`Entity`, `from_archetype`,
  `transport_carrier`, `takes_part_in_movement`, `is_housed`), `transport/mod.rs`, `world.rs` (`step_into`,
  `spawn_unit_facing`, `reap_dead_and_emit_shatter`), `catalog.rs` (`UnitArchetype`, `collision_class`),
  `catalog/collision_classes.rs`, `sim_types` `commands.rs` (`ArchetypeCapabilities`, `UnitOrder`, `AttackOrigin`,
  `FollowKind`), `weapon.rs` (`AttackVisibility`), `enums.rs` (`WeaponVisualKind`), `published_actions.rs`,
  `client/src/media.rs` (`presentable`), `battle_hud/{card,catalog}.rs` (`NoMedia`), `units.ron`'s
  `mothership_carrier`, `structures.ron`'s `trains_units`, `assets/media/{required_set,veo_budget}.ron`,
  `throughput_scenarios.ron`, `catalog_reference_values_tests.rs` (the `"Carrier / Mothership"` rows),
  `workspace_checks/src/identities.rs` (C5: no slug in production code; `hangar`, `member`, `magazine` name no
  identity), the weapon-delivery plan and evidence (probe, close list, throughput), the Gated plan §7 and §14, the
  collision-classes plan §2.
- No prototype: the mechanism is bookkeeping over existing orders and spawns, pinned by H-2 to H-11.
