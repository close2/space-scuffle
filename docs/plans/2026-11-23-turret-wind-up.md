# Blueprint: Who May Fire While Moving — `AllowedMovement`, Attack Orders and the Wind-Up Hold (B-90, B-45)

This plan settles bug B-90 of the register against StarCraft II 5.0.14, and with it the Phoenix half of B-45 and,
in a later package, its Void Ray half:

- **A turreted weapon works against `current_target` under any order (B-90, first half).** Today a unit whose weapon
  sits on a turret fires at its `current_target` whatever its front order is, so a Siege Tank analogue ordered to
  move away keeps shooting the unit it fought before. In 5.0.14 every weapon carries `OnlyFireWhileInAttackOrder 1`
  by default (`core` WeaponData:20); only three modelled weapons clear it (the Phoenix's, Immortal's and
  Colossus'). A turret is a mount, not a licence to fire.
- **A turreted weapon's wind-up does not hold its unit (B-90, second half).** The movement phase stands a unit still
  during a wind-up only when its weapon has no turret (`walking_goal`, `crates/sim_core/src/movement/locomotion.rs`).
  In 5.0.14 what decides whether a unit may move while it attacks is the weapon's `AllowedMovement`, not its mount:
  only `Moving` weapons fire while moving (`IonCannons`, `VoidRaySwarm`, `VolatileBurst`).
- **The Phoenix analogue cannot fire while moving (B-45).** Its weapon is `Moving` and clears both attack-order
  options (`swarm` WeaponData:176–181), so it fires at what it passes under a plain move. After this plan it does.
- **The Void Ray analogue cannot fire while moving (B-45).** Its weapon is `Moving` but keeps
  `OnlyFireWhileInAttackOrder` (the options that would clear it are commented out, `void` WeaponData:171–174), so it
  fires while moving only inside an attack order: its wind-up must not stop it. That half needs the movement change
  and lands in WP3.

The rules become two capability fields on `WeaponSpec`, read from the catalog: `allowed_movement`
(`Stopped`, the engine default; `Slowing`; `Moving`) and `fires_outside_attack_order` (`OnlyFireWhileInAttackOrder
0`). The combat phase and acquisition read them; `has_turret` keeps only its meaning as a mount (how the weapon aims).
No code names a unit (charter §3.3, C5).

The movement half of the fix, one condition in `walking_goal`, lives in `crates/sim_core/src/movement/`, which the
`contact-solver-plan` branch owns. It is isolated as WP3 and starts only when that branch has merged or released the
file (§11). Everything else is gated from the combat side.

A full plan is required (charter §6): a new rule in the simulation, a change of hashed encoding (checksum schema),
and data that moves battles wherever a turreted unit is ordered to move while it has a target, and wherever a
turreted unit chases during its wind-up.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`d71f7153` (checksum schema 38).
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-3 turn-before-move tolerance; D-δ engaged units may be nudged) | Closed | D-3 (why "moving" is read from the order, not from displacement) |
| Q0008: where the data is silent, search for what StarCraft II does | Standing rule; the meaning of `AllowedMovement` and the two options is the editor's, not the data's | §2.1 S10–S12 |
| Q0002: scoping is the agent's call if written down | Standing rule | §2.4, §14 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| The owner's guidance: no fitting to fixed seed sets; judge on principled properties | Followed | §6, §7 |
| No new owner question | — | §15 (Q0101 stays free) |

---

## 1. Summary

| | Today (`d71f7153`) | After this plan |
|---|---|---|
| Weapon target outside an attack order | A turreted weapon, or any weapon of a holding unit, works against `current_target` under any front order | A holding unit's weapon as today; otherwise only a weapon with `fires_outside_attack_order` (Phoenix, Immortal, Colossus), and, unless it is `Moving`, only while the unit's order does not walk it (D-2, D-3) |
| Target of such a weapon under a walking order | Whatever `current_target` was when the order came (stale) | Tracked by acquisition from where the unit is, as a holder's is: only enemies its weapon reaches, the order untouched (D-4) |
| Hull of a unit firing outside an attack order | Turned towards the target by the weapon | Never turned by the weapon; it fires at what lies in its arc (D-5). No shipped weapon reaches this until WP3 (§2.3) |
| Wind-up hold (WP3) | Every unit with a hull weapon stands; a turreted unit walks on | Every unit stands unless its weapon is `Moving` (Phoenix, Void Ray, Baneling) |
| Siege Tank (both modes), Hellion, Lurker, structures | Turret fires at `current_target` under any order | Fire only in an attack order or on hold; Tank and Hellion stand still for their wind-up in a chase (WP3) |
| Phoenix (`flying_saucer`) | Fires at a stale `current_target` under Move, nothing otherwise | Fires while moving at the best enemy it reaches (B-45) |
| Immortal, Colossus | Turret fires at a stale `current_target` under Move | No fire while their order walks them (`Slowing`, `Stopped`); stand for their wind-up in a chase (WP3) |
| Void Ray (`prismatic_saucer`) | Stands for each wind-up of a chase | WP3: keeps moving through its wind-up inside an attack order; no fire under a plain move (`OnlyFireWhileInAttackOrder` kept) |
| `WeaponSpec` | `has_turret`, … | + `allowed_movement: AllowedMovement` and `fires_outside_attack_order: bool`; both omitted in RON read the engine default |
| Hashed state | — | The two fields in the encoding of a projectile's `WeaponSpec`; no entity field; schema **next free at merge** (39 unless another branch takes it) |
| Worlds that move | — | WP1: none (schema bytes only). WP2: worlds where a turreted unit fires outside an attack order, expected none of the measured ones (§7). WP3: matrix battles with a turreted unit or the Void Ray that chase during a wind-up |
| Size | — | M: five packages; WP2 M, WP1 M (mechanical), the others S or XS |

---

## 2. What StarCraft II does

Tags: [DATA] read in the game data; [COMM] community or editor documentation; [INF] inferred; **verify** not
confirmed.

### 2.1 Sources

All game data is SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as
data. The override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`; a
later layer replaces only the fields it names. Each weapon was resolved through that chain by a script that strips
XML comments (`~/scratch/turret-wind-up/weapons.py`, §16), because the Void Ray's two options are commented out
(S6).

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | Every weapon defaults to firing only while its unit is in an attack order, and only at that order's target | `Options OnlyFireWhileInAttackOrder 1`, `OnlyFireAtAttackOrderTarget 1` on the `CWeapon` default | `core` WeaponData:20–21 [DATA] |
| S2 | No `CWeapon` default sets `AllowedMovement`; a weapon that names none has the engine's default | — | `core` WeaponData:3–31 [DATA]; the default's meaning, a unit that must stand to fire, is S10 (**verify**) |
| S3 | Damage point and backswing defaults | `DamagePoint 0.167`, `Backswing 0.5` | `core` WeaponData:17–18 [DATA] |
| S4 | The Phoenix's weapon fires while moving and outside attack orders, at any target | `IonCannons`: `OnlyFireWhileInAttackOrder 0`, `OnlyFireAtAttackOrderTarget 0`, `AllowedMovement Moving`; on the `Phoenix` turret | `swarm` WeaponData:175–182; `swarm` UnitData:1805 [DATA] |
| S5 | The Immortal's weapon fires outside attack orders but only while slowing | `PhaseDisruptors`: `AllowedMovement Slowing` (`voidmulti` WeaponData:207); `OnlyFireWhileInAttackOrder 0`, `OnlyFireAtAttackOrderTarget 0` (`balancemulti` WeaponData:185–186); on the `Immortal` turret (`liberty` TurretData:73–76) | [DATA] |
| S6 | The Void Ray's weapon fires while moving, but only in an attack order | `VoidRaySwarm`: `AllowedMovement Moving` (`void` WeaponData:176); the three options that would clear the order conditions are inside `<!-- … -->` (`void` WeaponData:171–174); `balancemulti` sets only `Hidden` and the cost (WeaponData:311–316); no turret (`void` UnitData:1423) | [DATA] |
| S7 | The Colossus' weapon fires outside attack orders and must stand | `ThermalLances`: no `AllowedMovement`; `OnlyFireWhileInAttackOrder 0`, `OnlyFireAtAttackOrderTarget 0` (`balancemulti` WeaponData:229–230); `Colossus` turret (`liberty` TurretData:49–53) | [DATA] |
| S8 | The Siege Tank (both modes), Hellion and Lurker keep both defaults and set no `AllowedMovement` | `90mmCannons` (`liberty` WeaponData:237–243, `balancemulti`:5), `CrucioShockCannon` (`liberty`:255–262), `InfernalFlameThrower` (`liberty`:582–591), `LurkerMP` (`swarm`:215–225, `void`:15) | [DATA] |
| S9 | Hold Position and Patrol are commands of the move ability, not of the attack ability; attack is its own ability | `CAbilMove` `CmdButtonArray` `Move`, `Patrol`, `HoldPos`, `AcquireMove`, `Turn` (`core` AbilData:533–561); `CAbilAttack` (`core` AbilData:115–139) | [DATA] |
| S10 | `AllowedMovement`: `Moving` lets the unit fire while it moves; `Slowing` while it decelerates; with neither the unit stands to fire | Editor field "Weapon: Allowed Movement"; map-makers set it to `Moving` for attack-while-moving and to deceleration for smoother attacks | [COMM] SC2Mapster forum threads (§16), **verify** the exact meaning of `Slowing` |
| S11 | Clearing `OnlyFireWhileInAttackOrder` and `OnlyFireAtAttackOrderTarget` lets a weapon fire under a move order, at targets it acquires itself | Editor options "Only Fire While Attacking", "Only Fire At Attack Target", unchecked for attack-while-moving | [COMM] SC2Mapster forum threads (§16) |
| S12 | A unit on Hold Position fires at enemies in reach | Common play knowledge; the data has no attack order behind `HoldPos` (S9), so the engine must acquire for it | [COMM], **verify** how the engine reconciles S1 with S9 |
| S13 | `UninterruptibleDuration 32767.9997` on every legacy weapon | The `CWeaponLegacy` default; no modelled weapon overrides it | `core` WeaponData:38 [DATA]; not a distinguishing fact, not modelled |
| S14 | The Hellion's turret is locked while it fires | `LegacyOptions LockTurretWhileFiring 1` | `liberty` WeaponData:590 [DATA] (registered, B-179) |
| S15 | Some weapons stop their unit without deceleration when they attack | `LegacyOptions NoDeceleration 1`: SCV, Probe, Drone (`voidmulti` WeaponData:103, :195, :218), Baneling (`liberty`:473), Mutalisk (`liberty`:400) | [DATA] (registered, B-181) |
| S16 | The Planetary Fortress' cannon may fire at targets other than its attack order's | `TwinIbiksCannon` `OnlyFireAtAttackOrderTarget 0` | `liberty` WeaponData:140 [DATA] (registered with the others, B-180) |

No owner question is needed: S1–S8 and S14–S16 are data; S10–S12 are the editor's meaning of fields whose values
are data, and the reading needed here (stand unless `Moving`; fire under a move only with the option cleared) is the
one every source agrees on. The one open detail, `Slowing` against `Stopped`, is registered (B-178) rather than
asked, because no shipped unit is affected by the simplification D-3 makes (§2.3).

### 2.2 The rules that follow

1. **A weapon fires only inside an attack order, unless its data says otherwise** (S1, S4, S5, S7). "Inside an
   attack order" means: the front order attacks a unit (`AttackTarget`, explicit or acquired), or the unit holds
   position (S12). Attack-move is an attack order whose target acquisition places an `AttackTarget` in front, as
   today (`SimWorld::engage`).
2. **Outside an attack order, a weapon that may fire acquires its own target** (S11): the best enemy it reaches from
   where the unit is, by the same ranking acquisition uses; it does not change the unit's order or move it.
3. **Whether the unit may be moving when the weapon fires is `AllowedMovement`** (S10): `Moving` fires while
   moving; `Slowing` and `Stopped` fire only when the unit is not walking under its order. Whether the weapon sits on
   a turret plays no part.
4. **A weapon's wind-up holds its unit unless the weapon is `Moving`** (S10). Today the hold is keyed on the mount.
5. **A turret is an aiming device.** A turret weapon aims by its turret and a hull weapon by the hull, as today
   (`slew_towards`). Outside an attack order the hull belongs to the unit's order, so a hull weapon fires only at what
   already lies in its arc (rule 1 of S11's reading, [INF]).

### 2.3 Survey: the two fields for every modelled weapon

Every weapon with an analogue in the catalog was resolved (S1's chain, comments stripped). The table lists the
weapons whose values differ from the defaults; every other modelled weapon has `AllowedMovement` unset (engine
default, `Stopped`) and both options at 1 (`core` WeaponData:20–21): Marine `GuassRifle`, Roach `AcidSaliva`, Zealot
`PsiBlades`, Siege Tank `90mmCannons` and `CrucioShockCannon`, Marauder `PunisherGrenades`, Reaper
`P38ScytheGuassPistol`, Ghost `C10CanisterRifle`, Hellion `InfernalFlameThrower`, Thor `ThorsHammer` and
`LanceMissileLaunchers`, Stalker `ParticleDisruptors`, Sentry `DisruptionBeam`, Adept `Adept`, High Templar
`HighTemplarWeapon`, Dark Templar `WarpBlades`, Zergling `Claws`, Ravager `RavagerWeapon`, Hydralisk `NeedleSpines`,
Lurker `LurkerMP`, Ultralisk `KaiserBlades`, Queen `TalonsMissile` and `AcidSpines`, Missile Turret
`LongboltMissile`, Photon Cannon `PhotonCannon`, Spine Crawler `ImpalerTentacle`, Spore Crawler `AcidSpew`.

| Analogue (slug) | 5.0.14 weapon | `AllowedMovement` | Order option (`OnlyFireWhileInAttackOrder`) | Turret | Decision |
|---|---|---|---|---|---|
| **Phoenix** (`flying_saucer`) | `IonCannons` | **Moving** (`swarm` WeaponData:181) | **0** (`swarm`:177) | yes | **In:** `Moving`, `fires_outside_attack_order` (B-45) |
| **Void Ray** (`prismatic_saucer`) | `VoidRaySwarm` | **Moving** (`void`:176) | 1 (S6) | no | **In:** `Moving` (WP1 data; acts in WP3) |
| **Immortal** (`void_goliath`) | `PhaseDisruptors` | **Slowing** (`voidmulti`:207) | **0** (`balancemulti`:185) | yes | **In:** `Slowing`, `fires_outside_attack_order` |
| **Colossus** (`tripod_annihilator`) | `ThermalLances` | unset → Stopped | **0** (`balancemulti`:229) | yes | **In:** `fires_outside_attack_order` |
| Baneling (`volatile_tick`) | `VolatileBurst` | **Moving** (`liberty`:470) | 1 | no | **In:** `Moving` (no observable change: damage point 0, never in wind-up) |
| SCV (`welder_drone`) | `FusionCutter` | **Slowing** (`voidmulti`:104) | 1 | no | **In:** `Slowing` |
| Probe (`grav_harvester`) | `ParticleBeam` | **Slowing** (`voidmulti`:196) | 1 | no | **In:** `Slowing` |
| Drone (`nectar_worker`) | `Spines` | **Slowing** (`voidmulti`:219) | 1 | no | **In:** `Slowing` |
| Viking (`vector_interceptor`) | `LanzerTorpedoes` | **Slowing** (`liberty`:274) | 1 | no | **In:** `Slowing` |
| Liberator (`sky_bastion`) | `LiberatorMissileLaunchers` | **Slowing** (`void`:329) | 1 | no (B-89) | **In:** `Slowing` |
| Banshee (`cloak_gunship`) | `BacklashRockets` | **Slowing** (`liberty`:557) | 1 | no | **In:** `Slowing` |
| Mutalisk (`sonic_bat`) | `GlaiveWurm` | **Slowing** (`liberty`:406) | 1 | no | **In:** `Slowing` |
| Corruptor (`sky_leviathan`) | `ParasiteSpore` | **Slowing** (`liberty`:569) | 1 | no | **In:** `Slowing` |
| Brood Lord (`spore_matriarch`) | `BroodlingStrike` | **Slowing** (`liberty`:418) | 1 | no | **In:** `Slowing` (its weapon is B-21's stand-in) |
| Planetary Fortress (`planetary_bulwark_dome`) | `TwinIbiksCannon` | unset | 1 (only `OnlyFireAtAttackOrderTarget 0`, S16) | yes | Defaults; the target option is B-180 |
| Carrier (`mothership_carrier`) | interceptors (B-22) | — | — | — | Defaults; out with B-22 (our weapon is a stand-in salvo) |

**What the `Slowing` rows change.** Under D-3 `Slowing` and `Stopped` behave alike: no weapon in an attack order is
gated by movement (the pursuit already stops in the band), and only the weapons with the order option cleared (three)
or `Moving` (three) are read by any rule. So every `Slowing` row is data only, read by no rule that distinguishes
it, and recorded so that B-178's later rule finds it in place. P-5 checks the values.

### 2.4 Neighbouring rules: in or out

| Rule | Source | Decision |
|---|---|---|
| `OnlyFireAtAttackOrderTarget 0` (Phoenix, Immortal, Colossus, Planetary Fortress): inside an attack order the weapon may fire at another target | S1, S4, S5, S7, S16 | **Out, registered B-180.** Our unit chooses one weapon per target plane, so the order's target is the weapon's target; modelling a weapon that fires elsewhere during an attack needs a per-weapon target |
| `Slowing` against `Stopped`: a `Slowing` weapon may start while the unit brakes, a `Stopped` one only at rest | S10 | **Out, registered B-178.** Combat cannot tell braking from a push without the movement phase (D-3); under attack orders every weapon today starts while braking into its band |
| `LockTurretWhileFiring` (Hellion) | S14 | **Out, registered B-179.** Ours slews the turret through the wind-up (`step_windup`) |
| `NoDeceleration` (workers, Baneling, Mutalisk) | S15 | **Out, registered B-181.** A movement rule (B-39's glide), under `movement/` |
| Patrol and `AcquireMove` | S9 | **Out.** Neither order is modelled (`UnitOrder` has none); when one is, it is an acquiring order like attack-move (**verify**) |
| Hold Position | S12 | **Kept as today:** a holder's weapon works against `current_target`, tracked within reach (`Engagement::Hold`) |
| Turret `YawArc` | `core` TurretData:9 (default 180); every modelled turret is `FreeRotate` or the Lurker's `YawArc 360` (`void` TurretData:14–19) | **Unchanged:** the modelled turrets turn freely, as ours do |
| B-89 (the Liberator's anti-air turret) | `void` UnitData:3317–3318 | **Out**, unchanged (research row) |

---

## 3. Diagnosis (on `d71f7153`, read from the code)

- **Target selection** (`SimWorld::advance_weapon_cycles`, `crates/sim_core/src/combat_resolution.rs`): a swing in
  `Windup` keeps its bound target; otherwise an `AttackTarget` front order names the target; otherwise
  `(has_turret || entity.is_on_hold()).then_some(entity.current_target)`. So any turreted weapon fires at
  `current_target` under any order (B-90, first half).
- **`current_target` under a move.** `Entity::commit_order` does not clear it for `Move` (`handle_interrupting_move`),
  and acquisition treats a `Move` front order as `Engagement::Busy` (`engagement_of`,
  `crates/sim_core/src/target_acquisition.rs`), so it is never refreshed: a turreted unit that is moved away keeps
  firing at the unit it last engaged, while that unit stays in reach.
- **The wind-up hold** (`walking_goal`, `crates/sim_core/src/movement/locomotion.rs`):
  `if matches!(actor.weapon_phase, WeaponPhase::Windup { .. }) && !has_turret { velocity = 0 }`. A turreted unit
  walks through its wind-up; a hull unit stands even when its weapon is `Moving` (the Void Ray). After a `Move`
  order resets a wind-up (`handle_interrupting_move`), the turreted unit re-enters `Ready`, starts a new swing at
  `current_target` on the next tick, and is not held: it fires while moving (B-90, second half).
- **Pursuit** (`Pursuit::aim_if_in_range`) stops every unit in its band and turns a hull towards the goal; a turret is
  left to the weapon phase. Correct for both mounts, and unchanged here.
- **Hull slew outside an attack order.** `step_ready_weapon` turns the mount towards a target outside its arc
  (`slew_towards` → `Entity::turn_hull_towards` for a hull weapon). Today only holders reach this without an attack
  order; a hull weapon firing under a move would fight the locomotion's heading.
- **Contact.** The shipped pipeline is `Legacy` (`assets/catalog/movement_rules.ron`), where
  `is_anchored_in_combat` treats `RotatingToTarget`, `Windup` and `Backswing` as anchored. So a turreted unit
  turning or winding up at a stale target under a move is anchored today; after WP2 it is not (§13 R-3).
- **Tests that assert the defect.** `turret_tracking_tests::test_independent_turret_tracking_while_retreating` sets a
  tank's `current_target`, orders it to `Move` away and asserts that it fires while retreating. It is a charter
  subject file (`docs/guides/test-gates.md`, "turrets") and is restated in WP2 (§12).
- **Where turreted units appear.** Matrix battles 2a, 2b, 3 (Immortal, Siege Tank), the Hellion and Colossus battles,
  the Void Ray battles (`prismatic_saucer`), the Lurker battle and the sieged-tank battles 37a–39b; all sides take
  `AttackMove`, `HoldPosition` or no order (the fixture has no `Move`, `SideOrder` in
  `crates/sim_fixtures/src/matchup.rs`). Showcase effects (`showcase_siege_line`: tanks, Lurkers; `showcase_raiders`:
  Hellions, tanks) under `AttackMoveToCell`. `golden_replay.ron` ("red_siege", a tank, under `AttackMove`);
  `golden_replay_orders.ron` ("red_siege"; "blue_spotter", the Phoenix, under `Move(12, 32)` at tick 10).

---

## 4. Design decisions

### D-1. Two fields on `WeaponSpec` (the rules' data)

```rust
/// Whether a weapon's unit may move while it attacks: `StarCraft` II's `CWeapon` `AllowedMovement`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AllowedMovement {
    /// The unit stands to fire (no `AllowedMovement`, the engine default): its wind-up holds it.
    #[default]
    Stopped,
    /// The unit may still be decelerating when it fires (`Slowing`). Today treated as `Stopped` (B-178).
    Slowing,
    /// The unit fires while it moves (`Moving`): its wind-up does not hold it.
    Moving,
}
```

- `WeaponSpec::allowed_movement: AllowedMovement` and `WeaponSpec::fires_outside_attack_order: bool` (the inverse of
  `OnlyFireWhileInAttackOrder`; `false` is the engine default, S1). Placed after `turret_idle`, in the struct and in
  `fields!(WeaponSpec { … })`.
- **Catalog.** `WeaponSpecDef` gains both with `#[serde(default)]`, documented "omitted reads the engine default".
  They are converted unchanged; no float is added (C7 allowances unchanged). A mode override carries its own (the
  sieged tank's `CrucioShockCannon` keeps the defaults).
- **Why not reuse `has_turret`.** The mount says how a weapon aims (`slew_towards`, `weapon_aim`, the pursuit's
  `aim_if_in_range`, the turret idle), and nothing about when it may fire. The data keeps the two apart (the Void Ray
  is `Moving` without a turret; the Siege Tank turreted and `Stopped`).
- **Alternative considered: one enum combining both** ("fires: InAttackOrder | Anywhere"). Rejected: the two are
  independent in the data (Immortal and Colossus clear the option without `Moving`; the Void Ray is `Moving` without
  clearing it).

### D-2. The order rule (B-90, first half)

In `advance_weapon_cycles`, the target a weapon works against when it is not mid-swing becomes:

```text
if front order is AttackTarget { target }            → target            (unchanged)
else if entity.is_on_hold()                          → current_target    (unchanged; S12)
else if weapon.fires_outside_attack_order
     && (weapon.allowed_movement == Moving || !entity.order_walks())
                                                     → current_target    (tracked by D-4)
else                                                 → none
```

The weapon read is the one the entity would use against `current_target`'s plane, resolved as today
(`resolved_weapon`, the siege weapon first). A swing in `Windup` keeps its bound target as today; a `Backswing`
with no target runs out as today. The `has_turret` read disappears from target selection.

### D-3. "Walking under its order": `Entity::order_walks`

A new `pub(crate) fn order_walks(&self) -> bool` in `crates/sim_core/src/entity.rs`: true when the front order is
`Move` or `AttackMove`, a task order with a goal (`has_task_order() && task.goal.is_some()`), or a `CastAbility`
(whose approach may walk). It mirrors the classification `walking_goal` makes, without reading the movement phase,
and a unit test checks both agree on every `UnitOrder` variant (P-2).

**Why the order and not the displacement.** `Entity::velocity` after the movement phase is the displacement
achieved, collision pushes included (`crates/sim_core/src/movement/collision.rs`, the "achieved" write). 5.0.14
does not cancel an attack because a unit is pushed, and friends may nudge an engaged unit along its ring (D-δ, Q0052
Addendum). Reading the order keeps pushes out, and keeps the rule in combat code. The cost is B-178: braking under
an attack order is not told apart from standing.

### D-4. Acquisition for a weapon that fires outside an attack order

`engagement_of` gains `Engagement::Track` for a unit whose front order would be `Busy` today (a non-attack, non-hold
order) and whose archetype has a weapon that may fire under it by D-2 (`fires_outside_attack_order`, and `Moving` or
`!order_walks()`). `Track` behaves as `Hold` does: only enemies its weapon reaches from where it stands
(`ScanScope::stationary`), only candidates whose weapon by plane satisfies the same condition, `current_target` set
or released (`ReleaseHoldTarget`), and the order never changed (`engage` already returns for a front order it does
not own). Every other `Busy` unit is unchanged, so a unit without such a weapon keeps its stale `current_target`
exactly as today (it is no longer fired at; §14 F-4).

`holds_fire` (the defensive acquire level of workers) is unchanged: workers' weapons keep the option (§2.3).

### D-5. A hull weapon outside an attack order does not turn the hull

In `step_ready_weapon` and `step_rotating_weapon` (`crates/sim_core/src/combat.rs`), a weapon working against a
target outside an attack order (D-2's third branch) does not enter `RotatingToTarget` for a hull mount: it starts a
swing when the target lies in its arc and otherwise waits. A turret slews as today. `advance_weapon_cycles` passes the
branch to `step_entity_weapon` as a small `Mandate` value (`AttackOrder` or `OwnInitiative`), so no rule reads
the order queue twice. No shipped hull weapon has `fires_outside_attack_order` (§2.3), so D-5 is latent and tested on
a scratch weapon (P-4).

### D-6. The wind-up hold follows `AllowedMovement` (B-90 second half, B-45 Void Ray; WP3, under `movement/`)

In `walking_goal`, the condition `Windup && !has_turret` becomes `Windup && weapon.allowed_movement != Moving`, for
the weapon the unit is swinging (the same `siege_weapon → ground_weapon → weapon` resolution the line uses today;
**verify** in WP3 whether an air-weapon swing should resolve the air weapon, which today's line does not consult,
and fix it to the weapon of the swing's target plane if so). Effects: the turreted Tank, Hellion, Immortal and
Colossus stand for their wind-up during a chase; the Void Ray and the Phoenix do not; every other unit as today.

**Why not from the combat side.** The alternative was to cancel a non-`Moving` wind-up on any tick the unit was
displaced. It is rejected: it would cancel rather than hold (5.0.14 holds the unit; the swing completes), and the
displacement includes pushes (D-3).

### D-7. Data (WP1)

Every row of §2.3's "In" decisions, in `assets/catalog/units.ron`, each with a comment citing its source:
`flying_saucer` (`allowed_movement: Moving`, `fires_outside_attack_order: true`), `prismatic_saucer` (both weapon
entries `Moving`), `void_goliath` (`Slowing`, `true`), `tripod_annihilator` (`true`), `volatile_tick` (`Moving`),
and `Slowing` on `welder_drone`, `grav_harvester`, `nectar_worker`, `vector_interceptor`, `sky_bastion`,
`cloak_gunship`, `sonic_bat` (both entries), `sky_leviathan`, `spore_matriarch` (both entries). No structure row
changes. Every other weapon keeps the defaults.

The data lands with the fields in WP1, before any rule reads it, so that WP1 is latent by construction and WP2's
moves are all the rule's.

### D-8. Validation

`WeaponSpecDef` validation refuses `fires_outside_attack_order` on a `suicide_on_attack` weapon: a unit would
detonate at a target it never chose, and no 5.0.14 weapon combines them (the Baneling keeps the option, §2.3). The
message names the field; a unit test covers it. No other combination is refused: `Moving` without the option (the
Void Ray) and the option without `Moving` (Immortal, Colossus) are both in the data.

### D-9. Hashed state and checksum

Both fields are hashed with `WeaponSpec`, in their struct position (D-1), so every projectile's encoding gains two
bytes. `Engagement::Track` and `Mandate` are transient (computed each tick). No entity field is added. The schema is
bumped once, in WP1, to the next free number at merge (§5).

### D-10. What the simulation publishes

Nothing new. A Phoenix firing under a move publishes its `AttackSwing` (from `weapon_phase`) and its beam events as
any swing does, while its `anim` stays `Walk` (§9).

### D-11. Records

- `DESIGN.md` §4: B-90 fixed (WP2 the order rule; WP3 the hold), B-45 fixed (WP2 the Phoenix; WP3 the Void Ray, with
  the correction that the Void Ray fires while moving only inside an attack order, S6).
- New rows, registered in WP0: **B-178** (`Slowing` treated as `Stopped`), **B-179** (the Hellion's
  `LockTurretWhileFiring`), **B-180** (`OnlyFireAtAttackOrderTarget 0` not modelled), **B-181** (`NoDeceleration`
  not modelled). Each "open, deferred; plan `2026-11-23-turret-wind-up` §14".
- Deviation-fixes plan: B-45's and B-90's rows point here.

---

## 5. Determinism and checksum

- **Integers and enums only.** The fields are a three-valued enum and a bool; the rules compare them. No float is
  added (C7).
- **No new collection or iteration order.** Acquisition keeps its read-only scan and applies decisions in entity
  order; `Track` uses the scratch buffers of `Hold`.
- **Goldens.** Both replays contain turreted units (§3). WP1 changes their bytes only through the schema and the
  projectile encoding.
- **WP1's proof** (the procedure of the strike-delays plan, §5):
  1. With both fields left out of the encoding and the schema at 38, the tree reproduces every recorded checksum of
     both goldens and `DEFAULT_SANDBOX_CHECKSUM` (`crates/sim_fixtures/tests/fixture_tests.rs`), on native debug and
     release and on `wasm32-wasip1` release and debug (`CARGO_TARGET_WASM32_WASIP1_RUNNER="wasmtime --dir=<real tree
     path>"`; the script `~/scratch/strike-delays/scripts/proof.sh`, copied).
  2. Then the fields go in, the schema becomes the next free number `N` (39 unless taken), and the checksums are
     re-recorded under the header "Re-recorded for plan 2026-11-23-turret-wind-up WP1 (checksum schema 38 -> N):
     each weapon also encodes whether its unit may move while it attacks and whether it fires outside an attack
     order, so every checkpoint changed. No behaviour did: no rule reads either field yet, and with them left out of
     the encoding at schema 38 both fixtures reproduce as recorded for plan 2026-11-23-strike-delays WP1." All four
     builds reproduce the new values.
  3. If `main` takes `N` first, the merge repeats step 2 with the next free number.
- **WP2 and WP3 change no encoding.** A golden move in either is traced to its first checkpoint and entity and
  explained by the rule, then re-recorded with a header naming the package, the entity and the rule; an unexplained
  move stops the package (§8). The prediction is in §7.
- **wasm.** No dependency changes. **Cost:** D-2 adds a field read per armed entity per tick; D-4 adds a scan only
  for a unit with such a weapon under a non-attack order (the Phoenix, Immortal and Colossus analogues when moved).
  The throughput bench is checked in the package gate as usual.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure from a seed. Constructed tests find weapons by property (`has_turret`,
`allowed_movement`, `fires_outside_attack_order`, plane), never naming a slug in a rule; the P-5 table names
analogues (as `b2_strike_spacing_follows_starcraft` does), and P-6 finds its units by property.

- **P-1 (order rule).** For a weapon without `fires_outside_attack_order`, turreted or not: a unit whose
  `current_target` is a live enemy in reach starts no swing under `Move`, `AttackMove` with no `AttackTarget` in
  front, `Stop` followed by an order that walks, or a task order; it starts one under `AttackTarget` and on hold. A
  turreted copy of a hull weapon behaves identically (the mount plays no part).
- **P-2 (walking rule).** For a scratch weapon with `fires_outside_attack_order` and `allowed_movement` ∈ {`Stopped`,
  `Slowing`}: no swing while `order_walks()`, a swing when the order does not walk (a non-walking `Busy` order). With
  `Moving`, a swing in both. `order_walks()` agrees with `walking_goal`'s classification for every `UnitOrder`
  variant (unit test in `entity.rs`).
- **P-3 (tracking).** Under a walking order, a unit with a `Moving`, `fires_outside_attack_order` weapon fires at the
  best enemy it reaches (by `TargetPriorityKey`), never at one beyond reach, never changes or loses its order, and
  releases the target when it leaves reach. A unit without such a weapon keeps its `current_target` untouched under
  the same order (no new scan).
- **P-4 (hull).** A scratch hull weapon with `fires_outside_attack_order` and `Moving`, under `Move`, never changes
  the hull's heading through the weapon (the heading equals the locomotion's), fires at a target in its arc and not
  at one behind it.
- **P-5 (the shipped values).** Every shipped weapon's `allowed_movement` and `fires_outside_attack_order` equal
  §2.3's table (the defaults for every unlisted analogue), each row citing its source.
- **P-6 (shipped behaviour).** The turreted ground analogue with neither field set (the Siege Tank's, found by
  property) ordered to `Move` away from an enemy it had attacked deals no damage while it moves and keeps its order;
  the turreted air analogue with `Moving` and the option (the Phoenix's) ordered to `Move` past an enemy air unit
  deals damage while its position changes every tick of the pass.
- **P-7 (zero recompilation).** In a catalog copy, setting the Phoenix's two fields to the defaults makes it hold
  fire under `Move` in the same binary (`runtime_catalog_tests.rs`).
- **P-8 (validation).** D-8's refusal, with the field named; an omitted field reads the default.
- **P-9 (unchanged inside an attack order).** WP2 changes nothing for a unit whose front order is `AttackTarget` or
  which holds: every test of `weapon_cadence_tests`, `stutter_step_tests`, `target_selection_tests`,
  `hull_turning_tests`, `multi_strike_timing_tests` and `attack_move_simulation_tests` passes unchanged, except the
  restated `turret_tracking_tests` (§12).
- **P-10 (the wind-up hold, WP3).** A unit in wind-up whose weapon is not `Moving` has zero velocity from its own
  motion that tick, turreted or not; a unit whose weapon is `Moving` keeps walking through its wind-up (a scratch
  hull weapon and the Void Ray's, found by property).
- **P-11 (confinement).** WP1 changes no world (all of §7 byte-identical; goldens and sandbox by the schema bytes
  only).
- **P-12 (direction, a prediction recorded before measuring, not a threshold).** WP2: in any measured world that
  moves, the turreted units' damage dealt while their front order is a `Move` falls to zero; no measured world is
  expected to move (§7). WP3: in the matrix battles that move, the turreted ground units release at least as many
  shots per battle as before (they stand instead of chasing during the wind-up), and the Void Ray's battles end no
  later on average; no winner is expected to change. A result against this is traced (§8 rule 3).

---

## 7. Worlds that move

**Baseline:** WP0's tree (behaviour-identical to `d71f7153`), measured in WP0.

**Commands.** These are the strike-delays plan's. Binaries are built once per tree and run from that tree so its
assets are read. Outputs go under `~/scratch/turret-wind-up/`; the scripts are copied from
`~/scratch/strike-delays/scripts/` (`measure.sh`, `compare.sh`, `firstdiff.py`, `proof.sh`, `client.sh`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/turret-wind-up/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/turret-wind-up/bin-<wp>/
~/scratch/turret-wind-up/scripts/measure.sh ~/scratch/turret-wind-up/bin-<wp> <tree> ~/scratch/turret-wind-up/world-<wp>
~/scratch/turret-wind-up/scripts/compare.sh ~/scratch/turret-wind-up/world-wp0 ~/scratch/turret-wind-up/world-<wp>
```

`measure.sh` runs the matrix (16 seeds and the seeds of record), battle 31 (100 seeds) and battle 33 (64 seeds), the
showcase and the showcase effects scenario on the default seed and seeds 1–8 (6,000 ticks each). Run in the
background, one at a time across worktrees (charter §7). The client skirmish is compared by its stage log
(`client.sh`).

**No probe.** The matrix fixture has no `Move` order (`SideOrder`), and a scenario only its initial orders, so a
"retreat under Move" world cannot be written without extending the fixture (F-3). The move of WP2 is shown by the
constructed tests P-1 to P-7, which are the acceptance.

| World | WP1 (fields, data; latent) | WP2 (rules) | WP3 (hold, after the movement branch) |
|---|---|---|---|
| Matrix (16 seeds + seeds of record) | byte-identical | **expected byte-identical**: sides take `AttackMove`, `HoldPosition` or none, so a turreted unit fires inside an `AttackTarget` acquisition or on hold, which D-2 keeps; a move is traced to a tick where a weapon fired outside an attack order | **moves** where a turreted ground unit (Immortal battles 2a, 2b, 3; Tank battle 3; Hellion battles; Colossus battles) or the Void Ray (its battles) is in wind-up while its pursuit would move it; P-12 |
| Battles 31, 33 | byte-identical | byte-identical (no turreted unit, no flagged weapon) | byte-identical (Stalker, Marauder, Marine, Zealot weapons are `Stopped` hull weapons: unchanged hold) |
| Showcase, default seed and 1–8 | byte-identical | expected byte-identical: `battle_automaton` (SCVs, Marines, Roaches) under `AttackMoveToCell`; trained units walk to rallies under `Move` with no `current_target` (**verify** the unit census in WP0) | byte-identical unless the census finds a turreted or `Moving` unit |
| Showcase effects, default seed and 1–8 | byte-identical | expected byte-identical (`AttackMoveToCell`; **verify** in WP0 that no unit is ordered to move later); a move is traced | **may move**: tanks and Hellions chase during wind-ups |
| Client skirmish stage log | identical | identical (the pilot trains `pulse_bot`) | identical |
| Goldens, sandbox | schema bytes only (§5) | `golden_replay.ron`: expected unchanged ("red_siege" under `AttackMove`). `golden_replay_orders.ron`: expected unchanged unless "blue_spotter" (Phoenix, air weapon only) passes within reach of an enemy air unit under its `Move` (**verify** by the WP0 census: the fixture's red squads are ground units, so no) | may move where "red_siege" chases during a wind-up; traced and re-recorded with a header (§5) |
| Constructed worlds (§12) | P-5, P-8 | P-1 to P-4, P-6, P-7, P-9 | P-10 |

**What WP2 and WP3 record** in the evidence: the comparison of every world against WP0, the census of §7's
**verify** items, and for each moved battle or golden, the first differing tick with the entity and the rule that
moved it (`firstdiff.py`).

---

## 8. Stop rules

1. **WP1 is latent.** Every world of §7 byte-identical, goldens and sandbox by the schema bytes only, proved (§5).
   Anything else is a defect of WP1.
2. **WP2 moves only what D-2 to D-5 explain.** Every first differing tick in a measured world or golden must show a
   weapon that, before WP2, fired (or turned, or tracked) outside an attack order. Any other first difference stops
   the package until it is traced; a second cause is registered as a bug, and WP2 lands only if it is separable.
3. **A result against P-12** (a turreted ground unit releasing fewer shots after WP3, a winner change, a Void Ray
   battle ending later on average) stops WP3 until it is traced. A winner change that the trace explains by the hold
   (a unit that stood for its wind-up instead of chasing) is recorded with the matrix expectation re-derived and a
   claim stating why, never by widening a range without a reason.
4. **No tuning constant.** The only values are §2.3's data.
5. **WP3 does not start** while `crates/sim_core/src/movement/locomotion.rs` is owned by an unmerged branch (§11).

---

## 9. Client consequences

- **The Phoenix analogue fires while it walks.** Its `AttackSwing` and beam events (`prism_beam`, `beam_burn`) are
  published during a `Move`, while its `anim` stays `Walk`. `displayed_action` (`crates/client/src/flipbook_clock.rs`)
  turns `Idle` with a swing into `Attack` and leaves `Walk` alone, so the flipbook keeps its walk loop and the beam is
  drawn over it (**verify** in WP2 by the effect snapshot tests; no client code change is planned). 5.0.14 shows the
  Phoenix's flight animation while it fires on the move, so this is the intended look.
- **Turreted units stop shooting under a move.** The client has no rule of its own about this; the command card,
  stance and admission are unchanged (`StanceOrders`, `Entity::settled_armed`).
- **WP3:** turreted units stand still for their wind-up during a chase; the Void Ray keeps gliding. Presentation
  follows `anim` and `AttackSwing` as today.
- **Code:** none. **Data:** none in the client's tables. The package gate runs the client tests its path rules
  select (the skirmish among them).

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP4 | D-11 (rows B-178 to B-181 in WP0; B-45, B-90 fixed in WP4) |
| `docs/plans/README.md` | plan, WP1, WP4 | Row |
| `crates/sim_types/src/weapon.rs`, `crates/sim_types/src/lib.rs` | WP1 | `AllowedMovement`, the two fields, docs, re-export |
| `crates/sim_core/src/catalog.rs` | WP1 | `WeaponSpecDef` fields and conversion |
| `crates/sim_core/src/catalog/validation.rs` | WP1 | D-8 and its unit test |
| `crates/sim_core/src/checksum/encoding.rs`, `crates/sim_core/src/checksum.rs` | WP1 | D-9, schema `N` |
| `crates/sim_core/tests/fixtures/golden_replay.ron`, `golden_replay_orders.ron`; `crates/sim_fixtures/tests/fixture_tests.rs` (`DEFAULT_SANDBOX_CHECKSUM`); the encoding unit tests' schema byte | WP1 (WP2, WP3 if moved) | §5 |
| Test files with `WeaponSpec { … }` literals (`grep -rln first_strike_delay_ticks crates/*/tests crates/*/src`) | WP1 | The two fields at their defaults (mechanical) |
| `assets/catalog/units.ron` | WP1 | D-7 |
| `crates/sim_core/tests/weapon_catalog_rules_tests.rs` | WP1 | `SC2_WEAPON_MOVEMENT`, `b90_weapon_movement_follows_starcraft` (P-5) |
| `crates/sim_core/tests/checksum_state_tests.rs` | WP1 | Each field changes the checksum of a world with a projectile |
| `crates/sim_core/src/combat_resolution.rs` | WP2 | D-2 (target selection, `Mandate`) |
| `crates/sim_core/src/combat.rs` | WP2 | D-5, `step_entity_weapon`'s signature, docs |
| `crates/sim_core/src/entity.rs` | WP2 | `order_walks` (D-3) and its unit test |
| `crates/sim_core/src/target_acquisition.rs` | WP2 | D-4 (`Engagement::Track`), module invariant 2 restated |
| `crates/sim_core/tests/turret_tracking_tests.rs` | WP2, WP3 | Restated (§12) |
| `crates/sim_core/tests/target_selection_tests.rs` | WP2 | P-1, P-2, P-3 |
| `crates/sim_core/tests/hull_turning_tests.rs` | WP2 | P-4 |
| `crates/sim_core/tests/runtime_catalog_tests.rs` | WP2 | P-7 |
| Callers of `step_entity_weapon` in tests (`weapon_cadence_tests`, others; `grep -rn step_entity_weapon crates`) | WP2 | The `Mandate` argument (mechanical) |
| `crates/sim_core/src/movement/locomotion.rs` | **WP3 only** | D-6 (one condition in `walking_goal`) and its docs |
| `crates/sim_core/tests/stutter_step_tests.rs` | WP3 | P-10 |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP3 (only under §8 rule 3) | A re-derived expectation with its reason |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP4 | B-45's and B-90's rows point here |
| `docs/evidence/2026-11-23-turret-wind-up.md` (new) | WP0 to WP4 | §7 |

`crates/gate/suites.ron` needs no change: every test is in an existing target (**verify** in WP1). Nothing under
`crates/sim_core/src/movement/` changes before WP3.

---

## 11. Work packages (commit order)

Each package runs on one branch from `main`, in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>` (check `quota -s` and `df -h ~/scratch` first). Failing
tests come first, with their red run pasted in the hand-back. Then `cargo gate quick` and the tests the change needs,
and a verifier runs `cargo gate package` (Q0061). §7's comparison runs in every package against WP0's tree. No
`unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust (C5); tests find weapons by
property. Check `docs/questions/open/` before the last commit. WP0 to WP2 may run as one branch and one session, one
commit each, with one `package` gate on the final tree (charter §4.3); WP3 is a separate branch.

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-45's and B-90's rows point to this plan; B-178 to B-181 registered (D-11).
- Evidence file created with §2.3's survey and the resolver script's path.
- Measure WP0's tree (§7) for the baseline; take the censuses §7 marks **verify** (showcase units, later orders in
  the showcase effects scenario, the units "blue_spotter" passes in `golden_replay_orders.ron`).
- **Gate:** docs-only.

### WP1: the fields and their data, latent (M, `sim_types`, `sim_core`, data)

Failing tests first (red because they do not compile, then on a field-only stub because the data is missing):
- `weapon_catalog_rules_tests::b90_weapon_movement_follows_starcraft` (P-5): the table `SC2_WEAPON_MOVEMENT` of
  `(analogue, allowed_movement, fires_outside_attack_order, source)` for §2.3's rows; every other weapon implied at
  the defaults, citing `core` WeaponData:20–21 and S2.
- `validation.rs` unit tests (P-8).
- `checksum_state_tests`: each field changes the checksum of a world with a projectile.

Then D-1, D-7, D-8, D-9, the docs and the mechanical literals; then §5's proof. **Acceptance:** §8 rule 1. Plan row
`Active`. Commit `feat(sim_core): weapons state whether they fire while moving and outside attack orders
(turret-wind-up WP1, schema N)`.

### WP2: the order rule, tracking and the hull (M, `sim_core`; B-90 first half, B-45 Phoenix)

Failing tests first (red on WP1's tree):
- `turret_tracking_tests` restated (§12): the tank case red (it fires today), the Phoenix case red (it has no
  target under `Move` today).
- `target_selection_tests`: P-1 (a turreted and a hull scratch copy, five orders), P-2, P-3.
- `hull_turning_tests`: P-4.
- `runtime_catalog_tests`: P-7.
- `entity.rs` unit test: `order_walks` against every `UnitOrder` variant.

Then D-2 to D-5. **Acceptance:** §8 rule 2; P-9's guards pass unchanged. Commit
`fix(sim_core): only an attack order, or a weapon's own data, lets it fire; the Phoenix analogue fires while moving
(turret-wind-up WP2, B-90, B-45)`, stating which worlds moved and why.

### WP3: the wind-up hold follows `AllowedMovement` (S, `movement/`; B-90 second half, B-45 Void Ray)

**Starts only when** `contact-solver-plan` has merged to `main` (or its owner releases
`crates/sim_core/src/movement/locomotion.rs` in writing in that plan), checked with `git worktree list` and the
plans index. If the solver pipeline has replaced `walking_goal` by then, D-6's condition goes where the wind-up hold
went, under both pipelines while both exist.

Failing tests first (red on WP2's merged tree):
- `stutter_step_tests::a_wind_up_holds_its_unit_unless_its_weapon_fires_while_moving` (P-10): a turreted scratch
  copy of a hull weapon stands; a `Moving` scratch hull weapon keeps walking.
- `turret_tracking_tests`: the shipped turreted ground analogue in a chase stands for its wind-up; the Void Ray
  analogue (found by property: a hull weapon with `Moving`) keeps moving through its wind-up inside an attack order.

Then D-6. **Acceptance:** §8 rules 3 and 5; §7 column WP3. Commit
`fix(sim_core): a wind-up holds its unit unless its weapon fires while moving (turret-wind-up WP3, B-90, B-45)`.

### WP4: bookkeeping (XS)

D-11's records; the evidence completed; this plan's `## As built`; the index row `Done` (or `Partly done` with WP3
pending if WP2 merges first, as the orchestrator decides). Merge `main`; `cargo gate package` once on the final tree
(by the verifier). The schema is re-recorded if `main` took the number (§5 step 3).

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where |
|---|---|---|
| Types and validation | Defaults, D-8 | WP1: `catalog/validation.rs` unit tests |
| Data | Both fields of every weapon against 5.0.14 | WP1: `weapon_catalog_rules_tests.rs` (P-5) |
| Encoding | Each field changes the checksum | WP1: `checksum_state_tests.rs` |
| Order classification | `order_walks` against `walking_goal` | WP2: `entity.rs` unit test |
| Rules | P-1 to P-4 | WP2: `target_selection_tests.rs`, `hull_turning_tests.rs` |
| Shipped behaviour | P-6 | WP2: `turret_tracking_tests.rs` |
| Zero recompilation | P-7 | WP2: `runtime_catalog_tests.rs` |
| Wind-up hold | P-10 | WP3: `stutter_step_tests.rs`, `turret_tracking_tests.rs` |
| Worlds | P-11, P-12: §7's table | every package (measure scripts); goldens and sandbox in the package gate |
| Charter subjects | `sim_core`'s `stutter_step_tests.rs`, `turret_tracking_tests.rs` (both extended here), `damage_and_shields_tests.rs`, `push_priority_tests.rs`, `full_match_tests.rs`, `golden_replay_tests.rs`; `sim_fixtures`' `fixture_tests.rs`; `client`'s skirmish | package gate |

**`turret_tracking_tests.rs` restated (WP2).** The charter subject "turrets" stays covered in this file, with the
module doc rewritten to the rule. `test_independent_turret_tracking_while_retreating` becomes two tests:
`a_turret_that_needs_an_attack_order_holds_fire_while_retreating` (the turreted ground analogue with neither field
set, `current_target` on a pursuer, ordered to `Move` away: its hull translates at full speed every tick, as today,
and it never enters `Windup`) and `a_turret_that_fires_while_moving_tracks_and_fires_while_retreating` (the turreted
analogue with `Moving` and the option, against an enemy air pursuer: the turret turns independently of the hull and
the weapon fires while the hull translates at full speed every tick, today's assertions on a unit whose data allows
it). The old test asserted the defect (§3); its re-derivation is the point of B-90.

**Guards that pass unchanged:** P-9's list; `b2_strike_spacing_follows_starcraft`, `b81_first_strike_delay_follows_starcraft`;
`stutter_step_tests` (Marine `Move` cases: hull weapon, no flag, no `current_target` firing; **verify** in WP2 that
no case relies on firing under `Move`).

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | S10/S11's meaning (community sources) is wrong in a detail | A unit fires, or holds fire, where 5.0.14 differs | The fields are data (P-7); the reading every source shares is the one used; B-178 holds the one detail not modelled |
| R-2 | Hold Position's acquisition is not what S12 assumes | Holders fire as today, which may differ | Unchanged behaviour; **verify** recorded |
| R-3 | Under `Legacy` contact, a turreted unit with a stale target under a move was anchored (`RotatingToTarget`/`Windup`) and no longer is; a Phoenix firing under a move becomes anchored during its swing | Friendly traffic flows differently around these units | Both follow the weapon phase, which now follows the rule; measured in §7; any move traced to it is explained by stop rule 2 |
| R-4 | WP3 waits on another branch for long | B-90's second half and the Void Ray stay open | WP2 is independent and lands first; WP4 may close the plan as `Partly done` |
| R-5 | `walking_goal` resolves the ground-first weapon, not the swing's | An air-weapon swing of a unit with both weapons reads the wrong `allowed_movement` | D-6's **verify**; the Void Ray's two entries carry the same values, so no shipped unit differs |
| R-6 | Another branch takes schema 39 | Conflict at merge | "Next free at merge"; §5 step 3 |
| R-7 | Literal sites in tests conflict with other branches | Merge noise | Mechanical additions; merge `main` before the package gate |

---

## 14. Out of scope and decisions recorded (Q0002)

- **F-1: B-178, `Slowing` against `Stopped`.** Needs the movement phase to report "braking under the order" apart
  from pushes. Hook: `AllowedMovement` is already in the data; the rule reads `Slowing` when the movement phase
  exposes it (after the contact solver).
- **F-2: B-180, `OnlyFireAtAttackOrderTarget 0`.** Needs a per-weapon target inside an attack order. Hook: the field
  would sit beside `fires_outside_attack_order`; D-4's `Track` scan is the acquisition it would use.
- **F-3: a matrix fixture with `Move` orders and timed orders.** Would let the measured worlds show retreats and
  kiting (the WP2 move). Not added here: a fixture format change and new battles need their own claims.
- **F-4: clearing `current_target` on a non-attack order.** A unit without a flagged weapon keeps a stale
  `current_target` under a move, as today; no combat rule reads it after WP2, but the solver pipeline's
  `in_range_of_live_target` does (`movement/`). Left to the contact-solver plan.
- **B-179** (`LockTurretWhileFiring`), **B-181** (`NoDeceleration`): registered, deferred (§2.4).
- **Patrol, `AcquireMove`:** not modelled orders.
- **B-89** (Liberator turret), **B-22** (Carrier interceptors), **B-21** (Brood Lord): unchanged.

---

## 15. Owner question

None is needed. The values are in the data (§2.3); the meanings of the two editor fields are read the way every
available source reads them (S10, S11); the one detail they leave open, `Slowing`, has no shipped effect under this
plan and is registered (B-178). Q0101 stays free.

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/`
  (`5444e12f13`), read as data.
- The weapon resolver (override chain, `parent`, class defaults, XML comments stripped), copied by WP0 to
  `~/scratch/turret-wind-up/weapons.py` with its output for every modelled weapon (run with `python3 -I`). The
  planning copy is in the planning session's scratchpad.
- Community sources for S10 and S11 (SC2Mapster forum, read 2026-11-23):
  [Brood Lord won't attack while moving](https://www.sc2mapster.com/forums/development/data/155281-brood-lord-wont-attack-while-moving),
  [Attack move: the impossible task](https://sc2mapster.com/forums/development/data/157722-attack-move-the-impossible-task),
  [Valkyrie unit data](https://sc2mapster.com/forums/development/data/226927-valkyrie-unit-data?comment=2),
  [Moving Bunker: allowing attacks while moving](https://us.forums.blizzard.com/en/sc2/t/moving-bunker-allowing-attacks-while-moving/8036).
- No prototype: WP1 is latent by construction, and WP2's effect is shown by constructed tests (§7 "No probe").
