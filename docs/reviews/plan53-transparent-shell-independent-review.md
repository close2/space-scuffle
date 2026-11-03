# Independent review: transparent-shell exact-depth ties

Status: read-only architectural review during the round10 workspace gate. No product edits, builds, phase observers or native runs. The reported normal/sparse Swarm difference (2,450 pixels, maximum channel difference153) is root's retained image evidence; this review does not independently reproduce it. Plan52 remains held and51 remains frozen.

## Pinned implementation evidence

The proposed diagnosis is plausible from the locked renderer, but source inspection alone does not prove it caused the retained pixels:

- `bevy_render-0.15.3/src/view/visibility/mod.rs::check_visibility` uses `visible_aabb_query.par_iter_mut()`, appends visible entities to thread-local queues, then drains those queues into each view's `VisibleEntities` list. This does not establish a canonical logical-actor sequence.
- `bevy_pbr-0.15.3/src/material.rs::queue_material_meshes` iterates the corresponding `RenderVisibleEntities` list and appends each material's items into `Transparent3d` with distance from `ViewRangefinder3d` plus material depth bias.
- `ViewRangefinder3d::distance_translation` computes inverse-view matrix row2 dotted with world translation. It is **view-space Z**, not Euclidean distance from the camera. With the current no-yaw RTS camera, same-worldY/Z actors can have identical depth despite differentX positions. The Swarm rows share height andZ; the two opposed columns overlap near the center.
- `bevy_core_pipeline-0.15.3/src/core_3d/mod.rs::Transparent3d::sort` calls `radsort::sort_by_key(items, |item| item.distance)`. Locked radsort0.1.1 is stable and keys only the float's radix representation. Equal keys retain input order.
- `mist/upload.rs::aligned_queue_system` establishes mist queue → selected-packet depth alignment → bubble queue. This preserves35's material-class shell-over-wake contract at exact ties, but does not canonicalize multiple shells within the bubble queue.

The engine's radix float representation distinguishes negative and positive zero. A correction that groups ties using ordinary `f32 ==` would merge keys that the current sorter treats separately. Prefer exact finite distance-bit groups, or explicitly justify matching the exact radix equivalence. Do not introduce an epsilon, distance bias, approximate row grouping or geometric perturbation.

## Evidence needed before calling the pixel cause proven

Record actual queued and post-sort items for the affected production Swarm view, correlated to the exact capture/frame. Include render and main entity identities, stable logical actor identity/domain, actual translation, distance bits, material asset identity, pipeline/draw function, and material/texture state needed to establish the same image input. Preserve actual view matrix/viewport, shader-facing time and profile identities; owner labels/colors alone are not complete renderer input equivalence.

Confirm which overlapping shell pairs have exact equal depth bits and whether their order changes between the retained normal/sparse/repeated cases. If those pairs do not explain the changed pixels, retain the failed hypothesis rather than introducing ordering changes as an assumed fix. Two opposing camera views and an unequal-depth control are useful to separate geometric depth from a tie rule.

Observe after all relevant queueing and after the actual engine sort. If inspecting before sorted batching, call it phase-order evidence; if claiming submitted draw order, also account for batching/batch ranges and actual render traversal. The current shells use separately created material instances, but do not assume batching behavior from that fact alone.

Plan35 already demonstrated an observer-effect hazard: adding a diagnostic system changed a formerly unordered material tie outcome. Preserve the original artifacts and use repeated captures from one preserved executable with observation enabled/disabled as appropriate. An observer can expose a consistent order without proving the unobserved historical run used it. Do not relabel historical images with an order never measured.

## Stable logical identity and ownership independence

Any new shell tie key should identify the presented instance, not its color or allocation history. Live snapshots already carry deterministic EntityId; Lab and Swarm use explicit preview instance identities. Domain/scene generation may be needed to distinguish simultaneously eligible namespaces or reset ownership. Specify the actual supported namespaces and duplicate rules rather than assuming every BubbleInstance belongs to a live SimWorld.

The key must not depend on PlayerId, palette slot, RGB, unit slug, material asset ID, render/main-world Bevy Entity allocation, or MistGpuIdentity pool slot. Those values can change under sparse-owner mapping, reallocation, reset or emitter reassignment without changing the intended logical actor. A missing or duplicate logical identity cannot silently fall back to raw Bevy entity order. Define typed evidence/failure for malformed owned shell identity and ensure ordinary real scene/spawn paths supply the key.

Existing producer inventory includes `scene_controller` Lab/live/Swarm creation, standalone real-media mist capture, and CPU fixtures. Preserve user restrictions: do not turn this into editor development, simulation/schema changes or identity-specific asset logic. Render identity is derived presentation data; authoritative checksum encoding remains unchanged.

## Ordering scope and renderer schedule

Retain the engine's exact geometric order for unequal distances and35's selected-packet mist depth/mist-before-shell behavior. A tie correction must run before sorted phase batching rewrites batch/instance ranges. The locked `SortedRenderPhasePlugin` installs GPU and non-GPU batching in `RenderSet::PrepareResources`; an after-sort/before-batching hook should name this exact relationship. Never reorder already packed batches without rebuilding their matching instance data.

Be explicit about unrelated transparency. Sorting shell occupants across noncontiguous shell slots leaves unrelated item indices unchanged, but can move an individual shell across an intervening unrelated draw and change their blend relationship. Therefore fixed indices alone do not prove unchanged unrelated compositing. One narrow policy is sorting contiguous shell-only subruns inside exact-depth runs; actual material queueing normally yields a shell block. If a wider permutation is needed, document and test its mixed-material contract rather than claiming it changes only shell-versus-shell ties.

Whichever contract is chosen, tests should include interleaved unrelated items, mist items, opposite cameras, exact equal depths, adjacent unequal float depths and signed zero. Preserve every item's distance bits, pipeline/draw function and entity association. Missing identity and duplicate-key tests must use real owned-shell classification, not a generic sorter that could silently skip unrecognized shell draws.

Do not claim order-independent transparency or physically correct intersecting shells. A deterministic tie policy makes an existing approximation reproducible; it does not resolve transparency intersections. Native overlap images must still be inspected against the intended appearance.

## Cost and acceptance boundaries

Avoid allocating per-frame maps/vectors merely to canonicalize the phase. If scratch capacity is retained, describe growth boundaries honestly. Do not claim whole-engine allocation freedom or linear complexity when sorting a tie group is O(k log k). A structured PERFORMANCE RATIONALE should tie the extra work to the observed crowd case; no arbitrary unit-count shortcut should disable correctness.

Before product release, root must approve the complete53 blueprint and evidence plan. Required verification includes actual phase proof, meaningful mixed-item/identity tests, repeated preserved-binary native captures with normal/sparse owner mappings, preservation of35 shell/wake behavior and unequal-depth controls, then coordinated workspace tests/strict Clippy/format. The current source gate remains untouched. This review authorizes no implementation or new compositor.

## Review of the completed contiguous-queue blueprint

The completed `53-round10-stable-transparent-shell-ties.md` selects a narrower
pre-sort policy: canonicalize each contiguous shell-only queued block after the
actual shell material queue and before the existing engine PhaseSort. This differs
from the possible post-sort policy discussed above. It needs no custom floating
comparator or tie grouping. Every shell retains its side of every intervening
unrelated queued item; stable engine sorting then preserves that relationship
for equal radix keys, while unequal keys follow the unchanged geometric sort.
Moving complete phase items before batching preserves their draw associations.
This resolves the mixed-material relationship concern without claiming to
canonicalize shell identities across separate blocks.

Two implementation obligations remain explicit review checks. First, missing,
duplicate and stale logical identities must be validated over the entire view,
even when permutation is performed independently within several blocks. Checking
duplicates only inside each block would miss an ambiguous identity separated by
an unrelated draw. Second, typed render-world errors must reach the existing
ordinary visible-error / automated fail-fast policy; an isolated RenderApp log
is not sufficient evidence that the capture or ordinary game rejected malformed
presentation state.

No additional mathematical contradiction was found in this narrowed blueprint.
Stage A must still establish the causal phase-order evidence before the proposed
correction is described as fixing the retained historical pixel difference.
This review does not approve implementation or relax the ongoing source freeze.
