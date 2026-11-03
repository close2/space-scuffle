# Q0019: Is a headless KWin compositor on the RX 580 acceptable as the display path for the frame budget?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** (1). The frame budget of record is measured on the RX 580 through a headless KWin
  compositor (`kwin_wayland --virtual --xwayland`, already installed), and every published figure names that path.
  Nothing is installed.

## Question

Remediation Phase 6 asks for the client's frame time at 200 and 400 units. The development server has no monitor.
The plan `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` first measured through Xvfb, which cannot present a
GPU image directly. Mesa then copies every frame from the GPU through the CPU into Xvfb
(`MESA_VK_WSI_DEBUG=sw`). The planning probes (§5.3 of that plan) show that this copy costs about 100 ms per
1280×720 frame on this VM, whatever is drawn. A frame with nothing on the field took 25 to 109 ms that way,
depending on the server's load. Those figures describe the VM, not the game.

A better path is already installed. KWin, the KDE compositor, can run with a virtual output and no screen. The
client connects to KWin's Xwayland and presents through the normal hardware path, so the image never leaves the GPU,
and KWin composites it with OpenGL on the same RX 580. Through it, the same build measured about 12 ms at 0 units,
16 ms at 200 and 16 ms at 400 (single runs, median frame interval). It also showed that the client is limited by
the CPU, not the GPU, and that the smog simulation costs about 8.5 ms of every frame.

Options:

1. **Headless KWin is the reference (default).** The figures are published as "RX 580 through a headless KWin
   compositor, hardware present, 1280×720, vsync off; no monitor". The remaining difference from a desktop is that
   KWin composites at 60 Hz on the same GPU and nothing is scanned out to a screen. The planning probes suggest
   this costs little, because the GPU is mostly idle (about 3 ms of instrumented GPU work in a 16 ms frame).
2. **Also measure on a real desktop.** The owner runs the same tool on a machine with a monitor
   (`frame_budget --display-path desktop`, release build, the commands are in the plan's WP6). The results are
   recorded next to the KWin figures. Phase 6 still exits on the KWin figures, unless you say it should wait.
3. **Install a different headless path.** For example `weston` with its headless backend, or an Xorg with a dummy
   driver. Both are new packages, and nothing suggests they would measure differently from KWin. Not recommended.

The Xvfb figures stay in the evidence under a heading that says they are not the game's frame time, because they
explain the earlier numbers. The frame-time target itself is a separate question,
[Q0015](0015-client-frame-time-target.md).

## Answer

Owner (2026-11-03): Agreed (Option 1: headless KWin compositor on the RX 580 is the reference display path).
