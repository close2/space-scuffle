# Q0104: Does "A, then click my own unit" attack it?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a), the selected units attack it; a unit never attacks itself.

## Question

In StarCraft II you can force your units to attack one of your own, for example to kill a stuck unit or
deny a building, by pressing A and left-clicking it. The 5.0.14 data supports this. The attack ability
leaves out your own and allied units only when units pick targets by themselves and on a right click. Ordinary
weapons accept your own units, and the few that must not (Point Defense Drone, Widow Mine, Planetary
Fortress) exclude them by name. No StarCraft II page we found says it outright.

Plan `2026-11-24-attack-orders` makes it work. Today the click only attack-moves to where the unit stands.

- **(a) Attack it (default).** Units that can hit it attack it. The others walk to it. The clicked unit itself
  does nothing even if it is selected.
- **(b) Never attack your own units.** The click on your own unit attack-moves to its position, as now.
- **(c) As (a), and a selected unit may attack itself.** The data does not rule it out for ordinary weapons,
  and two weapons exclude "self" by name, which hints that it is otherwise possible. We do not know the game
  does it.

Recommendation: (a). Right-clicking your own unit is unchanged in every case.

## Answer

- **Option (a): Attack it.** Units that can hit the clicked friendly unit attack it, the others walk to it, and the clicked unit itself does nothing even if selected. Right-clicking your own unit remains unchanged.

**Reasoning:** Matches StarCraft II's standard force-attack mechanic (used to kill stuck units, deny buildings, etc.).

## Resolution

Answered (a), the default, which was already built: plan `2026-11-24-attack-orders` WP2/WP3, merged to main in
e313268f (B-175). Units that can hit the clicked own unit attack it, the others walk to it, and the clicked unit
ignores the order. Nothing further to change.
