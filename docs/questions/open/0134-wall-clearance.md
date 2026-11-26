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

