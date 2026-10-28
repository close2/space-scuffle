# Round 3 — Bubble optical hierarchy and output color correctness

Status: architect blueprint; implementation requires orchestrator approval.

## Goal and observed failure

Restore readable glass shells and embedded real media at close and gameplay scales. The controlled native images `/tmp/space-scuffle-mist-close-v2/Close-1-fps60-msaa4.png` and `/tmp/space-scuffle-mist-gameplay-v2/Gameplay-1-fps60-msaa4.png` show broad clipped white annuli. Root's actual battle captures show damaged domes becoming almost entirely white. These fail the concept's hierarchy: creature first, thin glass reflections second, restrained tactical signals third. This is a separate round from mist geometry and does not authorize editor work.

The production shader currently lifts sampled media with `pow(rgb, 0.9) * 1.45`, adds Fresnel times 3.2, broad studio glare, facing accents, full-body hit gains reaching 6.8, and selection gain 3.5 before returning raw color. It has no `TONEMAP_IN_SHADER` branch. Locked Bevy 0.15.3 `bevy_pbr/src/render/pbr_functions.wgsl::main_pass_post_lighting_processing` applies `tone_mapping(color, view.color_grading)` for non-HDR cameras; the render target then performs the sRGB conversion. Consequently there are two independent defects: uncontrolled accumulated energy and an incomplete custom-material color-output contract. Fix both; uniformly lowering the camera exposure does not restore hierarchy.

## Ownership and integration

Own `assets/shaders/bubble_dome.wgsl`, new `assets/catalog/bubble_optics.ron`, new `crates/client/src/bubble_optics.rs`, focused additions to `bubble_shader.rs`, and `crates/client/tests/bubble_optics_tests.rs`. Coordinate narrow `lib.rs` module/plugin registration and its uniform-size assertion with the runtime owner. Update only affected layout assertions in `round2_visual_tests.rs`. Extend the existing real capture harness in `mist/capture.rs` with shell-only case choreography and output metrics, preserving existing mist cases.

No changes to catalog creature identities, texture/video assets, terrain lighting/exposure, map editor, simulation, shell topology, or scene synchronization behavior. Do not change `BubbleDomeMaterial` public fields: editor and other callers construct this material directly. A uniform extension with defaults is compatible with those constructors.

## Implementation design

1. Audit the locked renderer before editing. Record actual texture format (`Rgba8UnormSrgb` means hardware sampling already linearizes), material alpha mode, camera HDR settings, target format, and tonemapping specialization flags in the report. Use primary locked source rather than assuming current online Bevy examples exactly match 0.15. Do not gamma-correct sampled video twice.
2. Introduce unit-agnostic `BubbleOptics` settings: media exposure in stops, base dielectric reflectance, Fresnel width/exponent, reflection strength, glare roughness/strength, hit strength/angular spread, selection strength, and shell alpha. Each parameter has documented units and finite bounded validation. A typed error distinguishes read, parse, and invalid-field errors. Load the actual RON through `ResolvedAssetsRoot`; malformed/missing authored configuration exits explicitly in production. A default exists only for existing embedded API construction, not as a silent failed-load fallback.
3. Pack settings into four additional `Vec4` lanes of `BubbleUniformGpu` (160 bytes total, 16-byte alignment). Name/document lane components identically in Rust/WGSL and test offsets using safe `offset_of!`. Existing snapshot `from_instance` initializes optical defaults. `BubbleOpticsPlugin` applies the loaded lanes to existing material uniforms in `PostUpdate`, after `Update` synchronization and before render extraction. Compare lanes before mutation, iterate materials once, allocate nothing per frame. This reaches game, Visual Lab, and the real capture harness without modifying editor code or threading a new parameter through every scene constructor.
4. Keep media in linear color with a single intentional exposure adjustment. Build a narrow Schlick-inspired rim from the view normal; blend reflected light and transmitted media using complementary bounded weights. Separate a compact glossy highlight from the broad Fresnel term; use normalized intensity and controllable roughness instead of additive full-disk glare. Use the existing team hue and real scene directional light where practical; no unit-specific lookups or magic per-creature fixes.
5. Replace full-body damage flash with a bounded local response. Hull hits use the supplied hit direction and a smooth angular lobe; shield hits may use a traveling annular ripple because their snapshot has no impact direction. Neither response may erase the whole media disk. Selection and facing signals remain confined to thin rim regions with bounded coverage. Preserve selected, attacking, cloaked, construction, and damaged state semantics.
6. Compose linear scene color first. Under `TONEMAP_IN_SHADER`, import the locked Bevy tonemapping function and apply it once to the final straight-alpha color using `view.color_grading`; otherwise leave linear HDR color for the camera postprocess. Preserve alpha through mapping. Let the sRGB render target encode output; never hand-apply a second gamma curve. If debanding is needed, use the locked engine's documented order rather than inventing one. Keep final alpha/blend convention consistent with `AlphaMode::Blend`.
7. The shader remains an art-directed glass/media treatment, not a claim of physically exact refraction. Reflection/transmission budgeting is the engineering control that prevents runaway sums; mandatory visual evaluation decides the authored values. No post-hoc hard clamp of all channels as the primary fix.

## Verification round

### State and interface contracts

- Load the shipped optical RON using the exact production loader; reject nonfinite/out-of-range values, unknown fields, and missing assets explicitly.
- Verify 160-byte size, 16-byte alignment, old field offsets unchanged, and all optical-lane offsets. Assert snapshot-created defaults and the plugin's actual material update agree with the loaded settings.
- Exercise the real plugin under its actual schedule: creating materials before and after startup, replacing snapshot uniforms repeatedly, and keeping media handles/state fields intact. Assert steady entity/material counts and no scene reset dependency.
- Headless tests validate state contracts only. Source-substring shader assertions are not evidence of color correctness.

### Actual GPU images and pipeline contracts

Reuse the production asset loader, catalog-selected mobile and building archetypes, production material/shader, terrain, camera, and screenshot callback. Capture physical 1280×720 with scale override 1, fixed camera/time and known lighting. Await real bubble pipeline readiness with bounded timeout; distinguish retryable missing imports from terminal compile errors. Preserve native driver diagnostics separately from shader readiness and screenshot success.

Capture unmodified healthy, selected, attacking, peak shield hit, peak hull hit, recovering hit, and low-health states, with multiple catalog radii and both spheres/domes. Hold real media frame constant for same-state before/after comparison. Include a second oblique camera and gameplay-scale mixed groups. Capture non-HDR and HDR camera variants to verify exactly one tonemapping stage; do not change the main game's camera defaults to conceal the bug. Real battle captures must include an attacked building after integration.

Store each case's projected shell ROI and inner-media ROI with the PNG and metrics. Exclude antialiased outside/background pixels. Measure near-white saturation (`all RGB >= 250`) and compare with the same sampled-media baseline so naturally white source art is not mislabeled as a shell failure. Initial rejection thresholds: a connected saturated shell band wider than 5% of projected radius, over 3% saturated shell area in healthy state, or peak-hit inner ROI losing over 20% of its luminance variation relative to healthy. These are guardrails, not automatic acceptance; if source art makes a metric invalid, record the reason and replace the metric before accepting.

Manually inspect every state sequence against the concept and previous rejected capture. Reject broad neon rims, chalky/opaque glass, washed-out media, flickering hit outlines, lost team/selection readability, or glare that competes with the creature. Compact bright glints are allowed. Review close and gameplay captures at native size; a pretty enlarged crop alone does not pass. Re-run close/gameplay mist captures after this change because optical hierarchy affects wake evaluation.

Run targeted optical tests, then coordinated `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --all -- --check`. Save logs and image evaluations in `docs/reviews/`; record native renderer validation findings without suppressing them.

## Primary references

- Locked Bevy 0.15.3 `render/pbr_functions.wgsl`, `render/mesh.rs`, and `bevy_core_pipeline/src/tonemapping/tonemapping.wgsl` are the implementation authority for specialization, tone mapping, blending and output space.
- [Bevy tonemapping example](https://bevy.org/examples/3d-rendering/tonemapping/) demonstrates conditional custom-shader tone mapping; its current syntax must not be copied blindly into the older locked version.
- [Filament rendering theory](https://github.com/google/filament/blob/main/docs/Filament.md.html) documents linear-light material inputs and reflectance/energy conventions. Use these principles to bound the art treatment while retaining the game's stylization.

## Verification amendment — Native close must not masquerade as completed capture

Status: narrow amendment awaiting orchestrator approval; permitted repair to acceptance evidence during the source freeze only after approval.

Observed failure: `/tmp/space-scuffle-optics-v3-matrix/sphere-hull-hit.log` exits successfully after only two of three screenshot callbacks when a native window-close event ends Bevy's runner. `App::run` replaces its outer App with an empty App before invoking the runner, so inspecting `CaptureProgress` on that outer App after return cannot recover completion reliably.

Add a small cloned `CaptureCompletion` resource backed by an `Arc<AtomicU8>` bitmask, also retained outside the runner. Each of the three expected image indices owns one bit. Set its bit with Release ordering **only after** that callback has validated dimensions and successfully saved the new PNG, optional CSV and metrics. Existing files never count toward this mask. Duplicate acknowledgment of one image cannot stand in for another. Read the mask with Acquire ordering after `app.run()` returns.

If Bevy returns success but the mask is not `0b111`, return a typed `CaptureVerificationError::Incomplete { completed, expected: 3 }`; the existing binary error path logs the reason and returns nonzero. Preserve an existing runner error rather than translating it into success. Normal success requires all three fresh callback acknowledgments. This changes capture verification only, not game lifecycle or shaders.

Unit-test zero/one/two acknowledged callbacks plus successful runner status as errors, duplicate acknowledgment as incomplete, all three as success, and preservation of a non-success runner exit. Then conduct a real native interruption before the last screenshot and assert nonzero exit without three artifacts. Re-run the complete actual hull case and require three new PNGs/CSVs plus exit zero. Keep interrupted artifacts/logs as rejected evidence; do not patch their metadata to pretend completion.
