# Q0140: Where should a newly trained unit leave its building when no rally is set?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (b), keep today's rule and keep B-167 open

## Question

When a building finishes a unit, the unit appears on a free spot just outside it. With a rally set, Space Scuffle
picks the free spot nearest the rally, which StarCraft II also seems to do. Without a rally, it tries the spots
clockwise from the building's corner with the smallest map coordinates, and if all are blocked it searches wider rings.
Both rules are this project's own: StarCraft II's game data does not contain the placement search (every
ordinary training ability leaves its `Offset` at the engine default), and no guide or forum we could read
describes it. Today no recorded battle trains units, so only the skirmish match notices.

- (a) Accept today's rule as a recorded deviation, like the other sanctioned ones (Q0052), and close B-167.
- (b) Keep it as an open minor difference until someone can check the game (default).
- (c) Tell us what you see in StarCraft II (for example: "a Barracks with no rally drops units at its bottom
  edge, left to right"), and we make the start side and direction data in `structures.ron`, per building if needed.

Recommendation: (a), unless you can check in game, then (c).

## Answer

