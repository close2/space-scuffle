# Round 7: Fixed-body collision weights preserve registered foundations

Status: root approved the narrow45 dependency, including fixed weights, final apply guard and registered-foundation placement rejection. This is a narrow dependency exposed by45, not a general push-priority rewrite. No editor, catalog/media asset or construction workflow changes.

## Reproduced failure

The45 integration regression `air_foundation_collision_preserves_registered_footprint_position` places a real authored dome through `spawn_structure`, changes only its authored mover to Air, and overlaps it with an enemy Air actor. `resolve_collisions_and_push` moves the dome from `(17.5,17.5)` to `(16.125,17.5)` while map occupancy remains at the registered foundation. The failing simulation integration test log is `/tmp/space-scuffle-r7-movement-foundation.log` (exit101).

`compute_push_displacement` handles enemies before examining any immobility flags and returns half correction for both actors. Its friendly both-immobile branch also returns reciprocal displacement. Ground structure occupancy previously happened to prevent the physical write; Air correctly ignores that occupancy, exposing a real lifecycle violation. The fix cannot depend on mover type or specific dome identifiers.

## Mechanical contract

Introduce explicit pair collision mobility: `CollisionMobility::{Dynamic, Fixed}`. Fixed bodies have zero positional correction weight; dynamic bodies have unit weight. This models kinematic immobility rather than changing mass, selection orders or faction behavior.

The authoritative world classifies an entity Fixed when it has a registered map foundation, `is_immobile`, or `unpushable`. Foundation identity is decisive even if mutable entity flags are corrupted to movable. HoldPosition remains its existing friendly-priority rule, not automatically a new fixed-body capability; current same-layer enemy collision semantics for a merely holding mobile actor remain unchanged. Transported/dead actors remain excluded before pair processing. Channeling that explicitly sets the immobile/unpushable flags naturally uses Fixed; no new cast interruption rules.

Resolve an overlapping pair in this order:

1. Different movement layers are already excluded by45; this helper sees a same-layer pair.
2. Compute representable nonnegative overlap and the same deterministic separation normal. At coincident centers preserve the existing stable axis tie convention.
3. Both Fixed: return zero deltas. Neither entity nor foundation can be moved to solve an impossible authored/setup overlap.
4. One Fixed: assign the full positional correction along the separation normal to the Dynamic actor; fixed delta is exactly zero. No enemy branch, lateral priority or symmetric split may override zero weight. Terrain/domain validation still occurs on the proposed dynamic displacement; if it is blocked, retain the legal positions rather than moving the foundation or teleporting to a free point.
5. Both Dynamic: run existing priority, hold, enemy reciprocal and engagement/wedge rules unchanged. This round does not tune their constants.

Raw overlap and normal math must use45's checked/widened distance contract; do not reintroduce squared fixed-point overflow. The fixed branch uses only bounded multiplication of a unit direction by a representable overlap. No supply, queue, production reservation, registered footprint or entity-ID mutation occurs in this collision operation.

## API and compatibility

Add `compute_push_displacement_with_mobility(a, b, distance, combined_radius, mobility_a, mobility_b)`, documented in world units, and an explicit public mobility enum. The final approved API decision is typed migration: `compute_push_displacement` keeps its argument order but returns Result, and `resolve_push_interaction` returns Result with atomic position preparation. Existing `PushEntityParams` struct literals remain valid. Both entry points use one checked dynamic core; see the approved amendment below.

The authoritative world calls the new weighted helper with mobility resolved from current fields and the map registration. It must also suppress applying accumulated displacement to a currently Fixed recipient, since public phase entry or future accumulation changes must not bypass the invariant. Derive this check each collision phase; do not add authoritative entity fields or checksum schema. Existing diagnostic behavior for malformed layer/radius state remains45's responsibility.

Teleport/unload are deliberate placement operations, not collision corrections. Their45 destination contract must separately reject moving a registered foundation without an atomic map-placement transaction; add a typed `RegisteredFoundation` placement rejection before any fields/costs change. Do not silently turn teleport into a foundation relocation feature. Setup `spawn_structure` and future complete construction plan48 own registration/placement. This is a narrow lifecycle guard required by the same demonstrated map/entity identity invariant.

## Scope and ownership

Own `steering.rs` weighted helper/enum, narrow world collision classification/call/apply guards,45 displacement admission foundation guard, and focused tests. Coordinate with45 owner (same current track). No step/snapshot/client queue changes;38 retains those. No broader velocity/steering rewrite, live structure relocation or mass tuning.

## Acceptance

- Keep the failing Air foundation test and prove its real entity position and map footprint remain identical after collision; the dynamic actor receives any legal correction. Repeat Ground and Air, same/enemy owner, both ID orders.
- Generic mobile fixture marked `is_immobile` or `unpushable` is fixed in both layers; clearing those flags restores Dynamic behavior when no foundation exists. Corrupted movable flags on a registered foundation cannot detach it.
- Two Fixed actors at overlapping/coincident positions never move; one Fixed plus one Dynamic uses the full normal correction, stable for reversed argument order and exact tangency.
- Both Dynamic tests preserve existing push priority, hold and enemy behavior, including movement45 cross-layer exclusion and large/fractional broad-phase cases.
- If terrain blocks the dynamic correction, fixed body still stays fixed; there is no nearest-free search or foundation occupancy exemption.
- Blink/unload against an actually registered foundation reject before position, cooldown, energy, cargo/order state, supply or map mutation. Existing mobile placement tests still pass.
- Production43 setup/exit/reap tests and ledger checks remain passing; full sim_core and coordinated workspace tests/strict Clippy/fmt are required. This mechanical fix provides no flight-media visual acceptance.


## Approved arithmetic clarification

The new weighted API returns a typed geometry error. Validate nonnegative distance/radius and verify distance against widened raw geometry; reject inconsistent caller distance before correction. Fixed-body normals use checked component differences divided directly by the already widened distance, never `normalize_or_zero` through Q32.32 squared length. Checked component multiplication produces corrections; coincident centers use the documented deterministic unit axis. Final narrowing occurs only after wide intermediates. The Dynamic/Dynamic branch now uses the single checked core defined in the final approved amendment below. It preserves meaningful ordinary branch/priority behavior while widened normalization can change low raw bits. Geometry failures in authoritative world collision are reported through movement diagnostics. Tests cover 30k/40k/50k normal geometry, coincident centers, tangent circles, inconsistent/negative supplied distance and truly unrepresentable final distance.


## Remaining phase arithmetic boundary

The weighted pair API's checked contract does not imply phase-wide checked arithmetic. Existing world friendly/hard push accumulation, velocity/buff scaling and final speed capping still use Q32.32 operations;45/49 must not claim those unrelated aggregations are fully overflow-safe. Any newly introduced pair calculations must return typed failures, with boundary tests. A complete checked aggregate movement pipeline needs its own reviewed blueprint rather than an unreviewed change to all movement/weapon numeric semantics.

## Checked dynamic-core amendment (approved)

Repository call inventory: `compute_push_displacement` has exactly three direct integration-test call sites (two in `collision_and_immobility_tests.rs`, one in `push_priority_tests.rs`), the local `resolve_push_interaction` wrapper, and the new weighted helper. `resolve_push_interaction` has no repository caller. Both functions are re-exported from sim_core; no published embedding compatibility guarantee was found beyond this round's own proposed compatibility preference. Retaining an unchecked duplicate algorithm is therefore not justified.

Change `compute_push_displacement` to return `Result<(SimVec2, SimVec2), PushGeometryError>` and keep its existing argument order. It validates the supplied distance with the widened raw distance helper, then runs a **single checked dynamic core**. The weighted helper calls the same core for Dynamic/Dynamic; Fixed cases remain separate simple weights. Change `resolve_push_interaction` to return `Result<(), PushGeometryError>`: prepare both corrected positions with checked component additions before assigning either actor. Migrate the three integration tests to propagate typed results without weakening their displacement assertions. Expose the widened distance query with documented world-unit/Option semantics so external callers can supply the exact distance without using the old squared-Q32 helper. This is an intentional documented API change in the current workspace development, not a silent compatibility bridge.

The single dynamic core retains branch order and ordinary priority behavior, but every actual arithmetic operation has explicit handling: checked difference/add/scale/divide; widened length and checked normalization; checked dot/cross intermediates with final checked narrowing; unsigned raw absolute comparisons instead of signed MIN abs; bounded constants with their original values. Zero/epsilon normals retain the existing deterministic axis/zero policy. Normalize from the widened distance directly, including velocities and wedge vectors; do not square them in Q32.32. No expect, panic, catch_unwind or default displacement converts a typed error into apparent success. Validate only the branch actually used, so an unused hypothetical direction does not reject an otherwise representable result.

Tests exercise every dynamic branch (enemy, single friendly hold, both hold, idle/moving priority, moving/moving wedge) with ordinary fixtures, near-raw quantization inputs and large representable normal geometry. Negative/inconsistent distance and genuinely unrepresentable final deltas return errors. `resolve_push_interaction` failure preserves both complete entities. The world still reports pair errors through diagnostics; its separate accumulated-push/capping arithmetic remains explicitly outside this pair guarantee.


## Coincident fixed-body tie clarification (approved)

Dynamic/Dynamic retains its historical unconditional X-axis fallback at coincident centers. Its argument order is authoritative stable EntityId order in the world; this round does not promise swapped-argument physical invariance for that legacy tie. For the new Fixed/Dynamic branch only, choose normal +X when `a.id < b.id`, otherwise -X for distinct IDs. Repeated identical IDs produce a typed pair-identity error. The normal is antisymmetric under swapping arguments, so the same dynamic actor moves in the same physical direction regardless of call order. Both Fixed remains exactly zero. Test both numeric ID orders and both argument orders at coincidence, plus explicit legacy D/D axis behavior. Do not claim universal reversal invariance across the legacy tie convention.
