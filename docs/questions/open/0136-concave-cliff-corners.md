# Q0136: At a concave cliff corner, should the drawn cliff keep its 45° cut, or follow the cells?

- **Asked:** 2026-11-30
- **Blocking:** no
- **Default until answered:** (a) square nooks; WP3 of plan 2026-11-30-terrain-and-air-indicators builds it

## Background

The simulation's cliffs run along cell edges, and a unit on low ground may stand in the corner where two cliffs
meet. The client draws such an inner corner with a 45° cut (the StarCraft II look; a staircase of single-cell steps
becomes one straight diagonal cliff), so the cliff covers a triangle of the walkable cell, up to 0.35 deep at the
corner, and a unit pressed into the corner is drawn inside the rock. Outer corners keep their 45° cut either way,
because it removes rock instead of adding it. The review capture
`~/scratch/terrain-indicators/review/wp3/contact_sheet.png` shows the canyon of `showcase_canyon` in each option.

## Options

- **(a) Square inner corners (default).** The cliff follows the cells at inner corners and keeps the 45° cut at
  outer corners. No unit is ever drawn inside rock. Diagonal cliffs look toothed: chamfered steps with square nooks.
- **(b) Keep the 45° cut** as a sanctioned deviation. Diagonal cliffs stay straight; units in inner corners are drawn
  up to about 0.2 inside the rock.
- **(c) Straight diagonals moved onto the high side.** Keep the straight 45° look by drawing each diagonal through the
  low cells' corners, so it lies entirely in high ground. Low units are never cut; units on the high plateau stand
  up to half a cell over the drawn edge. It needs a wider rule than one dual cell and its own plan.

Recommendation: (a), because it is exact and local; (c) if the straight canyon look matters more than a later plan's
cost.

## Answer
