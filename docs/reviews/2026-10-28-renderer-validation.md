# Renderer validation observations

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

## Reproduction evidence

The existing `terrain_preview` was run without changing its shader or meshes. Native Vulkan produced all four screenshots in `/tmp/space-scuffle-round1-terrain-native`, but also emitted GPU validation errors. Therefore its successful exit does not prove a validation-clean renderer. The complete log is `/tmp/space-scuffle-round1-terrain-native.log`.

The pinned stack is Bevy 0.15.3, wgpu/wgpu-core/wgpu-hal 23.0.1, and Naga 23.1.0. The hardware adapter reported AMD Radeon 890M Graphics, RADV STRIX1, Mesa 26.2.4. The software Vulkan adapter reproduces the shader diagnostics. An alternative native OpenGL probe failed surface configuration with `incompatible window kind`, recorded in `/tmp/space-scuffle-round1-terrain-gl.log`; it is not a passing alternate verification route.

## Distinct failures

1. The existing preview's default Bevy asset path can fail to load the terrain shader. Supplying `BEVY_ASSET_ROOT=/home/cl/projects/space-scuffle` allowed the actual file to load. Normal game startup now has a validated asset-root resource; new runtime captures must use that root explicitly.
2. `VUID-StandaloneSpirv-None-10684` reports `ArrayStride` on the workgroup variable `previous_mip_depth`. The installed source identifies this variable in Bevy's SSAO `preprocess_depth.wgsl`. The same class is tracked in [wgpu issue 7696](https://github.com/gfx-rs/wgpu/issues/7696). Current upstream [wgpu Vulkan callback source](https://wgpu.rs/doc/src/wgpu_hal/vulkan/instance.rs.html) suppresses this diagnostic; suppression is not evidence of corrected code generation.
3. `VUID-StandaloneSpirv-MemorySemantics-10871` reports invalid atomic memory semantics. [Bevy issue 21512](https://github.com/bevyengine/bevy/issues/21512) documents the corresponding error and points to [Naga fix 8391](https://github.com/gfx-rs/wgpu/pull/8391), later incorporated with wgpu 28. Matching this diagnostic is evidence for an upstream compiler problem, not proof that every local render error shares that cause.
4. Native presentation reports semaphore acquisition and write-after-present synchronization errors. These require independent reproduction; do not assume the shader-compiler fixes address swapchain synchronization.

## Verification requirements

Keep validation enabled and preserve logs. Distinguish shader readiness from shader failure: Bevy may temporarily represent asynchronous shader loading as `ShaderNotLoaded` or `ShaderImportNotYetAvailable` and retry it. The official [compute example](https://github.com/bevyengine/bevy/blob/main/examples/shader/compute_shader_game_of_life.rs) illustrates that distinction; implementation must use the installed 0.15 APIs. A bounded timeout must still fail permanently missing assets.

The new mist and runtime battle harnesses must compile and draw their actual production shaders, wait for screenshot callbacks, and inspect output. Existing upstream diagnostics must remain explicitly recorded until addressed by a separately planned compatibility change. Do not turn off assertions, validation, or real assets to manufacture a passing result. An engine migration or pinned compiler backport needs its own architect blueprint, scope review, and full regression/render verification.
