# Blueprint: Fire Rules — the Locked Turret, Targets Off the Attack Order, and Turrets Between Shots (B-179, B-180, B-197)

This plan settles two rows that the turret wind-up plan registered and deferred (its §2.4 and §14 F-2), against
StarCraft II 5.0.14, and a third row that the same data expose:

- **B-179: the Hellion's turret is locked while it fires.** `InfernalFlameThrower` sets `LegacyOptions
  LockTurretWhileFiring 1` (`liberty` WeaponData:590), the only weapon of the eight layers that does. Ours slews the
  turret through the wind-up (`step_windup`, `crates/sim_core/src/combat.rs`).
- **B-180: a weapon may fire at targets other than its attack order's.** Every weapon defaults to
  `OnlyFireAtAttackOrderTarget 1` (`core` WeaponData:21). Four modelled weapons clear it: the Phoenix's
  `IonCannons`, the Immortal's `PhaseDisruptors`, the Colossus' `ThermalLances` and the Planetary Fortress'
  `TwinIbiksCannon` (§2.3). Inside an attack order, such a weapon may work against another enemy when it cannot work
  against the order's target. Ours always works against the order's target (`SimWorld::ordered_target`,
  `crates/sim_core/src/combat_resolution.rs`), so a Phoenix analogue chasing a fleeing air unit flies past others in
  reach without firing.
- **B-197 (registered here): a turret does not face its target while its weapon cools down.** Every legacy weapon
  carries `LegacyOptions FaceTargetWhileInCooldown 1` (the `CWeaponLegacy` default, `core` WeaponData:34), and none
  clears it. Ours freezes a turret from the release of a shot until its cooldown ends (`WeaponPhase::Cooldown` steps
  nothing, `step_backswing` slews only once the cooldown is 0). This row gives B-179 its meaning: a lock is the
  absence of the tracking that every other turret does between shots.

All three become rules over catalog data and one universal default. `WeaponSpec` gains
`fires_at_other_targets` (the inverse of `OnlyFireAtAttackOrderTarget`) and `locks_turret_while_firing`; each entity
gains a `weapon_target`, the enemy its weapon chose itself inside an attack order. No code names a unit (charter
§3.3, C5).

**What this plan does not do: the movement half.** Whether a unit under an attack order is walking (and so whether a
`Stopped` or `Slowing` weapon may fire at another target) is read here from the order and the reach to its target
(D-4), not from pursuit, which lives under `crates/sim_core/src/movement/` and is owned by the contact-solver branch.
WP6 replaces that reading by pursuit's own **after the contact solver merges**, and only if WP3's probe finds the two
differ. Nothing else touches `movement/`; the Phoenix's wind-up hold stays turret-wind-up WP3's (its Amendment 1).

A full plan is required (charter §6): new rules in the simulation, a change of hashed encoding (checksum schema),
golden replays and matrix battles that move by behaviour (B-197), and work across `sim_types`, `sim_core`, the
catalog and the fixtures.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`7dc356fc` (checksum schema 43; 44 reserved by the hidden-harvesters work).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-5 25 ticks per Faster second, whole-tick rounding) | Closed | D-1 (no new duration) |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule | §2.1 S9–S11 (the editor's meaning of the three options) |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.5, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0101, Q0106: vision and weapon filters | Open; defaults built | D-4 (the order's target "cannot be worked" includes "not seen", as built by B-185) |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| **Q0109** (draft, §15): what the lock covers, whether turrets track between shots, how a weapon picks a target off its order, and an immobile unit's order on a target out of reach | To be filed; defaults followed meanwhile | D-2, D-3, D-4, D-5 |

---

## 1. Summary

| | Today (`7dc356fc`) | After this plan |
|---|---|---|
| Turret of a weapon with `locks_turret_while_firing` (the Hellion analogue) | Slews through the wind-up; frozen from release to the end of the cooldown, as every turret | Does not turn from the start of a swing to the end of its backswing (D-2, Q0109 part 1); tracks its target in cooldown as every turret does after WP4 |
| Turret of any other weapon between shots | Frozen from release until the cooldown runs out; slews only once `Ready` | Faces its target, at its own rate, through the backswing and the cooldown (B-197, D-3) |
| Weapon with `fires_at_other_targets` whose attack order's target it cannot work against this tick (out of its engagement band, or failing the weapon filter) | Works against nothing | Works against `weapon_target`: the best enemy it reaches from where the unit is, ranked as acquisition ranks, while the unit is not walking or the weapon is `Moving` (D-4, D-5) |
| The same weapon when the order's target is workable | Works against it | Unchanged: the order's target first (Q0109 part 3) |
| `WeaponSpec` | … `allowed_movement`, `fires_outside_attack_order`, … | + `fires_at_other_targets: bool`, `locks_turret_while_firing: bool`; both omitted in RON read the engine default (`false`) |
| `Entity` | `current_target` | + `weapon_target: Option<EntityId>`, set and released by acquisition only |
| Hashed state | — | The two fields in each projectile's `WeaponSpec`, `Entity::weapon_target`; schema **next free at merge** (45, 44 being reserved) |
| Worlds that move | — | WP1: none (schema bytes only). WP2 (lock): none expected beyond turret angles, which no measured output records (probe a). WP3 (off-order): none expected (probe b: no Phoenix or Planetary Fortress analogue in a measured world, Immortals and Colossi chase). WP4 (B-197): every world with a turret whose target moves between shots (probe c), the goldens ("red_siege") by `turret_angle` |
| Size | — | M: six packages now (WP0 to WP5, each XS to M), and WP6 after the contact solver, conditional |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [INF] inferred from the data; [COMM] community or editor documentation (Q0008);
**verify** not confirmed.

### 2.1 Sources

| # | Source | What it gives |
|---|---|---|
| S1 | SC2Mapster SC2GameData at `5444e12f13` ("SC2 5.0.14.94342"), copy at `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`. `liberty WeaponData:N` is line N of that layer's file; the override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti` | Every [DATA] value |
| S2 | `CWeapon` default (`core WeaponData:3–31`): `Options OnlyFireWhileInAttackOrder 1` (:20), `OnlyFireAtAttackOrderTarget 1` (:21), `CanInitiateAttackOrder 1` (:19) | By default a weapon fires only in an attack order and only at its target [DATA] |
| S3 | `CWeaponLegacy` default (`core WeaponData:33–39`): `LegacyOptions FaceTargetWhileInCooldown 1` (:34); `UninterruptibleDuration 32767.9997` (:38) | Every legacy weapon faces its target while it cools down [DATA]; no layer clears the option (`grep FaceTargetWhileInCooldown */WeaponData.xml`: only :34) |
| S4 | `InfernalFlameThrower` (`liberty WeaponData:582–595`): `DamagePoint 0.25` (:588), `Backswing 0.75` (:589), `LegacyOptions LockTurretWhileFiring 1` (:590), `Effect InfernalFlameThrowerCP` (:591); later layers add only `MinScanRange 5.5` (`voidmulti :115–117`, `balancemulti :108–110`). On the `Hellion` turret (`liberty UnitData:11603`), `FreeRotate`, `YawRate 999.8437` (`liberty TurretData:95–98`), `Idle Hold` (`libertymulti TurretData:3–5`) | The one weapon that locks its turret [DATA]; `LockTurretWhileFiring` appears nowhere else in the eight layers |
| S5 | `InfernalFlameThrowerCP` (`liberty EffectData:2617–2648`): a `CEffectCreatePersistent` whose 24 searches lie on `PeriodicOffsetArray 0,-0.25,0` … `0,-6,0` (:2623–2646), period 0 (:2647); the default orients offsets from `OffsetVectorStartLocation SourceUnit` to `OffsetVectorEndLocation TargetPoint`, facing fallback `SourceUnit` (`core EffectData:34–40`) | The flame's line runs from the Hellion to the target point, not along the turret: the lock changes where the turret points, not where the flame lands [DATA + INF] |
| S6 | The weapons that clear `OnlyFireAtAttackOrderTarget` (every `Options index="OnlyFireAtAttackOrderTarget" value="0"` of the eight layers, mapped to its weapon, XML comments stripped): `IonCannons` (`libertymulti :21`, `swarm :178`), `PhaseDisruptors` (`voidmulti :206`, `balancemulti :186`), `ThermalLances` (`voidmulti :257`, `balancemulti :230`), `TwinIbiksCannon` (`liberty :140`), `90mmCannonsFake` (`voidmulti :12`, `balancemulti :12`), `ATALaserBattery`, `ATSLaserBattery`, `BattlecruiserWeaponSwitch` (`balancemulti :24`, `:35`, `:46`; `voidmulti` the same), `PrismaticBeam` (`liberty :126`), `MothershipBeam` (`liberty :525`), `MothershipPurifyWeapon` (`swarm :39`), `BuildingShield` (`swarm :83`), `VoidRaySwarmEnhanced` (`void :192`, `swarmmulti :130`). `VoidRaySwarm`'s two are inside `<!-- … -->` (`void :172–173`, `swarmmulti :110–111`). No layer sets either option back to 1 | §2.3 decides which are modelled [DATA] |
| S7 | Every weapon of S6 but `TwinIbiksCannon` also clears `OnlyFireWhileInAttackOrder`; `TwinIbiksCannon` clears only the target option (`liberty :140`) | The two options are independent in the data: the Planetary Fortress fires only in an attack order, but at any target in it [DATA] |
| S8 | `ThermalLances` `AcquirePrioritization ByDistanceFromTarget` (`liberty WeaponData:498`); ours already reads it as the Colossus analogue's `acquire_tie_break: ClosestToPrevious` (`units.ron`) | The Colossus' weapon ranks its own candidates by their distance from its target [DATA]; the other three keep the default (`grep AcquirePrioritization`: none on them) |
| S9 | Editor options "Only Fire While Attacking" and "Only Fire At Attack Target": unchecked, a weapon fires at targets it acquires itself, not only the attack ability's target; map-makers clear both for attack-while-moving | [COMM] SC2Mapster forum threads (§16); the turret wind-up plan's S11. What the weapon prefers when both its own choice and the order's target are workable is not stated (**verify**, Q0109 part 3) |
| S10 | "Lock Turret While Firing": no definition found in the data or the community sources searched (§16). The name and S3 together read: while the weapon is firing, its turret does not turn; between shots, it faces its target | [INF], **verify** which phases "firing" covers (Q0109 part 1) |
| S11 | Turret tracking between shots: a Siege Tank's turret follows a moving target between shots in play; S3 is the data behind it | [COMM] common play knowledge; [DATA] S3 |
| S12 | `90mmCannonsFake` (`balancemulti WeaponData:7–20`): `Hidden 1`, `CanInitiateAttackOrder 0`, both order options 0, `LinkedCooldown 0`, `Effect 90mmCannonsDummy` (`balancemulti EffectData:1066–1072`: a `CEffectDamage` with no amount, `Visibility Hidden`, no acquire or flee response); on the `SiegeTank` beside `90mmCannons` (`balancemulti UnitData:1473`, `voidmulti :1575`) | A hidden, harmless second weapon that lets the unsieged tank's turret work against any enemy under any order [DATA]; what it shows is **verify** (registered B-198, §2.5) |

### 2.2 The rules that follow

1. **A locked turret holds while its weapon fires** (S4, S10). From the tick a swing starts (`Windup`) to the end
   of its backswing, a weapon with `LockTurretWhileFiring` does not turn its turret. The swing releases as today: the
   release checks the engagement band, not the arc, and the flame's line runs to the target point (S5), so a lock
   changes no damage [INF]. A backswing that the next swing interrupts (`step_backswing` starts it from the ready
   step once the cooldown is 0) ends the firing, and the new swing may turn the turret as any ready weapon does.
2. **Every other turret faces its target between shots** (S3, S11). Through the backswing and the cooldown, a
   turreted weapon with a target turns its turret towards it at the turret's rate, as it does while `Ready`. A hull
   weapon's hull is turned by pursuit in its band already (`Pursuit::aim_if_in_range`, `movement/locomotion.rs`), so
   S3 needs nothing new for it [INF, **verify** in WP4 that every hull case is covered].
3. **A weapon that may fire at other targets prefers the order's target** (S2, S6, S9). Inside an attack order, a
   weapon with `OnlyFireAtAttackOrderTarget 0` works against the order's target whenever it can (in its engagement
   band and passing its filters); otherwise against the best enemy it reaches from where the unit is [INF,
   **verify**, Q0109 part 3].
4. **Its own target is ranked as acquisition ranks** (S8). By `TargetPriorityKey` and the archetype's tie-break,
   where "the previous target" of `ClosestToPrevious` is the order's target (the Colossus' `ByDistanceFromTarget`)
   [INF].
5. **Movement still gates it** (S2, the turret wind-up plan's rule 3). A weapon that is not `Moving` fires at its own
   target only while the unit is not walking; under an attack order, a mobile unit walks while its order's target is
   out of its weapon's reach (it chases). So the Phoenix's weapon fires at others while it chases; the Immortal's
   and Colossus' only while they stand (the order's target in reach but not workable, as an unseen one under B-185);
   the immobile Planetary Fortress' never under today's rule 6 [INF from the data's values].
6. **An immobile unit's order on a target it cannot work ends at once, as built** (`decide_acquisition`'s
   `AbandonAttack` for an `Explicit` immobile unit whose target fails `reaches_from_here` or the weapon filter, B-185).
   So under today's rule the Planetary Fortress' option never acts: its order ends on the tick its target leaves
   reach or sight, and acquisition picks the best enemy in reach as an automatic attack. Its data row is recorded
   (D-6) so that the option acts if 5.0.14 keeps such an order (**verify**, Q0109 part 4, answer (b)).

### 2.3 Survey: the two options for every modelled weapon

Every other modelled weapon keeps both defaults (S2, S3): `OnlyFireAtAttackOrderTarget 1` and no
`LockTurretWhileFiring`.

| Analogue (slug) | 5.0.14 weapon | `OnlyFireAtAttackOrderTarget` | `LockTurretWhileFiring` | `OnlyFireWhileInAttackOrder` (as built) | `AllowedMovement` (as built) | Turret | Decision |
|---|---|---|---|---|---|---|---|
| **Phoenix** (`flying_saucer`) | `IonCannons` | **0** (`swarm` :178; `libertymulti` :21) | — | 0 | Moving | yes | **In:** `fires_at_other_targets` |
| **Immortal** (`void_goliath`) | `PhaseDisruptors` | **0** (`balancemulti` :186; `voidmulti` :206) | — | 0 | Slowing | yes | **In:** `fires_at_other_targets` |
| **Colossus** (`tripod_annihilator`) | `ThermalLances` | **0** (`balancemulti` :230; `voidmulti` :257) | — | 0 | Stopped | yes | **In:** `fires_at_other_targets` |
| **Planetary Fortress** (`planetary_bulwark_dome`) | `TwinIbiksCannon` | **0** (`liberty` :140) | — | 1 (S7) | Stopped | yes | **In:** `fires_at_other_targets` (structure row) |
| **Hellion** (`napalm_wheeler`) | `InfernalFlameThrower` | 1 | **1** (`liberty` :590) | 1 | Stopped | yes | **In:** `locks_turret_while_firing` |
| Siege Tank (`railgun_crawler`) | `90mmCannonsFake` (second weapon) | 0 | — | 0 | Stopped | yes | **Out**, registered B-198 (S12; a harmless hidden weapon) |
| Void Ray (`prismatic_saucer`) | `VoidRaySwarm` | 1 (the 0 is commented out) | — | 1 | Moving | no | Defaults (the turret wind-up plan's S6) |
| Carrier / Mothership (`mothership_carrier`) | interceptors (B-22) | — | — | — | — | — | Defaults; our weapon is a stand-in salvo (B-22); `MothershipBeam` is not modelled |

Not modelled (no analogue): `ATALaserBattery`, `ATSLaserBattery`, `BattlecruiserWeaponSwitch`, `PrismaticBeam`,
`MothershipBeam`, `MothershipPurifyWeapon`, `BuildingShield`, `VoidRaySwarmEnhanced`.

**Validation that follows from the data.** `locks_turret_while_firing` on a weapon without a turret is refused (no
turret to lock; S4's only row is turreted). `fires_at_other_targets` on a `suicide_on_attack` weapon is refused, as
the turret wind-up plan's D-8 refuses `fires_outside_attack_order` there (a unit would detonate on a target it never
chose; no 5.0.14 weapon combines them). `fires_outside_attack_order` without `fires_at_other_targets` is not in the
data (S7 has only the opposite) but is coherent (outside an attack order there is no order's target) and is not
refused.

### 2.4 What the rules change, case by case

| Case | Rule | Who, in shipped data |
|---|---|---|
| A turret between shots, target moving | 2 | Every turreted weapon: Siege Tank (both modes), Phoenix, Hellion (outside its lock), Immortal, Colossus, Lurker (burrowed), Planetary Fortress, the turreted defence structures (`flak_array_dome`, `tesla_sentinel_dome`, `thorn_lasher_dome`, `spore_spire_dome`) |
| A turret during its own swing and backswing | 1 | Hellion only |
| Chasing an order's target, another enemy in reach | 3, 5 | Phoenix (fires at it); Immortal and Colossus (do not: they walk) |
| Standing in reach of an order's target it cannot work (unseen) | 3, 5 | Phoenix, Immortal, Colossus (the Planetary Fortress' order ends instead, rule 6) |
| An immobile unit's player order on a target out of reach | 6 | Unchanged: the order ends |

### 2.5 Neighbouring rules: in or out

| Rule | Source | Decision |
|---|---|---|
| B-179, B-180 | S4, S6 | **In** (D-2, D-4, D-5) |
| `FaceTargetWhileInCooldown` for turrets — **B-197, registered here** | S3 | **In** (D-3). Without it a lock is indistinguishable from today's frozen turret except in the wind-up |
| `FaceTargetWhileInCooldown` for hull weapons | S3 | **Unchanged:** pursuit turns the hull to its goal in the band (`aim_if_in_range`); **verify** in WP4 that a standing hull unit with a target and no pursuit (hold) does too |
| The Siege Tank's hidden `90mmCannonsFake` — **B-198, registered here** | S12 | **Out, deferred.** It deals nothing; it lets the turret aim (and perhaps play its attack) at enemies under any order. A presentation rule with its own question of what the game shows (**verify**) |
| Walking read from pursuit rather than from reach (D-4) | rule 5 | **Split:** WP6, after the contact solver merges, only if probe (b) finds the two differ |
| `Slowing` against `Stopped` (B-178) | turret wind-up plan | **Unchanged:** both stand to fire at another target |
| The Phoenix's wind-up hold (turret wind-up WP3, its Amendment 1) | — | **Unchanged:** a Phoenix firing at another target while it chases still stands for each 3-tick wind-up until that WP lands |
| `CanRetargetWhileChanneling`, `KeepChanneling` (channelled weapons) | `core`-to-`voidmulti` WeaponData (e.g. `liberty :127`, `:504`) | **Out, not registered here:** a channelled-weapon rule; no modelled weapon channels in our weapon phase (**verify** for the Void Ray's beam) |
| `CanInitiateAttackOrder 0` | S12 | **Out:** only on the fake weapon |
| Patrol, `AcquireMove` | turret wind-up plan §2.4 | **Out:** not modelled orders |

---

## 3. Diagnosis (on `7dc356fc`, read from the code)

1. **Turret phases.** `step_windup` (`combat.rs`) slews a turret towards the bound target every tick of the
   wind-up. `release_swing` charges `period − damage point` of cooldown and enters `Backswing`; `step_backswing`
   steps the ready path only once the cooldown is 0; `WeaponPhase::Cooldown` steps nothing; `idle_turret` runs only
   when the weapon has no target. So every turret is frozen from release until the cooldown runs out (B-197), and the
   Hellion's slews in its wind-up (B-179).
2. **The flame's direction.** `apply_splash` takes the line's axis from `attack_direction` (`combat_resolution.rs`),
   the vector from the attacker to the impact; the turret angle is never read for damage. So the lock is
   observable as `turret_angle` (hashed, published as `aim_angle_rad`) and in the arc test of the next swing.
3. **The order's target.** `SimWorld::ordered_target` returns the `AttackTarget`'s target for any weapon; when that
   target is out of the engagement band, `step_ready_weapon` returns false and the weapon idles in `Ready`. Nothing
   holds a second target.
4. **Acquisition under attack orders.** `decide_acquisition`: an `Explicit` attack is never replaced (it ends only
   for an immobile unit that cannot reach it); an `Acquired` attack keeps a target in reach unless outranked and
   replaces one out of reach with the best candidate, but keeps chasing it when the best candidate is that same
   target (a lower-ranked enemy in reach does not replace it). Both leave room for B-180: an explicit chase, and an
   automatic chase of a top-ranked target past lower-ranked ones.
5. **`current_target` under an attack order** equals the order's target (`Entity::commit_order`, `engage`), and
   pursuit reads it first (`movement/locomotion.rs`, `actor.current_target.or(…)`). So the weapon's own target cannot
   live there without redirecting the chase: it needs its own field.
6. **Hashed state.** `Entity` and `WeaponSpec` (inside each in-flight projectile) are hashed by their `fields!` lists
   in `checksum/encoding.rs`; schema 43 on `7dc356fc`.

---

## 4. Design decisions

### D-1. Two fields on `WeaponSpec`, one on `Entity`

- `WeaponSpec::fires_at_other_targets: bool`: the inverse of `OnlyFireAtAttackOrderTarget`; `false` is the engine
  default (S2). `WeaponSpec::locks_turret_while_firing: bool`: `LockTurretWhileFiring`; `false` is the default.
  Placed after `fires_outside_attack_order`, in the struct, in `fields!(WeaponSpec { … })` and in `WeaponSpecDef`
  with `#[serde(default)]`, documented "omitted reads the engine default". A mode override carries its own. No float
  (C7 unchanged).
- `Entity::weapon_target: Option<EntityId>`: the enemy the unit's weapon works against inside an attack order whose
  target it cannot work against (D-4). Written only by acquisition, read only by the combat phase; cleared by
  `commit_order` for every order (as the order's target changes), by transport loading, and when it is released
  (D-5). Hashed after `current_target`.
- **Why two booleans and not a mode.** The options are independent in the data (S7). **Why not reuse
  `current_target`.** §3 item 5: pursuit reads it.
- **Why no field for `FaceTargetWhileInCooldown`.** Every weapon of the eight layers keeps it (S3); a field nobody
  clears would be data with no source. If a later weapon clears it, the field is added then (§14 F-3).

### D-2. The lock (B-179)

In `step_windup`, the turret slews only when `!weapon.locks_turret_while_firing`. In the backswing (after D-3), a
locking weapon's turret holds until the backswing ends or a new swing starts out of it (rule 1). The engaged-arc
test of the next swing is unchanged, so a target that moved during the lock is re-acquired by the ready step's turn,
which `slew_and_try_swing` already does in the same tick. The lock never holds a hull.

### D-3. Turrets face their target between shots (B-197)

A turreted weapon with a target turns its turret towards it at `turret_turn_rate` in `Backswing` (unless locked,
D-2) and in `Cooldown`, through `slew_towards`, without changing the phase and without the engaged-arc
bookkeeping. One helper, `face_target_between_shots(entity, weapon, target)`, called from `step_backswing` (before
its ready step) and from the `Cooldown` arm of `step_entity_weapon`. Under `Mandate::OwnInitiative` a turret slews
as today; a hull weapon is untouched (rule 2).

**Alternative considered:** slew only in `Cooldown`. Rejected: S3's "in cooldown" is the whole period after the
release; our backswing runs inside it (the cooldown starts at release, `release_swing`).

### D-4. Which target a weapon works against inside an attack order (B-180)

`SimWorld::ordered_target` becomes:

```text
if front order is AttackTarget { target }:
    if the weapon (resolved against target's plane) can work against target this tick → (target, AttackOrder)
    else if weapon_target is Some(own)
         && the weapon resolved against own's plane has fires_at_other_targets
         && (that weapon is Moving || !entity.chases_order_target)  → (own, OwnInitiative)
    else                                                              → (target, AttackOrder)   (as today)
else … unchanged (hold, track, none)
```

- **"Can work against"** is the test the weapon phase applies: the target is available to the weapon
  (`StrikeContext::is_available`, which includes B-185's vision) and in the engagement band (`in_engagement_band`
  with no slop). The order's target keeps priority whenever it passes (rule 3).
- **`chases_order_target`** is acquisition's reading, stored nowhere: the unit is mobile and the order's target is
  outside `reaches_from_here` (rule 5). It is computed by acquisition when it sets `weapon_target` (D-5), so the
  combat phase reads only `weapon_target`'s presence; acquisition never sets one for a chasing unit whose weapon is
  not `Moving`. **verify** in WP3 that `reaches_from_here` agrees with pursuit's band (`Pursuit::aim_if_in_range`'s
  `min`/`max`) on every constructed case; a disagreement is probe (b)'s "gate mismatch" count and starts WP6.
- **Mandate.** `OwnInitiative`: a turret slews to the own target; a hull weapon fires only at what lies in its arc
  and never turns the hull, which belongs to the chase (the turret wind-up plan's D-5). All four shipped weapons are
  turreted.
- A swing in `Windup` keeps its bound target, as today, whichever target it started on.

### D-5. Acquisition keeps `weapon_target`

In `decide_acquisition`, for an `Acquired` or `Explicit` engagement whose unit has a weapon with
`fires_at_other_targets`:

1. If the order's target passes `passes_weapon_filters` and `reaches_from_here` → release `weapon_target`
   (`Acquisition::ReleaseWeaponTarget`, if set).
2. Else, if the unit chases (mobile, target out of reach) and no such weapon is `Moving` → release.
3. Else scan with `ScanScope::stationary` (only enemies in reach from where the unit stands) and a licence that the
   candidate's plane resolves to a weapon with `fires_at_other_targets` (as `licensed` does for `Track`), ranked by
   `TargetPriorityKey` with the archetype's tie-break, `ClosestToPrevious` measured from the order's target (rule 4).
   The best becomes `weapon_target` (`Acquisition::SetWeaponTarget`); none releases it.

This runs in addition to the existing decision for the order (which may still replace an `Acquired` target or abandon
an immobile unit's `Explicit` one; when the order changes, `commit_order` clears `weapon_target`). The decisions are
applied in entity order after the read-only scan, as every acquisition outcome is. The scan reuses the phase's scratch
buffers.

**Cost.** One extra stationary scan per tick only for a unit with such a weapon whose order's target it cannot reach
or see (§5).

### D-6. Data

`assets/catalog/units.ron`: `fires_at_other_targets: true` on `flying_saucer`, `void_goliath`,
`tripod_annihilator`; `locks_turret_while_firing: true` on `napalm_wheeler`. `assets/catalog/structures.ron`:
`fires_at_other_targets: true` on `planetary_bulwark_dome`'s `ground_weapon`. Each with a comment citing §2.3's
source lines. Every other weapon keeps the defaults.

### D-7. Validation

`WeaponSpecDef` validation refuses `locks_turret_while_firing` without `has_turret`, and `fires_at_other_targets` on
a `suicide_on_attack` weapon (§2.3), each with `SimCoreError::CatalogInvalidValue` naming the field; a unit test each.

### D-8. Hashed state and checksum

`fires_at_other_targets` and `locks_turret_while_firing` are hashed with `WeaponSpec`, in their struct position, so
every projectile's encoding gains two bytes. `Entity::weapon_target` is hashed after `current_target`. The schema is
bumped once, in WP1, to the next free number at merge (§5).

### D-9. What the simulation publishes

Nothing new. A turret's `aim_angle_rad` (`world/snapshot.rs`) now moves between shots (B-197) and holds during the
Hellion's firing (B-179). A swing at `weapon_target` publishes its `AttackSwing` and effects as any swing does, its
target being the own target.

### D-10. Records

- `DESIGN.md` §4: **B-197** and **B-198** registered in WP0 (number provisional, next free at merge); B-179, B-180
  pointed here in WP0 and marked fixed in WP5; B-197 fixed in WP5.
- The turret wind-up plan's §14 F-2 and §2.4 rows point here (WP5).

---

## 5. Determinism and checksum

- **Integers and booleans only.** No float is added (C7). The scan of D-5 is the existing deterministic scan with
  its existing ranking.
- **No new collection or iteration order.** Decisions are applied in entity order; the scratch buffers are the
  phase's.
- **WP1's proof** (the procedure of the weapon-validation plan's §5, scripts `proof.sh` and `goldens.sh`):
  1. With the three fields left out of the encoding and the schema at main's number, the tree reproduces every
     recorded checksum of both goldens and `DEFAULT_SANDBOX_CHECKSUM`, on native debug and release and on
     `wasm32-wasip1` release and debug.
  2. Then the fields go in, the schema becomes the next free number `N` (45 if the hidden-harvesters work has taken
     44, else whatever is next free at merge), and the checksums are re-recorded under the header "Re-recorded for
     plan 2026-11-24-fire-rules WP1 (checksum schema M -> N): each weapon also encodes whether it may fire at
     targets other than its attack order's and whether it locks its turret while firing, and each entity the target
     its weapon chose inside an attack order, so every checkpoint changed. No behaviour did: no rule reads the
     fields yet, and with them left out of the encoding at schema M both fixtures reproduce as recorded." All four
     builds reproduce the new values.
  3. If `main` takes `N` first, the merge repeats step 2 with the next free number.
- **WP2 and WP3 change no encoding;** WP4 neither. A golden move in any of them is traced to its first checkpoint
  and entity and explained by the package's rule, then re-recorded with a header naming the package, the entity and
  the rule; an unexplained move stops the package (§8). The predictions are in §7.3: WP4 moves both goldens through
  "red_siege"'s turret.
- **wasm.** No dependency changes. **Cost:** D-3 one turret slew per turreted unit with a target per tick (it
  already happens while `Ready`); D-5 one stationary scan per tick for an affected unit only. The throughput bench
  runs in the package gate.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure from a seed. Constructed tests find weapons by property (`has_turret`,
`locks_turret_while_firing`, `fires_at_other_targets`, `allowed_movement`, `is_immobile`), never naming a slug in a
rule; the data table of P-8 names analogues, as `b90_weapon_movement_follows_starcraft` does.

**Lock (WP2)**
- **P-1.** A turreted weapon with `locks_turret_while_firing` keeps `turret_angle` constant on every tick of its
  wind-up while its target moves across its front; the same weapon without the flag turns. The shot releases on the
  same tick in both, and the damage lands on the same units (the flame's line is the attacker-to-target vector).
- **P-2.** After WP4: the locking weapon also holds through its backswing; a new swing that starts out of the
  backswing turns the turret on its first tick.

**Off-order targets (WP3)**
- **P-3 (preference).** A unit whose weapon has `fires_at_other_targets`, under an explicit attack on a target in
  its band, fires only at that target while a second, higher-ranked enemy stands in reach.
- **P-4 (chase, `Moving`).** A scratch copy of an air weapon with `fires_at_other_targets` and `Moving`, under an
  explicit attack on a target that flees out of reach, deals damage to an enemy air unit it passes in reach while its
  order and its pursuit goal stay on the fleeing target; without the flag, it deals none.
- **P-5 (chase, not `Moving`).** The same with `Stopped` and with `Slowing`: no swing at the passed enemy while the
  unit chases.
- **P-6 (standing).** A unit with a `Stopped` weapon with `fires_at_other_targets`, under an explicit attack on an
  enemy in reach that its player does not see (B-185 holds fire, pursuit stands it in its band), fires at a seen
  enemy in reach; it returns to the order's target on the first tick that target is seen again.
- **P-7 (immobile).** An immobile unit with the flag, under an explicit attack on an enemy in reach and seen, never
  fires at another enemy in reach, even while its turret is still turning towards the order's target (rule 3: the
  order's target is workable, so `weapon_target` is not set). An explicit order on a target out of reach or unseen
  still ends (rule 6), and `weapon_target` is never set for an immobile unit.
- **P-8 (shipped values).** Every shipped weapon's `fires_at_other_targets` and `locks_turret_while_firing` equal
  §2.3's table (the defaults for every unlisted analogue and every mode's weapon), each row citing its source.
- **P-9 (ranking).** Among two enemies in reach of the own-target scan, the higher `TargetPriorityKey` is chosen;
  with `ClosestToPrevious`, among equals, the one nearer the order's target.
- **P-10 (zero recompilation).** In a catalog copy, clearing the Phoenix analogue's `fires_at_other_targets` makes
  P-4's pass deal no damage in the same binary (`runtime_catalog_tests.rs`); setting the Hellion analogue's lock off
  makes its turret turn in the wind-up.
- **P-11 (validation).** D-7's refusals, each naming the field; omitted fields read the defaults.

**Turrets between shots (WP4)**
- **P-12.** A turreted weapon whose target moves across its front between shots turns its turret towards it on
  every tick of its backswing and cooldown, at no more than `turret_turn_rate`; a hull weapon's mount does not change
  through the weapon phase.
- **P-13.** A turreted weapon with an engaged arc narrower than the target's bearing change during the cooldown
  starts its next swing no later than before, and on the tick its cooldown ends when the turret has caught up.

**Latent and confined**
- **P-14.** WP1 changes no world (goldens by the schema bytes only, proved).
- **P-15 (confined).** WP2, WP3 and WP4 each move only the worlds their probe counts (§7.2), and in each the first
  difference is a counted event.
- **P-16 (direction, a prediction recorded before measuring, not a threshold).** WP4: in battles that move, the
  turreted side's first swing after each cooldown starts no later, so its enemies lose no fewer units on average; no
  battle's side of record is expected to change. WP3: no measured world moves.

---

## 7. Worlds that move

### 7.1 Commands

The weapon-validation plan's scripts (`~/scratch/weapon-validation/scripts/measure.sh`, `compare.sh`, `client.sh`,
`goldens.sh`, `proof.sh`, `firstdiff.py`), copied to `~/scratch/fire-rules/scripts/` with their paths changed.
`measure.sh` runs `route_trace` with `--assets <tree root>/assets`, so each tree is measured with its own data.
Binaries are built once per tree:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
R=~/scratch/fire-rules
mkdir -p $R/bin-<wp> $R/tree-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} $R/bin-<wp>/
git archive HEAD assets | tar -x -C $R/tree-<wp>
$R/scripts/measure.sh $R/bin-<wp> $R/tree-<wp> $R/world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
```

`measure.sh` plays the matrix on its seeds of record and 16 more, battle 31 on 100 seeds, battle 33 on 64, and the
showcase and showcase effects scenarios on their default seed and seeds 1 to 8 for 6,000 ticks. Run it in the
background, one at a time across worktrees (charter §7). The client skirmish (`skirmish_match_tests`) is compared by
its stage log (`client.sh`). The matrix outputs are outcomes and unit tables, which carry no turret angle (**verify**
in WP0 from `sim_fixtures::trace`): a change of turret angle alone moves a golden but no measured output.

### 7.2 WP0's probes

On WP0's tree a scratch patch prints three kinds of event; it is never committed and is kept under
`~/scratch/fire-rules/probe/` with its diff (the probe may name slugs; it is scratch). The probe plays every world of
§7.1 and the two goldens (through a scratch test, as the weapon-validation probe did). The evidence records each
count per world and seed and the first event of each kind.

- **(a) Lock slews:** every tick `step_windup` changes the turret angle of a weapon that §2.3 marks locking; and
  every later swing of that weapon whose start was delayed by a turn the lock would have pre-empted (its
  `RotatingToTarget` ticks after such a wind-up). *Expected:* slews in the Hellion battles (9a, 9b, 10) and the
  showcase effects scenario (`showcase_raiders`); delayed swings zero (the Hellion analogue's turret turns 55.99° a
  tick and `slew_and_try_swing` starts the swing on the tick it aligns). So WP2 is predicted byte-identical in every
  measured output and golden.
- **(b) Off-order opportunities:** every tick a unit whose weapon §2.3 marks `fires_at_other_targets` is under an
  `AttackTarget` its weapon cannot work against (out of band, or failing the filter), while another available enemy
  lies in reach of that weapon; tagged with the engagement (`Explicit`, `Acquired`), whether the unit is immobile,
  whether it chases by D-4's reading, whether pursuit stood it this tick (`aim_if_in_range` returned true) — the
  **gate mismatch** count is "chases by reach but stood by pursuit" or the reverse — and the weapon's
  `allowed_movement`. *Expected:* events only for Immortal and Colossus analogues that chase (battles 2a, 2b, 3, 12a,
  12b, 12c, 26, 28, 30, 35), all gated out (`Slowing`, `Stopped`); no Phoenix or Planetary Fortress analogue in any
  measured world (the census of the turret wind-up plan's WP0: none in the showcase; the golden Phoenix is under
  `Move`, not an attack order). So WP3 is predicted byte-identical. A nonzero ungated count names the worlds WP3 may
  move.
- **(c) Frozen turrets:** every tick a turreted weapon in `Backswing` or `Cooldown` has a target whose bearing
  differs from `turret_angle`, and every `RotatingToTarget` tick that follows a cooldown (the delay D-3 removes),
  tagged with the weapon's arc and turret rate. *Expected:* the bearing events in every battle with a turret (2a, 2b,
  3, 9a, 9b, 10, 12a, 12b, 12c, 21, 26, 28, 30, 35, 37a, 37b, 39a, 39b, 40, 42), both goldens ("red_siege") and the
  showcase effects scenario; the delay events only where the arc is narrow and the rate finite (Immortal 11.25° at
  40.32°/tick, Hellion, Phoenix 2.4994° at 30.20°/tick, Colossus 56.25°, Lurker; never the Siege Tank's 360° arc,
  which fires without aiming).
- **Rule:** P-15 is judged on these counts. A world with a zero count that moves is a defect of the package.

### 7.3 Expected moves

| World | WP1 (latent, schema) | WP2 (lock) | WP3 (off-order) | WP4 (between shots) |
|---|---|---|---|---|
| Matrix (seeds of record and 16) | byte-identical | byte-identical (probe a: no delayed swing) | byte-identical (probe b: all gated) | **moves only in battles with probe (c) delay events** (expected Immortal 2a, 2b, 3, 28, 30; Colossus 12a, 12b, 12c, 26, 35; Hellion 9a, 9b, 10; Lurker 21), from the first counted tick; the tank battles only by turret angle (no measured output) |
| Battles 31, 33 | byte-identical | byte-identical | byte-identical | byte-identical (no turret) |
| Showcase, default and 1–8 | byte-identical | byte-identical | byte-identical | byte-identical unless the census finds a turret with a narrow arc (**verify** in WP0) |
| Showcase effects, default and 1–8 | byte-identical | byte-identical | byte-identical | moves where probe (c) counts delays (Hellions of `showcase_raiders`) |
| Client skirmish stage log | identical | identical | identical | identical (the pilot trains `pulse_bot`, no turret) |
| Goldens, sandbox | **schema bytes only** (proof, §5) | unchanged (no Hellion) | unchanged (no attack order on the Phoenix) | **both move** by "red_siege"'s `turret_angle` from its first cooldown with a moving target; traced and re-recorded (§5) |

**What each moving package records** for every world that moved (in the evidence, all seeds, before and after): the
first differing tick and the probe event behind it, each side's survivors and the tick of the last death.

**No new battle.** The matrix fixture has no explicit attack order (`SideOrder`, the turret wind-up plan's F-3), and
the attack-move case of B-180 needs a top-ranked target that stays out of reach while a lower-ranked one is in reach,
which no placement holds over a battle without a flee rule designed for it. The constructed tests P-3 to P-7 are the
acceptance; probe (b) records the exposure. A battle can follow F-3 (§14).

---

## 8. Stop rules

1. **WP1 is latent.** Every world of §7 byte-identical; goldens moved by the schema bytes only, proved (§5).
2. **WP2 and WP3 move nothing** unless their probe counted an event that predicts it (a delayed swing; an ungated
   opportunity). A move elsewhere, or one whose first differing tick is not a counted event, stops the package
   until traced.
3. **WP4 moves only the worlds probe (c) counted** (P-15), each from a counted event. A second cause is registered
   as a bug, and the package lands only if it is separable. A change of a battle's side of record is traced and
   reported to the orchestrator, with its claim, as Q0103 did for the solver, and not rewritten in the package.
4. **No tuning constant.** Every value is the data's (§2.3) or an existing engine rule's.
5. **Nothing under `crates/sim_core/src/movement/` changes** before WP6. A package that needs it stops and reports.
6. **WP4 may be split off.** If its moves cannot be explained within the package, WP2 and WP3 land without it, WP2's
   backswing half (P-2) is deferred with it, and B-197 stays open with the trace.

---

## 9. Client consequences

- Turrets visibly follow their targets between shots (B-197) and the Hellion analogue's turret holds while it
  flames (B-179), through `aim_angle_rad`; no client code reads the phases.
- A Phoenix analogue chasing one target fires at another: its `AttackSwing` and beam events name the own target,
  which the client draws as any beam.
- No `PlayerView`, `RenderSnapshot` field, shader or capture change. `effect_snapshot_tests.rs` runs in the package
  gate on its catalog triggers.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP5 | D-10 |
| `docs/plans/README.md` | plan, WP1, WP5 | Row |
| `docs/evidence/2026-11-24-fire-rules.md` (new) | WP0 to WP5 | §7, §5 proofs |
| `crates/sim_types/src/weapon.rs` | WP1 | The two fields, docs |
| `crates/sim_core/src/catalog.rs`, `crates/sim_core/src/catalog/validation.rs` | WP1 | Def fields; D-7 |
| `crates/sim_core/src/entity.rs` | WP1 | `weapon_target`, cleared by `commit_order` and transport loading |
| `crates/sim_core/src/transport/mod.rs` | WP1 | `weapon_target` cleared on loading |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-8, schema `N` |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `sim_fixtures`' `DEFAULT_SANDBOX_CHECKSUM`; the encoding unit tests' schema byte | WP1 (WP4 re-record) | §5 |
| Test files with `WeaponSpec { … }` literals (`grep -rln fires_outside_attack_order crates/*/tests crates/*/src`) | WP1 | The two fields at their defaults (mechanical) |
| `assets/catalog/units.ron`, `assets/catalog/structures.ron` | WP1 | D-6 |
| `crates/sim_core/src/combat.rs` | WP2, WP4 | D-2, D-3, module invariants |
| `crates/sim_core/src/combat_resolution.rs` | WP3 | D-4 (`ordered_target`) |
| `crates/sim_core/src/target_acquisition.rs` | WP3 | D-5, module invariant 2 restated |
| `crates/sim_core/tests/fire_rules_tests.rs` (new, `//!` doc) | WP2, WP3, WP4 | §12 |
| `crates/sim_core/tests/turret_tracking_tests.rs` | WP4 | P-12 (charter subject "turrets") |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP1 | P-8 (`SC2_FIRE_RULES`, `b180_b179_fire_rules_follow_starcraft`), P-11 |
| `crates/sim_core/tests/checksum_state_tests.rs` | WP1 | Each new field changes the checksum |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP3 | P-10 |
| `crates/gate/suites.ron` | WP2, only if needed | The new test target (**verify**: an uncovered target may fall to its crate's default class) |
| `docs/plans/2026-11-23-turret-wind-up.md` | WP5 | Pointers (§14 F-2, §2.4) |
| `crates/sim_core/src/movement/locomotion.rs` | **WP6 only** | The chase reading, if needed |

**Overlap.** The contact solver owns `movement/` and may add `Entity` fields; the hidden-harvesters work holds
schema 44. Whichever merges second rebases its `fields!` lists and takes the next schema number (R-3). Turret
wind-up WP3 changes `walking_goal` only; no file here overlaps it.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`
(check `quota -s` and `df -h ~/scratch` first). Write the failing tests first and paste their red run in the
hand-back. Run `cargo gate quick` and the tests the change needs; a verifier runs `cargo gate package` (Q0061). Run
§7's comparison in every package against WP0's tree. No `unwrap`; `expect` only with `invariant:`; no catalog
identity in production Rust (C5); tests find weapons by property. Check `docs/questions/open/` before the last commit.
WP0 to WP5 may run as one branch and one session, one commit each, with one `package` gate on the final tree (charter
§4.3). WP6 is a separate branch after the contact solver merges, and only if needed.

### WP0: records, baseline and probes (XS, docs only)

- `DESIGN.md` §4: B-179 and B-180 pointed here; **B-197** and **B-198** registered (D-10). Evidence file created
  with §2.3's survey and the grep and awk commands of §16.
- Measure WP0's tree (§7.1) for the baseline. Run §7.2's probes; record their counts and first events; take the
  censuses §7 marks **verify** (the trace columns; turrets in the showcase). State in the evidence which worlds WP2
  to WP4 are expected to move.
- File Q0109 from §15 (the orchestrator, or WP0 if asked).
- **Gate:** docs-only.

### WP1: the fields and their data, latent (S, `sim_types`, `sim_core`, data, schema)

Failing tests first (red: they do not compile, then on a field-only stub because the data is missing):
- `weapon_catalog_rules_tests::b180_b179_fire_rules_follow_starcraft` (P-8) and the two refusals (P-11).
- `checksum_state_tests`: each of the three fields changes the checksum.
- `entity.rs` unit tests: `commit_order` clears `weapon_target` for every `UnitOrder` variant.

Then D-1, D-6, D-7, D-8, the docs and the mechanical literals; then §5's proof. **Acceptance:** §8 rule 1. Plan row
`Active`. Commit `feat(sim_core): weapons state whether they fire at targets off their attack order and lock their
turret while firing, latent (fire-rules WP1, schema N)`.

### WP2: the locked turret (XS, `sim_core`, B-179)

Failing tests first (red on WP1's tree), in `fire_rules_tests.rs`, with a scratch copy of a shipped turreted weapon
found by property and a target walking across its front:
- `a_locked_turret_holds_through_its_wind_up` and `an_unlocked_turret_turns_through_its_wind_up` (P-1).
- `the_lock_changes_neither_release_nor_damage` (P-1).
- `runtime_catalog_tests::an_authored_lock_off_turns_the_turret` (P-10, lock half).

Then D-2's wind-up half. **Acceptance:** §8 rule 2. Commit `fix(sim_core): a weapon that locks its turret holds it
while firing, 5.0.14 LockTurretWhileFiring (fire-rules WP2, B-179)`.

### WP3: targets off the attack order (M, `sim_core`, B-180)

Failing tests first (red on WP2's tree), in `fire_rules_tests.rs`, scratch weapons found by property:
- `the_order_target_comes_first` (P-3).
- `a_weapon_that_fires_while_moving_strikes_what_it_passes_in_a_chase` (P-4).
- `a_weapon_that_stands_to_fire_holds_in_a_chase` (P-5, `Stopped` and `Slowing`).
- `a_standing_unit_fires_at_a_seen_enemy_while_its_target_is_unseen` (P-6).
- `an_immobile_unit_never_leaves_a_workable_order_target` and `an_immobile_units_order_on_an_unworkable_target_ends` (P-7).
- `its_own_target_is_ranked_as_acquisition_ranks` (P-9).
- `runtime_catalog_tests::an_authored_order_target_only_weapon_holds_in_a_chase` (P-10).
- The reach-against-pursuit agreement (D-4's **verify**) as a test that drives the movement phase and compares
  `reaches_from_here` with whether pursuit stood the unit, for each constructed case.

Then D-4 and D-5. **Acceptance:** §8 rule 2; P-15, P-16 (WP3 byte-identical). Commit `fix(sim_core): a weapon whose
data allows it fires at another target when it cannot work its attack order's, 5.0.14 OnlyFireAtAttackOrderTarget 0
(fire-rules WP3, B-180)`, stating which worlds moved and why.

### WP4: turrets face their target between shots (S, `sim_core`, B-197)

Failing tests first (red on WP3's tree):
- `turret_tracking_tests::a_turret_follows_its_target_between_shots` and `…_a_hull_weapon_does_not` (P-12).
- `fire_rules_tests::the_next_swing_starts_no_later` (P-13).
- `fire_rules_tests::a_locked_turret_holds_through_its_backswing` and `…_a_swing_out_of_the_backswing_turns_it`
  (P-2).

Then D-3 and D-2's backswing half. The goldens and every moved world are traced and re-recorded with headers (§5).
**Acceptance:** §8 rules 3 and 6; P-15, P-16; §7.3 column WP4. Commit `fix(sim_core): turrets face their target
while their weapon cools down, 5.0.14 FaceTargetWhileInCooldown (fire-rules WP4, B-197)`, stating which worlds moved
and why.

### WP5: bookkeeping (XS)

D-10's records; the evidence completed; this plan's `## As built` for WP0 to WP5; README row `Done` (or `Partly done`
if WP6 is needed, or if WP4 was split off under §8 rule 6). Merge `main`; `cargo gate package` once on the final tree
(by the verifier); the schema re-recorded if `main` took the number (§5).

### WP6: the chase read from pursuit (S, `movement/`, B-180's gate) — **after the contact solver merges**, conditional

Starts only if WP3's agreement test or probe (b)'s gate-mismatch count found that `reaches_from_here` and pursuit's
band disagree, and only once the contact-solver branch is on `main`. It begins with an amendment that fixes the
reading against the merged solver: pursuit reports, per unit and tick, whether it walked the unit under an attack
order (a transient written by the movement phase and read by the next acquisition, or a hashed field if it must
survive the tick, with its schema), and D-5's step 2 reads it. Then its own probe on the then-current `main`, failing
tests (the disagreeing cases of WP3), the world comparison and stop rule 2. If nothing disagrees, WP5 records that and
WP6 is dropped. B-178 (`Slowing`) is a separate row and is not settled here.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | Defaults; D-7's refusals | WP1: `catalog/validation.rs` unit tests, `weapon_catalog_rules_tests.rs` |
| State | `weapon_target` cleared by every order | WP1: `entity.rs` unit tests |
| Data | §2.3 against 5.0.14 | WP1: `weapon_catalog_rules_tests.rs` (P-8) |
| Encoding | Each field changes the checksum | WP1: `checksum_state_tests.rs` |
| Rules | P-1 to P-7, P-9, P-13 | WP2 to WP4: `fire_rules_tests.rs` |
| Turrets (charter subject) | P-12 | WP4: `turret_tracking_tests.rs` |
| Zero recompilation | P-10 | WP2, WP3: `runtime_catalog_tests.rs` |
| Worlds | P-14 to P-16: §7.3 | every package (measure scripts); goldens and sandbox in the package gate (release and `wasm32-wasip1`) |
| Charter subjects | `sim_core`'s subjects of the guide (`stutter_step_tests.rs`, `turret_tracking_tests.rs`, `damage_and_shields_tests.rs`, `push_priority_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`, `sc2_who_wins_matchup_matrix_tests.rs`, …) keep passing | package gate |

Re-derived (expected to change in WP4): tests that assert a turret angle after a shot while the target moved
(`turret_tracking_tests.rs`, `hull_turning_tests.rs`, `weapon_cadence_tests.rs`; **verify** by running them on WP4's
tree), each named in the commit. Guards that pass unchanged: `target_selection_tests.rs`, `weapon_validation_tests.rs`,
`multi_strike_timing_tests.rs`, `attack_move_simulation_tests.rs` (WP3 changes nothing for a weapon without the flag).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | "Firing" in `LockTurretWhileFiring` covers other phases than D-2's | The Hellion analogue's turret holds for a different span | Q0109 part 1; one condition in two places |
| R-2 | The engine lets the weapon prefer its own target over a workable order's target (S9 is silent) | A Phoenix ordered on one target fires at another in reach | Q0109 part 3; one branch of `ordered_target` |
| R-3 | Schema 44 (hidden harvesters) or 45 taken first; the contact solver adds `Entity` fields | Merge conflicts in `fields!` lists | "Next free at merge" (§5 step 3); the second branch rebases |
| R-4 | B-197 moves matrix battles whose claims are about other rules | A recorded claim might fail | Stop rule 3: traced and reported; rule 6 lets WP4 split off |
| R-5 | The reach-based chase reading disagrees with pursuit's band (ring, slop, solver gap) | A `Stopped` weapon fires at another target while pursuit walks the unit, or holds while it stands | D-4's **verify** test and probe (b)'s count; WP6 |
| R-6 | A Phoenix firing at another target while chasing stands for each wind-up (turret wind-up Amendment 1) | Its chase is slowed by 3 ticks a shot until that WP3 lands | Recorded; no measured world has a Phoenix in an attack order |
| R-7 | Hull weapons are not fully covered by pursuit's aim between shots | A hull unit on hold with a moving target faces it late | B-197's hull half is **verify** in WP4; registered as a separate row if found |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: WP6, the chase read from pursuit** (after the contact solver, conditional on a disagreement).
- **F-2: B-198, the Siege Tank's hidden `90mmCannonsFake`** (S12): presentation of the tank's turret under any
  order; needs Q0109-like confirmation of what the game shows and a second weapon slot per archetype. Deferred.
- **F-3: a `FaceTargetWhileInCooldown` field.** Added only when a modelled weapon clears the option (none does).
- **F-4: a matrix battle for B-180** (a Phoenix analogue under an explicit attack) waits for the fixture's explicit
  orders (the turret wind-up plan's F-3).
- **B-178** (`Slowing`), **B-181** (`NoDeceleration`): unchanged, movement rules.
- **Channelled weapons** (`KeepChanneling`, `CanRetargetWhileChanneling`): not surveyed further; no modelled weapon
  channels in our weapon phase.
- **Unmodelled weapons of S6** (Battlecruiser, Mothership, Void Ray's enhanced weapon, building shield): no
  analogue.

---

## 15. Draft owner question (filed as [Q0109](../questions/open/0109-turret-and-off-order-fire.md))

```markdown
# Q0109: Turrets while firing and between shots, and weapons that fire off their attack order

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** the Hellion's turret does not turn from the start of an attack to the end of its
  follow-through; every other turret follows its target between shots; a Phoenix, Immortal, Colossus or Planetary
  Fortress that cannot fire at its attack order's target (out of range or out of sight) fires at the best enemy it
  can reach, but always fires at the order's target when it can, and the Immortal and Colossus only while standing;
  a Planetary Fortress ordered to attack a unit out of range or out of sight drops the order at once (plan
  `2026-11-24-fire-rules` D-2 to D-5).

## Question

Space Scuffle is adopting three StarCraft II 5.0.14 weapon options. The game data name them but not exactly what they
do:

1. **The Hellion's locked turret** (`LockTurretWhileFiring`). While a Hellion flames, its turret does not turn. For
   how long: (a) from the start of the attack to the end of its follow-through, about 1 second (the default); (b)
   only until the flame comes out, about 0.25 seconds; (c) until its next attack starts?
2. **Turrets between shots** (`FaceTargetWhileInCooldown`). While a Siege Tank or Immortal waits to fire again, does
   its turret (a) keep following its target, so the next shot comes as soon as it is ready (the default); or (b) stay
   where it fired until it may fire again?
3. **Firing at another unit during an attack order** (the Phoenix's, Immortal's, Colossus' and Planetary Fortress'
   "Only Fire At Attack Target" is off). A Phoenix ordered to attack a Mutalisk that flies away passes an Overlord.
   (a) It fires at the Overlord only while the Mutalisk is out of range, and at the Mutalisk whenever it can (the
   default); (b) it fires at whichever of the two it ranks best, even with the Mutalisk in range.
4. **A Planetary Fortress ordered to attack a unit out of its range.** (a) The order is dropped at once (the
   default, as for every immobile unit now); (b) the order stays, and the Fortress fires at other enemies in range
   until the unit comes into range or dies.

If you can, check in a custom game: a Hellion flaming a Zergling that runs across it; a Siege Tank's turret against
a Marine walking around it; a Phoenix ordered onto a fleeing Mutalisk past an Overlord; a Planetary Fortress ordered
onto a unit out of range with Zerglings at its gate.

## Answer

```

The question number is the next free at filing (Q0109 as assigned for this plan; Q0107 and Q0108 are reserved
elsewhere).

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read with `grep` and `awk`: every `OnlyFireAtAttackOrderTarget`, `OnlyFireWhileInAttackOrder`,
  `LockTurretWhileFiring` and `LegacyOptions` line of every `WeaponData.xml` mapped to its enclosing weapon, with
  lines inside `<!-- … -->` flagged (S3, S4, S6); every `WeaponArray` that links the flagged weapons in
  `UnitData.xml` (§2.3); the Hellion's turret (`TurretData.xml`) and flame persistent (`EffectData.xml`); every
  `AcquirePrioritization` (S8). WP0 copies the commands into the evidence.
- Code read on `7dc356fc`: `combat.rs` (`step_windup`, `step_backswing`, `step_entity_weapon`, `slew_towards`,
  `idle_turret`), `combat_resolution.rs` (`advance_weapon_cycles`, `ordered_target`, `apply_splash`,
  `attack_direction`), `target_acquisition.rs` (`engagement_of`, `decide_acquisition`, `without_candidate`),
  `movement/locomotion.rs` (`Pursuit::aim_if_in_range`, read only), the turreted rows of `units.ron` and
  `structures.ron`, and the matrix fixture's battles with turreted units.
- Community sources searched for S9 and S10 (2026-11-24): SC2Mapster forum threads on "Only Fire At Attack Target"
  and attack-while-moving (as cited by the turret wind-up plan's §16), and the SC2Mapster wiki's turret page; none
  defines `LockTurretWhileFiring` or `FaceTargetWhileInCooldown`, so both readings are [INF] with Q0109 parts 1 and
  2.
- No prototype: WP1 is latent by construction, WP2 and WP3 are predicted byte-identical by probes (a) and (b), and
  WP4's moves are predicted by probe (c) and traced.

## As built

Not started.
