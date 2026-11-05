//! # Upgrades and Tech-Tree Integration Tests
//!
//! Verifies end-to-end upgrade scaling math, unit-specific stat modifications,
//! prerequisite structure enforcement, and multi-level upgrade stack calculations.
//!
//! ## Architectural Charter Compliance
//! - **Section 3 Strict Determinism:** Tests verify bit-for-bit identical results on fixed-point math.
//! - **Section 4 Testing Standards:** All 6 textbook scenarios from Plan 11 are verified.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_core::combat::calculate_hit_damage_upgraded;
use sim_core::error::SimCoreError;
use sim_core::world::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::ids::PlayerId;
use sim_types::math::{SimScalar, SimVec2};
use sim_types::UpgradeEffect;

fn create_test_world() -> SimWorld {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    sim_fixtures::world(map, 0x1234_5678)
}

#[test]
fn test_weapon_vs_armor_and_shield_upgrade_scaling() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p1 = PlayerId(1);
    let p2 = PlayerId(2);

    let pulse_bot_id = catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let psi_grey_id = catalog.id_by_slug("psi_grey").expect("psi_grey in catalog");

    let bot_arch = catalog.get(pulse_bot_id).expect("pulse_bot archetype");
    let grey_arch = catalog.get(psi_grey_id).expect("psi_grey archetype");

    // Player 1 researches infantry_weapons_1 (+1 damage)
    world
        .research_upgrade(p1, "infantry_weapons_1")
        .expect("research infantry_weapons_1");

    // Hit 1: pulse_bot (6 base + 1 upgrade = 7 damage) strikes base psi_grey (50 shields, 0 shield armor)
    let p1_upgrades = world
        .player_upgrade_state(p1)
        .expect("invariant: p1 has a player slot")
        .clone();
    let p2_upgrades = world
        .player_upgrade_state(p2)
        .expect("invariant: p2 has a player slot")
        .clone();
    let weapon = bot_arch.weapon.as_ref().expect("pulse_bot weapon");

    let res1 = calculate_hit_damage_upgraded(
        weapon,
        Some(&bot_arch.slug),
        &p1_upgrades,
        grey_arch,
        &p2_upgrades,
        SimScalar::from_num(50.0),
        SimScalar::from_num(100.0),
    );
    // 7 - 0 shield armor = 7 shield damage, 0 life damage
    assert_eq!(res1.shield_damage, SimScalar::from_num(7.0));
    assert_eq!(res1.life_damage, SimScalar::ZERO);
    assert!(!res1.killed);

    // Player 2 researches plasma_shields_1 (+1 shield armor) and ground_armor_1 (+1 ground armor)
    world
        .research_upgrade(p2, "plasma_shields_1")
        .expect("research plasma_shields_1");
    world
        .research_upgrade(p2, "ground_armor_1")
        .expect("research ground_armor_1");

    let p2_upgrades_lvl1 = world
        .player_upgrade_state(p2)
        .expect("invariant: p2 has a player slot")
        .clone();

    // Hit 2: against shields with 1 shield armor: 7 - 1 = 6 shield damage
    let res2 = calculate_hit_damage_upgraded(
        weapon,
        Some(&bot_arch.slug),
        &p1_upgrades,
        grey_arch,
        &p2_upgrades_lvl1,
        SimScalar::from_num(43.0),
        SimScalar::from_num(100.0),
    );
    assert_eq!(res2.shield_damage, SimScalar::from_num(6.0));
    assert_eq!(res2.life_damage, SimScalar::ZERO);

    // Hit 3: when shields are 0 and hit strikes hull:
    // Raw damage = 7. Target life armor: base 1.0 + 1.0 upgrade = 2.0.
    // Hull damage = 7 - 2 = 5.
    let res3 = calculate_hit_damage_upgraded(
        weapon,
        Some(&bot_arch.slug),
        &p1_upgrades,
        grey_arch,
        &p2_upgrades_lvl1,
        SimScalar::ZERO,
        SimScalar::from_num(100.0),
    );
    assert_eq!(res3.shield_damage, SimScalar::ZERO);
    assert_eq!(res3.life_damage, SimScalar::from_num(5.0));
}

#[test]
fn test_combat_shield_hp_bonus() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p1 = PlayerId(1);
    let pulse_bot_id = catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let bot_arch = catalog.get(pulse_bot_id).expect("pulse_bot arch");

    assert_eq!(bot_arch.max_hp, SimScalar::from_num(45.0));

    // Spawn existing bot before upgrade
    let bot_ent_1 = world
        .spawn_unit(
            pulse_bot_id,
            p1,
            SimVec2::new(SimScalar::from_num(10.0), SimScalar::from_num(10.0)),
        )
        .expect("spawn 1");
    assert_eq!(
        world.entities.get(&bot_ent_1).expect("ent 1").max_hp,
        SimScalar::from_num(45.0)
    );

    // Complete combat_shield
    world
        .research_upgrade(p1, "combat_shield")
        .expect("research combat_shield");

    // Existing bot receives +10 HP retroactively
    assert_eq!(
        world.entities.get(&bot_ent_1).expect("ent 1").max_hp,
        SimScalar::from_num(55.0)
    );
    assert_eq!(
        world.entities.get(&bot_ent_1).expect("ent 1").hp,
        SimScalar::from_num(55.0)
    );

    // Newly spawned bot also starts with 55 HP
    let bot_ent_2 = world
        .spawn_unit(
            pulse_bot_id,
            p1,
            SimVec2::new(SimScalar::from_num(12.0), SimScalar::from_num(10.0)),
        )
        .expect("spawn 2");
    assert_eq!(
        world.entities.get(&bot_ent_2).expect("ent 2").max_hp,
        SimScalar::from_num(55.0)
    );
    assert_eq!(
        world.entities.get(&bot_ent_2).expect("ent 2").hp,
        SimScalar::from_num(55.0)
    );

    // Effective max hp helper also returns 55
    assert_eq!(
        world.effective_max_hp(bot_arch, p1),
        SimScalar::from_num(55.0)
    );
}

#[test]
fn test_extended_thermal_lance_range() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p2 = PlayerId(2);
    let colossus_id = catalog
        .id_by_slug("tripod_annihilator")
        .expect("tripod_annihilator in catalog");
    let colossus_arch = catalog.get(colossus_id).expect("colossus arch");
    let weapon = colossus_arch.weapon.as_ref().expect("colossus weapon");

    // Base range is 7.0
    assert_eq!(weapon.max_range, SimScalar::from_num(7.0));
    assert_eq!(
        world.effective_weapon_range(weapon, &colossus_arch.slug, p2),
        SimScalar::from_num(7.0)
    );

    // Research extended_thermal_lance (+2.0 range)
    world
        .research_upgrade(p2, "extended_thermal_lance")
        .expect("research extended_thermal_lance");

    // Effective range is now 9.0
    assert_eq!(
        world.effective_weapon_range(weapon, &colossus_arch.slug, p2),
        SimScalar::from_num(9.0)
    );

    // At distance 8.5: target is in range with upgrade (8.5 <= 9.0)
    let dist = SimScalar::from_num(8.5);
    let in_range = dist <= world.effective_weapon_range(weapon, &colossus_arch.slug, p2);
    assert!(in_range);
}

#[test]
fn test_metabolic_boost_speed() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p3 = PlayerId(3);
    let mantis_id = catalog
        .id_by_slug("razor_mantis")
        .expect("razor_mantis in catalog");
    let mantis_arch = catalog.get(mantis_id).expect("mantis arch");

    // Base speed is 0.1652
    let base_speed = mantis_arch.max_speed;
    assert_eq!(
        world
            .effective_max_speed(mantis_arch, p3)
            .expect("valid fixture effective speed"),
        base_speed
    );

    // Research metabolic_boost: its catalog speed delta for razor_mantis adds to the base speed.
    let boost = sim_fixtures::shipped_catalogs()
        .upgrades
        .get_by_slug("metabolic_boost")
        .expect("metabolic_boost in catalog")
        .effects
        .iter()
        .find_map(|effect| match effect {
            UpgradeEffect::ModifyUnitStat {
                target_slug,
                move_speed_delta,
                ..
            } if target_slug == "razor_mantis" => Some(*move_speed_delta),
            _ => None,
        })
        .expect("metabolic_boost modifies razor_mantis");
    assert!(boost > SimScalar::ZERO, "metabolic_boost must speed up");
    world
        .research_upgrade(p3, "metabolic_boost")
        .expect("research metabolic_boost");

    let effective_speed = world
        .effective_max_speed(mantis_arch, p3)
        .expect("valid fixture effective speed");
    let expected_speed = base_speed + boost;
    assert_eq!(effective_speed, expected_speed);

    // Over 100 ticks, distance is 100 * effective_speed
    let distance_100_ticks = effective_speed * SimScalar::from_num(100.0);
    assert_eq!(
        distance_100_ticks,
        expected_speed * SimScalar::from_num(100.0)
    );
}

#[test]
fn test_tech_tree_prerequisite_validation() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p1 = PlayerId(1);

    // Command dome exists
    let command_dome_id = catalog
        .id_by_slug("foundry_core_dome")
        .expect("foundry_core_dome in catalog");
    let _cmd_ent = world
        .spawn_structure(command_dome_id, p1, 10, 10)
        .expect("spawn command dome");

    // Player 1 attempts to build machining_plant_dome (Factory), requires assembly_bay_dome (Barracks)
    let res = world.can_construct_structure(p1, "machining_plant_dome");
    assert!(matches!(
        res,
        Err(SimCoreError::MissingPrerequisiteStructure { ref target, ref required })
            if target == "machining_plant_dome" && required == "assembly_bay_dome"
    ));

    // Player 1 builds and completes assembly_bay_dome
    let barracks_id = catalog
        .id_by_slug("assembly_bay_dome")
        .expect("assembly_bay_dome in catalog");
    let barracks_ent = world
        .spawn_structure(barracks_id, p1, 20, 20)
        .expect("spawn barracks");
    world
        .entities
        .get_mut(&barracks_ent)
        .expect("barracks")
        .is_completed = true;

    // Now can_construct_structure("machining_plant_dome") succeeds!
    assert!(world
        .can_construct_structure(p1, "machining_plant_dome")
        .is_ok());

    // Player 1 attempts to research infantry_weapons_2 without weapons_arsenal_dome (Armory)
    let res_up = world.can_research_upgrade(p1, "infantry_weapons_2");
    assert!(matches!(
        res_up,
        Err(SimCoreError::MissingPrerequisiteStructure { ref target, ref required })
            if target == "infantry_weapons_2" && required == "weapons_arsenal_dome"
    ));

    // Player 1 constructs weapons_arsenal_dome and completes it
    let armory_id = catalog
        .id_by_slug("weapons_arsenal_dome")
        .expect("weapons_arsenal_dome in catalog");
    let armory_ent = world
        .spawn_structure(armory_id, p1, 30, 30)
        .expect("spawn armory");
    world
        .entities
        .get_mut(&armory_ent)
        .expect("armory")
        .is_completed = true;

    // Now can_research_upgrade("infantry_weapons_2") succeeds!
    assert!(world.can_research_upgrade(p1, "infantry_weapons_2").is_ok());
}

#[test]
fn test_multi_level_upgrade_stacking_3_plus_3() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let p1 = PlayerId(1);
    let p3 = PlayerId(3);

    let tank_id = catalog
        .id_by_slug("railgun_crawler")
        .expect("railgun_crawler in catalog");
    let ultra_id = catalog
        .id_by_slug("stag_behemoth")
        .expect("stag_behemoth in catalog");

    let tank_arch = catalog.get(tank_id).expect("tank arch");
    let ultra_arch = catalog.get(ultra_id).expect("ultra arch");
    let tank_weapon = tank_arch
        .get_mode("siege_mode")
        .and_then(|m| m.ground_weapon_override.as_ref()?.as_ref())
        .expect("tank siege weapon");

    // Player 1 researches vehicle_weapons_1, vehicle_weapons_2, vehicle_weapons_3
    world
        .research_upgrade(p1, "vehicle_weapons_1")
        .expect("research vehicle_weapons_1");
    world
        .research_upgrade(p1, "vehicle_weapons_2")
        .expect("research vehicle_weapons_2");
    world
        .research_upgrade(p1, "vehicle_weapons_3")
        .expect("research vehicle_weapons_3");

    // Player 3 researches ground_carapace_1, ground_carapace_2, ground_carapace_3, and chitinous_plating
    world
        .research_upgrade(p3, "ground_carapace_1")
        .expect("research ground_carapace_1");
    world
        .research_upgrade(p3, "ground_carapace_2")
        .expect("research ground_carapace_2");
    world
        .research_upgrade(p3, "ground_carapace_3")
        .expect("research ground_carapace_3");
    world
        .research_upgrade(p3, "chitinous_plating")
        .expect("research chitinous_plating");

    let p1_upgrades = world
        .player_upgrade_state(p1)
        .expect("invariant: p1 has a player slot")
        .clone();
    let p3_upgrades = world
        .player_upgrade_state(p3)
        .expect("invariant: p3 has a player slot")
        .clone();

    // Damage calculation:
    // Base damage: 40.0 + 4.0 * 3 = 52.0
    // Bonus vs armored: 30.0 + 5.0 * 3 = 45.0
    // Total raw damage = 52.0 + 45.0 = 97.0
    // Ultra armor: base 2.0 + 3.0 (carapace level 3) + 2.0 (chitinous plating stat delta) = 7.0
    // Applied life damage = (97.0 - 7.0).max(0.5) = 90.0!
    let res = calculate_hit_damage_upgraded(
        tank_weapon,
        Some(&tank_arch.slug),
        &p1_upgrades,
        ultra_arch,
        &p3_upgrades,
        SimScalar::ZERO,
        SimScalar::from_num(500.0),
    );

    assert_eq!(res.shield_damage, SimScalar::ZERO);
    assert_eq!(res.life_damage, SimScalar::from_num(90.0));
    assert!(!res.killed);
}

/// An out-of-range owner must not read another player's upgrades (9.4, F-07).
///
/// Player 9 has no storage slot. Before WP11 every lookup clamped it to slot 7 with
/// `.min(7)`, so player 9 silently fought with player 7's research.
#[test]
fn out_of_range_owner_does_not_alias_player_seven_upgrades() {
    let mut world = create_test_world();
    let catalog = sim_fixtures::shipped_catalogs().units;
    let colossus_id = catalog
        .id_by_slug("tripod_annihilator")
        .expect("tripod_annihilator in catalog");
    let colossus_arch = catalog.get(colossus_id).expect("colossus arch");
    let weapon = colossus_arch.weapon.as_ref().expect("colossus weapon");

    world
        .research_upgrade(PlayerId(7), "extended_thermal_lance")
        .expect("research extended_thermal_lance for player 7");
    assert_eq!(
        world.effective_weapon_range(weapon, &colossus_arch.slug, PlayerId(7)),
        weapon.max_range + SimScalar::from_num(2.0)
    );

    assert_eq!(
        world.effective_weapon_range(weapon, &colossus_arch.slug, PlayerId(9)),
        weapon.max_range
    );
    assert_eq!(
        world.effective_weapon_range(weapon, &colossus_arch.slug, PlayerId::NEUTRAL),
        weapon.max_range
    );
}

/// An out-of-range player has no mutable upgrade state to write (9.4, F-07).
#[test]
fn out_of_range_player_has_no_mutable_upgrade_state() {
    let mut world = create_test_world();
    assert!(world.player_upgrade_state_mut(PlayerId(7)).is_some());
    assert!(world.player_upgrade_state_mut(PlayerId(9)).is_none());
    assert!(world.player_upgrade_state(PlayerId(9)).is_none());
    assert!(world.player_upgrade_state_mut(PlayerId::NEUTRAL).is_none());
}
