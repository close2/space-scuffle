# Frame Budget with the Effect Slice (WP6c)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §10.3 and
the WP6c row of §9. Measured on 2026-11-10 by the implementing agent.

## Method

- **Tool.** `frame_budget` (`client_diagnostics`), release build, scenario `400` (eight armies of 50 units on the
  flat arena, the spec of [`crates/client_diagnostics/data/frame_budget.ron`](../../../../crates/client_diagnostics/data/frame_budget.ron)),
  1 500 measured ticks after 180 warm-up frames, headless KWin (`--display-path kwin-virtual`), 1280×720, vsync off,
  RX 580 (radv, Mesa 25.0.7), pinned to CPUs 32 to 47 as in the
  [recorded baseline](../../2026-11-03-frame-budget.md). Each run was one `with-gpu-lock` command
  (`GPU_LOCK_TIMEOUT=1200`), with 30 s between locked commands.
- **Arms.** B: the shipped client. S: `--effect-stress 200`, 200 splash events per second of game time,
  round-robin over every weapon whose presentation block names a splash effect (`shell_impact`, `acid_burst`,
  `spine_line`), each at a live unit of the presented snapshot chosen by a hash, with the zones and line the
  simulation publishes; the events go through the production effect wiring. H: `--hdr-bloom` (the D-11
  experiment, [`hdr_bloom_decision.md`](hdr_bloom_decision.md)). **Not covered:** the plan's 20 live areas and 50
  statuses; the effect framework draws neither yet (WP9, WP10), so there is nothing to stress.
- **Order.** Interleaved B, S, H triples, so slow drifts in machine load fall on every arm alike. Attribution runs
  (D*) add `--render-diagnostics` (per-pass GPU timings) and are not figures of record.
- **Effects data.** `assets/catalog/effects.ron` as committed with this file, except that the runs used the
  spine eruption curve before its last 15 ms tightening (no effect on cost).
- **Machine load.** The shared server was heavily loaded by other users' builds and test gates during the whole
  measurement: the one-minute load average ranged from 8.5 to 296 (16 CPUs available to this user). Runs B5 to
  H6 of the planned second series were stopped at a load average above 300.

## Figures (whole 1 500-tick window, frame interval in ms)

| Run | Load (start → end) | Frames | p50 | p95 | p99 | Main-world update p50 |
|---|---|---:|---:|---:|---:|---:|
| B1 | 31.6 → 22.8 | 4 148 | 9.98 | 36.26 | 53.79 | 7.91 |
| S1 | 19.7 → 25.3 | 3 080 | 18.25 | 31.63 | 38.66 | 14.32 |
| H1 | 23.8 → 26.2 | 3 141 | 17.70 | 32.50 | 42.23 | 13.62 |
| B2 | 23.7 → 23.5 | 3 088 | 18.01 | 32.02 | 41.47 | 14.16 |
| S2 | 25.8 → 23.6 | 3 191 | 17.95 | 30.10 | 37.39 | 14.21 |
| H2 | 18.4 → 17.2 | 4 315 | 11.16 | 28.88 | 37.21 | 8.55 |
| B3 | 13.9 → 10.2 | 5 390 | 10.22 | 17.69 | 20.82 | 8.44 |
| S3 | 8.6 → 10.0 | 5 290 | 10.33 | 18.37 | 22.68 | 8.41 |
| H3 | 8.5 → 15.0 | 4 151 | 13.58 | 22.13 | 31.70 | 10.46 |
| B4 | 62.9 → 97.1 | 3 130 | 17.29 | 33.92 | 47.67 | 14.06 |
| S4 | 99.2 → 229.5 | 4 017 | 11.20 | 36.64 | 48.28 | 9.29 |
| H4 | 163.1 → 296.2 | 5 046 | 11.08 | 18.15 | 22.35 | 8.57 |
| **B median (spread)** | | | **13.76** (9.98 to 18.01) | | **44.57** (20.82 to 53.79) | 11.25 |
| **S median (spread)** | | | **14.58** (10.33 to 18.25) | | **38.03** (22.68 to 48.28) | 11.75 |
| **H median (spread)** | | | **12.37** (11.08 to 17.70) | | **34.46** (22.35 to 42.23) | 9.51 |

The p50 of every arm is bimodal (about 10 ms or about 18 ms per run) with no relation to the arm, and the spread
within an arm (up to 80 %) is far larger than the budget's 5 % and 10 % lines. **The relative budget therefore
cannot be established to the plan's precision on this machine today.** What the data does show:

- **The quietest triple (B3, S3, H3; load 8.5 to 15):** the stress costs +1.1 % at p50 (10.22 → 10.33 ms) and
  +8.9 % at p99 (20.82 → 22.68 ms), inside the plan's +5 % and +10 %; the main-world update is unchanged
  (8.44 → 8.41 ms). Both arms stay well below the recorded 16.3 ms baseline in absolute terms.
- **Across all pairs** the stress arm's p50 median is 6 % above the baseline's (13.76 → 14.58 ms) and its p99
  median is lower (44.57 → 38.03 ms): noise in both directions.
- A re-measurement on a quiet machine (load below 8) with at least three pairs is recorded as open in the
  report of this WP.

## GPU time of the effect passes (RX 580, `--render-diagnostics`)

Effects draw in the `Transparent3d` phase (cards and decals), so their GPU time is the growth of
`main_transparent_pass_3d`; nothing else may change.

| Pass (mean ms, max ms) | DB (no effects) | DS (stress 200/s) | DH (HDR + bloom) |
|---|---:|---:|---:|
| `main_transparent_pass_3d` | 0.424 (1.524) | 0.585 (1.603) | 0.427 (1.822) |
| `main_opaque_pass_3d` | 2.183 (2.263) | 2.233 (2.283) | 2.160 (2.255) |
| `prepass` | 0.197 (0.758) | 0.198 (0.760) | 0.198 (0.763) |
| `bloom` | — | — | 0.281 (0.311) |
| Sum of instrumented passes | 3.541 | 3.732 | 3.812 |

**Effect passes under the stress: +0.16 ms mean, +0.08 ms at the maximum**, far under the plan's 1.5 ms. The
opaque pass moves by 0.05 ms (run-to-run variation). The bloom costs 0.28 ms on its own, before the untimed
tone-mapping pass.

## Crowd capture, coverage and overdraw

`--capture-tick 150 --capture <png>` (separate runs CB and CS, not figures of record):
[`stress_capture_400.png`](stress_capture_400.png) with the stream, [`stress_capture_400_baseline.png`](stress_capture_400_baseline.png)
without, legend [`stress_capture.txt`](stress_capture.txt). At tick 150 (6 s into the battle): 1 016 live effects,
498 cards, 1 427 decals; effect footprints cover **25.3 %** of the screen (union; transparent texels included, so
an upper bound), summed footprint area **0.92 screens**, at most 19 footprints on one 4-px cell. At tick 750 (DS):
1 151 live effects, footprints 14.4 %, overdraw 0.55. The units stay readable: every cluster of the baseline frame
is visible in the stressed one, with acid fronts as green rings, spine lines as thin streaks and dark cracks
between units.

## Budget sizing (data first)

The first lavapipe smoke of the stress mode with the WP6b budget refused 2 273 of 4 000 effects (the 480-slot
instance pool) and found 5 052 Critical instances without a slot (512 decals): 200 splashes a second with the
8 s scorch and the 6 s stain keep about 1 100 effects alive. The budget in `effects.ron` was resized to the
measured peak: effects 1 536, sprites 4 096, decals 2 048 (particles 30 000, lights 16 and the class caps
unchanged). Every stress run since: 12 000 events, **0 refused, 0 Critical overflow**; about 1 000 Detail
instances skipped for room and 4 000 for being smaller than 6 px, by design. The Major class cap (96) stays: in
such a crowd the Detail layers of effects spawned over it retire at birth, so the degradation never pops.
