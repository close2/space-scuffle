# Blueprint: Missile Movers for the Viking, Liberator, Banshee, Missile Turret and High Templar (B-52)

This plan fixes what remains of **B-52** (`DESIGN.md` §4) after plan
[`2026-11-27-weapon-delivery`](2026-11-27-weapon-delivery.md) built `WeaponSpec::missile_movers` and fixed the
Phoenix and Spine Crawler halves:

- **B-52:** "Projectile flight times of multi-phase movers: the Viking (`VikingFighterMissile`), Liberator
  (`LiberatorMissile`), Banshee (`BacklashRocketsLMWeapon`), Missile Turret (`LongboltMissileWeapon`) and High
  Templar (`HighTemplarWeaponMover`, voidmulti EffectData:1536, a `MissileDefault` child) analogues fly one constant
  speed. The mechanism exists since plan `2026-11-27-weapon-delivery` …; each remaining row is a data change that
  moves battles (12c, 19, 35 for the Viking), its movers recorded in that plan's evidence."

The design in one paragraph: four rows become data on the existing mechanism, and one needs a small extension. The
Viking, Liberator and Banshee analogues author `missile_movers` read from their 5.0.14 movers. The High Templar's
mover is `MissileDefault`'s constant 18.75, so its row needs no movers, only the right constant (1.05 per tick, not
1.0). The Missile Turret's mover ends its first phase **4 units before the target** (`Outro -4`), a target-relative
end the mechanism lacks: its phases end only after a path flown from the launch. So a phase's end becomes a typed
`PhaseEnd` (`Flown` or `Remaining`), the game's own sign rule for `Outro` (positive from the source, negative from the
target), checked after each step and once at the release. That extension is hashed (schema next free at merge); every
shipped flight is unchanged by it. The packages go one unit family at a time, because each reaches different battles:
the High Templar moves battles 13, 36a and 36b by at most a tick per bolt, the Viking moves 12c, 19 and 35 by eight to
ten ticks per missile, and the Liberator, Banshee and Missile Turret are predicted to reach no measured world. Two
neighbouring deviations found while planning are registered and deferred: **B-231** (the throw phases' direction is
not modelled) and **B-232** (the Brood Lord analogue's broodling missiles fly an unsourced constant).

**Status of this document:** a stage 1 blueprint under charter §6, written against `main` at `4ff8402f` (checksum
schema 57).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- **Numbers are provisional, next free at merge.** New bug rows start at **B-231** (B-231 and B-232 here). No owner
  question is drafted (§15). The checksum schema is the next free number at merge: `main` is at 57, the Gated plan
  takes the next ones (58 and 59 if it keeps two steps), so this plan takes **60** if those land first. Every schema
  mention below reads "next free at merge".

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S11 |
| Q0045 (align catalog times to SC2), Q0057 (ranges measured edge to edge) | Closed | D-2, D-3 |
| Q0103: natural movement outweighs close matchup flips | Standing guidance | §6.6 (close battles) |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Gated plan coordination rule (`2026-11-27-gated-movement` §11): no other package edits `movement/`, `steering.rs`, acquisition's admission test or `movement_rules.ron` while a Gated package is open | Standing for this plan | §5, stop rule 6 |
| Open on `main` at writing: Q0130 (interceptors hover), Q0131 (Carrier media) | Unrelated | — |

---

## 1. Summary

| | Today (`4ff8402f`) | After this plan |
|---|---|---|
| Viking analogue (`vector_interceptor`) Sky Torpedo Volley | Constant 1.5 per tick: 1 to 7 ticks over edge 0.5 to 9 | `VikingFighterMissile`: 0.112 per tick for the first 0.6875 of path, then from 0.112 accelerating 0.175616 per tick² to 1.568: **9 to 17 ticks** |
| Liberator analogue (`sky_bastion`) Lexington Rockets | Constant 1.2: 2 to 5 ticks over edge 0.5 to 5 | `LiberatorMissile`: 0.448 for the first 3.0 of path, then 0.896: **3 to 10 ticks** |
| Banshee analogue (`cloak_gunship`) Backlash Thermite Rockets | Constant 1.2: 2 to 6 ticks over edge 0.5 to 6 | `BacklashRocketsLMWeapon`: from 0.56 accelerating 0.06272 to 2.8: **3 to 9 ticks** |
| Missile Turret analogue (`flak_array_dome`) Longbolt AA Missiles | Constant 0.56: 3 to 15 ticks over edge 0.5 to 7 | `LongboltMissileWeapon`: 0.56 until 4 units of path remain, then accelerating 0.6272 to 1.12: **2 to 12 ticks** |
| High Templar analogue (`cerebral_elder`) Psi-Bolt | Constant 1.0 | Constant **1.05** (`MissileDefault`, no movers): the same tick or one earlier |
| Mechanism | A phase ends after a path flown (`ends_after`) | A phase ends after a path flown or when a path remains (`ends: Option<PhaseEnd>`), also checked at the release; every shipped flight byte-identical |
| Hashed state | — | `MissilePhase::ends` replaces `ends_after` in the encoding (a variant tag added); schema next free at merge. No new projectile field |
| Worlds | — | Predicted: battles 13, 36a, 36b (High Templar), 12c, 19, 35 (Viking) move by their missiles' flight times; outcomes predicted unchanged except possibly 35 (close, restated); no other measured world moves |
| Records | B-52 open | B-52 fixed; B-231 and B-232 registered, deferred |
| Size | — | M: WP0 (records, census, probe, baseline), WP1 (target-relative ends, latent), WP2 (High Templar), WP3 (Viking), WP4 (Liberator, Banshee), WP5 (Missile Turret), WP6 (confirmation, re-record, records) |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [EXT] an outside source (Q0008); **verify** not
confirmed. Conversions as everywhere in the catalog (Q0045): a Normal speed `v` is `v × 1.4 / 25` world units per
tick (`0.056 v`); a Normal acceleration `a` is `a × 1.4² / 25²` per tick squared (`0.003136 a`); a Normal time `t`
is `t × 25 / 1.4` ticks.

### 2.1 Sources

`L Kind:N` is line N of layer L's file in SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at
`~/scratch/combat_wp3_data/<layer>/`. The census script of the weapon-delivery plan
(`~/scratch/weapon-delivery/census/movers.py`, output `movers.txt`) resolved every mover below across the eight layers
(last layer wins, parents followed, `MotionPhases` merged by index); WP0 reruns it with line numbers (§7.1).

| # | Source | What it gives |
|---|---|---|
| S1 | Viking `LanzerTorpedoes`: `liberty WeaponData:266–275` (`Range 9`, `Period 2`, `DisplayAttackCount 2`, `AllowedMovement Slowing`), `voidmulti WeaponData:133–135` (`DamagePoint 0.05`); persistent `LanzerTorpedoes`, `liberty EffectData:1698–1706` (`PeriodCount 2`, `PeriodicEffectArray LanzerTorpedoesLM` twice, `PeriodicPeriodArray 0, 0.21`) | Two missiles, 0.21 Normal s (3.75 → 4 ticks) apart: our row's `attacks_count 2`, `strike_interval_ticks 4`, range 9, period 36, damage point 1 already match [DATA] |
| S2 | `LanzerTorpedoesLM`, `liberty EffectData:1707–1712`: `AmmoUnit VikingFighterWeapon`, `ValidatorArray RangeCheckLE15`, no `Movers`, no flag or `Visibility` override; the ammo unit, `liberty UnitData:6901–6905`: `Mover VikingFighterMissile` | The missile flies the ammo unit's mover at any range [DATA] |
| S3 | `VikingFighterMissile`, `liberty MoverData:531–567` (defined in no other layer): phase 0 `Throw`, `Speed 2`, `MaxSpeed 2`, `Outro 0.125,0.25`, `ThrowVector 0,-1,0`; phase 1 `Throw`, `Speed 2`, `MaxSpeed 2`, `Gravity 30`, `Outro 0.625,0.75`, `ThrowVector 0,-1,0`; phase 2 `Guidance`, `Speed 2`, `Acceleration 56`, `MaxSpeed 28`, `Outro 2,2.5`; phase 3 `Guidance`, `MaxSpeed 28` | Dropped at 2 for about 0.69 units, then guided from 2, accelerating at 56 to 28 [DATA; phase 3 keeping the acceleration is S11's carry-over rule, INF] |
| S4 | Liberator `LiberatorMissileLaunchers`, `void WeaponData:321–330`: `Period 1.8`, `DisplayAttackCount 2`, `Effect LiberatorMissileBurstPersistent` (`void EffectData:2942–2970`: `PeriodCount 2`, `LiberatorDamageMissileLM` twice, one `PeriodicPeriodArray 0.05`) | Two missiles 0.05 s apart; our row's 2 strikes, interval 1, period 32 already match (plan `2026-11-23-strike-delays` D-4) [DATA] |
| S5 | `LiberatorDamageMissileLM`, `void EffectData:2979–2983`: `LaunchLocation CasterUnit`, `ImpactEffect LiberatorMissileDamage`, `AmmoUnit LiberatorDamageMissile`, no `Movers`; the ammo unit, `void UnitData:3411–3415`: `Mover LiberatorMissile` | The ammo unit's mover at any range [DATA] |
| S6 | `LiberatorMissile`, `void MoverData:384–422`: phase 0 `Throw`, `Speed 1`, `Acceleration 3200`, `MaxSpeed 8`, `Outro 2.5,3.5`, `ThrowVector 0,-1,0`, `ThrowBandYaw 74.99,−74.99,64.995,−64.995`, `ThrowBandPitch 45,−45,5,−5`; phase 1 `Guidance`, `Acceleration 3200`, `AccelerationRange 7`, `MaxSpeed 16`, `Outro −2.5,−1.5`; phase 2 `Guidance` (no speed value) | Thrown at 8 (3200 reaches it in under a tenth of a tick) for 3 units, then 16 to the target; phase 2 carries 16 on, so the target-relative end of phase 1 changes no speed [DATA; "changes no speed" INF from S11] |
| S7 | Banshee `BacklashRockets`, `liberty WeaponData:548–562` (`Range 6`, `Period 1.25`, `DisplayAttackCount 2`); persistent `liberty EffectData:2550–2557`, `voidmulti EffectData:36–39` (`PeriodicPeriodArray` index 0 set to 0, then 0.15) | Two missiles 0.15 s (2.68 → 3 ticks) apart; our row's 2 strikes, interval 3, period 22 already match [DATA] |
| S8 | `BacklashRocketsLM`, `liberty EffectData:2558–2563`: `Movers Link BacklashRocketsLMWeapon` (no `IfRangeLTE`), no `AmmoUnit`; `BacklashRocketsLMWeapon`, `liberty MoverData:485–510`: phase 0 `Guidance`, `Speed 10`, `Acceleration 20`, `MaxSpeed 50`, `Outro 2.5`; phase 1 `Outro −1` only; phase 2 no speed value | One unbounded band; guided from 10, accelerating at 20 to 50 for the whole flight (phases 1 and 2 carry phase 0's values on, S11) [DATA; carry-over INF] |
| S9 | Missile Turret `LongboltMissile`, `liberty WeaponData:300–309` (`Range 7`, `Period 0.8608`, `DisplayAttackCount 2`); persistent `liberty EffectData:1779–1788` (`Channeled`, `PeriodicPeriodArray 0, 0.1`); `LongboltMissileLM` (:1789–1794): `AmmoUnit LongboltMissileWeapon`, no `Movers`; the ammo unit `liberty UnitData:11605–11608` names no `Mover`, so the default `##id##` (`core`) | Two missiles 0.1 s (1.79 → 2 ticks) apart; our row's 2 strikes, interval 2, period 15 already match; the mover is `LongboltMissileWeapon` [DATA] |
| S10 | `LongboltMissileWeapon`, `liberty MoverData:589–600`: phase 0 `Guidance`, `Speed 10`, `MaxSpeed 10`, `Outro −4`; phase 1 (comment "Accelerating 'kill phase'") `Acceleration 200`, `MaxSpeed 20` | Guided at a constant 10 until 4 units from the target, then accelerating at 200 (one tick to the cap) to 20 [DATA] |
| S11 | SC2 editor guides, *Missile Movers* (<https://s2editor-guides.readthedocs.io/Classic_Tutorials/04_Misc/missile-movers/>, read 2026-11-26): "Source-relative outros are positive and target-relative outros are negative"; a negative *Blend At* "means the blend starts when the missile is that distance from the target"; "To minimize Mover data entry, data from one motion phase automatically carries over to the next phase, unless it is irrelevant or overridden there"; a phase's actual speed usually carries over, but not into ballistic phases; the `Movers` range is "the distance between the launcher and the target" | The sign rule of `Outro`; a phase that sets no `Acceleration` or `MaxSpeed` keeps the previous phase's [EXT] |
| S12 | High Templar `HighTemplarWeapon`, `voidmulti WeaponData:284–294`, `balancemulti WeaponData:257–267` (`Range 6`, `Period 1.754`, `Effect HighTemplarWeaponLM`); `HighTemplarWeaponLM`, `voidmulti EffectData:1532–1537`, `balancemulti EffectData:1404–1409`: `Movers Link HighTemplarWeaponMover` (no `IfRangeLTE`); `HighTemplarWeaponMover` `parent="MissileDefault"` with no field of its own (`voidmulti MoverData:47`, `balancemulti MoverData:47`); `MissileDefault`, `core MoverData:13–25`: `Guidance`, `Acceleration 3200`, `MaxSpeed 18.75`, `Outro −1`, then a phase with no speed value | A constant 18.75 (3200 reaches it within a tenth of a tick, and the kill phase carries it on): 1.05 per tick, the value nine shipped `MissileDefault` weapons already fly [DATA] |
| S13 | High Templar spells: `PsiStorm` (`liberty AbilData:2059–2071`) runs `PsiStormPersistent` (`liberty EffectData:2069–2076`), a persistent at the point with `PsiStormSearch` periods; `Feedback` (`liberty AbilData:570–583`) runs `FeedbackSet` (`liberty EffectData:679–684`: `Feedback` damage and `FeedbackEnergyLoss`) | Neither launches a missile; Feedback is not shipped. No spell of the analogue needs movers [DATA] |
| S14 | Census of every other shipped missile launch (`~/scratch/weapon-delivery/census/movers.txt`) | Every other missile weapon flies a constant its row already carries (1.05 `MissileDefault`, 1.12 `PunisherGrenadesWeapon`, 2.24 at 40, 1.008 `PhotonCannonWeapon`), except the Brood Lord's broodling escort (S15) [DATA] |
| S15 | Brood Lord `BroodlingEscortLaunchA`, `liberty EffectData:2664–2667`, and the escort missiles (:2741–2748, :2774–2779): `Movers BroodLordWeaponRight` / `Left` `IfRangeLTE 500`; movers `liberty MoverData:44`, :620 (`Throw`, `Speed 5`, `Acceleration 3`, `MaxSpeed 8`, `Outro 1.5,4.5`; `Guidance`, `Acceleration 20`, `MaxSpeed 40`) | Our `spore_matriarch` flies a constant 0.9 with no source: registered as B-232, deferred (§14) [DATA] |

### 2.2 The rules that follow

1. **Each row's mover.** None of the five launches has a bounded band: the Viking, Liberator and Missile Turret fly
   their ammo unit's mover, the Banshee and High Templar their launch effect's single unbounded `Movers` entry (S2,
   S5, S8, S9, S12). So each row authors one mover with `up_to_range: None`, or none at all for a constant.
2. **Carry-over** (S11). A phase that sets no `Speed` keeps the speed it has; one that sets no `Acceleration` or
   `MaxSpeed` keeps the previous phase's. So phases that change nothing in the speed profile merge: the Viking's
   phases 0 and 1 (both 2), and 2 and 3 (phase 3 keeps `Acceleration 56` and sets the same `MaxSpeed 28`); the
   Liberator's phases 1 and 2; the Banshee's three phases into one. The weapon-delivery plan's Phoenix merge (its
   phases 0 and 1) is the same rule.
3. **An `Outro` sign picks the reference** (S11). Positive: the phase ends after that path flown from the launch (the
   mechanism's existing rule). Negative: it ends when the missile is that distance from the target. Only the Missile
   Turret's −4 changes a speed (S10); the Liberator's −2.5,−1.5 and the Banshee's −1 sit between phases that merge
   (rule 2), so they are not authored.
4. **3200 is instant**, as `MissileDefault`'s is (weapon-delivery S8): the Liberator's phases start at their caps (8,
   then 16), authored as constant phases with `speed` set, as the Phoenix's ≤ 1 band is.
5. **The path's shape is not modelled** (weapon-delivery §2.2 rule 6): only the flight time decides anything. The
   throws' direction (`ThrowVector`, `ThrowBandYaw`, `Gravity`) is therefore lost; that error is B-231 (§4 D-7).
6. **The High Templar's bolt is a constant** (S12): its row stays a constant missile; only the value changes.
7. **Strike structure already matches** (S1, S4, S7, S9): every row's strike count, interval, period and damage point
   equal the game's; this plan changes speeds only.

### 2.3 Findings and confidence

| Finding | Confidence | Basis |
|---|---|---|
| Each row's mover and phases as S3, S6, S8, S10, S12 | High | [DATA], resolved across layers by the census |
| Unset `Acceleration`/`MaxSpeed` carry on to the next phase | Medium-high | S11 [EXT] states it as the general rule; no field is named. Matters for the Viking: without it, phase 3 would hold the speed reached at 2.25 (13.4), giving 17 to 20 ticks at edge 6 to 9 instead of 15 to 17 (the alternative reading of §2.4); for the Banshee it changes no tick at edge ≤ 6 |
| A negative `Outro` measures to the target | High | S11 [EXT]; the Turret's comment "Accelerating 'kill phase'" agrees |
| The distance to the target, for a negative end, is our remaining path to impact (centre distance less the target's radius) | Our choice (D-3) | The game measures to the impact point in 3D; ours impacts at the target's edge, so "what is left to fly" is the same quantity the impact test reads |
| Speeds and accelerations converted | High | Arithmetic |
| Throw direction ignored | Low for the Liberator, medium for the Viking | Rule 5; B-231 bounds the error (D-7) |
| Flight ticks of §2.4 | Medium (follow from the rows above; fixed point may move a boundary by one tick) | `~/scratch/missile-movers-b52/flight.py` (§16); WP3 to WP5 pin them in the shipped data |

### 2.4 Flight ticks (planning script, release tick not counted)

The path a missile flies in the simulation is the edge distance plus the attacker's collision radius (the missile
starts at the attacker's centre and lands at the target's edge). Turret radius 1.125 (**verify** that
`launch_projectile` reads the structure's radius, not the 0.5 fallback).

| Row (attacker radius) | Edge distance | Today | 5.0.14 | Alternative reading |
|---|---|---|---|---|
| Viking (0.75) | 0.5 / 2 / 4 / 6 / 8 / 9 | 1 / 2 / 4 / 5 / 6 / 7 | **9 / 12 / 14 / 15 / 16 / 17** | no acceleration carried into phase 3: 9 / 12 / 14 / 17 / 19 / 20 |
| Liberator (0.75) | 0.5 / 1 / 2 / 3 / 4 / 5 | 2 / 2 / 3 / 4 / 4 / 5 | **3 / 4 / 7 / 8 / 9 / 10** | — (throw direction: B-231) |
| Banshee (0.75) | 0.5 / 1 / 2 / 3 / 4 / 5 / 6 | 2 / 2 / 3 / 4 / 4 / 5 / 6 | **3 / 3 / 5 / 6 / 7 / 8 / 9** | no acceleration carried: the same |
| Missile Turret (1.125) | 0.5 / 1 / 2 / 3 / 4 / 5 / 6 / 7 | 3 / 4 / 6 / 8 / 10 / 11 / 13 / 15 | **2 / 3 / 4 / 5 / 7 / 8 / 10 / 12** | phase 0 only (no target-relative end): today's column |
| High Templar (0.375) | 0.5 to 6 | `ceil(path / 1.0)` | `ceil(path / 1.05)`: one tick earlier on 105 of 601 edges on a 0.01 grid (e.g. 0.63–0.67, 5.88–5.92), the same elsewhere | — |

---

## 3. Diagnosis (on `4ff8402f`, read from the code)

1. **The rows.** `vector_interceptor` `air_weapon` `projectile_speed: Some(1.5)`, `sky_bastion` `air_weapon`
   `Some(1.2)`, `cloak_gunship` `weapon` `Some(1.2)`, `flak_array_dome` `air_weapon` `Some(0.56)` (phase 0's 10 alone),
   `cerebral_elder` `weapon` `Some(1.0)`; none authors `missile_movers`. The catalog-time audit (`docs/evidence/
   2026-11-04-catalog-time-audit.md` §8) classes the first four E ("no single speed") and the fifth unsourced.
2. **The mechanism** (`crates/sim_core/src/combat_resolution/missile_flight.rs`): `choose_mover` by edge distance;
   `launch_speed`; `MoverTick::of` (step the mean of the tick's end speeds); `enter_due_phases(mover, phase,
   travelled, speed)` advances while `current.ends_after.is_some_and(|end| travelled >= end)`; `flight_ticks` for the
   renderer event. `launch_projectile` (`combat_resolution.rs`) chooses the mover and starts at `launch_speed`; it does
   not call `enter_due_phases`. `advance_projectile` calls it after each step with `proj.travelled`.
3. **The type** (`crates/sim_types/src/weapon.rs`): `MissilePhase { speed, acceleration, max_speed, ends_after:
   Option<SimScalar> }`; raw `MissilePhaseDef` in `catalog.rs` (f32, no `deny_unknown_fields`, so a misspelt field
   is silently ignored); validation in `catalog/validation.rs` (an end on every phase but the last, positive, strictly
   increasing; at most 4 movers of 4 phases; `projectile_speed` equal to the greatest `max_speed`).
4. **Encoding** (`checksum/encoding.rs`): `fields!(MissilePhase { speed, acceleration, max_speed, ends_after })`,
   hashed inside `WeaponSpec` with each projectile. The schema number is itself encoded (`encoding.rs`, the header),
   so any schema step changes every checkpoint.
5. **A target-relative end cannot be expressed.** The Turret's `Outro −4` depends on where the target is, not on the
   path flown, and the target may move; no `ends_after` value is right for every launch distance.
6. **Tests that find "the" mover weapon by property** (`missile_mover_tests.rs`: `air_mover_unit` takes the first
   mobile air unit whose air weapon has movers, meant to be the Phoenix analogue) will find the Viking or Liberator
   analogue too once they gain movers, depending on catalog order. They must be narrowed by property (§6.3 M-14).
7. **Where the five units appear** (read from the files at `4ff8402f`): `matchup_matrix.ron` battles 12c, 19, 35
   (`vector_interceptor`) and 13, 36a, 36b (`cerebral_elder`); the throughput scenario "64x64, 67 casters" spawns
   `cerebral_elder` casters; the Starport analogue's `trains_units` names the Viking, Liberator and Banshee analogues
   (the AI may train them in the showcase and skirmish worlds); no golden, sandbox or scenario file names any of the
   five. WP0's census and probe measure what fires (§7.1).

---

## 4. Design decisions

### D-1. The rows

| Row | Field | Today | After |
|---|---|---|---|
| `cerebral_elder` `weapon` | `projectile_speed` | `Some(1.0)` | `Some(1.05)` (S12) |
| `vector_interceptor` `air_weapon` | `projectile_speed` | `Some(1.5)` | `Some(1.568)`, the top speed |
| | `missile_movers` | — | `[(up_to_range: None, phases: [(speed: Some(0.112), acceleration: 0.0, max_speed: 0.112, ends: Some(Flown(0.6875))), (speed: Some(0.112), acceleration: 0.175616, max_speed: 1.568, ends: None)])]` |
| `sky_bastion` `air_weapon` | `projectile_speed` | `Some(1.2)` | `Some(0.896)` |
| | `missile_movers` | — | `[(up_to_range: None, phases: [(speed: Some(0.448), acceleration: 0.0, max_speed: 0.448, ends: Some(Flown(3.0))), (speed: Some(0.896), acceleration: 0.0, max_speed: 0.896, ends: None)])]` |
| `cloak_gunship` `weapon` | `projectile_speed` | `Some(1.2)` | `Some(2.8)` |
| | `missile_movers` | — | `[(up_to_range: None, phases: [(speed: Some(0.56), acceleration: 0.06272, max_speed: 2.8, ends: None)])]` |
| `flak_array_dome` `air_weapon` | `projectile_speed` | `Some(0.56)` | `Some(1.12)` |
| | `missile_movers` | — | `[(up_to_range: None, phases: [(speed: Some(0.56), acceleration: 0.0, max_speed: 0.56, ends: Some(Remaining(4.0))), (speed: None, acceleration: 0.6272, max_speed: 1.12, ends: None)])]` |

Derivations (Normal value × 0.056 per tick, × 0.003136 per tick²): Viking `Speed 2` → 0.112, `Acceleration 56` →
0.175616, `MaxSpeed 28` → 1.568; the first end is the midpoint of phase 1's `Outro 0.625,0.75` (phase 0's end at
0.1875 changes no speed, rule 2), and phase 2's end at 2.25 is dropped because phase 3 carries its values. Liberator
`MaxSpeed 8` → 0.448, `16` → 0.896, end the midpoint of `Outro 2.5,3.5`. Banshee `Speed 10` → 0.56, `Acceleration
20` → 0.06272, `MaxSpeed 50` → 2.8. Turret `Speed 10` → 0.56, `Acceleration 200` → 0.6272, `MaxSpeed 20` → 1.12,
`Outro −4` → `Remaining(4.0)` (a single value: blend and stop at 4). High Templar `MaxSpeed 18.75` → 1.05.

Each changed field carries its citation comment, in the style of the Phoenix's (`units.ron`, "5.0.14 IonCannonsLMLeft
… Plan 2026-11-27-weapon-delivery D-2, D-3 (B-52)"), naming this plan, the sources of §2.1 and the merges of rule 2.
Nothing else of any row changes (§2.2 rule 7).

### D-2. Target-relative phase ends: the extension

In `crates/sim_types/src/weapon.rs`, `MissilePhase::ends_after` is replaced by:

```rust
/// Where the phase ends and the next begins (`Outro`, plan `2026-11-28-missile-movers-b52` D-2):
/// `None` on the last phase and only there.
pub ends: Option<PhaseEnd>,

/// The end of a missile phase: StarCraft II's `Outro`, whose sign picks the reference point
/// ("source-relative outros are positive and target-relative outros are negative").
pub enum PhaseEnd {
    /// The phase ends once the missile has flown this path since its release, world units
    /// (a positive `Outro`; the midpoint of its blend, weapon-delivery D-3).
    Flown(SimScalar),
    /// The phase ends once the missile's remaining path to its impact is at most this many
    /// world units (a negative `Outro`, its magnitude). The remaining path is the centre
    /// distance to the aim point less the target's radius, the quantity the impact test reads.
    Remaining(SimScalar),
}
```

RON: `ends: Some(Flown(1.25))`, `ends: Some(Remaining(4.0))`. The raw `MissilePhaseDef` mirrors it with `f32`
(`PhaseEndDef`), and `MissileMoverDef` and `MissilePhaseDef` gain `#[serde(deny_unknown_fields)]`, so a stale
`ends_after` is refused instead of read as `None`.

**The rule.** `enter_due_phases` takes the remaining path besides the path flown:

```
while the phase has a next one and its end is due:
    Flown(x):     travelled ≥ x
    Remaining(x): remaining ≤ x
  enter the next phase; if it sets `speed`, restart at it
```

- **After each step** (`advance_projectile`): `remaining = |target_pos − current_pos| − target_radius` after the move,
  with the aim point and radius `track_projectile_target` returned this tick. The target may move; the end follows it,
  as the game's does.
- **At the release** (`launch_projectile`, new): `enter_due_phases` runs once with `travelled = 0` and the launch's
  remaining path, so a Turret missile launched within 4 of its target starts in its kill phase. A `Flown(x)` end with
  `x > 0` (validation) is never due at the release, so every shipped mover starts exactly as today.
- **`flight_ticks`** (renderer event, presentation only): `remaining = path − travelled` over the event's path, a
  stationary target; also checked once before the first tick.

**Why a typed end, not a signed number.** A signed `ends_after` (negative = from the target, the game's own encoding)
would change no encoded byte and so no schema, but it makes one number mean two things, its validation a case split
on the sign, and its name wrong for half its values; the charter asks for the sound type. An additive second field
(`ends_within`) needs an "exactly one of two" invariant the type does not carry. Rejected both; the enum costs one
variant tag in the encoding and one schema step, which the re-record of WP6 absorbs.

**Why not approximate the Turret with bands.** A `Flown` end per launch band (`band − 4`) is wrong by up to a band's
width and wrong whenever the target moves; it would be an unsourced constant of the kind B-52 removes.

### D-3. Approximations, each recorded

| Game | Ours | Why / effect |
|---|---|---|
| A negative `Outro` measures to the target's impact point in 3D | The remaining path to the target's edge | The impact test's own quantity; consistent with our arrival rule (weapon-delivery D-3) |
| Phases blend between *Blend At* and *Stop At* | Switch at the midpoint (weapon-delivery D-3) | Unchanged |
| 3200 accelerations | Instant (phases start at their caps) | Under a tenth of a tick (rule 4) |
| Throw direction and gravity | Counted as progress towards the target | B-231 (D-7) |
| Two missiles left and right, or two in a burst | One projectile per strike, as today | Unchanged strike structure (rule 7) |

### D-4. Validation (`catalog/validation.rs`)

The weapon-delivery D-4 rules hold with `ends` for `ends_after`, refused with a typed `CatalogError` naming the field
(`….phases[i].ends`):
- an end missing on a phase that is not the last, or present on the last (unchanged);
- `Flown(x)` or `Remaining(x)` with `x ≤ 0`;
- consecutive `Flown` ends not strictly increasing (unchanged); consecutive `Remaining` ends not strictly decreasing;
- a `Flown` end after a `Remaining` end ("the path flown cannot be ordered after a distance to the target"; no 5.0.14
  mover the catalog ships does it, and the order of two such switches would depend on the launch distance).

### D-5. Hashing and the schema

| Value | Hashed? | Why |
|---|---|---|
| `MissilePhase::ends` (option tag, variant tag, value) in place of `ends_after` (option tag, value) | **Yes** | It decides impact ticks; `WeaponSpec` is hashed in full with each projectile |
| Catalog data (the new rows) | Not as such | Catalog data is not hashed; a row's `WeaponSpec` enters the checksum only inside a projectile of that weapon, so a world changes only where the weapon fires |
| `ActiveProjectile` | Unchanged | `mover`, `phase`, `travelled` suffice; the remaining path is recomputed from positions |

`checksum.rs` gains: "Schema N encodes each missile phase's end as `MissilePhase::ends`, an option tag, then a variant
tag (`Flown` 0, `Remaining` 1) and its value, in place of `ends_after` (plan `2026-11-28-missile-movers-b52` D-2,
D-5). The number is the next free at merge." The schema test in `encoding.rs` (`assert_eq!(…, 57)`) moves with it.

### D-6. Packaging by reach

One package per unit family, because each reaches different worlds and the comparison of each package against the
previous one must attribute every moved world to one family:

| Package | Rows | Predicted reach (WP0 measures it) |
|---|---|---|
| WP2 | High Templar | Battles 13, 36a, 36b; the 67-caster throughput scenario |
| WP3 | Viking | Battles 12c, 19, 35; showcase or skirmish only if WP0's probe finds the AI training the Viking |
| WP4 | Liberator, Banshee | None measured (showcase or skirmish as WP0 finds); if WP0 finds the two reaching different worlds, WP4 splits into WP4a (Liberator) and WP4b (Banshee) |
| WP5 | Missile Turret (needs WP1) | None measured (the skirmish only if the AI builds turrets) |

### D-7. B-231: the throw direction (registered, deferred)

**B-231** (provisional): "Missile throw phases fly towards the target in our model; 5.0.14 throws them along
`ThrowVector` with yaw and pitch bands and gravity (`void MoverData:384–394` the Liberator's `ThrowBandYaw` 65°–75°,
`liberty MoverData:531–550` the Viking's drop with `Gravity 30`, :446–484 the Phoenix's, :243–265 the short Spine
Crawler tentacle's). Flight times are therefore lower bounds: by at most the throw's length at the guidance speed
plus the turn back, about 1 tick for the Viking, the Phoenix and the tentacle, and up to about 7 ticks for the
Liberator (3 units thrown, at worst away from the target, at 0.896). Deferred: the path's shape decides nothing else
(attack missiles home; plan `2026-11-05-sc2-deviation-fixes` §8 narrowed B-52 to flight times), and the frame of
`ThrowVector` is not documented (Q0008 searched, 2026-11-26)." It replaces weapon-delivery R-1's "verify" with a
tracked row.

### D-8. The client

Presentation follows from data: the renderer event's flight time already comes from `flight_ticks`, and the travel
looks (`rocket_trail`, `plasma_bolt`) last `FlightTime` (`effects.ron`). No `client/src` change and no new look is
expected; `effect_catalog_tests`, `weapon_look_tests` and the steady-state tests must pass unchanged (stop rule 7). The
Viking, Liberator and Banshee have flipbook media (Veo batch 5; **verify** they are presentable in the harness), so W-21's limit of the weapon-delivery plan does not
apply: WP3's package gate runs the steady-state tests with Viking volleys in flight (M-18).

### D-9. Records

| Document | Change | WP |
|---|---|---|
| `DESIGN.md` §4 | B-52 restated with §2.1's sources per row and this plan; **B-231** and **B-232** registered (deferred) | WP0 |
| `DESIGN.md` §4 | B-52 **fixed**; the Combat row names target-relative phase ends | WP6 |
| `docs/plans/2026-11-27-weapon-delivery.md` §14 | B-52 bullet gains "*Taken up by plan `2026-11-28-missile-movers-b52`.*"; R-1 gains "(now B-231)" | WP6 |
| New `docs/evidence/2026-11-28-missile-movers-b52.md` | Census, probe, close-battle list, baseline, latent proof, worlds per package, restatements, held-out, goldens, bench | WP0 creates; WP1 to WP6 append |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | Restatements of close battles whose outcome moves (§6.6), each with its trace | WP2, WP3 as measured |
| `docs/plans/README.md` | Row `Proposed` (with the plan), `Active` (WP1), `Done` (WP6) | plan, WP1, WP6 |

---

## 5. Determinism, checksum and coordination

- **Integers only.** `Remaining` compares a fixed-point length (`SimVec2::length`, already used by the impact test on
  the same values) with a `SimScalar`; no new division, square root or float. Catalog decimals are converted once
  (`SimScalar::from_f32_lossy`), as every mover value is.
- **Order.** One projectile at a time in the existing order; the end check reads only the projectile's own state and
  its target's position after `track_projectile_target`, which the impact test already reads.
- **No new collection, randomness or clock.**
- **Latent proof (WP1).** With the schema at `main`'s number, WP1's tree reproduces every recorded checkpoint of both
  goldens and `DEFAULT_SANDBOX_CHECKSUM` on native debug and release and `wasm32-wasip1` debug and release (the
  weapon-delivery `proofcopy.sh` and `unhash.py`, copied). No golden world releases a mover missile (weapon-delivery
  WP0's probe; this WP0 re-measures), so the variant tag is never written there and the schema number is the only
  difference. Then the schema steps to the next free number; WP1 to WP5 do not re-record, and each hand-back says
  the golden tests fail by design.
- **Data packages do not touch the goldens.** WP0 predicts no golden or sandbox world fires the five weapons; if its
  probe finds one that does, stop rule 2 classifies it before WP2.
- **One re-record (WP6, final tree after merging `main`).** `golden_replay.ron`, `golden_replay_orders.ron` and
  `DEFAULT_SANDBOX_CHECKSUM`, under the header "Re-recorded for plan 2026-11-28-missile-movers-b52 (checksum schema M
  -> N): each missile phase's end is encoded as `MissilePhase::ends` with a variant tag; with the schema at M every
  checkpoint reproduced. Behaviour: none in these worlds (no weapon of the plan fires)." Proved on the four builds. If
  `main` has taken N first, the merge repeats the re-record with the next free number.
- **Throughput.** `cargo bench -p sim_core --bench throughput`, release, WP0's binary against the final tree's,
  interleaved, two rounds each: every scenario at **50 ticks per second or more**; **instructions within +0.5 %** of
  the whole run (`perf stat -e instructions:u`, `DESIGN.md` §5's method). The only scenario any row reaches is "67
  casters" (the High Templar's bolts, a constant missile: same code path, a different constant). The mover path adds
  one `match` per due-phase check of a mover missile; no bench scenario flies a mover missile.
- **Gated coordination.** Code is confined to `sim_types/src/weapon.rs`, `combat_resolution.rs` (`launch_projectile`,
  `advance_projectile`), `combat_resolution/missile_flight.rs`, `catalog.rs`, `catalog/validation.rs`, `checksum.rs`
  and `checksum/encoding.rs`, and the catalog data. Nothing under `crates/sim_core/src/movement/`, nor `steering.rs`,
  `movement_rules.ron` or `target_acquisition.rs`, is edited (stop rule 6). If a Gated package lands on `main`
  between two measured packages, the next package's base is re-measured after merging `main`, and the comparison is
  against that base (as plan `2026-11-28-carrier-interceptors` D-10 did).

---

## 6. Acceptance (fixed before implementation)

No acceptance is a figure of a seed. Constructed tests build their maps through `sim_map` (open ground; a plateau
where vision matters) and use shipped units from `sim_fixtures`, found by property, never by slug, except the shipped
values tables (M-7, M-11). A scratch catalog copy supplies values the shipped data lacks. Every test is written red
first against the previous package's tree.

### 6.1 The mechanism (WP1; `missile_flight.rs` unit tests, `crates/sim_core/tests/missile_mover_tests.rs`, `validation.rs`, `checksum_state_tests.rs`)

- **M-1 (Flown unchanged).** Every weapon-delivery test (W-1 to W-9, W-11, W-12) passes with `ends: Some(Flown(x))`
  for `ends_after: Some(x)`, its expected ticks untouched; the shipped Phoenix and Spine Crawler analogues land on the
  ticks of the weapon-delivery evidence (2, 8, 10; 6, 7, 7, 4, 4, 5).
- **M-2 (Remaining, stationary target).** A two-phase mover `(v₀, 0, v₀, Remaining(x))`, `(None, a, cap, None)`
  switches on the first tick after whose step the remaining path is ≤ x; the landing tick equals an oracle in the
  test (the integration of §2.4 in `f64`, as the weapon-delivery oracle does), for a table of paths around x.
- **M-3 (Remaining, moving target).** The same missile at a target that walks towards the launcher switches earlier,
  and at one that walks away later, than at a stationary one at the same release distance; the band chosen at the
  release is kept (W-4's rule).
- **M-4 (release check).** A launch whose remaining path is already ≤ x starts in the next phase (its first step at
  the next phase's integration); a `Flown(x > 0)` end is never entered at the release.
- **M-5 (validation).** Each D-4 rule refused with a typed error naming `….ends`; a stale `ends_after` field and any
  other unknown field in a mover or phase refused (`deny_unknown_fields`); the shipped catalog validates.
- **M-6 (encoding).** Two projectiles whose weapons differ only in an end's variant (`Flown(1)` against
  `Remaining(1)`) have different checksums.
- **M-7 (visual time).** For a `Remaining` mover, the renderer event's `duration_secs` equals `flight_ticks` over the
  event's path, including a launch inside the end distance.

### 6.2 High Templar (WP2)

- **M-8 (speed).** In `catalog_reference_values_tests.rs`, a table of the constant missile speeds of every shipped
  missile weapon without movers, each with its 5.0.14 mover (S12, S14): `MissileDefault` and children 1.05,
  `PunisherGrenadesWeapon` 1.12, the 40-speed movers 2.24, `PhotonCannonWeapon` 1.008; the Brood Lord analogue's row
  carries `known_bug: "B-232"` (0.9 against a throw-then-guidance mover), so the table never passes a mismatch
  silently. Red on WP1's tree: the High Templar analogue at 1.0.
- **M-9 (bolt flight).** The ground unit whose weapon is the 1.05 constant and which has a target-position spell (by
  property) releases at a held target at edge distances 0.63 and 5.9: impact on `ceil(path / 1.05)` ticks, one tick
  earlier than at 1.0 (paths from §2.4).

### 6.3 Viking, Liberator, Banshee, Missile Turret (WP3 to WP5)

- **M-10 (delivery census).** The weapon-delivery W-10 table is unchanged (all four stay missiles).
- **M-11 (reference movers).** `catalog_reference_values_tests.rs` `SC2_MISSILE_MOVERS` gains one row per package
  (Viking in WP3; Liberator and Banshee in WP4; Missile Turret in WP5), each citing §2.1 (S2–S3, S5–S6, S8, S9–S10)
  and the merges of rule 2; red before the package's data.
- **M-12 (flight ticks).** In `missile_mover_tests.rs`, each analogue (found by property: the mobile air unit whose
  air weapon has movers and range 9; the air unit whose air weapon has movers and splash; the unit whose primary `weapon`
  has movers; the structure whose air weapon has movers) attacks a held, unarmed target at §2.4's edge distances:
  each strike lands on §2.4's tick (± 1 at a boundary, recorded; otherwise stop rule 10), both strikes of a volley
  each after its own flight, no damage on the release tick.
- **M-13 (Turret kill phase).** The Turret analogue's missile at an air target holding at edge 6, and at one flying
  towards the Turret from edge 7: the phase switch happens on the first tick whose remaining path is ≤ 4, and the
  approaching target is struck earlier than §2.4's tick for its release distance.
- **M-14 (finders).** `air_mover_unit` and every other property finder in `missile_mover_tests.rs` and
  `weapon_catalog_rules_tests.rs` select one unit by a property unique to it (for the Phoenix analogue: an air weapon
  with a bounded first band); a test asserts each finder matches exactly one shipped archetype. Done in WP3, before
  the Viking's data, and kept green through WP5.
- **M-15 (target lost).** For the Viking and Turret: a target that dies or cloaks undetected mid-flight is missed on
  the last point, as weapon-delivery W-13 (now with phases running out on a lost target).
- **M-16 (cooldown).** The Viking's next release follows its period from the release, not the impact (W-14's rule,
  now with a 15-tick flight inside a 36-tick period).
- **M-17 (zero recompilation).** In a catalog directory copy, changing the Viking's first `Flown` end, or the Turret's
  `Remaining` distance, changes M-12's or M-13's tick in the same binary.
- **M-18 (client).** `effect_catalog_tests`, `weapon_look_tests` pass unchanged; `sync_steady_state_tests` and
  `sync_allocation_count_tests` (valgrind) pass with a Viking-analogue volley in flight (the analogue has media).

### 6.4 World check (pre-registered)

Every world that differs from the previous package's is explained by its **first difference** (`firstdiff.py` and a
per-entity trace) and classed:

- **S (schema bytes).** The goldens and `DEFAULT_SANDBOX_CHECKSUM` differ by the schema number alone (WP1's proof).
- **D (delivery).** The first difference is a projectile of **this package's** weapon whose position, phase or impact
  tick differs (or a strike landing on another tick because of it). Allowed only in worlds WP0's probe lists as
  firing that weapon.
- **P (presentation).** A renderer event's flight time, or a `PlayerView` or fog frame, differs in presentation only.
- **Anything else** (a D of another package's weapon; a target, order, position, damage or RNG draw not downstream of
  a D): a defect, stop rule 2.

### 6.5 Predictions (recorded before measuring, not thresholds)

| World | WP1 | WP2 (HT) | WP3 (Viking) | WP4 (Lib., Banshee) | WP5 (Turret) |
|---|---|---|---|---|---|
| Matrix 12c, 19 | identical | identical | D; outcome unchanged (the Viking is untouched by a held Colossus and Brood Lord; kills land about 10 ticks later per volley) | identical | identical |
| Matrix 35 | identical | identical | D; outcome may move: missiles in flight 9 to 10 ticks longer are lost more often when a Colossus leaves the Vikings' sight, so more colossi may survive (restated if so, §6.6) | identical | identical |
| Matrix 13, 36a, 36b | identical | D where a bolt lands a tick earlier; outcomes unchanged (battle 13's storm kills; 36a and 36b within their ranges) | identical | identical | identical |
| Every other matrix battle, battles 31, 33 | identical | identical | identical | identical | identical |
| Showcase, showcase effects, skirmish | identical | as WP0's probe (no HT expected) | D only if WP0's probe lists the Viking | D only if listed | D only if listed |
| Goldens, sandbox | S, proved, not re-recorded | S | S | S | S; re-recorded once in WP6 |
| Client skirmish stage log | identical | identical | identical | identical | identical |

### 6.6 Close battles

WP0 fixes the close-battle list from its baseline alone, by the rule the weapon-delivery and carrier-interceptors
WP0s applied (hold-facing plan §6.4): every matrix battle whose minority side wins at least 2 of its 20 baseline runs;
battles 31 and 33 at the same share over their 100 and 64 seeds; every battle a closed owner question or fixture
comment names close or restated; no other; a `known_bug` battle is not close. (The carrier-interceptors WP0 gave 14,
26, 31, 33, 35, 38a, 38b.) **The list is frozen when WP0 commits and not edited after.**

An **outcome move** is a run whose winner changes, or a battle whose expectation (`expect`, survivor ranges,
`blue_lead_hp`) fails on a seed of record or a development seed where it held at the base.

- A battle **on the list** whose outcome moves with a first difference of class D is **restated under charter §8**:
  its claim or comment gains the moved share, the first differing tick of one flipped seed traced (the release whose
  flight changed, its band and phase ticks, and the strike that landed elsewhere or not at all), and the 5.0.14
  source of the change (§2.1).
- **Any outcome move off the list, or on the list with a first difference not of class D, is stop rule 4**, reported
  with its trace for review before the package continues. In particular, 12c, 19, 13, 36a and 36b are predicted not
  close; a move in any of them stops.
- Held-out seeds (§7.3) only confirm; nothing is restated or tuned from them.

---

## 7. Measurement

### 7.1 WP0 census and probe (scratch, not committed)

- **(c) Census.** `movers.py` copied to `~/scratch/missile-movers-b52/census/`, extended to print each element's file
  and line and to resolve the five launches of §2.1 (`LanzerTorpedoesLM`, `LiberatorDamageMissileLM`,
  `BacklashRocketsLM`, `LongboltMissileLM`, `HighTemplarWeaponLM`) and `BroodlingEscortLaunchA`. It confirms S2–S3,
  S5–S6, S8–S10, S12, S15. A second listing, read from the files, prints every place each of the five units can enter
  a world: `matchup_matrix.ron` squads, battles 31 and 33, both goldens' squads, the sandbox, `scenarios/*.ron`,
  `start_loadouts.ron`, the throughput scenarios, and every structure whose `trains_units` names it.
- **A scratch probe patch** (`~/scratch/missile-movers-b52/probe/`, from the weapon-delivery probe; it may name slugs,
  being scratch) logs per world and seed, changing no state, for each of the five weapons: (r) releases; (t) the edge
  distance at each release with today's and §2.4's flight ticks (computed by the planning script's rule in the
  probe); (k) kills; (l) missiles lost in flight (disjointed). A positive control (a scratch test setting each weapon
  on an enemy for 300 ticks) proves a zero is a measured zero. **Predicted:** Viking releases in 12c, 19 and 35 only;
  High Templar releases in 36a and 36b and probably 13 (its zerglings stand at edge about 5.85, inside range 6); no
  Liberator, Banshee or Turret release in any matrix battle, battles 31 and 33, the goldens or the sandbox; the
  showcase, showcase effects and skirmish worlds measured, not predicted.
- **Close-battle list** (§6.6), **baseline** on four builds (both goldens and `DEFAULT_SANDBOX_CHECKSUM`),
  **throughput and instructions**.

### 7.2 Commands

The weapon-delivery plan's scripts, copied to `~/scratch/missile-movers-b52/scripts/` (`measure.sh` with the skirmish
world, `compare.sh`, `firstdiff.py`, `proof.sh`, `proofcopy.sh`, `unhash.py`, `heldout.sh`, `bench.sh`, `close.py`,
`matrixdiff.py`) with their paths changed:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/missile-movers-b52-plan
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/missile-movers-b52
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-<prev> $R/world-<wp>
python3 -I $R/scripts/matrixdiff.py $R/world-<prev> $R/world-<wp>
```

One at a time across worktrees, in the background, never beside a `cargo gate` or a workspace `cargo test`
(charter §7).

### 7.3 Held-out seeds (WP6, confirmation only)

Once, on `main` and the final tree: matrix seeds 7001 to 7016, battles 31 and 33 on 7001 to 7064, the showcase and
showcase effects on seeds 9 to 16, the skirmish scenario on seeds 9 to 12. Allowed: differences of class D or P only
in worlds WP0's probe lists for the weapon whose D it is; outcome moves only in close battles already restated, of
class D. Anything else is stop rule 3. Nothing is restated or tuned from them.

---

## 8. Stop rules

1. WP1's latent proof fails (a checkpoint differs with the schema at `main`'s number): stop; something other than the
   encoding moved.
2. A measured world's first difference is not of class S, D or P (§6.4), or is class D of a weapon not in the
   package, or in a world WP0's probe did not list for it: stop, trace, report.
3. A held-out seed differs outside §7.3's allowance: stop and report.
4. An outcome moves that §6.6 does not allow (off the close list, or on it with a first difference not of class D):
   stop for review, with the trace.
5. Throughput below 50 ticks/s in any scenario, or instructions above +0.5 %: stop and report.
6. Anything under `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron` or `target_acquisition.rs`
   would need an edit (the Gated coordination rule): stop and report to the orchestrator.
7. The client needs a `client/src` change, or a steady-state or allocation test fails: stop.
8. M-1 fails (a shipped constant or Flown flight moves), or a constant missile's event time changes: stop; fix the
   code, never the test.
9. WP0's census contradicts §2.1 (a phase, band, ammo unit or strike structure differs): stop WP0; the plan is amended
   before WP1.
10. A measured flight tick of M-12 differs from §2.4 by more than one tick at any listed distance, or by one tick
    without a fixed-point boundary to explain it: stop; amend §2.4 or the data with the reason.

---

## 9. Client consequences

None in code (D-8). Data only reaches the client through `ProjectileVisual::duration_secs`. Tests: M-18; the GPU and
diagnostics suites run in WP3's package gate through their catalogue triggers.

---

## 10. File ownership

| File | Change | WP |
|---|---|---|
| `DESIGN.md` | §4 rows (D-9) | WP0, WP6 |
| `docs/evidence/2026-11-28-missile-movers-b52.md` | New | WP0 to WP6 |
| `crates/sim_types/src/weapon.rs` | `PhaseEnd`; `MissilePhase::ends`; unit tests (RON forms, defaults) | WP1 |
| `crates/sim_core/src/combat_resolution/missile_flight.rs` | `enter_due_phases` with the remaining path; `flight_ticks` with the release check; module doc | WP1 |
| `crates/sim_core/src/combat_resolution.rs` | The release check in `launch_projectile`; the remaining path in `advance_projectile` | WP1 |
| `crates/sim_core/src/catalog.rs` | `PhaseEndDef`; `deny_unknown_fields` on the mover defs | WP1 |
| `crates/sim_core/src/catalog/validation.rs` | D-4 | WP1 |
| `crates/sim_core/src/checksum.rs`, `checksum/encoding.rs` | Schema line; `fields!(MissilePhase {…, ends})`; `PhaseEnd` encoding | WP1 |
| `crates/sim_core/tests/missile_mover_tests.rs` | M-1 to M-4, M-7 (WP1); M-9 (WP2); M-12 to M-17 (WP3 to WP5) | WP1 to WP5 |
| `crates/sim_core/tests/{checksum_state_tests,catalog_reference_values_tests,runtime_catalog_tests,weapon_catalog_rules_tests}.rs` and literals naming `ends_after` | M-5, M-6, M-8, M-10, M-11, M-14; `ends` in literals | WP1 to WP5 |
| `assets/catalog/units.ron` | Header comment (`ends`, `Flown`, `Remaining`); the shipped Phoenix row's `ends` (WP1, same values); `cerebral_elder` (WP2); `vector_interceptor` (WP3); `sky_bastion`, `cloak_gunship` (WP4) | WP1 to WP4 |
| `assets/catalog/structures.ron` | `thorn_lasher_dome`'s `ends` (WP1, same values); `flak_array_dome` (WP5) | WP1, WP5 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | Restatements under §6.6 only | WP2, WP3 |
| `crates/client/tests/{sync_steady_state_tests,sync_allocation_count_tests}.rs` | M-18's Viking volley, only if the existing staging cannot name it | WP3 |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` | Re-recorded once | WP6 |
| `docs/plans/2026-11-27-weapon-delivery.md` | §14 pointer, R-1 pointer | WP6 |
| `docs/plans/README.md` | Row | plan, WP1, WP6 |

**Not touched:** `crates/sim_core/src/movement/`, `steering.rs`, `movement_rules.ron`, `target_acquisition.rs`,
`effects/` (spell missiles keep their own speed field), `entity.rs`, every `client/src` file.

---

## 11. Work packages (commit order)

Each package is red first: its tests are written and fail on the previous package's tree, then the change makes them
pass. The implementation agent hands back after `cargo gate quick` and the tests its change needs; a verifier runs
`cargo gate package` on the final tree after merging `main` (Q0061, `docs/guides/test-gates.md`). Builds go to
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; logs, probes and proofs under
`~/scratch/missile-movers-b52/`. Each data package measures its worlds against the previous package's (§6.4) before
it commits.

### WP0: records, census, probe, baseline (XS, docs and scratch)

D-9's WP0 rows (B-52 restated, B-231 and B-232 registered); §7.1's census, probe with its control, close-battle list
(frozen), baseline on four builds, throughput and instructions. **Acceptance:** stop rule 9 holds; the evidence lists
every world's (r), (t), (k), (l) per weapon and the frozen list. Commit `docs(design): restate B-52, register B-231
and B-232 (plan 2026-11-28-missile-movers-b52 WP0)`.

### WP1: target-relative phase ends, latent (S, `sim_types`, `sim_core`)

D-2 to D-5. Red first: M-1 to M-7 (M-1 as a guard, green before and after). The two shipped rows move to `ends:
Some(Flown(…))` with the same values. **Acceptance:** M-1 to M-7 pass; every measured world byte-identical to WP0's;
latent proof 8 of 8, then the schema steps; stop rules 1, 6, 8 hold. Commit `feat(sim_core): target-relative missile
phase ends (B-52; latent)`.

### WP2: High Templar (XS, data and tests)

D-1's `cerebral_elder` row. Red first: M-8, M-9. Worlds against WP1's; restatements per §6.6. **Acceptance:** M-8, M-9
pass; every difference classed; stop rules 2 and 4 hold. Commit `fix(catalog): the High Templar analogue's bolt flies
MissileDefault's 1.05 (B-52)`.

### WP3: Viking (S, data and tests)

M-14's finders first (green on WP2's tree, red-proof by a scratch copy adding movers to a second air weapon). Then D-1's
`vector_interceptor` row. Red first: M-11's Viking row, M-12, M-15, M-16, M-17, M-18. Worlds against WP2's; battle 35
restated if its outcome moves. **Acceptance:** the tests pass; 12c and 19 keep their expectations; stop rules 2, 4,
7, 10 hold. Commit `fix(catalog): the Viking analogue's torpedoes fly VikingFighterMissile (B-52)`.

### WP4: Liberator and Banshee (S, data and tests)

D-1's `sky_bastion` and `cloak_gunship` rows (split into WP4a and WP4b if WP0 finds different reach). Red first: their
M-11 rows and M-12 cases. **Acceptance:** as WP3, worlds against WP3's. Commit `fix(catalog): the Liberator and Banshee
analogues' rockets fly their movers (B-52)`.

### WP5: Missile Turret (S, data and tests)

D-1's `flak_array_dome` row. Red first: its M-11 row, M-12 case, M-13, M-15, M-17. **Acceptance:** as WP3, worlds
against WP4's. Commit `fix(catalog): the Missile Turret analogue's missiles accelerate 4 from the target (B-52)`.

### WP6: confirmation, re-record, records (S)

Held-out seeds (§7.3); throughput and instructions (§5); goldens and sandbox re-recorded once on the final tree after
merging `main`, proved on four builds; D-9's WP6 rows; `## As built`. **Acceptance:** stop rules 3 and 5 hold; B-52
fixed. Commit `docs: close B-52 (plan 2026-11-28-missile-movers-b52 WP6)`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Level | File | Covers |
|---|---|---|
| Unit | `sim_types` `weapon.rs` | RON forms of `PhaseEnd` |
| Unit | `sim_core` `missile_flight.rs`, `catalog/validation.rs` | M-2, M-4, M-5, M-7 |
| Integration | `crates/sim_core/tests/missile_mover_tests.rs` | M-1 to M-4, M-9, M-12 to M-17 through `SimWorld` and `PlayerCommand` |
| Integration | `checksum_state_tests.rs`, `catalog_reference_values_tests.rs`, `weapon_catalog_rules_tests.rs`, `runtime_catalog_tests.rs` | M-5, M-6, M-8, M-10, M-11, M-14, M-17 |
| Determinism | goldens, sandbox, four builds | §5: latent proof, one re-record |
| Client | `effect_catalog_tests.rs`, `weapon_look_tests.rs`, steady-state and allocation tests | M-18 |
| Bench | `cargo bench -p sim_core --bench throughput`; `perf stat` | ≥ 50 ticks/s, ≤ +0.5 % instructions |

No `crates/gate/suites.ron` change is expected (no new test file); if one is needed, WP1 names it.

---

## 13. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1: acceleration does not carry into the Viking's phase 3 | Viking flights 2 to 3 ticks longer at edge ≥ 6 than authored | S11 states the general carry-over; the alternative is recorded in §2.4; a one-row data change if disproved |
| R-2: the throws' direction (B-231) | Flights lower bounds; the Liberator's by up to about 7 ticks | Registered, deferred with the reason (D-7) |
| R-3: battle 35 moves beyond restating (the Vikings stop winning) | A large behaviour change in a close battle | It is 5.0.14's mover (S3); restated with its trace; if the winner flips on most seeds, it is still restated, and reported to the orchestrator before WP3 merges |
| R-4: the High Templar's earlier bolts move 36a or 36b out of range | Stop rule 4 for a one-tick change | Predicted not to (4 damage, one tick); the trace decides; a review may restate a non-close battle only by an amendment |
| R-5: property finders pick the wrong analogue once four more weapons have movers | Old tests test the wrong unit | M-14 before WP3's data |
| R-6: schema number taken first | Merge conflict in the encoding and goldens | "Next free at merge"; WP6 re-records once on the final tree |
| R-7: a Gated package lands mid-plan | Bases shift | §5: re-measure the base after merging `main` |
| R-8: fixed point moves a §2.4 tick at a boundary | A test expectation off by one | M-12 allows ± 1 at a recorded boundary; stop rule 10 otherwise |

---

## 14. Out of scope and decisions recorded (Q0002)

- **The throw direction** (B-231, D-7): deferred; flight times stay lower bounds.
- **The Brood Lord analogue's broodling missiles** (**B-232**, provisional): "The Brood Lord analogue's missiles fly a
  constant 0.9 with no source; 5.0.14's `BroodlingEscortLaunchA` and its escort missiles fly `BroodLordWeaponRight` /
  `Left` (`liberty` EffectData:2664–2667, :2741–2748, :2774–2779; MoverData:44, :620: `Throw` 5 accelerating 3 to 8
  to `Outro 1.5,4.5`, then `Guidance` accelerating 20 to 40). Deferred to the plan that fixes B-21 (Brood Lord spawns
  no Broodlings), because the escort chain is that plan's delivery; it reaches battles 19 and 20." Not in B-52's rows;
  the movers mechanism already expresses it. M-8 lists it with `known_bug: "B-232"`.
- **Decelerating phases and `MinSpeed`:** no row here uses them (weapon-delivery D-4 keeps refusing negative
  acceleration).
- **The Liberator's Defender Mode weapon:** not a missile row of B-52.
- **High Templar spells:** no missile (S13).
- **Teams:** unchanged (Q0101 4(b)).

---

## 15. Owner questions

None drafted. Every value is read from the 5.0.14 data, the one inference (S11's carry-over) is the editor's
documented rule with its alternative recorded (R-1), and both deferrals (B-231, B-232) are scoping decisions recorded
in §14, which charter §8 leaves to the agent.

---

## 16. Material (scratch, not committed)

- Game data `~/scratch/combat_wp3_data/` (`5444e12f13`): `liberty` WeaponData:266–275, :300–309, :548–562; `voidmulti`
  WeaponData:133–135, :284–294; `balancemulti` WeaponData:257–267; `void` WeaponData:321–330; `liberty`
  EffectData:679–684, :1698–1712, :1779–1794, :2069–2076, :2550–2563, :2664–2667, :2741–2748, :2774–2779; `voidmulti`
  EffectData:36–39, :1532–1537; `balancemulti` EffectData:1404–1409; `void` EffectData:2923–2983; `liberty`
  MoverData:44, :485–510, :531–567, :589–600, :620; `void` MoverData:384–422; `voidmulti` and `balancemulti`
  MoverData:47; `core` MoverData:13–25; `liberty` UnitData:6901–6905, :11605–11608; `void` UnitData:3411–3415;
  `liberty` AbilData:570–583, :2059–2071.
- The weapon-delivery census output `~/scratch/weapon-delivery/census/movers.txt` and its evidence
  (`docs/evidence/2026-11-27-weapon-delivery.md`, "B-52's remaining rows").
- Outside source (Q0008), 2026-11-26: S11. No source found on `ThrowVector`'s frame (B-231).
- Planning script `~/scratch/missile-movers-b52/flight.py`: the weapon-delivery integration with this plan's
  `Remaining` end and release check, over §2.4's distances, with the alternative readings.
- Code read on `4ff8402f`: `missile_flight.rs`, `combat_resolution.rs` (`launch_projectile`, `advance_projectiles`,
  `advance_projectile`, `track_projectile_target`), `sim_types/src/weapon.rs` (`MissileMover`, `MissilePhase`,
  default lifetime 125), `catalog.rs` (`MissilePhaseDef`), `catalog/validation.rs`, `checksum.rs`,
  `checksum/encoding.rs`, `missile_mover_tests.rs` (its finders), `catalog_reference_values_tests.rs`
  (`SC2_MISSILE_MOVERS`), `weapon_catalog_rules_tests.rs` (the delivery census), the five rows and their `effects`
  blocks, `effects.ron` (`rocket_trail`, `plasma_bolt`), `matchup_matrix.ron` (12c, 13, 19, 35, 36a, 36b),
  `throughput_scenarios.ron`, `structures.ron` (`trains_units`).

---

## As built

Built on branch `missile-movers-b52-plan` from `4ff8402f` (`main` did not move during the work); evidence in
[`docs/evidence/2026-11-28-missile-movers-b52.md`](../evidence/2026-11-28-missile-movers-b52.md).

- **WP0** (`7ad7235a`): the census confirmed every source of §2.1 (stop rule 9 held); the probe and its control found
  Viking releases in 12c, 19 and 35 only, High Templar releases in 13, 36a and 36b only, and no Liberator, Banshee or
  Turret release in any measured world, golden or the sandbox (so WP4 did not split); the four-build baseline was 8 of
  8; the close list frozen: 14, 26, 31, 33, 35, 38a, 38b. The Turret analogue's releases read its radius 1.125 (§2.4's
  **verify**).
- **WP1** (`ecdaaea9`, C7 allowance `37e8ec78`): `PhaseEnd::{Flown, Remaining}` as D-2 states, checked after each step
  against the target's current aim point and once at the release; D-4's validation; `deny_unknown_fields` on the mover
  defs; the encoding's variant tag; schema 60 (provisional). Latent proof 8 of 8; worlds byte-identical. M-6 is a
  dedicated test (two projectiles differing only in the end's variant): the first, a mutation from `None`, did not
  pin the tag, which the scratch red showed.
- **WP2** (`51c47c2a`): the bolt at 1.05; M-8's table lists every shipped constant missile with its 5.0.14 mover and
  carries `known_bug` B-232 (the Brood Lord analogue) and, until their packages, B-52. Worlds: 13, 36a and 36b differ
  by a bolt landing a tick earlier (class D), no outcome moved.
- **WP3** (`87800150`, restatement `06a6585d`, literal `9024af20`): M-14's finders exact (`one_archetype`), then the
  Viking's movers. Flights 9, 12, 14, 15, 16, 17 ticks, §2.4 exactly. Worlds: 12c, 19, 35 (class D); 12c and 19 keep
  their outcomes; **battle 35 restated under §8**: no matrix run kills all six colossi (2 of 20 before).
- **WP4** (`53fb8665`, S-4 `4c94f740`, W-20 `16bc93e0`): the Liberator's and Banshee's movers, §2.4 exactly; worlds
  identical. S-4's victim holds (both missiles fly the same distance, its own premise); W-20 accepts a look that draws
  a mover weapon's landing with a splash (the Liberator analogue's).
- **WP5** (`6524a75d`): the Turret's movers with `Remaining(4.0)`, §2.4 exactly; M-13 at a holding target and one
  flying in from beyond range; worlds identical.
- **WP6**: held-out seeds confirm (class D only, in probe-listed worlds; battle 35 flips both ways on 4 of 16); every
  bench scenario at 80 ticks/s or more, instructions +0.0017 %; goldens and the sandbox re-recorded once at schema 60
  (`bca70ca5`), proved 8 of 8 recorded and 8 of 8 latent; B-52 fixed.

Deviations from the plan as written, each recorded:

- **M-13's approaching target** starts beyond the Turret's range, already flying in: the Viking-analogue target
  accelerates slowly, and ordered to approach only after the release it moved under half a unit, so the flight did not
  shorten. Its comparison is with a still target at its measured release distance (the oracle over the shipped
  phases), not §2.4's 12.
- **M-15 for the Turret** removes its copy's detection: the Turret analogue detects (as 5.0.14's does), so a cloaked
  target would not be undetected.
- **M-14** narrows the finders of `missile_mover_tests.rs`; the two "any air unit" and "any ground unit" finders of
  `weapon_catalog_rules_tests.rs` stay, since any such unit serves their tests and no mover data can change them.
- **M-18**: the steady-state and allocation tests run unchanged and pass, but without a Viking-analogue volley: the
  default battle has no Viking analogue and the running children fire nothing by design; a staged Viking window was not
  added. Left for the orchestrator to decide.
- Two older tests whose premise a moved flight broke were narrowed to their premise (S-4, W-20), with no change to
  `client/src` or the simulation.
