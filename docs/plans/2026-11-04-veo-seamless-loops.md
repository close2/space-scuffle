# Blueprint: Seamless Veo Loops, End Frame Pinned to the Start Frame, Seam Repair in the Bake (Q0046)

An amendment to the Veo media plan [`2026-11-04-phase-8-5-veo-media.md`](2026-11-04-phase-8-5-veo-media.md). It makes
looping clips (Idle, Walk, Cast) seamless by sending Veo 3.1 the same picture as the first and the last frame
(`lastFrame`), repairs any remaining seam in the bake (scale lock and a short cross-fade), gives review an
objective loop-continuity measure, and resumes that plan's WP4 (batch 1) with a one-clip pilot of the method.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-04 against `main` at `6e0cb35`.
No Veo request was made while planning. Line numbers drift; search for the quoted symbol. A claim that could not be
confirmed while planning is marked **verify**, and the first step of its package confirms or refutes it.

**Decision being carried out.** [Q0046](../questions/closed/0046-veo-loop-continuity-method.md). The orchestrating
agent first chose option (a) with ping-pong (b) as fallback. The owner then answered (2026-11-04, `6e0cb35`): ratify
(a) as primary, the same picture as start and end frame through `lastFrame` (start picture: frame 0 of clip
`0dbd7948`, which has the plain backdrop); for residual drift at the loop boundary, or as the fallback if `lastFrame`
is unsupported, prefer scale normalisation (option c) and/or a short cross-fade of the last few frames back into
frame 0 in `asset_baker` over ping-pong, so that directional motion such as rising vapour never runs backwards. This
plan follows the owner's answer: ping-pong is not built (D-5). Non-looping states are unchanged except Constructing's
hand-off (D-3). Other owner rules that bound this plan: Q0024 and Q0026 (up to 200 EUR including expansion and
regenerations, regenerate whenever a clip misses the criteria, keep sound, no loop jumps), Q0020 (the agent decides
visual questions; the owner may overrule).

**Why prompts are not enough.** Batch 1 evidence ([`2026-11-04-batch-1.md`](../evidence/veo/2026-11-04-batch-1.md)):
five clips of `plasma_siphon_dome` Idle, text-to-video and image-to-video, all push the camera in by 7 to 10 % over
4 s, whatever the wording or `negativePrompt`. The plain backdrop and `negativePrompt` in
`assets/media/veo_request.ron` fixed the rays and stay.

**Spending.** This plan's WP1 to WP3 spend nothing. WP4 (the pilot) spends 1 generation. Running total before it:
5 generations, **$2.00** at list price, 5 of 25 in the rolling window (all started 2026-11-04 between 10:01Z and
12:12Z, so they leave the window from 2026-11-11 10:01Z).

**Out of scope:** Cloud Storage (`gcsUri`) inputs or outputs (they need OAuth, Veo plan D-5); ping-pong playback
(D-5); an animation clock for the battle view (finding F-1, §5); audio playback.

---

## 1. Summary

| WP | Title | Spends | Size | Depends on |
|---|---|---|---|---|
| WP1 | Loop methods in data; `lastFrame` in the request; pin-image rules; ledger and provenance fields | No | M (1 day) | none |
| WP2 | Seam repair in the bake: loop sampling, scale lock, cross-fade, the shared SSIM and zoom estimator, provenance | No | M-L (1 to 2 days) | none (parallel with WP1) |
| WP3 | `asset_baker loop-check`: the loop-continuity measure, calibrated on the five rejected clips, raw and repaired | No | S (half a day) | WP2 (uses its estimator and sampling) |
| WP4 | Pilot: `plasma_siphon_dome` Idle with start = end = frame 0 of `0dbd7948`; review; decide method | 1 | S | WP1, WP2, WP3 merged |
| WP5 | Resume Veo plan WP4 (batch 1) on the chosen method, with the carried test patch | 13 to 16 | M | WP4 decided |

WP1 and WP2 own different files (§4) and run in two worktrees.

---

## 2. Facts this design rests on

### 2.1 What Google documents about `lastFrame` (checked 2026-11-04, no request made)

| Source | What it says |
|---|---|
| Veo 3.1 model page, <https://docs.cloud.google.com/vertex-ai/generative-ai/docs/models/veo/3-1-generate> | `veo-3.1-fast-generate-001` supports "Text to video, image to video, from first and last frame"; sound generation supported; lengths 4, 6 or 8 s; input resolutions 720p and 1080p; aspect ratios 16:9 and 9:16; image-to-video input image at most 20 MB; output 24 fps |
| First and last frames guide, <https://docs.cloud.google.com/vertex-ai/generative-ai/docs/video/generate-videos-from-first-and-last-frames> | lists `veo-3.1-generate-001` and `veo-3.1-fast-generate-001` for REST; the request puts `image` and `lastFrame` side by side in `instances[0]`, each `{gcsUri, mimeType}`; only `image/jpeg` and `image/png`; durations 4, 6, 8; no statement about audio or about the two images' sizes |
| API reference, <https://docs.cloud.google.com/vertex-ai/generative-ai/docs/model-reference/veo-video-generation> | now defers the instance schema to a separate `VideoGenerationModelInstance` page, which could not be fetched while planning. Third-party mirrors of that schema describe `lastFrame` as a union of `bytesBase64Encoded` or `gcsUri`, plus `mimeType` (not authoritative) |

Consequences:

- **Request shape:** `instances: [{prompt, image: {bytesBase64Encoded, mimeType}, lastFrame: {bytesBase64Encoded,
  mimeType}}]`, the inline form the pipeline already uses for `image`, which live Vertex Express runs accepted
  (ledger line 21). **Verify** on the pilot (WP4) that inline bytes are accepted for `lastFrame`. A refusal arrives
  as an error on the start request, before any operation exists, so it is not billed. The pipeline still records it
  as `start_failed`, which the weekly count treats as a start (conservative, `LedgerHistory::generations_since`).
- **Image rules the pipeline enforces before spending** (D-2): PNG or JPEG; 16:9 within 1 %; at least 1280×720;
  at most 20 MB. Start and end are the same file, so "same size and aspect" holds by construction.
- **Audio:** documented as supported by the model, silent for this mode. The pilot keeps `generate_audio: None`
  (sound on, the owner's choice). If Vertex refuses `lastFrame` with sound, the pilot records it and the plan relies
  on the seam repair (D-5) rather than dropping sound.
- **Gemini API mode** (a key that does not start with `AQ.`): the `-001` models do not exist there
  (`2026-11-04-api-notes.md`) and the inline shape there is undocumented, so a run that would send `lastFrame` in that
  mode is refused before any request (typed error `LastFrameNeedsVertex`).

### 2.2 What the code does today

- `veo_client.rs`: `VeoInstance { prompt, image }`; `initiate_generation(prompt, reference_image)` picks the MIME
  type from the file extension.
- `veo_plan.rs::resolve_keyframe`: an authored `assets/ai_keyframes/<slug>_<state>.{jpg,png}` if present; otherwise
  Idle is text-to-video and every other state is `ChainedFromIdle`, the frame at 1.0 s of the Idle clip
  (`veo_run.rs`, `CHAIN_KEYFRAME_TIME`). The committed keyframes are 1024×1024 squares from the original Veo work.
- Ledger `request_started` records `keyframe_sha256`; provenance `ClipOrigin::Veo` records `keyframe_sha256`.
- Bake (`ffmpeg_importer.rs`): 32 frames of 512 px per atlas (16,384 px), split evenly over the K published states,
  so a structure state keeps 16 frames, a unit with three states 10, the worker 8. `evenly_spaced_indices` keeps the
  first **and** the last frame. `pack_action` marks Idle, Walk and Cast looping, Attack one-shot, Constructing and
  Die held on their last frame.
- `decode_framed_clip` lets ffmpeg scale each frame (a centre crop to the square, or for a letterbox the whole 16:9
  frame fitted inside it, 512×288), keeps the per-state budget of frames, and only then completes them to squares in
  Rust (`frame_to_square`, bars in the colour measured from the clip). Between those two steps the kept content frames
  are `RgbaImage`s in Rust: that is where the seam repair goes (D-5).

---

## 3. Design decisions

### D-1. The loop treatment of each state is data

`assets/catalog/media_style.ron` gains a `loops` section, read into `MediaStyle` (`prompts.rs`):

```ron
loops: (
    // How each state is made seamless. Every state the atlas loops (Idle, Walk, Cast) needs an
    // entry; a non-looping state may only set `pin_end`.
    // - pin_end: the request sends the state's start picture also as `lastFrame`.
    // - seam: repair done by the baker on the kept frames, in this order: scale lock (undo a
    //   steady camera zoom), then a cross-fade of the last N frames back into frame 0.
    states: {
        Idle: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 2))),
        Walk: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 2))),
        Cast: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 2))),
        Constructing: (pin_end: true, seam: None),
    },
    // Exact catalog slug -> state -> treatment, for one identity whose clip needs another
    // one. A slug that is not in the catalog is refused (as `framing.overrides`).
    overrides: {},
),
```

New types in `prompts.rs`: `LoopSpec { pin_end: bool, seam: Option<SeamRepair> }` and `SeamRepair { scale_lock: bool,
crossfade_frames: u8 }`. `MediaStyle::load_for_catalog` checks: every looping state (as `pack_action` decides it) has
an entry; a `seam` on a non-looping state is refused; `crossfade_frames` is at most 4 and at most a quarter of the
state's frame budget; an override naming an unknown slug is refused (`UnknownLoopOverride`, like
`UnknownFramingOverride`). No identity appears in Rust (charter §3.3); switching a state or an identity to another
treatment is a data edit.

**Per state.** Idle, Walk and Cast: pinned, and repaired by default, because the owner asked for the repair against
any residual drift; on a clip that already loops, the repair does nothing measurable (D-5 skips a scale lock below
0.5 %, and a cross-fade into a matching frame changes little). Attack: one-shot, no entry. Constructing: held, not
looping, but `pin_end` so its last frame equals Idle's first and the switch to Idle on completion is seamless (D-3);
no seam repair, because nothing loops back.

### D-2. The start picture ("pin") and the request

- **Which picture.** For an `EndOnStartFrame` state the start picture is the state's resolved keyframe (D-3 for
  chained states), and the request sends it twice: as `image` and as `lastFrame`. `VeoInstance` gains
  `#[serde(rename = "lastFrame", skip_serializing_if = "Option::is_none")] last_frame: Option<VeoImageInput>`, and
  `initiate_generation` takes a `GenerationImages { start: Option<&Path>, end: Option<&Path> }` instead of one path;
  an end without a start is a typed error (`LastFrameWithoutStart`), never sent. The MIME type comes from the file's
  magic bytes (PNG or JPEG), not its extension; any other format is refused.
- **Pin rules** (`check_keyframe` gains a `PinShape` check, run by `validate-prompts` and before any credential is
  read): for an `EndOnStartFrame` state the picture must be PNG or JPEG, 16:9 within 1 %, at least 1280×720, at most
  20 MB (§2.1). A square legacy keyframe (all committed ones) is refused as a pin with a message naming the file; it
  stays valid as the plain start image of non-pinned states.
- **No pin yet.** An `EndOnStartFrame` state whose identity has no picture (no authored keyframe, nothing to chain
  from) is planned as before, text-to-video with no `lastFrame`, and `--dry-run` prints `loop: no start picture yet,
  this clip seeds one`. Review judges it with the loop measure (D-6). If it passes, it is accepted (no extra cost). If
  it fails only on loop continuity, its frame 0 becomes the identity's pin: `asset_baker keyframe-from-clip --at 0
  --output assets/ai_keyframes/<slug>_idle.png`, the clip is rejected with reason `seed: frame 0 kept as the loop
  pin`, and the next run pins. Frame 0 is chosen because a push-in makes it the widest framing, with the whole
  subject inside the frame. Evidence says text-to-video always pushes in, so §6 counts one seed per new identity.
- **The pilot's pin** is committed as `assets/ai_keyframes/plasma_siphon_dome_idle.png`: frame 0 of `0dbd7948`,
  1280×720 PNG, SHA-256 `c16db35c46ea07069528a09cd50ae14d2ae5c8869a8326822f623c5ead6497e1`, which is today
  `~/scratch/veo/wp4_keyframes/plasma_siphon_dome_idle.png` (the same picture ledger line 21 sent as `image`). The
  batch evidence records its origin.
- **Dry run** prints, per clip, `start: <path or source>` and `end: same as start` or `end: none`.

### D-3. One picture per identity: chained states use the pin

`ChainedFromIdle` today extracts the Idle clip's frame at 1.0 s. Two changes make all states of an identity start
from one picture, and make Constructing (pinned with `EndOnStartFrame`) end on exactly the frame Idle starts with:

- when the identity has an authored Idle picture that satisfies the pin rules (its pin), chained states use that file;
- otherwise they use the Idle clip's **first** frame (`CHAIN_KEYFRAME_TIME` becomes 0 s instead of 1.0 s). The 1.0 s
  value had no stated reason. Frame 0 is the picture the Idle loop restarts on, and it is a 16:9 frame of a real
  clip, so it satisfies the pin rules. `foundry_core_dome`'s Constructing (batch 2) uses this: its committed Idle clip
  is kept, and its square `foundry_core_dome_idle.jpg` is not a valid pin.

The keyframe docs in `prompts.rs` and the dry-run text say which applies.

### D-4. Records

- Ledger `request_started` gains `last_frame_sha256: Option<String>`, `#[serde(default, skip_serializing_if =
  "Option::is_none")]`, so the 25 committed lines parse unchanged and old clips keep their exact records.
- Provenance `ClipOrigin::Veo` gains `last_frame_sha256: Option<String>` (`#[serde(default)]`), and
  `StateProvenance` gains `seam: Option<SeamRecord>` (`#[serde(default)]`): the sampling used (`Loop` or `Span`),
  the scale correction applied to the first kept frame in per mille (0 when skipped), and the cross-fade frames. The
  strict audit adds a reason to `UnverifiedReason`: a `Veo` origin's `last_frame_sha256` differs from its ledger
  line. Nothing in the manifest changes, so the client is untouched.

### D-5. Seam repair in the bake

All three steps run in `decode_framed_clip`, on the kept content frames, before `frame_to_square`, only for states
with a `seam` (D-1). They change pixels of real frames only by resampling and blending them; no picture content is
made up (charter §1.2), and the provenance says what was done (D-4).

1. **Loop sampling.** `evenly_spaced_indices` keeps the first and the last frame. In a loop the last frame is followed
   by the first, so for a pinned clip (last ≈ first) the same picture shows twice at the seam, and for any clip the
   step across the seam is one source frame shorter than the others. A new `evenly_spaced_loop_indices(available,
   wanted)` returns `i·(available − 1)/wanted` for `i` in `0..wanted`: the final frame is left out and every step,
   the wrap included, has the same length. Held and one-shot states keep `evenly_spaced_indices` (a held state needs
   its last frame). Atlases already committed are not re-baked by this plan.
2. **Scale lock** (owner's option c). For each kept frame `k`, the zoom estimator of D-6 (shared code, new module
   `loop_metrics.rs`) measures its scale and centre offset against kept frame 0. The baker fits `ln(scale)` and the
   two offsets as straight lines in `k` by least squares (a steady push-in is a straight line in `ln(scale)`; a fit,
   not per-frame values, so estimator noise cannot make the subject wobble). Then:
   - if the fitted growth over the clip is under 0.5 %, nothing is done (recorded as 0);
   - if the residual of the fit exceeds 0.5 % of scale, or the growth exceeds 12 %, the bake stops with a typed error
     `SeamRepairRefused` naming the clip and the numbers: the subject moves or changes in a way a zoom does not
     explain (for example the morph in `a1eb317d`), and review must reject the clip;
   - otherwise every kept frame is enlarged about the fitted centre to the framing of the last kept frame, the most
     zoomed in (a crop-in: nothing outside the original frame is needed, and at most the measured growth, 7 to 10 %
     in the batch, is cut from the frame's border, which on these clips is plain backdrop).

   To keep detail, a state with `scale_lock` is decoded by ffmpeg at twice the frame size; the enlargement samples it
   bilinearly at sub-pixel positions, and a 2:1 box filter brings it to the frame size. Integer-pixel crops (ffmpeg's
   `zoompan`) are not used, because their rounding makes the subject jitter.
3. **Cross-fade** (owner's preference over ping-pong). The last `N` kept frames are blended towards kept frame 0 in
   linear light, with weights `(j + 1)/(N + 1)` for `j` in `0..N`, so the loop eases into its first frame. After the
   scale lock both frames have the same framing, so the blend reads as motion settling, not as a double image. With
   16 frames per structure state, `N = 2` blends the last 12 % of the loop; a unit state with 10 frames blends 20 %.

**Ping-pong is not built.** The owner prefers the two steps above, and ping-pong would need a manifest flag and a
change to both client frame computations (`bubble_shader.rs`, `scene_controller.rs::swarm_frame_index`). If a clip
ever cannot be repaired and its motion has no direction, a later amendment can add it.

**Frame budget.** Unchanged: 32 frames per atlas split over the states; the repair stores nothing extra.

### D-6. The review criterion "loop continuity", measured

A new subcommand `asset_baker loop-check --clip <file> --unit <slug> --state <state>` (and the same numbers printed
by `review` for every looping state) works on **the frames the atlas will hold**: the clip decoded through
`decode_framed_clip` at the bake's size, rate, framing and per-state budget, sampled and repaired as D-5 says. It
prints every number twice, `raw` (loop sampling only) and `repaired` (after D-5), and judges the repaired one:

1. `wrap`: SSIM between the last kept frame and the first (the step the loop shows when it restarts);
2. `step`: the median SSIM of consecutive kept frames;
3. `worst`: the lowest SSIM of all steps of the cycle, the wrap included (a cross-fade could move a jump from the
   wrap to the step before the blend; this catches it);
4. `zoom`: for each kept frame, the scale `s` in 0.85 to 1.15 (steps of 0.005, offsets up to ±4 % of the side) that
   best maps it onto frame 0 over the centre 80 % window, reported as growth `1/s − 1`; the largest is printed;
5. for a pinned clip, `start fidelity`: SSIM between decoded frame 0 and the pin picture, framed the same way.

SSIM is the standard luminance SSIM (Wang et al. 2004: 8×8 windows, `C1 = (0.01·255)²`, `C2 = (0.03·255)²`, mean
over windows), implemented in `loop_metrics.rs` (WP2) on the decoded `RgbaImage`s: deterministic, no new
dependency, unit tested on known pairs (identical images give 1.0; a constant shift lowers it predictably).
`loop-check` itself lives in `clip_review.rs` (WP3).

**Threshold (pass), on the repaired frames:** `worst ≥ step − 0.03` **and** largest `|growth| ≤ 1.5 %`; for a
pinned clip also `start fidelity ≥ 0.90`. Rationale: a seamless loop's restart should look like any other step, and
the margin of 0.03 is far below the smallest gap the batch measured (0.18 at full resolution, `a1eb317d`); 1.5 %
growth moves the edge of a subject filling a 512 px frame by about 4 px, and much less on screen at bubble size,
while the rejected clips grew 7 to 10 %.
The visual check stays: the first/last pair and, for a repaired clip, the last four frames and frame 0 side by side.

**Calibration (WP3, no spend):** the five rejected clips in `~/scratch/veo/clips/plasma_siphon_dome/idle/` must all
fail raw (on `worst`, on `zoom`, or both); a synthetic loop made with ffmpeg's `testsrc2` filter, cut to an exact
period, must pass raw; the same loop with a 5 % zoom ramp must fail raw and pass repaired. Then, free of charge, the
repaired numbers and contact sheets of the four push-in clips (`2aedc376`, `4567a693`, `282f8618`, `0dbd7948`) show
whether the repair alone fixes them; `a1eb317d` must be refused by the scale lock (morph). The thresholds and the
measured values go into the WP3 report and into the Veo plan's D-7 criteria, replacing "close to its first".

---

## 4. Work packages

Common rules: as the Veo plan §4 (own worktree, `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/loops-wpN`,
logs under `~/scratch/logs/veo_loops_*`, failing tests first with the red run pasted in the report, `//!` and `///`
docs, typed errors, no `unwrap`, `expect` only with `invariant:`, no catalog identity in Rust, never read `.env` or
`tmp/`, check `docs/questions/open/` before each push). Gate: `cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets --locked -- -D warnings`, `cargo test --workspace --locked`. No test makes a network call.

### WP1: Loop methods, `lastFrame`, pin rules, records (no spend)

Files: `crates/asset_baker/src/{prompts.rs, veo_client.rs, veo_plan.rs, veo_run.rs, ledger.rs, provenance.rs (origin
field only), error.rs, main.rs}`, `assets/catalog/media_style.ron`, `assets/ai_keyframes/plasma_siphon_dome_idle.png`,
tests `veo_tests.rs`, `veo_cli_tests.rs`, `ledger_tests.rs`, `prompt_tests.rs`.

1. Tests first, on the existing `FakeServer` and `CurlShim` (`crates/asset_baker/tests/common`):
   - a pinned Idle start request carries `instances[0].image` and `instances[0].lastFrame` with identical
     `bytesBase64Encoded` and `mimeType: "image/png"`; an Attack request has no `lastFrame`; an Idle with
     `pin_end: false` (an override) has none;
   - a PNG named `.jpg` is sent as `image/png` (magic bytes); a GIF is refused before any request;
   - `request_started` carries `last_frame_sha256` equal to `keyframe_sha256` for a pinned clip and omits it
     otherwise; every line of the committed `docs/evidence/veo/ledger.jsonl` still parses;
   - `validate-prompts` reports `PinShape` for a square pin, a pin over 20 MB and a 4:3 pin, and spends nothing;
   - a pinned Idle with no picture is planned text-to-video and the dry run says it seeds a pin;
   - Walk and Constructing of an identity with a valid authored Idle pin send that pin (D-3); without one they send
     the Idle clip's frame 0, and a square authored Idle picture is not used as their pin;
   - a run in Gemini mode that would send `lastFrame` is refused with `LastFrameNeedsVertex`; the fake server sees no
     request;
   - a start answered HTTP 400 `INVALID_ARGUMENT` naming `lastFrame` writes `start_failed`, ends the run (a new
     failure kind, Veo plan D-4) and is not retried;
   - `media_style.ron` with a `seam` on Attack, a looping state without an entry, `crossfade_frames: 5`, or an
     unknown override slug is refused at load.
2. Implement D-1 to D-4 (the provenance part of D-4 is only the `ClipOrigin` field; `seam` is WP2's).
3. Commit the pilot's pin (D-2) after checking its SHA-256 matches the value above.
4. Update the `//!` docs of the touched modules and `docs/evidence/veo/2026-11-04-api-notes.md` with §2.1's findings
   and links.

### WP2: Seam repair in the bake (no spend)

Files: new `crates/asset_baker/src/loop_metrics.rs` (SSIM, zoom estimator, line fit), `ffmpeg_importer.rs`,
`framing.rs` (the 2× decode filter), `provenance.rs` (`seam` record), `error.rs` (`SeamRepairRefused`), tests
`bake_integrity_tests.rs`, `provenance_tests.rs`, `ffmpeg_tests.rs`, new `loop_metrics_tests.rs`.

1. Tests first: SSIM of identical frames is 1.0, of an image and its 1-px shift below 1.0 and above 0.8, of unrelated
   noise below 0.2; the estimator recovers a known 5 % scale and a 6 px offset within 0.5 % and 1 px; the line fit
   of an exact ramp has zero residual; `evenly_spaced_loop_indices(96, 16)` never contains 95 and has equal steps;
   a fixture clip made by ffmpeg with a 6 % zoom ramp is baked with growth recorded as about 60 per mille and its
   repaired frames all within 1.5 % of frame 0; a ramp with a sudden jump is refused with `SeamRepairRefused`; a clip
   with 0.3 % drift is left unscaled; the cross-fade weights for `N = 2` are 1/3 and 2/3 and frame 0 is unchanged;
   a state without `seam` bakes byte for byte as before (compare with a fixture bake made before the change); old
   provenance files without `seam` still parse.
2. Implement D-5 and the `seam` part of D-4. The bake takes each state's `LoopSpec` as an argument from
   `bake-unit-videos`, which reads it from `MediaStyle` (WP1); until WP1 merges, WP2's tests pass it directly, and
   whichever WP merges second wires the call in `main.rs`.
3. Every heavy loop (SSIM windows, bilinear resampling) that departs from the plain iterator form carries a
   `// PERFORMANCE RATIONALE:` block; a 16-frame repair at 2× of 512×288 must stay under a second in release.

### WP3: `loop-check` and calibration (no spend)

Files: `crates/asset_baker/src/{clip_review.rs, main.rs}`, test `veo_cli_tests.rs` (or a new `loop_check_tests.rs`),
the Veo plan's D-7 text.

1. Tests first: the synthetic loop passes raw; the zoom-ramp loop fails raw and passes repaired; a loop with a hard
   cut two frames before the end fails on `worst` even after a cross-fade (fixtures made by ffmpeg at test time, as
   `ffmpeg_tests.rs` does).
2. Implement D-6 on WP2's `loop_metrics.rs` and print its numbers from `review` for every looping state.
3. Calibrate on the five rejected clips (no spend), raw and repaired, and paste the table and the repaired contact
   sheets in the report. If a rejected clip passes raw, the threshold is wrong: stop and revise D-6 before WP4. Record
   whether the repaired `0dbd7948` passes; it is the fallback clip of WP4.
4. Amend Veo plan D-7's loop-continuity criterion to cite `loop-check` and its threshold.

### WP4: Pilot of the method (1 generation)

1. Confirm WP1 to WP3 merged; `validate-prompts` and `generate-veo --unit plasma_siphon_dome --states idle --dry-run`
   print `start: assets/ai_keyframes/plasma_siphon_dome_idle.png`, `end: same as start`, 1 generation.
2. One logged background run, as the Veo plan's spending rule: `generate-veo --unit plasma_siphon_dome --states idle
   --max-generations 1 --ledger docs/evidence/veo/ledger.jsonl --clips-dir ~/scratch/veo/clips --videos-dir
   assets/videos -o assets/textures > ~/scratch/logs/veo_loops_pilot.log 2>&1`.
3. Outcomes:
   - **Start refused** (400 naming `lastFrame` or its bytes): no bill. Record the error in the evidence. Do not retry
     with another shape, since the documented alternative is `gcsUri`, which is out of scope. Fall back to the seam
     repair alone: set `pin_end: false` for Idle, Walk, Cast and Constructing (data), then `loop-check` and `review`
     `0dbd7948` (already paid, plain backdrop); if its repaired frames pass D-6 and D-7, accept and install it with
     `install-clip`, at no cost. Later identities are then generated without seeds (§6 shrinks by them).
   - **Accepted, clip passes `loop-check` and D-7:** method (a) confirmed. Install, record.
   - **Accepted, but `loop-check` fails** (the clip still jumps or morphs between the pinned ends): one more
     generation is allowed (a retry of batch 1's allowance) with the same pin; if that fails too, keep `pin_end` only
     if it helped measurably and rely on the seam repair, using whichever of the paid clips reviews best after repair.
4. Evidence: a "Pilot of `lastFrame`" section in `docs/evidence/veo/2026-11-04-batch-1.md` (or a dated successor):
   request shape accepted or the error, time to finish, sound track present (ffprobe), `loop-check` numbers raw and
   repaired, the first/last sheet, the verdict, the running total.

### WP5: Resume Veo plan WP4 (batch 1)

Runs Veo plan WP4 as amended there, with these changes:

1. **Carry the uncommitted test patch** `~/scratch/veo/veo_wp4_path_tests_on_shipped_media.patch` (1,098 lines,
   SHA-256 `252bbb646cf255904bd50b434cf548c54947277da8be28915e7c0fbc62d51c1f`; it deletes
   `crates/client/tests/battle_card_fixture_tests.rs`, adds `battle_card_path_tests.rs`, and removes `fixture_root`
   and its helpers from `crates/client/tests/support/battle_harness.rs`). It applies cleanly to `6682b81`
   (`git apply --check`). Apply it on the WP5 branch at the start, check the hash first, and commit it in the same
   commit that installs the batch's atlases, because its tests need them (Veo plan D-9). If it no longer applies,
   re-create it from its description in the batch-1 evidence, "State left behind". The scratch disk is not durable,
   so the WP5 worktree should take a copy of it on day one.
2. Order per identity: Idle first (pinned or seeding, D-2), its review and `loop-check`, then the identity's other
   states from the same picture (D-3).
3. Batch 1's looping clips: the five structures' Idle and `repair_shuttle` Idle and Walk; Constructing pinned for the
   hand-off. Expected seeds: `tech_module_pod`, `machining_plant_dome`, `aerospace_hangar_dome`, `repair_shuttle`
   (no 16:9 picture exists). Seeds do not use the retry allowance; regenerations after a failed review do.
4. Everything else as Veo plan WP4 steps 2 to 4, with `loop-check` output (raw and repaired) in the evidence for
   every looping clip. A clip whose raw numbers fail but whose repaired numbers pass is accepted; the evidence says
   so, and the provenance records the repair.

---

## 5. Risks and findings

| Item | Handling |
|---|---|
| Vertex refuses inline `lastFrame` | Pilot is one generation and a refusal is not billed; seam repair of the already-paid `0dbd7948` |
| `lastFrame` accepted but the clip morphs to meet it | `loop-check` start fidelity and `worst`; one retry, then seam repair |
| A clip pinned at both ends barely moves | D-7 "action readable at 512 px" still applies; the action prompt asks for motion |
| Scale lock makes the subject wobble | A straight-line fit, not per-frame values; sub-pixel resampling at 2×; `loop-check` measures growth on every repaired frame |
| Cross-fade shows a double image | Runs after the scale lock; at most 4 frames; `worst` step check; visual check of the last frames |
| The repair hides a morph | The scale lock refuses a clip its line does not explain; review still applies every D-7 criterion |
| Pin rules reject the committed square keyframes | Intended; a seed clip makes a 16:9 pin (§6 counts them) |
| The measure passes a visible jump | Calibration in WP3 on the five rejected clips before any spend; the visual first/last check stays |

**F-1 (finding; fixed by plan `2026-11-04-battle-flipbook-animation`).** Fixed on `main` by that plan's WP1
(`9d5884c`, `AttackSwing` in the snapshot), WP4 (`4bd48cf`, the GPU flipbook clock), WP5 (`ec04bf4`, the battle
plays its flipbooks on the presented simulation clock), WP3 (`1940231`, per-slice `duration_ms`) and WP6
(`d5779f7`, Lab, swarm and portrait on the same clock); measured in
[`docs/evidence/2026-11-04-battle-flipbook-animation.md`](../evidence/2026-11-04-battle-flipbook-animation.md).
`sim_core`'s dead `Entity::anim_progress` is removed by that plan's WP2, which waits for a lane B slot. The text
below is the finding as recorded. The simulation never advances `anim_progress` (`sim_core/src/entity.rs`
initialises it to zero and no system writes it; `world/snapshot.rs` copies it), so in the battle view every bubble
shows the first frame of its state's slice. Only the Visual Lab (`(elapsed · 1.5).fract()`) and the swarm preview
(`(elapsed · 2.0).fract()`) animate flipbooks, at fixed periods that ignore the manifest's `fps`. Loop seams are
therefore visible today only there, and will be in the battle once flipbooks animate. A presentation-side animation
clock (a period of `frame_count / fps` seconds per action, a phase per entity) belongs in its own plan. The
orchestrator should schedule it.

---

## 6. Generation count, cost and the weekly cap

| Step | Generations | Notes |
|---|---|---|
| WP4 pilot | 1 (+1 retry at most) | `plasma_siphon_dome` Idle is batch 1's tenth clip |
| WP5 batch 1, remaining clips | 9 | as the Veo plan's §2.4 |
| WP5 batch 1, seeds | about 4 | one per identity without a 16:9 picture; none if the pilot shows `lastFrame` is refused (WP4) |
| Batch 1 retries | at most 3 (one may be used by the pilot) | Veo plan D-4 |
| Veo WP5 batch 2 | 9 + about 3 seeds (`welder_drone`, `capacitor_dome`, `assembly_bay_dome`) + at most 3 retries | `foundry_core_dome` Constructing pins on frame 0 of its committed Idle clip (D-3), no seed |

**Required set still to generate:** about 26 generations (14 for batch 1, 12 for batch 2), at most 32 with every
retry: **$10.40 to $12.80** at list price ($0.40 per 4 s clip with sound), **$12.40 to $14.80** in total with the
$2.00 already spent. That is far inside the owner's 200 EUR.

**The weekly cap binds, not the money.** `assets/media/veo_budget.ron` keeps 12 per run and 25 per rolling 7 days,
the values the owner set in Q0024's first answer for the 19 required clips, when the allowance was thought to be a
free weekly quota. Five are used until 2026-11-11 10:01Z, so 20 remain: batch 1 (14 to 17) fits, but batch 2 (12 to
15) then waits until batch 1's own starts leave the window, about 2026-11-12, and the expansion batches (about 49
clips plus seeds) would take three more weeks. The owner's later answer (Q0024, via Q0043) is a 200 EUR allowance on
paid credit, which is about 500 clips at list price, so a weekly cap of 25 no longer protects anything the owner
asked for. **Q0047** asked whether to raise it to 60 per 7 days (12 per run unchanged). The owner answered yes on
2026-11-04 (prefer quality over saving generations, within 200 EUR), and `veo_budget.ron` now allows 60 per rolling
7 days, so batches 1 and 2 both fit this week.

---

## 7. Changes to the Veo plan

- **WP4 depends on this plan's WP4** (the pilot of the method) in addition to the letterbox plan's WP2; WP4's work is
  carried out as this plan's WP5.
- **D-7:** the loop-continuity criterion is measured by `loop-check` on the repaired frames (D-6 here); the keyframe
  chaining rule becomes D-3 here.
- **§2.1 baking:** looping states are sampled and repaired as D-5 here says.
- **§2.4 and §1:** seeds are extra generations of the required set (§6 here).
- The index row of the Veo plan says it is amended by this plan.
