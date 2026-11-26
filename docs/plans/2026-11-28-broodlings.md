# Blueprint: Broodlings — the Brood Lord's Escort Magazine, Spawn on Impact and Timed Life (B-21, B-232)

This plan fixes two rows of `DESIGN.md` §4 and one more that its research found:

- **B-21:** "Brood Lord spawns no Broodlings" (plan [`2026-11-05-sc2-deviation-fixes`](2026-11-05-sc2-deviation-fixes.md)
  §3.3, WP18 "→ own plans").
- **B-232:** "The Brood Lord analogue's missiles fly a constant 0.9 per tick with no source; 5.0.14's
  `BroodlingEscortLaunchA` and its escort missiles fly `BroodLordWeaponRight` / `Left` … It reaches battles 19 and
  20" (registered by plan [`2026-11-28-missile-movers-b52`](2026-11-28-missile-movers-b52.md) §14 and handed here).
- **B-233** (new, provisional): the Brood Lord analogue fires two strikes every period. 5.0.14 throws its **escorts**,
  at most two orbiting Broodlings that a magazine rebuilds one at a time, one per 2.5 Normal s; after the first
  volley it throws one at a time.

The design in one paragraph: in 5.0.14 the Brood Lord's weapon does nothing itself (`BroodlingStrike`, `Effect ""`).
Its attack is a **magazine** (`BroodLordHangar`, a `CAbilArmMagazine` like the Carrier's) of two **escort**
Broodlings that orbit it, invulnerable and untargetable. On an attack each armed escort **becomes the missile**: it
flies the `BroodLordWeaponRight`/`Left` mover, deals 20 at impact if the target still passes the filters, and when its
flight finishes, hit or miss, a ground **Broodling** is created at the target with a 5 Normal s timed life and an
attack order on the target. Here, three data-driven capabilities express that, none of them naming a unit:
- a **strike magazine** on the archetype (`ArchetypeCapabilities::strike_magazine`), whose armed charges gate and
  count the strikes of each release, rebuilt one at a time;
- a **spawn on impact** on the weapon (`WeaponSpec::impact_spawn`), which creates units where a missile finishes,
  with their first order and a timed life;
- a **timed life** on the entity (`Entity::timed_life`), which kills a unit when it runs out, without a damage
  source.

The escorts themselves are not entities: they cannot be touched, so only their count and arming decide anything,
and the magazine carries exactly that. The thrown missile flies the 5.0.14 mover through the existing
`missile_movers`. Broodlings are ordinary ground units that walk and acquire through today's movement and acquisition,
which this plan does not edit, so it runs beside the Gated movement packages. The Broodling and the Brood Lord
analogue get real Veo media in a capped batch; until it exists, the client keeps the Brood Lord analogue locked, as it
is today. Checksum schema: the next free number at merge (**61** provisional).

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `156721c2` (checksum
schema 60).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-233** (B-233 to B-236 here). The draft
  owner question is **Q0132** (§15, a fenced draft, not filed by this commit). The checksum schema is the next free
  number at merge: `main` is at 60 (58 and 59 unused, reserved by the Gated plan and not taken), so this plan takes
  **61** if nothing lands first. The new unit's `unit_type_id` is the next free at WP3 (**48** provisional; the
  interceptor took 47). Every schema mention below reads "next free at merge".

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S20, S21 |
| Q0045 (align times to SC2), Q0052 S-5 (25 ticks/s, half up), Q0057 (ranges edge to edge) | Closed | §2.3 conversions |
| Q0020: visual questions are the agent's call; Q0001/Q0024/Q0047 and the owner's chat rules: Veo approved within the total budget ($230), audio on, loops without jumps, no rays or radiating stripes in backdrops | Standing rules | D-12, WP6 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | §6.6 |
| Gated coordination (`2026-11-27-gated-movement`): no other package edits `movement/`, `steering.rs`, `target_acquisition.rs`' admission test or `movement_rules.ron` while a Gated package is open | Binding on this plan | D-10, stop rule 6 |
| Q0130 (interceptors hover), open, default (a) | Unrelated (Carrier) | — |
| Q0131 (Carrier media: light shafts), open, **blocking for the Carrier's media only**, default (c) | Read; its lesson is applied; no negative-term change is planned while it is open | D-12, WP6 |
| **Draft, not filed:** Q0132 "The Brood Lord fires its two escorts, then one per rebuild: accept the data's reading (steady damage halves, Broodlings added)?" | Proposed (§15); default (a) followed | D-3, B-233 |

---

## 1. Summary

| | Today (`156721c2`) | After this plan |
|---|---|---|
| Brood Lord analogue (`spore_matriarch`) attack | 2 strikes of 20 in one missile every 45 ticks, constant 0.9 per tick | Up to 2 escorts per release, one strike and one missile-borne Broodling each; escorts rebuilt one at a time, 45 ticks each, armed 9 ticks later: 2 strikes on the first release, then 1 per 45 ticks |
| Its missiles | Constant 0.9: 2 to 13 ticks over edge 0.5 to 10 | `BroodLordWeaponRight`: from 0.28 accelerating 0.009408 to 0.448 for 3.0 of path, then accelerating 0.06272 to 2.24: **5 to 21 ticks** |
| Broodlings | None | One per strike, created where the missile finishes (the target, or the point it was lost at), hit or miss: 20 life, armour 0, light + biological, ground, speed 0.1654, `NeedleClaws` 4 per 14 ticks melee; timed life 89 + 0 to 5 ticks; first order: attack the Brood Lord's target, or attack-move to the point; then acquires as any unit |
| Mechanisms | — | `ArchetypeCapabilities::strike_magazine`, `WeaponSpec::impact_spawn`, `Entity::timed_life`, `Entity::strike_magazine`; all generic, none names a unit |
| Movement and acquisition code | — | Unedited (D-10) |
| Client | `spore_matriarch` locked (no media) | A unit whose weapon spawns is presentable only with its spawn's media; real Veo clips for both identities (WP6); until then locked, truthfully |
| Hashed state | — | The entity's magazine state and timed life; the weapon's impact spawn; schema next free at merge |
| Worlds | — | Predicted: battle 20 moves (outcome unchanged: the roach dies later), battle 19 behaviour-identical (the Brood Lord never fires at the air Viking); no other measured world carries the unit (WP0 measures it) |
| Records | B-21, B-232 open | B-21, B-232, B-233 fixed; B-234 (structure-death Broodlings), B-235 (escorts not drawn), B-236 (spawn placement among units) registered, deferred |
| Size | — | L: WP0 (records, census, probe, baseline), WP1 (mechanisms, latent), WP2 (movers, B-232), WP3 (magazine data, B-233), WP4 (Broodling and spawn data, B-21), WP5 (client), WP6 (media, paid), WP7 (confirmation, re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. Conversions as everywhere in the catalog (Q0045, Q0052 S-5): a Normal time `t` is `t × 25 / 1.4` ticks,
rounded half up; a Normal speed `v` is `v × 0.056` world units per tick; a Normal acceleration `a` is `a × 0.003136`
per tick squared; a Normal turning rate `r` is `r × 0.056` degrees per tick; a Normal regeneration `r` per second is
`r × 0.056` per tick.

### 2.1 Sources

`L Kind:N` is line N of layer L's file in SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at
`~/scratch/combat_wp3_data/<layer>/`; layers apply in the order core, liberty, libertymulti, swarm, swarmmulti, void,
voidmulti, balancemulti, a later layer overriding a field.

| # | Source | What it gives |
|---|---|---|
| S1 | Brood Lord, `liberty UnitData:11102–11165`: `WeaponArray BroodlingStrike` (:11159), `AbilArray BroodLordHangar`, `BroodLordQueue2` (:11135–11136), `Radius 1`; `libertymulti UnitData:132–142` and `swarm :2414–2424` (life 225, armour 1); `voidmulti :137–145` (`Speed 1.875`); `balancemulti :113–119` (glossary only) | The attack is the hangar's; our row's vitals and speed already match [DATA] |
| S2 | `BroodlingStrike` (`CWeaponLegacy`), `liberty WeaponData:411–420`: `DisplayEffect BroodlingEscortDamage`, `TargetFilters Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable`, `Range 9.5`, `Period 2.5`, `AllowedMovement Slowing`, **`Effect ""`**; `voidmulti WeaponData:74–77`, `balancemulti :74–77`: `Range 10`, `MinScanRange 10.5` | The Brood Lord's own weapon deals and launches nothing: period 45 ticks, range 10, ground only; damage point and backswing the `CWeapon` defaults (3 and 9 ticks, as our row) [DATA] |
| S3 | `BroodLordHangar` (`CAbilArmMagazine`), `liberty AbilData:3808–3824`: `Leash 12`, `Flags Retarget 1`, `EffectArray Death InterceptorFate`, `EffectArray Release KillsToCaster`, `InfoArray Ammo1 Time 2.5 CountStart 2 Manage Recall Unit BroodlingEscort`, `Flags AutoBuild 1`, `AutoBuildOn 1`, **`External 1`**, `ExternalAngle −149.9853`, `149.9853`; `voidmulti AbilData:84–86`, `balancemulti :84–86`: `Leash 9` | A magazine of `BroodlingEscort` members held **outside** the unit at ±150° (behind it), 2 at the start, one built per 2.5 Normal s (44.64 → **45 ticks**), auto-built, no cost [DATA] |
| S4 | `BroodLordQueue2` (`CAbilQueue`), `liberty AbilData:2077–2082`: `QueueSize 2`, `Hidden`, `Passive` | At most 2 builds queued; one built at a time [DATA; one at a time is INF from a `CAbilQueue` with no `QueueCount`, as the Carrier plan S5 read it, and S20 agrees] |
| S5 | `BroodlingEscort` unit, `liberty UnitData:11194–11214` (parent `BroodlingDefault`): air, `Collide FlyingEscorts`, `Mover Fly`, `Speed 6`, flags `Uncommandable`, `Unselectable`, `Untargetable`, **`Invulnerable`**, `BehaviorArray StandardMissile`, **`BroodlingAttackDelay`**, `WeaponArray BroodlingEscort`; no `CostResource` | An escort cannot be hit, selected or ordered; it carries the weapon that throws it [DATA] |
| S6 | `BroodlingAttackDelay`, `liberty BehaviorData:1361–1368`: `Duration 0.5`, `AbilLinkDisableArray attack` | A new escort cannot attack for 0.5 Normal s (8.93 → **9 ticks**) [DATA; that it runs from the escort's creation is INF from its place in `BehaviorArray`] |
| S7 | `BroodlingEscort` weapon, `liberty WeaponData:421–429`: `TargetFilters Ground,Visible;…`, `Range 9`, `Arc 360`, `Period 10`, `Options Hidden`; no `Effect`, so the default `##id##`, the effect `BroodlingEscort` | Each escort's attack runs the switch `BroodlingEscort` [DATA; the default effect name is INF, **verify**] |
| S8 | The switch, `liberty EffectData:2654–2663`: `BroodlingEscort` (`CaseArray Validator IsStructure` → `BroodlingEscortStructure`, default `BroodlingEscortUnitSet`); `BroodlingEscortUnitSet` = `BroodlingEscortRelease` (`CEffectReleaseMagazine`, :2738–2740) then `BroodlingEscortLaunchA` | Against a unit: the escort leaves the magazine and is launched [DATA] |
| S9 | `BroodlingEscortLaunchA` (`CEffectLaunchMissile`), `liberty EffectData:2664–2672`: `ImpactEffect BroodlingEscortDamageUnit`, `Movers BroodLordWeaponRight IfRangeLTE 500`, `Movers BroodLordWeaponLeft IfRangeLTE 500`, `Flags ValidateWeapon 0`, `MoverRollingJump 1`, **`FinishEffect BroodlingEscortImpactA`**; no `AmmoUnit` | The escort itself flies as the missile [INF: no ammo unit]; damage on impact, then the finish effect [DATA] |
| S10 | `BroodlingEscortDamageUnit` (:2678–2683) and `BroodlingEscortDamage` (:2784–2790): parent `DU_WEAP_MISSILE`, `ValidatorArray BroodlingEscortFilters`, `Visibility Visible`, `Amount 20`; `BroodlingEscortFilters`, `liberty ValidatorData:1723–1726`: `Ground,Visible;Missile,Stasis,Dead,Hidden,Invulnerable` | 20 per escort, applied only if the target passes the filters at impact (plan `2026-11-24-weapon-validation` S3, S7: the Brood Lord validates at impact) [DATA] |
| S11 | `BroodlingEscortImpactA` (`CEffectCreateUnit`), `liberty EffectData:2684–2692`: `WhichLocation TargetUnitOrPoint`, `SpawnUnit Broodling`, `SpawnEffect BroodlingEscortLaunchB`, `CreateFlags Birth 0`, `SetFacing 1`, **`SpawnRange 5`**; no `SpawnCount` (so 1) | One Broodling per escort, created at the target unit, or at the point if the unit is gone, searching up to 5 for a place [DATA; `SpawnCount` default 1 is INF] |
| S12 | `BroodlingEscortLaunchB` (:2693–2699): `BroodlingEscortMissileB`, `MakePrecursor`, `BroodlingTimedLife`, `BroodlingEscortLaunchBTransfer` (`KillsToCaster`, :2700–2704); `BroodlingEscortMissileB` (:2705–2710, `AmmoUnit BroodLordBWeapon`, mover `liberty MoverData:36–43`: `Guidance`, `Acceleration 40`, `MaxSpeed 40`); `BroodlingEscortImpactB` (:2711–2716): `RemovePrecursor`, **`BroodlingAttack`**, `SuicideRemove`; `BroodlingAttack` (`CEffectIssueOrder`, :2769–2773): `Abil attack`, `Target Effect BroodlingEscort Value TargetUnitOrPoint`; `Precursor`, `core BehaviorData:224–236` (invulnerable, no draw, untargetable, no abilities) | The new Broodling is held as a precursor for one short missile flight (about one tick), then ordered to attack the escort's target unit, or the point; its kills are credited to the Brood Lord [DATA; "about one tick" INF] |
| S13 | `voidmulti EffectData:160–165`: `BroodlingEscortLaunch` and `BroodlingEscortLaunchB` take `BroodlingTimedLifeBroodLord` in place of `BroodlingTimedLife`; `BroodlingTimedLifeBroodLord`, `voidmulti EffectData:2289–2294`: `Behavior BroodlingFate`, `Flags UseDuration 1`, **`Duration 5`** | A Brood Lord's Broodling lives 5 Normal s (89.29 → **89 ticks**), not the behaviour's 8 [DATA] |
| S14 | `BroodlingFate`, `liberty BehaviorData:650–655`: parent `TimedLife`, `Duration 8`, `KillCredit Unknown`; `TimedLife`, `core BehaviorData:105–117`: **`DurationRandomMax 0.3`**, `ExpireEffect TimedLifeFate`, `Summoned`; `TimedLifeFate`, `core EffectData:207–211`: `Kill`, `NoKillCredit`, `Death Timeout` | Expiry kills with no credit; up to 0.3 Normal s (5.36 ticks) is added at random [DATA for the fields; that `UseDuration` keeps `DurationRandomMax` is INF, **verify**] |
| S15 | Structure case, `liberty EffectData:2717–2752`: `BroodlingEscortStructure` → `BroodlingEscortCU` (`NotHallucination`, `SpawnUnit Broodling`, `SpawnRange 5`, `SpawnEffect BroodlingEscortLaunch`: release, `BroodlingEscortMissile` with the same movers and `ImpactEffect BroodlingEscortDamage`, `FinishEffect BroodlingEscortImpact`: `RemovePrecursor`, `KillsToCaster`, `BroodlingAttack`), else `BroodlingEscortFallbackMissile` (damage only) | Against a structure the Broodling is created first and is itself thrown; the result is the same: 20 at impact, one Broodling at the target, attacking it [DATA; "the same" INF]. A hallucinated Brood Lord throws damage only (we model no hallucination) |
| S16 | Broodling, `liberty UnitData:11166–11193` (parent `BroodlingDefault`, :79–109): ground, `Collide Ground, ForceField, Small`, `Speed 3.836`, `Acceleration 1000`, turning 999.8437, `WeaponArray NeedleClaws`; `BroodlingDefault`: `LifeMax 30`, `LifeRegenRate 0.2734`, no `LifeArmor`, `Light`, `Biological`, `Sight 7`, `AttackTargetPriority 20`, `Radius 0.375`, `SeparationRadius 0.375`, `NoScore`, no `Food`, no cost; `voidmulti UnitData:150–156`: `Collide Locust`, **`LifeMax 20`**, **`Speed 2.9531`**; `swarmmulti :65–67`, `balancemulti :123–126` (collision, subgroup) | 20 life, armour 0, speed 0.1654 per tick, sight 7, radius 0.375, no supply, no cost, the small ground collision class [DATA] |
| S17 | `NeedleClaws` weapon, `liberty WeaponData:109–117`: `Melee`, `TargetFilters Ground,Visible;…`, `MinScanRange 15`, `Range 0.1`, `Period 0.6455`; `voidmulti WeaponData:402–404`: **`Period 0.8`**; damage `liberty EffectData:1413–1416`: `Amount 4` | 4 per 14 ticks (0.8 → 14.29 → 14), melee range 0.1, ground only, scan 15; damage point 3 and backswing 9 (`CWeapon` defaults) [DATA] |
| S18 | Upgrades, `liberty UpgradeData:502–503` (melee: `NeedleClaws` +1), :599–600 (ground carapace: `Broodling` armour +1), :674–676 (flyer attacks: `BroodlingEscortDamage`, `…Unit` +3), `libertymulti :131–138` (+2 per level) | The Broodling takes the Swarm's melee and ground-carapace upgrades; the escort strike +2 per flyer level, as our row already has [DATA] |
| S19 | `BroodLordWeaponRight`, `liberty MoverData:44–69`; `BroodLordWeaponLeft`, :620–644: phase 0 `Throw`, `Speed 5`, `Acceleration 3`, `MaxSpeed 8`, `Gravity 5`, `Outro 1.5,4.5`, `ThrowVector ±0.8,−1,0.75`; phase 1 `Guidance`, `Acceleration 20`, `MaxSpeed 40`; a `Wave` overlay | B-232's mover: the two are mirror images, so their flight times are equal [DATA] |
| S20 | Liquipedia, *Brood Lord (Legacy of the Void)* (<https://liquipedia.net/starcraft2/Brood_lord>), as quoted by a web search on 2026-11-26 (the page itself answered HTTP 429): "Damage: 20 (+2) (x2). DPS: 22.4 (+2.2)"; "If the Brood Lord has two Broodlings flying alongside it (the maximum), it will shoot both Broodlings on its attack … If it only has one Broodling by its side, it will only shoot that one"; "The recharge time for a Broodling is equal to the attack speed of the Brood Lord, so if ordered to continuously attack, the Brood Lord will be firing one Broodling at a time" | Two escorts at most; both thrown when armed; one at a time while attacking continuously [EXT; medium: search excerpts, not the page]. Its DPS figure counts two per period, which its own text says holds only for the first attack |
| S21 | Same search: no figure found for the Broodling's lifetime | S13's 5 Normal s stands on the data alone [EXT: nothing found] |
| S22 | Plan [`2026-11-24-hidden-harvesters`](2026-11-24-hidden-harvesters.md) S7: `CasterIsNotHidden` guards broodling spawns (`liberty EffectData:1582`, :1588) | Those are the **structure-death** spawns (`ZergBuildingSpawnBroodling6Delay`/`9Delay`, :1581–1592, run by `ZergBuildingDies6`/`9`, `liberty BehaviorData:1270–1285`); the Brood Lord's chain has no such validator (S11, S15) [DATA]. Structure-death Broodlings are not modelled: B-234 |

### 2.2 The rules that follow

1. **The magazine** (S3–S6, S20). A Brood Lord starts with 2 escorts. Each built escort is armed 9 ticks after it is
   built. Builds run one at a time, 45 ticks each, whenever fewer than 2 escorts exist or are building, at no cost.
2. **A release throws every armed escort** (S7, S8, S20), at most 2. With none armed the Brood Lord does not attack.
   Its own weapon (S2) gives the range, period, target filters and the swing (damage point 3, backswing 9); the
   escorts give the strikes. The consequence: 2 strikes on the first release, then 1 per 45 ticks once both have been
   spent (§2.4).
3. **The missile** (S9, S19): each escort flies `BroodLordWeaponRight` or `Left`, whose speeds are equal. Carry-over
   (plan `2026-11-28-missile-movers-b52` S11): phase 1 starts at the speed phase 0 reached.
4. **Impact** (S10): 20 (+2 per flyer weapons level) if the target passes `BroodlingEscortFilters` at impact; the
   missile follows its target while it lives (`ValidateWeapon 0`), as our row already does.
5. **Finish** (S9, S11, S15): when the missile's flight ends, **whether or not the damage applied**, one Broodling per
   escort is created at the target, or at the last point if the target was lost (`TargetUnitOrPoint`), searching up to
   5 for a valid place. It faces along the throw (`SetFacing`).
6. **The Broodling's orders** (S12): it attacks the escort's target unit; if the target is gone, it attack-moves to
   the point. After that it is an ordinary unit: it acquires by itself (no `DefaultAcquireLevel` override), and its
   owner may select and command it (no `Unselectable` or `Uncommandable` on `Broodling`).
7. **Timed life** (S13, S14): it dies 89 ticks after it is created, plus 0 to 5 ticks at random, with no kill credit
   and no damage source (death type `Timeout`).
8. **No supply, no cost, no score** (S16). Its kills are credited to the Brood Lord (`KillsToCaster`, S12); the
   simulation keeps no kill credit, so this changes nothing here.
9. **Miss and dead target** (rules 4, 5): a Broodling still spawns. If the Brood Lord dies while its missile flies, the
   missile flies on and the Broodling still spawns [INF: the escort is the missile; nothing in S9–S12 names the Brood
   Lord as a condition].

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| The Brood Lord's attack is the escort magazine; its own weapon has no effect | High | S1–S3 [DATA] |
| Two escorts at most, two at the start | High | S3 `CountStart 2`, S4 `QueueSize 2`, S20 [EXT]; no `MaxCount` is set in any layer, so the cap is S20's |
| Rebuilt one at a time, 45 ticks each, free | Medium-high | S3, S4 [DATA, INF], S20 [EXT] |
| A new escort armed 9 ticks after it is built | Medium, **verify** | S6 [DATA]; when the behaviour starts is INF |
| A release throws every armed escort, up to 2 | Medium-high | S20 [EXT]; S8 [DATA] for one escort's throw |
| The escort itself is the missile and launches from its orbit behind the Brood Lord | Medium | S3 `ExternalAngle`, S9 no `AmmoUnit` [INF]; the orbit distance is not in the data (D-11 A-3) |
| Damage 20 at impact, validated by the filters | High | S10 [DATA], weapon-validation S7 |
| One Broodling per escort, at the target or the point, hit or miss | Medium-high | S11 [DATA]; that a `FinishEffect` runs on a miss is INF from its name and its separation from `ImpactEffect` (**verify** against an editor reference) |
| Placement searches up to 5 world units | High for the value; low for whether other units block the place | S11 [DATA]; B-236 |
| Broodling vitals, speed, weapon, upgrades | High | S16–S18 [DATA] |
| Lifetime 89 ticks from a Brood Lord | High | S13 [DATA]; the task's "5.71 s" is `BroodlingFate`'s 8 Normal s (8 ÷ 1.4), which voidmulti replaces for the Brood Lord |
| Plus up to 5 ticks at random | Low-medium, **verify** | S14 [INF] |
| First order: attack the target, else the point; then acquires automatically | Medium-high | S12 [DATA], S16 (no acquire override) [INF] |
| Selectable and commandable by its owner | High | S16: no `Unselectable` or `Uncommandable` flag |
| No supply, no cost; kills credited to the Brood Lord; expiry gives no credit | High | S12, S14, S16 [DATA] |
| The precursor lasts about one tick | Medium | S12 (a 40-speed missile over a near-zero path) [INF] |
| B-232's mover values | High | S19 [DATA], resolved by the B-52 census |

### 2.4 Rates and flight ticks (planning script `~/scratch/broodlings-plan/flight.py`, §16)

**Flight** (path = edge distance + the Brood Lord's radius 1.0; the mover of D-2; release tick not counted):

| Edge distance | 0.5 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Today (constant 0.9) | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 12 | 13 |
| 5.0.14 (D-2) | **5** | **7** | **10** | **12** | **14** | **16** | **17** | **18** | **19** | **20** | **21** |

**Strikes** against a target held in range (rule 2; R = the first release, at the first swing's damage point):

| | R | R + 57 | R + 102 | R + 147 | … | 20 strikes after |
|---|---|---|---|---|---|---|
| Today | 2 | 2 (R + 45) | 2 (R + 90) | 2 | every 45 | R + 405 |
| 5.0.14 | 2 | 1 | 1 | 1 | every 45 | R + 822 |

Derivation of R + 57: both escorts are spent at R; the first rebuild ends at R + 45, is armed at R + 54, and the swing
it starts releases at R + 57 (damage point 3). The second rebuild runs R + 45 to R + 90, armed R + 99, released at the
next swing R + 102, and from then the queue keeps one build running, so one escort per 45 ticks. Steady escort damage
halves (20 per 45 ticks instead of 40); each Broodling adds 4 per 14 ticks for up to 89 ticks while it lives.

---

## 3. Diagnosis (on `156721c2`, read from the code)

1. **The rows.** `assets/catalog/units.ron`, `spore_matriarch` (`unit_type_id: 41`): `weapon` and `ground_weapon`
   both "Broodling Swarm", `base_damage: 20.0`, `attacks_count: 2`, `strike_interval_ticks: 0`, `period_ticks: 45`,
   `damage_point_ticks: 3`, `backswing_ticks: 9`, `max_range: 10.0`, `projectile_speed: Some(0.9)`, `missile_validation:
   (weapon: false, teleport: true)`, `attack_visibility: Visible(54)`, no `missile_movers`, `capabilities` with no
   hangar. `target_flags: (bits: 255)` on both: the ground-only rule lives in the archetype's weapon choice (**verify**
   in WP0 that the Brood Lord never releases at an air unit: battle 19 is the witness). Trained by the Swarm's
   structure whose `trains_units` is `["spore_matriarch"]` (`structures.ron`). No media.
2. **Reference tests carry the bugs.** `catalog_reference_values_tests.rs` lines `("Brood Lord", "weapon", 0.9, …,
   Some("B-232"))` and the same for `ground_weapon`; `weapon_catalog_rules_tests.rs` lists `spore_matriarch` as a
   `Missile` delivery citing `BroodlingEscortLaunchA`.
3. **A missile's end** (`combat_resolution.rs`): `advance_projectiles` collects finished projectiles and calls
   `resolve_projectile_impact` for `ProjectileStep::Impact`. A lost target under `ImpactLastKnownPosition` (the
   default, the Brood Lord's) keeps flying to the last point and ends in `Impact` with `target_id: None`, which lands
   no strike; `Disjointed` (fuel-out, `Fizzle`) lands nothing. **No code runs on a finish besides damage**; there is
   no "finish effect".
4. **Spawning.** `SimWorld::prepare_spawn` / `commit_spawn` (`world.rs`), used by production and by the hangar
   (`hangar/launch.rs`); `first_valid_placement` (`effects.rs`) searches rings of 0.5 up to a range for terrain valid
   for a movement layer (other units never block, plan `2026-11-10` D-8); ids from `next_entity_id`, never reused.
5. **No timed life.** No entity dies by a clock; deaths come from damage (and hangar members with their hangar).
6. **No strike gate.** A weapon releases whenever its cycle allows (`advance_weapon_cycles`); the strike count of a
   release is the weapon's `attacks_count`.
7. **The spell effect graph** (`catalog/spells.rs`, `EffectNode`) has no node that creates a unit, and weapon hits
   reach it only through upgrade unlocks (`run_weapon_hit_effects`).
8. **Kill credit.** The simulation keeps none (`match_state.rs`: defeat by structures only); nothing to change.
9. **The matrix harness** (`sim_fixtures/src/matchup.rs`) counts survivors among the squads it spawned (`living`), and
   ends a battle when a side's squads are dead. A spawned Broodling is neither counted nor keeps a battle running.
10. **The tick** (`SimWorld::step_into`): commands → production and buffs (upkeep) → unit tasks → vision → hangars →
    lost swings → acquisition → attack dashes → movement → collisions → weapons and projectiles → cast locks → reap.
11. **Media.** Of the Swarm, only `rhino_beetle` and `burrow_scorpion` have flipbooks; `spore_matriarch` has none
    (`ClientMediaStatus::presentable` refuses it, `CardLock::NoMedia`), so no client match fields it today.

---

## 4. Design decisions

### D-1. Model the escorts as a magazine, not as entities

The escorts are invulnerable, untargetable, unselectable and uncommandable (S5). Nothing in the world can touch them;
what they decide is **how many strikes a release has and when the next one can happen** (rules 1, 2). So the
magazine is modelled by its count, its build and its arming; the escorts' orbit is presentation (D-11 A-3, B-235).

- **Rejected: reuse the Carrier's hangar** (`HangarSpec`, members as entities). Its members are units that fly,
  take damage, dock and are directed by the hangar phase; escorts do none of that, and making them entities would put
  invulnerable flyers into movement, vision and acquisition (files the Gated plan protects) for no gameplay. The two
  share 5.0.14's `CAbilArmMagazine`, and the record says so; the hangar stays as it is.
- **Rejected: keep `attacks_count: 2` every period** (today). It doubles the steady escort damage (S20, §2.4) and
  would double the Broodlings: B-233.

### D-2. B-232: the escort missile's mover (data)

`spore_matriarch` `weapon` and `ground_weapon`:

| Field | Today | After |
|---|---|---|
| `projectile_speed` | `Some(0.9)` | `Some(2.24)`, the top speed (the B-52 validation: equal to the greatest `max_speed`) |
| `missile_movers` | — | `[(up_to_range: None, phases: [(speed: Some(0.28), acceleration: 0.009408, max_speed: 0.448, ends: Some(Flown(3.0))), (speed: None, acceleration: 0.06272, max_speed: 2.24, ends: None)])]` |

Derivations: `Speed 5` → 0.28, `Acceleration 3` → 0.009408, `MaxSpeed 8` → 0.448, the end the midpoint of `Outro
1.5,4.5` (weapon-delivery D-3); phase 1 `Acceleration 20` → 0.06272, `MaxSpeed 40` → 2.24, no `Speed`, so it starts
at the speed reached (carry-over). Right and Left are mirror images (S19), and both bands are `IfRangeLTE 500`, so one
unbounded mover is exact for the flight time. The throw's direction, gravity and the wave overlay are not modelled
(**B-231**, which already covers throw phases generically; this row joins its list in WP7). The reference row's
`known_bug: "B-232"` moves to `SC2_MISSILE_MOVERS`.

### D-3. B-233: the strike magazine (`sim_types`, `sim_core`)

On `ArchetypeCapabilities` (`crates/sim_types/src/commands.rs`):

```rust
/// Armed charges that each strike of this unit's weapons spends, rebuilt one at a time
/// (`StarCraft` II's `CAbilArmMagazine` with `External` members: the Brood Lord's escorts;
/// plan `2026-11-28-broodlings` D-3). `None` for every unit whose strikes need nothing.
#[serde(default)]
pub strike_magazine: Option<StrikeMagazineSpec>,

/// The magazine's authored rules.
pub struct StrikeMagazineSpec {
    /// Charges held at most, armed, arming and building together (2: S3, S20).
    pub capacity: u8,
    /// Charges armed when the unit is created (`CountStart`, 2).
    pub start: u8,
    /// Ticks to build one charge; one builds at a time (`Ammo1 Time` 2.5 Normal s = 45).
    pub build_ticks: u16,
    /// Ticks from a charge's build to its arming (`BroodlingAttackDelay` 0.5 Normal s = 9).
    pub arming_ticks: u16,
}
```

The state, on `Entity` (hashed; present exactly on units whose archetype has a magazine, created by
`prepare_spawn` with `start` armed, as the hangar's start members are):

```rust
/// A unit's strike magazine (plan `2026-11-28-broodlings` D-3).
pub struct StrikeMagazineState {
    /// Charges armed now.
    pub armed: u8,
    /// Ticks until the charge being armed is armed; 0 when none is arming.
    pub arming_left: u16,
    /// Ticks until the charge being built is built; 0 when none is building.
    pub build_left: u16,
}
```

**The rule.**
- **Upkeep** (`world/upkeep.rs`, per entity with a magazine, in id order, before weapons): if `arming_left > 0`,
  count it down; at 0 `armed += 1`. If `build_left > 0`, count it down; at 0 the charge starts arming
  (`arming_left = arming_ticks`; an `arming_ticks` of 0 arms it at once). Then, if no build runs and `armed` plus the
  charge arming (1 when `arming_left > 0`) is below `capacity`, start one: `build_left = build_ticks`. Validation makes
  `arming_ticks < build_ticks`, so at most one charge arms at a time and the state needs no list.
- **Weapon gate** (`combat_resolution.rs`, `advance_weapon_cycles`): a weapon of a unit with a magazine **does not
  begin a swing** while `armed == 0`; it keeps its target and its place in its cycle, as a weapon on cooldown does.
  The gate is read once per weapon step of such a unit only.
- **Release** (`dispatch_shot`): the strikes of the release are `min(armed, attacks_count)` (at least 1, by the
  gate), and `armed` drops by that many. The projectile carries that many strikes (`ActiveProjectile::strikes`), so
  one missile lands 1 or 2 strikes, as today's lands 2. Armed charges spent while the swing was in its damage point
  cannot happen: only the release spends.
- **Both weapon slots share it**: the Brood Lord's row authors the same weapon as `weapon` and `ground_weapon`, so the
  magazine lives on the archetype and the entity, not on a weapon.

**Validation** (typed `SimCoreError`, naming the slug and field): `capacity ≥ 1`; `start ≤ capacity`; `build_ticks
≥ 1`; `arming_ticks < build_ticks`; every weapon of the unit has `attacks_count ≤ capacity` and no
`magazine_launch`; a unit has no hangar and a strike magazine both.

**Why the two escorts in one missile.** Today one projectile carries both strikes of a release, and the two escorts
of 5.0.14 are thrown together and fly equal paths (S19), so one projectile with `strikes: 2` lands on the same tick as
two would. It spawns one Broodling per strike (D-4). Two projectiles would add a projectile per release for no
difference in any tick.

### D-4. B-21: spawn on impact (`sim_types::weapon`, `combat_resolution.rs`)

`WeaponSpec` gains, after `magazine_launch`:

```rust
/// Units this weapon's missile creates where its flight finishes, whether or not its strike
/// landed (`StarCraft` II's `FinishEffect` of a `CEffectCreateUnit`; plan
/// `2026-11-28-broodlings` D-4). `None`, the default, for every weapon that creates nothing.
#[serde(default)]
pub impact_spawn: Option<ImpactSpawn>,

/// What a finishing missile creates.
pub struct ImpactSpawn {
    /// The archetype created (RON: its slug).
    pub unit: UnitTypeId,
    /// Units created per strike the missile carries, at least 1 (`SpawnCount`, 1).
    pub per_strike: u8,
    /// World units around the finish point searched for a valid place (`SpawnRange`, 5).
    pub search_range: SimScalar,
    /// The timed life each created unit gets, if any (D-5).
    pub timed_life: Option<TimedLifeSpec>,
    /// The order each created unit gets (`CEffectIssueOrder`, `BroodlingAttack`).
    pub order: SpawnOrder,
}

/// The first order of a unit a missile creates.
pub enum SpawnOrder {
    /// Attack the missile's target if it is still a valid target for the created unit,
    /// otherwise attack-move to the finish point (`attack` on `TargetUnitOrPoint`).
    AttackTargetOrPoint,
}
```

**When it runs.** On every `ProjectileStep::Impact` of a projectile whose weapon has `impact_spawn`: after
`resolve_projectile_impact` (the strike, applied or refused by the filters at impact, weapon-validation D-4), whether
the strike landed, was refused, or the target had been lost and the missile flew to its last point. Not on
`Disjointed` (fuel-out or `Fizzle`): a missile removed without arriving finishes nowhere. The Brood Lord's missile
never fizzles (`ImpactLastKnownPosition`, `missile_validation.weapon: false`), so for it every launched escort makes a
Broodling (rule 9).

**Where.** The finish point is the target's position if `proj.target_id` is still `Some` (the impact tick's tracked
position), else `proj.target_pos` (the last point). The place is `first_valid_placement` for the created archetype's
movement layer from that point, up to `search_range`; with `per_strike × strikes` units, each takes the same search
(they may share a point; the existing collision resolution separates them on the next tick's movement, as it does
for any overlap). No valid place within the range: that unit is not created, and a
`SimulationDiagnostic::ImpactSpawnBlocked { tick, weapon, point }` is pushed (a typed, visible outcome, not a silent
drop). **Facing:** the direction from the missile's launch point to the finish point (`SetFacing`).

**When it commits.** Spawns are collected during the impacts into a retained buffer on the combat workspace
(`pending_spawns`, capacity kept at its high water mark; PERFORMANCE RATIONALE in the code) and committed **after the
last projectile of the tick resolves**, in the order the impacts resolved (the existing reverse-index order of
`advance_projectiles`), through `prepare_spawn` / `commit_spawn` with `owner = proj.owner`. So no Broodling created this
tick is struck by another projectile or splash of the same tick, and ids are deterministic. The buffer is empty at the
end of every tick, so it is not hashed.

**The created unit's order**, written as the hangar writes members' orders (`Entity::replace_orders`, never through
`PlayerCommand`): `AttackTarget { target, origin: Explicit }` when the missile's target lives and is a valid target
for the created unit's weapon (the same admission the hangar's D-6 reads, `world/commands.rs`, read, not changed);
otherwise `AttackMove { target: finish point }`. The unit is otherwise ordinary: its owner selects and commands it, and
when the order ends it acquires through the existing acquisition.

**The precursor** (S12, about one tick) is not modelled; committing after the projectile phase means the Broodling
first acts on the next tick, which is that tick (D-11 A-5).

**Validation:** `per_strike ≥ 1`; `search_range ≥ 0`; the unit exists, is mobile, is not a structure, has a weapon,
has `supply_cost` 0 and no hangar or strike magazine of its own (no chain); the weapon is a missile
(`projectile_speed` set; an instant weapon has no finish); the spawned unit is in no `trains_units`, start loadout or
scenario spawn list (it exists only by spawning; the hangar validation's rule); `per_strike × attacks_count ≤ 8`
(bounds the buffer).

**Rejected: a `CreateUnit` node in the spell effect graph, run from a weapon's finish.** The graph is the general
home of 5.0.14's effects and a later plan may move this there (structure-death Broodlings, B-234, need a death
response anyway). Today weapon hits reach the graph only through upgrade unlocks, the graph has no finish hook, and
an `EffectContext` has no strike count; adding all three for one row is more surface than a weapon field whose
fields are exactly 5.0.14's `CEffectCreateUnit` and `CEffectIssueOrder`. Recorded in §14.

### D-5. Timed life (`entity.rs`, `world/upkeep.rs`)

```rust
/// A unit's remaining life by the clock (`StarCraft` II `TimedLife` behaviours; plan
/// `2026-11-28-broodlings` D-5). Hashed.
pub timed_life: Option<u16>,   // ticks left; the unit dies when it reaches 0

/// How a spawned unit's clock is set (`sim_types::weapon`).
pub struct TimedLifeSpec {
    /// Ticks of life (`Duration`, 5 Normal s = 89).
    pub ticks: u16,
    /// Up to this many ticks added, drawn uniformly from 0 to it inclusive
    /// (`DurationRandomMax`, 0.3 Normal s = 5.36 → 5).
    pub random_extra_ticks: u16,
}
```

- **Set** at the spawn's commit: `ticks + draw`, `draw` from the world's seeded `ChaCha8Rng` in `[0, random_extra_ticks]`
  (one draw per created unit, in commit order; none when `random_extra_ticks` is 0). The draw changes the sequence of
  later random draws only in worlds where a Broodling is created (the worlds that move anyway).
- **Counted** in the per-entity upkeep (`update_production_and_buffs`): `timed_life -= 1`; at 0 the unit's life is set
  to 0 with no damage source, so `reap_dead_and_emit_shatter` removes it on the same tick with its ordinary death
  event (`Timeout`: no kill credit exists to withhold, §3 item 8). A unit spawned on tick T with 89 ticks dies on tick
  T + 89.
- The field is generic: any later spawn with a clock (structure-death Broodlings, B-234; Auto-Turret; Locusts) reuses
  it.

### D-6. The Broodling row (WP4)

`brood_mite` (provisional slug; `unit_type_id` next free, **48** provisional; faction `sylvan_swarm`;
`display_name: "Brood Mite"`; `lore`: "Short-lived parasite larva flung down by a Spore Matriarch, biting whatever it
lands on."; `sc2_analogue: "Broodling (Brood Lord spawn)"`; `shape: UnitSphere`; silhouette chosen in WP4 among the
Swarm's small melee silhouettes by the visual rule, Q0020), each value with its S16–S18 citation:
`mover_type: Ground`, collision class the small ground default (S16; plan `2026-11-25-collision-classes` lists
Broodling there), attributes `LIGHT | BIOLOGICAL`, `max_hp: 20.0`, `life_armor: 0.0`, `hp_regen_per_tick: 0.01531`,
`max_speed: 0.1654` (2.9531 × 0.056; WP4 checks the catalog's rounding rule against `razor_mantis`'s 0.1652 for the
same Normal speed and follows it), `acceleration` and
`deceleration` the catalog's instant value for 1000, `turn_rate_deg_per_tick: 55.99`, `collision_radius: 0.375`,
`separation_radius: 0.375`, `sight_radius: 7.0`, `scan_range: 15.0` (S17 `MinScanRange 15`; **verify** WP4 reads how the
catalog derives `scan_range` from `MinScanRange` and range, as the interceptor's 5.0 was), `attack_target_priority:
20`, `acquire_level` the default, `mineral_cost: 0`, `gas_cost: 0`, `supply_cost: 0`, `build_time_ticks: 0` (never
built; validation does not read it for a spawn-only unit), `selectable: true`, `order_targetable: true`,
`armor_class: Some("swarm_ground_carapace")`; weapon "Needle Claws": melee, `visual_kind` the Swarm's melee look,
`target_flags` ground only, `max_range: 0.1`, `base_damage: 4.0`, `attacks_count: 1`, `period_ticks: 14`,
`damage_point_ticks: 3`, `backswing_ticks: 9`, `random_delay_ticks: (-1.1161, 2.2321)`, `weapon_class:
Some("swarm_melee_attacks")`, `upgrade_bonus_per_level: 1.0`, `attack_visibility` and `launch_visibility` as the
Swarm's other melee rows; `effects` its existing melee looks; `veo_prompts` Idle, Walk, Attack written in WP4 in the
house style (D-12).

`spore_matriarch` gains (WP3 and WP4):

```ron
capabilities: (
    // 5.0.14 BroodLordHangar (CAbilArmMagazine, External): liberty AbilData:3808-3824 (CountStart 2,
    // Ammo1 Time 2.5 = 45 ticks, AutoBuild), BroodLordQueue2 QueueSize 2 (:2077-2082), BroodlingAttackDelay
    // 0.5 = 9 ticks (liberty BehaviorData:1361-1368); two at most (Liquipedia, Brood Lord); plan
    // 2026-11-28-broodlings D-3 (B-233).
    strike_magazine: Some((capacity: 2, start: 2, build_ticks: 45, arming_ticks: 9)),
    // every other capability as today
),
// on both weapon rows:
// 5.0.14 BroodlingEscortImpactA (liberty EffectData:2684-2692): one Broodling per escort at
// TargetUnitOrPoint, SpawnRange 5; BroodlingTimedLifeBroodLord Duration 5 = 89 ticks (voidmulti
// EffectData:2289-2294) plus TimedLife DurationRandomMax 0.3 = 5 ticks (core BehaviorData:105-117);
// BroodlingAttack (liberty EffectData:2769-2773); plan 2026-11-28-broodlings D-4, D-5 (B-21).
impact_spawn: Some((unit: "brood_mite", per_strike: 1, search_range: 5.0,
                    timed_life: Some((ticks: 89, random_extra_ticks: 5)),
                    order: AttackTargetOrPoint)),
```

`attacks_count: 2`, `base_damage: 20.0` and the swing stay (now meaning "up to two escorts, 20 each").

### D-7. Hashing and the schema

| Value | Hashed? | Why |
|---|---|---|
| `Entity::strike_magazine` (option tag, `armed`, `arming_left`, `build_left`) | **Yes** | Decides when and how many strikes |
| `Entity::timed_life` (option tag, ticks) | **Yes** | Decides a death tick |
| `WeaponSpec::impact_spawn` (option tag; unit id, per strike, search range, timed life option and values, order tag) | **Yes** | Hashed with each projectile's `WeaponSpec`, as every weapon field is |
| `pending_spawns` | No | Empty between ticks |
| `StrikeMagazineSpec` | Not as such | Catalog data |

`checksum.rs` gains: "Schema N adds each entity's strike magazine, `Entity::strike_magazine` (an option tag, then its
armed count, arming and build countdowns), and timed life, `Entity::timed_life` (an option tag, then its ticks), after
`hangar_link`; and each weapon's impact spawn, `WeaponSpec::impact_spawn` (an option tag, then its unit, count per
strike, search range, timed life and order), after `magazine_launch` (plan `2026-11-28-broodlings` D-3 to D-5). The
number is the next free at merge." The schema test moves with it.

### D-8. Packaging by cause

Each of the three changes that move battle 20 lands in its own measured package, so every moved world is attributed
to one cause: the mover (WP2, delivery), the magazine (WP3, strike count and timing), the Broodlings (WP4, new units).
WP1 builds all three mechanisms latent.

### D-9. Death, loss and edge cases

- **Brood Lord dies with missiles in flight:** they fly on, land and spawn (rule 9; the projectile owns its owner and
  weapon). Its magazine state goes with its entity.
- **Target dies, loads, cloaks undetected, burrows, lifts off or teleports in flight:** the strike is refused at
  impact (weapon validation), the missile finishes at the last point it tracked, the Broodling spawns there and
  attack-moves.
- **Target is a structure:** spawn at its centre fails terrain validation; the ring search finds the nearest valid
  point within 5 (around a 3×3 footprint that is its edge, about 1.5 to 2).
- **Target on a plateau or in a pit:** the finish point is on the target's level; the search is the created unit's
  layer, so it spawns on that level.
- **No place within 5** (deep inside a wall of structures): `ImpactSpawnBlocked`, nothing created.
- **Owner defeated:** spawns are committed as for production; the match state rules (structures only) are unchanged.
- **Broodlings in the matrix:** counted by neither side's survivors (§3 item 9), as squads are the claim; a battle
  ends when the Brood Lord's opponents are dead even if Broodlings live.

### D-10. Movement, acquisition and the Gated plan: reuse, in parallel

**Decision: Broodlings use the existing ground movement and acquisition with no edit to any file the Gated plan
protects; this plan runs in parallel with the Gated packages and is not sequenced after Gated WP4.** Reasons:
- A Broodling is an ordinary `MoverType::Ground` entity whose speed, acceleration, turning, radius and collision
  class come from its row; it moves under `AttackTarget` and `AttackMove`, both handled by today's movement for
  ground units.
- It is created through `prepare_spawn` / `commit_spawn`, after the movement and collision phases of its tick, at a
  terrain-valid point; overlaps are resolved by the existing collision phase on the next tick.
- The magazine, the spawn and the timed life live in `combat_resolution.rs`, a new `world/spawns.rs` (or
  `combat_resolution/impact_spawn.rs`), `world/upkeep.rs`, `entity.rs`, the catalog and the checksum. None of
  `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs` is edited; if any would
  need an edit, that is stop rule 6.

**What Gated's switch-over means for this plan.** Gated WP4 changes how every unit moves, Broodlings included. If a
Gated package lands on `main` between two measured packages of this plan, the next package's base is re-measured after
merging `main` (as the Carrier and B-52 plans did); battle 20 is judged under whatever pipeline `main` ships. Gated's
acceptance worlds carry no Brood Lord (WP0 confirms), so this plan cannot move them; after this plan lands, Gated's
re-measures see battle 20 with Broodlings, which its own baseline step absorbs.

### D-11. Approximations and deviations, each recorded

| # | What | Ours | Game | Record |
|---|---|---|---|---|
| A-1 | Throw path | Counted as progress to the target | `Throw` with `ThrowVector ±0.8,−1,0.75`, `Gravity 5`, a wave overlay | **B-231** (generic; the Brood Lord row is added to it in WP7) |
| A-2 | Two escorts thrown together | One projectile with 2 strikes | Two missiles, Right and Left | None: equal flight times (S19); D-3 |
| A-3 | Launch point and the escorts' orbit | The Brood Lord's centre; escorts not drawn | From the escort, at ±150° behind the Brood Lord (`ExternalAngle`), drawn orbiting | Launch point: detail, within the orbit's radius of the path (not in the data). Orbit not drawn: **B-235**, deferred (presentation) |
| A-4 | Who triggers the throw | The Brood Lord's weapon swing, gated by an armed charge | Each escort's own weapon (`BroodlingEscort`, range 9, from its orbit) under the hangar's `Retarget` | Detail: the period and the release tick follow the same rebuild and arming; the escort's range 9 from behind is within a radius of the Brood Lord's 10 (**verify** in WP0's census that no further limit applies) |
| A-5 | Precursor | None; the Broodling first acts on the next tick | Invulnerable, undrawn, about one tick | Detail |
| A-6 | Spawn placement among units | Terrain and structures only; units do not block; overlap separated | `CreateUnit` placement (whether units block is not in the data) | **B-236**, deferred, **verify** |
| A-7 | Leash 9, `Manage Recall` | Not modelled | Escorts return to the Brood Lord | None: escorts are not entities (D-1) |
| A-8 | Kill credit to the Brood Lord; `NoScore` | No kill accounting exists | Credited | None (§3 item 8) |
| A-9 | Timed-life random extra | Integer 0 to 5 ticks | Uniform 0 to 0.3 Normal s (5.36 ticks) | Detail; **verify** that `UseDuration` keeps it (S14) |
| A-10 | Hallucinated Brood Lord throws damage only | Not modelled (no hallucination) | `NotHallucination` (S15) | None |

### D-12. The client and the media (real assets, no stand-in)

**Code** (`client/src`, WP5):
1. **Presentability** (`media.rs`): a unit whose weapon has `impact_spawn` is presentable only if the spawned
   archetype is (as H-16 made a hangar unit depend on its member). Otherwise a trained Brood Lord could create units
   the client cannot draw.
2. **Spawned units are ordinary units** in `RenderSnapshot`: drawn from their flipbook, selectable, commandable, with
   their weapon's existing melee look. No birth look (`Birth 0`, S11); expiry is a death with the death look.
3. **Steady state.** Broodlings are created and die every 45 ticks while a Brood Lord attacks. The battle sync must
   not allocate in that steady state (charter §3.3): WP5 checks whether the render entity path reuses a pool for
   entities that enter and leave the snapshot; if not, it adds one (retained, sized by the high water mark), with a
   PERFORMANCE RATIONALE. The carrier plan's H-20 (members cycling) is the same need and is still waiting on media;
   whichever plan lands media first does the pool, and the other reuses it.
4. **No escort drawing** (B-235).

**Media: generate, do not reuse.** No Swarm unit with media is a small larva: `rhino_beetle` is a heavy armoured
beetle and `burrow_scorpion` a burrowing scorpion, and showing either as the Broodling would be a faked identity
(charter §1.2). And `spore_matriarch` itself has no media, so a Broodling's clips alone would show nothing in a match.
So **WP6 generates real Veo clips through `asset_baker`** for both:
- `brood_mite`: Idle, Walk, Attack (mobile, armed: `published_actions.rs`);
- `spore_matriarch`: Idle, Walk, Attack, its prompts rewritten on the batch-5 aircraft pattern (one creature seen from
  above at three-quarter, whole in the frame, no scenery), its Attack showing parasites thrown toward the lower right.

**Budget.** 6 clips at $0.40 (Veo 3.1 Fast, 720p, audio on) = $2.40; with seeds and regenerations the batch is capped
at **25 generations, $10.00**. Spent so far: $116.85 of $230 (Q0131); after this batch at most $126.85. The free
preflight checks the ledger's remaining total before anything is spent.

**The lesson of the Carrier batch (Q0131).** All eight saucer seeds drew light shafts, which the global negative
terms ("light rays") and two rewordings did not remove. So:
- **Seeds first, and few.** Each identity starts with **at most 4 seed Idles**; the batch stops (stop rule 8) if
  either identity's four seeds all break the backdrop rule (rays, shafts, radiating stripes, sunbursts, scenery),
  instead of spending on.
- **Texts avoid what brought the shafts:** no sky, clouds, "high above", "hovering", "glow from above" or lit-from-above
  wording; "seen from above at three-quarter, on a plain dark backdrop, lit softly from the front" is the house
  pattern; the Brood Lord analogue is described as a creature, not an aircraft.
- **No negative-term change.** `media_style.ron`'s global and per-kind negative terms stay as they are: per-identity
  negative terms are option (a) of Q0131, which is open. If Q0131 is answered (a) and the baker change is on `main`
  before WP6 starts, WP6 may use per-identity terms; that is an amendment, not part of this plan.
- **Review** with contact sheets in `docs/evidence/veo/review/`, loops without jumps, the owner's chat rules (Q0001,
  Q0024, Q0047). Baked by the existing importer, provenance recorded, both identities added to
  `assets/media/required_set.ron` as `Expansion`; evidence in `docs/evidence/veo/2026-11-28-batch-broodlings.md`.

**If Veo is blocked or stops** (key, service, quota, budget, or the seed rule): stop WP6, record the result, and ask
the owner with a filed question (charter §1.2); the simulation packages still land, and `spore_matriarch` stays locked
by `NoMedia` in the client, which is truthful and is today's state. WP5's steady-state test with Broodlings cycling
(B-12) then waits for the media, as the Carrier's H-20 does.

---

## 5. Determinism, checksum and coordination

- **Integers only.** Counters are `u8`/`u16`; the finish point, facing and search are fixed point; the random extra is
  one integer draw from the world's `ChaCha8Rng`. No float enters state.
- **Order.** Magazines and clocks count down in the per-entity upkeep in id order. Spawns commit after the projectile
  loop in the impact-resolution order; ids from `next_entity_id` in that order; draws in that order.
- **No new collection type, clock or global.** `pending_spawns` is a `Vec` on the combat workspace, retained.
- **Latent proof (WP1).** With the schema at `main`'s number and the new fields unhashed in a proof copy (the carrier
  plan's `proofcopy.sh` and `unhash.py`, copied and pointed at the three new fields), WP1's tree reproduces every
  recorded checkpoint of both goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and `wasm32-wasip1`
  debug and release. No shipped row uses a mechanism in WP1. Then the schema steps to the next free number; WP1 to WP6
  do not re-record, and each hand-back says the golden tests fail by design.
- **One re-record (WP7, final tree after merging `main`).** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM`, under the header "Re-recorded for plan 2026-11-28-broodlings (checksum schema M -> N):
  entity strike magazine and timed life, weapon impact spawn encoded; with the schema at M and the fields unhashed every
  checkpoint reproduced. Behaviour: none in these worlds (no Brood Lord)" (or the classed behaviour, if WP0's probe
  finds a Brood Lord there). Proved on the four builds. If `main` has taken N first, the merge repeats the re-record
  with the next free number.
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, WP0's binary against the final tree's,
  interleaved, two rounds each: every scenario at **50 ticks per second or more** (the charter's floor is 25; the
  plan's is 50); **instructions within +0.5 %** of the whole run (`perf stat -e instructions:u`, `DESIGN.md` §5's
  method) on the existing scenarios. The new per-entity work is two `Option` checks in upkeep. WP7 adds a **`brood`
  row** to `throughput_scenarios.ron` (a 200-unit ground battle of `pulse_bot`, found by property, plus 12 units with
  a strike magazine and impact spawn on the first army's side, found by property), which must itself run at ≥ 50
  ticks/s; it measures spawns, clocks and expiries every tick. Its figure enters `DESIGN.md` §5.
- **Gated coordination** as D-10.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` and use shipped units from
`sim_fixtures`, found by property, never by slug (the unit with a strike magazine; the unit created by an impact
spawn), except the shipped values tables. A scratch catalog copy supplies values the shipped data lacks. Every test is
written red first against the previous package's tree.

### 6.1 Mechanisms (WP1; unit tests in the touched modules; integration in new `crates/sim_core/tests/broodling_tests.rs`)

On a scratch catalog that gives a ground-attacking air unit a magazine `(2, 2, 45, 9)` and an impact spawn of a small
melee unit with timed life `(89, 5)`:
- **B-1 (magazine timing).** Against a held, unarmed target in range: strikes land 2 at the first impact, then 1 per
  release at R + 57, R + 102, R + 147, …, exactly §2.4's ticks (R the first release), over 20 strikes.
- **B-2 (gate).** With no armed charge, the weapon starts no swing, keeps its target and does not move; it swings on
  the tick a charge arms (if its cooldown is over).
- **B-3 (refill).** After an idle spell of ≥ 108 ticks (two builds and an arming) the next release is 2 strikes again.
- **B-4 (spawn count and place).** Each impact creates `strikes × per_strike` units at the target's position (on a
  free point) with the owner's id, facing the throw, with order `AttackTarget { target, Explicit }`.
- **B-5 (miss).** A target that dies, loads, cloaks undetected or teleports in flight: no strike lands, the units are
  created at the missile's last point with `AttackMove` to it. A target that becomes air (lifted): same.
- **B-6 (structure and terrain).** At a structure target the units appear at the nearest valid point within 5; at a
  target whose point and ring are all blocked, none appears and `ImpactSpawnBlocked` is reported.
- **B-7 (commit order).** A splash or second projectile resolving on the same tick does not strike a unit created this
  tick; two impacts on one tick create ids in the impact-resolution order; the same seed gives the same ids.
- **B-8 (timed life).** A created unit dies on tick T + 89 + d, d in [0, 5], with no damage source and its death
  event; over 200 spawns every d of 0 to 5 occurs and no other; the same seed gives the same d's.
- **B-9 (no Disjointed spawn).** A `Fizzle` missile whose target is lost creates nothing.
- **B-10 (validation and encoding).** Each D-3 and D-4 rule refused with a typed error naming the field; the shipped
  catalog validates; two worlds differing only in a magazine count, a clock or an impact spawn value have different
  checksums.

### 6.2 The rows (WP2 to WP4; `broodling_tests.rs`, `missile_mover_tests.rs`, reference tables)

- **B-11 (mover, WP2).** `SC2_MISSILE_MOVERS` gains the Brood Lord's row (S19, D-2); the `known_bug: "B-232"` rows
  leave the constant-speed table. In `missile_mover_tests.rs`, the air unit whose ground weapon has movers attacks a
  held ground target at §2.4's edge distances: impacts on §2.4's ticks (± 1 at a fixed-point boundary, recorded;
  otherwise stop rule 10).
- **B-12 (magazine, WP3).** The shipped Brood Lord analogue reproduces B-1 to B-3 with its shipped values; the
  reference test cites S3–S6.
- **B-13 (Broodling, WP4).** The shipped spawned archetype's values equal S16–S18 in the vitals, speed, weapon and
  upgrade tables (`catalog_reference_values_tests.rs`, labelled "Broodling"); the shipped Brood Lord analogue's
  impact creates it with 89 + 0..5 ticks of life and the attack order; it fights, is selectable and commandable by its
  owner (a `PlayerCommand` move to it is admitted), and is attacked by enemies' automatic targeting.
- **B-14 (zero recompilation).** In a catalog directory copy, changing `per_strike` to 2, the timed life to 50 or the
  magazine's `build_ticks` changes B-4's count, B-8's death tick or B-1's ticks in the same binary
  (`runtime_catalog_tests.rs`).
- **B-15 (supply).** Spawns capture no supply; the supply ledger validates every tick of a 600-tick Brood Lord battle.
- **B-16 (delivery census).** `weapon_catalog_rules_tests.rs`' row for the Brood Lord stays `Missile` and cites S9;
  the W-10 census is otherwise unchanged.

### 6.3 Client (WP5; `client` tests)

- **B-17.** `media` presentability: a unit whose weapon has `impact_spawn` is unpresentable while its spawn has no
  media, presentable when both have it (scratch media fixtures, never under `assets/`).
- **B-18.** Spawned units are selectable and commandable in the client; expiry plays the death look.
- **B-19 (steady state).** `sync_steady_state_tests` and `sync_allocation_count_tests` (valgrind) pass with a Brood Lord
  attacking for 600 ticks (Broodlings created and expiring); runs once the media of WP6 exists, as the Carrier's H-20
  does; until then it is recorded as waiting, not skipped silently (charter §1.2).

### 6.4 Media (WP6)

- **B-20.** Six accepted clips, baked flipbooks with provenance, `required_set.ron` entries, `asset_baker
  audit-media --strict` passes, `runtime_media_tests` load them from the runtime paths; the review evidence names each
  seed, its cost and the reason for each rejection. The ledger's total stays ≤ $230 and the batch's ≤ $10.

### 6.5 World check (pre-registered)

Every world that differs from the previous package's is explained by its **first difference** (`firstdiff.py` and a
per-entity trace) and classed:
- **S (schema bytes).** The goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the schema number and the new fields
  alone (WP1's proof).
- **D (delivery, WP2).** The first difference is a Brood Lord missile's position, phase or impact tick.
- **M (magazine, WP3).** The first difference is a Brood Lord release that does not happen, or carries fewer strikes,
  on a tick where it did before.
- **N (new units, WP4).** The first difference is the creation of a spawned unit (or the RNG draw of its clock), and
  everything after it is downstream.
- **P (presentation).** A renderer event, `PlayerView` or fog frame differs in presentation only.
- **Anything else** (a class of another package; a target, order, position, damage or draw not downstream of the
  package's class): a defect, stop rule 2.

### 6.6 Close battles

WP0 fixes the close-battle list from its baseline alone, by the rule of the weapon-delivery, carrier-interceptors and
B-52 WP0s (hold-facing plan §6.4): every matrix battle whose minority side wins at least 2 of its 20 baseline runs;
battles 31 and 33 at the same share over their 100 and 64 seeds; every battle a closed owner question or fixture comment
names close or restated; no other; a `known_bug` battle is not close. (The B-52 WP0 gave the carrier list plus battle
35's restatement.) **The list is frozen when WP0 commits and not edited after.**

An **outcome move** is a run whose winner changes, or a battle whose expectation (`expect`, survivor ranges,
`blue_lead_hp`) fails on a seed of record or a development seed where it held at the base.

- A battle **on the list** whose outcome moves with a first difference of the package's class is **restated under
  charter §8**: its claim or comment gains the moved share, one flipped seed's first differing tick traced (the release,
  strike count, flight or spawn that changed, and what it changed downstream), and the 5.0.14 source of the change
  (§2.1).
- **Any outcome move off the list, or on the list with a first difference not of the package's class, is stop rule
  4**, reported with its trace for review before the package continues. Battles 19 and 20 are predicted not close; a
  move in either stops.
- Held-out seeds (§7.3) only confirm; nothing is restated or tuned from them.

### 6.7 Predictions (recorded before measuring, not thresholds)

| World | WP1 | WP2 (mover) | WP3 (magazine) | WP4 (Broodlings) |
|---|---|---|---|---|
| Matrix 19 (Viking vs Brood Lord, held) | identical | identical (the Brood Lord never releases at the air Viking) | identical | identical |
| Matrix 20 (Brood Lord vs Roach) | identical | D: each strike lands about 7 ticks later (edge ≈ 3.6: 6 → 13 ticks); the roach still dies, a few ticks later | M: after the first volley one strike per 45 ticks; the roach (145 life, armour 1) dies around tick 300 instead of about 150, inside the 400-tick limit | N: Broodlings land on the roach, which turns on them; they add 3 per hit; the roach dies earlier than at WP3 |
| Every other matrix battle, battles 31, 33 | identical | identical | identical | identical |
| Showcase, showcase effects, skirmish | identical | D only if WP0's probe finds a Brood Lord (the AI may train it) | M likewise | N likewise |
| Goldens, sandbox | S, proved | S | S | S; re-recorded once in WP7 |
| Client skirmish stage log | identical | identical | identical | identical (the Brood Lord stays locked until WP6) |

Battle 20's margin is the risk: at WP3 the roach must still die by tick 400. The planning estimate (§2.4: 8 strikes of
19 against 145 life plus about 5 of regeneration, the 8th released at R + 282, R ≈ 3 to 6, flight ≈ 13) is about tick
300 without Broodlings; WP0's probe computes it from the measured release tick and the roach's measured regeneration. If WP0
predicts WP3 alone fails the limit, WP3 and WP4 are measured together (one package, classes M then N), because
5.0.14's escort damage never comes without its Broodlings; that merge is an amendment recorded before WP3 starts, not a
reaction to a failure.

---

## 7. Measurement

### 7.1 WP0 census and probe (scratch, not committed)

- **(c) Census.** `~/scratch/broodlings-plan/census/`: the B-52 census script copied and extended to resolve every
  element of §2.1 across the eight layers with its file and line (`BroodLordHangar` with every layer's override, the
  two unit types and `BroodlingDefault`, the weapons, the effect chain from `BroodlingEscort` to `BroodlingAttack`,
  the behaviours, the validators, the movers, the upgrades). It confirms S1–S19, prints every field of
  `CAbilArmMagazine` the hangar sets or inherits (a `MaxCount` anywhere would change D-3), and lists every other user
  of `BroodlingFate`, `TimedLife` and `CEffectCreateUnit` among shipped analogues (B-234's reach). A second listing
  prints every place `spore_matriarch` can enter a world: matrix squads, battles 31 and 33, both goldens, the
  sandbox, `scenarios/*.ron`, `start_loadouts.ron`, the throughput scenarios, and every `trains_units`.
- **(p) Probe** (`~/scratch/broodlings-plan/probe/`, from the B-52 probe; it may name slugs, being scratch): per world
  and seed, changing no state, for the Brood Lord: releases, strikes per release, edge distance and today's and
  §2.4's flight ticks at each release, the tick of each strike, whether the target was still valid at impact, and the
  roach's life per tick in battle 20. A positive control (a scratch test with a Brood Lord facing a ground enemy for
  300 ticks) proves a zero is a measured zero. From it, the battle 20 margin of §6.7 is computed for WP3 alone and for
  WP3 with WP4's Broodlings (estimated with a scratch patch of the mechanism, not committed). **Predicted:** releases
  only in battle 20; none in 19 (air target), in the goldens or the sandbox; the showcase, showcase effects and
  skirmish worlds measured, not predicted.
- **Close-battle list** (§6.6), **baseline** on four builds (both goldens and `DEFAULT_SANDBOX_CHECKSUM`),
  **throughput and instructions**.

### 7.2 Commands

The B-52 plan's scripts, copied to `~/scratch/broodlings-plan/scripts/` with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/broodlings-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/broodlings-plan
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
python3 -I $R/scripts/matrixdiff.py $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7).

### 7.3 Held-out seeds (WP7, confirmation only)

Once, on `main` and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase and
showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12. Allowed: differences of classes D, M, N, P
only in worlds WP0's probe lists as carrying a Brood Lord; outcome moves only in close battles already restated. Anything
else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. WP1's latent proof fails (a checkpoint differs with the schema at `main`'s number and the new fields unhashed):
   stop; something other than the encoding moved.
2. A measured world's first difference is not of class S or P or the package's class (§6.5), or is in a world WP0's
   probe did not list as carrying a Brood Lord: stop, trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. An outcome moves that §6.6 does not allow (battle 20's roach surviving to tick 400 included): stop for review, with
   the trace.
5. Throughput below 50 ticks/s in any scenario (the new `brood` row included), or instructions above +0.5 % on the
   existing scenarios: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit (the Gated coordination rule): stop and report to the orchestrator.
7. The client needs a change beyond D-12's items, or a steady-state or allocation test fails: stop.
8. **Media:** Veo blocked (key, service, quota, budget), the batch reaching 25 generations or $10, or **four seeds of
   either identity all breaking the backdrop rule**: stop WP6, record, file the question; nothing is installed.
9. WP0's census contradicts §2.1 (a `MaxCount`, a different rebuild time, a `SpawnCount`, another lifetime, or a
   condition on the finish effect): stop WP0; the plan is amended before WP1.
10. A measured flight tick of B-11 differs from §2.4 by more than one tick at any listed distance, or by one tick
    without a fixed-point boundary to explain it; or B-1's strike ticks differ from §2.4's at all: stop; amend §2.4 or
    the data with the reason.
11. Q0132 is answered other than (a) before WP3 lands: stop WP3 and amend D-3 and §6.7 first.

---

## 9. Client consequences

D-12 items 1 to 3 in code (WP5), media in WP6. Data reaches the client through `RenderSnapshot` entities (the
Broodlings), the missile's `ProjectileVisual::duration_secs` (the mover's flight time, as for every mover weapon) and
the death events of expiry. Tests: B-17 to B-19; the GPU and diagnostics suites run in WP5's package gate through
their catalogue triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (§13); §5 throughput figure | WP0, WP7 |
| `docs/evidence/2026-11-28-broodlings.md` | New | WP0 to WP7 |
| `crates/sim_types/src/commands.rs` | `StrikeMagazineSpec`, `ArchetypeCapabilities::strike_magazine` | WP1 |
| `crates/sim_types/src/weapon.rs` | `ImpactSpawn`, `SpawnOrder`, `TimedLifeSpec`, `WeaponSpec::impact_spawn` | WP1 |
| `crates/sim_core/src/entity.rs` | `StrikeMagazineState`, `Entity::strike_magazine`, `Entity::timed_life`, accessors | WP1 |
| `crates/sim_core/src/world.rs` | `prepare_spawn` stocks the magazine | WP1 |
| `crates/sim_core/src/world/upkeep.rs` | Magazine and clock countdowns | WP1 |
| `crates/sim_core/src/combat_resolution.rs` | The gate, the strike count at release, the spawn collection after impacts | WP1 |
| `crates/sim_core/src/combat_resolution/impact_spawn.rs` (new) | Spawn placement, commit, orders, clock draw | WP1 |
| `crates/sim_core/src/catalog.rs`, `catalog/validation.rs` | Defs, resolution, D-3/D-4 validation | WP1 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema line; fields | WP1 |
| `crates/sim_core/src/error.rs` or diagnostics | `ImpactSpawnBlocked` | WP1 |
| `crates/sim_core/tests/broodling_tests.rs` (new) | B-1 to B-10, B-12, B-13, B-15 | WP1, WP3, WP4 |
| `crates/sim_core/tests/{missile_mover_tests,catalog_reference_values_tests,weapon_catalog_rules_tests,runtime_catalog_tests,checksum_state_tests}.rs` | B-10, B-11, B-13, B-14, B-16 | WP1 to WP4 |
| `crates/gate/suites.ron` | The new test file in its catalogue | WP1 |
| `assets/catalog/units.ron` | `spore_matriarch` (WP2 movers, WP3 magazine, WP4 spawn); `brood_mite` (WP4); header comments of the new fields (WP1) | WP1 to WP4 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | Restatements under §6.6 only; battle 20's comment gains the mechanism's citation | WP2 to WP4 |
| `crates/client/src/media.rs` and the render entity path | D-12 items 1, 3 | WP5 |
| `crates/client/tests/…` | B-17 to B-19 | WP5 |
| `assets/media/media_style.ron` (identity overrides only, no negative terms), `assets/media/required_set.ron`, `assets/textures/`, `assets/videos/`, `docs/evidence/veo/`, `crates/asset_baker/tests/fixtures/unit_prompts_golden.txt` | Media | WP4 (prompts), WP6 |
| `crates/sim_core/benches/throughput_scenarios.ron` | `brood` row | WP7 |
| Goldens, `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded once | WP7 |
| `docs/plans/README.md` | Row | plan, WP1, WP7 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`hangar/`, `effects/` and `catalog/spells.rs`.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the change makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/broodlings-plan/`. Each data package measures its worlds against the previous package's (§6.5) before it
commits.

### WP0: records, census, probe, baseline (XS, docs and scratch)

§13's WP0 rows; §7.1's census, probe with its control, battle 20's margin, close-battle list (frozen), baseline on four
builds, throughput and instructions. **Acceptance:** stop rule 9 holds; the evidence lists every world's releases and
strikes and the frozen list. Commit `docs(design): restate B-21 and B-232, register B-233 to B-236 (plan
2026-11-28-broodlings WP0)`.

### WP1: the mechanisms, latent (L, `sim_types`, `sim_core`)

D-3 to D-5, D-7. Red first: B-1 to B-10 on the scratch catalog. **Acceptance:** they pass; every measured world
byte-identical to WP0's; latent proof 8 of 8, then the schema steps; stop rules 1 and 6 hold. Commit `feat(sim_core):
strike magazine, impact spawn and timed life (B-21, B-233; latent)`.

### WP2: the escort missile's mover (XS, data and tests)

D-2. Red first: B-11. Worlds against WP1's. **Acceptance:** B-11 passes; battle 20 moves by class D only; stop rules
2, 4, 10 hold. Commit `fix(catalog): the Brood Lord analogue's missiles fly BroodLordWeaponRight (B-232)`.

### WP3: the escort magazine (S, data and tests)

D-6's `strike_magazine`. Red first: B-12. Worlds against WP2's. **Acceptance:** B-12 passes; battle 20 moves by class M
only and keeps its expectation (or WP3 and WP4 were merged by amendment, §6.7); stop rules 2, 4, 10, 11 hold. Commit
`fix(catalog): the Brood Lord analogue throws its two escorts, rebuilt one at a time (B-233)`.

### WP4: the Broodling (M, data and tests)

D-6's `brood_mite` row (with its `veo_prompts`) and the `impact_spawn` on both weapon rows. Red first: B-13 to B-16.
Worlds against WP3's. **Acceptance:** the tests pass; battle 20 moves by class N only and keeps its expectation; stop
rules 2, 4, 6 hold. Commit `fix(catalog): the Brood Lord analogue's escorts land as Broodlings with a timed life
(B-21)`.

### WP5: client (M, `client`)

D-12 items 1 to 3. Red first: B-17, B-18 (B-19 written, recorded as waiting on WP6's media). **Acceptance:** they pass;
stop rule 7 holds. Commit `feat(client): spawned units, presentability of spawners (B-21)`.

### WP6: media (S, paid; `asset_baker` runs, assets, evidence)

D-12's batch with its seed rule. **Acceptance:** B-20; then B-19 runs and passes; or stop rule 8, recorded, question
filed, nothing installed. Commit `media: Veo batch for the Brood Lord analogue and its Broodling (plan
2026-11-28-broodlings WP6)`.

### WP7: confirmation, re-record, records (S)

Held-out seeds (§7.3); the `brood` throughput row and the comparison (§5); goldens and sandbox re-recorded once on the
final tree after merging `main`, proved on four builds; §13's WP7 rows; `## As built`. **Acceptance:** stop rules 3
and 5 hold; B-21, B-232, B-233 fixed. Commit `docs: close B-21, B-232 and B-233 (plan 2026-11-28-broodlings WP7)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `commands.rs`, `weapon.rs` | RON forms and defaults of the new types |
| Unit | `sim_core` `impact_spawn.rs`, `upkeep.rs`, `catalog/validation.rs` | Placement search, commit order, countdowns, D-3/D-4 rules |
| Integration | `crates/sim_core/tests/broodling_tests.rs` (new) | B-1 to B-10, B-12, B-13, B-15 through `SimWorld` and `PlayerCommand` |
| Integration | `missile_mover_tests.rs`, `catalog_reference_values_tests.rs`, `weapon_catalog_rules_tests.rs`, `runtime_catalog_tests.rs`, `checksum_state_tests.rs` | B-10, B-11, B-13, B-14, B-16 |
| Determinism | goldens, sandbox, four builds | §5: latent proof, one re-record |
| Client | media, selection, steady-state and allocation tests | B-17 to B-19 |
| Media | `audit-media --strict`, `runtime_media_tests` | B-20 |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s, ≤ +0.5 % instructions |

`crates/gate/suites.ron` gains `broodling_tests` in WP1, in the catalogue that selects `sim_core`'s combat tests.

---

## 13. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-21 restated with §2.1's chain (S3, S8–S13) and this plan; B-232 restated with S19 and this plan; **B-233** (two strikes every period; the magazine), **B-234** (Zerg structures spawn 6 or 9 Broodlings on death, `ZergBuildingDies6`/`9`, `liberty BehaviorData:1270–1285`, EffectData:1581–1592, 2791–2808, 8 Normal s each; not modelled; deferred to a plan that adds death responses), **B-235** (the escorts' orbit is not drawn; presentation, deferred), **B-236** (spawn placement ignores units, A-6, **verify**; deferred) registered | WP0 |
| `DESIGN.md` §4 | B-21, B-232, B-233 **fixed**; B-231 gains the Brood Lord row; the Combat row names strike magazines, impact spawns and timed life; doc 07's Brood Lord cell loses "*(Broodlings not modelled: bug B-21)*" | WP7 |
| `DESIGN.md` §5 | The `brood` throughput row | WP7 |
| `docs/plans/2026-11-28-missile-movers-b52.md` §14 | B-232 bullet gains "*Taken up by plan `2026-11-28-broodlings`.*" | WP7 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | B-21's row gains the pointer | WP7 |
| New `docs/evidence/2026-11-28-broodlings.md` | Census, probe, margin, close list, baseline, latent proof, worlds per package, restatements, held-out, goldens, bench | WP0 to WP7 |
| `docs/evidence/veo/2026-11-28-batch-broodlings.md` | The batch | WP6 |
| `docs/questions/open/0132-…` | Q0132 filed by the orchestrator if it accepts the draft (§15) | at merge of the plan |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP7) | plan, WP1, WP7 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **Structure-death Broodlings** (B-234): need a death response on structures; the spawn and timed-life mechanisms of
  this plan are built to serve it (a different lifetime, 8 Normal s, a count of 6 or 9, no order).
- **Escort orbit drawing** (B-235): presentation; the snapshot could later expose the armed count.
- **Spawn placement among units** (B-236): kept terrain-only, as casts' placement is.
- **A `CreateUnit` node in the spell graph** (D-4's rejected option): a later plan may move `impact_spawn` there when a
  second finish effect needs it.
- **Queen's `SpawnBroodlings`** (`void AbilData:1471`): campaign data, not multiplayer; not shipped.
- **Hallucination** (S15's fallback missile): not modelled anywhere.
- **The throw's path** (B-231): unchanged generic deferral.

---

## 15. Owner question (filed on main as [Q0132](../questions/open/0132-brood-lord-damage-rate.md))

```markdown
# Q0132: The Brood Lord throws its two escort Broodlings, then one at a time; accept the data's damage rate?

- **Asked:** 2026-11-28
- **Blocking:** no
- **Default until answered:** (a); the Brood Lord analogue throws both escorts on its first attack, then one per
  rebuild (every 45 ticks, 1.79 real seconds), and each escort lands as a Broodling

## Question

Today our Brood Lord analogue (`spore_matriarch`) deals two hits of 20 every 45 ticks and spawns nothing. In StarCraft
II 5.0.14 its attack is two "escort" Broodlings that orbit it: on an attack it throws every escort it has (two at most),
each hits for 20 and lands as a Broodling that fights for about 3.6 seconds; the escorts are rebuilt one at a time,
one per attack period. So after the first attack it throws one per period. Liquipedia's page says both things: "x2,
DPS 22.4" and "if ordered to continuously attack, the Brood Lord will be firing one Broodling at a time". The game
data supports the second (`BroodLordHangar`: 2 at the start, one built per 2.5 game seconds; plan
`2026-11-28-broodlings` §2).

- (a) **Follow the data:** two on the first attack, then one per period, plus a Broodling per hit. Steady hit damage
  halves; the Broodlings make up much of it, as in the game. **Recommended**: it is what 5.0.14 does.
- (b) **Keep two hits every period** and add a Broodling per hit: twice the game's Broodlings and hit damage after the
  first attack; recorded as a sanctioned deviation.

## Answer

```

---

## 16. Material (scratch, not committed)

- `~/scratch/broodlings-plan/flight.py`: §2.4's flight ticks (the B-52 integration rule: each tick steps the mean of
  its start and end speeds; impact when the remaining path is at most the step).
- `~/scratch/combat_wp3_data/`: the 5.0.14 data copy of §2.1.
- WP0 adds `census/`, `probe/`, `scripts/`, `bin-*`, `tree-*`, `world-*` under `~/scratch/broodlings-plan/`.
