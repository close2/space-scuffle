# Independent Review of WP8 (Concealment and Stance), Round 1

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), WP8 row of
§9, D-5, D-6, Amendment 2 (rules R4 to R6) and Amendment 11. Packet:
[`independent_review_packet.md`](independent_review_packet.md), section "WP8 round 1". Checklist: plan §7.2, as quoted
at the end of the packet; item 7 (fog) carries most weight. Owner rules: Q0076 (shimmer for an undetected cloaked
enemy; own concealed units stay readable; an undetected enemy never leaks). This is round 1 of at most five (Q0080).

## Method

I read the packet's WP8 section, the plan sections above, `assets/catalog/effects.ron` (the eight looks),
`assets/shaders/effects/distortion.wgsl` (diff against `main`), `client::battle_pipeline::fog` (`Presence`),
`client::effects::stance` (the shadow toggle) and the shimmer check in `crates/client/tests/stance_gpu_tests.rs`. I did
not read `slice_review.md`. I built and ran nothing; the figures below are ImageMagick crops (nearest-neighbour, 2x to
6x), `compare -metric AE` and amplified difference images of the committed PNGs, kept in
`~/scratch/wp8-review/`. Coordinates are window pixels of the 1280×720 battle captures, or cell-local pixels of the
pose sheets.

## 1. Fog and leaks (item 7)

**Verdict: accept.** I found no trace of an undetected enemy beyond the intended shimmer.

- **Undetected cloaked enemy, battle.** In `wp8_battle_undetected_radv.png` against `wp8_battle_bare_radv.png`, the
  difference around the enemy (window ≈ (775, 220)) is a single disc about 50 px across. It holds only bent tile seams,
  with nothing outside the disc. At the spot where the detected capture shows this unit's shadow (≈ (790–860, 225–265)
  in `wp8_battle_detected_radv.png`), the undetected and bare captures are identical. There is no contact darkening, no
  shadow, no rim, no tint and no bar. Lavapipe gives the same result. In the plain capture you see the shimmer only as a
  small sideways step in two tile seams. That is faint and right for Q0076.
- **Undetected cloaked enemy, pose.** In `wp8_shimmer_pose_*` cells 1 and 2, the seams inside the disc are bent by
  2–3 px and the colour does not change. Cell 3 (without looks) matches the empty ground, and the measurement line
  says "0 pixels".
- **Undetected burrowed enemy.** The pose test reports "empty vs burrowed: 0 changed, largest channel change 0" on both
  drivers. `Presence::presence` returns `Hidden` for burrowed and for out-of-sight units, and `Shimmer` only for an
  undetected cloaked unit in a cell the viewer sees. No held look is attached to a `Hidden` unit. This is sound.
  However, the battle tableau has no undetected *burrowed* enemy. See required change F-1.
- **An enemy seen starting to burrow.** The dig's one-shot layers keep playing after the unit vanishes at 33 ticks. The
  `earth_scar` decals (start 0.2, duration 2.0) outlive the vanish by about 0.9 s. The viewer saw the dig start, so this
  tells them nothing new, and StarCraft II also shows burrow dust. I accept this as an implementer decision.

### The `distortion.wgsl` fix

**Verdict: sound.** In WGSL, `@builtin(position)` in a fragment shader is in render-target pixels, with the viewport
offset already applied. The old code turned it into a viewport-relative UV, `(pos − viewport.xy) / viewport.zw`, and
then sampled a scene copy that covers the whole target with that UV. Whenever the viewport is smaller than the target,
this both shifts and stretches the read. In a battle, the 3D viewport leaves out the HUD bands. In these captures the
viewport is about rows 28–540 of 720, so a fragment near the bottom of the playfield read the copy roughly 180 px lower,
down in the black HUD area. Near the top it read about 28 px higher, from a different place.

The new code reads at `in.position + offset_px` divided by `textureDimensions(scene)`. It clamps the read to the
viewport, which also keeps it out of the HUD band of the copy, and pixel centres map to texel centres. The offset
magnitude is unchanged: the old x offset `along_x·(h/w)·…·w` px equals the new `along_x·…·h` px. So the effect lab,
whose viewport is the whole window, renders exactly as before.

**Effect on already-shipped effects: yes, they were wrong in battle.** `flame_line` (effects.ron l. 957) and
`lance_sweep` (l. 1065) carry `Distortion` heat-haze layers. In every real battle until now, their haze has been
painting a displaced and rescaled piece of the screen into its disc. That piece could be another unit, a terrain edge,
or black from the HUD band. This was not a fog leak, because the copy only contains what was drawn for this viewer.
But it was a visible defect that their lab reviews could not catch, since the lab's viewport is the window. The fix
corrects them. Their lab captures and goldens are still valid: no golden PNG changed in `main...HEAD`.

### The regression check (`shimmer_check`)

**Verdict: sound, with one limitation.** Each pixel that bends by more than 4 levels must stay within the per-channel
minimum and maximum of the reference frame inside ±6 px, with 4 levels of slack. The reference is the empty ground
(pose) or the same frame without effect layers (tableau). It caught the real bug: 133 of 2 074 bent pixels failed, and
the mean moved 4.10 levels.

The limitation is that it is a per-channel bounding box, not a colour-set test. On multi-coloured ground (grey, cyan
seams, rust), a colour that does not exist nearby can still pass if each channel falls inside its own range. A
dark-neutral shadow spot on light ground still fails it, which is the case that matters. The tableau check compares
against the bare frame, so it depends on the bare frame being clean. That is covered separately by the pose's "without
its looks: 0 pixels".

### Shimmer tolerances (question 5)

**Acceptable as set.**

- The 6 px reach is twice the largest declared bend (about 3 px at default zoom). It is not so wide that a misplaced
  read from 50 px or more away could pass.
- 0.5 % foreign pixels allows about 6 of roughly 1 200. That is filtering noise, and the measured value is 0.
- The 0.75-level mean shift is averaged over 1.6 radii. A tint confined to the 1.0-radius body would therefore need to
  average about 1.9 levels to reach the limit, which is still far below visibility. The measured shift is 0.05–0.19.

Optional: derive the reach from the declared bend (`(lens + 2·strength) × radius_px × 1.5`) so that a future stronger
lens cannot hide a misread inside a fixed 6 px.

## 2. Own concealed units (Q0076)

- **Own cloaked unit (`cloak_own`): accept.** In `wp8_own_cloaked_crop_radv.png` and in the battle at ≈ (690, 210),
  the picture shows at about half opacity under a pale cool rim with the ground visible through it, and it casts no
  shadow. It reads as "mine, hidden, here" at a glance. The looks add very little over the bare state: about 3 400 px
  change, mostly the rim and lens. That is acceptable, because the veil itself carries the read.
- **Own burrowed unit (`burrow_mound`): reject on readability.** At default zoom in the battle (≈ (580, 388)), the
  owner sees a dark-violet glossy bead about 28 px across with a specular highlight at upper left. The bead's lower
  third is a brown band containing three or four orange specks. That band is the middle strip of WP13's burrowed clip,
  and it is not readable as a scorpion, as dug in, or as anything else. Because of its full round outline and the
  highlight, it reads as a **small marble sitting on the ground**, not a dome sunk in a hollow. From this camera, a
  sphere sunk through a plane shows a round cap, and the glass highlight completes the illusion of a whole ball. The
  owner can tell that *something* is there, but not *what* it is.

## 3. Decision: the sink for units with a burrowed mode clip

**Recommendation: drop the area sink for units whose burrowed mode has media. Keep the ≤ 35 % sink only for units
without mode media.**

Reasons:

1. WP13's burrowed clip already depicts the burrowed state: the body under a heap of soil. With the 35 % sink, only
   the middle strip of that clip shows, and in these captures that strip is the least legible part. The project paid
   for that media (Q0075), and the sink hides it.
2. The sunk cap does not read as "sunk" (see §2). It reads as a smaller sphere. The sink therefore does not do the job
   it exists for, even for units without media. That is why it should be kept only as the fallback.
3. For a unit with a clip, the "dug in" read should come from the ground contact instead: no shadow (eased, see B-2),
   the `hollow` decal and heaped earth round the rim, the soil band, and suppression of the glass specular highlight on
   a burrowed dome (that highlight is what turns the cap into a marble). A shallow flattening is acceptable for one cue
   of lowness, as long as at least about 85 % of the clip's disc stays visible. An example is a sink of at most 0.15 of
   the diameter, or a vertical squash of the dome.
4. Plan test change: for archetypes with a burrowed mode clip, replace "above-ground area ≤ 35 %" with "the clip's
   disc ≥ 85 % visible, no shadow, mound looks inside 1.8 radii". Keep ≤ 35 % for archetypes without one. Record this
   in the amendment as a Q0002 decision.

## 4. Each look (checklist items 1–7)

Note on evidence: the sequences carry neither timestamps nor a fixed frame interval ("about every 0.15 to 0.5 s"). The
lavapipe dig has 4 frames, the radv emerge 3, and the lavapipe emerge 1. Items 3 and 4 (timing and pops) cannot be
judged from this, and some layers (clods, emerge throw) may simply fall between frames. Round 2 needs a fixed-interval
take (40 ms, as in the scale-out rounds) with time labels for dig, emerge, siege and unsiege. See F-2.

### `cloak_shimmer`: accept

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | accept | Battle ≈ (775, 220): a faint lens step in the seams, no shape of its own. Readable only to a player who looks for it, as Q0076 intends. |
| 2 Layered | accept (n/a) | Q0076 forbids more than one element; one distortion layer is right. |
| 3 Timing | not judged | Comes in over 0.35 s; the crawl is not evidenced (single frames). |
| 4 Cheapness | accept | No disc edge: the rim smoothstep (0.86→1.0) makes the bend vanish at the outline; no colour fringe in the difference image. |
| 5 Colour | accept | Colourless: mean shift ≤ 0.19 levels, 0 foreign pixels on both drivers. |
| 6 Crowd | not judged | Not in this packet. |
| 7 Fog | accept | See §1. |

### `cloak_own`: accept

Items 1, 5 and 7 accept, see §2. Item 4: the lens is invisible at default zoom, which is harmless. One note under B-2:
the shadow is removed in a single frame while the veil comes in over 0.35 s.

### `detected_mark`: accept, with a check

At ≈ (775, 220) in `wp8_battle_detected_*`, the enemy is drawn with its shadow, a thin pale-lilac outline light and its
picture. The outline light reads as "marked" without being a ring or a bar. Items 1, 4, 5 and 7 accept.

**Check for round 2:** the centre and lower body of the detected unit look milky, with ground texture showing through
at about 30 %, while a normally shown unit, such as pose cell 2, is crisp on dark violet. `detected_mark` declares no
veil. Either the stand-in's picture is like that, or the lens disc or a leftover cloak state is washing the body.
Please add a capture of the same detected unit with its looks off, so this can be told apart. If it is the look, the
unit is not "drawn as it is".

### `burrow_mound`: reject

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | **reject** | Reads as a glossy marble on the ground, not a low soil-caked dome in a hollow (§2). The owner cannot read the unit. |
| 2 Layered | accept | Shell, hollow decal, heaped-earth decals. |
| 4 Cheapness | **reject** | `wp8_burrow_pose_radv.png` cell 3, ≈ (230–323, 177–240) in cell coordinates, the same on lavapipe: one of the heaped-earth marks ends in a **hard straight diagonal edge** above and right of the dome. It reads as a grey wedge with a ruled edge, like the edge of a decal quad or projection box. The rest of the mound is a faint grey haze, too weak to read as earth on this light metal floor; the battle crop shows the same. |
| 5 Colour | reject | The mound marks are cool grey, not soil, and the dome is the bubble's violet with a brown band. "Soil-caked" does not read. |
| 7 Fog | accept | Undetected: 0 px. |

### `burrow_dig`: reject

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | partial | Frames 1–3 (`wp8_dig_sequence_radv.png`) show WP13's morph clip inside a nearly full-size bubble. Between frames 3 and 4 the body drops to the bead. The soil creeping up the glass is not distinguishable from the clip's own dirt. |
| 2 Layered | **reject** | The clods (three `burrow_clods` bursts) are not visible in any frame on either driver. The dust is visible only in radv frame 4, as a faint grey plume at upper right and a few dark smears lower right. The heaped earth is a faint haze. In practice the dig reads as one element (the shell) on top of the clip. |
| 3 Timing | not judged | No timestamps (F-2). The measurement shows the D-6 settle working: the channel ends with the body 76 % (radv) or 73 % (lavapipe) sunk, and the dig continues. |
| 4 Pops | **reject** | The bubble's shadow, a large dark ellipse wider than the unit (pose cell 2), is gone from frame 1 onward while the bubble is still at full size. `stance.rs` l. 552–556 inserts `NotShadowCaster` as soon as the shell's sink or veil is non-zero, so the shadow vanishes in one frame at the start of the dig. That is the largest pop in the sequence (B-2). |
| 7 Fog | accept | See §1. |

### `burrow_emerge`: reject (not evidenced, plus the shadow pop)

`wp8_emerge_sequence_radv.png` has three frames: the bead, then the unit fully standing with no shadow, then the unit
standing with its shadow. `wp8_emerge_sequence_lavapipe.png` has one frame, with the unit already up. No frame shows
the heave, thrown soil, dust, or the heaped earth that is meant to "stay a while". The `earth_scar` layer runs for
3.0 s with a `linger` curve, so it should be visible in frame 2 at about 0.3–1 s, and it is not. On a light floor that
again points to marks too faint to read. The shadow pops back in a single frame between frames 1 and 2 (B-2). I cannot
accept a look I cannot see.

### `mode_seal_sweep`: reject (mechanism accepted, look rejected)

- **Arc versus progress: accept.** The measured arc is 0.475 of the turn at progress 0.486 (radv) and 0.500 at 0.514
  (lavapipe), well inside ±5 %. In `wp8_siege_mid_*` the arc runs from about 9 o'clock over the top to about 3 o'clock,
  a clean half turn. The siege and unsiege sequences close and open the arc monotonically.
- **Look: reject.** The furrow is a **uniform-width, translucent, cool-grey band on an exact circle** at 1.85 radii,
  and the tile seams show straight through it. It reads as a drawn ring: a range or selection circle, which is UI
  language in an RTS. It does not read as ploughed ground. There is no berm of thrown soil on its outer edge, the width
  does not vary, and the 0.35 breakup is invisible at default zoom.
- **No lit head.** The packet's "its head lit" is not visible in any siege or unsiege frame, or in `wp8_siege_mid_*`.
- **The dust and clods at the start** show only as two brownish blobs inside the shadow in siege frame 1.
- **Shadow (item 4).** The ring is drawn over the bubble's shadow at full brightness. Compare the ≈ (590–610, 275–300)
  area of `wp8_battle_undetected_radv.png` with the bare frame: inside the dark shadow it shows a lit grey-brown band.
  This makes the mark float above the ground, and it is one more cue that it is an overlay.

### `siege_anchor`: reject

It is the same ring, closed, and the battle crop at ≈ (548, 245) reads as a selection ring round the tank. The "soil
pressed down under the stabilisers" (four `ground_crack` decals at 0.85 radii) lies under the bubble and its shadow,
and I cannot find it in any frame. The anchored stance therefore depends entirely on a ring that reads as UI. The
sieged clip (WP13) does read on its own, which is good, but the effect adds no ground read.

### Art rules (no rays, stripes, sunburst, beams or speed lines)

No violation in any look. The dark smears below the bead in dig frame 4 are short and irregular, not radial streaks.

### The shipped seven splash effects

These were not regressed by WP8 as far as the evidence shows: no golden or sheet changed. The arc mask (`FLAG_ARC`) and
the lens are off for them. The distortion fix changes `flame_line` and `lance_sweep` **in battle only**, and changes
them for the better (§1). Their lab captures are unchanged.

## Verdict table

| Look | Verdict | Main reason |
|---|---|---|
| `cloak_shimmer` | **accept** | Colourless, faint, nothing outside the disc; 0 foreign px on both drivers |
| `cloak_own` | **accept** | Half veil plus cool rim reads as "mine, hidden, here" |
| `detected_mark` | **accept** (check C-1) | Thin cold outline; possible washed body to be checked |
| `burrow_mound` | **reject** | Reads as a glossy marble; clip unreadable; hard straight edge on a mound decal; mound too faint and grey |
| `burrow_dig` | **reject** | Clods invisible, dust and earth faint; shadow pops off at the start |
| `burrow_emerge` | **reject** | None of its layers is visible in the evidence; shadow pops back on |
| `mode_seal_sweep` | **reject** | Arc tracks progress correctly, but it reads as a translucent UI ring; no lit head; drawn unshadowed over the shadow |
| `siege_anchor` | **reject** | Same ring; stabiliser soil not visible |
| Fog (item 7), whole WP | **accept** | No leak found; the `distortion.wgsl` fix and its check are sound |
| Shimmer tolerances | **accept** | 6 px, 0.5 %, 0.75 levels are adequate |
| Shipped effects | **no regression**; `flame_line` and `lance_sweep` were broken in battle before this fix |
| **Overall WP8 round 1** | **reject** | Fog and cloak accepted; burrow and siege looks need round 2 |

## Required changes for round 2

- **B-1 (burrowed readability).** Adopt §3: for archetypes with a burrowed mode clip, show at least about 85 % of the
  clip's disc, with at most a shallow sink or squash. Suppress the glass specular highlight on a burrowed dome. Keep the
  ≤ 35 % sink only for archetypes without mode media. Change the WP8 test to match.
- **B-2 (shadow pops).** Fade the shadow's strength with the sink and veil progress instead of toggling
  `NotShadowCaster`. A shadow-strength term in the bubble material, or a scaled shadow proxy, would both work. It must
  go out over the dig and come back over the emerge, and come and go with `cloak_own`'s 0.35 s.
- **B-3 (mound and dig ground marks).** Remove the straight edge on the heaped-earth decal at the upper right of the
  dome, by feathering the decal's box or projection edge. Make the heaped earth and the hollow read as soil on a light
  floor: a warmer, darker value (brown, not cool grey), opaque enough to read at 28 px, and an irregular rim of
  heaped clumps rather than a haze.
- **B-4 (dig and emerge secondary layers).** Make the clods and dust visible at default zoom on both drivers. Bigger,
  darker clods are needed, because none is visible now.
- **B-5 (furrow).** Make the furrow read as ground, not UI:
  - break the width (vary it by at least ±30 % along the turn) and break the circle (a radial wobble of a few percent);
  - make it darker and earthier, mostly opaque, with a lighter thrown-soil berm on its outer edge;
  - give the arc's head the declared light (freshly turned earth, brighter, for the leading 10–15° of the arc);
  - render it as a ground decal that receives the shadow, or at least is not drawn lit over it.

  Consider whether the ring is the right shape at all. A pair of short stabiliser gouges plus crushed soil under the
  hull may read better than a full circle. The arc-versus-progress rule can live on the gouges' growth.
- **B-6 (anchor).** Move the stabiliser soil out from under the bubble (beyond 1.0 radii, where the stabilisers would
  stand), so it is visible.

### Evidence and tests

- **F-1.** Add an undetected burrowed enemy to the battle tableau, and show that its area is identical to the bare
  frame (0 px within the 2-pixel tolerance).
- **F-2.** Make the sequences a fixed 40 ms interval with time labels: dig 1.8 s, emerge about 1 s including the
  lingering earth, and siege and unsiege across the whole channel. Use the same takes on both drivers.
- **C-1.** For `detected_mark`, add a capture with its looks off, to tell whether the washed body comes from the look.
