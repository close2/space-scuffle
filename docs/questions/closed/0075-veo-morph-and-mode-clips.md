# Q0075: May agents spend Veo clips on morph and mode unit animations (siege, burrow)?

- **Asked:** 2026-11-09
- **Blocking:** no
- **Default until answered:** no spend. The effect system of plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md) carries the burrow and siege reads (sinking
  shell, dirt mound, seal sweep), and each unit's clip stays the one of its default stance, a recorded limitation.

## Question

Units with a mode (the Siege-Tank analogue `railgun_crawler` in `siege_mode`, the Lurker analogue
`burrow_scorpion` in `burrowed`) play the same Veo flipbook in both stances: a sieged tank looks like a mobile
tank, and the morph itself has no animation. Plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md) (§D-4, WP13) can add atlas slices keyed by animation state and
mode: about 12 clips plus 5 retries, **$6.80** of the $200 Google-media limit (the ledger stood at $96.40 on
2026-11-09).

Options:

1. No spend (default). The effects carry the read; the unit clip does not change with the mode.
2. Spend up to $6.80 on WP13 once the effect slice is accepted.

Recommendation: option 2 after the slice, because a sieged tank that still looks mobile is the larger remaining
readability gap.

## Answer

**Option 2 — Spend up to $20.00 (target < $10.00, e.g., the planned ~$6.80) on WP13 for morph and mode unit animations (siege, burrow).**

**Engine & gameplay insight:** In StarCraft II, telling at a glance whether enemy Siege Tanks are in Tank Mode or Siege Mode (and seeing the 3-second siege/unsiege transformation animation) is critical for tactical decision-making and combat readability. Baking dedicated Veo mode and morph flipbook slices (targeting < $10, with a ceiling of up to $20 if extra retries are needed) is well within the remaining media budget and closes this readability gap.

## Resolution

Carried out by WP13 of plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md)
(Amendment 10), branch `effects-wp13`, 2026-11-12. The sieged `railgun_crawler` and the burrowed `burrow_scorpion`
each have a mode atlas (Idle and Attack in the mode, and the enter and leave morphs), shown by the client from the
published stance and played over the mode switch's channel. 11 Veo generations: 8 accepted, 3 rejected in review;
**$4.40**, the ledger going from $106.45 to $110.85, within the target of $10.00. Evidence and the accept/reject log:
[`docs/evidence/veo/2026-11-12-mode-clips.md`](../../evidence/veo/2026-11-12-mode-clips.md).
