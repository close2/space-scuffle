# Round 2 — Production-quality directional mist

Status: architect blueprint complete; requires orchestrator approval before implementation.

## Goal and rejection bar

Replace the current frame-dependent collection of glowing radial cards with a coherent, lit, temporally stable slipstream. The user rejected its current student-project appearance; a parameter adjustment is not an acceptable completion. The supplied concept at `tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg` establishes the quality bar: readable silhouettes, long directional flow, restrained translucent vapor, separated bright accents, and integration with terrain/lighting. Exact style imitation is not required. The effect must remain attractive in a close view and legible in a crowded gameplay view.

The concept and actual baseline images `/tmp/space-scuffle-mist-baseline/lab-{0,1,2}.png` were inspected. The latest baseline contains the full 1280×720 game window inside a larger desktop screenshot. It shows a short soft cyan/magenta blob between adjacent spheres with little directional structure or depth. The left UI obscures part of the showcase. White-clipped bubble rims are an adjacent renderer issue, not permission to alter all bubble shading in this round. Controlled verification must expose the entire wake and must not hide defects behind the inspector UI.

## Observed causes

- `scene_controller.rs` emits once per rendered frame in live battle. Lab emission checks parity of `(elapsed_seconds * 30)`, which can emit repeatedly during the same interval or miss intervals entirely.
- `vfx.rs` uses a process-global atomic emission counter, per-emission entity spawning and material creation, and particle despawning on expiry. This creates frame-dependent density and asset/entity churn.
- Velocity units are guessed with `if speed < 0.5 { speed * 25 }`; snapshot velocity documentation does not establish a per-second contract. Mist should follow actual rendered movement, not infer units from magnitude.
- A camera-facing square with radial discard, oversized Gaussian density, arbitrary depth bias, and saturated cyan/magenta noise produces overlapping cards instead of connected flow. Shader comments contradict each other about additive blending.
- Locked Bevy 0.15.3 source maps `AlphaMode::Add` and `Premultiplied` to `BLEND_PREMULTIPLIED_ALPHA`; a custom fragment shader owns its final output convention. Resolve the actual equation instead of trusting the current comments.

## Scope and file ownership

Own new `crates/client/src/mist/{mod.rs,emission.rs,ribbon.rs,material.rs,capture.rs}` modules, `assets/catalog/mist.ron`, the existing `assets/shaders/mist_wake.wgsl`, a new `crates/client/src/bin/mist_preview.rs`, and focused `crates/client/tests/mist_*_tests.rs`. Replace only mist-specific definitions/implementation in `vfx.rs`, retaining compatibility reexports for public material/system names used by existing tests. Combat projectiles, beams, shatter, bubble shaders, terrain shaders, and editor code are excluded.

Coordinate these narrow shared-file hunks with the runtime-map track and orchestrator:

1. `client/src/lib.rs`: module/export declaration and `MistWakePlugin` registration; remove duplicate legacy material/update registration as appropriate.
2. `scene_controller.rs`: remove old mist-only resource arguments and the two `emit_mist_wake` call blocks. Do not alter world stepping, terrain ownership, camera focus, or scenario initialization. The new plugin observes existing `BubbleInstance`/transform/preview components independently.
3. `client/Cargo.toml`: declare the new preview binary if auto-discovery is not sufficient.
4. Existing visual integration tests: update legitimate public API/uniform-layout assertions to the final implementation; do not weaken real shader/scene tests or touch editor tests.

The map track owns terrain and scenario integration. Mist installs `DepthPrepass` through an added-camera system in its own plugin, avoiding changes to map camera initialization.

## Architecture

### 1. Generic art and budget configuration

Load a unit-agnostic RON `MistStyle` containing world-unit/radius-scaled sample spacing, lifetime in seconds, width profile, restrained cool/warm tint, extinction, directional-light response, boost accent, maximum emitters, and samples per emitter. Validate finite positive intervals, capacity arithmetic, bounded opacity, and valid colors with a typed error. No creature names, IDs, or per-unit branches are allowed. Select moving entities by existing shape/state capabilities. Buildings do not emit mobile wakes.

Document budget defaults from the 400-unit target. An initial budget of 512 emitter slots and 32 history samples per slot is a testable starting point, not a license to leave unmeasured constants. Correct sampling/lifetime behavior and measured visual coverage determine the final art settings. Capacity exhaustion increments a visible diagnostic and returns a typed activation failure; it must not silently allocate unbounded spillover or fabricate a substitute effect.

### 2. Persistent emitter histories and motion sampling

Use a fixed-capacity pool with a preallocated free-slot stack. Each slot stores the full generational Bevy entity identity, a fixed-size ring of path samples, cumulative traveled distance, residual emission distance/time, local emission ordinal/seed, radius, mode, and most recent observed pose. Assign slots only when an eligible bubble appears. Iterate slots and use O(1) entity query lookup; do not perform nested entity searches. Missing owners begin a final fade and release their slots after remaining samples expire. Scene changes/reset fully clear ownership, residuals, and histories.

Ground gameplay emission on observed rendered transform displacement, measured in world units, and frame delta seconds. Place new samples at exact spacing boundaries along the observed motion segment, interpolating sample position and birth time within that segment. Use an additional bounded temporal criterion only while moving so slow curves remain adequately sampled. Stop stationary emission, let existing tails age naturally, and restart without bridging an old tail. A displacement larger than the configured continuity bound starts a fresh trail rather than drawing a streak across a teleport.

For the stationary Visual Lab conveyor, interpret the existing speed slider explicitly as world units/second. Advance an emitter-local virtual traveled-distance coordinate and map retained samples behind the anchored bubble at the current distance. This produces the same sampling and taper logic without pretending the showcase entity moved or modifying simulation state. Seed variation from emitter identity plus local sample ordinal; remove the global atomic counter entirely.

Large frame deltas must not create an unbounded catch-up loop. Calculate sample count arithmetically, retain only the newest samples that fit the lifetime/ring budget, and preserve the correct residual. Reset/pause policies must be explicit and tested. All client timing remains presentation-only.

### 3. Reusable ribbon geometry and bounded rendering resources

Use a preallocated mesh and pooled render entity per emitter slot, plus one shared mist material. Pre-size position, normal, UV0, UV1, color, and index buffers once. Mutate their existing storage during updates; never create new vectors, meshes, materials, or ECS entities per sample/frame. Pool acquisition/release toggles visibility and ownership. Startup may allocate bounded storage. Record stable capacities and asset/entity counts in a long-running test.

Each emitter draws a small layered ribbon built from its history, rather than a cloud of independent circular particles. Use one broad low-opacity strand and two thinner, offset flow strands within the same mesh. Centerline tangents follow the path; camera-facing side vectors derive from the tangent and view direction with a stable fallback when nearly parallel. Width and coverage taper smoothly at both ends. Clamp the join/miter extent on sharp turns to prevent spikes, and break geometry on discontinuities. Recompute conservative bounds so wakes remain in the frustum after movement. Keep independent meshes per emitter so Bevy can sort different transparent trails around scene entities; measure the bounded draw-call cost rather than combining all trails into one incorrectly sorted transparent object.

Use standard Bevy 0.15 mesh attributes where possible: UV0 carries world-distance-scaled longitudinal and cross-ribbon coordinates; UV1 carries normalized age and boost; color carries per-strand tint/coverage. Their exact semantics and units must be documented alongside WGSL. Update normalized sample age in existing vertex storage. The shader's coherent spatial coordinates must not reset with frame count or each ribbon segment.

This is an explicitly lit ribbon approximation of airy mist, not a claim of full volumetric fluid simulation. Its production quality is judged on actual renders; if it still reads as flat cards after iteration, reject it and extend the design before claiming success.

### 4. Material, lighting, and scene depth

Replace radial-disc support with smooth cross-ribbon extinction and long coherent filaments. Use low-frequency advected flow plus limited higher-frequency erosion to break edges, maintaining temporal coherence across adjacent segments. Avoid independent noise islands at each sample. Keep the broad body nearly neutral/cool and translucent; reserve small HDR accents for narrow ridges and boosted motion. Boost changes intensity/strand length modestly without hiding the entity or becoming the brightest feature in a normal battle.

Use explicit premultiplied-alpha output and `AlphaMode::Premultiplied`, with the verified equation `out.rgb = src.rgb + (1 - src.a) * dst.rgb`. Apply coverage once. Encode a stable cross-section normal/light response for a subtle lit core and darker translucent side; supply actual scene light direction or an explicit documented scene-light input, not a screen-space fake highlight that rotates with the camera. Do not add a large global depth bias.

Enable depth prepass on actual cameras and fade mist at opaque geometry intersections by comparing reconstructed linear scene depth with fragment depth in the same view coordinate system. Use Bevy's locked-version `prepass_depth` helper and respect reversed-Z and multisampling. The shader must compile both configured MSAA paths exercised by the actual client. Depth-test opaque occlusion normally; soft fading addresses intersections, not a license to render through cliffs. Transparent bubble-shell intersection/order remains a separate capture criterion since opaque prepass does not contain transparent shells.

### 5. Plugin integration

`MistWakePlugin` owns config/pool/material resources, startup allocation, camera depth-prepass attachment, emitter assignment, history update, and mesh update ordering. Run observation after lab/live/swarm transform synchronization and geometry preparation before render extraction. Declare ordering explicitly. Do not run both legacy and new emitters. A paused gameplay scene must not keep creating fresh wakes from stale velocity; transform-based sampling naturally prevents that. Existing tails may finish their presentation lifetime according to the documented pause policy.

Load style and shaders from the same validated `ResolvedAssetsRoot` used by Bevy. Any direct preview embedding must create that resource. Missing configuration, shader compilation errors, pool initialization failure, or invalid geometry must be explicit errors. No mock asset path, silent optional shader, or generated imitation creature art is allowed.

## Implementation and verification rounds

### Implementation A — State, sampling, and resource lifetime

Implement style validation, fixed-capacity histories, sampling, ownership, pool allocation, and direct ECS integration. Keep the old visual output until the new path is complete; never run both paths in a final implementation. Add unit/integration tests for equal straight-line trajectories sampled at 30/60/144 Hz, irregular dt, stop/start, pause, 90-degree turns, teleport/reset, entity-generation reuse, zero radius/non-finite input, and pool saturation. Compare sample positions and emission count within documented floating-point tolerance, not screenshots.

For 400 moving emitters over many simulated render frames, assert bounded emitter/history count, stable mesh/material/entity count after warmup, no capacity growth, and correct cleanup after all owners disappear. Document performance-oriented layouts with `PERFORMANCE RATIONALE` blocks. Do not add a global allocator/unsafe test hook; inspect stable buffer capacities and the hot loops, then profile the actual client.

### Implementation B — Ribbon rendering and production shader

Implement reusable geometry, packed attribute contract, coherent flow shading, premultiplied compositing, and soft depth intersections. Unit-test strip indices, finite normals, width/taper bounds, no connection across discontinuities, turn joins, UV continuity, and conservative bounds. Integrate actual shader compilation in the renderer; source-string assertions are insufficient.

### Verification C — Reproducible real GPU capture and evaluation

Create `mist_preview` as a deterministic render harness reusing the production `MistWakePlugin`, real `setup_bubble_assets_system`, actual catalog/flipbook assets, and actual rendering materials. Choose archetypes from catalog flags/capabilities, never hardcoded names. The harness may choreograph reproducible trajectories, but must not replace production shaders/loaders or manufacture creature assets.

Capture at least 1280×720 with a clean viewport and fixed exposure/camera/time:

1. Close-up: one moving bubble, full tail in frame, normal and boosted, stationary control, 90-degree turn, and stop/fade sequence.
2. Gameplay scale: several units moving in different directions over actual lit terrain, overlapping trails, cliff/opaque intersection, and unit-shell overlap. Include a real live-battle screenshot from the game binary after integration with the runtime-map track.
3. Crowd: 200–400 moving units, verifying readable silhouettes and bounded cost.
4. Temporal: matching equal-time captures at 30/60/144 Hz and a short image sequence through turning/stopping. A single attractive still does not establish quality.

Use the real render-world `PipelineCache`: fail on terminal compile errors, wait boundedly for pending compilation and asynchronous shader loads, and confirm visible mist geometry has submitted a ready pipeline before capture. `ShaderNotLoaded` and `ShaderImportNotYetAvailable` during initial asynchronous loading are retryable readiness states even when represented by an error enum; persistent missing files or deadline expiry fail explicitly. Await screenshot completion from the screenshot callback and validate dimensions/nonempty output; do not accept a stale file already present on disk. Lack of a renderer is a failing verification condition, not a passing skip. Software Vulkan may establish correctness/screenshots but cannot substantiate hardware frame-rate claims.

Preserve GPU validation diagnostics separately from application exit status. Root's initial native Vulkan captures reported built-in `previous_mip_depth` workgroup-layout/atomic diagnostics and swapchain synchronization errors; the alternate GL probe failed surface configuration with an incompatible-window-kind error. Neither an exit-zero screenshot nor a clean mist shader proves a validation-clean renderer. Reproduce/report these independently in the final harness. Do not disable validation, bypass the real shader, or replace the rendering path with a dummy to make verification pass; remaining renderer compatibility errors require a dedicated follow-up and must remain visible in the acceptance report.

Inspect the captures with `view_image` against the concept and baseline. Record each case as pass/reject with specific observations: flow direction, tail taper, edge softness, depth integration, local contrast, absence of obvious circles/quads, absence of popping, and whether units/attacks remain visually dominant. Reject and iterate any flat neon smudge, repeated card silhouette, harsh intersection, bright white clipping, geometry gap, frame-rate-dependent density, or dominant combat-obscuring fog. Automated coverage/brightness checks can catch gross defects but do not replace visual review. Keep original-resolution captures and performance observations as review artifacts.

Only after visual acceptance and edit freeze run all required gates: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`. Report actual hardware/backend, resolution, active trails, bounded allocations/resources, and observed timing; do not infer performance from software rendering or a headless simulation test.

## Reference basis

- Riot's [Clarity in League](https://www.leagueoflegends.com/en-us/news/dev/clarity-in-league/) grounds the art direction in visual hierarchy, legible silhouettes, clear direction, and restraint during combat. This plan applies those principles to a movement effect rather than treating brighter output as better quality.
- NVIDIA's [GPU Gems 3, chapter 23](https://developer.nvidia.com/gpugems/gpugems3/part-iv-image-effects/chapter-23-high-speed-screen-particles) explains particle overdraw and depth-based soft intersections. We adopt depth-aware fading and bounded screen coverage; a reduced-resolution compositor is not justified until measurements show it is needed.
- The installed Bevy 0.15.3 sources are the implementation authority: `bevy_pbr/src/material.rs` (`alpha_mode_pipeline_key`), `render/mesh.rs` (blend state), `render/forward_io.wgsl` (attribute interface), and `prepass/prepass_utils.wgsl` (depth sampling). Do not use latest-version API examples without checking the locked engine version.
