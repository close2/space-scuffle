# Q0148: Units re-plan before cutting a ramp corner, but a neighbour is bumped once. Ship that fix with mixed figures?

- **Asked:** 2026-11-28 (plan `2026-12-03-movement-work-on-solver`, WP-W2)
- **Blocking:** no
- **Default until answered:** (b). The fix is rebuilt and measured on today's movement. It merges only if every
  original check holds. Otherwise it stays parked with its figures, and bug B-241 stays open.

## Background

This is your 2026-11-29 ramp observation: a beetle pushed off its line by its own crowd aims straight through the
corner at the top of the west ramp and backs off along the wall (B-241). The fix is two lines. The unit re-checks the
leg to its next waypoint with the same test its steps use, and re-plans when that leg would cut a corner (plan
`2026-11-29-wall-clearance`, WP2).

When it was built (evidence `docs/evidence/2026-11-29-wall-clearance.md`, WP2 section):

**It worked:**
- no unit kept a corner-cutting leg longer than 19 ticks (before: up to 122);
- the beetle's reversal at the ramp is gone;
- every showcase battle ends sooner, with the same winner.

**It moved:**
- the central block got better;
- the west ramp channel got slightly worse: more wall contact, about 3 more ticks lost per run;
- bodies resting deeper in a wall than at the start of a tick rose from 2 to 4 over nine runs, all far inside the
  physical depth limit.

**It failed two existing tests:**
- each counts sudden back-and-forth turns of a marching column on one fixed run, and must find none;
- the re-planned beetle now walks past a pulse bot, and the contact between them turns the pulse bot's velocity 13°
  one way and 12° back on the next tick;
- the fix never acts on the pulse bot. The bump is today's contact solver, which is not being redesigned (Q0147).

## Options

- **(a) Judge the fix by its purpose.** The two tests keep counting the unit's own steering turns, which is what they
  were written to catch, and they must still find none. Contact bumps from neighbours are counted and judged by the
  wider zig-zag measures across 1,000 scenes instead. A test-only change must first still catch the steering defect
  the canyon test was written for. The mixed figures are recorded, as you accepted for WP1 in Q0137. The depth
  limit, the Hold Position, nudge and no-enemy-push checks, and the battle rules stay binding.
- **(b) Keep the rules as they were (default).** If the tests or figures fail again, the fix stays parked and the
  ramp corner bump stays.
- **(c) Something else,** for example a small separate fix in the contact solver for friends brushing past. That is
  solver work, which Q0147 closed.

Recommendation: (a). The fix removes a visible defect you reported. The test failure is a one-tick bump on a
neighbour, the kind of contact `Solver` makes everywhere today. The same answer would apply to route-line following
(WP3, B-242) if it meets the same two tests.

## Answer

