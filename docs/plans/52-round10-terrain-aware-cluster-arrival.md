# Round52: Terrain-connected cluster arrival

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: approved and released for implementation after the unchanged round10c workspace gate passed (688 tests, 88 harnesses, zero failed/ignored; Clippy and formatting passed; 448 source hashes unchanged). Implementation is limited to the owned arrival helper/caller/tests. No assets, editor work or native execution.

## Goal and reproduced defect

Prevent a ground Move or AttackMove from completing solely because an adjacent settled actor is geometrically close across disconnected terrain. Keep the existing proximity-based cluster policy, Ground/Air separation, command transaction rules and collision behavior. This is a correction to the game's current arrival contract, not a new formation system or an SC2-parity claim.

A source-independent temporary public-API harness loaded the actual `assets/maps/showcase_canyon.scmap.ron` through `MapFile::load/decode/to_game_map` and actual external runtime catalogs. It selected a mobile archetype by mover/shape capability, not a creature identifier. Generic controlled overrides were radius0.5 world units, maximum speed0.25 world units/tick and no weapons; all remaining entity/lifecycle state came from real spawn APIs. Seed: `0x5220261028` (`352726683688`). Ground/Air use their respective catalog-defined mover archetypes. No map or catalog files were changed.

The first deterministic usable cliff edge on that map yields:

| Field | Exact value |
| --- | --- |
| Moving actor | EntityId1, PlayerId0, `(25.5,1.5)`, cell `(25,1)`, cliff2 |
| Destination/settled selected groupmate | `(26.5,1.5)`, cell `(26,1)`, cliff1; groupmate EntityId2 |
| Raw actor position | `[109521666048,6442450944]` |
| Raw destination | `[113816633344,6442450944]` |
| Real Medium path | `(25.5,1.5) → (25.5,8.5) → (29.5,8.5) → (26.5,1.5)` |
| Input at tick0 | Player0 selects `[1,2]`, nonqueued Move or AttackMove to the exact destination |

Both group members have zero velocity at phase entry. At tick1 the accepted grouped command produces empty order queues for both actors; EntityId1 never moves. It remains on cliff2 through tick1024 despite the valid alternate ramp route. Both solo controls start moving at tick1 and reach the exact destination at tick75. This is not merely a failed unreachable-path request or a tolerance difference near an otherwise accessible endpoint.

A second controlled case places the settled groupmate at `(25.5,2.5)` on the actor's same terrain surface, retaining the destination across the cliff. Both grouped Move and AttackMove again complete immediately. Checking only actor-to-neighbor connectivity would therefore miss the defect: the neighbor-to-goal leg also needs a terrain witness.

Evidence:

- `/tmp/space-scuffle-r10-round52-harness01.log` and `/tmp/space-scuffle-r10-round52-evidence01/`: seven1024-tick cases, each independently rerun and compared for complete trace equality. Every tick retains actual positions, velocities, orders, funnel points, admission output, diagnostics and full authoritative checksum; no ignored checksum prefix or sampled-only replay claim.
- `summary.json`: group Move, group AttackMove and fog-enabled group Move never move; solo Move/AttackMove reach exactgoal at75; flat-ground and Air controls preserve their existing immediate proximity settlement.
- `/tmp/space-scuffle-r10-round52-harness02.log` and `...evidence02/`: both same-side-witness cases,16 ticks each, each exact-replayed. Both drop their orders at tick1 without moving. Original and amended temporary harness sources are retained beside their respective evidence; their Cargo manifest/lock are retained in evidence01.
- Map SHA256: `4bddbac99b3509a4fcf46c2fb90a9ea6e733d981b11e181fe321e32d1417b681`.
- Read-only simulation-source/catalog/map inventory SHA256: `04945ad888f9cf94fd14d0fe2f1038717a051af845b33b0b9301cbc5ffac25e6`. This was recorded after the read-only runs; it is not mislabeled a frozen pre-build inventory or a performance baseline.

All harness runs completed without simulation diagnostics and passed the captured supply-ledger validation. Harness exit0 records successful reproduction/replay, **not** correct group behavior. Flat/Air outcomes are controls for the retained project policy, not evidence that ground cliff completion is acceptable.

## Actual implementation and existing coverage

`movement/locomotion.rs::advance_checked_movement` constructs stable-ID obstacle slices from validated living, noncargo actors, partitioned by actual movement layer. `prepare_actor_motion` handles Move/AttackMove together. If its prospective route is empty, it first permits a genuinely legal one-tick arrival snap; otherwise it calls `check_cluster_arrival_except`, and only afterward asks ground pathfinding for a route. A true cluster result calls `MotionProposal::finish`, which atomically clears the route, zeros velocity, pops the current order and sets Idle.

`steering.rs::check_cluster_arrival_except` validates positive radii and excludes the querying identity. Any zero-velocity obstacle is a witness if it lies within `(actor radius + witness radius +1.5)` of the goal and the actor lies within `(actor radius + witness radius +0.15)` of it. All comparisons use50 widened arithmetic and the frozen f32-authored raw thresholds. It has no map, owner, shared-command or actual arrival-state information. It cannot establish a connected terrain surface from Euclidean distances alone.

Existing checked-movement tests protect exact raw thresholds, invalid-radius evidence and self-exclusion. Ramp tests protect actual movement/waypoint snaps against crossing cliff sides; they do not protect order completion without displacement. The existing eight-unit fan-out test checks progress, lateral spread and overlap after80 ticks, but does not require all orders to complete or prove their arrival terrain. None of these tests covered the reproduced false completion.

## Contract: an explicit local terrain witness

Keep every existing geometric condition. For a Ground-layer actor, a geometrically eligible settled neighbor additionally must establish **both**:

1. The actor center and witness center are connected by the shared exact ground-surface segment predicate.
2. The witness center and requested goal are connected by that same predicate.

The two segments form a concrete local terrain path through the witness. They must validate every crossed cell/cardinal/diagonal edge using `GameMap::ground_segment_is_connected`, including ramp profiles and their landings. Same cliff level, same map cell, a bounding rectangle, or a global component ID alone is not sufficient. A legal longer path elsewhere does not certify immediate local settlement: if the local witness fails, retain the order and let existing navigation continue along that longer path. Both reproduced cases must fail the arrival witness for different legs.

Air keeps the existing geometric cluster test, with its already validated full-radius map domain and Air-only obstacle slice. Ground cliffs/structures do not become flight barriers. Conservative CliffWalker/CliffJumper classifications retain the current Ground policy; no specialized movement inference is introduced.

The witness is a **terrain-surface path**, not a new collision-free placement or actual displacement. Preserve the existing distinction deliberately:

- `MotionPolicy` continues to validate actual actor/neighbor radii and map bounds. The active goal must remain inside the moving actor's full-radius domain.
- Ordinary Ground movement still uses its current center/clearance-class traversal, not43's stricter swept production footprint. This arrival check does not move a body or grant permission for a later move, push, snap, spawn, Blink or unload.
- The segment policy supplied for this terrain-only proof accepts occupancy. The shared map predicate still rejects unusable terrain, invalid ramps and disconnected edges. It already treats a placed structure's occupied cells as their underlying terrain when checking connectivity; that is intentional here.
- Do not add a static-structure wall-off/collision proof to this round. Existing cluster settlement can intentionally stop near a settled actor or dome when the requested center is occupied; making it prove transit through that occupied center would change that project policy. Structure-side settlement semantics remain unchanged. Do not call the resulting predicate a guarantee of collision-free transit through a foundation.
- Do not inspect current enemy structure occupancy, remembered blocked-cell masks or visibility in this new proof. Terrain connectivity is shared physical terrain; arrival must not acquire new hidden-structure knowledge. Existing ground route generation/revalidation still uses its original `PathRequestWithVision` and knowledge mask. Actual displacement still checks actual physical blockage.
- Other owners and fixed bodies retain their existing geometric-witness eligibility. Adding group IDs, shared destinations, friendly-only witnesses or formation reservations would be a separate policy change requiring evidence and a separate plan.

The new predicate is a monotonic restriction on the existing arrival branch: a previously false geometric result cannot become true. It adds no fallback arrival radius, closest-free goal, timeout success or arbitrary unit-count threshold.

## Narrow code/API design

1. In `steering.rs`, factor the existing loop into one crate-visible checked helper accepting an eligibility predicate for geometrically valid witnesses, e.g. `check_cluster_arrival_with(excluded, position, radius, target, obstacles, witness_allowed) -> Result<bool, PushGeometryError>`.
2. Keep both existing public helper signatures as geometry-only compatibility queries. `check_cluster_arrival` still excludes no identity; `check_cluster_arrival_except` still applies the requested identity exclusion. They delegate to the one checked core with an always-true eligibility predicate. Their rustdocs must say they alone do not establish world terrain connectivity.
3. The core preserves radius-validation/error ordering, stable obstacle order, zero-velocity requirement, self-exclusion and exact threshold raw values. Even if one valid witness is found, later malformed obstacle radii must still return the same typed error; do not early-return success and hide corruption. After a valid witness has established arrival, skip subsequent geometric comparisons and eligibility callbacks, but continue the same self-exclusion and positive-radius validation for every later nonexcluded obstacle before returning success. The remaining geometric comparisons are infallible widened predicates, so skipping them cannot hide an arithmetic diagnostic. Eligibility receives only immutable witness data and is a pure map-read closure at the world call site.
4. `prepare_actor_motion` supplies the layer-aware witness closure. Ground invokes the two shared `ground_segment_is_connected` queries with occupancy-accepting policies; Air returns true without querying ground terrain. The helper owns no new Vec/map/route/cache. Existing `MotionProposal` commits remain untouched; a failed witness simply reaches the current path-search/displacement flow.
5. Document the local proof and its occupancy/ordinary-clearance boundary beside the caller. Do not duplicate traversal arithmetic or add a second ramp implementation in sim_core.

No authoritative fields, command payloads, receipt/outcome layout, snapshot fields or serialization formats change. No checksum schema bump is needed: schema4 still encodes the same state layout. Changed ground behavior naturally changes replay checksums for affected inputs; old movement traces are retained as failing evidence, not updated into passing golden outputs without behavioral assertions. Catalog injection,51 transport absence and50 numeric diagnostics/proposal atomicity remain intact.

## Work bounds and performance claims

Do not run A*, enumerate global reachability, build a component cache or clone paths per settled-neighbor test. Perform terrain work only after the existing exact geometric and settled/self conditions identify an eligible witness. The shared segment query allocates no storage and visits crossed grid cells deterministically; worst-case work is bounded by the two segment lengths/map dimensions. Existing authored radii may be large, so do not invent an unconditional constant-time claim.

The world already scans its same-layer obstacle slice; this round does not claim to solve that existing crowd complexity or guarantee zero allocations across the entire simulation tick. Test a callback counter to prove geometrically ineligible/moving/self obstacles do not trigger terrain queries, a rejected candidate permits later candidates, and no further callback runs after the first accepted witness while a later malformed nonexcluded radius still returns the existing error. The implementation must document this bounded-work choice with the charter's PERFORMANCE RATIONALE block. Retain50's performance status as inconclusive and unchanged; no new wall-time acceptance claim comes from this correctness harness. Required workspace performance tests must still pass unchanged at the coordinated gate.

## Implementation and verification rounds

Implementation is released only after root approval and the preceding51/shared checkpoint.

1. Add the checked predicate core and preserve existing public wrappers; narrow caller wiring/documentation only.
2. Add focused tests before claiming the fix. Reuse loaded-map coordinates above with capability-selected generic fixtures and real public TickBundle commands, not direct route injection.
3. Independently audit unchanged threshold/error branches and run the retained external traces against the implemented code. Update the evidence directory rather than overwriting failures. The correction must survive exact repeated runs.

Required behavioral matrix:

- Both actual-map cross-cliff grouped Move/AttackMove cases: accepted admission remains; tick1 must not pop the actor's order, movement must follow legal terrain segments, and the actor must eventually reach the destination surface/valid neighborhood through the ramp. Account for normal live collision settlement instead of requiring both bodies to occupy one exact point. All ticks diagnostic-free; captured supply unchanged.
- Same-side settled witness but goal across cliff: both order kinds must also continue navigation. This prevents a one-leg-only fix.
- Loaded-map reversed actor/witness sides, ramp-side adjacency, diagonal cliff corner and malformed ramp profiles: no false completion through a terrain-disconnected leg.
- A valid short two-segment witness around connected terrain, same-cell/flat/tangent thresholds, reverse obstacle ordering with unequal distances, and already-settled flat groups preserve the existing result. Exact frozen0.15/1.5 arithmetic boundaries remain covered.
- Air pairs over the same cliff still use existing proximity settlement. Cross-layer neighbors cannot witness arrival; dead/cargo actors remain absent from obstacle slices.
- Real registered foundation at/near the occupied requested center on flat terrain: retain existing settlement result and map occupancy, with unchanged known/unknown masks. Separate disconnected-terrain cases still reject even if a structure is involved; structures never waive terrain edges.
- Fog-enabled and disabled original fixtures both reject false completion; do not reveal or mutate knowledge masks through an arrival query.
- Queued second Move/AttackMove: false witness must not pop the first order or expose the next intent early. Successful existing flat arrival still pops exactly one order; preserves proposal atomicity.
- Geometrically eligible but terrain-rejected first witness followed by a valid witness must succeed; invalid radius anywhere still yields typed failure without partial actor mutation. Pure public helpers retain their documented geometry-only result.
- Moderate grouped CPU movement over an authored ramp verifies no diagnostics, stable ordering and repeated full checksums. This is correctness/congestion smoke evidence, not a new timing benchmark or proof of optimal formations.

Focused checks: sim_core steering/checked movement, ramp movement, movement layers, AttackMove, transport and snapshot output tests; sim_map traversal suite if shared predicates are touched (the intended implementation does not change them). Then coordinated `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. No threshold relaxation or broad fixture weakening. A production capture may later confirm ordinary grouped ramp movement through the existing real renderer; it cannot substitute for the adversarial simulation assertions, and no new visual/control adapter is included without separate approval.

## Ownership and exclusions

Owned implementation files: `crates/sim_core/src/steering.rs`, the narrow cluster-arrival call in `movement/locomotion.rs`, and new/adjacent simulation tests. Avoid `world.rs` phase extraction/snapshot constructors, client queue/palette/capture code,51 transport representation/lifecycle,50 math primitives, assets and all editor code.

This plan does not implement formations, assignment of distinct destinations, crowd optimization, route-completion policy for all blocked goals, ownership-aware settlement, construction48, wall-off redesign, target acquisition or SC2 mechanics. It fixes the reproduced terrain-disconnected arrival witness completely within the stated existing project policy. If implementation uncovers a required change to that policy, stop the expansion and amend the blueprint for review rather than silently folding it into this correction.

## Frozen-gate fixture readiness audit

The two retained public-API harness sources and complete failing traces are ready for replay after release; no new test/build was run during the round10 hold. The implementation regression should use the recorded map coordinates directly and assert their terrain/alternate-path preconditions. Do not rerun the discovery search and silently substitute a different cliff if authored terrain changes.

| Fixture | Setup and decisive assertion |
| --- | --- |
| First-leg disconnection | Actual showcase canyon; Ground actor(25.5,1.5), groupmate/goal(26.5,1.5), radius0.5, speed0.25. Assert current differing levels and real alternate Medium path before tick0. Both Move/AttackMove must keep the first actor's order at tick1 and progress around the real ramp. |
| Second-leg disconnection | Same actor/goal; groupmate(25.5,2.5). Prove actor→witness connected and witness→goal disconnected. Both order kinds must survive tick1. |
| Reverse direction | Exchange actor/goal sides in the first fixture; prove the two-leg predicate and alternate route in that direction rather than assuming symmetry of a malformed ramp. |
| Flat/Air controls | Same exact numeric positions, either a flat map control or an actual capability-selected Air archetype. Preserve existing immediate cluster settlement; do not require exact point overlap. |
| Queued follow-up | Submit initial group Move/AttackMove followed by a queued second order in the same real TickBundle. Both command receipts must be accepted; disconnected settlement may not expose/pop the second order. Flat successful arrival pops only the current order. |
| Foundation occupancy | Capability-select a dome, author generic1×1 footprint/radius0.5 and no weapons for the fixture, and call actual spawn_structure. On a flat map, actor center(9.5,10.5), foundation center/goal(10.5,10.5) are tangent. Assert real footprint registration, terrain-only witness connectivity and unchanged foundation position/occupancy after movement. Repeat separate worlds with fog disabled/enabled, and preserve known-mask behavior rather than editing the ledger or occupancy. |
| Invalid geometry/error order | Unit-test the new shared checked helper with a valid witness followed by a malformed nonexcluded radius; the typed error must survive callback short-circuit. Self-excluded identity retains its existing skip behavior. A world malformed-actor fixture must honor50's existing per-actor validation: do not incorrectly require every unrelated healthy actor to stop when another actor is excluded during phase-entry validation. |
| Work bound | Count eligibility calls for self/moving/out-of-range obstacles, rejected then accepted witnesses, and later valid/invalid radii. After success, no further terrain callback; later malformed nonexcluded radius still errors. |

Existing `ramp_movement_tests.rs` provides loaded-map and minimal real-ramp construction patterns; `checked_movement_tests.rs` provides external-catalog capability selection and exact raw-threshold regressions; production transaction tests provide actual registered-dome fixture patterns. New crate-visible helper tests belong beside steering's private tests so no new public testing hook is introduced. End-to-end arrival tests belong in a dedicated `cluster_arrival_tests.rs` integration module. Catalog/runtime loading uses `CARGO_MANIFEST_DIR`-relative actual assets, never the temporary harness's absolute workspace root.

For actual-map runs retain every tick's `last_checksum`, periodic/direct full checksum agreement as appropriate, exact replay equality, no diagnostics, supply-ledger validation and actual displacement connectivity. The original1024-tick trace is a bounded observation window, not a new gameplay timeout rule. If corrected motion fails to arrive within that generous retained window, preserve the new trace and investigate rather than weakening order/terrain assertions or treating timeout as arrival. No native image claim comes from these CPU fixtures.

## Implemented bounded result and verification checkpoint

The approved helper/caller/test slice is implemented and frozen for coordinated workspace verification. Four helper tests, six new integration tests, the92-test focused movement/transport/snapshot/library suite and strict simulation all-target Clippy pass. Original external1024-tick traces replay with output-path-only adaptations: corrected Ground groups move at1 and complete at77; solo, Air and flat complete traces remain identical to retained originals. Both16-tick same-side-witness cases now move at1 without popping the order. No editor/assets/map implementation changes or performance/native claim. Full evidence and honest workspace-format pending status are in [the verification report](../reviews/round11-terrain-connected-arrival.md).
