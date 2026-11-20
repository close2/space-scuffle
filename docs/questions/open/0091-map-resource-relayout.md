# Q0091: May the two shipped maps' mineral lines and geysers be re-laid for resource footprints (B-147)?

- **Asked:** 2026-11-20
- **Blocking:** no
- **Default until answered:** yes. Plan `docs/plans/2026-11-20-resource-footprints.md` re-lays both maps as its D-10
  states, and the change can be reverted or reshaped later.

## Question

In StarCraft II a mineral field is a 2×1 obstacle and a geyser a 3×3 one. Ours are single cells that units walk
through. Plan `2026-11-20-resource-footprints` gives nodes their real footprints. That means re-laying the mineral
lines and geysers of `showcase_canyon` and `default_arena`; the exact cells are in the plan's D-10.

- **Why the geysers must move.** At their current places, 3×3 geysers leave a one-cell gap on each army's exit. In
  the prototype that caused 7,189 reversals, 31 give-ups and 4 flipped seeds. Even a three-cell gap flipped all nine
  showcase seeds. The plan puts the geysers behind the bases, where StarCraft II maps usually have them.
- **Mineral lines.** They take the game's 2×1 shape and spacing. `default_arena`'s lines are also moved off the
  cells where the `golden_replay` fixture places its halls.
- **Effect on the showcase.** In the prototype all nine winners hold. The default seed ends at tick 1,971 instead
  of 1,935, and income falls from about 92 to about 63 per worker-minute, which is the StarCraft II figure.

## Options

- **(a)** Re-lay both maps as the plan states (the default).
- **(b)** Re-lay them, but you give the layout. The work stops at WP3 until you do.
- **(c)** Do not give nodes footprints, and keep B-147 open.

## Context added by plan Amendments 1 and 2 (2026-11-20)

Measured on today's base, slot 1's geyser at (62, 53) sat beside the ground where the showcase spawns that army and
changed the battle; a first move to (59, 62) still sat beside the army's route round the north of its hall. Amendment 2
puts the geysers in the base corners: slot 1 at (53, 62) and (62, 62), slot 0 at (10, 1) and (1, 1). The default (a)
still applies, with these cells.
