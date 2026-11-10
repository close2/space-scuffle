# Blueprint: Spellcaster Mechanics (Psionic Storm, Abduct, Fungal Growth, Blinding Cloud)

The owner answered [Q0077](../questions/open/0077-caster-spells.md) with option 1: the presentation contracts
(`RenderSnapshot::areas` and `links`) stay as plan [`2026-11-09-effect-visualization`](2026-11-09-effect-visualization.md)
landed them, and a dedicated plan builds the mechanics of the four missing caster spells, in StarCraft II order of
combat impact: **Psionic Storm, then Abduct, then Fungal Growth, then Blinding Cloud**. This is that plan.

This plan:

- records each spell's StarCraft II 5.0.14 values and rules from the game data, with sources, converted to ticks
  (§3);
- lists what the simulation already has and what is missing (§2);
- builds the spells from a few generic, data-driven primitives rather than from named spells (§4 and §5);
- states the rules per spell and their interactions with hold position, channels, transports, burrow, cloak,
  structures and massive units (§6 and §7);
- gives the client side, the matrix battles, the tests to write first and the measurements (§8 to §11), the undetermined rules, risks and scope (§14 to §16);
- splits the work into packages, one spell per package in the owner's order (§12).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-10 against `main` at
`d03ed19`. Line numbers drift, so search for the quoted symbol. Claims not confirmed while planning are marked
**verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0077, when the caster spells are built | Answered: option 1, this plan, in the order Storm, Abduct, Fungal, Blinding Cloud. Closed by WP7 | All |
| Q0051 (charter §8), mechanics follow StarCraft II 5.0.14 | Standing rule | §3, D-2, D-3 |
| Q0052, sanctioned deviations (S-5: 25 ticks per Faster second with whole-tick rounding) | Standing rule | D-4 |
| Q0080, outcome of the effect review gate | Open; its answer decides whether the spell effects are reviewed art or stay unused | D-11, WP2c to WP5c |
| Q0072, per-ability hotkeys | Open, default "keys stay positional"; followed here | D-10 |
| Q0061, implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §12 |
| Q0002, scoping is the agent's call if written down | Standing rule | §16 |
| None new | — | Every open rule has a default (§14) |

---

## 1. Summary

1. **Four spells, one small effect language.** The catalog gains generic effect primitives, not spells: a
   **status** (a timed per-unit effect with optional initial and periodic damage, a speed factor, a weapon range
   delta, stun, reveal, detection and boarding suppression), an **area** (a search at a point that applies a
   status to the units its filter admits, once or periodically), a **spell missile** (carries another effect to a
   point), and a **pull** (a tether to a unit that, on contact, moves it to the caster). Psionic Storm is a periodic
   area applying a damage status; Fungal Growth a missile carrying a one-shot area applying a damage-and-slow
   status; Blinding Cloud a periodic ground-only area applying a range-reduction status; Abduct a pull applying a
   stun status. No Rust names a spell (C5); the catalog names statuses by string keys in a new
   `assets/catalog/statuses.ron`.
2. **The game data decides, including two corrections to the brief.** All values come from SC2GameData 5.0.14
   (`5444e12f13`, chain `core → liberty → swarm → void → voidmulti`, as
   [`2026-11-04-catalog-time-audit`](../evidence/2026-11-04-catalog-time-audit.md) §2.2 established). Two results
   differ from the brief's wording: **Fungal Growth does not root at 5.0.14**, it slows by 75 % (`voidmulti`
   EffectData:1561 replaces the root with `FungalGrowthSlowMovement`, `MoveSpeedMultiplier="0.25"`; Liquipedia
   patch 4.0.0 "Slows the target's movement speed by 75%"), and **Psionic Storm's radius is 1.5 and its damage 80**
   at 5.0.14 (Liquipedia now shows the 5.0.15 values, radius 2 and 110 or 112 damage, which this plan must not
   adopt).
3. **Times stay exact, then round per pulse.** The game's periods are not whole ticks (Storm 10.196 ticks, Fungal
   4.464). Statuses and areas store their period and duration as fixed-point ticks and fire pulse *k* on the tick
   nearest *k* × period from the application, halves up, as the weapon cadence already does (B-3,
   `weapon_delay_carry`). This keeps every total exact: Storm 8 × 10 = 80, Fungal 16 × 1.5625 = 25.
4. **The cast pipeline grows up first.** Today a cast out of range is refused and dropped, there is no cast
   backswing, cast range is measured centre to centre, and a group order makes every selected caster cast. WP1
   adds approach into range, the game's edge-based range (`RangeUseCasterRadius`), the after-cast lock
   (`CastOutroTime`), target vision, and one caster per command (`BestUnit`), all latent (no shipped ability uses
   them; matrix and goldens bit-identical).
5. **Existing hazard zones are replaced.** `ActiveHazardZone`, `SpawnHazardZone`, `last_storm_damage_tick` and the
   hard-coded "+2" storm evacuation are removed; matrix battle 13 becomes a real cast. This fixes B-23, B-24 and
   B-78 and the spell half of B-69, and the Psionic Storm research of B-68.
6. **Client.** Card entries from the catalog's targeted abilities, a targeting mode for point and unit casts with
   the area's radius shown, locks for research, energy and cooldown, and headless input tests that cast each spell.
   Areas and tethers are drawn through the effect system; whether the drawing is reviewed art or stays unused
   waits on Q0080 (D-11).
7. **Measurement.** Eight new matrix battles (four spell battles, each with a control without the spell) with
   claims, battle 13 rewritten, battle 33 and every other battle bit-identical, goldens re-recorded only for two
   latent checksum schema changes (new state in WP1b, hazard removal in WP2b), each with render snapshots
   byte-identical, and otherwise unchanged; debug, release and `wasm32-wasip1`.

---

## 2. What exists

### 2.1 Simulation pieces reused

| Piece | Where | Use here |
|---|---|---|
| `UnitOrder::CastAbility { ability_index, target_pos, target_entity }` | `sim_types/src/commands.rs` | The order for all four spells; unchanged |
| `AbilitySpec { name, target_kind, energy_cost, cooldown_ticks, cast_range, effect, leave_name }`, `AbilityTargetKind::{TargetPosition, TargetEntity}` | `sim_types/src/commands.rs` | Gains three fields (D-6); target kinds reused |
| `admit_cast` → `admit_effect` → `commit_cast`, `would_admit_cast` (mutation-free admission shared with the client) | `sim_core/src/world/abilities.rs` | Extended per primitive; the "every check before any write" invariant kept |
| `is_ability_unlocked` and `UpgradeEffect::UnlockAbility { target_slug, ability_slug }` | `world/upgrade_stats.rs`, `upgrades.rs` | Psionic Storm research (`upgrades.ron` `psionic_storm`, id 47, `effects: []` today, B-68) |
| Energy: `max_energy`, `start_energy`, `energy_regen_per_tick` | `catalog.rs`, `units.ron` | Already 200 / 50 / 0.0315 on the High Templar and Viper analogues and 200 / 75 / 0.0315 on the Infestor analogue, matching 5.0.14 (§3.5) |
| `ability_cooldowns: [u16; 4]` on `Entity` | `entity.rs` | Storm's cooldown; four slots suffice (no analogue gets more than two spells here) |
| `ActiveBuff { buff_id, remaining_ticks, speed_multiplier, attack_rate_multiplier }` with hard-coded IDs 1 to 3 | `entity.rs` | Left alone (the concussive slow, B-77, may move to statuses later, §16); statuses are a separate list |
| Shields-first damage with barrier absorption (B-41) | `world/abilities.rs` `step_hazards`, `entity.rs` `apply_damage` | Factored into one `apply_spell_damage` used by every damage status |
| Retaliation and helpless flee (plan 22) | `target_acquisition.rs` `apply_combat_retaliation` | The data's `Acquire` and `Flee` damage responses (D-9) |
| `is_damageable`, `is_entity_targetable`, `is_cloak_revealed` | `target_acquisition.rs` | Area searches use `is_damageable` (no aim, S-2); Abduct uses `is_entity_targetable` |
| `begin_mode_channel`, `apply_mode_transition`, `UnitModeSpec::burrowed` | `entity.rs`, `world/abilities.rs` | Abduct's instant unburrow; a mode channel survives Abduct (D-8) |
| `SpatialHashGrid` | `steering.rs` | Area searches query cells instead of scanning every entity |
| `validate_displacement_destination_prevalidated` (Blink) | `world/abilities.rs` | Abduct's landing placement |
| Snapshot contracts `AreaView`, `LinkView`, `AbilitySource`, `StatusFlags`, `StatusKind` | `sim_types/src/presentation.rs` | Areas from storms and clouds, links from Abduct; new status kinds (D-11) |
| Matrix fixture `Battle { …, hazard }`, `Side { research, stim, mode }` | `sim_fixtures/src/matchup.rs` | Gains a scripted cast and an energy override (§10) |

### 2.2 Gaps found while planning

- **No approach.** A non-queued cast runs at once in phase 1 (`world/commands.rs` `dispatch_cast`) and a queued
  cast runs once when it reaches the queue front (`movement/locomotion.rs`, `MotionAction::Cast`); out of range,
  either is refused and the queued one is popped. StarCraft II walks the caster into range first (`CAbilEffect`
  `Approach` stage, `core` AbilData:56-79).
- **Range is centre to centre.** `admit_cast` compares the target point with `cast_range` from the caster's centre.
  The game measures from the caster's edge (`Flags RangeUseCasterRadius`, `core` AbilData:48), and for a unit
  target to the target's edge.
- **No cast backswing.** Nothing models `CastOutroTime`; the caster moves on the next tick.
- **Every selected caster casts.** `dispatch_cast` executes the cast for each commanded unit; the game sends a
  targeted spell to one unit of the group (`Flags BestUnit`, `core` AbilData:350, the `CAbilEffectTarget`
  default).
- **Hazard zones are not the game's storm.** `step_hazards` damages structures (B-23), stops double damage only
  within one tick (`last_storm_damage_tick`), pushes idle victims out to radius + 2 (B-78), allocates two `Vec`s
  per tick, and battle 13's hazard is radius 3, 20 every 10 ticks for 60 (B-24).
- **No caster has a spell.** `cerebral_elder` (High Templar), `tongue_chameleon` (Viper) and `mycelium_shaman`
  (Infestor) have `abilities` empty; only `railgun_crawler` and `burrow_scorpion` carry mode abilities (B-69).
- **No melee flag on weapons.** Blinding Cloud reduces only ranged weapons (`RangedWeaponRange`); `WeaponSpec` has
  no notion of melee (`visual_kind` is presentation and is not used for rules).
- **No AI opponent.** The skirmish has no computer player, so "AI use" here means the scripted casts of the matrix
  (§10); autocast and AI casting are out of scope (§16).

---

## 3. StarCraft II 5.0.14 reference

**Source.** SC2GameData at [`5444e12f13`](https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13) ("SC2
5.0.14.94342"), read from the local copy the audit fetched (`~/scratch/combat_wp3_data/<layer>/`). A citation
`swarm` EffectData:1545 gives layer, file and line. Liquipedia (wikitext read 2026-11-10: `Infestor (Legacy of the
Void)`, `Viper (Legacy of the Void)`, `High Templar (Legacy of the Void)`, `Psionic Storm`, `Fungal Growth`) is a
cross-check and the source of patch history only: its stat boxes show values after 5.0.15.

**Conversion** (audit §2.3; S-5): Faster s = Normal s ÷ 1.4; ticks = Faster s × 25 = **Normal s × 17.857143**;
per-tick speed = Normal value × 1.4 ÷ 25. Distances are world units, equal to game units. Whole-tick values below
round halves up; periodic times are kept exact and rounded per pulse (D-4).

### 3.1 Psionic Storm (High Templar → `cerebral_elder`)

| Fact | Game value | Source | Ours |
|---|---|---|---|
| Energy | 75 | `liberty` AbilData:2067, `swarm` AbilData:1110 | 75 |
| Cooldown | 2 Normal s (`swarm` overrides `liberty`'s 2.5) | `swarm` AbilData:1111 | 35.714 → **36** ticks |
| Range | 9, from the caster's edge | `liberty` AbilData:2061; `core` AbilData:48 | 9 + caster radius 0.375 |
| Cast backswing | `CastOutroTime` 0.5 Normal s | `liberty` AbilData:2062 | 8.93 → **9** ticks |
| Caster may be moving | `AllowMovement` | `liberty` AbilData:2065 | no stop needed before the cast (D-7) |
| Requirement | `UsePsiStorm` = research `PsiStormTech`, 110 Normal s, 200/200 | `liberty` AbilData:2070, :2183-2185 | `psionic_storm` 1964 ticks, 200/200 (already in `upgrades.ron`); gains `UnlockAbility` |
| Area | persistent: a search at once, then 6 more every 0.5712 Normal s | `liberty` EffectData:2071-2074 | searches at 0, 10.2, …, 61.2 ticks → ticks 0, 10, 20, 31, 41, 51, 61; area lasts 61.2 ticks |
| Radius | 1.5 | `liberty` EffectData:2080 | 1.5 |
| Who | everything but structures, missiles, destructibles, stasis, dead, hidden (in a transport), invulnerable: **own, allied and enemy; ground and air; cloaked and burrowed** (an area needs no detection) | `liberty` EffectData:2079 | filter `alliance: Any`, `planes: [Ground, Air]`, `structures: Exclude` |
| What it applies | buff `PsiStorm`: duration 0.6665 Normal s, period 0.571, initial effect 10 damage, periodic 10 damage | `liberty` BehaviorData:709-716, EffectData:2089-2101 | status `psionic_storm`: duration 11.902 ticks, period 10.196, initial 10, periodic 10 |
| Stacking | one buff per unit (`MaxStackCount` 1, `core` BehaviorData:43); a new search refreshes its duration and does not repeat the initial damage | `core` BehaviorData:43; the 80 total below | re-application refreshes; phase kept (D-5) |
| Total | a unit inside throughout: damage at 0, 0.571, …, 3.997 Normal s = 8 × 10 = **80** over 4 Normal s (2.86 Faster s). A unit that leaves takes one more tick (buff outlives the search by 0.0953 s), and a unit touched once takes 20 (initial + one periodic) | derived; Liquipedia `Psionic Storm`: "Units take one extra tick after leaving", "a unit can't receive only a single tick" | pulse ticks 0, 10, 20, 31, 41, 51, 61, 71 |
| Damage kind | spell (no `Kind` set; `core` EffectData:61-70 default): shields first, armour ignored | `liberty` EffectData:2089-2101 | `apply_spell_damage` (D-9) |
| Responses | the initial damage asks victims to `Acquire` (attack back) and `Flee` | `liberty` EffectData:2093-2094 | D-9: retaliation against the caster; helpless victims flee by the existing rule |
| Not later values | 5.0.15: radius 2, range 8, 110 over 6 s | Liquipedia `High Templar (LotV)` patch history; Blizzard 5.0.15 notes (effect plan §3) | not adopted |

### 3.2 Abduct (Viper → `tongue_chameleon`)

| Fact | Game value | Source | Ours |
|---|---|---|---|
| Energy | 75 | `swarm` AbilData:1230 | 75 |
| Cooldown | none in the chain | `swarm` AbilData:1222-1239 | 0 |
| Range | 9, edge to edge | `swarm` AbilData:1233; `core` AbilData:48 | 9 + both radii |
| Cast backswing | `CastOutroTime` 0.8 Normal s ("the Viper can't move or use abilities for 0.57 seconds", patch 5.0.11) | `voidmulti` AbilData:1754; Liquipedia `Viper (LotV)` | 14.29 → **14** ticks |
| Targets | any unit but self, neutral, structures, heroic, missiles, destructibles, stasis, dead, hidden, invulnerable: **own and enemy, ground and air, massive allowed**; needs vision (`RequireTargetVision`, `core` AbilData:50), so a cloaked or burrowed enemy only when detected | `voidmulti` AbilData:1755 | filter `alliance: AnyButSelf`, `planes: [Ground, Air]`, `structures: Exclude`; targetable rule of `is_entity_targetable` |
| Not interruptible | `UninterruptibleArray` Cast, Channel, Finish | `swarm` AbilData:1236-1238 | the caster's own orders cannot cancel the pull once cast |
| Destination | a placeholder of the target's type created at the **caster's position at cast time**, offset (0, 0.1), within `SpawnRange` 2, ignoring units (`PlacementIgnoreBlockers`) | `swarm` EffectData:1447-1471 (`YoinkStartCreatePlaceholder`; SpawnRange :1459, SpawnOffset :1460) | landing point fixed at the cast (D-8) |
| Tongue | missile `YoinkMissile`, guidance, `MaxSpeed` 18.75, acceleration 3200 (instant), from the caster to the target | `swarm` EffectData:1486-1494, MoverData:143-152 (:147) | 1.05 world units per tick, follows the target |
| On contact (`YoinkSet`) | apply `YoinkTentacle`; cancel the target's orders (32, uninterruptible, except for units that would be mid-morph: Siege Tank, Hellion, Thor, Warp Prism…); **instant unburrow**; apply `Yoink`; start `YoinkDelayPersistent` | `voidmulti` EffectData:1035-1045, `swarm` EffectData:1498-1538, ValidatorData:971-978 | §6.2 |
| `Yoink` buff | stun, suppress cloak, suppress collision, suppress boarding; 2 Normal s at most; removed at landing | `swarm` BehaviorData:1074-1088 (Stun :1078, SuppressCloak :1079, SuppressCollision :1080, SuppressPassenger :1084, Duration :1087) | status `abducted`: stun, reveal, no collision, no boarding, ≤ 35.71 ticks |
| `YoinkTentacle` | the target cannot attack (`CAbilAttack` disabled) | `voidmulti` BehaviorData:562-566 | part of the stun |
| Delay before the flight | persistent, period 0.1 Normal s, until the launch succeeds (≤ 50 tries) | `swarm` EffectData:1539-1547 (:1545) | 1.79 → **2** ticks after contact |
| Flight | `YoinkMover`: ballistic, `FlightTime` 1 Normal s, flying path (over cliffs), air targets keep air height, the Colossus glides | `swarm` MoverData:157-177 (:162, :167) | **18** ticks (17.86), straight in the plane; the arc is presentation |
| Mothership | not abductable from 5.0.14 (heroic) | `voidmulti` AbilData:1755; Liquipedia patch 5.0.14 | no catalog unit is heroic; nothing to model (§16) |
| Later changes | 5.0.15 to 5.0.17 sieged-tank rules | Liquipedia | not adopted: at 5.0.14 a sieged tank is abductable and stays sieged |

### 3.3 Fungal Growth (Infestor → `mycelium_shaman`)

| Fact | Game value | Source | Ours |
|---|---|---|---|
| Energy | 75 | `liberty` AbilData:486 | 75 |
| Cooldown | shared with `Leech`, no time set | `liberty` AbilData:487 | 0 |
| Range | 10 (5.0.13: "range increased from 9 to 10"), from the caster's edge | `void` AbilData:674; Liquipedia `Infestor (LotV)` | 10 + caster radius 0.625 |
| Missile | `FungalGrowthLaunchMissile` to the target point; mover `FungalGrowthWeapon` guidance, `MaxSpeed` 12, ignores terrain | `void` EffectData:1141-1146, MoverData:13-26 (:18) | 0.672 world units per tick |
| On impact | search radius **2.25** (`voidmulti`); the unburrow-for-spellcast step removed | `voidmulti` EffectData:1559, :693-695 | radius 2.25 |
| Who | **enemies only** (`Player`, `Ally` excluded), no structures, missiles, stasis, dead, hidden, invulnerable; ground and air; cloaked and burrowed | `liberty` EffectData:731 | filter `alliance: Enemy`, `planes: [Ground, Air]`, `structures: Exclude` |
| Damage buff `FungalGrowth` | 4 Normal s, period 0.25, periodic `FungalGrowthDamage` 1.5625 (armoured bonus 0); **suppresses cloak** | `swarm` BehaviorData:838-841; `voidmulti` EffectData:2388-2390; `void` EffectData:1138-1140; `liberty` BehaviorData:328-337 (:332) | status `fungal_growth`: 71.43 ticks, period 4.464, periodic 1.5625, reveal |
| Total | 16 pulses × 1.5625 = **25** (5.0.12: "damage reduced from 30 to 25") | derived; Liquipedia `Infestor (LotV)` | pulse ticks 4, 9, 13, 18, 22, 27, 31, 36, 40, 45, 49, 54, 58, 63, 67, 71 |
| Movement buff | **`FungalGrowthSlowMovement`: speed × 0.25, no boarding, 4 Normal s** (replaces the root `FungalGrowthMovement` in `FungalGrowthSet`) | `voidmulti` EffectData:1561-1572, BehaviorData:691-705 (:696-698); Liquipedia patch 4.0.0 "Slows the target's movement speed by 75%" | same status: speed factor 0.25, no boarding |
| Blink | affected units cannot Blink, Tactical Jump or board (patch 4.7.1) | Liquipedia `Infestor (LotV)` | no boarding; Blink refused while the status runs (D-8) |
| Responses | `FungalGrowthSearchDummy` asks for `Acquire` and `Flee` | `liberty` EffectData:736-741 | D-9 |
| Stacking | one buff per unit; a recast refreshes the duration | `core` BehaviorData:43; Liquipedia `Fungal Growth` notes | refresh, phase kept |

### 3.4 Blinding Cloud (Viper → `tongue_chameleon`)

| Fact | Game value | Source | Ours |
|---|---|---|---|
| Energy | 100 | `swarm` AbilData:1202 | 100 |
| Cooldown | none | `swarm` AbilData:1197-1209 | 0 |
| Range | 10, from the caster's edge | `swarm` AbilData:1204 | 10 + caster radius 1.0 |
| Missile | none in the 5.0.14 chain (the ability's effect is the persistent itself) | `swarm` AbilData:1207 | the area opens on the cast tick |
| Area | a search at once, then 15 more every 0.5 Normal s (`voidmulti` count 15) | `swarm` EffectData:236-242 (:241); `voidmulti` EffectData:157-159 | searches at k × 8.929 ticks, k = 0 … 15; area lasts 133.93 ticks |
| Radius | 2 | `swarm` EffectData:247 | 2 |
| Who | **ground only** (`Ground` required), every player's units but the caster, structures included | `swarm` EffectData:246 | filter `alliance: AnyButSelf`, `planes: [Ground]`, `structures: Include` |
| Units | buff `BlindingCloud`: 0.625 Normal s, `WeaponRange 0, RangedWeaponRange -20` (melee untouched) | `swarm` BehaviorData:994-1000; `voidmulti` BehaviorData:1067-1069 | status `blinding_cloud`: 11.16 ticks, ranged range −20 |
| Structures | buff `BlindingCloudStructure`: `WeaponRange -20`, `DetectArc 0`, `RadarArc 0`, 0.625 s | `swarm` BehaviorData:19-26, EffectData:249-258 | status `blinding_cloud_structure`: all weapons −20, no detection |
| Bunker passengers | the cloud reaches units inside a bunker | `swarm` EffectData:3-12 | no bunker analogue exists (§16) |
| Duration history | 3.8.0: "reduced from 10 seconds to 6 seconds" (Normal-time era wording) | Liquipedia `Viper (LotV)` | data value used |

### 3.5 Casters' energy and bodies

| Unit (analogue) | Energy start / max / regen | Radius | Source | Catalog today |
|---|---|---|---|---|
| High Templar (`cerebral_elder`) | 50 / 200 / 0.5625 Normal per s | 0.375 | `liberty` UnitData:3250-3252, :3267 | 50 / 200 / 0.0315, radius 0.375 — matches |
| Viper (`tongue_chameleon`) | 50 / 200 / 0.5625 | 1 | `swarm` UnitData:2334-2336, :2370 | 50 / 200 / 0.0315, radius 1.0 — matches |
| Infestor (`mycelium_shaman`) | **75** / 200 / 0.5625 | 0.625 | `liberty` UnitData:10057-10059; `voidmulti` UnitData:830-831 | 75 / 200 / 0.0315, radius 0.625 — matches |

The energy regeneration delay (B-35) belongs to plan `2026-11-05-sc2-deviation-fixes` WP15 and is not changed here.

### 3.6 Default ability flags that apply to all four

From the `CAbilEffect` default (`core` AbilData:34-88): `RangeSlop 1` (a cast that started stays valid while the
target drifts up to 1 beyond range), `RangeUseCasterRadius`, `RequireTargetVision`, `WaitToSpend` (costs are paid
when the effect runs, so a refused or failed cast costs nothing), refunds on Approach/Prep/Cast, and `BestUnit`
(`core` AbilData:350): a targeted command goes to one unit of the selection.

**Data anomaly noted.** `voidmulti` UnitData:827 sets the Infestor's ability slot 5 to `InfestorEnsnare`, which
only `balancemulti` defines (outside the chain). It does not touch Fungal Growth (slot 4). Recorded, not modelled.

---

## 4. Design decisions

### D-1. Generic primitives, no spell in Rust

`AbilityEffectSpec` gains four variants, each a mechanism StarCraft II's data composes; every spell is data:

- `AreaOverTime(AreaSpec)`: a persistent area at the target point that searches `searches` times, `period_ticks`
  apart, the first at once, and applies a status to each admitted unit.
- `AreaOnce(AreaSearchSpec)`: one search at the target point (Fungal's impact).
- `Missile { speed, impact: Box<AbilityEffectSpec> }`: carries `impact` from the caster to the target point at
  `speed` world units per tick, ignoring terrain; `impact` must be `AreaOnce` or `AreaOverTime` (catalog check).
- `PullToCaster(PullSpec)`: a tether to the target unit, then a pull to a landing point fixed at the cast.

`SpawnHazardZone` is removed, with `ActiveHazardZone` and `Entity::last_storm_damage_tick` (D-12). No `match` on an
ability or status name exists anywhere (C5); statuses are referenced by string key from RON and interned at load.

### D-2. The 5.0.14 data wins over the brief and over Liquipedia's current pages

Fungal Growth slows (×0.25) and does not root; Storm is radius 1.5 and 80 damage. Liquipedia is used for patch
history and for rules the data expresses only through validators (Fungal blocks Blink). This is charter §8.

### D-3. Statuses are catalog data in `assets/catalog/statuses.ron`

A status is `StatusSpec { name, duration_ticks, period_ticks, initial_damage, periodic_damage, damage_responses,
move_speed_factor, ranged_weapon_range_delta, weapon_range_delta, range_floor, stun, reveal, suppress_detection,
suppress_collision, suppress_boarding, suppress_blink, presentation }` (§5.1). It is loaded with the unit catalog
(`UnitCatalog` gains a `StatusTable`; `SimWorld::with_catalogs` keeps its signature), interned to a dense
`StatusId(u16)` in name order, and embedded by `sim_fixtures` like the other shipped catalogs
(`fixture_tests.rs` checks equality). An ability names its status by key; an unknown key is a typed
`CatalogError`. A separate file was chosen over statuses inline in `units.ron` because two abilities (or two
archetypes) that apply the same status must share it for the non-stacking rule, and a shared table states that
once. Read at run time (C6); `runtime_catalog_tests.rs` gains a status edit that changes behaviour without
recompiling.

### D-4. Exact periods, rounded per pulse (S-5)

`duration_ticks` and `period_ticks` of statuses and areas are `SimScalar` ticks authored to six decimals from the
Normal-time value × 17.857143 (the RON keeps the Normal value in a comment with its citation). Pulse *k* (counted
from the application, or from the area's creation) fires on the first tick whose index since the start is
`round_half_up(k × period)`, computed from the exact product each time (no accumulated error). A pulse at exactly
the end of the duration fires (16 × 0.25 = 4 Normal s gives Fungal's 16th pulse, the game's 25 total). Whole-tick
values (cooldown, outro, delay, flight) are rounded once, halves up.

### D-5. Non-stacking and refresh

An entity holds at most one instance of each `StatusId`. Applying a status it already has sets the remaining
duration back to `duration_ticks` from the current tick; it does not repeat the initial damage and keeps the
periodic phase (pulse times still count from the first application). This reproduces Storm's 80, "one extra tick
after leaving", "never a single tick" (§3.1), and Fungal's refresh. Different statuses stack (Fungal and Storm on
one unit both damage).

### D-6. `AbilitySpec` gains three fields (serde defaults keep old RON)

- `cast_backswing_ticks: u16` (`CastOutroTime`, default 0);
- `single_caster: bool` (`BestUnit`, default `true` for `TargetPosition`/`TargetEntity`, ignored by other kinds);
- `target_filter: Option<TargetFilter>` for `TargetEntity` abilities (Abduct).

### D-7. Cast pipeline (WP1, latent)

1. **Approach.** A `CastAbility` order whose target is out of range walks the caster along its route (the existing
   walking path, as a `Move` to the target point or, for a unit target, a pursuit) until the range check passes,
   then casts. Range for a point is `distance(caster centre, point) ≤ cast_range + caster radius`; for a unit,
   edge to edge (`+ target radius`). `RangeSlop` is not needed: our cast resolves on the tick the check passes.
2. **Admission vs. order.** `would_admit_cast` keeps its meaning (castable now, used by mode switches). A new
   `would_accept_cast_order` answers "could this unit start this order" (alive, not carried, unlocked, cooldown
   ready, energy, target valid and visible), without range, for the client and for submission revalidation.
3. **Payment when the effect runs** (`WaitToSpend`): energy and cooldown are taken on the cast tick, after
   approach; a refused cast costs nothing (the existing invariant).
4. **Backswing.** After the cast the caster is locked for `cast_backswing_ticks`: no movement, no attack, no
   other cast; the next order waits. A new order (not Shift) cancels nothing of the effect already launched
   (Abduct is uninterruptible) and starts after the lock. The lock is a new hashed `Entity::cast_lock_ticks`.
5. **Moving casters.** No prior stop is needed (`AllowMovement` on Storm, Abduct and Blinding Cloud). Fungal lacks
   the flag in the data but has no cast time, so the difference is nil (**verify** at WP4 that no
   `CastIntroTime` exists in the chain for it; the planning read found none).
6. **One caster.** For a non-queued `single_caster` command the simulation picks one recipient: the commanded unit
   that would accept the order and is nearest the target (edge distance), ties broken by more energy, then the
   lowest `EntityId`; the others keep their orders. Queued commands go to every unit, as in the game where Shift
   distributes per unit (**verify**, SU-1). The pick is in the simulation, not the client, so a replay of the same
   `PlayerCommand` reproduces it.
7. **Vision for unit targets.** A `TargetEntity` cast requires `is_entity_targetable` for the caster's owner at
   admission (cloaked and burrowed enemies only under detection); an own unit is always visible.

### D-8. Abduct is a pull state on the target

- **At cast:** compute the landing point: the caster's position plus 0.1 along its heading, then the nearest point
  within 2 world units that is valid for the target's movement layer (terrain and structures only; units ignored,
  `PlacementIgnoreBlockers`), searched by `validate_displacement_destination_prevalidated` over a fixed ring
  pattern (centre, then 8 directions at 0.5, 1.0, 1.5, 2.0; first valid in that order). If none is valid the cast
  is refused without payment (SU-4). Then launch the tongue.
- **Tongue:** a `SpellMissile` that follows the target at 1.05 per tick; it reaches the target when within its
  speed of the target's edge. If the target dies, loads into a transport or becomes untargetable (cloaks with no
  detection) the tongue is dropped and nothing else happens (energy stays spent, as the game's missile is already
  launched).
- **Contact:** clear the target's order queue, hold position, current target and route; a weapon swing in
  progress is cancelled; any **mode channel in progress continues** and a settled mode stays (a sieged tank lands
  sieged, the `YoinkCancelOrder` validator's exemption for morphing units); a unit in a burrowed mode leaves it at
  once with no channel; apply status `abducted`; set `Entity::pull = Some(PullState { from, to, start_tick, delay
  2, flight 18 })`.
- **Flight:** from contact + 2 ticks, the position moves linearly from `from` to `to` over 18 ticks; the unit has
  no collision, is not pushed, does not push, and ignores terrain while in flight (flying path); vision follows the
  position. On landing the pull ends and `abducted` is removed (game: `YoinkImpactSet`); normal separation then
  resolves overlaps.
- **The caster's death** after contact does not stop the pull (the persistent sits on the target).
- **Carriers:** an abducted transport takes its passengers with it (passengers follow the carrier's position by
  the transport rules).
- **Link:** `RenderSnapshot::links` shows `{ from: caster, to: target }` from launch until landing while the caster
  lives.

### D-9. Spell damage and responses

`apply_spell_damage(entity, amount, source_owner, source_entity)`: barrier absorbs first (B-41), then shields,
then life; armour does not apply (spell kind). A status's `damage_responses: (acquire, flee)` mirror the game's
`ResponseFlags` on the initial damage: `acquire` makes the victim retaliate against the source caster if it
exists and can be targeted (`apply_combat_retaliation`), `flee` lets the existing helpless-flee branch move an
idle victim that cannot retaliate away from the source **position** (the area centre). This replaces the hazard's
"radius + 2" evacuation (B-78); the flee distance stays the unsourced B-13 value. Responses apply only on the
initial damage, as in the data (the periodic `PsiStormDamage` has none). Kill credit and the damage-taken timers
(`ticks_since_last_damage`) behave as for weapon damage.

### D-10. Client: generic targeted casts (positional keys, Q0072 default)

Card entries come from every `TargetPosition`/`TargetEntity` ability of the selection (`CommandCatalog`, by
capability, never by name), after the mode entries. Pressing one arms `CommandMode::CastAtPoint { ability }` or
`CastOnUnit { ability }`; a left click submits `CastAbility`, Shift queues, right click or the cancel key leaves.
The cursor shows the area's radius from the spec (`AreaSpec`/`AreaSearchSpec` radius) as the targeting ring.
Locks: `NeedsResearch` (Storm), new `NeedsEnergy`, new `Cooldown`, `Transforming`. Submission and revalidation call
`would_accept_cast_order` (D-7.2). `PlayerView` gains per-unit energy and cooldowns where it lacks them (**verify**
what the HUD already reads).

### D-11. Presentation: contracts first, art behind Q0080

- **Contracts (checksum-neutral, WP2 to WP5):** `areas` filled from `active_areas` (Storm, Blinding Cloud) with
  `total_ticks` = the area's lifetime and `source` = the casting ability; `links` from Abduct (D-8); a new
  `RenderSnapshot::spell_missiles: Vec<SpellMissileView { id, owner, position: [f32; 3], source: AbilitySource }>`
  for Fungal's missile and Abduct's tongue (refilled in place, fixed-size records, like `areas`); `StatusKind`
  gains `DamageOverTime`, `Stunned`, `RangeReduced`, `Revealed` (Fungal also sets the existing `Slowed`), each
  status's `presentation` field naming which flag it raises. Concealment reads the reveal: a revealed cloaked or
  burrowed unit is presented as seen.
- **Art:** each spell WP adds its effect to `assets/catalog/effects.ron` and names it in the caster's presentation
  block (`abilities: { "Psionic Storm": … }`), built from the framework's blocks and sent through the effect plan's
  §7 review gate. Q0080 decides how that gate runs now. Default until answered: build the effect, run the gate;
  an effect that fails ships unused exactly as `acid_burst` did, and the spell still shows the HUD's **targeting
  and area ring** (an indicator owned by `battle_hud`, like the order marker and the selection ring, drawn at the
  simulated radius for both players when in sight) and a plain tether line for `links`. These indicators are UI,
  not effect art, and are what StarCraft II also shows (its storm and cloud have ground outlines); they are not a
  stand-in for the reviewed effect.

### D-12. Hazard zones are replaced, not extended

`ActiveHazardZone` predates the data model and differs from the game in five ways (§2.2). It stays untouched
through WP1, so battle 13 keeps running on it, and the Storm data commit (WP2b) deletes it, its checksum fields and `place_hazard`; battle 13 becomes a cast; the tests that place hazards
directly (`spells_and_abilities_tests.rs` `test_psionic_storm_hazard_zone_aoe_and_non_stacking`,
`flee_and_evasion_tests.rs` `test_idle_unit_flees_psionic_storm_hazard_zone`, `effect_snapshot_tests.rs`,
`checksum_state_tests.rs`, `movement_layers_tests.rs`, `barrier_tests.rs`, `world_api_surface_tests.rs`,
`transport_ownership_tests.rs`, `production_transactions_tests.rs`) are rewritten against the new area through a
test-only scratch archetype or the shipped caster; none is deleted without a replacement asserting the same
property under the game's rules.

### D-13. Two checksum schema changes, both latent

1. **WP1b** adds all new hashed state at empty defaults and bumps `CHECKSUM_SCHEMA_VERSION` (25 → the next free
   number at the time; 26 if plan `2026-11-10-group-routing` has not taken it, else 27): `Entity::statuses:
   Vec<ActiveStatus>` (sorted by `StatusId`), `Entity::cast_lock_ticks`, `Entity::pull: Option<PullState>`,
   `SimWorld::active_areas: Vec<ActiveArea>`, `SimWorld::spell_missiles: Vec<SpellMissile>`, `next_area_id`,
   `next_missile_id`, and `WeaponSpec::melee` (hashed via `siege_weapon` and `ActiveProjectile::weapon_spec`).
2. **WP2b** removes `active_hazards`, `ActiveHazardZone`'s encoding and `Entity::last_storm_damage_tick`, and bumps
   once more.

Each time the goldens are re-recorded with the schema as the only cause, and every render snapshot of both golden
replays is shown byte-identical to the previous schema (as the flipbook plan WP2 did). Two bumps were chosen over
one because the hazard zone must keep battle 13 running until a real storm exists to replace it (D-12); folding
both into WP2b would put the new state and a behaviour change in one commit. Apart from these, packages change no
schema; they change behaviour only where a caster is present, which no golden contains (`golden_replay.ron` and
`golden_replay_orders.ron` hold no caster, **verify** at WP1b).

### D-14. `WeaponSpec::melee` from the game's weapon options

Blinding Cloud needs "ranged" (`RangedWeaponRange`). `WeaponSpec` gains `melee: bool` (serde default `false`),
set `true` in WP5's data commit for every catalog weapon whose game counterpart carries `Options index="Melee"` in
`WeaponData` (each with its citation in a RON comment; the audit scripts in `~/scratch/combat_wp3_scripts/`
extract them). No rule other than Blinding Cloud reads it.

---

## 5. Data model

### 5.1 `assets/catalog/statuses.ron`

```ron
// Timed per-unit effects applied by abilities. Times are 25 Hz ticks (Normal s × 17.857143), exact; pulses round
// per pulse (plan 2026-11-10-spellcaster-mechanics D-4). Every value cites SC2GameData 5.0.14 (5444e12f13).
(
    statuses: [
        (
            name: "psionic_storm",
            // PsiStorm buff: liberty BehaviorData:713-716 (0.6665 / 0.571 Normal s)
            duration_ticks: 11.901786,
            period_ticks: 10.196429,
            initial_damage: 10.0,            // liberty EffectData:2091
            periodic_damage: 10.0,           // liberty EffectData:2100
            damage_responses: (acquire: true, flee: true),   // liberty EffectData:2093-2094
            presentation: [DamageOverTime],
        ),
        (
            name: "abducted",
            duration_ticks: 35.714286,       // Yoink buff, swarm BehaviorData:1087 (removed at landing)
            stun: true, reveal: true, suppress_collision: true, suppress_boarding: true,
            presentation: [Stunned],
        ),
        (
            name: "fungal_growth",
            duration_ticks: 71.428571,       // swarm BehaviorData:839 (4 Normal s)
            period_ticks: 4.464286,          // swarm BehaviorData:840 (0.25)
            periodic_damage: 1.5625,         // voidmulti EffectData:2389
            damage_responses: (acquire: true, flee: true),   // liberty EffectData:738-739, on the search
            move_speed_factor: 0.25,         // voidmulti BehaviorData:697
            reveal: true,                    // liberty BehaviorData:332
            suppress_boarding: true, suppress_blink: true,
            presentation: [DamageOverTime, Slowed, Revealed],
        ),
        (
            name: "blinding_cloud",
            duration_ticks: 11.160714,       // swarm BehaviorData:997 (0.625)
            ranged_weapon_range_delta: -20.0,  // voidmulti BehaviorData:1068
            presentation: [RangeReduced],
        ),
        (
            name: "blinding_cloud_structure",
            duration_ticks: 11.160714,       // swarm BehaviorData:24
            weapon_range_delta: -20.0, suppress_detection: true,   // swarm BehaviorData:25
            presentation: [RangeReduced],
        ),
    ],
)
```

Every field not shown takes its neutral default (`period_ticks: None`, damages 0, factor 1, deltas 0, flags
false). `range_floor` (default 0.1, SU-2) bounds a reduced range from below; a weapon already at or below the floor
is unchanged. Validation (typed `CatalogError`): unique names; positive duration; period present iff periodic
damage is non-zero; factor in `0..=1`; non-negative damages.

### 5.2 Abilities in `units.ron`

```ron
// cerebral_elder (High Templar analogue)
abilities: [
    (
        name: "Psionic Storm",
        target_kind: TargetPosition,
        energy_cost: 75.0,              // liberty AbilData:2067
        cooldown_ticks: 36,             // swarm AbilData:1111 (2 Normal s)
        cast_range: 9.0,                // liberty AbilData:2061
        cast_backswing_ticks: 9,        // liberty AbilData:2062 (0.5)
        effect: AreaOverTime((
            radius: 1.5,                // liberty EffectData:2080
            searches: 7,                // initial + PeriodCount 6, liberty EffectData:2071-2072
            period_ticks: 10.2,         // liberty EffectData:2074 (0.5712)
            filter: (alliance: Any, planes: [Ground, Air], structures: Exclude),
            status: "psionic_storm",
            structure_status: None,
        )),
    ),
],
```

```ron
// tongue_chameleon (Viper analogue)
abilities: [
    (
        name: "Abduct",
        target_kind: TargetEntity,
        energy_cost: 75.0, cooldown_ticks: 0, cast_range: 9.0,
        cast_backswing_ticks: 14,       // voidmulti AbilData:1754 (0.8)
        target_filter: Some((alliance: AnyButSelf, planes: [Ground, Air], structures: Exclude)),
        effect: PullToCaster((
            tongue_speed: 1.05,         // swarm MoverData:147 (18.75 Normal)
            landing_offset: 0.1,        // swarm EffectData:1460
            landing_search_range: 2.0,  // swarm EffectData:1459
            delay_ticks: 2,             // swarm EffectData:1545 (0.1)
            flight_ticks: 18,           // swarm MoverData:167 (1 Normal s)
            status: "abducted",
        )),
    ),
    (
        name: "Blinding Cloud",
        target_kind: TargetPosition,
        energy_cost: 100.0, cooldown_ticks: 0, cast_range: 10.0,
        effect: AreaOverTime((
            radius: 2.0, searches: 16, period_ticks: 8.928571,
            filter: (alliance: AnyButSelf, planes: [Ground], structures: Include),
            status: "blinding_cloud",
            structure_status: Some("blinding_cloud_structure"),
        )),
    ),
],
```

```ron
// mycelium_shaman (Infestor analogue)
abilities: [
    (
        name: "Fungal Growth",
        target_kind: TargetPosition,
        energy_cost: 75.0, cooldown_ticks: 0, cast_range: 10.0,
        effect: Missile(
            speed: 0.672,               // void MoverData:18 (12 Normal)
            impact: AreaOnce((
                radius: 2.25,           // voidmulti EffectData:1559
                filter: (alliance: Enemy, planes: [Ground, Air], structures: Exclude),
                status: "fungal_growth",
                structure_status: None,
            )),
        ),
    ),
],
```

`upgrades.ron` `psionic_storm` gains `effects: [UnlockAbility(target_slug: "cerebral_elder", ability_slug:
"Psionic Storm")]`. Ability names are display data the unlock rule already keys on; no Rust reads them.

### 5.3 State (hashed, D-13)

| Type | Fields |
|---|---|
| `ActiveStatus` | `status: StatusId`, `applied_tick: Tick`, `expires_at: SimScalar` (ticks since `applied_tick`), `pulses_done: u16`, `source_owner: PlayerId`, `source_entity: Option<EntityId>`, `source_pos: SimVec2` |
| `ActiveArea` | `id: u32`, `owner: PlayerId`, `caster: Option<EntityId>`, `source: AbilitySource`, `center: SimVec2`, `created_tick: Tick`, `searches_done: u16` (spec read from the catalog through `source`) |
| `SpellMissile` | `id: u32`, `owner`, `caster: Option<EntityId>`, `source: AbilitySource`, `position: SimVec2`, `target: SpellTarget::{Point(SimVec2), Entity(EntityId)}`, `landing: Option<SimVec2>` (Abduct) |
| `PullState` | `caster: Option<EntityId>`, `source: AbilitySource`, `from: SimVec2`, `to: SimVec2`, `contact_tick: Tick` |
| `Entity::cast_lock_ticks` | `u16` |

Specs are not hashed (as `UnitModeSpec` is not); state points at them by `AbilitySource` and `StatusId`, both
stable for a catalog.

---

## 6. Simulation rules per spell

Tick placement (`world.rs` phases): casts resolve in phase 1 (non-queued, in range) or in movement (queued or after
approach), as today. **Phase 2** gains, in this order, replacing `step_hazards`: (a) spell missiles advance and
impact, in `id` order; (b) areas search, in `id` order, applying statuses; (c) statuses pulse, expire and apply
damage, entities in `EntityId` order, statuses in `StatusId` order; (d) pulls advance. Damage from (c) feeds the
same death reaping (phase 8). All searches use the spatial hash grid with candidates sorted by `EntityId`.

### 6.1 Psionic Storm

1. Cast: point in range (D-7), energy ≥ 75, cooldown 0, `psionic_storm` researched. Pay 75, cooldown 36, lock 9.
2. An `ActiveArea` opens at the point on the cast tick. Search *k* (k = 0 … 6) runs on `created + round(k × 10.2)`.
3. Each search applies `psionic_storm` to every unit within 1.5 of the centre (centre distance, as
   `CEffectEnumArea` measures to the unit's position; **verify** whether the game adds the unit's radius; the
   planning read found no `IncludeRadius` flag on `PsiStormSearch`) that is damageable, not a structure, any owner.
4. First application: 10 damage at once, responses on. Pulses at `applied + round(k × 10.196)`, 10 each, while the
   status lasts; refresh per D-5. Expiry: `applied + expires_at` passes without refresh.
5. The area closes after its last search (tick 61); `areas` shows it from creation to close.

### 6.2 Abduct

1. Cast: target unit admitted by `target_filter`, targetable for the caster's owner, edge distance ≤ 9 (after
   approach), energy ≥ 75, landing point exists (D-8). Pay 75, lock 14.
2. Tongue flies at 1.05 per tick toward the target; contact → §D-8 contact rules; delay 2; flight 18; landing.
3. While `abducted`: no movement of its own, no attack, no cast, no boarding, not pushed, no collision; orders
   given meanwhile are queued and start on landing (SU-3); visible to every player with vision (reveal).
4. A second Abduct on a unit already in a pull is refused at admission (game: validator `NoYoink`,
   `swarm` EffectData:1468).

### 6.3 Fungal Growth

1. Cast: point in range, energy ≥ 75. Pay 75; no cooldown, no lock.
2. A spell missile flies from the caster to the point at 0.672 per tick ignoring terrain; impact on the tick it
   reaches the point (distance ≤ speed).
3. Impact: one search, radius 2.25, enemies of the caster's owner only, no structures; apply `fungal_growth`.
   Responses fire on the application (the game's search dummy).
4. The status: 1.5625 every 4.464 ticks (16 pulses, 25 total), movement ×0.25, revealed (cloaked and burrowed
   units are targetable by anyone with vision of their cell), no boarding, no Blink; 71.43 ticks; refresh per D-5.

### 6.4 Blinding Cloud

1. Cast: point in range, energy ≥ 100. Pay 100.
2. An area opens on the cast tick; 16 searches at `round(k × 8.929)`, k = 0 … 15; radius 2; ground units and
   structures of every owner but the caster itself.
3. Units get `blinding_cloud`: every weapon with `melee: false` has effective range `max(range − 20, floor)` while
   it lasts (floor 0.1, SU-2; a weapon at or below the floor is untouched). Range modifiers combine as range +
   upgrades + status delta, then the floor. A sieged weapon whose `min_range` exceeds its reduced range cannot fire.
   Acquisition uses the reduced range, so a ranged unit in the cloud walks to contact or holds.
4. Structures get `blinding_cloud_structure`: all weapons reduced the same way, and the structure does not detect
   while it lasts (vision recomputes detection sources each tick from `is_detector && !suppressed`).
5. Air units are never affected; a ground unit outside the cloud is never affected, whatever it shoots at.

---

## 7. Interactions

| Case | Storm | Abduct | Fungal | Blinding Cloud |
|---|---|---|---|---|
| Hold Position | hit; holding units do not flee (existing rule) | cancelled with the orders | hit; slowed | hit; range reduced |
| Own / allied units | hit | can be targeted | never | hit (not the caster) |
| Air | hit | pulled, stays air | hit | never |
| Massive | hit | allowed | hit | ground ones hit |
| Structures | never | never | never | own status: range and detection |
| Cloaked, undetected | hit (area, S-2) | cannot be targeted | hit and revealed | hit |
| Burrowed, undetected | hit | cannot be targeted; detected → unburrowed at contact | hit and revealed | hit |
| In a transport | never (hidden) | never; a carrier is pulled with its passengers | never; cannot board while affected | never |
| Channels | the caster's own orders after the cast are not delayed beyond the 9-tick lock | target's orders and weapon swing cancelled; a mode channel continues | none | none |
| Mode units | sieged/burrowed take damage normally | sieged tank lands sieged; burrowed leaves burrow | cannot change their speed by mode (factor applies to every mode's speed) | sieged min range may forbid firing |
| Invulnerable | never | never | never | never |
| Barrier (Immortal analogue) | absorbs, then shields | — | absorbs | — |
| Caster dies | area keeps running | pull keeps running after contact; tongue in flight is dropped | missile keeps flying | area keeps running |

---

## 8. Client

- **Card** (`battle_hud/card.rs`, `catalog.rs`): `CardLabel::Cast { ability }` and `CardAction::Cast { ability }`
  (both `Copy`), derived from targeted abilities of the selection by capability; label text from the ability's
  `name`; locks per D-10. `derive_card` stays allocation-free once warm.
- **Targeting** (`battle_input/command_mode.rs`): `CastAtPoint` and `CastOnUnit` modes; a unit click resolves
  through the existing picking with fog (only what the viewer sees and, for cloaked or burrowed enemies, detects);
  the targeting ring uses the ability's area radius (point casts) or none (unit casts). A click outside range is
  accepted (the unit approaches, D-7); a click on an invalid unit is refused through the feed.
- **Submission** (`battle_pipeline/commands.rs`): `supported` and `capable` admit `CastAbility` for the casts the
  card offers, through `would_accept_cast_order`; revalidation at `prepare_tick` uses the same call.
- **Presentation** (`client::effects`, `battle_presentation`): areas, links, spell missiles and the new status
  flags per D-11; fog per the effect plan (an area or missile is drawn where the viewer sees its centre or
  position).
- **Tests** (new `crates/client/tests/battle_cast_tests.rs`, headless, no GPU): one per spell, through input
  alone: select the caster, press the card key, click; assert the `PlayerCommand`, the simulation's effect after
  N ticks (Storm kills held zerglings; Abduct lands the target within 2 of the caster's cast position; Fungal slows;
  the cloud's victims' effective range), the locks (no research, no energy, cooldown), Shift queueing, and that a
  multi-caster selection sends one cast. Classed in `crates/gate/suites.ron` (C22). GPU picture tests for the area
  ring and tether join `effect_gpu_tests.rs` only if WP2c passes review (D-11).

---

## 9. AI and scenarios

There is no computer opponent, so no AI casts spells. The matrix gets scripted casts (§10). The skirmish scenario
(`assets/scenarios/skirmish.ron`) is unchanged: the casters are trainable already where their factions' rosters
allow it (**verify** at WP2c that `cerebral_elder`, `tongue_chameleon` and `mycelium_shaman` are producible in the
skirmish tech tree; if not, that is a scenario data item for the WP, not a code change). Autocast and AI spell use
are out of scope (§16).

---

## 10. Matrix battles

### 10.1 Fixture extensions (`sim_fixtures/src/matchup.rs`)

- `Squad::energy: Option<f32>`: overrides start energy (a test fixture setting, like `research`), so a caster can
  cast on tick 1 without a 25-second wait.
- `Side::casts: Vec<ScriptedCast { tick: u32, ability: String, at: CastTarget }>` with `CastTarget::{Point(f32,
  f32), EnemyCentroid, FirstEnemy, FirstEnemyOfType(String)}`. On `tick`, the side's first living unit whose
  archetype has an ability of that `name` receives the `CastAbility` (queued behind nothing, `queue: false`), after
  the standing order. Resolved by name in the fixture only (C5 governs production Rust; fixtures name units today).
- `Battle::hazard` and `HazardFixture` are removed (D-12).

### 10.2 Battles

Expectations are measured on the matrix's seeds when each WP lands and recorded with the claim; the claim is the
contract, the counts are the measurement. Each spell battle has a control (same armies, no cast) that must end
the other way, which is what makes the claim checkable.

| Battle | Sides | Claim | Package |
|---|---|---|---|
| 13 `13_storm_vs_held_lings` (rewritten) | blue: 1 `cerebral_elder` (energy 75, `psionic_storm` researched) casting at the red centroid on tick 1; red: 8 `razor_mantis` holding, as today | One storm (80 over 71 ticks) kills every zergling inside 1.5 (35 life); those outside take nothing. Expected red (0, 0), blue (1, 1) | WP2 |
| 36a `36a_storm_turns_marines` / 36b control | blue: 1 `cerebral_elder` + 6 `psi_grey`; red: 16 `pulse_bot` attack-moving into them | With a storm on the marine ball the zealots win; without it the marines win | WP2 |
| 37a `37a_abduct_sieged_tank` / 37b control | blue: 1 `tongue_chameleon` + 8 `razor_mantis`; red: 2 sieged `railgun_crawler` holding behind 4 `pulse_bot` | Pulling one sieged tank into the zerglings wins the fight the control loses | WP3 |
| 38a `38a_fungal_marines` / 38b control | blue: 1 `mycelium_shaman` + 16 `razor_mantis`; red: 10 `pulse_bot` | 25 damage and the slow on a clump flip a fight the zerglings lose | WP4 |
| 39a `39a_cloud_on_tanks` / 39b control | blue: 1 `tongue_chameleon` + 8 `rhino_beetle` attack-moving; red: 3 sieged `railgun_crawler` | Under the cloud the tanks fire at melee range only (min range 2 forbids it) and the roaches win; the control is the tanks' | WP5 |

The army sizes above are starting points; the WP's measurement step tunes them only to make the control and the
spell battle land on opposite sides with margin on every seed, and records the sizes and why. Every other battle,
**battle 33 included, must be bit-identical** to `main` in outcome and in its trace (`sim_fixtures/src/trace`).

---

## 11. Tests first, per spell

All in `crates/sim_core/tests/`, written failing before the code, scratch archetypes in RON where a shipped unit
would make the test depend on balance. New files are classed in `crates/gate/suites.ron` (C22).

**WP1 (pipeline), `spellcaster_pipeline_tests.rs`:** approach then cast (point and unit), the cast tick equals the
first tick in edge range; no payment before the cast; backswing locks movement, attack and casts for exactly N
ticks; `single_caster` picks the nearest, then energy, then lowest id; queued casts go to all; a cloaked enemy
untargetable without detection, targetable with; `would_accept_cast_order` agrees with the tick on every refusal
(the existing `would_admit_cast` agreement test pattern); a status table unit test of D-4 (pulse ticks of
period 10.196 and 4.464 equal the lists in §3) and D-5 (refresh keeps phase, no repeated initial damage).

**WP2 (Storm), `psionic_storm_tests.rs`:** a unit inside throughout takes exactly 80 at ticks 0, 10, 20, 31, 41, 51,
61, 71; two overlapping storms still 80; a unit leaving after the first search takes 20; own units hit; structures
untouched; air hit; burrowed and cloaked hit without detection; loaded units untouched; shields before life and no
armour; a barrier absorbs; held units do not move; an idle unit that cannot retaliate flees from the centre; an
armed idle unit acquires the caster; refused without research, with < 75 energy, on cooldown (36 ticks); the area
appears in `areas` from creation to tick 61 with its source. Rewrites of the hazard tests (D-12).

**WP3 (Abduct), `abduct_tests.rs`:** tongue contact tick = launch + ceil(gap / 1.05); landing 2 + 18 ticks after
contact at the fixed point even if the Viper moves or dies; ground unit pulled across a cliff lands on the
caster's level; air stays air; a massive unit is pulled; a sieged tank lands sieged and a mode channel in progress
completes; a burrowed unit (detected) is unburrowed at contact; orders, hold, swing cancelled; no attack, cast,
boarding during the pull; refused with no landing point (Viper over unpathable ground, SU-4) and with no
payment; refused on an undetected cloaked enemy, on a structure, on itself; a tongue at a target that loads into a
transport is dropped; `links` has the tether from launch to landing.

**WP4 (Fungal), `fungal_growth_tests.rs`:** missile impact tick = cast + ceil(distance / 0.672); 25 in 16 pulses at
the §3.3 ticks; enemies only (own units and allies in the radius untouched); speed ×0.25 measured over a straight
walk; Blink refused and boarding refused while affected; a cloaked enemy becomes targetable without detection for
71 ticks; a recast refreshes without repeating damage; armour ignored.

**WP5 (Blinding Cloud), `blinding_cloud_tests.rs`:** a marine-analogue (range 5) in the cloud cannot hit a target at
1 but hits one in contact; a melee unit is unaffected; an air unit over the cloud is unaffected; a ground unit
outside shooting into the cloud is unaffected; the status lasts 11 ticks after the unit leaves; a sieged tank
inside cannot fire; a detector structure inside stops revealing a cloaked unit and resumes after; 16 searches at
the §3.4 ticks; own units affected; the caster's own body never.

Each package also extends `runtime_catalog_tests.rs` (a status or area value edited in a scratch catalog changes
the result without recompiling) and `catalog_reference_values_tests.rs` (every value of §3 as authored).

---

## 12. Work packages (commit order)

Each package: tests first, then code, then data in its own commit, then measurement. Implementation agents hand
back after `cargo gate quick` and the tests their change needs; a verifier runs `cargo gate package` and reports
the stamp or the failing targets (Q0061). Every data commit runs the matrix, battle 33 and both goldens in debug,
release and `wasm32-wasip1`, and records the result with traced causes in
`docs/evidence/2026-11-10-spellcaster-mechanics.md`.

| WP | Content | Expected measurement | Size |
|---|---|---|---|
| WP0 | This plan and its index row | — | — |
| WP1a | `sim_types`: `StatusSpec`, `AreaSpec`, `AreaSearchSpec`, `TargetFilter`, `PullSpec`, the four `AbilityEffectSpec` variants, `AbilitySpec` fields (D-6), `WeaponSpec::melee`, `StatusKind` variants, `SpellMissileView`; serde defaults; `sim_core` catalog loading of `statuses.ron` (empty file shipped), validation, `sim_fixtures` embedding | bit-identical | M |
| WP1b | Hashed state (D-13 item 1): `statuses`, `cast_lock_ticks`, `pull`, `active_areas`, `spell_missiles`, ids, `WeaponSpec::melee`, all empty or false; hazard zones untouched; schema bump | goldens re-recorded with the schema as only cause, render snapshots byte-identical; matrix bit-identical | S |
| WP1c | Cast pipeline (D-7): approach, edge range, payment at cast, backswing lock, single caster, target vision, `would_accept_cast_order`; status engine (D-4, D-5), areas, missiles, pulls, spell damage and responses (D-9) — all exercised by scratch archetypes only | bit-identical (no shipped ability uses them; mode toggles unchanged, `would_admit_cast` agreement test green) | L |
| WP1d | Client generic cast: card entries, `CommandMode` cast modes, locks, submission, targeting ring and tether indicators (D-11), `battle_cast_tests.rs` on a scratch caster | client tests; no simulation change | M |
| WP2a | Storm tests (§11) against the shipped catalog values | failing | S |
| WP2b | Storm data: `cerebral_elder` ability, `psionic_storm` status, upgrade unlock; battle 13 to the real High Templar analogue and the 5.0.14 storm (B-24); battles 36a/36b. Separate commit before it: hazard zones removed (D-12, D-13 item 2), their tests rewritten, `Battle::hazard` dropped, schema bump | hazard commit: goldens re-recorded for the schema only, render snapshots byte-identical. Data commit: 13 changes (recorded cause: real storm values), 36a/b new; every other battle and battle 33 bit-identical; goldens unchanged | M |
| WP2c | Storm presentation: effect entry and review (D-11, Q0080); input test with the shipped caster | review verdict recorded | M |
| WP3a/b/c | Abduct: tests, data (+ battles 37a/b), presentation (tether effect) | 37a/b new; rest bit-identical | M |
| WP4a/b/c | Fungal Growth: tests, data (+ 38a/b), presentation (missile, status) | 38a/b new; rest bit-identical | M |
| WP5a | Melee audit: `melee: true` on every catalog weapon whose game weapon has `Options Melee`, cited | bit-identical (no rule reads it yet; hash change only where a melee spec is hashed, **verify** none) | S |
| WP5b/c/d | Blinding Cloud: tests, data (+ 39a/b), presentation | 39a/b new; rest bit-identical | M |
| WP6 | Throughput: `cargo bench -p sim_core --bench throughput` with a new scenario of 20 live areas and 100 statuses over 400 units must stay ≥ 25 ticks/s (charter §3.1) and within 5 % of `main` on the existing scenarios; `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` windows with a live area, tether, missile and statuses | recorded in `DESIGN.md` §5 | S |
| WP7 | Bookkeeping: `DESIGN.md` (Abilities row; register: remove B-23, B-24, B-78, the four spells from B-69 and Psionic Storm from B-68; add SU items still open as B-IDs); plan `2026-11-05-sc2-deviation-fixes` WP18 note; this row to `Done` or `Partly done`; Q0077 moved to `closed/` with its resolution | — | S |

The order inside the owner's order is fixed by dependency: WP1 serves all four; WP2 to WP5 follow Storm →
Abduct → Fungal → Blinding Cloud. A package's `c` (presentation) step may slip behind the next spell's `a` and
`b` if the review gate is waiting on Q0080; mechanics never wait on presentation.

---

## 13. Determinism and checksum summary

- New state is fixed-point (`SimScalar`, `SimVec2`, `Tick`), in sorted containers (`Vec` sorted by id), iterated in
  id order; no float enters (catalog floats convert through `SimScalar::from_f32_lossy` at load as today, C7).
- No new random draw. Pulse times are pure functions of the exact period (D-4).
- Spatial-grid candidate lists are sorted by `EntityId` before applying anything.
- Two schema bumps (D-13), both latent. Goldens change only for them, with render snapshots byte-identical.
- The four spells change behaviour only where their casters are; no golden and no existing matrix battle besides
  13 contains one.
- Verified on Linux debug and release and `wasm32-wasip1` by the gate's golden replays; CI covers Windows and
  macOS ARM64.

---

## 14. Undetermined rules (defaults applied, research continues)

| ID | Question | Default | Where it would be settled |
|---|---|---|---|
| SU-1 | How `BestUnit` picks the caster, and whether Shift-queued spells go to one unit or all | nearest (edge), more energy, lowest id; queued to all | in-game trace or engine documentation; WP1c note |
| SU-2 | The floor of a weapon range reduced below 0 by `RangedWeaponRange -20` | 0.1 (the game's melee range, e.g. the Zealot's) | WP5a research |
| SU-3 | What happens to orders given to a stunned unit | queued, executed on landing | WP3a research |
| SU-4 | Abduct with no valid landing point within `SpawnRange` 2 | refused, no payment (the create-unit effect fails first, `WaitToSpend`) | WP3a research |
| SU-5 | Whether an area search measures to the unit's centre or edge | centre | WP2a research (`CEffectEnumArea` flags) |
| SU-6 | Meaning of Fungal's `ToSelection` command flag (`voidmulti` AbilData:414) | no effect on our single-caster rule | WP4a research |
| SU-7 | Whether Fungal's reveal (`SuppressCloak`) also exposes burrowed units | yes (Liquipedia `Infestor (LotV)`: "reveal them for its duration") | WP4a |

Each stays a default until sourced; an unsourced default that the matrix shows matters becomes a B-ID in WP7.

---

## 15. Risks

- **Approach and backswing touch every cast path**, including mode switches. Mitigation: WP1c's agreement test
  over every refusal, the mode-switch tests of plan `2026-11-09-client-mode-commands`, and matrix bit-identity.
- **Removing hazard zones touches nine test files.** Mitigation: D-12's rule (rewrite, never delete without the
  same property), in its own commit of WP2b before the storm data.
- **Pulls move units outside the movement system** (no collision, over cliffs). Risk of landing inside a
  structure placed meanwhile or of desync with transport passengers. Mitigation: the landing is validated again
  at landing; if invalid, the unit lands at the nearest valid point by the same ring search, else stays at `from`
  (recorded in tests).
- **Fractional periods** could drift if implemented as accumulations. Mitigation: D-4 computes each pulse from the
  exact product; unit tests pin the tick lists of §3.
- **Area searches per tick** over 400 units. Mitigation: spatial grid, reused buffers; a `PERFORMANCE RATIONALE`
  block where the grid query diverges from a plain scan (C11); WP6 measures.
- **Presentation blocked by Q0080.** Mitigation: D-11's indicators keep the spells readable; mechanics ship
  regardless.
- **Matrix claims may not separate** at first sizes. Mitigation: the measurement step tunes sizes with recorded
  reasons; the claim stays.
- **Brief vs data** (root vs slow, storm values). Mitigation: D-2 states the rule; WP7 notes it for the owner in
  the evidence file.

---

## 16. Out of scope (recorded, Q0002)

- Other caster spells: Feedback, Psionic Storm's later patches, Consume, Parasitic Bomb, Neural Parasite,
  Microbial Shroud, Snipe, EMP, Guardian Shield, Force Field, Liberator zones, Graviton Beam (B-69, B-70 remain).
- Autocast, AI spell use, smart-cast nuances beyond D-7.6.
- Heroic units (no catalog unit is heroic) and the Mothership exclusion.
- Bunker passengers under Blinding Cloud (no bunker analogue).
- Radar (`RadarArc`) under Blinding Cloud (no radar mechanic).
- The energy regeneration delay (B-35, WP15 of the deviation plan) and moving the concussive slow (B-77) onto
  statuses, which the status engine makes possible as a follow-up.
- Effect art beyond what each spell's `c` step submits to the review gate (Q0080).
