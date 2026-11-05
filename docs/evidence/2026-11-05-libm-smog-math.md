# Evidence: host-independent smog math through the `libm` crate (Q0014 option b)

Plan: [`docs/plans/2026-11-04-libm-smog-math.md`](../plans/2026-11-04-libm-smog-math.md), WP1 to WP3. Owner question
Q0014, option (b), approved "unless this becomes a performance problem". Base: `main` at `60c6628` (arm A). Change:
WP2 commit `a8cbfaa` (arm B). Host: the shared development VM (AMD EPYC 7301 under KVM, Linux 6.12.115, glibc 2.41,
`x86_64`), every measurement pinned to CPUs 32 to 47, with another agent's builds and tests running on the same
machine throughout.

**Result.** Every smog golden is now host-independent, proved by an interposer that moves every host `libm` result by
one ulp. The stop rule of plan D-6 did not fire: the smog startup system costs about 12 ms more (limit 50 ms), and the
400-unit frame budget shows no difference beyond the noise of the shared machine (§5.4): in the protocol's three
pairs arm B's main-update p50 is 0.81 ms higher at the median but well inside arm A's own spread of 2.39 ms, its
frame-interval p95 is lower, three repeat pairs split both ways, and `perf` puts the `libm` crate at 0.02 % of
the samples, about 0.01 ms per frame. The owner's condition ("unless this becomes a performance problem") is met.

## 1. Start conditions (WP1 steps 1 and 2)

- Open-findings WP9 is merged on `main` (`3f3ab76`, `ffa0ea8`; the plan index lists the open-findings plan as Done).
- `Cargo.lock` holds `libm 0.2.16` (checksum `b6d2cec3…`) and its sources are in the local registry
  (`libm-0.2.16`). WP2 added the direct dependency; `git diff Cargo.lock` shows one added line, the `"libm"` edge in
  `client`'s dependency list. Nothing was downloaded.

## 2. Audit: host `libm` calls on hashed smog paths (WP1 step 3)

Re-run on `60c6628` with the method list of plan D-4 over `crates/client/src/smog/`, and confirmed by check C23's red
run (40 findings, all below) and by the interposer runs of §3.

| Site | Function | Reaches | Routed to |
|---|---|---|---|
| `fluid.rs`, `advect_velocity` | `expf` | every fluid and system golden | `math::exp` |
| `fluid.rs`, `advect_density_and_tint` (`decay`, `glow_decay`) | `expf` ×2 | every fluid and system golden | `math::exp` |
| `kinematics.rs`, `bubble_team_color` beyond the palette | `sinf` ×3 | fluid goldens with more than 10 sources; preview, swarm and battle system goldens | `math::sin` |
| `kinematics.rs`, `generate_bouncing_bubbles`, crowd branch | `sinf` ×3, `cosf` | crowd fluid and kinematics goldens (recorded trig table before) | `math::sin`, `math::cos` |
| `kinematics.rs`, `generate_bouncing_bubbles` (both branches) and `sync_headings` | `atan2f` | kinematics and system goldens (headings, unhashed before) | `math::atan2` |
| `mod.rs`, `smog_noise_texel` | `sinf`, `cosf` (19 per texel), `powf` (5 per texel) | noise golden | `math::{sin, cos, powf}` |
| `mod.rs`, `showcase_style`: `Color::srgb(..).to_linear()` | `powf` (Bevy `ops::powf`) | startup and system goldens | `math::srgb_to_linear` |
| `mod.rs`, `sync_bouncing_smog_bubbles_system`: `Quat::from_rotation_y(-heading)` | `sinf`, `cosf` (glam `sin_cos`) | Visual Lab system golden (rotations, unhashed when stepped before) | `math::quat_from_rotation_y` |
| `mod.rs`, `sync_bouncing_smog_bubbles_system`: `transform.rotation.to_euler(EulerRot::YXZ).0` | `atan2f`, `asinf` (glam `to_euler`) | the heading of a bubble slower than `1e-4` world units per second (no golden: every showcase bubble moves) | **not in the plan's audit**; replaced by the render instance's heading (§6) |

No folded call was found: every site has a non-constant argument. `sqrt`, `floor`, `round`, `abs`, `clamp` and `powi`
stay as they are (exactly rounded or not a `libm` call). Outside `smog/`, `player_presentation/mod.rs` calls
`Color::to_linear` for the palette's player colours; the interposer runs show it reaches no smog hash, so it stays on
the host's library (plan D-2 keeps the rest of the client there).

## 3. Proof of host independence: the perturbing interposer (WP1 step 4, WP2 step 5)

The interposer is the appendix of the plan, built with `gcc -O2 -fno-builtin -shared -fPIC -o perturb.so perturb.c
-ldl -lm` in `~/scratch/plananswers-libm/`. **Deviation:** without `-fno-builtin`, GCC merged the `sinf` and `cosf`
calls inside the interposer's own `sincosf` into a call to `sincosf`, which recursed until the stack overflowed (the
first baseline run aborted in `smog_live_battle_systems_golden`); `objdump -d` of the rebuilt object shows `sincosf`
calling `sinf@plt` and `cosf@plt`. Each run is the debug `smog_golden_tests` binary with and without
`LD_PRELOAD=perturb.so`.

| Binary | Without the interposer | With the interposer |
|---|---|---|
| `main` `60c6628` (two classes) | 19 of 19 pass | 6 pass, 13 fail: the seven fluid goldens, the noise, startup and three system goldens, and `smog_golden_libm_canary` (every class-L golden). The four class-H kinematics goldens, `crowd_generator_matches_lattice` and `crowd_trig_table_matches_host` pass |
| WP2 `a8cbfaa` | 17 of 17 pass | **17 of 17 pass** |
| WP2 with the mutation of WP2 step 6 (`bubble_team_color`'s first channel back on `f32::sin`) | 16 pass; `smog_libm_crate_canary` fails | 12 pass; 5 fail: `fluid_crowd_golden`, `fluid_fast_sources_golden`, `fluid_variable_dt_golden`, `smog_swarm_systems_golden`, `smog_libm_crate_canary` |

The baseline row proves the interposer reaches the calls and that the old class split was right. The WP2 row proves
no host `libm` result reaches any smog hash. With the mutation, check C23 also fails, naming
`crates/client/src/smog/kinematics.rs:157`.

## 4. What changed in the goldens (WP2 step 5)

Recorded on this host. The seven fluid goldens and the noise golden did not change: on their arguments the `libm`
crate's results equal glibc 2.41's after the solver's arithmetic (and the noise's `u8` quantisation). Eight goldens
were re-recorded in `a8cbfaa`, whose message lists the old and new values. To attribute them, the new code was run
once more with the old hashing (headings left out, heading and aim zeroed in the instance and uniform, stepped
rotations unhashed), and a probe compared the crate with glibc on the goldens' arguments:

| Golden | Old hashing on the new code | Why it moved |
|---|---|---|
| `kinematics_showcase_golden`, `kinematics_non_planar_golden` | reproduces the old value | headings are now hashed |
| `kinematics_spatial_grid_golden`, `kinematics_all_pairs_fallback_golden` | differs | headings hashed; the 400- and 600-bubble crowds start from the crate's trig, and 28 of the generator's 2,400 trig values for `i < 600` differ from glibc's by one ulp |
| startup, Visual Lab, swarm and live battle system goldens | differs | headings, aims and rotations hashed; the linear values of four showcase colours (palette 2, 3, 5 and 8) differ from glibc's `powf` by one ulp |

On a sweep of 10,000 headings, `atan2` differs from glibc's in 1,572 cases and the yaw quaternion in 293, by one ulp
each; these values only became hashed in WP2. The semantic smog tests and the unit tests of `fluid.rs`,
`kinematics.rs` and `math.rs` pass unchanged.

## 5. Performance against the owner's condition (WP3, plan D-6)

Arm A is the release build of `60c6628`, arm B of `a8cbfaa`, each from a `git archive` export under
`~/scratch/libm_smog/src-<arm>`, built outside the lock. For the startup instrument, arm A runs a copy of WP2's bench
with the per-call part removed (uncommitted), because `client::smog::math` does not exist there.

### 5.1 Instrument 1: cost per call (informational)

`cargo bench -p client --bench smog_math -- --only calls`, arm B, two runs (identical to 0.15 ns):

| Function | Arguments | Host ns/call | `smog::math` ns/call | Difference |
|---|---|---:|---:|---:|
| `exp` | [-1, 0] | 7.13 | 7.26 | +0.12 |
| `sin` | [-40, 40] | 7.24 | 8.60 | +1.35 |
| `cos` | [-40, 40] | 7.29 | 8.52 | +1.24 |
| `sin_cos` | [-π, π] | 10.21 | 6.79 | −3.42 |
| `atan2` | unit circle | 25.23 | 14.55 | −10.68 |
| `powf` | [0, 1]^1.3 | 10.80 | 61.77 | +50.97 |

Only `powf` is markedly slower, about six times glibc's; the noise texture calls it 163,840
times at startup, which predicts about 8.4 ms, and its 622,592 `sin` and `cos` calls about 0.8 ms more. Per frame,
the solver calls `exp` three times and the kinematics one `atan2` and one `sin_cos` per showcase bubble: well under a
microsecond in total.

### 5.2 Instrument 2: smog startup

`setup_smog_volume_system` wall time, timed by two marker systems around it in a fresh headless application per run,
15 runs per arm and three back-to-back A/B pairs (06:39 to 06:41 UTC, one-minute load 6.6 to 18.2):

| Pair | Arm A median (min-max) ms | Arm B median (min-max) ms | B − A |
|---|---|---|---:|
| 1 | 124.60 (122.21-133.64) | 135.55 (133.90-149.42) | +10.95 |
| 2 | 123.27 (121.56-126.36) | 135.99 (134.30-145.60) | +12.72 |
| 3 | 124.54 (121.52-140.12) | 138.11 (134.82-159.63) | +13.57 |

Startup grows by 11 to 14 ms (spread of the three differences: 2.6 ms), as instrument 1 predicts. The stop rule's
limit is 50 ms. The plan asked for the noise texture's time alone as well; it is a private function, so the startup
system is timed as a whole, and instrument 1 attributes the growth to the noise's `powf`.

### 5.3 Instrument 3: solver micro-benchmark

`cargo bench -p client --bench smog_solver -- --contend K`, p50 ms per step, pairs back to back (06:41 to 06:45 UTC):

| Contend | Scenario | A pair 1 | B pair 1 | A pair 2 | B pair 2 |
|---:|---|---:|---:|---:|---:|
| 1 | showcase | 5.936 | 5.963 | 6.222 | 5.877 |
| 1 | crowd_48 | 4.166 | 4.252 | 4.428 | 4.172 |
| 1 | battle_240 | 3.717 | 3.888 | 3.964 | 3.739 |
| 1 | after_fight (fight) | 3.720 | 4.009 | 3.918 | 3.732 |
| 8 | showcase | 6.096 | 6.951 | 6.311 | 31.665 |
| 8 | battle_240 | 3.936 | 4.886 | 3.976 | 24.168 |

At one grid the arms differ by less than their own pair-to-pair spread (showcase A 5.936 to 6.222, B 5.877 to 5.963).
The second contended B run started at a machine-wide one-minute load of 121.7 (another user's job) and is not
comparable; a third pair at 07:16 UTC (load 7 to 12) gave A 5.872 and B 5.934 ms (showcase) and A 3.833 and B
3.745 ms (battle_240), equal within noise. The solver's per-voxel loops call no transcendental function, so no
solver change was expected.

### 5.4 Instrument 4: frame budget on the RX 580 (measure of record)

The open-findings plan's D-6 command exactly: two release `frame_budget` binaries, one `with-gpu-lock` command per
run holding headless KWin 6.3.6 (`--virtual`, 1280×720) and the tool, `--display-path kwin-virtual`, the RX 580
(RADV POLARIS10, Mesa 25.0.7), `taskset -c 32-47`, about 10 s between runs. Each arm's fixture root was prepared once
on `lavapipe` under `xvfb-run` without the lock. Every report exists and no log contains `frame_budget failed`. The
lock never waited more than 23 s. **CPUs 32-47 busy** is the share of those CPUs busy during the run, from
`/proc/stat`, the run itself included (about 2.8 cores, 17 %). Times in ms.

Runs 01 to 12 are the protocol's series: A and B interleaved, 0 and 400 units, three pairs each. Ten of the twelve
started above the protocol's load of 8 (another agent's builds and tests ran throughout), so, as the protocol asks,
loaded runs were repeated at the end: three more 400-unit pairs (13 to 18, the middle pair in B, A order), the
scenario the stop rule reads; the load did not stay below 8, so the repeats are mostly loaded too. Runs 19 and 20 are
one `perf record -F 997` run per arm at 400 units.

| Run | Arm | Units | Started (UTC) | Load | CPUs 32-47 busy | Lock wait (s) | Frames | p50 | p95 | p99 | Update p50 | Combat p50 / p95 / update p50 |
|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| 01 | A | 0 | 06:45:05 | 82.38 | 42.9 % | 0.0 | 6,244 | 7.91 | 21.38 | 29.99 | 5.10 | |
| 02 | B | 0 | 06:47:12 | 26.03 | 90.5 % | 0.0 | 5,877 | 8.61 | 20.37 | 27.23 | 6.27 | |
| 03 | A | 400 | 06:49:18 | 16.97 | 45.9 % | 0.0 | 4,436 | 12.26 | 22.86 | 33.99 | 9.95 | 11.96 / 17.24 / 9.50 |
| 04 | B | 400 | 06:50:42 | 11.76 | 61.7 % | 0.0 | 4,201 | 13.60 | 22.23 | 32.22 | 10.78 | 16.00 / 27.06 / 12.47 |
| 05 | B | 0 | 06:52:02 | 10.54 | 75.5 % | 22.8 | 5,937 | 8.16 | 21.99 | 30.58 | 5.56 | |
| 06 | A | 0 | 06:53:41 | 11.14 | 34.6 % | 10.0 | 8,471 | 7.10 | 8.61 | 10.43 | 4.54 | |
| 07 | B | 400 | 06:55:08 | 6.15 | 37.2 % | 13.0 | 4,583 | 12.40 | 18.90 | 33.92 | 10.28 | 13.52 / 18.71 / 10.70 |
| 08 | A | 400 | 06:56:38 | 8.11 | 32.4 % | 6.6 | 4,889 | 11.93 | 17.02 | 20.67 | 9.97 | 13.22 / 18.80 / 10.58 |
| 09 | A | 0 | 06:58:02 | 5.89 | 45.0 % | 2.9 | 8,150 | 7.27 | 9.26 | 11.08 | 4.66 | |
| 10 | B | 0 | 06:59:22 | 8.05 | 98.7 % | 13.3 | 2,990 | 19.96 | 31.61 | 39.90 | 13.26 | |
| 11 | A | 400 | 07:00:57 | 20.81 | 82.5 % | 0.0 | 3,421 | 15.07 | 32.34 | 47.43 | 12.34 | 15.19 / 33.27 / 12.29 |
| 12 | B | 400 | 07:02:15 | 20.41 | 81.9 % | 0.0 | 3,140 | 17.27 | 33.71 | 40.37 | 12.88 | 15.81 / 31.93 / 11.41 |
| 13 | A | 400 | 07:04:11 | 22.21 | 99.3 % | 0.0 | 2,214 | 26.54 | 42.19 | 51.16 | 19.70 | 28.77 / 44.80 / 20.94 |
| 14 | B | 400 | 07:05:34 | 22.84 | 95.5 % | 0.0 | 2,263 | 24.83 | 49.15 | 62.84 | 18.90 | 33.82 / 53.90 / 25.11 |
| 15 | B | 400 | 07:07:01 | 33.39 | 59.0 % | 0.0 | 4,056 | 12.30 | 31.24 | 40.66 | 10.18 | 20.16 / 38.72 / 15.42 |
| 16 | A | 400 | 07:08:20 | 16.47 | 29.3 % | 9.3 | 5,298 | 10.83 | 16.57 | 18.99 | 8.84 | 12.80 / 18.12 / 9.79 |
| 17 | A | 400 | 07:09:48 | 8.75 | 27.7 % | 9.0 | 4,848 | 12.05 | 17.34 | 20.06 | 9.63 | 12.96 / 17.85 / 10.04 |
| 18 | B | 400 | 07:11:14 | 10.77 | 17.1 % | 8.3 | 4,965 | 11.67 | 17.18 | 19.64 | 9.43 | 12.94 / 18.15 / 10.05 |
| 19 | B (`perf`) | 400 | 07:12:40 | 10.05 | 56.4 % | 12.0 | 4,207 | 13.64 | 21.93 | 25.61 | 11.17 | 16.42 / 23.80 / 12.95 |
| 20 | A (`perf`) | 400 | 07:14:12 | 10.71 | 25.9 % | 8.9 | 4,619 | 12.75 | 17.28 | 20.79 | 10.38 | 13.56 / 18.69 / 10.68 |

Medians of the protocol's series (runs 01 to 12), with min to max:

| Arm | Units | Interval p50 | Interval p95 | Main update p50 |
|---|---:|---|---|---|
| A | 0 | 7.27 (7.10-7.91) | 9.26 (8.61-21.38) | 4.66 (4.54-5.10) |
| B | 0 | 8.61 (8.16-19.96) | 21.99 (20.37-31.61) | 6.27 (5.56-13.26) |
| A | 400 | 12.26 (11.93-15.07) | 22.86 (17.02-32.34) | 9.97 (9.95-12.34) |
| B | 400 | 13.60 (12.40-17.27) | 22.23 (18.90-33.71) | 10.78 (10.28-12.88) |

Main update p50 at 400 units, B − A per pair: +0.83 (03/04), +0.31 (07/08), +0.54 (11/12), then in the repeats
−0.80 (13/14), +1.34 (15/16) and −0.20 (17/18). The two cleanest pairs, with CPUs 32-47 below 40 % busy in both runs,
are 07/08 (+0.31) and 17/18 (−0.20). At 0 units B's runs were the more contended ones (90.5, 75.5 and 98.7 % busy
against 42.9, 34.6 and 45.0 %), and WP2 changes nothing that runs per frame with no unit, so the 0-unit gap is load.

**Attribution (`perf`, runs 19 and 20).** The `libm` crate's symbols (`libm::math::{sinf, powf, expf, cosf}`) hold
0.02 % of arm B's 189 K samples, about 38 samples, which is about 0.01 ms per frame over the run's 4,387 rendered
frames (and includes the startup noise texture). The host's `libm.so.6` holds 0.57 % in B and 0.53 % in A (Bevy,
wgpu and the rest of the client still use it). The solver, `SmogGrid3d`, holds 14.1 % in B and 15.1 % in A. Per
frame, WP2 adds three `sin` per battle unit (`bubble_team_color`) and replaces three `exp` per solver step: about
10 µs at 400 units by instrument 1, which `perf` confirms.

### 5.5 Stop rule

The plan's rule fires if either holds:

- at 400 units, arm B's median main-update p50 or frame-interval p95 is worse than arm A's by more than 2 % **and**
  by more than the spread (max − min) of arm A's three runs. Main-update p50: B 10.78 against A 9.97 ms, +0.81 ms
  (+8.1 %), but arm A's spread is 2.39 ms, so the second condition fails. Frame-interval p95: B 22.23 against A
  22.86 ms, better. **Does not hold.** (On the repeat pairs alone: update p50 B 10.18 against A 9.63 ms, A's
  spread 10.86 ms; p95 B 31.24 against A 17.34 ms, A's spread 25.62 ms. Does not hold either.)
- startup grows by more than 50 ms at the median: +11 to +14 ms. **Does not hold.**

**The stop rule did not fire.** WP2 is merged as planned and no owner question is filed. The 8 % gap in the
protocol's pairs is load on a machine whose one-minute load ran from 6 to 82 during the series, not a cost: the two
clean pairs differ by +0.31 and −0.20 ms, and `perf` attributes about 0.01 ms per frame to the crate.

## 6. Deviations from the plan

- **Check ID.** Plan D-4 names the new check C22, which the tiered-gates plan took for `crates/gate/suites.ron`
  meanwhile. The check is **C23** (`crates/workspace_checks/src/smog_math.rs`, test
  `c23_smog_code_calls_no_host_transcendental_function`), listed in charter §9 and in `DESIGN.md`.
- **C23 also bans `to_euler(`**, and its path form reports `f32::sin(` and `f64::sin(` but not `math::sin(`.
- **A site the audit missed:** the showcase system read each bubble's heading back from its rotation with
  `Quat::to_euler` (host `atan2f`). It now reads the render instance's heading, which the same system writes every
  frame. The value is identical whenever `sync_headings` re-derives the heading (horizontal speed above `1e-4`), which
  holds for every showcase bubble in every golden. Below that speed the old readback returned the negated heading, so
  the bubble's yaw flipped every frame; that flip is gone.
- **`client::smog::math` is `pub`**, not `pub(crate)`, so the bench can time it; its `//!` says so.
- **Goldens that did not move.** Plan D-5 expected every fluid, noise and C-18 golden to be re-recorded; they did not
  change (§4), so only eight constants were re-recorded.
- **Startup instrument** times `setup_smog_volume_system` as a whole, not the noise texture alone (§5.2).
- **Interposer build flag** `-fno-builtin` (§3).
- **Contended solver pair** repeated (§5.3).
- **Frame budget:** three repeat 400-unit pairs and one `perf` run per arm were added to the protocol's twelve
  runs (§5.4); the loaded 0-unit runs were not repeated, because the stop rule reads 400 units only.

Raw logs are under `~/scratch/logs/libm_smog_*` and the reports and binaries under `~/scratch/libm_smog/`; they are
scratch files that may be cleaned at any time, so the tables here are the record.
