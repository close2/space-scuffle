# Round 55 — Explicit presentation motion discontinuities

Status: advisory architecture only; implementation is **not released**. Root review and final CPU3 successful isolated-cast evidence are prerequisites. Round52 product files remain frozen. This plan changes no mist field, reduction tolerance, sampling budget, movement mechanic, authored ability definition or map-editor behavior.

## Problem and current evidence

An authored `AbilityEffectSpec::BlinkTeleport` commits an exact destination in `SimWorld::try_execute_ability_prevalidated`. Its output is currently indistinguishable from an ordinary large move in `BubbleRenderInstance`: the snapshot carries pose and velocity but no semantic motion boundary. `BattleFrameState::compatible` compares identity, archetype, shape, owner and radius, then interpolates compatible positions. The real mist system observes these displayed transforms; `EmitterHistory::advance` clears its ring only when one observed displacement is strictly greater than `radius * continuity_radii`. Interpolation can split a successful teleport into several subthreshold displayed displacements and emit an artificial connecting trail. Distance alone cannot establish whether motion was continuous or an authored relocation.

Retain `/tmp/space-scuffle-r11-reduction-blink01.log` unchanged. It reports an interpolation bridge, but its transaction fixture also failed exact destination admission because the prior Move order advanced the actor after the cast. Cost/cooldown observations are not a substitute for the failed isolated-position assertion. The CPU3 owner is preparing a same-tick **Stop then CastAbility**, an otherwise identical Stop-only control, and a next-tick Move. The final executed transaction/output/display/history artifacts must be linked here before this becomes an implementation blueprint with accepted reproduction. Do not call blink01 a passed isolated-cast experiment or silently replace it with the corrected log.

Executed update: `/tmp/space-scuffle-r11-reduction-integration02.log` reports all5 focused integration tests passing, including exact successful generic Ground/Air Blink transaction assertions. For each layer, the tested window uses speed1,144Hz and the−1 phase offset with the positive Blink-distance offset. Both still report `FailureCategory::Discontinuity`; the passing test verifies that real command/transaction coverage occurred, not that the proposed representation passed discontinuity admission. Its printed frame780 is the then-current end-of-window reporting position, **not** the first bridge frame. The owner is adding retained first-crossing and old/new sample-birth evidence, including skipped multiple-tick boundaries. Do not promote this two-window checkpoint into completion of the full16296-window matrix.

Log SHA-256 identities: blink01 `fd9461b05c379507038cf85bd567d5e89316896d6aed57d5e0a76b4cf26c7213`; integration02 `bd91b6a47a80f2fcaa352767e6bb1789f4ea6a52a741238c8436ccdb3af7902a`. This update was established by read-only log/test inspection; this planning agent ran no shared build or test.

This is project presentation policy, not a claim of exact SC2 teleport graphics or timing. No unit identity determines the behavior. The actual semantic effect and successful destination transaction do.

## Existing path and boundaries

| Boundary | Actual current responsibility | Required preservation |
| --- | --- | --- |
| `sim_core/world.rs` ability preflight/commit | Checks cost, cooldown, range and exact destination before payment and relocation | Rejected/malformed casts preserve fields, including pending request under Plan50 |
| `sim_core/transport/mod.rs` unload transaction | Checks relation, completion, life and endpoint; clears carrier and relocates atomically | No new transport capacity/range policy; preserve captured supply and graph semantics |
| `SimWorld::write_render_snapshot` | Emits EntityId-ordered visible actor records into reused storage; drains combat events | Getter/publication does not create a discontinuity or mutate its counter |
| `battle_pipeline/timeline.rs` | Retains adjacent complete boundaries; interpolates pose and delays discrete state/events | Keep one-tick presentation lag and event release order |
| `scene_controller::sync_live_battle_system` | Applies the presented record to the same transform/material/`BubbleInstance` | Do not consult the newer authoritative pose to reset a still-old displayed actor |
| `mist/mod.rs`, `emission.rs` | Own bounded emitter histories and GPU incarnation identities | No synthetic connecting samples; no repeated reset on zero-tick frames |
| `mist/upload.rs` | Selects whole staged/retained packets and validates queued slot generation | An old packet cannot resurrect a pre-break wake under the new displayed identity |

Normal steering, collision pushes, ramp altitude following and target-arrival snaps remain continuous under their existing policy. Spawn/death and observed load absence already change membership; unload still needs an explicit boundary because catch-up can skip the absent snapshot. Manual public `Entity.position` edits are not inferred to be teleports: the production guarantee concerns checked semantic transactions. Catalog mover changes and unrelated morph semantics are not silently added to this round.

## Alternatives and proposed decision

1. **Keep/tune the distance heuristic:** rejected as semantic detection. It misses short teleports and interpolated long teleports, and can classify fast continuous travel as a break. Keep the existing defensive heuristic for nonsemantic jumps; do not use it as the acceptance oracle for Blink.
2. **Disable all interpolation or every ability's interpolation:** rejected. It changes ordinary motion/cadence and unrelated ability presentation without evidence.
3. **One-tick boolean or transient event:** possible only with an additional lossless accumulation/replay protocol. A flag disappears when several ticks execute before one display, and events require retained association to membership and display timing. Reusing combat events would conflate distinct contracts. No need for that queue here.
4. **Persistent per-actor continuity revision:** recommended. A scalar survives catch-up and repeated snapshots and requires no per-frame event allocation. Compatibility uses the revision; the emitter compares the revision it actually displayed. One or several undisplayed breaks are detected without inventing intermediate poses.

The proposal intentionally retains the existing large-jump history policy: old wake samples are cleared, rather than allocating another fading wake for every teleport. Whether that disappearance looks acceptable requires real visual review. This plan does not claim a separate fading-tail design has been implemented, or conceal an artistic rejection by labeling it a numerical pass.

## Simulation data and transaction contract

Add private retained `motion_revision: u64` to `Entity`, initialized to0 by the real entity constructor, with a documented read-only getter. Zero means the actor's initial continuous incarnation, not missing metadata. Add required `motion_revision: u64` to `BubbleRenderInstance`, copied verbatim by the snapshot writer. This is a logical continuity identity, not a tick, distance, ECS allocation, owner, shader time or presentation generation.

Only successful checked Blink relocation and successful unload increment it. A successful zero-distance semantic cast still establishes a boundary; do not infer intent from a nonzero displacement. Ordinary Move, AttackMove, push, stop, cast rejection, loading alone, snapshot extraction and visual frame delivery do not increment it. Loading already removes world membership; a later unload increments even if the client never displayed that absence. Reaping removes the actor; new actors receive new logical IDs and revision0.

For Blink, prepare `checked_add(1)` after ordinary eligibility/destination validation and before **any** payment, cooldown/channel mutation, position write or queued-request commit. Retain it in the existing local prepared effect state; commit it beside destination/velocity/waypoint updates. For unload, prepare it before clearing the carrier link or changing pose/orders. Exhaustion returns a typed invariant through existing movement/transport diagnostics, with the entity transaction unchanged. Do not wrap, saturate, panic or silently report ordinary refusal. Preserve Plan50 queued-malformation versus ordinary-ineligibility consumption semantics. Multiple successful operations, including direct world calls in one tick, produce distinct revisions; if an earlier operation succeeded before a later failure, do not pretend the entire tick rolled back.

No new counter map, global counter, event vector or catalog property is needed. The effect enum already supplies the capability-driven semantic cause. Keep pose and revision commits in the same prepared transaction. A private helper may centralize revision preparation/commit, but must not create a second unchecked relocation path.

### Checksum and serialized API consequences

The current checksum contract explicitly includes retained entity presentation state and is schema4. This is a retained deterministic field, and its exhaustion participates in future admission, so include it in canonical entity encoding and advance to **schema5**. Fix its documented encoding location adjacent to position, encode the `u64` little-endian with existing primitives, and update exact schema-byte/mutation tests. State this migration in the checksum docs. Do not exclude the field as though it were a drained visual event, or claim old/new digests should match.

The current snapshot serde format has no general versioned save/replay envelope. Do not invent one in this round. Require the new field on serialized bubble records; a missing legacy field is an explicit decode failure, not an implicit0 that could permit bridging. This is a deliberate Rust struct/serialized-output schema migration. Update all repository constructors and serialization tests mechanically. Existing real preview/Swarm/standalone constructors can specify0 because they construct a new known initial incarnation; this is not an unknown legacy-data fallback. The inspected constructor inventory is in sim_types, sim_core and client, with no map-editor constructor requiring development. Re-audit at implementation time and escalate any unexpected ownership conflict instead of editing the editor.

Preserve existing baseline source, catalogs, input commands, snapshots/checksums, display traces and binary identities before the migration. Re-run the same inputs and compare gameplay fields after explicitly removing only the added revision/digest fields; new schema hashes must not be used to hide a gameplay change. There is no promise of an unimplemented savecodec migration.

## Timeline and discrete-event timing

Add equality of `motion_revision` to the existing pose compatibility predicate. For incompatible revisions and `alpha < 1`, retain the previous **entire** displayed record, including its old revision. Do not snap to the newer pose merely because the current authoritative snapshot already contains a break. At `alpha >= 1`, initialization or `hold_exact`, publish current exactly as today. When both snapshots share a revision, preserve the existing interpolation mathematics and discrete-field policy.

Ordinary clock consumption often never calls `present(1)`. Example: snapshotT contains a Blink, while previous isT−1 and fraction0.4. Display remains at the previous pose/revision; T's events remain pending. After the next tick, T is previous and T+1 is current. Display now enters T's new continuity domain, optionally interpolating only T→T+1 ordinary motion, and pending events with tick≤T are released. This is the existing one-tick-lag contract, not a new extra delay. The first displayed post-break pose need not equal the exact landing point if part of the following continuous segment is already visible. Use the Stop+Cast fixture when an exact isolated landing image is required.

For several catch-up ticks, only the final adjacent pair is displayed. A break in an earlier consumed tick persists in both final revisions; a consumer previously displaying revision0 now sees revision1 (or a larger value) and resets once. A break in the newest current snapshot remains undisplayed until its discrete boundary, just like its events. Several undisplayed breaks may coalesce into one visible reset; no intermediate teleport poses or event times are fabricated. If two separate breaks are actually displayed on different frames, reset at both boundaries.

Speed0, pause and inactive-tab suspension use existing `hold_exact`, which promotes current without a simulation tick. Revision and body/event boundary follow that same promotion; reset must work even with a zero Time delta. While inactive, existing combat-event publication remains suspended; this plan does not change its return policy. Reset/new-session initialization already clears timeline buffers and uses a new committed `SessionGeneration`; recycled numeric entity IDs cannot inherit a prior session's visual identity.

No extra scan, sorting or lookup table is required: the existing ordered merge and record clone carry one scalar. Repeated zero-tick `present` calls must not mutate the simulation revision or create repeated notifications.

The existing `MotionProposal::commit` writes individual locomotion fields rather than replacing the whole Entity, and queued casts execute through the same checked ability worker before their order is popped. Keep the revision outside ordinary motion proposals so post-cast continuous motion/collision cannot restore an older copied revision. Preserve the pre-cast `pos_at_tick_start` bookkeeping from Plan50; it is not a continuity identity and must not be repurposed as one.

## Mist history and retained GPU packets

The emitter slot stores the revision last consumed from the **presented** `BubbleInstance`, associated with its existing ECS owner. Initial slot acquisition records the actor's current revision. A mismatch on an existing owner triggers one semantic break before ordinary sample generation, independently of distance and delta. Update the stored revision only after successful preparation/commit. A revision decrease for the same unchanged owner is a typed invalid presentation input, rather than silently accepting a reused identity; a new owner/session is handled by existing acquisition.

Add a checked `EmitterHistory::discontinue_at(position, dt)` operation for this boundary. It validates finite position, nonnegative finite delta and representable resulting clock, then clears the ring/residual and sets previous position to the new displayed position. Advance the history clock exactly once for the frame; retain cumulative actor-path distance so the jump itself adds no traveled phase. It emits no sample, including at dt0, and does not call ordinary `advance` again in the same frame. Subsequent ordinary frames resume sampling from that anchor. Keep full owner-reset behavior separate: replacing an owner still resets ownership-local clock/path as today. Do not repair unrelated producer accumulation in this slice.

For the same semantic break, preflight and increment the existing checked mist slot generation, reset virtual/conveyor anchoring consistently, and leave logical EntityId, owner and committed session identity unchanged. Document this generation as a **history incarnation** used for GPU retirement, including owner replacement and an explicit continuity break. It is client-only and is not the authoritative `u64` revision or part of simulation checksums.

Clearing only CPU samples is insufficient: the last selected/acknowledged packet can still contain old samples. The main ribbon remains hidden when acknowledged generation differs from the new slot generation; extraction carries the new identity. The actual queued draw must never use an older-generation selected packet under that identity. Preserve the current whole-packet selection and validation rules, checked counters and first-fault reporting. Do not mutate retained packet bytes, remap their origins to the landing point, force an upload, grow staging, or clear unrelated actors to disguise a busy uploader. New-generation empty history may remain hidden until later continuous samples are both prepared and genuinely published. The existing alignment hook must continue to reject an erroneously queued generation mismatch.

A dedicated real-device test must force retained-packet selection during a break while another actor remains continuous. Prove the broken actor's old generation is absent from actual queued draws, the other actor retains valid behavior, and the new wake appears only with matching generation after fresh post-break motion. Generation exhaustion is a typed visible failure before partially resetting that slot. Current owner-reset tests are relevant infrastructure but do not substitute for a same-owner discontinuity test.

## Scope, allocation and ownership

Proposed implementation files are Entity/checked ability/unload commits, checksum encoding/schema tests, sim_types snapshot/docs, timeline compatibility/tests, narrow client constructor/sync adaptations if required, and mist history/slot handling with tests. Coordinate world ability hunks and transport/checksum ownership explicitly. No steering arithmetic, clock policy, combat event type, map, assets, shader, packet layout or appearance changes are justified by this mechanism.

The authoritative and snapshot records add one scalar; the fixed slot adds one remembered scalar. There is no per-frame queue, `HashMap`, growing list of breaks, scene-wide reset or per-actor clone. Existing snapshot Vec capacity rules and mist fixed capacity remain. Add structured performance rationale for the persistent-token approach and verify warmed storage capacities in tests. A bounded representation does not justify an unmeasured claim that the whole existing renderer allocates zero bytes.

## Verification stages and exact acceptance

1. **Preserve/reproduce before changes.** Obtain corrected CPU3 Stop+Cast/Stop-only transaction evidence, actual commands and fixed-point positions/energy/cooldown, original tick outputs and display histories at30/60/144Hz, speeds1..4 and both40ms phase offsets. Keep speed0/pause cases explicit. Record both observed distance predicate and actual emitted samples. Retain blink01 as a failed fixture. A rejected cast or stationary empty ring cannot prove teleport behavior.
2. **Simulation/publication tests.** Real generic Ground/Air capability fixtures exercise successful short/threshold/long Blink, zero-distance successful cast, ordinary rejected placement/range/cost/cooldown, malformed input and revision exhaustion. Assert exact entity transaction equality on rejection, revision once per committed operation, repeated writer calls unchanged, and schema5 mutation sensitivity/replay determinism. Test successful/rejected unload, load→unload with no intermediate snapshot, loaded exclusion and carrier-death lifecycle without changing graph/supply rules.
3. **Real timeline timing tests.** Use actual TickBundle outputs, not manual interpolation poses. Assert old pose/revision at every fraction<1 on the discontinuous pair; ordinary continuous pairs remain unchanged. Cover fractions that never reach1, several catch-up ticks with the break first/middle/last, multiple successful breaks, zero-tick frames, pause/hold_exact/resume, tab return and new session with reused numeric IDs. Include a real visual combat event on the boundary or a separately labeled timeline event fixture, and assert body revision and event release obey the same displayed-tick contract. Do not label a manually inserted event as an emitted Blink effect; Blink currently emits no such combat event.
4. **History/production synchronization tests.** Through real Time/timeline/`BubbleInstance`/mist updates, assert exactly one break, no cross-boundary sample, no repeated zero-tick reset, no jump added to path phase, preserved local clock, and ordinary resumed sample growth. Short Blink must reset even below the heuristic. Continuous control movement must retain ordinary behavior. Test owner replacement, removal/fading, disabled/re-enabled wake and dt0 separately. Inspect source sample positions and generations, not only final sample count.
5. **Renderer and screenshots.** After separate approval of the native harness slice, capture actual pre-break wake, first visible post-break state, retained-upload interval and resumed wake using production assets/pipelines/camera and actual command transactions. Require clear before/after identity and no connecting or resurrected old-generation wake. Use an authentic supported Ground actor's own media; Air remains CPU-verifiable if its authentic animated media is unavailable. Do not substitute aliased concept media or claim art acceptance from a shader compilation test.
6. **Integration gates.** Run focused simulation/timeline/history/actual-device tests, then root-coordinated workspace tests, strict Clippy and format checks. Compare preserved ordinary-control traces separately from intentionally changed revision/checksum and teleport presentation. No thresholds, baseline artifacts or CPU3 failure classifications may be rewritten to count the old bridge as accepted.

### Native harness admission decision still required

The current normal `BattleCommandQueue` intentionally rejects `UnitOrder::CastAbility`. A normal-client end-to-end capture therefore cannot quietly inject an ability behind the sole tick owner. Before the native stage, root must choose a separately reviewed narrow queue admission extension for capability-indexed CastAbility (no new UI control, no duplicate range/payment mechanics), or a distinctly labeled real-renderer playback experiment using already verified authoritative outputs. The latter can test snapshot/timeline/history/GPU retirement but cannot claim the normal game's command adapter was exercised. Any generic capability fixture must be explicitly authored/inventoried external catalog data and use the actual loader and real media. No silent mutation of shipped catalog balance or fabricated snapshot trajectory is acceptable.

The simulation/timeline/history architecture is independent of that verification choice. Implementation remains held until the corrected CPU3 evidence, revision/checksum migration, visual reset policy and harness boundary are reviewed. Plan54 field work and Plan53 ordering remain separate; neither is released by this plan.

## Native admission option: normal capability-indexed command queue

Status: complete narrow planning option requested by root, **not implementation
approval**. This option replaces neither the simulation's ability rules nor the
CPU3 evidence. It permits a future real-client capture to submit through the
normal sole tick owner instead of calling an ability or advancing the world from
a capture system. It adds no user-interface ability palette or OS-input claim.

### Existing boundary inventory

- `battle_pipeline/commands.rs::supported` currently excludes `CastAbility`.
  `capable` therefore never admits it. `submit` captures exact recipients and the
  full command with a checked sequence/session identity. `prepare_tick` rechecks
  those same recipients immediately before making the actual `TickBundle`,
  preserving the pending batch on fatal catalog corruption and recording ordinary
  rejections without replacing recipients from later selection.
- `BattleSession::advance` is the sole tick owner: it prepares that bundle, calls
  `SimWorld::step_into`, publishes the snapshot, drains authoritative outcomes and
  diagnostics, and advances the presentation clock. Pending requests survive
  paused, inactive, speed-zero and no-tick frames. A capture must use this path.
- `world.rs::process_commands` dispatches a `CastAbility` to each owned recipient.
  With `queue=true` and an existing order, it appends through typed `issue_order`;
  otherwise it calls the existing prevalidated ability worker immediately.
  Immediate casts do not implicitly clear an existing Move order. The later
  movement phase may therefore add ordinary displacement in the same tick.
- `try_execute_ability_prevalidated` first checks a living, non-cargo,
  non-channeling actor, resolves its catalog and requested ability slot, checks
  cooldown, authored nonnegative energy cost/range, available energy, generic
  cast range and checked payment. Blink additionally requires an exact position,
  valid nonnegative effect range, widened range comparison and the existing
  destination-only full-footprint/layer/occupancy/foundation admission. Its
  proposed revision increment must be prepared here before any payment.
- Only after all fallible Blink preparation does the worker commit energy,
  cooldown, position, zero velocity, cleared funnel and the proposed revision.
  A failed placement/range/energy/cooldown attempt makes no ability payment or
  relocation. Corrupt numeric/catalog/transport state follows typed diagnostics;
  the client must not downgrade it to a harmless UI rejection.
- Deferred casts execute in `movement/locomotion.rs` when their existing motion
  and weapon/channel guards permit. An ordinary `Ok(false)` is consumed under
  Plan50's established policy; a malformed `Err` keeps the queued request and
  reports an invariant. This option does not add indefinite retry, atomic group
  casting or a new channel/interrupt policy.

The current simulation has no authoritative `AbilityOutcome` receipt. Its direct
public wrapper returns `bool` and reports corruption through diagnostics; command
dispatch does not expose per-recipient ordinary cast refusal. Existing client
Movement/Production notices must not be repurposed to fabricate ability success.
A submitted queue status means submitted, not cast. A general ability receipt API
would require a separate scoped decision, especially for deferred requests whose
execution occurs after their originating input tick.

The worker currently ignores `target_entity` and does not uniformly enforce
`AbilityTargetKind` metadata for all effects. Some non-Blink effects have separate
existing completeness concerns (for example hazard-ID wrapping and missing mode
lookup behavior). This option does not certify or repair them. Native acceptance
selects an authored Blink effect and exact position, and never relies on a
currently unsupported entity-targeting mechanic. If root requires complete new UI
exposure for all active effects, that is broader than this bounded queue option.

### Proposed queue changes and explicit policies

1. Admit `UnitOrder::CastAbility` in the ordinary explicit-recipient `submit` path.
   In `capable`, require only that `archetype.abilities.get(ability_index)` exists.
   Preserve current common owner/alive/completed/non-cargo checks and fatal
   missing-catalog handling at both submission and tick preparation. The index is
   local to each recipient's actual catalog definition; no global creature or
   ability-name registry, slug switch or identity constant is introduced.
2. Forward the exact index, `target_pos`, `target_entity` and queue flag unchanged.
   Do not duplicate range, energy, cooldown, channel, terrain, target semantics,
   payment or effect-specific checks in the client. Their state may change between
   admission and dispatch, or between two commands in the same real tick. A valid
   slot is client capability admission, not authoritative cast eligibility.
3. Client admission/revalidation remains atomic for the explicit recipient list:
   a missing slot on one selected recipient rejects the command before submission
   or bundle insertion. Once dispatched, preserve the existing simulation's
   per-recipient application, command order, duplicate-recipient behavior and
   ordinary refusal semantics. Do not claim an all-or-none paid group transaction.
4. Keep `submit_roster_order` unavailable for `CastAbility`. A numerical slot can
   mean different effects on different archetypes, so the old all-owned roster
   convenience must not silently cast unrelated capabilities. Return the existing
   typed `UnsupportedOrder` for that convenience path, document the distinction,
   and test it. Explicit multi-selection remains intentional: callers specify the
   actual recipients and slot. The native fixture selects exactly one actor.
5. No new persistent per-frame list, success flag or side-channel simulation
   command is needed. Existing action-time allocation and reusable bundle storage
   rules remain. Tests should check retained capacities through repeated queued
   submissions; do not claim the entire client is allocation-free.

Files are narrowly `battle_pipeline/commands.rs` plus its focused tests and API
docs. Existing `Capability(EntityId)` and status formatting already express a
missing slot; add an error variant only if a separately reviewed user-visible
semantic requires it. The sole tick-owner algorithm and authoritative payment
worker stay unchanged by the queue extension itself. Plan55's revision change
still owns the already identified simulation/checksum/timeline/history hunks.

### Actual native harness route and evidence

Use an explicit strict capture profile, mutually exclusive with incompatible
transport/ownership diagnostic profiles. Root reviews its exact authored actions,
checkpoints and budgets before implementation. The capture adapter submits via
`BattleSession.commands.submit` using the actual `SimWorld` and records session
identity, returned submission sequence, recipient, index and full order. It never
calls `execute_ability`, `step_into`, changes entity state, pushes a fabricated
snapshot, or mutates the command queue's private bundle.

Load an independently preserved external scenario/catalog fixture through the
production loader. Select an actual mobile actor by a supported authored Blink
capability and authentic own-file media; the fixture inventory states any generic
capability addition explicitly. Do not mutate shipped balance or repurpose aliased
concept media. Record the exact selected catalog definition and index, not just a
label or an ordinal assumed stable after catalog changes.

First use ordinary queued Move to establish a visible old wake. At the declared
input boundary submit **Stop then nonqueued CastAbility**, in that FIFO order,
with one recipient and the exact admitted endpoint. The Stop prevents an old Move
from adding displacement after the cast; its independent receipt is not evidence
that Blink succeeded. Advance only through the normal driver. A later separately
submitted Move produces the new wake. The control run uses the identical Stop and
later Move schedule but omits the cast. Each successful/rejected cast checkpoint
records actual input/session sequence evidence, authoritative before/after
position/energy/cooldown/revision, tick and direct/published checksums, diagnostics,
current/previous/presented identity and pose, and actual mist history/slot/packet
identity. Account for normal same-tick cooldown decrement and regeneration from
the real fixture instead of assuming the immediate worker's values survive the
whole tick. Inspect exact transactions, not queue text or disappearance of pending
intent, to prove application.

Because the current driver retains only its last input bundle within a multi-tick
advance, a narrowly scoped borrowed observation hook or capture-only recorder may
be proposed if exact executed-bundle/sequence evidence cannot be obtained from
existing read-only interfaces. Such a hook must run immediately after each actual
step, copy only into capture-owned bounded evidence and never alter execution.
It requires explicit review; exposing the last bundle as if it represented all
catch-up ticks is not acceptable. CPU queue/driver tests already have access to
owned internals and need no production mutation for their proof.

Use the real presentation boundary, not `current.tick == cast_tick` alone, for
first-visible-post-break capture. Preserve one-tick lag and event release; fractions
need never reach one. The adapter must not force `hold_exact` merely to disguise a
bridged ordinary interpolation interval. Test pause/hold_exact separately. Record
fixed camera, actual loaded images, shell/wake visibility, all exact required
pipeline readiness and readback/fingerprint evidence. Prove the old wake is absent
at the first displayed revision, no old generation is queued under a retained
packet, and new continuous motion appears only after matching publication. Keep
all failed images/manifests and forbid automatic camera/endpoint substitutions.

The real-device retained-packet test in the main blueprint is still required:
a natural native run that never encounters uploader retention cannot claim that
case. Native screenshots verify the declared visible states, not invisible GPU
history or an input adapter they did not exercise. No physical-key/mouse control
is added or certified by this capture route.

### Required focused tests and decision boundary

- Explicit valid slot submits unchanged; missing slot rejects with exact recipient
  and unchanged sequence/pending intent. Roster convenience refuses casts.
- Missing/foreign/dead/incomplete/cargo recipients and missing catalog retain the
  existing ordinary/fatal distinctions. Replacement generation rejects stale
  commands. Test capability removal between submission and preparation.
- Zero-tick, speed-zero, paused and inactive frames preserve pending casts. Exactly
  one normal tick dispatches the retained FIFO Stop+Cast; repeated presentation
  frames do not cast again. Catch-up ticks preserve command order and record the
  correct executed input, not the final unrelated bundle.
- Real generic Ground/Air Blink through `BattleSession::advance` checks exact
  destination, cost, cooldown and revision with the Stop-only control. Exercise
  out-of-range, blocked, outside-map, insufficient-energy, cooldown and channel
  refusal; submission itself may succeed, authoritative state proves refusal.
- Two same-tick requests whose eligibility changes after the first are decided by
  the worker, with no second invalid payment. A queued cast behind Move follows
  existing deferred ordinary-consumption/malformed-retention behavior; no new
  retry is inferred. Missing archetype or malformed rate/cost reports fatal
  diagnostics through the actual driver without partially applying that cast.
- Per-recipient mixed authoritative eligibility remains the existing partial
  behavior and is labeled explicitly. Client missing-slot group rejection remains
  before dispatch. No ability result is inserted into movement/production notices.
- Run the plan's timeline/history/retained-GPU and actual image checks after this
  queue extension is accepted, then the parent-coordinated complete quality gate.

Root can choose this option for normal-client end-to-end evidence or retain the
separately labeled renderer-playback alternative. This appendix proposes the
former without claiming that it is approved, that CPU3's full matrix has finished,
or that the discontinuity/field representation has passed admission.

## Selected native option: verified trace playback in the production renderer

Status: root selected this evidence boundary over the generic queue option above.
**The normal command queue remains unchanged.** The preceding queue appendix is a
retained, unselected alternative, not an approved feature. Core55 implementation
and this capture adapter remain held pending the final CPU3 corpus, baseline/gate
checkpoint and explicit review of this complete blueprint.

This harness proves simulation-output → production timeline → production scene
synchronization → actual mist history/packet/render behavior. Its command source
is a declared replay, not the normal client command adapter or OS input. All
manifests, console summaries and reports must name that boundary. It cannot certify
a complete ability-command UI, generic ability target handling or SC2 parity.

### Genuine trace and exact runtime provenance

Add a strict external playback specification, rejecting unknown fields and
incompatible profiles. It names the external scenario/catalog root, seed, fixed
viewport/camera, authored actor selection by capability, command schedule,
presentation rates/speeds/phase, checkpoints, frame/tick/wall deadlines and bounded
evidence sizes. Do not hardcode unit slugs, synthesize art or alter shipped data.
An explicitly authored generic Blink capability fixture may reuse an authentic
actor's own media without changing its identity; inventory that modified catalog
and its derivation. An unavailable authentic animated Air asset is a declared
native limitation, not permission to substitute the known concept-copy atlas.

Load the scenario, catalogs, map and player/presentation data through the same
`RuntimeBattleDefinition`/normal startup loaders and instantiate the same actual
world. Before graphics measurement, generate the complete bounded trace from that
world with actual `TickBundle`s and `SimWorld::step_into`. Retain:

- Exact initial snapshot and separately computed direct authoritative checksum.
- Every original command bundle (tick, owner, recipients, index, targets, queue
  flag), complete immutable output snapshot/events, direct checksum, movement and
  production outcomes and diagnostics. The producer never directly calls an
  ability or writes a pose. Stop then Cast is an actual ordered bundle.
- Actual selected definition/capability, full transaction state around the cast,
  map/player/resource setup and a matched Stop-only no-cast control trace.
- Byte-identical input copies plus hashes, source/compiler/binary preservation,
  schema versions and limits. Verify input hashes before loading, after trace
  generation and after capture; mutable files do not certify different decoded
  contents. The loaded same-buffer parsing guarantees of the existing loaders
  remain required.

Reject any unexpected producer diagnostic, command rejection, mismatch between
published/direct checksums, missing tick or failed transaction assertion. The
complete trace still remains evidence on failure. Neither a user-authored list of
positions nor manually fabricated snapshots qualify. Sharing a reviewed trace
format/producer helper with CPU3 is acceptable only without changing its existing
frozen baseline, policy or corpus behavior; do not copy the simulation algorithm
into the client or create a second renderer.

### One explicit playback owner and unchanged production presentation

Use the shared production App builder and `ClientAppPlugin`, real assets/materials,
map/camera/UI systems, real `BattleFrameState`, synchronization and mist plugins.
Add a capture-only playback resource/plugin and an explicit exclusive scheduling
choice for `BattleSet::AdvanceTicks`: normal `advance_battle_ticks` runs when the
playback resource is absent; the replay system runs only when it is present. No
ordinary client default, command-queue admission or game behavior changes. Tests
prove the run conditions are mutually exclusive and one authoritative advance
occurs per scheduled tick. Do not remove systems at runtime or install alternate
shaders/meshes to make the capture pass.

The replay's authoritative mirror is the actual loaded session world, reset from
the same loader/definition after graphics readiness and clock reset. It keeps
normal presentation metadata/map/UI reads coherent. At each scheduled playback
tick, this single owner executes the next original bundle into reusable scratch,
then compares the entire resulting snapshot, direct checksum, outcomes and
diagnostics with the immutable precomputed record. Only after exact agreement may
it publish the saved output through `BattleFrameState::begin_tick`, retained-buffer
`clone_from` and `finish_tick`. A mismatch is fatal before publishing the mismatched
output; preserve the record and actual comparison evidence. This additional mirror
execution is harness verification overhead, not production performance evidence.
It prevents a final-world object or stale UI from being paired with an earlier
saved presentation without disclosure.

Use the production `BattleTickClock` observation/consumption policy, actual
`TimePlugin` and explicit `ManualDuration` schedule, including nanosecond remainder
and phase handling. For each ordinary display frame, consume all scheduled tick
records in order and call the real `present(clock.fraction())` exactly once. Never
interpolate in the harness, edit body transforms, reset histories, force revisions,
or generate a fake event. Production event publication and scene sync consume the
real timeline. Verify actual media admission for the current output, fatal status
handling and session-generation lifecycle before publication. The mirror may
mutate before a detected mismatch; that run stops and is not falsely rolled back.

Initialization/reset must compare the actual loaded session's initial output and
direct checksum before playback begins. Graphics warmup may not consume the
measured trace. Reuse the existing capture readiness/reset protocol: exact required
pipelines/material/images and visible entities, then freshly instantiate the real
session and reset generic/virtual time, clock and trace cursor once. Track the
reset generation independently of motion revision. If a scene/session replacement
occurs unexpectedly, stop instead of resuming at an arbitrary cursor. Explicit
reset/tab lifecycle test cases use separately declared state transitions and
restart/continue semantics; they do not silently alter the primary cast trace.

### Checkpoints at the real discrete boundary

The default discontinuity run uses one mobile Ground actor with authentic media,
ordinary Move to produce a substantial old wake, then **Stop + nonqueued Cast** at
input tick T, an interval with no following movement, and a later ordinary Move
from the landing endpoint. The no-cast control has identical Stop and later Move
bundles. Both use the actual caster capability and exact accepted endpoint. Reject
an obstructed/hidden endpoint or ambiguous framing; do not silently select another
actor, camera or destination. The first native fixture's exact numerical schedule
must be declared and preserved before running.

Required captures and machine-readable observations are:

1. **Pre-cast wake:** a frame before T with a clearly visible old wake, actor and
   landing region inside the fixed actual gameplay viewport.
2. **Current cast, old displayed boundary:** current snapshot T, previous T−1 and
   an ordinary fraction strictly below one. The body and revision still belong to
   the previous continuity domain. Record that no interpolation bridge appears;
   do not treat the changed current snapshot as an already displayed teleport.
3. **First displayed new revision:** after the normal next tick makes T the
   previous boundary, at a normal fraction below one. With Stop and a stationary
   post-cast interval, the actual body is at the exact landing endpoint. The old
   wake/history generation must be absent, and new samples must not connect the
   old and new domains. Do not require `present(1)` to reach this state.
4. **Resumed continuous wake:** later actual Move creates new samples anchored in
   the new domain with matching slot/packet generation, while the old wake remains
   absent. The matched control continues to retain the appropriate ordinary
   history instead of being cleared by the harness.

Record every intervening display frame's current/previous/presented ticks,
fraction, actual elapsed/delta, body pose/revision, mist history clock/sample
provenance, slot generation/visibility and selected packet identity. Bound this
capture-only evidence explicitly; exact old/new sample births distinguish a real
reset from an empty or hidden fixture. Add multi-tick catch-up cases where the
cast is first/middle/last among consumed outputs, two successful casts between
frames, zero-tick frames and speed-zero/paused/tab-resume/reset as CPU integration
coverage; only separately declared native cases claim those visual conditions.
No new catch-up, event-aging or pause policy is introduced.

For screenshot Readback, freeze generic/virtual delta and the replay cursor while
preserving the **current interpolation fraction and published pose**. Do not use
ordinary `suspend`/`hold_exact`/pause as a substitute: those may deliberately promote
the current boundary and invalidate the sub-one timing checkpoint. The replay
owner's explicit readback-freeze path leaves the timeline unchanged while normal
scene/material sync and real renderer continue. Verify fingerprints at request,
callback and resumed frame, including body revision, clock/fraction, history and
slot/packet identity. The production mist system may reobserve the same displayed
revision at zero delta; it must not clear twice or age the history. Wait for actual
readback while retaining ordinary walltime deadlines. This capture pause is labeled
instrumentation and is distinct from the game's pause lifecycle tests.

### Retained GPU packet proof remains independent

A successful natural screenshot sequence does not prove the uploader's busy path.
Keep the blueprint's real-device forced-retention test separate: use the actual
production packet selection and generation guards, with a second continuously
moving actor. Establish an old acknowledged packet, introduce the actual displayed
motion revision, and exercise a documented existing retained-upload condition.
Assert old-generation draws are absent for the broken actor, unrelated actor data
remains valid, and post-break data appears only after matching-generation fresh
publication. Do not rewrite a retained packet, mark a fake acknowledgement, expand
staging limits or force an upload. The test must drain/join the renderer even on
failure. If a native run does observe genuine retention, retain that evidence,
but it cannot replace the controlled device regression or fabricate a busy case.

### Bounded implementation and tests

Suggested capture-only files: `runtime_capture/playback/{config,trace,driver,
evidence}.rs` and tests, or an equally small module split following actual size.
The shared App scheduling edit is limited to the explicit mutually exclusive tick
owner choice. Keep the default seven-goal and existing transport/ownership profiles
unchanged, and reject mixed modes. New saved-trace serialization requires an
explicit harness format version; it is not an invented game save codec. Reuse
current no-overwrite artifact and orderly failure/shutdown mechanisms.

Preallocate/reuse snapshot and outcome scratch from bounded trace maxima. Saved
records are immutable; `finish_tick` may drain its working-copy events but never
mutate the saved trace. No per-frame cloning of the entire world, growing history
maps or unbounded capture log is allowed. Capture-owned bounded record/serialization
allocation remains disclosed overhead. Ordinary timeline/history and selected
GPU-packet capacities keep their existing contracts.

Required checks before a native run:

- Actual-loader trace generation and an independent replay produce exact initial
  and per-tick commands, snapshots/events, outcomes and direct checksums.
- A mutated saved pose, revision, checksum, command, event, missing/duplicate tick
  or changed input file fails before accepting that publication. Preserve both
  expected and actual evidence; no repair/substitution.
- Exactly one tick owner is active; no normal queue item is executed in replay
  mode, and ordinary mode remains unchanged. Wrong session/generation, deadline,
  exhausted trace or evidence budget is a typed fatal capture error.
- Real timeline frame records prove all four boundary checkpoints with fractions
  below one, normal event-release timing, no double history reset and exact
  Stop+Cast versus Stop-only transaction assertions. Readback zero-delta frames
  cannot promote a boundary or age a history. Resume matches an uninterrupted
  same-schedule control once the explicit zero-delta capture frames are removed.
- Actual scene synchronization consumes saved-equal output without manual pose or
  history writes. Missing required media, failed pipeline, absent visible actor,
  invalid endpoint/framing or ambiguous selection fails with retained artifacts.
- Run the independent retained-GPU regression, preserve source/binary/asset/script
  inventories, request root's native interval and inspect every actual PNG against
  the declared goals. No screenshot or artifact here certifies a normal ability
  input adapter, broader effects, Air art availability or field-model fidelity.

Keep CPU3's original bridge, capacity and support-cut refusals unchanged. This
harness can eventually verify a narrowly approved discontinuity correction; it
cannot retroactively make the original bounded reducer or proposed mist field
admitted. Parent approval of the final core/harness blueprint and coordinated
quality gates remain prerequisites for implementation and acceptance.

### Media prerequisite correction and initial concrete fixture

This section closes the media and bounded-fixture questions from root review and
supersedes any preceding wording that could imply an existing Ground actor already
has authenticated art. The approved39 migration admits structurally valid own-file
legacy bundles; it does **not** authenticate them. The preserved strict audit
`/tmp/space-scuffle-r5-current-media-audit.json` records91 identities:81 MissingAsset,
6 KnownConceptDuplicate and4 LegacyUnverified, with `strict_accepted=false`.
[Round39's report](../reviews/round5-media-integrity-verification.md) explicitly
states that no legacy art is approved. Named videos and keyframes are not proof of
source-to-installed-atlas provenance or action correctness. A read-only current
asset-path inventory found no source-baking provenance declaration that closes
that gap. Thus **no authenticated animated Ground candidate is currently
established**. Absence of such evidence is not a new assertion that all source
videos are fake or all media must be replaced.

An explicitly labeled **current-media diagnostic** may use an own-file
LegacyUnverified Ground bundle to inspect history/packet behavior, subject to root
approval. Its manifest records that exact status and `authenticated_art=false`;
its screenshots cannot close the authenticated visual/art gate. Concept duplicates
are excluded from this diagnostic fixture rather than renamed. Final authenticated
visual acceptance remains held until real declared source-to-output provenance,
required-action correctness and actual art review are supplied. This plan neither
generates media nor silently substitutes a video/keyframe.

The initial external data fixture is now specified exactly:

- Author a separate32×32 flat level1 map document with every cell normally
  pathable, no ramps/blockers/resources/watchtowers, and two distinct start records: player0 at cell(4,4), player1 at
  cell(24,24), each footprint(5,5), IDs1 and2. Only player0 is assigned in the
  scenario; the real loader leaves the other start unoccupied. This satisfies
  the existing minimum-two-start and16-cell-separation validation without
  spawning a second actor. Use the real map-document parser/validator; warnings
  about a deliberately noncompetitive diagnostic map are recorded. A validation
  error fails setup. No editor changes or alternate map implementation.
- Copy the external catalog set and player palette into an inventoried diagnostic
  asset root. Its single-player loadout names the existing own-file legacy town
  hall and exactly one Ground mobile actor. The concrete first data selection is
  `foundry_core_dome` plus `pulse_bot`; these are authored RON values, never Rust
  branches/constants. Both retain LegacyUnverified status. The native harness
  resolves the unique living completed player0 Ground mobile actor with the
  fixture's Blink capability, not a numeric EntityId or assumed spawn order.
- In that external actor record only, set max energy100, energy regeneration0 and
  a single explicit active ability: target kind TargetPosition, energy cost10,
  outer cooldown50ticks, cast range8world units, effect
  `BlinkTeleport(max_range:8,cooldown_ticks:50)`. Preserve existing radius0.375,
  movement/turn/weapon settings and identity/media; do not mutate the loaded
  runtime catalog. Record this generic capability fixture's exact differences
  from the original catalog. It is test content, not shipped balance.
- One player, no initial autonomous attack orders, seed1592639215. The real startup
  loadout places the foundation and actor. No other actor, resource or AI moves;
  if the loader produces unexpected participants, fail the declared fixture.
  Do not manually move/remove the town hall or alter supply/cargo state.
- Full trace has exactly480 input ticks0..479 plus the actual initial snapshot0.
  Tick0 submits Move to(12.5,12.5). Tick256 submits Stop; require the actor already
  settled within its ordinary arrival contract near that location and with no
  active route by the end of the idle setup. Ticks257..320 have no commands.
  This64-tick idle interval is2.56s, longer than the retained1.35s mist lifetime;
  it establishes a stationary, empty-history measurement baseline without a
  harness history clear. The first displayed measurement state is output321.
- Tick321 submits Move to(20.5,12.5). Tick371 submits Stop followed by the exact
  nonqueued Blink to(19.5,12.5), target_entity=None. Its resulting output is372:
  keep input tick and published tick distinct. Ticks372..391 are command-free.
  Tick392 submits Move to(24.5,12.5). Tick448 submits Stop. Remaining ticks through
 479 are command-free. The control trace omits only the tick371 Cast command.
  Declared per-tick maxima are two commands and one recipient per command.
- At generation time require tick371's actual pre-cast point to be distinct from
  the endpoint, within both exact ranges and in the ordinary Ground policy;
  require the endpoint's full footprint and same-layer occupancy to pass the real
  destination admission. Require the actual authoritative cast distance and actual
  first-visible pre/post body displacement both to be strictly less than the
  actual rendered radius times loaded `continuity_radii`. With the proposed
  radius0.375 and continuity8, this threshold is3world units; the candidate
  landing19.5 is deliberately distinct from the original Move goal20.5. The
  nominal prior50ticks at authored0.09 speed suggest roughly2.5world units of
  relocation, but this is an unverified expectation, not an admitted trace.
  Arrival, turning and fixed-point motion determine the real pre-cast position.
  Require successful exact position/energy/cooldown and
  proposed revision change, no unexpected diagnostics, and unchanged no-cast
  control position. No automatic relocation, radius change, command rescheduling
  or replacement actor is allowed if the expected path/transaction fails.

These authored inputs give concrete coordinates and a bounded legal candidate;
planning alone does not claim that its simulation, camera or art has passed. Run
the actual loader/trace verification after approval. Any failed assumption is
retained and reported before a reviewed fixture revision, not repaired invisibly.
The publisher stores every actual fixed-point position rather than replacing it
with these requested destinations. The pre-cast position is authoritative trace
output, never a hand-computed approximation of accumulated movement.

The primary presentation schedule is144Hz, speed1, production40,000,000ns ticks.
After zero-time initialization, seed the first nonzero duration with40,000,000−1ns
and then use the existing integer-remainder144Hz cadence. Preserve that seed and
each requested/actual delta. Independent30Hz and60Hz controls belong to CPU tests,
not an undeclared extra native matrix. **Run the entire prelude through actual
production timeline, scene synchronization and mist systems from initial output0.**
No history reset or timeline reinitialization at321 is permitted. The continuous
control must prove the idle history is empty at321 while retaining its genuine
cumulative path/clock; empty history does not imply path coordinate zero.

The prelude has an explicit camera target(12.5,12.5), distance16, default pitch;
its early offscreen actor is allowed only before the screenshot measurement phase.
At output321 change once to the declared measurement target below and freeze
cursor/time while actual camera propagation, viewport and exact pipelines settle.
The actor has moved visibly before the idle interval, so the real mist pipeline
must have been exercised; do not accept readiness based on an unrelated pipeline.
A bounded zero-delta wait at321 may complete asynchronous graphics work without
aging or clearing history. Resume the same clock/fraction and trace after readiness.
There is no post-warmup world reset in this profile. This explicit deterministic
prelude protocol supersedes the earlier general fresh-world-after-readiness wording
for this fixture: the entire measured state derives from the same real commanded
world and fixed elapsed-time schedule. Initial missing required media or pipeline
failure still terminates rather than being hidden by the prelude.

All four screenshots use physical1920×1080, the actual production gameplay viewport,
fixed RTS target(18.5,12.5), distance16 and default58° pitch. No yaw, auto-fit,
per-checkpoint camera change or hidden panel resize. Before arming, require the
actual projected full shell and the declared old-wake/landing/new-wake regions to
be inside the gameplay viewport with32physical-pixel margin; require projected
shell diameter at least24pixels. Also require the actual pre/post displayed shell
centers to differ by at least24physical pixels under that same fixed camera while
both centers remain visible. Retain the raw world and physical-pixel deltas and
actual threshold. A tiny invisible jump cannot satisfy the diagnostic; a jump at
or above the old heuristic threshold cannot establish semantic-reset evidence.
These are diagnostic legibility requirements, not an art score. Use actual camera matrices and propagated bounds, validating
nonfinite/out-of-frustum cases; fail instead of widening the view automatically.
The town hall may be outside the viewport but its loaded state remains in source
and checksum evidence.

Checkpoint selection is fixed by tick/fraction rather than first flattering image:
pre-cast uses the first ordinary frame with current output370; the old displayed
boundary uses the first frame with current372 and previous371; first new revision
uses the first frame with current373 and previous372; resumed wake uses the first
frame with current420. Require `0 < fraction < 1` for all four and the precise
revision/membership/history predicates from the earlier section. A missing state
under the declared cadence fails; do not select a later substitute. Pausing for
readback preserves that exact fraction/time/history. The fixed control replay uses
the same four states and schedule, retaining its own clearly labeled images.

### Concrete storage, time and publication limits

The initial diagnostic profile has the following checked admission caps. These
are harness safety bounds, not changes to the producer's48-sample ring, proposed
17 controls, source-support tolerance or GPU upload policy:

| Quantity | Hard bound and refusal behavior |
| --- | --- |
| Original simulation trace | 480 input bundles,481 snapshots including initial; exact count required |
| Participants | At most8 entities in any output; expected fixture is the one actor and town hall |
| Commands | At most2 per tick and1 recipient per command; reject excess before generation |
| Snapshot events | At most32 per tick; primary fixture expects zero combat events |
| Actual ticks in one display | Production clock maximum remains unchanged; cap at its admitted policy, never override to drain faster |
| Measured display frames | At most2048 after output321 per native replay, including zero-delta readback/settle frames; attempted and completed counts separate |
| Prelude display frames | At most2048 through output321; all actual timeline/history states retained; no screenshot acceptance claimed |
| Readiness wait at321 | At most1024 additional zero-delta frames and60wall-clock seconds; cursor/fraction unchanged |
| Screenshot drain | At most256 frames and20wall-clock seconds per checkpoint; exact requested frame stays frozen |
| Entire native process |180wall-clock seconds, checked independently of simulation/virtual time |
| Trace encoding |64MiB exact serialized trace cap, checked before accepting/publishing it |
| Per-frame evidence |256KiB exact encoded record cap; at most48 actual history samples for each of the at most8 actors |
| Display evidence total |128MiB exact UTF-8/byte total, including prelude, readiness, measured frame records and metadata |
| Screenshot bytes | Four PNGs per run,16MiB each and64MiB aggregate; actual image dimensions exact |
| Captured media/input copies |512MiB total; existing root is roughly261MiB, no external symlink traversal |
| Other metadata/source manifest |16MiB aggregate, all paths/counts/hashes retained |

A paired cast/control invocation comprises two separately named outputs with these
per-run limits, or is refused before launch if its combined declared retention
would exceed the configured destination budget. Preflight enough free space for
both; never remove earlier failed evidence to make room. Limits are checked with
nonwrapping arithmetic before allocation/copy/publication. Count complete original
serialized bytes, not just component estimates. On exhaustion preserve the already
closed trace/records/PNGs and a typed incomplete manifest; never truncate a record
and label the run complete. A partially written current file remains explicitly
partial and is not listed as verified. New-file-only publication prevents replacing
baselines. No automatic cap increases or retry-until-green behavior.

Exact current own-file media hashes recorded during this review for the selected
Ground diagnostic actor are PNG SHA256
`187f632cd16f95c5d020a7d6691f8b995be16c7c6a006c5c1e57534628126447` and flipbook metadata
SHA256 `f113927d184d03f4992810092f61694d1c0b63eccfd60927f6bde39a17f6b7a0`. They prove
content identity only. Native setup must inventory all actually loaded media,
shaders/catalogs/maps and real source status again; these two hashes are not an
authenticity certificate or a complete bundle inventory. Final authenticated media
and art acceptance stays held even if this current-media diagnostic succeeds.


### Root-reviewed short-jump fixture correction

The original proposed landing20.5 could leave a roughly3-unit jump near the old
8-radius heuristic threshold. The corrected exact landing is19.5,12.5; the Move
before it still targets20.5,12.5 and resumed Move still targets24.5,12.5. Require
both actual authoritative and first-visible displacement strictly below the
loaded heuristic threshold, plus the fixed-camera24-pixel visibility guard above.
This ensures that the old distance-only reset cannot accidentally satisfy the
intended short-discontinuity test. The complete cast/control traces and their
numerical measurements determine admission; no automatic rescheduling, endpoint
substitution, zoom adjustment or tolerance change is permitted. This is a planning
correction only; implementation remains held pending full baseline/gate and final
approval. No native fixture has yet been executed under this specification.
