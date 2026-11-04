# C-18: smog solver baseline (open findings WP6)

This is the evidence file of remediation finding **C-18** (the smog solver, `SmogGrid3d::step`, costs several
milliseconds of CPU per frame), opened by WP6 of `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`. WP6
records the baseline that WP7, WP8 and WP9 compare against: four new characterisation goldens recorded on the
unmodified solver, a solver micro-benchmark, and a `perf` attribution of the solver on the frame-budget path. WP7
and WP8 append their benchmark rows here; WP9 appends its A/B frame-budget runs and closes the file.

No figure here is a frame budget of record (`DESIGN.md` §5 keeps those), and no pass or fail line applies (Q0015).

## 1. Characterisation goldens

Added to `crates/client/tests/smog_golden_tests.rs`, recorded on `main` at `db22ac2`, whose
`crates/client/src/smog/fluid.rs` is byte for byte that of `e85f79f`, the plan's base (the only change to
`smog/` since then is WP1's `kinematics.rs`). All four are class L (`libm`-bound), like the three fluid goldens
before them, and hash with the file's `Fnv64::grid` (every voxel's density, velocity, glow and vorticity
magnitude, plus both upload buffers) and `Fnv64::metrics` (every step).

| Test | Scenario | Constant |
|---|---|---|
| `fluid_wake_decay_golden` | Showcase sources (Visual Lab volume, default configuration) for 60 steps at 1/60 s, then no source for 240 steps; field hashed every 30 steps | `GOLDEN_FLUID_WAKE_DECAY = 0x6f8f_62ab_607f_d719` |
| `fluid_fast_sources_golden` | 20 sources from the crowd lattice rescaled to 8 to 12 world units per second, 120 steps at `dt = 0.05` (the clamp); the fluid peaks at 6.28 world units per second, a back-trace of 2.5 voxels along z; the test asserts more than 2 | `GOLDEN_FLUID_FAST_SOURCES = 0x7ffa_1917_b16b_ea08` |
| `fluid_bounds_shift_golden` | Showcase sources moving with the window; 40 steps, `set_bounds_quantized` by (+3, 0, −2) voxels, 40 steps, a shift of 100 voxels along x (the clearing path), 20 steps; field hashed every 30 steps, at each phase end and right after each shift | `GOLDEN_FLUID_BOUNDS_SHIFT = 0xb677_0ad8_9bf1_a12a` |
| `fluid_variable_dt_golden` | 120 steps cycling `dt` through 1/240, 1/144, 1/60, 1/30, 0.07 and 0 s, with the source count switching between 30 and 40 every 5 steps (coprime with the 6-step cycle), so confinement turns on and off and stale vorticity is packed | `GOLDEN_FLUID_VARIABLE_DT = 0xf4fc_5e58_2628_330d` |

The new steps (1/240, 1/144 and 0.05 s) feed `expf` arguments the existing canary did not cover. Their decay
factors are pinned by a second hash in `smog_golden_libm_canary`, `GOLDEN_LIBM_CANARY_C18 = 0x98d9_3224_8b8e_11df`,
so `GOLDEN_LIBM_CANARY` keeps its recorded value.

### Mutation red runs

The goldens pin current behaviour, so their red runs are mutations: each line below was changed in
`crates/client/src/smog/fluid.rs`, the whole of `smog_golden_tests.rs` (19 tests) was run, and the line was restored.
Every new golden fails under at least one mutation that none of the seven earlier fluid and system goldens catches.

| Mutation | Fails | New goldens that catch it where no earlier golden does |
|---|---|---|
| Floor decay `0.018 * dt` → `0.019 * dt` (the plan's WP6 red run) | all four new goldens, `fluid_showcase_golden`, the startup, Visual Lab, swarm and live-battle system goldens (9) | (earlier goldens fail too) `fluid_wake_decay_golden` fails, as the plan requires |
| Confinement skipped on a step without sources (`active_sources > 0 && active_sources <= 32`) | `fluid_wake_decay_golden` only | wake decay |
| `dt` clamp `0.05` → `0.049` | `fluid_fast_sources_golden`, `fluid_variable_dt_golden` | fast sources (and variable `dt`) |
| `dt` clamp `0.05` → `0.06` | `fluid_variable_dt_golden` only | variable `dt` |
| Stale vorticity cleared when confinement is skipped (`else { vorticity_mag.fill(0.0) }`) | `fluid_variable_dt_golden` only | variable `dt` |
| Quantized shift copies from `ix - dx` instead of `ix + dx` | `fluid_bounds_shift_golden` only | bounds shift |
| A shift wider than the grid keeps the field instead of clearing it | `fluid_bounds_shift_golden` only | bounds shift |
| Tint of voxels exposed by a quantized shift `0.60` → `0.61` (red channel) | none | none: the known tint hole |

The last row is the hole described in the plan's §2.1: tint reaches the hash only through the density bytes, where
density exceeds `1e-5`, and no smog reaches the exposed voxels within the 60 steps that follow the shift. The goldens
cannot close it without hashing the tint slabs, which the public API does not expose; WP8's differential test,
which compares every slab directly, is the check the plan assigns to it.

Logs: `~/scratch/logs/open_wp6_mut_*.log` (scratch; may be cleaned).

## 2. Solver micro-benchmark

`crates/client/benches/smog_solver.rs`, run with `cargo bench -p client --bench smog_solver -- --contend K`. It
times `SmogGrid3d::step` alone, at `dt = 1/60` s with the default configuration, and prints mean, p50 and p95 ms
per step per scenario. `--contend K` runs `K − 1` more grids on their own threads on the same scenario while the
first is measured, to reproduce the memory contention of the plan's §5.2. It is a relative measure for the C-18
chain; the A/B frame-budget runs are the figures of record.

| Scenario | Volume | Sources | Steps |
|---|---|---:|---:|
| `empty` | live-battle window, 28 × 4.8 × 22 world units | 0 | 600 |
| `showcase` | Visual Lab volume, 13 × 2.8 × 9 | 10 bouncing (confinement on) | 600 |
| `crowd_48` | Visual Lab volume | 48 bouncing (confinement off) | 600 |
| `battle_240` | live-battle window | 240 in rows of 20, alternate rows marching ±x at 2.5 world units per second, wrapping | 600 |
| `after_fight` | live-battle window | `battle_240` for 30 s, then none for 30 s; each half its own row | 1,800 + 1,800 |

### Baseline at `e85f79f`'s solver

The bench binary was built from this branch (`db22ac2` plus WP6), whose `fluid.rs` equals `e85f79f`'s, so it measures
the base solver; the bench target itself does not exist at `e85f79f`. Release profile, `taskset -c 32-47`, three
rounds, `--contend 1` and `--contend 8` alternating, on 2026-11-04 from 01:18 to 01:48 UTC.

The 16 CPUs 32 to 47 are dedicated to this plan's work but were shared with two other agents' builds and test runs
during the session, and a third user kept the machine's load high. The load figures, recorded around each run by
the script, are: the one-minute load average before the run, and the share of time CPUs 32 to 47 were busy in the 5 s
before the run (other work) and during it (other work plus the benchmark's own `K` threads).

| Round | `--contend` | Load before | CPUs 32-47 busy before | busy during |
|---|---:|---:|---:|---:|
| 1 | 1 | 33.6 | 65.0 % | 39.5 % |
| 1 | 8 | 22.4 | 29.4 % | 87.1 % |
| 2 | 1 | 35.1 | 68.7 % | 68.9 % |
| 2 | 8 | 43.9 | 32.4 % | 98.1 % |
| 3 | 1 | 13.8 | 13.9 % | 71.6 % |
| 3 | 8 | 23.2 | 62.9 % | 64.4 % |

Medians over the three rounds of each row's mean and p50, in ms per step (the per-round tables follow):

| Scenario | Sources | `--contend 1` mean | p50 | `--contend 8` mean | p50 |
|---|---:|---:|---:|---:|---:|
| `empty` | 0 | 2.33 | 2.14 | 3.79 | 3.16 |
| `showcase` | 10 | 7.09 | 7.23 | 9.08 | 7.97 |
| `crowd_48` | 48 | 4.95 | 4.52 | 5.19 | 4.84 |
| `battle_240` | 240 | 4.73 | 4.67 | 6.01 | 4.92 |
| `after_fight`, fight (0 to 30 s) | 240 | 4.67 | 4.50 | 5.76 | 5.51 |
| `after_fight`, after (30 to 60 s) | 0 | 5.45 | 5.66 | 7.92 | 6.67 |

What the baseline shows:

1. **An empty field is not free, and a field after a fight costs more than during it.** With no source at all,
   `after_fight`'s second half costs 5.66 ms at the median against 2.14 ms for `empty`, the same grid and window
   with no history: 3.5 ms more, for the same zero sources. This is the plan's §2.1 finding reproduced in isolation:
   velocity decays but never reaches exact zero, so the solver's sparse fast paths stop firing wherever a wake has
   passed. It matches the 3.4 to 3.7 ms excess of the 400-unit main update after the fight in
   `docs/evidence/2026-11-03-frame-budget.md`. WP9's quiescence is aimed at exactly this row.
2. **The showcase is the dearest scenario** (7.2 ms at the median): ten sources in the small Visual Lab volume cover
   many voxels each, and confinement runs. `crowd_48` skips confinement (more than 32 sources).
3. **Contention adds 0.3 to 2.1 ms at the median**, the most on the idle and after-fight fields, whose cost is memory
   traffic over the whole grid. The spread between rounds is large (round 3's `--contend 1` `battle_240` p95 of
   16.8 ms ran while other work started on the same CPUs), so the benchmark compares commits by medians over
   several rounds, never by one run.

Per-round tables, ms per step:

| Scenario | R1 c1 mean / p50 / p95 | R2 c1 | R3 c1 | R1 c8 | R2 c8 | R3 c8 |
|---|---|---|---|---|---|---|
| `empty` | 2.51 / 2.29 / 3.54 | 2.33 / 2.12 / 4.03 | 2.21 / 2.14 / 2.31 | 3.00 / 2.62 / 4.37 | 3.79 / 3.16 / 6.37 | 6.76 / 6.46 / 10.88 |
| `showcase` | 7.09 / 7.29 / 8.45 | 7.19 / 7.23 / 10.12 | 6.90 / 7.04 / 8.00 | 7.79 / 7.59 / 11.41 | 9.08 / 7.97 / 15.79 | 14.05 / 11.63 / 28.37 |
| `crowd_48` | 4.57 / 4.45 / 5.22 | 4.95 / 4.52 / 7.01 | 5.46 / 4.89 / 8.47 | 5.13 / 4.84 / 6.81 | 5.48 / 4.74 / 8.95 | 5.19 / 5.01 / 6.06 |
| `battle_240` | 4.73 / 4.67 / 5.40 | 4.60 / 4.43 / 6.14 | 8.27 / 6.34 / 16.76 | 6.01 / 5.42 / 9.54 | 6.04 / 4.92 / 13.08 | 4.97 / 4.80 / 5.53 |
| `after_fight`, fight | 4.67 / 4.50 / 5.55 | 4.57 / 4.42 / 5.57 | 6.13 / 4.99 / 12.88 | 5.76 / 5.51 / 7.48 | 9.98 / 9.37 / 16.61 | 4.81 / 4.62 / 5.56 |
| `after_fight`, after | 5.45 / 5.66 / 7.13 | 5.32 / 5.61 / 6.75 | 8.53 / 6.85 / 19.37 | 8.21 / 7.77 / 13.85 | 7.92 / 6.67 / 17.12 | 6.19 / 6.37 / 8.70 |

Logs: `~/scratch/logs/open_wp6_bench_c{1,8}_r{1,2,3}.log` (scratch; may be cleaned).

## 3. `perf` attribution on the frame-budget path

Arm A of the plan's D-6, the plan's base `e85f79f`: the release `frame_budget` built for WP3 in
`~/scratch/cargo-target/planbase` from a `git archive` export of `e85f79f`, run with `--spec` and `--source-assets`
pointing at a fresh export of the same commit (WP3's export had been removed; the report's commit therefore reads
"unavailable"). Scenario `400` (180 warm-up frames, 1,500 measured ticks), headless KWin 6.3.6 on the RX 580 (RADV
POLARIS10, Mesa 25.0.7), 1280×720, one `with-gpu-lock` per run with KWin and `perf record -F 997` inside it, exactly
D-6's command. The fixture root was prepared once on `lavapipe` without the lock.

| Run | Started (UTC) | Load before / after | Status | Frames | Interval p50 | p95 | p99 | Update p50 | Combat p50 | Combat p95 |
|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| 01 | 01:21 | 72.0 / 64.7 | disturbed (a machine-wide load spike; frame intervals doubled) | 1,565 | 34.4 | 69.7 | 103.0 | 30.4 | 31.5 | 62.8 |
| 02 | 01:44 | 26.4 / 14.0 | of record for attribution (loaded by D-6's threshold of 8, which the shared machine did not reach during the session) | 3,260 | 17.3 | 26.3 | 38.6 | 14.5 | 17.4 | 27.0 |

Both reports exist and neither log contains `frame_budget failed`. Run 02's frame figures are close to WP3's arm-A
`perf` run (`docs/evidence/2026-11-03-c17-combat-fx-pool.md`, run 07: p50 17.0, update p50 14.9 ms).

`perf report --no-children --sort sym -n`; milliseconds per frame are samples ÷ 997 ÷ rendered frames (measured
plus the 180 warm-up frames):

| Symbol | Run 02 samples | ms per frame | Run 01 samples | ms per frame |
|---|---:|---:|---:|---:|
| All samples | 182 K | | 85 K | |
| Rendered frames | 3,440 | | 1,745 | |
| `SmogGrid3d::step` (self time; the five stages are inlined into it) | 25,861 | **7.54** | 11,984 | 6.89 |
| `floorf` (all callers; the solver's trilinear `floor` is the hot one) | 1,535 | **0.45** | 713 | 0.41 |
| `__memmove_avx_unaligned_erms` (all callers, including the solver's eight `copy_from_slice` of 331,776 bytes each per step) | 6,455 | **1.88** | 3,116 | 1.79 |
| `SmogGrid3d::set_bounds_quantized` | 14 | 0.004 | 5 | 0.003 |

The runs recorded no call graph, so `memmove` is not split by caller; WP7, which removes the solver's copies, will
show the solver's share as the difference. `SmogGrid3d::step` alone takes 17 % of all samples of the run of record:
7.5 ms of CPU per rendered frame, above the 5.7 to 7.3 ms the planning build measured with timing lines (§4 below),
and the largest single symbol of the frame.

Reports, logs and `perf` data: `~/scratch/open_wp6_ab/a/` and `~/scratch/logs/open_wp6_perf_*.log` (scratch; may be
cleaned, so the tables here are the record).

## 4. Planning measurement (copied from the plan's §5.3, not a figure of record)

Taken while the plan was written, with a throwaway build of `e85f79f` that printed the duration of `step` and of
each stage, one locked run per scenario on the RX 580 under headless KWin. One run each, with printing in the frame.

| Units | `step` p50 | p95 | inject | confinement | advect velocity | advect density | pack | Voxels with nonzero velocity | with nonzero density |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 5.66 | 8.48 | 0.00 | 0.59 | 1.15 | 2.12 | 1.49 | 0 | 0 |
| 400 | 7.26 | 11.50 | 0.20 | 0.59 | 2.20 | 2.74 | 1.43 | 23,352 (28 %) | 1,027 (1.2 %) |

Times in ms; stage figures are means over the run. The 400 run averaged 29 smog sources (at most 74).

| Units | Load (1 min) | Frames | Frame interval p50 | p95 | Update p50 | `step` p50 | `step` p95 |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0 | 8.64 (after the build) | 5,121 | 11.70 | 15.01 | 10.44 | 5.66 | 8.48 |
| 400 | 3.85 | 3,794 | 15.53 | 19.78 | 13.25 | 7.26 | 11.50 |
