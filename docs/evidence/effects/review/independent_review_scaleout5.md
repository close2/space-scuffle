# Independent Review of the Effect Scale-Out, Round 5 (WP7)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3 and
Amendment 8. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "Scale-out round 5
(WP7)". Previous rounds: [`independent_review_scaleout4.md`](independent_review_scaleout4.md) and
[`independent_review_scaleout3.md`](independent_review_scaleout3.md). Checklist: plan §7.2, as quoted at the end of
the packet. This is the owner's last review round (Q0080).

## Method

I worked from the packet, reviews 3 and 4, Amendment 8, the images, and the new code: `client::effects::front` with
its four tests, the `sprite.wgsl` and `common.wgsl` diffs (`front_crest`, `front_texel`), the runtime's card seed,
the catalog's `edge_noise`, and the footprint test helpers in `effect_slice_gpu_tests.rs`. I formed every verdict
before I read the implementer's round-5 notes in `slice_review.md`. I read them afterwards only to check factual
claims (see the section near the end). I built and ran nothing in the workspace; the measurements below are
ImageMagick plus small stand-alone scripts on the committed PNGs.

I cropped cells and frames and enlarged them 4x to 8x with nearest-neighbour scaling. Measurements:

- **Outline spread:** for the cleave front, a mask of the pixels that differ from the same row's clean last column
  (gray difference, blur 0.7 px, threshold 8/255, isolated pixels removed), normalised to its bounding ellipse; the
  outermost lit pixel in each of 72 angular bins. I report the standard deviation of the outer radius over the
  rays on the front (r > 0.8) as a share of the radius. For the default-zoom rows (which the sheet shows at half
  size) I upsampled 3x before masking. Rounds 3 and 4 are `git show 16f8c44:` and `git show 8c4a6ee:`, measured with
  the same script, so the three rounds are comparable to each other (my absolute figures differ from review 4's by
  up to about a point because the mask details differ).
- **Regularity (the flagged risk):** on the same 72-ray profiles, (a) the share and rank of the 12th angular
  harmonic among harmonics 2–36, which a cog of 12 teeth would make dominant, and (b) a count of **narrow teeth**:
  rays at least 0.06 of the radius farther out than the rays two bins away on both sides.
- **Pops:** the pixels that change by more than 3 % between consecutive sequence frames, on review 3's 70×50 crop,
  for rounds 3, 4 and 5.
- **Per-hit identity:** the repeat take's sequence frames 2/17/32/47, 3/18/33/48 and 4/19/34/49 (the same age of each
  of the four hits), upsampled 3x, masked against the clean frame 74, all four profiles normalised to the first
  hit's ellipse; the pairwise Pearson correlation of the 72-ray profiles.
- **Crest placement:** a Python mirror of `crest_hash`, `crest_centres` and the runtime's seed chain, over 4 096
  consecutive hits, with the cleave's real salt.
- **Identity:** git blob identity of every non-cleave sheet, sequence and GPU golden against `f3899be`, and
  `compare -metric AE` on the cleave's goldens.

Rows 1–8 are as in earlier rounds: rows 1 and 3 are bright ground at default and close zoom, rows 5 and 7 are dark
teal ground at default and close zoom, and the even rows are their references. Sequence frames are numbered from 0
and are 40 ms apart.

**Verdicts in brief:**

- **`cleave_impact` is accepted**, with notes. The torn edge is back and a little deeper than round 3's, every hit
  has its own outline, and the per-sector guarantee now holds by construction rather than by a tuned seed. At the
  default zoom the twelve crests do not read as a cog. At close zoom on teal, a few crests stand as narrow, sharp
  teeth; that is a polish note, not a blocker.
- **Wording corrections: adequate.**
- **No regression** in the other six effects: 48 files byte-identical to round 4.

---

## 1. `cleave_impact` (heavy blow; Ultralisk analogue): **accept** (with notes)

### What the construction does, and whether Rust and shader agree

`front_crest` in `sprite.wgsl` and `crest_centres` in `client::effects::front` compute the same thing:

- rotation `unit(crest_hash(seed ^ 0x9E3779B9))`; per-crest jitter
  `(2 · unit(crest_hash(seed + index · 0x85EBCA6B)) − 1) · 7°`; centre `(index + rotation) / 12 + jitter`, wrapped to
  one turn (`rem_euclid` in Rust, `fract(… + 1.0)` in WGSL, equal because the jitter is above −1 turn);
- the same PCG output permutation, in wrapping `u32` arithmetic on both sides (the WGSL operands are run-time
  values, so they wrap rather than fail as const-expressions would); the same 24-bit `unit`;
- the constants agree: 7° = 0.019444444 turn, 2.5° = 0.0069444444, 1.5° = 0.0041666667.

The shader's `angle` is `atan2(y, x) / TAU + 0.5` in the card's own plane. The card's random roll and mirror only
rotate or reflect the crest set, which changes no gap. The runtime passes the ring card's own seed (`front_seed:
seed`, `hash3(instance.seed, layer_index · 4 + placement, index)`); the ring is layer 3 of the cleave, so salt 12,
as the tests assume. The GPU footprint test casts its 72 rays at even **ground** angles (`reach_pixels`), the
same frame as the crests, so the unit test's rays are the GPU test's rays.

The edge formula is `leading = 1 − width · front_edge · (1 − max(smoothstep(0, 1, bite), crest))`. `FRONT_CREST` is
back at 1.0 (round 3's bite curve), the two-octave mix is re-contrasted by 1.355 = 1/√(0.65² + 0.35²), and
`edge_noise` is 0.66, so the declared bite is 0.198 of the radius, under the 0.2 cap. On a crest the edge is on the
radius whatever the noise. All of this matches Amendment 8.

### Do the 64-seed tests prove what they claim?

- **Largest gap below 45°: yes, and it is a theorem, not a sample.** With 30° spacing and ±7° jitter, no gap can
  exceed 44°. The test only confirms the code does what the arithmetic says. My mirror over 4 096 hits gives a
  largest gap of 43.97°.
- **Every footprint sector has a ray on a crest: yes, geometrically.** A sector's nine rays span 40°; any 45° window
  (the sector widened by the 2.5° half width on each side) must contain a crest centre, and its nearest sector ray
  is within 2.5°. Rotation-invariant, so the card's roll does not matter.
- **Two hits place their crests differently: yes, but the bound is weak.** "At least 1° mean crest distance per
  pair" is close to vacuous (a crest set with a 1° offset is the same pattern to the eye). The real per-hit variety
  comes from the per-hit noise phase between the crests, which this test does not cover; the capture correlation
  below does. My mirror: minimum 1.95°, average 7.78° over 4 095 consecutive pairs.
- **Two inaccuracies (non-blocking):**
  1. The tests' helper says it draws the seeds "as the runtime draws them", but it uses the effect salt
     `0x00C1_EA7E`. The cleave has no pinned `seed`, so its real salt is the FNV-1a hash of its name, `0x2A1CEEB8`.
     Because the gap property holds for every seed, this changes nothing proven; with the real salt my mirror gives
     the same figures (above). The helper should take the salt from the resolved catalog, or its comment should
     say "stand-in seeds" (as `slice_review.md` already does).
  2. The tests prove where the **edge geometry** sits, not that dust is **visible** there. The front's breakup
     (`amount: 0.8`) and grain still multiply the coverage, so a crest can carry little dust. Only the GPU footprint
     test, on one seed per driver, checks visible pixels. That is acceptable for shipping; a footprint run over
     two or three more seeds would close it.

### Measurements

| Measurement | Round 3 | Round 4 | Round 5 |
|---|---|---|---|
| Outer-radius s.d., row 3 (bright, close), 0.076 s / 0.142 s | 3.7 % / 4.0 % | 2.2 % / 2.2 % | **5.0 % / 4.6 %** |
| Mean step between neighbouring rays, row 3, same ages | 4.1 % / 3.0 % | 1.5 % / 1.6 % | **3.8 % / 3.7 %** |
| Outer-radius s.d., row 1 (bright, default), 0.076 s / 0.142 s | 3.3 % / 3.4 % | 2.5 % / 2.4 % | **3.9 % / 4.2 %** |
| Outer-radius s.d., row 5 (teal, default), 0.076 s / 0.142 s | 4.2 % / 4.0 % | 3.3 % / 3.6 % | **4.8 % / 4.8 %** |
| Rays on the front, row 5, 0.076 s / 0.142 s (the rest are gaps) | 67 / 66 | 72 / 71 | **66 / 67** |
| 12th harmonic rank (of 35), row 1 / row 5, 0.076 s | 15 / 15 | 28 / 18 | **6 / 6** |
| 12th harmonic rank, row 3 / row 7, 0.076 s | 32 / 5 | 32 / 9 | **23 / 5** |
| 12th harmonic share, row 7, 0.076 s / 0.142 s | 5.9 % / 7.3 % | 3.6 % / 3.6 % | **6.1 % / 5.8 %** |
| Narrow teeth, row 1 / row 5, 0.076 s | 0 / 0 | 0 / 0 | **1 / 1** |
| Narrow teeth, row 3 / row 7, 0.076 s | 1 / 1 | 0 / 1 | **3 / 8** |
| Repeat take, mean pairwise outline correlation, frames 2 / 3 / 4 after each hit | 0.38 / 0.23 / 0.12 | 0.49 / 0.36 / 0.27 | **0.12 / 0.10 / 0.14** |
| Repeat take, highest pair | 0.70 | 0.67 | **0.49** (hits 3 and 4) |
| Pops on my crop, steps 4→5 … 12→13 | 83, 55, 43, 39, 54, 62, 100, 116, 66 | 87, 50, 46, 48, 62, 62, 100, 116, 66 | **84, 52, 42, 35, 49, 59, 100, 116, 66** |

The spread is back above round 3 at every zoom, the gaps on teal are back, and the four hits are less alike than in
any earlier round. The 12th harmonic rises at the default zoom (rank 6), so the crests do add a weak 12-fold
component, but it is not dominant anywhere, and on close teal its share equals round 3's, which had no crests.

### Checklist

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | Bounding boxes span the reach as in rounds 3 and 4 (row 3, 0.142 s: 55–145 × 34–110 in my crop, against round 4's 55–144 × 34–111), nothing beyond it. The outermost dust touches the reach in every sector by construction, and the outline is carried by separate clumps rather than a line. The flash is the single focal point at frames 0–1. On bright ground at the default zoom the front is still faint (sequence frames 2–5, 4x), as noted in round 3. |
| 2 | Layered | pass | Unchanged: flash and light, sparks, torn front, central dust, puffs, clods. |
| 3 | Timing | pass | No pop: my crop gives the same shape as rounds 3 and 4 (table), and the implementer's full-frame row (716–1 034 px from frame 4 on) agrees. The front thins and breaks over frames 5–8 and is gone at 9; every row is clean at 0.712 s. |
| 4 | No tells of cheapness | pass (with notes) | **(a) Not a ring.** Rows 1 and 5 at 0.076–0.142 s (5x and at native size): the front is a broken, uneven band with a clear notch at the upper left on bright ground and gaps on teal, as in round 3, not round 4's continuous pencil ellipse. **(b) Not the same outline.** Repeat take frames 3/18/33/48 (6x): hit 2 has a wide gap on its right, hit 3 a broken right side, hit 4 a ragged upper edge; round 4's four near-identical ellipses are gone. **(c) The flagged risk, cog or regular spikes:** at the default zoom, no. At native size each cell shows at most one narrow tooth, and the eye reads a ragged dust edge with a few small nubs at irregular places, because only those crests where the noise next to them happens to be low stand out at all; elsewhere they merge into noise crests. At **close zoom on teal** (row 7, 0.076 s, 7x) about eight crests stand proud as narrow, steep-sided teeth, 4–5 px wide and up to about 9 px tall. They are irregularly spaced (25–90°) and of different heights, so they read as a thorny, serrated edge rather than a gear, but they are the effect's one remaining visibly procedural detail, and it lasts about two frames. The cause is clear from the shader: a 5° flat top with only a 1.5° soft edge next to a bite of 0.198 of the radius makes a flank of about 9 px over 1.3 px. **(d) Still open from round 3:** the front's dark value on teal (a dark beaded outline around light dust), square clods, no readable crack. None of these is new. |
| 5 | Value and colour | pass (note) | As in rounds 3 and 4; note 4(d). |
| 6 | Crowd | pass | Too small and brief to pick out in the crowd; a hit's last dust overlaps the next flash for two or three frames, invisible under the flash (repeat sequence frames 13–17). |
| 7 | Fog | pass | Nothing beyond the reach on any row or age. |

**Verdict: accept.** Round 4's two failures (a ring again; one outline for every hit) are fixed, the footprint
rule is met by construction for every seed, and the new risk does not show at gameplay zoom.

**Optional polish (in order of value):**

1. **Soften the crest flanks:** widen `CREST_SOFT` from 1.5° to about 5–6° (keep the 2.5° flat half width, so the
   guarantee is untouched), so that a crest rises as a rounded shoulder rather than a spike. Optionally scale each
   crest's flat width per crest (2.5–4°) from the same hash. Gaps up to 44° minus the wider shoulders still leave
   room for deep bites.
2. Draw the front in the dust's light value at lower opacity on dark ground (round 3 and 4 note).
3. Take the test seeds' salt from the resolved catalog (or call them stand-in seeds); run the GPU footprint check on
   two or three more instance seeds, so visible dust on the crests is tested, not only their geometry.
4. As before: irregular clods, a readable crack at close zoom.

---

## 2. Wording corrections: **adequate**

- **Refused effects.** The round-4 row of the packet now says that no decal an effect was granted is dropped from a
  frame and that the 444 refused effects drew none of their marks, Critical ones included (whole set or none);
  `stress_capture.txt` says the same. That is exactly the correction I asked for. The "(round 3: about 2 050 Critical
  decals a frame)" comparison is now correctly qualified.
- **Reservation threshold.** Amendment 8 point 2 records that the 85 % mark is measured in reserved slots, that a
  reservation includes decals too small to draw at the current camera, why counting only drawable decals would make
  the ledger camera-dependent, and that the mark is reached at about 3 900 drawn decals at the frame-budget camera.
  That is what review 4 asked to record. The camera-independent refinement (excluding at catalog load the layers
  that can never reach the floor) is not recorded; it was optional and of small value, so this is not a gap.
- One nuance worth keeping in mind, not a correction: "no granted decal is dropped from a frame" is true of cuts;
  granted lasting marks do still leave early, by a 0.25 s erosion, under pressure (660 in the forced-cap take, 853
  at the normal stress load). The packet states those figures next to it, so nothing is hidden.

---

## 3. Regression check: **no regression**

- `git diff --stat f3899be 9200439` changes, among images and goldens, only the cleave's sheet, sequence, repeat take
  and its goldens `_0` and `_1` on both drivers.
- All 48 non-cleave files are **byte-identical** to `f3899be` (same git blob): the contact sheet, the sequence and the
  six goldens (`crates/client/tests/data/effects/{lavapipe,radv}/<effect>_{0,1,2}.png`) of each of `flame_line`,
  `lance_sweep`, `shell_impact`, `acid_burst`, `spine_line` and `flak_burst`. AE is therefore 0. Those goldens are
  what the GPU tests compare against, so they also cover the new `front_seed` field in the card record (it replaced
  padding at offset 148; the layout test pins it), which round fronts never read.
- The cleave's goldens `_0` and `_1` change by 3 051–3 204 pixels on both drivers; `_2` (after the front) is
  unchanged (AE 0).
- The crowd, jets, fog and decal-cap takes are unchanged since round 4; only the wording of `stress_capture.txt`
  changed.

| Shipped effect | Regression | Evidence |
|---|---|---|
| `flame_line` | no | sheet, sequence, 6 goldens identical to `f3899be` |
| `lance_sweep` | no | same |
| `shell_impact` | no | same |
| `acid_burst` | no | same |
| `spine_line` | no | same |
| `flak_burst` | no | same |

## Factual checks against `slice_review.md`

I read it after forming the verdicts above.

- **Confirmed:**
  - the direction and size of the spread change (round 4 smoothest, round 5 above round 3 on both grounds); its
    absolute figures differ from mine by up to a point, as it says of its own re-measurement;
  - the outline correlation ordering (round 5 < round 3 < round 4) and the "max 0.36 / 0.44 / 0.47" shape; my means
    are higher (0.10–0.14 for round 5) because I normalise all four hits to one ellipse, but the ranking is the same;
  - the pops (716–1 034 px from frame 4 on), which agree in shape with my crop;
  - the crest construction, the 44° bound and the mean crest distance of about 7.5° (my mirror: 7.78°, minimum
    1.95°; its "minimum 2.9°" was over fewer, stand-in seeds);
  - the golden changes (cleave only) and the wording corrections.
- **Qualified:**
  - "rays on the front 72 / 71" on teal: with my threshold the teal front has gaps (66–67 of 72 rays at the default
    zoom), as round 3 did. That is the desired look, not a defect, but the table understates how torn the teal
    front is.
  - The notes do not mention the narrow teeth at close zoom on teal (section 1, note 4(c)).
  - The unit tests' seeds are stand-ins (salt `0x00C1_EA7E`), not "as the runtime draws them" for this effect; the
    notes' "stand-in seeds" is the accurate phrase.

## Summary

| Item | Verdict | Top change |
|---|---|---|
| `cleave_impact` | **accept** | optional: soften the crest flanks (soft edge about 5–6°), so close-zoom crests are shoulders, not teeth |
| Wording: reservation threshold | **adequate** | none |
| Wording: refused effects | **adequate** | none |
| Regressions in the other six | **none** | none |

**Ready to ship (my view, after five scale-out rounds):** all seven effects.

| Effect | Ship | Accepted in |
|---|---|---|
| `shell_impact` | yes | slice rounds; unchanged since |
| `acid_burst` | yes | slice round 5; polish in scale-out round 1 |
| `spine_line` | yes | slice round 4; polish in scale-out round 1 |
| `flak_burst` | yes | scale-out round 1 |
| `flame_line` | yes | scale-out round 2; jets in battle accepted in round 4 |
| `lance_sweep` | yes | scale-out round 2 |
| `cleave_impact` | yes | scale-out round 5 (this review) |

**Remaining non-blocking polish, across rounds** (to be recorded as open; none blocks shipping):

| Effect or area | Item | First raised |
|---|---|---|
| `cleave_impact` | sharp narrow crest teeth at close zoom on teal (soften flanks) | scale-out 5 |
| `cleave_impact` | front's dark value on teal reads as a beaded outline around light dust | scale-out 3 |
| `cleave_impact` | front faint at the default zoom on bright ground | scale-out 3 |
| `cleave_impact` | square clods; crack not readable | scale-out 1 |
| `cleave_impact` | test seeds from the real salt; GPU footprint over more than one seed | scale-out 5 |
| `flame_line` | screen-size floor for the burning ground's width at the far camera (recorded in Amendment 8) | scale-out 4 |
| `flame_line` | flare and break the jet rod in frames 1–4; oxblood pockets in the billows; a blunt cut at the fog edge at 0.104 s | scale-out 2 |
| `flame_line` (crowd) | most `Major` effects give up their billows early in a big fight; revisit only if play-testing shows jets thinning too soon | scale-out 4 |
| `lance_sweep` | spread the groove's width variation instead of a wedge; larger spatter; airborne part weak at the default zoom | scale-out 2 |
| `flak_burst` | regular scalloped inner edge (about ten even lobes) with a slight glass-rim read at close zoom | scale-out 1 |
| `shell_impact` | irregular streak fringe for the blast mark (new texture); more ring width variation | slice round 4 |
| `acid_burst` | meniscus, inner rims, glob shapes, crowd variation, dried crust; stains dominate the crowd frame (Amendment 6's open proposal) | slice rounds; scale-out 2–4 |
| `spine_line` | torn clods; clods a step lighter than the furrows | slice round 5 |
| Decal ledger | optional camera-independent refinement: leave out of reservations the layers that can never reach the 6-pixel floor | scale-out 4 |
| Crowd evidence | an annotated crop of one soft retirement would strengthen the degradation take | scale-out 3 |
