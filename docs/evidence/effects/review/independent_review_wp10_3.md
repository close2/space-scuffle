# Independent Review of WP10 (Area and Link Looks), Round 3

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendment 22.
Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP10 round 3". Round 2:
[`independent_review_wp10_2.md`](independent_review_wp10_2.md). This is round 3 of at most five (Q0080). I did not
build this work and did not review earlier rounds. I formed every verdict below before reading the implementer's
self-checklist.

## Method

I read round 2's review, the packet section, Amendment 22, the `abduct_tether` entry of `assets/catalog/effects.ron`,
and the round-3 diff of `crates/client/src/effects/areas.rs`, `runtime.rs` and `effect_runtime_tests.rs`. I built and
ran nothing. I cropped and zoomed the `wp10r3_*` pictures, and the matching `wp10r2_*` ones, with ImageMagick into
`~/scratch/wp10-review3/`, and read pixel values from the committed JPEGs (about ±3 levels). As round 2 required, I
judge the two areas only on A2-1's numbers and on the crowd 2x and `out` sheets.

## Verdict table

| Item | Verdict | Main reason |
|---|---|---|
| A2-1 `psionic_storm` | **ACCEPT** | Rays: 0.5 R 96.8, 0.92 R 83.5, 0.97 R 83.5 (radv), lavapipe within 1 level; the rim is now the darkest part of the disc. Band 89.6, spill 0. The 2x crowd shows two dark-rimmed violet discs, not soap bubbles. No `out` frame shows a bare ring |
| A2-1 `blinding_cloud` | **ACCEPT** | Rays: 116.0, 109.3, 103.7 (radv), lavapipe within 2. Band 68.9, spill 0. The edge tears apart in the exit before the body goes |
| A2-2 `abduct_tether` | **REJECT (A3-1)** | (b) is met: the tongue is opaque. (a) is met on paper only: the period is gone, but the bulges are so weak that at 1:1 the tongue is a rigid, even, tapering bar with no visible travel. (c) is met by the number, but on radv the grip reads as a barbed harpoon head: two side spikes and a point, with the tongue narrowing to a tip into the victim. That breaks the art rule (a fleshy tongue, not a weapon) |
| V2-1 fog overlay | **ACCEPT** | The cells are a clean perspective staircase that agrees with the floor's tiles. The yellow circle and cyan cut agree with the drawn half-disc, and no violet pixel lies in a darkened cell |
| E2-1 test | **ACCEPT** | It moves the clip from A to B, asserts B on every live effect (at least four laid under A) on B's first frame, then `CLIP_NOTHING` on the frame sight is lost |
| P2-1 (taken) | **ACCEPT** | Cells taken over `radius + CUT_MARGIN`; the unit test is the case asked for; the `CHILD_SLOTS` comment is right |
| P2-4 (taken) | **ACCEPT** (with a test advisory, P3-4) | `retire` frees the slot at once; the slot is then unkept and released on a stale handle, which `EffectPool::free` ignores by generation. Correct |
| Regressions | **none** | Edge errors unchanged (1.72 and 1.51 px); fog: 0 of 194 unseen samples change on both drivers; tether ends 0.52 and 0.71 px |
| **Overall** | **REJECT (round 4, tether only)** | Both areas and all the engineering are accepted and closed. Only the tongue's grip, tip and bulge amplitude remain (A3-1) |

## 1. The areas (A2-1): accept

**Numbers.** I could check them roughly: a horizontal luminance profile through the storm in `wp10r3_areas_radv.jpg`
reads 80 to 93 in the rim on both sides and 70 to 163 inside, which agrees with the reported medians (rim about 84,
body about 97). All four conditions hold on both drivers, with margins of 13 to 14 levels (storm) and 6 to 12 levels
(cloud). E1-5 still holds (band 89.6 and 68.9 against 32; spill 0).

**Crowd 2x.** The two storms read as two dark-rimmed violet discs; the second storm's rim crosses the first as a darker
arc, which is right for overlapping areas. Nothing is lighter at the edge and nothing approaches white. The units
inside read; their rainbow speckle is B-120, correctly registered in `DESIGN.md` and not a WP10 defect. The cloud is
an even grey pall with a slightly darker lip; all eight units read.

**`out` sheets.** Storm: in frames 2 to 5 of row 2 the ending storm's rim goes first and its body leaves as eroding
violet fragments; no ring stands alone. Cloud: frame 2 of row 2 shows ring and body fading together, and frame 3
shows torn fragments of the edge with body murk around them, not a circle. Neither shows a bare ring.

A2-1 is met. Two taste notes are polish only (P3-1, P3-2).

## 2. `abduct_tether`: reject

Evidence: `wp10r3_tether_{radv,lavapipe}.jpg` (zoomed 4x in `~/scratch/wp10-review3/tether_*.png`) and frames 3 to
10 of `wp10r3_tether_sequence_radv.jpg` (`~/scratch/wp10-review3/tether_seq.png`).

**(b) Opaque silhouette: met.** 1 px inside each edge reads (88, 27, 25) and (78, 24, 21) on radv, and (83, 25, 23)
and (91, 29, 26) on lavapipe, all within 25 levels of the brief. There is no floor blend inside the edge. The edge is
now a hard, aliased stair-step at 3x (polish P3-3).

**(a) No period, visible travel: the letter is met, the intent is not.** I counted the tongue's vertical extent in each
column of the 3x shot. Through the middle it varies between 13 and 18 px, with peaks at irregular spacings of 26 to
52 px. The period is gone. But a ±15 % ripple is about one screen pixel at 1:1. In the 40 ms sequence the tongue is
an even, straight bar that thickens at the caster and thins towards the victim, and no bulge can be seen moving. The
cause is in the catalog: the core is `scale: 0.15`, the strips `0.19` and `0.2`, so a bulge can add only about 30 % to
the width, and `hold_dense` fills the puff's soft falloff, so the strips read as solid bands. The two scroll speeds
are also nearly equal (about 0.5 and 0.55 units a second), so the two strips move together. Round 2 traded a bead
chain for a rod. A tongue needs peristalsis that can be seen.

**(c) Grip: the number passes; the shape fails the art rule.** Ratio 2.57 on radv and 2.00 on lavapipe, against 1.8. On
radv the grip is a hard-edged four-point star: a spike up, a spike down and a point running into the victim. The
tongue narrows from 16 px to 3 px over its last 4 px before the bubble. Together with the flared swelling at the
caster and the taper along the length, the whole reads as a harpoon or spear thrown from the caster, with barbs
behind its head. On lavapipe the same layer is a frayed clump that reads as a ragged end, not a spear. That difference
is the clue: the grip is a `smoke_puff` sprite with `rotation: Random`, noise breakup and `hold_dense`. Pushing a
soft puff to 2.5 times its coverage turns its noisy falloff into hard spikes, and the shape is whatever the rotation and
the driver's rounding of the threshold give. That is also why lavapipe sits exactly at 2.00: the measured width
depends on where a spike happens to point. A fleshy tongue's tip is blunt and round, and it presses or wraps the
victim; it does not narrow to a point.

The offset is also expressed in the wrong unit. 7 % of the tongue's length is about 0.63 units at Abduct's range of 9,
nearly twice the grip's radius (the round-2 packet gives the grip as 0.68 units across, so 0.34). On a long tether the grip floats in front of the victim,
which strengthens the harpoon reading. Round 2 asked for about 0.6 of the grip's radius from the surface.

**Other checklist items.** Timing: pass (appears with the cast, follows, gone on contact). Colour: pass. Fog: pass
(the enemy tether is not drawn; P2-4 now removes it at once). Crowd: low risk.

## 3. Fog overlay (V2-1) and engineering

**V2-1: accept.** In `wp10r3_fogcells_radv.jpg` (zoomed: `~/scratch/wp10-review3/fogcells.png`) the 36 darkened cells
form a staircase of parallelograms whose edges run parallel to the floor's tile seams, so the slant is the camera's
perspective, not a bad projection. No cell is darkened twice. The yellow ellipse is the area's true circle and the
cyan cut runs inside it. Every violet pixel of the enemy storm lies on the seen side of the cut and on undarkened
cells, and the darkened cells start at or just beyond the cut. This agrees with 0 of 194 unseen samples changed. The
whole disc lies on the map. The worst neighbour ratio of 1.08 is plausible for this grid.

**E2-1: accept.** The test now holds clip A for ten frames, moves to a different clip B, and asserts on B's first
frame that every live effect (at least four, laid under A) carries B. It then asserts `CLIP_NOTHING` and the release on
the frame sight is lost. A clip set only at lay time would fail it.

**P2-1, P2-4: accept** (see the table). `retire` frees the instance; the slot, left unkept, is removed by the
`retain` pass, whose `release` on the now-stale handle is ignored by the pool's generation check. Nothing is left
behind.

## Required change

| ID | Change |
|---|---|
| A3-1 | `abduct_tether`, three parts, judged on the 3x shot and the 40 ms sequence on both drivers. (a) **Grip and tip, no point and no barbs.** Make the victim's grip a blunt, rounded swelling: a round, soft texture such as `droplet` (or the core's `Radial` shape), with fixed rotation, little or no noise breakup, and not pushed by `hold_dense` into hard spikes. The tongue must not narrow into the victim: report its width 1 px outside the victim's bubble, which must be at least the middle-half median. Report the grip's width on both drivers; the ratio must be at least 1.8 on each, and the two drivers within 20 % of each other (a stable shape, not a spike that happens to point the right way). (b) **Offset in the grip's own unit.** Set the grip back from the victim's surface by about 0.6 of the grip's radius, independent of the tongue's length, so it sits against the bubble at any range. Show one shot at Abduct's full range of 9 as well. (c) **Visible bulges.** Make the bulges wide enough to see at 1:1: over the middle half of the 3x shot, the widest column must be at least 1.6 times the narrowest, with no run of three equal, evenly spaced bulges (the round-2 condition stays). Give the two strips clearly different speeds (for example −1.6 and −0.9 in scroll, so one overtakes the other), and in the sequence a bulge must be seen to move at least 2 px towards the caster between two frames 80 ms apart; report its positions. Keep (b) of A2-2 (opaque edges within 25 levels) |

Nothing else about WP10 is reopened. Round 4 judges only the tether, on A3-1 and the seven checklist items.

## Non-blocking polish and advisories

- **P3-1 (storm, taste).** The rim is now the darkest, most saturated element, and the body has floor-bright holes
  (the profile reaches 163 inside, against a floor of about 200, and mid radius is now (87, 73, 128) against round 2's
  (74, 46, 138)). The storm reads a little like an inked outline round a mottled disc. Softening the band's inner side
  into the body, or filling the body a little more, would make it one storm cloud. This is the owner's taste.
- **P3-2 (both areas).** The outline is still nearly a perfect circle at 1:1; `edge_noise` 0.05 is barely visible.
  Breakup on the band's outer lip, within the ±2 px edge test, would help. Not required.
- **P3-3 (tether).** The edges are hard and aliased at 3x. An opaque ramp key does not require a hard alpha edge; a
  1 px soft rim in the dark edge colour would keep A2-2 (b) and lose the stair-steps.
- **P3-4 (P2-4 test).** No test covers the tether's retirement when an end leaves sight. One assertion in
  `effect_runtime_tests.rs` (an end becomes not shown; on that frame the tether's handle is dead and nothing of it is
  drawn) would pin it, as the charter asks for every behaviour.
- P2-2, P2-3, P2-5, the cliff case, P1-1, P1-2 and F1 stand as recorded in Amendment 22.
