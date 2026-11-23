//! Behavioural tests for bonus damage against any attribute (plan
//! `2026-11-05-sc2-deviation-fixes` WP6, B-14).
//!
//! # Responsibility
//! Pins the rule that a weapon's `attribute_bonuses` add, per strike, the bonus of every
//! attribute the target has, whatever the attribute: `StarCraft` II's `CEffectDamage`
//! `AttributeBonus` array, for example the Corruptor's +6 against Massive (`swarm`
//! EffectData:2062). Before WP6 a weapon could only name an Armored and a Light bonus.
//!
//! # How it fits
//! Scratch archetypes in RON go through `UnitCatalog::from_ron_str`, as a shipped catalog does,
//! and the strike is resolved with the simulation's own damage formula
//! (`calculate_hit_damage`). Catalog validation of the field is checked the same way.
//!
//! # Invariants
//! Every archetype is a scratch archetype written in this file, never a shipped identity (C5),
//! except where the shipped catalog is read whole and its structures found by property (plan
//! `2026-11-23-structure-attributes` P-5). Damage is in damage points per strike against an
//! unshielded target; the scratch targets are unarmoured.

use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_core::{calculate_hit_damage, calculate_hit_damage_upgraded};
use sim_types::{
    Attribute, AttributeBonuses, PlayerUpgradeState, ShapeKind, SimScalar, WeaponSpec,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Base damage of the scratch weapon.
const BASE: i32 = 10;

/// The scratch attacker with a weapon authoring `bonuses` (the RON list body) and three
/// targets: Armored (bits 2), Massive (bits 32), and Armored and Massive (bits 34).
fn catalog_ron(bonuses: &str) -> String {
    let unit = |slug: &str, id: u16, attributes: u16, weapon: &str| {
        format!(
            r#"(
            slug: "{slug}",
            unit_type_id: {id},
            faction: "automaton_directorate",
            display_name: "{slug}",
            lore: "Scratch archetype of attribute_bonus_tests.rs.",
            sc2_analogue: "None",
            showcase_hero: false,
            showcase_order: None,
            shape: UnitSphere,
            silhouette: LightProngs,
            mover_type: Ground,
            attributes: (bits: {attributes}),
            max_hp: 1000.0,
            max_shields: 0.0,
            max_energy: 0.0,
            start_energy: 0.0,
            life_armor: 0.0,
            shield_armor: 0.0,
            hp_regen_per_tick: 0.0,
            shield_regen_per_tick: 0.0,
            energy_regen_per_tick: 0.0,
            max_speed: 0.1,
            acceleration: 0.1,
            deceleration: 0.1,
            turn_rate_deg_per_tick: 180.0,
            collision_radius: 0.5,
            separation_radius: 0.5,
            push_priority: 20,
            sight_radius: 12.0,
            scan_range: 12.0,
            attack_target_priority: 20,
            primary_weapon: Either,
            acquire_tie_break: Closest,
            mineral_cost: 50,
            gas_cost: 0,
            supply_cost: 1,
            supply_provided: 0,
            build_time_ticks: 100,
            weapon: {weapon},
            is_detector: false,
        )"#
        )
    };
    let weapon = format!(
        r#"Some((
            name: "Bonus Rifle",
            visual_kind: Hitscan,
            target_flags: (bits: 255),
            min_range: 0.0,
            max_range: 6.0,
            firing_arc_deg: 360.0,
            acquire_arc_deg: 360.0,
            has_turret: false,
            turret_turn_rate_deg_per_tick: 0.0,
            turret_idle: Hold,
            base_damage: {BASE}.0,
            {bonuses}
            attacks_count: 1,
            first_strike_delay_ticks: 0,
            strike_interval_ticks: 0,
            period_ticks: 20,
            damage_point_ticks: 0,
            backswing_ticks: 0,
            random_delay_ticks: (0.0, 0.0),
            splash: None,
            projectile_speed: None,
        ))"#
    );
    format!(
        "(units: [{}, {}, {}, {}])",
        unit("bonus_attacker", 1, 1, &weapon),
        unit("armored_target", 2, 2, "None"),
        unit("massive_target", 3, 32, "None"),
        unit("armored_massive_target", 4, 34, "None"),
    )
}

/// Loads the scratch catalog whose weapon authors `bonuses`.
fn catalog(bonuses: &str) -> Result<UnitCatalog, Box<dyn std::error::Error>> {
    Ok(UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        &catalog_ron(bonuses),
    )?)
}

/// Damage of one strike of the scratch weapon against the target `slug`.
fn strike(catalog: &UnitCatalog, slug: &str) -> Result<SimScalar, Box<dyn std::error::Error>> {
    let archetype = |slug: &str| -> Result<_, Box<dyn std::error::Error>> {
        Ok(catalog.get(catalog.id_by_slug(slug).ok_or("slug")?)?)
    };
    let weapon = archetype("bonus_attacker")?
        .weapon
        .as_ref()
        .ok_or("weapon")?;
    let target = archetype(slug)?;
    Ok(
        calculate_hit_damage(weapon, target, SimScalar::ZERO, SimScalar::from_i32(1000))
            .life_damage,
    )
}

#[test]
fn a_bonus_against_massive_applies_to_a_massive_target_only() -> TestResult {
    let catalog = catalog("attribute_bonuses: [(Massive, 6.0)],")?;
    assert_eq!(
        strike(&catalog, "massive_target")?,
        SimScalar::from_i32(BASE + 6)
    );
    assert_eq!(
        strike(&catalog, "armored_target")?,
        SimScalar::from_i32(BASE)
    );
    Ok(())
}

#[test]
fn bonuses_against_two_attributes_add_up_on_a_target_with_both() -> TestResult {
    let catalog = catalog("attribute_bonuses: [(Armored, 10.0), (Massive, 6.0)],")?;
    assert_eq!(
        strike(&catalog, "armored_massive_target")?,
        SimScalar::from_i32(BASE + 16)
    );
    assert_eq!(
        strike(&catalog, "armored_target")?,
        SimScalar::from_i32(BASE + 10)
    );
    Ok(())
}

#[test]
fn a_weapon_with_no_bonus_deals_its_base_damage() -> TestResult {
    let catalog = catalog("attribute_bonuses: [],")?;
    assert_eq!(
        strike(&catalog, "armored_massive_target")?,
        SimScalar::from_i32(BASE)
    );
    Ok(())
}

/// Asserts that the scratch weapon authoring `bonuses` fails to load, naming `field`.
fn assert_rejected(bonuses: &str, field: &str) {
    let error = catalog(bonuses).err().map(|error| error.to_string());
    assert!(
        error.as_deref().is_some_and(|error| error.contains(field)),
        "`{bonuses}` must be rejected naming {field}; got {error:?}"
    );
}

#[test]
fn catalog_rejects_a_repeated_or_negative_bonus_and_the_old_fields() {
    assert_rejected(
        "attribute_bonuses: [(Massive, 6.0), (Massive, 2.0)],",
        "attribute_bonuses.Massive",
    );
    assert_rejected("attribute_bonuses: [(Light, -1.0)],", "attribute_bonuses");
    // A weapon written before WP6 names only the two old fields; it must not load as if it had
    // no bonus.
    assert_rejected("bonus_armored: 4.0, bonus_light: 0.0,", "attribute_bonuses");
}

/// Base damage of the scratch weapon struck against the shipped archetypes (plan
/// `2026-11-23-structure-attributes` P-5): large enough that no shipped life armour reaches the
/// minimum clamp, so a bonus shows in full.
const SHIPPED_BASE: i32 = 50;
/// The bonus of the scratch weapon against one attribute at a time (P-5).
const SHIPPED_BONUS: i32 = 7;

/// The scratch weapon with base damage [`SHIPPED_BASE`] and the given bonuses.
fn shipped_probe(bonuses: AttributeBonuses) -> Result<WeaponSpec, Box<dyn std::error::Error>> {
    let catalog = catalog("attribute_bonuses: [],")?;
    let mut weapon = catalog
        .get(catalog.id_by_slug("bonus_attacker").ok_or("slug")?)?
        .weapon
        .clone()
        .ok_or("weapon")?;
    weapon.base_damage = SimScalar::from_i32(SHIPPED_BASE);
    weapon.attribute_bonuses = bonuses;
    Ok(weapon)
}

/// Life damage of one strike of `weapon` against `target`, unshielded and at full life, with no
/// upgrade on either side.
fn life_damage(weapon: &WeaponSpec, target: &UnitArchetype) -> SimScalar {
    let none = PlayerUpgradeState::default();
    calculate_hit_damage_upgraded(
        weapon,
        None,
        &none,
        target,
        &none,
        SimScalar::ZERO,
        target.max_hp,
    )
    .life_damage
}

fn is_structure(archetype: &UnitArchetype) -> bool {
    matches!(archetype.shape, ShapeKind::BuildingDome { .. })
}

/// The attributes each row of the shipped `structures.ron` authors, by slug, read from the text
/// itself so the expectation does not come from the conversion under test.
fn authored_structure_attributes(
) -> Result<std::collections::BTreeMap<String, Vec<Attribute>>, Box<dyn std::error::Error>> {
    let mut authored = std::collections::BTreeMap::new();
    for definition in sim_fixtures::STRUCTURES_RON
        .split("structure_type_id:")
        .skip(1)
    {
        let slug = definition
            .split("slug: \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .ok_or("every structure row has a slug")?;
        let Some(list) = definition
            .lines()
            .find_map(|line| line.trim_start().strip_prefix("attributes: ["))
        else {
            return Err(format!("{slug}: no authored attributes").into());
        };
        let mut attributes = Vec::new();
        for name in list.trim_end_matches("],").split(',').map(str::trim) {
            let attribute = Attribute::ALL
                .into_iter()
                .find(|attribute| format!("{attribute:?}") == name)
                .ok_or_else(|| format!("{slug}: unknown attribute {name}"))?;
            attributes.push(attribute);
        }
        authored.insert(slug.to_owned(), attributes);
    }
    Ok(authored)
}

/// B-88 (P-5): for each of the eight attributes, a bonus against it reaches exactly the shipped
/// structures whose row authors it. Before the fix a Biological bonus missed the Zerg structures
/// and a Psionic bonus hit the Protoss ones, whatever their rows said.
#[test]
fn a_bonus_reaches_exactly_the_structures_that_author_its_attribute() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let authored = authored_structure_attributes()?;
    let plain = shipped_probe(AttributeBonuses::NONE)?;
    let structures: Vec<_> = catalogs
        .units
        .archetypes()
        .filter(|a| is_structure(a))
        .collect();
    assert_eq!(structures.len(), authored.len(), "one archetype per row");
    for attribute in Attribute::ALL {
        let bonused = shipped_probe(
            AttributeBonuses::NONE.with(attribute, SimScalar::from_i32(SHIPPED_BONUS)),
        )?;
        for structure in &structures {
            let row = authored
                .get(&structure.slug)
                .ok_or_else(|| format!("{}: no row", structure.slug))?;
            let difference = life_damage(&bonused, structure) - life_damage(&plain, structure);
            let expected = if row.contains(&attribute) {
                SimScalar::from_i32(SHIPPED_BONUS)
            } else {
                SimScalar::ZERO
            };
            assert_eq!(
                difference, expected,
                "{} ({:?}) authors {row:?}: a bonus against {attribute:?}",
                structure.slug, structure.sc2_inspiration
            );
        }
    }
    Ok(())
}

/// B-88 (P-5): a bonus against `Structure` reaches every shipped structure and no shipped unit.
#[test]
fn a_structure_bonus_reaches_every_structure_and_no_unit() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let plain = shipped_probe(AttributeBonuses::NONE)?;
    let bonused = shipped_probe(
        AttributeBonuses::NONE.with(Attribute::Structure, SimScalar::from_i32(SHIPPED_BONUS)),
    )?;
    for archetype in catalogs.units.archetypes() {
        let difference = life_damage(&bonused, archetype) - life_damage(&plain, archetype);
        let expected = if is_structure(archetype) {
            SimScalar::from_i32(SHIPPED_BONUS)
        } else {
            SimScalar::ZERO
        };
        assert_eq!(difference, expected, "{}", archetype.slug);
    }
    Ok(())
}

/// B-88 (P-5): the shipped structure strike (B-82, found by property) reaches every shipped
/// structure and no shipped unit.
#[test]
fn the_structure_strike_reaches_every_structure_and_no_unit() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let striking = catalogs
        .units
        .archetypes()
        .flat_map(|a| [&a.ground_weapon, &a.air_weapon, &a.weapon])
        .flatten()
        .find(|weapon| weapon.structure_strike.is_some())
        .ok_or("a shipped weapon has a structure strike")?;
    let strike = striking
        .structure_strike
        .ok_or("the weapon found has a structure strike")?;
    let mut without = striking.clone();
    without.structure_strike = None;
    for archetype in catalogs.units.archetypes() {
        let with = life_damage(striking, archetype);
        if is_structure(archetype) {
            assert_eq!(
                with, strike.damage,
                "{}: the structure strike",
                archetype.slug
            );
        } else {
            assert_eq!(
                with,
                life_damage(&without, archetype),
                "{}: no structure strike on a unit",
                archetype.slug
            );
        }
    }
    Ok(())
}
