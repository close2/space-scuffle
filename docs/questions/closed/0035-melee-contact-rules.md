# Q0035: Should we revisit the melee contact rules that decide battle 33?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** no change. The contact rules of commit `d4109e2` stay as they are, and battle 33 keeps
  its committed expectation (Q0007).

## Question

**What we found.** Battle 33 (24 marines against 12 zealots, no upgrades) is decided by K, the number of zealots
striking at the same time. The zealots need K of 5 or more for the whole fight; they get about 4.25. The rules
added by commit `d4109e2` (hostile bodies never overlap, blocked attackers flank sideways, the arrival rules) hold K
there. The investigation replayed battle 33 at `d4109e2`'s parent on the same 68 seeds:

| | Parent `b10e9ca` | `d4109e2` (= today) |
|---|---|---|
| Outcome on 68 seeds | zealots win 67, 1 unresolved | marines win 61, zealots 6, 1 unresolved |
| Marine survivors, mean | 0.04 | 7.41 |
| K, first 5 s of contact | 5.40 | 4.25 |
| Most zealots striking on one tick (median) | 8 | 6 |

Before `d4109e2`, zealot and marine bodies overlapped by 0.04 on average (0.13 at most, against radii summing to
0.875). That shallow overlap let one or two more zealots onto the marine front, which was enough to win. No other
cause comes close: all catalog differences from StarCraft II together move the result by 2.7 marine survivors and
do not change the winner.

Whether our K is too low is not settled. How StarCraft II spaces melee attackers around a solid enemy (which radius
governs, whether enemies can push each other) was **not found** in an authoritative source. At the size of the one
comparable published test (50 marines against 25 zealots, no upgrades, a 2011 forum test that left 40 % of the
marines alive), our marines keep 59 %. That hints our rules favour marines a little, but it is a single anecdotal
test.

`d4109e2` changed 14 files of `sim_core` and its rules depend on each other, so no single rule was reverted to
measure it alone.

**Options.**

1. **No change (default).** Battle 33 stays a marine win. The rules fixed real faults (units standing inside each
   other), and the evidence that StarCraft II packs zealots tighter is missing.
2. **Research first (recommended if StarCraft II fidelity matters here).** A plan that finds out how StarCraft II
   packs melee attackers against solid enemies (unit testers, the game data, measured spacing), and only then
   proposes a rule change with a whole-matrix measurement. Nothing in the game changes until that plan is approved.
3. **Allow a shallow hostile overlap again** now. Not recommended: it brings back what `d4109e2` fixed, it cannot be
   done as one isolated rule, and it moves every melee battle in the matrix, which was not measured.

**Side effects.** Any contact-rule change touches every battle with melee units and the movement tests, not just
battle 33. Battle 33's expectation would have to be re-recorded, which Q0007 currently keeps.

Evidence: [`docs/evidence/2026-11-03-battle-33-investigation.md`](../../evidence/2026-11-03-battle-33-investigation.md)
§3.4, §3.5, §5.2 and §6.

## Answer

Owner (2026-11-04): Option 1 (no change to the contact rules for now), but documented/todo for future investigation.

## Resolution

2026-11-06, combat alignment WP13 step 1 ([plan](../../plans/2026-11-04-combat-sc2-alignment.md), with the wording of
its amendment from plan `2026-11-05-sc2-deviation-fixes` WP0). Option 1: nothing in the game changed. The contact
rules of `d4109e2` stay for now; how StarCraft II spaces melee attackers is recorded bug B-50 in plan
[`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) (WP16, with open question U-6 on
which radius governs and whether enemies push each other). It is a deferral, not a sanctioned deviation. The
follow-up is F-1 in §8 of the combat alignment plan, and `DESIGN.md` §4's Combat row names B-50.

Closed.
