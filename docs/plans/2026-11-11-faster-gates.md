# Faster gates: test lanes, diff-aware catalog and lockfile selection, explicit sim→client contracts

Date: 2026-11-11. Status: Proposed. Base: `main` at `a0f8e7b`. Host: the shared Debian 13 VM, 16 cores, AMD Radeon
RX 580 on `radv`; the user slice's `memory.high` is now 66 GiB (`memory.max` 70 GiB). Charter sections touched:
§7 (the memory figures, WP8 only). §4.3, §5, §6 and the check table are not changed: `cargo gate full` stays the
CI command word for word (C16), and C22 keeps its meaning.

This is a stage 1 plan under charter §6, written by the planning agent for the orchestrator's decision to take
items 1, 2, 3 and 5 of the workspace-split evaluation
([`docs/evidence/2026-11-11-workspace-split-evaluation.md`](../evidence/2026-11-11-workspace-split-evaluation.md),
"the evaluation" below). It extends the tiered-gate blueprint
([`2026-11-04-tiered-test-gates.md`](2026-11-04-tiered-test-gates.md), "the blueprint") and does not replace it.
Line numbers drift; search for the quoted symbol.

## 1. Request

The owner asked whether splitting the workspace would make development faster. The evaluation answered no and
ranked cheaper measures; the owner has asked repeatedly for less time spent waiting on tests. This plan delivers
four of them:

1. **Test lanes.** Run test executables concurrently: one GPU lane and two general lanes, within a memory budget.
2. **Diff-aware selection for `crates/gate/suites.ron`.** A one-line catalog edit selects what it changes, not the
   complete suite. The `sim_fixtures` caller list, which C22 forces agents to edit by hand for every new
   `sim_core` test, is computed from the sources instead.
3. **Diff-aware selection for `Cargo.lock` and member manifests.** A dev-dependency-only change selects the member
   crate. A runtime or external dependency change still selects the complete suite.
4. **Explicit sim→client contracts.** The smog system goldens stop hashing the `Debug` text of a `sim_types`
   record.

Goals, in the owner's terms: a median `package` gate about half as long; no test removed, ignored or weakened; the
complete suite still runs on every `full` and every CI push; every rule written as data or as pure, tested code.

## 2. Starting point

### 2.1 Measured in the evaluation (28 `package` gates, 2026-11-09 to 11-11)

| Quantity | Value |
|---|---|
| Wall time in gates, sum; median gate | 36.4 h; 78 min |
| Test executables, commands and doc tests | 86.9 % of gate time |
| CPU/wall ratio over the test phase | 1.9 to 2.2 on 16 cores |
| Gates that selected the complete suite | 14 of 28 (68 % of gate time) |
| Causes of those 14 | `crates/gate/suites.ron` 9 (mostly `+ "sim_core/<new>_tests",` in the `sim_fixtures` list), `Cargo.lock` with `crates/sim_core/Cargo.toml` 4 (the dev-dependency `proptest`, already locked), `.github/workflows/ci.yml` 1 |
| Client tests caught only because a complete suite ran | 1 (`smog_golden_tests`, `StatusFlags` `Debug` text) |
| Slowest executables (`verify_spells_all`) | `battle_input_tests` 605 s, `editor_app_tests` 500 s, `battle_fog_tests` 475 s, `battle_hud_tests` 461 s, `battle_card_tests` 456 s, `effect_slice_gpu_tests` 372 s |

The four `Cargo.lock` gates also changed `crates/sim_core/Cargo.toml`, which is global too (`crates/*/Cargo.toml`).
The evaluation reported the lockfile because it sorts first. Rule 3 must therefore cover member manifests as well
as the lockfile, or those gates stay complete.

### 2.2 What the gate already has

- **A lane scheduler exists** (`crates/gate/src/schedule.rs`, `Scheduler`). `--jobs n` above 1 reserves one lane
  for `gpu` jobs and gives `valgrind` jobs two general lanes. The jobs run longest-first by the last stamp's wall
  time. The default is 1 (`crates/gate/src/cli.rs`, `RunOptions::default`) because of the blueprint's §2.6
  measurement on 2026-11-04. Four lanes were slower then, because several gates shared the cores, and one pixel
  test failed under load.
- **What has changed since §2.6.** The gate lock (Amendment 2) allows one gate at a time, so a gate no longer
  competes with another gate's binaries. The skirmish threshold proved load-independent (blueprint §11.4). Every
  heavy client binary has a measured thread cap that holds it near 6 GiB (Amendment 4). The test phase uses about
  2 of 16 cores.
- **Missing pieces for lanes:** no memory accounting (the caps bound each binary, nothing bounds their sum); no
  per-target timeout; no way to stop a running gate other than killing it; `valgrind` overlaps other work; the
  report's total adds up target wall times, which overstates the time once targets overlap; executables page in
  cold from the rotational scratch disk.
- **Selection is path-only.** `select` (`crates/gate/src/select.rs`) sees changed paths, never their contents. A
  path matching `global` selects the complete suite.

### 2.3 Measured peaks the budget starts from

Amendment 4 of the blueprint (radv, debug, at the caps): battle drivers 3.8 to 6.6 GiB, `editor_app_tests` 8.6 GiB
(process tree), `smog_bouncing_bubbles_tests` 5.7 GiB, `skirmish_match_tests` 5.6 GiB, `lab_scene_tests` 6.9 GiB,
`runtime_battle_tests` 5.9 GiB, other GPU binaries 0.3 to 4.0 GiB. `client/lib` was never measured. The worst
plausible three-lane combination is about 6.6 + 6.5 GiB (two general lanes) + 8.6 GiB (GPU lane) ≈ 22 GiB. This
plan does not rely on these figures. It measures every binary's peak on every run and schedules from the
measurements (§3.3).

## 3. Design: test lanes

### 3.1 Rules

- **L-1. Lanes.** `--jobs n` (default 3 after the A/B of §7; 1 until then) gives one GPU lane and `n − 1` general
  lanes. The GPU lane runs one `gpu` binary at a time; its children still take `/run/lock/gpu.lock` one at a time
  through the harness (charter §7). The runner never wraps anything in `with-gpu-lock`. When no `gpu` job is
  selected, all `n` lanes are general (unchanged).
- **L-2. Exclusive jobs run alone, first.** An exclusive job runs with nothing else running, on any lane. Three
  kinds of job are exclusive:
  - every `valgrind` job (its children run at up to one per core, and its counts must not be skewed by other
    load);
  - every job with no recorded memory peak on this host (a new target, or a fresh CI runner), since nothing
    bounds it yet;
  - every target marked `exclusive: true` in `suites.ron`: a test proven load-sensitive (§3.6).
  The exclusive jobs are queued ahead of all others, so they run one after another while every lane is still
  idle, and no lanes need to drain for them. Today's `valgrind` width of two lanes is replaced by this rule.
- **L-3. Order.** After the exclusive jobs, each queue runs longest-first by expected wall time, with ties broken
  by target name, as today. A general job that does not fit the memory budget (§3.3) may be overtaken by a later
  job that fits. Every job still runs eventually: the queue is finite, and any job may start once the lanes are
  idle (L-4).
- **L-4. Never stall.** When nothing is running, the head job starts, whatever its estimate and whatever the live
  memory reading says. A run can therefore become sequential, but it can never deadlock.
- **L-5. Test threads unchanged.** Each executable keeps its `test_threads` cap from `suites.ron`. Lanes multiply
  executables; they never raise any binary's thread count.
- **L-6. Pre-read.** Before spawning an executable, its lane reads the file once, sequentially, and discards the
  bytes, so the executable pages in at disk streaming speed and not at about 80 ms per random fault (evaluation
  §2.5). The time is reported per target as part of its wall time. The GPU harness already does this for its
  children.
- **L-7. Reruns, doc tests and commands stay sequential.** Failed targets are rerun once, alone, after the lanes
  finish (`rerun_failures`, unchanged). Doc tests and the catalog commands (golden replays, wasm check) run after
  the reruns, as now.

### 3.2 Where the defaults live

| Setting | Precedence | Default |
|---|---|---|
| Lanes | `--jobs n`, else `SPACE_SCUFFLE_GATE_JOBS`, else the default | 1 until WP7, then 3 |
| Memory budget (sum of running estimates) | `--memory-budget-gib`, else the default | 40 GiB, capped at the ceiling below |
| Live ceiling (slice `anon` + the next job's estimate) | `--memory-ceiling-gib`, else the cgroup limit minus 10 GiB | 56 GiB on this server (66 − 10) |

The cgroup limit is read by walking up from the gate's own cgroup (`/proc/self/cgroup`: the session scope)
to the first ancestor whose `memory.high` or `memory.max` is a number (here `user-1002.slice`, 66 GiB). When no
ancestor has a limit, `MemTotal` from `/proc/meminfo` is used. When cgroup v2 is missing, the live ceiling is off
and only the budget applies. The live reading is that cgroup's `anon` from `memory.stat` (charter §7: `memory.current`
also counts page cache).

**CI** keeps one lane: the job sets `SPACE_SCUFFLE_GATE_JOBS: 1` in the step's `env:`. The command stays
`cargo gate full`, word for word, which C16 compares with the charter. A fresh runner has no stamps, so L-2 would
run everything alone anyway. The explicit setting documents the choice and survives a future stamp cache. Lanes on
CI are out of scope (§11, D-7).

### 3.3 Memory admission

- **Measurement.** One sampler thread per run wakes every second. It scans `/proc` once, building a pid →
  (ppid, state) map, and for each running target sums `RssAnon` over the target's process and every descendant.
  That tree includes GPU children under `xvfb-run` and GNU `timeout`, which start their own process groups, so
  descendants are found through ppid and not through the process group. The sampler keeps each target's peak and
  reads the slice's `anon` too. Each stamp target gains `peak_anon_mib: Option<u64>`, and the stamp gains the run's
  slice peak, `jobs`, and the test phase's wall time. `#[serde(default)]` keeps old stamps readable.
- **Estimate.** A job's estimate is the largest `peak_anon_mib` it recorded in the last five stamps from this
  host (stamps already record `host`). A job with no recorded peak is exclusive (L-2). Using the maximum of five
  runs lets a Veo batch that grows a binary raise its estimate on the next run.
- **Admission.** A general job starts when a general lane is free and either nothing is running (L-4), or both of
  these hold:
  1. the sum of the running jobs' estimates plus its own is at most the budget;
  2. the live slice `anon` plus its own estimate is at most the ceiling.
  The GPU lane's job counts toward both. The live term covers what the estimates cannot see: other agents' builds
  in the same slice (charter §7 allows two building agents) and a binary that has outgrown its estimate.
- **No killing for memory.** Above the ceiling the gate stops admitting, prints one line naming the slice `anon`
  and the running targets, and waits. Killing a test for memory would turn load into red results. The kernel's
  `memory.high` throttling is the backstop it always was.
- **Purity.** The scheduler stays pure (`Scheduler::take(&mut self, live_anon: Option<u64>)`). The sampler and the
  cgroup reads live in a new module, `crates/gate/src/memory.rs`, which parses fixture text in its unit tests.

### 3.4 Timeouts and stopping

- **Per-target timeout.** Each target's limit is `max(4 × expected, expected + 15 min)`, where `expected` is its
  last recorded wall time. A target with no recorded time gets 2 h. `suites.ron` may set `timeout_s` for a target
  (unused at first). The constants are named and documented in `execute.rs`: today the longest executable takes
  605 s, a GPU child may wait up to 290 s for the lock and then run 120 s, and a GPU binary has many children, so
  the limit must exceed a slow but healthy run by a wide margin. It exists to free a lane from a hang, not to
  police speed. On expiry the lane sends `SIGTERM` to the target's whole descendant tree (from the sampler's map),
  then `SIGKILL` after 10 s. Signals are sent through `kill(1)`, as `tests/gate_lock_tests.rs` already does,
  because the workspace forbids `unsafe`. The outcome is the new `Outcome::TimedOut`. It counts as a failure and
  is rerun once, alone, with the same limit. If the rerun passes, the target is `FLAKY` with the note "timed out
  under lanes" (§3.6).
- **`cargo gate stop`.** This new command reads the gate lock's holder record (PID, tier, worktree:
  `gate_lock.rs`) and writes a stop request beside the lock (`<lock>.stop`, naming the holder's PID). The holder
  polls for it in the loop that already wakes every 25 ms. It kills every running target's tree as above, starts
  nothing more, skips reruns, doc tests and commands, and records each unstarted target as `Outcome::Skipped`. It
  then prints the table, writes no stamp, removes the request, exits with status 2 and releases the lock. `stop`
  with no holder says so and exits 0. A request that names a PID other than the current holder's is stale; the
  next gate to take the lock deletes it. `stop` never waits for the gate lock.
- **Ctrl-C and group kills keep working.** Test executables stay in the gate's process group, so a terminal
  Ctrl-C or `kill -TERM -- -<pgid>` still reaches every lane, as now. The gate prints that command and its process
  group at start. Killing only the gate's PID leaves its children running, as it does today. The holder record
  therefore gains the process group, and a gate that takes the lock warns, with the PIDs and the command, when
  processes of the previous holder's group are still alive. It does not kill them. They no longer hold the lock
  (Rust opens it close-on-exec), but they still use memory and the GPU.

### 3.5 Output

- **One line per event, printed under the lanes mutex.** At start: `[start] L2 client/battle_fog_tests (est. 5.3
  GiB, 233 s)`. At finish: `[12/58] PASS  233.4 s  5.1 GiB  L2 client/battle_fog_tests`. Lines never interleave
  mid-line, but their order follows finishing times, which differ from run to run. That is the only
  nondeterministic output.
- **Logs.** Each target still writes stdout and stderr to its own file. The GPU harness writes into its parent's
  file. A run also writes `lanes.tsv` in the log directory: target, lane, start, end, wall, CPU, peak, estimate
  and outcome, sorted by start. The A/B evidence and later diagnosis read this file.
- **Deterministic report.** The result table stays sorted by target. The final line replaces "X s in steps and
  tests" with `steps 101 s, tests 2,410 s wall (6,534 s of executables, overlap 2.7×), slice anon peak 24.8
  GiB`. The table's `wall` column stays per executable.

### 3.6 Flakes under lanes

The flake rule is unchanged: every failed target is rerun once, alone; one that then passes is `FLAKY`; a flaky
target never makes `package` green; only `full` repeats once (blueprint §3.5). Lanes make one class of flake more
likely, a test that fails only under load, so:

- A `FLAKY` line under lanes names the targets that overlapped the failed run, from `lanes.tsv`.
- `cargo gate timings` already lists repeated flakes. It also reports how many of a target's flakes happened with
  other targets running.
- A test shown to be load-sensitive is fixed (preferred; the skirmish threshold was fixed this way in §11.4 of the
  blueprint) or marked `exclusive: true` in `suites.ron` with a comment naming the evidence. Neither `#[ignore]`
  nor a retry loop is allowed (blueprint D3).

### 3.7 Shared paths between binaries

Lanes run binaries of the same crate at once. Today two tests can collide on a path only inside one binary. Under
lanes, two binaries can collide too, for example on a fixed file name under `CARGO_TARGET_TMPDIR`, which every
integration test of a package shares. WP5 audits every test file of `client`, `client_diagnostics` and
`asset_baker` for writes outside a `tempfile` directory: `CARGO_TARGET_TMPDIR` joins, `target/` paths and fixed
`~/scratch` paths. It fixes each collision by putting the binary's name into the path, and records the audit in
the evidence. Reads of `assets/` are shared and safe.

## 4. Design: selection

### 4.1 What still forces the complete suite

| Input | Before this plan | After |
|---|---|---|
| `cargo gate full`, CI | complete | complete (unchanged) |
| Root `Cargo.toml`, `.cargo/**`, `rust-toolchain*`, root `clippy.toml`, `third_party/**`, `tools/renderer_compatibility/**` | complete | complete (unchanged): they change how every crate builds |
| `crates/gate/src/**` | complete (`crates/gate/**`) | complete: the runner decides how every executable runs (environment, working directory, caps, lanes, timeouts), and a mistake there shows only when other crates' tests run |
| `crates/gate/tests/**` | complete | the `gate` crate's targets (crate rule) |
| `crates/gate/suites.ron` | complete | its semantic diff (§4.2); complete only for `global`, `docs` or `always` edits or an unreadable base |
| `Cargo.lock` | complete | its diff (§4.4); complete for any external change |
| `crates/<c>/Cargo.toml` | complete | dev-only diff: crate `<c>`; anything else complete (§4.4) |
| `crates/*/clippy.toml` | complete | complete (unchanged; rare) |
| `.github/workflows/**` | complete | the new rule "CI workflows": `workspace_checks/@fast` (C16 and C24 read `ci.yml` and `showcase.yml`). No local test exercises a workflow; the push's CI run does, and charter §4.3's fix-forward-or-revert-within-an-hour rule covers a broken one |
| A path no entry covers | complete (P3) | complete (unchanged) |

### 4.2 Diff-aware `suites.ron`

The gate reads the catalog at the base revision (§4.5), parses it with the same parser and compares the two parsed
catalogs (`crates/gate/src/catalog_diff.rs`, pure). Comments, ordering and formatting are invisible to the
comparison. For each kind of difference:

| Difference | Selects |
|---|---|
| `global`, `docs` or `always` changed | complete suite (cause `catalog scope changed`): these change what every path means |
| A `targets` entry added, removed or changed (class, triggers, `test_threads`, `exclusive`, `timeout_s`) | that target, if the workspace has it |
| A rule added, removed or changed (matched by name; a rename counts as a removal plus an addition) | the union of the old and new versions' `targets` and `crates`, resolved against today's workspace; references that no longer resolve are dropped |
| A `downstream` list gained references, or a key was added | the added references, resolved |
| A `downstream` list lost references, or a key was removed | nothing: removing a contract cannot make a test fail; `full` and C22 check the rest |
| A command added or changed | that command |
| The base catalog does not parse, or is missing at the base | complete suite (cause `base catalog unreadable`) |

The edited file lies under `crates/gate/`, so the crate rule also selects every `gate` target: `selection_tests`
and `suite_catalog_tests` (C22) always run on a catalog edit. `explain` prints each difference with what it
selected, for example `suites.ron: targets: client/editor_app_tests test_threads 2 → 3`.

### 4.3 The `sim_fixtures` contract, computed

`downstream: { "sim_fixtures": ["@callers"] }` replaces the 79 hand-written entries. `@callers` is a new
`TargetRef` variant, valid only as a `downstream` value (the parser rejects it elsewhere). It resolves to every
target outside the key crate whose own files (including `#[path]` support files) name `<key>::` in code, by the
existing `mentions_path_root`. `SourceIndex::build` already reads every target's files to resolve `mod`
declarations. It now records, per workspace crate, the targets whose code names it (`SourceIndex::callers_of`), so
`select` stays pure and reads no file.

- A new `sim_core` test that calls `sim_fixtures::` is in the contract with no catalog edit, and C22 has nothing
  to compare it against. This removes the churn behind 9 of the 14 complete gates.
- C22's test `the_sim_fixtures_contract_lists_exactly_its_callers` becomes
  `the_sim_fixtures_contract_is_computed_from_its_callers`. It requires the entry to be `["@callers"]`. It keeps
  its independent scan as an oracle and requires the scan to equal `callers_of("sim_fixtures")`. So the index and
  the test cannot drift apart unnoticed.
- `@callers` is not offered for `sim_types` or `sim_core`, because every client target names them. Their
  contracts stay curated lists of the tests that pin behaviour (blueprint §4.3).

### 4.4 Diff-aware `Cargo.lock` and member manifests

Both files are TOML. The gate parses them with `toml_edit`, at a version already in `Cargo.lock` (0.22.27 and
0.25.17 are both locked, through `bevy_macro_utils` and `proc-macro-crate`; WP3 picks one and pins it). No
package is downloaded or added to the lockfile beyond `gate`'s own dependency list. The new modules are
`crates/gate/src/lockfile.rs` and `crates/gate/src/manifest_diff.rs`, both pure over text.

**Member manifest** `crates/<c>/Cargo.toml`, base against head:

- The **dev sections** are `[dev-dependencies]`, every `[target.<cfg>.dev-dependencies]`, and the `[[test]]`,
  `[[bench]]` and `[[example]]` arrays. If the two manifests are equal once their dev sections are removed, the
  change is **dev-only**. It selects crate `<c>` as a crate change: its `fast` and `slow` targets. Any `[[test]]`
  entry that differs also selects that target, whatever its class. Downstream contracts are not selected: a
  dev-dependency never reaches a dependent.
- Anything else is a **runtime change** and selects the complete suite (cause `crates/<c>/Cargo.toml: [dependencies]
  changed`). That covers `[dependencies]`, `[build-dependencies]`, `[features]`, `[lib]`, `[lints]` and
  `[package]`.

**Lockfile**, base against head, keyed by (name, version, source):

- If any entry with a `source` (an external package) is added or removed, or its checksum or dependencies
  change, the change selects the complete suite (cause `Cargo.lock: external package <name> <version> changed`).
  This covers every version bump and every newly downloaded crate.
- If only workspace members' entries changed (no `source`), and only in their `dependencies` arrays, then every
  such member's manifest must also have changed with a dev-only diff. The lockfile then adds nothing beyond those
  crate changes. A member whose lock entry changed while its manifest did not, or changed at runtime, selects
  the complete suite (cause `Cargo.lock: <member> dependencies changed without a dev-only manifest change`).
- Added or removed members select the complete suite. That case also changes the root `Cargo.toml`.

`proptest` added to `sim_core` as a dev-dependency (already locked) therefore selects `sim_core`'s own targets,
and the golden-replay commands through their `crates/sim_core/**` paths. It no longer selects the complete suite.
A `serde` bump, a new crate from crates.io, or a new runtime dependency of a sim crate still selects everything.

**Feature unification caveat.** In a `cargo test --workspace` build, a dev-dependency's features unify with other
crates' builds of shared packages. A dev-only change could, in principle, change another crate's test build. The
rule accepts this because the new edge points at a package that is already locked (no external entry changed), the
gate builds per crate with exact filters (`build_invocations`), and `full` on `main` before each push, plus CI,
build the whole workspace. The risk is recorded as R-7.

### 4.5 Where the base comes from

`Git` gains `show(rev, path) -> Result<Option<String>, GateError>` (`None` when the path does not exist at
`rev`). For `ChangeSpec::Since(base)`, the "before" text is `base:path` and the "after" text is the working-tree
file. For `ChangeSpec::Between(a, b)` (the `merge` tier, `explain a..b`), it is `a:path` and `b:path`. `select`
gets its contents through a new input, `Change { paths, catalog_before, lock_before, manifests_before }`, which
the runner fills only for the files that changed. `select(tier, inputs, &paths)` stays, as the path-only form, and
treats those files as before: complete. Every existing caller and test keeps its meaning (P3: uncertain means
more).

## 5. Explicit sim→client contracts

**Choice: hash the bytes the smog consumes; do not add `smog_golden_tests` to the `sim_types` contract.**

The four system goldens of `crates/client/tests/smog_golden_tests.rs` (startup, Visual Lab, swarm, live battle)
hash `format!("{instance:?}")` for every bubble: the whole `sim_types::snapshot::BubbleRenderInstance`, by its
field names. They also hash what the smog produces from it: the bubble's transform, the `BubbleUniformGpu` bytes
of its material (everything `BubbleUniformGpu::from_instance` derives for the shader), its visibility, and the
fluid grid. The instances are built by the test itself (`battle_unit`, the showcase and lab helpers), not by a
simulation, so the `Debug` term pins the spelling of a `sim_types` record and not smog output. The file's own
history proves it. Each of its four re-records for a `sim_types` change (`anim_progress` → `attack_swing`;
`cloaked` → `concealment` plus three fields; the four `StatusFlags` fields) states that no smog output changed.

- **Adding the test to the `sim_types` contract** (21 s wall in the last `full` stamp) would catch the next such
  change earlier, but it keeps the false coupling. Every new `sim_types` field would still turn the goldens red
  and force a re-record that changes no output, now inside every sim package.
- **Hashing what the smog reads** removes that coupling. A test-local `hash_smog_inputs(h, instance)` hashes, by
  raw bits, exactly the fields the smog code reads or writes. Today that is `entity_id`, `shape`, `world_pos`,
  `ground_z`, `radius`, `velocity`, `heading_angle_rad` and `anim_state` (`crates/client/src/smog/mod.rs`:
  `setup_smog_volume_system`, `sync_bouncing_smog_bubbles_system`, `update_smog_field_system`; `still_frame_lanes` in `bubble_shader.rs`). WP4 re-derives the list from
  the code. Enum fields are hashed through an exhaustive `match` written in the test, with no `Debug` and no `_`
  arm, so a new variant fails to compile. Workspace clippy compiles every client test for every `package` gate,
  in about 100 s. The uniform bytes stay hashed, so any `sim_types` change that alters what the shader receives
  still fails the golden.

What remains coupled is real presentation behaviour: `from_instance`, which is client code, and the `sim_types`
helpers it calls, such as `Concealment::is_concealed`. A change to such a helper's meaning that alters uniform
bytes is not in any sim contract. `full` before the push catches it, as it does today. This is recorded as R-9,
with the red rule (blueprint §3.5 step 4) as the remedy if it ever happens.

The evaluation's other two boundary cases need nothing new. `battle_flipbook_tests` is already in the `sim_core`
contract. The scenario order enum matched in client tests breaks compilation, which workspace clippy catches
(`2e39536`). WP4 also audits every client test that compares a hash or a recorded golden. For each it records
whether it depends only on client code, is in a sim contract, or needs one, and it adds any missing contract to
`suites.ron`. The audit table goes in the evidence.

## 6. Tests first

Each WP writes its tests first and sees them fail where they add behaviour (charter §4). No test starts a GPU
child or a real gate run except where named.

### 6.1 `crates/gate/tests/selection_tests.rs` (real catalog and workspace)

The base side is synthesized in memory from the real files, so no Git history is needed. For a catalog test, the
real `suites.ron` text has one line removed and is parsed as the "before" catalog; the lockfile and manifest
tests do the same with their files.

- `a_new_downstream_reference_in_the_catalog_selects_that_target_not_the_complete_suite`
- `a_comment_only_catalog_edit_selects_only_the_gate_crate_and_the_charter_checks`
- `a_test_threads_change_selects_only_that_target_and_the_gate_crate`
- `a_rule_edit_selects_the_old_and_new_rule_targets`
- `a_global_or_docs_catalog_edit_selects_the_complete_suite`
- `an_unreadable_base_catalog_selects_the_complete_suite`
- `the_sim_fixtures_contract_selects_its_callers_without_a_list` (a `crates/sim_fixtures/src/lib.rs` change
  selects `sim_core/golden_replay_tests`, `client/lib` and `asset_baker/veo_tests`)
- `a_dev_dependency_only_change_selects_the_member_crate_not_the_complete_suite` (the real `Cargo.lock` and
  `crates/sim_core/Cargo.toml`, with `proptest` taken out of the "before" texts)
- `a_dev_only_change_selects_no_downstream_contract`
- `a_runtime_dependency_change_selects_the_complete_suite`
- `an_external_version_change_selects_the_complete_suite`
- `a_member_lock_change_without_a_dev_only_manifest_change_selects_the_complete_suite`
- `a_ci_workflow_change_runs_the_charter_checks_only`
- `a_gate_source_change_still_selects_the_complete_suite`
- `a_gate_test_change_selects_only_the_gate_crate`
- `the_path_only_form_still_selects_the_complete_suite_for_the_catalog_and_the_lockfile`

### 6.2 `crates/gate/tests/suite_catalog_tests.rs` (C22)

- `the_sim_fixtures_contract_is_computed_from_its_callers` (replaces the list comparison, §4.3)
- `callers_is_accepted_only_as_a_downstream_value`
- `every_exclusive_target_names_its_evidence_in_a_comment` (each `exclusive: true` line has a comment naming a log
  or evidence file)

### 6.3 Unit tests in `crates/gate/src`

- `catalog_diff.rs`: each row of §4.2's table on synthetic catalogs; reordering and comments give an empty diff;
  a rename is a removal plus an addition.
- `lockfile.rs`: parse members and external entries; `dependencies` entries of the form `"name"` and
  `"name version"`; a checksum change is external; an added external entry is external; a member-only diff names
  the members.
- `manifest_diff.rs`: `[dev-dependencies]`, `[target.'cfg(unix)'.dev-dependencies]` and `[[test]]` changes are
  dev-only; `proptest.workspace = true` added under dev is dev-only; `[dependencies]`, `[features]` and `[lints]`
  changes are runtime; whitespace and comments are invisible.
- `sources.rs`: `callers_of` finds a caller through a `#[path]` support file, ignores a mention in a comment or
  string, and excludes the key crate's own targets.
- `catalog.rs`: `@callers` parses only in `downstream`; `exclusive` and `timeout_s` parse; `timeout_s: Some(0)` is
  rejected.
- `schedule.rs` (pure replay, extending the existing `replay` helper with memory):
  `valgrind_runs_alone_with_no_other_job_even_the_gpu_lane`,
  `exclusive_and_unmeasured_jobs_run_first_and_alone_then_the_lanes_open`,
  `a_job_waits_until_the_running_estimates_leave_room_in_the_budget`,
  `a_job_above_the_budget_still_runs_when_nothing_else_does`,
  `live_memory_above_the_ceiling_admits_nothing_new_but_an_idle_run_still_starts_its_head`,
  `a_smaller_job_overtakes_one_that_does_not_fit`,
  `every_job_runs_exactly_once_over_many_seeded_random_runs` (500 runs from a fixed-seed generator written in the
  test, random classes, durations, peaks and live readings; checks lane limits, budget and completion),
  `one_lane_is_still_the_sequential_run_of_cargo_test`. The existing `valgrind_takes_two_general_lanes` is
  replaced, because its rule changes (L-2).
- `memory.rs`: `RssAnon` from `/proc/<pid>/status` text; `anon` from `memory.stat` text; the limit found by walking
  up from a session scope to a slice with a numeric `memory.high`; `max` everywhere falls back to `MemTotal`; the
  descendants of a pid in a synthetic ppid map, including a child that changed process group.
- `stamp.rs`: `a_stamp_without_peaks_still_parses`;
  `the_estimate_is_the_largest_peak_of_the_last_five_runs_of_this_host`; another host's stamps are ignored.
- `cli.rs`: `--jobs` beats `SPACE_SCUFFLE_GATE_JOBS`, which beats the default; `--memory-budget-gib` and
  `--memory-ceiling-gib` parse positive numbers; `--no-reuse`; `stop` parses with no options.
- `runner.rs`: the report's final line shows test-phase wall, the executables' sum and the overlap; a stopped run
  writes no stamp and is red.

### 6.4 `crates/gate/tests/lane_run_tests.rs` (new; real processes, no GPU)

The test binary re-runs itself as the fake test executable when `GATE_LANE_FAKE` is set, as the GPU harness
re-runs test children. The fake can sleep, allocate and touch N MiB, spawn a grandchild in a new process group,
write a marker and exit with a given status. `run_targets` is driven on a synthetic `Workspace`.

- `three_lanes_overlap_and_each_target_writes_only_its_own_log`
- `the_peak_of_a_process_tree_counts_a_grandchild_in_another_process_group` (the fake touches 300 MiB in a
  grandchild; the recorded peak is at least 250 MiB)
- `a_target_past_its_timeout_is_killed_with_its_whole_tree_and_reported_timed_out` (no live process of the tree
  remains, read from `/proc`)
- `a_stop_request_kills_running_targets_skips_the_rest_and_writes_no_stamp`
- `a_failed_target_is_rerun_alone_after_the_lanes_finish`
- `progress_lines_are_whole_lines_and_lanes_tsv_lists_every_target`

### 6.5 `crates/gate/tests/gate_lock_tests.rs`

- `stop_with_no_holder_says_so_and_exits_zero`
- `stop_writes_a_request_naming_the_holder_and_the_holder_ends_red_without_a_stamp` (through the real binary,
  with a tier whose selection is a fake slow target)
- `a_stale_stop_request_is_removed_when_the_next_gate_takes_the_lock`
- `a_new_holder_warns_about_live_processes_of_the_previous_holders_group`

### 6.6 `crates/client/tests/smog_golden_tests.rs` (WP4)

- `a_bubble_field_the_smog_never_reads_does_not_move_the_system_hash`: an instance whose `status`, `stance`,
  `owner` and `barrier_ratio` differ hashes equal through `hash_smog_inputs` plus the uniform bytes (WP4 confirms
  each field against `from_instance` and drops any the uniform reads).
- `a_field_the_smog_reads_moves_the_system_hash` (`world_pos`, `radius`).
- The four system goldens re-recorded once, in the WP4 commit, which lists old and new values. Proof that no smog
  output changed: on the same tree, the old hashing still reproduces the old values. With only the `Debug` term
  replaced, every non-system golden (fluid, kinematics, noise, canary) is unchanged, and the bubble uniform bytes
  and transforms are byte-identical, checked by a temporary comparison run whose output goes in the evidence.

## 7. The A/B measurement

**Tree.** `main` after WP1 to WP6 have merged: a clean checkout of one commit in a worktree with its own
`CARGO_TARGET_DIR` on `~/scratch`. Both runs use the complete suite, the largest case and the one the evaluation's
estimate is for. Both run back to back on the same day, under the gate lock, with no other gate queued; the
orchestrator holds packages back for about 2.5 h.

**Protocol** (`~/scratch/gate-ab/<date>/`):

1. Warm the build once: `cargo gate full --no-run`. Both runs then start from the same linked executables.
2. **Run A (sequential):** `cargo gate full --jobs 1 --no-reuse --stamp-dir ~/scratch/gate-ab/<date>/stamps
   --log-dir ~/scratch/gate-ab/<date>/A`. It records every target's wall, CPU and peak. `--no-reuse` (new, WP5)
   stops Amendment 3 from reusing a green stamp of the same tree. Copy A's stamp aside.
3. **Run B (lanes):** `cargo gate full --jobs 3 --no-reuse` with the same stamp directory, so its estimates and
   longest-first order come from A, and `--log-dir …/B`.
4. During both runs, a bounded sampler (`timeout 4h`, one line every 30 s) logs the load average, the slice's
   `anon` and `file`, and `/proc/pressure/io`. It stops when the gate exits, so no process outlives the
   measurement.

Running A first means B starts with A's executables in page cache. Pre-reading (L-6) gives both runs a warm start
anyway, and the evidence reports the slice's `file` figure at each start. If the two runs' median load averages
differ by more than a factor of two, the pair is repeated once at a quieter hour, and both pairs are reported.

**Acceptance** (all must hold; B against A):

| Criterion | Threshold |
|---|---|
| Test-phase wall time (executables, from `lanes.tsv`) | at least 40 % lower |
| Whole-gate wall time | reported, no threshold (build, clippy and commands are unchanged) |
| Results | every target that passed in A passes in B; no `FLAKY` and no `TIMED OUT` in B. A load-sensitive flake may be fixed or marked `exclusive` with its evidence, after which B is repeated once |
| Slice `anon` peak during B | below 56 GiB (the ceiling), and reported against the estimate sum |
| GPU children | none hits the harness's 120 s run limit or the 290 s lock wait; the GPU lane's wall time within 15 % of A's |
| Per-binary slowdown | reported for every target (B wall / A wall); a target slowed more than 2× is named in the evidence with its overlapping targets |

If B misses the 40 % threshold but meets every other criterion, `--jobs 4` is measured once more, provided B's
`anon` peak stayed below 35 GiB. The better setting is adopted if it reaches 40 %. Otherwise the default stays 1,
the plan records the result, and the selection work (WP1 to WP4) stands on its own.

The evidence file, `docs/evidence/<date>-gate-lanes.md`, records the protocol, both result tables, the
`lanes.tsv` timelines, the memory and load samples, and the shared-path audit (§3.7). The logs stay under
`~/scratch/gate-ab/<date>/`.

## 8. Expected savings

These are estimates from the evaluation's window. WP6 measures the lane figures.

| Change type (window count) | Today | After rules 2 and 3 | After lanes too |
|---|---|---|---|
| `suites.ron` edit with no client change (5 of the 9) | 70 to 169 min | as their selective siblings, 23 to 27 min | about 15 to 20 min |
| `suites.ron` edit in a package that also changes client code (4) | complete | the package's own selection | about −30 to −45 % |
| Sim dev-dependency (4, spellcaster) | complete | sim selection, about 30 to 60 min | about 20 to 35 min |
| CI workflow edit (1) | complete | charter checks, about 1 min | the same |
| Selective sim gates (8) | 21 to 64 min | unchanged | about −10 to −25 min |
| Client-only gates (2) | 74 to 94 min | unchanged | about −30 to −45 min |
| Complete gates that remain (packages spanning sim and client, runtime dependency changes) | 82 to 169 min | unchanged | test phase from about 6,500 s to 2,300 to 2,800 s |

Over the window, rules 2 and 3 turn 13 of the 14 complete gates selective and save about 5 to 7 h of the 36.4 h.
Lanes save about 40 to 55 % of the remaining test phase, roughly 8 to 11 h. Together that is about 13 to 17 h
(35 to 47 %), and the median `package` gate falls from 78 min to about 35 to 45 min. Gate-lock waiting shrinks in
proportion, because the lock was held at least 64 % of the time. WP4 adds no time: the smog goldens are no longer
re-recorded for sim spelling changes. The computed caller list (WP1) removes the hand edit from every new
`sim_core` test.

## 9. Work packages, in commit order

Each WP is one branch, with tests first and its own `cargo gate package`. Because every WP except WP4 changes
`crates/gate/src/**`, each of them selects the complete suite (§4.1). That is intended: the runner's changes need
it, and WP6 is the one deliberate complete-suite measurement. WP2 and WP3 have no runtime dependency on each
other's code and may be merged in either order. At most two building agents run at once (charter §4.3, Q0055).

| WP | Content | Depends on | Tests (§6) |
|---|---|---|---|
| WP1 | `@callers`: the `TargetRef` variant, `SourceIndex::callers_of`, the `sim_fixtures` entry replaced by `["@callers"]`, the C22 test rewritten, `explain` shows "downstream of sim_fixtures (calls sim_fixtures::)" | — | 6.2, `sources.rs`, `catalog.rs`, selection test on `sim_fixtures` |
| WP2 | Diff-aware `suites.ron`: `Git::show`, the `Change` input, `catalog_diff.rs`, `global` reduced to `crates/gate/src/**` for the gate (`crates/gate/Cargo.toml` is covered by `crates/*/Cargo.toml`), the "CI workflows" rule, `explain` output | WP1 | 6.1 (catalog and workflow rows), `catalog_diff.rs` |
| WP3 | Diff-aware `Cargo.lock` and member manifests: `lockfile.rs`, `manifest_diff.rs`, `toml_edit` (already locked) as `gate`'s dependency, the cause texts, `explain` output | WP2 (the `Change` input) | 6.1 (lockfile and manifest rows), `lockfile.rs`, `manifest_diff.rs` |
| WP4 | Smog contract: `hash_smog_inputs`, the four system goldens re-recorded with proof; the audit of client hash and golden tests, with any missing contract added to `suites.ron` | — (client tests only) | 6.6 |
| WP5 | Lane runtime: `memory.rs` and the sampler; peaks in stamps; exclusive jobs (L-2), memory admission (§3.3), `exclusive`/`timeout_s` in the catalog; timeouts and `Outcome::TimedOut`; `cargo gate stop`, the holder's process group and the stale-group warning; pre-read; progress lines, `lanes.tsv`, the report's final line; `--no-reuse`, `--memory-budget-gib`, `--memory-ceiling-gib`, `SPACE_SCUFFLE_GATE_JOBS`. The default stays `--jobs 1`. CI's step sets `SPACE_SCUFFLE_GATE_JOBS: 1` | WP2 | 6.3 (schedule, memory, stamp, cli, runner), 6.4, 6.5 |
| WP5a | The shared-path audit (§3.7) and its fixes in client, `client_diagnostics` and `asset_baker` tests | WP5 | each fix proven by running the two colliding binaries at once |
| WP6 | The A/B measurement (§7) and its evidence | WP1 to WP5a merged | — (measurement) |
| WP7 | If accepted: the default becomes the accepted lane count (cli doc and usage text; `RunOptions::default`); this plan's results section; the blueprint gains Amendment 6 pointing here and superseding its §2.6 conclusion and §6 row "Concurrent binaries" | WP6 | `cli.rs` default test |
| WP8 | Bookkeeping: charter §7's memory figures updated to the current slice (66 GiB `memory.high`; keep `anon` below about 56 GiB); plan index row and status | WP7 | `workspace_checks` |

## 10. Risks

- **R-1. GPU contention.** The GPU lane runs one binary at a time, as `--jobs 1` does today, and its children still
  take the shared lock. The new load is CPU and disk from the general lanes, which can push a GPU child toward its
  120 s limit (the 2026-11-10 child timeouts were page-cache misses). Mitigations: pre-read (L-6); the A/B
  criterion on GPU children; if needed, start general-lane executables under `nice -n 5`, measured as a variant
  before adoption.
- **R-2. Memory.** Thread caps bound each binary; the budget bounds their sum; the live ceiling covers other work in
  the slice; unmeasured jobs run alone. Peaks grow with every Veo batch; the five-run maximum follows them. The
  sampler's 1 s period can miss a sub-second spike, which the 10 GiB gap between the ceiling and `memory.high`
  absorbs.
- **R-3. Interleaved output.** Logs are per target and progress lines are printed whole under a mutex. Shared fixed
  paths between binaries are the real risk and are audited (§3.7, WP5a).
- **R-4. Flake classification.** Load-only failures make `package` red (a flake never makes it green). Reruns stay
  alone, flakes name their overlap, `timings` counts flakes under overlap, and the fix is a test fix or an
  `exclusive` mark with evidence. The A/B accepts no new flake.
- **R-5. Disk.** Three cold executables paging in at once on the rotational scratch disk would be slower than one.
  Pre-read turns that into sequential reads. The build step, 10 % of gate time, is unchanged and stays out of
  scope (host decision).
- **R-6. Selection misses from rules 2 and 3.** Mitigations: the tests of §6.1; `full` on `main` before each push;
  CI on every push; the red rule (blueprint §3.5 step 4) adds the missing rule in the fixing commit.
- **R-7. Dev-dependency feature unification** (§4.4). A dev-only change could alter another crate's workspace test
  build through unified features. Accepted, with `full` and CI as the backstop. If it ever happens, the rule
  narrows to "dev-only and the lockfile's member entries unchanged".
- **R-8. Orphans.** A gate killed by PID leaves its lanes' executables running, as one executable is left today. The
  stop command, the printed group kill and the next holder's warning make that visible and avoidable. Nothing is
  killed automatically.
- **R-9. Residual smog coupling** (§5). A change to the meaning of a `sim_types` helper that `from_instance` calls
  is caught only by `full`. Remedy if it happens: the red rule.
- **R-10. Measurement noise.** The other user's load reaches us through the disk. The A/B logs load and I/O
  pressure and repeats once when the two runs' loads differ more than twofold.

## 11. Decisions and out of scope

- **D-1.** Lanes reuse the existing scheduler and runner; no `cargo-nextest` (blueprint §6.1 still applies).
- **D-2.** `crates/gate/src/**` stays global (§4.1). Splitting it into "execution" and "selection" modules, with
  the latter selecting only the gate's tests, saves little: two gates in the window. It is deferred.
- **D-3.** Removed downstream references select nothing (§4.2).
- **D-4.** `valgrind` runs alone instead of on two lanes (L-2), as the orchestrator asked: its children use up to
  one core each, and it is about 2 min per gate.
- **D-5.** The smog contract is fixed by hashing consumed bytes, not by a wider sim contract (§5).
- **D-6.** No owner question. This is the orchestrator's decision under the owner's standing request for less
  waiting. The charter's rules on what runs (`package`, `full`, CI) are unchanged.
- **D-7. Out of scope:** the speed of the slow client battle drivers themselves (evaluation item 4, about 52 % of
  test time; consolidation is Q0034's open option); host decisions (solid-state scratch, page cache outside
  `memory.high`, evaluation item 6); `sccache` (item 7); lanes on CI (needs lavapipe peaks on a 4-CPU, 16 GB runner;
  a follow-up once local lanes are accepted); running the golden-replay commands alongside the lanes; narrower
  rules for `crates/*/clippy.toml`; the workspace split (evaluation item 8, not recommended).

## 12. Results

Filled in by WP6 and WP7.
