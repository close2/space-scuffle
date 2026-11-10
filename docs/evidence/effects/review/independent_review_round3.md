# Independent Review of the Effect Vertical Slice, Round 3

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Round 3"). Earlier rounds:
[`independent_review.md`](independent_review.md), [`independent_review_round2.md`](independent_review_round2.md).
Art direction: [`../reference.md`](../reference.md) ("Art direction"). Checklist: plan §7.2, as quoted in the packet.

The reviewer judged only the packet, the reference board, the plan's checklist and the images: the three contact
sheets with their legends, the three sequences, the crowd capture and its baseline with `stress_capture.txt`,
and the `fx_ground` texture sheet and frames. Cells were cropped and enlarged (nearest-neighbour, 2x to 4x), and a
few stain colours were sampled as 1-pixel averages of the sRGB image. Nothing was built or run. The reviewer did
not read the code or the implementer's verdicts (`slice_review.md`).

Cells are named as before: rows 1 to 8 as the packet lists them (1 bright default, 2 its reference, 3 bright close,
4 its reference, 5 dark default, 6 its reference, 7 dark close, 8 its reference); columns by the age in the legend.
Sequence frames are numbered from 0, 40 ms apart.

The bar is the owner's: "no cheap looking effects". **`shell_impact` is accepted again. `acid_burst` and
`spine_line` are rejected.** Both improved clearly, and most round-2 findings are resolved as written. `spine_line`
is now close: its eruption phase passes, and its one remaining tell, dark pen-stroke marks crossing the line, is
cheap to fix. `acid_burst` has a good first 0.4 s, but then it shrinks back from the reach and spends five seconds
as a flat, bright-green moss patch. The darker wet rim and the early drying that the packet describes do not show
in the pictures.

---

## 1. `shell_impact` (sieged artillery): **accept**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | Unchanged from round 2. The shock ring sits on the yellow reach from sequence frame 2 to frame 6. The char core fills the red 0.47 circle (row 4, 2.4–6.0 s). One focal point: the flash, then the fireball core. |
| 2 | Layered | pass | Flash, shock ring, fireball cards, debris chunks (row 3, 0.241 s, two dark chunks above the ring), smoke, char core and streaked blast mark are all there. |
| 3 | Timing | pass | The peak is at frames 0–1. The fireball cools over frames 7–13 and the smoke clears by about 1.2 s. The mark erodes away and is gone at 8.03 s (row 4, last cell). |
| 4 | No tells of cheapness | pass (with notes) | The round-2 notes still apply: the ring is a thin, even ellipse with a faint haze inside it, so it reads a little like a glass dome (row 3, 0.241 s, and sequence frames 2–6), and the streak fringe is a regular sunburst (row 3, 2.4–6.0 s). New, minor: on teal at age 0 the flash shows four soft lobes at its edge (row 7, age 0). None of these shows at gameplay zoom. |
| 5 | Value and colour | pass | The flash and core are the brightest values, and the ramp runs from yellow through orange and oxblood to smoke. The char core is a step darker than in round 2: a deep brown-black centre, still not pure black (rows 3 and 7, 2.4–6.0 s). |
| 6 | Crowd | pass | In the crowd frame the shells are small and never hide a unit. |
| 7 | Fog | pass | Nothing is drawn away from the impact point. |

**Round-2 findings (optional changes)**

| Finding | Status | Evidence |
|---|---|---|
| 3: darken the char core | resolved | Rows 3 and 7, 2.4 s: a darker core than in round 2. |
| 1: vary the ring's width, drop the fill | not (deferred to scale-out, as the packet says) | Sequence frames 2–6. |
| 2: a less regular streak fringe | not (deferred to scale-out) | Row 3, 2.4 s. |

**Remaining improvements, ranked** (none blocks acceptance)

1. Before scale-out, break the shock ring: vary its width by about 2x around the circumference and drop the haze
   inside it. In a crowd, many identical glass rims would be the first thing to read as cheap.
2. Make the streak fringe irregular (a new blast-mark texture), as recorded for scale-out.

---

## 2. `acid_burst` (bursting creature): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | **fail (partly)** | Frame 0 is a small lime oval of about half the reach. From frame 1 the stain fills the reach, and the front ring sits on the red 2.2 circle for frames 2–8 (row 4, 0.18 s). That part passes. But the front goes away at about frame 9 (0.36 s), and the stain left behind is only about 75–85 % of the reach and has deep bites: on the right and at the top at 0.48 s, on the right at 0.9 s, and at the top-left and right at 1.8 s (row 4). In the sequence the burst visibly contracts between frames 8 and 10. The stain is the Critical layer now, and in a crowd it is the only mark of the area. It should cover the true disc until it starts to erode. |
| 2 | Layered | pass (with notes) | There is a ground flash or core, a splash crown, globs, vapour, the front and the stain. Notes: at default zoom the airborne part still barely exists. The globs are not visible in the sequence (frames 1–6), and the splash crown is a small pale scribble at the centre (row 3, 0.18 s). The vapour reads as a soft lime smudge lying on the stain (row 3, 0.48 s), not as a rising puff. |
| 3 | Timing | pass (with notes) | The peak is at frames 0–1. The stain erodes from its edge and from irregular holes and is gone at 6.0 s. That is real erosion. Note: the drying the packet describes is not visible as a colour change. The stain samples about sRGB (94,170,77) at 0.9 s, (77,140,64) at 1.8 s, (80,128,63) at 3.0 s and (84,116,64) at 4.5 s. At 1.8 s it is still in the wet-emerald range, and it reads olive only at 4.5 s. What dries early is the sheen, not the colour. |
| 4 | No tells of cheapness | **fail** | (a) **From 0.9 s to 4.5 s (rows 1, 3, 5, 7; sequence rows 3–8) the stain is again a flat, mottled mid-green patch with a fuzzy edge**, like moss or felt on the ground. The relief and sheen of 0.18–0.48 s are gone by 0.9 s. For about five of its six seconds the effect is the "flat fill" of round 2 with noise on it. (b) The front at 0.18 s is a pale, translucent, even ellipse with a lighter band around the stain (row 3 and row 7, 0.18 s). It reads as a soap bubble or glass rim, not as a wet edge. In the lower left the ground shows through between the stain and the rim as a pale crescent. (c) The contraction when the front leaves (item 1) is a visible change of size, a soft pop. (d) Frame 0 is still a smooth, glowing lime oval with white sparkle dots (row 3, age 0), more like a gel than a burst. Round 2's bevel is gone. (e) The bumpy relief at 0.18–0.48 s is regular and blobby, like green coral or broccoli. It is good material in principle, but it is uniform over the whole disc. |
| 5 | Value and colour | **fail (partly)** | The centre is the brightest point, and the lime is limited to the first 0.5 s and the centre, which is good. But no darker, more saturated wet rim is visible at any age: from 0.48 s the edge is a soft, slightly lighter feather (rows 3 and 7, 0.48–1.8 s). From 0.9 s the field is one mid value again. In the crowd at far zoom the stains read as a light, pastel paint green (see the crowd section). |
| 6 | Crowd | pass (with notes) | Units stay visible. The stain is now Critical, so round 2's bare selection-circle rings are gone. See the crowd section for the green fields. |
| 7 | Fog | pass | Nothing is drawn outside the zone. |

**Round-2 findings**

| Finding | Status | Evidence |
|---|---|---|
| 4a/5/rc1: flat single-value fill, no sheen, lighter rim | partly | Relief and sheen show at 0.18–0.48 s. From 0.9 s the stain is flat again, and no darker rim shows. |
| 3 note / rc1: drying starts late | partly | The sheen goes early, but the colour is still emerald at 1.8 s (sampled) and reads olive only at 4.5 s. |
| 4b: evenly spaced round dots | resolved | No polka dots. The holes are irregular (rows 3 and 7, 3.0–4.5 s). |
| 4c/rc2: glossy lime button pop | partly | No bevel. Frame 0 is still a smooth glowing oval of half the reach. The crown is tiny. |
| 2 note / rc2: globs are white specks | not visible | No globs can be seen at default zoom (sequence frames 1–6) or at close zoom (row 3, 0.18 s). |
| 2 note / rc2: vapour is a flat patch to one side | partly | It is centred now, but it reads as a lime smudge on the stain, not as a rising puff. |
| 4e/6/rc3: selection-circle rings in the crowd | resolved | The stain stays. No bare rings in the crowd frame. |
| 4d/rc3: pale fringe at the stain's edge | partly | The grey-white fringe is gone. The edge is still a soft, lighter feather, not a dark wet rim, and at 0.18 s a pale gap shows between the stain and the front in the lower left (row 3). |

**Remaining changes, ranked**

1. **Keep the liquid look for the whole life.** The 0.18–0.48 s material (relief, sheen, darker pooling) must
   carry on into the drying stain. As it dries, keep the relief and swap the sheen for a crusted, cracked
   surface. Make the colour change visibly early: clearly olive-brown by about 1.5–2 s, not emerald. Draw the
   darker, more saturated rim as a band about 0.1–0.2 units wide, darker than the body, at every age until the
   edge erodes. Check by sampling: the rim must be darker than the centre at 0.9 s and at 1.8 s.
2. **Hold the true extent.** The stain must cover the full 2.2 disc (within ±2 px at default zoom, broken only
   in texture) until erosion begins. The front must hand over to a stain of the same size, so that nothing
   contracts at about 0.36 s.
3. **Make the front a wet edge, not a glass rim.** Drop the pale translucent band and the lighter halo, make the
   front a dark, wet, noise-broken lip that the stain runs into, and close the pale gap between them.
4. **Sell the burst in the air.** Frame 0 should already be ragged and larger, about 0.8 of the reach. Make the
   splash crown bigger and opaque enough to read at default zoom. Globs must be visible at default zoom (several
   pixels, dark body, lime highlight, on visible arcs). The vapour should rise off the stain, not lie on it.
5. Vary the relief across the disc (larger swells and flatter pools) so it does not read as uniform bubbles.

---

## 3. `spine_line` (burrowed spine attacker): **reject (narrowly)**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The line is found at once on both grounds. Its extent is true: the marks run from the first red circle to the last (row 4, 0.38 s and 1.9 s). The newest clump is now a clear focal point (see item 5). |
| 2 | Layered | pass | Ruptures open ahead of the front (row 3, 0.38 s, a dark rupture one point ahead), there is a soil heave at each point, clumps plus single spikes, a light dust burst, the scar and the fissure. |
| 3 | Timing | pass (with notes) | The front advances about one point every 2–3 frames (sequence rows 1–3), with anticipation. The spines sink from the source end (row 3, 1.27 s) and the line erodes from the source end (sequence rows 5–7). Note: at 2.53 s, the end of the lifetime, a faint stretch of fissure is still visible on the far half (row 3, last cell). Check that it reaches zero before despawn, so that it does not pop. |
| 4 | No tells of cheapness | **fail (narrowly)** | (a) **Dark pen-stroke marks cross the line.** The elongated ruptures and the leaning single spikes are narrow, uniform, dark strokes with hard ends. At 0.38 s there is a vertical bar at point 3 and a slanted bar ahead of the front (row 3; row 7). At 1.27 s, once the clumps at points 1–5 have sunk, only these strokes remain, and the line reads "/ \| — \\" (row 3, 1.27 s; sequence rows 3–4 at default zoom, where it reads as stitches or barbed wire). This is the round-2 glyph tell, moved from the clumps to the secondary marks. (b) The clumps are now splayed spike fans, a clear improvement. At default zoom (row 1, 0.76 s) they still look small and twiggy, like dark weeds or bird feet: flat silhouettes with almost no shading, standing about half a unit tall rather than 0.6–0.9. (c) The scar left at 1.9 s is a thin, pencil-like grey line (row 3, 1.9 s). The torn-earth rubble of the `earth_scar` texture (texture sheet, row 5) does not show in the game at all. (d) The ends are now feathered, and no hard cut is visible (resolved). |
| 5 | Value and colour | pass | The newest clump is a bright bone-ivory fan with a pale dust puff for its first frames (row 3, 0.2–0.76 s, rightmost clump; row 7 on teal), clearly the brightest thing in the line. Older clumps fall back to dark umber. Tips are faintly paler. The dust on teal is now faint, not blobby. |
| 6 | Crowd | pass (with notes) | Units stay visible. In the crowd frame the lines are thin grey hairlines with small tick marks, a little like barbed wire, but much less dominant than round 2's grey smears. |
| 7 | Fog | pass | Nothing is drawn on the attacker's side: the first mark at 0.076 s lies inside the first red circle (row 4, first cell with content). The fog fade is discussed below. |

**Round-2 findings**

| Finding | Status | Evidence |
|---|---|---|
| 4a: hooks and glyph-like spines | partly | The clumps are now straight, splayed spikes. The glyph look moved to the ruptures and single spikes (item 4a). |
| 4b: one or two spines per point | resolved | Fans of three to five plus single spikes (row 3, 0.76 s). |
| rc1: taller spikes | partly | At default zoom they still read short and twiggy (row 1, 0.76 s). |
| 5 / rc2: no focal point, tips not visible | resolved | Bright bone-ivory newest clump with a dust burst (rows 3 and 7, 0.2–0.76 s). |
| 4c / rc3: hard cut at both ends | resolved | No straight cut at either end (rows 3 and 7, 0.76–1.9 s). |
| 4d / rc3: stars and donuts at 1-unit spacing | resolved, with a new tell | No donuts or stars. The scar is continuous but thin as a pencil line, and the ruptures read as strokes. |
| 4e: pale dust blobs on teal | resolved | Row 7, 0.38 s. |
| Crowd (b): dirt over the battlefield | resolved | Thin lines in the crowd frame, no smears. |

**Remaining changes, ranked**

1. **Remove the pen strokes.** Make the ruptures short, wide, ragged tears that run *along* the line (within
   about ±20° of it), soft-edged and lower in contrast. Never draw them perpendicular. Sink the single leaning
   spikes together with their clump, or before it, so that no lone slanted stroke is left standing when the
   clump has gone.
2. **Give the clumps mass.** Make the spike bodies thicker at the base, with the key light showing on the left
   flank and a darker right flank, and with a small soil collar at the base. Scale them so that they read at
   0.6–0.9 units at default zoom.
3. **Let the scar read as torn earth.** Widen the scar to about 0.3–0.4 units, with uneven width, so the rubble
   texture shows, instead of a thin fissure line. Make sure everything is at zero at 2.53 s.

---

## Cross-effect consistency

**Not yet one art direction, but closer.** `shell_impact` and `spine_line` share the painted-matter look: umber
and soot marks darker than the ground, warm dust, and believable shading. `acid_burst` belongs to it for its
first 0.5 s (wet relief, sheen, a dark body). After that it becomes a bright, flat green patch that is lighter
in feel than the burn and the scar beside it, and it still looks like it comes from a flatter game.

## Crowd frame (`stress_capture_400.png` against the baseline)

Every unit and building stays readable in front of the effects, so item 6 passes for all three effects. Units
also lift clearly off the green.

The green fields are **acceptable as a stress case, but they show a real weakness.** 200 splashes a second with a
third of them acid, each stain lasting 6 s, is far beyond any real fight, so merged fields are expected. The
weakness is the look of those fields. At this camera distance the stains are flat, light, pastel-green paint
blobs with no wet relief, no darker rim and no visible drying (crops of the left and top armies). That is the
far-zoom look of a single stain too, so fixing change 1 of `acid_burst` (keep the material, a dark rim, early
olive) fixes the crowd look as well. The spine lines are thin, unobtrusive hairlines, and the shells are small.

Note: the round-3 capture was made on lavapipe in a debug build, not on the RX 580 path of rounds 1 and 2. The
packet says it is the same renderer, and nothing in the frame suggests otherwise.

## The burrowed attacker's line, drawn only where seen

**Not shown in the images provided, so it cannot be judged visually.** No contact sheet, sequence or crowd capture
contains fog or a line crossing from fog into sight. The packet says the partial case is tested in pixels. What
the images do show is correct: nothing is drawn behind the first search point, and the source end fades in softly
(row 4, 0.076 s). What the reviewer cannot check:

- whether the line's boundary at the fog edge is soft or a hard cut;
- whether the scar, fissure and dust are clipped at the same point as the spikes;
- whether a clump straddling the fog edge pops in or out.

**Request for round 4:** one contact-sheet row (or a short sequence) with the attacker in fog and the line running
into sight, at default and close zoom, so this can be judged. Until then item 7 is judged on the sheets and on the
packet's description.

## Summary

| Effect | Verdict | Items failed | Distance to accept |
|---|---|---|---|
| `shell_impact` | **accept** | none | — |
| `acid_burst` | reject | 1 (partly), 4, 5 (partly) | Medium: the material must carry through the whole life, plus true extent and a wet front. |
| `spine_line` | reject (narrowly) | 4 | Small: remove the pen-stroke ruptures and spikes, add mass to the clumps, widen the scar. |
