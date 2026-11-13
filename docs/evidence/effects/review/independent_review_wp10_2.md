# Independent Review of WP10 (Area and Link Looks), Round 2

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendment 21.
Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP10 round 2". Round 1:
[`independent_review_wp10_1.md`](independent_review_wp10_1.md). This is round 2 of at most five (Q0080). I did not
build this work and did not review round 1. I formed every verdict below before reading the implementer's
self-checklist.

## Method

I read round 1's review, the packet section, Amendment 21, the WP10 entries of `assets/catalog/effects.ron` (the looks
and the `storm_*`, `murk*` and `tongue` ramps), and `crates/client/src/effects/areas.rs` in full. I also read the
measuring, crowd and fog-overlay parts of `crates/client/tests/area_link_gpu_tests.rs` (`edge_visibility`,
`darken_quad`, `project`, the `FogCritical` staging) and the E1-2 test in `effect_runtime_tests.rs`. I built and ran
nothing. I cropped and zoomed every `wp10r2_*` picture with ImageMagick into `~/scratch/wp10-review2/` and read pixel
values from the committed JPEGs, so they are accurate to about ±3 levels. Unless I say otherwise, I describe radv.
lavapipe matches it: the measurements agree to within 7 levels, and the 2x storm crop matches pixel for pixel in
structure.

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| `psionic_storm` | **accept on condition A2-1** | A large step up. It has a dark, saturated indigo body that is darker than the floor (centre (79, 59, 133) on a floor of (176, 178, 172)). Two overlapping storms stay indigo; nothing approaches white. Units inside read clearly, and nothing is a ray or stripe. What remains: the outer band (0.85 to 1.0 R) is lighter and more transparent than the body, so the disc still has a glassy rim. Two overlapping storms read as two overlapping soap bubbles |
| `blinding_cloud` | **accept on condition A2-1** | It now reads as a dark grey pall covering the disc evenly (centre 113 against a floor of 193; mid radius 101). Units inside stay fully readable. It has the same rim defect: a lighter, translucent annulus between the dark murk and a perfect circular outline, which is what P1-4 noted. During the exit the outline lingers as a bare ring after the body has gone |
| `abduct_tether` | **reject (A2-2)** | It is no longer a laser: it is dark, with no light core. But the bulges are one texture strip repeating every 0.32 units, so at 1:1 the tongue reads as an evenly spaced bead chain or dotted line. That is visibly procedural, not a muscle. The grip at the victim sits centred on the bubble's surface, so the bubble hides half of it and it is barely wider than a bead. The silhouette is translucent: 2 px off the line it shows as (102, 85, 84), mostly floor, where the brief asks for dark edges of (45, 12, 12) |
| Overlapping storms not burning to white | **pass** | The brightest pixel inside the overlap in the end sequence is a unit glint. The body stays at about (74, 46, 138) |
| Units inside readable | **pass** (both areas) | The flat sheets lie under the bubbles. See the note on the storm units' rainbow speckle below: it is not a WP10 defect |
| Art rules (no rays, stripes, sunburst, beams or speed lines) | **pass** for both areas; **tether: no beam, but a bead chain** | Covered by A2-2 |
| A1-4 linear ramps | **pass** | Every WP10 key is the linear value of its noted sRGB colour. The sampled colours land on their briefs (storm (74, 46, 138) against the (45 to 80, 25 to 50, 95 to 145) ramp under a darkened floor; tongue centre (90, 35, 33) against (95, 30, 28)) |
| E1-1 exact fail-closed cut | **accept** (polish P2-1) | Exact for the disc: every corner of every unseen cell the disc overlaps lies at least `CUT_MARGIN` behind the cut, and cells are convex, so no unseen ground inside R is drawn. The unit test is the right one |
| E1-2 same-frame drop on lost sight | **accept, strengthen the test (E2-1)** | The code is right. The test would also pass if the children's clips were set only when they are laid |
| E1-3 binary search | **accept** | Insertion at the `Err` index keeps the slots sorted; refusal at capacity means no allocation. A `PERFORMANCE RATIONALE` block is present |
| E1-4 valgrind window | **accept** | `EffectStream` 0 blocks at 64 and 128 frames, with an enemy area seen in part under a real `BattleFog` |
| E1-5 contrast and spill | **accept** | Meaningful and passing: a band median of 64.8 (storm) and 53.5 (cloud) against a minimum of 32, and 0 spill pixels. It does not catch a rim that is *lighter* than the body (it measures absolute change); A2-1 adds that |
| V1-1 timing sequences | **pass** | The storm is whole by its third 40 ms frame, which is right for a storm. The cloud fades in over about 0.3 s, and both erode out over about 5 frames with no pop. The tether appears with the cast, follows, and is gone on contact |
| V1-2 crowd and overlap | **pass, with evidence notes** | See section 4 |
| V1-3 unseen-cell overlay | **reject the overlay (V2-1)** | It cannot be trusted: diagonal edges span several cells, and some bands are darkened twice. A grid of 1-unit cells on a flat floor cannot produce either |
| V1-3 cliff picture | **deferral accepted** | See section 5 |
| Round-1 accepts regressed? | **none** | The fog rules hold: 0 of 277 unseen samples change, on both drivers. The tether ends test is unchanged (0.52 and 0.71 px). The edge errors are now within ±1.72 px, with the cloud's worst ray improved from −1.94 to −1.52 |
| **Overall** | **REJECT (round 3 needed, narrow)** | Storm and cloud need only A2-1, a measurable rim fix. The tether needs A2-2. The fog overlay needs V2-1. Engineering needs one test strengthened |

## 1. `psionic_storm`: accept on condition A2-1

Evidence: `wp10r2_areas_*`, `wp10r2_psionic_storm_{0,1}_{in,out}_*`, `wp10r2_crowd2x_0_*`, `wp10r2_crowd_*`.

1. **Readable at gameplay zoom: pass.** At 1:1 it is an unmistakable violet danger zone, clearly separate from
   the bubbles and the floor.
2. **Layered: pass.** The body sheets, the darkened floor, the standing billows and the edge band all read. The laid
   flares cannot be seen at 1:1 (P2-3).
3. **Timing: pass** (V1-1).
4. **Cheapness: mostly pass.** The body is mottled and irregular, and tile seams show only faintly through its
   lower quarter.
5. **Value and colour: pass for the body; fail for the band.** A profile from the centre outwards along the
   horizontal in frame 6 of the come-in reads (72, 60, 110) to (79, 58, 133) in the body. It then lightens to
   (104 to 116, 90 to 102, 152 to 163) in the outer band, and ends on a crisp circle at 40 px. The measurements say
   the same: at 0.95 R it is (118, 106, 168), against (74, 46, 138) at mid radius. The cause is in the catalog: the
   body sheets sit in a disc at 0.5 R and a circle at 0.6 R with size 0.34 R, so they stop short of the edge. The
   band is a `smoke_puff` ring whose texture fades towards its lip, so `storm_rim`'s "darkest at its lip" key is
   multiplied by a low texture alpha. On screen this gives a dark interior, a lighter translucent annulus and a thin
   outline: the structure of a glass bubble. With two storms overlapping (crowd 2x) it reads as two soap bubbles.
6. **Crowd: pass.** The four units still inside at 2.0 s read clearly over the storm. Their rainbow speckle is the
   bubble shader's existing low-health glitch (`FLAG_CRT_GLITCH`, health below 0.35, `bubble_dome.wgsl`); they are
   taking storm damage. It is not a WP10 artefact.
7. **Fog: pass** (section 3).

## 2. `blinding_cloud`: accept on condition A2-1

Evidence: `wp10r2_areas_*` (top), `wp10r2_blinding_cloud_0_{in,out}_*`, `wp10r2_crowd2x_1_*`.

1. **Reads: pass.** It is a dark grey pall that darkens the ground to about 55 % of its value (centre 113 against
   193, mid radius 101 against 176), evenly across the disc. Round 1's one-sided smudge is gone.
2. **Layered: pass in structure.** The rolling `murk_billow`s can barely be seen at 1:1, so the cloud reads as a
   flat shade, not a rolling murk. That is acceptable for a top-down `Major` area.
3. **Timing: pass**, with one defect. In the exit sequence (row 2, frames 4 and 5) the body has gone and a faint
   circular outline is left alone for a frame or two: a bare glass ring.
4. **Cheapness: rim only.** The outline is a perfect circle (`edge_noise: 0.005`), and inside it lies a lighter,
   more transparent annulus. The circle placement at 0.6 R with size 0.31 R ends the murk at about 0.9 R. The
   `murk_edge` front is meant to be darkest at its lip, but the edge band's change is 53.5 levels, which puts its
   median at about 140 on a floor of 193, far lighter than the authored factor of (90, 90, 105). That is again the
   texture's falloff at the lip.
5. **Value: pass** for the body. The cold cast is present but faint, which is acceptable.
6. **Crowd: pass.** All eight units read cleanly.
7. **Fog: pass.**

**P1-4 (against `barrier_shell` and the glass bubbles).** Scale and placement keep both areas apart from the barrier
shell, a dome around one unit. Against the glass bubbles they share one feature only, the lighter annulus with a
circular outline. A2-1 removes it.

## 3. `abduct_tether`: reject

Evidence: `wp10r2_tether_*` (3x: whole, `Critical`, none), `wp10r2_tether_sequence_*`, `wp10r2_fog_*`. Zoomed to 9x
in `~/scratch/wp10-review2/tether_whole_9x.png`.

1. **Reads: partly.** It no longer reads as a beam: it is dark red-brown with no light core. At 1:1 it reads as a
   dotted or beaded red line between two units. That is legible as "this unit grabbed that one", but it does not
   read as a tongue.
2. **Layered:** there are three layers. The grip layer shows at the caster as a small fan. At the victim it is
   centred on the bubble's surface point (`Line(spacing: LinkLength)` at the two ends of the line, which
   `tether_ends` puts on the surfaces), so the victim's bubble covers half of it, and what is visible is hardly
   wider than one bead.
3. **Timing: pass.**
4. **Cheapness: fail.** The bulge layer is one `smoke_puff` strip with `repeat: 0.32`, so the 3x shot shows about 13
   identical, evenly spaced diamonds. A perfectly regular period is the signature of a tiled texture. The width
   varies, but only as a strict alternation, and the line stays straight with a constant envelope.
5. **Colour: pass on the centre line** ((90, 35, 33) against (95, 30, 28)), **fail on the edges.** 2 px off the line
   it is (102, 85, 84) on radv and (116, 110, 110) on lavapipe, which is mostly floor. The silhouette is soft and
   translucent where the brief asks for dark, opaque edges of (45, 12, 12).
6. **Crowd:** low risk.
7. **Fog: pass.** In the fog shot the enemy tether is not drawn. The viewer's own tether is drawn whole.

## 4. Evidence (V1-2) and staging notes

- In the storm crowd, the ring of eight units sits between the two storms, so each storm covers about half of it.
  The packet says "eight units in each area"; for the storms that is not what the picture shows. The two sides
  cannot be told apart at this zoom either. Neither point blocks: readability is shown.
- `wp10r2_crowd_*.jpg` is 2560 px wide with the right half black (an empty sheet slot).
- P1-3 (staging away from the map corner) was not taken. It now matters for the fog shot (V2-1).

## 5. Fog and engineering

**E1-1, accept.** `area_sight` collects the cells the disc overlaps, using a nearest-point test against R. For each of
32 axes it puts the cut `CUT_MARGIN` beyond the farthest corner of every unseen cell, and it keeps the axis that
retains the most seen samples. Because cells are convex and all four of their corners lie behind the cut, no unseen
point of the disc is on the drawn side: this is exact, not sampled. The unit tests (an edge between two old samples,
and a diagonal staircase, asserting no unseen corner is drawn) are the ones round 1 asked for. Two small points
(P2-1): an unseen cell that touches only the annulus from R to R + 0.05 is not counted, while the look's soft lip and
the clip feather can reach a little past R. Taking the cells over `radius + CUT_MARGIN` closes that at no cost, and
makes "fail-closed by construction" literally true. Also, the comment on `CHILD_SLOTS` still says "a lay every
0.11 s"; it is now 0.16 s for the storm and 0.2 s with 1.4 s lives for the cloud (7 alive, under 16).

**E1-2, accept the code; strengthen the test (E2-1).** `keep_area` cuts the slot and every child to `CLIP_NOTHING`
when sight is lost, before releasing it. A held area re-sets every child's clip each frame, and handles carry
generations, so a reused slot is never clipped by mistake. The test, however, holds one constant clip for ten frames
and then checks that the children carry it. That would pass even if a child's clip were set only when it is laid. A
tether whose end goes out of sight is released with a 0.15 s exit at its last line. That reveals nothing new, since
the line no longer follows, so it is advisory (P2-4).

**E1-3, E1-4, E1-5: accept** (see the table).

**V1-3 overlay, reject (V2-1).** In `wp10r2_fogcells_radv.jpg` (zoom: `~/scratch/wp10-review2/fogcells_zoom.png`), the
darkened region has straight diagonal edges spanning several cells (top left, and the right side running about 45°).
It also has a band darkened twice, across exactly the rows where the crescent stands. A grid of 1-unit cells on a
flat floor projects to non-overlapping quads with near-horizontal and perspective-vertical edges, so neither feature
can be real. The likely cause: the enemy area is staged at the map's top edge. `darken_quad` is given cells up to 5
rows beyond it, and `project` reads `terrain_altitude_at` off the map, which produces stretched quads that overlap
cells on the map. As it stands, the crescent's upper half appears to lie in darkened cells, which contradicts the
measured 0 of 277. The measurement is the trustworthy one, but evidence that seems to show a leak is worse than no
evidence.

**Cliff picture: deferral accepted.** The cut is computed per fog cell with no altitude term, so the unit tests and
the 0-of-277 count cover its correctness on any ground. The risk at a cliff is visual: decals smearing down a cliff
face, and cards standing over the lip. That belongs to the per-pixel mask (P1-1, already a condition before any area
larger than radius 2 ships) and to the decal system. Record it beside P1-1 as "show one partly seen area across a
cliff edge when the mask lands", with a staged map that has one.

## Required changes

| ID | Change |
|---|---|
| A2-1 | `psionic_storm` and `blinding_cloud`: the outer band must be at least as dark as the body, and the outline must not be a perfect glass circle. (a) In the measurements, report the median luminance over all 32 rays at 0.5 R, 0.8 R, 0.92 R and 0.97 R for each area on both drivers. Require L(0.92 R) ≤ L(0.5 R) + 4 and L(0.97 R) ≤ L(0.5 R) + 4. Keep E1-5's band ≥ 32 and spill 0. (b) Suggested route, catalog only: move each area's `Circle` placement out (about 0.72 R) so the sheets reach about 0.97 R. Give the edge band a mapping whose alpha does not fall to zero at the lip, so the authored "darkest at the lip" key shows. Raise the outline's irregularity (`edge_noise` about 0.02 to 0.03, or breakup on the ring's outer lip) as far as the ±2 px edge test allows. (c) The edge must not outlast the body in the exit: no frame of the `out` sequences may show a bare ring. Set the ring's exit at most equal to the body's, or let it erode with it. Round 3 judges both areas on these numbers plus the crowd 2x and `out` sheets; nothing else about them is reopened |
| A2-2 | `abduct_tether`: (a) No visible period: replace the single `repeat: 0.32` strip with two bulge strips of incommensurate repeats and scroll speeds (for example 0.32 and 0.53), or with width modulated by noise. The 3x shot must not show a run of three equal, evenly spaced bulges, and bulges must still visibly travel towards the caster in the sequence. (b) An opaque silhouette: 1 px inside the tongue's edge, the colour is within 25 levels per channel of (45, 12, 12) to (95, 30, 28), not a blend with the floor; report it beside the centre-line sample. (c) A visible grip: offset the victim's grip outwards from the surface point along the tongue (about 0.6 of its radius), so its visible width is at least 1.8 times the tongue's median width in the 3x shot. Report both widths in the measurements |
| V2-1 | Make the fog overlay trustworthy: darken only cells on the map. Assert that each projected quad's area is within a factor of 2 of its neighbours'. Draw the area's true circle and the cut line on the overlay. Stage the partly seen enemy area away from the map edge, so that the whole disc lies on the map (P1-3 becomes required for this shot only) |
| E2-1 | Strengthen the E1-2 test: hold the area under clip A for several frames, then under a different clip B, and assert that every live child carries B (not A) on B's first frame, then `CLIP_NOTHING` on the frame sight is lost |

## Non-blocking polish and advisories

- **P2-1** Take the unseen cells over `radius + CUT_MARGIN` in `area_sight`, with a unit test of an unseen cell
  touching only that annulus. Fix the stale `CHILD_SLOTS` comment.
- **P2-2** `area_sight` evaluates `sees` on 289 samples for each of 32 axes (about 9 000 calls per partly seen area
  per frame). Collect the seen samples once and only take dot products per axis.
- **P2-3** The storm reads as violet smoke rather than psionic energy: the laid flares cannot be seen at 1:1.
  Smaller, slightly brighter pockets that flicker in place (no rays, kept below white where two storms overlap) would
  give it charge. This is the owner's taste.
- **P2-4** Retire a tether at once (no exit) when it is dropped because an end left sight. Consider putting the grip
  into the `Critical` layer, so the LOD fallback is not a bare straight line.
- **P2-5** Evidence: fill or drop the black half of `wp10r2_crowd_*`, and centre one ring of units in each storm.
- P1-1 (per-pixel mask before any area larger than radius 2), P1-2 (sag) and F1 (Fungal Growth) stand as recorded
  in Amendment 21.
