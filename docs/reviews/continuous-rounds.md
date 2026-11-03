# Continuous implementation and verification rounds

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

## Operating constraints

- Continue until the user requests a stop; never infer completion from one passing round.
- Preserve pre-existing changes and do not develop the map editor.
- Reuse the versioned map format for game simulation, navigation, and matching runtime terrain.
- Architect blueprint precedes implementation; independently verify each result.
- Visual acceptance requires actual rendered captures and qualitative review against the stated goal.
- The user explicitly rejects the current mist quality. Similarity of style to the concept is optional; comparable production quality is required.
- Any commits must be unsigned. No commits have been made by this work.

## Round 1 implementation goals

1. Exact fixed-point persistence: plan 23, full-width raw-bit RON/binary round trips, checked legacy numeric input.
2. Dynamic-state desync detection: plan 24, canonical framed encoding, state mutation sensitivity, live RNG state, 2,000-tick replay.
3. Legal ramp movement: plan 25, common terrain/segment rules for navigation and physical motion, real versioned-map tests, arrival-snap guards.
4. Runtime asset-root validation: plan 26, explicit errors and a canonical root shared by Bevy and the texture loader.

Four work tracks are coordinated within the available three sub-agent slots plus the orchestrator. A separate fourth task was rejected by the app's approval policy; asset implementation followed serialization within one worker while other tracks and orchestrator verification continued.

## Round 1 verification findings

- Baseline formatting and Clippy passed before edits.
- Initial workspace tests could not open Xvfb inside the sandbox. A permitted unrestricted rerun progressed through integration suites, but its doctest phase lost an old dependency artifact during concurrent rebuilding. This is not a passing workspace gate; rerun after an editing freeze.
- Scalar tests pass over boundary cases and 10,000 seeded full-width values through both RON and bincode. Existing units and structures catalogs remain readable.
- Ten asset-root integration tests pass, including the actual game binary rejecting a missing explicit root before graphical startup.
- Checksum focused tests pass, including all entity/projectile field mutations and independent entity comparison during replay. Static catalogs remain an explicitly documented equality prerequisite.
- Independent traversal review found two arrival assignment paths bypassing movement validation, and unknown structures leaking through static clearance. These are being fixed and regression-tested before acceptance.
- The stable-source workspace Clippy and formatting gates pass. The full test run failed two existing fog-of-war navigation tests whose fixture marks cells with `RAMP` but supplies no ramp direction and mismatched base levels. Repairing the fixture to model actual ramp geometry must retain the original fog/known-terrain and wall-off behavioral assertions. Full workspace acceptance remains pending.

## Visual baseline and rejection

The source concept is `tmp/Gemini_Generated_Image_12da9g12da9g12da.jpg`. Actual existing-game captures are `/tmp/space-scuffle-mist-baseline/lab-0.png`, `lab-1.png`, and `lab-2.png`. The initial virtual screen cropped the window; the captures were replaced with a 1600×1000 virtual-screen run showing the complete 1280×720 game window. The lab UI still obscures part of the scene, so the next verification harness must provide controlled unobstructed views.

The current wake reads as a soft colored patch rather than a coherent flowing trail. It lacks the directional structure, subtle depth, variation in density, and integration with motion visible in the concept. It is rejected, not accepted because its shader executes.

Real native terrain captures were written to `/tmp/space-scuffle-round1-terrain-native/` (`overview`, `p0_base`, `mesa`, `chasm`). Running the existing preview needed `BEVY_ASSET_ROOT` explicitly, otherwise its default Bevy asset root could not load the terrain shader. These captures show an overly bright tiled surface and visible debug grid; they establish the starting point, not a quality approval. Driver validation diagnostics also remain to be investigated; process exit 0 alone does not establish clean rendering.

## Following architecture rounds

- Plan 27: replace the rejected mist with coherent, bounded, frame-rate-independent rendering and controlled GPU captures.
- Plan 28: load game scenarios from existing versioned maps and render the exact same terrain, without editor overlays or editor development.
- Plan 29: inject externally loaded catalogs into normal runtime world/scenario creation, with typed failures and deterministic indexing.

Plans 27, 28, and 29 were reviewed and approved for implementation. The navigation fixtures now express real directed ramps; the full simulation/map test run passes, including actual ascent and wall-off behavior. Required workspace gates will be repeated after the next source freeze; focused tests continue during implementation.

## Round 2 independent verification

- Runtime catalogs are loaded from the selected asset root and injected into world/scenario construction. Tests exercise edits to actual catalog files, malformed input, identity indexing, and scenario construction. Static catalog equality remains a checksum prerequisite.
- The first integrated native battle launch is rejected: `/tmp/space-scuffle-round2-battle-first/client.log` records an authored mist-style RON parse failure and terrain update running before `TerrainState` exists. The black window captures are failure evidence, not visual acceptance.
- Runtime terrain scheduling and camera scene transitions have been corrected and exercised through the production plugin schedule. Pipeline readiness now distinguishes retryable asynchronous shader loading from terminal compilation errors and rejects a timeout.
- Mist preview review identified additional verification gaps: physical versus logical screenshot dimensions on the native high-DPI display, gameplay cases using only parallel conveyor motion, and stopping captures before complete dissipation. These must be corrected before judging the effect.
- Catalog review found authored production durations narrowed from `u32` to `u16`; 65,536 ticks become zero. Plan 30 will address training duration preservation. Construction and research currently apply immediately and require separately scoped architecture rounds.
- Plan 31 will investigate genuine renderer validation compatibility; suppressing validation diagnostics is not an acceptable fix.

## Round 3 implementation and verification

- Plan 30 is implemented: training queues preserve authored `u32` durations, and checksum schema 2 encodes the widened value. Boundary/FIFO/completion/replay tests and targeted strict Clippy pass. Root independently ran `cargo test -p sim_core -p sim_map -p sim_types` to successful completion: `/tmp/space-scuffle-round3-simulation-independent.log`.
- The authored mist configuration is repaired and covered by a real-file parse test. All nine runtime battle tests now pass, including the full plugin startup and camera lifecycle regression. This does not replace native-window screenshot verification.
- Longer native game captures now render real battles. Close mist captures also complete with the actual compiled production shader. Both remain visually rejected; see `2026-10-28-mist-and-battle-visual-review.md` for exact artifacts and reasons.
- Plan 31 is approved in isolated stages. Compiler fixtures reproduce the two baseline SPIR-V defects. A narrow upstream atomic-semantics backport passes independent `spirv-val`; nested/shared-composite layout still fails, correctly preventing integration acceptance. Root workspace dependencies are unchanged at this checkpoint.
- Plan 32 is approved for checked weapon-period arithmetic, atomic upgrade application, positive rate admission, and explicit typed reporting of malformed runtime state. This includes a narrow normal-client error path; invalid state must not silently become ordinary no-fire behavior.
- Complete workspace gates remain pending the next coordinated source freeze. No overall round is declared accepted based only on subsystem tests or process exit status.

## Frozen workspace checkpoint after plans 30 and 32

Root ran all three required commands sequentially on frozen source. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all -- --check` passed. `cargo test --workspace` failed: `runtime_battle_tests` reported nine passed assertions, then its process terminated with SIGABRT during teardown. This is a failing test run, not a passing suite. The renderer owner is reproducing it before dependency integration; skipping the test or leaking renderer state is not an acceptable repair.

Evidence: `/tmp/space-scuffle-round3-workspace-results.json` and `/tmp/space-scuffle-round3-workspace-{tests,clippy,fmt}.log`. Existing Vulkan validation diagnostics also remain explicit failures of renderer acceptance.

Plan 33 (bubble optical hierarchy and correct output color handling) is approved for its next implementation round. Mist v3 remains rejected at both close and gameplay scales; removal of its false shadow and improved sampling do not yet establish production visual quality.

## Renderer repair and continuing visual rejection

The narrowly patched Naga 23.1.0 dependency now validates the targeted SPIR-V fixtures and preserves real GPU readback results. The runtime test process now exits normally after joining outstanding native pipeline workers. Native early-success and explicit-error probes retain their exit reasons. Root independently captured three actual mist frames using the rebuilt dependency: `/tmp/space-scuffle-optics-v2-close/`, with no Vulkan/compiler/presentation diagnostics in `/tmp/space-scuffle-r3-optics-v2-close.log`. Full workspace gates must still be repeated after the next source freeze.

Optics v1 removed the clipped white shell but remained dark and plastic-looking. Root inspected v2 close and healthy captures (`/tmp/space-scuffle-optics-v2-close/Close-1-fps60-msaa4.png`, `/tmp/space-scuffle-optics-v2-healthy/Shell-1-fps60-msaa4.png`): broad saturated purple/green bands still read as colored plastic rather than the restrained thin reflections of the concept. Both remain rejected. Mist remains rejected independently; a clean shader log does not establish artistic quality. Actual source atlases inspected for the hero, worker and building are opaque RGBA throughout. The worker atlas contains a concept scene, which is an existing media-quality defect; showcase filtering is not a repair for the actual game asset.

Plan 34 is implementing reproducible capture through the production app builder and ordinary scenario lifecycle. Required evidence includes real ramp traversal by the same actor, hostile damage with correlated effects, tab/camera restoration and reset. Capture controls explicitly do not verify OS keyboard/mouse input. Plan 35 is being drafted for persistent mist GPU storage; the existing stable CPU capacities do not avoid Bevy cloning and uploading modified mesh attributes. No editor development is part of these rounds.

The independent 400-emitter capture at `/tmp/space-scuffle-mist-v3-crowd-independent/` completed with clean renderer validation, 400 occupied slots, 12,812 retained samples and zero overflow. The screenshot clips some bottom-row actors and does not make wakes readable; it is rejected for visual acceptance. Its `visible=400` statistic describes enabled ribbons, not camera-frustum inclusion. Its `fps=60` is the requested virtual step rate, not measured rendering throughput.

Plan 36 is approved for actual native input verification, explicit UI/window input ownership and scenario-derived battle labels, after the next source freeze. A separate existing defect was found: UI commands call `world.step` directly and discard its snapshot, potentially losing combat visual events and advancing a paused game. It remains an explicit future command-queue architecture task, not silently bundled into input/UI work.

## Second frozen workspace checkpoint

Root ran all three required commands sequentially on frozen source: `/tmp/space-scuffle-round4-workspace-results.json`. Tests failed because the existing headless uniform packing test still expected 96 bytes after the approved 160-byte optical layout. Strict workspace Clippy and formatting passed. The repair now checks the actual shipped WGSL struct layout with Naga against Rust field offsets, and targeted optics/headless tests pass. A fresh full gate remains required. The workspace editor tests also emit the preexisting acquire/present diagnostics; their assertions passed, but these renderer diagnostics are not claimed clean. Editor source remains untouched as explicitly requested. Game/preview validation evidence is scoped separately.

Verified native window lifecycle evidence is `/tmp/space-scuffle-r3-native-verified/{1,2,3}/`; commands validate `_NET_WM_PID` before each window mutation. Root inspected restored actual gameplay and the resize/minimize/restore command evidence. Three ordinary closes exit normally. The mist completion guard was independently interrupted after its first callback and correctly returned 1 with typed incomplete evidence (`/tmp/space-scuffle-optics-v3-interrupted-guard/`). Fresh sphere-hull/dome-recovering replacement runs each save all three PNG/CSV pairs and exit 0 (`/tmp/space-scuffle-optics-v3-complete-reruns/`).

Production battle captures 04 and 05 include UI and match logical ticks/checksums/actors/camera poses across all seven checkpoints. Missing-media negative verification nevertheless exposed aliasing in the existing loader. Plan34 is being tightened to validate exact source media bytes/metadata and the actual entity material binding; capture success remains provisional until that negative test fails correctly. Plans 35 (persistent mist GPU storage), 36 (native input/responsive scenario UI), 37 (non-shadowing combat light cards) are approved for implementation after this gate. Plan 38 (command queue and retained render snapshots) is in architecture review preparation; 39 is reserved for truthful media provenance/loading.

The subsequent repaired gate (`/tmp/space-scuffle-round4-repaired-workspace-results.json`) passed the new actual-WGSL ABI/headless tests, then failed the runtime asset test because it still expected the prior startup diagnostic prefix. The real binary returned a nonzero status and the correct absent asset path/cause before graphics startup. Strict workspace Clippy and formatting passed again. The test contract is being aligned with the shared typed startup wrapper while retaining its meaningful failure assertions; remaining client integration targets will be preflighted before another full workspace run. No failed full run is counted as acceptance.

## Follow-up audit during the frozen gate

The remaining client integration preflight passed: runtime asset validation (10), runtime battle lifecycle (9), capture configuration (5), simulation diagnostics (1), and terrain (13, including four real llvmpipe poses). The full workspace gate is rerunning under `/tmp/space-scuffle-round4-final-workspace-*`; no implementation resumes before its result.

Independent source review confirms that simulation snapshots and combat events carry owner slots 0/1 directly as `TeamId`, while bubble, indicator and effect color matches reserve blue/red for 1/2. A separate shared-palette blueprint is being prepared. This is a semantic rendering defect in addition to weak visual readability.

A later simulation round must address transactional production admission: `process_commands` currently deducts production resources before finding a valid owned producer, accepts any owned entity, and ignores structure-spawn failures. `spawn_structure` also advances the entity allocator before placement validation. These findings are recorded for planned implementation; no unreviewed simulation edits are bundled into the current rendering/media rounds.

## Full gate: pathfinding performance remains failing

`/tmp/space-scuffle-round4-final-workspace-results.json` records workspace tests exit101 after543.11s, strictClippy exit0, formatting exit0. All client and sim_core suites passed; sim_map performance rejected100queries at333.803ms against the unchanged250ms debug budget. The deterministic reference checksum passed. Three unchanged isolated samples were234.712/240.543/334.044ms with83successfulqueries; machine load average was51.45. Evidence: `/tmp/space-scuffle-r5-path-performance-samples.log`. No failed measurement is discarded and no timing threshold is relaxed.

The orchestrator released the source freeze for disjoint approved37/35 and39 implementation while the traversal architect profiles the actual pathfinding hot path for a possible41 repair before36. This avoids treating a variable failed performance run as acceptance or suspending unrelated progress indefinitely. Another complete coordinated gate remains mandatory.

## Verified bounded repairs37 and41

The actual matched combat capture removes the rectangular light-card shadow while retaining terrain/unit shadows, with all seven checkpoints complete and303 logical manifest fields identical to the baseline. Traversal41 preserves the exact checksum and reduces profiled work32.78%, with five coordinated100-query samples84–89ms under the unchanged250ms limit. Focused results are accepted; the full workspace gate is still pending. See `round5-combat-shadows-and-path-performance.md`.

The combat capture additionally exposed a repeated ALSA output-stream error flood. Audio initialization/lifecycle is being planned as42 before measured performance work. No audio success or globally clean runtime is inferred from the successful image/GPU verification. Native input36 and exact media39 continue in parallel; editor development remains excluded.

## Round 6 independent runtime verification

The corrected production capture at `/tmp/space-scuffle-r6-viewport-battle/` completes all seven checkpoints and exits0. Root inspected the combat and Visual Lab PNGs: controls are contained at1920×900, the actual selected Lab actor is visible, and its identity matches the inspector/portrait. Compared with the run06 baseline, ticks, checksums, actors, camera poses and all other selected logical fields remain equal; viewport-dependent visible-unit counts increase at the ramp and Lab checkpoints. Raw comparison: `/tmp/space-scuffle-r6-independent-capture-review.json`. This does not verify native mouse input or establish overall art acceptance. Mist remains visually rejected.

The game explicitly records `DisabledNoPlaybackSubsystem`; its2413-byte capture log contains no ALSA POLLERR, validation or panic messages. The earlier incomplete42 capture remains retained as failure evidence. The ordinary missing-media window at `/tmp/space-scuffle-r6-media-failure-window/` displays the exact missing PNG in a readable panel at1280×720/native scale1.59375, stays at tick0, and returns exit1 after a PID-verified ordinary WM_DELETE. The automated negative capture also exits1 with no accepted checkpoints. No media aliases or substituted images satisfy these checks. Existing legacy artwork remains unverified, including six concept copies.

Native pointer verification remains open: Xephyr selected the real AMD Vulkan adapter but could not provide a supported surface format, so that attempt failed before input. Further owned-window delivery checks must show actual Winit/Bevy events and effects; successful automation return codes alone are insufficient.

Plan43 is approved for transactional training admission/completion, captured supply accounting and checksum schema3. Reserving supply for all admitted FIFO jobs is an explicit Space Scuffle policy, not asserted exact SC2 queue behavior. Build/construction remains a separately recorded incomplete lifecycle. Product implementation waits for the approaching coordinated full gate. Plan35 baseline recording is undergoing independent review before the dynamic-mesh implementation is replaced.


## Separate mover-layer defect found during plan43 preparation

Read-only inspection confirms that `step_movement_and_steering`, target pursuit, arrival snaps, hazard escape and collision displacement use ground traversal without consulting `UnitArchetype::mover_type`. Local steering/cluster-arrival obstacle lists and circle collision pairs likewise include both movement layers. The renderer alone raises Air instances above terrain. Consequently the existing authored Air classification does not establish actual flight, and CliffJumper/CliffWalker names do not establish implemented cliff traversal. This is not repaired by plan43's local production exit admission. Plan45 is architecture-only follow-up; no movement code is changed during the frozen workspace gate. Cliff-specific limits/timing must be explicitly authored or approved before implementation, not inferred from creature identity or lore.

## Round 6 frozen workspace gate: explicit fixture dependency failure

The complete gate invocation at `/tmp/space-scuffle-round6-workspace-results.json` records workspace tests exit101 after368.86s, strict workspace Clippy exit0 and formatting exit0. SHA-256 source inventories before/after are identical (`source-changes.json` is empty). Asset-baker suites, client unit tests, input-policy/mist/headless suites and the actual virtual-GPU startup test passed. The direct schedule integration fixture failed because it never registered the newly required explicit `InputOwnership` resource; root independently reproduced the exact missing-Res panic in `/tmp/space-scuffle-r6-schedule-failure.log`. It is a failing gate even though the ordinary production builder supplies the resource.

A narrow plan36 fixture repair is authorized: register the explicit noninteractive policy for this no-window schedule test, retain actual startup/assets/systems, and keep the production parameter required. Plan35 also requires both real-time warmup and authored virtual wake maturity before performance measurement; that bounded recorder correction is authorized before the release baseline. After focused and remaining-client preflight checks, repeat the full workspace gates. The earlier editor tests again emit their pre-existing acquire/present synchronization diagnostics; editor code remains excluded from this work and those messages are not reported as clean game-renderer evidence.

## Round 6 repaired workspace gate passes

The repeated frozen gate at `/tmp/space-scuffle-round6-repaired-workspace-results.json` passes all three required commands: workspace tests (643.092 seconds), strict all-target workspace Clippy (0.663 seconds), and formatting (0.661 seconds). Before/after source inventories match exactly. This establishes the integrated code checkpoint, not visual acceptance; the previously documented editor diagnostics and rejected mist artwork remain explicit limitations.

Before replacing dynamic mist meshes, the baseline owner will finish the approved conservative camera-support correction for the actual 0.7-radius emission offset, run focused checks, and snapshot source plus actual assets with verified SHA-256 inventories. Plans43 and46 remain held until that snapshot is immutable. Root also found a blueprint defect before GPU implementation: Bevy0.15.3 transparent sorting uses entity translation, so identity transforms cannot preserve distinct trail sort positions merely through world-coordinate AABBs. Plan35 requires a reviewed correction before replacement work.

The emission-support regression, focused strict Clippy and formatting passed. The read-only baseline is now `/home/cl/projects/space-scuffle/tmp/verification/mist-baseline-r6-20261028`: 399 inventoried entries, 279,475,732 bytes, inventory SHA-256 `3db921dd6f78cff85f76ae58f05f7eef0e0ae9e37d66612f2319d6f7f80506dd`. Root independently rehashed the snapshot and matched every manifest entry. The existing relative client-assets symlink resolves inside the copied trees. Plans43 and46 are released for implementation while the isolated release baseline builds; no mist geometry replacement is accepted or started from this snapshot alone.

Root also confirmed that the custom mist light selector ignores scene-light visibility, unlike the bubble-optics selector. Plan46 restores global environment and actual light visibility only; a separate reviewed mist-lighting change is required after the baseline comparison. Hidden-light behavior must not be claimed repaired by standard renderer visibility tests.

The original isolated release binary has SHA-256 `ac247b572b368fe817acadfac90e7c1c9e4b394072b7d28d645f6d8000453d48`. Its first actual zero-emitter run exits1 before measurement: `/tmp/space-scuffle-r6-mist-baseline-0.log` reports a window/settings mismatch and joins native pipeline workers. No benchmark result or screenshot is accepted. The native input probe independently observed resolution2040×1147 on frame1 and1280×720 from frame2 at scale1; this suggests window negotiation, but the failed benchmark's generic diagnostic does not establish which setting differed. A narrow recorder amendment must record actual settings, permit only bounded pre-readiness negotiation, and enforce required settings throughout warmup/measurement. Preserve the original snapshot/binary and identify the corrected derivative by exact file/hash differences.

The revised plan35 packet-ordering architecture is approved: freeze packet selection before Queue, correct only mist Transparent3d distances after material queueing and before phase sorting using the selected packet's emitter origin, and copy coherent sample/header/frame clocks before drawing. Root checked the required public APIs in locked Bevy0.15.3. Actual delayed-packet/camera/sorting tests remain mandatory. GPU replacement still waits for a successfully measured dynamic baseline.


## Round 7 measured dynamic-mist baseline and continued implementation

The bounded loading-readiness correction is preserved separately at `tmp/verification/mist-baseline-r6b-20261028`, inventory SHA-256 `ad7d21b88e6e319da77f3f579e830c15577119c4866557de76cea111bf60f435`. Root independently verified that only `crates/client/src/mist/benchmark_capture.rs` differs from the first immutable snapshot. Required resolution is enforced before warmup and throughout measurement; the original failed attempt remains evidence. The preserved corrected release executable has SHA-256 `b85c3a600abec631af331de121c4f42eb7e2cee06f0c37012776bfa912585f28`.

All six actual AMD Vulkan runs at 1280×720/MSAA4 exited0: forward0/100/400 and reverse400/100/0 emitters, with400 real catalog actors in every case. Forward whole-frame p95 values are27.155/22.753/41.172ms; reverse values indexed0/100/400 are18.250/25.677/50.627ms. The zero-emitter baseline itself exceeds16.667ms, so no60fps acceptance or clean subtraction of total mist cost is justified. At400 emitters, separately measured median CPU ribbon-geometry time is11.679/11.823ms and extracted geometry throughput is384.100/304.446MB/s. Variable host load and configured `AutoNoVsync` are recorded; resolved private backend presentation mode is unavailable and is not inferred.

Reports, metrics and actual PNGs reside under `/tmp/space-scuffle-r6b-mist-baseline-{0,100,400}` and `/tmp/space-scuffle-r6b-mist-baseline-repeat-{0,100,400}`. Root inspected the full zero/400-emitter middle captures and independently parsed all six timing distributions. All400 actors are framed; bright rigid trails, tiny creature discs, repetitive floor and blurred background remain unsuitable for art acceptance. Frozen performance baseline acceptance does not accept the mist appearance or legacy media provenance. The GPU-storage round must preserve render semantics and establish actual measured resource/CPU improvement before a separately reviewed visual replacement.

Production43 review exposed a fractional-origin swept-footprint hole in the initial cell-offset egress predicate. Its reviewed amendment uses allocation-free exact rational segment intersections against radius-expanded cells, with explicit conservative square clearance, tangency rules, i128 bounds and every crossed footprint terrain edge. Regression implementation is ongoing;43 is not yet accepted. Environment46 has focused and native transition evidence, with actual Lab background restored twice; the combined workspace gate remains required before its integrated acceptance.


## Round 7 combined gate in progress

After43's final public source-radius boundary regression, all owners froze product files. The coordinated driver `/tmp/space-scuffle-round7-workspace-gate.py` is running the three required workspace commands with source inventories before/after. Focused43 evidence includes the full sim_core suite,18 transaction tests plus the separate extreme-radius regression,3 corrected client presentation tests, strict sim_core/client Clippy and formatting. Those checks do not replace the pending full gate.

Independent root review confirmed the mobile producer footprint validator prevents huge positive source radii from entering perimeter enumeration; no speculative radius clamp or unrelated loop rewrite was added. The fractional-origin algorithm instead received actual public production completion coverage with real wall blockage/reap. Existing invalid test setups were migrated through authored scenario setup and real death lifecycle, preserving their sparse-owner/splash assertions.

Plan47 is approved for a bounded fresh-press camera-drag state after this gate: focus/UI/viewport loss cancels the gesture, ordered native cursor/focus/button events distinguish press-time geometry, and retained global mouse-held state cannot rearm by itself. Both failed native release-delivery attempts remain retained; the plan does not misreport their automation as proof of resumed dragging. Plan38 follows with the sole tick owner and retained presentation.35 follows with persistent GPU transport,45 with authored movement layers. Architecture-only construction48 remains a later independent round; editor development stays excluded.


The round7 frozen gate passes: workspace tests536.464s, strict all-target workspace Clippy1.597s, formatting1.025s; all exit0 and before/after source fingerprints are identical. Evidence: `/tmp/space-scuffle-round7-workspace-results.json` and sibling logs/inventories. The editor integration suite again emits Vulkan acquire/present synchronization diagnostics; runtime environment's intentionally missing-theme test reports its expected typed asset failure. Neither is represented as a clean editor-renderer result. No editor source was changed.

43 transactional production and46 environment lifecycle are accepted within their documented scopes.35 persistent GPU storage and47 drag cancellation are released for implementation.45's expanded displacement/outcome amendment still requires root review before its new slice;38 retains narrow step/snapshot ownership. Root is capturing deterministic fixed-time dynamic-mist baseline sequences from the immutable r6b source/binary while new source work resumes; these are visual equivalence references, not fresh performance measurements or art acceptance.


## Round 8 independent native and renderer review

47 now has focused state, scheduled camera/projection, input-policy and affected startup fixtures passing. Root inspected the explicit gesture state and its ordered event handling, including the prepass that denies presses for any resize/scale event in the batch and camera/window identity cancellation. Actual native run `/tmp/space-scuffle-r8-drag-native01` passes five delivered-input checks but fails to observe requested post-minimize pointer movement. That failure is retained; inactive gesture state alone is not proof of delivered-motion recovery. A dedicated owned helper-window focus experiment is underway to distinguish unmapping from ordinary focus changes. See `round8-camera-drag-verification.md`.

The first persistent-buffer35 Close capture at `/tmp/space-scuffle-r8-mist-storage-close01` exits0 on actual AMD RADV Vulkan with clean runtime logs and three decoded1280×720 screenshots. Root viewed the middle image and the preserved dynamic-mesh reference at the same fixture time. The appearance retains the rejected faint rigid wake; this is not art acceptance. Independent exact RGB comparison reports1380–1927 changed pixels per image,1208–1715 with a channel difference above3, and maximum channel differences127–133, confined to the wake region. The implementation owner must explain and verify those local differences before equivalence acceptance; a small whole-image average is insufficient. Exact metrics are retained in `comparison.json`.

Movement45 exposed a registered-foundation collision regression: the enemy push branch could move a foundation entity while its registered map footprint stayed fixed. The reviewed49 dependency assigns zero correction weight to actual foundations/immobile bodies and guards final application and explicit displacement admission. Its original failing simulation integration log remains retained. Root additionally identified incomplete extreme-arithmetic validation in the dynamic compatibility branch; a checked-core/API amendment is required before accepting its typed-error contract. No phase-wide checked accumulation claim is made. The last complete workspace gate predates35/45/47; a new coordinated gate is still required.

47 follow-up native evidence closes ordinary focus cancellation/recovery at scales1 and1.5. The owned-helper experiment keeps the game mapped while changing focus; run03 and root's `/tmp/space-scuffle-r8-drag-native-scale15` each pass three checks and close both owned windows normally with exit0. At scale1.5, delivered post-return cursor sequence24→27 leaves the cancelled target unchanged; a distinct new press followed by sequence27→30 moves it from(-0.162673,-0.140668) to(-0.283862,-0.233857). Root inspected the final actual screenshot: actors/UI remain visible, while the existing rigid mist and floor remain rejected. A raw release had already arrived before return motion, so native retained-held-bit motion is still not claimed; the scheduled event regression covers it. Run02's final failure was a driver demanding a redundant release, confirmed independently from its trace, and remains retained.38 is now implementing its approved reusable snapshot and sole tick-owner stages.

Root ran35's production-geometry compute readback on the actual AMD890M/RADV Vulkan device: `/tmp/space-scuffle-r8-mist-oracle-amd01.log`, exit0, clean log. This validates the unchanged shipped geometry function against the CPU reference within3e-5; it does not replace actual upload/phase/raster verification. The image difference is now localized to wake/sphere overlap, with at most one channel difference left of x700. Equal-depth transparent ordering is being investigated before any art or ordering change.

49 independent source review caught two unintended checked-core migration changes: the second moving actor's wedge sign and the idle-first lateral ID tie condition. Both require direct sign/branch regressions and restoration of the intended old policies. The new arithmetic representation may change last raw bits; no bit-identical legacy arithmetic claim is made. The new Fixed/Dynamic coincident-center policy is separately approved to use distinct EntityId order for an antisymmetric axis, while the legacy Dynamic/Dynamic fallback retains its documented stable world-pair convention.


35 transparent-order diagnosis now has a controlled same-binary comparison. The diagnostic-only baseline derivative contains exactly one changed source file (`mist/capture.rs`), independently rehashed399 entries, canonical inventory907300e0be58ececa16df4c6fbc55ec7d943649c9046b7f4371fd39a557a7c90. Its executable SHAae8ebfb67064f67a77d4b4b535db782a7e94ccf934a9b1d61055677e579fc193 produces three images pixel-identical to the preserved baseline and records mist before shell at equal distance bitsc0e7a11e. Original snapshots/binaries remain untouched.

The preserved storage probe SHA50484d29c0e4eb33bb16a822fc15db7f93ca1da1ca2c40c1bbc52c097aa08614, with the post-sort observer enabled, records the same order and differs from baseline by at most one channel value (36/32/19 changed pixels). Running that identical binary without registering the observer reproduces the bad overlap:1714/1208/1621 pixels differ by more than3 channels, maximum127/131/133; the free wake is pixel-identical between these two runs. All runs exit0 on actual AMD Vulkan with clean logs. Evidence: `/tmp/space-scuffle-r8-mist-phase-baseline01`, `...-storage01`, `...-storage-unobserved01`, their comparison JSON and sibling logs. Root viewed the corrected-looking observed capture, without accepting the art. The bad run's phase order was not directly observed, but this controlled experiment and the engine's distance-only stable sort establish fragile equal-depth scheduling as the correction target.

The explicit queue chain mist material → packet alignment → bubble material is approved to preserve the measured baseline tie policy without epsilon/depth-bias changes or unequal-depth reordering. Actual unobserved captures and observer crosschecks are required after the correction. Geometry readback alone did not expose this raster/compositing regression.


### Round 8 independent storage checkpoint, before combined gate

Root repeated the corrected Close executable with diagnostic phase observation disabled and enabled. Both match the preserved reference within one channel value on all three actual images; no pixels differ by more than three. The enabled run changes36/32/19 pixels relative to baseline; off-versus-on changes120/108/92 pixels, all at most one channel value. Art acceptance remains rejected; this is bounded baseline-equivalence evidence for the explicit material queue chain. Artifacts: `/tmp/space-scuffle-r8-mist-tie-fixed-off01` and `/tmp/space-scuffle-r8-mist-tie-fixed-on01`.

The debug, contended zero-emitter recorder smoke completed on actual AMD Vulkan with clean validation: `/tmp/space-scuffle-r8-mist-storage-recorder-smoke01`. Root inspected the middle native1280×720 image:400 actors, no active mist, correct requested fixture. All four new transfer CPU distributions have93 samples; the timed interval has zero modified/extracted mist mesh assets or geometry bytes, unchanged three target buffers819280bytes and three staging buffers2457840bytes,93 submitted packets76193040bytes and zero stalls. Wall p95=211.976ms is a debug plumbing observation, not release performance evidence. RSS rises during this short run; no stable-driver-memory claim is made.

Root reviewed/approved plan35 actual-owner reset verification amendment for implementation after the combined checkpoint. Plan44 conservative projected clipped-box coverage architecture is reviewed; volume implementation remains held until storage evidence is complete. Source35/38/45/49 is frozen for quiet pre50 benchmark collection followed by shared checks. No editor development or commits.


### Round 8 shared checkpoint passed

The complete frozen-source gate passed: `cargo test --workspace` (349.45 seconds), `cargo clippy --workspace --all-targets -- -D warnings` (1.31 seconds), and `cargo fmt --all -- --check` (0.43 seconds). Source inventories before/after report no changes. Full command output and inventories are retained under `/tmp/space-scuffle-round8-workspace-*`. This verifies the current 35 storage slice, 38 library foundations, 45/49 movement and fixed-body changes, and 47 camera cancellation together. It does not complete the pending storage reset/performance/art acceptance or migrate the running client scheduler.

The full 18-test round2 visual suite passed, including its real startup/frame-limit test that previously failed before initialization in the agent sandbox. Pre-existing editor Vulkan synchronization diagnostics remain in the complete log; editor code was not developed. Expected typed negative-case diagnostics remain distinguishable from test failures. Root independently ran the expanded all-three-buffer geometry/upload oracle on AMD Vulkan with exit0 (`/tmp/space-scuffle-r8-mist-oracle-amd02.log`), including the explicitly expected stale-generation rejection.

Pre50 unpinned timing collections01/02 both fail the unchanged 10% variation gate; all84 workload correctness samples pass. Their entire matrices remain inconclusive. A reviewed single-core CPU3 collection with actual sibling15/frequency evidence follows this shared gate, keeping fixtures, executable, sample counts and thresholds unchanged. No favorable-case selection or performance claim is made.


### Broader storage capture rejects global legacy-image equivalence

All seven 60Hz cases completed with clean actual AMD logs and21 decoded1280×720 screenshots at `/tmp/space-scuffle-r8-mist-visual-storage`. Close/Boost differ by at most1–2 channel values, Stationary and fully faded Stop are exact. Turn differs by3,980–7,021 pixels above3 values; early Stop differs by1,282–1,868. Gameplay and Crowd also have substantial overlap differences. Root inspected Turn, Stop, Gameplay and Crowd. All400 actual crowd actor/wake bounds pass frustum checks, with zero overflow; that does not confer visual acceptance. The free wake left of x700 in Turn/Stop differs by at most one value. Global image equivalence is rejected, not averaged away.

Actual phase captures at `/tmp/space-scuffle-r8-mist-phase-matrix01` show the preserved diagnostic dynamic baseline queues shell before mist for Turn/Stop at identical distance bitsc0e7a11e; the corrected storage queues mist before shell. Baseline diagnostic Turn/Stop images are pixel-exact against the original reference captures. Storage observer on/off stays within one channel value.

A controlled repeat at `/tmp/space-scuffle-r8-mist-phase-repeat02` establishes the old executable itself is unstable: the same preserved baseline-phase binary (SHAae8ebfb67064f67a77d4b4b535db782a7e94ccf934a9b1d61055677e579fc193), same Close arguments/assets and observer now queues shell before mist, whereas its earlier run queued mist before shell. Its three Close images change1,714/1,208/1,621 pixels above3 with maxima127/131/133; the entire free wake left of x700 is pixel-identical. Turn repeats exactly. Therefore one stable queue policy cannot reproduce every previously nondeterministic overlap image. Root requests a reviewed controlled dynamic-baseline derivative with only the same explicit generic mist-before-shell tie contract, preserving original artifacts and all art/geometry, before claiming equivalence. No fixture-specific ordering or opacity workaround is authorized.

### Round 8 controlled storage equivalence and reset verification

The corrected-order dynamic baseline now passes actual startup and all thirteen aligned cases:39 screenshots differ from storage by at most two channel values, with exact fixture metrics; six repeated Close/Turn images are pixel-exact across processes. The failed direct system-type-set baseline remains preserved; the successful derivative uses a reviewed ordinary empty dependency set between existing material queue systems. Root independently rehashed the399-entry source inventory and preserved executable. This resolves comparison against a stable generic compositing policy, not the rejected art quality.

Twelve preserved release runs, forward/reverse dynamic/storage at0/100/400 emitters with400 real actors, all completed on AMD Vulkan. Root independently decoded all36 PNGs, checked identical media identities, actual frustum counts and every storage transfer distribution's sample count. At400, storage main-update medians are0.243/0.249ms; sum of separately recorded CPU component means is0.608/0.627ms, not measured critical-path latency. Storage packet transfer is48.95/48.92MB/s versus dynamic raw mesh extraction448.49/332.72MB/s at their respective measured boundaries. All six storage runs have zero mesh mutations/extractions, zero new owned buffers and zero stalls. Whole-frame p95 remains17.109/17.137ms at400; even zero-emitter runs exceed16.667ms. Transparent GPU variation prevents a uniform speedup claim, short-run RSS growth prevents a memory-plateau claim, and no independent percentiles are subtracted. Full independent evidence: `/tmp/space-scuffle-r8-mist-paired-release/independent-summary.json`; detailed table and limits are in `2026-10-28-round8-mist-storage-verification.md`.

The exact preserved reset02 binary (SHA256 `ea233b03e931dd5b8d23c64c85693c9daa0e23c571968eed2729dece30746611`) then completed real AMD owner replacement with clean logs and exit0. Root rehashed all425 source entries, viewed all three1280×720 screenshots and independently decoded the report. Across48 main/queue frames,15,601 actual draws (14,001 after reset) match current owner and selected-packet generations. All400 replacement actor IDs differ while assets and reset poses match. Sequence110/frame100 has zero history samples and zero queued mist draws; sequence152/frame140 queues all400 new wakes. Three target and three staging buffer identities/bytes remain unchanged, with no stalls. Evidence: `/tmp/space-scuffle-r8-mist-owner-reset02/independent-audit.json` and sibling PNGs/RON/log.

35's bounded storage, equivalence and replacement contract is accepted subject to the next integrated workspace gate, which must cover the new reset fixture alongside active38/50 changes. The separate whole-frame budget and mist art remain failed. Plan44 Stage1 output-transform correction is released as the next implementation/verification round; light and volume stages still await their sequential review. Existing zero-emitter frame-budget failure is retained, not silently converted into acceptance. No editor development or commits.

### Round 9 live scheduler and checked-motion review

The38 production-plugin schedule passed its actual GPU fixture with normal shutdown. The actual-loader CPU integration and scene-lifecycle suites pass, including failed-candidate preservation. A first seven-checkpoint native battle capture also completed with exit0 at `/tmp/space-scuffle-r9-pipeline-battle/capture`. Root decoded all seven1920×900 images and viewed overview, combat, Lab and reset. Ramp boundaries are observed at ticks162/195; combat damage636 is represented by the retained effect captured at637. Reset returns tick0 with generation2 and the initial checksum; overview/reset differ in233 pixels by at most5 channel values. This is bounded lifecycle evidence, not an art pass, OS-control test or proof of fractional moving presentation.

Root found that manifest2's reset checkpoint retained combat_tick636/source17 and prior ramp trace without naming their original generation. The capture's historical proof must explicitly carry its source generation; owner is correcting this ambiguity and will preserve the old artifact. The approved subsequent native-probe extension will verify actual paused UI submission, correlated execution, moving interpolation at30/144Hz virtual cadences, pause/resume and reset with a pending request. Virtual cadence is not measured GPU/monitor performance.

The allocation attempt initially profiled schedule-caller scope while Bevy worker systems ran outside its collection boundary. That evidence is inconclusive for the intended driver/synchronizer claim. The reviewed refinement enables all-thread collection only after actual loader/warmup readiness, then dumps at the bounded phase boundary and requires real system symbols plus a positive allocating control. No zero-allocation claim is accepted from pointer reuse or an empty profiler dump.

50's new real-command numeric and transactional tests pass, and its first complete sim_core/sim_types checkpoint passes. Root review caught deferred Blink recording its destination as the tick origin and direct missing-catalog ability execution silently returning ordinary rejection. Both bounded corrections were approved with focused regressions; malformed operations must preserve state and report typed failures. Performance acceptance remains held under the original thresholds after three inconclusive pre-change timing matrices. All current work still requires the next coordinated frozen-source workspace gate. No editor changes or commits.

### Round 9 mist output contract verified on the actual GPU

The final shipped-helper test passed on AMD890M/RADV Vulkan with a clean log and exit0 (`/tmp/space-scuffle-r9-mist-output-amd01.log`). It compiles actual Bevy grading/tone-map/deband imports and checks independent expected color/alpha arithmetic, including HDR radiance and zero alpha. The initial test lifecycle panic/SIGSEGV remains preserved; the corrected test retains the real renderer while explicitly disabling pipelined ownership of its RenderApp.

Root then ran nine native Close/Boost/Crowd cases: new non-HDR, new HDR and preserved pre-change HDR control. All27 screenshots decoded1280×720, all runs exited0 with clean AMD logs, and referenced assets were rehashed before/after. Only the intended mist shader differs from the preserved asset inventory. Every actual non-HDR descriptor records TonyMcMapface, debanding, Rgba8UnormSrgb, premultiplied blending, MSAA4 and disabled depth writes; HDR records Rgba16Float with no in-material tone-map define and the same blending/depth contract.

The unchanged HDR path agrees with controls: Close/Boost differ by at most one channel value in7–20 pixels; all three Crowd HDR images are pixel-exact. Non-HDR changes occur in the wake region, with maximum channel differences30–45. Root inspected middle images for every new case/path and the older Close reference. The wake remains faint and flat; art acceptance is rejected. Evidence: `/tmp/space-scuffle-r9-mist-output/independent-output-audit.json`, descriptors, PNGs and results. This accepts44 Stage1's bounded output correction subject to the combined workspace gate, not the volume/lighting artwork or frame budget. Stage2's actual-view-light architecture is reviewed; concrete fixture schedule and combined gate precede implementation.

### Round 9 scheduler evidence and rejected OS capture freshness

The corrected seven-checkpoint battle capture completed normally at `/tmp/space-scuffle-r9-pipeline-battle-final/capture`. Root viewed every image and independently inspected checkpoint fields. Reset is session generation2/tick0 with the initial checksum, while retained ramp/combat history now explicitly identifies generation1. Ramp and visible combat checkpoints remain162/195 and637 (damage636). This resolves the historical-proof ambiguity without claiming those events happened in the replacement session. Existing weak mist and legacy artwork remain rejected.

The all-thread allocation profile05 contains the actual production advance/presentation/synchronization symbols. Root's independent positive-edge callgraph audit found no allocator descendants in the named warmed paused and moving no-tick paths; the same executable's escaping-allocation control records allocation, and actual authoritative-tick/membership work does allocate. This bounded CPU evidence excludes UI, GPU/driver, unrelated engine tasks and untested growth cases. Earlier profiler attempts remain inconclusive, including the incomplete fixture's missing normal asset-event drain.

The144Hz virtual-cadence native attempt01 passes five numerical checks. Root independently recomputed all23 recorded no-tick interpolation pairs: unchanged authoritative tick/checksum, matching instance/Transform coordinates, maximum interpolation error4.27e-7. Twenty ready replacement-generation rows show tick0, no pending request and cleared outcomes. However, decoded OS screenshots02–05 are pixel-identical despite queued/moving/paused/reset state changes. Their visual freshness is rejected. This is not evidence that rendered controls updated correctly; actual compositor capture and a new settled run are required. The30Hz attempt02 numerical results remain bounded evidence, with stale moving/reset images likewise unaccepted.

Native review also exposed raw Rust receipt formatting in ordinary order status. The reviewed narrow38 amendment replaces it with cached singular/plural queued/submitted wording and readable admission results while retaining typed identities in diagnostics/trace. No command semantics change. Product source is approaching freeze for the next combined gate; no editor work or commits.

### Round 9 integrated gate accepted; next implementation round prepared

All three complete commands passed on unchanged source: workspace tests649.72s, strict workspace/all-target Clippy1.98s, formatting0.42s. `/tmp/space-scuffle-round9-workspace-{results,source-before,source-after,source-changes}.json` and full logs preserve the evidence. This integrates35 reset/storage,38 production scheduling and readable status,44 Stage1 output correction and50 checked movement. The existing art/global frame-budget failures and50 performance hold remain explicit.

Both final native compositor matrices (`/tmp/space-scuffle-r9-native-pipeline30-03` and `...144-03`) pass five numerical checks and ordinary shutdown. Root viewed all ten fresh images and independently recomputed7/48 no-tick pairs with maximum errors3.33e-7/5.21e-7. The real images show queued intent, accepted movement, changed formation, exact pause and clean reset. Earlier stale X-window readbacks remain rejected. This closes38's bounded scheduler/UI verification, not the game's art direction.

The next reviewed parallel implementation rounds are44 actual-view directional lighting,40 unified authored owner colors, and51 carrier ownership/lifecycle. Root's fourth track preserves and independently verifies50's performance candidate.40's shared owner-field API change requires precisely one mechanical editor compile-consumer adaptation (remove unused TeamId import; compare the renamed owner with already-imported PlayerId(0)). This preserves existing player0 selection semantics; no editor feature, logic, refactor or independent development is included. All other pre-existing editor changes remain untouched.

The first preservation command correctly refused the repository's known `crates/client/assets -> ../../assets` link before creating a candidate. The old snapshot materialized that internal asset tree. The reviewed tool correction records and verifies this exact internal link and materializes its complete contents into the immutable snapshot, rejecting other links. This is a preservation-tool assumption failure, not a game-code failure; no product edits are released before snapshot acceptance.


### Round 10 actual-view lighting oracle and transport review

Root independently executed the preserved lighting test02 (SHA256 `bd68eb243226a333cd0ddc26692b2866af83746ac8a5fdd93091aac83cc65282`) on AMD890M/RADV Vulkan. It passed with clean logs, ordinary renderer shutdown and unchanged shipped shader (`8e366eedc3156058e5ba20ea5f4b14d7b6aa864df7debb9bb33acbc02f0ff90b`). Actual engine light-buffer offsets0/3904 differ between the two views; layer eligibility, RGB/lux response, parent world rotation, inherited hidden/restored visibility and zero-light state pass independent expected arithmetic. Evidence: `/tmp/space-scuffle-r10-lighting-amd02.log` and `-result.json`. Root read the actual oracle implementation and confirmed it binds Bevy's prepared LightMeta buffer and calls the shipped shader helper.

The first attempt exited0 but emitted B0004 because its parent lacked inherited-visibility components. That fixture is rejected as clean verification and remains preserved. The corrected ordinary parent also tests hidden/restored propagation, observing total/eligible counts2/1,1/0,2/1. These results accept the bounded compute oracle only. Actual fragment captures, production scene transitions, retained packet behavior and integrated checks remain pending; mist art remains rejected.

Root reviewed and released51's strict capture-only three-checkpoint transport adapter: real production scenario and typed load/unload, exactly one normal scheduled publication tick per transaction, exact-current snapshot/ECS membership, fixed camera, actual media and pixel readback. The expected membership difference must be exactly the selected passenger and the publication interval must retain evidence of no pickup shatter. No boarding UI, fabricated snapshot, alternate renderer or silent fixture selection is authorized.40's actual palette consumer tests and51's expanded lifecycle tests are progressing in parallel. No new shared-source acceptance is inferred from the preserved round9 gate.


### Round 10 verification progress and rejected attempts

All five actual-view fragment matrices now pass their bounded contracts, and root inspected all15 native screenshots. Independent audits recompute actual GPU RGB/direction responses, packet identity and retained-callback behavior. The deliberately invalid-lux held-callback case correctly exits1 with its typed error, zero mist draws, retained unrelated shell, unchanged selected packet and no new submission. These are lighting/storage correctness results; the faint flat wake still fails art review.

The real production Lab→Battle→Lab transition passes in scene-lights03 using preserved probe02, actual AMD Vulkan and three fresh2880×1800 Spectacle screenshots. Root independently verifies retained light/root/camera identities, visibility, real prepared buffer response, original/returned Lab equality and normal exit0. All observed checkpoint queues contain zero mist, so this scene trace is not mislabeled a moving-fragment test. Scene01's diagnostic pipeline startup panic and scene02's off-window native-pointer acquisition failure remain rejected and preserved; the corrected probe and measured owned-fullscreen input path address their separate causes.

Final51 native02 uses independently rehashed366-file preserved source and returns the same exact3images/checksums as native01: passenger4 alone leaves rendered membership26→25 and returns25→26 at the explicit(14,14) endpoint while carrier5 remains. Root reviewed all3 images. No boarding UI, media/art quality or whole-workspace acceptance is inferred.

40's first two normal ownership matrices stop honestly at11/15 images. Native01 found premature focus verification while a real projectile was still expiring; native02 found the residual current-frame time delta could expire the next short beam. Reviewed capture sequencing corrections retain actual effect lifetimes, unchanged geometry checks and bounded timeouts. Sparse-player capture and the coordinated workspace gate remain pending.52 has independently reproduced terrain-disconnected cluster arrival on the actual map; its two-leg terrain witness blueprint is reviewed and held until the shared checkpoint is verified. No map-editor development or commits occurred.


### Round 10 ownership numeric pass, visual rejection and declared retry

Ownership normal03 and sparse01 each complete15 actual AMD screenshot callbacks, clean exit0 and unchanged assets. Root independently decodes all recorded player identities, authored sRGB/linear colors, ring/effect materials, retained effects through Lab/Return and reset provenance. Normal actual players0/1 and sparse actual players2/7 pass; authored Lab preview roles stay separate. Root inspected all15 normal images and the first9 sparse images. Prior lifecycle failures remain preserved.

Numerical success is insufficient: the projectile and lightning are obscured by a showcase shell/wake, and lightning can legitimately be transparent during a shader strobe phase. Root rejects their visual acceptance and approves only the documented capture refinement: derive a clear foreground stage from actual propagated shell/mist bounds, retain camera distance14 and require12 physical pixels of projected separation. Both scripts declare17 checkpoints with the SAME lightning event sampled at40/80/120ms, preserving every OFF image. No shader, lifetime, material asset or gameplay change is authorized by this retry. Actual screenshots still decide visibility; bounds alone cannot certify fragment coverage.

44 Stage2 lighting and51 transport sources are frozen pending the coordinated workspace gate. The source fingerprint includes actual `assets/captures/*.ron` scripts. Parallel docs-only review continues for44 CPU reducer admission and52 terrain-aware cluster arrival. Mist/art remains rejected; no editor development or commits.


### Round 10 final ownership verification and integrated gate start

Preservation05 contains466 independently rehashed source/asset inputs and unchanged before/after inventory, SHA256 `fa83a99586ba993e1a8f0abac257b21299f61fcabd290fd2bbf04949d4a342e7`. Actual binary SHA256 `8ce4d5e70e908690dd918b73db6ba0bf075fe48b09f7ab5dffa5e18b879aa7df`. Normal05 and sparse02 complete all17 actual AMD captures with clean exit0 in55.52/48.82s, unchanged assets and ordinary renderer shutdown. Root independently audits all17 identity/material/time/geometry records in each. Exact lightning elapsed values40/80/120ms refer to one entity/material/owner/seed and the real180000007ns lifetime, unchanged stage/camera and40ms increments of the actual shader-time source. Independent effect/shell-wake gaps range74.95–156.91physical pixels.

Root directly inspected all8 new normal effect images. Projectile is clearly separate; lightning40ms is OFF,80/120ms are visible and unobstructed. All8 sparse effect images are byte/pixel exact to these inspected images. First9 normal images are exact to earlier root-viewed normal03; first8 sparse images are exact to earlier root-viewed sparse01. Root separately viewed the changed sparse Swarm image:2060 central-overlap pixels differ from sparse01 (maximum147),2450 differ from final normal (maximum153). The two declared unobstructed role representatives remain visible with independently verified identity/material. Full swarm-image equality,200-readable-actor coverage and art quality are not accepted. This variance is retained for separate read-only diagnosis.

Native04 remains a real failed run: after its first screenshot, RON rejected newly recordedu128 nanoseconds, exit1/20.90s. Approved narrow checked-u64 conversion now rejects overflow rather than truncating; actual >u32-nanosecond fingerprint RON round-trip and overflow tests pass. Focused capture library tests28, strict17 script tests7, strict client all-target Clippy and workspace formatting pass before final preservation. No shader, effect lifetime, seed or media was changed to obtain favorable visible phases.

Root starts the coordinated full workspace gate on frozen source using `/tmp/space-scuffle-round10-workspace-gate.py`. Fingerprints cover all runtime assets as well as Rust/Cargo and patched Naga inputs.44Stage2 and51 are frozen;52 and44CPU3 blueprints are approved but implementation remains held until the gate passes. Mist, media provenance, broad visual quality and the new Swarm-overlap variance remain unresolved. No map-editor development or commits.


### Round 10 first integrated gate rejected; test-only repair and retry

The first full command exits101 after423.14s at `simulation_diagnostics_tests::live_sync_reports_and_stops_on_invalid_simulation_rate`: its manually assembled app lacked the new required `PlayerPalette` resource, causing a system-parameter panic before its intended diagnostic assertion. Strict workspace/all-target Clippy passes8.53s and workspace formatting1.57s; all448 gate input hashes remain unchanged. The failed gate/logs stay at `/tmp/space-scuffle-round10-workspace-*`; it is not an accepted workspace checkpoint.

Root approved/reviewed the narrow fixture amendment: load `PlayerPalette` from the checked actual asset root with the actual catalog showcase count, initialize normal `PresentationStatus`, and keep every original invalid-rate/error/UI/stop-catch-up assertion. A private same-file helper contains only real fixture loading. No production optional-resource fallback, default palette, new actor identity or assertion weakening was introduced. The focused actual test passes1/1 in17.90s, targeted strict Clippy and workspace formatting pass. Root independently verifies this existing test file is the sole changed gate input. Native05/sparse02 product-source evidence is unchanged by this test-only correction.

The second coordinated gate runs under `/tmp/space-scuffle-round10b-workspace-*` on frozen source.52 and44CPU3 remain held pending a complete pass. Parallel docs-only53 diagnosis records a plausible view-space-depth tie/parallel visibility insertion issue for the Swarm overlap; StageA actual-phase evidence is approved only for post-gate release, and StageB correction remains conditional on causal evidence.


### Round10 second gate: preserved timing failure and bounded diagnosis

Gate10b exits101 after589.95s:579 tests pass, one fails across69 completed harnesses, none ignored. The sole failure is the existing sim_map100-query wall-time gate:269.776963ms exceeds the unchanged250ms debug limit;83 queries succeed and the concurrently running1,000-query deterministic checksum passes. Strict workspace/all-target Clippy and formatting pass;448 input hashes remain unchanged. Full workspace acceptance is withheld.

Root ran a predeclared six-process diagnostic using the SAME existing test binary, alternating solo/pair/pair/solo/solo/pair, preserving every result at `/tmp/space-scuffle-r10-path-timing-diagnostic01`. Solo path times99.80/113.18/111.30ms; paired128.21/114.37/122.27ms. All report83 successful queries and pass; vision59.18–69.40ms also passes. Paired runs include the actual deterministic checksum. CPU usage, host load and CPU pressure are retained. Read-only review confirms all18 sim_map files and workspace Cargo inputs match preserved pre50/post50 snapshots. Unrelated host workloads were active; contention is a plausible inference, not proven historical cause. No process was stopped, performance threshold changed, test ignored, workload reduced, or source optimized without evidence.

One new unchanged full gate10c is authorized with host-load/pressure observations retained. This is a new verification attempt, not relabeling the failed10b run.44CPU3,52 and53A remain held until complete acceptance; other rounds continue documentation/review. Mist/art remains rejected.


### Round10 independent mist field review during the source freeze

A separate root mathematical experiment and independent scalar/analytic review expose a limitation in the proposed future summed, clamped-age volume model. A straight2r source with32 edges, affine age/path and a single reduced edge has exactly zero geometric/attribute approximation error and zero arc deficit, yet the proposed field loses about5.176% integral mass. At one independently checked broadside ray, transmittance changes0.836810→0.996490. The cross-sectional and ray kernels were integrated analytically; separate trapezoidal/Simpson refinements agree. This is a mathematical special case, not a legal-motion reproduction, shader/GPU test, screenshot or defect claim about the current ribbon.

Evidence and scope are recorded in `round10-mist-field-subdivision.md` with both independent scripts/hashes. The result rejects admitting field fidelity from CPU control tolerances or arc deficit alone.44CPU3 remains a control-approximation prerequisite with unchanged scope/budgets; field design and actual legal-history/temporal/radiometric review remain mandatory before shader implementation. No density normalizer, tolerance relaxation or art acceptance is introduced. Product source remains frozen for10c.


### Round10c accepted integrated checkpoint; round11 implementation release

The unchanged full retry passes all three mandatory commands: `cargo test --workspace`688 passed across88 harnesses,0failed/0ignored,551.226s; strict workspace/all-target Clippy0.601s; formatting0.967s. Root independently parsed all results and rehashed448 inputs: before, after and current are identical. Evidence `/tmp/space-scuffle-round10c-workspace-independent-audit.json`, logs/results and host observations are retained. Earlier10/10b failures remain preserved. Existing dependency warnings and editor-only Vulkan presentation warnings remain in the logs; this is not a global renderer-clean claim.

This completes integrated acceptance for40 bounded player presentation,44Stage2 actual-view light binding/admission, and51 transport ownership under their separately documented limits. Native actual-image evidence remains unchanged by the test-only fixture repair. Media/art/mist quality, whole-frame performance, Swarm pixel variance and future-volume field fidelity remain unresolved.

Round11 has four parallel goals:52 correct terrain-disconnected cluster completion;44CPU3 implement and admit the bounded history-control reducer;53A gather actual transparent-shell ordering/source evidence;root independently review mathematics/integration and verify resulting artifacts.52 and53A implementation are explicitly released against their approved blueprints.44's original pure reducer implementation is released; any proposed authoritative-trace reuse amendment to its runner still requires review before replacing the original `step_into` contract.53B ordering correction and all volume shader/field code remain held. Each lane will move from implementation into focused verification, then a coordinated frozen-source full gate. No map-editor development or signed commits.


### Round11 terrain correction independently checked; renderer and mist work continues

52 implements only the reviewed checked-witness core, its two-leg terrain call and focused tests. Root diffed both product files against hash-matched10c preserved copies. Focused verification passes92 existing/regression tests, six distinct new integration tests, four helper tests and strict simulation-crate Clippy. Owned formatting passes; workspace formatting remains pending active client lanes.52 source is frozen pending the combined gate.

Root independently audited all7×1024 new external trace ticks and2×16 same-side-witness ticks. The harness sources differ from their preserved originals only in output directories. The three affected ground-group traces now move at1, follow the alternate route reaching y8.5, and clear their first order at77; final distance to the occupied goal is0.471911 world units. Both same-side-witness traces retain their orders and move throughout16 ticks. Solo Move/AttackMove, Air and flat-group complete traces remain exactly equal to their baselines, including checksums. Every checked new tick reports no diagnostics and equal direct/published checksum. Actual terrain-edge legality and supply invariants come from the reviewed Rust tests/harness, not reconstructed floating-point JSON. Evidence is in both round11 evidence directories' `root-independent-audit.json` and `round11-terrain-connected-arrival.md`.

44CPU3's initial pure reducer tests pass, but root review rejects an unplanned support-cut fraction search that changed the reference used by its own checks. The approved correction retains the ideal f64 boundary and reports exact reconstruction failure; any legal-motion representability problem remains a visible admission failure requiring review. Full corpus reuse has been explicitly approved: genuine once-generated per-family/speed world outputs are replayed through actual time/clock/timeline components, with direct-world parity checks and every16,296 planned current-catalog window accounted for. Reused checksums are provenance consistency, not independent repeated simulation evidence.

53A remains diagnostic-only. Root approves a passive opt-in selected-packet witness and separately labeled two-real-shell fixture, without ordinary upload/order changes. Review identifies and requires guarding actual queued-but-unpublished pipeline IDs before cache access, preserving the existing startup distinction established by44. No screenshot or causal acceptance exists for53A yet.54 begins an independent docs/math architecture review of candidate field representations; no field/shader implementation or budget relaxation is released. No map-editor development or commits.

### Round11 field investigation and diagnostic evidence review

Root accepts54 as an advisory investigation only. Its affine-extension candidate changes source-field semantics to the previous coarse result; its source-integrated convolution candidate has additive straight-partition semantics but unresolved quadrature cost. Both retain the separate near-closed-curve mass-loss failure. Neither is selected or authorized for production. Root read the full plan and mathematical script, verified its SHA256, and exactly reproduced12 axial probes and six convolution values without modifying the original artifacts. This reproduction is not a second independent mathematical implementation. Evidence: `/tmp/space-scuffle-r11-field-representation54/root-reproduction-audit.json`. The independent reviewer now examines44's pure reducer while its owner completes the genuine-world trace runner.

Root reviewed53A's actual phase, source, interval, fixture and selected-packet witness code. Checked published pipeline lookup and binary source association replace the identified unsafe/index-linear approaches. The documented compact metadata/raw-sidecar storage refinement is approved with separate64MiB ceilings, exact cached-content comparisons and explicit additional working-memory costs; neither ceiling is a process RSS claim. Review requests consistent asset accounting in incomplete artifacts and preservation from the direct screenshot-callback failure path. Actual native captures, causal diagnosis,53B ordering changes and visual acceptance remain pending.52 simulation source stays frozen for the combined gate.

The independent reducer reviewer found two additional pure-API defects: a support cut following exact duplicate samples named the wrong original outgoing edge, and subtraction could round a positive support interval entirely away at extreme scale ratios. Root approved narrow provenance tracking and typed nonprogress refusal; neither changes the ideal reference or legal-motion tolerances. Static corrections and independent arithmetic evidence are recorded in `round11-reduction-independent-review.md`; updated Rust execution and full corpus admission remain pending. Root also requires per-window distinct failure categories, failed-pass work counts and signed deficit ranges so an early reduction error cannot conceal a separate discontinuity or coverage failure.

53A's focused CPU evidence now passes eight protocol/real-sort tests, two selected-packet tests and one asset-inventory test. Missing queued-source associations must classify the result as limited even when the observed main-world table is unchanged. Failed callback artifacts must preserve the same available evidence and accounting as other failures; final compact encoding has a separately checked file-size cap. Root approves the declared two-shell experiment using actual Swarm logical IDs1000/1100 at Bevy centers(15.75,1.5,10)/(16.25,1.5,10), camera distance10 in both orders. Natural captures retain distance32. Actual equal-depth bits, readiness, frustum and screenshots remain required. Review catches and requires correcting the fixture's material-center coordinate mapping to match ordinary `BubbleUniformGpu` conversion. No53B order-policy change or GPU/native acceptance yet.


### Round11 preserved native matrix and full mist-history corpus

53A's predeclared twelve-run matrix uses one independently rehashed488-input snapshot and actual AMD Vulkan binary `3ea83f153d3fa6d73ceadf43d981408fc3f0382b3b4c92d6e105a2d885f5ba13`. The first three observer-off runs each complete all17 captures with exit0; root independently checks their ownership/material/time contracts. Natural01/02 retain six consecutive ready epochs with identical source/view/mist-byte evidence but genuinely varying logical shell order. Both controlled orders retain stable logical sequences and source payloads; their existing classification is nevertheless inconclusive because raw temporary render IDs change each epoch. The pinned renderer creates and deletes these proxies each frame. Independent review in `round11-shell-phase-identity.md` proposes an explicitly validated temporary-proxy relation; the original reports lack that marker/association proof and remain unchanged. The frozen matrix continues without a favorable retry or source amendment.53B remains held.

Root directly inspected the first Forward/Reverse screenshots: reversing the declared two-shell queue changes2666 pixels, maximum channel difference133, entirely inside their overlap bounds[1117,390,1162,473]. This is controlled overlap evidence, not retrospective identification of a screenshot's private GPU-copy epoch. Repeat2 Forward is byte-exact to the first image. Root also viewed Natural02 and Off03: the arena still obscures many actors and legacy scene media remains visually unsuitable. No art or200-readable-actor acceptance follows from numeric stability.

44CPU3 passes31 pure/runner tests, six actual-catalog integration tests, strict client Clippy and formatting. Its corrected ramp fixture observes genuine height3→6 on both tested movement layers; the prior world/text-row coordinate mistake and its failed coverage runs remain preserved. Corrected Blink fixtures use real Stop/Cast command bundles and observe paid energy/cooldown, exact relocation and the first actual cross-discontinuity history. No gameplay or emitter correction is included.

Root independently rehashed489 preserved inputs, the internal relative asset link, actual release compiler records and binary `47bd1468033d0256493090cbdce0e8b93c573176ba22e30afc76fd9763488391`. All workspace compilation records name the preserved manifest paths and report fresh=false. The full16,296-window corpus is running without sampling. Early actual zigzag histories already fail the17-control capacity, and separate exact support reconstruction and discontinuity failures are retained. These are explicit admission failures; the runner continues every declared window and no tolerance, budget or source geometry is changed to obtain a pass. Final aggregate accounting and the coordinated workspace gate remain pending.


### Round11 sealed order experiment and independent capacity bound

The declared12 native runs are sealed in `tmp/verification/shell-audit-r11-01/matrix-results.json`: three observer-off runs complete17 goals each; nine observer-on runs retain9 images each and their original inconclusive exit1. Root decodes all132 PNGs. Every Forward repeat is exact; every Reverse repeat is exact; the two orders differ only inside the measured overlap. All three natural runs show real logical-order variation across their six ready epochs, even in the separately labeled offline projection that removes raw temporary render IDs. Source/view/mist payloads otherwise remain equal within each run, and raw queued-to-sorted payload multisets match. Root's all-run projection reuses the independent reviewer's algorithm and is a reproduction, not a third algorithm. All remaining changed natural/off Swarm images were directly viewed; unrelated frames match previously inspected images.

Root approves the completed Plan53 temporary-proxy amendment for observer-only implementation, with actual marker/visible-pair/material-draw proof and unchanged raw evidence, budgets, readiness and interval rules. A new source checkpoint and declared native verification must follow. Ordinary shell-order correction remains separately held.

Root's exhaustive source-vertex graph for the first complete ControlBudget artifact finds a minimum19 controls against17 allowed. A separate80-digit Decimal implementation and breadth-layer reachability confirm all231 candidate pairs, the same25 feasible edges and the same minimum. Largest accepted error ratio0.0106676772 and smallest rejected1.0153993307 separate the result from floating-point boundary ambiguity. This bound applies to the declared original-vertex representation, not arbitrary controls or visual equivalence. Evidence is in `round11-reduction-control-capacity.md` and both preserved scripts/results. The full corpus continues unchanged; this design is not admitted.

Root reopened the actual concept image and the preserved actual-lit Close capture. The current wake remains nearly unreadable against bright terrain, while the reference has visible depth, directional body and tapered separation. Numeric source/lighting correctness does not meet that visual target. Plan55 remains planned: native continuity verification will use explicitly labeled playback of genuine authoritative outputs through the production renderer. The normal command queue stays unchanged; general ability-target/effect admission is not silently expanded by a mist test. No map-editor development or commits.

### Round11 completion request: no further rounds

The user requests finishing the running rounds without starting new ones. All
three delegated lanes close their current implementation, native evidence and
independent reviews. The already-running frozen-source workspace gate continues;
53B,55, the diagnostic shadow-view label correction and all further mist
representation/field experiments remain unstarted.

The full CPU admission corpus is complete:16,296 windows,13,018 passed and3,278
failed, with all6,885,592 attempted display updates completed. Independent audit
reconciles the exact catalog-derived inventory, preserved source/binary/input
bytes and retained trace checksums. Representation admission remains rejected.
The amended observer's eight-run matrix is also sealed: Off and six controlled
runs complete17 goals each; Natural retains its genuine logical-order variation
and inconclusive status. Root independently checks actual proxy associations and
raw sort permutations, all128 decoded PNGs, exact repeat images and the changed
Swarm views. No mist or general art acceptance follows.

The final scope, artifact links, audit limitations and integrated gate outcome
are recorded in [round11 closeout](round11-closeout.md). Planning documents are
retained as future proposals, not implementation approval. No new rounds or
commits are started.

The final round11 gate passes:759 tests across91 harnesses,0failed/0ignored,
828.929s; strict workspace/all-target Clippy1.330s; formatting0.576s. Root
independently rehashes472 inputs and confirms before/after/current equality.
Original dependency/window/presentation diagnostics remain preserved. This
accepts the bounded terrain correction and diagnostic implementation; it does
not admit the rejected mist representation or art. All lanes and the gate are
finished. Work stops at the user's requested boundary.
