# Q0116: How a Siege Tank's turret picks what to follow while it moves, and whether Immortals and Colossi do the same

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** the unsieged tank's turret follows the nearest enemy in range whenever its cannon has
  nothing to fire at (moving, or chasing a target out of range), switching as soon as another enemy is nearer; it
  never fires, slows the tank or alerts anyone. Immortal and Colossus turrets are unchanged for now (registered as
  B-218) (plan `2026-11-26-turret-tracking-weapon` D-3, §14 F-1).

## Question

StarCraft II 5.0.14 gives the unsieged Siege Tank a hidden second weapon that does no damage; its only effect is that
the turret turns towards enemies while the tank moves. Blizzard's 2015 note says Siege Tanks and Immortals "track the
nearest enemy while moving"; patch 5.0.13 made the tank and Immortal turrets keep tracking the unit they last attacked,
and 5.0.14 reverted that "due to causing unintentional retargeting".

1. **Which enemy does the turret follow?** (a) Always the nearest in range, switching when another becomes nearer
   (the default); (b) the first one it picked, until it leaves range or dies.
2. **Immortals and Colossi.** Their own weapons may fire outside attack orders but only while standing. In the game
   their turrets probably also aim at enemies while they walk. (a) Leave them as they are for now (the default; a
   later plan); (b) do them next, accepting that battles with Immortals or Colossi may change because their turrets
   are already aimed when they stop.
3. **If you can, check in a custom game:** a Siege Tank ordered to move past two Marines walking in opposite
   directions — does the turret switch between them, and does the tank ever pause, flash or alert them?

## Answer

- **1(a):** Always follow the **nearest enemy in range**, switching as soon as another enemy becomes nearer (matching StarCraft II 5.0.14's revert of 5.0.13).
- **2(b):** **Do Immortals and Colossi next**, accepting that battles with Immortals or Colossi may shift slightly because their turrets start pre-aimed when they stop.

## Resolution

- 1(a) is what plan `2026-11-26-turret-tracking-weapon` built (merged `7189c6d9`): nearest enemy in range, re-ranked
  every tick.
- 2(b): the Immortal and Colossus analogues get the same tracking next (B-218), as an amendment to that plan (§14
  F-1), accepting small shifts in their battles under `AGENTS.md` §8.
- 3 (an in-game check) was not answered; the rule stands on the patch notes as cited.
