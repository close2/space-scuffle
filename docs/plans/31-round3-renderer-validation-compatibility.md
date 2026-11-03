# Round 3: validation-clean Vulkan rendering on the locked client stack

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: approved staged implementation; isolated compiler/native-surface evidence passed, workspace integration and native validation in progress.

## Goal and acceptance boundary

Run the actual battle, Visual Lab, terrain preview and mist capture shaders on the available RADV Vulkan adapter with validation enabled, preserve their intended rendering, and obtain no shader-module or presentation synchronization errors. Screenshots must still show the actual scene and pass visual review. An exit code of zero or a populated pipeline cache is insufficient when the validation layer reports errors.

This round owns renderer compatibility and its regression harness. It does not change the map editor, maps, simulation, unit content, gameplay, or art to evade validation. Shared rendering APIs remain compatible. Existing dirty work is preserved.

## Evidence and upstream analysis

Baseline: `/tmp/space-scuffle-round1-terrain-native.log`, native Radeon 890M/RADV STRIX1/Mesa 26.2.4. Locked dependencies are Bevy 0.15.3, wgpu/core/hal 23.0.1, Naga 23.1.0. `spirv-val` and `spirv-dis` are installed. Software Vulkan reproduces compiler diagnostics, so the initial evidence does not support a hardware-only explanation.

Three defects must be separated:

1. Relaxed atomic operations carry memory-class bits. Installed Naga `src/back/spv/helpers.rs:74` returns `UNIFORM_MEMORY` or `WORKGROUP_MEMORY` without acquire/release ordering. Upstream [wgpu PR 8391](https://github.com/gfx-rs/wgpu/pull/8391/files) fixes the semantics by retaining the appropriate scope while using empty relaxed semantics. This is an actual code-generation repair, not a logging change. Barrier semantics in `writer.rs` are separate and must retain acquire/release ordering.
2. The installed writer unconditionally adds `ArrayStride` to arrays (`writer.rs:966`) and member offsets/matrix stride to structures (`decorate_struct_member`). The reported SSAO variable is a real `var<workgroup>` nested 8x8 float array from Bevy's `preprocess_depth.wgsl`. [wgpu issue 7696](https://github.com/gfx-rs/wgpu/issues/7696) remains open; [current upstream writer](https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/naga/src/back/spv/writer.rs) still has unconditional array decoration. Current callback suppression is not a compiler repair. Vulkan's [SPIR-V environment rules](https://docs.vulkan.org/spec/latest/appendices/spirvenv.html) govern which storage classes may use explicit layout. A version bump alone cannot be accepted as a fix.
3. Locked HAL `SwapchainImageSemaphores::get_present_wait_semaphores` returns only submit-produced semaphores. If no submission uses the acquired image, that list is empty and the acquire semaphore remains unconsumed. Bevy's renderer presents every acquired window texture after its graph, including startup frames without a ready camera render. This is a strong source-supported explanation for the local errors, requiring reproduction before being declared the local root cause. Upstream [presentation rework 9361](https://github.com/gfx-rs/wgpu/pull/9361/files) adds real present preparation and changes public APIs; it is not a small drop-in HAL patch. [Bevy issue 20592](https://github.com/bevyengine/bevy/issues/20592) reports matching symptoms but does not establish which exact local branch fails.

## Chosen strategy and alternatives

Keep the public Bevy/wgpu stack pinned for this round. Apply a reproducible, reviewed Naga compiler backport and enforce valid surface initialization in the application's render graph. Do not rewrite SSAO into scalar variables or disable SSAO/GPU preprocessing to hide compiler defects. Do not suppress VUIDs, reduce validation flags, swap in dummy shaders, or use offscreen-only captures to claim native presentation correctness.

A wholesale engine migration is not the preferred round: it changes material, ECS, input and egui APIs, risks forcing editor source changes, and still does not prove the outstanding explicit-layout issue solved. If a focused compiler correction cannot meet the tests below, retain the failure evidence and request a newly scoped orchestrator blueprint for a migration; do not silently broaden this implementation.

## Reproducible dependency ownership

Use a local, source-controlled patch of Naga 23.1.0 under `third_party/naga`, wired by `[patch.crates-io]` and a locked dependency update. Preserve upstream licenses and exact source provenance. A `third_party/README.md` records release checksum, upstream commits, local patch files, regression commands, and the condition for removing the fork. Keep upstream source untouched except reviewed changes; distinguish inherited upstream comments from project-authored code. Never edit the global Cargo registry cache.

Compiler changes are private implementation details, not a new runtime API. Verify `cargo tree -i naga` resolves the patched version for the actual Bevy renderer and that only one intended Naga version serves wgpu. Do not add a second unrelated compiler merely for a passing test.

Ownership: this round controls Cargo.toml/Cargo.lock patch entries, third-party compatibility source and provenance, a new `crates/client/src/render_compatibility.rs`, its narrow plugin registration, compatibility fixtures/tests, and validation evidence/scripts. Coordinate root Cargo edits and client plugin hunks. No other agent should update dependencies during this round.

## Compiler implementation blueprint

### Relaxed atomics

Backport the final accepted semantics of PR 8391, not its abandoned acquire/release proposal. Audit all users of `AddressSpace::to_spirv_semantics_and_scope`: atomic load/store/read-modify-write/compare-exchange paths must use relaxed semantics with correct scope; failed CAS semantics must also be relaxed. Do not change workgroup/storage barriers. Record the upstream commit actually applied and any adaptation to Naga 23 APIs.

### Explicit-layout type lowering

The fix must represent explicit buffer layout separately from logical/private/workgroup value layout. A single Naga type handle can occur in both host-visible storage and workgroup storage. Removing `ArrayStride` globally is wrong because it corrupts real uniform/storage buffer layout; removing it only from the one SSAO shader is also insufficient.

Introduce an internal layout class in SPIR-V type lookup: explicit interface layout versus logical layout. Build recursive type variants for composite arrays/structures and pointer pointees. Explicit interface arrays retain stride; explicit structures retain offsets, matrix stride/order and required block decorations. Logical variants have no forbidden explicit decorations. Scalars/vectors and otherwise layout-independent types can share IDs.

Route global Uniform/Storage pointers through explicit interface types. Workgroup globals, private variables, function locals, function parameters/results, and ordinary composite values use logical types. Pointers produced by access chains must preserve the storage class and pointee layout. When loading/storing composite values across the interface boundary, explicitly convert between structurally equivalent interface/logical composites using extract/construct operations; scalar/vector accesses need no conversion. Runtime arrays remain interface-only and cannot be copied as whole values. Constants, zero initialization and nested arrays must use their expected variant IDs.

Keep instruction ordering and type ID assignment deterministic. Document the type-class invariant at the writer and lookup boundary. This is the riskiest part of the round; do not accept a patch that handles workgroup-only arrays but still emits invalid code when a type is shared with a storage buffer. The first checkpoint is a standalone compiler fixture suite before compiling the application.

## Native presentation lifecycle

First reproduce the acquire-with-no-render branch with the actual window/render backend: cold-start delayed assets, a temporarily inactive camera, and a screenshot request before pipelines become ready. Capture validation logs and correlate affected frames with graph execution. A basic smoke render alone does not test this branch.

Implement a root render-graph node before Bevy's `CameraDriverLabel` that initializes each actually acquired window surface using the configured `ClearColor` and a real color-attachment clear pass. `ExtractedWindows::swap_chain_texture` owns the actual acquired surface. Create its view from `surface.texture` using the same original format contract as Bevy (`texture.format().add_srgb_suffix()`). Do not use `swap_chain_texture_view`: Bevy replaces that view with an offscreen capture texture during screenshots. Initializing that replacement would leave the real surface uninitialized and would not repair the present hazard. The node uses the existing `RenderContext` encoder, causing the same submission to acquire/wait, transition, initialize and ultimately present the real surface even when scene pipelines are pending. Normal camera/egui passes run afterward and retain their full rendering. This is a surface initialization invariant, not a replacement visual or dummy shader.

Skip windows without an acquired surface. A view belongs to each acquired surface lifetime; its creation is a documented GPU resource-lifecycle cost, distinct from avoidable per-frame scratch collections or material allocation. No substitute texture is allocated, and no stale view may survive resize/reacquisition. The node performs no device-wide idle wait, blocking poll, CPU pixel upload or per-frame scratch-map allocation. Render graph ordering ensures initialization cannot overwrite final scene output or screenshots. Verify screenshots are copied after camera output, not after only the initial clear. Keep the same clear color as the existing application.

A real standalone wgpu23/RADV/X11 probe has now reproduced this branch: six acquire→present frames without submission emit MissingAcquireWait,01430,WRITE_AFTER_PRESENT and01286; adding a real attachment clear and submission removes all validation errors. Evidence is `/tmp/space-scuffle-r3-surface-unrendered.log` and `/tmp/space-scuffle-r3-surface-initialized.log`.

Installed Bevy source additionally explains the screenshot trigger: CameraDriver considers a camera-targeted window handled, while screenshot capture substitutes an offscreen camera target. `submit_screenshot_commands` uses an asynchronous screenshot-to-screen pipeline to write the actual surface; when that pipeline is pending, the actual surface can receive no submission. Add a native regression that requests the first screenshot before that pipeline is ready, verifies acquisition/presentation remain validation-clean, then checks the screenshot callback's pixel/readback is the final real scene rather than the initial clear. A successful standalone surface probe is evidence for the lifecycle invariant, not a replacement for this production regression.

If synchronization errors persist after this branch is repaired, independently trace semaphore lifetime/recreation and resize/minimize behavior. Do not add arbitrary wait-idle calls. A further HAL/core backport requires a precise root-cause amendment; PR 9361 spans public APIs and cannot be partially transplanted by guessing.

## Tests and failure contracts

Compiler tests invoke the actual patched Naga WGSL frontend/validator/SPIR-V backend, then execute installed `spirv-val --relax-block-layout --target-env vulkan1.3`. Missing validator is an explicit test-environment error, not a skip that reports success. Emit disassembly on failure.

Required fixtures:

- nested workgroup arrays matching the real SSAO declaration, indexed across a barrier;
- one nested composite type used simultaneously in storage and workgroup memory, with scalar and whole-composite transfers in both directions;
- nested struct/array/matrix uniform and storage blocks retaining exact host-visible offsets and strides;
- function arguments/results and private locals containing arrays/structures;
- workgroup and storage atomics: load, store, add, exchange, compare-exchange success/failure;
- workgroup/storage barriers whose acquire/release semantics remain unchanged;
- zero-initialized workgroup arrays and dynamic storage arrays;
- extraction/construct correctness checked by GPU readback for a mixed-layout compute workload, not merely disassembly string checks.

The actual Bevy SSAO, GPU preprocessing, terrain, bubble, combat and mist shaders must compile and draw under validation after the focused tests. Add a native-window lifecycle probe exercising zero scene submissions, delayed shader readiness, normal rendering, screenshot callbacks, resize, minimize/restore, and clean close. Real surface tests are distinct from headless scheduler tests. The clear-only startup frames may exist while loading but may never be accepted as final visual artifacts.

A verification wrapper records command, adapter/driver, validation configuration, application exit, captured files, and all diagnostic lines. A nonzero process exit, timeout, pipeline error, missing/blank capture, or Vulkan validation error fails the run. Do not filter out the known baseline errors. Ordinary audio-device availability messages are recorded separately and do not establish GPU failure.

## Staged verification and completion

1. Establish unchanged locked-stack failing fixtures and exact native lifecycle reproduction.
2. Apply atomic/type-layout correction in isolated patched compiler; pass the full matrix and upstream relevant compiler tests.
3. Integrate the patch, rebuild actual binaries, compare lock/dependency resolution.
4. Add the actual surface initialization node; reproduce cold-start/resize/no-camera cases with validation enabled.
5. Capture actual battle startup, visible combat, tab switches/reset, terrain overview/ramps, mist motion and paused fading. Evaluate composition and effects against their own goals; compatibility must not degrade images or hide objects.
6. Run `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check` under a coordinated code freeze. Include patched-dependency tests explicitly because workspace commands may exclude vendored dependency tests.
7. Repeat actual native captures after gates with the same binaries. Store full logs and a concise report listing each baseline VUID and the evidence that it no longer reproduces.

Completion requires every defect class addressed with passing real evidence. If only atomics are repaired, report partial compatibility progress and keep the round open; neither an upstream bug label nor a successful screenshot excuses validation errors.

## Amendment: renderer shutdown ownership

A captured runtime-test core establishes a second lifecycle defect: the test process exits while an async pipeline worker still calls Vulkan. `RenderAppChannels::drop` waits for the render sub-app to return but does not join its independently scheduled pipeline tasks. The manual-App integration fixture must recover the render sub-app via the public channel API, finish every `CachedPipelineState::Creating` task through `PipelineCache::process_queue`, wait submitted device work, then drop normally. Three separate real test processes must exit cleanly. Assertions passing before SIGABRT are not success.

The same potential race exists in Winit early shutdown. Prefer a public render schedule hook over another dependency fork: an ExtractSchedule system observes (without consuming, clearing or replacing) the main world's `Events<AppExit>` and marks the render world's final frame. A shutdown system ordered after `RenderSet::PostCleanup` drains actual pipeline workers and GPU submissions on that frame. Bevy's locked schedule runs main systems before extraction; Winit checks `App::should_exit` after `App::update`; its world destruction waits for the dispatched render frame. Thus all original success/error exit reasons remain intact, and final-render shutdown happens before native resource teardown. No ordinary frame blocks on shader compilation.

The worker drain keeps PipelineCache/layout/device resources alive, polls actual task state and cooperatively yields. Pending shader imports that have never started a worker are not a shutdown dependency. Use an explicit deadline diagnostic; a deadline must not be treated as safe completion or justify leaking a task/device. The orchestrator-approved strategy records/logs a typed deadline failure but continues retaining resources until actual workers join. A shared status survives the Winit runner; its wrapper converts an otherwise successful AppExit to failure after normal teardown while preserving any original nonzero reason. External test watchdogs may terminate a driver that never returns and must mark that run failed. The manual fixture follows the same safe join rule and returns its error only after workers finish.

Regression coverage must exercise actual early success and error AppExit during pipeline compilation, confirm original reasons, and verify clean process exit repeatedly; successful readiness-only captures do not cover this branch. Preserve normal window close, capture failure and frame-limit behavior. No exit-event interception or fixed sleeps are permitted.

### Native close amendment: window-owned egui context

Actual resize/minimize/restore succeeds, but the documented graceful `xdotool windowquit` (WM_DELETE_WINDOW) causes exit101: `lab_ui_system` calls `EguiContexts::ctx_mut` after the primary window and its context are removed. This is a genuine ordinary-close lifecycle failure. The earlier `windowclose` command destroys the X window directly and produces SurfaceLost; retain that diagnostic separately rather than claiming it was ordinary close.

Proposed narrow correction: window UI and viewport selection use `try_ctx_mut`; when the window-owned context is absent, there is no interactive surface to draw into or select through, so that system returns for the closing frame. This is lifecycle handling, not an alternative UI, simulated context, or suppression of a shader/asset failure. Do not change input directions/ownership here. Extend the actual production-plugin integration fixture to remove the primary window, execute the real closing schedule and verify ordinary AppExit without panic, then orderly join the renderer. Repeat native resize/minimize/restore plus WM_DELETE_WINDOW with actual screenshots and clean process exit. Input policy remains separately approved plan36 after the workspace gate.
