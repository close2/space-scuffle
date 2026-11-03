# Q0006: What should happen when a player orders a move to an unreachable spot?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** keep walking to the nearest reachable point

## Question

Repro r09 in the remediation plan expects such an order to finish quickly. Units instead walk to the nearest
reachable point, which takes about 545 ticks at speed 0.09, so 20 of 20 still hold an order at tick 500 and the
repro still fails. Per-tick cost fell from 15.67 ms to 0.13 ms, so the performance problem itself is fixed.

Stopping at once would pass the repro, but clicking behind a wall would then do nothing. The recommendation is to
keep the walking and rewrite the repro's expectation.

## Answer

Owner (2026-11-03): Agree (keep the walking to the nearest reachable point and rewrite the repro's expectation).

## Resolution

2026-11-03, commit `ab53c32`. Units keep walking to the nearest reachable point. The scratch repro crate no longer
exists, so its expectation was rewritten in the remediation plan's Appendix A: r09 now expects every unit to have no
order left after 900 ticks, on the near side of the wall, within 6 cells of the nearest reachable point. The check
that runs is `unreachable_order_reports_retargets_and_terminates` in
`crates/sim_core/tests/movement_contact_tests.rs`, which already asserted this. `DESIGN.md` §4 cites the owner's
choice.
