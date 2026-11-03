# Q0020: May the smog solver set very slow fluid velocities to exactly zero?

- **Asked:** 2026-11-03
- **Blocking:** no
- **Default until answered:** yes. WP9 of `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md` sets every smog
  voxel whose speed is at most 0.001 world units per second to exactly zero, and re-records the smog goldens in one
  commit that explains why.

## Question

The smog simulation (`crates/client/src/smog/fluid.rs`) is the client's largest CPU cost: about 6 to 8 ms of every
frame (finding C-18 in `docs/plans/2026-11-01-codebase-remediation.md`). The plan above first speeds it up without
changing a single bit of its output: it stops copying memory it does not need to copy, and it skips regions of the
field that are provably empty.

That skipping only helps where the field is really empty, and the fluid's velocity never quite gets there. Damping
shrinks a velocity every frame but never makes it zero. Measured while planning: 30 seconds after the last unit
stopped, between 39,000 and 50,000 of the 82,944 cells still had a nonzero velocity, every one of them below 0.0001
world units per second. So after the first fight crosses a region, the solver keeps paying for that region for the
rest of the session. This is also why the frame budget measured after a fight was as slow as during it.

The proposal: when a cell's speed falls to 0.001 world units per second or less, set it to exactly zero.

- **Why it should not be visible.** The solver already treats that speed as "at rest" when it builds the texture the
  GPU draws: the packed velocity of such a cell is the same byte whether its speed is 0.001 or 0. In one frame, a
  speed of 0.001 moves smog by about 0.00005 world units, under 0.02 % of a cell. The result can differ from today's
  by at most one step of an 8-bit colour or opacity value, in rare cells.
- **What it buys.** The field returns to exactly zero about 20 seconds after the last unit leaves a region, and the
  skipping makes an empty field cost almost nothing (today an empty field costs about 5.7 ms per frame).
- **What it costs.** The smog's exact output changes, so seven bit-exact smog test hashes are re-recorded. That is
  allowed by the remediation rules when a commit explains it. It is unrelated to Q0014 (which is about the host's
  maths library) and keeps every hash in the same class.

Options:

1. **Yes (default).** Make the change after the bit-exact speed-ups, and publish the before-and-after measurement.
2. **No.** Keep only the bit-exact speed-ups. The solver stays at its fight-time cost after a fight. A later plan
   could instead run the smog at a lower fixed rate or on another thread; both change the output too, in ways that are
   easier to see (the smog would update less often than the screen, or one frame late).

## Answer

