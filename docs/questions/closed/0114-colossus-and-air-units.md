# Q0114: Should a colossus touch air units?

- **Asked:** 2026-11-25
- **Blocking:** no
- **Default until answered:** (a), no contact between the colossus analogue and air units, as today (B-211 open).

## Question

In StarCraft II 5.0.14 the Colossus' collision flags are `Colossus`, `Structure` and `Flying` (`liberty`
UnitData:4066–4068). Plan `2026-11-25-collision-classes` gives it its own collision class, so ground units walk
beneath it and colossi block only colossi. The `Flying` flag would also make it touch air units (Vikings, Phoenixes,
Medivacs above it), but a Blizzard developer answer from before release says the Colossus does not block flying units,
and our movement keeps ground and air bodies apart by design.

- **(a) No contact with air units (default).** Simple; matches the developer answer; B-211 stays registered as an
  unverified difference.
- **(b) Contact with air units.** Needs a collision model where one body touches two kinds of bodies (flag sets),
  which changes how steering lists are built; a separate plan.

Recommendation: (a).

## Answer

- **Option (a): No contact with air units.** Keep ground and air collision planes separate; Colossi block only Colossi while ground units walk beneath them, and air units fly over Colossi without physical push/collision.

**Reasoning:** Matches actual StarCraft II gameplay and Blizzard's developer statement (air units such as Vikings, Phoenixes, and Medivacs fly freely over Colossi without colliding; the `Flying` plane flag is for anti-air weapon targeting, not physical air-body contact).

## Resolution

Answered (a), the default already built: collision classes (plan `2026-11-25-collision-classes`, merged `c5f55598`)
keep the colossus analogue and air units in different classes, so they never touch. B-211 is closed as the owner's
reading (the `Flying` flag is for targeting, not air-body contact). The ground half (units walking beneath a colossus)
is Q0117.
