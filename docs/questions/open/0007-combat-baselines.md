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
