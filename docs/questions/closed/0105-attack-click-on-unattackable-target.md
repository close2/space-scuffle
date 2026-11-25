# Q0105: What does a unit do when you A-click it onto a target it cannot attack?

- **Asked:** 2026-11-24
- **Blocking:** no
- **Default until answered:** (a), it attack-moves to where the target stands.

## Question

Some units have an Attack command but no weapon for a given target: the Lurker before it burrows
(Attack but no weapon at all), or a ground-only unit A-clicked onto an air unit. Units with no attack at
all (Medivac, Raven, Observer) have Scan Move in the same slot, and the data says they move. For the first
group, the data does not say what happens.

- **(a) It attack-moves to where the target stands (default).** The army moves together, and the unit
  fights whatever it can hit on the way.
- **(b) It ignores the click and keeps its current orders.** The other selected units still act.

Recommendation: (a), unless you know the game does (b). It only matters for mixed selections or the
unburrowed Lurker.

## Answer

- **Option (a): Attack-move to where the target stands.**

**Reasoning:** Matches StarCraft II — in mixed selections (e.g. Marines + Marauders A-clicking an air unit) or with an unburrowed Lurker, units without a weapon for that target attack-move toward that location and engage valid targets along the way rather than standing idle.

## Resolution

Already in place: the default (a) was built by plan `2026-11-24-attack-orders` (D-5) and merged as `e313268f`.
Nothing further changes.

Superseded for unit targets by [Q0119](0119-follow-when-cannot-attack.md) (b): a member with Attack that cannot
attack an A-clicked mobile unit follows it with Scan Move (`Follow { AcquireMove }`, plan `2026-11-26-follow-order`)
instead of attack-moving to the point. An A-clicked structure is still attack-moved to.
