//! # StarCraft II Threat Response: Helpless Unit Flee & Hazard Evasion Integration Tests
//!
//! Verifies:
//! 1. `test_idle_ground_only_unit_flees_when_attacked_by_air_unit`:
//!    Idle ground-only unit with zero anti-air weapon capability flees away from an air attacker.
//! 2. `test_hold_position_ground_unit_does_not_flee_air_attack`:
//!    HoldPosition acts as an absolute invariant anchor; a holding unit never flees even when bombarded.
//! 3. `test_idle_unit_flees_undetected_cloaked_attacker`:
//!    Idle combatant attacked by an undetected cloaked unit flees because the attacker is untargetable.
//! 4. `test_idle_unit_flees_psionic_storm_hazard_zone`:
//!    Idle units caught inside an active AoE hazard zone evacuate radially outward outside the hazard radius.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown
)]

use sim_core::{ActiveHazardZone, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Helper to create a flat 32x32 ground arena map.
fn create_test_world() -> SimWorld {
    let map =
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    SimWorld::new(map, 0x4242)
}

/// Test 1: Idle Ground-Only Unit Flees When Attacked by Air Unit.
/// Zealot (psi_grey) cannot hit air units. When attacked by Mutalisk (sonic_bat),
/// the Zealot flees directly away from the attacker.
#[test]
fn test_idle_ground_only_unit_flees_when_attacked_by_air_unit() {
    let mut world = create_test_world();
    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let muta_type = world.catalog.id_by_slug("sonic_bat").expect("sonic_bat");

    let initial_zealot_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16));
    let initial_muta_pos = SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(16));

    let zealot_id = world
        .spawn_unit(zealot_type, player_red, initial_zealot_pos)
        .expect("spawn zealot");

    let muta_id = world
        .spawn_unit(muta_type, player_blue, initial_muta_pos)
        .expect("spawn muta");

    // Order Mutalisk to attack Zealot
    world
        .entities
        .get_mut(&muta_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget { target: zealot_id });

    // Step simulation for 60 ticks
    for tick in 0..60 {
        world.step(&empty_bundle(tick));
    }

    let zealot = world
        .entities
        .get(&zealot_id)
        .expect("zealot must be alive");
    assert!(
        zealot.is_alive(),
        "Zealot should survive 60 ticks of muta fire"
    );
    assert!(
        zealot.shields < SimScalar::from_num(50) || zealot.hp < SimScalar::from_num(100),
        "Zealot must have taken damage"
    );

    // Zealot must have fled westward away from (13.0, 16.0)
    assert!(
        zealot.position.x < initial_zealot_pos.x,
        "Zealot must have fled westward away from attacker: initial x = {}, final x = {}",
        initial_zealot_pos.x.to_f32(),
        zealot.position.x.to_f32()
    );

    let final_distance = zealot.position.distance(initial_muta_pos);
    assert!(
        final_distance >= SimScalar::from_num(5),
        "Zealot must have increased distance from attacker origin (initial 3.0, final {})",
        final_distance.to_f32()
    );
}

/// Test 2: HoldPosition Ground Unit Does Not Flee Air Attack.
/// When ordered to HoldPosition, the unit acts as an absolute invariant anchor and does not flee.
#[test]
fn test_hold_position_ground_unit_does_not_flee_air_attack() {
    let mut world = create_test_world();
    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let zealot_type = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    let muta_type = world.catalog.id_by_slug("sonic_bat").expect("sonic_bat");

    let zealot_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(16));
    let muta_pos = SimVec2::new(SimScalar::from_num(13), SimScalar::from_num(16));

    let zealot_id = world
        .spawn_unit(zealot_type, player_red, zealot_pos)
        .expect("spawn zealot");

    let muta_id = world
        .spawn_unit(muta_type, player_blue, muta_pos)
        .expect("spawn muta");

    // Issue HoldPosition to Zealot
    world
        .entities
        .get_mut(&zealot_id)
        .unwrap()
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("valid authored fixture order or buff");

    // Order Mutalisk to attack Zealot
    world
        .entities
        .get_mut(&muta_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget { target: zealot_id });

    // Step simulation for 60 ticks
    for tick in 0..60 {
        world.step(&empty_bundle(tick));
    }

    let zealot = world
        .entities
        .get(&zealot_id)
        .expect("zealot must be alive");
    assert!(
        zealot.is_holding_position,
        "Zealot must remain in HoldPosition stance"
    );
    let diff = (zealot.position - zealot_pos).length();
    assert!(
        diff < SimScalar::from_f32_lossy(0.01),
        "Zealot on HoldPosition must not have moved from (10.0, 16.0), displacement was {}",
        diff.to_f32()
    );
}

/// Test 3: Idle Unit Flees Undetected Cloaked Attacker.
/// When attacked by an invisible unit without detector coverage, the victim cannot target it and flees.
#[test]
fn test_idle_unit_flees_undetected_cloaked_attacker() {
    let mut world = create_test_world();
    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let marine_type = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let gunship_type = world
        .catalog
        .id_by_slug("cloak_gunship")
        .expect("cloak_gunship");

    let marine_pos = SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(16));
    let gunship_pos = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(16));

    let marine_id = world
        .spawn_unit(marine_type, player_red, marine_pos)
        .expect("spawn marine");

    let gunship_id = world
        .spawn_unit(gunship_type, player_blue, gunship_pos)
        .expect("spawn gunship");

    // Cloak the gunship
    world.entities.get_mut(&gunship_id).unwrap().is_cloaked = true;

    // Gunship attacks Marine
    world
        .entities
        .get_mut(&gunship_id)
        .unwrap()
        .order_queue
        .push_back(UnitOrder::AttackTarget { target: marine_id });

    // Step simulation for 50 ticks. The gunship's double strike can finish the marine
    // first, so the flight is judged on its last living position.
    let mut last_position = marine_pos;
    for tick in 0..50 {
        world.step(&empty_bundle(tick));
        if let Some(marine) = world.entities.get(&marine_id) {
            last_position = marine.position;
        }
    }

    assert!(
        last_position.x < marine_pos.x,
        "Marine must have fled westward away from unseen attacker at x=15: initial x = {}, final x = {}",
        marine_pos.x.to_f32(),
        last_position.x.to_f32()
    );
}

/// Test 4: Idle Unit Flees Psionic Storm Hazard Zone.
/// An idle mobile unit caught in a hazard zone evacuates radially outward outside the storm radius.
#[test]
fn test_idle_unit_flees_psionic_storm_hazard_zone() {
    let mut world = create_test_world();
    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    let roach_type = world
        .catalog
        .id_by_slug("rhino_beetle")
        .expect("rhino_beetle");

    let storm_center = SimVec2::new(SimScalar::from_num(15), SimScalar::from_num(15));
    let storm_radius = SimScalar::from_num(3);

    let roach_id = world
        .spawn_unit(roach_type, player_red, storm_center)
        .expect("spawn roach");

    // Spawn an active Psionic Storm hazard zone
    world.active_hazards.push(ActiveHazardZone {
        id: 1,
        owner: player_blue,
        center: storm_center,
        radius: storm_radius,
        delay_ticks: 0,
        remaining_ticks: 60,
        interval_ticks: 10,
        ticks_until_next_interval: 1,
        damage_per_interval: SimScalar::from_num(10),
        targets_friendly: false,
    });

    // Step simulation for 50 ticks
    for tick in 0..50 {
        world.step(&empty_bundle(tick));
    }

    let roach = world.entities.get(&roach_id).expect("roach must be alive");
    let dist_from_center = roach.position.distance(storm_center);

    assert!(
        dist_from_center > storm_radius,
        "Roach must have evacuated outside storm radius (radius {}, dist {})",
        storm_radius.to_f32(),
        dist_from_center.to_f32()
    );
}
