# Q0052: Are these six differences from StarCraft II sanctioned?

- **Asked:** 2026-11-05 (in chat, by the orchestrator)
- **Blocking:** no
- **Default until answered:** treat them as listed.

## Question

Under the standing rule [Q0051](0051-sc2-mechanics-standing-rule.md), plan
[`2026-11-05-sc2-deviation-fixes`](../../plans/2026-11-05-sc2-deviation-fixes.md) §4 lists six items as sanctioned
or as not deviations: S-1 configurable supply cap (default 200), unlimited selection, spectator zoom, RON data files
instead of StarCraft II's editor data; S-2 splash hits cloaked units (StarCraft II does this too); S-3 the
turn-before-move tolerance of one tick's turn (the data has none); S-4 a move to an unreachable point walks to the
nearest reachable one (StarCraft II does too); S-5 25 ticks per Faster second with whole-tick rounding; S-6 original
names, lore and looks.

## Answer

Owner (chat, 2026-11-05): "Confirmed." (followed by the refinement quoted in Q0051's Answer)

## Resolution

2026-11-05. S-1 to S-6 are sanctioned. Closed.

## Addendum (2026-11-20): movement contact deviations from Q0092

Sanctioned by the owner's answer to [Q0092](0092-movement-contact-redesign.md), option (a), for plan
`docs/plans/2026-11-20-contact-solver.md`:

- **D-α:** friendly bodies may overlap briefly, within the contact solver's tolerances (about 5 % of the radius sum
  while moving, 1 % at rest), which are authored in data. Never with enemies or terrain.
- **D-β:** moving friends push each other by priority, as a data switch.
- **D-γ:** the concave is reached by choosing a free slot on the weapon-range arc, not by a surround-walk mode.

## Addendum (2026-11-23): an attacking melee unit may be nudged along its ring, from Q0095

Sanctioned by the owner's answer to [Q0095](0095-engaged-units-nudged-sideways.md), option (b), for plan
`docs/plans/2026-11-20-contact-solver.md`:

- **D-δ:** allies who need room may nudge an attacking unit sideways along its attack ring. The data switch
  defaults to on. The unit is never pushed out of attack range and never by enemies. A unit on Hold Position is
  never nudged. With the switch off, StarCraft II's rule (attacking units are not pushed) applies.
