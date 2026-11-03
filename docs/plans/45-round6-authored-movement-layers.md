# Round 6: Authored movement layers and explicit cliff-capability boundary

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: reviewed and approved for the bounded Ground/Air lifecycle, only after plan43 acceptance and source freeze. No plan45 product edits are authorized before plan43 and the current gate complete. Plan43 implementation is accepted in its bounded scope: the combined round7 workspace tests, strict Clippy and formatting passed with no source changes (`/tmp/space-scuffle-round7-workspace-results.json`). The refreshed admission/ownership amendment below still requires orchestrator review before implementation. No editor or media changes belong here.

## Verified defect and bounded goal

`UnitArchetype::mover_type` is loaded from external catalog data into the deterministic `MoverType` enum. Ground, Air, CliffJumper and CliffWalker all exist in the current authored roster. The movement pipeline nevertheless sends ordinary movement, final-position snaps, attack pursuit, collision pushes and hazard escape through the ground traversal predicate. Navigation always requests the ground A*/funnel pathfinder. The shared obstacle list, cluster-arrival checks and collision pairs contain all layers. `build_render_snapshot` separately gives Air a visual altitude offset, so apparent flight is not evidence of authoritative flight.

Repair the complete Ground/Air movement and collision lifecycle using the existing authored distinction. Preserve ground terrain/ramp rules and ground pathfinding regressions. Keep specialized cliff traversal explicitly outside this implementation slice until its missing authored contract is resolved below. This proposal must not be represented as full implementation of every `MoverType` variant. No unit slug, faction or showcase identity may determine movement behavior.

## Existing map and source contracts

`sim_map::CellFlags` contains ground pathability, structure occupancy, impassable cliff, ramp, buildability, vision blocking and speed-zone bits. It has no no-fly mask or authored aerial obstacle height. `GameMap::ground_segment_is_connected` validates every crossed terrain edge, including actual ramp orientation and diagonal corners; caller policy owns occupancy. `PathRequestWithVision` exposes only ground clearance and optional ground-structure knowledge. Do not reinterpret LOS_BLOCKER or UNPASSABLE_CLIFF as aerial barriers, invent blocker heights, or add map-editor controls in this round.

The repository's Air enum documentation already specifies ignoring ground cliffs, pathing and ramps. The bounded project rule therefore treats current rectangular map bounds as the aerial terrain domain, including a unit's actual radius. Ground structures and ground units do not obstruct Air. Air units share an aerial collision layer with other Air units; existing friendly-push/hold/priority rules apply within each layer. Air movement does not acquire fog-dependent ground structure detours. No no-fly region is claimed supported without an authored map-format extension.

Blizzard's public protocol distinguishes a unit's observed flying status and provides separate terrain/pathing/placement grids. It also distinguishes ground/air weapon target types. These interfaces support separating movement domain from ground occupancy and combat targeting; they do not specify exact aerial separation, cliff-jump timing, launch clearance or cooldown. [Official raw observation schema](https://raw.githubusercontent.com/Blizzard/s2client-proto/master/s2clientprotocol/raw.proto), [official unit/weapon data schema](https://raw.githubusercontent.com/Blizzard/s2client-proto/master/s2clientprotocol/data.proto). Do not infer those missing rules from the protocol or claim exact SC2 parity.

## Architecture and ownership

Add a small deterministic movement-policy module in sim_core, owning movement-layer classification, layer-compatible collision admission and domain-specific segment/endpoint validation. It consumes an archetype's authored mover value, actual radius and immutable map; it does not load assets, access the clock, or branch on identity. Keep the public ground-only `SimWorld::is_position_pathable[_from]` APIs documented and compatible for existing embedding callers; internal entity movement must use the explicit policy-aware API.

The normal Ground branch delegates to existing map traversal/occupancy rules without weakening ramps or allowing disconnected endpoints. Air validates the full circle inside the rectangular map using widened raw fixed-point comparisons; a straight segment between valid endpoints remains in this convex domain. No speculative ground path or dummy funnel is needed. Out-of-domain command goals produce a typed rejected movement target, not an implicit clamped coordinate or infinite arrival loop. Client command-status consumption belongs to plan38. Use a distinct movement outcome with the same tick/index correlation envelope; do not overload the production-specific result type. The concrete handoff is specified below.

Cache the resolved policy locally during each entity update. Keep reusable per-layer steering obstacle buffers, rebuilding them in stable EntityId order once per phase. Filter cluster-arrival and local-avoidance inputs by layer. No per-entity allocation, per-frame archetype clone, or temporary per-entity full-world filtering. Extend existing steering helpers with an explicit layer parameter or supply the matching prebuilt slice; choose the smaller API that preserves their standalone tests. Structural footprint obstacles remain in the ground slice only.

The shared spatial index can still store all entities if exact pair admission checks layer compatibility before distance/push computation. Document the filter and ensure broad-phase radius covers both collider radii with a conservative integer envelope, including fractional bucket-boundary cases. Do not copy the current radius-flooring omission. A per-layer index is justified only by measured benefit; avoid an unmeasured storage rewrite. Retain stable pair order and existing same-layer priorities.

## Every displacement path must use the policy

Inventory and replace entity-aware calls in one bounded transaction: normal Move/AttackMove route creation, stale-path rejection, final-target and waypoint arrival snaps, steered movement, X/Y slides, AttackTarget pursuit, hazard escape selection, soft friendly pushes and hard enemy/immovable pushes. Missing any branch permits either grounded Air or cross-domain displacement. Transported/dead/immobile handling remains explicit and unchanged unless tests expose a direct layer violation.

Resolve mover policy and radius from the current catalog at each relevant phase; do not store a second authoritative movement signature. Air clears unused ground funnel waypoints before using its direct goal. Ground revalidates retained waypoint segments against the current ground navigation request and clearance class before reuse, clearing invalid routes and recomputing normally. Expose the existing pathfinder segment predicate through a documented read-only API rather than copying its footprint/knowledge logic. This also handles Air-to-Ground and radius changes without historical policy state. Invalid mutable definitions produce typed diagnostics before displacement; valid mutation to a domain that cannot contain the current actor also produces explicit evidence rather than teleporting it back into bounds. Route vectors remain existing authoritative state and retain their schema3 encoding; new derived workspaces/outcomes do not require a checksum version bump.

Do not alter weapon targetability, vision/detection, flight altitude art or cargo capacity/ownership/lifecycle merely because they mention air. The destination-only teleport/unload amendment below closes their direct position-write boundary without inventing transport mechanics. Their semantics are separate. Existing attack pursuit continues to respect weapon range/visibility while its actual displacement uses the attacker's layer. Rendering remains the existing snapshot boundary; source tests cannot establish the quality of flight visuals.

## CliffJumper and CliffWalker decisions still required

The enum and lore distinguish jumping and walking over cliffs, but the current catalog supplies no maximum level delta, discontinuity span, legal launch/landing footprint, jump duration/cooldown, collision/targetability during transition, or interruption policy. There is also no authored transition animation specification. Inventing generic values in Rust or treating both variants as unconstrained Air would violate data-driven mechanics and conceal absent behavior.

Do not add an unavailable-placeholder command branch or disable these current catalog identities as part of the Ground/Air repair. Keep their existing conservative ground behavior explicitly recorded as an unresolved feature, with tests proving this bounded round does not accidentally grant flight. Before a later complete cliff implementation, approve an authored optional traversal capability with dimensional units and values for each applicable definition, resolve the listed lifecycle rules from authoritative evidence or explicit project design, and write a separate implementation blueprint. If the orchestrator requires all variants in this round, this plan must return to architecture with those authored decisions; it is not implementation-ready for that larger scope.

## Production and client coordination

Completed plan43 owns production egress, supply and queue mutation. Its current exit path already distinguishes Air from conservative Ground, validates the full endpoint footprint, filters live blockers by layer, and retains a ready job/reservation when no legal exit exists. Reuse the classification vocabulary without changing candidate order, producer-boundary connectivity, foundation exemption, occupancy checks, payment or captured ledger. Production egress correctness alone does not establish subsequent flight. Plan38 owns command submission and status consumption; coordinate the typed movement admission handoff below.

No default scenario roster swap or substitute art is authorized. Actual visual verification must select an existing authored Air actor with its own valid media, through the real external scenario/catalog loader. If no valid requested animation exists, report the exact missing asset/action and retain that failure rather than using another unit's imagery. No map-editor work is needed: programmatic authored test maps and separate declarative runtime scenario fixtures suffice.

## Required verification

1. Real external catalog tests dynamically select Ground and Air definitions; no unit-name constants. Prove Air crosses real ground cliffs, ground structure footprints and ramp sidewalls within bounds while Ground remains blocked except at valid ramps.
2. Ground/Air coincident circles do not repel, steer around or prematurely terminate each other's movement; Ground/Ground and Air/Air still respect hold, friendly priority and enemy overlap behavior. Include larger/fractional-radius actors across spatial bucket boundaries.
3. Exercise actual queued Move, AttackMove and AttackTarget orders through full ticks, plus direct final snap, stale waypoint, hazard retreat and both collision displacement paths. No branch may call the wrong movement domain. Air must not use ground obstacle memory to repath.
4. Map edges validate the full radius with exact fixed-point boundary cases; rejected outside targets have typed nonfatal outcomes and no coordinate clamping. Large radii and arithmetic limits produce typed evidence rather than overflow.
5. Catalog policy mutation clears stale navigation or yields the approved typed invariant failure; establish deterministic behavior in tests. Preserve both existing specialized cliff variants' conservative behavior and document the still-missing capability rather than counting those fixtures as feature acceptance.
6. Multi-thousand-tick two-layer replay checks full state/checksum equality, stable simultaneous collision ordering and unchanged ground-only regression results. If new authoritative fields are necessary, bump checksum schema and test exact encoding/mutation sensitivity.
7. Performance evidence measures reusable buffer capacities and actual simulation work for mixed armies; it must not claim a requested simulation time step is measured throughput. Run focused suites, then coordinated workspace tests/strictClippy/fmt without concurrent benchmark contention.
8. Real production capture shows an actually admitted Air actor crossing a visible ground obstruction while a Ground actor remains correctly blocked, then same-layer separation and cross-layer overlap. Inspect the screenshots against actor visibility, clear obstacle context, retained UI and absence of rendering/media diagnostics. Do not accept apparent hovering alone as proof of movement correctness; manifest actor positions/ticks must correlate with the images.

## Approval boundary

Approve the complete Ground/Air lifecycle slice and explicitly retain cliff-specific mechanics as a separately authored follow-up, or supply/approve the missing capability contract before widening implementation. The orchestrator approved the bounded Ground/Air slice with cliff variants explicitly unresolved. Recompute derived movement policy/signatures where possible instead of adding redundant authoritative state, and validate mutable catalog inputs deterministically. Plan43 has priority and shared world.rs/client ownership must be coordinated before any source edit.


## Round7 implementation refresh: concrete site inventory

This section records the source audit performed during the combined workspace freeze; that gate subsequently passed. It supersedes speculative API and stored-signature suggestions above. No product source was changed during this architecture round.

| Site | Required layer behavior and preserved boundary |
| --- | --- |
| `world::process_commands` Move/AttackMove | Validate selected recipients and destination domain before any order mutation; preserve queue versus replacement and combat interruption semantics after acceptance. |
| `step_movement_and_steering` obstacle collection | Build reusable ground/air obstacle slices in stable ID order from living, non-transported actors; invalid live catalog identity/radius is diagnostic, never a default half-radius. |
| Move/AttackMove path creation, known-obstacle revalidation, cluster arrival | Ground retains current ground A*/funnel clearance classes and fog knowledge. Air never asks ground navigation or remembered structures for a route. Only matching-layer actors can settle a cluster. |
| Both arrival snaps, intermediate waypoint snap, steered displacement and both axis slides | Use the mover's actual current policy. Recheck the displacement even when within one tick of arrival. No endpoint-only waiver for continuous ground motion. |
| AttackTarget pursuit and its X/Y slides | Validate the attacker's displacement, including targets on another layer. Preserve authored weapon targetability, visibility, range, windup and turret rules. A target is not an avoidance obstacle when the existing pursuit helper excludes its ID. |
| Hazard evacuation and retaliation flee selection | Add an entity-aware destination query, retaining the current radial/angle search order. Internal call sites pass the actual victim policy/radius. Preserve the legacy public ground-only helper for embedding compatibility. |
| `resolve_collisions_and_push` pair enumeration | Exclude dead/loaded actors, reject different-layer pairs before geometric work, retain stable IDs and same-layer priority/mineral-walk rules. Cover unequal radii in broad phase. |
| Friendly accumulated push and hard accumulated push, all four slide branches | Validate each proposed continuous displacement with the affected actor's policy. Push immunity and existing speed limits remain unchanged. |
| BlinkTeleport preconditions and position commit | Destination-only admission, before energy/cooldown mutation, as specified below. Crossing a cliff by teleport is not walking through its edges. |
| `unload_unit_from_transport` | Destination-only admission before clearing transport state or changing position/orders, as specified below. No cargo workflow invention. |
| `production::production_exit` | Share layer classification/full endpoint primitives only. Preserve43's exact swept egress and its producer-only occupancy exemption; ordinary movement never receives this exemption. |
| `spawn_unit`, `spawn_structure`, `Entity::new` | Preserve43's explicit setup/spawn contracts. `spawn_unit` remains a setup API permitting arbitrary initial placement; layer-invalid live state is diagnosed when actual movement is attempted. Do not silently change fixture/setup placement semantics or supply capture. |
| `steering::resolve_push_interaction` | This entity-only pair helper has no catalog/map and no production callers. Approved49 now intentionally returns a typed Result, prepares both positions atomically, and honors actual immobile/unpushable flags. Callers still enforce layer/domain admission; the world instead uses the checked weighted query and validates resulting displacements. |

Projectile movement, chain-target spatial queries, vision, weapon selection and snapshot altitude are separate concerns and remain unchanged. A search for all authoritative `position =`, `+=` and `-=` sites is part of the implementation review, not merely a check of calls to the old ground predicate.

### Ground footprint boundary and route reuse

This round preserves existing ordinary Ground navigation classes and continuous center-segment terrain connectivity; it does not claim exact circular ground swept-clearance parity with43. Production's conservative swept square is stricter and remains mandatory for egress. Teleport/unload destination admission uses that same conservative full-footprint endpoint rule because a direct placement has no preceding navigation proof. This deliberate distinction must remain visible in docs/tests; improving ordinary ground swept clearance is a separately reviewed change if required.

For route reuse, promote the existing `Pathfinder::segment_is_passable` logic to a documented public read-only query accepting the existing `PathRequestWithVision` plus start/end, or an equivalent narrow wrapper. It must use exactly the existing static/dynamic clearance, known-structure and diagonal policy. Validate every retained segment with the current request before consuming it. No new radius classes, hidden-obstacle revelations or production exemptions are allowed. Record the added sim_map API as an explicit scope amendment. An Air actor has no ground route to retain, so its waypoint vector is cleared before direct motion.

### Destination-only teleport and unload admission amendment

Use one typed read-only destination preparation function accepting the actor, current catalog/map and requested world position. It returns distinct ordinary rejection reasons versus malformed live-definition/state errors. Definition checks include a positive radius and representable full-radius bounds. Full circle bounds use i128 raw coordinates; Air ignores ground terrain and foundations. Ground uses43's stationary conservative square footprint: inspect every intersected cell, terrain edge and actual structure occupancy, with strict tangency exclusion and **no** foundation exemption. Extract/reuse the existing proven geometry rather than create sampled placement checks.

Both layers test actual living, non-transported same-layer circular blockers, excluding the actor itself. Use the overflow-safe exact circle overlap convention already present in43: tangency is legal. Check authoritative occupancy at execution time. Do not silently choose a nearest-free point, alter the requested coordinate, grant a cross-layer blocker, or require teleport's intervening segment to be ground-walkable. No SC2 nearest-placement parity is asserted.

Blink retains existing ability index, life, channeling, energy, cooldown and authored range preconditions. Run the typed destination preparation before payment; a rejected destination leaves energy, cooldown vector, position, velocity, route, order queue, target and animation unchanged at the immediate API boundary. The existing `execute_ability -> bool` compatibility method can consume the typed preparation result, recording corruption through `SimulationDiagnostic::MovementInvariant`; ordinary invalid placement remains false. Expose the typed destination validation API so embedding callers can obtain the specific rejection without changing all unrelated ability result types. Queued ability processing retains its existing order-consumption behavior; this round does not claim transactional redesign of every spell command.

Add `try_unload_unit_from_transport(id, position) -> Result<(), DisplacementAdmissionError>`; retain the existing bool wrapper delegating to it. Require an existing living loaded actor, then prepare the exact requested endpoint and commit the existing unload fields once. On any failure all entity fields, cargo flag, supply/production ledger, IDs and events remain unchanged. Successful unload clears obsolete route state as well as the existing order/velocity reset, preventing a stale transport-era path from surviving. No guessed transport owner/capacity rules are added to the current standalone load/unload API.

### Movement command outcome handoff to38

Proposed concrete public types, subject to review of this amendment:

```rust
pub struct MovementOutcome {
    pub tick: Tick,
    pub command_index: usize,
    pub player: PlayerId,
    pub result: Result<MovementReceipt, MovementAdmissionError>,
}
pub struct MovementReceipt {
    pub recipient_count: usize,
}
```

The result is **one per Move/AttackMove command**, not one per recipient. `recipient_count` counts unique recipients; duplicates never append the same queued order twice. Validate the entire selection in command order first: valid player, nonempty selection, each ID exists/alive/owned, current movement definition is valid, and the requested destination lies inside each actor's full-radius map domain. Reject the whole command on the first failure; do not partially interrupt weapons, replace queues or append orders. Immobile/holding state is not mislabeled successful displacement: acceptance records scheduled intent under existing order rules, not arrival or an invented move capability. A selected actor's later death or changed public catalog state is handled at execution, not by a fictional receipt guarantee.

Terrain reachability and unknown obstacles are not command-domain rejection: keep current fog-aware navigation behavior rather than revealing hidden structures during admission. Ordinary rejection includes invalid player/selection/recipient/domain; missing live archetype, invalid radius or unrepresentable definition is an invariant error and also emits the existing fatal diagnostic channel. The receipt has no renderer fields or product identity. AttackTarget retains its existing command contract; only its physical pursuit changes in45, and it gains no fabricated arrival/success result.

Expose `movement_outcomes()` and capacity-preserving `drain_movement_outcomes()`. Retain reusable storage bounded in count by Move/AttackMove commands processed since the last authoritative step clear, with no history accumulation across ticks; direct `process_commands` callers must drain between batches just like43. Clear beside43's outcomes/diagnostics before the new tick's phases. Preserve the current incremented execution tick and exact original `TickBundle.commands` index, including interleaved Produce and unrelated commands. Derived outcomes/workspaces do not affect checksums or future decisions.

38 drains this distinct buffer immediately after **each** `step_into`, before another catch-up iteration clears it. It uses the same retained local command-index→submission-sequence mapping and session generation as43; never correlate by selection, position or buffer order. Ordinary rejection remains visible/nonfatal, invariant diagnostics halt catch-up with completed-tick evidence retained.38 has confirmed the proposed shape fits its design. Serialize the single clear hook and client adapter handoff after38's method extraction;45 otherwise avoids `step_into`, snapshot writer and UI edits. No placeholder client branch is needed before45's implementation exists.

### Arithmetic and allocation evidence

Keep shared `SpatialHashGrid::query_radius` behavior unchanged in this bounded round; its misleading `r_ceil` currently floors, so movement callers must pass an explicitly ceiled, map-bounded positive reach calculated in i128 from the querying radius plus the maximum live radius in its collision layer. Production already does this. Do not use `2 * radius_a`, which misses a larger neighbor. Test both entity-ID orders, fractional positions straddling a bucket boundary, unequal radii and extreme valid map-bounded radii. Exact pair overlap/normalization must avoid overflowing fixed-point squared distance or summed radii: use widened raw comparisons and checked representable local distance/displacement conversion, with typed evidence if a subsequent steering arithmetic operation is not representable. Do not silently truncate or wrap.

Reusable obstacle/outcome buffers are a design property, not proof of globally zero allocations. Current bucket vectors start with capacity8 and can grow on first crowd concentration; collision maps and existing ground path outputs also allocate. Measure new buffers after explicit population/concentration warmup and assert capacity retention for the tested workload. Report initial growth and any remaining existing hot allocations honestly. Do not repeat steering.rs's existing blanket zero-allocation documentation without correcting its qualification in the owned module. No full simulation throughput claim follows from a capacity assertion.

### Expanded end-to-end acceptance matrix

- Drive Move and AttackMove through actual `TickBundle` commands for both layers, then AttackTarget pursuit against same- and cross-layer enemies with valid authored weapons. Confirm routes, final positions and unchanged ground ramp-side rejection using existing ramp/fog suites.
- Test atomic mixed-selection rejection with unequal radii, duplicate IDs, queued and replacing orders, dead/foreign/missing recipients and invalid player. Compare pre/post order queues, weapon phases and positions for every recipient; interleave Produce and MovementOutcome and prove exact indices.
- Exercise two catch-up ticks through38's sole tick owner: outcomes survive both drains, match submission sequence/generation, ordinary rejection does not pause, fatal invariant preserves preceding evidence, and reset discards stale-generation notices. A no-tick frame emits no authoritative result.
- Force both snap branches, both steering slides, both pursuit slides, friendly and hard pushes at domain edges; demonstrate Air crossing a real cliff/placed structure while Ground remains terrain-constrained. Include dead/loaded actors as nonblockers and both specialized cliff variants as conservative Ground.
- Trigger actual hazard damage and retaliation on an Air victim, proving the selected flee route crosses ground-only obstacles but respects its own full-radius boundary; preserve hold/immobile behavior and positive-HP lifecycle guards from43.
- Execute real Blink and unload: Ground rejects blocked full footprints even when center cell is clear; Air can land over ground obstacles; both reject outside-radius bounds and living same-layer overlap, accept exact tangency and ignore opposite-layer occupancy. Immediate rejection preserves all fields/payment; no nearest-point relocation. Test public typed API plus legacy wrapper.
- Mutate a live archetype Ground→Air, Air→Ground, and increase/decrease radius while a route exists. Verify fresh current-policy checks without a stored signature; invalid/current-domain-incompatible mutation emits deterministic diagnostics with no invented reposition. Public direct order injection receives the same movement-domain checks before displacement even though it has no command index for an outcome.
- Train actual Air and Ground products using43 admission/countdown/completion, then issue movement after spawn. Assert collision layer, blocked-exit retry, captured supply/queued reservation transfer, death/reap and schema3 replay stay intact. A mover change must not reprice captured supply or release reservations.
- Run matching mixed-layer worlds for several thousand ticks with formation congestion, production, damage/death and catalog-policy mutation; compare full checksum and both `step`/`step_into` snapshots where38 is present. Derived drains must not change the checksum.
- Keep real capture acceptance separate from mechanics: use authentic available actor media, real external catalogs/scenario, actual UI/pipelines and manifest-correlated obstacle crossing. A missing required image/action is a typed failure and cannot be replaced by fake art. Screenshot quality and mechanism proof both remain required.

### Review/implementation sequence

1. Parent reviews this completed amendment, including the narrow sim_map route query, command-level atomic outcome policy, and teleport/unload endpoint admissions.
2. 43 combined gate acceptance is recorded above; wait for explicit45 amendment approval/release. Coordinate38's step/snapshot extraction and its buffer-clear/client-drain handoff before either edits those methods.
3. Implement policy/geometry extraction and isolated admission tests first, preserving43's golden exit/replay fixtures. Implement movement/steering/push/flee call sites coherently; no temporary unchecked displacement path is accepted as completion.
4. Run focused simulation/route/production tests and strict targeted lint; then coordinated client outcome integration and native capture. Freeze all sources for workspace tests, workspace strict Clippy and workspace formatting.
5. Report bounded Ground/Air acceptance, remaining ordinary-ground footprint distinction, unresolved cliff mechanics and measured allocation limits explicitly. A failed workspace gate or rejected screenshot remains failed evidence, not a completed round.


### Implementation review: cluster self-exclusion

Root approved the narrow arrival correction: the querying actor cannot count as its own settled blocking neighbor. Keep `check_cluster_arrival` compatible and add an explicit excluded-EntityId helper for authoritative movement. World callers provide their own ID and the already layer-filtered obstacle slice. The actual Air Move/AttackMove-from-rest test must reach the requested endpoint without its own zero-velocity entry terminating the order. This does not change the policy for distinct same-layer settled neighbors.


### Approved full-radius stutter-step fixture migration

The full45/49 sim_core run `/tmp/space-scuffle-r7-movement-full-sim.log` failed at `stutter_step_tests::test_stutter_step_backswing_cancellation`: its requested x=0 cannot contain the selected actor's positive radius, so the newly explicit command rejection correctly leaves Backswing unchanged. Root approved a narrow fixture migration: use that actor's exact catalog collision-radius x boundary at the original y, retain all damage/cancellation/three-tick westward movement assertions, and additionally assert the actual MovementOutcome is accepted. No production domain rule or combat timing is relaxed. Preserve the failed log, rerun this fixture, then the full simulation suite and strict checks.
