# Blueprint: Stop and Hold Position for Anchored Units, by Stance (B-98)

This plan gives every unit the plain orders its current stance has in StarCraft II 5.0.14, on the command card and
in the client's admission alike:

- **Stop reaches anchored units.** A sieged Siege Tank analogue (`railgun_crawler` in `siege_mode`) and a burrowed
  Lurker analogue (`burrow_scorpion` in `burrowed`) take Stop, by the `X` key and by the card's Stop button. Stop
  clears the unit's orders, among them a focused target, and the unit picks its next target by itself.
- **Hold Position follows the stance's data.** The burrowed Lurker analogue takes Hold Position. The sieged tank does
  not, because 5.0.14's `SiegeTankSieged` carries no `move` ability, and Hold Position is a command of `move`.
  B-98's text, "the game's sieged tank has both", read the card layout and missed the ability list. §2 corrects it.
- **One rule, written as data.** Each mode in `units.ron` states which of Stop and Hold Position a unit in it
  accepts. A read-only `sim_core` query turns that into the set for a unit's settled stance. The card and the
  admission both use that query, so a button is shown exactly when pressing it would reach a unit.

The simulation is not changed: Stop and Hold Position are already carried out for any unit (§3.2), and no hashed
state, checksum or world moves (§5, §7).

A full plan is needed (charter §6) because the change spans `sim_types`, `sim_core`, the catalog and the client.
It also decides the scope of four neighbouring rows, B-95 to B-98 (§8).

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-23 against `main` at
`2f317d0f`.
- Line numbers drift, so search for the quoted symbol.
- Claims not confirmed while planning are marked **verify**.
- Later decisions are appended as `## Amendment N (orchestrator, YYYY-MM-DD): <decision>`, numbered from 1.
- An `## As built` section follows the merge.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0051 (charter §8): mechanics follow StarCraft II 5.0.14; an unnamed difference is a bug | Standing rule | §2, every D |
| Q0052: sanctioned deviations. S-1 covers RON data instead of editor data (the stance's order set) | Closed | D-1 |
| Q0061: implementation agents hand back after `cargo gate quick`; a verifier runs `cargo gate package` | Standing rule | §11 |
| Q0002: scoping is the agent's call if written down | Standing rule | §8, §14 |
| The owner's guidance: no fitting to fixed seed sets; acceptance by principled properties | Followed | §6 |
| [Q0070](../questions/closed/0070-lurker-without-burrow-button.md) (option 1, the client mode commands) | Closed | This plan extends that plan's card |
| **None new** | The data settles every rule. If the owner prefers Hold Position on a sieged tank anyway (a deviation that allows more, charter §8), that is one data flip (P-3) | §2.2 |

---

## 1. Summary

1. **Data (D-1).** `UnitModeSpec` gets `orders: StanceOrders { stop: bool, hold_position: bool }`. It is required in
   `units.ron`, as `leave_cast_ticks` is, so a mode written without it fails to load. Shipped values are
   `siege_mode (stop: true, hold_position: false)` and `burrowed (stop: true, hold_position: true)`, from the
   morphed unit types' `AbilArray`. The default stance stays as it is today: a mobile archetype accepts both orders
   and an immobile one accepts neither. Armed structures, which do have Stop in 5.0.14, are a registered row
   (B-171), not part of this change.
2. **One rule in `sim_core` (D-2).** `UnitArchetype::stance_orders(Option<&UnitModeSpec>)` and
   `Entity::settled_stance_orders(&UnitArchetype)` read the stance the unit is in or is switching to. This is the
   `settled_mode` that the mode card and `immobile_after_channel` already use. Both queries are `&self` and
   allocate nothing.
3. **Admission (D-3).** In `battle_pipeline/commands.rs`, `capable` admits `Stop` when the settled stance's set has
   `stop`, and `HoldPosition` when it has `hold_position`. `Move`, `AttackMove`, `Idle` and `Load` keep the
   mobility rule.
4. **Card (D-4).** `derive_fixed` shows Stop when any completed selected unit's settled stance accepts Stop, and
   shows Hold the same way, each on its own. The stance comes from `PlayerView::mode_states` and the HUD catalog's
   set for each stance, which comes from the same `sim_core` function. Keys stay as they are (`X`, `H`).
5. **No world moves (§7).** The simulation's handling of Stop and Hold Position, the checksum and the fixtures are
   all unchanged. Every measured world is expected to be byte-identical; any difference is a defect.
6. **Neighbours (§8).** B-95, B-96 and B-97 are deferred, with reasons. Three new rows are registered (B-171 to
   B-173): Stop on armed structures, the simulation's own stance check for Stop and Hold, and a card whose Attack
   entry ignores the stance.

---

## 2. What StarCraft II does

### 2.1 Sources

All from SC2GameData `5444e12f13` (5.0.14), local copies under `~/scratch/combat_wp3_data/<layer>/`, read as data. The
override chain is `core → liberty → libertymulti → swarm → swarmmulti → void → voidmulti → balancemulti`. Every unit
below was checked in every layer; a layer not cited does not touch its `AbilArray` or `CardLayouts`.

| # | Fact | Data | Source |
|---|---|---|---|
| S1 | Stop is its own ability (`CAbilStop`, link `stop`); Hold Position is a command (`HoldPos`) of the move ability (`CAbilMove`, link `move`) | `CmdButtonArray index="HoldPos"` under `CAbilMove`; `CAbilStop id="stop"` | `core` AbilData:552–554 (in the `CAbilMove` default), :1362, :1371 |
| S2 | Stop and Hold Position go to every selected unit that has them (`ToSelection`) | `Flags index="ToSelection"` | `core` AbilData:553, :1015–1016 |
| S3 | **Sieged Siege Tank:** `stop`, `attack`, `Unsiege`; **no `move`** | `AbilArray` | `liberty` UnitData:5172–5174; no later layer changes it (`libertymulti`:493, `swarm`:1899, `swarmmulti`:629, `voidmulti`:1579, `balancemulti`:1477 hold no `AbilArray`) |
| S4 | Its layout still lists Move, Stop, Hold Position and Patrol | `LayoutButtons` | `liberty` UnitData:5184–5188 |
| S5 | **Unsieged Siege Tank:** `stop`, `attack`, `move`, `SiegeMode` | `AbilArray` | `liberty` UnitData:5108–5111, :5114–5115 |
| S6 | **Burrowed Lurker:** `attack`, `BurrowLurkerMPUp`, `BurrowLurkerMPDown`, `move`, then `stop`, `LurkerHoldFire`, `LurkerRemoveHoldFire` | `AbilArray` | `liberty` UnitData:793–796; `void` UnitData:767–769 |
| S7 | Its layout: Stop (0, 1), Attack (0, 4), Move (0, 0), Hold Position (0, 2), Patrol (0, 3), Unburrow (2, 4) | `LayoutButtons` | `liberty` UnitData:799–804 |
| S8 | **Burrowed Widow Mine:** `WidowMineUnburrow`, `WidowMineAttack`; no `stop`, no `move` | `AbilArray` | `swarm` UnitData:3312–3313 |
| S9 | **Liberator in Defender Mode:** `stop`, `attack`, the two morphs, `LiberatorAATarget`; no `move` | `AbilArray` | `void` UnitData:3381–3385, :3389 |
| S10 | **Rooted Spine and Spore Crawlers:** `stop`, `attack`, uproot; no `move`. Uprooted: `stop`, `move`, root | `AbilArray` | `liberty` UnitData:8652–8654, :8784–8786; :8712–8714 |
| S11 | **Burrowed Roach:** `burrowedStop` (its Stop button needs Tunneling Claws), then `stop` in multiplayer; `move` | `AbilArray`; `CAbilStop id="burrowedStop"` | `liberty` UnitData:7281–7282, AbilData:3626–3628; `voidmulti` UnitData:1469 |
| S12 | **Armed structures** have Stop: Photon Cannon, Missile Turret, Planetary Fortress | `AbilArray Link="stop"` | `liberty` UnitData:11083, :6064, :10696 |
| S13 | Siege Mode's ability set changes 0.5 s into the morph; Unsiege's and the Lurker's burrow have no `Abils` section, so their sets change at the start | `SectionArray index="Abils"` | `liberty` AbilData:1308–1311 (Siege Mode), :1334–1341 (Unsiege); `swarm` AbilData:1663–1673 (the burrow) |
| S14 | Siege Mode is not `Interruptible`; the Lurker's burrow is, and has a Cancel command | `Flags` | `liberty` AbilData:1301–1327; `swarm` AbilData:1658–1659 |
| S15 | In 5.0.14 the burrowed Lurker shares the unburrowed Lurker's subgroup | `SubgroupAlias value="LurkerMP"` | `voidmulti` UnitData:990; `balancemulti` UnitData:921 |

### 2.2 The rules that follow

1. **A unit has Stop exactly when its current unit type has a `stop` ability (S1).** That holds for the sieged tank
   (S3), the burrowed Lurker (S6), the Liberator in Defender Mode (S9), the rooted crawlers (S10) and every mobile
   form in the data. It does not hold for the burrowed Widow Mine (S8).
2. **A unit has Hold Position exactly when its current unit type has `move` (S1).** The burrowed Lurker has it (S6),
   and so do burrowed units in general (S11). The sieged tank (S3), the Liberator in Defender Mode (S9), the rooted
   crawlers (S10) and the burrowed Widow Mine (S8) do not.
3. **A layout button whose ability the unit lacks is not shown.** The sieged tank's layout lists Move and Patrol
   (S4), and a sieged tank cannot be ordered to move, so the layout entries without an ability cannot be live
   buttons. **Verify** against the editor's documentation or an in-game card. B-98's citation of
   `UnitData:5185–5186` is the layout (S4), not the ability list (S3). The corrected row says Stop only (WP0).
4. **A press reaches every selected unit that has the command (S2).** A sieged tank selected with Marines takes no
   Hold Position, and the Marines do. This is the card's existing "union of the selection" rule (phase-5 D-7).
5. **What Stop does** (the `CAbilStop` default): the unit drops its orders, a focused target among them, and
   acquires its next target by itself. Nothing in the data treats an anchored unit's Stop differently.
   **Verify** that Stop does not end a running morph that is not `Interruptible` (S14). The simulation already
   leaves the channel running (§3.2).
6. **What Hold Position does** for a unit that cannot move: it stays put, which it does anyway, and engages enemies
   in range without chasing. For an anchored unit this means "acquire in range", and the simulation does exactly
   that (§3.2). The Lurker's separate Hold Fire (S6) is a different ability (§14).
7. **When the set changes during a switch (S13).** The leaving switches and the burrow change the set at their
   start. Siege Mode changes it 0.5 Normal s after its start (8.93 ticks, read as the morph's
   `RandomDelayMax` of 0.5 is read for `cast_random_delay_max_ticks` 8.9286). This plan judges a switching
   unit by its settled stance, as the mode card does (D-2), so a tank that is sieging loses Hold at the start
   rather than 0.5 s in. That is a detail difference, recorded in §14.

---

## 3. Diagnosis (on `2f317d0f`, read from the code)

### 3.1 The client

- **The card** (`crates/client/src/battle_hud/card.rs`, `derive_fixed`) shows Stop and Hold whenever any completed
  selected archetype is `mobile`. `ArchetypeCommands::mobile` is `!archetype.is_immobile_by_default()`
  (`battle_hud/catalog.rs`, `commands_of`), a fact about the type and not the stance. A sieged tank or a burrowed
  Lurker therefore **shows both buttons**.
- **Admission** (`crates/client/src/battle_pipeline/commands.rs`, `capable`) admits `Move`, `HoldPosition`, `Stop`
  and `Idle` only for a unit that is `!entity.immobile_after_channel(archetype) && max_speed > 0`. The comment
  calls this "a UI selection policy". A sieged or burrowed unit is therefore **refused both orders**.
- **The result.** Selected alone, an anchored unit shows Stop and Hold, but pressing either (button, `X` or `H`;
  `battle_input/mod.rs` `order_selection`) leaves `submit_selection` with no recipient. The press is rejected with
  `SubmissionError::EmptySelection` and its status line. B-98's wording, "no Stop or Hold on the card", describes
  what the player gets, not what is drawn: the card offers two buttons that do nothing. WP1's first tests pin this
  (red on `main`).
- **Finding F-1 (the card and admission disagree).** The card judges by type and admission judges by stance.
  Nothing makes them answer the same question. The mode entries already avoid this, because the card and
  admission both read `settled_mode` (plan `2026-11-09-client-mode-commands` D-3). Stop and Hold never got that
  treatment.

### 3.2 The simulation

- `world/commands.rs` sends `Stop` and `HoldPosition` to `issue_plain_order`, which commits them for every owned
  unit through `Entity::issue_order` → `commit_order` (`entity.rs`). No admission refuses them for an immobile,
  channelling or moded unit.
- **Stop** (`commit_order`): a winding shot is cancelled (`Windup → Ready`), a backswing ends (`Backswing →
  Cooldown`), velocity becomes zero, and the queue, funnel waypoints and Hold are cleared. A running channel
  (`channeling_remaining_ticks`, `pending_ability`) is not touched, so a switch in progress finishes. With an empty
  queue, `target_acquisition.rs` `engagement_of` returns `Seek`, and `decide_acquisition` scans `stationary` for an
  immobile unit. The unit re-acquires on the next acquisition pass. This is rule 5.
- **Hold Position**: `is_holding_position = true` and the queue becomes `[HoldPosition]`. `engagement_of` returns
  `Hold`, and the scan is `stationary`. For an immobile unit, `Hold` and `Seek` differ only in how the current
  target is kept and released (`ReleaseHoldTarget`); both acquire in range and neither moves. This is rule 6.
- **Finding F-2 (the simulation's rule is permissive).** Because there is no stance check, a scripted command can
  hold a sieged tank. The matrix does: battles 37a, 37b, 39a and 39b give their sieged tanks `order: HoldPosition`
  with `mode: Some("siege_mode")` (`crates/sim_core/tests/fixtures/matchup_matrix.ron`). In those battles the
  order comes after the siege cast on tick 0 (plan `2026-11-09-client-mode-commands` D-6a). A player cannot do this
  once D-3 is in place. Making the simulation refuse it would change those four battles' hashed order queues and
  needs its own measured change, so it is registered (B-172) and deferred (§8).
- The goldens never siege or burrow (`golden_replay.ron`: `red_siege` attack-moves and gets `Stop` unsieged on tick
  760; `golden_replay_orders.ron` has no mode). No pilot issues Stop or Hold: the only issuers in non-test code are
  the client's input, the lab sandbox (`lab_ui.rs`) and `sim_fixtures::matchup`.
- The catalog is not hashed (`checksum/encoding.rs` hashes no `UnitModeSpec`; plan `2026-11-09-client-mode-commands`
  D-6 relied on the same fact for `leave_name`).

---

## 4. Design decisions

### D-1. The stance's plain orders are data

In `crates/sim_types/src/commands.rs`:

```rust
/// The plain orders a unit accepts in one stance (`StarCraft` II: whether the stance's unit type
/// carries the `stop` ability, and the `move` ability whose `HoldPos` command is Hold Position).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StanceOrders {
    /// Whether a `Stop` order reaches the unit.
    pub stop: bool,
    /// Whether a `HoldPosition` order reaches the unit.
    pub hold_position: bool,
}
```

with the associated constants `StanceOrders::ALL` and `StanceOrders::NONE`. `UnitModeSpec` gains
`pub orders: StanceOrders` (`#[serde(default)]` on the `sim_types` struct, which serializes it, as for its other
fields). `UnitModeSpecDef` in `crates/sim_core/src/catalog.rs` gets `orders: StanceOrders` with **no default**, so
the loader requires it in RON, as it requires `leave_cast_ticks` ("required, so a catalog written before the field
fails to load"). The field's doc names S1 to S3 and S6.

Shipped values (`assets/catalog/units.ron`), each with a source comment:

| Mode | `orders` | Source |
|---|---|---|
| `railgun_crawler` `siege_mode` | `(stop: true, hold_position: false)` | S3 |
| `burrow_scorpion` `burrowed` | `(stop: true, hold_position: true)` | S6 |

No validation rule is added. Every combination exists in the data: both (S6), Stop only (S3, S9, S10), and neither
(S8). Mobility and the order set are independent facts, as `is_immobile` and `unpushable` already are.

**The default stance is not authored here.** An archetype's own stance keeps today's rule: `ALL` for a mobile
archetype (`!is_immobile_by_default()`, which covers `max_speed > 0`) and `NONE` otherwise. This is exact for every
shipped mobile unit, since each has `stop` and `move` in 5.0.14, as the unsieged tank (S5) and the unburrowed Lurker
(`liberty` UnitData:727–728) do. Authoring the default stance would touch every row of `units.ron` and
`structures.ron` and would bring in armed structures' Stop (S12). That is B-171 (D-6).

### D-2. One rule in `sim_core`

```rust
impl UnitArchetype {
    /// The plain orders a unit of this archetype accepts in `stance`: the mode's authored set, or,
    /// for the default stance (`None`), every plain order when the archetype is mobile and none
    /// when it is not. Pure; the one rule the client's card and admission share.
    #[must_use]
    pub fn stance_orders(&self, stance: Option<&UnitModeSpec>) -> StanceOrders;
}

impl Entity {
    /// The plain orders this unit accepts in the stance it is in or is switching to
    /// ([`Self::settled_mode`]). `archetype` must be this unit's.
    #[must_use]
    pub fn settled_stance_orders(&self, archetype: &UnitArchetype) -> StanceOrders;
}
```

`stance_orders` goes next to `is_immobile_by_default` (`catalog.rs`), and `settled_stance_orders` goes next to
`immobile_after_channel` (`entity.rs`). Neither file is under `movement/`.

The settled stance is used, not the active one, for three reasons. It is what the mode card and `capable`'s
mobility already use (client-mode-commands D-3, D-7). It matches the data for the leaving switches and the burrow
(S13: their sets change at the start). And it keeps a press and the tick that runs it from disagreeing while a unit
switches. The 0.5 s of Siege Mode's `Abils` delay is the one difference (§2.2 rule 7, §14).

### D-3. Client admission

In `capable` (`battle_pipeline/commands.rs`), the combined arm is split:

```rust
UnitOrder::Stop => entity.settled_stance_orders(archetype).stop,
UnitOrder::HoldPosition => entity.settled_stance_orders(archetype).hold_position,
UnitOrder::Move { .. } | UnitOrder::Idle => mobile,
```

The comment changes from "a UI selection policy" to cite S1 to S3 and this plan. `AttackMove` and `Load` keep
`mobile`. `prepare_tick` revalidates through the same `capable`, so a unit that settles into a stance without
Hold between the press and the tick is dropped from the command, as `retain_casters` and `validate_pending`
already do for other orders. `any_recipient` follows automatically.

For an unsieged unit the result is the same as today: `ALL` when mobile, and `mobile` covers
`immobile_after_channel`, which for a unit with no mode is its `is_immobile`. There is one difference. A unit made
immobile by something other than a mode (the legacy `CloakToggle` effect, `world/abilities.rs`, which no shipped
unit uses) keeps Stop and Hold under the new rule, because its stance is still the default one. The data has no
such unit. This is recorded in §14.

### D-4. The card

`ArchetypeCommands` (`battle_hud/catalog.rs`) gains `default_orders: StanceOrders` and
`mode_orders: Vec<StanceOrders>`, indexed like the archetype's `modes`. Both are filled once per session in
`commands_of` through `UnitArchetype::stance_orders`, so the HUD calls the simulation's function and keeps no copy
of the rule. `derive_fixed` becomes:

```rust
/// Stop and Hold, each shown when a completed selected unit's settled stance accepts it.
fn derive_fixed(model: &HudModel, facts: &[SelectedFact], card: &mut CommandCard)
```

For each non-foundation fact, it reads the unit's set: the `ModeStateView` of `fact.id` in `model.view.mode_states`
(binary search, as `mode_side` does), then `mode_orders[settled_mode]`, or `default_orders` when there is no state
or no settled mode. Stop is shown if any unit's set has `stop`, and Hold if any has `hold_position`. The two fixed
slots stay at their positions and keys (`STOP_KEY`, `HOLD_POSITION_KEY`). A slot whose order no selected unit takes
is empty: the unit does not have the command (rule 3), so it is hidden and not locked. Phase-5 D-8 ("a lock is
shown") is about commands a unit has but cannot use yet.

`mobile` stays on `ArchetypeCommands` for the Attack entry (D-6, B-173). The derivation allocates nothing: a
binary search and two flags for each fact.

### D-5. Keys and mixed selections

`X` and `H` (`battle_input/mod.rs`, `order_selection`) and the card's buttons (`battle_hud/actions.rs`,
`CardAction::Order`) all go through `submit_selection`, so D-3 covers them all. A mixed selection gets each order
sent to the members that take it (S2). A selection where nobody takes it is refused with `EmptySelection`, as
today. There is no new status text.

### D-6. Records

- **B-98:** corrected text, "Anchored units are refused Stop and Hold Position although the card shows them; in
  5.0.14 the sieged tank has Stop (`liberty` UnitData:5172) and the burrowed Lurker has Stop and Hold Position
  (`liberty` UnitData:796, `void` UnitData:767)". Fixed by WP2.
- **B-171 (new): armed structures have no Stop.** 5.0.14's Photon Cannon, Missile Turret, Planetary Fortress and
  rooted crawlers have `stop` (S10, S12). The card shows no Stop for a structure, and admission refuses it.
  Deferred: it needs the default stance's set authored per archetype (D-1), which touches every row.
- **B-172 (new): the simulation does not check the stance for Stop and Hold Position.** Only the client applies
  `StanceOrders`. A scripted command (matrix battles 37a, 37b, 39a and 39b) holds a sieged tank, which 5.0.14 has
  no command for (S3). Deferred: the fix is a refusal in `issue_plain_order` through `settled_stance_orders`, which
  changes those battles' hashed order queues. It needs a measured package and new fixture orders.
- **B-173 (new): the card's Attack entry ignores the stance.** It is shown for a type that is `mobile && armed`, so
  a burrowed Lurker analogue gets none. 5.0.14's burrowed Lurker has `attack` (S6, S7), though a right-click on an
  enemy already attacks (`capable`'s `is_sieged && siege_weapon` arm). A sieged tank gets an Attack entry whose
  click on the ground (`AttackMove`) admission refuses. What 5.0.14 does with an anchored unit's attack on a point
  is **verify**. Deferred: it needs that source and a stance-aware "armed".
- **Detail differences (no number, the deviation-fixes plan's Amendment 2 class):** Siege Mode's set changes at the
  switch's start, not 0.5 s in (S13). The burrowed Lurker's Move and Patrol (S7) are not offered, because Patrol is
  not modelled and a move order to a unit of speed 0 has no effect in either game (**verify** for 5.0.14).

---

## 5. Determinism and checksum

- **No hashed state changes.** `StanceOrders` is catalog data, and the catalog is not hashed (§3.2). No entity field,
  order or encoding changes. **The checksum schema stays 36.** No "next free at merge" number is taken.
- **No golden re-record.** `golden_replay`, `golden_replay_orders` and `DEFAULT_SANDBOX_CHECKSUM` are expected to be
  unchanged on all four builds (Linux, Windows, macOS ARM64, `wasm32-wasip1`), because no code path that runs in a
  step changes. The two new queries are `&self`, and the only callers are the client and tests.
- If a golden or a sandbox checksum moves, that is a defect of the package (§10 rule 1). It is not a re-record,
  and the golden proof procedure is not used.

---

## 6. Acceptance (principled properties, fixed before implementation)

Each property is a statement about the rule. None is a figure taken from a seed.

- **P-1 (the card is the admission).** For every selection drawn from {worker; tank unsieged, sieging, sieged,
  unsieging; Lurker unburrowed, burrowing, burrowed, unburrowing; a completed structure; a foundation} and every
  mixture of two of them, the card shows Stop exactly when `any_recipient(world, player, selection, Stop)` holds,
  and Hold exactly when `any_recipient(…, HoldPosition)` holds. The test is exhaustive over that set, not a sample.
- **P-2 (the stance decides, by the data).** For a unit in each stance of the shipped catalog,
  `settled_stance_orders` equals its mode's authored `orders`, or the default stance's rule. A switching unit
  answers for the stance it settles in.
- **P-3 (zero recompilation, C6).** A catalog injected at run time with `siege_mode` `hold_position: true` makes the
  sieged tank's card show Hold and admission accept it. One with `burrowed` `stop: false` hides Stop for the
  burrowed Lurker. Both run without rebuilding.
- **P-4 (sourced values).** The shipped `orders` equal §2.1 S3 and S6. `catalog_reference_values_tests` pins them
  with the citations.
- **P-5 (Stop on an anchored unit does what Stop does).** Through input alone: a sieged tank told to attack one
  enemy and then given Stop (`X`, and separately the card button) has an empty queue after the tick, and the next
  acquisition pass chooses its target by the ordinary ranking, the same one an idle sieged tank placed at that
  spot would choose. A Stop given while it sieges leaves the switch running to its end.
- **P-6 (Hold on a burrowed unit).** Through input: a burrowed Lurker analogue given `H` has `[HoldPosition]` and
  attacks an enemy that walks into range. A sieged tank given `H` alone gets nothing and the status line reports
  the refusal. A sieged tank with a Marine-class unit given `H` sends one command whose entities are the Marine
  only (S2).
- **P-7 (no world moves).** §7's table holds byte for byte.
- **P-8 (no identity, no allocation).** No slug or mode name appears in the client or `sim_core` change (C5).
  `derive_fixed` allocates nothing once warm; the existing card allocation test covers it, extended with an anchored
  selection.

---

## 7. Worlds that move

**None are expected to.** The simulation's step is unchanged (§5). The catalog gains a field that no step reads.

| World | WP1 | WP2 | Why |
|---|---|---|---|
| Matrix (seeds of record and 16), battles 31 (100 seeds) and 33 (64 seeds) | byte-identical | (client only; not run) | No step reads `orders`. 37a, 37b, 39a and 39b keep their scripted Hold (B-172) |
| Showcase and showcase effects, default seed and 1–8 | byte-identical | (not run) | As above |
| `golden_replay`, `golden_replay_orders`, `DEFAULT_SANDBOX_CHECKSUM` | unchanged | unchanged | §5 |
| Client skirmish stage log (`skirmish_match_tests`) | identical | identical | The pilot issues no Stop or Hold (§3.2), and its other orders' admission is unchanged |

**Commands** (the splash-centres plan's, with outputs under `~/scratch/anchored-commands/`; scripts copied from
`~/scratch/splash-centres/scripts/`, and `client.sh` from `~/scratch/production-queue/scripts/`):

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>
cargo build --release --locked -p sim_core --example matchup_trace --example route_trace
mkdir -p ~/scratch/anchored-commands/bin-<wp> && cp "$CARGO_TARGET_DIR"/release/examples/{matchup_trace,route_trace} ~/scratch/anchored-commands/bin-<wp>/
~/scratch/anchored-commands/scripts/measure.sh ~/scratch/anchored-commands/bin-<wp> <tree> ~/scratch/anchored-commands/world-<wp>
~/scratch/anchored-commands/scripts/compare.sh ~/scratch/anchored-commands/world-wp0 ~/scratch/anchored-commands/world-<wp>
# expected: "differing=0"
```

`world-wp0` is measured on `main` at the branch point. Run these in the background, one at a time across
worktrees (charter §7). No prototype was built, because a prototype would measure a constant.

---

## 8. Scope: B-95 to B-98

| Row | Decision | Why |
|---|---|---|
| **B-98** | **In: fixed by WP2**, with the corrected text (D-6) | The subject of this plan. It is a client rule plus a data field, and it changes no world |
| **B-95** (a queued switch toggles whatever stance the unit has) | **Deferred** to the abilities plan (deviation-fixes WP18) | The fix is a directional `CastAbility` or a separate ability for each direction. That changes a hashed order, so it needs a schema and a golden proof. Its cause is unrelated to Stop and Hold |
| **B-96** (an order queued behind an anchoring switch stays at the front) | **Deferred** (WP18 research). This plan adds a mitigation and a lead | Mitigation: with Stop on anchored units, a player can now clear the stuck order with `X`. Lead for the research: 5.0.14's sieged tank has no `move` (S3), so a queued Move behind Siege Mode has no ability to run in the game, while the burrowed Lurker keeps `move` (S6). What the engine does with such an order is still unsourced. The fix touches `walking_goal` in `sim_core/src/movement/`, which another branch owns now |
| **B-97** (the Lurker's burrow cannot be cancelled) | **Deferred** to its own plan | It is a new simulation mechanic (an `Interruptible` channel that ends early and restores the stance, S14) and a new card entry, Cancel. Whether Stop cancels an interruptible morph is **verify**. This plan keeps Stop from touching a channel (§3.2), so it does not prejudge that |

Three rows are registered by WP0 and deferred: B-171, B-172 and B-173 (D-6).

---

## 9. Client consequences

- `battle_pipeline/commands.rs`: `capable`'s `Stop` and `HoldPosition` arms (D-3), and their comment.
- `battle_hud/catalog.rs`: `default_orders`, `mode_orders` (D-4).
- `battle_hud/card.rs`: `derive_fixed` by stance (D-4).
- No change to `battle_input/` or `battle_hud/actions.rs`: their paths already go through `submit_selection`.
- No PlayerView change: `mode_states` already carries the settled mode of every moded unit of the viewer.
- No GPU or windowed check is needed. The fixed slots' geometry and presses are already tested headless
  (`battle_card_tests`), and the drawing of a fixed slot does not change. The gate's `battle_hud/**` rule selects
  the HUD window GPU tests anyway.

---

## 10. Stop rules

1. **No world moves.** If any row of §7 differs, or if a golden or sandbox checksum differs on any build, the
   package stops and the difference goes to the orchestrator with its trace.
2. **No existing test is loosened.** A client test that fails because it expected an anchored unit's Stop or Hold to
   be refused, or the old card, is rewritten from P-1 and P-6 and listed in the As built. Any other failure is a
   finding and stops the package.
3. **No unit identity** in code, and no default for `orders` in the RON loader.

---

## 11. Work packages (commit order)

Each package: one branch from `main`, its own worktree and `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree>`.
The implementation agent writes the failing tests first, runs `cargo gate quick` and the tests its change needs,
and hands back. A verifier runs `cargo gate package` (Q0061). §7's comparison runs in WP1, the only package that
touches `sim_core`.

### WP0: records (S, docs only)

- `DESIGN.md` §4 register: B-98 corrected (D-6); B-171, B-172 and B-173 added, with this plan's link.
- `docs/plans/2026-11-05-sc2-deviation-fixes.md`: the B-98 row's text, to match.
- `docs/plans/README.md`: this plan's row, `Proposed`.
- **Gate:** docs-only `package`.

### WP1: data and the shared rule (M, `sim_types`, `sim_core`, catalog)

Failing tests first (red on `main`, because the field and queries do not exist):
- `crates/sim_core/tests/stance_orders_tests.rs` (new, with a `//!` doc): P-2 for each shipped stance and each
  direction of a switch (constructed worlds through `SimWorld::spawn_unit` and `execute_ability`); `stance_orders`
  of an immobile-by-default archetype is `NONE`.
- `catalog_reference_values_tests`: P-4.
- `runtime_catalog_tests` (asset_baker) or the `sim_core` runtime-catalog test that injects RON: a mode without
  `orders` fails to load with the typed error; P-3's simulation half (the query flips).
- Unit tests in `catalog.rs` for the default-stance rule.

Then the change: D-1 and D-2. Every `UnitModeSpec` literal gets `orders`: `sim_types`'s own tests,
`published_actions.rs`, and `sim_core/tests/{combat_correctness,burrow_attack,transport_ownership,
spells_and_abilities}_tests.rs`, together with the RON strings that `catalog/validation.rs` and
`world/abilities.rs` tests load.

**Acceptance:** P-2, P-3 (simulation half), P-4; §7 on this tree against `world-wp0`.

### WP2: client (M, `client`)

Failing tests first (red on WP1's tree):
- `battle_hud/card.rs` unit tests: a sieged tank shows Stop and no Hold; a burrowed Lurker shows both; a
  sieging tank shows Stop and no Hold; an unsieging tank shows both; a sieged tank with a worker shows both;
  a structure shows neither.
- `battle_pipeline/commands/tests.rs`: P-1, exhaustive over §6's set, comparing `derive_fixed`'s fixed slots
  with `any_recipient`. Also `a_unit_leaving_its_anchoring_mode_may_be_told_to_move`'s neighbours: Stop admitted
  for a sieged tank, Hold refused for it; both admitted for a burrowed Lurker; a mixed command keeps only the
  taking members.
- `crates/client/tests/battle_mode_tests.rs` (through input alone): P-5 and P-6 (`X` and the card's Stop on a
  sieged tank after a right-click attack; `H` on a burrowed Lurker that then kills an approaching enemy; `H` on a
  sieged tank refused with its status).
- `crates/client/tests/battle_card_tests.rs`: `stop_and_hold_appear_and_their_keys_still_work` stays as it is, and a
  sibling test does the same for an anchored selection: the sieged tank's Hold slot is empty and its Stop slot
  works by mouse and by key.
- P-3's client half: the HUD catalog built from an injected catalog with `siege_mode` `hold_position: true`
  shows Hold for a sieged tank.
- The card allocation test extended with an anchored selection (P-8).

Then the change: D-3 and D-4.

**Acceptance:** P-1, P-3, P-5, P-6, P-8; the client package gate; the skirmish stage log (§7).

### WP3: bookkeeping (S)

- `DESIGN.md`: B-98 fixed with WP2's commit and the evidence; the HUD row mentions stance-aware Stop and Hold.
- `docs/evidence/2026-11-23-anchored-commands.md`: §7's comparisons, the test list, and the sources of §2.1.
- This plan's `## As built`; README row `Done`.
- **Gate:** docs-only `package`.

---

## 12. Test plan (summary, per `docs/guides/test-gates.md`)

| Layer | What it proves | Where | Gate selection |
|---|---|---|---|
| Data | Required field, sourced values, run-time read | WP1: `catalog_reference_values_tests`, the runtime-catalog test, `catalog.rs` units | `catalog` rule (`assets/catalog/**`) and the `sim_core` crate |
| Rule | Settled stance → order set, every stance and switch | WP1: `stance_orders_tests` (new) | `sim_core` crate rule (also selects `client/battle_mode_tests`) |
| Admission | Stop and Hold by stance; mixed selections; revalidation at the tick | WP2: `battle_pipeline/commands/tests.rs` | `battle_pipeline/**` |
| Card | Fixed slots by stance; P-1 against admission; no allocation | WP2: `battle_hud/card.rs` units, `battle_card_tests` | `battle_hud/**` (HUD window GPU tests ride along) |
| Input | `X`, `H` and the buttons on anchored units, through input alone | WP2: `battle_mode_tests`, `battle_card_tests` | `client` crate (`Slow`, `test_threads` as catalogued) |
| Worlds | §7 | WP1 (measure), WP2 (skirmish log) | — |

The gate catalog (`crates/gate/suites.ron`) needs no new rule: the new `sim_core` test file is picked up by the
crate rule, and the client targets already exist. If `stance_orders_tests` is fast, it is classed `Fast`.

---

## 13. Risks

| # | Risk | Effect | Mitigation |
|---|---|---|---|
| R-1 | Rule 3 is wrong and the game shows a dead Hold button on a sieged tank | A detail of the card differs | Pressing it would do nothing in the game either (no `move`, S3); a data flip (P-3) if a source shows otherwise |
| R-2 | A test elsewhere asserts that an anchored unit's Stop is refused (for example in `battle_input_tests` or `battle_context_order_tests`, both `Slow` and outside `quick`) | `package` fails after `quick` passed | WP2 runs those two targets before it hands back (the production-queue plan's Amendment 1 lesson); stop rule 2 |
| R-3 | `mode_states` is not sorted by entity, so the binary search misses | Wrong card | `mode_side` already relies on the order; WP2's card tests cover a selection of several moded units |
| R-4 | The movement branch changes `settled_mode` or `immobile_after_channel` | A conflict in `entity.rs` | Both live outside `movement/`; WP1 rebases before its gate |
| R-5 | Stop on a tank that is sieging is now admitted, and a player expects it to cancel the siege | Surprise | It does not cancel in 5.0.14 either (Siege Mode is not `Interruptible`, S14); the cancel belongs to B-97 |

---

## 14. Out of scope and decisions recorded (Q0002)

- **B-95, B-96, B-97**: §8.
- **B-171** (Stop on armed structures), **B-172** (the simulation's own stance check), **B-173** (stance-aware
  Attack): D-6.
- **The Lurker's Hold Fire** (`LurkerHoldFire`, `LurkerRemoveHoldFire`, S6) and the Ghost's: not modelled; they
  belong to the abilities plan (deviation-fixes WP18).
- **The Widow Mine and the Liberator's Defender Mode**: not modelled as modes (B-27 for the Liberator). Their sets
  (S8, S9) can be written in `orders` when they are, with no code change.
- **Burrowed units in general** (Roach, Zergling, Infestor): no shipped archetype has a burrow mode besides the
  Lurker analogue (B-38, B-69). The burrowed Roach's Stop needing Tunneling Claws (S11) is noted for that work.
- **Siege Mode's set changes 0.5 s into the switch** (S13); here it changes at the start (D-2). A detail difference.
- **Move and Patrol for the burrowed Lurker** (S7): not offered (D-6, detail difference).
- **A unit made immobile outside a mode** keeps the default stance's orders (D-3); no shipped unit is one.
- **Subgroups**: the burrowed Lurker shares its subgroup with the unburrowed one in 5.0.14 (S15), which corrects
  plan `2026-11-09-client-mode-commands` §2.5's note (from `liberty` alone). That plan's card shows both
  directions regardless, so nothing changes here. WP3 notes the correction in that plan's As-built section.
- **The lab inspector** (`lab_ui.rs`) sends Hold through its own sandbox path; it is unchanged.

---

## 15. Material (scratch, not committed)

- Game data: `~/scratch/combat_wp3_data/{core,liberty,libertymulti,swarm,swarmmulti,void,voidmulti,balancemulti}/
  {AbilData,UnitData}.xml` (`5444e12f13`), read as data with `grep`, `sed` and `awk`. There is no `ButtonData`
  locally. Cards were read from `UnitData`'s `AbilArray` and `CardLayouts`.
- No web source was used. Rule 3 and the items marked **verify** wait for an in-game card or the editor's
  documentation.
- No prototype (§7).

---

## As built

Implemented on branch `anchored-commands-plan` in one session, one commit per package, measured against the WP0 tree
([evidence](../evidence/2026-11-23-anchored-commands.md)).

| WP | Commit | What landed | Worlds |
|---|---|---|---|
| WP0 | `00de5b4f` | B-98 corrected; B-171 to B-173 registered; deviation-fixes row; README row; baseline of every §7 world (483 files) | — |
| WP1 | `5f553bad` | `StanceOrders` (D-1), required in `units.ron`; `UnitArchetype::stance_orders` and `Entity::settled_stance_orders` (D-2) | none moved (483 files, differing 0); goldens and sandbox unchanged, schema 36 |
| WP2 | `5212e264` | `capable` by stance (D-3); `default_orders` and `mode_orders` in the HUD catalog and `derive_fixed` by stance (D-4); B-98 fixed | skirmish stage log identical |
| WP3 | this commit | Records | — |

Deviations from the text above, none of which changes a decision:

1. **P-5's "empty queue after the tick".** The tick that carries the Stop also runs the acquisition pass, which
   already puts the unit's own `AttackTarget { origin: Acquired }` in front of its queue. The test therefore checks
   that no ordered (`Explicit`) target is left after the tick, and then that the unit's target is the one it chose
   idle at that spot. This is rule 5 (the unit acquires its next target by itself), seen one pass earlier.
2. **P-6's burrowed unit is the shipped Lurker analogue.** It now has media and is presentable, so
   `battle_mode_tests` finds it by data (an anchoring, arming mode with `hold_position`) and needs no stand-in. The
   approaching enemy is a presentable ground walker set to one life and given a `Move` through `world_mut` (setup,
   not an order under test).
3. **The card allocation test (P-8)** is `battle_hud::card::tests::a_warm_card_of_an_anchored_selection_allocates_no_new_text`,
   a sibling of the two warm-card tests, which checks that both fixed slots keep their text buffers when warm.
   `sync_steady_state_tests` passes unchanged.
4. **`battle_mode_tests`' `frail_enemy`** is split into `enemy_structure` (full life) and `frail_enemy` (one life),
   with the same behaviour for its existing callers, because P-5 needs enemies that survive a few shots.
5. **The subgroup note (§14)** went into plan `2026-11-09-client-mode-commands` as a new closing section, since that
   plan has no As-built section.
6. **`UnitModeSpec` literals in tests** take `StanceOrders::ALL`; none of those tests reads the field.

The gate catalog needs no rule: `stance_orders_tests` runs in under a second and is `Fast` by default.
