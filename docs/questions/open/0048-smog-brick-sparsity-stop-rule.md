# Q0048: The smog brick sparsity missed its stop rule. Go on to the quiescence change anyway?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** WP9 (quiescence) does not start, as the plan's stop rule says. WP8 stays on its branch,
  finished and tested; merging it is the orchestrator's call. All other work goes on.

## Question

Finding C-18 is about the cost of the smog solver, which runs every frame. The plan
[`2026-11-04-open-findings-c17-c18-p9-gaps`](../../plans/2026-11-04-open-findings-c17-c18-p9-gaps.md) speeds it up in
two steps:

- **WP8 (done).** Skip the 8 × 4 × 8 blocks of the grid whose result cannot change. The output stays exactly the
  same.
- **WP9 (next).** Set very small fluid velocities to exactly zero, so the field can come back to rest after a fight.
  This changes the output on purpose, below what the 8-bit textures show (Q0020).

The plan made WP8 stop and report if its benchmark did not show two things: the empty field at least 5 times
faster, and the 240-source battle at least 1.3 times faster. The results are in
[`docs/evidence/2026-11-04-c18-smog-solver.md`](../../evidence/2026-11-04-c18-smog-solver.md), §6. Times are ms per
step at the median, uncontended:

| Scenario | Before | After | Change |
|---|---:|---:|---:|
| Empty battle window | 2.12 | 0.12 | 18 times faster (rule: 5) |
| 240 sources marching over the whole window | 4.16 | 3.72 | 1.12 times faster (rule: 1.3) |
| The 30 s after that fight | 4.44 | 3.17 | −29 % |
| Visual Lab showcase (10 sources, small volume) | 6.13 | 7.21 | **18 % slower** |

Why the battle gains little, and the showcase loses:

- **Battle.** The 240 sources sweep the whole window, so no block rests. The plan expected only blocks near the
  wakes to be busy.
- **After the fight.** Velocities never return to exactly zero, so the wakes keep blocks busy long after the fight.
  WP9 is the fix for this.
- **Showcase.** Every block is busy, and the bookkeeping that decides what to skip costs about 1.1 ms per step.

Options:

- **(a) Merge WP8 and go on to WP9 (recommended).** The empty and after-fight gains are real. WP9 is what lets wakes
  come back to rest, and WP9's frame-budget runs will measure both changes in the real game, which is the figure of
  record. A follow-up would then try to win back the showcase's 1.1 ms. One idea: skip the bookkeeping on steps
  where nearly every block is busy.
- **(b) Merge WP8, but first win back the showcase cost, then decide on WP9.**
- **(c) Do not merge WP8.** Keep the dense solver, and close C-18 with WP7's gains only.

## Answer

