# Q0009: Keep the terrain anisotropy test override, or replace the server's Mesa?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** keep the override; on this server GPU tests run on `radv`, which never applies it

## Question

On this server's Mesa 22.3.6, lavapipe cannot finish a frame of hex-tiled terrain with anisotropic filtering.
The first terrain frame was still unfinished after 280 s at 1600x900 and after 120 s at 64x36, so a smaller window,
fewer frames, or a longer timeout does not help. Mesa rewrote this sampler in 25.1, and CI on Mesa 25.2.8 finishes the
`map_editor` render in under 200 s. Details are in `docs/plans/2026-11-02-server-gate.md`.

The terrain shader is correct, so production stays at anisotropy 16. To let the gate run here, `client` now reads
`SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY`. The test harness sets it to 1 only on Mesa older than 25.1.

This has two costs:

- A production crate has a code path that exists for testing. It is unset for players, and it logs a warning when used.
- On this server, hex-tiled ground under anisotropic filtering is not rendered by any test. CI still covers it.

The options:

1. **Keep the override (default).** The gate runs here today, and CI keeps full coverage.
2. **Move the server to Mesa 25.1 or later, then delete the override.** This is the cleanest result, but it needs
   root. Update 2026-11-03: the server now runs Debian 13, whose Mesa is 25.0.7, still before the rewrite. So this
   now means `trixie-backports` or another Mesa source, once one carries 25.1 or later.
3. **Run the two terrain render tests on the physical GPU on this server.** `radv` is not affected, but the gate
   would then need the GPU lock, which AGENTS.md section 7 deliberately avoids.

I recommend option 2 when the server is next upgraded, and option 1 until then.

**Update 2026-11-03.** Following the owner's instruction to use the GPU, option 3 is now the default
(`docs/plans/2026-11-03-gpu-test-harness.md`, decision D7). On this server every GPU test child runs on the RX 580
through `radv`, under one GPU lock per child, and `radv` renders hex-tiled ground at the production anisotropy of 16
in all five terrain-drawing tests. The harness applies the override only to a child on `lavapipe` before Mesa 25.1,
so its only remaining consumer is a forced `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` run on this server, used to
reproduce CI. CI runs Mesa 25.2.8 and never applies it. The remaining choice is: keep the override until the
server's Mesa is 25.1 or later and then delete it (recommended), or delete it now, which makes a forced-`lavapipe`
run of the five terrain tests here time out.

## Answer
