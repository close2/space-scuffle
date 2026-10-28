# Isolated renderer compiler compatibility checkpoint

The approved blueprint is `docs/plans/31-round3-renderer-validation-compatibility.md`. The isolated checkpoint under `/tmp/space-scuffle-render-compatibility` is now integrated into the root workspace after orchestrator approval; production validation remains in progress.

## Provenance

- Naga 23.1.0 published crate SHA-256: `364f94bc34f61332abebe8cad6f6cd82a5b65cff22c828d05d0968911462ca4f`.
- Published `.cargo_vcs_info.json` identifies upstream commit `9993f5c9eceb91f9bb6b9c5c6994f35f50e5f378`, also the peeled `naga-v23.1.0`/`v23.1.0` tag.
- The comparison corpus comes from the same upstream commit's `naga/tests/in/*.wgsl`.
- Atomic correction follows the accepted behavior of [wgpu PR 8391](https://github.com/gfx-rs/wgpu/pull/8391), merge commit `7a2afeb014cb30a976059717c4ae7c85f01d858e`: relaxed atomic semantics with the existing storage/workgroup scopes. This is an adapted backport, not a claim of an identical full cherry-pick.
- Logical/interface type separation is a local compiler correction. It is not represented as an upstream-approved fix.

The isolated directory contains exact source copies, fixture inputs, baseline and corrected SPIR-V/disassembly/validator output, `naga23-vulkan-layout-and-atomics.patch`, and `evidence-sha256.txt`. Upstream source warnings are retained, not silenced or repaired incidentally.

## Regression results

Unpatched Naga reproduced both actual application VUID classes: 10871 for workgroup/storage atomics, and 10684 for a nested workgroup array and a composite shared between storage and workgroup memory. The atomic-only backport fixed 10871 while preserving the layout failures. Control-barrier semantics remain workgroup 264 and storage 72 in both disassemblies.

The combined correction separates logical values from buffer-interface composite types. Host-visible offsets/strides remain attached to the interface variants; workgroup/private/function values use undecorated logical variants. Scalar/vector/matrix identities are shared. Runtime-array structures are interface-only and retain a valid Block type. Loads and stores convert sized composites recursively with extract/construct instructions; out-of-bounds guarded loads retain the same physical result type before conversion.

Four focused fixtures pass `spirv-val --relax-block-layout --target-env vulkan1.3` for both SPIR-V 1.0 and 1.5: atomics, nested workgroup arrays, shared buffer/workgroup composites, and uniform/storage mat3/mat4 layout. Actual wgpu23 Vulkan execution on RADV verifies all 48 output scalars bit-for-bit after storage→workgroup→private/function→storage transfers. This is real GPU readback, not only a textual shader test.

Comparing all 87 upstream WGSL fixtures under identical default Naga options: unpatched 57 pass, corrected 64 pass, zero previously passing fixtures regress. Seven atomic/workgroup fixtures improve. The remaining 23 failures remain explicitly recorded; this corpus includes unsupported/default-configuration fixtures and is not presented as an entirely passing upstream test suite. An intermediate implementation caused five runtime-array-structure regressions; these were repaired by preserving interface-only Block types and are not skipped.

The patched Naga library's 60 unit tests pass with `wgsl-in,spv-out` enabled. Relevant logs:

- `/tmp/space-scuffle-r3-compiler-baseline.log`
- `/tmp/space-scuffle-r3-compiler-atomic-patched.log`
- `/tmp/space-scuffle-r3-compiler-layout-final.log`
- `/tmp/space-scuffle-r3-compiler-layout-15.log`
- `/tmp/space-scuffle-r3-compiler-corpus-baseline.log`
- `/tmp/space-scuffle-r3-compiler-corpus-patched.log`
- `/tmp/space-scuffle-r3-compiler-unit.log`

## Explicitly retained unrelated edge

`uniform_mat2_edge.wgsl` contains uniform arrays of mat2x2. Both exact unpatched Naga and the correction fail strict Vulkan extended-uniform-layout validation because emitted matrix stride 8 does not satisfy alignment 16 without an additional feature contract. `/tmp/space-scuffle-r3-compiler-baseline-expanded.log` and the corresponding SPIR-V retain this evidence. No validator relaxation or device feature was enabled to hide it. The new supported uniform fixture uses mat3/mat4; the mat2 fixture is preserved as a distinct known limitation, not deleted or claimed fixed. Any repair of that pre-existing uniform feature/layout contract requires separate scope review.

## Remaining acceptance gates

The isolated checkpoint originally gated dependency integration. Integration, real compiler/readback checks, production runtime fixture tests, native screenshot validation and verified window lifecycle checks are now complete as detailed below. The coordinated full workspace test/clippy/fmt gate is running on frozen source; its result remains required before round completion. Runtime composition/art quality and input/UI correctness remain independent reviewed rounds; compiler correctness cannot certify them.

## Native presentation isolation

The real native surface probe at `/tmp/space-scuffle-render-compatibility/src/bin/surface_probe.rs` ran six frames on the same RADV adapter and X11 surface. Without a render submission, it emitted 17 validation diagnostics spanning MissingAcquireWait,01430,WRITE_AFTER_PRESENT and01286. With an actual attachment clear and submission before each present, the same protocol emitted zero validation diagnostics. Logs are `/tmp/space-scuffle-r3-surface-unrendered.log` and `/tmp/space-scuffle-r3-surface-initialized.log`. Both processes exited normally, demonstrating again why exit status alone cannot validate rendering.

Bevy0.15 source review identified the screenshot-specific distinction: `swap_chain_texture_view` may be an offscreen replacement; `swap_chain_texture.texture` is the actual acquired surface. The screenshot-to-screen pipeline can still be pending after camera rendering has targeted the replacement. Blueprint31 now requires initialization of the actual surface and a production screenshot-pipeline-pending regression. The production node now initializes `swap_chain_texture.texture` before CameraDriver in the client and both native preview paths. Its final native validation remains a required acceptance gate.

## Manual runtime fixture teardown

The coordinated workspace test gate reported all nine runtime assertions passed, then the test process aborted. The preserved core for PID 1460892 shows the main thread in native process teardown while an AsyncCompute worker still calls `PipelineCache::start_create_render_pipeline` → `RenderDevice::create_pipeline_layout` → Vulkan validation. This is a teardown race, not a passing test.

The manual production-App fixture now takes ownership of the real render sub-app through Bevy 0.15 public `RenderAppChannels::recv`, closes its scheduling channel, processes all in-flight pipeline tasks to completion, waits submitted device work, then drops the render world normally. Its task-drain diagnostic deadline is 30 seconds and records explicit failure while retaining resources until workers actually finish; cooperative yields use observed task readiness rather than an arbitrary sleep. Queued pipelines with unavailable shader assets need not compile during shutdown: only workers actually executing native calls must finish. The renderer/device and pipeline resources stay alive until those workers finish. No tasks are leaked, tests skipped, or process exit substituted for destruction.

Native Winit early exit has the same underlying exposure: `WinitAppRunnerState::exiting` clears the world and `RenderAppChannels::drop` only receives the render app; it does not drain independently scheduled compilation workers. Successful capture readiness avoids unfinished pipelines at its acceptance boundary, but is not a universal early-exit guarantee. The approved production correction now observes AppExit during ExtractSchedule and joins work after the final Render PostCleanup; its wrapper preserves nonzero exit reasons and rejects a nominally successful exit after a diagnosed deadline. The manual fixture repair alone was not treated as evidence of this native branch.

## Integrated source checkpoint

Reviewed Naga source is preserved in `third_party/naga`, with exact patch/provenance and licenses; `tools/renderer_compatibility` retains the runnable fixtures, GPU readback and native surface probe. Root `[patch.crates-io]` and Cargo.lock resolve the actual Bevy/wgpu graph to this single Naga23.1.0 source (`/tmp/space-scuffle-r3-patched-dependency-tree.log`). Vendored upstream tests and diagnostic tools remain separate from workspace members and have explicit validation commands. The source-tree probe passes real validator and GPU checks again in `/tmp/space-scuffle-r3-integrated-compiler.log`.

The repaired manual production fixture exited normally in three separate Vulkan processes: `/tmp/space-scuffle-r3-runtime-teardown-repeat-{1,2,3}.log`. These repeats precede compiler integration, so their visible baseline compiler VUIDs are expected and are not claimed validation-clean.

## Real integrated renderer verification

`/tmp/space-scuffle-r3-runtime-patched-tests.log`: all nine runtime integration tests pass and the process exits normally. The actual production fixture joins 18 active native pipeline workers. Neither baseline compiler VUID appears after integration.

`renderer_shutdown_probe` uses the real graphical game builder, waits until actual pipeline tasks enter Creating, then sends ordinary success or error-7 AppExit. First native runs preserve exits 0 and 7, respectively; their final render hooks join 10 and 4 active workers. Logs `/tmp/space-scuffle-r3-native-shutdown-{success,error}.log` contain no Vulkan validation errors. No synchronous-compilation mode, alternate shader or explicit process exit is used.

Strict client all-target clippy passes (`/tmp/space-scuffle-r3-compatibility-clippy2.log`); full workspace formatting passes (`/tmp/space-scuffle-r3-compatibility-format-check.log`). Full workspace tests/clippy and final native image review remain coordinated root gates.

Three independent native success/error process pairs preserve exits 0/7 and contain no Vulkan validation errors. Logs are `/tmp/space-scuffle-r3-native-shutdown-{success,error}.log` and suffixes `-2`, `-3`. Each observes actual compilation and joins real native work rather than changing to synchronous compilation.

The rebuilt real mist capture repeats the previously failing native first-screenshot path and now exits 0 with three acknowledged PNGs, no compiler VUIDs, and no MissingAcquireWait/WRITE_AFTER_PRESENT/01286 diagnostics: `/tmp/space-scuffle-r3-compatibility-mist-close.log`. Images are under `/tmp/space-scuffle-r3-compatibility-mist-close/`. Inspected `Close-1-fps60-msaa4.png`: textured terrain, unit media, shell and shadow remain present; this is the final real scene, not the initialization clear. Art quality (including shell rainbow breadth and subtle wake visibility) remains independently reviewed by the optics/mist owner and is not accepted on compiler evidence alone.

## Native resize/minimize/close findings

`/tmp/space-scuffle-r3-native-window-lifecycle/` records actual1280×720 resize, native IsUnMapped minimization and IsViewable restoration, with screenshots showing the real map/UI and advancing simulation. Its final xdotool `windowclose` directly destroys the native X window (per the installed manual) and provokes SurfaceLost; that run is recorded as failure, not ordinary-close evidence.

Repeating with the documented graceful `windowquit` produces a real ordinary-close failure: `EguiContexts::ctx_mut` panics in `lab_ui_system` after window-owned context removal, process exit101. Evidence is `/tmp/space-scuffle-r3-native-window-graceful/{commands.log,client.log,before.png,resized.png,restored.png}`. The native close gate remains failed pending the proposed narrow context-lifecycle correction in blueprint31. Resize/minimize/restore screenshots are valid, but do not certify successful shutdown.

### External automation evidence correction

The first external native-window scripts used `xdotool search --pid PID --name ...` without `--all`. Installed xdotool combines those predicates with OR, so simultaneous preview windows could be selected despite the requested PID. A concurrent root optical capture was closed by such a probe. Those external close-repeat/time-out results are invalid evidence and must not be used to diagnose game shutdown or certify native lifecycle acceptance. All external window operations were stopped immediately. The independent renderer_shutdown_probe success/error runs and actual production-App window-removal test remain valid: they do not use OS window selection.

Repeat external lifecycle only after the orchestrator releases the desktop capture window. Require `search --all --pid PID --name ...`, exactly one matching target, and an independent `_NET_WM_PID` property match against the spawned process before every resize/minimize/restore/close action. Store the verified PID/window pair with each command. Never use a global close/focus action during concurrent captures. Until this rerun, external native lifecycle coverage is provisional even when an earlier process exited0.

### Correctly targeted native lifecycle acceptance

After the desktop was released by the orchestrator, `/tmp/space-scuffle-verified-native-lifecycle.py` reran three independent actual client processes. Every operation requires conjunctive `search --all --pid`, exactly one target and `_NET_WM_PID` equality against the spawned child before proceeding. `/tmp/space-scuffle-r3-native-verified/{1,2,3}/commands.log` retains these per-action identity checks. All three processes exit0 with normal window close and renderer join; all logs contain no Vulkan VUID or panic.

Run1 observes native1280×720 resize, IsUnMapped minimization, IsViewable restoration and graceful WM_DELETE_WINDOW. Its `restored.png` was inspected: actual terrain, unit/structure media, UI and advancing simulation remain visible after restoration. Runs2/3 repeat independently verified graceful close after a real rendered scene is visible. These verified runs supersede the ambiguous earlier external-window experiments; their failures remain documented rather than deleted.

The narrow UI correction uses `try_ctx_mut` only to handle absence of the window-owned context. An existing primary window with no context still logs a genuine error and emits failure. The production-plugin regression removes the primary window, injects a same-frame pointer click, executes the real schedule, asserts ordinary success and joins17 active shader workers (`/tmp/space-scuffle-r3-primary-window-removal-test.log`). Fresh client all-target strict clippy passes in `/tmp/space-scuffle-r3-window-close-clippy2.log`; unrelated capture34 formatting is coordinated with its owner before the shared gate.

## Coordinated gate scope and outstanding editor diagnostics

The first coordinated post-integration gate passed strict workspace clippy and formatting; workspace tests failed on an unrelated stale96-byte bubble uniform packing assertion after the approved optical layout changed to160bytes. Its optical owner is repairing that assertion and rerunning meaningful layout checks before the next gate. This is a failed test gate until the corrected full run passes.

The untouched `editor_app_tests` still emit native presentation synchronization diagnostics: their editor path does not install the game/preview surface-initialization plugin. The user explicitly excluded editor work. This round therefore claims validation-clean evidence only for the actual game and supported native preview/capture paths tested above, not the editor or every renderer launched by workspace tests. The compiler patch is shared, but editor presentation lifecycle remains outside the authorized scope. No editor code or tests were modified to hide those diagnostics.
