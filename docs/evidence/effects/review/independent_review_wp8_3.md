# Independent Review of WP8 (Concealment and Stance), Round 3

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), Amendments
11 to 13. Packet: [`independent_review_packet.md`](independent_review_packet.md), section "WP8 round 3". Previous
rounds: [`independent_review_wp8_1.md`](independent_review_wp8_1.md) and
[`independent_review_wp8_2.md`](independent_review_wp8_2.md). The round-2 required changes were S-1, S-2, B2-1 to
B2-3 and F2-1 to F2-3. Owner rules: Q0076, Q0080 (this is round 3 of at most five). I did not take part in building
this work or in rounds 1 and 2.

## Method

I read the packet's round-3 section, both earlier reviews, Amendment 13, Q0076, the frame-budget note
[`2026-11-12-frame-budget-wp8-shadows.md`](../../2026-11-12-frame-budget-wp8-shadows.md), and the round-3 diff
`1547da7..a34b370`. From that diff I read `bubble_shadow.wgsl`, `effects/common.wgsl`, `decal.wgsl`, `sprite.wgsl`,
`effects/render.rs`, `runtime.rs`, `instances.rs`, `placement.rs`, `stance.rs`, `scene_controller.rs` and the
WP8 entries in `effects.ron`. I checked the batching claims against the Bevy 0.15.3 sources
(`bevy_pbr/src/render/mesh.rs`, `light.rs`, `bevy_core_pipeline/src/core_3d/mod.rs`, `bevy_render/src/render_phase`).
I did not read the implementer's self-checklist. I built, ran and gated nothing.

The figures below come from ImageMagick work on the committed PNGs: crops at 2× to 6× (nearest-neighbour),
`compare -metric AE`, amplified differences, per-pixel full/bare ratio profiles, and colour samples. The working
files are in `~/scratch/wp8-review3/`. Battle coordinates are window pixels of the 1280×720 captures (radv unless
stated otherwise). Sequence frames are named by their `t` label. As the packet notes (F2-3), "bare" keeps the Shell.

## 1. Fog and leaks (item 7)

**Verdict: accept.**

- **Undetected burrowed enemy.** In a 120×110 patch round (796, 211), before, bare and the tableau are identical
  (0 px at 1 % fuzz) on both drivers. This matches the printed "0 pixels changed where it stands".
- **Undetected cloaked enemy, shadow included.** In a 160×130 patch round (893, 230), wide enough for its shadow's
  reach, before and bare are identical (0 px) on both drivers. So the unit casts no shadow. The new foot-anchored
  caster and the conditional batching did not bring one back. Tableau against before: 1 619 px (radv) and 1 529 px
  (lavapipe) at 1 % fuzz, all inside one disc. The amplified difference shows only displaced tile seams. The
  largest channel change is 58, and the patch mean moves 0.08 levels (121.79 → 121.87). There is no dark or
  coloured blob. The printed "0 of them to a colour not near them" (both drivers) is consistent with this.
- The darkening pass cannot leak. The hidden burrowed enemy holds no look, and the shimmer is drawn in
  `Transmissive3d`, before any darkening `Transparent3d` item.

## 2. S-1: the shadow's shape, and `cloak_own`

**S-1 is closed. `cloak_own`: accept (the regression is fixed).**

- The shader now scales about `foot = centre − (0, |world_from_local[1]|, 0)`. That is the sphere's lowest point,
  because the bubble mesh is a unit sphere scaled uniformly by the radius. A shrinking caster therefore keeps its
  contact point, and its shadow contracts toward the foot along the light.
- `wp8r3_battle_undetected_radv.png` round (691, 212), compared with round 2's capture at 6×: the detached
  12×8 px dark-green dot below and to the right of the bubble is gone. All that remains is a soft dark area under the
  lower third of the veiled body, seen through the veil, plus a faint green sliver no more than about 3 px wide
  just outside the rim at the bottom. It is attached and faint, and it reads as a unit standing lightly on the
  ground. `wp8r3_own_cloaked_crop_*` cell 2 against cell 3 shows the same on both drivers.
- **Dig** (t = 480 to 760 ms): the shadow contracts under the body and never separates from it. **Emerge**
  (t = 360 to 440 ms): it grows back out from under the body's lower edge. There is no gap of lit floor in any
  frame.

## 3. Engineering

### 3.1 Conditional `NoAutomaticBatching` (S-2)

**Correct, with one gap in the tests (advisory E3-2).**

- **Rebuilt every frame: confirmed.** Bevy 0.15.3 has two extract paths. `extract_meshes_for_cpu_building` and
  `extract_meshes_for_gpu_building` both query `Has<NoAutomaticBatching>` with no `Changed` filter, and both call
  `render_mesh_instances.clear()` before rebuilding. `BinnedRenderPhase` clears its batchable, unbatchable and
  batch-set lists every frame, and `queue_shadows` bins with `BinnedRenderPhaseType::mesh(should_batch())`. A
  component inserted or removed in `Update` therefore takes effect in that frame's extract, whichever path radv
  selects.
- **No mismatched frame.** `present_stances` computes the shell, writes it to the material (`sync_bubble_shell`) and
  queues the component change from the same `shell` value in the same system. The commands are applied before the
  end of `Update`, so the material change and the component change reach the same extract. A newly spawned bubble
  gets the default (neutral) shell and no component, which is consistent. Its first non-neutral shell arrives
  together with the component. If the material is missing, the component may still be inserted. That only
  unbatches a bubble, which is always correct.
- **The predicate matches the shader.** `casts_own_shadow` (`driven.z > 0 || shape.x > 0`) is exactly the branch
  condition in `bubble_shadow.wgsl` (`clamp(driven.z) > 0 || shape.x > 0`). So no bubble whose caster is moved can
  stay in a batched bin. Since every bin's representative is then neutral, every batched instance casts correctly.
- **Is the mixed-bubble test sufficient?** It covers the important static case. The burrowed unit (unbatched) casts
  nothing (round it: "0 burrowed"), and a neutral unit among the batched ones keeps its shadow exactly ("0 pixels
  of its shadow ring changed"). Together these exclude both failure modes of a wrong representative: the neutral
  units losing their shadows, or the burrowed one casting a full shadow. It does not cover:
  - a shadow-lane-only bubble (`cloak_own`). The battle tableau gives indirect evidence for this, a veiled unit
    with an attached shadow beside neutral units;
  - the insert and remove frames;
  - stale components.

  None of these can go wrong under the code as written (the reasoning above). A cheap headless ECS test would still
  lock this in against later edits (E3-2, advisory).

### 3.2 The darkening pass (B2-2, B2-3)

**Correct for the shipped effects and for the WP8 decals. One ordering risk for darkening cards (E3-1, required).**

- **Blend.** Colour `src × Dst + dst × 0` is the target multiplied by the output. Alpha `0 × src + 1 × dst` leaves
  the target's alpha alone. `darkening_factor` returns `mix(1, rgb, a × coverage)`, clamped, with alpha 1. This is
  correct on HDR and MSAA targets. Every early exit in the two fragment shaders is a `discard`, never a
  `return vec4(0)`. (Returning 0 would have blackened the target in the multiply pass.) The liquid branch returns 1,
  and the catalog refuses a darkening liquid.
- **The shipped effects are untouched.** Without a darkening element the queue adds exactly the items it added
  before. The key gains `darken: false`, which specialises the same shader, plus one vertex branch that is never
  taken. `DecalInstance` keeps its 64-byte size: `darken` takes the old `padding_a`, which was always 0. So no
  shipped decal is moved to `NOWHERE` by stale data. No shipped card sets `FLAG_DARKEN` (bit 8 was unused). No
  golden or lab PNG changed in the diff. `effect_slice_gpu_tests.rs` only learns the new `Circle` placement for its
  margin.
- **Order (decals).** Darkening decals are queued at `f32::MIN` before the other decals. `Transparent3d::sort` is
  `radsort::sort_by_key`, an LSD radix sort, which is stable. So the order "darken, then the others over them" is
  kept, and nothing transparent is drawn before either. This is correct.
- **Order (cards): risk E3-1.** A run's darkening item is queued at the **run's** sort distance (its first card's
  centre), not at the front. A darkening ring therefore multiplies whatever transparent item sorted before it
  where it overlaps on screen. That includes the body of any bubble whose centre sorts as farther than the ring's
  run.
  - **Evidence for the tank itself: no defect.** Its body is not multiplied by the soil. Inside the body the full
    and bare views differ by a factor of about 0.92, neutral in hue (for example 77,79,93 → 71,73,88 at (548, 232)).
    On the ground beside it, the soil's factor is about 0.6 and warm (180,184,185 → 121,113,103 at (518, 262)).
    The body's change is what the bubble's slight translucency lets through from the darkened ground behind it,
    which is correct.
  - **Not evidenced: a neighbour.** A unit standing on the back half of the anchor ring (marines round a sieged
    tank are the common case) has its centre one radius up but up to 1.85 tank radii farther back. Whether it sorts
    before or after the ring's run depends on the pitch and the run's first card. If it sorts before, a darker
    crescent of the ring would be printed across its body.
  - The round-2 premultiplied ring had the same ordering flaw. Multiplication makes it more visible, and the anchor
    ring is **held for as long as the tank is sieged**.
  - Every darkening card is ground-lying (the catalog refuses a darkening standing ring). The fix is therefore
    trivial: queue the darkening card items at `f32::MIN`, like the decals.
- **Cost.** While any darkening card exists (that is, while any tank is sieged), every card run gets a second item
  that runs the vertex stage over all of its instances and then collapses them. This costs no fragments, but it
  doubles the card vertex work and the item count for the rest of the match. It is probably small, but it has not
  been measured (see §4). If E3-1 queues the darkening items at `f32::MIN`, one darkening item per frame drawing
  only the cards that darken would remove most of this cost. That is advisory.

## 4. Frame budget (S-2)

**Adequate for what S-2 asked, with a stated limit.**

| | `main` (2 runs) | `wp8` (2 runs) | Δ |
|---|---|---|---|
| p50 | 12.3, 11.9 → 12.10 | 12.5, 12.4 → 12.45 | +0.35 |
| p95 | 20.8, 19.8 → 20.30 | 19.7, 18.9 → 19.30 | −1.00 |
| Update p50 | 9.35 | 9.45 | +0.10 |

- The p50 gap (+0.35 ms) is smaller than `main`'s own run-to-run gap (0.4 ms). It is far inside the 9.7 to 15.1 ms
  range of the record runs, and the p95 moves the other way. With n = 2 per build, this cannot resolve a change
  below about 0.5 ms at p50 or about 1.5 ms at p95. It does rule out the 0.5 to 2 ms render-thread regression that
  round 2 feared from unconditional unbatching.
- That is the right question for S-2. The round-2 cost came from unbatching 400 neutral bubbles, and the
  conditional form removes it by construction (§3.1). The 400 scenario almost certainly holds no unit that is
  concealed, burrowing or switching mode, so the measurement in effect confirms that ordinary units are back in
  the batched bin. That is what was needed.
- **Not measured:** the darkening pass and unbatched shadows *in use*. The cost is bounded by a few draws per
  concealed unit, plus the doubled card items while a tank is sieged. I accept this without a further run. A later
  `frame_budget` scenario with some sieged and burrowed units in view would close it (advisory).

## 5. The looks (checklist items 1 to 7)

### `burrow_mound`: accept (narrowly)

| Item | Verdict | Reason |
|---|---|---|
| 1 Reads | accept | Pose cell 3 and the battle at (580, 385): the burrowed clip (scorpion in soil) fills the matte disc (97.8 %), with no shadow ("0 burrowed"), and the floor round it is now visibly disturbed. It reads as a unit gone into the ground with earth thrown out, not as a token lying on the floor. |
| 2 Ground | accept | The heaps now show outside the body: mostly below and to the right of it at default zoom, 57.0 / 57.2 % of the ground between 1.05 and 1.6 radii beside the body, and 0 px beyond the ring. B2-1 is met. |
| 4 Cheapness | partial (carried forward) | The heaps are flat. Multiply-only marks have no lit crest, and the `smoke_puff` texture gives them darker ragged rims, so up close they read as dried mud or tea-stain splotches rather than raised heaps. The "hollow lip" does not show as a lip: it merges into the general darkening. This is not a blocker at battle zoom. |
| 5 Colour | accept | Warm brown and earthy, and darker inside the neighbouring shadow, never lighter. |
| 7 Fog | accept | §1. |

The unit without mode media (F2-1, `wp8r3_plain_burrow_pose_*`) is now pictured with its looks. It shows a sunk bead
in a small darkened patch, 24 % of the disc. That is correct.

### `mode_seal_sweep`: accept

| Item | Verdict | Reason |
|---|---|---|
| Arc against progress | accept | 0.472 of the turn at 0.472, both drivers. |
| 1 Reads | accept | `wp8r3_siege_mid_*` and siege t = 1 200 to 2 600 ms: a brown band, irregular in width with darker clods, sweeping round the hull. It reads as a ploughed band rather than a UI ring, but only just (see A3-1 on its edges). |
| 4 Head | accept | The head is now a ragged taper over several pixels, not round 2's 3.6° cut (siege t = 2 200 ms at about 4 o'clock, and the mid shot's lower-right end). The "lit fresh earth" shows only as a lighter fade, because a darkening element cannot lighten. That is acceptable: the cut end was the defect. |
| Shadow | accept | Where the band crosses the hull's shadow (t ≥ 2 600 ms) it is darker than the shadow, never lighter. The anchored check finds 0 lightened pixels within 2.2 radii, on both drivers. I recounted this on the captures as 0 lighter and 2 492 / 2 501 darker by more than 6 levels. |
| Item 4, band edges | note | Across the band, the ground's factor steps from 1.0 to about 0.68 within one window pixel on the outer side, and back from 0.65 to 0.98 within one on the inner side. The band is crisp-edged at default zoom. For a 1.5 s transient this is acceptable, but it is the same element as the held anchor ring (A3-1). |
| 7 Fog | accept | |

### `siege_anchor`: reject

| Item | Verdict | Reason |
|---|---|---|
| B2-3 straight edge | **closed** | No straight edge crosses the shadow, either in `wp8r3_anchored_crop_*` or in the battle at (548, 244). The amplified difference against bare is an unbroken ellipse. The implementer's diagnosis is consistent with the pictures: the edge was the shadow's own flank, revealed by round 2's lightening. |
| Pale smear | **closed** | The whitish `crushed` layer is gone. The inner disc is a warm darkening (factor about 0.5 to 0.65). |
| 1 Reads | **reject** | Settled (battle, anchored crop, siege t ≥ 3 000 ms), the look is a **uniform sepia disc with a crisp, darker circular rim** at about 1.85 radii. A horizontal profile of the full/bare ratio at y = 250 jumps from 1.00 to 0.73 within one pixel at the outer edge, with no fade. The rows at y = 262 and 275 behave the same (1.00 → 0.72, 1.00 → 0.78). The tile seams run through unchanged, and the five `ground_crack` decals are not distinguishable at default or 4× zoom. A hard-edged, evenly tinted round pad held round a unit for the whole siege reads as a selection, aura or range indicator, not as soil the stabilisers pressed down. |
| 4 Cheapness | **reject** | The hard circular boundary (above) is the procedural tell: an evenly round, crisp edge at constant radius. |
| Shadow | accept | It darkens only. 0 px are lightened. |
| 5 Colour | accept | Warm brown. |
| 7 Fog | accept | |

## 6. Regressions in previously accepted looks and in shipped effects

- **`cloak_shimmer`: no regression.** Disc-bounded, colourless, no shadow (§1). The shimmer pose's measurements
  match round 2's.
- **`detected_mark`: no regression.** `wp8r3_battle_detected_*` round (893, 230): the body is crisp. The full/bare
  difference is at most 11 levels near the unit.
- **`burrow_dig`: no regression.** Clods (t = 560 to 800 ms), then the darkened heaps spread out under the dust as
  the body sinks (t ≥ 760 ms), with no single-frame pop. The shadow contracts to the foot. "The burrow's channel
  ended at 1.36 s with the body 77 % sunk" is unchanged.
- **`burrow_emerge`: no regression, and slightly better.** The lingering earth now shows round the body
  (t = 40 to about 1 000 ms) and fades out. The shadow grows back attached (§2). There is no dip.
- **`cloak_own`:** fixed (§2).
- **Shipped splash effects: no regression found.** The reasons are in §3.2: the same items, the same pipelines in
  effect, no stale instance data, and no golden changed. I did not re-run the goldens. That is for the package gate.
- **Art rules.** No rays, stripes, sunbursts, beams or speed lines anywhere in the round-3 material.

## Verdict table

| Look / item | Verdict | Main reason |
|---|---|---|
| Fog (item 7), whole WP | **accept** | 0 px at the burrowed enemy; the cloaked enemy is a seam-only disc with no shadow, on both drivers |
| S-1 shadow shape | **closed** | It shrinks about the sphere's lowest point; dig and emerge shadows stay attached |
| `cloak_own` | **accept** | No detached dot; a faint shadow attached under the veiled body |
| `cloak_shimmer` | **accept** (no regression) | Unchanged |
| `detected_mark` | **accept** (no regression) | Body crisp |
| `burrow_mound` | **accept** (narrowly) | Earth now shows beside the body (57 %); it reads as dug in. The heaps are flat stains with no crest (carried forward) |
| `burrow_dig` | **accept** (no regression) | Clods, then heaps spreading; the shadow contracts to the foot |
| `burrow_emerge` | **accept** (no regression) | Lingering earth visible; shadow regrows attached |
| `mode_seal_sweep` | **accept** | Soft ragged head; darkens over the shadow; the band edges are crisp (A3-1) |
| `siege_anchor` | **reject** | Straight edge and pale smear are fixed, but the held look is a hard-edged, uniformly tinted round pad with a darker rim (a selection or aura language); the cracks do not show |
| S-2 batching, correctness | **accept** | Bevy 0.15.3 rebuilds instances and bins each frame; shell and component change together; the predicate equals the shader's |
| Mixed-bubble test | **sufficient for the static case** | A headless lifecycle test is advisable (E3-2) |
| Darkening pass, correctness | **accept for decals and shipped effects; E3-1 for cards** | Card darkening items sort at the run's distance and can multiply over a neighbour's body |
| Frame budget | **adequate** | +0.35 ms p50 is within `main`'s own spread; the conditional form removes the feared cost by construction. The darkening pass in use is unmeasured (advisory) |
| Shipped effects | **no regression** | |
| **Overall WP8 round 3** | **reject (narrowly)** | One look (`siege_anchor`) and one small ordering fix (E3-1) remain |

## Required changes for round 4

- **A3-1 (`siege_anchor`, held look).** The settled anchor must not read as an outlined disc.
  - Give the ring and the pressed soil a soft outer falloff over at least about 0.15 radii (several pixels at
    default zoom) instead of a one-pixel step.
  - Break the boundary's roundness. Either vary the radius strongly round the turn, or let the held look be
    compaction concentrated where the stabilisers press (a few irregular darker patches or short gouges round the
    hull) rather than an even full annulus. Drop the darker rim band at the edge.
  - Either make the cracks visible at default zoom or remove them.
  - Keep "darkens only" (0 lightened px) and the fog result.
  - Picture the settled tank in the battle and in the anchored crop, and print a radial falloff figure (for
    example, the width in pixels over which the ground's factor goes from 0.95 to 0.75 at the outer edge).

  The same softening of the band edge should apply to `mode_seal_sweep`, which shares the element. That is not a
  separate rejection.
- **E3-1 (darkening card order).** Queue the darkening card items at `f32::MIN` (as the darkening decals are), so a
  ground-lying darkening card never multiplies over a bubble or card drawn before it. Add evidence or a GPU check:
  a neutral unit standing on the back half of an anchored tank's ring keeps its body unchanged (within the
  translucency allowance) against the frame without the looks.

### Advisory (not blocking)

- **E3-2.** A headless test that steps `present_stances` through neutral → cloaked (shadow lane only) → neutral and
  burrowing → burrowed → emerged, and asserts `Has<NoAutomaticBatching> == casts_own_shadow(shell)` on every frame.
- **E3-3.** Once E3-1 lands, draw the darkening cards as one item over only the cards that darken, rather than a
  second item per run over all cards. Or measure a `frame_budget` scenario with sieged and burrowed units in view.
- **`burrow_mound` volume.** A small premultiplied crest (a lit upper edge on a few heaps) over the darkening would
  turn the stains into heaps. Optional.
