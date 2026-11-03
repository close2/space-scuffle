# Q0015: What frame time should the client meet at 200 and 400 units?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** Phase 6 exits once the frame time is *measured* at 200 and 400 units, as its exit
  criterion says. The figures go into `DESIGN.md` §5 next to two reference lines, 16.7 ms (60 frames per second) and
  33.3 ms (30 frames per second). They pass or fail nothing. A figure above a reference line is published as it
  is, and a later plan can work on it.

## Question

Remediation Phase 6 (`docs/plans/2026-11-01-codebase-remediation.md`) asks for the "frame budget measured at 200
and 400 units", and the plan `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` designs that measurement. It runs
a release build on the development server's RX 580, with production settings, a 1280×720 window and vsync off, five
runs per scenario. It records the 50th, 95th and 99th percentile and the worst frame interval. The criterion does
not say what the budget *is*, so nothing can pass or fail it yet.

Options:

1. **Measure and publish only (default).** No target. The numbers are reported against the 60 and 30 frames per
   second lines.
2. **A target that the measurement must meet**, for example 95th-percentile frame interval ≤ 16.7 ms at 200 units
   and ≤ 33.3 ms at 400 units on the RX 580. If a target is missed, Phase 6 stays open and a performance plan
   follows.
3. **A different reference machine or resolution.** The RX 580 is the only GPU we have. It is shared, and it is
   driven through Xvfb with Mesa's software presentation path (`MESA_VK_WSI_DEBUG=sw`), because the server has no
   display. That path adds a CPU copy of every frame, so on a real desktop the figures would be somewhat better.

**Update (2026-11-03):** option 3's description of Xvfb was too mild. The software present path costs about 100 ms
per frame on this server, so the frame budget is now measured through a headless KWin compositor with hardware
present instead. The display path is asked separately as [Q0019](0019-frame-budget-display-path.md). This question's
default is unchanged.

CI cannot check any of these targets. It has only `lavapipe`, a CPU rasteriser, on shared runners. CI checks only
that the measurement tool still runs and that its scenario is valid.

## Answer

Owner (2026-11-03): Start with option 1 (measure and publish only, reported against the 60 and 30 FPS lines).
