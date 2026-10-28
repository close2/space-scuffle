# Mist output transformation — Stage 1 verification

Stage 1's bounded output contract is accepted by the orchestrator, subject to the next integrated workspace gate. The actual AMD helper and nine native capture cases passed. The existing mist art and whole-frame performance target remain rejected. Plan 35's resource and equivalence evidence is recorded separately in the round 8 storage review.

The shipped fragment already has straight linear radiance and separate opacity. Its new output helper applies Bevy's view grading and tone mapping in the non-HDR path, followed by the engine's optional debanding, before multiplying RGB by opacity once. Zero alpha produces exact zero. The HDR path retains linear premultiplied radiance. Density, geometry, lighting selection, media and style are unchanged.

Locked Bevy 0.15.3 `mesh.rs` selects `PREMULTIPLIED_ALPHA_BLENDING` and disables depth writes for this material. Its color attachment is `ViewTarget::TEXTURE_FORMAT_HDR` for HDR or the default sRGB attachment otherwise. The mesh key generates `TONEMAP_IN_SHADER`, the selected tone-map method, and optional `DEBAND_DITHER`. Locked `pbr_functions.wgsl` performs grading/tone mapping and perceptual dither before alpha handling. The GPU's sRGB attachment conversion remains responsible for the final transfer; this change adds no manual second encoding.

A capture-only observer records the actual successful mist pipeline descriptor in `mist-output-pipeline.ron`: all fragment definitions, target format, blend factors, sample count and depth-write flag. It rejects a mismatched output contract and refuses required mist screenshots without a successful observed pipeline. The metadata unit test exercises missing-pipeline, wrong HDR path and wrong blend rejection. This test verifies admission logic, not rendering.

The actual GPU arithmetic test extracts `mist_output` from the shipped WGSL and compiles it with the real engine `tone_mapping`, `screen_space_dither`, `powsafe` and `ColorGrading` imports. It checks linear HDR, Reinhard with one stop of exposure, and the same LDR path with debanding. Inputs cover HDR-range radiance, black, five alpha values including zero, and independent analytic expected RGB. It verifies finite output, unchanged alpha, and tone mapping before premultiplication. Native captures must additionally verify the actual production default tone-map method and material/postprocess pipeline.

The first test attempt failed because the default pipelined renderer moved `RenderApp` into its worker after the first update; the harness then panicked and teardown raised SIGSEGV. The complete failure is retained in `/tmp/space-scuffle-r9-mist-output-test.log`. The test now explicitly disables only `PipelinedRenderingPlugin`, retaining the real renderer and real shader imports in the main-thread render app. The corrected test passed on software Vulkan (`llvmpipe`), with its backend discovery diagnostics preserved in `/tmp/space-scuffle-r9-mist-output-test02.log`. This is not AMD evidence. The final fixture also selects the explicit headless `DontExit` window policy.

Focused metadata admission tests pass (`/tmp/space-scuffle-r9-mist-output-metadata02.log`). Strict client all-target Clippy passes (`/tmp/space-scuffle-r9-mist-output-clippy03.log`), and exact workspace formatting passes (`/tmp/space-scuffle-r9-mist-output-fmt.log`). The next integrated workspace gate remains pending.

Preserved artifacts for independent native verification:

- `tmp/verification/mist-output-r9-artifacts/mist_preview-output`: SHA256 `23e7640d96f9612b144a8dd62f273cdeb041ff0d5baec8d5a59432a27de88751`.
- `tmp/verification/mist-output-r9-artifacts/client-output-tests`: SHA256 `0ad5167a34f244989e36451e095d639739ec439d0bdadc741dc87a183e31dcf6`.
- Live shipped mist shader: SHA256 `be4928718e293b72a1c083dd0c0f5f6cd18f6040c683a79a19a0ecd7c669b619`.

Native comparison uses the same Close, Boost and Crowd fixtures, 60 Hz virtual time, physical 1280×720 and MSAA4, in both HDR and non-HDR modes. Close/Boost retain the difficult bright deck; Crowd includes dark cliff-adjacent terrain. The earlier preserved shader supplies an unchanged-HDR control. Successful compilation alone does not establish acceptable appearance, and HDR/LDR screenshots are not expected to be pixel-identical: HDR tone maps the composited scene, whereas non-HDR tone maps surfaces before blending.

The preserved final helper executable subsequently passed on actual AMD Radeon 890M/RADV Vulkan, exit 0 with a clean native log: `/tmp/space-scuffle-r9-mist-output-amd01.log`. Its SHA256 was checked before execution. The native image matrix is running separately; this helper pass does not substitute for those specialized material captures.

## Actual native matrix and visual review

All nine native AMD cases completed with exit 0 and clean renderer logs, producing 27 decoded 1280×720 PNGs. Artifacts, asset identity checks and independent comparison are under `/tmp/space-scuffle-r9-mist-output`; `independent-output-audit.json` records differences. The corrected Close, Boost and Crowd cases each ran with HDR and LDR. The preserved gated implementation supplied corresponding HDR controls. Every referenced live asset matched the gated asset identity except the explicitly identified mist shader.

Actual LDR descriptors contain Tony McMapface and debanding definitions, `Rgba8UnormSrgb`, source-One/destination-OneMinusSrcAlpha color and alpha blend, four samples and no depth writes. HDR descriptors use `Rgba16Float`, the same blend/sampling/depth policy, and no `TONEMAP_IN_SHADER`. Thus the production method and output specialization are observed rather than inferred from CLI settings.

The unchanged HDR branch matches its preserved control within one channel value in Close and Boost (7–20 changed pixels per image); all three Crowd HDR images are pixel-exact. LDR changes remain localized to the wake, with maximum channel differences of 30–45 against the pre-correction images. These changes are expected from the missing display transform correction, not geometry changes.

Both root and the implementation agent inspected the six corrected middle images. The creature remains readable, but Close and Boost still show only a faint, flat strip on the bright deck, with no convincing volumetric body or downstream breakup. Crowd wakes remain small pale traces that do not provide strong directional readability. The color contract passes; art remains rejected. No brightness increase or successful shader compilation is treated as a visual-quality pass.

After native acceptance, a reviewed test-only cleanup replaced unused numeric LUT binding definitions with Bevy's public binding constants. The analytic Reinhard variant does not sample a LUT, so the preserved AMD result remains valid; no captured shader, geometry or runtime behavior changed. The next integrated gate reruns the current helper test. Public evidence APIs also received explanatory rustdoc.
