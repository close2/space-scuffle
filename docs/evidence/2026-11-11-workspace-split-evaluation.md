# Evaluation: splitting the workspace into separate Cargo projects

Owner question (chat, 2026-11-11): would splitting Space Scuffle into separate Rust projects, still in this monorepo,
make development faster, by (a) not building and gating unrelated code, and (b) separating functionality more
clearly? This is an evaluation, not a split request. It uses the gate logs of the last three days, the gate's own
selection reasons, the repository history and a few cheap measurements.

**Short answer: do not split.** The gate already selects tests per crate. Measured over the last three days, a split
would save little on the change types we make most often. Where it would save more (sim changes skipping client
tests), it would do so by dropping the client tests that caught three of the five red gates in the window. Three
cheaper changes would save more: run test executables in lanes, stop catalog bookkeeping from selecting the
complete suite, and make the slow client battle-driver tests cheaper. The split's cost is a rewrite of the gate tool,
charter checks C1/C15/C22, CI, profiles and lints, plus a lockfile-skew risk to determinism.

## 1. Sources

- **Gate stdout logs** `~/scratch/logs/verify_*.log`, `merge_group_a.log`, `veo_b5b_gate_quick.log`: every gate
  run from 2026-11-09 00:33 to 2026-11-11 07:51. That is 28 `package` gates (one cut off before its summary,
  `verify_spells.log`, timed from its step logs), one `merge` gate and one `quick` gate. Each log has the changed
  paths with their selection reasons, the complete-suite cause, the selected targets with their reasons, and each
  target's wall and CPU time.
- **Per-gate step logs** `~/scratch/logs/gate/<tree>/<tier>/`: `fmt.log`, `clippy.log`, `build.log`, one log per
  target and per command. Step durations are file modification times (clippy = `clippy.log` − `fmt.log`; build =
  `build.log` − `clippy.log`; tests and commands = last file − `build.log`). The cargo-reported `Finished … in`
  times agree within a few seconds, except where clippy waited for a lock.
- **Stamps** `~/scratch/gate-stamps/*.ron`, **merge history** of `main` since 2026-11-08 (`git log --first-parent`),
  **CI runs** (`gh run list`), and `docs/evidence/2026-11-10-gpu-child-timeouts.md`.
- **Scripts and their output**: `~/scratch/split-eval/scripts/` (`parse.py`, `steps.py`, `rows.json`,
  `steps.json`). They are not committed.

## 2. Where turnaround time goes today

### 2.1 Totals

| Quantity (28 `package` gates, 2026-11-09 to 11-11) | Value |
|---|---|
| Wall time in gates, sum | **36.4 h** (in about 57 h of calendar time; the gate lock was held for at least 64 % of it) |
| Median gate | **78 min** (quartiles 47 / 78 / 103 min; range 13 to 169 min) |
| Format and clippy (workspace, all targets) | 1.4 h, **3.8 %**; median 101 s |
| Build of the selected test executables | 3.7 h, **10.1 %**; median 385 s; spikes of 1,459 s and 2,569 s under disk load |
| Test executables, commands (golden replays, wasm check) and doc tests | 31.6 h, **86.9 %** |
| Sum of test-executable wall times | 28.5 h |
| Gates that selected the complete suite | **14 of 28** (24.6 h, 68 % of all gate time; median 103 min) |
| Selective gates | 14 (11.8 h; median 51 min) |
| Red gates | 5 (§2.4) |

The gate runs one test executable at a time (`--jobs 1`, `crates/gate/src/cli.rs`). Over the test phase the CPU/wall
ratio was 1.9 to 2.2 in the four gates checked (`spells_all`, `ramps`, `pathb`, `group_b`): about two of the 16
cores are busy.

### 2.2 By crate and class (test-executable wall time, all 28 gates)

| Crate and class | Hours | Share |
|---|---|---|
| `client` slow (unit tests, battle drivers; capped at 2–3 test threads) | 14.9 | 52.3 % |
| `client` GPU | 6.1 | 21.4 % |
| `client` other (one gate's log lacks classes) | 1.6 | 5.5 % |
| `client` fast | 1.1 | 3.7 % |
| `client` valgrind | 0.9 | 3.1 % |
| `client_diagnostics` (GPU and fast) | 1.0 | 3.4 % |
| **client and client_diagnostics together** | **25.5** | **89.5 %** |
| `asset_baker` | 1.5 | 5.3 % |
| `sim_types`, `sim_map`, `sim_core`, `sim_fixtures` | 1.25 | **4.4 %** |
| `gate`, `workspace_checks` | 0.2 | 0.7 % |

The slowest single executables in the newest complete gate (`verify_spells_all.log`) were `client/battle_input_tests`
605 s, `editor_app_tests` 500 s, `battle_fog_tests` 475 s, `battle_hud_tests` 461 s, `battle_card_tests` 456 s and
`effect_slice_gpu_tests` 372 s. On 2026-11-04 the blueprint (`docs/plans/2026-11-04-tiered-test-gates.md` §2.3)
measured the same battle drivers at 50 to 120 s. They have grown four to six times since. The causes are the
thread caps that bound memory (Amendments 2 and 4), more tests per binary, and bigger shipped atlases.

### 2.3 Why gates selected what they did

**Complete-suite causes (14 gates):**

| Cause | Gates | What the change actually was |
|---|---|---|
| `crates/gate/suites.ron` (global `crates/gate/**`) | 9: WP9, WP12, WP12b, WP13b, WP13c, pathfinding A, integration, friendly blocking, effects | Mostly one-line additions: a new `sim_core` test added to the `sim_fixtures` downstream list, which C22 keeps equal to the sources (`git log -- crates/gate/suites.ron`: 30 commits since 11-08, most of them `+ "sim_core/<new>_tests",`) |
| `Cargo.lock` (global) | 4: the spellcaster package's gates | `sim_core` gained the dev-dependency `proptest`, which was already in the lockfile; the only lockfile change was one line in `sim_core`'s entry |
| `.github/workflows/ci.yml` | 1 | The CI memory fix |

Five of the nine `suites.ron` gates (WP9, WP12, WP13b, WP13c, friendly blocking) changed no client code. As
selective gates they would have looked like their selective siblings WP10, WP11 and WP13a (23 to 27 min) instead
of 70 to 169 min.

**Sim-only changes still run client tests.** In the eight selective gates whose packages changed simulation crates
(and often `assets/catalog`) but no client code, client tests were most of the test time. The table below splits
that time by the reason the gate gave:

| Gate | Test time | Client, selected only by a sim downstream contract | Client, selected only by a data rule (`catalog`, `media`, `showcase cards`) | Client, both |
|---|---|---|---|---|
| WP10 | 1,261 s | 556 s | 475 s | 27 s |
| WP11 | 1,137 s | 465 s | 448 s | 27 s |
| WP13a | 1,360 s | 616 s | 492 s | 28 s |
| pathfinding B | 806 s | 695 s | 0 | 0 |
| flipbook walk | 1,042 s | 823 s | 27 s | 0 |
| follow-ups | 1,874 s | 771 s | 534 s | 154 s |
| group routing A (`sim_map` only) | 680 s | 136 s | 0 | 0 |
| group routing B | 2,174 s | 665 s | 571 s | 531 s |

The downstream contracts (`suites.ron` `downstream`) cost 136 to 823 s per sim gate, median about 10 min. The data
rules cost a similar amount whenever `assets/catalog/**` changes. They would stay after any split, because the client
reads the same catalog at run time. Eleven of the 22 crate-changing merges in the window touched `assets/catalog`.

**Client-only changes run no simulation tests already.** The two client-only gates (ramp geometry 74 min, GPU child
timeouts 94 min) selected 58 and 60 targets, none of them in a simulation crate. Their time is the client's own
tests, which any layout would run.

### 2.4 What the red gates caught

| Gate | Failing target | Cause | Crosses the sim/client boundary? |
|---|---|---|---|
| WP12 (`verify_wp12.log`), a package with no client code | `client/battle_flipbook_tests` (`attack_release_shows_the_release_frame`, `sustained_fire_changes_no_material`) | The simulation's timing change broke the client's flipbook cadence pins | **Yes**, selected by the `sim_core` contract |
| spellcaster 1 (`verify_spells1.log`) | `client/smog_golden_tests` | `sim_types::StatusFlags` gained fields, which changed the `Debug` text of `BubbleRenderInstance` that four client goldens hash (fixed in `3403eb7`) | **Yes**. It ran only because `Cargo.lock` selected the complete suite: `smog_golden_tests` is in no sim contract |
| storm (`verify_storm.log`) | `client/lib` `battle_hud::catalog::tests::targeted_effect_abilities_become_casts_shared_by_name_and_target`; `client/editor_app_tests` (GPU child) | Psionic Storm catalog data against the HUD's catalog reader; GPU child failure | **Yes** (data → client); the second is environment |
| WP13b | `client_diagnostics/frame_budget_tests` timeout | Cold page cache (gpu-child-timeouts evidence) | No, environment |
| friendly blocking | four GPU children timed out | The same | No, environment |

The history adds two more boundary cases. `2e39536`: `AttackMoveToCell`, a new `sim_types` enum variant, made an
irrefutable `let` in `client/tests/battle_presentation_tests.rs` refutable (E0005). The package gate's
**workspace clippy** caught it, without running any client test. `4495a14`: the effects package's `sim_types`
`Debug` spelling moved the four smog system goldens again. Every genuine red in the window came from a client test
that a simulation or catalog change broke. None came from a test of a crate the package changed alone; the
implementers had already run their own tests.

### 2.5 Environment factors

From `docs/evidence/2026-11-10-gpu-child-timeouts.md` and this window:

- Agent target directories sit on `~/scratch` (`/dev/sdb`), a rotational disk shared with another user. Under load
  it reads about 8 MiB/s at 33 ms per request, with I/O pressure of 60 to 97 %. Relinking three test executables once
  took 39.5 min. This is what makes the build step spike (1,459 s for `cimem`, 2,569 s for `blocking`, against a
  median of 385 s).
- The user slice's `memory.high` counts page cache. A gate links about 18 GiB of test executables, so it evicts its
  own executables before it runs them, and cold executables page in at about 80 ms a fault. The harness now pre-reads
  GPU children. The parent test executables are still read cold.
- The other user's load (1-minute load average 207 to 273 during this evaluation) reaches us mainly through that
  disk, not the CPU: the CPU controller shares CPU equally between user slices.
- Gates queue on the gate lock. With at least 64 % lock occupancy, a package's turnaround also includes waiting
  for other packages' gates (`verify_b114.log` waited behind a gate that had held the lock for 12 minutes and then
  ran 2 more hours). Any cut in gate time also cuts the queue.

## 3. What a split would change

The three layouts considered:

- **A:** a `sim/` workspace {`sim_types`, `sim_map`, `sim_core`, `sim_fixtures`} and a `client/` workspace
  {`client`, `client_diagnostics`, `asset_baker`} that depends on the sim crates by path. The tools stay in one of
  them or at the root.
- **B:** A, plus a third workspace for the tools {`gate`, `workspace_checks`}.
- **C:** no split; stronger boundaries and selection rules (§5).

### 3.1 Builds

Measured in scratch copies under `~/scratch/split-eval/` (`git archive main`; own target directories; `-j 4`,
`nice -n 10`; unlocked because the gate lock had been held for more than 30 minutes; load average about 250 from
the other user, so treat the absolute times as noisy):

| Measurement | Today's workspace, `-p` the four sim crates | Sim-only workspace (layout A/B) |
|---|---|---|
| Lockfile entries | 657 (workspace) | 46; every one at the same version as today |
| Packages in the dependency graph (normal, build, dev) | 45 for the four sim crates; 394 for the workspace | 45 |
| `cargo metadata` | 4.6 s | 3.8 s |
| Cold `cargo test --no-run` of the four sim crates | 337 s, 44 units compiled | 330 s, 44 units compiled |
| After touching `sim_types/src/lib.rs` | 208 s | 192 s |

The two layouts compile the same 44 units in the same time, within this load's noise (2 % cold, 8 % warm). The
warm rebuild is long because `sim_core` and `sim_map` build at `opt-level = 2` in the dev profile. Logs:
`~/scratch/split-eval/{mono-sim-cold,simws-cold}.log`.

What the measurement shows, and what Cargo's model implies:

- **A sim-only build is already sim-only.** With resolver 2, `cargo test -p sim_core` resolves features only for the
  packages it builds and compiles those 45 packages, not Bevy. A sim workspace builds the same units. A split makes
  sim-only builds no faster. It only stops someone from typing `--workspace`.
- **The client side gets slower, not faster.** A client workspace that uses the sim crates by path compiles them,
  and their dependencies, again in its own target directory: the cold sim build above, once more per worktree and
  per sim change. A shared `CARGO_TARGET_DIR` avoids the duplicate only when both lockfiles resolve the shared
  dependencies identically. Cargo's target-directory lock then serializes the two builds.
- **Lockfile skew.** Two lockfiles can resolve `serde`, `ron`, `rand` and `rand_chacha` to different versions. Only
  `fixed` is pinned exactly. The determinism jobs would then prove the sim built with the sim lockfile, while players
  run the sim built with the client lockfile. Avoiding that needs a new check that the shared entries are equal,
  which is extra tooling for a guarantee the single lockfile gives for free. Bevy and wgpu would stay in the client
  lockfile only. That is true of the sim crates' graph today too: C2 checks it.
- **Clippy** is 3.8 % of gate time. Splitting it would save at most a few minutes per gate, and it would lose the
  check that caught `AttackMoveToCell`: workspace clippy compiles every downstream target against a sim API change
  in about 100 s, without running a test.
- **Incremental compilation** is per crate and per target directory. A split adds a second copy of the same
  incremental state; it does not make either copy faster.

### 3.2 Test selection

A split can skip only what the gate could skip by editing `suites.ron`. Removing the `sim_types`, `sim_map`,
`sim_core` and `sim_fixtures` downstream contracts would give a sim-only gate exactly what a sim workspace's gate
would run. So the split adds no new way to skip tests. It only takes away the choice, and there are two ways to
live with that:

1. **Path dependencies with honest gating.** Every sim change also runs the client workspace's contract tests. That
   is today's selection plus the duplicate sim compile (§3.1). Saving: none; cost: the extra compile.
2. **Deferring the client.** The client tests run only in the client workspace's own gates, or when the client pins
   a newer sim. This saves the contract tests: 136 to 823 s, median about 10 min, per sim gate (§2.3), plus that
   gate's client relink. But the three genuine reds of §2.4 would then turn up in a later client gate, after the sim
   package had merged. WP12 shows the price: its red gate and the fix-and-rerun cost 4,193 s + 4,262 s, and with
   deferral that cycle would happen again in another package, against a `main` already red for clients. That moves
   the safety later and makes it costlier; it does not remove its cost.

The data rules stay in both cases. `assets/catalog/**` is read by `sim_fixtures` (`include_str!`), `sim_core`,
`asset_baker` and `client`, so a catalog edit must run client tests whatever the layout. Half the crate-changing
merges in the window edited the catalog.

**Where a split would save, and the cheaper rule that saves the same.** In layout B a `suites.ron` edit would no
longer select the complete suite of every workspace, and in A a sim dev-dependency change would not touch the
client's lockfile. Both savings are real (§2.3: 13 of the 14 complete-suite gates). Both come from rules in
`crates/gate/suites.ron` and `crates/gate/src/select.rs`, so both can be had without a split (§5, items 2 and 3).

### 3.3 Charter checks, gate tool and CI

| Item | Today | After a split |
|---|---|---|
| C1 (§5 graph equals the Cargo graph) | One `cargo metadata` | Per-workspace metadata merged, plus cross-workspace path edges; C1 must be rewritten |
| C2 (no engine dependency in sim crates) | `cargo metadata` of the workspace, transitive | Still needed: a path dependency on the client can be added to a sim crate in either layout |
| C15, C20, C22, C16 | One crate list, one `suites.ron`, CI runs `cargo gate full` | `cargo gate` reads one `cargo metadata` (`crates/gate/src/workspace.rs`) and computes reverse dependencies from it; it would need multi-workspace discovery, per-workspace stamps and catalogs, and a cross-workspace contract table. C22's "covers every tracked path" must span workspaces |
| `[workspace.lints]`, `[workspace.dependencies]`, `[profile.dev.package.sim_core] opt-level = 2`, `[patch.crates-io] naga` | Once, in the root `Cargo.toml` | Copied into each workspace and kept equal. Profiles of path dependencies come from the consuming workspace, so the client workspace must repeat the sim `opt-level` lines, or `skirmish_match_tests` slows down |
| CI (`.github/workflows/ci.yml`) | `check-and-test` (57 to 90 min per push), `determinism-native`, `determinism-wasm`, `throughput` | The determinism, wasm and throughput jobs already run `-p sim_*` only and would be unchanged. `check-and-test` becomes two jobs, one of which still compiles the sim crates |
| `.cargo/config.toml` (debug-section compression, `cargo gate` alias) | Root | Shared if it stays at the root of the repository; the alias must name a workspace |

Migration cost estimate: a full plan (charter §6, since it changes the gate, the charter and CI), the gate's
multi-workspace work, rewrites of C1/C15/C16/C20/C22, CI, and the charter's §4/§5/§7. Several working days of
agent time, followed by the risk of a run of gate misses while the new catalogs settle.

### 3.4 Code separation

What a split would enforce that the current checks do not:

- **Nothing about dependencies.** C1 already fails any edge not in the charter's graph, and C2 any engine, GPU,
  window or audio crate reached by a sim crate, even transitively. A split would not stop a sim crate from adding a
  path dependency to `client`; C1/C2 would still be needed.
- **Feature unification during `--workspace` builds.** In one workspace, a feature that a client dependency enables
  on a shared crate is active when the sim crates build together with the client. Sim code could therefore compile
  in `--workspace` builds while relying on that feature. CI's determinism jobs (`cargo test -p sim_types -p sim_map
  -p sim_core`, the `wasm32-unknown-unknown` check) build the sim crates alone and catch this. The split would make
  it structural; today it is checked.

Current leaks, looked for and not found: the sim crates have no `#[doc(hidden)]` items, no `[features]`, and their
`test_support` is `pub(crate)`. The client uses `sim_core::{catalog, scenario, world, production, entity, economy,
upgrades, transport}`, the public catalog and world API that charter §3.2 assigns to it (admission, placement,
terrain). The coupling that does hurt is not a dependency edge. It is **presentation contracts that sit in
`sim_types` but are pinned by client tests**: the `Debug` text of `BubbleRenderInstance` hashed by
`smog_golden_tests`, the scenario order enum matched in client tests, and weapon cadence pinned by
`battle_flipbook_tests`. A split does not loosen these. It only reports them later. The useful fix is to make each
such contract explicit: hash the bytes the smog consumes instead of `Debug` text, and list `smog_golden_tests` in
the `sim_types` contract until then. Today that test is not in any sim contract and was caught only by a
complete-suite gate.

## 4. Expected savings by change type

Per `package` gate, against the medians of this window (sim-code gates: WP10, WP11, WP13a, pathfinding B, flipbook
walk, follow-ups, group routing A and B, median about 31 min, but 70 to 169 min when `suites.ron` or `Cargo.lock`
changed).

| Change type (window count) | Today | Split A/B, client tested honestly | Split A/B, client deferred | Alternatives 1–3 of §5 |
|---|---|---|---|---|
| Docs only (14 merges) | fmt and `workspace_checks`, about 1 min | 0 | 0 | 0 |
| Sim code, no catalog (4 of the sim gates) | 21 to 43 min | +3 to 6 min (second sim compile, §3.1) | −2 to −14 min, and boundary breaks found later | about −10 to −20 min (lanes) |
| Sim code plus catalog (about 7) | 23 to 64 min | +3 to 6 min | −8 to −11 min; the catalog's client tests remain | about −10 to −25 min |
| Sim change that edits `suites.ron` or a sim dev-dependency (5 to 6) | 70 to 169 min (complete suite) | about −45 to −100 min (sim catalog only) | the same | about −45 to −100 min from rules 2–3 alone, more with lanes |
| Client only (2) | 74 to 94 min | 0 (the sim tests were not selected anyway) | 0 | about −30 to −45 min (lanes) |
| Media and `asset_baker` (3) | 13 to 86 min | 0 | 0 | about −5 to −30 min |
| Gate or catalog edit (2, plus the 9 above) | complete suite, 100+ min | B: the gate's tests only | the same | the same, from rule 2 |
| Packages spanning sim and client (6) | 82 to 169 min, complete suite | 0, or worse (two gates) | 0 | about −40 to −60 min |

The lane figures in the last column are estimates, to be measured (§5, item 1). They come from the CPU/wall ratio of
about 2 on a 16-core slice and from the class split of §2.2. With one GPU lane and two general lanes, the GPU
binaries (20 to 30 % of test time) overlap the rest, and the slow headless binaries share two lanes. The test phase
of `verify_spells_all` (6,534 s of executables) would become about max(2,035 s GPU, 4,500 s / 2 general) ≈ 2,300 to
2,800 s with some contention, against 6,534 s now.

Over the 28 gates of the window, the honest split saves nothing measurable outside the 13 complete-suite gates, and
rules 2 and 3 capture those without a split (about 5 to 7 h of the 36.4 h). The deferring split additionally saves
about 10 min on each of roughly 8 sim gates (about 1.3 h), and moves the three boundary breaks of §2.4 to a later,
costlier point.

## 5. Recommendation and cheaper alternatives

**Recommendation: do not split the workspace.** Keep one workspace and one lockfile, and take the cheaper measures
below. Revisit only if the client ever consumes the simulation as a versioned library, such as a server or a
second client built from a released sim. Until then the client is the simulation's main consumer, and gating it with
every sim change is the point.

Ranked by benefit per cost:

1. **Run the gate's executables in lanes (`--jobs 3`: one GPU lane, two general).** The blueprint (§2.6) kept
   `--jobs 1` because on 2026-11-04 several gates ran at once on the same cores. Since Amendment 2 the gate lock
   allows one gate at a time, and the test phase now uses about 2 of 16 cores. Expected: test phase −40 to −60 %,
   about −30 to −45 min on a complete gate and −10 to −20 min on a selective one; about 10 to 14 h of the window's
   36.4 h. Cost: one A/B measurement on the same tree, then a default change. Risks: memory (the thread caps keep
   each binary near 6 GiB, so about 20 GiB for three lanes, inside the 36 GiB rule), load-sensitive pixel tests
   (§11.4 already made the skirmish threshold load-independent), and cold-executable paging. Pre-reading each
   parent executable, as the harness does for GPU children, also helps here.
2. **Stop catalog bookkeeping from selecting the complete suite.** Remove `crates/gate/suites.ron` from `global`.
   Select from its diff instead: the targets whose entries, triggers or rules changed, plus `gate/@fast` and
   `suite_catalog_tests`. Also compute the `sim_fixtures` contract from the sources at selection time (C22 already
   computes it to compare), so a new sim test needs no catalog edit at all. Keep `crates/gate/src/**` global if
   wanted. Expected: 9 of the window's 14 complete gates become selective; about −45 to −100 min each where no
   client code changed, about 4 to 5 h in the window. Cost: about half a day in `select.rs` and its tests. Risk:
   low; `full` and CI still run everything.
3. **Make `Cargo.lock` selection diff-aware.** When the only changed lockfile entries are workspace members'
   dependency lists that point at packages already locked, select those members and their contracts instead of the
   complete suite. External version changes stay global. Expected: the spellcaster package's 4 complete gates; about
   −20 to −40 min each where the client was not touched. Cost: small. Risk: low.
4. **Make the slow client battle drivers cheaper.** They are 52 % of all test time and grew four to six times in a
   week (§2.2). Profile one, say `battle_input_tests` at 37 tests and 430 to 630 s, to see how its time splits
   between app start-up, atlas decoding and simulation warm-up. Then share the decoded atlases across the tests of a
   process, or consolidate binaries (Q0034's second option, on hold since 2026-11-04). Expected: large but unknown
   until profiled; every 10 % here is about 1.5 h in the window. Cost: an investigation, then a plan.
5. **Make the boundary contracts explicit** (§3.4). Add `client/smog_golden_tests` to the `sim_types` contract, or
   better, hash the bytes the smog consumes instead of `Debug` text. This costs a little time and fixes a real miss.
6. **Host: target directories on solid-state storage, or page cache outside `memory.high`.** The build step is
   10 % of gate time, with spikes up to 43 min, and cold executables slow the tests too. Expected: build median and
   spikes down sharply, plus fewer GPU-child timeouts. Cost: an owner/host decision.
7. **`sccache`.** It would help cold builds in new worktrees (each one compiles Bevy). It does not cache linking or
   incremental crates, and linking the large test executables is most of a warm gate build. Expected: modest. Cost:
   low. Measure before adopting.
8. **The split itself** (layout A or B): little benefit beyond items 2 and 3, a costly migration, and a determinism
   risk from lockfile skew. Not recommended.

## 6. Out of scope

- No change was made to the gate, the catalog or CI. Items 1 to 5 each need the note or plan charter §6 requires.
- The lane estimate of §4 is not measured. Measuring it needs the gate lock and a whole client test build, and it is
  item 1's first step.
- Quick-tier gates and implementers' own builds outside gates are not counted. Their build costs would move with the
  same factors (§3.1, item 6).
