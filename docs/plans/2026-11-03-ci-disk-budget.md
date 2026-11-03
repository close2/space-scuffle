# CI disk budget: compressed debug info and a disk guard

Date: 2026-11-03. Status: Done (verified on CI in run 41639354662). Scope: `.github/workflows/ci.yml` and a new repository `.cargo/config.toml`. No
Rust source changes.

## 1. Problem

CI run 41607070459 on `87fd289` failed in the job "Format, Clippy & Workspace Tests (Headless + Virtual GPU)"
(`ubuntu-latest`, image `ubuntu-24.04` version 20261018.96.1, hosted on AWS `us-west-2`). Four client test
binaries failed to link, `editor_app_tests`, `battle_input_tests`, `skirmish_match_tests` and
`mesher_golden_tests`, each with `collect2: fatal error: ld terminated with signal 7 [Bus error], core dumped`
from `rust-lld`. The next run (`6aec43f`, 41611292118) passed. The full log is
`~/scratch/logs/ci-87fd289-job.log`.

## 2. Root cause: the runner's disk fills while the client tests link

The evidence supports disk exhaustion. Nothing in the log contradicts it, but the log has no `df` output, so it is
not proven. Section 4 adds the measurement.

- **All four failures happen at the same moment.** They are within 125 ms of each other (09:02:42.035 to
  09:02:42.160), in four linkers running in parallel, 63 s after `client` started compiling. A linker bug would
  fail one binary in a way that can be repeated. A shared resource that runs out fails every writer at once.
- **The signal is SIGBUS, not SIGKILL.** `rust-lld` writes its output through a memory-mapped file
  (`FileOutputBuffer`). When the filesystem cannot allocate a page that is written through the mapping, the kernel
  sends SIGBUS. Running out of memory would show as SIGKILL (signal 9) from the OOM killer. The stack traces end in
  `llvm::parallelFor`, the loop that writes output sections.
- **No other cause is visible.** The log has no `No space left` message, because a mapped write cannot return
  ENOSPC. It has no OOM message, no runner warning, and no failure before linking. Clippy and the 29 crates the
  test step compiled all succeeded.
- **The measured need is larger than the documented disk.** GitHub documents 14 GB of SSD for `ubuntu-latest` on
  public repositories (4 CPUs, 16 GB RAM: docs.github.com, "GitHub-hosted runners", runner table, read
  2026-11-03). In practice the free space on `/` is somewhat larger, but how much is not documented and varies by
  image. A green run means the job fits on most runners, with almost no margin.

### 2.1 Measurement

This was a cold `cargo test --workspace --locked --no-run` on this VM with the CI toolchain (Rust 1.99.0, `cc` with
the default self-contained `rust-lld`, the user's `mold` setting overridden, `CARGO_INCREMENTAL=0`). The target
directory was `~/scratch/cargo-target/ci-disk-plan`, deleted afterwards. The raw numbers are in
`~/scratch/logs/cidisk_du.txt` and `~/scratch/logs/cidisk_zsizes.txt`.

| Quantity | Value |
|---|---|
| `target/` at the end of the build | 20 885 MiB (20.4 GiB) |
| ... of which executables (138) | 18 205 MiB |
| ... of which 33 `client` integration-test binaries | 11 850 MiB (302 to 459 MiB each) |
| ... of which `client` and `client_diagnostics` bins, lib and bin tests (19 others) | 5 137 MiB |
| ... of which the other crates' executables (86) | 1 219 MiB |
| Everything else (rlibs 1 709 MiB, build scripts, fingerprints) | 2 680 MiB |
| Growth while the client tests link | 4.2 GiB to 20.4 GiB in about 80 s |

The debug info is already reduced: `[profile.test] debug = "line-tables-only"` reaches every unit, and the build
plan shows no unit with full debug info. Even so, the line tables of the Bevy dependencies (opt-level 2, heavily
inlined) take about 300 MiB in every binary that links the client. For example, `mesher_golden_tests` is 305 MiB,
of which 300 MiB is `.debug_*` and 2 MiB is `.text`. The CI job also needs space this measurement leaves out: the
clippy artifacts in the same `target/`, the toolchain, the cargo registry, the apt packages, and the 841 MB cache
archive while it is restored.

## 3. Options

| Option | Disk effect | Cost | Faithful to the local gate, useful backtraces |
|---|---|---|---|
| A. Compress debug sections at link time (`-Wl,--compress-debug-sections=zlib`) | Executables 18.2 GiB to about 6.4 GiB, `target/` 20.4 GiB to about 9.1 GiB | About +0.3 to 0.6 s per Bevy link (4.0 s to 4.3 to 4.6 s measured on `skirmish_match_tests`); one full rebuild per target directory | Yes, if set in the repository config (both sides then build the same binaries). Backtraces keep `file:line` (verified, §3.1) |
| B. `CARGO_PROFILE_TEST_DEBUG=0` in CI | Executables shrink to about 136 MiB per Bevy binary, or less | CI builds different binaries from the local gate | No. Backtraces lose `file:line` |
| C. `strip = "debuginfo"` | Same as B | Same as B | No |
| D. `split-debuginfo = "unpacked"` | Debug info stays in the `.o` files, which are as large and sit in the same `target/` | Linking is faster | Partly. Backtrace support for split DWARF is not reliable on stable |
| E. Free preinstalled runner software (Android SDK, .NET, GHC, CodeQL) | Gains GBs, but the amount is not documented for this AWS image | About 1 to 3 minutes of `rm -rf`; the result depends on the image | Yes, but it treats the symptom; the binaries keep growing with each new test file |
| F. Consolidate the 33 client test files into one binary (`tests/client/main.rs` with modules) | 11.85 GiB to about 0.5 GiB; 32 fewer Bevy links | A large test-layout refactor; the GPU children that run their own binary, `CARGO_BIN_EXE_*` users and per-file `support/` includes all have to be reworked | Yes. It is the best long-term fix, but larger than this incident needs |
| G. Build and run per crate, deleting each crate's binaries | The peak is still the client's 16 GiB | The gate becomes several commands | No. CI would no longer run the charter's single gate command |

**Chosen: A, plus a disk guard (§4).** It cuts the peak by about 11.3 GiB (55 %) without changing any test. CI
and the local gate build the same binaries, and backtraces still give file and line. F stays open as a later
decision (§7) and stacks with A.

### 3.1 Checks behind option A

- **Size.** The measurement ran `objcopy --compress-debug-sections=zlib` on all 138 executables: 18 205 MiB went
  to 6 025 MiB. Relinking `skirmish_match_tests` with the linker flag gave 213 MiB, against 195 MiB from
  `objcopy`. `lld` compresses at a lower zlib level, so the whole-workspace estimate is 6 025 x 213/195, about
  6 590 MiB of executables. Adding the 2 680 MiB of other files gives about 9.1 GiB for `target/`.
- **Backtraces.** A panicking test program built with `-C debuginfo=line-tables-only` printed
  `at ./cidisk_bt.rs:1:24` with `zlib` compression, under both `rust-lld` and `mold`. With `zstd` the frame lost
  its file and line, because the standard library's symbolizer cannot read zstd sections. The setting must
  therefore be `zlib`.
- **Config merge.** Cargo joins `rustflags` arrays across config files. On this VM,
  `cargo +nightly -Zunstable-options config get` with a repository config and the user's `~/.cargo/config.toml`
  showed `["-C", "link-arg=-fuse-ld=mold", "-C", "link-arg=-Wl,--compress-debug-sections=zlib"]`. `mold` 2.37.1
  accepts the flag.
- **Scope.** The table `[target.x86_64-unknown-linux-gnu]` does not affect Windows, macOS or wasm32. The Linux
  determinism job picks it up, which does no harm: its release golden replay has no debug info to compress.
- **One gap.** An environment variable `RUSTFLAGS` replaces config `rustflags` entirely. The guard in §4 detects
  this.

## 4. Changes

### 4.1 New file `.cargo/config.toml`

```toml
# Workspace Cargo configuration, applied on every machine that builds this checkout: the local gate
# (AGENTS.md §4) and CI use the same linker flags, so they build the same test binaries.
#
# PERFORMANCE RATIONALE:
# Why: Every client test binary links Bevy, and with `debug = "line-tables-only"` each one still carries about
#   300 MiB of DWARF line tables. With 138 test binaries, `target/` reached 20.4 GiB, which filled the GitHub
#   runner's disk and killed rust-lld with SIGBUS (docs/plans/2026-11-03-ci-disk-budget.md).
# What: Ask the linker to zlib-compress the `.debug_*` sections. Executables shrink from 18.2 GiB to about
#   6.4 GiB.
# Trade-off: About 0.5 s more per large link. Panic backtraces keep file and line, because std's symbolizer
#   reads zlib sections. Do not change this to zstd: std cannot read zstd sections, and backtraces would lose
#   their line numbers.
# Note: Cargo joins this array with `rustflags` from ~/.cargo/config.toml, but an environment variable
#   RUSTFLAGS replaces it. CI's "Test binaries carry compressed debug info" step fails if that happens.
[target.x86_64-unknown-linux-gnu]
rustflags = ["-C", "link-arg=-Wl,--compress-debug-sections=zlib"]
```

### 4.2 `.github/workflows/ci.yml`, job `check-and-test`

```diff
       - name: Rust Cargo Cache
         uses: Swatinem/rust-cache@v2
 
+      # The test build needs about 9.1 GiB of target/ when cold (docs/plans/2026-11-03-ci-disk-budget.md).
+      # Report the disk before any build, so a red run shows how much the runner had to begin with.
+      - name: Disk report after cache restore
+        run: |
+          df -h / "$GITHUB_WORKSPACE"
+          du -sh target 2>/dev/null || echo "target/ not restored"
+
       - name: Check formatting (cargo fmt)
         run: cargo fmt --all -- --check
 
       - name: Lint workspace (cargo clippy)
         run: cargo clippy --workspace --all-targets --locked -- -D warnings
 
+      - name: Disk budget before the test build
+        run: |
+          df -h "$GITHUB_WORKSPACE"
+          du -sh target
+          need_gib=12
+          avail_gib=$(( $(df --output=avail -k "$GITHUB_WORKSPACE" | tail -1) / 1024 / 1024 ))
+          if [ "$avail_gib" -lt "$need_gib" ]; then
+            echo "::error title=Disk budget::${avail_gib} GiB free under $GITHUB_WORKSPACE; the test build needs ${need_gib} GiB (docs/plans/2026-11-03-ci-disk-budget.md)"
+            exit 1
+          fi
+          echo "Disk budget: ${avail_gib} GiB free, ${need_gib} GiB required"
+
       - name: Run all workspace unit & integration tests (including xvfb + llvmpipe GPU tests)
         run: cargo test --workspace --locked
+
+      - name: Test binaries carry compressed debug info
+        run: |
+          bin=$(ls -t target/debug/deps/skirmish_match_tests-* | grep -v '\.d$' | head -1)
+          readelf -SW "$bin" | sed 's/^ *\[ *[0-9]*\]//' \
+            | awk '$1 == ".debug_info" { found = 1; ok = ($7 ~ /C/) } END { exit !(found && ok) }' \
+            || { echo "::error title=Disk budget::$bin has uncompressed .debug_info; is RUSTFLAGS overriding .cargo/config.toml?"; exit 1; }
+          ls -l "$bin"
+
+      - name: Disk report after the test build
+        if: always()
+        run: |
+          df -h "$GITHUB_WORKSPACE"
+          du -sh target target/debug/deps 2>/dev/null || true
+          find target/debug/deps -maxdepth 1 -type f -size +100M -printf '%s %f\n' 2>/dev/null \
+            | sort -rn | head -5 | awk '{ printf "%6.0f MiB  %s\n", $1 / 1048576, $2 }'
```

The 12 GiB threshold is the measured cold need (9.1 GiB) plus about 3 GiB of margin, rounded. It is checked after
clippy, so it counts what clippy and the restored cache have already taken. If the first green run shows a much
larger free figure, leave the threshold as it is: it is a floor, not a forecast. Change it only with new numbers
recorded in this plan.

The `awk` field `$7` is `readelf`'s `Flg` column once the `[Nr]` prefix is removed. The assertion was checked on
this VM against one compressed and one uncompressed `skirmish_match_tests` binary: exit 0 and exit 1.

## 5. Verification

Local, before pushing (charter §4 gate, in a worktree with its own `CARGO_TARGET_DIR` on `~/scratch`):

1. `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and
   `cargo fmt --all -- --check` all pass. The first run rebuilds everything once, because the rustflags changed.
2. `du -sm $CARGO_TARGET_DIR` after `cargo test --workspace --no-run` is at most about 10 GiB. Record the figure
   here, next to the 20 885 MiB baseline.
3. Run the `readelf` assertion from §4.2 against the worktree's `skirmish_match_tests` binary; it passes.
4. Backtraces: run one test that fails on purpose, for example by flipping an assertion temporarily without
   committing it, with `RUST_BACKTRACE=1`. The backtrace names `crates/.../tests/<file>.rs:<line>`.

On CI, after the push:

5. The job is green. The step "Disk report after cache restore" shows the runner's real free space, which was
   never recorded before. "Disk budget" prints `N GiB free, 12 GiB required` with N at least 12. "Test binaries
   carry compressed debug info" passes. "Disk report after the test build" shows `target/` at about 10 GiB or less
   and its five largest binaries at about 100 to 230 MiB, not 300 to 460 MiB.
6. Copy the three `df` figures and the `du` figure from that run into this plan's §8, with the run ID.
7. Wall time of the job does not grow by more than about one minute against the recent green runs (20 to 34
   minutes).

## 6. What red looks like

| Symptom | Meaning | Action |
|---|---|---|
| "Disk budget" fails with `::error title=Disk budget::N GiB free ...` | The runner began with too little space, or the restored cache and clippy took more than expected | Read the two `df` reports. If the image shrank, add option E (free preinstalled software) as a step before the cache restore, and record the numbers here |
| "Test binaries carry compressed debug info" fails | `RUSTFLAGS` or a changed config is overriding `.cargo/config.toml`; disk use is back at 20 GiB | Remove the override, or put the flag back into it |
| SIGBUS from `rust-lld` again, with "Disk report after the test build" showing `/` at 100 % | The test binaries have outgrown even the compressed budget | Take up option F (§7) |
| SIGBUS with plenty of free space in the after-build report | The disk hypothesis is refuted for that run | Open a new investigation: memory, or a linker bug; capture `dmesg` and retry with `-j2` |
| A backtrace in a CI failure has no `file:line` | Compression changed to `zstd`, or the debug level dropped | Restore `zlib` and `line-tables-only` |

## 7. Owner decisions

1. **The flag lives in the repository's `.cargo/config.toml`, not in CI only.** CI and the local gate then build
   the same binaries, which charter §4 needs. It also cuts the main checkout's 59 GiB `target/`. The cost is one
   full rebuild in every existing target directory and worktree. The alternative is
   `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS` set on the CI job only, which leaves local builds as they are
   but means CI builds different binaries from the local gate. The default here is the repository config.
2. **Consolidate the client integration tests (option F) later?** This would cut the client's 33 Bevy links to
   one, save about 11 GiB on top of option A, and shorten every gate run. It is a refactor of
   `crates/client/tests/`, and it is not part of this plan.

Both decisions are asked as [Q0013](../questions/closed/0013-ci-disk-budget.md), which states the defaults above.

## 8. Results

### 8.1 Local (2026-11-03, implementation)

Both builds were cold `cargo test --workspace --locked --no-run` runs from the same commit tree on this VM, with
`RUSTC_WRAPPER=sccache` and `CARGO_INCREMENTAL=0`. The linker was the user's `mold` (from `~/.cargo/config.toml`,
whose `rustflags` cargo joins with the repository's), not CI's `rust-lld`, so CI's figures will differ somewhat.

| Quantity | Without `.cargo/config.toml` | With it |
|---|---|---|
| `du -sm` of the target directory | 21 088 MiB | 9 742 MiB (-54 %) |
| Five largest executables in `debug/deps` | 458 to 468 MiB | 234 to 241 MiB |
| `skirmish_match_tests` | 458 MiB | 234 MiB (245 043 400 bytes) |
| `.debug_info` / `.debug_line` of `skirmish_match_tests` | 101.1 / 49.6 MiB, flag empty | 34.3 / 14.3 MiB, flag `C` |

The `target/` figure meets §5 step 2 (at most about 10 GiB). The largest binaries are slightly above the
"about 100 to 230 MiB" that §5 step 5 expects; that range came from `rust-lld`, and these numbers from `mold`.

The four `ci.yml` snippets were extracted from the parsed workflow and run with `bash -e` (the shell GitHub uses
for `run:` without `shell:`) in a directory holding each target directory as `target/`:

- "Test binaries carry compressed debug info": exit 0 with compression, and exit 1 with
  `::error title=Disk budget::target/debug/deps/skirmish_match_tests-... has uncompressed .debug_info; ...` without.
- "Disk budget": prints `Disk budget: 1248 GiB free, 12 GiB required`, and with the threshold raised to 99999 in a
  copy it prints the `::error` line and exits 1.
- Both disk reports exit 0 and print the figures above.

Backtrace (§5 step 4): with an assertion in `the_pilot_reads_the_screen_only` flipped temporarily, not committed,
`RUST_BACKTRACE=1` printed `panicked at crates/client/tests/skirmish_match_tests.rs:219:9` and the frame
`at ./tests/skirmish_match_tests.rs:219:9`, from a binary whose `.debug_info` carries the `C` flag.

Gate, after the rebuild: `cargo fmt --all -- --check` and `cargo clippy --workspace --all-targets --locked -- -D
warnings` pass. `cargo test --workspace --locked --no-fail-fast` on lavapipe: 1 504 passed, 2 failed, 4 ignored.
The failures were `production_plugin_schedule_initializes_terrain_and_restores_tab_camera`, known to fail only on
lavapipe, and `the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window`, whose child hit its 300 s limit
while the shared VM's load average was about 40 on 16 cores. Rerun alone on lavapipe, `skirmish_match_tests`
passed (3 passed, 1 ignored, 118 s). `runtime_battle_tests` on the RX 580 under `with-gpu-lock`: 18 passed.

### 8.2 CI

Run 41639354662 on `main` at `c529d4c` (the first green run carrying the change; run 41630288360 on `79887c6`
failed only on the smog goldens and showed the same figures):

| Step | `/` size | Used | Free | `target/` |
|---|---|---|---|---|
| After the cache restore | 145 GiB | 64 GiB | 81 GiB | 3.0 GiB |
| Disk budget (before the test build) | 145 GiB | 65 GiB | 80 GiB (79 GiB by the guard's count, 12 required) | 3.7 GiB |
| After the test build | 145 GiB | 74 GiB | 71 GiB | 13 GiB |

`Test binaries carry compressed debug info` passed. This runner had about ten times the free space of the runner that
hit the SIGBUS at `87fd289`; GitHub's runner disk varies, which is why the guard checks free space at run time rather
than assuming a size. With compression the test build used 13 GiB of `target/`, so even the earlier small runner
(about 14 GB free) now has room.
