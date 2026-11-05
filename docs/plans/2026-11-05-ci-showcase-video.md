# Blueprint: A Showcase Video Rendered by CI and Shown in the README

The owner asked (chat, 2026-11-05): "please add a round which let's the ci generate a video (or animated webp/gif)
demonstrating battle, fog, mist, units... if possible referenced from the readme. the video should give a nice intro
and allow users to see where the project is (/is heading)."

This plan adds a `showcase` tool to `crates/client_diagnostics` that renders a scripted, deterministic tour of a real
battle through the production client, frame by frame at a fixed simulated rate, with title, caption and status cards
drawn in-engine. It encodes the frames into an MP4 and a size-capped animated WebP teaser. A new GitHub Actions
workflow runs it on software Vulkan (`lavapipe`) after every green CI run on `main` and deploys the result to GitHub
Pages. A new root `README.md` embeds the teaser and links to the full video.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-05 against `main` at `c85644c`.
Line numbers drift; search for the quoted symbol. Claims not confirmed while planning are marked **verify**.
Estimates not yet measured are marked **estimate**. No cargo command was run while planning: gates were running.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0056 (new, WP0) Enable GitHub Pages for `close2/space-scuffle` to host the showcase? | To be asked, non-blocking. Default: the agent enables Pages with source "GitHub Actions" in WP5 | D-10, WP0, WP5 |
| [Q0020](../questions/closed/0020-smog-velocity-quiescence.md) and the owner's standing rule that visual choices are the agent's | Visual choices in this plan are the agent's (caller's brief) | D-6, §4 |
| [Q0034](../questions/open/0034-client-test-consolidation-after-guard-trip.md) CI disk guard | Open. The showcase runs on its own runner and keeps its own disk guard; it changes nothing in `ci.yml` | D-12 |
| [Q0009](../questions/closed/0009-terrain-anisotropy-test-override.md) terrain anisotropy on old lavapipe | Closed; the showcase applies the same rule as the GPU harness | D-12, WP2 |

---

## 1. Summary

**Pipeline, in five lines.**

1. `showcase take` runs the production client (`client::build_graphical_app`, `ClientMode::Skirmish`) on
   `assets/scenarios/showcase.ron`, with time advanced by exactly 40 ms per frame (`TimeUpdateStrategy::ManualDuration`),
   so one rendered frame is one simulation tick at 1× and the result does not depend on how fast the GPU is.
2. A director reads `assets/captures/showcase.ron` (the storyboard) and, per scene, sets only the camera, the battle
   speed (pause, 1× to 4×) and an egui overlay. Scene boundaries can wait for real events (first hostile damage), so
   the storyboard survives simulation changes.
3. Every frame is read back with Bevy's `Screenshot` and streamed in order to an `ffmpeg` child, which writes a
   lossless FFV1 mezzanine; a manifest records each frame's tick, checksum and BLAKE3 hash.
4. `showcase assemble` cuts and encodes `showcase.mp4` (H.264, 1280×720, 25 fps, ≤ 25 MiB), `showcase.webp`
   (teaser, 800×450, 12.5 fps, ≤ 4.5 MiB), `showcase-poster.png`, `showcase.json` and an `index.html` player page.
5. `.github/workflows/showcase.yml` runs `showcase render` on `lavapipe` after green CI on `main` and deploys the
   folder to GitHub Pages. `README.md` embeds `https://close2.github.io/space-scuffle/showcase.webp` and links to the
   page.

| WP | Title | Changes expected results? | Size | Depends on |
|---|---|---|---|---|
| WP0 | Q0056 and the plan's row | no (docs) | XS | — |
| WP1 | Storyboard, generated cards and the showcase scenario (pure code, no GPU) | no | M | WP0 |
| WP2 | The take: director, overlay, frame recorder, GPU smoke test, calibration | no (new tool) | L | WP1 |
| WP3 | Assemble: encoders, size ladder, player page, `render` orchestration, first full video locally | no | M | WP2 |
| WP4 | CI render job (artifact only), first CI timing and the calibration rule | no | S | WP3 |
| WP5 | Publishing to Pages, `README.md`, check C24, `DESIGN.md` and charter rows | no | M | WP4, Q0056 default |
| WP6 | Bookkeeping: evidence, plan status, Q0056 status | no (docs) | XS | WP5 |

No simulation code changes. No simulation result, golden or checksum changes. The only production-client change is
moving two pure lavapipe helpers from the test harness into `client::render_compatibility` (D-12).

---

## 2. What exists

### 2.1 The capture tool (`crates/client_diagnostics/src/runtime_capture/`)

- `battle_capture` (`src/bin/battle_capture.rs`) calls `runtime_capture::run_capture`. It builds the real client with
  `client::build_graphical_app(ClientMode::LiveBattle, None, &scenario)`, sets the window to the script's resolution
  with `set_scale_factor_override(Some(1.0))`, freezes time with `TimeUpdateStrategy::ManualDuration(Duration::ZERO)`
  and resumes with a fixed `ManualDuration(40 ms)` (`driver.rs`, `freeze`, `resume`).
- It waits for readiness (`observe_loading`, `pipelines::PipelineEvidence` for compiled pipelines, `media.rs` for the
  exact flipbook bytes), then requests `Screenshot::primary_window()` and saves the image in the `readback` observer
  (`driver.rs`, `readback`). The screenshot includes the egui layer (`ui_attachment.rs` aligns the UI attachment).
- `isolate_ui_input` (`runtime_capture/mod.rs`) clears egui input and gives egui a deterministic clock.
- The script format (`config.rs`, `CaptureScript`) is a closed set of verification goals (`Goal::Overview`,
  `RampEntry`, `Combat`, ...) with strict sequence validation. It is a verifier of still images, not a timeline. The
  showcase therefore gets its own storyboard format (D-3) and reuses the capture's readiness, pipeline evidence and
  input isolation, not its script.
- The capture switches smog **off** (`opt_smog_volume = false`, `SmogConfig.enabled = false`) so its pixel checks
  are stable. The showcase keeps the production default (smog on), because the smog is one of the things to show.
- `assets/captures/default_battle.ron` captures at 1920×900 and allows the `Combat` goal up to 2,400 ticks.

### 2.2 The frame-budget tool (`crates/client_diagnostics/src/frame_budget/`)

- It builds the production client with `PresentMode::AutoNoVsync` and measures frames; its spec lives in
  `crates/client_diagnostics/data/frame_budget.ron`. Its CI smoke test (`tests/frame_budget_tests.rs`) runs the
  debug binary through the GPU harness by including `crates/client/tests/support/gpu_run.rs` with `#[path]`.
- **The only lavapipe frame-cost figure in the repository:** "On `lavapipe` a 400-unit frame of the debug build takes
  about 2 s on the 16-core development server (180 warm-up frames in 368 s)" (`frame_budget_tests.rs`,
  `SMOKE_FRAMES`). Earlier plans say "a lavapipe frame of the shipped map takes seconds" (debug builds).
- On the RX 580 (release, 1280×720) a 400-unit frame takes about 12 ms (`DESIGN.md` §5); the smog solver costs about
  6 ms of CPU per frame during a 400-unit fight.

### 2.3 Modes, profiles, fog and smog (`crates/client`)

- `ClientMode::Skirmish` selects `BattleProfile::Player` (`lib.rs`, `ClientAppPlugin::build`): HUD, fog of war, no
  inspector. `ClientMode::LiveBattle` is the `Sandbox` profile: the 360 px egui inspector panel on the left and no
  fog. `client --skirmish --scenario <path>` combines the Player profile with any scenario (`main.rs`).
- **Fog of war in the battle hides enemies outside the viewer's `Visible` cells** (`battle_hud/fog.rs`,
  `apply_battle_fog`; `battle_pipeline/fog.rs`, `BattleFog`). It does **not** shade terrain: the terrain shader's LOS
  modulation (`assets/shaders/terrain.wgsl`, overlay bit 3, `OVERLAY_FLAG_LOS`) is set only by the editor's LOS
  preview (`editor/los_preview.rs`). The video can therefore show fog only as enemies that appear when they come
  into sight, and its caption says exactly that. Shaded terrain fog is a game feature, out of scope here (§9).
- **"Mist" is the smog.** The runtime mist was deleted in `0f31027` (remediation decision D2; `DESIGN.md` §4,
  "Runtime mist: Removed"). What exists is the volumetric smog (`client::smog`): a 3-D Eulerian field stirred by every
  visible bubble, raymarched by `assets/shaders/smog_volume.wgsl`, in a window centred on the camera focus
  (`live_battle_volume`, 28 × 22 world units, 4.8 high). The video shows and names it "smog".
- The battle clock runs at `simulation_speed` 1 to 4 (`LabState.simulation_speed`, HUD buttons) and caps a frame at
  five ticks (`battle_pipeline/clock.rs`).
- `RtsCamera` (`camera.rs`) has `target` (map plane, world units = grid cells), `distance` (3.0 to 48.0,
  `MIN_CAMERA_DISTANCE`, `MAX_CAMERA_DISTANCE`), `pitch` (radians, default 58°) and `look_at_y`.

### 2.4 Scenarios and media

- `assets/scenarios/default_battle.ron`: map `showcase_canyon` (64 × 64, two cliff levels, ramps), seed
  1592639215, both slots `battle_automaton` (2 `welder_drone`, 8 `pulse_bot`, 2 `rhino_beetle`, town hall
  `foundry_core_dome`), both attack-moving to the other's start.
- Real Veo media exists for 12 of 91 identities (`DESIGN.md` §4 "Media coverage"); every identity in
  `battle_automaton` and its town hall has an atlas in `assets/textures/`. Missing media fails loudly, so the
  showcase scenario must field only identities with media (WP1 test).

### 2.5 The GPU harness (`crates/client/tests/support/gpu_run.rs`)

- Picks `radv` or `lavapipe` from `SPACE_SCUFFLE_GPU_TEST_DRIVER` and the host; on `radv` it takes
  `/run/lock/gpu.lock` per child; children run under GNU `timeout` and `xvfb-run`, with run limits of 120 s (`radv`)
  and 300 s (`lavapipe`).
- On `lavapipe` before Mesa 25.1, a frame of hex-tiled terrain does not finish with anisotropic filtering; the
  harness sets `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY=1` after parsing `vulkaninfo --summary`
  (`mesa_release`, `anisotropy_override`, `lavapipe_release`). The development server has Mesa 25.0.7, so it needs
  the override too. Ubuntu 24.04's Mesa on the runner: **verify** (WP4 logs it).
- Manifests: `lvp_icd.json` here, `lvp_icd.x86_64.json` on Ubuntu (`LAVAPIPE_MANIFESTS`).

### 2.6 CI (`.github/workflows/ci.yml`)

- Jobs: `check-and-test` (ubuntu-latest, 60 min timeout, about 50 min per run in the last eight runs, all green on
  2026-11-05), three `determinism-native` jobs, `determinism-wasm`, `throughput`. About eight runs a day.
- `check-and-test` frees preinstalled software, installs `mesa-vulkan-drivers`, `vulkan-tools`, `xvfb`, `valgrind`
  and **`ffmpeg`** from Ubuntu's archive, uses `Swatinem/rust-cache@v2`, and stops at a disk guard needing 12 GiB
  (Q0013, Q0034: AWS runners start with about 6 GiB free).
- Workflow-level env: `SPACE_SCUFFLE_VIRTUAL_GPU=1`, `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`.
- Check C16 reads only `.github/workflows/ci.yml` (`workspace_checks/src/charter_claims.rs`, `CI_WORKFLOW`), so a
  second workflow file does not disturb it. `.github/workflows/**` is `global` in `crates/gate/suites.ron`.

### 2.7 Repository facts (read-only `gh` queries, 2026-11-05)

- `close2/space-scuffle` is **public**. Pages is **not enabled** (`has_pages: false`). The default workflow token
  permission is **read** (`default_workflow_permissions: "read"`), so every write must be declared in the workflow.
  The agent's `gh` token has admin rights on the repository. Repository size 276 MB.
- **There is no `README.md` at the repository root**, and none in history. `DESIGN.md` is the entry document.
  The workspace licence field is `MIT OR Apache-2.0` (`Cargo.toml`); there is no `LICENSE` file.

### 2.8 Fonts and tools

- egui's bundled fonts come with `bevy_egui` through `epaint_default_fonts` 0.31.1 (already in `Cargo.lock`):
  `Ubuntu-Light.ttf` (Ubuntu Font Licence 1.0), `Hack-Regular.ttf` (MIT and Bitstream Vera), `NotoEmoji-Regular.ttf`
  (OFL). Bevy's `default_font` is a FiraMono subset (OFL). No font file needs to be added to the repository, and text
  rendered into video frames does not redistribute the font file.
- The development server has Debian's `ffmpeg` 7.1.5 (`/usr/bin/ffmpeg`, with `libx264`, `libwebp_anim`, `ffv1`,
  `palettegen`). It is a distribution package, not a downloaded binary. Ubuntu 24.04's `ffmpeg` (6.1) has the same
  encoders.

---

## 3. Design decisions

### D-1. A separate `showcase` binary in `client_diagnostics`

The shipped `client` binary gains no flag (the same choice as `frame_budget`, remediation decision D3). The tool is
`crates/client_diagnostics/src/bin/showcase.rs` over a module `client_diagnostics::showcase`, with subcommands:

| Subcommand | GPU | What it does |
|---|---|---|
| `showcase take --storyboard <rel> --take <name> --out <dir>` | yes | Renders one take into `<dir>/<take>.mkv` and `<dir>/<take>.manifest.ron` |
| `showcase assemble --storyboard <rel> --takes <dir> --revision <sha> --revision-date <YYYY-MM-DD> --out <dir>` | no | Cuts and encodes the published files |
| `showcase render --driver radv\|lavapipe --out <dir> [--revision ..]` | via children | Runs `take` once per take in a child (wrapped in `with-gpu-lock` on `radv`, under `xvfb-run`), then `assemble` |

`render` exists so CI and a developer run the same sequence (D-16). It locks per take, never for the whole render
(charter §7).

The module reuses, by making them `pub(crate)`: `runtime_capture::pipelines::PipelineEvidence` (readiness),
`isolate_ui_input` (deterministic egui), and the loading observation of `runtime_capture::driver`. It does not reuse
`CaptureScript`, whose closed goal set is a verifier contract.

### D-2. Fixed-timestep offline capture

- `TimeUpdateStrategy::ManualDuration(40 ms)` for every frame after readiness. The video frame rate is therefore
  25 fps = `sim_types::TICKS_PER_SECOND`; at 1× each frame is exactly one tick, at 4× four ticks. The storyboard's
  `frame_rate` must equal the tick rate (validated), so no frame shows a fractional tick.
- Every time source the client reads is virtual time: the flipbook clock is presented simulation time
  (`flipbook_clock.rs`), the smog steps by `time.delta_secs()`, egui's clock is set by `isolate_ui_input`. The
  overlay fades are functions of the frame index, never of egui animation time.
- Readiness (pipelines compiled, media admitted, first battle tick published) is awaited with time frozen at zero
  (`ManualDuration(ZERO)`), exactly as the capture does; frame 0 is the first frame after readiness.
- Consequence: the same commit gives the same battle and, on the same driver, byte-identical frames. A slow
  `lavapipe` only makes the run take longer. WP2's test proves the byte identity on one driver.

### D-3. Takes and storyboard

A *take* is one client process, one mode, one scenario, one continuous simulation. A *scene* is a span of frames in
a take with one camera move, one speed and one overlay. The storyboard lists takes, their scenes in order, and the
teaser cut. Several takes allow later scenes in other modes (for example the Visual Lab, §9); v1 has one take,
`battle`, in `ClientMode::Skirmish` (Player profile: real HUD, real fog, no developer inspector).

Why one Skirmish take is enough for v1: it shows terrain, ramps, fog (enemies appear on sight), the HUD, units and
domes with their Veo flipbooks at close range, combat effects and smog. The Visual Lab and the swarm stress test
show the 360 px inspector panel (`lab_ui.rs`, `SidePanel::left("game_inspector")`), and hiding it would need a
production option; deferred (§9).

### D-4. Storyboard schema (`assets/captures/showcase.ron`, read at run time)

```ron
(
    format_version: 1,
    resolution: (1280, 720),
    frame_rate: 25,                       // must equal sim_types::TICKS_PER_SECOND
    takes: [
        (
            name: "battle",
            mode: Skirmish,               // Skirmish | LiveBattle (VisualLab reserved, rejected in v1)
            scenario: "scenarios/showcase.ron",
            readiness_timeout_seconds: 600,
            scenes: [
                (
                    name: "title",
                    speed: Paused,                         // Paused | Speed(1..=4)
                    length: Frames(125),                   // Frames(n) | UntilCombat(min: n, max: m)
                    camera: Orbit(center: (32.0, 32.0), distance: 46.0, pitch_deg: 62.0,
                                  start_deg: 200.0, sweep_deg: 20.0),
                    overlay: TitleCard,
                ),
                // ... §4
            ],
        ),
    ],
    teaser: (width: 800, frame_step: 2, cuts: [(scene: "title", first: 0, frames: 75), ...]),
)
```

Camera variants (all positions on the map plane in world units, angles in degrees in the file, converted once):
`Fixed(focus, distance, pitch_deg)`, `Path(keys: [(frame, focus, distance, pitch_deg)])` (smoothstep between
keys), `Orbit(center, distance, pitch_deg, start_deg, sweep_deg)`, `FollowArmy(player, distance, pitch_deg)`
(centroid of the player's living mobile units), `FollowCombat(distance, pitch_deg)` (centroid of the positions in
hostile combat events of the last 25 ticks, falling back to the midpoint of the two army centroids),
`FollowUnit(player, unit_types: [slug, ...], distance, pitch_deg)` (lowest `EntityId` alive of the first listed
type that has one alive). Follow modes are smoothed by a critically damped spring with a fixed 40 ms step
(deterministic; unit-tested), and the scene's first frame starts from the previous scene's last pose unless the
scene says `cut: true`.

Overlay variants: `None`, `TitleCard`, `Caption(title, body, subsystems: [..])`, `StatusCard`. A scene may also set
`badge: Some("Fast-forward ×4")`; the badge is required whenever `speed` is above 1, so time-lapse is never hidden
(validated).

Validation (typed `ShowcaseError::InvalidStoryboard`, first fault in a fixed order, like `CaptureScript::validate`):
version 1; resolution within 320..=4096 and even; `frame_rate` equals the tick rate; unique safe names; at least
one scene; lengths 1..=3000 frames and `UntilCombat` with `min <= max`; total maximum frames ≤ 4,000; distances in
`MIN_CAMERA_DISTANCE..=MAX_CAMERA_DISTANCE`; pitch 15°..=85°; finite numbers; teaser cuts inside their scenes'
minimum lengths; speeds above 1 carry a badge; every `Caption.subsystems` entry names a row of `DESIGN.md` §4 whose
status is `Working` or `Partial` (D-7); every `FollowUnit.unit_types` slug is a catalog identity with admitted
media. Unit slugs appear only in the RON file, never in Rust (check C5).

### D-5. The director

A `Showcase` resource holds the storyboard, the scene index, the frame index within the take and the pending
`UntilCombat` state. One exclusive system in `BattleSet::ExternalControl` (as `runtime_capture::driver::control`):

1. sets `LabState.battle_paused` and `simulation_speed` from the scene;
2. computes the camera pose for this frame and writes `RtsCamera.target`, `distance`, `pitch`;
3. clears keyboard and mouse input (`ButtonInput::reset_all`, as the capture does), so no edge pan or hotkey acts;
4. advances the scene when its length is reached; `UntilCombat` ends on the first frame whose tick contains a
   hostile damage event in the authoritative `BattleSession` world (the same correlation as the capture's
   `correlated_combat`), but not before `min`, and fails loudly at `max` ("the storyboard expects combat by frame
   N of scene S"). Never a silent fallback.

The director never changes the simulation world: it issues no `PlayerCommand`, edits no entity, and reads
`RenderSnapshot` and the session world only (charter §3.2). The scenario's initial attack-move orders drive the
battle. **verify** in WP1: `ClientMode::Skirmish` applies the scenario's `initial_orders` to the viewer's own slot
as it does in `LiveBattle` (`runtime_battle/mod.rs` applies them per scenario; WP1's test proves it headless).

### D-6. Text overlays: in-engine egui, not ffmpeg `drawtext`

- **Chosen:** an egui system in the showcase module paints on `egui::Order::Foreground`, after the HUD. The cards and
  captions are therefore in the frames the engine renders, read back like the rest of the image (charter §1.2: the
  video is engine output end to end; ffmpeg only cuts, scales and encodes).
- **Rejected:** ffmpeg `drawtext`. It needs a font path and fontconfig on each machine, escapes text badly, renders
  differently between ffmpeg versions, and would make the text the only part of the video the engine did not draw.
- Fonts: egui's defaults (Ubuntu-Light for text, Hack for the revision line). Nothing to add to the repository.
- Layout is designed in points for 720 lines; `egui` `pixels_per_point` is `height / 720` so a different resolution
  scales the same layout.
- Look (agent's choice): caption band top-centre, 60 % black rounded panel, title 28 pt, body 18 pt, at most two
  lines; title card and status card on a 70 % black full-frame panel over the live scene; fade in and out over 10
  frames as a function of the frame index; badge in the top-right corner. The HUD stays visible in battle scenes:
  it is the real game.

### D-7. Generated cards that do not rot

All card text except captions is computed from repository data at run time (`showcase::cards`, pure functions with
typed errors):

| Card element | Source | Rule |
|---|---|---|
| Title | literal `"Space Scuffle"` is the project name; the pitch is the first sentence of `DESIGN.md` §1 ("Space Scuffle is a real-time strategy game in the spirit of StarCraft II: deterministic simulation, ...") | Markdown emphasis stripped; error if the section or sentence is missing |
| Revision line | `--revision` and `--revision-date` (the workflow passes `github.sha` and the commit date), plus the adapter name from Bevy's `RenderAdapterInfo` (for example `llvmpipe (LLVM ..)` or `AMD Radeon RX 580 (RADV POLARIS10)`) | Footer of title and status cards: "Real engine output · rendered on <adapter> · <sha7> · <date>" |
| "Where it is" | `DESIGN.md` §4 table | Count of rows per status ("23 subsystems working, 3 partial, 2 not built", numbers as found), then the `Partial` and `Not built` subsystem names |
| "Where it is heading" | `DESIGN.md` §1 line starting `**Intent, not built:**`, and `docs/plans/README.md` rows with status `Active` or `Proposed` whose ID is dated (numbered plans are closed history) | Up to four plans, newest first by ID; each shown as its summary column cut at the first ` (` or `:` and at 70 characters on a word boundary |

Captions are short hand-written sentences in the storyboard, but each names the `DESIGN.md` §4 subsystems it shows;
validation fails if a named row disappears or becomes `Not built` or `Removed`, so a caption cannot keep advertising
a removed feature. The markdown table reader is a small private parser in `showcase::cards` (the workspace-checks
parser lives in a crate `client_diagnostics` must not depend on, check C1).

### D-8. Frame transport and the mezzanine

- Each frame spawns `Screenshot::primary_window()` with a `ShowcaseFrame { index, tick, checksum }` component; the
  observer converts to RGB8, checks the size, hashes it with BLAKE3 (already a dependency), and writes it to the stdin
  of an `ffmpeg` child (`-f rawvideo -pix_fmt rgb24 -s WxH -r 25 -i - -c:v ffv1 -level 3 -g 1 <take>.mkv`).
- Readbacks are asynchronous. A reorder buffer of at most 16 frames writes frames strictly by index; a gap at the
  end, a duplicate or an overflow is `ShowcaseError::FrameOrder` and a non-zero exit. No frame is dropped silently.
- The take manifest (`<take>.manifest.ron`) records per scene its first frame, frame count and start tick, and per
  frame the tick, the simulation checksum and the BLAKE3 hash of the RGB bytes, plus the adapter, the anisotropy
  override and wall-clock render time per frame (reported only, never used for control).
- Why FFV1 and not PNG files: 1,600 PNG frames at 1280×720 take about 2 to 3 GB (**estimate**) and the encoder would
  read them all again; raw RGB is 2.76 MB a frame (4.4 GB). FFV1 is lossless at about 0.4 to 0.8 MB a frame
  (**estimate**, ≤ 1.3 GB per take), so every published encode is cut from an exact copy of the engine's pixels.
- `ffmpeg` is found on `PATH`; if it is missing, the take fails before the client starts (fail loudly, charter
  §1.2).

### D-9. Formats, sizes and the size ladder

| Output | Encoding | Budget | Purpose |
|---|---|---|---|
| `showcase.mp4` | H.264 High, `yuv420p`, 1280×720, 25 fps, `-crf 23 -preset slow -maxrate 2.5M -bufsize 5M`, `+faststart`, no audio | ≤ 25 MiB (48 to 64 s at ≤ 2.5 Mbit/s is ≤ 20 MB) | Full quality, played by the Pages player page |
| `showcase.webp` | animated WebP (`libwebp_anim`), the teaser cut, 800×450, every 2nd frame (12.5 fps), `-loop 0 -compression_level 6`, quality ladder 70 → 60 → 50 → 40, then width 640 with the same ladder | ≤ 4.5 MiB (hard) | Inline in the README |
| `showcase-poster.png` | the storyboard's poster frame (first frame of `clash` + 50), 1280×720 | — | `<video poster>`, social preview |
| `showcase.json` | revision, date, CI run URL, adapter, Mesa release, anisotropy override, frames per scene, render seconds, encoder versions, sizes, SHA-256 of each file | — | Provenance |
| `index.html` | static page from `crates/client_diagnostics/data/showcase_site.html` with placeholders replaced: `<video controls muted loop playsinline poster>` and the provenance as text; no JavaScript | — | The link target from the README |

Why these formats:

- **GitHub does not render `<video>` in a README** unless the file was uploaded through the web UI's attachment
  URLs (`user-attachments`), which CI cannot do. An image renders inline from any URL GitHub's image proxy (camo)
  accepts. So the README needs an animated image, and the MP4 is one click away.
- **Animated WebP over GIF.** GIF is limited to 256 colours per frame, so the smog's gradients and the glass bubbles
  band and dither, and it is typically 3 to 5 times larger than lossy WebP for the same footage. WebP keeps 24-bit
  colour and alpha-free lossy frames; every current browser renders it (Safari since 14).
- **The 4.5 MiB cap:** camo is understood to refuse images above 5 MiB (`CAMO_LENGTH_LIMIT`, 5,242,880 bytes, in
  GitHub's open-source camo; **verify** in WP5 by fetching the rendered README's camo URL). Teaser estimate: 312
  frames × 12 to 16 KB at quality 60 ≈ 3.7 to 5.0 MB (**estimate**); the ladder settles it.
- **The ladder is deterministic and fails loudly.** `assemble` tries each step in order and keeps the first under
  budget; if none fits it exits non-zero with every size it tried, and nothing is published. MP4 above its budget is
  the same error.

### D-10. Where the files are published: GitHub Pages (Q0056, default: enable)

| Option | README can embed? | Plays the MP4? | Cost to the repository | Verdict |
|---|---|---|---|---|
| (a) CI commits the files to `main` | yes (relative path) | no (download) | every run adds 5 to 25 MB of history to `main` forever; bot commits race agents' pushes and the agents' merge flow | rejected |
| (b) CI force-pushes an orphan `showcase` branch, README uses the raw URL | yes | no (raw serves `application/octet-stream` for MP4) | history stays one commit, but every `git clone` and every `git fetch` of all branches downloads the current 25 to 30 MB again after each update (about eight a day on the shared server) | fallback |
| (c) GitHub Pages, deployed from the workflow's artifact | yes (`https://close2.github.io/space-scuffle/showcase.webp`, correct `image/webp` type) | yes, a real `<video>` page with correct `video/mp4` | none to git; Pages limits (1 GB site, 100 GB/month soft bandwidth, 10 builds/hour soft) are far away | **chosen** |
| (d) A rolling release whose assets are replaced each run | uncertain: release downloads redirect with `application/octet-stream` and `Content-Disposition: attachment`, which camo may refuse | download only | a permanent prerelease and tag in the releases list; notifications to watchers | rejected |
| (e) Workflow artifact only | no stable URL; artifacts expire and need a login | no | none | rejected as the publication; kept as a 14-day debug copy |

Pages needs one repository setting: source "GitHub Actions". The repository is public, so publishing it adds no new
exposure of code, but it creates a public website under the owner's account, so WP0 records it as Q0056 with the
default "enable" (the agent's token has admin rights: `gh api -X POST repos/close2/space-scuffle/pages -f
build_type=workflow`). If the owner declines, the fallback is (b) with the same `assemble` output; only the publish
job and the README URLs change.

### D-11. When it runs

- `on: workflow_run: { workflows: [CI], types: [completed], branches: [main] }` with
  `if: github.event.workflow_run.conclusion == 'success'`, plus `workflow_dispatch`. The video only ever shows a
  commit that passed CI. The job checks out `github.event.workflow_run.head_sha` (a `workflow_run` workflow always
  runs the definition on the default branch, which is what we want).
- `concurrency: { group: showcase, cancel-in-progress: false }`: one run at a time, and GitHub keeps only the newest
  pending run, so a burst of pushes renders the latest one and a long render is never cancelled into starvation (CI
  takes about 50 minutes and pushes come about hourly; cancelling would risk never finishing).
- **Skip when nothing the video shows changed.** The job reads `showcase.json` from the live site (`curl`); if
  `git diff --quiet <published revision> HEAD -- assets crates Cargo.lock DESIGN.md docs/plans/README.md
  .github/workflows/showcase.yml` succeeds (after `git fetch --depth=1 origin <published revision>`), it ends green
  without rendering. Docs-only commits therefore cost one minute. `workflow_dispatch` has an input `force` that skips
  this test.
- Cost: GitHub-hosted minutes are free for public repositories; the limit that matters is wall time and runner
  availability (D-13). Owner decision with a default: cadence "after every green CI run" (alternative: nightly).

### D-12. The CI job

Two jobs, so that the job that runs repository code never holds a write token:

```yaml
name: Showcase video
on:
  workflow_run: { workflows: [CI], types: [completed], branches: [main] }
  workflow_dispatch:
    inputs: { force: { type: boolean, default: false } }
permissions: { contents: read }
concurrency: { group: showcase, cancel-in-progress: false }
env:
  SHOWCASE_SITE: https://close2.github.io/space-scuffle
  SPACE_SCUFFLE_VIRTUAL_GPU: "1"
jobs:
  render:
    if: github.event_name == 'workflow_dispatch' || github.event.workflow_run.conclusion == 'success'
    runs-on: ubuntu-latest
    timeout-minutes: 90
    permissions: { contents: read }
    steps:
      - checkout (ref: workflow_run.head_sha or github.sha; fetch-depth: 1)
      - skip test (D-11) -> sets an output; later steps run only if it says render
      - free preinstalled runner software (the same list as ci.yml)
      - apt: mesa-vulkan-drivers vulkan-tools xvfb ffmpeg + the client's build libraries (as ci.yml)
      - toolchain dtolnay/rust-toolchain@1.99.0
      - Swatinem/rust-cache@v2 with key "showcase-release" (D-13 on cache size)
      - disk guard: 10 GiB free required (release build ~4 GiB, mezzanine <= 1.3 GiB, outputs, margin) -- estimate, WP4 measures
      - cargo build --release --locked -p client_diagnostics --bin showcase
      - target/release/showcase render --driver lavapipe --out "$RUNNER_TEMP/showcase" --revision "$SHA" --revision-date "$DATE" --run-url "$RUN_URL"
      - job summary: scene table, frames, render seconds, p50/p95 frame wall time, sizes (from showcase.json)
      - actions/upload-pages-artifact@v3 (path: $RUNNER_TEMP/showcase/site)
      - actions/upload-artifact@v4 (the site plus manifests, retention 14 days)
  deploy:
    needs: render
    if: needs.render.outputs.rendered == 'true'
    runs-on: ubuntu-latest
    permissions: { pages: write, id-token: write }
    environment: { name: github-pages, url: ${{ steps.deployment.outputs.page_url }} }
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

- `render --driver lavapipe` selects the first existing manifest of `lvp_icd.json`, `lvp_icd.x86_64.json`, sets
  `WGPU_BACKEND=vulkan` and `VK_ICD_FILENAMES`, and sets `SPACE_SCUFFLE_TERRAIN_MAX_ANISOTROPY=1` when
  `vulkaninfo --summary` reports Mesa below 25.1 (Q0009). To avoid a second copy of that rule, WP2 moves
  `mesa_release`, `anisotropy_override` and `FIRST_BOUNDED_ANISOTROPY_MESA` from
  `crates/client/tests/support/gpu_run.rs` into `client::render_compatibility` (public, documented, with their
  existing unit tests), and `gpu_run.rs` calls them there. The video footer names the adapter, and `showcase.json`
  records the override, so a viewer can tell the terrain was sampled without anisotropy.
- `xvfb-run -a -s "-screen 0 1920x1080x24"`, so the 1280×720 window fits.
- No secret is used. The deploy job runs no repository code: it only deploys the uploaded artifact.

### D-13. Time budget and the calibration rule

There is no measured release-build `lavapipe` frame time for this scene. The nearest figure is the debug build at
400 units: about 2 s per 1280×720 frame on 16 shared cores (§2.2). The showcase scene has about 30 bubbles, two
domes, the terrain and the smog volume; a release build removes most CPU cost, and the runner has 4 vCPUs for
llvmpipe's rasteriser.

**Planning estimate:** 0.6 to 2.0 s per frame at 1280×720 on the runner; planning figure 1.2 s.

| Step | Cold cache | Warm cache |
|---|---|---|
| Setup, freeing disk, apt | 4 min | 4 min |
| Release build of `showcase` | 14 min (**estimate**) | 4 min |
| Client start, shader compilation on lavapipe, readiness | 2 min | 2 min |
| Render 1,200 to 1,600 frames at 1.2 s | 24 to 32 min | 24 to 32 min |
| Assemble (x264 slow, WebP ladder up to 8 attempts on ≤ 312 frames) | 3 min | 3 min |
| Upload and deploy | 1 min | 1 min |
| **Total** | **48 to 56 min** | **38 to 46 min** |

**Budget:** render step ≤ 40 min, job ≤ 60 min, `timeout-minutes: 90`.

**Calibration rule** (WP2 measures locally, WP4 on CI; the result goes into the storyboard, not code):

1. Measure *c*, the median wall time per frame of the `battle` take on the runner, from the manifest.
2. If 1,600 × *c* + 120 s ≤ 40 min (*c* ≤ 1.4 s): keep 1280×720.
3. Otherwise set the storyboard resolution to 960×540 (0.56 of the pixels) and measure again.
4. If still above, shorten `clash`, `smog` and `units` to fit, record it in WP4's evidence, and leave the length
   rule visible in the storyboard comments.

Parallel takes do not help: llvmpipe already uses every core for one frame.

Cache: the repository's Actions cache is limited to 10 GB and `ci.yml`'s jobs already use it. The showcase saves its
release cache only on `workflow_dispatch` and on the first run of a new `Cargo.lock` (`save-if`), so it does not evict
the test caches on every run. If WP4 sees eviction of `check-and-test`'s cache, set `cache-targets: false` for the
showcase (registry only).

### D-14. `README.md`

New file at the repository root, short and without facts that rot:

1. `# Space Scuffle` and a two-sentence pitch that points to `DESIGN.md` §1.
2. The teaser, linked to the player page:
   `<a href="https://close2.github.io/space-scuffle/"><img src="https://close2.github.io/space-scuffle/showcase.webp" width="800" alt="Space Scuffle showcase: a real battle rendered by CI"></a>`
   and one line: "Rendered by CI from the latest green `main` with the real engine on software Vulkan
   ([workflow](.github/workflows/showcase.yml)); the closing card is generated from `DESIGN.md` and the plan index."
3. "Where it is": one sentence and a link to `DESIGN.md` §4. "Where it is heading": link to `docs/plans/README.md`
   and `docs/questions/`. No status is copied into the README.
4. Build and run: `cargo run --release -p client -- --skirmish`, the system packages (as in `ci.yml`), and the
   charter link for contributors (`AGENTS.md`).
5. Licence: "MIT OR Apache-2.0 (workspace `Cargo.toml`)".

### D-15. Checks and documents

- **New check C24** (`crates/workspace_checks`, charter §9 table): `README.md` exists; its relative links resolve
  (the C17 resolver); its showcase image and link use the site named by `SHOWCASE_SITE` in
  `.github/workflows/showcase.yml`, and its image file name is the one `assemble` writes (`showcase.webp`, a
  `pub const` in `client_diagnostics::showcase`, read by the check as text from the source to avoid a dependency).
- `DESIGN.md` §4: a new row "Showcase video | Working | `showcase` tool, storyboard, CI workflow, Pages | evidence
  `crates/client_diagnostics/tests/showcase_tests.rs` | lavapipe output; no terrain fog shading in the game (§9)".
  §6 gains `README.md`. C17 keeps the links true.
- `AGENTS.md` §4 table, `client_diagnostics` row: add "showcase storyboard, cards and a reproducible take" with
  `showcase_tests.rs`, so C15 enforces the test's existence. §9 table: add C24.
- `crates/gate/suites.ron`: classify `client_diagnostics/showcase_tests` as `Gpu` with triggers (renderer, smog,
  terrain, camera, `battle_hud/**`, `assets/captures/showcase.ron`, `assets/scenarios/showcase.ron`,
  `crates/client/tests/support/gpu_run.rs`); `showcase_storyboard_tests` and `showcase_encode_tests` stay `fast`
  (unlisted). Add `"README.md"` to the `charter documents` rule (C24 reads it); the `docs` glob `**/*.md` already
  keeps it out of other suites.

### D-16. Local reproduction on the RX 580 (charter §7)

```bash
# In the agent's worktree:
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree-name>
cargo build --release --locked -p client_diagnostics --bin showcase
OUT=/home/spacescuffle/scratch/showcase/$(date +%F)-$(git rev-parse --short HEAD)
"$CARGO_TARGET_DIR/release/showcase" render --driver radv --out "$OUT" \
    --revision "$(git rev-parse HEAD)" --revision-date "$(git show -s --format=%cs HEAD)"
```

`render --driver radv` runs each take as
`with-gpu-lock xvfb-run -a -s "-screen 0 1920x1080x24" env WGPU_BACKEND=vulkan
VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json MESA_VK_WSI_DEBUG=sw showcase take ...`, one lock per take
(a take is about 1 to 3 minutes on the RX 580, **estimate**), then `assemble` without the lock. It refuses to start
if `with-gpu-lock` is not on `PATH` (no unlocked use of the physical GPU). To reproduce CI exactly:
`showcase render --driver lavapipe ...` (no lock; add `taskset -c 0-3` to match the runner's four vCPUs). `ffmpeg`
is Debian's package (`/usr/bin/ffmpeg`); nothing is downloaded.

---

## 4. Storyboard v1

Take `battle`, `ClientMode::Skirmish`, viewer slot 0, scenario `assets/scenarios/showcase.ron` (a copy of
`default_battle.ron` with its own file, so the video does not move when tests change the default battle: map
`showcase_canyon`, both slots `battle_automaton`, mutual attack-move, the same seed). 25 fps.

| # | Scene | Length (frames) | Speed | Camera | Overlay | What it shows |
|---|---|---|---|---|---|---|
| 1 | `title` | 125 (5 s) | Paused at tick 0 | `Orbit` around the map centre, distance 46, pitch 62°, 20° sweep | `TitleCard`: "Space Scuffle", the `DESIGN.md` §1 pitch, revision footer | The whole canyon, both bases, the domes playing their clips |
| 2 | `terrain` | 150 (6 s) | 2×, badge | `Path`: own base → the ramp → along the cliff edge, distance 30 → 22, pitch 58° → 48° | Caption "High ground and ramps" / "Units below a cliff cannot see up it; ramps are the way up." (subsystems: Terrain, cliffs and ramps; Simulation vision and watchtowers) | Cliff levels, ramps, the army setting out |
| 3 | `advance` | `UntilCombat(min: 100, max: 500)` | 4×, badge "Fast-forward ×4" | `FollowArmy(player 0)`, distance 26, pitch 55° | Caption "Fog of war" / "You see only what your units see: the enemy appears when it comes into sight." (Simulation vision and watchtowers; Movement and pathfinding) | Group movement, path through the ramp, enemies popping into sight |
| 4 | `clash` | 300 (12 s) | 1× | `FollowCombat`, distance 16, pitch 52° | Caption "Combat" / "Weapons wind up, fire and recover; shields take damage before the hull; splash hits groups." (Combat) | Weapon effects, shields, splash, deaths |
| 5 | `smog` | 175 (7 s) | 1× | `FollowCombat`, distance 9, pitch 30° (low) | Caption "Smog" / "A volumetric smog field, stirred and carved by every bubble." (Smog fog presentation) | The volumetric smog in the fight |
| 6 | `units` | 150 (6 s) | 1× | `FollowUnit(player 0, ["rhino_beetle", "pulse_bot", "welder_drone"])`, distance 4.5, pitch 35° | Caption "Units" / "Each unit is a glass bubble playing a video clip of its creature, generated with Veo." (Bubble and dome rendering; Media coverage) | A flipbook close up, walking or attacking |
| 7 | `closing` | 200 (8 s) | 2×, badge | `Orbit` around the combat centroid, distance 40, pitch 60° | `StatusCard`: "Where it is" and "Where it is heading" (D-7), revision footer | The project's state from `DESIGN.md` and the plan index |

Length: 1,200 to 1,600 frames, 48 to 64 s. Teaser cuts (`frame_step: 2`): `title` 0+75, `advance` last 75 (cut
from the end of a variable scene is expressed as `from_end: true`), `clash` 0+150, `smog` 0+100, `units` 0+75,
`closing` 0+150 → 625 frames → 312 WebP frames, 25 s. Poster frame: `clash` + 50.

The concrete numbers (distances, pitches, path keys) are first drafts; WP3 adjusts them after watching the first
full local render. That tuning changes only the RON file.

---

## 5. File ownership

| File | WP | Change |
|---|---|---|
| `docs/questions/open/0053-github-pages-for-the-showcase.md` | WP0 | new |
| `docs/plans/2026-11-05-ci-showcase-video.md`, `docs/plans/README.md` | WP0, WP6 | new plan and row |
| `crates/client_diagnostics/src/showcase/{mod.rs,storyboard.rs,cards.rs}` | WP1 | new |
| `assets/captures/showcase.ron`, `assets/scenarios/showcase.ron` | WP1 (WP3 tunes) | new |
| `crates/client_diagnostics/tests/showcase_storyboard_tests.rs` | WP1 | new (fast) |
| `crates/client_diagnostics/src/showcase/{director.rs,overlay.rs,recorder.rs,take.rs}` | WP2 | new |
| `crates/client_diagnostics/src/runtime_capture/{mod.rs,pipelines.rs,driver.rs}` | WP2 | visibility `pub(crate)` of the reused pieces only |
| `crates/client/src/render_compatibility.rs`, `crates/client/tests/support/gpu_run.rs` | WP2 | move `mesa_release`, `anisotropy_override`, the Mesa constant |
| `crates/client_diagnostics/src/bin/showcase.rs`, `crates/client_diagnostics/Cargo.toml` | WP2 | new binary |
| `crates/client_diagnostics/tests/showcase_tests.rs` | WP2 | new (GPU) |
| `crates/gate/suites.ron` | WP2, WP5 | classify the GPU test; `README.md` in the charter-documents rule |
| `crates/client_diagnostics/src/showcase/{encode.rs,site.rs,render.rs}`, `crates/client_diagnostics/data/showcase_site.html` | WP3 | new |
| `crates/client_diagnostics/tests/showcase_encode_tests.rs` | WP3 | new (fast, needs `ffmpeg`) |
| `docs/evidence/2026-11-xx-showcase-video.md` | WP2 to WP6 | new: calibration, timings, sizes, review notes |
| `.github/workflows/showcase.yml` | WP4, WP5 | new |
| `README.md` | WP5 | new |
| `crates/workspace_checks/src/docs.rs` (or a new `readme.rs`), `crates/workspace_checks/tests/charter.rs` | WP5 | C24 |
| `AGENTS.md` §4 and §9, `DESIGN.md` §4 and §6 | WP5 | rows |

---

## 6. Work packages

Every package: tests first (they fail against the stub), then the code; one `cargo gate package` per package after
merging `main`, its stamp in the hand-back (charter §4.3, owner rule on gate tiers). Agents build with
`CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/<worktree>` and keep captures and logs on `~/scratch`.

### WP0: Q0056 and the plan's row (docs)

- Write `docs/questions/open/0053-github-pages-for-the-showcase.md`: "Enable GitHub Pages (source: GitHub Actions)
  for close2/space-scuffle to host the CI showcase video at `https://close2.github.io/space-scuffle/`?" Blocking: no.
  Default until answered: yes, the agent enables it in WP5; options (a) Pages (default), (b) orphan `showcase` branch
  with raw URLs (cost: every clone and fetch downloads the current files again), (c) no publication, artifact only.
  Also state the cadence default (after every green CI run on `main`, skipped when nothing shown changed) as a
  second, smaller point the owner may override.
- Add this plan and its row (`Proposed` → `Active` when WP1 starts).
- Gate: docs-only `cargo gate package` (fmt and `workspace_checks`).

### WP1: Storyboard, cards and scenario (no GPU)

Tests first (`showcase_storyboard_tests.rs` and unit tests in `storyboard.rs`, `cards.rs`):

1. `the_shipped_storyboard_loads_and_validates`: `assets/captures/showcase.ron` passes `Storyboard::validate` against
   the real `DESIGN.md` and the real catalog.
2. `validation_reports_the_first_fault_in_order`: one mutation per rule of D-4 (bad version, odd resolution, frame
   rate ≠ 25, duplicate scene, unsafe name, length 0, `UntilCombat` min > max, total > 4,000, distance out of range,
   pitch out of range, NaN, speed > 1 without badge, teaser cut past a scene's minimum, unknown subsystem, `Removed`
   subsystem, unknown unit slug, slug without media) gives the expected `InvalidStoryboard` reason.
3. `status_card_counts_design_rows_and_lists_heading_plans`: from small fixture texts (status counts,
   Partial/Not-built names, `Intent, not built` line, Active/Proposed dated plans sorted newest first, numbered plans
   ignored, cut at ` (` or `:` and 70 characters on a word boundary).
4. `cards_fail_with_typed_errors_on_missing_sections`: missing §1, missing §4 table, malformed row →
   `ShowcaseError::Cards { source_file, reason }`.
5. `status_card_from_the_real_repository_is_non_empty`: real `DESIGN.md` and `docs/plans/README.md` give at least
   one status count and the pitch sentence.
6. `the_showcase_scenario_fields_only_media_backed_identities`: load `assets/scenarios/showcase.ron` through
   `client::runtime_battle::BattleDefinition`, instantiate, and check every spawned archetype has an atlas and
   flipbook metadata under `assets/textures/`.
7. `skirmish_applies_the_scenario_initial_orders_to_the_viewer` (**verify** item of D-5): headless, run the session
   N ticks under the Player profile and check player 0's units moved toward slot 1's start. If it fails, stop and
   report to the orchestrator: making the viewer's army move would need a production change or `PlayerCommand`s
   from the director, which this plan does not authorise.
8. `follow_spring_is_deterministic_and_converges`: the camera spring from a fixed start reaches 1 % of the target
   within its settling frames and gives bit-identical results for identical input.

Code: `showcase::storyboard` (serde types, `load`, `validate`), `showcase::cards` (pitch, status card, heading
list, markdown table reader), `ShowcaseError` (`thiserror`), the scenario file and the storyboard draft.
Gate: `cargo gate package` (the `captures` and `maps and scenarios` rules select their suites).

### WP2: The take (GPU)

Tests first:

1. `showcase_smoke_take_is_byte_reproducible` (`showcase_tests.rs`, through `gpu_run.rs`, `Gpu` class): a smoke
   storyboard written to a temp dir (480×270, four scenes of 4 frames: `TitleCard` paused, `Path` at 2×, `FollowArmy`
   at 4×, `StatusCard`) is rendered twice by the debug `showcase take` as two harness children, with
   `allow_for_hex_tiling_driver_defect()`. Both manifests have 16 frames, frames 0 to 15 in order, identical ticks,
   checksums and BLAKE3 hashes; the mezzanine decodes (`ffprobe`) to 16 frames of 480×270; no frame is uniform
   (the image is not blank); the manifest names the harness's adapter. Each child stays within the `lavapipe` limit
   of 300 s (debug, 32 frames at 480×270 plus start-up, **estimate** 90 to 150 s).
2. `frame_order_buffer_rejects_gaps_duplicates_and_overflow` (unit, no GPU).
3. `overlay_fade_is_a_function_of_the_frame_index` (unit): alpha at frames 0, 5, 10, last-10, last.
4. `director_pauses_and_sets_speed_per_scene` and `until_combat_fails_loudly_at_max` (unit, on a headless
   `BattleSession` without a renderer).
5. Moved helpers: `mesa_release` and `anisotropy_override` tests move with them to `client::render_compatibility`;
   `gpu_run.rs` uses them; the harness's own tests still pass.

Code: `director.rs`, `overlay.rs`, `recorder.rs`, `take.rs`, `bin/showcase.rs` (`take` subcommand), the
`pub(crate)` changes in `runtime_capture`, the helper move. `suites.ron`: `client_diagnostics/showcase_tests` as
`Gpu` with the triggers of D-15.

Measurement (results into `docs/evidence/2026-11-xx-showcase-video.md`): a release `take` of the full `battle` take
(a) on `radv` with `with-gpu-lock`, (b) on `lavapipe` under `taskset -c 0-3`, 100 frames, with the machine's load
average recorded. These calibrate D-13 before CI exists. Gate: `cargo gate package` (GPU tests run through the
harness; never wrap the gate in `with-gpu-lock`).

### WP3: Assemble, player page and `render`; first full video

Tests first (`showcase_encode_tests.rs`, needs `ffmpeg` and fails without it, like `asset_baker`'s ffmpeg tests):

1. `assemble_writes_every_file_within_budget`: a temp mezzanine of 60 frames at 320×180 made with ffmpeg's
   `testsrc2` (a temporary test fixture, never under `assets/`; charter §1.2) and a matching manifest; the outputs
   exist, `ffprobe` gives H.264 `yuv420p` at 25 fps with the frame count of the cut, the WebP is animated with the
   expected frame count and loops, `showcase.json` hashes match the files.
2. `size_ladder_steps_down_then_fails_loudly`: with a tiny budget the ladder reports each attempt in order and
   returns `ShowcaseError::OverBudget` with every size; with a generous budget it keeps the first step.
3. `site_template_replaces_every_placeholder`: no `{{` left; the HTML names the four files; text is escaped.
4. `render_refuses_radv_without_with_gpu_lock` and `render_selects_the_lavapipe_manifest_and_override` (unit, with
   an injected environment).

Code: `encode.rs` (argument builders, ladder, cut lists from the manifest), `site.rs`, the template,
`render.rs` (`render` and `assemble` subcommands).

Then the first full local render on the RX 580 (D-16). The agent watches it: at least one still per scene extracted
with ffmpeg and viewed, plus the WebP; tunes the storyboard numbers (camera, lengths); records timings, sizes and
review notes in the evidence file. Gate: `cargo gate package`.

### WP4: CI render job (artifact only)

- Add `.github/workflows/showcase.yml` with the `render` job of D-12, uploading only the 14-day artifact (no deploy
  yet). Merge to `main`; then `gh workflow run showcase.yml -f force=true` (dispatch works only once the file is on the
  default branch).
- Read the run: Mesa release, anisotropy override, cold build time, start-up time, *c*, total time, disk before and
  after, sizes. Apply the calibration rule of D-13; any storyboard change is a commit in this WP. Re-run once to see
  the warm-cache time and confirm reproducibility: the per-frame hashes of two CI runs of the same commit are
  compared (expected identical on the same runner image; a difference is recorded, not hidden, and investigated
  before WP5).
- Gate: `cargo gate package` (the workflow path is `global`, so the full suite runs).

### WP5: Publishing, README and checks

1. Per Q0056's default (or the owner's answer): `gh api -X POST repos/close2/space-scuffle/pages -f
   build_type=workflow`; add the `deploy` job and the skip test of D-11; dispatch once; confirm
   `https://close2.github.io/space-scuffle/showcase.webp` serves `image/webp` and `index.html` plays the MP4.
2. Test first for C24 (`charter.rs` and unit tests on fixture text: missing README, broken relative link, wrong
   site, wrong file name), then the check.
3. Write `README.md` (D-14). Verify the README renders the image: render it with `gh api markdown -f mode=gfm
   -f context=close2/space-scuffle -F text=@README.md`, take the camo URL of the image and `curl -sI` it: HTTP 200,
   `content-type: image/webp`, length equal to the file. If camo refuses (size or type), lower the WebP budget in the
   storyboard or switch the teaser width per D-9 and record why.
4. `DESIGN.md` §4 row and §6 entry; `AGENTS.md` §4 row and §9 C24 row; `suites.ron` charter-documents rule.
5. Gate: `cargo gate package`.

### WP6: Bookkeeping

Evidence file complete (calibration, CI timings, sizes, camo verification, links to the runs); this plan's row
`Done` with the WP commits; Q0056's status (answered or default carried out, left open for the owner to close).
Docs-only gate.

---

## 7. Testing summary

| Test | File | Class | GPU? | Runs on CI? |
|---|---|---|---|---|
| Storyboard validation, cards, scenario media, Skirmish orders, spring | `crates/client_diagnostics/tests/showcase_storyboard_tests.rs`, unit tests in `showcase/*.rs` | fast | no | yes, `cargo test --workspace` |
| Byte-reproducible smoke take | `crates/client_diagnostics/tests/showcase_tests.rs` | Gpu | through `gpu_run.rs` (lock on `radv`, `lavapipe` on CI) | yes |
| Encoding, ladder, site, render environment | `crates/client_diagnostics/tests/showcase_encode_tests.rs` | fast | no (`ffmpeg`, already installed on CI) | yes |
| C24 README | `crates/workspace_checks/tests/charter.rs` | fast | no | yes |
| The real video | `.github/workflows/showcase.yml` | — | `lavapipe` | after green CI |

- **C21:** no test builds a render device in its own process; the only GPU test starts `showcase take` as a child
  through `GpuCommand`.
- **C22:** `showcase_tests` is classified `Gpu` with triggers; the new `assets/captures/showcase.ron` and
  `assets/scenarios/showcase.ron` are covered by the existing `captures` and `maps and scenarios` rules; `README.md`
  by `docs` and the charter-documents rule; the workflow by `global`.
- **C4, C5, C6, C8, C10, C11:** every new file starts with `//!`; no catalog slug in Rust (they are in the RON);
  nothing under `assets/` is embedded; functions under the clippy threshold; `expect` only with `invariant:`; the
  reorder buffer and the raw-frame pipe carry a `PERFORMANCE RATIONALE` block if they deviate from the plain form.

---

## 8. Risks

| Risk | Likelihood | Effect | Mitigation |
|---|---|---|---|
| `lavapipe` frames are slower than 1.4 s on the runner | medium | job over budget | D-13 rule: 960×540, then shorter scenes; `timeout-minutes: 90`; measured in WP2 and WP4 before publishing |
| Old Mesa on the runner cannot finish hex-tiled terrain frames | known for Mesa < 25.1 | hang | the Q0009 override, decided by the moved helper; footer and `showcase.json` say so |
| A dropped or reordered readback | low | a corrupt video | the reorder buffer fails the take; nothing is published; the next green CI run tries again |
| Simulation changes move the fight (other WPs change combat and movement all the time) | high | camera misses the action | event-anchored scenes (`UntilCombat`, `FollowCombat`, `FollowArmy`); bounded waits fail loudly with the scene named |
| A unit in `FollowUnit` dies before scene 6 | medium | wrong subject | the list of types falls back in order; all dead → loud error; the order is data |
| The README image is refused by camo (size or type) | low to medium | broken image in the README | 4.5 MiB cap, WebP type from Pages, WP5 verifies with the real camo URL |
| Release cache evicts CI's test caches (10 GB repo limit) | medium | slower CI | `save-if` on dispatch and new lockfile only; registry-only fallback |
| Disk on the small AWS runner | low (the job frees 15 to 30 GiB first) | job fails at the guard | own 10 GiB guard with the runner class in the message (as `ci.yml`) |
| Showcase failures are mistaken for CI failures | low | noise | separate workflow named "Showcase video"; it never gates CI or merges |
| Pages declined by the owner | low | no Pages URL | fallback (b), same `assemble` output |
| Video shows lavapipe image quality (no anisotropy), legacy atlases without provenance (`psi_grey`, `rhino_beetle`) | certain | slightly blurrier terrain at grazing angles | stated in the footer and `showcase.json`; local radv renders show the reference look |
| Captions overstate a feature | low | misleading intro | captions name `DESIGN.md` rows; validation rejects rows that are removed or not built; the fog caption describes only hiding enemies |

---

## 9. Out of scope and follow-ups (charter §8, Q0002)

- **Terrain fog shading in the battle.** StarCraft II darkens terrain outside current vision and shrouds unexplored
  terrain; Space Scuffle's battle only hides enemies (§2.3). This plan does not fake it in the video. A follow-up
  plan should add it (the terrain shader's LOS modulation exists for the editor) and, until then, `DESIGN.md`'s bug
  register could list it; the orchestrator decides.
- **A Visual Lab take** (close-up heroes, swarm stress test): needs a production option to hide the developer
  inspector panel; then a second take in the storyboard needs no new showcase code.
- Audio or music; cross-fades between scenes; picture-in-picture.
- Showing every catalog identity: only 12 of 91 have media; the video grows with the Veo expansion by editing the
  scenario and the storyboard.
- Re-rendering on a GPU runner; uploading to a video host; a README `<video>` (impossible from CI, D-9).
- Rendering in the main CI job (would lengthen it and share its disk budget).

---

## 10. Decisions for the owner (with defaults)

| Decision | Default until answered | Where |
|---|---|---|
| Enable GitHub Pages (source: GitHub Actions) and publish the showcase at `https://close2.github.io/space-scuffle/` | Yes: the agent enables it in WP5 (repository public; reversible in Settings) | Q0056, D-10 |
| Cadence: after every green CI run on `main`, skipping runs where nothing shown changed | That; alternative nightly | Q0056 (second point), D-11 |
| Add a root `README.md` (none exists) | Yes, as D-14; content is the agent's | D-14 |

Visual choices (storyboard, overlay look, camera) are the agent's under the standing rule; no question is needed.

## Amendment 1 (orchestrator, 2026-11-05)

- The owner answered the Pages decision in chat ("Enable GitHub Pages (Recommended)"); it is recorded as closed
  [Q0056](../questions/closed/0056-github-pages-showcase.md) (renumbered from the plan's draft Q0053, which another
  question took). WP0 therefore only adds the plan row; WP5 enables Pages without waiting.
- The README is created by WP5, as the plan says; the owner asked for the video to be referenced from it.
