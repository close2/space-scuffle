# Q0145: When a unit is created on the battlefield, may it appear on top of other units?

- **Asked:** 2026-11-27 (plan `2026-11-27-missile-throws-and-escorts` D-9, bug B-236)
- **Blocking:** no
- **Default until answered:** (b). Today's rule stays: a created unit's spot is searched against terrain and
  buildings only, and B-236 stays open as a minor difference to check.

## Question

When a Brood Lord analogue's missile lands, it creates a Broodling at the target, searching up to 5 world units for a
free spot (`BroodlingEscortImpactA`, `SpawnRange 5`, `liberty` EffectData:2684–2692). Space Scuffle's search only
avoids cliffs, water and buildings. If the spot holds other units, the Broodling appears overlapping them and the
normal movement pushes them apart on the next tick.

StarCraft II runs a "placement check" for such units (the `Placement` flag, on by default, `core` EffectData:41–55).
The game data says that check reads footprint layers, which only buildings and terrain have (`core` MoverData:8–12).
Nothing in the data or any guide we found says whether other units also block the spot. The structure-death Broodlings
turn the check off (`Placement 0`), so this question affects only units created by a check, today the Brood Lord's.

- (a) Accept today's rule as what the data says and close B-236.
- (b) Keep it as an open minor difference until someone can check the game (default).
- (c) Tell us what you see in StarCraft II (for example: "Broodlings that land in a clump of marines appear beside
  them, never inside"). If units block, the spawn search will skip spots where a unit stands.

Recommendation: (a), unless you can check in game, then (c).

## Answer

- **Option (c): Skip spots occupied by other units during spawn placement** (for battlefield spawns with placement enabled, such as Brood Lord Broodlings), so spawned units appear *beside* existing units and never inside them.

**Reasoning:** Matches StarCraft II (Broodlings land on the nearest free non-colliding spot beside targets, forming a surround rather than spawning inside Marines/Tanks) and preserves the new movement contact layer's zero-enemy-overlap / no-enemy-shove invariant on tick 0.
