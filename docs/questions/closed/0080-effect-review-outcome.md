# Q0080: Continue, change the approach for, or leave out the two effects that failed review?

- **Asked:** 2026-11-10
- **Blocking:** no
- **Default until answered:** no further effect review rounds. The effect framework, the burrow leak fix (B-106)
  and the shell impact ship; the acid burst and the spine line stay defined in `assets/catalog/effects.ron` but no
  unit uses them, and the scale-out of the effect plan (WP7 onwards) waits for this answer.

## Question

Plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md) gates its scale-out on an
independent quality review of three effects ("no cheap looking effects"), with a stop after two failed rounds
(Amendment 1). The outcome:

- **Shell impact** (the sieged Siege-Tank analogue's splash): rejected in round 1, **accepted** in round 2.
- **Acid burst** (Baneling analogue): **rejected twice.** Round 2: the stain is one flat green value with evenly
  spaced dots, and the pop reads as a glossy button rather than a burst.
- **Spine line** (Lurker analogue): **rejected twice.** Round 2: the curved spines read as a row of handwritten
  glyphs, the clumps are sparse, the line ends are cut by hard edges and there is no focal point.

Reviews: [round 1](../../evidence/effects/review/independent_review.md),
[round 2](../../evidence/effects/review/independent_review_round2.md). Contact sheets:
[shell impact](../../evidence/effects/review/shell_impact_contact.png),
[acid burst](../../evidence/effects/review/acid_burst_contact.png),
[spine line](../../evidence/effects/review/spine_line_contact.png).

Options:

1. **A third round** for the acid burst and the spine line, working from the round-2 reviewer's ranked changes.
2. **Change the approach,** for example owner-provided art direction or reference images for these two looks, or
   another texture source, before any further round.
3. **Leave them out:** the Baneling and Lurker analogues keep the default combat looks, and the scale-out (other
   weapons, modes, statuses, areas) resumes with the framework as it is.

The default above holds meanwhile.

## Answer

**Option 1 — Continue with further review rounds (up to 5 rounds total before asking again) for `acid_burst` and `spine_line` working from the reviewer's ranked fixes, and unblock the WP7+ effect scale-out.**

**Visual & technical insight:** Both `acid_burst` and `spine_line` already passed 5 of 7 criteria in Round 2 (readability, multi-layer composition, timing/anticipation/retraction, crowd clarity, and fog clipping), and the Round 2 independent review isolated concrete, actionable fixes for the remaining two items:
1. **`spine_line`:** Replace the C/J-curved hook silhouettes (which read like handwritten glyphs at default zoom) with straight or gently tapering conical chitin spikes (3–5 splayed outward per clump, taller at `0.6–0.9` units); brighten the bone-ivory tip and rim light on the newest erupting clump for its first 2–3 frames to create a clear travelling focal point; and feather the churned-earth/fissure decal over `~0.3` units at both ends while breaking up the regular 1-unit leftover marks into an irregular scar.
2. **`acid_burst`:** Drive the interior of the ground stain with noise-based value and thickness variation plus a darker, more saturated wet rim and upper-left key-light sheen (removing the flat felt-disc fill and evenly spaced polka-dot speckles), start drying toward olive earlier (`~0.6s`), and replace the bevelled button-like rim on the frame 0–2 pop with a ragged noise-broken burst and larger airborne globs.
You may iterate up to **5 review rounds total** before pausing to ask again, and proceed with the WP7+ effect scale-out.

## Resolution

Recorded in plan [`2026-11-09-effect-visualization`](../../plans/2026-11-09-effect-visualization.md), Amendment 2
(commit `c5b5de2`): the stop rule is five review rounds; round 3 for `acid_burst` and `spine_line` works from
round 2's ranked changes (commit `ca082cd`, packet
[`independent_review_packet.md`](../../evidence/effects/review/independent_review_packet.md), section "Round 3").
The scale-out (WP7 onwards) is unblocked; both effects return to their units once a round accepts them.
