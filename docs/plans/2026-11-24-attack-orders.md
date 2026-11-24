# Blueprint: The A Command by Stance; Scan Move for Unarmed Units; Attacks on Friendly Units (B-174, B-175, B-186)

This plan closes the two rows that plan [`2026-11-23-stance-checked-orders`](2026-11-23-stance-checked-orders.md)
deferred in its D-8, and one wider row found while grounding them:

- **B-174: the unburrowed Lurker analogue has no Attack.** 5.0.14's unburrowed Lurker carries the `attack` ability
  without a weapon (S7). Here the card shows no Attack for it, client admission drops it from an `AttackMove`, and the
  unburrowing Lurker is refused `AttackTarget` in its settled stance. Each stance now *authors* its A command apart
  from its weapons (`StanceOrders::attack`, D-1). An attack-mover with no weapon walks to the point and acquires
  nothing (D-3, §2.2 rule 2). The simulation already does this.
- **B-186 (new, provisional number): unarmed mobile units are dropped from an A-move.** In 5.0.14 every unarmed
  mobile unit puts Scan Move (`move,AcquireMove`) in the Attack slot of its card (S6). So Medivacs, Ravens,
  Observers, Warp Prisms, Infestors, Vipers and Overlords walk with an army that is A-moved. Here client admission
  (`mobile && armed`) drops all eight analogues. Without this row, B-174's fix would cover one unit and leave seven
  behind. The same field covers them, so this plan fixes both together.
- **B-175: an A-click on one's own unit.** 5.0.14's attack ability leaves out own and allied units only in automatic
  acquisition and the smart (right) click (S1). Its command has no alliance filter (S1, S4). Ordinary weapons'
  target filters do not exclude own units, and the few weapons that must not hit them exclude them explicitly (S3).
  So an A-click on one's own unit attacks it. Here the click becomes an attack-move to the ground under the unit.
  The data reading is strong, but no in-game source was found (**verify**). This plan implements it as the default
  of draft question QA (§15), and the client change is one branch that is easy to reverse.

The simulation's `issue_plain_order` now admits `AttackTarget` per unit, by the unit's settled stance and weapons
(D-3). That is the authoritative half of both rows, which the previous plan's §14 left to B-174. No hashed field,
encoding or schema changes. Goldens and the sandbox checksum are expected unchanged (§5). No measured world is
expected to move (§7).

A full plan is needed (charter §6): this is a new authored catalog field, it changes simulation admission, and it
spans `sim_types`, `sim_core`, `assets/catalog`, `sim_fixtures` and `client`.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-24 against `main` at
`36b914fd`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.
- B-numbers B-186 and B-187 are provisional. The next free numbers at WP0's commit replace them, and every mention
  in this plan is renumbered in that commit.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-1: RON data instead of editor data) | Closed | D-1 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §14 |
| Q0098: an anchored unit's Attack on the ground is refused (answered (a)) | Closed | D-4, D-9 |
| The owner's guidance: no fitting to fixed seed sets; acceptance by principled properties | Followed | §6 |
| **QA = Q0104: does an A-click on one's own unit attack it, and may a unit attack itself?** | Filed (see §15) | D-3, D-5; default (a): attack it, never itself |
| **QB = Q0105: what does a unit do when it is A-clicked onto a unit it cannot attack?** | Filed (see §15) | D-5; default (a): it attack-moves to the point under the unit |

---

## 1. Summary

1. **Data (D-1).**
   - `sim_types::StanceOrders` gains `attack: AttackCommand`, where `AttackCommand` is `None`, `ScanMove` or
     `Attack`. The two shipped modes author `attack: Attack` (S10).
   - The default stance stays derived: `Attack` when armed, `ScanMove` when mobile and unarmed, `None` otherwise.
     An archetype may author the whole default set instead, with the new optional unit field
     `default_stance_orders`. The Lurker analogue does, with `attack: Attack` (S7).
   - Catalog validation refuses `ScanMove` in a stance without `hold_position`. Both are commands of the `move`
     ability (S5).
2. **One rule for "can attack this unit" (D-2).** `Entity::can_attack(archetype, target, target_archetype)` holds
   when all of these are true:
   - the unit's settled stance has `Attack`;
   - the target is not the unit itself;
   - the settled stance has a weapon against the target's plane, and its `target_flags` meet the target's
     attributes.

   The target's owner is not read. The simulation's admission and the client's `can_execute` both call it.
3. **The simulation (D-3, D-7).**
   - `issue_plain_order` gives `AttackTarget` only to the units for which `can_attack` holds (`ToSelection`).
   - A scenario's attack-move reaches every mobile, non-harvesting unit whose settled stance has an A command,
     instead of only armed ones.
4. **The client (D-4 to D-6).**
   - The card's A-slot entry appears for every unit with an A command. It reads "Attack", or "Scan Move" when no
     selected unit has `Attack`.
   - Admission gives `AttackMove` to every mobile member with an A command.
   - An A-click on any unit, one's own included, sends `AttackTarget` to the members that can attack it, and
     `AttackMove` to the point under it to every other member that has an A command.
5. **Records (D-8).**
   - Fixed: B-174, B-175 and B-186.
   - B-187 registered and deferred: a scan move aimed at a unit follows it in 5.0.14. That needs the follow order
     B-163 also needs.
   - B-176's `AttackMove` half has a package prepared for after the contact solver merges (WP5, D-9).
6. **Worlds (§7).** Every measured output is predicted byte-identical. The checksum schema stays 40.

---

## 2. What StarCraft II does

### 2.1 Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data. The
override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. S10 and
S11 are earlier plans' sources, re-read for this one.

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | **The attack ability's only alliance filters are for acquisition and the smart click.** `AcquireFilters` excludes `Player, Ally, Neutral, Dead, Hidden, Invulnerable, Benign`; `SmartFilters` excludes `Player, Ally`. `CAbilAttack` has no `TargetFilters` of its own; `Alignment Negative` | `CAbilAttack default` | `core` AbilData:115–139 (:117–119) |
| S2 | Its `Execute`, `Towards` and `Barrage` commands go to every selected unit that has them | `Flags index="ToSelection"` | `core` AbilData:127–135 |
| S3 | **Weapons accept own units unless they say otherwise.** Default `TargetFilters` `Visible;-`. Ordinary weapons exclude `Missile, Stasis, Dead, Hidden, Invulnerable` and not `Player` or `Ally`. The weapons that must not hit their own side exclude it by name: `PointDefenseLaser` (`Self, Player, Ally, Neutral`), `BypassArmorDroneWeapon` (`Player, Ally, Neutral`), `WidowMineDummy` (`Ally`), `TwinIbiksCannon` and `BuildingShield` (`Self`) | `CWeapon default`; `TargetFilters` | `core` WeaponData:3–31 (:29); `liberty` WeaponData:9, :19, :35 (ordinary), :71, :136; `swarm` WeaponData:8, :76; `voidmulti` WeaponData:267; `balancemulti` WeaponData:240 |
| S4 | An attack ability that must not take friendly clicks says so in its acquire and smart filters: `AttackWarpPrism` excludes `Self, Player, Ally, Neutral, Enemy` in both, and turns off `ToSelection` | `CAbilAttack id="AttackWarpPrism"` | `liberty` AbilData:1100–1110 |
| S5 | **The move ability carries five commands: `Move`, `Patrol`, `HoldPos`, `AcquireMove` and `Turn`, each `ToSelection`.** `AcquireMove` (face `AcquireMove`, Scan Move) is a move that acquires targets | `CAbilMove default` | `core` AbilData:533–561 (:555–557) |
| S6 | **Every unarmed mobile unit's card puts two buttons in the Attack slot (row 0, column 4): `attack,Execute` (face `Attack`) and then `move,AcquireMove` (face `AcquireMove`).** These units carry `move` and no `attack`, so the slot is Scan Move | `LayoutButtons … Row="0" Column="4"`; `AbilArray` | Medivac `liberty` UnitData:5475–5478, :5485–5486; Observer :3443–3445, :3452–3453; Overlord :7391–7395, :7400–7401; Overseer :7520–7521; Raven :5621–5622 (`voidmulti` :2653–2654, `balancemulti` :2490–2491); Infestor :10108–10109 (`balancemulti` :740–741); Warp Prism :3859–3860; Viper `swarm` UnitData:2363–2364; Swarm Host `swarm` :1214–1215 |
| S7 | **The unburrowed Lurker has the same pair, and 5.0.14 adds `attack` to it, so its slot is Attack although it has no weapon** | `AbilArray`; `LayoutButtons` | `liberty` UnitData:727–729, :735–736; `voidmulti` UnitData:974; `balancemulti` UnitData:909 |
| S8 | **Every unit with a weapon carries an attack ability** (`attack`, `BattlecruiserAttack`, `attackProtossBuilding`). The units with an attack ability and no weapon are the unburrowed Lurker, the Adept's shade, the Nexus and overcharged Pylon (under `PurifyNexusRequirements`), the Bunker (a redirect) and three campaign or flyover units | `AbilArray`, `WeaponArray`, joined over all eight layers | Computed (§16); the shade `void` UnitData:3147–3152 |
| S9 | The burrowed Roach changes its slot from Attack to Scan Move in LotV: it has no weapon while burrowed | `LayoutButtons` | `liberty` UnitData:7284; `voidmulti` :1472; `balancemulti` :1383 |
| S10 | The sieged Siege Tank and the burrowed Lurker carry `attack` | `AbilArray` | `liberty` UnitData:5172–5174, :793–796 (stance-checked-orders S3, S5) |
| S11 | The Lurker's burrow and unburrow change the ability set at the start of the switch | no `Abils` section | `swarm` AbilData:1663–1673 (anchored-commands S13) |

**Web sources.**
- Liquipedia's *Micro (StarCraft)* page describes Attack as "A, then left-click on a target", with no rule about
  friendly units.
- A Blizzard forum answer says "press A then left click on it, this will force attack on the unit/building even if it
  is your own". It is on the StarCraft: Remastered forum (`us.forums.blizzard.com/en/starcraft/t/…/2150`), so it is
  lineage, not 5.0.14 evidence.
- A StarCraft II forum thread on friendly fire (`…/en/sc2/t/which-units-deal-friendly-fire/12103`) covers splash
  only.

No StarCraft II source states the A-click on one's own unit, so B-175 stays **verify** (QA).

### 2.2 The rules that follow

1. **Every mobile stance has an A command (S5–S8).** It is Attack when the stance carries `attack`, and every armed
   stance does (S8, S10). Otherwise it is Scan Move, the `AcquireMove` of the `move` ability. The only unarmed
   stances with Attack in multiplayer are the unburrowed Lurker and the Adept's shade (S7, S8). The Nexus and the
   Bunker are covered by the previous plan's §14. An anchored stance without `move` and without `attack` has no A
   command, as with the unarmed structures.
2. **An attack-mover with no weapon walks.** For a unit with no weapon, both Scan Move and an attack-move on a point
   are a move that acquires nothing. The simulation's `AttackMove` already behaves this way: `weapon_against` finds
   no weapon, the scan has no candidate, and the unit walks under `Engagement::Seek` (§3.3). So one order,
   `AttackMove`, carries both commands on a point. Autocast acquisition (the Medivac's heal) is not modelled here.
3. **An A on the ground reaches every member with an A command (S2, S5),** mobile ones. Q0098 refuses it for
   anchored members.
4. **An A on a unit (S1–S3, S5).**
   - A member with Attack whose weapon can target the unit attacks it.
   - A member with Scan Move gets `AcquireMove` aimed at the unit. As with `Move` on a unit, it follows the unit
     (**verify**). Here there is no follow order (B-163), so it walks to the point under the unit (B-187).
   - A member with Attack that cannot target the unit is not covered by the data: the unarmed Lurker, or a
     ground-only weapon against an air unit. The command validates against the caster's weapons (S1 has no filter
     of its own), so the game may refuse it for that unit or move it. QB decides. The default is to treat it like
     Scan Move, so it walks with the others.
5. **A friendly target (S1, S3, S4).** Only acquisition and the smart click leave out own and allied units. The
   attack command validates through weapon filters that accept them. So an A-click on one's own unit attacks it, and
   a right click on it does not (the client's `friendly_order` is unchanged).
6. **Attacking oneself.** Default weapon filters do not exclude `Self`, and two weapons exclude it by name (S3).
   Whether an ordinary unit can attack itself is therefore open. The default here is no (QA). Every weapon this
   catalog ships would hit at distance zero, and the result has no use.
7. **A unit attacked by its own side does not call for help or answer.** Acquisition and call-for-help leave out the
   attacker's own side (S1's `AcquireFilters`). The simulation already does this: `target_acquisition.rs` skips
   call-for-help when the attacker owns the victim, and the scan skips own units.

---

## 3. Diagnosis (on `36b914fd`, read from the code)

### 3.1 The client

- `capable` (`battle_pipeline/commands.rs`): `AttackMove => mobile && armed` and `AttackTarget => armed`, where
  `armed` is `settled_armed`. An unarmed mobile unit takes no A-move (B-174, B-186).
- `root_entries` (`battle_hud/card.rs`) shows Attack only for a member that is armed in its settled stance. The eight
  unarmed archetypes have no A slot.
- `handle_targeting_pointer` (`battle_input/mod.rs`) has two cases in Attack mode.
  - A click on a unit the viewer does not own sends one `AttackTarget` command to the selection. `submit_selection`
    then drops every member that cannot attack, so a Medivac analogue in an army A-clicked onto an enemy is left
    behind (S6 says it would scan-move).
  - A click on one's own unit falls through to `AttackMove` at the ground under it (B-175).
- `order_subject` admits an `AttackTarget` subject only when `entity.owner != player`. `can_execute` looks up the
  settled replacement weapon itself, which duplicates the rule the simulation applies.

### 3.2 The simulation's `AttackTarget`

- `world/commands.rs::issue_plain_order` checks only Stop and Hold. Every owned recipient takes an `AttackTarget`,
  whether it is unarmed, its weapon cannot reach the target's plane, or the target is the unit itself.
- No path reads the target's owner. `combat_resolution.rs::ordered_target` fires at the front order's target.
  `target_acquisition.rs` keeps an `Explicit` target while `is_entity_targetable` holds, and that check reads
  damageability and concealment, not owner. Call-for-help skips a same-owner attacker (`attacker.owner ==
  victim.owner`). So an `AttackTarget` on an own unit should already be carried out (**verify** in WP2's red run;
  the chase in `movement/locomotion.rs` reads the target's position and, by reading, no owner).
- A scripted or remote command can therefore order an unarmed unit to "attack". It then chases the target forever,
  because `Explicit` with a mobile unit never abandons.

### 3.3 The simulation's `AttackMove` for an unarmed unit

`engagement_of` gives `Seek` and `scan_candidates` skips every neighbour for which `weapon_against` is `None`, so an
unarmed attack-mover walks its route and acquires nothing. Nothing in the simulation needs to change for §2.2
rule 2. Movement admission (`movement/mod.rs::validate_movement_order`) accepts `AttackMove` for any owned live unit,
the stance unchecked (B-176).

### 3.4 Scenarios

`scenario.rs::attack_move_combatants` orders every unit that is mobile, not a harvester, and has a weapon. An unarmed
mobile unit is left out of a scenario's A-move. No shipped scenario attack-moves one:
- `battle_automaton` (showcase, default battle) is `welder_drone`, `pulse_bot` and `rhino_beetle`;
- `showcase_raiders` is all armed;
- `showcase_siege_line` gets `EnterModes` only;
- `skirmish` has no initial orders.

Changing the rule therefore moves no shipped world. Its comment in `showcase_effects.ron` ("an attack-move would
not take them along") is updated.

### 3.5 The catalog

There are eight unarmed archetypes, all mobile, with `weapon`, `ground_weapon` and `air_weapon` all `None`:

| Slug | 5.0.14 analogue | A slot in 5.0.14 |
|---|---|---|
| `repair_shuttle` | Medivac | Scan Move (S6) |
| `omni_probe_drone` | Raven | Scan Move (S6) |
| `stealth_eye_orb` | Observer | Scan Move (S6) |
| `phase_prism_saucer` | Warp Prism | Scan Move (S6) |
| `mycelium_shaman` | Infestor | Scan Move (S6) |
| `tongue_chameleon` | Viper | Scan Move (S6) |
| `amber_pollen_dome` | Overlord | Scan Move (S6) |
| `burrow_scorpion` (default stance) | Lurker | Attack (S7) |

The units' modes are `siege_mode` (`railgun_crawler`) and `burrowed` (`burrow_scorpion`). Both are armed and carry
`attack` (S10). Every armed archetype and stance carries `attack` (S8), so the derived rule of D-1 is exact for every
shipped stance except the Lurker's default, which is authored.

---

## 4. Design decisions

### D-1. The A command is stance data (B-174, B-186)

In `sim_types/src/commands.rs`:

```rust
/// What the A key does for a unit in one stance (StarCraft II's Attack slot, row 0 column 4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttackCommand {
    /// No A command: an unarmed structure, an anchored stance with neither `attack` nor `move`.
    #[default]
    None,
    /// The move ability's `AcquireMove` (Scan Move): an attack-move on a point; never a unit attack.
    ScanMove,
    /// The `attack` ability: an attack-move on a point, or an attack on a unit its weapons can target.
    Attack,
}

pub struct StanceOrders {
    pub stop: bool,
    pub hold_position: bool,
    /// The stance's A command (plan `2026-11-24-attack-orders` D-1).
    pub attack: AttackCommand,
}
```

- **The field is required in RON**, as `stop` and `hold_position` are. The two authored mode sets in `units.ron`
  (`siege_mode` and `burrowed`) gain `attack: Attack`, with their `AbilArray` citations. `StanceOrders::ALL` gains
  `attack: Attack` (the mobile armed default), and `NONE` gains `attack: None`.
- **The default stance stays derived.** `UnitArchetype::stance_orders(None)` returns
  `default_stance_orders.unwrap_or(derived)`, where `derived` is:

  ```rust
  StanceOrders {
      stop: mobile || armed,
      hold_position: mobile,
      attack: if armed { Attack } else if mobile { ScanMove } else { None },
  }
  ```

  `armed` is `stance_armed(None)`. The doc cites S6 to S8.
- **`default_stance_orders: Option<StanceOrders>`** is a new optional field (`#[serde(default)]`) of
  `UnitArchetypeDef` (units only) and of `UnitArchetype`. Structures always derive and set `None`; no structure
  breaks the rule (stance-checked-orders P-4). The Lurker analogue's row authors
  `default_stance_orders: Some((stop: true, hold_position: true, attack: Attack))`. A comment cites S7 and the
  shared `stop` and `move` (`liberty` UnitData:727–728). This is the "authored default stance" that the previous
  plan's D-2 named as the way out.
- **Validation.** `CatalogError` gains a typed variant: a stance that authors `attack: ScanMove` without
  `hold_position: true` is refused at load, because both are `move` commands (S5). Every shipped stance passes. P-8
  tests the refusal. No other combination is constrained. An `Attack` stance without a weapon is the Lurker's case
  and is legal.
- **Not hashed.** The catalog is not part of the checksum (anchored-commands §3.2).

### D-2. One rule: "can this unit attack that unit" (B-174, B-175)

In `sim_core`, next to `stance_armed` and `settled_armed`:

```rust
impl UnitArchetype {
    /// The weapon a unit of this archetype uses in `stance` against a target of `target_mover`:
    /// the mode's replacement ground weapon when it sets one, otherwise the archetype's, by
    /// `weapon_against` (the rule `apply_mode_transition` applies to a live unit).
    pub fn stance_weapon_against(&self, stance: Option<&UnitModeSpec>, target_mover: MoverType)
        -> Option<&WeaponSpec>;
}

impl Entity {
    /// The A command of the stance this unit is in or is switching to.
    pub fn settled_attack_command(&self, archetype: &UnitArchetype) -> AttackCommand;

    /// Whether an attack order on `target` reaches this unit (StarCraft II `ToSelection`):
    /// its settled stance has `Attack`, `target` is another unit, and the settled stance's
    /// weapon against `target`'s plane exists and its `target_flags` meet `target`'s
    /// attributes. The target's owner is not read (5.0.14 `core` AbilData:117-119, B-175).
    pub fn can_attack(&self, archetype: &UnitArchetype, target: &Entity,
        target_archetype: &UnitArchetype) -> bool;
}
```

All three are pure reads and allocate nothing. They live in `catalog.rs` and `entity.rs`, outside `movement/`. The
client's `can_execute` drops its own replacement-weapon lookup and calls `can_attack`. That is one rule, used in two
places.

`target_flags` is 255 for every shipped weapon, so the attribute half changes nothing today. It is kept because
acquisition (`scan_candidates`) applies it, and an explicit attack that ignored it would be a second rule.

### D-3. The simulation admits `AttackTarget` per unit (B-174, B-175)

`issue_plain_order`'s `takes` gains an arm:

```rust
UnitOrder::AttackTarget { target, .. } => target_snapshot.is_some_and(|(target, target_arch)|
    arch.is_some_and(|arch| ent.can_attack(arch, target, target_arch))),
```

- The target and its archetype are looked up **before** the recipient loop. The target must exist and be alive
  (`is_alive`). The borrow is shared, then the recipient is borrowed mutably, as `dispatch_cast` does. A target
  that is gone refuses the command for every recipient. That is not a failure, so no diagnostic is raised, as with
  Stop and Hold (stance-checked-orders D-1).
- Visibility and concealment are not admission. They stay with acquisition, which drops an untargetable `Explicit`
  target as it does today. That matches the client, whose `order_subject` already requires the target to be
  available for combat.
- **The owner is not read**, so an own or allied unit is attacked (§2.2 rule 5). The unit itself is not (rule 6,
  QA).
- Queued and immediate orders are treated alike, as with Stop and Hold.
- A unit refused the order is left byte for byte as it was. The others get the order as today.

### D-4. Client admission and the card (B-174, B-186)

- `capable`:
  - `AttackMove { .. } => mobile && entity.settled_attack_command(archetype) != AttackCommand::None`. Q0098's
    refusal of anchored members is unchanged: `mobile` already reads the settled stance.
  - `AttackTarget { .. } => entity.settled_attack_command(archetype) == AttackCommand::Attack`. `can_execute` adds
    the weapon and self check through `can_attack`.
  - The comments cite S6 to S8 and this plan.
- `order_subject`: an `AttackTarget`'s subject is any unit available for combat (`is_entity_available_for_combat`).
  The `owner != player` filter goes (B-175). The context order (right click) never builds an `AttackTarget` on an
  own unit (`friendly_order`), so the smart click is unchanged (S1's `SmartFilters`).
- `any_attacker`: a member whose settled A command is `Attack`. For every shipped unit this is today's armed member,
  plus the unburrowed and unburrowing Lurker analogue.
- **The card.** `ArchetypeCommands`' `default_orders` and `mode_orders` already carry `StanceOrders`, so they carry
  `attack` too. `root_entries` emits the A-slot entry when any completed, non-foundation member's settled set has
  `attack != None`, read through the existing settled-mode helper.
  - The label is `CardLabel::Attack` when some such member has `Attack`, and the new `CardLabel::ScanMove`
    ("Scan Move") otherwise (S6's face).
  - Both open `CommandMode::AttackMove` with the same key.
  - `default_armed`, `mode_armed` and `settled_armed` in `card.rs` lose their last reader, and the implementer
    removes them unless a test still needs them.
- **Slot shifts (R-2).** The eight unarmed archetypes gain the A slot as their first root entry, which moves their
  later entries one slot and one key. For example, the Infestor and Viper analogues' casts move. That matches the
  data, where the slot is column 4 of the top row on their cards (S6). The card orders entries and has no
  coordinates (stance-checked-orders §14).

### D-5. The A-click on a unit (B-175, §2.2 rule 4)

`handle_targeting_pointer`, in `CommandMode::AttackMove`: the picked unit is any unit `view.pick_unit` returns, with
the same fog rules as today, **including one's own**. The order is built by a new
`CommandQueue::submit_attack_on_unit(world, player, selection, target, point, queued)` in `battle_pipeline/commands.rs`:

1. **Attackers:** the members for which `can_execute(AttackTarget { target, Explicit })` holds.
2. **Walkers:** every other member, except the target itself, for which `capable(AttackMove { point })` holds.
   These are the Scan Move members, plus (by QB's default) members with `Attack` that cannot attack this target.
   `point` is the ground under the unit.
3. One `AttackTarget` command for the attackers, then one `AttackMove` command for the walkers. Either is left out
   when empty. Both empty gives `SubmissionError::EmptySelection`, and the mode stays armed (Q0098's rule).
4. The marker is `Attack` on the unit, as today.

The two commands go into the same tick, in that order, with disjoint recipients, so the order between them does not
matter in the simulation. Their sequences are returned like a single submission's. The implementer picks the shape:
a small two-element return, or two `submit` calls whose outcomes both reach the status line. The partition allocates
two `Vec`s once per click, not per frame. Clicks are events, so the zero-steady-state-allocation rule for sync
systems does not apply.

The members' unit-target A command in 5.0.14 has no counterpart here when the member scan-moves (B-187, deferred).

### D-6. Scenario attack-moves follow the A command (B-186)

`scenario.rs::attack_move_combatants` orders every unit of the slot that meets all of these:
- not `is_immobile`;
- not a harvester (unchanged; a scenario choice, documented);
- its settled stance's A command is not `None`.

The function's doc and `apply_initial_orders`' doc say "has an A command (plan 2026-11-24-attack-orders D-6)"
instead of "has a weapon". `ScenarioError::NoCombatants` is unchanged. No shipped scenario changes outcome (§3.4,
§7). `showcase_effects.ron`'s comment is reworded: player 0 is given no attack-move, so its burrowing units stay in
the line.

### D-7. What does not change in the simulation

`AttackMove` for an unarmed unit (§3.3). The `Explicit` target rules. Call-for-help (already same-owner-exempt).
`attack_dash.rs`'s same-owner exemption: a dash such as Charge does not start toward one's own unit. Whether 5.0.14's
Charge validator agrees is **verify**, out of scope (§14). Splash friendly fire.

### D-8. Records

- **B-174:** fixed. WP1 (data) and WP3 (client), with the simulation half in WP2.
- **B-175:** fixed by WP2 (simulation) and WP3 (client), as the default of QA. If QA is answered (b), WP3's own-unit
  branch is reverted and the simulation keeps its rule, because a refusal by the client is enough.
- **B-186 (new, fixed here):** "Unarmed mobile units are dropped from an A-move. 5.0.14 puts Scan Move
  (`move,AcquireMove`) in their Attack slot (`liberty` UnitData:5485–5486 and S6), so Medivacs, Ravens, Observers,
  Warp Prisms, Infestors, Vipers and Overlords walk with an A-moved army. Here client admission and scenario
  attack-moves leave them out."
- **B-187 (new, deferred):** "A Scan Move aimed at a unit, and (by QB's default) an attack by a unit that cannot
  attack the clicked unit, go to the point under it. 5.0.14's `AcquireMove` on a unit follows it (**verify**). This
  needs the follow order B-163 needs, a movement change."
- **B-176:** unchanged except for a note that its `AttackMove` half is prepared as WP5 of this plan (D-9).

### D-9. After the contact solver merges: movement admission by A command (WP5, B-176's `AttackMove` half)

`movement/mod.rs::validate_movement_order` admits a command atomically. Under `ToSelection` (S2, S5), an
`AttackMove` should reach only the recipients that are mobile in their settled stance (`immobile_after_channel` is
false) and whose settled A command is not `None`. The others are filtered out before admission rather than refusing
the command. A command left with no recipient is valid and does nothing.

That is a change in the code the contact-solver branch owns, so it waits for that merge. No shipped client path
sends such a command after WP3. Fixtures and scenarios do not either: P-9 extends the fixture check to `AttackMove`
recipients. The `Move` half of B-176 stays with B-176's own plan, since it depends on B-96. If B-176's plan comes
first, WP5 folds into it.

---

## 5. Determinism and checksum

- **No hashed field, encoding or catalog hashing changes.** `AttackCommand` and `default_stance_orders` are
  catalog data, and the queries are pure reads. **The checksum schema stays 40.** No "next free at merge" number is
  taken (41 and 42 are reserved elsewhere).
- **Fixed point only.** The new code compares enums, `Option`s and `AttributeFlags`. No float enters, so C7 is
  unaffected.
- **Goldens.** `golden_replay.ron` and `golden_replay_orders.ron` script no `AttackTarget`. Their `AttackMove`s go
  to armed squads (`pulse_bot`, `rhino_beetle`, `psi_grey`, `railgun_crawler`, never sieged), which D-4 and D-6
  leave as they are. Both goldens and `DEFAULT_SANDBOX_CHECKSUM` are expected unchanged on native debug and release,
  Windows, macOS ARM64 and `wasm32-wasip1`.
- **Matrix fixtures.** `AttackFirstEnemy` (`12c_viking_vs_colossus`, `20_brood_lord_vs_roach`) gives one armed unit
  an attack on an enemy its weapon reaches: the Viking analogue's air weapon against a cliff walker, and the Brood
  Lord analogue's ground weapon against a ground unit. Both pass D-3. The crowd harness (`sim_fixtures/src/harness`)
  attacks a `Pillar` of owner 1 with owner 0's `Attack` role units. These are armed (**verify** in WP2's grep).
- **If a golden or the sandbox checksum moves, that is a defect of the package** (§8 rule 1), traced to the command
  that a unit now refuses. It is not a re-record.
- **Throughput.** One target lookup per `AttackTarget` command and one `can_attack` per recipient. No per-tick step
  changes.

---

## 6. Acceptance (principled properties, fixed before implementation)

Tests find units by property: unarmed and mobile; armed only in a mode; authored default stance; a ground-only
weapon against an air unit. Production code uses no slug (C5).

- **P-1 (derivation against the data).** For every shipped archetype, `stance_orders(None).attack` is `Attack` when
  armed, `ScanMove` when mobile and unarmed, and `None` otherwise, unless `default_stance_orders` is authored, and
  only the Lurker analogue authors it.
  - `catalog_reference_values_tests` pins a table read from §2.1, keyed by `sc2_inspiration` (test data). The
    Medivac, Raven, Observer, Warp Prism, Infestor, Viper and Overlord analogues give `ScanMove` (S6). The Lurker
    analogue gives `Attack` (S7). The two modes give `Attack` (S10). The previous plan's armed and unarmed
    structures give `Attack` and `None`. Each row carries its citation.
- **P-2 (simulation admission, exhaustive).** For every shipped archetype, in each stance and each direction of
  every switch while it channels, given `AttackTarget` immediately and queued, against:
  - an enemy ground unit;
  - an enemy air unit;
  - an own unit;
  - itself.

  The order reached the unit exactly when `can_attack` holds. A unit it did not reach is equal, field for field, to
  the same unit in an untouched world (the previous plan's three-world method, As built item 2).
- **P-3 (`ToSelection`).** One `AttackTarget` command naming an armed ground-only unit and an unarmed unit, aimed at
  an enemy air unit, reaches neither. Aimed at an enemy ground unit, it reaches the armed one only.
- **P-4 (an own target is attacked).** An armed unit given `AttackTarget` on an own unit of the same player in reach
  damages it within one weapon period plus its damage point. The victim acquires no target and calls no help. A
  mobile attacker out of reach closes in and attacks.
  - If the chase does not close in, that is a finding under `movement/`. §8 rule 4 applies: the reach-only case
    stays, and the chase moves to WP5.
- **P-5 (an unarmed attack-mover walks).** An unarmed mobile unit given `AttackMove` across enemies arrives at the
  point without acquiring (`current_target` stays `None`). An armed unit beside it acquires. This pins §3.3, the
  rule B-174's fix relies on.
- **P-6 (client = simulation).** For every selection in the stance-checked-orders P-3 set, extended with every
  unarmed archetype and the unburrowing Lurker analogue:
  - the card shows the A slot exactly when `any_recipient` holds for an `AttackMove` on a reachable point, or
    `any_attacker` holds;
  - its label is Attack exactly when `any_attacker` holds;
  - for `AttackTarget`, client admission of each member equals the simulation's delivery (P-2's method).
- **P-7 (through input).**
  - A Medivac-class unit alone shows Scan Move. `A` then a ground click attack-moves it, and an enemy click
    attack-moves it to the point under the enemy.
  - The unburrowed Lurker analogue shows Attack and A-moves on the ground.
  - A mixed army (marine-class, Medivac-class, unburrowed Lurker analogue) A-clicked on the ground sends one
    `AttackMove` naming all three.
  - A-clicked on an enemy ground unit, it sends `AttackTarget` to the marine-class unit and `AttackMove` to the other
    two.
  - A-clicked on one's own unit, armed members attack it (an `Explicit` target after the tick) and the rest
    attack-move to it. The clicked unit, if selected, gets neither order.
  - A right click on one's own unit is unchanged.
- **P-8 (zero recompilation, C6; validation).** In a catalog edited at run time:
  - removing the Lurker analogue's `default_stance_orders` makes it Scan Move;
  - setting a Medivac-class row's `default_stance_orders` to `attack: None` removes its A slot and its A-move
    admission;
  - `siege_mode` with `attack: None` refuses `AttackTarget` to a sieged tank in the simulation.

  A stance with `attack: ScanMove, hold_position: false` fails to load with the typed error. All of these run
  without a rebuild.
- **P-9 (fixtures honest).** The previous plan's first-tick check is extended. Every unit named in a first-tick
  `AttackTarget` of a matrix battle ends tick 1 with that `Explicit` target, and every unit named in an `AttackMove`
  has an A command in its settled stance. P-9 already exists; this adds its two order kinds.
- **P-10 (scenarios).** `attack_move_combatants` orders an unarmed mobile unit placed on an attack-moving side, and
  still leaves out harvesters and anchored units (a unit test with a small world).
- **P-11 (worlds).** §7's table holds byte for byte.
- **P-12 (no identity, no allocation).** No slug or mode name appears in the production changes (C5). The warm-card
  allocation tests are extended with a Medivac-class unit and the unburrowed Lurker analogue.

---

## 7. Worlds that move

**Commands.** These are the stance-checked-orders plan's, with outputs under `~/scratch/attack-orders/`. Copy
`base.sh`, `measure.sh`, `compare.sh` and `client.sh` from `~/scratch/stance-orders/scripts/` and change the worktree
and target paths in each. `measure.sh` already passes `--assets "$R/assets"` to `route_trace`. Keep that, because
`route_trace`'s default reads the assets of the tree it was compiled in. `matchup_trace` embeds the matrix fixture
and the catalog, so it is rebuilt for each tree.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
R=~/scratch/attack-orders
$R/scripts/base.sh <wp>        # release matchup_trace and route_trace, then measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

Run these in the background, one at a time across worktrees (charter §7). Check `quota -s` and `df -h ~/scratch`
first.

| World | WP1 (data, queries) | WP2 (simulation admission, scenarios) | WP3 (client) |
|---|---|---|---|
| Matrix, seeds of record and 16 (`matrix.csv`, `matrix.md`) | byte-identical (nothing reads the new field in the simulation) | byte-identical (§5: both `AttackFirstEnemy` attackers pass D-3) | not run |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical | byte-identical | not run |
| Showcase and showcase effects, default seed and 1–8 | byte-identical | byte-identical (§3.4: no attack-moving side has an unarmed unit) | not run |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | unchanged (§5) | unchanged |
| Client skirmish stage log (`skirmish_match_tests`) | identical | identical | identical: the pilot's loadout (`standard_automaton`, welder drones) trains armed units only (**verify** what the pilot trains; an unarmed unit it A-moves would now walk along, which §8 rule 3 traces) |

---

## 8. Stop rules

1. **No golden or sandbox checksum moves in any package** (§5). A move is traced to its command and stops the
   package. It is not a re-record.
2. **WP1 and WP2 change no measured world** (§7). A difference is traced to the first differing tick and unit, and
   stops the package. If the cause is a fixture order that a unit now refuses, the fixture is honest-checked (P-9)
   and the case goes to the orchestrator with the trace. It is not fitted.
3. **WP3 changes no stage log line.** If the skirmish pilot A-moves an unarmed unit, its walking along is the fix.
   The difference is recorded with the unit and tick, and the orchestrator decides. Any other cause stops WP3.
4. **No existing test is loosened.** A test that fails because it expected an unarmed unit to be dropped from an
   A-move, or an own-unit A-click to attack-move, or a card without the A slot, is rewritten from P-1 to P-8 and
   listed in the As built. A failure that shows the simulation does not carry out an attack on an own unit (P-4) is
   a finding.
   - If it is in `movement/`, the case moves to WP5 and WP2 keeps the reach-only test.
   - If it is outside `movement/`, it is fixed in WP2 and listed.

   Any other failure stops the package.
5. **No unit identity in code** and no tuning constant. Every rule is the data's (§2.1) or an existing engine rule.

---

## 9. Client consequences

- `battle_pipeline/commands.rs`:
  - `capable`'s `AttackMove` and `AttackTarget` arms, `can_execute` through `can_attack`, `order_subject` without
    the owner filter, and `any_attacker` (D-4);
  - `submit_attack_on_unit` (D-5).
- `battle_hud/catalog.rs`: nothing new. `StanceOrders` carries `attack`. `default_armed` and `mode_armed` are
  removed if unread (D-4).
- `battle_hud/card.rs`: the A-slot entry by settled `attack`, and `CardLabel::ScanMove` with text "Scan Move" (D-4).
- `battle_input/mod.rs`: `handle_targeting_pointer` picks any unit and calls `submit_attack_on_unit` (D-5). Its
  comment about B-175 is replaced.
- `battle_input/command_mode.rs`: doc of `AttackMove` (Attack or Scan Move). `reconcile` is unchanged in code and
  follows `capable`.
- No `PlayerView` change. No GPU or windowed check is needed: the entry draws text only (`card.rs`'s label match).

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP4 | §4: B-174 and B-175 point here (WP0) and are fixed (WP4); B-186, B-187 registered (WP0), B-186 fixed (WP4); B-176's note; the HUD row (WP4) |
| `docs/plans/README.md` | plan, WP4 | Row |
| `docs/evidence/2026-11-24-attack-orders.md` (new) | WP0 to WP4 | §7, red runs, §2.1's sources, the S8 join's output |
| `crates/sim_types/src/commands.rs` | WP1 | `AttackCommand`, `StanceOrders::attack`, `ALL`, `NONE` (D-1) |
| `crates/sim_core/src/catalog.rs` | WP1 | `default_stance_orders` (def and archetype), `stance_orders(None)`, `stance_weapon_against`, the validation variant; unit tests |
| `crates/sim_core/src/entity.rs` | WP1 | `settled_attack_command`, `can_attack` (D-2) |
| `assets/catalog/units.ron` | WP1 | `attack: Attack` in the two mode sets; the Lurker analogue's `default_stance_orders` |
| `crates/sim_core/tests/catalog_reference_values_tests.rs` | WP1 | P-1 |
| `crates/sim_core/tests/runtime_catalog_tests.rs` (or the test that injects RON) | WP1, WP2 | P-8 |
| `crates/sim_core/src/world/commands.rs` | WP2 | `issue_plain_order` (D-3) and its doc |
| `crates/sim_core/src/scenario.rs` | WP2 | `attack_move_combatants` and the docs (D-6); P-10 |
| `assets/scenarios/showcase_effects.ron` | WP2 | Comment only |
| `crates/sim_core/tests/stance_orders_tests.rs` | WP2 | P-2, P-3, P-5 |
| `crates/sim_core/tests/friendly_attack_tests.rs` (new) | WP2 | P-4 |
| `crates/sim_fixtures/src/matchup.rs` | WP2 | P-9's extension |
| `crates/client/src/battle_pipeline/commands.rs`, `.../commands/tests.rs` | WP3 | D-4, D-5; P-6 |
| `crates/client/src/battle_hud/card.rs`, `crates/client/src/battle_hud/catalog.rs` | WP3 | D-4 |
| `crates/client/src/battle_input/mod.rs`, `crates/client/src/battle_input/command_mode.rs` | WP3 | D-5 |
| `crates/client/tests/battle_mode_tests.rs`, `crates/client/tests/battle_card_tests.rs` | WP3 | P-7 |
| `crates/client/tests/battle_input_tests.rs`, `battle_context_order_tests.rs`, `battle_card_path_tests.rs` | WP3, only if they fail | Rewritten by §8 rule 4 |
| `docs/plans/2026-11-23-stance-checked-orders.md` | WP4 | A closing line: B-174 and B-175 fixed by this plan |
| `crates/sim_core/src/movement/mod.rs` | **WP5 only, after the contact solver merges** | D-9 |

Nothing under `crates/sim_core/src/movement/` changes before WP5.

---

## 11. Work packages (commit order)

Each package is one branch from `main`, in its own worktree, with
`CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`.
- Write the failing tests first and paste their red run into the hand-back.
- Run `cargo gate quick` and the tests the change needs, including the `Slow` targets named in the package. A
  verifier then runs `cargo gate package` (Q0061).
- No `unwrap`. `expect` only with `invariant:`. No catalog identity in production Rust (C5). Tests find units by
  property.
- Check `docs/questions/open/` before the last commit.

WP0 to WP4 may run as one branch and one session, one commit each, with one `package` gate on the final tree
(charter §4.3). Each package's §7 measurement is still taken against the tree before it.

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-174 and B-175 read "open; plan 2026-11-24-attack-orders". B-186 and B-187 are registered under
  the next free numbers (D-8), and this plan is renumbered in the same commit if they differ. B-176 gains its note.
- Evidence file with §2.1's sources and the S8 join (the script stays in scratch, §16).
- Measure WP0's tree into `world-wp0` (§7).
- **Gate:** docs-only `package`.

### WP1: the A command as data (S, `sim_types`, `sim_core`, catalog; B-174 and B-186 data)

Failing tests first. They do not compile until the field exists. Once it exists with the old derivation
(`attack: None` everywhere), P-1's table is red.
- `catalog.rs` units: the derivation for a mobile armed, a mobile unarmed, an immobile armed and an immobile unarmed
  archetype, and an authored default set winning over it.
- P-1 in `catalog_reference_values_tests`.
- P-8's validation half and its run-time edits of the Lurker and Medivac-class rows.
- `can_attack` units: self refused, owner ignored, plane and flags read, and the settled replacement weapon used
  while burrowing.

Then the change: D-1, D-2.

**Acceptance:** P-1, P-8 (data and validation); §7's WP1 column against `world-wp0`; client crate builds (the
`StanceOrders` literal in tests and `ArchetypeCommands` gains `attack`, with the same meaning everywhere else).

### WP2: the simulation (M, `sim_core`, `sim_fixtures`; B-174 and B-175 simulation halves, B-186 scenarios)

Failing tests first (red on WP1's tree):
- `stance_orders_tests.rs`: P-2, P-3. P-5 passes on both trees, since it pins §3.3. It is kept as the rule's guard.
- `friendly_attack_tests.rs`: P-4. Expected green on WP1's tree (§3.2). If it is red, §8 rule 4 applies before any
  change.
- `scenario.rs` units: P-10.
- `matchup.rs`: P-9's extension. Expected green on WP1's tree: no fixture orders a unit that D-3 refuses. A red
  case is a finding for §8 rule 2.

Then the change: D-3, D-6. Grep for every `AttackTarget` issued straight to a world in tests: `crates/sim_core/tests/`,
`crates/sim_fixtures/`, and the client's GPU and world tests (`runtime_battle_tests`, `stance_gpu_tests`, and others
that build a world). Each one that orders an unarmed unit, a self-attack or an out-of-plane attack is rewritten by
§8 rule 4 and listed.

**Acceptance:** P-2 to P-5, P-8 (simulation), P-9, P-10; §7's WP2 column against WP1's tree; the goldens on the four
builds through the package gate. Before hand-back, run `sim_core/sc2_who_wins_matchup_matrix_tests` (`Slow`) and
`client/battle_mode_tests` and `client/battle_card_tests` (`Slow`, selected by the `sim_core` rule).

### WP3: the client (M, `client`; B-174, B-175, B-186 client halves)

Failing tests first (red on WP2's tree):
- `card.rs` units:
  - a Medivac-class unit shows Scan Move;
  - the unburrowed and the unburrowing Lurker analogue show Attack;
  - a mixed selection with any `Attack` member shows Attack;
  - an unarmed structure shows nothing;
  - the eight archetypes' slot order (R-2).
- `battle_pipeline/commands/tests.rs`: P-6, and `submit_attack_on_unit`'s partition. That covers the attackers, the
  walkers, the target excluded, both commands empty giving `EmptySelection`, and an own target.
- `battle_mode_tests.rs` and `battle_card_tests.rs` (through input): P-7.
- P-12: the warm-card allocation test extended.

Then the change: D-4, D-5.

**Acceptance:** P-6, P-7, P-8 (client), P-12; the skirmish stage log (§7). Before hand-back, run the `Slow` client
targets this package's code reaches, which are outside `quick`:
- `client/battle_input_tests`
- `client/battle_context_order_tests`
- `client/battle_card_tests`
- `client/battle_card_path_tests`
- `client/battle_mode_tests`
- `client/battle_hud_tests`
- `client/sync_steady_state_tests`

### WP4: bookkeeping (XS)

- `DESIGN.md`: B-174, B-175 and B-186 fixed, each with its commits and the evidence. The HUD row mentions Scan Move
  and the A-click on any unit.
- Evidence: §7's comparisons, the test list and the red runs.
- This plan's `## As built`; README row `Done`; the stance-checked-orders plan's closing line.
- **Gate:** docs-only `package`.

### WP5: movement admission by A command (S, `sim_core/src/movement/`; **after the contact solver merges**; B-176's `AttackMove` half)

Not started until the contact-solver branch is on `main`. Then a new branch is made from that `main`, and this
section is re-read against the merged `movement/mod.rs`.
- Failing tests first: an `AttackMove` naming a sieged tank, an unarmed structure and a marine-class unit moves the
  marine only, and leaves the other two field for field unchanged.
- Then the change: D-9.
- **Acceptance:** that test; §7's matrix and showcase columns byte-identical (no fixture sends such a command, P-9);
  the goldens.
- If B-176's own plan exists by then, WP5 moves into it and this plan's As built says so.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where | Gate selection |
|---|---|---|---|
| Data | The A command per stance against the sourced table; validation; run-time edits | WP1: `catalog.rs` units, `catalog_reference_values_tests`, runtime-catalog test | `catalog` rule; `sim_core` crate |
| Rule | `can_attack`: stance, self, plane, flags; owner ignored | WP1: `entity.rs` or `catalog.rs` units | `sim_core` crate |
| Admission | The simulation gives `AttackTarget` per unit; exhaustive over stances and switches | WP2: `stance_orders_tests` (`Fast`) | `sim_core` crate |
| Behaviour | An own target is attacked; an unarmed attack-mover walks | WP2: `friendly_attack_tests` (new), `stance_orders_tests` | `sim_core` crate |
| Scenario | Attack-move recipients by A command | WP2: `scenario.rs` units | `sim_core` crate |
| Fixture | No scripted attack misses its unit | WP2: `sim_fixtures/src/matchup.rs` units | `sim_fixtures` crate |
| Matrix | Nothing moved | WP2: `sim_core/sc2_who_wins_matchup_matrix_tests` | `Slow`, run explicitly |
| Client | Card, admission and A-click equal the simulation; through input | WP3: `card.rs` units, `commands/tests.rs`, `battle_mode_tests`, `battle_card_tests` | `battle_hud/**`, `battle_pipeline/**`, `client` crate |
| Neighbours | Nothing else in input broke | WP3: `battle_input_tests`, `battle_context_order_tests`, `battle_card_path_tests`, `battle_hud_tests` | `Slow`, run explicitly |
| Worlds | §7 | WP1 and WP2 (measure), WP3 (skirmish log) | — |

The gate catalog (`crates/gate/suites.ron`) needs no new rule. The new `friendly_attack_tests.rs` falls under the
`sim_core` crate rule. **Verify** that the catalog lists new integration targets by crate rule and not by name; if
by name, WP2 adds its row.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | The chase in `movement/` does not close on an own target | P-4's out-of-reach half red | §8 rule 4: reach-only in WP2, the chase in WP5 |
| R-2 | The eight unarmed archetypes' card entries move one slot and key, and a test clicks a cast by slot | `package` fails | Search the card tests for the Infestor- and Viper-class casts by property before WP3; a test that clicks by slot finds the entry by label (§8 rule 4) |
| R-3 | A test or fixture orders an unarmed or out-of-plane `AttackTarget` straight into a world | Red after WP2 | WP2's grep |
| R-4 | QA answered (b) or (c) | D-5's own-unit branch and D-3's owner rule change | (b): one client filter restored; the simulation keeps D-3 (a refusal upstream is enough). (c): `can_attack` drops the self check; the rest stands |
| R-5 | QB answered (b) | Walkers that hold `Attack` but cannot attack the target are left out | One filter in `submit_attack_on_unit`; no simulation change |
| R-6 | The S8 join is approximate (it does not follow `parent` inheritance) | A unit's A command misread | Every shipped analogue's row in P-1 is read by hand from its own `AbilArray` and `LayoutButtons` (S6, S7, S10) |
| R-7 | The contact-solver branch touches `entity.rs` near `settled_stance_orders` | Conflict | Rebase before the gate; the new queries are a few lines |
| R-8 | Two commands from one click confuse the status line or the outcome correlation | A rejection shown twice or lost | P-7's mixed case asserts one marker and the status line; the sequences are returned together (D-5) |

---

## 14. Out of scope and decisions recorded (Q0002)

- **A follow order** for Scan Move (and B-163's rally) on a unit: B-187, a movement change.
- **Autocast acquisition during an attack-move** (the Medivac's heal): no autocast exists here.
- **The Adept's shade** carries `attack` without a weapon (S8). No shade is modelled.
- **The burrowed Roach's Scan Move** (S9): no burrowed Roach analogue is shipped. If one is added, its mode authors
  `attack: ScanMove` and needs no code.
- **Own cloaked units.** `is_entity_available_for_combat` asks for the attacker's detection of a concealed target,
  own units included. Whether 5.0.14 counts one's own cloaked unit as `Visible` to oneself is **verify**. It is
  unchanged here.
- **Charge and other dashes toward an own unit** (`attack_dash.rs` skips them): **verify** against the dash
  ability's validators, unchanged.
- **The Nexus and the Bunker's redirected Attack**: the previous plan's §14.
- **Allied players**: no shipped mode has allies. The rule reads no owner, so an ally is treated as an own unit,
  which is S1's reading for both.
- **Patrol** (the move ability's `Patrol`): not a command here.

---

## 15. Draft owner questions (for the orchestrator to file; not filed by this plan)

Filed as [Q0104](../questions/closed/0104-attack-click-on-own-unit.md) and [Q0105](../questions/closed/0105-attack-click-on-unattackable-target.md) (Q0101 and Q0103 are taken by the vision and contact-solver plans).

```markdown
# Q0104: Does "A, then click my own unit" attack it?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a), the selected units attack it; a unit never attacks itself.

## Question

In StarCraft II you can force your units to attack one of your own, for example to kill a stuck unit or
deny a building, by pressing A and left-clicking it. The 5.0.14 data supports this. The attack ability
leaves out your own and allied units only when units pick targets by themselves and on a right click. Ordinary
weapons accept your own units, and the few that must not (Point Defense Drone, Widow Mine, Planetary
Fortress) exclude them by name. No StarCraft II page we found says it outright.

Plan `2026-11-24-attack-orders` makes it work. Today the click only attack-moves to where the unit stands.

- **(a) Attack it (default).** Units that can hit it attack it. The others walk to it. The clicked unit itself
  does nothing even if it is selected.
- **(b) Never attack your own units.** The click on your own unit attack-moves to its position, as now.
- **(c) As (a), and a selected unit may attack itself.** The data does not rule it out for ordinary weapons,
  and two weapons exclude "self" by name, which hints that it is otherwise possible. We do not know the game
  does it.

Recommendation: (a). Right-clicking your own unit is unchanged in every case.

## Answer

```

```markdown
# Q0105: What does a unit do when you A-click it onto a target it cannot attack?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a), it attack-moves to where the target stands.

## Question

Some units have an Attack command but no weapon for a given target: the Lurker before it burrows
(Attack but no weapon at all), or a ground-only unit A-clicked onto an air unit. Units with no attack at
all (Medivac, Raven, Observer) have Scan Move in the same slot, and the data says they move. For the first
group, the data does not say what happens.

- **(a) It attack-moves to where the target stands (default).** The army moves together, and the unit
  fights whatever it can hit on the way.
- **(b) It ignores the click and keeps its current orders.** The other selected units still act.

Recommendation: (a), unless you know the game does (b). It only matters for mixed selections or the
unburrowed Lurker.

## Answer

```

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,…,balancemulti}/{AbilData,UnitData,WeaponData}.xml`
  (`5444e12f13`), read with `grep`, `sed` and `awk`. For each unit, its `AbilArray` and its row 0 column 4
  `LayoutButtons` were read across all eight layers.
- S8's join: a short Python script (stdlib `xml.etree`, run with `-I`) that applies each layer's `AbilArray` and
  `WeaponArray` entries, `index` and `removed` included, but not `parent` inheritance. It lists every unit with an
  attack ability and no weapon, and every unit with a weapon and no attack ability. WP0 copies its output, not the
  script, into the evidence.
- Web, as listed under §2.1: Liquipedia *Micro (StarCraft)*; Blizzard forums
  `us.forums.blizzard.com/en/starcraft/t/how-can-i-remove-building/2150` (StarCraft: Remastered) and
  `us.forums.blizzard.com/en/sc2/t/which-units-deal-friendly-fire/12103`. None states the 5.0.14 A-click on an own
  unit.
- No prototype. §3.2's claim that an own target is attacked today is reasoned from the code, and WP2's P-4 measures
  it first.

---

## As built

WP0 to WP4 on branch `attack-orders-plan`, one commit each: WP0 `e97c2e82`, WP1 `1d353ed4`, WP2 `5e5b4fe8`,
WP3 `86189817`, WP4 (this section and the records). WP5 on branch `attack-orders-wp5`, cut from `main` at
`6b813f1e` after the contact solver merged and merged with `main` at `708576ac`: `2174010b` (D-9 and its tests),
`31210942` (battle 21's fixture order), and the records. Measurements, red runs and rewritten tests are in the
[evidence](../evidence/2026-11-24-attack-orders.md).

1. **B-numbers.** B-186 and B-187 kept: `main` ended at B-185 when this branch merged it (the vision branch took
   B-182 to B-185). They are marked "next free at merge" in their rows.
2. **No schema, golden or world moved.** Every measured world of §7 is byte-identical on WP1 and WP2 (483 of 483
   files), and the skirmish stage log is identical on WP3. The checksum schema is untouched.
3. **`CatalogError::ScanMoveWithoutHoldPosition`** is the typed variant of D-1, on the catalog's existing typed error
   (`catalog::CatalogError`, reached as `SimCoreError::SpellCatalog`). Its `stance` names `default_stance_orders` or
   `modes.<name>.orders`.
4. **D-3's shape.** The simulation decides each recipient in a read-only `SimWorld::takes_plain_order` before
   borrowing it mutably, rather than looking the target up once before the loop: the target lookup is per
   recipient (a map read), with the same answer. A target that is gone or dead refuses every recipient, with no
   diagnostic.
5. **D-5's shape.** `BattleCommandQueue::submit_attack_on_unit` validates both commands before queuing either and
   returns the last command's sequence; the input marks one Attack marker at the unit. The walkers' point is the
   simulation point of the picked unit's ground point (`sim_point(unit.ground)`).
6. **P-4 (R-1) did not occur.** An attack ordered on an own unit in reach strikes it within one weapon period plus
   its damage point, and a mobile attacker out of reach closes in and strikes; the victim and an own bystander
   acquire nothing. Nothing moved to WP5 under §8 rule 4.
7. **P-6's delivery** is read from the simulation's command phase (`SimWorld::process_commands`): an armed structure
   out of reach takes the attack and drops it later in the same tick, by combat's rule.
8. **P-7 lives in `battle_mode_tests.rs`** (four tests); `battle_card_tests.rs` needed no change and passes. The
   shipped Medivac and Lurker analogues are presentable, so P-7 uses them unedited.
9. **Tests rewritten by §8 rule 4** (listed per package in the evidence): in WP1 the catalog unit tests of the
   default stance, `stance_orders_tests`' default-stance test (Stop and Hold only), two runtime-catalog RON edits and
   two `UnitArchetype` literals; in WP2 `scenario_tests::enter_modes_queues_behind_earlier_orders`, whose burrowing
   units now walk along (the test authors them without an A command at run time to keep its "at once" branch); in
   WP3 the two Lurker card tests, the previous plan's P-3 Attack-half test (replaced by P-6) and the self-attack
   expectation of `stationary_armed_actor_can_target_hostile_but_cannot_attack_move` (`Capability`, not
   `InvalidTarget`).
10. **Removed:** the HUD's `default_armed`, `mode_armed` and `settled_armed` (no reader left). `is_armed` and
    `ArchetypeCommands::armed` stay; tests read them.
11. **The S8 join** found what the plan's §16 described, with two notes recorded in the evidence: the Bunker's
    redirect is a `CAbilRedirect`, so it does not appear, and `WarpPrismPhasing` carries `AttackWarpPrism`, which
    excludes every alliance (S4).
12. **WP5 (D-9) as planned.** D-9 was re-read against the merged `movement/mod.rs` and fit unchanged.
    `validate_movement_order` skips, for an `AttackMove`, each recipient that `takes_attack_move` refuses (anchored
    by `Entity::immobile_after_channel`, or `Entity::settled_attack_command` is `None`) after the selection's own
    checks (existence, owner, alive, not carried), so a refused recipient's domain is not asked;
    `issue_movement_prevalidated` drops the same recipients from its retained buffer before committing, so the
    receipt counts the units that took the order and a route group forms from them only. A command left with no
    recipient is `Ok` with `recipient_count` 0 and changes nothing. `Move` is unchanged (B-176's own half).
13. **The plan's "no fixture sends such a command" was wrong for one battle.** P-9's extended check (an
    `AttackMove` names only units mobile in their settled stance with an A command, and each took it) named
    `21_lurker_vs_6_marines`: its Lurker side burrows on the first tick and was then given an attack-move, which D-9
    now filters away. The order changed nothing observable: the matrix, battles 31 and 33 and the showcases are
    byte-identical to `main` (483 of 483 files), and battle 21's tick and unit traces on its 20 seeds are identical
    too. Under §8 rule 2 the fixture was made honest, not fitted: the side now gives no order (`order: None`), and
    the measurements repeat byte for byte on that tree. The orchestrator may prefer another fixture shape.
