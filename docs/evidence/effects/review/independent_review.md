# Independent Review of the Effect Vertical Slice, Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md). Reference board:
[`../reference.md`](../reference.md) (it names sources and principles but no reference images, so the comparison
with StarCraft II and League of Legends below is from the reviewer's knowledge of those games).

The reviewer judged only the packet and the images it lists: the three contact sheets, the three sequences and the
two crowd captures. The reviewer cropped and enlarged cells to look at them; nothing was built or run. The
reviewer did not read the code or the implementer's verdicts (`slice_review.md`).

Cells are named by sheet, row and age. Rows are numbered 1 to 8 as the packet lists them (1 bright default,
2 its reference, 3 bright close, 4 its reference, 5 dark default, 6 its reference, 7 dark close, 8 its
reference). Sequence frames are numbered from 0, 40 ms apart.

The owner's bar is "no cheap looking effects". **All three effects are rejected.** `shell_impact` is the closest
to acceptance. `acid_burst` and `spine_line` each have a basic problem of shape and style that tuning alone will
not fix.

---

## 1. `shell_impact` (sieged artillery): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | **fail** | The burst reads at once (sequence frames 0–2: white flash, then a ring). The Critical layer does not mark the true size for long enough. The shock ring is fully visible only in frames 2–3 (80–120 ms). From frame 3 its top half is hidden behind the fireball, and by frame 6 (240 ms) it is two separate arcs at the bottom (contact row 3 and row 7, age 0.241 s). The scorch, the only layer that stays, reaches only about 0.8 units (row 2, ages 2.4–6.0 s: it ends between the red full-damage circle and the yellow reach circle). The reference board says the scorch "marks the true reach" (1.25). Nothing marks the 0.47 full-damage core at all. |
| 2 | Layered | pass | Flash, shock ring, fireball flipbook, embers, smoke and scorch are separate (row 3, ages 0 to 1.2 s). Debris and sparks are listed on the reference board but are not visible in any frame of the sequence or the sheets. |
| 3 | Timing | pass (with notes) | The peak is at frame 0–1. The fireball cools from yellow to orange to brown to grey over frames 4–20. The smoke thins out. The scorch erodes from its edge (row 3, age 6.0 s) and is gone at 8.03 s. Notes: the fireball does not expand after frame 2. It holds one silhouette and only changes colour (frames 3–12), so it reads as a sprite fading in place rather than a blast pushing out. The ring is the only thing that moves outward. |
| 4 | No tells of cheapness | **fail** | (a) The flash at age 0 (rows 3 and 7) is a flat, clipped white "cotton-ball" silhouette with a hard lobed outline and no internal gradient. (b) On dark ground at 0.241 s (row 7) every fireball lobe has a thin bright rim, like soap bubbles. This looks outlined, not volumetric. (c) The shock ring is a clean band of constant width and constant brightness (sequence frames 2–5). Professional shockwaves are distorted, uneven in width and brightest on their leading edge. (d) The scorch (rows 1, 3 and 7, 2.4–6.0 s) is one uniform flat grey with a stair-stepped, dithered edge and the same cauliflower silhouette at every age. It has no darker centre, no radial streaks and no burnt rim. |
| 5 | Value and colour | pass (with notes) | The flash is the brightest point, and the ramp runs white, yellow, orange, brown, grey. Note: the scorch is neutral mid-grey, not char. On the bright ground (row 3, 2.4 s) it reads as a shadow or a dirt smudge, not a burn. |
| 6 | Crowd | pass | In `stress_capture_400.png` the shell leaves small dark smudges that never hide a unit. It is the least noticeable of the three effects. |
| 7 | Fog | pass | Nothing is drawn away from the impact point. |

**Changes, ranked by impact**

1. **Make the ground mark state the reach.** Extend the scorch, or a fainter outer blast ring of scoured dust under it, to radius 1.25. Grade it: a near-black char core of about 0.47 radius, a brown-grey middle, and a feathered radial-streak fringe that fades out at the reach. Use the noise-sheet erosion for the edge instead of the current alpha-clipped cauliflower. Keep the ring at the reach visible for its first 120 ms, drawn over the fireball's base or as a ground decal with soft depth, so that it is never cut to two arcs.
2. **Give the shock ring and flash real energy.** For the ring: vary its width with the ring noise, brighten its leading edge, and make it thinner and fainter as it expands. Do not erode it into two symmetric arcs. For the flash: a radial gradient from a white core to a warm halo, roughly circular and soft, held for only 1–2 frames, with no cloud silhouette. Remove the bright rim around fireball lobes (row 7, 0.241 s). It is probably premultiplied or edge-lit alpha. Self-shade the lobes darker at their edges instead.
3. **Add the missing debris and motion.** Add 6–10 dark debris chunks and 4–8 bright spark streaks thrown out ballistically in frames 1–8, a short dust ring kicked outward along the ground, and a fireball that grows by about 30 % over frames 1–6 before it cools. A sieged artillery hit in StarCraft II reads through this outward throw. At present the motion is all colour change in place.

---

## 2. `acid_burst` (bursting creature): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass (with notes) | At 0.180 s the front ring sits on the red 2.2 circle (row 4) and the whole disc is green. The area is identifiable by frame 2. Notes: at age 0 the burst covers only about 55 % of the radius (row 4, age 0). The brief says the effect covers "its whole area at once". The fresh stain at 1.8 s fills only about 80 % of the radius and sits off-centre towards the lower left (row 4, age 1.8 s). |
| 2 | Layered | **fail** | There are four layers (yellow core, front ring, splat, stain), but all four are coplanar ground decals. Nothing rises off the ground: no globs or droplets flung up, no splash crown, no vapour. In every cell the effect reads as one flat sticker stack (rows 3 and 7). The texture set includes a crown and droplets (`acid_crown` and `droplet` in `fx_acid_texture_sheet.txt` in this directory), but they do not show on the sheets. |
| 3 | Timing | **fail** | The peak at frame 0 and the ring at the radius by frame 2 are good. Then the ring erodes into four symmetric crescent wedges at the diagonals (sequence frames 7–9) and vanishes by about 0.4 s. From 0.5 s to 1.8 s the splat only darkens from lime to mid-green, with no visible drying: no shrinking wet edge, no change of sheen. The stain then fades evenly (rows 3 and 7, 3.0–4.5 s). The stain loses value uniformly instead of eroding. |
| 4 | No tells of cheapness | **fail** | (a) The splat is a clip-art "paint splat" with regular radial spikes and round droplets (row 3, ages 0 and 0.9 s), a cartoon graphic on realistic terrain. (b) At 3.0–4.5 s (rows 3 and 7) the stain is two overlapping, offset, half-transparent copies of a splat with a visible double edge, like two stickers. (c) The ring's four-wedge erosion is mirror-symmetric (sequence frame 8). In the crowd capture it repeats as the same four-petal shape over every burst (`stress_capture_400.png`, centre left). It is the most obvious repetition in the slice. (d) The front ring is a flat band of constant width with notch cut-outs (row 3, 0.180 s). |
| 5 | Value and colour | **fail** | At age 0 the core is the brightest (pale yellow). From 0.18 s on, ring, splat and centre share one pastel lime value, so there is no focal point. The stain is a flat, saturated mid-green (row 3, 1.8 s). Drying acid should desaturate towards yellow-olive with a darker wet rim. On the teal ground (row 7) the pastel lime has almost the same value as the ground and reads washed out. |
| 6 | Crowd | pass (with notes) | Units stay visible through the rings in `stress_capture_400.png`. However, the identical pale-green rings and four-petal arcs are what makes the crowd frame look busy and repetitive. |
| 7 | Fog | pass | Nothing is drawn outside the zone. |

**Changes, ranked by impact**

1. **Add a vertical, volumetric burst.** Frames 0–6: a wet crown or dome of acid globs that bursts up and out (12–20 billboarded droplets of random size on ballistic arcs, landing inside the 2.2 radius and each leaving a small secondary splat), plus a low, fast green-yellow vapour puff that thins out by 0.6 s. A Baneling in StarCraft II reads as a gooey pop, then a spray. The current effect is all decal and never leaves the ground.
2. **Replace the clip-art splat and the two-sticker stain with one eroding stain.** Make a single stain decal that covers the full 2.2 disc with an irregular, noise-driven edge (no regular spike rosette). Give it a darker, more saturated wet rim and a lighter centre. Let it dry by erosion: the noise-sheet threshold eats in from the edge and from holes inside, and the colour shifts from wet emerald to dull olive or yellow crust. Randomise rotation and mirroring per instance, and never stack two copies offset from each other.
3. **Fix the front ring and the value structure.** Make the ring's erosion asymmetric and noise-driven: no four-way symmetric wedges, a random phase per instance, a brighter leading edge and a wavering width. Keep one focal point after frame 2, a bright yellow-green centre over a darker green field. Raise the ring's value contrast on dark and teal ground, or put a thin darker outer edge under it so that it separates from the terrain.

---

## 3. `spine_line` (burrowed spine attacker): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | **fail** | On the bright ground at default zoom (row 1, and the sequence) the spines are 4–6 px tall and pale beige, almost the same value as the sand. What reads is a dashed brown line made of the dark ground blobs. The line is therefore found, but not as spines. The extent is not true: at ages 0.075–0.375 s (rows 3 and 4) pale streaks extend about 0.6 units behind the first search circle, and at 0.75–1.25 s spines lean about 0.5 units past the last circle. That is far outside ±2 px of the 8-unit line. |
| 2 | Layered | **fail** | There are two layers: flat spine cards and dark ground blobs. There is no eruption dust or debris kick, no ground crack opening ahead of each spine, and no impact accent. The plan treats this as a Major effect, which needs at least three layers. |
| 3 | Timing | pass (with notes) | The front travels about one search point every 2–3 frames (sequence frames 0–20), retraction follows from the source end (row 3, 0.75–1.875 s), and the marks erode out by 2.5 s. Notes: each spine appears at full height in a single frame, with no anticipation (bulge or crack) and no overshoot. Each spine also retracts by fading rather than sinking. |
| 4 | No tells of cheapness | **fail** | (a) Every spine is the same tapered wedge leaning the same way at about 45° (row 3, 0.375–0.75 s; row 7). Together they read as a comb, or a row of grass blades or feathers, not chitin. (b) They are visibly flat cards: from the 58° camera they have no thickness or shading. (c) The ground marks at 1.25–1.875 s (rows 3 and 7) are identical oval dashes at exactly 1-unit spacing, a perfectly regular dashed line that looks like a stencil. (d) The "cracks" are soft blobs, not cracks. |
| 5 | Value and colour | **fail** | No element is brightest at a focal point. The leading spine, which should carry the eye, looks like every other spine. On bright ground the spines lack value contrast (beige on beige, row 3). |
| 6 | Crowd | pass (with notes) | In `stress_capture_400.png` the lines remain readable as thin beige "barbed wire" and do not hide units. Dozens of them over open ground give the frame a scratched-lens clutter, so contrast and length should be kept honest. |
| 7 | Fog | **fail** (pending confirmation) | Pale streaks are drawn about 0.6 units behind the first search circle, on the attacker's side (rows 3, 4, 7 and 8, ages 0.075–0.375 s). If the first search point is at or next to the attacker, which the nine circles spanning eight units suggest, this draws effect pixels around and behind a burrowed unit. The packet's own rule is that nothing is drawn at the source end. Clip every layer to the segment from the first search point to the last. |

**Changes, ranked by impact**

1. **Redesign the spine silhouette and value.** Use 3–5 spine variants: curved, hooked or triangular chitin, thicker at the base, darker (bone to dark umber with a pale tip highlight), with lean angle (±25° around vertical, not all leaning one way), height (±30 %) and rotation randomised per spine. Two or three spines per search point, splayed, read as a clump erupting rather than a comb. A bright edge or rim on the newest spine gives the travelling front a focal point. The Lurker in StarCraft II reads because its spines are dark and sharp against any ground.
2. **Add the eruption layers.** For each search point: a crack decal that opens 1–2 frames before the spine (anticipation), a short burst of dirt and rock chunks plus a dust puff at eruption, and a spine that overshoots its height by about 10 % and settles. On retraction the spine sinks into the ground rather than fading. Replace the identical oval dashes with real crack decals (variants, random rotation, jittered spacing). The `ground_crack` texture is in the set but is not legible on the sheets.
3. **Clip to the true line.** No spine, streak or dust may extend before the first search point or past the last one: tilt the spines' lean along the line or clamp each card's footprint. This fixes items 1 and 7 together.

---

## Things that would look cheap to a player even where an item passes

- **The whole slice is flat.** All three effects are almost entirely ground decals and camera-facing cards with no thrown or volumetric element. StarCraft II and League of Legends area effects always have a short airborne component (debris, droplets, dust) that sells the impact. The flatness is the main reason the slice looks like placeholder art.
- **Style mismatch between the effects.** The shell's photographic flipbook fireball sits next to the acid's cartoon clip-art splat and the spines' stylised flat wedges. These look like three different art sets. Pick one treatment (painted-realistic, as the shell's fireball and the terrain suggest) and bring the other two into it.
- **Symmetric erosion.** Both ring layers (the shell's two arcs and the acid's four wedges) erode symmetrically. In a crowd this pattern repeats everywhere and is the first thing an experienced eye notices.
- **Alpha-clip stair-stepping.** On the scorch, the stain and the spine blobs (all close rows, enlarged), the eroded edges are hard and pixel-stepped. Erosion should be soft (a small smoothstep band of the threshold), not a binary clip.

## Summary

| Effect | Verdict | Items failed |
|---|---|---|
| `shell_impact` | reject | 1, 4 |
| `acid_burst` | reject | 2, 3, 4, 5 |
| `spine_line` | reject | 1, 2, 4, 5, 7 |
