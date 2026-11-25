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
