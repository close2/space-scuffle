# Q0053: Can a Marine use Stimpack when its life is at or below the cost?

- **Asked:** 2026-11-05
- **Blocking:** no
- **Default until answered:** no change. A unit may stim at any life above 1, pays the cost, and is left with at least
  1 life. The difference stays recorded as bug B-33 in plan
  [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) (§5 U-8) and in the `DESIGN.md`
  bug register.

## Question

Stimpack costs life: 10 for the Marine and 20 for the Marauder. What should happen when the unit has that much life
or less?

**Our rule.** `Entity::prepare_overclock` (`crates/sim_core/src/entity.rs`) refuses the stim only at 1 life or less.
Otherwise it takes the cost and leaves the unit at least 1 life. A Marine with 6 life can stim and is left with 1.

**What the sources say.**
- The game data give only the cost: `Stimpack` `<Cost><Vital index="Life" value="10"/></Cost>` (`liberty`
  AbilData:1229, SC2GameData `5444e12f13`, the 5.0.14 data). `StimpackMarauder` (AbilData:983) costs 20. The rule
  for a life cost that the unit cannot pay is in the engine, not in the data.
- Liquipedia's [Stimpack](https://liquipedia.net/starcraft2/Stimpack) page says "You can never kill the unit itself
  by using Stimpack". It does not say whether the button is refused at low life or whether the unit is left with
  some life. The [Marine](https://liquipedia.net/starcraft2/Marine_(Legacy_of_the_Void)) page gives only the cost.

So two rules both fit the sources:
- (a) the stim is refused unless the unit has **more** life than the cost (the plan's reading of "not enough life");
- (b) the stim is allowed and the unit is left with some life, as our rule does.

**Options.**
1. **Measure it in the game** (recommended): in the StarCraft II editor or a unit tester, bring a Marine to 10 life
   and to 6 life and press Stimpack. Tell us whether it stims and what life it has afterwards. We then apply that
   rule in one commit, with a test.
2. **Adopt (a)** without a measurement.
3. **Keep our rule** and record it as an accepted difference.

No matrix battle stims a unit at low life, so the choice changes no measured result; it changes when a player can
stim a hurt Marine in skirmish.

## Answer

