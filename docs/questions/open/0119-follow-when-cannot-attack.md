# Q0119: When you A-click a unit, should the units that cannot attack it follow it, as Scan Move units do?

- **Asked:** 2026-11-26
- **Blocking:** no
- **Default until answered:** (a), they attack-move to where the target stood when you clicked (your answer to
  Q0105).

## Question

With the follow order (plan `2026-11-26-follow-order`), an A-click on a unit now makes units without any weapon
(Medivac, Raven, Observer: Scan Move) follow that unit and fight what they can on the way, as StarCraft II does.
Units that do have a weapon, just not one for this target (a Marauder A-clicked onto a Viking, an unburrowed
Lurker), still attack-move to the point where the target stood when you clicked, as you chose in Q0105. If the
target flies away, they stop at that old point.

- **(a) Keep your Q0105 answer (default).** They go to the point. It is simple and predictable.
- **(b) Let them follow too.** They follow the target and fight on the way, like the Scan Move units next to them,
  so a mixed army stays together. The game data does not say which the game does for these units.

Recommendation: (b) if you want mixed selections to move as one; (a) otherwise. Either is a one-line change in the
client.

## Answer

- **Option (b): Let them follow too.** Units that have a weapon but cannot hit the A-clicked target (e.g. a Marauder A-clicked onto a Viking, or an unburrowed Lurker) follow the target and fight valid enemies on the way, matching Scan Move units so mixed armies stay and move together (updating `Q0105` for unit-targeted A-clicks).
