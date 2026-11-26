# Q0135: What should the Baneling analogue cost, now that the other trained morphs carry their whole cost?

- **Asked:** 2026-11-29
- **Blocking:** no
- **Default until answered:** (a) 50 minerals, 25 gas, 1 supply

## Background

Our Brood Lord, Ravager and Lurker analogues are trained directly (no morphing exists yet), so they now carry their
5.0.14 totals (B-239, B-86): the parent unit's cost plus the morph's. The Baneling analogue (`volatile_tick`) still
carries only the morph's increment: 25 minerals, 25 gas, 0 supply. Its 5.0.14 total is 50 minerals and 25 gas
(`liberty` UnitData:9365–9366) for half a supply (`Food -0.5`, :9315), but our supply is a whole number, and our
Zergling analogue (`razor_mantis`) is one Zergling priced as StarCraft II's pair (50 minerals, 1 supply). So the
Baneling's "whole" cost depends on a convention, not on data.

## Options

- **(a) 50 / 25 and 1 supply (default).** StarCraft II's total cost; supply rounded up to a whole unit, as our Zergling's.
- **(b) 75 / 25 and 1 supply.** One of our Zerglings (50 / 1) plus the morph (25 / 25).
- **(c) Keep 25 / 25 and 0 supply** until a morph model exists (WP17), as recorded today.

Recommendation: (a). No measured battle trains a Baneling, so no outcome moves either way.

## Answer
