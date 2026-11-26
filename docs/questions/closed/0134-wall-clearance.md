# Q0134: How much room should walking units keep from walls?

- **Asked:** 2026-11-29
- **Blocking:** no
- **Default until answered:** (a)

## Question

In the showcase, units graze cliffs and the central block. Their discs touch the wall at clearance 0 for up to 77
ticks (3 s), and twice a seed or so a unit aims through a corner and has to back off (evidence
`docs/evidence/2026-11-29-wall-grazing.md`). Routes keep 0.05 world units beyond the body's radius at corners; any
crowd push of more than that puts the body on the wall, and nothing brings it back. StarCraft II's published
pathing is a radius funnel with no stated extra margin, so a tangent line round a corner is the game's too. What
looks different is that our units stay on the wall and bump into corners.

- (a) Fix the defects only: no corner cut through a "wedge" of rounding depth (B-240), re-check the current leg
  after a push (B-241), and follow the route line so a pushed unit returns to the 0.05 margin (B-242). Units still
  touch walls briefly in a crowd. No deviation from StarCraft II.
- (b) (a), plus a larger route clearance, a fraction of the radius such as r/4 (0.09 for a pulse bot), so that a
  visible gap normally stays. It is recorded as a minor deviation (wider berth round corners, slightly longer
  routes); narrow gaps keep their exact fit test, so no path is lost.
- (c) (a), plus steering that sees walls (Detour-style wall segments in avoidance, or a soft band in the contact
  solver), so crowds are kept off walls actively. Most like a modern crowd system, but it is a new rule with a
  tuning constant that no StarCraft II source gives, and it costs throughput.

Recommendation: (a) first, then decide (b) on the measured result.

## Answer

- **Option (a) now:** Fix the three defects only (`B-240` rounding-wedge corner cutting, `B-241` re-checking the current leg after a push, and `B-242` following the route line back to the `0.05` margin).

**Testing & follow-up guidance:** Do **not** use the showcase alone as the deciding factor for whether wider wall clearance (`(b)`) or active wall avoidance (`(c)`) is needed later — the showcase is only one example. Add dedicated tests where **large groups of units travel through narrow sections/chokepoints and around corners**, and judge wall grazing, corner bumping, and any follow-up clearance change against those broader crowd tests.

## Resolution

Followed as answered, on branch `wall-clearance-plan`:
- **(a) now.** Plan [`2026-11-29-wall-clearance`](../../plans/2026-11-29-wall-clearance.md) fixes the three defects
  only: WP1 (B-240), WP2 (B-241), WP3 (B-242). WP4, the larger margin of (b), is not built, and (c) stays out of
  scope; either needs a new owner decision.
- **Crowd tests.** WP0 added dedicated crowd scenes (`sim_fixtures::trace::crowd`, runner
  `crates/sim_core/examples/crowd_trace.rs`, commit `fa9dc3f3`): groups of 12, 24 and 48 units of three radii through
  chokepoints of 2, 3 and 5 cells, round a convex 90° corner, a hairpin round a thin wall's end and a 45° stepped
  wall, all measured by the route-clearance meter (wall grazing, episodes, reversals at walls, ticks lost, corner
  cutting). Their base figures under the arms S, R and L are in evidence
  [`2026-11-29-wall-clearance`](../../evidence/2026-11-29-wall-clearance.md) WP0.
- **Judged on both.** The plan's Amendment 1 makes these crowd scenes, together with the showcase seeds, the basis
  of WP1 to WP3's acceptance and of any later (b) or (c) decision; the showcase alone never decides.
