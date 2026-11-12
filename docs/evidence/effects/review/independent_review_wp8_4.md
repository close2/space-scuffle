# Independent Review of WP8 (Concealment and Stance), Round 4

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md),
Amendments 11 to 14. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP8 round 4".
Previous rounds: [`independent_review_wp8_1.md`](independent_review_wp8_1.md),
[`independent_review_wp8_2.md`](independent_review_wp8_2.md) and
[`independent_review_wp8_3.md`](independent_review_wp8_3.md). Round 3 required A3-1 and E3-1, and advised E3-2 and
E3-3. Owner rules: Q0076, and Q0080 (this is round 4 of at most five). I did not take part in building this work or
in rounds 1 to 3.

## Method

I read the packet's round-4 section, review 3, Amendment 14, Q0076, and the round-4 diff `20c398d..8d600b9`: the
`siege_anchor` and `mode_seal_sweep` entries in `effects.ron`, `decal.wgsl`, `sprite.wgsl`, `common.wgsl`,
`effects/render.rs` (the queue, `cut_runs` and `DrawEffectRun`), `effects/runtime.rs` (`push_sorted_card`,
`sort_cards`, the card writer), `instances.rs`, the catalog schema and resolver, and the new tests in
`stance_gpu_tests.rs`, `battle_fog_tests.rs` and `effect_runtime_tests.rs`. I did not read the implementer's
self-checklist before forming these verdicts. I built, ran and gated nothing.

The figures come from ImageMagick work on the committed PNGs: crops at 2× to 6× (nearest-neighbour), full/bare ratio
maps (sRGB grey) and profiles, `compare -metric AE` against the round-3 captures, and amplified differences. The
working files are in `~/scratch/wp8-review4/`. Battle coordinates are window pixels of the 1280×720 captures (radv
unless stated otherwise). Sequence frames are named by their `t` label.

## 1. `siege_anchor` (A3-1): accept

The held look is now a soft darkening under and round the hull and four darker soft patches at about 1.4 radii.
There is no ring, rim or outline.

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | accept | Battle (548, 244) and `wp8r4_anchored_crop_*`: two warm brown smudges show on the lit floor at about 9 o'clock (upper and lower left of the bubble), and a third sits at about 3 o'clock inside the hull's shadow. The fourth is under or behind the bubble. A faint pressed tone links them under the hull. It reads as ground pushed down where something heavy stands. It does not read as a selection, aura or range language: nothing traces a circle, and the ratio map (full/bare) shows no closed boundary. The stance cue at default zoom is now modest. It rests mostly on the two lit-side smudges (polish P4-2). |
| 2 Layered | accept | It is a Minor event: the pressed disc, the four patches, and the settling dust (`unsiege` t = 40 to 400 ms, and the siege's last frames). |
| 3 Timing | accept | Siege t = 3 360 to 3 560 ms: the furrow erodes while the soil and patches come in over 0.6 s. There is no pop. Unsiege t = 40 to about 600 ms: dust and clods, and the anchor fades. Only authored decals stay. |
| 4 Cheapness | accept | **Edge:** the ratio profiles across the left patch (crop rows y = 55, 60, 65, battle crop at (478, 189)) fall 0.99 → 0.90 → 0.76 → 0.58 → 0.42 over about 10 px in sRGB. On the right side the fade runs over 10 to 14 px. No row has a one-pixel step. I recomputed the printed 4.5 px (0.19 radii) for the 0.95 → 0.75 *linear* band, and it agrees with these sRGB widths. **Radial streaks:** none. The ratio map at 6× and siege t = 3 360 to 3 560 ms at 4× show smooth blobs and no spokes, rays or sunburst on either driver. **Tell:** the patches are near-round dots of similar size at equal radius (polish P4-2), but at default zoom they read as smudges, not as a pattern. |
| 5 Colour | accept | Darkens only: "darken 1945 / 1968, lighten 0 of 8 163" (radv / lavapipe). Patch cores keep 0.30 to 0.45 of the light, and the pressed soil keeps 0.67 to 0.85. It is warm brown, and darker inside the shadow, never lighter. |
| 6 Crowd | accept | Five held decals per sieged tank. There is no held card any more, so a sieged tank adds nothing to the card pass. |
| 7 Fog | accept | It is unchanged in kind (decals of the unit's own look). The fog figures below hold. |

Its body is not multiplied. Where its translucent body overlaps the left patch, the dark treads at crop (52, 58)
drop from 59 to 40 levels, with the same absolute drop (about 18 levels) on brighter pixels nearby. That is the
bubble's translucency over a 0.3 patch behind it, not a multiply over the body, which would scale every pixel by the
same factor. The code agrees (§3).

## 2. `mode_seal_sweep`: accept (with polish)

- **Edges.** `wp8r4_siege_mid_radv.png` columns x = 150 and 290 of the full half: 0.98 → 0.79 → 0.60 → 0.74 → 0.96
  and 0.95 → 0.68 → 0.84 → 0.74 → 0.92. That is a fade over several pixels on each side, instead of round 3's
  1.00 → 0.68 in one pixel. A3-1's band requirement is met.
- **Still a furrow?** Yes, but only just, and less than in round 3. At the mid shot it is a soft brown stroke of
  uneven width (wider at its ends, thin at the top right), with no dark-trench / lighter-berm structure left and no
  clod texture. The digging-in reads from the whole sequence: clods and dust at t = 160 to 560 ms, then the band
  sweeping round. When it closes (t ≈ 2 600 to 3 100 ms) it is a thin soft ellipse round the hull for about half a
  second. That is the nearest the WP comes to ring language, but it is soft, uneven, short-lived, and erodes into
  the anchor. I do not reject it. See P4-1.
- **Erosion.** t = 3 360 to 3 520 ms: it breaks up into irregular pieces, with no rays and no pop.
- The arc still tracks the progress: "at 0.472, its arc draws 0.450" (both drivers), within the 3 % head allowance.
- Darkens only, including where it crosses the shadow.

## 3. E3-1: darkening cards as a sorted prefix: correct order, but a new run-cutting regression (E4-1, required)

**What is right.**
- `push_sorted_card` gives a `FLAG_DARKEN` card the depth `f32::MAX`. `sort_cards` sorts the depth in descending
  order, so every darkening card leads `sorted`. `darkening_cards` is counted only after the capacity check, and the
  push always follows, so the count equals the prefix length. Particles never carry the flag.
- `queue_effects` adds the darkening decals, then the darkening card item (`DARKENING_RUN`, drawing
  `0..darkening_cards`), then the ordinary decals, all at `f32::MIN`. The radix sort is stable, so this order holds,
  and no transparent item (a bubble, card or decal) draws before them. A darkening card can therefore multiply only
  the opaque scene. E3-1 as asked is done.
- The darkening pipeline collapses non-darkening instances, and the normal pipeline collapses darkening ones
  (`sprite.wgsl` vertex, `DARKEN_PASS`). Each card is drawn by exactly one pass.
- **The back-of-furrow check.** The worker stands 1.62 world units behind the tank, on the furrow, which passes
  behind the lower part of its body on screen. "0 of 211 body pixels below 0.85 (lowest 0.857); the furrow darkens
  78 pixels round its foot" (both drivers). The furrow's own factor is about 0.6, so a multiply over the body would
  have pushed most body pixels below 0.85, and the threshold is sensitive enough. The test does not show that the
  round-3 queue would have *failed* it (no negative control; advisory A4-3). The body evidence is sound, and the
  code makes the property hold by construction.

**What is wrong: E4-1.** The darkening prefix is still fed into the cutting of the *ordinary* card runs, with its
real positions.

```rust
// render.rs, queue_effects
cut_runs(
    gpu.centers.iter().map(|center| rangefinder.distance_translation(center)),  // includes the prefix
    &runs.others, &mut runs.cards,
);
...
distance: rangefinder.distance_translation(&first),  // first = centers[range.start]
```

`cut_runs` assumes its distances ascend (back to front), and its cursor `below` only moves forward. The prefix's
cards are first in the buffer, but their *real* distances can be anywhere. A furrow ring's centre is the tank's foot.
If that tank is nearer the camera than other card effects, the first prefix card moves `below` past every other
transparent item nearer than the tank. Every ordinary card farther than the tank then sees no crossing and falls
into the first run. That run is queued at the prefix card's distance, the tank's.

Result: while any tank is switching mode (the furrow is the only darkening card: about 3 s per switch, plus 0.6 s
of erosion), every shipped card effect behind that tank (smoke, flame, flak, cleave and spine cards) is drawn
*after* the bubbles and cards that stand between it and the tank, and paints over them. This happens precisely in
combat, where tanks siege. Round 3's queue did not have this, because each darkening card sat at its own sorted
depth.

The shipped goldens cannot catch it: none of them contains a darkening card. No golden or lab PNG changed in this
diff.

## 4. E3-2 and E3-3

- **E3-2: accept, sufficient.** `the_batching_marker_follows_the_shell_on_every_frame` steps the real app frame by
  frame through neutral (20), cloaked (40), neutral (40), burrowing and burrowed (120), and emerging (120). On every
  frame it asserts `contains::<NoAutomaticBatching>() == casts_own_shadow(shell)`, reading the shell from the
  material asset. It also asserts that the marker was inserted and removed exactly once per concealment, (1, 1) then
  (2, 2), so the shadow-lane-only cloak and the sink path are both exercised, and it ends batched. It checks the main
  world, not the render world. Review 3 established that extraction rebuilds from the main world every frame, so
  this is the right place.
- **E3-3: accept.** One darkening card item draws only the prefix (`runs.darkening`), and the per-run second items
  are gone. E4-1's fix removes the prefix from the ordinary runs too, which also saves their collapsed vertex work.

## 5. Regressions

Compared with the round-3 captures (`compare -metric AE -fuzz 2%`):

| Capture | radv | lavapipe | Finding |
|---|---|---|---|
| `emerge_sequence`, `burrow_pose`, `shimmer_pose` | 0 | 0 | identical |
| `own_cloaked_crop` | 9 | 0 | identical (`cloak_own` holds) |
| `dig_sequence` | 8 508 | 0 | radv only: clods in t = 360 to 440 ms show on the left in round 4 and not in round 3. lavapipe is identical, so this is per-run variance in the capture, not a change of look. Both versions are accepted looks. |
| `plain_burrow_pose` | 1 710 | 1 755 | The unit's animation phase and the mound's grain. Same look (sunk bead in a small patch). |
| `battle_undetected` / `battle_detected` | 2 545 / 2 609 | 2 455 / 2 467 | Confined to the anchored tank (the new look), plus radv-only animated-sprite noise at the burrowed scorpion (round (580, 378): largest change 38, mean 0.57, lavapipe 0) and a few unit sprites. |

- **Fog: accept.** "Undetected burrowed enemy: 0 pixels changed", the patch test "empty vs burrowed: 0", and "its
  shimmer bends 1 224 / 1 070 pixels, 0 to a colour not near them" on both drivers. These match round 3.
- **`cloak_shimmer`, `cloak_own`, `detected_mark`, `burrow_mound`, `burrow_dig`, `burrow_emerge`:** no regression.
  "57.0 / 57.2 % beside the body, 0 beyond the ring", "channel ended at 1.36 s, 77 % sunk" and "0 burrowed shadow"
  are unchanged.
- **Shipped splash effects:** the goldens are unchanged (no golden in the diff). The `DecalInstance` size is
  unchanged: `outline_soft` takes the old padding, which was always 0, and `soft == 0` keeps the old
  one-pixel smoothstep. The `sprite.wgsl` band change is gated by `ring_inner > 0 && warp > 0` (only the furrow has
  `waver`). **But** E4-1 changes the shipped cards' draw order while a darkening card exists. The goldens do not
  exercise that.
- **Art rules:** no rays, stripes, sunburst, beams or speed lines anywhere in the round-4 material.

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| `siege_anchor` (A3-1) | **accept** | Soft pressed soil plus four soft patches; no ring or rim; a fade over about 10 px; 0 lightened; no streaks |
| `mode_seal_sweep` | **accept** (polish P4-1) | Soft band edges; erodes into the anchor; thinner and flatter than round 3, but still reads with the clods |
| Fog (item 7) | **accept** | 0 px at the burrowed enemy; shimmer within the ground's colours |
| `cloak_shimmer`, `cloak_own`, `detected_mark` | **accept** (no regression) | Identical captures |
| `burrow_mound`, `burrow_dig`, `burrow_emerge` | **accept** (no regression) | Identical, or capture variance only |
| E3-1, darkening item order | **done** | Prefix at `f32::MAX`, one item at `f32::MIN` after the darkening decals; the back-of-furrow body keeps ≥ 0.857 |
| E3-1, side effect on ordinary runs | **E4-1, required** | `cut_runs` is fed the prefix's real distances, so ordinary cards behind a switching tank merge into one run drawn at the tank's depth |
| E3-2 | **accept** | Per-frame lifecycle test with insert and remove counts |
| E3-3 | **accept** | One darkening item over the prefix only |
| Shipped effects | **goldens unchanged; draw order at risk (E4-1)** | |
| **Overall WP8 round 4** | **reject (narrowly), engineering only** | Every look is accepted. One small queue fix (E4-1) remains. |

## Required change for round 5

- **E4-1 (card runs exclude the darkening prefix).** In `queue_effects`, cut the ordinary card runs from
  `gpu.centers[gpu.darkening_cards as usize..]` only, and offset each resulting range by `darkening_cards`. For
  example, give `cut_runs` a start index, or iterate `centers.iter().skip(n)` and add `n` to the pushed indices.
  The first run's queued distance then becomes that of the first *non-darkening* card. The normal pipeline already
  collapses the darkening cards, so leaving them out of the ordinary ranges is correct.
  - **Test (headless, required):** a unit test beside `cut_runs` (or of the extracted helper) with a prefix whose
    distance is *nearer* than all following cards (for example prefix −1.0, then −10, −9, −8, with others −9.5 and
    −8.5). Assert that no ordinary range contains a prefix index, and that the cards are cut at −9.5 and −8.5
    exactly as without the prefix.
  - **Evidence (optional but welcome):** one capture of a shipped card effect behind a tank mid-switch, with a
    neutral unit between them, against the same frame without the switch.
  - This touches neither the looks nor the goldens, so round 5 needs no new art review for it, only this fix and
    its test.

## Advisory (not blocking)

- **A4-1.** A card whose depth is NaN (a NaN centre) sorts above `f32::MAX` under `total_cmp` and would push one
  darkening card out of the prefix, so no pass would draw it. Keep depths finite (for example
  `depth.is_finite().then_some(depth).unwrap_or(0.0)`), or use `f32::INFINITY` together with a finite guard.
- **A4-2.** The anchored falloff is 0.19 radii against a 0.15 floor. That is a thin margin if the soil is later
  thickened.
- **A4-3.** The back-of-furrow test has no negative control. A one-off run with the darkening item at the run's
  distance, showing that the body check fails, would prove its sensitivity. A note in the evidence is enough.

## Polish (non-blocking)

- **P4-1, `mode_seal_sweep`.** It lost the trench-and-berm structure and most of its body: it now reads as a soft
  brown stroke, and as a thin ellipse when closed. Lower `WAVER_SOFT` to about 0.35, or raise the band's core
  intensity, so a darker core of about two pixels survives. Let the `WAVER_ACROSS` shading show again across that
  core.
- **P4-2, `siege_anchor`.** The 3 o'clock patch falls in the hull's shadow and the fourth hides under the bubble, so the stance
  cue is mostly the two lit-side smudges. Raise `size_jitter` (0.15 → about 0.35) and `spread`, and stretch the
  patches tangentially (pads, not dots), so the four marks look less like equal round dots. Avoid radial
  elongation, which would point toward rays. With WP13's sieged atlas on `main`, the hull itself now carries the
  stance, so a subtle ground mark is fine.
- **`burrow_mound` crest:** still optional, as in round 3.
