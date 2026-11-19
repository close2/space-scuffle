# Q0090: What next for the splash stream's frame cost (B-137), now that the build work is done?

- **Asked:** 2026-11-19
- **Blocking:** no
- **Default until answered:** no further work. B-137 stays open with the measured residual, and the plan
  `docs/plans/2026-11-19-retained-effect-instances.md` ends at its decision gate.

## Where it stands

The plan cut the CPU work that builds the effect frame by 55 % (stream S, 10.1 M → 4.5 M instructions per frame) and
51 % (stream P). Its last step moved the lasting ground marks to a GPU compute pass that is bit-identical to the old
renderer on both drivers. The plan's acceptance bar is still missed:

- A stream of 200 splashes a second still adds about 3.7 ms of CPU per frame (P: 5.2 ms) to the 400-unit battle,
  against a bar of 0.70 ms (+5 % of a 14.0 ms frame).
- Only about 1.2 to 1.5 ms of that is the effect code itself. The rest lies outside it: Bevy's render world, the
  driver and the kernel, which grow with the number of drawn instances.
- The measurements come from a host loaded by another user. The per-pair spread (−4.3 to +10.5 ms) is wider than the
  bar, so a quiet-machine series could still move the figure.

Evidence: `docs/evidence/effects/review/frame_budget_b137_retained.md`.

## Options

- **(a)** Accept the residual. B-137 stays open as a recorded cost, with a quiet-machine series taken when the host
  is idle (the default).
- **(b)** Change budgets in data: fewer lasting marks or shorter lives per splash look. The looks change, so each
  change goes through the independent effects reviews (Q0080) and needs your sign-off on the result.
- **(c)** Keep the looks and plan the next engineering step, retained cards (the plan's third option). It is a
  similar-sized package to the decal work, and it reduces the effect code's share, not the render-world share.
- **(d)** Relax the §10.3 bar for this stress case, for example +10 % at p50.
