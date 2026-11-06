# Q0058: Does an order given between the strikes of a multi-strike attack cancel the later strikes?

- **Asked:** 2026-11-06
- **Blocking:** no
- **Default until answered:** no change. Once a multi-strike weapon (the Zealot's, the Thor's ground cannons, the
  Reaper's pistols, the Queen's talons) has released its first strike, its later strikes land on their ticks
  whatever the attacker is ordered to do; only the attacker's death, or boarding a transport, ends the burst. If the
  game does cancel them, the difference stays recorded under bug B-2 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP5 (bug B-2)
now spaces the strikes of one attack as StarCraft II does: the Zealot's second blade lands 5 ticks (0.28 Normal
seconds) after the first, the Thor's second cannon shot 7 ticks after, and so on (evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §5.1). In the game data each
such attack is a persistent effect with the flag `Channeled` (for example `liberty` EffectData:1919, the Zealot's
`PsiBladesBurst`). A channeled effect ends when its caster stops channelling, but neither the data nor Liquipedia say
which orders stop a weapon's channel: a Move, a Stop, a new attack target, or none of them (the weapons default to an
uninterruptible attack, `core` WeaponData:38).

**Why it matters.** A player who moves a Zealot right after its first blade either still gets the second blade (our
default) or loses it. It decides whether stutter-stepping a Zealot or a Thor costs damage. The Viking's and Banshee's
bursts are not `Channeled`, so they land in either case.

**Options.**

1. **No change (default).** Later strikes land unless the attacker dies or boards a transport.
2. **A Move or Stop ends a `Channeled` burst**, a new attack target does not. Needs a catalog flag per weapon
   (`Channeled`) and a rule commit.
3. **Any new order ends a `Channeled` burst.**

A measurement in the StarCraft II editor's unit tester would settle it: a Zealot attacking a training dummy, a Move
issued one game loop after the first blade; count the damage.

**Recommendation.** Option 1 until measured.

## Answer
