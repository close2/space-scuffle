# Blueprint: Fast GPU Test Children on the RX 580 (Q0083)

The GPU lane decides how long a package gate takes. In the WP12 package gate it took about 86 of the 93 test
minutes (setup evaluation 2026-11-16, item 7). The same GPU tests run faster on CI's 4-vCPU runner with software
Vulkan than here on the RX 580: `effect_slice_gpu_tests` 1,997 s on radv against 937 s on lavapipe, and
`editor_app_tests` 618 s against 181 s. On radv each child keeps about one CPU core busy the whole time.

The owner answered Q0083 (`docs/questions/open/0083-gate-gpu-driver.md`):

- **First, option 1:** make radv fast by removing the frame readback / WSI bottleneck. Render offscreen, and copy a
  frame into a `HOST_VISIBLE | HOST_CACHED` staging buffer only on the ticks where a test inspects pixels.
- **Option 2 is pre-authorized:** if lavapipe stays faster, or parallelises better, for functional gate tests, those
  tests run on lavapipe. radv is kept for performance (frame budget) and for review captures.

This plan:

- measures first;
- moves every child that only reads its own pictures offscreen;
- moves every child that needs a window from Xvfb with software present to a headless KWin on the same GPU;
- proves the pictures unchanged on both drivers;
- then applies a measured rule for option 2.

**Status of this document:** a stage 1 blueprint under charter §6, written on 2026-11-16 against `main` at
`937c3f4b`. A full plan is required because the work spans packages (`client`, `client_diagnostics`, `gate`),
changes the GPU test harness that every GPU test uses, and needs a charter §7 text change. Line numbers drift, so
search for the quoted symbol. Claims not confirmed while planning are marked **verify**.

**Questions.**

| Question | State | Where in this plan |
|---|---|---|
| Q0083: gate GPU driver | Answered: option 1 first, option 2 pre-authorized | Whole plan; D-9, §8 |
| Q0084: charter compaction | Answered: option 2 (exact diff shown to the owner first) | §9 (same procedure for the §7 text) |
| Q0019: frame-budget display path | Closed: headless KWin is the path of record | D-4 (the same path for windowed children) |
| Q0012: GPU lock wait 290 s | Closed | Unchanged |
| Q0061: implementation agents hand back after `quick`; a verifier runs `package` | Standing rule | §6 |
| Q0002: scoping is the agent's call if written down | Standing rule | §12 |
| New: Q0086, the §7 text (Q0085 is the charter-compaction diff) | Filed at the end of WP1 | §9 |

---

## 1. Summary

| | Children that read only their own pictures | Children that need a window | Gate driver |
|---|---|---|---|
| Today | Hidden window under `xvfb-run`; radv with `MESA_VK_WSI_DEBUG=sw` copies every presented frame through the CPU into Xvfb | Same | radv with the lock (lavapipe on CI) |
| Measured cost | About 105 ms per presented 1280×720 frame on radv (§2.2) | Same, about 160 ms at 1600×900 (estimate, scaled by pixels) | — |
| Chosen path | No window, no winit, no display. The camera renders to an `Image` and is captured with `Screenshot::image` only on capture ticks (D-1, D-2) | Headless KWin (`kwin_wayland --virtual --xwayland`): radv's normal DRI3 present, composited on the same GPU inside the child's lock (D-4) | radv, unless §8's rule picks lavapipe for functional targets (D-9) |
| Picture change | None: bit-identical to today's capture on radv and lavapipe (D-6; tolerance only with a recorded cause) | None: bit-identical (D-6) | Per-driver goldens unchanged |
| Expected effect (estimate, WP1 confirms) | `effect_slice` child about 80 s → about 30 s | `editor_app` verify-render and the skirmish look children lose their present cost | §8 |

---

## 2. What is known

### 2.1 The lane (setup evaluation 2026-11-16, item 7; `wp12-package.log`)

- GPU lane, serial: about 5,150 s.
  - `effect_slice_gpu_tests`: 1,997 s wall and 2,192 s CPU, from 7 subjects × 3 children plus 4 more, about 80 s per
    child.
  - `weapon_battle_gpu`: 623 s. `editor_app`: 618 s. `area_link`: 318 s. `stance`: 283 s. `smog_bubbles`: 272 s.
- General lanes: 4,770 s over two lanes, so about 40 min each. Valgrind runs alone for 381 s.
- On radv, CPU time ≈ wall time: one core is busy. On lavapipe, CPU/wall ≈ 4.6, because the rasteriser runs on
  many threads.
- No gate sets `SPACE_SCUFFLE_GPU_RUN_LEDGER`, so no per-child phases exist for these runs.
- `/run/lock/gpu-lock.log` shows no peer use, so lock contention is ruled out.
- In 2026-11-03, short children were 2.3 times *faster* on radv (harness plan §3.5: 277 s against 635 s). Children
  that render few frames are fast on radv, and the frame-heavy lab and look children are slow. That pattern points to
  a cost per presented frame.

### 2.2 The present path, measured (plan `2026-11-03-close-phase-6-7-8-exits`, §5.3)

`vkcube` on the RX 580 (radv, Mesa 25.0.7):

| Display path | Size | Per frame |
|---|---|---|
| Xvfb, `MESA_VK_WSI_DEBUG=sw` | 1280×720 | about 105 ms |
| Xvfb, `MESA_VK_WSI_DEBUG=sw` | 640×360 | about 20 ms |
| Xvfb, no `sw` | — | fails: no DRI3 |
| Headless KWin, through its Xwayland | 1280×720 | 0.33 ms (immediate); 16.8 ms with FIFO (60 Hz virtual output) |

- The software path's cost grows with pixel count, not with what is drawn. This fits the owner's analysis: the CPU
  reads every image out of write-combined memory across PCIe.
- Which memory type Mesa 25.0.7's software WSI uses on radv (VRAM BAR or write-combined GTT) is **verify**. It does
  not matter for this plan, because both chosen paths avoid it.
- `frame_budget` already declares its display path (`frame_budget/display.rs`) and measures through headless KWin
  (Q0019).

### 2.3 How frames are read back today

- Every pixel test captures with Bevy's `Screenshot`: `Screenshot::primary_window()` in the lab driver
  (`effects/lab/driver.rs`), in the look tests, `frame_limit_system`, `runtime_capture/driver.rs` and the editor
  verification.
- In Bevy 0.15.3 (`bevy_render/src/view/window/screenshot.rs`), a screenshot redirects that frame's output into a
  texture it creates. It then calls `copy_texture_to_buffer` into a `MAP_READ | COPY_DST` buffer and blits the
  texture back to the target. `map_async` delivers the bytes. Window and `Image` targets use the same mechanism
  (`NormalizedRenderTarget::Image` branch).
- wgpu-hal 23.0.1 (`vulkan/device.rs`, the `MAP_READ` branch) asks gpu-alloc 0.6.2 for `UsageFlags::DOWNLOAD`.
  gpu-alloc's `reverse_priority` ranks `HOST_VISIBLE`, then `HOST_CACHED`, then `HOST_COHERENT`, and prefers memory
  that is not `DEVICE_LOCAL`. On a radv device that lists a cached GTT type, that type is chosen (**verify** with
  WP1's `vulkaninfo` list).
- So the owner's readback design (a DMA copy into cached host memory, only on inspected ticks) is what the capture
  already does. The per-frame cost is the presentation of every frame, captured or not.
- Versions (Cargo.lock): Bevy 0.15.3, wgpu 23.0.1, ash 0.38, gpu-alloc 0.6.2, winit 0.30.13, bevy_egui 0.33.0.
  Host: Mesa 25.0.7, Xvfb 21.1.16, `kwin_wayland` and `Xwayland` installed, `perf` 6.12,
  `perf_event_paranoid` = 1.

### 2.4 Which children need a window

| Target | Child builds | Needs a window? | Path after this plan |
|---|---|---|---|
| `client/effect_slice_gpu_tests` | `client::effects::lab::lab_app` (hidden window, winit on any thread) | No: 3D scene only | Offscreen |
| `client/effect_gpu_tests` | Own app, `WindowPlugin { primary_window: Some(..) }` | No | Offscreen |
| `client/combat_fx_slot_gpu_tests` | Own app | No | Offscreen |
| `client/flipbook_clock_gpu_tests` | Own app | No | Offscreen |
| `client_diagnostics/lib` (`shell_order_audit::gpu_tests`) | Already `primary_window: None`, `WinitPlugin` disabled, `RenderTarget::Image` | No | Offscreen: drop `xvfb-run` |
| `client/runtime_battle_tests` (production plugin schedule child) | `WinitPlugin` disabled, so no surface | No | Offscreen: drop `xvfb-run` |
| `client/{stance,status,area_link,weapon_battle,concealment,mode_atlas}_gpu_tests` | Production skirmish (`build_graphical_app_off_main_thread`): HUD in the picture, camera viewport from the panels and the window | Yes, by assembly | KWin (D-4) |
| `client/dome_fill_tests` | Production Visual Lab assembly | Yes, by assembly | KWin |
| `client/skirmish_match_tests` (`the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window`), `client/battle_hud_window_tests`, `client/battle_result_window_tests` | Real window by design | Yes | KWin |
| `client/editor_app_tests` (`test_render_verification_under_xvfb`, `map_editor --verify-render`, 1600×900) | Editor binary (egui) | Yes | KWin |
| `client/gpu_validation_tests` | `client` binary under validation, swapchain included | Yes: validates the present path | KWin (now validates radv's DRI3 WSI) |
| `client/{round2_visual,smog_bouncing_bubbles,lab_scene,terrain}_tests` | `client` / `terrain_preview` binaries | Yes (the binaries open a window) | KWin |
| `client_diagnostics/frame_budget_tests` | `frame_budget` binary, display path declared | Yes | KWin, declares `kwin-virtual` |
| `client_diagnostics/showcase_tests` | `showcase take` | Yes (HUD) | KWin |

The production skirmish children were left out of the offscreen path on purpose (§12). Making them offscreen would
mean:

- rendering the HUD through `bevy_egui`'s `EguiRenderToImage`;
- rewriting 9 `PrimaryWindow` call sites (`input_policy`, `camera`, `selection`, `lab_ui`, `battle_input`) to take
  the image size instead of the window size;
- and the tests would then no longer exercise the production window path.

---

## 3. Diagnosis

- **Hypothesis H1 (owner's, consistent with §2.1 and §2.2):** a radv child's extra time is
  presented frames × about 105 ms of CPU copy in the software WSI.
  - Example: an `effect_slice` child spends about 25 s on startup (an offscreen child of the same kind,
    `actual_loaded_scene_child`, takes 26 s: `docs/evidence/2026-11-10-gpu-child-timeouts.md`). The remaining
    ~55 s at 105 ms is about 520 presented frames (estimate, **verify** in WP1).
  - lavapipe pays a software present too. Its images are in cached system memory, so that copy is cheap (**verify**).
- **Not the cause (ruled out or already handled):**
  - lock contention (§2.1);
  - the readback buffer (§2.3);
  - a cold executable (`preload_executable` already reads it before the lock).
- **Unknown:** whether FIFO pacing under Xvfb's software present also throttles. Bevy's window default is
  `PresentMode::AutoVsync`. WP1 records the frame interval.

---

## 4. Design decisions

### D-1. Offscreen for every child that reads only its own pictures

- **New production module `client::offscreen`** (production code, because `lab_app` and the review tool
  `client_diagnostics::effect_lab` use it). It holds:
  - `OffscreenCanvas { image: Handle<Image>, size: UVec2 }`, created as an `Image` of the size and format of D-3 with
    usage `RENDER_ATTACHMENT | COPY_SRC | TEXTURE_BINDING | COPY_DST`;
  - `windowless_plugins(builder)`, which applies `WindowPlugin { primary_window: None, exit_condition:
    ExitCondition::DontExit, .. }`, disables `bevy::winit::WinitPlugin` and adds
    `ScheduleRunnerPlugin::run_loop(Duration::ZERO)`. Without winit the default runner runs once (**verify**), and
    `run_render_app` needs a loop;
  - `CaptureTarget { PrimaryWindow, Image(Handle<Image>) }` with `fn screenshot(&self) -> Screenshot`.
- **`lab_app` gains `surface: LabSurface { Window, Offscreen }`.**
  - Tests and `effect_lab` use `Offscreen`.
  - `Window` stays for interactive viewing and for D-6's equivalence test.
  - The lab camera targets the canvas, and `drive_lab` captures through the program's `CaptureTarget`.
- **`effect_gpu_tests`, `combat_fx_slot_gpu_tests` and `flipbook_clock_gpu_tests` change their own apps the same
  way.**
- The 1280×720 size, camera, time steps, `settled` frame and PNG writing do not change.

### D-2. No readback code of our own; copy only on capture ticks

- Captures stay on Bevy's `Screenshot` (§2.3): a GPU copy into a `DOWNLOAD` (`HOST_CACHED`-ranked) buffer, only on
  the requested frame.
- A persistent staging buffer or our own copy pass is rejected: it would duplicate Bevy's code with no measured need.
- If WP1 shows radv placing `MAP_READ` buffers in uncached memory, that becomes a finding, and this decision is
  reopened by amendment.
- Frames between captures do no copying, because nothing presents them.

### D-3. The offscreen image has the swapchain's format

- WP1 records the surface format Bevy configures today under Xvfb (expected `Bgra8UnormSrgb`, **verify**). The
  canvas uses that format.
- The render graph (main texture, tonemapping, deband dither, upscaling into the output) is the same for a window
  and an image target. Only the output target changes, so pixels must match. `Image::try_into_dynamic` converts
  BGRA to RGBA as it does today.

### D-4. Windowed children on radv present through a headless KWin

- The harness launches `kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts --width W --height H
  --socket ss-gpu-<pid>-<n> --exit-with-session "<status shim> <program> <args>"` under GNU `timeout`, after taking
  the lock.
  - KWin composites on the RX 580 (radeonsi), inside the child's lock (Q0019's practice).
  - `XDG_RUNTIME_DIR` must be set; it is checked as a prerequisite.
  - `MESA_VK_WSI_DEBUG` is removed.
  - W×H is the screen the caller asks for: 1280×720, or 1600×900 for the editor.
- **Exit status.** KWin exits 0 even when the program it runs fails (§5.3 probes). The harness therefore writes a
  per-run status shim into a temporary directory. The shim runs the program and writes its exit code to a status
  file, and the harness treats that file as the child's exit status. A missing status file is a failure.
  - KWin splits the session string on whitespace, so the harness refuses (panics naming the argument) any argument
    or path that contains whitespace.
- **Requirements WP1 must confirm** (probe P-K):
  1. the program's exit status reaches the harness;
  2. the program's whole stdout/stderr reaches the captured log (the adapter line and validation messages);
  3. the timeout's signal kills KWin, Xwayland and the program with no leftovers;
  4. concurrent runs use distinct sockets;
  5. per-run KWin start and stop overhead is at most 3 s;
  6. the environment the harness sets reaches the program.

  If 1 to 4 cannot all hold, D-4 falls back to launching KWin without a session and starting the child against its
  Xwayland display (`--xwayland-display` and readiness wait, **verify**). If neither holds, windowed children stay
  on Xvfb with `sw` (today's path, recorded as a limit). The offscreen part of this plan stands either way.
- **Display choice:** `SPACE_SCUFFLE_GPU_TEST_DISPLAY = auto | kwin | xvfb`. `auto` means KWin on radv when
  `kwin_wayland` and `XDG_RUNTIME_DIR` exist, otherwise Xvfb with `sw`. The fallback is printed on the selection
  line and recorded in the ledger (`display=`), never silent. A forced choice never falls back, as with
  `SPACE_SCUFFLE_GPU_TEST_DRIVER`. lavapipe always uses Xvfb: KWin would put compositing on the physical GPU without
  a lock.

### D-5. The test display does not wait for vertical blank

- With `SPACE_SCUFFLE_VIRTUAL_GPU=1` (already "a test display": hidden window), `assemble_graphical_app` and the
  editor set `PresentMode::AutoNoVsync`. Under KWin, FIFO caps a child at 60 Hz (§2.2).
- Present mode does not affect what is rendered. Frame-equals-tick assumptions were removed in harness plan D9.
  WP3 re-audits the windowed children for any timing that relies on the frame interval.
- Only applies if WP1 shows that FIFO limits a child.

### D-6. Pictures are proven unchanged, and no golden is re-recorded

- **Equivalence tests** (D-1 and D-4) capture the same program on today's path and on the new path, on radv and on
  lavapipe. The expected result is bit-identical PNG pixels.
  - A difference is accepted only with a recorded cause (format swizzle or rounding), and only within a maximum of
    1/255 on any channel. Anything larger stops the WP (§7, stop rule).
- **This plan re-records no golden.** The per-driver effect goldens (`crates/client/tests/data/effects/{radv,lavapipe}`,
  ≥ 99 % within 8/255 and a mean of at most 1.5/255) must pass unchanged.

### D-7. The harness says which display a child used

- `GpuCommand` gains `display: ChildDisplay { None, Virtual { screen } }`, and the ledger line gains
  `display=offscreen|kwin|xvfb-sw|xvfb`.
  - `virtual_gpu_command` and `virtual_gpu_command_on_screen` keep their meaning (a window).
  - New `offscreen_gpu_command(binary)` and `run_offscreen_test_child(test_name, label)` add no `xvfb-run`, remove
    `DISPLAY`, `WAYLAND_DISPLAY` and `MESA_VK_WSI_DEBUG`, and keep `SPACE_SCUFFLE_VIRTUAL_GPU=1`, the driver, the lock
    and the adapter check. With `DISPLAY` removed, a child that opens a window by accident fails loudly instead of
    reaching a desktop.
- The `ChildTimeline` milestone "Creating new window" is absent for offscreen children, so its time counts toward
  "startup and frames", as documented.
- The module doc's "Limits of the gate" changes: radv windowed children now exercise the DRI3 present path through
  KWin.
- The run limits (120 s radv, 300 s lavapipe), the lock and validation are unchanged.

### D-8. C21 knows the new builders

- `RENDER_DEVICE_MARKERS` (`crates/workspace_checks/src/gpu.rs`) gains `lab_app` (missing today) and
  `windowless_plugins`, so a test cannot build an offscreen renderer in its own process without the harness.
- Planted-tree unit tests cover both markers.

### D-9. Option 2: a measured rule, applied per target

- Applied only after WP2 to WP4. The rule is in §8.
- If adopted:
  - `suites.ron` targets gain `gpu_driver: Some(Radv)`. Unmarked GPU targets run on the functional driver, and the
    gate exports `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` for them **unless the variable is already set**, so CI's
    explicit `lavapipe` and a developer's forced `radv` win.
  - Lavapipe GPU targets take no lock, so they leave the dedicated GPU lane and are admitted like `Slow` targets,
    against the memory budget of plan `2026-11-11-faster-gates` (40 GiB measured peaks, 56 GiB `anon` ceiling).
  - The harness sets `LP_NUM_THREADS` per lavapipe child to the value §8's measurement picks.
- A radv canary set stays on radv so the real driver keeps gate coverage: `gpu_validation_tests`,
  `battle_hud_window_tests` and `frame_budget_tests` (the frame-budget tool's real present path).
- The frame-budget measurement itself and review captures stay on radv (owner).

### D-10. Measurement before code

- WP1 commits only evidence.
- Its offscreen prototype is a measurement-only patch in a scratch worktree, never committed. This is the practice
  of `2026-11-03-close-phase-6-7-8-exits` §5.3.
- If H1 is refuted (the software present is less than 25 % of a radv child's time), stop and replan from the
  profile.

---

## 5. File ownership

| WP | Files |
|---|---|
| WP1 | `docs/evidence/2026-11-17-gpu-test-readback.md` (new); scratch only: `~/scratch/gpu-readback/` |
| WP2 | `crates/client/src/offscreen.rs` (new), `crates/client/src/lib.rs` (module), `crates/client/src/effects/lab/{mod,driver}.rs`; `crates/client/tests/support/gpu_run.rs` (D-7 offscreen); `crates/client/tests/{effect_slice,effect_gpu,combat_fx_slot,flipbook_clock}_gpu_tests.rs`, `crates/client/tests/runtime_battle_tests.rs` (call sites); new `crates/client/tests/capture_path_gpu_tests.rs`; `crates/client_diagnostics/src/shell_order_audit/gpu_tests.rs`, `crates/client_diagnostics/src/effect_lab.rs`; `crates/workspace_checks/src/gpu.rs`; `crates/gate/suites.ron` (new target `client/capture_path_gpu_tests` classed `Gpu`, C22) |
| WP3 | `crates/client/tests/support/gpu_run.rs` (D-4, D-7 KWin); `crates/client/src/lib.rs` (`assemble_graphical_app`, D-5), the editor's window setup (D-5); `crates/client_diagnostics/tests/frame_budget_tests.rs` (declares the harness's display); `crates/client/tests/capture_path_gpu_tests.rs` (KWin equivalence); `crates/client/tests/gpu_validation_tests.rs` (allowlist only if a cause is recorded) |
| WP4 | `crates/gate/src/{execute,runner,test_log}.rs` (ledger variable, top-5 table), `crates/gate` tests |
| WP5 | Evidence file (append) |
| WP6 (conditional) | `crates/gate/src/{catalog,schedule,execute}.rs`, `crates/gate/suites.ron`, `crates/gate/tests/{suite_catalog_tests,selection_tests}.rs`, `crates/client/tests/support/gpu_run.rs` (`LP_NUM_THREADS`) |
| WP7 | `AGENTS.md` §7 and `docs/guides/gpu-and-disk.md` (owner-approved text only), `docs/plans/README.md`, pointer amendment in `docs/plans/2026-11-03-gpu-test-harness.md`, `docs/questions/` (Q0083 and Q0086 closed with resolutions) |

---

## 6. Work packages (commit order)

- Each code WP writes its tests first (red), then the code, runs `cargo gate quick` and hands back. A verifier runs
  `cargo gate package` (Q0061).
- WP2 and WP3 change `gpu_run.rs`, a support file of every GPU target, so their package gates run the whole GPU lane.
  That run is also a measurement and is recorded.
- Sizes: S ≤ 1 day, M is 2 to 3 days. Branch `gpu-readback`.
- WP2 and WP3 merge to `main` only together with the owner-approved §7 text (§9).

| WP | Content | Tests (written first) | Gate and exit | Size |
|---|---|---|---|---|
| WP0 | This plan's row | — | Docs-only `package` | — |
| WP1 | Measure: ledger, profile, A/B, P-K, memory types, formats | — (evidence only) | Evidence file; H1 confirmed or refuted (D-10); Q0086 filed | S to M |
| WP2 | Offscreen children (D-1 to D-3, D-6, D-7 offscreen, D-8) | T-1, T-3, T-5, T-6, T-7 | `package` (whole GPU lane) on radv; the same GPU targets once with `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`; ledger attached | M |
| WP3 | KWin display for windowed radv children (D-4, D-5, D-7) | T-2, T-4, T-5, T-8 | As WP2 | M |
| WP4 | Gate writes the ledger and prints the slowest children | T-9 | `package` (gate source change selects the complete suite) | S |
| WP5 | Re-measure; option 2 decision (§8) | — | Evidence; decision recorded as an amendment | S |
| WP6 | Only if §8 says so: per-target driver, scheduling, `LP_NUM_THREADS` (D-9) | T-10 | `package` and `merge`; CI green | M |
| WP7 | Charter text, bookkeeping | `workspace_checks` (C18, C20, C21, C22) | `merge`; plan row Done | S |

### WP1: Measure

All GPU commands run one at a time, each in its own `with-gpu-lock` when on radv. Any workspace `cargo test` runs
under `flock ~/scratch/gate.lock`. Before each run, record `date -u`, `/proc/loadavg` and the `anon` line of the
user slice's `memory.stat`. Raw logs go to `~/scratch/gpu-readback/<date>/`. Only the tables go into the evidence
file.

- **M1. Per-child ledger, both drivers.**
  - Build the test binaries once.
  - Run every GPU target as the gate runs it (its `test_threads`), with
    `SPACE_SCUFFLE_GPU_RUN_LEDGER=~/scratch/gpu-readback/<date>/ledger-radv.tsv` and
    `SPACE_SCUFFLE_GPU_RUN_LOG_DIR=…/logs-radv`.
  - Repeat with `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` (`ledger-lavapipe.tsv`). The local lavapipe is Mesa 25.0.7,
    so terrain children get the Q0009 anisotropy override. Record that.
  - Table per target: children, sum and maximum of `run_s`, `preload_s`, `lock_wait_s`, phases, and target wall time.
- **M2. Profile one radv child.**
  - Run the `effect_slice` stills child of subject 0 directly with the harness's environment:
    `with-gpu-lock perf record -F 997 -g -o …/perf-slice-radv.data -- xvfb-run -a -s "-screen 0 1280x720x24" env WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json WGPU_ADAPTER_NAME=radv MESA_VK_WSI_DEBUG=sw SPACE_SCUFFLE_VIRTUAL_GPU=1 WINIT_UNIX_BACKEND=x11 SPACE_SCUFFLE_EFFECT_SLICE_OUT=<dir> CARGO_MANIFEST_DIR=<crates/client> <effect_slice_gpu_tests binary> --ignored --exact effect_slice_stills_child --nocapture`
    (plus the subject-index variable the child reads, **verify** its name in `run_child`).
  - `perf` follows `xvfb-run`'s children, so Xvfb is profiled as well.
  - Report `perf report --no-children --sort comm,dso,sym` top 20, and the share of cycles in the WSI copy path
    (Mesa's x11 software present, the copy routine it calls, and Xvfb's `PutImage`/SHM handling).
  - Convert that share to seconds and, using §2.2's per-frame figure, to presented frames.
  - Same profile for `windowed_hud_child`, and for the slice child on lavapipe.
- **M3. Present microbenchmark (`vkcube --c 300`, 1280×720).** Arms:
  - Xvfb with `sw`, immediate and FIFO;
  - Xvfb with each other software-WSI flag Mesa 25.0.7 documents for `MESA_VK_WSI_DEBUG` (list them from the Mesa
    25.0 docs; **verify** which exist);
  - KWin through Xwayland, immediate and FIFO;
  - lavapipe under Xvfb.

  Record ms per frame. Then measure KWin's start and stop overhead: 10 runs of a session that only runs `true`.
- **M4. A/B on real children, with and without `MESA_VK_WSI_DEBUG=sw`.**
  - (a) Today's path.
  - (b) KWin with no `sw`, by hand per §5.3's command: `windowed_hud_child`, `status_pose_child` and
    `map_editor --verify-render` (1600×900).
  - (c) Offscreen, with the measurement-only `lab_app` patch (D-10): the slice stills child on radv and on lavapipe.
  - Per arm: child wall time (3 runs), frame interval, and a bit-for-bit comparison of every PNG with arm (a). This
    is an early read on D-3 and D-6.
- **M5. Memory types and formats.**
  - `with-gpu-lock vulkaninfo` with the radv manifest: list memory types and heaps, and apply gpu-alloc's
    `reverse_priority` to `DOWNLOAD` to name the type a screenshot buffer gets. Same for lavapipe.
  - Record the surface format Bevy configures under Xvfb, KWin and lavapipe, from a debug log line in the
    measurement patch (`wgpu_core` surface configure, **verify** which target logs it).
- **P-K. KWin harness requirements (D-4 items 1 to 6):**
  - a session that exits 3 (status reaches the shim file);
  - a session that prints 1 MiB to stderr (all of it captured);
  - a session that sleeps under a 10 s `timeout` (no leftover `kwin_wayland` or `Xwayland`, checked with `pgrep -u`);
  - two concurrent runs with distinct sockets;
  - `env` printed by the session (variables reach the program).
- **Exit.** The evidence file states:
  - H1 confirmed or refuted, and the radv per-child share;
  - whether P-K holds, and which D-4 variant is chosen;
  - the canvas format (D-3);
  - FIFO or no FIFO (D-5);
  - the expected new lane time (estimate);
  - the paths the §7 text must name.

  The orchestrator then files Q0086 with the exact §7 diff (§9).

### WP2: Offscreen children

- D-1 to D-3, D-7 (offscreen) and D-8. The four test children and `effect_lab` switch to offscreen. The
  `shell_order_audit` and `runtime_battle` children switch to `run_offscreen_test_child`.
- **Tests.**
  - **T-1** `capture_path_gpu_tests::an_offscreen_lab_capture_equals_its_window_capture`. One program: the first
    splash subject, stills at 0.08, 0.3 and 0.75 of its life, plus three 40 ms sequence frames. It runs as two
    children:
    - `LabSurface::Window` through `virtual_gpu_command` with the display forced to `xvfb`, which is today's path;
    - `LabSurface::Offscreen` through `run_offscreen_test_child`.

    The parent requires bit-identical pixels, or the D-6 tolerance with a printed cause. It runs on the selected
    driver; WP2's gate runs it on radv and on forced lavapipe, and CI on lavapipe.
  - **T-3** the effect goldens pass unchanged on radv and lavapipe (`effect_slice_gpu_tests`, untouched thresholds).
  - **T-5** `gpu_run` unit tests:
    - an offscreen command has no `xvfb-run`;
    - `DISPLAY`, `WAYLAND_DISPLAY` and `MESA_VK_WSI_DEBUG` are removed;
    - the ledger line has `display=offscreen`;
    - the timeline of an offscreen log has no window milestone.
  - **T-6** `workspace_checks::gpu` planted-tree tests:
    - `lab_app(` in a non-ignored test is a finding;
    - `windowless_plugins` inside a `gpu_run` child is clean.
  - **T-7** `offscreen` unit tests:
    - the canvas has the four usages and the chosen format;
    - `CaptureTarget::Image` yields `Screenshot::image` of that handle;
    - `windowless_plugins` leaves no `PrimaryWindow` and no `WinitPlugin`.

### WP3: KWin display for windowed children

- D-4, D-5 and D-7 (KWin). `frame_budget_tests` declares `--display-path kwin-virtual` when the harness's display is
  KWin, and `xvfb-software-present` when it is Xvfb (it reads the choice from the harness).
- **Tests.**
  - **T-2** `capture_path_gpu_tests::a_kwin_capture_equals_its_xvfb_capture` (radv only; on lavapipe it asserts the
    display is `xvfb`). The production skirmish child of `concealment_gpu_tests` (three shots, short) runs under Xvfb
    with `sw`, then under KWin. Pixels must be bit-identical, or within the D-6 tolerance with a cause.
  - **T-4** every windowed GPU target passes on radv through KWin and on forced lavapipe through Xvfb.
    - `battle_hud_window_tests` must still read 1280×720 physical pixels at scale factor 1 (**verify** KWin's virtual
      output scale).
    - `gpu_validation_tests` must pass with its current allowlist. A new finding on the DRI3 path is fixed or
      allowlisted with its cause (harness plan D8), never silently.
  - **T-5 (KWin)** unit tests of the command builder:
    - the order is `timeout`, then `kwin_wayland` with its flags, then the session string;
    - the socket name is unique per run;
    - an argument with whitespace is refused;
    - a missing status file is a failure;
    - `display=kwin` is recorded in the ledger.
  - **T-8** radv-only self-tests through the shared lock (a real KWin uses the GPU):
    - `a_failing_kwin_session_fails_the_run` (exit 3 reaches the verdict);
    - `a_timed_out_kwin_session_leaves_nothing_running` (20 s limit through a test-only limit override; no
      `kwin_wayland`/`Xwayland` of the run remains).

    On lavapipe both assert that the display choice is `xvfb`.

### WP4: The gate writes the ledger

- Setup evaluation item 7(a). For every run, the gate sets `SPACE_SCUFFLE_GPU_RUN_LEDGER=<run log dir>/gpu-ledger.tsv`
  unless the variable is already set. The result table is followed by the five slowest children (label, driver,
  display, `run_s`, slowest phase).
- **Test T-9:** a gate unit test with a fixture ledger prints the five slowest in order, and a run without GPU
  targets prints nothing.

### WP5: Re-measure and decide option 2

- Re-run M1 on `main` with WP2 to WP4. Report per-target times (radv against lavapipe) and the lane against the
  general lanes.
- Apply §8. Record the outcome as an amendment of this plan: option 1 only, or option 2 with its canary set and
  `LP_NUM_THREADS`.

### WP6: Option 2 (only if §8 adopts it)

- D-9.
- **Tests T-10:**
  - `catalog` parses `gpu_driver`;
  - `suite_catalog_tests` refuses `gpu_driver` on a non-`Gpu` target;
  - `schedule` places lavapipe GPU targets outside the GPU lane and under the memory budget, and keeps `Radv` ones in
    it;
  - `execute` sets the driver variable only when it is unset;
  - `selection_tests` counts stay derived from the catalog.
- The §7 text for option 2 goes to the owner in the same question or a follow-up (§9).

### WP7: Bookkeeping

- `AGENTS.md` §7 and `docs/guides/gpu-and-disk.md` (where the harness and driver detail of §7 lives since Q0085)
  with the approved text only.
- A pointer amendment in `2026-11-03-gpu-test-harness.md` (D5's environment table, the "Limits" bullet).
- This plan's row: Done.
- Q0083 and Q0086 closed with their resolutions.
- `suites.ron` comment updates (the radv/lavapipe sentences in the GPU block).

---

## 7. Test plan (summary) and stop rule

| Layer | What it proves | Where |
|---|---|---|
| Picture equivalence | Offscreen equals window capture; KWin equals Xvfb capture; radv and lavapipe | `capture_path_gpu_tests.rs` (T-1, T-2) |
| Goldens | No re-recording; per-driver effect goldens pass | `effect_slice_gpu_tests.rs` (T-3) |
| Whole lane | Every GPU target on radv (new paths) and on lavapipe | WP2 and WP3 gates, CI (T-4) |
| Harness | Command per display, ledger `display=`, KWin exit status, refusal of whitespace, no leftovers | `gpu_run` unit tests and radv self-tests (T-5, T-8) |
| Production helper | Canvas usages and format, capture target, windowless plugins | `offscreen.rs` unit tests (T-7) |
| Charter | C21 markers; C22 classes the new target; C20 paths in §7 exist | `workspace_checks`, `suite_catalog_tests.rs` (T-6) |
| Gate | Ledger and slowest table; option 2 scheduling | `crates/gate` tests (T-9, T-10) |

**Stop rule:**

- If T-1 or T-2 shows a difference above 1/255, or one without a stated cause, stop the WP and hand back with the
  difference image.
- If any effect golden fails, stop. Do not re-record.
- If H1 is refuted in WP1, stop the plan (D-10).

---

## 8. Decision rule for option 2

Measured in WP5, on one tree (`main` after WP4), with no other heavy job (charter §7), in the paired order radv,
lavapipe, lavapipe, radv. Each run is a `cargo gate package` whose change selects every GPU target, such as a
comment-only change to `crates/client/tests/support/gpu_run.rs`, and whose ledger WP4 records. Let:

- `R` = the GPU lane's wall time on radv;
- `G` = the longest general lane;
- `W_radv`, `W_lvp` = the test phase's wall time with each driver.

1. **The GPU lane is no longer the critical path.** If `R ≤ G + 5 min` in both radv runs, keep radv: option 1 is
   enough. Record the result and skip WP6.
2. **Otherwise, build WP6's gate part behind the existing driver variable and measure lavapipe fairly.** In this run,
   lavapipe GPU targets are admitted into the general lanes by memory; `LP_NUM_THREADS` is tried at 4 and 8.
   Adopt option 2 for functional targets only if **all** hold, in both pairs:
   - `W_lvp ≤ 0.85 × W_radv`, and `W_radv − W_lvp ≥ 10 min`;
   - every test that passes on radv passes on lavapipe;
   - the slice's `anon` peak stays below 56 GiB, and no target exceeds its measured memory peak by more than 25 %.
3. Otherwise keep radv for everything, and keep WP6's code only if it serves something else (forcing lavapipe in a
   gate is already possible through the driver variable). Revert it if it serves nothing.

The thresholds come from the faster-gates A/B (plan `2026-11-11-faster-gates`, WP6), which accepted a measured
change only when every run showed it. 15 % and 10 min keep a run-to-run spread of a few minutes from deciding a
charter change.

---

## 9. Charter §7 wording (owner)

The charter states the harness's display path as part of a rule ("runs each GPU test child on the physical GPU
(`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`)"). Since Q0085 that sentence is in
`docs/guides/gpu-and-disk.md`, which §7 links and which binds like the charter; the diff Q0086 shows covers both
files. This plan makes that sentence false for offscreen and KWin
children, so the text must change, and that change belongs to the owner.

- The orchestrator files Q0086 at the end of WP1 with the exact diff (Q0084's procedure). Its default: WP2 and WP3 are
  developed and gated on `gpu-readback`, and merge to `main` together with the approved text, not before.
- Draft for option 1 (final wording after WP1). The bullet "GPU tests lock the GPU themselves" would read:

  > `crates/client/tests/support/gpu_run.rs` runs each GPU test child on the physical GPU (`radv`). A child that only
  > reads its own pictures renders offscreen, with no window and no display. A child that needs a window presents
  > through a headless KWin (`kwin_wayland --virtual --xwayland`), which composites on the same GPU inside the child's
  > lock; where KWin is missing, the harness uses `xvfb-run` with `MESA_VK_WSI_DEBUG=sw` and says so. It takes …
  > *(rest unchanged)*

- Draft additions for option 2 (only if §8 adopts it):
  - The bullet "Use the physical GPU" gains: "Gate GPU tests run on `lavapipe`, unless their entry in
    `crates/gate/suites.ron` asks for `radv`; frame-budget measurements and review captures use the physical GPU
    (owner, Q0083)."
  - "Software Vulkan is the fallback" gains "or for gate GPU tests (Q0083)".

---

## 10. CI impact

- CI stays on lavapipe (`SPACE_SCUFFLE_GPU_TEST_DRIVER: lavapipe` in `ci.yml`).
- **Offscreen children** no longer start Xvfb or present on CI either. `effect_slice_gpu_tests` (937 s on CI) should
  shorten (estimate; the next green CI run's log is recorded in WP5).
- **Windowed children** stay on Xvfb on CI. KWin is not installed there and is not needed. `xvfb` stays in the apt
  list.
- The 150-min job timeout and the thread caps in `suites.ron` (needed for the 16 GB runner) are unchanged.
- **Under option 2,** CI's explicit variable keeps lavapipe, and the scheduling change does not apply on CI, which
  keeps one lane (`SPACE_SCUFFLE_GATE_JOBS: 1`).
- The showcase render job (lavapipe, its own `xvfb-run` launcher) is unchanged.

---

## 11. Risks

| Risk | Mitigation |
|---|---|
| H1 is wrong: the radv slowdown is elsewhere | WP1 profiles before any code; stop rule D-10 |
| The offscreen picture differs (format, dither, scale factor, sRGB view) | D-3 uses the swapchain format; T-1 on both drivers; stop rule |
| Without winit the app runs one update and exits | `windowless_plugins` adds `ScheduleRunnerPlugin::run_loop`; T-7; the existing `shell_order_audit` child is the precedent |
| KWin hides the exit status, splits arguments, leaves processes, or needs `XDG_RUNTIME_DIR` | Status shim, refusal of whitespace, P-K, T-8; prerequisite check; fallback chain in D-4 |
| KWin start-up eats the gain on short children | P-K bounds it at 3 s; per-child ledger with `display=`; short children may use `xvfb` by an explicit per-command override if measured slower |
| New validation findings on radv's DRI3 path | Treated as findings: fixed or allowlisted with cause (harness D8) |
| FIFO under KWin caps frames at 60 Hz | D-5 (`AutoNoVsync` on the test display) |
| Frame-time assumptions resurface with faster frames | WP3 audit (harness plan D9 pattern); tick-based staging already in place |
| C21 misses a new offscreen builder | D-8 markers and planted-tree tests |
| Option 2's lavapipe CPU load slows the general lanes | §8 decides on whole-gate wall time, not lane time; `LP_NUM_THREADS` measured |
| Option 2 loses radv coverage (the production anisotropic sampler locally on Mesa 25.0; driver-specific races like harness §3.3) | Canary set on radv (D-9); CI's Mesa 25.2 lavapipe keeps the production sampler |
| The charter text waits for the owner | Q0086 filed after WP1; development and gates continue on the branch |

---

## 12. Out of scope (recorded, Q0002)

- **Offscreen production skirmish children** (the HUD through `EguiRenderToImage`, window-independent viewport and
  input). This needs a client refactor of the 9 `PrimaryWindow` sites and would stop the tests exercising the window.
  The KWin path removes their present cost without it.
- **Merging each `effect_slice` subject's three children into one** (setup evaluation 7(d)). WP4's ledger will show
  whether startup now dominates; that would be a follow-up note.
- **Letting the GPU lane run during the exclusive valgrind job** (setup evaluation 7(c)): a gate scheduling note of
  its own.
- **KWin for the manual capture tools** (`battle_capture`, `runtime_capture`, `showcase render` on radv). They can
  adopt the harness's display choice later; the review contact sheets of `effect_lab` already gain from WP2.
- `VK_EXT_headless_surface`: wgpu 23 cannot create such a surface. Weston is not installed. A Bevy or wgpu upgrade is
  not part of this plan.
- The frame-budget measurement itself: it is unchanged and already uses headless KWin (Q0019).
- Bevy default-feature trimming (setup evaluation item 10).

---

## Amendment 1: WP2 as built (2026-11-17, short note, charter §6)

WP2 is on branch `gpu-readback`, made from `main` at `7bf4b7ff`. It follows D-1 to D-3, D-6, D-7 (offscreen part)
and D-8 with WP1's settlements: the canvas is `Bgra8UnormSrgb` (D-3), the windowless app loops with
`ScheduleRunnerPlugin::run_loop(Duration::ZERO)` (D-1), D-2 stands (no readback code of our own), and D-5 is not
needed. Nothing from WP3 onwards is touched: windowed children still run under `xvfb-run` with
`MESA_VK_WSI_DEBUG=sw`, and `docs/guides/gpu-and-disk.md` waits for Q0086. Per Q0086's default, the branch merges to
`main` only together with WP3 and the approved text.

**As built.**

- **`client::offscreen`** (`crates/client/src/offscreen.rs`): `CANVAS_FORMAT`, `CANVAS_USAGES`,
  `OffscreenCanvas { image, size }` with `image(size)`, `create(images, size)` and `install(app, size)` (the last
  inserts the canvas and `CaptureTarget::Image` of it), `CaptureTarget { PrimaryWindow, Image }` with
  `screenshot()` and `camera_target()`, `windowless_plugins(group)`, and the typed `OffscreenError`
  (`NoImageAssets`, `EmptyCanvas`). Choices the plan left open: `windowless_plugins` takes and returns a
  `PluginGroupBuilder` and also sets `close_when_requested: false`; `CaptureTarget` is a resource that the program's
  camera spawn and its screenshots read, so a program names its target once.
- **`lab_app(root, program, vocabulary, size, surface)`** with `LabSurface { Window, Offscreen }`. The stage camera
  is spawned with `Camera { target: capture.camera_target(), .. }` (for `Window` this equals `Camera::default()`),
  and the driver spawns `capture.screenshot()`. `LabAppError` gains `Canvas`. The effect GPU tests and
  `client_diagnostics::effect_lab` (the contact sheets of `battle_capture --effect-lab`) use `Offscreen`.
- **Harness (D-7).** `ChildDisplay { None, Virtual { screen } }`; `GpuCommand::renderer(binary, &display,
  selection)` is public so the unit tests can build a command for a fixed selection; `offscreen_gpu_command` and
  `run_offscreen_test_child`. An offscreen child has no `xvfb-run`, has `DISPLAY`, `WAYLAND_DISPLAY` and
  `MESA_VK_WSI_DEBUG` removed, inherits `WINIT_UNIX_BACKEND`, and keeps `SPACE_SCUFFLE_VIRTUAL_GPU=1`, the driver
  variables, the lock and the adapter check. The ledger line gains `display=` after `driver=`, with two values beyond
  the plan's list: `host` (no `xvfb-run`, the test process's own display) and `none` (a lock self-test). `GpuRun`
  keeps its ledger line (`ledger`), and `GpuCommand` gains the read-only `display_name`, `command_line` and
  `environment`; it still offers no way to start a child except through the lock.
- **Converted children (offscreen):** every child of `effect_slice_gpu_tests` (stills, sequence halves, masked line,
  shots, looks: 92 children), `effect_gpu_tests`, `combat_fx_slot_gpu_tests` and `flipbook_clock_gpu_tests` (own
  apps through `windowless_plugins` and `OffscreenCanvas::install`; the flipbook child no longer reads a window's
  scale factor, because a canvas's is 1), and, through `run_offscreen_test_child`, the `runtime_battle_tests`
  production-schedule child and both `client_diagnostics` `shell_order_audit::gpu_tests` children, which already had
  no surface.
- **D-8:** `RENDER_DEVICE_MARKERS` gains `lab_app` and `windowless_plugins`, with the planted-tree test T-6.
- **Deviation from §5's file list:** `crates/gate/src/sources.rs` also changes. C22
  (`suite_catalog_tests`) requires every `Gpu` target to call the harness, and `uses_gpu_harness` knew only
  `run_test_child`, `GpuCommand` and `virtual_gpu_command`, so `client_diagnostics/lib` failed once its children
  moved to `run_offscreen_test_child`. The markers `run_offscreen_test_child` and `offscreen_gpu_command` are added
  with unit tests. `crates/gate/src/**` is in the catalog's `global` list, so WP2's `package` gate selects the
  complete suite.
- **Tests.** T-1 is `capture_path_gpu_tests` (new target, classed `Gpu`). WP3's display override does not exist
  yet, so its window child uses `virtual_gpu_command`, which is the Xvfb path today; the test also checks the
  ledger's `display=` of both children. T-5 lives in `gpu_validation_tests.rs`, with the harness's other tests:
  `an_offscreen_child_has_no_display_and_no_software_present`, `a_windowed_child_keeps_its_virtual_display`,
  `the_ledger_line_names_the_display` and `an_offscreen_timeline_has_no_window_phase`. T-7 is five unit tests in
  `offscreen.rs`; the windowless-plugins test builds `GamePresentationPlugins` with `backends: None`, so it creates
  no render device (C21).

**Evidence** (raw data in `~/scratch/gpu-readback/`; scratch files that may be cleaned, so the tables below are the
record). Before-binaries were built from `7bf4b7ff` in a detached worktree, after-binaries from the branch, both
debug, both run directly as the gate runs them (`--test-threads` from `suites.ron`), one target at a time under
`flock ~/scratch/gate.lock`; GPU children took the lock through the harness.

- **Pictures (D-6, stop rule): no difference anywhere.**
  - T-1 on radv and on lavapipe: 6 of 6 captures bit-identical between the window child and the offscreen child.
  - Every capture of the converted targets, before against after, byte-compared (both builds use the same PNG
    encoder, so equal bytes mean equal pixels): on radv the 924 slice PNGs that `SPACE_SCUFFLE_EFFECT_SLICE_KEEP`
    keeps (31 effects: the splash children's and the look children's captures; the shot and masked-line children
    keep none and passed their pixel checks on both paths) and the 11 PNGs of `effect_gpu_tests` are byte-identical in both after-rounds against the first
    before-round; the `combat_fx_slot` child's three PNGs and slot report and the `flipbook_clock` child's report
    (the centre pixels of 64 shots) are identical when the children run by hand on both paths. On lavapipe (one
    round of each), the same 924 slice PNGs and 11 effect-block PNGs are byte-identical before against after, and
    so are the `combat_fx_slot` and `flipbook_clock` outputs.
  - T-3: the per-driver effect goldens pass unchanged in every run (no golden re-recorded, no threshold changed).
- **Timing on radv**, interleaved as before r1, after r1, after r2, before r2 (2026-11-17 06:28 to 08:17 UTC; one-minute
  load 12 to 187 on 16 vCPUs, `anon` below 0.5 GiB; another user's GPU work made some children wait for the lock,
  which the ledger separates). Wall is the test executable's whole run; run_s and lock wait are ledger sums.
  Round 1's before-arm walls come from the start stamps and log times (1 s resolution), because the script's wall
  field failed in that arm.

| Target | Arm | Wall s | Children | Sum run_s | Max run_s | Sum lock_wait_s |
|---|---|---:|---:|---:|---:|---:|
| `client/effect_slice_gpu_tests` | before r1 | 2116 | 92 | 1843.7 | 56.0 | 103.8 |
| | after r1 | 866.3 | 92 | 713.4 | 27.4 | 0.0 |
| | after r2 | 856.7 | 92 | 666.1 | 28.9 | 37.1 |
| | before r2 | 2235.5 | 92 | 1924.5 | 75.5 | 126.9 |
| `client/effect_gpu_tests` | before r1 | 124 | 1 | 4.5 | 4.5 | 118.6 |
| | after r1 | 5.4 | 1 | 3.4 | 3.4 | 0.0 |
| | after r2 | 83.0 | 1 | 3.3 | 3.3 | 78.4 |
| | before r2 | 4.1 | 1 | 3.5 | 3.5 | 0.0 |
| `client/combat_fx_slot_gpu_tests` | before r1 | 1 | 1 | 1.5 | 1.5 | 0.0 |
| | after r1 | 1.8 | 1 | 1.5 | 1.5 | 0.0 |
| | after r2 | 8.5 | 1 | 2.1 | 2.1 | 6.2 |
| | before r2 | 1.6 | 1 | 1.4 | 1.4 | 0.0 |
| `client/flipbook_clock_gpu_tests` | before r1 | 5 | 1 | 4.8 | 4.8 | 0.0 |
| | after r1 | 6.5 | 1 | 6.4 | 6.4 | 0.0 |
| | after r2 | 4.3 | 1 | 4.2 | 4.2 | 0.0 |
| | before r2 | 4.5 | 1 | 4.5 | 4.5 | 0.0 |
| `client/runtime_battle_tests` | before r1 | 29 | 1 | 13.7 | 13.7 | 0.0 |
| | after r1 | 34.0 | 1 | 18.2 | 18.2 | 0.0 |
| | after r2 | 39.4 | 1 | 15.9 | 15.9 | 0.0 |
| | before r2 | 27.5 | 1 | 12.8 | 12.8 | 0.0 |
| `client_diagnostics/lib` | before r1 | 14 | 2 | 12.4 | 11.8 | 0.8 |
| | after r1 | 11.6 | 2 | 10.3 | 10.1 | 10.3 |
| | after r2 | 17.8 | 2 | 17.4 | 17.1 | 0.3 |
| | before r2 | 12.3 | 2 | 10.8 | 10.4 | 10.7 |
| **Converted targets** | before r1 / r2 | 2289 / 2286 | 98 | 1880.5 / 1957.6 | | 223.2 / 137.6 |
| | after r1 / r2 | 925.6 / 1009.7 | 98 | 753.2 / 709.0 | | 10.3 / 121.9 |

  - `effect_slice_gpu_tests` falls from 2,116 to 2,236 s to 857 to 866 s (0.38 to 0.41 of its time), and its child time
    from 1,844 to 1,925 s to 666 to 713 s (0.35 to 0.39). The slowest child falls from 56 to 76 s to 27 to 29 s, so the
    radv run limit of 120 s has more room. This is less than WP1's 0.16 to 0.3 for the stills child alone: the
    sequence, shot and look children spend more of their time in start-up, which the display never cost.
  - The other converted targets render a few frames each; their differences are within the noise of the shared
    host (the `runtime_battle_tests` binary's wall is mostly its 17 headless tests, and those two children never
    presented). They gain nothing measurable, as WP1 predicted for children that present few frames.
  - The converted targets together: about 2,290 s before and 925 to 1,010 s after, wall; 1,880 to 1,960 s of child
    time before and 709 to 753 s after.
- **On lavapipe** (forced, one round, before then after, 08:17 to 09:20 UTC, load 31 to 71; information only, not
  interleaved): `effect_slice_gpu_tests` 1,780 s before and 1,840 s after (child time 1,639 and 1,691 s); the small
  targets were 1.4 to 28 s before and 4.7 to 37 s after, including the two children whose launch only lost
  `xvfb-run`, so the spread is the host's. As WP1 found, a lavapipe child is bound by rasterising, which no display
  change helps; CI keeps lavapipe and gains nothing measurable from WP2. Every target passed, the goldens included.

**Not done here (Q0061):** the `package` gate, which WP2's change runs over the complete suite (gate source) and the
whole GPU lane (`gpu_run.rs`), is the verifier's. WP2's own check ran `cargo gate quick`, the converted targets
above on both drivers, `capture_path_gpu_tests` on both drivers, `gpu_validation_tests` on radv, the `offscreen`
unit tests, `workspace_checks` and the `gate` crate's tests.

---

## Amendment 2: WP3 as built, WP2 review fixes, Q0086 applied (2026-11-17, short note, charter §6)

WP3 is on branch `gpu-readback` after `main` at `345da93f` was merged in (Q0086 answered). It follows D-4 (the
session variant with the status shim, as P-K chose), D-7 (KWin) and the WP1 findings. **Status: code and
lavapipe/unit checks done; the radv checks are not.** After the server's reboot the RX 580 has no render node
(`/dev/dri` holds only `card0`), so the harness selects `lavapipe` by itself. Per the owner, no lavapipe result
stands in for radv: the radv list at the end of this amendment is still to run before the branch merges.

**As built (`crates/client/tests/support/gpu_run.rs`).**

- **Display selection.** `SPACE_SCUFFLE_GPU_TEST_DISPLAY = auto | kwin | xvfb`, read once per test process by
  `selected_display()` and decided by the pure `select_display(request, driver, &HostDisplay)`:
  - `auto`: KWin on radv where `kwin_wayland` and `Xwayland` are on `PATH` and `XDG_RUNTIME_DIR` is a directory;
    otherwise `xvfb-run` (radv with `MESA_VK_WSI_DEBUG=sw`), with a `WARNING:` selection line naming what is
    missing; otherwise the process's own display (`host`); otherwise an error.
  - lavapipe never uses KWin (`auto` keeps it on Xvfb; `kwin` with lavapipe is an error), so no compositor runs on
    the physical GPU without the lock.
  - A forced `kwin` or `xvfb` never falls back. `virtual_gpu_command_with(binary, screen, request)` forces a display
    for one command (T-1, T-2, T-8).
- **Command.** `ChildDisplay { None, Xvfb { screen }, Kwin { screen, runtime_dir }, Host }`. `GpuCommand` now keeps
  the program, its arguments and variables, and builds the launch line at run time (`launch_command`): `timeout
  --kill-after=10 <limit>`, then `kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts --width W
  --height H --socket ss-gpu-<pid>-<n> --exit-with-session "<shim> <status> <program> <args>"`, W×H from the screen
  (1280×720 by default, 1600×900 for the editor). A KWin child gets `DISPLAY`, `WAYLAND_DISPLAY` and
  `MESA_VK_WSI_DEBUG` removed (KWin sets the display variables for its session), `XDG_RUNTIME_DIR` set explicitly,
  `WINIT_UNIX_BACKEND=x11` (as in WP1's M4), and the driver, lock and adapter check of every child.
- **Status shim.** Per run, `try_run` writes `session.sh` into `<XDG_RUNTIME_DIR>/ss-gpu-<pid>-<n>.session/` before
  the lock and removes the directory afterwards (a drop guard, also on panic). The shim runs the program and writes
  its exit status through a temporary name. `kwin_session_verdict` is the rule: `timeout`'s 124 is a timeout;
  otherwise the status file's code replaces KWin's; a missing or unreadable file fails the run, naming KWin's own
  code. The directory is not named like the socket: the first radv attempt used the socket's name, and KWin
  could not create its socket there (exit 1, no status), which this rule correctly reported as a failure.
- **Session words.** KWin splits the session string like a shell, so `session_word` refuses (panics naming it)
  any path or argument with a character outside ASCII letters, digits and `-_./=,:+@%`, a strict superset of the
  plan's "whitespace". The children's binaries, test names and temporary directories pass.
- **Ledger.** `display=kwin`. KWin's own start counts toward the `launch` phase; its and Xwayland's log lines have
  no `tracing` timestamp, so neither the validation parser nor the timeline reads them.
- **Self-test limit.** `set_run_limit_for_self_test(secs)`, used only by T-8.
- **D-5 is not built.** WP1 found no windowed child reaching KWin's 60 Hz (20 to 25 fps), so present mode stays
  `AutoVsync`; no production file changed in WP3.
- **`frame_budget_tests`** declares `--display-path` from the harness's choice (`kwin` → `kwin-virtual`, `xvfb-sw`
  or `xvfb` → `xvfb-software-present`, `host` → `desktop`) and checks `WAYLAND_DISPLAY` accordingly.
- **Module docs** of `gpu_run.rs` describe display selection, the session and the new limits; `suites.ron` needs
  no change (no target added; every change is in files the targets already own).

**T-2 deviation: the witness.** The plan named the `concealment_gpu_tests` child. Checked first, on radv before the
reboot: two Xvfb runs of `concealment_pose_child` differ in 7,906 to 10,442 pixels by up to 76/255 (KWin against
KWin about the same), so it cannot witness bit-identity. Its own patch checks still run on every display, unchanged.
The witness is the lab window child of `capture_path_gpu_tests` (fixed time steps): by hand on radv, two Xvfb runs,
two KWin runs and Xvfb against KWin were pixel-identical in 6 of 6 captures in every pair (Xvfb 25.1 to 25.8 s,
KWin 5.7 to 5.9 s). `a_kwin_capture_equals_its_xvfb_capture` runs that child forced on Xvfb and forced on KWin and
requires identical pixels; on a process without KWin (lavapipe, or radv without KWin) it states why and, on
lavapipe, that the display is Xvfb. The production skirmish children, `status_pose_child` and the editor keep
exactly the checks they have; no tolerance, bar, golden or reference changed.

**Tests (T-5, T-8) in `gpu_validation_tests.rs`.** `an_xvfb_child_keeps_its_virtual_display`,
`a_kwin_child_runs_its_session_under_timeout` (order and words of the launch line, variables, `display=kwin`),
`kwin_sockets_are_unique_per_run`, `a_kwin_argument_with_whitespace_is_refused`, `session_words_are_plain`,
`kwin_session_verdicts` (a missing status file fails), `kwin_screens_are_parsed`, `display_selection_matrix`, and
the radv self-tests `a_failing_kwin_session_fails_the_run` (exit 3 reaches the verdict and the program's output is
captured) and `a_timed_out_kwin_session_leaves_nothing_running` (20 s limit). The latter finds the run's processes
by a variable only it sets on `timeout`, plus all of `timeout`'s descendants by `/proc/<pid>/stat`, because
Debian's `kwin_wayland` has file capabilities and its `environ` is unreadable; and its session first connects
`xdpyinfo`, because KWin starts Xwayland only when an X client connects (seen by hand: a session running only
`sleep` has no Xwayland). After the run none of the sampled processes (same PID and start time) may remain.

**WP2 review (accepted with conditions), fixed in their own commits except where noted.**

- F1: the "GPU renderer" rule covers `crates/client/src/offscreen.rs`; no `client_diagnostics` GPU target uses it.
- F2: `GpuCommand::renderer` is private; runnable renderers come only from `selected_driver()`. The command tests
  use `GpuCommand::inspect`, whose commands name a nonexistent manifest and are refused by `try_run`.
- F3 (in the WP3 commit, same test functions): the lab program gains an empty-ground capture; every capture must
  not be one colour, and each still must change at least 500 pixels of the ground by more than 8/255. On lavapipe
  the effect captures change 13,698 to 20,280 pixels.
- F5: `client::offscreen` states that a canvas keeps a skipped frame and why a capture cannot show it (Bevy
  0.15.3's screenshot redirects that frame's output into its own texture).
- F6: `#[expect(.., reason)]` in `effect_gpu_tests`; long doc lines wrapped; T-1's window child is forced onto
  Xvfb and refuses `display=host` (in the WP3 commit).

**Q0086** is answered (option 1, plus the owner's standing delegation of GPU-rule edits) and closed. The guide's
bullet carries the approved text plus two sentences matching what was built: lavapipe stays on Xvfb, and the
display variable forces a choice without fallback.

**Checks run (no radv).** `cargo gate quick`; the non-GPU harness tests; on lavapipe (selected automatically,
no render node): T-1 (7 of 7 captures identical, F3 sanity passed), T-2 and T-8 (each reports "no KWin run" and
checks the lavapipe display is Xvfb), and all 19 windowed GPU targets listed below (19 of 19 pass; 52 children: 48 on Xvfb, 1 offscreen, 3 without a display; no KWin, no leftover Xvfb or KWin processes; `weapon_battle_gpu_tests` alone took 15 min on lavapipe under a host load near 120). These are lavapipe results and say nothing about radv. Before the reboot, on radv:
`a_failing_kwin_session_fails_the_run` passed; the timeout self-test failed twice, on its own process search (fixed
as described above), never on leftovers: `pgrep` showed no `kwin_wayland` or `Xwayland` after either run.

**Still to do on radv (the RX 580 must be back; nothing here was measured on lavapipe in its place):**

1. `gpu_validation_tests` on radv: `a_timed_out_kwin_session_leaves_nothing_running` (never passed yet in its final
   form), `a_failing_kwin_session_fails_the_run` again, and the validated `client` runs under KWin (new DRI3 path:
   any finding is fixed or allowlisted with its cause).
2. T-2 `a_kwin_capture_equals_its_xvfb_capture` and T-1 on radv.
3. T-4: every windowed GPU target on radv through KWin (the 19 targets of the ledger below), including
   `battle_hud_window_tests` (1280×720 at scale 1 under KWin), `editor_app_tests` (1600×900), `frame_budget_tests`
   (declares `kwin-virtual`) and `showcase_tests`. If a child's existing checks fail under KWin, stop and report.
4. Interleaved before/after radv timings of those targets, before = the same binaries with
   `SPACE_SCUFFLE_GPU_TEST_DISPLAY=xvfb` (exactly the pre-WP3 path), in the order xvfb r1, kwin r1, kwin r2, xvfb r2,
   with `~/scratch/gpu-readback/wp3/bin/arm.sh <xvfb|kwin> <round>`, summarised in Amendment 1's ledger format.
5. The verifier's `cargo gate package` (Q0061).

### Amendment 2, radv results (2026-11-17, after the RX 580 returned)

Radv POLARIS10, Mesa 25.0.7, built from `63566e1b`; the build after the server's restart was clean. Logs are in
`~/scratch/gpu-readback/wp3/radv/`.

1. **`gpu_validation_tests` on radv** (`--test-threads=1`, as the gate runs it): 46 passed, 0 failed, 2 ignored
   (the lock children). `a_failing_kwin_session_fails_the_run`: the exit-3 session reaches the verdict in 0.97 s.
   `a_timed_out_kwin_session_leaves_nothing_running` passes in its final form: `timeout` ends the session at 20.17 s
   (exit 124), and no sampled process, KWin and Xwayland included, remains. The validated runs under KWin find
   nothing: `client --battle` 24.2 s, `--skirmish` 21.3 s, `--lab` 19.2 s, and the best-practices control 19.3 s.
   KWin's log lines about missing decoration and overview plugins are harmless and are not validation findings.
2. **T-1 and T-2 on radv** (`capture_path_gpu_tests`): both pass. T-1: the window child (forced onto Xvfb) against
   the offscreen child, 7 captures compared, 0 differ. T-2: the same window child forced onto Xvfb and forced onto
   KWin, 7 captures compared, 0 differ. F3: each effect capture changes 13,691 to 20,280 pixels of the empty ground.
   Child times: window on Xvfb 30.2 and 35.5 s, window on KWin 13.3 s, offscreen 9.9 s.
3. **T-4 on radv: stopped, one target fails under KWin.** In both KWin rounds, 18 of the 19 targets' checks pass
   under KWin (50 and 48 KWin children; the 2 Xvfb children are T-1's and T-2's forced ones). `terrain_tests`
   `test_terrain_preview_renders` fails in both KWin rounds and passes in both Xvfb rounds: `assertion left ==
   right failed: Image width must be 1600 for overview, left: 1280` (`terrain_tests.rs:1142`). Cause:
   the test builds its child with `virtual_gpu_command` and no screen, so KWin's virtual output is the default
   1280×720 and KWin fits the 1600×900 `terrain_preview` window to it. Xvfb's default 1280×1024 screen never
   resized an X window, which hid the missing screen. The fix is test-only: `virtual_gpu_command_with(...,
   Some("1600x900x24"), ...)`, as `editor_app_tests` does. It is not made here; per the stop rule, the
   orchestrator decides. `showcase_tests` failed once (KWin round 2) for a reason outside WP3: it waited 290 s for the
   GPU lock, held by the other user's `corpus` test; it passed under KWin in round 1. No `kwin_wayland`, `Xwayland`
   or `ss-gpu-*` session directory remained after any round.
4. **Interleaved timings** (`~/scratch/gpu-readback/wp3/timing/<arm>-<round>/`, order xvfb 1, kwin 1, kwin 2,
   xvfb 2; built from `63566e1b`). **The host was not quiet:** the other user ran a parallel build throughout, and
   the 1-minute load at a target's start ranged from 7 to 270 on 16 cores (recorded per target in `meta.txt`).
   Per-target wall times swing by up to 30× between rounds of the same arm (`battle_hud_window_tests` 21 s and
   608 s on Xvfb), so they show no reliable per-target difference.

   | round | targets passed | children (KWin / Xvfb / other) | wall s | children's run s | GPU-lock wait s |
   |---|---|---|---|---|---|
   | xvfb 1 | 19 / 19 | 0 / 48 / 4 | 2182 | 1472 | 278 |
   | kwin 1 | 18 / 19 | 50 / 2 / 4 | 1717 | 1400 | 32 |
   | kwin 2 | 17 / 19 | 48 / 2 / 4 | 2695 | 1125 | 65 |
   | xvfb 2 | 19 / 19 | 0 / 48 / 4 | 4316 | 1591 | 96 |

   The children's own run time, which excludes lock waits and cargo, is lower under KWin (1400 and 1125 s against
   1472 and 1591 s, about 16 % less on average). That direction matches WP1, but the load makes the size
   unreliable. A clean figure needs a rerun on a quiet host, after the terrain fix.

## Amendment 3 (orchestrator, 2026-11-18): the terrain preview's screen size; timings move to WP4

**`terrain_tests` under KWin (short note, test-only, charter §6).** `test_terrain_preview_renders` asks for a
1600×900 window. It calls `gpu_run::virtual_gpu_command` without a screen size, and only passed on Xvfb because
Xvfb never resizes X windows. KWin's default 1280×720 output shrinks the window, and the test's own check
(`terrain_tests.rs`, "Image width must be 1600") catches it.

**Fix.** The test asks for its screen explicitly, `Some("1600x900x24")`, through `virtual_gpu_command_with`, as
`editor_app_tests` does.

**Audit.** Every windowed GPU test whose window is larger than 1280×720 states its screen size. The harness then
refuses a KWin child whose requested window exceeds its output, with a message naming the missing size, instead
of letting the compositor shrink it silently. A unit test covers that refusal. No assertion or bar changes.

**Tested by.** `terrain_tests` passes on radv under KWin and under Xvfb (forced), and T-4 is rerun under KWin on
all 19 windowed targets.

**Timings.** The four interleaved rounds ran while another user's build loaded the host (load 7 to 270 on 16
cores). Per-target times varied up to 30× within one display, so they give no reliable figure. They are recorded
as they are (Amendment 2): the children's run time was about 16 % lower under KWin. The merge does not wait for a
quiet host. The display path is accepted on correctness: T-1, T-2, T-4, the KWin safety tests and bit-identical
pictures. WP4's per-gate ledger then measures the real lane time on every gate run, which, over many runs, is a
better figure than one quiet window. `showcase_tests`' one failure in KWin round 2 waited 290 s for a GPU lock
another user held. It is not a display fault, and it is recorded.

**Amendment 3, as built and tested (2026-11-18, `c3af042a`).**

- **Fix.** `test_terrain_preview_renders` builds its child with `virtual_gpu_command_on_screen(..,
  Some("1600x900x24"))`, the call `editor_app_tests` uses. The note named `virtual_gpu_command_with`, but that
  forces one display for the command; the editor's call keeps the gate's own display choice, which is what "as
  `editor_app_tests` does" asks.
- **Refusal.** `GpuCommand::window(width, height)` declares the window a child asks for. `try_run` refuses the run
  before the lock, panicking with the label and a `WindowTooLarge` message, when the window is larger than the output
  of the stated screen under KWin (`window_fits_screen`: the stated size, or 1280×720). The message names the
  `Some("<w>x<h>x24")` to state. The check applies on Xvfb too, so the lavapipe runs of CI catch a missing screen
  that only KWin would show; offscreen and host children are not checked. `terrain_tests` and `editor_app_tests`
  declare their 1600×900 windows. Limit: the harness cannot see a window size the test does not declare (Bevy logs
  none at its default level), so a new oversized window is caught only once it is declared; review checks this.
- **Unit tests** (`gpu_validation_tests`): `a_window_must_fit_the_stated_screen` (fits, each side too large,
  stated and default screens, the message) and `an_oversized_window_is_refused_before_the_run` (an inspected
  command on KWin and on Xvfb is refused with the window message ahead of every other check; a fitting window
  reaches the inspection refusal).
- **Audit.** Only two windowed children exceed 1280×720: `terrain_preview` and the editor, both 1600×900, both
  now stating their screen and declaring their window. The client's window and the effect lab's are 1280×720;
  the other canvases are smaller and offscreen.
- **Results on radv** (logs in `~/scratch/gpu-readback/wp3/a3/` and `timing/kwin-a3/`; the GPU was shared with two
  other agents). `cargo clippy -p client --tests -D warnings` is clean. `terrain_tests`: 14 of 14 pass under KWin
  (`display=kwin`) and 14 of 14 under forced Xvfb (`display=xvfb-sw`). T-4 rerun under KWin: 17 of 19 targets
  passed in the first pass (36 KWin children, 1,307 s of lock waits). The other two, `battle_result_window_tests`
  and `weapon_battle_gpu_tests`, each waited 290 s for a GPU lock another agent's `weapon_battle_gpu_tests` held,
  and never started a child. These were lock waits, not display faults. Rerun with `GPU_LOCK_TIMEOUT=2400`, both
  pass under KWin (21 KWin children, 178 s of lock waits). So all 19 windowed targets pass on radv under KWin, and
  no `kwin_wayland`, `Xwayland` or session directory remained. The timings are not rerun; they move to WP4.

**Amendment 3, review fixes (2026-11-18).** The independent review of WP3 (`29878106..b05aec6b`) accepted it with
conditions. Each finding and what it became:

1. **Settle after the run (medium).** The timeout self-test no longer waits 5 s for leftovers. It checks once,
   immediately after the run returns, that none of the processes sampled during the run (same PID and start time)
   still runs.
2. **Holes in the process search (medium).** After the run, the probe also scans every process for the run's tag
   in its environment (a re-parented or late Xwayland). It checks that `KWin`'s socket, its `.lock` and the session
   directory are gone, and that no X socket that appeared during the run remains with a lock naming a process of
   the run, or with no lock. A missing `xdpyinfo` now fails the probe on radv instead of quietly skipping Xwayland.
3. **The worst-case bound (low to medium).** `GpuCommand::try_run` now runs the child through `run_to_end`.
   `timeout` leads its own process group. When `timeout` exits, while it is still an unreaped zombie whose PID
   cannot be reused, the harness sends `SIGKILL` to the whole group. It then reads the pipes, waiting at most
   `OUTPUT_DRAIN_SECS` (5 s) for them to close, and only then releases the lock. So a group member that ignores
   `SIGTERM` or outlives `KWin` holds neither the pipe nor the lock, and a hung radv child holds the GPU at most
   135 s (120 s limit, 10 s grace, 5 s drain). Tests: `a_session_child_that_ignores_sigterm_cannot_outlive_the_run`
   (radv, `KWin`; a session program with `trap '' TERM`) and `a_leftover_group_member_cannot_hold_the_output` (any
   host; a backgrounded `sleep` that ignores `SIGTERM` holds the pipes after its parent exits 7).
4. **Stale status file (low).** `KwinSession::prepare` creates the session directory with `create_dir`, so an
   existing directory, and any stale `status` in it, refuses the run before the lock. Test:
   `a_stale_session_directory_is_refused`. `SessionFiles` now also removes the socket and socket lock a killed `KWin`
   leaves.
5. **Undeclared windows (low).** `client::render_compatibility::WindowSizeCheckPlugin`, active only under
   `SPACE_SCUFFLE_VIRTUAL_GPU`, records each window's requested size when it is spawned. From the first frame on, it
   fails the app (error exit) when the window's physical size differs from that size times its scale factor.
   `RenderCompatibilityPlugin` installs it, which covers the client, the lab, the terrain preview and the
   `client_diagnostics` tools; the map editor, which does not use that plugin, adds it itself. Unit test:
   `a_window_of_another_size_than_requested_is_a_mismatch`. The harness's refusal of a declared oversized window
   stays as the earlier, clearer message.
6. **Guide wording (low).** `docs/guides/gpu-and-disk.md` says "where KWin, Xwayland or `XDG_RUNTIME_DIR` is
   missing", and says the harness takes the lock and releases it once the child and every process it started are
   gone.
7. **Session bus (low).** A `KWin` child's command removes `DBUS_SESSION_BUS_ADDRESS`, so `KWin` never registers on
   a developer's session bus; the T-5 command test checks it.
8. **What T-2 proves (info).** T-2 compares Bevy screenshots, which copy the swapchain image Bevy rendered before
   it is presented, so before `KWin` composites it. It therefore proves that rendering is identical with radv's DRI3
   present and with its software present, which is what D-6 asks. It does not compare `KWin`'s composited output,
   which no test reads.
