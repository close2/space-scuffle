# Q0071: How exactly does the Zealot's Charge trigger and end?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no change to the rule WP13c landed. The dash starts on any tick a unit with the
  capability has an attack order on a visible enemy whose gap (centre distance less both collision radii) is above
  0.6 and whose centre is less than 4 away in a straight line, unless it holds position or the ability is on
  cooldown. It ends when its 63 ticks run out or when the unit's weapon starts a swing; a new order does not end it.
  A difference stays recorded under bug B-43 in `DESIGN.md`.

## Question

**Background.** Plan [`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) WP13c (bug
B-43) gives the Zealot analogue (`psi_grey`) StarCraft II's Charge dash once the Charge upgrade is researched. The
data (SC2GameData `5444e12f13`, chain `core → liberty → swarm → void → voidmulti`) states the parts: the ability
`Charge` is an autocast augment of the `attack` command, on by default, with a 10 Normal s cooldown and the autocast
validator `CasterNotHoldingPosition` (`liberty` AbilData:3825-3834). Its effect applies the `Charging` buff to the
caster if `ChargeMinTriggerDistance` (order target range `GT 0.6`, `liberty` ValidatorData:183-190) and
`ChargeMaxDistance` (source to target location `LT 4`, `Pathing 1`, :1697-1704) pass (`liberty` EffectData:2062-2068).
`Charging` lasts 3.5 Normal s, multiplies speed by 2.2, carries the `Channeling` flag, and is removed when
`ChargeTargetAttacked` fails, that is when the `PsiBlades` weapon fires (`liberty` BehaviorData:967-975,
ValidatorData:3007-3011). Evidence [`2026-11-05-sc2-deviation-fixes.md`](../../evidence/2026-11-05-sc2-deviation-fixes.md)
§16.

What the data names but does not define:

1. **The 0.6.** `CValidatorUnitCompareOrderTargetRange` compares "the range to the order's target". Whether that
   is measured from the edges (as weapon and scan ranges are, [Q0057](../closed/0057-scan-range-measuring-rule.md))
   or from the centres is not stated. WP13c reads edges: a Zealot already within 0.6 of its target's edge does not
   dash.
2. **The 4.** `CValidatorLocationCompareRange` with `Pathing 1` compares the *pathing* distance between the two
   units' locations (centres). WP13c measures it in a straight line: around a wall or a ramp the walk can be longer
   than 4 while the straight line is shorter, so ours dashes slightly more often.
3. **When the autocast looks.** Whether the engine tries the augment every game loop while the attack order runs,
   or only when the order starts or retargets, is not stated. WP13c tries it every tick.
4. **What ends it.** "Weapon firing" (`CValidatorUnitWeaponFiring`) is read as the start of a swing. The
   `Channeling` buff flag's meaning (does a new order end the buff?) is not documented; Liquipedia's Charge page says
   that switching to a farther target while charging lets the Zealot cover more ground, so WP13c lets the dash run
   on through a new order.

**Why it matters.** Items 1 and 2 decide at which distances a Zealot dashes, item 3 how soon after a target comes
within 4, and item 4 how far a dash can carry a Zealot that is ordered elsewhere.

**Options.**

1. **No change (default).** As above.
2. **Measured.** Editor tests with a Zealot (Charge researched) against a stationary Marine: start it at gaps of
   0.5 and 0.7 (item 1); put a wall between them so the walk is about 5 and the straight line 3.5 (item 2); log the
   Zealot's speed each game loop as the Marine walks into range (item 3); and issue a move order mid-dash (item 4).

**Recommendation.** Option 2; any change it shows is a few lines in `world/attack_dash.rs`.

## Answer
