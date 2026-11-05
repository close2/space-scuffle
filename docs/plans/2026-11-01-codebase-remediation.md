# Codebase Remediation Plan

**Status:** Proposed, awaiting owner decisions in §3. No code has been changed.
**Date:** 2026-11-01. **Audited:** working tree on top of `443052c`, including uncommitted changes.
**Line numbers** refer to that working tree and will drift. Search for the quoted symbol if they do.

---

## 1. Verdict

The code *looks* rigorous and is often well built at the small scale, but much of the game does not work.
Fixed-point math, the checksum, tick pipeline, uniform layouts and module docs are good. Above that layer,
basic RTS behaviour is broken or missing, and the verification that should have caught this mostly cannot.

| Gate | Result | What it really shows |
|---|---|---|
| `cargo test --workspace` | 788 passed, 0 failed, 0 ignored | Coexists with every bug in §4. Run 2026-11-01 |
| `clippy -D warnings` | Pass | Cast, length and dead-code lints are disabled crate-wide in `sim_core`, `sim_map`, `client` |
| `cargo fmt --check` | Pass | Genuine |
| GitHub CI | 3 runs, **all failed** | Last run 2026-10-26; 17 commits are unpushed; no protection exists |
| Cross-platform determinism | Claimed by the charter | Never run on anything but x86-64 Linux |
| 10,000+ ticks/s | Claimed by the charter | Measured below: **64 to 969 ticks/s** |

### Why 788 tests missed this

1. **Tests inject state instead of playing.** `sc2_who_wins_matchup_matrix_tests.rs` has 66 direct
   `order_queue` pokes and 0 `PlayerCommand`s.
2. **Tests skip silently.** Five video tests use a path that does not exist from their working directory,
   take a `return`, and report `ok`. No video is baked in `cargo test`.
3. **Some assertions cannot fail.** `collision_and_immobility_tests.rs:308,565` compares a dot product of
   magnitude ~0.06 against -0.3, so the anti-vibration tests pass however hard units vibrate.
4. **Some tests lock in the bug.** `sc2_battle_benchmarks_tests.rs:94` relies on a Hold Position unit dying
   passively.
5. **Fixes were verified against the plan's scenario, not the feature.** No test plays a whole game loop,
   so nothing notices that the player cannot give one unit an order.

### Effort is going to the wrong place

About 20,500 lines (42%) of `crates/client/src` are diagnostics, capture and reduction code that players never
run, compared with about 16,100 lines of runtime. Rounds 8 to 11 went to mist sub-pixel evidence while
`mist/` is switched off at runtime and combat, movement and input have the defects below.

---

## 2. Evidence levels

Every finding carries one of these, so nobody has to guess how far to trust it.

| Tag | Meaning |
|---|---|
| **[R]** | **Reproduced.** A real test failed, or a real measurement was taken, on 2026-11-01. See Appendix A. |
| **[V]** | **Verified.** The orchestrator read the source and confirmed it. Not executed. |
| **[A]** | **Audited.** Traced line by line by a review agent, not independently re-checked. Reproduce first. |

**Rule:** before fixing any [V] or [A] item, write the failing test. If it passes, the finding is wrong:
record that here and move on.

### Not reproduced or corrected

- **"Crowds beyond ~60 units never finish arriving": not reproduced.** 60 and 150 `pulse_bot`s on a flat
  64×64 map all completed within 6,000 ticks. The `1.5` vs planned `2.5` mismatch at `steering.rs:516` is
  real but is not shown to cause a failure. Do not change it without a failing case (ramps and chokes are
  untested).
- **"Hold Position units never fire"** applies to non-turret units. Turret units read `current_target`.
- **F-01** in the 2026-10-26 review is fixed for LiveBattle. The flat sandbox survives only for headless
  checks and tests.
- **Reviewed and clean:** no float reaches authoritative state; no `HashMap`, clock or thread use in sim
  crates; checksum covers persistent entity state; IDs do not alias; weapon period sums exactly; tick clock
  and interpolation; uniform layouts match WGSL; no secrets are tracked by git.

---

## 3. Decisions needed from the owner

These change the scope of the phases below. Each has a recommendation, which the plan assumes.

| # | Decision | Recommendation | Why |
|---|---|---|---|
| D1 | What is the near-term product? | **A playable skirmish**: select, command, fight, win or lose | Today it is a scripted battle viewer. This sets the scope of Phase 5 |
| D2 | Which fog system survives? | **Keep `smog/`, delete runtime `mist/`** | `mist/` is permanently disabled (C-4). Smog still needs C-2 and C-7 fixed |
| D3 | Keep the ~20,500 lines of diagnostics? | **Keep what still protects the game, in a separate crate. Delete reducer research** | The reducer was rejected (3,278 of 16,296 windows failed) and is not used at runtime |
| D4 | How to settle the uncommitted work? | **Commit as-is in coherent units, then fix forward** | It passes the suite, and a partial commit breaks the build (`lib.rs` references untracked modules) |
| D5 | Economy and tech: build or cut? | **Build a minimal version** (P5.5 to P5.7), or remove the orders | `Harvest`, `ReturnCargo`, research and construction do not function, which breaks charter §1.1 |
| D6 | Are procedural flipbooks allowed? | **Test fixtures only, never at runtime paths** | §1.2 forbids stand-ins, §4 and §5 require them |
| D7 | Required media scope? | **The units in shipped loadouts, then expand** | Only 4 of 91 catalog identities have real media; 81 have none and 6 are copies of one image. **Decided** by the owner in [Q0001](../questions/closed/0001-media-scope.md) (option B, paid Veo); carried out by step 8.5: the required set of 10 identities (`assets/media/required_set.ron`) is verified, 12 of 91 identities have real media, none is a copy (2026-11-05, `473b977`) |
| D8 | Charter §6 model rule | **Make it tool-neutral** | It names a Gemini model and is an uncommitted edit. This audit's agents ran on Claude, so it could not be followed literally. Approved by the owner (Q0003); §6 names no model |

---

## 4. Findings

Severity is impact on someone using the game or relying on the charter.

### 4.1 Combat and rules (`sim_core`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| S-1 | High | **[R]** | Hold Position units never fire: `_ => false`, and the weapon step reads targets only from `AttackTarget`. **Fixed** in `56437da` (3.3): holders acquire in-range targets without pursuit; test `combat_correctness_tests.rs::s1_hold_position_fires_at_enemy_in_range` | `world.rs:1960`, `:2507` |
| S-2 | High | **[R]** | Explicit focus fire is replaced by the nearest enemy within 3 ticks. **Fixed** in `56437da` (3.4): explicit and acquired targets are marked, acquired ones with hysteresis; player commands always explicit in `3052128`; test `combat_correctness_tests.rs::s2_explicit_focus_fire_survives_nearby_enemy` | `world.rs:1930`, `:2075` |
| S-3 | High | **[R]** | Anti-air-only structures shoot ground units: `weapon = ground.or(air)`. **Fixed** in `f3d6ba2` (2.8); test `weapon_catalog_rules_tests.rs::s3_air_only_structure_does_not_shoot_ground_units` | `catalog.rs:970`, `:177` |
| S-4 | High | **[V]** | Projectile weapons ignore `attacks_count`: one hit vs the instant path's loop, so `attacks_count: 2` projectile units are likely at half damage. **Fixed** in `7a0e01c` (3.2); test `combat_correctness_tests.rs::s4_projectile_weapon_applies_every_strike` | `world.rs:2970` vs `:2677` |
| S-5 | High | [A] | Friendly-fire splash makes allies attack their own unit: no owner check found. **Fixed** in `56437da` (3.5); test `combat_correctness_tests.rs::s5_friendly_splash_does_not_provoke_allies` | `world.rs:1738`, `:1856` |
| S-6 | High | [A] | Windup fires at the current target, ignoring `Windup.target_id`, with no range recheck. **Fixed** in `56437da` (3.6); tests `combat_correctness_tests.rs::s6_windup_fires_at_its_bound_target`, `::s6_windup_rechecks_range_at_release` | `combat.rs:454`, `world.rs:2568` |
| S-7 | High | [A] | A sieged unit locks onto a target inside min range and stops firing. **Fixed** in `56437da` (3.6); test `combat_correctness_tests.rs::s7_sieged_unit_retargets_inside_minimum_range` | `world.rs:1949`, `combat.rs:355` |
| S-8 | High | **[V]** | `Build` is free, instant and unchecked, and its result is discarded. **Fixed** in `3a2865e` (5.4): transactional admission, payment on arrival, construction lifecycle | `world.rs:1139` |
| S-9 | High | **[V]** | **There is no income.** The only write to minerals is a subtraction; `Harvest` and `ReturnCargo` have no handler. **Fixed** in `3a2865e` (5.5); income through real input in `7a432f9` | `production/mod.rs:542` |
| S-10 | High | **[V]** | `research_upgrade` has no non-test caller. [A] Cost and time unread; not idempotent. **Fixed**: repeats rejected in `3052128` (3.9), research order and queue in `3a2865e` (5.6), card entry in `5482e95` | `world.rs:491` |
| S-11 | High | [A] | `Unlock*` upgrades, `sight_delta` and several overrides are never read. **Fixed** in `3052128` (3.9): `sight_delta` feeds vision, unlocks gate abilities and overclock, and an effect nothing applies is a load error; tests `combat_correctness_tests.rs::s11_sight_delta_extends_vision`, `weapon_catalog_rules_tests.rs::s11_upgrade_effects_must_resolve` | `world.rs:824` |
| S-12 | Med | [A] | Target dying in Windup burns a full cooldown. **Fixed** in `56437da` (3.6); test `combat_correctness_tests.rs::s12_target_death_in_windup_burns_no_cooldown` | `combat.rs:458` |
| S-13 | Med | [A] | Ground splash hits air; stat bonus ignores falloff. **Fixed** in `7a0e01c` (3.2); tests `combat_correctness_tests.rs::s13_ground_splash_does_not_reach_air`, `damage_and_shields_tests.rs::s13_splash_falloff_scales_stat_bonus_damage` | `world.rs:2705`, `combat.rs:191` |
| S-14 | Med | [A] | `bonus_upgrade_per_level` only applies vs ARMORED. **Fixed** in `7a0e01c` (3.2); test `damage_and_shields_tests.rs::s14_bonus_upgrade_follows_the_weapons_attribute_bonus` | `combat.rs:67` |
| S-15 | Med | **[V]** | `firing_arc_deg: 360` wraps to arc 0, the strictest possible; 16 catalog entries use it. Per-shot cost not measured. **Fixed** in `f3d6ba2` (2.8): an arc of 180° or more is unrestricted; test `weapon_catalog_rules_tests.rs::s15_authored_360_degree_arc_is_unrestricted` | `catalog.rs:732`, `math.rs:797`, `combat.rs:322` |
| S-16 | Med | [A] | Catalog validation allows crashes: `projectile_speed: 0` divides by zero on every peer. **Fixed** in `f3d6ba2` (2.3); test `weapon_catalog_rules_tests.rs::s16_projectile_speed_must_be_positive` | `validation.rs:213`, `world.rs:2618` |
| S-17 | Med | [A] | Attack path ignores cloak and invulnerability (latent: no shipped cloak). **Fixed** in `56437da` (3.7); test `combat_correctness_tests.rs::s17_attack_path_respects_availability` | `world.rs:2477` |
| S-18 | Med | [A] | Mode-transition copies have diverged; instant vs channel behave differently. **Fixed** in `3052128` (3.8): one `apply_mode_transition`; test `combat_correctness_tests.rs::s18_instant_and_channelled_transitions_agree` | `world.rs:1006`, `:1250` |
| S-19 | Med | [A] | Shielded structures never regenerate; `is_gas_extractor` ignored. **Fixed** in `3052128` (3.8): authored structure regeneration, and `is_gas_extractor` reaches the capabilities, consumed by the economy in `3a2865e` (5.5); tests `combat_correctness_tests.rs::s19_shielded_structures_regenerate`, `weapon_catalog_rules_tests.rs::s19_gas_extractor_flag_reaches_capabilities` | `catalog.rs:1014` |
| S-20 | Med | [V] | `world.rs` is 3,205 lines; `step_weapons_and_projectiles` is ~630, with 3 splash copies that already disagree. **Fixed**: combat moved out in Phase 3 (3.1); the rest split in `8ac33ad` (9.1), leaving `world.rs` a 373-line module root | `world.rs:2458` |

### 4.2 Movement (`steering.rs`, `movement/`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| M-1 | Crit | **[R]** | Friendly units walk through each other: head-on centres reach 0.03 against contact distance 0.75. `goal_dir·velocity` is compared as if it were a cosine. **Fixed** in `d4109e2` (4.1); test `movement_contact_tests.rs::head_on_friendlies_keep_contact_and_pass` (repro r03) | `steering.rs:379`, `:174`; `collision.rs:43` |
| M-2 | Crit | **[R]** | Short orders on a stationary group are dropped on tick 1: 0.000 progress. Idle squadmates count as "settled". **Fixed** in `d4109e2` (4.2): arrival is recorded per destination, not inferred from velocity; test `movement_contact_tests.rs::short_order_on_a_stationary_pair_moves_them` (repro r04) | `locomotion.rs:387`, `steering.rs:509` |
| M-3 | High | **[R]** | Unreachable orders never end: 20/20 still ordered after 500 ticks at 15.67 ms/tick. The `find_path` error is dropped and A* reruns. **Fixed** in `d4109e2` (4.3), on the nearest-reachable-point search of `b10e9ca`; tests `movement_contact_tests.rs::unreachable_order_reports_retargets_and_terminates`, `reachability_tests.rs::nearest_point_is_shared_across_a_connected_region` | `locomotion.rs:349`, `:421` |
| M-4 | High | **[R]** | Enemies shove Hold Position units: 0.324 displacement. Hostile branch runs before the immobile check. **Fixed** in `d4109e2` (4.4); test `movement_contact_tests.rs::enemy_cannot_shove_a_holding_unit` (repro r07) | `steering.rs:612`, `world.rs:2336` |
| M-5 | High | [A] | Corner waypoints cannot pop when a blocker sits on them: handoff 0.28 vs contact 0.75. **Fixed** in `d4109e2` (4.5). The finding as written was not reproduced (the old code escaped by rebuilding its route every tick); that rebuild is gone, and waypoints now pop by passing; test `movement_contact_tests.rs::route_continues_past_a_blocker_on_its_corner_waypoint` | `locomotion.rs:361` |
| M-6 | High | [A] | Stuck units are invisible: velocity stays nonzero with zero displacement, no stuck detection. **Fixed** in `d4109e2` (4.5): velocity holds achieved displacement, with no-progress detection; test `movement_contact_tests.rs::stuck_unit_reports_no_motion_and_abandons_its_order` | `locomotion.rs:593` |
| M-7 | High | [A] | Pursuit has no pathing: melee slides along cliffs instead of taking the ramp. Planned, never implemented. **Fixed** in `d4109e2` (4.6); test `movement_contact_tests.rs::melee_pursuit_takes_the_ramp_and_reaches_the_enemy` | `locomotion.rs:562` |
| M-8 | High | [A] | Clearance is corner-anchored and asymmetric, so large units fail near +x/+y walls. Compounds M-3. **Fixed** in `1ed9888` (2.4); tests `clearance_field_tests.rs::m8_clearance_is_invariant_under_map_rotation`, `pathfinding_properties.rs::clearance_has_no_directional_bias` | `pathfinding.rs:52`, `:479`, `:679` |
| M-9 | High | [A] | Ramp-corner slide trap is back: X is always tried before Y. **Fixed** in `d4109e2` (4.5): one slide helper tries the dominant axis first; test `movement_contact_tests.rs::pursuit_rounds_a_single_blocked_cell_promptly` | `locomotion.rs:596`, `world.rs:2437` |
| M-10 | High | [A] | Route-retention fix is dead code in the live game: `vision_aware_pathfinding` is always true. **Fixed** in `d4109e2` (4.7): the first leg of a route asks only whether the unit can walk it, so retention runs; test `movement_contact_tests.rs::route_survives_cells_narrower_than_the_tier` | `locomotion.rs:316`, `world.rs:241` |
| M-11 | High | [A] | Steering is O(N²) despite a documented spatial hash. **Fixed** in `d4109e2` (4.8): neighbours come from a tick-start grid; test `movement/grid.rs::query_returns_every_body_in_reach_and_skips_far_ones`; cost measured by the release benchmark (Q-3) | `steering.rs:265`, `:352`, `:496` |
| M-12 | High | [A] | Smooth steering excludes final approach and pursuit, with a discontinuity at exactly 4 m. **Fixed** in `d4109e2` (4.7): one continuous steering function, the 4 m switch deleted. No test pins the absence of the seam; the movement suites cover it only indirectly | `locomotion.rs:440` |
| M-13 | Med | [A] | Buffed units lose speed on any friendly contact; cap uses base speed. **Fixed** in `d4109e2` (4.9); test `movement_contact_tests.rs::buffed_unit_keeps_its_speed_through_friendly_contact` | `world.rs:2408` |
| M-14 | Med | [A] | Wedge still in `compute_push_displacement`; low-priority movers shove high-priority idlers. **Fixed** in `d4109e2` (4.4); tests `movement_contact_tests.rs::low_priority_mover_does_not_shove_a_high_priority_idler`, `::moving_pair_separates_radially` | `steering.rs:699` |
| M-15 | Med | [A] | Unrelated idle friendlies and buildings end Move orders early. **Fixed** in `d4109e2` (4.2); tests `movement_contact_tests.rs::unrelated_idle_friendly_on_the_line_does_not_end_a_move`, `::building_beside_the_goal_does_not_end_a_move` | `locomotion.rs:399` |
| M-16 | Med | [A] | Exact head-on tie-break steers both units to the same world side. **Fixed** in `d4109e2` (4.1); test `movement_contact_tests.rs::exact_head_on_tie_separates_to_opposite_world_sides` | `steering.rs:296`, `:391` |
| M-17 | Med | [A] | Collision and steering ignore cliff separation. **Fixed** in `d4109e2` (4.9); test `movement_contact_tests.rs::units_on_either_side_of_a_cliff_do_not_push_each_other` | `world.rs:2236` |
| M-18 | Low | [A] | `from_f32_lossy` in tick loops, contrary to its docs; magic numbers throughout. **Fixed** in `d4109e2` (4.9): tuning lives in `movement/tuning.rs` as exact rationals, with no float in tick loops; checked by C7 (`workspace_checks` `charter.rs::c07_floats_in_simulation_crates_only_at_listed_boundaries`, `35ba670`) | `steering.rs:176` onward |
| M-19 | Med | [V] | A marching group jostles in a crowd: on `showcase_canyon` with loadout `battle_automaton` (8 `pulse_bot`, 2 `rhino_beetle`, seed 1592639215, attack-move to (52.5, 52.5)), the marines (entities 4 to 10) reverse heading by 8° to 175° on consecutive ticks near (28 to 30, 10 to 12): 40 velocity zig-zags between ticks 216 and 287 at speed 0.09, 20 between ticks 152 and 312 at 0.126 (combat alignment WP7); one `rhino_beetle` once (tick 179). Found when WP7 made the marines reach the crowd inside the 180-tick window of `collision_and_immobility_tests.rs::battlefield_bubble_trajectories_are_smooth_without_micro_jitter`; it is not caused by mixed speeds (the all-0.09 group shows it too, later). That test now judges the open march before the crowd (`JITTER_MARCH_DISTANCE`); the reproduction (window 36.0 world units) is in `docs/evidence/2026-11-04-combat-sc2-alignment.md` §6.4. Open, for a later movement plan | `movement/`, `steering.rs` |
| M-20 | Low | [V] | The Immortal's Barrier is not modelled: in StarCraft II 5.0.14 an immortal that takes damage autocasts a buff that absorbs up to 100 damage for 3 Normal s, cooldown 45 (`void` BehaviorData:97 to 101, `voidmulti` BehaviorData:278, `voidmulti` AbilData:585 to 591). `void_goliath` has no such capability, so with the tank's StarCraft II period (combat alignment WP12, group W1) battle `3_immortal_vs_tank` ends with the immortal at 176 life instead of 200, and its expectation was re-recorded to 176 with the claim naming the gap (`docs/evidence/2026-11-04-combat-sc2-alignment.md` §13.13). Open, for a later capability plan (an absorb buff in `UnitArchetype`, data-driven) | `catalog.rs`, `world/abilities.rs` |
| M-21 | Low | [V] | Melee units waiting out their cooldown at a contact ring are jostled once per weapon cycle: in `collision_and_immobility_tests.rs::test_converging_units_respect_speed_cap_and_do_not_vibrate` (eight `welder_drone` around a held one, seed `0x00C0_FFEE`), a unit that is no longer anchored by its swing lurches inward about 0.08 and steps back about 0.015 on the next tick. On the catalog before combat alignment WP12 group W2 each unit did this twice in 120 ticks (entity 6 at ticks 15 and 24; the test allowed 2); with the SCV's StarCraft II period (19 → 27 ticks, a longer unanchored cooldown) entity 6 does it a third time at tick 40. The test now judges vibration up to each unit's first strike, where the crowd converges; the jostle after it is this finding. Open, for a later movement plan (with M-19) | `movement/`, `steering.rs` |
| M-22 | Low | [V] | A ready weapon that is not aligned starts its swing one tick late: `step_ready_weapon` (`combat.rs`) turns the mount and enters `RotatingToTarget` without checking the arc again, while `step_rotating_weapon` turns and checks in the same call. So a unit whose turn this tick aligns it still waits a tick, on every new target outside its arc. With the zealot and zergling arcs of 180° (every direction) this never happened; with StarCraft II's 11.25° (combat alignment WP17 commit d, audit §10.3) melee fights last 1 to 4 ticks longer (`docs/evidence/2026-11-04-combat-sc2-alignment.md` §14.4). The game turns and attacks in one motion. Open, for a later weapon-state plan (check alignment after the slew in `step_ready_weapon`, with a whole-matrix measurement) | `combat.rs` |

### 4.3 Foundations (`sim_types`, `sim_map`)

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| F-1 | High | **[R]** | Debug and release disagree: `length()` of (32768, 32768) panics in debug, returns 0 in release. Peers on different profiles diverge. **Fixed** in `71c4df0` (2.1, 2.2): saturating operators, exact `i128` length; tests `overflow_policy_tests.rs::r10_length_of_large_vector_is_exact`, `::operators_saturate_at_both_bounds`; debug and release replays agree in the CI determinism jobs (Q-2) | `math.rs:358`, `:583` |
| F-2 | High | **[V]** | Watchtower capture is never called, so towers never grant vision. [A] Ownership never releases. **Fixed** in `dae34ba` (2.6); tests `watchtower_tick_tests.rs::f2_unit_walking_onto_a_tower_captures_it_and_gains_vision`, `::f2_tower_is_released_when_its_occupant_dies` | `grid.rs:689`, `:709` |
| F-3 | Med | **[R]** | `normalize()` is wrong for small vectors: length 1.4142. **Fixed** in `71c4df0` (2.2); test `overflow_policy_tests.rs::r11_normalize_small_vector_is_unit` | `math.rs:576`, `:605` |
| F-4 | Med | **[V]** | Ramp visibility branch is dead: `cliff_level > s` and `== s` cannot both hold. **Fixed** in `dae34ba` (2.6): ramp vision by height; test `ramp_vision_tests.rs::f4_observer_near_the_top_of_a_ramp_sees_the_plateau` | `vision.rs:248` |
| F-5 | Med | [A] | `validate_map`, the gate for untrusted files, converts before bounds-checking and can overflow. **Fixed** in `1c79d85` (2.3); test `map_gate_tests.rs::f5_huge_resource_cell_reports_out_of_bounds` | `validation.rs:622`, `:704`, `:951` |
| F-6 | Med | [A] | 160 of tenths 1..=200 fail round-trip, e.g. radius 12.3 is rejected. **Fixed** in `1c79d85` (2.7); test `map_gate_tests.rs::f6_every_authorable_tenth_round_trips` | `map_format.rs:746`, `:851` |
| F-7 | Med | [A] | Small units are routed through zero-width diagonal gaps, so diagonal wall-offs do not work. **Fixed** in `1ed9888` (2.5); test `clearance_field_tests.rs::f7_no_tier_passes_between_structures_touching_at_a_corner` | `pathfinding.rs:590` |
| F-8 | Med | [A] | Legacy Tiled importer and `ScMapData` bypass all validation, with no production caller. **Fixed** in `91bcb27` (2.7): the importer, `ScMapData` and `sim_map`'s `serde_json` are deleted; no test, as the code is gone | `loader.rs` |
| F-9 | Med | [A] | New ramp fallback is roughly cubic and swallows errors. **Fixed** in `06edc46` (2.5); test `ramp_smoothing_tests.rs::f9_real_map_routes_stay_legal_across_ramps` | `pathfinding.rs:738` |
| F-10 | Med | [A] | Destroying a structure fabricates pathability. **Fixed** in `1c79d85` (2.5); test `map_gate_tests.rs::f10_removing_a_structure_restores_the_cell_flags_it_found` | `grid.rs:679` |
| F-11 | Med | [A] | Ramp landing validation ignores pathability. **Fixed** in `1c79d85` (2.5); test `map_gate_tests.rs::f11_upper_landing_must_be_ground_pathable` | `validation.rs:522`, `:560` |
| F-12 | Med | [A] | Sight radius silently capped at 16. **Fixed** in `dae34ba` (2.6): oversized sight is rejected at load, not capped; tests `ramp_vision_tests.rs::f12_sight_beyond_the_old_cap_reaches_its_authored_distance`, `weapon_catalog_rules_tests.rs::f12_catalog_sight_beyond_vision_is_rejected_not_shortened` | `vision.rs:21` |
| F-13 | Med | [A] | Faction names, armour classes and abilities in Rust enums, against charter §3.3. **Fixed** in `56f243f` (9.3): factions, weapon and armour classes are data in `assets/catalog/factions.ron`, checked by C5; ability kinds stay engine mechanics (Phase 9 decision D-7) | `enums.rs:8`, `upgrades.rs:47` |

### 4.4 Client runtime

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| C-1 | Crit | **[V]** | Selection lasts one frame: sync overwrites it from a snapshot where `selected` is always false, so rings and HP bars never show in battle. **Fixed** in `822c1b6` (5.1) | `scene_controller.rs:564`, `world.rs:3197` |
| C-2 | Crit | [A] | Smog destroys and recreates two 3D textures every frame, ~1.66 MB; the code comment claims the opposite. **Fixed** in `52abebb` (6.1): persistent smog textures written in place with `Queue::write_texture`; 116 image and 58 material `Modified` events over 59 steps went to 0 and 0 | `smog/mod.rs:900`, `:169` |
| C-3 | High | **[V]** | No unit-level commands: no right-click outside the editor, `pick_terrain` unused, only the whole roster can be ordered. **Fixed** in `822c1b6` (5.2), context orders in `6bc1fab` | `lab_ui.rs:738` |
| C-4 | High | **[V]** | `mist/` is permanently off: smog `enabled` defaults true, so 14,677 lines do nothing at runtime while still allocating [A]. **Fixed** in `4c9fda6` and `0f31027` (6.2): the depth prepass is attached at camera setup and the runtime `mist/` is deleted (D2) | `mist/mod.rs:455` |
| C-5 | High | [A] | Every unit's material is re-prepared every frame. **Fixed** in `9f419bb` (6.1): sync compares before `Assets::get_mut`; a paused frame modifies 0 of 26 bubble materials | `scene_controller.rs:553` |
| C-6 | High | [A] | Smog startup is a chain of silent fallbacks against charter §1.2, with fabricated unit IDs. **Fixed** in `ba9a5c1` (6.3): smog failures are typed and reach the media failure path | `smog/mod.rs:397`–`:454` |
| C-7 | High | [A] | Smog draws over units by a single box-centre sort key, likely a remaining cause of smog in front of units. Needs a capture. **Fixed** in `c37943a` (6.3): smog now never draws in front of a unit, a deliberate trade, proved by a GPU luminance test on the 200-bubble swarm | `smog/material.rs:108` |
| C-8 | Med | [A] | Visual Lab default state hides its own subject; several controls do nothing. **Fixed** in `7117874` and `93afd13` (6.5) | `lab_ui.rs:167`, `:437` |
| C-9 | Med | [A] | Combat VFX buttons spawn ~22 units off-screen. **Fixed** in `7117874` (6.5) | `lab_ui.rs:509` |
| C-10 | Med | [A] | 50 of 200 swarm bubbles spawn inside the arena plateau. **Fixed** in `7117874` (6.5) | `scene_controller.rs:790` |
| C-11 | Med | [A] | Construction progress never reaches the shader; dome fill direction is also wrong. **Fixed**: progress published in `3a2865e`, fill by height in `d90e76e` (5.8); pixels on the live skirmish in `7a432f9` | `world.rs:3196`, `bubble_shader.rs:332` |
| C-12 | Med | [A] | Selection hits hidden entities and shared materials. **Fixed** in `822c1b6` (5.1); fogged enemies unpickable in `24c656f` | `selection.rs:153` |
| C-13 | Med | [A] | Smog: unordered systems, hardcoded 64×64 clamps, 16-cavity cap, velocity scale 20 vs 25 Hz tick. **Fixed** in `12296f2` (6.3): ordered systems, bounds from the map, velocity scale from the tick rate; the 16-cavity cap is kept on purpose | `smog/mod.rs:160`, `:686`, `:849`, `:805` |
| C-14 | Med | [A] | Camera snaps on cliffs; projectile cards go edge-on; melee rotated twice. **Fixed** in `ab5ef61` (6.5) | `camera.rs:233`, `vfx.rs:318`, `:611` |
| C-15 | Med | [A] | Per-frame allocation and asset churn; `LabState` owns the authoritative world. **Fixed** in `257824d`, `52620f6` and `00eff7c` (6.6). Proved by test: asset churn, entities and retained buffers in `6cc3ec6` (`sync_steady_state_tests.rs`); allocations per steady frame counted under valgrind DHAT in `77a9db6` (`sync_allocation_count_tests.rs`); one owner per concern in `88ca528` (`system_ownership_tests.rs`) | `scene_controller.rs:106`, `lab_ui.rs:98` |
| C-16 | Med | [A] | Parallel implementations: `arena_render` vs `terrain/`, `bubble_shader` vs `bubble_optics`, and dead markers. **Fixed** in `8392b40`, `07f7014` and `64fd330` (6.6); the writer table of `88ca528` keeps one writer per concern | `lib.rs:27` |
| C-17 | Med | **[V]** | Every combat effect creates its own `CombatFxMaterial` asset: one per tracer, projectile, beam and slash. In a 400-unit fight that is hundreds of material creations and removals a second, each with its own uniform buffer and bind group. It is event-driven, so outside the steady-state claim of Phase 6, but it is the most likely asset cost at 400 units. Found while planning the Phase 6 exit (`docs/plans/2026-11-03-close-phase-6-7-8-exits.md` §2.4); WP6 of that plan measures it. **Fixed** in `690be35` (merged `63dcc15`; open-findings WP3): pooled material slots, the per-effect data in a `CombatFxInstance` component written to the slot's buffer in the render world; test `sync_steady_state_tests.rs::combat_frames_reuse_effect_materials` (a 240-frame combat window spawns 173 effects and adds no material). In heavy combat frames on the RX 580 the frame interval's median falls from 20.4 to 16.8 ms and its p95 from 27.0 to 20.2 ms; evidence `docs/evidence/2026-11-03-c17-combat-fx-pool.md` | `vfx.rs` `hitscan_tracer`, `projectile`, `beam`, `melee_slash` |
| C-18 | Med | **[R]** | The smog solver costs about 8.2 ms of CPU on every frame, whatever the unit count: 19.7 % of all cycles in a 400-unit `perf` run and most of the 0-unit baseline's 10.8 ms main-world update. It is the largest single client cost. Found by WP6 of `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`; evidence `docs/evidence/2026-11-03-frame-budget.md`. **Partly fixed** by open-findings WP6 to WP9: baseline goldens and benchmark `fd7048c` (merged `35ced86`), bit-exact refactors `93a89a3` (merged `155aab7`) and brick sparsity `7fdbb54`, `ab12883` (merged `e935e0a`), then quiescence `3f3ab76` (merged `1629c00`; Q0020). The idle and after-fight costs are fixed: solver per frame at 0 units 5.0 → 0.3 ms, and after a fight the field comes to rest. What remains: a 400-unit fight still costs about 6.1 ms of solver per frame (7.8 ms at the base), above D-1's 2.0 ms trigger, so options F (lower fixed smog rate) and G (worker thread) are reopened for a later plan. Frame interval at 400 units on the RX 580: p50 16.75 → 12.03 ms, p95 36.30 → 20.92 ms. Evidence `docs/evidence/2026-11-04-c18-smog-solver.md` §7 and §8 | `client::smog::fluid::SmogGrid3d::step` |

### 4.5 Map editor

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| E-1 | Crit | **[V]** | F5 Playtest panics and loses unsaved work: editor registers none of `BattleSession`, `PlayerPalette`, `PresentationStatus`; Bevy 0.15 defaults to panic. Confirmed in source, not run. [A] Even with them, the playtest world feeds no snapshot. **Fixed** in `9875f48` and `bce31af` (7.1) | `editor/app.rs:227` |
| E-2 | High | **[V]** | Window X bypasses the unsaved-changes guard: `close_when_requested` is never set. **Fixed** in `1e03482` (7.2) | `bin/map_editor.rs:87` |
| E-3 | High | [A] | Releasing a stroke over the UI loses its undo entry. **Fixed** in `270217a` (7.3) | `editor/ui.rs:1459` |
| E-4 | High | [A] | Symmetric painting does not update the mirrored terrain. **Fixed** in `956682c` (7.4) | `editor/ui.rs:1480`, `tools.rs:130` |
| E-5 | High | [A] | Opening a different-size map leaves old chunks and garbled textures; `Err` swallowed. **Fixed** in `956682c` (7.4); the document-load order is pinned by test in `88ca528` | `terrain/mod.rs:73`, `editor/ui.rs:1130` |
| E-6 | Med | [A] | Discard clears `dirty` too early; Save never resumes the pending action. **Fixed** in `1e03482` (7.2) | `editor/ui.rs:1393` |
| E-7 | Med | **[V]** | Ctrl+Shift+Z undoes: the Ctrl+Z branch runs first, so redo is unreachable. **Fixed** in `270217a` (7.3) | `editor/ui.rs:391` |
| E-8 | Med | [A] | Other logic faults: start-location symmetry, silent playtest errors, Eraser creates invalid cells, per-keystroke undo entries, `doc` marked changed every frame. **Fixed** in `96eab79` (7.5) | `tools.rs:729`, `ui.rs:1253`, `:1589`, `:318` |
| E-9 | Med | [A] | Render verification can pass on stale PNGs from an earlier run. **Fixed** in `ca953f5` and `69dd84e` (7.5) | `verification.rs:152` |
| E-10 | Low | [A] | Plan gaps: drag ramps, entity moving, playtest HUD unreachable; panics on bad CLI input; 919-line `editor_ui_system`. **Fixed** in `b9c65f3` and `1318d30` (7.5) | `ui.rs:277`, `app.rs:117` |

### 4.6 Asset baker and media

| ID | Sev | Evidence | Defect | Where |
|---|---|---|---|---|
| B-1 | High | **[V]** | Six catalog units are byte-identical copies of `concept_video.png`, and players see them. **Fixed** in step 8.5: the three off the playable path removed in `7929844` (Veo WP2), the three on it (`welder_drone`, `capacitor_dome`, `assembly_bay_dome`) replaced by their own Veo atlases in `70ad1d6` (Veo batch 2, merged `473b977`); `media_tests.rs::no_shipped_texture_is_a_copy_of_the_concept_atlas` keeps it fixed | `assets/textures/` |
| B-2 | High | [V] | API key is in curl's argv and `-L` can forward it to redirect hosts. [A] Derived `Debug` prints it. **Fixed** in `b342c77` (8.1) | `veo_client.rs:501` |
| B-3 | High | [A] | HTTP error bodies are saved as `.mp4` and cached forever: no `--fail`, no temp file, size-only cache check. **Fixed** in `4c425c8` (8.2) | `veo_client.rs:501`, `:492` |
| B-4 | High | [A] | Veo polling can hang and loses paid operations on transient errors. **Fixed** in `4c425c8` (8.2) | `veo_client.rs:390` |
| B-5 | Med | [A] | `DEFAULT_PROJECT_ID = "000000000000"` is silently substituted; `/home/cl/...` paths are hardcoded, including key lookup. **Fixed** in `d3f70d5` (8.3) | `veo_client.rs:42`, `:778` |
| B-6 | Med | [A] | Default bake silently keeps ~0.67 s of each 4 s clip. **Fixed** in `cf784cd` (8.4) | `ffmpeg_importer.rs:313` |
| B-7 | Med | [A] | Importer fabricates missing actions, defeating `require_action`. **Fixed** in `cf784cd` (8.4) | `ffmpeg_importer.rs:194`, `:427` |
| B-8 | Med | [A] | `procedural` can overwrite real art via fuzzy name matching. **Fixed** in `cf784cd` (8.4) | `procedural_generator.rs:61` |
| B-9 | Med | [A] | Veo accepts unknown units and unsanitised path components. **Fixed** in `d3f70d5` (8.3) | `veo_client.rs:641`, `:672` |

### 4.7 Engineering system

| ID | Sev | Evidence | Defect |
|---|---|---|---|
| Q-1 | Crit | **[V]** | CI has never passed, and 17 commits are unpushed. [A] Last failure may be missing `libxkbcommon-x11` (suspected). **Fixed**: X11 runtime library and `--locked` in `c44a317` (0.3); first green run on `main` is 41568002256 (`6a3aa26`, 2026-11-03, all five jobs), and `main` is pushed as work merges. CI is not a required check (Q0005) |
| Q-2 | Crit | [A] | Determinism across platforms is unverified: no matrix, no wasm target, no golden `SimWorld` checksum. **Fixed** in `98af39a` (1.3): golden replay checksums on Linux x86_64, Windows x86_64, macOS ARM64 (debug and release) and `wasm32-wasip1` under wasmtime; test `golden_replay_tests.rs::golden_replay_reproduces_every_checkpoint`; first all-green CI run 41568002256 |
| Q-3 | High | **[R]** | Performance claim is false. See Appendix A; nothing in the repo measures it. **Fixed**: release benchmark of Appendix A's scenarios in `412aefa` (1.4); per-tick map and sight costs cut in `ae075fb`; the charter claim replaced by a real-time floor, checked by the CI `throughput` job, in `4f3ee7d` and `0336642` (9.5); figures in `DESIGN.md` §5 |
| Q-4 | High | **[V]** | Clippy is hollow: crate-wide allows remove the lints this engine most needs. **Fixed**: `sim_core` and `sim_map` in `e469b8b` (1.8), `client` in `64fd330` (6.6). `asset_baker` keeps one crate-wide `clippy::doc_markdown` allow |
| Q-5 | High | **[V]** | Video tests skip silently: `crates/asset_baker/tmp` does not exist. Other silent `return`s [A]. **Fixed**: a committed fixture clip in `1b6faff` (1.1), loaded by `asset_baker/tests/common/mod.rs::fixture_clip`; the other silent passes removed, and `SPACE_SCUFFLE_SKIP_GPU_TESTS` rejected on CI, in `8329538` (1.1) |
| Q-6 | High | [A] | No property testing, despite charter §4's heading. **Fixed**: `proptest` added in `412aefa` (1.5); properties in `71c4df0` (`sim_types/tests/math_properties.rs`: arithmetic, angle wrap) and `1ed9888` (`pathfinding_properties.rs::funnel_paths_are_legal_and_no_shorter_than_a_straight_line`) |
| Q-7 | High | [A] | Vulkan validation errors do not fail tests. **Fixed** in `8329538` (1.7): GPU tests fail on an unlisted validation ID or a missing layer; CI installs the layers in `37011d3`; tests `gpu_validation_tests.rs::error_finding_with_validation_id_is_an_unlisted_violation`, `::live_battle_passes_vulkan_validation` |
| Q-8 | High | [A] | Rust policy is false: `rust-version` 1.80 vs locked `fixed` needing 1.93; floating `stable` toolchain; `fixed` on a caret range. **Fixed** in `c44a317` (0.4): `rust-toolchain.toml` pins 1.99.0, `rust-version = "1.99.0"`, `fixed = "=1.31.0"`; no test, CI builds with the pinned toolchain and `--locked` |
| Q-9 | High | [A] | `DESIGN.md` is stale: 22.4/30 Hz vs 25, NavMesh + RVO vs grid A*, nonexistent crates, absolute `file:///` links. **Fixed** in `0336642` (9.5): rewritten with a per-subsystem status table, checked by C17 |
| Q-10 | Med | [A] | Shader "syntax" tests are `.contains()`; 66 plans with colliding numbers and no status index; 545 `/tmp` references, so evidence is ephemeral. **Fixed**: shader tests in Phase 1 (`bb406f9`); plan index, unique IDs and retention lines in `10d92c5` (9.6), checked by C18 and C19 |
| Q-11 | Med | **[V]** | `tools/renderer_compatibility/target` (1.3 GB) is unignored, a `git add -A` hazard; no `[profile]` section. **Fixed**: unrooted `target/` in `.gitignore` in `ecce29f` (0.1), which ignores `tools/renderer_compatibility/target`; `[profile]` sections in `c44a317` (0.5); no test |
| Q-12 | Med | [A] | `asset_baker → sim_core` is not in charter §5; `sim_map` needs `serde_json` only for dead code. **Fixed**: `serde_json` left `sim_map` before Phase 9; the edge is drawn and §5 is checked against Cargo by C1 in `0336642` (9.5) |

---

## 5. Phases

Order matters. Phases 0 and 1 make later claims trustworthy. Phases 2 to 4 are ordered so that movement is
not tuned on broken math and clearance. Phases 7 and 8 can run in parallel with anything after Phase 1.
Size: S under a day, M a few days, L a week or more.

**Rules for every phase**

- Failing test first, through `PlayerCommand` where the behaviour is player-visible.
- Each commit fixes one finding and names its ID.
- Simulation changes alter checksums. Update a golden only in a commit that explains why it changed.
- A phase is done when its exit criteria pass on **CI**, not locally.
- Update this file's finding table: mark items fixed, or refuted with the reason.

### Phase 0: Establish a real baseline (M)

| Step | Action |
|---|---|
| 0.1 | Fix `.gitignore` to unrooted `target/` (Q-11) |
| 0.2 | Settle the tree per D4, in order: sim changes, smog, editor, docs. Run the gate before each commit |
| 0.3 | Fix CI: add the X11 runtime library, use `--locked`, get one green run, then push |
| 0.4 | Pin the exact Rust version and `fixed = "=1.31.0"`; set `rust-version` truthfully (Q-8) |
| 0.5 | Add profiles: `[profile.dev.package."*"] opt-level = 2`, `[profile.test] debug = "line-tables-only"` |
| 0.6 | Resolve D1 to D8 and record the outcomes here |

**Exit:** green CI on `main`; clean working tree. The third criterion, CI required on the default branch, was
dropped by the owner (Q0005): CI is not a required check. Agents look at CI regularly and fix it when it is red.

### Phase 1: Make verification truthful (L)

| Step | Finding | Action |
|---|---|---|
| 1.1 | Q-5 | Commit a tiny fixture clip. Replace every silent `return` with a failure or an explicit, CI-rejected opt-out |
| 1.2 | M-tests | Fix the impossible vibration assertions by normalising both vectors. Add the missing assertions: squad reaches target, melee reaches enemy, `world.diagnostics()` empty |
| 1.3 | Q-2 | Add a replay fixture with per-N-tick golden checksums. Run on Linux, Windows, macOS ARM64, and `wasm32-wasip1` via wasmtime |
| 1.4 | Q-3 | Add a release benchmark for Appendix A's scenarios. Publish the numbers; correct the charter claim until it is met |
| 1.5 | Q-6 | Add `proptest` to `sim_types` and `sim_map`: arithmetic, angle wrap, funnel invariants |
| 1.6 | Q-10 | Replace `.contains()` shader tests with naga parse and validate |
| 1.7 | Q-7 | Fail on unlisted Vulkan validation IDs, with a documented allowlist |
| 1.8 | Q-4 | Remove crate-wide allows from `sim_core` and `sim_map`; replace with `#[expect(..., reason = "...")]` per site. Add `clippy::unreachable`. Replace casts with checked conversions. `client` follows in Phase 6 |
| 1.9 | new | Add one end-to-end test that plays a battle through `PlayerCommand` only, to game end. **Done**: `full_match_tests.rs` in `9111d8e`; through input events alone, `skirmish_match_tests.rs` in `7a432f9` |

**Exit:** the suite cannot be green with skipped real work; determinism runs on all four targets, or the
charter is corrected. Any platform divergence becomes a top-priority finding.

### Phase 2: Simulation foundations (M)

| Step | Finding | Action |
|---|---|---|
| 2.1 | F-1 | Give `SimScalar` one explicit, profile-independent overflow policy. Recommended: saturating operators, plus `checked_*` for callers that can act on failure. Check cost with the 1.4 benchmark |
| 2.2 | F-1, F-3 | Compute length, distance and normalise on exact `i128` raw squares. Remove the `Err → ZERO` fallback |
| 2.3 | F-5, S-16 | Validate bounds before converting. Complete catalog bundle validation at load |
| 2.4 | M-8 | Replace corner-anchored clearance with a centre-symmetric distance field based on real radius. Do this before Phase 4 |
| 2.5 | F-7, F-9–F-11 | Block zero-width diagonals; rewrite the ramp fallback with precomputed corridor indices; preserve base cell flags; validate landing pathability |
| 2.6 | F-2, F-4, F-12 | Wire watchtowers into the tick with release on vacancy; implement ramp vision by progress; reject oversized sight radii at load |
| 2.7 | F-6, F-8 | Fix tenths round-trip by rounding; delete `loader.rs` importer, `ScMapData` and `serde_json` unless needed |
| 2.8 | S-3, S-15 | Fix `weapon_against` fallback; treat arc ≥ 180° as unrestricted |

**Exit:** F-1, F-3 and S-3 repros pass; property tests pass; no silent fallback in `SimVec2`.

### Phase 3: Combat correctness (L)

Start by extracting one `apply_weapon_hit` and one splash path from `step_weapons_and_projectiles`
(S-20), with no behaviour change. The next fixes then land in one place.

| Step | Finding | Action |
|---|---|---|
| 3.1 | S-20 | Extract `combat_resolution` with one damage and splash path. Characterise current behaviour by test first |
| 3.2 | S-4, S-13, S-14 | Apply `attacks_count` to projectiles; filter splash by layer; scale bonuses correctly |
| 3.3 | S-1 | Hold Position acquires in-range targets without pursuit. Update the test that relies on passive holders |
| 3.4 | S-2 | Add an explicit vs auto marker to `AttackTarget`, with hysteresis for auto targets |
| 3.5 | S-5 | Do not retaliate against or alert on friendly damage |
| 3.6 | S-6, S-7, S-12 | Bind shots to `Windup.target_id` with a range recheck; retarget inside min range; no cooldown on a cancelled swing |
| 3.7 | S-17 | One availability gate at acquire, fire, hit and splash |
| 3.8 | S-18, S-19 | One `apply_mode_transition`; authored structure regen |
| 3.9 | S-10, S-11 | Reject repeated upgrades. Read or reject at load every unread upgrade field. Research wiring is in Phase 5 |

**Exit:** S-1 to S-3 repros pass; matchup matrix moved to data, run through `PlayerCommand`, on several seeds,
then re-baselined with each change explained.

### Phase 4: Movement (L)

Treat this as a redesign of the arrival and contact rules, not another tuning round. Earlier rounds patched
the same area repeatedly, and several fixes regressed or never ran (M-9, M-10).

| Step | Finding | Action |
|---|---|---|
| 4.1 | M-1, M-16 | Compare normalised velocities in co-movement; let contact cancel inward velocity; use a symmetric tie-break |
| 4.2 | M-2, M-15 | Replace velocity-inferred arrival with an explicit group or order identity and arrival state. Exclude buildings |
| 4.3 | M-3 | Propagate path errors as typed outcomes; stop or retarget to the nearest reachable cell; add a repath cooldown |
| 4.4 | M-4, M-14 | Check `cannot_be_pushed` first; delete the wedge; move only the pushable side |
| 4.5 | M-5, M-6, M-9 | Pop waypoints by passing, not distance; store achieved velocity; add no-progress detection; try the dominant axis first via one shared slide helper |
| 4.6 | M-7 | Route pursuit over terrain, with throttled repaths |
| 4.7 | M-10, M-12 | One continuous steering function, no mode split at 4 m; make the retention fix reachable |
| 4.8 | M-11 | Query a tick-start spatial grid; add reusable scratch buffers |
| 4.9 | M-13, M-17, M-18 | Use effective speed for caps; require terrain connectivity for contact; named constants or data-driven tuning |

**Exit:** M-1 to M-4 repros pass; new tests for head-on, crossing, chokes, ramps, blocked waypoints and
short orders; 400-unit benchmark at least 2× Appendix A.

### Phase 5: Playable game loop (L, depends on D1 and D5)

| Step | Finding | Action | Status |
|---|---|---|---|
| 5.1 | C-1, C-12 | Keep selection client-side, applied in sync; respect visibility; handle misses | Done, `822c1b6` |
| 5.2 | C-3 | Right-click move and attack through `BattleCommandQueue`, owner-filtered | Done, `822c1b6`; context orders `6bc1fab` |
| 5.3 | new | Box select, control groups, Hold and Stop bindings | Done, `822c1b6` |
| 5.4 | S-8 | Transactional `Build` admission with cost, tech, builder and typed outcome, plus a construction lifecycle | Done, `3a2865e`; knowledge-based admission `866563f`; card and placement `5482e95` |
| 5.5 | S-9 | Harvester state machine and income; retype `Harvest.target` | Done, `3a2865e`; right click on a node `6bc1fab` |
| 5.6 | S-10 | Research order and queue, reusing the production transaction | Done, `3a2865e`; card `5482e95`; stim admission by unlock `ced793b` |
| 5.7 | new | Production cancel and refund; load and unload orders; win and loss condition | Done, `3a2865e`; queue panel `5482e95`; result screen `760532a` |
| 5.8 | C-11 | Publish construction progress; fix dome fill | Done, `3a2865e`, `d90e76e`, `f093116` |

**Exit:** a person can play a match to a result using only mouse and keyboard, and 1.9 covers it.

**Exit status (2026-11-03): met; CI green on `main` at `7c947ad` (run 41583874524, all five jobs).** `skirmish_match_tests.rs` (`7a432f9`) wins the shipped
skirmish headless through input events alone, with a pilot that reads only the screen, and proves the HUD, the
placement and the dome fill on a real window under Vulkan validation. It runs as `cargo run -p client -- --skirmish`.
Gas, research and transport are built and tested on temporary fixture roots, and disabled in the shipped game for want
of media, per `docs/questions/closed/0001-media-scope.md`; they are enabled in the shipped game since Veo batch 1
(`fb1be5b`, merged `2c5ed5e`, tests `battle_card_path_tests.rs`). Plan and evidence:
`docs/plans/2026-11-02-phase-5-playable-client.md`, `docs/evidence/2026-11-03-result-window-victory.md`.

### Phase 6: Client performance and structure (L)

| Step | Finding | Action | Status |
|---|---|---|---|
| 6.1 | C-2, C-5 | Persistent smog textures with `queue.write_texture`; dirty checks before `get_mut`. Measure before and after | Done, `52abebb`, `9f419bb`; before and after counts in the commit messages |
| 6.2 | C-4, D2 | Remove the losing fog system; move the depth prepass to camera setup | Done, `4c9fda6`, `0f31027` |
| 6.3 | C-6, C-7, C-13 | Typed smog failures; correct compositing with a capture to prove it; map-derived bounds | Done, `ba9a5c1`, `c37943a`, `12296f2`; compositing proved on the swarm scene, not yet in a live battle |
| 6.4 | D3 | Create `client_diagnostics`; move capture and audit there; delete rejected reducer code; remove `ControlledFixture` branches from production paths | Done, `5654fda`, `bd909dc`, `0f31027` |
| 6.5 | C-8 to C-10, C-14 | Fix Lab defaults and dead controls, off-screen VFX, swarm placement, camera damping, card orientation | Done, `7117874`, `93afd13`, `ab5ef61` |
| 6.6 | C-15, C-16 | Move the world from `LabState` to `BattleSession`; remove arena, dead markers and aliases; then drop `client` crate-wide allows | Done, `257824d`, `52620f6`, `00eff7c`, `8392b40`, `07f7014`, `64fd330`; proved by test in `6cc3ec6`, `77a9db6`, `88ca528` |

**Exit:** no per-frame heap allocation or asset churn in sync systems, proved by test; one owner per concern;
frame budget measured at 200 and 400 units.

**Exit status (2026-11-03): met; CI green on `main` at `c918aa9` (run 41711531706, all six jobs).** No asset churn
and no per-frame allocation in sync systems: `crates/client/tests/sync_steady_state_tests.rs` (`6cc3ec6`) runs the
real client App over steady frames of the live battle, the skirmish HUD and the swarm. It proves that no asset of
any type the client registers is created, modified or removed, apart from the C-5 rule for units that turn, that
no entity is spawned or despawned, and that every retained buffer of the sync systems keeps its address and
capacity. `crates/client/tests/sync_allocation_count_tests.rs` (`77a9db6`) counts heap allocations from outside the
process under valgrind's DHAT, with no `unsafe` code (Q0011's default). It finds none per frame in the live battle
synchroniser (`advance_battle_ticks`, `sync_live_battle_system`, `publish_combat_events`) on paused frames, beyond a
baseline app without those systems, and a planted allocation is counted exactly. The count covers that
synchroniser; the other sync systems are covered by the asset-churn and retained-buffer proof. One owner per concern:
`crates/client/tests/system_ownership_tests.rs` (`88ca528`) compares every system that can write a declared concern
with a table of its owners, and `input_ownership_is_sampled_after_the_inspector_draws` with
`editor_app_tests.rs::document_load_applies_after_the_editor_ui_requests_it` pin the two orderings that make no
schedule cycle when reversed. Frame budget: `frame_budget` (`crates/client_diagnostics`, `b925db3`, `af1023d`)
measured the real client on the RX 580 through a headless KWin compositor, with hardware present, 1280×720, vsync
off and no monitor (Q0019's default). Median of five runs, frame interval p50, p95 and p99: 15.6, 19.0 and 20.6 ms at
200 units, 16.3, 20.9 and 23.6 ms at 400 units (12.3, 15.2 and 16.6 ms with no units). The figures are published
against the 16.7 and 33.3 ms reference lines, with no target, as Q0015's default says; Q0015 stays open. Evidence:
`docs/evidence/2026-11-03-frame-budget.md`. Left for later plans, recorded there: the smog solver costs about 8.2 ms
of every frame at any unit count (C-18, new); C-17's per-effect materials are measured, about 0.44 ms per frame on
average on the main threads plus longer frame intervals in heavy combat frames, and stay open; and the 400-unit
screenshot shows no smog, so C-7 is still proved on the swarm scene only. Plan:
`docs/plans/2026-11-03-close-phase-6-7-8-exits.md`.

**Later (2026-11-04):** C-17 fixed and C-18 partly fixed by `docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md`
(see their rows in §4.4).

**Later (2026-11-05):** the smog goldens no longer depend on the host's C math library: the smog code computes every
exponential, sine, cosine, arctangent and power through the `libm` crate (`client::smog::math`), checked by C23
(owner question Q0014, option b; plan `docs/plans/2026-11-04-libm-smog-math.md`). Measured cost: about 12 ms more at
smog startup and about 0.01 ms per frame; evidence `docs/evidence/2026-11-05-libm-smog-math.md`.

### Phase 7: Editor hardening (M, parallel)

| Step | Finding | Action | Status |
|---|---|---|---|
| 7.1 | E-1 | Add an App-level test that enters Playtest. Register resources and feed the session from the editor world. Autosave before entry | Done, `9875f48`; selection and orders end to end `bce31af` |
| 7.2 | E-2, E-6 | Handle window close in every phase; correct `dirty` handling; resume pending actions after save | Done, `1e03482` |
| 7.3 | E-3, E-7 | Finish strokes outside the pointer guard; block undo while painting; fix redo order | Done, `270217a` |
| 7.4 | E-4, E-5 | Sync the rects actually touched; one `load_document` that rebuilds chunks and surfaces errors | Done, `956682c`; load order pinned by test in `88ca528` |
| 7.5 | E-8 to E-10 | Remaining logic faults; verification on fresh output; split `editor_ui_system` | Done, `96eab79` (E-8), `ca953f5` and `69dd84e` (E-9), `b9c65f3` and `1318d30` (E-10) |

**Exit:** edit, save, playtest, return and reopen is covered by an automated App test.

**Exit status (2026-11-03): met; CI green on `main` at `6b39cd2` (run 41665216936).**
`crates/client/tests/editor_app_tests.rs::edit_save_playtest_return_and_reopen_round_trip` (added in `b9c65f3`,
merged in `7decbd7`) boots the real `MapEditorPlugin` on a fixture map and drives it with input events. It paints a
stroke, saves with Ctrl+S, enters playtest with F5 and checks that bubbles are drawn, returns with Escape with the
document and its history intact, reopens the file through the real load path (`LoadDocumentRequest::Open`) and
playtests again. It needs no GPU (`backends: None`), so it runs in CI's `cargo test --workspace`. Limits, which do
not affect the criterion: the save uses the shortcut, not the egui button; the reopen sends the event the file
browser sends, not a click; editor egui clicks and text fields are not exercised. The order of
`apply_document_load_system` after `editor_ui_system`, unpinned when the exit was met, is pinned since `88ca528`
(`document_load_applies_after_the_editor_ui_requests_it`). Plan: `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`
§2.2.

### Phase 8: Asset pipeline and media (M, parallel)

| Step | Finding | Action | Status |
|---|---|---|---|
| 8.1 | B-2 | Pass the key through `curl -K -` or a native client; allowlist hosts; follow redirects without the key; redact `Debug` | Done, `b342c77`; tests `veo_transport_tests.rs` (`b2_*`) |
| 8.2 | B-3, B-4 | `--fail-with-body`, temp file, validate with ffprobe, rename; timeouts, bounded retries, persisted operation names | Done, `4c425c8`; tests `veo_reliability_tests.rs` (`b3_*`, `b4_*`) |
| 8.3 | B-5, B-9 | Typed error for a missing project; remove `/home/cl` paths; exact catalog slugs only | Done, `d3f70d5`; tests `veo_tests.rs` (`b5_*`, `b9_*`) and `veo_reliability_tests.rs` (`b9_*`) |
| 8.4 | B-6, B-7, B-8 | Resample the whole clip or fail; never fabricate actions; procedural output outside runtime paths, per D6 | Done, `cf784cd`; tests `bake_integrity_tests.rs` (`b6_*`, `b7_*`, `b8_*`) |
| 8.5 | B-1, D7 | Remove the six copied atlases; generate real media for the required set; make `audit-media --strict` pass for that set | Done, plan [`2026-11-04-phase-8-5-veo-media.md`](2026-11-04-phase-8-5-veo-media.md): WP1 `2f302d4`, WP2 `960a25f`, `e9b963a`, `7929844`, pilot `89d20ec`, batch 1 `69f0ff0` to `fb1be5b` (merged `2c5ed5e`), batch 2 `9f132d4`, `e2573de`, `70ad1d6`, `c3c3e10` (merged `473b977`); amended by the letterbox, seamless-loops and structure-action-text plans; tests `media_tests.rs` (`real_catalog_media_audit_is_ordered_and_strict_passes_only_with_the_ledger`, `no_shipped_texture_is_a_copy_of_the_concept_atlas`) |

Generating media spends real money and needs the owner's approval per run.

**Exit:** strict audit passes for the agreed set; a failed download cannot enter the cache.

**Exit status (2026-11-05): met on `main` at `473b977`; CI not yet run on it.** Both halves of the exit hold.

- **Strict audit for the agreed set.** The set is `assets/media/required_set.ron`: the ten identities a player
  fields or builds on the skirmish's mineral, gas, research and transport paths ([Q0001](../questions/closed/0001-media-scope.md),
  option B). `asset_baker audit-media --strict --ledger docs/evidence/veo/ledger.jsonl` exits 0 and reports
  "Required set …: 10 of 10 verified", with no identity a `KnownConceptDuplicate` (output in
  [`2026-11-05-batch-2.md`](../evidence/veo/2026-11-05-batch-2.md), "Strict audit"). The test
  `real_catalog_media_audit_is_ordered_and_strict_passes_only_with_the_ledger` in
  `crates/asset_baker/tests/media_tests.rs` runs the same audit on the shipped files in `cargo test`, and
  `no_shipped_texture_is_a_copy_of_the_concept_atlas` keeps B-1 fixed.
- **A failed download cannot enter the cache.** Unchanged since 2026-11-03: the six `b3_*` tests of
  `crates/asset_baker/tests/veo_reliability_tests.rs` assert that nothing is cached, green on CI at `6b39cd2`
  (run 41665216936).

Gate: `cargo gate package` stamp `c37a8885923d7a119f5ed06e21ecee4793981151.package.ron` (Veo batch 2 at `cb2672c`,
114 targets, GPU tests and the golden replays in release and on `wasm32-wasip1` included, all green) and the merge
stamp of `473b977`. `main` has not been pushed since `ab2042c`, so no CI run covers the batch yet; the next push of
`main` runs it, and its run id belongs here. Spend for the whole step: 41 Veo generations, $16.40 at list price
(Veo 3.1 Fast, 720p, with sound), inside the owner's 200 EUR (Q0024). Evidence:
[`2026-11-04-pilot.md`](../evidence/veo/2026-11-04-pilot.md),
[`2026-11-04-batch-1.md`](../evidence/veo/2026-11-04-batch-1.md),
[`2026-11-05-batch-2.md`](../evidence/veo/2026-11-05-batch-2.md), ledger
[`ledger.jsonl`](../evidence/veo/ledger.jsonl). The expansion to the rest of the Automaton faction (Veo plan WP6)
is not part of this exit. Plans: `docs/plans/2026-11-03-close-phase-6-7-8-exits.md` §2.3,
`docs/plans/2026-11-04-phase-8-5-veo-media.md`.

### Phase 9: Structure and documentation (M)

| Step | Finding | Action | Status |
|---|---|---|---|
| 9.1 | S-20 | Split the rest of `world.rs`: abilities, vision, collision | Done, WP1 `8ac33ad` |
| 9.2 | large functions | Split `validate_map`, `build_chunk_mesh` and `sync_*`. Enable `too_many_lines` | Done: threshold and guard rails WP0 `808b6dd`; splits WP2 `b3e47ae`, WP3 `bdb3abf`, WP4 `ec55bf2`, WP5 `554bb81`, WP6 `821c6b0`, WP7 `295f1d9`, WP8 `ed7aa8e`; last suppressions removed and C8 in `0336642` |
| 9.3 | F-13 | Move faction and armour identity into catalog data | Done, WP12 `56f243f` |
| 9.4 | F-5 legacy, F-7 legacy | Close the open 2026-10-26 items: sandbox `let _`, `.min(7)`, compatibility `include_str!`, typed `PlayerId` | Done: WP10 `2797d4a` (F-05, F-08), WP11 `331ccb4` (F-07) |
| 9.5 | Q-9, Q-12 | Rewrite `DESIGN.md` with per-subsystem status; align charter §4 and §5 | Done, WP13 `0336642` |
| 9.6 | Q-10 | One plan status index; fix numbering; move kept evidence out of `/tmp` | Done, WP9 `10d92c5` |

**Exit:** the charter describes the code as it is, and every claim in it is checked by CI.

**Exit status (2026-11-03): met; CI green on `main` at `6b39cd2` (run 41665216936, all six jobs including `throughput`).** Every rule of `AGENTS.md`
now carries a tag: a CI check, a review rule or a procedure (charter §9). The checks are clippy lints, the CI jobs
(`check-and-test`, the determinism jobs and the new `throughput` job) and C1 to C21 in
`crates/workspace_checks/tests/charter.rs`. Claims that no check covers were reworded as review rules: the client's
read discipline (§3.2), the steady-state allocation claim of §3.3 until Q0011 is answered, documentation quality
and when a performance divergence is justified. Charter §6 is unchanged (Q0003). The owner can object to any
charter change in Q0010. Gaps found during the phase and left for later plans are listed in §9 of
`docs/plans/2026-11-03-phase-9-structure-and-docs.md` and in `DESIGN.md` §4. Plan:
`docs/plans/2026-11-03-phase-9-structure-and-docs.md`.

**Later (2026-11-04):** the four gaps of the Phase 9 blueprint's §9 that were left for bugfix plans
(`MapDocument::encode` panics, the last dual column and row unmeshed, peer bounces reported as 0 above 512 bubbles,
two phase orders not pinned by the golden replay) are fixed by
`docs/plans/2026-11-04-open-findings-c17-c18-p9-gaps.md` WP2, WP4, WP1 and WP5.

---

## 6. Explicitly out of scope until Phase 5 exits

- More mist or smog art-direction rounds.
- Shell-ordering work (Plan 53B) and presentation discontinuities (Plan 55).
- New units, structures or abilities.
- Networking, replay and bot API, which have no implementation yet.

A full rewrite is not recommended. The foundations are sound, and the failures are concentrated and fixable.

---

## Appendix A: Reproduction results

Run 2026-11-01 from a scratch crate depending on the real sim crates. Each test asserts the *correct*
behaviour, so a failure proves the bug. The file below is the complete suite. To run it, create a crate with
path dependencies on `sim_core`, `sim_map` and `sim_types`, copy `Cargo.lock` and `rust-toolchain.toml`, and
run `cargo test --release --test repro -- --nocapture --test-threads=1`.

| Test | Finding | Result |
|---|---|---|
| r01 hold fires | S-1 | **FAIL**: enemy at distance 2.0 undamaged after 200 ticks |
| r02 idle fires | control | pass, so r01 is not a setup fault |
| r03 head-on | M-1 | **FAIL**: min centre distance 0.030, contact is 0.75 |
| r04 short order | M-2 | **FAIL**: 0.000 progress in 100 ticks |
| r05 focus fire | S-2 | **FAIL**: order replaced by the nearer enemy |
| r06 anti-air | S-3 | **FAIL**: `flak_array_dome` damaged a ground unit |
| r07 hold shove | M-4 | **FAIL**: holder displaced 0.324 |
| r08 crowd of 60 | steering arrival | pass, not reproduced |
| r13 crowd of 150 | steering arrival | pass, not reproduced |
| r09 unreachable | M-3 | **FAIL**: 20/20 still ordered, 15.67 ms/tick. Expectation rewritten (Q0006), see below |
| r10 large length | F-1 | **FAIL**: release returns 0; debug panics with add overflow |
| r11 small normalise | F-3 | **FAIL**: 1.4142 |

Throughput, release build, one run on the audit host, attack-move scenario (r12):

| Map | Units | Ticks/s | Against 10,000 |
|---|---:|---:|---:|
| 64×64 | 0 | 969 | 10× short |
| 64×64 | 100 | 490 | 20× short |
| 64×64 | 400 | 112 | 89× short |
| 128×128 | 400 | 81 | 123× short |
| 256×256 | 0 | 64 | 156× short |

All still exceed real time at 25 Hz. The empty-map rows show cost that scales with map area, not units, which
fits the per-tick full-state checksum and vision scan [A]. Profile before optimising.

```rust
use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::*;

fn s(v: f64) -> SimScalar { SimScalar::from_num(v) }
fn v(x: f64, y: f64) -> SimVec2 { SimVec2::new(s(x), s(y)) }
fn flat(n: u32) -> SimWorld {
    SimWorld::new(GameMap::new(n, n, MapCell::default_ground(CliffLevel(0))).unwrap(), 7)
}
fn unit(w: &mut SimWorld, slug: &str, p: u8, x: f64, y: f64) -> EntityId {
    let id = w.catalog.id_by_slug(slug).unwrap();
    w.spawn_unit(id, PlayerId(p), v(x, y)).unwrap()
}
fn order(w: &mut SimWorld, t: &mut u32, p: u8, es: Vec<EntityId>, o: UnitOrder) {
    let b = TickBundle { tick: Tick(*t), commands: vec![PlayerCommand { player_id: PlayerId(p), entities: es, order: o, queue: false }], state_checksum: None };
    w.step(&b); *t += 1;
}
fn run(w: &mut SimWorld, t: &mut u32, n: u32) {
    for _ in 0..n { w.step(&TickBundle { tick: Tick(*t), commands: vec![], state_checksum: None }); *t += 1; }
}
fn dist(w: &SimWorld, a: EntityId, b: EntityId) -> f64 {
    let (p, q) = (w.entities[&a].position, w.entities[&b].position);
    let (dx, dy) = ((p.x - q.x).to_f32() as f64, (p.y - q.y).to_f32() as f64);
    (dx * dx + dy * dy).sqrt()
}

#[test] fn r01_hold_position_fires_at_enemy_in_range() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "rhino_beetle", 1, 12.0, 10.0);
    let hp0 = w.entities[&b].hp;
    order(&mut w, &mut t, 0, vec![a], UnitOrder::HoldPosition);
    run(&mut w, &mut t, 200);
    assert!(w.entities.get(&b).map_or(true, |e| e.hp < hp0), "holder never damaged enemy 2.0 away (hp unchanged)");
}

#[test] fn r02_control_idle_unit_does_fire() {
    let mut w = flat(32); let mut t = 0;
    let _a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "rhino_beetle", 1, 12.0, 10.0);
    let hp0 = w.entities[&b].hp;
    run(&mut w, &mut t, 200);
    assert!(w.entities.get(&b).map_or(true, |e| e.hp < hp0));
}

#[test] fn r03_head_on_friendlies_do_not_interpenetrate() {
    let mut w = flat(64); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 20.0, 30.0);
    let b = unit(&mut w, "pulse_bot", 0, 40.0, 30.0);
    order(&mut w, &mut t, 0, vec![a], UnitOrder::Move { target: v(40.0, 30.0) });
    order(&mut w, &mut t, 0, vec![b], UnitOrder::Move { target: v(20.0, 30.0) });
    let mut min = 99.0f64;
    for _ in 0..500 { run(&mut w, &mut t, 1); min = min.min(dist(&w, a, b)); }
    println!("head-on min centre distance = {min:.3}");
    assert!(min > 0.5, "contact distance 0.75; min centre distance {min:.3}");
}

#[test] fn r04_short_move_order_on_stationary_pair_moves() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let b = unit(&mut w, "pulse_bot", 0, 10.8, 10.0);
    order(&mut w, &mut t, 0, vec![a, b], UnitOrder::Move { target: v(10.4, 11.5) });
    run(&mut w, &mut t, 100);
    let (pa, pb) = (w.entities[&a].position, w.entities[&b].position);
    let moved = f64::max((pa.y - s(10.0)).to_f32() as f64, (pb.y - s(10.0)).to_f32() as f64);
    println!("short-order max progress = {moved:.3}");
    assert!(moved > 0.5, "units barely moved: {moved:.3}");
}

#[test] fn r05_explicit_focus_fire_survives_nearby_enemy() {
    let mut w = flat(32); let mut t = 0;
    let a = unit(&mut w, "pulse_bot", 0, 10.0, 10.0);
    let near = unit(&mut w, "psi_grey", 1, 12.0, 10.0);
    let far = unit(&mut w, "rhino_beetle", 1, 18.0, 10.0);
    run(&mut w, &mut t, 2); // establish vision
    order(&mut w, &mut t, 0, vec![a], UnitOrder::AttackTarget { target: far });
    run(&mut w, &mut t, 3);
    let front = w.entities[&a].order_queue.front().cloned();
    println!("front after focus order = {front:?} (far={far:?} near={near:?})");
    assert!(matches!(front, Some(UnitOrder::AttackTarget { target }) if target == far));
}

#[test] fn r06_anti_air_structure_ignores_ground() {
    let mut w = flat(32); let mut t = 0;
    let dome = w.catalog.id_by_slug("flak_array_dome").unwrap();
    w.spawn_structure(dome, PlayerId(0), 10, 10).unwrap();
    let b = unit(&mut w, "rhino_beetle", 1, 16.0, 11.0);
    let hp0 = w.entities[&b].hp;
    run(&mut w, &mut t, 300);
    assert!(w.entities.get(&b).is_some_and(|e| e.hp == hp0), "air-only dome damaged a ground unit");
}

#[test] fn r07_enemy_cannot_shove_hold_position() {
    let mut w = flat(32); let mut t = 0;
    let h = unit(&mut w, "pulse_bot", 0, 15.0, 15.0);
    let m = unit(&mut w, "rhino_beetle", 1, 10.0, 15.0);
    order(&mut w, &mut t, 0, vec![h], UnitOrder::HoldPosition);
    order(&mut w, &mut t, 1, vec![m], UnitOrder::Move { target: v(25.0, 15.0) });
    let p0 = w.entities[&h].position; let mut d = 0.0f64; let mut ticks = 0;
    for _ in 0..300 { run(&mut w, &mut t, 1); let Some(e) = w.entities.get(&h) else { break }; ticks += 1;
        d = d.max(f64::hypot((e.position.x - p0.x).to_f32() as f64, (e.position.y - p0.y).to_f32() as f64)); }
    println!("holder max displacement {d:.3} over {ticks} ticks");
    assert!(d < 0.01, "HoldPosition unit displaced {d:.3}");
}

#[test] fn r13_crowd_of_150_finishes_move() {
    let mut w = flat(64); let mut t = 0;
    let mut ids = vec![];
    for i in 0..150 { ids.push(unit(&mut w, "pulse_bot", 0, 5.0 + (i % 15) as f64, 5.0 + (i / 15) as f64)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(40.0, 40.0) });
    run(&mut w, &mut t, 6000);
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("150 units: after 6000 ticks, {busy}/150 still have an order");
    assert_eq!(busy, 0);
}

#[test] fn r08_crowd_of_60_finishes_move() {
    let mut w = flat(64); let mut t = 0;
    let mut ids = vec![];
    for i in 0..60 { ids.push(unit(&mut w, "pulse_bot", 0, 10.0 + (i % 10) as f64, 10.0 + (i / 10) as f64)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(40.0, 40.0) });
    run(&mut w, &mut t, 6000);
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("after 6000 ticks, {busy}/60 still have an order");
    assert_eq!(busy, 0);
}

fn walled(n: u32) -> GameMap {
    let mut m = GameMap::new(n, n, MapCell::default_ground(CliffLevel(0))).unwrap();
    let cx = n / 2;
    for y in 0..n {
        let idx = (y * n + cx) as usize;
        m.cells_mut()[idx].flags = CellFlags::empty();
    }
    m
}

// Expectation rewritten 2026-11-03 (Q0006): the owner chose to keep walking to the nearest reachable point,
// so the order ends there (about 545 ticks at speed 0.09), on the near side of the wall. The audit's version
// asserted no order left after 500 ticks. The scratch repro crate no longer exists, so this version was not run
// here; the check that runs is `movement_contact_tests::unreachable_order_reports_retargets_and_terminates`
// in sim_core.
#[test] fn r09_unreachable_order_walks_to_nearest_reachable_point() {
    let mut w = SimWorld::new(walled(64), 7); let mut t = 0;
    let mut ids = vec![];
    for i in 0..20 { ids.push(unit(&mut w, "pulse_bot", 0, 5.0 + (i % 5) as f64 * 1.5, 5.0 + (i / 5) as f64 * 1.5)); }
    order(&mut w, &mut t, 0, ids.clone(), UnitOrder::Move { target: v(50.0, 50.0) });
    run(&mut w, &mut t, 900);
    let busy = ids.iter().filter(|i| !w.entities[i].order_queue.is_empty()).count();
    println!("unreachable: {busy}/20 still ordered after 900 ticks");
    assert_eq!(busy, 0);
    for i in &ids {
        let p = w.entities[i].position;
        assert!(p.x.to_f32() < 32.0, "unit crossed the wall");
        let d = f64::hypot((p.x.to_f32() - 31.5) as f64, (p.y.to_f32() - 50.5) as f64);
        assert!(d < 6.0, "unit stopped {d:.2} from the nearest reachable point");
    }
}

#[test] fn r10_length_of_large_vector_is_not_zero() {
    let l = v(32768.0, 32768.0).length();
    println!("length = {l:?}");
    assert!(l > SimScalar::ZERO);
}

#[test] fn r11_normalize_small_vector_is_unit() {
    let raw = SimScalar::from_raw(92681);
    let n = SimVec2::new(raw, SimScalar::ZERO).normalize();
    println!("normalize small = {n:?}");
    let l = n.unwrap().x.to_f32() as f64;
    assert!((l - 1.0).abs() < 0.01, "normalised x = {l}");
}

fn bench(n: u32, units: usize, ticks: u32) -> f64 {
    let mut w = flat(n); let mut t = 0;
    let (mut a, mut b) = (vec![], vec![]);
    for i in 0..units { let (c, r) = ((i % 20) as f64, (i / 20) as f64);
        if i % 2 == 0 { a.push(unit(&mut w, "pulse_bot", 0, 5.0 + c, 5.0 + r)); }
        else { b.push(unit(&mut w, "pulse_bot", 1, n as f64 - 6.0 - c, n as f64 - 6.0 - r)); } }
    order(&mut w, &mut t, 0, a, UnitOrder::AttackMove { target: v(n as f64 - 8.0, n as f64 - 8.0) });
    order(&mut w, &mut t, 1, b, UnitOrder::AttackMove { target: v(8.0, 8.0) });
    let st = std::time::Instant::now();
    run(&mut w, &mut t, ticks);
    ticks as f64 / st.elapsed().as_secs_f64()
}
#[test] fn r12_throughput_report() {
    for (n, u) in [(64, 0), (64, 100), (64, 400), (128, 400), (256, 0)] {
        println!("map {n}x{n}, {u} units: {:.0} ticks/s", bench(n, u, 300));
    }
}
```
