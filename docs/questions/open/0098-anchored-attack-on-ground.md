# Q0098: What does a sieged tank, burrowed Lurker or armed structure do with Attack on the ground?

- **Asked:** 2026-11-23
- **Blocking:** no
- **Default until answered:** (a), the click is refused for units that cannot walk.

## Question

Units that cannot walk (a sieged Siege Tank, a burrowed Lurker, a Photon Cannon) have an Attack button in
StarCraft II 5.0.14. Plan `2026-11-23-stance-checked-orders` makes it work on enemy units: A, then click an enemy,
attacks it. The game data does not say what happens when such a unit is told to attack a point on the ground. Its
button even has its own face, `AttackBuilding`, whose tooltip is not in the local data.

- **(a) Refused (default).** For a selection of only such units, the click does nothing and the status line says no
  unit can do that. The Attack cursor stays on, so you can click an enemy next. Walking units in a mixed selection
  still attack-move.
- **(b) Accepted as "attack-move in place".** The unit drops its current orders, including a focused target, and
  attacks whatever is in range, as after Stop. This needs a simulation change in movement admission, which waits
  for the movement work under way.

Recommendation: (a), unless you know the game does (b). Either way it is small: it only matters when every
selected unit is anchored.

## Answer

- **Option (a): Refused.** For a selection of only anchored units/structures (sieged Siege Tank, burrowed Lurker, Photon Cannon, Spine Crawler, etc.), clicking empty ground with Attack does nothing, reports on the status line that no unit can do that, and keeps the Attack cursor active so the player can click an enemy next. Walking units in a mixed selection still attack-move normally.

**Reasoning:** Matches StarCraft II verbatim — anchored units and armed structures cannot attack-move and cannot fire at empty ground (SC2 displays *"Must target a unit"* and leaves the attack cursor armed), which also prevents a slight misclick next to an enemy unit from silently cancelling an active focus-fire order.
