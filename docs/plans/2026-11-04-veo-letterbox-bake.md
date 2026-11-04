# Blueprint: Letterbox Bake for Wide Veo Clips, and `generateAudio: false` (Q0043)

An amendment to the Veo media plan [`2026-11-04-phase-8-5-veo-media.md`](2026-11-04-phase-8-5-veo-media.md). It
changes how `asset_baker` fits a 16:9 clip into a square game frame, makes the Veo request parameters data, and gets
the held pilot clip accepted and installed. That plan's WP4 (batch 1) starts only after this plan's WP2.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-04 against `main` at `6a3815f`.
No Veo request was made while planning. Line numbers drift; search for the quoted symbol. A claim that could not be
confirmed while planning is marked **verify**, and the first step of its package confirms or refutes it.

**Decision being carried out.** [Q0043](../questions/closed/0043-veo-square-crop.md), answered 2026-11-04 by the
orchestrating agent under the owner's standing rule in Q0020: option (a), the baker fits the whole 16:9 frame into
the square with backdrop-coloured bars and cuts nothing; also try `generateAudio: false` on the next paid clip, and if
the API refuses the field, record it and drop it. The owner can overrule either choice.

**Spending.** This plan spends **nothing**. Running total: 1 generation, **$0.40 of the owner's $15 target** (Q0024;
ceiling $50), 1 of 25 generations in the rolling week (pilot evidence,
[`2026-11-04-pilot.md`](../evidence/veo/2026-11-04-pilot.md), "Cost"). The first live use of `generateAudio: false`
is WP4's first clip of the Veo plan.

**Out of scope:** portrait (9:16) generation (Q0043 option b); prompt changes (option c); any shader change; re-baking
an atlas that is already committed (§2.4).

---

## 1. Summary

| WP | Title | Spends | Size | Depends on |
|---|---|---|---|---|
| WP1 | Framing policy in data, letterbox bake with a measured bar colour, framing in provenance and in the review sheet; Veo request parameters in data with `generateAudio` | No | M (about 1 day) | none |
| WP2 | Accept and install the held pilot clip; scratch bake and visual check; close Q0043 | No | S (2 to 3 hours, plus one locked GPU capture) | WP1 merged |

Veo plan WP4 then starts (see §5).

---

## 2. Current state and findings

### 2.1 Where the crop happens

`crates/asset_baker/src/ffmpeg_importer.rs`, `extract_clip_frames`: one ffmpeg filter,
`crop=min(iw\,ih):min(iw\,ih),scale={frame_size}:{frame_size},fps={fps}`. Every bake goes through it:
`extract_and_pack_video` (single clip, `bake_concept_video`, `bake_custom_video`) and
`bake_multi_action_videos_with_states` (via `probe_clip`), which `provenance::bake_with_provenance` and the CLI
`bake-unit-videos` call. Decoded frames are opaque RGB PNGs converted to RGBA with alpha 255.

### 2.2 How the client shows an atlas

- `crates/client/src/media.rs` uploads the strip as a `Rgba8UnormSrgb` texture array, one layer per frame.
- `assets/shaders/bubble_dome.wgsl` samples **only `.r`, `.g`, `.b`** of the layer. The texture's alpha is never read;
  the bubble's alpha comes from `optics_signals.y` and the Fresnel term. There is **no chroma key** and no keyed
  background: the clip's backdrop is shown as it is, inside the glass.
- The media covers a disc: `lens_uv = 0.5 + disk * lens_scale` with `lens_scale` 0.48 (0.35 at the centre with
  `FLAG_INNER_WARP`), for both `UnitSphere` (screen-space disc) and `BuildingDome` (hemisphere normals). So a square
  frame's corners are never seen, but its top and bottom bands are.

**Consequence:** transparent bars are not possible without a shader change; with alpha ignored they would render as
whatever RGB the transparent pixels hold. Bars must be opaque, and they are visible inside the bubble wherever their
colour differs from the clip's own backdrop.

### 2.3 The backdrop is not the prompted colour

The style sentence in `assets/catalog/media_style.ron` asks for a "dark slate-indigo radial studio backdrop
(#0B101B)". Veo did not follow the hex value. Measured on the pilot's frame at 1.0 s (ffmpeg area-average of the
outermost 4 rows): top `(47, 51, 77)`, bottom `(40, 44, 70)`, about `#2C304A`, with lighter and darker radial rays.
`pulse_bot`'s idle clip measures `(2, 3, 8)` at the top: each clip has its own backdrop.

Planning mock from the real pilot frame, committed as
[`plasma_siphon_dome_idle_framing_mock.png`](../evidence/veo/review/plasma_siphon_dome_idle_framing_mock.png): left,
today's centre crop (base and ramps cut); middle, letterbox with the prompt's `#0B101B` (bars read as two black
stripes); right, letterbox with the measured `#2C304A` (bars continue the backdrop; only the rays stop at the seam).

### 2.4 Existing atlases

| Atlas | Shape | Source | Provenance | Effect of this plan |
|---|---|---|---|---|
| `pulse_bot` | `UnitSphere` | 1280×720 legacy Veo clips | yes, v1, pins atlas SHA-256 `5e92b7c6…` | none: units keep the centre crop (D-2), the record stays valid |
| `foundry_core_dome` | `BuildingDome` | 1280×720 legacy Veo clips | none (legacy) | none now; Veo plan WP5 re-bakes it with its Constructing clip, and that bake letterboxes both states |
| `psi_grey`, `rhino_beetle` | `UnitSphere` | legacy Veo clips | none | none |
| `welder_drone`, `capacitor_dome`, `assembly_bay_dome`, `concept_video` | | copies of `concept_video.png` | none | none; the Veo plan's WP5 replaces the three copies |

**No committed atlas changes and none is re-baked in this plan.** The audit verifies committed hashes, not a
re-bake (Veo plan §5), so `pulse_bot` stays `Verified`.

### 2.5 Request parameters

`VeoParameters` (`veo_client.rs`) has `aspectRatio`, `durationSeconds`, `sampleCount`, all `Option`, serialized
only when `Some`. `VeoClient::parameters()` returns the Rust literal `VeoParameters::default()` (16:9, 4 s, 1
sample). `veo_run.rs` writes the same value into the ledger's `request_started.parameters` before the start request.
A start refused with an HTTP error other than a quota refusal writes `start_failed` and ends the run (the Veo plan's
D-4 "first failure of a new kind"); its `request_started` line still counts toward the rolling budget (conservative,
unchanged here).

---

## 3. Design decisions

### D-1. Fit inside, never cut, for the shapes that need it

**Letterbox** scales the whole clip frame to fit inside the square (`scale=S:S:force_original_aspect_ratio=decrease`
in ffmpeg, so a 1280×720 clip at `S = 512` becomes 512×288), then the baker composes it centred on an `S×S` canvas in
Rust. Bars go on the short axis: top and bottom for a wide clip, left and right for a tall one, none for a square
clip (whose result must then equal the centre crop's). The composing code is plain integer arithmetic on
`RgbaImage`, so the same decoded frames always give the same bytes.

**Centre crop** keeps today's filter string byte for byte, so a crop bake is unchanged.

### D-2. Which identities letterbox: by render shape, in data

The policy lives in `assets/catalog/media_style.ron` (house style, read by `asset_baker` alone), keyed by the
archetype's render shape, never by identity:

```ron
framing: (
    unit_sphere: CenterCrop,
    building_dome: Letterbox(feather_permille: 30),
    overrides: {},          // slug -> framing, for one identity whose clip needs the other treatment
),
```

- **Structures (`BuildingDome`) letterbox.** They are wide, Veo fills the 16:9 width with them, and the pilot
  shows that a crop loses the base and ramps.
- **Units (`UnitSphere`) keep the centre crop.** Units are taller than wide and centred (`pulse_bot` fits the crop);
  letterboxing would shrink them to about 56 % of the frame height inside an already small bubble, and their
  committed atlas would have to change for no gain.
- **`overrides`** names a slug in data when one clip needs the other treatment (for example a wide flying unit), so
  a good paid clip need not be regenerated. A slug that is not in the catalog is refused when the file loads.

`bake-unit-videos` already loads the catalog (`run_bake_unit_videos`); it resolves the identity's framing from its
`archetype.shape` and the policy and passes it to `bake_with_provenance`. The match is on `ShapeKind`, a render enum,
so no identity appears in Rust (charter §3.3).

### D-3. Bar colour: measured from the clip's own border

Not the prompt's hex value (§2.3 shows Veo ignores it and the bars would show as stripes), not transparent (§2.2).
For each clip, after decoding and before packing:

- take the outermost `band = max(1, short_side / 64)` rows (or columns) along each edge that receives a bar (4 rows
  of a 512×288 frame), in **every frame kept** for the clip;
- the bar colour is the **per-channel median** of all those pixels, as integers. The median ignores a subject that
  touches the edge, such as the pilot's vapour plume, and one colour per clip means the bars never flicker;
- the colour is written into the provenance record (D-5), so a reader sees what was chosen.

**Feather.** A linear blend over the content's outermost `feather = S * feather_permille / 1000` rows (15 px at 512)
from the frame's own pixels to the bar colour, in integer arithmetic. It removes the hard line where the radial rays
end (mock, right panel). `feather_permille: 0` disables it. The value is data; WP2's capture decides whether 30 stays.

### D-4. The review shows what ships

`review` today tiles 8 full 16:9 frames. It gains a second sheet, `<slug>_<state>_framed.png` (full size in
`~/scratch/veo/review/`, a copy at most 1,024 px wide in `docs/evidence/veo/review/`), built from the same framing
function the bake uses for that identity, so the criterion is judged on the shipped square. The Veo plan's D-7
criterion "subject stays inside the square crop" becomes "subject stays inside the framed square (letterbox for
structures, crop for units, per `media_style.ron`)".

### D-5. Provenance records the framing

`BakeParameters` gains `framing: BakeFraming` (`CenterCrop`, or `Letterbox { feather_permille }`) and
`StateProvenance` gains `bar_rgb: Option<[u8; 3]>` (the measured colour, `None` for a crop or a square clip). Both
fields are `#[serde(default)]` (`CenterCrop`, `None`), so existing records, such as `pulse_bot`'s, parse unchanged
and say truthfully that they were cropped. `PROVENANCE_FORMAT_VERSION` stays 1: the change only adds defaulted
fields, every v1 file stays readable, and the audit's hash checks are unaffected.

### D-6. Veo request parameters in data, with `generateAudio`

**Overruled in part by the owner (Q0043's answer, 2026-11-04):** keep sound, which the game may play when a unit is
clicked. The data mechanism below stays; WP2 set `generate_audio: None` (no field sent, as in the pilot) in
`assets/media/veo_request.ron`, and the shipped-settings test now asserts that no `generateAudio` key is sent.

New data file `assets/media/veo_request.ron` (not secret), read by `generate-veo`, and shown by `--dry-run`:

```ron
(
    aspect_ratio: "16:9",
    duration_seconds: 4,
    sample_count: 1,
    generate_audio: Some(false),   // None: the field is not sent (the pilot's request)
)
```

`VeoParameters` gains `generate_audio: Option<bool>`, serialized as `generateAudio` only when `Some`. The client is
built with the loaded parameters (replacing the `VeoParameters::default()` literal in `VeoClient::parameters`), and
the ledger records them as sent. Old ledger lines without the field still parse (`Option` fields default to `None`).
Data rather than a CLI flag, because the fallback ("if refused, drop it") is then a one-line data edit, reviewed and
committed with its reason, and every later run follows it.

**If Vertex refuses the field** (a 4xx on the start request naming `generateAudio`; a refused start costs no money),
the run stops with `start_failed` in the ledger. The agent then sets `generate_audio: None`, records the refusal in
[`2026-11-04-api-notes.md`](../evidence/veo/2026-11-04-api-notes.md) and the batch evidence, and reruns. **If it is
accepted**, the evidence records that the downloaded clip has no audio stream (`ffprobe`) and the price row used
($0.08 per second without audio, $0.32 a clip), with the caveat that billing itself is not visible to the agent.

---

## 4. Work packages

Common rules (as in the Veo plan §4): own worktree, `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/veo-lb-wpN`,
logs `~/scratch/logs/veo_lb_wpN_*`, `df -h / ~/scratch` first, target removed after merge. Failing test first, red run
pasted in the report. `//!` on new files, `///` on public items, `thiserror` errors, no `unwrap`, `expect` only with
`invariant:`, no function over 100 lines, no identity in Rust (slugs only in data and test fixtures). Never read `.env`
or `tmp/`. No Veo call. `docs/questions/open/` checked before each push. Gate:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

### WP1: Framing and request parameters (no spend)

**Files:** `crates/asset_baker/src/{ffmpeg_importer.rs, provenance.rs, clip_review.rs, prompts.rs, veo_client.rs,
veo_run.rs, main.rs, lib.rs}` (a new `src/framing.rs` holds the policy type, the border median, the feather and the
composer), `assets/catalog/media_style.ron`, new `assets/media/veo_request.ron`; tests below.

1. **Red first, through the public CLI** (no new API needed, so the failure is a real assertion, not a compile
   error). A test helper writes a synthetic wide clip with `/usr/bin/ffmpeg` into a temp dir: 192×108, 1 s, 8 fps,
   `yuv420p`, backdrop `(44, 48, 74)`, a 12 px saturated green column at each side edge spanning only the middle
   rows (so the top and bottom rows are pure backdrop). Test `a_structure_bake_keeps_the_edges_a_centre_crop_loses`
   (`crates/asset_baker/tests/framing_tests.rs`) runs `bake-unit-videos` for a fixture structure (catalog fixture with
   a `BuildingDome` archetype) and asserts green pixels near both side edges of every atlas frame. Today it fails:
   the crop removes both columns. Paste the red run.
2. **Unit and integration tests**, added with the code:
   - the bars equal the recorded `bar_rgb` exactly, and `bar_rgb` is within ±4 per channel of the synthetic
     backdrop (`yuv420p` rounding);
   - a bright plume touching the top edge over 20 % of its width does not move the median off the backdrop;
   - a square clip bakes byte-identically under both framings; a 108×192 clip is pillarboxed (bars left and right);
   - the feather changes only the outermost `feather` rows of the content, monotonically toward the bar colour;
     `feather_permille: 0` leaves a hard edge;
   - the centre-crop filter string equals today's literal (golden), so crop bakes cannot drift;
   - `bake-unit-videos` picks `Letterbox` for a `BuildingDome` fixture, `CenterCrop` for a `UnitSphere` fixture, and
     the `overrides` entry when present; an override naming an unknown slug fails `media_style.ron` loading with a
     typed error;
   - provenance: a letterbox bake records `framing` and `bar_rgb`; the committed `pulse_bot.provenance.ron` (no
     `framing` field) loads as `CenterCrop` and `audit-media` still reports `pulse_bot` `Verified`;
   - `review` writes the framed sheet with the identity's framing (letterboxed tiles for the structure fixture);
   - request parameters: with the existing `FakeServer`/`CurlShim` (`tests/common`), the start body carries
     `"generateAudio":false` when the data says `Some(false)` and no such key for `None`; the ledger's
     `request_started.parameters` matches the body; the committed `docs/evidence/veo/ledger.jsonl` lines still parse;
     a malformed `veo_request.ron` is a typed error before any credential is read.
3. Implement D-1 to D-6. Update `ffmpeg_importer.rs`'s `//!`, the `PERFORMANCE RATIONALE` block that names the crop,
   `lib.rs`'s module list, and the `///` of `FfmpegExtractConfig` and the bake functions, which say "center-crops".
4. Measure at 512 px on the pilot clip in a scratch root (no commit): bake time before and after (the extra median
   and compose pass over 32 frames should be negligible; record it).

**Gate:** common gate. `audit-media` on the real `assets/` reports the same statuses as before (paste both).

### WP2: Accept and install the pilot clip; close Q0043 (no spend)

1. Confirm WP1 is merged. Re-run `asset_baker review --unit plasma_siphon_dome --ledger docs/evidence/veo/ledger.jsonl
   --sheets-dir ~/scratch/veo/review --evidence-dir docs/evidence/veo/review`. Look at the framed sheet (image read)
   and record the verdict against the amended D-7 criteria in a new section "Acceptance after the letterbox change"
   of [`2026-11-04-pilot.md`](../evidence/veo/2026-11-04-pilot.md): expected pass on every criterion, the dome whole
   inside the square.
2. **Install:** `asset_baker install-clip --unit plasma_siphon_dome --state idle --clip
   ~/scratch/veo/clips/plasma_siphon_dome/idle/2aedc376-7dea-454e-88df-0176ec15c700.mp4 --videos-dir assets/videos
   --ledger docs/evidence/veo/ledger.jsonl` (flags as `install-clip --help` prints them, **verify**). Commit
   `assets/videos/plasma_siphon_dome_idle.mp4` (1.35 MB). It must be installed before WP4, because `generate-veo`
   reuses an installed Idle clip and chains the Constructing keyframe from its 1.0 s frame (`veo_plan.rs`
   `existing_idle`).
3. **Scratch bake and visual check.** `bake-unit-videos --unit plasma_siphon_dome --idle … -o
   ~/scratch/veo/letterbox-root/textures --ledger …` into a scratch root (as in the pilot evidence), then render it in
   the real client on the RX 580 with `SPACE_SCUFFLE_ASSETS` pointing at the scratch root: one
   `client_diagnostics battle_capture` run with a scratch scenario that places the structure, as one locked command
   (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock …`, charter §7). Look at it: no visible
   stripe at the bar seams inside the dome; the whole dome visible. If the stripe shows, adjust
   `feather_permille` in data and repeat (each capture its own lock). If the client's media gate will not show an
   identity that lacks its Constructing state (**verify**), record that, rely on the framed sheet here, and do the
   in-game check in Veo plan WP4 step 3, which captures each enabled card anyway. **The committed atlas is not baked
   here:** WP4 bakes Idle and Constructing together, so the repository gets one atlas blob, not two.
4. Close Q0043: `## Resolution` naming WP1's and this WP's commits, `git mv` to `docs/questions/closed/`.
5. Records: this plan's row → Done with commits; the Veo plan's row notes the amendment; pilot evidence cost line
   unchanged ($0.40, 1 generation).

**Gate:** common gate (no code change expected; `cargo test --locked -p workspace_checks` at least).

---

## 5. Changes to the Veo plan

Made in the same commit as this blueprint:

- **WP4 starts after this plan's WP2.** The pilot's Idle clip is then installed and accepted, so batch 1 needs 9
  generations as planned. The first clip of WP4 is the first live request with `generateAudio: false` (D-6); WP4's
  evidence records the outcome and the price row used. With silent clips accepted, batch 1 costs 9 × $0.32 = $2.88
  at list price, otherwise 9 × $0.40 = $3.60; the running total would then be $3.28 or $4.00 of the $15 target.
- **D-7's criterion** "subject stays inside the square crop" becomes "inside the framed square" (D-4 above).
- **§2.1's "Baking" line** notes the framing policy.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| The bar seam is visible inside the dome | Measured colour, feather, an in-game capture in WP2 (or WP4); the feather is data |
| The structure looks small in its dome (about 56 % of the frame height) | Accepted by Q0043's decision; the capture shows it; the owner can overrule (Q0020's rule) |
| A wide unit is cut by the crop | `overrides` in data letterboxes one identity without regenerating its clip |
| Vertex refuses `generateAudio` | Refused start costs no money; one data edit drops it (D-6) |
| A later ffmpeg changes decoded pixels and so the median | Provenance records the ffmpeg version and the colour; the audit checks committed hashes, not a re-bake |
| Old provenance or ledger files stop parsing | Defaulted fields; tests over the committed `pulse_bot` record and the committed ledger |

## 7. Owner questions

None new. Q0043 is answered and is closed by WP2. Q0024's cost target is tracked in §5.
