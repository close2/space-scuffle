# Q0064: When does the Colossus' Thermal Lances damage land?

- **Asked:** 2026-11-06
- **Blocking:** no
- **Default until answered:** no change. Both of the Colossus' strikes on its target, and its sweep's damage to the
  units on the line, land on the tick the weapon releases. Whatever the game does differently stays recorded under
  bugs B-81 (the target's strikes) and B-85 (the line's searches spaced in time) in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP8b (bug
B-18) built the line area its sweep needs: in the data, a line of 11 searches of radius 0.15 across the attack,
centred on the target, once per sweep (evidence
[`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md) §9.1; the data commit is held,
§9.4). Amendment 8 assigned to WP8b the delay of the Colossus' strikes on its target (B-81). The 5.0.14 data give the
pieces, but not how the engine combines them:

- `ThermalLances` fires two persistents, `ThermalLancesForward` and `ThermalLancesReverse` (`liberty`
  EffectData:2357-2360). Each has `PeriodCount` 11 (`liberty` EffectData:2365, :2390), a period of 0.0227 Normal s
  (`voidmulti` EffectData:954, :995; earlier 0.0312 and 0.025), `ExpireDelay` 0.73 Normal s and `WhichLocation`
  `TargetPoint` (`swarm` EffectData:2322-2331, which override `liberty`'s 0.375 and `TargetUnit`), and
  `ExpireEffect` `ThermalLancesMU`, the damage (`liberty` EffectData:2385, :2410).
- Each search hands a found unit to `ThermalLancesDamageDelay`, which damages it 0.25 Normal s later (`liberty`
  EffectData:780-785).
- WP5 read the target's delay as the `liberty` `ExpireDelay` of 0.375 Normal s (7 ticks); `swarm` sets 0.73.

**What is not stated.** (1) Whether `ExpireDelay` counts from the persistent's creation or from its last period
(an SC2Mapster forum tutorial says "after the conditions of the persistent have been completed", which would put the
target's damage at 11 × 0.0227 + 0.73 = 0.98 Normal s, 17 or 18 ticks). (2) Whether a persistent placed at
`TargetPoint` still applies its `ExpireEffect` to the target unit, or whether the target is struck only by a search
(its searches exclude `Target`). (3) Whether a period's first entry delays the first search here as it does for the
multi-strike weapons (evidence §5.1), which would put the line's damage between 0.27 and 0.50 Normal s (5 to 9
ticks).

**Why it matters.** It decides when a Colossus' damage lands: at once, as now, or between about 5 and 18 ticks
after its 0.0832 Normal s damage point. A target that dies or leaves in between would be spared.

**Options.**

1. **No change (default).** Every strike and the whole sweep land on the release tick.
2. **Measured times.** Apply the times an editor measurement gives: a Colossus attacking a stationary Marine, with a
   second Marine 1 to the side of the first, logging the game loop of each Marine's damage.

**Recommendation.** Option 1 until measured; then option 2 as a rule for persistent timing, which the Lurker's
spines (0.125 Normal s apart, B-85) also need.

## Answer
