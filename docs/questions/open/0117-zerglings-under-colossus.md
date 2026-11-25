# Q0117: Let ground units walk under the colossus (as in StarCraft II), even though zerglings then crowd tighter?

- **Asked:** 2026-11-26
- **Blocking:** only the one-line data change that gives the colossus analogue its own collision class (B-209), parked
  on branch `collision-classes-wp3-wip` (head `42761cf2`; local only until pushed). The mechanism it needs is merged. Other work continues.
- **Default until answered:** (b), keep the change parked. The colossus analogue keeps colliding with every ground
  unit, as today.

## Background

In StarCraft II 5.0.14 the Colossus collides only with other colossi, structures and air units (`liberty`
UnitData:4066–4068), so zerglings and other ground units walk beneath it, and it steps over its own army. Ours
collides with every ground unit (B-209). Plan `2026-11-25-collision-classes` built collision classes as data (merged)
and then tried the change for the colossus analogue. Only battle 26 (100 zerglings against 6 colossi) moved; every
other battle, the showcases, the replays and the benchmarks stayed byte-identical, and the cost is within budget.

Battle 26 on 24 runs (evidence `docs/evidence/2026-11-25-collision-classes.md` §3):

| | Today | Zerglings walk under colossi |
|---|---|---|
| Zergling wins | 20 of 24 | 24 of 24 |
| Zerglings left at the end (mean) | 14.9 | 33.2 |
| Battle length (mean) | 439 ticks | 322 ticks |
| Zerglings standing while an enemy has free room (per run) | about 2,500–3,300 | 711 |
| Zerglings overlapping each other by more than 5 % (per 1,000 zergling-ticks) | 54.8 | 124.1 |
| Zergling shots per engaged second | 2.122–2.138 | 2.113 |
| Unexplained target switches (per run) | 1.50–3.04 | 3.58 |

Nothing stands while able to fight, and no unit pushes an enemy. The change stopped because three pre-registered crowd
checks failed: zerglings now pack much more tightly round and under the colossi, so they overlap each other more than
twice as often, and they switch targets and fire slightly differently. Battle 26's restated claim ("zerglings mostly
win; when the colossi win, they win narrowly", Q0103) would need restating again: the zerglings now win every run with
up to 45 left.

## Options

- **(a) Accept it.** Merge the data change, restate battle 26 (zerglings win every run), and register the tighter
  zergling packing as a crowd bug to work on with the friend-overlap issue of Q0113.
- **(b) Keep it parked (default).** Fix the friend crowding first (the same friend-overlap problem stopped Q0113's
  work), then switch the colossus change on.
- **(c) Sanction today's rule.** Record "ground units cannot walk under a colossus" as a deviation and close B-209.

Recommendation: (b) if friend crowding is to be fixed soon anyway; (a) if you prefer the game's rule now.

## Answer

- **Option (b): Keep parked for now.** Fix friend crowding first (`Q0118` / `Q0120` movement redesign), then switch on `B-209` (ground units walking under Colossi) cleanly without introducing a 2× friendly-overlap regression on `main` beforehand.
