# Q0149: The crowd fix will not come. Let ground units walk under the colossus anyway?

- **Asked:** 2026-11-28 (plan `2026-12-03-movement-work-on-solver`, WP-C1)
- **Blocking:** no
- **Default until answered:** (b). The one-line change is re-measured on today's movement. It merges only if every
  original check holds. Otherwise it stays parked, and B-209 stays open.

## Background

In StarCraft II, small ground units walk beneath a colossus (B-209). You answered Q0117 with (b): keep the change
parked until friend crowding is fixed, then switch it on cleanly. The crowding fixes have now ended: the crowd cap
C-3′ failed its checks, and the movement redesign stopped under your Q0147 rule. So that condition cannot be met
with today's movement.

When the change was tried (Q0117), only battle 26 moved (100 zerglings against 6 colossi):

| | Today | Walk under |
|---|---|---|
| Zergling wins | 20 of 24 | 24 of 24 |
| Zerglings left (mean) | 14.9 | 33.2 |
| Zerglings standing while an enemy has room (per run) | about 2,500–3,300 | 711 |
| Zergling friends overlapping by more than 5 % (per 1,000 zergling-ticks) | 54.8 | 124.1 |
| Unexplained target switches (per run) | 1.50–3.04 | 3.58 |

Nothing stood while it could fight, and nothing pushed an enemy. Three crowd checks failed because the zerglings pack
tighter round and under the colossi. That is the known crowd overlap of today's movement (B-219).

## Options

- **(a) Switch it on.** It merges once the change is re-measured on today's movement and the following hold:
  - no unit stands while able to fight;
  - no body is corrected by a body it should not touch;
  - every world without a colossus is byte-identical.

  The three crowd figures are reported and traced to B-219, whose row records battle 26. Battle 26 is restated
  (zerglings win every run).
- **(b) Keep it parked (default).** It switches on only if a re-measure passes every original check.
- **(c) Sanction today's rule.** Record "ground units cannot walk under a colossus" as a deviation and close B-209.

Recommendation: (a). It is StarCraft II's rule, the change is one line of data, and the extra overlap is the same
crowd behaviour every dense fight shows today.

## Answer

