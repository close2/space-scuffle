# Round52 verification: terrain-connected cluster arrival

Status: implementation and focused CPU verification complete; owned source frozen for the next coordinated workspace gate. The preceding unchanged10c gate passed before this implementation. This report does not claim the new combined workspace gate or visual acceptance. No editor, assets, renderer, map implementation, schema or authoritative field changed.

## Result and retained policy

Ground Move/AttackMove cluster completion now requires both actor→settled witness and witness→goal to pass the shared exact terrain-segment predicate. Failed local witnesses leave the order available for existing navigation. Air retains the geometric policy. The predicate deliberately accepts underlying structure terrain: this is arrival near a settled body, not permission to move, spawn or teleport through an occupied cell. Fog/structure knowledge is neither consulted nor mutated by the new query.

The single checked helper preserves both public geometry-only APIs, exact numeric thresholds, self-exclusion and nonpositive-radius error order. After the first successful witness it skips further geometry/map callbacks while still validating every later nonexcluded radius. All new terrain callbacks are immutable map reads; existing per-actor proposal commits and50 corruption behavior remain intact.

Owned product files: `crates/sim_core/src/steering.rs`, the cluster-arrival call in `crates/sim_core/src/movement/locomotion.rs`, and `crates/sim_core/tests/cluster_arrival_tests.rs`. Approval/fixture contracts remain in [plan52](../plans/52-round10-terrain-aware-cluster-arrival.md).

## Reproduction replay

The original external harnesses were copied with exactly one change each: their output directories now name new evidence. Their seven1024-tick cases and two16-tick same-side-witness cases retain original seed, map, catalog selection, generic numeric fixture, commands and horizons. Each complete trace was independently repeated by the harness and compared exactly. No prior failure artifact was overwritten.

| Retained case | Before | After |
| --- | --- | --- |
|Ground group Move/AttackMove, fog off/on|Order disappears at1; no movement through1024|First motion1; first empty order77 after the ramp route|
|Same-side witness with goal across cliff, both orders|Order disappears at1|Motion begins1; order remains active through the original16-tick window|
|Solo Move/AttackMove|Exact goal75|Complete trace unchanged, including checksums; exact goal75|
|Air and flat-ground groups|Immediate geometric settlement1|Complete trace unchanged, including checksums|

Corrected group final positions intentionally need not coincide with the occupied goal. Real collision/arrival policy leaves a valid destination-side neighborhood. The loaded-map integration tests independently require legal every-tick terrain motion, eventual local destination connectivity and completion. Their separate256-tick paired-world matrix covers reverse direction, both disconnected legs and fog variants; it does not replace or shorten the retained1024-tick external comparison.

Evidence:

- `/tmp/space-scuffle-r11-round52-evidence01/` and `...evidence02/`: full JSONL traces, scenario, exact adapted harness, Cargo manifest/lock, preserved executable and SHA256.
- `/tmp/space-scuffle-r11-round52-harness/adaptation.json`: original/copied source hashes and sole output-path substitutions.
- `/tmp/space-scuffle-r11-arrival-replay-summary.json`: all nine trace summaries and exact unchanged-control comparisons. Every tick has matching recomputed/last full checksum and no diagnostics.
- `/tmp/space-scuffle-r11-arrival-replay01.log` and `...replay02.log`: both exit0; every repeat retained.
- `/tmp/space-scuffle-r11-arrival-source-final.json`:104 current simulation/map/catalog input hashes. The earlier inventory is retained; its only subsequent change was the strengthened foundation knowledge-mask test. This is verification provenance, not a frozen prebuild/compiler or performance claim.

## Focused checks

Four private helper tests pass: eligibility/callback work bound, later invalid radius despite success or moving neighbor, valid real ramp versus forbidden side with reversed candidates/error order, and diagonal-cliff/malformed-ramp rejection plus flat positive control. Existing exact raw numeric-boundary tests remain in the checked movement suite.

Six new integration tests pass across the focused runs: authored cliff replay matrix, flat/Air single-order pop, disconnected queued followup retention, real registered foundation, dead/cargo/cross-layer absence, and eight-actor ramp movement. The last test establishes every actor makes actual progress, every displacement remains terrain-connected, no diagnostics and exact paired replay; it does **not** claim all eight actors arrived or that formations are optimal. The foundation test exercises actual initially unknown and later normally revealed masks, requires arrival queries to preserve both, and retains actual placement/position/supply invariants.

The existing focused suite passes92 tests across seven harnesses:29 library tests (including the four new helper tests),8 AttackMove,17 checked movement,16 movement-layer,3 ramp,4 snapshot-output and15 transport tests. Log: `/tmp/space-scuffle-r11-arrival-regression01.log`. All failed counts and ignored counts are zero.

Strict `cargo clippy -p sim_core -p sim_map -p sim_types --all-targets -- -D warnings` passes (`...arrival-clippy03.log`). Scoped rustfmt check of the three owned files passes (`...arrival-owned-fmt.log`). Workspace fmt checks01/02 report only other lanes' in-progress mist witness/shell observer files; they are retained failures, not a claimed workspace-format pass. Owners were informed and their files were not edited. Root coordinates the final workspace tests/Clippy/fmt after all lanes freeze.

The first integration compile attempt exposed a fixture Tick wrapper omission; it was repaired and the failure log retained. Initial strict checking found three diagnostics-empty assertions needing the repository's richer equality style; those were corrected and final checks passed. None required weakening product behavior or test assertions. Detailed focused logs use `/tmp/space-scuffle-r11-arrival-{unit03,integration02,participation01,crowd01,foundation02,clippy03}.log`.

## Limits

This fixes the demonstrated terrain-disconnected witness. It does not redesign group destinations, allegiance, occupancy-based settlement, blocked-goal timeouts, pathfinding, formations or congestion performance. It makes no SC2-parity claim. The existing50 performance acceptance remains inconclusive and unchanged. No native image or artistic-quality claim is derived from these simulation tests.

Root independently audited all seven1024-tick JSON traces and exact baseline metadata/scenario: `/tmp/space-scuffle-r11-round52-evidence01/root-independent-audit.json` with script `/tmp/space-scuffle-r11-arrival-independent-audit.py`. Ground groups reach maxY8.5 along the alternate route and settle0.471911 world units from the goal at77; all four control traces remain exactly equal to baseline. That JSON audit verifies recorded state/checksums, not terrain legality or supply through a floating-point reconstruction; those remain actual Rust test/harness assertions.
