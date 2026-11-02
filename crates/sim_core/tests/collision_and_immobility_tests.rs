//! # Integration Tests: Collision Steering, Push Priority, and Immobility
//!
//! Verifies:
//! 1. Moving friendly units push idle friendly units laterally (perpendicular to velocity vector), preserving forward momentum.
//! 2. Units on `HoldPosition` act as hard immovable walls (`cannot_be_pushed == true`).
//! 3. Units in Siege Mode or channeling spells (`is_immobile = true`, `unpushable = true`) cannot be displaced.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::unreadable_literal
)]

use sim_core::scenario::{build_world, PlayerSetup, StartLoadoutCatalog};
use sim_core::steering::{compute_push_displacement, PushEntityParams};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Test 7: Moving friendly unit pushes idle friendly unit laterally (perpendicular to velocity),
/// preventing lane blockage while preserving forward advance.
#[test]
fn test_moving_friendly_pushes_idle_friendly_laterally() -> Result<(), Box<dyn std::error::Error>> {
    let mover_params = PushEntityParams {
        id: EntityId(1),
        pos: SimVec2::new(SimScalar::from_num(5), SimScalar::from_num(10)),
        vel: SimVec2::new(SimScalar::from_num(1), SimScalar::ZERO), // Moving East (+X)
        owner: PlayerId(0),
        is_holding: false,
        push_priority: 20,
        cannot_be_pushed: false,
    };

    let idle_params = PushEntityParams {
        id: EntityId(2),
        pos: SimVec2::new(SimScalar::from_num(5.5), SimScalar::from_num(10)),
        vel: SimVec2::ZERO, // Idle
        owner: PlayerId(0), // Friendly
        is_holding: false,
        push_priority: 20,
        cannot_be_pushed: false,
    };

    let distance = sim_core::movement::checked_distance(mover_params.pos, idle_params.pos)
        .ok_or("distance")?;
    let combined_radius = SimScalar::ONE;

    let (delta_mover, delta_idle) =
        compute_push_displacement(mover_params, idle_params, distance, combined_radius)?;

    // Mover should suffer 0 displacement along forward vector
    assert_eq!(
        delta_mover,
        SimVec2::ZERO,
        "Moving friendly must not be pushed backward by idle friendly"
    );

    // Idle unit must be pushed laterally (non-zero Y component, perpendicular to (+1, 0))
    assert!(
        delta_idle.y.abs() > SimScalar::ZERO,
        "Idle friendly must be pushed laterally perpendicular to mover's velocity vector"
    );
    Ok(())
}

/// Test 8: Friendly unit on HoldPosition acts as an immovable wall:
/// Moving friendly cannot displace it (`delta == 0`); mover is displaced outward.
#[test]
fn test_hold_position_friendly_acts_as_immovable_wall() -> Result<(), Box<dyn std::error::Error>> {
    let mover_params = PushEntityParams {
        id: EntityId(1),
        pos: SimVec2::new(SimScalar::from_num(5), SimScalar::from_num(10)),
        vel: SimVec2::new(SimScalar::from_num(1), SimScalar::ZERO),
        owner: PlayerId(0),
        is_holding: false,
        push_priority: 20,
        cannot_be_pushed: false,
    };

    let hold_params = PushEntityParams {
        id: EntityId(2),
        pos: SimVec2::new(SimScalar::from_num(5.6), SimScalar::from_num(10)),
        vel: SimVec2::ZERO,
        owner: PlayerId(0),
        is_holding: true,
        push_priority: 20,
        cannot_be_pushed: true,
    };

    let distance = sim_core::movement::checked_distance(mover_params.pos, hold_params.pos)
        .ok_or("distance")?;
    let combined_radius = SimScalar::ONE;

    let (delta_mover, delta_hold) =
        compute_push_displacement(mover_params, hold_params, distance, combined_radius)?;

    // Hold unit must not move an inch
    assert_eq!(
        delta_hold,
        SimVec2::ZERO,
        "Friendly on HoldPosition must not be displaced at all"
    );

    // Mover absorbs 100% of the displacement away from the immovable wall
    assert!(
        delta_mover.x < SimScalar::ZERO,
        "Mover must be displaced backward/away from the HoldPosition immovable wall"
    );
    Ok(())
}

/// Test 9: Unit in Siege Mode (`is_immobile = true`, `unpushable = true`) cannot be displaced
/// by moving units during gameplay ticks.
#[test]
fn test_immobile_unit_unpushable_during_siege_and_spell() {
    let width = 32;
    let height = 32;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x51E6E);

    let player_blue = PlayerId(0);

    let crawler_id = world
        .catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler");
    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");

    // Spawn Crawler at (10.0, 10.0)
    let sieged_ent_id = world
        .spawn_unit(
            crawler_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("spawn crawler");

    // Put unit into Siege Mode
    {
        let ent = world.entities.get_mut(&sieged_ent_id).unwrap();
        ent.is_sieged = true;
        ent.is_immobile = true;
        ent.unpushable = true;
    }

    assert!(
        world
            .entities
            .get(&sieged_ent_id)
            .unwrap()
            .cannot_be_pushed(),
        "Sieged entity must report cannot_be_pushed() == true"
    );

    let sieged_pos_before = world.entities.get(&sieged_ent_id).unwrap().position;

    // Spawn a friendly mover directly overlapping the sieged crawler at (10.3, 10.0)
    let mover_id = world
        .spawn_unit(
            pulse_bot_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(10.3), SimScalar::from_num(10)),
        )
        .expect("spawn mover");

    // Order mover to charge into the sieged unit
    world
        .entities
        .get_mut(&mover_id)
        .unwrap()
        .issue_order(
            UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_num(9), SimScalar::from_num(10)),
            },
            false,
        )
        .expect("valid authored fixture order or buff");

    // Run collision resolution ticks
    for tick in 1..=10 {
        world.step(&empty_bundle(tick));

        let sieged_pos = world.entities.get(&sieged_ent_id).unwrap().position;
        assert_eq!(
            sieged_pos, sieged_pos_before,
            "Sieged immobile unit must remain locked in place and never be displaced"
        );
    }
}

/// Test 9: Multi-unit convergence smoothness, speed cap enforcement, and anti-vibration invariants.
///
/// Spawns 1 enemy target and 8 converging friendly units surrounding it in close proximity.
/// Verifies:
/// 1. Every unit on every tick satisfies `|pos(t) - pos(t-1)| <= max_speed + 0.002` (strict speed cap).
/// 2. Zero high-frequency direction-reversal vibration (reversals <= 2 per unit across 120 ticks).
#[test]
fn test_converging_units_respect_speed_cap_and_do_not_vibrate() {
    let width = 64;
    let height = 64;
    let map = GameMap::new(width, height, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: valid map");
    let mut world = SimWorld::new(map, 0x00C0_FFEE);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    // Pick a mobile melee unit and target unit from catalog
    let melee_archetype = world
        .catalog
        .archetypes()
        .into_iter()
        .find(|a| {
            a.max_speed > SimScalar::ZERO
                && a.weapon
                    .as_ref()
                    .is_some_and(|w| w.max_range <= SimScalar::from_num(2))
        })
        .expect("must find a melee archetype in catalog");
    let melee_type_id = melee_archetype.unit_type_id;
    let max_speed = melee_archetype.max_speed;

    let target_pos = SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20));
    let target_id = world
        .spawn_unit(melee_type_id, player_red, target_pos)
        .expect("spawn enemy target");

    // Hold target in place to serve as stationary convergence focus
    if let Some(target) = world.entities.get_mut(&target_id) {
        target.is_holding_position = true;
    }

    // Spawn 8 friendly units in a circle around target at radius 3.0
    let mut converging_ids = Vec::new();
    let spawn_radius = SimScalar::from_num(3);
    for i in 0..8 {
        let angle_f = (i as f32) * (std::f32::consts::TAU / 8.0);
        let offset = SimVec2::new(
            spawn_radius * SimScalar::from_f32_lossy(angle_f.cos()),
            spawn_radius * SimScalar::from_f32_lossy(angle_f.sin()),
        );
        let pos = target_pos + offset;
        let id = world
            .spawn_unit(melee_type_id, player_blue, pos)
            .expect("spawn converging friendly");
        converging_ids.push(id);
    }

    // Order all 8 units to AttackTarget
    for &id in &converging_ids {
        if let Some(unit) = world.entities.get_mut(&id) {
            unit.issue_order(UnitOrder::attack(target_id), false)
                .expect("valid authored fixture order or buff");
        }
    }

    // Track per-unit trajectory: (prev_pos, prev_delta, reversal_count)
    let mut unit_tracker: std::collections::BTreeMap<EntityId, (SimVec2, Option<SimVec2>, u32)> =
        std::collections::BTreeMap::new();
    for &id in &converging_ids {
        let p = world.entities.get(&id).unwrap().position;
        unit_tracker.insert(id, (p, None, 0));
    }

    // Step 120 ticks
    for tick in 1..=120 {
        world.step(&empty_bundle(tick));

        for &id in &converging_ids {
            let Some(ent) = world.entities.get(&id) else {
                continue;
            };
            let cur_pos = ent.position;
            let (prev_pos, prev_delta, reversals) = unit_tracker.get_mut(&id).unwrap();

            let delta = cur_pos - *prev_pos;
            let tick_disp = delta.length();

            // Invariant 1: Strict speed cap (unit displacement per tick must not exceed max_speed + epsilon)
            let allowed_cap = max_speed + SimScalar::from_f32_lossy(0.002);
            assert!(
                tick_disp <= allowed_cap,
                "Tick {}: Unit {:?} exceeded max_speed: disp = {}, allowed = {}",
                tick,
                id,
                tick_disp,
                allowed_cap
            );

            // Invariant 2: Anti-vibration ping-pong reversal check
            if let Some(p_delta) = *prev_delta {
                let threshold = SimScalar::from_f32_lossy(0.015);
                if tick_disp > threshold && p_delta.length() > threshold {
                    let dot = delta.dot(p_delta);
                    if dot < SimScalar::from_f32_lossy(-0.3) {
                        *reversals += 1;
                    }
                }
            }

            *prev_delta = Some(delta);
            *prev_pos = cur_pos;
        }
    }

    // Invariant 3: Reversal count per unit must remain <= 2 across 120 ticks
    for (&id, &(_, _, reversals)) in &unit_tracker {
        assert!(
            reversals <= 2,
            "Unit {:?} experienced excessive high-frequency vibration: {} reversals in 120 ticks",
            id,
            reversals
        );
    }
}

fn load_canyon_map() -> GameMap {
    MapFile::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/showcase_canyon.scmap.ron"),
    )
    .expect("load showcase_canyon")
    .decode()
    .expect("decode showcase_canyon")
    .to_game_map()
    .expect("to_game_map")
}

/// Integration Test: Squad Move across ramp settles to Idle and auto-acquires enemies.
#[test]
fn test_squad_move_across_ramp_settles_and_auto_acquires() {
    let map = load_canyon_map();
    let mut world = SimWorld::new(map, 0x1234);
    let p_blue = PlayerId(0);
    let p_red = PlayerId(1);

    let ranged_arch = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Ground
                && arch
                    .weapon
                    .as_ref()
                    .or(arch.ground_weapon.as_ref())
                    .is_some_and(|w| w.max_range > SimScalar::from_num(3))
        })
        .expect("ranged archetype");
    let ranged_type = ranged_arch.unit_type_id;

    let melee_arch = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Ground
                && arch
                    .weapon
                    .as_ref()
                    .or(arch.ground_weapon.as_ref())
                    .is_some_and(|w| w.max_range <= SimScalar::from_num(2))
        })
        .expect("melee archetype");
    let melee_type = melee_arch.unit_type_id;

    // Spawn 4 ranged and 2 melee on canyon floor (CliffLevel(1)) near Ramp 2 (bottom at x = 29, y = 8..=11)
    let mut squad = Vec::new();
    for i in 0..4 {
        let pos = SimVec2::new(
            SimScalar::from_num(32 + i % 2),
            SimScalar::from_f32_lossy(9.0 + (i / 2) as f32 * 1.5),
        );
        let id = world
            .spawn_unit(ranged_type, p_blue, pos)
            .expect("spawn ranged");
        squad.push(id);
    }
    for i in 0..2 {
        let pos = SimVec2::new(
            SimScalar::from_num(34),
            SimScalar::from_f32_lossy(9.0 + i as f32 * 1.5),
        );
        let id = world
            .spawn_unit(melee_type, p_blue, pos)
            .expect("spawn melee");
        squad.push(id);
    }

    // Command squad to Move up Ramp 2 onto high ground (CliffLevel(2)) at (22.0, 10.0)
    let move_target = SimVec2::new(SimScalar::from_num(22), SimScalar::from_num(10));
    for &id in &squad {
        world
            .entities
            .get_mut(&id)
            .unwrap()
            .issue_order(
                UnitOrder::Move {
                    target: move_target,
                },
                false,
            )
            .expect("issue move order");
    }

    // Step simulation until all units complete their Move order
    let mut all_idle = false;
    for tick in 1..=400 {
        world.step(&empty_bundle(tick));
        all_idle = squad.iter().all(|id| {
            let ent = &world.entities[id];
            ent.order_queue.is_empty() && ent.velocity == SimVec2::ZERO
        });
        if all_idle {
            break;
        }
    }
    assert!(
        all_idle,
        "All 6 units must complete Move order and settle to Idle"
    );

    // Spawn an enemy within weapon range of the arrived squad on high ground
    let enemy_pos = SimVec2::new(SimScalar::from_num(22), SimScalar::from_num(13));
    let enemy_id = world
        .spawn_unit(melee_type, p_red, enemy_pos)
        .expect("spawn enemy");

    // Step a few ticks to allow auto-acquisition
    for tick in 401..=410 {
        world.step(&empty_bundle(tick));
    }

    // Arrived units must auto-acquire enemy
    let acquiring_units = squad
        .iter()
        .filter(|id| world.entities[id].current_target == Some(enemy_id))
        .count();
    assert!(
        acquiring_units > 0,
        "Settled squad must auto-acquire enemy target, got {} acquiring",
        acquiring_units
    );
}

/// Integration Test: Higher-priority melee ally slides around stationary engaged ranged ally.
#[test]
fn test_higher_priority_melee_slides_around_engaged_ranged_ally() {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("valid flat map");
    let mut world = SimWorld::new(map, 0x1234);
    let p_blue = PlayerId(0);
    let p_red = PlayerId(1);

    let ranged_arch = world
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.weapon
                    .as_ref()
                    .or(a.ground_weapon.as_ref())
                    .is_some_and(|w| w.max_range > SimScalar::from_num(3))
        })
        .min_by_key(|a| a.push_priority)
        .expect("ranged unit with lowest push priority");
    let ranged_type = ranged_arch.unit_type_id;
    let r_ranged = ranged_arch.collision_radius;
    let p_ranged = ranged_arch.push_priority;

    let melee_arch = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.push_priority > p_ranged
                && a.weapon
                    .as_ref()
                    .or(a.ground_weapon.as_ref())
                    .is_some_and(|w| w.max_range <= SimScalar::from_num(2))
        })
        .expect("melee higher priority");
    let melee_type = melee_arch.unit_type_id;
    let r_melee = melee_arch.collision_radius;

    // Enemy at (15, 10) on HoldPosition
    let enemy_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(10));
    let enemy_id = world
        .spawn_unit(melee_type, p_red, enemy_pos)
        .expect("enemy");
    world
        .entities
        .get_mut(&enemy_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::HoldPosition);

    // Friendly ranged at (10, 10) engaged with enemy
    let ranged_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let ranged_id = world
        .spawn_unit(ranged_type, p_blue, ranged_pos)
        .expect("ranged");
    world
        .entities
        .get_mut(&ranged_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::attack(enemy_id));

    // Friendly melee at (8.5, 10) pursuing the same enemy
    let melee_pos = SimVec2::new(SimScalar::from_f32_lossy(8.5), SimScalar::from_num(10));
    let melee_id = world
        .spawn_unit(melee_type, p_blue, melee_pos)
        .expect("melee");
    world
        .entities
        .get_mut(&melee_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::attack(enemy_id));

    let min_sep_threshold = (r_ranged + r_melee) * SimScalar::from_f32_lossy(0.80);
    let mut prev_melee_pos = melee_pos;
    let mut prev_melee_delta: Option<SimVec2> = None;
    let mut reversals = 0;

    for tick in 1..=80 {
        world.step(&empty_bundle(tick));
        let ranged_curr = world.entities[&ranged_id].position;
        let melee_curr = world.entities[&melee_id].position;
        let ranged_drift = (ranged_curr - ranged_pos).length();
        assert!(
            ranged_drift < SimScalar::from_f32_lossy(0.15),
            "Tick {}: Engaged ranged unit drifted by {} (must remain anchored)",
            tick,
            ranged_drift
        );

        // Invariant 2: Melee unit never walks on top of ranged unit
        let sep = (melee_curr - ranged_curr).length();
        assert!(
            sep > min_sep_threshold,
            "Tick {}: Melee and ranged units overlapped too deeply: sep = {} (min = {})",
            tick,
            sep,
            min_sep_threshold
        );

        // Track melee movement reversals
        let delta = melee_curr - prev_melee_pos;
        let disp = delta.length();
        if let Some(p_delta) = prev_melee_delta {
            if disp > SimScalar::from_f32_lossy(0.015)
                && p_delta.length() > SimScalar::from_f32_lossy(0.015)
                && delta.dot(p_delta) < SimScalar::from_f32_lossy(-0.3)
            {
                reversals += 1;
            }
        }
        prev_melee_delta = Some(delta);
        prev_melee_pos = melee_curr;
    }

    assert!(
        reversals <= 2,
        "Melee unit wrapping around ally must not vibrate (reversals = {})",
        reversals
    );
}

/// Integration Test: 10-Unit LiveBattle squad ramp traversal on showcase_canyon.
#[test]
fn test_live_battle_10_unit_squad_ramp_traversal() {
    let map = load_canyon_map();
    let mut world = SimWorld::new(map, 0x1234);
    let p_blue = PlayerId(0);

    let small_arch = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.collision_radius <= SimScalar::from_num(0.4)
                && a.max_speed > SimScalar::ZERO
        })
        .expect("small ground unit");
    let small_type = small_arch.unit_type_id;

    let medium_arch = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && a.collision_radius > SimScalar::from_num(0.4)
                && a.collision_radius <= SimScalar::from_num(0.8)
                && a.max_speed > SimScalar::ZERO
        })
        .expect("medium ground unit");
    let medium_type = medium_arch.unit_type_id;

    // Spawn 8 small + 2 medium units in canyon floor south-west of Ramp 1 (bottom landing at x = 34, y = 52..=55).
    // Ramp 1 ascends East to top landing at x = 38. Low ground pocket is at x = 35..=37, y = 51.
    let mut squad = Vec::new();
    for i in 0..8 {
        let pos = SimVec2::new(
            SimScalar::from_f32_lossy(30.0 + (i % 3) as f32 * 1.2),
            SimScalar::from_f32_lossy(48.0 + (i / 3) as f32 * 1.2),
        );
        let id = world
            .spawn_unit(small_type, p_blue, pos)
            .expect("spawn small");
        squad.push(id);
    }
    for i in 0..2 {
        let pos = SimVec2::new(
            SimScalar::from_f32_lossy(30.0 + i as f32 * 1.5),
            SimScalar::from_f32_lossy(46.0),
        );
        let id = world
            .spawn_unit(medium_type, p_blue, pos)
            .expect("spawn medium");
        squad.push(id);
    }

    // Command squad to AttackMove onto high ground plateau at (45.0, 53.0) across Ramp 1
    let destination = SimVec2::new(SimScalar::from_num(45), SimScalar::from_num(53));
    for &id in &squad {
        world
            .entities
            .get_mut(&id)
            .unwrap()
            .issue_order(
                UnitOrder::AttackMove {
                    target: destination,
                },
                false,
            )
            .expect("issue attack move");
    }

    for tick in 1..=350 {
        world.step(&empty_bundle(tick));
    }

    // Invariant: ALL 10 units must have ascended the ramp onto high ground (x >= 38.0).
    // ZERO units stuck at the ramp bottom or in the side pocket (x <= 37.0).

    for &id in &squad {
        let ent = &world.entities[&id];
        assert!(
            ent.position.x >= SimScalar::from_num(38),
            "Unit {:?} (type {:?}) stuck at x={:?}, y={:?} (must ascend ramp to x >= 38)",
            id,
            ent.unit_type_id,
            ent.position.x,
            ent.position.y
        );
    }
}

/// Integration Test: Exact LiveBattle default_battle scenario dual ramp traversal.
/// Verifies that every mobile combatant spawned from `battle_automaton` at Player 0 start (6, 6)
/// traverses the first ramp down to the canyon floor, crosses the canyon, and ascends the
/// second ramp onto enemy high ground without getting stuck at the bottom of either ramp.
#[test]
fn test_live_battle_scenario_dual_ramp_traversal() {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = MapFile::load(&manifest_dir.join("../../assets/maps/showcase_canyon.scmap.ron"))
        .expect("load canyon map")
        .decode()
        .expect("decode canyon map");
    let loadouts =
        StartLoadoutCatalog::load(manifest_dir.join("../../assets/catalog/start_loadouts.ron"))
            .expect("load start loadouts");
    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "battle_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "battle_automaton".to_string(),
        },
    ];
    let mut world = build_world(&doc, &loadouts, &players, 1592639215).expect("build world");

    let p0 = PlayerId(0);
    let p1 = PlayerId(1);
    let target_approach_p1 = SimVec2::new(
        SimScalar::from_f32_lossy(52.5),
        SimScalar::from_f32_lossy(52.5),
    );
    let target_approach_p0 = SimVec2::new(
        SimScalar::from_f32_lossy(5.5),
        SimScalar::from_f32_lossy(5.5),
    );

    // Collect Player 0 combatants
    let mut p0_combatants = Vec::new();
    let mut small_medium_p0 = 0;
    let mut large_p0 = 0;

    for (&id, entity) in &world.entities {
        if entity.owner != p0 || entity.is_immobile {
            continue;
        }
        let arch = world.catalog.get(entity.unit_type_id).expect("archetype");
        if arch.capabilities.harvester.is_some() {
            continue;
        }
        if arch.ground_weapon.is_some() || arch.air_weapon.is_some() || arch.weapon.is_some() {
            p0_combatants.push(id);
            if arch.collision_radius > SimScalar::from_f32_lossy(0.5) {
                large_p0 += 1;
            } else {
                small_medium_p0 += 1;
            }
        }
    }

    assert!(
        small_medium_p0 >= 6,
        "Player 0 must have at least 6 small-medium combatants (found {small_medium_p0})"
    );
    assert_eq!(
        large_p0, 2,
        "Player 0 must have exactly 2 large radius=0.75 combatants (found {large_p0})"
    );

    // Issue AttackMove toward enemy base approach (52.5, 52.5)
    for &id in &p0_combatants {
        world
            .entities
            .get_mut(&id)
            .unwrap()
            .issue_order(
                UnitOrder::AttackMove {
                    target: target_approach_p1,
                },
                false,
            )
            .expect("issue attack move");
    }

    // Step simulation to tick 980 (before combat resolution wipes out units at enemy base)
    for tick in 1..=980 {
        world.step(&empty_bundle(tick));
    }

    // Invariant: EVERY Player 0 combatant descended Ramp 2, crossed canyon, and ascended Ramp 1
    // onto the enemy high ground plateau (CliffLevel(2), x >= 42.0, y >= 45.0).
    for &id in &p0_combatants {
        let ent = world.entities.get(&id).expect("combatant alive");
        let cliff = world
            .map
            .world_to_cell(ent.position)
            .ok()
            .and_then(|(cx, cy)| world.map.cell(cx, cy).ok())
            .map(|c| c.cliff_level);
        assert_eq!(
            cliff,
            Some(CliffLevel(2)),
            "Player 0 unit {:?} stuck at ({:?}, {:?}) with cliff {:?} (must be CliffLevel(2) on high ground)",
            id,
            ent.position.x,
            ent.position.y,
            cliff
        );
        assert!(
            ent.position.x >= SimScalar::from_num(42) && ent.position.y >= SimScalar::from_num(45),
            "Player 0 unit {:?} stuck at ({:?}, {:?}) (must ascend enemy ramp to x >= 42, y >= 45)",
            id,
            ent.position.x,
            ent.position.y
        );
    }

    // Vice-versa: fresh world, Player 1 squad marching to Player 0 base approach (5.5, 5.5)
    let mut world_rev =
        build_world(&doc, &loadouts, &players, 1592639215).expect("build world rev");
    let mut p1_combatants = Vec::new();
    let mut small_medium_p1 = 0;
    let mut large_p1 = 0;

    for (&id, entity) in &world_rev.entities {
        if entity.owner != p1 || entity.is_immobile {
            continue;
        }
        let arch = world_rev
            .catalog
            .get(entity.unit_type_id)
            .expect("archetype");
        if arch.capabilities.harvester.is_some() {
            continue;
        }
        if arch.ground_weapon.is_some() || arch.air_weapon.is_some() || arch.weapon.is_some() {
            p1_combatants.push(id);
            if arch.collision_radius > SimScalar::from_f32_lossy(0.5) {
                large_p1 += 1;
            } else {
                small_medium_p1 += 1;
            }
        }
    }

    assert!(
        small_medium_p1 >= 6,
        "Player 1 must have at least 6 small-medium combatants (found {small_medium_p1})"
    );
    assert_eq!(
        large_p1, 2,
        "Player 1 must have exactly 2 large radius=0.75 combatants (found {large_p1})"
    );

    for &id in &p1_combatants {
        world_rev
            .entities
            .get_mut(&id)
            .unwrap()
            .issue_order(
                UnitOrder::AttackMove {
                    target: target_approach_p0,
                },
                false,
            )
            .expect("issue attack move");
    }

    let mut ascended_ticks: std::collections::BTreeMap<EntityId, u32> =
        std::collections::BTreeMap::new();
    for tick in 1..=950 {
        world_rev.step(&empty_bundle(tick));

        for &id in &p1_combatants {
            if ascended_ticks.contains_key(&id) {
                continue;
            }
            if let Some(ent) = world_rev.entities.get(&id) {
                let (cx, cy) = world_rev.map.world_to_cell(ent.position).unwrap();
                let cell = world_rev.map.cell(cx, cy).unwrap();
                if cell.cliff_level == CliffLevel(2) && ent.position.x <= SimScalar::from_num(25) {
                    ascended_ticks.insert(id, tick);
                }
            }
        }
    }
    for &id in &p1_combatants {
        assert!(
            ascended_ticks.contains_key(&id),
            "Player 1 unit {:?} failed to ascend Ramp 1 onto CliffLevel(2) high ground",
            id
        );
    }
}

#[test]
fn battlefield_bubble_trajectories_are_smooth_without_micro_jitter(
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = MapFile::load(&manifest_dir.join("../../assets/maps/showcase_canyon.scmap.ron"))?
        .decode()?;
    let loadouts =
        StartLoadoutCatalog::load(manifest_dir.join("../../assets/catalog/start_loadouts.ron"))?;
    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "battle_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "battle_automaton".to_string(),
        },
    ];
    let mut world = build_world(&doc, &loadouts, &players, 1592639215)?;

    let p0 = PlayerId(0);
    let target_approach_p1 = SimVec2::new(
        SimScalar::from_f32_lossy(52.5),
        SimScalar::from_f32_lossy(52.5),
    );

    let mut p0_combatants = Vec::new();
    for (&id, entity) in &world.entities {
        if entity.owner != p0 || entity.is_immobile {
            continue;
        }
        let arch = world.catalog.get(entity.unit_type_id).expect("archetype");
        if arch.capabilities.harvester.is_some() {
            continue;
        }
        if arch.ground_weapon.is_some() || arch.air_weapon.is_some() || arch.weapon.is_some() {
            p0_combatants.push(id);
        }
    }

    for &id in &p0_combatants {
        world.entities.get_mut(&id).unwrap().issue_order(
            UnitOrder::AttackMove {
                target: target_approach_p1,
            },
            false,
        )?;
    }

    let mut prev_headings: std::collections::BTreeMap<EntityId, (f32, f32)> =
        std::collections::BTreeMap::new();
    let mut prev_turns: std::collections::BTreeMap<EntityId, (u32, f32, f32)> =
        std::collections::BTreeMap::new();
    let mut prev_positions: std::collections::BTreeMap<EntityId, SimVec2> =
        std::collections::BTreeMap::new();
    let mut prev_pos_headings: std::collections::BTreeMap<EntityId, (f32, f32)> =
        std::collections::BTreeMap::new();
    let mut prev_pos_turns: std::collections::BTreeMap<EntityId, (u32, f32, f32)> =
        std::collections::BTreeMap::new();

    let mut total_angle_deg = 0.0_f32;
    let mut total_samples = 0_usize;
    let mut zig_zags = 0_usize;

    let mut total_pos_angle_deg = 0.0_f32;
    let mut total_pos_samples = 0_usize;
    let mut pos_zig_zags = 0_usize;

    for &id in &p0_combatants {
        if let Some(ent) = world.entities.get(&id) {
            prev_positions.insert(id, ent.position);
        }
    }

    for tick in 1..=180 {
        world.step(&empty_bundle(tick));
        for &id in &p0_combatants {
            let Some(ent) = world.entities.get(&id) else {
                continue;
            };

            // 1. Velocity heading
            let vx = ent.velocity.x.to_f32();
            let vy = ent.velocity.y.to_f32();
            let speed = (vx * vx + vy * vy).sqrt();
            if speed >= 0.01 {
                let hx = vx / speed;
                let hy = vy / speed;

                if let Some(&(phx, phy)) = prev_headings.get(&id) {
                    let dot = (hx * phx + hy * phy).clamp(-1.0, 1.0);
                    let angle_deg = dot.acos().to_degrees();
                    let cross = phx * hy - phy * hx;

                    total_angle_deg += angle_deg;
                    total_samples += 1;

                    let turn_sign = if cross > 0.02 {
                        1.0_f32
                    } else if cross < -0.02 {
                        -1.0_f32
                    } else {
                        0.0_f32
                    };

                    if let Some(&(prev_tick, prev_sign, prev_angle_deg)) = prev_turns.get(&id) {
                        if prev_tick == tick - 1
                            && prev_sign * turn_sign < 0.0
                            && angle_deg > 8.0
                            && prev_angle_deg > 8.0
                        {
                            zig_zags += 1;
                        }
                    }

                    prev_turns.insert(id, (tick, turn_sign, angle_deg));
                }
                prev_headings.insert(id, (hx, hy));
            }

            // 2. Position displacement delta_p = p_t - p_{t-1}
            let prev_pos = prev_positions
                .insert(id, ent.position)
                .unwrap_or(ent.position);
            let dpx = (ent.position.x - prev_pos.x).to_f32();
            let dpy = (ent.position.y - prev_pos.y).to_f32();
            let d_dist = (dpx * dpx + dpy * dpy).sqrt();
            if d_dist >= 0.01 {
                let dhx = dpx / d_dist;
                let dhy = dpy / d_dist;

                if let Some(&(pdhx, pdhy)) = prev_pos_headings.get(&id) {
                    let dot = (dhx * pdhx + dhy * pdhy).clamp(-1.0, 1.0);
                    let angle_deg = dot.acos().to_degrees();
                    let cross = pdhx * dhy - pdhy * dhx;

                    total_pos_angle_deg += angle_deg;
                    total_pos_samples += 1;

                    let turn_sign = if cross > 0.02 {
                        1.0_f32
                    } else if cross < -0.02 {
                        -1.0_f32
                    } else {
                        0.0_f32
                    };

                    if let Some(&(prev_tick, prev_sign, prev_angle_deg)) = prev_pos_turns.get(&id) {
                        if prev_tick == tick - 1
                            && prev_sign * turn_sign < 0.0
                            && angle_deg > 8.0
                            && prev_angle_deg > 8.0
                        {
                            pos_zig_zags += 1;
                        }
                    }

                    prev_pos_turns.insert(id, (tick, turn_sign, angle_deg));
                }
                prev_pos_headings.insert(id, (dhx, dhy));
            }
        }
    }

    assert_eq!(
        zig_zags, 0,
        "Units must have zero sign-reversing velocity zig-zag oscillations (> 8 deg) while marching (got {zig_zags})"
    );
    assert_eq!(
        pos_zig_zags, 0,
        "Units must have zero sign-reversing position displacement zig-zag oscillations (> 8 deg) while marching (got {pos_zig_zags})"
    );
    assert!(
        total_samples > 150,
        "Must have recorded at least 150 velocity motion samples"
    );
    let avg_vel_angle = total_angle_deg / (total_samples as f32);
    assert!(
        avg_vel_angle < 3.0,
        "Average per-tick velocity angular deviation during march must be < 3.0 deg/tick (got {avg_vel_angle:.2} deg/tick)"
    );
    assert!(
        total_pos_samples > 150,
        "Must have recorded at least 150 motion samples"
    );
    let avg_pos_angle = total_pos_angle_deg / (total_pos_samples as f32);
    assert!(
        avg_pos_angle < 3.0,
        "Average per-tick position angular deviation during march must be < 3.0 deg/tick (got {avg_pos_angle:.2} deg/tick)"
    );

    Ok(())
}

/// SC2 Push Rule Verification: Two moving friendly units that overlap in `SimWorld` resolve
/// contact purely radially along their line of centers (zero perpendicular/lateral shoving).
#[test]
fn test_sc2_moving_units_never_push_each_other_laterally() -> Result<(), Box<dyn std::error::Error>>
{
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::new(map, 0x5C20);
    let unit_type = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && !a.is_immobile_by_default()
                && a.max_speed > SimScalar::ZERO
        })
        .ok_or("ground unit")?
        .unit_type_id;

    let pos_a = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let pos_b = SimVec2::new(SimScalar::from_f32_lossy(10.4), SimScalar::from_num(10));
    let id_a = world.spawn_unit(unit_type, PlayerId(0), pos_a)?;
    let id_b = world.spawn_unit(unit_type, PlayerId(0), pos_b)?;

    // Both units are actively moving East (+X) along y = 10.0
    world.entities.get_mut(&id_a).ok_or("a")?.velocity =
        SimVec2::new(SimScalar::from_f32_lossy(0.1), SimScalar::ZERO);
    world.entities.get_mut(&id_b).ok_or("b")?.velocity =
        SimVec2::new(SimScalar::from_f32_lossy(0.1), SimScalar::ZERO);

    world.resolve_collisions_and_push();

    let after_a = world.entities[&id_a].position;
    let after_b = world.entities[&id_b].position;

    assert_eq!(
        after_a.y,
        SimScalar::from_num(10),
        "Moving unit A must not be shoved laterally in Y by moving unit B"
    );
    assert_eq!(
        after_b.y,
        SimScalar::from_num(10),
        "Moving unit B must not be shoved laterally in Y by moving unit A"
    );
    assert!(
        after_a.x < pos_a.x && after_b.x > pos_b.x,
        "Overlapping moving units must separate radially along X: after_a={after_a:?}, after_b={after_b:?}"
    );
    Ok(())
}
