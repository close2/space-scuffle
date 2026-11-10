# Blueprint: Spellcaster Mechanics (Psionic Storm, Abduct, Fungal Growth, Blinding Cloud)

> **Amendment 1 (owner decision, 2026-11-10): composable effect tree.** The owner decided in chat that spells are
> built from a small composable effect tree in StarCraft II's style instead of four fixed mechanisms. What changed:
>
> - **D-1 is rewritten.** The four `AbilityEffectSpec` variants `AreaOverTime`, `AreaOnce`, `Missile` and
>   `PullToCaster` are gone. In their place is one variant, `Effect(key)`, naming the root of an effect tree in a
>   new shared table `assets/catalog/ability_effects.ron`. The tree has ten node kinds, each the counterpart of an
>   SC2 editor effect class: `Set`, `Search`, `Persistent`, `LaunchMissile`, `Damage`, `ApplyStatus`,
>   `RemoveStatus`, `ModifyVitals`, `CancelOrders` and `Displace`, plus `Use(key)` to reference a named node.
>   Every node can carry an optional `require` filter, the counterpart of the editor's validators.
> - **New decisions D-15 to D-20** cover target contexts (which unit or point each child acts on), the evaluation
>   order, the validation limits (depth, node count, search count, no nested search, no cycles, all as typed
>   `CatalogError`s), storage without allocation per evaluation, the presentation mapping and the statuses'
>   effect references.
> - **Statuses stay in the shared `statuses.ron` table** (D-3, D-4 and D-5 are unchanged in substance). Their
>   damage moves into effect references (`initial_effect`, `periodic_effect`), as SC2's `CBehaviorBuff` does
>   (D-20). Fungal Growth now applies two statuses, as the game data does: damage and reveal, and the slow.
> - **§5.2 rewrites the four spells as trees.** Every value is still cited. A new **§5.4** writes four further
>   spells as data only: EMP, Feedback, Transfusion and Steady Targeting. It names what each one would still need.
> - **D-6 adds `AbilitySpec::origin`** (Abduct's landing placeholder). D-8, D-9, D-11 and D-13 are restated in
>   terms of the tree. §6 restates its phase order, and §11 to §15 are updated: the work packages,
>   tests, the determinism summary, the undetermined rules (SU-8 to SU-10) and the risks.
> - **WP1c becomes the effect-tree engine,** with its own unit, property and allocation tests. The cast pipeline
>   moves to WP1d and the client to WP1e. The packages after it still build one spell each.
>
> Everything else stands: the 5.0.14 values (§3), the cast pipeline (D-7), the hazard replacement (D-12), the
> matrix battles (§10) and the scope (§16).

The owner answered [Q0077](../questions/open/0077-caster-spells.md) with option 1: the presentation contracts
(`RenderSnapshot::areas` and `links`) stay as plan [`2026-11-09-effect-visualization`](2026-11-09-effect-visualization.md)
landed them, and a dedicated plan builds the mechanics of the four missing caster spells, in StarCraft II order of
combat impact: **Psionic Storm, then Abduct, then Fungal Growth, then Blinding Cloud**. This is that plan.

This plan:

- records each spell's StarCraft II 5.0.14 values and rules from the game data, with sources, converted to ticks
  (§3);
- lists what the simulation already has and what is missing (§2);
- builds the spells from a small composable effect tree in StarCraft II's style rather than from named spells (§4
  and §5, Amendment 1);
- states the rules per spell and their interactions with hold position, channels, transports, burrow, cloak,
  structures and massive units (§6 and §7);
- gives the client side, the matrix battles, the tests to write first and the measurements (§8 to §11), the undetermined rules, risks and scope (§14 to §16);
- splits the work into packages, one spell per package in the owner's order (§12).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-10 against `main` at
`d03ed19`. Amendment 1 was written the same day against `main` at `74932fb`. Line numbers drift, so search for
the quoted symbol. Claims not confirmed while planning are marked **verify**.

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

1. **Four spells, one small effect tree (Amendment 1).** The catalog gains an effect tree in the style of the SC2
   editor, not spells. Its ten node kinds are `Set`, `Search`, `Persistent`, `LaunchMissile`, `Damage`,
   `ApplyStatus`, `RemoveStatus`, `ModifyVitals`, `CancelOrders` and `Displace` (D-1). It also has **statuses**:
   timed per-unit effects with a speed factor, a weapon range delta, stun, reveal, and suppression of detection and
   boarding. A status's damage or healing is itself an effect reference.
   - Psionic Storm is `Persistent → Search → ApplyStatus`, and the status's initial and periodic effects are
     `Damage` nodes.
   - Fungal Growth is `LaunchMissile → Search → Set[ApplyStatus, ApplyStatus, Damage(0, responses)]`.
   - Blinding Cloud is `Persistent → Search → Set` of two filtered `ApplyStatus` nodes.
   - Abduct is `LaunchMissile(tether) → Set[CancelOrders, ApplyStatus, Persistent → Displace]`.

   No Rust names a spell (C5). The catalog names effects and statuses by string keys, in the new
   `assets/catalog/ability_effects.ron` and `assets/catalog/statuses.ron`.
2. **The game data decides, including two corrections to the brief.** All values come from SC2GameData 5.0.14
   (`5444e12f13`, chain `core → liberty → swarm → void → voidmulti`, as
   [`2026-11-04-catalog-time-audit`](../evidence/2026-11-04-catalog-time-audit.md) §2.2 established). Two results
   differ from the brief's wording: **Fungal Growth does not root at 5.0.14**, it slows by 75 % (`voidmulti`
   EffectData:1561 replaces the root with `FungalGrowthSlowMovement`, `MoveSpeedMultiplier="0.25"`; Liquipedia
   patch 4.0.0 "Slows the target's movement speed by 75%"), and **Psionic Storm's radius is 1.5 and its damage 80**
   at 5.0.14 (Liquipedia now shows the 5.0.15 values, radius 2 and 110 or 112 damage, which this plan must not
   adopt).
3. **Times stay exact, then round per pulse.** The game's periods are not whole ticks (Storm 10.196 ticks, Fungal
   4.464). Statuses and persistents store their period and duration as fixed-point ticks and fire pulse *k* on the tick
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
| `AbilitySpec { name, target_kind, energy_cost, cooldown_ticks, cast_range, effect, leave_name }`, `AbilityTargetKind::{TargetPosition, TargetEntity}` | `sim_types/src/commands.rs` | Gains four fields (D-6); target kinds reused |
| `admit_cast` → `admit_effect` → `commit_cast`, `would_admit_cast` (mutation-free admission shared with the client) | `sim_core/src/world/abilities.rs` | Extended with the effect tree's cast-time checks (D-17); the "every check before any write" invariant kept |
| `is_ability_unlocked` and `UpgradeEffect::UnlockAbility { target_slug, ability_slug }` | `world/upgrade_stats.rs`, `upgrades.rs` | Psionic Storm research (`upgrades.ron` `psionic_storm`, id 47, `effects: []` today, B-68) |
| Energy: `max_energy`, `start_energy`, `energy_regen_per_tick` | `catalog.rs`, `units.ron` | Already 200 / 50 / 0.0315 on the High Templar and Viper analogues and 200 / 75 / 0.0315 on the Infestor analogue, matching 5.0.14 (§3.5) |
| `ability_cooldowns: [u16; 4]` on `Entity` | `entity.rs` | Storm's cooldown; four slots suffice (no analogue gets more than two spells here) |
| `ActiveBuff { buff_id, remaining_ticks, speed_multiplier, attack_rate_multiplier }` with hard-coded IDs 1 to 3 | `entity.rs` | Left alone (the concussive slow, B-77, may move to statuses later, §16); statuses are a separate list |
| Shields-first damage with barrier absorption (B-41) | `world/abilities.rs` `step_hazards`, `entity.rs` `apply_damage` | Factored into one `apply_spell_damage` used by the `Damage` node (D-9) |
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

### D-1. Spells are composable effect trees (Amendment 1), no spell in Rust

`AbilityEffectSpec` gains one variant, `Effect(String)`. It names the root node of an effect tree in the new
shared table `assets/catalog/ability_effects.ron`. Effects work as they do in the StarCraft II data editor.
Each node is one small action, a node can start other nodes, and a spell is the tree its root starts. The editor
calls these nodes effects, of classes `CEffect*`. The four spells in `liberty`, `swarm`, `void` and `voidmulti`
EffectData are built only from the classes below (§3; the chains are quoted in §5.2).

| Node | Fields | Acts on (D-15) | SC2 editor equivalent | Source |
|---|---|---|---|---|
| `Set { children }` | 1 to 8 children, run in order | passes its context on unchanged | `CEffectSet` (`EffectArray`, run in array order) | `liberty` EffectData:714-719 (`FungalGrowthSet`); `swarm` EffectData:1498-1509 (`YoinkSet`) |
| `Search { radius, filter, max_count, child }` | radius in world units (≤ 16); `TargetFilter`; optional cap | a point; `child` runs once for each unit found, with that unit as its target | `CEffectEnumArea` (`AreaArray Radius/Effect/MaxCount`, `SearchFilters`) | `liberty` EffectData:2077-2084 (`PsiStormSearch`); `voidmulti` EffectData:1558-1560 |
| `Persistent { anchor, initial, periodic, period_ticks, count }` | `anchor: TargetPoint \| TargetUnit`; `initial` runs at creation; `periodic` runs at pulses 1 to `count`; period in exact ticks (D-4) | a point (fixed) or a unit (followed) | `CEffectCreatePersistent` (`InitialEffect`, `PeriodicEffectArray`, `PeriodicPeriodArray`, `PeriodCount`, `WhichLocation`) | `liberty` EffectData:2069-2076; `swarm` EffectData:1539-1547 (`WhichLocation TargetUnit`) |
| `LaunchMissile { to, speed, view, impact }` | `to: TargetPoint \| TargetUnit` (homing); speed in world units per tick; `view: Projectile \| Tether` | flies from the caster to the target; `impact` runs at the target | `CEffectLaunchMissile` (`ImpactLocation`, `ImpactEffect`, the mover's `MaxSpeed`) | `void` EffectData:1141-1146; `swarm` EffectData:1486-1494; MoverData:143-152 |
| `Damage { amount, attribute_bonus, responses }` | spell damage (shields first, no armour, D-9); bonus per attribute (`AttributeFlags`, as weapons already use); `responses: (acquire, flee)` | the target unit | `CEffectDamage` (`Amount`, `AttributeBonus`, `ResponseFlags`) | `liberty` EffectData:2089-2101; `void` EffectData:1138-1140 |
| `ApplyStatus { status }` | key into `statuses.ron` | the target unit | `CEffectApplyBehavior` | `liberty` EffectData:2085-2088 |
| `RemoveStatus { status }` | key into `statuses.ron` | the target unit | `CEffectRemoveBehavior` | `swarm` EffectData:1601-1611 |
| `ModifyVitals { life, shields, energy }` | each optional `(change, change_fraction)`: change in points, fraction of the current value, both clamped to `0..=max` | the target unit | `CEffectModifyUnit` (`VitalArray Change/ChangeFraction`) | `liberty` EffectData:2491-2499 (EMP), :609-615 (Feedback); `voidmulti` EffectData:997-1002 (Transfusion) |
| `CancelOrders { force_unburrow }` | clears the order queue, hold, current target, route and weapon swing; a mode channel in progress continues; with the flag, a burrowed mode is left at once | the target unit | `CEffectCancelOrder` plus the `InstantUnburrow` set | `swarm` EffectData:1525-1531; `void` EffectData:2527-2533 |
| `Displace { to, flight_ticks, tether, on_land }` | `to: Origin` (D-6 `origin`); straight flight in the plane, flying path; `on_land` runs on the moved unit when it lands | the target unit | `CEffectLaunchMissile` that launches the **target unit** with a mover (`YoinkLaunchTarget`, `LaunchLocation TargetUnit`, `FinishEffect`) | `swarm` EffectData:1554-1565, :1595-1600; MoverData:157-177 |
| `Use(key)` | a reference to a named node; resolved and erased at load | — | an effect link by id | every `EffectArray value="…"` |

**Validators.** Every node may carry `require: Option<TargetFilter>`, checked against the node's target unit when
the node runs. If it fails, the node and its children do nothing. This is the counterpart of the editor's
`ValidatorArray` restricted to unit predicates. Blinding Cloud's `CEffectSwitch` on `IsStructure` (`swarm`
EffectData:254-258) becomes a `Set` of two `ApplyStatus` nodes with complementary filters. So no `Switch` node is
needed. `TargetFilter` is `(alliance, planes, structures: Include | Exclude | Only, without_status:
Option<key>)`. `without_status` expresses the game's `NoYoink` validator (`swarm` EffectData:1468).

**Not built, deliberately.** The following are recorded as extensions; none is a new node:

- `Persistent` periodic offsets (`PeriodicOffsetArray`): no spell here uses them.
- `Displace` destinations other than `Origin`, such as knock-backs away from the source.
- `Damage` shield bonus and vital-fraction amounts, which EMP and Feedback need (§5.4).
- Channelled persistents, which Steady Targeting needs.
- `CEffectCreateUnit`, `CEffectIssueOrder`, `CEffectTeleport`, `CEffectSwitch`, `CEffectEnumTransport` and
  `CEffectModifyPlayer`. No spell in scope needs them (§16).

None of the four spells uses `ModifyVitals`, but it is still built and unit-tested in WP1c. The owner asked for
the heal and restore cases, and three of the §5.4 examples use the node.

`SpawnHazardZone` is removed, with `ActiveHazardZone` and `Entity::last_storm_damage_tick` (D-12). No `match` on an
ability, effect or status name exists anywhere (C5). Effects and statuses are referenced by string key from RON
and interned at load to `EffectId(u16)` and `StatusId(u16)`.

### D-2. The 5.0.14 data wins over the brief and over Liquipedia's current pages

Fungal Growth slows (×0.25) and does not root; Storm is radius 1.5 and 80 damage. Liquipedia is used for patch
history and for rules the data expresses only through validators (Fungal blocks Blink). This is charter §8.

### D-3. Statuses are catalog data in `assets/catalog/statuses.ron`

A status is `StatusSpec { name, duration_ticks, period_ticks, initial_effect, periodic_effect, move_speed_factor, ranged_weapon_range_delta, weapon_range_delta, range_floor, stun, reveal, suppress_detection,
suppress_collision, suppress_boarding, suppress_blink, presentation }` (§5.1). The two effect fields hold keys
into `ability_effects.ron` (D-20). It is loaded with the unit catalog
(`UnitCatalog` gains a `StatusTable`; `SimWorld::with_catalogs` keeps its signature), interned to a dense
`StatusId(u16)` in name order, and embedded by `sim_fixtures` like the other shipped catalogs
(`fixture_tests.rs` checks equality). An ability names its status by key; an unknown key is a typed
`CatalogError`. The effect table `ability_effects.ron` is loaded, interned to `EffectId(u16)` and embedded the
same way. A separate file was chosen over statuses inline in `units.ron` because two abilities (or two
archetypes) that apply the same status must share it for the non-stacking rule, and a shared table states that
once. Read at run time (C6); `runtime_catalog_tests.rs` gains a status edit that changes behaviour without
recompiling.

### D-4. Exact periods, rounded per pulse (S-5)

`duration_ticks` and `period_ticks` of statuses, and `period_ticks` of `Persistent` nodes, are `SimScalar` ticks authored to six decimals from the
Normal-time value × 17.857143 (the RON keeps the Normal value in a comment with its citation). Pulse *k* (counted
from the application, or from the persistent's creation) fires on the first tick whose index since the start is
`round_half_up(k × period)`, computed from the exact product each time (no accumulated error). A pulse at exactly
the end of the duration fires (16 × 0.25 = 4 Normal s gives Fungal's 16th pulse, the game's 25 total). Whole-tick
values (cooldown, outro, flight) are rounded once, halves up. Abduct's 0.1 s delay is now a one-pulse
`Persistent` with period 1.785714. That pulse rounds to 2 ticks, the same value as before.

### D-5. Non-stacking and refresh

An entity holds at most one instance of each `StatusId`. Applying a status it already has sets the remaining
duration back to `duration_ticks` from the current tick. It does not rerun the status's `initial_effect`, and it keeps the
periodic phase (pulse times still count from the first application). This reproduces Storm's 80, "one extra tick
after leaving", "never a single tick" (§3.1), and Fungal's refresh. Different statuses stack (Fungal and Storm on
one unit both damage). The context the status stores (D-15) stays that of the first application: its caster, owner and source
point. So kill credit and responses on later pulses go to the first caster (SU-8).

### D-6. `AbilitySpec` gains four fields (serde defaults keep old RON)

- `cast_backswing_ticks: u16` (`CastOutroTime`, default 0);
- `single_caster: bool` (`BestUnit`, default `true` for `TargetPosition`/`TargetEntity`, ignored by other kinds);
- `target_filter: Option<TargetFilter>` for `TargetEntity` abilities (Abduct);
- `origin: Option<OriginPlacement { forward_offset, search_range }>`, default `None`. When it is set, admission
  resolves a placement point for the target unit near the caster. It starts from the caster's position plus
  `forward_offset` along its heading, then tries a fixed ring pattern within `search_range`; D-8 gives the
  pattern. The resolved point becomes the tree's `origin` (D-15). If no point is valid, the cast is refused with
  no payment (SU-4). This is the counterpart of Abduct's placeholder `YoinkStartCreatePlaceholder`, a
  `CEffectCreateUnit` with `WhichLocation CasterUnitOrPoint`, `SpawnOffset 0,0.1`, `SpawnRange 2` and
  `PlacementIgnoreBlockers` (`swarm` EffectData:1447-1471). The game's later effects refer to it by location
  (`ImpactLocation Effect="YoinkStartCreatePlaceholder"`, :1491, :1556). Without the field, `origin` is the
  caster's position on the cast tick.

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

### D-8. Abduct is a tree whose `Displace` node holds a displacement state on the target

The rules below are unchanged from the first version of this plan. Each one now belongs to a node of the tree in
§5.2.

- **At cast (ability `origin`, D-6).** The landing point starts at the caster's position plus 0.1 along its
  heading. It is then the nearest point within 2 world units that is valid for the target's movement layer.
  Only terrain and structures count; units are ignored (`PlacementIgnoreBlockers`). The search uses
  `validate_displacement_destination_prevalidated` over a fixed ring pattern: the centre, then 8 directions at
  0.5, 1.0, 1.5 and 2.0, taking the first valid point in that order. If none is valid, the cast is refused
  without payment (SU-4). Otherwise the root `LaunchMissile` launches the tongue.
- **Tongue (`LaunchMissile { to: TargetUnit, view: Tether }`).** The tongue follows the target at 1.05 per
  tick and reaches it when it is within its speed of the target's edge. If the target dies, loads into a
  transport or becomes untargetable (cloaks with no detection), the missile is dropped and its `impact` never
  runs. Energy stays spent, because the game's missile is already launched.
- **Contact, the `impact` set.**
  - `CancelOrders { force_unburrow: true }` clears the target's order queue, hold position, current target and
    route, and cancels a weapon swing in progress. Any **mode channel in progress continues**, and a settled mode
    stays: a sieged tank lands sieged, as the `YoinkCancelOrder` validator's exemption for morphing units
    allows. A unit in a burrowed mode leaves it at once, with no channel.
  - `ApplyStatus("abducted")` applies the status.
  - `Persistent { anchor: TargetUnit, count: 1, period 1.785714 }` waits 2 ticks, then runs `Displace`.
- **Flight (`Displace { to: Origin, flight_ticks: 18 }`).** It sets `Entity::displacement = Some(Displacement {
  from, to, start_tick, flight_ticks, … })`. The position moves linearly from `from` to `to` over 18 ticks. In
  flight the unit has no collision, is not pushed, does not push, and ignores terrain (flying path); vision
  follows the position. On landing the displacement ends and `on_land` runs `RemoveStatus("abducted")`, as the
  game's `YoinkImpactSet` does. Normal separation then resolves overlaps.
- **The caster's death** after contact does not stop the pull, because the persistent and the displacement sit on
  the target.
- **Carriers.** An abducted transport takes its passengers with it; they follow the carrier's position by the
  transport rules.
- **Link.** The tongue's `view: Tether` and the `Displace` node's `tether: true` put `{ from: caster, to: target }`
  in `RenderSnapshot::links` from launch until landing, while the caster lives (D-19).
- **The game's two statuses become one.** The game applies `YoinkTentacle` (attack disabled) and `Yoink` (stun and
  more). `abducted` has `stun`, which already forbids attacking, so a separate tentacle status would change
  nothing.

### D-9. Spell damage and responses

`apply_spell_damage(entity, amount, source_owner, source_entity)` is what the `Damage` node calls. A barrier
absorbs first (B-41), then shields, then life. Armour does not apply, because the damage is of spell kind. The
amount is `amount` plus the node's `attribute_bonus` for each attribute the target has, read the same way weapons
read it (`sim_types/src/weapon.rs`).

The node's `responses: (acquire, flee)` mirror the game's `ResponseFlags`, which sit on a `CEffectDamage`:

- `acquire` makes the victim retaliate against the context's caster, if it exists and can be targeted
  (`apply_combat_retaliation`).
- `flee` lets the existing helpless-flee branch move an idle victim that cannot retaliate away from the context's
  **source point** (D-15: the centre of the search that found it). This replaces the hazard's "radius + 2"
  evacuation (B-78); the flee distance stays the unsourced B-13 value.

Responses apply only where the data puts them:

- Storm's `PsiStormDamageInitial` has them; the periodic `PsiStormDamage` has none (`liberty` EffectData:2089-2101).
- Fungal's are on a zero-damage `FungalGrowthSearchDummy` that runs at impact (`liberty` EffectData:736-741).
  §5.2 expresses it as `Damage { amount: 0, responses }`. A `Damage` with amount 0 deals nothing, does not reset
  the damage-taken timers and grants no kill credit; it only raises the responses.

Kill credit and the damage-taken timers (`ticks_since_last_damage`) behave as for weapon damage.

### D-10. Client: generic targeted casts (positional keys, Q0072 default)

Card entries come from every `TargetPosition`/`TargetEntity` ability of the selection (`CommandCatalog`, by
capability, never by name), after the mode entries. Pressing one arms `CommandMode::CastAtPoint { ability }` or
`CastOnUnit { ability }`; a left click submits `CastAbility`, Shift queues, right click or the cancel key leaves.
The cursor shows the ability's targeting radius as the targeting ring. That radius is computed from its effect
tree at load (D-19).
Locks: `NeedsResearch` (Storm), new `NeedsEnergy`, new `Cooldown`, `Transforming`. Submission and revalidation call
`would_accept_cast_order` (D-7.2). `PlayerView` gains per-unit energy and cooldowns where it lacks them (**verify**
what the HUD already reads).

### D-11. Presentation: contracts first, art behind Q0080

- **Contracts (checksum-neutral, WP2 to WP5).** D-19 gives the mapping from tree nodes:
  - `areas` come from point-anchored `Persistent` instances that search (Storm, Blinding Cloud). Each has
    `total_ticks` = the persistent's lifetime and `source` = the casting ability.
  - `links` come from tether missiles and displacements (Abduct, D-8).
  - A new `RenderSnapshot::spell_missiles: Vec<SpellMissileView { id, owner, position: [f32; 3], source:
    AbilitySource }>` shows `Projectile` missiles (Fungal's). It is refilled in place with fixed-size records,
    like `areas`.

  `StatusKind`
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

1. **WP1b** adds all new hashed state at empty defaults and bumps `CHECKSUM_SCHEMA_VERSION`. The new version is
   the next free number at the time: 26 if plan `2026-11-10-group-routing` has not taken it, else 27. The new state
   is:
   - `SimWorld::statuses: Vec<ActiveStatus>`, sorted by `(EntityId, StatusId)` (D-18). It replaces the
     per-entity list of the first version of this plan.
   - `Entity::cast_lock_ticks`.
   - `Entity::displacement: Option<Displacement>` (renamed from `pull`).
   - `SimWorld::persistents: Vec<ActivePersistent>` (formerly `active_areas`).
   - `SimWorld::effect_missiles: Vec<ActiveMissile>` (formerly `spell_missiles`).
   - `next_persistent_id` and `next_missile_id`.
   - `WeaponSpec::melee`, hashed via `siege_weapon` and `ActiveProjectile::weapon_spec`.

   Every instance hashes the `EffectId` of the node it will run next (a persistent's `Persistent` node, a missile's
   `LaunchMissile`, a displacement's `Displace`) and its stored `EffectContext` (D-15). The context is encoded in
   a fixed field order: owner, caster (presence byte and id), ability source, origin, source point, then the
   target as a tag byte (point 0, unit 1) and its payload. Statuses hash their `StatusId`, timing and context.
   `EffectId` and `StatusId` are dense indices in name order, stable for a given catalog, as `StatusId` already
   was. A catalog edit can therefore change checksums, which is right, because it changes behaviour. Specs and the
   effect arena are not hashed, just as `UnitModeSpec` is not.
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

### D-15. Target contexts (Amendment 1)

Every node runs with an `EffectContext`, which plays the part of the SC2 editor's effect locations. The editor
names *Caster*, *Source*, *Target* and *Origin* units and points. Its effects choose among them with
`WhichUnit`, `WhichLocation`, `ImpactLocation` and `LaunchLocation`, for example `WhichUnit Value="Caster"`
(`voidmulti` EffectData:1231-1234), `ImpactLocation Value="TargetPoint"` (`void` EffectData:1143) and
`WhichLocation Value="TargetUnit"` (`swarm` EffectData:1541). The community's explanation of these terms is in
the SC2Mapster thread ["Source, target, outer, origin, caster point/unit"](https://www.sc2mapster.com/forums/development/miscellaneous-development/174731-source-target-outer-origin-caster-point-unit?comment=7)
and its companion ["target location: what do these values mean"](https://www.sc2mapster.com/forums/development/data/185821-target-location-what-do-these-values-mean?comment=6).
Ours is the subset the spells need, fixed per node, so a node never chooses its location in data.

The context has these fields:

| Field | Type | Meaning |
|---|---|---|
| `owner` | `PlayerId` | The caster's owner at the cast. Never changes along the tree. Alliance filters are relative to it. |
| `caster` | `Option<EntityId>` | The casting unit. Nodes that read it check that it is alive (retaliation, links). `AnyButSelf` excludes it. |
| `ability` | `AbilitySource` | The ability that started the tree, for presentation and for art lookup. |
| `origin` | `SimVec2` | Captured on the cast tick: the placed point of D-6 `origin`, else the caster's position. |
| `source_point` | `SimVec2` | Where the current target was produced from. This is the flee origin of D-9. |
| `target` | `EffectTarget::{Point(SimVec2), Unit(EntityId)}` | What the node acts on. |

Each node passes a context to its children as follows. Every field not named is passed on unchanged.

| Node | Needs | Context given to its children |
|---|---|---|
| Root, `TargetPosition` ability | — | `target = Point(clicked point)`, `source_point = caster position` |
| Root, `TargetEntity` ability | — | `target = Unit(target)`, `source_point = caster position` |
| Status `initial_effect` / `periodic_effect` (D-20) | — | the context stored in the status, with `target = Unit(bearer)` |
| `Set` | point or unit | the same context |
| `Search` | point or unit (the unit's current position is the centre) | for each unit found: `target = Unit(found)`, `source_point = centre` |
| `Persistent { anchor: TargetPoint }` | point or unit (the unit's position at creation) | `target = Point(anchor)`, `source_point = anchor` |
| `Persistent { anchor: TargetUnit }` | unit | `target = Unit(anchor)`, `source_point = the unit's position at the pulse`. The persistent ends early if the unit dies, is loaded or becomes hidden. |
| `LaunchMissile { to: TargetPoint }` | point or unit (its position at launch) | `impact`: `target = Point(p)`, `source_point = p` |
| `LaunchMissile { to: TargetUnit }` | unit | `impact`: `target = Unit(u)`, `source_point = the missile's position at contact` |
| `Displace` | unit | `on_land`: `target = Unit(u)`, `source_point = the landing point` |
| `Damage`, `ApplyStatus`, `RemoveStatus`, `ModifyVitals`, `CancelOrders` | unit | leaves; no children |

A unit-targeted node whose unit is dead (life ≤ 0, not yet reaped), hidden or gone does nothing. This mirrors the
editor's default `Dead` and `Hidden` exclusions. A node's `require` filter is checked against the same unit.

**Static check.** At load, the kind (`Point` or `Unit`) is propagated from every ability root, according to its
target kind, and from every status's effect entries, which are always `Unit`. A node that needs a unit but is
reached with a point fails the load with `CatalogError::EffectTargetMismatch { effect, reached_from }`. A
`Displace { to: Origin }` reached from an ability without `origin` placement fails with
`CatalogError::DisplaceWithoutOrigin`. That check also follows `ApplyStatus` edges into the status's effects.

### D-16. Evaluation order (Amendment 1)

1. **A walk** is one synchronous evaluation of a node and everything under it that runs at once. It is
   depth-first and pre-order: a node acts, then its children run in authored order, and each child's whole
   synchronous subtree finishes before its next sibling starts. That is how a `CEffectSet` runs its `EffectArray`;
   SU-9 records the remaining doubt.
   - `Search` gathers all its candidates first. It takes the spatial-grid cells that overlap the circle, keeps the
     units within `radius` of the centre (centre distance, SU-5) that pass `filter`, and sorts them by `(distance²,
     EntityId)` ascending. It truncates the list to `max_count`, then runs `child` for each unit in that order. A
     unit killed or moved by an earlier child stays in the list; the dead check of D-15 skips it.
   - `ApplyStatus` on a unit without that status inserts it and runs its `initial_effect` inside the same walk.
     On a unit that has it, the status is refreshed (D-5).
   - `Persistent` creates an instance with the next id and runs `initial` inside the walk.
   - `LaunchMissile` creates an instance. Nothing else runs now.
   - `Displace` sets the unit's displacement. If the unit already has one, the node does nothing: a unit can have
     only one displacement.
2. **Where walks start.** Casts start walks in phase 1, or in movement for queued casts and casts after approach,
   as before. Phase 2 replaces `step_hazards` with four sub-steps, in this order:
   - (a) **missiles** advance in id order, and each one that arrives runs `impact`, then is removed;
   - (b) **persistents** in id order run `periodic` when pulse *k* is due (D-4), and are removed after their last
     pulse or when their anchor unit is gone;
   - (c) **statuses** in `(EntityId, StatusId)` order run `periodic_effect` when a pulse is due, then expire;
   - (d) **displacements** in `EntityId` order advance, and run `on_land` on landing.
3. **Instances created during a sub-step** are appended with fresh ids. Each sub-step iterates over the count it
   had when it started, so instances created in phase 2 are first stepped on the next tick. Their synchronous
   parts (`initial`, `initial_effect`) have already run. Missiles created by a cast in phase 1 are stepped in (a)
   on the same tick, which gives the impact tick `cast + ceil(distance / speed)` of §11. A status applied
   during (a) or (b) cannot pulse on the same tick, because periods are at least one tick (D-17) and pulse 1 falls
   at `round_half_up(period) ≥ 1`. A displacement's position is a pure function of `tick − start_tick` and first
   moves on the tick after it is created.
4. **Removal** is order-preserving (`retain`), so id order is iteration order. Damage from any sub-step leaves life
   ≤ 0 for the existing reaping in phase 8.

The order depends only on ids, `EntityId`s and fixed-point distances. It never depends on container addresses or
on float values.

### D-17. Validation limits (Amendment 1)

All limits are checked when the catalog loads, before any world exists. Each failure is a typed variant of
`CatalogError`, never a `String`. Every bound has a named constant in `sim_core`, with a comment saying why it
has its value.

| Check | Limit | Error |
|---|---|---|
| Every effect and status key resolves; names unique | — | `UnknownEffect { name, referenced_by }`, `UnknownStatus`, `DuplicateEffect` |
| No cycle through children, `Use`, or `ApplyStatus` → status effects | — | `EffectCycle { path }` |
| Synchronous depth: the longest chain run in one walk. `LaunchMissile.impact`, `Persistent.periodic`, `Displace.on_land` and status `periodic_effect` start new walks; `Persistent.initial` and status `initial_effect` do not. | `MAX_EFFECT_DEPTH = 8` (the deepest walk of the four spells, Storm's cast walk `Persistent → Search → ApplyStatus → Damage`, is 4) | `EffectTreeTooDeep { root, depth }` |
| `Set` width | 1 ..= `MAX_SET_CHILDREN = 8` | `EmptySet`, `SetTooWide` |
| Nodes reachable from one root, counting status effects | `MAX_TREE_NODES = 64` | `EffectTreeTooLarge` |
| Nodes in the whole catalog after flattening | `MAX_EFFECT_NODES = 4096` (fits `EffectId(u16)`, with margin) | `TooManyEffectNodes` |
| No `Search` inside a walk started under another `Search`'s child | — | `NestedSearch { outer, inner }` |
| `Search` radius; `max_count` | `0 < radius ≤ 16`; `1 ..= 256` when set | `InvalidSearch` |
| `Persistent` and status periods; counts | `period_ticks ≥ 1.0`; `1 ≤ count ≤ 1024`; a persistent has `initial` or `periodic`; `periodic` is set exactly when `count ≥ 1` | `InvalidPeriod`, `EmptyPersistent` |
| Missile speed | `0.01 ≤ speed ≤ 16` world units per tick | `InvalidMissileSpeed` |
| `Displace.flight_ticks` | `1 ..= 250` | `InvalidDisplacement` |
| `Damage` amount and bonuses; `ModifyVitals` | non-negative; at least one vital, `change_fraction` in `-1 ..= 1` | `NegativeDamage`, `InvalidVitalChange` |
| Target contexts (D-15) | — | `EffectTargetMismatch`, `DisplaceWithoutOrigin` |
| An `Effect(key)` ability has target kind `TargetPosition` or `TargetEntity` | — | `EffectAbilityNeedsTarget` |

**Work bound.** The rules above bound the work, without any cap at run time that could silently drop a spell:

- A walk visits at most `MAX_TREE_NODES` nodes per unit found by its single `Search`. There are no nested
  searches, so a walk is at most linear in the units within 16 of one point.
- Each live instance (missile, persistent, status, displacement) starts at most one walk per tick, because
  periods are at least one tick.
- Instances are created only by casts, which energy and cooldowns limit, and by walks, which the limits bound.
- WP6 measures the worst realistic case: 20 storms and 100 statuses among 400 units.

### D-18. Storage without allocation per evaluation (Amendment 1)

- **Effect arena (catalog, read-only).** At load, `ability_effects.ron` is flattened into
  `EffectCatalog { nodes: Vec<EffectNode>, children: Vec<EffectId>, facts: Vec<EffectFacts>, by_name:
  BTreeMap<String, EffectId> }`.
  - A node's children are a `(start: u16, len: u8)` range into the shared `children` slab.
  - `Use` is resolved and erased.
  - Named nodes take ids in name order. Inline nodes follow, in the depth-first order of their named parents,
    so ids are deterministic for a catalog.
  - `EffectFacts` holds what is computed once: the target kind needed, the synchronous depth, `area_radius`
    (D-19) and, for roots, `targeting_radius` and `needs_origin`.
- **Evaluator.** The evaluator is iterative, with an explicit fixed-size stack `[Frame; MAX_EFFECT_DEPTH]` held on
  the native stack, no recursion. A `Frame` is `{ node, context, next_child, search_cursor }`. The candidate list
  of the one active `Search` is a reused scratch buffer in `SimWorld`. It is not state and not hashed, and
  `clear()` keeps its capacity. The `NestedSearch` rule is what lets a single buffer suffice.
- **Instances.** `persistents` and `effect_missiles` are `Vec`s in `SimWorld` with capacity reserved when the
  world is created (64 each). They grow, amortised, only beyond that and never shrink.
- **Statuses.** Statuses live in one world table, `SimWorld::statuses`, sorted by `(EntityId, StatusId)`. Lookup
  is a binary search and insertion a shift. A per-entity `Vec` would allocate the first time each unit is
  affected. Entity removal uses `retain`.
- **Displacement.** `Entity::displacement` is an inline `Option`.

```rust
// PERFORMANCE RATIONALE:
// Why: a storm on a clump applies a status to tens of units each pulse; a Vec per entity would allocate
// the first time each unit is affected, and a boxed node tree would chase pointers on every walk.
// What: one world-level status table sorted by (EntityId, StatusId), a flattened node arena with child ranges,
// a fixed-depth explicit evaluation stack, and one reused search buffer (sound because searches never nest).
// Trade-off: insertion into the sorted table is O(statuses) and the arena is less readable than a recursive
// enum; in exchange evaluation allocates nothing in steady state and iteration order is the hash order.
```

The test is a counting global allocator in its own integration-test binary. It counts with an `AtomicUsize`; a
`static mut` is not used (C13). After a warm-up, 200 ticks with two live storms, a missile, a displacement and
twenty statuses must allocate zero bytes (§11). **Verify** at WP1c that C13 accepts an atomic static in test
code. If it does not, count with valgrind, as `sync_allocation_count_tests.rs` does.

### D-19. Presentation mapping (Amendment 1)

The presentation is filled from the instances after the tick, refilled in place, and never hashed. It is the
unhashed contract of D-11.

- **`areas`.** There is one `AreaView` per live `Persistent` instance with `anchor: TargetPoint` whose
  `EffectFacts::area_radius` is `Some(r)`. `r` is the largest radius of a `Search` that the persistent's
  `initial` or `periodic` subtree reaches synchronously. The view has centre = anchor, radius = `r`,
  `total_ticks = round_half_up(count × period)`, elapsed counted from creation, and `source` = the context's
  ability. Persistents anchored to a unit, and persistents that do not search, are not drawn as areas.
- **`spell_missiles`.** There is one `SpellMissileView` per live missile with `view: Projectile`.
- **`links`.** There is one `LinkView { from: caster, to: target unit }`, shown only while the caster lives, for
  each of:
  - a live missile with `view: Tether`;
  - a live unit-anchored persistent whose periodic subtree contains a `Displace { tether: true }` (Abduct's
    2-tick delay, so the tether does not flicker);
  - a live displacement whose node has `tether: true`.
- **Status flags.** These come from each status's `presentation` list, as in D-11.
- **Targeting ring.** The ring comes from the root's `targeting_radius`: the radius of the first `Search` met in
  pre-order through `Set`, `Persistent` and `LaunchMissile.impact`, or `None`.

No presentation field names a node kind the client must interpret. The client reads views, as it does today.

### D-20. A status's damage is an effect reference (Amendment 1)

**Decision: yes.** `StatusSpec` loses `initial_damage`, `periodic_damage` and `damage_responses`. It gains
`initial_effect: Option<key>` and `periodic_effect: Option<key>`, which run on the bearer with the stored
context (D-15). The reasons:

1. **It is the game's model.** `CBehaviorBuff` has `InitialEffect` and `PeriodicEffect` fields
   (`liberty` BehaviorData:715-716 for `PsiStorm`, `liberty` BehaviorData:337 for `FungalGrowth`).
2. **Responses fall out of the data.** The data puts responses on the initial damage effect and not on the
   periodic one, so no status field is needed.
3. **Heals over time come free.** Transfusion's `TransfusionHealTick` is a `ModifyVitals` (§5.4), with no new
   status fields.
4. **There is one damage path.**

The costs are an indirection, and a cycle check that follows status edges (D-17). The duration, period,
non-stacking and refresh rules (D-3, D-4, D-5) are unchanged.

The modifiers stay status fields: speed, range, stun, reveal and the suppressions. In the game they are
`Modification` fields of the buff, not effects. `FinalEffect` (on expiry) is not adopted; none of the four
spells uses it.

---

## 5. Data model

### 5.1 `assets/catalog/statuses.ron`

```ron
// Timed per-unit effects applied by `ApplyStatus` nodes. Times are 25 Hz ticks (Normal s × 17.857143), exact;
// pulses round per pulse (plan 2026-11-10-spellcaster-mechanics D-4). Damage and healing are effect references
// into ability_effects.ron (D-20). Every value cites SC2GameData 5.0.14 (5444e12f13).
(
    statuses: [
        (
            name: "psionic_storm",
            // PsiStorm buff: liberty BehaviorData:709-716 (0.6665 / 0.571 Normal s)
            duration_ticks: 11.901786,
            period_ticks: Some(10.196429),
            initial_effect: Some("psionic_storm_damage_initial"),   // InitialEffect, liberty BehaviorData:715
            periodic_effect: Some("psionic_storm_damage"),          // PeriodicEffect, liberty BehaviorData:716
            presentation: [DamageOverTime],
        ),
        (
            name: "abducted",
            // Yoink buff, swarm BehaviorData:1074-1088 (Stun :1078, SuppressCloak :1079, SuppressCollision :1080,
            // SuppressPassenger :1084, Duration :1087); removed at landing by the Displace node's on_land.
            // YoinkTentacle (attack disabled, voidmulti BehaviorData:562-566) is subsumed by stun (D-8).
            duration_ticks: 35.714286,
            stun: true, reveal: true, suppress_collision: true, suppress_boarding: true,
            presentation: [Stunned],
        ),
        (
            name: "fungal_growth",
            duration_ticks: 71.428571,       // swarm BehaviorData:839 (4 Normal s)
            period_ticks: Some(4.464286),    // swarm BehaviorData:840 (0.25)
            periodic_effect: Some("fungal_growth_damage"),          // liberty BehaviorData:337
            reveal: true,                    // SuppressCloak, liberty BehaviorData:332
            presentation: [DamageOverTime, Revealed],
        ),
        (
            name: "fungal_growth_slow",
            // FungalGrowthSlowMovement, voidmulti BehaviorData:691-705
            duration_ticks: 71.428571,       // :696 (4 Normal s)
            move_speed_factor: 0.25,         // :697
            suppress_boarding: true,         // SuppressPassenger, :698
            suppress_blink: true,            // Liquipedia Infestor (LotV), patch 4.7.1 (D-2)
            presentation: [Slowed],
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

Every field not shown takes its neutral default: `period_ticks: None`, no effects, factor 1, deltas 0, flags
false. `range_floor` (default 0.1, SU-2) bounds a reduced range from below; a weapon already at or below the floor
is unchanged.

**Validation** fails with typed `CatalogError`s:

- names are unique;
- the duration is positive;
- `period_ticks` is present exactly when `periodic_effect` is, and is at least 1.0 (D-17);
- the speed factor is in `0..=1`;
- every effect key resolves (D-17).

**Fungal now applies two statuses**, as the data does: `FungalGrowthSet` applies `FungalGrowth` and, at
`voidmulti`, `FungalGrowthSlowMovement` (`voidmulti` EffectData:1561-1563). The first version of this plan merged
them into one. Keeping them separate costs nothing and keeps each status one game behaviour. Their durations are
equal, so they expire on the same tick.

### 5.2 The four spells as trees

**`assets/catalog/ability_effects.ron`.** A child is written inline or as `Use("name")`. Every node accepts
`require: Some(filter)` (default `None`, D-1). A filter's omitted fields default to admitting everything:
`alliance: Any`, both planes, `structures: Include`, no `without_status`. Line citations without a layer are in
the layer named in the block's heading comment.

```ron
// Ability effect trees. Node kinds mirror the SC2 editor's CEffect classes (plan 2026-11-10-spellcaster-mechanics
// D-1). Times are 25 Hz ticks; speeds world units per tick (Normal × 1.4 ÷ 25). SC2GameData 5.0.14 (5444e12f13).
(
    effects: [
        // ---- Psionic Storm: liberty EffectData:2069-2101 ----
        (
            name: "psionic_storm",                          // PsiStormPersistent, :2069-2076
            node: Persistent((
                anchor: TargetPoint,                        // CEffectCreatePersistent default, core EffectData:35
                initial: Some(Use("psionic_storm_search")), // InitialEffect, :2071
                periodic: Some(Use("psionic_storm_search")),// PeriodicEffectArray, :2073
                period_ticks: 10.2,                         // PeriodicPeriodArray 0.5712, :2074
                count: 6,                                   // PeriodCount, :2072
            )),
        ),
        (
            name: "psionic_storm_search",                   // PsiStormSearch, :2077-2084
            node: Search((
                radius: 1.5,                                // :2080
                filter: (alliance: Any, planes: [Ground, Air], structures: Exclude),   // :2079
                child: ApplyStatus((status: "psionic_storm")),   // PsiStormApplyBehavior, :2085-2088
            )),
        ),
        (
            name: "psionic_storm_damage_initial",           // PsiStormDamageInitial, :2089-2096
            node: Damage((amount: 10.0, responses: (acquire: true, flee: true))),  // :2091, :2093-2094
        ),
        (
            name: "psionic_storm_damage",                   // PsiStormDamage, :2097-2101 (no responses)
            node: Damage((amount: 10.0)),                   // :2100
        ),

        // ---- Abduct: swarm EffectData:1447-1611 (voidmulti :1035-1045) ----
        (
            name: "abduct",                                 // YoinkLaunchMissile, :1486-1494
            node: LaunchMissile((
                to: TargetUnit,
                speed: 1.05,                                // YoinkMissile MaxSpeed 18.75, swarm MoverData:147
                view: Tether,
                impact: Set((children: [                    // YoinkSet, :1498-1509
                    CancelOrders((force_unburrow: true)),   // YoinkCancelOrders :1525-1531; InstantUnburrow
                    ApplyStatus((status: "abducted")),      // YoinkApplyTentacleBehavior :1520, YoinkApplyBehavior :1532
                    Persistent((                            // YoinkDelayPersistent, :1539-1547
                        anchor: TargetUnit,                 // WhichLocation TargetUnit, :1541
                        periodic: Some(Displace((           // YoinkLaunchTarget, :1554-1565
                            to: Origin,                     // ImpactLocation Effect="YoinkStartSet" (the placeholder), :1556
                            flight_ticks: 18,               // YoinkMover FlightTime 1 Normal s, swarm MoverData:167
                            tether: true,
                            on_land: Some(RemoveStatus((status: "abducted"))),   // YoinkImpactSet, :1595-1611
                        ))),
                        period_ticks: 1.785714,             // PeriodicPeriodArray 0.1, :1545
                        count: 1,                           // PeriodCount 50 are retries until the launch succeeds
                                                            // (:1544); ours succeeds at once or not at all (D-8)
                    )),
                ])),
            )),
        ),

        // ---- Fungal Growth: liberty EffectData:416-420, 714-741; void :1138-1146; voidmulti :693-695, :1558-1572 ----
        (
            name: "fungal_growth",                          // FungalGrowthLaunchMissile, void :1141-1146
            node: LaunchMissile((
                to: TargetPoint,                            // ImpactLocation TargetPoint, void :1143
                speed: 0.672,                               // FungalGrowthWeapon MaxSpeed 12, void MoverData:18
                view: Projectile,
                // ImpactEffect FungalGrowthInitialSet (liberty :416-420; SurfaceForSpellcast removed, voidmulti :693-695)
                impact: Search((
                    radius: 2.25,                           // FungalGrowthSearch, voidmulti :1559
                    filter: (alliance: Enemy, planes: [Ground, Air], structures: Exclude),   // liberty :731
                    child: Set((children: [                 // FungalGrowthSet, liberty :714-719
                        ApplyStatus((status: "fungal_growth")),        // [0], liberty :717
                        ApplyStatus((status: "fungal_growth_slow")),   // [1], voidmulti :1561-1563
                        // FungalGrowthSearchDummy, liberty :736-741 (radius 2.25, voidmulti :407-408): responses only
                        Damage((amount: 0.0, responses: (acquire: true, flee: true))),
                    ])),
                )),
            )),
        ),
        (
            name: "fungal_growth_damage",                   // FungalGrowthDamage
            node: Damage((amount: 1.5625)),                 // voidmulti :2389; armoured bonus 0, void :1139
        ),

        // ---- Blinding Cloud: swarm EffectData:232-258; voidmulti :157-159 ----
        (
            name: "blinding_cloud",                         // BlindingCloudCP, :236-242
            node: Persistent((
                anchor: TargetPoint,                        // BlindingCloudSearch ImpactLocation TargetPoint, :245
                initial: Some(Use("blinding_cloud_search")),   // :238
                periodic: Some(Use("blinding_cloud_search")),  // :240
                period_ticks: 8.928571,                     // :241 (0.5)
                count: 15,                                  // voidmulti :158
            )),
        ),
        (
            name: "blinding_cloud_search",                  // BlindingCloudSearch, :243-248
            node: Search((
                radius: 2.0,                                // :247
                filter: (alliance: AnyButSelf, planes: [Ground], structures: Include),   // :246
                child: Set((children: [                     // BlindingCloudSwitch on IsStructure, :254-258
                    ApplyStatus((status: "blinding_cloud", require: Some((structures: Exclude)))),           // :232-235
                    ApplyStatus((status: "blinding_cloud_structure", require: Some((structures: Only)))),    // :249-253
                ])),
            )),
        ),
    ],
)
```

The trees reproduce every number of §3:

- **Storm.** The initial search runs on the cast tick, and periodic searches 1 to 6 at `round(k × 10.2)`: ticks
  0, 10, 20, 31, 41, 51, 61. Status pulses fall at 0 (initial), then 10, 20, 31, 41, 51, 61, 71: 80 in all.
- **Fungal.** 16 pulses at the §3.3 ticks give 25.
- **Blinding Cloud.** The initial search plus 15 gives 16 searches at `round(k × 8.929)`.
- **Abduct.** Contact, then the displacement starts at contact + 2 and lands at contact + 20.

Bunker passengers under Blinding Cloud (`BlindingCloudStructureSet` → `CEffectEnumTransport`, `swarm`
EffectData:3-12) remain out of scope (§16). They would need an enumerate-passengers node.

**Abilities in `units.ron`** keep their §3 values and name their root:

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
        effect: Effect("psionic_storm"),
    ),
],
```

```ron
// tongue_chameleon (Viper analogue)
abilities: [
    (
        name: "Abduct",
        target_kind: TargetEntity,
        energy_cost: 75.0, cooldown_ticks: 0, cast_range: 9.0,   // swarm AbilData:1230, :1233
        cast_backswing_ticks: 14,       // voidmulti AbilData:1754 (0.8)
        target_filter: Some((alliance: AnyButSelf, planes: [Ground, Air], structures: Exclude,
                             without_status: Some("abducted"))),   // voidmulti AbilData:1755; NoYoink, swarm EffectData:1468
        origin: Some((forward_offset: 0.1, search_range: 2.0)),  // YoinkStartCreatePlaceholder, swarm EffectData:1459-1460
        effect: Effect("abduct"),
    ),
    (
        name: "Blinding Cloud",
        target_kind: TargetPosition,
        energy_cost: 100.0, cooldown_ticks: 0, cast_range: 10.0,  // swarm AbilData:1202, :1204
        effect: Effect("blinding_cloud"),
    ),
],
```

```ron
// mycelium_shaman (Infestor analogue)
abilities: [
    (
        name: "Fungal Growth",
        target_kind: TargetPosition,
        energy_cost: 75.0, cooldown_ticks: 0, cast_range: 10.0,   // liberty AbilData:486; void AbilData:674
        effect: Effect("fungal_growth"),
    ),
],
```

`upgrades.ron` `psionic_storm` gains `effects: [UnlockAbility(target_slug: "cerebral_elder", ability_slug:
"Psionic Storm")]`. Ability names are display data the unlock rule already keys on; no Rust reads them.

### 5.3 State (hashed, D-13)

| Type | Fields |
|---|---|
| `EffectContext` | `owner: PlayerId`, `caster: Option<EntityId>`, `ability: AbilitySource`, `origin: SimVec2`, `source_point: SimVec2`, `target: EffectTarget::{Point(SimVec2), Unit(EntityId)}` (D-15) |
| `ActiveStatus` (in `SimWorld::statuses`, sorted by `(entity, status)`) | `entity: EntityId`, `status: StatusId`, `applied_tick: Tick`, `expires_at: SimScalar` (ticks since `applied_tick`), `pulses_done: u16`, `context: EffectContext` |
| `ActivePersistent` | `id: u32`, `node: EffectId`, `context`, `anchor: PersistentAnchor::{Point(SimVec2), Unit(EntityId)}`, `created_tick: Tick`, `pulses_done: u16` |
| `ActiveMissile` | `id: u32`, `node: EffectId`, `context`, `position: SimVec2`, `target: EffectTarget` |
| `Displacement` (on `Entity`) | `node: EffectId`, `context`, `from: SimVec2`, `to: SimVec2`, `start_tick: Tick` |
| `Entity::cast_lock_ticks` | `u16` |

Specs and the effect arena are not hashed, just as `UnitModeSpec` is not. State points at them through `EffectId`
and `StatusId`, which are stable for a catalog.

### 5.4 Further spells as data only (generality check, not in scope)

These are written against the node set of D-1 to test its generality. None of them is built by this plan (§16).
Each needs a few things the four spells do not, and each one is named here. The cast values not quoted here
(energy, range, missile speed) are **verify** items for the plan that builds the spell.

**EMP (Ghost).** EMP becomes `LaunchMissile → Search → Set`, all existing nodes.

```ron
(name: "emp", node: LaunchMissile((                     // EMPLaunchMissile, liberty EffectData:2464-2470
    to: TargetPoint, speed: /* EMP2Weapon mover, verify */, view: Projectile,
    impact: Search((
        radius: 1.5,                                    // swarm EffectData:2652-2654
        filter: (alliance: Any, planes: [Ground, Air], structures: Include),   // "-;Hidden,Invulnerable", liberty :2476
        child: Set((children: [                         // EMPSet, liberty :2478-2483
            Damage((amount: 0.0, shield_bonus: 100.0, responses: (acquire: true, flee: true))),  // swarm :1014-1016
            ModifyVitals((energy: Some((change: -100.0)))),                                      // swarm :1017-1020
            ApplyStatus((status: "emp_decloak")),       // EMPApplyDecloakBehavior, liberty :793
        ])),
    )),
))),
```

- **Needs a new field:** `Damage.shield_bonus`, the game's `ShieldBonus`: extra damage that only shields take.
- **Approximation available today:** `ModifyVitals((shields: Some((change: -100.0))))` changes the vitals the
  same way. It does not count as damage, so a barrier would not absorb it and it would grant no kill credit.

**Feedback (High Templar).** Feedback is a unit-targeted `Set` of two nodes:
`Set[Damage(amount from energy), ModifyVitals(energy −100 %)]`.

```ron
(name: "feedback", node: Set((children: [               // FeedbackSet, liberty EffectData:679-684
    Damage((amount: 0.0, amount_from_target_vital: Some((vital: Energy, fraction: 0.5)),   // voidmulti :401-403
            responses: (acquire: true, flee: true))),   // liberty :598-599
    ModifyVitals((energy: Some((change_fraction: -1.0)))),   // FeedbackEnergyLoss, liberty :609-615
]))),
```

- **Order matters, and D-16 fixes it.** The damage reads the energy before the next sibling drains it, as in the
  game's array order.
- **Needs a new field:** `Damage.amount_from_target_vital`, the game's `VitalFractionCurrent`.
- **Needs a filter predicate:** "has energy", for the ability's target filter. The game's exclusions
  (`NotShieldBattery`, `NotNexus`, …; `swarm` :2202-2205, `voidmulti` :404) become structure exclusions.

**Transfusion (Queen).** Transfusion is an instant heal plus a heal-over-time status. Both use existing nodes,
and the periodic heal is a status effect reference (D-20).

```ron
(name: "transfusion", node: Set((                       // TransfusionImpactSet, voidmulti EffectData:890-895
    require: Some((life_below_max: true)),              // LifeNotFull, :891
    children: [
        ModifyVitals((life: Some((change: 75.0)))),     // Transfusion, voidmulti :997-1002
        ApplyStatus((status: "transfusion")),           // TransfusionAB, :879-882
    ],
))),
(name: "transfusion_heal_tick", node: ModifyVitals((life: Some((change: 0.3125))))),   // voidmulti :883-889
// statuses.ron: (name: "transfusion", duration_ticks: 178.571429 /* 10 Normal s, voidmulti BehaviorData:541 */,
//                period_ticks: Some(1.116071) /* 0.0625, :542 */, periodic_effect: Some("transfusion_heal_tick"))
```

- 160 pulses × 0.3125 = 50 over 7.14 Faster s. With the 75 at once, that is the game's 125.
- The period of 1.116 ticks passes D-17's limit of at least one tick.
- **Needs a filter predicate:** `life_below_max`. No new node.

**Steady Targeting (Ghost, `ChannelSnipe`).** Steady Targeting cannot be expressed yet. Its shape would be
`Persistent(anchor: TargetUnit, channelled, count 32, period 1.116, expire: Damage(130, +40 vs Psionic))`
(`voidmulti` EffectData:1206-1230, AbilData:1248-1262). It needs three things this plan does not have:

1. **A channel.** The persistent is `Channeled` (:1215), so the caster's channel order holds it. It is cancelled
   when the caster takes another order, is stunned or dies, or when `PeriodicValidator` fails (:1218). The
   caster is refunded (`ChannelSnipeEnergyRefund`, :2145-2152).
2. **`Persistent.expire`**, the game's `ExpireEffect` (:1216).
3. **An attribute requirement in `TargetFilter`** (`Biological`, AbilData:1254).

`Damage.attribute_bonus` already covers the +40 against Psionic. The channel is a new capability: caster-side
channel state and its cancellation rules. That belongs to the plan that builds snipe, not to a new node here.

**Summary.**

| Spell | Expressible today | Still needs |
|---|---|---|
| EMP | approximately (shields through `ModifyVitals`) | field `Damage.shield_bonus` for the game's exact semantics |
| Feedback | no | field `Damage.amount_from_target_vital`; filter predicate "has energy" |
| Transfusion | almost | filter predicate `life_below_max` |
| Steady Targeting | no | channel capability, `Persistent.expire`, attribute filter |

None of the four needs a new node kind. Three need small field extensions, and one needs a channel capability.

---

## 6. Simulation rules per spell

Tick placement and evaluation order are those of D-16. Casts start their tree's walk in phase 1 (non-queued,
in range) or in movement (queued, or after approach). Phase 2 replaces `step_hazards` with four sub-steps:

- (a) missiles;
- (b) persistents;
- (c) statuses;
- (d) displacements.

Damage feeds the same death reaping (phase 8). Every search uses the spatial hash grid, with candidates sorted by
`(distance², EntityId)`. The rules below are what the §5.2 trees do. They are stated per spell because the tests
of §11 assert them.

### 6.1 Psionic Storm

1. Cast: point in range (D-7), energy ≥ 75, cooldown 0, `psionic_storm` researched. Pay 75, cooldown 36, lock 9.
2. The root `Persistent` opens at the point on the cast tick. Its `initial` search runs at once, and periodic
   search *k* (k = 1 … 6) on `created + round(k × 10.2)`.
3. Each search applies `psionic_storm` to every unit within 1.5 of the centre (centre distance, as
   `CEffectEnumArea` measures to the unit's position; **verify** whether the game adds the unit's radius; the
   planning read found no `IncludeRadius` flag on `PsiStormSearch`) that is damageable, not a structure, any owner.
4. First application: the status's `initial_effect` deals 10 at once, responses on. Pulses at `applied + round(k × 10.196)`, 10 each, while the
   status lasts; refresh per D-5. Expiry: `applied + expires_at` passes without refresh.
5. The persistent closes after its last pulse (tick 61); `areas` shows it from creation to close (D-19).

### 6.2 Abduct

1. Cast: target unit admitted by `target_filter`, targetable for the caster's owner, edge distance ≤ 9 (after
   approach), energy ≥ 75, origin placement exists (D-6, D-8). Pay 75, lock 14.
2. The tongue (`LaunchMissile`, `Tether`) flies at 1.05 per tick toward the target. Contact runs the `impact`
   set of D-8. After a one-pulse persistent of 2 ticks, the 18-tick `Displace` runs, then landing.
3. While `abducted`: no movement of its own, no attack, no cast, no boarding, not pushed, no collision; orders
   given meanwhile are queued and start on landing (SU-3); visible to every player with vision (reveal).
4. A second Abduct on a unit that carries `abducted` is refused at admission by the target filter's
   `without_status` (game: validator `NoYoink`, `swarm` EffectData:1468). A `Displace` on a unit that is already
   displaced does nothing (D-16).

### 6.3 Fungal Growth

1. Cast: point in range, energy ≥ 75. Pay 75; no cooldown, no lock.
2. A `LaunchMissile` flies from the caster to the point at 0.672 per tick ignoring terrain; impact on the tick it
   reaches the point (distance ≤ speed).
3. Impact runs one search: radius 2.25, enemies of the caster's owner only, no structures. Each unit found gets
   `fungal_growth`, then `fungal_growth_slow`, then a zero-damage `Damage` that raises the responses (the game's
   search dummy; SU-10).
4. `fungal_growth` deals 1.5625 every 4.464 ticks (16 pulses, 25 in all) and reveals the unit: cloaked and burrowed
   units become targetable by anyone with vision of their cell. `fungal_growth_slow` multiplies movement by 0.25
   and forbids boarding and Blink. Both last 71.43 ticks and refresh per D-5.

### 6.4 Blinding Cloud

1. Cast: point in range, energy ≥ 100. Pay 100.
2. A `Persistent` opens on the cast tick; 16 searches (initial + 15) at `round(k × 8.929)`, k = 0 … 15; radius 2; ground units and
   structures of every owner but the caster itself.
3. Units get `blinding_cloud` (the `require: structures Exclude` branch): every weapon with `melee: false` has effective range `max(range − 20, floor)` while
   it lasts (floor 0.1, SU-2; a weapon at or below the floor is untouched). Range modifiers combine as range +
   upgrades + status delta, then the floor. A sieged weapon whose `min_range` exceeds its reduced range cannot fire.
   Acquisition uses the reduced range, so a ranged unit in the cloud walks to contact or holds.
4. Structures get `blinding_cloud_structure` (the `structures Only` branch): all weapons reduced the same way, and the structure does not detect
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
  the targeting ring uses the ability's `targeting_radius` from the loaded effect catalog (D-19), or none. A click outside range is
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

**WP1a (catalog), unit tests in `sim_core`'s catalog module.** One test per `CatalogError` variant of D-17 and
D-15, each on a minimal scratch RON:

- unknown key; duplicate name; a cycle through `Use`, and one through `ApplyStatus` → status → effect;
- depth 9; a `Set` of 0 and of 9; 65 nodes under one root; a nested `Search`;
- bad radius, period, count, speed and flight;
- a unit node reached from a point root; `Displace` without `origin`;
- an `Effect` ability of a non-targeted kind.

Flattening is deterministic: the same RON gives the same `EffectId`s and the same arena, and reordering the
named entries in the file leaves the ids unchanged, because ids are assigned in name order. `EffectFacts` give
the expected depth, `area_radius` and `targeting_radius` for the four §5.2 trees, loaded from a scratch copy.

**WP1c (effect-tree engine): `effect_tree_tests.rs`, `effect_tree_properties.rs`,
`effect_tree_allocation_tests.rs`.** These run on scratch archetypes and scratch effect catalogs only.

- *Per node.*
  - `Set` runs its children in order: a `Damage` then a `ModifyVitals` that drains energy, observed in order.
  - `Search` admits exactly the filter's units (alliance, planes, structures, `without_status`, dead and hidden
    excluded), applies them in `(distance², EntityId)` order and stops at `max_count`.
  - `Persistent` pulses at the D-4 ticks; a unit anchor ends when the unit dies or loads.
  - `LaunchMissile` to a point impacts on `cast + ceil(d / speed)`; to a unit it homes, and is dropped if the
    target dies, loads or becomes untargetable.
  - `Damage`: shields, then life, armour ignored, attribute bonus, barrier first, both responses; amount 0 only
    raises responses.
  - `ApplyStatus` / `RemoveStatus`: insert, refresh and remove, with `initial_effect` run once.
  - `ModifyVitals`: change and fraction, clamped to `0..=max`.
  - `CancelOrders`: queue, hold and swing cleared, a mode channel kept, a burrowed mode left.
  - `Displace`: linear flight, no collision, landing re-validated, `on_land` run.
  - `require` filters each node.
- *Contexts (D-15).* A search child sees the found unit and `source_point` = the centre. A status's periodic
  effect sees the bearer and the first applier's caster. `origin` is fixed at the cast.
- *Order (D-16).* Instances created in phase 2 first step on the next tick. Two persistents created on the same
  tick pulse in id order. A status applied in sub-step (b) does not pulse in (c) on the same tick.
- *Status timing (D-4, D-5).* Pulse ticks for periods 10.196 and 4.464 equal the lists in §3. A refresh keeps
  the phase and does not rerun `initial_effect`.
- *Property tests (proptest, a new `sim_core` dev-dependency from the workspace).*
  - Random valid trees of up to `MAX_TREE_NODES` nodes, over random worlds of up to 64 units, give identical
    checksums when evaluated twice and when units are inserted in a permuted order.
  - The node visits counted by a test hook never exceed the D-17 work bound.
  - Every generated invalid tree is rejected with the matching error variant.
- *Allocation (D-18).* A counting global allocator in its own test binary. After warm-up, 200 ticks with two
  storms, a missile, a displacement and twenty statuses allocate nothing.

**WP1d (pipeline), `spellcaster_pipeline_tests.rs`:**

- approach then cast (point and unit): the cast tick equals the first tick in edge range;
- no payment before the cast;
- the backswing locks movement, attack and casts for exactly N ticks;
- `single_caster` picks the nearest caster, then the one with more energy, then the lowest id; queued casts go to
  all;
- a cloaked enemy is untargetable without detection and targetable with it;
- `origin` placement refuses a cast with no valid point, without payment;
- `would_accept_cast_order` agrees with the tick on every refusal (the existing `would_admit_cast` agreement test
  pattern).

**WP2 (Storm), `psionic_storm_tests.rs`:** a unit inside throughout takes exactly 80 at ticks 0, 10, 20, 31, 41, 51,
61, 71; two overlapping storms still 80; a unit leaving after the first search takes 20; own units hit; structures
untouched; air hit; burrowed and cloaked hit without detection; loaded units untouched; shields before life and no
armour; a barrier absorbs; held units do not move; an idle unit that cannot retaliate flees from the centre; an
armed idle unit acquires the caster; refused without research, with < 75 energy, on cooldown (36 ticks); the area
appears in `areas` from creation to tick 61 with its source and radius 1.5 (D-19). Rewrites of the hazard tests (D-12).

**WP3 (Abduct), `abduct_tests.rs`:** tongue contact tick = launch + ceil(gap / 1.05); landing 2 + 18 ticks after
contact at the fixed point even if the Viper moves or dies; ground unit pulled across a cliff lands on the
caster's level; air stays air; a massive unit is pulled; a sieged tank lands sieged and a mode channel in progress
completes; a burrowed unit (detected) is unburrowed at contact; orders, hold, swing cancelled; no attack, cast,
boarding during the pull; refused with no landing point (Viper over unpathable ground, SU-4) and with no
payment; refused on an undetected cloaked enemy, on a structure, on itself; a tongue at a target that loads into a
transport is dropped; `links` has the tether without a gap from launch to landing, through the 2-tick delay (D-19).

**WP4 (Fungal), `fungal_growth_tests.rs`:** missile impact tick = cast + ceil(distance / 0.672); 25 in 16 pulses at
the §3.3 ticks; enemies only (own units and allies in the radius untouched); speed ×0.25 measured over a straight
walk; Blink refused and boarding refused while affected; the two statuses expire on the same tick; a cloaked enemy becomes targetable without detection for
71 ticks; a recast refreshes without repeating damage; armour ignored.

**WP5 (Blinding Cloud), `blinding_cloud_tests.rs`:** a marine-analogue (range 5) in the cloud cannot hit a target at
1 but hits one in contact; a melee unit is unaffected; an air unit over the cloud is unaffected; a ground unit
outside shooting into the cloud is unaffected; the status lasts 11 ticks after the unit leaves; a sieged tank
inside cannot fire; a detector structure inside stops revealing a cloaked unit and resumes after; 16 searches at
the §3.4 ticks; own units affected; the caster's own body never.

Each package also extends `runtime_catalog_tests.rs` (a status or effect-node value edited in a scratch catalog
changes the result without recompiling) and `catalog_reference_values_tests.rs` (every value of §3 as authored).

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
| WP1a | `sim_types`: `EffectNode` and its variant structs (D-1), `TargetFilter` (with `structures: Only`, `without_status`), `EffectContext`, `EffectTarget`, `StatusSpec` with effect references (D-20), `AbilityEffectSpec::Effect`, the `AbilitySpec` fields (D-6, including `origin`), `WeaponSpec::melee`, `StatusKind` variants, `SpellMissileView`; serde defaults. `sim_core`: loading of `statuses.ron` and `ability_effects.ron` (both shipped empty), flattening into the arena with `EffectFacts` (D-18), every D-15/D-17 validation as a typed `CatalogError` with its unit test (§11). `sim_fixtures` embedding of both files | bit-identical | M |
| WP1b | Hashed state (D-13 item 1): `SimWorld::statuses`, `cast_lock_ticks`, `Entity::displacement`, `persistents`, `effect_missiles`, ids, `WeaponSpec::melee`, the `EffectContext` encoding, all empty or false; hazard zones untouched; schema bump | goldens re-recorded with the schema as the only cause, render snapshots byte-identical; matrix bit-identical | S |
| WP1c | **Effect-tree engine:** the iterative evaluator and the target contexts (D-15), the evaluation order and the phase 2 sub-steps (D-16), every node of D-1 including `ModifyVitals` and `require`, the status engine (D-4, D-5, D-20), spell damage and responses (D-9), the storage of D-18 with its `PERFORMANCE RATIONALE`, and the presentation mapping (D-19, unhashed). Its own unit, property and allocation tests (§11). Exercised by scratch catalogs and archetypes only; casts reach it through the existing immediate path | bit-identical (no shipped ability has an `Effect`) | L |
| WP1d | Cast pipeline (D-7): approach, edge range, payment at cast, backswing lock, single caster, target vision, `origin` placement (D-6), `would_accept_cast_order` | bit-identical (mode toggles unchanged, `would_admit_cast` agreement test green) | M |
| WP1e | Client generic cast: card entries, `CommandMode` cast modes, locks, submission, targeting ring from `targeting_radius` and tether indicators (D-11, D-19), `battle_cast_tests.rs` on a scratch caster | client tests; no simulation change | M |
| WP2a | Storm tests (§11) against the shipped catalog values | failing | S |
| WP2b | Storm data: `cerebral_elder` ability, the Storm tree in `ability_effects.ron` and the `psionic_storm` status (§5), upgrade unlock; battle 13 to the real High Templar analogue and the 5.0.14 storm (B-24); battles 36a/36b. Separate commit before it: hazard zones removed (D-12, D-13 item 2), their tests rewritten, `Battle::hazard` dropped, schema bump | hazard commit: goldens re-recorded for the schema only, render snapshots byte-identical. Data commit: 13 changes (recorded cause: real storm values), 36a/b new; every other battle and battle 33 bit-identical; goldens unchanged | M |
| WP2c | Storm presentation: effect entry and review (D-11, Q0080); input test with the shipped caster | review verdict recorded | M |
| WP3a/b/c | Abduct: tests, data (tree, `abducted`, `origin`, `target_filter`; + battles 37a/b), presentation (tether effect) | 37a/b new; rest bit-identical | M |
| WP4a/b/c | Fungal Growth: tests, data (tree, two statuses; + 38a/b), presentation (missile, status) | 38a/b new; rest bit-identical | M |
| WP5a | Melee audit: `melee: true` on every catalog weapon whose game weapon has `Options Melee`, cited | bit-identical (no rule reads it yet; hash change only where a melee spec is hashed, **verify** none) | S |
| WP5b/c/d | Blinding Cloud: tests, data (tree with the two filtered branches, two statuses; + 39a/b), presentation | 39a/b new; rest bit-identical | M |
| WP6 | Throughput: `cargo bench -p sim_core --bench throughput` with a new scenario of 20 live areas and 100 statuses over 400 units must stay ≥ 25 ticks/s (charter §3.1) and within 5 % of `main` on the existing scenarios; `sync_steady_state_tests.rs` and `sync_allocation_count_tests.rs` windows with a live area, tether, missile and statuses | recorded in `DESIGN.md` §5 | S |
| WP7 | Bookkeeping: `DESIGN.md` (Abilities row; register: remove B-23, B-24, B-78, the four spells from B-69 and Psionic Storm from B-68; add SU items still open as B-IDs); plan `2026-11-05-sc2-deviation-fixes` WP18 note; this row to `Done` or `Partly done`; Q0077 moved to `closed/` with its resolution | — | S |

No spell package adds Rust beyond what WP1c built: each one is data plus tests, unless its tests reveal an engine
defect. Such a fix is made in the engine, with an engine test, and never as a spell-specific branch. The order
inside the owner's order is fixed by dependency: WP1 serves all four; WP2 to WP5 follow Storm →
Abduct → Fungal → Blinding Cloud. A package's `c` (presentation) step may slip behind the next spell's `a` and
`b` if the review gate is waiting on Q0080; mechanics never wait on presentation.

---

## 13. Determinism and checksum summary

- New state is fixed-point (`SimScalar`, `SimVec2`, `Tick`), in sorted containers (`Vec` sorted by id), iterated in
  id order; no float enters (catalog floats convert through `SimScalar::from_f32_lossy` at load as today, C7).
- No new random draw. Pulse times are pure functions of the exact period (D-4).
- Walks are depth-first in authored order. Instances step in id order, statuses in `(EntityId, StatusId)` order,
  and search candidates in `(distance², EntityId)` order (D-16). No order depends on addresses or floats.
- `EffectId`s and `StatusId`s are assigned in name order at load, so they are identical on every platform for
  the same catalog.
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

| SU-8 | Whose context a refreshed status keeps when a second caster refreshes it (kill credit, responses) | the first applier's (D-5) | WP2a research (`CBehaviorBuff` refresh semantics) |
| SU-9 | Whether a `CEffectSet` finishes each child's synchronous subtree before its next child (depth-first), and how `CEffectEnumArea` orders its hits | depth-first; hits ordered by distance, then id (D-16) | WP1c research; editor documentation |
| SU-10 | Fungal's `FungalGrowthSearchDummy` is its own area of radius 2.25 with its own filters. Whether its responses reach units the enemy-only search skips | modelled inside the search, so only enemies respond (§5.2) | WP4a research (`CEffectDamage` default search filters) |

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
- **A general engine is harder to get right than four mechanisms.** Mitigations:
  - WP1c lands the engine alone, with per-node unit tests, property tests and an allocation test, before any
    spell data exists;
  - the D-17 validation rejects shapes the engine does not support, at load;
  - each spell package is data, so a wrong tree is a catalog fix, not code.
- **Abstraction drift.** Nodes could grow spell-specific flags. Mitigation: a new field or node needs an SC2
  editor equivalent, cited as in D-1. §5.4 lists the known extensions in advance.
- **Context mistakes** (a node acting on the wrong unit). Mitigation: the static target-kind check of D-15 at
  load, and the context tests of §11.
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

- Building the §5.4 examples and their field extensions (`Damage.shield_bonus`, `amount_from_target_vital`,
  filter predicates for energy, life and attributes, the channel capability, `Persistent.expire`); periodic
  offsets; `Displace` destinations other than `Origin`; `Switch`, `CreateUnit`, `IssueOrder` and `EnumTransport`
  nodes.
- Other caster spells: Feedback, Psionic Storm's later patches, Consume, Parasitic Bomb, Neural Parasite,
  Microbial Shroud, Snipe, EMP, Guardian Shield, Force Field, Liberator zones, Graviton Beam (B-69, B-70 remain).
- Autocast, AI spell use, smart-cast nuances beyond D-7.6.
- Heroic units (no catalog unit is heroic) and the Mothership exclusion.
- Bunker passengers under Blinding Cloud (no bunker analogue).
- Radar (`RadarArc`) under Blinding Cloud (no radar mechanic).
- The energy regeneration delay (B-35, WP15 of the deviation plan) and moving the concussive slow (B-77) onto
  statuses, which the status engine makes possible as a follow-up.
- Effect art beyond what each spell's `c` step submits to the review gate (Q0080).
