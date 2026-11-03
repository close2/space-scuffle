# Production mist round — verification record

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: implementation in progress; visual acceptance explicitly withheld.

## Completed engineering contracts

The replacement owns bounded emitter histories, reusable layered ribbon meshes, one shared material, explicit authored style validation, exact runtime asset-root loading, scene-light input, depth-tested premultiplied output, and real capture readiness/screenshot callbacks. Gameplay uses observed world motion; only the Visual Lab/close fixture uses a conveyor. Removed owners release ownership capacity immediately, allowing 400 replacements within the 512-slot budget; retained unowned samples fade only until their slot is reused. Disabling/re-enabling an emitter clears stale continuity. Ribbons are non-shadow-casters; the original solid shadow line was a real defect, not an artistic feature.

Shipped RON fixed arrays originally had invalid square-bracket syntax; the actual startup failed. Tuples fixed the real asset and a regression now loads that file. This is why synthetic configuration tests were insufficient.

Focused evidence:

- `/tmp/space-scuffle-r2-mist-style-test.log`: shipped style load passes.
- `/tmp/space-scuffle-r2-mist-lifecycle.log`: two production-plugin state tests pass, including 400-owner generations/replacement with stable mesh/material/entity counts and disabled motion continuity.
- `/tmp/space-scuffle-r2-mist-state-v3.log`: five integration tests pass (authored style, 30/60/144 Hz sampling, stop/teleport/bounded history, geometry buffer identity/bounds, invalid configuration).
- `/tmp/space-scuffle-r2-mist-capture-time.log`: exact common-rate capture target regression passes. Movement advances the fixture counter in Update; Last captures that pose. Close frames target 10/6, 12/6, 14/6 seconds, stop frames 10/6, 15/6, 20/6 seconds. Readback pauses simulation time; metrics record actual sample seconds. These targets are exact at all three supported rates.
- `/tmp/space-scuffle-r2-mist-build-v3.log`: actual preview binary builds. Existing v3 images predate the final capture-time correction; do not use them as the final cross-rate timing comparison.
- `/tmp/space-scuffle-r2-mist-clippy-v3.log`: mist lint findings fixed; that invocation failed on another concurrently edited runtime integration test. Coordinated workspace gates are owned by the orchestrator and not yet a claim here.

## Actual GPU evaluation

Root runs native Vulkan captures because this worker cannot open the native display. The actual production shaders, terrain, media, catalog and loaders are used; no mock renderer or replacement assets. Physical output is 1280×720 with scale override 1.0. Cold renderer initialization can exceed 30 seconds; callbacks wait for real pipeline readiness rather than a fixed early screenshot delay.

1. `/tmp/space-scuffle-mist-close-v1/Close-1-fps60-msaa4.png`: **rejected**. Poor camera composition, faint wake, conspicuous solid dark ribbon shadow. Fixed camera ground height/framing and non-shadow-caster behavior.
2. `/tmp/space-scuffle-mist-close-v2/Close-1-fps60-msaa4.png`: **rejected**. Correct framing and shadow behavior, but wake is a faint flat cyan wash with no convincing flow/depth.
3. `/tmp/space-scuffle-mist-gameplay-v2/Gameplay-1-fps60-msaa4.png`: **rejected**. CPU metrics report nine visible histories, but motion directions cannot be read from the actual image. CPU visibility is not a visual pass.
4. `/tmp/space-scuffle-mist-stationary-v2/Stationary-1-fps60-msaa4.png`: controlled no-motion reference with matching camera, terrain and media time.
5. `/tmp/space-scuffle-mist-close-v3/Close-1-fps60-msaa4.png`: **rejected**. Optical-depth density, coherent erosion and separate filaments improve coverage but still read as flat haze. Close ROI x300..700/y250..440 mean absolute RGB difference against stationary rises 0.77→1.66 levels, with pixels differing over eight levels rising 3,798→11,303 out of 76,000. These are diagnostic comparisons, not quality thresholds or acceptance.

Locked Bevy source excludes premultiplied materials from the opaque depth prepass, so mist does not self-occlude that texture. Its blend state is `PREMULTIPLIED_ALPHA_BLENDING`; the custom shader applies coverage once. Reverse-Z depth reconstruction uses the locked engine helper for both sampled scene depth and fragment depth. This source audit supports the contracts but does not replace a rendered depth/coverage diagnostic if further uncertainty remains.

Native captures continue to record existing Vulkan validation diagnostics (built-in shader workgroup/atomic semantics and surface synchronization). Exit-zero PNG production does not establish a clean driver-validation result. Those diagnostics remain tracked independently; none were suppressed.

## Remaining acceptance work

- Improve visible directional flow and depth to the concept standard at close and gameplay scale. No current image is accepted.
- Correct excessive shell highlight clipping through separately approved plan33, then re-evaluate mist without changing terrain exposure to conceal the defect.
- Add real opaque cliff/ramp intersection and contrasting ground-value fixtures while retaining the difficult pale-deck baseline.
- Capture turn, boost, stop through full expiry, 30/60/144 Hz matching-time sequences, MSAA variants, and the 400-unit crowd. Inspect actual images and stability; do not infer visual success from state tests.
- Integrate final live-battle capture and required workspace tests/clippy/fmt under a coordinated source freeze.

## Render-path audit and actual crowd evidence

Locked Bevy extraction clones every modified Mesh, including its attribute/index vectors. Therefore this implementation's fixed main-world capacities do not establish allocation-free render extraction. Approved plan35 explicitly replaces that path with persistent GPU storage after preserving a measured release baseline; no performance completion claim is made now.

Root's actual `/tmp/space-scuffle-mist-v3-crowd-independent/Crowd-1-fps60-msaa4.png` submitted 400 owners with no pool overflow and clean corrected-renderer diagnostics. It clips bottom actors and does not establish that all 400 are inside the frustum. Requested virtual fps60 is not measured performance. Root rejects the crowd-scale mist readability. The actual runtime `/tmp/space-scuffle-r4-runtime-captures-run02/04-combat.png`, independently inspected here, shows narrow rigid blue ribbons and a separate solid rectangular shadow from combat light cards. Plan37 addresses that shadow correctness independently; it does not resolve mist art.
