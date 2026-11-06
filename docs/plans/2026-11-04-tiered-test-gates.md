# Tiered test gates: run what a change can break, run everything on a schedule

Date: 2026-11-04. Status: Active, WP1 to WP6a implemented (§11); Amendments 2, 3 and 4 implemented. Host: the shared Debian 13 VM, 16 pinned cores of an AMD
EPYC 7301, AMD Radeon RX 580 on `radv`; other agents were building and testing at the same time, as they always
are. Charter sections touched: §4.3 (rewritten), §5 (one new crate), §9 (C16 changed, C22 added). §6 is not
touched.

## 1. Owner request

The owner wrote, in chat on 2026-11-04:

> our gates checks are too strict. It isn't necessary that every change passes all gates. Devise a plan to reduce
> time "wasted" on tests. Find a good middle-ground.

Today charter §4.3 asks every agent to run, before completing any task, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked`. This plan
replaces that single gate with four tiers that differ in what they test and when they run, keeps the complete
suite as a safety net that still runs often, and encodes the tiers in a tool and a data file, so every agent
selects tests the same way.

Goals:

1. An implementation package spends minutes, not most of an hour, on tests before it hands back.
2. No test is deleted, disabled or marked `#[ignore]` to save time. `cargo test --workspace --locked` stays the
   complete suite, and CI keeps running it on every push.
3. Every regression the complete suite would catch is caught within hours, on `main`, before it is pushed.
4. The selection is mechanical, written as data, and itself tested.

## 2. Measurements

### 2.1 Sources

- **Agent logs.** `~/scratch/logs/` holds 1,227 logs from 2026-11-03 and 2026-11-04. 61 of them are complete
  workspace test runs (at least 140 test binaries reported). For each, the per-binary `finished in` times were
  summed and attributed to their crate from the `Running unittests src/lib.rs (…/deps/<crate>-<hash>)` lines that
  open each package's block. 107 clippy logs and 20 orchestrator merge logs were read the same way. The scripts
  were throwaway and are not part of this plan.
- **One timed run of my own**, at `95456b3` merged with `main` at `1940231`, in a fresh target directory
  (`~/scratch/cargo-target/plan-gates`, deleted afterwards): a cold `cargo test --workspace --locked --no-run`, then
  every test executable run one after another (as `cargo test` does), then the same executables run concurrently
  (§2.6), each with its wall time and CPU time taken from `wait4`. Executables were started the way Cargo starts
  them: working directory the package root, `CARGO_MANIFEST_DIR` and `CARGO` set. The per-binary tables
  (`seq.tsv`, `par4.tsv`) are kept in `~/scratch/logs/plan-gates-measurement/`.
- **The merge history:** the 45 merge commits on `main` since 2026-11-03, classified by the paths they change.

### 2.2 The complete suite

| Quantity | Value |
|---|---|
| Test binaries (unit, integration, bins; doc tests excluded) | 152 executables, plus 8 doc-test runs that take under 1 s together |
| Tests | about 1,910, of which 10 are `#[ignore]`d GPU children launched by their parent test |
| Cold test build (`--no-run`, empty target directory) | 414 s (6.9 min) in my run; the logs show 12 to 26 min when several agents build at once |
| Test build before a run (61 logged runs, warm or partly warm) | 1.8 s to 26.6 min; most between 1.5 and 8 min, the long ones when several agents built at once |
| Test run time, sum of binaries (61 logged runs) | 941 s to 2,167 s; median 1,254 s on 2026-11-03 and 1,571 s on 2026-11-04 |
| Test run time, my sequential run | 1,336 s (22.3 min), 152 executables, all green; 5,172 s of CPU time |
| Clippy, workspace, all targets (107 logged runs) | median 28 s, p75 99 s, p90 198 s, max 321 s |

A complete gate therefore costs about 25 to 35 minutes of wall time when the build is warm, and 35 to 55 minutes
when it is not.

### 2.3 Where the run time goes

Per crate, five representative logged runs (seconds of test run time):

| Crate | gate3 (combat WP5/6) | gate2 (combat WP5/6) | anim WP3 | anim WP5 | open WP9 |
|---|---|---|---|---|---|
| `client`, headless integration tests | 426 | 420 | 821 | 704 | 551 |
| `client`, GPU tests (13 binaries through `gpu_run`) | 479 | 341 | 406 | 615 | 362 |
| `client`, unit tests (`src/lib.rs`, about 290 tests) | 186 | 110 | 112 | 125 | 107 |
| `client`, valgrind (`sync_allocation_count_tests`) | 101 | 81 | 117 | 118 | 116 |
| `asset_baker` | 257 | 195 | 209 | 233 | 195 |
| `sim_core` | 122 | 78 | 95 | 100 | 83 |
| `client_diagnostics` | 50 | 24 | 62 | 39 | 65 |
| `sim_types`, `sim_map`, `sim_fixtures`, `workspace_checks` together | 7 | 6 | 6 | 6 | 6 |
| **Total** | **1,628** | **1,255** | **1,829** | **1,942** | **1,485** |

The `client` crate is 74 to 80 % of the run. The simulation crates, which most correctness work changes, are 5 to
7 %. The slowest binaries across these runs (seconds, range over the five runs):

| Binary | Class | Seconds |
|---|---|---|
| `client` unit tests | headless | 107 to 186 |
| `client/battle_flipbook_tests` (9 tests) | headless battle driver | 109 to 135 (two runs) |
| `client/sync_allocation_count_tests` (valgrind) | valgrind | 81 to 118 |
| `client/battle_input_tests` | headless battle driver | 50 to 120 |
| `client/gpu_validation_tests` | GPU | 63 to 163 |
| `client/battle_card_tests` | headless battle driver | 47 to 100 |
| `client/skirmish_match_tests` (4 tests) | GPU | 53 to 148 |
| `client/sync_steady_state_tests` | headless | 55 to 100 |
| `client/battle_fog_tests` | headless battle driver | 37 to 93 |
| `client/battle_hud_tests` | headless battle driver | 50 to 80 |
| `client/battle_context_order_tests` | headless battle driver | 35 to 83 |
| `client/editor_app_tests` | GPU | 54 to 98 |
| `asset_baker/procedural_tests` (4 tests) | ffmpeg | 41 to 64 |
| `asset_baker/loop_check_tests` | ffmpeg | 39 to 52 |
| `asset_baker/veo_cli_tests` | process spawning | 34 to 58 |
| `client_diagnostics/frame_budget_tests` | GPU | 14 to 53 |
| `sim_core/sc2_who_wins_matchup_matrix_tests` | whole matrix | 21 to 38 |

The variance is load, not the code: the same binary at the same commit differs by a factor of two between runs.

My own sequential run, by class (§3.2) and crate, wall and CPU seconds:

| Crate and class | Binaries | Wall s | CPU s | CPU / wall |
|---|---|---|---|---|
| `client` slow (unit tests and the battle drivers) | 10 | 557 | 2,383 | 4.3 |
| `client` GPU | 13 | 317 | 891 | 2.8 |
| `asset_baker` slow | 3 | 117 | 274 | 2.3 |
| `client` valgrind | 1 | 96 | 1,014 | 10.6 |
| `asset_baker` fast | 17 | 74 | 172 | 2.3 |
| `sim_core` (all) | 49 | 79 | 207 | 2.6 |
| `client` fast | 19 | 55 | 175 | 3.2 |
| `client_diagnostics` GPU | 2 | 35 | 35 | 1.0 |
| everything else (`sim_types`, `sim_map`, `sim_fixtures`, `workspace_checks`, `client_diagnostics` fast) | 38 | 6 | 19 | — |
| **Total** | **152** | **1,336** | **5,172** | **3.9** |

### 2.4 How often the complete suite runs

| Day | Complete runs in the logs | Their test run time | Merges that changed a crate |
|---|---|---|---|
| 2026-11-03 | 22 | 7.9 h | 12 |
| 2026-11-04 (to 20:15) | 39 | 17.0 h | 17 |

That is about 2.1 complete runs per merged package that changes a crate, each about 25 minutes plus its build:
about an hour of wall time per package spent in the gate. The repeats come from the rule itself: a package runs the
gate, merges `main`, and runs it again; a red run (usually a fault in the package's own crate) costs a third.
Plan-only packages (16 of the 45 merges) already ran only `cargo test -p workspace_checks`.

The orchestrator's re-verification on `main` is already targeted: the 20 most recent merge logs ran 2 to 64 test
binaries, 18 s to 351 s of tests, plus 4 s to 1.5 min of build. It costs 1.5 to 6 minutes per merge.

### 2.5 What the complete runs caught

Seven of the 61 runs had failing binaries. Every failure was in one of three places:

| Run | Failing binaries | Where the change was |
|---|---|---|
| `p9-wp12-gate1` | `asset_baker/runtime_catalog_tests`; `client/round2_visual_tests::test_zero_hardcoded_creature_names_in_rust_src` | catalog and source scan across crates |
| `p9-wp12-gate2` | `client/runtime_battle_tests`; `sim_core/scenario_tests` | same package, its own crates |
| `veo_wp2_ws1` | `client/lab_scene_tests` | package changed `asset_baker`, `assets/` and `client` |
| `combat_wp4_probe_tests` | four `sim_core` binaries | `sim_core` (own crate) |
| `anim_wp1_test2` | `workspace_checks` C7 | charter check |
| `open_wp9_gate_2` | `workspace_checks` C11 | charter check |
| `anim_wp4_test1` | `client/headless_tests`, `client/smog_golden_tests` | `client` (own crate) |

Every failure was caught by one of: the tests of a crate the package changed, `workspace_checks`, or a test that
reads shipped data (`assets/catalog`, `assets/media`) the package changed. None was a test of a crate the package
did not touch, reached only through the dependency graph. This is a small sample, and §8 treats the remaining risk.

### 2.6 Running test binaries concurrently

`cargo test` runs its binaries one after another. Many binaries cannot use 16 cores: `skirmish_match_tests` has 4
tests, `procedural_tests` 4, `battle_flipbook_tests` 9, the GPU binaries spend most of their time in one child
process at a time, and the valgrind binary runs its children itself. My run measured the CPU time of each binary:

- **Sequential (as `cargo test`):** 1,336 s wall, 5,172 s CPU: on average 3.9 of the 16 cores were busy.
- **Four lanes, GPU binaries in one lane** (started 20:49, while another agent's complete gate, `combat-wp7`, and
  other builds ran on the same 16 cores): 1,555 s wall, **slower** than sequential. Every binary took about 2.5
  times its sequential wall time for the same CPU time; the 15 GPU binaries spent 1,448 s of wall time for 934 s
  of CPU, waiting for the GPU lock that the other gate's children also took. One test failed that had passed
  sequentially: `skirmish_match_tests::the_skirmish_hud_and_foundation_render_cleanly_on_a_real_window`
  (`crates/client/tests/skirmish_match_tests.rs:347`, "the ring at 0.35 changed by 65.9, a built one by up to
  24.0"), a pixel comparison between frames that is sensitive to load.

Conclusion: on this VM the 16 cores are already shared by several agents' gates running at once, so running
binaries concurrently inside one gate gains nothing measurable today and exposes load-sensitive tests. The time
must come from **running fewer tests, fewer times**, not from packing them tighter. The runner keeps a `--jobs`
option, default 1, and WP2 re-measures it at a quiet hour (§6).

### 2.7 The dependency graph does not narrow much

Reverse dependencies, from `cargo metadata` and charter §5:

| A change in | reaches (normal and dev dependencies) |
|---|---|
| `sim_types` | every crate |
| `sim_map`, `sim_core` | `sim_core`, `asset_baker`, `client`, `client_diagnostics`, `sim_fixtures` |
| `sim_fixtures` | `sim_core`, `asset_baker`, `client`, `client_diagnostics` (all as a dev-dependency) |
| `asset_baker` | `client`, `client_diagnostics` |
| `client` | `client_diagnostics` |
| `client_diagnostics`, `workspace_checks` | nothing else |

Of the 29 crate-changing merges since 2026-11-03, 9 changed `client` only, 7 `asset_baker` only (and `assets/`),
7 `sim_core` (with `sim_fixtures`) or more, 6 several crates. A gate of "changed crates and all their reverse
dependencies' tests" would run the `client` suite, 75 % of the total, for 26 of 29 packages. It would save
almost nothing. Compilation of reverse dependencies is a different matter: `cargo clippy --workspace --all-targets`
already compiles every target and takes a median 28 s, so a broken downstream API is caught by clippy without
running a single downstream test.

## 3. Design

### 3.1 Principles

- **P1. Compile everything, test what the change can break.** Clippy over the whole workspace in every required
  tier catches every downstream compile break for under a minute. Tests are selected.
- **P2. The selection is data.** A catalog file maps paths to test targets. A tool reads it. Agents do not choose
  tests by hand.
- **P3. Uncertain means more.** A path no rule covers selects the complete suite. Workspace-wide inputs
  (`Cargo.toml`, `Cargo.lock`, `.cargo/`, toolchain and lint configuration) select the complete suite.
- **P4. The complete suite still runs, on a schedule.** On `main` before every push, in the background, and on CI on
  every push. Its result decides whether `main` is pushed.
- **P5. Nothing is hidden from `cargo test --workspace`.** No `#[ignore]` for slow tests (that attribute already
  marks GPU children, which must never run on their own), no features that turn tests off. Tiers select test
  targets; the complete suite runs all of them.

### 3.2 Test classes

Every test target of the workspace (a `lib` or `bin` unit-test target, an integration test file, a bench run as a
test) has exactly one class in the catalog. The class decides in which tiers a target runs when its crate changes.

| Class | Meaning | Members today |
|---|---|---|
| `fast` (default) | a run under about 20 s on an idle machine, no GPU, no valgrind | all `sim_types`, `sim_map`, `sim_fixtures`, `workspace_checks` targets; all `sim_core` targets except one; most `asset_baker` and `client_diagnostics` targets; the cheaper `client` integration files |
| `slow` | headless but long: whole battles, whole matrices, ffmpeg | `client` unit tests; `client/battle_*_tests` that drive the shipped skirmish (`battle_card`, `battle_card_fixture`, `battle_context_order`, `battle_flipbook`, `battle_fog`, `battle_hud`, `battle_input`, `battle_result`); `client/sync_steady_state_tests`; `sim_core/sc2_who_wins_matchup_matrix_tests`; `asset_baker/procedural_tests`, `loop_check_tests`, `veo_cli_tests` |
| `gpu` | launches children through `crates/client/tests/support/gpu_run.rs` | the 13 `client` integration files that call into `gpu_run` (`battle_hud_window`, `battle_result_window`, `combat_fx_slot_gpu`, `dome_fill`, `editor_app`, `flipbook_clock_gpu`, `gpu_validation`, `lab_scene`, `round2_visual`, `runtime_battle`, `skirmish_match`, `smog_bouncing_bubbles`, `terrain`), `client_diagnostics/frame_budget_tests`, and the `client_diagnostics` unit tests (its `shell_order_audit::gpu_tests`) |
| `valgrind` | counts allocations under valgrind | `client/sync_allocation_count_tests` |

The class list is checked (§5.4): a target the catalog does not list is `fast`, and the catalog test fails if a
`gpu` target is missing (a target that calls `run_test_child` or `GpuCommand` must be classed `gpu`) or if the
catalog names a target that does not exist.

### 3.3 The four tiers

| Tier | Who, when | Runs | Typical wall time |
|---|---|---|---|
| `quick` | anyone, as often as useful; never required | `cargo fmt --all -- --check`; clippy on the changed crates only (`-p`); the `fast` targets of the changed crates; `workspace_checks` | 1 to 4 min |
| `package` | every implementation agent, **once**, after merging `main`, before its final commit and hand-back | fmt; clippy, whole workspace, all targets; `workspace_checks`; every target of every changed crate (all classes, see the GPU rule below); the targets the catalog's path rules select (§4); for a simulation change, the golden replay in release and on `wasm32-wasip1`, and `cargo check` on `wasm32-unknown-unknown` | 8 to 28 min (§7); 1 to 2 min for docs only |
| `merge` | the orchestrator, on `main`, for each merged branch | fmt; clippy, whole workspace; `workspace_checks`; plus, only if `main` moved since the branch's `package` stamp in a crate the branch also selects, the `fast` and `slow` targets of the `package` selection | under 2 min, or 3 to 8 min with overlap |
| `full` | the orchestrator on `main`, in the background, before every push that contains a crate change; CI on every push | fmt; clippy; the complete suite (`cargo test --workspace --locked`, run by `cargo gate`); release and `wasm32-wasip1` golden replays | 25 to 35 min locally (22 min of tests in my run, plus clippy, build and goldens) |

The GPU rule in `package`: the `gpu` and `valgrind` targets of a changed crate run only when the change touches a
path that their catalog entry lists as a trigger, or the target's own file. For `client` the GPU triggers are the
renderer, shader, window and HUD-window code (§4.2); a change in, say, `client/src/battle_input/` runs the
headless battle drivers but not the 13 GPU binaries. Docs-only changes (`docs/**`, `*.md`) run `quick`'s fmt and
`workspace_checks` only, which is what plan agents do today.

### 3.4 Stamps: making "trust the package gate" checkable

When a tier passes, `cargo gate` writes a stamp: `~/scratch/gate-stamps/<tree-hash>.ron` with the tier, the tree
hash of the verified working tree (`git rev-parse HEAD^{tree}` with a clean tree; the tool refuses to stamp a
dirty tree), the `main` commit it was merged with (`git merge-base HEAD main`), the selected targets, their wall
times and the host. Stamps are not committed: they are local evidence, under `~/scratch` by charter §7.

- The implementation agent's hand-back names its stamp.
- `cargo gate merge <branch>` walks the branch's commits from its head back to the merge base and takes the newest
  one with a green `package` stamp for its tree. If there is none, or a later commit on the branch changes a file
  outside `docs/` and `*.md`, it runs the `package` tier on the merge result itself and says so. If the stamp
  exists, it computes which crates `main` changed between the stamp's base and the current `main`; only if that
  set intersects the branch's selection does it run the selected `fast` and `slow` targets (semantic conflicts of
  two independently green branches live there).
- `cargo gate full` writes a stamp keyed by the `main` commit; the push rule (§3.5) reads it.

### 3.5 When `full` runs, and what red means

- **Before every push of `main`** that includes a crate change since the last green `full` stamp, the orchestrator
  runs `cargo gate full` in a dedicated worktree (`~/scratch/worktrees/main-verify`, its own target directory), in
  the background, and keeps merging meanwhile. It pushes the commit that the green stamp names, not later ones.
  Pushes happen "at regular intervals" (owner rule); with about one push every one to two hours, `full` runs 6 to
  10 times a day instead of 39.
- **CI** keeps running the complete suite and the determinism matrix on every push (Q0005: not required, look
  regularly, fix when red).
- **Red rule.** A red `full` run or a red CI run on `main`:
  1. The tool prints, for each failing target, the merges since the last green stamp whose changed paths the
     catalog maps to that target, newest first; that is the suspect list.
  2. The orchestrator stops merging packages whose selection includes the failing target's crate.
  3. Fix forward within one hour (a new small package with its own blueprint amendment, as today), or revert the
     suspect merge with `git revert -m 1 <merge>` and hand the package back. Whichever comes first.
  4. Every failure that `full` caught but `package` did not select is a **catalog miss**. The fix adds the missing
     path rule to `crates/gate/suites.ron` in the same commit, and the plan of the package records it. That
     closes the loop: the catalog learns from each miss.
- **Flakes.** A failure that passes on an immediate rerun of the same target is recorded in the stamp as a flake,
  is not treated as green, and gets a finding; the `full` run is repeated once. Two flakes of one target in a week
  are a bug to fix, not to rerun.

### 3.6 What `package` no longer does

- It does not run twice. Merge `main` into the branch first, then gate once. If `main` moves again before the
  hand-back, the agent does not rerun; the `merge` tier handles overlap.
- It does not run the tests of reverse dependencies wholesale (§2.7); it runs the catalog's downstream contracts
  (§4.3).
- It does not run GPU and valgrind targets for changes outside their triggers.

## 4. The suite catalog `crates/gate/suites.ron`

### 4.1 Shape

```ron
SuiteCatalog(
    // Paths whose change selects the complete suite (P3).
    global: ["Cargo.toml", "Cargo.lock", ".cargo/**", "rust-toolchain*", "clippy.toml", "crates/*/Cargo.toml",
             "crates/*/clippy.toml", ".github/workflows/**", "crates/gate/**"],
    // Paths that select nothing beyond fmt and workspace_checks.
    docs: ["docs/**", "**/*.md"],
    // Targets that are not `fast`, with their class and the extra paths that trigger them.
    targets: {
        "client/gpu_validation_tests": (class: Gpu, triggers: ["crates/client/tests/support/gpu_run.rs"]),
        "client/sync_allocation_count_tests": (class: Valgrind, triggers: ["crates/client/src/battle_pipeline/**",
            "crates/client/src/presentation.rs", "crates/client/src/runtime_battle/**"]),
        "client/battle_flipbook_tests": (class: Slow, triggers: []),
        // ...
    },
    // Path rules: a change under `paths` selects `targets` (in any crate) and `crates` (all their targets
    // except Gpu and Valgrind ones without a matching trigger).
    rules: [
        (name: "shaders", paths: ["assets/shaders/**"], targets: ["client/@gpu", "client/headless_tests",
            "client/round2_visual_tests", "client_diagnostics/@gpu"]),
        // ...
    ],
    // For each crate: the downstream targets a change in it selects (§4.3).
    downstream: { "sim_core": [...], ... },
)
```

`client/@gpu` names every target of a class in a crate. Globs are Git's pathspec globs, matched against paths
relative to the repository root.

### 4.2 Initial path rules

The implementation writes these rules, then validates them against history (WP4) before they are trusted.

| Rule | Paths | Selects |
|---|---|---|
| crate sources | `crates/<c>/**` | every `fast` and `slow` target of `<c>`, plus its `gpu`/`valgrind` targets whose triggers match; plus `downstream[<c>]` |
| test support | `crates/client/tests/support/**` | every `client` target that declares `mod support` (all classes) |
| GPU renderer | `crates/client/src/{bubble_shader.rs,bubble_optics.rs,render_compatibility.rs,flipbook_clock.rs,vfx.rs,vfx/**,terrain/**,smog/**,camera.rs,lab_stage.rs,scene_controller.rs,editor/**}` | `client/@gpu` |
| HUD windows | `crates/client/src/battle_hud/**`, `crates/client/src/battle_presentation.rs` | `client/battle_hud_window_tests`, `client/battle_result_window_tests` |
| live sync | `crates/client/src/{battle_pipeline/**,presentation.rs,runtime_battle/**,player_presentation/**}` | `client/sync_allocation_count_tests`, `client/skirmish_match_tests`, `client/runtime_battle_tests` |
| shaders | `assets/shaders/**` | `client/@gpu`, `client_diagnostics/@gpu`, `client/headless_tests`, `client/round2_visual_tests` |
| catalog | `assets/catalog/**` | `sim_core` (all `fast`/`slow`), `sim_fixtures`, `asset_baker/{runtime_catalog_tests,media_tests,provenance_tests,prompt_tests}`, `client/{player_presentation_tests,runtime_media_tests,lab_scene_tests,round2_visual_tests,battle_card_tests}`, the golden replays |
| maps and scenarios | `assets/maps/**`, `assets/scenarios/**` | `sim_map`, `sim_core`, `sim_fixtures`, `client/{runtime_battle_tests,skirmish_match_tests,battle_fog_tests,editor_core_tests}` |
| media | `assets/media/**`, `assets/videos/**`, `assets/textures/**`, `assets/ai_keyframes/**`, `assets/source_art/**` | `asset_baker/{media_tests,bake_integrity_tests,provenance_tests,terrain_assets_tests}`, `client/{runtime_media_tests,lab_scene_tests,terrain_tests}` |
| captures | `assets/captures/**` | `client_diagnostics` (all `fast`) |
| renderer tool | `tools/renderer_compatibility/**` | complete suite until a narrower rule is written (P3) |
| `DESIGN.md`, `AGENTS.md`, `docs/plans/README.md` | — | `workspace_checks` (C15 to C20 read them) |

### 4.3 Downstream contracts

The tests a change in a crate selects in its reverse dependencies. They are the tests that exercise the crate's
behaviour through its consumer, not merely compile against it.

| Changed crate | Downstream targets |
|---|---|
| `sim_types` | all of `sim_map`, `sim_core`, `sim_fixtures` (all `fast`, under 2 min together); `client/{headless_tests,pipeline_presentation_tests,player_presentation_tests,simulation_diagnostics_tests}`; `asset_baker/runtime_catalog_tests` |
| `sim_map` | `sim_core` (all `fast` and `slow`), `sim_fixtures`; `client/{editor_core_tests,editor_entity_placement_tests,mesher_golden_tests,runtime_battle_tests}` |
| `sim_core` | `sim_fixtures`; `asset_baker/runtime_catalog_tests`; `client/{pipeline_presentation_tests,player_presentation_tests,simulation_diagnostics_tests,battle_result_tests,skirmish_match_tests}` |
| `sim_fixtures` | the targets of `sim_core`, `asset_baker` and `client` that call `sim_fixtures::` (found by the catalog test, §5.4) |
| `asset_baker` | `client/{runtime_media_tests,runtime_assets_tests,lab_scene_tests}`; `client_diagnostics/palette_fixture_tests` |
| `client` | `client_diagnostics` (all `fast`; its `gpu` targets by trigger) |

`skirmish_match_tests` is downstream of `sim_core` on purpose: it is the whole-match proof through input, and
combat and economy changes are what it exists to catch.

### 4.4 The `fast` boundary

20 s is the boundary because the median warm clippy is 28 s: a `quick` run should cost about as much as a clippy.
The tool's timing report (§5.3) lists any `fast` target whose median over its last 10 runs exceeded 30 s, and any
`slow` target under 10 s, so the classes follow the code rather than this table.

## 5. The tool: `crates/gate`

### 5.1 Why a crate and not a shell script

The selection logic (globs, graph closure, stamps, scheduling) needs tests to meet charter §4, and a shell script
cannot be tested to that standard. A small binary crate fits the workspace's conventions (typed errors, `//!`
docs, clippy, C1 to C21) and runs anywhere Cargo runs. An alias in `.cargo/config.toml` makes it short:

```toml
[alias]
gate = "run --quiet --locked --package gate --"
```

`gate` depends on no other workspace crate (it reads `cargo metadata` and Git as data, as `workspace_checks`
does), so a broken simulation crate cannot break the gate. Its dependencies are already in `Cargo.lock`:
`serde`, `serde_json`, `ron`, `thiserror`, plus `tempfile` for tests. No new crate is downloaded.

### 5.2 Commands

```text
cargo gate quick                  # optional inner loop
cargo gate package                # required before hand-back; writes a stamp
cargo gate merge <branch>         # orchestrator, after `git merge --no-ff <branch>` on main
cargo gate full                   # complete suite; writes a stamp keyed by the commit
cargo gate explain [<base>]       # prints the selection and the rule behind each target; runs nothing
cargo gate timings                # class report from recent stamps (§4.4)
```

Common options: `--base <rev>` (default: `git merge-base HEAD main`), `--jobs <n>` (default from §5.3),
`--log-dir <dir>` (default `~/scratch/logs/gate/<tree-hash>/`), `--no-run` (select and build only).

Changed paths are `git diff --name-only <base>` (committed and uncommitted) plus untracked files that are not
ignored. A tier refuses to run with `CARGO_TARGET_DIR` unset in a worktree under `.claude/worktrees/` (charter §7).

### 5.3 Running the selection

1. Build: one `cargo test --locked --no-run --message-format=json` invocation with `-p` and `--lib`/`--bins`/
   `--test <name>` for exactly the selected targets (`--workspace` for `full`). Clippy runs before it, once.
2. Run: the executables from the build's JSON, each started as Cargo starts it (working directory = package root,
   `CARGO_MANIFEST_DIR`, `CARGO`, `RUST_BACKTRACE` inherited), stdout and stderr to one log file per target.
3. Concurrency: a pool of `--jobs` lanes, default 1 (sequential, as `cargo test`; §2.6). With more lanes, `gpu`
   targets share one dedicated lane, so at most one GPU binary runs at a time and no other lane waits for it
   (my experiment in §2.6 let GPU binaries block general lanes; the runner must not). The harness already
   serializes children on `/run/lock/gpu.lock`, and two GPU binaries at once would only make children wait toward
   their 290 s lock timeout. The `valgrind` target
   takes two lanes, since it runs its own children at up to one per core. Longest-first by the last stamp's
   timings, so the tail is short.
4. Doc tests: `cargo test --doc` for the selected crates after the binaries (under 1 s today).
5. Result: a table of every target with class, wall time, CPU time and result; on failure, the failing tests' names
   and the path of each log, and for `full`, the suspect merges (§3.5).

The runner never wraps anything in `with-gpu-lock`; GPU children lock themselves (charter §7).

### 5.4 The catalog test (charter check C22)

`crates/gate/tests/suite_catalog_tests.rs` checks, against `cargo metadata` and the repository:

- every target the catalog names exists; every `@class` reference names a class with at least one member;
- every test file that uses `run_test_child`, `GpuCommand` or `virtual_gpu_command` is classed `gpu`; every file
  that invokes `valgrind` is classed `valgrind`;
- every `downstream` key is a workspace crate and every value a target of one of its reverse dependencies;
- every tracked path matches a crate rule, a `docs` glob, a `global` glob or a path rule (an unmatched path would
  silently select the complete suite, which is safe but slow; the test makes the omission visible);
- `full` selects every test target `cargo metadata` lists, so `cargo gate full` and `cargo test --workspace` run
  the same tests.

Unit tests in the crate cover the pure parts: glob matching, the reverse-dependency closure on synthetic graphs,
selection per tier on synthetic catalogs, lane scheduling, stamp round trip, and refusal of a dirty tree.

## 6. Making the complete suite itself cheaper

| Measure | Expected effect | Decision |
|---|---|---|
| Concurrent binaries (§5.3) | none measurable under today's shared load (§2.6: 1,555 s with four lanes against 1,336 s sequential); on an idle machine, 3.9 of 16 cores busy suggests up to 2 to 3 times | Keep `--jobs`, default 1; WP2 measures `--jobs 2` and `4` three times each at a quiet hour and changes the default only if the complete suite is at least 25 % faster with no new failure |
| Gate once instead of twice per package | about 30 min per package | Do it (charter text, WP5) |
| `opt-level = 1` for the `client` package in the dev profile | the headless battle drivers spend their time in unoptimized `client` systems; their deps and the simulation are already at `opt-level = 2` | Measure first (WP6): adopt if the `slow` client targets lose at least 30 % of run time and the warm rebuild of `client` grows by less than 60 s |
| Remove the duplicate catalog-slug scan from `client/round2_visual_tests` | `test_zero_hardcoded_creature_names_in_rust_src` scans every crate's sources for catalog slugs, which C5 in `workspace_checks` already does; living in a `gpu` binary, it makes catalog changes select a GPU target | WP6: compare with C5 on the same inputs; if C5 covers it, delete the duplicate, else move the difference into C5 |
| `#[ignore]` for slow tests with a dedicated runner | none of value: `#[ignore]` already marks GPU children, which must not run on their own, and `cargo test --workspace` would stop being the complete suite (P5) | Rejected |
| Consolidating the client integration tests into one binary | faster link and less disk | Not proposed: the owner chose to keep separate binaries (Q0013, Q0034) |
| `cargo-nextest` | see §6.1 | Not now |

### 6.1 `cargo-nextest`

`cargo install cargo-nextest --locked` downloads source from crates.io and compiles it, so it does not break the
rule against running binaries downloaded from the network. I do not recommend it now:

- Its speed comes from running binaries and tests concurrently. §2.6 measured that this gains nothing under the
  VM's shared load, and the in-repository runner can do the same with no new dependency. Nextest also runs every
  test in its own process; for this suite that adds about 1,900 process start-ups, many of them Bevy apps, and
  changes the behaviour of tests that share state inside a binary or re-run their own binary by test name (the
  `gpu_run` children).
- It brings its own dependency tree, resolved outside the workspace's `Cargo.lock`, which every agent machine and
  CI would have to build and keep at one version; the workspace's supply chain is otherwise pinned by
  `Cargo.lock`.
- Its features this plan needs (filter by package and test, test groups for the GPU lane) are a few hundred lines
  in `crates/gate`; its features this plan does not need (retries, JUnit, archive and partition) do not pay for
  the above.

Reconsider it if per-test timeouts or retries become necessary; it would then be an owner question, because it
widens the supply chain.

## 7. Expected savings

Numbers are wall time on this VM under its usual shared load, from §2.

### 7.1 Per implementation package

| Package kind (share of the 29 crate merges) | Today: about 2.1 complete gates | With `package` once | Saved |
|---|---|---|---|
| `sim_core` or `sim_map` change (7 of 29) | about 60 min | 178 s of tests + about 4 min of build and clippy + 1.5 min of release and wasm goldens: about 9 min | about 50 min |
| `asset_baker` change, with `assets/` (7 of 29) | about 60 min | 225 s (code only) to 361 s (with the catalog) of tests + about 4 min: 8 to 10 min | about 50 min |
| `client` change without renderer paths (about half of 9) | about 60 min | 614 s of tests (803 s when it touches the live sync and so selects valgrind and the skirmish) + about 4 min: 14 to 17 min | about 45 min |
| `client` renderer or shader change (about half of 9) | about 60 min | 1,062 s of tests + about 4 min: about 22 min | about 38 min |
| several crates, including `client` renderer paths (6 of 29) | about 60 min | up to the complete suite once: 25 to 30 min | about 30 min |
| docs only (16 of 45 merges) | `workspace_checks` | the same | none |

### 7.2 Per merge, and per day

- **Merge tier:** today 1.5 to 6 min per merge; with a valid stamp and no overlap, fmt + clippy + `workspace_checks`,
  about 1 to 2 min warm. With overlap it is about what it is today.
- **Full tier:** 6 to 10 runs a day (one per push) of 25 to 35 min each, in the background, instead of 39
  foreground runs of the same length.
- **Per day at the 2026-11-04 rate** (17 crate merges): complete-gate time drops from about 17 h of test run time
  (plus builds) to about 4.5 h of foreground `package` time (4 simulation packages at 9 min, 4 `asset_baker` at
  9, 5 `client` at 17, 4 multi-crate at 28) plus 3 to 5 h of background `full` runs that block no one: about 9 h
  of machine time instead of about 20, and an agent waits 9 to 28 min for its gate instead of about an hour.
  Fewer gates at once also means less contention for the 16 cores and the GPU lock, which §2.6 shows slows
  every gate running at the same time.

## 8. Risks and mitigations

| Risk | Mitigation |
|---|---|
| A package passes `package` and breaks a test the catalog did not select | `full` before every push and CI on every push catch it within hours; the red rule (§3.5) finds the suspect merge from the catalog, and each miss adds a rule; WP4 replays history before the catalog is trusted |
| Two branches, each green, break each other on `main` | the `merge` tier reruns the selection when `main` moved in an overlapping crate; `full` before push |
| `main` is red between a merge and the next `full` | pushes only at a green `full` stamp, so the remote `main` is red only when CI finds what the VM cannot (other platforms, lavapipe); revert within an hour |
| The catalog rots as files are added | the catalog test (C22) fails on stale names, unclassed GPU files and unmatched paths; the timing report flags misclassified targets |
| Agents keep running the complete suite out of habit | the charter text says what each tier is for, and `cargo gate package` prints the time it saved against the last `full` stamp |
| Load-sensitive tests (§2.6: a `skirmish_match_tests` pixel comparison failed under load) | the runner's default stays sequential; fewer gates at once lower the load; WP6 makes that test's thresholds load-independent or records why it cannot be; the flake rule (§3.5) catches the rest |
| GPU lock waits grow when several agents run `package` at once | one GPU lane per runner and GPU targets only on trigger reduce the number of GPU children per package from about 25 to the few the change needs |

## 9. Charter changes

### 9.1 §4.3, replacing item 3

```markdown
3. **Quality gates.** [CI: C16, C22] Tests are selected by `cargo gate`, from the catalog
   `crates/gate/suites.ron`, in four tiers:
   - `cargo gate quick`: optional, while working: fmt, clippy on the changed crates, their fast tests.
   - `cargo gate package`: required once, after merging `main`, before an implementation agent's final commit.
     Clippy on the whole workspace, `workspace_checks`, every test of the changed crates (GPU and valgrind tests
     when the change touches their triggers), the tests the catalog's path rules and downstream contracts select,
     and the golden replays in release and on `wasm32-wasip1` for simulation changes. Its stamp goes in the
     hand-back.
   - `cargo gate merge <branch>`: the orchestrator, for each merge to `main`.
   - `cargo gate full`: the full gate, on `main` before every push that contains a crate change, and on CI. Push
     only a commit with a green full stamp. When it is red, fix forward or revert within an hour, and add the
     missing catalog rule.
   - **Full gate** (what `cargo gate full` and CI run):
     - `cargo fmt --all -- --check`
     - `cargo clippy --workspace --all-targets --locked -- -D warnings`
     - `cargo test --workspace --locked`
   - Docs-only changes run `cargo fmt --all -- --check` and `cargo test --locked -p workspace_checks`.
```

The §4 test-subject table gains a row: `gate` | selection, globs, scheduling, stamps, catalog consistency |
`selection_tests.rs`, `suite_catalog_tests.rs`. §5 gains the node `GT["crates/gate<br/>(tiered test gates)"]` with
no edges to other workspace crates (C1). §6 is not changed.

### 9.2 C16, changed, and C22, new

C16 today requires that every `cargo` code span in §4 appears as a command in `.github/workflows/ci.yml`. With the
tiers in §4.3, the spans `cargo gate …` are not CI commands. The new C16:

- every `cargo` span in the **Full gate** list of §4.3 (the list under the line that starts with `- **Full gate**`)
  is a command in `ci.yml`, as before;
- every other span of the form `cargo gate <tier>` names a tier that `crates/gate` defines (the tier names are
  parsed from `crates/gate/src/tier.rs`'s `Tier` enum, which `workspace_checks` reads as text, as it reads other
  sources);
- any other `cargo` span in §4 is a finding, so a new gate command cannot appear without a check.

C22 (§9 table): "`crates/gate/suites.ron` names existing targets, classes every GPU and valgrind test, and covers
every tracked path", verified by `crates/gate/tests/suite_catalog_tests.rs`.

### 9.3 CI

CI keeps its commands. It must not depend on the selection logic: if the catalog had a bug, CI would still run
everything. One addition: the job summary gets the per-binary timing table (`cargo gate timings --from-log`
parses the `cargo test` output), so CI shows the same class report as the VM. No new download.

## 10. Work packages

| WP | Content | Depends on | Gate for the WP itself |
|---|---|---|---|
| WP1 | `crates/gate` skeleton: `Tier`, catalog types and parser, glob matching, `cargo metadata` reverse-dependency closure, change detection from Git, `explain` command; unit tests; §5 graph node; §4 table row | — | complete suite (the tiers do not exist yet) |
| WP2 | Runner: build of the selection, lanes (`--jobs`, default 1; a GPU lane and a double-width valgrind slot when above 1), logs, doc tests, result table; `full` writes stamps; measure `--jobs 2` and `4` at a quiet hour (§6) | WP1 | complete suite, and `cargo gate full` green |
| WP3 | Catalog content: classes (§3.2), path rules (§4.2), downstream contracts (§4.3); `suite_catalog_tests.rs` (C22) | WP1 | complete suite |
| WP4 | Historical replay: for each of the 45 merges since 2026-11-03 and the 7 red runs of §2.5, `cargo gate explain <merge>^1..<merge>` must select every target that failed in that package's logged runs; record the table in this plan's §11 and fix the rules until it passes | WP3 | `cargo gate package` + complete suite once |
| WP5 | Tiers `quick`, `package`, `merge`; stamps; red-rule suspect list; charter §4.3 text (§9.1), C16 change and C22 row (§9.2); `.cargo/config.toml` alias; CI timing summary | WP2, WP3, WP4 | complete suite (the last package gated the old way) |
| WP6 | Cheaper suite: `opt-level = 1` measurement for `client` with its acceptance rule; the duplicate slug scan in `round2_visual_tests` versus C5; the load-sensitive threshold at `skirmish_match_tests.rs:347` (§2.6) | WP5 | `cargo gate package` |
| WP6a | Compiler-crash retry (Amendment 1, D5): when a build or clippy step's log contains `the compiler unexpectedly panicked`, the runner deletes that target directory's `debug/incremental` (and `release/incremental` for release steps), reruns the step once, and records the retry in the result table and the stamp; a second crash stays red. Unit test with a fake step | WP5 | `cargo gate package` |
| WP7 | One week of use: compare catalog misses, `full` red runs and gate time per package against §7; adjust the `fast` boundary and the default jobs; record in §11 | WP5 | — |

Each WP has its tests written first and seen to fail where it adds behaviour, per §4.

## 11. Results

WP7 adds its week of use here.

### 11.1 WP4: the historical replay

Evidence: [`docs/evidence/2026-11-04-tiered-gates-replay.md`](../evidence/2026-11-04-tiered-gates-replay.md), with
the table of every merge. `cargo gate explain m^1..m` for the 45 most recent first-parent merges of `main` at
`a151620`, plus Phase 9 WP12 (`56f243f`) for the two red runs of §2.5 that belong to it, compared with every
failing target in the logs of each package (red-first tests and mutation checks included, not only the complete
runs):

| Quantity | Value |
|---|---|
| Merges replayed | 45 + 1 |
| `package` selection: subset / docs only / complete suite | 26 (26 to 132 of 156 targets, median 60) / 14 / 5 |
| Failing targets in the packages' logs | 65 |
| Of them in the seven red complete runs of §2.5 | 15, all selected |
| Not selected | 1: `client/battle_input_tests` in Veo loops WP2 (`79f53f5`, `asset_baker` only) |
| Catalog misses | 0 |
| Rules added | none |

The one unselected failure is a flake, not a miss: Bevy's embedded tonemapping look-up table failed to
decompress (`AccLogTooBig`) once in 1,288 logs, and the same binary passed in the package's next two complete runs.
No file of the change reaches that code; a rule for it would add a headless battle driver to every `asset_baker`
package for nothing. Two corrections to §2.5 from the logs: `combat_wp4_probe_tests` failed three `sim_core`
binaries and `sim_fixtures/trace_tests`, and `p9-wp12-gate1` also failed `client/runtime_battle_tests` and
`sim_core/scenario_tests`; all are selected.

### 11.2 WP5: the tiers in the charter

What WP5 added, and where it departs from the text above:

- **Charter.** §4.3 is the text of §9.1, with one change: its last bullet reads "Docs-only changes:
  `cargo gate package` runs fmt and `workspace_checks` only" instead of naming `cargo test --locked -p
  workspace_checks`. The new C16 (§9.2) makes every `cargo` span outside the Full gate list a finding unless it is
  a `cargo gate <tier>`, and that command is neither; the `package` tier selects exactly fmt and the two
  `workspace_checks` targets for a docs-only change (§11.1: 14 of the 45 merges). §9 has C16 reworded and the C22
  row; §6, §7 and §8 are unchanged.
- **C16** (`crates/workspace_checks/src/charter_claims.rs`) checks the Full gate list against `ci.yml`, every
  `cargo gate <tier>` against the variants of `pub enum Tier` in `crates/gate/src/tier.rs` (read as text), and
  reports any other `cargo` span. Two additions the plan did not spell out: a bare `cargo gate` (the tool named in
  prose) is accepted, and a CI step's command matches up to a pipe or redirection, so the test step can keep its
  log.
- **CI.** The test step runs `cargo test --workspace --locked 2>&1 | tee "$RUNNER_TEMP/cargo-test.log"` under
  `shell: bash` (pipefail, so a red run stays red); a new step, `if: always()` and `continue-on-error: true`,
  appends `cargo gate timings --from-log` to the job summary: totals per class, every binary by time, and the
  classes this one run contradicts. CI still runs nothing selected (D2).
- **Flakes** (§3.5). Every tier reruns each failed target once, at once and alone (logs in `rerun/`). A target
  that passes then is `FLAKY`: not green, with a note to record a finding. A `full` run red only by flakes is
  repeated once, and the repeat's stamp records them (`flakes`); `cargo gate timings` lists every target with two
  flakes in seven days. The whole-run repeat is limited to `full`, as §3.5 says; a red-by-flake `package` run is
  rerun by its agent.
- **Red rule** (§3.5, step 1). A red `full` run prints, per failing target, the first-parent changes of `main`
  since the last green `full` stamp that is an ancestor of `HEAD` (else the 50 newest) whose `package` selection
  includes it, newest first; a target no change selects is marked as a catalog miss or a flake. For a red CI run,
  `cargo gate suspects --from-log <log>` (or with target names, `--base <rev>` to choose the start) prints the
  same list from the CI log. This command is not in the plan's §5.2; the red rule needs it for CI.
- **Time saved** (§8). `quick`, `package` and `merge` print the test run time saved against the newest `full`
  stamp, or say that there is none yet.
- **Not done here:** WP5's own gate was to be the complete suite (§10); it ran `cargo gate package` instead, at
  the orchestrator's instruction, with the package tier run by hand because a change to `crates/gate/**` selects
  the complete suite (see the hand-back).

### 11.3 WP6a: the compiler-crash retry

`crates/gate/src/crash.rs` (Amendment 1, D5). Every fixed Cargo step of a tier (clippy, the test build, doc tests,
and the catalog commands, which compile too) runs through `run_with_crash_retry`: when the step fails and its log
contains `the compiler unexpectedly panicked`, the log is kept as `<step>.crash.log`, the incremental caches the
step used are deleted, and the step runs once more. The caches are `debug/incremental`, plus
`release/incremental` for `--release`, plus the same under `<triple>/` for `--target <triple>`, all under the
target directory `cargo metadata` reports (`Workspace::target_directory`); compiled dependencies are kept. A second
crash, or any failure without that text, is returned as it is. The result table marks the step "retried once after
a compiler crash", a red one names the kept first log, and a green stamp lists retried steps in a new field
`retries` (older stamps read as none). The test build's JSON output is not logged, so any stdout line carrying the
crash text is copied into `build.log`; rustc prints its crash report to stderr, which reaches the log anyway.

Tests first: `crash::tests` drive a fake step that writes its log (crash then pass: green, retried, cache gone,
dependencies kept, first log kept; crash twice: red after exactly two attempts; a failure without the text and a
passing step: one attempt, nothing deleted) and the cache list per profile and target. They were red against a
stub that never retried. `runner::tests` cover the table and the stamp's list; `workspace::tests` the target
directory from metadata.

### 11.4 WP6: the cheaper suite

**The duplicate slug scan.** `client/round2_visual_tests::test_zero_hardcoded_creature_names_in_rust_src` searched
every `.rs` file under a `src/` directory of `crates/` for each **unit** slug as a plain, case-sensitive substring of
the whole text. C5 (`workspace_checks/src/identities.rs`) compares the slugs of all five identity catalogs and the
former enum variants, folded (case, `_`, `-` and spaces ignored), with whole identifiers and whole string
literals of production code. On the same inputs:

| Input | Duplicate | C5 before | C5 after |
|---|---|---|---|
| The repository at this branch | clean | clean | clean |
| `spawn_lantern_moth`, `"assets/units/lantern_moth/idle.mp4"` (a unit slug inside a longer identifier or literal) | reported | **missed** | reported |
| `LANTERN_MOTH_RANGE`, `StarLeagueBanner`, `"hive_dome_ready"` (other spellings, other catalogs) | missed | missed | reported |
| `"slantern_mothy"` (the slug's letters inside other words) | reported | not reported | not reported |
| a slug in a comment, a `#[cfg(test)]` module under `src/`, or `sim_fixtures`/`workspace_checks` sources | reported | not reported | not reported |
| `LanternMoth`, `"Lantern Moth"`, any structure, upgrade, loadout or faction slug, a former enum variant | missed | reported | reported |

The one production case only the duplicate caught, an identity inside a longer name, is now in C5: an identity of
two or more words is also reported as a run of whole words of an identifier or literal (words split at
non-alphanumerics and lower-to-upper case changes). A one-word identity (`charge`) stays whole-only, since it is an
ordinary word too. The cases left only to the duplicate are ones the charter permits and C5 deliberately skips
(comments, test code, the dev-only crates) or false matches. The new unit test
`a_multi_word_identity_inside_a_longer_name_or_literal_is_reported` was red against the old C5; the repository has
no finding under the extended check. The duplicate and its helper are deleted (D3 allows it). The `catalog` path
rule still selects `client/round2_visual_tests`, because four of its other tests read the shipped unit catalog.

**The skirmish threshold** (`skirmish_match_tests.rs`, the ring comparison of §2.6). The failure at load ("a built
one by up to 24.0") came from the dome's flipbook advancing with wall time between the two shots. Battle flipbook
WP5 (2026-11-04, after the §2.6 run's base) made the flipbook clock presented simulation time, which stops while
the child holds the battle paused; a slow frame can add at most Bevy's 250 ms virtual-time cap (about six ticks)
between the shots, and `lavapipe`, whose frames take seconds, always runs at that cap, so it bounds what any load
can do. Measured on 2026-11-05 at load averages of 17 to 27: built rings changed by 0.3 and 0.4, then 1.2 and 1.4
on radv (two runs), and 1.1 and 1.7 on lavapipe, against 60.1 to 68.8 for crossed rings (the old runs: 6.7 to 18.7
against 68.9 to 85.0). The threshold is therefore load-independent as the code stands; no code change was needed,
and the reason and numbers are now in the doc comment of `CROSSED_RATIO`. Logs:
`~/scratch/logs/gates_wp6_skirmish_before_{1,2,lvp}.log`.

**`opt-level = 1` for `client`.** Not adopted: the rule's run-time half is not met. Measured on
2026-11-05 in one target directory (the dependencies are shared; only `client` and its test crates differ), with
`--config 'profile.dev.package.client.opt-level=1'` for the candidate, runs back to back in pairs: a warm rebuild
(`touch crates/client/src/lib.rs`, then `cargo test -p client --no-run`), then each of the ten `slow` client
targets through `cargo test`. Another agent's builds ran on the same cores throughout; the 1-minute load average
on 16 cores was 9 to 410.

| Pair | Variant | Warm rebuild s | `slow` targets s | Of which `client/lib` | Load during the targets |
|---|---|---|---|---|---|
| 1 | `opt-level = 0` (today) | 125.6 | 644.8 | 181.9 | 51 to 409 |
| 1 | `opt-level = 1` | 570.1 | 473.4 (−26.6 %) | 44.6 | 18 to 41 |
| 2 | `opt-level = 0` | 84.2 | 556.6 | 110.5 | 9 to 22 |
| 2 | `opt-level = 1` | 40.2 | 407.2 (−26.8 %) | 43.5 | 15 to 43 |
| 3 | `opt-level = 0`, rebuild only | 42.8 | — | — | 88 |
| 3 | `opt-level = 1`, rebuild only | 55.0 | — | — | 110 |
| 4 | `opt-level = 0`, rebuild only | 94.0 | — | — | 132 |
| 4 | `opt-level = 1`, rebuild only | 110.1 | — | — | 112 |

- **Run time:** the `slow` client targets lost 26.6 % and 26.8 %, short of the 30 % the rule asks. Both pairs agree
  although their loads leaned in opposite directions (pair 1 favoured the candidate, pair 2 the baseline), so the
  shortfall is not an artefact of one pair; a single variant still moved by 16 % between its two runs, so a true
  gain a little above 30 % cannot be excluded, but nothing measured reaches it. Almost all of the gain is the unit
  tests (`client/lib`, −61 % and −75 %); the nine other targets together lost 7 % and 18 % (single targets moved between +10 % and −43 %),
  since their time is mostly in the simulation and Bevy, which are already at `opt-level = 2`.
- **Warm rebuild:** growth of −44, +12 and +16 s in pairs 2 to 4, within the 60 s allowed; pair 1's +444 s is an
  outlier this measurement cannot explain (the same rebuild took 40 to 110 s in the other three pairs).
- **Verdict:** keep `opt-level = 0` for `client`. A narrower change, optimizing only the unit tests, is not
  possible per target in Cargo profiles and is not pursued (D4). Logs and per-target times:
  `~/scratch/logs/gates_wp6_opt_{cold,warm,r1,r2,r3,r4}_{o0,o1}.{tsv,log}`.

## 12. Decisions and scope

- **D1. No owner question.** The owner asked for this middle ground; the charter's §4.3 is the agents' rule and
  changes with this plan. §6 stays as it is.
- **D2. CI unchanged in what it runs.** It stays the complete safety net and does not use the catalog.
- **D3. No test is removed or ignored for speed.** WP6 removes the slug scan only if it duplicates C5.
- **D4. Not in scope:** consolidating client test binaries (Q0013, Q0034); `cargo-nextest` (§6.1); sharing one
  target directory between agents; making individual slow tests faster beyond WP6.
- **D5. Amendment 1 (orchestrator, 2026-11-05): compiler crashes are retried once.** On 2026-11-04/05 three gate runs
  went red only because rustc 1.99 panicked on a corrupted incremental cache (`try_mark_green`; `attrs_for_def`
  during clippy), in `main-merge` and `main-full` target directories that consecutive gates share. Clearing
  `debug/incremental` and rerunning was green each time. WP6a automates exactly that one retry; it does not turn
  incremental compilation off, because the warm rebuild is what keeps `quick` and `package` fast.

## Amendment 2 (orchestrator, 2026-11-05): one gate at a time, and capped test threads

**Cause.** On 2026-11-05 four `cargo gate` runs in four target directories ran at once. The client's headless battle
drivers, whose every test holds a whole Bevy app, ran with libtest's default of one thread per CPU (16) and reached
27 GiB (`battle_card_tests`), 16 GiB (`battle_hud_tests`) and 13.6 GiB (`battle_input_tests`) of RSS. The user's
cgroup (`user-1002.slice`, `memory.high` 44 GiB, no swap) went over its limit, the kernel throttled the whole
slice and the server froze. GPU children then overran their 120 s run limit only as a symptom; the limit stays.

**Decision.**

- **The gate lock.** `cargo gate quick|package|merge|full` take an exclusive `flock(2)` on `~/scratch/gate.lock`
  (`SPACE_SCUFFLE_GATE_LOCK` overrides the path) before their first step and hold it until they end
  (`crates/gate/src/gate_lock.rs`). A waiting gate prints one line naming the holder: the holder writes its PID,
  tier, worktree and start time into the file once it has the lock, and clears them before it releases it; a holder
  without a record (`flock(1)`) is named from `/proc/locks`. `explain`, `timings` and `suspects` never lock. A gate
  that holds the lock sets `SPACE_SCUFFLE_GATE_LOCK_HELD` to the lock's path for every process it starts, so a
  nested `cargo gate` does not wait for its parent; and a gate whose ancestor holds the lock (`flock
  ~/scratch/gate.lock cargo gate package`) runs inside that hold instead of deadlocking. The kernel releases the
  lock when the holder dies, so a crash leaves no stale lock.
- **Capped test threads.** A `targets` entry of `suites.ron` may set `test_threads: Some(n)`; the gate runs that
  executable with `--test-threads=n`. The three battle drivers above get 4, from the measurement below. Other
  `slow` client binaries keep libtest's default until a measurement shows they need a cap. `cargo test
  --workspace` (CI, and the full suite outside the gate) is unchanged; on this server it is wrapped in `flock
  ~/scratch/gate.lock` (charter §7).
- **Charter §7** gains one bullet: one `cargo gate` or workspace `cargo test` at a time across all worktrees, the
  gate serializing itself through the lock, and a check of the slice's `anon` memory (`memory.stat`, below about
  36 GiB of the 44 GiB limit; `memory.current` also counts reclaimable page cache) before other heavy runs.

**Measurement.** Debug build of `aba9fa8`, one executable at a time under the gate lock, peak RSS read as `VmHWM`
from `/proc/<pid>/status` every 0.2 s, on the shared 16-core VM with other agents' work running. Logs:
`~/scratch/logs/gpu-child-budget/` (`results.txt` and one log per run).

| Binary (tests) | 1 thread | 2 threads | 4 threads | 16 threads (incident) |
|---|---|---|---|---|
| `battle_card_tests` (28) | 3.4 GiB, 435 s | 3.9 GiB, 200 s | 5.4 GiB, 100 s | 27 GiB |
| `battle_hud_tests` (33) | 3.3 GiB, 449 s | 3.9 GiB, 221 s | 6.0 GiB, 113 s | 16 GiB |
| `battle_input_tests` (35) | 4.6 GiB, 712 s | 4.1 GiB, 326 s | 5.1 GiB, 124 s | 13.6 GiB |

Four threads keep each binary near 6 GiB, so a gate (which runs one executable at a time with the default
`--jobs 1`) stays far below the slice limit, at a quarter or less of the one-thread wall time. The 16-thread figures
are the incident's, not re-measured.

**Tests.** `gate_lock.rs` unit tests: the holder record round-trips; the waiting message names a recorded holder or
the `/proc/locks` process; a free lock is taken at once and the record is cleared on release; two holders
serialize and the second names the first; the marker skips the lock only for the same file; `/proc/locks` and
`/proc/<pid>/stat` lines parse. `tests/gate_lock_tests.rs`, through the real binary: a tier waits for a holder in
another process tree, prints the line naming it, and proceeds on release; a nested gate under the marker, a gate
under `flock` on the lock and a gate whose parent process holds the lock do not wait; `explain` never locks.
`catalog.rs` and `execute.rs` unit tests cover `test_threads` parsing, the rejection of 0, and the argument.

## Amendment 3 (orchestrator, 2026-11-05): test a tree once (owner, Q0055)

- `cargo gate merge` and `cargo gate full` first look for a green stamp whose recorded tree equals `HEAD^{tree}`
  (clean working tree only). `full` accepts a green `full` stamp or a green `package` stamp of the same tree; `merge`
  accepts any green `package`, `merge` or `full` stamp of the same tree. If one exists, the tier prints which stamp
  it reuses and exits green without building, and writes its own stamp that names the reused one. Otherwise it runs
  as before. `quick` and `package` are unchanged (an agent's package run is the test of record).
- Tests first: a stamp for the same tree is reused; a stamp for another tree, a red stamp, or a dirty working tree
  is not; the reuse message names the stamp. CI is unchanged and still runs the complete suite on every push.

- Implemented (2026-11-05): `Stamp::reusable` in `crates/gate/src/stamp.rs` finds the stamp (`full` prefers `full`
  over `package`; `merge` prefers `merge`, then `full`, then `package`) and requires a clean tree, the same tree hash
  and every recorded target passed; `run_tier` checks it after loading the repository and before any step. A reused
  run prints `cargo gate <tier>: tree <hash> already has a green <tier> stamp (<path>, commit <hash>); reusing it,
  nothing is built or run`, and writes its own stamp with no targets and `reused: Some("<file name>")`; when the
  reused stamp is the tier's own (a second `full` on a tree with a green `full`), that stamp is kept as it is.
  `--no-run` never reuses. Unit tests in `stamp.rs`: reuse of a same-tree stamp by `merge` and `full`, the
  accepted tiers per tier, and no reuse for another tree, a red stamp or a dirty working tree; the message names
  the stamp and the tree.

## Amendment 4 (orchestrator, 2026-11-05): a measured thread cap for every heavy client binary

**Cause.** On 2026-11-05 package gates ran uncapped client binaries at tens of GiB: `battle_context_order_tests`
reached 26.6 GiB and `editor_app_tests` 9.3 GiB (still growing), while only the three battle drivers of Amendment 2
had a cap. Every Veo batch adds atlases, so each Bevy app a test builds grows, and with it every binary's peak at
libtest's default of one thread per CPU. Separately, `tests/gate_lock_tests.rs` killed only the `flock` of its
competing holder, orphaning the `sleep 600` under it, which kept the real gate lock open for minutes through a
descriptor it inherited.

**Decision.**

- **Every `slow` or `gpu` client integration binary is measured, and each above about 6 GiB gets the
  `test_threads` that keeps it near 6 GiB in the least wall time.** The caps in `crates/gate/suites.ron`:

  | Binary | Cap |
  |---|---|
  | `battle_card_tests`, `battle_hud_tests`, `battle_input_tests` | 2 (was 4) |
  | `battle_card_path_tests`, `battle_fog_tests`, `battle_result_tests`, `sync_steady_state_tests` | 2 |
  | `battle_context_order_tests`, `battle_flipbook_tests` | 3 |
  | `smog_bouncing_bubbles_tests`, `skirmish_match_tests` (GPU) | 2 |
  | `lab_scene_tests` (GPU) | 4 |
  | `editor_app_tests` (GPU) | 2, a deviation: see below |

  The other binaries stay at libtest's default. `cargo test --workspace` (CI, and the full suite outside the gate)
  is unchanged.
- **`editor_app_tests` gets 2, not 1.** It is the only binary that no thread count above 1 brings near 6 GiB: at 2
  threads its process tree peaks at 8.6 GiB while a `map_editor --verify-render` child runs beside an in-process
  app, and 1 thread (6.2 GiB) doubles its wall time to 761 s, longer than any other client binary. With one gate at
  a time, 8.6 GiB is far below the slice's 36 GiB working limit.
- **The gate lock tests kill whole process groups.** Every child of `tests/gate_lock_tests.rs` (the competing
  `flock` holder and each `gate` under test) starts in its own process group (`process_group(0)`), and dropping it
  sends `SIGKILL` to the group through `kill(1)` (the workspace forbids `unsafe`, so not `libc::kill`) and then
  reaps the child. The holder's `sleep` lasts 60 s instead of 600, a backstop for a test process killed from
  outside. No other test spawns a long-lived helper that it kills by process ID alone: the GPU harness runs its
  children under GNU `timeout`, which signals the whole group.

**Measurement.** Debug build of `1e0a03d`, one executable at a time, each run under `flock -o
~/scratch/gate.lock`, with the environment the gate gives a test executable. Peak RSS is the largest sum of `VmRSS`
over the binary's process tree (its session and descendants, which includes GPU children under `xvfb-run`),
sampled every 0.2 s; the largest single-process `VmHWM` differs from it only for the GPU binaries, given in
brackets. The slice's `anon` memory was below 1 GiB before every run; other agents' work ran meanwhile. Logs:
`~/scratch/logs/test-caps/` (`all-results.txt` and one log per run). Every run passed every test.

| Binary (tests) | Class | 16 threads (default) | 4 threads | 3 threads | 2 threads | 1 thread | Cap |
|---|---|---|---|---|---|---|---|
| `battle_card_tests` (28) | slow | 28.0 GiB, 84 s | 9.0 GiB, 171 s | | 6.5 GiB, 318 s | | 2 |
| `battle_card_path_tests` (4) | slow | 7.2 GiB, 33 s | = default | | 3.8 GiB, 56 s | | 2 |
| `battle_context_order_tests` (24) | slow | 28.2 GiB, 65 s | 8.9 GiB, 128 s | 6.6 GiB, 170 s | 5.6 GiB, 255 s | | 3 |
| `battle_flipbook_tests` (9) | slow | 14.7 GiB, 89 s | 7.5 GiB, 97 s | 5.9 GiB, 116 s | 4.2 GiB, 169 s | | 3 |
| `battle_fog_tests` (20) | slow | 28.1 GiB, 81 s | 8.4 GiB, 130 s | 7.4 GiB, 151 s | 5.3 GiB, 233 s | | 2 |
| `battle_hud_tests` (33) | slow | 28.2 GiB, 88 s | 8.9 GiB, 169 s | | 6.2 GiB, 337 s | | 2 |
| `battle_input_tests` (35) | slow | 28.0 GiB, 103 s | 8.5 GiB, 195 s | | 5.9 GiB, 381 s | | 2 |
| `battle_result_tests` (18) | slow | 28.3 GiB, 61 s | 8.3 GiB, 105 s | | 6.1 GiB, 191 s | | 2 |
| `sync_steady_state_tests` (7) | slow | 12.4 GiB, 49 s | 8.8 GiB, 91 s | 7.1 GiB, 110 s | 5.3 GiB, 150 s | | 2 |
| `editor_app_tests` (40) | gpu | 27.3 GiB, 113 s | 10.3 [8.4] GiB, 195 s | | 8.6 [7.2] GiB, 370 s | 6.2 [4.4] GiB, 761 s | 2 |
| `lab_scene_tests` (10) | gpu | 11.8 [9.9] GiB, 48 s | 6.9 [6.6] GiB, 49 s | | 4.6 [3.6] GiB, 81 s | | 4 |
| `skirmish_match_tests` (4) | gpu | 7.2 [3.7] GiB, 60 s | = default | | 5.6 [3.7] GiB, 63 s | | 2 |
| `smog_bouncing_bubbles_tests` (25) | gpu | 16.3 [14.1] GiB, 40 s | 7.7 [6.9] GiB, 89 s | 7.7 [5.4] GiB, 100 s | 5.7 [3.7] GiB, 159 s | | 2 |
| `runtime_battle_tests` (18) | gpu | 5.9 [3.9] GiB, 76 s | | | | | none |
| `round2_visual_tests` (22) | gpu | 4.0 [3.5] GiB, 26 s | | | | | none |
| `battle_hud_window_tests` (1) | gpu | 3.7 GiB, 23 s | | | | | none |
| `gpu_validation_tests` (28) | gpu | 3.7 GiB, 107 s | | | | | none |
| `battle_result_window_tests` (1) | gpu | 3.6 [3.7] GiB, 24 s | | | | | none |
| `dome_fill_tests` (4) | gpu | 3.6 [3.5] GiB, 28 s | | | | | none |
| `terrain_tests` (14) | gpu | 0.7 [0.3] GiB, 13 s | | | | | none |
| `combat_fx_slot_gpu_tests` (1) | gpu | 0.3 [0.2] GiB, 2 s | | | | | none |
| `flipbook_clock_gpu_tests` (3) | gpu | 0.3 [0.2] GiB, 5 s | | | | | none |

The three battle drivers capped at 4 in Amendment 2 now reach 8.5 to 9.0 GiB at 4 threads (5.1 to 6.0 GiB then), so
they drop to 2. Where 4 threads overshoot and 2 threads undershoot, 3 threads were measured, and they are chosen
when they stay near 6 GiB (`battle_context_order_tests` 6.6, `battle_flipbook_tests` 5.9). `lab_scene_tests` keeps 4
(6.9 GiB in 49 s against 4.6 GiB in 81 s). `client/lib` holds unit tests, not an integration binary, and was not
measured.

**Tests.** `catalog.rs` and `execute.rs` already cover `test_threads`; the catalog test (C22) parses the new caps.
`tests/gate_lock_tests.rs` gains `dropping_the_holder_leaves_no_process_of_its_group`: it waits until the holder's
group holds both `flock` and `sleep`, drops the holder, and requires that no live (non-zombie) process of the group
remains, read from `/proc`. With the old drop it fails, naming the orphaned `sleep`.
