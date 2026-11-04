# Battle flipbook animation: frame budget before and after, and capture determinism (WP7)

This is the evidence for WP7 of [`docs/plans/2026-11-04-battle-flipbook-animation.md`](../plans/2026-11-04-battle-flipbook-animation.md).
It changes no code. It answers two questions about the presentation clock that WP4 to WP6 added: does it cost frame
time at 400 units, and does a capture still produce the same pixels when it is run twice?

No figure here is a frame budget of record. `DESIGN.md` §5 keeps those, and no pass or fail line applies (Q0015).

## Conclusions

1. **The animation costs no measurable frame time at 400 units.** Arm *base* is `main` at `d5779f7` with only
   animation WP4, WP5 and WP6 reverted. Arm *new* is `d5779f7`. Medians of five interleaved runs each:

   - frame interval p50: 12.13 → 12.03 ms (−0.8 %);
   - main-update p50: 9.90 → 9.73 ms (−1.7 %);
   - combat-window p50: 13.34 → 13.02 ms.

   Every p50 is inside the plan's 3 % line, so no attribution was owed. The plan expected exactly this: per-bubble
   packing gains a few integer operations and no allocation.
2. **The tails are within the noise of this shared machine.** The new arm's p95 and p99 medians are higher: 16.77 →
   18.11 ms and 19.36 → 22.60 ms. The per-run ranges overlap almost completely, though. Base p95 runs from 14.17 to
   19.70 ms and new from 16.31 to 18.93 ms. Base p99 runs from 15.97 to 28.99 ms and new from 18.42 to 31.02 ms. The
   base median is pulled down by run 13, the quietest run of the series. The combat window's p95, which holds the
   bubbles that animate most, is lower in the new arm (17.61 → 17.40 ms). On this evidence no tail cost can be
   attributed to the animation.
3. **The GPU cost of frame blending is below what one diagnostic run can resolve.** The bubbles are drawn in
   `main_transparent_pass_3d` (`AlphaMode::Blend`). That pass took a mean of 0.458 ms per frame in the base arm and
   0.460 ms in the new arm, +0.002 ms. The plan's limit was +0.3 ms. The instrumented passes add up to 3.588 and
   3.533 ms. Blending stays on (Q0020). There is nothing to review.
4. **Captures are deterministic on the RX 580.** Two `battle_capture` runs of the default script at `d5779f7`
   produced seven byte-identical PNGs out of seven. Their manifests differ only in the output directory inside each
   image path: ticks, presented ticks, checksums, cameras, pixel metrics and pipeline counts are all equal.
5. **The `lavapipe` capture agrees as far as it got.** It matched the RX 580 run tick for tick and checksum for
   checksum on every checkpoint it reached: overview, ramp entry and ramp exit. It did not reach the combat
   checkpoint. The script's wait budget is counted in frames, and software rendering on a machine at a load of 9 to
   37 runs out of frames before tick 822. The per-bubble layer comparison the plan asks for could not be made: the
   capture manifest does not record the layer each bubble chose (see "Deviations").
6. **The plan's literal baseline mostly measures the C-18 smog change.** That baseline is `main` just before
   animation WP4 (`be5a476`). The C-18 smog quiescence (open-findings WP9, `1629c00`) and the combat rebalances
   (`317dcc3`) were merged between it and `d5779f7`. That is why arm *base* exists. Arm *pre* (`be5a476`) ran 19.98 ms
   at the median against the new arm's 12.03 ms, and its after-combat p50 was 20.34 ms against 11.33 ms. That
   matches the size of the C-18 A/B in [`2026-11-04-c18-smog-solver.md`](2026-11-04-c18-smog-solver.md) §8.
   It is recorded here for completeness and is not an effect of the animation.

## Arms, host and build

| Arm | Source | What it contains |
|---|---|---|
| `pre` | `be5a476` | `main` before animation WP4, the plan's literal baseline (WP1 already merged) |
| `base` | `d5779f7` with `git revert -m 1` of `d5779f7`, `ec04bf4` and `4bd48cf`, in that order (local commit `f2c007d`, not pushed) | today's `main` without the presentation clock: no `flipbook_clock.rs`, the 160-byte uniform, bubbles on frame 0 of their slice |
| `new` | `d5779f7` | animation WP1 and WP3 to WP6 |

Two revert conflicts were resolved mechanically, and neither touches the measured binary.
`flipbook_clock.rs` was taken at its WP4 state so that WP4's own revert could delete it, and
`smog_golden_tests.rs` was kept as on `main`. Against `be5a476`, the base arm's `crates/client/src` differs only
in the C-18 solver (`smog/fluid.rs`) and one test fixture line. Its `client_diagnostics` is identical.

- **Build:** `cargo build --release --locked -p client_diagnostics --bin frame_budget --bin battle_capture`, for
  each arm in a `git archive` export under `~/scratch/anim_wp7/src-<arm>`, with
  `CARGO_TARGET_DIR=~/scratch/cargo-target/anim-wp7{-pre,-base,}`. All builds ran outside the lock and before the
  runs. The reports' `commit` field reads "unavailable" because the exports are not git checkouts.
- **Host:** the shared development VM, AMD EPYC 7301 under KVM, Linux 6.12.115. Every run was pinned with
  `taskset -c 32-47`. Another agent's client test binaries were also running on those CPUs during parts of the
  session, which the CPU-busy column shows.
- **GPU:** AMD Radeon RX 580 (RADV POLARIS10), Mesa `25.0.7-2+deb13u1`. Compositor: KWin 6.3.6
  (`kwin_wayland --virtual --xwayland`), 1280×720, `--display-path kwin-virtual`.
- **Scenario:** `400` of `crates/client_diagnostics/data/frame_budget.ron`: eight players, 50 units each,
  180 warm-up frames, 1,500 measured ticks, combat window 750 ticks from the first combat event.

## How each run was taken

Each run was taken exactly as in [`2026-11-03-frame-budget.md`](2026-11-03-frame-budget.md), "How each run was
taken". It was one `with-gpu-lock` command holding KWin and the tool together, then the lock was released, then a
10 s gap:

```bash
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
with-gpu-lock bash -c "date -u +%s.%N > $W/run-NN.lockgot; grep -E '^cpu(3[2-9]|4[0-7]) ' /proc/stat > $W/run-NN.stat0; \
  taskset -c 32-47 kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts \
    --width 1280 --height 720 --socket anim-wp7-NN \
    --exit-with-session '$FB --display-path kwin-virtual --work-dir $W/work --scenario 400 --out $W/run-NN.ron'; \
  grep -E '^cpu(3[2-9]|4[0-7]) ' /proc/stat > $W/run-NN.stat1"
```

- Each arm's fixture root was prepared once without the GPU, on `lavapipe` under `xvfb-run`, with
  `--scenario 0 --warmup-frames 1 --measure-frames 1`.
- **Order:** `pre base new`, `base new pre`, `new pre base`, `pre base new`, `base new pre` (runs 01 to 15). Then
  one `--render-diagnostics` run per arm (16 to 18).
- **Lock wait** is the time from just before the request to the first command inside the lock. **Load** is the
  one-minute load average the tool records before the run. **CPUs busy** is the share of CPUs 32 to 47 that was
  busy during the run, from `/proc/stat`, including the run itself (about 2.8 cores).
- **Acceptance:** a run counted only if its report existed and its log did not contain `frame_budget failed`. All
  18 runs passed both checks.
- **Load rule.** Every run but one started above D-4's "loaded" threshold of 8, or near it. The shared machine
  stayed between 3.4 and 42.5 for the whole session, so repeating the loaded runs would not have produced unloaded
  ones. All runs are kept, as the protocol says. The interleaving spreads the load over the arms.
- Statistics were recomputed from each report's frame rows with the tool's nearest-rank definition and its
  combat-window rule. The after-combat window is every frame past the combat window.

Raw reports, logs and captures are under `~/scratch/anim_wp7/` and `~/scratch/logs/anim_wp7_*`. They are scratch
files that may be cleaned at any time, so the tables here are the record.

## 1. Frame budget at 400 units (runs 01 to 15)

Medians over the five runs of each arm, in ms:

| Arm | Frame interval p50 | p95 | p99 | max | Main update p50 | Update p95 | Simulation p50 | Combat p50 | Combat p95 | After-combat p50 | After-combat p95 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `pre` (`be5a476`) | 19.98 | 40.94 | 56.95 | 115.0 | 16.23 | 36.30 | 1.72 | 17.98 | 30.71 | 20.34 | 46.55 |
| `base` (`d5779f7` minus WP4–6) | 12.13 | 16.77 | 19.36 | 69.4 | 9.90 | 13.46 | 0.10 | 13.34 | 17.61 | 11.30 | 14.67 |
| `new` (`d5779f7`) | **12.03** | 18.11 | 22.60 | 68.0 | **9.73** | 14.17 | 0.10 | 13.02 | 17.40 | 11.33 | 14.89 |
| new vs base | −0.8 % | +8.0 % | +16.7 % | −2.0 % | −1.7 % | +5.3 % | 0 | −2.4 % | −1.2 % | +0.3 % | +1.5 % |

Per run (times in ms; "Combat" and "After" are p50 / p95 / update p50 of their windows):

| Run | Arm | Load | CPUs 32–47 busy | Lock wait (s) | Frames | p50 | p95 | p99 | max | Update p50 | Update p95 | Sim p50 | Combat | After |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 01 | pre | 42.48 | 74.3 % | 0.0 | 2,501 | 20.71 | 47.34 | 68.52 | 115.5 | 17.49 | 41.44 | 1.82 | 20.86 / 46.82 / 17.14 | 20.66 / 48.15 / 17.86 |
| 02 | base | 21.36 | 21.1 % | 30.2 | 4,719 | 12.13 | 18.31 | 23.20 | 71.8 | 9.90 | 13.89 | 0.09 | 13.49 / 20.39 / 10.43 | 11.30 / 14.67 / 9.50 |
| 03 | new | 6.03 | 23.5 % | 30.4 | 4,767 | 12.08 | 18.06 | 22.60 | 71.9 | 9.73 | 14.17 | 0.10 | 13.72 / 19.46 / 10.45 | 10.97 / 14.85 / 9.22 |
| 04 | base | 4.54 | 15.8 % | 1.4 | 4,845 | 12.06 | 16.77 | 18.70 | 67.7 | 9.84 | 13.26 | 0.10 | 13.34 / 17.61 / 10.21 | 11.14 / 14.62 / 9.43 |
| 05 | new | 3.41 | 16.0 % | 7.0 | 4,784 | 12.01 | 18.11 | 22.20 | 68.6 | 9.86 | 13.63 | 0.10 | 13.02 / 19.93 / 10.15 | 11.33 / 14.73 / 9.56 |
| 06 | pre | 4.64 | 42.7 % | 11.9 | 3,392 | 16.43 | 27.07 | 38.17 | 66.4 | 13.46 | 23.88 | 0.15 | 14.97 / 20.62 / 11.93 | 20.34 / 32.71 / 17.68 |
| 07 | new | 15.93 | 16.5 % | 24.2 | 4,673 | 12.03 | 18.51 | 31.02 | 64.9 | 9.66 | 15.70 | 0.10 | 11.87 / 15.85 / 9.13 | 12.21 / 22.87 / 10.23 |
| 08 | pre | 14.75 | 32.9 % | 14.1 | 3,876 | 14.44 | 23.14 | 35.97 | 68.3 | 12.07 | 19.57 | 0.12 | 14.73 / 21.43 / 11.63 | 14.26 / 24.80 / 12.49 |
| 09 | base | 11.57 | 18.3 % | 6.9 | 4,783 | 12.25 | 16.50 | 19.36 | 72.8 | 10.06 | 13.46 | 0.10 | 13.24 / 17.03 / 10.25 | 11.58 / 15.16 / 9.88 |
| 10 | pre | 7.29 | 17.3 % | 0.2 | 2,664 | 19.98 | 40.94 | 59.81 | 115.0 | 16.23 | 36.30 | 1.72 | 17.98 / 30.71 / 13.93 | 24.82 / 51.94 / 21.95 |
| 11 | base | 25.38 | 20.3 % | 4.7 | 4,510 | 12.39 | 19.70 | 28.99 | 69.4 | 10.03 | 16.16 | 0.10 | 14.56 / 23.95 / 10.91 | 11.31 / 15.19 / 9.55 |
| 12 | new | 14.81 | 32.8 % | 5.5 | 5,114 | 10.82 | 18.93 | 29.58 | 68.0 | 8.74 | 14.61 | 0.08 | 11.97 / 17.40 / 9.10 | 9.62 / 21.18 / 8.07 |
| 13 | base | 7.88 | 22.0 % | 1.3 | 5,537 | 10.74 | 14.17 | 15.97 | 64.8 | 8.54 | 11.99 | 0.08 | 10.98 / 14.24 / 8.39 | 10.40 / 14.02 / 8.71 |
| 14 | new | 7.21 | 25.9 % | 0.5 | 4,762 | 12.46 | 16.31 | 18.42 | 62.8 | 10.04 | 13.41 | 0.10 | 13.71 / 17.19 / 10.52 | 11.44 / 14.89 / 9.62 |
| 15 | pre | 7.84 | 30.6 % | 7.5 | 2,584 | 21.23 | 41.16 | 56.95 | 122.8 | 17.77 | 36.84 | 1.74 | 22.65 / 35.63 / 18.67 | 19.13 / 46.55 / 16.65 |

The longest lock waits, 24 to 30 s for runs 02, 03 and 07, were waits for another user's GPU slot. Each locked
command held the GPU for about 70 s.

**Not attributed: the `pre` arm is bimodal.** Runs 01, 10 and 15 ran at 20 to 21 ms with a simulation p50 of
1.7 ms and 2,500 to 2,700 frames. Runs 06 and 08 ran at 14 to 16 ms with a simulation p50 of 0.1 ms. A simulation
p50 of 1.7 ms suggests frames that execute more than one tick to catch up. That arm predates C-18's quiescence, and
the other two arms do not show the pattern. It does not bear on the animation, so it was not pursued.

## 2. GPU passes (runs 16 to 18, `--render-diagnostics`; not the frame budget)

One run per arm, mean and maximum GPU time per frame, in ms. The cascades are nested in `shadows`, and the sum
counts only passes that are not nested.

| Pass | `base` mean | max | `new` mean | max | `pre` mean | max |
|---|---:|---:|---:|---:|---:|---:|
| `main_opaque_pass_3d` | 2.187 | 2.270 | 2.184 | 2.188 | 2.239 | 2.451 |
| `main_transparent_pass_3d` (bubbles) | **0.458** | 1.308 | **0.460** | 1.215 | 0.493 | 1.302 |
| `prepass` | 0.198 | 0.768 | 0.198 | 0.200 | 0.159 | 0.632 |
| `shadows` | 0.746 | 3.241 | 0.692 | 1.376 | 0.777 | 3.248 |
| **Sum of passes not nested** | **3.588** | | **3.533** | | **3.667** | |

| Run | Arm | Load | CPUs 32–47 busy | Lock wait (s) | p50 | p95 | Update p50 |
|---|---|---:|---:|---:|---:|---:|---:|
| 16 | base | 17.95 | 77.4 % | 9.3 | 14.53 | 21.94 | 11.56 |
| 17 | new | 19.33 | 54.9 % | 0.3 | 11.65 | 20.89 | 9.54 |
| 18 | pre | 14.32 | 15.2 % | 0.0 | 14.13 | 17.68 | 11.90 |

Run 16 shared its CPUs with the other agent's tests (77 % busy), which explains its frame figures. GPU pass times
are measured on the GPU and are not affected by CPU load in the same way.

## 3. Capture determinism (`battle_capture`, default script, `d5779f7`)

The default script `captures/default_battle.ron` renders at 1920×900, so KWin ran at `--width 1920 --height 900`.
A first attempt at 1280×720 was refused by the tool ("screenshot dimensions differ from requested physical
resolution") and is not counted. Each RX 580 capture was one locked command:

```bash
SPACE_SCUFFLE_ASSETS=~/scratch/anim_wp7/src-new/assets WGPU_BACKEND=vulkan \
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock bash -c "... \
  kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts --width 1920 --height 900 \
    --socket anim-wp7-cap-NAME --exit-with-session '$BC --out ~/scratch/anim_wp7/cap-NAME'"
```

| Checkpoint | Tick | Checksum | `radv1` SHA-256 (prefix) | `radv2` | `lavapipe` |
|---|---:|---|---|---|---|
| `01-overview` | 0 | 11741425318084745558 | `f9fae925…` | identical | same tick and checksum |
| `02-ramp-entry` | 162 | 13374663991037547240 | `a9c3bbcc…` | identical | same tick and checksum |
| `03-ramp-exit` | 195 | 9608615364495996334 | `abef7ec9…` | identical | same tick and checksum |
| `04-combat` | 822 | 16433788678318063248 | `e7fa39a1…` | identical | not reached (wait budget) |
| `05-visual-lab` | 822 | 16433788678318063248 | `8b68ab6b…` | identical | not reached |
| `06-battle-return` | 822 | 16433788678318063248 | `92cbdb4c…` | identical | not reached |
| `07-reset` | 0 | 11741425318084745558 | `ff13a931…` | identical | not reached |

- **RX 580, twice:** both runs logged seven `Verified runtime capture` lines and exited 0. All seven PNGs are
  byte-identical, and the two manifests differ only in their output directory names.
- **`lavapipe` (`llvmpipe`, under `xvfb-run` at 1920×900, no lock):** two attempts. The first, at a load of 37,
  timed out at checkpoint 02 after 145 frames at tick 132. The second, at a load of 9 to 23, timed out at checkpoint
  04 after 531 frames at tick 502. Every checkpoint either attempt reached matches the RX 580 runs. The script's
  wait budgets are counted in frames, and software rendering on this shared machine is too slow to fit them. This
  is a limit of the environment, not a divergence.

## Deviations from the plan

- **Baseline arm.** The plan says "at `main` before WP4". That commit is measured as arm `pre`. The comparison of
  record is arm `base`, because unrelated changes between `be5a476` and `d5779f7` (C-18 WP9, combat WP4 to WP6)
  move the frame time by about 40 %. Arm `base` holds them equal.
- **Layer per bubble on `lavapipe`.** The plan asks for this to be read "from the manifest the tool writes". The
  `battle_capture` manifest has no per-bubble layer field (`runtime_capture/artifacts.rs`, `Record`), and the
  `lavapipe` run did not reach a checkpoint with fighting bubbles. What is shown instead is that CPU and GPU pick
  the same frame on both backends. WP4's GPU parity test (`flipbook_clock_gpu_tests.rs`) proves it on `lavapipe`
  in CI, and the identical RX 580 captures prove the result is repeatable. Adding the field is a small
  `client_diagnostics` change for a later package. WP7 is measurement only.
- **Capture with and without blending** (§6 risk table). Blending is a Lab toggle (`opt_frame_blend`). The battle
  packs the default feature flags and has no switch that `battle_capture` can reach, so no battle capture without
  blending was taken. The GPU table above shows blending's cost, and Q0020 keeps it on.
