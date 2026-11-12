# Independent Review of WP8 (Concealment and Stance), Round 2

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendments
11 and 12. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP8 round 2". Previous
round: [`independent_review_wp8_1.md`](independent_review_wp8_1.md) (required changes B-1 to B-6, evidence F-1, F-2,
check C-1). Owner rules: Q0076, Q0080 (this is round 2 of at most five). The reviewer did not take part in building
this work or in round 1.

## Method

I read the packet's round-2 section, round 1's review, Amendments 11 and 12, Q0076, the round-2 diff of
`assets/catalog/effects.ron`, `assets/shaders/bubble_shadow.wgsl`, `assets/shaders/effects/sprite.wgsl`,
`distortion.wgsl`, `common.wgsl`, `effects/instances.rs` and `scene_controller.rs` (`b9d043c..9eeff33`), the checks in
`crates/client/tests/stance_gpu_tests.rs`, and Bevy 0.15.3's `ShadowBinKey` (`bevy_pbr/src/render/light.rs`). I did
not read the implementer's self-checklist. I built and ran nothing. The figures below come from ImageMagick crops
(nearest-neighbour, 2× to 8×), `compare -metric AE`, amplified difference images and colour samples of the committed
PNGs, kept in `~/scratch/wp8-review2/`. Battle coordinates are window pixels of the 1280×720 captures. Sequence frames
are named by their `t` label.

One fact about the evidence matters for several findings. The test's "bare" or "without effect layers" view
(`apply_effect_view`) clears only sprites, decals, distortions and lights. The **Shell** (sink, veil, rim, matte,
shadow lane) still applies in every "bare" capture. A bare-versus-full difference therefore shows only what the
ground and air layers add.

## 1. Fog and leaks (item 7)

**Verdict: accept.**

- **Undetected burrowed enemy (F-1).** At about (796, 211), a 120×110 patch of `wp8r2_battle_bare_*` is identical to
  `wp8r2_battle_before_*` (0 px at 1 % fuzz) on both drivers, and the tableau equals both of them. This agrees with the
  printed "0 pixels changed where it stands". F-1 is closed.
- **Undetected cloaked enemy, including its shadow.** A 160×130 patch round (893, 230), wide enough to hold the
  shadow's reach to the lower right, is identical in bare and before (0 px, both drivers). So the unit casts no
  shadow, and the new shadow pipeline did not bring one back: the hidden bubble is not drawn into the shadow maps
  either. Tableau against before: 1 076 px (radv) and 915 px (lavapipe), all inside one disc. The amplified
  difference holds only displaced tile seams. Measured: 0 foreign pixels and a mean shift of 0.40 / 0.38 levels in
  battle (0.19 / 0.07 in the pose), within 0.75.
- **Strengthened `shimmer_check` (`blend_of_neighbours`).** This is sound and much stronger than round 1's
  13×13 per-channel box. A bent pixel must now lie within 6 levels (Euclidean) of the per-channel box of *one* 2×2
  block within 6 px. A dark neutral spot on light, multi-coloured ground now fails unless one 2×2 block already
  holds a dark pixel in every channel. One minor overstatement: the doc comment says it "refuses a colour assembled
  from channels of different pixels". Within a single 2×2 block it still accepts such a colour, because it tests the
  box, not the convex hull of the four texels. For a 2×2 block this is harmless, but the comment should say "of
  pixels in different blocks". The loop `-6..6` is asymmetric (dx from −6 to 5), but each block spans `left..left+1`,
  so the reach is ±6 px as intended.

## 2. C-1: the detected enemy's milky body

**Closed: the look does not cause it, and it no longer shows.** In `wp8r2_battle_detected_radv.png` at (893, 230),
the detected enemy is crisp on dark violet, in the same way in `wp8r2_battle_detected_bare_radv.png`. The difference
between the two is at most 12 levels, and only 10 px exceed 8 levels. Round 1's `wp8_battle_detected_radv.png` at
(775, 220) showed the same stand-in with a milky centre. That was most likely an earlier capture moment, while the
cloak's veil was still eroding out after detection. The bare view keeps the Shell, so it could not have told a
Shell-caused wash apart on its own. That does not matter now that the full capture is crisp.

A side note on `detected_mark`: its addition to the frame is now nearly invisible (max 12 levels). The "marked"
read rests on the rim. That is acceptable, because round 1 accepted the look as a thin cold outline.

## 3. The shadow fix (B-2) and its engineering risk

### What it does and whether it is correct

`bubble_shadow.wgsl` is Bevy's prepass vertex stage plus the shell. It scales the caster about the **sphere's
centre** by `1 − shadow` and lowers it by the sink. `NoAutomaticBatching` is added to every bubble in
`spawn_bubble`.

- **The batching diagnosis is right.** In Bevy 0.15.3, `ShadowBinKey` is `{ pipeline, draw_function, asset_id }`
  (the mesh). There is no material bind group in it. All bubbles share one sphere, so a bin holds every bubble, and
  the bin draws with the representative entity's material. Without the change, every bubble would cast with one
  bubble's shell. With `NoAutomaticBatching`, `should_batch()` is false and each bubble becomes an unbatchable item
  with its own material bind group. This is correct.
- **The main pass is unaffected, as claimed.** Blended bubbles are sorted `Transparent3d` items. Each bubble has its
  own material (its own shell uniform), so consecutive items never shared a bind group and were never batched.
- **The shape of the fade is wrong (new defect S-1).** Shrinking about the centre, which lies about one radius above
  the ground, moves the shrinking shadow toward the *centre's* projection along the light. That point lies beside the
  unit, not under it. The shadow therefore pulls away from the body as it shrinks:
  - `cloak_own` holds `shadow: 0.7`. The caster stays at 30 % size, which leaves a **small hard dark-green dot
    detached from the unit**, about 12×8 px, below-right of the veiled bubble at about (703, 236) in
    `wp8r2_battle_undetected_radv.png` and `_lavapipe.png`. Round 1's capture (`wp8_battle_undetected_radv.png`) has
    no dot there. It reads as a pebble or a hole, not as a faint shadow. **This is a regression of the accepted
    `cloak_own`.**
  - In the dig (t = 560 to 1 000 ms) and the emerge (t = 280 to 400 ms), the shadow shrinks and grows over about
    0.4 s. That is not a pop, so B-2's "no pop" is met. However, at small sizes it is a separate dark spot off the
    body's lower right (emerge t = 280 ms), with a gap of lit floor between it and the bubble.

  Fix: scale about the caster's ground contact (`centre − radius·up`, or the shell's ground y) instead of its centre.
  The shadow then contracts toward the unit's foot and stays attached. For `cloak_own`, either go to `shadow: 1.0`
  (the transition still fades over 0.35 s, and round 1 accepted no shadow) or use the anchored scale so that 0.7
  gives a small shadow at the foot.

### Cost at 400 units

The battle light has three cascades (`terrain/theme_gpu.rs`, `num_cascades: 3`).

| | Before | After |
|---|---|---|
| Shadow pass | 1 draw per cascade for all bubbles sharing the sphere (≈ 3 draws) | 1 draw per bubble per cascade it falls in: up to ≈ 3 × 420 ≈ 1 260 draws, in practice probably 600 to 1 000 after cascade culling |
| Main pass (bubbles) | ≈ 1 per visible bubble (sorted, never batched) | unchanged |

Each unbatched shadow draw costs a bind-group set (material) plus a dynamic-offset mesh bind group and a draw on the
render thread, and adds one more phase item in queue and sort. A reasonable estimate is about 0.5 to 2 ms of extra
render-thread CPU at 400 units on this machine. With pipelined rendering, this hides behind the main update (p50
9.3 ms) only while the render thread has slack. The 400-unit p95 is already 19.1 ms, above the 16.7 ms line.

**Recommendation.** Do not ship the unconditional form without measurement. There is a cheaper and still correct
alternative: **add `NoAutomaticBatching` only to bubbles whose shell is non-neutral for the caster** (`shadow > 0` or
`sink > 0`), meaning units that are cloaked, burrowing, burrowed or emerging. Insert it no later than the frame the
lane leaves 0, and remove it once the lane is back at 0 and the look is released. The remaining batched bin then
holds only neutral bubbles, so any representative casts the right shape. The cost becomes proportional to concealed
units (a handful), and the extra draws for 400 ordinary units disappear. Per-instance shadow data would also work,
but Bevy 0.15 has no per-instance custom data in the standard mesh pipeline, so it would need a custom pipeline. That
is more code for no gain over the conditional component.

- If the implementer keeps the unconditional component, a `frame_budget` re-measurement at 400 units (same protocol
  as `docs/evidence/2026-11-12-frame-budget-two-armies.md`) is **required before merge**, accepting p50 and p95
  within run-to-run noise of 11.8 / 19.1 ms.
- If they adopt the conditional form, a full re-measurement is not required. One 400-unit run to confirm no change is
  advisable, but it does not block the merge.

## 4. The rejected looks (checklist items 1 to 7)

### `burrow_mound`: reject (narrowly)

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | **accept** for the owner's readability | `wp8r2_burrow_pose_radv.png` cell 3: the full burrowed clip (scorpion under soil) shows, with no glass highlight and no shadow. The marble is gone. My count on the committed pose, burrowed (bare) against empty inside the disc: 21 672 px, against about 22 170 px for the standing disc (radius ≈ 84 cell px), so about 98 %, above 85 %. The battle at (580, 388) reads the same at default zoom. |
| 2 Layered / ground | **reject** | The "dug in" read was meant to come from the ground: hollow, heaped earth, soil band. In practice the looks add almost nothing outside the disc. In pose cell 3 against cell 4, the differing area's bounding box is 180×183 against the body's 168×168: the looks reach only about 6 px beyond the rim, as a thin pale-lilac crescent under the bottom edge and a few dark specks. No brown earth and no hollow is visible round the unit. The battle figure agrees: "the burrowed looks change 662 pixels, farthest at 1.35 radii", against roughly 8 600 px of ground between 1.0 and 1.8 radii at that scale. The cause is the data: the `hollow` decal is 1.15 radii, and the seven `mound` heaps are centred *at* 1.0 radii with size 0.45, so with the body now nearly full-size the bubble covers most of them. The unit reads as a flat matte violet token or porthole lying on the floor with a scorpion in soil inside. It does not read as a marble, but not as dug in either. |
| 4 Cheapness | accept | The hard straight edge of round 1 is gone. No box edges round the burrowed unit. |
| 5 Colour | partial | Inside the disc the soil is the clip's own. The ground ramps are now warm brown, but they barely show. |
| 7 Fog | accept | §1. |

The no-clip unit's deep sink: `wp8r2_plain_burrow_pose_*` shows only empty, standing and burrowed **without effect
layers**. The bead (about 26×20 px, a brown-rimmed cap with blue-violet inside) is correct for "sunk", and 24 % of
its disc shows. However, its mound with layers on is not pictured. See F2-1.

### `burrow_dig`: accept

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | accept | The morph clip plays inside a full-size bubble, the soil rises in the clip, the body settles to the shallow clip sink, and the highlight goes matte by about t = 1.36 s. "The burrow's channel ended at 1.36 s with the body 77 % sunk", and the sequence keeps settling to t = 2.36 s without rising. |
| 2 Layered | accept | Clods are now visible: dark-brown chunks at the lower left and right of the bubble, t = 600 to 800 ms. Dust shows as pale grey-brown smudges at the right, t = 1 280 to 1 400 ms. Heaped earth is not visible (see `burrow_mound`). |
| 3 Timing | accept | 40 ms steps with labels (F-2 closed). The shadow shrinks over t = 560 to 1 000 ms, and the clods lead the dust. |
| 4 Pops | accept | No single-frame change. The shadow shrinks progressively (though it detaches, S-1), and the highlight fades. The dust drawn lighter over the shadow at t = 560 to 720 ms is airborne and acceptable. |
| 5 Colour | accept | The clods and dust are warm and earthy. |
| 7 Fog | accept | |

### `burrow_emerge`: accept

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | accept | t = 40 to 280 ms: the burrowed clip with clods thrown out round the rim. t = 320 to 400 ms: the morph clip heaves, and the standing clip follows from 400 ms. |
| 2 Layered | accept | Clods (dark brown, right and lower left, t = 80 to 360 ms) and dust (pale smears lower left, t = 240 to 600 ms). The lingering heaped earth is not visible after t ≈ 560 ms. That is the same cause as `burrow_mound`, covered by B2-1. |
| 4 Pops / the dip | accept | The bubble's outline is the same size in every frame from t = 240 to 440 ms, so there is no one-frame dip. The shadow grows back over about 0.12 s from a small detached spot (t = 280 ms, S-1), not in one frame. |
| 7 Fog | accept | |

### `mode_seal_sweep`: reject

| Item | Verdict | Reason |
|---|---|---|
| Arc against progress | accept | 0.456 of the turn at progress 0.472 (both drivers), within ±5 %. |
| 1 Reads | improved | `wp8r2_siege_mid_*` (Critical only, bubble hidden): a brown, irregular arc with dark clumps and a slightly varying width. It no longer reads as a clean UI circle. On the light floor it is still semi-translucent, with tile seams showing through. |
| 4 Cheapness | **reject** | The arc's **head is a hard radial cut**: at t = 2 160 to 2 480 ms in `wp8r2_siege_sequence_radv.png`, the leading end at about 4 o'clock ends in a straight edge (`ARC_HEAD` is 0.01 of a turn, about 3.6°, which reads as a cut at this size). There is no visible light at the head (`ARC_HEAD_LIGHT` 0.35 on the shading channel does not lift it enough). A band sweeping round with a crisp cut end is exactly the language of an RTS cooldown or progress ring. |
| Shadow | **reject** | The furrow is not under the shadow. Where it crosses the tank's shadow (lower right, t = 2 240 ms onward, and the settled battle), it is a lit brown band over the dark shadow. The amplified difference against bare is orange (brighter, redder) inside the shadow. I found no mechanism in the round-2 diff that makes a `Ring` card receive the shadow. |
| 7 Fog | accept | |

### `siege_anchor`: reject

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | **reject** | Battle, the tank at about (548, 245), radv and lavapipe alike: a translucent brown disc-halo round the hull at about 1.85 radii. Inside it, a **pale whitish smear** (the `crushed` layer, size 1.5 radii) lies on the ground, also visible in `wp8r2_unsiege_sequence_radv.png` at t = 0. It reads as haze or frost, not as soil pressed flat. The five `ground_crack` decals at 1.35 radii are not distinguishable at default zoom. |
| 4 Cheapness | **reject** | A **straight edge cuts the tank's shadow**: in `wp8r2_battle_undetected_*` against `_bare_*`, the change inside the shadow ends along a straight line about 47 px long, running from about (540, 265) to (587, 253) in window pixels (it is the boundary of the orange region in the amplified difference). On the 1.85-radius ring (≈ 40 px) a chord that long would be clearly curved, so this is a box, card or seam edge, not the ring's own boundary. The settled siege frames (t = 3 080 to 3 560 ms) show it too. The implementer must find which layer draws it: the `crushed` decal box, the ring card, or the arc seam. |
| Shadow | **reject** | Same as the sweep: the ring and the crushed layer are drawn over the shadow and lighten it. |
| 5 Colour | partial | The ring is brown, but the inner smear is pale grey-white. |
| 7 Fog | accept | |

## 5. Regressions

- **`cloak_own`: regressed** (S-1). A detached small dark shadow dot now sits beside the veiled unit. Otherwise the
  half veil and cool rim are unchanged and still read.
- **`cloak_shimmer`: no regression.** Colourless, nothing outside its disc, no shadow (§1).
- **`detected_mark`: no regression.** The body is crisp (§2).
- **The shipped splash effects: no regression found.** The round-2 shader changes are a rename (`lens` → `warp`,
  same offset 156, still 0 for every shipped card) and a ring branch taken only when `warp > 0` or `FLAG_ARC` is set.
  No shipped effect sets either. The round-2 catalog diff touches only WP8 looks and ramps. No golden or lab PNG
  changed. The unconditional `NoAutomaticBatching` changes no picture, only cost (§3).
- **Art rules.** No rays, stripes, sunbursts, beams or speed lines anywhere. The arc's dark clumps are irregular, not
  stripes.

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| Fog (item 7), whole WP | **accept** | Undetected burrowed and cloaked enemies: 0 px outside the shimmer disc, no shadow, on both drivers |
| `shimmer_check` (whole colours) | **accept** | Sound; the doc comment slightly overstates it |
| `cloak_shimmer` | **accept** | Unchanged and clean |
| `cloak_own` | **reject (regression)** | Held `shadow: 0.7` leaves a small detached dark dot beside the unit (S-1) |
| `detected_mark` | **accept**; C-1 closed | Body crisp with and without looks |
| `burrow_mound` | **reject** | Marble fixed and clip readable (≈ 98 % of disc), but the ground (hollow, heaped earth) is hidden under the near-full body; it reads as a flat token |
| `burrow_dig` | **accept** | Clods and dust visible, shadow shrinks over 0.4 s, settle continues; F-2 sequences good |
| `burrow_emerge` | **accept** | Clods, dust and heave visible; no dip; shadow grows back (detached spot, S-1) |
| `mode_seal_sweep` | **reject** | Earthier, but its head is a hard radial cut with no light (progress-ring language) and it is lit over the shadow |
| `siege_anchor` | **reject** | Straight edge across the shadow; pale haze instead of pressed soil; ring lit over the shadow |
| Shadow fix, correctness | **accept the batching diagnosis; reject the shrink centre** | Bevy 0.15 bins by mesh, so the fix is needed; scaling about the centre detaches the shadow |
| Shadow fix, cost | **measure or narrow** | Up to about 1 260 extra shadow draws at 400 units; apply `NoAutomaticBatching` only to bubbles with a non-neutral shell, or re-run `frame_budget` before merge |
| Shipped effects | **no regression** | |
| **Overall WP8 round 2** | **reject** | Fog, dig and emerge now pass; mound, sweep, anchor and the `cloak_own` shadow need round 3 |

## Required changes for round 3

- **S-1 (shadow shape).** In `bubble_shadow.wgsl`, scale the caster about its ground contact point instead of its
  centre, so a fading shadow contracts toward the unit's foot and never separates from it. For `cloak_own`, use
  `shadow: 1.0` (the 0.35 s fade remains) or show that 0.7 then leaves an attached faint shadow at the foot. Picture
  `cloak_own` in the battle again.
- **S-2 (shadow cost).** Insert `NoAutomaticBatching` only on bubbles whose shell's shadow lane or sink is non-zero.
  Insert it no later than the first non-neutral frame, and remove it after release. Add a unit or GPU test that two
  bubbles, one neutral and one with `shadow: 1`, cast different shadows while a third neutral bubble is batched.
  Otherwise, keep the unconditional form and re-measure 400 units with `frame_budget` before merge.
- **B2-1 (burrowed ground read).** Make the ground round a burrowed clip unit visible outside its near-full disc:
  move the `mound` heaps out to about 1.15 to 1.35 radii, make them large enough to show beyond the rim, and make the
  `hollow` show as a dark lip just outside the outline (≥ about 1.1 radii), so the token reads as set into the floor.
  The same applies to `burrow_emerge`'s lingering earth. Add to the test that the burrowed looks change a stated
  minimum share of the ground ring between 1.05 and 1.6 radii, not just a pixel count.
- **B2-2 (furrow head and shadow).** Give the arc's head a soft, wider, clearly lit end (freshly turned earth, about
  10–15° of the turn, brighter than the band), not a 3.6° cut. Make the ring and the pressed soil darken the ground
  multiplicatively (or sample the shadow), so that inside the tank's shadow they are no lighter than the shadow. If a
  `Ring` card cannot do that, render the furrow as decals. Consider short stabiliser gouges instead of a full sweeping
  ring, which still reads as a progress indicator.
- **B2-3 (anchor artefacts).** Find and remove the straight edge across the tank's shadow (about (540, 265) to
  (587, 253) in `wp8r2_battle_undetected_radv.png`). Make the `crushed` layer read as darker pressed soil, not a pale
  smear.

### Evidence

- **F2-1.** Picture the no-clip unit's burrowed pose *with* its effect layers, not only bare.
- **F2-2.** Print the clip unit's pose line ("clip unit standing changes … burrowed … (…%); round it (its shadow)
  …") into the measurements file. It is asserted in the test, but it is missing from `wp8r2_measurements.txt`, so
  the ≥ 85 % and ≤ 15 % shadow figures are not on record (my estimate from the images is about 98 % and no visible
  shadow).
- **F2-3.** Note in the packet that the "bare" view keeps the Shell, so readers do not take it as the unit without
  any look.
