# Independent Review of the Effect Scale-Out, Round 3 (WP7)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3 and
Amendment 6. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "Scale-out round 3
(WP7)". Previous round: [`independent_review_scaleout2.md`](independent_review_scaleout2.md). Checklist: plan §7.2,
as quoted at the end of the packet.

## Method

I judged from the packet, the round-2 review, Amendment 6 and the images. I formed every verdict before I read the
implementer's round-3 self-assessment in `slice_review.md`, and I read it afterwards only to check factual claims
(named below). I read no code beyond the `effects.ron` diff, and I built and ran nothing.

The material was the `cleave_impact` sheet, sequence and repeat take, the pop table, and the four crowd takes. I
cropped cells and frames and enlarged them 4x to 8x with nearest-neighbour scaling (ImageMagick). I measured in five
ways:

- **Extents:** the bounding box of the pixels that differ by more than 3–5 % (blurred 1 px, opened) from the same
  row's clean last column, against the yellow reach circle in the reference row.
- **Pops:** the pixels that change by more than 3 % between consecutive sequence frames, on a 70×50 crop around the
  effect in the 240×135 sequence cells. I ran the same crop on round 2's sequence (`git show 0988f84:`), so the two
  rounds are measured identically and independently of the implementer's own table.
- **Per-hit identity:** the repeat take's frames 3/18/33/48 and 5/20/35/50 (the same age of each of the four hits)
  side by side.
- **Crowd stability:** one 140×110 region tracked across all eight frames of the degradation take and of the
  forced-cap take.
- **Identity:** `compare -metric AE` of every non-cleave sheet, sequence and GPU golden against commit `0988f84`.

Rows 1–8 are as in earlier rounds (1/3 bright default/close, 5/7 dark default/close, even rows their references).
Sequence frames are numbered from 0 and are 40 ms apart. Sheet px are pixels of the 2560×1440 sheet.

**Verdicts in brief:**

- **`cleave_impact` is accepted**, with notes. The glass bubble, the pop and the identical circle are fixed.
- **The footprint rule change is sound in intent but one clause is too weak**: "the farthest ray is within 2 px" is
  satisfied by a single clump. I accept it with that clause made per sector (below).
- **The other six did not regress**: 48 files, AE 0 against round 2.
- **Crowd: the decal headroom and the degradation frames pass. The jets finding is a real defect** that the crowd
  pass cannot ignore: in a normal battle the napalm jet reads as a thin orange laser beam. I recommend option 1
  (count against the class cap only while Detail layers are alive), not raising the cap.

---

## 1. `cleave_impact` (heavy blow; Ultralisk analogue): **accept** (with notes)

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | **The outermost dust marks the reach.** Row 3, 0.142 s: marks span 116–203 × 55–131 against the yellow reach at 114–206 × 54–132 (inside by 1–3 px, nothing beyond). Row 1, 0.142 s: 132–188 × 67–115 against 130–190 × 65–116. At 0.076 s (frame 2) the front already spans 121–199 (row 3), so the area is identifiable within 0.1 s. The flash is the single focal point at frames 0–1. **Note:** at the default zoom on the bright ground the front is faint: it needs a 3 % threshold to measure, and by eye it is a thin, light ragged line (sequence frames 2–5, 6x). It reads, but only just. |
| 2 | Layered | pass | Flash and light (frames 0–1), sparks, the torn front, the central dust mass, dust puffs, dark clods. The crack is still not recognisable on any cell. |
| 3 | Timing | pass | **The pop is gone.** On my crop, the steps 4→5 … 9→10 change 83, 55, 43, **39**, 54, 62 px; round 2 on the same crop: 78, 52, 46, **370**, 145, 57. In a 6x strip of frames 2–9 the front thins and breaks into fewer, fainter clumps over frames 5–8 and is gone at 9, with no frame where it vanishes at once. The implementer's full-frame table ([`cleave_impact_pops.txt`](cleave_impact_pops.txt)) shows the same shape. The later bump at 10→12 (100, 116 px) is the central dust eroding and the last clod going; it was identical in round 2, and by eye it is a smooth erosion. Every row is clean at 0.712 s. |
| 4 | No tells of cheapness | pass (with notes) | **(a) No bubble.** At 0.076–0.285 s (rows 3 and 7, 8x) there is no tinted disc: inside the front is bare ground, with the dust mass offset towards one side. The front is a band of clumps with gaps (left side at 0.142 s on teal, lower left on bright) and varying width. **(b) No identical circle.** At equal ages the four hits of the repeat take have their gaps and thick clumps in different places, and the central dust differs in shape. **Notes:** (i) on teal (row 7, 0.076–0.142 s, 8x) the front is a *darker* grey-brown than the teal while the central dust is a light tan. The front therefore reads as a dark beaded outline drawn around light dust rather than as the same dust thrown outward, and along its upper right its bumps are fairly regular, like a bottle-cap edge. On the bright ground it reads as soft dust. (ii) The clods are still squarish (row 3, 0.285 s). (iii) The crack is still not readable. None of these shows at the default zoom. |
| 5 | Value and colour | pass | The flash is the brightest point at frames 0–1, the clods are the darkest, and the dust lies between. See note 4(i) for the front's value on teal. |
| 6 | Crowd | pass | In the crowd frames it cannot be picked out (small, short, brown), as in round 2. In the repeat take a hit's last faint dust (frames 15–17 of the single take: 312, 115, 16 px) overlaps the next hit's flash by two or three frames; that is invisible under the flash. |
| 7 | Fog | pass | Nothing is drawn beyond the reach on any row or age. |

**Round-2 points:** glass bubble **fixed**; pop at frame 7→8 **fixed** (39 px against 370 on the same crop); same
circle per hit **fixed**; crack and square clods **open** (optional, as before).

**Optional polish**

1. Draw the front in the same light-dust value as the central mass (at least on dark ground), so that it reads as
   dust thrown outward and not as an outline. Break the regular bump period on the front's leading edge with a
   second, lower noise octave.
2. Give the front a little more opacity at the default zoom on light ground (frames 2–4).
3. As before: a readable crack at close zoom; irregular clod silhouettes.

---

## 2. The footprint rule for fronts with `edge_noise`: **sound, with one clause strengthened**

The old rule (85 % of 72 rays within ±2 px of the reach) cannot be met by any torn edge deeper than 7 % of the
radius. That is a test that dictates a look, and the look it forces (a smooth ellipse) is exactly what two review
rounds rejected. Changing it is right. The two contracts that matter to a player are (a) the drawn area never
claims more than the reach, and (b) the reach's outline is shown by the outermost dust. The new rule keeps (a)
unchanged and gives (b) only partly.

**The weakness.** Clause (b) is "the farthest ray ends within 2 px of the reach". One clump on one ray satisfies it.
Combined with the widened band (inside by up to `edge_noise × thickness` = 15 % of the reach, about 4.4 px at the
default zoom, plus 2 px), the rule would pass a front that is correct on one side and systematically short on the
other: a squashed ellipse from a wrong pitch projection, an aspect error, or a missing quadrant from a broken noise
phase. Those are real defect classes for this code path. A global scale error larger than 2 px would still fail,
and a centre offset larger than 2 px would fail clause (a).

The implementer reports a mean ray error of −5.4 px with 89 % within the band; the bite (about 4.4 px at default
zoom) plus 2 px gives a band to −6.4 px, so the typical ray sits near the inner edge of what is allowed. That is
consistent with the look, but it leaves little evidence that the outline is carried all the way round.

**What I would accept:** the rule as proposed, plus:

1. **Per sector, not global, for clause (b).** Split the 72 rays into 8 sectors of 45°; in every sector the
   farthest ray must end within 2 px of the reach. This keeps the torn look (each sector needs one clump on the
   reach) and catches anisotropic and quadrant defects.
2. **The bite is bounded by the declaration.** Keep it derived from the declared `edge_noise × thickness`, and cap
   `edge_noise` in the catalog check (it is already in [0, 1)); a front should not be able to buy a large tolerance
   by declaring a large bite. I would cap the resulting bite at 20 % of the reach.
3. Round fronts (bite 0) keep the old rule, as proposed.

My measured extents (item 1 above) show this front would pass the per-sector clause: its bounding box touches the
reach on all four sides at 0.142 s.

---

## 3. Regression check: **no regression**

`git diff --stat 0988f84 eb2155c` touches no sheet, sequence or golden of the other six effects. `compare -metric
AE` against `git show 0988f84:` gives **0** for all 48 files: the contact sheet, the sequence and the six GPU goldens
(`crates/client/tests/data/effects/{lavapipe,radv}/<effect>_{0,1,2}.png`) of each of `flame_line`, `lance_sweep`,
`shell_impact`, `acid_burst`, `spine_line` and `flak_burst`. The `effects.ron` diff changes only the cleave's ramp,
its two curves, its front block and the decal pool size. This shows the files are unchanged; that the shader's new
`front_edge` row draws the round fronts identically rests on the GPU tests passing against these unchanged goldens,
which the implementer reports (mean 0.000/255) and which I did not run.

---

## Crowd (item 6)

### Decal pool 6 144 and behaviour at the cap: **pool accepted; the at-cap behaviour is not acceptable as a design, only as an unreachable state**

- **Headroom.** 4 206–4 296 decals a frame at one tick per frame against 6 144 is 30 % headroom, which meets
  round 2's ask (about 25 %). Accepted.
- **At the cap.** I tracked one region over the eight frames of [`stress_decal_cap_400.png`](stress_decal_cap_400.png)
  and of [`stress_degradation_400.png`](stress_degradation_400.png). In the normal take the two acid stains in it are
  present and steady in all eight frames. In the forced-cap take a stain shows in frame 0 only as its dotted green
  rim, is gone in frame 1, and another stain appears as a bare green ring from frame 1. So at the cap two things go
  wrong: lasting marks flicker in and out frame to frame, **and** effects lose part of their decals, so an acid
  stain becomes a hollow ring. Both are pops (item 6) and the second is a wrong picture.
- **Is that acceptable?** Not as the cap's designed behaviour. Headroom makes it unlikely, not impossible: a bigger
  fight, more napalm wheelers, or a longer stain will reach it, and the failure is the most visible kind. The cap
  should degrade **stably**:
  1. **Sticky slots.** An effect's decals get their slots when it is born and keep them until it ends; the per-frame
     write order must not decide which lasting marks are drawn. A mark that was drawn last frame is drawn this frame.
  2. **Whole effects, not parts.** When the pool is full, a new Critical decal set is either admitted whole or, if
     it must be, the **oldest** lasting mark is retired by a short erosion (0.2–0.3 s) to make room. Never draw a
     subset of one effect's decals.
  3. **Pressure before the cap.** Above about 85 % occupancy, shorten lasting marks' remaining hold (they erode
     earlier) so the pool rarely reaches 100 %.
  4. A test at a forced small pool that asserts no decal that was drawn in frame N is missing in frame N+1 unless its
     effect is eroding.

### Degradation frames: **pass**

[`stress_degradation_400.png`](stress_degradation_400.png): eight frames 40 ms apart, six retirements and continuing
Detail skips. I could not find the retirements by eye in any pair of consecutive frames, which is the pass
condition, and the tracked region's stains are steady. The decal count falls smoothly (4 296 → 4 206). The strip
does not mark where the retirements happen, so this is evidence of absence rather than of a seen soft retirement; an
annotated crop of one retirement would make it stronger. Optional.

### Jets not burning: **a real defect; recommend option 1**

[`stress_jets_400.png`](stress_jets_400.png), frame 0 (4x): the jet is a thin, straight, saturated orange line from
the wheeler to the far end, with no fire around it, and it is gone two frames later, leaving only the grey band. In
a real battle at the default camera, without any stream, `flame_line` therefore reads as a **laser beam**. That
breaks the project's art rule (no beams) and makes the accepted lab look of `flame_line` irrelevant in play. It is
not a crowd edge case: 204 live effects were enough.

The cause is clear from Amendment 6 and the catalog: the `Major` cap counts instances, and a flame line holds its
slot for 3.5 s for a band that has no Detail layers left. The catalog's own budget comment says that over the cap
"the Detail layers of the **oldest** Major effects retire"; but the oldest are already only bands, so retiring their
Detail frees nothing and the newest jet loses its fire at birth. The rule does the opposite of its intent.

- **Option 1 (recommended): count an instance against its class cap only while it has live Detail layers.** A band
  is then a decal, budgeted by the decal pool where it belongs. This matches the stated intent, costs nothing in a
  fight without lingering marks, and keeps the cap meaningful.
- **Option 2 (raise the `Major` cap): not recommended alone.** It moves the cliff, not removes it; a fight with more
  wheelers or longer bands reaches the new number and the beam returns.
- **Also do:** when the cap is still reached, retire Detail of the oldest live-fire instance, never the newborn's
  (the newest event is the most important information on screen). And give the jet's Critical layers a look that
  still reads as fire when everything else is cut: a wider, torn, noisy core rather than an even rod, and a short
  ground-fire strip along the band, so that the degraded jet is a poorer flame and not a beam. That also covers the
  75-unit frame-budget camera, where the fire cards fall below the 6-pixel floor.
- Re-take [`stress_jets_400.png`](stress_jets_400.png) after the change and show billowing fire on at least the
  jets born in those ten frames.

---

## Factual checks against `slice_review.md`

Read after the verdicts above. The 7→8 pop figures (2 638 → 791 full-frame) agree in shape with my independent crop
(370 → 39). The decal counts per frame, the 6 144 pool, the forced-cap description (Detail first, then Critical
dropped, a different set each frame) and the jets cause agree with what I see. Its statement for the forced-cap
take, "stains and bands flicker", is right but incomplete: stains also lose their fill and show as hollow rings. Its
claim that the round fronts draw exactly as before rests on GPU tests I did not run; the goldens they compare
against are unchanged (AE 0).

## Summary

| Item | Verdict | Top change |
|---|---|---|
| `cleave_impact` | **accept** | optional: the front in the dust's light value on dark ground; a second noise octave on its edge; more opacity at default zoom |
| Footprint rule for `edge_noise` fronts | **accept with a change** | clause (b) per 45° sector, not global; bite capped at 20 % of the reach |
| Decal pool 6 144 | **accept** (30 % headroom) | |
| Behaviour at the decal cap | **not acceptable as designed** | sticky slots, whole effects only, retire oldest by erosion, pressure before the cap, a test |
| Degradation frames | **pass** | optional: annotate one retirement |
| Jets not burning | **defect** | option 1 (count against the cap only while Detail lives); never retire the newborn; a degraded jet must still read as fire, not a beam |

| Round-2 point | Status |
|---|---|
| cleave: glass bubble / pop at 7→8 / same circle per hit | fixed / fixed (39 px vs 370 on one crop) / fixed |
| cleave: crack readable, clods square | open (optional) |
| crowd: decal headroom | fixed (30 %); at-cap behaviour needs a stable design |
| crowd: degradation frames | delivered; pass |
| crowd: frame with jets burning | delivered; shows the jets do not burn in a battle (defect) |

| Shipped effect | Regression | Evidence |
|---|---|---|
| `flame_line` | no | sheet, sequence, 6 goldens AE 0 vs `0988f84` |
| `lance_sweep` | no | same |
| `shell_impact` | no | same |
| `acid_burst` | no | same |
| `spine_line` | no | same |
| `flak_burst` | no | same |
