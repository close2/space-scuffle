# Q0076: What does a player see of an enemy cloaked unit that no detector covers?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** a faint refractive shimmer where the unit is, as in StarCraft II; it cannot be
  selected, targeted, inspected or shown with a health bar. Burrowed enemies are hidden entirely either way.

## Question

Plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md) (§D-5) fixes an information leak: today an undetected enemy cloaked or burrowed unit is drawn to every
viewer. For burrowed units the answer is clear (StarCraft II shows nothing). For cloaked units StarCraft II shows a
faint distortion that an attentive player can spot ("if you can make out the distortion of cloaked units, just fire
upon that", Blizzard forum). No shipped unit cloaks yet; the engine supports it.

Options:

1. Shimmer, not selectable or targetable (default; StarCraft II's behaviour).
2. Hide entirely, like a burrowed unit.

## Answer

