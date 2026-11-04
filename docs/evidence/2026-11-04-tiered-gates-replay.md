# Tiered test gates WP4: the historical replay of the catalog

Work package 4 of [`docs/plans/2026-11-04-tiered-test-gates.md`](../plans/2026-11-04-tiered-test-gates.md) (§10,
results in §11.1). Written 2026-11-04 against `main` at `a151620`, with the catalog `crates/gate/suites.ron` as
merged in WP3.

## 1. Question

Would the `package` tier have selected every test that failed while the last 45 packages were being made? A test
that failed in a package's own runs but that `cargo gate package` would not have run for that package's change is
a catalog miss (plan §3.5), and WP4 adds the missing rule.

## 2. Method

1. **The merges.** The 45 most recent first-parent merges of `main` at `a151620`
   (`git log --first-parent --merges -n 45 a151620`), from `b4a223e` (2026-11-03 22:43) to `a151620` itself
   (2026-11-04 21:50). Phase 9 WP12 (`56f243f`, 2026-11-03 14:20) is older, but two of the seven red complete runs
   of plan §2.5 (`p9-wp12-gate1`, `p9-wp12-gate2`) belong to it, so it is replayed as an extra row.
2. **The selection.** For each merge `m`, `cargo gate explain m^1..m`: the `package` selection for exactly the
   change the package brought to `main`, with today's catalog and today's workspace targets. Nothing was built or
   run for the replay.
3. **The failures.** Every log under `~/scratch/logs/` (1,288 files) whose name starts with a package's log prefix
   (`anim_wp1_` for animation WP1, `combat_wp4_` for combat WP4, and so on: 28 prefixes) was read for Cargo's
   `error: test failed, to rerun pass -p <crate> --lib|--test <name>` lines; each names one failing target. These
   are all logged failures of the package, not only those of its complete runs: red-first tests and mutation
   checks in the package's own crates are included, which makes the replay stricter than plan §2.5. Logs of
   packages not merged at `a151620` (`combat_wp7_*`, `open_wp11_*`, `veo_act_*`) and of packages older than the
   window (`exits_*`, `p9-*` other than WP12, `gpu-harness-*`, `smoggold-*`, `cidisk-*`, `ci-*`) are not
   attributed.
4. **The check.** A failing target is selected if `explain` lists it. The scripts were throwaway, like the plan's
   §2 scripts; the table below is their complete output.

## 3. Result

- **45 merges replayed:** 26 select a subset (26 to 132 of the 156 test targets, median 60), 14 are docs only
  (fmt and the two `workspace_checks` targets), 5 select the complete suite (`Cargo.lock` twice,
  `.cargo/config.toml`, `crates/client/Cargo.toml`, `.github/workflows/ci.yml`: global paths, plan P3).
- **65 failing targets** in the logs of 27 packages (59 in the 45 merges, 6 in Phase 9 WP12).
- **All seven red complete runs of plan §2.5 are covered:** every one of their 15 failing targets is selected by
  the `package` tier of its package's merge (rows `1629c00`, `4bd48cf`, `be5a476`, `9d5884c`, `07cb246` and the
  extra row `56f243f`). §2.5 lists four `sim_core` binaries for `combat_wp4_probe_tests`; the log shows three
  `sim_core` binaries and `sim_fixtures/trace_tests`, all selected. It lists two failing binaries for
  `p9-wp12-gate1`; the log shows four (`client/runtime_battle_tests` and `sim_core/scenario_tests` as well), all
  selected.
- **One target not selected:** `client/battle_input_tests` in `loops_wp2_gate_test` (Veo loops WP2, `79f53f5`,
  which changed only `crates/asset_baker/`). It is **not a catalog miss but a flake**, and no rule was added:
  - The failing test, `a_box_finishes_on_release_over_the_inspector`, panicked inside Bevy
    (`bevy_core_pipeline-0.15.3/src/tonemapping/mod.rs:457`) while decompressing Bevy's own embedded tonemapping
    look-up table: `SuperDecompressionError("Failed to decompress Zstandard for mip 0: ... AccLogTooBig { got: 16,
    max: 8 }")`. No file of the change is read by that code: the table is compiled into `bevy_core_pipeline`.
  - The package's next complete run, `loops_wp2_gate_test2` (16 minutes later), passed the same binary, 35 of 35
    tests, and so did its final complete run, `loops_wp2_gate_test4`.
  - The error appears in no other log of the 1,288.

  Under the flake rule of plan §3.5 such a failure is rerun and recorded, not mapped: a rule from `asset_baker` to
  `client/battle_input_tests` would add a 50 to 120 s headless battle driver to every `asset_baker` package to
  catch a failure no `asset_baker` change can cause.
- **Rules added: none.** The catalog of WP3 passes the replay as merged.

## 4. Table

"Failing targets" lists every target that failed in any logged run of the package; "Red run of §2.5" names the
complete runs of the plan's §2.5 among them.

| # | Merge | Package | Package tier selects | Changed crates | Failing targets in the package's logs | Red run of §2.5 | Missed |
|---|---|---|---|---|---|---|---|
| 1 | `a151620` | tiered gates WP1-WP3: cargo gate (explain, quick, package, merge, full), suites catalog, check C22 | complete suite (156 targets): .cargo/config.toml matches global `.cargo/**` | gate | none | - | none |
| 2 | `d7b63b9` | animation WP7: frame budget A/B (no measurable cost), capture determinism | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 3 | `abb9d9f` | the tiered test gates plan | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 4 | `d5779f7` | animation WP6: Lab, swarm and portrait on the shared flipbook clock; duration_ms drives slice periods | 54 targets | client | `client/lab_scene_tests`, `client/lib`, `client/sync_steady_state_tests` | - | none |
| 5 | `1940231` | animation WP3: per-slice duration_ms in flipbook manifests | 73 targets | asset_baker, client | `asset_baker/bake_integrity_tests`, `asset_baker/manifest_tests` | - | none |
| 6 | `ec04bf4` | animation WP5: battle flipbooks play on the presented simulation clock | 55 targets | client | `client/battle_flipbook_tests` | - | none |
| 7 | `317dcc3` | combat WP5+WP6: zealot Biological (inert), marine SC2 timing; Q0036/Q0038/Q0040/Q0044 closed | 67 targets | sim_core | `sim_core/golden_replay_tests` | - | none |
| 8 | `1629c00` | open-findings WP9: smog quiescence (Q0020), goldens re-recorded, frame-budget A/B | 53 targets | client | `client/lib`, `client/smog_golden_tests`, `workspace_checks/charter` | `open_wp9_gate_2` | none |
| 9 | `4bd48cf` | animation WP4: GPU flipbook clock, 176-byte bubble uniform, integer frame selection, parity test | 54 targets | client | `client/flipbook_clock_gpu_tests`, `client/headless_tests`, `client/smog_golden_tests` | `anim_wp4_test1` | none |
| 10 | `be5a476` | combat WP4: zealot weapon to SC2 values; battle 33 re-recorded as a close fight (Q0036, Q0044) | 84 targets | sim_core, sim_fixtures | `sim_core/golden_replay_tests`, `sim_core/sc2_battle_benchmarks_tests`, `sim_core/sc2_who_wins_matchup_matrix_tests`, `sim_fixtures/trace_tests` | `combat_wp4_probe_tests` | none |
| 11 | `b35d793` | Veo loops WP4 pilot: lastFrame accepted inline, seam fixed; both clips fail on in-loop motion; weekly cap 60 (Q0047) | 27 targets | asset_baker | `asset_baker/ledger_tests`, `asset_baker/veo_cli_tests` | - | none |
| 12 | `9d5884c` | animation WP1: AttackSwing in the snapshot, shared resolved_weapon, no checksum change | 132 targets | client, sim_core, sim_types, workspace_checks | `client/smog_golden_tests`, `sim_core/attack_swing_snapshot_tests`, `workspace_checks/charter` | `anim_wp1_test2` | none |
| 13 | `71d80a0` | Veo loops WP3: loop-check, calibration; rescued 0dbd7948 not accepted (seam ghost) | 26 targets | asset_baker | `asset_baker/loop_check_tests` | - | none |
| 14 | `e935e0a` | open-findings WP8: smog brick sparsity, bit-exact (Q0048) | 53 targets | client | `client/lib`, `client/smog_golden_tests` | - | none |
| 15 | `79f53f5` | Veo loops WP2: seam repair in the bake (loop sampling, scale lock, cross-fade) | 26 targets | asset_baker | `asset_baker/framing_tests`, `asset_baker/loop_metrics_tests`, `asset_baker/provenance_tests`, `client/battle_input_tests` | - | `client/battle_input_tests` |
| 16 | `49177b4` | combat WP2: damage point 0 fires on the tick its swing starts; golden_replay_orders re-recorded | 80 targets | sim_core, sim_fixtures | `sim_core/golden_replay_tests`, `sim_core/weapon_cadence_tests`, `sim_fixtures/trace_tests` | - | none |
| 17 | `b7becda` | Veo loops WP1: loop policy in data, lastFrame pin, pin rules, ledger and provenance records | 82 targets | asset_baker | `asset_baker/veo_cli_tests`, `asset_baker/veo_reliability_tests` | - | none |
| 18 | `36943e1` | combat WP0: trace whole-matrix mode, shared expectation check, baseline evidence, Q0041 note | 80 targets | sim_core, sim_fixtures | `sim_fixtures/lib` | - | none |
| 19 | `35c663e` | Veo WP4 (stopped): plain backdrop and negativePrompt fix the rays; camera push-in blocks loops (Q0046) | 114 targets | asset_baker, client | none | - | none |
| 20 | `b2beb83` | combat WP1: backswing no longer delays the next attack; period > damage point check (Q0038) | 60 targets | sim_core | `sim_core/weapon_cadence_tests` | - | none |
| 21 | `47237a1` | the combat plan amendment for Q0044/Q0045 (turn rates, overclock move, Phoenix, full audit) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 22 | `16a6352` | Veo letterbox WP2: pilot clip installed, in-game check on RX 580, Q0043 closed, audio kept | 27 targets | asset_baker | none | - | none |
| 23 | `3474357` | Veo letterbox WP1: letterbox bake for structures, Veo request settings in data | 82 targets | asset_baker | `asset_baker/framing_tests` | - | none |
| 24 | `155aab7` | open-findings WP7: smog solver bit-exact refactors I (ping-pong slabs, truncating cast, dead slabs) | 53 targets | client | `client/lib`, `client/smog_golden_tests` | - | none |
| 25 | `b9f51ae` | combat WP3 research: catalog time and speed audit against SC2 5.0.14 (Q0045) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 26 | `0e5565a` | the combat SC2-alignment plan (Q0035-Q0041 answers; Q0044) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 27 | `d3d7749` | the Veo letterbox-bake plan (Q0043 option a, generateAudio false in data) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 28 | `0cf5c1f` | Veo pilot: first generation (plasma_siphon_dome idle, $0.40), held for square crop (Q0043) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 29 | `35ced86` | open-findings WP6: C-18 baseline goldens, smog solver benchmark, perf attribution | complete suite (156 targets): crates/client/Cargo.toml matches global `crates/*/Cargo.toml` | client | `client/smog_golden_tests` | - | none |
| 30 | `77985b5` | Veo media WP3 (stopped at preflight 404; batch-1 prompts, Q0042) | 63 targets | - | none | - | none |
| 31 | `07cb246` | Veo media WP2: provenance records, required set, real strict audit; pulse_bot re-baked | 62 targets | asset_baker, client | `asset_baker/provenance_tests`, `client/lab_scene_tests` | `veo_wp2_ws1` | none |
| 32 | `62d9f83` | open-findings WP5: golden replay pins the vision/acquisition and upkeep/arrival orders | 60 targets | sim_core | `sim_core/attack_move_simulation_tests`, `sim_core/collision_and_immobility_tests`, `sim_core/golden_replay_tests`, `sim_core/high_ground_combat_tests`, `sim_core/movement_contact_tests`, `sim_core/spells_and_abilities_tests` | - | none |
| 33 | `2c6c165` | zealot/marine investigation WP5: conclusion and owner questions Q0035-Q0041 | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 34 | `db22ac2` | zealot/marine investigation WP4: experiments | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 35 | `51946c5` | open-findings WP4: terrain chunks mesh the last dual column and row | 53 targets | client | `client/terrain_tests` | - | none |
| 36 | `c68d5e8` | zealot/marine investigation WP3: battle trace tool and 68-seed runs | complete suite (156 targets): Cargo.lock matches global `Cargo.lock` | sim_core, sim_fixtures | `sim_fixtures/lib`, `sim_fixtures/trace_tests` | - | none |
| 37 | `38132d0` | Veo media WP1: pipeline readiness (published states, ledger, budget, preflight, review, install) | complete suite (156 targets): Cargo.lock matches global `Cargo.lock` | asset_baker | `asset_baker/veo_cli_tests`, `workspace_checks/charter` | - | none |
| 38 | `70c9f03` | the CI runner clean-up step (frees unused preinstalled software before the build) | complete suite (156 targets): .github/workflows/ci.yml matches global `.github/workflows/**` | - | none | - | none |
| 39 | `52cc954` | the plan for the CI disk guard trip on small runners (Q0034) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 40 | `f81b5a0` | open-findings WP10: findings audit, 54 rows marked fixed with evidence | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 41 | `63dcc15` | open-findings WP3: pooled combat-effect materials (C-17) | 54 targets | client, client_diagnostics | `client/combat_fx_slot_gpu_tests`, `client/lib`, `client/sync_steady_state_tests` | - | none |
| 42 | `2296113` | zealot/marine investigation WP2: comparison and contact model | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 43 | `2210b31` | open-findings WP2: MapDocument::encode returns a typed error | 121 targets | client, client_diagnostics, sim_map | `client/gpu_validation_tests`, `sim_map/map_format_tests`, `sim_map/validation_golden_tests` | - | none |
| 44 | `eef92c3` | zealot/marine investigation WP1: SC2 reference data with sources | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| 45 | `b4a223e` | plans: Phase 8.5 Veo media, zealot/marine investigation, libm smog math (Q0024-Q0026) | docs only: fmt, `workspace_checks` (2 targets) | - | none | - | none |
| extra | `56f243f` | Phase 9 WP12 (red runs p9-wp12-gate1/2; outside the 45) | 150 targets | asset_baker, client, sim_core, sim_fixtures, sim_types, workspace_checks | `asset_baker/runtime_catalog_tests`, `client/round2_visual_tests`, `client/runtime_battle_tests`, `sim_core/faction_catalog_tests`, `sim_core/scenario_tests`, `workspace_checks/charter` | `p9-wp12-gate1`, `p9-wp12-gate2` | none |
