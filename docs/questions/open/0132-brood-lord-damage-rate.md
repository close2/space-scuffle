# Q0132: The Brood Lord throws its two escort Broodlings, then one at a time; accept the data's damage rate?

- **Asked:** 2026-11-28
- **Blocking:** no
- **Default until answered:** (a); the Brood Lord analogue throws both escorts on its first attack, then one per
  rebuild (every 45 ticks, 1.79 real seconds), and each escort lands as a Broodling

## Question

Today our Brood Lord analogue (`spore_matriarch`) deals two hits of 20 every 45 ticks and spawns nothing. In StarCraft
II 5.0.14 its attack is two "escort" Broodlings that orbit it: on an attack it throws every escort it has (two at most),
each hits for 20 and lands as a Broodling that fights for about 3.6 seconds; the escorts are rebuilt one at a time,
one per attack period. So after the first attack it throws one per period. Liquipedia's page says both things: "x2,
DPS 22.4" and "if ordered to continuously attack, the Brood Lord will be firing one Broodling at a time". The game
data supports the second (`BroodLordHangar`: 2 at the start, one built per 2.5 game seconds; plan
`2026-11-28-broodlings` §2).

- (a) **Follow the data:** two on the first attack, then one per period, plus a Broodling per hit. Steady hit damage
  halves; the Broodlings make up much of it, as in the game. **Recommended**: it is what 5.0.14 does.
- (b) **Keep two hits every period** and add a Broodling per hit: twice the game's Broodlings and hit damage after the
  first attack; recorded as a sanctioned deviation.

## Answer

