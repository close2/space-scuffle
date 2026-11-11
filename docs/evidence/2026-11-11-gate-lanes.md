# Evidence: faster gates (smog contract, shared paths, lane A/B)

Plan: [`docs/plans/2026-11-11-faster-gates.md`](../plans/2026-11-11-faster-gates.md). This file records WP4's
proof and audit, WP5a's shared-path audit, and WP6's A/B measurement as each lands.

## 1. WP4: the smog system goldens hash what the smog reads

### 1.1 Fields the smog code reads or writes

Re-derived from the code (`crates/client/src/smog/mod.rs` unless noted). It is the plan's list of eight fields,
exactly:

| Field | Where |
|---|---|
| `anim_state` | `setup_smog_volume_system`, through `bubble_shader.rs::still_frame_lanes` and `frame_index_at` |
| `heading_angle_rad` | read in `sync_bouncing_smog_bubbles_system` (scratch harvest) |
| `world_pos`, `velocity`, `heading_angle_rad` | written in `sync_bouncing_smog_bubbles_system` |
| `shape`, `ground_z` | `live_battle_volume` and `collect_smog_sources`, from `update_smog_field_system` |
| `velocity`, `radius`, `entity_id` (team colour of a bubble outside the showcase) | `collect_smog_sources` |

`BubbleUniformGpu::from_instance` (`bubble_shader.rs`) reads `shape`, `world_pos`, `radius`,
`heading_angle_rad`, `aim_angle_rad`, `is_attacking`, `anim_state`, `hp_ratio`, `shield_ratio`,
`shield_hit_flash`, `hull_hit_angle_rad`, `hull_hit_flash`, `build_or_queue_progress`, `selected`,
`concealment.is_concealed()` and `speed_boosted`. It reads none of `status`, `stance`, `owner` and
`barrier_ratio`, so all four stay in the "never read" test, with `silhouette`, `unit_type`, `weapon_visual`,
`attack_target_pos` and `attack_swing`.

A correction to plan §5: the system goldens hash only the bubbles that carry `BouncingSmogBubble`, the ten showcase
bubbles, whose instances come from the client's `showcase_render_instance`. The test's own `battle_unit`
instances reach the hash only through the fluid grid. The conclusion stands.

### 1.2 Re-recorded goldens (glibc 2.41, x86_64)

| Golden | Old | New |
|---|---|---|
| startup | `0xac23_14d4_13dd_e3ed` | `0x4459_0814_2009_2b71` |
| Visual Lab | `0x5150_d25b_ae37_bbfc` | `0x33ee_40dd_7e58_d19e` |
| swarm | `0xb9ec_a555_c65c_d6cf` | `0xc4b0_ef45_a4e7_36ab` |
| live battle | `0xb986_235a_dab0_bdd5` | `0xdac9_628c_19f9_46f1` |

### 1.3 Proof that no smog output changed

The output of the comparison runs is kept in `~/scratch/gate-ab/2026-11-11-wp4-smog-proof.out`.

- (a) On the same tree, with the `Debug` term switched back on, all 19 tests passed on the old constants.
- (b) With the new hashing, every non-system golden passed unchanged (7 fluid, 4 kinematics, noise, the `libm`
  canary); only the four system goldens moved, to the values above.
- (c) A temporary dump of every hashed bubble's transform (f32 bits) and `BubbleUniformGpu` bytes was taken three
  ways: HEAD's test file, the new file, and the new file with the `Debug` term. All three were byte-identical:
  startup 2,160 bytes, Visual Lab 8,640, swarm and live battle 6,480 each (the showcase bubbles are hidden and not
  stepped in those two tabs, so their dumps agree).

Test first: with a temporary `Debug`-based body, `a_bubble_field_the_smog_never_reads_does_not_move_the_system_hash`
failed (11199385904259833432 against 5326919497195356506); with the real body both new tests pass.

### 1.4 Audit of client tests that compare a hash or a recorded golden

| Test | Compares | Depends on | Contract |
|---|---|---|---|
| `smog_golden_tests`: fluid, kinematics, noise, canary | FNV of the client smog solver, kinematics and noise | client code and `libm` | none needed |
| `smog_golden_tests`: four system goldens | transforms, uniform bytes, visibility, grid, `hash_smog_inputs` | client code; `AnimState` order and `Concealment::is_concealed` through `from_instance`; `sim_map` map size; `sim_core` catalog order through `BattleDefinition::load` | none added: R-9 of the plan (`full` catches it); a new field or variant fails to compile under workspace clippy |
| `mesher_golden_tests` | FNV of the terrain mesh and picks | client mesher on `sim_map` grids; `asset_baker` terrain themes | in `downstream["sim_map"]`; terrain textures under the "media" rule |
| `effect_slice_gpu_tests` | PNG goldens with a tolerance, per driver | client effects; `sim_core` catalog and `sim_types` splash specs | GPU class: the "GPU renderer" and "catalog" rules; a sim change to splash publication is caught by `full` |
| `runtime_battle_tests`, `pipeline_presentation_tests`, `player_presentation_tests`, `headless_tests` | `compute_checksum` equal or unequal between runs (no recorded value) | `sim_core` determinism | already in the `sim_map`, `sim_types` and `sim_core` contracts |
| `#[cfg(test)]` modules of `crates/client/src` | relational only (sparse equals dense); no recorded hash constant | client code | none needed |

No contract was missing, so `suites.ron` is unchanged by WP4.

## 2. WP5a: shared paths between test binaries

Lanes run binaries of the same crate at once (plan §3.7). No path is written by two different test binaries of
`client`, `client_diagnostics` or `asset_baker`, or written by one while another reads it, so WP5a changes no file.

Method: (1) a source read of every write in their `tests/**` (support files included) and `#[cfg(test)]` modules:
`fs::write`, `File::create`, `create_dir*`, `.save(`, `OpenOptions`, `copy`, `rename`, `remove_*`, and every
`CARGO_BIN_EXE_*` call whose output paths default to relative locations; (2) all 68 general (not GPU, not valgrind)
test executables run one after another under `strace -f --seccomp-bpf`, recording every file-creating syscall of
the executable and its children (the `asset_baker` CLI, ffmpeg, the `client` binary). All 68 passed, and after
removing `/proc`, `/dev`, `/sys` and per-test `tempfile` directories, the only writes left were symlinks inside
tempdirs that point at `assets/`. The traces and scripts are kept in `~/scratch/gate-ab/2026-11-11-wp5a-traces/`.
The GPU and valgrind binaries were audited by reading the code only.

| File | Path written | Other binaries writing or reading it | Verdict |
|---|---|---|---|
| All 68 general targets of `client`, `client_diagnostics`, `asset_baker` | only per-test `tempfile` directories (confirmed by strace, children included) | none | safe |
| `asset_baker/tests/common/cli.rs` and every `CARGO_BIN_EXE_asset_baker` call | every output directory and ledger inside a tempdir; calls relying on relative defaults set `current_dir` to a tempdir | none | safe |
| `client/tests/sync_allocation_count_tests.rs` (valgrind) | its own kept tempdir, passed to its children | its own children; valgrind runs alone (L-2) | safe |
| `client/tests/support/gpu_run.rs`: `/run/lock/gpu.lock` | the GPU lock file | every GPU child and `with-gpu-lock`, through `flock` | safe, shared by design |
| `gpu_run.rs`: `SPACE_SCUFFLE_GPU_RUN_LEDGER`, `SPACE_SCUFFLE_GPU_RUN_LOG_DIR` | opt-in; one `O_APPEND` write per line, or `<label>-<pid>-<n>.log` | never set by the gate; names unique per process | safe |
| `gpu_run.rs`: Xvfb display and `xvfb-run` auth | display numbered from the pid, then `-a`; auth directory from `mktemp` | only `gpu` binaries start Xvfb, one at a time on the GPU lane | safe |
| GPU children: Mesa shader cache | driver-managed, safe for concurrent processes | general binaries build no render device (C21) | safe |
| `client/tests/effect_slice_gpu_tests.rs` goldens | `tests/data/effects/<driver>/`, written only with `SPACE_SCUFFLE_EFFECT_GOLDEN_UPDATE` | no other binary reads them | safe |
| Other GPU tests of `client` and `client_diagnostics` | child output, screenshots and reports in the parent's tempdir | none | safe |

Outside the three crates: `sim_core/effect_tree_allocation_tests` writes dhat output under fixed names in
`CARGO_TARGET_TMPDIR/effect_tree_allocation/`; only that binary writes there, and valgrind jobs run alone.

## 3. WP6: the A/B of `--jobs 1` against `--jobs 3`

### 3.1 Protocol

One clean tree, `main` at `3c98e71` (tree `ffbc8f49576785322730d9289f801518dd5c1167`), in an agent worktree with its
own `CARGO_TARGET_DIR` (`~/scratch/cargo-target/gate-ab`). The script, outputs, `lanes.tsv` files, stamps and
sampler logs are kept in `~/scratch/gate-ab/2026-11-11/` (`ab.sh`, `analyze.py`, `gate-{warm,A,B}.out`,
`{A,B}/lanes.tsv`, `sampler-{A,B}.log`, `A-stamp/`, `stamps/`).

1. Warm build: `cargo gate full --no-run` (green, 1,270 s).
2. Run A: `cargo gate full --jobs 1 --no-reuse --stamp-dir …/stamps --log-dir …/A`.
3. Run B: `cargo gate full --jobs 3 --no-reuse` with the same stamp directory, so its estimates and order came
   from A's stamp, and `--log-dir …/B`.
4. During each run a sampler under `timeout 4h` logged, every 30 s, the load average, the slice's `anon` and
   `file`, and `/proc/pressure/io`; it was killed when its run ended.

The gate lock was free and no other gate ran. One other agent (effects, round 2) built and rendered in parallel
without the gate lock, so the background load differed between A and B (§3.3).

### 3.2 Results

| Quantity | A (`--jobs 1`) | B (`--jobs 3`) |
|---|---|---|
| Result | GREEN, 199 of 199 targets pass | GREEN, 199 of 199 targets pass |
| Test-phase wall time (`lanes.tsv`) | **7,076 s** | **2,842 s (−59.8 %)** |
| Sum of executable wall times | 7,066 s | 7,055 s (overlap 2.5×) |
| Whole gate (start to end) | 7,950 s | 3,947 s |
| Steps (fmt, clippy, build, doc tests, commands) | 855 s | 1,062 s |
| `FLAKY` / `TIMEOUT` | none | none |
| Slice `anon` peak (gate report) | 38.2 GiB | 38.5 GiB |
| Load average, median / p90 / max | 33.0 / 52.9 / 110.8 | 23.6 / 32.1 / 41.5 |
| `/proc/pressure/io` some avg60, median / max | 69.5 / 92.8 | 72.1 / 95.4 |
| Slice `file` at start | 51.0 GiB | 31.4 GiB |

The steps differ by cache state, not by lanes: A's clippy was warm (7 s) and its doc tests and replays rebuilt
(699 s, 139 s); B's clippy rebuilt (919 s) and its doc tests and replays were warm (13 s, 5 s). Steps are outside the
acceptance (plan §7).

B in detail:

- 92 targets ran exclusive first (valgrind, and every target with no recorded peak, mostly small and fast) for
  600 s in total; then the GPU lane ran 19 binaries in 2,242 s and the two general lanes 42 and 46 binaries in
  2,108 s and 2,105 s.
- The slice `anon` peak of both runs is `client/sync_allocation_count_tests` under valgrind, whose process tree
  peaked at 37.0 GiB. It ran alone in both runs (L-2). The largest sum of estimates that ran together on the lanes
  was 28.5 GiB (measured peaks 29.8 GiB), below the 40 GiB budget; the ceiling (56 GiB) was never reached and the
  gate never held a target back for memory.
- GPU lane: its 19 binaries took 2,242 s in B against 2,637 s in A (15 % faster, within the 15 % criterion). Every
  GPU binary passed, so no child reached the harness's 120 s run limit or its 290 s lock wait.
- No target slowed more than 2×. The largest ratios: `client/smog_golden_tests` 25.9 → 46.6 s (1.80×, beside
  `round2_visual_tests`, `headless_tests`, `pipeline_presentation_tests` and `lab_scene_tests`),
  `client/simulation_diagnostics_tests` 24.0 → 41.1 s (1.71×), `gate/suite_catalog_tests` 22.8 → 34.9 s (1.53×).
  The long battle drivers slowed 7 to 22 % (`battle_input_tests` 540 → 579 s, `battle_hud_tests` 443 → 507 s,
  `battle_card_tests` 380 → 440 s, `battle_context_order_tests` 202 → 247 s); the GPU and valgrind binaries got
  faster (`effect_slice_gpu_tests` 658 → 568 s, `skirmish_match_tests` 243 → 140 s,
  `sync_allocation_count_tests` 337 → 217 s).

### 3.3 Background load

The load was higher during A (median 33.0) than during B (23.6), and B's own lanes add about three running binaries
to its figure, so the outside load was roughly 31 during A and 19 during B: it favoured B. I/O pressure was the
same in both runs (median 69.5 and 72.1). The difference in the medians is 1.4×, below the plan's factor of two
that would repeat the pair.

It does not change the verdict:

- A was not slowed by the load. Its heaviest binaries ran at or below their figures in the evaluation's window
  (`battle_input_tests` 540 s against 605 s, `editor_app_tests` 445 against 500, `battle_hud_tests` 443 against
  461, `battle_fog_tests` 418 against 475, `battle_card_tests` 380 against 456).
- The sum of executable times is the same in both runs (7,066 s and 7,055 s): B's saving is the 2.5× overlap of its
  lanes, not faster binaries.
- For B to miss the 40 % threshold, A's test phase would have to have been 4,737 s or less, that is A's binaries
  33 % faster than they ran, which their agreement with the historical figures rules out.

### 3.4 Verdict

**Accepted.** Every criterion of plan §7 holds: test-phase wall time −59.8 % (at least 40 % required); every target
that passed in A passed in B, with no `FLAKY` and no `TIMEOUT`; B's slice `anon` peak 38.5 GiB, below 56 GiB, with at
most 28.5 GiB of estimates on the lanes at once; no GPU child at its limits and the GPU lane 15 % faster than in A;
no target slowed more than 2×. The default becomes `--jobs 3` (WP7). CI stays at one lane.
