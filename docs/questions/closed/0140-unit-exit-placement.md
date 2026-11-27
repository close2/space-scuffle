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

- **Sanctioned symmetric deviation (record in `Q0052` and close `B-167`):** When no rally is set, start the exit placement search at the building perimeter spot **nearest to the centre of the map** (instead of the lowest-coordinate corner) and proceed clockwise from there (then wider rings if blocked).

**Reasoning:** Deliberate fairness departure from StarCraft II (where fixed-side spawning gives asymmetric advantages depending on spawn position/base placement). Orienting unrallied spawns relative to the map centre makes symmetric spawn positions behave identically and spawns units facing toward the battlefield rather than trapped against a back edge.

## Resolution

2026-11-27. A sanctioned symmetric deviation, recorded in [Q0052](0052-sanctioned-sc2-deviations.md) as
S-exit-order. Plan `docs/plans/2026-12-01-rallies-and-smart-command.md` Amendment 1 builds it on branch `rallies-wp3`:
with no rally, `production/exits.rs` tries a ring's cells clockwise from the cell whose centre is nearest the map's
centre (the map's width and height halved, in fixed point; a tie goes to the cell earlier in the ring's clockwise
walk from its minimum corner), then wider rings; with a rally, nearest the rally as before. B-167 is removed from the
bug register. Closed.
