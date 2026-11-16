# Q0085: May the compacted AGENTS.md (diff below) be merged?

- **Asked:** 2026-11-16
- **Blocking:** no
- **Default until answered:** the compaction is not merged. AGENTS.md stays as it is on `main`; the work waits on
  branch `charter-compaction`.

**Where to look:** the proposal is the local branch `charter-compaction` (commit `281fd0e4`, not pushed), checked out at `/home/spacescuffle/space-scuffle/.claude/worktrees/agent-a8402d20c48fb1541`. The moved text is in `docs/guides/gpu-and-disk.md`, `docs/guides/test-gates.md` and `docs/guides/performance-rationale.md` there (`git diff main charter-compaction`). Only this question file is on `main`.

## Question

You answered Q0084 with option 2: prepare a lossless editorial compaction of the charter, move long how-to text to
linked files, and show the exact diff before merging. This is that diff.

**Size of `AGENTS.md`** (tokens estimated as bytes / 4):

| | Bytes | Words | ~Tokens |
|---|---|---|---|
| Before (`main`, 4e09981a) | 23,914 | 3,420 | ~5,980 |
| After | 20,357 | 2,907 | ~5,090 |
| Saved | 3,557 (15 %) | 513 (15 %) | ~890 per agent turn |

The saving is modest because about 5 KB of the charter are tables and lists that stay as they are: the §4
test-subject table (C15), the §4.3 **Full gate** list (C16), the §5 Mermaid graph (C1) and the §9 check table. §6
(1.4 KB) is unchanged, because its rules are yours. Section numbers and headings are unchanged, so the
roughly 400 references to "charter §N" elsewhere in the repository stay valid.

**What was done.** No rule, invariant or check tag was removed, weakened or added. Long operating detail moved,
word for word, to three new guides; the charter keeps the rule and links the guide:

- `docs/guides/gpu-and-disk.md`: all of the old §7 (commands, ICD files, lock file and log, timeouts, harness
  behaviour, memory and disk detail).
- `docs/guides/test-gates.md`: all of the old §4.3 tier detail (what each tier runs, the stamp rules, docs-only
  changes, thread caps).
- `docs/guides/performance-rationale.md`: the §2.3 example block.

The links in the charter are code spans, so check C20 now fails if a guide is moved or deleted. The intro gains one
sentence so that moved text keeps its force: *"A rule that links a guide under `docs/guides/` is followed as that
guide describes, and the guide binds like this charter."* That sentence is the only new wording with normative
weight; please check it in particular.

**Rule-by-rule mapping** (old place → where it lives now):

| Old place | Content | Now |
|---|---|---|
| Preamble | Binding charter, tags | Unchanged, plus the guide sentence above |
| §1 intro | Textbook-grade reference purpose | §1, one sentence |
| §1.1 | No todo/unimplemented/markers; do it right; no global mutable state; no unwrap; typed errors; `expect` with `invariant:`; no `let _ =` | §1.1, unchanged |
| §1.2 bullet 1 | Never fake media or systems in code, with the Veo, pixel-shape, wobbling JPEG and dummy-data examples [Review] | §1.2, shorter wording, same examples and tag |
| §1.2 bullet 2 | Stop and escalate when blocked; fail loudly, ask the user [Procedure] | §1.2, shorter wording, same cases and tag |
| §1.2 bullet 3 | Real end-to-end verification | §1.2, unchanged |
| §2.1 | `//!` module docs: responsibility, fit, invariants [C4] | §2.1, one paragraph |
| §2.2 | `///` docs [missing_docs]; units and spaces [Review] | §2.2, one paragraph |
| §2.3 | Idiomatic by default; PERFORMANCE RATIONALE block with Why/What/Trade-off [C11; Review] | §2.3 keeps the rule, the cases and the three labels; the example block is in `docs/guides/performance-rationale.md` |
| §2.4 | 100-line function limit [clippy, C8] | §2.4, unchanged wording |
| §3.1 determinism | Bit-for-bit identical on four targets | §3.1, one bullet (the crates are named in the §3.1 heading) |
| §3.1 rest | No floats [C7]; banned collections and clocks [C9]; golden replays on every target; no engine deps [C2]; throughput | §3.1, unchanged |
| §3.2 | Client layer rules | §3.2, unchanged |
| §3.3 | No hardcoded identities [C5, C6]; catalog file list; capability components; dynamic loading and zero per-frame allocations | §3.3, same rules; catalog files listed once under `assets/catalog/` instead of with the full path each time |
| §4.1, §4.2, table | Unit and integration tests; test-subject table [C15] | §4, unchanged |
| §4.3 tiers | `quick`, `package`, `merge`, `full`: when each is required and who runs it; the verifier hand-off (Q0061); green stamp before push; fix forward or revert within an hour | §4.3, one line per tier, same obligations |
| §4.3 tier contents | What `quick` and `package` run; docs-only `package` runs fmt and `workspace_checks` | `docs/guides/test-gates.md` |
| §4.3 test a tree once | Stamp reuse rules; merge one at a time, batch pushes, at most two building agents (Q0055) | §4.3 keeps a one-line stamp rule and the Q0055 rules; the exact stamp conditions (including the `package` stamp counting for `merge` and `full`) are in the guide |
| §4.3 Full gate | Three commands CI runs [C16] | §4.3, same three commands; the per-executable thread caps are in the guide |
| §5 | Graph [C1] and dependency notes | §5, unchanged |
| §6 | Two-stage workflow | §6, unchanged (owner-governed) |
| §7 intro | Shared VM, RX 580, software Vulkan | §7 one sentence; device node and driver names in `docs/guides/gpu-and-disk.md` |
| §7 use the GPU | Physical GPU by default | §7; the "about 1 % of the time" reason is in the guide |
| §7 lock per command | `with-gpu-lock`, one process or capture at a time, also for a binary with no driver selected | §7; command line, rationale (VRAM, resets, timings) in the guide |
| §7 lock timeout | 600 s default, raise only for one long command; lock file and log (Q0060) | §7 keeps the rule; lock file, log and `GPU_LOCK_TIMEOUT=1200` in the guide |
| §7 software Vulkan | Fallback only (CI or reproducing CI), no lock; `WGPU_BACKEND=vulkan` required | §7 keeps both rules; ICD paths and the OpenGL explanation in the guide |
| §7 GPU tests | Harness locks per child; never wrap `cargo test --workspace` in the lock | §7 keeps the rule; driver selection, timeouts, `SPACE_SCUFFLE_GPU_TEST_DRIVER` in the guide |
| §7 one gate at a time | One `cargo gate` or workspace `cargo test`; `flock ~/scratch/gate.lock`; memory below ~56 GiB | §7 keeps the rule; the cgroup file and the page-cache note in the guide |
| §7 disk | Hot builds on the home disk, per-worktree target directory, cold files on `~/scratch`, remove merged worktrees | §7 keeps the rule; quota command and main checkout's `target/` in the guide |
| §8 | Owner questions, scoping, StarCraft II mechanics | §8, unchanged |
| §9 | Tags and check table | §9, unchanged |

**One trade-off to know.** The tier contents now in `docs/guides/test-gates.md` (for example "`cargo gate quick`:
fmt, clippy on the changed crates, their fast tests") are no longer under check C16, which only reads `AGENTS.md`
§4. C16 still checks every `cargo gate <tier>` the charter names and every **Full gate** command, so nothing that
is checked today stops being checked except those descriptive lines. If you want them checked, C16 could be
extended to read the guide too (a small follow-up, not done here).

`cargo test -p workspace_checks` (all charter checks, C1 to C24) and `cargo fmt --all -- --check` pass on the
branch.

Options:

1. **Merge as is.**
2. **Merge with changes**: say which (for example keep a passage in the charter, or drop the guide sentence).
3. **Don't merge**: AGENTS.md stays as on `main`, and the branch and guides are dropped.

Recommendation: option 1.

### Exact diff of `AGENTS.md` (`git diff main -- AGENTS.md`)

````diff
diff --git a/AGENTS.md b/AGENTS.md
index a01f3fae..6dfda1f2 100644
--- a/AGENTS.md
+++ b/AGENTS.md
@@ -1,12 +1,13 @@
 # AGENTS.md — Engineering Standards & Architectural Charter
 
-This document is the binding engineering charter for all human developers and AI agents working on **Space Scuffle**. Every pull request, crate, module, shader, and test must adhere strictly to these principles. Each rule carries a tag saying how it is checked; §9 explains the tags.
+This document is the binding engineering charter for all human developers and AI agents working on **Space Scuffle**. Every pull request, crate, module, shader, and test must adhere strictly to these principles. Each rule carries a tag saying how it is checked; §9 explains the tags. A rule that links a guide under `docs/guides/` is followed as that guide describes, and the guide binds like this charter.
 
 ---
 
 ## 1. Core Philosophy: A Textbook-Grade Reference Project
 
-**Space Scuffle** is built to be an exemplary, production-grade software engineering showcase—a codebase that can be presented to university students and systems engineers as a reference for how to design, document, test, and scale a large, real-time, deterministic project in Rust.
+**Space Scuffle** is a production-grade showcase for university students and systems engineers of how to design,
+document, test and scale a large, real-time, deterministic project in Rust.
 
 ### 1.1 Zero Shortcuts, Zero `TODO`s, Zero Placeholders
 - **No `todo!()`, `unimplemented!()` or `TODO`/`FIXME`/`XXX` comments.** Every feature, function and match arm on
@@ -22,10 +23,12 @@ This document is the binding engineering charter for all human developers and AI
   - A fallible result is never discarded with `let _ =` in the simulation crates. [CI: C12]
 
 ### 1.2 Zero Fake Assets, Procedural Stand-Ins, or Silent Workarounds
-- **Never Fake High-Quality Media or Systems in Code:** [Review]
-  - When a task requires real video sequences (e.g., Veo `.mp4` animations), real 3D rendering, or an external service/API, **never** invent a cheap programmatic hack to pretend the requirement is met (such as drawing crude pixel shapes in Rust, wobbling/translating a single static JPEG by a few pixels to fake a "walking/attacking video", or silently substituting dummy data).
-- **Stop and Escalate When a Tool, Key, or Asset Is Blocked:** [Procedure]
-  - If an API key is missing, a service is disabled, a shader fails to load, or a tool cannot produce the required quality, **fail loudly and ask the user immediately** rather than writing a silent fallback or low-quality workaround.
+- **Never fake media or systems in code.** When a task requires real video (e.g. Veo `.mp4` animations), real 3D
+  rendering or an external service/API, no programmatic hack pretends it is met: no crude pixel shapes drawn in
+  Rust, no static JPEG wobbled a few pixels to fake a "walking/attacking video", no silent dummy data. [Review]
+- **Stop and escalate when a tool, key or asset is blocked** (missing API key, disabled service, shader that fails
+  to load, tool below the required quality): fail loudly and ask the user immediately; never write a silent fallback
+  or low-quality workaround. [Procedure]
 - **Real End-to-End Verification Only:** tests compile the real shaders, load real assets from the runtime paths the
   binary uses, and never pass by skipping work; a local GPU opt-out panics in CI. Procedural flipbooks exist only as
   temporary test fixtures, never under `assets/`. [CI: C15 subjects, `runtime_assets_tests.rs`,
@@ -37,31 +40,27 @@ This document is the binding engineering charter for all human developers and AI
 
 Because this codebase serves as a teaching example for large-scale architecture:
 
-1. **Module-Level Architecture Docs (`//!`):** [CI: C4]
-   - Every file/module must begin with a `//!` doc comment explaining **what responsibility this module owns**, **how it fits into the larger system**, and **what invariants it maintains**.
-2. **Public API Docs (`///`):**
-   - All public structs, enums, traits, and functions must have `///` rustdoc comments. [CI: missing_docs] They explain parameters, return values, coordinate spaces, and units (e.g., radians vs. binary angles, ticks vs. seconds, world units vs. grid cells). [Review]
-3. **Justified Performance Optimizations (`// PERFORMANCE RATIONALE:`):** [CI: C11 format; Review]
-   - Default to clean, idiomatic, readable, and type-safe Rust.
-   - Whenever we diverge from the most idiomatic or elegant code for performance reasons (e.g., flat structure-of-arrays layout, lookup tables for deterministic trig, GPU buffer packing, avoiding allocations in hot loops), you **must** document it with a structured comment block:
-     ```rust
-     // PERFORMANCE RATIONALE:
-     // Why: In a 400-unit battle, naive O(N^2) pairwise collision checks perform 160,000 tests/tick.
-     // What: We partition entities into a fixed-size spatial hash grid using pre-allocated flat slabs.
-     // Trade-off: Slightly more bookkeeping on position updates in exchange for O(N) neighborhood queries
-     // and zero heap allocations during the simulation tick.
-     ```
-4. **Short Functions:**
-   - No production function exceeds 100 lines, as counted by `clippy::too_many_lines` with the threshold in
-     `clippy.toml`. Tests are exempt. A suppression outside test code is not allowed. [CI: clippy, C8]
+1. **Module-Level Architecture Docs (`//!`):** every file/module begins with a `//!` doc comment explaining what
+   responsibility the module owns, how it fits into the larger system, and what invariants it maintains. [CI: C4]
+2. **Public API Docs (`///`):** all public structs, enums, traits and functions have `///` rustdoc comments.
+   [CI: missing_docs] They explain parameters, return values, coordinate spaces and units (e.g. radians vs. binary
+   angles, ticks vs. seconds, world units vs. grid cells). [Review]
+3. **Justified Performance Optimizations (`// PERFORMANCE RATIONALE:`):** default to clean, idiomatic, readable,
+   type-safe Rust. Whenever code diverges from the most idiomatic or elegant form for performance (e.g. flat
+   structure-of-arrays layout, lookup tables for deterministic trig, GPU buffer packing, avoiding allocations in hot
+   loops), it must carry a `// PERFORMANCE RATIONALE:` comment block with `Why:`, `What:` and `Trade-off:` lines, as
+   in the example in [`docs/guides/performance-rationale.md`](docs/guides/performance-rationale.md).
+   [CI: C11 format; Review]
+4. **Short Functions:** no production function exceeds 100 lines, as counted by `clippy::too_many_lines` with the
+   threshold in `clippy.toml`. Tests are exempt. A suppression outside test code is not allowed. [CI: clippy, C8]
 
 ---
 
 ## 3. Strict Determinism & Separation of Concerns
 
 ### 3.1 The Simulation Boundary (`sim_types`, `sim_map`, `sim_core`)
-- **100% Platform-Independent Determinism:**
-  - Simulation crates (`sim_types`, `sim_map`, `sim_core`) must produce **bit-for-bit identical results** across Linux (`x86_64`), Windows (`x86_64`), macOS (`ARM64`), and WebAssembly (`wasm32`).
+- **100% Platform-Independent Determinism:** the simulation crates produce **bit-for-bit identical results** on
+  Linux x86_64, Windows x86_64, macOS ARM64 and WebAssembly (`wasm32`).
   - **No Floats in Authoritative State:** positions, velocities, angles, health and cooldowns use `SimScalar`,
     `SimVec2` and `SimAngle`. Floats appear in the simulation crates only where they produce presentation output or
     read authored data. [CI: C7]
@@ -90,16 +89,15 @@ Because this codebase serves as a teaching example for large-scale architecture:
     matches (e.g. `rhino_beetle`, `pulse_bot`, `marine`, `RhinoBeetle`). [CI: C5]
   - Adding, removing or rebalancing these requires zero code changes and zero recompilation: production code reads
     them at run time and embeds no catalog. [CI: C6, `runtime_catalog_tests.rs`]
-  - They live in `assets/catalog/` (`assets/catalog/units.ron`, `assets/catalog/structures.ron`,
-    `assets/catalog/upgrades.ron`, `assets/catalog/factions.ron`, `assets/catalog/start_loadouts.ron`) and
-    `assets/scenarios/`.
-- **Capability-Driven Mechanics:** [CI: C5 partial; Review]
-  - Mechanics (e.g., stim/overclock, harvesting, detectors) must be queried via optional capability components on `UnitArchetype` (`Option<OverclockSpec>`, `Option<HarvesterSpec>`), never by branching on specific `UnitTypeId`s.
-- **Dynamic Asset Loading & Zero Per-Frame Allocations:**
-  - The client and asset baker discover flipbooks and catalog entries at run time.
-  - Battle and preview synchronization systems allocate nothing in steady state and use no O(N²) search. [CI:
-    `sync_allocation_count_tests.rs` (counted by valgrind for the live battle sync), `sync_steady_state_tests.rs`
-    (assets, entities, retained buffers); Review for the rest]
+  - They live in `assets/catalog/` (`units.ron`, `structures.ron`, `upgrades.ron`, `factions.ron`,
+    `start_loadouts.ron`) and `assets/scenarios/`.
+- **Capability-Driven Mechanics:** mechanics (e.g. stim/overclock, harvesting, detectors) are queried via optional
+  capability components on `UnitArchetype` (`Option<OverclockSpec>`, `Option<HarvesterSpec>`), never by branching
+  on specific `UnitTypeId`s. [CI: C5 partial; Review]
+- **Dynamic Asset Loading & Zero Per-Frame Allocations:** the client and asset baker discover flipbooks and catalog
+  entries at run time. Battle and preview synchronization systems allocate nothing in steady state and use no O(N²)
+  search. [CI: `sync_allocation_count_tests.rs` (counted by valgrind for the live battle sync),
+  `sync_steady_state_tests.rs` (assets, entities, retained buffers); Review for the rest]
 
 ---
 
@@ -126,29 +124,23 @@ Code without tests is considered incomplete.
 | `gate` | Selection, globs, scheduling, stamps, catalog consistency | `selection_tests.rs`, `suite_catalog_tests.rs` |
 
 3. **Quality gates.** [CI: C16, C22] Tests are selected by `cargo gate`, from the catalog
-   `crates/gate/suites.ron`, in four tiers:
-   - `cargo gate quick`: optional, while working: fmt, clippy on the changed crates, their fast tests.
-   - `cargo gate package`: required once per package, after merging `main`, on the package's final tree.
-     Clippy on the whole workspace, `workspace_checks`, every test of the changed crates (GPU and valgrind tests
-     when the change touches their triggers), the tests the catalog's path rules and downstream contracts select,
-     and the golden replays in release and on `wasm32-wasip1` for simulation changes. The implementation agent
-     hands back after `quick` and the tests its change needs; a verifier agent on a cheap model then runs `package`
-     and reports the stamp, or the failing targets, which go back to the implementation agent (owner, Q0061).
+   `crates/gate/suites.ron`, in four tiers. What each tier runs, and when a stamp lets a tier pass at once, is in
+   [`docs/guides/test-gates.md`](docs/guides/test-gates.md).
+   - `cargo gate quick`: optional, while working.
+   - `cargo gate package`: required once per package, after merging `main`, on the package's final tree. The
+     implementation agent hands back after `quick` and the tests its change needs; a verifier agent on a cheap model
+     then runs `package` and reports the stamp, or the failing targets, which go back to the implementation agent
+     (owner, Q0061).
    - `cargo gate merge <branch>`: the orchestrator, for each merge to `main`.
-   - `cargo gate full`: the full gate, on `main` before a push that contains a crate change, and on CI. Push only a
-     commit whose tree has a green stamp. When it is red, fix forward or revert within an hour, and add the missing
-     catalog rule.
-   - **Test a tree once.** A tier passes at once, without building or running anything, when its tree already has a
-     green stamp of that tier or a stricter one; a green `package` stamp counts for `merge` and `full` when the
-     agent merged `main` right before its gate and nothing landed since (the merged tree is the tested tree). Merge
-     branches one at a time in that order, batch pushes to a few a day, and run at most one or two building agents
-     at once (owner, Q0055).
+   - `cargo gate full`: on `main` before a push that contains a crate change, and on CI. Push only a commit whose
+     tree has a green stamp. When it is red, fix forward or revert within an hour, and add the missing catalog rule.
+   - **Test a tree once:** a tree with a green stamp of a tier, or of a stricter one, is not tested again for it.
+     Merge branches one at a time in that order, batch pushes to a few a day, and run at most one or two building
+     agents at once (owner, Q0055).
    - **Full gate** (what CI runs on every push):
      - `cargo fmt --all -- --check`
      - `cargo clippy --workspace --all-targets --locked -- -D warnings`
-     - `cargo gate full`: every test executable and doc test of the workspace, each executable with its thread
-       cap from `crates/gate/suites.ron` so that the run fits the runner's memory, and the golden replays.
-   - Docs-only changes: `cargo gate package` runs fmt and `workspace_checks` only.
+     - `cargo gate full`: every test executable and doc test of the workspace, and the golden replays.
 
 ---
 
@@ -222,33 +214,26 @@ cannot break the tool that selects its tests.
 
 ## 7. Shared Development Server: GPU and Disk
 
-Development runs on a shared VM. Its physical **AMD Radeon RX 580** (Ellesmere, `/dev/dri/renderD128`, driven by `radv`) is shared with another autonomous agent user, and the machine also has software Vulkan (`lavapipe` / `llvmpipe`). Every agent must follow these rules:
-
-- **Use the physical GPU.** It is the default for every GPU task: rendering, visual checks, validation runs and GPU tests. The other users need it only about 1% of the time, so do not avoid it.
-- **Hold the GPU lock per command, never for hours.** Wrap each command that runs on the physical GPU in `with-gpu-lock`, and release it between tasks so the other users get a short slot. Lock one test process or one capture at a time, not a whole test suite or session. A wait of a few seconds for the lock is normal. Unlocked use risks VRAM contention, driver resets, and skewed timings. This includes running `client` or any other binary with no driver selected, because Vulkan may then pick the physical GPU.
-  ```bash
-  WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json with-gpu-lock <command>
-  ```
-- **The lock's timeout is 600 seconds by default.** `with-gpu-lock` takes an exclusive `flock` on `/run/lock/gpu.lock` and releases it when the command exits. It no longer passes the lock to the command it runs, so a process the command leaves behind cannot hold it, and it logs every acquisition, timeout and release to `/run/lock/gpu-lock.log` (Q0060). Raise it with `GPU_LOCK_TIMEOUT=1200` only for a single long command, such as a frame-budget measurement.
-- **Software Vulkan is the fallback, not the default.** Use `lavapipe` only where no physical GPU exists (CI) or to reproduce a CI result exactly. It needs no lock:
-  ```bash
-  WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json <command>
-  ```
-  `WGPU_BACKEND=vulkan` is required in both commands: `VK_ICD_FILENAMES` only filters Vulkan drivers, and without it wgpu may fall back to its OpenGL backend, which runs on the physical GPU through `radeonsi` with no lock. The ICD files are `radeon_icd.json` and `lvp_icd.json`; the `*.x86_64.json` names some distributions use do not exist on this server.
-- **GPU tests lock the GPU themselves, one test child at a time.** `crates/client/tests/support/gpu_run.rs` runs
-  each GPU test child on the physical GPU (`radv`, with `MESA_VK_WSI_DEBUG=sw` under `xvfb-run`). It takes
-  `/run/lock/gpu.lock`, the lock `with-gpu-lock` uses, just before the child starts, and releases it when the
-  child exits, so other users get the GPU between children. Run `cargo test --workspace` as it is and never wrap it
-  in `with-gpu-lock`; the harness detects that and fails. A child waits up to `GPU_LOCK_TIMEOUT` (default 290 s)
-  for the lock and then has 120 s to finish. Where the radeon driver, a usable render node or `with-gpu-lock` is
-  missing, as on CI, the harness uses `lavapipe` with no lock. `SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe` reproduces
-  CI on this server, and `=radv` fails instead of falling back.
-- **Run one `cargo gate` or workspace `cargo test` at a time across all worktrees.** `cargo gate` serializes
-  itself through the gate lock (`~/scratch/gate.lock`, `crates/gate/src/gate_lock.rs`); wrap any other workspace
-  `cargo test` in `flock ~/scratch/gate.lock`. Before other heavy runs, check the `anon` line of
-  `/sys/fs/cgroup/user.slice/user-1002.slice/memory.stat` and keep it below about 56 GiB of the 66 GiB limit
-  (`memory.current` also counts reclaimable page cache).
-- **Hot build output goes on the home disk; large files go on `~/scratch`.** The home disk (NVMe, with a per-user quota; check it with `quota -s`) holds the repository, active worktrees and hot build output. The main checkout's own `target/` stays in the checkout, and every agent worktree builds into a separate target directory, `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`. Large logs, captures, rendered sheets, corpora and gate stamps belong under `~/scratch`, a slow spinning disk for cold, large, sequential files. Remove a worktree and its target directory once its branch is merged.
+Development runs on a shared VM whose physical **AMD Radeon RX 580** is shared with another autonomous agent user;
+the machine also has software Vulkan (`lavapipe`). Every agent follows these rules, with the commands, lock and
+driver details of [`docs/guides/gpu-and-disk.md`](docs/guides/gpu-and-disk.md):
+
+- **Use the physical GPU** by default for every GPU task: rendering, visual checks, validation runs and GPU tests.
+- **Hold the GPU lock per command, never for hours.** Wrap each command that runs on the physical GPU in
+  `with-gpu-lock`, one test process or one capture at a time, never a whole test suite or session; this includes
+  running `client` or any other binary with no driver selected. Select the driver with both `WGPU_BACKEND=vulkan`
+  and `VK_ICD_FILENAMES`. Raise the lock's 600-second timeout only for a single long command.
+- **Software Vulkan is the fallback, not the default:** use `lavapipe` only where no physical GPU exists (CI) or to
+  reproduce a CI result exactly. It needs no lock.
+- **GPU tests lock the GPU themselves, one test child at a time,** through `crates/client/tests/support/gpu_run.rs`.
+  Run `cargo test --workspace` as it is and never wrap it in `with-gpu-lock`.
+- **Run one `cargo gate` or workspace `cargo test` at a time across all worktrees.** Wrap any workspace
+  `cargo test` outside `cargo gate` in `flock ~/scratch/gate.lock`, and before other heavy runs keep the user
+  cgroup's anonymous memory below about 56 GiB of the 66 GiB limit.
+- **Hot build output goes on the home disk; large files go on `~/scratch`.** Every agent worktree builds into
+  `CARGO_TARGET_DIR=/home/spacescuffle/cargo-target/<worktree-name>`; large logs, captures, rendered sheets,
+  corpora and gate stamps belong under `~/scratch`. Remove a worktree and its target directory once its branch is
+  merged.
 
 ---
 
````

## Answer

**Approved to merge — plus 6 optional suggestions below to cut `AGENTS.md` roughly in half again (~2,500 tokens).**

> **Autonomy & pre-approval note:** All 6 items below are **genuine suggestions, not mandates**. You are explicitly invited to evaluate each one on its engineering merits and accept all, some, or none of them (you may freely reject any suggestion whose trade-offs you dislike). **For any suggestions you choose to accept (including updating `charter.rs` checks such as `C1`, `C15`, or `C16` to point at `DESIGN.md` or `docs/guides/`), you have full pre-approval to merge directly without asking for another round of owner sign-off.**

### Optional Suggestions to Further Reduce `AGENTS.md` (and Why)

The `charter-compaction` branch did a clean lossless pass (-15%), while preserving several large inventory tables, diagrams, and historical footnotes because checks in `crates/workspace_checks/tests/charter.rs` currently scan `AGENTS.md` directly. Evaluating `AGENTS.md` on *what actually helps an agent write, plan, or review code on every single turn*:

1. **Move §9's 24-row Check Table (`C1`–`C24`) and verbose inline `[CI: ...]` file lists out of `AGENTS.md` (~700 tokens):**
   - *Reasoning:* `crates/workspace_checks/tests/charter.rs` already runs automatically in `cargo gate` and prints a descriptive failure whenever violated. A subagent working on `sim_core` or UI layout gets no value from reading in every prompt that `C23` checks Smog transcendental math or `C24` checks `README.md`'s showcase image path. Moving the `C1`–`C24` reference table to `docs/guides/test-gates.md` (or `crates/workspace_checks/README.md`) keeps 100% of CI enforcement while saving ~36 lines on every turn.
2. **Move §4.2's 38-test-file inventory table to `docs/guides/test-gates.md` and point `C15` there (~650 tokens):**
   - *Reasoning:* `crates/gate/suites.ron` already catalogs test targets (`C22`), and historical filenames like `round2_visual_tests.rs` do not teach architectural rules. Keeping only the 2-sentence testing standard in `AGENTS.md` §4 (every crate requires `#[cfg(test)]` unit tests for edge/error cases and `tests/*.rs` integration tests covering end-to-end public APIs) and having `C15` read the subject/file table from `docs/guides/test-gates.md` saves ~650 tokens per turn with zero loss of test-presence checking.
3. **Replace §5's 22-arrow Mermaid graph (+ `sim_fixtures` linker footnote) with a concise 4-line layering rule, and move the Mermaid graph to `DESIGN.md` for `C1` (~450 tokens):**
   - *Reasoning:* First, line 155 of the Mermaid diagram still says `clearance A* + funnel`, which is obsolete now that WP9 replaced grid clearance corridors with the CDT navigation mesh + triangle A*. Second, a 4-line acyclic layer chain (`sim_types` → `sim_map` → `sim_core` → `asset_baker` → `client` → `client_diagnostics`; `gate` standalone; `sim_*` engine-free) is faster and clearer for LLMs to follow in `AGENTS.md`, while `DESIGN.md` is the natural home for the full Mermaid graph checked by `C1`.
4. **Align §7 with `Q0083` and keep orchestrator-only host thresholds in `docs/guides/gpu-and-disk.md` (~300 tokens):**
   - *Reasoning:* Lines 221 and 226–227 still say *"Use the physical GPU by default for every GPU task... Software Vulkan is the fallback, not the default"*, whereas `Q0083` established that the physical GPU (`radv`) is strictly required only when measuring actual GPU performance (and visual captures if desired), and `lavapipe` is allowed if faster/parallelizable for functional gate tests. Also, host-level cgroup details (`56 GiB` anonymous memory cap) fit naturally in `docs/guides/gpu-and-disk.md`, leaving §7 focused on `with-gpu-lock` discipline, never wrapping `cargo test` in `with-gpu-lock`, per-worktree NVMe `CARGO_TARGET_DIR`, and `~/scratch` for large cold outputs.
5. **Trim historical incident "scar tissue" and decision-history footnotes (~200 tokens):**
   - *Reasoning:* Examples like *"no static JPEG wobbled a few pixels to fake a 'walking/attacking video'"* (§1.2, now enforced mechanically by `bake_integrity_tests.rs`), §6 item 4 (*"Owner rule: the split into full plans and short notes is the owner's..."*, which adds no rule beyond §6 items 1–3), and legislative chains like `(Q0051, which supersedes Q0008; the sanctioned list is Q0052)` or `(owner, Q0061)` can be stated directly as clean present-tense rules (linking `DESIGN.md` for sanctioned deviations).
6. **Move the duplicated 3-command CI Full Gate list in §4.3 to `docs/guides/test-gates.md` and point `C16` at `docs/guides/test-gates.md` (~100 tokens):**
   - *Reasoning:* This both removes the redundant 3-line block from `AGENTS.md` §4.3 *and* closes the trade-off you noted in `Q0085` by extending `C16` to validate `docs/guides/test-gates.md` as well.
