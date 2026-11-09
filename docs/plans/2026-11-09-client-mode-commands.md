# Blueprint: Mode Commands in the Skirmish Client (Siege Mode and Burrow; Q0070 option 1)

This plan lets a player switch a unit's mode from the skirmish client: Siege Mode and Tank Mode for the Siege Tank
analogue (`railgun_crawler`), Burrow and Unburrow for the Lurker analogue (`burrow_scorpion`). It does so for every
catalog ability that toggles a declared mode, so the card is built from capabilities and the client names no
identity (charter §3.3, C5). It is the "client package" that the default of
[Q0070](../questions/open/0070-lurker-without-burrow-button.md) asks for. Without it, a trained Lurker analogue
can never attack in a skirmish, because plan `2026-11-05-sc2-deviation-fixes` WP13b (bug B-42) made it attack only
when burrowed.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-09 against `main` at `a227e7d`.
The Lurker analogue's mode and its Burrow ability are on branch `worktree-agent-af3d5253dd330d7b1` at `e97f5c2`,
which is about to merge; WP0 waits for that merge. Line numbers drift, so search for the quoted symbol. Claims that
could not be confirmed while planning are marked **verify**. No planning probe was run (no builds, by instruction),
so every expectation about checksums is unmeasured until WP1 measures it.

This is a full plan, not a short note (charter §6): it spans `sim_types`, `sim_core`, `sim_fixtures`, `client` and
`gate`, and it changes simulation behaviour, by a small measured amount.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| [Q0070](../questions/open/0070-lurker-without-burrow-button.md) the Lurker without a Burrow button (on the B-42 branch) | Open; default option 1, "keep the fix, add a client Burrow/Siege command". This plan is that command | D-1, WP6 |
| [Q0069](../questions/open/0069-lurker-burrow-sections.md) when during its burrow the Lurker becomes burrowed | Open; unaffected. The card reads the stance the simulation reports, whatever Q0069 decides | §8 |
| [Q0051](../questions/closed/0051-sc2-mechanics-standing-rule.md) follow StarCraft II | Standing rule (charter §8) | §2.5, D-3 to D-7 |
| [Q0061](../questions/closed/0061-cheap-verification.md) who waits for gates | Standing rule (charter §4.3) | §5 |
| New question: per-ability hotkeys (number assigned when filed) | Filed by WP6 with the default "keys stay positional" | D-8, WP6 |

---

## 1. Summary

1. **Card entries come from data.** An archetype ability whose `target_kind` is `ToggleMode` and whose effect is
   `SwitchMode { mode_name }`, naming one of the archetype's own `modes`, becomes two card entries: one that enters
   the mode (labelled with the ability's `name`) and one that leaves it (labelled with a new catalog field,
   `leave_name`). Which entry a selected unit contributes depends on the stance it is in or is switching to. The
   client reads `AbilityTargetKind` and `AbilityEffectSpec`, which are mechanics; it never reads a slug or mode name
   as a literal (D-2, D-3).
2. **One submission path, with a direction.** A press goes to every selected unit that can switch in that
   direction now, one `PlayerCommand` per distinct `ability_index`, with Shift queueing. The client stores the
   direction with the pending command and checks it again when the tick takes the command, so a unit that has
   meanwhile switched is rejected, not toggled the wrong way (D-4, D-5).
3. **Two small simulation fixes, measured.** A mode switch given without Shift now replaces the unit's orders, as
   StarCraft II's non-transient morphs do. Today a tank sieged while it walks keeps its `Move` order and never fires
   again, because a `Move` at the front of the queue makes the unit `Busy` for target acquisition. And a queued
   switch at the front of an anchored unit's queue now runs, where today `walking_goal` stops every immobile unit
   before it reads the queue. Both changes are expected to leave every golden replay and matrix battle bit-identical;
   WP1 measures that (D-6).
4. **Read-only queries in `sim_core`, so client admission cannot drift.** `SimWorld::would_admit_cast` runs the
   simulation's own `admit_cast` without mutation, and two `Entity` queries report the stance a unit will settle in.
   `PlayerView` gains the viewer's mode states and locked abilities, so the HUD keeps reading only the view (D-9).
5. **Proof through input alone.** A new headless binary, `crates/client/tests/battle_mode_tests.rs`, boots the
   shipped skirmish, spawns the units through the harness as `battle_card_tests.rs` already does, and sieges,
   unsieges, burrows and attacks with keys and clicks only. The Lurker analogue kills an enemy after a key press,
   which is Q0070's proof (D-10).

What it does not do (§8): other abilities (targeted spells, Blink, hazards), autocast, cancelling a morph, subgroups,
per-ability hotkeys, Stop and Hold for anchored units.

---

## 2. What is known

### 2.1 How input becomes a `PlayerCommand` today

- **Keys and clicks.** `battle_hud/actions.rs`: `collect_card_keys` reads ordered `KeyboardInput` events and calls
  `press_key`, which finds the card slot whose `key` matches and calls `apply_card_action`; a click on a slot goes
  through `HudIntent` and `apply_hud_intent` to the same function. Shift (`SHIFT_MODIFIERS`) becomes `queued`. A
  disabled slot is refused with `SubmissionError::CardLocked(tooltip)`.
- **`CardAction::Order(order)`** calls `BattleCommandQueue::submit_selection(world, viewer, selection, order,
  queued)` (`battle_pipeline/commands.rs`). It keeps the selected members for which `can_receive_order` and
  `can_execute` hold and submits one `PlayerCommand` for all of them. `prepare_tick` revalidates every pending
  command with `validate` against the world of the tick that will execute it, and turns a failure into a
  `RejectedSubmission`.
- **`supported`** is exhaustive over `UnitOrder` and refuses `CastAbility` on purpose: "Abilities have no UI in Phase
  5, so this order is refused deliberately". `capable` returns `false` for `CastAbility`.
- **The existing pattern, Overclock (Stimpack).** The card shows `CardLabel::Overclock` when a selected archetype has
  `overclock` (`battle_hud/card.rs`, `overclock_entry`), disabled with `CardLock::NeedsResearch(upgrade)` until
  `PlayerView::overclock_ready` lists the type. The press sends `CardAction::Order(UnitOrder::UseStim)`, and
  `capable` admits a unit through `can_overclock`, which calls the simulation's own `Entity::can_pay_overclock` and
  `SimWorld::is_ability_unlocked`: "the one rule admission and the inspector share".
- **Mobility.** `capable` lets `Move`, `HoldPosition`, `Stop` and `Idle` reach only units with `!entity.is_immobile`
  and a positive `max_speed`, as a UI policy. `AttackTarget` reaches a unit that is armed or
  `entity.is_sieged && entity.siege_weapon.is_some()`, which a burrowed Lurker analogue satisfies on the B-42 branch
  (`apply_mode_transition` sets `is_sieged` when the mode replaces the ground weapon).

### 2.2 The command card (`battle_hud/card.rs`, Phase 5 decisions D-6 to D-8)

- Fifteen positional slots, keys by position only (`input_bindings.rs`, `COMMAND_CARD_KEYS`):
  `Q E R T Y / F G J K L / Z C V B N`. `W A S D` pan; Stop is `X` and Hold is `H` as fixed entries. "No key belongs
  to any unit" (phase-5 plan D-6, `input_bindings.rs` invariants).
- The card is the union of what the selection can do, with no subgroups (phase-5 D-7). A lock is shown, not hidden
  (D-8). Root order today: Attack (if any selected archetype is mobile and armed), Build, Return cargo, Overclock,
  Unload, Cancel build, then train and research options.
- `derive_card` is pure over `HudModel` and allocates nothing once warm. `CardLabel` is `Copy` and owns no string;
  `label_text` resolves it through `CommandCatalog`.
- `CommandCatalog` (`battle_hud/catalog.rs`) turns each archetype into `ArchetypeCommands` once per session: flags
  read from capabilities, never from identity (`overclock_unlock` finds the unlocking upgrade by
  `UpgradeEffect::UnlockAbility`).

### 2.3 How the simulation runs a mode switch

- **Data.** `UnitArchetype::abilities: Vec<AbilitySpec>` and `modes: Vec<UnitModeSpec>` (`sim_core/src/catalog.rs`).
  On `main`, only `railgun_crawler` has one: `name: "Siege Mode"`, `target_kind: ToggleMode`, `effect: SwitchMode(
  mode_name: "siege_mode")`, mode `siege_mode` (`cast_ticks` 72, `leave_cast_ticks` 63, random extras 8.9286,
  immobile, unpushable, the Crucio Shock Cannon as `ground_weapon_override`). The B-42 branch adds `burrow_scorpion`:
  `name: "Burrow"`, mode `burrowed` (`cast_ticks` 45, `leave_cast_ticks` 11, extras 4.4643, immobile, unpushable,
  cloaked, the Subterranean Spines as `ground_weapon_override`), and removes its default weapon. No other archetype
  has an ability (DESIGN.md register B-69).
- **Dispatch.** `world/commands.rs` `dispatch_cast`: for each owned recipient, if `cmd.queue` and the unit's
  `order_queue` is not empty, the `CastAbility` order is pushed onto the queue; otherwise
  `execute_ability_prevalidated` runs now. The non-queued path does not touch `order_queue`.
- **Admission.** `world/abilities.rs` `admit_cast(&self, …)` is read-only. It refuses (`Ok(None)`) a dead, carried or
  channelling unit, an unknown or locked ability (`is_ability_unlocked(arch, owner, &ability.name)`), a cooldown,
  missing energy and an out-of-range target, then calls `admit_effect`, which admits `SwitchMode` unconditionally.
- **Commit.** `commit_cast` pays, then for `SwitchMode` computes the channel with `mode_channel_ticks` (leaving when
  `active_mode` equals the mode) and calls `begin_mode_channel`, which sets `is_immobile`, `unpushable`,
  `channeling_remaining_ticks` and `pending_ability`; velocity and funnel waypoints are cleared. `upkeep_entity`
  counts the channel down and then calls `apply_mode_transition`, which toggles: in the mode → `leave_mode`,
  otherwise enter.
- **Queued casts.** `movement/locomotion.rs` `walking_goal` turns a front `CastAbility` into `MotionAction::Cast`,
  which runs `try_execute_ability_prevalidated` and pops the order. But `walking_goal` first returns for any unit
  that `is_on_hold() || is_immobile || channeling_remaining_ticks > 0`, so a sieged or burrowed unit never reaches a
  queued cast.
- **Acquisition.** `target_acquisition.rs` `engagement_of`: an empty queue, `Idle` or `AttackMove` is `Seek`, `Hold`
  is `Hold`, `AttackTarget` is explicit or acquired, and **any other front order (for example `Move`) is `Busy`**,
  which never acquires. A unit channelling is skipped.
- **A `Move` to a channelling unit** replaces its orders (`commit_order`, `handle_interrupting_move`) and leaves the
  channel running, so a tank told to move while it unsieges finishes the channel and then walks. Movement admission
  (`validate_movement_order`) does not refuse immobile units.

### 2.4 Tests and gates that already exist

- Card derivation: `battle_hud/card.rs` unit tests (for example
  `overclock_is_disabled_until_researched_and_names_what_to_research`). Presses with real input:
  `crates/client/tests/battle_card_tests.rs` (spawns units through `Harness::world_mut().spawn_unit`, as its
  invariants allow; `overclock_before_research_issues_nothing_by_mouse_or_key`) and `battle_card_path_tests.rs`
  (`research_through_the_card_then_overclock_works`).
- Admission: `battle_pipeline/commands/tests.rs`, including `absent_session_and_unsupported_capability_are_explicit`,
  which uses `CastAbility` as the unsupported order.
- Simulation: `crates/sim_core/tests/spells_and_abilities_tests.rs`; the B-42 branch adds
  `crates/sim_core/tests/burrow_attack_tests.rs` and a matrix side field `mode`, cast on the first tick by
  `sim_fixtures::matchup::mode_commands` **after** `side_commands` in the same bundle.
- Gate catalog `crates/gate/suites.ron`: client Harness binaries are `Slow` with `test_threads: Some(2)` or `Some(3)`
  (Amendment 4 of plan `2026-11-04-tiered-test-gates`: peak RSS near 6 GiB); `battle_hud/**` selects the two HUD
  window GPU tests, `battle_pipeline/**` the live-sync targets (`sync_allocation_count_tests`, `skirmish_match_tests`,
  `runtime_battle_tests`).

### 2.5 StarCraft II (5.0.14; SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`)

Local copies of the `Abil` files are under `~/scratch/sc2dev_wp11/src/`; `UnitData` and `GameHotkeys.txt` were read
from <https://github.com/SC2Mapster/SC2GameData/tree/5444e12f13> while planning.

| Behaviour | Data | Source |
|---|---|---|
| Siege Mode and Unsiege are two abilities (`CAbilMorph` `SiegeMode` → `SiegeTankSieged`, `Unsiege` → `SiegeTank`) | One button each, on different unit types | `liberty` AbilData:1301, :1328 |
| Burrow and Unburrow of the Lurker are two abilities (`BurrowLurkerMPDown`, `BurrowLurkerMPUp`) | One button each | `swarm` AbilData:1654, :1675 |
| No research needed in 5.0.14 | Siege Mode's `Requirements` cleared; the Lurker's burrow names none | `void` AbilData:806; `swarm` AbilData:1657 |
| Default hotkeys | Siege Mode `E`, Unsiege `D`, Burrow `R`, Lurker Unburrow `R`, Stimpack `T` | `liberty` GameHotkeys.txt:942, :1199, :115, :1035; `swarm` GameHotkeys.txt:78 |
| Card positions (row, column of the 5 × 3 grid) | Siege Mode (2, 0); Unsiege (2, 1); Lurker Burrow (2, 4); Lurker Unburrow (2, 4) | `liberty` UnitData:5118, :5189, :737, :801 |
| Sieged and unsieged tanks share a subgroup, so a mixed selection shows both buttons | `SubgroupAlias SiegeTank` on `SiegeTankSieged` | `liberty` UnitData:5182 |
| The burrowed Lurker has no `SubgroupAlias`, so mixed Lurkers are two subgroups (Tab switches) | only `LeaderAlias` | `liberty` UnitData:813 |
| Siege Mode goes to the whole selection | `CmdButtonArray` flag `ToSelection` | `liberty` AbilData:1305 |
| A morph is not transient, so a non-queued one replaces the unit's orders; Stimpack is transient and does not | `Flags Transient` on `Stimpack`, absent on both morphs (**verify** the flag's meaning in the editor documentation) | `liberty` AbilData:1231 |
| Both morphs suppress movement while they run | `SuppressMovement` | `liberty` AbilData:1307, :1333; `swarm` AbilData:1662 |
| The Lurker's burrow can be cancelled while it runs | `Interruptible`, `CmdButtonArray index="Cancel"` | `swarm` AbilData:1658-1659 |
| A sieged tank's card keeps Stop and Hold Position | `LayoutButtons` Stop (0, 1), HoldPos (0, 2) | `liberty` UnitData:5185-5186 |
| Autocast | Neither morph has an autocast button for players; the burrow validators in `void` ValidatorData:1021-1049 serve the computer AI (Q0070 option 3) | as cited in Q0070 |
| Shift queues an order behind the current ones | Standard command queueing (**verify** a written source, for example Liquipedia's hotkey or queueing pages) | — |

---

## 3. Design decisions

### D-1. Q0070 option 1, built without waiting for the answer

Q0070 is not blocking, and its default is this plan. If the owner answers option 2 or 3 before WP6, WP6 records the
answer and the orchestrator decides whether this plan continues; the client command is useful for the Siege Tank
analogue either way, so WP1 to WP5 do not wait.

### D-2. What becomes a card entry

A **mode ability** is an `AbilitySpec` of an archetype with `target_kind == AbilityTargetKind::ToggleMode` and
`effect == AbilityEffectSpec::SwitchMode { mode_name }` where `archetype.get_mode(mode_name)` exists. Only mode
abilities get entries. `SiegeModeToggle` and `CloakToggle` are legacy effects with no shipped user; they are left
out and recorded (§8).

Each mode ability yields an **enter** entry and a **leave** entry. The labels come from data:

- enter: `AbilitySpec::name` (shipped: "Siege Mode", "Burrow");
- leave: a new field `AbilitySpec::leave_name: Option<String>` (`#[serde(default)]`; shipped: "Tank Mode",
  "Unburrow", StarCraft II's button names).

Catalog validation (`sim_core/src/catalog/validation.rs`) gains one rule with a typed error: a `ToggleMode` ability
must have a `SwitchMode` effect naming a mode of its own archetype, and must have a non-empty `leave_name`. A shipped
catalog that breaks it fails to load, loudly (charter §1.1). Other target kinds must leave `leave_name` empty.

The HUD's `CommandCatalog` interns every distinct `mode_name` as a `ModeId(u16)`, assigned in catalog order (by
archetype, then by ability), and stores for each `ModeId` its enter and leave labels, taken from the first
archetype that declares it. Each `ArchetypeCommands` gets `modes: Vec<ModeCommand { mode: ModeId, ability_index:
u8, mode_index: u8, unlock: Option<u32> }>`; `unlock` generalizes `overclock_unlock` to
`UnlockAbility { target_slug, ability_slug == ability.name }`. Two archetypes sharing a `mode_name` (a future Roach
and Lurker both burrowing) therefore share one entry, as StarCraft II's shared Burrow button does. No string is
compared to a literal anywhere in the client.

### D-3. Which direction a selected unit contributes

The direction is read from the unit's **settled mode**: the mode it is in, or, while it channels a switch, the mode
it will be in when the channel ends. A new read-only `sim_core` query computes it, so the client has no copy of the
rule:

```rust
impl Entity {
    /// The mode this unit is in, or will be in when its current mode channel ends.
    pub fn settled_mode<'a>(&'a self, archetype: &'a UnitArchetype) -> Option<&'a UnitModeSpec>;
    /// Whether this unit will be anchored once its current channel, if any, has ended.
    pub fn immobile_after_channel(&self, archetype: &UnitArchetype) -> bool;
}
```

`settled_mode` reads `active_mode`, `channeling_remaining_ticks` and `pending_ability` exactly as `commit_cast` and
`upkeep_entity` use them: a pending `SwitchMode` whose mode equals `active_mode` settles in no mode, any other
pending `SwitchMode` settles in its mode. `immobile_after_channel` is `entity.is_immobile` for a unit that does not
channel, and otherwise the settled mode's `is_immobile`, or `archetype.is_immobile_by_default()` without one.

A unit whose settled mode is *M* contributes to *M*'s leave entry; any other holder of a mode ability for *M*
contributes to its enter entry. A selection of sieged and unsieged tanks therefore shows both Siege Mode and Tank
Mode, as StarCraft II does through `SubgroupAlias`. For the Lurker analogue StarCraft II would show the active
subgroup's one button; ours shows both, a consequence of phase-5 D-7 (no subgroups), recorded in §8.

### D-4. Admission: one rule shared with the simulation, plus direction

`supported` keeps refusing `CastAbility` on the generic paths (`submit`, `submit_selection`, `submit_roster_order`):
a cast needs a direction, which only the new path carries. Its comment changes to say so. The new path:

```rust
/// Which way a mode command switches.
pub enum ModeDirection { Enter, Leave }

impl BattleCommandQueue {
    /// Sends the switch of `mode_name` in `direction` to every member of `selection` that can make it now,
    /// one command per distinct ability index, in ascending index order.
    pub fn submit_mode_switch(
        &mut self, world: &SimWorld, player: PlayerId, selection: &[EntityId],
        mode_name: &str, direction: ModeDirection, queued: bool,
    ) -> Result<SubmissionSequence, SubmissionError>;
}
```

A member is a recipient when, in this order: `accepts_orders` holds for the player; it is owned, alive, completed and
not carried (`can_receive_order`); its archetype has a mode ability for `mode_name` (the first such ability, by
index); `world.would_admit_cast(entity, ability_index, None)` is true; and its settled mode matches the direction
(`Leave` exactly when it settles in `mode_name`). `SimWorld::would_admit_cast(&self, EntityId, u8, Option<SimVec2>)
-> bool` is new and returns `matches!(self.admit_cast(..), Ok(Some(_)))`: the simulation's own admission, read-only,
which covers channel, unlock, cooldown, energy and transport. A refusal of `admit_cast` by error (malformed data) is
not a recipient here and is reported by the tick, as today.

Consequences, all recorded: a unit already channelling is not a recipient (the simulation would refuse an immediate
cast; StarCraft II's interruptible burrow could be cancelled, §8). An empty recipient list is
`SubmissionError::EmptySelection`, as for every other order.

Recipients are grouped by `ability_index` (the matchup fixture already sends one command per unit for the same
reason: "each unit's archetype places the ability at its own index"). Each group becomes one
`PlayerCommand { order: CastAbility { ability_index, target_pos: None, target_entity: None }, queue: queued }`. The
press returns the sequence of the last command. Allocation happens on the user action only.

### D-5. Revalidation keeps the direction

`PendingCommand` gains `mode: Option<PendingMode { direction: ModeDirection }>`. `prepare_tick` validates a pending
command with `mode: Some(_)` by `validate_mode_switch`, which repeats D-4's member test for every listed entity
against the tick's world (the mode name is recovered from the recipient's ability at `ability_index`), and fails the
whole command with `SubmissionError::Capability(id)` on the first that no longer qualifies, as `validate` does.
Without this, a unit whose earlier queued switch completed between the press and the tick would be toggled back.

A queued switch (`queue: true` behind other orders) is stored in the unit's `order_queue` and runs later, as a toggle
of whatever stance the unit then has. The client cannot see a unit's queue, so it cannot refuse a second queued
switch that would undo the first. StarCraft II's morphs are separate abilities per direction, so its second press
would fail instead. Recorded as a bug for the deviation register (§8, proposed B-94), not fixed here: the fix is a
directional effect in the simulation, which belongs with the abilities plan (deviation-fixes WP18).

### D-6. Two simulation fixes (one cause per commit, measured)

**D-6a. A mode switch given without Shift replaces the unit's orders.** In `dispatch_cast`, on the non-queued path,
when `execute_ability_prevalidated` returns `true` and the ability's `target_kind` is `ToggleMode`, the unit's orders
are replaced as `Stop` replaces them, without Stop's weapon-phase changes: `order_queue.clear()`,
`is_holding_position = false` (velocity and funnel waypoints are already cleared by `commit_cast`). A refused cast
changes nothing. Why: StarCraft II's morphs are not transient (§2.5), and today a tank or Lurker sieged or burrowed
while it walks keeps a `Move` at the front of its queue, which `engagement_of` treats as `Busy`: it never fires. It
also walks to the old point after it unsieges. Transient abilities (the stim order) are untouched.

The matchup fixture casts `mode_commands` after `side_commands` in the same bundle, so D-6a would now clear the
side's first order. `BattleRun::new` is changed to put the mode commands first ("a player burrows, then gives the
order"). Commit order inside one tick: before, the side order was committed and then the cast cleared velocity and
waypoints; after, the cast runs and then the side order is committed, and neither step reads what the other writes
beyond velocity and waypoints, which both clear. The expected result is bit-identical state for every matrix battle,
including those with a `mode` side; WP1 measures it.

**D-6b. A queued switch at the front of an anchored unit's queue runs.** In `walking_goal`, a front `CastAbility`
of a unit that is not channelling becomes `MotionAction::Cast` before the immobile and hold checks return. Today a
sieged tank told to focus a target and then, with Shift, to unsiege keeps the `CastAbility` forever after the target
dies. Channelling units still stop first. Units that never hold a front `CastAbility` are untouched, so every golden
replay and matrix battle is expected to stay bit-identical.

**What is not changed.** No new persistent state, no checksum format change, no new order. The two queries of D-3 and
`would_admit_cast` are `&self`. `AbilitySpec::leave_name` is catalog data, outside the checksum.

### D-7. Mobility while a unit leaves a mode

`capable` uses `!entity.immobile_after_channel(archetype)` instead of `!entity.is_immobile` for `Move`,
`AttackMove`, `HoldPosition`, `Stop`, `Idle` and `Load`. Then a player can press Tank Mode and right-click the ground
at once, and the tank walks when the channel ends, as in StarCraft II; the simulation already behaves so (§2.3, last
point). Units that do not channel are judged as today. A unit entering a mode is anchored after its channel, so
moves to it stay refused, as today. Stop and Hold for an anchored unit stay refused (a pre-existing UI policy that
differs from StarCraft II, recorded in §8).

### D-8. Card placement and keys

Mode entries come right after Attack and before Build, in `ModeId` order, the enter entry of a mode before its leave
entry. Keys stay positional (phase-5 D-6). The resulting keys:

| Selection | Card | StarCraft II |
|---|---|---|
| Siege Tank analogue in tank mode | Attack `Q`, Siege Mode `E` | Siege Mode `E` |
| Sieged | Attack `Q`, Tank Mode `E` | Unsiege `D` (`D` pans here) |
| Mixed | Attack `Q`, Siege Mode `E`, Tank Mode `R` | `E` and `D` |
| Lurker analogue, unburrowed (unarmed, so no Attack) | Burrow `Q` | Burrow `R` |
| Burrowed | Unburrow `Q` | Unburrow `R` |

Per-ability hotkeys would break phase-5 D-6 ("no key belongs to any unit") and collide with the camera keys, and the
choice is the owner's: WP6 files a non-blocking question ("Should the command card use StarCraft II's per-ability
hotkeys, from catalog data, instead of positional keys?") with the default "keys stay positional". Until it is
answered the difference is a UI choice, not a unit mechanic, and is not a B-ID.

Locks (D-8 of phase 5, shown rather than hidden):

- `CardLock::NeedsResearch(upgrade)` when every contributor's `(unit_type, ability_index)` is in
  `PlayerView::ability_locks` (no shipped mode ability is locked; the rule exists because `admit_cast` checks
  `is_ability_unlocked` for every ability).
- New `CardLock::Transforming` when every unlocked contributor is channelling (`ModeStateView::channeling`). Its
  tooltip is "Transforming"; a key press on it reports through the feed as every locked entry does.

New card types: `CardAction::SwitchMode { mode: ModeId, direction: ModeDirection }` and `CardLabel::Mode { mode:
ModeId, direction: ModeDirection }`, both `Copy`, so `derive_card` still allocates nothing once warm.
`apply_card_action` resolves the mode name through `ctx.model.catalog` and calls `submit_mode_switch`.

### D-9. What the HUD reads (`PlayerView`)

The HUD reads only the view (`battle_hud/model.rs` invariant). `sim_types::PlayerView` gains:

```rust
/// One of the viewer's units whose archetype declares modes.
pub struct ModeStateView {
    /// The unit.
    pub entity: EntityId,
    /// Index into the archetype's `modes` of the mode it is in or switching to (`Entity::settled_mode`).
    pub settled_mode: Option<u8>,
    /// Whether it is channelling a switch now.
    pub channeling: bool,
}
/// One of the viewer's types whose ability at `ability_index` still needs research.
pub struct AbilityLockView { pub unit_type: UnitTypeId, pub ability_index: u8 }

pub mode_states: Vec<ModeStateView>,     // sorted by entity
pub ability_locks: Vec<AbilityLockView>, // sorted
```

`write_player_view` fills `mode_states` in `record_entity` for owned, live, uncarried units with a non-empty `modes`,
and `ability_locks` in `record_options` for each completed type and each ability with `!is_ability_unlocked`.
Both are cleared by `PlayerView::clear` and refilled without allocation once warm (the view's invariant). Only the
viewer's own units appear, so nothing hidden leaks.

### D-10. Tests

- **Simulation** (`crates/sim_core/tests/spells_and_abilities_tests.rs`, WP1): a mode switch without Shift empties a
  walking unit's queue, and the anchored unit then acquires and fires; a refused switch leaves the queue; a stim
  leaves the queue (transient); a queued switch behind a focus order on an anchored unit runs after the target dies;
  `would_admit_cast` agrees with `execute_ability` on every refusal reason (dead, carried, channelling, locked,
  cooldown, energy); `settled_mode` and `immobile_after_channel` before, during and after both channels. Matchup unit
  test: mode commands precede side commands.
- **View** (`crates/sim_core/tests/player_view_tests.rs`, WP2): `mode_states` through a full enter and leave,
  `ability_locks` with a test upgrade that unlocks a mode ability; enemy units absent.
- **Catalog** (`catalog/validation.rs` unit tests, WP2): each refusal of the new rule; the shipped catalog loads.
- **Client admission** (`battle_pipeline/commands/tests.rs`, WP3): direction per settled mode, mixed selections,
  grouping by ability index (a test archetype with the mode ability at index 1), channelling and locked members left
  out, `EmptySelection`, revalidation rejecting a unit that switched in between, generic `submit` still refusing
  `CastAbility`, a leaving unit admitted for `Move`. The existing `absent_session_and_unsupported_capability_are_explicit`
  keeps `CastAbility` as its unsupported order.
- **Card** (`battle_hud/card.rs` and `catalog.rs` unit tests, WP4): entries and order for each row of D-8's table,
  shared `ModeId` across two archetypes, both locks, empty card for an inspected enemy and a closed match, labels from
  data, no allocation once warm (`sync_steady_state_tests` covers the HUD refresh).
- **Input, headless** (new `crates/client/tests/battle_mode_tests.rs`, WP5). Boots the shipped skirmish
  (`Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO)`); setup spawns units and enemies through
  `world_mut`, as `battle_card_tests.rs` does; every order arrives through input events. Archetypes are chosen by
  capability (a mode ability whose mode replaces the ground weapon; "armed by default" or not), never by slug:
  1. `siege_and_tank_mode_by_key`: select by click, tap the key of the enter slot, the pending command is
     `CastAbility` with the unit's index; run until the view settles; the slot at the same position is now the leave
     entry; tap it; the unit is mobile again.
  2. `a_unit_armed_only_in_its_mode_burrows_and_kills`: the Lurker analogue next to an enemy within its mode
     weapon's range; tap Burrow; run; the enemy loses life and dies. This is Q0070's proof through input alone.
  3. `a_walking_unit_switches_and_drops_its_move` (D-6a): right-click far ground, tap the enter key; after the
     channel the unit fires at an enemy in range.
  4. `shift_queues_the_switch_behind_a_move`: right-click, then Shift and the key; the unit walks, then switches.
  5. `leave_then_right_click_walks_after_the_channel` (D-7).
  6. `a_mixed_selection_switches_each_unit_its_way`: box-select both stances; each entry goes only to its units.
  7. `a_key_while_transforming_says_why`: the `Transforming` lock reaches the feed and nothing is queued.
  8. `a_click_on_the_mode_slot_does_the_same_as_its_key`.
  The binary fails on any simulation diagnostic, as `skirmish_match_tests.rs` does.
- **GPU.** None new. Mode entries are drawn by the generic `draw_slot`; `battle_hud_window_tests` and the windowed
  skirmish child already render the card through the harness of charter §7, and the "HUD windows" path rule selects
  them for this change. No test builds a render device in its own process (C21).

### D-11. Documents

- `DESIGN.md` §4: "HUD and command card", What exists: "…, mode commands built from catalog `ToggleMode`
  abilities (Siege Mode, Burrow analogues)"; Evidence adds `crates/client/tests/battle_mode_tests.rs`. "Abilities and
  hazard zones", Known gaps: "the skirmish client issues only mode switches; other abilities have no command (B-69)".
- Bug register (plan `2026-11-05-sc2-deviation-fixes` §4 and the `DESIGN.md` table): the proposed B-IDs of §8, with
  their schedule.
- `assets/catalog/units.ron`: `leave_name` on both shipped mode abilities (WP2).
- The plan index row (this commit) and its status updates.

---

## 4. File ownership

| File | WP |
|---|---|
| `crates/sim_core/src/world/commands.rs` (`dispatch_cast`) | WP1b |
| `crates/sim_core/src/movement/locomotion.rs` (`walking_goal`) | WP1c |
| `crates/sim_core/src/world/abilities.rs` (`would_admit_cast`), `crates/sim_core/src/entity.rs` (`settled_mode`, `immobile_after_channel`) | WP1a |
| `crates/sim_fixtures/src/matchup.rs` (`BattleRun::new` order, unit test) | WP1b |
| `crates/sim_core/tests/spells_and_abilities_tests.rs` | WP1 |
| `crates/sim_types/src/commands.rs` (`AbilitySpec::leave_name`), `crates/sim_types/src/player_view.rs` (two views) | WP2 |
| `crates/sim_core/src/catalog/validation.rs`, `crates/sim_core/src/player_view.rs`, `crates/sim_core/tests/player_view_tests.rs`, every `AbilitySpec { … }` literal (19 today) | WP2 |
| `assets/catalog/units.ron` | WP2 |
| `crates/client/src/battle_pipeline/commands.rs`, `commands/tests.rs`, `battle_pipeline/status.rs` (if a sentence changes) | WP3 |
| `crates/client/src/battle_hud/{catalog.rs,card.rs,actions.rs}` | WP4 |
| `crates/client/tests/battle_mode_tests.rs`, `crates/gate/suites.ron` | WP5 |
| `DESIGN.md`, `docs/plans/2026-11-05-sc2-deviation-fixes.md` (register rows only), `docs/plans/README.md`, `docs/questions/`, `docs/evidence/2026-11-09-client-mode-commands.md` | WP1 (evidence), WP6 |

---

## 5. Work packages

Commit order is WP0, WP1a, WP1b, WP1c, WP2, WP3, WP4, WP5, WP6, each a separate commit (WP1 is three) on one branch
from `main`. Each implementation agent works in its own worktree with
`CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>` (charter §7), runs `cargo gate quick`
and the tests its change needs, and hands back (Q0061). A verifier on a cheap model runs `cargo gate package` on the
final tree after `main` is merged in, and the stamp goes in the hand-back; failures go back to the implementation
agent. Client tests are run as they are and never wrapped in `with-gpu-lock` (charter §7). Simulation packages also
get the golden replays in release and on `wasm32-wasip1` through `package` (charter §4.3).

### WP0: Preconditions

- The B-42 branch (`worktree-agent-af3d5253dd330d7b1`) is on `main`, with `burrow_scorpion`'s `burrowed` mode and
  Burrow ability. If it changed on the way, update §2.3 by amendment before WP1.
- This plan's row is in `docs/plans/README.md` (done with this plan's commit).

### WP1: Simulation (three commits, each measured)

- **WP1a, queries (bit-identical by construction).** `SimWorld::would_admit_cast`, `Entity::settled_mode`,
  `Entity::immobile_after_channel`, with `///` docs and unit tests. Nothing calls them in the tick.
- **WP1b, D-6a.** `dispatch_cast` replaces orders after an admitted non-queued `ToggleMode` cast; `BattleRun::new`
  sends mode commands first. Tests of D-10. Measure: `sim_core/golden_replay_tests` (debug, release, `wasm32-wasip1`),
  `sim_core/sc2_who_wins_matchup_matrix_tests`, `sim_core/full_match_tests`, `sim_core/burrow_attack_tests`.
  Expected: every checksum and outcome unchanged. If anything moves, stop, record which battle and why in the
  evidence file, and hand back to the orchestrator; a re-record follows the deviation-fix procedure (its D-8 style:
  name the cause, re-record from the first moved checkpoint, one cause per commit).
- **WP1c, D-6b.** `walking_goal` runs a front cast for an anchored, non-channelling unit. Same measurement, same
  expectation.
- Evidence: `docs/evidence/2026-11-09-client-mode-commands.md` §1, with the commands run and their results.

### WP2: Catalog field, validation and view

`AbilitySpec::leave_name`; the validation rule of D-2 with a typed error; `leave_name: Some("Tank Mode")` and
`Some("Unburrow")` in `units.ron`; `ModeStateView`, `AbilityLockView` and their filling (D-9). Update every
`AbilitySpec` and `PlayerView` literal. Tests of D-10. The `assets/catalog/**` rule and the `sim_types` downstream
contract select the dependants; expected bit-identical (no state change).

### WP3: Client admission

`ModeDirection`, `submit_mode_switch`, `PendingMode`, `validate_mode_switch`, the `capable` mobility change of D-7,
and the comment of `supported`. Tests of D-10.

### WP4: Client card

`ModeId`, `ModeCommand` and labels in `CommandCatalog`; `CardAction::SwitchMode`, `CardLabel::Mode`,
`CardLock::Transforming`; the root order of D-8; `apply_card_action`. Tests of D-10. The `battle_hud/**` rule runs
the HUD window GPU tests in `package`.

### WP5: Input tests and the gate catalog

`crates/client/tests/battle_mode_tests.rs` (D-10). In `crates/gate/suites.ron`: classify it
`"client/battle_mode_tests": (class: Slow, test_threads: Some(2))` next to `battle_card_tests`, add it to the
"catalog" rule's targets and to the `sim_core` downstream contract (it is the client proof of mode admission), and
run `crates/gate/tests/suite_catalog_tests.rs` (C22). Measure its peak RSS at 2 threads; if it passes about 6 GiB,
lower the cap as Amendment 4 of plan `2026-11-04-tiered-test-gates` prescribes and record the figure in the
hand-back.

### WP6: Bookkeeping

- `DESIGN.md` rows of D-11; the register rows of §8 (B-94 to B-97, or the next free IDs) in the deviation-fixes plan
  and in `DESIGN.md`, each scheduled as §8 says.
- Q0070: add under its question the commits that built option 1. If `## Answer` has text, add `## Resolution` and
  `git mv` it to `closed/` (questions README, rule 2); if not, leave it open, because only the owner closes it by
  answering.
- File the per-ability hotkey question of D-8 in `docs/questions/open/` with its default.
- This plan's row: `Done` with the commits, or `Partly done` naming what is missing.
- Evidence file §2: the WP5 RSS figure and the gate stamps.

---

## 6. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| R-1. WP1b moves a matrix battle or a golden replay (the reorder argument of D-6a is wrong somewhere) | Re-records | Measured in its own commit; stop and record before any re-record |
| R-2. Mode entries shift other root entries one position for selections that contain a mode unit | A test or the skirmish pilot that assumes a fixed position breaks | Tests and the pilot find slots by `CardAction` or `CardLabel` (`position_of`, `position_of_label`); `skirmish_match_tests` runs in `package` through the `battle_pipeline/**` and `sim_core` rules |
| R-3. Revalidation by direction rejects presses more often than players expect around channel ends | A feed line instead of a switch | Same tick world as the simulation; tested (D-10); the refusal names the reason |
| R-4. `PlayerView` growth costs allocation in steady state | `sync_steady_state_tests` or the valgrind count fails | Lists are cleared and refilled in place; both are small (mode units only, locks only) |
| R-5. The new binary's memory | Gate pressure on the shared server | `test_threads: Some(2)` and a measured figure (WP5) |
| R-6. StarCraft II claims marked **verify** (transient semantics, Shift queueing) are wrong | D-6a would be a deviation instead of a fix | Cited data supports them; WP1b records the source it confirms with, or files a question before landing |
| R-7. The B-42 branch changes its data before merging | §2.3 out of date | WP0 checks and amends |

---

## 7. Determinism summary

| WP | Simulation state | Goldens and matrix |
|---|---|---|
| WP1a | none (read-only queries) | bit-identical by construction |
| WP1b | orders after a non-queued mode switch | expected bit-identical (no golden casts; matrix casts reordered); measured |
| WP1c | queued cast on an anchored unit | expected bit-identical (no such queue in any fixture); measured |
| WP2 | none (catalog field outside the checksum; view is output) | bit-identical |
| WP3 to WP6 | none (client and documents) | bit-identical |

No checksum format change, no float in simulation state (C7), no new collection type (C9).

---

## 8. Out of scope and follow-ups (recorded)

Deferred without asking, as charter §8 allows when written down (Q0002):

1. **Other abilities.** Targeted casts (`TargetPosition`, `TargetEntity`: Blink, hazards such as Storm), with the
   cursor mode they need in `CommandMode`; `InstantSelf` abilities other than the stim order; the legacy
   `SiegeModeToggle` and `CloakToggle` effects. All wait for the abilities plan of deviation-fixes WP18 (B-69), which
   should reuse D-2 to D-5: a capability-driven card entry, `would_admit_cast`, one command per ability index.
2. **Overclock as an ability.** Stim stays a capability with `UseStim` (deviation-fixes WP3 owns its catalog form).
3. **Proposed B-94: a queued mode switch toggles whatever stance the unit has when it runs** (D-5). StarCraft II's
   per-direction morphs (`liberty` AbilData:1301, :1328) would refuse the second. Fix: a directional effect
   (enter or leave) in the simulation; deviation-fixes WP18.
4. **Proposed B-95: an order queued behind a switch into an anchoring mode stays at the front of the anchored unit's
   queue**, and a `Move` there makes it `Busy`. What StarCraft II does with an order its morphed unit cannot carry
   out is **(unsourced)**. Deviation-fixes WP18 (research).
5. **Proposed B-96: the Lurker analogue's burrow cannot be cancelled while it runs** (`Interruptible` and a Cancel
   button, `swarm` AbilData:1658-1659). Deviation-fixes WP18.
6. **Proposed B-97: anchored units get no Stop or Hold** (the client's mobility policy, `capable`); StarCraft II's
   sieged tank card has both (`liberty` UnitData:5185-5186). A client package, small; scheduled with the next client
   work.
7. **Per-ability hotkeys** (D-8): the owner's question filed by WP6.
8. **Subgroups and Tab** (phase-5 D-7): a mixed Lurker selection shows both directions.
9. **Autocast or automatic burrowing** (Q0070 option 3): not a player mechanic in StarCraft II.
10. **The lab inspector** (`lab_ui.rs`, which offers Overclock) gets no mode buttons.
11. **A transformation cue** on the bubble while a unit channels: presentation, not needed for play.
12. **Q0069** (when the burrowed weapon and cloak arrive) and **B-93** (burrowed units still block): unaffected; the
    card follows whatever stance the simulation reports.
