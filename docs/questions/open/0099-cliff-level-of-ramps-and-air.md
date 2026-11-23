# Q0099: Which cliff level does a unit on a ramp, and an air unit, stand on?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** a ramp counts as its lower level; an air unit has the level of the ground below it;
  the level is read at the blast's centre (plan `2026-11-23-splash-cliff-levels` D-1, D-2).

## Question

In StarCraft II 5.0.14 a blast (Siege Tank, Planetary Fortress, Ultralisk, Liberator anti-air, Baneling) damages only
units on the same cliff level as its centre (`SameCliff`), and Space Scuffle is adopting that (bug B-170). The game
data do not say how three cases count:

1. **A unit on a ramp.** (a) The ramp counts as its lower level (the default: it is how our maps store ramps, and the
   Baneling's 5.0.14 fix for targets "on another level" fits a Baneling on a ramp attacking a wall at its top);
   (b) it counts as the nearer level, switching at the ramp's middle; (c) it counts as both, so a ramp never separates
   anything.
2. **An air unit.** (a) It has the level of the ground below it (the default, the literal reading); (b) air blasts
   ignore levels.
3. **Where the level is read.** (a) At the blast's centre point (the default); (b) at the struck unit the blast is
   centred on. These differ only for a target standing right at a cliff edge.

If you can, check in the editor: a sieged Siege Tank on high ground firing at a Marine standing on the top cell of a
ramp, with a second Marine one cell lower on the ramp and a third on the high ground beside the ramp's top; and a
Liberator's anti-air shot at a Viking over high ground with a second Viking over the low ground 1 apart.

## Answer

