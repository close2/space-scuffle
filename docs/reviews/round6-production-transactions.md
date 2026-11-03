# Round 6 production transaction verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Scope: approved plan43, including the dead-at-entry phase2 guard and living-producer recheck after hazards. No editor/assets changes and no plan45 movement implementation. Build remains the recorded free/immediate construction defect; this work is mobile training, setup-spawn atomicity and supply lifecycle, not full SC2 construction or exact SC2 queue behavior.

## Implemented transaction boundary

- Authored `trains_units` and `requires_structures` resolve through the current external catalog. Complete-bundle admission rejects blank, duplicate and dangling references; partial parsers remain usable. Producer eligibility, geometry and current capability are validated before payment and before countdown mutation. Removing permission from a paid job produces typed invariant evidence and retains the job/reservation.
- `ProductionJob` captures product, u32 countdown and full enqueue-time supply reservation. This is the explicitly approved Space Scuffle FIFO policy. `ProductionOutcome` distinguishes normal player rejection from corruption; full ticks clear outcomes and consumers can drain without discarding buffer capacity. `ProductionBlock` reports current exit obstruction separately from fatal diagnostics.
- Entities capture supply consumption/provision at insertion. Registered dead entities remain in the ledger until actual reap. Provider totals retain uncapped values; effective supply is capped at 200. Setup can construct an over-cap scenario, but training cannot oversubscribe capacity. The current default battle's authored roster is 14 supply with a 15-supply provider per player; no artificial capacity is granted.
- Shared spawn preparation checks player, definition/vitals, ID vacancy/exhaustion and prospective supply before mutation. Structure placement is the last fallible domain operation; entity insertion/counters follow only after success. Production transfers its existing reservation directly, never charging then correcting afterward.
- Completion enumerates a deterministic local perimeter, checks continuous producer-boundary terrain/occupancy with the complete footprint, and uses refreshed spatial buckets including earlier same-phase children. Only the producing foundation is exempted during emergence, not terrain or other walls. Ready jobs remain intact while blocked.
- Dead-at-entry entities do not regenerate or train. Producers killed by same-phase hazards cannot finish after the hazard. Reap prepares captured ledger release and validates the actual foundation before removal; it emits shatter only for committed removals and does not invent refunds.
- Checksum schema3 includes all captured entity supply, job reservations and uncapped provider totals. Outcome/block buffers are derived observations. Production occupancy does participate in admission but is rebuilt deterministically from authoritative entities before use; these reproducible derived buffers are excluded from the checksum.

## Evidence and retained failures

- Existing timer4 + checksum9 integration tests pass: `/tmp/space-scuffle-r6-production-tests01.log`. Wide durations retain real RON input, FIFO rules and high-bit sensitivity.
- First new transaction10 tests pass: `/tmp/space-scuffle-r6-production-tests02.log`.
- Expanded transaction14 tests pass: `/tmp/space-scuffle-r6-production-tests03.log`. Coverage includes actual placed-wall ring blockage/removal for small and large products, disconnected clear plateau rejection, two producers contesting one exit, aerial occupancy versus ground obstacles, corruption before countdown, captured provider totals/death, prerequisites and 2000-tick replay.
- Strict sim_core all-target Clippy03 passes: `/tmp/space-scuffle-r6-production-clippy03.log`. Subsequent test additions and narrow source refinements still require rechecking; earlier Clippy failures are retained as logs01/02.
- First full sim_core run failed: `/tmp/space-scuffle-r6-production-full-sim.log`, exit101. `damage_and_shields_tests` directly cleared the entity map between splash scenarios, violating the new captured ledger. The fixture now uses real death/reap while retaining its splash assertions. The separately approved client presentation fixture likewise uses actual reap instead of direct removal. Checksum-only mutation/reinsertion fixtures remain intentional and unchanged.

Final focused verification now passes:

- Full sim_core suite and doctests: `/tmp/space-scuffle-r6-production-full-sim02.log`, exit0. Includes the repaired splash fixture, schema3 exact bytes, bounded extreme slab comparisons, one-raw-unit tangency, ramp orientation/map-edge behavior, derived command outcomes, checked supply/vitals boundaries and existing simulation regressions.
- Final public transaction suite: **18 passed**, `/tmp/space-scuffle-r6-production-tests04.log`. The last added regression uses a real fractional mobile producer, real placed wall and actual production phase: the paid ready job persists without allocator/economy mutation until the wall is reaped. This prevents a future center-snapping workaround from passing only helper tests.
- Client fixture first failed because its sparse-owner setup also rewrote entity ownership after initialization, invalidating the captured ledger. That retained failure is `/tmp/space-scuffle-r6-production-client-fixture.log`. The approved repair remaps copied scenario/player/map-start/initial-order data before actual initialization; no counters are patched manually. All **3 presentation tests pass**, `/tmp/space-scuffle-r6-production-client-fixture02.log`.
- Strict sim_core + client all-target Clippy passes `/tmp/space-scuffle-r6-production-clippy05.log`; full workspace formatting passes `/tmp/space-scuffle-r6-production-fmt02.log`. Existing third-party Naga lifetime warnings remain printed and are not claimed repaired.

The exact swept-square slab amendment and dead-producer guards are reviewed and approved. Product source is frozen for the orchestrator's combined workspace gate. No full workspace acceptance is claimed until that coordinated gate completes.

## Remaining handoff and boundaries

Plan38 owns normal client command submission and bounded outcome display. It must drain `production_outcomes` immediately after each actual tick, including catch-up ticks, mapping command indices to its retained submission metadata. Normal rejected orders remain nonfatal. Existing fatal diagnostics continue to stop/report corrupted simulation state. No new UI visuals have been claimed by this simulation round.

Normal movement and collision currently have the separately recorded Ground/Air mover-policy defect. Production exit admission handles its local layer checks, but does not establish correctness of subsequent flight or specialized cliff movement; approved plan45 remains held until43 acceptance. No broad media-source baking or replacement artwork belongs to this round.

Final independent review confirmed that the existing mobile source-footprint boundary already prevents enormous positive radii from entering perimeter enumeration: both admission and countdown call the same i128 map-bound validator. The additional public regression exercises admission rejection without payment and paid-job retention without countdown/ID/economy mutation. It passes `/tmp/space-scuffle-r6-production-source-radius.log` (1 passed, 18 other transaction tests filtered). Final strict sim_core all-target Clippy06 and workspace fmt03 pass. No perimeter/radius clamp was introduced. Product source is frozen and no workloads remain running for the orchestrator's combined gate.
