# Q0057: Are StarCraft II scan ranges measured from the units' edges or their centres?

- **Asked:** 2026-11-05
- **Blocking:** no
- **Default until answered:** no change. Every unit keeps its catalog `scan_range` (8 to 12, centre to centre),
  and the difference stays recorded as bug B-12 in `DESIGN.md`.

## Question

**Background.** [Q0037](../closed/0037-scan-ranges.md) (option 3) asked for StarCraft II's scan ranges across the catalog.
Combat alignment WP14 found them: the game data give every weapon a `MinScanRange` (default 5; most ground weapons
`Range` + 0.5 in `voidmulti`), and a unit scans to that or its weapon range, whichever is greater. The table, with a
citation per value, is in [`docs/evidence/2026-11-04-catalog-time-audit.md`](../../evidence/2026-11-04-catalog-time-audit.md)
§11 (the marine 5.5, the zealot 5, the Thor analogue 10.5, workers and melee units 5).

**What is missing.** No source says how the game measures a scan range. Weapon range is measured from the
attacker's edge (the editor adds the unit's radius unless a weapon sets "Use Range To Center"), and our weapon reach
is edge to edge. Our scan range is measured centre to centre. The value means very different things under the two
rules:

- **Centre to centre** (our rule, values copied as they are): the marine scans 5.5 between centres, but already
  reaches a zealot at 5.875 between centres, so it would only acquire what it can already shoot. This contradicts
  why the game sets `Range` + 0.5.
- **Edge to edge** (likely, from that `+ 0.5`, but not stated anywhere we found): the marine scans 5.5 beyond both
  radii, 6.375 between centres against a zealot. This needs a small code change (scan distance minus both radii),
  not only data.

**What each rule does** (every reference value at once, against `main` at `77f19c0`; evidence
[`2026-11-04-combat-sc2-alignment.md`](../../evidence/2026-11-04-combat-sc2-alignment.md) §15):

| | Centre to centre | Edge to edge | Today (9 / 9) |
|---|---|---|---|
| Matrix battles that move (20 seeds) | 26 | 15 | — |
| Battles whose winner over the seeds changes | none | none | — |
| Expectation failures, all 1000 runs | 1 (battle 35, seed 4) | 2 (battle 33, seeds 1 and 5) | 0 |
| Battle 33, 68 seeds: marine / zealot wins / unresolved | 16 / 38 / 14 | 21 / 37 / 10 | 20 / 35 / 13 |
| Battle 33 marine survivors, change per seed (SE) | −0.32 (0.42) | +0.21 (0.47) | — |

In battle 33 the scan range matters through timing: under the centre rule a marine notices a zealot only when it is
already inside its weapon reach, about 17 ticks later than today, and the marines release about 5 fewer shots
(a fifth of a volley); under the edge rule it notices the zealot before it can fire, and the battle is as today.

**Options.**

1. **No change (default).** B-12 stays recorded until the rule is sourced (for example by a measurement in the
   StarCraft II editor's unit tester: a marine on Stop, a zealot walked towards it, the centre distance at which the
   marine starts shooting or turning).
2. **Edge to edge** with the reference values: a rule commit (scan distance minus both radii) and one data commit
   with every value of the table; the moved battles and battle 33 re-recorded.
3. **Centre to centre** with the reference values: one data commit.

**Recommendation.** Option 1 until the rule is measured in the game; then option 2 or 3 accordingly.

## Answer

