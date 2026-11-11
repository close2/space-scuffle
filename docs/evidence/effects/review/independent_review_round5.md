# Independent Review of the Effect Vertical Slice, Round 5 (final)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Round 5"). Earlier rounds:
[`independent_review.md`](independent_review.md), [`independent_review_round2.md`](independent_review_round2.md),
[`independent_review_round3.md`](independent_review_round3.md), [`independent_review_round4.md`](independent_review_round4.md).
Art direction: [`../reference.md`](../reference.md) ("Art direction"). Checklist: plan §7.2, as quoted in the packet.

The reviewer judged only the packet, the reference board, the plan's checklist and the images. That means the three
contact sheets and sequences with their legends, the fog sheet and fog sequence, the crowd capture with
`stress_capture.txt`, and the `fx_acid` texture sheet. For comparison the reviewer also used round 4's sheets, the
shell sequence and the crowd frame from commit `b72b693`. Cells were cropped and enlarged with nearest-neighbour
scaling, 1.5x to 7x. Sizes were measured as pixel runs along the centre row of a cell. A stain pixel is one whose
3-row average differs from the cell's clean ground by more than 25 levels in any channel. The reach is the
outermost red pixel of the reference circle in the row below, on the same scan line. Colours are 3×3 averages of
the sRGB image. Nothing was built or run. The reviewer did not read the code or the implementer's verdicts
(`slice_review.md`).

Cell names are as before. Rows 1 to 8 follow the packet's list: 1 bright default, 2 its reference, 3 bright close,
4 its reference, 5 dark default, 6 its reference, 7 dark close, 8 its reference. Columns are named by the age in the
legend. Sequence frames are numbered from 0 and are 40 ms apart. "Sheet px" are pixels of the 2560×1440 sheet. At
default zoom 1 sheet px is 2 game px; at close zoom it is 3.

The bar is the owner's: "no cheap looking effects". **All three effects are accepted.** `shell_impact` and
`spine_line` did not regress, and the spine line's three polish notes are fixed. `acid_burst` meets both of round
4's blocking points. The stain's visible edge now lies on the reach, within 1 sheet px (≤ 2 game px) at every age
from 0.18 s to 3.0 s, on both grounds and both zooms. No pale band shows between stain and lip in any frame. The
fix has a price. The stain lost the organic, ragged puddle edge and the dark wet rim that round 4 praised. It is
now a clean, slightly wavering ellipse, and in the crowd the stains read as stacked green discs. I do not judge this
cheap at gameplay zoom: the Critical layer's silhouette is meant to be the gameplay circle (reference board, "Shape
language"). It is the first thing to polish, ranked below.

---

## 1. `shell_impact` (sieged artillery): **accept** (no regression)

All eight contact-sheet columns are pixel-identical to round 4 (ImageMagick `compare`, fuzz 3 %, 0 differing
pixels per column). The sheet's first age after the event is 0.241 s, after the mark's new start at 0.12 s. Only
sequence frames 1–2 changed. Inside the racing shock ring the ground is now clean bright ground instead of round
4's grey veil (enlarged crop of frames 0–3, round 4 against round 5).

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The shock ring sits on the yellow reach (row 4, 0.241 s; sequence frames 1–6). The char core fills the red 0.47 circle (row 4, 2.4–6.0 s). One focal point: the flash, then the fireball. |
| 2 | Layered | pass | Flash, shock ring, fireball, debris chunks (row 3, 0.241 s), smoke (0.642–1.2 s), char core and streaked blast mark. |
| 3 | Timing | pass | The peak is at frames 0–1 and the fireball cools over frames 2–13. The mark erodes, and every row is clean at 8.03 s. |
| 4 | No tells of cheapness | pass (with notes) | The veil inside the ring in frames 1–2 is gone. The streak fringe is still a regular sunburst (row 3, 2.4 s), deferred to scale-out. It does not show at gameplay zoom. |
| 5 | Value and colour | pass | The flash and core are the brightest values. The ramp runs yellow, orange, oxblood, then smoke. The char is deep brown-black. |
| 6 | Crowd | pass | Small orange sparks and scorch dots that never hide a unit. |
| 7 | Fog | pass | Nothing is drawn away from the impact point. |

**Round-4 improvements**

| Improvement | Status | Evidence |
|---|---|---|
| 1a: mark only after the ring has passed | **resolved** | Sequence frames 1–2: no grey inside the ring. |
| 1b: irregular streak fringe | not (deferred to scale-out) | Row 3, 2.4 s. |
| 2: more ring-width variation | not (deliberately left) | Sequence frames 2–6 as in round 4. |

**Remaining improvements, ranked** (polish only)

1. Make the streak fringe irregular with a new blast-mark texture at scale-out.
2. Give the ring about 2x width variation, with one or two breaks.

---

## 2. `acid_burst` (bursting creature): **accept**

### Measurement: stain edge against the reach

Pixel runs along the centre row (the scan line of the reference circle's widest span):

| Ground, zoom | Reach (sheet px) | 0 s | 0.18 s | 0.48 s | 0.9 s | 1.8 s | 3.0 s | 4.5 s |
|---|---|---|---|---|---|---|---|---|
| bright, default (row 1/2) | 87 | 0.83 | 0.98 (−1/−1) | 0.98 (−1/−1) | 0.98 (−1/−1) | 0.98 (−1/−1) | 0.98 (−1/−1) | 0.76 (drying) |
| dark, default (row 5/6) | 87 | 0.84 | 0.99 (−1/0) | 0.99 (−1/0) | 0.99 (−1/0) | 0.99 (−1/0) | 0.99 (−1/0) | 0.76 (drying) |
| dark, close (row 7/8) | 135 | 0.84 | 1.00 (0/0) | 0.99 (0/−1) | 0.99 (0/−1) | 0.99 (0/−1) | 0.99 (0/−1) | 0.77 (drying) |

The values in brackets are the left and right offsets in sheet px from the circle's outermost red pixel, where
negative means inside. On bright close (row 3) my automatic threshold was confused by the ground texture, so I read
the edge pixels directly. At y = 450, from 0.18 s to 3.0 s, the transition from ground (≈ 178,170,160) to stain
(≈ 40–83, 86–112, 36–59) runs over 2–3 px. It is centred on the circle's outer pixel, one pixel inside it, on both
sides. That is within ±1 sheet px, which is ±2 game px at default zoom. Round 4 measured 0.87 here. **The edge
noise is fine:** at default zoom the waver is invisible, and at close zoom it shows as a slightly irregular outline.

### Measurement: band between stain and lip (sequence, default zoom, centre row, luminance outward to inward)

| Frame | Left edge, outside → inside | Right edge, inside → outside |
|---|---|---|
| 2 | 168, 144, 113, 82, **71, 67, 66**, 68, 70, 72, 75, 77 | 58, 60, 63, 66, 67, 68, 67, 66, **64, 63, 65**, 92, 127, 160 |
| 4 | 166, 137, 101, 71, **66**, 68, 70, 73, 77, 79, 80, 80 | 60, 62, 64, 66, 68, 69, 69, 68, **66, 65**, 81, 115, 150, 167 |
| 8 | 166, 137, 102, 73, **68**, 71, 73, 77, 81, 83, 83, 83 | 62, 63, 65, 66, 68, 69, 69, 69, 68, **66, 65**, 83, 117, 152 |

The ground falls straight to the stain within 2–3 px. A 1–3 px band just inside is *darker* than the body (the lip;
bold), and nothing is lighter or see-through at any frame from 1 to 10. Round 4 found a band of luminance 120–154
there. The stain is 64–65 sequence px wide from frame 2 to frame 10, so nothing contracts when the front leaves.

### Checklist

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | From frame 1 the emerald disc covers the reach, and the Critical stain holds the true size within ±2 game px until 3.0 s (tables above). One focal point: the hot core under the crown in frames 1–9. The crown, globs and fumes stay inside the disc and do not compete. |
| 2 | Layered | pass | Stain, lip, splash crown (1.3 of the reach, larger and more torn than round 4's tuft; row 3, 0.18 s), hot core, three glob shapes, fumes, then the drying crust. |
| 3 | Timing | pass | The peak is at frames 1–2. The size holds without contraction (frames 2–10 at 64–65 px). The stain dries by erosion after 1.8 s: ragged bites at 4.5 s, clean at 6.0 s on every row. |
| 4 | No tells of cheapness | pass (with notes) | (a) **Resolved:** no pale band between stain and lip (above). (b) **Resolved:** the globs are no longer identical pale-lime commas; they vary in shape and size and are dark with a small highlight. (c) **Resolved:** the fumes are smaller, lime-tinted puffs, not a white smoke column (row 3, 0.48–0.9 s). Notes, none blocking at gameplay zoom: (d) **the edge lost its character.** Round 4's ragged puddle edge, with its dark wet rim and lit meniscus, is replaced by a clean ellipse (round 4 against round 5, row 3, 0.9–3.0 s). The body is close to uniform once dry: at 1.8 s the centre row's luminance runs 68–84, a gentle left-to-right key-light gradient with soft mottling, and only the bottom edge is darker (65 against 77). At default zoom the dried stain reads as a smooth olive oval, a little like a felt disc. (e) A **pale double streak** runs just inside the upper-left edge at every age from 0 to 3.0 s (row 3; faint in row 1). It reads as the stain texture's own inner rim (the `fx_acid` sheet, `acid_stain` row, shows concentric rims), now cut inside the disc outline, like a ghost seam. (f) The landed globs read as **thin dark glyphs on the stain**, like a "J", an "S" or a "13" (row 3 and row 7, 0.18–0.48 s; tiny "13" ticks in sequence frames 10–14). The torn-neck glob seen edge-on draws as a pen stroke. (g) Frame 0 (row 3, age 0) shows a concentric inner disc with white sparkle dots, a saucer look, for one frame only. |
| 5 | Value and colour | pass | The core is the brightest point, and lime appears only on the core, crown and highlights. The body is wet emerald (≈ 40–51, 104–113, 36–46) at 0.18–0.48 s, olive-green at 0.9 s (≈ 62–77, 92–101, 49–58) and olive-brown at 1.8–3.0 s (≈ 70–83, 79–93, 53–62). It is darker than both grounds and never black. |
| 6 | Crowd | pass (with note) | Every unit stays readable on the stains. See the crowd section: the stains now read as overlapping clean discs, and there is more green than in round 4. |
| 7 | Fog | pass | Nothing is drawn outside the reach at any age. The edge is on the reach, not beyond it (largest overshoot 0 sheet px on the centre row). |

**Round-4 findings**

| Finding | Status | Evidence |
|---|---|---|
| rc1 / item 1: visible edge at 0.87 of the reach | **resolved** | 0.98–1.00 at 0.18–3.0 s on all four rows measured; offsets 0 to −1 sheet px. |
| rc2 / item 4a: pale see-through band, frames 1–9 | **resolved** | Sequence profiles above: the lip is darker than the body; nothing lighter. |
| item 3: 8 % contraction at frames 7–10 | **resolved** | 64–65 px from frame 2 to frame 10. |
| rc3 / 4b: one teardrop glob sprite | **resolved, with a new note** | Three shapes, dark bodies. Landed and edge-on, they read as dark glyphs (note 4f). |
| rc4 / 4c: white fumes | **resolved** | Row 3, 0.48–0.9 s: small lime-tinted puffs. They are still a little cauliflower-like at close zoom. |
| rc5 (optional): small "Y" crown | **partly** | Larger and more torn (row 3, 0.18 s). It is still a central fountain shape, and that is acceptable. |
| Round 4's resolved rim and relief (rc1 of round 3) | **partly regressed** | The dark wet rim and the lit ragged edge are mostly gone (note 4d). The relief inside is softer. |

**Remaining improvements, ranked** (polish; none blocks acceptance)

1. **Give the disc's edge material character again, inside the outline.** Draw a dark wet meniscus band about 0.05
   to 0.1 of the radius wide, with a broken specular highlight on the upper-left side, inside the disc outline.
   While drying, add a slightly lighter, cracked crust ring. The outline stays on the reach; only the inner band
   changes. That restores round 4's puddle read without moving the Critical edge.
2. **Remove the ghost inner rim** (the pale double streak inside the upper-left edge), for example by masking the
   stain texture's own rims or choosing a texture region without them.
3. **Globs on the ground:** splat landed globs into short blots, or fade them on landing, so no thin dark stroke
   lies on the stain. Avoid the edge-on neck variant at small sizes.
4. **Crowd look:** vary each stain's interior (rotation, pooling offset, drying phase) and soften where stains
   overlap, so that a field reads as one liquid and not as stacked discs. Recheck the green coverage (below).
5. More dried-crust texture from 1.8 s (cracks, pooling), so the dried oval is not a nearly flat fill.
6. Frame 0: drop the concentric inner disc and the sparkle dots.

---

## 3. `spine_line` (burrowed spine attacker): **accept** (no regression; polish notes fixed)

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | Found at once on both grounds (rows 1 and 5). Clumps stand on the red circles from the first point to the last (rows 2 and 4, 0.973 s). The newest clump is the focal point while the front travels (sequence frames 3–22). |
| 2 | Layered | pass | Tear and heave ahead of each clump, the clumps, dust, the scar and the fissure. |
| 3 | Timing | pass | The front advances one point every 2–3 frames. The spines sink from the source end (sequence rows 3–4), and the scar erodes and is gone by 3.244 s on all rows. |
| 4 | No tells of cheapness | pass (with notes) | (a) **The first clump is whole.** At 0.26 s (row 3, enlarged against round 4) a smaller fan of four or five spikes stands at point 1, with no lone "L" spike. The last clump is whole too (row 3, 0.973 s). (b) The heavy, shaded cones and soil collars are unchanged from round 4. (c) Note: the scar at 1.6 s is still more a soft, periodically blotched smudge with a thin centre line than torn earth with clods (row 3, 1.622 s). The blotches repeat at about the point spacing. This is minor. |
| 5 | Value and colour | pass | The newest clump's tips are the brightest values, and older clumps fall back to umber. **On teal the scar is now a darker teal:** 3-row average (85, 116, 110) against the ground's (94, 144, 138) and (90, 137, 133) on the rows above and below (row 7, 1.622 s). Round 4 found a grey-tan film. |
| 6 | Crowd | pass | Rows of small brown spike tufts with darker streaks. They are busy where many lines cross, but never hide a unit. |
| 7 | Fog | pass | Nothing at or behind the attacker on the plain sheet. The fog sheet is judged below. |

**Round-4 notes**

| Note | Status | Evidence |
|---|---|---|
| 1: first clump cut to one spike ("L"/hook) | **resolved** | Row 3, 0.26 s and 0.487 s, enlarged against round 4. |
| 2: grey-tan scar on teal | **resolved** | Row 7, 1.622 s and 2.433 s: a darker shade of teal (sample above). |
| 3: lone dark chip at the far end, 1.6 s | **resolved** | Row 7, 1.622 s: the scar's far end fades out with no chip (round 4 showed one there). |

**Remaining improvements, ranked** (polish)

1. Let the scar show some torn clods and break up its periodic blotches, so that at close zoom it reads as torn
   earth and not as a smudge.

---

## Cross-effect consistency

**Pass.** All three effects share one art direction: painted matter, a key light from the upper left, and lasting
marks darker than the ground. The spine scar now darkens teal too, which closes round 4's one exception. The acid
burst's first 0.5 s is still the most saturated and glossy moment of the three, as an acid splash should be. Its
dried stain fits the family by colour but is cleaner in shape than the shell's ragged blast mark and the spine's
scar. Acid improvement 1 would bring it closer.

## Crowd frame (`stress_capture_400.png`, against round 4)

**Pass, with one note.** Every unit and building stays readable. `stress_capture.txt` records footprints covering
31.9 % of the screen (upper bound; round 4: 31.7 %) and at most 38 footprints over one cell (round 4: 65). There
were none retired and no Critical instance without a slot. The stains are now true-size discs, so the green share
of the frame rose. Pixels with G exceeding both R and B by 20 make up 7.0 % of the frame, against 5.5 % in round 4.
The fields now read as **many overlapping clean green circles**, some lighter where they overlap, like stacked
lily pads (enlarged crop of the left armies). Round 4's fields read as irregular puddles. This is more graphic than
round 4, but units lift off it clearly. At 200 splashes a second, far beyond a real fight, it is acceptable. Acid
improvement 4 addresses it. As in rounds 3 and 4, the capture is a debug build on lavapipe.

## The burrowed attacker's line, drawn only where seen (`spine_line_fog_contact.png`, `spine_line_fog_sequence.png`)

**Pass, unchanged from round 4.** Nothing is drawn on the fogged side at any age, on either ground or zoom (all
cells). The first seen clump stands whole at the fog edge (rows 3 and 7, 0.26 s and 0.487 s, enlarged). Its outer
left spike leans about 1 sheet px over the fogged ground, well within the stated 0.4 units. The ground marks start
just on the seen side and fade in, so no mark ends in a straight cut of its own. The new smaller end clump suits
the edge well: no half clump and no pop.

## Summary

| Effect | Verdict | Items failed | Round-4 findings | Top remaining improvement |
|---|---|---|---|---|
| `shell_impact` | **accept** | none | veil resolved; fringe and ring width deferred | irregular streak fringe (scale-out) |
| `spine_line` | **accept** | none | all three resolved | scar shows clods, less periodic |
| `acid_burst` | **accept** | none | both blocking points resolved (edge 0.98–1.00 of the reach, ±2 game px; no band) | restore a wet meniscus and crust edge inside the disc outline; remove the ghost inner rim; landed globs not glyph-like |

| Area | Verdict |
|---|---|
| Consistency | pass |
| Crowd | pass (acid fields read as stacked discs; polish) |
| Fog edge | pass |
