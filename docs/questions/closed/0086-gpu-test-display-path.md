# Q0086: May the GPU test guide say that test children render offscreen or present through a headless KWin?

- **Asked:** 2026-11-17
- **Blocking:** no
- **Default until answered:** WP2 (offscreen children) and WP3 (headless KWin for windowed children) of plan
  `docs/plans/2026-11-17-gpu-test-readback.md` are developed and gated on the branch `gpu-readback`, and merge to
  `main` only together with the approved text below, not before. Until then `main` keeps today's path (Xvfb with
  `MESA_VK_WSI_DEBUG=sw`) and the guide stays as it is.

## Question

`docs/guides/gpu-and-disk.md` binds like the charter (Q0085). Its bullet "GPU tests lock the GPU themselves" says the
harness runs every GPU test child "on the physical GPU (`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`)".
`AGENTS.md` §7 itself no longer names the display path, so it does not change.

WP1 measured why GPU children are slow on the RX 580 (`docs/evidence/2026-11-17-gpu-test-readback.md`):

- The software present path that `MESA_VK_WSI_DEBUG=sw` selects costs about 90 to 120 ms of CPU per presented
  1280×720 frame. Mesa's WSI thread sends every frame to Xvfb over its socket, and the kernel reads each image out of
  uncached GPU memory one byte stream at a time. In a profiled effect-lab child that was about 30 of its 33 seconds.
- The same effect-lab child takes about 6 s offscreen and about 7 s through a headless KWin, against about 32 to
  39 s today, with **bit-identical** pictures on radv and on lavapipe.
- Children that open a window (the production skirmish, the editor) need a display. Through a headless KWin
  (`kwin_wayland --virtual --xwayland`) radv presents with its normal DRI3 path, and KWin composites on the same GPU
  inside the child's lock: a status-look child went from about 55 to 62 s to about 24 s.

Plan WP2 and WP3 make the guide's sentence false, so the text must change. The proposed diff (option 1 of Q0083,
which the owner chose):

```diff
-- **GPU tests lock the GPU themselves, one test child at a time.** `crates/client/tests/support/gpu_run.rs` runs
-  each GPU test child on the physical GPU (`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`). It takes
+- **GPU tests lock the GPU themselves, one test child at a time.** `crates/client/tests/support/gpu_run.rs` runs
+  each GPU test child on the physical GPU (`radv`). A child that only reads its own pictures renders offscreen,
+  with no window and no display. A child that needs a window presents through a headless KWin
+  (`kwin_wayland --virtual --xwayland`), which composites on the same GPU inside the child's lock; where KWin is
+  missing, the harness uses `xvfb-run` with `MESA_VK_WSI_DEBUG=sw` and says so. It takes
   `/run/lock/gpu.lock`, the lock `with-gpu-lock` uses, just before the child starts, and releases it when the child
```

The rest of the bullet is unchanged. On lavapipe (CI, or `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe`) windowed children
stay on Xvfb, because KWin would composite on the physical GPU without the lock.

Option 2 of Q0083 (gate GPU tests on lavapipe) is decided later by the plan's §8 rule, after WP2 to WP4. WP1 found
that offscreen radv is about four times faster than lavapipe for the effect-lab child (6 s against 25 s), so it does
not expect option 2 to be adopted. If §8 adopts it anyway, its text comes to the owner as a follow-up question.

Options:

1. **Approve the diff above (recommended).**
2. Approve with changes: write them under Answer.
3. Keep today's path: WP2 and WP3 are not merged, and the gate's GPU lane stays at about 86 minutes.

## Answer

**Option 1 — Approved (merge the `docs/guides/gpu-and-disk.md` diff alongside WP2 and WP3).**

**Standing governance delegation on GPU operational rules:** In addition to approving this diff, note that the GPU rules in `AGENTS.md` §7 and `docs/guides/gpu-and-disk.md` exist as practical guardrails so agents do not overlook shared-VM hazards (such as forgetting `with-gpu-lock` on the physical RX 580 or accidentally picking the wrong driver) — **not** to force human approval on well-engineered test/display/driver improvements. Going forward, **agents may update the GPU operational rules in `docs/guides/gpu-and-disk.md` (and `AGENTS.md` §7 if needed, including adopting `Q0083` Option 2 or refining display/readback paths) directly without opening an owner question, provided the change is well-reasoned, preserves shared-host GPU lock safety whenever touching the physical GPU, and is documented in the plan/commit.**

## Resolution

Applied on branch `gpu-readback` (plan `docs/plans/2026-11-17-gpu-test-readback.md`, Amendment 2), to reach `main`
together with WP2 and WP3 as the default said. The bullet "GPU tests lock the GPU themselves" in
`docs/guides/gpu-and-disk.md` now says that offscreen children have no display, that windowed children present
through a headless KWin inside the child's lock and fall back to `xvfb-run` with `MESA_VK_WSI_DEBUG=sw` where KWin
is missing, and says so. Two sentences were added to the approved diff, matching what WP3 built: `lavapipe`
children stay on Xvfb (stated in the question, now in the guide), and `SPACE_SCUFFLE_GPU_TEST_DISPLAY=kwin` or
`=xvfb` forces the display and never falls back. `AGENTS.md` §7 does not change.

The standing delegation is recorded: agents may change the GPU operational rules in `docs/guides/gpu-and-disk.md`
and `AGENTS.md` §7 without an owner question when the change is well reasoned, keeps the shared lock safe whenever
the physical GPU is used, and is documented in the plan or commit.
