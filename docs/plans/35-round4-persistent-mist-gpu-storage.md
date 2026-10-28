# Round 4 — Persistent mist GPU storage and measured crowd cost

Status: approved by the orchestrator; implementation follows the coordinated gate and the smaller plan 37 combat-shadow correction.

## Goal and evidence

Keep the approved directional-history semantics while eliminating per-frame dynamic Mesh extraction and geometry rebuilding. Demonstrate actual 400-emitter runtime cost, bounded upload memory, correct transparent sorting/culling, and visual equivalence of the production shader. This round does not declare the currently rejected mist art acceptable.

Locked Bevy 0.15.3 `render_asset.rs:248` clones each modified main/render-world asset. Every visible ribbon currently calls `Assets<Mesh>::get_mut`, causing cloned attribute/index vectors in extraction. `mesh/allocator.rs::allocate_meshes` then processes and uploads those extracted meshes. Stable main-world Vec capacity and asset IDs therefore do **not** prove an allocation-free render path. The CPU writer also recalculates every vertex five times, once per attribute; moving deformation into the vertex shader removes those repeated trigonometric/normal calculations.

Root's real `/tmp/space-scuffle-mist-v3-crowd-independent/Crowd-1-fps60-msaa4.png` proves a submitted 400-owner load without capacity overflow and with a clean corrected renderer log. It does not prove 60 measured frames/second: the CLI's fps is a virtual timestep. Bottom actors are clipped, and CPU `visible=400` means nonempty ribbon histories, not 400 visible frustum intersections. Mist remains visually unreadable in that image.

## Scope

Own `client/src/mist/{mod.rs,emission.rs,ribbon.rs,material.rs,capture.rs}`, new `mist/gpu.rs`, new `mist/upload.rs`, production mist WGSL, and focused GPU-layout/upload/lifecycle tests. Preserve the approved optical shader, renderer compiler repair, runtime maps/capture builder, and all map-editor files. Add only the narrow render-plugin wiring needed by the existing `MistWakePlugin`; coordinate any shared lib/test imports with the orchestrator.

Do not upgrade Bevy/wgpu, disable renderer validation, replace actual assets, use CPU-rendered screenshots, or migrate to a different effect while claiming equivalence. Preserve the real depth fade, non-shadow-caster state, premultiplied blending, actual scene-light response, and existing stop/turn/generation semantics.

## Architecture

### Immutable geometry and explicit identity

Use immutable topology whose vertices encode `(sample_index, edge_side, strand_index, emitter_slot)`. Allocate one small immutable slot mesh per pool slot at initialization, all sharing one material and shader. The slot index is a documented integer vertex attribute, never hidden in position/transform bits. Indices and vertex attributes never change during normal play, resets or camera movement; no mesh `Modified` events are emitted after warmup.

This preserves Bevy's per-entity transparent depth sorting and frustum culling without introducing an unreviewed custom batching path. Locked Bevy 0.15.3 does **not** expose the current online example's `MeshTag/get_tag` API; do not copy that newer example or smuggle identity into the transform. Compared with one shared mesh and a fully custom instanced draw command, static slot meshes retain the mature engine sorting/lifecycle path at a bounded one-time memory cost. Report the exact initial mesh bytes and draw calls. If measured draw submission fails the performance target, stop acceptance and amend this blueprint for custom sorted instancing; do not conceal that cost or combine all trails into one unsorted mesh.

The vertex shader reads its slot and neighboring history samples from fixed storage buffers, computes tangent, camera-facing side, coherent layered displacement, width/taper, normal, UVs and coverage, then emits the same production fragment inputs. Use the actual view uniform, so multiple cameras receive appropriate facing geometry. CPU code computes only conservative bounds from retained samples and maximal configured displacement; it does not reconstruct every vertex. Preserve separate render entities and `NotShadowCaster`. The packet emitter origin remains the geometric sorting anchor; see the reviewed-coordinate amendment below. Identity transforms are not suitable for the locked queue implementation.

### Packed bounded state

Define documented aligned POD layouts shared by Rust/WGSL:

- A 64-byte emitter header: history start/count/generation/enabled flags; origin/radius; conveyor offset/boost; sample-base/capacity and reserved explicitly initialized padding.
- A 32-byte sample: world position and relative age at upload; cumulative path distance and initialized padding.
- An 80-byte shared frame block: the existing four 16-byte color/light/time lanes plus a 16-byte geometry lane containing lifetime, width and initialized reserved values. This block is copied atomically with headers/samples into a persistent storage binding, not republished through a newly prepared material every frame. CPU publication metadata also retains a monotonically checked packet sequence. See the time-coordinate amendment below.

Use the authored `max_emitters` and `samples_per_emitter` to validate checked multiplication, binding sizes, vertex indices and adapter storage limits before allocation. For the current 512×48 budget, sample storage is 786,432 bytes and headers 32,768 bytes, totaling 819,200 bytes of emitter/sample state plus the 80-byte shared frame block: 819,280 bytes per complete packet. At 60 actual updates/second, a full upload is 49,156,800 bytes/second; these are derived budgets, not measured performance. CPU extraction uses preallocated flat arrays and `copy_from_slice`/bounded loops; ordinary `ExtractResource` cloning of a Vec-backed resource is forbidden here.

Keep owner generations explicit. When a slot is reused, reset its history and publish the new generation together with its state. An old submitted packet must never display a previous owner's trail on a replacement entity. Use a publication acknowledgment protocol: hide a reassigned slot until the render upload has submitted the matching generation. A preallocated shared publication record holds one generation and last-submitted AABB per slot behind a short-lived mutex; no Vec cloning occurs when reading or updating it. The upload node publishes acknowledgments only after its copy commands have been submitted, so later draws observe the same queue ordering. Main-world synchronization exposes a replacement only when its owner generation equals the acknowledged generation, and unions pending bounds with the last published bounds. An acknowledgment for an obsolete generation cannot expose a newer owner. Test transitions with deliberately delayed submission and repeated reassignment; do not assume entity extraction and uploads always coincide.

### Persistent device buffers and bounded transfers

Bind actual persistent raw GPU buffers using locked `AsBindGroup`'s documented `#[storage(..., read_only, buffer)]` support. Do not use `ShaderStorageBuffer::set_data` on every frame if its RenderAsset path creates/replaces GPU buffers. Allocate the device storage once per validated capacity, record creation counters/bytes, and retain handles across scene resets.

Use a bounded ring of staging buffers, initially three slots sized for a full packet, with explicit states:

`MappedWritable -> CopyRecorded -> SubmittedAwaitingMap -> MappedWritable`.

Write into an existing mapped range, drop the range, unmap, and record `copy_buffer_to_buffer` before any view draws. Integrate a real render-graph upload node before the camera driver; use its existing command encoder. Only request remapping after submission, and handle map errors explicitly through typed diagnostics. Callbacks update preallocated slot state; no per-frame growing queues or allocation of new staging buffers when the GPU is late. Configure/validate the ring size rather than relying on an unbounded staging-belt growth policy.

If no staging slot is writable, retain the last fully published visual packet and increment a reported upload-stall counter. Do not publish a mixture of new headers and old samples. Replacement entities remain hidden until their matching generation is published. Bounds must include the last published geometry as well as the current pending state, so delayed uploads cannot clip an otherwise valid old trail. A configurable bounded readiness deadline turns persistent map/submission failure into a typed error. A healthy measured run must have zero sustained stalls; transient backpressure is visible telemetry, not a successful performance claim.

The mapped CPU packet arrays, GPU target buffers and staging-ring capacities are all fixed after initialization. Wgpu/driver command submission and callback internals may still allocate; instrument and report that boundary honestly rather than asserting the entire engine is allocation-free. Avoid `Queue::write_buffer` as the final bounded transfer implementation because its temporary staging allocation is hidden from this pool's budget. Wgpu's documented staging-belt API demonstrates the submit/remap order, but its automatic chunk growth is not a bounded-memory guarantee.

## Verification round

### CPU/packing correctness

- Reuse all 30/60/144 Hz emission, teleport, stopped, disable/re-enable, removal, 400-owner replacement and capacity tests.
- Assert every GPU field offset/stride and total byte budget, including maximum configured capacities and overflow/adapter-limit rejection. Test malformed counts/slot indices/generation mismatches without out-of-bounds access.
- Compare CPU reference geometry with GPU-computed positions/normals/ages through actual GPU readback on straight, turning, stopping and camera-rotation cases. The old pure CPU geometry calculation may remain a test-only oracle; production must not retain its per-frame Mesh writes.
- Test the staging state machine under delayed completions, out-of-order callback delivery, map errors, scene reset and rapid generation reuse. No buffer/count growth or old-owner rendering is permitted.
- Inspect actual post-warmup asset events and extracted mesh assets: zero ribbon Mesh modifications/extractions across movement, camera changes and reset. Verify static topology handles and persistent device-buffer identities survive 400-owner replacements.

### Actual GPU and visual equivalence

Use the corrected renderer compatibility plugin, real assets and production shader pipeline. Require readiness and clean validation logs; do not skip a failing adapter. Capture equal-time close/gameplay/turn/stop/boost scenes against the last recorded art version. Compare screenshots at native resolution and evaluate depth/alpha/sorting, intersections and silhouette quality; CPU/GPU vertex agreement alone does not prove the composite is right.

For 400 actors, fit the camera to **all actual actor bounds plus full wake bounds** using the actual projection and viewport aspect. Assert those projected bounds lie inside the viewport with a documented margin and that each submitted actor is inside a camera frustum. Capture moving opposite directions, overlapped trails, terrain slope/cliff intersections and a scene reset. Keep the difficult pale-ground case and add contrasting actual ground values. A counter named `visible` must distinguish active history, submitted draw, frustum inclusion and evaluated image coverage.

### Measured performance and allocation boundary

Add a benchmark mode distinct from deterministic visual capture. Use real elapsed time for frame measurements and a deterministic independent trajectory clock; log requested simulation rate separately from measured frame rate. Disable screenshot readback during the timed interval and capture only after recording.

Run release builds on the actual adapter, record GPU/driver/backend/resolution/MSAA/presentation mode, and compare 0/100/400 moving emitters over at least ten seconds after shader/asset warmup. Record wall-frame median/p95/p99, CPU emission/extraction/upload/submission timing, actual GPU timestamps where supported, draw/vertex counts, bytes submitted per second, staging stalls, mesh extraction events, owned buffer creation count/bytes, and resident memory trend. When timestamp queries are unavailable, state that clearly and report CPU/wall measurements without pretending they are GPU time.

The orchestrator confirmed that these three cases retain the same **400 moving real catalog actors**, terrain, fitted camera and graphics settings, enabling mist on only the first 0, 100 or 400 actors. This isolates incremental mist cost from changing bubble/media draw cost. Optional total-actor scaling may explain a broader frame bottleneck, but cannot replace the fixed-scene comparison.

The shared machine may run unrelated CPU-heavy work. Do not stop or alter those processes. Record system load at each measurement boundary, retain identical instrumentation and GPU-query availability, and alternate/repeat baseline and replacement runs when contention changes. Report measured conditions and variability; a faster sample under lower contention is not sufficient evidence for a rendering improvement or a 60 Hz pass.

The orchestrator approved an instrumentation boundary clarification: report directly measured main mist-update/geometry time, independent wall-frame time, the locked renderer's actual CPU/GPU pass diagnostics, and public extracted-mesh event counts/bytes. Finer private engine extraction/upload CPU timings are unavailable through this version's public systems and must be explicitly marked unavailable. Do not label schedule delay as extraction time or fork the renderer solely to create a metric. External profiling may be separately planned if the measured evidence leaves a relevant bottleneck unresolved.

Use bounded histograms or preallocated sample arrays during measurement; never allocate one heap record per frame. Keep benchmark counters out of simulation determinism. Compare the unmodified dynamic-Mesh baseline with the new path on the same device/scene/settings. A 60 Hz target requires p95 wall-frame time no greater than 16.67 ms at the specified 400-unit settings; distinguish overall rendering cost from incremental mist cost. If the whole game baseline already misses the target, report both rather than calling a faster but still failing result accepted.

Acceptance requires no per-frame mesh cloning, stable owned memory after warmup, zero recurring upload stalls, correct ownership/culling, clean actual shader validation, and independently accepted visual evidence. Run coordinated workspace tests, strict all-target Clippy and formatting checks after integration. Store commands, logs, hardware details and capture evaluations under `docs/reviews/`.

## Primary implementation references

- Locked Bevy 0.15.3 `render_asset.rs`, `mesh/allocator.rs`, `render_resource/bind_group.rs` and its mesh/view vertex interfaces are authoritative for this workspace.
- [Wgpu buffer documentation](https://wgpu.rs/doc/wgpu/struct.Buffer.html) identifies temporary queue staging and custom mapped-buffer management.
- [Wgpu StagingBelt lifecycle](https://docs.rs/wgpu/latest/wgpu/util/struct.StagingBelt.html) explains write/finish/submit/recall ordering. Use locked 23.0.1 APIs, not newer convenience methods, and do not assume the belt enforces a memory cap.
- [Bevy storage-buffer example](https://bevy.org/examples/shaders/storage-buffer/) shows the concept but uses newer APIs and per-frame Vec construction; neither is acceptable to copy unchanged into this implementation.

## Approval clarifications and implementation order

The orchestrator approved this blueprint after the coordinated gate, with these mandatory clarifications:

1. **Preserve the measured baseline before replacement.** First add the bounded benchmark recorder/camera fit to the existing dynamic-mesh path, then run release 0/100/400 cases with actual wall timing and hardware metadata. Retain the executable and logs under `/tmp` plus reproducible command/source identification in the review. Only afterward replace dynamic geometry. Both implementations must use identical trajectories, graphics settings, camera fit and timed intervals. Do not lose the baseline by overwriting its only binary.
2. **Coordinate spaces are explicit.** GPU samples store world-space positions. Emitter headers store a world-space conveyor offset, emitter origin and rendered radius in world units. The custom vertex shader computes final world-space positions directly and calls the locked world-to-clip helper once; it does not apply the mesh world matrix again. The earlier identity-transform proposal is superseded: use the actual acknowledged emitter origin as a geometric transform/sort anchor, and express the conservative world AABB locally by subtracting that anchor. Preserve the baseline origin-based sorting policy, rather than changing to AABB-center sorting. The render-phase adjustment below makes the distance correspond to the packet actually selected for this frame, including stalls. Test translated, elevated, turning and camera-rotated paths against the CPU oracle, including nonzero conveyor offsets.
3. **New slot data must never be drawn by stale owners.** Reassignment hides the pool render entity before visibility propagation; the extracted upload generation and extracted visibility must come from the same immutable extraction epoch. An upload of a different owner generation is forbidden until the render world confirms that the previous-generation slot is absent from **all** queued view draws for that epoch. The upload node checks this invariant before recording the copy; a violation reports a typed publication-order error instead of overwriting the shared slot. The replacement is exposed only after matching post-submit acknowledgment returns to the main world. Test deliberately delayed visibility extraction, a delayed upload and two rapid owner replacements with real GPU captures/readback; do not rely solely on ordinary schedule order.
4. **Owned resource counters are not driver allocation counters.** Report persistent target buffers, staging ring, CPU packet capacity and immutable mesh assets separately from wgpu/backend allocations and resident-memory observations. An unchanged owned-buffer counter is insufficient to claim that the driver made no allocations.

### Approved baseline warmup amendment

Before timing, require both at least three actual wall seconds after pipeline readiness and enough virtual trajectory frames to exceed the authored history lifetime plus one maximum emission interval. A slow renderer can otherwise spend three real seconds while its history is still filling. Convert validated durations to a checked integer frame requirement (round upward), reject invalid/overflowing inputs, and test the exact boundary and overflow paths. Record actual warmup wall duration, virtual frames and required frames in the report. This amendment changes benchmark readiness only; it does not change emission, geometry or art.

### Approved baseline provenance amendment

After the measured interval ends, aggregate actual fixture actor identities/actions and copy their already-admitted `ClientMediaStatus` audit records into the report. Include actor counts and exact PNG/metadata digests, with the truthful provenance status. Reject missing audit evidence; do not reread or rehash media during timing. Before the release build, snapshot all source/Cargo inputs and runtime assets, including dirty/untracked files, into an isolated directory with a SHA-256 inventory. Exclude credentials, tool state, Git internals, generated captures and build outputs. Verify the inventory after copying; build and run against that immutable snapshot with explicit asset root. Preserve the final executable hash. This permits unrelated approved source work while the isolated release build runs without invalidating comparison evidence.

### Approved independent vertical camera support

The fixed crowd trajectory's horizontal excursion must not be reused as vertical support. Derive horizontal bounds from grid origins plus horizontal motion and the larger actor/wake support. Derive the upper Y bound from maximum actual terrain elevation plus maximum actor center height (the authored sphere radius) plus the larger actor/wake support; derive the lower Y bound conservatively from minimum terrain elevation minus that support. Test larger authored radii for which the previous mixed-axis bound underestimates the top. Preserve actual per-image actor/wake frustum rejection after this correction.


### Approved packet-aligned transparent ordering and time

The orchestrator reviewed the locked public queue/phase/rangefinder APIs and approved this amendment. Replacement implementation remains held until the baseline has actually been measured. The baseline snapshot does not contain this replacement.

**Locked engine evidence.** `bevy_pbr-0.15.3/src/material.rs::queue_material_meshes`, around line 859, computes `Transparent3d.distance` from `mesh_instance.translation` plus material depth bias, not from the AABB center. `RenderMeshInstances` exposes the same translation to queueing whether mesh uniforms are built on CPU or GPU. Current `mist/mod.rs` sets the transform to `slot.origin`, and CPU ribbon vertices are local `world_position - pose.origin`. Therefore origin-zero transforms would collapse meaningful sorting, while switching to a bounds center would silently change the baseline's policy.

**Geometry, culling and sorting contract.** Keep the acknowledged packet's real emitter origin as the main-world ribbon transform translation (rotation identity, scale one); it is not an encoding channel. The main-world AABB is the union of retained published support and pending support, translated into local coordinates by subtracting this anchor. For a missing/new generation, keep the entity hidden until matching submission acknowledgment. The vertex shader deliberately reconstructs world geometry directly from storage; it applies only the view projection afterward. This separation is explicit: the transform anchors engine culling/sorting metadata, while the custom shader owns world deformation. Never both add the origin in the vertex shader and multiply by the mesh world transform.

**Freeze the rendered packet before queueing.** Reserve a writable staging slot and select either the next complete packet or the retained published packet after extraction, before `RenderSet::Queue`. This selection is immutable for that render frame. Staging becoming writable later does not change the packet after distances have been computed. A failed invariant or copy preparation fails the frame explicitly rather than drawing distances for a packet that was not submitted.

The main-world acknowledgment can be one render frame behind the packet selected for a fresh upload. Therefore add a bounded render-phase correction after `queue_material_meshes::<MistWakeMaterial>` and before `sort_phase_system::<Transparent3d>`. Iterate existing per-view phase items without creating new vectors; identify mist items through an explicitly extracted slot/generation component. For each item, compute the locked view rangefinder distance from the selected packet's emitter origin, retaining the material's existing depth-bias contract (zero for this material). Other transparent materials remain untouched. This preserves baseline sorting on fresh packets and uses the old packet's origin when upload backpressure retains old geometry. It also handles each camera's own rangefinder independently.

The upload graph node before the camera driver must copy exactly that selected packet. Before copying a changed generation, confirm the old generation is absent from every queued view draw, as already required above. After the actual `render_system` submission, publish packet sequence, generations, origins and bounds together and start staging remapping. Callback completion affects next-frame slot availability only. Queue order protects prior submitted draws from a later copy into persistent buffers.

**Time is part of the packet.** Each sample stores its age in seconds evaluated from its owning emitter's local history clock at extraction. All samples/header/frame data come from the same extraction epoch. The frame block stores the exact visual clock used by the existing shader (`MistWakeUniformGpu.warm.w`) and the geometry parameters; the CPU publication record identifies the packet sequence. The GPU computes normalized age solely from the published age and published lifetime. It does not add the current Bevy/material clock to a relative age, and it does not integrate elapsed time a second time.

When a staging slot is unavailable, retain geometry, sample ages, style/light values and visual noise time from the same previously published packet. This deliberately freezes one coherent presentation packet during bounded backpressure; it does not mix old positions with advancing taper or a newly generated owner's data. The next submitted packet carries current authoritative presentation-history ages, including any stop fade that elapsed meanwhile. A healthy accepted run must have no recurring stalls; prolonged lag remains a typed failure. Reset/reassignment resets the relevant local history clock and increments ownership generation while leaving the global visual clock semantics unchanged. A retired old-generation packet cannot become visible on a replacement. No mutable per-frame material publication is used to sneak a second clock around this protocol.

The added 80-byte frame block is a third persistent device buffer (beside headers and samples), with the same bounded staging packet and post-submit acknowledgment. Report these three device buffers and all three staging allocations separately from engine/driver allocations. Copy offsets/sizes are checked and aligned; any adapter binding limit failure is explicit.

**Required verification.** Use actual per-view queued phase distances and real GPU readback/captures for translated, overlapping trails with different origins, including camera azimuth reversal and two simultaneous views. Match the baseline emitter-origin distance policy on unstalled frames. Deliberately retain an old packet while the main-world owner moves: its queued distance, geometry age, noise time and culling support must all remain those of the selected packet. Then release staging and verify they switch together. Exercise stopping, full fade, reset and rapid generation reuse across that delay. Compare positions/normals/age attributes to the CPU oracle at the exact published packet clock. Test zero/one/multiple retained packets, maximum ages, and float conversion tolerances explicitly; screenshots must reject ordering flips or stale-owner visuals even if numerical field tests pass.

The main culling union is an intentional conservative overestimate; its center is not a new sorting policy. A future volume/art renderer may need different transparency handling, but that belongs to plan 44 and cannot be substituted into this equivalence round.

### Approved bounded window-negotiation amendment

The first actual isolated release baseline exits 1 on the recorder's generic settings guard: `/tmp/space-scuffle-r6-mist-baseline-0.log`. No performance result is accepted. Earlier native probe evidence shows a transient larger first-frame physical window followed by 1280×720, but the generic benchmark error did not report observed values, so resolution negotiation is a hypothesis rather than a proven cause of this particular failure.

Allow a physical-resolution mismatch only while the recorder remains in pre-readiness `Loading`, with the existing 180-second wall deadline still checked on every observation. Correct physical 1280×720, actual configured presentation mode and camera MSAA are mandatory before entering warmup. A presentation/MSAA mismatch is always fatal; any resolution mismatch after entering warmup or measurement is fatal. Errors must report the phase and actual observed resolution, presentation mode and MSAA alongside expected settings. Test loading wait, expiration, exact transition, and post-readiness rejection. No automatic resizing, wrong-size reporting, fallback renderer or art change is introduced.

Preserve the failed snapshot and executable. Create a new identified derivative from the exact frozen baseline tree plus only the reviewed recorder file/test amendment, and emit an explicit old/new file-digest diff linked to the original inventory. Do not copy concurrently edited live simulation/environment modules. Apply the same recorder correction to the live source, then run focused checks and the release build against the derivative. The first real run must still establish its actual settings and complete all artifacts before acceptance.

The recorded `presentation_mode` is the actual configured Bevy Window policy, not a verified backend swapchain mode. The locked renderer keeps its surface configuration private; wgpu selects a supported fallback for `AutoNoVsync`. Record resolved swapchain mode as unavailable without independent evidence. Do not infer Immediate/Mailbox behavior from this policy or fork the renderer to expose it.

### Equal-distance material ordering correction proposed after first GPU capture

The first real AMD Close capture completed with clean validation, and the actual production geometry function passes GPU readback against the retained CPU oracle on both AMD and software Vulkan. Nevertheless, overlap pixels differ from the preserved baseline: in middle Close, all 1,208 pixels differing by more than three channel values are at x >= 700 (the shell/wake intersection); the entire free wake to the left differs by at most one channel value. New overlap pixels are substantially brighter. This is evidence for a material-order regression, not permission to change opacity or accept the comparison.

The locked Bevy `Transparent3d::sort` is a stable radix sort keyed only by `distance`. Mist and its owner's shell have precisely the same emitter-origin sort anchor and zero material depth bias. Thus otherwise unordered material queue systems determine the tie through insertion order; adding a dependency after mist queueing can change that order without changing either distance. Preserve the baseline's shell-over-wake appearance with an explicit schedule chain: `queue_material_meshes<MistWakeMaterial> -> align_transparent_packet -> queue_material_meshes<BubbleDomeMaterial>`, all before `PhaseSort`. This changes no geometric anchor, introduces no artificial distance epsilon, and affects only exact distance ties. All unequal distances retain Bevy's back-to-front sort. Shared materials remain generic, independent of unit identity.

Before acceptance, log actual phase insertion distances and material identities for a controlled overlapping fixture, verify the scheduling chain with a regression, then repeat actual Close captures and quantify overlap and free-wake differences separately. Extend the required delayed-packet/two-camera tests to assert the same tie contract. If the hypothesis is falsified, retain the failure evidence and continue diagnosis; do not mask the discrepancy with art changes.

The diagnostic derivation is preserved at `tmp/verification/mist-baseline-r8-phase-probe`, with a verified399-entry inventory and exactly one changed source file (`capture.rs`). Its executable hash is `ae8ebfb67064f67a77d4b4b535db782a7e94ccf934a9b1d61055677e579fc193`. All three observed baseline images are byte-for-byte equal to the preserved baseline. Sixteen actual sorted phases show mist at index0 followed by shell at index1, with identical distance bits `c0e7a11e`.

The same live executable (`50484d29c0e4eb33bb16a822fc15db7f93ca1da1ca2c40c1bbc52c097aa08614`) with observation enabled also shows mist-before-shell and matches the baseline within one channel value. With observation disabled, that exact executable reproduces the large overlap error while its entire free wake remains pixel-identical to the observed run. Therefore observer registration changes the formerly unordered tie outcome; we did not directly measure the bad run's phase order, and must not pretend that we did. Root approved the explicit queue chain based on these controlled results. Acceptance requires the corrected executable to match with observation both disabled and enabled. This is a scheduling correction; shader/style remain unchanged.

Transfer timing now explicitly covers application-owned extraction/packet packing, staging selection/mapped copy, render-graph copy-command recording, and post-submission acknowledgment as separate bounded distributions. These are CPU durations, not GPU upload timing or private engine phases. The benchmark records actual mist mesh Modified events, actual extraction events, owned target/staging buffer creations and bytes, submitted packet bytes, and stalls. It rejects new buffer creation or geometry mutation/extraction during the timed interval. Driver/internal map-view allocation behavior is not represented by these application counters.

### Actual400-owner reset and culling capture amendment (review before implementation)

Add `--verify-owner-reset` to the existing real capture tool, accepted only with nonbenchmark `Crowd`; reject every other combination before creating the app. Default captures and all measured benchmarks remain unchanged. This fixture uses the same400 admitted catalog actors, media/mesh/material handles, initial radius choices, terrain, camera and world trajectories as ordinary Crowd. It does not replace actors with geometry proxies or choose different media.

Retain the three-image callback/completion contract. Image0 captures the ordinary frame100 (60Hz) state. After its actual callback completes, presentation time is already frozen for readback. On the first subsequent Update with image index1 and no pending screenshot, an exactly-once reset system despawns all400 actor entities and recreates them with the same current transforms, catalog instances, real mesh/material handles, and immutable fixture origin/index. It deliberately does not copy `AssignedEmitter`; the production pool retires and reassigns ownership. Deferred commands must be applied at Update end before the production PostUpdate mist synchronization, transform propagation, visibility/culling and render extraction. Time delta stays zero during this transition: new histories start empty at a defined local epoch, and the first reset image must show the identical actor poses with no leaked old wake. The tool must not silently advance until an attractive later frame.

Image1 is requested on that first reset frame after the main-world pool has assigned all400 new owners, advanced their checked slot generations, and reports empty new histories. Its actual render frame publishes the reset packet while hidden empty ribbons cannot expose prior generations. Keep time frozen until this screenshot callback completes. Then resume the ordinary exact virtual cadence and capture image2 after forty additional60Hz-equivalent frames (2/3 second), with new moving histories visible. Record actual elapsed frame/time for each image rather than claiming ordinary Crowd sample timestamps. Existing bounded wall/readiness and complete-callback failure policies apply; reset must execute exactly once and never trigger again during readback.

Add an opt-in bounded queue audit owned by the capture fixture, initialized before measurement (this mode cannot benchmark). The real `align_transparent_packet` path records selected packet sequence, selected versus retained state, current view IDs, and each actually queued slot/generation after real material queueing and frustum visibility filtering. No test-side approximate visibility replaces that evidence. Every queued identity must already satisfy the production selected-generation check. Capture audit records include main-frame ownership/generation state and the render sequence they refer to, so asynchronous prior-render observations are not mislabeled as same-frame state. Retain a bounded per-render-frame audit throughout reset and settle, including zero-queue frames; overflow or missing sequence evidence fails verification. Inspect the first reset packet, the first actual new-generation queued draws, and settled draws, not only three stills.

Store exact before/after actor entity IDs, slot generations, topology mesh IDs, material handles, and owned target/staging buffer IDs/counts. Require all400 actor identities to change, no extra topology/material/buffer allocations, no budget overflow, and all actual actor plus published/pending wake bounds to pass the existing frustum check. Audit data must prove that retired generations never reappear in queued draws. Unit tests verify option validation and exactly-once transition timing; real native PNGs, actual queue audit and recorded resource identities establish owner-reset/culling behavior. This amendment is a verification fixture for the approved storage lifecycle, not an art change or simulation feature.

### Amendment: baseline material tie nondeterminism and a comparable corrected baseline

The expanded actual matrix invalidated the earlier inference that the original dynamic path had one stable shell/wake tie policy. Close matched the corrected storage result, while Turn, early Stop, Gameplay and Crowd had substantial shell-overlap differences. Free-wake Turn/Stop pixels left of x700 differed by at most one channel. Root observed the original dynamic Turn/Stop phases as shell then mist at exactly equal distance bits `c0e7a11e`; their diagnostic PNGs exactly matched the original references. More decisively, a repeated Close run of the **same preserved diagnostic executable** changed from the previously observed mist→shell order to shell→mist. Its three images changed 1,714/1,208/1,621 pixels by more than three channel values, with maxima127/131/133, while free-wake pixels remained byte-identical. Evidence: `/tmp/space-scuffle-r8-mist-phase-repeat02/same-binary-comparison.json`.

Classify this as an existing nondeterministic equal-depth material-ordering defect. The original screenshots remain truthful historical artifacts but cannot all define one stable rendering contract. Preserve the approved generic policy: mist material queueing precedes bubble material queueing for exactly equal distances, retaining creature readability; unequal-distance sorting remains entirely geometric. The policy is independent of capture case, unit identity, movement direction, opacity and material values. It does not solve transparent intersections or constitute visual-art acceptance.

Before treating overlap differences as storage regressions or improvements, create a **new identified dynamic baseline derivative from the immutable r6b snapshot**, not from live product sources or the observer derivative. Change only its `crates/client/src/mist/capture.rs` to configure a dependency between the already registered system type sets:

```rust
use bevy::ecs::schedule::IntoSystemSet;
app.sub_app_mut(RenderApp).configure_sets(
    Render,
    bevy::pbr::queue_material_meshes::<super::MistWakeMaterial>
        .into_system_set()
        .before(bevy::pbr::queue_material_meshes::<BubbleDomeMaterial>.into_system_set()),
);
```

Apply this configuration after the existing material plugins have been added. Do not register either queue function a second time, insert a diagnostic observer, alter vertex/fragment shaders or geometry, change material data, or add a numeric sort bias. Locked Bevy exposes these type sets and rejects ambiguous multiple instances. The existing storage path already has the equivalent stronger chain mist queue→packet alignment→bubble queue, so this amendment requires no product ordering change.

Record every derivative file/symlink hash using the baseline schema, the original inventory hash, and an explicit old/new content diff proving **only capture.rs** changed. Preserve the original r6b and diagnostic binaries. Build the derivative in the isolated target cache, hash/copy its executable, and run it against its own exact asset root. First inspect its real Close and Turn captures against the corrected storage path, without an observer. Repeat the same executable to establish stable images. Then use the complete aligned 60 Hz cases (Close/Boost/Turn/Stop/Stationary/Gameplay/Crowd) and the existing 30/144 Hz timing cases. Compare both free-wake and shell-overlap pixels with small numerical tolerance, actual frustum evidence and actual screenshots; report any remaining discrepancy explicitly.

The original six release measurements remain the original dynamic implementation's measured performance evidence. A new paired comparison must state whether it uses that original baseline or this corrected-order derivative. Since the corrected policy can affect overdraw and therefore GPU cost, obtain matched corrected dynamic and storage 0/100/400 runs, forward and reverse, before attributing GPU/wall-time changes to storage. Keep identical recorder settings, actual media and native adapter, and record source/executable identities. The old main-world geometry cost remains useful evidence but does not remove the need for matched corrected rendering. No benchmark or actual matrix is marked passed by the architecture amendment alone.

This amendment is submitted for orchestrator review before creating the derivative. The independently approved owner-reset fixture may continue without modifying ordering or art.

#### Reviewed API correction after actual startup rejection

The first ordered derivative compiled but actual startup exited101 before any PNG: `/tmp/space-scuffle-r8-mist-visual-ordered-baseline/close.log`. Locked Bevy rejects configuring a function's `SystemTypeSet` directly (`schedule/config.rs::SystemSetConfig::new`, assertion `configuring system type sets is not allowed`). Therefore the direct `configure_sets` example above is a **rejected API proposal**, not an executable accepted recipe. Preserve this failed derivative/inventory and executable unchanged.

Use a named empty set as an ordering bridge instead. Configure that ordinary set after the existing mist queue function and before the existing bubble queue function:

```rust
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StableMistShellQueueOrder;

app.sub_app_mut(RenderApp).configure_sets(
    Render,
    StableMistShellQueueOrder
        .after(bevy::pbr::queue_material_meshes::<super::MistWakeMaterial>)
        .before(bevy::pbr::queue_material_meshes::<BubbleDomeMaterial>),
);
```

No extra system belongs to this set. Read-only inspection of locked `bevy_ecs-0.15.4/src/schedule/schedule.rs::get_dependency_flattened` (lines1264–1312) confirms that empty sets explicitly connect every incoming dependency to every outgoing dependency before removal. Thus this bridge yields mist→bubble ordering while neither configuring a system type set nor registering another queue instance. Both existing functions remain registered exactly once by their material plugins. No runtime barrier, queue work, altered depth key or capture-case branch is introduced.

Before building the next actual derivative, run a CPU schedule regression against the locked Bevy version: register two test systems once in reverse declaration order, configure the empty bridge with `.after(first).before(second)`, execute repeatedly and assert exact first→second order and one call each. Separately inspect the actual material queue system sets and the bridge dependency edges without running synthetic materials as visual evidence. This tests dependency flattening; the real native material startup and full captures remain mandatory.

After orchestrator review, derive a fresh snapshot directly from immutable r6b with only this replacement declaration/configuration in `capture.rs`, no observer and no other change. Use the same hash-diff/build/native acceptance protocol. The failed prior derivative remains separate historical evidence. Product storage's existing valid mist→alignment→bubble chain is unchanged.
