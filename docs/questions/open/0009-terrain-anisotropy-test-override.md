# Q0009: Keep the terrain anisotropy test override, or replace the server's Mesa?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** keep the override, which applies only to lavapipe older than Mesa 25.1

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
   root. Debian's `bookworm-backports` offers 25.0.7, which is still before the rewrite, so it means Debian 13 or
   another Mesa source.
3. **Run the two terrain render tests on the physical GPU on this server.** `radv` is not affected, but the gate
   would then need the GPU lock, which AGENTS.md section 7 deliberately avoids.

I recommend option 2 when the server is next upgraded, and option 1 until then.

## Answer
