# HDR and Bloom: Decision (D-11, WP6c)

Plan: [`docs/plans/2026-11-09-effect-visualization.md`](../../../plans/2026-11-09-effect-visualization.md), D-11 and
the WP6c row of §9. Decided on 2026-11-10 by the implementing agent, which Q0020 allows for visual questions.

**Decision: rejected.** The battle camera stays a plain (non-HDR) camera with no bloom. Effect ramps are capped at
1.0 in every channel, and the effect catalog loader now refuses a ramp channel outside 0 to 1
(`EffectCatalogError::RampOutOfRange`), so the decision cannot be undone by data alone. Hot cores reach display
white through additive layering and the `Light` block.

## What was compared

- **Experiment arm.** `client_diagnostics::hdr_bloom`: every 3D camera gets `Camera { hdr: true }` and Bevy's
  `Bloom` with a prefilter threshold of 1.0 (soft knee 0.2), additive composite, intensity 0.18. The camera's tone
  mapping is Bevy's default (TonyMcMapface); with `hdr` it runs as a post pass over everything, effects included,
  instead of inside each PBR shader. Effects are drawn into the `Rgba16Float` view target (the renderer already
  specialises its pipelines for an HDR view).
- **Effects.** The three slice effects with the round-0 data of WP6b (ramps up to 6.0), each through the
  production effect lab (`battle_capture --effect-lab <effect> [--hdr-bloom]`), both arms on lavapipe so the
  only difference is the camera. Sheets: [`hdr/`](hdr/) (`<effect>_hdr_contact.png`), and the close cells side by
  side in [`hdr/ldr_hdr_ab_close.png`](hdr/ldr_hdr_ab_close.png) (per row: LDR flash, HDR flash, LDR fireball or
  front, HDR fireball or front).
- **Battle.** The 400-unit `frame_budget` scenario on the RX 580 with `--capture` at tick 750, with and without
  `--hdr-bloom`: [`hdr/battle_ldr_hdr_tick750.png`](hdr/battle_ldr_hdr_tick750.png) (left shipped, right HDR).
- **Frame budget.** Interleaved `frame_budget` runs (see [`frame_budget_wp6c.md`](frame_budget_wp6c.md)).

## Findings

| Question | Measurement | Result |
|---|---|---|
| Does the threshold keep terrain and bubbles unchanged? | Empty-ground lab frames (age 1.0), LDR vs HDR: mean absolute difference 0.05/255 (bright) and 0.04/255 (dark), **0 pixels** differ by more than 8/255. Battle frame at tick 750: mean absolute difference 0.06/255, 1 620 of 921 600 pixels (0.18 %) above 8/255. | Yes: within the visual tests' tolerance. Re-baselining would not have been needed for the scene. |
| What does HDR do to the effects? | Effect crop (400×300 around the effect, close zoom, bright ground), mean HSL saturation LDR → HDR: `shell_impact` fireball 47.4 → 37.0, flash 35.7 → 40.0; `acid_burst` flash 71.6 → 46.9, front 84.8 → 49.8. Brightest pixel: 255 → 236 (fireball), 251 → 233 (acid front). | The post-pass tone mapper compresses and desaturates the effects: the fireball loses its contrast against bright ground, the acid turns pastel ([`hdr/ldr_hdr_ab_close.png`](hdr/ldr_hdr_ab_close.png)). The bloom itself adds a faint halo around the white flash for its 0.12 s and nothing visible elsewhere. |
| What would adoption take? | — | Every ramp re-authored for a tone-mapped target (values far above 1.0 to survive TonyMcMapface, or a different tone mapper for the whole scene, which would move every scene golden), and the halo only exists during flashes. |
| Cost on the RX 580 | `render/bloom` 0.281 ms mean (max 0.311) in the `--render-diagnostics` run; the tone-mapping post pass and the `Rgba16Float` target's bandwidth are not instrumented. Frame interval: see the frame-budget file. | A fixed cost on every frame for an effect visible a few frames per explosion. |

**Why rejected.** With the scene unchanged, the only gain is a halo during flashes, and the price is a washed-out
look of every effect until all ramps are re-authored for a tone-mapped target, plus about 0.3 ms GPU per frame
(more with tone mapping and bandwidth). The LDR look with additive layering keeps the fireball's orange-to-white
core and the acid's saturated green, which is what the readability rules ask for (focal point brightest, hot to
dark). If the project later moves the whole scene to HDR (for lighting reasons), the cap check is the single
place to lift, and this experiment's flag (`--hdr-bloom`) re-runs the comparison.

## Consequences in this WP

- `assets/catalog/effects.ron`: every ramp channel above 1.0 capped. The fire and flash ramps were re-authored so
  the capped keys keep their hue (a plain per-channel clip of `(1.30, 0.42, 0.07)` and above turned the fireball
  pale yellow): `blast_fire` 0.50 → `(0.85, 0.30, 0.05)`, 0.75 → `(1.00, 0.62, 0.16)`, 1.00 → `(1.00, 0.92, 0.60)`;
  `blast_flash` 0.25 → `(1.00, 0.62, 0.22)`, 0.60 → `(1.00, 0.86, 0.55)`, 1.00 → `(1.00, 1.00, 0.92)`.
- `client::effects::catalog`: `RampOutOfRange` load error naming file, ramp, key and value, with its test in
  `effect_catalog_tests.rs`.
- No visual golden changed because of HDR (it was not adopted); smog goldens are untouched (D-9).
