# Round 4: Reproducible captures of the production game

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: approved and implemented; capture-only egui attachment amendment approved. Source frozen for shared gates; actual acceptance evidence is recorded in docs/reviews/round4-runtime-capture-verification.md.

## Goal and evidence

Capture the actual normal game at reproducible overview, ramp-traversal, combat, reset and tab-transition checkpoints. Every image must be acknowledged by GPU readback, show the intended scene subjects inside the camera viewport, and carry enough state evidence to prove the requested action occurred. Human/agent visual inspection remains required; a successful process or a nonblank PNG does not certify high visual quality.

The root's attempted native `xdotool` input produced unchanged camera images despite apparent command success. External OS input is therefore not a dependable capture driver here. This harness intentionally tests direct production control/state actions and their downstream systems. It must **not** claim to verify keyboard, mouse, window focus, or OS input adapters.

Current `terrain_preview` and `mist_preview` assemble specialized scenes. They remain useful for their own components, but substituting them for the real game would miss normal `ClientAppPlugin`, scenario initialization, camera/tab ownership and reset behavior. `runtime_battle::BattleRenderStatus` presently establishes that some pipelines compiled; that alone is insufficient for capture acceptance.

## Narrow architecture and ownership

Add a `battle_capture` binary and a `runtime_capture` module/plugin, with a small versioned RON capture script under `assets/captures/`. Extract the current graphical startup chain in `lib.rs` into a shared fallible App builder used by **both** existing `run_app_with_scenario` and the capture binary. The builder must keep normal client plugin registration, asset resolution, external `BattleDefinition::load`, catalog insertion, window defaults, native rendering and normal mode selection identical. Preserve the headless compatibility path separately; it is not a GPU capture path.

A suitable API is `build_graphical_app(mode, max_frames, scenario_path) -> Result<App, ClientStartupError>`. Return typed asset-root/path/scenario startup errors; keep current human-readable reporting/exit behavior in the existing run wrapper. The capture entry point calls this builder in `LiveBattle` mode with no frame-limit success shortcut, then adds only its control/evidence plugin. Explicit capture options may set resolution, deterministic time and disabled live input; they must not silently change normal launch defaults, material options, MSAA, postprocessing, light rig or shaders.

Own new capture source/tests/config, the minimal builder extraction, and narrowly scoped render-evidence exposure if needed. Coordinate any system-order additions in `lib.rs`/`scene_controller.rs` with their owners. No simulation mechanics, unit catalog balancing, map contents, map-editor source, alternate renderer, fake world, synthetic combat events, or special capture shader/material path. Use the already loaded `BattleDefinition` and its actual runtime lifecycle throughout.

Do not register `ClientAppPlugin` twice or reconstruct its plugin list inside the harness. Do not launch the headless sandbox constructor and later replace its map. A failure to load the selected scenario or its real art assets is a capture failure, never a fallback.

## Declarative script and controls

The CLI accepts a selected scenario, a capture-script path and a new output directory. The script has a schema version, bounded tick/frame/wall-time budgets, explicit viewport resolution, and a sequence of named checkpoints/actions. Validate duplicate names, invalid enum/version data, nonfinite camera values, unsupported zoom, out-of-bounds image regions and impossible action ordering before rendering. Keep identifiers filename-safe; reject path traversal and duplicate output paths. Do not accept stale files as evidence.

Use a small closed action set with the existing production semantics:

- Focus/zoom the actual `RtsCamera` on a map point, map center, a deterministically tracked entity, or an observed combat contact. Preserve normal pitch and terrain-height sampling, and require the requested distance inside the camera's existing supported range. Apply controls before `camera_control_system`, then assert the resulting target/distance/transform and projected subject position after that system. No direct `Transform` override that bypasses the camera controller.
- Set `LabState.current_tab` to Live Battle or Visual Lab, then let `sync_scene_camera`, scene management and the ordinary preview/live sync systems perform the transition. Assert actual camera restoration and root visibility before capture; merely assigning the enum is not proof.
- Set `battle_reset_requested` while the simulation is paused, then wait for `manage_battle_scene` to instantiate the same loaded scenario. Assert tick zero, the saved initial checksum/entity roster, exactly one battle root and normal reset cleanup before capture. Never invoke `SimWorld::new` or replace `lab.sim_world` from the capture driver.
- Advance the existing live simulation until an exact target tick or a bounded observable condition. Do not directly invoke `world.step` from a second driver, mutate unit positions/health/orders, teleport an actor onto a ramp, or inject fake damage/VFX to satisfy a checkpoint. Existing scenario initial orders drive the battle.

Capture controls operate directly on production state APIs, not native synthetic key events. Disable live keyboard/mouse influence for the capture run explicitly, so accidental input cannot alter results. Use schedule ordering to clear capture-run input before ordinary camera/selection/UI controls consume it, without changing interactive behavior outside the capture plugin. Document this isolation as a capture-specific option, not an input-system test.

## Deterministic simulation and asynchronous rendering

Use Bevy's existing `TimeUpdateStrategy::ManualDuration` with 40 ms simulation frames and normal speed one. Keep the battle paused and virtual time zero while loading, settling transitions, waiting for pipelines or awaiting screenshot completion. The existing `sync_live_battle_system` remains the sole owner of simulation stepping. Verify exactly one expected tick advances on each running capture frame; any overshoot or unrequested tick is a failure.

Implement an explicit state machine, for example `Loading -> ApplyAction -> Observe -> Settle -> RequestReadback -> AwaitReadback -> Next`, plus terminal Failed/Complete states. Observe after the actual production sync and camera/transform/visibility propagation. On reaching a checkpoint, pause immediately before another simulation update can occur and set the next frame's manual duration to zero. Keep simulation, camera and effect time frozen until readback acknowledges the current image. Allow enough zero-time render frames for changed meshes, materials, VFX and transforms to reach the render world; do not advance gameplay just to wait for the GPU.

Pipeline readiness, loading and screenshot completion are asynchronous and must not select gameplay ticks based on machine speed. Condition predicates are evaluated at each deterministic simulation tick, then retained while rendering catches up. Record both tick and capture ordinal; frame count is secondary diagnostic information. Use bounded simulation ticks **and** a real monotonic wall-clock deadline, plus a conservative frame budget, for every wait and the entire run. Virtual time is zero during many waits, so a timeout based only on `Time` would hang indefinitely. Wall time must never enter authoritative simulation state.

Promise reproducible logical ticks, checksums, action sequence, camera poses and subject selection on a matching build/data bundle. Do not promise byte-identical PNGs across different GPUs, drivers or rasterization stacks.

## Condition evidence and required screenshot sequence

Ship a script for the existing real default battle; discover the useful checkpoint bounds by running that scenario rather than guessing and declaring success. Select actors deterministically by player/capability and stable entity iteration, never by hardcoded creature identity. Conditions that do not occur within their bounds must fail honestly.

1. **Initial overview:** tick zero, a camera region that shows terrain layout, bases/armies and meaningful height transitions at the supported overview zoom. Save the initial logical state for reset comparison. Do not label a cropped map as a complete-map overview.
2. **Ramp entry:** observe a real moving ground combatant enter an authored ramp cell, retaining its preceding non-ramp position/elevation. Focus the same actor/ramp and capture it visibly on the slope, with nearby high/low terrain readable.
3. **Ramp exit:** track that same actor until it reaches the opposite non-ramp elevation through the ramp. Record sampled positions/cells/elevations and the entry/exit ticks. Capture the actor beyond the slope. Simply touching a ramp flag and returning to the original elevation is insufficient traversal evidence.
4. **Combat contact:** observe actual hostile damage (HP/shield decrease or death) together with real combat visual-event evidence, then frame the recorded contact. Require visible opposing combatants and a produced effect/hit response where the authored weapon supplies one. Let normal VFX processing create the effect; freeze after it reaches the scene. Do not infer combat solely from an attack order or a nonempty generic event buffer.
5. **Visual Lab transition:** switch through the normal tab lifecycle. Assert hero-framing camera, visible lab content and hidden battle root, then capture. Production UI remains present.
6. **Return to battle:** switch back and assert the retained battle camera pose and unchanged paused simulation checksum/tick. Capture the restored scene; camera metadata and projected subjects must prove the state transition actually happened.
7. **Reset overview:** trigger the normal reset, assert the original loaded terrain/roster/checksum at tick zero and a single scene/light rig without accumulated bubbles/effects, then restore the same overview control action and capture. Compare logical reset evidence and visual composition with the initial overview; do not demand identical UI clock text or cross-driver pixels.

Optional additional close-up checkpoints may improve review, but may not replace any required category. A capture manifest must identify a skipped/failed checkpoint as failure rather than claiming an incomplete run succeeded.

## Actual asset, pipeline and visible-subject gates

Before requesting an image, require all of the following rather than an any-pipeline-ready predicate:

- The selected scenario/map/catalog/theme paths match the shared production loader. The loaded simulation map and rendered `TerrainState` agree. All real map sprite handles in `MapPresentation`, required bubble texture arrays, and terrain texture handles have loaded image data; check failures and missing resources explicitly.
- The real render-world `PipelineCache` contains successful render pipelines whose fragment/vertex shader handles match the exact required production materials for that checkpoint: terrain and bubble/dome for battle, bubble/dome for lab, and mist/combat material variants when those effects are required. Reuse the installed Bevy API already used by mist capture (`PipelineDescriptor::RenderPipelineDescriptor`, descriptor shader handles and `CachedPipelineState`). Resolve expected handles from the normal shader assets, not alternate source strings. A compiled but unrelated UI pipeline does not satisfy this gate.
- No terminal pipeline error, pending required pipeline, simulation diagnostic, battle initialization failure or failed asset request exists. Preserve validation layers and log output. Existing native compiler/presentation validation issues remain failures; this harness does not suppress them or claim compatibility is fixed by taking a screenshot.
- The intended production entities exist with expected material handles and actual `ViewVisibility`/frustum evidence. Project tracked actors, ramp bounds or combat contact through the real camera and require them inside the configured gameplay image region. Also record battle/lab root visibility, unit counts and terrain chunk counts. A shader cached from an earlier tab is not proof that its current subjects are visible.

Pipeline inspection can be a capture-only render-world evidence resource, or a narrow extension of `BattleRenderStatus`; avoid a second materially different readiness policy inside unrelated binaries. Use actual successful pipeline IDs/handles in the manifest so the result is auditable.

## Readback, files and quality rejection

Request `Screenshot::primary_window()` and use the `ScreenshotCaptured` callback to obtain actual rendered pixels. Associate each callback with a pending run/checkpoint token, and do not advance or exit success until its image has been converted, validated and saved successfully. Screenshot request creation or file existence alone is not completion. Unexpected/out-of-order callbacks, readback failures, wrong dimensions, invalid pixel format, PNG write failures and manifest write failures are typed capture errors.

Use a newly created output directory or fail if requested output files already exist. Write a per-run manifest containing source paths, script/version, viewport, checkpoint tick/checksum, camera target/distance/transform, tracked subject IDs/projected bounds, scene counts, required pipeline evidence, asset readiness and image metrics. Write the final success status only after every expected image callback succeeds; partial files remain clearly marked incomplete.

Measure nonblank output in an explicit script-defined **gameplay region**, not the full image where egui text alone could hide a blank world. Require a nontrivial luminance range/occupied-color distribution, reasonable finite pixel data and expected subject projections in that same region. Document thresholds as corruption/blank-frame checks, not aesthetic scoring. Keep the actual full screenshot including UI; do not crop away broken composition in the saved evidence.

After capture, open every generated screenshot and evaluate against its individual goal. Reject clipped subjects, terrain/units hidden by UI, unreadable elevation, missing effects, stale camera poses, doubled reset objects, incorrect tab content, severe shader artifacts and amateurish presentation. Repeat the capture after a separately approved product fix when necessary. A contact sheet may aid comparison, but full-resolution originals and manifests remain the evidence. Automated metrics never override a visibly poor result.

## Tests and acceptance

Unit-test script parsing/validation, checkpoint state transitions, deadlines, token matching, output collision rejection and nonblank-image metrics. Tests using synthetic pixels are allowed only for the metric algorithm; they are never accepted as real render evidence. Behavioral tests should exercise direct production camera/tab/reset actions and assert their observed state, including a deliberately ineffectual action or unmet condition causing explicit failure.

An actual graphical integration run must use the shared builder, real scenario/assets and production pipelines, emit all required fresh screenshots and a complete manifest, and be visually reviewed. Run twice with the same script and compare checkpoint ticks/checksums/selected actors/camera poses to establish logical reproducibility. Test a missing real asset, unreachable checkpoint and unwritable output to verify nonzero exit and no success manifest. GPU-less environments may report an explicit unavailable prerequisite; they may not pass the acceptance run with a mock renderer.

Run focused tests during implementation, then coordinate `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. Capture logs and artifacts under a clearly named `/tmp/space-scuffle-r4-runtime-captures-*` run directory. No code changes begin until blueprint approval and the current source-freeze checkpoint is released.

## Separately scoped UI finding

`lab_ui.rs::render_live_battle_tab` still labels squads as “Forward Vanguard Assault” and “Heavy Siege Armor Division” although the normal game now loads external scenario/loadout definitions. These descriptors are stale and should receive a separate planned, data-driven UI correction. Record their presence during visual review; do not conceal them or bundle an unsolicited UI rewrite into this capture-harness round. Capture reliability and UI correctness are distinct acceptance questions.

## Approved-implementation discovery: egui screenshot attachment amendment

The first full run produced seven real images with correct logical checkpoints, but all UI was missing. This is rejected evidence, not an intentional clean-scene mode. Installed `bevy_egui 0.33.0::EguiNode` draws the window branch directly into `ExtractedWindow.swap_chain_texture_view`. Bevy 0.15.3 screenshot preparation instead redirects the camera's `ViewTarget` via `ViewTargetAttachments`; its final screenshot blit copies the camera image over the original surface after graph execution. The UI therefore never enters `ScreenshotCaptured`.

Proposed narrow correction, requiring orchestrator approval before implementation: add a capture-only render attachment adapter after `prepare_view_targets` and before `RenderSet::Render`. For the actual window camera, assign its already prepared `ViewTarget::out_texture()` handle to that same extracted window's UI attachment view. Do not change `swap_chain_texture`, camera target, shaders, UI geometry, render graph ordering, or normal interactive startup. `EguiNode` then paints the ordinary production UI into the exact same frame target as the camera; screenshot readback and final surface blit both include it. Each next extraction explicitly drops the old extracted window view, preventing stale cross-frame reuse. The renderer compatibility surface initialization already addresses the actual acquired `swap_chain_texture`, so this does not undo that fix.

Readiness must additionally require nonempty actual `EguiRenderOutput.paint_jobs`; the full-resolution screenshots must show the real production UI. An absent/ambiguous window-camera mapping is explicit capture failure. Verify every image again, including foreground UI occlusion of subjects; keep the gameplay ROI outside UI and adjust declarative framing if necessary. Check fresh screenshots differ from the rejected UI-free run, and compare repeated logical manifests. This is an attachment integration correction, not screenshot compositing or alternate rendering.

## Negative-test discovery: exact unit-media evidence amendment

The missing-media native test removed the selected starting building's actual PNG from an isolated asset root. It nevertheless completed because existing `setup_bubble_assets_system` aliases another loaded archetype's texture and metadata into the missing archetype's registry slots. Therefore registry membership is not sufficient evidence and this negative result blocks capture acceptance.

Proposed capture-only correction: validate the exact visible archetype slug's `textures/{slug}.png` and `textures/{slug}.flipbook.ron` under the already selected asset root. Parse and validate its real metadata, require matching name/layout/dimensions, decode actual image data and compare metadata and bytes with the actual registered array `Image`. Also inspect the visible entity's `BubbleDomeMaterial.video_texture` handle and require it to be that validated registered texture. Missing, malformed, dimension-inconsistent or aliased media becomes an explicit typed capture failure, retaining the precise source path.

Keep a per-run cache of successfully validated immutable asset ID/metadata pairs, invalidating on changed binding metadata/handle. The run contract assumes a fixed data bundle; the harness does not implement live content authoring. These checks are capture overhead, not a production per-frame loader. Tests must cover a real valid pair, missing file, malformed metadata, wrong metadata identity, changed image dimensions/bytes, and material-handle mismatch through the same helper used by the gate. Repeat the real isolated-root missing-PNG run and require nonzero exit/incomplete manifest. Preserve the failed negative run as evidence of the initial inadequate gate.

Do not silently remove or rewrite production fallback behavior inside this amendment. That larger media provenance and loader contract belongs to separately planned round39. Do not regenerate, replace or alter any existing art file to make the capture pass.
