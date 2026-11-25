# Blueprint: Weapon Delivery — Missiles Where 5.0.14 Launches Them (B-227; B-52 in part)

This plan fixes the row of `DESIGN.md` §4 that plan
[`2026-11-27-launch-and-spell-visibility`](2026-11-27-launch-and-spell-visibility.md) registered in its WP0 census and
deferred (its §14):

- **B-227:** "Three shipped weapons are delivered otherwise than 5.0.14 delivers them: the Phoenix analogue's Disruptor
  Beam (`flying_saucer`) and the Spine Crawler analogue's Impaler Spine (`thorn_lasher_dome`) strike at once, but the
  game's `IonCannons` launches two missiles (`IonCannonsLMLeft`, `IonCannonsLMRight`, `liberty` EffectData:1972,
  :1977) and `ImpalerTentacle` launches `ImpalerTentacleLM` (`liberty` WeaponData:365, EffectData:2151); the
  Carrier/Mothership analogue's Interceptor Fleet Salvo (`mothership_carrier`) is a missile, but the Carrier launches
  interceptors (`InterceptorLaunch` → `CarrierInterceptor`, a `CEffectUseMagazine`, `liberty` EffectData:893, :1940)
  and the Mothership's beam is instant (`MothershipBeam`, `voidmulti` EffectData:2669). So the launch image of B-205
  is missing for the first two and shown where the game has none for the third."

It also fixes the Phoenix and Spine Crawler halves of **B-52** ("Projectile flight times of multi-phase movers, High
Templar and Spine Crawler"), because a missile weapon is not fixed until its missile flies as long as the game's.

The design in one paragraph: the Phoenix and Spine Crawler analogues become missile weapons, and their missiles fly
the game's **movers**, not one constant speed. A weapon gains `missile_movers`, the launch effect's `Movers` array:
range bands (`IfRangeLTE`), each a short list of speed phases (start speed, acceleration, top speed, and the path
length after which the next phase starts, the phase's `Outro`). A missile picks its band once at the release and
integrates its speed exactly per tick; a weapon with no movers flies at its constant `projectile_speed` as today, byte
for byte. The Carrier's interceptors are a new subsystem and are **not** built here: they stay with the existing
**B-22** (sc2-deviation-fixes WP18). Until then, the analogue's salvo keeps its missile but shows no launch image
(`launch_visibility: Hidden`), which matches 5.0.14 under either identity. The analogue's mixed identity (Mothership
vitals, Carrier cost, range and weapon) is registered as **B-228** with a drafted owner question, **Q0129**. No
matrix battle carries any of the three units (§7.1 predicts no measured world moves); constructed tests carry the
proof. Schema: the next free number at merge.

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `2f922a50` (checksum
schema 50), with branch `launch-spell-visibility-plan` (`7afceb2a`, schema 53, about to merge) read for B-227,
`launch_visibility` and its census.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-228**; the draft question is **Q0129**.
  The checksum schema is the next free number at merge: `main` is at 50, the Gated plan reserves 51 and 52, the
  launch-and-spell-visibility plan takes 53, so this plan takes the next free number when it merges (**54** if those
  three land first). Every schema mention below reads "next free at merge".
- **Prerequisite.** The launch-and-spell-visibility plan is on `main` before WP0 of this plan: B-227's row,
  `WeaponSpec::launch_visibility` and the launch image (B-205) are its work, and this plan's tests read them.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S10 |
| Q0045 (align catalog times to SC2), Q0057 (ranges measured edge to edge) | Closed | D-2 (band distance), D-3 |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | §6.5 (close battles) |
| Q0128 1(a): a launch image goes to the aimed-at unit's owner | Filed by the visibility plan; default followed | D-7, W-16 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Gated plan coordination rule (`2026-11-27-gated-movement` §11): no other package edits `movement/`, `steering.rs`, acquisition's admission test or `movement_rules.ron` while a Gated package is open | Standing for this plan | §5, stop rule 6 |
| **Draft, not filed:** Q0129 "Is the Mothership Carrier the Carrier, the Mothership, or a hybrid?" | Proposed (§15); default (a) followed meanwhile | D-7, §14 |

---

## 1. Summary

| | Today (`2f922a50` + visibility) | After this plan |
|---|---|---|
| `flying_saucer` Disruptor Beam | Instant `LaserBeam`: both strikes land on the release tick | A missile carrying both strikes, released on the damage point (3 ticks) and landing 2 to 11 ticks later by distance (§2.3 table); launch image at the release, reveal at the impact |
| `thorn_lasher_dome` Impaler Spine | Instant `MeleeSlash` at 7 range | A missile (the tentacle) landing 4 to 7 ticks after the release; the short band (≤ 3) is the slower one, as the game's throw-then-strike mover is |
| `mothership_carrier` salvo | Missile, launch image at the release | Missile kept (interceptors are B-22); `launch_visibility: Hidden`, so no launch image; impact reveal unchanged |
| Missile flight | One constant speed per weapon | Constant speed, or the weapon's movers: range bands of speed phases, integrated exactly per tick |
| Every other weapon | — | Byte-identical (no movers authored) |
| Hashed state | — | `WeaponSpec::missile_movers`; `ActiveProjectile` gains `mover`, `phase`, `travelled`; schema next free at merge |
| Worlds | — | Predicted byte-identical in every measured world (no measured world fires these weapons; WP0 measures it); goldens move by the schema bytes alone and are re-recorded once |
| Records | B-227 open, B-52 open, B-22 open | B-227 fixed (its Carrier clause handed to B-22); B-52 narrowed to the Viking, Liberator, Banshee, Missile Turret and High Templar; B-228 and Q0129 registered |
| Size | — | M: WP0 (records, census, probe, baseline), WP1 (movers, latent), WP2 (the data), WP3 (client looks and tests), WP4 (confirmation, re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. Conversions as everywhere in the catalog (Q0045): a Normal speed `v` is `v × 1.4 / 25` world units per
tick (`0.056 v`); a Normal acceleration `a` is `a × 1.4² / 25²` per tick squared (`0.003136 a`, as `units.ron`'s
`acceleration` rows); a Normal time `t` is `t × 25 / 1.4` ticks.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`; `liberty EffectData:N` is line N of that layer's file | Every [DATA] value |
| S2 | `CEffectLaunchMissile` default, `core EffectData:95–109`: `PointFallback 1`, `ValidateTeleport 1`, `ValidateWeapon 1`, `Visibility Snapshot`, `ImpactLocation TargetUnit`, `ValidatorArray ##id##TargetFilters` | The flight checks and fallback every weapon missile has unless overridden [DATA] |
| S3 | Phoenix weapon `IonCannons`: `liberty WeaponData:184–193` (no `Effect`, so the `CWeaponLegacy` default `##id##`, `core` WeaponData:37, names the persistent `IonCannons`; no `DamagePoint` or `Backswing`, so the `CWeapon` defaults 0.167 and 0.5, `core` WeaponData:17–18); `Period 1.1` (`swarm` :176, `swarmmulti` :151); `Range 5` (`libertymulti` :18–20) | Damage point 2.98 → 3 ticks, backswing 8.93 → 9, period 19.6 → 20: **our row's values already match** [DATA] |
| S4 | `IonCannons` persistent, `liberty EffectData:1959–1967`: `PeriodCount 2`, `PeriodicEffectArray` `IonCannonsLMLeft`, `IonCannonsLMRight`, `PeriodicPeriodArray 0, 0`; reordered Right-then-Left in `libertymulti` :117–120 | **Two missiles, both on the release tick** [DATA] |
| S5 | `IonCannonsLM` and its children, `liberty EffectData:1968–1981`: `AmmoUnit IonCannonsWeapon`; `Movers MissileDefault IfRangeLTE 1`, `Movers IonCannonsWeapon{Left,Right} IfRangeLTE 6`; `ImpactEffect IonCannonsU{Left,Right}`; no flag or `Visibility` override | Mover chosen by launch range; every S2 default holds [DATA] |
| S6 | `IonCannonsU`, `liberty EffectData:1982–1987`: `CEffectDamage` parent `DU_WEAP_MISSILE`, `Visibility Visible`, `Amount 5`, `AttributeBonus Light 5`; Left and Right inherit it. `libertymulti` :124–133 and `swarm` :2254–2263 add `SearchFilters` and `ExcludeArray Target` but no `AreaArray` | Each missile deals 5 (+5 light) to its target; no area [DATA; "no area" INF from the missing `AreaArray`] |
| S7 | Movers `IonCannonsWeapon`, `…Left`, `…Right`, `liberty MoverData:446–484`: phase 0 `Throw`, `Speed 6`, `MaxSpeed 6`, `Outro 0.4,0.85`; phase 1 `Outro 1,1.5` (no driver or speed set); phase 2 `Guidance`, `Acceleration 60`, `MaxSpeed 40`. Left and Right differ from the parent only in `ThrowVector` | A missile thrown out at 6 for about 1.25 units, then accelerating at 60 to 40 [DATA; phase 1 carrying phase 0's speed is INF, **verify**] |
| S8 | `MissileDefault`, `core MoverData:13–25`: `Guidance`, `Acceleration 3200`, `MaxSpeed 18.75` | At 3200 the missile reaches 18.75 within 0.006 Normal s, a tenth of a tick: a constant 1.05 per tick (the catalog audit's rule, `docs/evidence/2026-11-04-catalog-time-audit.md` §8) [DATA] |
| S9 | Spine Crawler weapon `ImpalerTentacle`, `liberty WeaponData:365–375`: `Range 7`, `DamagePoint 0.3332`, `Effect ImpalerTentacleLM`; `Period 1.85` (`libertymulti` :15–17, `swarm` :172–174); no `Backswing` (default 0.5) | Damage point 5.95 → 6, backswing 9, period 33.04 → 33: **our row's values already match** [DATA] |
| S10 | `ImpalerTentacleLM`, `liberty EffectData:2151–2162`: `AmmoUnit SpineCrawlerWeapon` (a `MISSILE_INVULNERABLE`, `liberty` UnitData:8731); `Flags Return 1`, `ReturnDelay 0.175`, `ReturnMovers …Retract{Short,Long}`; `Movers SpineCrawlerTentacleExtendShort IfRangeLTE 3`, `…ExtendLong IfRangeLTE 7`; `ValidatorArray index 0 SpineCrawlerLMTargetFilters`, a validator no layer defines; no other flag or `Visibility` override. `ImpalerTentacleU` (:2163–2167): 25, +5 armored, `DU_WEAP_MISSILE` | One missile (the tentacle), validated and falling back as S2; it returns after the impact, which deals nothing [DATA]. The undefined validator checks nothing, as the undefined `##id##TargetFilters` of every other launch [INF] |
| S11 | `SpineCrawlerTentacleExtendShort`, `liberty MoverData:243–265`: phase 0 `Throw`, `Speed 1`, `Acceleration 10`, `MaxSpeed 20`, `Outro 0.2,0.4`; phase 1 `Guidance`, `Acceleration 140`, `MaxSpeed 200`. `…ExtendLong` (:266–278): one `Guidance` phase, `Speed 12`, `Acceleration 140`, `MaxSpeed 200` | Short shots are lifted slowly first, then strike; long shots strike at once and accelerate [DATA]. The catalog audit (§8, row `thorn_lasher_dome`) found "mover … not in the data" because it looked for the ammo unit's own mover; the launch effect's `Movers` array names these |
| S12 | SC2 editor guides, *Missile Movers* (<https://s2editor-guides.readthedocs.io/Classic_Tutorials/04_Misc/missile-movers/>, read 2026-11-25): `Speed` is "the speed the missile starts the phase with", and speed "normally carries over" to later phases; `Outro`'s two values are *Blend At* and *Stop At*, positive values "the distance from the unit that fired the missile" along the core driver's path; the `Movers` list's range field: "the distance between the launcher and the target must be less than or equal to this value"; `Throw` moves "along an arbitrary linear path", `Guidance` "after the target" | How phases chain and how a band is chosen [EXT, editor documentation] |
| S13 | Carrier weapon `InterceptorLaunch`, `liberty WeaponData:601–614`: `Range 8`, `Period 0.5`, `DamagePoint 0`, `Backswing 0`, `Effect InterceptorLaunchPersistent` (`liberty EffectData:893–902`: `Channeled`, `PeriodCount 8`, `PeriodicValidator CasterNotDead`, each period `CarrierInterceptor`; period 0.5, 0.375 in `voidmulti` :427–429); `CarrierInterceptor` is a `CEffectUseMagazine` (`liberty` :1940) | The Carrier's "attack" sends up to 8 interceptors out of its magazine, one each 0.375 s; no missile, no damage of its own [DATA] |
| S14 | `CarrierHangar`, a `CAbilArmMagazine`: `liberty AbilData:2083–2098` (`Leash 12`, `Retarget`, `InfoArray Ammo1 Time 8 CountStart 4 Manage Recall Unit Interceptor`, `AutoBuild`, `LeashRetarget`), `swarm` :467–469 (`MaxCount 8`), `voidmulti` :1296–1300 and `balancemulti` :1133–1137 (`Time 12`, `AutoBuildOn`) | Interceptors are units, 8 at most, 4 at the start, built one at a time in 12 Normal s each, recalled beyond a leash of 12 [DATA] |
| S15 | `Interceptor`, `liberty UnitData:3544–3601`: 40 life, 40 shields, speed 7.5; weapon `InterceptorBeam` (`CWeaponStrafe`, `liberty WeaponData:626–637`: `Range 2`, `Period 3`, 2 attacks through `InterceptorBeamPersistent` :1943–1952); damage `InterceptorBeamDamage` 5, `Visibility Visible` (:1953–1958; AA and AG variants `voidmulti` :2262–2283) | The damage is the interceptor's, instant, from 2 range [DATA] |
| S16 | `Mothership`, `liberty UnitData:4244–4316` (`voidmulti` :1057–1061): 350 life, 350 shields; `MothershipBeam` (`liberty WeaponData:513–526`: `Range 7`, `Period 2.21`, `DamagePoint 0`; `Effect MothershipBeamSetCustom`, `voidmulti` :395) → beam persistents of `MothershipBeamDamage` (6, `Visible`, `liberty EffectData:957–963`; `voidmulti` :2665–2675) | The Mothership's attack is an instant beam [DATA] |
| S17 | Carrier, `liberty UnitData:3469–3539`: 300 life, 150 shields, speed 1.875; trained in 90 Normal s (`balancemulti` AbilData:813) | Against our `mothership_carrier`: 350/350 (Mothership), 350/250 cost, 6 supply, 1,607 ticks (90 s, Carrier), range 8 (Carrier), speed 0.1048 (both) [DATA, values from `units_dump_lotv.txt` of S1] |

### 2.2 The rules that follow

1. **The Phoenix fires two missiles at once** (S4, S5). Their movers differ only in the throw direction (S7), so
   they fly the same path length and land on the same tick [INF]. Each carries one strike of 5 (+5 light) (S6). So
   the weapon is one release whose two strikes fly together: our `attacks_count: 2`, `strike_interval_ticks: 0`
   already says that, and one projectile carrying both strikes (`ActiveProjectile::strikes`) is exact.
2. **The Spine Crawler fires one missile** (S10), 25 (+5 armored).
3. **Both are ordinary weapon missiles** (S2, S5, S10): `ValidateWeapon` and `ValidateTeleport` on, `PointFallback`
   on, `Visibility Snapshot` at the launch. Our `missile_validation` default, `target_loss_behavior` default
   (`ImpactLastKnownPosition`) and `launch_visibility` default are each exact; no field is authored. A missile
   whose target dies, is loaded, turns invulnerable, cloaks or burrows undetected, or teleports flies on to the last
   point and lands there; neither damage has an area, so it strikes nothing.
4. **The damage point is the release.** The weapon's `DamagePoint` (S3, S9) is when the launch effect runs; the
   damage lands at the impact. Our rows' damage points, backswings and periods already equal the game's and do not
   change.
5. **The mover is chosen once, at the launch, by range** (S5, S10, S12): the first `Movers` entry whose
   `IfRangeLTE` admits the launcher-to-target distance; if none, the ammo unit's own mover. The Phoenix: 1.05
   constant at ≤ 1, its thrown-then-guided mover above (S7, S8; the ammo unit `IonCannonsWeapon` has the same
   phases, so a Phoenix with Anion Pulse-Crystals, range 7, flies it too). The Spine Crawler: short at ≤ 3, long at
   ≤ 7 (S11); nothing exists beyond 7, which no Spine Crawler reaches.
6. **Speed phases** (S7, S11, S12). A phase starts at its `Speed` if it sets one, otherwise at the speed carried in;
   it changes by `Acceleration` per second up to `MaxSpeed`; it ends when the path flown since the launch reaches
   its `Outro`. The game blends two phases between *Blend At* and *Stop At*; we switch at the midpoint (D-3). The
   path shape (`Throw` sideways or up, `Guidance` after the target) is not modelled: a homing missile cannot be
   dodged (plan `2026-11-11-contact-and-missiles` §2.2), so only its flight time decides anything, which is the rule
   plan `2026-11-05-sc2-deviation-fixes` §8 set for B-52 ("flight times, not the phase shape").
7. **The Carrier launches no missile** (S13): its weapon spends interceptors from a magazine (S14), and each
   interceptor is a unit that flies out and deals instant damage from 2 range (S15). The Mothership's beam is instant
   (S16). Neither launch effect is a `CEffectLaunchMissile`, so in 5.0.14 neither shows the struck player an image
   at the launch; the Carrier's interceptors are seen as units where they fly.
8. **The analogue mixes two units** (S17): its vitals are the Mothership's, its cost, supply, build time, range,
   weapon name and lore ("deploying swarms of autonomous saucer interceptors") the Carrier's. The catalog reference
   tests exempt it (`catalog_reference_values_tests.rs`, `"Carrier / Mothership"` in five exemption lists).

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| The Phoenix fires two missiles on its release tick; the Spine Crawler one | High | S4, S5, S10 [DATA] |
| Both missiles keep every `CEffectLaunchMissile` default (validation, `PointFallback`, `Snapshot`) | High | S2, S5, S10 [DATA] |
| Damage point, backswing and period of both rows already match 5.0.14 | High | S3, S9 against `units.ron`, `structures.ron` |
| Mover choice by launch range, first band that admits it | High for the data, medium for the distance measured | S5, S10, S12. Edge to edge (D-2) is [INF]: the bands' upper ends (6 for the Phoenix's range 5, 7 for the Spine Crawler's range 7) only cover the weapon's reach if they are measured as its range is |
| Phase speeds, accelerations and top speeds | High | S7, S8, S11 [DATA] |
| A phase without `Speed` carries the previous speed; the Phoenix's phase 1 is a `Throw` at 6 | Medium, **verify** | S12 [EXT]; phase 1 names no driver |
| Path flown in a throw counts as progress towards the target | Low-medium, **verify** | Not modelled (rule 6); the error is the throw's sideways share of ≤ 1.5 units |
| `Outro` switch at the blend midpoint | Our choice (D-3) | S12 says the blend runs between the two values |
| Resulting flight ticks (D-2's integration): Phoenix 2 at ≤ 1 edge range, then 5 (at 1.01), 7 (2), 8 (3), 9 (4), 10 (5), 10 (6), 11 (7); Spine Crawler 5 to 7 at ≤ 3 (5 at 0.1, 6 at 0.5–1, 7 at 2–3), 4 at 3.01–5, 5 at 6–7 | Medium (follows from the rows above) | `~/scratch/weapon-delivery/flight.py` (planning script, §16); WP1's tests pin the integration, WP2's the shipped ticks |
| The Carrier's attack is a magazine of interceptor units, not a missile; the Mothership's an instant beam | High | S13 to S16 [DATA] |
| The analogue's intended identity | Unclear | S17; hence Q0129 |

---

## 3. Diagnosis (on `2f922a50` plus the visibility branch, read from the code)

1. **Delivery is chosen by `projectile_speed`.** `dispatch_shot` (`crates/sim_core/src/combat_resolution.rs`) calls
   `launch_projectile(shot, speed)` when `shot.weapon.projectile_speed` is `Some`, else `resolve_instant_shot`.
   `flying_saucer`'s `air_weapon` and `thorn_lasher_dome`'s `ground_weapon` set `projectile_speed: None`
   (`assets/catalog/units.ron`, `structures.ron`).
2. **One speed per missile.** `ActiveProjectile::speed` (`entity.rs`, hashed) is set at the launch and never
   changes. `advance_projectile` impacts when `distance to the target's centre ≤ speed + target radius`, otherwise
   moves `speed` towards the aim point; the missile starts at the attacker's centre (`shot.origin`).
3. **The visual's flight time is computed apart.** `launch_projectile` derives `ticks_to_impact =
   ceil(max(dist − target_radius − attacker_radius, MIN_FLIGHT_DISTANCE) / speed)` for the renderer event
   (`CombatEventVisual::ProjectileVisual::duration_secs`), presentation only.
4. **Validation** (`catalog/validation.rs`, `WeaponSpec` checks): a `Projectile` visual kind needs a speed; a
   non-default `missile_validation` or (after the visibility merge) `launch_visibility` on an instant weapon is
   refused; the speed must be positive.
5. **Encoding.** `WeaponSpec` is hashed in full with each projectile (`fields!(WeaponSpec {…})`,
   `checksum/encoding.rs`), as is `ActiveProjectile` (`fields!(ActiveProjectile {…})`).
6. **The client draws a projectile weapon only through a flying travel effect.** `effect_catalog_tests.rs` (its
   `delivery` helper and the per-weapon loop) requires a projectile weapon to name a `travel` that `flies` the path
   and an impact; a `sustain`ed travel (`prism_beam`, today the Phoenix's) is allowed only on an instant beam.
   `thorn_lasher_dome` names an impact (`impaler_strike`) and no travel.
7. **The Carrier analogue** is one 8-strike missile weapon (`attacks_count: 8`, `projectile_speed: Some(1.2)`,
   range 8, `attack_visibility: Visible(54)`), recorded as **B-22** "Carrier interceptors are one 8-strike weapon"
   (`DESIGN.md` §4, sc2-deviation-fixes WP18) and as a class E row of the catalog audit ("not modelled: our single
   8-strike weapon abstracts the interceptors").
8. **B-52 is open** (`DESIGN.md` §4: "Projectile flight times of multi-phase movers, High Templar and Spine
   Crawler | WP14"); sc2-deviation-fixes U-15 lists the Viking, Liberator, Banshee, Missile Turret and Phoenix movers.
   No mechanism for a changing speed exists, so every such row carries an unsourced constant.

---

## 4. Design decisions

### D-1. The two rows

| Row | Field | Today | After |
|---|---|---|---|
| `flying_saucer` `air_weapon` | `visual_kind` | `LaserBeam` | `Projectile` |
| | `projectile_speed` | `None` | `Some(2.24)`, the top speed (D-5) |
| | `missile_movers` | — | two bands (D-2 table) |
| | everything else | — | unchanged (damage point 3, backswing 9, period 20, 2 strikes together, `Visible(54)` at the impact) |
| `thorn_lasher_dome` `ground_weapon` | `visual_kind` | `MeleeSlash` | `Projectile` |
| | `projectile_speed` | `None` | `Some(11.2)`, the top speed (D-5) |
| | `missile_movers` | — | two bands (D-2 table) |
| | everything else | — | unchanged |
| `mothership_carrier` `weapon` | `launch_visibility` | default `Snapshot` | `Hidden` (D-7) |

Each changed field carries its citation comment, in the style of the rows' existing comments.

### D-2. Missile movers: the type and the rule

In `crates/sim_types/src/weapon.rs`, after `launch_visibility`:

```rust
/// The movers a weapon's missile may fly: `StarCraft` II's `CEffectLaunchMissile` `Movers`
/// array, tried in order at the launch (plan `2026-11-27-weapon-delivery` D-2). Empty, the
/// default: the missile flies at the constant `projectile_speed`.
#[serde(default)]
pub missile_movers: Vec<MissileMover>,

/// One entry of a launch effect's `Movers` array: the speed phases a missile flies when the
/// launch distance is at most `up_to_range`.
pub struct MissileMover {
    /// Greatest edge-to-edge distance, world units, between attacker and target at the
    /// release for which this mover is chosen (`IfRangeLTE`); `None` admits any distance and
    /// is allowed only on the last mover.
    pub up_to_range: Option<SimScalar>,
    /// The mover's `MotionPhases`, flown in order; at least one.
    pub phases: Vec<MissilePhase>,
}

/// One `MotionPhases` entry, reduced to its speed profile (the path shape is not modelled).
pub struct MissilePhase {
    /// Speed the phase starts at, world units per tick (`Speed`); `None` carries the speed
    /// the missile has when the phase starts (0 on the first phase).
    pub speed: Option<SimScalar>,
    /// Change of speed per tick, world units per tick squared (`Acceleration`); not negative.
    pub acceleration: SimScalar,
    /// Greatest speed in the phase, world units per tick (`MaxSpeed`); positive.
    pub max_speed: SimScalar,
    /// Path length from the launch, world units, at which the next phase starts (`Outro`,
    /// D-3); `None` on the last phase and only there.
    pub ends_after: Option<SimScalar>,
}
```

`ActiveProjectile` (`entity.rs`) gains three hashed fields: `mover: Option<u8>` (the index of the chosen mover in
`weapon_spec.missile_movers`; `None` for a constant missile), `phase: u8`, and `travelled: SimScalar` (path flown since
the launch). `speed` keeps its meaning, now "the speed at the start of the next tick".

**The rule, at the release** (`launch_projectile`): the edge-to-edge distance `d` is the one the weapon's range check
reads (Q0057's edge rule: centre distance less both range radii). The mover is the first whose `up_to_range` is
`None` or `≥ d`; validation guarantees one exists when the list is not empty. `speed` is the first phase's `speed`
(or 0), `phase` 0, `travelled` 0.

**The rule, each tick** (`advance_projectile`, for a missile with a mover): with phase `p`,

```
next   = min(speed + p.acceleration, p.max_speed)   (a speed carried in above the cap drops to it)
step   = (speed + next) / 2
impact when distance to the aim point's centre ≤ step + target radius   (as today, with `step` for `speed`)
else   move `step` towards the aim point; travelled += step; speed = next;
       while p has ends_after and travelled ≥ ends_after: p += 1; if p.speed is Some, speed = p.speed
```

`step` as the mean of the speeds at the two ends of the tick integrates constant acceleration exactly: after `k`
ticks the path is `v₀k + ak²/2`, the continuous value at whole ticks, until the cap. A constant missile (`mover:
None`) keeps today's code path and arithmetic exactly (step = `speed`, nothing else written), which is what makes WP1
latent. Lifetime, target tracking, validation, teleport and fallback (`track_projectile_target`) are untouched and
apply to both kinds.

**The visual's flight time** uses one helper, `flight_ticks(profile, path)`, for both kinds, over today's path
(`max(dist − target_radius − attacker_radius, MIN_FLIGHT_DISTANCE)`): for a constant missile it returns
`ceil(path / speed)` (today's value exactly, proved by a unit test over a table of paths); for a mover it steps the
same integration up to `max_projectile_lifetime_ticks`. Presentation only, no allocation.

**Shipped values** (Normal values from §2.1, converted as §2's header):

| Weapon | Mover | `up_to_range` | Phases (`speed`, `acceleration`, `max_speed`, `ends_after`) | Source |
|---|---|---|---|---|
| `flying_saucer` | `MissileDefault` | 1.0 | (1.05, 0.0, 1.05, None) | S5, S8 (3200 is instant: constant 18.75) |
| | `IonCannonsWeapon{Left,Right}` and the ammo unit's own `IonCannonsWeapon` | None | (0.336, 0.0, 0.336, 1.25), (None, 0.18816, 2.24, None) | S5, S7: `Speed 6`, `MaxSpeed 6`, phases 0 and 1 merged to the midpoint of phase 1's `Outro 1,1.5`; `Acceleration 60`, `MaxSpeed 40` |
| `thorn_lasher_dome` | `SpineCrawlerTentacleExtendShort` | 3.0 | (0.056, 0.03136, 1.12, 0.3), (None, 0.43904, 11.2, None) | S10, S11: `Speed 1`, `Acceleration 10`, `MaxSpeed 20`, `Outro 0.2,0.4` → 0.3; `Acceleration 140`, `MaxSpeed 200` |
| | `SpineCrawlerTentacleExtendLong` | None | (0.672, 0.43904, 11.2, None) | S10, S11: `Speed 12`, `Acceleration 140`, `MaxSpeed 200`; its `IfRangeLTE 7` is the weapon's range, and no further mover exists (rule 5), so the last band is unbounded |

**Why bands of phases, not one equivalent speed.** A constant speed matched at the weapon's full range is a one-line
data change, but it is wrong by a factor of two to three at short range for both weapons (the Spine Crawler's short
mover is the *slower* one: 5 to 7 ticks against 2 to 4 for a constant matched at 7), and it would leave B-52's other
rows with no way to be fixed but more unsourced constants. The phase list is the game's own structure reduced to the
one quantity that decides anything, flight time; it adds two small types, three hashed projectile fields and one
branch in `advance_projectile`.

**Why the band is chosen once.** The game picks the mover when it creates the missile (S12: "the distance between
the launcher and the target"); a target that walks closer does not change it.

**Rejected:** a `Delivery` enum replacing `projectile_speed` (cleaner, but it rewrites every weapon literal in eight
test files and both catalogs for no behaviour); per-phase drivers (`Throw` path geometry), since no rule reads a
missile's path.

### D-3. Approximations, each recorded

| Game | Ours | Why / effect |
|---|---|---|
| `Outro` blends two phases between *Blend At* and *Stop At* | Switch at the midpoint | One switch point; the blend is a fraction of a tick at these speeds |
| `Throw` flies an authored direction (sideways, up) | Its path counts as progress towards the target | Path not modelled (§2.2 rule 6). The Phoenix's throw is ≤ 1.5 units; the short tentacle's 0.3; the flight time is under-counted by at most the throw's sideways share (**verify**, R-1) |
| Two Phoenix missiles, left and right | One projectile, 2 strikes | Same path length, same tick (§2.2 rule 1) |
| The tentacle returns (`Flags Return`, `ReturnMovers`) | Not modelled | The return deals nothing and gates nothing (the weapon's period runs from the release) |
| A missile is a unit (`IonCannonsWeapon`, 10 life; `SpineCrawlerWeapon` invulnerable) | Not a unit | No shipped effect targets missiles (Point Defense Drone not shipped); as for every other missile |
| Arrival test 3D at the target's attach point | Our edge rule | Unchanged (plan `2026-11-11-contact-and-missiles` §2.2) |

### D-4. Validation (`catalog/validation.rs`)

Each refused with a typed `CatalogError` naming the definition and the field:

- `missile_movers` not empty on a weapon without `projectile_speed` ("an instant weapon launches no missile"), as
  `missile_validation` is.
- A mover with no phase; a phase with `max_speed ≤ 0`, negative `acceleration` or negative `speed`, or a `speed`
  above its `max_speed`.
- `ends_after` missing on a phase that is not the last, present on the last, not positive, or not greater than the
  previous phase's.
- `up_to_range` not ascending, negative, or `None` on any but the last mover; the list not ending with `None`
  (every launch must find a mover; a weapon whose range exceeds a bounded last band would otherwise launch none).
- `projectile_speed` differing from the greatest `max_speed` of all phases (D-5).
- At most 4 movers and 4 phases each (the Phoenix's launch has 3 movers counting its ammo unit's, and 3 phases;
  bounded lists keep `flight_ticks` and the clone per launch small).

Negative acceleration (the game allows it) is refused for now: no shipped mover has it, and a decelerating missile
needs a minimum speed the type does not carry. Recorded in §14.

### D-5. `projectile_speed` with movers

`projectile_speed` stays the delivery switch (`Some` = a missile) read by `dispatch_shot`, the client tests and the
diagnostics. With movers it is documented and validated as **the missile's top speed**, the greatest `max_speed` of
its phases: a value every consumer can still read meaningfully (e.g. a client asking how fast a shot can fly), and
never a dead field. Its doc becomes: "The missile's speed per tick: its constant speed, or with `missile_movers` its
top speed (validated). `None`: an instant weapon."

### D-6. What is hashed, and the schema

| Value | Hashed? | Why |
|---|---|---|
| `WeaponSpec::missile_movers` (each mover's `up_to_range` and phases' four values, in order, with a length prefix) | **Yes**, after `launch_visibility` | It decides impact ticks; `WeaponSpec` is hashed in full with each projectile and every behaviour field since schema 38 rode it with a schema step |
| `ActiveProjectile::{mover, phase, travelled}` | **Yes**, after `target_teleport_count` | Authoritative flight state |
| `MissileMover`, `MissilePhase` encodings | `fields!` with every field | — |

`checksum.rs` gains: "Schema N adds each weapon's missile movers (`WeaponSpec::missile_movers`: count, then per mover
its range bound and phases) after `launch_visibility`, and each projectile's mover index, phase and path flown
(`ActiveProjectile::{mover, phase, travelled}`) after `target_teleport_count` (plan `2026-11-27-weapon-delivery`
D-6). The number is the next free at merge." The `mothership_carrier`'s `Hidden` changes its projectiles' bytes
through `launch_visibility`, already hashed by the visibility plan.

### D-7. The Carrier: scope decision

**Decision: the interceptors are not planned here; they stay with B-22.** Reasons:

1. They are a subsystem, not a delivery: a magazine ability with build time, auto-build and a starting count (S14),
   interceptors as units with their own vitals, movement, weapon and leash (S15), launch and recall, the Carrier's
   attack as a channeled 8-period spend (S13), and death rules (`InterceptorFate`). That is new hashed state, a new
   unit kind, production, and AI and client work, larger than this whole plan.
2. **B-22** already registers it ("Carrier interceptors are one 8-strike weapon", sc2-deviation-fixes WP18), so a
   new bug number would duplicate it. B-227's third clause is handed to B-22 (row text in D-9).
3. The analogue's identity is unclear (§2.2 rule 8): if the owner chooses the Mothership (Q0129 (b)), interceptors
   are never built and the salvo becomes an instant beam instead. Planning interceptors before that answer could
   waste a large package.

**What this plan does for it now** (data, no mechanism): `launch_visibility: Hidden` on its salvo, cited "5.0.14
launches no missile for this attack: the Carrier's `CarrierInterceptor` is a `CEffectUseMagazine` (`liberty`
EffectData:1940) and the Mothership's beam is instant (`voidmulti` :2669), so no launch image; B-22, Q0129". This
removes the one visible error B-227 names for it under either identity, changes no reveal (a `Hidden` launch raises
nothing), and moves no outcome. The salvo's missile and its `Visible` impact stay until B-22 or Q0129 replaces them.

**Registered:** **B-228** (provisional) "The Mothership Carrier analogue mixes two units: its life and shields
(350/350) are the Mothership's, its cost (350/250), supply (6), build time (90 s), range (8), weapon and lore the
Carrier's (`liberty` UnitData:3469–3539, :4244–4316; `balancemulti` AbilData:813); five reference tables exempt it.
Open; Q0129 decides the identity; deferred to the plan that fixes B-22." And Q0129, drafted in §15.

### D-8. The client: looks from the catalog, nothing faked

- **Phoenix analogue:** `travel: Some("plasma_bolt")`, the Protoss energy bolt seven missile weapons already fly
  (the Stalker analogue's among them). The Ion Cannons' missiles are energy bolts; `prism_beam`, a sustained beam
  look, may only ride an instant beam (`effect_catalog_tests.rs`). `impact: Some("beam_burn")` stays.
- **Spine Crawler analogue:** a new `Shot` effect `impaler_tentacle` in `assets/catalog/effects.ron`, authored from
  the shipped `chitin_spine` texture and `spine_chitin` ramp that `impaler_strike` already draws: a ribbon from the
  dome to the target lasting `FlightTime` (the `lasts: FlightTime` convention of `plasma_bolt`). Its `impaler_strike`
  impact stays and now lands after the flight. No new texture, sprite sheet or video; the look is data, as every
  other effect is.
- No `client/src` change is expected: projectile events, the travel slot, the flight-time binding and the impact
  delay already exist. If one is needed, that is stop rule 7.

### D-9. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-227 restated with §2's sources and this plan; B-52 restated: the Phoenix and Spine Crawler halves taken by this plan; B-22 gains "B-227's Carrier clause: no launch image until interceptors (this plan D-7); identity Q0129"; **B-228** registered | WP0 |
| `DESIGN.md` §4 | B-227 **fixed** (Phoenix, Spine Crawler; Carrier clause handed to B-22 with the interim `Hidden`); B-52 narrowed to the Viking, Liberator, Banshee, Missile Turret and High Templar (the movers now make those data changes; S11-style sources noted: the High Templar's `HighTemplarWeaponMover`, `voidmulti` EffectData:1536); the Combat row names missile movers | WP4 |
| `docs/plans/2026-11-27-launch-and-spell-visibility.md` §14 | B-227 bullet gains "*Taken up by plan `2026-11-27-weapon-delivery`.*" | WP4 |
| `docs/evidence/2026-11-04-catalog-time-audit.md` | Not edited (a dated record); the evidence file of this plan cites its rows `flying_saucer` and `thorn_lasher_dome` and corrects "mover not in the data" for the Spine Crawler | — |
| New `docs/evidence/2026-11-27-weapon-delivery.md` | Census, probe, close-battle list, baseline on four builds, latent proof, worlds, held-out, goldens, bench | WP0 creates; WP1 to WP4 append |
| `docs/questions/open/0129-…` | Q0129 filed from §15 (number next free at filing) | WP0 |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP4) | plan, WP1, WP4 |

---

## 5. Determinism, checksum and coordination

- **Integers only.** Movers are `SimScalar` (fixed point), converted once from RON as `projectile_speed` is
  (`SimScalar::from_f32_lossy` in `catalog.rs`'s raw weapon). The integration is additions, a halving and a `min`;
  no division by a variable, no square root, no float. `flight_ticks` for the renderer event steps the same fixed
  point; only its result is turned into seconds, as today.
- **Order.** One projectile at a time, in the existing order; the mover is a pure function of the release.
- **No new collection, randomness or clock.** The two `Vec`s are catalog data, cloned with `WeaponSpec` into each
  projectile as `name` and `splash` already are.
  `// PERFORMANCE RATIONALE:` is not needed for that clone (it is the existing idiom); if WP1's bench shows the
  clone matters, the fix (an `Arc`'d weapon spec) is its own plan, not a workaround here.
- **Latent proof (WP1).** With `missile_movers` and the three projectile fields left out of the encoding and the
  schema at `main`'s number, the tree reproduces every recorded checkpoint of both goldens and
  `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and `wasm32-wasip1` debug and release (the visibility plan's
  `proofcopy.sh` and `unhash.py`, copied to `~/scratch/weapon-delivery/scripts/`). Then the fields are encoded at the
  next free schema. WP1 to WP3 do not re-record; their golden tests fail by design, and each hand-back says so.
- **One re-record (WP4, final tree after merging `main`).** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM`, under the header "Re-recorded for plan 2026-11-27-weapon-delivery (checksum schema M ->
  N): each weapon encodes its missile movers and each projectile its mover, phase and path flown; with those left out
  at schema M every checkpoint reproduced. Behaviour: none in these worlds (no weapon with movers fires; the
  Mothership Carrier does not appear)." Proved on the four builds. If `main` has taken N first, the merge repeats the
  re-record with the next free number. `golden_replay_orders`' `blue_spotter` (`flying_saucer`) has no ground target
  in its world (WP0 confirms zero releases; otherwise stop rule 2 classifies it).
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, on WP0's tree and the final tree: every
  scenario at **50 ticks per second or more**; **instructions within +0.5 %** of the whole run (`perf stat -e
  instructions:u`, `DESIGN.md` §5's method). No bench scenario fires a mover weapon (its units are `pulse_bot` and
  three casters), so the measured cost is one `Option` test per projectile per tick; a constructed micro-check in
  WP1 (W-9) bounds the mover path. Above budget is stop rule 5.
- **Gated coordination.** All code is in `combat_resolution.rs` (`launch_projectile`, `advance_projectile`, a new
  private `missile_flight` helper or module beside it), `entity.rs`, the catalog and the encoding. Nothing under
  `crates/sim_core/src/movement/`, nor `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`, is edited.
  Acquisition does not read projectiles. The release distance reuses the combat phase's existing edge-distance
  helper (read, not edited; if the only helper lives in `target_acquisition.rs`, WP1 calls it or copies the
  three-line formula into combat code with a comment naming the original, never edits it). If any of those files
  would need an edit, that is stop rule 6.

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (open ground, or a plateau
where vision matters) and use shipped units from `sim_fixtures`, found by property (a weapon with movers, an air or
ground target, `launch_visibility`), never by slug, except the shipped-values tables (W-10, W-18). A scratch catalog
copy supplies values the shipped data lacks. Every test is written red first against the previous package's tree.

### 6.1 The mechanism (WP1; unit tests in the new `missile_flight` code, integration in new `crates/sim_core/tests/missile_mover_tests.rs`)

- **W-1 (constant missiles unchanged).** For a table of paths and speeds, `flight_ticks` of a constant profile equals
  `ceil(path / speed)`; a projectile without movers impacts on exactly the tick and at exactly the position `main`'s
  code gives (old formula kept in the test).
- **W-2 (exact integration).** One phase (`v₀`, `a`, cap): the impact tick equals the smallest `k` with
  `v₀k + ak²/2 ≥ path` while uncapped, and the capped continuation after; the speed never exceeds `max_speed`.
- **W-3 (phases).** A two-phase mover switches on the first tick whose `travelled ≥ ends_after`; a phase with `speed`
  restarts at it, one without carries the speed in.
- **W-4 (bands).** Launch distances just below, at and just above each `up_to_range` choose the expected mover
  (`≤` inclusive); a target that closes in mid-flight keeps the mover chosen at the release.
- **W-5 (validation).** Every rule of D-4 refused with a typed error naming the field; omitted `missile_movers` reads
  empty; the shipped catalog validates.
- **W-6 (encoding).** Two projectiles differing only in `mover`, `phase`, `travelled`, or their weapon's movers, have
  different checksums.
- **W-7 (lifetime).** A mover missile whose flight exceeds `max_projectile_lifetime_ticks` fuels out as a constant
  one does.
- **W-8 (lost target mid-phase).** The target dies during phase 0: the missile keeps its profile to the last point
  and lands there; no unit takes damage.
- **W-9 (visual time).** For a mover missile the renderer event's `duration_secs` equals `flight_ticks` over today's
  path; for every constant missile in the shipped catalog it is byte-identical to `main`'s.

### 6.2 The data (WP2; `crates/sim_core/tests/missile_mover_tests.rs`, `weapon_catalog_rules_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs`)

- **W-10 (delivery census).** A table of every shipped weapon (unit and structure, mode overrides included): missile
  or instant, with its 5.0.14 launch effect and citation, against `projectile_speed`. `mothership_carrier` is listed
  with `known_bug: "B-22"` and must stay a missile until B-22 changes it (so the census never silently passes a
  mismatch). Red on WP1's tree: `flying_saucer` and `thorn_lasher_dome` mismatch.
- **W-11 (Phoenix release and impact).** A Phoenix-analogue unit (found as the shipped weapon with movers against air)
  attacks an air target at edge distances 0.5, 3 and 5: no damage on the release tick; both strikes land together on
  the tick of §2.3's table; the impact's reveal (`Visible`) starts on that tick, not the release.
- **W-12 (Spine Crawler release and impact).** The ground-weapon structure with movers attacks a ground target at edge
  distances 0.5, 2, 3, 3.5, 5 and 7: impact ticks as §2.3's table (the short band slower than the long).
- **W-13 (target lost).** For both weapons: the target dies, is loaded, turns invulnerable, or cloaks undetected
  mid-flight → the missile lands on the last point and damages nothing. A blinking (teleporting) target is lost
  likewise.
- **W-14 (cooldown from the release).** Both weapons' next release follows the period from the release, not the
  impact (unchanged rule, now with a flight in between).
- **W-15 (Carrier interim).** `mothership_carrier`'s salvo released at an unseen player's unit leaves no launch image;
  its impact reveals it as before.
- **W-16 (launch images).** With the visibility plan's rule: a Phoenix or Spine Crawler release at a unit whose owner
  does not see the attacker lists the attacker's image on the release tick (Q0128 1(a) default).
- **W-17 (zero recompilation).** In a catalog directory copy, changing the Spine Crawler's long band `max_speed` or
  removing its short band changes W-12's impact tick in the same binary.
- **W-18 (reference values).** `catalog_reference_values_tests.rs` gains a movers table citing S5, S7, S8, S10, S11
  for both rows, and the two rows leave any "instant" expectation they are in.

### 6.3 Client (WP3)

- **W-19 (catalog).** `effect_catalog_tests` passes: both weapons are `Delivery::Projectile` with a travel that flies
  and an impact; `impaler_tentacle` validates.
- **W-20 (impact after flight).** In `weapon_look_tests.rs`, a `ProjectileVisual` of each weapon asks for its travel
  and its impact delayed by its flight time.
- **W-21 (steady state).** `sync_steady_state_tests` and `sync_allocation_count_tests` (valgrind) unchanged with a
  Phoenix-analogue volley in flight.
- `weapon_battle_gpu_tests`, `effect_gpu_tests` and `client_diagnostics`' `effect_mix` tests run in WP3's package
  gate through their catalogue triggers; their per-weapon delivery reads `projectile_speed`, so the two rows move
  into their projectile cases without test edits (if an edit is needed, it is a test update named in the hand-back,
  not a `client/src` change).

### 6.4 World check (pre-registered)

Every world that differs from WP0's baseline is explained by its **first difference** (`firstdiff.py` and a per-entity
trace) and classed:

- **S (schema bytes).** The goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the encoding alone (WP1's proof).
- **D (delivery).** The first difference is a release of `flying_saucer`'s or `thorn_lasher_dome`'s weapon (a
  projectile created where a strike landed before), or a projectile with a mover. Allowed only in worlds WP0's probe
  lists as firing those weapons.
- **P (presentation).** A `PlayerView` or fog frame differs in images only (a launch image gained for these two, lost
  for the Carrier).
- **Anything else** (a target, order, position, damage or RNG draw not downstream of a D release): a defect, stop
  rule 2.

### 6.5 Close battles

WP0 fixes the close-battle list from its baseline alone, by the rule of the hold-facing plan §6.4 (as the
visibility plan's WP0 applied it): every matrix battle whose minority side wins at least 2 of its 20 baseline runs;
battles 31 and 33 at the same share over their 100 and 64 seeds; every battle a closed owner question or fixture
comment names close or restated; no other; a `known_bug` battle is not close. (On `main` this gave 14, 26, 31, 33,
35, 38a, 38b; WP0 recomputes it on its own tree.) The list is not edited after WP1 starts.

- A battle **on the list** whose outcome moves, with a first difference of class D, is restated under charter §8:
  its claim or comment gains the moved share, the first differing tick of one flipped seed traced, and the 5.0.14
  source of the change.
- **Any outcome move not on the list, or on the list with a first difference not of class D, is stop rule 4.**
- WP0 predicts no matrix battle fires either weapon (§7.1), so no outcome is expected to move at all.

### 6.6 Predictions (recorded before measuring, not thresholds)

| World | WP1 (latent) | WP2 (data) |
|---|---|---|
| Matrix (seeds of record and 16 development seeds) | byte-identical | byte-identical (no battle carries the three units) |
| Battles 31, 33 | byte-identical | byte-identical |
| Showcase, showcase effects (default and 1–8) | byte-identical | byte-identical unless WP0's probe finds the AI training one of the three units; then class D or P only, listed |
| Goldens, sandbox | S, proved, not re-recorded | S (the movers' bytes ride every projectile's weapon spec; `blue_spotter` fires nothing); re-recorded once in WP4 |
| Client skirmish stage log | identical | identical |

---

## 7. Measurement

### 7.1 WP0 census and probe (scratch, not committed)

- **(c) Census.** The visibility plan's resolver (`~/scratch/launch-spell-visibility/census/sc2.py`, copied to
  `~/scratch/weapon-delivery/census/`) extended to print, for every shipped weapon's launch effect, its `Movers`
  array (link, `IfRangeLTE`), each mover's phases (driver, `Speed`, `Acceleration`, `MaxSpeed`, `Outro`) resolved
  across parents and layers, and the ammo unit's own mover. It confirms §2.1 S5, S7, S8, S10, S11 and records the
  movers of B-52's remaining rows (Viking, Liberator, Banshee, Missile Turret, High Templar) for the follow-up.
- **A scratch probe patch** (`~/scratch/weapon-delivery/probe/`, with its diff; it may name slugs, being scratch)
  logs, per world and seed, without changing state: (r) releases of each of the three weapons; (t) for each, the
  edge distance at the release and the band it would choose; (k) kills by those weapons. **Predicted zero in every
  matrix battle, battles 31 and 33, both goldens and the sandbox**; the showcase and skirmish stage are measured, not
  predicted.
- **Close-battle list** (§6.5), **baseline** on four builds (both goldens and `DEFAULT_SANDBOX_CHECKSUM`),
  **throughput and instructions**.

### 7.2 Commands

The visibility plan's scripts, copied to `~/scratch/weapon-delivery/scripts/` (`measure.sh`, `compare.sh`,
`firstdiff.py`, `proof.sh`, `proofcopy.sh`, `unhash.py`, `heldout.sh`, `bench.sh`, `close.py`, `matrixdiff.py`) with
their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/weapon-delivery-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/weapon-delivery
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7).

### 7.3 Held-out seeds (WP4, confirmation only)

Once, on `main` (with the visibility plan) and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001
to 7064, the showcase and showcase effects on seeds 9 to 16. Byte-identical expected, or differences of class D or P
only in worlds WP0's probe names; anything else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. WP1's latent proof fails (a checkpoint differs with the new fields left out): stop; something other than the
   encoding moved.
2. A measured world's first difference is not of class S, D or P (§6.4), or is class D in a world WP0's probe did not
   list: stop, trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. An outcome moves that §6.5 does not allow (any battle off the close list, or a close battle whose first difference
   is not class D): stop for review, with the trace.
5. Throughput below 50 ticks/s in any scenario, or instructions above +0.5 %: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit (the Gated coordination rule): stop and report to the orchestrator.
7. The client needs a `client/src` change (D-8), or a steady-state or allocation test fails: stop.
8. W-1 or W-9 fails for a constant missile (the refactor moves a shipped missile): stop; fix the code, never the test.
9. WP0's census contradicts §2.1 (a phase, band or flag differs from S5, S7, S8, S10, S11): stop WP0; the plan's
   values are amended before WP1.

---

## 9. Client consequences

Data only (D-8): two `effects` blocks in `units.ron` and `structures.ron` change their travel, and `effects.ron`
gains `impaler_tentacle`. Tests: W-19 to W-21. No GPU test is added: the travel and impact blocks, pool and shaders
are those `effect_gpu_tests.rs` and `weapon_battle_gpu_tests.rs` already prove, and both run in WP3's package gate.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (D-9) | WP0, WP4 |
| `docs/evidence/2026-11-27-weapon-delivery.md` | New | WP0 to WP4 |
| `docs/questions/open/0129-…` | Q0129 from §15 | WP0 |
| `crates/sim_types/src/weapon.rs` | `MissileMover`, `MissilePhase`, `WeaponSpec::missile_movers`; `projectile_speed` doc; unit tests (defaults, RON forms) | WP1 |
| `crates/sim_core/src/entity.rs` | `ActiveProjectile::{mover, phase, travelled}` | WP1 |
| `crates/sim_core/src/combat_resolution.rs` (and a new private `combat_resolution/missile_flight.rs` if the module grows past readability) | Band choice at the release; the integration in `advance_projectile`; `flight_ticks` for the event; module doc | WP1 |
| `crates/sim_core/src/catalog.rs` | Raw movers parsed from RON (f32) and converted | WP1 |
| `crates/sim_core/src/catalog/validation.rs` | D-4 | WP1 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema line; the new `fields!` | WP1 |
| `crates/sim_core/tests/missile_mover_tests.rs` | New: W-2 to W-4, W-7, W-8 (WP1); W-11 to W-16 (WP2) | WP1, WP2 |
| `crates/sim_core/tests/{checksum_state_tests,weapon_catalog_rules_tests,catalog_reference_values_tests,runtime_catalog_tests}.rs` and the test files whose `WeaponSpec` literals list every field | W-5, W-6, W-10, W-17, W-18; `missile_movers: Vec::new()` in literals | WP1, WP2 |
| `assets/catalog/units.ron` | `flying_saucer` air weapon (D-1, D-2), its `effects` travel (D-8); `mothership_carrier` `launch_visibility: Hidden`; header comment naming `missile_movers` | WP2 (data), WP3 (travel) |
| `assets/catalog/structures.ron` | `thorn_lasher_dome` ground weapon, its `effects` travel | WP2, WP3 |
| `assets/catalog/effects.ron` | `impaler_tentacle` | WP3 |
| `crates/client/tests/{effect_catalog_tests,weapon_look_tests,sync_steady_state_tests,sync_allocation_count_tests}.rs` | W-19 to W-21 | WP3 |
| `crates/gate/suites.ron` | Only if the new test file is not a `fast` target (the visibility plan found none needed) | WP1 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded once (§5) | WP4 |
| `docs/plans/2026-11-27-launch-and-spell-visibility.md` | §14 pointer | WP4 |
| `docs/plans/README.md` | Row | plan, WP1, WP4 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`effects/` (spell missiles keep their own speed field; §14), every `client/src` file.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the code makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/weapon-delivery/`. GPU tests lock the GPU themselves; `cargo test` is never wrapped in `with-gpu-lock`.

### WP0: records, census, probe, baseline (XS, docs and scratch)

After the visibility plan is on `main`. D-9's WP0 rows (B-227 and B-52 restated, B-22 amended, B-228 registered,
Q0129 filed); §7.1's census, probe, close-battle list, baseline on four builds, throughput and instructions.
**Acceptance:** stop rule 9 holds; the evidence lists every world's (r), (t), (k) counts. Commit `docs(design):
restate B-227, register B-228 (plan 2026-11-27-weapon-delivery WP0)`.

### WP1: missile movers, latent (M, `sim_types`, `sim_core`)

D-2 to D-6. Red first: W-1 to W-9 (W-1 and W-9's constant halves pass before and after, as guards). Then the types,
parsing, validation, projectile fields, the integration and `flight_ticks`, and the encoding at the next free schema
after §5's latent proof. No shipped row authors movers. **Acceptance:** W-1 to W-9 pass; every measured world
byte-identical to WP0's; goldens differ by the schema only (proved, not re-recorded); stop rules 1, 6, 8 hold.
Commit `feat(sim_core): missile movers, speed phases by launch range (B-52, B-227; latent)`.

### WP2: the data (S, catalog and `sim_core` tests)

D-1, D-7. Red first: W-10 to W-18 on WP1's tree. Then the two rows' delivery and movers, the Carrier's `Hidden`, the
reference tables. Worlds measured against WP1's (§6.4 to §6.6). **Acceptance:** W-10 to W-18 pass; every world's
first difference classed; stop rules 2, 4 hold. Commit `feat(catalog): the Phoenix and Spine Crawler analogues
launch missiles (B-227)`.

### WP3: client looks and tests (S, data and `client` tests)

D-8. The two travels, `impaler_tentacle`, W-19 to W-21. **Acceptance:** they pass with no `client/src` change (stop
rule 7); the GPU and diagnostics suites green in the package gate. Commit `feat(effects): missile looks for the
Phoenix and Spine Crawler analogues`.

### WP4: confirmation, re-record, records (S)

Held-out seeds (§7.3); throughput and instructions (§5); goldens and sandbox re-recorded once on the final tree after
merging `main`, proved on four builds; D-9's WP4 rows; `## As built`. **Acceptance:** stop rules 3 and 5 hold; B-227
fixed. Commit `docs: close B-227 (plan 2026-11-27-weapon-delivery WP4)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `weapon.rs` | Defaults and RON forms of the movers |
| Unit | `sim_core` missile flight code, `catalog/validation.rs` | W-1 to W-3, W-5, W-9 |
| Integration | `crates/sim_core/tests/missile_mover_tests.rs` (new) | W-2 to W-4, W-7, W-8, W-11 to W-16 through `SimWorld` and `PlayerCommand` |
| Integration | `checksum_state_tests.rs`, `weapon_catalog_rules_tests.rs`, `catalog_reference_values_tests.rs`, `runtime_catalog_tests.rs` | W-6, W-10, W-17, W-18 |
| Determinism | goldens, sandbox, four builds | §5: latent proof, one re-record |
| Client | `effect_catalog_tests.rs`, `weapon_look_tests.rs` | W-19, W-20 |
| Steady state | `sync_steady_state_tests.rs`, `sync_allocation_count_tests.rs` (valgrind) | W-21 |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s, ≤ +0.5 % instructions |

---

## 13. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1: the throw phases' sideways path makes the game's flights longer than ours | Phoenix and short-range tentacle flights up to about 2 ticks short (**verify**) | Recorded in D-3; a later path model would change only `flight_ticks` and the step; no outside measurement exists (Q0008 searched) |
| R-2: the band distance is centre to centre in the game | Bands switch about 1.7 units (Phoenix) or 1.6 (Spine Crawler against a small unit) further out | §2.3 argues edge to edge; W-4 pins ours; a one-line change if disproved |
| R-3: the Phoenix's strikes, now delayed, change a world unexpectedly (AI armies) | A measured world moves | WP0's probe lists every world that fires it; stop rule 2 for any other |
| R-4: Spine Crawler defence weaker against melee (5–7 tick flight) | Skirmish balance shifts | It is the game's mover (S11); recorded; no matrix battle carries it |
| R-5: the `Vec` clone per launch costs throughput | Bench regression in mover-heavy fights | Only two weapons; bench budget; stop rule 5 |
| R-6: schema number taken first | Merge conflict in `fields!` and the goldens | "Next free at merge"; WP4 re-records once on the final tree |
| R-7: Q0129 answers (b) Mothership | The salvo becomes an instant beam; `Hidden` launch becomes moot | The interim value is harmless under either answer; the B-22 or B-228 plan removes it |
| R-8: the client's projectile look for the Spine Crawler reads as a needle, not a tentacle | Presentation only | Authored from the dome's own chitin texture and ramp (D-8); a bespoke look can follow without code |

---

## 14. Out of scope and decisions recorded (Q0002)

- **Carrier interceptors** (B-22): not planned here (D-7). B-227's Carrier clause is handed to B-22 with the interim
  `Hidden` launch; the identity is B-228 and Q0129.
- **B-52's other rows** (Viking, Liberator, Banshee, Missile Turret, High Templar): their movers become data changes
  on this mechanism, but each moves battles (12c, 19, 35 for the Viking) and needs its own close-battle list and
  restatements. A follow-up plan, reusing WP0's census output.
- **Spell missiles** (`LaunchMissileDef::speed`, Abduct's tongue, Fungal Growth): both are constant in 5.0.14
  (`YoinkMissile`, `FungalGrowthWeapon`, single fast or constant guidance phases), so they need no movers.
- **Negative acceleration** (D-4) and **minimum speed** (`MinSpeed`): no shipped mover uses them.
- **The tentacle's return**, missile units, and the throw path's geometry (D-3).
- **A matrix battle for the Phoenix analogue** (for example against the Mutalisk analogue): useful coverage, but a
  battle needs a sourced expectation; left to the matrix capability work.
- **Teams:** unchanged (Q0101 4(b)).

---

## 15. Owner question (filed on main as [Q0129](../questions/closed/0129-mothership-carrier-identity.md))

````markdown
# Q0129: Is the Mothership Carrier the Carrier, the Mothership, or a mix?

- **Asked:** 2026-11-27
- **Blocking:** no
- **Default until answered:** (a); until interceptors are built (bug B-22), the unit keeps its missile salvo, shows
  no image of itself when it fires, and keeps its current life, shields and cost

## Question

The Cosmic Enclave's "Mothership Carrier" names two StarCraft II units, and its values come from both. Its life and
shields (350 and 350) are the Mothership's; its cost (350 minerals, 250 gas), supply (6), build time, attack range (8),
weapon name ("Interceptor Fleet Salvo") and description ("deploying swarms of autonomous saucer interceptors") are the
Carrier's. The two attack very differently: a Carrier sends out up to 8 small interceptor craft, which it builds
itself and which can be shot down; a Mothership fires an instant beam. Today ours fires one salvo of 8 missiles,
which neither does.

- (a) **The Carrier.** Interceptors get built (bug B-22): a hangar of up to 8, built one by one, flying out to attack
  and recalled. Life and shields become the Carrier's (300 and 150). **Recommended**: most of its values, its name for
  the weapon and its description already say Carrier, and the Enclave has no other Carrier.
- (b) **The Mothership.** Its attack becomes an instant beam (6 damage, range 7), cost 400/400 and supply 8; it gains
  the Mothership's abilities later. No interceptors.
- (c) **Keep the mix** as a deliberate deviation, written down: Mothership toughness with a Carrier attack, the
  missile salvo standing in for interceptors.

Only matches with this unit change; it is in none of the test battles.

## Answer

````

---

## 16. Material (scratch, not committed)

- Game data `~/scratch/combat_wp3_data/` (`5444e12f13`): `liberty` WeaponData:184–193, :365–375, :513–526, :601–637;
  `liberty` EffectData:893–902, :957–963, :1940–1989, :2151–2167; `libertymulti` EffectData:117–133, WeaponData:15–20;
  `swarm` EffectData:2251–2263, WeaponData:172–181; `swarmmulti` WeaponData:148–156; `voidmulti` EffectData:427–429,
  :1532–1537, :2262–2283, :2665–2675, AbilData:1296–1300; `balancemulti` AbilData:1133–1137; `liberty`
  MoverData:243–278, :446–484; `core` MoverData:13–25, EffectData:95–109, :131–133; `liberty` AbilData:2083–2098;
  `swarm` AbilData:467–469; `liberty` UnitData:3469–3601, :4244–4316, :4718–4721, :8731–8734;
  `units_dump_lotv.txt` for the Carrier, Interceptor and Mothership.
- Outside source (Q0008), 2026-11-25: S12, the editor guide's *Missile Movers* page. No source found on whether a
  phase without a driver keeps the previous driver, or on the band distance's measure; hence **verify**.
- Planning script `~/scratch/weapon-delivery/flight.py` (copy of the session scratch script): D-2's integration over
  §2.3's distances; WP1's W-2 reproduces it in fixed point.
- Code read on `2f922a50` and `7afceb2a`: `combat_resolution.rs` (`dispatch_shot`, `launch_projectile`,
  `advance_projectiles`, `advance_projectile`, `track_projectile_target`, `resolve_projectile_impact`), `entity.rs`
  (`ActiveProjectile`), `sim_types/src/weapon.rs` (`WeaponSpec`, `MissileValidation`, `ProjectileTargetLossBehavior`,
  `AttackVisibility`), `catalog/validation.rs` (weapon checks), `checksum/encoding.rs`, `client/src/effects/systems.rs`
  (`travel_request`, the impact delay), `client/tests/effect_catalog_tests.rs` (`delivery`), the three rows and their
  `effects` blocks, `upgrades.ron` (`anion_pulse_crystals`, +2 range), the throughput scenarios, both goldens'
  squads, the visibility plan and its evidence.
- No prototype: the integration is four lines and is pinned by W-2.

---

## As built

Built on branch `weapon-delivery-plan` as written (WP0 to WP4); measurements in
[`docs/evidence/2026-11-27-weapon-delivery.md`](../evidence/2026-11-27-weapon-delivery.md).

| WP | Commit | Result |
|---|---|---|
| WP0 | `4cde25ea` | B-227 and B-52 restated, B-22 amended, B-228 registered (Q0129 was already filed on `main`); the mover census confirms §2.1 (stop rule 9 holds); the probe, with a positive control, finds no release of the three weapons in any measured world, the skirmish scenario included; baseline 8/8 at schema 53; close battles 14, 26, 31, 33, 35, 38a, 38b |
| WP1 | `16934087` | `MissileMover`, `MissilePhase`, `WeaponSpec::missile_movers`, `ActiveProjectile::{mover, phase, travelled}`; new private module `combat_resolution/missile_flight.rs`; validation D-4; hashed at schema 56 (provisional: 54 and 55 reserved for the Gated plan); latent proof 8/8; worlds byte-identical. C7's float allowances of `catalog.rs` and `validation.rs` rose by five each (`workspace_checks/tests/charter.rs`, authored decimals) |
| WP2 | `0cf9656b` | The two rows and the Carrier's `Hidden` launch; §2.3's flight ticks measured exactly at every distance (W-11, W-12); worlds byte-identical to WP1's, so no world is classed and no close battle restated |
| WP3 | `c788b407` | `plasma_bolt` for the Phoenix analogue, new `impaler_tentacle` for the Spine Crawler analogue; no `client/src` change |
| merge | `1943995d` | `main` `39df7177` (Q0129 answered (a)); latent proof on the merged tree 8/8 |
| WP4 | `3518b58d`, this commit | Goldens and sandbox re-recorded at schema 56 and proved 8/8; final worlds and held-out seeds byte-identical to `main`'s; every bench scenario ≥ 51 ticks/s under heavy load, instructions −2.09 %; B-227 fixed, B-52 narrowed |

**Deviations.** W-21 could not stage a Phoenix-analogue volley in the client harness (`flying_saucer` has no flipbook
media); the steady-state and allocation tests pass unchanged with the shipped data. W-13's air target has no "loaded"
case (an air unit boards nothing). W-14 clears the random delay on a scratch copy. The skirmish scenario was added to
the measured worlds as §7.1's "skirmish stage".

**Owner answer.** Q0129 was answered (a), the Carrier, during WP4. This plan built that default (the salvo stays a
missile with a `Hidden` launch); the Carrier's vitals (300/150) and the interceptor hangar are B-22's own package, and
the B-22 and B-228 rows say so. Q0129 is closed by the orchestrator.

