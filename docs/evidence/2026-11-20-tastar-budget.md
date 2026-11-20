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
