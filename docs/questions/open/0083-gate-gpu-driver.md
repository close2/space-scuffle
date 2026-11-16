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
