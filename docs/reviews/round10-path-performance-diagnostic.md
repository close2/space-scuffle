# Round10 path-performance failure: independent source review

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: read-only diagnosis; no product edits, builds, benchmark launches or test reruns by this reviewer. Root owns one declared six-trial diagnostic series. The failed full gate remains failed; no threshold change or replacement pass is implied.

## Retained failure

`/tmp/space-scuffle-round10b-workspace-tests.log` records `sim_map::determinism_tests::test_performance_quality_gate` failing at 269.776963 ms for 100 fixed-seed Medium-clearance path requests, with 83 successful requests. The unchanged debug threshold is 250 ms. The performance test aborts at that assertion, so this failed invocation provides no 400-source vision timing. The concurrent 1,000-request path/vision checksum test passes its unchanged `0xadc6cb16b772f1a2` reference. The two-test harness finishes in 3.40 seconds.

Root's gate record reports tests exit101 after589.945 seconds, Clippy and formatting exit0, and448 source hashes unchanged. A successful checksum is semantic evidence; it does not prove acceptable elapsed time or identical machine scheduling.

## Source and historical comparison

All18 current `crates/sim_map` files, including its manifest and tests, are byte-identical to both `/tmp/space-scuffle-r7-pre50/source` and `/tmp/space-scuffle-r9-post50/source`. Workspace `Cargo.toml` and `Cargo.lock` are also byte-identical. The only current sim_types differences against post50 are render snapshot owner fields and their serialization tests. Against pre50, math.rs additionally changes only the approved `SimAngle::atan2` diagonal test from signed `abs()` to `unsigned_abs()`; the pathfinder's scalar/vector distance operations are unchanged. This supports absence of a recent pathfinding source regression, not a guarantee of identical emitted binaries or historical CPU conditions.

Reviewed current hot-path files: pathfinding.rs, traversal.rs, grid.rs and the actual test fixture. The41 no-ramp fast branch remains after both loaded cells' terrain checks and before ramp-profile lookup; actual ramps still take the complete validation path.41's optional Stage2 early non-improving-A* rejection remains deferred: the present implementation still checks geometry before deciding whether tentative g improves a visited cell. That existing redundant work is an optimization opportunity already documented in41, not evidence that it newly caused this failure.

Plan41 retained five coordinated filtered samples at84.219–88.842 ms, median86.741 ms, all83 successes. Its separate Callgrind totals fell1,415,834,826→951,736,895 instructions after the flat-cell branch; overlapping inclusive shares must not be added. Its normal concurrent two-test harness also passed (1.59 seconds total). Earlier contended41 samples reached222.540 ms even after the optimization. These records establish historical variance and reduced algorithmic work; they do not attribute today's entire delay to contention.

Important current hashes:

- pathfinding.rs: `fc795e2d4b86045d6ea3cd4f1ccee98a4094e6bb238c5a66567cd740e33dde3c`
- traversal.rs: `a34a95df989ea392b793f939958ce4f60343a74a6957608144328694d4a64216`
- determinism_tests.rs: `3bc609e2f5ba37b6ecd94ca635e00a4c8b589f800d57838c9b4bdacbe545f65b`
- Cargo.lock: `6de366b02ad5c61cfa646e37e8c758bf3f8e165291f10c43fdc3a2c72584dc3f`

## Diagnostic interpretation and decision

Root declared one existing-binary series in the order solo, pair, pair, solo, solo, pair, retaining all results and CPU/load/pressure observations under `/tmp/space-scuffle-r10-path-timing-diagnostic01`. Solo isolates the performance fixture; pair retains the actual workspace harness concurrency with the checksum workload. Neither changes the test source or acceptance threshold. Root's initial host observation includes substantial unrelated CPU work; no such process is stopped or reprioritized.

The source review currently demonstrates no required algorithm repair. Do not implement deferred41 Stage2, alter test scheduling, ignore the test or relax the threshold merely to manufacture a green gate. A bounded passing diagnostic distribution would establish that this exact binary can meet the existing threshold in those recorded conditions and that the historical failure was not consistently reproduced. It would not prove the historical scheduler cause or retroactively pass round10b. Consistently slow samples would justify a separate reviewed profiling/repair step before more full gates. Root must explicitly decide the next gate action from the complete declared series; no automatic retry-until-pass.

## Completed declared diagnostic series

Independently read root's results.json and retained six-run record. Exact existing
binary SHA256 is `513570ca7ac8d3c2f00edf02c02a3c88176541f23e855a809e8f7ce7c2f0a54a`.
All six exits are0, every path batch retains83 successes, and the three full paired
harness invocations also pass their checksum test. Ordered path times are99.802,
128.212,114.366,113.183,111.297 and122.268 ms. Solo range99.802–113.183 ms;
paired range114.366–128.212 ms. Vision ranges59.181–69.395 ms. Root recorded
process CPU usage, host load and CPU pressure for every sample.

This bounded series did not reproduce the269.777 ms failure, including three
normal-concurrency paired runs. Together with unchanged path source, it supports
a transient timing failure rather than a demonstrated recent algorithm regression.
It does not identify the historical scheduling/frequency cause. No code repair
is justified by the current evidence. Root explicitly launched a new unchanged
full round10c gate; its outcome remains separate from both the failed10b gate and
these diagnostic passes. No automatic further repetitions are prescribed.
