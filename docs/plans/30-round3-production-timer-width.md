# Round 3: Lossless authored production durations

Status: approved by the orchestrator; implementation stage authorized.

## Goal and demonstrated defect

Preserve the full authored `u32` training duration from catalog decoding through command scheduling, per-tick countdown, completion, and canonical state encoding. An actual external unit catalog edited to `build_time_ticks: 65536` loads correctly today but `process_commands` queues `Some(0)`: `Entity.production_queue` stores `u16` and the command handler narrows with `as u16`. Thus a valid authored duration completes on the first production update instead of after 65,536 updates. The independent reproducer is `/tmp/space-scuffle-catalog-duration-review.rs`.

This is a representation and state-transition correction, not a balance change. It does not introduce creature identifiers or unit-specific exceptions. All `u32` durations, including zero and `u32::MAX`, must remain representable without truncation, overflow, floating-point conversion, or an arbitrary upper cap.

## Ownership and limits

Own the `production_queue` field and rustdoc in `crates/sim_core/src/entity.rs`, its enqueue expression and production-phase documentation in `world.rs`, checksum schema/version tests, and a new focused public-API integration test file. Preserve all previous checksum/catalog changes and other agents' world edits. No editor or visual changes.

The existing structure placement API and `Build` command create completed buildings immediately; they have no construction countdown. `spawn_structure` is also the setup API used by scenarios and placement tests. Do not silently reinterpret that setup API or claim this round implements timed construction. The existing `research_upgrade` API likewise applies completed research immediately; it does not consume `research_ticks`. These are separate missing gameplay lifecycles, not `u32` narrowing in an existing queue. If the orchestrator requires timed construction in this round, obtain approval of an expanded design that specifies builder capability, placement/payment atomicity, progress, interruption, completed-building capabilities, and setup compatibility before implementation.

Existing production authorization/payment, placement of trained units, ignored spawn errors after legacy catalog mutation, and cancellation/refunds remain distinct issues. Do not bundle ad hoc fixes for those mechanics into a timer representation change. Tests must use a valid owned producer and a loaded, stable catalog so their results isolate the timer contract.

## Representation and compatibility

Change `Entity.production_queue` to `VecDeque<(UnitTypeId, u32)>`. Keep the tuple layout and FIFO container; a new job abstraction is unnecessary for a single timer-width correction and would impose additional public API migration. Document that the second element counts remaining 25 Hz production update passes, not milliseconds or an absolute world tick. Queue the exact `arch.build_time_ticks` value without any cast.

This is a deliberate Rust source-level API change for callers that explicitly construct `(UnitTypeId, u16)` tuples. Integer literals continue to infer correctly; explicitly typed callers convert with lossless `u32::from`. Preserve public function signatures and catalog RON schemas. There is no serialized production-queue format to migrate in this repository; inspect callers and derives again before implementation to verify this remains true.

Do not narrow the parsed catalog duration, clamp it to 65,535, divide by tick frequency, or introduce a hidden maximum to preserve the old queue type. At 25 Hz, wide durations may be unusual authored choices, but the type already promises their preservation.

## Tick transition contract

Preserve the current sequencing: `SimWorld::step` processes commands, then updates production in the same tick. A newly queued positive duration N completes on its Nth production update, counting the update in the enqueue tick. Direct `process_commands` only queues; it must not advance a timer or spawn a result.

At each production update, decrement only the front job with `saturating_sub(1)`. If its remaining count is zero afterward, remove exactly that job and schedule one completed unit spawn. Later queued jobs do not consume a tick in that update. A zero-duration job completes on its first production update, matching existing behavior; multiple zero-duration jobs still consume one queue slot per update. Completion produces a normal completed entity through the existing spawn API and leaves the producer's completion state unchanged.

No new arithmetic is needed. `u32::saturating_sub(1)` is defined at both zero and `u32::MAX`; no multiplication, signed cast, elapsed-time addition, or wraparound occurs in this countdown. Queue duration is independent of `current_tick` wrapping. Do not advance the whole simulation for billions of ticks to test a boundary.

## Canonical checksum schema

Increment `CHECKSUM_SCHEMA_VERSION` from 1 to 2. The queue's remaining count now encodes as four little-endian bytes instead of two; both the world and historical partial digest domains use the new schema header. The existing generic tuple encoder should correctly select `u32` automatically. Keep collection framing and field order unchanged.

Update the explicit-byte canonical encoder test's schema bytes and add a queue-specific exact byte regression that includes a value above 65,535. Add a world-digest regression comparing otherwise identical queues with remaining counts 1 and 65,537; equal low 16 bits must not hide their distinct future behavior. This is an intentionally incompatible digest version even though schema 1 was introduced during this session. Catalog compatibility remains a separate prerequisite; this change does not add catalog fingerprints.

## Required behavioral tests

Use real loaded catalog definitions selected by available properties, never hardcoded creature names or numeric IDs. Tests can mutate a cloned selected archetype's duration/cost for focused fixtures; at least one regression must edit and decode actual RON data to prove the authored `u32` value reaches the command and queue unchanged.

1. Table-driven exact enqueue and first-update checks for 0, 1, 65,535, 65,536, 65,537, and `u32::MAX`. Assert the parsed duration, direct command enqueue value, first decrement, actual entity count, and no premature spawn for durations above one. Cover an actual `step` carrying the command to prove same-tick decrement ordering.
2. Completion checks after long-duration enqueue/decrement: explicitly set the existing queue's remaining count to 2 as a documented test-only near-completion fixture, then execute two normal `step` calls. The first must retain one remaining tick and no spawn; the second must remove the job and spawn exactly one entity of the requested catalog type and owner with `is_completed == true`. This verifies real transitions without tens of thousands of unrelated combat ticks. Do not claim that the fixture simulates the omitted interval; enqueue preservation plus decrement algebra plus boundary execution provide the evidence.
3. FIFO semantics with a wide-duration first job and a second job of a distinguishable duration: completing the first must leave the second untouched until the next production update. Include zero-duration jobs to ensure the loop does not accidentally drain the whole queue in one tick.
4. Empty queue remains empty; completed queue does not spawn repeatedly on later ticks. Keep test worlds free of opponents and avoid unrelated movement/combat effects.
5. Two identically built worlds, including a wide queued duration, receive identical command bundles and run multiple actual steps with equal entity state and dynamic checksums. This confirms widened countdown participation in deterministic replay without pretending to establish cross-platform proof on one host.
6. Exact canonical encoding and high-bit mutation sensitivity as described above. Existing comprehensive entity-field mutation and replay tests must continue to pass.

## Adjacent timer audit and follow-up boundaries

- `UpgradeDef.research_ticks` is `u32` and parses losslessly, but no research countdown exists; `research_upgrade` immediately applies effects. A future research queue needs its own plan and at least `u32` duration storage.
- Structure `build_time_ticks` parses as `u32`, but `Build` uses completed placement immediately. A future construction state machine needs its own explicit lifecycle contract and at least `u32` remaining duration. This round must report that limitation clearly.
- Authored ability cooldowns, channeling/cast durations, overclock/buff durations, hazard intervals/durations, weapon timing phases and projectile lifetime currently use `u16` on both definition/runtime sides. No analogous direct `u32` narrowing was found there. This is a width audit, not proof all mechanics or arithmetic are correct.
- Weapon period modifiers deserve a separate arithmetic plan: `effective_period_from_upgrades` adds an authored signed delta to a base period in `i32`, then narrows to `u16`; `step_weapons_and_projectiles` similarly narrows a buff-scaled period. Extreme positive deltas or slow-attack multipliers can overflow or truncate. Do not change these during the queue fix.

## Verification and acceptance

Run focused production/checksum tests and sim_core library tests first; record results under `/tmp/space-scuffle-r3-production-*.log`. Have an independent verification round inspect actual command-to-queue types, zero/FIFO semantics, the versioned byte representation, and test evidence for 65,535/65,536/high-bit differences. Source code review alone is insufficient.

At the shared source freeze coordinate and pass `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`; do not run overlapping full builds while other agents edit. Report any failing gate instead of claiming completion. No screenshot is needed for this strictly simulation-state change; any subsequent construction visual feature requires separate screenshot acceptance.
