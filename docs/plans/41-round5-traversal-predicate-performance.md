# Round 5: preserve legal traversal while removing repeated predicate work

> Evidence under /tmp in this document was on the development notebook and was not retained (Q-10, 2026-11-03).

Status: approved; stage1 implemented and targeted verification passed. Stage2 deferred because the measured stage1 result supplies headroom. Coordinated full-workspace acceptance remains pending other ongoing round changes. This narrow gate repair precedes36 native input work.

## Goal and evidence

Restore useful headroom under the existing100-query performance gate without relaxing its threshold, changing path geometry or hiding shared-machine contention. Preserve exact deterministic waypoints/checksum, fog/known-structure semantics, all ramp boundaries, continuous movement validation and malformed-ramp rejection. No editor or rendering work.

The coordinated workspace run failed `sim_map::determinism_tests::test_performance_quality_gate`:100 queries took333.803ms against the existing250ms debug limit. The unchanged targeted root samples took234.712ms,240.543ms and334.044ms, with83 successful requests each (`/tmp/space-scuffle-r5-path-performance-samples.log`). This proves variance and insufficient headroom, not a measured before/after regression attributable solely to traversal25. The host had roughly51 load average on24 available CPUs and unrelated compilation/corpus work. Do not claim those processes caused the entire failure without scheduler evidence.

A read-only Callgrind run of the exact existing filtered test binary measured1,415,834,826 instructions. Inclusive shares overlap: pathfinding98.17%, portal construction57.65%, terrain connectivity78.83%, `cardinal_edge_is_connected`73.34%, `ramp_profile`27.29%, and cell request passability37.56%. Artifacts: `/tmp/space-scuffle-r5-path-callgrind.out`, `...-inclusive.txt`, `...-test.log`, `...-callgrind.log`. The instrumented run necessarily fails its wall-clock assertion and exits101; its15-second duration is excluded from all performance acceptance claims. The profile stops at that assertion before visibility timing, so it describes path work plus initialization/harness overhead.

The source explains this cost: widened portal strips repeatedly test several ordinary cardinal edges. `cardinal_edge_is_connected` already loads both cells and validates flags, then calls `ramp_profile` twice (which fetches the cells again), checks optional profile validity and only afterward handles the common no-ramp/same-level case. The default64×64 arena contains only16 ramp cells. A* also validates terrain, footprints and fog before checking whether the neighbor's tentative g-score could improve its current record. These are pure redundant computations; neither is necessary for path ordering or correctness.

## Stage 1: specialize the shared predicate by existing terrain data

Inside `GameMap::cardinal_edge_is_connected`, retain the existing endpoint bounds/lookup and both existing terrain-usable flag checks first. Once both loaded cells are known to have `ramp == None`, return their cliff-level equality directly. For a self-edge that equality is necessarily true. This is exactly the existing `(None,None)` outcome, reached before redundant profile lookup and optional-profile machinery. A RAMP flag without an actual direction retains the same semantics it has now; this change does not reinterpret map data.

All edges involving an actual ramp continue through the existing complete profile and bounded-run validation. Do not infer connectivity from matching height alone if either cell has a ramp direction. Do not special-case a particular map, unit type, clearance tier or numeric cliff level. Diagonal steps still require all four cardinal edges. Structures still count as underlying terrain where the current flags permit them, while callers continue to enforce physical/perceived occupancy. Invalid cliffs remain rejected before the fast branch.

Optionally reuse the already-loaded cell reference for a self-edge only if the implementation remains clearer; this is secondary and must be measured separately. Do not cache terrain profiles or connectivity across map changes, add unsafe indexing, alter map storage, bypass continuous segment checks, or weaken portal validation. A derived cache would add invalidation/state obligations without evidence that it is needed.

Document the branch with the required performance rationale: repeated flat-cell checks dominate portal work; inspect existing cell metadata before computing ramp profiles; retain the fully validated ramp path as the trade-off. Avoid blanket `inline(always)` or changing compiler optimization profiles to make the test pass.

## Stage 2: skip geometry for A* candidates that cannot improve

Only after stage1 tests/profiling, assess whether further headroom is necessary. Move the existing tentative g-score and generation-aware improvement check to immediately after an in-bounds neighbor index is available, before footprint/terrain/fog work. Diagonal/cardinal cost remains14/10 and saturating arithmetic remains unchanged. A neighbor already visited in the current generation with tentative g greater than or equal to its stored value can be skipped immediately. Stale generations cannot use old g-scores.

Proof obligation: the current algorithm performs no state mutation during passability/geometry checks and only writes predecessor/g/f/heap entries on a strict improvement or first visit. A non-improving candidate therefore cannot affect any current output regardless of its terrain legality. Moving this rejection preserves heap insertion/decrease order among every candidate capable of affecting the search; equal-cost predecessor tie behavior is unchanged. Keep every existing legality check for improving candidates. No closed-set change, heuristic change, neighbor reorder, new direct-path shortcut or funnel change is authorized.

If stage1 already provides sufficient measured headroom, stage2 can remain deferred rather than expanding the patch for an unmeasured gain. Record which stage was implemented and its separate evidence.

## Verification and acceptance

Preserve the baseline binary and raw profile before rebuilding. Record final diffs so the profile and timing refer to known source. Run all existing sim_map tests, especially actual map loading/ramp traversal, malformed and overlong ramps, diagonal corners, segment boundary reversal, known/unknown real structures, fog-aware routes, large footprints, smoothing and the unchanged deterministic checksum `0xadc6cb16b772f1a2`. Run sim_core pathfinding/movement integration to cover actual steering/snapping. Exact checksum or path changes are failures to investigate, not new expected values to bless.

Inspect current predicate boundary coverage before adding tests. Add only missing meaningful boundaries: ordinary same/different-level edges and self-edges, ground/structure/cliff combinations, and an adjacent valid or malformed actual ramp that must remain on the full path. Tests assert independent geometry/policy outcomes, not a second copy of the optimized implementation. Existing broad geometric and deterministic suites remain the primary semantic regression evidence. If stage2 is used, include repeated mixed-clearance/vision requests on one pathfinder so stale generations and equal-cost alternatives remain covered; do not expose test-only algorithm modes in production APIs.

Repeat Callgrind with the exact filtered test after the patch and compare instruction counts and the same inclusive sites. Its wall-clock assertion failure remains expected instrumentation behavior and is not a gate pass. A reduction in instructions corroborates algorithmic improvement independently of shared CPU scheduling; report it together with unchanged success count/checksum.

For native timing, coordinate a short no-build/no-capture interval among project agents. Do not kill or reprioritize unrelated user workloads. Run a declared fixed series of at least five unchanged filtered samples and preserve every result, including failures. Record available CPU/load context and min/median/max rather than selecting the fastest run. Also run the full two-test determinism binary with its normal test concurrency because the workspace runs the checksum and performance tests together. Do not serialize or skip the performance test to conceal that context. The250ms debug and100ms optimized thresholds remain exactly unchanged.

Finally run the required coordinated workspace tests, strict all-targets clippy and fmt. A shared-machine timing failure remains a failure and must be reported with evidence; no threshold relaxation or “probably contention” acceptance. Optimized/release sampling may supplement debug evidence if needed but cannot replace the existing debug gate. No visual change is intended, so screenshots are not semantic evidence for this patch.

## Ownership and non-goals

Own only `sim_map/src/traversal.rs`, optionally the narrow candidate-ordering hunk in `pathfinding.rs`, necessary focused tests and this report. Preserve existing traversal25 dirty work. No map schema/assets/editor changes, no gameplay balance, no output allocation refactor, no path caching, no diagonal/ramp exemptions and no checksum update. Client39/37 can proceed independently; coordinate CPU-intensive verification windows with their owners. Resume36 after this measured gate repair is accepted.

## Implementation and measured result

Implemented stage1 only: the documented no-ramp branch follows both terrain flag checks and precedes profile lookup. Added one independent public-API boundary fixture covering same/different-level ordinary terrain, actual placed-structure terrain versus physical occupancy, cliff precedence, self-edges, impassable cells and out-of-bounds cells. All existing ramp/malformed-profile logic is unchanged. No A* ordering, thresholds, compiler profiles, checksum reference or caches changed.

The original test executable is preserved as `/tmp/space-scuffle-r5-path-baseline-tests`. Callgrind instruction totals fell from1,415,834,826 to951,736,895 (32.78% fewer). Inclusive `ramp_profile` instructions fell from386,406,766 to22,823,346; portal construction fell from816,288,335 to510,943,041. These overlapping inclusive counts are attribution, not additive totals. Both instrumented processes exit101 at the unchanged timing assertion; neither is claimed as a passing timing test. Stage1 artifacts use `/tmp/space-scuffle-r5-path-stage1-callgrind*`.

The first fixed five native samples all passed at118.818,152.732,222.540,166.990 and128.545ms. A delayed coordination message showed that37's previously started build was still active, so these remain explicitly contended evidence in `/tmp/space-scuffle-r5-path-stage1-samples.log`; they are not presented as the coordinated interval.

After37 confirmed build completion, the second declared five-sample series ran with project agents holding builds/captures:85.733,87.159,86.741,88.842 and84.219ms. Minimum84.219ms, median86.741ms, maximum88.842ms. All five retained83 successful queries and passed visibility timing too (48.33–48.94ms). Host load average was17.74 on24 available CPUs; unrelated workloads were neither stopped nor reprioritized. Every sample/exit is preserved in `/tmp/space-scuffle-r5-path-stage1-coordinated-samples.log`. Lower concurrent load means wall-time gains cannot be attributed entirely to code; the independent instruction reduction confirms less work.

`cargo test -p sim_map` passed all83 tests, including normal concurrent checksum/performance tests and the unchanged reference checksum. Logs: `/tmp/space-scuffle-r5-traversal-map-tests.log`. Strict all-targets sim_map clippy and workspace format checking passed (`...-clippy.log`, `...-fmt.log`). Targeted actual movement/fog verification is recorded in `/tmp/space-scuffle-r5-traversal-core-tests.log`. The orchestrator must still run the required coordinated full workspace gate after the other active source changes stabilize; this report does not convert an earlier failed workspace gate into a pass.
