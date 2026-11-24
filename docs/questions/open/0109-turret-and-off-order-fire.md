# Q0109: Turrets while firing and between shots, and weapons that fire off their attack order

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** the Hellion's turret does not turn from the start of an attack to the end of its
  follow-through; every other turret follows its target between shots; a Phoenix, Immortal, Colossus or Planetary
  Fortress that cannot fire at its attack order's target (out of range or out of sight) fires at the best enemy it
  can reach, but always fires at the order's target when it can, and the Immortal and Colossus only while standing;
  a Planetary Fortress ordered to attack a unit out of range or out of sight drops the order at once (plan
  `2026-11-24-fire-rules` D-2 to D-5).

## Question

Space Scuffle is adopting three StarCraft II 5.0.14 weapon options. The game data name them but not exactly what they
do:

1. **The Hellion's locked turret** (`LockTurretWhileFiring`). While a Hellion flames, its turret does not turn. For
   how long: (a) from the start of the attack to the end of its follow-through, about 1 second (the default); (b)
   only until the flame comes out, about 0.25 seconds; (c) until its next attack starts?
2. **Turrets between shots** (`FaceTargetWhileInCooldown`). While a Siege Tank or Immortal waits to fire again, does
   its turret (a) keep following its target, so the next shot comes as soon as it is ready (the default); or (b) stay
   where it fired until it may fire again?
3. **Firing at another unit during an attack order** (the Phoenix's, Immortal's, Colossus' and Planetary Fortress'
   "Only Fire At Attack Target" is off). A Phoenix ordered to attack a Mutalisk that flies away passes an Overlord.
   (a) It fires at the Overlord only while the Mutalisk is out of range, and at the Mutalisk whenever it can (the
   default); (b) it fires at whichever of the two it ranks best, even with the Mutalisk in range.
4. **A Planetary Fortress ordered to attack a unit out of its range.** (a) The order is dropped at once (the
   default, as for every immobile unit now); (b) the order stays, and the Fortress fires at other enemies in range
   until the unit comes into range or dies.

If you can, check in a custom game: a Hellion flaming a Zergling that runs across it; a Siege Tank's turret against
a Marine walking around it; a Phoenix ordered onto a fleeing Mutalisk past an Overlord; a Planetary Fortress ordered
onto a unit out of range with Zerglings at its gate.

## Answer

