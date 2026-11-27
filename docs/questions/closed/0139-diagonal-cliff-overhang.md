# Q0139: Straight diagonal cliffs draw high-ground units hanging over the cliff. Is that size acceptable?

- **Asked:** 2026-12-01
- **Blocking:** no
- **Default until answered:** (a) keep it; plan 2026-12-01-straight-diagonal-cliffs builds option (c) of Q0136 as
  answered

## Question

You chose straight 45° diagonal cliffs drawn through the low cells' corners (Q0136 (c)). Units on the low ground
are never drawn inside rock. The cost falls on the high side. The straight line runs through the middle of every
high step cell, so the outer half of each step is drawn as low floor, a full cliff level (3 world units) below
where the simulation holds units. A ground unit standing in the middle of such a cell, or at its outer corner, is
drawn hanging over the cliff edge. That is 25 of the 28 ground unit types: every type with a radius up to 0.56. The
other cliffs keep the 0.2 limit the terrain plan set (today 0.077). The review capture
`~/scratch/straight-diagonals/review/contact_sheet.png` (rows `brink_*`) shows a small unit at the corner and at the
centre of a step cell.

## Options

- **(a) Keep it (default).** The straight look as answered. The overhang is recorded as a sanctioned deviation of
  the drawn terrain.
- **(b) Square inner corners after all** (Q0136 (a), what `main` draws today): no overhang beyond 0.08, toothed
  diagonals.
- **(c) The 45° cut through the midpoints** (Q0136 (b)): straight diagonals half a cell further out. Units on the
  high side overhang by less, and units in the low nooks are drawn up to 0.2 inside the rock.

Recommendation: (a), since it is what you chose and the alternatives are the two looks you turned down; (b) if
units floating on diagonal plateaus look wrong in the capture.

## Answer

- **Fourth option (align simulation diagonal cliffs with the straight 45° edge):** After inspecting `brink_0.3125.png`, small high-ground units floating completely out over the low-ground floor looks broken, while sawtooth cliffs (`Q0136 (a)`) and low-ground rock clipping (`Q0136 (b)`) are also undesirable. Keep the straight 45° diagonal look, and plan a simulation-side follow-up that clips the walkable high-ground half-cell along the same 45° diagonal line (matching how StarCraft II achieves straight diagonal cliffs with neither floating high-ground units nor low-ground rock clipping). Track the current overhang as an open bug until that simulation-side diagonal cliff plan lands.

## Resolution

Planned in [`docs/plans/2026-12-02-diagonal-cliff-walkability.md`](../../plans/2026-12-02-diagonal-cliff-walkability.md)
(the commit that closes this question; status Proposed). The drawing keeps the straight 45° diagonals. The cut
predicate moves to `sim_map`, so drawing and simulation read one rule, and the simulation treats the triangle of a
cut cell beyond the drawn foot as wall: the navigation mesh gets the foot as a diagonal wall and blocks the triangle,
and clearance, traversal and placement follow. A body then stands its radius behind the drawn foot, as at every
axis-aligned cliff (drop 0.077 under the smallest body, against 3.0 today). Until the plan lands, the overhang is the
open bug **B-251** in `DESIGN.md` §4. The smaller overhang at square plateau corners (B-250) is asked separately in
Q0146.
