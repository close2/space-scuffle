# Independent Review of the Effect Scale-Out, Round 1 (WP7)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3.
Packet: [`independent_review_packet.md`](independent_review_packet.md) (section "Scale-out round 1 (WP7)").
Earlier review: [`independent_review_round5.md`](independent_review_round5.md) (what was accepted for the shipped
three). Art direction: [`../reference.md`](../reference.md) ("Art direction"). Checklist: plan §7.2, as quoted at
the end of the packet.

## Method

I judged from the packet, the reference board, the checklist and the images, and from nothing else. I looked at
seven contact sheets with their legends and seven sequences. For the regression check I compared them with round 5's
sheets and sequences of the shipped three, from commit `58b8da2`. I did not read the code or the implementer's
verdicts, and I built and ran nothing. I cropped cells and enlarged them 2x to 8x with nearest-neighbour scaling
(ImageMagick). I measured the sizes in three ways:

- **Lines:** the extent of the mark on the centre line of the cell, against the red reference line and its
  end circles in the row below.
- **Circles:** the outermost pixel that differs by more than 25 levels from the same pixel in the clean last
  column, against the outermost reference pixel on the widest scan line.
- **Changes from round 5:** pixel counts above an 8 % difference from the clean last column.

Cells are named as in round 5. Rows 1 to 8 are: 1 bright default, 2 its reference, 3 bright close, 4 its reference,
5 dark default, 6 its reference, 7 dark close, 8 its reference. Columns are named by their age in the legend.
Sequence frames are numbered from 0 and are 40 ms apart. "Sheet px" means pixels of the 2560×1440 sheet.

The bar is the owner's: "no cheap looking effects".

**Verdicts in brief:**

- **`flak_burst` is accepted**, with notes.
- **`flame_line`, `lance_sweep` and `cleave_impact` are rejected.** Each fails one or two items, for concrete
  reasons that can be fixed.
- **`shell_impact` regressed.** Its scoured blast mark, the lasting mark at the reach, has mostly disappeared under
  the new random draw.
- **`acid_burst` and `spine_line` did not regress.** Acid's frame-0 note is only partly fixed. The spine scar is less
  periodic but has one stray piece.

---

## 1. `flame_line` (napalm jet; Hellion analogue): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass (with note) | **The Critical scar lies on the line.** Row 1, 1.729 s: sheet px 98–220 against line ends 97–222. Row 3: 88–230 against 88.5–230.5. **Early coverage:** at 0.104 s the jet covers the first 60–65 % of the line (row 1: 87–179), and it reaches the end at frame 4 (160 ms). That is a racing jet, as intended. **Note:** the airborne flames extend about 0.4–0.6 units *behind* the first search point at every zoom (row 3, 0.104–0.277 s: they start at sheet px 72–73 against the line's 88), so they would engulf the attacker's own nose. They also reach about 0.3 units past the far end (row 3, 0.277 s). |
| 2 | Layered | pass | Jet, burning flames, embers, grey smoke puffs (1.0 s) and the burnt streak. |
| 3 | Timing | pass | Travel takes four frames and the flames burn to about 0.5 s. The streak breaks up by erosion (2.594 s) and every row is clean at 3.459 s. |
| 4 | No tells of cheapness | **fail** | (a) **Bubble outlines.** At 0.277 s the flames are a froth of small *ring-outlined* circles: a lacy rim around each round puff, like soap foam or popcorn (rows 3 and 7, enlarged; sequence frames 5–12). The puffs' alpha erosion draws a light rim around each hole. The pattern shows at default zoom as a speckled, bubbly fringe (sequence frames 4–10) and is the cheapest-looking element in the scale-out. (b) **A beaded streak.** From 0.519 s the burnt streak is a row of oval beads of near-equal size at about the point spacing, which reads as a dashed or Morse line (row 3, 1.038–2.594 s). It erodes into separated dashes at 2.594 s. It is not a continuous scorched band of uneven width. |
| 5 | Value and colour | **fail** | **The fire is pale cream, not fire.** Sampled on row 3: 0.104 s, core (255, 254, 231) with a (240, 179, 107) fringe; 0.277 s, the burning flames above the line (215, 178, 150) and (230, 191, 153), a desaturated beige barely 40 levels off the ground's (175, 167, 159). The board's fire ramp (white-yellow, then orange `(1.0, 0.5, 0.1)`, then oxblood, then soot) appears only on the thin strip at the line itself ((192, 135, 78), 0.277 s). The near-white part lasts for about five frames (sequence frames 1–5), not two. The streak is a neutral (70, 66, 60), darker than both grounds; that is right. |
| 6 | Crowd | not judged | See "Crowd" below. |
| 7 | Fog | pass (sheets) | Nothing is drawn away from the line except the flames behind its start (item 1 note). No fog picture is given for this line; see "Fog" below. |

**Ranked changes**

1. **Remove the bubble rims.** Erode the flame cards with a soft step on a flame-shaped mask (tongues and licks
   stretched along the jet's direction), not as holes in round puffs. No edge brighter than the body should appear
   where a puff erodes. Use real flame flipbook frames if they exist.
2. **Give the fire its palette.** Keep the white-yellow core for two frames at the nozzle and the travelling tip, and
   hold the body at saturated orange, with oxblood and soot at the tips and on the trailing flames. The burning part
   should be clearly orange against both grounds, not beige.
3. **Make the streak one scorched band.** It should be continuous, of uneven width, a little wider than now near the
   middle, with a few glowing embers for 0.3–0.5 s. Break it up by noise at a scale unrelated to the 0.25 spacing.
4. Clip the flames behind the first search point, or start them at the nozzle, so they do not engulf the attacker.
   Keep the overshoot past the far end within the reach.

---

## 2. `lance_sweep` (thermal lance; Colossus analogue): **reject (narrowly)**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The sweep is found at once on both grounds (row 1 and row 5, 0.086–0.432 s). **The scar lies on the line:** row 3 sheet px 132–188 against 131–188, and row 1 135–184 against 134–185. Across the line the scar covers about the middle half of the full-damage width (row 3, 1.439 s: dark core 4 px within the 7 px between the red edges), which is acceptable for a line. |
| 2 | Layered | pass (weakly) | Sweeping head, glowing line, smoke puffs and scar. The distortion layer is not visible on any cell, as the packet says it is meant to be subtle. There is **no airborne part** that sells the burn: no sparks, molten spatter or flare at the head. The smoke is faint (below). |
| 3 | Timing | pass | The head sweeps the line in five frames (sequence frames 1–5). The line cools from orange to dark over 0.4–0.9 s, and every row is clean at 2.878 s. |
| 4 | No tells of cheapness | **fail** | (a) **The head is a clean capsule.** At 0.086 s it is a short, round-ended tube of even width with a yellow-white centre stripe (rows 3 and 7, enlarged; sequence frames 1–5). It reads as a glowing cigarette or a UI pill, not a burning point. (b) **The lasting mark is a ruler line.** From 0.864 s it is a straight, near-black stroke of constant 3–4 px width with parallel, unbroken edges (rows 3 and 7, 0.864–2.159 s; colour (77, 72, 66)). At default zoom it reads as a pencil line drawn on the map. The art direction asks that a line be "exact in extent but broken in texture". (c) **The smoke is a dotted row.** It is five faint puffs of one size at equal spacing and equal height above the line (row 7, 0.864 s, enlarged). On the bright ground a contrast-stretched crop shows straight sides on them. I do not count that against the effect, because it is not visible at normal contrast. |
| 5 | Value and colour | pass | The head is the brightest point (yellow-white), and the line cools through orange to dark. The scar is darker than both grounds and not black. |
| 6 | Crowd | not judged | See "Crowd". |
| 7 | Fog | pass (sheets) | Nothing is drawn off the line. |

**Ranked changes**

1. **Make the scar a burnt groove, not a stroke.** Vary its width by about twofold along the line, and make its
   edges ragged and soft. Add a lighter scorched halo either side that fades. Let short glowing cracks and embers
   stay in it for a few tenths of a second as it cools, so that it cools from the inside.
2. **Give the head matter.** Use a hot, noise-broken flare at the contact point, with a short spray of sparks or
   molten droplets thrown sideways from the head as it sweeps. That gives the effect its airborne part. Drop the
   round capsule ends.
3. **Smoke:** vary the puffs in size, offset and timing so they do not form a dotted row. Let them drift and rise.

---

## 3. `flak_burst` (anti-air burst; Liberator AA analogue): **accept** (with notes)

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The ring lies on the 1.5 reach: row 1, 0.133 s, −1/−2 sheet px; row 5, −1/−1. At 0.05 s it is already at −3 to −5. One focal point: the flash at frames 0–1, then the fireball core. |
| 2 | Layered | pass | Flash, ring, fireball and smoke. |
| 3 | Timing | pass (with note) | Peak at frames 0–1. The ring holds frames 1–6 and fades over frames 6–7 (no pop). The fireball erodes into smoke by frame 9, and the smoke erodes away and is clean at 1.66 s. **Note:** the smoke is a fixed silhouette. From frame 10 to frame 31 its three lobes keep the same shape and place (a "trefoil") and only thin out. Smoke at altitude would drift and spread. |
| 4 | No tells of cheapness | pass (with notes) | (a) The ring has a smooth outer edge and a **regular scalloped inner edge**, about ten even lobes (row 7, 0.133 s, enlarged). With its dark inner band it reads slightly as a glass bowl rim, which is the "glass rim" look round 4 rejected for the acid front. At default zoom it reads as a pressure front, so it does not block acceptance. (b) The fireball breaks into a cellular, lacy pattern at 0.133–0.249 s, the same kind as the accepted shell fireball. (c) There are no shrapnel streaks or sparks, which a flak burst would be expected to throw. |
| 5 | Value and colour | pass | White only at frames 0–1. The fireball runs yellow, orange, oxblood, then grey smoke. The smoke is warm grey, never blue. |
| 6 | Crowd | not judged | See "Crowd". The air plane matters here. |
| 7 | Fog | pass | Nothing is drawn outside the reach. The smoke stays inside the ring. |

**Ranked improvements** (polish)

1. Let the smoke drift (with a slight wind and rise) and grow by about 30 % over its life, so the trefoil does not
   hang still.
2. Break the ring's inner edge with noise of irregular period, not even scallops, and vary its width as the shell's
   ring does (see "Consistency").
3. Add a few short, bright shrapnel streaks thrown radially in the first 0.15 s.
4. Provide one capture at the air units' altitude among air units, because the sheets show the burst only on the
   ground.

---

## 4. `cleave_impact` (heavy blow; Ultralisk analogue): **reject**

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass (with note) | The ring lies on the 1.5 reach: row 1, −2/−2 at 0.072 s and −2/−1 at 0.135 s; row 5, −3/−3 and −2/−2. It is at about two thirds of the reach at frame 1 and on the reach at frame 2. Nothing marks the 0.5 or 1.0 zones; the dust "star" happens to cover the centre. **There is no focal point** (see item 5). |
| 2 | Layered | pass (weakly) | Faint ground darkening, ring, a dust burst in lobes, and a few dark soil specks at 0.45 s. There are no chunks on arcs, no crack and no impact flash. |
| 3 | Timing | **fail** | **The ring pops.** It holds constant contrast from frame 3 to frame 6. Then it vanishes in one frame: on the right edge of the ring, centre line, it is 124 at frames 3–6 against ground of about 168, and 155 at frame 7, which is ground. Frame 0 (0.0 s) is only a faint smudge (row 3), so the blow has no punch at the event. The dust erodes properly (frames 7–15). |
| 4 | No tells of cheapness | **fail** | (a) **A pencil ring.** The reach ring is a thin dark line of constant width and contrast around the whole circle (rows 3 and 7, 0.072–0.27 s). It is a vector-clean ring, which the art direction rules out, and it is unlike the shell's glowing, wavering front. On teal a small dark circle of about 0.15 radius also sits at the centre at age 0 (row 7), like a target mark. (b) **A pinwheel of dust.** The dust is four or five lobes splayed radially at nearly even angles, like a flower or a propeller (row 3, 0.27–0.45 s). It is the most recognisable silhouette in the set. An Ultralisk strikes every 0.6 s, so the shape will repeat in front of the same unit. (c) Pops: see item 3. |
| 5 | Value and colour | **fail** | The whole effect is one mid-value grey-brown: the dust, the ring and the specks. No element is brighter or more saturated than the rest, so there is no focal point at the blow and nothing reads as an impact rather than dust settling. On teal the dust is a reddish brown (row 7), which is acceptable as soil. |
| 6 | Crowd | not judged | See "Crowd". Stacking under the 0.6 s cadence is the main risk. |
| 7 | Fog | pass | Lobes of the rising dust go beyond the reach at 0.45 s (row 3, upper left), as airborne dust may. No ground layer does. |

**Ranked changes**

1. **Give the blow a punch and a focal point.** Add a short, bright, warm impact flash or ground glint at the struck
   edge for frames 0–1, a burst of dark soil chunks and pebbles on ballistic arcs, and a brief radial crack or
   ground heave at the centre. Frame 0 must show the blow.
2. **Replace the pencil ring with a dust front:** a low, noise-broken, wavering skirt of dust racing to the reach,
   brightest at its leading edge and darker inside. Fade it by erosion over 2–3 frames instead of cutting it at
   frame 7.
3. **Break the pinwheel.** Throw the dust in a cone along the blow's direction (the attacker's facing) with
   irregular lobes, and draw the rotation, lobe count and lengths per hit, so that two consecutive hits never show
   the same silhouette.
4. Show a sequence of three or four consecutive hits from the same attacker, 0.6 s apart, to prove there is no
   stacking and that each hit looks different.

---

## 5. Regressions of the shipped three

### `shell_impact`: **regression: yes**

The new random draw has removed most of the **scoured blast mark**, the radial streaked mark sized to the reach
that round 5 accepted (rows 3 and 7). These crops are round 5 against now, enlarged:

- **0.241 s:** round 5 shows the streaked mark under the ring. Now it is barely there.
- **0.642–1.204 s:** the streak fringe below the smoke is gone.
- **2.409–6.023 s:** the lasting mark is now a small char disc with a few stubby whiskers to about 0.6 of the reach.
  Round 5 had a char core inside a full ring of soot streaks out to the reach.

**Measured:** pixels marked against the clean last column fell by 45–55 % on rows 1, 3 and 7 at every age from 0.642
s to 6.023 s. For example, row 3: 2113 against 1113 px at 1.204 s, and 1368 against 648 px at 4.015 s. Rows 5 and 6
fell by about 30 %. At gameplay zoom the burn mark now reads as a dark dot. The reference board says "its scorch
marks the true reach". The flash, ring, fireball and char core are unchanged in kind.

The issue is not that this one draw is unlucky. The space of draws admits a shell with almost no blast mark.
Constrain the draw (the erosion threshold or phase, or the variant) so that every instance keeps its streaks to at
least about 0.8 of the reach. This is the place to do round 5's deferred improvement 1, an irregular streak fringe.

### `acid_burst`: **regression: no** (frame-0 note partly resolved)

Rounds 5 and 1 differ in only about 10,000 sheet pixels in all. The stain, its edge on the reach, the lip, the drying
and the crust look the same as in round 5 (rows 1, 3 and 7, 0.18–4.5 s, side by side).

- **Frame 0 (row 3 and row 7, enlarged):** the concentric inner disc is now about 0.3 of the radius (it was about
  0.55). Its ring of fine-spray dots is darker and greener, but it is **still there**: a small inner disc ringed by
  regular pale-green dots. The saucer look is smaller, not gone.
- **Frame 1:** the front now shows as a second concentric ring at about 0.8 of the radius (sequence frame 1). In
  round 5 it was nearer the edge.

The effect is still a single-frame or two-frame thing and is not blocking.

**Unchanged as stated, and still open:** the ghost inner rim at the upper left (row 3, every age), and the clean
ellipse edge without a meniscus.

### `spine_line`: **regression: no**

- **Clumps:** shape and height are unchanged (bounding box 167×16 against 167×15 sheet px, row 3, 0.973 s). Their
  dark area is about 15 % smaller (587 against 696 px), which is within the variation of a new draw.
- **Scar:** it is less periodic than in round 5 (row 3, 1.622 s; sequence frame 30). Round 5's improvement 1 is
  partly met.
- **New minor note:** with pieces scattered 0.3 across, one scar piece now lies as a detached blotch below the line
  near its far end (row 3, 1.622 s and 2.433 s; sequence frame 45).
- **Early heave:** it is smaller. At frame 2 (80 ms) the first point shows only a faint smudge, where round 5 had a
  clear heave patch. This matches the packet's 62 % early coverage. The first clump still stands at frame 4–6, so I
  do not count it as a regression of item 1.

Keep scattered pieces overlapping the line's centre, so none reads as detached.

---

## Crowd (item 6): not re-taken

The crowd frame was not re-taken with the new effects, so item 6 is open for all four. To judge it I would need the
following:

- **One stress capture** in the same setup as round 5, with all seven effects streamed: 200 a second, round-robin,
  over the 400-unit fight. It should come with `stress_capture.txt` figures (coverage, peak footprints per cell,
  retired instances).
- **Air units in that battle,** so that `flak_burst` is seen at the air plane over and among air units, and does not
  hide them.
- **Repeated hits:** one or more `stag_behemoth` attackers hitting at their real 0.6 s cadence, with a short sequence
  of the same attacker's consecutive cleaves. This checks stacking and silhouette repetition.
- **Crossing jets:** several `napalm_wheeler` jets crossing, to see whether the bubbly flames and beaded streaks
  add up to clutter.
- **Budget degradation:** a few frames across a budget retirement, to confirm that it is not visible as popping.

## Fog (item 7)

On the sheets nothing is drawn where it should not be, except the flame's back-fill behind the first search point
(see `flame_line` item 1). This matters little for a visible attacker.

The packet's fog test allows a line seen only from its fifth search point to draw up to 1 unit before that point.
That is four search points for `flame_line` and `lance_sweep` (spacing 0.25), much looser than the spine line's half
spacing. A fog sheet for `flame_line` would settle it. It is not blocking, because neither attacker is concealed.

## Consistency across all seven

**Partial.**

**Shared across all seven:** the upper-left key light, lasting marks darker than the ground (the flame and lance
streaks are near-neutral (70–77, 66–72, 60–66), and the cleave dust darkens teal), warm-grey smoke, and acid,
chitin and soil in their own palettes.

**Three inconsistencies:**

1. **Fire palette.** The shell and flak fireballs follow the board's ramp: saturated yellow-orange, then oxblood,
   then soot. The flame jet is a pale cream and the lance head a cream-yellow tube, which read as a different fire.
2. **Ring language.** There are three kinds of ring:
   - the shell's soft, glowing, wavering pressure front;
   - flak's glassy rim with even scallops;
   - cleave's thin dark pencil line.

   One rule should govern them all: a noise-broken front of varying width, brightest or densest at its leading
   edge, eroding out.
3. **Line marks.** The spine scar is torn earth. The flame streak is a row of beads and the lance scar a ruler
   stroke. Both new line marks should take the same broken, uneven-width treatment as the spine scar.

## Summary

| Effect | Verdict | Items failed | Top change |
|---|---|---|---|
| `flame_line` | **reject** | 4, 5 | remove the bubble-rim flame erosion; give the fire its orange ramp; make the streak one uneven scorched band |
| `lance_sweep` | **reject (narrow)** | 4 | turn the ruler-line scar into a ragged, uneven burnt groove with cooling embers; give the head a flare and sparks |
| `flak_burst` | **accept** | none | let the smoke drift and grow; irregular ring edge; shrapnel streaks |
| `cleave_impact` | **reject** | 3, 4, 5 | add an impact punch (flash, chunks, crack); replace the pencil ring with an eroding dust front; break the pinwheel per hit |

| Shipped effect | Regression | Evidence |
|---|---|---|
| `shell_impact` | **yes** | the streaked blast mark at the reach is mostly gone; marked area −45 to −55 % from 0.64 s to 6.0 s |
| `acid_burst` | no | about 10,000 px of change in all; frame 0's inner disc is smaller but its dots remain |
| `spine_line` | no | scar less periodic; one detached scar piece; early heave smaller |

| Area | Verdict |
|---|---|
| Consistency | partial (fire palette, ring language, line marks) |
| Crowd | open: needs a re-take with all seven effects, air units, repeated cleaves and crossing jets |
| Fog | pass on sheets; a fog sheet for `flame_line` is wanted |
