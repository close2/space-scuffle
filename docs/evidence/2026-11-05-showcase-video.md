# Evidence: the CI showcase video (WP1 to WP3)

Plan: [`2026-11-05-ci-showcase-video.md`](../plans/2026-11-05-ci-showcase-video.md). This file records the
calibration, timings, sizes and review notes of WP2 and WP3, measured on the shared development server on
2026-11-05; WP4 to WP6 add the CI figures.

Large outputs (mezzanines, MP4, WebP, stills) live under `~/scratch/showcase/` on the development server and
are not committed; they are lost evidence once that directory is cleaned.

## Conditions

- Server: 16 allowed cores (`Cpus_allowed_list` 32-47), load average 45 to 115 during the measurements
  (other agents' builds and gates). Every figure below is wall time under that load.
- Release build of `showcase` (`cargo build --release --locked -p client_diagnostics --bin showcase`): 10 m 49 s
  from cached dependencies of another target directory, 21 m 48 s for an incremental rebuild at load 115.
- `ffmpeg` 7.1.5 (Debian package).

## WP2: the GPU smoke test

`showcase_smoke_take_is_byte_reproducible` (`crates/client_diagnostics/tests/showcase_tests.rs`), debug build,
through the GPU harness on `radv`: first child 25.3 s, second 19.9 s, 16 frames each, ticks, checksums and
BLAKE3 hashes identical. The first run was red: the plan's 480×270 smoke frame failed its own rule "resolution
within 320..=4096" (height 270); the rule is now 320..=4096 wide and 180..=4096 high.

## Test memory (gate rule of 2026-11-05)

Peak RSS of the new fast test executables at libtest's default thread count, measured after the server freeze
of 2026-11-05: `showcase_storyboard_tests` 2,079 MiB (7 tests, one headless Bevy app, 22.7 s),
`showcase_encode_tests` at most the same (3 tests, ffmpeg children, 4.9 s). Far below the battle_* binaries,
so neither gets a `test_threads` cap. `showcase_tests` runs its takes as GPU-harness children, one at a time.

## WP2: calibration (plan D-13)

| Driver | Frames | Resolution | Cores | Readiness | p50 per frame | p95 | Mean | Take |
|---|---|---|---|---|---|---|---|---|
| `radv`, RX 580, under `with-gpu-lock` | 1,100 (full take) | 1280×720 | all 16 allowed | 7.7 s | 171 ms | 372 ms | 163 ms | 179 s |
| `lavapipe` (Mesa 25.0.7, LLVM 19.1.7), anisotropy override 1 | 100 (calibration storyboard: every scene shortened) | 1280×720 | 4 (`taskset -c 32-35`) | 26.8 s | 863 ms | 1,580 ms | 958 ms | 99 s |

On `radv` the frame time is the CPU side (simulation, smog solver, readback and the FFV1 encoder at load 50),
not the GPU. The calibration storyboard is the shipped one with each scene cut to 5 to 20 frames
(`~/scratch/showcase/calib/calibration.ron`).

**Calibration decision.** With *c* = 0.96 s (the mean, above the median), the shipped take of 1,100 frames
takes 1,100 × 0.96 + 120 ≈ 1,176 s, about 20 minutes, inside the 40-minute render budget (rule: 1,600 × *c* +
120 s ≤ 40 min, *c* ≤ 1.4 s). The storyboard keeps 1280×720. WP4 repeats the measurement on the runner, whose 4
vCPUs are not shared with other agents.

## WP3: the full local renders

All on `radv` with `showcase render --driver radv` (one `with-gpu-lock` per take):

| Render | Storyboard | Frames | Take | Whole render | MP4 | WebP | Notes |
|---|---|---|---|---|---|---|---|
| r1 | first draft | 1,200 | 184 s | about 17 min | 9,651,303 B | 4,322,556 B (800 px, q60; q70 was 5,234,900 B) | review findings 1 to 3 |
| r2 | closing paused, shorter advance, combat clusters | 1,141 | 173 s | about 15 min | — | — | finding 4 |
| r3 | smog before the advance, units framed higher | 1,100 | 179 s | 598 s | 7,649,152 B (44.0 s, 1,100 frames, H.264 High `yuv420p` 25 fps) | 3,607,664 B (800 px, q60, 295 frames) | finding 5 |
| r4 | army centroid over armed units only | 1,100 | 157 s | about 10 min | — | — | finding 6 |
| r5 | `units` and `smog` follow one unit | 1,100 | 168 s | 10 min 20 s | 7,978,030 B (44.0 s) | 3,658,324 B (800 px, q60, 295 frames) | final: `~/scratch/showcase/r5/site/` |

Mezzanine: 783 MB for 1,200 frames (0.65 MB a frame, FFV1). Of the whole render, the teaser ladder is the
largest step after the take: one rung (select over the full mezzanine, 800 px, `compression_level 6`) took
about 6 minutes at load 50, and r1 needed two rungs. The ladder now starts at quality 60. WP4 should measure a
rung on the runner; if it is slow there, `compression_level` 4 is the first thing to try.

### Review findings and what changed (the storyboard only, unless noted)

1. **Combat followed empty ground (r1).** The two armies do not meet: each attack-moves past the other and
   raids the other's base. `FollowCombat` averaged both fights and looked at the empty plateau between them.
   Fixed in code: `FollowCombat` follows the fight nearest the previous focus (damage positions within 10
   world units of the one nearest the camera).
2. **The result window (r1).** The viewer's base falls around tick 1,225 and the Player profile shows its real
   "Defeat" window, which appeared under the status card. The scenes before `closing` now end near tick 1,100
   and `closing` is paused (the smog keeps moving and the orbit keeps turning), so the video never reaches the
   result.
3. **Unit close-up cut by the HUD (r1).** The battle viewport ends above the HUD's bottom panel; at distance 4.5
   and pitch 35° the followed unit sat at its lower edge. Now distance 8, pitch 55°.
4. **The smog barely showed in a static fight (r2).** The smog is most visible in the wakes of moving bubbles
   (the purple and green trails in the unit close-up). The `smog` scene now follows the marching army low and
   close (distance 9, pitch 30°) before the armies arrive.
5. **The army centroid included builders (r3).** Unarmed units stay at home, so the centroid of all mobile
   units lay between the base and the army. Fixed in code: `FollowArmy` and the fallback of `FollowCombat` use
   living, mobile, armed units.
6. **The army splits around the central plateau (r4).** Its centroid then lies on the plateau, and a low camera
   showed the cliff, not the units. `smog` now follows one unit (`pulse_bot`, then `rhino_beetle`) at distance
   10 and pitch 35°; the purple and green wakes of the marching column are the smog the caption names.

Accepted as they are:

- The bottom quarter of every battle frame is the real HUD's command-card panel, empty because nothing is
  selected; the director selects nothing (it never acts as the player).
- Fog of war shows only as enemies hidden outside sight (§2.3 of the plan); the caption says only that.
- The teal patches on the ground are terrain decoration, not smog.

### Stills of the final render (r5)

`~/scratch/showcase/r5/stills/s01.png` to `s08.png` (frames 60, 200, 350, 480, 580, 700, 850, 1000) and the
contact sheet of the `smog` scene `~/scratch/showcase/r5/stills/smogsheet.png`; the earlier contact sheets are
`~/scratch/showcase/r1/stills/sheet1.png` and `~/scratch/showcase/r2/stills/sheet.png`. Checked: no blank or black frame, captions legible at
1280×720, title card with the `DESIGN.md` pitch and the footer "Real engine output · rendered on AMD Radeon RX
580 Series (RADV POLARIS10) · <sha7> · 2026-11-05", status card with "23 subsystems working, 3 partial, 2 not
built" and the four newest dated active plans, units animated (flipbook clips change between frames), the
fast-forward badge on every scene above 1×.

Final render r5: take ready after 5.0 s, 1,100 frames in 168 s (p50 156 ms, p95 344 ms per frame on `radv`),
mezzanine 705 MB, scenes title 125, terrain 150, units 150, smog 125, advance 50 (`UntilCombat`, combat
already under way), clash 300, closing 200 frames. MP4 7,978,030 B and WebP 3,658,324 B, both within their
budgets (25 MiB and 4.5 MiB).
