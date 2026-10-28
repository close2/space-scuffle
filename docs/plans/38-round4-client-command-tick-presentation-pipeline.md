# Round 4 architecture: one client tick owner, queued commands and continuous snapshot presentation

Status: architecture approved and implementation released after the round7 combined gate and bounded47 gesture review/verification. Stage1 reusable snapshot APIs and client command/timing primitives are underway. Preserve45 movement ownership; its owner has relinquished step/snapshot extraction and both production/movement outcome clears remain required.

## Goal and boundaries

Every live-battle command enters the same deterministic tick path. Rendering continues smoothly between ticks from retained immutable snapshots, paused scenes show the exact latest authoritative pose, and every completed tick's combat events reach presentation exactly once. Reuse client snapshot/event/command storage instead of allocating and discarding buffers every frame. Preserve current simulation rules, loaded scenario/catalogs, renderer31 lifecycle, optics, capture34 and input/UI36 contracts.

This is a staged client pipeline refactor plus a narrow reusable snapshot-output API in sim_core. It does not rewrite networking, combat mechanics, world entity storage, the editor, map format, terrain art or general asset loading. Zero-allocation claims apply precisely to the warmed steady-state presentation/empty-command path under test; spawn/despawn, user actions, capacity growth, Bevy internals, UI rendering and existing simulation internals must not be misrepresented as universally allocation-free.

## Verified defects and current contracts

- `lab_ui::issue_sandbox_command` mutates `SimWorld` directly with `world.step`, including while paused, and discards its returned snapshot. That bypasses the fixed tick accumulator and loses its drained combat events/checksum presentation. UI command order relative to normal stepping is currently not an explicit schedule contract.
- `sync_live_battle_system` combines timing, simulation, event transfer, spawning/material updates and pose smoothing. It creates `accumulated_combat_events: Vec` every frame, formats a new checksum String after each produced snapshot, and calls allocating `build_render_snapshot` on every paused frame.
- `SimWorld::step` increments `current_tick`, processes the borrowed input bundle, runs all authoritative phases, updates checksum and returns `build_render_snapshot`. That builder allocates `bubbles` and takes `pending_events`, leaving its allocation behind. Building a snapshot therefore also consumes events; it is not a harmless read-only refresh.
- On an unpaused render frame with insufficient accumulated time for a tick, `latest_snapshot` is None and synchronization returns before pose smoothing. On a paused capture frame with delta0, the exponential lerp factor is0, so an already lagging Transform can remain wrong indefinitely. The distance-based snap threshold is not a substitute for a defined presentation timeline.
- Dead-entity removal scans all snapshot bubbles for every render entity (`retain` plus `iter().any`), giving quadratic membership work. The BTreeMap itself is persistent but does not make the whole system allocation-free or linear.
- Snapshot bubble order follows deterministic BTreeMap entity order. Snapshot floats are created at the approved fixed-point→render boundary. `CombatEventVisual` carries self-contained visual coordinates but no tick; the bridge must attach its producing snapshot tick without altering authoritative state.
- Existing callers supply `TickBundle.tick = world.current_tick` before `step`, while the returned snapshot has the incremented tick. Preserve and document that existing input/output boundary in this refactor. Do not silently reinterpret tick numbers or introduce an off-by-one replay change.
- Capture34 changes ManualDuration between0 and40ms and expects the ordinary game tick owner to execute; it does not require an alternate direct-step path. Tab switching currently freezes battle stepping and discards its Bevy bubble entities; reset replaces the world. Preserve those externally visible lifecycle decisions while retaining reusable presentation buffers appropriately.

## Stage A: command queue and sole authoritative tick owner

Introduce focused live-session resources/modules rather than adding another large mixed system:

- `BattleCommandQueue`: ordered pending player commands, monotonic local submission sequence, and session generation. Each command captures its intended recipients at submission. Do not resolve a later selection instead. Store typed submission/rejection diagnostics separately from the simulation's tick diagnostics.
- `BattleTickClock`: integer Duration accumulator, declared40ms tick interval, speed multiplier and per-frame catch-up budget. This wall-clock scheduler is client state, not authoritative simulation state. Conversion to interpolation float occurs only when deriving the presentation fraction.
- `BattleFrameState`: previous/current reusable snapshots, explicit initialization/generation, queued tick-tagged visual events and presentation cursor. It owns snapshot publication, never simulation mutation from rendering.

Only `advance_battle_ticks` calls `SimWorld::step_into` during an active session. Initial world construction/reset remains an explicit transaction; UI, selection, capture scripts and VFX must never call `step` or simulation phases directly. Keep compatibility APIs for headless simulation users.

The initial client migration covers the existing movement/attack/hold/overclock controls; it does not add new construction or ability UI. UI/controller actions produce intent. Resolve command recipients using actual ownership and the relevant catalog capability: movement requires movable recipients; attack orders require an applicable weapon; overclock requires its optional capability and valid current activation constraints. Unsupported actions report typed rejection rather than sending misleading orders. No unit-name/type-ID branches. A participant/opponent comes from the scenario-derived selection introduced in36, not fixed slots0/1.

Queue submission may allocate for a new user action; it is not a frame synchronization allocation. Keep ordered storage and reuse the outer tick-bundle command Vec. Preserve user submission order, including repeated nonqueued orders and queued/Shift orders; do not silently coalesce orders that may have side effects. The command source and execution order must be explicit before the tick owner runs.

Commands submitted while paused remain pending and visibly indicated. They do not advance the world. Commands execute once in the first subsequent eligible tick, then leave the queue; no-tick frames do not consume them. Commands for dead/changed-ownership recipients are revalidated at that tick, with a visible typed outcome and no substitution of other units. Reset clears old-generation commands and presentation events; commands cannot leak into the replacement world. Switching to another tab retains the same session's pending commands but does not execute them while the battle is suspended. Existing pause controls remain authoritative for the client clock.

The current0.25-second accumulator clamp discards wall-time debt, not authoritative tick numbers: the simulation slows relative to real elapsed time, while each executed checksum-bearing tick remains sequential. Preserve that distinction in code comments, diagnostics and tests.

Introduce an explicit validated `BattleClockPolicy`:40ms authoritative tick period, the existing five-tick per-frame catch-up budget, and the existing250ms maximum retained simulation-time debt as compatibility defaults. These are named client scheduling policy values, not unit mechanics. The supported speed range remains0(pause) through4 until a separately declared policy changes it. Validate positive tick period, nonzero frame budget, maximum debt at least one period and permitted speed at session/configuration setup; reject invalid policy rather than silently clamp it.

Compute `delta.checked_mul(speed)` and accumulator `checked_add` before mutation. Overflow or an unsupported speed produces a typed clock diagnostic, pauses further advancement and leaves pending commands/world untouched for that frame; never wrap Duration or silently substitute a duration. Diagnostic totals/counters also use checked updates rather than wrapping.

If the checked sum exceeds maximum debt, retain exactly the configured maximum, record the exact discarded scheduling duration after speed scaling and increment a visible slowdown/debt-limit diagnostic. Expose retained debt, last discarded scaled scheduling duration and its cumulative total in the existing diagnostics presentation/capture evidence, and log the transition into a limiting episode without flooding every frame. This deliberately preserves bounded interactive slowdown after a stall instead of accumulating unbounded hidden lag. It never increments tick numbers without executing their simulation phases and never discards queued commands.

After executing the frame's tick budget, retain the bounded remainder for the next frame. Test long stalls, speed4, repeated overload and return to normal frames. Same tick-indexed commands and seed must yield the same checksums regardless of render grouping; tests must compare results at equal executed tick counts rather than incorrectly demand equal ticks per wall-clock second under the documented slowdown policy.

A tick with simulation diagnostics is still an actually executed tick, not a transaction that can be pretended rolled back. Retain its snapshot/events/typed diagnostics, stop further catch-up, pause the session and emit failure through the existing runtime contract. Preserve completed earlier ticks in the same frame. Do not return from the loop after discarding their evidence.

## Stage B: reusable simulation snapshot output

Add narrow APIs with explicit output ownership:

```rust
pub fn step_into(&mut self, input: &TickBundle, output: &mut RenderSnapshot);
pub fn write_render_snapshot(&mut self, output: &mut RenderSnapshot);
```

`step` and `build_render_snapshot` remain compatibility wrappers that create an output and call the corresponding implementation. They must not duplicate simulation phase execution. Document that `write_render_snapshot` consumes pending visual events exactly once, matching existing builder semantics; callers may not use it as a per-frame getter. A separate read-only snapshot refresh API is unnecessary for this round.

`write_render_snapshot` clears/reuses the output's bubble storage, writes stable entity-ordered render records, and transfers pending visual events while retaining reusable capacity on both sides. Use existing Vec operations with clear ownership rather than cloning every snapshot. Reserve capacity only when actual growth requires it; do not hardcode a creature count or allocate an arbitrary maximum-size world buffer. Initial session allocation/growth is explicit. After the event bridge has consumed a snapshot's events, its storage is recycled for later extraction.

The client alternates previous/current output buffers via swaps. It never deep-clones both snapshots each render frame. Initial session setup extracts once and marks the initial pose as exact. Ordinary paused/no-tick frames read retained data and never call the builder. Update the checksum display only when a new checksum is published, using a retained String and `fmt::Write` rather than `format!` on every presentation frame.

Required equivalence tests run identical worlds through `step` and `step_into`, compare snapshot bytes/fields and checksums tick-for-tick, verify event draining once, and confirm repeated no-event writes retain buffer capacity. Include dynamic spawn/despawn and bursts of combat events. This API must not change simulation math, command semantics, checksums or catalog behavior. Existing snapshot serialization remains compatible.

## Stage C: a precise presentation timeline

Use two adjacent completed snapshots and a documented one-tick interpolation delay during continuous play. Let previous/current be ticks n−1/n and alpha be the retained accumulator fraction in[0,1]. Continuous pose fields interpolate from previous to current at every render frame, including frames with no simulation tick. If catch-up debt exceeds one interval after the tick budget, clamp alpha to1 and report the retained debt; do not take modulo and falsely rewind presentation while still behind. Never extrapolate across a cliff or invent a simulation position.

Interpolate position/terrain elevation and shortest-arc heading/aim only when the same entity exists in both snapshots and its spatial identity remains compatible. Keep shape, archetype, ownership, health, ability state and other discrete data tied to the displayed tick. Do not blend across entity replacement, creation/destruction, a reset or incompatible shape changes. Newly visible entities appear at their displayed tick; entities removed by the current snapshot remain at their last valid pose until the presentation cursor reaches the removal tick. A new session and paused presentation show current data exactly without a synthetic predecessor.

Pause transition snaps presentation to the current authoritative snapshot, drains all visual events through that tick once and resets the interpolation phase. Repeated paused frames—even delta0—hold that exact pose without rebuilding snapshots or replaying effects. Resume holds the exact current pose until a new adjacent snapshot establishes the next interpolation pair; it must not jump backward to an obsolete predecessor. Tab return and reset likewise invalidate old interpolation history and present the retained/new current snapshot exactly before normal interpolation resumes.

All render-facing pose consumers must agree. The entity Transform, interpolated BubbleInstance world position/heading used by indicators and mist, camera projection/selection and relevant material orientation must derive from the same presented pose. Keep authoritative discrete snapshot data distinguishable from client overlays such as selection; presentation must not overwrite selection every tick. Do not leave Transform interpolated while wake/indicator source data still claims the current tick's un-interpolated position.

Animation-state changes are discrete. Do not interpolate progress across unrelated animation states or assume every animation loop can be averaged; preserve the current approved animation contract unless a separately tested same-state phase rule is needed. This round's required continuity concerns spatial pose and timing, not an unreviewed animation-system redesign.

### Combat events share the displayed timeline

Attach `(produced_tick, within_tick_sequence, session_generation)` to events as each tick is extracted. Retain them until the presentation cursor reaches their tick; flush all newly eligible events in stable tick/order sequence. A one-tick position delay must not fire a tracer from a future pose immediately while its source unit is still visibly at the previous pose. Pausing snaps to current and flushes events through current once; ordinary zero-delta frames do not resend them.

If several authoritative ticks occur before one presentation frame, retain and deliver every eligible event from every completed tick, including a short-lived entity that spawns and dies between presented frames. Event coordinates already carry the source tick's visual positions; do not retarget them to whatever entity currently occupies the selection. Keep last-consumed sequence/tick state so replaying or reusing a snapshot cannot duplicate effects. Clear old-generation events only on an explicit session reset/replacement.

The event staging buffer is persistent and capacity-reusing. VFX entity/material allocation caused by a newly emitted effect is a separate lifecycle cost; it does not justify allocating aggregation scratch on input-free render frames. Existing VFX consumers should receive a stable ordered stream after presentation eligibility is determined.

## Stage D: reconciliation and scheduling

Split entity reconciliation from per-frame interpolation. Reconcile additions/removals when the displayed discrete snapshot changes, not by scanning the entire world every frame. Since snapshot bubbles and authoritative IDs are ordered, use a linear merge/two-pointer walk or a reusable generation-marked lookup; eliminate `retain`×`snapshot.iter().any`. Retain the existing render entity map for direct entity access. Allocate Bevy entities/materials only when their lifecycle actually changes.

Define explicit client system sets, coordinated with36 and34:

1. native/UI input ownership and intent collection;
2. transactional reset/session changes;
3. fixed tick advancement and snapshot/event publication;
4. presentation cursor update and displayed-snapshot entity reconciliation;
5. per-frame pose/material presentation;
6. eligible combat-event delivery and VFX/mist consumers;
7. transform propagation, selection/projection and capture observation at their correct frame boundaries.

Exact Update/PostUpdate placement must preserve36's current-frame camera transform/picking contract and34's controls-before-game-state and readback-after-real-render contract. Avoid a schedule cycle by distinguishing collection, reconciliation and observation. Make deferred entity creation visible before downstream consumers that require those entities. The prior public `sync_live_battle_system`/alias must be migrated deliberately with call sites; do not keep a hidden second tick owner for compatibility.

## Verification matrix and rejection criteria

### Deterministic command/tick behavior

- Queue commands on frames with0,1 and multiple ticks; assert exact execution tick and one execution per command.
- Pause, enqueue movement/overclock, run zero-duration frames: checksum/current_tick unchanged; resume consumes commands once at the documented boundary.
- Preserve submitted order and Shift queue behavior. Reject unsupported capability/invalid player; test dead or transferred recipients without substituting other entities.
- Reset after queued commands and events: new generation gets neither old intent nor old effects. Tab suspend/return retains only the current session's intended queue.
- Replay the same tick-indexed command trace with30/60/144Hz and jittered frame groupings; compare every authoritative checksum and command application sequence. Render timing may differ; authoritative results may not.

### Snapshot/event conservation

- Compare old/new extraction APIs across movement, combat and spawn/despawn using actual catalog-defined worlds.
- Produce events in each of several catch-up ticks, including death and a short-lived entity; verify ordered exactly-once presentation, no last-snapshot-only loss.
- Force a diagnostic on a later catch-up tick; retain earlier completed tick evidence and report failure without continuing.
- Assert snapshot building drains once and does not run on unchanged paused/no-tick frames; repeated reads cannot steal pending events.

### Presentation behavior

- At known snapshot endpoints/midpoints, verify world-space pose, ground elevation and shortest angular wrap across±π. Position updates must occur on a frame with no new tick.
- Use actual directed ramp traversal fixtures to prove interpolated movement follows adjacent legal simulation samples. Do not smooth over topology discontinuities with arbitrary distance thresholds.
- Pause while rendering between ticks: exact current pose on that frame, including Time delta0; resume without a backward jump.
- Creation, removal, reset and tab return respect displayed-tick membership and event eligibility. Selection, indicator, wake and Transform coordinates agree.
- Verify any queued event deferred by interpolation is flushed once when pausing or crossing its tick; no ghost effect from reset generations.

### Allocation/performance evidence

Verify Vec/String capacities and storage identity remain stable after warmup, then measure the isolated steady-state extraction/presentation path with an actual allocation counter/profiler. Capacity assertions alone are not proof of zero allocations. Use a vetted test-only measurement mechanism or external allocation profiler without introducing unsafe code into project modules. Distinguish simulation-internal and Bevy/UI allocations by stack/phase; do not claim whole-frame zero allocation from a narrow measurement. Exercise no-tick frames, paused frames, ordinary ticks, event bursts and growth separately. Check linear reconciliation on increasing entity counts without an O(N²) membership loop.

### Actual visual/runtime gate

Use the real production builder, shaders, assets and renderer31 shutdown. Extend capture34 with tracked moving-unit samples between ticks, a paused exact-pose checkpoint and a command-driven encounter through the ordinary queue. Capture at multiple presentation rates and inspect consecutive screenshots/video for motion continuity, ramp contact, tracer alignment, stable pause/reset and visible intended action. Direct transform assignment or a mocked world cannot certify this path. Every capture must include fresh readback and truthful completion metadata.

Run targeted tests at each stage, then coordinated `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. Keep all stages unmerged/unaccepted until the client uses one coherent path and actual visual evidence passes. A queue alone is insufficient if events still disappear; interpolation alone is insufficient if ticks can still be advanced by UI.

## Ownership and staged review checkpoints

Proposed new client modules: battle commands/clock, retained frame state and presentation/reconciliation. Narrow changes: `lab_ui.rs` intent submission only, `scene_controller.rs` removal of mixed tick/sync code, `runtime_battle/scene.rs` reset-generation handoff, `lib.rs` scheduling, `runtime_capture` observer compatibility, sim_core reusable snapshot APIs and focused tests. Preserve36's UI/input ownership and responsive viewport work; implement38 only after those interfaces stabilize.

Checkpoint1 reviews queue/tick semantics and exact snapshot API equivalence. Checkpoint2 reviews retained timeline/event conservation and allocation evidence. Checkpoint3 reviews actual multi-rate moving/paused/commanded game captures and full gates. Any change to combat rules, snapshot wire format, networking semantics or unrelated simulation allocation internals requires a separately reviewed scope amendment.

## Architecture refresh after36,43 and46

This refresh is approved by the orchestrator, including the last-published UI model and committed-generation metadata deltas. It is documentation only during the combined verification freeze; implementation remains held until explicit release. The existing source has individual `.before/.after` edges rather than a shared game pipeline `SystemSet`; the sets below are proposed38 interfaces, not an assertion that they already exist.

### Exact current schedule and migration edges

Current `ClientAppPlugin` Update edges are `manage_battle_scene → sync_live_battle_system → refresh_battle_presentation → lab_ui_system → sync_scene_camera → camera_control_system`. Scene management is additionally before camera control. Selected actor identity refresh is after Lab preview and before UI; input ownership and world viewport application are after UI and before camera control. Several VFX and presentation systems are registered without mutual ordering. Consequently simply adding `lab_ui_system.before(sync_live_battle_system)` creates a cycle through the existing battle-presentation refresh edge. Replace these edges deliberately rather than appending contradictory ones.

Capture34's `driver::control` runs in Update before scene camera, scene management, camera controls, live synchronization, Lab preview and UI. Its egui input-isolation adapter stays in PreUpdate after `EguiPreUpdateSet::ProcessInput` and before `BeginPass`. Its `driver::observe` remains in Last. Preserve actual render-world screenshot alignment after `prepare_view_targets` and before `RenderSet::Render`, fresh screenshot completion and renderer31 shutdown; no capture-only tick owner is introduced.

Define public client schedule sets with these responsibilities and ordered boundaries:

1. `ExternalControl`: deterministic capture/controller setup before ordinary intent collection. Native input processing and egui BeginPass remain in their engine-owned earlier schedules.
2. `CollectIntent`: UI reads the retained published model and writes commands, pause/tab/reset intent. Synchronize selected actor identity from currently existing presentation entities before UI. Sample `InputOwnership` and apply the actual world viewport after UI has established current response rectangles, preserving36's same-frame ownership decisions.
3. `CommitSession`: restore per-tab camera state before scene management, so successful reset/new-map camera framing remains the final transaction result. Scene management prepares and commits the world/root once. Apply deferred commands explicitly before the session initialization observer and later systems query the committed root/resources. No tick executes against a partially committed replacement.
4. `AdvanceTicks`: sole tick owner checks active tab, fatal media/simulation status and pause state, consumes eligible generation-matched queue entries and publishes every executed tick. It drains typed diagnostics and production outcomes after each tick, before another catch-up iteration can clear them.
5. `PreparePresentation`: move the presentation cursor, reconcile displayed membership, and apply deferred spawns/despawns before pose consumers. Refresh cached battle counts, queue/clock/outcome summaries after publication for the following UI pass. Remove the old refresh-before-UI edge; the UI deliberately displays the last fully published model instead of running a second UI pass or creating a cycle. Initialization/committed metadata must be ready before controls become actionable.
6. `Present`: update all interpolated pose sources together, then indicator/material consumers. Camera controls remain after current UI ownership and committed scene camera, and before propagation. Deliver eligible combat events before `process_combat_events_system`, then make deferred VFX spawns available before their first required consumer. Existing Lab/Swarm producers retain one active scene policy; changing their scene lifecycle or animation rules is outside this refactor.

Configure these boundaries once in the normal plugin and reuse them in real production/capture/direct-schedule fixtures. Do not maintain a compatibility alias that secretly steps again. `sync_live_battle_system` and `step_client_simulation_system` exports and all explicit capture/test ordering references must migrate together.

PostUpdate remains the engine-facing boundary:46's environment stage applies committed globals/root visibility before `VisibilitySystems::VisibilityPropagate`; it captures the real Lab baseline only in PostStartup. Transform propagation and camera projection update precede36 selection. Bubble optics stays after `TransformSystem::TransformPropagate` and `VisibilitySystems::CheckVisibility`. Current mist geometry runs in PostUpdate before transform propagation;38 supplies its presented pose beforehand but does not repair the separately known mist light-filtering defect or retime35's renderer storage work. Runtime map billboards and Last capture observation must observe the chosen committed generation and current camera. Render extraction and acquired-surface initialization continue using46's active globals.

### Successful commit is the only reset-generation boundary

Introduce a checked session generation shared by the command queue, clock, frame state and the successfully committed root. Do not infer a new session from `BattleDefinition::is_changed`, an allocation address, a tab click, reset request or temporary absence of a root. Candidate definition changes can fail while46 correctly keeps the previous world/root/environment.

At successful `manage_battle_scene` commit, after all fallible world/map/asset preparation, publish the new generation with the new root and world. Check generation increment before committing; exhaustion is a typed failure preserving the old session. The subsequent deferred boundary makes the committed root, generation and resources visible together. Initialize the new snapshot once, reset the clock/interpolation state, discard old-generation pending intent/events/notices, and refresh the committed session's cached labels. Candidate failure changes none of those identities or retained buffers and continues exposing its existing fatal diagnostic. A successful reset invalidates all prior commands even if the new entity IDs and seed happen to match. Tab suspend/resume does not increment generation.

The existing46 committed environment component remains attached to exactly that successfully prepared root. Its ownership and baseline restoration must not be replaced by reading the latest candidate theme. Preserve the now-tested pending-first-root behavior and standard light visibility. Headless direct-schedule embeddings explicitly initialize their supplied session through the same session-initialization helper; missing production resources do not become optional fallbacks.

Approved adjacent presentation consistency change: `refresh_battle_presentation` currently rebuilds metadata on candidate `BattleDefinition::is_changed`. During38 it should key session metadata to the committed generation, so a failed replacement cannot attach candidate player labels to the surviving old world. This is a narrow consistency change to the retained UI model, not a scenario loader or roster redesign.

### Exact43 outcome handoff and ownership agreement

The stable43 APIs are `SimWorld::production_outcomes() -> &[ProductionOutcome]`, `drain_production_outcomes() -> Drain<'_, ProductionOutcome>`, and `production_blocks() -> &[ProductionBlock]`. `ProductionOutcome` contains `tick`, `command_index`, `player`, and `Result<ProductionReceipt, ProductionAdmissionError>`. Ordinary admission rejections are nonfatal; `ProductionBlock::ExitObstructed` is current producer state regenerated by the production phase. Neither belongs in `CombatVisualEvent`, fatal `SimulationDiagnostic`, nor serialized `RenderSnapshot`.

The authoritative step clears `production.outcomes` and tick diagnostics before incrementing/executing the next tick. Preserve that exact order in `step_into`. Immediately after each executed tick, the client drains outcomes into reusable session-owned notification storage before continuing catch-up. Associate `command_index` with the aligned local submission-sequence metadata for that exact TickBundle; do not use current UI selection, outcome position, product identity or the next bundle as a lookup. Keep the authoritative outcome tick (the incremented/current execution tick), distinct from the input bundle's pre-step boundary tick. Tag the derived client notice with session generation and submission sequence. A malformed index or impossible generation mismatch is a client pipeline invariant error, not an ordinary affordability rejection.

Deliver ordinary outcomes to UI/capture evidence immediately after authoritative publication, independent of the deliberately delayed visual/combat-event cursor. Record every outcome once, including multiple ticks in one frame; maintain persistent counts/last-readable diagnostic without unbounded hidden notification growth. Refresh retained text only when outcomes change. Pause does not drain or replay a notice repeatedly; tab return retains current-session evidence; successful replacement clears it. Producer obstruction display is a latest-state view, not a stream of repeated command rejections. No production button is added by this round, and43 remains responsible for authoritative production validation, payment, supply and completion. Queue adapters/tests can exercise Produce outcomes through the same tick owner without duplicating43's admission logic.

Existing movement/hold/attack/overclock commands do not gain fictional authoritative success receipts. Distinguish client submission/recipient revalidation evidence from43's actual production outcome. Accepted queue admission means scheduled intent, not proof that every downstream mechanic succeeded. Preserve fatal simulation diagnostic handling separately and stop catch-up only for those failures; an ordinary Produce rejection must not pause the match or suppress earlier visual events.

Checksum owner confirmed the narrow implementation boundary:38 owns only extraction of `SimWorld::step` into the single `step_into` phase path and extraction of `build_render_snapshot` into the reusable writer. Preserve43's outcome-clear line and phase order.43 owns admission, spawn preparation/commit, supply/death lifecycle, production workspace/completion, checksum schema and production outcome types. No broad world.rs formatting/rewrite or simultaneous overlapping edits; announce exact method work after release.43 has no pending step-signature/snapshot changes.

Additional required regressions: two Produce outcomes in separate catch-up ticks survive the second tick's clear and map to their actual local commands; rejection remains nonfatal; a fatal later tick preserves earlier notices/events; failed replacement preserves old generation/queued commands/environment and published metadata; successful same-seed reset discards them; actual capture control runs before UI/ticks without a schedule cycle; the production environment/visibility tests from46 remain unchanged and passing. The queued-command source must replace direct UI stepping rather than leaving it reachable through an old helper.

Two existing initialization/failure boundaries remain explicit. `BattleDefinition::instantiate` applies validated declarative initial orders to the uncommitted starting world and recomputes its initial checksum without executing a tick; this remains scenario construction, not a second live tick owner. Do not route those initial orders through a later UI queue or shift the first gameplay tick as a side effect of38.

The exact39 media checks stay before initial advancement and at every newly produced animation-state snapshot. If a completed tick exposes unavailable media, retain its authoritative snapshot, drained command outcomes and already-produced events as failure evidence before applying the existing media policy. Do not manufacture valid visual assets, advance more ticks, or pretend the completed tick rolled back. Normal windows retain the readable media failure panel and disabled progression; bounded captures fail. Previous successfully rendered state can remain visible as the existing failure presentation, with no claim that it represents the failed tick. This closes the same catch-up evidence-loss class for media failure as for fatal simulation diagnostics while preserving39's admission policy.

## Integration ownership and post-gate verification sequence

Read-only coordination during round7's frozen gate establishes these method/file boundaries. None of this authorizes implementation before the orchestrator releases the applicable round.

| Owner | Product edit boundary | Shared-file restriction |
| --- | --- | --- |
|47 drag cancellation | Focused drag state/helper; camera middle-gesture block; native probe telemetry; required direct input fixtures | Finish and freeze camera/probe changes before38 changes schedule bindings. Do not move tick ownership or edit movement simulation. |
|38 tick/presentation | New client command/clock/frame modules; live section of scene_controller; UI command submission; runtime scene commit-generation handoff; cached battle model; plugin/capture ordering | Preserve47 geometry/gesture semantics,39 media admission,46 environment module and all Lab/Swarm behavior. Announce scene_controller/lib/capture hunks before concurrent visual work. |
|38 reusable sim output | `SimWorld::step`/new `step_into`, `build_render_snapshot`/new writer; snapshot equivalence tests | No spawn/admission/production/reap/movement changes, no snapshot schema or checksum changes. Preserve43 output clears and phase order. |
|43 production | Production module and lifecycle/outcome APIs, spawn/reap/economy and schema3 | Frozen accepted APIs remain inputs to38; any defect repair must be announced before touching the same world.rs revision. |
|45 movement | Movement policy module; Move/AttackMove/AttackTarget processing; pathing/steering/push/hazard displacement; layer fixtures and any justified checksum work | No camera/client UI/scene edits and no snapshot altitude/style changes. Step clear/outcome hook requires a serialized handoff with38 rather than simultaneous edits to that method. |

No ownership entry permits whole-file cleanup/formatting that rewrites another lane's methods. When world.rs method edits overlap in time, one owner waits for a compile-stable slice; do not rely on text-merging independently edited copies. Existing phase methods remain in their current order after38 extraction, so45 can work on their internals independently once that method boundary is stable.

45's exact typed invalid-target API is a coordination checkpoint, not a reason for38 to invent a generic authoritative success receipt. Movement policy owns validation/rejection reasons;38 owns consumption, local-submission mapping and visible status. Agree its concrete derived output type, recipient granularity, clear-at-step/drain timing and autonomous-movement failure distinction before implementing either side of the hook. Preserve43's established production outcome API rather than wrapping/relabeling ordinary rejection as fatal. If45 is not yet integrated,38 can complete its existing-control/43 behavior and tests, then add the movement handoff as one reviewed slice; do not add an unused placeholder branch.

The verification sequence after a successful shared gate is:

1. **47 focused implementation first.** Run pure ordered-gesture state tests, scheduled camera/input policy tests with actual projection/nonzero viewport origin, then the real production schedule and direct headless embeddings. Check strict client all-target Clippy and workspace formatting. Build/copy one immutable probe only after that source passes. Reserve one native interval for the exact delivered-event cancellation/rearm matrix at scale1 and one nonunit scale; preserve failed delivery attempts. Freeze47 after independent evidence review before changing its registration under38.
2. **38 snapshot extraction checkpoint.** Serialize the two narrow world.rs methods with45. Compare `step` and `step_into` outputs/checksums using identical worlds, including queued production, combat events, death and ordinary rejection; prove draining events/outcomes once and buffer reuse. Run sim_core's production transaction/timer/checksum and movement regressions appropriate to the stable45 state. No client behavior changes are accepted from this API checkpoint alone.
3. **38 sole-owner queue/schedule checkpoint.** Remove direct UI stepping and switch the client coherently to one tick owner. Test pause/enqueue/resume, command order, zero/multiple-tick frames, checked clock limits, reset generation, failed candidate preservation and immediate43 outcome drain. Include actual production plugin startup, direct schedule/headless resources, runtime capture ordering and unchanged46 environment/visibility tests; these catch required-resource or cycle failures before native work.
4. **38 retained presentation checkpoint.** Verify no-tick interpolation, exact paused pose, resume without rewind, membership changes, every catch-up event and failure evidence, consistent Transform/instance/indicator pose and reused buffers. Take isolated allocation evidence only after relevant code is stable, explicitly separating Bevy/VFX/user-action allocations. Do not benchmark during another lane's build/GPU work.
5. **38 actual runtime checkpoint.** Build/copy the final production capture binary, reserve native/GPU use, and capture moving intervals, paused exact pose and queued commands through the actual UI/controller path. Inspect tracked consecutive images at more than one presentation rate; manifests must report authoritative ticks/poses, presented pose/event cursor and command outcome identity. Then freeze all participating sources and run the coordinated complete workspace tests, strict Clippy and formatting. Functional tests never confer visual-art acceptance.

Direct-schedule inventory must be updated deliberately: `run_headless_check` in client/lib.rs, `round2_visual_tests::test_bevy_ecs_schedule_all_systems_no_conflicts`, `input_policy_tests`' minimal camera app, runtime_battle_tests' partial scene app, and capture34's explicit system ordering. They currently register production functions/resources manually. New required resources/events must be installed explicitly; an `Option<Res<_>>` fallback or skipped failing fixture would defeat the production contract. Existing native input and46 lifecycle tests remain separate guardrails after38 schedule migration.

### Stage-one checkpoint and monotonic publication preflight

The first implementation checkpoint introduces the real reusable simulation output APIs and complete, independently tested command/clock/timeline primitives. It deliberately leaves the existing production scheduling and UI command caller intact until the coherent stage-two migration. This checkpoint does not claim the old direct-step UI defect, render interpolation defect, or per-frame allocations are repaired in the running game. The primitives are ordinary compiled library code with no second tick owner or alternate production behavior installed.

Client timeline initialization has an explicit unpublished phase: generation replacement clears prior evidence, the real world writer fills the initial buffer, and `finish_initialization` publishes exactly once. Neither ticking nor presentation can use that unfinished buffer. Visual event eligibility additionally waits for the first actually published pose of the new generation. Reused or older generation activation fails before clearing queued commands or retained presentation. All replacement identity checks occur before the production scene-commit boundary when stage two is wired.

The client rejects an exhausted `u32` simulation tick identity before `step_into`, command dequeue, snapshot-buffer rotation, or event drain. This protects the client timeline's monotonic event comparisons without altering the existing headless world's wrapping tick behavior; headless wrap remains an explicit separate limitation. Tests hold the real world at `u32::MAX` and require identical checksum, tick, queued command count, current/presented snapshots, and both simulation/presentation event evidence after rejection. Nonfinite or out-of-range interpolation fractions and missing/double lifecycle completion are typed failures, never implicit resets.

Command admission separates direct attacks from movement: AttackMove requires mobility and a weapon; AttackTarget permits stationary armed actors and requires a live exposed hostile target plus an applicable authored weapon (including an active siege weapon). Hold/Stop/Idle remain the existing mobile-roster control policy rather than a general assertion that stationary simulation actors cannot receive any such order. Actual authoritative production and movement receipts retain their existing semantics and exact command indices. Standalone tests exercise preparation followed by the real `step_into` and immediate drains; full per-frame notification delivery remains stage-two work.

Buffer pointer/capacity tests demonstrate reuse only for the specific retained vectors being inspected. They are not allocation-profiler evidence and do not establish a whole-client or whole-simulation zero-allocation claim.

### Frozen-checkpoint review amendment: reuse authoritative combat availability

Status: approved after the shared gate; implemented in the released38 source slice. The focused target-availability tests pass; full production migration verification remains in progress.

The queue's current direct-target filter duplicates only existence, ownership, live HP and transport exclusion. The existing `SimWorld::is_entity_available_for_combat(attacker_owner, target_id)` already delegates to the simulation's shared predicate and additionally rejects invulnerability and a cloaked target lacking both current visibility and detector coverage. The client must call this public predicate instead of maintaining a competing subset. This is admission consistency with existing simulation behavior, not a new vision, diplomacy or targeting mechanic.

Keep the checks that the shared predicate deliberately does not own: validate the submitting player; resolve the target entity; require target.owner != submitting owner because the current sim has owner hostility rather than alliance state; validate the live target's catalog definition; and check each recipient's `weapon_against(target.mover_type, active_siege_weapon)` as currently implemented. AttackTarget must not require mobility, reachability or present weapon range: stationary weapons and mobile approach remain valid. AttackMove still requires the existing mobile/armed roster policy. Do not replace authored weapon selection with a boolean “has any weapon” shortcut.

The shared predicate currently permits an ordinary uncloaked target in fog; only its cloaked case requires visible/detected cells. This amendment must preserve that existing behavior rather than silently adding a new all-target line-of-sight admission rule. If the game's command-visibility contract later changes, it belongs in an authoritative reviewed targeting change, not a client-only branch. The predicate also does not check team alliances because those do not exist. “Available for combat” is the exact claim; avoid describing the predicate as all-purpose validation of every attack mechanic.

Apply the same availability check both at submission and in the existing queued-command revalidation before execution. A target becoming invulnerable, cloaked without coverage, dead, transported or friendly before the tick creates the existing ordinary `InvalidTarget(target)` rejection, preserves world state and never fabricates a movement/production receipt. Missing live catalog definitions retain their fatal handling. Do not dequeue any pending batch if fatal revalidation fails. No new authoritative AttackTarget outcome is added;45's movement outcome API remains Move/AttackMove only.

Focused test design uses real catalog-selected actors and the actual public vision implementation, without creature IDs:

1. Start with an owned armed stationary recipient and hostile target whose layer has an applicable weapon. Establish the public simulation predicate returns true and queue admission succeeds without a tick/checksum mutation. AttackMove for that stationary recipient remains rejected.
2. Mark the hostile target invulnerable: require the simulation predicate false, immediate submission `InvalidTarget`, and no pending insertion. Separately queue while vulnerable, change invulnerability, prepare the next bundle, and require exactly one correlated ordinary rejection with no command or world mutation.
3. For cloak, construct coverage through `PlayerVisionMap::begin_tick_update`, `apply_vision_source` and `end_tick_update` at the actual target position. No coverage rejects; ordinary sight without a detector rejects; sight with a detector accepts. A target outside the map rejects. Use valid supported map coordinates and a real authored mover/radius; don't edit private detection arrays or replace the predicate with a mock.
4. Queue under detector coverage, remove the detector source for the next real visibility update, then prepare: reject that same captured target once, retain unrelated valid pending commands and their aligned sequence indices. Restoring coverage permits a genuinely new submission, not resurrection of the rejected one.
5. Preserve the existing friendly/dead/missing/loaded target and missing-catalog cases. The loaded case may use the current helper until the separately reviewed transport identity migration is released; that migration must update the fixture to an actual carrier, never exempt the queue check.
6. Assert an uncloaked fogged target remains equivalent to the authoritative predicate. This explicit regression prevents an accidental client-only fog policy from being mistaken for correctness.

Run all battle_pipeline tests, the existing combat retaliation/projectile tests that exercise combat availability, strict client/sim_core all-target Clippy and formatting after the released change. Integration into the real queued tick owner is still required before claiming this affects ordinary gameplay. The quiet/frozen round adds only this architecture text.

Review clarification: availability precedes target-archetype lookup. An unavailable target is an ordinary `InvalidTarget`; if the target is otherwise available but its live definition is missing, preserve fatal `MissingArchetype`. During the eventual sole-owner integration, run timeline identity preflight before `prepare_tick` can dequeue intent. Once an actual tick executes, publish its output and retain that tick's outcomes/events even if a subsequent presentation or media failure stops the frame. These are approved ordering constraints, not permission to edit source during the shared gate.

### Allocation measurement fixture and evidence limits

Use a dedicated CPU integration fixture loading `ResolvedAssetsRoot`, the actual default `BattleDefinition` and external catalogs, then the ordinary `setup_bubble_assets_system` / indicator loader. Install the real `advance_battle_ticks → sync_live_battle_system → publish_combat_events` chain with required resources. Eight paused app updates warm asset decoding, initial entities/materials, system state and retained vectors; verify readiness and no retained loader failure before measuring. This is real client presentation without a GPU window, UI or VFX entity consumer, not an alternative production loader.

Keep named non-inlined measurement functions for three separate observed workloads: 64 warmed paused updates (zero simulation ticks); moving no-tick interpolation updates with a pre-established adjacent snapshot pair and sub-tick time budget; and an explicitly separate authoritative tick/membership-change workload. The latter must be reported as lifecycle work and must not be folded into a steady-state zero-allocation claim. Validate matching published instance/Transform positions, retained world checksum when no tick executes, and actual membership changes respectively. Use real death/reap/supply lifecycle if a membership fixture needs a removal.

Run the existing filtered test executable under external Callgrind only after the coordinated quiet interval, with `--tool=callgrind --collect-atstart=no --toggle-collect=<exact demangled measurement symbol> --callgrind-out-file=<evidence path>`. Inspect the emitted function/call records and `callgrind_annotate --tree=both` for allocator descendants including Rust allocation/reallocation and libc allocation entry points. Resolve actual symbols from the built test binary rather than guessing a mangled function name. A separate named positive-control function performs an escaping `Vec` allocation and is independently profiled to prove allocator calls are recorded; it is outside the production measured phases. If compiler/library symbol stripping prevents identifying those calls, record the instrument as inconclusive instead of inferring no allocations.

Preserve raw profiler outputs, exact commands and binary hash. Instrumented execution times are not performance timings. Report allocation-call evidence only for the named warmed paths and observed iteration counts; this cannot establish absence of allocations in engine tasks, graphics drivers, UI, simulation internals, new effects or untested capacity-growth cases. No project unsafe allocator implementation or new dependency is needed.

Profiler setup refinement: whole-process instrumentation can spend the watchdog budget decoding real PNGs before the measurement phase. Preserve that attempt as inconclusive if it times out. A test-only, explicitly requested readiness handshake may start Valgrind with `--instr-atstart=no`; after the actual loader/warmup succeeds, write a readiness marker, let the external controller enable instrumentation for that exact verified PID, then release the named phase. The handshake has a bounded timeout and is absent from ordinary tests; no loader or image decode is bypassed. Marker/control allocations remain outside the named measurement function. Continue using the independent escaping-allocation positive control to verify collection.

### Narrow native queue and presentation evidence extension (awaiting review)

Keep the existing seven-checkpoint battle capture and its strengthened delayed-combat proof. Do not add a second simulation controller or relax its contact/frustum assertions. For the remaining queued UI and moving-frame evidence, extend the existing native input probe with an optional second, buffered pipeline CSV and an explicit optional presentation-step duration. The ordinary probe stays passive with its existing initial paused state. The optional duration uses Bevy's normal manual TimeUpdateStrategy for controlled virtual presentation cadence; it is recorded as virtual frame duration, never represented as measured monitor/GPU FPS. Test at 33,333,333ns and 6,944,444ns, preserving normal production queue/tick/presentation systems and actual Vulkan rendering.

The second recorder avoids adding parameters beyond the existing native-input system limit. It reads the real BattleSession, LabState, actual BubbleInstance/Transform components and camera after normal presentation/transform propagation. Record frame, elapsed virtual duration, successful generation, world/current/presented ticks, world/output checksums, alpha/debt, paused/readiness, pending command count, cumulative authoritative outcomes and last outcome identity. Track one deterministic actual moving render identity per committed generation, including unit type, owner, current and previous authoritative positions, published instance position, actual Transform position and window projection. Never choose a creature constant or manufacture a movement command. A recorder failure preserves the existing shared nonzero exit status after normal renderer/media shutdown.

The exclusive native driver enters Live Battle through its actual UI, inspects a screenshot and clicks the real Move order while paused. Verify native button delivery, one pending captured request, unchanged tick/checksum/position across subsequent paused frames, then click Resume and require pending count zero plus exactly one matching authoritative movement outcome at an actual next tick. The current scenario's other valid initial movement supplies the tracked moving actor; no direct entity edits or hidden control writes are used. The driver then observes multiple frames with unchanged world tick but changing published/Transform position, and validates their interpolation against the adjacent authoritative samples. Pause must publish exact current pose and remain stable; resume must not rewind before the next pair. Reset must increment generation and discard old pending/event evidence. The two virtual cadences exercise different tick/no-tick ratios through the same production scheduler.

Take real native screenshots before paused submission, with the queued request still paused, during movement, and after exact pause at each cadence; verify decoded images and actual visible actor/UI contents. Screenshot timestamps are bracketed by recorded frame rows and are visual evidence, not falsely labeled exact asynchronous GPU-frame tokens. Numeric per-frame claims come from the actual component/authoritative trace, while the existing battle_capture supplies exact attached GPU screenshot evidence. Preserve strict owned-PID checks and normal WM_DELETE closure; host input delivery failures remain failures rather than bypasses. Add pure recorder field/identity/exit handling tests and record this tool's allocations separately from the measured production paths.

Native extension review clarifications: preserve one tracked render entity ID within a successful generation; replace it only on an observed disappearance/death lifecycle and record the old/new identity. Reset verification must first create a real pending paused request, then observe its discard and the new generation. Treat the integer presented source tick separately from interpolation alpha; a fractional displayed pose is not falsely labeled an exact authoritative tick. Screenshots remain bracketed visual evidence only.

All-thread allocation refinement: function-triggered Callgrind collection on the test/schedule thread excludes worker-thread system bodies. Preserve that initial dump as insufficient for driver/synchronizer allocation claims. Following the same actual-loader handshake, enable instrumentation with collection already enabled for all threads; use explicit named phase-boundary dumps and halt collection/instrumentation at the end boundary before teardown. Include caller contexts where necessary to distinguish shared allocator callsites. Require actual `advance_battle_ticks`, `BattleSession::advance` and `sync_live_battle_system` bodies in the captured records before drawing conclusions. Do not switch the production or fixture scheduler to a different executor merely to simplify profiling. Attribute scheduler/worker allocations separately only when recorded call stacks permit; otherwise report an inclusive interval, not a nonexistent per-system count. Retain the positive allocation control and both earlier inconclusive attempts.

Approved fixture-lifecycle correction: the first all-thread dump exposed `Assets<BubbleDomeMaterial>::get_mut` growing its queued asset-event Vec because the minimal CPU fixture had manually inserted asset storages without the production AssetPlugin/`init_asset` event schedule. Preserve that detected allocation path and avoid labeling it a production defect. Install the actual AssetPlugin and register the existing mesh/image/material types through `init_asset`; let its normal asset schedule drain events. No manual clear, mocked storage or GPU renderer is added. Re-run the same named phases and positive control with the real loader and worker bodies present. Document actual stage ordering and separately observed lifecycle/simulation allocations.

### Final verification amendment: player-facing order wording

The native screenshots exposed Rust `Debug` receipts in the ordinary battle panel.
Keep the typed notices (generation, submission, tick, command index, exact result)
unchanged for diagnostics and the passive trace; render cached player-facing text
only when queue/outcome state changes. Use singular/plural `1 order queued` /
`N orders queued`, and `1 order submitted` / `N orders submitted`, without an
internal tick number. Accepted movement reads `Move order accepted for N units`
(with singular `unit`); accepted production reads `Production order accepted`.
Neither message claims arrival or production completion. Ordinary rejection maps
each typed variant to a readable cause: missing/destroyed/unavailable selection,
foreign ownership, unavailable capability/target/product/prerequisite, destination
outside the map, or insufficient minerals/gas/supply (retain numeric resource
requirements). Corrupt-state rejection says the battle cannot process the order;
full typed invariant details remain in Diagnostics. Do not format EntityId,
UnitTypeId, enum variants or Rust Result syntax in these ordinary status strings.

Verify singular/plural, accepted movement/production and exhaustive typed rejection
mappings with focused pure tests. The next settled native screenshots must show
these actual strings. This is presentation-only: no admission, queue, tick or
receipt semantics change.

Native screenshot checkpoints must wait after every action for actual subsequent
probe/render frames and readiness, including replacement generation after Reset.
A decoded image remains an independent acceptance check: successful CSV assertions
do not establish that an OS screenshot depicts the newly presented generation.
Retain the earlier stale screenshot artifacts and their limited claims.
