# Veo: the broodlings batch, the Brood Lord analogue and its Broodling (2026-11-26)

Evidence for WP6 of plan [`2026-11-28-broodlings`](../../plans/2026-11-28-broodlings.md) (D-12), run on 2026-11-26
(UTC) from branch `broodlings-plan` (WP5 `41ae62a2`), with the method of batches 1 to [6](2026-11-09-batch-6.md) and
the lesson of the [carrier batch](2026-11-26-batch-carrier.md): seeds first and few (at most four per identity), the
pin from an accepted seed's first frame, the pinned states after it, and the review against the written criteria.

**First result: the batch stopped at the seed Idles, and no clip was accepted** (the continuation, below, installs six clips). `spore_matriarch` used its four seeds: the
first was seen nearly level with its left wing cut by the frame edge (the backdrop was plain), and the other three,
whose texts asked for a view from above, drew diagonal light shafts across the backdrop, which breaks the owner's
backdrop rule, as the Carrier's saucers did (Q0131). Every seed flapped its wings although the Idle asks for a still
pose. With no seed left, the identity cannot be pinned, and without it a Broodling's media would show nothing in a
match (it exists only by the Brood Lord analogue's spawn). `brood_mite`'s first seed gave the design wanted and was
taken as its pin; three Idles pinned to it failed the loop criterion (the body flushed in colour once, then the head
and claws nodded). The batch stopped under the plan's D-12 seed rule and charter §1.2: the tool fell below the
required quality. Nothing was installed, baked or added to `assets/media/required_set.ron`, and the Broodling's pin
(frame 0 of `7a642fe5`, SHA-256 `46ce66af5f73a4f3c915fd34dedb3af69f5b1e98856ba365b1891db2b0539c8b`) is kept at
`~/scratch/broodlings/pins/brood_mite_idle_pin.png`, not under `assets/`. `spore_matriarch` stays without media, so the
client keeps it locked (`CardLock::NoMedia`; B-17: a spawning unit is presentable only with its spawn's media). That
is truthful: no stand-in was made. The question went to the owner as
[Q0133](../../questions/closed/0133-brood-lord-media-light-shafts.md).

## Scope

`UnitCatalog::publishable_actions` gives both identities Idle, Walk and Attack (mobile, armed). The plan's batch: 6
clips, at most 25 generations or $10.00, at most 4 seed Idles per identity.

## Authored texts (before any spend)

`spore_matriarch`'s texts named "high above ground" and "displacing air" and were rewritten as a creature (not an
aircraft), with no sky, cloud, "high above", "hovering" or lit-from-above wording. `brood_mite` (new in WP4) follows the
scorpion's pattern. `media_style.ron` gained both identities' overrides (a still pose, a glide or a scuttle in place,
an Attack that flings two larvae or stabs toward the lower right and returns to the pose), the letterbox (feather 30
‰), Attack pinned at its end and the Broodling's Walk without scale lock or cross-fade, all explained in the file. The
global and per-kind negative terms are unchanged (Q0131 is open). Every wording passed `validate-prompts` before the run
that used it. The golden list of unit prompts changes in exactly the six lines of these identities.

## Preflight

`asset_baker veo-preflight --credentials-dir <main checkout>`: mode Vertex AI Express, model
`veo-3.1-fast-generate-001`, HTTP 404, "Model visible: no", the result since 2026-11-04 (Express keys cannot read the
model resource but can generate; [Q0042](../../questions/closed/0042-veo-preflight-404.md)). The budget check of
`generate-veo --dry-run` read **$116.85 spent of $230.00** from the ledger before the first run.

## Spending

Four runs of the release binary, each one Idle per identity (`--seed 1` to `4`, `--allow-regeneration` after the
first), `--credentials-dir <main checkout>`; logs `~/scratch/broodlings/veo-logs/r0{1..4}-*.log`, run ids
`20261126T052825Z-1230457`, `20261126T053015Z-1239748`, `20261126T053152Z-1245996`, `20261126T053349Z-1254765`.
**8 generations, 8 × $0.40 = $3.20** at list price (Veo 3.1 Fast, 720p, with sound); the tool reported after the last
run: **$120.05 spent of $230.00, $109.95 remaining.** Every result came back inline, with no filtered result, no quota
refusal and no failure; all eight clips are 4 s, 1280 × 720, 96 frames. The logs and the new ledger lines hold no
key-like string.

## Per clip

Review: `asset_baker review` (16:9 and framed sheets; `loop-check` of the Idle, the pinned ones against the pin). The
sheets under [`review/`](review/) are of the last clip of each identity
(`<slug>_idle{,_framed,_repaired,_seam}.png`); the full reason of every rejection is in its `rejected_in_review` ledger
line.

| Identity | Attempt | Operation | Clip SHA-256 | Seed | Start | Loop-check | Verdict |
|---|---|---|---|---|---|---|---|
| `spore_matriarch` | A1 | `15dd8a58` | `2f66934f` | 1 | text | repair refused (11.4 %) | rejected: seen nearly level, lit floor, left wing tip cut; backdrop plain |
| | A2 | `d9af8f6d` | `2fb22267` | 2 | text ("seen from above looking down on its back") | repair refused (10.7 %) | rejected: diagonal light shafts; wings flap |
| | A3 | `9e0c2119` | `cff42dbd` | 3 | text ("like a museum specimen … evenly lit") | repair refused | rejected: strong radiating light shafts; front view |
| | A4 | `c75dcc96` | `ac6bea13` | 4 | text (A1's lighting, view from above) | repair refused (9.5 %) | rejected: light shafts; wings flap; last seed |
| `brood_mite` | A1 | `7a642fe5` | `d2edc2fb` | 1 | text | worst 0.781 < 0.792 | rejected as a seed: the design wanted. **Pin** `46ce66af…` |
| | pin A1 | `6cd69ec4` | `c0897958` | 2 | pin, both ends | repair refused (1.03 %) | rejected: the segments flush pink and fade |
| | pin A2 | `c985ed0a` | `19ded3d2` | 3 | pin, both ends ("colours … never change") | repair refused (1.11 %); fidelity 0.996 | rejected: head and claws nod |
| | pin A3 | `b0f92e7e` | `77976fe1` | 4 | pin, both ends ("head … never move, turn or nod") | repair refused (1.35 %) | rejected: head and claws still nod |

The Broodling's design reads well (a pale segmented larva with dark chitin plates, hooked legs and two needle claws,
plain backdrop, a soft shadow). The Brood Lord analogue's design reads well too (a green manta-ray beast with amber
spore pods), but a winged creature seen from above draws the same light-shaft staging as the saucers, against negative
terms that already name light rays, god rays and spotlight beams.

## What would continue the batch

Q0133 sets out the choices. The texts and overrides of this batch stay, so a continuation starts from them; 17
generations and $6.80 of the batch's cap remain. B-19 (the client's steady state with Broodlings cycling) waits for
these media, as the Carrier's H-20 does.

## Continuation under the owner's answer to Q0133 (2026-11-26)

The owner answered [Q0133](../../questions/closed/0133-brood-lord-media-light-shafts.md): up to 4 more seeds per
identity, with per-identity negative terms (Q0131 (a)) or a clean start still for framing, then the best available
seed; faint light shafts and minor staging flaws are accepted as a temporary exception, and a polish pass will redo
the videos.

**Per-identity negative terms.** `asset_baker` gained `actions.negative_overrides` in `media_style.ron` (exact slug →
nouns appended after the kind's negative terms, for every clip of that identity; an unknown slug or blank terms are
refused at load; tests `negative_overrides_append_to_one_identitys_clips` and
`invalid_negative_overrides_are_refused_at_load` in `prompt_tests.rs`). `spore_matriarch` takes "light shafts,
tractor beam, beam of light from above, volumetric light, haze, flapping wings"; `brood_mite` "head turning, nodding,
twitching, colour change". The global and kind terms are unchanged, so no other identity's prompt moved.

**Spending.** The ledger read $120.05 before the continuation. Two runs: `20261126T085857Z-172874` (2 Idles, seed 5)
and `20261126T090121Z-190105` (6 pinned clips, seed 6); logs `~/scratch/broodlings/veo-logs/r05-idle.log`,
`r06-pinned.log`. **8 generations, $3.20; the batch: 16 generations, $6.40 of its $10.00 cap; the ledger: $123.25 of
$230.00.** Every result came back inline, 4 s, 1280 × 720, 96 frames; no key-like string in the logs.

| Identity | State | Operation | Clip SHA-256 | Seed | Start | Loop-check (repaired) | Verdict |
|---|---|---|---|---|---|---|---|
| `spore_matriarch` | Idle | `a5d6f9b6` | `d8154ef4` | 5 | text + identity terms | repair refused (wings flap) | rejected as a seed, taken as the **pin** `473270ff…`: seen from above at 3/4, plain backdrop with faint light shafts at the top right, the left wing tip touching the frame edge |
| | Idle | `2db3d6ab` | `a18bc0ed` | 6 | pin, both ends | wrap 0.984, growth +17.65 % FAIL | **accepted (exception)**: the wings beat slowly in place, faint shafts at the top |
| | Walk | `3eb844b5` | `82563d03` | 6 | pin, both ends | wrap 0.954, worst 0.714 < 0.720, growth +17.65 % FAIL | **accepted (exception)**: wing beats in place, faint shafts |
| | Attack | `8d7bb6cb` | `9c40f57d` | 6 | pin, both ends | one-shot | **accepted**: two pale larvae flung to the lower right, back to the pose |
| `brood_mite` | Idle | `d0ea6caf` | `a5a0d307` | 5 | pin + identity terms | worst 0.847 < 0.849, growth −1.84 % FAIL | rejected (kept aside): the head still nods slightly |
| | Idle | `a05e5b81` | `2abf143e` | 6 | pin, both ends | wrap 0.999, worst 0.826 < 0.879, growth −2.07 % FAIL | **accepted (exception)**: still, colours steady, a slight nod of the head |
| | Walk | `5f036fff` | `a8f21daa` | 6 | pin, both ends | wrap 0.819, worst 0.752 < 0.771 FAIL | **accepted (exception)**: the legs step in place, the body holds |
| | Attack | `d57c22e3` | `95a47aab` | 6 | pin, both ends | one-shot | **accepted**: it rears and stabs with both claws toward the lower right, then settles |

**Flaws accepted as the temporary exception (Q0133), each recorded:** the Brood Lord analogue's Idle and Walk beat
their wings (an uneven zoom of +17.65 % that no seam repair can lock) and show faint light shafts at the top of the
backdrop; its pin's left wing tip touches the frame edge. The Broodling's Idle nods its head slightly and its Walk's
worst step is 0.019 under the loop threshold. All four loops wrap smoothly (0.82 to 0.999). `media_style.ron`'s loops
give the Brood Lord analogue's Idle and Walk no scale lock and a cross-fade of 2, so the bake's seam repair is not
refused; the Broodling keeps the default Idle and its Walk's no scale lock.

**Installed and baked through the pipeline.** `install-clip` for the six clips; `bake-unit-videos` for both identities
(letterboxed, feather 30 ‰; logs `~/scratch/broodlings/veo-logs/bake-*.log`); the pins committed as
`assets/ai_keyframes/{spore_matriarch,brood_mite}_idle.png`; both added to `assets/media/required_set.ron` as
`Expansion`; `crates/client/tests/data/flipbook_coverage.ron` lists both atlases. `audit-media --strict`: **32 of 32
verified**, both identities `Verified [Expansion]`. `cargo test -p asset_baker` (including `bake_integrity_tests`,
`prompt_tests`, `media_tests`) and the client's `runtime_media_tests`, `runtime_assets_tests`, `battle_flipbook_tests`,
`round2_visual_tests test_flipbook_textures_512_invariants` and `smog_golden_tests` pass. The client unlocks
`spore_matriarch` (B-17: it is presentable with its Broodling's media). 9 generations ($3.60) of the batch's cap remain.
