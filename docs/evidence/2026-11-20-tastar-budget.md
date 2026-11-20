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
