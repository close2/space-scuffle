# Blueprint: Remediation Step 8.5, Real Veo Media for the Required Set (Q0001 Option B)

This plan carries out step 8.5 of the remediation plan (`docs/plans/2026-11-01-codebase-remediation.md` §5 Phase 8,
finding B-1, decision D7): remove the six atlases that are copies of `concept_video.png`, generate real media with
Veo for the required set, and make `asset_baker audit-media --strict` pass for that set. It then extends real media
to the rest of the Automaton faction, in order of value to the skirmish.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-03 (UTC; the file carries the
date its work starts) against `main` at `9e693cf`. No Veo request was made while planning. Line numbers drift; search
for the quoted symbol. A claim that could not be confirmed while planning is marked **verify**, and the first step of
its package confirms or refutes it.

**Owner decision.** [Q0001](../questions/closed/0001-media-scope.md), answered 2026-11-03: "Option B. Veo should be
part of my ultra plan. So use it. (I think I have weekly limits and it would be a shame to waste them)."
[Q0002](../questions/closed/0002-player-skirmish-defaults.md)'s answer adds that the temporary procedural fixtures for
these assets are to be replaced by real art once baked, and that the agent may decide scoping if it documents it.

**Secrets.** The Veo key lives in `.env` or `tmp/api-key.txt`. **No agent ever opens, prints, greps, copies or
sources these files**, and no agent runs `env`, `printenv`, `set` or a shell with `-x` in a session where the key
could be in the environment. Only `asset_baker` reads them, inside its own process (`resolve_credentials_from`,
`veo_client.rs`), and it never prints the key (redacted `Debug`, scrubbed errors, key passed to curl on stdin). This
plan keeps that and adds tests that the new ledger and logs cannot contain it (WP1).

**Out of scope:** media for the Enclave and Swarm factions (they have no starting bank, Q0008, so the skirmish never
fields them); Die animations (the simulation publishes no `Die` action today, `published_actions.rs`); audio; a GCS
download path (D-5).

---

## 1. Summary

| WP | Title | Spends generations | Size | Depends on |
|---|---|---|---|---|
| WP1 | Pipeline readiness: exact action sets, prompts in data, prompt validation, ledger, budget caps, quota stop, free preflight, keyframe chaining | No | L (3 to 4 days) | none |
| WP2 | Provenance records and a real `--strict` audit over a data-defined required set; remove the three off-path copies | No | M-L (2 to 3 days) | none (parallel with WP1) |
| WP3 | Preflight and a one-clip pilot | 1 | S (half a day, mostly waiting) | WP1, WP2 |
| WP4 | Batch 1: the five Phase 5 identities; replace the Q0002 fixtures | 9 (plus at most 3 retries) | M (1 to 2 days) | WP3 accepted |
| WP5 | Batch 2: the mineral path (B-1 replacements) and the town hall's missing state; strict audit passes; Phase 8 exit | 9 (plus at most 3 retries) | M (1 to 2 days) | WP4 |
| WP6 | Expansion batches 3 to 5: the rest of the Automaton faction, by value, within the weekly limit | about 49 (plus at most 5 retries per batch) | M per batch | WP5 |
| WP7 | Records | No | S | WP5 (and again after each WP6 batch) |

**Generation count.** The required set (WP3 to WP5) needs **19 clips**: 1 pilot plus 9 in batch 1 (the pilot's clip
is one of batch 1's ten) and 9 in batch 2, plus at most 6 retries. The whole Automaton faction needs about **68**
(§2.4). Every clip is 4 seconds, 16:9, one sample.

**Order and parallelism.** WP1 and WP2 run in parallel (two worktrees; they own different files, D-9). WP3 to WP5 are
strictly sequential and each ends with a review before the next spends anything. WP6 runs one batch per session,
paced by the weekly limit (D-4).

---

## 2. Current state

### 2.1 The pipeline (`crates/asset_baker`)

- **Command:** `asset_baker generate-veo` (`main.rs`), `--unit <slug>` or `--all-starters`, `--model` (default
  `veo-3.1-fast-generate-001`), `--keyframes-dir assets/ai_keyframes`, `--videos-dir assets/videos`, `-o
  assets/textures`, `--size 512`, `--fps 15`, `--dry-run`. The default paths are relative to the working directory.
- **Credentials** (`resolve_credentials_from`): environment `GEMINI_API_KEY`, then `GOOGLE_API_KEY` (project from
  `GOOGLE_CLOUD_PROJECT`), then `.env` in the working directory or its parent, then `tmp/api-key.txt`. A key starting
  `AQ.` selects Vertex Express (`us-central1-aiplatform.googleapis.com`, `:predictLongRunning`, polled with
  `:fetchPredictOperation`), anything else the Gemini API (`generativelanguage.googleapis.com/v1beta`). An `AQ.` key
  with no project fails with `ProjectIdMissing`. **`--dry-run` resolves credentials too** (it prints which mode it
  will use), so today even a dry run reads the key file.
- **Transport** (hardened in `b342c77`, `4c425c8`): `x-goog-api-key` header in a curl config on stdin; HTTPS only to
  the two hosts above; redirects followed by hand without the key; `--fail-with-body`; poll every 5 s, 300 s overall;
  bounded backoff on 408, 429, 5xx and network faults; a start request retried only on 429 or connect failure, so a
  retry cannot pay twice; downloads to a temp file, validated by `ffprobe`, atomically renamed.
- **Results:** inline `bytesBase64Encoded` first, then `uri`, then `gcsUri`. A `gs://` URI fails the host allowlist
  with `UntrustedHost`. A filtered (RAI) result with no video raises `VeoOperationFailed`.
- **Resume:** `assets/videos/<slug>_<state>.operation.json` (`operation_name`, `model`) is written before polling and
  cleared when the clip is installed or the operation fails terminally. A rerun resumes the operation instead of
  paying again.
- **Request:** `{instances: [{prompt, image?}], parameters: {aspectRatio: "16:9", durationSeconds: 4, sampleCount:
  1}}`. **Every identity costs three clips, Idle, Walk and Attack**, whatever it is.
- **Prompts** (`build_action_prompt`): the catalog's `veo_prompts[state]` if present, otherwise `"{display_name}:
  {lore_description}"`; then a fixed action description and a fixed style sentence, both string literals in Rust.
  All 42 units in `units.ron` have Idle, Walk and Attack prompts; all 49 structures have `veo_prompts: {}`. Nothing
  validates prompt content.
- **Not present:** any budget or count cap, any record of requests beyond stdout, any check that the key works
  without paying, any stop on a quota error.
- **Baking** (`ffmpeg_importer.rs`): `/usr/bin/ffmpeg`, square crop, scale, resample the whole clip; a vertical
  strip at most 16,384 px tall, so with 512 px frames and K states each state gets `32 / K` frames; output
  `<out>/<slug>.png` and `<slug>.flipbook.ron`; frames stay opaque. No provenance is written. **Amended
  2026-11-04** ([`2026-11-04-veo-letterbox-bake.md`](2026-11-04-veo-letterbox-bake.md), Q0043): the framing is a
  data policy by render shape; structures are letterboxed with a bar colour measured from the clip, units keep the
  centre crop.

### 2.2 What the game requires

The client's media gate (`crates/client/src/media.rs`) requires every action that
`UnitCatalog::publishable_actions` (`crates/sim_core/src/published_actions.rs`) returns: Idle always; Constructing
if some archetype can raise it; Walk if it can move; Attack if it can move and be armed; Cast if it harvests or
builds. So a structure needs **Idle and Constructing**, never Walk or Attack; the worker needs **Idle, Walk, Attack
and Cast**; an unarmed unit needs Idle and Walk. `generate-veo` and `bake-unit-videos` produce only Idle, Walk and
Attack: **a dome baked by today's pipeline would still say "No approved media"**, and two of its three paid clips
(Walk, Attack) would be wasted. This is the most important fix before any spend (WP1).

An identity without approved media is disabled in the build and train cards with "No approved media"
(`battle_hud/card.rs`, `CardLock::NoMedia`, Phase 5 D-9). The skirmish (`assets/scenarios/skirmish.ron`) is
Automaton against Automaton, loadout `standard_automaton` (`foundry_core_dome` and six `welder_drone`).

### 2.3 What exists

| Identity | Role | Media | Required actions |
|---|---|---|---|
| `foundry_core_dome` | town hall | real atlas; real Idle, Walk, Attack clips in `assets/videos`; legacy manifest maps Constructing onto other frames | Idle, Constructing |
| `pulse_bot` | combat (marine) | real atlas and clips | Idle, Walk, Attack |
| `welder_drone` | worker | **copy of `concept_video.png`** (B-1) | Idle, Walk, Attack, Cast |
| `capacitor_dome` | supply | **copy** | Idle, Constructing |
| `assembly_bay_dome` | production (barracks) | **copy** | Idle, Constructing |
| `railgun_crawler` | combat | **copy** | Idle, Walk, Attack |
| `tesla_sentinel_dome`, `flying_saucer` | other factions' or off-path identities | **copy** | (not needed for the skirmish) |
| the other 25 Automaton identities | | none | |

The six copies are byte-identical (17,746,848 bytes each, the bytes of `concept_video.png`). `audit-media` reports
them as `KnownConceptDuplicate`. `--strict` cannot pass at all today: `strict_accepted` is hard-coded `false`
(`media_audit.rs`), and there is no provenance record or required set in code (plan 39 deferred them).

**Q0002 fixtures.** `crates/client/tests/support/battle_harness.rs::fixture_root` writes procedural atlases into a
temporary root, then links the real `textures/` over it (copying overwrites, so a real atlas replaces a fixture of
the same name). `crates/client/tests/battle_card_fixture_tests.rs` uses it for `plasma_siphon_dome`,
`tech_module_pod` and `repair_shuttle` (gas, research, transport). Tests that assume missing media and will flip:
`battle_card_tests.rs` (`the_extractor_card_entry_is_disabled_for_want_of_media`,
`a_disabled_entry_issues_nothing_and_says_why`, which asserts every locked structure lacks media),
`battle_hud/card.rs`'s unit test expecting a full page of `NoMedia`, and `runtime_media_tests.rs` (needs at least one
identity without media).

**Storage.** No `.gitattributes`, so nothing is in LFS; media is committed as plain blobs. `assets/textures` is
221 MB on disk (each 512 px unit atlas 9 to 18 MB), `assets/videos` 22 MB (12 clips of 1.1 to 2.7 MB), the git pack
153 MiB.

### 2.4 Clips needed, by batch

Counts follow §2.2's rules (**verify** each against `publishable_actions` in WP1; the tool prints the exact list).

| Batch | Identities | Clips |
|---|---|---|
| 1 (WP3 pilot + WP4): Phase 5's gas, research and transport | `plasma_siphon_dome` 2, `tech_module_pod` 2, `machining_plant_dome` 2, `aerospace_hangar_dome` 2, `repair_shuttle` 2 (unarmed) | 10 |
| 2 (WP5): mineral path, B-1 replacements, town hall | `welder_drone` 4, `capacitor_dome` 2, `assembly_bay_dome` 2, `foundry_core_dome` Constructing 1 | 9 |
| 3 (WP6): units the barracks trains, and the copied combat unit | `railgun_crawler` 3, `shock_golem` 3, `jet_jumper` 3, `specter_android` 3 | 12 |
| 4 (WP6): tech, upgrades and defence structures | `mechanics_lab_dome`, `orbital_relay_dome`, `planetary_bulwark_dome`, `weapons_arsenal_dome`, `fortress_bunker_dome`, `flak_array_dome`, `radar_beacon_dome`, `covert_ops_nexus_dome`, `fusion_reactor_dome`, `duplicator_module_pod`, 2 each | 20 |
| 5 (WP6): late units | `napalm_wheeler`, `colossus_mech`, `vector_interceptor`, `sky_bastion`, `cloak_gunship` 3 each, `omni_probe_drone` 2 | 17 |

**Required set** (the Phase 8 exit): batches 1 and 2 plus the two real identities, that is every identity a player
can field or build in the skirmish's economy, gas, research and transport paths: 12 identities, 19 new clips.
Batches 3 to 5 are the expansion D7 asks for ("the units in shipped loadouts, then expand").

---

## 3. Design decisions

### D-1. Generate exactly the published actions

`generate-veo` takes its state list from `publishable_actions` for the identity, never a fixed Idle/Walk/Attack
list, and `bake-unit-videos` bakes exactly that set (the library's `bake_multi_action_videos_with_states` already
takes any list). An identity whose existing clips cover some states (for example `foundry_core_dome`'s Idle) gets
only the missing ones; existing clips for states it does not publish (the dome's Walk and Attack) are not baked.
`--dry-run` prints, per identity, the states to generate, the states reused, and the total clip count of the run.

### D-2. Prompts live in data and are validated before any spend

Charter §3.3 puts "media generation prompts" in data. Today the action descriptions and the style sentence are Rust
literals, and the structures have no prompts at all, so a dome would get "Walk animation: dynamic forward stride".

- **New data file `assets/catalog/media_style.ron`**: the style sentence and one action description per
  `AnimState`, including authored Constructing and Cast descriptions (today Cast is "Active tactical combat
  animation."). `build_action_prompt` composes `veo_prompts[state]` + action description + style from it. The
  existing style text is carried over unchanged, so prompts of the four existing identities do not change.
- **Authored prompts** in `structures.ron` (Idle, Constructing) and `units.ron` (every required state, including
  `welder_drone`'s Cast: harvesting a crystal or welding) for every identity of the batch being generated. Each
  prompt describes one subject, centred, matching the existing look (studio backdrop, 3/4 view), and for a structure
  says "no characters". The WP that runs a batch writes its prompts first and commits them before spending.
- **Validation** (`asset_baker validate-prompts`, also run by `generate-veo` before credentials are read): for each
  clip of the run, the identity has an authored `veo_prompts` entry for that state (the display-name fallback is
  refused for a paid run); the composed prompt is within a length limit (**verify** the model's limit in the Vertex
  Veo reference; default 1,024 characters); it contains no placeholder text (`TODO`, `TBD`, `{`, `lorem`); it names
  no real person, brand or franchise (a short deny list in `media_style.ron`, with the current style sentence's
  "StarCraft II" reworded to a description of the look, **owner-visible change**, see D-10); and the keyframe, if
  any, exists and is a readable image. A failure lists every problem and spends nothing.

### D-3. A ledger of every request, committed

A new append-only ledger, one JSON object per line, at a path given by a **required** flag `--ledger` (the plan uses
`docs/evidence/veo/ledger.jsonl`, committed). One line per event: `request_started` (time UTC, model, mode, slug,
state, prompt SHA-256, prompt text, keyframe SHA-256 or none, parameters, run id), `operation_named` (operation
name), `operation_done` (outcome: `video_inline`, `video_uri`, `gcs_uri` with the URI, `filtered` with the API's
reason, `failed` with the error code and message), `downloaded` (bytes, SHA-256, ffprobe duration and frame
count, path), `quota_refused` (HTTP status and the error's `status`/`reason` fields), `rejected_in_review` (written
by the `review` subcommand, D-7). Never the key, never a header, never a full curl command line: the ledger writer
takes typed events, not strings, and a test feeds a fake key through a whole fake run and asserts that no byte
sequence of it appears in the ledger, stdout or stderr.

Why committed: the scratch disk is not durable across server moves (it moved on 2026-11-02), operation names are
not secrets, and the budget check (D-4) and the provenance records (D-6) read it.

### D-4. Budget: hard caps, quota stops, no blind retries

The owner believes the allowance is a weekly limit. A refused request (quota) costs nothing; a generated clip that
cannot be used costs a generation. So:

- **`--max-generations N` is required** for any non-dry run. The tool counts the clips it would start (D-1) and
  refuses if the run needs more than N. It never starts more than N in one invocation, retries included.
- **Rolling budget.** `--weekly-budget M` (default from `assets/media/veo_budget.ron`, a non-secret data file;
  initial value from Q0024's default): the tool sums `request_started` lines of the last 7 days in the ledger and
  refuses a run that would exceed M.
- **Quota stop.** The first start request refused with HTTP 429 and an error `status` of `RESOURCE_EXHAUSTED` (or a
  message naming a quota) ends the whole run: the tool writes `quota_refused`, starts nothing more, and exits with a
  distinct exit code (3) so the agent knows to wait for the next window instead of retrying. The existing transient
  429 backoff stays for polling only; for a start request, the first 429 that is a quota refusal is final.
- **No automatic regeneration.** A filtered or failed operation is recorded and the run moves to the next clip.
  Regenerating a clip is a separate invocation after a human-readable reason is written (D-7).
- **Stop on the unexpected.** The first `gcs_uri`, the first `filtered` result and the first failure of a kind not
  seen before end the run after recording it, so a systematic problem costs one generation, not a batch.
- **Retry allowance per batch:** at most 3 regenerations for batches 1 and 2, 5 for each expansion batch. Beyond
  that, the batch stops and the agent files a question.

### D-5. The two risks recorded for the first live run

1. **`x-goog-api-key` was never tried live.** A new subcommand `asset_baker veo-preflight` makes one authenticated,
   free request on the same transport: a GET of the model resource (Gemini: `v1beta/models/{model}`; Vertex Express:
   `v1/publishers/google/models/{model}` or the project-scoped equivalent, **verify** which the Express key accepts
   in the Vertex reference). It prints the mode, the HTTP status and whether the model is visible, and nothing else.
   A 401/403 here costs nothing and stops WP3 before any spend.
2. **`gcsUri` results are refused.** The request never sets `storageUri`, so the service returns the video inline
   (plan 06 recorded the live Vertex response as inline `bytesBase64Encoded`, `06-plan-round3-veo31-video-generation.md`
   §6). If a `gcsUri` arrives anyway, the operation response (URI included) is recorded in the ledger and the run
   stops (D-4). Downloading from GCS needs OAuth credentials this pipeline does not have, and adding them is out of
   scope; the owner can fetch such a clip from the console, and `generate-veo --import <slug> <state> <file>`
   installs a clip from a file with the same validation and a provenance record naming the operation. WP3's pilot
   is the single generation that proves which case applies.

Also checked before the pilot (WP1, against the Vertex Veo API reference, **verify**): whether `generateAudio:
false` is accepted for the chosen model and lowers the cost (the pipeline discards audio); whether
`durationSeconds: 4` is the minimum; and the model id still exists (preflight shows it).

### D-6. Provenance and a real strict audit

- **Provenance file** `assets/textures/<slug>.provenance.ron`, written by the baker: per published state, the clip
  path under `assets/videos`, its SHA-256, and its origin, either `Veo { model, operation, prompt_sha256,
  keyframe_sha256, ledger_line }` or `LegacyVeo { commit }` (a clip committed by the original Veo work, plan 06,
  for which no operation name was kept); the bake parameters (size, fps, frames per state, ffmpeg version); the atlas
  SHA-256 and the frame range of each state.
- **Required set** in data: `assets/media/required_set.ron`, a list of identity slugs with a tier name (`required`,
  `expansion`). Initially the 12 required identities of §2.4.
- **Audit:** a new status `Verified` when the provenance file parses, the atlas and every clip match their hashes,
  every published action of the identity has its own frame range from its own clip (no shared ranges), no frame range
  is a concept duplicate, and every `Veo` origin has a matching `downloaded` line in the ledger. `--strict` passes
  when every identity of the required set is `Verified`; it no longer hard-codes `false`. Identities outside the set
  are reported, not failed.
- `foundry_core_dome` and `pulse_bot` become `Verified` by re-baking from their committed clips with `LegacyVeo`
  origins (no generation) plus, for the dome, batch 2's Constructing clip. `LegacyVeo` is real Veo output; the
  origin says honestly that its request was not recorded.

### D-7. Review before acceptance, and keyframe chaining for a consistent look

- **Keyframe chaining.** Each identity's first generated state is Idle; the clip's frame at 1.0 s is extracted with
  ffmpeg (`asset_baker keyframe-from-clip`) and used as the image input of its other states, so Walk, Attack,
  Constructing and Cast show the same design. Existing keyframes in `assets/ai_keyframes` are used where they exist
  (`welder_drone_idle.jpg`, `foundry_core_dome_*.jpg`). This costs no generation and is real media (a frame of a
  real clip). Batch order within a run therefore puts every Idle before the identity's other states, and the tool
  enforces it.
- **Review.** `asset_baker review <slug>` writes a contact sheet (ffmpeg `tile`, 8 frames per clip) to
  `~/scratch/veo/review/` and a downscaled copy (at most 1,024 px wide) to `docs/evidence/veo/review/`. The agent
  looks at it (an image read) and records `accept` or `reject` with a reason against written criteria: one subject,
  matches its prompt, no text or watermark, no cut or morph to a different object, subject stays inside the framed
  square (letterbox for structures, crop for units, amended by `2026-11-04-veo-letterbox-bake` D-4), action
  readable at 512 px, consistent with the identity's other states. **Amended 2026-11-04 (WP4, owner's review
  points):** loop continuity (Q0026), **measured since 2026-11-04 by `asset_baker loop-check`** (also printed by
  `review` for every looping state; seamless-loops plan
  [`2026-11-04-veo-seamless-loops.md`](2026-11-04-veo-seamless-loops.md) D-6) on the frames the atlas will hold
  after the bake's seam repair: the worst step of the cycle, the wrap included, at least the median step SSIM minus
  0.03; no kept frame zoomed against frame 0 by more than 1.5 %; for a pinned clip, frame 0's SSIM against the pin
  at least 0.90; and a clip the bake's scale lock refuses fails. The visual check of loop-check's seam sheet (the
  last four frames and frame 0) stays, and decides when it sees what the numbers miss, such as the double image of
  a cross-fade over a camera arc (loops WP3 calibration,
  [`2026-11-04-batch-1.md`](../evidence/veo/2026-11-04-batch-1.md)); and
  **a plain, uniform backdrop with no radiating stripes, sunburst, light rays, spotlight beams or patterned
  gradient** (owner, relayed 2026-11-04). A clip that fails either is rejected. Rejected clips stay in `~/scratch/veo/clips/`
  and are recorded in the ledger. Owner taste is final: Q0026 shows the owner the pilot and
  batch 1 sheets.

### D-8. Where the files live

| What | Where | Committed |
|---|---|---|
| Every downloaded clip, accepted or not, with its operation response | `~/scratch/veo/clips/<slug>/<state>/<operation-id>.mp4` and `.json` | No |
| Accepted clips | `assets/videos/<slug>_<state>.mp4` (the existing convention, 1 to 3 MB each) | Yes |
| Atlases, manifests, provenance | `assets/textures/<slug>.{png,flipbook.ron,provenance.ron}` | Yes |
| Ledger, review sheets (downscaled), batch evidence | `docs/evidence/veo/` | Yes |
| Full-size review sheets, bake temp files, logs | `~/scratch/veo/`, `~/scratch/logs/veo_*` | No |

The tool's default paths stay relative; this plan always passes `--videos-dir`, `-o` and a new `--clips-dir
~/scratch/veo/clips` explicitly, and `generate-veo` writes the downloaded clip to the clips dir first and installs
it into `assets/videos` only after review (`asset_baker install-clip`).

Repository growth for the required set: about 19 clips × 2 MB plus 10 new or re-baked atlases × 12 MB, roughly
160 MB; the whole faction roughly 600 MB more. GitHub accepts files up to 100 MB, so plain blobs work, but the
clone grows. Q0025 asks whether to move media to LFS; the default commits plain blobs for the required set, and asks
again before the expansion batches.

### D-9. The Q0002 fixtures and the tests that flip

When a batch lands, for each identity it covers:

- `battle_card_fixture_tests.rs`: the test of that identity's path (gas, research, transport) switches from
  `fixture_root()` to the real runtime root (`ResolvedAssetsRoot` of `assets/`), so it runs on the shipped atlas, as
  charter §1.2 asks of end-to-end tests. When no test needs a fixture for an identity, the identity is removed from
  `fixture_root`'s list; when none is left, `fixture_root` and its procedural generation in the harness are deleted
  (the procedural generator itself stays, as `asset_baker`'s tested fixture tool, D6).
- Tests that need "an identity without media" stop naming one: they ask the catalog and the runtime root for an
  identity whose media is missing (for example an Enclave unit) and skip that assertion's setup only if none exists,
  failing loudly with a message if the test can no longer find one (it then needs a fixture root of its own). The
  assertion "every locked structure lacks media today" becomes "every locked structure is locked for want of media,
  and exactly the identities without verified media are locked".

### D-10. File ownership

| File | WP |
|---|---|
| `crates/asset_baker/src/{veo_client.rs, main.rs, ffmpeg_importer.rs}`, new `src/{ledger.rs, budget.rs, prompts.rs}`, tests `veo_*_tests.rs`, new `ledger_tests.rs`, `prompt_tests.rs` | WP1 |
| `assets/catalog/media_style.ron` (new), `assets/media/veo_budget.ron` (new); `sim_core::catalog` (loading `media_style.ron` if it is a catalog file, **verify** where the catalog loader lives and whether `asset_baker` loads it alone instead) | WP1 |
| `crates/asset_baker/src/{media_audit.rs, manifest.rs, media.rs}`, new `src/provenance.rs`, `assets/media/required_set.ron`, tests `media_tests.rs`, `bake_integrity_tests.rs`, new `provenance_tests.rs` | WP2 |
| `assets/catalog/{units,structures}.ron` prompts; `assets/textures/*`, `assets/videos/*`, `docs/evidence/veo/*` | WP3 to WP6 (one batch at a time) |
| `crates/client/tests/{battle_card_fixture_tests.rs, battle_card_tests.rs, runtime_media_tests.rs}`, `tests/support/battle_harness.rs`, `crates/client/src/battle_hud/card.rs` (unit test only) | WP4, then WP5 |
| remediation plan Phase 8, `DESIGN.md`, `docs/plans/README.md` rows (this plan, 05, 06, 09) | WP7 |

The style sentence's reference to a commercial game is reworded in WP1 (D-2). This changes every future prompt
slightly; existing clips are not regenerated for it.

### D-11. The two unused legacy structure clips stay (WP7, a Q0002 scoping decision)

Recorded 2026-11-05. `assets/videos/foundry_core_dome_walk.mp4` and `foundry_core_dome_attack.mp4` (2.0 and 2.1 MB,
added in `3d9a9c4` with the original Veo work) are no longer used by any atlas. Batch 2 replaced the foundry's
Idle and baked only Idle and Constructing, and a structure publishes neither Walk nor Attack (D-1). **Decision:
leave them.** Nothing reads them: the bake takes clips by explicit path, the client loads atlases, and the audit
checks only the clips a provenance record names. Deleting them would not shrink the repository, because the
blobs stay in history (Q0025). It would also turn this records package into an asset change with its own gate
selection. **To bring it back:** remove both files in any later media batch that touches `assets/videos`; no
code or test names them.

---

## 4. Work packages

Common rules: each WP in its own worktree, `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/veo-wpN`, logs
`~/scratch/logs/veo_wpN_*`, `df -h / ~/scratch` checked first, target removed after merge. Failing test first, red
run pasted in the report. `//!` on every new file, `///` on public items, typed errors (`thiserror`), no `unwrap`,
`expect` only with `invariant:`, no function over 100 lines, no catalog identity in Rust (the slugs above appear only
in data, tests' fixture data and this plan). `docs/questions/open/` checked before each push. Never read `.env` or
`tmp/`. Gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

**Spending rule for WP3 to WP6.** A generating command is always run as one logged invocation in the background:

```bash
cd <worktree> && cargo run --release --locked -p asset_baker -- generate-veo \
  --unit <slug> [--unit <slug> ...] --max-generations <N> --ledger docs/evidence/veo/ledger.jsonl \
  --clips-dir ~/scratch/veo/clips --videos-dir assets/videos -o assets/textures \
  > ~/scratch/logs/veo_<wp>_<run>.log 2>&1      # run in the background; read the log every few minutes
```

run from a worktree whose parent directory holds the key files the tool looks for (the tool searches the working
directory and its parent; **verify** that a worktree under `.claude/worktrees/` finds them, and if not, pass a new
`--credentials-dir <dir>` flag that WP1 adds, pointing at the main checkout, without the agent reading the dir). The
log is safe to read: the tool prints no secret (WP1's test proves it).

### WP1: Pipeline readiness (no spend)

1. **Tests first** (red runs pasted), with the existing `FakeServer` and `CurlShim` of `crates/asset_baker/tests/common`:
   - the clip list of a structure fixture is Idle and Constructing, of a harvester Idle, Walk, Attack, Cast, of an
     unarmed unit Idle and Walk; existing clips are reused, not regenerated (D-1);
   - `--dry-run` reads no credentials: run it with a credentials dir containing a file that fails the test if
     opened (or with no key anywhere) and assert success and the printed count;
   - `validate-prompts` rejects a missing prompt, the fallback, an over-long prompt, a placeholder, a deny-listed
     term, a missing keyframe; spends nothing (the fake server sees no request);
   - ledger: every event of a fake run is written, in order; a fake key never appears in ledger, stdout or stderr;
   - budget: `--max-generations` missing is an error; a run needing N+1 is refused before any request; the rolling
     7-day sum from a fixture ledger refuses an over-budget run;
   - quota stop: the fake server answers the second start with 429 `RESOURCE_EXHAUSTED`; exactly one generation is
     recorded, `quota_refused` is written, exit code 3;
   - stop on `gcsUri`, on `filtered`, on a new failure kind: the run ends after recording, the operation response is
     kept in the clips dir;
   - preflight: one GET with the key header to the allowlisted host; prints status; never the key;
   - keyframe chaining: Idle is generated first and its frame at 1.0 s becomes the image of the other states
     (request bodies on the fake server carry it);
   - `install-clip` and `--import` validate with ffprobe and refuse a clip without a ledger entry (or, for
     `--import`, without an operation name).
2. Implement D-1 to D-5 and D-7's `keyframe-from-clip`, `review` and `install-clip`; add `--clips-dir`,
   `--credentials-dir` and a `--states` filter (a subset of the identity's published states, refused if it names a
   state the identity does not publish).
3. Move the style sentence and action descriptions to `assets/catalog/media_style.ron` (D-2), rewording the franchise
   reference; add authored Constructing and Cast descriptions.
4. Check the Vertex Veo reference (web) for: the request fields (`generateAudio`, `durationSeconds` range,
   `storageUri` behaviour when absent, `personGeneration`), the model id, the prompt length limit, the quota error
   shape; record the findings with links in `docs/evidence/veo/<date>-api-notes.md` and adjust D-4's quota detection
   and D-5's preflight URL to them.
5. Documentation: `asset_baker`'s `//!` and `README`-level usage in `DESIGN.md`'s media row (no new markdown file).

**Gate:** common gate. No network call is made by any test.

### WP2: Provenance and the strict audit (no spend)

1. Tests first: a fixture identity with a provenance file and matching hashes is `Verified`; a changed atlas byte, a
   changed clip byte, a shared frame range, a concept duplicate, a missing ledger line each make it not verified with
   a typed reason; `--strict` passes for a required set of verified fixtures and fails naming each unverified one.
2. Implement D-6: `provenance.rs`, the baker writes provenance, the audit's `Verified` status and required set.
3. Re-bake `foundry_core_dome` (Idle only for now) and `pulse_bot` from their committed clips with `LegacyVeo`
   provenance (**verify** the commit that added each clip with `git log --diff-filter=A -- assets/videos/<file>`).
   `pulse_bot` becomes `Verified`; the dome does not yet (no Constructing).
4. **Remove the three copies that are off the playable path:** `railgun_crawler`, `tesla_sentinel_dome` and
   `flying_saucer` (atlas and manifest). They become "No approved media" in the game, which is the truth. The three
   on the mineral path (`welder_drone`, `capacitor_dome`, `assembly_bay_dome`) stay until batch 2 replaces them,
   because removing them would make the skirmish unplayable; the audit keeps reporting them as
   `KnownConceptDuplicate`, and WP7 records that B-1 is open until WP5.
5. `required_set.ron` with the 12 identities of §2.4.

**Gate:** common gate. `audit-media --strict` fails, naming the required identities not yet verified (expected).

### WP3: Preflight and pilot (1 generation)

1. Confirm WP1 and WP2 merged. Write and commit the authored prompts for **batch 1** (all five identities), run
   `validate-prompts` and `generate-veo --dry-run` for batch 1; paste the dry-run's clip list (expected 10) in the
   report.
2. `asset_baker veo-preflight`. If it fails, stop: record in the evidence, file a question (the key or mode is
   wrong), spend nothing.
3. **Pilot:** `generate-veo --unit plasma_siphon_dome --states idle --max-generations 1` (a `--states` filter added in
   WP1). Text-to-video; a structure is the simplest subject and the clip is needed anyway.
4. Check and record in `docs/evidence/veo/<date>-pilot.md`: the result shape (inline, uri or gcsUri), the time to
   finish, the clip's duration, resolution and frame count, the ledger lines, the review sheet and the verdict. Bake
   it into a scratch root (`-o ~/scratch/veo/pilot-root/textures`) and run the client's runtime media test against
   that root if feasible, else the audit.
5. **Stop rule:** anything unexpected (gcsUri, filtered, wrong size, poor look) stops the plan here with a question;
   nothing more is spent. If the owner has answered Q0026 with a rejection of the look, adjust the style data and run
   the pilot once more (counts as a retry).

### WP4: Batch 1 (9 generations, at most 3 retries)

**Amended 2026-11-04:** WP4 starts only after WP2 of
[`2026-11-04-veo-letterbox-bake.md`](2026-11-04-veo-letterbox-bake.md) is merged: the letterbox bake exists, the
pilot's Idle clip is accepted and installed, and `assets/media/veo_request.ron` sends `generateAudio: false`. WP4's
first clip is the first live request with that field; if Vertex refuses it, record the refusal, set `generate_audio:
None` and rerun (that plan's D-6). Cost record before WP4: 1 generation, $0.40 at list price, of the owner's $15
target (Q0024). Batch 1 then costs $2.88 (silent, $0.32 a clip) or $3.60 ($0.40 a clip) at list price; record the
running total in the batch evidence.

**Amended again 2026-11-04 (owner's answer to Q0043, recorded in `791439f`; carried out in letterbox WP2):** keep
sound. `assets/media/veo_request.ron` now says `generate_audio: None` (no field sent, as in the pilot, which returned
a sound track), so batch 1 is billed at the with-audio rate, $0.40 a clip, $3.60 for nine. The owner also allows up
to 200 EUR of the credit (Q0024, Q0026) and asks for a new generation whenever a clip misses the criteria, and adds a
review point: **loop continuity**, the clip's last frame should be close to its first, because the atlas loops. The
pilot's Idle clip, installed by letterbox WP2, has a slow camera push-in, so its loop restarts with a visible jump of
about 5 to 8 % in scale (pilot evidence, "Acceptance after the letterbox change"). Before generating its Constructing
clip, decide whether to regenerate `plasma_siphon_dome` Idle with a prompt asking for a locked-off camera (one
generation, `install-clip --replace`); the Constructing keyframe chains from whichever Idle clip is installed.

**Amended a third time 2026-11-04 (Q0046):** WP4 stopped after five `plasma_siphon_dome` Idle clips, all rejected
for a camera push-in of 7 to 10 % that no prompt wording removed
([`2026-11-04-batch-1.md`](../evidence/veo/2026-11-04-batch-1.md)); the pilot's clip is uninstalled. WP4 now
depends on WP1 to WP4 of [`2026-11-04-veo-seamless-loops.md`](2026-11-04-veo-seamless-loops.md) (looping clips sent
with the same picture as `image` and `lastFrame`, a seam repair in the bake, the `loop-check` measure, and a
one-clip pilot) and is carried out as that plan's WP5, which also applies the held test patch for D-9. The
`plasma_siphon_dome` Idle comes from that pilot, so step 1 below generates nine clips plus the seed clips that plan
counts (§6), and the keyframe chaining of D-7 follows its D-3.

1. `generate-veo` for the five identities with `--max-generations 9` (the pilot's Idle is reused). Review every
   clip (D-7); regenerate rejected ones within the retry allowance.
2. Install, bake, write provenance. `audit-media` shows the five `Verified`.
3. Replace the Q0002 fixtures and flip the tests (D-9) for gas, research and transport. The gas, research and
   transport paths now run in the real game: run the skirmish smoke test (Phase 5's skirmish pilot in
   `tests/support/skirmish_pilot.rs`) and a manual capture of each card enabled
   (`client_diagnostics battle_capture`, one locked command on the RX 580 per capture, charter §7).
4. Evidence: `docs/evidence/veo/<date>-batch-1.md` (clips, ledger lines, sheets, verdicts, retries used).

### WP5: Batch 2 (9 generations, at most 3 retries); Phase 8 exit

1. Prompts for `welder_drone` (Cast included), `capacitor_dome`, `assembly_bay_dome`, `foundry_core_dome`
   Constructing; validate; dry run (expected 9).
2. Generate, review, install, bake with provenance. **Delete the three remaining copies** as they are replaced:
   no file in `assets/textures` is then byte-identical to `concept_video.png` (a test in `media_tests.rs` asserts it
   over the whole directory, so B-1 cannot return).
3. `audit-media --strict` passes for the required set: paste the report. Run the full gate and the client's GPU
   tests (the harness locks per child).
4. Evidence `…-batch-2.md`. Hand the Phase 8 exit to WP7.

### WP6: Expansion batches 3 to 5

One batch per session, in §2.4's order (3, 4, 5); within a batch, identities in the listed order, so a quota stop
leaves the most valuable ones done. Each batch: prompts, validation, dry run, generate within the total budget (Amendment 1),
review, install, bake, provenance, add the identities to `required_set.ron` under tier `expansion`, flip any test
that named them as lacking media, evidence file. A quota stop ends the session; the next session resumes with the
same command (the ledger and operation records make it idempotent). The order may be revised by the owner at any
time through Q0024.

### WP7: Records

After WP5: remediation plan step 8.5 **Done** with commits, B-1 **Fixed**, Phase 8 "Exit status" rewritten as met
with the strict audit's output and CI run; `DESIGN.md` media row; `docs/plans/README.md` rows of this plan, 05, 06 and
09 (real media count); the remediation plan's D7 row. After each WP6 batch: this plan's row and the media count.
Q0001's and Q0002's resolutions are written by whoever closes those questions, citing these commits.

---

## 5. Risks

| Risk | Mitigation |
|---|---|
| The key fails live (header never tried) | Free preflight (D-5) before the pilot |
| Results come back as `gcsUri` | Pilot is one generation; the run stops; ledger keeps the URI; `--import` path |
| A structure is generated with Walk/Attack and still shows "No approved media" | D-1, tested in WP1 |
| Weekly limit wasted on failures | Prompt validation and dry run first; caps; quota stop; stop on the unexpected; no automatic regeneration (D-4) |
| States of one identity look like different objects | Keyframe chaining (D-7) |
| The key leaks into a log or the ledger | Typed ledger events; a test with a fake key over a whole fake run; agents never read key files |
| The look is not what the owner wants | Q0026 with the pilot's sheet; style is data, so a change is a data edit |
| Content filter refuses a prompt | Deny list; one filtered result stops the run; prompt reworded before a retry |
| Repository growth | D-8 estimate; Q0025 |
| ffmpeg version changes an atlas on rebake | Provenance records the ffmpeg version; the audit verifies hashes of committed files, not a rebake |
| Weekly allowance is actually per-second billing on a cloud account | Q0024 asks; caps bound the spend either way |

---

## 6. Decisions for the owner

- **Q0024 (new, not blocking):** how is the key billed, and what weekly cap should agents keep? Default: treat it as
  a weekly allowance of unknown size: at most 12 generations per invocation, at most 25 per rolling 7 days, and stop
  at the first quota refusal.
- **Q0025 (new, not blocking):** commit generated media as plain git blobs or move media to Git LFS? Default: plain
  blobs for the required set (about 160 MB); ask again before batch 3.
- **Q0026 (new, not blocking):** who accepts the look of a clip? Default: the agent accepts against D-7's written
  criteria and links the pilot's and each batch's contact sheets in the evidence; the owner can reject any clip later,
  which queues its regeneration in the next batch.

---

## 7. Records after WP5 (WP7, 2026-11-05)

**Status.** WP1 to WP5 are done, and so are the three amending plans
([`2026-11-04-veo-letterbox-bake.md`](2026-11-04-veo-letterbox-bake.md),
[`2026-11-04-veo-seamless-loops.md`](2026-11-04-veo-seamless-loops.md),
[`2026-11-04-veo-structure-action-text.md`](2026-11-04-veo-structure-action-text.md)). The required set in
`assets/media/required_set.ron` has 10 identities: batch 1's five, batch 2's four and `pulse_bot`. §2.4 and WP2
counted 12, but `foundry_core_dome` was counted twice, once in batch 2 and once as a real identity. `audit-media
--strict --ledger docs/evidence/veo/ledger.jsonl` verifies all 10
([batch 2](../evidence/veo/2026-11-05-batch-2.md), "Strict audit"). B-1 is fixed (`7929844`, `70ad1d6`; test
`no_shipped_texture_is_a_copy_of_the_concept_atlas`). Remediation step 8.5 is done, and Phase 8's exit is recorded
as met at `473b977`; its CI run id is added after the next push of `main`. 12 of 91 catalog identities have real
media. [Q0001](../questions/closed/0001-media-scope.md), [Q0026](../questions/closed/0026-veo-clip-acceptance.md)
and [Q0046](../questions/closed/0046-veo-loop-continuity-method.md) are closed. Q0024 and Q0025 stay open with a
status note.

**Spend.** 41 generations since the pilot, the pilot included: **$16.40** at list price (Veo 3.1 Fast, 720p, with
sound, $0.40 a clip). Batch 1, with the pilot and the loop experiments, spent 24 ($9.60); batch 2 spent 17
($6.80). All of it is within the owner's 200 EUR (Q0024, Q0026). §1 estimated 19 clips plus at most 6 retries. The
difference is seeds and pins (seamless-loops plan §6) and the per-clip series (structure action text plan D-4).

**Deviations recorded in the batch evidence.** `foundry_core_dome` Idle was regenerated instead of kept (`LegacyVeo`
became `Veo` provenance). The square legacy keyframes `welder_drone_idle.jpg` and `foundry_core_dome_idle.jpg` were
removed. `welder_drone` Cast has its own pin, and `welder_drone` is letterboxed. See
[`2026-11-05-batch-2.md`](../evidence/veo/2026-11-05-batch-2.md), "Deviations from the plan, and why".

**Open follow-ups.**

1. **Expansion (WP6, batches 3 to 5) is not started.** It waits for the orchestrator to schedule it. Before its
   first spend, Q0025's answer asks for a second ask on storage, and Q0024's first answer asks to check in on
   spending.
2. **Unit Cast text.** The unit Cast kind text in `assets/catalog/media_style.ron` has no camera or loop clause.
   It also asks for "tool arms reaching", which conflicts with the loop policy (Cast is looped and pinned at both
   ends). Batch 2 used a `welder_drone` override. `grav_harvester` (Enclave) and `nectar_worker` (Swarm) also
   publish Cast. They are outside this plan's scope, but whichever plan brings them in must review their Cast
   text, or the kind text itself, before generating. A Cast that shows a steady effect needs its own pin (batch 2).
3. **Legacy clips.** `foundry_core_dome_walk.mp4` and `_attack.mp4` are kept, unused (D-11).
4. **`psi_grey` and `rhino_beetle`** (Enclave and Swarm) keep legacy atlases without provenance
   (`LegacyUnverified`). They are outside the required set and this plan's scope.
5. **CI.** No CI run covers batch 2 yet (`main` was last pushed at `ab2042c`). The run id of the next push of
   `main` goes in the remediation plan's Phase 8 exit status.

## 8. Amendment 1 (orchestrator, 2026-11-05): the only limit is total spend

**Owner (chat, 2026-11-05):** "the only bro limit you need to take into account are the 200$. no weekly, round or
batch limit (unless it's a limit dictated by Google)". This replaces the per-run cap (Q0024's 12) and the rolling
7-day cap (Q0024's 25, Q0047's 60). It is recorded in Q0024's Answer.

### D-12. Budget rule

- **One limit:** the total list-price cost of every generation the service accepted, summed from the ledger
  (`request_started` lines; a start refused for quota costs nothing, as today), plus the cost of the generations a run
  is about to start, must not exceed **$200.00**. The owner's earlier allowance was 200 EUR; dollars are the smaller
  amount, so the cap is stated in US cents: `total_budget_usd_cents: 20000`.
- **Price per generation** is data, not code: a table in `assets/media/veo_budget.ron` keyed by model id (today
  `veo-3.1-fast-generate-001`: 40 cents for a 4 s 720p clip with sound, the list price the batch evidence uses). A
  `request_started` line whose model has no price is an error (typed), never a free generation.
- **No run, batch or window cap of our own.** `max_generations_per_run`, `weekly_generations`, `window_days`,
  `--weekly-budget` and the `AboveRunCap` / `WindowExceeded` errors are removed. `--max-generations` stays required:
  it is the run's stated intent (what the dry run prints and the ledger records), not a limit, and a run still
  stops when it reaches it.
- **Google's limits:** quota and rate-limit responses keep their current handling (backoff, then a clean stop that
  the next run resumes); the stop message says it is Google's limit.
- **Reporting:** every run prints spent / remaining in dollars before and after, and the dry run prints the cost of
  the planned generations.

### WP8: Total-spend budget (code + data; no spend)

1. Tests first in `crates/asset_baker/src/budget.rs` and `crates/asset_baker/tests/veo_cli_tests.rs`: a run that
   would cross $200.00 by one generation is refused with a typed `BudgetError::TotalExceeded { spent_cents,
   needed_cents, budget_cents }`; exactly reaching it is allowed; 13 or more generations in one run and more than 60
   in seven days are allowed; an unpriced model in the ledger is a typed error; the shipped budget file loads with
   20000 cents and the price table.
2. Replace `BudgetConfig`'s fields, the ledger summation (`used_in_window` becomes the total spent in cents), the
   `generate-veo` arguments and messages, the budget file and its comment, and the docs that state the old caps
   (this plan's §2/D-sections that name 12 or 60, `docs/evidence/veo/*` only where they state a current rule).
3. Gate: `cargo gate package` once after merging main.
4. Close Q0024 with a Resolution naming the commit (`git mv` to `closed/`).

WP6 (expansion) may start after WP8 merges. Q0025 (storage of the expansion media) is unaffected by this amendment.
