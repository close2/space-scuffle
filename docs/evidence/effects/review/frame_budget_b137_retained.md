# Frame Budget after Retained Effect Instances: the Series of Record and the Decision Gate (WP7)

Plan: [`docs/plans/2026-11-19-retained-effect-instances.md`](../../../plans/2026-11-19-retained-effect-instances.md)
(B-137), WP7 and D-13, D-14. Measured on 2026-11-19 on the shared development server, with release binaries of the WP6
head (`0552a43c`). The figures of WP1 to WP6 are in [`retained_decals_baseline.md`](retained_decals_baseline.md).

## The series of record (D-14 layer 3)

`frame_budget_series --arms B,S,P --pairs 6 --display-path kwin-virtual --cpus 32-47`: 18 runs in balanced order, each
child under its own `with-gpu-lock` on the RX 580 with a headless KWin, at scenario 400. B is the shipped client, S adds
200 splashes a second, and P is the plan's §10.3 stress (S plus the held set of `--effect-mix` with no shots or deaths).
The report is `~/scratch/retained-decals/series-wp7/series-wp7.ron`.

| Run | Load (start → end) | Frames | p50 | p99 | Update p50 | CPU mean (ms/frame) |
|---|---|---:|---:|---:|---:|---:|
| B-00 | 26.8 → 11.1 | 4467 | 12.56 | 22.85 | 9.98 | 33.57 |
| S-00 | 11.1 → 16.2 | 3947 | 13.95 | 28.64 | 11.38 | 36.32 |
| P-00 | 16.2 → 36.1 | 2219 | 21.15 | 181.22 | 17.85 | 44.09 |
| P-01 | 36.1 → 25.8 | 2601 | 18.07 | 86.68 | 15.51 | 44.57 |
| S-01 | 25.8 → 30.4 | 2907 | 17.82 | 69.54 | 14.29 | 44.27 |
| B-01 | 30.4 → 41.4 | 3473 | 16.13 | 46.64 | 12.83 | 40.81 |
| B-02 | 41.4 → 37.6 | 3115 | 15.21 | 55.58 | 12.05 | 43.61 |
| S-02 | 37.6 → 29.4 | 2359 | 18.05 | 87.97 | 13.91 | 39.28 |
| P-02 | 29.4 → 18.6 | 3581 | 15.42 | 32.91 | 12.57 | 38.96 |
| P-03 | 18.6 → 12.8 | 3646 | 14.82 | 40.81 | 12.35 | 38.39 |
| S-03 | 12.8 → 10.3 | 4086 | 13.77 | 22.99 | 11.40 | 35.67 |
| B-03 | 10.3 → 9.4 | 4538 | 11.82 | 29.09 | 9.62 | 31.44 |
| B-04 | 9.4 → 40.8 | 3387 | 14.31 | 98.66 | 10.82 | 35.41 |
| S-04 | 40.8 → 25.0 | 2264 | 21.50 | 83.90 | 17.88 | 45.06 |
| P-04 | 25.0 → 18.6 | 3466 | 16.08 | 32.85 | 13.27 | 39.38 |
| P-05 | 18.6 → 21.7 | 3296 | 16.71 | 42.32 | 13.49 | 40.33 |
| S-05 | 21.7 → 21.7 | 2760 | 15.71 | 68.80 | 13.07 | 37.87 |
| B-05 | 21.7 → 16.2 | 3774 | 13.73 | 43.07 | 10.36 | 33.95 |

**Re-recorded baseline.** B's median wall p50 over the series is **14.02 ms**, at a one-minute load of 9 to 41. This is
the baseline of plan 2026-11-09 §10.3 asked for by its Amendment 33 step 4. It was taken on a loaded host, so it is not
a quiet-machine figure.

**Pairs against B** (pair `i` is the arm's run `i` against B's run `i`):

| Arm | p50 ratio per pair | Median p50 ratio | CPU per frame over B per pair (ms) | Median | Quiet pairs |
|---|---|---:|---|---:|---:|
| S | 1.11, 1.11, 1.19, 1.16 (quiet), 1.50, 1.14 | 1.15 | +2.74, +3.46, −4.33, +4.23, +9.65, +3.92 | **+3.69** | 1 |
| P | 1.68, 1.12, 1.01, 1.25, 1.12, 1.22 | 1.17 | +10.52, +3.76, −4.65, +6.95, +3.97, +6.38 | **+5.18** | 0 |

**Verdict: the bar is missed.** With fewer than 4 quiet pairs the verdict takes the CPU-time route (D-14). The bar is
5 % of max(14.02, 11.86) ms, that is **0.70 ms**. S is at +3.69 ms and P at +5.18 ms, five and seven times the bar. The
wall route says the same: the one quiet pair (S, load ≤ 16 at both runs' ends) is at a p50 ratio of 1.165, and every
pair's ratio is above 1.05 but one of P's (1.013).

**How firm the verdict is.** The pairs' CPU differences spread from −4.3 to +9.7 ms (S) and −4.7 to +10.5 ms (P),
far wider than the bar. One pair per arm is negative: pair 2, whose B run (B-02) measured the highest CPU of the B runs
under a load of 38 to 41. Five of six pairs per arm lie at +2.7 ms or more. So the miss is clear, but the size of the
remaining gap is not known better than to a few milliseconds.

Against WP1's series (base binaries, no quiet pair, S +3.62 ms, P +5.17 ms), the medians did not move. Over the same
work packages the frame build's instructions fell by 5.6 M (S) and 6.0 M (P) a frame (`effect_frame`, below). The
series therefore cannot resolve the saving: the process CPU time counts every thread, and its spread between B runs (31
to 44 ms a frame) dwarfs a saving of this size. The instruction counts are the supporting evidence that the main world's
build did shrink. They are not evidence that the frame met the bar.

## The remaining cost per phase (D-13)

**Build instructions per frame** (`effect_frame`, pinned to one CPU, marginal over 1 200 frames):

| Stream | Base (WP1) | WP5 | WP6 |
|---|---:|---:|---:|
| S | 10.10 M | 6.01 M | 4.51 M (−55 %) |
| P | 11.89 M | 7.38 M | 5.88 M (−51 %) |

**Self time of the remaining build** (`perf record` of `effect_frame` at WP6, share of the run's samples):

| Phase | S | P |
|---|---:|---:|
| Cards: `write_layers`, `build_frame` (inlined ledger and bursts), `place`, `push_card` | 49 % | 53 % |
| Decal table: re-sort of the row order (`sort_unstable_by_key` in `rewrite`) | 24 % | 18 % |
| Decal table: `rewrite`, `expire` | 4 % | 5 % |
| `max_placed_delay` (the ended-layer skip) | 3 % | 2 % |
| Card sort, copies, trigonometry, start-up | the rest | the rest |

The order is re-sorted on every frame whose rows changed, which at 200 splashes a second is every frame. The plan rated
this risk "low", at about 50 µs for 6 144 keys. It is now the second-largest phase of the build, a fifth to a quarter of
it.

**In the client** (`perf record -F 997 -g` of one S and one B run, series recipe, whole record): `client::effects`
functions take 4.5 % of S's samples and 1.1 % of B's. Most of the difference is the card path and the order re-sort
above, with `GpuDecalRecord::of` (the patches) at about 0.1 %.

## The GPU (one `--render-diagnostics` P run)

| Pass (RX 580, mean ms) | WP14 DB (no effects) | WP14 DP | WP7 P |
|---|---:|---:|---:|
| `effect_decal_age` (the new compute pass) | — | — | 0.034 (max 0.074) |
| `main_transparent_pass_3d` | 0.666 | 1.234 | 1.266 |
| Effect passes over DB | — | +0.57 | **+0.63** |

The effect passes, the aging pass included, stay far under §10.3's 1.5 ms.

## The kernel's buffer validation (D-12)

Amendment 34 found about 2 800 extra kernel samples of command submission and buffer validation in S over B
(`amdgpu_cs_ioctl` → `ttm_bo_validate`). With the decals no longer uploaded every frame (only the patches and the
order), the two arms are now close:

| Inclusive share of all samples | S | B |
|---|---:|---:|
| `amdgpu_cs_ioctl` | 1.28 % | 1.18 % |
| `ttm_bo_validate` | 0.68 % | 0.57 % |
| `amdgpu_vram_mgr_new` | 0.42 % | 0.34 % |
| Kernel (all) | 16.1 % | 15.3 % |

This is an observation, not a bar (D-12). The GPU is shared with another user.

## The decision gate (D-13): the next step is not chosen here

The bar is missed, so B-137 stays open with these figures. The plan's ranked options, recorded as given, are for the
owner or orchestrator to choose from (this hand-back does not choose):

1. **Parallel card build.** Evaluate slots in fixed chunks on Bevy's `ComputeTaskPool` into per-chunk retained
   buffers, concatenated in chunk order, so the emitted order is unchanged. The cards are about half of the remaining
   build.
2. **Budget data first** (§10.3: "a miss shrinks budgets in data first"). Shorter-lived Detail cards and a lower
   `min_screen_px`. This changes looks, so it needs Q0080 review rounds and the owner's look sign-off.
3. **Retained cards** (a plan of its own).

Observations beside the ranked list:
- The per-frame re-sort of the decal order (a fifth to a quarter of the build) is a cost inside this plan's own design.
  Keeping the order sorted on insertion, or merging the new rows into it, would remove most of it without a look change.
- The CPU-time verdict could not separate the WP1 and WP6 series. A quiet-machine wall series, which stays open under
  D-14, or a per-thread CPU column for the main world (the main-world update p50 is reported but is wall time), would
  measure what this series cannot.
