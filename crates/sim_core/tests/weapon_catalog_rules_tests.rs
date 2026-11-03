//! Behavioural tests for catalog-defined weapon rules and authored-data validation.
//!
//! # Responsibility
//! Verifies that what the catalog says about a weapon reaches the simulation unchanged, and
//! that definitions which would crash or silently misbehave at runtime are rejected at load.
//!
//! # How it fits
//! `UnitCatalog` is the gate between editable RON and deterministic state. Every peer loads
//! the same data, so a bad value that reaches a tick fails on all of them at once.
//!
//! # Invariants
//! Invalid inputs are produced by editing the real shipped catalog text, so each case proves
//! the validator rejects that one field and not a fixture that was already malformed.

use sim_core::catalog::UnitCatalog;
use sim_core::{is_within_firing_arc, SimCoreError, SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{EntityId, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle};

const UNITS: &str = include_str!("../../../assets/catalog/units.ron");
const STRUCTURES: &str = include_str!("../../../assets/catalog/structures.ron");
const UPGRADES: &str = include_str!("../../../assets/catalog/upgrades.ron");

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn flat_world(side: u32) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(side, side, MapCell::default_ground(CliffLevel(0)))?;
    Ok(sim_fixtures::world(map, 7))
}

fn run(world: &mut SimWorld, ticks: u32) {
    for tick in 0..ticks {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: vec![],
            state_checksum: None,
        });
    }
}

fn spawn_unit(
    world: &mut SimWorld,
    slug: &str,
    player: u8,
    position: SimVec2,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit_type = world
        .catalog
        .id_by_slug(slug)
        .ok_or("unit not in catalog")?;
    Ok(world.spawn_unit(unit_type, PlayerId(player), position)?)
}

/// Asserts that replacing `from` in the shipped text produces a rejection naming `field`.
fn assert_rejected(source: &'static str, from: &str, to: &str, field: &str) {
    assert!(
        source.contains(from),
        "fixture text {from:?} not in catalog"
    );
    let edited = source.replacen(from, to, 1);
    let (units, structures) = if std::ptr::eq(source, UNITS) {
        (edited.as_str(), STRUCTURES)
    } else {
        (UNITS, edited.as_str())
    };
    let outcome = UnitCatalog::from_ron_str_bundle(units, structures).map(|_| ());
    assert!(
        matches!(
            &outcome,
            Err(SimCoreError::CatalogInvalidValue { field: rejected, .. })
                if rejected.ends_with(field)
        ),
        "{from:?} -> {to:?} must be rejected naming {field:?}, got {outcome:?}"
    );
}

#[test]
fn shipped_catalog_passes_validation() -> TestResult {
    UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?;
    Ok(())
}

#[test]
fn s3_air_only_structure_does_not_shoot_ground_units() -> TestResult {
    let mut world = flat_world(32)?;
    let dome = world
        .catalog
        .id_by_slug("flak_array_dome")
        .ok_or("dome not in catalog")?;
    world.spawn_structure(dome, PlayerId(0), 10, 10)?;
    let ground = spawn_unit(&mut world, "rhino_beetle", 1, SimVec2::from_i32(16, 11))?;
    let starting_hp = world.entities.get(&ground).ok_or("unit missing")?.hp;

    run(&mut world, 300);

    let hp = world.entities.get(&ground).map(|unit| unit.hp);
    assert_eq!(hp, Some(starting_hp), "air-only dome damaged a ground unit");
    Ok(())
}

#[test]
fn s3_air_only_structure_still_shoots_air_units() -> TestResult {
    let mut world = flat_world(32)?;
    let dome = world
        .catalog
        .id_by_slug("flak_array_dome")
        .ok_or("dome not in catalog")?;
    let air_slug = world
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.mover_type == sim_types::MoverType::Air && archetype.max_hp > SimScalar::ZERO
        })
        .map(|archetype| archetype.slug.clone())
        .ok_or("catalog has no air unit")?;
    world.spawn_structure(dome, PlayerId(0), 10, 10)?;
    let air = spawn_unit(&mut world, &air_slug, 1, SimVec2::from_i32(13, 11))?;
    let starting_hp = world.entities.get(&air).ok_or("unit missing")?.hp;

    run(&mut world, 300);

    assert!(
        world
            .entities
            .get(&air)
            .is_none_or(|unit| unit.hp < starting_hp),
        "air-only dome must still engage air targets"
    );
    Ok(())
}

#[test]
fn s15_authored_360_degree_arc_is_unrestricted() -> TestResult {
    let world = flat_world(16)?;
    let dome = world
        .catalog
        .get_by_slug("flak_array_dome")
        .ok_or("dome not in catalog")?;
    let weapon = dome.air_weapon.as_ref().ok_or("dome has no air weapon")?;

    let origin = SimVec2::from_i32(8, 8);
    for (x, y) in [(9, 8), (8, 9), (7, 8), (8, 7), (7, 7)] {
        assert!(
            is_within_firing_arc(
                SimAngle::ZERO,
                origin,
                SimVec2::from_i32(x, y),
                weapon.firing_arc
            ),
            "360 degree weapon refused a target at ({x}, {y})"
        );
    }
    Ok(())
}

#[test]
fn s15_narrow_arcs_keep_their_authored_half_angle() -> TestResult {
    let world = flat_world(16)?;
    let narrow = world
        .catalog
        .archetypes()
        .filter_map(|archetype| archetype.ground_weapon.as_ref())
        .find(|weapon| weapon.firing_arc == SimAngle::from_degrees(15))
        .ok_or("catalog has no 15 degree weapon")?;
    let origin = SimVec2::from_i32(8, 8);
    assert!(!is_within_firing_arc(
        SimAngle::ZERO,
        origin,
        SimVec2::from_i32(7, 8),
        narrow.firing_arc
    ));
    Ok(())
}

#[test]
fn s16_projectile_speed_must_be_positive() {
    let structure_speed = "projectile_speed: Some(0.80)";
    for bad in [
        "projectile_speed: Some(0.0)",
        "projectile_speed: Some(-1.0)",
    ] {
        assert_rejected(STRUCTURES, structure_speed, bad, "projectile_speed");
    }
    assert_rejected(
        STRUCTURES,
        structure_speed,
        "projectile_speed: Some(1e-12)",
        "projectile_speed",
    );
}

#[test]
fn s16_weapon_ranges_and_cadence_must_be_usable() {
    assert_rejected(
        STRUCTURES,
        "firing_arc_deg: 360.0",
        "firing_arc_deg: 361.0",
        "firing_arc_deg",
    );
    assert_rejected(
        STRUCTURES,
        "firing_arc_deg: 360.0",
        "firing_arc_deg: -1.0",
        "firing_arc_deg",
    );
    assert_rejected(
        UNITS,
        "attacks_count: 1",
        "attacks_count: 0",
        "attacks_count",
    );
    assert_rejected(UNITS, "period_ticks: 15", "period_ticks: 0", "period_ticks");
    assert_rejected(UNITS, "min_range: 0.0", "min_range: 99.0", "min_range");
    assert_rejected(UNITS, "min_range: 0.0", "min_range: -1.0", "min_range");
}

#[test]
fn s16_unit_vitals_and_geometry_must_be_usable() {
    assert_rejected(UNITS, "max_hp: 45.0", "max_hp: 0.0", "max_hp");
    assert_rejected(
        UNITS,
        "collision_radius: 0.375",
        "collision_radius: 0.0",
        "collision_radius",
    );
    assert_rejected(
        UNITS,
        "sight_radius: 9.0",
        "sight_radius: -1.0",
        "sight_radius",
    );
    assert_rejected(UNITS, "acceleration: ", "acceleration: -", "acceleration");
    assert_rejected(
        UNITS,
        "shield_regen_per_tick: 0.0",
        "shield_regen_per_tick: -0.5",
        "shield_regen_per_tick",
    );
}

#[test]
fn f12_catalog_sight_beyond_vision_is_rejected_not_shortened() {
    assert_rejected(
        UNITS,
        "sight_radius: 9.0",
        "sight_radius: 33.0",
        "sight_radius",
    );
    assert_rejected(
        STRUCTURES,
        "sight_radius: 22.0",
        "sight_radius: 33.0",
        "sight_radius",
    );
}

#[test]
fn s16_bundle_resolves_every_upgrade_reference() -> TestResult {
    let units = UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?;
    units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(UPGRADES)?)?;

    let location = "researched_at: \"";
    assert!(UPGRADES.contains(location), "fixture text not in catalog");
    let dangling = UPGRADES.replacen(location, "researched_at: \"no_such_", 1);
    let result = units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(&dangling)?);
    assert!(
        matches!(result, Err(SimCoreError::CatalogInvalidValue { ref field, .. }) if field == "researched_at"),
        "dangling researched_at must be rejected, got {result:?}"
    );

    let structures = STRUCTURES.replacen(
        "researches_upgrades: [\"",
        "researches_upgrades: [\"no_such_",
        1,
    );
    assert_ne!(structures, STRUCTURES, "fixture text not in catalog");
    let bad_units = UnitCatalog::from_ron_str_bundle(UNITS, &structures)?;
    let result = bad_units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(UPGRADES)?);
    assert!(
        matches!(result, Err(SimCoreError::CatalogInvalidValue { ref field, .. }) if field == "researches_upgrades"),
        "dangling researches_upgrades must be rejected, got {result:?}"
    );
    Ok(())
}

/// S-11: an upgrade effect the simulation could not apply is a load error.
#[test]
fn s11_upgrade_effects_must_resolve() -> TestResult {
    let units = UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?;
    for (from, to, field) in [
        (
            "ability_slug: \"overclock\"",
            "ability_slug: \"no_such_ability\"",
            "UnlockAbility.ability_slug",
        ),
        (
            "UnlockAbility(\n                    target_slug: \"",
            "UnlockAbility(\n                    target_slug: \"no_such_",
            "UnlockAbility.target_slug",
        ),
        (
            "ModifyUnitStat(\n                    target_slug: \"",
            "ModifyUnitStat(\n                    target_slug: \"no_such_",
            "ModifyUnitStat.target_slug",
        ),
        ("sight_delta: 0.0", "sight_delta: 40.0", "sight_delta"),
    ] {
        assert!(
            UPGRADES.contains(from),
            "fixture text {from:?} not in catalog"
        );
        let edited = UPGRADES.replacen(from, to, 1);
        let result = units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(&edited)?);
        assert!(
            matches!(result, Err(SimCoreError::CatalogInvalidValue { field: ref rejected, .. }) if rejected == field),
            "{from:?} must be rejected, got {result:?}"
        );
    }
    Ok(())
}

/// S-11: a mode override nothing applies is a load error, not a silent no-op.
#[test]
fn s11_mode_overrides_without_effect_are_rejected() {
    let mode = "mode_name: \"siege_mode\",";
    for (added, field) in [
        ("mover_type_override: Some(Air),", "mover_type_override"),
        (
            "sight_radius_override: Some(12.0),",
            "sight_radius_override",
        ),
        ("air_weapon_override: Some(None),", "air_weapon_override"),
    ] {
        let edited: &'static str = Box::leak(
            UNITS
                .replacen(mode, &format!("{mode} {added}"), 1)
                .into_boxed_str(),
        );
        let outcome = UnitCatalog::from_ron_str_bundle(edited, STRUCTURES).map(|_| ());
        assert!(
            matches!(&outcome, Err(SimCoreError::CatalogInvalidValue { field: rejected, .. }) if rejected.ends_with(field)),
            "{field} must be rejected, got {outcome:?}"
        );
    }
}

/// S-19: gas extraction is carried from structure data into the archetype's capabilities.
#[test]
fn s19_gas_extractor_flag_reaches_capabilities() -> TestResult {
    let units = UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?;
    let extractors = units
        .archetypes()
        .filter(|a| a.capabilities.is_gas_extractor)
        .count();
    assert_eq!(extractors, 3, "one extractor per faction");
    Ok(())
}
