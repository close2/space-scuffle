# Q0062: When a strike breaks a unit's shields with less left over than its armour, how much reaches life?

- **Asked:** 2026-11-06
- **Blocking:** no
- **Default until answered:** no change. A strike that breaks the shields always deals at least 0.5 to life, even
  when what is left after the shields is smaller than life armour. If the game deals less (0, or something else),
  the difference stays recorded under bug B-15 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP7 (bug
B-15, research item U-3) now sends to life what the shields did not take of the *shield-mitigated* damage, less
life armour, as Liquipedia's
[Damage Calculation](https://liquipedia.net/starcraft2/Damage_Calculation) states ("Total Damage" step; evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §7.1). The formula is
engine code, not game data, so the 5.0.14 data files cannot settle it. One case stays open: the remainder after the
shields is positive but not larger than life armour. Example: a 10-damage strike against 9 shields, shield armour
0, life armour 2 leaves 1 for life, minus 2 armour.

- Liquipedia's text, read literally, sends the whole remainder (1) to life with no armour in that case: its
  "Total Damage" subtracts armour only when the damage exceeds shields plus armour. That makes life damage jump
  from 2 to 0 as the strike grows past shields plus armour, which reads as an editing slip rather than a rule.
- Liquipedia's Armor page says "every attack deals at least 0.5 damage", which supports 0.5 for the strike as a
  whole, but says nothing about the life part of a strike that already damaged shields.
- No other source was found (TeamLiquid, SC2Mapster and Hive Workshop searched, 2026-11-06).

**Why it matters.** It decides how much a Protoss unit with armour upgrades loses when its last shield points go:
the options differ by at most the unit's life armour per strike, but on every shield break of a fast-hitting attacker
(Marines, Zerglings) against an armoured Protoss unit.

**Options.**

1. **No change (default).** The life part is at least 0.5.
2. **The life part may be 0.** Armour can absorb the whole remainder; the strike's 0.5 minimum is met by the
   shields it took.
3. **Liquipedia's literal text.** The remainder reaches life unarmoured when it is not larger than armour.

A measurement in the StarCraft II editor's unit tester would settle it: a Zealot (shield armour 0, life armour 1,
then upgraded to life armour 3) with its shields set to 4, hit once by a Marine (6 damage); read its life to two
decimals.

**Recommendation.** Option 1 until measured.

## Answer

**Option 1 (No change — the life part is at least `0.5`; no editor test needed).**

**Engine insight:** In StarCraft II's C++ damage pipeline, when a hit breaks a unit's remaining shields with strictly positive post-shield-armor spillover (`remainder > 0`), the hull damage step applies its own `max(0.5, remainder - life_armor)` minimum floor rather than allowing life armor to reduce positive spillover to `0.0`. Thus any hit that exceeds current shields always chips at least `0.5` HP off life regardless of how much life armor the target has. The landed WP7 code (`apply_damage` in `crates/sim/src/combat.rs`) already matches SC2 and requires no change.

## Resolution

2026-11-09. Option 1: no change. The landed WP7 rule already deals at least 0.5 to life when a strike breaks the
shields with a positive remainder, which the owner's answer confirms as the engine's. Bug B-15 is closed and leaves
the `DESIGN.md` register; plan `2026-11-05-sc2-deviation-fixes` Amendment 22 (package G) records it. No code commit.
