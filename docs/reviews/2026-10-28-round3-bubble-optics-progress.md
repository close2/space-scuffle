# Bubble optics — implementation and actual verification

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: production shader implemented; full visual acceptance remains pending.

## Color contract and implementation

The actual media loader creates `Rgba8UnormSrgb` arrays, so sampled RGB is already linear. Default cameras are non-HDR (`Camera::default().hdr == false`), and the material uses straight-alpha blending. Locked Bevy 0.15.3 applies `tone_mapping(color, view.color_grading)` under `TONEMAP_IN_SHADER`; the sRGB target encodes output. The replacement follows this conditional contract and leaves HDR tonemapping to the camera. It removes the unexplained media gamma lift and unbounded broad additive shell/hit gains.

The shader uses bounded reflection/transmission weights, a compact actual-scene-light glare lobe, narrow tactical signals and localized hull/shield damage responses. The real media, animation flags, construction, cracks, cloak, selection and attacking capabilities are preserved. Unit-agnostic settings load strictly from `assets/catalog/bubble_optics.ron`; numeric validation reports the named invalid field. Four additional uniform lanes extend the existing 96-byte prefix to 160 bytes without changing old offsets or public material fields.

`BubbleOpticsPlugin` synchronizes authored lanes after snapshot updates and transform propagation. It reads actual light `GlobalTransform`, including parents. Root's review caught that Bevy `Assets::iter_mut()` queues modifications for every visited asset: this was corrected to query attached material handles, compare through immutable lookups, and obtain mutable access only for changed lanes. Shared handles therefore cause one update, not duplicate updates. No media handles or snapshot signals are changed.

## Focused results

- `/tmp/space-scuffle-r3-optics-tests.log`: five tests pass: shipped style and uniform offsets; malformed/bounded inputs; actual schedule after snapshot replacement and late material attachment; no spurious modification events with shared handles; parented directional-light world orientation.
- `/tmp/space-scuffle-r3-optics-clippy.log`: targeted client library and optical integration test Clippy passes with warnings denied.
- `/tmp/space-scuffle-r3-optics-fmt.log`: workspace formatting check passes at this checkpoint.
- `/tmp/space-scuffle-r3-optics-build.log`: real capture executable built with the 160-byte material layout. Old 96-byte binaries are incompatible with the current shader and must not be used.

The orchestrator owns the final coordinated workspace gates. The preceding shared checkpoint passed Clippy/formatting but its runtime integration test process aborted during renderer teardown after assertions passed; that is not a successful workspace test gate. The renderer track is repairing teardown and compiler validation separately.

## Actual native captures

The capture harness now requires the actual bubble pipeline, supports shell sphere/dome selection through catalog shape/media capabilities, uses catalog radii, and supports healthy/selected/attacking/shield-hit/hull-hit/recovering/damaged states plus HDR and scale variants. Readiness resets both virtual and generic presentation time to eliminate machine-dependent shader pulse phases from warmup. The next executable rebuild also records actual virtual seconds, optical state, HDR and radius scale in metrics.

- `/tmp/space-scuffle-optics-v1-sphere/Shell-1-fps60-msaa4.png`: actual pipeline rendered, shell ROI has 0/9908 near-white pixels. **Fixture rejected for creature evaluation**: the first generic catalog entry's media is a full concept scene containing multiple bubbles. This is an actual loaded catalog image, not a manufactured stand-in, but it cannot prove creature readability. The sphere selector now requires existing `showcase_hero`, matching the real mist close-view subset; no identifiers are hardcoded. The image also suggests overly dark glass and broad glare need review on proper hero media.
- `/tmp/space-scuffle-optics-v1-dome-hit/Shell-1-fps60-msaa4.png`: actual peak shield-hit dome retains clear building media and a localized cyan ripple instead of full-body white clipping. Shell ROI has 0/5972 near-white pixels and inner display-luminance standard deviation 39.02. This is positive evidence for the bounded hit response, not completion of all optical acceptance cases.

CSV measurements use projected ray/sphere intersections, exclude terrain-hidden dome pixels and the antialiased outer edge, and record shell saturation plus inner-media variation. Gameplay overlap requires manual review; these numbers do not certify aesthetics or full shader/driver correctness. Actual native driver validation diagnostics remain separate from successfully rendered PNGs and belong to the renderer compatibility round.

Remaining: same-hero close comparison, healthy/hit matched sphere and dome sequences, selection/attack/damage/recovery, HDR/non-HDR, multiple radii/cameras, gameplay and attacked-building production capture. Then re-evaluate mist on pale and contrasting terrain with real cliff intersections; removing shell clipping does not itself resolve rejected mist quality.

## Subsequent iterations and acceptance corrections

- V2 `/tmp/space-scuffle-optics-v2-close/Close-1-fps60-msaa4.png`: **rejected**, independently inspected. Smaller glare is preferable, but the angularly varying spectrum colors too broad a band and reads as colored plastic.
- V3 `/tmp/space-scuffle-optics-v3-close/Close-1-fps60-msaa4.png`: root inspected a much thinner pale contour and readable hero media, with clean corrected-renderer validation. This improves shell hierarchy but does not resolve the opaque media disk or overall art acceptance.
- `/tmp/space-scuffle-optics-v3-matrix/sphere-attacking/Shell-1-fps60-msaa4.png` and `sphere-shield-hit/Shell-1-fps60-msaa4.png`: **rejected**, independently inspected. Orange attack aperture and cyan shield ripple are diagrammatic rings over the creature. The bounded hull response also remains an airbrushed patch. Avoiding saturation is necessary but does not establish production art quality.
- `/tmp/space-scuffle-optics-v3-matrix/dome-healthy/Shell-1-fps60-msaa4.png`: independently inspected. Actual building media remains readable and the contour no longer clips; full state/multi-scale quality is still under review.

Root inspected the source PNGs and found fully opaque alpha throughout the tested hero/building/worker images. The shader must not blindly luminance-key dark creature pixels or claim physical transparency. The catalog worker entry containing a full concept scene is a separate runtime media-quality defect; excluding it from showcase comparisons does not fix or hide that defect in the game.

Visibility correctness now follows actual `ViewVisibility` after Bevy's visibility check and excludes unrelated render layers. The five optical tests pass with brighter hidden/unrelated-layer lights included in the parented-light regression (`/tmp/space-scuffle-r3-optics-tests-v3.log`). This round's product source is frozen for coordinated workspace gates.

The original harness incorrectly allowed native close to return success after fewer than three screenshot callbacks. Two matrix cases are rejected as incomplete: sphere-hull-hit saved 2/3, dome-recovering saved 1/3. The approved amendment adds an external atomic per-image completion mask and a typed error for otherwise-successful incomplete runs. All three guard unit tests pass (`/tmp/space-scuffle-r3-optics-completion-tests.log`), and the fixed executable is built (`/tmp/space-scuffle-r3-optics-completion-build.log`). Real interrupted-run verification and clean complete reruns remain pending; existing partial artifacts are not reclassified as successful.

## Completion guard and layout repair verified

Root's verified-PID native interruption at `/tmp/space-scuffle-optics-v3-interrupted-guard` exits 1 after 1/3 PNGs with the typed incomplete-capture error. Fresh complete sphere-hull-hit and dome-recovering reruns at `/tmp/space-scuffle-optics-v3-complete-reruns` both exit 0 with three PNG/CSV pairs. The recovered dome's ring/soft-patch art remains rejected; functional completion is now correctly distinguished from visual acceptance.

The full gate exposed a stale headless 96-byte assertion. Its docs/assertion now match the 160-byte ABI, and a new test parses the **actual shipped WGSL BubbleUniform declaration** with the same vendored Naga compiler, comparing every member offset and total span to Rust `offset_of!`/`size_of!`. The existing complete native shader pipeline evidence remains necessary; this declaration-layout test does not replace it. The dev-only version dependency resolves through the existing root patch to `third_party/naga`, requiring no new packages. A direct dev path dependency was rejected because workspace formatting followed the published vendor crate's unavailable test paths; using the root patch preserves the intended workspace boundary.

`/tmp/space-scuffle-r3-optics-layout-repair.log`: six optical tests and all five headless tests pass. `/tmp/space-scuffle-r3-optics-layout-clippy.log`: strict targeted Clippy passes. Final coordinated workspace gate remains the orchestrator's next checkpoint.

`/tmp/space-scuffle-r3-optics-naga-dependency-check.log`: offline client test-target compilation passes after the dependency declaration repair. `/tmp/space-scuffle-r3-optics-naga-fmt.log`: exact `cargo fmt --all -- --check` passes. Offline dependency inspection confirms one patched Naga 23.1.0 shared by the client tests, Bevy and wgpu.
