# Phase 5 Blueprint: The Playable Client

Finishes Phase 5 of `docs/plans/2026-11-01-codebase-remediation.md`.

**Exit criterion:** a person can play a match to a result using only mouse and keyboard, and 1.9 covers it.

**Delivery status (2026-11-03):** WP0 to WP9 and the addendum (§8) are delivered; the exit criterion is met locally
and awaits CI on `main`. A person plays the shipped skirmish with `cargo run -p client -- --skirmish`: mine, raise
structures from the card with a placement preview, train, group, attack-move, win or lose, and play again, under fog,
with mouse and keyboard only. `skirmish_match_tests.rs` (`7a432f9`) proves it. **Deferred:** gas, research and
transport are built and proven on temporary fixture roots (D-9), but disabled in the shipped game ("No approved media")
until media exists, per `docs/questions/open/0001-media-scope.md`. Out of scope as listed in §6.

**Status of this document:** Stage 1 blueprint under charter §6. It was written by static reading only: nothing
was compiled or run. Every `file:line` reference was read on branch `codebase-remediation` at `b263734`. Where a
claim is a prediction about runtime behaviour, it is marked **verify**, and the implementer must confirm it with
the first test of that package.

**Rules for every package**

- Failing test first, through real input events or `PlayerCommand`.
- The gate passes before each commit: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --all -- --check`.
- Each commit names its step (5.x) and finding, if any, and may be merged alone.
- No unit or structure identifier in Rust. Tests may read slugs from data, as `full_match_tests.rs` does, but
  `crates/client/src` may not name one.
- Every new file starts with a `//!` block stating responsibility, fit and invariants.

---

## 1. Summary

The simulation half of Phase 5 is done. The client can select, move, attack and queue. What is missing falls into
five groups:

1. The command pipeline refuses every new order and never reads their outcomes.
2. The player has no game to boot into: no skirmish mode, and the only UI is a developer inspector that shows
   enemy rosters and lets the player switch sides.
3. There is no HUD, command card, placement, result screen or attack-move input.
4. **There is no fog of war in the client at all.** Every enemy is drawn and pickable wherever it stands.
5. **Most of the catalog cannot be shown.** A foundation of a structure without media is a fatal error, and that
   includes the gas extractor. See §2.6 and decision O1 in §7.

The plan has ten packages. WP0 to WP3 run in parallel, WP4 and WP5 run in parallel after them, WP6 to WP8 run in
parallel after those, and WP9 closes the phase.

---

## 2. Current state

### 2.1 Command pipeline

| # | Finding | Evidence |
|---|---|---|
| G-1 | `supported()` ends in a wildcard that refuses `Harvest`, `ReturnCargo`, `Build`, `Construct`, `CancelConstruction`, `Research`, `CancelProduction`, `Load`, `UnloadAll` and `CastAbility` | `battle_pipeline/commands.rs:351-363` |
| G-2 | `capable()` also ends in a wildcard returning false | `commands.rs:365-388` |
| G-3 | `can_receive_orders` requires `is_completed`, so `CancelConstruction`, whose recipients are unfinished by definition, can never pass | `commands.rs:391-393` |
| G-4 | `capable` has no catalog access, but `Build` and `Construct` need the structure's faction. `attack_target` shows the existing pattern for a resolved subject | `commands.rs:396-437` |
| G-5 | A test pins `ReturnCargo` as unsupported | `commands/tests.rs:128-142` |
| G-6 | `capture_outcomes` drains production and movement outcomes only. `game_command_outcomes()` and `task_events()` never reach the session, so a rejected `Build`, a `MatchOver` refusal and a builder giving up are all invisible | `battle_pipeline/driver.rs:195-227`, `driver.rs:18-24`, `sim_core/economy/mod.rs:302-315` |
| G-7 | Five distinct research refusals collapse into one sentence | `battle_pipeline/status.rs:98-102` |
| G-8 | `UseStim` before research silently does nothing: no outcome exists on that path, and `capable` checks only the archetype | `sim_core/world.rs:1232-1254`, `commands.rs:380-382` |
| G-9 | `submit` does not know the match ended. Commands are queued, then refused by the simulation with no notice (G-6) | `commands.rs:179-200`, `world.rs:1149-1164` |

Worth keeping: `submit_selection` already drops incapable members and fails only when none remain
(`commands.rs:247-283`), `prepare_tick` revalidates and reports rejection (`commands.rs:291-348`), and
`submit_roster_order` exists for roster buttons (`commands.rs:206-237`).

### 2.2 Input

| # | Finding | Evidence |
|---|---|---|
| G-10 | Right click resolves to attack or move and nothing else | `battle_input/mod.rs:337-378` |
| G-11 | No attack-move input exists for a selection. `A` pans, so the `StarCraft` key is taken. The only attack-move is the inspector's roster button | `input_bindings.rs:25`, `lab_ui.rs:695-708` |
| G-12 | `collect_key_intent` reads ordered events, so a chord survives a low frame rate. New keys must use this path, not `ButtonInput` | `battle_input/mod.rs:380-422` |
| G-13 | System order is `lab_ui_system` → `sample_input_ownership` → `collect_pointer_intent` → tick, inside `BattleSet::CollectIntent` | `battle_input/mod.rs:79-90`, `lib.rs:327-338`, `lib.rs:365-373` |
| G-14 | `ownership.pointer` is `inside world rect && !wants_pointer_input()`. egui's `wants_pointer_input` is false on the press frame when no widget has claimed the pointer (`is_pointer_over_area() && !any_down`), so a floating window is **not** protected on the frame that matters. Panels are protected by the world rect | `input_policy.rs:152-160`, `lab_ui.rs:417-418` |

### 2.3 Boot and UI

| # | Finding | Evidence |
|---|---|---|
| G-15 | `--scenario` exists, but its default is `scenarios/default_battle.ron` and there is no way to ask for the skirmish by name | `main.rs:44-46`, `runtime_battle/mod.rs:34` |
| G-16 | `ClientMode` has no player-facing mode. `LiveBattle` is the developer sandbox | `lib.rs:155-165`, `lib.rs:281-285` |
| G-17 | The inspector shows every player's roster and offers a `Control` combo that switches sides | `lab_ui.rs:604-612`, `lab_ui.rs:648-660` |
| G-18 | Failures (`battle_failure`, media) are shown only in the inspector. A player mode that hides the inspector must show them elsewhere | `lab_ui.rs:356-369` |
| G-19 | `RenderSnapshot.players` is present but nothing reads economy, `match_result` or `resource_nodes` in the client | `sim_types/snapshot.rs:199-217`, `grep` of `crates/client/src` |

### 2.4 Fog

| # | Finding | Evidence |
|---|---|---|
| G-20 | `sync_live_battle_system` spawns a visible bubble for every snapshot record, with no vision check | `scene_controller.rs:585-672` |
| G-21 | `is_entity_available_for_combat` checks cloak and damageability, not fog. `can_pick` relies on it, so today a fogged enemy can be clicked | `target_acquisition.rs:91-104`, `battle_input/mod.rs:154-163` |
| G-22 | Tracers and shatters are delivered regardless of vision | `driver.rs:305-316` |
| G-23 | Resource billboards vanish the moment a node runs out, wherever it is | `session.rs:91-93`, `runtime_battle/scene.rs:383-389` |
| G-24 | `Build` admission calls the full `check_build`, so ordering a build on a hidden enemy structure is refused at once, with its `EntityId` in the reason. That is a remote structure detector | `construction.rs:181-188`, `construction.rs:228`, `sim_map/grid.rs:581-590` |
| G-25 | `Harvest` admission refuses an exhausted node at any range, leaking that a distant node has been mined out | `economy/harvest.rs:81-83` |
| G-26 | The fix for G-24 already exists: `known_blocked_cells[player]` records structure cells the player has seen, and clears when seen empty | `world.rs:167`, `world.rs:1664-1695` |

The snapshot's `players` and `resource_nodes` are unfiltered for every slot (`world.rs:1929-1953`). That is fine as
long as a viewer-scoped layer sits between it and the player. WP1 adds one.

### 2.5 Simulation API gaps for the UI

`RenderSnapshot` is global and has no viewer. These are the real gaps:

| Need | Today | Gap |
|---|---|---|
| Per-structure queue | `Entity.production_queue`, public field | Not in any snapshot |
| Buildable list | Rule lives inside `admit_build` (`can_construct && same faction`, `construction.rs:229-238`) | No shared predicate. The catalog has no build list and needs none |
| Tech availability | `missing_prerequisite` is `pub(crate)` (`production/mod.rs:518`), `tier_locked` is private (`research.rs:23`) | No public query, and a client copy would drift |
| Placement validity | `check_build` is private and sees through fog | Needs a viewer-safe query (G-24) |
| Passengers and slots | `carrier_slots_used` is private (`transport/orders.rs:49`) | Loaded units are also absent from the snapshot (`world.rs:1956`) |
| Cargo | `Entity.cargo`, public field | Not in any snapshot |
| Stim availability | `is_ability_unlocked` is public (`world.rs:735`) | Must be surfaced for G-8 |
| Task owner | `TaskEvent` has no owner | Client cannot filter events of dead units (`economy/mod.rs:203-210`) |

### 2.6 Media: the blocking gap

`advance_battle_ticks` calls `media.require` for every snapshot bubble and its `anim_state`
(`driver.rs:263-268`). Failure is fatal for the battle (`driver.rs:285-287`), and that is correct under charter §1.2.

Shipped flipbooks (`assets/textures/*.flipbook.ron`), audited with a script:

- **Automaton structures with media, 3 of 17:** `foundry_core_dome`, `assembly_bay_dome`, `capacitor_dome`.
- **Automaton trainables with media, 3 of 13:** `welder_drone`, `pulse_bot`, `railgun_crawler`. The last needs
  `machining_plant_dome`, which has none.
- **Missing and needed:** `plasma_siphon_dome` (the only extractor), `tech_module_pod` (prerequisite of
  `overclock_stim`), `mechanics_lab_dome`, `machining_plant_dome`, `aerospace_hangar_dome`, `repair_shuttle` (the
  only Automaton carrier).

**Consequences.** Today, a player who starts an extractor stops the match with a fatal error. Gas, research and
transport cannot be reached in the shipped skirmish, because every path to them passes through an unpresentable
identity. The mineral path, worker → Assembly Bay → `pulse_bot` → destroy the base, is fully presentable, and it
is the path `full_match_tests.rs` wins with.

This plan therefore builds all of the UI, gates each entry on real media, and proves gas, research and transport
through the real loader using temporary fixtures (decision D-9). See O1 in §7.

### 2.7 Dome fill

`bubble_dome.wgsl:172-179` compares `in.uv.y` with `build_progress`. The finding C-11 understates the defect.

- `Sphere::mesh().uv(80, 52)` has its poles on **±Z**, not ±Y: `vertices.push([x, y, z])` with `z = sin(stack_angle)`
  (`bevy_mesh` 0.15 `primitives/dim3/sphere.rs:182-193`).
- The dome is that same full sphere (`bubble_shader.rs:424-427`), placed with no rotation
  (`scene_controller.rs:601-607`).

So the "vertical" fill actually sweeps horizontally across the map's Y axis. No choice of `uv.y` direction can
fix it.

`build_or_queue_progress` reaches the shader only at `bubble_shader.rs:283`. The fill runs only when
`anim_state == 0`, which is `Constructing` (`AnimState` declaration order, `enums.rs:70-83`, cast at
`bubble_shader.rs:274`), so a finished building with a queue draws correctly today. No progress ring exists
anywhere in `indicators.rs`. The handover's worry is about a future ring, and D-11 removes it.

### 2.8 Test assets to reuse

- `Harness` drives the real plugin with real events (`tests/battle_input_tests.rs:57-452`), but its `look_at` writes
  `RtsCamera` directly (`:218-226`) and it is private to that file.
- `tests/support/gpu_run.rs` launches a binary under `xvfb-run` on lavapipe, with a 300 s limit (`:37`).
  `assert_passes_validation` is the entry point (`:335`).
- `runtime_media_tests.rs:30-40` builds a temporary assets root through the real loader. That is the precedent
  for D-9.
- `asset_baker::procedural_generator::generate_procedural_atlas` (`procedural_generator.rs:290`) can produce
  fixture atlases.
- `full_match_tests.rs` gives working build sites and army sizes for the skirmish map.

---

## 3. Design decisions

**D-1: `ClientMode::Skirmish` and a `BattleProfile`, leaving `--battle` alone.** Add `--skirmish`. It selects
`assets/scenarios/skirmish.ron` and the `Player` profile. The `Sandbox` profile keeps today's inspector, so the
5.1-5.3 tests, GPU captures and pixel-row assertions do not move. The HUD and fog run only in `Player`.

*Why not make the skirmish the `--battle` default:* `default_battle.ron` is used by a dozen test files and by
`run_headless_check` (`lib.rs:671-675`).

**D-2: A viewer-scoped `PlayerView`, written by `sim_core`, typed in `sim_types`.** `SimWorld::write_player_view(&self,
viewer, &mut PlayerView)` fills reusable buffers once per executed tick. The HUD reads this record and never the
world, and everything hidden from `viewer` is absent by construction. It contains integers and enums only, so no
fixed-to-float conversion is added.

*Why not extend `RenderSnapshot`:* it is global, it is cloned into three buffers per frame by
`copy_match_status_from`, and it would put private data for all eight players into the presentation path.

*Why not let the HUD read `SimWorld` as `battle_presentation.rs` does:* tech rules would be duplicated in the
client, fog policy would live where no headless test can reach it, and it moves against step 6.6.

`battle_input` and `commands.rs` keep reading the world for admission. That is existing, deliberate behaviour. The
one HUD exception is the placement ghost, which calls the viewer-scoped `preview_site` because its input is a cursor
cell, not a tick.

**D-3: Admission uses what the player knows; arrival uses the truth.** `Build` admission and `preview_site` count a
structure only if it is the player's own or in `known_blocked_cells`. The arrival check stays complete, and a
hidden structure ends the order with `BuildAbandoned` once the builder is there to see it. `Harvest` refuses an
exhausted node only if the node's cell is currently `Visible`. Otherwise the worker walks there and
`step_harvest` reports it (`harvest.rs:155`).

Public knowledge: terrain, and the position and kind of every node, because both are drawn from the map at load
(`scene.rs:230-254`). Private: structures, units, remaining amounts.

**D-4: Panels reserve space, so the world rect is the hit test.** HUD bars are `TopBottomPanel`s drawn **before**
`lab_ui_system` computes `geometry.world` (`lab_ui.rs:418`). `apply_world_viewport` then shrinks the camera to the
same rect, and `ownership.pointer` is false over any panel, with no new mechanism. The one floating surface that
takes clicks, the result window, is covered by a new `!context.is_pointer_over_area()` term in
`sample_input_ownership`, closing G-14. The message feed is a non-interactable `Area`, so clicks pass through it
(**verify** with a test).

**D-5: One state machine for targeted orders.** `CommandMode` in `battle_input/command_mode.rs` is `Normal`,
`AttackMove`, or `Place(structure)`. While not `Normal`, left click in the world issues the order and right click
or `Escape` cancels. Shift keeps the mode, so several sites can be queued.

**D-6: Command cards are positional and derived.** Fifteen slots, with fixed keys by position, so no key belongs
to any unit:

```
Q E R T Y
F G J K L
Z C V B N
```

`W A S D` pan, and `H` and `X` keep Hold and Stop. Entries fill slots in order. Stop and Hold appear on the card
showing their own keys and take no positional key. `Escape` is `CANCEL_KEY`: it leaves a mode or goes back a page.

**D-7: The card is the union of what the selection can do.** There are no subgroups. An entry appears if any
selected unit can do it. A train or research entry goes to the selected capable structure with the shortest
queue, lowest ID on a tie, because the simulation otherwise always picks the first (`production/mod.rs:479-515`).
That choice is UI policy, so it lives in the client.

The Build page lists every structure with `UnitArchetype::can_raise`. There are 17 for Automaton, so the page
pages: 13 entries, `More`, `Back`.

**D-8: Disabled, with the reason, rather than absent.** An entry that cannot be used now is shown disabled with
a tooltip: missing structure, tier locked, researched, in progress, not enough resources, or no media. Resource
and supply shortage leave the entry *enabled*, and the simulation's typed rejection reaches the feed. A player
who presses too early should learn why, and income can change between frames. Hard locks are disabled.

**D-9: Media gate plus test fixtures.** `ClientMediaStatus::presentable(registry, unit_type)` checks every action
the simulation can publish. `CommandCatalog` evaluates it once per session. An unpresentable product is disabled
with the text "No approved media". No code changes are needed when real media arrives.

Tests of gas, research and transport use a **temporary** assets root containing the real files plus procedural
atlases for the missing identities. That passes the real loader and never touches a runtime path, as remediation
decision D6 recommends and charter §1.2 requires. It is the only permitted use of procedural atlases here.

**D-10: Fog in the `Player` profile hides, and does not shroud.** Enemy bubbles and events outside `Visible` cells
are hidden. Nodes keep their last seen state. Drawing darkness on terrain is art direction and is out of scope.
Consequence: an enemy structure disappears when vision leaves. Remembered structures are out of scope.

**D-11: Production progress is HUD-only.** `build_or_queue_progress` keeps its meaning. The world view shows only
construction, through the dome fill. Production progress appears only in the queue panel, for a selected structure
of the viewer. This also prevents an enemy's queue from leaking. No ring is added.

**D-12: Match end stops orders, not sight.** Once `match_result.is_over()` the card is hidden, `CommandMode` resets,
right click and order keys do nothing, and `submit` refuses with `SubmissionError::MatchOver`. Left click still
selects, so the player can look around. The world keeps ticking, per `match_state.rs:14-18`.

**D-13: The exit test reads the screen, not the world.** The pilot sees only `PlayerView`, `BattleSelection` and
bubbles that are Bevy-visible, and it acts only through input events. It cannot cheat fog, and it cannot pass by
writing state.

### Module map

```
crates/sim_types/src/player_view.rs      PlayerView and its records (new)
crates/sim_core/src/player_view.rs       write_player_view, preview_site (new)
crates/client/src/battle_input/
    command_mode.rs                      CommandMode, placement_cell (new)
    context_order.rs                     pure right-click resolution (new)
crates/client/src/battle_hud/            all new
    mod.rs        BattleHudPlugin, HudGeometry
    catalog.rs    CommandCatalog: per-session labels and capabilities
    model.rs      HudModel: PlayerView plus selection facts, refreshed on change
    card.rs       CommandCard, CardAction, pure card derivation
    actions.rs    executes a CardAction, card hotkeys
    panels.rs     egui drawing only
    feed.rs       BattleFeed: bounded message log
    placement.rs  live preview and ghost
    fog.rs        BattleFog: enemy visibility, event filter, node memory
    result.rs     result window and failure banner
```

`battle_hud` depends on `battle_input` and `battle_pipeline`, never the reverse. `battle_input` knows nothing of the
card: hotkeys are read by a `battle_hud` system of their own (WP6). `BattleProfile` lives in `battle_presentation.rs`,
because `lab_ui.rs` and `battle_hud` both read it.

---

## 4. Work packages

| WP | Title | Steps | Depends on | Parallel group |
|---|---|---|---|---|
| WP0 | Boot the skirmish | item 2 | none | A |
| WP1 | `PlayerView` and knowledge-based admission | sim gaps | none | A |
| WP2 | Command pipeline | item 1 | none | A |
| WP3 | Dome fill | 5.8 | none | A |
| WP4 | Context orders and command modes | 5.4 to 5.7 input | WP0, WP1, WP2 | B |
| WP5 | HUD shell and read-only panels | item 3 | WP0, WP1, WP2 | B |
| WP6 | Command card, queue, placement | item 3 | WP4, WP5 | C |
| WP7 | Fog presentation | fog | WP1, WP5 | C |
| WP8 | Result screen | item 3 | WP4, WP5 | C |
| WP9 | Exit proof | exit | all | D |

**Parallelism.** Groups run in order. Packages inside a group can use separate git worktrees.

- **Group A** has disjoint files. WP0 owns `lib.rs`, `main.rs` and the battle input test file. WP1 owns `sim_types`,
  `sim_map/grid.rs`, `production/construction.rs` and `economy/harvest.rs`. WP2 owns `battle_pipeline/*` and
  `economy/mod.rs`. WP3 owns the shader.
- **Group B:** WP4 owns `battle_input/*` and `input_bindings.rs`. WP5 owns `battle_hud/*`, `lab_ui.rs`, `input_policy.rs`
  and `lib.rs`.
- **Group C:** new files are disjoint, but each adds a registration line to `battle_hud/mod.rs`. Expect a trivial
  merge there. WP7 alone touches `driver.rs`, `scene.rs` and `sim_types/snapshot.rs`.
- **WP9 is sequential** and last. Do not start it until C is merged.

---

### WP0: Boot the skirmish

**Goal.** `client --skirmish` boots the real skirmish, and a permanent test proves it. Share the harness.

**Files**

- `crates/client/src/runtime_battle/mod.rs`
- `crates/client/src/lib.rs`, `crates/client/src/battle_presentation.rs`
- `crates/client/src/main.rs`
- `crates/client/tests/support/battle_harness.rs` (new)
- `crates/client/tests/battle_input_tests.rs`
- `crates/client/tests/runtime_battle_tests.rs`

**Types and signatures**

```rust
// runtime_battle/mod.rs
/// The playable skirmish, resolved beneath the validated assets root.
pub const SKIRMISH_SCENARIO: &str = "scenarios/skirmish.ron";

// lib.rs
pub enum ClientMode { VisualLab, LiveBattle, Skirmish, SwarmStressTest, HeadlessCheck }

// battle_presentation.rs
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BattleProfile {
    /// Developer inspector with every player's data.
    #[default]
    Sandbox,
    /// A person playing one side: HUD, fog and no inspector.
    Player,
}
```

`ClientMode::Skirmish` maps to `LabTab::LiveBattle` at `lib.rs:281-285` and inserts `BattleProfile::Player`. All other
modes insert `Sandbox`.

In `main.rs`, change `scenario` to `Option<PathBuf>` and add `--skirmish`. Resolve:

```rust
let scenario = args.scenario.unwrap_or_else(|| {
    PathBuf::from(if args.skirmish { SKIRMISH_SCENARIO } else { DEFAULT_BATTLE_SCENARIO })
});
```

`--skirmish` takes precedence over `--battle`. Update the `--battle` help text so it says *sandbox*.

Move `Harness`, `assets()`, `sim_position` and their helpers into `tests/support/battle_harness.rs`, included with
`#[path]` and `#[allow(dead_code, reason = ...)]` as `gpu_run.rs` is. Add a scenario parameter:
`Harness::boot_scenario(mode, scenario: &str)`.

**Edge cases.** `BattleDefinition::load` validates loadouts and instantiates (`runtime_battle/mod.rs:224`).
`initial_orders: []` must not trip `NoCombatants`. Only the `AttackMoveToStart` branch can (`:274`).

**Tests first** (`runtime_battle_tests.rs`)

- `skirmish_scenario_loads_through_the_client_loader`: real assets, two participants, each owning one town hall, and
  no armed mobile unit at start.
- `skirmish_starts_both_players_with_the_loadout_bank`: reads `start_loadouts.ron`; asserts the snapshot's `players`.
- `skirmish_mode_opens_the_live_battle_under_the_player_profile`: boots the App; asserts tab, profile and a committed world.

The move of `Harness` changes no assertion, and existing tests must pass unmodified.

---

### WP1: `PlayerView` and knowledge-based admission

**Goal.** Close §2.5 and G-24, G-25, with no client change.

**Checksums and goldens: no change.** `write_player_view` and `preview_site` take `&self`. `PlayerView` is not
checksummed and not part of `RenderSnapshot`. `BubbleRenderInstance`, which is `deny_unknown_fields`, is untouched.
Admission now accepts orders it used to refuse only when the site or node is out of the player's knowledge.
`golden_replay.ron` contains neither `Build` nor `Harvest`, so no golden moves. If any golden does change, stop and
report it: that would disprove this section.

**Files**

- `crates/sim_types/src/player_view.rs` (new), `crates/sim_types/src/lib.rs`
- `crates/sim_core/src/player_view.rs` (new), `crates/sim_core/src/lib.rs`
- `crates/sim_core/src/catalog.rs`
- `crates/sim_core/src/production/construction.rs`, `production/mod.rs`, `production/research.rs`
- `crates/sim_core/src/economy/harvest.rs`
- `crates/sim_core/src/transport/orders.rs`
- `crates/sim_map/src/grid.rs`

**Types** (`sim_types/src/player_view.rs`, `Serialize, Deserialize`, documented units)

```rust
pub struct PlayerView {
    pub tick: Tick,
    pub viewer: PlayerId,
    pub status: PlayerStatusView,
    pub map_width: u32,
    pub map_height: u32,
    /// Row-major: `y * map_width + x`.
    pub visibility: Vec<VisibilityState>,
    /// The viewer's structures with a non-empty queue, by entity ID.
    pub queues: Vec<StructureQueueView>,
    /// Jobs of all `queues`, flat. Each queue addresses a range of this.
    pub jobs: Vec<QueuedJobView>,
    /// The viewer's carriers, by entity ID.
    pub carriers: Vec<CarrierView>,
    /// The viewer's units holding cargo, sorted.
    pub laden: Vec<EntityId>,
    /// Everything the viewer's entities could make or raise, sorted by subject.
    pub options: Vec<CommandOptionView>,
    /// Types the viewer owns whose overclock is researched, sorted.
    pub overclock_ready: Vec<UnitTypeId>,
    /// Nodes in `Visible` cells, by node ID.
    pub visible_nodes: Vec<VisibleNodeView>,
}

pub struct StructureQueueView { pub entity: EntityId, pub first_job: u32, pub job_count: u32, pub exit_blocked: bool }
pub struct QueuedJobView { pub item: QueuedItem, pub remaining_ticks: u32, pub total_ticks: u32 }
pub enum QueuedItem { Unit(UnitTypeId), Research(u32) }
pub struct CarrierView { pub entity: EntityId, pub slots_used: u32, pub capacity: u32 }
pub struct VisibleNodeView { pub id: ResourceNodeId, pub remaining: u32 }

#[derive(PartialOrd, Ord, ...)]
pub enum CommandSubject { Train(UnitTypeId), Raise(UnitTypeId), Research(u32) }
pub enum OptionLock { MissingStructure(UnitTypeId), TierLocked, Researched, Researching }
pub struct CommandOptionView {
    pub subject: CommandSubject,
    pub lock: Option<OptionLock>,
    pub minerals: u32,
    pub gas: u32,
    /// Supply the job reserves; zero for structures and research.
    pub supply: u32,
}

/// Why a site cannot be ordered, by what the viewer knows. Carries no entity identity.
pub enum SiteRefusal {
    OutOfBounds, UnbuildableTerrain, CliffLevelMismatch, OverlapsRamp,
    KnownStructure, CoversResource, ExtractorNeedsGas,
}
```

The terrain variants are flattened because `sim_types` cannot name `sim_map::PlacementFailureReason` (charter §5).

```rust
// PERFORMANCE RATIONALE to include in the code:
// Why: The HUD reads queues every frame and they are rewritten every tick.
// What: One flat job slab addressed by range, refilled in place.
// Trade-off: Readers slice by range rather than holding a nested Vec.
```

**Signatures**

```rust
// sim_core/src/player_view.rs
impl SimWorld {
    /// Fills `out` with what `viewer` may know. Reads no hidden state and allocates only on growth.
    pub fn write_player_view(&self, viewer: PlayerId, out: &mut PlayerView);
    /// Whether `viewer` may order `structure` at this minimum corner, by what `viewer` knows.
    pub fn preview_site(&self, viewer: PlayerId, structure: UnitTypeId, cell_x: u32, cell_y: u32)
        -> Result<(), SiteRefusal>;
}

// sim_core/src/catalog.rs
impl UnitArchetype {
    /// Whether this unit can raise `structure`. The one rule admission and the card share.
    pub fn can_raise(&self, structure: &UnitArchetype) -> bool {
        self.capabilities.can_construct
            && self.faction == structure.faction
            && matches!(structure.shape, ShapeKind::BuildingDome { .. })
    }
}

// sim_map/src/grid.rs
impl GameMap {
    /// As `can_place_building`, with `occupied` deciding whether a structure on a cell counts.
    pub fn can_place_building_where(&self, cell_x: u32, cell_y: u32, w: u8, h: u8,
        occupied: impl Fn(u32, u32, EntityId) -> bool) -> Result<()>;
}
```

`can_place_building` becomes a call to `can_place_building_where` with `|_, _, _| true`, so `place_structure`
(`grid.rs:621`) keeps the full check.

**Changes to admission**

- `construction.rs`: split `check_build` into `check_build(player, type, x, y, Knowledge)`, with
  `enum Knowledge { Full, Player }`. `Knowledge::Player` passes `occupied` as "the occupant is `player`'s, or
  `known_blocked_cells[player][idx]` is set". Keep the money and prerequisite checks in both. `admit_build` uses
  `Player`, and `step_build` (`:466`) uses `Full`. `admit_build` calls `can_raise` instead of its inline closure.
- `harvest.rs:81-83`: refuse with `ResourceExhausted` only when the node's cell is `Visible` to `player`.
- `transport/orders.rs`: `carrier_slots_used` becomes `pub(crate)`.
- `production/mod.rs` and `research.rs`: add `pub(crate) fn upgrade_lock(&self, player, def) -> Option<OptionLock>`
  built on the existing `tier_locked`. `prepare_research` keeps its own typed errors. The agreement test keeps them
  in step.

**`write_player_view` data flow**

1. Clear all buffers. Copy `status` the same way as `write_render_snapshot`, `world.rs:1929-1938`.
2. `visibility.clone_from(vision.states())`. `clone_from` reuses capacity.
3. Walk `entities` once, by ID, keeping only `owner == viewer`, alive, not loaded. Collect queues and jobs,
   `laden`, carriers, the set of completed structure types, and owned archetypes.
4. For each owned archetype, add `Train` for `trains_units` and `Research` for `researches_upgrades` (completed
   structures only), and `Raise` for every `can_raise` structure (a scratch sorted `Vec` of structure archetypes,
   kept in the workspace).
5. Sort and dedupe `options`. Compute `lock` from the completed-type set.
6. `visible_nodes`: map resources whose cell is `Visible`.

Use `self.production_blocks()` for `exit_blocked`. A viewer outside `0..8` yields an empty view and no panic.

**Edge cases**

- Tick 0: no vision yet, and own structures must still count (`Knowledge::Player` tests ownership first).
- A foundation is a structure for placement but not a prerequisite: `completed` types only, as `missing_prerequisite`
  requires.
- Dead entities awaiting reap are excluded.
- Researched and queued: `Researching` is per player, across all structures, matching `research.rs:97-105`.

**Tests first** (`crates/sim_core/tests/player_view_tests.rs`, on the real skirmish world)

- `the_view_changes_nothing`: writing it for every slot leaves `compute_checksum` and `next_entity_id` identical.
- `a_warmed_view_keeps_its_buffers`: `capacity()` and pointers stable across 500 ticks.
- `no_enemy_data_reaches_the_view`: enemy queue, cargo and carriers are absent.
- `the_view_and_admission_agree`: at several stages of a scripted game, for every `options` entry, issue the real
  command: `lock == None` plus enough money is accepted, and any `lock` is refused. Use a fresh world per stage, built
  by replaying the same commands.
- `a_hidden_node_reports_nothing_until_seen`.
- `preview_ignores_a_structure_the_player_has_never_seen`, `preview_refuses_a_structure_seen_once_even_in_fog`,
  `preview_refuses_the_players_own_structure_on_tick_zero`.
- `a_build_on_a_hidden_structure_is_accepted_and_abandoned_on_arrival`: asserts `BuildAbandoned(Blocked(..))`, and
  that no minerals are spent.
- `an_exhausted_hidden_node_takes_the_order_and_ends_it_on_arrival`.
- `an_exhausted_visible_node_is_refused`. `harvest_income_tests.rs:225-230` must keep passing unmodified. **Verify:**
  its worker sees the node after the first order's tick.
- `can_raise_matches_admission_for_every_catalog_pair`.

`sim_map` unit tests: `can_place_building_where` with `|_, _, _| false` passes over a structure and still refuses a
ramp, cliff mismatch and unbuildable cell. `sim_types`: `PlayerView` RON round trip.

---

### WP2: Command pipeline

**Goal.** Every order the card needs passes client admission, and every result reaches the session.

**Files**

- `crates/client/src/battle_pipeline/commands.rs`, `commands/tests.rs`
- `crates/client/src/battle_pipeline/driver.rs`, `driver/tests.rs`
- `crates/client/src/battle_pipeline/session.rs`
- `crates/client/src/battle_pipeline/status.rs`, `status/tests.rs`
- `crates/sim_core/src/economy/mod.rs`

**Orders.** Replace both wildcards with exhaustive matches, so the next order added fails to compile rather than
being silently refused. `CastAbility` stays the one explicit `Err(UnsupportedOrder)`, with a comment that abilities
have no UI in Phase 5.

Replace `attack_target` with:

```rust
/// The archetype an order is aimed at: an attack's target, or the structure of `Build` or `Construct`.
fn order_subject<'w>(world: &'w SimWorld, player: PlayerId, order: &UnitOrder)
    -> Result<Option<&'w UnitArchetype>, SubmissionError>;
```

`Build` resolves `structure_type`. A failed lookup is `InvalidStructure(UnitTypeId)`, an ordinary rejection.
`Construct` resolves `foundation`, and it must be this player's and alive, otherwise `InvalidTarget`.

Change `can_execute(entity, archetype, order, subject)`. The `reaches_target` check applies to `AttackTarget`
only. Arms of `capable`, with `subject` added:

| Order | Capable when |
|---|---|
| `Harvest` | `capabilities.harvester.is_some()` |
| `ReturnCargo` | `entity.cargo.is_some()` |
| `Build`, `Construct` | `archetype.can_raise(subject)` |
| `CancelConstruction` | `entity.construction.is_some()` |
| `Research`, `Produce` | `true`; admission belongs to `sim_core`, as the existing comment says |
| `CancelProduction` | `!entity.production_queue.is_empty()` |
| `Load` | mobile, on the ground layer, via `MovementLayer::from_mover` |
| `UnloadAll` | `capabilities.transport_capacity > 0` |

`Load` to the carrier itself: exclude `entity.id == carrier` here too, as `transport/orders.rs` does.

Change `can_receive_orders` to take the order:

```rust
fn can_receive_order(entity: &Entity, order: &UnitOrder) -> bool {
    entity.is_alive()
        && !entity.is_loaded_in_transport()
        && (entity.is_completed || matches!(order, UnitOrder::CancelConstruction))
}
```

Add `SubmissionError::{MatchOver, PlayerDefeated(PlayerId), InvalidStructure(UnitTypeId)}`. All are non-fatal. Check
the first two at the top of `validate` from `world.match_state`, so `submit` refuses and `prepare_tick` rejects
anything still pending at the end.

Extract `pub fn is_armed(archetype: &UnitArchetype) -> bool` for the card to share. `runtime_battle/mod.rs:263-265` can
use it too.

**Outcomes.** In `sim_core/economy/mod.rs`, add `pub owner: PlayerId` to `TaskEvent`, set in `end_task`, and add
`pub fn drain_task_events`. Derived output: no checksum effect.

In `driver.rs`:

```rust
pub enum AdmissionOutcome {
    Production(ProductionOutcome),
    Movement(MovementOutcome),
    /// Economy, construction, cancellation, transport and refused commands.
    Game(GameCommandOutcome),
}
```

`identity()` gains the arm, `capture_outcomes` drains `drain_game_command_outcomes()` with the others, and
`session.task_events: Vec<TaskEvent>` follows the `notices` pattern: cleared at the top of `advance`, extended per
tick by `world.drain_task_events()`.

**Status.** In `status.rs` add `write_game_reason(output, &GameCommandError)`, `write_build_rejection` and
`write_task_event(output, &TaskEventKind)`, all exhaustive with no wildcard and no entity identifiers. Give each
research refusal in `write_production_reason` its own sentence, closing G-7.

`Invariant` errors keep the wording "the battle cannot process this order".

**Edge cases**

- A `MatchOver` outcome per pending command at the end is expected and is not a pipeline error: `record_outcome`
  correlates by index as for any other command.
- `validate` runs again in `prepare_tick`, so a foundation finished while `CancelConstruction` was pending is
  `Capability`, reported in status.
- `Build` with `queue: true` from a selection where several can build: all are recipients, and the simulation picks
  the nearest.
- Editor playtest uses the same driver (`editor/app.rs:200`). New `BattleSession` fields need `Default` only.

**Tests first** (`commands/tests.rs`)

- Replace `absent_session_and_unsupported_capability_are_explicit` so it uses `CastAbility` for unsupported, and add
  `return_cargo_needs_cargo`.
- One test per table row, capable and incapable, including a mixed `submit_selection`.
- `cancel_construction_reaches_an_unfinished_foundation`, `every_other_order_refuses_a_foundation`.
- `build_goes_only_to_units_that_can_raise_it`, `build_of_an_unknown_structure_is_a_typed_rejection`.
- `commands_after_the_match_are_refused_at_submission_and_at_preparation`.
- `prepared_sequence_indices_match_real_game_outcomes`, extending the test at `:211`.

`driver/tests.rs`: `a_refused_build_reaches_the_session_as_a_notice`, `task_events_of_every_tick_in_a_frame_survive`.

`status/tests.rs`: a compile-exhaustive table test with one case per `GameCommandError` variant; every message is
non-empty and has no `EntityId` text.

---

### WP3: Dome fill (5.8, C-11)

**Goal.** A foundation fills from rim to apex in proportion to construction.

**Files**

- `assets/shaders/bubble_dome.wgsl`
- `crates/client/src/bubble_shader.rs`
- `crates/client/tests/bubble_optics_tests.rs`
- `crates/client/tests/lab_scene_tests.rs` (or a new `dome_fill_tests.rs`)

**Change.** Replace `in.uv.y` with a height from the sphere centre that the shader already computes:

```wgsl
// Share of a shell's height below this fragment: 0 at the base, 1 at the top. A dome stands on its
// centre, so only its upper half counts. UVs cannot give this: Bevy's UV sphere has its poles on Z.
fn construction_height(world_y: f32, center_y: f32, radius: f32, shape_kind: u32) -> f32 {
    let height = (world_y - center_y) / max(radius, 0.001);
    if (shape_kind == 1u) {
        return clamp(height, 0.0, 1.0);
    }
    return clamp(0.5 * (height + 1.0), 0.0, 1.0);
}
```

In the fill block use it for the comparison, the scanline and the grid's vertical term. Keep `in.uv.x` for
the other grid term. Correct the misleading comment at `bubble_shader.rs:426`: the dome is a full sphere whose
lower half lies below ground. Fix the comment on `build_progress` in both the WGSL and Rust structs.

`BubbleUniformGpu` layout is unchanged, so `bubble_optics_tests.rs:285-310` still holds.

**Presentation of queued jobs (D-11).** No code. Add to `snapshot.rs:54-57`'s doc comment that a client draws only
the foundation case in the world. Add a test that pins the behaviour.

**Tests first**

- `the_fill_reads_height_not_uv`: via `shader_ir::production_module("bubble_dome.wgsl")`, assert `construction_height`
  exists and that the fragment entry calls it. Add a Rust mirror only if it is compared numerically against the
  compiled shader in the GPU test. Do not assert on source text.
- `only_a_constructing_dome_uses_its_progress`: `BubbleUniformGpu::from_instance` with `Idle` and `Some(0.3)` leaves
  `anim_state != 0`, and with `Constructing` carries `0.3`.
- A real pixel test under `gpu_run`: the windowed child from WP9 captures a foundation early and late. Compare the
  mean luminance of the lower and upper thirds of the dome's screen rectangle: the early frame's upper third shows
  the grid, the late frame's does not. It lands with WP9, because that child is what can stage a foundation.

**Edge cases.** The Lab's "Construction Progress" slider feeds the same uniform (`scene_controller.rs:185`). It must keep
working for domes and for unit spheres, which is why `shape_kind` is handled. `smog/mod.rs:507` passes `Some(1.0)`
with `Walk` and is unaffected.

---

### WP4: Context orders and command modes

**Goal.** Right click means what the target implies. Attack-move and `CommandMode` exist. No HUD yet: the card comes
in WP6, but this package fully works by key.

**Files**

- `crates/client/src/input_bindings.rs`
- `crates/client/src/battle_input/mod.rs`
- `crates/client/src/battle_input/command_mode.rs` (new)
- `crates/client/src/battle_input/context_order.rs` (new)
- `crates/client/src/battle_pipeline/commands.rs`, `status.rs`, for `SubmissionError::SiteRefused`
- `crates/client/tests/battle_context_order_tests.rs` (new)

**Bindings**

```rust
/// Leaves a command mode or goes back one card page.
pub const CANCEL_KEY: KeyCode = KeyCode::Escape;

/// Number of command card positions.
pub const COMMAND_CARD_SLOTS: usize = 15;

/// Key of each command card position, in reading order. A position's key never depends on the unit.
pub const COMMAND_CARD_KEYS: [KeyCode; COMMAND_CARD_SLOTS] = [
    KeyCode::KeyQ, KeyCode::KeyE, KeyCode::KeyR, KeyCode::KeyT, KeyCode::KeyY,
    KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL,
    KeyCode::KeyZ, KeyCode::KeyC, KeyCode::KeyV, KeyCode::KeyB, KeyCode::KeyN,
];
```

Add both to `ACTION_KEYS`, so the existing uniqueness test covers them, and update `BATTLE_CONTROLS_HELP`.

**`command_mode.rs`**

```rust
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandMode {
    #[default]
    Normal,
    /// The next left click on the world orders an attack-move there.
    AttackMove,
    /// The next left click places this structure.
    Place(UnitTypeId),
}

/// The minimum corner a `Place` click at world point `(x, z)` means, if on the map.
///
/// Centres the footprint on the point. An extractor (`capabilities.is_gas_extractor`) snaps to cover a gas node within
/// `EXTRACTOR_SNAP_CELLS` of the point.
pub fn placement_cell(world: &SimWorld, structure: &UnitArchetype, x: f32, z: f32) -> Option<(u32, u32)>;
```

`CommandMode` has no input of its own in this package: nothing in WP4 enters a mode. The card does that in WP6, so a
person reaches attack-move and placement only after WP6. Each package is green, not complete for a player. WP4 tests
set the resource directly to test the state machine.

Do not add a dedicated attack-move key. Keys are positional (D-6). Do not add order marker kinds. New marker materials
are art direction, so `Harvest`, `Construct`, `Load` and `Build` reuse `OrderMarkerKind::Move`.

**`context_order.rs`**. Pure, tested without Bevy:

```rust
pub enum PointerTarget { Enemy(EntityId, Vec3), Friendly(EntityId, Vec3), Node(ResourceNodeId, Vec3), Ground(Vec3) }

/// The order a right click on `target` means for `selection`, first match wins.
pub fn context_order(world: &SimWorld, owner: PlayerId, selection: &[EntityId], target: PointerTarget)
    -> Option<(UnitOrder, OrderMarkerKind, Vec3)>;
```

Precedence, all by capability:

| Target | Condition on selection | Order |
|---|---|---|
| Enemy | any | `AttackTarget`, `Explicit` |
| Friendly foundation | any `can_raise` it | `Construct` |
| Friendly carrier | any can board, target not the sole selection | `Load` |
| Friendly completed dropoff | any `cargo` | `ReturnCargo` |
| Friendly completed extractor | any harvester | `Harvest` of the node under it |
| Node | any harvester | `Harvest` |
| Anything else | | `Move` to the ground under the pointer |

Neutral units count as ground, as at `battle_input/mod.rs:353`. A selection with no harvester right-clicking a node
gets `Move`.

The extractor's node: scan `world.map.resources()` for a node inside the structure's footprint. O(nodes) per click.

**Node picking.** In `BattleView`, add `pick_node(ray) -> Option<(ResourceNodeId, Vec3)>`: `ray_sphere_intersect` over
`MapPresentation.objects[..resource_count]`, with `visible` true, radius `NODE_PICK_RADIUS`. Index equals
`ResourceNodeId`, since objects are in map order with resources first (`scene.rs:62`). Pick the nearest. `pick_unit`
runs first, so a dome over a gas node wins. Apply `TERRAIN_OCCLUSION_MARGIN` as units do.

`BattleView` gains `map_objects: Option<Res<MapPresentation>>`. Optional because the resource appears only after the
first commit (`scene.rs:309`).

**Pointer flow**

Restructure `collect_pointer_intent` into `handle_normal_pointer` (today's body) and `handle_targeting_pointer`:

- If `selection.bind_owner` clears or `CommandMode != Normal` and the selection has no eligible unit, reset to `Normal`.
- In targeting, left press with `ownership.pointer`: resolve ground, then act by mode. Stay in the mode if Shift is
  held, otherwise return to `Normal`. No gesture begins.
  - `AttackMove`: `submit_selection`, `shift` queues.
  - `Place`: `placement_cell`, then `world.preview_site`. A refusal does not submit: `queue.reject` with
    `SubmissionError::SiteRefused(SiteRefusal)`, and the mode stays so the player can try elsewhere. Otherwise submit
    `Build`, and show a marker at the site's centre.
- In targeting, right press cancels and issues nothing.
- `CANCEL_KEY` in `collect_key_intent` resets the mode to `Normal`.

Order guard (D-12): `fn orders_open(world: &SimWorld) -> bool`, false once `match_state.result.is_over()`. Check it
before right-click orders and order keys, and reset the mode when it is false. Reset the mode on a new generation too,
beside `selection.bind_generation` (`scene_controller.rs:571`).

**Edge cases**

- A click that lands on an egui panel: `ownership.pointer` false, no order, mode unchanged.
- Selection empties while in a mode: reset to `Normal`.
- The window loses focus during a mode: the mode stays, since it is not a held gesture.
- The enemy under the pointer fails `can_pick`: falls through to Ground. That is today's behaviour.
- A multi-selected group whose only harvester dies between click and tick: `prepare_tick` reports `Capability`.
- Shift right click on a node queues `Harvest`. This is the normal end of a build order.
- `Load` for a selection above capacity: `CarrierFull` from the simulation, reaching the feed.

**Tests first**

`context_order` unit tests: one per table row plus a precedence case (a carrier that is also a dropoff, built from a
modified archetype in the test).

`battle_context_order_tests.rs`, real events on the skirmish:

- `right_click_on_a_mineral_sends_a_worker_to_harvest_and_income_rises`.
- `right_click_on_a_mineral_with_no_harvester_selected_moves`.
- `shift_right_click_on_a_mineral_queues_after_a_move`.
- `a_worker_with_cargo_returns_it_on_right_click_at_a_depot`.
- `right_click_on_a_foundation_resumes_it`: place by `CommandMode::Place` set in the test.
- `attack_move_mode_orders_the_ground_and_returns_to_normal`.
- `shift_keeps_the_mode`, `right_click_cancels_the_mode_and_issues_nothing`, `escape_cancels_the_mode`.
- `a_left_click_in_a_mode_changes_no_selection`.
- `orders_after_the_match_do_nothing`, with `world_mut` setting enemy structure HP to zero. This is a behaviour test,
  not the exit test.

- `a_refused_site_issues_nothing_and_keeps_the_mode`, `a_placed_build_pays_on_arrival`.

`input_bindings` tests extend automatically. Add `command_card_keys_avoid_panning_hold_and_stop`. **Verify** that
`Escape` reaches `collect_key_intent`, and that `bevy_egui` does not consume it.

---

### WP5: HUD shell and read-only panels

**Goal.** In the `Player` profile the inspector is gone, and the player sees resources, supply, selection and
messages. The world viewport excludes the panels.

**Files**

- `crates/client/src/battle_hud/{mod,catalog,model,panels,feed}.rs` (new)
- `crates/client/src/lib.rs`, `crates/client/src/lab_ui.rs`, `crates/client/src/input_policy.rs`
- `crates/client/src/media.rs`
- `crates/client/tests/battle_hud_tests.rs` (new)

**Plugin and ordering**

```rust
pub struct BattleHudPlugin;
```

Registered in `ClientAppPlugin` only. Editor and `run_headless_check` do not add it (`lib.rs:573`, `editor/app.rs:200`).

```rust
/// Visible rectangles of HUD controls in egui points, recorded each frame for hit tests and evidence.
#[derive(Resource, Default)]
pub struct HudGeometry {
    pub top: Option<egui::Rect>,
    pub bottom: Option<egui::Rect>,
    pub card: [Option<egui::Rect>; COMMAND_CARD_SLOTS],
    pub card_fixed: [Option<egui::Rect>; 2],
    pub queue: [Option<egui::Rect>; QUEUE_ROWS],
    pub pause: Option<egui::Rect>,
    pub speed_down: Option<egui::Rect>,
    pub speed_up: Option<egui::Rect>,
    pub play_again: Option<egui::Rect>,
    pub exit: Option<egui::Rect>,
}
```

All systems `.run_if(resource_equals(BattleProfile::Player))`:

```
CollectIntent:        battle_hud_system            .before(lab_ui_system)
PreparePresentation:  refresh_hud_model            .after(sync_live_battle_system)
                      collect_battle_feed          .after(refresh_hud_model)
```

**`lab_ui_system`** gains `BattleProfile` in its `battle` tuple. Under `Player`, after the egui context check:

```rust
geometry.targets.fill(None);
geometry.pixels_per_point = ctx.pixels_per_point();
geometry.inspector = None;
geometry.world = Some(ctx.available_rect());
return;
```

Because `battle_hud_system` ran first, `available_rect` already excludes the bars. No camera or picking code changes.

**`input_policy.rs:160`**:

```rust
ownership.pointer = inside && !context.wants_pointer_input() && !context.is_pointer_over_area();
```

`is_pointer_over_area` is false over a `Background` layer's unused rect, so a panel-free world is unaffected.
**Verify** in the Sandbox tests that nothing regresses, especially `a_box_finishes_on_release_over_the_inspector`.

**`catalog.rs`**. Built once per generation:

```rust
/// Labels and capabilities of every identity a card can show, from catalog data alone.
#[derive(Default)]
pub struct CommandCatalog {
    archetypes: BTreeMap<UnitTypeId, ArchetypeCommands>,
    upgrades: BTreeMap<u32, UpgradeCommand>,
}

pub struct ArchetypeCommands {
    pub label: String,
    pub mobile: bool, pub armed: bool, pub harvester: bool, pub overclock: bool, pub carrier: bool,
    pub structure: bool,
    pub trains: Vec<UnitTypeId>,
    pub researches: Vec<u32>,
    /// Structures this unit can raise, in catalog order. Empty for all but builders.
    pub raises: Vec<UnitTypeId>,
    /// Why this identity cannot be shown, if it cannot.
    pub unpresentable: Option<String>,
}
```

All strings come from `display_name`. `ClientMediaStatus` gains:

```rust
/// Whether every action the simulation can publish for `unit_type` is admitted.
pub fn presentable(&self, registry: &FlipbookRegistry, unit_type: UnitTypeId) -> Result<(), ClientMediaError>;
```

It calls `require` for each `AnimState`. `AnimState` needs an `ALL` const in `sim_types/enums.rs`. A new variant then
fails an exhaustive test rather than silently skipping.

**`model.rs`**

```rust
#[derive(Resource, Default)]
pub struct HudModel {
    pub view: PlayerView,
    pub catalog: CommandCatalog,
    pub viewer: Option<PlayerId>,
    generation: Option<SessionGeneration>,
    last_selection: Vec<EntityId>,
    /// One per distinct selected type, sorted by type.
    pub selection_groups: Vec<SelectionGroup>,
    /// Cached display text, rewritten only when its numbers change.
    pub resource_line: String,
    pub clock_line: String,
    dirty: bool,
}
```

`refresh_hud_model` runs every frame and is O(S) with no allocation when nothing changed:

1. On a new generation: rebuild `catalog`, clear feed, reset `CommandMode`.
2. If the world tick or viewer changed, call `write_player_view`. Read the world only here.
3. If `selection.ids() != last_selection` or step 2 ran: `last_selection.clone_from(..)`, rebuild `selection_groups`
   by binary search in `session.frames.presented().bubbles`, which is sorted by ID (`scene_controller.rs:674-677`).

```rust
// PERFORMANCE RATIONALE to include in the code:
// Why: This runs every frame, while its inputs change at 25 Hz at most.
// What: Compare tick and selection, then refill retained buffers only on a change.
// Trade-off: A little state in exchange for a frame with no world access or allocation.
```

**`panels.rs`**

`battle_hud_system` is thin: it passes data to pure draw functions.

- Top panel `battle_resources`, height 28: `resource_line`, `clock_line`, Pause, Speed `-` and `+` (bounds 1 to 4, as
  `lab_ui.rs:632`, writing `LabState`).
- Bottom panel `battle_command`, exact height 180, in three columns: selection, queue, card. In this package queue and
  card columns are laid out and empty. Under one selected unit show its name and health and shield bars from the
  presented bubble. Under several, list `selection_groups` as name and count. An inspected enemy shows name and health
  only.
- `egui::Area` `battle_feed`, anchored to bottom-left above the panel, `.interactable(false)`.

`format!` in a widget is forbidden on a per-frame path. Use cached strings. egui's own per-frame layout allocation
is outside our control and documented in a `PERFORMANCE RATIONALE` on the system.

Failures: show `media.failure_text()`, `owner_status.failure_text()` and `lab.battle_failure` in a red panel, in
the same priority as `lab_ui.rs:356-369`, including its Reset rule. This closes G-18 and belongs here, not in WP8,
so that no commit hides a failure.

**`feed.rs`**

```rust
pub const FEED_LINES: usize = 5;
pub const FEED_TTL_SECONDS: f32 = 8.0;

#[derive(Resource, Default)]
pub struct BattleFeed { /* fixed ring of FEED_LINES reusable Strings */ }
```

`collect_battle_feed` reads, for `viewer` only: `session.commands.status()` when it changes, `session.rejected`,
`session.notices` with `Err`, and `session.task_events` where `owner == viewer`. `Accepted` receipts are not shown.
`Refunded` is. `StructureCompleted` is a positive line. Ordinary rejections never stop anything.

Filter `session.notices` by viewer too, through a new `AdmissionOutcome::player()`. The client submits for one player
today, but a second local participant must not leak.

**Edge cases**

- `presentation.controlled` is `None` before the first refresh. Draw nothing and leave the rect at full window.
- `Player` profile with a failed scenario: `geometry.world` is still set, panels show the failure.
- Window narrower than the card: the card scrolls inside its column. Use `exact_height`, never `resizable`.
- A paused battle: `refresh_hud_model` still reacts to selection changes.
- The Lab's tab switching does not exist in `Player`, so `current_tab` cannot leave `LiveBattle`.

**Tests first** (`battle_hud_tests.rs`, booting `ClientMode::Skirmish`)

- `the_player_profile_shows_no_inspector_and_no_other_players_data`: `geometry.inspector` is `None`, and the HUD
  model holds only player 0.
- `the_world_viewport_excludes_both_panels`: camera viewport against `HudGeometry` rects.
- `a_click_on_the_bottom_panel_moves_nothing`, `a_right_click_on_the_top_panel_moves_nothing`: pending orders stay
  zero, selection unchanged.
- `a_drag_from_the_world_ends_over_the_panel_and_still_selects`.
- `resources_show_the_bank_and_change_with_income`.
- `a_refused_order_appears_in_the_feed`: an order that is admitted by the client and refused by the simulation.
- `a_task_event_of_another_player_stays_out_of_the_feed`.
- `the_model_allocates_nothing_on_a_frame_without_change`: buffer pointers and capacities stable across 200 frames.
- `a_presentation_failure_is_visible_without_the_inspector`.
- `the_feed_does_not_block_a_click_beneath_it`. This is the **verify** from D-4.
- `presentable_refuses_an_identity_without_media` and `presentable_accepts_a_shipped_identity`, in
  `runtime_media_tests.rs`, driven from catalog data.

---

### WP6: Command card, queue, placement

**Goal.** Every order is reachable from the card by mouse and by key. Build has a placement preview.

**Files**

- `crates/client/src/battle_hud/{card,actions,placement}.rs` (new), `panels.rs`, `mod.rs`
- `crates/client/tests/battle_card_tests.rs` (new), with fixture root helpers in `tests/support/battle_harness.rs`

**`card.rs`**

```rust
/// One thing a command card position can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardAction {
    /// Send this to the selection at once.
    Order(UnitOrder),
    /// Enter a targeted mode.
    Mode(CommandMode),
    /// Train at the selected capable structure with the shortest queue.
    Train(UnitTypeId),
    /// Research at the selected capable structure with the shortest queue.
    Research(u32),
    OpenBuildMenu,
    NextPage,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardPage { #[default] Root, Build { page: u8 } }

/// One filled position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardSlot {
    pub action: CardAction,
    pub enabled: bool,
    /// Which key shows on the button, and fires it.
    pub key: KeyCode,
    /// Identity in `CommandCatalog`, so the slot owns no string.
    pub label: CardLabel,
    /// First reason this cannot be used, if any.
    pub lock: Option<CardLock>,
}

/// The card on show.
#[derive(Resource, Debug, Default)]
pub struct CommandCard {
    pub slots: [Option<CardSlot>; COMMAND_CARD_SLOTS],
    /// Stop and Hold, which keep their own keys.
    pub fixed: [Option<CardSlot>; 2],
    pub page: CardPage,
}

/// Fills `card` for the selection. Pure, so every case is tested without Bevy.
pub fn derive_card(model: &HudModel, facts: &[SelectedFact], card: &mut CommandCard);

pub struct SelectedFact { pub id: EntityId, pub unit_type: UnitTypeId, pub foundation: bool }
```

`CardLabel` and `CardLock` are small `Copy` enums naming a catalog identity and a `sim_types::OptionLock` or media
reason. Default `CommandCard` is empty.

`foundation` comes from `anim_state == Constructing` on the presented bubble, as `snapshot.rs:54-57` documents.

Re-derive when `refresh_hud_model` reports a change or the page or mode changes, not every frame.

**Root page**, in order, each only if a selected unit qualifies:

| Entry | Qualifies | Action |
|---|---|---|
| Attack | mobile and armed | `Mode(AttackMove)` |
| Build | any non-empty `raises` | `OpenBuildMenu` |
| Return cargo | any in `view.laden` | `Order(ReturnCargo)` |
| Overclock | archetype `overclock` | `Order(UseStim)`; disabled unless the type is in `view.overclock_ready` |
| Unload | a carrier with `slots_used > 0` | `Order(UnloadAll)` |
| Cancel build | any foundation | `Order(CancelConstruction)` |
| Train X | union of `trains` over completed structures | `Train(X)` |
| Research Y | union of `researches` | `Research(Y)` |

`fixed` holds Stop and Hold if any selected unit is mobile.

**Build page.** `raises` of the selected builders, in catalog order, `Mode(Place(type))`, 13 per page, then `More` if
needed and `Back`.

**Enabled** is `lock.is_none() && unpresentable.is_none()`, per D-8. The tooltip names the first reason: "Requires
X", "Researched", "In progress", "Requires the previous level", "No approved media", or the cost. All from data.

An inspected enemy gives an empty card. So does a finished match.

**`actions.rs`**

```rust
pub fn apply_card_action(action: &CardAction, queued: bool, ctx: &mut CardContext<'_>);
```

`CardContext` holds session, selection, `CommandMode`, card page, viewer and model. Both the HUD click and the hotkey
call this one function.

- `Order`: `queue.submit_selection`.
- `Train`, `Research`: pick the shortest queue among selected capable structures, then `queue.submit`, with
  `entities: vec![best]`. `Shift` here has no meaning, since structures do not queue orders.
- `Mode`: set the mode. Entering `Place` of an unpresentable or locked entry cannot happen, since the entry is
  disabled, and `actions` re-checks it and rejects with status.
- Errors: `queue.reject(&error)`, which reaches the feed.

**Hotkeys.** `collect_card_keys`, a `battle_hud` system with its own `EventReader<KeyboardInput>`, runs
`.after(collect_key_intent)` under the same `ownership.keyboard` guard. For a pressed, non-repeat key, it finds the slot
and calls `apply_card_action`. A disabled slot writes its reason to the status. `CANCEL_KEY` with `CommandMode::Normal`
applies `CardAction::Back`. Ctrl is ignored, so `Ctrl+`letter does nothing here.

**Event order matters.** One frame may hold two taps, Build then a structure, so `apply_card_action` must re-derive the
card after a page change, before the next event in the same read. A stale page would fire the wrong slot. Test it with a
whole chord in one frame, as `a_whole_chord_delivered_within_one_frame_still_assigns_the_group` does.

**Queue panel.** For a single selected structure with a queue: up to `QUEUE_ROWS = 5` rows, each the item's label and
a progress bar computed as `(total - remaining) as f32 / total.max(1) as f32`. The front job shows live progress. Add
"+N more" past five. Clicking a row submits `CancelProduction { job_index }` by `queue.submit`, with
`entities: vec![structure]`. `exit_blocked` shows "Exit blocked" on the front row.

Edge: while paused, repeated clicks on the same row shift indices. Disable rows while any `CancelProduction` for this
structure is in `queue.pending_commands()`.

A foundation shows its construction percentage from the bubble in the same panel, with a Cancel entry. Several
selected structures show no queue, only a count of jobs per structure.

**`placement.rs`**

```rust
#[derive(Resource, Default)]
pub struct PlacementPreview {
    pub cell: Option<(u32, u32)>,
    pub verdict: Option<Result<(), SiteRefusal>>,
    last_tick: u32,
}
```

`update_placement_preview`, in `Present`, runs only in `Place`. It resolves `pick_terrain`, calls `placement_cell`, and
calls `world.preview_site` only when the cell or tick changed. It positions one pooled ghost entity: a flat quad scaled
to the footprint, with two fixed unlit transparent `StandardMaterial`s, legal and refused. Hidden outside `Place`.

Issuing is WP4's. The ghost is only sight, but both use `placement_cell` and `preview_site`, so what the player sees is
what the click does. The ghost is red for a known obstruction only, so it cannot reveal a hidden structure.

**Edge cases**

- Multiple selected unit types: card is the union, and `Build` goes to the nearest capable unit.
- A builder selected with a non-builder: `submit_selection` filters. The non-builder keeps its orders.
- Footprint over the map edge: `placement_cell` returns `None`, ghost hidden, and a click is rejected with `OutOfBounds`.
- Cursor over a panel: ghost keeps the last cell, and a click there is not a placement.
- The selection changes in `Place`: if no builder remains, reset to `Normal`.
- Money ran out between preview and arrival: `BuildAbandoned(Minerals)` in the feed. The order was free until then
  (`construction.rs:1-13`).
- Fog: an unexplored site previews legal if terrain allows. On arrival, a hidden building ends the order in the feed.
- Supply cap: `Train` entry stays enabled, and the `Supply` rejection reaches the feed.
- Stale card: the structure dies between frames. `submit` reports `MissingRecipient` or `EmptySelection`.

**Tests first**

`card.rs` unit tests (no Bevy): worker, structure, foundation, mixed group, inspected enemy, carrier with and without
passengers, laden and unladen, 17-entry Build page paging, tech lock, unpresentable entry.

`battle_card_tests.rs`, real clicks using `HudGeometry` rects and real keys on the skirmish with shipped media:

- `build_key_then_structure_key_then_click_raises_a_building`, asserting the foundation, its `Constructing` state and
  the payment.
- `the_same_flow_works_by_mouse_alone`.
- `a_whole_build_chord_in_one_frame_reaches_the_structure_page`.
- `a_site_on_a_cliff_shows_refused_and_issues_nothing`.
- `a_hidden_enemy_structure_does_not_turn_the_preview_red`: pan to the enemy start at tick 0 and hover its town hall.
- `the_extractor_card_entry_is_disabled_for_want_of_media`, on real assets: this is the guard for O1.
- `train_cancel_and_refund_through_the_queue_panel`.
- `train_goes_to_the_shorter_queue`.
- `cancelling_a_foundation_refunds_three_quarters`, reading the percent from `CONSTRUCTION_CANCEL_REFUND_PERCENT`.
- `a_disabled_entry_issues_nothing_and_says_why`.
- `stop_and_hold_appear_and_their_keys_still_work`.

With a **fixture root** (D-9), the real loader on a temporary directory holding real files plus procedural atlases
for the identities each test needs:

- `gas_through_the_card_extractor_snap_harvest_and_income`.
- `research_through_the_card_then_overclock_works`.
- `load_and_unload_through_right_click_and_the_card`. This one starts from a scenario file written into the temporary
  root whose loadout includes a carrier, so no world writes are needed.

---

### WP7: Fog presentation

**Goal.** In the `Player` profile, nothing outside the viewer's knowledge reaches the screen.

**Files**

- `crates/client/src/battle_hud/fog.rs` (new)
- `crates/client/src/battle_pipeline/driver.rs`
- `crates/client/src/runtime_battle/scene.rs`
- `crates/sim_types/src/snapshot.rs`
- `crates/client/tests/battle_fog_tests.rs` (new)

**Systems**

```rust
/// Viewer knowledge in the shape presentation needs. Written from `HudModel.view`.
#[derive(Resource, Default)]
pub struct BattleFog {
    pub viewer: Option<PlayerId>,
    width: u32,
    visible: Vec<bool>,
    /// Last seen presence of each node, in map order. Starts true for all.
    pub node_present: Vec<bool>,
}

impl BattleFog {
    /// Whether map position `(x, y)` is in a `Visible` cell. False off the map.
    pub fn sees(&self, x: f32, y: f32) -> bool;
}
```

- `refresh_battle_fog` follows `refresh_hud_model`, on tick change. `node_present` updates only for nodes in
  `view.visible_nodes`.
- `apply_battle_fog`, in `PreparePresentation`, `.after(sync_live_battle_system)`, after its `apply_deferred`. For each
  `LiveBattleBubble`: an owned bubble stays `Inherited`; any other is `Inherited` if `fog.sees(world_pos[0], world_pos[1])`,
  else `Hidden`. Use `set_if_neq`. O(N) per frame, no allocation.

Because `pick_unit` already requires `InheritedVisibility` (`battle_input/mod.rs:180`), a hidden enemy is unpickable,
so G-21 closes with no change to picking. Overhead bars and rings are children and inherit it. `units_in_box` selects
own units only.

- `publish_combat_events` gains `fog: Option<Res<BattleFog>>`. It must stay optional, since the editor runs this system
  (`editor/app.rs`). An event is dropped if `event.owner() != viewer` and `!fog.sees(anchor)`. It must still pop, so
  the event queue drains. Add `CombatEventVisual::anchor(&self) -> [f32; 2]` in `sim_types/snapshot.rs`, returning the
  map-plane point: `to` for tracers and beams, `pos` otherwise. **Verify** the axis order against `vfx.rs`.
- `update_map_presentation`: when `Some(fog)` has a viewer, read `fog.node_present` instead of
  `published.resources_present`. The `Sandbox` path is unchanged. The system runs in `PostUpdate`, so the order is
  already after the refresh.

Check smog: `smog/mod.rs:759` reads `Option<&Visibility>` for bubbles. **Verify** that a hidden bubble emits no wake.
If it does, skip hidden bubbles there in this package.

**Edge cases**

- Selection `inspected` of an enemy that fades: `selection.forget` is not called, since it is still in the snapshot. The
  panel must show nothing about it: `refresh_hud_model` clears `inspected` when its bubble is hidden.
- An enemy selected with attack order in progress: units continue. Explicit targets are the simulation's business.
- A control group is own units only, and never hidden.
- Enemy structures disappear in fog (D-10). The pilot in WP9 handles this.
- `Sandbox`: `BattleFog.viewer` stays `None`, no system runs, and all current tests remain valid.

**Tests first** (`battle_fog_tests.rs`, skirmish, `ClientMode::Skirmish`)

- `an_enemy_outside_vision_is_hidden_and_cannot_be_picked`.
- `an_enemy_in_vision_appears_and_can_be_attacked`: walk a worker to the enemy base by input.
- `a_right_click_on_a_hidden_enemy_moves_there`.
- `combat_out_of_sight_sends_no_visual_events_and_the_queue_still_drains`.
- `a_node_exhausted_out_of_sight_stays_on_screen_until_seen`: uses `world_mut` to zero a far node, then walks in.
- `an_inspected_enemy_that_leaves_vision_leaves_the_panel`.
- `the_sandbox_profile_still_draws_everything`.

---

### WP8: Result screen

**Goal.** The match ends in a screen the player can act on.

**Files**

- `crates/client/src/battle_hud/result.rs` (new)
- `crates/client/tests/battle_hud_tests.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerResult { Victory, Defeat, Draw, Eliminated }

/// What `result` means to `viewer`. `Eliminated` is a viewer out of a match others still play.
pub fn viewer_result(result: MatchResult, players: &[PlayerStatusView; 8], viewer: PlayerId) -> Option<ViewerResult>;
```

`Eliminated` exists for larger matches: `match_state.rs:17-18`. `submit` already refuses them through
`PlayerDefeated`.

`egui::Window::new(..)`, centred, not collapsible, not resizable, title-less. Text: Victory, Defeat, Draw, or
Eliminated, plus the match length from `MatchResult`'s tick at 25 Hz, converted with `SIM_TICK_PERIOD`. Buttons
`Play again` (sets `lab.battle_reset_requested`) and `Exit` (`AppExit::Success`), recorded in `HudGeometry`.

Draw it from `session.frames.presented().match_result`, so the screen appears with the displayed tick, not one tick
early. The world keeps ticking behind it. Pause is left alone.

`Play again`: `manage_battle_scene` commits a new generation (`scene.rs:143-146`, `:302`), and `HudModel`, `BattleFeed`,
`BattleFog`, `CommandMode` and `BattleSelection` reset on the generation change. WP5's rule covers the first,
and WP4's the mode. WP7 adds the fog reset. Assert all of them here.

**Edge cases**

- A click on the window never reaches the world: `is_pointer_over_area`, from WP5.
- The window appears on a frame when `CommandMode` was `Place`: WP4's guard resets it, and the ghost hides.
- Orders pending when the match ends: refused and reported, and the feed shows one line, not one per order.
  Deduplicate adjacent identical lines in `BattleFeed`.
- Reset fails: `lab.battle_failure` shows through the banner, and the result window is suppressed.

**Tests first**

`viewer_result` table test, all cases. Then `battle_hud_tests.rs`, staging ends with `world_mut`, since these test the
screen and not the path to it:

- `destroying_the_enemy_shows_victory`, `losing_the_last_structure_shows_defeat`, `both_falling_on_one_tick_shows_a_draw`.
- `no_order_is_accepted_after_the_end_but_selection_still_works`.
- `a_click_on_the_result_window_reaches_no_world_system`: a click at its centre, on the press frame.
- `play_again_starts_a_new_match_with_clean_state`, by a real click.
- `exit_ends_the_app_successfully`.

---

### WP9: Exit proof

**Goal.** Prove the exit criterion, and update the records.

**Files**

- `crates/client/tests/support/skirmish_pilot.rs` (new)
- `crates/client/tests/skirmish_match_tests.rs` (new)
- `crates/client/tests/support/battle_harness.rs`
- `crates/client/tests/gpu_validation_tests.rs`, for the windowed run
- `docs/plans/2026-11-01-codebase-remediation.md`, `AGENTS.md` only if a claim changed

**The pilot.** One Bevy plugin, used by both runs.

```rust
pub struct SkirmishPilotPlugin { pub stop_after: PilotGoal }
pub enum PilotGoal { MatchResult, FoundationRaised }
```

It runs in `PreUpdate`, before input processing, and emits at most one input event per frame: `CursorMoved`,
`MouseButtonInput` or `KeyboardInput`, the same events `Harness` sends, written by the same small `InputScript` queue.

**What the pilot may read (D-13):** `HudModel.view`, `BattleSelection`, `HudGeometry`, `MapPresentation`, the camera, and
bubbles whose `InheritedVisibility` is true. **Not** `BattleSession::world()`, which `skirmish_pilot.rs` must not
mention. Add a test that reads the pilot's source and fails if it does.

**What it may write:** nothing except events. No `world_mut`, no `RtsCamera`, no `LabState`.

**Stages,** following the non-tech script of `full_match_tests.rs`:

| Stage | Input |
|---|---|
| Speed | Click Speed `+` to 4 |
| Mine | For each worker: left click it, right click a mineral |
| Raise | Wait for the cost, from `options`. Click a worker, Build key, structure key, click the site, Shift right click a mineral |
| Train | Wait for the structure in `selection_groups`. Click it, press the train key when affordable, until 6 units exist |
| Group | Box select the army, `Ctrl+1` |
| March | Pan by held keys, `W` and `D`, to the enemy start, which is map data. Recall `1`, card Attack, click the ground |
| Assault | While enemy bubbles are visible, keep attack-moving. When no mobile enemy is visible and a dome is, right click it |
| End | Read the result window, and record it |

Sites come from `PlacementPreview`, not from constants: move the cursor outward from the start until the verdict is
legal, as a person would. The catalog type comes from the card: the
first enabled Build entry whose archetype `trains` a mobile armed unit. **No slug appears in the pilot.**

A guard fails the run past 12,000 ticks, printing the stage and bank, as `full_match_tests.rs:398-403` does. Any
`session.diagnostics` entry or `lab.battle_failure` fails immediately. A rejected order fails too, except rejections
the pilot provokes on purpose (none on this path).

The pilot must keep the world point it clicks inside `geometry.world`, panning if it falls under a panel.

**Headless test** (`skirmish_match_tests.rs`)

```rust
#[test]
fn a_match_is_won_through_real_input_alone() { ... }
```

`Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO)`, add the pilot, `app.update()` until the goal. Then:

- `match_result` is `Victory` for player 0.
- The result window is on screen, with `HudGeometry.play_again` set.
- `session.outcome_count > 0`, and no rejection occurred.
- A further right click issues nothing and the feed says why.
- `Play again`, by real click, gives tick 0 and `InProgress`.

Expected cost: about 8,000 ticks at 4 per frame, so 2,000+ frames, plus the sim's debug cost, "half a minute"
per match per `full_match_tests.rs:462`. **Measure it.** If it exceeds two minutes in a debug build, raise
`[profile.test]` optimisation for `sim_core` and `sim_map` rather than shortening the match. Do not gate this test
out of the suite.

**Windowed run on lavapipe.** A whole match is not possible here: `gpu_run.rs:37` allows 300 s and says lavapipe takes
most of 60 s for 30 to 60 frames. So the windowed run proves the **real renderer**, and the headless run proves the
**whole match**. Neither alone meets the criterion, and the plan does not claim otherwise.

The test re-executes its own test binary as the child, so no scripted-play code ships in the client:

```rust
#[test]
fn the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window() {
    if gpu_run::gpu_tests_opted_out() { return; }
    let mut command = gpu_run::virtual_gpu_command(&std::env::current_exe().expect("test binary"));
    command.args(["--ignored", "--exact", "--nocapture", "windowed_skirmish_child"]);
    command.env("SPACE_SCUFFLE_WINDOWED_CHILD_OUT", directory.path());
    gpu_run::assert_passes_validation(&mut command, "windowed skirmish");
    // then assert on the screenshots
}

#[test]
#[ignore = "launched by the test above on a virtual display"]
fn windowed_skirmish_child() {
    // build_graphical_app(ClientMode::Skirmish, None, SKIRMISH_SCENARIO), add the pilot with
    // PilotGoal::FoundationRaised, then render_compatibility::run_render_app.
}
```

- Under Xvfb with no window manager `Window.focused` may stay false, which disables all input by design
  (`input_policy.rs:149`). The child's plugin sets it true each frame. This lives in test support only. **Verify.**
- Script: select a worker, open Build, show the ghost, place a building, let the foundation start. Capture.
- Then, in this child only, stage the end by writing enemy structure HP to zero through `world_mut`, and capture the
  result window. This is a rendering test of the window, not a claim of play, and the test's doc comment says so.
- Assert: exit status zero, validation clean, the foundation's dome shows the fill (WP3's luminance check), and the
  screenshot differs between the panel region and the world.

Remember the capture artefact: Bevy screenshots of egui panels come out flat. So screenshot assertions cover the
world view and the ghost, not HUD text. HUD content is asserted through ECS and `HudGeometry` in the headless tests.

A `client --skirmish --max-frames 60` run through `assert_passes_validation`, added beside
`live_battle_passes_vulkan_validation` (`gpu_validation_tests.rs:181`), covers the CLI path.

**Records.** In `2026-11-01-codebase-remediation.md`: mark 5.1 to 5.8 and C-11 done, and note the new
`skirmish_match_tests.rs` under 1.9. Update `DESIGN.md` only if it states the old controls. State the run command:
`cargo run -p client -- --skirmish`.

---

## 5. Risks

| Risk | Effect | Mitigation |
|---|---|---|
| Media decision O1 goes the other way mid-phase | Fixture tests become real-asset tests | D-9 needs no code change; only tests swap roots |
| `is_pointer_over_area` changes Sandbox behaviour | Inspector tests regress | WP5 runs the whole `battle_input_tests.rs` first. Revert to a result-window-only guard if it fails |
| `Knowledge::Player` admits more than today | A test relying on an early refusal fails | Only hidden sites differ. A failure there is a correct behaviour change; adjust the test in the same commit, with the reason |
| Exit test takes minutes in debug | Gate slows on a shared CPU | Measure in WP9. Raise test optimisation, do not skip |
| Pilot stalls on steering or a blocked exit | Flaky exit test | The script is deterministic with `ManualDuration`, and the match seed is fixed. A stall prints its stage |
| Xvfb has no focus and cursor warp is odd | Windowed child does nothing | Set focus in test support; inject events directly, as the headless path does |
| egui layout shifts move rects by a frame | Clicks miss in tests | `HudGeometry` is recorded each frame, and the pilot reads it on the frame it moves the cursor |
| `selection.forget` drops boarded units from control groups (`scene_controller.rs:679-687`) | A group loses passengers after unload | Accepted for this phase; a test pins it. See §6 |
| A hidden enemy structure reappears with no memory | Player loses a known target | D-10; attack-move to the place still works |
| Fixture atlases escape to a runtime path | Charter §1.2 breach | Written only under `tempfile::tempdir`. A test asserts `assets/textures` is unchanged after the run |

---

## 6. Out of scope

- Enemy AI, new units or structures, and art-direction work, per the brief and remediation §6.
- Terrain fog or shroud rendering, remembered enemy structures, and a minimap.
- `CastAbility` and all ability UI. Stim is the only active command.
- Rally points, idle-worker shortcuts, select-all, double-click to select type, and camera jump on group recall.
- Keeping passengers in control groups across a load.
- Per-frame allocation inside egui itself. Phase 6 measures it.
- Rebinding keys, menus, audio, and networking or replay.

---

## 7. Decisions for the owner

**O1: Real media, or a smaller skirmish?** This is the one decision that changes what a player gets.

With shipped media, a real match can mine, build three structure types, train two unit types, and win or lose. Gas,
research and transport are built and tested but disabled in the real game, shown as "No approved media".

- **Recommended: accept this for the Phase 5 exit**, and treat the media as Phase 8.5 work. The exit criterion is
  met honestly, and no code changes when the art lands.
- **Otherwise: approve a paid generation run first.** Minimum sets:
  - gas and research: `plasma_siphon_dome`, `tech_module_pod`
  - transport, additionally: `machining_plant_dome`, `aerospace_hangar_dome`, `repair_shuttle`

**Assumptions, each overridable without replanning:** remediation decision D6 stands, so procedural atlases are
allowed in temporary test fixtures. Fog hides enemies but draws no shroud (D-10). `client` with no flags keeps opening
the Visual Lab, and players use `--skirmish`.

---

## 8. Addendum: follow-ups found by WP0 to WP8 (written before WP9)

Stage 1 addendum under charter §6. Packages WP0 to WP8 are merged. Each item below was found by one of them, is
fixed before WP9's exit proof, and lands as its own commit, failing test first.

**A1: the media gate asks only for what the simulation can publish (D-9).** `ClientMediaStatus::presentable` required
every `AnimState`, including `Walk` for an immobile dome. Authentic extractor media without a walk clip would then
stay disabled, which breaks D-9's promise that real media switches an entry on with no code change.

- `sim_core/src/published_actions.rs` owns the rule `UnitCatalog::publishable_actions(&UnitArchetype) -> ActionSet`,
  a `Copy` bit set. It reads capability data only: `Idle` always; `Constructing` when any archetype `can_raise` it;
  `Walk` when it can ever move (mobile by default, a mode that clears `is_immobile`, or a burrow toggle); `Attack`
  when it can move and can ever be armed; `Cast` for a harvester or a builder; never `Die`, which leaves the
  snapshot. The module table names the writer of `Entity::current_anim` behind each rule.
- The rule may over-approximate, never under-approximate. Two tests hold it to what worlds emit:
  `published_actions_tests.rs` (every archetype fights and casts every ability on open ground) and
  `full_match_tests.rs` (every snapshot of the scripted match, which adds harvesting and construction, and must
  show all five states).
- `load_registry` stores each identity's set beside its admission, and `presentable` checks only that set.
- Tests: `presentable_ignores_an_action_the_simulation_never_publishes` (a shipped dome stripped of `Walk` stays
  presentable), and `presentable_refuses_an_identity_missing_a_single_action` now strips an action the rule names.
  The fixture root fills only the published gaps.

**A2: stim admission checks the unlock (G-8 remainder).** `capable` for `UseStim` checked only the archetype, so a
mixed selection of a researched and an unresearched overclock type was admitted whole, and the unresearched units
silently did nothing. Add `pub fn can_overclock(world, entity, archetype) -> bool` in `battle_pipeline/commands.rs`:
the archetype has an overclock with a positive rate, the unit can pay its health, and
`SimWorld::is_ability_unlocked(archetype, owner, OVERCLOCK_ABILITY_SLUG)`. `capable` takes the world and calls it, so
`submit_selection` drops the locked members and `submit` refuses a command naming one. The inspector's roster
`Overclock` already goes through `submit_roster_order`, which now applies the rule; its button is shown only when
`PlayerPresentation::overclock`, now counted with the same function, is non-zero. Tests: one mixed selection in
`commands/tests.rs` (only the unlocked type receives the order, and both do after research) and the roster path.

**A3: a real image of the result window.** WP8's X display grabs came back black. Capture the result window from
the virtual X server's own framebuffer while the production child shows it, look at it, and keep it under
`docs/evidence/` with a note on how it was made. Bevy screenshots do not count, because they show egui flat.

**A4: a new match starts unpaused at normal speed.** Pause and speed live in `LabState`, so they carried into
`Play again`. `manage_battle_scene` resets both when it commits a replacement generation. The first commit leaves
them alone, so a harness or capture that boots paused keeps doing so. Test: pause and raise the speed through the
HUD, finish the match, press `Play again` by a real click, and assert `Pause` is off and the speed is 1.

**A5: no order marker survives into a new session.** The marker pool is spawned once, so a marker shown in the last
0.7 s of a match survived `Play again`. `OrderMarkerAssets` records the session generation, and
`update_order_markers` hides the whole pool and restarts the slot cycle when it changes. That system runs in
`Present`, after `CommitSession`, so any marker issued earlier in the replacing frame belongs to the old session.
Test: issue an order, press `Play again` within the marker's lifetime, and assert every marker is hidden.

**WP9 deviations from §4, decided here.**

- *Windowed child staging.* A lavapipe frame of the shipped map takes seconds, and the clock caps a frame at five
  ticks, so the windowed child cannot earn minerals or wait out construction inside `gpu_run`'s 300 s limit. Its
  pilot raises the foundation through real input, as in the headless run, but the child first funds the bank and
  later sets the foundation's construction share, both through `world_mut`. That makes it a rendering test of the
  HUD, the ghost and the fill, as §4 already says of its staged end, and its doc comment says so. The headless test
  stages nothing.
- *The fill check uses WP3's ring method,* not the thirds of the dome's screen rectangle, because WP3 showed the
  thirds check passes on the broken `uv.y` shader. The child projects rings of the live foundation's surface through
  the live camera. A ring between the early and the late share must change between the two shots far more than a
  ring below both shares, which is built in both.
- *Rejections the pilot provokes.* None on the path, as §4 says. The test counts `BattleCommandQueue` refusals and
  `BattleSession::rejected` records and fails on any.
