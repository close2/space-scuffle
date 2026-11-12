# Independent Review of the Effect Scale-Out, Round 2 (WP7)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Scale-out round 2 (WP7)").
Previous round: [`independent_review_scaleout1.md`](independent_review_scaleout1.md). Checklist: plan §7.2, as quoted
at the end of the packet.

## Method

I judged from the packet, the round-1 review, the checklist and the images. I formed every verdict before I read
the implementer's round-2 self-assessment in `slice_review.md`. I read it afterwards only to check two factual
claims, and I name both below. I did not read the code, and I built and ran nothing.

The material was the contact sheets of `flame_line`, its fog take, `lance_sweep`, `cleave_impact` and its repeat
take, with their sequences, and the four stress captures. I cropped cells and enlarged them 3x to 10x with
nearest-neighbour scaling (ImageMagick), and I contrast-stretched one strip. I measured in four ways:

- **Extents:** the bounding box of the pixels that differ by more than 8–10 % from the same row's clean last column,
  against the red line or yellow reach circle in the reference row.
- **Colours:** colour clusters (`-colors 5/6`) in a crop around the element.
- **Pops:** the pixels that change by more than 3 % between consecutive sequence frames.
- **Identity:** `compare -metric AE` against `git show main:<path>`.

Rows 1–8 and the names of cells and frames are as in round 1: rows 1/3 bright default/close, rows 5/7 dark
default/close, and the even rows are their references. Sequence frames are numbered from 0 and are 40 ms apart.
Sheet px are pixels of the 2560×1440 sheet.

The bar is the owner's: "no cheap looking effects".

**Verdicts in brief:**

- **`flame_line` is accepted** with notes. All four round-1 points are fixed.
- **`lance_sweep` is accepted** with notes. All three round-1 points are fixed.
- **`cleave_impact` is rejected.** The punch, the value structure and the per-hit variation are fixed. The pencil
  ring has become a **glass bubble**: a clean elliptical rim around a translucent disc, which still **pops** in one
  frame and is the same circle on every hit.
- **`shell_impact` is restored.** Its sheet, its sequence and all six radv/lavapipe goldens are identical to `main`
  (AE = 0).
- **`acid_burst`, `spine_line` and `flak_burst` did not regress.** Their sheets and goldens are unchanged since
  round 1.

---

## 1. `flame_line` (napalm jet; Hellion analogue): **accept** (with notes)

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | **On the line, with no back-fill and no overshoot.** Row 3 marks span sheet px 88–231 at 0.104 s and 0.278 s, and 88–232 at 1.043 s, against line ends 88.5–230.5. Row 1 spans 98/99–222/223 against 97–222. Only the rising smoke at 1.043 s leaves the line (78–237), as airborne smoke may. The jet covers about 60 % of the line at 0.104 s and reaches the end at frame 4. The jet's tip is the focal point while it travels, and the band then marks the line until about 2.6 s. |
| 2 | Layered | pass | Jet, billowing fire with flame tongues, soot, smoke puffs (1.043 s), embers in the band, and the scorched band. |
| 3 | Timing | pass | The jet is bright from frame 1. The fire peaks at frames 5–8, turns to soot from about frame 12 (0.5 s) and is out by frame 14. The band erodes into flecks at 2.607 s (row 3), and every row is clean at 3.476 s. |
| 4 | No tells of cheapness | pass (with notes) | **(a) The bubble rims are gone.** At 0.278 s (rows 3 and 7, 6x) the fire is soft-eroding billows and short tongues. No eroded hole has a light rim. **(b) The band is one strip.** At 1.043–1.738 s (row 3) it is a continuous scorched band of uneven width, with three or four thicker reaches joined by thinner necks. Their period is about a quarter of the line, unrelated to the 0.25 spacing. It is no longer a row of beads. **Notes:** (i) In frames 1–4 the jet is a straight, even-width yellow rod. Its upper edge is flat, with small regular notches along the front half (row 3, 0.104 s, 6x). For four frames it reads slightly as a tube or beam rather than a spreading cone of liquid fire. (ii) The billows have little value structure inside: no darker oxblood pockets and no hotter cores. At close zoom the body reads a little as orange cotton. Neither shows at the default zoom. |
| 5 | Value and colour | pass | **The fire is now fire.** Jet core (254, 210, 113) and (251, 218, 140). Fire body at 0.278 s (238, 165, 99), with (250, 197, 140) highlights and (146, 118, 97) soot on the bright ground. On teal the body is (232, 164, 94). Round 1's body was a beige (215, 178, 150). It now matches the shell and flak fireballs. The yellow-white phase is short (frames 1–4, on the jet only). The band is neutral and darker than both grounds. |
| 6 | Crowd | see "Crowd" | |
| 7 | Fog | pass | **Fog take** ([sheet](flame_line_fog_contact.png), [sequence](flame_line_fog_sequence.png)): on every row and at every age nothing is drawn on the darkened, fogged ground. The jet, fire, band and smoke all begin at the fog edge (row 3, 0.104–1.043 s, 4x). **Note:** at 0.104 s the jet starts with a blunt vertical cut exactly on the fog edge. That is correct for the fog, and it does not reveal the attacker. |

**Round-1 points:** bubble rims **fixed**; pale cream **fixed**; beaded streak **fixed**; flames behind the first
point and overshoot **fixed**.

**Optional polish**

1. Let the jet flare slightly wider towards its tip, and break its upper edge with noise of irregular period, so that
   frames 1–4 read as a spray of liquid fire rather than a rod.
2. Put a few darker oxblood pockets in the billows at 0.2–0.4 s, so the fire has an inner hot-to-dark structure.

---

## 2. `lance_sweep` (thermal lance; Colossus analogue): **accept** (with notes)

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | **On the line.** Row 3 marks span 131–189 at 0.25–0.469 s against 131–188. Row 1 spans 135/136–184/185 against 134–185. The head is the brightest point (peak luma 238 at 0.094 s, rows 3 and 7), and it sweeps the line in about five frames. |
| 2 | Layered | pass | Head flare, molten groove, spatter (row 7, 0.25 s: small dark droplets either side of the line), smoke puffs (0.939 s), the cooling groove, scorched halo and cracks. The distortion layer is still invisible on every cell, as stated. |
| 3 | Timing | pass | The head reaches the end by frame 5. The groove glows orange to 0.469 s and its glow shrinks inward. It is dark at 0.939 s, eroded at 1.564 s, and only a few specks remain at 2.346 s (row 7). Every row is clean at 3.128 s. |
| 4 | No tells of cheapness | pass (with notes) | **(a) No ruler line.** At 0.25–0.469 s (rows 3 and 7, 5x) the groove is ragged, with torn dark edges and an orange core that varies in width. **(b) No capsule.** At 0.094 s the head is a torn flare with a short orange trail and no round ends. **(c) No dotted smoke.** At 0.939 s the three or four puffs differ in size, height and spacing. **Notes:** (i) From 0.939 s the cooled groove tapers systematically. It is a thin line over its first half and a thick ragged blob at its far end (rows 3 and 7, 0.939–1.564 s). The width varies, but as a wedge, and at default zoom it reads slightly like a comet or tadpole rather than an evenly burnt cut. (ii) The spatter is too small to see at the default zoom (rows 1 and 5). The airborne part is visible only at close zoom. (iii) At frame 0 and at the head at 0.25 s a soft round glow dot remains. It is small and does not read as the old capsule. |
| 5 | Value and colour | pass | Head peak (235, 207, 173) to near-white. Groove core (248, 152, 98) cooling through (187, 140, 113) to the neutral scar at (116, 110, 101), which is darker than both grounds and not black. |
| 6 | Crowd | see "Crowd" | The battle cannot field the attacker yet. |
| 7 | Fog | pass (sheets) | Nothing is drawn off the line. No fog take; the attacker is never concealed. |

**Round-1 points:** ruler-line scar **fixed**; capsule head **fixed**; airborne part **fixed, but weak at the
default zoom**; dotted smoke **fixed**.

**Optional polish**

1. Distribute the groove's width variation along the whole line, or bias it towards the middle, so it does not
   taper to a point at the start.
2. Make two or three of the molten droplets larger and brighter for frames 1–5, so the spray reads at the default
   zoom.

---

## 3. `cleave_impact` (heavy blow; Ultralisk analogue): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | **The front is on the reach.** Row 3, 0.142 s: 115–205 × 54–131 against the yellow reach at 114–206 × 54–132. Row 1: 131–189 against 130–190. **The flash is the focal point** at frames 0–1 (row 3, 0.0 and 0.028 s). |
| 2 | Layered | pass | Flash, light, sparks (faint radial flecks at 0.028 s), front, dust burst, puffs and dark clods. The crack is not recognisable on any cell. |
| 3 | Timing | **fail** | Frame 0 now shows the blow, which is fixed. **But the front still pops.** Its rim is present at full strength in sequence frames 2–7 and absent at frame 8. Between frames 7 and 8, 370 px of the cell change by more than 3 %, against 46–78 px for each of the steps 4→5, 5→6 and 6→7. A contrast-stretched strip of frames 6–9 shows the rim intact at 7 and gone at 8, with no thinning and no erosion of the rim. The 0.85/255 figure in `slice_review.md` is a whole-frame mean, and it hides this. The dust inside does erode properly (frames 8–16). |
| 4 | No tells of cheapness | **fail** | **(a) A glass bubble.** From 0.076 s to 0.475 s the front is a translucent disc with a smooth, unbroken elliptical rim, slightly darker than its inside (rows 3 and 7, 4x). It is clearest on teal, where it looks like a soap bubble, a glass dome or a shield lying over the dust. The rim is not noise-broken, does not vary in width and has no leading-edge density. The small puffs "kicked out behind it" do not break the silhouette. This is the same vector-clean ring that round 1 rejected, made wider and filled. **(b) Repetition.** In the repeat take (sequence frames 6, 22, 37 and 52) the interior dust differs per hit, which is fixed. The rim, though, is the identical perfect ellipse every 0.6 s, and it is the most visible part of the effect at the default zoom (row 1). **(c)** The clods at 0.285–0.475 s are dark squarish blobs with near-straight sides (row 7, 4x). That is acceptable at the default zoom. **(d)** Pops: see item 3. |
| 5 | Value and colour | pass | **Fixed.** The flash is the brightest point at frames 0–1. The clods are the darkest. The dust lies between, as a soil brown on teal. |
| 6 | Crowd | see "Crowd" | No stacking: in the repeat take each hit is gone (frame 14–15) before the next one starts. |
| 7 | Fog | pass | No ground layer goes beyond the reach. |

**Round-1 points:** no punch at frame 0 **fixed**; one value **fixed**; pinwheel **fixed**, with the dust varying per
hit; pencil ring that pops **not fixed**: it is now a filled bubble with the same clean rim, and it still pops at
frame 8.

**Ranked changes**

1. **Remove the clean rim.** The front must not have a continuous outline. Make it a broken skirt: dense, ragged
   clumps at the leading edge, gaps where the noise drops out, and width varying about twofold around the circle.
   Do not draw a uniform translucent disc inside it. The inside should be bare ground with dust, not a tinted lens.
   The Critical contract (93 % of rays within ±2 px) can be met by the clumps' outer edge.
2. **Erode the front out, do not cut it.** Over frames 5–8 the rim's clumps should shrink and break apart from the
   inside out. The largest consecutive-frame change on the front alone should be no larger than its neighbours'.
3. Rotate or flip the front's noise per hit, so that consecutive hits do not show the same outline.
4. Optional: make the crack readable at close zoom, and break the clods' square silhouettes.

---

## 4. `shell_impact`: **restored, no regression**

The sheet and the sequence are identical to `main`'s: `compare -metric AE` gives 0 for
[`shell_impact_contact.png`](shell_impact_contact.png) and [`shell_impact_sequence.png`](shell_impact_sequence.png)
against `git show main:` copies. These files are the round-5 files, so they alone do not prove that the code draws
them again. Independent support is that the six GPU goldens re-recorded in this round
(`crates/client/tests/data/effects/{lavapipe,radv}/shell_impact_{0,1,2}.png`) are also identical to `main`'s (AE 0).
Round 1's regression (blast mark 45–55 % weaker) is fixed.

Round 1 also asked to constrain the draw space so that no instance loses its blast mark, and the shell's own polish
items (an irregular streak fringe, ring width variation). These are **still open**, as the packet says.

## 5. `acid_burst`, `spine_line`, `flak_burst`: **no regression**

None of their sheets, sequences or GPU goldens changed in this round. The diff from round 1's commit `f3d08f3` touches
none of them, so round 1's findings stand as written: acid's frame-0 dots, spine's detached scar piece, and flak's
fixed trefoil smoke and even scallops.

---

## Crowd (item 6): **pass for readability; two points open**

[`stress_capture_400.png`](stress_capture_400.png) and its detail, against the baseline:

- **Units stay readable.** Every unit sprite is drawn above every effect. In the detail crop no unit is hidden or
  lost in the effects, and selection would still work by eye. Coverage is 31.5 % of the screen, with a summed area of
  2.15 screens and at most 42 footprints per cell. That is busy, not illegible.
- **Air.** The anti-air bursts show as warm glows among the air units (baseline detail, upper middle and left) and do
  not cover them.
- **Crossing jets.** Their scorched bands cross the field as thin grey-brown streaks, in the baseline as well. They
  read as burnt ground and do not clutter. **No jet is burning on the captured frame** (no orange fire anywhere at
  tick 150). So the crowd picture shows the jets' marks but not their fire over units, which was round 1's question.
- **What dominates is the acid.** Overlapping saturated green stains cover a large share of the fight. Acid was
  accepted, and a round-robin stream gives its 4.5 s stain a large share, so this is not this round's regression.
  Still, at this zoom acid is the one effect that changes the look of the ground the most. Consider lowering the
  stain's saturation or opacity after the first second, or in crowded cells.
- **Cleave and lance** cannot be picked out in the crowd. Small, short and brown, they disappear into it, which is
  acceptable for secondary splashes. The cleave's bubble rim does not stand out at this zoom.

**Open points**

1. **Decal pool headroom is too thin.** 3 921 of 4 096 decals were in use on the captured frame (96 %). That is at
   the normal stress rate, and the flame and lance bands take several decals each. A denser fight, or more
   napalm-wheelers, will hit the cap. Show what happens at the cap: which decals give way, and whether that is
   visible as lasting marks popping out. Then either raise the pool or merge each band into fewer decals, to keep
   about 25 % headroom at the stress load.
2. **Degradation frames are still missing.** Round 1 asked for a few consecutive frames across a budget retirement.
   The figures show 34 retirements and 892 Detail skips, but no picture of them. Also add one crowd frame taken
   while jets are burning (for example 2–3 ticks after a jet volley).

## Consistency across all seven: **mostly consistent; one exception**

- **Fire palette: now consistent.** The flame body (238, 165, 99), the lance core (248, 152, 98) and the shell and
  flak fireballs share the yellow, orange, soot ramp.
- **Line marks: now consistent.** The flame band and the lance groove are torn, uneven ground marks like the spine
  scar.
- **Ring language: still inconsistent.** The shell has a soft, wavering pressure front and flak a scalloped glassy
  rim, but the cleave's front is now a clean-rimmed bubble. The rule stated in round 1 still applies to the cleave
  (and, as polish, to flak): a noise-broken front of varying width, densest at the leading edge, eroding out.

## Summary

| Effect | Verdict | Items failed | Top change |
|---|---|---|---|
| `flame_line` | **accept** | none | optional: flare and break the jet rod in frames 1–4; oxblood pockets in the billows |
| `lance_sweep` | **accept** | none | optional: spread the groove's width variation instead of a wedge; larger spatter |
| `cleave_impact` | **reject** | 3, 4 | replace the clean-rimmed translucent bubble with a broken dust skirt that erodes out instead of vanishing at frame 8 |

| Round-1 point | Status |
|---|---|
| flame: bubble rims / cream colour / beaded streak / back-fill and overshoot | fixed / fixed / fixed / fixed |
| lance: ruler scar / capsule head / no airborne part / dotted smoke | fixed / fixed / fixed (weak at default zoom) / fixed |
| cleave: no punch / pencil ring and pop / pinwheel / one value | fixed / **not fixed** (bubble rim, pop at frame 8) / fixed / fixed |
| shell: blast mark regressed | fixed (sheet, sequence and goldens equal `main`) |

| Shipped effect | Regression | Evidence |
|---|---|---|
| `shell_impact` | no | AE 0 against `main` for the sheet, the sequence and six goldens |
| `acid_burst` | no | unchanged since round 1 |
| `spine_line` | no | unchanged since round 1 |
| `flak_burst` | no | unchanged since round 1 |

| Area | Verdict |
|---|---|
| Consistency | mostly consistent; the cleave's ring is the exception |
| Crowd | pass for unit readability; open: decal pool at 96 % (needs headroom or a shown cap behaviour), no degradation frames, no frame with jets burning |
| Fog (`flame_line`) | pass: nothing drawn in the fog at any age or row |
