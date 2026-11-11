# Independent Review of the Effect Vertical Slice, Round 4

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Round 4"). Earlier rounds:
[`independent_review.md`](independent_review.md), [`independent_review_round2.md`](independent_review_round2.md),
[`independent_review_round3.md`](independent_review_round3.md). Art direction: [`../reference.md`](../reference.md)
("Art direction"). Checklist: plan §7.2, as quoted in the packet.

The reviewer judged only the packet, the reference board, the plan's checklist and the images: the three contact
sheets and sequences with their legends, the new fog sheet and fog sequence, the crowd capture (round 4, and round
3's from commit `fd24c31` for comparison) with `stress_capture.txt`, and the `fx_ground` and `fx_acid` texture
sheets and frames. Cells were cropped and enlarged (nearest-neighbour, 2.5x to 10x). Sizes were measured on the
sheets as pixel runs along the centre row of a cell against the reference circle of the row below. Colours are
1- to 4-pixel averages of the sRGB image. Nothing was built or run. The reviewer did not read the code or the
implementer's verdicts (`slice_review.md`).

Cells are named as before: rows 1 to 8 as the packet lists them (1 bright default, 2 its reference, 3 bright close,
4 its reference, 5 dark default, 6 its reference, 7 dark close, 8 its reference); columns by the age in the legend.
Sequence frames are numbered from 0, 40 ms apart. "Sheet px" are pixels of the 2560×1440 sheet. A default-zoom cell
shows the frame at half scale, so 1 sheet px there is 2 px of the game frame. A close-zoom cell shows it at one third.

The bar is the owner's: "no cheap looking effects". **`shell_impact` is accepted again. `spine_line` is accepted.
`acid_burst` is rejected, narrowly.** The spine line fixed all three round-3 findings. Its pen strokes are gone, and
it now reads as a heavy row of chitin spikes tearing through the ground. The acid burst's drying stain is now a
convincing puddle: dark rim, relief kept, olive-brown by 1.8 s. Two things still fail. The Critical stain covers
only about 0.87 of the reach, not 0.98 (measured below), about 10 game px short at default zoom. And during the
burst's most visible 0.3 s a pale, translucent band lies between the stain and the front's lip. That band is the
round-3 "glass rim", now running all the way around.

---

## 1. `shell_impact` (sieged artillery): **accept**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The shock ring sits on the yellow reach in sequence frames 1–6 and row 4 at 0.241 s. The char core fills the red 0.47 circle (row 4, 2.4–6.0 s). One focal point: the flash at age 0, then the fireball core. |
| 2 | Layered | pass | Flash, shock ring, fireball, debris chunks (row 3, 0.241 s, two dark chunks outside the ring), smoke column (0.642–1.2 s), char core and streaked blast mark. |
| 3 | Timing | pass | Peak at frames 0–1. The fireball cools over frames 2–13. The smoke has cleared by 2.4 s. The mark erodes, and the ground is clean at 8.03 s (rows 1–8, last column). |
| 4 | No tells of cheapness | pass (with notes) | The ring's width now varies somewhat: it is thicker at the lower left than at the top in sequence frames 2–6. It is still a fairly clean, thin ellipse with a faint grey veil inside it (sequence frames 2–6, row 3 at 0.241 s), but less glassy than in round 3. The streak fringe is still a regular sunburst (row 3, 2.4 s), as recorded for scale-out. Neither shows at gameplay zoom. |
| 5 | Value and colour | pass | Flash and core are the brightest values. The ramp runs yellow, orange, oxblood, smoke. The char centre is deep brown-black, not pure black. |
| 6 | Crowd | pass | In the crowd frame the shells are small orange sparks and scorch dots, and never hide a unit. |
| 7 | Fog | pass | Nothing is drawn away from the impact point. |

**Round-3 findings (optional changes)**

| Finding | Status | Evidence |
|---|---|---|
| 1: vary the ring's width by about 2x, drop the haze inside | partly | Width varies a little (sequence frames 2–6, lower left thicker). A faint grey veil is still inside the ring, probably the streak mark beneath it, which now shows through instead of dust. |
| 2: a less regular streak fringe | not (deferred to scale-out, as stated) | Row 3, 2.4 s. |
| New note: four lobes at the flash's edge on teal | not changed, not visible at gameplay zoom | Row 7, age 0. |

**Remaining improvements, ranked** (none blocks acceptance)

1. At scale-out, make the streak fringe irregular (new blast-mark texture), and let the mark under the ring appear
   only once the ring has passed, so that no grey veil sits inside the ring while it races out.
2. Push the ring's width variation further (about 2x, with one or two breaks in it).

---

## 2. `acid_burst` (bursting creature): **reject (narrowly)**

Measurement of the stain against the reach, because the packet states 0.98. Diameters are pixel runs along the centre
row of row 3 (close zoom), against the red reach circle of row 4 at the same height. Threshold: visibly darker than
the ground. The reach is 135 sheet px across.

| Age | 0.18 s | 0.48 s | 0.9 s | 1.8 s | 3.0 s | 4.5 s |
|---|---|---|---|---|---|---|
| Round 3 (`fd24c31`) | 0.99 (front) | 0.77 | 0.73 | 0.70 | 0.57 | 0.42 |
| **Round 4** | 1.00 (front) | **0.87** | **0.87** | **0.87** | 0.82 | 0.57 |

Default zoom gives the same result: at 0.48–1.8 s the stain spans 122–200 sheet px against the reach's 116–203
(row 1 against row 2). It is 6 sheet px (12 game px) short on the left and 3 (6 game px) on the right. Edge pixels
at 0.9 s (row 3, x = 92–102 of the cell) are plain ground (≈ 180,172,163) up to the stain's first dark pixel. No
faint feather reaches out to the circle.

My reading of this (from the pictures only): the stain's box is 0.98 of the reach, but its ragged, eroded edge lies
about 13 % of the radius inside the box. The packet's first round-4 take at 1.14 overshot by up to 11 px at its
peaks. Its *mean* edge then lay at about the reach, and only the noise peaks overshot. The edge noise is simply too
deep for a Critical layer.

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | **fail (partly)** | The burst reads at once: from frame 1 the green disc and the front's dark lip mark the area, and the lip sits exactly on the reach at 0.18 s (row 4). But after the front leaves (sequence frames 8–10), the Critical stain marks only 0.87 of the reach for its whole wet and dried life (table above): 6–12 game px short at default zoom against a ±2 px bar. That is much better than round 3's 0.70–0.77, but in a crowd the stain alone marks the area, and it understates it visibly. |
| 2 | Layered | pass | Stain, front lip, splash crown, hot core (frames 1–4), eleven globs (visible now, sequence frames 2–11), rising fumes (from frame 9), then the drying crust. The airborne part is visible at default zoom this time. Note: the crown is still a small "Y" tuft at the centre (row 3, age 0; sequence frames 0–4), not the 1.0–1.18-of-reach splash the packet describes. |
| 3 | Timing | pass | The peak is at frames 1–2. The stain holds its size until about 1.8 s and then dries by erosion from the edge and from holes (row 3, 3.0 s and 4.5 s), and the ground is clean at 6.0 s. The contraction when the front leaves is now small: about 8 % of the diameter between sequence frames 7 and 10 (round 3: about 25 %). It is still noticeable as the ring vanishes. |
| 4 | No tells of cheapness | **fail** | (a) **A pale, translucent band lies between the stain and the lip** in sequence frames 2–8 and row 3/row 7 at 0.18 s. It is about 0.1 of the radius wide, all the way round: sampled at 0.18 s, left edge, the lip (91,127,81), then the band (120–154, 139–151, 109–140), lighter than the stain (≈ 45,110,45) and see-through to the ground. The whole reads as a green jelly in a glass dish. The packet says the stain lies under the lip from the first frame, but the stain's visible edge is the 0.87 one, so the gap shows. (b) **The globs are one teardrop sprite** (`fx_acid` texture sheet, row 3: a single glob variant), drawn as identical pale-lime commas. At 0.18 s (rows 3 and 7) they lie on the stain as six or seven matching stickers, like tadpoles. At default zoom they are small enough to pass. (c) The fumes at 0.48–0.9 s are a dense, near-white, twisting column (rows 3 and 7), more like cigarette smoke than acid vapour. Minor. (d) Resolved: the flat moss look. From 0.9 s the stain keeps its relief, shows darker pooling and a darker wet rim, and dries to an olive-brown crust with a dark edge line (row 3 and row 7, 0.9–3.0 s). Note: at 1.8–3.0 s a light, whitish feather sits on the right and upper-left edge (row 3), the relief highlight. It is faint. |
| 5 | Value and colour | pass | The centre core is the brightest point, and lime is limited to the core and highlights in the first 0.5 s. The body is emerald (≈ 45–53, 105–137, 45–53) while wet. A darker rim is visible along the lower and left edges at 0.9 s and 1.8 s, and the body is olive-brown by 1.8 s. The dried crust is darker than the bright ground and reads as a dried puddle on teal (row 7, 1.8 s). |
| 6 | Crowd | pass | See the crowd section: the fields now vary from fresh green to olive, with dark edges, and every unit stays visible. |
| 7 | Fog | pass | Nothing is drawn outside the zone. A glob or two land just outside the lower edge at 0.48 s (row 4), inside the reach. |

**Round-3 findings**

| Finding | Status | Evidence |
|---|---|---|
| rc1 / 4a / 5: keep the liquid look, dark rim, early olive | **resolved** | Rows 3 and 7, 0.9–3.0 s: relief kept, darker rim, olive-brown by 1.8 s. |
| rc2 / 1 / 4c: hold the true extent, no contraction | **partly** | It holds its size from 0.12 s to 1.8 s, which is the main improvement. But the size it holds is 0.87 of the reach, not the full disc (measurements above), and an 8 % contraction remains when the ring goes. |
| rc3 / 4b: front a dark wet lip, no glass rim, no pale gap | **partly** | The lip itself is now a dark line on the reach. The pale gap between stain and lip remains and now runs round the whole circle (item 4a). |
| rc4 / 4d: frame 0 ragged and larger | **partly** | Frame 0 is the stain at about 0.75 of the reach (sequence frame 0; row 4, age 0), ragged and dark green with no glowing oval. The crown is still small. |
| rc4 / 2 note: globs visible, vapour rising | **resolved, with a new note** | Globs visible on arcs at default zoom. Fumes rise. But globs are one sprite (4b), and fumes are near-white (4c). |
| rc5 / 4e: vary the relief | **resolved** | Pools and swells vary across the disc (row 3, 0.48–0.9 s). |
| Crowd: flat pastel fields | **resolved** | See the crowd section. |

**Remaining changes, ranked**

1. **Put the stain's visible edge on the reach.** Measure the visible edge, not the box. Reduce the depth of the
   edge noise of the Critical stain, so that its ragged edge wavers by about ±2 game px (about ±2 % of the radius)
   around the reach. Then size it so that the mean edge lies on the reach. Leave the deep bites for the drying
   phase after 1.8 s. Check it on the sheet as above: about 0.97–1.0 at 0.48, 0.9 and 1.8 s.
2. **Close the band between stain and lip.** With change 1 most of it goes. Make the lip's inner side opaque
   wet emerald too, so that it reaches inward over the stain, and no see-through, lighter ring shows at any frame
   from 1 to 9. The contraction at frames 8–10 then disappears as well.
3. **Glob variants.** Three or more glob shapes (round, split, stretched), random size, orientation along the
   velocity, a darker body with a single small highlight. Fewer of them should lie flat on the stain in the
   0.18 s frame.
4. Fumes thinner, a little yellow-green, lower opacity, so that they do not read as white smoke.
5. Optional: a larger, more torn crown in frames 0–4, as the packet intends (1.0 of the reach). Today it is a
   small central tuft.

Changes 1 and 2 are the blocking ones. Both are adjustments of size, noise depth and the lip's ramp, not new
material.

---

## 3. `spine_line` (burrowed spine attacker): **accept**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The line is found at once on both grounds (rows 1 and 5). Its extent is true: clumps stand on the red circles from the first to the last (row 2 and row 4, 0.973 s). The newest clump is the focal point while the front travels (sequence frames 3–22). |
| 2 | Layered | pass | Tear and heave ahead of each clump (a soft dark patch one point ahead, sequence frames 3–14), clumps, dust, the scar and the fissure. |
| 3 | Timing | pass | The front advances one point every 2–3 frames. The spines sink from the source end (row 3, 0.973 s; sequence rows 3–4), and the scar erodes from the source end (sequence rows 5–7). The last column (3.244 s) is clean on all four grounds. Note: the legend's last age is 3.244 s, while the packet's text says 3.284 s; the sheet is what counts. |
| 4 | No tells of cheapness | pass (with notes) | (a) **The pen strokes are gone.** At no age does a lone slanted or crossing stroke remain: at 0.973 s (row 3) only sinking clumps; at 1.622 s only the scar (row 3; sequence rows 4–5 at default zoom). (b) The clumps now have mass: heavy cones lit on the left, a shaded right flank, a dark collar of soil, about 0.7–0.9 units tall against the grid (row 4, 0.487–0.973 s). At default zoom they read as spiked chitin crowns, not weeds. (c) Notes, none blocking: the first clump (point 1) is cut by the line's own start and stands as a single tall spike with a bright edge, like an "L" or a hook (row 3, 0.26 s; row 7, 0.097–0.487 s; sequence frames 3–20). On teal the scar is a grey-tan film of about the ground's value (sampled (119,126,116) on ground (95,138,133)), not a darker shade of the ground (row 7, 1.622 s). The scar reads as a soft smudge with a thin centre line more than as torn earth with clods. A single dark chip remains at the far end at 1.622 s (row 3 and row 7). |
| 5 | Value and colour | pass | The newest clump's tips and lit flanks are the brightest values in the line, and older clumps fall back to umber. Bone-ivory tips on every spike. Marks are darker than the bright ground. |
| 6 | Crowd | pass | In the crowd frame the lines are rows of small brown spike tufts over a soft grey streak. They no longer read as barbed wire. They are a bit busy where many lines cross the open ground between the armies, but never hide a unit. |
| 7 | Fog | pass | Nothing is drawn at or behind the attacker on the plain sheet (row 4, 0.097 s: the first mark starts inside the first red circle). The fog sheet is judged in its own section below. |

**Round-3 findings**

| Finding | Status | Evidence |
|---|---|---|
| rc1 / 4a: pen-stroke ruptures, lone leaning spikes | **resolved** | Row 3 at 0.973 s and 1.622 s, and sequence rows 3–5: no strokes. The tears are soft dark patches along the line. The one remaining lone spike is the cut first clump (note 4c). |
| rc2 / 4b: clumps small and twiggy, flat | **resolved** | Row 3 and row 7, 0.487–0.973 s: shaded heavy cones, soil collar, about 0.7–0.9 units tall. |
| rc3 / 4c: thin pencil scar | **resolved** | Row 3, 1.622 s: a furrow about 0.3–0.5 wide, uneven. It reads softer than torn earth (note 4c). |
| 3 note: fissure visible at the old last column | **resolved** | The last column (3.244 s) is clean. |

**Remaining improvements, ranked** (none blocks acceptance)

1. Do not show the first clump cut to one spike. Either keep the clump whole and move its card forward so that
   it lies within the line, or use a smaller single-cone variant at point 1 with the same mass and shading.
2. Make the scar darken the ground on every ground (a multiply or darkening blend of the ground's own colour),
   so that on teal it is a darker shade of teal, not a grey-tan film. Let a few clods of the `earth_scar` texture
   show at close zoom.
3. Remove the lone dark chip left at the far end at 1.6 s, or let it erode with the scar.

---

## Cross-effect consistency

**Close to one art direction now.** `shell_impact` and `spine_line` share the painted-matter look: umber, soot and
dust darker than the ground, warm highlights, left key light. `acid_burst`'s dried stain (0.9 s on) fits that
family: olive-brown crust, a dark edge, relief lit from the upper left. What still sets the acid burst apart is its
first 0.3 s: a saturated, glossy green dome in a translucent rim, with identical pale-lime droplets. It looks more
like a toy or a jelly than the shell's fire and the spines' chitin. Fixing changes 1–3 of `acid_burst` would close
the gap. The scar on teal (spine change 2) is the one place where the spine line breaks the "marks darker than the
ground" rule.

## Crowd frame (`stress_capture_400.png` against the baseline and round 3)

**Pass, and clearly better than round 3.** Every unit and building stays readable in front of the effects.
`stress_capture.txt` records footprints covering 31.7 % of the screen (upper bound) and up to 65 footprints over
one cell, none retired and no Critical instance without a slot. No popping can be judged on one frame. The
acid fields are no longer flat pastel paint. They range from fresh saturated green to dark olive, show dark edges,
and step down in value as they age, so the green no longer reads as one carpet. Units lift off them clearly (crop
of the left armies). The spine lines read as rows of small spike tufts with grey streaks. Where several lines
cross open ground between the armies (upper left, lower right) they add busy, scratchy texture. That is acceptable
at 200 splashes a second, which is far beyond a real fight. The shells are small sparks and dots. The fresh stains
are still the most saturated thing on the screen, and acid change 1 (stain at true size) will slightly raise their
coverage. Recheck the frame after that change.

Note: like round 3, this capture is a debug build on lavapipe, not the RX 580 path of rounds 1 and 2. Nothing in
the frame suggests a different renderer.

## The burrowed attacker's line, drawn only where seen (`spine_line_fog_contact.png`, `spine_line_fog_sequence.png`)

**Pass.** This is the material round 3 asked for, and it answers the three open questions:

- **Soft or hard edge.** The fog itself is the terrain's hard, cell-aligned step. The line's ground marks (tear,
  heave, scar, dust) start just on the seen side of that step and fade in over a short distance (row 3, 0.973 s and
  1.622 s; fog sequence frames 9–11 and 30–60). So no mark is cut off by a straight line of its own. Where a mark
  reaches the step, it reads as hidden by the fog, as units do, and not as a stencil.
- **Marks clipped with the spikes.** Yes. Nothing at all is drawn on the fogged side at any age, on either
  ground, at either zoom (all fog-sheet cells, columns 0–7). At 0.097 s, when the front is still under the fog,
  the visible ground is empty, which is correct.
- **No pop for a clump at the edge.** The first seen clump appears whole (fog sequence frame 11; row 3,
  0.487 s). Its outer left spike leans about 2 sheet px (about 0.1–0.2 units) over the fogged ground at close zoom
  (enlarged crop of row 3, 0.487 s). That is well within the stated 0.4, and reads naturally: a spike standing on
  seen ground, leaning into the dark. There is no half clump and no cut. The clump erupts with the same timing as
  on the plain sheet, so nothing pops.

Nothing is drawn at the attacker. Item 7 passes for `spine_line` on evidence now, not only on description.

## Summary

| Effect | Verdict | Items failed | Distance to accept |
|---|---|---|---|
| `shell_impact` | **accept** | none | — |
| `spine_line` | **accept** | none | — (notes: the cut first clump, the scar's value on teal) |
| `acid_burst` | **reject (narrowly)** | 1 (partly), 4 | Small: the stain's visible edge on the reach (shallower edge noise, then resize), and no see-through band between stain and lip. Glob variants recommended. |
