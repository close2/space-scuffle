# CI disk guard tripped: free preinstalled runner software before the build

Date: 2026-11-04. Status: Partly done (Azure figures recorded; waiting for an AWS run). Scope: `.github/workflows/ci.yml`, job `check-and-test`, and §8 of
[`2026-11-03-ci-disk-budget.md`](2026-11-03-ci-disk-budget.md). No Rust source, no Cargo configuration, no test
layout changes. Owner question: [Q0034](../questions/open/0034-client-test-consolidation-after-guard-trip.md).

## 1. Problem

CI run 41737635735 on `main` at `2296113`, a docs-only commit, failed in "Format, Clippy & Workspace Tests" at the
step "Disk budget before the test build":

```
/dev/mapper/cow-root   72G   67G  5.4G  93% /
##[error]5 GiB free under /home/runner/work/space-scuffle/space-scuffle; the test build needs 12 GiB
```

The guard from [`2026-11-03-ci-disk-budget.md`](2026-11-03-ci-disk-budget.md) §4.2 did its job: it stopped the
build before `rust-lld` could die with SIGBUS. The neighbouring runs passed. The log is
`~/scratch/logs/ci-2296113.log`.

## 2. Evidence: two runner classes, and the small one always fails

The job logs of the last 40 runs of the workflow were read (`~/scratch/logs/cidisk2_runs/summary.txt`, one block
per job, newest first). Each log header names the cloud, the region and the image version.

| Runner class | Jobs | Root device | Size | Used before the build | Free at the guard | Result |
|---|---|---|---|---|---|---|
| Azure (eastus, eastus2, centralus, westus, westus2, westus3, southcentralus, westcentralus, northcentralus) | 37 | `/dev/root` | 145 GiB | 64 to 67 GiB | 77 to 80 GiB | Passed the disk guard every time |
| AWS `us-west-2` | 3 | `/dev/mapper/cow-root` | 72 GiB | 66 GiB | 5.4 to 5.9 GiB | Failed every time |

The three AWS jobs are:

- 41607070459 at `87fd289` (image 20261018.96.1): the original SIGBUS, before the guard existed.
- 41697962452 at `955d807` (image 20261101.114.1): the guard tripped with 5.9 GiB free. This run was red too, and
  it was not reported.
- 41737635735 at `2296113` (image 20261018.96.1): the guard tripped with 5.4 GiB free.

So the fault is not one bad runner. About 1 job in 13 lands on the AWS class, and on that class this job cannot
pass. Both classes run the same `ubuntu-24.04` image with the same preinstalled software. They use the same 66 to
67 GiB before our build; the AWS disk is simply half the size. That is why freeing preinstalled software (option
A below) works on the small class.

What the test build needs, measured on every green Azure run since the cache grew: `target/` goes from 5.7 GiB
(restored cache) to 13 to 14 GiB, and `/` goes from 67 to 75 GiB used. **The test build writes about 8 GiB.**
The 12 GiB threshold of the guard is that figure plus about 4 GiB of margin, and it stays.

## 3. Options

Estimates for option A come from the runner image's software list (`actions/runner-images`,
`images/ubuntu/Ubuntu2404-Readme.md`, image 20261026.354.1, read 2026-11-04;
`~/scratch/logs/cidisk2_runs/Ubuntu2404-Readme-main.md`) and from the sizes that community clean-up actions
commonly report. They are not measured on our runners. The step in §4 measures each one on every run, so the
first green run after the change replaces them with real figures (§6).

| Option | Disk effect on the 72 GiB runner | Cost | Verdict |
|---|---|---|---|
| **A. Delete preinstalled software the job never uses**, before the cache restore | Expected +15 to 30 GiB (table below); +6.6 GiB is enough to pass the guard | 1 to 2 minutes of `rm -rf` per run; a workflow-only change | **Do now** |
| B. Consolidate the 37 client integration test files into one binary | About -7 GiB of test-build output (37 binaries of about 200 MiB become one of about 250 MiB) | A large refactor, and libtest would run GPU parents from different files concurrently in one process (§3.2) | Not needed after A; asked as Q0034 |
| C. Less debug info in test builds | `debug = "line-tables-only"` is already set (`Cargo.toml`, `[profile.test]`). The next step, no debug info for dependencies only, saves about 3 GiB | Bevy and wgpu frames lose `file:line` in backtraces; one full rebuild everywhere | Fallback only (§3.3) |
| D. Smaller cargo cache | Restored `target/` is 5.7 GiB, archive 1.6 GiB. The build needs those dependencies anyway, so not restoring them saves almost no peak disk | Several minutes of dependency compilation per run | Rejected |

### 3.1 Option A in detail

What the job uses: apt packages it installs itself, the `dtolnay/rust-toolchain` toolchain in `~/.rustup`, the
cargo registry in `~/.cargo`, `xvfb`, `valgrind`, `ffmpeg` and the runner's own Node.js for the actions. It uses no
Android, .NET, Haskell, Swift, Julia, CodeQL, PowerShell, Conda or browsers.

| Path | Present in the image list? | Commonly reported size |
|---|---|---|
| `/usr/local/lib/android` | Yes: SDK platforms 34 to 37.2, build-tools, two NDKs (`ANDROID_HOME`, `ANDROID_NDK_*`) | 9 to 14 GiB |
| `/usr/share/dotnet` | Yes: eleven .NET SDKs, 8.0 to 10.0 | 3 to 5 GiB |
| `/usr/local/.ghcup` and `/opt/ghc` | Yes: GHC 9.14.1 via GHCup 0.2.6.2. On 24.04 GHCup installs under `/usr/local/.ghcup`; `/opt/ghc` is the 22.04 path, kept for tolerance | 5 to 6 GiB |
| `/opt/hostedtoolcache/CodeQL` | Yes: CodeQL Action Bundle 2.27.3 | 1.5 to 5 GiB |
| `/usr/share/swift` | Yes: Swift 6.4.1 | about 2 GiB |
| `/usr/local/julia*` | Yes: Julia 1.13.2 | about 1 GiB |
| `/usr/share/miniconda` | Yes: Miniconda 26.9.4 (`CONDA`) | about 0.5 to 1 GiB |
| `/usr/local/share/powershell` | Yes: PowerShell 7.6.9 | about 0.2 GiB |
| `/usr/local/share/chromium`, `/opt/microsoft`, `/opt/google` | Browsers: Chromium, Edge, Chrome | about 1 GiB together |
| Docker images | No: the 24.04 image lists no cached Docker images | about 0; **not included**, `docker image prune` would only cost time |

Kept on purpose: `/opt/hostedtoolcache` as a whole (Python and Node there may be used by actions in future), the
preinstalled Rust in `~/.rustup` (the toolchain action installs into it), and apt package purges (slow, and they
risk removing libraries our `apt-get install` needs).

Expected sum: about 23 GiB, with a plausible range of 15 to 30 GiB. On the 72 GiB runner that gives about
25 to 30 GiB free at the guard and 17 to 22 GiB after the test build, against 5.4 GiB today.

The step runs on **every** runner, not only on small ones. The Azure runners carry the same software, so each
green run measures what every path frees. The clean-up code is then exercised 13 times as often as the AWS class
alone would, and a broken path list shows up in the next run, not weeks later. On the 145 GiB runners it costs 1
to 2 minutes of a 20 to 34 minute job, which the charter's preference for a single, always-exercised code path
justifies.

The step must come **before** "Install Linux system & virtual Vulkan GPU dependencies" and the cache restore:
the restore alone takes 7.3 GiB at its peak (1.6 GiB archive plus 5.7 GiB extracted).

**One unknown.** `/dev/mapper/cow-root` suggests a copy-on-write device-mapper layer over a base image. Deleting
files frees blocks in the filesystem, so `df` will show more space. If the copy-on-write store behind that layer
were smaller than the filesystem, writes could still fail. Nothing documents this. The "Disk report after the test
build" step already shows the answer the first time an AWS runner passes (§6).

### 3.2 Option B in detail, and why not now

Savings: the 37 files under `crates/client/tests/` each link Bevy. On CI the largest are about 214 to 219 MiB
(`skirmish_match_tests` 214 MiB, run 41736583932). One binary would replace about 7.5 GiB with about 0.3 GiB, so
the test build would write about 1 GiB instead of 8. It would also save 36 Bevy links of about 4 s each.

Costs, from the current test code:

- **Child re-exec by name.** Seven files rerun their own binary with `std::env::current_exe()` and
  `--ignored --exact --nocapture <CHILD_TEST>`: `battle_hud_window_tests`, `battle_result_window_tests`,
  `combat_fx_slot_gpu_tests`, `dome_fill_tests`, `skirmish_match_tests`, `sync_allocation_count_tests` and
  `gpu_validation_tests` (`ancestor_lock_child`), plus `gpu_run::run_test_child` (used by
  `runtime_battle_tests`). In one binary every libtest name gains a module prefix (`skirmish_match_tests::...`),
  so each constant must change and stay in step with the module tree. A wrong name fails loudly (libtest finds
  no test, and `GpuCommand::run` rejects a run with zero passed), so this is work, not a hidden risk.
- **Concurrency.** Cargo runs test binaries one after another today, so GPU parents from different files never
  overlap. In one binary, libtest runs them on parallel threads, and several lavapipe or valgrind children would
  run at once on a 4-core runner with a 300 s limit each. That needs a process-wide serialisation of GPU children,
  or `--test-threads=1` for the client tests, which makes the gate slower.
- **Isolation.** Tests that today own a process (environment variables, process-wide statics, panic hooks) would
  share one. Each must be checked.
- **Includes.** 22 files pull in `support/` modules through `mod support` or `#[path]`; these become one module
  tree, with new `dead_code` boundaries. Check C21 in `crates/workspace_checks/src/gpu.rs` also reads the client
  test files.
- **`CARGO_BIN_EXE_*`** users (`client`, `map_editor`, `terrain_preview`) keep working, since they are
  integration tests either way.

Option A alone moves the 72 GiB runner from 5.4 GiB to an expected 25 to 30 GiB free at the guard, twice the
threshold. B is not needed for the disk. Its other benefit, a faster local gate, is real, but it is a different
decision with a different cost (the concurrency rework). The owner's answer to Q0013 made B conditional on the
guard tripping again, and it has. This plan therefore asks rather than overrides: Q0034, default **A only, no
consolidation**. If the owner answers "do B", it gets its own blueprint.

### 3.3 Option C in detail

`[profile.test] debug = "line-tables-only"` and the zlib-compressed debug sections are both in place. A CI
binary is about 215 MiB, of which about 155 MiB is code, symbols and relocations, and about 60 to 80 MiB
compressed DWARF, most of it Bevy's. Setting `debug = false` for dependencies only
(`[profile.test.package."*"]`) would save about 70 MiB in each of about 40 Bevy binaries, about 3 GiB in total.
Frames inside Bevy, wgpu and the other dependencies would lose `file:line`; workspace frames keep it. It is the
fallback if A ever falls short, not part of this plan.

## 4. Changes: `.github/workflows/ci.yml`, job `check-and-test`

Insert as the first step after "Checkout repository":

```yaml
      # GitHub serves this job on two runner classes with the same image: Azure, 145 GiB root, and AWS
      # us-west-2, 72 GiB root (/dev/mapper/cow-root). Both use about 66 GiB before our build, which leaves
      # the AWS class about 6 GiB, and the test build writes about 8 GiB
      # (docs/plans/2026-11-04-ci-disk-guard-tripped.md). Delete preinstalled software this job never uses.
      # It runs on every runner, so every run measures what each path frees. A missing path is skipped.
      - name: Free preinstalled runner software
        run: |
          avail_mib() { echo $(( $(df --output=avail -k / | tail -1) / 1024 )); }
          echo "Runner class: $(cat /sys/class/dmi/id/sys_vendor 2>/dev/null || echo unknown)," \
            "image ${ImageOS:-unknown} ${ImageVersion:-unknown}," \
            "root $(df --output=source,size -h / | tail -1 | tr -s ' ')"
          df -h /
          start=$(avail_mib)
          for path in \
              /usr/local/lib/android \
              /usr/share/dotnet \
              /usr/local/.ghcup \
              /opt/ghc \
              /opt/hostedtoolcache/CodeQL \
              /usr/share/swift \
              /usr/local/julia* \
              /usr/share/miniconda \
              /usr/local/share/powershell \
              /usr/local/share/chromium \
              /opt/microsoft \
              /opt/google; do
            if [ ! -e "$path" ]; then
              echo "absent   $path"
              continue
            fi
            before=$(avail_mib)
            sudo rm -rf "$path"
            echo "freed $(( $(avail_mib) - before )) MiB  $path"
          done
          echo "Freed in total: $(( ($(avail_mib) - start) / 1024 )) GiB"
          df -h /
```

Notes for the implementer:

- `/usr/local/julia*` is left unquoted in the list on purpose, so the glob expands. If it matches nothing, bash
  leaves the pattern as is, `-e` is false, and the line prints `absent`.
- `sudo rm -rf` of an absent path cannot fail, but the `-e` test keeps the log readable and the deltas honest.
  A failing `rm` (for example a busy file) fails the step under `bash -e`. That is intended: a clean-up that
  silently does nothing would show up only later as a guard trip.
- `/sys/class/dmi/id/sys_vendor` is expected to read `Amazon EC2` on the AWS class and `Microsoft Corporation`
  on Azure; record the real values in §8. It needs no network. `ImageOS` and `ImageVersion` are set on GitHub-hosted runners.
- Do not change "Disk budget before the test build". Its threshold stays at 12 GiB (§2).
- Extend the `::error` message of the guard with the runner class, so a red run names it without reading the
  header:
  `... the test build needs ${need_gib} GiB; runner $(cat /sys/class/dmi/id/sys_vendor 2>/dev/null) (docs/plans/2026-11-04-ci-disk-guard-tripped.md)`.

Then update [`2026-11-03-ci-disk-budget.md`](2026-11-03-ci-disk-budget.md): add a sentence to its §6 row "Disk
budget fails" pointing to this plan, and append the runner-class table of §2 to its §8.2.

## 5. Local verification before pushing

1. Extract the new step's `run:` script from the parsed workflow, as the previous plan did, and run it with
   `bash -e` after replacing the path list with throwaway directories under
   `~/scratch/cidisk2_free/` (one holding a 1 GiB file made with `fallocate`, one absent) and `sudo` with the
   function `sudo() { "$@"; }`. It must print `freed ~1024 MiB` for the first, `absent` for the second, a total
   line, and exit 0. Then make one directory undeletable (a file inside a subdirectory made read-only with
   `chmod 555`) and confirm the step exits non-zero. Delete `~/scratch/cidisk2_free/` afterwards (restore the
   permission first).
2. `python3 -c 'import yaml; yaml.safe_load(open(".github/workflows/ci.yml"))'` parses the workflow.
3. Gate (charter §4): `cargo fmt --all -- --check` and `cargo test --locked -p workspace_checks` (C20 checks the
   paths the charter names, `ci.yml` among them; C18 reads the plan index), with `CARGO_TARGET_DIR` on
   `~/scratch`. No Rust changes, so the full workspace test is not needed for this change.

## 6. Verification on CI

A run cannot be sent to the AWS class on purpose. Verification therefore has two parts.

1. **Every run, at once (Azure).** The first green run after the push shows, in "Free preinstalled runner
   software", the runner class, the `freed N MiB` line of each path and the total. Copy them into §8 of this plan
   with the run ID. The total must be at least 10 GiB; if it is less, the AWS class would still be short of the
   threshold (5.4 + 10 > 12 needs only 6.6 GiB, the rest is margin), so revisit the list before relying on it.
   The job's wall time grows by no more than about 2 minutes against the 20 to 34 minute baseline.
2. **The next AWS run (about 1 in 13).** Find it by the runner class line or by `cow-root` in the disk reports.
   Record in §8: free space before and after the clean-up, at the guard, and after the test build. Expected about
   25 to 30 GiB at the guard and 17 to 22 GiB after the build. When this run is green, set this plan's status to
   `Done` in `docs/plans/README.md`.

Until an AWS run has passed, the status is `Partly done`, with the Azure figures as evidence.

## 7. What red looks like

| Symptom | Meaning | Action |
|---|---|---|
| "Free preinstalled runner software" fails | An `rm` failed: a file busy or a path that changed type | Read the step log; fix that path. Do not add `|| true` |
| Every path prints `absent` | The image changed its layout (for example `ubuntu-latest` moving to Ubuntu 26.04, announced for November 2026) | Read the new image's `Ubuntu2604-Readme.md` and update the list |
| The guard trips on an AWS runner after the clean-up | The clean-up freed less than 6.6 GiB there, or the image grew | Compare the per-path figures with the Azure ones; add paths. If the image has simply outgrown the class, take up option C (§3.3), then B through Q0034 |
| The guard passes on AWS but the build fails with SIGBUS or `No space left`, while "Disk report after the test build" shows free space | The copy-on-write store behind `cow-root` is smaller than the filesystem (§3.1, "One unknown") | Freed blocks do not help on that class. Take up B or C to cut what the build writes, and record the finding here |
| A green AWS run | The fix works | Record figures (§6 step 2), set status `Done` |

## 8. Results

### 8.1 Local (2026-11-04, implementation)

The step of §4 is in `ci.yml` word for word, with the guard's `::error` message extended as §4 asks. The workflow
parses with `yaml.safe_load`. `cargo fmt --all -- --check` and `cargo test --locked -p workspace_checks` pass.

Dry run (§5 step 1). The step's `run:` text was extracted from the parsed workflow, its path list replaced with
`~/scratch/cidisk2_free/present` (holding a 1 GiB `fallocate` file) and `~/scratch/cidisk2_free/absent`, its `df`
calls pointed at `~/scratch` (a separate filesystem from `/` on the development VM), and `sudo` defined as
`sudo() { "$@"; }`. The implementation agent's sandbox refuses to execute a multi-line shell script, so it checked each
part of the script as a plain command:

- `test -e` on the absent path is false, so the step prints `absent` and continues.
- An unmatched glob such as `/usr/local/julia*` stays literal in bash, so it too prints `absent` (the VM has no
  Julia, and `ls -d /usr/local/julia*` finds nothing).
- `rm -rf` of a directory holding a file inside a `chmod 555` subdirectory exits 1 (`Permission denied`), which
  ends the step under GitHub's default `bash -e`.
- `/sys/class/dmi/id/sys_vendor` reads `QEMU` on the development VM, without network access.
- `df --output=avail -k` read 1,349,846,684 KiB before `rm -rf` of the directory holding the 1 GiB file and
  1,350,895,268 KiB after: 1,048,584 KiB, so the step's line reads `freed 1024 MiB`. A first attempt with
  seconds between the readings showed a negative delta, because other users write to the shared `~/scratch`
  disk; on a runner the step runs alone, so its per-path deltas are clean.

### 8.2 CI

**Azure run (§6 step 1):** run 41744388382 at `70c9f03`, green on all six jobs. Runner class `Microsoft Corporation`,
image `ubuntu24 20261026.354.1`, `/dev/root 145G`. The test job took 42.6 min (baseline 35–43 min).

| Path | Freed |
|---|---:|
| `/usr/local/lib/android` | 11 290 MiB |
| `/usr/share/dotnet` | 5 882 MiB |
| `/usr/local/.ghcup` | 3 744 MiB |
| `/opt/ghc` | absent |
| `/opt/hostedtoolcache/CodeQL` | 1 370 MiB |
| `/usr/share/swift` | 3 539 MiB |
| `/usr/local/julia*` (`julia1.13.2`) | 1 043 MiB |
| `/usr/share/miniconda` | 1 147 MiB |
| `/usr/local/share/powershell` | 1 367 MiB |
| `/usr/local/share/chromium` | 671 MiB |
| `/opt/microsoft` | 844 MiB |
| `/opt/google` | 445 MiB |
| **Total** | **30 GiB** |

Free space on `/`: 86 GiB before the clean-up, 116 GiB after, 108 GiB at the guard, 100 GiB after the test build.
On the AWS class (5.4–5.9 GiB free before our build) the same image would leave about 35 GiB at the guard, about
three times the 12 GiB threshold, if the copy-on-write store behind `cow-root` releases the space (§3.1).

**AWS run (§6 step 2):** pending; it cannot be forced. Record the same figures when a job lands on `Amazon EC2`.
