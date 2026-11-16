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

