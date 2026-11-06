//! Reference-value tests for the non-time catalog audit (plan `2026-11-05-sc2-deviation-fixes`
//! package 9, B-47).
//!
//! # Responsibility
//! Holds the `StarCraft` II 5.0.14 values of every non-time field the audit corrected, one table
//! per field kind, and checks the shipped catalog against them through its public API.
//!
//! # How it fits
//! The audit (evidence `2026-11-05-sc2-deviation-fixes` §9) reads SC2Mapster `SC2GameData` at
//! `5444e12f13` through the layer chain `core → liberty → swarm → void → voidmulti`. Each row cites
//! the layer, file and line of the value. A table covers every identity of its kind that has an
//! analogue, so a catalog edit that drifts from the game fails here, not only the rows a commit
//! changed.
//!
//! # Invariants
//! Rows name analogues, never catalog identities (charter C5), except the upgrade rows, which name
//! the catalog's upgrade slugs because upgrades carry no analogue field. Values are compared after
//! the catalog's own fixed-point conversion, to 0.001.

use sim_core::catalog::UnitArchetype;
use sim_types::{AttributeFlags, SimScalar};

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// The analogue of `archetype`: its `sc2_analogue` (units) or `sc2_inspiration` (structures),
/// without the parenthesised role.
fn analogue(archetype: &UnitArchetype) -> &str {
    archetype
        .sc2_analogue()
        .and_then(|analogue| analogue.split(" (").next())
        .unwrap_or_default()
}

fn is_structure(archetype: &UnitArchetype) -> bool {
    archetype.attributes.contains(AttributeFlags::STRUCTURE)
}

/// Checks every archetype of the kind `structures` selects against `rows`, by analogue:
/// `read` gives the archetype's values in the rows' order. Every archetype of that kind must
/// have a row and every row an archetype, except the `exempt` analogues (class E in the audit).
fn check_table<const N: usize>(
    what: &str,
    structures: bool,
    rows: &[(&str, [f32; N], &str)],
    exempt: &[&str],
    read: impl Fn(&UnitArchetype) -> [f32; N],
) -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes() {
        if is_structure(archetype) != structures || exempt.contains(&analogue(archetype)) {
            continue;
        }
        let name = analogue(archetype);
        let Some((_, expected, source)) = rows.iter().find(|(unit, ..)| *unit == name) else {
            return Err(format!("{} ({name}): no {what} row", archetype.slug).into());
        };
        let actual = read(archetype);
        for (index, (&got, &want)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (got - want).abs() < 0.001,
                "{} ({name}): {what}[{index}] is {got}, StarCraft II {want} ({source})",
                archetype.slug
            );
        }
        seen.insert(name.to_owned());
    }
    for (unit, ..) in rows {
        assert!(seen.contains(*unit), "{what}: row {unit} has no archetype");
    }
    Ok(())
}

fn f(value: SimScalar) -> f32 {
    value.to_f32()
}

/// Structure life, shields and life armour (`CUnit` `LifeMax`, `ShieldsMax`, `LifeArmor`).
/// "default 0" marks a field no layer sets.
const SC2_STRUCTURE_VITALS: [(&str, [f32; 3], &str); 49] = [
    (
        "Command Center",
        [1500.0, 0.0, 1.0],
        "liberty UnitData:10367; default 0; liberty UnitData:10368",
    ),
    (
        "Orbital Command",
        [1500.0, 0.0, 1.0],
        "liberty UnitData:10510; default 0; liberty UnitData:10511",
    ),
    (
        "Planetary Fortress",
        [1500.0, 0.0, 2.0],
        "liberty UnitData:10652; default 0; voidmulti UnitData:1262",
    ),
    (
        "Supply Depot",
        [400.0, 0.0, 1.0],
        "swarm UnitData:1951; default 0; liberty UnitData:5654",
    ),
    (
        "Refinery",
        [500.0, 0.0, 1.0],
        "liberty UnitData:5767; default 0; liberty UnitData:5768",
    ),
    (
        "Barracks",
        [1000.0, 0.0, 1.0],
        "liberty UnitData:5829; default 0; liberty UnitData:5830",
    ),
    (
        "Engineering Bay",
        [850.0, 0.0, 1.0],
        "liberty UnitData:5961; default 0; liberty UnitData:5962",
    ),
    (
        "Bunker",
        [400.0, 0.0, 1.0],
        "liberty UnitData:6147; default 0; liberty UnitData:6148",
    ),
    (
        "Sensor Tower",
        [200.0, 0.0, 0.0],
        "liberty UnitData:9443; default 0; default 0",
    ),
    (
        "Missile Turret",
        [250.0, 0.0, 0.0],
        "liberty UnitData:6027; default 0; default 0",
    ),
    (
        "Factory",
        [1250.0, 0.0, 1.0],
        "liberty UnitData:6228; default 0; liberty UnitData:6229",
    ),
    (
        "Ghost Academy",
        [1250.0, 0.0, 1.0],
        "liberty UnitData:9763; default 0; liberty UnitData:9764",
    ),
    (
        "Starport",
        [1300.0, 0.0, 1.0],
        "liberty UnitData:6361; default 0; liberty UnitData:6362",
    ),
    (
        "Armory",
        [750.0, 0.0, 1.0],
        "liberty UnitData:6496; default 0; liberty UnitData:6497",
    ),
    (
        "Fusion Core",
        [750.0, 0.0, 1.0],
        "liberty UnitData:10907; default 0; liberty UnitData:10908",
    ),
    (
        "Tech Lab",
        [400.0, 0.0, 1.0],
        "liberty UnitData:6632; default 0; liberty UnitData:6633",
    ),
    (
        "Reactor",
        [400.0, 0.0, 1.0],
        "liberty UnitData:467; default 0; liberty UnitData:468",
    ),
    (
        "Nexus",
        [1000.0, 1000.0, 1.0],
        "swarm UnitData:1727; swarm UnitData:1729; liberty UnitData:4174",
    ),
    (
        "Pylon",
        [200.0, 200.0, 1.0],
        "liberty UnitData:4324; liberty UnitData:4328; liberty UnitData:4325",
    ),
    (
        "Assimilator",
        [300.0, 300.0, 1.0],
        "voidmulti UnitData:50; voidmulti UnitData:52; liberty UnitData:4113",
    ),
    (
        "Gateway",
        [500.0, 500.0, 1.0],
        "liberty UnitData:4384; liberty UnitData:4388; liberty UnitData:4385",
    ),
    (
        "Forge",
        [400.0, 400.0, 1.0],
        "swarm UnitData:1613; swarm UnitData:1615; liberty UnitData:4527",
    ),
    (
        "Photon Cannon",
        [150.0, 150.0, 1.0],
        "liberty UnitData:11044; liberty UnitData:11048; liberty UnitData:11045",
    ),
    (
        "Shield Battery",
        [200.0, 200.0, 1.0],
        "voidmulti UnitData:1568; voidmulti UnitData:1570; liberty UnitData:1589",
    ),
    (
        "Cybernetics Core",
        [550.0, 550.0, 1.0],
        "liberty UnitData:10807; liberty UnitData:10811; liberty UnitData:10808",
    ),
    (
        "Twilight Council",
        [500.0, 500.0, 1.0],
        "liberty UnitData:4594; liberty UnitData:4598; liberty UnitData:4595",
    ),
    (
        "Stargate",
        [600.0, 600.0, 1.0],
        "liberty UnitData:9630; liberty UnitData:9634; liberty UnitData:9631",
    ),
    (
        "Robotics Facility",
        [450.0, 450.0, 1.0],
        "liberty UnitData:9559; liberty UnitData:9563; liberty UnitData:9560",
    ),
    (
        "Templar Archives",
        [500.0, 500.0, 1.0],
        "liberty UnitData:4657; liberty UnitData:4661; liberty UnitData:4658",
    ),
    (
        "Dark Shrine",
        [500.0, 500.0, 1.0],
        "liberty UnitData:9500; liberty UnitData:9504; liberty UnitData:9501",
    ),
    (
        "Robotics Bay",
        [500.0, 500.0, 1.0],
        "liberty UnitData:9827; liberty UnitData:9831; liberty UnitData:9828",
    ),
    (
        "Fleet Beacon",
        [500.0, 500.0, 1.0],
        "liberty UnitData:9698; liberty UnitData:9702; liberty UnitData:9699",
    ),
    (
        "Hatchery",
        [1500.0, 0.0, 1.0],
        "swarm UnitData:2269; default 0; liberty UnitData:8201",
    ),
    (
        "Lair",
        [2000.0, 0.0, 1.0],
        "swarm UnitData:2273; default 0; liberty UnitData:8283",
    ),
    (
        "Hive",
        [2500.0, 0.0, 1.0],
        "liberty UnitData:8362; default 0; liberty UnitData:8363",
    ),
    (
        "Extractor",
        [500.0, 0.0, 1.0],
        "liberty UnitData:8142; default 0; liberty UnitData:8143",
    ),
    (
        "Spawning Pool",
        [1000.0, 0.0, 1.0],
        "swarm UnitData:1911; default 0; liberty UnitData:8880",
    ),
    (
        "Evolution Chamber",
        [750.0, 0.0, 1.0],
        "liberty UnitData:8437; default 0; liberty UnitData:8438",
    ),
    (
        "Spine Crawler",
        [300.0, 0.0, 2.0],
        "liberty UnitData:8608; default 0; liberty UnitData:8609",
    ),
    (
        "Spore Crawler",
        [300.0, 0.0, 1.0],
        "voidmulti UnitData:1623; default 0; liberty UnitData:8743",
    ),
    (
        "Roach Warren",
        [850.0, 0.0, 1.0],
        "liberty UnitData:11758; default 0; liberty UnitData:11759",
    ),
    (
        "Baneling Nest",
        [850.0, 0.0, 1.0],
        "liberty UnitData:9959; default 0; liberty UnitData:9960",
    ),
    (
        "Hydralisk Den",
        [850.0, 0.0, 1.0],
        "liberty UnitData:8942; default 0; liberty UnitData:8943",
    ),
    (
        "Lurker Den",
        [850.0, 0.0, 1.0],
        "liberty UnitData:654; default 0; liberty UnitData:655",
    ),
    (
        "Spire",
        [850.0, 0.0, 1.0],
        "swarm UnitData:1925; default 0; liberty UnitData:9005",
    ),
    (
        "Greater Spire",
        [1000.0, 0.0, 1.0],
        "liberty UnitData:9075; default 0; liberty UnitData:9076",
    ),
    (
        "Infestation Pit",
        [850.0, 0.0, 1.0],
        "liberty UnitData:2057; default 0; liberty UnitData:2058",
    ),
    (
        "Nydus Network",
        [850.0, 0.0, 1.0],
        "liberty UnitData:9892; default 0; liberty UnitData:9893",
    ),
    (
        "Ultralisk Cavern",
        [850.0, 0.0, 1.0],
        "swarm UnitData:2019; default 0; liberty UnitData:9209",
    ),
];

#[test]
fn b47_structure_vitals_follow_starcraft() -> TestResult {
    check_table(
        "life, shields, armour",
        true,
        &SC2_STRUCTURE_VITALS,
        &[],
        |a| [f(a.max_hp), f(a.max_shields), f(a.life_armor)],
    )
}

/// Structure sight (`CUnit` `Sight`). The Sensor Tower's radar is not modelled (B-69).
const SC2_STRUCTURE_SIGHT: [(&str, [f32; 1], &str); 49] = [
    ("Command Center", [11.0], "liberty UnitData:10410"),
    ("Orbital Command", [11.0], "liberty UnitData:10556"),
    ("Planetary Fortress", [11.0], "liberty UnitData:10694"),
    ("Supply Depot", [9.0], "liberty UnitData:5691"),
    ("Refinery", [9.0], "liberty UnitData:5804"),
    ("Barracks", [9.0], "liberty UnitData:5866"),
    ("Engineering Bay", [9.0], "liberty UnitData:5998"),
    ("Bunker", [10.0], "liberty UnitData:6184"),
    ("Sensor Tower", [12.0], "liberty UnitData:9480"),
    ("Missile Turret", [11.0], "liberty UnitData:6062"),
    ("Factory", [9.0], "liberty UnitData:6266"),
    ("Ghost Academy", [9.0], "liberty UnitData:9801"),
    ("Starport", [9.0], "liberty UnitData:6399"),
    ("Armory", [9.0], "liberty UnitData:6534"),
    ("Fusion Core", [9.0], "liberty UnitData:10943"),
    ("Tech Lab", [9.0], "liberty UnitData:6672"),
    ("Reactor", [9.0], "liberty UnitData:472"),
    ("Nexus", [11.0], "liberty UnitData:4217"),
    ("Pylon", [10.0], "voidmulti UnitData:1300"),
    ("Assimilator", [9.0], "liberty UnitData:4149"),
    ("Gateway", [9.0], "liberty UnitData:4421"),
    ("Forge", [9.0], "liberty UnitData:4563"),
    ("Photon Cannon", [11.0], "liberty UnitData:11081"),
    ("Shield Battery", [9.0], "liberty UnitData:1599"),
    ("Cybernetics Core", [9.0], "liberty UnitData:10844"),
    ("Twilight Council", [9.0], "liberty UnitData:4631"),
    ("Stargate", [9.0], "liberty UnitData:9667"),
    ("Robotics Facility", [9.0], "liberty UnitData:9593"),
    ("Templar Archives", [9.0], "liberty UnitData:4694"),
    ("Dark Shrine", [9.0], "liberty UnitData:9537"),
    ("Robotics Bay", [9.0], "liberty UnitData:9864"),
    ("Fleet Beacon", [9.0], "liberty UnitData:9735"),
    ("Hatchery", [12.0], "voidmulti UnitData:649"),
    ("Lair", [12.0], "voidmulti UnitData:932"),
    ("Hive", [12.0], "liberty UnitData:8404"),
    ("Extractor", [9.0], "liberty UnitData:8177"),
    ("Spawning Pool", [9.0], "liberty UnitData:8914"),
    ("Evolution Chamber", [9.0], "liberty UnitData:8472"),
    ("Spine Crawler", [11.0], "liberty UnitData:8650"),
    ("Spore Crawler", [11.0], "liberty UnitData:8782"),
    ("Roach Warren", [9.0], "liberty UnitData:11793"),
    ("Baneling Nest", [9.0], "liberty UnitData:9994"),
    ("Hydralisk Den", [9.0], "liberty UnitData:8977"),
    ("Lurker Den", [9.0], "liberty UnitData:660"),
    ("Spire", [9.0], "liberty UnitData:9039"),
    ("Greater Spire", [9.0], "liberty UnitData:9110"),
    ("Infestation Pit", [9.0], "liberty UnitData:2063"),
    ("Nydus Network", [9.0], "liberty UnitData:9928"),
    ("Ultralisk Cavern", [9.0], "liberty UnitData:9243"),
];

#[test]
fn b47_structure_sight_follows_starcraft() -> TestResult {
    check_table("sight", true, &SC2_STRUCTURE_SIGHT, &[], |a| {
        [f(a.sight_radius)]
    })
}
