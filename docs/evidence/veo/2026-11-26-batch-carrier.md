# Veo: the carrier batch, the Carrier and its interceptor (2026-11-26), stopped for the owner, then continued

Evidence for WP5 of plan [`2026-11-28-carrier-interceptors`](../../plans/2026-11-28-carrier-interceptors.md) (D-11),
run on 2026-11-26 (UTC) from branch `carrier-interceptors-plan` (WP4 `43b0529e`), with the method of batches 1 to
[6](2026-11-09-batch-6.md): authored texts on the batch-5 aircraft pattern, the seed clip, then the pin, then the
pinned states, and the D-7 review.

**First result: the batch stopped at the seed Idles, and no clip was accepted** (the continuation, below, installs six clips). Four seeds of each identity, eight
generations, all drew diagonal light shafts across the backdrop, which breaks the owner's backdrop rule (no radiating
stripes, rays or spotlight beams; `sky_bastion`'s seed `a25e084f` was rejected for the same thing in batch 5). Two
rewordings did not remove them. The batch stopped under the plan's stop rule 8 and charter §1.2: the tool fell below
the required quality, and the question went to the owner as
[Q0131](../../questions/closed/0131-carrier-media-light-shafts.md). Nothing was installed, baked or added to
`assets/media/required_set.ron`. `mothership_carrier` and `saucer_interceptor` stay without media, so the client keeps
the Carrier locked (`CardLock::NoMedia`; H-16: a hangar unit is presentable only with its member's media). That is
truthful: no stand-in was made.

## Scope

`UnitCatalog::publishable_actions` gives both identities Idle, Walk and Attack (mobile, armed: the Carrier through
its launch weapon). The plan's batch: 6 clips, at most 25 generations or $10.00.

## Authored texts (before any spend)

The authored texts named a cloudscape (a scene), a "massive shadow" (a floor to cast it on), "spinning rim lights" (a
turning part), "a fast banking glide" and "a strafing pass" (travel and a turn). They were rewritten in `units.ron` on
the batch-5 aircraft pattern (one aircraft seen from above at three-quarter, nose to the lower right, whole in the
frame), and `media_style.ron` gained the two identities' overrides (a still hover, flight in place shown by light and
vapour trails, an Attack that fires or launches toward the lower right and returns to the hover pose), the letterbox
(feather 30 ‰) and Attack pinned at its end, all explained in the file. Every wording passed `validate-prompts` (the
1,024-character limit) before the run that used it. The golden list of unit prompts
(`crates/asset_baker/tests/fixtures/unit_prompts_golden.txt`) changes in exactly the six lines of these identities
(the three `saucer_interceptor` lines are new: WP3 added the row without them).

## Preflight

`asset_baker veo-preflight --credentials-dir <main checkout>`: mode Vertex AI Express, model
`veo-3.1-fast-generate-001`, HTTP 404, "Model visible: no". This is the result the preflight has given every day since
2026-11-04: Express keys cannot read the model resource but can generate
([Q0042](../../questions/closed/0042-veo-preflight-404.md), answered (a); the pilot proved it). The budget check of
`generate-veo --dry-run` read $113.65 spent of $230.00 from the ledger before the first run.

## Spending

Four runs of the release binary, each one generation per identity (Idle only, `--seed 1` to `4`, `--allow-regeneration`
after the first), `--credentials-dir <main checkout>`; logs `~/scratch/carrier/veo-logs/r0{1..4}-seed-idle.log`, run
ids `20261126T001919Z-1853791`, `20261126T002115Z-1865846`, `20261126T002253Z-1876065`, `20261126T002430Z-1902022`.
**8 generations, 8 × $0.40 = $3.20** at list price (Veo 3.1 Fast, 720p, with sound); the tool reported after the last
run: **$116.85 spent of $230.00, $113.15 remaining.** Every result came back inline, with no `gcsUri`, no filtered
result, no quota refusal and no failure; all eight clips are 4 s, 1280 × 720, 96 frames. The logs hold no key-like
string.

## Per clip

Review: `asset_baker review` (16:9 and framed sheets; `loop-check` of the Idle, raw and repaired). Sheets are under
[`review/`](review/) as `<slug>_idle_<operation>{,_framed,_loopcheck_repaired,_loopcheck_seam}.png`. The full reason
of every rejection is in its `rejected_in_review` ledger line.

| Identity | Attempt | Operation | Clip SHA-256 | Seed | Wording | Loop-check | Verdict |
|---|---|---|---|---|---|---|---|
| `saucer_interceptor` | A1 | `0cc51230` | `3ce8544e` | 1 | first text | repaired PASS | rejected: diagonal light streaks; seen nearly level |
| | A2 | `3ddf92e4` | `53ac1c38` | 2 | "seen from high above … evenly lit empty space" | PASS | rejected: faint light shafts; the view is right |
| | A3 | `7510266f` | `dbc5be3d` | 3 | "floating still … softly lit from the front" | repaired PASS (push-in +4.5 %) | rejected: light shafts |
| | A4 | `aa2190cf` | `79539eda` | 4 | "a round disc-shaped drone … nothing shines down on it" | repaired PASS (push-in +3.9 %) | rejected: bright light shafts |
| `mothership_carrier` | A1 | `cdac2745` | `90b1cc19` | 1 | first text | repair refused (uneven zoom) | rejected: strong god rays |
| | A2 | `67905046` | `64b884d6` | 2 | as the interceptor's A2 | repair refused (+3.4 %) | rejected: light shafts |
| | A3 | `770a880f` | `65ff0c55` | 3 | as A3 | repair refused (+4.1 %) | rejected: strong light shafts |
| | A4 | `edd27ce5` | `2d9a2d74` | 4 | as A4 | repaired PASS | rejected: faint light shafts |

Both designs read well (a small golden disc drone with an amber core and rim lights; a wide golden carrier disc with
a ring of amber bays and a cyan core), and both are distinct from the Enclave's other saucers. The defect is the
backdrop alone, on every seed: a saucer seems to draw the light-shaft staging of flying-saucer pictures even against
the negative prompt (which already names light rays, god rays, rays of light and spotlight beams).

## What would continue the batch

Q0131 sets out the choices. The texts and overrides of this batch stay, so a continuation starts from them; 17
generations and $6.80 of the batch's cap remain.

## Continuation under the owner's answer to Q0131 (2026-11-26)

The owner answered [Q0131](../../questions/closed/0131-carrier-media-light-shafts.md): option (a) with a hard cap of
4 more seeds per identity, then option (b), the best available seed; faint light shafts are accepted as a temporary
exception, and a polish pass will redo the videos. Run from branch `carrier-media` (cut from `main` `0f0608cd`).

**Per-identity negative terms.** `media_style.ron`'s `actions.negative_overrides` (added by the broodlings package)
gives both identities "light shafts, tractor beam, beam of light from above, volumetric light, haze, light streaks".
Seeds 5 and 6 kept the texts; for seeds 7 and 8 the two Idle texts in `units.ron` were reworded on the Brood Lord
analogue's pattern ("held still, seen from above at a three-quarter angle looking down on its top … plain dark
backdrop, lit softly from the front"; no "floating" or "high above"), and the golden list of unit prompts changes in
exactly those two lines. Every wording passed `validate-prompts` before the run that used it.

**Preflight.** `veo-preflight`: Vertex AI Express, HTTP 404, "Model visible: no", as every day since 2026-11-04
(Q0042). `generate-veo --dry-run` read **$123.25 spent of $230.00** before the first run.

**Spending.** Five runs of the release binary, `--credentials-dir <main checkout>`; logs
`~/scratch/carrier-media/veo-logs/r0{5..9}-*.log`; run ids `20261126T103259Z-1532138` (2 Idles, seed 5),
`20261126T103422Z-1540341` (2, seed 6), `20261126T103601Z-1549844` (2, seed 7), `20261126T103729Z-1566151` (the
interceptor's Idle, seed 8), `20261126T103836Z-1593945` (6 pinned clips, seed 9). **13 generations, $5.20; the batch:
21 generations, $8.40 of its $10.00 cap (4 generations left unused); the ledger: $128.45 of $230.00.** Every result
came back inline, 4 s, 1280 × 720, 96 frames; no filtered result, quota refusal or failure; no key-like string in the
logs or the new ledger lines.

| Identity | State | Operation | Clip SHA-256 | Seed | Start | Loop-check | Verdict |
|---|---|---|---|---|---|---|---|
| `saucer_interceptor` | Idle A5 | `d249c10a` | `bdeed039` | 5 | text + identity terms | repaired PASS | rejected: diagonal light shafts at the upper right |
| | Idle A6 | `b97dddcd` | `0988152c` | 6 | as A5 | worst 0.892 < 0.904 | rejected: light shafts; propeller-like parts on its dome |
| | Idle A7 | `195c6479` | `6b0fd0c7` | 7 | reworded text | worst 0.867 < 0.870 | rejected: light shafts; rotors on its top |
| | Idle A8 | `cbe197b6` | `34aaf27b` | 8 | as A7 | worst 0.877 < 0.885 | rejected: light shafts; quadcopter-like rotors; the last seed |
| | (pin) | `3ddf92e4` | `53ac1c38` | 2 | first batch's A2 | | the best available seed (faint shafts, the view right): **pin** frame 0, SHA-256 `b303d4b9…` |
| | Idle | `b9cb72f1` | `eca736b1` | 9 | pin, both ends | repaired wrap 0.999, worst 0.912 < 0.934 FAIL; fidelity 0.995 | **accepted (exception)**: still; the rim lights blink; faint shafts at the upper right |
| | Walk | `323525cb` | `d181fb8d` | 9 | pin, both ends | raw worst 0.875 < 0.882, growth +2.43 % FAIL; repaired without scale lock wrap 0.969 | **accepted (exception)**: amber light trails stream to the upper left, appearing after the first frame and gone by the last; faint shafts |
| | Attack | `7f78344e` | `1fe46c47` | 9 | pin, both ends | one-shot | **accepted**: two amber beams fire toward the lower right, then it hovers in the pose |
| `mothership_carrier` | Idle A5 | `901d34c9` | `80d6a68c` | 5 | text + identity terms | repair refused (1.25 %) | rejected: faint shafts at the top; seen nearly level |
| | Idle A6 | `ebc5154c` | `437cf00b` | 6 | as A5 | repair refused (2.75 %) | rejected: turns in place, seen nearly level (backdrop nearly plain) |
| | Idle A7 | `4b5d07ee` | `046d1853` | 7 | reworded text | repair refused (+7.7 % push-in) | rejected as a clip, taken as the **pin** (frame 0, SHA-256 `106e64bb…`): **no light shafts**, seen from above at 3/4; a soft dark shadow to the lower left |
| | Idle | `727f5501` | `17ca3ffc` | 9 | pin, both ends | repaired wrap 0.999, worst 0.976 PASS; fidelity 0.994 | **accepted**: still; the bays dim and glow once |
| | Walk | `36b3207c` | `3ed57646` | 9 | pin, both ends | repaired wrap 0.999, worst 0.958 PASS | **accepted**: vapour trails stream to the upper left, appearing after the first frame and gone by the last |
| | Attack | `25a9e6ce` | `ee5b3f63` | 9 | pin, both ends | one-shot | **accepted**: the front bays flash and a stream of small golden drones flies out to the lower right, then it hovers in the pose |

Sheets: the rejected seeds as `<slug>_idle_<operation>{,_framed,_loopcheck_repaired,_loopcheck_seam}.png`, the
accepted clips as `<slug>_<state>{,_framed,_repaired,_seam}.png`, under [`review/`](review/). The full reason of every
rejection is in its `rejected_in_review` ledger line.

**Flaws accepted as the temporary exception (Q0131), each recorded:** no interceptor seed was free of light shafts,
so its pin and all three of its clips show faint diagonal shafts at the upper right of the backdrop. Its Idle's worst
loop step is 0.022 under the threshold (the rim lights blink), and its Walk's light trails appear after the pinned
first frame and vanish before the pinned last one, so the loop shows them for most of its length and not at the seam
(an uneven zoom of +2.43 %: `media_style.ron`'s loops give it no scale lock and a cross-fade of 2, so the bake's seam
repair is not refused). The Carrier's Walk has the same trail timing but passes the loop criterion. The Carrier's pin
and clips show a soft dark shadow falling to the lower left on a slightly graded backdrop, with no light shafts. Both
Attacks are clean.

**Installed and baked through the pipeline.** `install-clip` for the six clips; `bake-unit-videos` for both identities
(letterboxed, feather 30 ‰; logs `~/scratch/carrier-media/veo-logs/bake-*.log`); the pins committed as
`assets/ai_keyframes/{mothership_carrier,saucer_interceptor}_idle.png`; both added to `assets/media/required_set.ron`
as `Expansion`; `crates/client/tests/data/flipbook_coverage.ron` lists both atlases. `audit-media --strict`: **34 of 34
verified**, both identities `Verified [Expansion]`. The client unlocks the Carrier (H-16: a hangar unit is presentable
only with its member's media). H-20, the client's steady state with interceptors cycling, is not written here: it
waits for the render-entity pool (B-237), as the Broodlings' B-19 does.

**Tests.** `cargo test -p asset_baker` (including `bake_integrity_tests`, `prompt_tests`, `media_tests`), `sim_core`'s
`golden_replay_tests` (goldens and worlds untouched: no simulation change), and the client's `runtime_media_tests`,
`runtime_assets_tests`, `battle_flipbook_tests`, `round2_visual_tests` and `smog_golden_tests` (on the physical GPU)
pass; log `~/scratch/carrier-media/veo-logs/media-tests.log`.
