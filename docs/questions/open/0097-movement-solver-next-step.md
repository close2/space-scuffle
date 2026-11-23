# Q0097: The new movement solver has stopped under its own rules. Switch over, iterate once more, or stop?

- **Asked:** 2026-11-23
- **Blocking:** no. Nothing switches over without your answer.
- **Default until answered:** the cost-only step that changes no behaviour (a cache of the ring intervals, plan
  `docs/plans/2026-11-20-contact-solver.md` Amendment 11 step 6), then option **(a)** below. The cache helps under
  (a) and (b) alike.

## Background

The new movement solver you approved in Q0092 is in `main`, switched off (`contact.pipeline: Legacy` is the
default). Plan Amendments 11 to 13 fixed two defects: the ring geometry and the enemy dead band. Three further
rescue attempts were each stopped by the plan's rule that a step may not make any passing figure fail. These were
a detour round the crowd, a contact-threat fix and a wavefront. By the plan's own limits the trade-off now comes to
you.

All three failures share one cause. A unit picks the side on which it passes a neighbour, and that choice switches
on and off as it touches and leaves bodies in a crowd. Every rescue that brings more units into contact with the
front rank makes it switch more.

**Today's movement (Legacy) against the solver as it stands** (development seeds 1–1,000; evidence §13.6). Of the
33 pre-registered pass/fail rows, Legacy passes **13** and the solver passes **22**.

| Measure | Legacy | Solver |
|---|---:|---:|
| Back-and-forth steps at gaps / corners / corridors (M1) | 17.7 / 20.9 / 8.8 | 1.1 / 0.5 / 0.0 |
| Give-ups at gaps / crossings / corridors | 454 / 152 / 275 | 46 / 2 / 20 |
| Gap scenes completed | 80.5 % | 97.8 % |
| Friends over 5 % overlap while moving (gap) | 14.7 % | 1.6 % |
| Mirror battle fairness (blue's share; 0.5 is fair) | 0.438, unfair | 0.554, fair |
| **Give-ups when attackers fill an arc round a target** | **25** | **305** |
| **Give-ups at a corner** | **0** | **7** (99.7 % complete) |
| Arc fill (share of ring places taken) | 0.980 | 0.967 |

The solver is worse than Legacy, beyond noise, only on the two bold rows, plus small rises within their limits. It
also fills two-wide corridors worse, which is Q0095.

**Cost** (Q0093 (c): at most +60 % instructions and at least 50 ticks per second):

| Scenario | Solver against Legacy | Ticks per second | Bar |
|---|---:|---:|---|
| 400 units, 64 × 64, armies meet | +65 % | 98 | fails by 5 points |
| 128 × 128, armies apart | +7 % | 171 | holds |
| Showcase canyon | +54 % | 78 | holds |

The cache in the default removes about 0.95 G of the 6.7 G excess on the 64 × 64 scenario. That alone should
bring it under +60 %. It will be measured, not assumed.

## Options

- **(a) One more design round on the side rule (the default after the cache).** Replace the solver's one-partner
  side rule with a published method that keeps a passing side through contact. The candidate is ORCA (van den Berg
  et al. 2011), whose half-responsibility and colliding case address exactly this mechanism. It would go through a
  new independent review, a fresh acceptance range and the same stop rules. Risk: ORCA has known corner deadlocks
  (survey VO-4), and it is more weeks of work.
- **(b) Switch over now, after the cache.** Run the held-out seeds once (both pipelines side by side) and make the
  solver the default if the held-out figures confirm the table above. Then delete the old movement code. You would
  sanction the remaining failures as recorded deviations or open bugs: arc and corner give-ups, the rows where both
  pipelines fail, and the corridor miss. Each is fixed later like any other bug. Units would move noticeably better
  almost everywhere, with more units giving up on reaching a crowded target's ring.
- **(c) Stop the redesign.** Keep today's movement and delete the dormant solver. The 13-of-33 pipeline stays, and
  its known back-and-forth bugs stay open.

My recommendation is **(b)**. On the pre-registered rows the solver passes 22 of 33 against today's 13. It fails
only two rows that today's movement passes: corner give-ups, and one arc scene over the friend-overlap maximum. On
one row that both fail, arc give-ups, it is clearly worse. The plan's absolute thresholds were set as goals for the final design, and today's movement
also fails most of them. Option (a) can still follow (b) as ordinary bug work on the new default.

## Answer
