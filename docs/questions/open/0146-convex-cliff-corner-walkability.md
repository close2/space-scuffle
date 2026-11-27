# Q0146: Should units also keep off the cut corners of square cliffs (B-250)?

- **Asked:** 2026-12-02
- **Blocking:** no
- **Default until answered:** (a) only diagonals are clipped; B-250 stays open

## Question

Following your answer to Q0139, plan `docs/plans/2026-12-02-diagonal-cliff-walkability.md` clips the walkable ground of 45°
diagonal cliffs to the drawn line, so no unit floats there. The same kind of overhang exists, smaller, at every
outer corner of a square plateau (B-250): the drawing trims each such corner with a short 45° cut (one eighth of
the cell), and a small unit standing right in the corner is drawn hanging 2.84 world units over the cut (5.84 on a
two-level corner). Both maps have these corners (20 on default_arena).

## Options

- **(a) Leave square corners as they are (default).** Only diagonals are clipped. B-250 stays an open presentation
  bug.
- **(b) Clip the corners in the simulation too**, the same mechanism: units keep their radius from the short 45°
  line, so they stop about 0.09 cells earlier in a corner. This changes movement near every plateau corner, so
  battles on default_arena (the golden replays) move; it would get its own package after the diagonal one.
- **(c) Draw square corners sharp instead** (no 45° trim on convex corners). No simulation change; plateaus get hard
  right-angled corners.

Recommendation: (b), since it is the rule you chose for diagonals applied to the remaining case, after the diagonal
package has landed; (c) if you prefer to keep movement untouched.

## Answer

