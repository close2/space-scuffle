# Evidence: Effect Visualization (WP0 to WP15)

Plan [`2026-11-09-effect-visualization`](../plans/2026-11-09-effect-visualization.md), WP15. This file closes the
plan. It is an index: for each work package it names what shipped, its commits and merge to `main`, its review
rounds and verdicts, its key measurements and the evidence pictures, and what it spent. The detail lives in the
plan's amendments, the review files under [`effects/review/`](effects/review/) and the measurement files beside
them; nothing here replaces them.

Every commit and merge below was checked with `git log` on `main` at `477c740e` (2026-11-18). Reviews are
independent reviews under [Q0080](../questions/closed/0080-effect-review-outcome.md) (up to five rounds per step)
unless marked as the implementer's.

## 1. Summary

| Item | Result |
|---|---|
| Determinism (D-1, §10.1) | Every golden replay (debug, release, `wasm32-wasip1`), the matchup matrix and the checksum schema stayed bit-identical at every WP; no re-record was made under this plan (WP2 `f57da973`; each later amendment states D-1 held, and WP14 ran `golden_replay_tests`, `sc2_who_wins_matchup_matrix_tests` and `determinism_lockstep_tests` green on its branch) |
| Leak (B-106) | Closed by WP3 (`0a360f5e`): an undetected burrowed enemy changes 0 pixels where it stands (radv and lavapipe, [`wp8r4_measurements.txt`](effects/review/wp8r4_measurements.txt)) |
| Vocabulary | 59 effects in `assets/catalog/effects.ron`, every one drawn at least once by the WP14 every-effect frame budget; every shipped weapon, mode, status, ability look and death chosen in data (C5, C6) |
| Review rounds | 29 independent review rounds across the slice, scale-out, WP8 to WP11, WP13 and WP14 (§3), plus the implementer's slice and texture reviews and the identity plan's two |
| Frame budget (§10.3) | Effect passes' GPU time +0.16 ms (WP6c slice) and +0.47 to +0.57 ms (WP14) against the 1.5 ms bar on the RX 580; p50/p99 bars met against the recorded baseline; the main-world cost of the splash stream is registered as B-137 (§4) |
| Spend | **$17.25** under this plan: 28 Veo clips ($11.20) and 55 paid image-model stills ($6.05); ledger total **$113.65 of the $230 cap** on 2026-11-18 (§5) |
| Register | B-106, B-107, B-108, B-109, B-110 and B-111 fixed; B-119 fixed in WP8; B-122 to B-126 and B-128 fixed by plan [`2026-11-17-effect-snapshot-identity`](../plans/2026-11-17-effect-snapshot-identity.md); open: B-117, B-118, B-120, B-121, B-127, B-130, B-132, B-133, B-137 (plan Amendment 32) |

## 2. Per work package

### WP0 to WP6c: contract, leak fix, framework, pipeline and the vertical slice

| WP | What shipped | Commits | Evidence |
|---|---|---|---|
| WP0 | Plan row; Q0075 to Q0078 filed; B-106 to B-111 registered | `dd38da55` | Plan §13 |
| WP1 | `sim_types` presentation contract (concealment, stance, statuses, barrier ratio, effect sources, `SplashVisual`, areas, links, `revealed`) | `9260eacd` | `snapshot_serialization.rs` round trips |
| WP2 | `sim_core` snapshot writer for that contract; nothing hashed changes | `f57da973` | `sim_core/tests/effect_snapshot_tests.rs`, `player_view_tests.rs`; goldens (debug, release, `wasm32-wasip1`), matrix, lockstep and checksum unchanged (commit message) |
| WP3 | One `BattleFog` rule for every reader; B-106 fixed | `0a360f5e` | `battle_fog_tests.rs`, `concealment_gpu_tests.rs` |
| WP4a | `bevy_hanabi` spike: **no-go** (criteria 5 and 6 fail); sprite-instanced particles instead (D-10) | `c09bd47b` | [`effects/hanabi-spike.md`](effects/hanabi-spike.md) |
| WP4 | `client::effects`: vocabulary loader, unit tables, runtime, budget and level of detail, instanced renderer | `f4f39d4a`, `618263a9`, `9e97682a` | `effect_catalog_tests.rs`, `effect_runtime_tests.rs`, `effect_gpu_tests.rs` |
| WP5 | `asset_baker` effect-texture pipeline and image-model client | `9c01f5bd` | `effect_texture_tests.rs`, `image_client_tests.rs`, `bake_integrity_tests.rs` |
| WP6a | Slice textures `fx_noise`, `fx_blast`, `fx_acid`, `fx_ground`, with provenance | `e7da8a88`, `98d2902d` | [`effects/review/texture_review.md`](effects/review/texture_review.md); bake recipes in [`effects/recipes/`](effects/recipes/) |
| WP6b | `shell_impact`, `acid_burst`, `spine_line`; effect lab | `a838c88c` to `1eaaf641` | `effect_slice_gpu_tests.rs` with per-driver goldens |
| WP6c | HDR and bloom **rejected** (D-11; ramps capped at 1.0); slice frame budget; review gate | `e7225484` to `29bb16c4` | [`hdr_bloom_decision.md`](effects/review/hdr_bloom_decision.md), [`frame_budget_wp6c.md`](effects/review/frame_budget_wp6c.md), [`slice_review.md`](effects/review/slice_review.md) (implementer) |

**Merges.** `d03ed193` (2026-11-10, landed at the review gate with `shell_impact` only, Amendment 1) and `af68b4f8`
(2026-11-11, `acid_burst` and `spine_line` shipped after review round 5, Amendment 3).

**Review gate (Amendments 1 to 3).**

| Round | Review | `shell_impact` | `acid_burst` | `spine_line` |
|---|---|---|---|---|
| 1 | [`independent_review.md`](effects/review/independent_review.md) | reject | reject | reject |
| 2 | [`independent_review_round2.md`](effects/review/independent_review_round2.md) | **accept** | reject | reject |
| 3 | [`independent_review_round3.md`](effects/review/independent_review_round3.md) | accept | reject | reject |
| 4 | [`independent_review_round4.md`](effects/review/independent_review_round4.md) | accept | reject | **accept** |
| 5 | [`independent_review_round5.md`](effects/review/independent_review_round5.md) | accept | **accept** | accept |

**Measured (WP6c).** Effect passes under 200 splashes a second: +0.16 ms mean on the RX 580; the quietest triple
+1.1 % p50 and +8.9 % p99 against the baseline; bloom alone 0.28 ms and the tone mapper washed out every effect
(acid front saturation 85 to 50), so it was rejected. Pictures: the slice's contact sheets and sequences
(`shell_impact_*`, `acid_burst_*`, `spine_line_*`), the fog takes (`*_fog_*`) and the crowd capture
(`stress_capture_400*.png`) in [`effects/review/`](effects/review/).

### WP7: splash impacts, ribbons and distortion (scale-out)

- **Shipped:** `Ribbon` and `Distortion` blocks; `flak_burst`, `flame_line`, `lance_sweep`, `cleave_impact`; the
  projectile impact wiring (in `effects::systems` since WP4). Commits `3dbf9bda` to `87582e95`; merge `1d258fdc`
  (2026-11-12). Amendments 4 to 9.
- **Reviews:** [`independent_review_scaleout1.md`](effects/review/independent_review_scaleout1.md) to
  [`_scaleout5.md`](effects/review/independent_review_scaleout5.md): `flak_burst` accepted in round 1, `flame_line`
  and `lance_sweep` in round 2, `cleave_impact` in round 5 (Amendment 9's table).
- **Pictures:** `flak_burst_*`, `flame_line_*`, `lance_sweep_*`, `cleave_impact_*`, `stress_*_400*.png`.

### WP8: concealment and stance

- **Shipped:** `burrow_dig`, `burrow_mound`, `burrow_emerge`, `cloak_shimmer`, `cloak_own`, `detected_mark`,
  `mode_seal_sweep`, `siege_anchor`; the `Shell` hook; the client-local settle (D-6); B-119 fixed (the distortion
  read). Commits `fcc104f5` to `ce47b86f`; merge `08766a43` (2026-11-12). Amendments 11 to 15.
- **Reviews:** [`independent_review_wp8_1.md`](effects/review/independent_review_wp8_1.md) to
  [`_wp8_5.md`](effects/review/independent_review_wp8_5.md); round 5 (engineering only) accepted.
- **Measured** ([`wp8r4_measurements.txt`](effects/review/wp8r4_measurements.txt),
  [`wp8r5_measurements.txt`](effects/review/wp8r5_measurements.txt)): an undetected burrowed enemy changes 0 pixels;
  an undetected cloaked enemy's shimmer bends 1 224 pixels with 0 foreign colours (mean shift 0.40 levels); the
  burrow's channel ends with the body 77 % sunk. Frame budget with the per-bubble shadows: p50 12.45 ms against
  `main`'s 12.10 ms, p99 lower ([`2026-11-12-frame-budget-wp8-shadows.md`](2026-11-12-frame-budget-wp8-shadows.md)).
- **Pictures:** `wp8r4_*` (radv and lavapipe): dig, emerge, siege and unsiege sequences, battle tableaux detected and
  undetected, anchored and own-cloaked crops.

### WP9: statuses and deaths

- **Shipped:** `stim_surge`, `concussive_daze`, `charge_streak`, `barrier_shell` and its absorb flash;
  `vehicle_wreck`, `bio_burst` deaths (the glass shatter stays the default); effect pipelines warmed every frame.
  Commits `81f3d84b` to `1fca0573`; merge `6398fb35` (2026-11-13). Amendments 17 to 19.
- **Reviews:** [`independent_review_wp9_1.md`](effects/review/independent_review_wp9_1.md) (reject),
  [`_wp9_2.md`](effects/review/independent_review_wp9_2.md) (reject),
  [`_wp9_3.md`](effects/review/independent_review_wp9_3.md) (**accept**).
- **Measured** ([`wp9r3_measurements.txt`](effects/review/wp9r3_measurements.txt)): each status's look held on every
  frame its status is published and on none other (stim 88/88, daze 74/88, charge 10/88, barrier 88/88); the charge
  laid dust only while moving; 8 absorbed hits, 8 flashes.
- **Pictures:** `wp9r3_*` sequences (stim, daze, charge, barrier, deaths), battle frames and lab sheets, both drivers.

### WP10: areas and links

- **Shipped:** `psionic_storm`, `blinding_cloud`, `abduct_tether`; the `abilities` map in presentation blocks; the
  per-frame fog cut that never draws unseen ground. Commits `17e68f81` to `6c02b593`; merge `6b79d525`
  (2026-11-13). Amendments 20 to 23.
- **Reviews:** [`independent_review_wp10_1.md`](effects/review/independent_review_wp10_1.md) (reject),
  [`_wp10_2.md`](effects/review/independent_review_wp10_2.md) (areas on condition, tether reject),
  [`_wp10_3.md`](effects/review/independent_review_wp10_3.md) (areas accept, tether reject),
  [`_wp10_4.md`](effects/review/independent_review_wp10_4.md) (tether **accept**; B-121 registered).
- **Measured** ([`wp10r3_measurements.txt`](effects/review/wp10r3_measurements.txt),
  [`wp10r4_measurements.txt`](effects/review/wp10r4_measurements.txt)): worst area edge error 1.72 px (storm) and
  1.51 px (cloud) against ±2 px, 0 px spill, 0 of 194 unseen samples changed on both drivers (round 3); the tether's
  ends 0.52 and 0.71 px from the bubbles' surfaces (round 2) and its bulges travelling about 2.3 px per 80 ms
  towards the caster (round 4).
- **Pictures:** `wp10r3_*`, `wp10r4_*` (storm, cloud, tether sequences, crowd and fog frames, both drivers).

### WP11: every weapon's own look

- **Shipped:** `Shot` and `Missile` events; tracers, beams, projectiles, impacts, blows and the default
  `glass_shatter` death for every shipped unit and structure, replacing the legacy `combat_fx` cards in battle (the
  legacy module stays as the fallback for synthetic events); hits on the struck bubble's glass; Fungal Growth's
  missile, impact and mark (F1). Commits `59364764` to `b144655c`; merge `3f5e898e` (2026-11-16). Amendments 24 to
  28.
- **Reviews:** [`independent_review_wp11_1.md`](effects/review/independent_review_wp11_1.md) (reject),
  [`_wp11_2.md`](effects/review/independent_review_wp11_2.md) (reject),
  [`_wp11_3.md`](effects/review/independent_review_wp11_3.md) (accept with conditions C1 to C5),
  [`_wp11_4.md`](effects/review/independent_review_wp11_4.md) (accept with conditions C1, C2; met in round 5,
  Amendment 28).
- **Measured** ([`wp11r5_measurements.txt`](effects/review/wp11r5_measurements.txt), radv): every family's impact
  beside the struck bubble beats twice its own blank run plus 20 pixels (for example `bullet_hit` 59 px against a
  blank of 0, `psi_slash` 233 px against 48). Coverage: `every_shipped_weapon_names_the_looks_of_the_event_it_publishes`.
- **Pictures:** `wp11r4_battle_lavapipe_*` (one per family and plane), `wp11r3_looks_*`, `wp11r2_fungal_*`.
- **Follow-ups** B-122 to B-126 and B-128, fixed by plan `2026-11-17-effect-snapshot-identity` (Amendment 30; its
  own evidence file [`2026-11-17-effect-snapshot-identity.md`](2026-11-17-effect-snapshot-identity.md)).

### WP12: mode commands end to end

- **Shipped:** three headless tests in `client/tests/skirmish_match_tests.rs`: burrow and siege by key publish their
  stance and hold their looks; the opponent sees nothing under its fog; a burrowed unit is seen only where detected.
  Commit `2cdcab7f`; merge `5b503df5` (2026-11-16). Amendment 29. No review needed (tests only); no finding.
  B-127 (a radv child stall in its package gate) registered.

### WP13: mode and morph unit clips

- **Shipped:** mode atlases for the sieged `railgun_crawler` and the burrowed `burrow_scorpion` (Idle, Attack, enter
  and leave morphs); B-111 fixed. Commits `222d0bb7`, `4e84927a`; merge `573a88cc` (2026-11-12); the tank's reframe
  `77b77f67` to `fc8232fb`, merge `d9f9b0eb` (2026-11-12). Amendments 10 and 16.
- **Reviews:** [`veo/modes/independent_review_wp13.md`](veo/modes/independent_review_wp13.md) (all 8 slices
  accepted) and [`veo/modes/independent_review_reframe.md`](veo/modes/independent_review_reframe.md) (art accepted;
  its record items fixed in `fc8232fb`). Clip log: [`veo/2026-11-12-mode-clips.md`](veo/2026-11-12-mode-clips.md).
- **Spend:** 11 clips ($4.40) then 7 for the reframe ($2.80): $7.20 of the $20 cap (Q0075).

### WP14: the showcase's effects take and every effect in the frame budget

- **Shipped:** a second showcase take `effects` playing `assets/scenarios/showcase_effects.ron` (a siege line that
  anchors and burrows; raiders that attack into it); storyboard scenes that list the effects they must spawn; the
  scenario order `EnterModes`; `frame_budget --effect-mix`; the effect pool at 6 144. Commits `8afb6a78`,
  `2632ea82`, `ff9456c4`, evidence `74b3df13`; merge `477c740e` (2026-11-18). Amendment 31. Scoped out (Amendment
  31): `fx_energy` and texture variants (no consumer; nothing spent) and the Baneling beat (no unit media).
- **Review:** [`independent_review_wp14_1.md`](effects/review/independent_review_wp14_1.md): accept with conditions
  W14-1 (the cost attribution) and W14-2 (`entrench` composition), both met in round 2 (`ff9456c4`, the packet's
  "WP14" section).
- **Measured** ([`frame_budget_wp14.md`](effects/review/frame_budget_wp14.md), radv):

  | Bar (§10.3, WP14 row) | Result |
  |---|---|
  | Effect passes' GPU time ≤ 1.5 ms | +0.47 ms (splashes), +0.57 ms (the plan's stress), +0.54 ms (every effect): met |
  | The plan's stress: p50 ≤ 17.1 ms, p99 ≤ 26.0 ms (recorded baseline + 5 %, + 10 %) | 14.97 and 25.20 ms: met |
  | Every effect at once (beyond the plan's stress) | 17.15 and 26.93 ms: over by 0.3 % and 3.6 % |
  | Against today's faster baseline (11.86 ms p50) | +26 % p50 for the plan's stress; the cost is main-world update from the splash stream (B-137) |
  | Every effect of the vocabulary drawn | 59 of 59 |
  | The effects take spawns each effect its scenes ask for | radv full length and the cut-down GPU test (`showcase_tests`): all |
  | CI showcase render on lavapipe | local lavapipe (4 CPUs): both takes complete, video 19.5 MB, teaser 4.26 MB; CI step estimated at about 35 min; **first CI measure pending** (below) |

- **Pictures:** [`wp14_effects_take_radv.jpg`](effects/review/wp14_effects_take_radv.jpg),
  [`wp14_entrench_strip_radv.jpg`](effects/review/wp14_entrench_strip_radv.jpg),
  [`wp14_siege_sequence_radv.jpg`](effects/review/wp14_siege_sequence_radv.jpg), crowd frames
  [`wp14_crowd_mix_400.jpg`](effects/review/wp14_crowd_mix_400.jpg) and
  [`wp14_crowd_baseline_400.jpg`](effects/review/wp14_crowd_baseline_400.jpg).

**The first CI render is not yet measured.** On 2026-11-18 CI on `main` at `477c740e` was still running, and the
`Showcase video` workflow renders only after CI passes. The last render before WP14 (`main` at `c06d8f5c`, the
`battle` take alone, run 44292699970) took 24 min 43 s in its "Render the showcase" step (job 33 min). The step has
no timeout of its own; the job's limit is 90 min (review W14-8). To read the first measure:

1. `gh run list --workflow showcase.yml` and take the first run whose head is `477c740e` or later.
2. `gh api repos/{owner}/{repo}/actions/runs/<run id>/jobs --jq '.jobs[0].steps[] | [.name, .started_at, .completed_at] | @tsv'`:
   the duration of "Render the showcase" is the figure against the 40 min budget of Amendment 31.
3. The run's step summary (table "Take … Render (s)") gives each take's own render seconds, from `showcase.json`.

If the step exceeds 35 min, shorten `siege`, then `closing` (Amendment 31, "Open").

### WP15: bookkeeping

This file; `DESIGN.md` §4 (B-107 to B-110 closed, the "Effect presentation" and "Showcase video" rows); the plan's
WP table and Amendment 32; the plan's row in [`docs/plans/README.md`](../plans/README.md) set to Done. C22: every
test file the plan added is selected by `crates/gate/suites.ron` (the `sim_core` files as their crate's own tests,
the `client`, `client_diagnostics` and `asset_baker` files also by named rules, GPU files classed `Gpu`); checked by
`gate/tests/suite_catalog_tests.rs` and `cargo gate quick`.

## 3. Review rounds at a glance

| Step | Rounds | Final verdict | Files |
|---|---|---|---|
| Slice (WP6c gate) | 5 | accept (all three) | `independent_review.md`, `_round2.md` to `_round5.md` |
| Scale-out (WP7) | 5 | accept (all four) | `independent_review_scaleout1.md` to `5` |
| WP8 | 5 | accept | `independent_review_wp8_1.md` to `5` |
| WP9 | 3 | accept | `independent_review_wp9_1.md` to `3` |
| WP10 | 4 | accept | `independent_review_wp10_1.md` to `4` |
| WP11 | 4 (+ round 5 meeting conditions) | accept with conditions, met | `independent_review_wp11_1.md` to `4` |
| WP13 | 2 | accept (art) | `veo/modes/independent_review_wp13.md`, `_reframe.md` |
| WP14 | 1 (+ round 2 meeting conditions) | accept with conditions, met | `independent_review_wp14_1.md` |
| Identity plan follow-ups | 2 | accept | `independent_review_identity_2.md`, `_3.md` |

The review packet that every round appended to is
[`effects/review/independent_review_packet.md`](effects/review/independent_review_packet.md).

## 4. Frame budget history

| When | Load | Effect passes (GPU) | p50 / p99 against the recorded baseline | Source |
|---|---|---|---|---|
| WP6c | 200 splashes/s, the slice's three looks | +0.16 ms | +1.1 % / +8.9 % (quietest triple) | [`frame_budget_wp6c.md`](effects/review/frame_budget_wp6c.md) |
| WP8 | battle with per-bubble shadows | — | 12.45 against 12.10 ms (p50), p99 lower | [`2026-11-12-frame-budget-wp8-shadows.md`](2026-11-12-frame-budget-wp8-shadows.md) |
| WP14 | 200 splashes/s, 20 areas, 50 statuses | +0.57 ms | 14.97 / 25.20 ms (bars 17.1 / 26.0) | [`frame_budget_wp14.md`](effects/review/frame_budget_wp14.md) |
| WP14 | every effect at once | +0.54 ms | 17.15 / 26.93 ms | same |

B-137 tracks the main-world update cost of the shipped splash looks (+3.6 to +4.1 ms over today's faster
baseline).

## 5. Media spend

Counted from [`veo/ledger.jsonl`](veo/ledger.jsonl) at `477c740e` at the list prices of
`assets/media/veo_budget.ron` (a Veo clip 40 cents, an image 11 cents; a start the service refused for quota costs
nothing).

| WP | Ledger events | Count | Spend |
|---|---|---|---|
| WP6a (2026-11-10) | `effect_clip_request_started` (`fireball_flipbook` 1, `acid_crown` 4, `spine_erupt` 3, `acid_bubbles` 2) | 10 clips | $4.00 |
| WP6a (2026-11-10) | `image_downloaded` (12 elements; 29 more starts refused for quota, free) | 52 images | $5.72 |
| WP6c round 4 (2026-11-11) | `image_downloaded` (`acid_glob`, three variants) | 3 images | $0.33 |
| WP13 (2026-11-12) | `mode_clip_request_started` (first batch 11, reframe 7) | 18 clips | $7.20 |
| WP7 to WP12, WP14, WP15 | none | 0 | $0.00 |
| **This plan** | | **28 clips, 55 images** | **$17.25** |
| **Ledger total** (every plan) | 269 clips, 55 images | | **$113.65 of $230** |

Against the plan's §6.6 lines: Veo effect clips $4.00 of $12.00; images $6.05 of the owner's $25 target (cap below
$50, Q0078); mode clips $7.20 of $20 (Q0075). The WP14 lines (≤ $8.80 Veo, ≤ $20 images) are unspent and stay
available to a polish round.
