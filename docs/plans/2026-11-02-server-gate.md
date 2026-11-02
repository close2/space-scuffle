# Server gate: making `cargo test --workspace --locked` pass honestly

Date: 2026-11-02. Host: 16 pinned cores of an AMD EPYC 7301, Debian 12, Mesa 22.3.6, LLVM 15, Vulkan
loader and validation layers 1.3.239. Also covers the first CI run on `main` (41538515234): Ubuntu 24.04,
Mesa 25.2.8, layers 1.3.275.

## Failures and causes

### 1. `Unable to find extension: VK_EXT_swapchain_colorspace`

The line does not come from the validation layer. It is the `log::warn!` at `wgpu-hal-23.0.1/src/vulkan/instance.rs:299`,
in `desired_extensions`: `wgpu_hal` asks for the optional extension, finds that the instance does not offer it,
drops it, and continues. `vulkaninfo` confirms that Mesa 22.3.6 lists 14 instance extensions and this is not one
of them. It is a host capability notice that only happens to share the messenger's log target.

**Fix.** One allowlist entry for that exact text, with the reason. The match is exact, so any other missing
extension, such as `VK_EXT_debug_utils`, and every real validation error still fail. Tests prove both.

### 2. Best-practices control

Layer 1.3.239 names IDs `UNASSIGNED-BestPractices-...`; newer layers use `BestPractices-...`.

**Fix.** A named predicate accepts exactly those two forms, so the control still requires a best-practices finding
that came from the real layer, and an unrelated `UNASSIGNED-...` finding does not satisfy it.

### 3. Terrain renders never finish (not a throughput problem)

Measured on `terrain_preview`, debug build unless noted, at a load average of 30 to 60:

| Configuration | Result |
|---|---|
| 1600x900, anisotropy 16 (production) | frame 3 at 1 s; frame 4 not reached in 280 s |
| 320x180, anisotropy 16 | frame 4 not reached in 200 s |
| 64x36, anisotropy 16 | frame 4 not reached in 120 s |
| 1600x900, anisotropy 2 | frame 4 not reached in 100 s |
| Release build, 1600x900, anisotropy 16 | frame 4 not reached in 100 s |
| 1600x900, anisotropy 16, hex tiling bypassed in the shader | finishes, 93 s, 44 frames, about 2 s per frame |
| 1600x900, anisotropy 1 | finishes, 52 s and 78 s, 43 frames, 1.1 to 1.7 s per frame |
| Release build, 1600x900, anisotropy 1 | finishes, 70 s, 44 frames, about 1.4 s per frame |
| 800x450, anisotropy 1 | finishes, 33 s, 43 frames, about 0.5 s per frame |

Conclusions:

- 43 frames are needed, and the window size does not change that. The first frame that draws terrain is the
  one that stalls.
- `perf` puts the stalled process in `llvmpipe-N` threads, in `[JIT]` code. Rust, debug or release, is not a factor:
  the release build behaves the same.
- Cutting pixels by 494 times does not make that frame finish, and neither does a smaller clamp. This is why
  a smaller window, fewer frames, an opt-level, or a longer timeout cannot fix it.

**Cause.** A defect in llvmpipe's anisotropic sampler, triggered by a valid shader.
`lp_build_sample_aniso` in Mesa 22.3.6 (`lp_bld_sample_soa.c`) is an elliptical filter that visits every texel in
the footprint's bounding box. It takes that footprint from coordinate differences across the pixel quad
(`lp_build_packed_ddx_ddy_twocoord`), ignores the explicit gradients of `textureSampleGrad`, and never applies
`maxAnisotropy` to the box. `hex_sample` rotates and offsets its coordinates per hex vertex and passes correct
gradients. Where a quad crosses a hex edge, the coordinates jump, which is legal, so the coordinate difference
is up to about `2 * 16 tiles * 1024 texels`: on the order of 30,000 texels per axis, or about 10^9 texel reads for
one sample, of which `hex_sample` takes nine per layer. The work is finite but is effectively unbounded, and
thousands of quads cross hex edges. Bypassing `hex_sample` removes the stall with anisotropy 16 still on.

The rewrite to the N-probe algorithm of the Vulkan specification is absent from the 25.0.0 source and present in
25.1.0. CI runs Mesa 25.2.8 and finished this same render in about 110 s. Debian's `bookworm-backports` offers
25.0.7, so an apt upgrade is unlikely to help here, and the server user has no root access.

**Fix.** The shader and the production sampler are correct and stay unchanged. The render has to run at
`anisotropy_clamp = 1` on affected drivers only, because `maxAnisotropy` above 1 selects the defective path:

- `client::terrain::theme_gpu` reads `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY`. Unset means the production value 16.
  An invalid value is a typed error, and an override logs a warning.
- `gpu_run.rs` reads `driverInfo` from `vulkaninfo --summary` and sets the variable to 1 only for Mesa older than
  25.1, saying so on stderr. Mesa 25.1 and later, including CI and current notebooks, keep running at 16.
- If the version is unknown, the harness keeps the production sampler, so the failure direction is a timeout
  and not a quiet reduction in coverage.

**What the tests still prove on an affected host.** The real shader, pipelines, meshes, textures and screenshots,
plus all image checks and Vulkan validation. What is lost is hex-tiled ground under anisotropic filtering. The
anisotropic sampler itself stays covered here: `client --battle` spawns the same terrain material at 16 and
completes under validation.

The new variable is a production code path that exists only for testing, and it goes beyond the fixes listed for
this task. It is recorded as question 0009 for the owner.

`GPU_RUN_TIMEOUT_SECS` stays at 300, and its stale comment is replaced with these measurements.

### 4. CI: `VK_LAYER_KHRONOS_validation was not inserted`

The report was that only `map_editor` fails on CI. It is the first validated run on CI, not the only one.
`cargo test` stops at the first failing test binary, and `editor_app_tests` comes before `gpu_validation_tests`,
`lab_scene_tests`, `round2_visual_tests` and `terrain_tests`, none of which ran. No validated run has passed on CI.

**Cause.** Ubuntu 24.04's `xvfb-run` (2:21.1.12-1ubuntu1.9, line 184) runs the command as `"$@" 2>&1`, whereas
Debian 12's and Arch's do not. The client's stderr therefore arrives on stdout, the gate parses an empty stderr,
and the run looks unvalidated. The layer almost certainly was inserted. The run exited 0 and no other violation
was reported, which is also consistent with an empty stream.

**Fix.** The gate reads both streams through one `captured_log` helper. That applies to the failure tails and to
`smog_steps_reach_the_gpu_through_queue_writes`, which reads upload logs from stderr and would see none on CI.
Verified here by pointing the harness at the Ubuntu
script, which reproduces the failure before the fix and passes after it.

**Open risk.** Validation has never been seen working on CI, so layer 1.3.275 may report findings that neither
layer version seen so far reported. Only a CI run can show that.

## Commits

One commit per fix: this plan, colorspace allowlist, best-practices control, stream capture, terrain anisotropy.
