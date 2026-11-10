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

**Option 1 (Faint refractive shimmer for undetected cloaked enemies, not selectable or targetable), plus StarCraft II's visibility cues for burrowed enemies.**

**Engine & gameplay insight:** In StarCraft II:
1. **Undetected cloaked enemies:** Render as a subtle translucent refractive distortion ("shimmer") while moving or standing, and cannot be selected, targeted, inspected, or shown with a health bar.
2. **Undetected burrowed enemies:** While a stationary, non-attacking burrowed unit hides its unit sprite to an enemy without detection, several burrowed states **are** visually noticeable in SC2 even without a detector:
   - **Moving burrowed units** (such as a Roach tunneling underground with *Tunneling Claws* or a burrowed Infestor moving) leave a visible dark subterranean dirt-ripple / ground-disturbance trail along the terrain so an attentive player can spot their underground movement and land AoE spells on them.
   - **Attacking burrowed units** (such as the Lurker / `burrow_scorpion`) visibly erupt their line of subterranean spines (`spine_line`) starting from the burrowed attacker's position toward the target, revealing the Lurker's location on the ground via its attack effect even when the Lurker itself is undetected.
   - (And in SC2 LotV, units like the Widow Mine show a faint surface disturbance / targeting cue when acquiring.)
Please ensure undetected cloaked units show the refractive shimmer, stationary non-attacking burrowed enemies stay hidden, and burrowed enemies that move underground or fire (like the Lurker's spine line) show their ground disturbance / attack origin to the enemy player.
