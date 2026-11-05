# Q0037: Should units acquire targets at StarCraft II's scan range?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. Every unit keeps its catalog `scan_range` (8 to 12; 9 for the marine and
  the zealot).

## Question

**What we found.** In StarCraft II (Legacy of the Void) a unit acquires targets within its weapon range + 0.5, and
at least 5 for short-range units: 5.5 for the marine, 5 for the zealot. Our catalog gives both 9, measured centre to
centre. Battle 33 (24 marines against 12 zealots), 68 seeds, marine survivors against today's 7.41:

| Scan range (marine / zealot) | Marine wins / zealot wins / unresolved | Marine survivors | Paired Δ (SE) |
|---|---|---|---|
| 9 / 9 (today) | 61 / 6 / 1 | 7.41 | — |
| 5.5 / 9 | 50 / 5 / 13 | 5.78 | −1.63 (0.52) |
| 9 / 5 | 58 / 1 / 9 | 6.35 | −1.06 (0.55) |
| 5.5 / 5 | 37 / 11 / 20 | 4.29 | −3.12 (0.59) |

With 1500 ticks, so every fight ends, 5.5 / 5 gives 44 marine wins to 24. This is the largest single data effect
found, but its **mechanism is not isolated**: the number of zealots in contact barely moves (4.25 → 4.45) while
zealot swings rise from 53 to 62. The likely reading is that the marines focus different zealots.

**Options.**

1. **No change (default).**
2. **The marine and zealot only.** Not recommended: the rule is general, and two units on it would be inconsistent.
3. **A plan that applies StarCraft II's rule across the catalog** (42 units carry a `scan_range`), finds the
   mechanism in battle 33 first, and measures the whole matrix before any change.

**Side effects.** Battle 33 would fail its committed expectation more often: on 3 of its 4 seeds of record with every
reference value. A catalog-wide change would move battles that were not measured (the whole-matrix control changed
only the marine and the zealot). Scan range also sets how far units react on attack-move, so it changes how every
fight starts.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§1.4, §1.5, §2.3, §5.3, §5.6 and §7.

## Answer

Owner (2026-11-04): Option 3 (a plan that applies StarCraft II's scan-range rule across the catalog and measures the whole matrix before changing values), but low priority.

## Resolution

Carried out as combat alignment WP14 (plan [`2026-11-04-combat-sc2-alignment`](../../plans/2026-11-04-combat-sc2-alignment.md)),
after sc2-deviation-fixes WP4, on 2026-11-05: the mechanism in battle 33 (timing, not focus: a marine whose scan is
below its weapon reach acquires a zealot only once it can already shoot it), the reference table for every armed
identity (audit [§11](../../evidence/2026-11-04-catalog-time-audit.md)) and the whole matrix with every reference
value (evidence [§15](../../evidence/2026-11-04-combat-sc2-alignment.md); 26 battles move under our centre rule and 15
under an edge rule, and no battle's winner over its seeds changes). No value was applied: whether the game measures a scan range from the units' centres or edges is
not sourced and decides what the values do. That decision is [Q0057](../open/0057-scan-range-measuring-rule.md);
until it is answered every unit keeps its `scan_range`, and the difference stays recorded as bug B-12.
