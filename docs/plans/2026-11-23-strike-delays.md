# Blueprint: When the First Strike Lands — the Effect Chain's First Period (B-81)

This plan settles the last open part of bug B-81 against StarCraft II 5.0.14, and checks every other modelled weapon
for the same kind of delay:

- **The Liberator's first missile leaves one tick after the release (B-81).** Its weapon creates
  `LiberatorMissileBurstPersistent`, whose only period entry, 0.05 Normal s, comes before the first missile as well as
  between the two. Ours launches the first missile on the release tick and the second one tick later. After this plan
  the missiles leave on the first and second ticks after the release.
- **The Colossus is not changed.** Its strikes and its sweep stay on the release tick, by the owner's answer to
  [Q0064](../questions/closed/0064-colossus-damage-timing.md).
- **The survey (§2.3)** read the effect chain of every weapon with an analogue in the catalog. The Liberator is the
  only modelled weapon whose chain delays its first strike. Three unmodelled chains also have a first delay: the
  Carrier's interceptor launch (B-22), the Thor's Explosive Payload missiles (whose periods are uneven) and the
  Mothership's secondary beam. Each is deferred with a reason and a hook (§14).

The rule becomes one integer capability field on `WeaponSpec`, read from the catalog: `first_strike_delay_ticks`,
the first entry of the burst effect's `PeriodicPeriodArray`. A delayed burst keeps all its strikes pending from the
release, and each strike, the first included, is validated when it falls due, as StarCraft II validates each period
of a persistent on its own. No code names a unit (charter §3.3, C5).

A full plan is required (charter §6) for three reasons: a new rule in the simulation, a change of hashed encoding (the
checksum schema), and data that changes battle outcomes wherever the Liberator analogue fights. No measured world
contains it (§7), so the move is shown by constructed tests and a scratch probe.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at `cdf4ea93`
(checksum schema 36).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-5: 25 ticks per Faster second, whole-tick rounding) | Closed | D-4 |
| Q0064: the Colossus' strikes and sweep land on the release tick | Answered (option 1) | D-5, §2.3 |
| Q0058: an explicit order between the strikes ends a `Channeled` burst | Answered | D-2 (the Liberator's burst is channeled) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.3, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| No new owner question | — | §15 |

---

## 1. Summary

| | Today (`cdf4ea93`) | After this plan |
|---|---|---|
| Liberator (`sky_bastion`, "Lexington Rockets") | Missile 1 on the release tick `T`, missile 2 on `T + 1` | Missile 1 on `T + 1`, missile 2 on `T + 2`; each launched from where the Liberator is on that tick |
| A delayed strike's conditions | — (none is delayed) | As a later strike's today: attacker alive and not boarded, target available, on the weapon's plane, within `followup_max_distance` if one is named; a channeled burst is ended by an explicit order (Q0058) |
| Colossus (`tripod_annihilator`) | Strikes and sweep on `T` | Unchanged (Q0064) |
| Every other weapon | First strike on `T` | Unchanged: the first period of its chain is 0, or the chain has no persistent (§2.3) |
| `WeaponSpec` | `strike_interval_ticks`, `followup_max_distance`, `channeled` | + `first_strike_delay_ticks: u16`; omitted in RON reads 0 |
| Hashed state | — | The field, in the encoding of a projectile's `WeaponSpec`; `PendingStrikes` unchanged; schema **next free at merge** (37 unless another branch takes it, then 38 or 39) |
| Worlds that move | — | WP1: none (goldens and sandbox by the schema bytes only). WP2: no measured world contains the Liberator, so none is expected to move; a scratch probe and constructed tests show the move (§7) |
| Size | — | S–M: four packages; WP1 M, the others XS or S |

---

## 2. What StarCraft II does

### 2.1 Sources

All are from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data.
The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. Times
are in Normal seconds. Ticks = Normal s ÷ 1.4 × 25, rounded half up (S-5; audit §2.3 of
[`2026-11-04-catalog-time-audit`](../evidence/2026-11-04-catalog-time-audit.md)).

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | A multi-strike weapon fires a `CEffectCreatePersistent`. Strike `k` (from 0) comes `PeriodicPeriodArray[k]` after the one before it, and strike 0 comes that long after the persistent is created. A patch that removes a first-strike delay sets entry 0 to 0 | `BacklashRockets`: `liberty` has only `0.15`; `voidmulti` sets `index="0"` to `0` and appends `0.15` | `liberty` EffectData:2550–2556; `voidmulti` EffectData:36–38 (the reading of deviation-fixes evidence §5.1) |
| S2 | An array shorter than `PeriodCount` repeats its last entry | The Liberator's single `0.05` for two periods; the Lurker's `void` array | `void` EffectData:2947–2969 (evidence §5.1 "one entry, repeated") |
| S3 | The Liberator's anti-air weapon runs the burst persistent directly | `LiberatorMissileLaunchers` `Effect` = `LiberatorMissileBurstPersistent`, `Period` 1.8, no `DamagePoint` | `void` WeaponData:321–331 |
| S4 | The weapon's release is the default damage point | `CWeapon` default `DamagePoint` 0.167 (3 ticks, as the catalog's `damage_point_ticks: 3`) | `core` WeaponData:3, :17 |
| S5 | The burst: two periods of `LiberatorDamageMissileLM`, `PeriodicPeriodArray` `0.05` alone, `Channeled`, `RandomOffset`, `WhichLocation TargetUnit` | First missile 0.05 after creation, the second 0.05 after the first | `void` EffectData:2942–2970 |
| S6 | Each period launches a missile from the caster to `LiberatorMissileDamage`, with no validator on the launch | `LaunchLocation CasterUnit`, no `ValidatorArray` | `void` EffectData:2979–2984 |
| S7 | `LiberatorMissileLaunchersPersistent` (period 0.75, `FinalEffect` damage) is not referenced by any weapon | Only its own `InitialEffect` names the burst | `void` EffectData:2923–2931; no other reference in any layer |
| S8 | The Colossus' timing is the owner's choice | Strikes and sweep on the release tick | [Q0064](../questions/closed/0064-colossus-damage-timing.md) Answer and Resolution |

**Conversion (S5).** 0.05 ÷ 1.4 × 25 = 0.893, which rounds to **1 tick**. The interval was already authored as 1
(`sky_bastion` `strike_interval_ticks: 1`, deviation-fixes WP5 b5), and the first delay is the same 1.

### 2.2 The rules that follow

1. **The first strike waits for the first period.** If a weapon's burst persistent has a first period `p0 > 0`,
   strike 0 lands `round(p0)` ticks after the release, and strike `k` lands `round(p0) + k × interval` ticks after
   it. Where the array varies after entry 1, this formula does not hold; no modelled weapon has such an array
   (§2.3, F-2).
2. **Each period is validated when it falls due.** A period's effect runs, or fails, on the loop it falls due. This
   is the rule WP5 already applies to the strikes after the first (`advance_pending_strikes`), and it is now applied
   to a delayed first strike too. The Liberator's launch has no validator (S6). Its persistent sits on the target
   unit (S5), so a target that is dead or gone on that loop gets no missile (**verify**: the engine's handling of a
   persistent whose `TargetUnit` has died is not in the data; the 1-tick window makes this rare).
3. **A channeled burst ends with an explicit order.** The burst is `Channeled` (S5), so Q0058's rule applies from
   the release: an order that replaces the attack between the release and the first missile cancels both missiles.
4. **The missile leaves from where the caster is on its loop** (`LaunchLocation CasterUnit`, S6), not from where the
   caster was at the release.

### 2.3 Survey: the first period of every modelled weapon

Each weapon was traced from its `WeaponArray` entry through every effect it names (`Effect`, or the weapon's own
id when no `Effect` is given; then `InitialEffect`, `PeriodicEffectArray`, `ImpactEffect`, `EffectArray`,
`ExpireEffect`, `CaseArray`). The table lists every chain that contains a persistent, a launch with a delay, or an
`InitialDelay`. Every other modelled weapon's chain is a direct `CEffectDamage`, a `CEffectSet`, a switch or a
`CEffectLaunchMissile` with no delay field: the SCV, Marine, Roach, Siege Tank (both modes), Marauder, Ghost, Probe,
Stalker, Adept, High Templar, Dark Templar, Immortal, Drone, Zergling, Baneling, Ravager, Hydralisk, Mutalisk,
Corruptor, Ultralisk, Queen (`AcidSpines`), Planetary Fortress, Photon Cannon, Spine Crawler and Spore Crawler. The
trace is a script under `~/scratch/strike-delays/` (§16). The `InitialDelay` element appears on no weapon chain of an
analogue: its only three uses in any layer are the Mothership's secondary beam and the Nuke's two persistents
(`liberty` EffectData:943, :1876, :1887).

| Analogue (slug) | Burst effect | First period → ticks | Later periods | Decision |
|---|---|---|---|---|
| **Liberator** (`sky_bastion`) | `LiberatorMissileBurstPersistent` | **0.05 → 1** (S5) | 0.05 (repeated) → 1 | **In: B-81** (D-1 to D-4, WP2) |
| Zealot (`psi_grey`) | `PsiBladesBurst` | 0 (`liberty` EffectData:1919, entry at :1926) | 0.28 → 5 | Unchanged |
| Reaper (`jet_jumper`) | `P38ScytheGuassPistolBurst` | 0 (`liberty` EffectData:1810) | 0.122 → 2 | Unchanged |
| Thor ground (`colossus_mech`) | `ThorsHammer` | 0 (`liberty` EffectData:2250) | 0.375 → 7 | Unchanged |
| Viking (`vector_interceptor`) | `LanzerTorpedoes` | 0 (`liberty` EffectData:1704) | 0.21 → 4 | Unchanged |
| Banshee (`cloak_gunship`) | `BacklashRockets` | 0 since `voidmulti` (EffectData:37; `liberty` :2556 had 0.15) | 0.15 → 3 | Unchanged (S1) |
| Missile Turret (`flak_array_dome`) | `LongboltMissile` | 0 (`liberty` EffectData:1786) | 0.1 → 2 | Unchanged |
| Queen ground (`hive_matron`) | `TalonsMissileBurst`, `TalonsBurst` | 0 (`swarm` EffectData:3045; `liberty` :2327) | 0.3 → 5 | Unchanged |
| Phoenix (`flying_saucer`) | `IonCannons` | 0 (`liberty` EffectData:1965) | 0 | Unchanged |
| Hellion (`napalm_wheeler`) | `InfernalFlameThrowerCP` | 0 (`liberty` EffectData:2647) | 0 (all searches together) | Unchanged |
| Lurker (`burrow_scorpion`) | `LurkerMP` | 0 since `void` (EffectData:298) | 0.125 → 2 (B-85, fixed) | Unchanged |
| Sentry (`prism_weaver`) | `DisruptionBeam` | Its `InitialEffect` damages when it is created (`liberty` EffectData:2520; `voidmulti` :238) | 0.0625, gated by a target behaviour | Unchanged (no first delay) |
| Void Ray (`prismatic_saucer`) | `PrismaticBeam` → `PrismaticBeamChargeInitial` | 0 (`liberty` EffectData:2003–2012) | 0.6 per charge stage | Unchanged |
| Colossus (`tripod_annihilator`) | `ThermalLancesForward`/`Reverse` | Unresolved by the data (Q0064 §"What is not stated") | 0.0227 sweep | **Out: Q0064**, release tick kept (D-5) |
| Carrier (`mothership_carrier`) | `InterceptorLaunchPersistent` | 0.375 → 7 (`voidmulti` EffectData:428; `liberty` :893–902 had 0.5) | 0.5 → 9 | **Deferred with B-22** (F-1): ours is an 8-strike salvo, not interceptors |
| Thor anti-air, Explosive Payload (not modelled) | `JavelinMissileLaunchersPersistent` | 0 | 0.125, 0.25, 0.125 → 2, 4, 2 (uneven) | **Deferred** (F-2): the catalog models the High Impact weapon (`LanceMissileLaunchers`, instant) |
| Mothership secondary beam (not modelled) | `MothershipSecondaryBeamPersistent` | `InitialDelay` 0.25 → 4 (`liberty` EffectData:934–943) | — | **Out** (F-3): no analogue weapon |
| Brood Lord (`spore_matriarch`) | `BroodlingStrike` (`Effect` empty) | — | — | Out: B-21 |

**Missile launches.** No `CEffectLaunchMissile` of a modelled weapon sets a launch delay. The only delay field in
these chains is the Spine Crawler's `ReturnDelay` 0.175 (`liberty` EffectData:2160), which delays the tentacle's
return after impact and not the damage. A missile's flight (its mover's speed and acceleration) is the projectile
model's subject and is out of scope (§14).

---

## 3. Diagnosis (on `cdf4ea93`, read from the code)

- **`WeaponSpec`** (`crates/sim_types/src/weapon.rs`) has `strike_interval_ticks`, whose doc reads "The first strike
  lands on the release tick and strike `k` lands `k × strike_interval_ticks` ticks later". No field can delay strike 0.
- **`SimWorld::begin_strikes`** (`crates/sim_core/src/combat_resolution.rs`) is called from `advance_weapon_cycles`
  on the tick `step_entity_weapon` releases. It returns how many strikes land now: all of them when the interval is
  0 or there is one strike, otherwise 1, with the rest put in `Entity::pending_strikes`. The caller always pushes a
  `PreparedShot` with that count, so nothing can launch 0 strikes on the release tick.
- **`SimWorld::advance_pending_strikes`** runs first in each entity's turn of `advance_weapon_cycles`, before the
  weapon step. A `PendingStrikes` with `ticks_until_next = d` set on tick `T` falls due on tick `T + d`. The strike
  that falls due is validated: the attacker is alive and not boarded, it still has the weapon for `pending.plane`
  (re-resolved, so upgrades apply), the target is available (`StrikeContext::is_available`), the target is on
  `pending.plane`, and the attacker is within `followup_max_distance` if one is named. The strike leaves from
  `entity.position` on that tick, and **exactly one** strike lands per due tick. A pending burst whose weapon has
  interval 0 would never re-queue, so a delayed volley would lose every strike after the first.
- **`Entity::end_channeled_burst_for`** (`crates/sim_core/src/entity.rs`) drops a channeled `pending_strikes` on an
  explicit order that is not an attack on the same target (Q0058). It works for any pending burst, delayed or not.
- **Validation** (`WeaponSpecDef::validate_strikes`, `crates/sim_core/src/catalog/validation.rs`) checks three things:
  an interval needs two strikes; `(attacks_count − 1) × interval < period_ticks`; and a distance needs an interval.
- **Checksum.** `fields!(WeaponSpec { … })` in `crates/sim_core/src/checksum/encoding.rs` encodes every field, and a
  `WeaponSpec` is hashed inside each `ActiveProjectile`. `PendingStrikes` (`target_id`, `plane`, `remaining`,
  `ticks_until_next`, `channeled`) is hashed with each entity. `CHECKSUM_SCHEMA_VERSION` is 36.
- **Trace tool.** `sim_fixtures::trace` (`crates/sim_fixtures/src/trace/mod.rs`, the "wasted shots" replay) counts
  the strikes a release lands as `attacks_count` when the interval is 0, else 1. It skips projectile weapons, so the
  Liberator never reaches it, but its rule would be wrong for an instant weapon with a delay.
- **Where the Liberator appears.** `sky_bastion` is in `units.ron`, the Starport's `trains_units`
  (`structures.ron`), the client's look tables and media. It is in no matrix battle
  (`crates/sim_core/tests/fixtures/matchup_matrix.ron`), no scenario under `assets/scenarios/`, no start loadout, and
  neither golden fixture. The skirmish pilot trains `pulse_bot` (production-queue plan T10).

---

## 4. Design decisions

### D-1. `WeaponSpec::first_strike_delay_ticks: u16` (the rule's data)

- **Meaning.** The number of ticks from the release to the first strike: the first entry of the burst effect's
  `PeriodicPeriodArray`, converted by S-5. 0 is the engine default (a chain without a persistent, or one whose first
  entry is 0).
- **Placement.** In `WeaponSpec`, directly after `attacks_count` and before `strike_interval_ticks`, in the struct
  and in `fields!(WeaponSpec { … })` (D-6).
- **Catalog.** `WeaponSpecDef` gains `#[serde(default)] first_strike_delay_ticks: u16`, with the doc "Omitted reads
  as 0, the engine default". It is converted unchanged. It is an integer, so no float token is added (C7
  allowances unchanged).
- **Why a default and not a required field** (unlike `strike_interval_ticks`). The value is 0 for every weapon but
  one. WP2's table test reads every shipped weapon against the data (P-5), so an omitted wrong value cannot pass
  silently. This follows `channeled` and `SplashSpec::impact`.
- **Alternative considered: a per-strike array** (`strike_periods_ticks: Vec<u16>`, mirroring `PeriodicPeriodArray`).
  It would subsume both scalars and represent the Thor's Explosive Payload (0, 2, 4, 2). It is rejected for now. It
  would rewrite every weapon row and every `WeaponSpec` literal, put a heap value into every weapon copy in the hot
  path, and describe no shipped weapon better than two scalars do. It is recorded as F-2's hook, to be taken up when
  a weapon with uneven periods is modelled.

### D-2. The rule: a delayed burst keeps every strike pending (B-81)

In `begin_strikes`:

```text
if weapon.first_strike_delay_ticks > 0:
    entity.pending_strikes = Some(PendingStrikes { target_id, plane,
        remaining: weapon.attacks_count,
        ticks_until_next: weapon.first_strike_delay_ticks,
        channeled: weapon.channeled })
    return 0
otherwise: today's rule
```

`advance_weapon_cycles` pushes a `PreparedShot` only when `begin_strikes` returns more than 0. In
`advance_pending_strikes`, a strike that falls due lands as today, with one change. **The number that lands on a due
tick is `remaining` when the weapon's interval is 0, otherwise 1**, and the burst is re-queued only when strikes remain
and the interval is above 0. So a delayed volley (interval 0, several strikes) lands whole on its due tick, as one
`PreparedShot` with that many strikes, exactly as it lands on the release tick today, only shifted.

**Consequences, each intended:**
- The first strike is validated on its due tick like every later strike (§2.2 rule 2). Its target must still be
  available and on the plane, and the distance condition applies if the weapon names one.
- An explicit order in the delay ends a channeled burst before its first strike (§2.2 rule 3, Q0058). The Liberator's
  burst is channeled.
- The missile leaves from the attacker's position on the due tick (`origin: entity.position`), §2.2 rule 4.
- The weapon is re-resolved on the due tick (upgrades completed in the delay apply), as for later strikes today.
- A burst still pending at the next release is replaced, as today. Validation (D-3) keeps every shipped burst inside
  its period.

**Invariant (P-1).** For any weapon and any target that stays available, adding a delay `d` moves every strike of a
burst exactly `d` ticks later and changes nothing else. The count, damage, splash, projectile and presentation
events are the same.

### D-3. Validation

In `validate_strikes`:
1. The burst's last strike lands at `first_strike_delay_ticks + (attacks_count − 1) × strike_interval_ticks` ticks
   after the release, and this must be below `period_ticks`. The message names `first_strike_delay_ticks` when the
   delay alone, or together with the interval, breaks the bound.
2. A delay is allowed on a single-strike weapon. It means "the one strike lands `d` ticks later".
3. `followup_max_distance` needs `strike_interval_ticks > 0` **or** `first_strike_delay_ticks > 0`, since a delayed
   first strike is also validated later. The field's doc becomes "at which a strike that lands after the release
   tick still lands".
4. A delay on a `suicide_on_attack` weapon is refused. The attacker is removed at dispatch, so a delayed burst would
   have no attacker to land from.

Each refusal names the field. Each has a unit test in `validation.rs`.

### D-4. Data (B-81)

`sky_bastion` air weapon: `first_strike_delay_ticks: 1` (S5; 0.893 → 1), with a comment citing `void`
EffectData:2942–2970 and WeaponData:328. `strike_interval_ticks` stays 1 and `channeled` stays `true`. Its burst
then ends 2 ticks after the release, within its period of 32. No other row changes (§2.3).

### D-5. The Colossus

No change, by the owner's answer to Q0064 (S8). P-5's table records the Colossus with delay 0, citing Q0064 as the
reason rather than the data. The B-81 register row is closed with the Colossus part named as the owner's choice.

### D-6. Hashed state and checksum

The new field is hashed with `WeaponSpec`, in its struct position (D-1), so every projectile's encoding gains two
bytes. `PendingStrikes` already holds everything a delayed burst needs: `remaining` may now equal `attacks_count`, and
its doc changes accordingly. No entity field is added. The schema is bumped once, in WP1, to the next free number at
merge (§5).

### D-7. What the simulation publishes

Nothing new. The `ProjectileVisual` (or tracer, beam or blow) of a delayed strike is published on the tick it
launches, because publication happens in `dispatch_shot`. The swing (`AttackSwing`, from `weapon_phase`) is
unchanged. For the Liberator this means its two `ProjectileVisual`s appear 1 tick after the swing's release instead
of on it (§9).

### D-8. Records

- `DESIGN.md` §4: **B-81 fixed** by this plan, with the Colossus part closed by Q0064.
- No new row is registered. The deferred chains (F-1 to F-3) belong to existing rows or unmodelled units: B-22 for
  the Carrier, the deviation-fixes plan's Thor-mode note for the Explosive Payload, and no analogue for the
  Mothership beam. B-178 stays free.
- Deviation-fixes plan: B-81's row in its register points here. Evidence §5.1's "Not modelled, recorded" bullet
  stays as history.

---

## 5. Determinism and checksum

- **Integers only.** The field is a `u16` of ticks, and the rule changes only integer counters. No float is added
  (C7).
- **No new collection or iteration order.** Pending strikes are queued in each entity's turn of
  `advance_weapon_cycles`, as today.
- **Goldens.** Neither replay contains the Liberator (`golden_replay.ron`: `pulse_bot`, `rhino_beetle`, `psi_grey`,
  `railgun_crawler`, `flying_saucer`, `foundry_core_dome`; `golden_replay_orders.ron`: marines, an Observer analogue,
  Roach analogues, zealots). So behaviour cannot move. Only WP1's encoding and schema move every checkpoint of both,
  and the sandbox checksum.
- **WP1's proof** (the procedure of the splash-centres and production-queue plans):
  1. With the field left out of the encoding and the schema at 36, the tree reproduces every recorded checksum of both
     goldens and `DEFAULT_SANDBOX_CHECKSUM` (`crates/sim_fixtures/tests/fixture_tests.rs`). This is checked on native
     debug and release, and on `wasm32-wasip1` release and debug
     (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree path>"`).
  2. Then the field goes in, the schema becomes the next free number `N`, and the checksums are re-recorded under the
     header "Re-recorded for plan 2026-11-23-strike-delays WP1 (checksum schema 36 -> N): each weapon also encodes
     the delay of its first strike, so every checkpoint changed. No behaviour did: every shipped delay is 0, and with
     the field left out of the encoding at schema 36, both fixtures reproduce as recorded for plan
     2026-11-23-splash-centres WP1." The schema byte of the encoding unit tests moves with it. All four builds
     reproduce the new values.
  3. If `main` takes `N` first (other branches may take 37 and 38), the merge repeats step 2 with the next free
     number.
- **WP2 leaves both goldens and the sandbox unchanged.** A move is traced to its first checkpoint and entity, and an
  unexplained one stops the package.
- **wasm.** No dependency changes. **Cost:** one comparison per release. The throughput bench is unaffected and is
  checked in the package gate as usual.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure from a seed. Constructed tests find weapons by property (`attacks_count`, `projectile_speed`,
`first_strike_delay_ticks`, `channeled`) and build scratch weapons from shipped ones, never naming a slug in a rule.
The P-5 table names analogues, as `b2_strike_spacing_follows_starcraft` does.

- **P-1 (shift).** A scratch weapon is copied from a shipped multi-strike weapon. With delay `d` ∈ {1, 3} and the
  target held in place and alive, its strikes land on `T + d + k × interval` for `k` = 0 … `attacks_count − 1`, with
  the same per-strike damage as with `d` = 0 on `T + k × interval`. This is checked for an instant weapon and for a
  projectile weapon (by launch tick), and for interval 0 (the whole volley lands on `T + d`, in one shot) and interval
  above 0.
- **P-2 (nothing on the release tick).** With `d > 0`, no strike, projectile or delivery event of that attack exists
  on tick `T`. With `d = 0`, everything is as today; WP5's tests are guards and pass unchanged.
- **P-3 (validated when due).** In a delayed burst, the first strike is lost in each of these cases: the target is
  killed by another unit in the delay; the target becomes unavailable (cloaked and unrevealed, or boarded) in the
  delay; or, for a weapon with a distance, the target is beyond it when the strike falls due. Each later strike still
  tries, as today. If the attacker dies or boards in the delay, no strike lands.
- **P-4 (channel).** An explicit move order given in the delay, on a channeled delayed burst, lands no strike. An
  attack order on the same target keeps it. A non-channeled delayed burst lands all its strikes after a move order.
- **P-5 (the shipped values).** Every shipped weapon's `first_strike_delay_ticks` equals the first period of its 5.0.14
  burst (§2.3): 1 for the Liberator analogue, 0 for every other weapon, and 0 for the Colossus by Q0064. Each row cites
  its source.
- **P-6 (the Liberator, shipped).** A `sky_bastion` attacking a held air target (found by property: an air unit of
  the other faction) launches its first missile on `T + 1` and its second on `T + 2`, where `T` is the tick its
  weapon releases. Each missile's origin is the Liberator's position on its launch tick.
- **P-7 (validation).** D-3's four rules, each refused with the field named, and the boundary accepted
  (`delay + (n − 1) × interval = period − 1`).
- **P-8 (zero recompilation).** Setting the Liberator's delay to 0 in a catalog copy restores today's launch ticks in
  the same binary (`runtime_catalog_tests.rs`).
- **P-9 (confinement).** WP1 changes no world (all of §7 byte-identical; goldens and sandbox by the schema bytes
  only). WP2 changes only worlds that contain the Liberator analogue. Of the measured worlds, none does.
- **P-10 (direction in the probe, a prediction recorded before measuring, not a threshold).** Each volley lands one
  tick later and the attack period is unchanged. So in the probe battles (§7) the Liberators' side should win no more
  seeds than before, its mean end tick should not fall, and every seed's first differing tick should be one tick
  after a Liberator release. A result against this is traced (§8 rule 3).

---

## 7. Worlds that move

**Baseline:** WP0's tree (behaviour-identical to `cdf4ea93`), measured in WP0.

**Commands.** These are the splash-centres plan's commands. Binaries are built once per tree and run from that tree
so its assets are read. Outputs go under `~/scratch/strike-delays/`, and the scripts are copied from
`~/scratch/splash-centres/scripts/` (`measure.sh` includes the `showcase_effects` runs) and
`~/scratch/stance-orders/scripts/client.sh`:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/strike-delays/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/strike-delays/bin-<wp>/
~/scratch/strike-delays/scripts/measure.sh ~/scratch/strike-delays/bin-<wp> <tree> ~/scratch/strike-delays/world-<wp>
~/scratch/strike-delays/scripts/compare.sh ~/scratch/strike-delays/world-wp0 ~/scratch/strike-delays/world-<wp>
# the probe (not committed):
~/scratch/strike-delays/bin-<wp>/matchup_trace --fixture ~/scratch/strike-delays/liberator_probe.ron --all-battles --seeds 20 --out ~/scratch/strike-delays/world-<wp>/probe
```

Run in the background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is
compared by its stage log (`client.sh`).

**The probe** (`~/scratch/strike-delays/liberator_probe.ron`, written in WP0 in the matrix fixture's format, **verify**
the fields `--fixture` requires): `p1_4_liberators_vs_8_mutalisks` (a clumped air fight where the splash matters) and
`p2_2_liberators_vs_2_vikings_in_range` (spawned within 5 of each other, so range does not decide). It is a scratch
file, not a matrix battle (F-4).

| World | WP1 (rule, latent) | WP2 (data) |
|---|---|---|
| Matrix (16 seeds + seeds of record) | byte-identical | byte-identical: no battle spawns `sky_bastion` |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical | byte-identical |
| Showcase, default seed and 1–8 (6,000 ticks) | byte-identical | byte-identical: `battle_automaton` has no Liberator, and the run trains none (**verify** in WP0 by the unit census of the baseline output) |
| Showcase effects, default seed and 1–8 | byte-identical | byte-identical: `showcase_siege_line` and `showcase_raiders` have no Liberator |
| Client skirmish stage log | identical | identical: the pilot trains `pulse_bot` |
| Goldens, sandbox | schema bytes only (§5) | unchanged |
| Probe p1, p2 (20 seeds) | byte-identical | **moves** per P-10: wins, survivors, end tick, and the first differing tick of each seed, recorded before and after |
| Constructed worlds (§12) | P-1 to P-4, P-7 on scratch weapons | P-5, P-6, P-8 |

**What WP2 records** in the evidence: the probe's figures before and after, and for one seed of each battle, a trace
of the first differing tick (a Liberator release and the missing launch).

---

## 8. Stop rules

1. **WP1 is latent.** Every world of §7 is byte-identical, and the goldens and sandbox move by the schema bytes only,
   proved (§5). Anything else is a defect of WP1.
2. **WP2 moves only worlds with the Liberator analogue** (P-9). A difference in any measured world of §7 stops the
   package until it is traced. Such a world, by §3, contains no Liberator, so a move there means the rule leaked.
3. **A probe result against P-10**, or a first differing tick that is not one tick after a Liberator release, stops
   WP2 until it is traced. If the trace shows a second cause, it is registered as a bug, and WP2 lands only if the
   delay is separable from it.
4. **No tuning constant.** The only number is S5's, converted by S-5.
5. **A golden move in WP2** is traced as in §5, or the package stops.

---

## 9. Client consequences

- **Presentation timing changes for one weapon.** The Liberator analogue's two `ProjectileVisual` events (its
  `rocket_trail` travel look, then the `flak_burst` splash at impact) are published one tick later relative to the
  swing's release. The `Attack` flipbook pose, driven by `AttackSwing` from the weapon phase, is unchanged. So the
  rockets now leave 40 ms after the pose's release frame, as in 5.0.14. No event kind, field or schema changes.
- **Code:** none. **Data:** none in the client's tables; the Liberator's effects block (`units.ron`) is unchanged.
- **Tests.** `effect_snapshot_tests::every_shipped_weapon_publishes_its_delivery_or_splash` collects events until the
  first one appears plus a window (its `fire` loop), so a 1-tick-later event passes (**verify** in WP2).
  `combat_correctness_tests::s4_projectile_weapon_applies_every_strike` picks the first shipped ground multi-strike
  projectile weapon (the Banshee analogue, delay 0) and is unchanged. The client's `effect_catalog_tests` read looks,
  not ticks. The package gate runs the client tests that its path rules select.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP3 | D-8 |
| `docs/plans/README.md` | plan, WP1, WP3 | Row |
| `crates/sim_types/src/weapon.rs` | WP1 | The field and its doc; `strike_interval_ticks`'s and `followup_max_distance`'s docs (D-1, D-3) |
| `crates/sim_core/src/catalog.rs` | WP1 | `WeaponSpecDef` field and conversion |
| `crates/sim_core/src/catalog/validation.rs` | WP1 | D-3 and its unit tests |
| `crates/sim_core/src/combat_resolution.rs` | WP1 | D-2 (`begin_strikes`, the push in `advance_weapon_cycles`, `advance_pending_strikes`), docs |
| `crates/sim_core/src/entity.rs` | WP1 | `PendingStrikes` docs (`remaining` may be the whole burst; the first strike may be pending) |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-6, schema `N` |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` (`DEFAULT_SANDBOX_CHECKSUM`); the encoding unit tests' schema byte | WP1 | §5 |
| `crates/sim_fixtures/src/trace/mod.rs` | WP1 | Strikes on the release tick: 0 when delayed (§3) |
| `crates/sim_fixtures/tests/trace_tests.rs` | WP1 | The trace's rule for a delayed instant scratch weapon |
| Test files with `WeaponSpec { … }` literals (`sim_core/tests`: `attack_dash`, `attack_move_simulation`, `attribute_bonus`, `barrier`, `burrow_attack`, `checksum_state`, `combat_correctness`, `combat_retaliation_and_projectile`, `crowd_edge`, `damage_and_shields`, `hull_turning`, `multi_strike_timing`, `sc2_battle_benchmarks`, `second_line`, `spells_and_abilities`, `target_selection`, `weapon_cadence`, `weapon_catalog_rules`; `sim_fixtures/tests/trace_tests`) | WP1 | `first_strike_delay_ticks: 0` (mechanical; the list is `grep -rln strike_interval_ticks crates/*/tests`) |
| `crates/sim_core/tests/multi_strike_timing_tests.rs` | WP1, WP2 | §12 |
| `crates/sim_core/tests/checksum_state_tests.rs` | WP1 | The field changes the checksum |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP2 | `SC2_FIRST_STRIKE_DELAY`, `b81_first_strike_delay_follows_starcraft` (P-5) |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP2 | P-8 |
| `assets/catalog/units.ron` | WP2 | D-4 |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP3 | B-81's row points here |
| `docs/evidence/2026-11-23-strike-delays.md` (new) | WP0 to WP3 | §7 |

Nothing under `crates/sim_core/src/movement/` changes; another branch works there. `crates/gate/suites.ron` needs no
change, because every test target touched already exists (**verify** in WP1).

---

## 11. Work packages (commit order)

Each package runs on one branch from `main`, in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` (check `quota -s` and `df -h ~/scratch` first). Failing
tests come first, with their red run pasted in the hand-back. Then `cargo gate quick` and the tests the change needs,
and a verifier runs `cargo gate package` (Q0061). §7's comparison is run in every package against WP0's tree. No
`unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust (C5); tests find weapons by
property. Check `docs/questions/open/` before the last commit. All four packages may run as one branch and one
session, one commit each, with one `package` gate on the final tree (charter §4.3).

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-81's row points to this plan. Evidence file created, with §2.3's survey and the trace script's
  path.
- Write the probe fixture. Measure WP0's tree (§7, probe included) for the baseline, and take the showcase unit census
  (§7's **verify**).
- **Gate:** docs-only.

### WP1: the rule, latent (M, `sim_types`, `sim_core`, `sim_fixtures`)

Failing tests first. They are red on WP0's tree because they do not compile; the red run then shows the rule missing
once a field-only stub compiles. They go in `multi_strike_timing_tests.rs`, with scratch archetypes as that file
already builds them:
- `a_delayed_burst_lands_every_strike_after_the_delay` (P-1, interval > 0, `d` ∈ {1, 3}, instant).
- `a_delayed_volley_lands_whole_after_the_delay` (P-1, interval 0, two strikes, one shot).
- `a_delayed_single_strike_lands_after_the_delay` (D-3 rule 2).
- `a_delayed_projectile_burst_launches_from_where_its_attacker_is` (P-1 by launch tick; the attacker moved in the
  delay; origin checked).
- `nothing_lands_on_the_release_tick_of_a_delayed_burst` (P-2).
- `a_delayed_first_strike_is_validated_when_due` (P-3: target killed, target cloaked, beyond the distance, attacker
  killed).
- `an_order_in_the_delay_ends_a_channeled_burst` and `a_delayed_burst_that_is_not_channeled_survives_an_order` (P-4).
- `validation.rs` unit tests (P-7): the bound with the delay; a delay on one strike accepted; a distance with a delay
  and no interval accepted; a delay on a suicide weapon refused; an omitted field reads 0.
- `checksum_state_tests`: the field changes the checksum of a world with a projectile.
- `trace_tests`: a delayed instant scratch weapon's release replays 0 strikes.

Then the change: D-1, D-2, D-3, D-6, the docs, the trace tool, and the mechanical literals. Then §5's proof.
**Acceptance:** §8 rule 1; WP5's existing tests in `multi_strike_timing_tests.rs` pass unchanged as guards. Plan row
`Active`.

Commit `feat(sim_core): a weapon may delay its first strike (strike-delays WP1, schema N)`.

### WP2: the Liberator's first missile (S, data, B-81)

Failing tests first (red on WP1's tree):
- `weapon_catalog_rules_tests::b81_first_strike_delay_follows_starcraft` (P-5): the table `SC2_FIRST_STRIKE_DELAY`
  of `(analogue, delay, source)`, with the Liberator at 1 and every other modelled analogue implied 0. The test's doc
  cites §2.3's zero-period sources and Q0064 for the Colossus. It is red on the Liberator.
- `multi_strike_timing_tests::the_liberators_missiles_leave_after_its_first_period` (P-6) on the shipped row.
- `runtime_catalog_tests` (P-8): the delay set to 0 in a catalog copy restores launch on the release tick.

Then the data (D-4). **Acceptance:** §8 rules 2 and 3; §7 column WP2 with the probe recorded. Commit
`fix(sim_core): the Liberator analogue's first missile waits its burst's first period (strike-delays WP2, B-81)`,
stating that no measured world moved and how the probe moved.

### WP3: bookkeeping (XS)

D-8's records. The evidence is completed, this plan's `## As built` is written, and the README row is set to `Done`.
Merge `main`; `cargo gate package` runs once on the final tree (by the verifier). The schema is re-recorded if `main`
took the number (§5 step 3).

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | The default 0, D-3's four rules and the boundary | WP1: `catalog/validation.rs` unit tests |
| Rule | P-1 to P-4 | WP1: `multi_strike_timing_tests.rs` |
| Encoding | The field changes the checksum | WP1: `checksum_state_tests.rs` |
| Trace tool | Delayed releases replay no strike | WP1: `sim_fixtures/tests/trace_tests.rs` |
| Data | Every weapon's first delay against 5.0.14 | WP2: `weapon_catalog_rules_tests.rs` |
| Shipped behaviour | P-6 | WP2: `multi_strike_timing_tests.rs` |
| Zero recompilation | P-8 | WP2: `runtime_catalog_tests.rs` |
| Worlds | P-9, P-10: §7's table | every package (measure scripts, probe); goldens and sandbox in the package gate |
| Charter subjects | `sim_core`'s subjects of `docs/guides/test-gates.md` (`stutter_step_tests.rs`, `turret_tracking_tests.rs`, `damage_and_shields_tests.rs`, `push_priority_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`) and `sim_fixtures`' `fixture_tests.rs` keep passing; `client`'s skirmish | package gate |

Expected to be re-derived: none beyond the mechanical literals. The guards that must pass unchanged are
`b2_strike_spacing_follows_starcraft` and WP5's seven `multi_strike_timing_tests` (the Liberator's interval stays 1),
plus `s4_projectile_weapon_applies_every_strike`. Any test that asserts the Liberator's launch on its release tick
would be re-derived from D-2; none was found by `grep -rn "sky_bastion" crates/sim_core/tests` (**verify** in WP2).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The reading that entry 0 delays the first strike (S1) is wrong for an array of one entry | The Liberator's missiles at `T`, `T + 1` after all | S1 is the data's own patch pattern; one data value; P-8 shows the switch |
| R-2 | A delayed strike's conditions differ from 5.0.14 (a persistent on a dead `TargetUnit`) | A rare lost missile | §2.2 rule 2 **verify**; one tick's window; the conditions are the existing ones for later strikes |
| R-3 | Another branch takes schema 37 (or 37 and 38) | Conflict at merge | "Next free at merge"; §5 step 3 |
| R-4 | The literal sites in tests (about 20 files) conflict with other branches | Merge noise | Mechanical one-line additions; merge `main` before WP3's gate |
| R-5 | A probe fixture field the trace tool needs is missing | Probe does not run | Copy a matrix battle's block verbatim and change units and positions only |
| R-6 | Interval-0 volleys now land `remaining` strikes from a pending burst | A later weapon with a delay and interval 0 behaves as P-1 says | P-1's interval-0 case; no shipped weapon combines them |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: the Carrier's interceptor launch delay** (0.375 Normal s, 7 ticks; then 0.5, 9 ticks apart; `voidmulti`
  EffectData:428, `liberty` :893–902). Ours is one 8-strike salvo, a stand-in for interceptors (B-22). Putting a delay
  on the stand-in would fit a number to a model the game does not have. Hook: when B-22 models interceptors, their
  launch persistent's first period is this plan's `first_strike_delay_ticks` (or F-2's array) on the launch weapon.
- **F-2: uneven periods** (the Thor's Explosive Payload, `JavelinMissileLaunchersPersistent` 0, 0.125, 0.25, 0.125 →
  0, 2, 4, 2 ticks, `liberty` EffectData:2259–2270). It is not modelled: the catalog carries the High Impact weapon.
  Hook: replace the two scalars with a per-strike array (D-1's alternative) when the mode is modelled.
- **F-3: the Mothership's secondary beam** (`InitialDelay` 0.25, `liberty` EffectData:934–943). It has no analogue
  weapon.
- **F-4: a matrix battle with the Liberator analogue.** It is not added. The probe shows the move, and a new matrix
  battle needs expectations and a claim of its own; it would belong to the Defender Mode work (WP18, B-27).
- **The Colossus** (Q0064) and the Lurker's spines (B-85, fixed): no change.
- **Missile flight** (mover speed, acceleration, `ReturnDelay`): the projectile model's subject, not a strike delay.
- **The burst bound against random delays.** Validation compares the burst with `period_ticks`. A negative
  `random_delay_min` (the Liberator's −1.1 ticks) can bring the next release a tick or two sooner. Every shipped burst
  ends at least 25 ticks before its period, so this is recorded and not tightened.

---

## 15. Owner question

None is needed. The value is in the data (S5), the conversion is sanctioned (S-5), the Colossus is the owner's answer
(Q0064), and the channel rule is the owner's answer (Q0058). The one open point, a persistent whose target unit dies
(§2.2 rule 2), affects at most one tick per volley and follows the rule already applied to every later strike, so it is
marked **verify**. It is not asked. Q0100 stays free.

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read as data.
- The effect-chain trace script and its output for every analogue weapon, copied by WP0 to
  `~/scratch/strike-delays/chain.py` and `~/scratch/strike-delays/chains-*.txt` (run with `python3 -I`). The planning
  copies are in the planning session's scratchpad.
- The probe fixture `~/scratch/strike-delays/liberator_probe.ron` (WP0).
- No prototype: WP1 is latent by construction, and WP2 acts on no measured world.

---

## As built

Implemented on branch `strike-delays-plan`, one commit per package, then merged with `main`:

| Package | Commit | What |
|---|---|---|
| WP0 | `3b748790` | B-81's register row points here; evidence file with the survey material, the probe fixture and the baseline (483 files) |
| WP1 | `bec1f37c` | D-1, D-2, D-3, D-6, the trace tool, the mechanical literals; schema 37 on `cdf4ea93` with the proof; every measured world and the probe byte-identical |
| WP2 | `b8d20d12` | D-4: `sky_bastion`'s air weapon `first_strike_delay_ticks: 1`; P-5, P-6, P-8; no measured world moved; the probe moved |
| merge | `36fe3f1a` | `main` at `e8cc3f80` took schema 37 (splash cliff levels): renumbered to **38**, proof repeated (§5 step 3) |
| WP3 | this commit | D-8's records, the evidence, this section, the index row `Done` |

Measurements, red runs and the proofs are in [`docs/evidence/2026-11-23-strike-delays.md`](../evidence/2026-11-23-strike-delays.md).

**Deviations from the text above, none of which changes a decision:**

- **Mechanical literals (§10).** Only thirteen test files construct `WeaponSpec` or a scratch weapon row that needed
  `first_strike_delay_ticks: 0`; `combat_correctness`, `sc2_battle_benchmarks` and `weapon_catalog_rules` read the
  field set only by name and needed no literal. `checksum_state_tests` gained the field in its mutation list and a
  test of its own (`a_projectiles_first_strike_delay_changes_the_world_checksum`).
- **Trace tool.** The rule for the strikes a release lands became a public function,
  `sim_fixtures::trace::strikes_on_release`, so that `trace_tests` checks it on copies of shipped instant weapons
  (found by property) with a delay; the wasted-shot replay calls it.
- **Validation (D-3).** The refusal of a delay on a suicide weapon and the bound are checked in `validate_strikes`;
  the bound names `first_strike_delay_ticks` whenever the delay is above 0 and the burst breaks the bound, and
  `strike_interval_ticks` otherwise, so WP5's message for an undelayed burst is unchanged.
- **Probe output.** The probe was written beside each measured world (`world-<wp>-probe/`) rather than inside it,
  so that `compare.sh` compares the measured worlds only.
- **The first differing tick (P-10, stop rule 3).** The per-tick trace samples units, not projectiles, so the first
  observable difference of each probe seed is the impact of the first Liberator missiles, not their launch one tick
  after the release. In all 48 seeds that difference is red life arriving one tick later (or a missile launched a
  tick later striking a different set of Mutalisks); no other field differs first.
- **P-10's end tick.** p1's mean end tick fell by 0.5 ticks (472.9 → 472.4; 16 seeds later, 7 earlier), against the
  prediction, while wins (24 → 23) and survivors (1.67 → 1.62) moved as predicted. The fall comes from two long
  chases (seeds 4 and 18) whose kill timelines start one tick later and then diverge. The trace found no second
  cause (evidence, WP2), so WP2 landed under stop rule 3.
- **Client skirmish stage log (§7).** Not compared: the implementation hand-back runs `cargo gate quick` and the
  simulation targets; the pilot trains `pulse_bot`, and no measured simulation world moved. The package gate's
  client selection covers the skirmish.
- **`crates/gate/suites.ron`** needed no change (§10's verify): every test added is in an existing target.
