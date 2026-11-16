# Q0082: May a gate count a flake that passes alone as green (with the flake recorded)?

- **Asked:** 2026-11-16
- **Blocking:** no
- **Default until answered:** the rule stays as it is ("a flake is not green", tiered-gates blueprint §3.5). Agents
  add `cargo gate --resume`, so a red run repeats only the targets that did not pass on the same tree, and fix the
  known flaky GPU tests.

## Question

A setup evaluation (2026-11-16, report `~/scratch/setup-evaluation-2026-11-16.md`, item 3) measured what flakes
cost:

- **CI:** 5 of the last 30 runs hit the 150-minute timeout. In 4 of the last 5 cancelled runs, the run was red only
  because one GPU test flaked (`gpu_validation_tests` three times, `concealment_gpu_tests` once). The whole run was
  then repeated and timed out.
- **Locally:** the WP12 package gate (about 160 minutes) was red only because `stance_gpu_tests` stalled once and
  passed on its immediate rerun (B-127). The tree has no green stamp, so the whole gate has to run again.

Options:

1. **Keep the rule, cut the repeat cost (default).** A flake stays red. `cargo gate --resume` reuses every result
   that passed on the same tree, so the second attempt runs only the failed targets. The flaky tests get fixed
   (`gpu_validation_tests` on lavapipe already breaks the "two flakes in a week" rule).
2. **Green with a recorded flake.** A target that fails, then passes when rerun alone, counts as green. The stamp
   records the flake, and the register gets an entry; a second flake of the same target within a week still makes
   it a bug to fix. This saves the most time but relaxes your rule.

Recommendation: option 1 now. Option 2 only if flakes keep costing whole runs after the known ones are fixed.

## Answer

**Option 1 (Keep "a flake is not green", add `cargo gate --resume` to reuse passing targets on the same git tree, and fix the known flaky GPU tests).**

**Technical insight:** Keeping a flake red prevents flaky tests from accumulating silently, while `cargo gate --resume` eliminates the multi-hour penalty of re-running already-green targets on the same tree. Note also that the VM (`10.0.120.19`) currently has Debian trixie's `libvulkan1` `1.4.309.0-1`, which has a known upstream Khronos `Vulkan-Loader` use-after-free race between `loader_get_icd_and_device` and concurrent `vkCreateDevice`/`vkDestroyDevice` (`KhronosGroup/Vulkan-Loader` #2018 / PR #1866, fixed in `v1.4.345`), which may also contribute to intermittent multi-instance/multi-device Vulkan test stalls or crashes.

## Resolution

Option 1 carried out, merged in `f81e75e3`:
- **The rule stays.** A flake is not green.
- **`cargo gate <tier> --resume`** reuses only the results that passed on the same tree. The run must also have the same `rustc -V`, `suites.ron`, `RUSTFLAGS` and test driver, and its tree must stay clean from start to end. A target that passed only on its rerun counts as `Flaky` and is never reused. Details are in Amendment 1 of `docs/plans/2026-11-11-faster-gates.md`.
- **B-129 is fixed.** This was the `gpu_validation_tests` flake: the frame limit expired while pipelines were still compiling asynchronously.
- **B-130 stays open.** This is `concealment_gpu_tests`, which did not reproduce in 35 runs. CI now uploads a cancelled run's logs, and the `full` repeat no longer overwrites the first attempt's logs, so the next occurrence keeps its failure text.
- **B-127 stays open,** waiting for a recurrence. This is the radv stall in `stance_gpu_tests`, which was seen with the loader version that has the use-after-free race; the loader has since been updated.
