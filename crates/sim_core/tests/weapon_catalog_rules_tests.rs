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
use sim_core::{is_within_firing_arc, SimCoreError, SimWorld, UpgradeCatalog, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{EntityId, MoverType, PlayerId, SimAngle, SimScalar, SimVec2, Tick, TickBundle};

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
    let outcome =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), units, structures)
            .map(|_| ());
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
    UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, STRUCTURES)?;
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
            archetype.mover_type == MoverType::Air && archetype.max_hp > SimScalar::ZERO
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

/// `StarCraft` II 5.0.14 units whose only weapon (in the mode the catalog models) has `TargetFilters`
/// that require `Air`: the unit's name as an archetype's `sc2_analogue` begins with it, the
/// weapon, and where the data say so (`SC2GameData` `5444e12f13`; plan
/// `2026-11-05-sc2-deviation-fixes` WP2, B-25 to B-27).
const SC2_AIR_ONLY_UNITS: [(&str, &str, &str); 3] = [
    (
        "Viking",
        "LanzerTorpedoes",
        "liberty WeaponData:271, UnitData:6885",
    ),
    (
        "Corruptor",
        "ParasiteSpore",
        "liberty WeaponData:567, UnitData:11274",
    ),
    // The ground weapon `LiberatorAGWeapon` belongs to the Defender Mode unit `LiberatorAG`
    // (void UnitData:3387), which the catalog does not model yet (B-27, WP18).
    (
        "Liberator",
        "LiberatorMissileLaunchers",
        "void WeaponData:326, UnitData:3317",
    ),
];

/// The shipped archetypes whose `StarCraft` II analogue is `unit`.
fn archetypes_with_analogue<'a>(
    world: &'a SimWorld,
    unit: &'a str,
) -> impl Iterator<Item = &'a sim_core::catalog::UnitArchetype> {
    world.catalog.archetypes().filter(move |archetype| {
        archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next())
            == Some(unit)
    })
}

#[test]
fn b25_to_b27_units_with_an_air_only_starcraft_weapon_have_no_ground_weapon() -> TestResult {
    let world = flat_world(16)?;
    for (unit, weapon, source) in SC2_AIR_ONLY_UNITS {
        let mut found = 0;
        for archetype in archetypes_with_analogue(&world, unit) {
            found += 1;
            let slug = &archetype.slug;
            for mover in [MoverType::Ground, MoverType::CliffJumper] {
                assert!(
                    archetype.weapon_against(mover, None).is_none(),
                    "{slug} fires at {mover:?} units, but {unit}'s {weapon} targets air only ({source})"
                );
            }
            assert!(
                archetype.weapon_against(MoverType::Air, None).is_some(),
                "{slug} must keep its air weapon ({unit}'s {weapon})"
            );
        }
        assert_eq!(found, 1, "one shipped archetype has the analogue {unit}");
    }
    Ok(())
}

#[test]
fn b25_to_b27_air_only_units_leave_a_ground_unit_unharmed() -> TestResult {
    // A ground unit that cannot shoot back stands in range of each air-only unit for 300 ticks.
    let probe = flat_world(16)?;
    let ground_slug = probe
        .catalog
        .archetypes()
        .find(|archetype| {
            archetype.mover_type == MoverType::Ground
                && archetype.max_hp > SimScalar::ZERO
                && archetype.air_weapon.is_none()
                && !archetype
                    .attributes
                    .contains(sim_types::AttributeFlags::STRUCTURE)
        })
        .map(|archetype| archetype.slug.clone())
        .ok_or("catalog has a ground unit without an air weapon")?;
    for (unit, _, _) in SC2_AIR_ONLY_UNITS {
        let air_slug = archetypes_with_analogue(&probe, unit)
            .next()
            .map(|archetype| archetype.slug.clone())
            .ok_or("an archetype with the analogue")?;
        let mut world = flat_world(32)?;
        spawn_unit(&mut world, &air_slug, 0, SimVec2::from_i32(10, 10))?;
        let ground = spawn_unit(&mut world, &ground_slug, 1, SimVec2::from_i32(13, 10))?;
        let starting_hp = world.entities.get(&ground).ok_or("unit missing")?.hp;
        run(&mut world, 300);
        let hp = world.entities.get(&ground).map(|entity| entity.hp);
        assert_eq!(hp, Some(starting_hp), "{air_slug} damaged {ground_slug}");
    }
    Ok(())
}

/// Every weapon of the shipped archetypes: the ground, air and legacy single slots.
fn shipped_weapons(world: &SimWorld) -> impl Iterator<Item = &WeaponSpec> {
    world.catalog.archetypes().flat_map(|archetype| {
        [
            archetype.ground_weapon.as_ref(),
            archetype.air_weapon.as_ref(),
            archetype.weapon.as_ref(),
        ]
        .into_iter()
        .flatten()
    })
}

#[test]
fn s15_authored_360_degree_arc_is_unrestricted() -> TestResult {
    // Any shipped weapon authored at 180 degrees or more: the loader stores half a turn.
    let world = flat_world(16)?;
    let weapon = shipped_weapons(&world)
        .find(|weapon| weapon.firing_arc == SimAngle::DEG_180)
        .ok_or("catalog has no unrestricted weapon")?;

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
    // Any shipped weapon whose half angle is below a quarter turn refuses a target behind it.
    let world = flat_world(16)?;
    let narrow = shipped_weapons(&world)
        .find(|weapon| weapon.firing_arc < SimAngle::from_degrees(90))
        .ok_or("catalog has no narrow weapon")?;
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
fn s16_projectile_speed_must_be_positive() -> Result<(), Box<dyn std::error::Error>> {
    // The first projectile speed a shipped structure states, whatever its value.
    let structure_speed = STRUCTURES
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("projectile_speed: Some("))
        .ok_or("a shipped structure fires projectiles")?
        .trim_end_matches(',');
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
    Ok(())
}

#[test]
fn s16_weapon_ranges_and_cadence_must_be_usable() -> Result<(), Box<dyn std::error::Error>> {
    // The first firing arc a shipped structure states, whatever its value.
    let structure_arc = STRUCTURES
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("firing_arc_deg: "))
        .ok_or("a shipped structure has a weapon")?
        .trim_end_matches(',');
    for bad in ["firing_arc_deg: 361.0", "firing_arc_deg: -1.0"] {
        assert_rejected(STRUCTURES, structure_arc, bad, "firing_arc_deg");
    }
    assert_rejected(
        UNITS,
        "attacks_count: 1",
        "attacks_count: 0",
        "attacks_count",
    );
    assert_rejected(UNITS, "period_ticks: 15", "period_ticks: 0", "period_ticks");
    assert_rejected(UNITS, "min_range: 0.0", "min_range: 99.0", "min_range");
    assert_rejected(UNITS, "min_range: 0.0", "min_range: -1.0", "min_range");
    Ok(())
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

/// `StarCraft` II 5.0.14 `EnergyStart` of every unit and structure whose analogue has energy, as
/// the name an archetype's `sc2_analogue` begins with, the value, and where the data set it
/// (`SC2GameData` `5444e12f13`, `UnitData`; plan `2026-11-05-sc2-deviation-fixes` WP3, B-34).
/// `EnergyMax` is 200 for each, 100 for the Shield Battery.
const SC2_ENERGY_START: [(&str, f32, &str); 13] = [
    ("Phoenix", 50.0, "liberty UnitData:3727"),
    ("Ghost", 75.0, "voidmulti UnitData:600"),
    ("Medivac", 50.0, "liberty UnitData:5439"),
    ("Banshee", 50.0, "liberty UnitData:5372"),
    ("Raven", 50.0, "liberty UnitData:5633"),
    ("Sentry", 50.0, "liberty UnitData:11296"),
    ("High Templar", 50.0, "liberty UnitData:3250"),
    ("Queen", 25.0, "liberty UnitData:11414"),
    ("Infestor", 75.0, "voidmulti UnitData:831"),
    ("Viper", 50.0, "swarm UnitData:2334"),
    ("Orbital Command", 50.0, "liberty UnitData:10513"),
    ("Nexus", 50.0, "voidmulti UnitData:1096"),
    ("Shield Battery", 50.0, "voidmulti UnitData:1563"),
];

#[test]
fn b34_everything_with_energy_starts_with_its_starcraft_energy() -> TestResult {
    let mut world = flat_world(64)?;
    let mut checked = 0;
    for (index, (unit, start, source)) in (0_u32..).zip(SC2_ENERGY_START) {
        // Each spawns on its own 10-cell square, clear of the others' footprints.
        let (x, y) = (4 + (index % 6) * 10, 4 + (index / 6) * 10);
        let (slug, type_id, is_structure) = archetypes_with_analogue(&world, unit)
            .next()
            .map(|archetype| {
                (
                    archetype.slug.clone(),
                    archetype.unit_type_id,
                    archetype
                        .attributes
                        .contains(sim_types::AttributeFlags::STRUCTURE),
                )
            })
            .ok_or("an archetype with the analogue")?;
        let id = if is_structure {
            world.spawn_structure(type_id, PlayerId(0), x, y)?
        } else {
            let at = SimVec2::from_i32(i32::try_from(x)?, i32::try_from(y)?);
            spawn_unit(&mut world, &slug, 0, at)?
        };
        let energy = world.entities.get(&id).ok_or("spawned")?.energy;
        assert_eq!(
            energy,
            SimScalar::from_num(start),
            "{slug} ({unit}) must start with {start} energy ({source})"
        );
        checked += 1;
    }
    let with_energy = world
        .catalog
        .archetypes()
        .filter(|archetype| archetype.max_energy > SimScalar::ZERO)
        .count();
    assert_eq!(
        checked, with_energy,
        "every archetype with energy is listed"
    );
    Ok(())
}

#[test]
fn b34_start_energy_must_lie_between_zero_and_the_maximum() {
    // The first caster's pool: `max_energy: 200.0` followed by its start energy. A leading 9
    // puts any start energy from 0 to 200 above the maximum (50 reads 950).
    assert_rejected(
        UNITS,
        "max_energy: 200.0,\n            start_energy: ",
        "max_energy: 200.0,\n            start_energy: 9",
        "start_energy",
    );
    assert_rejected(
        UNITS,
        "start_energy: 0.0",
        "start_energy: -1.0",
        "start_energy",
    );
    assert_rejected(
        STRUCTURES,
        "start_energy: 0.0",
        "start_energy: 1.0",
        "start_energy",
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
    let units =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, STRUCTURES)?;
    units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        UPGRADES,
    )?)?;

    let location = "researched_at: \"";
    assert!(UPGRADES.contains(location), "fixture text not in catalog");
    let dangling = UPGRADES.replacen(location, "researched_at: \"no_such_", 1);
    let result = units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &dangling,
    )?);
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
    let bad_units =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, &structures)?;
    let result = bad_units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        UPGRADES,
    )?);
    assert!(
        matches!(result, Err(SimCoreError::CatalogInvalidValue { ref field, .. }) if field == "researches_upgrades"),
        "dangling researches_upgrades must be rejected, got {result:?}"
    );
    Ok(())
}

/// S-11: an upgrade effect the simulation could not apply is a load error.
#[test]
fn s11_upgrade_effects_must_resolve() -> TestResult {
    let units =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, STRUCTURES)?;
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
        let result = units.validate_upgrade_references(&UpgradeCatalog::from_ron_str(
            &sim_fixtures::shipped_factions(),
            &edited,
        )?);
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
        let outcome =
            UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), edited, STRUCTURES)
                .map(|_| ());
        assert!(
            matches!(&outcome, Err(SimCoreError::CatalogInvalidValue { field: rejected, .. }) if rejected.ends_with(field)),
            "{field} must be rejected, got {outcome:?}"
        );
    }
}

/// S-19: gas extraction is carried from structure data into the archetype's capabilities.
#[test]
fn s19_gas_extractor_flag_reaches_capabilities() -> TestResult {
    let units =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, STRUCTURES)?;
    let extractors = units
        .archetypes()
        .filter(|a| a.capabilities.is_gas_extractor)
        .count();
    assert_eq!(extractors, 3, "one extractor per faction");
    Ok(())
}
