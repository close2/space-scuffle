# Blueprint: Close the Exits of Remediation Phases 6, 7 and 8

This plan closes the exit criteria of Phases 6, 7 and 8 in `docs/plans/2026-11-01-codebase-remediation.md` §5.
Phases 5 and 9 have an "Exit status" paragraph confirmed by CI. Phases 6, 7 and 8 have none, and the finding
tables of §4 still show most of their findings as open, although the code was fixed on 2026-11-01 and 2026-11-02.

**Status of this document:** Stage 1 blueprint under charter §6, written on 2026-11-03 against `main` at `2be8992`.
The audit in §2 comes from reading the tree and `git log`. The measurements in §2.5 and §5 were taken on the
development server while this plan was written; their commands and logs are named there. Line numbers drift: search
for the quoted symbol. A claim about runtime behaviour that could not be confirmed is marked **verify**, and the first
test of its package confirms or refutes it.

**Out of scope:** step 8.5 (B-1, D7), real media for the required set. It needs paid Veo generation with the
owner's approval for each run, and the media scope is still open as [Q0001](../questions/open/0001-media-scope.md).
It stays blocked on the owner. Phase 8 therefore cannot fully exit through this plan (§2.3).

---

## 1. Summary

| Phase | Exit criterion | Status | What closes it |
|---|---|---|---|
| 6 | No per-frame heap allocation or asset churn in sync systems, proved by test | **Partly met** | WP2 (asset churn, retained buffers, every sync system), WP3 (allocation count, external, no `unsafe`) |
| 6 | One owner per concern | **Partly met**: the code is done, no test proves it | WP4 (writer table and ordering tests) |
| 6 | Frame budget measured at 200 and 400 units | **Not met** | WP5 (measuring tool), WP6 (measurement on the RX 580, recorded) |
| 7 | Edit, save, playtest, return and reopen is covered by an automated App test | **Met** | WP1 records it |
| 8 | Strict audit passes for the agreed set | **Not met, blocked on the owner** (8.5, Q0001) | Nothing in this plan |
| 8 | A failed download cannot enter the cache | **Met** | WP1 records it |

Every step of the three phases except 8.5 has landed (§2.1 to §2.3). What is missing is proof, measurement and
bookkeeping, not fixes. One new question was filed:
[Q0015](../questions/open/0015-client-frame-time-target.md), on the frame-time target. Its default is to measure
and publish without a pass or fail line.

A valgrind probe (§5.2) shows that an allocation count without `unsafe` is feasible and reproducible. It also
shows that a paused frame of the live-sync test app allocates about 25 times today. Part of that is Bevy's own
work, so WP3 attributes it before it asserts anything.

The investigation of today's simulation throughput (§2.5) is already done. It found **no regression** since the
handover. The handover's figures (11,114 ticks/s on an empty 64×64 map, 639 at 400 units) were never reproduced on
this server at any commit. Every commit from `ae075fb` on measures about 3,400 and 200 here. A small residual,
about 3 to 4 % slower at 400 units between `b263734` and `main`, is left to WP7.

---

## 2. Audit of the current tree

### 2.1 Phase 6: client performance and structure

**Steps.**

| Step | Findings | Landed in | Notes |
|---|---|---|---|
| 6.1 | C-2, C-5 | `52abebb` (persistent smog textures, `Queue::write_texture`), `9f419bb` (compare before `Assets::get_mut`) | Measured before and after in the commit messages: smog went from 116 image and 58 material `Modified` events over 59 steps to 0 and 0. Bubbles went from 26 of 26 materials modified per frame to 0 when paused |
| 6.2 | C-4, D2 | `4c9fda6` (depth prepass at camera setup), `0f31027` (runtime mist and reducer deleted) | Done |
| 6.3 | C-6, C-7, C-13 | `ba9a5c1` (typed smog failures), `c37943a` (smog behind every unit, with a GPU luminance test on the 200-bubble swarm), `12296f2` (ordering, map-derived bounds, velocity scale) | The 16-cavity cap is kept on purpose (`12296f2`). Smog was proved by capture on the swarm scene, not in a live battle (handover §5). WP6 adds a live-battle frame at 400 units as a by-product |
| 6.4 | D3 | `5654fda` (`client_diagnostics`), `bd909dc` (fixture branches out of capture), `0f31027` | `grep -rn ControlledFixture crates/client` finds nothing |
| 6.5 | C-8, C-9, C-10, C-14 | `7117874`, `93afd13`, `ab5ef61` | Done; C-9 and C-14 are checked by tests only (handover §5) |
| 6.6 | C-15, C-16, Q-4 (client) | `257824d` (world into `BattleSession`), `52620f6` and `00eff7c` (cached lineup), `8392b40` and `07f7014` (arena and dead markers), `64fd330` (client crate-wide allows dropped) | Done |

**Exit criterion 1: no per-frame heap allocation or asset churn in sync systems, proved by test. Partly met.**

"Sync systems" means every system that brings the simulation's published state, or the presentation state derived
from it, into ECS components, assets or retained UI models on every frame. The inventory below lists all of them,
from `crates/client/src/lib.rs` (`add_battle_pipeline_systems`, `add_presentation_systems`,
`add_combat_effect_and_inspector_systems`) and the plugins `BattleHudPlugin`, `SmogPlugin`, `TerrainPlugin`,
`BubbleOpticsPlugin` and `SceneEnvironmentPlugin`.

Excluded on purpose, with the reason:

- Input and UI systems: `lab_ui_system`, `battle_hud_system`, `battle_result_system`, `collect_*_intent`,
  `apply_hud_intent`, `draw_selection_box`. egui builds its frame from scratch and allocates every frame by
  design. Charter §3.3 binds "synchronization systems", not immediate-mode UI.
- `advance_battle_ticks`: the simulation tick. The throughput benchmark covers it, and the simulation allocates
  per tick (commands, snapshot).
- Combat effect spawning (`process_combat_events_system` and the `update_*` effect systems in `vfx.rs`). These run on
  events, not in steady state. A finding about them is recorded in §2.4 (C-17).

| System (file) | Asset churn evidence | Retained-buffer evidence | Gap |
|---|---|---|---|
| `sync_live_battle_system` (`scene_controller.rs`) | `pipeline_presentation_tests.rs`: 0 `BubbleDomeMaterial` events over 64 paused frames; only turning units touch materials on moving frames | Hook for an external profiler (`SPACE_SCUFFLE_ALLOCATION_GATE`), never run by any test or script | Other asset types; no allocation count |
| `refresh_battle_presentation` (`battle_presentation.rs`) | n/a (no assets) | Unit test `roster_entries_stay_sorted_by_type_and_are_reused` | none for buffers |
| `sync_lab_preview_system` | `52620f6` tests: steady map objects raise no `Modified` events | Lineup rebuild rule tested | Not in a single steady-state test |
| `sync_swarm_stress_system` | none | none | Both |
| `refresh_hud_model` (`battle_hud/model.rs`) | n/a | `battle_hud_tests.rs`: `the_model_allocates_nothing_on_a_frame_without_change`, `the_model_reuses_its_buffers_on_every_tick` | none |
| `refresh_battle_fog`, `apply_battle_fog` | `battle_fog_tests.rs`: `the_fog_rewrites_only_on_a_tick_and_reuses_its_buffers` | `battle_pipeline/fog.rs`: `a_refill_reuses_its_buffers_and_counts_itself` | none |
| `refresh_hud_focus`, `collect_battle_feed`, `refresh_command_card`, `refresh_queue_panel` | n/a | Unit tests in `card.rs` and `feed.rs` cover derivation, not reuse **verify** | Reuse over frames |
| `refresh_result_screen` | n/a | `battle_result_tests.rs`: `the_verdict_text_does_not_allocate_on_a_frame_without_change` | none |
| `update_map_presentation` (`runtime_battle`) | `runtime_battle_tests.rs`: `steady_map_objects_leave_their_materials_unmodified` | n/a | none |
| `selection_system` (`selection.rs`) | `battle_input_tests.rs`: `selecting_one_unit_leaves_every_other_material_unchanged` | n/a | Steady frames with no selection change |
| `update_indicators_system` (`indicators.rs`) | none | none | Both |
| `sync_optics` (`bubble_optics.rs`) | `bubble_optics_tests.rs`: `unchanged_shared_materials_emit_no_spurious_asset_changes` | n/a | none |
| `apply_scene_environment` | `environment/tests.rs`: `steady_frames_do_not_mark_global_resources_changed` | n/a | none |
| `sync_bouncing_smog_bubbles_system`, `update_smog_field_system` | `smog_bouncing_bubbles_tests.rs`: `smog_stepping_never_modifies_image_or_material_assets` | Sources scratch reused in code (`sources_scratch.clear()`), not tested | Buffer reuse |
| `upload_smog_frame_system` (render world) | GPU test `smog_steps_reach_the_gpu_through_queue_writes` | Writes in place by construction | none (render world is outside the harness) |
| `update_terrain_system` (`terrain/mod.rs`) | n/a | `terrain_tests.rs`: `test_rebuild_reuses_capacity` | Steady frames with no edit |
| `update_placement_preview`, order markers | none | none | Both |
| `sync_scene_camera`, `manage_battle_scene`, `camera_control_system` | none | none | Steady frames |

So asset churn is proved for the systems with the largest past churn (C-2, C-5), one system at a time and one asset
type at a time. Nothing proves that the whole steady-state frame creates, modifies or removes no asset, spawns no
entity and keeps every retained buffer. No test counts allocations at all. Q0011 explains why: a counting
`#[global_allocator]` needs `unsafe impl`, and the workspace forbids `unsafe_code`.

**Exit criterion 2: one owner per concern. Partly met.** The structural work is done. The authoritative world lives
in `BattleSession` (`257824d`), and the parallel arena, markers and aliases are gone (`8392b40`, `07f7014`). The
mist/smog duplication is gone (`0f31027`). `bubble_shader.rs` and `bubble_optics.rs` now own different lanes of one
material: snapshot lanes and optics lanes. `9f419bb` stopped the two systems from fighting over them. But no test
states who owns what, so a second writer could come back unnoticed. Phase 9 also found (blueprint §9, finding of
WP6) that no test pins two client orderings that make no schedule cycle when reversed:
`sample_input_ownership.after(lab_ui_system)` and the editor's `apply_document_load_system.before(editor_ui_system)`.
WP4 closes both.

**Exit criterion 3: frame budget measured at 200 and 400 units. Not met.** Nothing measures frame time.
`FrameTimeDiagnosticsPlugin` only feeds a smoothed FPS readout in the Lab UI (`lab_ui.rs`). `DESIGN.md` §5 says "Not
yet measured". The shipped scenarios cannot reach 200 units: `battle_automaton` spawns 12 units per side, and both
shipped maps are 64×64. WP5 and WP6 close it.

### 2.2 Phase 7: editor hardening

| Step | Findings | Landed in |
|---|---|---|
| 7.1 | E-1 | `9875f48` (playtest through the production battle pipeline), `bce31af` (selection and orders end to end) |
| 7.2 | E-2, E-6 | `1e03482` |
| 7.3 | E-3, E-7 | `270217a` |
| 7.4 | E-4, E-5 | `956682c` |
| 7.5 | E-8, E-9, E-10 | `96eab79`, `ca953f5` and `69dd84e` (E-9), `b9c65f3` and `1318d30` (E-10) |

**Exit criterion: edit, save, playtest, return and reopen is covered by an automated App test. Met.**
`crates/client/tests/editor_app_tests.rs::edit_save_playtest_return_and_reopen_round_trip` (added in `b9c65f3`,
merged in `7decbd7`) boots the real `MapEditorPlugin` on a fixture map and drives it with input events. It paints a
stroke, saves with Ctrl+S, enters playtest with F5 and checks that bubbles are drawn, returns with Escape (document
and history intact), reopens the file through the real load path (`LoadDocumentRequest::Open`) and playtests again.
The test needs no GPU (`backends: None`), so it runs in CI's `cargo test --workspace`. CI was green on `main` at
`6b39cd2` (run 41665216936).

Limits, which do not affect the criterion: the save uses the shortcut, not the egui button, and the reopen sends the
event the file browser sends, not a click. Editor egui clicks and text fields are not exercised (handover §5). The
order of `apply_document_load_system` before `editor_ui_system` is not pinned (WP4).

### 2.3 Phase 8: asset pipeline and media

| Step | Findings | Landed in | Tests |
|---|---|---|---|
| 8.1 | B-2 | `b342c77` | `veo_transport_tests.rs`, all `b2_*` |
| 8.2 | B-3, B-4 | `4c425c8` | `veo_reliability_tests.rs`, all `b3_*` and `b4_*` |
| 8.3 | B-5, B-9 | `d3f70d5` | `veo_tests.rs` (`b5_*`, `b9_*`), `veo_reliability_tests.rs::b9_only_exact_catalog_slugs_can_spend_money_or_touch_the_filesystem` |
| 8.4 | B-6, B-7, B-8 | `cf784cd` | `bake_integrity_tests.rs`, all `b6_*`, `b7_*` and `b8_*` |
| 8.5 | B-1, D7 | not started | Blocked on the owner (Q0001); paid generation needs approval for each run |

**Exit criterion: a failed download cannot enter the cache. Met.** `veo_reliability_tests.rs`:
`b3_http_error_body_never_becomes_a_video`, `b3_success_status_with_a_non_video_body_is_rejected`,
`b3_truncated_video_is_rejected`, `b3_inline_bytes_are_validated_too`, `b3_failed_download_preserves_the_existing_file`,
`b3_poisoned_cache_entry_is_refused_without_spending_money`, each asserting `assert_nothing_cached`. They ran green on
CI at `6b39cd2`.

**Exit criterion: strict audit passes for the agreed set. Not met, owner-blocked.** There is no agreed set until
Q0001 is answered. Two notes for whoever runs 8.5. First, Vertex authentication moved to an `x-goog-api-key` header
and was never tried against the live service (handover §5). Second, a Vertex result delivered as `gcsUri` would be
refused by the download allowlist (`generativelanguage.googleapis.com`, `us-central1-aiplatform.googleapis.com` in
`veo_client.rs`). That refusal is loud and costs nothing, but it would stop the run. The first approved 8.5 run
checks both.

### 2.4 Finding rows in §4 of the remediation plan

The §4 tables mark only C-1, C-3, C-11, C-12 (Phase 5) and Q-9, Q-10, Q-12 (Phase 9) as fixed. WP1 marks these rows,
in the format the table already uses (`**Fixed** in <commit> (<step>)`). Before writing each row it confirms with
`git show --stat <commit>` that the commit touches the named code:

| Row | Mark |
|---|---|
| C-2 | Fixed in `52abebb` (6.1) |
| C-4 | Fixed in `4c9fda6` and `0f31027` (6.2) |
| C-5 | Fixed in `9f419bb` (6.1) |
| C-6 | Fixed in `ba9a5c1` (6.3) |
| C-7 | Fixed in `c37943a` (6.3): smog now never draws in front of a unit, a deliberate trade |
| C-8 | Fixed in `7117874` and `93afd13` (6.5) |
| C-9, C-10 | Fixed in `7117874` (6.5) |
| C-13 | Fixed in `12296f2` (6.3); the 16-cavity cap is kept on purpose |
| C-14 | Fixed in `ab5ef61` (6.5) |
| C-15 | Fixed in `257824d`, `52620f6`, `00eff7c` (6.6); proof by test in WP2 and WP3 of this plan |
| C-16 | Fixed in `8392b40`, `07f7014`, `64fd330` (6.6) |
| E-1 | Fixed in `9875f48`, `bce31af` (7.1) |
| E-2, E-6 | Fixed in `1e03482` (7.2) |
| E-3, E-7 | Fixed in `270217a` (7.3) |
| E-4, E-5 | Fixed in `956682c` (7.4) |
| E-8 | Fixed in `96eab79` (7.5) |
| E-9 | Fixed in `ca953f5`, `69dd84e` (7.5) |
| E-10 | Fixed in `b9c65f3`, `1318d30` (7.5) |
| B-2 | Fixed in `b342c77` (8.1) |
| B-3, B-4 | Fixed in `4c425c8` (8.2) |
| B-5, B-9 | Fixed in `d3f70d5` (8.3) |
| B-6, B-7, B-8 | Fixed in `cf784cd` (8.4) |
| Q-4 | Add: client half fixed in `64fd330` (step 1.8 deferred it to Phase 6) |
| B-1 | Unchanged: open, 8.5, Q0001 |

New finding, added by WP1 to §4.4 with evidence tag **[V]** (read in source, not run):

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| C-17 | Med | **[V]** | Every combat effect creates its own `CombatFxMaterial` asset: one per tracer, projectile, beam and slash. In a 400-unit fight that is hundreds of material creations and removals a second, each with its own uniform buffer and bind group. It is event-driven, so outside the steady-state claim, but it is the most likely asset cost at 400 units. WP6 measures it; no fix in this plan | `vfx.rs` `hitscan_tracer`, `projectile`, `beam`, `melee_slash` |

The Phase 6, 7 and 8 step tables get a `Status` column like Phase 5's and Phase 9's. They are filled from §2.1 to §2.3.

### 2.5 Throughput: is today's lower figure a regression? (measured)

The handover (§5) reported 11,114 ticks/s on an empty 64×64 map, 2,706 at 100 units and 639 at 400 units. Today's
`DESIGN.md` §5 shows 3,395, 1,187 and 200. The handover figures came from Appendix A's `r12` repro, rebuilt on the
owner's notebook. Today's figures come from `cargo bench -p sim_core --bench throughput` on this server.

**The benchmark exists at every commit worth comparing.** `crates/sim_core/benches/throughput.rs` was added in
`412aefa` (2026-11-01), the commit after the audit. Its scenario table (`throughput_scenarios.ron`) has not changed
since then. The benchmark has the same layout as `r12`: `pulse_bot`, 20-wide formations in opposite corners,
attack-move across the map, 300 ticks measured. So no port was needed. Each commit was exported with
`git archive <commit> | tar -x -C ~/scratch/planexits_old_<commit>` and built with its own pinned toolchain
(`rust-toolchain.toml` is `1.99.0` at every one of them). Each was then run with `cargo bench -p sim_core --bench
throughput --no-run --locked` and the binary run directly. Rounds were interleaved, so load drift hits every commit
alike. Logs: `~/scratch/logs/planexits_bench_compare.log` and `~/scratch/logs/planexits_bench_bisect.log`.

Measured 2026-11-03 16:17 to 16:28 UTC, load average (1 min) 2.7 to 9.4 on 16 cores. The table shows the median of
three repeats per run (the benchmark's own figure), and the range over three interleaved runs:

| Commit | What it is | 64², 0 | 64², 100 | 64², 400 | 128², 400 | 256², 0 |
|---|---|---:|---:|---:|---:|---:|
| `412aefa` | bench added, before Phases 2 to 4 | 774–813 | 345–355 | 72 | 56–59 | 48–54 |
| `d4109e2` | Phase 4 movement, before `ae075fb` | 766–775 | 338–340 | 89 | 74–75 | 48 |
| `ae075fb` | "stop paying per tick for map area and unchanged sight" (Q-3) | 3385–3401 | 1236–1238 | 200–202 | 226–227 | 213 |
| `3a2865e` | Phase 5 game loop | 3375–3404 | 1197–1228 | 199–203 | 221–228 | 212–213 |
| `b263734` | the handover's commit | 3356–3396 | 1205–1221 | 200–202 | 227–228 | 211–213 |
| `2be8992` (`main`) | today | 3376–3405 | 1191–1207 | 192–195 | 216–219 | 212–213 |

Conclusions:

1. **No regression since the handover.** `b263734` and `main` measure the same on this server to within a few
   percent. The step in throughput came from `ae075fb`, and every later commit keeps it.
2. **The handover's figures are not comparable.** At no commit does this server come near 11,114 or 639. At the
   audit's code, the notebook measured 969 on the empty 64×64 map (Appendix A) and this server measures 774 to 813,
   about 1.2× slower. But 11,114 against 3,400 is 3.3×, which host speed alone does not explain. The rebuilt `r12`
   must have differed from Appendix A in a way the handover does not record. WP7 runs the `r12` source as printed
   in Appendix A on this server to settle that. It is not a code question: the benchmark is the measurement of
   record (charter §3.1).
3. **A small residual:** `main` is 3 to 4 % slower than `b263734` on both 400-unit rows (192–195 against 200–202;
   216–219 against 227–228), consistently across six interleaved runs, and level elsewhere. Candidates in
   `git log b263734..main -- crates/sim_*/src` are `866563f` (`PlayerView` written every tick), `90d9e25`
   (`PerPlayer` indexing) and `99bb566` (classes from data). WP7 bisects it. It is far above the 25 ticks/s floor,
   so it blocks nothing.

### 2.6 Gaps from the Phase 9 blueprint (§9) and whether they belong here

| Gap (Phase 9 blueprint §9) | Belongs to | Decision |
|---|---|---|
| No test pins `sample_input_ownership.after(lab_ui_system)` (WP6 finding) | Phase 6, "one owner per concern" (input ownership) | **In this plan**, WP4 |
| No test pins the editor's `apply_document_load_system.before(editor_ui_system)` (WP6 finding) | Phase 7, step 7.4 ("one `load_document`") | **In this plan**, WP4 |
| Above 512 bubbles, `step_bouncing_kinematics` reports 0 peer bounces (WP7 finding) | Phase 6 by subject (smog, C-13), not its exit | Not here: a diagnostics counter only; it changes a pinned golden, so it belongs to a smog bugfix plan with Q0014 in view |
| Terrain mesher leaves the last dual column or row of a side that is a multiple of 16 (WP4 finding) | Client rendering; nearest is Phase 6 | Not here: a visible rendering bug, not part of any exit; needs its own failing test and a deliberate spawn-layout change |
| `MapDocument::encode` panics on a cell count mismatch (WP2 finding) | `sim_map` library (charter §1.1); the editor's save path calls it but always passes consistent documents | Not here: a separate bugfix plan |
| Golden replay does not pin two phase orders (WP1 and WP3 findings) | Simulation, Phases 3 and 4 | Not here |
| Smog goldens depend on the host `libm` (Q0014) | Owner question | Not here |

The four gaps marked "Not here" stay listed in `DESIGN.md` §4 as they are today.

---

## 3. Design decisions

### D-1. Recording exits

A phase's exit is recorded only after its criteria pass on CI (remediation §5, rules). WP1 records **Phase 7** as
met, citing run 41665216936 at `6b39cd2`. It records **Phase 8** as "steps 8.1 to 8.4 done; exit not met: the
failed-download criterion is met, the strict-audit criterion waits for 8.5 (Q0001)". **Phase 6** is recorded by WP6
only after WP2 to WP5 are merged and a CI run on `main` that contains them is green. Its paragraph names that run, the
frame-budget evidence file and the state of Q0015.

### D-2. Proving "no per-frame heap allocation or asset churn" under Q0011's default

Q0011's default: no counting `#[global_allocator]`, because it needs `unsafe`, and the §3.3 allocation claim stays
tagged Review until the owner answers. This plan does not touch that default. The proof has three layers, and each
says exactly what it shows.

1. **Asset churn (WP2, CI).** In the real client App, over steady frames, no `AssetEvent` of any kind (`Added`,
   `Modified`, `Removed`, `Unused`, `LoadedWithDependencies`) is raised for any asset type the client registers.
   The one exception: a moving frame may modify a `BubbleDomeMaterial` only for a unit whose heading changed (the C-5
   rule). No entity is spawned or despawned. This is complete for asset churn: Bevy raises an event for every
   creation, mutable access and removal through `Assets<T>`.
2. **Retained buffers (WP2, CI).** For every sync system that owns a growable buffer, the buffer's address and
   capacity stay the same over steady frames. This shows reuse. It does not show that no temporary is allocated and
   freed within the frame, as Q0011 points out.
3. **Allocation count (WP3, external).** Valgrind's `memcheck` replaces `malloc` from outside the process and
   reports "total heap usage: N allocs" at exit. Each Rust allocation reaches `malloc` through `std::alloc::System`.
   A test child runs the same warmed steady-state phase for F₁ and then F₂ frames, in two otherwise identical
   processes. The allocation count must not grow with the frame count: N(F₂) − N(F₁) = 0. No code of ours replaces
   the allocator, so no `unsafe` is added and `unsafe_code = "forbid"` holds everywhere. The test file already
   anticipates this kind of profiler: `pipeline_presentation_tests.rs` keeps `#[inline(never)]` measured phases "for
   external call-stack profiling".

Layer 3 is a fourth option to Q0011's list. It does not answer Q0011, which stays open: the owner may still prefer
an in-process counter. WP3 records this in Q0011 under a new `## Note from an agent` heading, without changing its
question or default. If WP3's test runs on CI, the §3.3 tag changes from "[Review; …]" to name the test (WP3 step 6).
That is a charter edit outside §6, as Phase 9 did with its tags.

### D-3. "One owner per concern" as a test

Bevy keeps, for every system, the set of components and resources it may write (`System::component_access()`,
`Access::has_resource_write`, `Access::has_component_write`; bevy_ecs 0.15.4, `src/query/access.rs:278` and `:301`).
WP4 builds the production App, initialises its schedules and computes, for each concern on a declared list, the set
of systems that can write it. The test compares that set with a table in the test file. A new writer fails the test
with its system name, so it has to be argued in review and added to the table on purpose.

Limits, stated in the test's `//!` block: deferred `Commands` writes (inserts, spawns) are not in a system's access.
Exclusive systems (`apply_deferred`) have write access to everything and are skipped by name.

Ordering is proved separately. The test checks that the schedule's dependency graph has a path from the first
system to the second. Reversing the constraint, or deleting it, then fails the test even when no cycle results.

### D-4. Frame-budget measurement

The measurement must be reproducible, run on the RX 580, and change nothing in the shipped game or shipped data.

- **Where the tool lives:** `crates/client_diagnostics`, the crate for capture, audit and probe tools (D3), as a new
  binary `frame_budget`. It builds the real client with `client::build_graphical_app(ClientMode::LiveBattle, None,
  scenario)` and adds a recorder plugin. The shipped `client` binary gains no flag.
- **Scenario and spawning:** through the production scenario path, with no direct spawning. The tool builds a
  temporary assets root under a work directory given on its command line (`~/scratch/frame-budget/root`). It
  hard-links `textures/` and copies the text directories, the way `crates/client/tests/support/battle_harness.rs`
  `fixture_root` does. Into that copy it writes:
  - a flat map `maps/frame_budget_arena.scmap.ron`, 128×128 cells, theme from the spec, with two start locations
    whose positions come from the spec. The map is generated through the `sim_map` map-document API and must pass
    the map gate;
  - two loadouts appended to the copied `catalog/start_loadouts.ron`: per side, 100 units (the 200 scenario) and 200
    units (the 400 scenario). Their mix is the shipped source loadout named in the spec (`battle_automaton` today:
    2 welder, 8 pulse, 2 rhino), scaled proportionally with largest-remainder rounding so each side has exactly N.
    Several entries of one slug are used if a count exceeds `u8`;
  - `scenarios/frame_budget_{0,200,400}.ron`, both players attack-moving to the other's start, like
    `default_battle.ron`. The 0 scenario has the two town halls and no units. It is the baseline: map, smog, HUD and
    presentation with nothing on the field.

  No unit slug appears in Rust (C5). The source loadout slug, counts, map size, start cells and timings live in a
  data file, `crates/client_diagnostics/data/frame_budget.ron`, read at run time from a path argument whose default
  is resolved from `CARGO_MANIFEST_DIR`. Nothing is embedded, so C6 does not apply.
- **What is "200 units":** 200 mobile units alive at the start of measurement, 100 per side, plus the two town halls.
  The report records the live unit count on every frame, because units die during the window.
- **Camera:** a fixed pose over the midpoint between the start locations, high enough that the spec's frustum check
  holds. On every measured frame the recorder projects every live unit's position through the real camera and
  counts those inside the view. If fewer than 95 % are in view on any measured frame, the run fails: a budget for
  units that are not drawn measures nothing (charter §1.2).
- **Window and presentation:** 1280×720, the client's default. `PresentMode::AutoNoVsync` is set on the primary
  window after the App is built, so the frame interval reflects work, not the display. Production settings
  otherwise: smog, HUD, fog and effects as the player gets them.
- **Timing:** `std::time::Instant` (allowed outside the simulation crates). Per frame, the recorder writes into a
  vector preallocated for the whole window (no allocation while measuring):
  - frame interval (wall time between consecutive `First` schedules): what the player sees;
  - main-world update time (`First` to `Last`): the CPU side;
  - simulation time, from a system before `BattleSet::AdvanceTicks` to one after it, and the number of ticks
    executed that frame;
  - live units, units in view, combat events that frame, and the simulation tick.

  GPU time is not measured: wgpu timestamp queries need an optional device feature and a separate readback, beyond
  this plan. Bevy renders in a pipelined thread, so the frame interval already includes a GPU-bound frame as a long
  interval.
- **Warm-up:** measurement starts on the first frame after the battle's media and map presentation are ready (the
  conditions `frame_limit_system` checks) plus 180 frames, so pipeline compilation and first-use uploads are excluded.
  It then runs for 1,500 simulation ticks (60 s of game time), which covers the approach and the whole engagement at
  both sizes. The tool fails if the battle ends or fails before that.
- **Statistics:** per run, the 50th, 95th and 99th percentile and the maximum of the frame interval, plus the 50th
  and 95th of main-world update time and simulation time. Percentiles use nearest rank, unit-tested. The same is
  reported over the combat window: frames from the first combat event until 750 ticks later.
- **Repeats and load:** 5 runs per scenario, interleaved (0, 200, 400, 0, 200, 400, …), so load drift hits all three
  alike. Each run records `/proc/loadavg` before and after, `nproc`, the time spent waiting for the GPU lock, the
  commit, Mesa version and adapter name. The recorded figure for a scenario is the median over runs of each
  statistic. A run is "loaded" when its starting one-minute load average exceeds 8 (half the cores). At least 3
  unloaded runs are required per scenario. If the server will not give them within the session, the median of all
  runs is recorded, marked as loaded and with the load stated. Unrecorded measurements do not exist.
- **GPU discipline (charter §7):** build first, outside the lock. Then one run per locked command:
  `WGPU_BACKEND=vulkan VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/radeon_icd.json MESA_VK_WSI_DEBUG=sw with-gpu-lock
  xvfb-run -a -s "-screen 0 1280x720x24" <release frame_budget> --scenario 400 --out <dir>/run-NN.ron`. A run takes
  about 90 to 150 s, inside the default 600 s lock timeout. The lock is released between runs.
- **What gets recorded where:** raw reports under `~/scratch/frame-budget/<date>/` (not committed). One evidence
  file, `docs/evidence/<date>-frame-budget.md`, with the exact commands, environment, the per-run table and the
  medians. `DESIGN.md` §5 "Frame budget" gets a three-row table (0, 200, 400) with frame-interval p50, p95, p99 and
  max, main-update p50 and simulation p50, the reference lines of Q0015 and a link to the evidence. The Phase 6 exit
  paragraph quotes the 200 and 400 medians.
- **What CI can check, and what it cannot:** CI has only `lavapipe` on shared runners. Frame times there say nothing
  about a GPU, so **no CI job measures or gates frame time.** What CI checks (WP5): the tool builds; its fixture
  root, map and loadouts pass the real loaders and yield exactly 0, 200 and 400 units; and a short smoke run (30
  measured frames at 400 units, through the GPU test harness, on `lavapipe` in CI and on `radv` locally) writes a
  valid report: 30 frames, 400 live units on the first, at least 95 % in view, finite statistics, and no Vulkan
  validation error. That keeps the measurement working; the numbers stay local evidence.

### D-5. File ownership and order

Work packages own disjoint files (§4). `docs/plans/2026-11-01-codebase-remediation.md` is edited by WP1 (§4 rows,
step `Status` columns, Phase 7 and 8 paragraphs) and later by WP6 (the Phase 6 exit paragraph only). `DESIGN.md` is
edited by WP4 (the "Selection and input" and "Map editor" rows) and WP6 (§5). `AGENTS.md` is edited only by WP3, and
only §3.3's tag. Charter §6 is not touched (Q0003).

---

## 4. Work packages

| WP | Title | Closes | Size | Depends on | Group |
|---|---|---|---|---|---|
| WP1 | Bookkeeping: finding rows, step status, Phase 7 and 8 records | §2.2 to §2.4 | S (2 to 3 h) | none | A |
| WP2 | Steady-state sync tests: asset churn, entities, retained buffers | Phase 6 exit 1 (layers 1 and 2) | M (1 to 2 days) | none | A |
| WP3 | External allocation count under valgrind | Phase 6 exit 1 (layer 3) | M (1 day, after its feasibility step) | none | A |
| WP4 | Writer table and ordering tests | Phase 6 exit 2; Phase 9 §9 ordering gaps | S-M (1 day) | none | A |
| WP5 | `frame_budget` tool and its CI smoke test | Phase 6 exit 3 (tool) | M (1 to 2 days) | none | A |
| WP6 | Measure on the RX 580; record; Phase 6 exit paragraph | Phase 6 exit 3 (record) and the Phase 6 record | S (half a day of agent time, 1 to 2 h of locked GPU in single runs) | WP2, WP3, WP4, WP5; WP7 for its note | B |
| WP7 | Throughput residual and the handover figures | §2.5 items 2 and 3 | S (half a day) | none | A |

**Parallelism.** Group A runs in parallel: WP1 to WP5 and WP7 have disjoint files (each WP's "Files" list).
At most three gates at once (charter §7, Phase 9 experience): start WP2, WP4, WP5 first, then WP3, WP7, WP1.
Group B (WP6) starts when all of group A is merged and CI on `main` is green.

Each package works in its own worktree with `CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/exits-wpN`,
checks `df -h / ~/scratch` first, and removes its target directory after merge. Common gate before each commit:

```bash
export CARGO_TARGET_DIR=/home/spacescuffle/scratch/cargo-target/exits-wpN
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked    # plain: the GPU harness takes the lock per child; never wrap it
```

Rules for every package: failing test first (a red run pasted into the agent's report). Each commit names the
finding (C-15, C-16, …) and the step. Every new Rust file begins with `//!` (responsibility, fit, invariants). No
`unwrap`. `expect` only with an `invariant:` message. No catalog identity in Rust. No function over 100 lines
outside tests. Check `docs/questions/open/` before each push.

---

### WP1: Bookkeeping

**Files (owned):** `docs/plans/2026-11-01-codebase-remediation.md` (§4.4, §4.6, §4.7 rows; Phase 6, 7 and 8
step tables; Phase 7 and Phase 8 paragraphs), `docs/plans/README.md` (this plan's row: status `Active`).

**Steps**

1. For each row in §2.4, run `git show --stat <commit>` and confirm it touches the named code. If it does not,
   write the row as it really is and say so in the report.
2. Mark the rows as in §2.4. Add C-17 to §4.4.
3. Add a `Status` column to the Phase 6, 7 and 8 step tables, filled from §2.1 to §2.3 (8.5: "Blocked on the owner,
   Q0001").
4. Under Phase 7, add: **"Exit status (2026-11-03): met; CI green on `main` at `6b39cd2` (run 41665216936).**
   `editor_app_tests.rs::edit_save_playtest_return_and_reopen_round_trip` (`b9c65f3`) …", plus the limits of §2.2.
5. Under Phase 8, add: **"Exit status (2026-11-03): not met.** Steps 8.1 to 8.4 are done. A failed download cannot
   enter the cache (`veo_reliability_tests.rs`, `b3_*`, green on CI at `6b39cd2`). The strict audit waits for 8.5,
   which is blocked on the owner (Q0001)." Add the two notes of §2.3 for the 8.5 run.
6. Do not write a Phase 6 exit paragraph (WP6).

**Red run.** Not applicable (documentation). Check C17 to C19 instead: `cargo test -p workspace_checks`.

**Gate.** Common gate (C18 and C19 run in it).

---

### WP2: Steady-state sync tests

**Files (owned):** new `crates/client/tests/sync_steady_state_tests.rs`. Production files only if a test exposes a
real defect (step 6).

**Design.** One test binary, using `#[path = "support/battle_harness.rs"]` (unchanged) to boot the production
`ClientAppPlugin` without a renderer. The test adds an observation plugin of its own:

- `AssetEventTally`, one counter per asset type and event kind. It is filled in `Last` by a generic system
  `tally::<T: Asset>` that reads `EventReader<AssetEvent<T>>`. It is registered for every asset type the client
  registers: `BubbleDomeMaterial`, `SelectionRingMaterial`, `StandardMaterial`, `CombatFxMaterial`,
  `SmogVolumeMaterial`, `TerrainMaterial`, `Image`, `Mesh`. **verify** the list: the test's first assertion compares
  it with the asset types registered in the App (`AppTypeRegistry` or `world.components()` resource IDs of
  `Assets<_>`), so a new asset type fails the test until it is added.
- `EntityCensus`: the entity count after `Last`.
- A list of retained buffers, each read by a closure from a public field or accessor and recorded as
  `(name, ptr as usize, capacity)`. Where a buffer has no public read path, add a `#[doc(hidden)] pub fn
  retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)>` to the owning type, as `HudModel`
  already exposes what `battle_hud_tests.rs` reads.

**Tests (write first, in this order)**

1. `asset_types_under_watch_are_every_asset_the_client_registers`.
2. `paused_battle_frames_create_modify_and_remove_nothing`: `Harness::battle()`, wait until media and map
   presentation are ready, warm 30 frames, reset tallies, step 64 paused frames. Every tally is zero; the entity
   count is unchanged; every retained buffer keeps its address and capacity. This covers every system in the §2.1
   inventory at once.
3. `selected_structure_keeps_card_queue_and_feed_buffers_on_every_tick`: the skirmish (`Harness::boot(Skirmish)`),
   select the local town hall, resume, run 30 ticks of warm-up, then 200 ticks. Buffers of `CommandCard`,
   `QueuePanel`, `BattleFeed` and the focus model keep address and capacity; no asset tally other than
   `BubbleDomeMaterial::Modified` for turning units is non-zero. **verify** the skirmish's workers move in that window;
   if they do not, order them to harvest first, through input events.
4. `marching_frames_touch_only_turning_units`: the default battle, resumed, over the ticks before the first combat
   event (detect through `CombatVisualEvent`; fail if contact comes before 50 ticks, so the window is real). There
   are no `Added` or `Removed` events of any type, the entity count is constant, and `Modified` events occur only on
   `BubbleDomeMaterial`. Their count is at most the number of (unit, frame) pairs whose heading changed (the rule
   `pipeline_presentation_tests.rs` uses).
5. `swarm_stress_frames_create_modify_and_remove_nothing`: `Harness::boot(SwarmStressTest)`, warmed, 64 frames, all
   tallies zero except smog field writes, which go through `Queue::write_texture` in the render world and raise no
   event.

**Mutation red runs** (make, see the named test fail, revert; paste each failure):

- In `scene_controller.rs` `update_live_instance`, call `bubble_materials.get_mut(handle)` unconditionally: test 2
  fails on `BubbleDomeMaterial::Modified`.
- In `indicators.rs` `update_indicators_system`, add one `materials.add(StandardMaterial::default())` per frame:
  test 2 fails on `StandardMaterial::Added`.
- In `battle_hud/queue.rs` `refresh_queue_panel`, replace a reused `String`'s `clear()` with `= String::new()` before
  writing: test 3 fails on that buffer's address.
- In `scene_controller.rs` `sync_swarm_stress_system`, spawn one empty entity per frame: test 5 fails on the census.

**Step 6, if a test fails for real:** it is a finding. Record it in the remediation plan's §4.4 as C-18 and onward,
with tag **[R]**, fix it in its own commit naming that ID, and keep the test.

**Gate.** Common gate.

---

### WP3: External allocation count under valgrind

**Files (owned):** new `crates/client/tests/sync_allocation_count_tests.rs`; `.github/workflows/ci.yml` (add
`valgrind` to the `check-and-test` apt list); `AGENTS.md` §3.3 tag (step 6 only);
`docs/questions/open/0011-allocation-counter-unsafe.md` (a `## Note from an agent` section only; the question and
default are not edited).

**Feasibility, measured while planning (§5.2).** Read §5.2 before step 1. It records whether a Bevy test child runs
under `valgrind` on this server, how long it takes, and whether the allocation count is stable between identical
runs.

**Design**

- A parent test, `steady_state_frames_allocate_nothing`, re-executes its own test binary
  (`std::env::current_exe()`) under `valgrind --tool=memcheck --leak-check=no --error-limit=no` with
  `--exact <child test> --nocapture --test-threads=1`. It does this four times: F = 64, 128, 64, 128, passed as
  `SPACE_SCUFFLE_STEADY_FRAMES`. It parses `total heap usage: N allocs` from each child's stderr.
- The child test (`#[ignore]`-free, but it returns at once unless `SPACE_SCUFFLE_STEADY_FRAMES` is set, and says so
  on stdout) builds the same app as `pipeline_presentation_tests.rs` `presentation_app`. That is the production live
  synchroniser on the real default battle, with `MinimalPlugins`. Its executor is single-threaded
  (`ExecutorKind::SingleThreaded` on every schedule), so thread scheduling cannot change the count. It warms 64
  frames, then runs F frames, paused, then exits.
- **Baseline subtraction.** The paused frame allocates about 25 times today, counting Bevy's own work (§5.2). So
  a second child, `baseline_frames`, builds the identical app without the client's sync systems: the same plugins,
  resources and executor, but without `sync_live_battle_system`, `refresh_battle_presentation` and the other
  presentation systems the first child adds. Each child's per-frame slope is S = (N(128) − N(64)) / 64. The
  assertion is S(full) = S(baseline): the client's sync systems add no allocation per frame. Bevy's own per-frame
  allocations are reported, not asserted.
- Assertions: the two runs at the same F report the same N (the measurement is deterministic), and
  S(full) − S(baseline) = 0. On failure the message shows all counts and both slopes.
- `valgrind` missing: the parent test fails with an instruction to install it. There is no skip and no opt-out
  (charter §1.2). CI installs it from Ubuntu's archive in the same `apt-get install` as `xvfb` and `ffmpeg`. That is
  not a network binary run by an agent.
- Positive control, a second child, `planted_allocation_is_counted`: the same app, plus a test-only system in
  `Update` that does `std::hint::black_box(Vec::<u8>::with_capacity(64))`. The parent asserts N(128) − N(64) = 64 for
  it. Without this, a counter that sees nothing would pass.
- Second scope, if time allows: the same pair over a ticking (not paused) frame window of the default battle before
  first contact. If ticks allocate inside the simulation (they do: commands and snapshot), assert instead that the
  allocation count per tick is constant between windows and record the number. Do not claim zero for that path.

**Steps**

1. **Attribute the 25 allocations per frame first.** Run the probe of §5.2 under `valgrind --tool=dhat` at 64 and
   128 frames and diff the allocation sites. Record which are Bevy's and which are the client's in the report. Any
   client site is a real finding: record it (C-18 onward, **[R]**) and fix it in its own commit before writing the
   assertion. If the count is unstable between identical runs even with the single-threaded executor, do not write
   the test. Write the instability into
   Q0011's note, keep the §3.3 tag at Review, and stop. WP2 still closes the asset-churn half, and WP6 records the
   allocation half as "not proved by test; Q0011".
2. Write the positive control first and see it pass the parent (64 allocations over 64 frames).
3. Write `steady_state_frames_allocate_nothing`. If it fails, the failure lists the allocating frames' count. Find
   the allocation (rerun the child under `valgrind --tool=dhat` at F = 64 and 128 and diff the call stacks), record
   it as a finding (C-18 and onward, **[R]**), fix it in its own commit and rerun.
4. Mutation red run: insert `let _scratch: Vec<u8> = Vec::with_capacity(1);` in `update_live_instance` (or the
   live sync's per-instance helper). The parent test fails with a per-frame average of one allocation per bubble.
   Revert.
5. Add `valgrind` to `ci.yml` and measure the test's time on CI. If it exceeds 10 minutes, move it to its own job
   that reuses the debug test build. Do not drop it.
6. When it is green on CI, change charter §3.3's tag to "[CI: `sync_allocation_count_tests.rs` (counted by valgrind
   for the live battle sync), `sync_steady_state_tests.rs` (assets, entities, retained buffers); Review for the
   rest]". Add the note to Q0011: what was built, that it needs no `unsafe`, and that it measures the live
   synchroniser's steady state, not the whole frame. The owner can still choose option 1.

**Gate.** Common gate, plus the parent test passing on this server.

---

### WP4: Writer table and ordering tests

**Files (owned):** new `crates/client/tests/system_ownership_tests.rs`; `crates/client/tests/editor_app_tests.rs`
(one new test at the end, using the existing `EditorHarness`); `DESIGN.md` §4 rows "Selection and input" and "Map
editor".

**Tests (first)**

1. `every_concern_has_its_declared_writers`: boot the production client with `battle_harness.rs` (Player profile,
   the skirmish). Initialise every main-world schedule (`First`, `PreUpdate`, `Update`, `PostUpdate`, `Last`) and,
   for each concern below, collect the names of the systems whose access can write it. Compare with the table in the
   test:

   | Concern | Type | Declared writers |
   |---|---|---|
   | Authoritative world | `BattleSession` | read by the implementer and listed (expected: `manage_battle_scene`, `advance_battle_ticks`, the command commit systems) |
   | Selection | `BattleSelection` | the battle input systems that select, and `sync_live_battle_system` for reconciling departed units **verify** |
   | Bubble materials | `Assets<BubbleDomeMaterial>` | `sync_live_battle_system`, `sync_lab_preview_system`, `sync_swarm_stress_system` (snapshot lanes); `sync_optics` (optics lanes); spawn paths named as found |
   | Smog field | `SmogGrid3d`, `SmogGpuFrame` | `update_smog_field_system` |
   | Fog | `BattleFog` | `refresh_battle_fog` |
   | HUD model | `HudModel` | `refresh_hud_model`, `refresh_hud_focus` |
   | Camera | `RtsCamera` (component) | `camera_control_system`, `sync_scene_camera` |
   | Input ownership | the resource `sample_input_ownership` writes | `sample_input_ownership` |
   | Terrain | `TerrainChunks` | `update_terrain_system` |

   Systems with `has_write_all()` (`apply_deferred`) are skipped and listed in the test's doc. The table is
   documentation: a reviewer reading it sees each concern's owners. A name that cannot be read through
   `System::name()` uses the full path Bevy reports.
2. `input_ownership_is_sampled_after_the_inspector_draws`: the dependency graph of `Update` has a path from
   `lab_ui_system` to `sample_input_ownership`. **verify** which `ScheduleGraph` accessor gives the flattened
   system-to-system edges in bevy_ecs 0.15.4 (the `.after(fn)` edge goes through the function's `SystemTypeSet`).
   If none is public, build the graph from `ScheduleGraph::dependency()` and the hierarchy, resolving set nodes to
   their member systems.
3. In `editor_app_tests.rs`: `document_load_applies_before_the_editor_ui_reads_it`, the same check for
   `apply_document_load_system` before `editor_ui_system` in the editor App.

**Mutation red runs:** add a second writer (a test-only system with `ResMut<BattleFog>`) through the production
plugin path → test 1 fails, naming it. Change `.after(lab_ui_system)` to `.before(lab_ui_system)` in `lib.rs` → test
2 fails. Delete the editor's `.before(editor_ui_system)` → test 3 fails. Revert each.

**Docs.** In `DESIGN.md` §4, replace "No test pins the order of client systems that could run either way without a
schedule cycle (Phase 9 blueprint §9)" with a link to the new tests, and add the editor test to the "Map editor"
row's evidence.

**Gate.** Common gate.

---

### WP5: The `frame_budget` tool and its CI smoke test

**Files (owned):** new `crates/client_diagnostics/src/frame_budget/` (`mod.rs`, `fixture.rs`, `recorder.rs`,
`report.rs`), new `crates/client_diagnostics/src/bin/frame_budget.rs`, new
`crates/client_diagnostics/data/frame_budget.ron`, `crates/client_diagnostics/src/lib.rs` (one `pub mod` line),
`crates/client_diagnostics/Cargo.toml` (only if a dependency is missing; prefer `ron`, `serde` and `clap`, already
used), new `crates/client_diagnostics/tests/frame_budget_tests.rs`.

**Design** (D-4): the binary takes `--spec <path>` (default from `CARGO_MANIFEST_DIR`), `--work-dir <dir>` (required),
`--scenario <0|200|400>` (a key into the spec), `--out <report.ron>` and `--measure-frames <n>` (smoke runs only;
it replaces the tick window). It:

1. builds or refreshes the fixture root in `<work-dir>/root` (`fixture.rs`), refusing a work directory inside the
   repository's `assets/` (the B-8 rule);
2. sets `SPACE_SCUFFLE_ASSETS` for its own process before building the App, or passes the root through whatever
   `ResolvedAssetsRoot` offers, whichever `build_graphical_app` honours **verify**;
3. builds the App, sets `PresentMode::AutoNoVsync` and the camera pose, adds the recorder, runs, and writes the report
   (`report.rs`: per-frame rows, environment block, summary) as RON plus a Markdown summary on stdout;
4. exits non-zero with a typed error when the battle fails, the frustum check fails, the window ends early or the
   scenario has the wrong unit count.

**Tests (first)**

- Unit, in `report.rs`: nearest-rank percentiles (known vectors, empty input is an error, a single sample);
  combat-window selection.
- Unit, in `fixture.rs`: largest-remainder scaling sums to exactly N for N = 0, 1, 100, 200, 255, 256 (the `u8`
  split) and keeps the source ratio within one unit.
- Integration, no GPU: `fixture_root_scenarios_spawn_exactly_their_units`. Build the root in a temp directory, load
  each scenario with `client::runtime_battle::BattleDefinition::load` and `instantiate`, and count mobile units per
  player: 0/0, 100/100, 200/200. This also proves the map passes validation and the spawn ring has room. If it
  reports `NotEnoughSpawnCells`, move the start cells inward in the data file, not in code.
- Integration, GPU, through the harness: `frame_budget_smoke_run_writes_a_valid_report`. It includes
  `crates/client/tests/support/gpu_run.rs` with `#[path = "../../client/tests/support/gpu_run.rs"]`, the way client
  tests include it, and runs the debug `frame_budget` binary (`env!("CARGO_BIN_EXE_frame_budget")`) with
  `--scenario 400 --measure-frames 30` under the harness's validated run. It asserts the report has 30 frames, 400
  live units on the first, at least 95 % in view on every frame, and finite statistics, and that the run had no
  Vulkan validation error. Check C21 still passes: it must recognise this as going through the harness. **verify**
  that the `GpuCommand`-style builder in `gpu_run.rs` (around `pub fn run_validated`) accepts an arbitrary
  executable. If it only re-runs test children, run the binary from a test child instead.

**Red runs.** Before `fixture.rs` exists, the integration test fails to compile. Then make the scaling drop the
remainder: the 200 scenario yields 198 or 199 per side and the test fails. In the smoke test, aim the camera away
from the map (temporarily): the run fails the frustum check. Revert each.

**Gate.** Common gate. Note the smoke test's time on `lavapipe`
(`SPACE_SCUFFLE_GPU_TEST_DRIVER=lavapipe cargo test -p client_diagnostics --test frame_budget_tests`). If it exceeds
the harness's 120 s child limit, lower the frames, not the unit count.

---

### WP6: Measure on the RX 580 and record

**Files (owned):** new `docs/evidence/<date>-frame-budget.md`; `DESIGN.md` §5 ("Frame budget"); the Phase 6 section
of `docs/plans/2026-11-01-codebase-remediation.md` (exit paragraph only); this plan's row in `docs/plans/README.md`.

**Steps**

1. `git pull`; read `docs/questions/open/` (Q0011 and Q0015 may be answered; follow their answers).
2. Build outside the lock: `cargo build --release -p client_diagnostics --bin frame_budget --locked`.
3. Prepare the root once: run the tool's fixture step (`--scenario 0 --measure-frames 1` on `lavapipe` is enough,
   no lock).
4. Fifteen locked runs, one command each, in the interleaved order of D-4, with the exact command of D-4. Before
   each run, append `date -u`, `cat /proc/loadavg` and `nproc` to `~/scratch/logs/frame_budget_<date>.log`. Never
   wrap more than one run in one lock. If a run is loaded (one-minute load above 8), still keep it. Add runs at the
   end until each scenario has three unloaded runs, or stop after 30 runs and record what there is.
5. Also save one screenshot of the 400 scenario at the middle of the combat window (`--screenshot-at-tick`, if WP5
   provides it; otherwise a separate `battle_capture` run). Look at it: units visible, smog behind units (C-7).
   Commit it as `docs/evidence/<date>-frame-budget-400.jpg` (under 300 KB), because the handover lists "smog in a
   live battle was not visually confirmed".
6. Write the evidence file: commit, Mesa version, adapter, resolution, present mode, `MESA_VK_WSI_DEBUG=sw` and what it
   costs, the per-run table, the medians, the combat-window medians, the lock waits, the load per run, and C-17's
   visible effect if any (combat-window p99 against march-window p99).
7. Replace the "Frame budget" paragraph in `DESIGN.md` §5 with the three-row table and the reference lines (Q0015).
   Add one sentence from WP7's result to the throughput paragraph.
8. When CI on `main` is green with WP2 to WP5 merged, add the Phase 6 exit paragraph:
   **"Exit status (<date>): met; CI green on `main` at `<sha>` (run <id>)."** It names the tests of WP2 to WP4 and the
   evidence file, quotes the 200 and 400 medians, and states what the allocation proof covers: WP3's scope, or
   "asset churn and retained buffers only, Q0011" if step 1 of WP3 stopped. Set this plan's row to `Done`.

**Red run.** Not applicable. The measurement is checked by its own refusal paths, tested in WP5.

**Gate.** Common gate (C17 checks the new links in `DESIGN.md`).

---

### WP7: Throughput residual and the handover figures

**Files (owned):** new `docs/evidence/<date>-throughput-history.md`. No code change on `main`. The repro crate lives
under `~/scratch`.

**Steps**

1. **The handover figures.** Create `~/scratch/exits-r12/` as Appendix A of the remediation plan describes: a crate
   with path dependencies on the three simulation crates of an exported tree, a copy of `Cargo.lock` and
   `rust-toolchain.toml`, and `tests/repro.rs` holding only Appendix A's helpers and `r12_throughput_report`. Do
   this for `ae075fb` and for `main`. `SimWorld::new(map, seed)` no longer exists on `main` (catalogs leave
   `sim_core` in `03d578d`). Port it the way `throughput.rs` builds its world, with `RuntimeCatalogs::load_from_directory`
   and `SimWorld::with_catalogs`, and record the port in the evidence. Run `cargo test --release --test repro --
   --nocapture --test-threads=1` three times each, interleaved with the benchmark, logging the load average.
   - If `r12` gives the benchmark's figures, the handover's figures were host-specific: say so, and that the
     benchmark stays the measurement of record.
   - If `r12` is several times faster than the benchmark, diff what they do (catalog, spawn, orders, ticks) and name
     the difference.
2. **The residual.** Bisect the 3 to 4 % at 400 units between `b263734` and `main` with the benchmark. Export each
   candidate (`git archive`), build in `~/scratch/cargo-target/exits-wp7/<sha>`, and run five interleaved rounds per
   candidate. Start with `866563f`, `90d9e25` and `99bb566`, then `git bisect` by hand over
   `git log b263734..main -- crates/sim_*/src`. A step under 2 % is noise at this load: say so and stop.
3. Write the evidence: tables, loads, commands and the conclusion. If one commit explains the residual, record it as
   a finding (**[R]**, Q-3 family) for a later plan, with its cost per tick at 400 units. No optimisation here.

**Gate.** Common gate (docs only, so C17 to C19 are what matter).

---

## 5. Measurements taken while planning

### 5.1 Throughput history

See §2.5. Logs: `~/scratch/logs/planexits_bench_compare.log`, `~/scratch/logs/planexits_bench_bisect.log`. They
are scratch files that may be cleaned at any time, so the tables in §2.5 are the record. The exported trees and
build directories were deleted after the runs.

### 5.2 Valgrind feasibility for WP3

Run 2026-11-03 16:32 to 16:40 UTC, valgrind 3.24.0 (Debian package, already installed). A temporary copy of
`crates/client/tests/pipeline_presentation_tests.rs` (deleted afterwards, never committed) read the frame count of
`measured_paused_presentation` from an environment variable. It was built in debug at `2be8992` and run as
`valgrind --tool=memcheck --leak-check=no --error-limit=no <binary> --exact
warmed_paused_path_preserves_real_world_and_exact_render_pose --test-threads=1`. The app uses Bevy's default
multi-threaded executor. Log: `~/scratch/logs/planexits_valgrind_probe.log`.

| Run | Measured paused frames | Total allocations | Wall time |
|---|---:|---:|---:|
| 1 | 64 | 21,887 | 115 s |
| 2 | 128 | 23,496 | 115 s |
| 3 | 64 | 21,887 | 117 s |
| 4 | 128 | 23,498 | 116 s |

Findings that shape WP3:

1. **It works and is affordable.** A child takes about 2 minutes under valgrind, almost all of it in setup (media
   decoding). Four children take 8 minutes; CI's runners are slower, so WP3 step 5 may need its own job.
2. **The count is reproducible.** It is identical at 64 frames, and differs by 2 at 128 frames. The multi-threaded
   executor is the likely source of the ±2, so the single-threaded executor of WP3's design is required, not
   optional.
3. **The paused frame is not allocation-free today.** It costs about 25 allocations per frame:
   (23,497 − 21,887) / 64 ≈ 25.2, about 4.9 KB. That is the whole frame of a `MinimalPlugins` app, so it includes
   Bevy's own per-frame work (time, events, the executor, asset event bookkeeping) as well as the live
   synchroniser. The probe cannot tell them apart. So WP3 measures a **baseline** too (below), and attributes the
   difference with DHAT before asserting anything.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| Valgrind counts differ between identical runs (threads, hash seeds, lazy statics) | Single-threaded executor; two runs per F; WP3 step 1 stops cleanly and the claim stays Review |
| Valgrind is too slow on CI | WP3 step 5: own job reusing the debug build; the paused-frame window keeps F small |
| The shared GPU or server load makes frame figures noisy | Interleaved runs, load recorded per run, medians, minimum of three unloaded runs, load stated when not achieved |
| Xvfb with software presentation inflates frame times | The 0-unit baseline is measured the same way; the evidence states the path; Q0015 option 3 |
| 200 units per side do not fit the spawn ring | WP5's no-GPU integration test finds it first; the start cells are data |
| The writer table lists systems whose access is wider than their real writes (`ResMut` taken but rarely written) | The table documents access, which is what Bevy enforces; a wide access is itself worth a reviewer's look |
| The smoke run at 400 units is too slow on `lavapipe` within the harness's child limit | Lower `--measure-frames`, never the unit count |
| A sync system really allocates or churns | It becomes a finding with its own commit (WP2 step 6, WP3 step 3); the exit waits for it |

---

## 7. Decisions for the owner

- [Q0015](../questions/open/0015-client-frame-time-target.md): the frame-time target. Default: measure and publish
  only.
- [Q0011](../questions/open/0011-allocation-counter-unsafe.md) stays open. This plan follows its default and adds an
  external count that needs no `unsafe` (D-2). The owner may still choose option 1 or 2, which would replace WP3's
  method, not its test's assertions.
- [Q0001](../questions/open/0001-media-scope.md) still blocks 8.5 and with it Phase 8's exit.
