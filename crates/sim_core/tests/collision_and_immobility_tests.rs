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

use sim_core::steering::{compute_push_displacement, PushEntityParams};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
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
fn test_moving_friendly_pushes_idle_friendly_laterally() {
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

    let distance = mover_params.pos.distance(idle_params.pos);
    let combined_radius = SimScalar::ONE;

    let (delta_mover, delta_idle) =
        compute_push_displacement(mover_params, idle_params, distance, combined_radius);

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
}

/// Test 8: Friendly unit on HoldPosition acts as an immovable wall:
/// Moving friendly cannot displace it (`delta == 0`); mover is displaced outward.
#[test]
fn test_hold_position_friendly_acts_as_immovable_wall() {
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

    let distance = mover_params.pos.distance(hold_params.pos);
    let combined_radius = SimScalar::ONE;

    let (delta_mover, delta_hold) =
        compute_push_displacement(mover_params, hold_params, distance, combined_radius);

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
    world.entities.get_mut(&mover_id).unwrap().issue_order(
        UnitOrder::Move {
            target: SimVec2::new(SimScalar::from_num(9), SimScalar::from_num(10)),
        },
        false,
    );

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
            unit.issue_order(UnitOrder::AttackTarget { target: target_id }, false);
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
