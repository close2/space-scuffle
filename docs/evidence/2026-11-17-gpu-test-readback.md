# Evidence: Where a GPU Test Child's Time Goes on the RX 580 (readback plan WP1)

Measurements for WP1 of plan `docs/plans/2026-11-17-gpu-test-readback.md` (owner answer to Q0083). WP1 commits only
this file (plan D-10). The offscreen prototype was a measurement-only patch, kept as
`~/scratch/readback-wp1/proto/offscreen-measure.patch` and never committed.

## Method

- **Tree:** `main` at `c4bf2f16`, branch `readback-wp1`. Debug test binaries in
  `/home/spacescuffle/cargo-target/readback-wp1`.
- **Host:** RX 580 (RADV POLARIS10), Mesa 25.0.7-2+deb13u1, LLVM 19.1.7 for lavapipe, KWin 6.3.6, vulkan-tools
  1.4.304, perf 6.12.117, 16 vCPUs.
- **Locks:** every radv command ran in its own `with-gpu-lock`; M1 ran under `flock ~/scratch/gate.lock`, and its
  GPU children took the lock themselves through the harness.
- **Load:** another user's builds ran during the whole session. The one-minute load average was 9 to 310 on
  16 vCPUs; every run's `date -u`, `/proc/loadavg` and `anon` line are kept next to its log. Absolute times are
  therefore noisy. Each comparison below is between arms run back to back, interleaved, and the conclusions hold in
  every round.
- **Raw data:** `~/scratch/readback-wp1/` (`m1-radv/`, `m1-lavapipe/`, `m2/`, `m3/`, `m4/`, `m5/`, `pk/`, scripts in
  `bin/`). The plan named `~/scratch/gpu-readback/<date>/`; the directory was renamed after the branch. These are
  scratch files that may be cleaned at any time, so the tables below are the record.

## Findings

1. **H1 is confirmed.** The software present path of `MESA_VK_WSI_DEBUG=sw` costs about 105 ms of CPU per
   presented 1280×720 frame. In the profiled effect-slice stills child (33.1 s), Mesa's WSI thread used about 24.5 s
   of CPU and Xvfb about 5 s. The same child takes about 6 s offscreen. The present path is about 80 % of that
   child, far above D-10's 25 % threshold.
2. **Mechanism.** The WSI thread sends each frame to Xvfb with `writev` on the X socket (`XPutImage`, not
   `XShmPutImage`: the profile shows the frame going through `unix_stream_sendmsg`). The kernel's `copy_from_iter`
   reads the mapped swapchain image at about 35 MB/s (3.7 MB in about 105 ms), which is consistent with uncached CPU
   reads of GPU-written memory. Plan §2.2's **verify** (VRAM BAR or write-combined GTT) stays open, because the
   profile does not name the memory type; it does not matter, because both chosen paths stop the CPU from reading
   each frame.
3. **Offscreen pictures are bit-identical** to today's window capture, on radv and on lavapipe, with a
   `Bgra8UnormSrgb` canvas: the PNG files are byte-identical in all three rounds (D-3, D-6).
4. **Headless KWin removes the present cost** for windowed children (0.3 to 1 ms per frame immediate, 17 to 19 ms
   FIFO), and the effect-slice child under KWin gives the same bit-identical pictures.
5. **P-K holds:** all six D-4 requirements are met by the session variant (`--exit-with-session` with a status shim).
6. **The readback buffer is already in cached host memory** (D-2 stands): gpu-alloc gives a `MAP_READ` buffer memory
   type 5 on radv, `HOST_VISIBLE | HOST_COHERENT | HOST_CACHED` in system memory.
7. **New for WP3:** the pictures of two windowed children depend on frame timing, so a KWin-against-Xvfb comparison
   cannot be bit-identical for them (§M4).

## M5. Memory types and formats

`vulkaninfo` (radv under `with-gpu-lock`, lavapipe without):

| Driver | Type | Heap | Flags |
|---|---|---|---|
| radv | 0, 1 | 0: 3.75 GiB VRAM | `DEVICE_LOCAL` |
| radv | 2 | 1: 70.8 GiB system | `HOST_VISIBLE | HOST_COHERENT` |
| radv | 3, 4 | 2: 256 MiB VRAM BAR | `DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT` |
| radv | 5, 6 | 1: system | `HOST_VISIBLE | HOST_COHERENT | HOST_CACHED` |
| lavapipe | 0 | 0: 141.6 GiB | `DEVICE_LOCAL | HOST_VISIBLE | HOST_COHERENT | HOST_CACHED` |

- wgpu-hal 23.0.1 asks for `HOST_ACCESS | DOWNLOAD` for a `MAP_READ` buffer. gpu-alloc 0.6.2's `reverse_priority`
  scores type 5 at 0 (best), type 2 at 2 and types 3 and 4 at 10, so a screenshot buffer is cached system memory on
  radv. On lavapipe every allocation is cached host memory.
- The BAR is 256 MiB (no resizable BAR), so the GPU cannot expose its VRAM to the CPU cheaply anyway.
- **Surface format (D-3):** `wgpu_core::device::global` logs the surface configuration. Bevy configures
  `Bgra8UnormSrgb` with `present_mode: Fifo` (from `AutoVsync`) for the effect-slice child on Xvfb with radv, Xvfb
  with lavapipe and KWin with radv, and for the windowed HUD child on Xvfb and KWin. **The canvas uses
  `Bgra8UnormSrgb`.**
- **Runner (D-1 verify):** with `bevy_window` enabled, `DefaultPlugins` does not contain `ScheduleRunnerPlugin`
  (`bevy_internal` 0.15.3, `default_plugins.rs`), so a windowless app needs `ScheduleRunnerPlugin::run_loop`, as D-1
  says. The prototype used `run_loop(Duration::ZERO)`.

## M3. Presentation alone (`vkcube`, 1280×720)

300 frames per arm (5,000 for KWin immediate), each arm in its own lock. Two passes, at loads 27 to 87 and 33 to 145.

| Display path | `MESA_VK_WSI_DEBUG` | Present mode | ms per frame (pass a / b) |
|---|---|---|---:|
| Xvfb, radv | `sw` | immediate | 117 / 105 |
| Xvfb, radv | `sw` | FIFO | 93 / 106 |
| Xvfb, radv | `sw,noshm` | immediate | 243 / 155 |
| Xvfb, radv | `sw,linear` | immediate | 223 / 185 |
| Xvfb, radv | `sw,buffer` | immediate | 144 / 171 |
| Xvfb, radv | `buffer` or `noshm` without `sw` | immediate | fails: no DRI3 (exit 134) |
| Xvfb, radv, 640×360 | `sw` | immediate | 23 / 30 |
| KWin through Xwayland, radv | none | immediate | 0.37 / 1.05 |
| KWin through Xwayland, radv | none | FIFO | 17.6 / 18.9 |
| Xvfb, lavapipe | none | immediate | 11.4 / 7.8 |
| Xvfb, lavapipe | none | FIFO | 5.0 / 5.0 |

- Mesa 25.0.7's `MESA_VK_WSI_DEBUG` flags (from the driver's option table): `buffer`, `sw`, `noshm`, `linear`,
  `dxgi` (Windows only) and `nowlts` (Wayland only), plus `MESA_VK_WSI_PRESENT_MODE`. No flag makes the software
  path cheap; every other combination is slower or fails.
- The cost scales with the pixel count (640×360 is about a quarter), not with what is drawn.
- **KWin start and stop** (a session that runs `/bin/true`, 10 runs, each in its own lock): 2.5 s for the first,
  then 0.6 to 1.2 s (median 0.75 s). D-4's 3 s bound holds.

## M2. Profiles (`perf record -F 997 -g`)

Each child as the harness runs it, `xvfb-run` and the child followed by `perf`. Loads 76 to 144.

**Effect-slice stills child, subject 0, radv, Xvfb with `sw` (33.1 s):**

| Thread | Share of cycles | Samples (≈ CPU s at 997 Hz) |
|---|---:|---:|
| `WSI swapchain queue` (Mesa) | 65.9 % | 24,452 (24.5 s) |
| main thread (the child) | 14.5 % | 7,595 |
| `Compute Task Pool` (Bevy) | 12.9 % | 10,020 |
| `Xvfb` | 4.9 % | 4,950 (5.0 s) |

- Top symbol: `rep_movs_alternative` in the kernel, 64.5 % of all cycles, on the WSI thread:
  `writev` → `sock_write_iter` → `unix_stream_sendmsg` → `skb_copy_datagram_from_iter` → `copy_page_from_iter` →
  `_copy_from_iter`. That is the X socket write of each frame's image, read from the mapped swapchain image.
- Converted: about 24.5 s on the WSI thread at about 105 ms per frame (M3) is about 230 to 260 presented frames;
  the Mesa overlay layer counted about 370 presented frames in a full run (M4), whose frames also wait for Xvfb.
- **Share of the child:** the WSI copy and Xvfb together are about 29.5 s of a 33.1 s child.

**Windowed HUD child, radv, Xvfb with `sw` (13.6 s):** the WSI thread has 1.7 % of cycles. The child is bound by
start-up work on the `Compute Task Pool` (83 %: PNG decoding of the shipped atlases, smog grid set-up, terrain
albedo down-sampling). It presents only a few frames (the overlay layer counted about 2), so a display change gains it
nothing.

**Effect-slice stills child, lavapipe, Xvfb (34.7 s):** 16 `llvmpipe-N` rasteriser threads with about 6 % of
cycles each; 68 % of all cycles are in JIT-compiled shader code and 22 % in `libvulkan_lvp.so`. The present copy is
not visible. lavapipe's child is bound by rasterising, which no display change helps.

## M4. Real children: today's path against KWin and offscreen

Child wall time in seconds, from a shim inside the display (so KWin's start-up is excluded; it is about 0.5 s
more), three interleaved rounds. Round 3 ran at load 36 to 168 and is slower for every arm.

| Child | Xvfb + `sw` (today) | KWin, no `sw` | Offscreen (prototype) | Pictures against today's path |
|---|---|---|---|---|
| Effect-slice stills, subject 0, radv | 38.5, 31.7, 46.6 | 7.5, 6.8, 12.5 | 6.3, 5.9, 13.7 | Bit-identical (8 of 8 PNGs, every round, both paths) |
| The same, lavapipe | 30.0, 28.1, 51.0 | — | 24.7, 26.8, 52.0 | Bit-identical (8 of 8, every round) |
| `windowed_hud_child` (1280×720) | 12.9, 22.3, 20.2 | 13.5, 13.1, 33.4 | — | Report identical (1280×720 at scale 1.0, viewport 1280×512) |
| `status_pose_child` (validated) | 61.8, 54.8, 82.0 | 23.6, 25.2, 36.8 | — | Not bit-identical: see below |
| `map_editor --verify-render` (1600×900, validated) | 31.5, 32.3, 48.0 | 15.4, 21.8, 29.7 | — | Not reproducible on either path: see below |

**Frame interval** (Mesa overlay layer, `no_display`, one extra run per arm; it counts presented frames only):

| Child | Xvfb + `sw` | KWin |
|---|---|---|
| Effect-slice stills | about 370 frames at 5.1 fps | about 460 frames at 46 fps |
| `status_pose_child` | about 450 frames at 3.9 fps | about 570 frames at 20 fps |
| `map_editor --verify-render` | about 140 frames at 3.7 fps | about 180 frames at 25 fps |

**Pictures that depend on frame timing (new finding for WP3):**

- `status_pose_child`: two runs on today's path differ by up to 2/255 on up to about 550 pixels of 921,600 in 8 of
  10 poses; KWin runs are bit-identical to each other; KWin against Xvfb differs by up to 6/255 on up to 2,315
  pixels (0.25 %). The poses advance on `TimeUpdateStrategy::ManualDuration`, so some state still follows the real
  frame interval.
- `map_editor --verify-render`: two runs on today's path already differ (up to 152/255 on 25,000 pixels); KWin runs
  differ from each other and from Xvfb on most of the frame (the camera lands a few pixels elsewhere). The editor
  camera eases with `time.delta_secs()` (`crates/client/src/editor/camera.rs`), and verification settles for a fixed
  number of frames, so the pose depends on the frame interval.
- Consequence: D-6's "bit-identical" holds for offscreen children; for windowed children it holds only where the
  child is reproducible on today's path. T-2 must use a child that is bit-reproducible run to run on Xvfb first
  (WP3 checks the planned `concealment_gpu_tests` child before relying on it), and the editor's comparison stays
  with its existing structural checks. This is WP3's frame-interval audit (D-5), found early.

## P-K. KWin harness requirements (D-4 items 1 to 6)

KWin 6.3.6, `kwin_wayland --virtual --xwayland --no-lockscreen --no-global-shortcuts --width 1280 --height 720
--socket <name> --exit-with-session "<shim> <program>"`, each in its own `with-gpu-lock`, `XDG_RUNTIME_DIR` set.

| # | Requirement | Probe | Result |
|---|---|---|---|
| 1 | Exit status reaches the harness | session exits 3 | Holds: the shim's status file says 3, and KWin 6.3.6 itself exits 3 (unlike the §5.3 probes of plan `2026-11-03-close-phase-6-7-8-exits`). The status file stays the source of truth |
| 2 | Whole stdout/stderr captured | session writes 1 MiB to stderr, then a marker | Holds: all 16,384 lines and the marker in KWin's output |
| 3 | Timeout leaves nothing | `timeout --kill-after=10 10` around a session that sleeps 60 s | Holds: exit 124 after 12.0 s; no `kwin_wayland`, `Xwayland` or `sleep` left. With a 5 s grace at load 87 KWin needed `SIGKILL` (exit 137) and also left nothing but a zombie reaped within 2 s; keep the harness's 10 s grace |
| 4 | Concurrent runs use distinct sockets | two sessions at once | Holds: `WAYLAND_DISPLAY` `sspk-c1` and `sspk-c2`, `DISPLAY` `:1` and `:0` |
| 5 | Start and stop at most 3 s | 10 sessions of `/bin/true` | Holds: 0.6 to 1.2 s, first 2.5 s |
| 6 | Environment reaches the program | session prints `env` | Holds: a probe variable, `VK_ICD_FILENAMES`, `WGPU_BACKEND` and `XDG_RUNTIME_DIR` arrive; KWin sets `DISPLAY` and `WAYLAND_DISPLAY` |

**Chosen D-4 variant:** the session variant with the status shim. The fallback (`--xwayland-display`) is not needed.

## M1. Per-child ledger, both drivers

Every GPU target of `crates/gate/suites.ron` (24), one after another, each as the gate runs it: `cargo test -p
<crate> --test <target> -- --test-threads=<its cap>`, with `SPACE_SCUFFLE_GPU_RUN_LEDGER` and
`SPACE_SCUFFLE_GPU_RUN_LOG_DIR` set and `SPACE_SCUFFLE_GPU_TEST_DRIVER` forced. Every target passed on both drivers.
Phases are sums of the harness's `ChildTimeline`; "after last record" is the time after a child's last timestamped
log record, where most children render and capture without logging, so the phases split little beyond launch. Each
child is attributed to its target by its log's label and start time.

**radv** (Xvfb with `sw`, 2026-11-17 03:24 to 04:34 UTC, load 12 to 310):

| Target | Wall s | Children | Sum run_s | Max run_s | Sum preload_s | Sum lock_wait_s | Sum launch | Sum render device | Sum app build | Sum startup and frames | Sum after last record | Exit≠0 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `client/battle_hud_window_tests` | 21 | 1 | 20 | 19.5 | 0.0 | 0 | 1 | 0 | 0 | 18 | 0 | 0 |
| `client/battle_result_window_tests` | 22 | 1 | 21 | 21.4 | 0.0 | 0 | 0 | 0 | 0 | 20 | 0 | 0 |
| `client/combat_fx_slot_gpu_tests` | 4 | 1 | 3 | 2.9 | 0.0 | 0 | 0 | 0 | 0 | 0 | 2 | 0 |
| `client/concealment_gpu_tests` | 22 | 1 | 21 | 21.0 | 0.0 | 0 | 0 | 0 | 0 | 0 | 20 | 0 |
| `client/mode_atlas_gpu_tests` | 30 | 1 | 30 | 29.6 | 0.0 | 0 | 0 | 0 | 0 | 0 | 29 | 0 |
| `client/dome_fill_tests` | 30 | 1 | 29 | 28.8 | 0.0 | 0 | 0 | 0 | 0 | 0 | 28 | 0 |
| `client/editor_app_tests` | 215 | 1 | 53 | 53.1 | 0.0 | 0 | 1 | 0 | 0 | 0 | 52 | 0 |
| `client/effect_gpu_tests` | 21 | 1 | 8 | 8.1 | 0.0 | 0 | 1 | 0 | 0 | 0 | 7 | 0 |
| `client/effect_slice_gpu_tests` | 2278 | 92 | 1866 | 50.2 | 2.0 | 253 | 64 | 11 | 0 | 26 | 1766 | 0 |
| `client/flipbook_clock_gpu_tests` | 9 | 1 | 7 | 6.6 | 0.0 | 0 | 0 | 0 | 0 | 0 | 6 | 0 |
| `client/gpu_validation_tests` | 104 | 6 | 101 | 31.6 | 0.1 | 0 | 2 | 1 | 0 | 33 | 65 | 0 |
| `client/lab_scene_tests` | 38 | 1 | 28 | 28.4 | 0.1 | 0 | 1 | 0 | 0 | 0 | 26 | 0 |
| `client/round2_visual_tests` | 31 | 1 | 29 | 29.4 | 0.1 | 0 | 0 | 0 | 0 | 0 | 28 | 0 |
| `client/runtime_battle_tests` | 43 | 1 | 20 | 19.9 | 0.2 | 0 | 0 | 0 | 0 | 19 | 0 | 0 |
| `client/skirmish_match_tests` | 141 | 1 | 37 | 36.8 | 0.0 | 0 | 1 | 0 | 0 | 0 | 36 | 0 |
| `client/smog_bouncing_bubbles_tests` | 78 | 1 | 18 | 18.2 | 0.0 | 0 | 0 | 0 | 0 | 18 | 0 | 0 |
| `client/stance_gpu_tests` | 160 | 3 | 158 | 73.1 | 0.1 | 0 | 1 | 0 | 0 | 1 | 156 | 0 |
| `client/status_gpu_tests` | 149 | 3 | 149 | 69.1 | 0.1 | 0 | 1 | 0 | 0 | 1 | 148 | 0 |
| `client/weapon_battle_gpu_tests` | 542 | 20 | 530 | 36.8 | 0.6 | 2 | 4 | 2 | 0 | 4 | 520 | 0 |
| `client/area_link_gpu_tests` | 122 | 3 | 109 | 37.9 | 0.1 | 0 | 1 | 0 | 0 | 1 | 108 | 0 |
| `client/terrain_tests` | 20 | 1 | 18 | 17.9 | 0.0 | 0 | 0 | 0 | 0 | 0 | 17 | 0 |
| `client_diagnostics/frame_budget_tests` | 27 | 1 | 27 | 26.9 | 0.1 | 0 | 1 | 0 | 0 | 1 | 25 | 0 |
| `client_diagnostics/showcase_tests` | 40 | 2 | 39 | 24.2 | 0.1 | 0 | 1 | 0 | 1 | 37 | 0 | 0 |
| `client_diagnostics/lib` | 12 | 2 | 11 | 10.4 | 0.1 | 1 | 0 | 0 | 0 | 10 | 0 | 0 |
| **Total** | 4159 | 147 | 3332 | | | | | | | | |

**lavapipe** (Xvfb, 04:35 to 06:00 UTC, load 15 to 196; Mesa 25.0.7, so the harness applied the Q0009 terrain
anisotropy override, which the children's logs confirm):

| Target | Wall s | Children | Sum run_s | Max run_s | Sum preload_s | Sum lock_wait_s | Sum launch | Sum render device | Sum app build | Sum startup and frames | Sum after last record | Exit≠0 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `client/battle_hud_window_tests` | 19 | 1 | 19 | 19.0 | 0.0 | 0 | 0 | 0 | 0 | 17 | 1 | 0 |
| `client/battle_result_window_tests` | 29 | 1 | 28 | 28.3 | 0.0 | 0 | 1 | 0 | 0 | 26 | 1 | 0 |
| `client/combat_fx_slot_gpu_tests` | 2 | 1 | 2 | 1.7 | 0.0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |
| `client/concealment_gpu_tests` | 34 | 1 | 33 | 33.5 | 0.0 | 0 | 0 | 0 | 0 | 6 | 27 | 0 |
| `client/mode_atlas_gpu_tests` | 49 | 1 | 47 | 47.2 | 0.0 | 0 | 1 | 0 | 0 | 8 | 38 | 0 |
| `client/dome_fill_tests` | 40 | 1 | 38 | 38.3 | 0.1 | 0 | 0 | 0 | 0 | 0 | 37 | 0 |
| `client/editor_app_tests` | 209 | 1 | 65 | 65.0 | 0.0 | 0 | 0 | 0 | 0 | 5 | 60 | 0 |
| `client/effect_gpu_tests` | 14 | 1 | 12 | 11.6 | 0.0 | 0 | 1 | 0 | 0 | 0 | 10 | 0 |
| `client/effect_slice_gpu_tests` | 2133 | 92 | 1966 | 74.6 | 2.1 | 0 | 96 | 13 | 0 | 28 | 1832 | 0 |
| `client/flipbook_clock_gpu_tests` | 11 | 1 | 10 | 10.0 | 0.0 | 0 | 0 | 0 | 0 | 0 | 10 | 0 |
| `client/gpu_validation_tests` | 111 | 6 | 109 | 33.2 | 0.1 | 0 | 2 | 0 | 0 | 42 | 65 | 0 |
| `client/lab_scene_tests` | 107 | 1 | 99 | 99.1 | 0.1 | 0 | 0 | 0 | 0 | 0 | 98 | 0 |
| `client/round2_visual_tests` | 39 | 1 | 38 | 37.6 | 0.0 | 0 | 1 | 0 | 0 | 1 | 36 | 0 |
| `client/runtime_battle_tests` | 40 | 1 | 27 | 26.6 | 0.3 | 0 | 0 | 0 | 0 | 24 | 1 | 0 |
| `client/skirmish_match_tests` | 156 | 1 | 58 | 57.6 | 0.1 | 0 | 1 | 0 | 0 | 9 | 48 | 0 |
| `client/smog_bouncing_bubbles_tests` | 90 | 1 | 42 | 41.8 | 0.1 | 0 | 1 | 1 | 0 | 38 | 1 | 0 |
| `client/stance_gpu_tests` | 283 | 3 | 282 | 122.9 | 0.1 | 0 | 2 | 0 | 0 | 18 | 261 | 0 |
| `client/status_gpu_tests` | 216 | 3 | 215 | 99.2 | 0.1 | 0 | 3 | 0 | 0 | 18 | 194 | 0 |
| `client/weapon_battle_gpu_tests` | 1102 | 20 | 1092 | 70.2 | 0.7 | 0 | 18 | 4 | 0 | 150 | 919 | 0 |
| `client/area_link_gpu_tests` | 234 | 3 | 218 | 84.0 | 0.1 | 0 | 2 | 0 | 0 | 24 | 192 | 0 |
| `client/terrain_tests` | 17 | 1 | 15 | 15.4 | 0.0 | 0 | 0 | 0 | 0 | 0 | 15 | 0 |
| `client_diagnostics/frame_budget_tests` | 44 | 1 | 44 | 43.7 | 0.1 | 0 | 0 | 0 | 0 | 0 | 43 | 0 |
| `client_diagnostics/showcase_tests` | 63 | 2 | 62 | 31.8 | 0.0 | 0 | 2 | 0 | 1 | 56 | 3 | 0 |
| `client_diagnostics/lib` | 17 | 2 | 18 | 16.8 | 0.1 | 0 | 0 | 0 | 0 | 16 | 1 | 0 |
| **Total** | 5059 | 147 | 4537 | | | | | | | | |

- **Lane:** 4,159 s on radv here, against about 5,150 s in the WP12 package gate (which ran beside two general
  lanes). `effect_slice_gpu_tests` is 55 % of it (2,278 s; 92 children, 1,866 s of child time). Its 253 s of lock
  wait is the binary's own children queuing behind each other, not another user (`/run/lock/gpu-lock.log` shows only
  this session).
- **radv against lavapipe, same host and load:** lavapipe took 5,059 s in total. It was about as fast for the slice
  (2,133 s) and twice as slow for the production-skirmish children (`weapon_battle_gpu_tests` 1,102 s against
  542 s, `stance` 283 against 160, `status` 216 against 149). The faster lavapipe figures of Q0083 come from CI's
  dedicated runner, not from this host under load.
- **Present-bound children:** the effect-lab children (`effect_slice`, `effect_gpu`) and the production-skirmish
  look children. Short windowed children that present only a few frames (`battle_hud_window_tests`, the result
  window) are bound by start-up asset work (§M2) and gain little from any display change.

## Decisions for WP2 and WP3

| Plan item | Outcome |
|---|---|
| H1 (D-10) | Confirmed: the software present is about 80 % of the effect-slice child on radv |
| D-2 readback | Stands: `MAP_READ` buffers are `HOST_CACHED` system memory on radv |
| D-3 canvas format | `Bgra8UnormSrgb`; offscreen PNGs byte-identical on radv and lavapipe |
| D-4 KWin | P-K holds; the session variant with the status shim |
| D-5 no vsync | Not needed now: no windowed child reaches KWin's 60 Hz (20 to 25 fps measured); the slice children, which would (46 fps), go offscreen. WP3 still audits frame-interval dependence (status, editor) |
| D-6 equivalence | Holds for offscreen children; for windowed children only where today's path is itself reproducible (§M4) |
| Expected lane | About 1,700 to 2,450 s (28 to 41 min) on radv, against 4,159 s measured here (estimate). The slice's children
shrink to 0.16 to 0.3 of their time offscreen (§M4), which saves about 1,550 to 1,750 s with their lock waits; the
windowed production children (about 1,400 s of child time) shrink to 0.4 to 0.9 under KWin (status 0.42, editor 0.5
to 0.7, HUD 1.0), which saves about 140 to 700 s. WP5 measures it |
| §7 text | Only `docs/guides/gpu-and-disk.md` names the display path since Q0085; `AGENTS.md` §7 does not change. The text names `crates/client/tests/support/gpu_run.rs`, `kwin_wayland --virtual --xwayland` and the Xvfb fallback. Drafted as Q0086 |
