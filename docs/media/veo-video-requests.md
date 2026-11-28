# Veo video requests for an outside generator

- **Written:** 2026-11-28, from `main` at `1c0f922c`
- **For:** an agent outside this repository that generates Veo clips with its own tokens and adds them to the branch
  `veo-media-requests`
- **Reviewed and installed by:** this project's orchestrator, after hand-in (§6)

This document is self-contained. It says which clips to make, the exact request for each, how to hand them in, and
the bar each one must clear. Where something is not yet decided here, it names the default to follow.

---

## 1. Ground rules

1. **Real generated video only.** Every clip is the untouched file the Veo service returned. No clip may be drawn,
   rendered, composited, re-timed, trimmed, re-encoded, looped by hand, interpolated, or made by moving a still
   picture. No clip may be passed off as a different identity's clip. This is the project charter's rule against fake
   media ([`AGENTS.md`](../../AGENTS.md) §1.2). The baker trims, crops, letterboxes and repairs loop seams itself, so
   you never need to.
2. **When a clip will not come out right, regenerate it.** If no attempt passes (§5), hand in the best one, marked
   `best_effort`, with your notes. Never fake the missing clip.
3. **When you are blocked, stop and say so.** This applies to a refused key, a service error, a quota, a filtered
   result, or a missing start picture. Write it in the hand-in notes (§4.3) and send nothing in its place.
4. **Cost.** Your generations cost this project nothing. The project's spend cap, `assets/media/veo_budget.ron`
   ($230 total, $128.45 spent on 2026-11-28), does not apply to them and is not charged. Record every attempt anyway,
   rejected ones included (§4.3), so that the project's media history stays complete.
5. **Touch only `docs/media/veo-handin/`.** Do not edit `assets/`, `docs/evidence/veo/ledger.jsonl`, any `.ron`
   catalog, or any code. The orchestrator moves the accepted files into place (§6).
6. **No secrets in the repository.** Never commit a key, a token, a signed URL or an account identifier. Operation
   names are fine.

---

## 2. Request parameters shared by every clip

These are the values the project's own pipeline sends, from `asset_baker generate-veo` and
[`assets/media/veo_request.ron`](../../assets/media/veo_request.ron). Use them unchanged.

| Parameter | Value | Notes |
|---|---|---|
| Model | `veo-3.1-fast-generate-001` (Veo 3.1 Fast) | Every installed clip uses this model. Veo 3.1 standard (`veo-3.1-generate-001`) is allowed for a clip that keeps failing on Fast. Record which model you used. |
| Aspect ratio | `16:9` | The baker fits it into the square game frame. |
| Resolution | 720p (1280×720) | The pipeline sends no resolution field, and the service returns 720p. Do not ask for 1080p or 4K. |
| Duration | `4` seconds | |
| Sample count | `1` per request | One clip per request, so each attempt is recorded on its own. |
| Frame rate | 24 fps (what the service returns) | Every installed clip is H.264, 1280×720, 24/1 fps, 4.0 s, with an AAC audio track. |
| Audio | On | Send no `generateAudio` field, or send `true`. The owner keeps sound for click feedback. Never ask for a silent clip. |
| Start picture (`image`) | The PNG named per clip (§3) | Image-to-video. Send the file's bytes unchanged. |
| End picture (`lastFrame`) | The same PNG as the start picture | Pins the loop: the clip must end on the picture it starts from. If your API cannot send `lastFrame`, say so in the notes. Do not fake the end. |
| Seed | Any; record it | Use a different seed for each attempt. If your API takes no seed, record `null`. |
| Negative prompt | As given per clip (§3) | Send it as `negativePrompt`. |
| Camera, view, background | Set by the prompt text | A static tripod shot, a 3/4 isometric view from above, and a plain flat dark slate-indigo backdrop (#0B101B) with no floor, shadow, rays or gradient. The backdrop is not keyed: the baker letterboxes the clip with bars of the backdrop's own colour, so the backdrop must stay one even colour from edge to edge. |

The prompt is always three parts in this order: the identity's authored text, then the action text for the state,
then the house style sentence. §3 gives the text already joined. Send it character for character: the
`prompt_sha256` values below are the SHA-256 of that UTF-8 text, and they are how the project matches a clip to its
request.

---

## 3. The clips

There are **two clips**.

| # | Identity | State | Why | Install name |
|---|---|---|---|---|
| 1 | `brood_mite_escort` (provisional, see 3.1) | Idle (a looping hover) | Owner answer Q0144 (b): the Brood Lord analogue's two escorts get their own real Veo clip | `brood_mite_escort_idle.mp4` |
| 2 | `colossus_mech` | Walk (a looping march in place) | The only missing state of a unit whose Idle and Attack are installed. Without a Walk it cannot be baked, so it is never shown in a battle (`DESIGN.md` §4, the "Media coverage" row) | `colossus_mech_walk.mp4` |

Nothing else is requested; §7 explains why.

### 3.1 Clip 1: the Brood Lord analogue's flying escort

**Source.** [`docs/questions/open/0144-brood-lord-escort-look.md`](../questions/open/0144-brood-lord-escort-look.md),
answered (b) by the owner, and plan
[`2026-11-27-missile-throws-and-escorts`](../plans/2026-11-27-missile-throws-and-escorts.md), Amendment 1 and D-8.
The Brood Lord analogue (`spore_matriarch`) carries two small escorts behind it, at ±150° from its facing. In the
reference game they are its Broodlings carried in flight. The project's Broodling analogue is `brood_mite`: a small
pale segmented larva that already has real clips. The escort is the **same creature, carried through the air**. In
the game it is drawn as a small bubble behind the host. It loops while the escort is held and disappears when the
escort is thrown.

**Identity name (not yet decided).** The escort is not in the catalog yet. Plan WP4 adds it as its own presentation
"look" with its own flipbook. Default: name the clip `brood_mite_escort`. If WP4 picks another slug, the orchestrator
renames the file; you change nothing.

**Start and end picture.** [`assets/ai_keyframes/brood_mite_idle.png`](../../assets/ai_keyframes/brood_mite_idle.png):
1280×720 PNG, SHA-256 `46ce66af5f73a4f3c915fd34dedb3af69f5b1e98856ba365b1891db2b0539c8b`. This is the `brood_mite`
Idle pin, so the escort keeps the Broodling's silhouette and palette. Send it as both `image` and `lastFrame`.

*If no pinned attempt reads as flight:* the crouched pin may keep looking grounded. In that case you may make one
text-to-video seed with the same prompt, with no start picture. Hand in its frame 0 as a 1280×720 PNG named
`brood_mite_escort_idle.png` in the clip folder (§4.1). Then generate the pinned loops on that picture. Record both
steps. The seed clip itself is a pin source, not a loop, and must not be handed in as one.

**Prompt** (982 characters, `prompt_sha256` `0485a3020754b7926e9a45052f5a3811a7a8994bd91bedccca2925cfd6b2028b`):

```text
Brood Mite Escort: the same small pale segmented larva as the Brood Mite, dark chitin plates, short hooked legs and two curved needle claws, carried through the air in the crouched pose of the start picture, seen from above at a three-quarter angle, front toward the lower right, whole in the middle of the frame, lit softly from the front. Idle animation, one static tripod shot: the camera never moves, zooms or pans, and the larva keeps the same size, pose and place in frame. It floats in place with a slow, gentle, even bob, legs and claws held still; it never turns or tilts and its colours never change. The plain dark backdrop stays unchanged. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

This text is new, so the pipeline did not print it. It is written to the house pattern of
[`assets/catalog/media_style.ron`](../../assets/catalog/media_style.ron) and follows the `brood_mite` Idle override.
It also avoids the words that drew light shafts in earlier batches ("hovering", "high above", sky, clouds, light from
above). The orchestrator adds it to the catalog together with WP4. If you change a word, record the exact text you
sent (§4.3).

**Negative prompt.** This is the project's global text, then `brood_mite`'s own terms, then the light-shaft terms the
flying identities need:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry, head turning, nodding, twitching, colour change, light shafts, tractor beam, beam of light from above, volumetric light, haze, floor, shadow
```

**Loop treatment the baker will apply.** This is the project default for a looping Idle: both ends pinned, a scale
lock, and a 2-frame cross-fade. Judge your attempts with that in mind (§5).

**Framing.** Letterboxed into the square, as `brood_mite` is. The whole creature must stay inside the 16:9 frame,
with a clear margin on every side.

**Attempts.** The owner's cap of "about 4 generations, at most $4.00" was set to limit this project's spending. Your
attempts are free, so that cap does not bind you. Default: up to **8 attempts**. Stop at the first one that passes §5.
If none passes, hand in the best one marked `best_effort`. Record every attempt.

### 3.2 Clip 2: `colossus_mech` Walk

**Source.** `DESIGN.md` §4, the "Media coverage" row: "`colossus_mech` has an Idle and an Attack but no accepted
Walk". The open item is in [`docs/evidence/veo/2026-11-09-batch-5b.md`](../evidence/veo/2026-11-09-batch-5b.md),
"For the next session", item 1. The free dry run (`asset_baker generate-veo --unit colossus_mech --dry-run`) plans
exactly this one clip as `regenerate`. It also prints `Prompts valid for 1 clip(s) to generate`.

**Identity.** The Thor analogue, "Titan AA-Walker": a heavy, wide, two-legged mech with two arm-cannons, seen at 3/4.
It faces the lower right of the frame.

**Start and end picture.**
[`assets/ai_keyframes/colossus_mech_idle.png`](../../assets/ai_keyframes/colossus_mech_idle.png): 1280×720 PNG,
SHA-256 `2e3076d43adda5c42ac569ecd172c7927107a755d2ee934ad9e91c9b795725f4`. This is the identity's Idle pin, the same
picture the installed Idle and Attack start from. Send it as both `image` and `lastFrame`.

**Prompt**, exactly as the dry run prints it (930 characters, `prompt_sha256`
`4a1d741d6ccc709ff493b93923c22317ce88e6bb166fc920f7cf7c076335c652`, the same hash as the last Walk request in the
project's ledger):

```text
Titan AA-Walker: Heavy lumbering marching steps of its massive mechanical legs. Walk animation, one static tripod shot: the camera never moves, zooms or pans, and the mech never turns, leans or steps toward the camera: its chest and feet keep facing the lower right of the frame, at the same place and size, from the first frame to the last. It marks time on the spot without travelling: short heavy steps, left, right, left, right, each foot lifting a little and stamping back down in the same footprint at an even pace, both arm-cannons hanging low at its sides. Nothing else appears in the frame. Seamless loop: the last frame matches the first. Stylized hand-painted 3D character, painterly sci-fi aesthetic with crisp rim lighting, plain flat uniform dark slate-indigo studio backdrop (#0B101B) with no pattern, no rays, no stripes and no gradient, 3/4 isometric front view, glowing cyan/amber energy cores, bold proportions.
```

**Negative prompt**, as the dry run prints it:

```text
radiating stripes, sunburst, light rays, god rays, rays of light, stripes, spotlight beams, patterned background, gradient background, vignette, camera movement, zoom, dolly, push-in, pan, text, watermark, morphing, shape-shifting geometry
```

**Loop treatment the baker will apply** (`media_style.ron`, the `loops` overrides): both ends pinned, no scale lock,
no cross-fade. The raw frames must loop on their own.

**Framing.** Letterboxed into the square (the `framing` override). The whole mech, both cannons included, must stay
inside the frame.

**What went wrong before.** Seven earlier Walks were rejected. Each one either turned the mech toward the camera,
stepped forward and back, or grew in frame by 2.4 to 4.0 % (the limit is 1.5 %). A clip that passes is a march on the
spot that keeps the three-quarter facing and the size of the picture.

**Attempts.** Default: up to **12** with the prompt above. If none passes, you may try the batch-5b evidence's option
(c): add, right after `It marks time on the spot without travelling:`, the words
`its body held perfectly still as if hung from a crane, only the legs moving:`. Record the exact text you sent. Up to
6 more attempts. If still none passes, hand in the best one marked `best_effort`. Do not make a new start picture: a
new picture would withdraw the installed Idle and Attack.

---

## 4. Hand-in format

### 4.1 Folder layout

Commit to the branch `veo-media-requests`, under this folder only:

```text
docs/media/veo-handin/
  brood_mite_escort/
    idle/
      handin.json                        # required, see 4.3
      <operation-stem>.mp4               # each attempt you propose, 1 to 3 files
      brood_mite_escort_idle.png         # only if you seeded a new pin (3.1)
  colossus_mech/
    walk/
      handin.json
      <operation-stem>.mp4
```

- **`<operation-stem>`** is the last path segment of the service's operation name, or an 8-character id you choose
  if the service gives none. It must use only `[A-Za-z0-9_-]`, and it must be the same id as the attempt's
  `operation_stem` in `handin.json`.
- **Commit only the proposed attempts.** That is the one you rank best, plus at most two runners-up. List rejected
  attempts in `handin.json`, but do not commit their files.
- **Raw `.mp4` goes here, in staging, not under `assets/`.** Do not touch `assets/videos/`. Any file there named
  `<slug>_<state>.mp4` counts as installed. The orchestrator installs a clip only after review (§6).

### 4.2 Commits

- One commit per clip folder, for example `media(veo-handin): colossus_mech walk, 9 attempts, 1 proposed`.
- Commit unsigned, and do not rewrite or force-push existing history on the branch.

### 4.3 `handin.json`

One JSON object per clip folder. The orchestrator turns it into the project's ledger and evidence records, so you do
not write those yourself.

```json
{
  "format_version": 1,
  "slug": "colossus_mech",
  "state": "walk",
  "generator": {
    "environment": "Gemini API | Vertex AI | AI Studio | other (name it)",
    "agent": "short free-text description of who ran it, without account details",
    "cost_to_this_project_usd": 0
  },
  "attempts": [
    {
      "operation_stem": "a1b2c3d4",
      "operation": "full operation resource name, if the service gave one, else null",
      "date_utc": "2026-11-29T10:15:00Z",
      "model": "veo-3.1-fast-generate-001",
      "prompt": "the exact prompt text sent",
      "prompt_sha256": "lowercase hex SHA-256 of the UTF-8 prompt",
      "negative_prompt": "the exact negative prompt sent",
      "start_image_sha256": "SHA-256 of the image sent as the start picture, or null",
      "last_frame_sha256": "SHA-256 of the image sent as lastFrame, or null if none was sent",
      "parameters": { "aspectRatio": "16:9", "durationSeconds": 4, "sampleCount": 1, "seed": 7, "generateAudio": null },
      "file_sha256": "SHA-256 of the returned .mp4, or null if the service returned none",
      "file_committed": true,
      "verdict": "proposed | runner_up | rejected | best_effort | failed",
      "reason": "one line against the criteria in section 5"
    }
  ],
  "notes": "anything the reviewer should know: blocks, refusals, filtered results, prompt variants, pin seeding"
}
```

- List **every** attempt in the order it was made, including refused or filtered requests (`verdict: "failed"`,
  `file_sha256: null`).
- `file_sha256` must match the committed file byte for byte. The orchestrator checks it before anything else.
- `seed` is `null` when your API takes none.

---

## 5. Acceptance criteria (check before proposing a clip)

These are the project's written review criteria (D-7 of the Veo media plan, kept in force by the owner's answer to
[Q0026](../questions/closed/0026-veo-clip-acceptance.md)), the owner's chat rules, and charter §1.2. A proposed clip
meets all of them:

1. **Real and untouched.** It is the service's output file, with no edit of any kind.
2. **One subject**: exactly the identity, whole. No second creature or unit, no text, logo or watermark.
3. **Same object as its pin.** Same silhouette, proportions, plates, colours and lights as the start picture, from
   the first frame to the last. No morph, no cut, no change of design or colour.
4. **Static camera.** No zoom, push-in, pan or drift. The subject keeps its size and place in frame. Growth over the
   clip is at most 1.5 %.
5. **Plain backdrop.** One even dark slate-indigo colour to every edge. No floor, ground plane, cast shadow, light
   shafts, rays, radiating stripes, sunburst, gradient, vignette or scenery appears at any moment.
6. **The action is right, and reads small.**
   - Escort: clearly airborne, with a gentle bob. It never turns toward the camera. It reads as the Broodling at the
     size of a Broodling's bubble.
   - Walk: legs clearly lifting and stamping in place through the whole clip, facing the lower right. It never travels
     and never turns toward the camera. It reads at 512 px.
7. **Seamless loop.** The last frame matches the first (both clips are pinned at both ends), with no jump when the
   clip restarts. If you can build the repository's `asset_baker` (Rust, with `ffmpeg` and `ffprobe` installed), you
   can measure this yourself. `asset_baker loop-check --unit <slug> --state <state> --clip <file>` is free and works
   offline. It prints PASS or FAIL against the same thresholds the reviewer uses. `--unit` must be a catalog slug, so
   until the escort has its own entry, measure it as `--unit brood_mite --state idle --pin <the pin you used>`.
   `brood_mite` Idle has the same loop treatment the escort will get.
8. **Inside the frame.** The whole subject, with its weapons, claws and legs, stays inside the 16:9 frame with a
   margin.
9. **Sound present.** An AAC track as returned. Its content is not judged.

**Unusable clip:** reject it in `handin.json` with the criterion it fails, and generate another. A clip that fails
any of 1 to 5 is never proposed, not even as `best_effort`. Criteria 6 to 8 may be missed only by a `best_effort`
clip, and the notes must say how.

---

## 6. What happens here after hand-in

The orchestrator, or an agent it assigns, does these steps.

1. **Check the hand-in.** Each committed file's SHA-256 must match `handin.json`. `ffprobe` must report H.264,
   1280×720, 24 fps, about 4 s, and an audio track.
2. **Record provenance.** Today's pipeline cannot record a clip generated outside it. `install-clip` installs only a
   clip whose SHA-256 has a `downloaded` line in `docs/evidence/veo/ledger.jsonl`. `generate-veo --import` records a
   clip only for an operation this pipeline started. A baked atlas's provenance origin is either `Veo` (cited from the
   ledger) or `LegacyVeo` (a commit, meant for the original pre-ledger clips). Before installing, the orchestrator
   decides how to record these clips. It writes the ledger and evidence entries from `handin.json`; the outside agent
   writes neither. Recommended: a small `asset_baker` change, under a short plan note, that adds an
   "external generation" ledger event and provenance origin, so that `audit-media --strict` can verify these clips as
   it verifies the rest. The fallback, `bake-unit-videos --legacy-veo <state>=<hand-in commit>`, passes the strict
   audit, but it would label these clips as legacy work.
3. **Review.** `asset_baker review` writes contact sheets (the full 16:9 frames and the framed square) into
   `docs/evidence/veo/review/`, and `asset_baker loop-check` measures the loop on the frames the atlas will keep. A
   rejection is recorded with its reason. The owner can reject any shipped clip later, and it is then regenerated.
4. **Install.** The accepted file becomes `assets/videos/colossus_mech_walk.mp4`, or
   `assets/videos/brood_mite_escort_idle.mp4`, through `install-clip` or the path chosen in step 2. A seeded escort
   pin moves to `assets/ai_keyframes/`. The escort's prompt and its loop and framing entries go into
   `assets/catalog/media_style.ron` and the catalog with plan WP4.
5. **Bake.**
   - `colossus_mech`: `asset_baker bake-unit-videos --unit colossus_mech --idle … --walk … --attack … --ledger
     docs/evidence/veo/ledger.jsonl` builds the atlas, its manifest and `<slug>.provenance.ron` under
     `assets/textures/`.
   - The escort: the same command, with `--idle` only, once WP4 has given the escort its catalog entry.
   - Both identities are added to `assets/media/required_set.ron` as `Expansion`, and
     `asset_baker audit-media --strict --ledger docs/evidence/veo/ledger.jsonl` must report every required identity
     `Verified`.
6. **Tests that must pass on the result.**
   - `asset_baker`: `media_tests.rs`, `bake_integrity_tests.rs`, `provenance_tests.rs`, `veo_cli_tests.rs`,
     `loop_check_tests.rs`, `prompt_tests.rs` (the golden list of unit prompt hashes is updated when a prompt text
     joins the catalog).
   - `client`: `runtime_media_tests.rs` and `runtime_assets_tests.rs`, which load the real atlases from the runtime
     paths; the escort tests T-15 to T-18 of plan WP4; the GPU media tests.
   - The `cargo gate package` tier for the package. Then `DESIGN.md` §4 is updated: `colossus_mech` gets media, and
     B-235 is fixed by WP4.
7. **Evidence.** A batch file `docs/evidence/veo/<date>-batch-<name>.md` lists every attempt from `handin.json`, its
   verdict and reason, and that it came from an outside generator at no cost to the project's budget.

---

## 7. What is not requested, and why

On 2026-11-28 the free audit `asset_baker audit-media --strict --ledger docs/evidence/veo/ledger.jsonl` passed: every
one of the 34 identities in `assets/media/required_set.ron` is `Verified`, so the game's required media is complete.
The two mode atlases (`railgun_crawler-siege_mode`, `burrow_scorpion-burrowed`) are verified as well.

The same audit lists 57 catalog identities with no media, and `psi_grey` and `rhino_beetle` with legacy atlases that
have no provenance. A dry run over all 59 plans 134 clips, but it fails validation:

- 66 clips have no authored prompt. The pipeline refuses a display-name fallback for a paid run.
- The other prompts are the generic unit texts, which earlier batches had to rewrite for every identity because they
  drew floors, dust, scenery and turning parts.
- Two keyframes are square legacy JPEGs that cannot pin a loop.

`DESIGN.md` records these identities as waiting for a later batch (`grav_harvester`, `nectar_worker`) or out of scope
(most of the Cosmic Enclave and Sylvan Swarm). Each needs prompts written and vetted by a plan before any clip is made.
**Do not generate clips for them from this document.** If the owner asks for them later, a new request document will
list their exact prompts and pins.

The identities with no media are: `amber_pollen_dome`, `ancient_tree_dome`, `apex_spire_dome`, `astral_vagrant`,
`behemoth_den_dome`, `burrow_spire_dome`, `canopy_grove_dome`, `carapace_mound_dome`, `caustic_nursery_dome`,
`celestial_beacon_dome`, `cerebral_elder`, `chitin_spire_dome`, `citadel_nexus_dome`, `colossus_mech` (Walk only, clip
2 above), `crystalline_forge_dome`, `cybernetics_conduit_dome`, `flying_saucer`, `grav_harvester`,
`harmonic_recharge_dome`, `hive_matron`, `infestation_nursery_dome`, `metamorphic_chamber_dome`, `mortar_stag`,
`mycelium_shaman`, `nectar_worker`, `phase_prism_saucer`, `primal_world_tree_dome`, `prism_weaver`,
`prismatic_saucer`, `psi_pylon_dome`, `psionic_archives_dome`, `razor_mantis`, `rift_siphon_dome`,
`robotic_assembly_bay_dome`, `robotic_foundry_dome`, `shadow_visitor`, `sky_leviathan`, `sonic_bat`,
`spawning_hollow_dome`, `spine_den_dome`, `spore_spire_dome`, `spore_vent_dome`, `stag_behemoth`, `stealth_eye_orb`,
`stellar_portal_dome`, `subterranean_network_dome`, `templar_conclave_dome`, `tesla_sentinel_dome`,
`thorn_lasher_dome`, `thorn_viper`, `tongue_chameleon`, `tripod_annihilator`, `void_goliath`, `void_sanctuary_dome`,
`volatile_tick`, `warp_gate_dome`, `warp_strider`.

The polish pass that the owner's answers to Q0131 and Q0133 deferred, which would redo the Carrier's, interceptor's,
Brood Lord analogue's and Broodling's imperfect loops, is not planned yet, so it is not requested here either.
