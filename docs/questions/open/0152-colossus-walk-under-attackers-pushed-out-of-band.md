# Q0152: Ground units walking under the colossus also push attacking zerglings out of their band. Does that still block it?

- **Asked:** 2026-11-28 (plan `2026-12-03-movement-work-on-solver`, WP-C1; evidence `2026-11-25-collision-classes` §5)
- **Blocking:** no
- **Default until answered:** (b). H-S2 binds under every answer to Q0149, as the plan's stop rule G-2 says, so the
  colossus line stays parked on branch `colossus-c1-wip` even under Q0149 (a), and B-209 stays open.

## Background

In StarCraft II, small ground units walk beneath a colossus (B-209, Q0149). The one-line data change that does this
was re-measured on today's movement (`Solver`), on 24 seeds and on 24 held-out seeds. As in Q0149's table, only
battle 26 (100 zerglings against 6 colossi) moves:
- the zerglings win 24 of 24, against 20 of 24 today;
- nothing stands while it could fight;
- no body touches a body of another class;
- every world without a colossus is unchanged.

Q0149 (a) would accept the three crowd figures that fail (B4, B6, B7′) as today's known crowd overlap (B-219).

The new measurement adds a figure Q0149 did not show. It is one of your contact rules: **attackers are pushed by
allies only within their weapon band.** Today `Solver` already breaks this rule a little in every dense fight (B-225).
With zerglings packed beneath and round the colossi, it happens about twice as often:

| Battle 26, per run | Today | Walk under |
|---|---|---|
| Zergling-ticks moved by friends out of their band (24 seeds) | 21.1 (95 % interval 19.0 to 23.2) | 43.8 |
| The same, held-out seeds | 22.5 (19.8 to 25.5) | 44.0 |
| As a share of zergling-ticks | 0.10 % | 0.23 % |

Hold Position is never broken (0 in both), and enemies never push (0 in both).

## Options

- **(a) Treat it like the crowd figures.** H-S2 in battle 26 is B-225's known behaviour in a denser crowd. Its figure
  goes into B-225's row. Under Q0149 (a) the line then merges. B-225 itself stays open.
- **(b) Keep it binding (default).** The line merges only once B-225's count beneath a colossus is back within
  today's, which needs B-225 fixed. That is a change to the solver, which Q0147's stop rule excludes for now.

Recommendation: (a), if you choose Q0149 (a). Like the crowd overlap, it is the same `Solver` behaviour every dense
fight shows, made more frequent by a correct StarCraft II rule. Under Q0149 (b) or (c) this question has no effect.

## Answer

