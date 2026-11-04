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

## 5. WP7: bit-exact refactors I (ping-pong slabs, truncating cast, dead slabs)

WP7 of the plan changes `crates/client/src/smog/fluid.rs` only, in the commit `perf(smog): C-18 WP7 bit-exact
refactors I` on top of `35ced86`:

1. **Ping-pong slabs.** `advect_velocity` and `advect_density_and_tint` exchange each field with its scratch slab
   (`std::mem::swap`) instead of copying the scratch slabs back: eight `copy_from_slice` of 331,776 bytes per step
   (2.65 MB read and written) are gone. The precondition, that each pass writes every voxel of every scratch slab
   it swaps, is pinned by the unit test `advection_passes_write_every_scratch_voxel`, which fills the scratch slabs
   with a NaN sentinel before each pass and requires none of it in the fields afterwards.
2. **Truncating cast.** `trilinear_indices_and_weights` casts its clamped operands with `as usize` instead of
   `floor() as usize`, so the `floorf` call (the `x86_64` baseline has no SSE4.1 `roundss`) is gone. The unit test
   `truncating_cast_matches_floor_on_clamped_operands` pins the equivalence for negative, `-0.0`, NaN, infinite,
   near-integer and out-of-range operands.
3. **Dead slabs.** `pressure` and `divergence` (allocated and cleared, never read) are removed: 0.66 MB less, two
   fewer `fill` per `clear`. The stale 49,152-voxel figures in the touched rationale blocks now read 82,944.

Each divergence carries a `PERFORMANCE RATIONALE` block (Why, What, Trade-off).

### Goldens

All 19 tests of `smog_golden_tests.rs` pass unchanged, among them the eleven fluid and system goldens (the three
earlier fluid goldens, the four of §1 and the four system goldens) and both `libm` canaries. No constant changed.

Mutation red runs (each applied to the WP7 file, the tests run, the file restored; logs
`~/scratch/logs/open_wp7_red_*.log`):

| Mutation | Fails |
|---|---|
| The plan's red run: `gx as usize` → `gx.round() as usize` | the eleven fluid and system goldens |
| A non-bit-exact reordering: `sample_indexed` sums its eight corners in reverse order | the eleven fluid and system goldens |
| The dormant fast path of `advect_density_and_tint` stops writing `tint_b_scratch` | `advection_passes_write_every_scratch_voxel` ("tint_b: 81387 voxels were not written by the pass"); no golden, which is the tint hole of §1 and why the unit test exists |

### Benchmark, before and after

Two release bench binaries built in the same target directory: arm A from `35ced86` (WP6's solver, equal to
`e85f79f`'s), arm B from the WP7 commit. Five rounds on 2026-11-04 from 10:18 to 10:31 UTC, each round running
`--contend 1` then `--contend 8`, and within each the two arms back to back, A first in odd rounds and B first in
even ones; every run pinned with `taskset -c 32-47`. The machine-wide one-minute load stood between 124 and 256
throughout (another user's work outside CPUs 32 to 47); the share of time CPUs 32 to 47 were busy is recorded per
run below, in the 5 s before it (other work) and during it (other work plus the benchmark's own `K` threads).

Medians over the five rounds of each row's mean and p50, ms per step:

| Scenario | Sources | `--contend 1` A mean / p50 | B mean / p50 | p50 change | `--contend 8` A mean / p50 | B mean / p50 | p50 change |
|---|---:|---:|---:|---:|---:|---:|---:|
| `empty` | 0 | 3.12 / 2.69 | 2.25 / 2.12 | −21 % | 4.02 / 3.94 | 2.92 / 2.51 | −36 % |
| `showcase` | 10 | 8.69 / 8.22 | 6.18 / 6.17 | −25 % | 7.83 / 7.84 | 6.91 / 6.51 | −17 % |
| `crowd_48` | 48 | 4.99 / 4.53 | 4.24 / 4.06 | −10 % | 5.12 / 4.91 | 4.51 / 4.18 | −15 % |
| `battle_240` | 240 | 4.97 / 4.62 | 4.26 / 4.01 | −13 % | 5.56 / 5.09 | 4.38 / 4.15 | −18 % |
| `after_fight`, fight (0 to 30 s) | 240 | 5.30 / 4.70 | 4.49 / 4.17 | −11 % | 5.54 / 5.08 | 4.64 / 4.12 | −19 % |
| `after_fight`, after (30 to 60 s) | 0 | 6.08 / 6.05 | 4.70 / 4.71 | −22 % | 7.05 / 6.88 | 5.31 / 5.15 | −25 % |

What it shows:

1. **Every scenario is faster in both columns**, by 10 to 25 % uncontended and 15 to 36 % contended, the range
   the plan's standalone probe measured for options A and B (§5.1 and §5.2 of the plan: 10 to 16 % alone, 13 to
   32 % contended). As there, the saving is largest under contention and on the fields whose cost is whole-grid
   memory traffic (`empty`, `after_fight` after).
2. **The after-fight excess remains.** `after_fight`'s second half still costs 2.6 ms (p50) more than `empty`
   uncontended: the copies are gone, but velocities that never reach exact zero still defeat the sparse fast
   paths. WP8 (bricks) and WP9 (quiescence) address it.
3. Arm A's medians here sit close to or above WP6's baseline of §2 (the machine was more loaded this time), which is
   why the comparison is read only between interleaved arms of the same session.

Per-run load:

| Round | `--contend` | Arm | Started (UTC) | Load (1 min) | CPUs 32-47 busy before | during |
|---|---:|---|---|---:|---:|---:|
| 1 | 1 | A | 10:18:39 | 220.5 | 8.5 % | 36.4 % |
| 1 | 1 | B | 10:19:15 | 221.7 | 79.4 % | 25.6 % |
| 1 | 8 | A | 10:19:47 | 255.5 | 9.2 % | 61.9 % |
| 1 | 8 | B | 10:20:37 | 229.6 | 9.3 % | 62.5 % |
| 2 | 1 | B | 10:21:10 | 224.0 | 7.7 % | 53.0 % |
| 2 | 1 | A | 10:21:44 | 250.0 | 83.4 % | 52.7 % |
| 2 | 8 | B | 10:22:30 | 241.3 | 60.5 % | 62.2 % |
| 2 | 8 | A | 10:23:04 | 230.4 | 8.0 % | 63.1 % |
| 3 | 1 | A | 10:23:44 | 202.7 | 9.3 % | 16.1 % |
| 3 | 1 | B | 10:24:29 | 186.4 | 9.4 % | 15.1 % |
| 3 | 8 | A | 10:25:02 | 198.6 | 6.6 % | 62.6 % |
| 3 | 8 | B | 10:25:44 | 201.1 | 58.1 % | 80.6 % |
| 4 | 1 | B | 10:26:31 | 213.6 | 54.7 % | 34.5 % |
| 4 | 1 | A | 10:27:02 | 147.6 | 20.3 % | 56.5 % |
| 4 | 8 | B | 10:27:39 | 124.3 | 50.0 % | 78.1 % |
| 4 | 8 | A | 10:28:26 | 143.1 | 25.6 % | 71.6 % |
| 5 | 1 | A | 10:29:05 | 170.2 | 16.2 % | 23.0 % |
| 5 | 1 | B | 10:29:45 | 175.3 | 7.4 % | 14.6 % |
| 5 | 8 | A | 10:30:18 | 174.9 | 7.1 % | 60.8 % |
| 5 | 8 | B | 10:31:03 | 189.8 | 15.4 % | 61.3 % |

Logs (per-round tables with p95): `~/scratch/logs/open_wp7_bench_{a,b}_c{1,8}_r{1..5}.log` and
`open_wp7_bench_index.log` (scratch; may be cleaned, so the tables here are the record).

## 6. WP8: bit-exact refactor II (brick sparsity)

WP8 of the plan changes `crates/client/src/smog/fluid.rs`, adds `crates/client/src/smog/bricks.rs`, makes the two
upload buffers private (`density_bytes()`, `velocity_bytes()`, and `#[doc(hidden)] retained_byte_buffers()` for
`sync_steady_state_tests.rs`), and updates their readers (`smog/mod.rs`, `smog/gpu_upload.rs`,
`smog_golden_tests.rs`). Commits `7fdbb54` (bricks), `370907f` (two test gaps closed) and `ab12883` (linear runs).

### What was built

The grid is cut into 8 × 4 × 8 bricks (12 × 3 × 9 = 324). Per brick the solver keeps the flags of the plan's D-2:
velocity at rest and scalars at rest (one set per slab, swapped with the slabs), occupancy (`!(density < 1e-6)` or
`!(glow < 1e-6)` possible; also one set per slab), vorticity at rest, the largest `|u|` per axis written by the
velocity advection, and the largest vorticity magnitude written by the curl. Per 8-voxel packing segment it keeps
whether the density alpha bytes are 0 and whether the velocity bytes are `(128, 128, 128, 0)`. The rules are the
plan's, pass by pass:

| Pass | Rule as built |
|---|---|
| Injection | After each source, every brick its voxel box touches loses its velocity and scalar rest flags and is marked occupied |
| Curl | A brick whose velocity, and its face neighbours' velocity, is at rest has a curl of `+0.0`: skipped if its vorticity flag is set, else its interior voxels are filled with `+0.0`. Otherwise computed; the flag is whether all written bits are 0 |
| Confinement force | A brick whose largest written magnitude is below `1e-4` (a NaN counts as `+inf`) is skipped; every other brick loses its velocity rest flag |
| Velocity advection | A brick whose velocity is at rest is skipped if its scratch flag is set, else filled with `+0.0`; otherwise computed, recording the largest `|u|` per axis |
| Density advection | A brick with no occupied brick within `ceil((max |u| + |buoyancy|) dt / cell) + 2` voxels per axis is skipped if its scratch flag is set, else filled with density 0, glow 0 and the tail tint; otherwise computed |
| Packing | Rows in linear order; a segment is skipped when its brick's scalars, velocity and vorticity are at rest and both of its byte flags are set |
| `new`, `clear`, `set_bounds`, `set_bounds_quantized` | Rescan every flag of the fields and the scratch slabs from the slabs and bytes |

The per-voxel code of a computed brick is the dense code, moved into per-voxel and per-row functions with the same
expressions. Each pass first decides per brick (skip, fill or compute). It then walks maximal runs of one decision
in the dense solver's linear voxel order. A second loop folds what each computed run wrote into its bricks' flags;
the "Benchmark" section below explains why.

### Tests

- **`bricks.rs` unit tests:** voxel-to-brick round trip for every voxel; face neighbours clipped at the grid's
  faces; the box query against a brute-force reference (boxes inside, partly outside, wholly outside, and empty);
  the reach formula; segments and runs in linear order.
- **`sparse_solver_matches_dense_solver_bit_for_bit`** (`fluid.rs`). A `cfg(test)` switch, `force_dense`, makes
  every skip query answer "compute", and makes packing walk the plain linear order. Two grids run a 600-step script,
  one sparse and one dense. After every step the test compares, bit for bit: all 20 slabs (fields and scratch slabs,
  tint included), both byte buffers, the metrics and the bounds. The script covers:
  - sources that appear, move at 11 world units per second, and stop;
  - 40 sources, then 13;
  - buoyancy +0.45, −0.45 and 0, and dissipation 0 and default;
  - `dt` cycling 1/240, 1/144, 1/60, 1/30, 0.07, 1/60, 0 and 1/90 s;
  - three quantized shifts (one of them along y), a full clear, a small `set_bounds`, and a shift wider than the
    grid.
- **`brick_flags_keep_their_promises_after_every_step`.** After every step of the same script, every flag that
  promises rest holds bit for bit, and every brick flagged unoccupied is empty. The test also requires each kind of
  flag to fire at least once.
- **`smog_resting_on_a_brick_face_matches_dense_solver`.** Smog rests on a brick's lower face, with zero velocity
  and buoyancy and power-of-two cells. A back-trace then lands exactly on a voxel centre, and its weight-zero upper
  corner is the smog in the next brick. This is the only case where the reach margin decides the result (see the
  red runs).
- **`advection_passes_write_every_scratch_voxel` (WP7).** It now rescans the flags after poisoning the scratch
  slabs, so the flags describe the slabs. Its precondition becomes "every scratch voxel holds this step's value".

### Goldens

All 19 tests of `smog_golden_tests.rs` pass unchanged at every WP8 commit, among them the eleven fluid and system
goldens and both `libm` canaries. No constant changed.

### Mutation red runs

Each mutation was applied to the final code. The `fluid.rs` unit tests and `smog_golden_tests.rs` were run, and the
file was restored (logs `~/scratch/logs/open_wp8_red_*.log`):

| Mutation | Unit tests that fail | Goldens that fail |
|---|---|---|
| The plan's: reach margin `+ 2` → `+ 0` | `smog_resting_on_a_brick_face_matches_dense_solver` ("step 0: tint_r[23358] differs: sparse 0.28, dense 0.58800006") | none |
| The plan's: injection sets the velocity flag to rest instead of clearing it | the flag invariant test ("step 0: brick 112, voxel (39, 3, 28): velocity scratch"), the differential test, `test_bubble_moving_through_existing_smog_influence` | 11 |
| The plan's: packing walks the grid in another order (y-major rows) | the differential test ("step 0: metrics") | 10 |
| Curl skips on the brick's own flag, ignoring its face neighbours | the differential test | 10 |
| The confinement force keeps the velocity flags of the bricks it changes | the flag invariant test, the differential test | 10 |
| A quantized shift does not rescan the flags | the flag invariant test (step 100, scalar scratch), the differential test ("step 101: tint_r[0] differs: sparse 0.6, dense 0.28") | none: the tint hole of §1 |

Two of the plan's expectations did not hold on the first implementation, and `370907f` strengthened the tests:

- **The reach mutation passed the scripted differential test and every golden**, including
  `fluid_fast_sources_golden`. The margin matters only where a back-trace lands exactly on a voxel centre next to a
  brick that holds smog, and no scripted step reached that case. The face test above builds it.
- **The packing mutation failed ten goldens but not the differential test**, because the dense reference shared
  the sparse walk. The reference now packs in plain linear order.

### Benchmark

Two release bench binaries were built in the same target directory (`~/scratch/cargo-target/open-wp8`): arm A from
`155aab7` (main, WP7's solver), arm B from this branch's final solver (the sources of `ab12883`). Five rounds ran
on 2026-11-04 from 14:15 to 14:26 UTC:

- Each round ran `--contend 1`, then `--contend 8`.
- Within each, the two arms ran back to back: A first in odd rounds, B first in even ones.
- Every run was pinned with `taskset -c 32-47`.
- The machine-wide one-minute load was 9.8 to 21.2.
- CPUs 32 to 47 were 0.8 to 76 % busy in the 5 s before a run, with other users' work. The table at the end of
  this section gives the figures per run.

Medians over the five rounds of each row's mean and p50, ms per step:

| Scenario | Sources | `--contend 1` A mean / p50 | B mean / p50 | p50 change | `--contend 8` A mean / p50 | B mean / p50 | p50 change |
|---|---:|---:|---:|---:|---:|---:|---:|
| `empty` | 0 | 2.17 / 2.12 | 0.13 / 0.12 | −94 % | 3.14 / 2.94 | 0.15 / 0.12 | −96 % |
| `showcase` | 10 | 6.01 / 6.13 | 7.03 / 7.21 | **+18 %** | 6.43 / 6.52 | 7.84 / 7.75 | **+19 %** |
| `crowd_48` | 48 | 4.20 / 4.01 | 4.36 / 4.33 | +8 % | 4.81 / 4.40 | 4.70 / 4.33 | −2 % |
| `battle_240` | 240 | 4.10 / 4.16 | 3.80 / 3.72 | −11 % | 4.47 / 4.39 | 4.01 / 3.87 | −12 % |
| `after_fight`, fight (0 to 30 s) | 240 | 4.04 / 3.97 | 3.72 / 3.71 | −7 % | 4.47 / 4.29 | 4.09 / 3.92 | −9 % |
| `after_fight`, after (30 to 60 s) | 0 | 4.26 / 4.44 | 3.74 / 3.17 | −29 % | 5.00 / 5.03 | 3.79 / 3.22 | −36 % |

What it shows:

1. **An idle field is now almost free.** It costs 0.12 ms against 2.1 to 2.9 ms: 18 to 25 times faster.
2. **After a fight the saving is partial** (−29 to −36 %). The velocity left by the wakes never reaches exact zero
   (plan §2.1), so most bricks of the battle window keep computing. The p95 of this row (5.9 ms) is far above its
   p50 (3.2 ms), because the first seconds after the fight still compute nearly everything. WP9's quiescence
   targets this row.
3. **A field that is active everywhere gains little, or loses.**
   - `battle_240` sweeps 240 sources over the whole window, so no brick rests: −11 %.
   - The Visual Lab `showcase` (ten large sources in a small volume, confinement on) is **18 % slower** than the
     dense solver. Nothing is skipped there, and the per-brick bookkeeping (decisions, folds, row re-flagging) costs
     about 1.1 ms.

**Stop rule (plan, WP8 step 5).**

- `empty` is 18 times faster; the rule asks for 5. Met.
- `battle_240` is 1.12 times faster; the rule asks for 1.3. **Not met**, so WP8 stops and reports before WP9.

The plan's §2.1 expected that, at 400 units, only bricks near wakes are visited. In this benchmark's `battle_240`
the wakes cover the window, and because velocities never return to zero they keep covering it. Q0048 asks the owner
how to go on.

**How the walk was chosen.** The first implementation (`7fdbb54`) walked the grid brick by brick. Its three-round
A/B used the same method (12:33 to 12:38 UTC, logs `~/scratch/logs/open_wp8_bench_v1/`) and measured `empty` −96 %,
`battle_240` −6 % and `showcase` +35 % (+53 % contended). `perf` showed two causes:

- the curl loop, the one loop LLVM vectorizes (`sqrtps`), had become scalar;
- a brick walk touches about 80 pages per brick across the slabs.

Walking runs of one decision in linear order (`ab12883`) restored the vectorized curl and the dense memory order,
and brought `showcase` to +18 %. An instrumented build (per-pass timers, not committed) put the remaining
`showcase` excess in three passes: confinement (about +28 %), packing (about +44 %) and velocity advection (about
+8 %).

Per-run load:

| Round | `--contend` | Arm | Started (UTC) | Load (1 min) | CPUs 32-47 busy before | during |
|---|---:|---|---|---:|---:|---:|
| 1 | 1 | A | 14:15:34 | 19.6 | 30.7 % | 34.8 % |
| 1 | 1 | B | 14:16:05 | 15.0 | 23.6 % | 34.4 % |
| 1 | 8 | A | 14:16:32 | 11.3 | 14.6 % | 72.2 % |
| 1 | 8 | B | 14:17:03 | 12.3 | 13.3 % | 84.2 % |
| 2 | 1 | B | 14:17:34 | 18.0 | 41.4 % | 37.1 % |
| 2 | 1 | A | 14:18:02 | 13.4 | 32.1 % | 31.1 % |
| 2 | 8 | B | 14:18:31 | 9.8 | 14.6 % | 64.6 % |
| 2 | 8 | A | 14:19:00 | 11.0 | 0.8 % | 56.3 % |
| 3 | 1 | A | 14:19:34 | 11.9 | 14.7 % | 62.8 % |
| 3 | 1 | B | 14:20:09 | 21.2 | 17.9 % | 25.3 % |
| 3 | 8 | A | 14:20:36 | 15.4 | 31.4 % | 86.5 % |
| 3 | 8 | B | 14:21:11 | 19.8 | 41.9 % | 78.1 % |
| 4 | 1 | B | 14:21:44 | 21.0 | 27.5 % | 18.5 % |
| 4 | 1 | A | 14:22:13 | 16.9 | 11.8 % | 24.2 % |
| 4 | 8 | B | 14:22:43 | 15.0 | 12.3 % | 58.6 % |
| 4 | 8 | A | 14:23:12 | 15.0 | 2.0 % | 72.2 % |
| 5 | 1 | A | 14:23:48 | 17.9 | 41.9 % | 33.0 % |
| 5 | 1 | B | 14:24:18 | 17.9 | 33.5 % | 48.0 % |
| 5 | 8 | A | 14:24:47 | 17.1 | 62.3 % | 93.5 % |
| 5 | 8 | B | 14:25:24 | 19.0 | 75.8 % | 99.9 % |

Round 5's contended runs overlapped other work on CPUs 32 to 47: B's `showcase` and `after_fight` rows took 2 to 3
times as long as in its other rounds. The medians keep those runs out of the result.

Logs: `~/scratch/logs/open_wp8_bench_{a,b}_c{1,8}_r{1..5}.log` and `open_wp8_bench_index.log`; the first
implementation's are in `open_wp8_bench_v1/`. They are on scratch and may be cleaned, so the tables here are the
record.

## 7. WP9: quiescence, re-recorded goldens and the A/B measurement

WP9 of the plan changes `crates/client/src/smog/fluid.rs` and `crates/client/tests/smog_golden_tests.rs` in one
commit, `3f3ab76` (`perf(smog): C-18 WP9 quiescence; smog goldens re-recorded`), on top of `main` at `4bd48cf`.
The owner answered Q0020 with option 1, so the plan's step 1 does not skip anything.

### The change (plan D-3)

After `advect_velocity` computes a voxel's three new components, a voxel with
`ux * ux + uy * uy + uz * uz <= SMOG_QUIESCENT_SPEED_SQ` gets exactly `+0.0` in all three. The constant is `1e-6`, a
speed of at most `1e-3` world units per second. It also replaces the `1e-6` speed literal in
`pack_textures_and_compute_metrics`, so the two uses cannot drift apart. The vorticity and glow literals of that test
are unchanged. Both arms of WP8's differential test call the same voxel function, so quiescence applies to the
sparse and the dense solver alike.

### Tests

- **New: `velocity_returns_to_exact_rest_after_sources_stop`** (`fluid.rs`). The showcase sources run for 60 steps
  at 1/60 s, then no source for 1,800 steps (30 s). The test asserts that every velocity component of every voxel is
  exactly `+0.0`, and that every brick's velocity, velocity scratch and vorticity flags are at rest.
  - Red at WP8 (`main` at `5d307ca`): 84,723 velocity components are still nonzero.
  - Red with a threshold of `1e-4` world units per second (`SMOG_QUIESCENT_SPEED_SQ = 1e-8`) instead of `1e-3`:
    15 nonzero components remain after 30 s.
  - Green with `1e-3`.
- **Unchanged and passing:** the 25 semantic tests of `smog_bouncing_bubbles_tests.rs`, the eight earlier unit
  tests of `fluid.rs` (among them `sparse_solver_matches_dense_solver_bit_for_bit`,
  `brick_flags_keep_their_promises_after_every_step` and `smog_resting_on_a_brick_face_matches_dense_solver`), and
  the 21 other smog unit tests (bricks, kinematics, material). The plan's stop rule (a semantic test that fails
  would mean the change is visible) did not fire.

### Goldens

The commit re-records eleven constants: the seven fluid and system goldens named by D-3, and all four of WP6's
characterisation goldens, which change too. Each of the eleven failed with the new code before the re-record
(log `open_wp9_golden_new_code.log`). Under the `1e-8` threshold all eleven hash to a third value, different from
both columns below (log `open_wp9_red_threshold.log`). Recorded on glibc 2.41, `x86_64`; every golden stays class
L, because the change adds no `libm` call.

| Constant | Old | New |
|---|---|---|
| `GOLDEN_FLUID_SHOWCASE` | `0xb42e_8fd5_0690_ef08` | `0x2b35_6d55_7c4a_1810` |
| `GOLDEN_FLUID_MIXED_24` | `0x2e2d_8269_5e27_898d` | `0x743d_d564_7e66_28b6` |
| `GOLDEN_FLUID_CROWD_48` | `0x6ab3_c16e_837d_e74b` | `0xe040_edb2_fa5b_187e` |
| `GOLDEN_FLUID_WAKE_DECAY` | `0x6f8f_62ab_607f_d719` | `0x2ff5_8b66_774b_7a2b` |
| `GOLDEN_FLUID_FAST_SOURCES` | `0x7ffa_1917_b16b_ea08` | `0x5fb4_f8c6_25fd_54e2` |
| `GOLDEN_FLUID_BOUNDS_SHIFT` | `0xb677_0ad8_9bf1_a12a` | `0xea2a_91d9_b0cc_39c0` |
| `GOLDEN_FLUID_VARIABLE_DT` | `0xf4fc_5e58_2628_330d` | `0x55ae_4990_ed7a_4886` |
| `GOLDEN_SYSTEMS_STARTUP` | `0x5690_15a8_6873_e107` | `0xdc52_4ec7_f496_22d1` |
| `GOLDEN_SYSTEMS_VISUAL_LAB` | `0xe585_490b_a96b_dced` | `0x4174_0f4e_bdad_29c9` |
| `GOLDEN_SYSTEMS_SWARM` | `0x8d70_5277_8a52_19e3` | `0xecc6_5d0a_0949_f693` |
| `GOLDEN_SYSTEMS_LIVE_BATTLE` | `0x5e7e_6426_b5ee_e540` | `0x5527_286e_5980_202b` |

The old values of the four system goldens are those of `main` at `4bd48cf`. Animation WP1 (a renamed field in the
hashed `Debug` text) and animation WP4 (a 176-byte bubble uniform) re-recorded them there, with no change to the
smog output. WP9 was rebased onto both before its own re-record, so quiescence is the only cause of every change
in the table.

Checked unchanged: the four kinematics goldens, `GOLDEN_SYSTEMS_NOISE`, `GOLDEN_LIBM_CANARY` and
`GOLDEN_LIBM_CANARY_C18`.

### Benchmark at the final commit

Three release bench binaries were built in one target directory (`~/scratch/cargo-target/open-wp9-rel`):

- WP7 from `155aab7` (the dense solver);
- WP8 from `5d307ca` (bricks);
- WP9 from the WP9 commit (bricks and quiescence; its solver is that of `3f3ab76`).

Five rounds ran on 2026-11-04, with the same method as §5 and §6. Each round ran `--contend 1`, then
`--contend 8`. Within each, the three arms ran back to back in a rotating order. Every run was pinned with
`taskset -c 32-47`. Rounds 1 to 3 ran from 17:17 to 17:26 UTC and rounds 4 and 5 from 17:50 to 17:59 UTC, after the
frame-budget runs.

Medians over the five rounds of each row's mean, p50 and p95, ms per step, `--contend 1`:

| Scenario | Sources | WP7 mean / p50 / p95 | WP8 | WP9 | WP9 vs WP8 (p50) | WP9 vs WP7 (p50) |
|---|---:|---|---|---|---:|---:|
| `empty` | 0 | 2.31 / 2.22 / 3.53 | 0.25 / 0.12 / 0.20 | 0.14 / 0.12 / 0.15 | +3 % | −94 % |
| `showcase` | 10 | 6.80 / 6.26 / 10.90 | 7.42 / 7.26 / 10.26 | 6.48 / 6.03 / 9.27 | −17 % | −4 % |
| `crowd_48` | 48 | 4.37 / 4.16 / 8.01 | 4.38 / 4.35 / 4.78 | 5.00 / 4.33 / 8.61 | −1 % | +4 % |
| `battle_240` | 240 | 4.71 / 4.23 / 8.26 | 3.93 / 3.74 / 4.98 | 3.76 / 3.77 / 4.01 | +1 % | −11 % |
| `after_fight`, fight (0 to 30 s) | 240 | 4.37 / 4.17 / 7.17 | 3.92 / 3.73 / 5.07 | 3.90 / 3.79 / 4.01 | +2 % | −9 % |
| `after_fight`, after (30 to 60 s) | 0 | 4.82 / 4.49 / 8.79 | 4.10 / 3.34 / 6.95 | 1.38 / 0.14 / 4.07 | −96 % | −97 % |

Other users' work kept CPUs 32 to 47 between 97 and 100 % busy before every run of rounds 4 and 5. The contended
rows are therefore read from rounds 1 to 3 only (medians of three, `--contend 8`):

| Scenario | WP7 mean / p50 / p95 | WP8 | WP9 | WP9 vs WP8 (p50) | WP9 vs WP7 (p50) |
|---|---|---|---|---:|---:|
| `empty` | 3.58 / 3.05 / 10.00 | 0.24 / 0.12 / 0.19 | 0.22 / 0.15 / 0.21 | +28 % | −95 % |
| `showcase` | 6.21 / 6.35 / 6.92 | 7.70 / 7.57 / 11.55 | 7.14 / 6.37 / 13.57 | −16 % | +0 % |
| `crowd_48` | 4.26 / 4.09 / 4.55 | 4.80 / 4.33 / 7.06 | 4.50 / 4.38 / 5.09 | +1 % | +7 % |
| `battle_240` | 4.26 / 4.16 / 4.53 | 3.81 / 3.81 / 4.02 | 3.98 / 3.94 / 4.43 | +4 % | −5 % |
| `after_fight`, fight | 4.29 / 4.08 / 5.14 | 4.12 / 3.77 / 6.36 | 4.74 / 3.97 / 8.18 | +5 % | −3 % |
| `after_fight`, after | 4.45 / 4.60 / 5.74 | 4.79 / 3.42 / 7.52 | 1.47 / 0.24 / 4.36 | −93 % | −95 % |

What it shows:

1. **The after-fight field is now nearly as cheap as an empty one.** At the median it costs 0.14 ms against 3.34 ms
   at WP8 and 4.49 ms at WP7 (−96 and −97 %). The mean (1.38 ms) and the p95 (4.07 ms) carry the first 19 s or
   so after the fight, while the wake still decays above `1e-3` world units per second. This is the row WP9 was
   for.
2. **The empty field and the fights are unchanged against WP8**, within ±5 % at the median. Quiescence does not
   help a field that sources keep stirring.
3. **The Visual Lab showcase regression of WP8 is gone at the median.** WP8 is again 16 to 17 % slower than the
   dense solver here, reproducing §6's +18 %. WP9 brings `showcase` back to the dense solver's cost: −4 %
   uncontended and +0 % contended at p50, and a lower p95 uncontended. The likely cause is that the fringes of the
   showcase volume now come to rest, so their bricks are skipped and pay back the bookkeeping. **No follow-up
   finding is recorded.** Q0048's condition, "if the showcase regression still matters afterwards", is not met.

Per-run load: the one-minute load average, and the share of time CPUs 32 to 47 were busy in the 5 s before the run
and during it. The second figure includes the benchmark's own threads.

| Round | `--contend` | Arm | Started (UTC) | Load (1 min) | CPUs 32-47 busy before | during |
|---|---:|---|---|---:|---:|---:|
| 1 | 1 | WP7 | 17:17:05 | 11.15 | 59.0 % | 87.7 % |
| 1 | 1 | WP8 | 17:17:36 | 13.54 | 98.9 % | 93.3 % |
| 1 | 1 | WP9 | 17:18:05 | 14.62 | 100.0 % | 93.3 % |
| 1 | 8 | WP7 | 17:18:28 | 15.02 | 87.0 % | 99.1 % |
| 1 | 8 | WP8 | 17:19:03 | 16.81 | 61.5 % | 93.8 % |
| 1 | 8 | WP9 | 17:19:32 | 16.60 | 64.3 % | 99.6 % |
| 2 | 1 | WP9 | 17:19:59 | 16.51 | 37.6 % | 21.2 % |
| 2 | 1 | WP8 | 17:20:22 | 12.34 | 58.1 % | 66.2 % |
| 2 | 1 | WP7 | 17:20:49 | 12.02 | 62.6 % | 67.4 % |
| 2 | 8 | WP9 | 17:21:21 | 13.61 | 61.1 % | 88.4 % |
| 2 | 8 | WP8 | 17:21:48 | 16.68 | 32.7 % | 78.7 % |
| 2 | 8 | WP7 | 17:22:19 | 17.68 | 54.3 % | 96.3 % |
| 3 | 1 | WP8 | 17:22:54 | 18.78 | 17.6 % | 18.7 % |
| 3 | 1 | WP9 | 17:23:24 | 18.51 | 11.3 % | 33.6 % |
| 3 | 1 | WP7 | 17:23:51 | 16.81 | 61.4 % | 56.4 % |
| 3 | 8 | WP8 | 17:24:26 | 18.66 | 19.5 % | 92.5 % |
| 3 | 8 | WP9 | 17:25:02 | 24.31 | 40.5 % | 80.7 % |
| 3 | 8 | WP7 | 17:25:31 | 21.50 | 65.0 % | 81.9 % |
| 4 | 1 | WP7 | 17:50:03 | 22.42 | 99.5 % | 96.1 % |
| 4 | 1 | WP9 | 17:50:51 | 25.11 | 80.8 % | 65.3 % |
| 4 | 1 | WP8 | 17:51:16 | 22.50 | 66.7 % | 63.5 % |
| 4 | 8 | WP7 | 17:51:45 | 20.08 | 60.4 % | 99.4 % |
| 4 | 8 | WP9 | 17:52:34 | 25.06 | 99.0 % | 100.0 % |
| 4 | 8 | WP8 | 17:53:24 | 33.00 | 99.7 % | 99.7 % |
| 5 | 1 | WP9 | 17:54:15 | 31.02 | 97.5 % | 98.5 % |
| 5 | 1 | WP7 | 17:54:42 | 28.58 | 97.9 % | 99.2 % |
| 5 | 1 | WP8 | 17:55:28 | 42.34 | 98.9 % | 99.6 % |
| 5 | 8 | WP9 | 17:56:14 | 51.37 | 99.3 % | 99.9 % |
| 5 | 8 | WP7 | 17:57:08 | 60.01 | 99.8 % | 99.9 % |
| 5 | 8 | WP8 | 17:58:33 | 52.56 | 99.7 % | 97.3 % |

Logs: `~/scratch/logs/open_wp9_bench_w{7,8,9}_c{1,8}_r{1..5}.log` and `open_wp9_bench_index.log` (scratch; may be
cleaned, so the tables here are the record).

### Frame-budget A/B (plan D-6), the measure of record for the C-18 chain

The two arms:

- **Arm A** is the plan's base, `e85f79f`. It is the release `frame_budget` of `~/scratch/cargo-target/planbase`,
  as in §3, with `--spec` and `--source-assets` pointing at the `git archive` export `~/scratch/open_wp6_base`.
- **Arm B** is the WP9 commit on top of `main` at `9d5884c`: WP3's pooled combat effects (C-17) and WP7 to WP9 of
  C-18. It was built from a `git archive` export as `d9f2044`. Its solver (`fluid.rs`, `bricks.rs`) is that of
  `3f3ab76`, apart from a rewording of the new `PERFORMANCE RATIONALE` block. After the runs, `main` gained combat
  WP4 (`be5a476`, a weapon rebalance in `sim_core`) and animation WP4 (`4bd48cf`, the GPU flipbook clock and a
  176-byte bubble uniform). The solver did not change, but the final commit's battle and bubble rendering differ
  slightly from arm B's, so arm B measures C-17 and C-18 on `9d5884c`.

Each arm's fixture root was prepared once on `lavapipe` without the lock. Then 16 runs, each one `with-gpu-lock`
holding KWin and the tool, exactly D-6's command:

- headless KWin 6.3.6 (`--virtual`, 1280×720) on the RX 580 (RADV POLARIS10, Mesa 25.0.7);
- `taskset -c 32-47`, `--display-path kwin-virtual`;
- about 10 s between runs.

The first 12 runs interleave A and B, 0 and 400 units, three pairs each. Then one `perf record -F 997` run per arm
at 400 units, as D-6 asks, and one per arm at 0 units, which the plan does not ask for, to give the solver's cost
at 0 units. Every report exists and no log contains `frame_budget failed`. The lock never waited more than 10 s.

The shared machine never fell below D-6's load threshold of 8 during the session, so every run is "loaded" by
that rule, and repeating loaded runs would not have produced unloaded ones. The per-run table gives the load. Run
10 (B, 0 units) overlapped a machine-wide load spike, to 118 at its end: its p95 is 161 ms and it rendered half the
frames of the other B runs at 0 units. It is kept, as the protocol says, and it is the reason B's p95 and p99 at 0
units are no better than A's.

Medians over the three interleaved runs of each arm and scenario, in ms. The combat window is the report's: 750
ticks from the first combat event. The after-combat window covers the frames past it, up to the end of the
1,500-tick measurement; it was computed from the report's per-frame records with the tool's nearest-rank
percentiles.

| Arm | Units | Frame interval p50 | p95 | p99 | Main update p50 | Combat p50 | Combat p95 | Combat update p50 | After-combat p50 | After-combat p95 | After-combat update p50 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| A (`e85f79f`) | 0 | 13.38 | 25.93 | 33.75 | 11.47 | | | | | | |
| B (WP9) | 0 | 12.60 | 26.49 | 35.72 | 9.36 | | | | | | |
| A (`e85f79f`) | 400 | 16.75 | 36.30 | 53.57 | 14.97 | 21.20 | 39.58 | 15.64 | 17.77 | 37.57 | 15.48 |
| B (WP9) | 400 | **12.03** | **20.92** | **27.24** | **9.82** | 12.09 | 23.76 | 10.04 | 12.01 | 15.82 | 10.17 |

At 400 units, B's frame interval is 28 % shorter at the median and 42 % shorter at p95. Its main update is 34 %
shorter. After the fight, the base stayed close to its combat cost (p50 17.77 against 21.20 ms, p95 37.57
against 39.58 ms). B's after-combat window costs no more than its fight (p50 12.01, p95 15.82 ms). At 0 units, B's main update is 18 % shorter at the median. The cleaner pair 05/06 shows the size of
the change better: 5.08 against 10.24 ms. The frame-interval medians at 0 units are mixed, because run 10 was
disturbed.

Per run: the one-minute load before the run, and the share of time CPUs 32 to 47 were busy in the 5 s before it.
The lock wait is the time from the request to the lock being held.

| Run | Arm | Units | `perf` | Started (UTC) | Load | CPUs 32-47 busy before | Lock wait (s) | Rendered frames | p50 | p95 | p99 | Update p50 | Combat p50 / p95 / update p50 | After-combat p50 / p95 / update p50 |
|---|---|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| 01 | A | 0 | no | 17:26:10 | 16.12 | 41.1 % | 8.2 | 3,645 | 15.44 | 25.93 | 33.75 | 13.05 | | |
| 02 | B | 0 | no | 17:27:50 | 32.27 | 42.5 % | 5.5 | 4,212 | 12.60 | 26.49 | 35.72 | 9.36 | | |
| 03 | A | 400 | no | 17:29:29 | 35.10 | 59.2 % | 0.0 | 3,098 | 16.75 | 36.30 | 53.57 | 14.97 | 22.80 / 47.22 / 19.63 | 15.11 / 21.68 / 13.51 |
| 04 | B | 400 | no | 17:30:59 | 28.67 | 23.7 % | 0.0 | 3,631 | 14.37 | 29.14 | 43.84 | 11.67 | 18.57 / 38.10 / 14.13 | 12.57 / 20.70 / 10.49 |
| 05 | B | 0 | no | 17:32:23 | 20.02 | 8.9 % | 0.0 | 6,799 | 7.80 | 15.27 | 27.91 | 5.08 | | |
| 06 | A | 0 | no | 17:33:44 | 16.73 | 16.4 % | 10.0 | 4,964 | 11.82 | 16.51 | 20.42 | 10.24 | | |
| 07 | B | 400 | no | 17:35:17 | 12.42 | 18.6 % | 0.0 | 4,826 | 12.03 | 16.35 | 21.77 | 9.82 | 12.03 / 16.62 / 9.34 | 12.01 / 15.82 / 10.17 |
| 08 | A | 400 | no | 17:36:39 | 11.60 | 20.3 % | 0.0 | 3,200 | 16.66 | 34.73 | 43.31 | 13.83 | 16.01 / 23.89 / 13.06 | 17.77 / 37.57 / 15.48 |
| 09 | A | 0 | no | 17:38:03 | 12.84 | 17.7 % | 0.0 | 3,629 | 13.38 | 33.95 | 51.31 | 11.47 | | |
| 10 | B | 0 | no | 17:39:26 | 38.56 | 56.9 % | 0.0 | 1,765 | 15.68 | 160.75 | 334.22 | 11.69 | | |
| 11 | A | 400 | no | 17:41:04 | 98.88 | 58.9 % | 0.0 | 2,594 | 20.51 | 41.21 | 59.89 | 16.45 | 21.20 / 39.58 / 15.64 | 19.82 / 45.17 / 17.32 |
| 12 | B | 400 | no | 17:42:27 | 35.41 | 14.2 % | 0.0 | 5,050 | 10.66 | 20.92 | 27.24 | 9.06 | 12.09 / 23.76 / 10.04 | 9.92 / 13.22 / 8.50 |
| 13 | A | 400 | yes | 17:43:49 | 18.26 | 9.5 % | 0.0 | 3,575 | 16.32 | 21.64 | 24.91 | 14.19 | 17.27 / 22.74 / 14.28 | 15.77 / 19.67 / 14.13 |
| 14 | B | 400 | yes | 17:45:13 | 10.03 | 13.9 % | 0.0 | 4,244 | 13.03 | 21.09 | 28.33 | 10.84 | 15.50 / 23.88 / 12.08 | 12.14 / 15.07 / 10.37 |
| 15 | A | 0 | yes | 17:47:11 | 15.55 | 84.7 % | 0.0 | 3,145 | 16.08 | 35.46 | 57.15 | 13.86 | | |
| 16 | B | 0 | yes | 17:48:40 | 21.63 | 55.5 % | 0.0 | 5,940 | 8.30 | 24.02 | 36.04 | 5.89 | | |

The `perf` runs' own frame figures are not used for the comparison, because `perf` slows a run.

### `SmogGrid3d::step` per frame (`perf`)

`perf report --no-children --sort sym -n`. Milliseconds per frame are samples ÷ 997 ÷ rendered frames (measured
plus the 180 warm-up frames). The solver figure adds the `bricks.rs` symbols that were not inlined into `step`
(`Runs::next`, `BrickFlags::any_in`, `curl_at_rest`, `scalar_sources_empty`) to `step`'s self time.

| Run | Arm | Units | Rendered frames | Solver samples | **Solver ms per frame** | `memmove` ms per frame | `floorf` ms per frame |
|---|---|---:|---:|---:|---:|---:|---:|
| 15 | A | 0 | 3,325 | 16,575 | **5.00** | 1.60 | 0.00 |
| 16 | B | 0 | 6,120 | 1,955 | **0.32** | 0.79 | 0.00 |
| 13 | A | 400 | 3,755 | 29,190 | **7.80** | 1.87 | 0.43 |
| 14 | B | 400 | 4,424 | 26,863 | **6.07** | 0.97 | 0.00 |

Run 13 reproduces §3's 7.54 ms at the base. At 0 units the solver now costs a sixteenth of the base. At 400 units
it costs 22 % less per frame. The fight keeps sources moving across the whole battle window, so few bricks rest,
which matches `battle_240` in the benchmark.

### D-1's trigger: fired

At 400 units on the frame-budget path, the solver still costs **6.1 ms per frame**, above D-1's threshold of
2.0 ms. By the plan's rule this is a remaining part of C-18. Options F (a lower fixed smog rate with an accumulator)
and G (stepping on a worker thread) are reopened for a later plan, which chooses between them. This plan implements
neither.

- **WP9 records it here.** D-1 also asks WP9 to record it in the remediation plan, but D-7 gives that file's C-18
  row to WP11, and WP9 touches only its own files. So WP11 is to mark C-18 **Partly fixed**: the idle and
  after-fight costs are fixed, and the 400-unit fight cost remains, at 6.1 ms per frame against 7.8 at the base.
- **The figures.** At 0 units: 5.0 → 0.3 ms per frame. At 400 units: 7.8 → 6.1 ms per frame. Frame interval at
  400 units: p50 16.75 → 12.03 ms and p95 36.30 → 20.92 ms (medians of three interleaved runs).

Reports, logs and `perf` data: `~/scratch/open_wp9_ab/{a,b}/` and `~/scratch/logs/open_wp9_fb_*.log`, with the
index `open_wp9_fb_index.log` (scratch; may be cleaned, so the tables here are the record).

## 8. Status of C-18

| Step | What | Output | Commit |
|---|---|---|---|
| WP6 | Characterisation goldens, benchmark, attribution | Unchanged | `35ced86` (merge) |
| WP7 | Ping-pong slabs, truncating cast, dead slabs | Bit-exact | `155aab7` (merge) |
| WP8 | Brick sparsity | Bit-exact | `e935e0a` (merge) |
| WP9 | Quiescence at `1e-3` world units per second | Deliberate (D-3, Q0020); 11 goldens re-recorded | `3f3ab76` |

An idle field and a field after a fight now cost almost nothing. A 400-unit fight still costs about 6 ms of solver
per frame, which D-1 leaves to a later plan (options F or G).
