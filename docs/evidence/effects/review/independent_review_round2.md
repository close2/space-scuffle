# Independent Review of the Effect Vertical Slice, Round 2

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Round 2"). Round 1:
[`independent_review.md`](independent_review.md). Art direction: [`../reference.md`](../reference.md) ("Art
direction"). Checklist: plan §7.2, as quoted in the packet.

The reviewer judged only the packet, the reference board and the images listed in the packet: the three contact
sheets with their legends, the three sequences and the two crowd captures. The reviewer cropped and enlarged
cells (nearest-neighbour, 3x to 4x) to look at them. Nothing was built or run. The reviewer did not read the code
or the implementer's verdicts (`slice_review.md`).

Cells are named as in round 1: rows 1 to 8 as the packet lists them (1 bright default, 2 its reference, 3 bright
close, 4 its reference, 5 dark default, 6 its reference, 7 dark close, 8 its reference), columns by the age in the
legend. Sequence frames are numbered from 0, 40 ms apart.

The bar is the owner's: "no cheap looking effects". **`shell_impact` is accepted. `acid_burst` and `spine_line`
are rejected.** Both improved a lot since round 1, and the round-1 findings are mostly addressed as written. Each
still has one tell that a player would notice at gameplay zoom: the acid is a flat, single-value green disc, and
the spines read as a row of hook-shaped glyphs with no focal point.

---

## 1. `shell_impact` (sieged artillery): **accept**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The shock front is a whole ring at the yellow reach circle from sequence frame 1 to frame 6 (40–240 ms). It is drawn in front of the fireball and is never cut into arcs. The char core fills the red 0.47 circle almost exactly (row 4, ages 2.4–6.0 s). The streaked blast mark reaches the yellow circle at the bottom and sides and stops about 0.1–0.15 short at the top (row 4, 2.4 s). That is within a pixel or two at default zoom (row 2). One focal point: the flash, then the fireball core. |
| 2 | Layered | pass | Flash, shock ring, three-card fireball, dark debris chunks on arcs (row 3, 0.241 s, two chunks above the ring), smoke, char core and streaked blast mark are all distinct. |
| 3 | Timing | pass | Frame 0 is a small hot point and frame 1 is the peak. The fireball visibly grows over frames 2–6 and cools from yellow to orange to oxblood over frames 7–13. Smoke rises and thins by 1.2 s. The mark holds and then erodes away between 6.0 and 8.03 s (row 4, the last cell is clean). |
| 4 | No tells of cheapness | pass (with notes) | The cotton-ball flash is gone. It is now a soft radial glow with a slightly noise-broken edge (row 3, age 0). Fireball lobes have no bright soap-bubble rims on dark ground (row 7, 0.241 s). The mark has a dark core, radial streaks and a feathered fringe, with no stair-stepping at 3x. Notes: (a) the shock ring is still a fairly thin, even ellipse at default zoom (sequence frames 2–6), and the area inside it carries a faint grey haze, so for those frames it reads slightly as a glass dome. (b) The streak fringe is a regular sunburst, and its top edge shows a faint curved lip (row 7, 2.4–6.0 s). Neither is noticeable at gameplay zoom. |
| 5 | Value and colour | pass | The flash and core are the brightest values, and the ramp runs white, yellow, orange, oxblood, smoke grey. The mark is darker than both grounds. On teal it reads as a brown burn, not a shadow (row 7, 2.4 s). The char core is dark brown-grey rather than near-black. That is acceptable. |
| 6 | Crowd | pass | In `stress_capture_400.png` the shells are the least visible effect and never hide a unit. |
| 7 | Fog | pass | Nothing is drawn away from the impact point. |

**Round-1 findings**

| Finding | Status | Evidence |
|---|---|---|
| Ring cut to two arcs; scorch only about 0.8; nothing marks 0.47 | resolved | Whole ring at reach for frames 1–6. Char core at 0.47 and blast mark at about 1.2 (row 4). |
| 4a cotton-ball flash | resolved | Soft radial glow (row 3, age 0). |
| 4b bright rims on lobes | resolved | Row 7, 0.241 s. |
| 4c clean ring of constant width | partly | The ring is brighter on its leading edge, but at default zoom it still reads as a clean, even ellipse. |
| 4d uniform grey stair-stepped scorch | resolved | Graded core, streaks and soft fringe. |
| No outward throw; one silhouette | resolved | The fireball grows, debris arcs out, smoke drifts. |

**Remaining improvements, ranked**

1. Break the shock ring more strongly: vary its width by a factor of about two around the circumference, and drop the faint fill inside it, so that it reads as a pressure front and not as a glass rim.
2. Make the streak fringe less regular: fewer, longer and unequal streaks, plus a few gaps, so that it does not read as a sunburst stamp when many shells land close together.
3. Darken the char core a step towards the art direction's `0.01–0.03` linear.

---

## 2. `acid_burst` (bursting creature): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The disc reaches the red 2.2 circle by 0.18 s (row 4) and fills it at 0.48–0.9 s. At age 0 it covers about 55–60 % of the radius. The burst still does not cover the whole area at once, but by frame 4 it does. One focal point exists: the lime centre. |
| 2 | Layered | pass (with notes) | Pop dome, globs, spray, vapour, wet stain and front are present (sequence frames 0–10). Notes: the globs and spray read only as tiny white specks (row 3, 0.18 s). The vapour is a pale, flat, lichen-like patch to one side of the centre (sequence frames 3–12). It does not read as a rising puff. The airborne part barely sells the burst. |
| 3 | Timing | pass (with notes) | The peak is at frame 0–1. The front reaches the reach by 0.18 s. The stain shifts to olive and erodes from its edge and from holes (rows 3 and 7, 3.0–4.5 s) and is gone at 6.0 s. This is real erosion. Note: from 0.48 s to 1.8 s the stain is still the same saturated green. The drying starts late. |
| 4 | No tells of cheapness | **fail** | (a) From 0.48 s to 1.8 s (rows 1, 3, 5 and 7) the stain is one flat, uniform mid-green fill with no sheen, depth or value variation, like a felt pad laid on the ground. That is the "flat single-value fill" the art direction bans. (b) Darker round dots are sprinkled evenly over it (row 3, 0.9–3.0 s): a polka-dot pattern, not liquid. (c) At age 0 and in frames 1–2 the pop is a glossy lime ellipse with a bright bevelled rim, which reads as a UI button or a jelly lozenge, not as a burst. (d) Along the stain's edge at 0.48–1.8 s there is a pale grey-white fringe that is lighter than the ground (row 3 and row 4, right side). (e) In the crowd (see item 6) the effect is reduced to thin pale-green ring outlines. |
| 5 | Value and colour | **fail** | The centre is the brightest point, which is good. The field, however, is a single saturated value. The rim is lighter (lime) at 0.18 s, not the darker wet rim the art direction asks for. On teal (row 7) the green disc separates from the ground now, but only as a flat shape. |
| 6 | Crowd | pass (with notes) | Units stay visible. Because the burst's Detail layers are retired over the Major cap, most acid bursts in `stress_capture_400.png` show only their front, a thin pale-green circle. That reads as a selection or range ring and is the most graphic element in the frame. |
| 7 | Fog | pass | Nothing is drawn outside the zone. |

**Round-1 findings**

| Finding | Status | Evidence |
|---|---|---|
| 2: everything is a coplanar decal | partly | A dome, globs and vapour exist, but the globs are specks and the vapour is a flat patch. |
| 3: four symmetric wedges; no drying; uniform fade | resolved | The front is asymmetric, the stain dries to olive and it erodes. |
| 4: clip-art splat; two stickers; notched ring | partly | The splat and the stacking are gone. They were replaced by a flat felt disc with polka dots. |
| 5: no focal point; pastel lime; washes out on teal | partly | There is a focal centre and it separates on teal. The field is still one value, and the rim is lighter, not darker. |
| 1 note: 55 % at age 0 | not | The disc is still about 55–60 % at age 0. It is acceptable because it is full by frame 4. |

**Remaining changes, ranked**

1. **Give the liquid material.** Add value variation inside the stain: a darker, more saturated wet rim; wet highlights or sheen streaks that catch the key light from the upper left; viscous thickness variation driven by noise; and darker pooling where the noise is low. Remove the evenly spaced round dots. Start drying (desaturating towards olive and losing sheen) from about 0.6 s, not 2 s.
2. **Make the pop a burst, not a button.** At frames 0–3, use a ragged, noise-broken lime glow with globs visibly leaving it, larger globs (several pixels at default zoom) with dark wet bodies and a lime highlight, and no bevelled rim. The vapour should rise and spread as a soft, low, centred cloud, not lie as a flat patch to one side.
3. **Keep the material in the crowd.** Under the Major cap, keep a cheap version of the stain (one decal) rather than only the front ring. Otherwise, darken and break the ring so that it never reads as a selection circle. Remove the pale fringe at the stain's edge.

---

## 3. `spine_line` (burrowed spine attacker): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The line is found at once on both grounds (rows 1 and 5). It is a dark line, darker than the ground. Its extent is now true: every layer starts at the first search point and ends at the last (rows 4 and 8, 0.21–1.97 s). |
| 2 | Layered | pass | Cracks open ahead of the spines (row 3, 0.21 s, a dark crack burst one point ahead of the front), churned earth, the fissure, spines, dust puffs and leftover marks. |
| 3 | Timing | pass | The front advances about one point every 2–3 frames (sequence rows 1–2). Anticipation by the crack is visible. Retraction runs from the source end (row 3, 1.3–1.97 s) and the marks are gone by about 3 s. |
| 4 | No tells of cheapness | **fail** | (a) The curved spine variants read as glyphs. At default zoom (row 1, 0.79 s; sequence rows 2–3) the line looks like a row of handwritten characters ("x y 2 C a"). Close up (row 3) individual spines are C-shaped, J-shaped and claw-shaped hooks rather than spikes rising out of the ground. (b) They are sparse: one or two visible spines per point, not a clump. (c) The churned earth is cut by a hard straight edge at both ends of the line (row 3, 0.79–1.97 s, left and right ends). The clip is visible as a vertical edge. (d) The leftover marks at 1.3–2.0 s alternate small dark stars and donut rings at exactly 1-unit spacing (row 3 and row 4, 1.97 s). This is still a stencil-like regular row. (e) On teal the dust puffs are pale flat blobs at the spines' feet (row 7, 0.39 s). |
| 5 | Value and colour | **fail** | No focal point. The newest clump is no brighter than the others (row 3, 0.21–0.39 s; sequence frames 5–20). The bone-ivory tips are not visible at default or close zoom. Every spine is the same dark umber. |
| 6 | Crowd | pass (with notes) | Units remain visible. The lines are the dominant element of `stress_capture_400.png`, as long grey smears beside the dark spine rows, and give the open ground a dirty, scribbled look. |
| 7 | Fog | pass | Nothing is drawn before the first search point (rows 4 and 8). The burrowed attacker's side is clean. |

**Round-1 findings**

| Finding | Status | Evidence |
|---|---|---|
| 1 and 5: pale spines on pale sand; a dashed line | partly | The spines are dark and readable. There is still no focal point, and the leftover marks are still a regular row. |
| 2 and 3: two layers; no anticipation or overshoot; fading retraction | resolved | Six layers, the crack ahead of the front, and spines that sink back. |
| 4: comb of identical wedges | partly | The spines vary, but the variants read as hooks and letters, not chitin spikes. |
| 7: streaks behind the first point | resolved | Clipped to the segment. |

**Remaining changes, ranked**

1. **Change the spine silhouette to spikes.** Use straight or gently curved, tapering spikes, wide at the base and needle-sharp at the tip, mostly upright (±20°). Use three to five per point, splayed outward from a shared base, and make them taller (about 0.6–0.9 units). Drop the C- and J-shaped hook variants. Each clump should read as an eruption, not as a character.
2. **Add the focal point.** Give the newest clump, for its first two to three frames, a bright bone-ivory tip and rim light plus a short, light dust burst. Keep the tips visibly paler than the bodies on all spines.
3. **Soften the ends and break the leftover row.** Feather the churned earth and fissure over about 0.3 units at both ends instead of a straight cut. Jitter, rotate and vary the leftover marks, drop the donut rings, and let them merge into one continuous broken scar.

---

## Cross-effect consistency

**Partly one art direction.** `shell_impact` and `spine_line` now share the painted-matter look: warm dust, umber
earth, soft feathered ground marks and believable shading. `acid_burst` is still outside it. Its saturated,
single-value disc and bevelled lime pop are graphic shapes, not liquid matter, and next to the shell's burn mark
it looks like it comes from a different, flatter game.

## Crowd frame (`stress_capture_400.png` against the baseline)

Units, buildings and attack lines stay visible everywhere (item 6 passes for all three). The frame is dominated by
the spine lines' grey earth smears. The acid bursts shrink to thin pale-green rings that look like selection
circles, and the shells hardly show. Two notes: (a) the crowd degradation of the acid (front only) is a visible
change of look, not a smooth one; (b) at this density the churned-earth layer of the spine line should be lighter
or shorter-lived, since it reads as dirt over the whole battlefield.

## Summary

| Effect | Verdict | Items failed |
|---|---|---|
| `shell_impact` | **accept** | none |
| `acid_burst` | reject | 4, 5 |
| `spine_line` | reject | 4, 5 |
