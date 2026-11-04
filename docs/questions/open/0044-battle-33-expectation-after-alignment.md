# Q0044: After the StarCraft II corrections, battle 33 is a close fight. How should its expectation be recorded?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** option 1. Each commit of plan
  [`2026-11-04-combat-sc2-alignment`](../../plans/2026-11-04-combat-sc2-alignment.md) that moves battle 33 off its
  expectation re-records it as the smallest survivor ranges that hold on the four seeds of record, and writes the
  measured 68-seed distribution into the battle's claim. Seeds and setup do not change.

## Question

**What we found.** Battle 33 (24 marines against 12 zealots) expects `blue: (4, 16), red: (0, 0)`: a marine win on
all four seeds of record. Your answers to Q0036, Q0038, Q0039 and Q0040 move the zealot and the marine to StarCraft
II's values. A planning probe played battle 33 on 68 seeds with those values on scratch catalogs (the regeneration
delay and the catalog-wide audit not included; the zealot's new cadence approximated):

| Catalog | Marine wins / zealot wins / unresolved (68 seeds) | Marine survivors, mean (SD) | Seeds of record (marines – zealots alive) |
|---|---|---|---|
| Today | 61 / 6 / 1 | 7.41 (3.34) | 8–0, 6–0, 9–0, 13–0 |
| Zealot changes (damage point 0, every 21 ticks, range 0.1, backswing 9; attributes and shield regeneration, both inert here) | 38 / 13 / 17 | 4.07 (3.15) | 3–0, 6–0, 3–1, 7–0 |
| Zealot and marine changes together | 47 / 15 / 6 | 4.75 (3.47) | 7–0, 0–1, 9–0, 5–0 |

So with the values you chose, battle 33 becomes about a 70 % marine win, and one or two of its four seeds of record
no longer meet `blue: (4, 16), red: (0, 0)`. The plan cannot keep the expectation as it is, and Q0041 (document the
spread, do not widen) was answered for today's 90 %.

**Options.**

1. **Re-record to the seeds of record and document the distribution (default).** For example `blue: (0, 9),
   red: (0, 1)` for the last row above, with the claim saying "about 70 % marine win over 68 jittered seeds" and
   pointing to the evidence. Cheap and honest, but the range then admits a narrow zealot win, so the test guards
   less than today: it still fails if either side wins clearly.
2. **Add a distribution test** next to the matrix: play battle 33 on 68 seeds in release mode and require the marine
   win share to stay in a band (for example 55 % to 85 %). Strongest guard, but it costs about 30 s per run in release
   and much more in debug, so it would run in its own CI job, not in `cargo test --workspace`.
3. **Per-seed expectations** for battle 33 (a schema change in the matrix fixture). Precise, but each later data
   change would re-record four pairs of numbers, and it says nothing about seeds outside the four.
4. **Change the seeds or the setup** so the outcome is robust again. Not recommended: it tunes the test to a result.

**Side effects.** Options 1 and 3 only change `crates/sim_core/tests/fixtures/matchup_matrix.ron`. Option 2 adds a
test and a CI job. The low-priority scan-range work (Q0037) would move battle 33 again (the investigation measured
−3.1 marine survivors), so whichever option is chosen will be applied once more if that work lands.

Evidence: the plan's §5 (planning probe) and
[`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md) §4.2,
§5.3 and §5.7.

## Answer

Owner (2026-11-04): Option 1 (re-record to the seeds of record and document the distribution). Having a close battle isn't a bad thing.
