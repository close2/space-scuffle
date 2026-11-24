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

- **1(c):** A unit on a ramp counts as **both** cliff levels (so a ramp never separates splash between high and low ground).
- **2(b):** Air blasts **ignore cliff levels** (air-to-air splash does not miss across a ground cliff line).
- **3(a):** The cliff level is read **at the blast's centre point**.

## Resolution (2026-11-24)

Built by plan [`2026-11-24-owner-answers-cliff-and-vision`](../../plans/2026-11-24-owner-answers-cliff-and-vision.md)
(bug B-200, fixed; [evidence](../../evidence/2026-11-24-owner-answers-cliff-and-vision.md)):

- **1(c):** a ramp cell counts as both of its landings: `GameMap::cliff_span_at` replaces `cliff_level_at` and returns
  a span of levels, and a `SameCliff` area strikes a unit whose span overlaps that of a search point reaching it. The
  Baneling analogue's direct strike on a target on another level is the exact complement. Commit `c0094e7e` (WP1).
- **2(b):** an area on the air plane and a unit in the air span every level, so air blasts ignore cliff levels. A
  Colossus analogue stands on the ground and keeps the ground's level (plan §14 F-2). Commit `b74aaa10` (WP2).
- **3(a):** already built by plan `2026-11-23-splash-cliff-levels`, commit `e839bb4c`: the level is read at each
  search point. WP1 adds the test that pins it (`the_level_is_read_at_the_blast_centre`).

No measured world, golden or sandbox checksum moved.
