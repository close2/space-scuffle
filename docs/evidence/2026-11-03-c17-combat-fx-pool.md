# C-17: pooled combat effect materials, A/B on the RX 580 (open findings WP3)

This is the evidence for step 5 of WP3 in `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`, which closes
remediation finding **C-17**: every combat effect created its own `CombatFxMaterial`, and every live effect wrote
its age through `Assets::get_mut` on every frame, so Bevy prepared the material again (a new uniform buffer and a
new bind group) per live effect per frame. WP3 replaces that with pooled material slots, the effect's data in a
`CombatFxInstance` component, and a render-world `Queue::write_buffer` of changed instances (decision D-4).

The comparison is the plan's D-6 A/B: two release binaries of `frame_budget`, run alternately in one session on
the 400-unit scenario. It is attribution, not a frame budget of record (`DESIGN.md` §5 keeps those). No pass or
fail line applies (Q0015: measure and publish against the 16.7 ms and 33.3 ms lines).

## Conclusions

1. **The heavy combat frames lose their extra time.** In frames with 10 to 49 combat events and one simulation
   tick, pooled over the runs of record, the frame interval's median falls from **20.4 to 16.8 ms** and its p95
   from **27.0 to 20.2 ms**. Those were the frames in which the Phase 6 evidence found C-17's cost (20.4 against
   15.1 ms). The main-world update of those frames does not change (median 14.8 against 14.8 ms), so the time saved was
   outside the main world: extraction, render-asset preparation and drawing.
2. **The combat window's tail shortens; the whole run's median does not move.** Median over the runs of record:
   combat-window p95 **21.6 to 19.7 ms** and p99 **24.5 to 21.6 ms**; whole-run p50 **15.8 against 16.0 ms**. Heavy
   combat frames are fewer than one frame in twenty of a run, so the run's median is set by other frames. Frames
   without combat events read 1.0 ms slower in B (15.5 against 14.5 ms at the median), but the whole-run medians of
   single runs of one arm spread over 1.0 ms (A: 15.0 to 16.0 ms), and the only work B adds is the upload of changed live effects
   (0.05 ms per frame averaged over its `perf` run), so this is read as run-to-run variation.
3. **The per-effect preparation is gone from the profile.** Symbols specialised for `CombatFxMaterial` fall from
   1,667 to 1,225 `perf` samples (0.46 to 0.34 ms per rendered frame). The ones that existed only because effects
   were created, modified and removed disappear: `Assets::get_mut` (171 to 0), `extract_render_asset` (102 to 13),
   `unprepared_bind_group` and `as_bind_group` (106 to 1), and the hash-table growth of the render-asset map (88 to
   0). What remains is Bevy's per-entity material work (extraction, queueing, `RenderAssets::get`, setting the bind
   group), which every drawn effect costs under the `Material` API (D-4 option b would remove it; out of scope).
   wgpu's `create_bind_group` and `create_buffer` symbols, which count every material of the scene, fall from 2,309
   to 1,625 samples; `write_buffer` rises from 159 to 484, and the new upload system takes 185 samples (0.05 ms per
   frame).
4. **The pool stays small.** In the client's steady-state test (`combat_frames_reuse_effect_materials`, the default
   battle at 40 ms per frame), a 240-frame combat window with 157 combat events spawned 173 effects and added no
   material: the pool had reached its peak during the 120 warm-up frames.

## Arms, host and display path

- **Arm A (base):** `e85f79f`, the plan's base, exported with `git archive` into a scratch directory and built with
  `cargo build --release -p client_diagnostics --bin frame_budget --locked` in
  `CARGO_TARGET_DIR=~/scratch/cargo-target/planbase`. Its reports say "Commit: unavailable" because the export is
  not a git checkout.
- **Arm B (WP3):** commit `ca7d386`, a work-in-progress commit of the WP3 branch that was later squashed. Its
  production sources (`crates/*/src`) are identical to those of the WP3 commit; only tests and this file changed
  after it. Built the same way in `CARGO_TARGET_DIR=~/scratch/cargo-target/open-wp3`.
- **Host:** the shared development VM, AMD EPYC 7301 under KVM, Linux 6.12.115. CPUs 32 to 47 were dedicated to this
  work and every run was pinned to them with `taskset -c 32-47`. Two other agents were building and testing on the
  machine during the session, which is why load is recorded per run.
- **GPU:** AMD Radeon RX 580 Series (RADV POLARIS10), Mesa `25.0.7-2+deb13u1`, Vulkan.
- **Display path:** `kwin-virtual` (KWin 6.3.6, `kwin_wayland --virtual --xwayland`), 1280×720, present mode
  `AutoNoVsync`, as in `docs/evidence/2026-11-03-frame-budget.md` and Q0019's answer.
- **Scenario:** `400`, the eight-player arena of `crates/client_diagnostics/data/frame_budget.ron`: 180 warm-up
  frames, 1,500 measured ticks, combat window of 750 ticks from the first combat event (tick 14 in every run).
- **Fixture roots:** one per arm, prepared once on `lavapipe` under `xvfb-run` without the lock
  (`--scenario 0 --warmup-frames 1 --measure-frames 1`).

## How each run was taken

One `with-gpu-lock` command per run, KWin and the tool inside it, released between runs with a 10 s gap. With `FB`
the arm's release binary, `W` the arm's scratch directory and `NN` the run number:

```bash
WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json \
with-gpu-lock bash -c "date -u +%s.%N > $W/run-NN.lockgot; \
  taskset -c 32-47 kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts \
    --width 1280 --height 720 --socket open-wp3-ARM-NN \
    --exit-with-session '$FB --display-path kwin-virtual --work-dir $W/work --scenario 400 --out $W/run-NN.ron'"
```

The `perf` runs put `perf record -F 997 -o $W/perf-NN.data --` in front of `$FB` inside the session string. A run
counts only if its report exists and its log lacks `frame_budget failed`; every run below passed both checks. Load
is the one-minute load average from `/proc/loadavg`, recorded by the tool just before and just after measuring. A
run is **loaded** if it started above 8 (D-6) and **disturbed** if the load rose above 8 during it; neither counts
among the runs of record, and each was answered with a repeat at the end. Statistics are nearest-rank, recomputed
from each report's frame rows; they match the tool's own summaries.

## 1. Runs (400 units, headless KWin, 1280×720)

Taken on 2026-11-03 from 22:05 UTC, in this order. Times in ms; the combat columns cover the 750-tick combat window.

| Run | Arm | Load before | Load after | Status | Frames | p50 | p95 | p99 | max | Update p50 | Update p95 | Combat frames | Combat p50 | Combat p95 | Combat p99 |
|---|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 01 | A | 13.21 | 6.16 | loaded | 3,674 | 16.1 | 21.1 | 23.7 | 67.7 | 13.9 | 17.6 | 1,804 | 16.2 | 22.2 | 24.6 |
| 02 | B | 5.14 | 3.67 | record | 3,737 | 16.0 | 19.7 | 21.5 | 65.6 | 13.9 | 17.7 | 1,897 | 15.7 | 19.6 | 21.6 |
| 03 | A | 4.51 | 6.78 | record | 3,627 | 16.0 | 23.7 | 28.4 | 71.7 | 13.9 | 19.1 | 1,726 | 16.5 | 25.8 | 29.9 |
| 04 | B | 4.62 | 3.22 | record | 3,708 | 16.2 | 19.7 | 21.9 | 68.6 | 14.1 | 17.7 | 1,862 | 16.1 | 19.7 | 21.5 |
| 05 | A | 3.36 | 4.39 | record | 3,743 | 15.8 | 20.7 | 22.7 | 64.6 | 13.9 | 17.6 | 1,833 | 16.1 | 21.3 | 23.0 |
| 06 | B | 4.00 | 3.92 | record | 3,799 | 15.6 | 19.7 | 23.5 | 70.0 | 13.5 | 17.6 | 1,909 | 15.5 | 19.7 | 23.3 |
| 07 | A | 3.70 | 3.98 | perf | 3,471 | 17.0 | 22.5 | 25.0 | 71.7 | 14.9 | 19.2 | 1,722 | 17.0 | 23.3 | 25.1 |
| 08 | B | 4.49 | 6.24 | perf | 3,434 | 17.1 | 22.6 | 26.3 | 121.0 | 14.9 | 19.7 | 1,703 | 17.1 | 23.5 | 28.3 |
| 09 | A | 6.34 | 15.28 | disturbed | 2,259 | 25.2 | 42.7 | 48.2 | 99.7 | 20.8 | 37.0 | 1,143 | 24.3 | 42.1 | 48.2 |
| 10 | B | 16.58 | 16.09 | loaded | 2,728 | 18.1 | 41.4 | 49.9 | 76.3 | 15.3 | 33.2 | 956 | 31.4 | 47.5 | 52.4 |
| 11 | A | 4.87 | 5.77 | record | 3,885 | 15.0 | 20.6 | 23.3 | 67.4 | 12.7 | 17.5 | 1,830 | 16.1 | 21.6 | 24.5 |
| 12 | B | 4.62 | 10.76 | disturbed | 3,855 | 15.0 | 21.5 | 26.7 | 153.8 | 12.7 | 18.0 | 1,779 | 16.3 | 23.5 | 29.3 |

**Runs of record:** A 03, A 05, A 11, B 02, B 04, B 06.

**Medians over the runs of record:**

| Arm | Runs | Frame interval p50 | p95 | p99 | Update p50 | Combat p50 | Combat p95 | Combat p99 |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| A | 03, 05, 11 | 15.8 | 20.7 | 23.3 | 13.9 | 16.1 | 21.6 | 24.5 |
| B | 02, 04, 06 | 16.0 | 19.7 | 21.9 | 13.9 | 15.7 | 19.7 | 21.6 |

## 2. Frames grouped by combat events, one tick that frame

As in the Phase 6 evidence (its §2 table), every frame of the runs of record that executed exactly one simulation
tick, grouped by the number of combat events it published. Pooled over the runs of record of each arm.

| Combat events | Arm | Frames | Mean live units | Update p50 | Interval p50 | Interval p95 |
|---|---|---:|---:|---:|---:|---:|
| 0 | A | 1,074 | 44 | 14.3 | 14.5 | 18.6 |
| 0 | B | 1,074 | 44 | 15.1 | 15.5 | 18.6 |
| 1 to 9 | A | 2,922 | 97 | 14.6 | 15.0 | 21.1 |
| 1 to 9 | B | 2,922 | 97 | 14.9 | 15.3 | 18.7 |
| 10 to 49 | A | 498 | 289 | 14.8 | 20.4 | 27.0 |
| 10 to 49 | B | 498 | 289 | 14.8 | 16.8 | 20.2 |

## 3. `perf` attribution (one run per arm)

`perf report --no-children --sort sym` over runs 07 (A) and 08 (B). Milliseconds per frame are samples ÷ 997 ÷
rendered frames (measured plus the 180 warm-up frames). `perf` slows a run by about 8 %, so these runs' own frame
figures are not used above.

| | A (run 07) | B (run 08) |
|---|---:|---:|
| All samples | 184 K | 174 K |
| Rendered frames | 3,651 | 3,614 |
| Symbols specialised for `CombatFxMaterial` | 1,667 (0.46 ms/frame) | 1,225 (0.34 ms/frame) |
| of which `Assets::<CombatFxMaterial>::get_mut` | 171 | 0 |
| of which `extract_render_asset` | 102 | 13 |
| of which `unprepared_bind_group` and `as_bind_group` | 106 | 1 |
| of which `prepare_assets` | 57 | 32 |
| of which hash-table growth of the render-asset map | 88 | 0 |
| of which `RenderAssets::get` and other lookups | 431 | 510 |
| of which per-entity extraction, queueing and draw (`extract_mesh_materials`, `queue_*`, `SetMaterialBindGroup`) | 528 | 562 |
| `upload_combat_fx_uniforms_system` | n/a | 185 (0.05 ms/frame) |
| wgpu `create_bind_group` (every material in the scene) | 1,410 | 1,039 |
| wgpu `create_buffer` (every material in the scene) | 899 | 586 |
| wgpu `write_buffer` | 159 | 484 |

The remaining `CombatFxMaterial` cost is per drawn entity, not per created or modified asset, so it follows the
number of live effects and is the same for any material in Bevy 0.15.

## 4. Correctness evidence behind the measurement

- `crates/client/tests/sync_steady_state_tests.rs::combat_frames_reuse_effect_materials`: over 240 frames of
  steady combat, `CombatFxMaterial` raises no `Modified` and no `Removed` event, and adds no more materials than
  the pool created. It failed on `main` with 844 `Modified` events.
- `crates/client/tests/combat_fx_slot_gpu_tests.rs::a_reused_slot_draws_its_new_effect`, on the RX 580: a second
  effect drawn with the first effect's slot shows its own colour (light by channel: first `[2.07, 3.76, 5.75]`
  million levels, blue; second `[5.76, 2.55, 2.29]`, red), and aging it to 0.97 fades the card to black. With the
  render-world write removed, the second capture repeated the first exactly.
- `crates/client/src/vfx/pool.rs` unit tests: slot reuse, a 500-spawn script in which no slot is ever held twice,
  and slot return on every despawn path.

Raw reports, logs and `perf` data are under `~/scratch/open_wp3_ab/` and `~/scratch/logs/open_wp3_*`. They are
scratch files that may be cleaned at any time, so the tables here are the record.
