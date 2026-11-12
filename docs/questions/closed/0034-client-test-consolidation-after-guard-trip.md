# Q0034: The CI disk guard tripped again. Consolidate the client tests now, or free runner disk only?

- **Asked:** 2026-11-04
- **Blocking:** no
- **Default until answered:** Free preinstalled runner software before the build (plan
  `docs/plans/2026-11-04-ci-disk-guard-tripped.md`, option A). The client integration tests stay as separate
  binaries; no consolidation is planned.

## Question

In Q0013 you chose to keep the client integration tests as separate binaries "unless the disk guard trips again".
It has tripped: CI run 41737635735 at `2296113` stopped at the guard with 5.4 GiB free (12 GiB needed). An earlier
run, 41697962452 at `955d807`, tripped it too.

The cause is now clear from the last 40 runs. GitHub gives this job one of two runner classes with the same
software image: Azure machines with a 145 GiB disk (37 of 40 jobs, about 78 GiB free) and AWS machines in
`us-west-2` with a 72 GiB disk (3 of 40, about 6 GiB free). Every AWS job failed, including the original SIGBUS.
Both classes have about 66 GiB of preinstalled software, much of it unused by us (Android SDK, .NET, Haskell,
CodeQL, Swift and others).

Options:

- **A only (default, recommended).** A CI step deletes that unused software before the build. Expected gain on
  the small runner: 15 to 30 GiB, which puts it at about 25 to 30 GiB free against the 12 GiB the guard needs.
  Cost: about 1 to 2 minutes per CI run; a workflow-only change; no test changes.
- **A, then consolidate the client tests as well.** Merging the 37 test files into one binary would cut what the
  test build writes from about 8 GiB to about 1 GiB and speed up the local gate. It is a large refactor: seven
  tests rerun their own binary by test name, and GPU tests from different files would start running in parallel
  inside one process, which needs new serialisation. It would get its own plan.
- **Consolidate instead of A.** Not recommended: it costs more and fixes less quickly.

The agent reads the Q0013 trigger as "if what we build no longer fits". What we build still fits: the small
runner simply starts with too little free disk, and option A fixes that directly. That is why the default does not
consolidate. If you meant the trigger literally, answer "consolidate" and it will be planned.

## Answer

Owner (2026-11-04): Option A only for now (free preinstalled runner software before the build; keep client integration tests as separate binaries).

Owner update (2026-11-11): **Close Q0034 and plan `2026-11-04-ci-disk-guard-tripped.md` now based on the verified 30 GiB freed on the Azure runner.** Do not keep Q0034 or the plan open waiting for GitHub Actions to randomly assign an AWS `us-west-2` runner; move Q0034 to `closed/`.

## Resolution

Option A, as the owner answered (2026-11-04): CI frees preinstalled runner software before the build, and the
client integration tests stay separate binaries. The owner closed the question on 2026-11-11 on the verified Azure
figures (run 41744388382 freed 30 GiB) without waiting for an AWS `us-west-2` runner. Plan
[`2026-11-04-ci-disk-guard-tripped.md`](../../plans/2026-11-04-ci-disk-guard-tripped.md) is set to Done; its §7
remains the guide if the guard ever trips on an AWS runner.
