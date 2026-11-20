# Evidence: TA*'s Channel Under Partial Knowledge (B-149)

Measurements for plan [`2026-11-20-tastar-budget`](../plans/2026-11-20-tastar-budget.md). Each work package appends
its section. The plan's prototype figures (its §4) were taken on `e58f1af1`; the figures here are taken on the
implementation tree and its base, named in each section. The measurement patch (switches and probes) and its scripts
live under `~/scratch/tab/`, never on the branch.

## WP0: records

- `DESIGN.md` §4: B-149 restated with the plan's diagnosis (equal bounds across an unknown footprint's lattice; the
  anytime budget and the visit cap; S11's own statement of the problem) and pointed at this plan; open.
- The visit cap's residual (plan D-4, drafted as B-150) is registered as **B-154**: B-150 to B-153 were registered by
  plan `2026-11-20-resource-footprints` (`6549d1b3`, on its branch), checked with `git log --all -S"B-15"`.
- Pointer amendments: plan `2026-11-09-sc2-pathfinding` Amendment 8 (D-5's budget replaced by S11's rule) and plan
  `2026-11-10-group-routing` Amendment 2 (its D-5 "unchanged anytime budget").

## WP1: failing tests first

The corpus's generator moved into `crates/sim_map/tests/support/layouts.rs` (the draw order of the scratch
`zz_pkf_corpus.rs`, with its contour shapes and per-cell switch), and `partial_knowledge_routes_agree_with_the_oracle`
uses it: on `main`'s search it still reports 485 of 489 within 1 %, worst 1.1343, as before the move. Run with
`--include-ignored` on the WP1 tree (`main`'s search at `e58f1af1`), release:

| Test | Lands with | On `main` |
|---|---|---|
| `the_straight_channel_across_an_unknown_footprint_is_found` (T1, 25 q 11) | WP2 | red: 57.104, turns at the free (48, 38), (49, 41), (49, 43) |
| `a_capped_state_keeps_the_straight_channel` (T2, 92 q 13) | WP2 | red: 50.904 |
| `a_wedged_start_beside_unknown_cells_takes_the_short_channel` (T3, per-cell squares 19 q 8) | WP2 | red: 26.195 |
| `partial_knowledge_routes_agree_with_the_oracle`, tightened (worst ≤ `MESH_RATIO_BOUND`) | WP2 | red: worst 1.1343 |
| `the_anytime_loop_runs_to_the_incumbent` (T7, 113 q 1) | WP3 | red: 76.571 |
| `a_legal_channel_is_sought_before_the_midpoints` (per-cell contours 58 q 13) | WP4 | red: falls back |
| `an_illegal_first_pull_does_not_end_the_search` (per-cell contours 92 q 12) | un-ignored | green: 69.905 |

Each case first checks that the regenerated request is the plan's (endpoints and radius).

**Deviations from the plan's WP1 table.**
- `an_illegal_first_pull_does_not_end_the_search` asserts "no fallback, and at most 70.863 × 1.001", not "within
  1.001 of 70.863": the plan has it green on `main`, whose route is 69.905, shorter than the legal-only search's
  70.863, so a two-sided bound could not be green there. Red on the WP2 and WP3 trees (a fallback) as planned.
- The tightened oracle test is ignored as a whole until WP2 (it gains its assertion here), so its earlier assertions
  do not run on the WP1 tree alone; the branch lands as a whole.
- The four unit tests of `search.rs` (`a_start_on_the_first_portal_line_does_not_inflate_the_bound`,
  `a_clamped_free_end_bounds_nothing`, `channel_bounds_never_exceed_a_legal_pull`, `the_search_ends_on_the_incumbent`)
  test functions the plan adds in WP2 and WP3, so they land with that code.

## WP2: D-1, the channel-geodesic bound

**Method.** Three scratch trees, all removed after measurement: `tab-base` (`main` `e58f1af1` with the bench's
`BENCH_ONLY` switch and the corpus test), `tab-cand` (the WP2 tree with the same two additions; cost and throughput)
and `tab-measure` (the WP2 tree with the measurement patch `~/scratch/tab/impl/apply_measure.py`: a switch per rule,
`TAB_OFF_D1`, the cap `TAB_VISITS`, the admissibility check `TAB_STAT`, the partial-knowledge funnel's `PKF_LOG` and
`PKF_PROBE` probes, and the scratch corpus tests of the plan's §3.1). Corpora and battles come from `tab-measure`:
base is that binary with `TAB_OFF_D1=1`. **Control:** with D-1 off, every corpus file, the golden-corpus logs, the
contour corpus and the whole battle set are byte-identical to the prototype's base (`~/scratch/tab/v-base`,
`bat-base`, `glog-base-*`, `cr-base.txt`), which reproduced `main`; the three pins pass with D-1 off.

**The bound as built.** The point funnel is advanced per node and re-read from the ancestors when its apex moves
(the plan's incremental form); unit test `channel_bounds_never_exceed_a_legal_pull` checks on every node of the
four test-size corpora and the all-known one (3,247 channels with a legal pull) that it equals the funnel
recomputed from the start, and that no node bounds a legal pull from above. On the oracle corpus the routes are
request for request those of the prototype's recomputed form (`TAB_FG=4`, budget 256, `~/scratch/tab/o-TAB_FG_4.txt`).

| Corpus | Base | D-1 (budget 256) |
|---|---|---|
| Partial-knowledge oracle (1,820 from fitting starts) | 1,804 within 1.01, worst 1.1343, none shorter | **1,814**, worst **1.0193**, none shorter |
| All 1,964 routed requests of that corpus | — | 66 change: 58 shorter, 8 longer (7 fitting, at most +0.649 %, 137 q 7; the wedged 81 q 10 +0.766 %) |
| All-known oracle | 1,743 / 1,744, worst 1.0193 | identical; one wedged route of 1,889 changes, shorter |
| Fallbacks from fitting starts (whole squares, whole contours, per-cell squares, per-cell contours) | 0, 1, 1, 5 | 0, 1, 1, 5: per-cell contours 104 q 3 becomes legal and **92 q 12 falls back** (WP2's stated exception until WP4) |
| Stress search | 10 fallbacks | 3, all among the 10 |
| All-known corpora (150, 1,000, 2,000 flush) | 0 / 0 / 0 | 0 / 0 / 0 |
| Layouts, routes longer (whole contours, per-cell squares, per-cell contours) | — | 12 (at most +1.225 %, 128 q 5), 5 (at most +1.52 %, 92 q 12), 15 (at most +4.305 %, 140 q 8, T6) |
| `PATH_GOLDEN` corpus (174 routed) | — | 12 change: 11 shorter, 1 longer (+0.24 %, fitting); fallbacks 3 → 3 (wedged) |
| `CONTOUR_PATH_GOLDEN` corpus (258 routed) | — | 16 change: 13 shorter, 2 longer (+1.48 % fitting r 0.5, +4.66 % wedged r 1.0), 1 wedged fallback → legal |
| `CONTOUR_ROUTE_CHECKSUM` corpus (1,000) | — | 1 changes, shorter: (61.75, 19.5) → x 2.0, r 0.5, 62.090 → 61.497 |
| Admissibility (`TAB_STAT`, every legal pull of the five corpora) | 1 | 1, the same: layout 74 q 12, a wedged start, the start node (`g` 0) whose `h`, the straight distance, exceeds the route's Q32.32 length by rounding; not a term of D-1, so no pad (D-6) |
| Path-search cost (instructions, three corpora; `tab-base` against `tab-cand`) | 12.317 + 14.421 + 19.395 = **46.132 G** | 12.438 + 14.180 + 15.955 = **42.573 G (−7.7 %)** |

The 32 longer routes of the four layout corpora were run again with the cap at 64 (diagnostic, D-1 on, budget 256):
21 come back to at most their base length; the other 11 (oracle 128 q 6, 137 q 7; whole contours 45 q 8, 113 q 1;
per-cell squares 83 q 7, 115 q 14; per-cell contours 43 q 0, 46 q 0, 75 q 14, 81 q 5, 83 q 7) do not, and 113 q 1 is
T7, which the budget cuts; they are traced again on the WP3 tree.

**Battles** (`measure.sh`, same binary): the matrix (1,160 runs), battle 33 (64 seeds), battle 31 (`--seeds 100`) and
the showcase to tick 3,000 on the default seed and seeds 1 to 8 are **byte-identical** to the control. `sim_core`'s
`golden_replay_tests`, `fog_of_war_pathfinding_tests` and `wall_contact_tests` pass in release.

**Throughput** (instructions retired, each scenario alone through `BENCH_ONLY`, under `flock ~/scratch/gate.lock`,
`tab-base` against `tab-cand`):

| Scenario | Base (G) | D-1 (G) | Change |
|---|---|---|---|
| 64×64, 0 units | 1.85985 | 1.86097 | +0.06 % |
| 64×64, 100 units | 6.20175 | 6.20287 | +0.02 % |
| 64×64, 400 units | 30.90531 | 30.90643 | +0.004 % |
| 128×128, 400 units | 31.22504 | 31.22959 | +0.01 % |
| 256×256, 0 units | 29.12343 | 29.14178 | +0.06 % |
| Showcase canyon, 400 units | 53.76819 | 53.95638 | **+0.35 %** |
| 64×64, 67 casters, 400 units | 48.06513 | 48.06625 | +0.002 % |

**Pins.** `PATH_GOLDEN` `0x4062_b0b6_83dd_1afb` → `0xf322_b17b_9841_7b5e`, `CONTOUR_PATH_GOLDEN`
`0x1da8_6365_bcbd_3b37` → `0x30cd_bf6a_3de1_a900`, `CONTOUR_ROUTE_CHECKSUM` `0x7ac0_f0b9_f8b4_2057` →
`0xec9a_71b8_655a_01c2`: D-1 alone already gives the plan's would-be values of the adopted design (§4.6). Re-recorded
here, with proof headers, since they are identical in native debug, native release, `wasm32-wasip1` release and
`wasm32-wasip1` debug (runner `wasmtime --dir=<worktree>`); every other test of both binaries passes on all four.

**Tests.** T1, T2, T3 and the tightened oracle test are un-ignored (oracle test: 489 of 489 within 1 %, worst
1.0054; `mesh_route_lengths_are_within_one_percent_of_the_oracle` 522 / 523, worst 1.0220, and
`exact_fit_routes_agree_with_the_oracle` 293 routes, worst 1.0443, 153 of 159, both unchanged);
`an_illegal_first_pull_does_not_end_the_search` is ignored ("B-149, WP4"). New unit tests:
`a_start_on_the_first_portal_line_does_not_inflate_the_bound` (T4's channel: every node's `g + h` at most 42.975;
Mononen's strict crossing tests give 48.88 on the same portals), `a_clamped_free_end_bounds_nothing` (T5),
`channel_bounds_never_exceed_a_legal_pull`.

**Deviations.**
- T5: the plan gives 34.47 at depth 3 for a bound that treats (26, 26) as a side. On the route's channel, read from
  the search, the point bound peaks at 33.87 with the end as a side and 33.84 without, both below 34.324. The unit
  test therefore checks what holds: (26, 26) is a clampable end of the channel, the channel bound stays below the
  route, and no node of any legal channel of that search exceeds its pull.
- The clampable stamps are a `Vec<u32>` holding `2 × generation + answer`, as D-1's rationale asks; the node carries
  three points, a length and three depths (positions, not vertex indices: the start is not a vertex).

**Stop rules (D-1's figures, WP2's exception):** winners, survivor ranges, give-ups and reversals unchanged (battles
byte-identical); control reproduces `main`; pins re-recorded on four builds, every other pin unmoved; throughput at most
+0.35 %; path cost −7.7 %; oracle 1,814 / 1.0193 and 1,743 / 1,744, none shorter; longer routes as the prototype's
D-1 row (7 fitting, at most +0.65 %); fallbacks: only 92 q 12, the stated exception; admissibility as base. **Held.**

## WP3: D-2, S11's termination

`NAV_TA_STAR_EXTRA_EXPANSIONS` is removed (its only reader was `search.rs`; no other crate, test or guide names it);
the loop ends when the least open `f` is at least the incumbent's length. The work bound of D-2 is stated in the
module's invariants and `ta_star`'s doc: `3 × NAV_TA_STAR_TRIANGLE_VISITS × T` expansions per pass; for a
lane-restricted search per word, the words being at most `(|target| + 1) · (2H)^K` (plan `2026-11-10-group-routing`
D-5, the plan's **verify**). The open list is left as the loop ends (`reset` clears it at the next search), so the unit
test can read where the search stopped.

**Method.** As WP2; base is the measurement binary with `TAB_OFF_D1=1 TAB_OFF_D2=1` (the budget restored). Control:
every corpus file, the contour corpus, the golden-corpus logs and the battle set are byte-identical to WP2's control,
hence to `main`. On the oracle corpus the routes are request for request the prototype's D-1 + D-2 recomputed form
(`~/scratch/tab/o-TAB_FG_4_TAB_EXTRA_0.txt`).

| Corpus | Base | D-1 + D-2 |
|---|---|---|
| Partial-knowledge oracle | 1,804 / 1,820, worst 1.1343 | **1,815**, worst **1.0193**, none shorter |
| All 1,964 routed requests | — | 69 change: 63 shorter, 6 longer (5 fitting at most +0.24 %, 59 q 14; the wedged 81 q 10 +0.77 %) |
| All-known oracle | 1,743 / 1,744 | identical; one wedged route changes, shorter |
| Fallbacks from fitting starts (four layout corpora) | 0, 1, 1, 5 | 0, 1, 1, 5 (per-cell contours: 104 q 3 legal, **92 q 12** falls back, the exception until WP4) |
| Stress search | 10 | 1 (among the 10) |
| All-known corpora | 0 / 0 / 0 | 0 / 0 / 0 |
| Layouts, routes longer (whole contours, per-cell squares, per-cell contours) | — | 10 (at most +1.225 %), 5 (at most +1.52 %), 15 (at most +4.305 %, T6) |
| Golden corpora, `CONTOUR_ROUTE_CHECKSUM` corpus | — | as WP2, request for request; the three pins unchanged from WP2 |
| Admissibility (`TAB_STAT`) | 1 | 1, base's start-node rounding (WP2) |
| Path-search cost | 46.132 G | **45.010 G (−2.4 %)**; the prototype's D-1 + D-2 45.11 G |

**Longer routes traced** (rule 8; with the cap at 64, D-1 and D-2 on): 32 of the 35 longer routes of the four layout
corpora come back to at most their base length. The other three are named:
- per-cell contours 46 q 0 (+0.053 %): the visit cap; at 255 visits the route is 65.817, below base's 65.854;
- per-cell squares 115 q 14 (+0.325 %) and per-cell contours 43 q 0 (+0.338 %): base's route is a retry's repair of
  an illegal inscribed pull (115 q 14 the zero-room retry, 43 q 0 the pinned retry: first pull 46.776, illegal,
  pinned 46.362), while TA* ranks channels by their inscribed pulls; with D-1 and D-2 it finds a channel whose
  inscribed pull is shorter than base's (46.519 for 43 q 0) and legal, so no retry runs. Unchanged at 255 visits.
  This is the plan's §9 item "an illegal pull as incumbent in the normal search", kept.

**Battles**: byte-identical to the control (matrix, battle 33, battle 31 on 100 seeds, showcase on nine seeds).

**Throughput** (instructions, `tab-cand` at WP3 against `tab-base`): 64×64 0 units +0.06 %, 100 units +0.02 %, 400
units +0.004 %, 128×128 +0.01 %, 256×256 +0.06 %, showcase canyon 53.76819 → 53.93817 G **+0.32 %**, casters
+0.002 %.

**Tests.** `the_anytime_loop_runs_to_the_incumbent` (T7: 75.372) is un-ignored. Unit test
`the_search_ends_on_the_incumbent` checks that the search ends with no open `f` below the route on T1 and T7.
**Deviation:** the plan names T1 alone; with D-1 in place T1 ends on its own after 29 expansions, so the test is
green with the budget back (`TAB_OFF_D2=1`), and T7 (red with the budget back: 185 open channels below 76.571) is
added to hold D-2.

**Stop rules (WP3's figures, the 92 q 12 exception):** held, as WP2.

## WP4: D-3, a legal-only search before the fallback

`Query` gains `legal_only`; `MeshSearch::legal_only_retry` runs TA* once more with it after the zero-room retry has
failed, takes a channel as the incumbent only when its inscribed pull is legal, and restores the first channel's
portals for the midpoint fallback when it finds none. Its expansions are not added to `MeshRoute::expansions`
(as the prototype); `MeshSearch::fallbacks` counts only real fallbacks. The module doc states the order of the attempts.

**Method.** As WP2 and WP3; base is the measurement binary with `TAB_OFF_D1=1 TAB_OFF_D2=1 TAB_OFF_D3=1`.
**Controls:** all three off reproduces WP2's control (hence `main`) byte for byte in every corpus, the contour
corpus, the golden logs and the battle set; D-3 off reproduces WP3's corpora byte for byte; D-1 off (D-2 + D-3) gives
1,809 / 1,820, worst 1.0549 (the plan's "budget unbounded, cap 4" row); D-2 off (D-1 + D-3) gives 1,814 / 1.0193.
With all three on, every corpus file and the contour corpus are request for request the prototype's adopted design
(`~/scratch/tab/v-final3`, `cr-final3.txt`), and the golden-corpus logs equal its `glog-final3-*`.

| Corpus | Base | D-1 + D-2 + D-3 |
|---|---|---|
| Partial-knowledge oracle | 1,804 / 1,820, worst 1.1343 | **1,815**, worst **1.0193**, none shorter |
| All-known oracle | 1,743 / 1,744 | identical; one wedged route shorter |
| Fallbacks from fitting starts (whole squares, whole contours, per-cell squares, per-cell contours) | 0, 1, 1, 5 | **0, 1, 1, 2**; no new fallback: per-cell contours 58 q 13, 104 q 3 and 104 q 15 become legal; left: whole contours 59 q 9, per-cell 8 q 2 (both kinds), per-cell contours 59 q 9 (class N) |
| 92 q 12 (per-cell contours) | 69.905 | 70.863 (+1.37 %), legal, by the legal-only search |
| Stress search | 10 | **0** |
| All-known corpora | 0 / 0 / 0 | 0 / 0 / 0 |
| Routes longer (oracle, whole contours, per-cell squares, per-cell contours) | — | 6 (5 fitting ≤ +0.24 %, wedged 81 q 10 +0.77 %), 10 (≤ +1.225 %), 5 (≤ +1.52 %), 16 (≤ +4.305 %, T6; 92 q 12 among them) |
| Golden corpora, contour corpus, pins | — | as WP2; the three pins unchanged, passing in native debug and release and `wasm32-wasip1` release and debug |
| Admissibility (`TAB_STAT`) | 1 | 1 (base's start-node rounding) |
| Path-search cost | 46.132 G | **46.091 G (−0.1 %)**; D-3's searches cost 1.08 G over WP3, on the corpora's would-be fallbacks |

**The plan's count of 15 longer per-cell contour routes** is 16 by this count, with the plan's maximum (+4.30 %);
the same count on the prototype's own files (`v-base` against `v-final3`) gives 16, and the routes are identical
to the prototype's, so the difference is in the plan's tally, not in the routes.

**Longer routes traced** (cap 64, all rules on): all but four come back to at most base. 46 q 0 comes back at 255
visits (the cap); 115 q 14 and 43 q 0 are base's retry repairs (WP3); 92 q 12 is D-3's route where D-1 + D-2 alone
fall back (its first channel's illegal pull, 65.377, ends the search). All four are the plan's §9 "illegal pull as
incumbent" item or the cap (B-154).

**Battles**: byte-identical to the control (matrix, battle 33, battle 31 on 100 seeds, showcase on nine seeds).
`golden_replay_tests` (native release and `wasm32-wasip1` release), `fog_of_war_pathfinding_tests` and
`wall_contact_tests` pass.

**Throughput** (instructions, `tab-cand` at WP4 against `tab-base`):

| Scenario | Base (G) | Adopted (G) | Change |
|---|---|---|---|
| 64×64, 0 units | 1.85985 | 1.86097 | +0.06 % |
| 64×64, 100 units | 6.20175 | 6.20288 | +0.02 % |
| 64×64, 400 units | 30.90531 | 30.90644 | +0.004 % |
| 128×128, 400 units | 31.22504 | 31.22959 | +0.01 % |
| 256×256, 0 units | 29.12343 | 29.14178 | +0.06 % |
| Showcase canyon, 400 units | 53.76819 | 53.93064 | **+0.30 %** |
| 64×64, 67 casters, 400 units | 48.06513 | 48.06625 | +0.002 % |
| Whole benchmark | 201.149 | 201.339 | +0.09 % |

**Tests.** `a_legal_channel_is_sought_before_the_midpoints` (58 q 13: 50.403) and
`an_illegal_first_pull_does_not_end_the_search` (92 q 12: 70.863) are un-ignored.

**Stop rules (rule 9 without exception):** winners, survivors, give-ups, reversals: battles byte-identical; controls
reproduce `main`; pins identical on four builds, `VALIDATION_GOLDEN`, `DECODE_GOLDEN`, `SHOWCASE_MESH_DIGEST`,
`EXPECTED_CHECKSUM` and the cross-platform checksum unmoved; throughput at most +0.30 %, path cost −0.1 %; oracle
1,815 / 1.0193 and 1,743 / 1,744, none shorter; longer routes at the prototype's counts and amounts, each traced;
fallbacks 0, 1, 1, 2 from fitting starts, none new, stress 0, all-known 0; admissibility as base. **Held.**

## WP5: tests and pins

- The three pins were re-recorded in WP2 (D-1 alone gives the adopted values) and stayed through WP3 and WP4, so WP5
  re-records none; their headers name the changed requests by class and the old values the control reproduces.
- `partial_knowledge_routes_agree_with_the_oracle_per_cell`, the per-cell half: the same 40 layouts with each cell
  known or not; the oracle runs on the arena with every known cell as a one-cell structure; 457 routes from fitting
  starts, none shorter than the oracle by more than 10⁻³ (worst ratio 1.0309, reported only).
  **Deviation:** the plan asks for "no fallback from a fitting start"; per-cell layout 8 q 2 is inside the 40
  layouts and is the partial-knowledge funnel's class-N residual that this plan keeps (§4.2, §9), so the test allows
  exactly that one, by name. A separate test rather than a half of the existing one, so each prints its own figures.
- `crates/gate/suites.ron` needs no change: the new tests are in existing targets (C22).

## WP6: final figures and bookkeeping

The final tree's figures are WP4's (WP5 changed tests only). Against the plan's prototype (§4): every corpus, the
contour corpus and the golden-corpus logs are request for request the prototype's adopted design; the three pins
are its would-be values; path search −0.1 % (prototype −0.5 % against its own base, 46.47 G; this base measured
46.13 G); showcase scenario +0.30 % instructions (prototype +0.50 %).

**Wall clock** (`tab-cand` at WP4, one run under `flock ~/scratch/gate.lock` on the shared host, median of three):
64×64 0 units 3,249 ticks/s, 100 units 778, 400 units 140; 128×128 178; 256×256 201; showcase canyon 107; casters
204. Every scenario is above 25 ticks/s; a quiet-host run stays an open confirmation item.

**Records.** `DESIGN.md` §4: B-149 fixed with the figures; B-154 keeps the visit cap's residual (4 oracle routes above
1.01, at most 1.0185; per-cell contours 140 q 8 +4.3 %, back at 64 visits; 46 q 0 back at 255); B-146's row notes
that its accepted residual 58 q 13 and its stress residual are gone (D-3). `docs/plans/README.md`: the plan is Done.
The showcase did not move, so no storyboard check is needed.

The scratch trees `tab-base`, `tab-cand` and `tab-measure` and their target directories are removed; the measurement
patch, scripts and results stay under `~/scratch/tab/impl/`.
