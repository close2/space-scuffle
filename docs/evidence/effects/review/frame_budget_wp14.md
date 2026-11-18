# Frame Budget with Every Effect (WP14)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §10.3 and
WP14, Amendment 31. Measured on 2026-11-18 by the implementing agent on branch `effects-viz-wp14`. It follows
[`frame_budget_wp6c.md`](frame_budget_wp6c.md), which measured the slice's splashes only.

## Method

- **Tool.** `frame_budget` (`client_diagnostics`), release build, scenario `400` (eight armies of 50 units on the
  flat arena), 1 500 measured ticks after the spec's warm-up, 1280×720, vsync off.
- **radv:** the RX 580 under a headless KWin (`--display-path kwin-virtual`), pinned to CPUs 32 to 47, one
  `with-gpu-lock` command per run (`GPU_LOCK_TIMEOUT=1200`), the recipe of
  [`2026-11-03-c17-combat-fx-pool.md`](../../2026-11-03-c17-combat-fx-pool.md).
- **Arms.**
  - **B:** the shipped client.
  - **S:** `--effect-stress 200`, the WP6c splash stream (200 splash events a second over every weapon whose block
    names a splash; now seven looks).
  - **P (the plan's §10.3 stress):** `--effect-stress 200 --effect-mix --mix-shots 0 --mix-deaths 0`: the splashes,
    20 live areas, 4 tethers, 2 spell missiles and 50 held unit looks (statuses, stances, morphs, concealment).
  - **M (every effect):** `--effect-stress 200 --effect-mix`: P plus 200 shots and blows a second (over every
    weapon that names a travel or impact) and 20 deaths a second (one unit type per death look). M draws every
    effect of the vocabulary: 59 of 59 were spawned or held in each M run (the report's "Effects spawned or held
    at least once" line).
  - **D\*:** the same arms with `--render-diagnostics` (per-pass GPU timings); not figures of record.
- **Machine load.** The shared server was heavily loaded during the first series (one-minute load 36 to 106 on the
  16 CPUs this user has); those runs are listed but are not of record. The second series ran at load 4 to 14.

## Effect pool sizing (data first, §10.3)

The first M run (DM-01) refused 24 608 of 55 000 effect spawns for a full instance pool (`budget.effects` 2 048).
A refusal turns a whole new effect away (the runtime does not count refusals by class); no spawned effect's
Critical layer went without a slot, so the pool degraded as designed under a load no battle produces. 6 144 is
chosen so that the every-effect measure draws everything, not because a battle needs it (review round 1, W14-7): next to the splash stream's 1 100 or so live effects, the 20 storms
and clouds keep emitting flares (`storm_flare`, 3 450 in a run) and billows (`murk_billow`, 2 858). At 4 096 it
refused 147 (DM-02); at 6 144 none (DM-03 and every later run). `effects.ron` ships 6 144. The held-beam table
keeps its 800 (a quarter of the pool, now 1 536, no longer limits it); no beam was refused at 6 144.

## Figures of record (load 4 to 14, frame interval in ms, whole window)

| Run | Load (start → end) | Frames | p50 | p95 | p99 | Main-world update p50 |
|---|---|---:|---:|---:|---:|---:|
| B-04 | 6.1 → 6.5 | 4 686 | 11.82 | 19.45 | 21.22 | 9.52 |
| P-04 | 6.7 → 5.6 | 3 592 | 15.66 | 23.14 | 25.01 | 13.38 |
| M-04 | 5.3 → 6.3 | 3 283 | 17.26 | 24.54 | 27.45 | 14.94 |
| B-05 | 6.2 → 4.8 | 4 622 | 11.89 | 19.73 | 21.89 | 9.66 |
| P-05 | 4.5 → 6.1 | 3 954 | 14.27 | 22.93 | 25.39 | 12.29 |
| M-05 | 14.3 → 8.2 | 3 324 | 17.04 | 24.18 | 26.41 | 14.82 |
| **B median** | | | **11.86** | | **21.56** | 9.59 |
| **P median** | | | **14.97** | | **25.20** | 12.84 |
| **M median** | | | **17.15** | | **26.93** | 14.88 |

Against the plan's bars, which are set on the **recorded baseline** of
[`2026-11-03-frame-budget.md`](../../2026-11-03-frame-budget.md) (400 units: p50 16.3, p99 23.6 ms; p50 at most
17.1, p99 at most 26.0):

| Arm | p50 (bar 17.1) | p99 (bar 26.0) | Verdict |
|---|---:|---:|---|
| P, the plan's stress | 14.97 | 25.20 | **within** both |
| M, every effect (beyond the plan's stress) | 17.15 | 26.93 | over by 0.3 % (p50) and 3.6 % (p99) |

Against **today's** baseline on the same machine (B, 11.86 ms), which is 27 % faster than the recorded one, the
relative increases are larger: P +26 % at p50 and +17 % at p99, M +45 % and +25 %. The difference is the main
world's update (B 9.6, P 12.8, M 14.9 ms at p50), not the GPU (below).

**Attribution (corrected after review round 1, W14-1).** This file first put that cost on the 20 live areas, the 50
held looks and their emitted children. The runs below that are not of record say otherwise: the splash stream
alone (S, no areas, no held looks) already adds as much update time as the plan's whole stress.

| Pair | Load | B p50 / update p50 | S p50 / update p50 | S over B, update |
|---|---|---|---|---|
| DB-01 / DS-01 (both with timestamp queries) | 7 to 10 | 12.30 / 9.64 | 15.55 / 13.25 | +3.6 ms |
| B-03 / S-03 | 9 to 42 | 11.90 / 9.76 | 16.80 / 13.87 | +4.1 ms |
| B median / P median (of record) | 4 to 14 | 11.86 / 9.59 | P: 14.97 / 12.84 | +3.3 ms |

WP6c measured the same 200 splashes a second at +1.1 % p50 and no update change, with the slice's three looks. Since
then the vocabulary gained the heavier splash looks (flame bands, lance grooves, cleave fronts) with Critical ground
marks, so the likely cost is in the splash path (decal reservation, erosion above 85 % of the decal pool, per-frame
decal instance building), not `area_sight`. Registered as B-137 in `DESIGN.md` §4 with its order: a quiet B/S/P
series to confirm, a profile of S, `area_sight` only if P exceeds S, then a re-recorded baseline with the plan's
stress held to +5 % (p50) of it.

## GPU time of the effect passes (RX 580, `--render-diagnostics`)

Effects draw in `Transparent3d`, so their GPU time is the growth of `main_transparent_pass_3d`.

| Pass (mean ms, max ms) | DB (none) | DS (splashes) | DP (plan's stress) | DM-03 (every effect) |
|---|---:|---:|---:|---:|
| `main_transparent_pass_3d` | 0.666 (1.011) | 1.133 (2.476) | 1.234 (1.590) | 1.204 (2.242) |
| `main_opaque_pass_3d` | 2.184 (2.190) | 2.244 (2.336) | 2.242 (2.280) | 2.239 (2.334) |
| **Effect passes over DB** | — | **+0.47** | **+0.57** | **+0.54** |

Every arm is far under the plan's 1.5 ms. The opaque pass moves by 0.06 ms (run-to-run variation).

## Crowd capture (checklist item 6)

`--capture-tick 150` at the spec's far view, as in WP6c (retaken in review round 2, W14-6; the first take used a
closer review camera that cut the fight at the frame's top): [`wp14_crowd_mix_400.jpg`](wp14_crowd_mix_400.jpg)
with every effect, [`wp14_crowd_baseline_400.jpg`](wp14_crowd_baseline_400.jpg) without. Not figures of record.
At tick 150 with every effect: 2 283 live effects, 2 877 cards, 3 203 decals; effect footprints cover 15.1 % of the
screen (union, transparent texels included, so an upper bound), summed footprint area 1.9 screens, at most 69
footprints on one 4-pixel cell. Without: 60 live effects, footprints 0.17 %. WP6c's splash stream covered 25.3 %
at the same tick and view; the mix covers less because the battle's armies stand in one row here, and the far view
culls small layers by their size on screen. Every cluster of the baseline frame is visible in the stressed one.
The left of the frame is off the map in both. The mix's shots fly between two hashed units, so its beams and
tracers cross the whole fight, which a battle's shots (at weapon range) never do: the frame is busier than any
battle would be.

## lavapipe

### Showcase render (`showcase render --driver lavapipe`, 4 CPUs, as on CI's 4 vCPUs)

`taskset -c 32-35`, `LP_NUM_THREADS=4`, release build, Xvfb, under load (other users' builds and this round's own
gate on the other CPUs), so the times are upper bounds.

| Take | Frames | Render s | s/frame | Scenes' effects |
|---|---:|---:|---:|---|
| `battle` (unchanged; `closing` moved out) | 1 222 | 1 811.7 | 1.48 | its scenes start on ticks 251, 305, 683, 953 as before |
| `effects` | 591 | 676.3 | 1.14 | every asked-for effect shown (`entrench`: 4 of 4; `siege`: 5 of 5, plus `gauss_tracer`, `bullet_hit`) |

The effects take costs 0.77 of the battle take per frame. Site: video 19.5 MB (budget 26.2 MB), teaser 4.26 MB at
640 px, quality 50 (budget 4.72 MB; the ladder's fifth of six rungs).

**CI estimate (D-13 of the showcase plan).** The last CI render (run 44241374353, `5a29900`) took 1 058.5 s for the
1 422-frame `battle` take on the runner's lavapipe (0.744 s a frame). With the effects take at 0.77 of that per
frame, it adds about 591 × 0.744 × 0.77 ≈ 340 s, so the takes need about 1 400 s (23 min) against 1 058 s
before. That run's job took 36 min in all, about 28 of them in the render step (setup and the cached build took the
rest), so the step should reach about 35 min with the larger video and one more teaser rung: inside the 40-minute
render budget, with less margin than before. The first CI render after the merge measures it (WP15); if it
exceeds 35 min, the storyboard's length rule applies (shorten `siege`, then `closing`).

### Frame budget on lavapipe (400 units, Xvfb software presentation, 12 CPUs, short window)

`--warmup-frames 30 --measure-frames 300`, under load 29 to 104, so indicative only:

| Arm | Frame interval p50 (ms) | p99 | Effects spawned, refused | Effects drawn |
|---|---:|---:|---|---|
| B | 1 526.9 | 5 091.0 | — | — |
| P (the plan's stress) | 1 578.4 | 3 052.3 | — | — |
| M (every effect) | 1 497.1 | 3 314.9 | 58 054, 0 (Critical overflow 0) | 59 of 59 |

On lavapipe a 400-unit frame takes about 1.5 s, dominated by rasterising the scene; the effects move it by less
than the run-to-run noise. This is the CI driver's reference that every effect draws within the budget and pool.

## Runs not of record (load 36 to 106)

Interleaved B, S, M triples taken while other users' builds loaded the machine; the effect counts are valid,
the frame times are not. The S arm (splashes only) was measured only here and in DS-01 (load 7 to 10, p50
15.55 ms with timestamp queries), so its cost relative to WP6c's +1 % is not established; the seven splash looks
shipped since WP6c (flame bands, lance grooves, cleave fronts) are heavier than the slice's three. A quiet S series
is an open item.

| Run | Load (start → end) | Frames | p50 | p95 | p99 | Update p50 | Effects spawned, refused |
|---|---|---:|---:|---:|---:|---:|---|
| B-01 | 55.9 → 64.4 | 2 579 | 19.20 | 47.06 | 84.36 | 15.60 | — |
| S-01 | 61.1 → 86.5 | 2 020 | 25.86 | 59.67 | 96.49 | 22.32 | 17 606, 0 |
| M-01 | 86.6 → 105.8 | 1 260 | 41.97 | 100.81 | 149.34 | 37.62 | 52 655, 0 |
| B-02 | 96.0 → 44.4 | 3 240 | 12.35 | 42.76 | 94.21 | 10.18 | — |
| S-02 | 35.5 → 85.2 | 2 558 | 21.34 | 42.00 | 58.35 | 18.99 | 17 595, 0 |
| M-02 | 61.2 → 53.7 | 2 499 | 18.79 | 52.49 | 101.38 | 16.37 | 54 055, 0 |
| B-03 | 42.5 → 15.7 | 4 627 | 11.90 | 19.58 | 21.80 | 9.76 | — |
| S-03 | 12.5 → 8.7 | 2 863 | 16.80 | 45.61 | 80.73 | 13.87 | 17 594, 0 |
| M-03 | 7.9 → 6.7 | 3 396 | 16.55 | 24.78 | 28.14 | 14.41 | 54 949, 0 |
| DB-01 | 9.6 → 7.5 | 4 429 | 12.30 | 20.59 | 27.65 | 9.64 | — |
| DS-01 | 7.2 → 10.2 | 2 944 | 15.55 | 44.42 | 98.23 | 13.25 | 17 594, 0 |
| DM-01 (pool 2 048) | 16.7 → 16.1 | 3 485 | 15.62 | 26.49 | 35.15 | 13.26 | 30 729, **24 608** |
| DM-02 (pool 4 096) | 22.5 → 11.8 | 3 160 | 17.44 | 27.52 | 36.53 | 14.84 | 54 672, 147 |
| DM-03 (pool 6 144) | 12.8 → 15.4 | 3 203 | 17.64 | 25.33 | 36.48 | 15.38 | 54 840, 0 |
| DP-01 | 5.5 → 5.3 | 3 660 | 15.40 | 22.88 | 24.97 | 13.27 | 32 599, 0 |

Raw logs and reports: `~/scratch/effects-viz-wp14/fb/` on the development server (not committed).
