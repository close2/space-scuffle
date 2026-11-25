# Blueprint: The Turret That Tracks Under Any Order — the Siege Tank's Hidden Weapon (B-198)

This plan settles **B-198** of `DESIGN.md` §4 against StarCraft II 5.0.14:

- **B-198: the Siege Tank analogue has no second, hidden weapon.** 5.0.14's unsieged `SiegeTank` carries
  `90mmCannonsFake` beside `90mmCannons` (`balancemulti` UnitData:1473, `voidmulti` :1575). It is a hidden weapon
  that may not start an attack, clears both attack-order options, and strikes with a `CEffectDamage` that has no
  amount, no notification and no response (`balancemulti` WeaponData:7–20, EffectData:1066–1072). It shares the
  tank's turret with the real cannon, so the turret turns towards an enemy in reach under any order: while the tank
  moves under a Move order, and while it chases an attack target out of reach past another enemy. Ours has one
  weapon, whose data keeps both attack-order options, so its turret idles (resets to the hull while it moves) under
  every order but an attack or a hold.

The plan models the hidden weapon as what it is observably: a **turret capability of the host weapon** that aims the
shared turret, never fires, never draws, never charges a cooldown and never touches an order (D-1). No code names a
unit (charter §3.3, C5); the only shipped carrier is the tank-mode weapon of `railgun_crawler`, in data.

A full plan is required (charter §6): a new mechanic in the simulation, a change of hashed encoding (checksum
schema), golden replays that move by behaviour (turret angles), and work across `sim_types`, `sim_core`, the catalog
and the fixtures.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-25 against `main` at
`91350af1` (checksum schema 48).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (<role>, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- Numbers are provisional, next free at merge: bug rows from **B-218**, the question draft **Q0116**, the schema
  (49 if nothing else takes it; the collision-classes branch takes none).

**Questions and standing rules.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0103's standing guidance (charter §8): a sound behaviour change that moves a close battle is restated with evidence; broken behaviour is a bug | Standing rule | §6.4, §8 |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S12 to S14 (patch notes) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0109 parts 2 and 3 (answered (a)): turrets face their target between shots; the order's target comes first | Closed | D-4 (the real weapon owns the turret whenever it works) |
| The owner's guidance: no fitting to fixed seed sets; held-out seeds only confirm | Followed | §6, §7.4 |
| `docs/questions/open/` on `91350af1`: Q0113 (enemies block, trade-off), Q0115 (rally card keys) | Read, unrelated | — |
| **Q0116** (provisional, §15): how the tracked enemy is chosen, and whether the Immortal and Colossus turrets also aim while walking | Draft, not filed | D-3, §2.4 (B-218) |

---

## 1. Summary

| | Today (`91350af1`) | After this plan |
|---|---|---|
| Tank-mode turret, unit under a Move order, enemy in reach | Idles: `ResetIfMoving` turns it to the hull while the unit moves, holds while it stands | Turns towards the best enemy in reach at the turret's rate (D-3, D-4) |
| Same, under an attack order whose target is out of reach, another enemy in reach | Holds where it is (the weapon has a target, so no idle; out of band, so no slew) | Turns towards that other enemy; the real weapon takes the turret back on the tick its target enters the band (D-4) |
| Same, under an attack order or a hold with a workable target | The real weapon aims and fires | Unchanged: the real weapon owns the turret (D-4 rule 1) |
| Sieged tank, or a tank in its siege or unsiege channel | Sieged weapon; nothing stepped during the channel | Unchanged: the sieged weapon has no tracking, and no weapon steps during a channel (S6, D-5) |
| Orders, path, `current_target`, `weapon_target`, weapon phase, cooldown, RNG | — | Unchanged by tracking on every tick (D-5) |
| Damage, swings, attack snapshots, reveals, retaliation, flight, alerts | — | None from tracking (S3, D-5) |
| `WeaponSpec` | … `fires_at_other_targets`, `locks_turret_while_firing`, … | + `turret_tracks_any_enemy: bool` (omitted reads `false`) |
| `Entity` | `weapon_target` | + `tracked_target: Option<EntityId>`, written by acquisition only, read by the combat phase for the turret only |
| Hashed state | — | Both fields; schema **next free at merge** (49 provisional) |
| Worlds that move | — | Only worlds where a tank-mode tank has a free turret and an enemy in reach (probe (t)); in them only turret yaw (class Y), unless a later real swing depends on the turret's angle (class P, pre-aim, §6.3) |
| Size | — | S–M: WP0 (records, probes), WP1 (latent fields and data), WP2 (the rule), WP3 (confirmation, one golden re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [COMM] community, patch notes or editor documentation (Q0008); [INF] inferred;
**verify** not confirmed.

### 2.1 Sources

Game data is SC2GameData `5444e12f13` (5.0.14.94342), local copies under `~/scratch/combat_wp3_data/<layer>/`, read
as data. The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti →
balancemulti`; a later layer replaces only the fields it names, and a `WeaponArray` entry without an index appends.

| # | Fact | Data or source | Tag |
|---|---|---|---|
| S1 | `90mmCannonsFake`: `Options Hidden 1`, `CanInitiateAttackOrder 0`, `OnlyFireWhileInAttackOrder 0`, `OnlyFireAtAttackOrderTarget 0`, `LinkedCooldown 0`; `TargetFilters "Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable"`; `MinScanRange 7.5`, `Range 7`, `Period 1.04`; `Effect 90mmCannonsDummy`. No `Arc`, `DamagePoint`, `Backswing`, `AllowedMovement` or `RandomDelay`: the defaults | `balancemulti` WeaponData:7–20; identical `voidmulti` WeaponData:7–20 | [DATA] |
| S2 | The real cannon has the same range, period and filters: `90mmCannons` `TargetFilters` as S1, `Range 7`, `Period 1.04` (`liberty` WeaponData:237–243), `MinScanRange 7.5`, `Arc 360` (`balancemulti` :3–6) | as cited | [DATA] |
| S3 | `90mmCannonsDummy`: a `CEffectDamage` (parent `DU_WEAP`) with no `Amount`, `ResponseFlags Acquire 0`, `Flee 0`, `Visibility Hidden`, `Flags Notification 0`. It inherits `DU_WEAP`'s `SearchFlags CallForHelp 1` (`core` EffectData:121–130) and `CEffectDamage`'s defaults (`core` EffectData:61–70) | `balancemulti` EffectData:1066–1072; `voidmulti` :1191–1197 | [DATA]; whether a zero-amount hidden hit calls for help is **verify** |
| S4 | Both weapons sit on one turret: `90mmCannons` on `Turret="SiegeTank"` (`liberty` UnitData:5127), `90mmCannonsFake` appended on the same `Turret="SiegeTank"` (`balancemulti` UnitData:1473, `voidmulti` :1575). The sieged unit carries only `CrucioShockCannon` on `SiegeTankSieged` (`liberty` UnitData:5181) | as cited | [DATA] |
| S5 | The `SiegeTank` turret is `FreeRotate` with `Idle ResetIfMoving` | `liberty` TurretData:77–79 | [DATA]; ours is `turret_idle: ResetIfMoving(40.32)` |
| S6 | The fake is disabled through each morph: `SiegeMode`'s facing section starts `SiegeTankMorphDisableDummyWeaponAB`, a 4 s buff with `WeaponDisableArray 90mmCannonsFake`; `Unsiege` starts `SiegeTankUnmorphDisableDummyWeaponAB`, 3.5 s | `balancemulti` AbilData:772–778, :954–958, BehaviorData:406–421, EffectData:1073–1080; `voidmulti` AbilData:915–921, :1112–1118, BehaviorData:465–480 | [DATA]. 4 Normal s and 3.5 Normal s are 71.4 and 62.5 ticks at 25 per Faster second (S-5 of Q0052), our `cast_ticks 72` and `leave_cast_ticks 63` [INF] |
| S7 | Engine defaults the fake inherits: `ArcSlop 11.25`, `DamagePoint 0.167`, `Backswing 0.5`, `RandomDelayMin -0.0625`, `RandomDelayMax 0.125`, `FaceTargetWhileInCooldown 1`; no `AllowedMovement` (the turret wind-up plan's S2: the unit stands to fire) | `core` WeaponData:3–39 | [DATA]; the meaning of a missing `AllowedMovement` is that plan's S10 [COMM] |
| S8 | `CanInitiateAttackOrder 0`: the weapon never makes its unit start an attack; `LinkedCooldown 0`: its cooldown is its own, not shared with the unit's other weapons | Editor fields "Can Initiate Attack", "Linked Cooldown" | [COMM] (editor field names), **verify** the exact engine meaning of `LinkedCooldown` |
| S9 | `OnlyFireWhileInAttackOrder 0` and `OnlyFireAtAttackOrderTarget 0` let a weapon work outside attack orders and against targets it chooses itself | The turret wind-up plan's S11, the fire-rules plan's S9 | [COMM] |
| S10 | A weapon's turret follows the weapon's target; firing is gated by `AllowedMovement` separately | [INF] from S11–S13: the 2015 note says the tank's turret tracks while moving, and the fake, which has no `AllowedMovement`, is the only data that can do it | [INF], **verify** |
| S11 | Legacy of the Void beta balance update (16 July 2015), General: Siege Tanks and Immortals track the nearest enemy while moving | Blizzard news, LotV beta balance updates (<https://news.blizzard.com/en-us/starcraft2/19825246>; the StarCraft Wiki's LotV beta patch page) | [COMM]; exact wording **verify** (the wiki page refused the fetch; the search result paraphrased it) |
| S12 | 5.0.13 (March 2024): "Siege Tank (Unsieged) Turret now tracks previously attacked unit." and "Immortal Turret now tracks previously attacked unit."; the Cyclone's "Weapon now has turret tracking" | <https://news.blizzard.com/en-us/article/24078322/starcraft-ii-5-0-13-patch-notes> | [COMM], quoted from the page |
| S13 | 5.0.14: "Siege Tank and Immortal tracking change from previous patch reverted due to causing unintentional retargeting." | <https://news.blizzard.com/en-us/article/24162754/starcraft-ii-5-0-14-patch-notes> | [COMM], quoted from the page |
| S14 | No layer gives the fake an actor event of its own; this copy has no `ActorData` | `ls ~/scratch/combat_wp3_data/*/` | [DATA] absence in our copy; whether the tank plays an attack animation when the fake strikes is **verify** |

### 2.2 Survey: hidden, dummy and fake weapons of the eight layers

Every `CWeapon*` whose id contains `Fake` or `Dummy`, or that sets `Options Hidden 1` on a unit's `WeaponArray`
(`grep -n 'CWeapon[A-Za-z]* id="[^"]*\(Fake\|Dummy\)' */WeaponData.xml`; the carriers by `grep -n 'Link="<id>"'
*/UnitData.xml */BehaviorData.xml`):

| Weapon | Where | Carrier in 5.0.14 | What it is | Tracking-only? |
|---|---|---|---|---|
| `90mmCannonsFake` | `voidmulti`, `balancemulti` WeaponData:7 | `SiegeTank` (unsieged) | S1: harmless strike, both order options cleared, own cooldown, same turret | **Yes. Modelled here** |
| `CycloneLockOnDummy` | `void` WeaponData:34–43 (Arc 360, Range 7, `CycloneFakeWeaponDummyDamage`, an empty `CEffectDamage`, `void` EffectData:1009–1011) | Added by `LockOnDisableAttack` while a Cyclone is locked on (`void` BehaviorData:395–405); `balancemulti` BehaviorData:232–237 removes that `WeaponArray` entry | Was tracking-only during Lock On; removed in 5.0.14, whose Cyclone weapon tracks by itself (S12) | Not in 5.0.14; no analogue |
| `CycloneFakeWeapon` | `void` WeaponData:44–53 | No `WeaponArray` links it | Unused | — |
| `BattlecruiserWeaponSwitch` | `balancemulti` WeaponData:42–66 | `Battlecruiser` slot 0 (`balancemulti` UnitData:98) | Hidden selector whose effect `BattlecruiserDamageSet` deals the real damage | No (a real weapon); no analogue |
| `PurifierBeamDummyTargetFire` | `voidmulti` WeaponData:375–392 | `voidmulti` UnitData:1059 | Hidden target-fire helper (`Effect TargetFirePriority`) | No; no analogue |
| `OracleDisplayDummy`, `DisruptionBeamDisplayDummy`, `VoidRaySwarmDisplayDummy`, `VoidRaySwarmDummy`, `TalonsDummy`, `WidowMineDummy`, `VolatileBurstDummy`, `InterceptorsDummy` | `balancemulti`, `voidmulti`, `void`, `swarm`, `swarmmulti`, `liberty` | Display (tooltip) weapons, disabled or unlinked, or weapons with a real effect | UI or real attacks | No |

So the tank's fake is the **only tracking-only weapon on a 5.0.14 multiplayer unit** [DATA]. The Immortal tracks
while moving (S11) by its own weapon, `PhaseDisruptors`, whose order options are cleared (the fire-rules plan's §2.3);
that is B-218 (§2.4).

### 2.3 The rules that follow

1. **The tracking weapon never harms and never commands** (S1, S3, S8). It deals nothing, alerts nobody, reveals
   nothing, makes nobody flee, starts no attack, and its cooldown is its own. Its only lasting effect is on the shared
   turret (S4). [DATA + INF]
2. **It works under any order and against any enemy it reaches** (S1, S9): Move, an attack whose target it cannot
   work, idle and hold alike. Under an attack or a hold with a workable target, the real cannon works against that
   target (index 0 of the weapon array, the order's target first; Q0109 part 3) and its target owns the turret
   [INF, **verify** how the engine arbitrates two weapons on one turret].
3. **It aims while the unit moves, and fires only while it stands** (S7, S10, S11). Its strike is harmless (rule 1),
   so firing is unobservable but for a possible animation (S14) and a possible hold of a standing unit for the
   0.167 s damage point; aiming is the observable. [INF]
4. **Its choice is the nearest enemy, re-ranked as acquisition ranks, with no memory of a previous target** (S11:
   "nearest enemy"; S12–S13: keeping a "previously attacked unit" was tried in 5.0.13 and reverted in 5.0.14).
   [COMM + INF, **verify**, Q0116 part 1]
5. **It reaches what the real cannon reaches** (S1, S2): range 7, the same filters (ground, visible, not dead,
   hidden or invulnerable), the same turret. [DATA]
6. **It is off while the tank sieges or unsieges, and absent while sieged** (S4, S6). [DATA]

### 2.4 Neighbouring rules: in or out

| Rule | Source | Decision |
|---|---|---|
| B-198, the tank's tracking weapon | S1–S6 | **In** (D-1 to D-7) |
| A weapon that may work outside attack orders but must stand to fire (Immortal `Slowing`, Colossus `Stopped`) does not aim its turret while its unit walks under a Move: ours classes such a unit `Engagement::Busy` while walking. The 2015 note (S11) says the Immortal tracks the nearest enemy while moving — **registered here as B-218** (provisional) | S10, S11; `PhaseDisruptors`, `ThermalLances` (fire-rules §2.3) | **Out, deferred.** Same aim-without-fire mechanism, other units, narrow arcs (Immortal 11.25°, Colossus 56.25°), so it moves first shots and outcomes; it needs its own probe and close-battle review. §14 F-1 says how D-3 extends to it. Q0116 part 2 |
| The fake's strike: a 0.167 s damage point that would hold a standing unit, an attack animation, a possible call for help (S3, S7, S14) | S3, S7, S14 | **Not modelled, recorded as a minor detail** (charter §8 allows it): no swing exists, so nothing holds, animates or calls. **verify** in game (Q0116 part 3) |
| A tracking range of its own (`MinScanRange 7.5` against `Range 7`) | S1 | **Not modelled:** the fake's range equals the host's (S2); tracking uses the host weapon's reach from where the unit stands, as the fire-rules own-target scan does. A field is added when a weapon's tracking range differs (§14 F-2) |
| The Cyclone's lock-on dummy | §2.2 | **Out:** removed in 5.0.14; no analogue |
| B-89 (the Liberator's second turret entry) | `void` UnitData:3317–3318 | **Unchanged**, a separate row |
| The contact solver, `movement/`, enemy contacts | — | **Out** (task scope): nothing under `crates/sim_core/src/movement/` changes |

---

## 3. Diagnosis (on `91350af1`, read from the code)

1. **One weapon per plane.** `UnitArchetype::weapon_against` (`catalog.rs`) returns one weapon per target plane
   (ground, air, or the better of both for a cliff walker); the sieged override replaces the ground weapon. The tank
   authors one `weapon` (`units.ron`, `railgun_crawler`): `firing_arc_deg 360`, `acquire_arc_deg 360`,
   `has_turret`, `turret_turn_rate_deg_per_tick 40.32`, `turret_idle ResetIfMoving(40.32)`. Its siege override:
   `firing_arc_deg 11.25`, `acquire_arc_deg 0.0`, `turret_turn_rate 20.16`, `turret_idle Hold`.
2. **Who has a target.** `SimWorld::ordered_target` (`combat_resolution.rs`) gives an attack order's target (or
   `weapon_target`, fire rules D-4), a holder's `current_target`, or, only for a weapon with
   `fires_outside_attack_order`, the `current_target` acquisition tracks under another order. The tank's weapon
   clears neither option, so under a Move it has no target.
3. **The turret without a target.** `step_entity_weapon` (`combat.rs`) calls `idle_turret` when `target_opt` is
   `None`: `ResetIfMoving` turns the turret to the hull on a tick the unit was displaced. With a target out of the
   engagement band and the weapon `Ready`, `step_ready_weapon` returns before any slew: the turret holds.
4. **Between shots** (fire rules D-3, B-197) the turret faces the target in `Backswing` and `Cooldown`; a locking
   weapon holds from `Windup` to the end of `Backswing` (D-2, B-179).
5. **Acquisition** (`target_acquisition.rs`) skips units that are channelling, action-locked, hidden or incomplete;
   `engagement_of` classes a walking unit under a non-attack order without a weapon that may fire under it as
   `Busy`. `decide_weapon_target` is the model for a second, order-neutral decision: a stationary scan
   (`ScanScope::stationary`) with a licence on the weapon resolved for the candidate's plane, ranked by
   `TargetPriorityKey` with the archetype's tie-break, applied in entity order.
6. **No weapon steps during a mode channel** (`SimWorld::steps_weapon`: `channeling_remaining_ticks == 0`), and the
   turret angle is kept through a mode transition (no writer of `turret_angle` outside `combat.rs` but spawn,
   `world.rs`). So the angle a tank-mode turret has when the tank starts to siege is the sieged weapon's first aim.
7. **Hashed state.** `turret_angle` is hashed (`fields!(Entity { … turret_angle … })`, `checksum/encoding.rs`) and
   published as `aim_angle_rad` (`world/snapshot.rs`); `WeaponSpec` is hashed inside each in-flight projectile;
   schema 48 (`CHECKSUM_SCHEMA_VERSION`).

---

## 4. Design decisions

### D-1. A turret capability of the host weapon, not a second weapon

`WeaponSpec::turret_tracks_any_enemy: bool` (RON `turret_tracks_any_enemy`, `#[serde(default)]`, omitted reads
`false`): "the turret of this weapon also aims, under any order, at the best enemy this weapon reaches from where its
unit stands, whenever the weapon itself leaves the turret free; nothing is fired" — 5.0.14's hidden second weapon on
the same turret (S1–S4). Placed after `locks_turret_while_firing` in the struct, in `WeaponSpecDef`, and in
`fields!(WeaponSpec { … })`. A mode override carries its own (the tank's siege weapon omits it: S4).

**Alternatives considered.**
- **(a) A tracking-only second weapon** (a `tracking_weapon: Option<WeaponSpec>` slot, or a `tracking_only` flag on
  any weapon). Faithful in structure, but our unit chooses one weapon per plane (§3 item 1) and every consumer of
  weapons (`weapon_against`, `resolved_weapon`, `scan_extent`, `engagement_of`, upgrades, the render snapshot's
  weapon list, the client card) would have to learn to skip it. Of its ~30 fields only range, filters and turret
  are observable (rules 1 and 5), and all three equal the host's (S2, S4). The rest would be data that no rule
  reads. **Rejected.**
- **(b) `turret_tracking: Option<TurretTrackingSpec { max_range }>`.** A capability with its own reach. No 5.0.14
  value differs from the host's (S2), so the field would have no source. **Deferred** to the first weapon that needs
  it (§14 F-2).
- **(c) A `TurretIdle` variant.** `TurretIdle` is the turret's `Idle` field (S5), turret data; tracking is weapon
  data and runs before the idle (D-4). **Rejected.**

Charter §3.3: a capability queried on the weapon, never a branch on a `UnitTypeId`.

### D-2. `Entity::tracked_target`

`Entity::tracked_target: Option<EntityId>`: the enemy a weapon with `turret_tracks_any_enemy` aims the turret at this
tick. Written **only by acquisition** (D-3), read **only by the combat phase for the turret** (D-4). Pursuit, the
wind-up hold, `ordered_target`, the render snapshot's swing and every order rule never read it. Hashed after
`weapon_target`.

It is rewritten each tick: acquisition emits `SetTrackedTarget` or `ReleaseTrackedTarget` for every unit whose value
changes, including a release for a unit it otherwise skips (channelling, action-locked, hidden, incomplete) that
still holds one, so the field never outlives the tick it was chosen in. Boarding a transport and `commit_order`
clear it as they clear `weapon_target`.

**Why stored and hashed, not a scratch list.** The combat phase reads it while iterating entities mutably (one field
read, no lookup); a hashed field keeps every value the turret is driven by in the checksum, as `weapon_target` is.
The schema bump is needed anyway for D-1's byte in each projectile's `WeaponSpec`.

### D-3. How the tracked enemy is chosen (acquisition)

A new decision, `decide_tracked_target`, beside `decide_weapon_target`, run for every unit acquisition does not
skip:

1. **Carrier.** If no weapon the unit now carries (`ground_weapon`, `air_weapon`, `weapon`, `active_siege_weapon`;
   the same list as `decide_weapon_target`) has `turret_tracks_any_enemy`, release.
2. **The real weapon owns the turret.** Release when this tick's acquisition leaves the unit's own weapon a workable
   target: the front order is `AttackTarget` whose target passes `passes_weapon_filters` and `reaches_from_here`; or
   the unit holds or tracks (`Engagement::Hold`, `Engagement::Track`) and its `current_target` does; or this tick's
   order decision engages a target (`Acquisition::Engage`). The combat phase re-checks ownership exactly (D-4); this
   step only saves the scan.
3. **Scan.** Otherwise `scan_candidates` with `ScanScope { stationary: true, tracking: None, own_target: None,
   tracked: true }` — a new licence: the candidate's plane must resolve (`weapon_against`, with the active siege
   weapon) to a weapon with `turret_tracks_any_enemy`. The scan's existing gates apply (enemy, targetable, seen by
   the unit's player, in the cell grid, `target_flags`, `min_range`, `reaches_from_here`).
4. **Rank** by `TargetPriorityKey` with the archetype's tie-break (`Closest` for the tank: "the nearest enemy",
   S11), with `previous` read as `previous_target_position` as acquisition does. **No stickiness:** the best
   candidate is taken every tick, so the turret turns to the nearest enemy as the nearest changes (rule 4; 5.0.13's
   "tracks previously attacked unit" was reverted, S12–S13). Ties end in the entity id (deterministic).
5. **Outcome.** The best candidate is `SetTrackedTarget` when it differs from the held one; no candidate releases.

Decisions are applied after the read-only scan in entity order, with the phase's scratch buffers, as every
acquisition outcome is. Tracking never emits `Engage`, `AbandonAttack`, `ReleaseHoldTarget`, `SetWeaponTarget` or
`Sight`, and never writes `current_target`, `previous_target_position` or `movement`.

**Alternative considered: keep the tracked enemy while it stays valid, unless outranked** (invariant 3 of
`target_acquisition.rs`). Steadier turret, but it is the 5.0.13 behaviour Blizzard reverted (S13). Q0116 part 1 asks;
the choice is one branch in step 4.

### D-4. When the turret follows the tracked enemy (combat)

In `advance_weapon_cycles`, after the real weapon's target is settled, the combat phase resolves a **tracking aim**:
`tracked_target`'s snapshot, if it is live and available to the unit (`swing_target_live`'s test: the same as a
swing's) and its plane resolves to a weapon with `turret_tracks_any_enemy` and `has_turret`. The weapon step then
applies one rule:

1. **The real weapon owns the turret** on a tick it has a target (`target_opt` is `Some`) and either is in
   `Windup`, `Backswing` or `Cooldown` (it slews, faces between shots, or is locked, as today), or its target lies in
   its engagement band (the ready or rotating step turns it).
2. **The turret is free** otherwise: no target, or a `Ready` or `RotatingToTarget` weapon whose target is outside
   its engagement band (the ready step returns before any slew, §3 item 3).
3. **A free turret with a tracking aim** turns towards the aim by at most the tracking weapon's `turret_turn_rate`
   (`slew_towards`), and the idle (`idle_turret`) does not run that tick: the two are exclusive, so the turret never
   turns by more than its rate in a tick (fire-rules invariant 7).
4. **A locked turret stays locked:** in `Windup` or `Backswing` of a weapon with `locks_turret_while_firing`, the
   turret does not turn for tracking either (D-2 of the fire rules). No shipped weapon has both flags.
5. **Nothing else changes:** no phase change, no `engaged_target`, no cooldown tick of its own, no swing, no shot, no
   `charge_random_delay`, no `AttackSwing` or effect.

`step_entity_weapon`'s public signature stays; the implementation adds a sibling taking the tracking aim (for
example `step_entity_weapon_tracking(entity, weapon, target_opt, mandate, tracked: Option<SimVec2>)`) and makes
`step_entity_weapon` call it with `None`, so the many direct callers in tests are untouched.

**Why "free" includes a target out of band.** Rule 2 of §2.3: under an attack order whose target the cannon cannot
work, the fake (`OnlyFireAtAttackOrderTarget 0`) works against another enemy. Without this clause the turret would
hold while the tank chases past enemies.

### D-5. Interactions, rule by rule

| With | Rule |
|---|---|
| The real weapon's target and the one-weapon-per-plane choice | Unchanged. Tracking is not a weapon: `weapon_against`, `resolved_weapon`, `scan_extent`, `engagement_of` and the render snapshot's weapon see the host weapon only. The real target always owns the turret (D-4 rule 1) |
| `fires_outside_attack_order` (turret wind-up D-2) | Independent. A weapon with that flag tracks `current_target` under `Engagement::Track` and owns the turret as a real target; tracking only fills ticks it leaves the turret free. A walking `Busy` unit's weapon is B-218's (out) |
| `fires_at_other_targets` / `weapon_target` (fire rules D-4) | Independent. `weapon_target` is a real target and owns the turret; tracking never sets it and is released while the order's target is workable (D-3 step 2) |
| D-3 of the fire rules (turrets between shots) | Unchanged: in `Backswing` or `Cooldown` with a target, the real weapon owns the turret. With no target between shots (target died), the turret is free and tracks; the cooldown runs as today |
| The wind-up hold (`wind_up_holds`, turret wind-up D-6) | Unchanged: tracking never enters `Windup`, so nothing holds the unit for it (§2.4: the fake's own 0.167 s is not modelled) |
| Acquisition, leash, pursuit | Tracking never changes an order, `current_target`, the chase goal, `MovementMemory` (`last_seen`, `abandoned_target`) or the scan of the order decision. A tracked enemy that walks out of reach is dropped the next tick; nothing chases it |
| Mode channels and sieged stance | No weapon steps during a channel (§3 item 6) and acquisition releases the target (D-2), which is 5.0.14's disable (S6). The sieged weapon has no flag. The turret's angle at the start of the channel is kept, as today |
| Vision, cloak, hidden units | The scan's gates: only seen, targetable enemies; a hidden (carried, housed) or cloaked-undetected unit is never tracked; the combat phase re-checks availability (D-4) |
| Damage-side readers | No hit, so no `record_attack_presentation`, snapshot, reveal, retaliation, call for help, flight or regeneration delay |
| RNG | No draw: nothing tracking does reaches `charge_random_delay` or any generator |
| Presentation | `aim_angle_rad` (`world/snapshot.rs`) carries the tracked yaw; no new field. The client draws the turret from it (**verify** in WP2 with one constructed render-snapshot check that no swing is published while tracking) |

### D-6. Data

`assets/catalog/units.ron`, `railgun_crawler`'s `weapon`: `turret_tracks_any_enemy: true`, with a comment citing S1,
S4 and S6 (`balancemulti` WeaponData:7–20, UnitData:1473; the sieged unit carries no fake). The siege override omits
it. No other row sets it (§2.2).

### D-7. Validation

`WeaponSpecDef` validation refuses `turret_tracks_any_enemy` without `has_turret` (a tracking hull would turn the
hull, which belongs to movement): `SimCoreError::CatalogInvalidValue` naming the field; a unit test. Combinations with
the other flags are coherent and not refused.

### D-8. Hashed state and checksum

`turret_tracks_any_enemy` is hashed with `WeaponSpec` in its struct position (one byte per projectile);
`Entity::tracked_target` after `weapon_target`. The schema is bumped once, in WP1, to the next free number at merge
(49 provisional), proved latent in WP1 on scratch and re-recorded **once on the final tree** in WP3 (§5).

### D-9. Records

- `DESIGN.md` §4: B-198 pointed here in WP0, marked fixed in WP3; **B-218** registered in WP0 (provisional, next
  free at merge): "A weapon that may work outside attack orders but must stand to fire (the Immortal's
  `PhaseDisruptors`, `Slowing`; the Colossus' `ThermalLances`, `Stopped`) does not aim its turret while its unit
  walks under a non-attack order; the LotV beta note of 16 July 2015 says Immortals track the nearest enemy while
  moving (**verify**, Q0116 part 2)", deferred to plan `2026-11-26-turret-tracking-weapon` §14 F-1.
- The fire-rules plan's §14 F-2 and §2.5 row for B-198 point here (WP3).

---

## 5. Determinism and checksum

- **Integers only.** No float (C7); the scan is the existing deterministic scan with its existing key, which ends in
  the entity id.
- **Order.** Decisions applied in entity order after the read-only scan; the combat phase iterates `entities` in id
  order, as today. No new collection; no `HashMap`.
- **WP1's proof** (the scripts of `~/scratch/wind-up-hold/scripts/proof.sh` and `goldens.sh`, copied to
  `~/scratch/turret-tracking/scripts/`): with the two fields left out of the encoding and the schema at main's
  number, the tree reproduces every recorded checksum of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug
  and release and on `wasm32-wasip1` debug and release. The fields are then encoded at schema `N`. WP1 and WP2 do
  not re-record: their golden tests fail by design and the hand-backs say so.
- **One re-record, WP3, on the final tree.** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM` (`crates/sim_fixtures/tests/fixture_tests.rs`), proved on the four builds, under the
  header "Re-recorded for plan 2026-11-26-turret-tracking-weapon (checksum schema 48 -> N): each weapon encodes
  whether its turret tracks any enemy and each entity the enemy its turret tracks; with the fields left out at schema
  48 every checkpoint reproduced. Behaviour: <first differing checkpoint, entity, class Y or P, per §6.3>". If
  `main` takes `N` first, the merge repeats the re-record with the next free number.
- **Throughput.** `cargo bench -p sim_core --bench throughput` on WP0's tree and on the final tree, release, as the
  hold-facing plan did: every scenario at **50 ticks per second or more**. **Instruction budget: +0.5 % per
  scenario** over WP0's tree (`perf stat -e instructions:u`, the guide's method in `DESIGN.md` §5). The cost is one
  stationary scan per tick per unit that carries the flag and has no workable target, and one snapshot read and one
  slew per such unit in the combat phase; every other unit pays one boolean test per weapon. A scenario above the
  budget, or below 50 ticks/s, is stop rule 7.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure from a seed. Constructed tests find weapons by property (`turret_tracks_any_enemy`,
`has_turret`, `firing_arc`, `acquire_arc`, `locks_turret_while_firing`, `is_immobile`), never by slug, except the
shipped-values table (T-10), which names analogues as `b180_b179_fire_rules_follow_starcraft` does. Every test is
written first and shown red (the field does not exist; then, on WP1's latent tree, the turret idles).

### 6.1 Constructed tests (WP2 unless marked), in `crates/sim_core/tests/turret_tracking_tests.rs`

- **T-1 (tracks while moving).** A unit whose weapon has the flag walks under `Move` past an enemy that comes into
  and leaves its reach. On every tick the enemy is in reach, the turret's bearing error to it does not grow by more
  than the enemy's own angular motion seen from the unit that tick, and the turret turns by at most
  `turret_turn_rate`; it never turns towards the hull while the enemy is in reach. After the enemy leaves reach, the
  idle (`ResetIfMoving`) runs as today.
- **T-2 (nothing but the turret moves).** The world of T-1 against its twin with the flag cleared in a catalog copy
  (same binary): on every tick, every entity's position, velocity, heading, order queue, `current_target`,
  `weapon_target`, `previous_target_position`, `movement` memory, weapon phase, cooldown, `weapon_delay_carry` and
  `engaged_target` are equal; the RNG is in the same state (compare a draw from a clone of each world's generator
  after the run); only `turret_angle` and `tracked_target` differ.
- **T-3 (no harm).** In T-1's world the enemy's life and shields never change, no `PreparedShot`, projectile,
  `AttackSwing`, attack snapshot, attack reveal or effect is recorded, an enemy worker in reach does not flee, and
  idle enemy allies are not alerted.
- **T-4 (the chase).** A unit with the flag under an explicit attack on a target out of reach, with a second enemy in
  reach: the turret turns to the second enemy; the order, `current_target` and the pursuit goal stay on the order's
  target; on the tick the order's target enters the engagement band, the real weapon turns the turret to it, and
  its first swing starts on the same tick as in the flag-off twin whenever the real weapon's `acquire_arc` is 180°.
- **T-5 (the real weapon first).** Under an attack order (or a hold) with a workable target and a nearer enemy also
  in reach, the turret follows the order's target on every tick, through wind-up, backswing and cooldown;
  `tracked_target` is `None`.
- **T-6 (between shots, target gone).** A unit whose target dies in its cooldown, another enemy in reach, no order
  left that engages it on that tick: the turret turns to the other enemy; the cooldown runs out on the same tick as
  in the flag-off twin.
- **T-7 (lock).** A scratch turreted weapon with both `locks_turret_while_firing` and the flag, its target dying in
  the wind-up's last tick: through the rest of the swing's lock span (the backswing) the turret does not turn,
  although another enemy is tracked.
- **T-8 (modes).** During a mode channel the turret does not turn and `tracked_target` is `None`; in the sieged
  stance (a mode override without the flag) it is never set.
- **T-9 (vision and availability).** An enemy in reach that the unit's player does not see, a cloaked undetected
  enemy, a housed or carried unit and a dead unit are never tracked; an enemy that leaves sight is released the next
  tick.
- **T-10 (shipped values; WP1, `weapon_catalog_rules_tests.rs`).** Exactly one shipped weapon has the flag: the
  `railgun_crawler` tank-mode weapon (S1, S4); its siege override and every other weapon, structure weapon and mode
  weapon omit it. Each row cites its source.
- **T-11 (zero recompilation; `runtime_catalog_tests.rs`).** In a catalog copy, clearing the tank weapon's flag
  makes T-1's turret idle in the same binary; setting it on a scratch copy of another turreted weapon makes that one
  track.
- **T-12 (validation; WP1).** The flag without `has_turret` is refused, naming the field; omitted reads `false`.
- **T-13 (encoding; WP1, `checksum_state_tests.rs`).** Each of the two fields changes the checksum.
- **T-14 (ranking).** Of two enemies in reach with equal rank, the nearer is tracked; when the other becomes nearer,
  the turret turns to it on that tick (no stickiness, D-3 step 4); a higher `TargetPriorityKey` rank wins over a
  nearer one.
- **T-15 (pre-aim, class P).** A scratch copy of the weapon with a narrow `acquire_arc` and the flag: a unit tracks
  enemy A while it walks, then is ordered to attack A. Its first swing starts no later than in the flag-off twin.
  With the turret angles of both twins set equal on the tick the attack order is given, every later tick is equal
  in both but `tracked_target`: the only channel from tracking to the real weapon is the turret angle.
- **T-16 (presentation; `attack_swing_snapshot_tests.rs`).** While a unit only tracks, its render snapshot publishes
  the turret's yaw in `aim_angle_rad` and no swing.

### 6.2 Latent package (WP1)

- **L-1.** With the data in place and no rule reading it, every measured world (§7) is byte-identical to WP0's and
  the goldens differ by the schema bytes only (§5's proof).

### 6.3 World check (WP2, against WP1's tree; pre-registered)

Every world that moves is explained by its **first difference**, found with `firstdiff.py` and a per-entity trace,
and classed:

- **Y (yaw only).** The first difference is the `turret_angle` of a unit carrying the flag, on a tick probe (t)
  counted (a free turret with an enemy in reach). A world whose every difference stays in turret angles and
  `tracked_target` of such units is class Y. Byte-level only; matrix outputs do not record turret angles
  (**verify** in WP0 from `sim_fixtures::trace`), so a class-Y world moves only the goldens.
- **P (pre-aim).** A later real-weapon event differs (a swing start, a rotation tick count, a shot) on a unit whose
  turret tracking moved before, and the trace shows that weapon's aim gate read the moved turret angle: an
  `acquire_arc` or `firing_arc` below 180°, or a `LeastTurning` tie-break. In shipped data the only such weapon a
  tracking unit can reach is the sieged weapon (`acquire_arc 0`, `firing_arc 11.25°`), when a tank that tracked
  sieges; the tank-mode weapon fires in every direction (arc 360°) and its tie-break is `Closest`. Each class-P world
  is traced to the siege (or other) event and recorded with the change of its first sieged swing (earlier or later:
  the turret may have been pointed at another enemy).
- **Anything else** (an order, a position, a target, an RNG draw, a hold differing first) is a defect: stop rule 2.

Probes (§7.1) count, per world: (t) tracking opportunities; (p) pre-aim exposures (a tracking tick followed, before
the turret is next reset, by a mode channel start or a real swing with an arc below 180°).

### 6.4 Close battles (fixed in WP0, before any behaviour change)

WP0 computes and commits to the evidence file, from WP0's baseline only, the **close-battle list**, by the rule of the
hold-facing plan's §6.4:

1. every matrix battle in which, over its 20 baseline runs (seeds of record and the 16 development seeds), the side
   that wins fewer runs wins at least 2 (10 %);
2. battles 31 and 33 if the same share holds over their 100 and 64 baseline seeds;
3. every battle a closed owner question or a fixture comment names close or restated (`grep -n "close\|restated"
   crates/sim_core/tests/fixtures/matchup_matrix.ron`);
4. no other. A `known_bug` battle (28, B-192) is not close; a change of its state is stop rule 5.

The list is not edited after WP1 starts.

### 6.5 Outcome moves

"Moves" means its **outcome** moves: on some measured seed the winner changes or the expectation fails where it held.

- A **close** battle whose outcome moves (only possible through class P) is restated under charter §8 with evidence:
  the first differing tick of one flipped seed, traced to the entity and the class-P event, and, if the claim's
  range breaks, the expectation re-derived over seeds of record and 1 to 20 with a fixture comment citing this plan.
- **Any other** battle whose outcome moves is a **stop for review** (stop rule 4), with the trace.
- Held-out seeds (§7.4) only confirm; nothing is restated or tuned from them.

### 6.6 Predictions (recorded before measuring, not thresholds)

- **Matrix.** The tank appears in 3 (tank mode, attack-moving against one Immortal), 37a, 37b, 39a, 39b, 40 and 42
  (sieged, or ordered to; WP0's census settles which start sieged). Battle 3 has no second enemy to track while the
  tank's target is out of reach; the sieged battles carry no flag. **Predicted: every matrix battle byte-identical**,
  unless the census finds a tank that tracks before it sieges (class P).
- **Battles 31, 33**: no tank, byte-identical.
- **Showcase and showcase effects**: tanks under attack-move (census in WP0); class Y where an acquired chase passes
  another enemy, class P only if a tracking tank later sieges by script. No outcome move predicted.
- **`golden_replay`**: "red_siege" (three tanks) attack-moves from tick 120 and stops at 760; class Y wherever a
  tank's acquired target is out of reach with another enemy in reach, or none is engaged on a tick one is in reach
  (after Stop, acquisition engages on the same tick, so the turret stays the real weapon's). Probe (t) on the golden
  decides; if it counts zero, the golden moves by the schema bytes only.
- **`golden_replay_orders`, the sandbox**: no tank (census), schema bytes only.
- **Client skirmish stage log**: identical (the pilot trains `pulse_bot`).

---

## 7. Measurement

### 7.1 Probes (WP0, scratch, not committed)

A scratch patch on WP0's tree (`~/scratch/turret-tracking/probe/`, with its diff; it may name slugs, being scratch)
logs per world and seed:

- **(t) Tracking opportunities:** each tick a tank-mode tank (found by the archetype's tank-mode weapon) has a free
  turret by D-4's rule and an enemy its weapon reaches from where it stands, tagged with its front order.
- **(p) Pre-aim exposures:** each (t) episode followed, before the turret next returns to the hull, by a mode channel
  start (siege) or a real swing whose weapon's arc is below 180°.
- **(c) Census:** which worlds have tanks, in which stance at tick 0, which siege by script or order; the sandbox's
  and both goldens' squads; whether `sim_fixtures::trace` or `matrix.csv` records any turret angle.

### 7.2 Commands

The hold-facing plan's scripts, copied to `~/scratch/turret-tracking/scripts/` (`measure.sh`, `compare.sh`,
`firstdiff.py`, `proof.sh`, `client.sh`, `heldout.sh`, `bench.sh`) with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/turret-tracking
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 development seeds, battle 31 on 100 seeds, battle 33 on
64, and the showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. One at a
time across worktrees, in the background, never beside a `cargo gate` (charter §7).

### 7.3 Expected moves

| World | WP1 (latent) | WP2 (tracking) |
|---|---|---|
| Matrix | byte-identical | byte-identical (§6.6), else class P only, traced |
| Battles 31, 33 | byte-identical | byte-identical |
| Showcase, default and 1–8 | byte-identical | class Y (no measured output) or P; census decides |
| Showcase effects, default and 1–8 | byte-identical | as the showcase |
| Goldens, sandbox | schema bytes only, proved (not re-recorded) | `golden_replay` class Y if probe (t) counts there; re-recorded once in WP3 |
| Client skirmish stage log | identical | identical |

### 7.4 Held-out seeds (confirmation only)

Run **once**, in WP3, on WP0's tree and on the final tree, after every development-seed result is settled: matrix
seeds 7001 to 7016, battles 31 and 33 on seeds 7001 to 7064, the showcase and showcase effects on seeds 9 to 16. They
must show only class Y and P differences and no outcome move outside §6.4's list. A contradiction stops the plan
(stop rule 6); nothing is tuned or restated from them.

---

## 8. Stop rules

1. **WP0 is records only;** the close-battle list and the probes' counts are committed (the list) or stored (the
   probes) before WP1's first build.
2. **One cause.** Every world WP2 moves shows a first difference of class Y or P (§6.3). Any other first difference
   stops the package until traced; a second cause is registered (from B-219) and the package lands only if it is
   separable.
3. **WP1 moves nothing** but the schema bytes (L-1).
4. **A non-close battle's outcome moves:** stop for review with the trace; not restated by the implementer.
5. **A `known_bug` battle's state changes** (28): stop for review.
6. **The held-out run contradicts the development seeds:** stop; report both.
7. **Throughput below 50 ticks/s on any scenario, or instructions above +0.5 % on any scenario** (§5): stop and
   profile.
8. **A tracking unit's order, path, target or weapon timing differs** from its flag-off twin in a constructed test
   other than T-15's pre-aim: stop (rule 1 of §2.3 broken).
9. **No tuning constant.** Every value is the data's or an existing engine rule's.
10. **Overlap.** Nothing under `crates/sim_core/src/movement/` changes, nor the contact solver or the parked
    enemy-contact work. A package that seems to need it stops and reports.

---

## 9. Client consequences

- The tank-mode turret visibly follows enemies in reach while the tank moves or chases, through `aim_angle_rad`; no
  client code changes. T-16 checks that tracking publishes no swing, so no attack animation, muzzle flash or sound
  plays (the fake's own animation in 5.0.14 is **verify**, S14).
- No `PlayerView`, `RenderSnapshot` field, shader or capture change.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | B-198 pointed here, then fixed; B-218 registered |
| `docs/plans/README.md` | plan, WP3 | Row |
| `docs/evidence/2026-11-26-turret-tracking-weapon.md` (new) | WP0–WP3 | §2.2 survey commands, census, baseline, close-battle list, probes, comparisons, traces, held-out, throughput |
| `crates/sim_types/src/weapon.rs` | WP1 | `turret_tracks_any_enemy`, docs |
| `crates/sim_core/src/catalog.rs`, `crates/sim_core/src/catalog/validation.rs` | WP1 | Def field; D-7 |
| `crates/sim_core/src/entity.rs` | WP1 | `tracked_target`; cleared by `commit_order` and transport loading (`crates/sim_core/src/transport/mod.rs`) |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-8, schema `N` |
| `assets/catalog/units.ron` | WP1 | D-6 |
| Test files with `WeaponSpec { … }` literals (`grep -rln locks_turret_while_firing crates/*/tests crates/*/src`) | WP1 | The field at its default (mechanical) |
| `crates/sim_core/src/target_acquisition.rs` | WP2 | D-3 (`decide_tracked_target`, `ScanScope::tracked`, the two `Acquisition` variants, the release for skipped units), module invariant 2 extended |
| `crates/sim_core/src/combat.rs` | WP2 | D-4 (the free-turret rule, the tracking sibling of `step_entity_weapon`), module invariant 8 |
| `crates/sim_core/src/combat_resolution.rs` | WP2 | The tracking aim in `advance_weapon_cycles` |
| `crates/sim_core/tests/turret_tracking_tests.rs` | WP2 | T-1 to T-9, T-14, T-15 |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs`, `checksum_state_tests.rs` | WP1 | T-10, T-12, T-13 |
| `crates/sim_core/tests/runtime_catalog_tests.rs`, `attack_swing_snapshot_tests.rs` | WP2 | T-11, T-16 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | WP3 | Re-recorded once (§5) |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP3, only under §6.5 | A close battle restated with its trace |
| `docs/plans/2026-11-24-fire-rules.md` | WP3 | Pointer lines (§2.5 B-198 row, §14 F-2) |

`crates/gate/suites.ron` needs no change: every test is in an existing target (**verify** in WP1).

---

## 11. Work packages (commit order)

One branch from `main`, one worktree, `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` (check
`quota -s` and `df -h ~/scratch` first); one commit per package; failing tests first, with the red run pasted in the
hand-back; `cargo gate quick` and the tests the change needs; one `cargo gate package` by a verifier on the final
tree (Q0061, charter §4.3). No `unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust
(C5). Check `docs/questions/open/` before the last commit.

### WP0: records, baseline, census, close battles, probes (S, docs and scratch)

- `DESIGN.md`: B-198's row points here; B-218 registered (D-9). Evidence file with §2.2's survey and its commands.
- Baseline measurement of §7.2 on WP0's tree; census and probes (t), (p), (c); the close-battle list (§6.4)
  committed with the command that derived it; throughput baseline (§5). The evidence states which worlds WP2 may move
  and in which class.
- File Q0116 from §15 if the orchestrator chooses to (default followed either way).
- **Gate:** docs only.

### WP1: the fields and their data, latent (S, `sim_types`, `sim_core`, data, schema)

- Red first: T-10, T-12, T-13; `entity.rs` unit test that `commit_order` clears `tracked_target` for every
  `UnitOrder` variant.
- D-1, D-2 (the field, cleared by `commit_order` and loading; nothing writes it yet), D-6, D-7, D-8; the docs and the
  mechanical literals; §5's proof (latent). **Acceptance:** L-1, stop rule 3. Goldens not re-recorded. Commit
  `feat(sim_core): weapons state whether their turret tracks any enemy, latent (turret-tracking WP1, schema N)`.

### WP2: the turret tracks (M, `sim_core`, B-198)

- Red first (on WP1's tree): T-1 to T-9, T-11, T-14 to T-16.
- D-3, D-4, D-5's checks. Measure against WP1's tree; class every moved world (§6.3); §6.5 for outcomes.
  **Acceptance:** stop rules 2, 4, 5, 8. Commit `fix(sim_core): a turret whose weapon tracks any enemy aims at the
  best enemy in reach under any order without firing, 5.0.14 90mmCannonsFake (turret-tracking WP2, B-198)`, stating
  which worlds moved and in which class.

### WP3: confirmation and records (S)

- Held-out run (§7.4); throughput and instructions (§5); goldens and sandbox re-recorded once on the final tree,
  proved on four builds; `DESIGN.md` B-198 fixed; pointers in the fire-rules plan; README row; `## As built`. Merge
  `main` first; take the schema number free at that time.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | Default; D-7's refusal | WP1: `catalog/validation.rs` unit tests, `weapon_catalog_rules_tests.rs` (T-12) |
| State | `tracked_target` cleared by every order | WP1: `entity.rs` unit tests |
| Data | §2.2 against 5.0.14 | WP1: `weapon_catalog_rules_tests.rs` (T-10) |
| Encoding | Each field changes the checksum | WP1: `checksum_state_tests.rs` (T-13) |
| Acquisition | D-3: carrier, ownership, licence, ranking, release of skipped units | WP2: `target_acquisition.rs` unit tests; T-9, T-14 |
| Turrets (charter subject) | T-1 to T-8, T-15 | WP2: `turret_tracking_tests.rs` |
| Zero recompilation | T-11 | WP2: `runtime_catalog_tests.rs` |
| Presentation | T-16 | WP2: `attack_swing_snapshot_tests.rs` |
| Worlds | §6.3 to §6.6, §7 | WP2, WP3 (scripts); goldens and sandbox in the package gate (release and `wasm32-wasip1`) |
| Guards that pass unchanged | `fire_rules_tests.rs`, `weapon_cadence_tests.rs`, `hull_turning_tests.rs`, `stutter_step_tests.rs`, `target_selection_tests.rs`, `weapon_validation_tests.rs`, `attack_move_simulation_tests.rs` | package gate |

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | 5.0.14's fake keeps a target while valid instead of re-ranking (D-3 step 4) | The turret switches between equal enemies more often than in the game | Q0116 part 1; one branch |
| R-2 | The engine lets the fake take the turret while the real cannon is between shots without a target, or never while it has one | Small differences of yaw between shots | D-4 rules 1–2 follow the order's-target-first answer of Q0109; **verify** in game (Q0116 part 3) |
| R-3 | Class P in a shipped world: a tank tracks while moving, then sieges, and its first sieged shot comes earlier or later | A matrix or showcase outcome could move | §6.3 classes and traces it; §6.5 decides; the census predicts none in the matrix |
| R-4 | The schema number is taken first | Merge conflict in `fields!` and the goldens | "Next free at merge" (§5) |
| R-5 | Cost: the extra stationary scan | Instructions | Only flagged units without a workable target scan; budget +0.5 % (§5) |
| R-6 | The free-turret test in combat and the ownership test in acquisition disagree (a target crossing the band edge between the phases) | `tracked_target` set but unused, or released one tick early | The combat phase decides (D-4); acquisition only saves the scan; T-4 covers the edge tick |
| R-7 | B-218 (Immortal, Colossus) is the bigger observable | Players see turrets that do not aim while walking | Registered, Q0116 part 2; F-1 says how D-3 extends |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: B-218, aim without fire for weapons gated by movement.** Extend D-3's carrier test from
  `turret_tracks_any_enemy` to "a turreted weapon with `fires_outside_attack_order` whose fire the movement gate
  blocks" (`fires_without_attack_order(weapon, walking)` false only because of `walking`), and let D-4 aim it; no new
  field. It moves Immortal and Colossus battles through class P (narrow arcs), so it needs its own probe, close-battle
  list and review; it also bears on `fires_at_other_targets` chases (fire-rules P-5). A follow-up plan after Q0116.
- **F-2: a tracking range of its own** (`Option<TurretTrackingSpec { max_range }>`), added when a weapon's tracking
  reach differs from its own.
- **The fake's strike** (damage point hold, animation, call for help): not modelled (§2.4).
- **The Cyclone's lock-on dummy, the Battlecruiser selector and the display dummies:** no analogue or not
  tracking-only in 5.0.14 (§2.2).
- **`movement/`, the contact solver, collision classes and the parked enemy-contact work:** untouched.

---

## 15. Owner question (filed as [Q0116](../questions/open/0116-siege-tank-turret-tracking.md))

```markdown
# Q0116: How a Siege Tank's turret picks what to follow while it moves, and whether Immortals and Colossi do the same

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** the unsieged tank's turret follows the nearest enemy in range whenever its cannon has
  nothing to fire at (moving, or chasing a target out of range), switching as soon as another enemy is nearer; it
  never fires, slows the tank or alerts anyone. Immortal and Colossus turrets are unchanged for now (registered as
  B-218) (plan `2026-11-26-turret-tracking-weapon` D-3, §14 F-1).

## Question

StarCraft II 5.0.14 gives the unsieged Siege Tank a hidden second weapon that does no damage; its only effect is that
the turret turns towards enemies while the tank moves. Blizzard's 2015 note says Siege Tanks and Immortals "track the
nearest enemy while moving"; patch 5.0.13 made the tank and Immortal turrets keep tracking the unit they last attacked,
and 5.0.14 reverted that "due to causing unintentional retargeting".

1. **Which enemy does the turret follow?** (a) Always the nearest in range, switching when another becomes nearer
   (the default); (b) the first one it picked, until it leaves range or dies.
2. **Immortals and Colossi.** Their own weapons may fire outside attack orders but only while standing. In the game
   their turrets probably also aim at enemies while they walk. (a) Leave them as they are for now (the default; a
   later plan); (b) do them next, accepting that battles with Immortals or Colossi may change because their turrets
   are already aimed when they stop.
3. **If you can, check in a custom game:** a Siege Tank ordered to move past two Marines walking in opposite
   directions — does the turret switch between them, and does the tank ever pause, flash or alert them?

## Answer

```

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`). Commands: `grep -rn "SiegeTankDummyWeapon\|90mmCannonsFake\|90mmCannonsDummy\|SiegeTankMorphDisableDummy\|SiegeTankUnmorph" --include=*.xml .`;
  `grep -n 'CWeapon[A-Za-z]* id="[^"]*\(Fake\|Dummy\)' */WeaponData.xml` and, per id, `grep -n 'Link="<id>"\|value="<id>"' */UnitData.xml */BehaviorData.xml */AbilData.xml */EffectData.xml`;
  `sed -n 1,45p core/WeaponData.xml`; `grep -n 'id="DU_WEAP"' -A10 core/EffectData.xml`.
- Patch notes (fetched 2026-11-25): 5.0.13 and 5.0.14 notes as cited in S12–S13; the LotV beta balance update of
  16 July 2015 found by search only (S11, **verify** wording).
- Code read on `91350af1`: `combat.rs` (`step_entity_weapon`, `idle_turret`, `slew_towards`,
  `face_target_between_shots`, `step_backswing`, `step_ready_weapon`, `Mandate`, `fires_without_attack_order`),
  `combat_resolution.rs` (`advance_weapon_cycles`, `attack_order_target`, `can_work`, `ordered_target`,
  `steps_weapon`, `resolved_weapon`), `target_acquisition.rs` (`acquire_targets`, `engagement_of`, `licensed`,
  `reaches_from_here`, `decide_acquisition`, `decide_weapon_target`, `scan_candidates`), `catalog.rs`
  (`weapon_against`), `entity.rs` (`weapon_target`, `active_siege_weapon`, `hull_aimed_by_weapon`),
  `sim_types/src/weapon.rs` (`WeaponSpec`), `units.ron` (`railgun_crawler`), the golden fixtures' squads and orders,
  and the matrix battles with `railgun_crawler`.
- No prototype: WP1 is latent by construction, WP2's reach is predicted by probes (t) and (p) and classed by §6.3.

---

## As built

Built on branch `turret-tracking-plan` from `main` `91350af1` (no `main` commit landed while it was built), by the
plan as written; Q0116's defaults (part 1 (a), part 2 (a)). Evidence:
[`docs/evidence/2026-11-26-turret-tracking-weapon.md`](../evidence/2026-11-26-turret-tracking-weapon.md).

| WP | Commit | What |
|---|---|---|
| WP0 | `7700aa31` | B-198 pointed here, B-218 registered (provisional); survey, baseline, census; probes (t) and (p) count zero in every measured world and both goldens; close-battle list 14, 26, 31, 33, 35, 38a, 38b |
| WP1 | `48023594` | `WeaponSpec::turret_tracks_any_enemy` (tank-mode weapon only), `Entity::tracked_target`, D-7, schema 49 (provisional); L-1 held, latent proof on four builds |
| WP2 | `9a8e2ce2` | D-3 (`decide_tracked_target`, `has_workable_target`, `ScanScope::tracked`, release of skipped units), D-4 (`step_entity_weapon_tracking`, `TrackingAim`, `tracking_aim`); no measured world moved (no class Y or P) |
| WP3 | this commit | Held-out run identical; goldens and `DEFAULT_SANDBOX_CHECKSUM` re-recorded once at schema 49, both proofs on four builds; B-198 fixed; fire-rules pointers; throughput: stop rule 7 |

**Deviations and decisions.**
- T-5 and T-8 are guards (what tracking must not do) and pass on WP1's tree; the other ten new behaviour tests were
  red there.
- The instruction budget is read on the whole benchmark run (no per-scenario selector exists), as the hold-facing
  plan read it.
- The acquisition unit tests cover the carrier and the release of a skipped unit; the licence is covered by T-9 (an
  air transport in reach is never tracked) and ownership by T-5.
- **Stop rule 7 fired:** +2.30 % instructions (budget +0.5 %), every scenario at 80 ticks/s or more. The cost is the
  eight bytes `Entity::tracked_target` adds (code generation of the scan loop), not the tracking rule; the
  evidence holds the bisection. Left for review.
- Schema 49 and B-218 stay provisional, next free at merge.


## Amendment 1 (2026-11-26, orchestrator review of the stop-rule-7 stop)

WP3 stopped on stop rule 7: whole-benchmark instructions +2.30 % against the +0.5 % budget, with every world byte-identical
and every other check passed. The profile in the evidence file traces the whole increase to `scan_candidates`, with
the same number of scans: WP1 alone (fields present, no rule reading them) is +2.26 %, and WP1 without
`Entity::tracked_target` is +0.01 %. The 8-byte field changes the scan loop's code generation, not its work.

**Decision: D-2 changes.** The tracked enemy is not stored on `Entity`. It lives in one world-level side table in
`SimWorld` (a sorted, deterministic structure keyed by entity id, e.g. a `Vec` of `(EntityId, EntityId)` kept sorted, or
an index-aligned column allocated only when the catalog flags a weapon), holding entries only for units whose weapon
carries `turret_tracks_any_enemy`. It is written by acquisition, read by the combat phase for the turret alone,
cleared on the same events as before (`commit_order`, boarding, death, the skipped-unit release), and hashed into the
checksum in id order (schema stays one bump, 49 provisional). Accepting the cost and tuning the scan loop's code
generation were rejected: the first pays 2.3 % on every unit for a turret-only presentation rule, the second is
fragile against compiler changes.

**Acceptance unchanged.** T-1 to T-16, the world classification (all byte-identical so far), the latent proof at 48,
the single re-record on the final tree, and stop rule 7 (≤ +0.5 % instructions per the plan's reading, ≥ 50 ticks/s)
all apply to the amended tree. If the side table still exceeds the budget, the package stops again for review.
