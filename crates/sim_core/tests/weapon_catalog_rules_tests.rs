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

use sim_core::catalog::{AcquireLevel, AcquireTieBreak, PrimaryWeapon, UnitArchetype, UnitCatalog};
use sim_core::{is_within_firing_arc, SimCoreError, SimWorld, UpgradeCatalog, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{AllowedMovement, Attribute, AttributeBonuses, SplashImpact};
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
) -> impl Iterator<Item = &'a UnitArchetype> {
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

/// Every weapon of the shipped archetypes: the ground, air and legacy single slots, and the
/// weapon of [`mode_only_weapon`].
fn shipped_weapons(world: &SimWorld) -> impl Iterator<Item = &WeaponSpec> {
    world.catalog.archetypes().flat_map(own_weapons)
}

/// The weapon a unit without one of its own carries only in a mode (the Lurker analogue, armed
/// only burrowed as `StarCraft` II's `LurkerMPBurrowed`, B-42).
fn mode_only_weapon(archetype: &UnitArchetype) -> Option<&WeaponSpec> {
    if archetype.weapon.is_some() || archetype.ground_weapon.is_some() {
        return None;
    }
    archetype
        .modes
        .iter()
        .find_map(|mode| mode.ground_weapon_override.as_ref()?.as_ref())
}

/// The archetype's own weapons, the weapon of [`mode_only_weapon`] in the `weapon` slot.
fn own_weapons(archetype: &UnitArchetype) -> impl Iterator<Item = &WeaponSpec> {
    [
        archetype.ground_weapon.as_ref(),
        archetype.air_weapon.as_ref(),
        archetype
            .weapon
            .as_ref()
            .or_else(|| mode_only_weapon(archetype)),
    ]
    .into_iter()
    .flatten()
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

/// Plan `2026-11-24-weapon-validation` D-1: an instant weapon has no missile, so a missile
/// validation it authors would be silently meaningless; validation refuses it, naming the
/// field. The first shipped instant weapon is edited, once with each flag cleared.
#[test]
fn an_instant_weapon_authoring_missile_validation_is_refused() {
    for flags in [
        "(weapon: false, teleport: true)",
        "(weapon: true, teleport: false)",
    ] {
        assert_rejected(
            UNITS,
            "projectile_speed: None,",
            &format!("projectile_speed: None,\n                missile_validation: {flags},"),
            "missile_validation",
        );
    }
}

/// P-5 (plan `2026-11-24-weapon-validation` §2.3, D-6): every shipped missile validates both
/// the weapon and teleports in flight (5.0.14's `CEffectLaunchMissile` defaults, `core`
/// EffectData:100–101), except the Brood Lord's, whose three missiles clear `ValidateWeapon`
/// (`liberty` EffectData:2668, :2749, :2780) and validate at impact instead; no instant weapon
/// authors the field. Mode weapons (a siege mode's override) are included.
#[test]
fn missile_validation_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut brood_missiles = 0;
    for archetype in world.catalog.archetypes() {
        let brood = archetype
            .sc2_analogue()
            .is_some_and(|analogue| analogue.starts_with("Brood Lord"));
        let modes = archetype.modes.iter().flat_map(|mode| {
            [
                mode.ground_weapon_override.as_ref(),
                mode.air_weapon_override.as_ref(),
            ]
            .into_iter()
            .flatten()
            .flatten()
        });
        for weapon in own_weapons(archetype).chain(modes) {
            let slug = &archetype.slug;
            let expected = if brood && weapon.projectile_speed.is_some() {
                brood_missiles += 1;
                sim_types::MissileValidation {
                    weapon: false,
                    teleport: true,
                }
            } else {
                sim_types::MissileValidation::default()
            };
            assert_eq!(
                weapon.missile_validation, expected,
                "{slug}'s {} validates as 5.0.14's missiles do",
                weapon.name
            );
        }
    }
    assert!(
        brood_missiles > 0,
        "the Brood Lord analogue's missiles are shipped"
    );
    Ok(())
}

/// V-17 (plan `2026-11-27-launch-and-spell-visibility` D-1): an instant weapon launches no
/// missile, so a launch visibility it authors would be read by nothing; validation refuses it,
/// naming the field. A visible launch reveals its attacker for at least one tick, so a missile
/// weapon authoring a zero reveal is refused too. The first shipped weapon of each kind is edited.
#[test]
fn an_instant_weapon_or_a_zero_reveal_authoring_launch_visibility_is_refused() {
    for visibility in ["Hidden", "Visible(reveal_ticks: 54)"] {
        assert_rejected(
            UNITS,
            "projectile_speed: None,",
            &format!("projectile_speed: None,\n                launch_visibility: {visibility},"),
            "launch_visibility",
        );
    }
    assert_rejected(
        UNITS,
        "projectile_speed: Some(",
        "launch_visibility: Visible(reveal_ticks: 0),\n                projectile_speed: Some(",
        "launch_visibility",
    );
}

/// V-19 (plan `2026-11-27-launch-and-spell-visibility` §2.1 S3, S4, D-1): no shipped weapon
/// authors a launch visibility, so every one reads the engine default of `CEffectLaunchMissile`,
/// a snapshot (`core` EffectData:102), which no 5.0.14 weapon launch overrides (the five
/// launch overrides of the game data are spells). Mode weapons are included.
#[test]
fn launch_visibility_follows_starcraft() -> TestResult {
    assert!(
        !UNITS.contains("launch_visibility:") && !STRUCTURES.contains("launch_visibility:"),
        "no shipped weapon row authors the field"
    );
    let world = flat_world(16)?;
    let mut missiles = 0;
    for archetype in world.catalog.archetypes() {
        let modes = archetype.modes.iter().flat_map(|mode| {
            [
                mode.ground_weapon_override.as_ref(),
                mode.air_weapon_override.as_ref(),
            ]
            .into_iter()
            .flatten()
            .flatten()
        });
        for weapon in own_weapons(archetype).chain(modes) {
            missiles += usize::from(weapon.projectile_speed.is_some());
            assert_eq!(
                weapon.launch_visibility,
                sim_types::AttackVisibility::Snapshot,
                "{}'s {} launches as 5.0.14's missiles do",
                archetype.slug,
                weapon.name
            );
        }
    }
    assert!(missiles > 0, "premise: missile weapons are shipped");
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
        "sight_radius: 11.0",
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
        // B-43: the attack dash unlocks only on an archetype that carries one.
        (
            "ability_slug: \"overclock\"",
            "ability_slug: \"attack_dash\"",
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

/// `StarCraft` II 5.0.14 units with the `CUnit` flag `AlwaysThreatens` among the analogues this
/// catalog models, as the name an archetype's `sc2_analogue` begins with and where the data set
/// it (`SC2GameData` `5444e12f13`, `UnitData`; plan `2026-11-05-sc2-deviation-fixes` WP4, B-7).
/// The Medivac's flag is set in `liberty` and cleared in `swarm` (`UnitData:1704`).
const SC2_ALWAYS_A_THREAT: [(&str, &str); 5] = [
    ("High Templar", "liberty UnitData:3276"),
    ("Raven", "liberty UnitData:5584"),
    ("Lurker", "voidmulti UnitData:971"),
    ("Infestor", "liberty UnitData:10080"),
    ("Viper", "swarm UnitData:2319"),
];

/// The analogues whose `CUnit` `DefaultAcquireLevel` is `Defensive` (the workers); every other
/// unit keeps the `core` default `Offensive` (`core` `UnitData:31`).
const SC2_DEFENSIVE_ACQUIRE: [(&str, &str); 3] = [
    ("SCV", "liberty UnitData:4726"),
    ("Probe", "liberty UnitData:3065"),
    ("Drone", "liberty UnitData:7034"),
];

#[test]
fn b7_threat_flags_and_acquire_levels_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let listed = |table: &[(&str, &str)], archetype: &UnitArchetype| {
        table.iter().any(|(unit, _)| {
            archetype
                .sc2_analogue()
                .and_then(|analogue| analogue.split(" (").next())
                == Some(*unit)
        })
    };
    for (unit, source) in SC2_ALWAYS_A_THREAT.iter().chain(&SC2_DEFENSIVE_ACQUIRE) {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        assert_eq!(
            archetype.always_a_threat,
            listed(&SC2_ALWAYS_A_THREAT, archetype),
            "{}: always_a_threat as StarCraft II's AlwaysThreatens",
            archetype.slug
        );
        let defensive = listed(&SC2_DEFENSIVE_ACQUIRE, archetype);
        assert_eq!(
            archetype.acquire_level == AcquireLevel::Defensive,
            defensive,
            "{}: acquire_level as StarCraft II's DefaultAcquireLevel",
            archetype.slug
        );
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 `AttackTargetPriority` of the analogues that differ from the rule
/// "units 20, structures 11", and where the data set it (`SC2GameData` `5444e12f13`,
/// `UnitData`; plan `2026-11-05-sc2-deviation-fixes` WP4, B-8). Every unit analogue this
/// catalog models has 20 (Liquipedia "Automatic Targeting": all units but cocoons, larvae,
/// unburrowed Widow Mines and Interceptors, none of which is modelled). The Bunker's 19 is the
/// empty Bunker's: the catalog's Bunker carries no passengers, so it is never the filled
/// Bunker of 20. The Spine Crawler's 20 is the rooted one's: the catalog's cannot uproot.
const SC2_STRUCTURE_PRIORITY: [(&str, u8, &str); 6] = [
    ("Planetary Fortress", 20, "liberty UnitData:10660"),
    ("Photon Cannon", 20, "liberty UnitData:11051"),
    ("Spine Crawler", 20, "liberty UnitData:8614"),
    ("Missile Turret", 19, "liberty UnitData:6032"),
    ("Spore Crawler", 19, "liberty UnitData:8748"),
    ("Bunker", 19, "liberty UnitData:6153"),
];

#[test]
fn b8_every_archetype_has_its_starcraft_attack_target_priority() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, source) in SC2_STRUCTURE_PRIORITY {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let is_structure = archetype
            .attributes
            .contains(sim_types::AttributeFlags::STRUCTURE);
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let (expected, source) = SC2_STRUCTURE_PRIORITY
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue)
            .map_or(
                if is_structure {
                    (11, "every other structure")
                } else {
                    (20, "every unit")
                },
                |&(_, priority, source)| (priority, source),
            );
        assert_eq!(
            archetype.attack_target_priority, expected,
            "{} ({analogue:?}): AttackTargetPriority {expected} ({source})",
            archetype.slug
        );
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 units this catalog models whose two weapons are distinct ground and
/// air weapons, with the first `WeaponArray` entry (the primary) an air weapon (`SC2GameData`
/// `5444e12f13`; plan `2026-11-05-sc2-deviation-fixes` WP4, B-10). Thor: `JavelinMissileLaunchers`
/// (`Air` filter, `liberty` WeaponData:216) before `ThorsHammer`; Queen: `AcidSpines` (`Air`,
/// WeaponData:104) before `Talons`. Liquipedia "Automatic Targeting" names the same primaries.
/// Every other modelled unit has one weapon per plane from a single `StarCraft` II weapon, or
/// none, so its primary weapon is `Either`.
const SC2_AIR_PRIMARY: [(&str, &str); 2] = [
    ("Thor", "liberty UnitData:5270"),
    ("Queen", "liberty UnitData:11478"),
];

#[test]
fn b10_primary_weapons_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, source) in SC2_AIR_PRIMARY {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let air_primary = SC2_AIR_PRIMARY.iter().any(|(unit, _)| {
            archetype
                .sc2_analogue()
                .and_then(|analogue| analogue.split(" (").next())
                == Some(*unit)
        });
        let expected = if air_primary {
            PrimaryWeapon::Air
        } else {
            PrimaryWeapon::Either
        };
        assert_eq!(
            archetype.primary_weapon, expected,
            "{}: primary weapon as StarCraft II's WeaponArray",
            archetype.slug
        );
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 `CWeapon` `AcquirePrioritization` of the analogues this catalog models
/// whose weapons set one (`SC2GameData` `5444e12f13`; plan `2026-11-05-sc2-deviation-fixes` WP4,
/// B-11). Every other weapon keeps the engine default, the closest target. Liquipedia also names
/// the Immortal and the unsieged Siege Tank as `ByDistanceFromTarget`, but their 5.0.14 weapons
/// (`PhaseDisruptors`, `90mmCannons`) set nothing, and the data win.
const SC2_ACQUIRE_TIE_BREAK: [(&str, AcquireTieBreak, &str); 3] = [
    (
        "Thor",
        AcquireTieBreak::LeastTurning,
        "ByAngle, liberty WeaponData:217, :230",
    ),
    (
        "Ultralisk",
        AcquireTieBreak::LeastTurning,
        "ByAngle, liberty WeaponData:435, :445",
    ),
    (
        "Colossus",
        AcquireTieBreak::ClosestToPrevious,
        "ByDistanceFromTarget, liberty WeaponData:498, void WeaponData:128",
    ),
];

#[test]
fn b11_acquire_tie_breaks_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, source) in SC2_ACQUIRE_TIE_BREAK {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let expected = SC2_ACQUIRE_TIE_BREAK
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue)
            .map_or(AcquireTieBreak::Closest, |&(_, tie_break, _)| tie_break);
        assert_eq!(
            archetype.acquire_tie_break, expected,
            "{}: tie-break as StarCraft II's AcquirePrioritization",
            archetype.slug
        );
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 strike spacing of the multi-strike weapons this catalog models:
/// `(analogue, strike_interval_ticks, followup_max_distance, source)` (plan
/// `2026-11-05-sc2-deviation-fixes` WP5, B-2; evidence §5.1). The interval is the effect's
/// `PeriodicPeriodArray` in Normal seconds ÷ 1.4 × 25, rounded half up; the distance is the
/// `CValidatorLocationCompareRange` on a later strike. Every weapon of every other archetype lands
/// its strikes together: the Phoenix's two missiles leave on one loop and the Colossus' two
/// strikes expire together; the Carrier's and the Brood Lord's attacks are B-22 and B-21.
const SC2_STRIKE_SPACING: [(&str, u16, Option<f32>, &str); 8] = [
    (
        "Zealot",
        5,
        Some(2.1),
        "PsiBladesBurst 0, 0.28: liberty EffectData:1919; ZealotAttackDamageMaxRange: voidmulti ValidatorData:867",
    ),
    ("Thor", 7, None, "ThorsHammer 0, 0.375: liberty EffectData:2242"),
    ("Viking", 4, Some(15.0), "LanzerTorpedoes 0, 0.21: liberty EffectData:1698; RangeCheckLE15 on each missile: liberty EffectData:1707, ValidatorData:1956"),
    ("Reaper", 2, Some(7.0), "P38ScytheGuassPistolBurst 0, 0.122: liberty EffectData:1803; ReaperAttackDamageMaxRange: voidmulti ValidatorData:877"),
    ("Banshee", 3, Some(15.0), "BacklashRockets 0, 0.15: liberty EffectData:2550, voidmulti EffectData:36; RangeCheckLE15 on each missile: liberty EffectData:2558"),
    ("Liberator", 1, None, "LiberatorMissileBurstPersistent 0.05 repeated: void EffectData:2942"),
    ("Missile Turret", 2, Some(15.0), "LongboltMissile 0, 0.1: liberty EffectData:1779; RangeCheckLE15 on each missile: liberty EffectData:1789"),
    ("Queen", 5, Some(5.0), "TalonsMissileBurst 0, 0.3: swarm EffectData:3038; QueenTalonsAGAttackDamageMaxRange: voidmulti EffectData:649, ValidatorData:872"),
];

#[test]
fn b2_strike_spacing_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, _, source) in SC2_STRIKE_SPACING {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let (interval, distance) = SC2_STRIKE_SPACING
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue)
            .map_or((0, None), |&(_, interval, distance, _)| {
                (interval, distance.map(SimScalar::from_f32_lossy))
            });
        for weapon in own_weapons(archetype) {
            let (interval, distance) = if weapon.attacks_count > 1 {
                (interval, distance)
            } else {
                (0, None)
            };
            assert_eq!(
                (weapon.strike_interval_ticks, weapon.followup_max_distance),
                (interval, distance),
                "{} {}: strike spacing as StarCraft II's effect",
                archetype.slug,
                weapon.name
            );
        }
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 analogues whose multi-strike weapon's burst effect sets the
/// `Channeled` flag: `(analogue, source)` (Q0058, plan `2026-11-05-sc2-deviation-fixes`
/// Amendment 22, package A). No `CEffectCreatePersistent` default sets the flag (`core`
/// EffectData:34-40). The Viking's `LanzerTorpedoes` (`liberty` EffectData:1698), the Banshee's
/// `BacklashRockets` (`liberty` EffectData:2550, `voidmulti` EffectData:36) and the Phoenix's
/// `IonCannons` (`liberty` EffectData:1959) do not set it, so their bursts survive an order.
const SC2_CHANNELED_BURSTS: [(&str, &str); 7] = [
    ("Zealot", "PsiBladesBurst: liberty EffectData:1924"),
    ("Thor", "ThorsHammer: liberty EffectData:2247"),
    (
        "Reaper",
        "P38ScytheGuassPistolBurst: liberty EffectData:1808",
    ),
    (
        "Liberator",
        "LiberatorMissileBurstPersistent: void EffectData:2945",
    ),
    ("Missile Turret", "LongboltMissile: liberty EffectData:1784"),
    (
        "Queen",
        "TalonsMissileBurst: swarm EffectData:3043; TalonsBurst: liberty EffectData:2325",
    ),
    (
        "Colossus",
        "ThermalLancesForward and ThermalLancesReverse: liberty EffectData:2371, :2396",
    ),
];

#[test]
fn q0058_channeled_bursts_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, source) in SC2_CHANNELED_BURSTS {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let listed = SC2_CHANNELED_BURSTS
            .iter()
            .any(|(unit, _)| Some(*unit) == analogue);
        for weapon in own_weapons(archetype) {
            let expected = listed && weapon.attacks_count > 1;
            assert_eq!(
                weapon.channeled, expected,
                "{} {}: channeled as StarCraft II's burst effect",
                archetype.slug, weapon.name
            );
        }
    }
    Ok(())
}

/// One row of [`SC2_ATTRIBUTE_BONUSES`]: analogue, catalog weapon name if the row is for one
/// weapon only, `(attribute, bonus)` pairs, and the source.
type BonusRow = (
    &'static str,
    Option<&'static str>,
    &'static [(Attribute, f32)],
    &'static str,
);

/// `StarCraft` II 5.0.14 `CEffectDamage` `AttributeBonus` of the weapons of the armed analogues
/// this catalog models: `(analogue, weapon, [(attribute, bonus)], source)`, where `weapon` names
/// the catalog weapon a row is for when the analogue's weapons differ (plan
/// `2026-11-05-sc2-deviation-fixes` WP6, B-14; evidence §6.1). A row whose source says "none" is
/// an analogue whose damage effects set no `AttributeBonus` anywhere in the layer chain.
const SC2_ATTRIBUTE_BONUSES: [BonusRow; 41] = [
    ("SCV", None, &[], "none"),
    ("Marine", None, &[], "none"),
    ("Roach", None, &[], "none"),
    ("Zealot", None, &[], "none"),
    (
        "Siege Tank",
        None,
        &[(Attribute::Armored, 10.0)],
        "90mmCannons, liberty EffectData:1666",
    ),
    (
        "Phoenix",
        None,
        &[(Attribute::Light, 5.0)],
        "IonCannonsU, liberty EffectData:1986",
    ),
    (
        "Marauder",
        None,
        &[(Attribute::Armored, 10.0)],
        "PunisherGrenadesU, liberty EffectData:1651",
    ),
    (
        "Reaper",
        None,
        &[],
        "P38ScytheGuassPistol: Light 5 in liberty EffectData:1817, set to 0 by void EffectData:1499; voidmulti EffectData:346 leaves it",
    ),
    (
        "Ghost",
        None,
        &[(Attribute::Light, 10.0)],
        "C10CanisterRifle, liberty EffectData:1634",
    ),
    (
        "Hellion",
        None,
        &[(Attribute::Light, 6.0)],
        "InfernalFlameThrower, liberty EffectData:2607",
    ),
    (
        "Thor",
        Some("High Impact Payload"),
        &[(Attribute::Massive, 10.0)],
        "LanceMissileLaunchersDamage, voidmulti EffectData:749",
    ),
    ("Thor", None, &[], "ThorsHammerDamage: none"),
    (
        "Viking",
        None,
        &[(Attribute::Armored, 4.0)],
        "LanzerTorpedoesDamage, liberty EffectData:1718",
    ),
    ("Liberator", None, &[], "none"),
    ("Banshee", None, &[], "none"),
    ("Probe", None, &[], "none"),
    (
        "Stalker",
        None,
        &[(Attribute::Armored, 5.0)],
        "ParticleDisruptorsU, voidmulti EffectData:583",
    ),
    ("Sentry", None, &[], "none"),
    (
        "Adept",
        None,
        &[(Attribute::Light, 12.0)],
        "AdeptDamage, voidmulti EffectData:722",
    ),
    ("High Templar", None, &[], "none"),
    ("Dark Templar", None, &[], "none"),
    (
        "Immortal",
        None,
        &[(Attribute::Armored, 30.0)],
        "PhaseDisruptors, liberty EffectData:2439",
    ),
    (
        "Colossus",
        None,
        &[(Attribute::Light, 5.0)],
        "ThermalLancesMU, voidmulti EffectData:966",
    ),
    (
        "Void Ray",
        None,
        &[(Attribute::Armored, 4.0)],
        "PrismaticBeamMUx1, liberty EffectData:2050",
    ),
    ("Carrier / Mothership", None, &[], "none"),
    ("Drone", None, &[], "none"),
    ("Zergling", None, &[], "none"),
    (
        "Baneling",
        None,
        &[(Attribute::Light, 19.0)],
        "VolatileBurstU, voidmulti EffectData:1026",
    ),
    ("Ravager", None, &[], "none"),
    ("Hydralisk", None, &[], "none"),
    (
        "Lurker",
        None,
        &[(Attribute::Armored, 10.0)],
        "LurkerMPDamage, voidmulti EffectData:817",
    ),
    ("Mutalisk", None, &[], "none"),
    (
        "Corruptor",
        None,
        &[(Attribute::Massive, 6.0)],
        "ParasiteSporeDamage, swarm EffectData:2062",
    ),
    ("Brood Lord", None, &[], "none"),
    (
        "Ultralisk",
        None,
        &[],
        "KaiserBladesDamage, void EffectData:1168 sets Armored 0",
    ),
    ("Queen", None, &[], "none"),
    ("Planetary Fortress", None, &[], "none"),
    ("Missile Turret", None, &[], "none"),
    ("Photon Cannon", None, &[], "none"),
    (
        "Spine Crawler",
        None,
        &[(Attribute::Armored, 5.0)],
        "ImpalerTentacleU, liberty EffectData:2166",
    ),
    (
        "Spore Crawler",
        None,
        &[(Attribute::Biological, 10.0)],
        "SporeCrawlerU, voidmulti EffectData:870",
    ),
];

#[test]
fn b14_attribute_bonuses_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        for weapon in own_weapons(archetype) {
            let row = SC2_ATTRIBUTE_BONUSES.iter().find(|(unit, name, ..)| {
                Some(*unit) == analogue && name.is_none_or(|name| name == weapon.name)
            });
            let Some((_, _, bonuses, source)) = row else {
                return Err(format!(
                    "{} {}: armed, but its analogue has no row",
                    archetype.slug, weapon.name
                )
                .into());
            };
            let expected = bonuses
                .iter()
                .fold(AttributeBonuses::NONE, |all, &(attribute, bonus)| {
                    all.with(attribute, SimScalar::from_f32_lossy(bonus))
                });
            assert_eq!(
                weapon.attribute_bonuses, expected,
                "{} {}: attribute bonuses as StarCraft II's ({source})",
                archetype.slug, weapon.name
            );
        }
    }
    Ok(())
}

/// B-16: a splash is a list of zones (`CEffectDamage` `AreaArray`), each a radius and the
/// fraction of the strike it deals. Load rejects a splash with no zone, a fraction outside
/// `0 < fraction <= 1`, and radii that do not grow from one zone to the next.
#[test]
fn b16_splash_zones_must_be_usable() {
    let tank_inner = "(radius: 0.4687, fraction: 1.0)";
    for bad in [
        "(radius: 0.4687, fraction: 0.0)",
        "(radius: 0.4687, fraction: 1.5)",
        "(radius: 0.4687, fraction: -0.5)",
    ] {
        assert_rejected(UNITS, tank_inner, bad, "fraction");
    }
    assert_rejected(UNITS, tank_inner, "(radius: -1.0, fraction: 1.0)", "radius");
    assert_rejected(
        UNITS,
        "(radius: 0.7812, fraction: 0.5)",
        "(radius: 0.4687, fraction: 0.5)",
        "radius",
    );
    assert_rejected(
        UNITS,
        "zones: [(radius: 2.2, fraction: 1.0)]",
        "zones: []",
        "zones",
    );
}

/// B-18: a line splash needs at least one search point, and each offset must be a finite
/// fixed-point value.
#[test]
fn b18_line_splash_must_be_usable() {
    let zones = "zones: [(radius: 2.2, fraction: 1.0)]";
    let line = |offsets: &str| {
        format!("{zones}, shape: Line(origin: Attacker, axis: Along, offsets: [{offsets}])")
    };
    assert_rejected(UNITS, zones, &line(""), "offsets");
    assert_rejected(UNITS, zones, &line("1.0, 3000000000.0"), "offsets[1]");
}

/// One row of [`SC2_SPLASH_ZONES`]: the analogue, its `(radius, fraction)` zones and the source.
type SplashRow = (&'static str, &'static [(f32, f32)], &'static str);

/// `StarCraft` II 5.0.14 `CEffectDamage` `AreaArray` of the circular splash weapons this catalog
/// models: `(analogue, [(radius, fraction)], source)` (plan `2026-11-05-sc2-deviation-fixes`
/// package 8a, B-16; evidence §8.1). The Hellion, Lurker, Colossus and Ultralisk areas are lines and an
/// arc in the game, so their zones belong with their shapes (B-18, B-19; packages 8b, 8c).
const SC2_SPLASH_ZONES: [SplashRow; 4] = [
    (
        "Siege Tank",
        &[(0.4687, 1.0), (0.7812, 0.5), (1.25, 0.25)],
        "CrucioShockCannonBlast, liberty EffectData:1676-1678",
    ),
    (
        "Baneling",
        &[(2.2, 1.0)],
        "VolatileBurstU, liberty EffectData:1757",
    ),
    (
        "Liberator",
        &[(1.5, 1.0)],
        "LiberatorMissileDamage, void EffectData:2938",
    ),
    (
        "Planetary Fortress",
        &[(0.5, 1.0), (0.8, 0.75), (1.25, 0.375)],
        "TwinIbiksCannon, liberty EffectData:2540-2542",
    ),
];

/// Analogues whose splash is not a circle in the game; their zones wait for their shape.
const NON_CIRCULAR_SPLASH: [&str; 4] = ["Hellion", "Lurker", "Colossus", "Ultralisk"];

#[test]
fn b16_splash_zones_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut checked = std::collections::BTreeSet::new();
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next())
            .unwrap_or_default();
        let mode_weapons = archetype.modes.iter().flat_map(|mode| {
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .flatten()
                .flatten()
        });
        let weapons = [
            &archetype.weapon,
            &archetype.ground_weapon,
            &archetype.air_weapon,
        ]
        .into_iter()
        .flatten()
        .chain(mode_weapons);
        for weapon in weapons {
            let Some(splash) = &weapon.splash else {
                continue;
            };
            if NON_CIRCULAR_SPLASH.contains(&analogue) {
                continue;
            }
            let Some((_, zones, source)) =
                SC2_SPLASH_ZONES.iter().find(|(unit, ..)| *unit == analogue)
            else {
                return Err(format!(
                    "{} {}: splashes, but its analogue has no row",
                    archetype.slug, weapon.name
                )
                .into());
            };
            let expected: Vec<sim_types::SplashZone> = zones
                .iter()
                .map(|&(radius, fraction)| sim_types::SplashZone {
                    radius: SimScalar::from_f32_lossy(radius),
                    fraction: SimScalar::from_f32_lossy(fraction),
                })
                .collect();
            assert_eq!(
                splash.zones, expected,
                "{} {}: splash zones as StarCraft II's ({source})",
                archetype.slug, weapon.name
            );
            checked.insert(analogue);
        }
    }
    assert_eq!(
        checked.len(),
        SC2_SPLASH_ZONES.len(),
        "every row reaches a shipped weapon"
    );
    Ok(())
}

/// `StarCraft` II 5.0.14 `SearchFlags` `OffsetByUnitRadius` of every splash weapon this catalog
/// models: `(analogue, flag, source)` (Q0063, plan `2026-11-05-sc2-deviation-fixes` Amendment
/// 22, package B). A `CEffectDamage` inherits 1 from `DU_WEAP` (`core` EffectData:128) unless it
/// clears it; a line's searches are `CEffectEnumArea` effects, whose default sets no flag (`core`
/// EffectData:71-77).
const SC2_SPLASH_OFFSET: [(&str, bool, &str); 8] = [
    (
        "Siege Tank",
        false,
        "CrucioShockCannonBlast clears it: liberty EffectData:1680",
    ),
    (
        "Baneling",
        false,
        "VolatileBurstU inherits it from DU_WEAP (liberty EffectData:1753), but its area is \
         centred on the Baneling itself (ImpactLocation CasterUnit, :1759), which has no edge \
         facing an attacker: cleared, plan 2026-11-23-splash-centres D-2 (Q0096)",
    ),
    (
        "Liberator",
        true,
        "LiberatorMissileDamage, parent DU_WEAP_MISSILE: void EffectData:2932",
    ),
    (
        "Ultralisk",
        true,
        "KaiserBladesDamage, parent DU_WEAP: liberty EffectData:2296",
    ),
    (
        "Hellion",
        false,
        "InfernalFlameThrowerE, a CEffectEnumArea: liberty EffectData:2609",
    ),
    (
        "Lurker",
        false,
        "LurkerMPSearch, a CEffectEnumArea: swarm EffectData:3022",
    ),
    (
        "Colossus",
        false,
        "ThermalLancesE, a CEffectEnumArea: liberty EffectData:2412",
    ),
    (
        "Planetary Fortress",
        false,
        "TwinIbiksCannon clears it: liberty EffectData:2545",
    ),
];

#[test]
fn q0063_splash_offset_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut checked = std::collections::BTreeSet::new();
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next())
            .unwrap_or_default();
        let mode_weapons = archetype.modes.iter().flat_map(|mode| {
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .flatten()
                .flatten()
        });
        for weapon in own_weapons(archetype).chain(mode_weapons) {
            let Some(splash) = &weapon.splash else {
                continue;
            };
            let (_, expected, source) = SC2_SPLASH_OFFSET
                .iter()
                .find(|(unit, ..)| *unit == analogue)
                .ok_or_else(|| format!("{} {}: no offset row", archetype.slug, weapon.name))?;
            assert_eq!(
                splash.offset_by_unit_radius, *expected,
                "{} {}: OffsetByUnitRadius as StarCraft II's ({source})",
                archetype.slug, weapon.name
            );
            checked.insert(analogue);
        }
    }
    assert_eq!(
        checked.len(),
        SC2_SPLASH_OFFSET.len(),
        "every row reaches a weapon"
    );
    Ok(())
}

/// `StarCraft` II 5.0.14 centre and opening condition of every splash weapon this catalog
/// models: `(analogue, impact, target radius below which the area opens, source)` (plan
/// `2026-11-23-splash-centres` D-1 to D-5). A `CEffectDamage` impacts `TargetUnitOrPoint` unless
/// it says otherwise (`core` EffectData:66); a line's searches are placed by their persistent.
const SC2_SPLASH_IMPACT: [(&str, SplashImpact, Option<f32>, &str); 8] = [
    (
        "Siege Tank",
        SplashImpact::Target,
        Some(1.25),
        "CrucioShockCannonBlast impacts its target (liberty EffectData:1668-1681); the weapon's \
         CrucioShockCannonSwitch (swarm WeaponData:169-171, EffectData:2183-2187) gives a target \
         of Radius 1.25 or more the area-less CrucioShockCannonDirected (TargetRadiusLarge, \
         liberty ValidatorData:824-833)",
    ),
    (
        "Baneling",
        SplashImpact::Attacker,
        None,
        "VolatileBurstU and VolatileBurstU2 ImpactLocation CasterUnit: liberty EffectData:1759, \
         :1055",
    ),
    (
        "Liberator",
        SplashImpact::Target,
        None,
        "LiberatorMissileDamage, the default: void EffectData:2932",
    ),
    (
        "Ultralisk",
        SplashImpact::Target,
        None,
        "KaiserBladesDamage, the default: liberty EffectData:2296",
    ),
    (
        "Hellion",
        SplashImpact::Target,
        None,
        "a line placed by its persistent: liberty EffectData:2609",
    ),
    (
        "Lurker",
        SplashImpact::Target,
        None,
        "a line placed by its persistent: swarm EffectData:3022",
    ),
    (
        "Colossus",
        SplashImpact::Target,
        None,
        "a line placed by its persistent: liberty EffectData:2412",
    ),
    (
        "Planetary Fortress",
        SplashImpact::Target,
        None,
        "TwinIbiksCannon, the default: liberty EffectData:2533-2546",
    ),
];

#[test]
fn splash_impact_and_condition_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut checked = std::collections::BTreeSet::new();
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next())
            .unwrap_or_default();
        let mode_weapons = archetype.modes.iter().flat_map(|mode| {
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .flatten()
                .flatten()
        });
        for weapon in own_weapons(archetype).chain(mode_weapons) {
            let Some(splash) = &weapon.splash else {
                continue;
            };
            let (_, impact, below, source) = SC2_SPLASH_IMPACT
                .iter()
                .find(|(unit, ..)| *unit == analogue)
                .ok_or_else(|| format!("{} {}: no impact row", archetype.slug, weapon.name))?;
            assert_eq!(
                (splash.impact, splash.target_radius_below),
                (*impact, below.map(SimScalar::from_f32_lossy)),
                "{} {}: ImpactLocation and radius switch as StarCraft II's ({source})",
                archetype.slug,
                weapon.name
            );
            checked.insert(analogue);
        }
    }
    assert_eq!(
        checked.len(),
        SC2_SPLASH_IMPACT.len(),
        "every row reaches a weapon"
    );
    Ok(())
}

/// One row of [`SC2_LINE_SPLASH`]: the analogue, the radius of each search, where the line
/// starts, the direction of its offsets, the offsets, the ticks between searches and the source.
type LineRow = (
    &'static str,
    f32,
    sim_types::SplashLineOrigin,
    sim_types::SplashLineAxis,
    &'static [f32],
    u16,
    &'static str,
);

/// `StarCraft` II 5.0.14 line areas of the weapons this catalog models (plan
/// `2026-11-05-sc2-deviation-fixes` package 8b, B-18; evidence §9.1). Every search deals the
/// whole strike (no `Fraction` is set; `core` EffectData:69).
const SC2_LINE_SPLASH: [LineRow; 3] = [
    (
        "Lurker",
        0.5,
        sim_types::SplashLineOrigin::Attacker,
        sim_types::SplashLineAxis::Along,
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
        // 0.125 Normal s between spines (`PeriodicPeriodArray`, void EffectData:297-307) is
        // 2.23 ticks; the owner's note on Q0064 allows 2 (B-85).
        2,
        "LurkerMP and LurkerMPSearch, swarm EffectData:3010-3019, voidmulti EffectData:809, :821; \
         void EffectData:297-307",
    ),
    (
        "Hellion",
        0.15,
        sim_types::SplashLineOrigin::Attacker,
        sim_types::SplashLineAxis::Along,
        &[
            0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0, 3.25, 3.5, 3.75, 4.0,
            4.25, 4.5, 4.75, 5.0, 5.25, 5.5, 5.75, 6.0, 6.5,
        ],
        0,
        "InfernalFlameThrowerCP and InfernalFlameThrowerE, liberty EffectData:2609-2646, swarm \
         EffectData:2244-2246",
    ),
    (
        "Colossus",
        0.15,
        sim_types::SplashLineOrigin::Target,
        sim_types::SplashLineAxis::Across,
        &[
            -1.25, -1.0, -0.75, -0.5, -0.25, 0.0, 0.25, 0.5, 0.75, 1.0, 1.25,
        ],
        // Instant by the owner's answer to Q0064.
        0,
        "ThermalLancesForward/Reverse and ThermalLancesE, liberty EffectData:2373-2408, :2412; \
         swarm EffectData:2323, :2328",
    ),
];

#[test]
fn b18_line_splash_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut checked = std::collections::BTreeSet::new();
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next())
            .unwrap_or_default();
        let Some((_, radius, origin, axis, offsets, period_ticks, source)) =
            SC2_LINE_SPLASH.iter().find(|(unit, ..)| *unit == analogue)
        else {
            continue;
        };
        let mode_weapons = archetype.modes.iter().flat_map(|mode| {
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .flatten()
                .flatten()
        });
        for weapon in own_weapons(archetype).chain(mode_weapons) {
            let Some(splash) = &weapon.splash else {
                continue;
            };
            let expected = sim_types::SplashSpec {
                zones: vec![sim_types::SplashZone {
                    radius: SimScalar::from_f32_lossy(*radius),
                    fraction: SimScalar::ONE,
                }],
                shape: sim_types::SplashShape::Line(sim_types::SplashLine {
                    origin: *origin,
                    axis: *axis,
                    offsets: offsets
                        .iter()
                        .copied()
                        .map(SimScalar::from_f32_lossy)
                        .collect(),
                    period_ticks: *period_ticks,
                }),
                friendly_fire: false,
                offset_by_unit_radius: false,
                impact: SplashImpact::Target,
                target_radius_below: None,
                // A line's searches are `CEffectEnumArea`s, which do not set `SameCliff`
                // (plan `2026-11-23-splash-cliff-levels` S3, S11 to S13).
                cliff: sim_types::SplashCliff::Any,
                strike_target_on_other_level: false,
            };
            assert_eq!(
                splash, &expected,
                "{} {}: line splash as StarCraft II's ({source})",
                archetype.slug, weapon.name
            );
            checked.insert(analogue);
        }
    }
    assert_eq!(
        checked.len(),
        SC2_LINE_SPLASH.len(),
        "every row reaches a shipped splash weapon"
    );
    Ok(())
}

/// Plan `2026-11-23-splash-cliff-levels` P-5 (§2.3, B-170): every shipped area's cliff rule, by
/// its shape and centre. A circle is a damage effect's own `AreaArray`, which keeps to the level
/// of its search point (`CEffectDamage` `SearchFlags SameCliff 1`, `core` EffectData:67; the
/// siege shot `CrucioShockCannonBlast`, `TwinIbiksCannon`, `KaiserBladesDamage`,
/// `LiberatorMissileDamage`). A line is a persistent's `CEffectEnumArea` searches, which do not
/// set the flag (`core` EffectData:71–77; `InfernalFlameThrowerE`, `ThermalLancesE`,
/// `LurkerMPSearch`). Exactly the attacker-centred circle, the Baneling's burst, strikes a target
/// on another level directly (`BanelingVolatileBurstDirectFallbackSet`, `voidmulti`
/// EffectData:2327–2358, on `CliffLevelNotEqual`, ValidatorData:894–898).
#[test]
fn splash_cliff_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    let mut seen = std::collections::BTreeSet::new();
    let (mut lines, mut circles, mut bursts) = (0, 0, 0);
    for archetype in world.catalog.archetypes() {
        let mode_weapons = archetype.modes.iter().flat_map(|mode| {
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .flatten()
                .flatten()
        });
        for weapon in own_weapons(archetype).chain(mode_weapons) {
            let Some(splash) = &weapon.splash else {
                continue;
            };
            if !seen.insert((archetype.slug.clone(), weapon.name.clone())) {
                continue;
            }
            let label = format!("{} {}", archetype.slug, weapon.name);
            let line = matches!(splash.shape, sim_types::SplashShape::Line(_));
            let burst = splash.impact == SplashImpact::Attacker;
            let expected = if line {
                lines += 1;
                sim_types::SplashCliff::Any
            } else {
                if burst {
                    bursts += 1;
                } else {
                    circles += 1;
                }
                sim_types::SplashCliff::Same
            };
            assert_eq!(splash.cliff, expected, "{label}: the cliff rule");
            assert_eq!(
                splash.strike_target_on_other_level, burst,
                "{label}: the other-level strike"
            );
        }
    }
    assert_eq!(
        (lines, circles, bursts),
        (3, 4, 1),
        "§2.3's eight shipped areas"
    );
    Ok(())
}

/// `StarCraft` II 5.0.14 analogues whose weapon's burst persistent delays its first strike, with
/// the delay in ticks (the first `PeriodicPeriodArray` entry, Normal s ÷ 1.4 × 25, rounded half
/// up; S-5) and its source: `(analogue, delay, source)` (plan `2026-11-23-strike-delays` §2.3,
/// B-81). Every analogue not listed delays nothing: its chain has no persistent, or the first
/// period is 0 — the Zealot's `PsiBladesBurst` (`liberty` EffectData:1926), the Reaper's
/// `P38ScytheGuassPistolBurst` (:1810), the Thor's `ThorsHammer` (:2250), the Viking's
/// `LanzerTorpedoes` (:1704), the Banshee's `BacklashRockets` (0 since `voidmulti`
/// EffectData:37), the Missile Turret's `LongboltMissile` (:1786), the Queen's
/// `TalonsMissileBurst` (`swarm` EffectData:3045) and `TalonsBurst` (`liberty` :2327), the
/// Phoenix's `IonCannons` (:1965), the Hellion's `InfernalFlameThrowerCP` (:2647), the Lurker's
/// `LurkerMP` (0 since `void` EffectData:298), the Sentry's `DisruptionBeam` (its
/// `InitialEffect` damages on creation, `liberty` :2520) and the Void Ray's `PrismaticBeam`
/// (:2003-2012). The Colossus is listed at 0 by the owner's answer to Q0064 (its strikes and
/// sweep land on the release tick), not by the data. The Carrier's interceptor launch (B-22) is
/// not modelled as a delay (plan §14, F-1).
const SC2_FIRST_STRIKE_DELAY: [(&str, u16, &str); 2] = [
    (
        "Liberator",
        1,
        "LiberatorMissileBurstPersistent PeriodicPeriodArray 0.05 (0.893 ticks): void EffectData:2969; weapon Effect: void WeaponData:328",
    ),
    (
        "Colossus",
        0,
        "the owner's answer to Q0064: the strikes and sweep land on the release tick",
    ),
];

#[test]
fn b81_first_strike_delay_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, source) in SC2_FIRST_STRIKE_DELAY {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let delay = SC2_FIRST_STRIKE_DELAY
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue)
            .map_or(0, |&(_, delay, _)| delay);
        for weapon in own_weapons(archetype) {
            assert_eq!(
                weapon.first_strike_delay_ticks, delay,
                "{} {}: the first strike's delay as StarCraft II's first period",
                archetype.slug, weapon.name
            );
        }
    }
    Ok(())
}

/// `StarCraft` II 5.0.14 weapons whose `AllowedMovement` or `OnlyFireWhileInAttackOrder` differs
/// from the engine defaults, resolved through the override chain with XML comments stripped:
/// `(analogue, allowed_movement, fires_outside_attack_order, source)` (plan
/// `2026-11-23-turret-wind-up` §2.3, B-90, B-45). Every weapon of every other archetype, and every
/// mode's weapon, has the defaults: no `CWeapon` default sets `AllowedMovement` (S2, `core`
/// WeaponData:3–31), so the unit stands to fire (`Stopped`), and `Options
/// OnlyFireWhileInAttackOrder 1` is on the `CWeapon` default (`core` WeaponData:20–21) — among them
/// the Siege Tank's `90mmCannons` and `CrucioShockCannon`, the Hellion's `InfernalFlameThrower` and
/// the Lurker's `LurkerMP` (S8). The Void Ray's options that would clear the attack-order
/// condition are commented out (`void` WeaponData:171–174), so it keeps it.
const SC2_WEAPON_MOVEMENT: [(&str, AllowedMovement, bool, &str); 14] = [
    (
        "Phoenix",
        AllowedMovement::Moving,
        true,
        "IonCannons AllowedMovement Moving: swarm WeaponData:181; OnlyFireWhileInAttackOrder 0: swarm WeaponData:177",
    ),
    (
        "Void Ray",
        AllowedMovement::Moving,
        false,
        "VoidRaySwarm AllowedMovement Moving: void WeaponData:176; the options are commented out: void WeaponData:171-174",
    ),
    (
        "Immortal",
        AllowedMovement::Slowing,
        true,
        "PhaseDisruptors AllowedMovement Slowing: voidmulti WeaponData:207; OnlyFireWhileInAttackOrder 0: balancemulti WeaponData:185",
    ),
    (
        "Colossus",
        AllowedMovement::Stopped,
        true,
        "ThermalLances no AllowedMovement; OnlyFireWhileInAttackOrder 0: balancemulti WeaponData:229",
    ),
    (
        "Baneling",
        AllowedMovement::Moving,
        false,
        "VolatileBurst AllowedMovement Moving: liberty WeaponData:470",
    ),
    (
        "SCV",
        AllowedMovement::Slowing,
        false,
        "FusionCutter AllowedMovement Slowing: voidmulti WeaponData:104",
    ),
    (
        "Probe",
        AllowedMovement::Slowing,
        false,
        "ParticleBeam AllowedMovement Slowing: voidmulti WeaponData:196",
    ),
    (
        "Drone",
        AllowedMovement::Slowing,
        false,
        "Spines AllowedMovement Slowing: voidmulti WeaponData:219",
    ),
    (
        "Viking",
        AllowedMovement::Slowing,
        false,
        "LanzerTorpedoes AllowedMovement Slowing: liberty WeaponData:274",
    ),
    (
        "Liberator",
        AllowedMovement::Slowing,
        false,
        "LiberatorMissileLaunchers AllowedMovement Slowing: void WeaponData:329",
    ),
    (
        "Banshee",
        AllowedMovement::Slowing,
        false,
        "BacklashRockets AllowedMovement Slowing: liberty WeaponData:557",
    ),
    (
        "Mutalisk",
        AllowedMovement::Slowing,
        false,
        "GlaiveWurm AllowedMovement Slowing: liberty WeaponData:406",
    ),
    (
        "Corruptor",
        AllowedMovement::Slowing,
        false,
        "ParasiteSpore AllowedMovement Slowing: liberty WeaponData:569",
    ),
    (
        "Brood Lord",
        AllowedMovement::Slowing,
        false,
        "BroodlingStrike AllowedMovement Slowing: liberty WeaponData:418",
    ),
];

/// The weapons an archetype's modes put in place of its own (the sieged tank's, the burrowed
/// Lurker's).
fn mode_weapons(archetype: &UnitArchetype) -> impl Iterator<Item = &WeaponSpec> {
    archetype.modes.iter().flat_map(|mode| {
        [&mode.ground_weapon_override, &mode.air_weapon_override]
            .into_iter()
            .filter_map(|weapon| weapon.as_ref()?.as_ref())
    })
}

#[test]
fn b90_weapon_movement_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, _, source) in SC2_WEAPON_MOVEMENT {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    let defaults = (AllowedMovement::Stopped, false);
    let default_source = "core WeaponData:20-21, no AllowedMovement";
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let listed = SC2_WEAPON_MOVEMENT
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue);
        let (expected, source) =
            listed.map_or((defaults, default_source), |row| ((row.1, row.2), row.3));
        // A mode's weapon is another 5.0.14 weapon; no modelled one differs from the defaults.
        let checks = own_weapons(archetype)
            .map(|weapon| (weapon, expected, source))
            .chain(mode_weapons(archetype).map(|weapon| (weapon, defaults, default_source)));
        for (weapon, expected, source) in checks {
            assert_eq!(
                (weapon.allowed_movement, weapon.fires_outside_attack_order),
                expected,
                "{} {}: AllowedMovement and OnlyFireWhileInAttackOrder as StarCraft II's weapon \
                 ({source})",
                archetype.slug,
                weapon.name,
            );
        }
    }
    Ok(())
}

/// P-11 (plan `2026-11-23-vision-and-detection` §2.3): 5.0.14 sets `Visibility Visible` on
/// every damage effect of a unit on the air plane (air units and the Colossus, `liberty`
/// EffectData:2421, :1715, :2229, :2589, :2788, :2566, :1984, :125, `void` :1807, :2935,
/// `voidmulti` :2264, …), each revealing its attacker for `AttackReveal`'s 3 Normal s (`core`
/// BehaviorData:237–240), 54 whole ticks; every other weapon keeps the `Snapshot` default, and no
/// shipped weapon is `Hidden`.
#[test]
fn attack_visibility_follows_starcraft() -> TestResult {
    let catalog =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), UNITS, STRUCTURES)?;
    let mut visible = 0;
    let air_plane = |mover: MoverType| matches!(mover, MoverType::Air | MoverType::CliffWalker);
    for arch in catalog.archetypes() {
        let own = [&arch.weapon, &arch.ground_weapon, &arch.air_weapon]
            .into_iter()
            .flatten()
            .map(|weapon| (arch.mover_type, weapon));
        // A mode's weapons fire on the plane the mode puts the unit on.
        let modes = arch.modes.iter().flat_map(|mode| {
            let mover = mode.mover_type_override.unwrap_or(arch.mover_type);
            [&mode.ground_weapon_override, &mode.air_weapon_override]
                .into_iter()
                .filter_map(|weapon| weapon.as_ref().and_then(Option::as_ref))
                .map(move |weapon| (mover, weapon))
        });
        for (mover, weapon) in own.chain(modes) {
            let expected = if air_plane(mover) {
                visible += 1;
                sim_types::AttackVisibility::Visible { reveal_ticks: 54 }
            } else {
                sim_types::AttackVisibility::Snapshot
            };
            assert_eq!(
                weapon.attack_visibility, expected,
                "{} ({mover:?}) {}",
                arch.slug, weapon.name
            );
        }
    }
    assert!(visible > 0, "the shipped catalog has air-plane weapons");
    Ok(())
}

/// `StarCraft` II 5.0.14 weapons whose `OnlyFireAtAttackOrderTarget` or `LockTurretWhileFiring`
/// differs from the engine defaults, resolved through the override chain with XML comments
/// stripped: `(analogue, fires_at_other_targets, locks_turret_while_firing, source)` (plan
/// `2026-11-24-fire-rules` §2.3, B-180, B-179). Every weapon of every other archetype, and every
/// mode's weapon, has the defaults: `Options OnlyFireAtAttackOrderTarget 1` is on the `CWeapon`
/// default (`core` WeaponData:21), and `LockTurretWhileFiring` appears only on the Hellion's
/// `InfernalFlameThrower`. The Void Ray's options that would clear the target condition are
/// commented out (`void` WeaponData:172–173), and the Siege Tank's hidden `90mmCannonsFake`, which
/// clears it, is modelled as a turret capability of the tank's own weapon
/// ([`b198_turret_tracking_follows_starcraft`], plan `2026-11-26-turret-tracking-weapon` D-1).
const SC2_FIRE_RULES: [(&str, bool, bool, &str); 5] = [
    (
        "Phoenix",
        true,
        false,
        "IonCannons OnlyFireAtAttackOrderTarget 0: libertymulti WeaponData:21, swarm WeaponData:178",
    ),
    (
        "Immortal",
        true,
        false,
        "PhaseDisruptors OnlyFireAtAttackOrderTarget 0: voidmulti WeaponData:206, balancemulti \
         WeaponData:186",
    ),
    (
        "Colossus",
        true,
        false,
        "ThermalLances OnlyFireAtAttackOrderTarget 0: voidmulti WeaponData:257, balancemulti \
         WeaponData:230",
    ),
    (
        "Planetary Fortress",
        true,
        false,
        "TwinIbiksCannon OnlyFireAtAttackOrderTarget 0: liberty WeaponData:140",
    ),
    (
        "Hellion",
        false,
        true,
        "InfernalFlameThrower LegacyOptions LockTurretWhileFiring 1: liberty WeaponData:590",
    ),
];

/// P-8 of plan `2026-11-24-fire-rules`: every shipped weapon's `fires_at_other_targets` and
/// `locks_turret_while_firing` equal §2.3's table, the defaults for every unlisted analogue and
/// every mode's weapon.
#[test]
fn b180_b179_fire_rules_follow_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, _, source) in SC2_FIRE_RULES {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    let defaults = (false, false);
    let default_source = "core WeaponData:21, no LockTurretWhileFiring";
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let listed = SC2_FIRE_RULES
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue);
        let (expected, source) =
            listed.map_or((defaults, default_source), |row| ((row.1, row.2), row.3));
        // A mode's weapon is another 5.0.14 weapon; no modelled one differs from the defaults.
        let checks = own_weapons(archetype)
            .map(|weapon| (weapon, expected, source))
            .chain(mode_weapons(archetype).map(|weapon| (weapon, defaults, default_source)));
        for (weapon, expected, source) in checks {
            assert_eq!(
                (
                    weapon.fires_at_other_targets,
                    weapon.locks_turret_while_firing
                ),
                expected,
                "{} {}: OnlyFireAtAttackOrderTarget (inverted) and LockTurretWhileFiring as \
                 StarCraft II's weapon ({source})",
                archetype.slug,
                weapon.name,
            );
        }
    }
    Ok(())
}

/// Where a 5.0.14 unit's turret-tracking behaviour comes from (plan
/// `2026-11-26-turret-tracking-weapon` D-1, Amendment 2 D-A1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackingSource {
    /// A hidden weapon on the same turret that never harms (the Siege Tank's `90mmCannonsFake`).
    HiddenWeapon,
    /// The unit's own weapon, whose attack-order options are cleared while its movement gate
    /// blocks its fire as the unit walks (the Immortal's and the Colossus' weapons, A2.1 S20).
    ClearedOptions,
}

/// `StarCraft` II 5.0.14 units whose turret also aims, under any order, at an enemy its weapon
/// reaches: `(analogue, source of the behaviour, data)` (plan `2026-11-26-turret-tracking-weapon`
/// §2.2, D-6, B-198; Amendment 2 A2.1, B-218). The survey of every `Fake` and `Dummy` weapon of
/// the eight layers finds no other tracking-only weapon on a 5.0.14 multiplayer unit, and of the
/// weapons that clear `OnlyFireWhileInAttackOrder` only `PhaseDisruptors` and `ThermalLances` (and
/// the tank's fake) must slow or stand to fire (S20).
const SC2_TURRET_TRACKING: [(&str, TrackingSource, &str); 3] = [
    (
        "Siege Tank",
        TrackingSource::HiddenWeapon,
        "90mmCannonsFake (Options Hidden 1, OnlyFireAtAttackOrderTarget 0, a CEffectDamage without \
         amount) on Turret SiegeTank beside 90mmCannons: balancemulti WeaponData:7-20, \
         UnitData:1473, EffectData:1066-1072",
    ),
    (
        "Immortal",
        TrackingSource::ClearedOptions,
        "PhaseDisruptors OnlyFireWhileInAttackOrder 0, OnlyFireAtAttackOrderTarget 0, \
         AllowedMovement Slowing: voidmulti WeaponData:205-207 (balancemulti :185-186), on \
         Turret Immortal (liberty UnitData:10791; FreeRotate, YawRate 720, Idle FollowMovement, \
         YawIdleRate 360: liberty TurretData:73-76)",
    ),
    (
        "Colossus",
        TrackingSource::ClearedOptions,
        "ThermalLances OnlyFireWhileInAttackOrder 0, OnlyFireAtAttackOrderTarget 0, no \
         AllowedMovement (stands to fire): voidmulti WeaponData:256-257 (balancemulti :229-230), \
         on Turret Colossus (liberty UnitData:4099; FreeRotate, YawRate 999.8437, Idle \
         FollowMovement, YawIdleRate 90: liberty TurretData:49-53)",
    ),
];

/// T-10 of plan `2026-11-26-turret-tracking-weapon`, extended by Amendment 2's T-A1: exactly three
/// shipped weapons track any enemy with their turret, the tank-mode weapon of the Siege Tank
/// analogue (S1, S4) and the weapons of the Immortal and Colossus analogues (A2.1 S16, S17, S19).
/// The tank's siege mode's weapon, every mode's weapon and every other weapon, structures'
/// included, omit it: `SiegeTankSieged` carries only `CrucioShockCannon` (`liberty`
/// UnitData:5181), and 5.0.14 disables the fake through each morph (`balancemulti`
/// AbilData:772-778, :954-958).
#[test]
fn b198_turret_tracking_follows_starcraft() -> TestResult {
    let world = flat_world(16)?;
    for (unit, _, source) in SC2_TURRET_TRACKING {
        assert!(
            archetypes_with_analogue(&world, unit).next().is_some(),
            "an archetype with the analogue {unit} ({source})"
        );
    }
    let default_source = "no hidden tracking weapon and no cleared attack-order options with a \
                          movement gate on the 5.0.14 unit or mode";
    let mut tracking = std::collections::BTreeSet::new();
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let listed = SC2_TURRET_TRACKING
            .iter()
            .find(|(unit, ..)| Some(*unit) == analogue);
        let (expected, source) = listed.map_or((false, default_source), |row| (true, row.2));
        let checks = own_weapons(archetype)
            .map(|weapon| (weapon, expected, source))
            .chain(mode_weapons(archetype).map(|weapon| (weapon, false, default_source)));
        for (weapon, expected, source) in checks {
            assert_eq!(
                weapon.turret_tracks_any_enemy, expected,
                "{} {}: turret tracking as StarCraft II's unit ({source})",
                archetype.slug, weapon.name,
            );
            if weapon.turret_tracks_any_enemy {
                tracking.insert((archetype.slug.clone(), weapon.name.clone()));
            }
        }
    }
    assert_eq!(
        tracking.len(),
        SC2_TURRET_TRACKING.len(),
        "exactly one shipped weapon per listed unit tracks any enemy: {tracking:?}"
    );
    Ok(())
}

/// T-A2 of plan `2026-11-26-turret-tracking-weapon` Amendment 2 (D-A1: F-1's rule kept as a check
/// on the shipped data, not as a rule of the simulation). Every shipped turreted weapon, modes'
/// included, that may fire outside attack orders (`fires_outside_attack_order`) but not while its
/// unit moves (`allowed_movement` other than `Moving`) carries `turret_tracks_any_enemy`, and
/// every weapon carrying it is either such a weapon or the weapon of a unit listed with a hidden
/// tracking weapon.
#[test]
fn b218_every_weapon_that_stands_to_fire_outside_attack_orders_tracks() -> TestResult {
    let world = flat_world(16)?;
    let mut classed = 0;
    for archetype in world.catalog.archetypes() {
        let analogue = archetype
            .sc2_analogue()
            .and_then(|analogue| analogue.split(" (").next());
        let hidden = SC2_TURRET_TRACKING.iter().any(|(unit, source, _)| {
            Some(*unit) == analogue && *source == TrackingSource::HiddenWeapon
        });
        for weapon in own_weapons(archetype).chain(mode_weapons(archetype)) {
            let stands_outside_orders = weapon.has_turret
                && weapon.fires_outside_attack_order
                && weapon.allowed_movement != AllowedMovement::Moving;
            classed += usize::from(stands_outside_orders);
            if stands_outside_orders {
                assert!(
                    weapon.turret_tracks_any_enemy,
                    "{} {}: a turreted weapon that may fire outside attack orders but not while \
                     moving aims its turret while the unit walks (A2.1 S10, S20)",
                    archetype.slug, weapon.name,
                );
            }
            if weapon.turret_tracks_any_enemy {
                assert!(
                    stands_outside_orders || hidden,
                    "{} {}: tracks without standing to fire outside attack orders or a listed \
                     hidden tracking weapon",
                    archetype.slug,
                    weapon.name,
                );
            }
        }
    }
    assert!(
        classed > 0,
        "premise: a shipped turreted weapon fires outside attack orders but not while moving"
    );
    Ok(())
}

/// T-12 of plan `2026-11-26-turret-tracking-weapon` (D-7): a weapon whose turret tracks any enemy
/// needs a turret, and the refusal names the field; a turreted weapon may track.
#[test]
fn turret_tracking_refusal_names_its_field() -> TestResult {
    assert_rejected(
        UNITS,
        "has_turret: false,",
        "has_turret: false, turret_tracks_any_enemy: true,",
        "turret_tracks_any_enemy",
    );
    let untracked = UNITS.replace("turret_tracks_any_enemy: true,", "");
    let edited = untracked.replacen(
        "has_turret: true,",
        "has_turret: true, turret_tracks_any_enemy: true,",
        1,
    );
    assert_ne!(
        edited, untracked,
        "premise: the shipped catalog has a turret"
    );
    UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &edited, STRUCTURES)?;
    Ok(())
}

/// P-11 of plan `2026-11-24-fire-rules` (D-7): a lock needs a turret to lock, and a weapon that
/// removes its attacker when it fires may not fire at a target its attack order did not name;
/// each refusal names its field.
#[test]
fn fire_rule_refusals_name_their_field() {
    assert_rejected(
        UNITS,
        "has_turret: false,",
        "has_turret: false, locks_turret_while_firing: true,",
        "locks_turret_while_firing",
    );
    assert_rejected(
        UNITS,
        "suicide_on_attack: true,",
        "suicide_on_attack: true, fires_at_other_targets: true,",
        "fires_at_other_targets",
    );
}

/// P-11 of plan `2026-11-24-fire-rules`: a turreted weapon may lock its turret, and any weapon
/// that does not remove its attacker may fire at other targets; both fields may be set together.
#[test]
fn fire_rules_are_accepted_where_they_make_sense() -> TestResult {
    let edited = UNITS.replacen(
        "has_turret: true,",
        "has_turret: true, locks_turret_while_firing: true, fires_at_other_targets: true,",
        1,
    );
    assert_ne!(edited, UNITS, "premise: the shipped catalog has a turret");
    UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &edited, STRUCTURES)?;
    Ok(())
}
