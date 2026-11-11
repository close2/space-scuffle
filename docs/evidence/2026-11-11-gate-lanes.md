# Evidence: faster gates (smog contract, shared paths, lane A/B)

Plan: [`docs/plans/2026-11-11-faster-gates.md`](../plans/2026-11-11-faster-gates.md). This file records WP4's
proof and audit, WP5a's shared-path audit, and WP6's A/B measurement as each lands.

## 1. WP4: the smog system goldens hash what the smog reads

### 1.1 Fields the smog code reads or writes

Re-derived from the code (`crates/client/src/smog/mod.rs` unless noted). It is the plan's list of eight fields,
exactly:

| Field | Where |
|---|---|
| `anim_state` | `setup_smog_volume_system`, through `bubble_shader.rs::still_frame_lanes` and `frame_index_at` |
| `heading_angle_rad` | read in `sync_bouncing_smog_bubbles_system` (scratch harvest) |
| `world_pos`, `velocity`, `heading_angle_rad` | written in `sync_bouncing_smog_bubbles_system` |
| `shape`, `ground_z` | `live_battle_volume` and `collect_smog_sources`, from `update_smog_field_system` |
| `velocity`, `radius`, `entity_id` (team colour of a bubble outside the showcase) | `collect_smog_sources` |

`BubbleUniformGpu::from_instance` (`bubble_shader.rs`) reads `shape`, `world_pos`, `radius`,
`heading_angle_rad`, `aim_angle_rad`, `is_attacking`, `anim_state`, `hp_ratio`, `shield_ratio`,
`shield_hit_flash`, `hull_hit_angle_rad`, `hull_hit_flash`, `build_or_queue_progress`, `selected`,
`concealment.is_concealed()` and `speed_boosted`. It reads none of `status`, `stance`, `owner` and
`barrier_ratio`, so all four stay in the "never read" test, with `silhouette`, `unit_type`, `weapon_visual`,
`attack_target_pos` and `attack_swing`.

A correction to plan §5: the system goldens hash only the bubbles that carry `BouncingSmogBubble`, the ten showcase
bubbles, whose instances come from the client's `showcase_render_instance`. The test's own `battle_unit`
instances reach the hash only through the fluid grid. The conclusion stands.

### 1.2 Re-recorded goldens (glibc 2.41, x86_64)

| Golden | Old | New |
|---|---|---|
| startup | `0xac23_14d4_13dd_e3ed` | `0x4459_0814_2009_2b71` |
| Visual Lab | `0x5150_d25b_ae37_bbfc` | `0x33ee_40dd_7e58_d19e` |
| swarm | `0xb9ec_a555_c65c_d6cf` | `0xc4b0_ef45_a4e7_36ab` |
| live battle | `0xb986_235a_dab0_bdd5` | `0xdac9_628c_19f9_46f1` |

### 1.3 Proof that no smog output changed

The output of the comparison runs is kept in `~/scratch/gate-ab/2026-11-11-wp4-smog-proof.out`.

- (a) On the same tree, with the `Debug` term switched back on, all 19 tests passed on the old constants.
- (b) With the new hashing, every non-system golden passed unchanged (7 fluid, 4 kinematics, noise, the `libm`
  canary); only the four system goldens moved, to the values above.
- (c) A temporary dump of every hashed bubble's transform (f32 bits) and `BubbleUniformGpu` bytes was taken three
  ways: HEAD's test file, the new file, and the new file with the `Debug` term. All three were byte-identical:
  startup 2,160 bytes, Visual Lab 8,640, swarm and live battle 6,480 each (the showcase bubbles are hidden and not
  stepped in those two tabs, so their dumps agree).

Test first: with a temporary `Debug`-based body, `a_bubble_field_the_smog_never_reads_does_not_move_the_system_hash`
failed (11199385904259833432 against 5326919497195356506); with the real body both new tests pass.

### 1.4 Audit of client tests that compare a hash or a recorded golden

| Test | Compares | Depends on | Contract |
|---|---|---|---|
| `smog_golden_tests`: fluid, kinematics, noise, canary | FNV of the client smog solver, kinematics and noise | client code and `libm` | none needed |
| `smog_golden_tests`: four system goldens | transforms, uniform bytes, visibility, grid, `hash_smog_inputs` | client code; `AnimState` order and `Concealment::is_concealed` through `from_instance`; `sim_map` map size; `sim_core` catalog order through `BattleDefinition::load` | none added: R-9 of the plan (`full` catches it); a new field or variant fails to compile under workspace clippy |
| `mesher_golden_tests` | FNV of the terrain mesh and picks | client mesher on `sim_map` grids; `asset_baker` terrain themes | in `downstream["sim_map"]`; terrain textures under the "media" rule |
| `effect_slice_gpu_tests` | PNG goldens with a tolerance, per driver | client effects; `sim_core` catalog and `sim_types` splash specs | GPU class: the "GPU renderer" and "catalog" rules; a sim change to splash publication is caught by `full` |
| `runtime_battle_tests`, `pipeline_presentation_tests`, `player_presentation_tests`, `headless_tests` | `compute_checksum` equal or unequal between runs (no recorded value) | `sim_core` determinism | already in the `sim_map`, `sim_types` and `sim_core` contracts |
| `#[cfg(test)]` modules of `crates/client/src` | relational only (sparse equals dense); no recorded hash constant | client code | none needed |

No contract was missing, so `suites.ron` is unchanged by WP4.
