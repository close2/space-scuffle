# Round 8: client pipeline foundations checkpoint

Status: focused verification passed; production migration and coordinated workspace gate remain pending. This is a coherent stage-one checkpoint under approved blueprint38, not completion of the gameplay pipeline goal. No editor sources were changed.

## Implemented boundary

`SimWorld::step_into` now owns the existing authoritative phase sequence, including both production/movement outcome clears; compatibility `step` delegates to it. `write_render_snapshot` fills retained output vectors and appends/drains the world's pending visual events exactly once while preserving both sides' vector capacities. Compatibility `build_render_snapshot` delegates to that writer. `RenderSnapshot` gained only `Default`, with no schema change. Checksum and phase semantics remain unchanged.

The compiled `client::battle_pipeline` module provides complete standalone command queue, checked clock, and adjacent-snapshot timeline primitives. It is not registered as a second running pipeline. The current production UI still directly steps commands, and the existing live synchronization still owns its old accumulator/rendering behavior. Those defects remain open until the coherent next implementation slice replaces them together.

Command submission captures recipients, owner, queue flag, generation and local sequence without stepping the world. Execution preparation revalidates those exact recipients; ordinary changes reject the corresponding command, while a fatal missing live catalog definition leaves the entire pending batch and prior output unchanged. Real production/movement outcomes have been exercised through the prepared bundle and mapped using their authoritative command indices. No authoritative success receipt is invented for other commands.

The clock uses checked integer durations, a 40 ms simulation period, five-tick frame budget and explicit 250 ms maximum scaled debt. Discarded wall time is recorded; no authoritative tick identities are skipped. Speed zero clears retained phase even after a budget-limited frame, and cumulative slowdown diagnostics remain available without falsely repeating the last frame's discarded duration.

The timeline has explicit uninitialized, initializing, published and tick-in-progress phases. It rejects missing/double publication, nonfinite/out-of-range interpolation and reused generations. It interpolates compatible spatial records while retaining prior discrete membership/state until the next displayed tick; paused presentation holds the exact latest snapshot. Pending visual events retain generation/tick/within-tick order and become eligible only after a corresponding pose is published. Exhausted client tick identity is rejected before rotating buffers, stepping, dequeuing commands or consuming evidence. Headless wrapping tick behavior is unchanged and remains a separate limitation.

## Verification evidence

- `/tmp/space-scuffle-r8-pipeline-primitives07.log`: 24 client library tests passed. Includes initialization misuse, tick exhaustion with unchanged real world/queue/events, pause/resume, no-tick interpolation, multi-tick event order, reset, radius discontinuity, shortest-angle interpolation, generation/sequence overflow, stationary direct attacks, fatal catalog revalidation and real production/movement index mapping.
- `/tmp/space-scuffle-r8-pipeline-snapshot03.log`: all four real simulation integration tests passed. Compatibility and reusable snapshots/checksums match through 120 ticks and deaths; the paid-production/rejected-production/movement trace preserves exact outcomes and subsequent clearing; writer event consumption and growth/shrink are checked.
- `/tmp/space-scuffle-r8-pipeline-clippy02.log`: `cargo clippy -p client -p sim_core --all-targets -- -D warnings` passed. Vendored Naga inherited warnings remain visible; no unrelated dependency cleanup was performed.
- `/tmp/space-scuffle-r8-pipeline-fmt01.log`: `cargo fmt --all -- --check` passed.

Earlier focused runs are retained, including Clippy01's excessive-bool/matching-arm findings, repaired using an explicit publication phase and documented merged roster policy. No assertion was weakened or skipped. Buffer pointer/capacity assertions concern those specific retained vectors only; no whole-client allocation claim is made. No native screenshot is claimed for these behaviorally unwired library primitives.

## Freeze and next slice

Owned files for this checkpoint are the two narrow step/snapshot regions of `sim_core/src/world.rs`, `sim_core/tests/snapshot_output_tests.rs`, the `RenderSnapshot` Default derive, `client/src/battle_pipeline/`, and the single module declaration in `client/src/lib.rs`. Other dirty sources belong to prior work or concurrent lanes and were preserved.

Freeze this boundary for the shared workspace gate. After release, stage two must install the sole tick owner, queue UI actions, initialize committed generations/metadata atomically, drain every tick's typed outcomes, and publish retained presentation through explicit schedule/deferred boundaries. Production plugin/direct-fixture/capture tests, real frame-rate and paused visual evidence, scoped allocation profiling, and a final coordinated workspace gate are still required. Current functional results do not accept rejected game art or prove the running application has adopted the new pipeline.
