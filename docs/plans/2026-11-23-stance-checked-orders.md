# Blueprint: Orders Checked by Stance in the Simulation; Stop and Attack for Armed Structures; Attack by Stance (B-171, B-172, B-173)

This plan finishes what plan [`2026-11-23-anchored-commands`](2026-11-23-anchored-commands.md) began. That plan gave
each stance a set of plain orders (`StanceOrders`) and made the client's card and admission read it. Three rows were
left open, and this plan closes all three:

- **B-172: the simulation checks the stance.** `issue_plain_order` gives Stop and Hold Position only to the units
  whose settled stance takes them, as StarCraft II sends a command to every selected unit that has it (`ToSelection`)
  and to no other. This is the authoritative rule. The client's check becomes a preview of it, and a scripted, replayed
  or remote command can no longer hold a sieged tank. Matrix battles 37a, 37b, 39a and 39b scripted exactly that, so
  their orders are restated first, in a package of their own, from what each battle is meant to test.
- **B-171: armed structures take Stop.** The default stance's set becomes `stop = mobile || armed`,
  `hold_position = mobile`. This rule is derived from capabilities and is exact for every shipped archetype against
  5.0.14's `AbilArray`s (§2.2). The Photon Cannon, Missile Turret, Planetary Fortress, Spine Crawler and Spore
  Crawler analogues get Stop. Unarmed structures and the Shield Battery analogue get none.
- **B-173: the Attack entry follows the stance.** The card shows Attack for a unit that is armed in its settled
  stance. That covers the burrowed Lurker analogue, the sieged tank and, with B-171, the armed structures. In Attack
  mode, a click on an enemy unit attacks that unit (`AttackTarget`). This is what makes Attack meaningful for a unit
  that cannot walk. A click on the ground still attack-moves the mobile members. An anchored unit's attack on a
  point is not in the data, so it is refused (default, **verify**, draft question Q0098, §15).

No hashed field, encoding or schema changes. Goldens and the sandbox checksum are expected unchanged (§5). No
measured world is expected to change its outcome, but the four matrix battles' hashed order queues do change (§7).

A full plan is needed (charter §6) because B-172 changes battle-relevant simulation admission and test scenarios, and
the work spans `sim_core`, `sim_fixtures` and `client`.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`aa92b094`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations (S-1: RON data instead of editor data) | Closed | D-2 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §8, §14 |
| The owner's guidance: no fitting to fixed seed sets; acceptance by principled properties; scenarios restated on principle | Followed | §6, D-5 |
| **Q0098 (draft, §15): what an anchored unit does with Attack on the ground** | Not filed; for the orchestrator | D-7; default (a), refused |

---

## 1. Summary

1. **B-172 (D-1, D-5).** `SimWorld::issue_plain_order` keeps each `Stop` and `HoldPosition` from a unit whose
   `Entity::settled_stance_orders` lacks it, and gives it to every other recipient. The refused unit is left exactly
   as it was. This is S2's `ToSelection`, the rule the client's `capable` already applies. Before the code changes,
   WP1 restates the four matrix battles so that no scripted order is one its unit refuses. The sieged tanks get no
   order, because siege already anchors them (S3), and the marines beside them keep their Hold.
2. **B-171 (D-2).** `UnitArchetype::stance_orders(None)` becomes `{ stop: mobile || armed, hold_position: mobile }`,
   with "armed" being the new `UnitArchetype::stance_armed(None)`. The card, admission and simulation all read this
   one function, so all three follow.
3. **B-173 (D-3, D-6, D-7).** A new read-only pair, `UnitArchetype::stance_armed(Option<&UnitModeSpec>)` and
   `Entity::settled_armed(&UnitArchetype)`, answers whether a unit has a weapon in its settled stance, by the same
   rule the simulation uses to pick weapons (`apply_mode_transition`, `weapon_against`). The card's Attack entry,
   `capable`'s `AttackTarget` and `AttackMove` arms, and the Attack mode's viability read it. The Attack mode's
   click on an enemy unit sends `AttackTarget`.
4. **Records.** B-171, B-172 and B-173 are fixed. Three neighbours are registered and deferred: B-174 (the
   unburrowed Lurker analogue has no Attack), B-175 (Attack on a friendly unit) and B-176 (the simulation's
   movement admission does not check the stance; it lives under `movement/`, which another branch owns).
5. **Worlds (§7).** Every measured output is expected to be byte-identical across every package. §3.3 gives the
   reasoning for the four matrix battles, and §8 says what happens if they differ. The checksum schema stays 36.

---

## 2. What StarCraft II does

### 2.1 Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data. The
override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. S1 to S3 and
S6 are the anchored-commands plan's, re-read for this plan.

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | Stop is its own ability (`CAbilStop`, link `stop`); Hold Position is the `HoldPos` command of `move` | `CAbilStop default`; `CmdButtonArray index="HoldPos"` | `core` AbilData:1013–1027, :1362; :552–554 |
| S2 | Stop, Hold Position and Attack go to every selected unit that has them (`ToSelection`) | `Flags index="ToSelection"` | `core` AbilData:1015–1016 (Stop), :127–129 (Attack `Execute`), :553 (move) |
| S3 | **Sieged Siege Tank:** `stop`, `attack`, `Unsiege`; no `move` | `AbilArray` | `liberty` UnitData:5172–5174 |
| S4 | Its Attack button has face `AttackBuilding`; the unsieged tank's has `Attack` | `LayoutButtons` | `liberty` UnitData:5187; :5116 |
| S5 | **Burrowed Lurker:** `attack`, both burrow abilities, `move`, `stop`; Attack face `AttackBuilding` | `AbilArray`, `LayoutButtons` | `liberty` UnitData:793–796, :800; `void` UnitData:767 |
| S6 | **Armed structures carry `stop` and `attack`; Attack face `AttackBuilding`** (the Planetary Fortress `AttackBuildingPFort`) | `AbilArray`, `LayoutButtons` | Photon Cannon `liberty` UnitData:11083–11084, :11090–11091; Missile Turret :6064–6065, :6072–6073; Planetary Fortress :10696–10697, :10708–10709; Spine Crawler :8652–8653, :8659–8660; Spore Crawler :8784–8785, :8793–8794 |
| S7 | **Unarmed structures carry no `stop` and no `attack`** | `AbilArray` | Command Center `liberty` UnitData:10411–10418; Orbital Command :10557–10564; Nexus :4218–4223 |
| S8 | **Shield Battery:** `liberty` gives it `stop` as entry 2; 5.0.14 removes entry 2 | `AbilArray index="2" removed="1"` | `liberty` UnitData:1603–1605; `voidmulti` UnitData:1557; `balancemulti` UnitData:1463 |
| S9 | The Nexus's `stopProtossBuilding` and `attackProtossBuilding` require `PurifyNexusRequirements` | `CmdButtonArray … Requirements` | `swarm` AbilData:954–960, :968–970; `swarm` UnitData:1732; `swarmmulti` UnitData:828 |
| S10 | The Bunker's Stop and Attack are redirects to its passengers | `CAbilRedirectInstant id="StopRedirect"`, `CAbilRedirectTarget id="AttackRedirect"` | `liberty` UnitData:6192–6193; AbilData:3606–3611, :3633–3638 |
| S11 | **Unburrowed Lurker** carries `attack` in 5.0.14 although it has no weapon; its card has Attack | `AbilArray Link="attack"`; `LayoutButtons Face="Attack"` | `voidmulti` UnitData:974; `balancemulti` UnitData:909; `liberty` UnitData:735 |
| S12 | The attack ability excludes the viewer's own and allied units from automatic acquisition and from the smart (right) click only | `AcquireFilters`, `SmartFilters` | `core` AbilData:118–119 |
| S13 | Stop drops the unit's orders, a focused target among them; nothing in `CAbilStop` treats a structure differently | `CAbilStop default` | `core` AbilData:1013–1027 |

### 2.2 The rules that follow

1. **Admission is per unit (S2).** A command reaches each named unit whose current type has the ability and skips
   the rest. A Hold given to sieged tanks and marines together reaches the marines only. Nothing in the data refuses
   the whole command, so the simulation applies the rule unit by unit.
2. **The default stance's Stop is "mobile or armed" (S6–S8).** Every shipped mobile archetype has `stop` and
   `move` (anchored-commands D-1). Every armed structure has `stop` (S6). The unarmed structures in the data have
   none (S7), including the Shield Battery, whose `stop` 5.0.14 removes (S8, assuming `index="2"` is the third
   `liberty` entry, the `stop` at :1605: **verify** the editor's indexing). The default stance's Hold Position is
   "mobile", since no structure has `move`.
   Two edge cases have nothing to model. The Nexus's Stop and Attack depend on `PurifyNexusRequirements` (S9). The
   multiplayer Nexus has no weapon, so the requirement is taken as unmet (**verify**; `RequirementData` is not
   local), and the rule's answer (none) stands. The Bunker's Stop is a redirect to its passengers (S10). The Bunker
   analogue (`fortress_bunker_dome`) is unarmed and carries no passengers, so it gets none (§14).
3. **The Attack entry is "armed in the stance", except for one unit (S3, S5, S6, S11).** Every armed stance in the
   data has `attack`: mobile armed units, the sieged tank, the burrowed Lurker and the armed structures. Unarmed
   structures have none (S7). The exception is the unburrowed Lurker, which has `attack` without a weapon (S11).
   That is B-174, deferred (§8).
4. **An attack on a unit attacks that unit.** `CAbilAttack`'s `Execute` takes a unit or a point (S2, S4). With a
   unit target it is the attack the right click already sends to an enemy (`AttackTarget`). That the A-click on a
   unit means this is common knowledge of the game and is not stated in the data (**verify**). An anchored unit
   whose target is out of reach abandons the attack, which the simulation already does
   (`target_acquisition.rs`, `Engagement::Explicit` with `is_immobile`).
5. **An anchored unit's attack on a point is not in the data.** No `CAbilAttack` field covers a caster without
   `move`. Anchored stances and structures show their button with a separate face, `AttackBuilding` (S4–S6), and
   mobile units show `Attack`. That suggests the game presents Attack for them differently, but the face's tooltip
   is in `ButtonData`, which is not local. D-7 takes the reading that changes no simulation rule (refused) and
   drafts Q0098.
6. **Stop on an armed structure (S13)** drops a focused target, and the structure picks its next target by itself.
   It does not touch its production queue (`que5Passive…`, a separate ability) or its rally (S6's Planetary
   Fortress carries `RallyCommand` and `CommandCenterTrain` beside `stop`).

---

## 3. Diagnosis (on `aa92b094`, read from the code)

### 3.1 The simulation (B-172)

- `world/commands.rs` `dispatch_command` sends every order that is not economic, movement, production, stim or cast
  to `issue_plain_order`. That function calls `Entity::issue_order` for each owned recipient, with no capability
  check. Stop, Hold Position, `Idle` and `AttackTarget` therefore reach any unit, whatever its stance.
- The client's `capable` (`battle_pipeline/commands.rs`) already decides Stop and Hold by
  `entity.settled_stance_orders(archetype)` and revalidates at the tick (`prepare_tick`). The simulation does not.
  A command that does not come through the client is unchecked: the matchup fixtures, the goldens, the lab sandbox
  and, later, a network peer.
- `sim_fixtures::matchup::side_commands` sends the side's standing order to every unit of the side after
  `mode_commands` has cast the switch on tick 0. In 37a and 37b (two sieged tanks with four marines) and in 39a and
  39b (three sieged tanks), the sieged tanks are given `HoldPosition`. Their settled stance is `siege_mode` with
  `hold_position: false`.
- The unit test `mode_commands_precede_the_standing_order_which_survives_the_switch` (`sim_fixtures/src/matchup.rs`)
  asserts that a sieged tank keeps a Hold given after the switch. It pins the behaviour B-172 removes.

### 3.2 The default stance (B-171)

- `UnitArchetype::stance_orders(None)` (`catalog.rs`) is `NONE` for an archetype that `is_immobile_by_default()`,
  which includes every structure. Armed structures therefore have no Stop on the card
  (`battle_hud/card.rs::derive_fixed` through `ArchetypeCommands::default_orders`), and admission refuses it.
- In the simulation a Stop on a structure is accepted today (§3.1). `commit_order`'s `Stop` arm clears the queue,
  an `Explicit` `AttackTarget` among it, and the next acquisition pass (`Seek`) chooses again. Production
  (`production_queue`) and the rally are separate fields (**verify** the rally's field when writing P-5's test).

### 3.3 Hold against no order for a sieged tank (what B-172 changes in a battle)

An anchored unit acquires the same way whether it holds or not. The engagement differs only in bookkeeping:

- In `target_acquisition.rs`, `engagement_of` returns `Hold` for a queue fronted by `HoldPosition` and `Seek` for an
  empty one. `stationary = entity.is_immobile || Hold`, so the scan is the same for a sieged tank.
- `engage` puts `AttackTarget { origin: Acquired }` at the front for `Seek`, and for `Hold` sets only `current_target`.
  Afterwards a seeking tank is `Acquired(target)`, and a holding one reads `current_target`. Both keep an in-reach
  target unless something outranks it. When nothing is left, `Acquired` with `is_immobile` gives `AbandonAttack`
  (pop the order, clear the target) and `Hold` gives `ReleaseHoldTarget` (clear the target). Either way the unit has
  no target and scans again.
- The other readers of Hold (`is_holding_position` in fleeing and call-for-help, `is_on_hold` in
  `combat_resolution.rs`, `attack_dash.rs` and `locomotion.rs`) are each also gated by `is_immobile` or by a turret.
  The tanks are immobile and turreted.

So for the sieged tanks of 37a/b and 39a/b, "no order" and "Hold" should choose the same targets on the same ticks.
The hashed state differs: the order queue, `is_holding_position`, and the `movement.restart()` and funnel clearing
of `engage`. The matrix outputs (`matrix.csv`: end tick, survivors, hulls, blocked ticks, reversals) are outcome
measures, so they are expected byte-identical (§7). That is a prediction, recorded before measuring.

### 3.4 The client's Attack (B-173)

- `root_entries` (`card.rs`) shows Attack when a completed selected type is `mobile && armed`. Both are facts about
  the archetype (`ArchetypeCommands`, `catalog.rs`'s `commands_of`), not the stance. The burrowed Lurker analogue
  (`weapon: None`, armed only through `burrowed`'s `ground_weapon_override`) gets no Attack, and neither does any
  structure. The sieged tank shows Attack because its archetype is mobile and armed.
- `CommandMode::AttackMove` (`battle_input/command_mode.rs`) is viable only while `any_recipient` of an `AttackMove`
  holds (`reconcile`). `capable` admits `AttackMove` for `mobile && armed`. For a sieged tank selected alone, the
  mode is therefore dropped on the next reconcile. The Attack button exists but cannot be used.
- `handle_targeting_pointer` (`battle_input/mod.rs`) turns every left click in Attack mode into `AttackMove` at the
  ground point, even on an enemy unit. A mobile unit A-clicked on an enemy therefore attack-moves to where the
  enemy stood instead of focusing it (rule 4). Only the right click (`context_order`) sends `AttackTarget`.
- `capable`'s `AttackTarget` arm is `armed || entity.is_sieged && entity.siege_weapon.is_some()`, using the active
  stance. The card uses the archetype and the mode card uses the settled stance (anchored-commands D-2), so three
  different questions are asked about one unit.

---

## 4. Design decisions

### D-1. The simulation admits Stop and Hold Position by the settled stance (B-172)

In `world/commands.rs::issue_plain_order`, before `ent.issue_order`:

```rust
// A unit takes Stop or Hold Position only when its settled stance has the command
// (StarCraft II `ToSelection`, `core` AbilData:1015-1016; plan 2026-11-23-stance-checked-orders D-1).
let takes = match cmd.order {
    UnitOrder::Stop => arch.is_some_and(|arch| ent.settled_stance_orders(arch).stop),
    UnitOrder::HoldPosition => arch.is_some_and(|arch| ent.settled_stance_orders(arch).hold_position),
    _ => true,
};
```

`arch` is the archetype that the existing `glides` test already looks up. A unit that does not take the order is
skipped and left byte for byte as it was: queue, `is_holding_position`, velocity, weapon phase and `current_target`.
The other recipients get the order as today. A unit whose archetype is missing from the catalog does not take
Stop or Hold. The live catalog always holds every spawned type, so this branch is defensive, matching the
existing `is_ok_and`.

- **Queued and immediate orders alike.** The client's `capable` does not read `queue`, and the simulation keeps the
  same rule. For an order queued behind a switch that has not started yet, the data has no answer (B-96's open
  question), and this plan does not decide it (§14).
- **No outcome record and no diagnostic.** A skipped unit is the game's rule (S2), not a failure. It matches how
  `issue_plain_order` already skips a unit the player does not own. The charter's "no silent failures" covers errors.
  This is admission, documented in the function's doc comment, and a command that reaches no unit is valid and does
  nothing, as in the game.
- **`Idle` and `AttackTarget` are out of scope** (§14): `Idle` is not a player command in the game, and the
  simulation's `AttackTarget` handling of an unarmed unit belongs with B-174.

### D-2. The default stance: Stop when mobile or armed (B-171)

```rust
pub fn stance_orders(&self, stance: Option<&UnitModeSpec>) -> StanceOrders {
    match stance {
        Some(mode) => mode.orders,
        None => {
            let mobile = !self.is_immobile_by_default();
            StanceOrders { stop: mobile || self.stance_armed(None), hold_position: mobile }
        }
    }
}
```

Its doc cites S6 to S8 and §2.2 rule 2. **Derived, not authored.** The anchored-commands plan deferred B-171 because
authoring the default stance's set would touch every row of `units.ron` and `structures.ron`. The derivation is
exact for every shipped archetype (P-4 checks it against a sourced table), it reads capabilities rather than
identities (C5), and a weapon edited in the catalog moves it without a rebuild (C6, P-8). If a future archetype
breaks it (the unburrowed Lurker's `attack` is the one known break, for Attack only, B-174), an authored
`default_orders` can replace the derivation then.

### D-3. "Armed in a stance", one rule in `sim_core` (B-171, B-173)

```rust
impl UnitArchetype {
    /// Whether a unit of this archetype carries a weapon in `stance`: the mode's replacement
    /// ground weapon when it sets one, otherwise the archetype's ground, single or air weapon,
    /// by the rule `Entity::apply_mode_transition` and `weapon_against` apply to a live unit.
    #[must_use]
    pub fn stance_armed(&self, stance: Option<&UnitModeSpec>) -> bool;
}

impl Entity {
    /// Whether this unit is armed in the stance it is in or is switching to
    /// ([`Self::settled_mode`]). `archetype` must be this unit's.
    #[must_use]
    pub fn settled_armed(&self, archetype: &UnitArchetype) -> bool;
}
```

`stance_armed(Some(mode))` is `mode.ground_weapon_override` holding `Some(Some(_))`, or the archetype's own
`ground_weapon`, `weapon` or `air_weapon`. This is how the live unit fights. `apply_mode_transition` stores the
override's weapon as `siege_weapon` and does nothing for `Some(None)`. `weapon_against` then falls back to the
archetype's ground weapon and always reads the archetype's air weapon. The query must agree with the live unit
(P-6), not with the override's documented intent. `Some(None)` not disarming, and `air_weapon_override` not being
applied, are recorded as findings in §14. No shipped mode uses either: `units.ron` holds no `Some(None)` override
  and no `air_weapon_override: Some`.

`stance_armed(None)` equals the client's `is_armed(archetype)`. The client's `is_armed` stays, as the rule its tests
use for "armed by default", and it gains a doc line pointing at `stance_armed`. Both queries are `&self`, allocate
nothing, and live next to `stance_orders` and `settled_stance_orders`, outside `movement/`.

### D-4. Admission and card read the shared rules (B-171, B-173)

- `capable` (`battle_pipeline/commands.rs`):
  - `UnitOrder::AttackTarget { .. } => entity.settled_armed(archetype)`. This replaces the active-stance
    `armed || is_sieged && siege_weapon`, so it now uses the settled stance, as Stop, Hold and the mode entries do.
  - `UnitOrder::AttackMove { .. } => mobile && entity.settled_armed(archetype)`. For every shipped unit this matches
    today's `mobile && armed`, since no shipped unit is mobile and armed only through a mode.
  - `Stop` and `HoldPosition` are unchanged in code and follow D-2 through `settled_stance_orders`.
- `ArchetypeCommands` (`battle_hud/catalog.rs`) gains `default_armed: bool` and `mode_armed: Vec<bool>`, indexed like
  `mode_orders`, filled once per session in `commands_of` through `stance_armed`. `armed` and `mobile` are kept
  for their remaining readers (**verify**: `mode_unit` in the card tests reads `is_armed`, not the field).
  `default_orders` already reads `stance_orders(None)`, so armed structures' Stop slot appears with no card change.
- `root_entries` (`card.rs`): Attack is shown when any completed selected unit's settled stance is armed. The
  stance is looked up as `settled_orders` does, by binary search of `mode_states` with `mode_armed[settled]` or
  `default_armed`. The two lookups share one private helper that returns the settled mode index, so neither
  duplicates the search. No allocation.
- **The Planetary Fortress analogue's card gains Attack as its first root entry**, which moves its train entries by
  one slot and one key (R-2). That matches the data (S6: Attack at (0, 4), training on its own rows) as far as this
  card can: it places root entries in order and has no layout coordinates.

### D-5. The four matrix battles, restated on principle (B-172)

| Battle | What it is meant to test (its claim) | Old orders | New orders | Why |
|---|---|---|---|---|
| 37a | Abduct pulls a sieged tank out of its partner's cover and zerglings kill it | Red side `HoldPosition`, `mode: Some("siege_mode")` | Red side `HoldPosition`; the tank squad `order: Some(None)` | The tanks stand because they are sieged (S3). A player cannot hold a sieged tank. The marines hold, so they do not walk out of cover |
| 37b | Control of 37a without the cast | As 37a | As 37a | Same |
| 39a | Blinding Cloud on sieged tanks turns the fight | Red side `HoldPosition`, `mode: Some("siege_mode")` (tanks only) | Red side `order: None` | The side is tanks alone. Siege is the stance under test, and no order exists for it in the game |
| 39b | Control of 39a | As 39a | As 39a | Same |

Each battle's comment gains one line: "The sieged tanks take no order: 5.0.14's sieged tank has no Hold Position
(`liberty` UnitData:5172–5174); siege alone anchors them (plan 2026-11-23-stance-checked-orders D-5)." Claims and
expectations stay as they are, because the prediction (§3.3) is that the outcomes do not move. If they do move, §8
rule 2 applies, and the claim is re-read from what the battle is meant to test, not fitted.

**Fixture honesty, a new property (P-9).** A `sim_fixtures` unit test plays tick 1 of every matrix battle. It
asserts that every unit named in a first-tick `HoldPosition` command ends the tick holding (`is_holding_position`),
and that every unit named in a `Stop` has an empty queue or an acquired target only. On `aa92b094` it fails on
exactly 37a, 37b, 39a and 39b. It keeps a scripted order from silently missing its unit again.

The unit test `mode_commands_precede_the_standing_order_which_survives_the_switch` is rewritten from its intent: a
standing order given after a switch survives it. It finds a mode whose `orders.hold_position` is true, by property
(today `burrowed`), instead of the sieged tank, and keeps its assertion that cast commands precede standing orders.

### D-6. The Attack mode aims at units (B-173)

- `handle_targeting_pointer` (`battle_input/mod.rs`), for `CommandMode::AttackMove`: the click first picks a unit
  under the pointer with `view.pick_unit(world, owner, ray)`, the same fog rules as selection and unit casts. If the
  pick is a unit the viewer does not own, the order is `UnitOrder::attack(unit.id)` with the `Attack` marker on the
  unit. Otherwise the order is `AttackMove` at the ground point, as today. A click on one's own unit stays a ground
  attack-move (B-175, deferred).
- `CommandMode::order_at` keeps the ground case. The unit branch lives in `handle_targeting_pointer`, since it needs
  the view's picking, as `cast_target` does. **Verify** whether allied units exist in any shipped mode. The rule is
  "not owned by the viewer", and admission and the simulation refuse a target that cannot be attacked.
- `CommandMode::reconcile`: the Attack mode is viable while some member could take either of its two orders. That
  is `any_recipient` of an `AttackTarget` (whose capability does not read the target, **verify**) or of an
  `AttackMove`. `eligibility_order` becomes a small set of orders, or `reconcile` gains an arm for Attack. The
  implementer picks the shape with fewer branches.
- A mixed selection A-clicked on an enemy sends `AttackTarget` to every member armed in its settled stance (S2),
  anchored ones included. A sieged tank out of reach drops the attack by itself (§2.2 rule 4).

### D-7. An anchored unit's attack on a point: refused (default of Q0098)

A ground click in Attack mode sends `AttackMove` through `submit_selection`. `capable` admits only mobile, armed
members. With only anchored members it is refused with `SubmissionError::EmptySelection` and its status line. The
mode stays armed, because only an accepted order ends it (`handle_targeting_pointer`'s rule), so the player can
click an enemy next. Nothing changes in the simulation. Its movement admission accepts `AttackMove` for any unit
(B-176), but no client command reaches it for an anchored unit, and no fixture sends one to a sieged tank. Battle
21's burrowed Lurker attack-moves, and 5.0.14's burrowed Lurker has `move` and `attack` (S5).

Q0098 (§15) asks whether 5.0.14 instead accepts the order as an attack-move in place: the unit drops its orders and
scans, as Stop does. If the owner says so, it is a simulation rule in movement admission (B-176's area) plus a
client admission change, a later package.

### D-8. Records

- **B-171, B-172, B-173:** fixed, each with its commit (WP2, WP2, WP3) and this plan's link.
- **B-174 (new): the unburrowed Lurker analogue has no Attack.** 5.0.14's unburrowed Lurker carries `attack`
  without a weapon (S11). Its Attack is an attack-move, and in a mixed army A-moved across the map it walks with
  the others. Here it has no Attack entry, and an `AttackMove` drops it from the command, so it stays behind.
  Deferred: it needs a stance's Attack authored apart from its weapons (an `attack` in `StanceOrders`, or a
  default-stance field), and a rule for what an unarmed attack-mover does in the simulation (it walks and acquires
  nothing, **verify**). The unburrowing Lurker analogue is refused `AttackTarget` under D-4's settled rule, while
  5.0.14's has `attack` from the start of the unburrow (anchored-commands S13). This is the same row.
- **B-175 (new): Attack on a friendly unit.** In 5.0.14 the attack ability's smart click and its automatic
  acquisition skip one's own and allied units (S12), but its `Execute` command does not, so an A-click on one's own
  unit attacks it (**verify**). Here it attack-moves to the unit's position (D-6). Deferred: it needs the
  simulation's `AttackTarget` to be checked for friendly targets, which no plan has done.
- **B-176 (new): the simulation's movement admission does not check the stance.** `validate_movement_order`
  (`movement/mod.rs`) accepts `Move` and `AttackMove` for a sieged tank, which 5.0.14 has no `move` for (S3). The
  client refuses both. Deferred: the code is under `crates/sim_core/src/movement/`, which another branch owns now,
  and its answer for a queued order depends on B-96. Q0098's answer decides the `AttackMove` half.

---

## 5. Determinism and checksum

- **No hashed field, encoding or catalog hashing changes.** `stance_orders`, `stance_armed` and their entity forms
  are pure reads. The catalog is not hashed (anchored-commands §3.2). **The checksum schema stays 36.** No "next
  free at merge" number is taken.
- **Fixed point only.** The queries compare `Option`s and `bool`s. No float enters, so C7 is unaffected.
- **Goldens.** `golden_replay.ron` holds a `HoldPosition` for `red_psi` (zealots, mobile) on tick 640 and a `Stop`
  for `red_psi` and `red_siege` (tanks never sieged) on tick 760. `golden_replay_orders.ron` holds a `HoldPosition`
  for `red_ledge` (`psi_grey`, mobile) on tick 0. Every one reaches a unit whose default stance takes it, under both
  the old and the new rule. Neither replay places an armed structure that is ordered. Both goldens and
  `DEFAULT_SANDBOX_CHECKSUM` are expected unchanged on native debug and release, Windows, macOS ARM64 and
  `wasm32-wasip1`.
- **If a golden or the sandbox checksum moves, that is a defect of the package** (§8 rule 1), not a re-record. It
  means a recorded order reached a unit the rule now refuses. That would be found by tracing the first differing
  checkpoint to its command. Only an owner decision could turn it into a re-record. In that case the
  splash-centres procedure would apply: the old behaviour reproduces every recorded checksum on native debug and
  release and on `wasm32-wasip1` release and debug, then the replay is re-recorded with a proof header. No schema
  byte would move.
- **Old replays.** A replay recorded before this plan that holds a sieged tank replays differently. No such replay is
  kept.
- **Throughput.** One catalog lookup (already made) and one stance read per recipient of a plain order. No step
  that runs every tick changes, and the bench is unaffected.

---

## 6. Acceptance (principled properties, fixed before implementation)

No acceptance is a figure taken from a seed. Tests find units by property (an anchoring mode with or without
`hold_position`, an armed or unarmed structure, a mobile armed unit), never by slug in production code (C5).

- **P-1 (the simulation admits by stance).** For every shipped archetype, in each of its stances (default, every
  mode, and each direction of every switch while it channels), for `Stop` and `HoldPosition`, given immediately and
  queued: after the tick, the order reached the unit exactly when `settled_stance_orders` holds it. A unit it did
  not reach is equal, field for field, to the same unit in a world that received no command. The test is exhaustive
  over this set.
- **P-2 (`ToSelection`).** One command naming a sieged tank and a marine-class unit: `HoldPosition` reaches the
  marine only, and `Stop` reaches both. A command whose every recipient refuses changes nothing.
- **P-3 (client = simulation).** For every selection in the anchored-commands P-1 set, extended with an armed and an
  unarmed completed structure and every mixture of two: the card shows Stop, Hold and Attack exactly when
  `any_recipient` holds for `Stop`, `HoldPosition` and an `AttackTarget`. For Stop and Hold, the simulation, stepping
  a clone of the world with the command, delivers the order to at least one unit exactly then.
- **P-4 (B-171, the default stance against the data).** For every shipped archetype, `stance_orders(None)` equals
  `{ stop: mobile || stance_armed(None), hold_position: mobile }`. In addition,
  `catalog_reference_values_tests` pins a table read from §2.1, keyed by the structures' `sc2_inspiration` field
  (test data, not production code): Photon Cannon, Missile Turret, Planetary Fortress, Spine Crawler and Spore
  Crawler give `(stop: true, hold_position: false)`; Command Center, Orbital Command, Nexus, Shield Battery and
  Bunker give `NONE`. Each row carries its citation.
- **P-5 (Stop on an armed structure does what Stop does).** Through input alone (`X`, and separately the card
  button), an armed structure first told to attack one enemy, then given Stop, carries no `Explicit` target after
  the tick. Its next target is the one an idle copy at the same spot would choose. An armed producing structure
  (found by property: armed, `trains_units` not empty) keeps its production queue and rally across the Stop.
- **P-6 (armed means what the simulation fires).** For every shipped archetype and each of its stances,
  `stance_armed` is true exactly when a unit spawned and switched into that stance has a weapon against ground or
  air (`weapon_against` with its `active_siege_weapon`).
- **P-7 (Attack by stance, through input).**
  - A sieged tank alone: `A` arms Attack, and a click on a visible enemy gives it `AttackTarget` (an `Explicit`
    target after the tick). A click on the ground is refused with the `EmptySelection` status, and the mode stays
    armed.
  - A burrowed Lurker analogue alone shows Attack, and A-click on an enemy attacks it.
  - An unarmed structure shows no Attack. An armed one does, and A-click on an enemy in its range attacks that enemy.
  - A mobile armed unit A-clicked on an enemy gets `AttackTarget` for it, not an `AttackMove`. A-clicked on the
    ground it attack-moves, as today.
  - A mixed selection (sieged tank and marine-class unit) A-clicked on the ground sends one `AttackMove` naming the
    marine only.
- **P-8 (zero recompilation, C6).** In a catalog edited at run time:
  - an armed structure whose weapons are removed loses Stop and Attack on the card and in the simulation's admission;
  - `siege_mode` with `hold_position: true` lets the simulation hold a sieged tank;
  - `burrowed` with its `ground_weapon_override` removed hides the burrowed Lurker's Attack.

  All three run without a rebuild.
- **P-9 (fixtures honest).** D-5's test: no matrix battle scripts a Stop or Hold Position that misses a unit it
  names.
- **P-10 (worlds).** §7's table holds byte for byte.
- **P-11 (no identity, no allocation).** No slug or mode name appears in the production changes (C5). The card's
  warm derivation of a selection with an armed structure and an anchored unit allocates nothing, which the existing
  warm-card allocation tests check, extended with that selection.

---

## 7. Worlds that move

**Commands** (the anchored-commands plan's, with outputs under `~/scratch/stance-orders/`). Copy the scripts from
`~/scratch/anchored-commands/scripts/` (`base.sh`, `measure.sh`, `compare.sh`, `client.sh`) and change the root path
in each. `matchup_trace` embeds the matrix fixture and the catalog it was compiled with, so it is rebuilt for each
tree.

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
R=~/scratch/stance-orders
$R/scripts/base.sh <wp>        # builds matchup_trace and route_trace in release, runs measure.sh into world-<wp>
$R/scripts/compare.sh $R/world-wp0 $R/world-<wp>
$R/scripts/client.sh <wp>      # client targets in release, one at a time under ~/scratch/gate.lock
```

Run these in the background, one at a time across worktrees (charter §7; check `quota -s` and `df -h ~/scratch`
first).

| World | WP1 (fixture) | WP2 (simulation rule, B-171, B-172) | WP3 (client Attack, B-173) |
|---|---|---|---|
| Matrix, seeds of record and 16 (`matrix.csv`, `matrix.md`) | **37a, 37b, 39a, 39b may move; predicted byte-identical** (§3.3). Every other battle byte-identical | byte-identical to WP1 (after WP1 no scripted order is refused) | not run (client only) |
| Battles 31 (100 seeds), 33 (64 seeds) | byte-identical (no mode, no Stop or Hold) | byte-identical | not run |
| Showcase and showcase effects, default seed and 1–8 | byte-identical | byte-identical: no scenario or pilot issues Stop or Hold (`assets/scenarios/` holds neither; the automaton pilots, **verify**) | not run |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | unchanged (§5) | unchanged |
| Client skirmish stage log (`skirmish_match_tests`) | identical | identical: the pilot issues no Stop or Hold to a structure or anchored unit (**verify**) | identical: the pilot's attacks go to units armed by default and never sieged (**verify**: whether `standard_automaton` builds and sieges a tank; if so, its `AttackTarget` admission is unchanged, since siege is armed in both stances) |

**What WP1 records for 37a, 37b, 39a and 39b**, in the evidence, on all 20 seeds, before and after: wins, survivors,
end tick and the red lead's fate. If any row differs, it also records the first tick where a sieged tank's
`current_target` differs between the two runs, with the two targets and both engagements, traced with
`matchup_trace --battle <name> --seed <seed>` and its unit CSV.

---

## 8. Stop rules

1. **No golden or sandbox checksum moves in any package** (§5). A move is traced to its command and stops the
   package. It is not a re-record.
2. **WP1: a matrix outcome that moves** is traced to its first differing tank target (§7).
   - If the cause is the Hold/Seek bookkeeping of §3.3, the prediction was wrong. The evidence records why. The
     battle's expectation is re-recorded from what the battle is meant to test (the pulled tank dies in 37a and not
     in 37b; the cloud flips 39a and not 39b), with the trace as evidence and the old line kept in the comment.
   - If a battle's claim no longer holds on every seed, WP1 stops, because the battle's design depended on an order
     the game does not have, and goes to the orchestrator.
   - Any other cause stops WP1.
3. **WP2 and WP3 change no measured world** (§7). A difference stops the package.
4. **No existing test is loosened.** A test that fails because it expected an anchored unit or a structure to take
   a refused order, or expected the old Attack entry, is rewritten from P-1 to P-7 and listed in the As built. Any
   other failure is a finding and stops the package.
5. **No unit identity in code**, and no tuning constant. Every rule is the data's (§2.1) or an existing engine rule.

---

## 9. Client consequences

- `battle_pipeline/commands.rs`: `capable`'s `AttackTarget` and `AttackMove` arms (D-4), with their comments citing
  S3, S5, S6 and this plan.
- `battle_hud/catalog.rs`: `default_armed`, `mode_armed` (D-4).
- `battle_hud/card.rs`: the Attack entry by settled stance, and a helper shared with `settled_orders` (D-4).
- `battle_input/mod.rs`: `handle_targeting_pointer`'s unit pick for Attack (D-6).
- `battle_input/command_mode.rs`: `reconcile`'s viability for Attack (D-6), and doc updates for `AttackMove`.
- No `PlayerView` change: `mode_states` already carries the settled mode.
- No GPU or windowed check is needed. The Attack entry's drawing does not change, and the gate's `battle_hud/**`
  rule selects the HUD window GPU tests anyway.

---

## 10. File ownership

| File | WP | Change |
|---|---|---|
| `DESIGN.md` | WP0, WP4 | §4: B-171 to B-173 point to this plan (WP0) and are fixed (WP4); B-174 to B-176 registered (WP0); the HUD row (WP4) |
| `docs/plans/README.md` | plan, WP4 | Row |
| `docs/evidence/2026-11-23-stance-checked-orders.md` (new) | WP0 to WP4 | §7, red runs, sources |
| `crates/sim_core/tests/fixtures/matchup_matrix.ron` | WP1 | D-5 (37a, 37b, 39a, 39b orders and comments; expectations only by §8 rule 2) |
| `crates/sim_fixtures/src/matchup.rs` | WP1 | The rewritten standing-order test and the P-9 test (D-5) |
| `crates/sim_core/src/catalog.rs` | WP2 | `stance_armed`; `stance_orders(None)` (D-2, D-3); unit tests |
| `crates/sim_core/src/entity.rs` | WP2 | `settled_armed` next to `settled_stance_orders` (D-3) |
| `crates/sim_core/src/world/commands.rs` | WP2 | `issue_plain_order` (D-1) and its doc |
| `crates/sim_core/tests/stance_orders_tests.rs` | WP2 | P-1, P-2, P-6, the simulation half of P-8 |
| `crates/sim_core/tests/catalog_reference_values_tests.rs` | WP2 | P-4 |
| `crates/sim_core/tests/runtime_catalog_tests.rs` (or the test that injects RON) | WP2 | P-8 |
| `crates/client/src/battle_hud/card.rs` (tests) | WP2 | An armed structure shows Stop and not Hold; `a_structure_shows_neither_stop_nor_hold` stays for the unarmed factory |
| `crates/client/src/battle_pipeline/commands/tests.rs` | WP2, WP3 | P-3 (WP2: Stop and Hold with structures; WP3: Attack) |
| `crates/client/src/battle_pipeline/commands.rs` | WP3 | D-4 |
| `crates/client/src/battle_hud/catalog.rs`, `crates/client/src/battle_hud/card.rs` | WP3 | D-4 |
| `crates/client/src/battle_input/mod.rs`, `crates/client/src/battle_input/command_mode.rs` | WP3 | D-6, D-7 |
| `crates/client/tests/battle_mode_tests.rs` | WP3 | P-7 (anchored units) |
| `crates/client/tests/battle_card_tests.rs` | WP2, WP3 | P-5 (WP2), P-7's armed structure (WP3), and any slot test of the Planetary Fortress analogue (R-2) |
| `crates/client/tests/battle_input_tests.rs`, `crates/client/tests/battle_context_order_tests.rs` | WP3, only if they fail | Rewritten by §8 rule 4 |
| `docs/plans/2026-11-23-anchored-commands.md` | WP4 | A closing line: B-171 to B-173 fixed by this plan |
| `docs/plans/2026-11-05-sc2-deviation-fixes.md` | WP4, **verify** | Only if it lists B-171 to B-173 |

Nothing under `crates/sim_core/src/movement/` changes. B-176 is deferred for that reason.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`.
Write the failing tests first and paste their red run into the hand-back. Run `cargo gate quick` and the tests the
change needs, including the `Slow` targets named in the package. A verifier then runs `cargo gate package` (Q0061).
Rules for the code: no `unwrap`; `expect` only with `invariant:`; no catalog identity in production Rust (C5); tests
find units by property. Check `docs/questions/open/` before the last commit. All five packages may run as one branch
and one session, one commit each, with one `package` gate on the final tree (charter §4.3). Each package's §7
measurement is still taken against the tree before it.

### WP0: records and baseline (XS, docs only)

- `DESIGN.md` §4: B-171, B-172 and B-173 read "open; plan 2026-11-23-stance-checked-orders". B-174, B-175 and
  B-176 are registered (D-8), and the next free number becomes B-177.
- Evidence file created, with §2.1's sources.
- Measure WP0's tree into `world-wp0` (§7).
- **Gate:** docs-only `package`.

### WP1: the four battles restated (S, test data)

Failing tests first (red on WP0's tree): P-9's test fails on exactly 37a, 37b, 39a and 39b. The rewritten
standing-order test passes on both trees, because it no longer uses a sieged tank.

Then the change: D-5's orders and comments.

**Acceptance:** P-9; §7's WP1 column against `world-wp0`; `sim_core/sc2_who_wins_matchup_matrix_tests` (`Slow`)
run before hand-back; `sim_fixtures` tests.

### WP2: the simulation's rule; Stop for armed structures (M, `sim_core`, client tests, B-171, B-172)

Failing tests first (red on WP1's tree):
- `stance_orders_tests.rs`: P-1, P-2, P-6, P-8's simulation half. These do not compile until `stance_armed` and
  `settled_armed` exist. Once a stub returns the old answers, P-1's sieged-tank Hold and the armed structure's Stop
  are red.
- `catalog_reference_values_tests`: P-4.
- `catalog.rs` units: the default-stance rule for a mobile, an armed immobile and an unarmed immobile archetype.
- Client: the card unit test "an armed structure shows Stop and no Hold", and P-3's Stop and Hold half in
  `battle_pipeline/commands/tests.rs` with the two structures added to the set. Through input in
  `battle_card_tests`: P-5.

Then the change: D-1, D-2, D-3. Grep for every `HoldPosition` and `Stop` issued straight to a world in tests
(`crates/sim_core/tests/`, and `crates/client/tests/`, including the GPU targets `stance_gpu_tests`,
`concealment_gpu_tests`, `mode_atlas_gpu_tests`, `status_gpu_tests`, `rally_gpu_tests`, `runtime_battle_tests`).
Each one that sends one to a unit whose stance lacks it is rewritten by §8 rule 4, and listed.

**Acceptance:** P-1, P-2, P-3 (Stop, Hold), P-4, P-5, P-6, P-8 (simulation); §7's WP2 column against WP1's tree; the
goldens on the four builds through the package gate. Before hand-back, run `client/battle_card_tests` and
`client/battle_mode_tests` (`Slow`, selected by the `sim_core` rule).

### WP3: Attack by stance (M, `client`, B-173)

Failing tests first (red on WP2's tree):
- `card.rs` units: a sieged tank, a burrowed Lurker analogue and an armed structure show Attack; an unarmed
  structure does not; a burrowing Lurker analogue shows it and an unburrowing one does not (the settled stance).
- `battle_pipeline/commands/tests.rs`: P-3's Attack half; `AttackTarget` admitted for a burrowed and a burrowing
  Lurker analogue and for an armed structure; `AttackMove` refused for anchored units and admitted for the
  marine-class member of a mixed selection.
- `command_mode.rs` units: Attack mode stays viable for an anchored-only selection and ends for an unarmed one.
- `battle_mode_tests.rs` (through input): P-7's anchored cases. `battle_card_tests.rs`: P-7's armed structure and
  the mobile unit's A-click on an enemy.
- P-11: the warm-card allocation test extended with an armed structure and an anchored unit.

Then the change: D-4, D-6, D-7.

**Acceptance:** P-3, P-7, P-8 (client), P-11; the skirmish stage log (§7). Before hand-back, run the `Slow` client
targets that this package's code paths reach: `client/battle_input_tests`, `client/battle_context_order_tests`,
`client/battle_card_tests`, `client/battle_card_path_tests`, `client/battle_mode_tests` and
`client/battle_hud_tests`. These are outside `quick` (the production-queue plan's Amendment 1 lesson).

### WP4: bookkeeping (XS)

- `DESIGN.md`: B-171 and B-172 fixed (WP2's commit), B-173 fixed (WP3's), with the evidence. The HUD row mentions
  Attack by stance and the unit-aimed A-click.
- Evidence: §7's comparisons, the test list and the red runs.
- This plan's `## As built`; README row `Done`; the anchored-commands plan's closing line.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where | Gate selection |
|---|---|---|---|
| Fixture | No scripted order misses its unit; the standing order survives a switch whose stance takes it | WP1: `sim_fixtures/src/matchup.rs` units | `sim_fixtures` crate; `sim_core` crate (the fixture) |
| Matrix | The four battles' claims still hold | WP1: `sim_core/sc2_who_wins_matchup_matrix_tests` | `Slow`, run explicitly before hand-back |
| Rule | Stance → Stop, Hold and armed, every stance and switch; the simulation's admission per unit | WP2: `stance_orders_tests` (`Fast`), `catalog.rs` units | `sim_core` crate rule |
| Data | The default stance against the sourced table; run-time edits | WP2: `catalog_reference_values_tests`, runtime-catalog test | `catalog` rule and the `sim_core` crate |
| Admission | Client equals simulation for Stop, Hold and Attack | WP2 and WP3: `battle_pipeline/commands/tests.rs` (`client/lib`, `Slow`) | `battle_pipeline/**` |
| Card | Stop for armed structures; Attack by settled stance; no allocation | WP2 and WP3: `card.rs` units, `battle_card_tests` (`Slow`, 2 threads) | `battle_hud/**` |
| Input | `X` and the button on an armed structure; A-click on units and ground for anchored, mobile and mixed selections | WP2 and WP3: `battle_card_tests`, `battle_mode_tests` (`Slow`, 2 threads) | `client` crate |
| Neighbours | Nothing else in input broke | WP3: `battle_input_tests` (`Slow`, 2), `battle_context_order_tests` (`Slow`, 3), `battle_card_path_tests` (`Slow`, 2), `battle_hud_tests` (`Slow`, 2) | `client` crate |
| Worlds | §7 | WP1 and WP2 (measure), WP2 and WP3 (skirmish log) | — |

The gate catalog (`crates/gate/suites.ron`) needs no new rule: the touched test files are already catalogued or fall
under their crate's rule.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | §3.3's prediction is wrong and a sieged tank chooses differently without Hold | 37a/b or 39a/b move | §8 rule 2: traced and re-recorded from the battle's intent, or WP1 stops |
| R-2 | The Planetary Fortress analogue's train entries move one slot and key (D-4), and a test clicks them by slot | `package` fails | Search `battle_card_tests`, `battle_card_path_tests` and `skirmish_match_tests` for the fortress by property before WP3; a test that clicks by slot finds the entry by label instead (§8 rule 4) |
| R-3 | A test or GPU fixture holds or stops a structure or sieged unit straight through the world | Red after WP2 | WP2's grep (§11) |
| R-4 | The skirmish pilot sieges tanks and relies on the active-stance `AttackTarget` arm | Stage log differs in WP3 | The settled and active stances agree except while switching; a difference stops WP3 (§8 rule 3) and is traced |
| R-5 | §2.2 rule 2's readings (Shield Battery's removed entry, the Nexus's unmet requirement) are wrong | Two structures' Stop differs | Both are unarmed here, so a correction would need an authored default stance (D-2's way out) |
| R-6 | Q0098 is answered (b) | D-7 reverses | A client admission arm plus a movement-admission rule, a later package after the movement branch (B-176) |
| R-7 | The movement branch touches `entity.rs` near `settled_stance_orders` | Conflict | Rebase before the gate; the new query is a few lines |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-174** (the unburrowed Lurker's Attack), **B-175** (Attack on a friendly unit), **B-176** (movement admission
  by stance): D-8.
- **Queued Stop and Hold behind a switch** that has not started: the same settled-stance rule as the client. What
  the game does with an order queued for a stance the unit is not yet in is B-96's open question.
- **`Idle`**: not a player command (no ability in the data), so it is left unchecked in the simulation.
- **A mode's `Some(None)` weapon override and `air_weapon_override`** are not applied to a live unit
  (`apply_mode_transition`). D-3's query follows the live unit. No shipped mode uses either. This is a
  finding for the abilities plan (deviation-fixes WP18), not a numbered row, because it changes nothing that ships.
- **The Bunker's redirected Stop and Attack** (S10): the Bunker analogue carries no passengers. That belongs to the
  transport work.
- **The Nexus's Stop and Attack** under `PurifyNexusRequirements` (S9): read as absent in multiplayer, **verify**.
- **Card layout coordinates**: the card orders root entries and has no rows and columns, so Attack's (0, 4) place
  in the data is not reproduced (phase-5 card design).
- **Siege Mode's 0.5 s `Abils` delay**: unchanged detail difference (anchored-commands §14).

---

## 15. Draft owner question (for the orchestrator to file; not filed by this plan)

```markdown
# Q0098: What does a sieged tank, burrowed Lurker or armed structure do with Attack on the ground?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** (a), the click is refused for units that cannot walk.

## Question

Units that cannot walk (a sieged Siege Tank, a burrowed Lurker, a Photon Cannon) have an Attack button in
StarCraft II 5.0.14. Plan `2026-11-23-stance-checked-orders` makes it work on enemy units: A, then click an enemy,
attacks it. The game data does not say what happens when such a unit is told to attack a point on the ground. Its
button even has its own face, `AttackBuilding`, whose tooltip is not in the local data.

- **(a) Refused (default).** For a selection of only such units, the click does nothing and the status line says no
  unit can do that. The Attack cursor stays on, so you can click an enemy next. Walking units in a mixed selection
  still attack-move.
- **(b) Accepted as "attack-move in place".** The unit drops its current orders, including a focused target, and
  attacks whatever is in range, as after Stop. This needs a simulation change in movement admission, which waits
  for the movement work under way.

Recommendation: (a), unless you know the game does (b). Either way it is small: it only matters when every
selected unit is anchored.

## Answer

```

---

## 16. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/
  {AbilData,UnitData}.xml` (`5444e12f13`), read with `grep`, `sed` and `awk` (for each unit, its `AbilArray` and
  `LayoutButtons` across all eight layers). There is no `ButtonData` or `RequirementData` locally.
- No web source was used. Items marked **verify** wait for the editor's documentation or an in-game check.
- No prototype. §3.3's prediction is reasoned from the code, and WP1 measures it.

---

## As built

Implemented on branch `stance-orders-plan` in one session, one commit per package, each measured against the tree
before it and against WP0 ([evidence](../evidence/2026-11-23-stance-checked-orders.md)).

| WP | Commit | What landed | Worlds |
|---|---|---|---|
| WP0 | `fe9891eb` | B-171 to B-173 point here; B-174 to B-176 registered; evidence with §2.1's sources; baseline (483 files) | — |
| WP1 | `31d222da` | D-5: 37a/37b tank squads `order: Some(None)`, 39a/39b no side order; P-9 and the rewritten standing-order test | none moved (483 files, differing 0); 37a, 37b, 39a, 39b byte-identical, as §3.3 predicted |
| WP2 | `48041dc5` | D-1 (`issue_plain_order`), D-2 (`stance_orders(None)`), D-3 (`stance_armed`, `settled_armed`); B-171, B-172 fixed | none moved; goldens and sandbox checksum unchanged |
| WP3 | `e4cb94fa` | D-4 (card and admission by settled armed), D-6 (A-click on an enemy attacks it, mode viability), D-7 (ground refused for anchored-only); B-173 fixed | skirmish stage log identical |
| WP4 | this commit | Records | — |

Deviations from the text above, none of which changes a decision:

1. **P-9 also checks the stance.** Before WP2 the simulation delivered a scripted Hold to a sieged tank, so a test
   that only read `is_holding_position` after tick 1 would have passed on the WP0 tree. P-9 asserts that the named
   unit's settled stance takes the order *and* that the unit holds (or, for Stop, carries no order but an acquired
   attack). It failed on exactly 37a, 37b, 39a and 39b, as planned. 39a and 39b drop the side's `order` field (its
   default is `None`) rather than writing `order: None`.
2. **P-1 is tested against a forced copy.** `SimWorld` has no `Clone`, so each case builds three identical worlds:
   one commanded, one untouched and one whose unit is given the order straight through `Entity::issue_order`. A
   taken order leaves the commanded unit equal to the forced one, a refused order equal to the untouched one, entity
   for entity and by world checksum. Gas extractors are left out (they need a geyser to stand on).
3. **P-3's simulation half commands one member at a time** in a rebuilt world (the set is built deterministically
   and checked identical), since admission is per unit (P-2) and a world cannot be cloned. Delivery to a selection is
   then "delivered to some member". The Attack half compares the card with admission of an attack on a ground and on
   an air enemy, since the card's entry names no target.
4. **`can_execute` aims with the settled stance's weapon.** D-4 moved `capable`'s `AttackTarget` arm to the settled
   stance; the target-reach check beside it read the active siege weapon, which would refuse an attack from a
   burrowing Lurker analogue (armed only once burrowed). It now reads the settled mode's replacement ground weapon,
   the weapon `apply_mode_transition` will set. For a unit not switching this is the active weapon, as before.
5. **`any_attacker`** (`battle_pipeline/commands.rs`) is the Attack mode's target-free question: a completed owned
   member armed in its settled stance. `reconcile` keeps the Attack mode while `any_recipient` of an `AttackMove`
   or `any_attacker` holds (D-6's "fewer branches" shape: one arm, no order set).
6. **Tests rewritten by §8 rule 4:** `stance_orders_tests`' old default-stance test (replaced by the mobile-or-armed
   rule); `catalog.rs`'s speed-0 walker assertion; `multi_strike_timing_tests::q0058_an_explicit_order_ends_a_channeled_burst`,
   whose scratch shooter had speed 0 and so no longer takes Hold Position (the bench now gives it a speed of 0.1;
   nothing tells it to walk); the burrowed Lurker analogue's card test, which now expects Attack. The P-5 card test
   finds its structure, enemies and producing structure by property and holds the two enemies through `world_mut`
   (setup); the first attack on the structure is setup too, since before WP3 no input could give it.
7. **`ArchetypeCommands::armed` and `mobile`** are kept as the plan says, but no production code reads them after
   WP3 (the Attack entry was their last reader); they stay for the HUD catalog's public surface and its tests.
8. **WP2's commit carries `cargo fmt`'s reflow of WP1's `matchup.rs` test** (formatting only).

Findings, not changed here:

- A foundation of an armed structure would take a Stop in the simulation (its archetype's default stance is
  armed), while client admission refuses every order but cancel to a foundation. No shipped flow sends one; the
  game gives a structure under construction no `stop`. It belongs with the foundation rules, not this plan.
- §14's finding stands: a mode's `Some(None)` ground override and `air_weapon_override` are not applied to a live
  unit; `stance_armed` follows the live unit (P-6).

**Q0098 answered (a)** by the owner (refused, the Attack cursor stays on), the plan's default, which WP3 implements;
the question is closed on `main` (`docs/questions/closed/0098-anchored-attack-on-ground.md`). The answer's one added
detail, a status line saying that no unit can do that, is met by the existing `EmptySelection` text, "Order
rejected: select units that can perform this order" (`battle_pipeline/status.rs`, outside this plan's files),
which `battle_mode_tests::a_sieged_tank_attacks_a_clicked_enemy_and_refuses_the_ground` asserts. StarCraft II's own
wording, "Must target a unit", is not copied.
