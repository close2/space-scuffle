# Q0007: Are the changed combat outcomes and data choices acceptable?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** keep them as committed

## Question

These were changed during Phase 3, and nothing was tuned to reach them.

1. **Battle 33:** marines now beat zealots with 6 to 13 of 24 left. Plan 21 calls it a close zealot win, so the
   expected result was changed. Is the battle a bug to fix, or is the new result fine?
2. **Battle 28** was extended from 600 to 1000 ticks. Battles 11a, 25b, 31, 32 and 35 also changed.
3. **Upgrades:** 16 `UnlockAbility` effects naming nonexistent abilities were deleted from `upgrades.ron`.
4. **Regeneration:** `structures.ron` gained regen.
5. **Splash ignores cloak detection**, a deliberate deviation from finding S-17.

## Answer

Owner (2026-11-03): Keep all five as committed. I do however want an "investigation" why internet says that Zealots should win, and why we have 6-13 of 24 marines surviving. This is a big difference.

## Effect of the answer

2026-11-03. All five items stay as committed; nothing is reverted. The owner asked for an investigation of battle
33: why published StarCraft II sources say zealots win this matchup, while the simulation leaves 6 to 13 of 24
marines alive. A plan for that investigation is being written. This question stays open until its findings are
reported here.

2026-11-04. The investigation is done (plan `docs/plans/2026-11-04-zealot-marine-investigation.md`; findings in
[`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md) §6).
In short:

- **The "zealots win" claims** assume small numbers, Charge or a surround. The one published no-Charge test near
  battle 33's size (50 marines against 25 zealots) is a marine win with 40 % alive. Our small zealot fights (battle
  25a and the benchmarks) give zealot wins on every seed, as the sources say.
- **The 6 to 13 survivors** come from how many zealots strike at once: about 4.25, against the 5 the zealots need.
  The solid-body contact rules of commit `d4109e2` and the zealots' 3-deep block hold it there. Before `d4109e2` it
  was 5.4 and the zealots won 67 of 68 seeds. Over 68 seeds the marines win 61, with 7.41 alive on average.
- **The catalog's differences from StarCraft II** (zealot wind-up and period, scan ranges, marine speed and more)
  are real and together favour the marines by about 2.7 survivors, but with all of them corrected the marines still
  win about 70 % of seeds.

Each proposed change is its own question, all defaulting to no change: Q0035 (contact rules), Q0036 (zealot weapon
timing), Q0037 (scan ranges), Q0038 (marine backswing and the backswing rule), Q0039 (values left in Normal time),
Q0040 (zealot attributes) and Q0041 (battle 33's expectation fails on 7 of 64 jittered seeds). This question stays
open for the owner to close once those are read.
