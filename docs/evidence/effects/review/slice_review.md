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
| 1 | `effects.ron` of this WP | [`shell_impact_contact.png`](shell_impact_contact.png), [`acid_burst_contact.png`](acid_burst_contact.png), [`spine_line_contact.png`](spine_line_contact.png) and the `_sequence.png` beside each | lavapipe |

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
