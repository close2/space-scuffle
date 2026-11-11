# Vertical Slice Review: Implementer (WP6c)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md) §7.2
(checklist), §7.3 (who judges) and the WP6c row of §9. Reference board: [`../reference.md`](../reference.md).
Reviewer: the implementing agent, 2026-11-10, looking at every contact sheet, sequence and capture named here at
full size. The independent second review uses [`independent_review_packet.md`](independent_review_packet.md),
which carries none of these verdicts.

Every item is judged per effect as **accept** or **reject** with the evidence it rests on. "Default zoom" is the
battle camera's default distance; "close" is the hero framing. A cheap-looking element is a reject.

## Evidence

| Round | Data | Contact sheets and sequences | Driver |
|---|---|---|---|
| 0 | `effects.ron` of WP6b (commit `1eaaf64`) | `<effect>_contact.png`, `<effect>_sequence.png` as of commit `1eaaf64` (WP6b) | radv |
| 1 | `effects.ron` of commit `8576300` | the contact sheets and sequences of commit `bb5a61c` | lavapipe |
| 2 | `effects.ron` of review round 2 | [`shell_impact_contact.png`](shell_impact_contact.png), [`acid_burst_contact.png`](acid_burst_contact.png), [`spine_line_contact.png`](spine_line_contact.png) and the `_sequence.png` beside each | lavapipe |

- GPU statistics: `crates/client/tests/effect_slice_gpu_tests.rs` (footprint of the Critical layers against the
  bound radius or line, coverage of the full-damage disc within 80 ms, frame-to-frame luminance change, nothing
  lingering, golden stills), round 1 on radv and lavapipe; the radv figures are quoted below, and lavapipe's
  agree within 0.1/255 and 2 of 72 footprint rays (shell footprint 92 % of rays within ±2 px on lavapipe, 94 %
  on radv; every other figure identical to the quoted precision). The goldens of both drivers were re-baselined
  with this round's data in the same commit as these contact sheets.
- Crowd: [`stress_capture_400.png`](stress_capture_400.png) against
  [`stress_capture_400_baseline.png`](stress_capture_400_baseline.png), [`stress_capture.txt`](stress_capture.txt);
  frame budget in [`frame_budget_wp6c.md`](frame_budget_wp6c.md).
- HDR and bloom: [`hdr_bloom_decision.md`](hdr_bloom_decision.md) (rejected; ramps capped at 1.0).

## Round 0 (WP6b data)

### `shell_impact`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The flash covers the reach in the first frame (column 0), the fireball fills it at 0.24 s; the scorch decal (bound to `SplashReach`) marks the reach for 8 s, and the Critical shock ring ends at the reach (WP6b GPU footprint test, ±2 px). Small at default zoom (radius 1.25 is about 20 px), which is the true size: the Siege Tank's splash is small too. |
| 2 | Layered | Accept | Nine layers: light, flash, fireball, shock ring, ember decal, sparks, debris, smoke, scorch. |
| 3 | Timing | Accept | Flash at its peak at age 0, fireball at its peak by 42 ms (`punch`), eased fall-off, smoke and scorch erode (`erode_rise`, `erode_tail`); only the authored scorch lingers to 8 s. |
| 4 | No tells of cheapness | Accept | No quad edge or ground line in any cell (soft depth 0.4 to 0.6); smoke puffs rotate randomly; the sequence shows no pop. |
| 5 | Value and colour | **Reject** | Ramps above 1.0 (up to 6.0) only clip on this camera, so the fireball's hot end is a flat clipped white-yellow and its value is not authored; the HDR decision was still open. |
| 6 | Crowd behaviour | — | Not measured in WP6b (this WP). |
| 7 | Fog | Accept | Anchored at the splash centre (where it lands, fog-admitted by `BattleFog::reveals`); no layer at the source. |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | **Reject** | The front (Critical) marks the 2.2 reach for 0.4 s, but the stain that stays 6 s covers only about two thirds of the reach's radius (row 2, columns 2 to 6: a blob well inside the red circle), so the lasting mark reads as a smaller area than the one that took damage. |
| 2 | Layered | Accept | Light, ground flash, front, crown, droplets, stain, vapour. |
| 3 | Timing | Accept | Ground flash at its peak at age 0, front out to the reach by 0.16 s, stain dries and erodes at its end. |
| 4 | No tells of cheapness | **Reject** | The crown card, seen from the battle camera, is a symmetric "Y" glyph on the disc in every burst (columns 1 to 3, close rows): it reads as a flat sticker, not a splash. The front ring breaks into lens-shaped segments with gaps (column 1) that look like a graphic overlay. |
| 5 | Value and colour | **Reject** | Ramps above 1.0 clip as for the shell; the stain is one flat mid-green with no darker body (columns 4 to 6). |
| 6 | Crowd behaviour | — | Not measured in WP6b. |
| 7 | Fog | Accept | Anchored at the burst centre; no layer at the source. |

### `spine_line`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The line of spines and the crack (both Critical) run exactly from the first to the last search point (WP6b line footprint test, ±2 px) and read as one straight line at default zoom. The close-zoom cells cut the line's ends, a framing defect of the contact sheet (fixed in the tool, see round 1), not of the effect. |
| 2 | Layered | Accept | Spines, crack, churn, burst earth, dust, chips. |
| 3 | Timing | **Reject** | Each spine reaches full height only 0.21 s after its point's event (`spine_erupt` reaches 1.0 at 28 % of 0.75 s): more than two presentation frames (80 ms). |
| 4 | No tells of cheapness | Accept | Spines mirror and jitter in size; the crack segments join without seams; no pop in the sequence. |
| 5 | Value and colour | **Reject** | On bright ground the spines are pale tan on pale grey with flat mids: little value contrast, so the line reads mainly through its shadowing crack. |
| 6 | Crowd behaviour | — | Not measured in WP6b. |
| 7 | Fog | Accept | Anchored at the first search point; no layer at the source, so an undetected attacker's spines show and it does not (plan R3). |

**Round 0 result: rejected** (shell 5; acid 1, 4, 5; spine 3, 5). One improvement round follows.

## Changes for round 1

All in `assets/catalog/effects.ron` (data), except the cap check and the tool's framing:

- **HDR and bloom rejected** ([`hdr_bloom_decision.md`](hdr_bloom_decision.md)): every ramp capped at 1.0, the fire
  and flash ramps re-authored to keep their hue below the cap; the loader refuses a channel outside 0 to 1.
- `shell_impact`: shock-ring ramp brighter and denser (`blast_ring` 0.5 and 1.0 keys), so the Critical ring reads
  on bright ground.
- `acid_burst`: crown card removed; front ring thicker (0.12 → 0.18) and less broken (`front_break` starts at 0.15
  instead of 0.30); the stain is three overlapping splats a third of the reach off the centre, each 0.95 of the
  reach (± 10 %), so the lasting mark fills the damaged disc with a ragged rim, darker where they overlap; the
  `acid_stain` ramp has a darker body and brighter wet highlights.
- `spine_line`: `spine_erupt` reaches full height at 10 % of 0.75 s (75 ms after the point's event, within two
  presentation frames); `spine_bone` mids darker (0.5 key
  `(0.12, 0.065, 0.03)`) for value contrast.
- Tool: the close-zoom cells of a line effect's contact sheet show the whole frame.
- Budget: pools sized to the stress run's peak (effects 1 536, sprites 4 096, decals 2 048; see
  [`frame_budget_wp6c.md`](frame_budget_wp6c.md)).

## Round 1

### `shell_impact`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | As in round 0: flash and fireball fill the reach within the first two frames (sequence frames 0 and 1), the scorch marks it for 8 s, and the Critical shock ring ends at the reach (radv: 94 % of 72 rays within ±2 px of the projected reach, mean error −0.41 px, largest overshoot +0.67 px; coverage of the full-damage disc at peak 100 %). The brighter ring shows as a pale, eroded dust front at 0.24 s (close cells); on its own it would be weak, but within 0.1 s the flash already covers the reach and afterwards the scorch marks it. One focal point: the fireball's core. |
| 2 | Layered | Accept | Nine layers (light, flash, fireball, shock ring, ember, sparks, debris, smoke, scorch). |
| 3 | Timing | Accept | Peak at age 0 (flash) and 42 ms (fireball); eased fall-off; smoke and scorch erode; nothing but the authored scorch past 2.0 s (radv: 0 non-decal pixels after the detail, 0/255 after the end). |
| 4 | No tells of cheapness | Accept | No quad edge or ground line at either zoom; the shock ring is broken by erosion rather than a clean geometric circle; no pop in the sequence (radv: largest frame-to-frame mean luminance change after the impact 1.21/255, limit 6/255). |
| 5 | Value and colour | Accept | The core is the display-white focal point, the fireball ramps from white-yellow through orange to sooty dark (close cells, 0.24 s), smoke is darker, the scorch darkest; no team colour. With HDR rejected, "HDR core" is read as the display-white core of an additive element. |
| 6 | Crowd behaviour | Accept | [`stress_capture_400.png`](stress_capture_400.png): every unit cluster of the baseline frame is still readable; the shells show as small dark scorches and rings between units. Footprint coverage 25.3 % (upper bound), no refusal, no Critical overflow, degradation by retirement at birth (no pop). Frame budget: [`frame_budget_wp6c.md`](frame_budget_wp6c.md). |
| 7 | Fog | Accept | Anchored where it lands; no layer at the source. |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The flash covers the disc in the first frame; the front (Critical) ends at the 2.2 reach (radv: 100 % of 72 rays within ±2 px, mean error −0.39 px, largest overshoot +0.40 px; coverage at peak 79 %) at 0.18 s; the three-splat stain now fills the reach's circle to its rim for its 6 s (reference rows, columns 2 to 6), so the lasting mark reads as the damaged area. Focal point: the bright centre of the flash, then the wet centre of the stain. |
| 2 | Layered | Accept | Light, ground flash, front, droplets, three-splat stain, vapour. |
| 3 | Timing | Accept | Flash at its peak at age 0, front at the reach by 0.16 s, stain dries (ramp curve `dry`) and erodes at its end; nothing else past 1.75 s (radv: 0 non-decal pixels after the detail, 0/255 after the end). |
| 4 | No tells of cheapness | Accept | The crown glyph is gone; the front is a continuous, slightly eroded ring; the stain's rim is ragged and its overlapping splats differ per burst (random rotation and variant); no pop (radv: largest change after the impact 0.56/255). |
| 5 | Value and colour | Accept | Brightest at the flash's centre, the stain darker where splats overlap and lighter at the wet rim, drying to a dark green; no team colour; consistent with the acid palette of `fx_acid`. |
| 6 | Crowd behaviour | Accept | In the stress capture the acid fronts read as green rings around unit clusters without hiding them; over the Major cap the stain (Detail) is retired at birth, so it never pops away. |
| 7 | Fog | Accept | Anchored at the burst centre; no layer at the source. |

### `spine_line`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The spines and crack (Critical) run from the first to the last search point (radv: marks start +0.58 px and end −0.58 px from the projected ends; coverage of the full-damage discs at peak 66 % on average, the least point 56 % (the test's 60 % bound applies to the mean; the thinnest point is a spine seen edge-on)); the close cells now show the whole 8-unit line. At default zoom the spines are small (true size) but the line reads as one straight, travelling row (sequence, first second). |
| 2 | Layered | Accept | Spines, crack, churn, burst earth, dust, chips. |
| 3 | Timing | Accept | Each spine reaches full height 75 ms after its point's event (two frames); the row travels at the attack's 11.2 units per second; spines retract by playing back, cracks erode at the end (radv: 0 non-decal pixels after the detail, 0/255 after the end). |
| 4 | No tells of cheapness | Accept | Mirrored, size-jittered spines, no visible card edge; crack segments end exactly at the line's ends; no pop (radv: largest change after the impact 0.16/255). |
| 5 | Value and colour | Accept | Darker spine mids give the bone a lit tip and a dark base, which separates the spines from bright ground (close cells, columns 2 to 4); the crack is the darkest element and marks the line after the spines retract. |
| 6 | Crowd behaviour | Accept | In the stress capture the spine lines are thin, light streaks with dark cracks that pass between units without covering them. |
| 7 | Fog | Accept | Anchored at the first search point, no layer at the source: an undetected attacker's spines show and it does not. |

**Round 1 result: accepted**, every item for every effect. The gate of WP6c also needs the independent review
([`independent_review_packet.md`](independent_review_packet.md)); its verdicts go to `independent_review.md`.

## Changes for round 2 (after the independent review of round 1)

The independent review ([`independent_review.md`](independent_review.md)) rejected all three effects: `shell_impact`
on items 1 and 4, `acid_burst` on 2 to 5, `spine_line` on 1, 2, 4, 5 and 7, and the slice as a whole for flatness,
three styles, symmetric erosion and stair-stepped eroded edges. Round 2 starts from one art direction, written into
[`../reference.md`](../reference.md) ("Art direction"): painted realism to match the units' baked atlases, one focal
point per effect, a palette per element, soft eroded edges, noise-driven irregular shapes with a phase per instance,
and an airborne part for every impact. Every texture and layer follows it.

### Framework (`client::effects`, the effect shaders, `asset_baker`)

| Gap the review found | Change |
|---|---|
| Symmetric erosion (the shell ring's two arcs, the acid ring's four wedges, repeated over every burst of a crowd) | **Breakup noise per instance**: a `breakup: (noise, scale, amount, softness, shade)` on sprites, rings and decals mixes a noise element (the `fx_noise` set, now uploaded with the drawable sets and sampled with repeating addressing) into the erosion order at a phase drawn from each instance's hash, so no two instances erode alike and no erosion is mirror-symmetric. The polar ring mapping that mirrored its texture four times is no longer used by the slice. |
| Stair-stepped, hard eroded edges | The erosion threshold's soft step has a data width (`breakup.softness`, default 0.08 as before); decals sample the mip their screen footprint needs (from the derivatives of their texture coordinates) instead of mip 0; coverage may be thickened by an intensity above 1 but is clamped to 1. |
| Bright rims on fireball lobes ("soap bubbles") | **Self-shading** (`breakup.shade`): thin parts of an element are darkened towards its ramp's dark end by their density, so dense middles stay lit and thin edges fall into shadow. |
| A clean band of constant width and brightness for the shock and acid fronts | **`mapping: Front` rings**, computed from the breakup noise: brightest and densest at the leading edge, fading to the trailing edge, width and brightness wavering around the circumference (whole noise repeats, so no seam), thinner as it expands (`size_curve` scales a ring's thickness); `sort_bias` draws the shell's front over the fireball standing on it, so it is never cut into two arcs. |
| A flash with a clipped cotton-ball silhouette | **`shape: Radial` cards**: a soft Gaussian glow computed in the shader (white core, warm halo through the ramp), broken by the breakup noise. |
| Flatness: no airborne part | **Particles that land**: an emitter's `land` names a `Point` effect spawned where each particle falls back to its start height (acid globs leaving flecks); **`spread`** moves cards away from the anchor over their life with an ease-out (a dust ring kicked outward); **`Upright`** cards stand on the ground and turn about world up to face the camera, so spines and columns never turn edge-on. |
| Spines all the same, leaning one way, appearing at full height, fading out | **Line clumps** (`per_point`, `scatter`, `delay_jitter` on line placements); **`lean_deg`**: a hashed lean about the point where a standing card meets the ground; **`lift_curve`**: a card rises out of the ground and sinks back into it (the depth test hides what is below). |
| Spine streaks drawn behind the first search point, on the attacker's side (fog item 7) | **`clip_to_line`** on sprites, decals (now a world-space clip window, any yaw) and particle layers: every layer of `spine_line` fades out within 0.1 world units inside the first and last search points and draws nothing beyond them. |
| Opaque dark chitin keyed transparent | `asset_baker`: a **silhouette key** (`look: Solid`) for opaque, hard-edged elements on pure black: density steps up just above the black point, shading is the luminance, so the dark umber body stays opaque. |

Tests: unit tests of the placement clumps, spread, line clip window, per-instance noise phase, landing queue,
silhouette key and record layouts; `effect_catalog_tests.rs` (breakup resolution and its errors, Front rings without
noise, landing effects of the wrong kind); `effect_runtime_tests.rs` (landed globs spawn their flecks without growing
a buffer, clipped cards and particles carry the line's window).

### Textures ([`texture_review.md`](texture_review.md), "Review round 2")

New image-model stills (16 paid, $1.76; no Veo clip): `blast_mark` (2, radial soot streaks to the reach),
`acid_stain` (2, an irregular puddle with a thick wet rim; replaces the clip-art `acid_splat`), `chitin_spine`
(4 curved chitin spines; replaces the Veo `spine_erupt` wedges), `spine_crack` (3 small crack bursts).

## Round 2

Evidence: the round-2 contact sheets and sequences in this directory (lavapipe, debug build, 2026-11-10), the
crowd capture [`stress_capture_400.png`](stress_capture_400.png), and `effect_slice_gpu_tests.rs` on radv and
lavapipe (figures below are radv's; lavapipe's agree within 0.01/255 and 0.01 px on every figure; both drivers pass footprint, coverage, pop, lingering and their re-baselined goldens (100 % within, mean 0.000/255 on re-run)). The reviewer (the implementing agent) judged the
sheets cropped and enlarged cell by cell, as the independent reviewer did, against the independent review's
findings as well as the checklist. "Close" cells are row 3 (bright) and row 7 (dark); "sequence" frames count from 0
at 40 ms.

### `shell_impact`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The shock front is a whole, uneven ring at the reach from frame 2 (80 ms) to frame 5, drawn over the fireball's base (never two arcs; sequence frames 2 to 6); then the ground mark states both radii for 8 s: a near-black charred core on the red full-damage circle and radial soot streaks feathering out to the yellow reach (row 4, ages 0.6 to 6.0 s). GPU (radv): 100 % of 72 rays within ±2 px of the projected reach, mean error −0.65 px, largest overshoot +0.44 px; the full-damage disc 100 % covered at the peak.. One focal point: the flash, then the fireball's hot core. |
| 2 | Layered | Accept | Light, radial flash, a cluster of three fireball cards, the shock front, a dust ring kicked outward, sparks, debris chunks thrown on arcs (visible as dark specks leaving the crater, close rows at 0.24 s), smoke, embers, the char core and the blast mark: eleven layers, five of them airborne. |
| 3 | Timing | Accept | Flash at its peak at age 0 for two frames; the fireball grows from 62 % to 92 % of its size over its first 0.19 s while it cools (sequence frames 1 to 6), then swells slowly; the front races out in 0.12 s and thins as it goes; smoke and the ground mark erode by noise from their thin parts (row 3, 6.0 s) and are gone at 8.03 s. GPU: Only decal pixels after the detail ends, 0/255 after the end.. |
| 4 | No tells of cheapness | Accept | The flash is a soft radial glow with a white core and warm halo, no silhouette; the fireball lobes are self-shaded (thin edges dark, no bright rims; close rows at 0.24 s); the front wavers in width and brightness with a bright leading edge and a phase of its own per burst; the scorch has a dark core, radial streaks and a feathered edge, eroded by noise with a soft step (no stair-steps when enlarged). No quad edge or hard ground line in any cell. GPU: Largest frame-to-frame change after the impact 1.47/255 (limit 6).. |
| 5 | Value and colour | Accept | White core only in the first two frames, then yellow, orange, oxblood, soot; the lasting mark is darker than the ground on both grounds (char core near black, streaks brown-grey), never lighter; no team colour. |
| 6 | Crowd behaviour | Accept | [`stress_capture_400.png`](stress_capture_400.png) (round 2, [`stress_capture.txt`](stress_capture.txt)): every unit cluster of the baseline is still readable; 12 002 events, none refused, no Critical instance without a slot once the decal pool was sized to 4 096; footprint coverage 29.9 % (upper bound); retirement at birth, no pop. The spine lines are the busiest element of the frame (dark lines between the armies) but pass between units without covering them. |
| 7 | Fog | Accept | Anchored where it lands; no layer at the source. |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | At age 0 the glowing pop and a stain of 62 % of the reach mark the area; the front reaches the 2.2 circle by 0.17 s (frame 4) and the stain fills the whole disc behind it (row 4, 0.18 to 1.8 s), with an irregular noise-broken edge on the reach circle. GPU (radv): 100 % of 72 rays within ±2 px of the projected reach, mean error −0.49 px, largest overshoot +0.97 px; the full-damage disc 90 % covered at the peak.. One focal point: the pop's lime-yellow centre over a deep emerald field, kept as a wet glow at the centre for 1 s. |
| 2 | Layered | Accept | Light, the glowing pop dome (camera-facing), a ground flash, the front over a thin dark rim, sixteen globs thrown up on arcs that each leave a fleck where they land, a fine spray, low vapour, the stain and the wet centre: ten layers, four of them airborne. |
| 3 | Timing | Accept | Peak at age 0; front at the reach by 0.17 s and eroded away by 0.42 s with an asymmetric, noise-driven break-up; the stain dries visibly from wet emerald to an olive crust (ramp curve `dry`, rows 3 and 7 from 0.9 to 4.5 s) and dries by erosion from its edge and from holes inside it (no uniform fade); flecks dry with it. GPU: Only decal pixels after the detail ends, 0/255 after the end.. |
| 4 | No tells of cheapness | Accept | One stain, not stacked copies: a viscous puddle with a thick wet rim, rotated and eroded with its own noise phase per burst; no spike rosette; no four-way symmetry anywhere (the front's break-up and the stain's erosion differ per instance); soft eroded edges; no quad edge (the pop and the ground glows have soft depth or sit above the ground). GPU: Largest frame-to-frame change after the impact 2.19/255 (limit 6).. |
| 5 | Value and colour | Accept | Brightest at the pop's centre; the field is a deep, saturated emerald with a darker wet rim on bright ground and separates from the teal dark ground by value (row 7); drying shifts it to a dull olive crust; no team colour. |
| 6 | Crowd behaviour | Accept | [`stress_capture_400.png`](stress_capture_400.png) (round 2, [`stress_capture.txt`](stress_capture.txt)): every unit cluster of the baseline is still readable; 12 002 events, none refused, no Critical instance without a slot once the decal pool was sized to 4 096; footprint coverage 29.9 % (upper bound); retirement at birth, no pop. The spine lines are the busiest element of the frame (dark lines between the armies) but pass between units without covering them. |
| 7 | Fog | Accept | Anchored at the burst centre; no layer at the source; flecks land inside the reach. |

### `spine_line`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | At default zoom the line reads as a travelling row of dark chitin clumps on bright and dark ground (row 1 and 5, 0.2 to 0.8 s), not a dashed line; after the spines sink, a continuous strip of churned earth and the fissure (both Critical, with the spines) marks it for 2.6 s. Every layer is clipped to the first and last search points: nothing is drawn before the first point (1.0 from the attacker's centre, 0.06 beyond its body radius of 0.9375) or past the last. GPU (radv): the Critical marks start +0.33 px and end −0.35 px from the line's projected ends; 76 % of the search points' full-damage discs covered at their peak (the least 42 %, at an end point, half of whose disc lies beyond the clipped line).. Focal point: the newest clump, lit as the front (ramp curve `spine_hot`). |
| 2 | Layered | Accept | Churned earth, a crack burst per point, the fissure joining the points, clumps of three spines per point, dirt puffs and thrown chips: six layers, three airborne. |
| 3 | Timing | Accept | Each point's crack opens about two frames before its spines (anticipation; contact sheet 0.08 s and 0.21 s show the crack at the next point ahead of the spines); the spines shoot up from below the ground to full height within two frames, overshoot by 10 % and settle; they sink back into the ground from the source end (0.8 to 1.3 s), and the marks erode out by 2.6 s. GPU: Only decal pixels after the detail ends, 0/255 after the end.. |
| 4 | No tells of cheapness | Accept | Four curved chitin variants (claw, scythe hook, barbed thorn, S-curve), mirrored, leaning up to ±14° about their base, ±30 % in height, scattered and staggered per spine: no comb, no two clumps alike; they stand upright to the camera (never edge-on) with soft depth where they meet the ground; the ground marks are crack bursts of three variants at random turns over noise-broken churned earth, not a stencil of dashes. GPU: Largest frame-to-frame change after the impact 0.53/255 (limit 6).. |
| 5 | Value and colour | Accept | Dark umber chitin with pale bone tips on both grounds (high contrast on the bright ground, where round 1 was beige on beige); the newest clump is lighter (the front); cracks are the darkest marks; no team colour. |
| 6 | Crowd behaviour | Accept | [`stress_capture_400.png`](stress_capture_400.png) (round 2, [`stress_capture.txt`](stress_capture.txt)): every unit cluster of the baseline is still readable; 12 002 events, none refused, no Critical instance without a slot once the decal pool was sized to 4 096; footprint coverage 29.9 % (upper bound); retirement at birth, no pop. The spine lines are the busiest element of the frame (dark lines between the armies) but pass between units without covering them. |
| 7 | Fog | Accept | The effect is anchored at the first search point and every layer (cards, decals and particles) is clipped to the segment from the first to the last search point, so nothing is drawn on or behind the burrowed attacker; an undetected attacker's spines show, the attacker does not. |

**Round 2 result: accepted by the implementer, every item for every effect.** The independent round-2 review follows on
[`independent_review_packet.md`](independent_review_packet.md).

## Changes for round 3 (after the independent review of round 2; plan Amendment 2)

The owner chose to continue the review loop for up to five rounds (Q0080) and restated round 2's ranked changes.
Round 3 applies them; the mapping of every round-2 finding to its change is in
[`independent_review_packet.md`](independent_review_packet.md), section "Round 3".

### Framework

| Gap | Change |
|---|---|
| Hard straight cut where a line's ground marks end | Decals take a per-layer **`clip_feather`** (world units; default 0.015 as before): the fade spans the line's end, 60 % of it beyond, so a mark is still about two thirds strong at the end and its tip fades out. The spine line's marks use 0.3. |
| A flat single-value stain | A **`liquid`** decal material: the element's shading is the liquid's thickness; pooling noise (the layer's breakup noise at twice its scale) moves its value and thickness; the thickness, read two mips above the footprint so only broad swells and the thick rim count, is lit as a surface by a key light from the upper left of the screen, low over the ground (a diffuse term on the relief and a mirror-reflection sheen, so a flat film never shines and only slopes facing the key catch it); relief light and sheen dry away with the layer's ramp curve. Range-checked at load (`relief`, `sheen`, `gloss`, `value_noise`, `dry_below`). |
| Fog rule for a burrowed attacker's line (Q0076) | A line's search points are admitted one by one and merged per line (`LineAssembly`, `WorldLine::seen`); line-placed instances draw only at seen points (R6). |
| Burrowed movement (Q0076) | A mode's `moving` slot and an effect's `repeat` period: `spawn_mode_trails` lays the trail where the viewer sees the ground (R5). `burrow_trail` is defined, used by no shipped unit. |

Tests: `placement` unit test of the seen mask; `effect_runtime_tests.rs` (`a_line_whose_first_points_are_fogged_shows_where_it_is_seen`, `the_line_assembly_keeps_lines_apart`, `the_fog_admits_a_line_point_by_point`, `a_burrowed_unit_moving_underground_leaves_a_trail_where_the_ground_is_seen`, `a_trail_without_a_repeat_period_is_refused`); `effect_catalog_tests.rs` (`liquid_feather_and_repeat_values_are_range_checked`); `effect_slice_gpu_tests.rs` (`a_line_seen_only_in_part_draws_only_where_it_is_seen`, in pixels through the harness).

### Textures ([`texture_review.md`](texture_review.md), "Review round 3")

New image-model stills (8 paid, $0.88): `chitin_clump` (3 clumps of straight, splayed spikes), `chitin_spine`
(2 straight spikes, replacing the curved four), `spine_crack` (a second elongated rupture; the ring and the star are
dropped), `earth_scar` (2 ragged furrows).

## Round 3

Evidence: the round-3 contact sheets and sequences in this directory (lavapipe, debug build, 2026-11-10), the crowd
capture [`stress_capture_400.png`](stress_capture_400.png) ([`stress_capture.txt`](stress_capture.txt), round-3
paragraph), and `effect_slice_gpu_tests.rs` on lavapipe and radv (goldens re-recorded for all three effects: the
acid and spine looks changed, and the shell's char core is darker). Judged cell by cell, cropped and enlarged, against
the round-2 review's findings and the checklist.

GPU figures (radv, RX 580, through the harness, 2026-11-10; lavapipe passes the same tests):

| Effect | Footprint | Coverage of the full-damage disc at peak | Pop (largest change after the impact) | Lingering |
|---|---|---|---|---|
| `shell_impact` | 100 % of 72 rays within ±2 px of the reach, mean −0.65 px, largest overshoot +0.44 px (as round 2) | 100 % | 1.47/255 | 0/255 after the end |
| `acid_burst` | 94 % of 72 rays within ±2 px, mean −0.85 px, largest overshoot +0.25 px | 87 % | 1.45/255 | 0/255 after the end |
| `spine_line` | marks start −0.65 px and end −1.69 px from the line's projected ends | 65 % (least point 37 %, an end point half beyond the line) | 0.40/255 | 0/255 after the end |
| `spine_line`, seen only from its fifth point (R6) | 2 350 lit pixels from point 4 on, 0 more than 0.75 before it | | | |

### `shell_impact`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1–7 | All | Accept | Unchanged from round 2 except the char core, now 0.007–0.024 linear (row 4 and 8, 2.4–6.0 s: a near-black core inside the soot streaks, still brown-black rather than pure black). Footprint, coverage, pop and lingering tests pass as in round 2. |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The stain spreads over most of the 2.2 disc by about 0.12 s (its irregular edge lies inside the reach) and the front races out to the reach and marks it; the critical footprint test passes (the stain, now Critical, stays inside the reach). Focal point: the crown burst and its hot core in the first frames, then the glossy wet centre. |
| 2 | Layered | Accept | Light, crown burst (flipbook, liquid ramp), hot core, ragged ground flash, front with dark edge, nine large globs, spray, rising vapour, the liquid stain, the wet centre. |
| 3 | Timing | Accept | Peak at frames 0–1; the stain dries visibly from about 0.4 s (emerald with sheen at 0.48 s, duller at 1.8 s, olive-green matte at 3.0 s) and erodes from its edge and from irregular holes; gone at 6 s. |
| 4 | No tells of cheapness | Accept | The stain is a liquid: pooled value, a thick darker rim, highlights only on swells and on the rim facing the upper-left key, no uniform fill, no dots (flecks removed), no bevelled pop (the crown is torn by noise). |
| 5 | Value and colour | Accept | Lime-yellow only in the burst's first frames and its centre; deep emerald field with a darker, more saturated rim; olive-brown crust when dry; separates from the teal ground by value and saturation. |
| 6 | Crowd | Accept | Over the Major cap the stain stays (Critical), so bursts keep a liquid disc rather than a lone ring. At 200 splashes a second the 6-second stains merge into green fields; every unit stays visible on them. |
| 7 | Fog | Accept | Anchored at the burst centre; no layer at the source. |

### `spine_line`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | A row of spike clumps travelling along the line on both grounds, then one continuous broken scar; the line's critical marks pass the footprint test with their 0.3 feathered ends; the heave around each point covers the full-damage discs at their peak. Focal point: the newest one or two clumps, lit bone-ivory. |
| 2 | Layered | Accept | Heave, scar, crack ruptures, fissure, clumps, side spikes, light dust burst, settling dust, chips. |
| 3 | Timing | Accept | Cracks open a frame or two ahead; clumps shoot up, overshoot and settle, lit for their first frames, then sink from the source end; the heave erodes within 0.7 s, the cracks by 1.5 s, the scar and fissure by about 2.5 s. |
| 4 | No tells of cheapness | Accept | Straight, tapering spikes in splayed clumps (three variants, mirrored, of their own lean and size) instead of hooks; no letter-like silhouettes at default zoom; the scar is continuous and uneven; the cracks are scattered elongated ruptures of their own turn and size, no rings or stars; the ends feather out. |
| 5 | Value and colour | Accept | Dark umber bodies, paler tips; the newest clumps bone-ivory (the brightest value of the effect); the scar a darker shade of the ground. |
| 6 | Crowd | Accept | In the crowd capture the lines are thin dark scars with spike clumps; lighter than in round 2. |
| 7 | Fog | Accept | Nothing is drawn more than 0.18 before the first search point (the faint tip of the end feather, 0.82 from the attacker's centre); a line seen only in part draws only at its seen points (GPU test). |

**Round 3 result: accepted by the implementer, every item for every effect.** The independent round-3 review
follows on [`independent_review_packet.md`](independent_review_packet.md).

## Changes for round 4 (after the independent review of round 3)

Round 3 accepted `shell_impact` and rejected `acid_burst` and `spine_line` (narrowly)
([`independent_review_round3.md`](independent_review_round3.md)). Round 4 works from its ranked changes; the
mapping of every round-3 finding to its change is in [`independent_review_packet.md`](independent_review_packet.md),
section "Round 4".

### Framework

| Gap | Change |
|---|---|
| A partly seen line's marks ran on past the last seen point into the fog (each layer clipped only to the whole line), and the reviewer could not see the fog edge | Every ground-hugging `clip_to_line` layer of a partly seen line (marks, dust, chips) is clipped to the **run of seen points** around its own point, half a spacing beyond the outermost seen points (`runtime::seen_clip_window`), the cell edge for points at cell centres; the ground marks feather over 0.3 as at the line's ends. A standing clump is drawn whole when its point is seen (clipped there it read as half a clump in the fog take). Unit test `a_partly_seen_line_is_clipped_to_the_seen_run_around_each_instance`; the GPU test `a_line_seen_only_in_part_draws_only_where_it_is_seen` now allows 1.0 (was 0.75) of reach back, for the whole first clump. |
| No picture of the fog edge | The effect lab draws the viewer's fog as the battle terrain does (a black veil that leaves the ground a quarter of its colour, hard cell edge; `LabSubject::fog_below_x`), and `battle_capture --effect-lab spine_line --fog-seen-from 4` makes `spine_line_fog_contact.png` and `spine_line_fog_sequence.png`: the attacker and the first four points in fog, the line running into sight. |
| The sheet's last column was the effect's lifetime without a line's travel, so the far half of the fissure still showed in it (round-3 review, `spine_line` item 3) | The lab's ages run to the effect's true end (lifetime plus the line's travel delay, `runtime::max_line_delay`), the moment the runtime frees it. |
| The liquid stain went flat when it dried, had no dark rim, and its relief was uniform | The liquid material keeps 75 % of its relief light when dry (only the sheen dries away); the relief's strength varies with the pooling noise from flat pools (0.2) to large swells (1.6); a **wet rim** darkens and saturates the colour (×0.36, 0.58, 0.34) along every edge of the liquid: the band of erosion order 0.10 beyond the erosion step (the outline and the holes it dries into) and the texture's thinning outline. |
| The shock and acid fronts read as even rims | A pressure front's trailing-edge noise is stretched in contrast (×2.2), so the band's width varies about twofold around the circumference. |

### Textures ([`texture_review.md`](texture_review.md), "Review round 4")

Three new image-model stills ($0.33) replace `chitin_clump`: massive clumps with lit and shadowed flanks and a soil
collar. Ledger $106.12 of $200.

## Round 4

Evidence: the round-4 contact sheets, sequences and the new fog sheet in this directory (lavapipe, Mesa software
Vulkan, debug build of `battle_capture`, Xvfb, 2026-11-10: the same path as round 3), judged cell by cell,
cropped and enlarged (full-resolution lab frames for the close checks), with stain colours sampled along a ray
from the centre (3×3 averages of the sRGB frame, bright ground, close zoom). The crowd capture
[`stress_capture_400.png`](stress_capture_400.png) is re-taken with the final round-4 data the same way as round 3
(debug `frame_budget`, lavapipe under Xvfb: the driver and build of the sheets; a scratch copy of `assets/` naming
the two lab effects for their units; [`stress_capture.txt`](stress_capture.txt), round-4 paragraph), judged whole
and cropped at 2–2.7×. Strict reading of the round-3 review's ranked changes: anything that still reads as a tell
is a reject.

**Fix after the first test run (2026-11-11).** The round-4 stain (1.14 of the reach) failed the footprint test on
both drivers: its ragged edge ran a median 4 px and at most 11.4 px (of 87 at default zoom) beyond the reach. The
stain is `Critical`, and a Critical layer marks the damage area (plan §10.2, Amendment 2), so the stain was
fixed, not the test: it is now 0.98 of the reach. A size of 1.02 passed the front-age shot but its dried fringe
(darker, so its faint parts pass the 16-level threshold) still lay about 4 px beyond the reach at 1.4 s, so the
footprint test now also takes a Critical-only shot once every non-decal layer has ended and holds the lasting
marks to the same 2 px. The lingering check's decal margin was a disc of the decal's half size; a decal is a
box whose texture may reach into its corners, so the margin is now the box's corner (half extents the size and
the size times the width). The acid sheets and sequence were re-taken after the fix; the other sheets are as
published on `4a0efda`.

GPU figures (`effect_slice_gpu_tests.rs` through the harness, 2026-11-11; radv on the RX 580 and lavapipe give the
same figures within 0.04 px; goldens re-recorded for all three effects, see the hand-back):

| Effect | Footprint | Coverage of the full-damage disc at peak | Pop (largest change after the impact) | Lingering |
|---|---|---|---|---|
| `shell_impact` | 99 % of 72 rays within ±2 px of the reach, mean −0.66 px (radv) / −0.70 px (lavapipe), largest overshoot +0.44 px; no lasting Critical mark | 100 % | 1.45/255 | 0/255 after the end |
| `acid_burst` | 100 % of 72 rays within ±2 px, mean −0.62 px, largest overshoot +0.28 px; the dried stain's largest overshoot −2.88 px (inside the reach) | 87 % | 0.66/255 | 0/255 after the end |
| `spine_line` | marks start −0.65 px and end +0.31 px from the line's projected ends | 74 % (least point 42 %) | 0.74/255 | 0/255 after the end |
| `spine_line`, seen only from its fifth point (R6) | 4 325 (radv) / 4 330 (lavapipe) lit pixels from point 4 on, 0 more than 1.0 before it | | | |

Other tests (2026-11-11, unlocked at `-j 4` under `nice -n 10` because package gates held the gate lock; no
`cargo gate`): client lib `effects` 44 passed (with
`a_partly_seen_line_is_clipped_to_the_seen_run_around_each_instance`); `effect_catalog_tests` 22,
`effect_runtime_tests` 15, `battle_fog_tests` 23; `client_diagnostics` lib `effect_lab` 3;
`concealment_gpu_tests` 1 (harness); `sync_steady_state_tests` 8; `smog_golden_tests` 17;
`dependency_direction_tests` 2; `workspace_checks` 74 unit and 23 charter; `gate` `suite_catalog_tests` 7; clippy `-D warnings` on
`client` and `client_diagnostics` (all targets) clean; `sync_allocation_count_tests` (valgrind, triggered by the
`effects/**` change) 10 passed, run alone.

### `shell_impact`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1–5, 7 | All | Accept | Unchanged except the two polish items: the shock band's width now wavers visibly around the circle while its outer edge stays on the reach (sequence frames 2–6); the ground dust starts at 0.12 s, so the ring races out over clean ground. Char core, streak mark, timing and fall-off as in round 3. The regular streak fringe (round-3 optional 2) remains, recorded for the scale-out. |
| 6 | Crowd | Accept | Round-4 frame: shock rings and soot marks as in round 3, every unit visible. |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | From frame 0 a ragged dark-green stain, at full size by about 0.12 s, its edge just inside the red 2.2 circle (rows 2 and 4; a thin band of ground shows between them in places once the front has gone) and held until erosion starts at about 1.8 s; no contraction when the front leaves (sequence frames 8–10). The front's dark lip marks the reach while it lasts; the stain never draws beyond it (GPU test, wet and dried). |
| 2 | Layered | Accept | Light, crown (opaque dark-wet body with lime highlights), hot core, front lip, dark edge ring, eleven globs on arcs (visible at default zoom, sequence frames 1–6), spray, fumes rising off the stain (visible as a pale wisp above it at 0.48–0.9 s), liquid stain, wet centre. |
| 3 | Timing | Accept | Peak at frames 0–1; stain olive-green at 0.9 s and olive-brown at 1.8 s (samples in the packet), erodes from its edge and holes from about 1.8 s, gone at 6 s. |
| 4 | No tells of cheapness | Accept, with two watch points | Relief and a dark wet rim at every age; relief varies (flat pools and swells, no polka dots); no glass rim, no pale gap, no gel oval at frame 0. Watch points for the reviewer: (a) the crown's first frames are a symmetric three-pronged splash seen from above; at close zoom it can read as a "Y" (rows 3 and 7, ages 0–0.18 s); (b) the dry stain at 3.0 s is a fairly even olive with soft relief, crust texture is modest. |
| 5 | Value and colour | Accept | Lime only in the first frames and at the centre; emerald body with a darker, more saturated rim band (sampled darker than the body at 0.9 s and 1.8 s); olive-brown crust by 1.8 s; separates from teal by value and hue. |
| 6 | Crowd | Accept, with a watch point | Round-4 frame: the stains are no longer pastel fields. Fresh ones are saturated emerald with a darker rim, older ones dark olive to olive-brown with a darker edge, so the field reads as splashes of different ages rather than one green carpet; every unit stays visible on them. Watch point: where several dried stains overlap they merge into flat olive patches whose relief does not read at this zoom (lily-pad-like shapes). |
| 7 | Fog | Accept | Anchored at the burst centre; nothing outside the zone. |

### `spine_line`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | A row of heavy spike clumps travelling along the line, then the torn-earth scar and the fissure; marks run from the first to the last search point. |
| 2 | Layered | Accept | Heave, scar, tears along the line, fissure, clumps with soil collars, light dust burst, settling dust, chips. |
| 3 | Timing | Accept | Tears open a frame or two before each clump; clumps shoot up, overshoot, settle, sink from the source end; the sheet now runs to the true end (3.28 s) and the last column is clean. |
| 4 | No tells of cheapness | Accept, with a watch point | No strokes across the line at any age (single spikes removed, tears run along it); clumps have volume (lit and shadowed flanks, soil collar) and stand about 0.7–0.9 units. Watch point: at close zoom the clumps' outer spikes splay wide, so a row of them can read as a row of spiky crowns; the scar is wider and broken, but its clods read only at close zoom. |
| 5 | Value and colour | Accept | The newest clump bone-ivory but still shaded; older clumps umber; scar darker than the ground with lighter clods. |
| 6 | Crowd | Accept | Round-4 frame: each line reads as a row of dark spike clumps on a soft brown-grey scar, clearly lines of spikes rather than round 3's thin pencil lines; they cross stains and units without hiding them. The faint fissure tails of older lines are thin grey streaks. |
| 7 | Fog | Accept | Fog sheet: nothing at the attacker; the ground marks and dust end at the fog edge (the cell edge half a spacing before the first seen point), fading in over 0.3; the first seen clump is drawn whole (a first take cut it in half at the edge, fixed), so its outer spikes lean a little over the fogged ground; no pop. |

**Round 4 result (implementer):** accepted, every item for every effect, with the watch points above. The
independent round-4 review follows on [`independent_review_packet.md`](independent_review_packet.md).

## Changes for round 5 (after the independent review of round 4)

Round 4 accepted `shell_impact` and `spine_line` and rejected `acid_burst` narrowly
([`independent_review_round4.md`](independent_review_round4.md)): its stain's visible edge lay at 0.87 of the
reach, and a pale, see-through band lay between the stain and the front's lip. Round 5 is the last of Q0080's five
rounds. The mapping of every round-4 finding to its change is in
[`independent_review_packet.md`](independent_review_packet.md), section "Round 5"; the plan's round-5 note
records the decisions.

### Framework

| Gap | Change |
|---|---|
| A Critical decal's visible edge is the texture's ragged outline eaten by deep noise bites, so it cannot be put on a radius (round 4: box 0.98, visible edge 0.87) | A decal may have a **disc outline** (`outline: (edge_noise, fill)`, `decal.wgsl`): its visible edge is a disc of the box's half length whose noise edge wavers by `edge_noise` of the radius (fine noise read on the circle of directions), and the texture is read from its middle (`fill`) inside it, so the texture's own outline lies outside. The wet rim follows the disc's edge. `DecalInstance` grows from 128 to 144 bytes. Range checks in `effect_catalog_tests`. |
| The footprint test only held the lasting Critical marks inside the reach, so a stain 13 % short passed | **Tightened:** the held stain's visible edge (once every non-decal layer has ended, 1.39 s) must lie within ±2 px of the reach on at least 85 % of 72 rays, and never more than 2 px beyond it; the threshold is the test's "lit" bar (any channel more than 16 levels from the empty ground). |
| A standing clump at the line's first point was cut by the line start into a lone spike | `runtime::fit_inside_line`: a standing card clipped to its line is made smaller and moved inward at a line end so that it stands whole inside the line (keeping its far edge), never wider than the line; unit test `a_standing_card_at_a_line_end_is_fitted_whole_inside_the_line`. |

### Textures ([`texture_review.md`](texture_review.md), "Review round 5")

New `fx_acid/acid_glob`: three image-model stills ($0.33; round, tearing in two, stretched), silhouette-keyed.
The 27 existing `fx_acid` layers are byte-identical. Ledger $106.45 of $200.

## Round 5

Evidence: the round-5 contact sheets, sequences and fog sheet in this directory (lavapipe, debug
`battle_capture`, Xvfb, 2026-11-11: the path of rounds 3 and 4), judged cell by cell and on the full-resolution lab
frames. Edge measurements: the outermost pixel more than 16 levels from the clean ground along the centre row,
against the middle of the reach circle's line in the reference frame. Colours: 3×3 averages of the sRGB frame.

**Stain edge against the reach** (bright ground; px of the 1280×720 frame; negative is inside):

| Age | 0.18 s | 0.48 s | 0.9 s | 1.8 s | 3.0 s |
|---|---|---|---|---|---|
| Default zoom, left / right | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 | 0 / 0 (drying bites start inside) |
| Close zoom, left / right | | +2 / +1 | +2 / 0 | +2 / 0 | |
| Diameter against the reach (round 4: 0.87) | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 |

**Band between stain and lip** (sequence, bright ground, default zoom, centre row from the reach inward): every
sample from the lip inward is wet emerald, darker than or equal to the body; no lighter band. Frame 2: lip and
inner side (41–50, 111–125, 36–50), body (47–57, 115–134, 47–57); frame 4: (40–48, 109–117, 36–48), body
(52–57, 127–136, 52–58); frame 8: (41–47, 109–115, 38–47), body (54–58, 125–140, 52–59). Round 4 sampled
(98–109, 131–132, 88–98) there against a body of (45, 103, 42).

GPU figures (`effect_slice_gpu_tests.rs` through the harness, 2026-11-11; goldens re-recorded on both drivers):

| Effect | Footprint | Coverage of the full-damage disc at peak | Pop | Lingering |
|---|---|---|---|---|
| `acid_burst` | front: 100 % of 72 rays within ±2 px, mean +0.11 px (lavapipe) / +0.10 px (radv), largest overshoot +1.27 px; **held stain's visible edge: 100 % of rays within ±2 px, mean −0.18 px, largest overshoot +1.27 px** (both drivers) | 98 % (round 4: 87 %) | 0.22/255 | 0/255 after the end |
| `shell_impact` | 99 % of 72 rays within ±2 px, mean −0.70 px (lavapipe) / −0.66 px (radv), largest overshoot +0.44 px | 100 % | 1.22/255 | 0/255 |
| `spine_line` | marks start −0.65 px and end +0.31 px from the line's projected ends | 75 % (least point 42 % lavapipe, 41 % radv) | 0.76/255 | 0/255 |

### `acid_burst`

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The stain's visible edge lies on the reach at every held age (table above), so the stain alone marks the area once the front has gone; nothing contracts when the front fades (frames 8–10). |
| 2 | Layered | Accept | Light, larger torn crown, hot core, wet lip, dark edge ring, eleven globs of three shapes, spray, faint yellow-green fumes, liquid stain, wet centre. |
| 3 | Timing | Accept | As round 4: peak at frames 0–1, olive by 0.9–1.8 s, drying bites from about 1.8 s, clean at 6 s. |
| 4 | No tells of cheapness | Accept, with a watch point | No pale or see-through band (samples above); globs are dark wet bodies of three shapes and sizes, oriented along their flight; fumes thin and greenish. Watch point: while held, the stain is a near-perfect disc with a fine wavering edge (what the review asked for); the irregularity now comes from the relief and pooling inside it and from the drying bites. A faint pale relief highlight still runs along the upper edge at 0.9–1.8 s. |
| 5 | Value and colour | Accept | Unchanged ramp; lime only in the crown, core and glob highlights; dark wet rim along the disc edge. |
| 6 | Crowd | Accept, with a watch point | Round-5 frame ([`stress_capture_400.png`](stress_capture_400.png)): every unit and building stays readable on the stains; the stains range from fresh emerald to dried olive with dark rims. Watch point: at true size and with a near-circular held edge, the fresh stains overlap into rounder, larger green fields than in round 4 (the round-4 review expected their coverage to rise), at 200 splashes a second, far beyond a real fight. |
| 7 | Fog | Accept | Anchored at the burst centre; nothing beyond the reach. |

### `spine_line` (accepted in round 4; polish only)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1–3, 5, 7 | All | Accept | Unchanged except the polish below; the fog sheet is re-taken and unchanged in behaviour (the fitting applies only at the line's own ends). |
| 4 | No tells of cheapness | Accept | The first clump is a smaller whole clump of several spikes inside the line (close zoom, 0.097 s and 0.26 s), not a lone "L"; no dark chip at the far end at 1.622 s (the clumps fade over their last 15 % as they sink); the scar darkens every ground: on teal at 1.622 s the line's row averages (79, 102, 97) against the clean ground's (92, 137, 133) on the same row (round 4: (119, 126, 116) on (95, 138, 133)). |

### `shell_impact` (accepted; optional change)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1–7 | All | Accept | The blast mark now comes in at 0.12 s, after the ring has passed the reach, so the ring races out over clean ground (sequence frames 2–4); its end is unchanged. The ring's width variation is unchanged (the second optional change was not taken, to avoid regressing an accepted effect). |

The crowd frame is re-taken with the round-5 data (figures in [`stress_capture.txt`](stress_capture.txt),
round-5 paragraph): `spine_line` and `shell_impact` read as in round 4 (rows of spike clumps over a darker scar;
small sparks), and no effect hides a unit.

Other tests (2026-11-11, unlocked at `-j 4` under `nice -n 10`; no `cargo gate`, Q0061): client lib `effects`
45 passed (with `a_standing_card_at_a_line_end_is_fitted_whole_inside_the_line` and the 144-byte decal record);
`effect_catalog_tests` 22 (outline range checks), `effect_runtime_tests` 15, `battle_fog_tests` 23;
`client_diagnostics` lib `effect_lab` 3; `gpu_validation_tests` effect shaders (both MSAA variants) 1;
`effect_slice_gpu_tests` 4 on lavapipe and on radv (radv's shell and spine children timed out once under a load
average above 100 and passed on a rerun); `concealment_gpu_tests` 1 (harness); `sync_steady_state_tests` 8;
`sync_allocation_count_tests` (valgrind) 10, run alone; `smog_golden_tests` 17; `dependency_direction_tests` 2;
`workspace_checks` 74 unit and 23 charter; `gate` `suite_catalog_tests` 7; `cargo clippy --workspace
--all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.

**Round 5 result (implementer):** accepted, every item for every effect, with the watch points above. The
independent round-5 review follows on [`independent_review_packet.md`](independent_review_packet.md).

## Scale-out round 1 (WP7, plan Amendment 4)

Evidence: the contact sheets and sequences of this round in this directory (lavapipe, debug `battle_capture`,
Xvfb, 2026-11-11: the slice's path), judged cell by cell and on the full-resolution lab frames:
[`flame_line_contact.png`](flame_line_contact.png), [`lance_sweep_contact.png`](lance_sweep_contact.png),
[`flak_burst_contact.png`](flak_burst_contact.png), [`cleave_impact_contact.png`](cleave_impact_contact.png), the
`_sequence.png` beside each, and the re-taken [`acid_burst_contact.png`](acid_burst_contact.png) and
[`spine_line_contact.png`](spine_line_contact.png) for the polish. Bindings are the weapons' own:
`flame_line` 25 searches of radius 0.15 over 6.25 (spacing 0.25); `lance_sweep` 11 searches of radius 0.15 over
2.5 across the struck point; `flak_burst` one zone of 1.5; `cleave_impact` 0.5 / 1.0 / 1.5. No texture was made and
nothing was spent this round (ledger unchanged: 251 Veo requests at $0.40 and 84 image requests, $106.45 of $200).

GPU figures (`effect_slice_gpu_tests.rs` through the harness, lavapipe, 2026-11-11, with the name-salted random
choices; RADV_NOTE):

| Effect | Footprint | Coverage of the full-damage disc at peak | Pop | Lingering |
|---|---|---|---|---|
| `flame_line` | marks start −1.27 px and end +1.00 px from the line's projected ends | 99 % (least point 90 %) | 1.63/255 | 0/255 after the end |
| `lance_sweep` | marks start −0.14 px and end +0.21 px | 87 % (least point 41 %) | 0.21/255 | 0/255 |
| `flak_burst` | 97 % of 72 rays within ±2 px of the reach, mean −0.85 px, largest overshoot +0.32 px | 65 % | 1.35/255 | 0/255 |
| `cleave_impact` | 100 % of 72 rays within ±2 px of the reach, mean −0.71 px, largest overshoot +0.44 px | 94 % | 0.63/255 | 0/255 |
| `acid_burst` (polish, new salt) | front and held stain: 100 % of rays within ±2 px, mean +0.17 / −0.09 px, largest overshoot +1.35 px | 99 % | 0.19/255 | 0/255 |
| `shell_impact` (new salt) | 100 % of rays within ±2 px, mean −0.61 px, largest overshoot +0.41 px | 100 % | 1.17/255 | 0/255 |
| `spine_line` (polish, new salt) | marks start +0.10 px and end +1.42 px | **62 %** (least point 42 %; round 5: 75 %) | 0.65/255 | 0/255 |

Watch point for `spine_line`: its coverage of the full-damage discs within 80 ms fell from 75 % to 62 % (the test's
floor is 60 %) with the scar's wider scatter and the new random draw; at points where a scattered scar piece and
the heave both lie off the point, the disc is about 40 % covered in its first frames.

Fog (R6): the masked-line test now runs for every line subject; with the line seen from its fifth search point
on, `flame_line` draws 2 983 px from that point and 0 px more than 1 unit before it, `lance_sweep` 538 and 0,
`spine_line` 3 258 and 0.

### `flame_line` (Hellion analogue)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The jet's core (Critical) races from the nozzle to the line's end in 0.14 s and the soot streak (Critical) marks the line from 0.02 s for 3 s; footprint within 1.3 px of the line's ends. One focal point: the hot core streaming along the line. |
| 2 | Layered | Accept | Hot core ribbon, sooty glowing sheath ribbon, flame tongues (fireball flipbook cards every other point), burning napalm on the ground, soot streak, smoke, burning droplets, heat haze (distortion). |
| 3 | Timing | Accept | Peak by frame 2–3 (the head reaches the end at 0.14 s), the tail leaves the nozzle at 0.12 s and the sheath erodes by 0.45 s; the napalm burns out by 0.8 s; the streak erodes at its end (3 s). |
| 4 | No tells of cheapness | Accept, with a watch point | Close zoom 0.1 s: the flame tongues billow over a torn sheath; at 0.26 s a wall of fire along the line. Watch point: in the first two frames the core's head is a smooth, rounded glow ahead of the tongues (it reads as the jet's hot tip), and the ribbons are smooth tubes where their noise is below a pixel (the default zoom's 0.3-unit-wide jet). |
| 5 | Value and colour | Accept | Brightest in the core near the nozzle, orange body, sooty dark parts; the lasting streak is darker than both grounds. |
| 6 | Crowd | Not measured | No crowd frame this round (the stress mode streams the slice's three effects; recorded under "What remains"). |
| 7 | Fog | Accept | No layer at the attacker (no light at the anchor); the masked test: nothing more than 1 unit before the first seen point. |

### `lance_sweep` (Colossus analogue)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The stroke (Critical) sweeps across the struck point in 0.21 s and the burnt line (Critical) stays 2.5 s; footprint within 0.3 px of the line's ends. The line is 2.5 units long (about 55 px at default zoom), the true size. |
| 2 | Layered | Accept | Hot stroke ribbon on the ground, wide heat glow ribbon, flare at each newly reached point, cherry-red groove, burnt line, sparks, thin smoke, heat haze (distortion). |
| 3 | Timing | Accept | The stroke reaches each point at 12 units per second; the groove cools over 1 s and erodes; the line erodes at its end. |
| 4 | No tells of cheapness | Accept, with a watch point | At default zoom the stroke is a short, comet-shaped glow travelling across; at close zoom, while it is young, it is a smooth glowing streak (its noise is below a pixel at 10 px width). Smoke puffs are scattered and staggered, not a row. |
| 5 | Value and colour | Accept | White-hot at the stroke, cherry red behind, char last. |
| 6 | Crowd | Not measured | As above. |
| 7 | Fog | Accept | No layer at the source; masked test 0 px before the seen run. |

Known limitation (Amendment 4): the beams from the Colossus analogue to the sweep point are the legacy per-strike
beam, which ends at the struck point; the sweep is drawn on the ground.

### `flak_burst` (Liberator analogue's anti-air rockets)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The shock front (Critical) reaches the 1.5 reach in about 0.1 s in the air units' plane; the flash and the fireball cluster fill the area in the first frames. |
| 2 | Layered | Accept | Light, flash, fireball cluster (four cards), shock front, burning fragments, flak smoke. |
| 3 | Timing | Accept | Flash at its peak at once, fireball by 2 frames, smoke drifting and eroding by 1.7 s; no ground mark (an air burst). |
| 4 | No tells of cheapness | Accept, with a watch point | Same fireball and front materials as the accepted `shell_impact`. Watch point: the lab draws it on the ground; in battle it bursts at the air units' altitude, where nothing is below it to soften the cards. |
| 5 | Value and colour | Accept | Hot core, orange body, dark grey-brown smoke. |
| 6 | Crowd | Not measured | As above. |
| 7 | Fog | Accept | Anchored where it lands. |

### `cleave_impact` (Ultralisk analogue)

| # | Item | Verdict | Reason |
|---|---|---|---|
| 1 | Reads at gameplay zoom | Accept | The dust front (Critical) marks the 1.5 reach (100 % of rays within ±2 px) and the soil thrown up marks the full-damage zone. |
| 2 | Layered | Accept | Dust front, churned earth, eight dust puffs, chips, debris. |
| 3 | Timing | Accept | Front at the reach by about 0.1 s; everything gone within 0.9 s, so blows every 0.6 s never stack into a lasting mark. |
| 4 | No tells of cheapness | **Reject** | Close zoom on the bright ground: the front's band is a thin, even, glassy rim (a lighter leading edge over a darker band), which reads as a lens or a selection circle rather than thrown dust. Dust and soil inside it read as matter. To change: a front made of dust cards along the circle (a placement on the ring) instead of the computed front, or a darker, broken front. |
| 5 | Value and colour | Accept | Earth tones a step darker than the ground; no colour. |
| 6 | Crowd | Not measured | As above. |
| 7 | Fog | Accept | Anchored where it lands. |

### Polish of the shipped three (Amendment 3's list)

| Effect | Item taken | Result |
|---|---|---|
| `acid_burst` | 6: no concentric inner ring and no sparkle dots at frame 0 | The front and its dark edge start at 0.3 of the full-damage radius (under the crown), the spray takes the glob's dark wet ramp. Items 1–5 (meniscus, inner rims, glob letters, crowd variation, dried crust) are not taken: each needs shader or texture work and its own review. |
| `spine_line` | Less periodic blotches | The scar's pieces scattered 0.3 along and across the line (was 0.1). The churned patches are kept: scattering them 0.18 to 0.3 or varying their size ±40 % lowered the full-damage coverage from 75 % to 62 %. Torn clods (a new texture) are not taken. |
| `shell_impact` | — | Not taken (a new blast-mark texture and the front shader shared with `acid_burst`). |

The random choices of an effect's instances (rotations, placements, variants, noise phases) are now salted by the
effect's name instead of its dense id (Amendment 4), because adding the four effects had shifted `shell_impact`'s
id and with it its look (golden 0: 86.7 % of pixels within 8/255). Every effect therefore shows a different, equally
random draw than in round 5; the goldens of all seven subjects are re-recorded with this cause on both drivers and
the three shipped effects' sheets are re-taken, so the reviewer can check them for regressions:
[`shell_impact_contact.png`](shell_impact_contact.png), [`acid_burst_contact.png`](acid_burst_contact.png),
[`spine_line_contact.png`](spine_line_contact.png).

**Scale-out round 1 result (implementer):** `flame_line`, `lance_sweep` and `flak_burst` accepted with the watch
points above; `cleave_impact` rejected on item 4; crowd behaviour (item 6) not measured for the new effects.
