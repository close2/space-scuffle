# Q0083: Should the gate's GPU tests run on software Vulkan (lavapipe) if it stays faster than the RX 580?

- **Asked:** 2026-11-16
- **Blocking:** no
- **Default until answered:** GPU tests keep running on the physical GPU (radv), as charter §7 says. Agents
  measure why radv is slower and fix it if they can.

## Question

In the WP12 package gate, the GPU test lane took about 86 of the 93 test minutes and decides how long a gate
takes. The same GPU tests run much faster on CI's 4-vCPU runner with software Vulkan (lavapipe) than here on the
RX 580:

| Test | Local radv | CI lavapipe |
|---|---|---|
| `effect_slice_gpu_tests` | 1,997 s | 937 s |
| `editor_app_tests` | 618 s | 181 s |

On radv, each test child keeps about one CPU core busy the whole time. The likely cause, not yet confirmed, is the
`MESA_VK_WSI_DEBUG=sw` path the harness uses: it copies every frame from video memory with the CPU.

Options:

1. **Keep radv, find the cause (default).** Profile one radv child, record each child's timing, and try a cheaper
   way to read frames back. Charter §7 stays as it is.
2. **Lavapipe for the gate, radv for pictures.** Gates run GPU tests on lavapipe (as CI does, and with no lock, so
   they could run in parallel). Review pictures and frame-budget measurements stay on the RX 580. This changes
   charter §7's "use the physical GPU" rule for tests.

Recommendation: option 1 first. If radv cannot get close to lavapipe, option 2 could save about an hour per gate.

## Answer

**Option 1 first (try to make `radv` on the RX 580 fast by fixing the frame-readback/WSI bottleneck), with Option 2 pre-authorized if `lavapipe` remains faster or more parallelizable for non-performance gate tests — physical GPU (`radv`) is strictly required only when actual GPU performance (such as frame-budget benchmarks) needs measuring (and for visual review captures if desired).**

**Vulkan / PCIe technical insight:** Your hypothesis about `MESA_VK_WSI_DEBUG=sw` is almost certainly the root cause of the 100% CPU core peg and 2×–3.4× slowdown on the discrete Polaris RX 580:
1. With `MESA_VK_WSI_DEBUG=sw`, Mesa's software WSI presentation path maps or reads swapchain images directly from discrete `DEVICE_LOCAL` VRAM across the PCIe bus with the CPU. On x86 PCIe, CPU loads from uncached / write-combined (`UC` / `WC`) PCIe BAR memory run at ~20–80 MB/s (stalling a CPU core on PCIe read transactions every single rendered frame), whereas `lavapipe` renders in `HOST_CACHED` L3/DDR4 RAM at ~30+ GB/s.
2. For headless Bevy tests, rendering offscreen (`RenderTarget::Image` with `WindowPlugin { primary_window: None, .. }` or headless render targets, dropping `MESA_VK_WSI_DEBUG=sw` altogether) and issuing a GPU DMA copy (`vkCmdCopyImageToBuffer`) into a **`HOST_VISIBLE | HOST_CACHED`** system-RAM staging buffer *only on the specific ticks where a test actually inspects pixels* lets the RX 580's DMA engine push frames at ~10 GB/s in `< 0.2 ms` without CPU stalls.
3. Try fixing the `radv` headless/readback path first so we get fast GPU execution; however, per the owner's rule (*"GPU is only required if actual performance needs measuring"*), if `lavapipe` is still faster or significantly speeds up wall-clock gates via lock-free parallel test execution, you are pre-authorized to run functional gate tests on `lavapipe` while reserving the physical RX 580 (`radv`) for performance/frame-budget measurements (and review renders).

## Resolution (2026-11-18)

Carried out by plan `docs/plans/2026-11-17-gpu-test-readback.md`; the measurements are in
`docs/evidence/2026-11-17-gpu-test-readback.md`.

- **Option 1 was done.** WP1 confirmed the cause: `MESA_VK_WSI_DEBUG=sw` cost about 105 ms of CPU per presented
  1280×720 frame, about 80 % of an effect-slice child. WP2 renders every child that reads only its own pictures
  offscreen, capturing through Bevy's screenshot, which already copies into `HOST_CACHED` memory only on capture
  ticks. WP3 presents every windowed radv child through a headless KWin on the same GPU. Pictures stayed
  bit-identical, and no golden was re-recorded. WP4 makes every gate record a per-child ledger with lock waits.
  Merged in `bdfcffcc` (WP2, WP3) and `f404c7d7` (WP4).
- **The radv GPU lane halved.** Serially, the 25 GPU targets took 2,190 and 2,596 s on radv on 2026-11-18, against
  4,159 s before. In package gates the GPU lane is 2,132 to 2,635 s, against 3,560 to 5,216 s before.
- **lavapipe is no longer faster, and option 2 was not adopted** (plan Amendment 6, decision rule §8). On this host,
  lavapipe took 4,867 and 4,875 s for the same targets, about twice radv's time and 8 to 9 times its CPU. Local
  radv is now also faster than CI's dedicated lavapipe on every large target (for example `effect_slice_gpu_tests`
  716 to 1,005 s against 1,004 s, `stance_gpu_tests` 69 to 93 s against 308 s). The GPU lane is still the gate's
  critical path, by 21 to 24 min. But moving the functional targets to lavapipe would make a gate about 15 min
  slower as the scheduling was planned, and could not meet §8's bar (at least 15 % and 10 min faster) even with
  CI's lavapipe speed or unlimited lanes.
- **Outcome:** every gate GPU test keeps running on radv, as charter §7 says; no charter or guide text changes. WP6
  (per-target driver) is not built. `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` still forces lavapipe, as CI does.
