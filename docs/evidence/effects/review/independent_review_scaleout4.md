# Independent Review of the Effect Scale-Out, Round 4 (WP7)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.3 and
Amendment 7. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "Scale-out round 4
(WP7)". Previous round: [`independent_review_scaleout3.md`](independent_review_scaleout3.md). Checklist: plan §7.2,
as quoted at the end of the packet.

## Method

I worked from the packet, the round-3 review, Amendment 7, the images, and the new code: `client::effects::ledger`,
the runtime's spawn, ledger-settling and retirement paths, `torn_front_holds` with its negative test, the
decal-cap runtime test, and the `effects.ron` and `sprite.wgsl` diffs. I formed every verdict before I read the
implementer's round-4 notes in `slice_review.md`. I read them afterwards only to check factual claims (see the
section near the end). I built and ran nothing.

I cropped cells and frames and enlarged them 4x to 8x with nearest-neighbour scaling (ImageMagick). I took six
kinds of measurement:

- **Outline roundness:** for the cleave front, I made a mask of the pixels that differ from the same row's clean
  last column (gray difference, blur 0.7 px, threshold 8/255, isolated pixels removed). I normalised it to its
  bounding ellipse and took the outermost lit pixel in each of 72 angular bins. From those I report the standard
  deviation of the outer radius over the rays on the front (r > 0.8), as a share of the radius. Round 3 is
  `git show 16f8c44:`, measured the same way.
- **Pops:** the pixels that change by more than 3 % between consecutive sequence frames, on the same 70×50 crop as
  round 3. I ran it on both rounds.
- **Per-hit identity:** frames 3, 18, 33 and 48 of the repeat take (the same age of each of the four hits), for both
  rounds.
- **Crowd stability:** one 150×110 region tracked across the eight frames of the forced-cap take and of the
  degradation take.
- **Jets:** ten frames of the central jet at the default camera, and the far-camera detail enlarged 8x in total.
- **Identity:** `compare -metric AE` of every non-cleave sheet, sequence and GPU golden against `16f8c44`.

Rows 1–8 are as in earlier rounds: rows 1 and 3 are bright ground at default and close zoom, rows 5 and 7 are dark
ground at default and close zoom, and the even rows are their references. Sequence frames are numbered from 0 and
are 40 ms apart.

**Verdicts in brief:**

- **`cleave_impact` is rejected.** The front passes the new footprint rule because it was made round. At the default
  zoom it reads as a thin wobbly ring, and the four hits of the repeat take now share nearly the same outline. That
  undoes round 3's main gain. The rule is right. The way it was met is wrong. A by-construction fix is given below.
- **Footprint rule: acceptable.** The test and its negative test are sound.
- **Decal cap: accepted.** Marks are whole and stable at the forced cap. The early erosion at normal load is
  acceptable; I recommend no pool change and no drawable-only counting.
- **Jets: accepted.** They burn as fire at the default camera. At the far camera they are short, torn orange strips,
  not beams, though the margin is small. Making the body of fire and the burning ground `Critical` is sound.
- **No regression** in the other six effects: 48 files, AE 0.

---

## 1. `cleave_impact` (heavy blow; Ultralisk analogue): **reject**

The change: the bite now mixes a second octave in at 0.35 (`FRONT_SECOND_OCTAVE`), and the edge sits on the radius
over the top 30 % of the contrasted noise (`FRONT_CREST` 0.7, from 1.0). Both changes push the same way. Mixing two
independent noise reads lowers their variance (by about a quarter at these weights), and the lower crest puts more
of the circle on the radius. Together they remove most of the bites, not just the one short sector.

| Measurement | Round 3 | Round 4 |
|---|---|---|
| Outer-radius s.d., row 3 (bright, close), 0.076 s / 0.142 s | 3.5 % / 3.9 % | **2.1 % / 2.3 %** |
| Mean step between neighbouring rays, row 3, same ages | 2.6 % / 2.8 % | **1.6 % / 1.6 %** |
| Rays on the front, row 3, 0.076 s (others fall back to the central dust: gaps) | 71 / 72 | **72 / 72** |
| Outer-radius s.d., row 7 (teal, close), 0.076 s / 0.142 s | 3.9 % / 4.1 % | 3.1 % / 3.7 % |
| Implementer's mean ray error (radv) | −5.4 px | −2.66 px |

The front's outer silhouette is about 40 % smoother on bright ground and has no gap left. The mean bite is halved.

| # | Item | Verdict | What I see |
|---|---|---|---|
| 1 | Reads at gameplay zoom | pass | The reach is shown even more clearly than before: the outermost dust touches the reach all the way round (per-sector rule passes). At 0.076 s, row 3, the mask spans 120–199 × 57–126 against round 3's 121–198 × 59–125. |
| 2 | Layered | pass | Unchanged: flash and light, sparks, front, central dust, puffs, clods. |
| 3 | Timing | pass | No pop. On my crop the steps from frame 4 on are 87, 50, 46, **48**, 62, 62, 100, 116, 66 px (round 3: 83, 55, 43, 39, 54, 62, 100, 116, 66). The implementer's full-frame row (775–1 048 px) has the same shape. |
| 4 | No tells of cheapness | **fail** | **(a) A ring again at the default zoom.** Rows 1 and 5 at 0.076–0.142 s (5x): on teal the front is a thin, continuous, dark ellipse around the lighter central dust, like a hand-drawn circle or a selection ring. Round 3 had visible notches at its upper left and lower left. On bright ground it is a near-continuous oval line whose thickness varies a little. Sequence frames 2–4 (6x) read the same way: a wavy pencil ring, which is the round-1 finding. Close zoom (rows 3 and 7) still shows a soft, uneven band with some ragged clumps on its lower left, so up close it is better than a ring. But the default zoom is the zoom that counts. **(b) The same circle on every hit, again.** Repeat take frames 3/18/33/48: round 3's four outlines had flats and notches in different places. Round 4's are four nearly identical ellipses, and only the interior dust differs. Round 2's "same circle on every hit" finding, fixed in round 3, has largely returned. Notes from round 3 still open: the dark front on teal (now more salient because the line is continuous), square clods, no readable crack. |
| 5 | Value and colour | pass (note) | As in round 3. The dark-on-teal outline is the note above. |
| 6 | Crowd | pass | Too small and brief to pick out in the crowd, as before. |
| 7 | Fog | pass | Nothing beyond the reach. |

**Why the fix went wrong.** Round 3's front failed one sector, by 3.6 px, for one noise phase. The response turned
two global knobs until the worst sector of that seed passed. By `slice_review.md`'s own table, `FRONT_CREST` 1.0 with
the second octave still failed (−2.63 px), as did 0.8. So the look of every hit was rounded to rescue one sector of
one hit. The per-sector clause asks for one clump on the reach in every 45°. It does not ask for a round front.

**What I would accept (round 5).** Make the per-sector guarantee hold by construction, and restore round 3's bite
depth:

1. Go back to round 3's bite: `FRONT_CREST` 1.0. Keep the second octave for irregular spacing, but add it in
   without lowering the variance: for example, re-contrast after the mix, or raise `FRONT_WAVER_CONTRAST` for the
   bite read.
2. Add a set of **guaranteed crests**: 12 narrow arcs (about 8–10° wide) where the bite is forced to 0 (edge on the
   radius). Space them 30° apart, with one per-instance rotation and a per-crest jitter of up to ±7°. The largest gap
   between crests is then under 45°, so every 45° test sector contains one, whatever the instance's noise phase.
   Between the crests the noise bites as deeply as in round 3, and the rotation and jitter keep each hit's outline
   its own.
3. Add a capture assertion that two hits of the repeat take differ in outline. For example, the correlation of their
   72-ray outer-radius profiles stays below a set bound. This keeps a future footprint fix from silently rounding
   the front again.

As an optional extra, lighten the front's value only on dark ground, by drawing it in the dust's ramp at a lower
opacity rather than in a darker value. The implementer's concern about contrast on bright ground is real, but on teal
the dark continuous line is now the effect's most visible tell.

---

## 2. Footprint rule: **acceptable**

- `torn_front_holds` splits the 72 rays into 8 chunks of 9 rays. In each chunk the largest finite error must be at
  least −2 px, and a chunk with nothing lit fails (its fold stays −∞). That is the clause I asked for.
- `a_torn_front_must_reach_the_reach_in_every_sector` covers the two defect classes I named: a front squashed on one
  side (rays 36–71 at −5 px) and a missing quarter (rays 18–35 unlit). Both fail as they should, and a torn front
  with clumps every third ray passes.
- `MAX_FRONT_BITE` (0.2) is enforced in the catalog resolver.
- **Limit worth knowing:** the negative test exercises the helper on synthetic rays, not the shader. Whether a
  sector passes depends on each instance's noise phase. That is why section 1 asks for a by-construction guarantee
  rather than tuning to the test's seed.

---

## 3. Decal cap

### Forced-cap take (pool 2 048): **accept**

I tracked the upper-middle region over the eight frames of [`stress_decal_cap_400.png`](stress_decal_cap_400.png)
and compared it with the same frames of [`stress_degradation_400.png`](stress_degradation_400.png):

- The large stains are present as full discs in every frame. None appears in one frame and vanishes in the next, and
  none draws as a hollow ring.
- One stain on the lower edge fades over frames 1–3. That is an erosion, not a cut.
- The green arc that appears at frames 3–4 is a live acid burst's ring. It is not a decal, and the normal take shows
  it too.
- The visible difference from the normal take is that the cap take has fewer grey scorch bands: whole effects were
  refused their marks, which is the designed behaviour.

Round 3's two failures (flicker and hollow rings) are gone.

The code matches the design:

- `BudgetLedger::grant` is all-or-nothing against `marks_reserved + need <= capacity`.
- `settle_ledger` recounts the reservations each frame from the live layers.
- `marks_fade` draws a decal layer only while its effect holds its slots, and fades it with `mark_fade` once those
  are retired.
- The ledger allocates nothing.

`lasting_marks_at_the_decal_cap_never_flicker_and_draw_whole` checks the right invariant: owners are identified by
slot and birth, every owner draws its marks whole, and marks vanish only when retired or ended. Two weaknesses in
it:

- It accepts a retired mark vanishing at once. The fade itself is covered only by the ledger's unit test.
- It runs with no camera, so the size floor is not exercised.

Both are acceptable.

**One factual correction.** "No Critical decal is dropped" is true only of frame writes (`critical_overflow`). The
444 effects refused room drew none of their decals, Critical ones included: for example, an acid stain or a blast
mark. Whole-or-none is the right rule, but the packet should say "444 effects drew no marks, Critical included",
not imply that every Critical mark was kept.

### Early erosion at normal load: **acceptable; keep counting every decal; do not raise the pool**

At 200 splashes a second (7 times the decal load of the real battle: 1 507 decals in the jets take), 853 of the
oldest lasting marks erode out early over the run. That is about 36 per eight frames, spread over the screen and
each faded over 0.25 s. I could not see them in the degradation take. The real battle stays far below the 5 222-slot
pressure mark, so it erodes nothing early.

- **Count only drawable decals: unsound.** Whether a decal clears the 6-pixel floor depends on the camera. If
  reservations followed the camera, a zoom-in would make reserved-nothing decals drawable at once. They would then
  either stay invisible (a camera-dependent hole in the marks: stains appear on zoom-out and vanish on zoom-in) or
  claim slots on the spot and force a burst of erosions triggered by the camera. Either way, the frame-to-frame
  stability the ledger exists to give would depend on the camera again. The pool bounds the worst case, a close
  camera where every reserved decal is drawable, so it must count every decal.
- **Raise the pool: not needed.** It would only shift where a load far beyond any real battle starts shortening old
  marks. If the owner wants more room, the case should be a close-camera frame-time measurement, not this figure.
- **Accept as is.** Record in the plan that the 85 % mark is measured in reservations, and that at the frame-budget
  camera this is about 3 900 drawn decals. A camera-independent refinement is possible later: leave out of the
  reservation the layers that could never reach the floor even at the closest zoom, computed once at catalog load.
  It would gain little, because at the closest zoom almost every layer is drawable.

---

## 4. Jets: **accept**

- **Default camera** ([`stress_jets_400.png`](stress_jets_400.png), central jet, ten frames at 1.8x):
  - Frames 0–1: a tapered stream with a short yellow tip.
  - Frames 2–4: a torn, billowing orange body with soot along its whole length.
  - Frames 5–9: separate fire tufts and smoke over burning ground along the line.

  It reads as fire over the units in every frame. Round 3's thin laser line is gone. The frame-0 tip is a straight
  streak for one or two frames, which is acceptable for a jet's launch.
- **Far camera** ([`stress_jets_far_400_detail.png`](stress_jets_far_400_detail.png), 8x in total):
  - The jets are short strips, about 5–6 px wide and 40–60 px long on screen.
  - Their edges are ragged, their width is uneven, they break into clumps along one of them, and they carry no
    white-hot core.

  They read as streaks of fire rather than beams, but only just: at native size they are thin orange streaks.
  Optional: widen the burning ground slightly at this camera, for example with a screen-size floor on its decals'
  width, so the strip reads as burning ground and not as a tracer.
- **The `Critical` change is sound for the budget.**
  - The body of fire is one ribbon per jet. The burning ground is a few decals that last 0.6 s. Both are small and
    linear in the number of jets.
  - The stress run reports no Critical write without a slot.
  - Under the new class-cap rule, those layers are the only ones that survive stripping, which is exactly what must
    survive.
  - At the decal cap, the burning ground follows whole-or-none like any other mark. That is acceptable.
  - The lab look is unchanged: the `flame_line` sheet, sequence and goldens are AE 0.
- **Class-cap rule.**
  - The code counts an effect as burning while `now < fire_end` and its detail is not retired.
  - On a spawn over the cap it retires the oldest burning effect that still has detail (`retire_oldest_fire`), and
    marks the newborn over the cap only when no such effect exists.
  - The two runtime tests cover both halves.
  - **Note:** 1 718 of 1 842 effects in the stress run, and 483 in the plain battle, gave their detail up during
    their life. Each such retirement erodes over 0.1 s. I saw no pop among the ten jet frames, but that rate means
    most `Major` effects lose their billows early in a big fight. That is the intended trade-off. Revisit it only if
    play-testing shows jets thinning too soon.

---

## 5. Regression check: **no regression**

- `git diff --stat 16f8c44 8c4a6ee` changes only the cleave's goldens and images among the subject files.
- `compare -metric AE` against `16f8c44` gives **0** for all 48 non-cleave files: the contact sheet, the sequence and
  the six goldens (`crates/client/tests/data/effects/{lavapipe,radv}/<effect>_{0,1,2}.png`) of each of `flame_line`,
  `lance_sweep`, `shell_impact`, `acid_burst`, `spine_line` and `flak_burst`.
- The cleave's goldens `_0` and `_1` change by about 1 450–2 050 pixels (more than 1 %) on both drivers. `_2` is
  unchanged.
- In the crowd frame ([`stress_capture_400_detail.png`](stress_capture_400_detail.png)), the flame lines show as
  torn orange strips. The other effects look as in round 3, and acid stains still dominate the frame (Amendment 6's
  open proposal).

## Factual checks against `slice_review.md`

I read it after forming the verdicts above.

- **Confirmed:**
  - the crest table: 1.0 and 0.8 fail and 0.7 passes, which I used above;
  - the pops figures, whose shape agrees with my crop;
  - the decal counts per frame;
  - the 853 / 660 / 444 figures, which agree with `stress_capture.txt`;
  - the class-cap and `Critical` descriptions, which agree with the code.
- **Corrected:**
  - "0 Critical decals dropped" (see section 3).
  - Its 0.6 row says that value is "noticeably rounder". The taken 0.7 is already round enough to read as a ring at
    the default zoom, which the implementer's notes do not mention.

## Summary

| Item | Verdict | Top change |
|---|---|---|
| `cleave_impact` | **reject** | Restore round 3's bite depth (`FRONT_CREST` 1.0, second octave without loss of variance). Meet the per-sector clause by construction: 12 jittered guaranteed crests, at most 45° apart. Add a test that two hits differ in outline. |
| Footprint rule (per-sector, bite ≤ 20 %) | **acceptable** | None. Do not tune the look to one test seed. |
| Decal cap, forced-cap take | **accept** | Packet wording: refused effects drew no marks, Critical included. |
| Early erosion at normal load | **accept as is** | Keep counting every decal; no pool raise; record that the 85 % is measured in reservations. |
| Jets, default camera | **accept** | |
| Jets, far camera | **accept (narrow)** | Optional: a screen-size floor on the burning ground's width. |
| `flame_line` body and burning ground `Critical` | **sound** | |

| Round-3 point | Status |
|---|---|
| footprint clause per sector; bite capped | done |
| stable decals at the cap | done |
| jets burn in a battle | done |
| cleave: second noise octave | taken, but together with the crest change it rounded the front (rejected) |
| cleave: front's value on teal | not taken; now more visible |

| Shipped effect | Regression | Evidence |
|---|---|---|
| `flame_line` | no | sheet, sequence, 6 goldens AE 0 vs `16f8c44` |
| `lance_sweep` | no | same |
| `shell_impact` | no | same |
| `acid_burst` | no | same |
| `spine_line` | no | same |
| `flak_burst` | no | same |
