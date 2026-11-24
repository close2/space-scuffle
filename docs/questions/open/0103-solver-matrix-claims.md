# Q0103: With the new movement solver, two battle claims break. Change the claims, call them bugs, or switch the nudge off?

- **Asked:** 2026-11-24
- **Blocking:** the merge of the new movement solver, together with [Q0102](../closed/0102-solver-cost-gap.md). Other work
  continues meanwhile.
- **Default until answered:** (a) for battle 26 and (b) for battle 28. These are applied when you answer, or at the
  merge if that comes first. Until then the solver branch keeps the old expectations, and its battle test fails on
  these two battles.

## Background

The test suite has a "matchup matrix": fixed battles, each with a claim about who wins, played on 24 runs with
slightly shifted start positions. Making the new movement solver the default (plan `2026-11-20-contact-solver`,
WP6) breaks two claims. Both breaks have been traced to their cause (evidence `2026-11-20-contact-solver.md`
§13.12 and §13.13).

### Battle 26: "Six colossi vaporize a hundred zerglings"

The cause is the nudge you approved in Q0095 (deviation D-δ in Q0052). A zergling attacking a colossus may slide
sideways along it when other zerglings need room. So more zerglings fit round each colossus and attack at once:
10 round the first colossus against 3 before. The colossi die before their sweeps have killed enough zerglings.

| Version | Zergling wins (24 runs) |
|---|---:|
| Old movement code | 1 |
| New solver, nudge on (as you chose) | 20 |
| New solver, nudge off | 0 |

Nothing else is broken:
- the colossi fire as often as before;
- each shot hits slightly more zerglings, not fewer;
- no unit overlaps an enemy more than before.

The claim was already fragile. Changing one movement constant by one part in four billion once made the zerglings
win a recorded run. We found no published StarCraft II test of exactly 100 zerglings against 6 colossi. For scale:
100 zerglings cost 2,500 minerals and 50 supply; 6 colossi cost 1,800 minerals, 1,200 gas and 36 supply.

### Battle 28: "Without splash, immortals are overrun" (100 zerglings against 10 immortals)

The zerglings now lose 19 of 24 runs; with the old movement code they won all 24. The cause is a new defect,
registered as B-192:
- the immortals stand in two columns;
- every zergling picks the nearest immortal, in the front column;
- once the space round the front immortals is full, the other zerglings stand and wait behind them;
- they never switch to the rear column, which has free room all fight.

We built a fix, D-ε: a zergling whose automatic target is fully surrounded picks the next enemy that still has
room. It works for battle 28, with the zerglings winning 24 of 24. It was stopped by the plan's own rule, though,
because it also moves three other battles outside their recorded ranges:
- **battle 33:** the zealots now win 21 of 24;
- **battle 38a:** 10 zerglings are left, against 3 to 8 before;
- **battle 38b:** the zerglings win 9 of 24.

It also exposed something the solver already does (B-193): a crowd of zerglings shoves the enemies it is fighting.
On one run of battle 28 the engaged immortals were pushed 3.6 world units in total under the solver without the
fix, and 94.6 with it, against 0.85 under the old code. Whether StarCraft II lets a crowd push enemies at all still
needs checking. The fix is kept on its own branch (`contact-solver-g6p-wip`) for a later attempt.

Battle 26 with the fix: the zerglings win 22 of 24.

## Options

**Battle 26:**
- **(a) Restate the claim to what is measured** (zerglings mostly win), with a comment in the test citing the nudge
  D-δ. Default.
- **(b) Keep the claim and treat the result as a bug,** to be fixed in a later package without turning the nudge off.
- **(c) Make the nudge off by default.** This reverses your Q0095 choice. The colossi win again, but the solver
  loses the choke and corridor benefit you wanted from the nudge, and battle 28 gets harder for the zerglings still.

**Battle 28:**
- **(a) Restate the claim to what is measured** (the immortals mostly win).
- **(b) Keep the claim and merge with the battle marked as known bug B-192,** so the test reports it as an expected
  failure until a later package fixes it. Default: the claim matches StarCraft II, and the old code met it.
- **(c) Do not merge the solver until B-192 is fixed** (a further attempt at D-ε that also deals with the shoving,
  B-193).

## Answer

- **Battle 26 — Option (a):** Restate the claim to what is measured (zerglings mostly win), citing the Q0095 melee ring-nudge (`D-δ`).
- **Battle 28 — Option (b):** Keep the StarCraft II claim (zerglings overrun immortals) and merge the movement solver with Battle 28 marked as an expected failure for known bug `B-192`, so `B-192` (retargeting around a surrounded front row) and `B-193` (enemy shoving) remain tracked for a dedicated follow-up fix.

**Standing guidance (recorded in `AGENTS.md` §8):** Good, natural pathing and movement outweigh locking in the exact winner of close matchups. When a movement improvement shifts a close battle's winner (even across all runs, e.g. Battle 26 or close battles like 33/38a/38b when evaluating `D-ε`), restate the claim as a minor deviation rather than halting work; treat a shift as a bug only when the underlying behaviour itself is broken or unexpected (such as `B-192`'s stalled rear Zerglings in Battle 28 or `B-193`'s enemy shoving).
