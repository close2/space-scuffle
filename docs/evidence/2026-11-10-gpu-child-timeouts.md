# Evidence: GPU test children that exceed their 120 s limit under load

Short-note investigation (charter §6, single-cause bug fix) of four GPU-harness children that timed out in package
gates and passed on the immediate rerun:

| Gate log | Target and test | Child |
|---|---|---|
| `~/scratch/logs/gate/7dc7a6f581451f35f81556ce5d865f64e299fd24/package/` (2026-11-09) | `client_diagnostics/frame_budget_tests`, `frame_budget_smoke_run_writes_a_valid_report` | `frame_budget` binary, scenario 400, validated |
| `~/scratch/logs/gate/391705a94dff8e871f7345cd6416c62ee0730531/package/` (2026-11-10) | the same | the same |
| the same | `client/runtime_battle_tests`, `production_plugin_schedule_initializes_terrain_and_restores_tab_camera` | in-process production plugin schedule |
| the same | `client/battle_hud_window_tests`, `the_skirmish_window_gives_the_camera_only_what_the_panels_leave` | windowed HUD child |
| the same | `client_diagnostics/lib`, `shell_order_audit::gpu_tests::actual_loaded_scene_validates_temporary_and_synchronized_proxy_lifecycles` | `actual_loaded_scene_child` |

Two flakes of `frame_budget_tests` in a week make this a bug to fix (tiered-gate blueprint §3.5). The owner's rule
(2026-11-05) is to fix the cause, not to scale the run limit with load.

## Method

Debug test binaries of the branch, built in `~/scratch/cargo-target/gpu-timeouts`. Each child ran through the
harness on `radv`, its test alone (`<binary> --exact <test>`), or the whole binary as the gate runs it. The harness
now records, per child, the time it spent in each phase of its own log (`ChildTimeline` in
`crates/client/tests/support/gpu_run.rs`), so the numbers below come from the children's own timestamps:

| Phase | Ends at |
|---|---|
| launch | the child's first log record: `xvfb-run` starting its server, the executable loading, work before Bevy's `LogPlugin` |
| render device | Bevy's `AdapterInfo` line |
| app build | `Creating new window` (windowed children only) |
| startup and frames | the child's last log record |
| after the last log record | the exit the harness observed |

Alongside each run the measuring script read the user slice's cgroup counters (`memory.events` `high`,
`memory.stat`, and the `cpu`, `memory` and `io` pressure totals) and `perf stat -e major-faults,task-clock` over the
test process and its children. Scripts and raw ledgers: `~/scratch/gpu-timeouts/` (`summary.tsv`, `ledger-*.tsv`,
`childlogs/`).

## Where the time went in the failing runs

From the timestamps in the gate logs (the harness then printed only the last 30 lines, so the earlier phases of two
runs are missing):

| Child (gate run) | Phase in the failing run | Same phase, healthy run |
|---|---|---|
| windowed HUD (`391705a`) | `SystemInfo` to `AdapterInfo` 9.0 s; to the window 31.6 s; to the audio policy 10.7 s; to `Media unapproved` 19.6 s; killed 120 s after spawn | 0.06 s; 0.2 s; 0.01 s; 22.7 s |
| production plugin schedule (`391705a`) | `SystemInfo` to `AdapterInfo` 38.9 s | 0.1 s |
| `actual_loaded_scene_child` (`391705a`) | `SystemInfo` to `AdapterInfo` 15.2 s; to the audio policy 72.3 s | 0.2 s; under 1 s |
| `frame_budget` (`7dc7a6f`) | Vulkan instance created to the event loop starting (`XRandR` warning) 38.0 s | 0.25 s |
| `frame_budget` (`391705a`) | the same, 48.4 s | 0.25 s |

Phases that take a tenth of a second on a healthy run took 15 to 72 s, 100 to 300 times longer. CPU contention cannot
do that: a child of one busy thread slowed by 100 would need 100 runnable threads per core of the slice.

## Measurements before the fix

These runs used the harness with phase recording but without the pre-read; the scope experiment below used a
temporary switch that is not part of the commit.

### Healthy, warm page cache

| Child | Run time | Phases |
|---|---|---|
| windowed HUD | 26.5 s, 27.4 s, 30.0 s, 32.3 s | launch 0.2 s, render device 0.1 s, app build 0.2 s, startup and frames 25.8 to 31.5 s |
| production plugin schedule | 26.7 s, 28.1 s | launch 0.2 s, render device 0.1 s, startup and frames 26.3 to 27.6 s |
| `frame_budget` scenario 400 | 32.9 s, 37.6 s | launch 0.6 s, render device 0.1 s, the rest after the last record (`RUST_LOG=warn`) |
| `actual_loaded_scene_child` | 26.1 s | launch 0.2 s, render device 0.2 s, startup and frames 25.4 s |

`perf stat` of the HUD test: 30.4 s of task clock for a 30.0 s child, and 34 major faults. The children are bound
by one busy thread, not by the GPU.

### CPU load from our own side (hypothesis rejected)

A nice-0 `cargo build --release -j 16 -p client` from a clean target directory, repeated, ran in the login session
while the children ran; the machine's load average was 250 (another user's builds included):

| Child | Run time under the build | Idle |
|---|---|---|
| windowed HUD | 30.9 s | 26.5 to 32.3 s |
| production plugin schedule | 29.0 s | 26.7 to 28.1 s |
| `frame_budget` | 38.3 s | 32.9 to 37.6 s |
| `actual_loaded_scene_child` | 31.2 s | 26.1 s |

At most 1.2 times slower. The CPU controller gives each user slice an equal share, so the other user's load does
not reach us, and sixteen compiler threads in our own slice leave the child most of a core. Running each child in
its own `systemd-run --user --scope` (so its group competes with the whole session at equal weight) was tried and
measured no better (37.6, 40.8, 40.0 and 38.2 s under the same build), so it is not part of the fix.

### Whole binaries as the gate runs them (hypothesis rejected)

`runtime_battle_tests` (18 tests, 16 threads), `client_diagnostics` unit tests (103 tests, 16 threads) and
`frame_budget_tests` (6 tests), each child overlapping its sibling tests: 27.4 s, 26.1 s and 37.0 s. Peak anonymous
memory of the slice 7.0 GiB, no `memory.high` event.

### Cold page cache (the cause)

The slice's counters since boot: 37.5 million `memory.high` events, 5.3 billion pages scanned by direct reclaim,
1.55 billion file pages refaulted, 52.5 million major faults. The page cache fills the slice to its `memory.high`
(44 GiB; `memory.current` 35 to 38 GiB of which 32 to 34 GiB is file cache during these measurements), so every
large write (a gate links 138 test executables, about 18 GiB; other agents build at the same time) evicts older
file pages, among them executables the gate built earlier and runs later.

Agent worktrees build under `~/scratch` (charter §7), which is `/dev/sdb`, a rotational disk shared with the other
user. During these measurements its I/O pressure stood between 60 % and 97 % (`/proc/pressure/io`, full), its
average request took 33 ms, and a sequential read of an evicted 31 MiB file took 3.8 s (8 MiB/s). The root disk
(`/dev/sda`, solid-state, where the worktrees and their `assets/` live) read an evicted 234 MiB executable in 0.7 s.

The measuring script dropped the cached pages of the test executables, the `frame_budget` binary, the shipped
assets and the two Vulkan libraries with `posix_fadvise(DONTNEED)`, then ran the HUD test with the harness as it was
(no pre-read):

| Windowed HUD child | Warm | Cold |
|---|---|---|
| Run time | 30.0 s | 89.2 s |
| Major faults (`perf stat`) | 34 | 738 |
| Task clock | 30.4 s | 27.3 s |
| launch | 0.2 s | 24.8 s |
| render device | 0.1 s | 7.5 s |
| app build | 0.2 s | 20.5 s |
| startup and frames | 29.4 s | 36.3 s |

The CPU work is the same; the extra minute is the executable paging in at about 80 ms a fault, and its phase
pattern is the failing runs' pattern. Reading the 235 MiB executable sequentially first took 38.0 s on the busy disk,
after which the child ran in 28.1 s with 189 major faults (assets and libraries on the solid-state disk).

The 120 s limit already excludes the wait for the GPU lock: the lock is taken before `timeout` starts. It included
the executable's paging, which depends on the disk and the other users of the page cache, not on the child.

## Fix

The harness reads the part of the child's executable that the loader maps (up to the end of its last `PT_LOAD`
segment: about 88 of 235 MiB, the rest being symbol tables and debug sections) sequentially, in 1 MiB reads, before
it takes the GPU lock (`preload_executable`), and records the time as `preload_s` in the ledger. On a warm cache
the read costs a few hundredths of a second; on a cold one it replaces several hundred seeks inside the locked,
time-limited run with one sequential read outside it, which also shortens how long the GPU lock is held. The run
limit, the lock wait and the work each child does are unchanged; nothing scales with load.

The harness also records each child's phases in the ledger, and a timeout now names its slowest phase and lists the
others, so the next slow child says where its time went. `SPACE_SCUFFLE_GPU_RUN_LOG_DIR` keeps each child's whole
log for an investigation like this one.

Rejected: running children in their own `systemd-run --user --scope` (measured no better under our own build load,
above), lowering thread caps (the binaries' sibling tests did not slow their children), and fewer frames or units in
the smoke tests (the frames are not where the time went, and each test's proof is kept whole).

## After the fix

Same binaries rebuilt with the fix, through the harness on `radv`; `preload_s` is outside the run limit and before
the lock.

| Child | 5 warm runs: run time (preload) | Cold, files evicted: preload, run time | Cold under our nice-0 `cargo build --release -j 16`: preload, run time |
|---|---|---|---|
| windowed HUD | 37.5, 27.9, 27.0, 26.8, 27.0 s (0.03 s) | 8.6 s, 35.2 s; 21.0 s, 27.6 s | 7.1 s, 30.0 s |
| production plugin schedule | 26.9, 26.3, 26.9, 28.0, 26.5 s (0.03 s) | 0.2 s, 32.6 s | 5.8 s, 32.1 s |
| `frame_budget` scenario 400 | 31.8, 32.6, 31.5, 32.6, 31.3 s (4.4 s once, then 0.02 s) | 7.3 s, 32.3 s | 12.6 s, 38.7 s |
| `actual_loaded_scene_child` | 24.3, 24.6, 24.8, 24.6, 24.6 s (0.01 s) | 14.5 s, 37.2 s | 7.5 s, 33.2 s |

Before the fix the cold HUD child ran 89.2 s inside the limit; after it, the slowest of the 32 runs above took 38.7 s,
leaving 81 s of the 120 s limit. The cold runs' phases before the first frame stay at warm values (launch 0.2 to
1.5 s, render device 0.1 to 0.4 s). The first `frame_budget` warm run's 4.4 s pre-read is the gate's own case: the
binary had just been relinked and part of it was already evicted. One warm HUD run waited 289.5 s for the GPU lock,
behind another agent's `with-gpu-lock` captures of 3 to 4 minutes each, within 0.5 s of the 290 s lock wait; that is
a separate risk, recorded here and in the hand-back.

On `lavapipe` (`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`, limit 300 s), once each: windowed HUD 32.9 s (pre-read
5.0 s), production plugin schedule 30.5 s (5.7 s), `frame_budget` 57.8 s (9.1 s), `actual_loaded_scene_child`
28.5 s (0.04 s).

## Out of scope, for the owner

The scratch disk's throughput under shared load (8 MiB/s sequential, 33 ms a request, I/O pressure 60 to 97 %) and
the user slice's `memory.high` with page cache counted against it make every cold executable on this server slow,
not only GPU children: the parent test process of the HUD test took 13.6 s just to list its tests from a cold
executable, and relinking three test executables took 39.5 minutes during this work. Moving agent target
directories, or the slice limits, are host decisions.
