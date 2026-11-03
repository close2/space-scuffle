# Round 10b: unchanged pathfinding timing failure

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

This is a read-only diagnostic review, not a performance fix or a passing workspace
gate. Product source, compiler profiles and test thresholds remain unchanged.

## Observed gate result

`/tmp/space-scuffle-round10b-workspace-results.json` records workspace tests exiting
101 after 589.945 seconds, strict all-target Clippy exiting 0, and formatting exiting
0. The test log contains one failing test and one panic:
`sim_map::determinism_tests::test_performance_quality_gate`, with 100 queries taking
269.776963 ms and 83 successes. Its concurrently scheduled checksum test passed the
existing `0xadc6_cb16_b772_f1a2` reference. Root counted 579 passed tests, one failed
test and 69 harnesses, and verified all 448 frozen inputs unchanged.

Cargo stopped at this failing integration binary. The later sim_map funnel,
map-format, ramp-profile, ramp-traversal, validation, vision and wall-off suites,
sim_types tests and workspace doctests were not reached. The visibility timing
assertion inside the failing performance test was also not reached: the earlier
path assertion aborts that test. Executed client and sim_core suites passed,
including the repaired real-palette diagnostic fixture; this is not a claim that
the whole workspace passed.

The log retains presentation validation errors in `editor_app_tests`:
MissingAcquireWait, WRITE_AFTER_PRESENT and semaphore VUID01286. These are the
previously recorded editor scope limitation; the editor remains untouched. Passed
client fixtures also log deliberate invalid-resource/pipeline/lifecycle cases.
Other observed warnings include XSETTINGS/XRandR/software-adapter messages, four
egui missing-node-data messages in `runtime_battle_tests`, and inherited local Naga
compiler warnings. A successful test exit does not certify those messages as visual
quality evidence. None is an additional failing test in this gate.

## Profile and scheduling facts

Neither workspace nor sim_map Cargo.toml declares a profile override. No Cargo
`config` or `config.toml` exists in the workspace, `/home/cl/projects`, `/home/cl`,
`/home`, or `/` `.cargo` directories. This reviewing agent's environment has no
Cargo profile/build overrides, RUSTFLAGS or RUST_TEST_THREADS; that observation is
not a retrospective dump of the gate runner's environment. The runner executes
the three required Cargo commands sequentially, without test-thread/profile flags.
The test executable is `target/debug/deps/determinism_tests-699923ae9d0b5ace` and
uses the unchanged debug threshold: `duration.as_millis() < 250`.

Despite the test module's old word “simultaneous”, its 100 path requests are
sequential in one test thread. The normal libtest harness also schedules the
1000-query checksum/vision test in this two-test executable. The measured interval
includes request generation and queries, excludes arena/pathfinder initialization,
and measures wall time rather than scheduled CPU execution. Therefore harness and
host scheduling can affect it. Those facts do not establish the historical cause
of the 269.777 ms result.

## Existing optimization evidence

Plan41's flat-terrain branch is still present after bounds/terrain flag validation
in `traversal.rs::cardinal_edge_is_connected`; actual ramps still use the full
predicate. Its optional stage2 A* candidate rejection remains deferred: current
pathfinding computes the generation-aware tentative-score improvement after the
legality checks. There is no newly established performance regression at that site.

The preserved round5 Callgrind comparison measured 1,415,834,826 versus 951,736,895
instructions (32.78% fewer), primarily reducing redundant ramp-profile work in
portal connectivity. Those overlapping inclusive counters describe that historical
binary, not a current instruction profile. The coordinated round5 native samples
were 84.219–88.842 ms; host/load and executable context differ, so comparing their
minimum with a failed workspace wall-time sample cannot isolate code cost.
The independent checksum lane additionally verified all 18 sim_map source files
and Cargo inputs identical across the preserved pre/post50 snapshots.

## Bounded unchanged-binary follow-up

Root ran a declared six-run diagnostic series without source/profile/harness edits:
solo path runs 99.80/113.18/111.30 ms; normal paired runs
128.21/114.37/122.27 ms. All retained 83 successes. Visibility took 59.18–69.40 ms,
and both actual assertions and the paired reference checksums passed. Every sample,
including the original failed gate, remains evidence. These samples support the
single unchanged full-gate10c retry, with host pressure/load recorded at its start
and end. They do not retroactively pass gate10b or prove contention as its cause.

No product optimization is justified by the present evidence. Do not weaken the
threshold, serialize the acceptance harness, alter optimization flags, skip tests,
or stop unrelated host work. If unchanged failures recur, first request a bounded
current-binary instruction/scheduler diagnostic and compare actual work; do not
apply plan41 stage2 merely because it is an available idea. Gate10c remains the
required complete verification, not a selection among isolated passing samples.
