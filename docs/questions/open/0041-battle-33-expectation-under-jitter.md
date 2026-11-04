# Q0041: Battle 33 is a marine win on about 90 % of seeds, not all. Should its fixture say so?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. Battle 33's expectation (`blue: (4, 16), red: (0, 0)`), seeds and claim
  stay as committed.

## Question

**What we found.** The matrix test plays battle 33 (24 marines against 12 zealots) on four seeds; the first uses the
authored positions, the others jitter each spawn by up to ±0.12. Those four pass, with 8, 6, 9 and 13 marines alive.
The investigation played 64 more jittered seeds the same way:

| | Result on 68 seeds |
|---|---|
| Marine wins | 61 |
| Zealot wins (2 or 3 zealots left) | 6: seeds 1, 6, 28, 50, 58, 63 |
| Unresolved at 500 ticks | 1: seed 19, 3 marines against 1 zealot (a marine win at 1500 ticks) |
| Marine survivors | mean 7.41, SD 3.34, range 0 to 14 |

So the expectation fails on 7 of 64 new seeds. The battle is about a 90 % marine win, and its seeds of record happen
to be among the winners. The spread comes from how many zealots reach the marines at once, which small changes in
the start positions shift (correlation −0.79 with survivors). The fixture's claim ("solid bodies give marines the
edge over zealots they do not kite") does not mention the risk. The small zealot fights (25a, the benchmarks) give
the same result on every seed.

**Options.**

1. **No change (default).** The test is deterministic on its four seeds, which is what it checks.
2. **Document it** (recommended): add to the fixture's claim, or a comment beside the battle, that the outcome is
   about a 90 % marine win under jitter, with a pointer to the evidence. No expectation or seed changes.
3. **Widen the expectation** to admit a zealot win. Not recommended: the test would then check almost nothing.
4. **Change the seeds or the setup** to make the outcome robust. Not recommended: it tunes the test to a result.

**Side effects.** Options 1 and 2 change no result. A future change from Q0035 to Q0039 would move this distribution,
and with it the seeds of record.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§4.2, §4.3, §5.6 and §7.

## Answer

Owner (2026-11-04): Option 2 (document in the fixture's claim/comment that the outcome is about a 90% marine win under jitter, with a pointer to the evidence).
