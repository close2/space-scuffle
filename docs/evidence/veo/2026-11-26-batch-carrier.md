# Veo: the carrier batch, the Carrier and its interceptor (2026-11-26), stopped for the owner

Evidence for WP5 of plan [`2026-11-28-carrier-interceptors`](../../plans/2026-11-28-carrier-interceptors.md) (D-11),
run on 2026-11-26 (UTC) from branch `carrier-interceptors-plan` (WP4 `43b0529e`), with the method of batches 1 to
[6](2026-11-09-batch-6.md): authored texts on the batch-5 aircraft pattern, the seed clip, then the pin, then the
pinned states, and the D-7 review.

**Result: the batch stopped at the seed Idles, and no clip is accepted.** Four seeds of each identity, eight
generations, all drew diagonal light shafts across the backdrop, which breaks the owner's backdrop rule (no radiating
stripes, rays or spotlight beams; `sky_bastion`'s seed `a25e084f` was rejected for the same thing in batch 5). Two
rewordings did not remove them. The batch stopped under the plan's stop rule 8 and charter §1.2: the tool fell below
the required quality, and the question went to the owner as
[Q0131](../../questions/open/0131-carrier-media-light-shafts.md). Nothing was installed, baked or added to
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
