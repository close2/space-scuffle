# Reviewed renderer compatibility source

`naga/` is the published Naga 23.1.0 crate, with its MIT and Apache-2.0 licenses preserved. The release crate SHA-256 is `364f94bc34f61332abebe8cad6f6cd82a5b65cff22c828d05d0968911462ca4f`; its upstream source commit is `9993f5c9eceb91f9bb6b9c5c6994f35f50e5f378` (wgpu `naga-v23.1.0`). No global Cargo cache files are modified.

The only code changes are in `src/back/spv/{helpers,mod,writer,block}.rs`, recorded as `patches/naga23-vulkan-layout-and-atomics.patch` against that release:

- Relaxed atomic memory semantics adapt the accepted correction in [wgpu PR 8391](https://github.com/gfx-rs/wgpu/pull/8391), merge commit `7a2afeb014cb30a976059717c4ae7c85f01d858e`. Scope and separate barrier semantics remain unchanged.
- Local logical/interface composite type lowering retains host buffer offsets and strides while preventing forbidden explicit decorations in workgroup/private/function memory. Whole sized composite transfers explicitly convert representations; unsized buffer blocks remain interface-only. This part is a project patch, not an upstream backport.

The isolated regression runner is `tools/renderer_compatibility`. It compiles real WGSL with this compiler, invokes `spirv-val --relax-block-layout --target-env vulkan1.3`, retains disassembly, and checks all 48 output scalars after a real Vulkan storage/workgroup/private composite transfer. Run from that directory:

```sh
cargo run --bin renderer-compiler-probe -- /tmp/renderer-compatibility-spv10
PROBE_SPIRV_15=1 cargo run --bin renderer-compiler-probe -- /tmp/renderer-compatibility-spv15
cargo test --manifest-path ../../third_party/naga/Cargo.toml --lib --features wgsl-in,spv-out
```

`spirv-val`, `spirv-dis`, a Vulkan adapter, and the native validation layer are required. Missing tools/backend are errors. `surface_probe` independently demonstrates native acquire/present behavior: without `--clear` it deliberately reproduces the original validation failure; with `--clear` it submits a real surface initialization before presentation. It is diagnostic evidence, not a passing production test.

`uniform_mat2_edge.wgsl` records a distinct preexisting uniform extended-layout failure (mat2 column stride 8 rather than 16). It fails on both the published and patched compiler; it is retained as a known boundary, not weakened or included among passing fixtures. The comparative upstream corpus had 57/87 baseline passes and 64/87 patched passes, with zero baseline-pass regressions. Full provenance and evidence are in `docs/reviews/2026-10-28-renderer-compatibility-isolated.md`.

Keep this fork until an upstream release provides equivalent fixes and passes this fixture matrix plus actual game/native screenshot validation. A version bump or filtered validation log is insufficient. Upstream's inherited coding style, comments and lints are not project-authored shortcuts; do not sweep unrelated upstream code into this patch.
