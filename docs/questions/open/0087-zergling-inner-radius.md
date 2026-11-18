# Q0087: Does the Zergling meet terrain and structures at its Radius (0.375), or as a point?

- **Asked:** 2026-11-17
- **Blocking:** no
- **Default until answered:** reading (a): an unset `InnerRadius` means no separate terrain radius, so the Zergling
  analogue (`razor_mantis`) keeps meeting terrain and structures at its collision radius 0.375. No data change for
  it; plan `docs/plans/2026-11-18-inner-radius.md` D-4.

## Question

StarCraft II gives every ground unit a second radius, `InnerRadius`, for "collision with terrain / structures"
(Blizzard, 5.0.13 patch notes). A unit meets terrain and structures at `min(InnerRadius, Radius)` and other units
at `Radius` (SC2Mapster threads 161926, 175792, 155862). Plan `2026-11-18-inner-radius` authors this as the catalog
field `terrain_radius` for 17 ground archetypes, from the 5.0.14 game data (SC2GameData `5444e12f13`).

The 5.0.14 Zergling is the one ground unit whose value the data does not show:

- Its `UnitData.xml` block (liberty:7563) sets no `InnerRadius`, and the `CUnit` class default (core:41-83) sets
  none either (it sets only `InnerRadiusSafetyMultiplier` 1, core:82). The engine therefore uses its schema
  default, which the data does not contain.

Two readings exist:

- **(a) Unset means no separate terrain radius.** The Zergling meets terrain at its `Radius` 0.375, like the Marine.
  This is today's behaviour and makes no unsourced claim.
- **(b) Unset is 0.** Under the `min` rule the Zergling would then be a point against terrain and structures.
  Community advice to "set inner radius to 0" so that units pass through buildings (SC2Mapster 161926) points this
  way. Against it, the same data gives `ChangelingZergling` (`parent="Zergling"`) an explicit `InnerRadius` 0.375
  (liberty:12004), which suggests the designers wanted a nonzero value for the shape.

**Recommendation: (a)**, accepted as a recorded minor difference under charter §8. Reading (b) would be a one-line
data change plus a loader rule allowing an authored 0 meaning "centre point against terrain", with its own
measurement (matrix, goldens, showcase), and would need an in-game or editor observation to source it.

Options:

1. Accept (a) as a recorded minor difference (default).
2. Direct research into (b): an editor or in-game test of a Zergling against a wall or a building gap.

## Answer

