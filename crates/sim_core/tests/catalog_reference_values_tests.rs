//! Reference-value tests for the non-time catalog audit (plan `2026-11-05-sc2-deviation-fixes`
//! package 9, B-47).
//!
//! # Responsibility
//! Holds the `StarCraft` II 5.0.14 values of every non-time field the audit corrected, one table
//! per field kind, and checks the shipped catalog against them through its public API. Later
//! packages add their sourced fields here too: arcs, mounts and turret idle (WP10), the
//! random firing delay (WP11, B-3), and push priorities (plan `2026-11-09-friendly-blocking`
//! WP5, B-48).
//!
//! # How it fits
//! The audit (evidence `2026-11-05-sc2-deviation-fixes` §10) reads `SC2Mapster` `SC2GameData` at
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
use sim_core::WeaponSpec;
use sim_types::upgrades::UpgradeEffect;
use sim_types::{
    AttackCommand, AttackVisibility, AttributeFlags, PhaseEnd, SimAngle, SimScalar, TurretIdle,
};

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

/// Structure attributes as `AttributeFlags` bits (`CUnit` `Attributes`; the citation is the
/// structure's `Attributes` lines in `liberty` `UnitData`, which no later layer changes; plan
/// `2026-11-23-structure-attributes` §2.3, B-88). Every structure is Armored and Structure;
/// Terran ones add Mechanical, Zerg ones Biological, Protoss ones nothing.
const SC2_STRUCTURE_ATTRIBUTES: [(&str, [f32; 1], &str); 49] = [
    (
        "Command Center",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:10400-10402",
    ),
    (
        "Orbital Command",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:10546-10548",
    ),
    (
        "Planetary Fortress",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:10684-10686",
    ),
    (
        "Supply Depot",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:5681-5683",
    ),
    (
        "Refinery",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:5794-5796",
    ),
    (
        "Barracks",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:5856-5858",
    ),
    (
        "Engineering Bay",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:5988-5990",
    ),
    (
        "Bunker",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6174-6176",
    ),
    (
        "Sensor Tower",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:9470-9472",
    ),
    (
        "Missile Turret",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6052-6054",
    ),
    (
        "Factory",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6256-6258",
    ),
    (
        "Ghost Academy",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:9791-9793",
    ),
    (
        "Starport",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6389-6391",
    ),
    (
        "Armory",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6524-6526",
    ),
    (
        "Fusion Core",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:10933-10935",
    ),
    (
        "Tech Lab",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6662-6664",
    ),
    (
        "Reactor",
        [74.0],
        "Armored+Mechanical+Structure, liberty UnitData:6599-6601",
    ),
    (
        "Nexus",
        [66.0],
        "Armored+Structure, liberty UnitData:4208-4209",
    ),
    (
        "Pylon",
        [66.0],
        "Armored+Structure, liberty UnitData:4353-4354",
    ),
    (
        "Assimilator",
        [66.0],
        "Armored+Structure, liberty UnitData:4140-4141",
    ),
    (
        "Gateway",
        [66.0],
        "Armored+Structure, liberty UnitData:4412-4413",
    ),
    (
        "Forge",
        [66.0],
        "Armored+Structure, liberty UnitData:4554-4555",
    ),
    (
        "Photon Cannon",
        [66.0],
        "Armored+Structure, liberty UnitData:11072-11073",
    ),
    (
        "Shield Battery",
        [66.0],
        "Armored+Structure, liberty UnitData:1585-1586",
    ),
    (
        "Cybernetics Core",
        [66.0],
        "Armored+Structure, liberty UnitData:10835-10836",
    ),
    (
        "Twilight Council",
        [66.0],
        "Armored+Structure, liberty UnitData:4622-4623",
    ),
    (
        "Stargate",
        [66.0],
        "Armored+Structure, liberty UnitData:9658-9659",
    ),
    (
        "Robotics Facility",
        [66.0],
        "Armored+Structure, liberty UnitData:9584-9585",
    ),
    (
        "Templar Archives",
        [66.0],
        "Armored+Structure, liberty UnitData:4685-4686",
    ),
    (
        "Dark Shrine",
        [66.0],
        "Armored+Structure, liberty UnitData:9528-9529",
    ),
    (
        "Robotics Bay",
        [66.0],
        "Armored+Structure, liberty UnitData:9855-9856",
    ),
    (
        "Fleet Beacon",
        [66.0],
        "Armored+Structure, liberty UnitData:9726-9727",
    ),
    (
        "Hatchery",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8232-8234",
    ),
    (
        "Lair",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8314-8316",
    ),
    (
        "Hive",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8394-8396",
    ),
    (
        "Extractor",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8167-8169",
    ),
    (
        "Spawning Pool",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8904-8906",
    ),
    (
        "Evolution Chamber",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8462-8464",
    ),
    (
        "Spine Crawler",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8640-8642",
    ),
    (
        "Spore Crawler",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8772-8774",
    ),
    (
        "Roach Warren",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:11783-11785",
    ),
    (
        "Baneling Nest",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:9984-9986",
    ),
    (
        "Hydralisk Den",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:8967-8969",
    ),
    (
        "Lurker Den",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:650-652",
    ),
    (
        "Spire",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:9029-9031",
    ),
    (
        "Greater Spire",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:9100-9102",
    ),
    (
        "Infestation Pit",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:2046-2048",
    ),
    (
        "Nydus Network",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:9918-9920",
    ),
    (
        "Ultralisk Cavern",
        [70.0],
        "Armored+Biological+Structure, liberty UnitData:9233-9235",
    ),
];

/// B-88 (P-1): every structure authors its analogue's 5.0.14 attributes, not a set derived
/// from its shields.
#[test]
fn b88_structure_attributes_follow_starcraft() -> TestResult {
    check_table(
        "structure attributes",
        true,
        &SC2_STRUCTURE_ATTRIBUTES,
        &[],
        |a| [f32::from(a.attributes.bits())],
    )
}

/// Whether a structure hides its harvesters, as 1 or 0 (5.0.14 `CBehaviorResource`
/// `Flags HideHarvesters` on a behaviour of the structure's `BehaviorArray`; plan
/// `2026-11-24-hidden-harvesters` S1, S2, D-1). Only the three gas extractors carry such a
/// behaviour; no later layer clears the flag.
const SC2_STRUCTURE_HIDES_HARVESTERS: [(&str, [f32; 1], &str); 49] = [
    ("Command Center", [0.0], "no resource behaviour with HideHarvesters"),
    ("Orbital Command", [0.0], "no resource behaviour with HideHarvesters"),
    ("Planetary Fortress", [0.0], "no resource behaviour with HideHarvesters"),
    ("Supply Depot", [0.0], "no resource behaviour with HideHarvesters"),
    ("Refinery", [1.0], "HarvestableVespeneGeyserGas Flags HideHarvesters 1, liberty BehaviorData:824; Refinery BehaviorArray, liberty UnitData:5806"),
    ("Barracks", [0.0], "no resource behaviour with HideHarvesters"),
    ("Engineering Bay", [0.0], "no resource behaviour with HideHarvesters"),
    ("Bunker", [0.0], "no resource behaviour with HideHarvesters"),
    ("Sensor Tower", [0.0], "no resource behaviour with HideHarvesters"),
    ("Missile Turret", [0.0], "no resource behaviour with HideHarvesters"),
    ("Factory", [0.0], "no resource behaviour with HideHarvesters"),
    ("Ghost Academy", [0.0], "no resource behaviour with HideHarvesters"),
    ("Starport", [0.0], "no resource behaviour with HideHarvesters"),
    ("Armory", [0.0], "no resource behaviour with HideHarvesters"),
    ("Fusion Core", [0.0], "no resource behaviour with HideHarvesters"),
    ("Tech Lab", [0.0], "no resource behaviour with HideHarvesters"),
    ("Reactor", [0.0], "no resource behaviour with HideHarvesters"),
    ("Nexus", [0.0], "no resource behaviour with HideHarvesters"),
    ("Pylon", [0.0], "no resource behaviour with HideHarvesters"),
    ("Assimilator", [1.0], "HarvestableVespeneGeyserGasProtoss Flags HideHarvesters 1, liberty BehaviorData:836; Assimilator BehaviorArray, liberty UnitData:4151"),
    ("Gateway", [0.0], "no resource behaviour with HideHarvesters"),
    ("Forge", [0.0], "no resource behaviour with HideHarvesters"),
    ("Photon Cannon", [0.0], "no resource behaviour with HideHarvesters"),
    ("Shield Battery", [0.0], "no resource behaviour with HideHarvesters"),
    ("Cybernetics Core", [0.0], "no resource behaviour with HideHarvesters"),
    ("Twilight Council", [0.0], "no resource behaviour with HideHarvesters"),
    ("Stargate", [0.0], "no resource behaviour with HideHarvesters"),
    ("Robotics Facility", [0.0], "no resource behaviour with HideHarvesters"),
    ("Templar Archives", [0.0], "no resource behaviour with HideHarvesters"),
    ("Dark Shrine", [0.0], "no resource behaviour with HideHarvesters"),
    ("Robotics Bay", [0.0], "no resource behaviour with HideHarvesters"),
    ("Fleet Beacon", [0.0], "no resource behaviour with HideHarvesters"),
    ("Hatchery", [0.0], "no resource behaviour with HideHarvesters"),
    ("Lair", [0.0], "no resource behaviour with HideHarvesters"),
    ("Hive", [0.0], "no resource behaviour with HideHarvesters"),
    ("Extractor", [1.0], "HarvestableVespeneGeyserGasZerg Flags HideHarvesters 1, liberty BehaviorData:848; Extractor BehaviorArray, liberty UnitData:8179"),
    ("Spawning Pool", [0.0], "no resource behaviour with HideHarvesters"),
    ("Evolution Chamber", [0.0], "no resource behaviour with HideHarvesters"),
    ("Spine Crawler", [0.0], "no resource behaviour with HideHarvesters"),
    ("Spore Crawler", [0.0], "no resource behaviour with HideHarvesters"),
    ("Roach Warren", [0.0], "no resource behaviour with HideHarvesters"),
    ("Baneling Nest", [0.0], "no resource behaviour with HideHarvesters"),
    ("Hydralisk Den", [0.0], "no resource behaviour with HideHarvesters"),
    ("Lurker Den", [0.0], "no resource behaviour with HideHarvesters"),
    ("Spire", [0.0], "no resource behaviour with HideHarvesters"),
    ("Greater Spire", [0.0], "no resource behaviour with HideHarvesters"),
    ("Infestation Pit", [0.0], "no resource behaviour with HideHarvesters"),
    ("Nydus Network", [0.0], "no resource behaviour with HideHarvesters"),
    ("Ultralisk Cavern", [0.0], "no resource behaviour with HideHarvesters"),
];

/// B-161 (plan `2026-11-24-hidden-harvesters` P-1): every structure authors whether it hides
/// its harvesters as its analogue does, and no unit hides any.
#[test]
fn b161_structures_hide_harvesters_as_starcraft() -> TestResult {
    check_table(
        "hides_harvesters",
        true,
        &SC2_STRUCTURE_HIDES_HARVESTERS,
        &[],
        |a| [f32::from(u8::from(a.capabilities.hides_harvesters))],
    )?;
    let catalogs = sim_fixtures::shipped_catalogs();
    for archetype in catalogs.units.archetypes().filter(|a| !is_structure(a)) {
        assert!(
            !archetype.capabilities.hides_harvesters,
            "{}: a unit hides no harvesters",
            archetype.slug
        );
    }
    Ok(())
}

/// B-88 (P-2, plan `2026-11-23-structure-attributes` D-3): one meaning of "structure". An
/// archetype has the `Structure` attribute exactly when it has a building shape, and there are
/// exactly as many as the shipped `structures.ron` has rows, so combat's attribute test and the
/// effect filters' shape test agree on every shipped archetype.
#[test]
fn the_structure_attribute_marks_exactly_the_structures() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut structures = 0_usize;
    for archetype in catalogs.units.archetypes() {
        let building = matches!(archetype.shape, sim_types::ShapeKind::BuildingDome { .. });
        assert_eq!(
            archetype.attributes.contains(AttributeFlags::STRUCTURE),
            building,
            "{}: the Structure attribute and the building shape disagree",
            archetype.slug
        );
        structures += usize::from(building);
    }
    let mut rows = sim_core::catalog::UnitCatalog::from_ron_str(
        &sim_fixtures::shipped_factions(),
        "(units: [])",
    )?;
    rows.load_structures_from_ron_str(sim_fixtures::STRUCTURES_RON)?;
    let rows = rows.archetypes().count();
    assert!(rows > 0, "the shipped structures.ron has rows");
    assert_eq!(
        structures, rows,
        "one structure archetype per structures.ron row"
    );
    Ok(())
}

/// Costs the audit corrected: `(analogue, [minerals, gas], source)` (`CUnit` `CostResource`). The other
/// unit and structure costs match the game's build cost (evidence §10.1: a Zerg structure's data cost
/// includes the Drone; morphs and the Zergling pair are B-86).
const SC2_CORRECTED_COSTS: [(&str, [u32; 2], &str); 4] = [
    (
        "Armory",
        [150, 50],
        "liberty UnitData:6502; voidmulti UnitData:2100",
    ),
    ("Dark Shrine", [150, 150], "void UnitData:1914, :1915"),
    (
        "Liberator",
        [150, 125],
        "void UnitData:3305; voidmulti UnitData:468",
    ),
    (
        "Ultralisk",
        [275, 200],
        "voidmulti UnitData:1940; liberty UnitData:8063",
    ),
];

/// Research cost of every upgrade: `(upgrade slug, [minerals, gas], source)`, the research ability's
/// `InfoArray` `Resource` entries.
const SC2_RESEARCH_COSTS: [(&str, [u32; 2], &str); 82] = [
    (
        "infantry_weapons_1",
        [100, 100],
        "liberty AbilData:1700 (EngineeringBayResearch Research3)",
    ),
    (
        "infantry_weapons_2",
        [150, 150],
        "voidmulti AbilData:1811 (EngineeringBayResearch Research4)",
    ),
    (
        "infantry_weapons_3",
        [200, 200],
        "voidmulti AbilData:1815 (EngineeringBayResearch Research5)",
    ),
    (
        "infantry_armor_1",
        [100, 100],
        "liberty AbilData:1722 (EngineeringBayResearch Research7)",
    ),
    (
        "infantry_armor_2",
        [150, 150],
        "voidmulti AbilData:1819 (EngineeringBayResearch Research8)",
    ),
    (
        "infantry_armor_3",
        [200, 200],
        "voidmulti AbilData:1823 (EngineeringBayResearch Research9)",
    ),
    (
        "vehicle_weapons_1",
        [100, 100],
        "liberty AbilData:1875 (ArmoryResearch Research6)",
    ),
    (
        "vehicle_weapons_2",
        [175, 175],
        "liberty AbilData:1880 (ArmoryResearch Research7)",
    ),
    (
        "vehicle_weapons_3",
        [250, 250],
        "liberty AbilData:1885 (ArmoryResearch Research8)",
    ),
    (
        "ship_weapons_1",
        [100, 100],
        "liberty AbilData:1905 (ArmoryResearch Research12)",
    ),
    (
        "ship_weapons_2",
        [175, 175],
        "liberty AbilData:1910 (ArmoryResearch Research13)",
    ),
    (
        "ship_weapons_3",
        [250, 250],
        "liberty AbilData:1915 (ArmoryResearch Research14)",
    ),
    (
        "vehicle_ship_plating_1",
        [100, 100],
        "voidmulti AbilData:26 (ArmoryResearch Research15)",
    ),
    (
        "vehicle_ship_plating_2",
        [175, 175],
        "voidmulti AbilData:31 (ArmoryResearch Research16)",
    ),
    (
        "vehicle_ship_plating_3",
        [250, 250],
        "voidmulti AbilData:36 (ArmoryResearch Research17)",
    ),
    (
        "combat_shield",
        [100, 100],
        "swarm AbilData:984 (BarracksTechLabResearch Research2)",
    ),
    (
        "overclock_stim",
        [100, 100],
        "swarm AbilData:980 (BarracksTechLabResearch Research1)",
    ),
    (
        "concussive_shells",
        [50, 50],
        "swarm AbilData:988 (BarracksTechLabResearch Research3)",
    ),
    (
        "infernal_preigniter",
        [100, 100],
        "voidmulti AbilData:319 (FactoryTechLabResearch Research2)",
    ),
    (
        "drilling_claws",
        [75, 75],
        "voidmulti AbilData:323 (FactoryTechLabResearch Research5)",
    ),
    (
        "smart_servos",
        [100, 100],
        "void AbilData:653 (FactoryTechLabResearch Research7)",
    ),
    (
        "personal_cloaking",
        [150, 150],
        "liberty AbilData:1843 (GhostAcademyResearch Research1)",
    ),
    (
        "cloaking_field",
        [100, 100],
        "voidmulti AbilData:967 (StarportTechLabResearch Research1)",
    ),
    (
        "neosteel_armor",
        [150, 150],
        "liberty AbilData:1695 (EngineeringBayResearch Research2)",
    ),
    (
        "hi_sec_auto_tracking",
        [100, 100],
        "liberty AbilData:1690 (EngineeringBayResearch Research1)",
    ),
    (
        "advanced_ballistics",
        [150, 150],
        "liberty AbilData:3369 (FusionCoreResearch Research2)",
    ),
    (
        "ground_weapons_1",
        [100, 100],
        "liberty AbilData:2102 (ForgeResearch Research1)",
    ),
    (
        "ground_weapons_2",
        [150, 150],
        "swarm AbilData:502 (ForgeResearch Research2)",
    ),
    (
        "ground_weapons_3",
        [200, 200],
        "swarm AbilData:506 (ForgeResearch Research3)",
    ),
    (
        "ground_armor_1",
        [100, 100],
        "liberty AbilData:2117 (ForgeResearch Research4)",
    ),
    (
        "ground_armor_2",
        [150, 150],
        "swarm AbilData:510 (ForgeResearch Research5)",
    ),
    (
        "ground_armor_3",
        [200, 200],
        "swarm AbilData:514 (ForgeResearch Research6)",
    ),
    (
        "plasma_shields_1",
        [150, 150],
        "swarm AbilData:518 (ForgeResearch Research7)",
    ),
    (
        "plasma_shields_2",
        [200, 200],
        "voidmulti AbilData:404 (ForgeResearch Research8)",
    ),
    (
        "plasma_shields_3",
        [250, 250],
        "voidmulti AbilData:408 (ForgeResearch Research9)",
    ),
    (
        "air_weapons_1",
        [100, 100],
        "liberty AbilData:3379 (CyberneticsCoreResearch Research1)",
    ),
    (
        "air_weapons_2",
        [175, 175],
        "liberty AbilData:3384 (CyberneticsCoreResearch Research2)",
    ),
    (
        "air_weapons_3",
        [250, 250],
        "liberty AbilData:3389 (CyberneticsCoreResearch Research3)",
    ),
    (
        "air_armor_1",
        [100, 100],
        "voidmulti AbilData:301 (CyberneticsCoreResearch Research4)",
    ),
    (
        "air_armor_2",
        [175, 175],
        "voidmulti AbilData:305 (CyberneticsCoreResearch Research5)",
    ),
    (
        "air_armor_3",
        [250, 250],
        "voidmulti AbilData:309 (CyberneticsCoreResearch Research6)",
    ),
    (
        "warp_gate_tech",
        [50, 50],
        "liberty AbilData:3409 (CyberneticsCoreResearch Research7)",
    ),
    (
        "charge",
        [100, 100],
        "voidmulti AbilData:1075 (TwilightCouncilResearch Research1)",
    ),
    (
        "phase_blink",
        [150, 150],
        "liberty AbilData:3434 (TwilightCouncilResearch Research2)",
    ),
    (
        "resonating_glaives",
        [100, 100],
        "void AbilData:834 (TwilightCouncilResearch Research3)",
    ),
    (
        "shadow_stride",
        [100, 100],
        "voidmulti AbilData:1319 (DarkShrineResearch Research1)",
    ),
    (
        "psionic_storm",
        [200, 200],
        "liberty AbilData:2184 (TemplarArchivesResearch Research5)",
    ),
    (
        "extended_thermal_lance",
        [150, 150],
        "voidmulti AbilData:865 (RoboticsBayResearch Research6)",
    ),
    (
        "gravitic_boosters",
        [100, 100],
        "liberty AbilData:2151 (RoboticsBayResearch Research2)",
    ),
    (
        "gravitic_drive",
        [100, 100],
        "liberty AbilData:2158 (RoboticsBayResearch Research3)",
    ),
    (
        "anion_pulse_crystals",
        [150, 150],
        "swarm AbilData:1050 (FleetBeaconResearch Research3)",
    ),
    (
        "flux_vanes",
        [100, 100],
        "voidmulti AbilData:381 (FleetBeaconResearch Research5)",
    ),
    (
        "melee_attacks_1",
        [100, 100],
        "liberty AbilData:2247 (evolutionchamberresearch Research1)",
    ),
    (
        "melee_attacks_2",
        [150, 150],
        "liberty AbilData:2252 (evolutionchamberresearch Research2)",
    ),
    (
        "melee_attacks_3",
        [200, 200],
        "liberty AbilData:2257 (evolutionchamberresearch Research3)",
    ),
    (
        "missile_attacks_1",
        [100, 100],
        "liberty AbilData:2277 (evolutionchamberresearch Research7)",
    ),
    (
        "missile_attacks_2",
        [150, 150],
        "liberty AbilData:2282 (evolutionchamberresearch Research8)",
    ),
    (
        "missile_attacks_3",
        [200, 200],
        "liberty AbilData:2287 (evolutionchamberresearch Research9)",
    ),
    (
        "ground_carapace_1",
        [150, 150],
        "liberty AbilData:2262 (evolutionchamberresearch Research4)",
    ),
    (
        "ground_carapace_2",
        [200, 200],
        "voidmulti AbilData:1680 (evolutionchamberresearch Research5)",
    ),
    (
        "ground_carapace_3",
        [250, 250],
        "voidmulti AbilData:1684 (evolutionchamberresearch Research6)",
    ),
    (
        "flyer_attacks_1",
        [100, 100],
        "liberty AbilData:2430 (SpireResearch Research1)",
    ),
    (
        "flyer_attacks_2",
        [175, 175],
        "liberty AbilData:2437 (SpireResearch Research2)",
    ),
    (
        "flyer_attacks_3",
        [250, 250],
        "liberty AbilData:2444 (SpireResearch Research3)",
    ),
    (
        "flyer_carapace_1",
        [100, 100],
        "voidmulti AbilData:1797 (SpireResearch Research4)",
    ),
    (
        "flyer_carapace_2",
        [175, 175],
        "voidmulti AbilData:1801 (SpireResearch Research5)",
    ),
    (
        "flyer_carapace_3",
        [250, 250],
        "voidmulti AbilData:1805 (SpireResearch Research6)",
    ),
    (
        "metabolic_boost",
        [100, 100],
        "liberty AbilData:2410 (SpawningPoolResearch Research2)",
    ),
    (
        "adrenal_glands",
        [200, 200],
        "liberty AbilData:2403 (SpawningPoolResearch Research1)",
    ),
    (
        "centrifugal_hooks",
        [100, 100],
        "voidmulti AbilData:1772 (BanelingNestResearch Research1)",
    ),
    (
        "glial_reconstitution",
        [100, 100],
        "liberty AbilData:901 (RoachWarrenResearch Research2)",
    ),
    (
        "tunneling_claws",
        [100, 100],
        "voidmulti AbilData:859 (RoachWarrenResearch Research3)",
    ),
    (
        "muscular_augments",
        [100, 100],
        "voidmulti AbilData:536 (HydraliskDenResearch Research2)",
    ),
    (
        "grooved_spines",
        [75, 75],
        "voidmulti AbilData:529 (HydraliskDenResearch Research1)",
    ),
    (
        "nanomuscular_swell",
        [100, 100],
        "voidmulti AbilData:543 (HydraliskDenResearch Research3)",
    ),
    (
        "adaptive_talons",
        [100, 100],
        "voidmulti AbilData:1348 (LurkerDenResearch Research1)",
    ),
    (
        "seismic_spines",
        [150, 150],
        "voidmulti AbilData:1355 (LurkerDenResearch Research2)",
    ),
    (
        "pneumatized_carapace",
        [100, 100],
        "swarm AbilData:1284 (LairResearch Research2)",
    ),
    (
        "chitinous_plating",
        [150, 150],
        "liberty AbilData:1065 (UltraliskCavernResearch Research3)",
    ),
    (
        "anabolic_synthesis",
        [150, 150],
        "voidmulti AbilData:1105 (UltraliskCavernResearch Research1)",
    ),
    (
        "microbial_shroud",
        [150, 150],
        "voidmulti AbilData:599 (InfestationPitResearch Research6)",
    ),
    (
        "neural_parasite",
        [150, 150],
        "swarm AbilData:1073 (InfestationPitResearch Research4)",
    ),
];

#[test]
fn b47_corrected_costs_follow_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    for (unit, [minerals, gas], source) in SC2_CORRECTED_COSTS {
        let archetype = catalogs
            .units
            .archetypes()
            .find(|archetype| analogue(archetype) == unit)
            .ok_or(format!("an archetype with the analogue {unit}"))?;
        assert_eq!(
            [archetype.mineral_cost, archetype.gas_cost],
            [minerals, gas],
            "{} ({unit}): cost as StarCraft II's ({source})",
            archetype.slug
        );
    }
    Ok(())
}

#[test]
fn b47_research_costs_follow_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    for upgrade in catalogs.upgrades.upgrades() {
        let Some((_, [minerals, gas], source)) = SC2_RESEARCH_COSTS
            .iter()
            .find(|(slug, ..)| *slug == upgrade.slug)
        else {
            return Err(format!("{}: no research cost row", upgrade.slug).into());
        };
        assert_eq!(
            [upgrade.mineral_cost, upgrade.gas_cost],
            [*minerals, *gas],
            "{}: research cost as StarCraft II's ({source})",
            upgrade.slug
        );
    }
    assert_eq!(
        catalogs.upgrades.upgrades().count(),
        SC2_RESEARCH_COSTS.len()
    );
    Ok(())
}

/// One weapon reference row: analogue, weapon block (`weapon`, `ground_weapon` or `air_weapon`),
/// value and source.
type WeaponRow = (&'static str, &'static str, f32, &'static str);

/// The weapon a unit without one of its own carries only in a mode (the Lurker analogue, armed
/// only burrowed as `StarCraft` II's `LurkerMPBurrowed`, B-42); checked as its `weapon` block.
fn mode_only_weapon(archetype: &UnitArchetype) -> Option<&WeaponSpec> {
    if archetype.weapon.is_some() || archetype.ground_weapon.is_some() {
        return None;
    }
    archetype
        .modes
        .iter()
        .find_map(|mode| mode.ground_weapon_override.as_ref()?.as_ref())
}

/// The mode weapons that replace a weapon of the archetype's own (Siege Mode's cannon).
fn replacing_mode_weapons(archetype: &UnitArchetype) -> impl Iterator<Item = &WeaponSpec> {
    let replaces = mode_only_weapon(archetype).is_none();
    archetype
        .modes
        .iter()
        .filter(move |_| replaces)
        .filter_map(|mode| mode.ground_weapon_override.as_ref()?.as_ref())
}

/// The archetype's weapons by block name; `weapon` is the ground fallback (a structure's is a
/// copy of its ground weapon, so structures skip it), or the weapon of [`mode_only_weapon`].
fn weapon_blocks(archetype: &UnitArchetype) -> [(&'static str, Option<&WeaponSpec>); 3] {
    [
        (
            "weapon",
            archetype
                .weapon
                .as_ref()
                .or_else(|| mode_only_weapon(archetype)),
        ),
        ("ground_weapon", archetype.ground_weapon.as_ref()),
        ("air_weapon", archetype.air_weapon.as_ref()),
    ]
}

/// Checks `read` of every weapon of every archetype of the kind `structures` selects, for which
/// `applies` holds, against `rows` by analogue and block; every row must be used.
fn check_weapons(
    what: &str,
    structures: bool,
    rows: &[WeaponRow],
    exempt: &[&str],
    applies: impl Fn(&WeaponSpec) -> bool,
    read: impl Fn(&WeaponSpec) -> f32,
) -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes() {
        let name = analogue(archetype);
        if is_structure(archetype) != structures || exempt.contains(&name) {
            continue;
        }
        for (block, weapon) in weapon_blocks(archetype) {
            let Some(weapon) = weapon.filter(|w| applies(w)) else {
                continue;
            };
            if structures && block == "weapon" {
                continue;
            }
            // A unit's ground weapon defaults to its `weapon`; only authored blocks have rows.
            let derived = block != "weapon" && archetype.weapon.as_ref() == Some(weapon);
            if derived
                && !rows
                    .iter()
                    .any(|(unit, b, ..)| *unit == name && *b == block)
            {
                continue;
            }
            let Some((_, _, want, source)) = rows
                .iter()
                .find(|(unit, b, ..)| *unit == name && *b == block)
            else {
                return Err(format!("{} ({name}) {block}: no {what} row", archetype.slug).into());
            };
            let got = read(weapon);
            assert!(
                (got - want).abs() < 0.001,
                "{} ({name}) {block}: {what} is {got}, StarCraft II {want} ({source})",
                archetype.slug
            );
            seen.insert((name.to_owned(), block));
        }
    }
    for (unit, block, ..) in rows {
        assert!(
            seen.contains(&((*unit).to_owned(), *block)),
            "{what}: row {unit} {block} has no weapon"
        );
    }
    Ok(())
}

/// Damage per strike of every armed structure (the weapon's first `CEffectDamage` `Amount`).
const SC2_STRUCTURE_DAMAGE: [WeaponRow; 6] = [
    (
        "Planetary Fortress",
        "ground_weapon",
        40.0,
        "TwinIbiksCannon, liberty EffectData:2537",
    ),
    (
        "Missile Turret",
        "air_weapon",
        12.0,
        "LongboltMissileU, liberty EffectData:1797",
    ),
    (
        "Photon Cannon",
        "ground_weapon",
        20.0,
        "PhotonCannonU, liberty EffectData:2578",
    ),
    (
        "Photon Cannon",
        "air_weapon",
        20.0,
        "PhotonCannonU, liberty EffectData:2578",
    ),
    (
        "Spine Crawler",
        "ground_weapon",
        25.0,
        "ImpalerTentacleU, liberty EffectData:2165",
    ),
    (
        "Spore Crawler",
        "air_weapon",
        20.0,
        "SporeCrawlerU, voidmulti EffectData:871",
    ),
];

#[test]
fn b47_structure_weapon_damage_follows_starcraft() -> TestResult {
    check_weapons(
        "damage",
        true,
        &SC2_STRUCTURE_DAMAGE,
        &[],
        |_| true,
        |w| f(w.base_damage),
    )
}

/// Structures whose life armour Neosteel Armor raises by 2 (`TerranBuildingArmor`, `liberty`
/// UpgradeData:842-898, every modelled Terran structure; B-51). The same upgrade adds 2 cargo slots
/// to the Bunker and 5 to the Command Center (`voidmulti` UpgradeData:265-268), which wait for
/// structure cargo (B-64, WP17).
const SC2_NEOSTEEL_ARMOR_TARGETS: [&str; 17] = [
    "Command Center",
    "Orbital Command",
    "Planetary Fortress",
    "Supply Depot",
    "Refinery",
    "Barracks",
    "Engineering Bay",
    "Bunker",
    "Sensor Tower",
    "Missile Turret",
    "Factory",
    "Ghost Academy",
    "Starport",
    "Armory",
    "Fusion Core",
    "Tech Lab",
    "Reactor",
];

/// One `ModifyUnitStat` effect: target slug, armour delta, range delta.
type StatEffect = (String, f32, f32);

/// The `(target slug, armour delta, range delta)` of every `ModifyUnitStat` effect of `slug`.
fn stat_effects(slug: &str) -> Result<Vec<StatEffect>, Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let upgrade = catalogs
        .upgrades
        .upgrades()
        .find(|upgrade| upgrade.slug == slug)
        .ok_or(format!("upgrade {slug}"))?;
    Ok(upgrade
        .effects
        .iter()
        .filter_map(|effect| match effect {
            UpgradeEffect::ModifyUnitStat {
                target_slug,
                armor_delta,
                weapon_range_delta,
                ..
            } => Some((target_slug.clone(), f(*armor_delta), f(*weapon_range_delta))),
            _ => None,
        })
        .collect())
}

#[test]
fn b51_neosteel_armor_raises_every_terran_structure() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let effects = stat_effects("neosteel_armor")?;
    for unit in SC2_NEOSTEEL_ARMOR_TARGETS {
        let archetype = catalogs
            .units
            .archetypes()
            .find(|archetype| analogue(archetype) == unit)
            .ok_or(format!("an archetype with the analogue {unit}"))?;
        assert!(
            effects.iter().any(|(target, armor, range)| {
                *target == archetype.slug && (armor - 2.0).abs() < 0.001 && range.abs() < 0.001
            }),
            "Neosteel Armor gives {} ({unit}) +2 armour (liberty UpgradeData:842-898)",
            archetype.slug
        );
    }
    assert_eq!(
        effects.len(),
        SC2_NEOSTEEL_ARMOR_TARGETS.len(),
        "no other target"
    );
    Ok(())
}

/// Advanced Ballistics adds range only to the Liberator's Defender Mode weapon and its zone
/// (`LiberatorAGRangeUpgrade`, `voidmulti` UpgradeData:154-155), which the catalog does not model
/// (B-27, WP18): it must not lengthen the anti-air weapon (B-87).
#[test]
fn b87_advanced_ballistics_leaves_the_anti_air_range() -> TestResult {
    let effects = stat_effects("advanced_ballistics")?;
    assert!(
        effects.iter().all(|(_, _, range)| range.abs() < 0.001),
        "Advanced Ballistics lengthens no modelled weapon: {effects:?}"
    );
    Ok(())
}

/// Unit attributes as `AttributeFlags` bits (`CUnit` `Attributes`, every layer; the citation is the
/// unit's element). Attributes the catalog does not model (Robotic, Hover, Summoned) are not listed.
const SC2_UNIT_ATTRIBUTES: [(&str, [f32; 1], &str); 42] = [
    (
        "SCV",
        [13.0],
        "Light+Biological+Mechanical, liberty UnitData:4722",
    ),
    ("Marine", [5.0], "Light+Biological, liberty UnitData:4814"),
    ("Roach", [6.0], "Armored+Biological, liberty UnitData:7178"),
    ("Zealot", [5.0], "Light+Biological, liberty UnitData:3166"),
    (
        "Siege Tank",
        [10.0],
        "Armored+Mechanical, liberty UnitData:5066",
    ),
    (
        "Marauder",
        [6.0],
        "Armored+Biological, liberty UnitData:10960",
    ),
    ("Reaper", [5.0], "Light+Biological, liberty UnitData:4884"),
    ("Ghost", [20.0], "Biological+Psionic, liberty UnitData:4980"),
    ("Hellion", [9.0], "Light+Mechanical, liberty UnitData:11544"),
    (
        "Thor",
        [42.0],
        "Armored+Mechanical+Massive, liberty UnitData:5201",
    ),
    (
        "Viking",
        [10.0],
        "Armored+Mechanical, liberty UnitData:6828",
    ),
    (
        "Medivac",
        [10.0],
        "Armored+Mechanical, liberty UnitData:5431",
    ),
    (
        "Liberator",
        [10.0],
        "Armored+Mechanical, void UnitData:3278",
    ),
    ("Banshee", [9.0], "Light+Mechanical, liberty UnitData:5365"),
    ("Raven", [9.0], "Light+Mechanical, liberty UnitData:5569"),
    ("Probe", [9.0], "Light+Mechanical, liberty UnitData:3061"),
    (
        "Stalker",
        [10.0],
        "Armored+Mechanical, liberty UnitData:3951",
    ),
    (
        "Sentry",
        [24.0],
        "Mechanical+Psionic, liberty UnitData:11288; Light 0 voidmulti UnitData:1553",
    ),
    ("Adept", [5.0], "Light+Biological, void UnitData:3176"),
    (
        "High Templar",
        [21.0],
        "Light+Biological+Psionic, liberty UnitData:3243",
    ),
    (
        "Dark Templar",
        [21.0],
        "Light+Biological+Psionic, liberty UnitData:3341",
    ),
    (
        "Immortal",
        [10.0],
        "Armored+Mechanical, liberty UnitData:10726",
    ),
    (
        "Colossus",
        [42.0],
        "Armored+Mechanical+Massive, liberty UnitData:4029",
    ),
    ("Observer", [9.0], "Light+Mechanical, liberty UnitData:3410"),
    (
        "Warp Prism",
        [26.0],
        "Armored+Mechanical+Psionic, liberty UnitData:3812",
    ),
    (
        "Void Ray",
        [10.0],
        "Armored+Mechanical, liberty UnitData:3745",
    ),
    (
        "Carrier",
        [42.0],
        "Armored+Mechanical+Massive, liberty UnitData:3469",
    ),
    ("Drone", [5.0], "Light+Biological, liberty UnitData:7030"),
    ("Zergling", [5.0], "Light+Biological, liberty UnitData:7544"),
    ("Baneling", [4.0], "Biological, liberty UnitData:9301"),
    ("Ravager", [4.0], "Biological, liberty UnitData:976"),
    (
        "Hydralisk",
        [5.0],
        "Light+Biological, liberty UnitData:7665",
    ),
    ("Lurker", [6.0], "Armored+Biological, liberty UnitData:690"),
    ("Mutalisk", [5.0], "Light+Biological, liberty UnitData:7783"),
    (
        "Corruptor",
        [6.0],
        "Armored+Biological, liberty UnitData:11212",
    ),
    (
        "Brood Lord",
        [38.0],
        "Armored+Biological+Massive, liberty UnitData:11102",
    ),
    (
        "Ultralisk",
        [38.0],
        "Armored+Biological+Massive, liberty UnitData:8004",
    ),
    (
        "Queen",
        [20.0],
        "Biological+Psionic, liberty UnitData:11405",
    ),
    (
        "Infestor",
        [22.0],
        "Armored+Biological+Psionic, liberty UnitData:10049",
    ),
    (
        "Viper",
        [22.0],
        "Armored+Biological+Psionic, swarm UnitData:2313",
    ),
    (
        "Overlord",
        [6.0],
        "Armored+Biological, liberty UnitData:7354",
    ),
    (
        "Interceptor",
        [9.0],
        "Light+Mechanical, liberty UnitData:3592-3593",
    ),
];

#[test]
fn b47_unit_attributes_follow_starcraft() -> TestResult {
    check_table(
        "attributes",
        false,
        &SC2_UNIT_ATTRIBUTES,
        &["Phoenix"],
        |a| [f32::from(a.attributes.bits())],
    )
}

/// Damage per strike of every armed unit (the weapon's first `CEffectDamage` `Amount`). The Brood
/// Lord deals its damage through Broodlings (B-21) and has no row.
const SC2_UNIT_DAMAGE: [WeaponRow; 45] = [
    (
        "SCV",
        "weapon",
        5.0,
        "FusionCutter, liberty EffectData:1623",
    ),
    (
        "Marine",
        "weapon",
        6.0,
        "GuassRifle, liberty EffectData:1628",
    ),
    (
        "Marine",
        "air_weapon",
        6.0,
        "GuassRifle, liberty EffectData:1628",
    ),
    (
        "Roach",
        "weapon",
        16.0,
        "AcidSalivaU, liberty EffectData:2144",
    ),
    (
        "Zealot",
        "weapon",
        8.0,
        "PsiBlades, liberty EffectData:1931",
    ),
    (
        "Siege Tank",
        "weapon",
        15.0,
        "90mmCannons, liberty EffectData:1664",
    ),
    (
        "Marauder",
        "weapon",
        10.0,
        "PunisherGrenadesU, liberty EffectData:1649",
    ),
    (
        "Reaper",
        "weapon",
        4.0,
        "P38ScytheGuassPistol, liberty EffectData:1816",
    ),
    (
        "Ghost",
        "weapon",
        10.0,
        "C10CanisterRifle, liberty EffectData:1633",
    ),
    (
        "Hellion",
        "weapon",
        8.0,
        "InfernalFlameThrower, liberty EffectData:2604",
    ),
    (
        "Thor",
        "weapon",
        30.0,
        "ThorsHammerDamage, swarm EffectData:2355",
    ),
    (
        "Thor",
        "ground_weapon",
        30.0,
        "ThorsHammerDamage, swarm EffectData:2355",
    ),
    (
        "Thor",
        "air_weapon",
        25.0,
        "LanceMissileLaunchersDamage, voidmulti EffectData:750",
    ),
    (
        "Viking",
        "air_weapon",
        10.0,
        "LanzerTorpedoesDamage, liberty EffectData:1717",
    ),
    (
        "Liberator",
        "air_weapon",
        5.0,
        "LiberatorMissileDamage, voidmulti EffectData:753",
    ),
    (
        "Banshee",
        "weapon",
        12.0,
        "BacklashRocketsU, liberty EffectData:2567",
    ),
    (
        "Probe",
        "weapon",
        5.0,
        "ParticleBeam, liberty EffectData:1917",
    ),
    (
        "Stalker",
        "weapon",
        13.0,
        "ParticleDisruptorsU, voidmulti EffectData:584",
    ),
    (
        "Stalker",
        "air_weapon",
        13.0,
        "ParticleDisruptorsU, voidmulti EffectData:584",
    ),
    (
        "Sentry",
        "weapon",
        6.0,
        "DisruptionBeamDamage, liberty EffectData:2530",
    ),
    ("Adept", "weapon", 10.0, "AdeptDamage, void EffectData:2772"),
    (
        "High Templar",
        "weapon",
        4.0,
        "HighTemplarWeaponDamage, voidmulti EffectData:1543",
    ),
    (
        "Dark Templar",
        "weapon",
        45.0,
        "WarpBlades, liberty EffectData:1937",
    ),
    (
        "Immortal",
        "weapon",
        20.0,
        "PhaseDisruptors, liberty EffectData:2434",
    ),
    (
        "Colossus",
        "weapon",
        10.0,
        "ThermalLancesMU, voidmulti EffectData:965",
    ),
    (
        "Void Ray",
        "weapon",
        6.0,
        "VoidRaySwarmDamage, void EffectData:1809",
    ),
    (
        "Void Ray",
        "air_weapon",
        6.0,
        "VoidRaySwarmDamage, void EffectData:1809",
    ),
    ("Drone", "weapon", 5.0, "Spines, liberty EffectData:2130"),
    ("Zergling", "weapon", 5.0, "Claws, liberty EffectData:2134"),
    (
        "Baneling",
        "weapon",
        16.0,
        "VolatileBurstU, voidmulti EffectData:1027",
    ),
    (
        "Ravager",
        "weapon",
        16.0,
        "RavagerWeaponDamage, void EffectData:1603",
    ),
    (
        "Hydralisk",
        "weapon",
        12.0,
        "NeedleSpinesDamage, liberty EffectData:2179",
    ),
    (
        "Hydralisk",
        "air_weapon",
        12.0,
        "NeedleSpinesDamage, liberty EffectData:2179",
    ),
    (
        "Lurker",
        "weapon",
        20.0,
        "LurkerMPDamage, swarm EffectData:3033",
    ),
    (
        "Mutalisk",
        "weapon",
        9.0,
        "GlaiveWurmU1, liberty EffectData:2230",
    ),
    (
        "Mutalisk",
        "air_weapon",
        9.0,
        "GlaiveWurmU1, liberty EffectData:2230",
    ),
    (
        "Corruptor",
        "air_weapon",
        14.0,
        "ParasiteSporeDamage, swarm EffectData:2063",
    ),
    (
        "Ultralisk",
        "weapon",
        35.0,
        "KaiserBladesDamage, void EffectData:1169",
    ),
    (
        "Queen",
        "weapon",
        4.0,
        "TalonsMissileDamage, liberty EffectData:2333",
    ),
    (
        "Queen",
        "ground_weapon",
        4.0,
        "TalonsMissileDamage, liberty EffectData:2333",
    ),
    (
        "Queen",
        "air_weapon",
        9.0,
        "AcidSpines, liberty EffectData:2849",
    ),
    (
        "Carrier",
        "weapon",
        0.0,
        "InterceptorLaunch deals none: CarrierInterceptor, a CEffectUseMagazine, liberty EffectData:1940",
    ),
    (
        "Interceptor",
        "weapon",
        5.0,
        "InterceptorBeamDamage Amount 5, liberty EffectData:1953-1958",
    ),
    (
        "Carrier",
        "air_weapon",
        0.0,
        "InterceptorLaunch deals none: CarrierInterceptor, a CEffectUseMagazine, liberty EffectData:1940",
    ),
    (
        "Interceptor",
        "air_weapon",
        5.0,
        "InterceptorBeamDamage Amount 5, liberty EffectData:1953-1958",
    ),
];

/// Damage per upgrade level the audit corrected or resolved through an indexed upgrade override:
/// `(analogue, per level, source)`.
const SC2_DAMAGE_PER_LEVEL: [(&str, f32, &str); 2] = [
    (
        "Ravager",
        2.0,
        "RavagerWeaponDamage, ZergMissileWeaponsLevel1 void UpgradeData:517",
    ),
    (
        "Ultralisk",
        3.0,
        "KaiserBladesDamage, ZergMeleeWeaponsLevel1 void UpgradeData:624",
    ),
];

#[test]
fn b47_unit_weapon_damage_follows_starcraft() -> TestResult {
    check_weapons(
        "damage",
        false,
        &SC2_UNIT_DAMAGE,
        &["Phoenix", "Brood Lord"],
        |_| true,
        |w| f(w.base_damage),
    )?;
    let catalogs = sim_fixtures::shipped_catalogs();
    for (unit, per_level, source) in SC2_DAMAGE_PER_LEVEL {
        let weapon = catalogs
            .units
            .archetypes()
            .find(|archetype| analogue(archetype) == unit)
            .and_then(|archetype| archetype.weapon.clone())
            .ok_or(format!("an armed archetype with the analogue {unit}"))?;
        assert!(
            (f(weapon.upgrade_bonus_per_level) - per_level).abs() < 0.001,
            "{unit}: +{per_level} a level ({source})"
        );
    }
    Ok(())
}

/// Bonus damage gained per upgrade level by each attribute bonus (`Effect,<damage>,AttributeBonus[…]`
/// of the weapon's `…Level1` upgrade; none for the Viking and the Void Ray).
const SC2_BONUS_PER_LEVEL: [WeaponRow; 16] = [
    (
        "Siege Tank",
        "weapon",
        1.0,
        "90mmCannons, TerranVehicleWeaponsLevel1 liberty UpgradeData:87",
    ),
    (
        "Marauder",
        "weapon",
        1.0,
        "PunisherGrenadesU, TerranInfantryWeaponsLevel1 liberty UpgradeData:20",
    ),
    (
        "Ghost",
        "weapon",
        1.0,
        "C10CanisterRifle, TerranInfantryWeaponsLevel1 liberty UpgradeData:24",
    ),
    (
        "Hellion",
        "weapon",
        1.0,
        "InfernalFlameThrower, TerranVehicleWeaponsLevel1 liberty UpgradeData:89",
    ),
    (
        "Thor",
        "air_weapon",
        1.0,
        "LanceMissileLaunchersDamage, TerranVehicleWeaponsLevel1 voidmulti UpgradeData:314",
    ),
    (
        "Viking",
        "air_weapon",
        0.0,
        "LanzerTorpedoesDamage, no upgrade entry",
    ),
    (
        "Stalker",
        "weapon",
        1.0,
        "ParticleDisruptorsU, ProtossGroundWeaponsLevel1 voidmulti UpgradeData:491",
    ),
    (
        "Stalker",
        "air_weapon",
        1.0,
        "ParticleDisruptorsU, ProtossGroundWeaponsLevel1 voidmulti UpgradeData:491",
    ),
    (
        "Adept",
        "weapon",
        1.0,
        "AdeptDamage, ProtossGroundWeaponsLevel1 void UpgradeData:685",
    ),
    (
        "Immortal",
        "weapon",
        3.0,
        "PhaseDisruptors, ProtossGroundWeaponsLevel1 liberty UpgradeData:252",
    ),
    (
        "Colossus",
        "weapon",
        1.0,
        "ThermalLancesMU, ProtossGroundWeaponsLevel1 voidmulti UpgradeData:486",
    ),
    (
        "Void Ray",
        "weapon",
        0.0,
        "VoidRaySwarmDamage, no upgrade entry",
    ),
    (
        "Void Ray",
        "air_weapon",
        0.0,
        "VoidRaySwarmDamage, no upgrade entry",
    ),
    (
        "Baneling",
        "weapon",
        2.0,
        "VolatileBurstU AttributeBonus[Light], ZergMeleeWeapons liberty UpgradeData:509",
    ),
    (
        "Lurker",
        "weapon",
        1.0,
        "LurkerMPDamage, ZergMissileWeaponsLevel1 voidmulti UpgradeData:433",
    ),
    (
        "Corruptor",
        "air_weapon",
        1.0,
        "ParasiteSporeDamage, ZergFlyerWeaponsLevel1 liberty UpgradeData:679",
    ),
];

#[test]
fn b47_bonus_damage_per_level_follows_starcraft() -> TestResult {
    check_weapons(
        "bonus per level",
        false,
        &SC2_BONUS_PER_LEVEL,
        &["Phoenix"],
        |w| {
            w.attribute_bonuses
                .entries()
                .any(|(_, bonus)| bonus > SimScalar::ZERO)
        },
        |w| f(w.bonus_upgrade_per_level),
    )?;
    // The Siege Mode cannon: +4 and Armored +1 a level (`voidmulti` UpgradeData:302, :305).
    let catalogs = sim_fixtures::shipped_catalogs();
    let tank = catalogs
        .units
        .archetypes()
        .find(|archetype| analogue(archetype) == "Siege Tank")
        .ok_or("a Siege Tank analogue")?;
    let sieged = tank
        .modes
        .iter()
        .find_map(|mode| mode.ground_weapon_override.clone().flatten())
        .ok_or("a Siege Mode weapon")?;
    assert_eq!(
        [
            f(sieged.upgrade_bonus_per_level),
            f(sieged.bonus_upgrade_per_level)
        ],
        [4.0, 1.0],
        "Siege Mode: +4 (+1 Armored) a level (voidmulti UpgradeData:302, :305)"
    );
    Ok(())
}

/// Unit life, shields and life armour (`CUnit` `LifeMax`, `ShieldsMax`, `LifeArmor`). The
/// saucer without one analogue is class E (evidence §10.1); the Carrier takes the Carrier's
/// (Q0129 (a); plan `2026-11-28-carrier-interceptors` D-1, B-228).
const SC2_UNIT_VITALS: [(&str, [f32; 3], &str); 42] = [
    ("SCV", [45.0, 0.0, 0.0], "liberty UnitData:4729; default 0"),
    (
        "Thor",
        [400.0, 0.0, 1.0],
        "liberty UnitData:5207; default 0; liberty UnitData:5208",
    ),
    (
        "Marine",
        [45.0, 0.0, 0.0],
        "liberty UnitData:4819; default 0",
    ),
    (
        "Roach",
        [145.0, 0.0, 1.0],
        "liberty UnitData:7183; default 0; liberty UnitData:7239",
    ),
    (
        "Zealot",
        [100.0, 50.0, 1.0],
        "liberty UnitData:3171; liberty UnitData:3175; liberty UnitData:3172",
    ),
    (
        "Siege Tank",
        [175.0, 0.0, 1.0],
        "voidmulti UnitData:1577; default 0; liberty UnitData:5073",
    ),
    (
        "Marauder",
        [125.0, 0.0, 1.0],
        "liberty UnitData:10965; default 0; liberty UnitData:10966",
    ),
    ("Reaper", [60.0, 0.0, 0.0], "swarm UnitData:1844; default 0"),
    (
        "Ghost",
        [100.0, 0.0, 0.0],
        "liberty UnitData:4985; default 0",
    ),
    (
        "Hellion",
        [90.0, 0.0, 0.0],
        "liberty UnitData:11549; default 0",
    ),
    (
        "Viking",
        [135.0, 0.0, 0.0],
        "voidmulti UnitData:2000; default 0",
    ),
    (
        "Medivac",
        [150.0, 0.0, 1.0],
        "liberty UnitData:5436; default 0; liberty UnitData:5437",
    ),
    (
        "Liberator",
        [180.0, 0.0, 0.0],
        "void UnitData:3293; default 0",
    ),
    (
        "Banshee",
        [140.0, 0.0, 0.0],
        "liberty UnitData:5370; default 0",
    ),
    (
        "Raven",
        [140.0, 0.0, 1.0],
        "liberty UnitData:5574; default 0; liberty UnitData:5575",
    ),
    (
        "Probe",
        [20.0, 20.0, 0.0],
        "liberty UnitData:3068; liberty UnitData:3071; default 0",
    ),
    (
        "Stalker",
        [80.0, 80.0, 1.0],
        "liberty UnitData:3956; liberty UnitData:3960; liberty UnitData:3957",
    ),
    (
        "Sentry",
        [40.0, 40.0, 1.0],
        "liberty UnitData:11293; liberty UnitData:11300; liberty UnitData:11294",
    ),
    (
        "Adept",
        [70.0, 70.0, 1.0],
        "voidmulti UnitData:27; voidmulti UnitData:25; void UnitData:3193",
    ),
    (
        "High Templar",
        [40.0, 40.0, 0.0],
        "liberty UnitData:3248; liberty UnitData:3254; default 0",
    ),
    (
        "Dark Templar",
        [40.0, 80.0, 1.0],
        "liberty UnitData:3346; liberty UnitData:3350; liberty UnitData:3347",
    ),
    (
        "Immortal",
        [200.0, 100.0, 1.0],
        "liberty UnitData:10731; liberty UnitData:10734; liberty UnitData:10789",
    ),
    (
        "Colossus",
        [250.0, 100.0, 1.0],
        "voidmulti UnitData:246; voidmulti UnitData:248; liberty UnitData:4035",
    ),
    (
        "Observer",
        [40.0, 30.0, 0.0],
        "liberty UnitData:3415; voidmulti UnitData:1137; default 0",
    ),
    (
        "Warp Prism",
        [80.0, 100.0, 0.0],
        "voidmulti UnitData:2045; swarm UnitData:1768; default 0",
    ),
    (
        "Void Ray",
        [150.0, 100.0, 0.0],
        "liberty UnitData:3750; liberty UnitData:3753; default 0",
    ),
    (
        "Drone",
        [40.0, 0.0, 0.0],
        "liberty UnitData:7037; default 0",
    ),
    (
        "Zergling",
        [35.0, 0.0, 0.0],
        "liberty UnitData:7549; default 0",
    ),
    (
        "Baneling",
        [30.0, 0.0, 0.0],
        "liberty UnitData:9306; default 0",
    ),
    (
        "Ravager",
        [120.0, 0.0, 1.0],
        "liberty UnitData:989; default 0; liberty UnitData:990",
    ),
    (
        "Hydralisk",
        [90.0, 0.0, 0.0],
        "voidmulti UnitData:736; default 0",
    ),
    (
        "Lurker",
        [190.0, 0.0, 1.0],
        "voidmulti UnitData:984; default 0; liberty UnitData:709",
    ),
    (
        "Mutalisk",
        [120.0, 0.0, 0.0],
        "liberty UnitData:7788; default 0",
    ),
    (
        "Corruptor",
        [200.0, 0.0, 2.0],
        "liberty UnitData:11217; default 0; liberty UnitData:11272",
    ),
    (
        "Brood Lord",
        [225.0, 0.0, 1.0],
        "swarm UnitData:2416; default 0; swarm UnitData:2417",
    ),
    (
        "Ultralisk",
        [500.0, 0.0, 2.0],
        "swarm UnitData:1993; default 0; voidmulti UnitData:1906",
    ),
    (
        "Queen",
        [175.0, 0.0, 1.0],
        "liberty UnitData:11411; default 0; liberty UnitData:11473",
    ),
    (
        "Infestor",
        [90.0, 0.0, 0.0],
        "liberty UnitData:10054; default 0",
    ),
    (
        "Viper",
        [150.0, 0.0, 1.0],
        "void UnitData:1420; default 0; swarm UnitData:2331",
    ),
    (
        "Overlord",
        [200.0, 0.0, 0.0],
        "liberty UnitData:7361; default 0",
    ),
    (
        "Carrier",
        [300.0, 150.0, 2.0],
        "liberty UnitData:3474; liberty UnitData:3478; liberty UnitData:3475",
    ),
    (
        "Interceptor",
        [40.0, 40.0, 0.0],
        "liberty UnitData:3550; liberty UnitData:3553; default 0",
    ),
];

#[test]
fn b47_unit_vitals_follow_starcraft() -> TestResult {
    check_table(
        "life, shields, armour",
        false,
        &SC2_UNIT_VITALS,
        &["Phoenix"],
        |a| [f(a.max_hp), f(a.max_shields), f(a.life_armor)],
    )
}

/// Unit sight (`CUnit` `Sight`).
const SC2_UNIT_SIGHT: [(&str, [f32; 1], &str); 42] = [
    ("SCV", [8.0], "liberty UnitData:4759"),
    ("Marine", [9.0], "liberty UnitData:4850"),
    ("Roach", [9.0], "liberty UnitData:7210"),
    ("Zealot", [9.0], "liberty UnitData:3201"),
    ("Siege Tank", [11.0], "liberty UnitData:5107"),
    ("Marauder", [10.0], "liberty UnitData:10992"),
    ("Reaper", [9.0], "liberty UnitData:4923"),
    ("Ghost", [11.0], "liberty UnitData:5019"),
    ("Hellion", [10.0], "liberty UnitData:11580"),
    ("Thor", [11.0], "liberty UnitData:5238"),
    ("Viking", [10.0], "liberty UnitData:6866"),
    ("Medivac", [11.0], "liberty UnitData:5474"),
    ("Liberator", [10.0], "void UnitData:3300"),
    ("Banshee", [10.0], "liberty UnitData:5401"),
    ("Raven", [11.0], "liberty UnitData:5607"),
    ("Probe", [8.0], "liberty UnitData:3099"),
    ("Stalker", [10.0], "liberty UnitData:3988"),
    ("Sentry", [10.0], "liberty UnitData:11325"),
    ("Adept", [9.0], "void UnitData:3205"),
    ("High Templar", [10.0], "liberty UnitData:3290"),
    ("Dark Templar", [8.0], "liberty UnitData:3376"),
    ("Immortal", [9.0], "liberty UnitData:10760"),
    ("Colossus", [10.0], "liberty UnitData:4070"),
    ("Observer", [11.0], "liberty UnitData:3442"),
    ("Warp Prism", [10.0], "liberty UnitData:3848"),
    ("Void Ray", [10.0], "liberty UnitData:3780"),
    ("Carrier", [12.0], "liberty UnitData:3507"),
    ("Drone", [8.0], "liberty UnitData:7063"),
    ("Zergling", [8.0], "liberty UnitData:7577"),
    ("Baneling", [8.0], "liberty UnitData:9340"),
    ("Ravager", [9.0], "liberty UnitData:999"),
    ("Hydralisk", [9.0], "liberty UnitData:7698"),
    ("Lurker", [11.0], "voidmulti UnitData:985"),
    ("Mutalisk", [11.0], "liberty UnitData:7810"),
    ("Corruptor", [10.0], "liberty UnitData:11242"),
    ("Brood Lord", [12.0], "liberty UnitData:11132"),
    ("Ultralisk", [9.0], "liberty UnitData:8041"),
    ("Queen", [9.0], "liberty UnitData:11446"),
    ("Infestor", [10.0], "liberty UnitData:10092"),
    ("Viper", [11.0], "swarm UnitData:2343"),
    ("Overlord", [11.0], "liberty UnitData:7390"),
    ("Interceptor", [7.0], "liberty UnitData:3581"),
];

#[test]
fn b47_unit_sight_follows_starcraft() -> TestResult {
    check_table("sight", false, &SC2_UNIT_SIGHT, &["Phoenix"], |a| {
        [f(a.sight_radius)]
    })
}

/// Maximum range of every armed unit's weapon (`CWeapon` `Range`; `core` WeaponData:12 is the class
/// default 5).
const SC2_UNIT_RANGE: [WeaponRow; 47] = [
    ("SCV", "weapon", 0.2, "voidmulti WeaponData:105"),
    ("Marine", "weapon", 5.0, "core WeaponData:12"),
    ("Marine", "air_weapon", 5.0, "core WeaponData:12"),
    ("Roach", "weapon", 4.0, "swarm WeaponData:167"),
    ("Zealot", "weapon", 0.1, "liberty WeaponData:154"),
    ("Siege Tank", "weapon", 7.0, "liberty WeaponData:241"),
    ("Marauder", "weapon", 6.0, "liberty WeaponData:248"),
    ("Reaper", "weapon", 5.0, "void WeaponData:90"),
    ("Ghost", "weapon", 6.0, "liberty WeaponData:577"),
    ("Hellion", "weapon", 5.0, "core WeaponData:12"),
    ("Thor", "weapon", 7.0, "liberty WeaponData:231"),
    ("Thor", "ground_weapon", 7.0, "liberty WeaponData:231"),
    ("Thor", "air_weapon", 11.0, "voidmulti WeaponData:130"),
    ("Viking", "air_weapon", 9.0, "liberty WeaponData:272"),
    ("Liberator", "air_weapon", 5.0, "core WeaponData:12"),
    ("Banshee", "weapon", 6.0, "liberty WeaponData:555"),
    ("Probe", "weapon", 0.2, "voidmulti WeaponData:197"),
    ("Stalker", "weapon", 6.0, "liberty WeaponData:490"),
    ("Stalker", "air_weapon", 6.0, "liberty WeaponData:490"),
    ("Sentry", "weapon", 5.0, "core WeaponData:12"),
    ("Adept", "weapon", 4.0, "void WeaponData:317"),
    ("High Templar", "weapon", 6.0, "voidmulti WeaponData:291"),
    ("Dark Templar", "weapon", 0.1, "liberty WeaponData:165"),
    ("Immortal", "weapon", 6.0, "swarm WeaponData:61"),
    ("Colossus", "weapon", 7.0, "voidmulti WeaponData:255"),
    ("Void Ray", "weapon", 6.0, "void WeaponData:166"),
    ("Void Ray", "air_weapon", 6.0, "void WeaponData:166"),
    ("Drone", "weapon", 0.2, "voidmulti WeaponData:220"),
    ("Zergling", "weapon", 0.1, "liberty WeaponData:361"),
    ("Baneling", "weapon", 0.25, "liberty WeaponData:467"),
    ("Ravager", "weapon", 6.0, "void WeaponData:143"),
    ("Hydralisk", "weapon", 5.0, "core WeaponData:12"),
    ("Hydralisk", "air_weapon", 5.0, "core WeaponData:12"),
    ("Lurker", "weapon", 8.0, "voidmulti WeaponData:145"),
    ("Mutalisk", "weapon", 3.0, "liberty WeaponData:401"),
    ("Mutalisk", "air_weapon", 3.0, "liberty WeaponData:401"),
    ("Corruptor", "air_weapon", 6.0, "liberty WeaponData:571"),
    ("Brood Lord", "weapon", 10.0, "voidmulti WeaponData:75"),
    (
        "Brood Lord",
        "ground_weapon",
        10.0,
        "voidmulti WeaponData:75",
    ),
    ("Ultralisk", "weapon", 1.0, "liberty WeaponData:436"),
    ("Queen", "weapon", 5.0, "core WeaponData:12"),
    ("Queen", "ground_weapon", 5.0, "core WeaponData:12"),
    ("Queen", "air_weapon", 7.0, "liberty WeaponData:105"),
    (
        "Carrier",
        "weapon",
        8.0,
        "InterceptorLaunch Range 8, liberty WeaponData:601-614",
    ),
    (
        "Interceptor",
        "weapon",
        2.0,
        "InterceptorBeam Range 2, liberty WeaponData:626-637",
    ),
    (
        "Carrier",
        "air_weapon",
        8.0,
        "InterceptorLaunch Range 8, liberty WeaponData:601-614",
    ),
    (
        "Interceptor",
        "air_weapon",
        2.0,
        "InterceptorBeam Range 2, liberty WeaponData:626-637",
    ),
];

#[test]
fn b47_unit_weapon_ranges_follow_starcraft() -> TestResult {
    check_weapons(
        "range",
        false,
        &SC2_UNIT_RANGE,
        &["Phoenix"],
        |_| true,
        |w| f(w.max_range),
    )
}

/// Unit collision radius (`CUnit` `Radius`; `core` UnitData:41 is the class default 0.5).
const SC2_UNIT_RADIUS: [(&str, [f32; 1], &str); 42] = [
    ("SCV", [0.375], "liberty UnitData:4743"),
    ("Marine", [0.375], "liberty UnitData:4837"),
    ("Roach", [0.5], "class default, core UnitData:41"),
    ("Zealot", [0.5], "class default, core UnitData:41"),
    ("Siege Tank", [0.875], "liberty UnitData:5094"),
    ("Marauder", [0.5625], "liberty UnitData:11036"),
    ("Reaper", [0.375], "liberty UnitData:4910"),
    ("Ghost", [0.375], "liberty UnitData:5005"),
    ("Hellion", [0.625], "liberty UnitData:11568"),
    ("Thor", [1.0], "void UnitData:1404"),
    ("Viking", [0.75], "liberty UnitData:6857"),
    ("Medivac", [0.75], "liberty UnitData:5464"),
    ("Liberator", [0.75], "void UnitData:3327"),
    ("Banshee", [0.75], "liberty UnitData:5391"),
    ("Raven", [0.625], "liberty UnitData:5597"),
    ("Probe", [0.375], "liberty UnitData:3083"),
    ("Stalker", [0.625], "liberty UnitData:3973"),
    ("Sentry", [0.5], "class default, core UnitData:41"),
    ("Adept", [0.5], "class default, core UnitData:41"),
    ("High Templar", [0.375], "liberty UnitData:3267"),
    ("Dark Templar", [0.375], "liberty UnitData:3361"),
    ("Immortal", [0.75], "liberty UnitData:10744"),
    ("Colossus", [1.0], "liberty UnitData:4051"),
    ("Observer", [0.5], "class default, core UnitData:41"),
    ("Warp Prism", [0.875], "liberty UnitData:3833"),
    ("Void Ray", [1.0], "liberty UnitData:3767"),
    ("Drone", [0.375], "liberty UnitData:7049"),
    ("Zergling", [0.375], "liberty UnitData:7563"),
    ("Baneling", [0.375], "liberty UnitData:9320"),
    ("Ravager", [0.75], "liberty UnitData:1018"),
    ("Hydralisk", [0.625], "liberty UnitData:7676"),
    ("Lurker", [0.9375], "void UnitData:754"),
    ("Mutalisk", [0.5], "class default, core UnitData:41"),
    ("Corruptor", [0.625], "liberty UnitData:11231"),
    ("Brood Lord", [1.0], "liberty UnitData:11120"),
    ("Ultralisk", [1.0], "liberty UnitData:8022"),
    ("Queen", [0.875], "liberty UnitData:11428"),
    ("Infestor", [0.625], "voidmulti UnitData:830"),
    ("Viper", [1.0], "swarm UnitData:2370"),
    ("Overlord", [1.0], "liberty UnitData:7376"),
    ("Carrier", [1.25], "liberty UnitData:3491"),
    ("Interceptor", [0.25], "liberty UnitData:3565"),
];

#[test]
fn b47_unit_radii_follow_starcraft() -> TestResult {
    check_table("radius", false, &SC2_UNIT_RADIUS, &["Phoenix"], |a| {
        [f(a.collision_radius)]
    })
}

/// B-116: the radius of every ground unit's disc against terrain and structures, `StarCraft` II's
/// `min(InnerRadius, Radius)` (plan `2026-11-18-inner-radius` §2.2; 5.0.14, `5444e12f13`). Rows
/// name analogues, never archetypes (charter §3.3).
const SC2_UNIT_TERRAIN_RADIUS: [(&str, [f32; 1], &str); 27] = [
    ("SCV", [0.3125], "voidmulti UnitData:1528; 5.0.13 notes"),
    ("Probe", [0.3125], "voidmulti UnitData:1287; 5.0.13 notes"),
    ("Drone", [0.3125], "voidmulti UnitData:523; 5.0.13 notes"),
    ("Zealot", [0.375], "liberty UnitData:3186"),
    ("Marauder", [0.375], "liberty UnitData:11016"),
    ("Hellion", [0.5], "liberty UnitData:11570"),
    ("Stalker", [0.5], "liberty UnitData:3975"),
    ("Sentry", [0.375], "liberty UnitData:11395"),
    ("Adept", [0.375], "void UnitData:3233"),
    ("Immortal", [0.5], "liberty UnitData:10746"),
    ("Colossus", [0.5625], "liberty UnitData:4053"),
    ("Ravager", [0.5], "liberty UnitData:1019"),
    ("Hydralisk", [0.375], "liberty UnitData:7685"),
    ("Lurker", [0.5], "void UnitData:755"),
    ("Ultralisk", [0.75], "liberty UnitData:8024"),
    ("Queen", [0.5], "liberty UnitData:11417"),
    ("Infestor", [0.5], "liberty UnitData:10071"),
    (
        "Marine",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:4839",
    ),
    (
        "Reaper",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:4912",
    ),
    (
        "Ghost",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:5007",
    ),
    (
        "Siege Tank",
        [0.875],
        "InnerRadius 0.875 equals Radius, liberty UnitData:5096",
    ),
    (
        "Thor",
        [1.0],
        "InnerRadius 1.0 equals Radius, void UnitData:1406",
    ),
    (
        "High Templar",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:3268",
    ),
    (
        "Dark Templar",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:3362",
    ),
    (
        "Baneling",
        [0.375],
        "InnerRadius 0.375 equals Radius, liberty UnitData:9322",
    ),
    (
        "Roach",
        [0.5],
        "InnerRadius 0.625 >= Radius 0.5, no effect, liberty UnitData:7197",
    ),
    (
        "Zergling",
        [0.375],
        "InnerRadius unset (liberty UnitData:7563): read as Radius, plan D-4, Q0087",
    ),
];

/// B-116: every ground unit meets terrain at its 5.0.14 effective `InnerRadius`; an air unit's
/// terrain radius is its collision radius.
#[test]
fn b116_unit_terrain_radii_follow_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes() {
        if is_structure(archetype) {
            continue;
        }
        if archetype.mover_type == sim_types::MoverType::Air {
            assert_eq!(
                archetype.terrain_radius,
                archetype.collision_radius,
                "{} ({}): an air unit's terrain radius is its radius",
                archetype.slug,
                analogue(archetype)
            );
            continue;
        }
        let name = analogue(archetype);
        let Some((_, [want], source)) = SC2_UNIT_TERRAIN_RADIUS
            .iter()
            .find(|(unit, ..)| *unit == name)
        else {
            return Err(format!("{} ({name}): no terrain radius row", archetype.slug).into());
        };
        let got = f(archetype.terrain_radius);
        assert!(
            (got - want).abs() < 0.001,
            "{} ({name}): terrain radius is {got}, StarCraft II {want} ({source})",
            archetype.slug
        );
        seen.insert(name.to_owned());
    }
    for (unit, ..) in &SC2_UNIT_TERRAIN_RADIUS {
        assert!(
            seen.contains(*unit),
            "terrain radius: row {unit} has no archetype"
        );
    }
    Ok(())
}

/// B-116: a unit whose terrain radius is below its collision radius is a ground unit with a row
/// above that says so, so no unsourced value can be authored. A structure's terrain radius is not
/// authored: it is its footprint shape's smaller half-extent (half its footprint's smaller side
/// for a full box, plan `2026-11-18-structure-radius` D-5; the Spore Crawler analogue's 0.5 on
/// 2×2 cells, plan `2026-11-18-contour-footprints` WP6), below its radius, and no rule reads it.
#[test]
fn only_ground_units_with_a_smaller_inner_radius_author_one() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut smaller = 0;
    for archetype in catalogs.units.archetypes() {
        if let sim_types::ShapeKind::BuildingDome { shape, .. } = archetype.shape {
            assert!(
                (f(archetype.terrain_radius) - f(shape.inscribed_half_extent())).abs() < 0.001
                    && archetype.terrain_radius <= archetype.collision_radius,
                "{}: a structure's terrain radius is its shape's smaller half-extent",
                archetype.slug
            );
            continue;
        }
        if archetype.terrain_radius >= archetype.collision_radius {
            continue;
        }
        let name = analogue(archetype);
        let row = SC2_UNIT_TERRAIN_RADIUS
            .iter()
            .find(|(unit, ..)| *unit == name)
            .ok_or_else(|| format!("{} ({name}): unsourced terrain radius", archetype.slug))?;
        assert!(
            row.1[0] < f(archetype.collision_radius),
            "{} ({name}): its row ({}) does not give a smaller inner radius",
            archetype.slug,
            row.2
        );
        assert!(!is_structure(archetype), "{}: a structure", archetype.slug);
        smaller += 1;
    }
    assert_eq!(smaller, 17, "the 17 ground analogues of plan §2.2");
    Ok(())
}

/// B-82: the Baneling's damage to a structure is its own effect, `VolatileBurstU2`: 80 (`liberty`
/// EffectData:1058), +5 a level (`ZergMeleeWeapons`, `liberty` UpgradeData:505), `ArmorReduction`
/// 0 (`liberty` EffectData:1063). No other modelled weapon has one.
#[test]
fn b82_only_the_baneling_has_its_structure_strike() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut found = 0;
    for archetype in catalogs.units.archetypes() {
        for (block, weapon) in weapon_blocks(archetype) {
            let Some(strike) = weapon.and_then(|w| w.structure_strike) else {
                continue;
            };
            assert_eq!(
                analogue(archetype),
                "Baneling",
                "{} {block}: only the Baneling has a structure strike",
                archetype.slug
            );
            assert_eq!(
                (
                    f(strike.damage),
                    f(strike.upgrade_per_level),
                    strike.ignores_armor
                ),
                (80.0, 5.0, true),
                "{} {block}: VolatileBurstU2 (liberty EffectData:1058, :1063; UpgradeData:505)",
                archetype.slug
            );
            found += 1;
        }
    }
    assert!(found > 0, "the Baneling analogue has its structure strike");
}

/// The analogues whose weapon carries `Options index="Melee"` (plan
/// `2026-11-10-spellcaster-mechanics` D-14, WP5a): no `CWeapon` default sets it (`core`
/// WeaponData:3-31) and no later layer clears it. The game weapons the catalog leaves out (the
/// Roach's and Hydralisk's melee weapons, `liberty` WeaponData:86-89 and :46-49, the
/// Ultralisk's `Ram`, :441-451) have no catalog counterpart; the analogues' ranged weapons are
/// not melee.
const SC2_MELEE_WEAPONS: [(&str, &str); 8] = [
    ("SCV", "FusionCutter, liberty WeaponData:201"),
    ("Probe", "ParticleBeam, liberty WeaponData:148"),
    ("Zealot", "PsiBlades, liberty WeaponData:158"),
    ("Dark Templar", "WarpBlades, liberty WeaponData:169"),
    ("Drone", "Spines, liberty WeaponData:355"),
    ("Zergling", "Claws, liberty WeaponData:363"),
    ("Baneling", "VolatileBurst, liberty WeaponData:472"),
    ("Ultralisk", "KaiserBlades, liberty WeaponData:439"),
];

#[test]
fn d14_melee_weapons_follow_starcraft() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut melee = Vec::new();
    for archetype in catalogs.units.archetypes() {
        let modes = replacing_mode_weapons(archetype).map(|weapon| ("siege_mode", Some(weapon)));
        for (block, weapon) in weapon_blocks(archetype).into_iter().chain(modes) {
            let Some(weapon) = weapon else {
                continue;
            };
            let row = SC2_MELEE_WEAPONS
                .iter()
                .find(|(unit, _)| *unit == analogue(archetype));
            assert_eq!(
                weapon.melee,
                row.is_some(),
                "{} {block} {}: melee per {}",
                archetype.slug,
                weapon.name,
                row.map_or("no Options Melee in its weapon", |(_, source)| source)
            );
            if weapon.melee {
                melee.push(analogue(archetype));
            }
        }
    }
    melee.sort_unstable();
    melee.dedup();
    let mut want: Vec<_> = SC2_MELEE_WEAPONS.iter().map(|(unit, _)| *unit).collect();
    want.sort_unstable();
    assert_eq!(melee, want, "every melee row has its analogue");
}

/// Where a weapon sets no `Arc`: no `CWeapon` default sets it (`core` WeaponData:3-31), so the
/// engine's 0 applies (catalog-time audit §10.3).
const ARC_UNSET: &str = "Arc not set: engine 0 (core WeaponData:3-31)";

/// Acquisition arc of every unit weapon whose mount matches the game: `Arc` ÷ 2 of the weapon,
/// in binary angle units (65 536 a turn; 32 768 is 180°, every direction). Plan
/// `2026-11-05-sc2-deviation-fixes` WP10, B-29; weapons per catalog-time audit §10.3. The row
/// with block `siege_mode` is the Siege Tank's Siege Mode cannon, checked through its mode.
const SC2_UNIT_ACQUIRE_ARC: [WeaponRow; 43] = [
    ("SCV", "weapon", 0.0, ARC_UNSET),
    ("Marine", "weapon", 0.0, ARC_UNSET),
    ("Marine", "air_weapon", 0.0, ARC_UNSET),
    ("Roach", "weapon", 0.0, ARC_UNSET),
    ("Zealot", "weapon", 0.0, ARC_UNSET),
    (
        "Siege Tank",
        "weapon",
        32_768.0,
        "Arc 360, voidmulti WeaponData:5",
    ),
    ("Siege Tank", "siege_mode", 0.0, ARC_UNSET),
    ("Marauder", "weapon", 0.0, ARC_UNSET),
    ("Reaper", "weapon", 0.0, ARC_UNSET),
    ("Ghost", "weapon", 0.0, ARC_UNSET),
    ("Thor", "weapon", 0.0, ARC_UNSET),
    ("Thor", "ground_weapon", 0.0, ARC_UNSET),
    (
        "Thor",
        "air_weapon",
        512.0,
        "LanceMissileLaunchers Arc 5.625, void WeaponData:82",
    ),
    ("Viking", "air_weapon", 0.0, ARC_UNSET),
    ("Liberator", "air_weapon", 0.0, ARC_UNSET),
    ("Banshee", "weapon", 0.0, ARC_UNSET),
    ("Probe", "weapon", 0.0, ARC_UNSET),
    ("Stalker", "weapon", 0.0, ARC_UNSET),
    ("Stalker", "air_weapon", 0.0, ARC_UNSET),
    ("Sentry", "weapon", 0.0, ARC_UNSET),
    ("Adept", "weapon", 0.0, ARC_UNSET),
    ("High Templar", "weapon", 0.0, ARC_UNSET),
    ("Dark Templar", "weapon", 0.0, ARC_UNSET),
    ("Void Ray", "weapon", 0.0, ARC_UNSET),
    ("Void Ray", "air_weapon", 0.0, ARC_UNSET),
    (
        "Carrier",
        "weapon",
        32_768.0,
        "Arc 360, liberty WeaponData:606",
    ),
    ("Drone", "weapon", 0.0, ARC_UNSET),
    ("Zergling", "weapon", 0.0, ARC_UNSET),
    ("Baneling", "weapon", 0.0, ARC_UNSET),
    ("Ravager", "weapon", 0.0, ARC_UNSET),
    ("Hydralisk", "weapon", 0.0, ARC_UNSET),
    ("Hydralisk", "air_weapon", 0.0, ARC_UNSET),
    ("Mutalisk", "weapon", 0.0, ARC_UNSET),
    ("Mutalisk", "air_weapon", 0.0, ARC_UNSET),
    ("Corruptor", "air_weapon", 0.0, ARC_UNSET),
    ("Brood Lord", "weapon", 0.0, ARC_UNSET),
    ("Brood Lord", "ground_weapon", 0.0, ARC_UNSET),
    ("Ultralisk", "weapon", 0.0, ARC_UNSET),
    ("Queen", "weapon", 0.0, ARC_UNSET),
    ("Queen", "ground_weapon", 0.0, ARC_UNSET),
    ("Queen", "air_weapon", 0.0, ARC_UNSET),
    (
        "Interceptor",
        "weapon",
        1_792.0,
        "InterceptorBeam Arc 19.6875, liberty WeaponData:626-637",
    ),
    (
        "Interceptor",
        "air_weapon",
        1_792.0,
        "InterceptorBeam Arc 19.6875, liberty WeaponData:626-637",
    ),
];

/// Acquisition arc of every structure weapon: `Arc` ÷ 2, binary angle units.
const SC2_STRUCTURE_ACQUIRE_ARC: [WeaponRow; 6] = [
    (
        "Planetary Fortress",
        "ground_weapon",
        8_192.0,
        "Arc 90, liberty WeaponData:138",
    ),
    ("Missile Turret", "air_weapon", 0.0, ARC_UNSET),
    ("Photon Cannon", "ground_weapon", 0.0, ARC_UNSET),
    ("Photon Cannon", "air_weapon", 0.0, ARC_UNSET),
    ("Spine Crawler", "ground_weapon", 0.0, ARC_UNSET),
    ("Spore Crawler", "air_weapon", 0.0, ARC_UNSET),
];

/// The analogues whose weapon mount the catalog-time audit (§10.5, B-28) found different: their
/// arcs are checked with their mounts, in [`SC2_WEAPON_MOUNTS`].
const MOUNT_DIFFERS: [&str; 5] = ["Hellion", "Immortal", "Colossus", "Lurker", "Phoenix"];

#[test]
fn b29_unit_acquisition_arcs_follow_starcraft() -> TestResult {
    let (siege, rows): (Vec<WeaponRow>, Vec<WeaponRow>) = SC2_UNIT_ACQUIRE_ARC
        .iter()
        .partition(|(_, block, ..)| *block == "siege_mode");
    check_weapons(
        "acquisition arc",
        false,
        &rows,
        &MOUNT_DIFFERS,
        |_| true,
        |w| f32::from(w.acquire_arc.raw()),
    )?;
    let [(_, _, want, source)] = siege[..] else {
        return Err("one Siege Mode row".into());
    };
    let mut checked = 0;
    for archetype in sim_fixtures::shipped_catalogs().units.archetypes() {
        for weapon in replacing_mode_weapons(archetype) {
            let got = f32::from(weapon.acquire_arc.raw());
            assert!(
                (got - want).abs() < 0.001,
                "{} {}: acquisition arc {got}, StarCraft II {want} ({source})",
                archetype.slug,
                weapon.name
            );
            checked += 1;
        }
    }
    assert_eq!(
        checked, 1,
        "one replacing weapon override: the Siege Tank's Siege Mode"
    );
    Ok(())
}

#[test]
fn b29_structure_acquisition_arcs_follow_starcraft() -> TestResult {
    check_weapons(
        "acquisition arc",
        true,
        &SC2_STRUCTURE_ACQUIRE_ARC,
        &[],
        |_| true,
        |w| f32::from(w.acquire_arc.raw()),
    )
}

/// One weapon mount: analogue, weapon block, the turret's `YawRate` in degrees per tick (`None`
/// for a hull weapon), the engaged and the acquisition arcs as half angles in degrees, and the
/// source.
type MountRow = (
    &'static str,
    &'static str,
    Option<f32>,
    f32,
    f32,
    &'static str,
);

/// The weapons whose mount the catalog-time audit (§10.5) found different from the game, as
/// `StarCraft` II 5.0.14 has them (B-28). Turret rates are Normal °/s × 1.4 ÷ 25; arcs are
/// `Arc` ÷ 2 + `ArcSlop` (engaged) and `Arc` ÷ 2 (acquisition), `ArcSlop` 11.25 by default
/// (`core` WeaponData:15).
const SC2_WEAPON_MOUNTS: [MountRow; 6] = [
    (
    "Hellion",
    "weapon",
    Some(55.99),
    11.25,
    0.0,
    "Hellion turret, liberty UnitData:11603; YawRate 999.8437, liberty TurretData:97; \
     InfernalFlameThrower sets no Arc",
),
    ("Immortal", "weapon", Some(40.32), 11.25, 0.0, "Immortal turret, liberty UnitData:10791; parent FreeRotate, YawRate 720, liberty TurretData:26, :73; PhaseDisruptors sets no Arc"),
    ("Colossus", "weapon", Some(55.99), 56.25, 45.0, "Colossus turret, liberty UnitData:4099; YawRate 999.8437, liberty TurretData:51; ThermalLances Arc 90, liberty WeaponData:500"),
    ("Lurker", "weapon", Some(156.79), 11.25, 0.0, "Lurker turret of LurkerMPBurrowed, void UnitData:775; YawRate 2799.8437, void TurretData:18; LurkerMP sets no Arc (swarm WeaponData:215, void :14, voidmulti :144)"),
    ("Phoenix", "air_weapon", Some(30.20), 2.4994, 2.4994, "Phoenix turret, swarm UnitData:1805; YawRate 539.2968, swarm TurretData:33; IonCannons Arc 4.9987, ArcSlop 0, swarm WeaponData:179-180"),
    ("Thor", "air_weapon", None, 14.0625, 2.8125, "a hull weapon: the Thor's WeaponArray names no Turret, liberty UnitData:5270-5271; LanceMissileLaunchers Arc 5.625, void WeaponData:82"),
];

/// Whether two angles in degrees agree to within one binary unit (360 ÷ 65 536 degrees).
fn same_angle(got: f32, want: f32) -> bool {
    (got - want).abs() <= 360.0 / 65_536.0
}

#[test]
fn b28_weapon_mounts_follow_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    for (unit, block, rate, engaged, acquisition, source) in SC2_WEAPON_MOUNTS {
        let archetype = catalogs
            .units
            .archetypes()
            .find(|archetype| analogue(archetype) == unit)
            .ok_or(unit)?;
        let (_, weapon) = weapon_blocks(archetype)
            .into_iter()
            .find(|(b, _)| *b == block)
            .ok_or(block)?;
        let weapon = weapon.ok_or(block)?;
        let got = (
            weapon
                .has_turret
                .then(|| weapon.turret_turn_rate.to_degrees_f32()),
            weapon.firing_arc.to_degrees_f32(),
            weapon.acquire_arc.to_degrees_f32(),
        );
        let matches = match (got.0, rate) {
            (Some(got_rate), Some(want_rate)) => same_angle(got_rate, want_rate),
            (None, None) => true,
            _ => false,
        } && same_angle(got.1, engaged)
            && same_angle(got.2, acquisition);
        assert!(
            matches,
            "{} ({unit}) {block}: turret, engaged, acquisition {got:?}, StarCraft II \
             {rate:?}, {engaged}, {acquisition} ({source})",
            archetype.slug
        );
    }
    Ok(())
}

/// Idle behaviour (`CTurret` `Idle`, `YawIdleRate` in degrees per tick = Normal °/s × 1.4 ÷ 25)
/// of every turreted weapon, by analogue and block; `siege_mode` is the Siege Tank's Siege Mode
/// cannon. Every turret here descends from `FreeRotate`, `Idle` `Hold` (`liberty`
/// TurretData:24), unless its row says otherwise (B-30). A `ResetIfMoving` turret with no
/// `YawIdleRate` in any layer (no `CTurret` default sets it, `core` TurretData:3-10) returns at
/// its `YawRate` (owner answer to Q0065).
const SC2_TURRET_IDLE: [(&str, &str, TurretIdleRef, &str); 13] = [
    (
        "Siege Tank",
        "weapon",
        TurretIdleRef::ResetIfMoving(40.32),
        "SiegeTank, liberty TurretData:77-79, YawRate 720 from FreeRotate; no YawIdleRate, returns at YawRate (Q0065)",
    ),
    (
        "Siege Tank",
        "siege_mode",
        TurretIdleRef::Hold,
        "SiegeTankSieged, liberty TurretData:80-83",
    ),
    (
        "Hellion",
        "weapon",
        TurretIdleRef::ResetIfMoving(55.99),
        "Hellion, liberty TurretData:95-98, YawRate 999.8437; no YawIdleRate, returns at YawRate (Q0065)",
    ),
    (
        "Immortal",
        "weapon",
        TurretIdleRef::FollowMovement(20.16),
        "FollowMovement, YawIdleRate 360, liberty TurretData:74-75",
    ),
    (
        "Colossus",
        "weapon",
        TurretIdleRef::FollowMovement(5.04),
        "FollowMovement, YawIdleRate 90, liberty TurretData:50-52",
    ),
    (
        "Lurker",
        "weapon",
        TurretIdleRef::Hold,
        "Lurker turret, Hold, void TurretData:16",
    ),
    (
        "Phoenix",
        "air_weapon",
        TurretIdleRef::ResetIfMoving(30.20),
        "Phoenix, swarm TurretData:31-39, YawRate 539.2968; no YawIdleRate, returns at YawRate (Q0065)",
    ),
    (
        "Planetary Fortress",
        "ground_weapon",
        TurretIdleRef::Hold,
        "PlanetaryFortress, liberty TurretData:88-90",
    ),
    (
        "Missile Turret",
        "air_weapon",
        TurretIdleRef::Spin(2.52),
        "Spin, YawIdleRate 45, liberty TurretData:85-86",
    ),
    (
        "Photon Cannon",
        "ground_weapon",
        TurretIdleRef::Hold,
        "PhotonCannon, liberty TurretData:48",
    ),
    (
        "Photon Cannon",
        "air_weapon",
        TurretIdleRef::Hold,
        "PhotonCannon, liberty TurretData:48",
    ),
    (
        "Spine Crawler",
        "ground_weapon",
        TurretIdleRef::Hold,
        "SpineCrawler, liberty TurretData:61-63",
    ),
    (
        "Spore Crawler",
        "air_weapon",
        TurretIdleRef::Hold,
        "SporeCrawler, liberty TurretData:69-72 (a YawIdleRate under Hold)",
    ),
];

/// A reference idle behaviour, rates in degrees per tick.
#[derive(Clone, Copy, Debug)]
enum TurretIdleRef {
    Hold,
    ResetIfMoving(f32),
    FollowMovement(f32),
    Spin(f32),
}

impl TurretIdleRef {
    /// Whether the catalog's `idle` is this behaviour, rates to one binary unit.
    fn matches(self, idle: TurretIdle) -> bool {
        let rate = |angle: SimAngle| angle.to_degrees_f32();
        match (self, idle) {
            (Self::Hold, TurretIdle::Hold) => true,
            (Self::ResetIfMoving(want), TurretIdle::ResetIfMoving { rate: got })
            | (Self::FollowMovement(want), TurretIdle::FollowMovement { rate: got })
            | (Self::Spin(want), TurretIdle::Spin { step: got }) => same_angle(rate(got), want),
            _ => false,
        }
    }
}

#[test]
fn b30_turret_idle_follows_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut checked = 0;
    for archetype in catalogs.units.archetypes() {
        let name = analogue(archetype);
        let modes = replacing_mode_weapons(archetype).map(|weapon| ("siege_mode", Some(weapon)));
        for (block, weapon) in weapon_blocks(archetype).into_iter().chain(modes) {
            let Some(weapon) = weapon.filter(|w| w.has_turret) else {
                continue;
            };
            // A structure's `weapon`, and a unit's ground weapon left to default, copy another.
            let derived = block == "ground_weapon" && archetype.weapon.as_ref() == Some(weapon);
            if (is_structure(archetype) && block == "weapon")
                || (!is_structure(archetype) && derived)
            {
                continue;
            }
            let (_, _, want, source) = SC2_TURRET_IDLE
                .iter()
                .find(|(unit, b, ..)| *unit == name && *b == block)
                .ok_or_else(|| format!("{} ({name}) {block}: no idle row", archetype.slug))?;
            assert!(
                want.matches(weapon.turret_idle),
                "{} ({name}) {block}: idle {:?}, StarCraft II {want:?} ({source})",
                archetype.slug,
                weapon.turret_idle
            );
            checked += 1;
        }
    }
    assert_eq!(checked, SC2_TURRET_IDLE.len(), "every row has its turret");
    Ok(())
}

/// The random firing delay of every legacy weapon, `(RandomDelayMin, RandomDelayMax)` in ticks:
/// −0.0625 and 0.125 Normal s (`core` WeaponData:35–36, the `CWeaponLegacy` default) × 25 ÷ 1.4.
/// The 5.0.14 weapons that override it (`liberty` WeaponData:76–77 `PointDefenseLaser`, :285 and
/// :297 the Battlecruiser's batteries, :336–337 `D8Charge`; `voidmulti` WeaponData:22–40,
/// :68–69, :151–152, :165–166, :281–282: the Battlecruiser, Mothership Core, Oracle and
/// `BypassArmorDroneWeapon`) belong to no shipped analogue, so every shipped weapon has it (B-3).
const SC2_RANDOM_DELAY_TICKS: (f32, f32) = (-1.1161, 2.2321);

#[test]
fn b3_every_weapon_has_the_legacy_random_delay() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut checked = 0;
    for archetype in catalogs.units.archetypes() {
        let modes = replacing_mode_weapons(archetype);
        let blocks = weapon_blocks(archetype).into_iter().filter_map(|(_, w)| w);
        for weapon in blocks.chain(modes) {
            let got = (f(weapon.random_delay_min), f(weapon.random_delay_max));
            let want = SC2_RANDOM_DELAY_TICKS;
            assert!(
                (got.0 - want.0).abs() < 0.001 && (got.1 - want.1).abs() < 0.001,
                "{} {}: random delay {got:?} ticks, StarCraft II {want:?} (core WeaponData:35-36)",
                archetype.slug,
                weapon.name
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "every weapon is checked: {checked}");
}

/// Siege Mode's channels, ticks (Normal s × 25 ÷ 1.4, rounded half up): entering is the
/// `SiegeMode` morph's longest section, `Stats` delay 0.5 plus duration 3.5417 (the sections'
/// greatest sum is the morph's time, `SC2Mapster` `CAbilMorph` reference), 4.0417 s = 72; leaving
/// is `Unsiege`'s 3.5417 s = 63; each adds up to `RandomDelayMax` 0.5 s = 8.9286 ticks
/// (`liberty` AbilData:1301-1341; no later layer changes a time: `swarm` AbilData:351, :1353,
/// `void` :805, `voidmulti` :915, :1112).
const SC2_SIEGE_MODE: (u16, u16, f32, f32) = (72, 63, 8.9286, 8.9286);

#[test]
fn b4_siege_mode_timings_follow_starcraft() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let modes: Vec<_> = catalogs
        .units
        .archetypes()
        .filter(|arch| analogue(arch) == "Siege Tank")
        .flat_map(|arch| arch.modes.iter())
        .collect();
    assert_eq!(modes.len(), 1, "the Siege Tank analogue has Siege Mode");
    let mode = modes[0];
    let (cast, leave, cast_extra, leave_extra) = SC2_SIEGE_MODE;
    assert_eq!((mode.cast_ticks, mode.leave_cast_ticks), (cast, leave));
    for (got, want) in [
        (mode.cast_random_delay_max_ticks, cast_extra),
        (mode.leave_random_delay_max_ticks, leave_extra),
    ] {
        assert!(
            (f(got) - want).abs() < 0.001,
            "extra {got:?}, StarCraft II {want}"
        );
    }
}

/// The plain orders of each shipped anchoring stance, `(analogue, stop, hold_position)` (plan
/// `2026-11-23-anchored-commands` D-1, P-4). A stance accepts Stop when its unit type carries the
/// `stop` ability and Hold Position when it carries `move`, whose `HoldPos` command it is (`core`
/// AbilData:552-554, :1362, :1371). `SiegeTankSieged` has `stop`, `attack` and `Unsiege` and no
/// `move` (`liberty` UnitData:5172-5174; no later layer sets its `AbilArray`); `LurkerMPBurrowed`
/// has `move` and `stop` (`liberty` UnitData:793-796, `void` UnitData:767-769).
const SC2_STANCE_ORDERS: [(&str, bool, bool, &str); 2] = [
    (
        "Siege Tank",
        true,
        false,
        "liberty UnitData:5172-5174 (SiegeTankSieged)",
    ),
    (
        "Lurker",
        true,
        true,
        "liberty UnitData:793-796, void UnitData:767-769 (LurkerMPBurrowed)",
    ),
];

#[test]
fn b98_anchoring_stances_accept_the_orders_of_their_starcraft_unit_type() {
    let catalogs = sim_fixtures::shipped_catalogs();
    for (name, stop, hold_position, source) in SC2_STANCE_ORDERS {
        let modes: Vec<_> = catalogs
            .units
            .archetypes()
            .filter(|arch| analogue(arch) == name)
            .flat_map(|arch| arch.modes.iter())
            .filter(|mode| mode.is_immobile)
            .collect();
        assert_eq!(modes.len(), 1, "the {name} analogue has one anchoring mode");
        let orders = modes[0].orders;
        assert_eq!(
            (orders.stop, orders.hold_position),
            (stop, hold_position),
            "{name}: Stop and Hold Position, StarCraft II {source}"
        );
    }
}

/// The default stance's plain orders of the structures whose `AbilArray` 5.0.14 sources, as
/// `(analogue, stop, hold_position, source)` (plan `2026-11-23-stance-checked-orders` D-2, P-4,
/// B-171). Armed structures carry `stop` and no `move`; unarmed ones carry neither. The Shield
/// Battery's `stop` (its `liberty` entry 2) is removed in 5.0.14; the Nexus's requires
/// `PurifyNexusRequirements`, unmet in multiplayer; the Bunker's redirects to passengers it
/// does not carry here.
const SC2_STRUCTURE_ORDERS: [(&str, bool, bool, &str); 10] = [
    ("Photon Cannon", true, false, "liberty UnitData:11083-11084"),
    ("Missile Turret", true, false, "liberty UnitData:6064-6065"),
    (
        "Planetary Fortress",
        true,
        false,
        "liberty UnitData:10696-10697",
    ),
    ("Spine Crawler", true, false, "liberty UnitData:8652-8653"),
    ("Spore Crawler", true, false, "liberty UnitData:8784-8785"),
    (
        "Command Center",
        false,
        false,
        "liberty UnitData:10411-10418",
    ),
    (
        "Orbital Command",
        false,
        false,
        "liberty UnitData:10557-10564",
    ),
    (
        "Nexus",
        false,
        false,
        "liberty UnitData:4218-4223; swarm AbilData:954-960, :968-970 (PurifyNexusRequirements)",
    ),
    (
        "Shield Battery",
        false,
        false,
        "liberty UnitData:1603-1605; voidmulti UnitData:1557; balancemulti UnitData:1463 (removed)",
    ),
    (
        "Bunker",
        false,
        false,
        "liberty UnitData:6192-6193; AbilData:3606-3611, :3633-3638 (passenger redirects)",
    ),
];

#[test]
fn b171_structures_take_the_plain_orders_of_their_starcraft_unit_type() {
    let catalogs = sim_fixtures::shipped_catalogs();
    for (name, stop, hold_position, source) in SC2_STRUCTURE_ORDERS {
        let structures: Vec<_> = catalogs
            .units
            .archetypes()
            .filter(|arch| is_structure(arch) && analogue(arch) == name)
            .collect();
        assert_eq!(structures.len(), 1, "one {name} analogue");
        let orders = structures[0].stance_orders(None);
        assert_eq!(
            (orders.stop, orders.hold_position),
            (stop, hold_position),
            "{name}: Stop and Hold Position, StarCraft II {source}"
        );
    }
}

/// The A command of each sourced stance, as `(analogue, mode, command, source)` (plan
/// `2026-11-24-attack-orders` D-1, P-1; `mode` is `None` for the default stance). Every unarmed
/// mobile unit puts the `move` ability's `AcquireMove` (Scan Move) in its Attack slot (row 0,
/// column 4) and carries no `attack` (S6); the unburrowed Lurker carries `attack` without a
/// weapon (S7); the sieged tank and the burrowed Lurker carry `attack` (S10); the armed
/// structures carry `attack` and the unarmed ones none that a multiplayer game enables (plan
/// `2026-11-23-stance-checked-orders` S6, S7, S9, S10).
const SC2_ATTACK_COMMANDS: [(&str, Option<&str>, AttackCommand, &str); 20] = [
    (
        "Medivac",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:5475-5478, :5485-5486",
    ),
    (
        "Raven",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:5621-5622; voidmulti :2653-2654; balancemulti :2490-2491",
    ),
    (
        "Observer",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:3443-3445, :3452-3453",
    ),
    (
        "Warp Prism",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:3859-3860",
    ),
    (
        "Infestor",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:10108-10109; balancemulti :740-741",
    ),
    (
        "Viper",
        None,
        AttackCommand::ScanMove,
        "swarm UnitData:2363-2364",
    ),
    (
        "Overlord",
        None,
        AttackCommand::ScanMove,
        "liberty UnitData:7391-7395, :7400-7401",
    ),
    (
        "Lurker",
        None,
        AttackCommand::Attack,
        "liberty UnitData:727-729, :735-736; voidmulti UnitData:974; balancemulti UnitData:909 (LurkerMP)",
    ),
    (
        "Lurker",
        Some("burrowed"),
        AttackCommand::Attack,
        "liberty UnitData:793-796 (LurkerMPBurrowed)",
    ),
    (
        "Siege Tank",
        Some("siege_mode"),
        AttackCommand::Attack,
        "liberty UnitData:5172-5174 (SiegeTankSieged)",
    ),
    (
        "Photon Cannon",
        None,
        AttackCommand::Attack,
        "liberty UnitData:11083-11084",
    ),
    (
        "Missile Turret",
        None,
        AttackCommand::Attack,
        "liberty UnitData:6064-6065",
    ),
    (
        "Planetary Fortress",
        None,
        AttackCommand::Attack,
        "liberty UnitData:10696-10697",
    ),
    (
        "Spine Crawler",
        None,
        AttackCommand::Attack,
        "liberty UnitData:8652-8653",
    ),
    (
        "Spore Crawler",
        None,
        AttackCommand::Attack,
        "liberty UnitData:8784-8785",
    ),
    (
        "Command Center",
        None,
        AttackCommand::None,
        "liberty UnitData:10411-10418",
    ),
    (
        "Orbital Command",
        None,
        AttackCommand::None,
        "liberty UnitData:10557-10564",
    ),
    (
        "Nexus",
        None,
        AttackCommand::None,
        "liberty UnitData:4218-4223; swarm AbilData:968-970 (PurifyNexusRequirements)",
    ),
    (
        "Shield Battery",
        None,
        AttackCommand::None,
        "liberty UnitData:1603-1605; balancemulti UnitData:1463",
    ),
    (
        "Bunker",
        None,
        AttackCommand::None,
        "liberty UnitData:6192-6193; AbilData:3606-3611 (passenger redirect)",
    ),
];

/// Plan `2026-11-24-attack-orders` P-1 (B-174, B-186): every sourced stance's A command, read
/// through `UnitArchetype::stance_orders` as the simulation and the client read it.
#[test]
fn b186_stances_have_the_a_command_of_their_starcraft_unit_type() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    for (name, mode, command, source) in SC2_ATTACK_COMMANDS {
        let archetypes: Vec<_> = catalogs
            .units
            .archetypes()
            .filter(|arch| analogue(arch) == name)
            .collect();
        assert_eq!(archetypes.len(), 1, "one {name} analogue");
        let arch = archetypes[0];
        let stance = match mode {
            Some(mode) => Some(
                arch.get_mode(mode)
                    .ok_or_else(|| format!("the {name} analogue has its mode {mode}"))?,
            ),
            None => None,
        };
        assert_eq!(
            arch.stance_orders(stance).attack,
            command,
            "{name} {mode:?}: the A command, StarCraft II {source}"
        );
    }
    Ok(())
}

/// Plan `2026-11-24-attack-orders` D-1, P-1: the default stance's A command is derived (Attack
/// when armed, Scan Move when mobile and unarmed, none otherwise) for every shipped archetype
/// but the one that authors its default stance, and only the Lurker analogue does (S7). Every
/// shipped mode authors Attack, and every one of them is armed (S8, S10).
#[test]
fn the_a_command_is_derived_for_every_archetype_but_the_lurker_analogue() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut authored = Vec::new();
    for arch in catalogs.units.archetypes() {
        let derived = if arch.stance_armed(None) {
            AttackCommand::Attack
        } else if arch.is_immobile_by_default() {
            AttackCommand::None
        } else {
            AttackCommand::ScanMove
        };
        match arch.default_stance_orders {
            Some(orders) => {
                authored.push(analogue(arch).to_owned());
                assert_eq!(arch.stance_orders(None), orders, "{}", arch.slug);
            }
            None => assert_eq!(arch.stance_orders(None).attack, derived, "{}", arch.slug),
        }
        for mode in &arch.modes {
            assert!(arch.stance_armed(Some(mode)), "{}", mode.mode_name);
            assert_eq!(
                mode.orders.attack,
                AttackCommand::Attack,
                "{}",
                mode.mode_name
            );
        }
    }
    assert_eq!(authored, ["Lurker"]);
}

/// Unit `Acceleration` (`CUnit`, Normal units per second squared) at 5.0.14, plan
/// `2026-11-05-sc2-deviation-fixes` WP12 (B-39). `1000` is instant.
const SC2_ACCELERATION: [(&str, f32, &str); 43] = [
    ("SCV", 2.5, "liberty UnitData:4732"),
    ("Marine", 1000.0, "liberty UnitData:4822"),
    ("Roach", 1000.0, "liberty UnitData:7188"),
    ("Zealot", 1000.0, "liberty UnitData:3179"),
    ("Siege Tank", 1000.0, "liberty UnitData:5076"),
    ("Phoenix", 3.25, "liberty UnitData:3686"),
    ("Marauder", 1000.0, "liberty UnitData:10969"),
    ("Reaper", 1000.0, "liberty UnitData:4892"),
    ("Ghost", 1000.0, "liberty UnitData:4991"),
    ("Hellion", 1000.0, "liberty UnitData:11552"),
    ("Thor", 1000.0, "liberty UnitData:5211"),
    ("Viking", 2.625, "liberty UnitData:6839"),
    ("Medivac", 2.25, "swarm UnitData:1706"),
    ("Liberator", 3.5, "void UnitData:3297"),
    ("Banshee", 3.25, "liberty UnitData:5377"),
    ("Raven", 2.0, "liberty UnitData:5579"),
    ("Probe", 2.5, "liberty UnitData:3075"),
    ("Stalker", 1000.0, "liberty UnitData:3964"),
    ("Sentry", 1000.0, "liberty UnitData:11304"),
    ("Adept", 1000.0, "void UnitData:3201"),
    ("High Templar", 1000.0, "liberty UnitData:3258"),
    ("Dark Templar", 1000.0, "liberty UnitData:3354"),
    ("Immortal", 1000.0, "liberty UnitData:10738"),
    ("Colossus", 1000.0, "liberty UnitData:4043"),
    ("Observer", 2.125, "liberty UnitData:3423"),
    ("Warp Prism", 2.625, "voidmulti UnitData:2043"),
    ("Void Ray", 2.0, "liberty UnitData:3758"),
    ("Carrier", 1.0625, "liberty UnitData:3483 (Carrier)"),
    ("Drone", 2.5, "liberty UnitData:7041"),
    ("Zergling", 1000.0, "liberty UnitData:7554"),
    ("Baneling", 1000.0, "liberty UnitData:9311"),
    ("Ravager", 1000.0, "liberty UnitData:995"),
    ("Hydralisk", 1000.0, "liberty UnitData:7675"),
    ("Lurker", 1000.0, "liberty UnitData:714"),
    ("Mutalisk", 3.5, "void UnitData:3092"),
    ("Corruptor", 3.0, "voidmulti UnitData:269"),
    ("Brood Lord", 1.0625, "liberty UnitData:11113"),
    ("Ultralisk", 1000.0, "liberty UnitData:8015"),
    ("Queen", 1000.0, "liberty UnitData:11420"),
    ("Infestor", 1000.0, "liberty UnitData:10062"),
    ("Viper", 3.0, "void UnitData:1417"),
    ("Overlord", 1.0625, "liberty UnitData:7366"),
    (
        "Interceptor",
        1000.0,
        "liberty UnitData:3559 (Acceleration 1000, instant)",
    ),
];

/// Normal units per second squared to world units per tick per tick: one tick is 1.4 ÷ 25
/// Normal seconds, so a rate scales by (1.4 ÷ 25)².
fn per_tick_squared(normal: f32) -> f32 {
    normal * 1.4 * 1.4 / (25.0 * 25.0)
}

/// Checks one rate of every mobile archetype against `rows` within 0.00001 per tick²: the
/// rates are two orders of magnitude below `check_table`'s tolerance.
fn check_rates(
    what: &str,
    rows: &[(&str, f32, &str)],
    read: impl Fn(&UnitArchetype) -> f32,
) -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = 0;
    for archetype in catalogs.units.archetypes() {
        if is_structure(archetype) {
            continue;
        }
        let name = analogue(archetype);
        let Some(&(_, normal, source)) = rows.iter().find(|(unit, ..)| *unit == name) else {
            return Err(format!("{} ({name}): no {what} row", archetype.slug).into());
        };
        let (got, want) = (read(archetype), per_tick_squared(normal));
        assert!(
            (got - want).abs() < 0.00001,
            "{} ({name}): {what} is {got}, StarCraft II {normal} Normal = {want} ({source})",
            archetype.slug
        );
        seen += 1;
    }
    assert_eq!(seen, rows.len(), "{what}: every row has one archetype");
    Ok(())
}

#[test]
fn b39_acceleration_follows_starcraft() -> TestResult {
    check_rates("acceleration", &SC2_ACCELERATION, |arch| {
        f(arch.acceleration)
    })
}

/// The analogues whose data sets `Deceleration` (Normal units per second squared). Every other
/// analogue's is unset, which the engine reads as its `Acceleration` (owner answer to Q0067): its
/// catalog value copies the converted `acceleration`.
const SC2_DECELERATION: [(&str, f32, &str); 2] = [
    ("High Templar", 1000.0, "liberty UnitData:3259"),
    ("Overlord", 1.625, "voidmulti UnitData:1199"),
];

#[test]
fn b39_deceleration_follows_starcraft() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = 0;
    for arch in catalogs.units.archetypes().filter(|a| !is_structure(a)) {
        let name = analogue(arch);
        let (got, want, source) = match SC2_DECELERATION.iter().find(|(unit, ..)| *unit == name) {
            Some(&(_, normal, source)) => {
                seen += 1;
                (f(arch.deceleration), per_tick_squared(normal), source)
            }
            None => (
                f(arch.deceleration),
                f(arch.acceleration),
                "unset: the Acceleration (Q0067)",
            ),
        };
        assert!(
            (got - want).abs() < 0.00001,
            "{} ({name}): deceleration {got}, expected {want} ({source})",
            arch.slug
        );
    }
    assert_eq!(seen, SC2_DECELERATION.len());
}

/// `PushPriority` of every mobile analogue (B-48, plan `2026-11-09-friendly-blocking` WP5).
/// `CUnit` sets 10 (`core` UnitData:59) and no layer overrides it for a modelled unit: only
/// `Critter` (5, `liberty` UnitData:156) and `SlaynElemental` (11, `void` UnitData:3569) do. The
/// rows below are the exceptions; every other mobile analogue must hold the default.
const SC2_PUSH_PRIORITY_DEFAULT: (u8, &str) = (10, "CUnit PushPriority, core UnitData:59");

/// The analogues whose priority differs from the default. The Ultralisk's 5.0.14
/// `AlliedPushPriority` 1 (`voidmulti` UnitData:1943; 5.0.14 patch notes: "now pushes allied units
/// out of the way when moving") is added to its `PushPriority` 10 (Q0073's default, option 1).
const SC2_PUSH_PRIORITY: [(&str, u8, &str); 1] = [(
    "Ultralisk",
    11,
    "PushPriority 10 + AlliedPushPriority 1, voidmulti UnitData:1943 (Q0073)",
)];

#[test]
fn b48_push_priorities_follow_starcraft() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut seen = 0;
    for arch in catalogs.units.archetypes().filter(|a| !is_structure(a)) {
        let name = analogue(arch);
        let (want, source) = match SC2_PUSH_PRIORITY.iter().find(|(unit, ..)| *unit == name) {
            Some(&(_, priority, source)) => {
                seen += 1;
                (priority, source)
            }
            None => SC2_PUSH_PRIORITY_DEFAULT,
        };
        assert_eq!(
            arch.push_priority, want,
            "{} ({name}): push priority {}, StarCraft II {want} ({source})",
            arch.slug, arch.push_priority
        );
    }
    assert_eq!(seen, SC2_PUSH_PRIORITY.len(), "every row has an archetype");
}

/// Asserts `value` equals `want` to 0.001 after the catalog's fixed-point conversion.
fn assert_close(value: SimScalar, want: f32, source: &str) {
    assert!(
        (f(value) - want).abs() < 0.001,
        "{} is not {want} ({source})",
        f(value)
    );
}

/// Psionic Storm as authored (plan `2026-11-10-spellcaster-mechanics` §3.1, §5): the High Templar
/// analogue's ability, its effect tree, its status and the research that unlocks it, each value
/// cited from `SC2GameData` 5.0.14 in the RON files.
#[test]
#[allow(clippy::too_many_lines)]
fn psionic_storm_values_follow_starcraft() -> TestResult {
    use sim_core::catalog::EffectNode;
    use sim_types::{AbilityEffectSpec, AbilityTargetKind, EffectAnchor, StructureRule};

    let catalogs = sim_fixtures::shipped_catalogs();
    let caster = catalogs
        .units
        .archetypes()
        .find(|archetype| analogue(archetype) == "High Templar")
        .ok_or("the High Templar analogue")?;
    let ability = caster
        .abilities
        .iter()
        .find(|ability| ability.name == "Psionic Storm")
        .ok_or("Psionic Storm")?;
    assert_eq!(ability.target_kind, AbilityTargetKind::TargetPosition);
    assert_close(ability.energy_cost, 75.0, "liberty AbilData:2067");
    assert_eq!(
        ability.cooldown_ticks, 36,
        "2 Normal s, swarm AbilData:1111"
    );
    assert_close(ability.cast_range, 9.0, "liberty AbilData:2061");
    assert_eq!(
        ability.cast_backswing_ticks, 9,
        "0.5 Normal s, liberty AbilData:2062"
    );
    assert!(ability.single_caster && ability.target_filter.is_none() && ability.origin.is_none());
    let AbilityEffectSpec::Effect(root) = &ability.effect else {
        return Err("Psionic Storm runs an effect tree".into());
    };

    let spells = catalogs.units.spells();
    let node = |id| spells.node(id).map(|node| node.kind).ok_or("a node");
    let root = spells.effect_id(root).ok_or("the root")?;
    let EffectNode::Persistent {
        anchor: EffectAnchor::TargetPoint,
        initial: Some(initial),
        periodic: Some(periodic),
        period_ticks,
        count: 6,
    } = node(root)?
    else {
        return Err("a point persistent of six pulses, liberty EffectData:2072".into());
    };
    assert_eq!(
        initial, periodic,
        "one search, liberty EffectData:2071, :2073"
    );
    assert_close(
        period_ticks,
        10.2,
        "0.5712 Normal s, liberty EffectData:2074",
    );
    let EffectNode::Search {
        radius,
        filter,
        max_count: None,
        child,
    } = node(initial)?
    else {
        return Err("the persistent searches".into());
    };
    assert_close(radius, 1.5, "liberty EffectData:2080");
    assert!(filter.ground && filter.air, "liberty EffectData:2079");
    assert_eq!(filter.structures, StructureRule::Exclude);
    assert_eq!(filter.alliance, sim_types::FilterAlliance::Any);
    let EffectNode::ApplyStatus { status } = node(child)? else {
        return Err("the search applies a status".into());
    };
    let status = spells.status(status).ok_or("the status")?;
    assert_close(
        status.spec.duration_ticks,
        11.901_786,
        "0.6665 Normal s, liberty BehaviorData:713",
    );
    assert_close(
        status.spec.period_ticks.ok_or("a period")?,
        10.196_429,
        "0.571 Normal s, liberty BehaviorData:714",
    );
    let EffectNode::Damage {
        amount,
        responses,
        visibility,
        ..
    } = node(status.initial_effect.ok_or("initial damage")?)?
    else {
        return Err("the initial effect is damage".into());
    };
    assert_close(amount, 10.0, "liberty EffectData:2091");
    assert_eq!(
        visibility,
        AttackVisibility::Snapshot,
        "PsiStormDamageInitial names no Visibility: the CEffectDamage default, core EffectData:62"
    );
    assert!(
        responses.acquire && responses.flee,
        "liberty EffectData:2093-2094"
    );
    let EffectNode::Damage {
        amount,
        responses,
        visibility,
        ..
    } = node(status.periodic_effect.ok_or("periodic damage")?)?
    else {
        return Err("the periodic effect is damage".into());
    };
    assert_close(amount, 10.0, "liberty EffectData:2100");
    assert_eq!(
        visibility,
        AttackVisibility::Hidden,
        "PsiStormDamage Visibility Hidden, liberty EffectData:2099"
    );
    assert!(
        !responses.acquire && !responses.flee,
        "no responses on a pulse"
    );

    let research = catalogs
        .upgrades
        .get_by_slug("psionic_storm")
        .ok_or("the research")?;
    assert!(research.effects.iter().any(|effect| matches!(
        effect,
        UpgradeEffect::UnlockAbility { target_slug, ability_slug }
            if *target_slug == caster.slug && ability_slug == "Psionic Storm"
    )));
    Ok(())
}

/// Abduct as authored (plan `2026-11-10-spellcaster-mechanics` §3.2, §5): the Viper analogue's
/// ability, its target filter and origin placement, its effect tree, the cast's mark (bug
/// B-114) and the `abducted` status, each value cited from `SC2GameData` 5.0.14 in the RON
/// files.
#[test]
#[allow(clippy::too_many_lines)]
fn abduct_values_follow_starcraft() -> TestResult {
    use sim_core::catalog::EffectNode;
    use sim_types::{
        AbilityEffectSpec, AbilityTargetKind, EffectAnchor, FilterAlliance, MissileView,
        StructureRule, TargetPlane,
    };

    let catalogs = sim_fixtures::shipped_catalogs();
    let caster = catalogs
        .units
        .archetypes()
        .find(|archetype| analogue(archetype) == "Viper")
        .ok_or("the Viper analogue")?;
    let ability = caster
        .abilities
        .iter()
        .find(|ability| ability.name == "Abduct")
        .ok_or("Abduct")?;
    assert_eq!(ability.target_kind, AbilityTargetKind::TargetEntity);
    assert_close(ability.energy_cost, 75.0, "swarm AbilData:1230");
    assert_eq!(
        ability.cooldown_ticks, 0,
        "no cooldown, swarm AbilData:1222-1239"
    );
    assert_close(ability.cast_range, 9.0, "swarm AbilData:1233");
    assert_eq!(
        ability.cast_backswing_ticks, 14,
        "0.8 Normal s, voidmulti AbilData:1754"
    );
    assert!(ability.single_caster);
    let filter = ability.target_filter.as_ref().ok_or("a target filter")?;
    assert_eq!(
        filter.alliance,
        FilterAlliance::AnyButSelf,
        "voidmulti AbilData:1755"
    );
    assert!(
        filter.planes.contains(&TargetPlane::Ground) && filter.planes.contains(&TargetPlane::Air)
    );
    assert_eq!(filter.structures, StructureRule::Exclude);
    assert_eq!(
        filter.without_status.as_deref(),
        Some("abduct_mark"),
        "NoYoink counts YoinkMarker, swarm ValidatorData:812-815"
    );
    let origin = ability.origin.ok_or("origin placement")?;
    assert_close(
        origin.forward_offset,
        0.1,
        "SpawnOffset, swarm EffectData:1460",
    );
    assert_close(
        origin.search_range,
        2.0,
        "SpawnRange, swarm EffectData:1459",
    );
    let AbilityEffectSpec::Effect(root) = &ability.effect else {
        return Err("Abduct runs an effect tree".into());
    };

    let spells = catalogs.units.spells();
    let node = |id| spells.node(id).map(|node| node.kind).ok_or("a node");
    let root = spells.effect_id(root).ok_or("the root")?;
    let EffectNode::Set { first, len: 2 } = node(root)? else {
        return Err("YoinkStartSet: the mark, then the tongue, swarm EffectData:1472-1478".into());
    };
    let mark_status = spells.status_id("abduct_mark").ok_or("the mark status")?;
    assert_eq!(
        spells
            .node(root)
            .and_then(|node| node.require)
            .and_then(|filter| filter.without_status),
        Some(mark_status),
        "NoYoink on YoinkStartCreatePlaceholder, swarm EffectData:1468"
    );
    let start = |index| spells.set_child(first, index).ok_or("a child");
    assert_eq!(
        node(start(0)?)?,
        EffectNode::ApplyStatus {
            status: mark_status
        },
        "YoinkMarkerBehavior, swarm EffectData:1475, :1482-1485"
    );
    let mark = spells.status(mark_status).ok_or("the mark")?;
    assert_close(
        mark.spec.duration_ticks,
        35.714_286,
        "2 Normal s, swarm BehaviorData:1092",
    );
    assert!(
        !mark.spec.stun && !mark.spec.reveal && mark.spec.presentation.is_empty(),
        "hidden, no modification, swarm BehaviorData:1089-1096"
    );
    let EffectNode::LaunchMissile {
        to: EffectAnchor::TargetUnit,
        speed,
        view: MissileView::Tether,
        impact,
        visibility,
    } = node(start(1)?)?
    else {
        return Err("a homing tether missile, swarm EffectData:1486-1494".into());
    };
    assert_close(speed, 1.05, "18.75 x 1.4 / 25, swarm MoverData:147");
    assert_eq!(
        visibility,
        AttackVisibility::Snapshot,
        "YoinkLaunchMissile names no Visibility: the CEffectLaunchMissile default, core EffectData:102"
    );
    let EffectNode::Set { first, len: 3 } = node(impact)? else {
        return Err("the contact set of three, swarm EffectData:1498-1509".into());
    };
    let child = |index| spells.set_child(first, index).ok_or("a child");
    assert_eq!(
        node(child(0)?)?,
        EffectNode::CancelOrders {
            force_unburrow: true
        },
        "YoinkCancelOrders and InstantUnburrow, swarm EffectData:1503-1504"
    );
    let EffectNode::ApplyStatus { status: applied } = node(child(1)?)? else {
        return Err("the contact applies the status".into());
    };
    let EffectNode::Persistent {
        anchor: EffectAnchor::TargetUnit,
        initial: None,
        periodic: Some(flight),
        period_ticks,
        count: 1,
    } = node(child(2)?)?
    else {
        return Err("a one-pulse delay on the target, swarm EffectData:1539-1547".into());
    };
    assert_close(
        period_ticks,
        1.785_714,
        "0.1 Normal s, swarm EffectData:1545",
    );
    let EffectNode::Displace {
        flight_ticks: 18,
        tether: true,
        on_land: Some(on_land),
    } = node(flight)?
    else {
        return Err("an 18-tick tethered flight, swarm MoverData:167".into());
    };
    assert_eq!(
        node(on_land)?,
        EffectNode::RemoveStatus { status: applied },
        "YoinkRemoveBehavior, swarm EffectData:1601-1605"
    );

    let status = spells.status(applied).ok_or("the status")?;
    assert_eq!(status.spec.name, "abducted");
    assert_close(
        status.spec.duration_ticks,
        35.714_286,
        "2 Normal s, swarm BehaviorData:1087",
    );
    let spec = &status.spec;
    assert!(
        spec.stun && spec.reveal && spec.suppress_collision && spec.suppress_boarding,
        "Stun, SuppressCloak, SuppressCollision, SuppressPassenger, swarm BehaviorData:1078-1084"
    );
    assert!(spec.period_ticks.is_none() && status.initial_effect.is_none());
    Ok(())
}

/// Fungal Growth as authored (plan `2026-11-10-spellcaster-mechanics` §3.3, §5): the Infestor
/// analogue's ability, its missile, search and set, and the two statuses, each value cited from
/// `SC2GameData` 5.0.14 in the RON files.
#[test]
#[allow(clippy::too_many_lines)]
fn fungal_growth_values_follow_starcraft() -> TestResult {
    use sim_core::catalog::EffectNode;
    use sim_types::{
        AbilityEffectSpec, AbilityTargetKind, AttributeBonuses, EffectAnchor, FilterAlliance,
        MissileView, StructureRule,
    };

    let catalogs = sim_fixtures::shipped_catalogs();
    let caster = catalogs
        .units
        .archetypes()
        .find(|archetype| analogue(archetype) == "Infestor")
        .ok_or("the Infestor analogue")?;
    let ability = caster
        .abilities
        .iter()
        .find(|ability| ability.name == "Fungal Growth")
        .ok_or("Fungal Growth")?;
    assert_eq!(ability.target_kind, AbilityTargetKind::TargetPosition);
    assert_close(ability.energy_cost, 75.0, "liberty AbilData:486");
    assert_eq!(
        ability.cooldown_ticks, 0,
        "the Leech link has no time, liberty AbilData:487"
    );
    assert_close(ability.cast_range, 10.0, "void AbilData:674");
    assert_eq!(
        ability.cast_backswing_ticks, 0,
        "no CastOutroTime in the chain"
    );
    assert!(ability.single_caster && ability.target_filter.is_none() && ability.origin.is_none());
    let AbilityEffectSpec::Effect(root) = &ability.effect else {
        return Err("Fungal Growth runs an effect tree".into());
    };

    let spells = catalogs.units.spells();
    let node = |id| spells.node(id).map(|node| node.kind).ok_or("a node");
    let EffectNode::LaunchMissile {
        to: EffectAnchor::TargetPoint,
        speed,
        view: MissileView::Projectile,
        impact,
        visibility,
    } = node(spells.effect_id(root).ok_or("the root")?)?
    else {
        return Err("a missile to the point, void EffectData:1141-1146".into());
    };
    assert_close(speed, 0.672, "12 x 1.4 / 25, void MoverData:18");
    assert_eq!(
        visibility,
        AttackVisibility::Snapshot,
        "FungalGrowthLaunchMissile names no Visibility: the CEffectLaunchMissile default, core EffectData:102"
    );
    let EffectNode::Search {
        radius,
        filter,
        max_count: None,
        child,
    } = node(impact)?
    else {
        return Err("the impact searches, liberty EffectData:728-735".into());
    };
    assert_close(radius, 2.25, "voidmulti EffectData:1559");
    assert_eq!(
        filter.alliance,
        FilterAlliance::Enemy,
        "liberty EffectData:731"
    );
    assert!(filter.ground && filter.air);
    assert_eq!(filter.structures, StructureRule::Exclude);
    let EffectNode::Set { first, len: 3 } = node(child)? else {
        return Err("FungalGrowthSet and the search dummy, liberty EffectData:714-741".into());
    };
    let child = |index| spells.set_child(first, index).ok_or("a child");
    let EffectNode::ApplyStatus { status: damage } = node(child(0)?)? else {
        return Err("FungalGrowthApplyBehavior, liberty EffectData:720-723".into());
    };
    let EffectNode::ApplyStatus { status: slow } = node(child(1)?)? else {
        return Err("FungalGrowthSlowMovementApplyBehavior, voidmulti EffectData:1562".into());
    };
    let EffectNode::Damage {
        amount,
        attribute_bonus,
        responses,
        visibility,
    } = node(child(2)?)?
    else {
        return Err("FungalGrowthSearchDummy, liberty EffectData:736-741".into());
    };
    assert_close(amount, 0.0, "a dummy");
    assert_eq!(
        visibility,
        AttackVisibility::Snapshot,
        "FungalGrowthSearchDummy names no Visibility: the CEffectDamage default, core EffectData:62"
    );
    assert_eq!(attribute_bonus, AttributeBonuses::NONE);
    assert!(
        responses.acquire && responses.flee,
        "liberty EffectData:738-739"
    );

    let damage = spells.status(damage).ok_or("the damage status")?;
    assert_eq!(damage.spec.name, "fungal_growth");
    assert_close(
        damage.spec.duration_ticks,
        71.428_57,
        "4 Normal s, swarm BehaviorData:839",
    );
    assert_close(
        damage.spec.period_ticks.ok_or("a period")?,
        4.464_286,
        "0.25 Normal s, swarm BehaviorData:840",
    );
    assert!(
        damage.spec.reveal,
        "SuppressCloak, liberty BehaviorData:332"
    );
    assert!(
        damage.spec.suppress_blink,
        "CasterNotFungalGrowthed, swarm ValidatorData:537-541"
    );
    assert!(damage.initial_effect.is_none());
    let EffectNode::Damage {
        amount,
        attribute_bonus,
        responses,
        visibility,
    } = node(damage.periodic_effect.ok_or("periodic damage")?)?
    else {
        return Err("FungalGrowthDamage".into());
    };
    assert_close(amount, 1.5625, "voidmulti EffectData:2389");
    assert_eq!(
        visibility,
        AttackVisibility::Snapshot,
        "FungalGrowthDamage names no Visibility: the CEffectDamage default, core EffectData:62"
    );
    assert_eq!(
        attribute_bonus,
        AttributeBonuses::NONE,
        "Armored 0, void EffectData:1139"
    );
    assert!(!responses.acquire && !responses.flee);

    let slow = spells.status(slow).ok_or("the slow")?;
    assert_eq!(slow.spec.name, "fungal_growth_slow");
    assert_close(
        slow.spec.duration_ticks,
        71.428_57,
        "4 Normal s, voidmulti BehaviorData:696",
    );
    assert_close(
        slow.spec.move_speed_factor,
        0.25,
        "voidmulti BehaviorData:697",
    );
    assert!(
        slow.spec.suppress_boarding,
        "SuppressPassenger, voidmulti BehaviorData:698"
    );
    assert!(slow.spec.period_ticks.is_none() && slow.periodic_effect.is_none());
    Ok(())
}

/// Blinding Cloud (plan `2026-11-10-spellcaster-mechanics` §3.4, WP5): the ability, the tree
/// and both statuses as authored, against `SC2GameData` 5.0.14 (`5444e12f13`).
#[test]
#[allow(clippy::too_many_lines)]
fn blinding_cloud_values_follow_starcraft() -> TestResult {
    use sim_core::catalog::EffectNode;
    use sim_types::{
        AbilityEffectSpec, AbilityTargetKind, EffectAnchor, FilterAlliance, StructureRule,
    };

    let catalogs = sim_fixtures::shipped_catalogs();
    let caster = catalogs
        .units
        .archetypes()
        .find(|archetype| analogue(archetype) == "Viper")
        .ok_or("the Viper analogue")?;
    let ability = caster
        .abilities
        .iter()
        .find(|ability| ability.name == "Blinding Cloud")
        .ok_or("Blinding Cloud")?;
    assert_eq!(ability.target_kind, AbilityTargetKind::TargetPosition);
    assert_close(ability.energy_cost, 100.0, "swarm AbilData:1202");
    assert_eq!(
        ability.cooldown_ticks, 0,
        "no cooldown, swarm AbilData:1197-1209"
    );
    assert_close(ability.cast_range, 10.0, "swarm AbilData:1204");
    assert_eq!(
        ability.cast_backswing_ticks, 0,
        "no CastOutroTime in the chain"
    );
    assert!(ability.single_caster && ability.target_filter.is_none() && ability.origin.is_none());
    let AbilityEffectSpec::Effect(root) = &ability.effect else {
        return Err("Blinding Cloud runs an effect tree".into());
    };

    let spells = catalogs.units.spells();
    let flat = |id| spells.node(id).ok_or("a node");
    let node = |id| flat(id).map(|node| node.kind);
    let EffectNode::Persistent {
        anchor: EffectAnchor::TargetPoint,
        initial: Some(initial),
        periodic: Some(periodic),
        period_ticks,
        count: 15,
    } = node(spells.effect_id(root).ok_or("the root")?)?
    else {
        return Err("a point persistent of 15 pulses, voidmulti EffectData:158".into());
    };
    assert_eq!(initial, periodic, "one search, swarm EffectData:238, :240");
    assert_close(
        period_ticks,
        8.928_571,
        "0.5 Normal s, swarm EffectData:241",
    );
    let EffectNode::Search {
        radius,
        filter,
        max_count: None,
        child,
    } = node(initial)?
    else {
        return Err("BlindingCloudSearch, swarm EffectData:243-248".into());
    };
    assert_close(radius, 2.0, "swarm EffectData:247");
    assert_eq!(
        filter.alliance,
        FilterAlliance::AnyButSelf,
        "swarm EffectData:246"
    );
    assert!(
        filter.ground && !filter.air,
        "Ground required, swarm EffectData:246"
    );
    assert_eq!(filter.structures, StructureRule::Include);
    let EffectNode::Set { first, len: 2 } = node(child)? else {
        return Err("BlindingCloudSwitch, swarm EffectData:254-258".into());
    };
    let mut seen = Vec::new();
    for index in 0..2 {
        let branch = flat(spells.set_child(first, index).ok_or("a child")?)?;
        let EffectNode::ApplyStatus { status } = branch.kind else {
            return Err("an ApplyStatus branch".into());
        };
        let rule = branch.require.ok_or("a filtered branch")?.structures;
        let name = spells.status(status).ok_or("a status")?.spec.name.clone();
        seen.push((name, rule));
    }
    assert_eq!(
        seen,
        [
            ("blinding_cloud".to_owned(), StructureRule::Exclude),
            ("blinding_cloud_structure".to_owned(), StructureRule::Only),
        ],
        "CaseDefault BlindingCloudAB and Case IsStructure, swarm EffectData:256-257"
    );

    let units = spells
        .status(
            spells
                .status_id("blinding_cloud")
                .ok_or("the unit status")?,
        )
        .ok_or("listed")?;
    assert_close(
        units.spec.duration_ticks,
        11.160_714,
        "0.625 Normal s, swarm BehaviorData:997",
    );
    assert_close(
        units.spec.ranged_weapon_range_delta,
        -20.0,
        "voidmulti BehaviorData:1068",
    );
    assert_close(
        units.spec.weapon_range_delta,
        0.0,
        "WeaponRange 0, voidmulti BehaviorData:1068",
    );
    assert_close(units.spec.range_floor, 0.1, "SU-2");
    assert!(!units.spec.suppress_detection && units.periodic_effect.is_none());

    let structures = spells
        .status(
            spells
                .status_id("blinding_cloud_structure")
                .ok_or("the structure status")?,
        )
        .ok_or("listed")?;
    assert_close(
        structures.spec.duration_ticks,
        11.160_714,
        "0.625 Normal s, swarm BehaviorData:24",
    );
    assert_close(
        structures.spec.weapon_range_delta,
        -20.0,
        "swarm BehaviorData:25",
    );
    assert_close(
        structures.spec.ranged_weapon_range_delta,
        0.0,
        "every weapon instead",
    );
    assert!(
        structures.spec.suppress_detection,
        "DetectArc 0, swarm BehaviorData:25"
    );
    assert!(structures.periodic_effect.is_none());
    Ok(())
}

/// B-135: every structure's radius, the edge its ranges measure to: its analogue's 5.0.14 engine
/// radius `max(Radius, ⌈16 · Shape.Radius⌉ ÷ 16)` (liberty `UnitData`, `FootprintData.xml` at
/// `5444e12f13`; the live value of BurnySc2/sc2-techtree `data.json`, `c2c90e9b56`; plan
/// `2026-11-18-structure-radius` §2.3). The floor of that plan's D-4 is gone with B-138 (plan
/// `2026-11-18-contour-footprints` D-10). Rows name analogues (charter §3.3).
const SC2_STRUCTURE_RADIUS: [(&str, [f32; 1], &str); 49] = [
    (
        "Command Center",
        [2.75],
        "liberty UnitData:10377 Radius 2.5; Footprint:2015 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Orbital Command",
        [2.75],
        "liberty UnitData:10523 Radius 2.5; Footprint:2015 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Planetary Fortress",
        [2.75],
        "liberty UnitData:10661 Radius 2.5; Footprint:2015 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Nexus",
        [2.75],
        "liberty UnitData:4184 Radius 2.0; Footprint:2015 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Hatchery",
        [2.75],
        "liberty UnitData:8208 Radius 2.0; Footprint:2261 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Lair",
        [2.75],
        "liberty UnitData:8290 Radius 2.0; Footprint:2261 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Hive",
        [2.75],
        "liberty UnitData:8370 Radius 2.0; Footprint:2261 Shape 2.6926; engine 2.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Barracks",
        [1.8125],
        "liberty UnitData:5838 Radius 1.75; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Engineering Bay",
        [1.8125],
        "liberty UnitData:5970 Radius 1.25; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Bunker",
        [1.8125],
        "liberty UnitData:6154 Radius 1.25; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Factory",
        [1.8125],
        "liberty UnitData:6238 Radius 1.625; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Ghost Academy",
        [1.8125],
        "liberty UnitData:9773 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Starport",
        [1.8125],
        "liberty UnitData:6371 Radius 1.625; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Armory",
        [1.8125],
        "liberty UnitData:6506 Radius 1.25; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Fusion Core",
        [1.8125],
        "liberty UnitData:10915 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Gateway",
        [1.8125],
        "liberty UnitData:4394 Radius 1.75; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Forge",
        [1.8125],
        "liberty UnitData:4536 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Cybernetics Core",
        [1.8125],
        "liberty UnitData:10817 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Twilight Council",
        [1.8125],
        "liberty UnitData:4604 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Stargate",
        [1.8125],
        "liberty UnitData:9640 Radius 1.75; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Robotics Facility",
        [1.8125],
        "liberty UnitData:9569 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Templar Archives",
        [1.8125],
        "liberty UnitData:4667 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Robotics Bay",
        [1.8125],
        "liberty UnitData:9837 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Fleet Beacon",
        [1.8125],
        "liberty UnitData:9708 Radius 1.5; Footprint:1935 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Spawning Pool",
        [1.8125],
        "liberty UnitData:8886 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Evolution Chamber",
        [1.8125],
        "liberty UnitData:8444 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Roach Warren",
        [1.8125],
        "liberty UnitData:11765 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Baneling Nest",
        [1.8125],
        "liberty UnitData:9966 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Hydralisk Den",
        [1.8125],
        "liberty UnitData:8949 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Lurker Den",
        [1.8125],
        "liberty UnitData:676 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Infestation Pit",
        [1.8125],
        "liberty UnitData:2077 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Nydus Network",
        [1.8125],
        "liberty UnitData:9899 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Ultralisk Cavern",
        [1.8125],
        "liberty UnitData:9215 Radius 1.5; Footprint:2205 Shape 1.803; engine 1.8125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Refinery",
        [1.6875],
        "liberty UnitData:5776 Radius 1.5; Footprint:959 Shape 1.6772; engine 1.6875 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Assimilator",
        [1.6875],
        "liberty UnitData:4122 Radius 1.5; Footprint:959 Shape 1.6772; engine 1.6875 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Extractor",
        [1.6875],
        "liberty UnitData:8149 Radius 1.5; Footprint:959 Shape 1.6772; engine 1.6875 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Supply Depot",
        [1.25],
        "liberty UnitData:5663 Radius 1.25; Footprint:1950 Shape 1.1181; engine 1.25 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Missile Turret",
        [1.125],
        "liberty UnitData:6033 Radius 0.75; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Tech Lab",
        [1.125],
        "liberty UnitData:6642 Radius 1.0; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Reactor",
        [1.125],
        "liberty UnitData:6579 Radius 1.0; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Pylon",
        [1.125],
        "liberty UnitData:4335 Radius 1.0; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Photon Cannon",
        [1.125],
        "liberty UnitData:11052 Radius 1.0; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Shield Battery",
        [1.125],
        "liberty UnitData:1612 Radius 1.0; Footprint:1950 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Dark Shrine",
        [1.5],
        "liberty UnitData:9510 Radius 1.5; Footprint:1950 Shape 1.1181; engine 1.5 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Spine Crawler",
        [1.125],
        "liberty UnitData:8615 Radius 0.875; Footprint:5369 Shape 1.1181; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Spore Crawler",
        [0.875],
        "liberty UnitData:8749 Radius 0.875; Footprint2x2Contour2 Shape 0.559, Footprint:162; engine 0.875 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Sensor Tower",
        [0.75],
        "liberty UnitData:9452 Radius 0.75; Footprint1x1 Shape 0.7072, Footprint:1106; engine 0.75 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Spire",
        [1.125],
        "liberty UnitData:9011 Radius 1.0; Footprint2x2CreepContour Shape 1.1181, Footprint:2073; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
    (
        "Greater Spire",
        [1.125],
        "liberty UnitData:9082 Radius 1.0; Footprint2x2IgnoreCreepContour Shape 1.1181, Footprint:1238; engine 1.125 (sc2-techtree c2c90e9b56)",
    ),
];

#[test]
fn b135_structure_radii_follow_starcraft() -> TestResult {
    check_table("structure radius", true, &SC2_STRUCTURE_RADIUS, &[], |a| {
        [f(a.collision_radius)]
    })
}

/// B-135: each analogue's `[Radius, Shape.Radius, E]` (liberty `UnitData` `Radius`, its
/// footprint's `FootprintData.xml` `<Shape><Radius>`, the live engine radius of sc2-techtree).
const SC2_STRUCTURE_ENGINE_RADIUS: [(&str, [f32; 3], &str); 49] = [
    (
        "Command Center",
        [2.5, 2.6926, 2.75],
        "liberty UnitData:10377, Footprint:2015; sc2-techtree c2c90e9b56",
    ),
    (
        "Orbital Command",
        [2.5, 2.6926, 2.75],
        "liberty UnitData:10523, Footprint:2015; sc2-techtree c2c90e9b56",
    ),
    (
        "Planetary Fortress",
        [2.5, 2.6926, 2.75],
        "liberty UnitData:10661, Footprint:2015; sc2-techtree c2c90e9b56",
    ),
    (
        "Nexus",
        [2.0, 2.6926, 2.75],
        "liberty UnitData:4184, Footprint:2015; sc2-techtree c2c90e9b56",
    ),
    (
        "Hatchery",
        [2.0, 2.6926, 2.75],
        "liberty UnitData:8208, Footprint:2261; sc2-techtree c2c90e9b56",
    ),
    (
        "Lair",
        [2.0, 2.6926, 2.75],
        "liberty UnitData:8290, Footprint:2261; sc2-techtree c2c90e9b56",
    ),
    (
        "Hive",
        [2.0, 2.6926, 2.75],
        "liberty UnitData:8370, Footprint:2261; sc2-techtree c2c90e9b56",
    ),
    (
        "Barracks",
        [1.75, 1.803, 1.8125],
        "liberty UnitData:5838, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Engineering Bay",
        [1.25, 1.803, 1.8125],
        "liberty UnitData:5970, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Bunker",
        [1.25, 1.803, 1.8125],
        "liberty UnitData:6154, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Factory",
        [1.625, 1.803, 1.8125],
        "liberty UnitData:6238, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Ghost Academy",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9773, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Starport",
        [1.625, 1.803, 1.8125],
        "liberty UnitData:6371, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Armory",
        [1.25, 1.803, 1.8125],
        "liberty UnitData:6506, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Fusion Core",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:10915, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Gateway",
        [1.75, 1.803, 1.8125],
        "liberty UnitData:4394, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Forge",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:4536, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Cybernetics Core",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:10817, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Twilight Council",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:4604, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Stargate",
        [1.75, 1.803, 1.8125],
        "liberty UnitData:9640, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Robotics Facility",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9569, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Templar Archives",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:4667, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Robotics Bay",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9837, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Fleet Beacon",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9708, Footprint:1935; sc2-techtree c2c90e9b56",
    ),
    (
        "Spawning Pool",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:8886, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Evolution Chamber",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:8444, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Roach Warren",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:11765, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Baneling Nest",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9966, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Hydralisk Den",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:8949, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Lurker Den",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:676, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Infestation Pit",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:2077, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Nydus Network",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9899, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Ultralisk Cavern",
        [1.5, 1.803, 1.8125],
        "liberty UnitData:9215, Footprint:2205; sc2-techtree c2c90e9b56",
    ),
    (
        "Refinery",
        [1.5, 1.6772, 1.6875],
        "liberty UnitData:5776, Footprint:959; sc2-techtree c2c90e9b56",
    ),
    (
        "Assimilator",
        [1.5, 1.6772, 1.6875],
        "liberty UnitData:4122, Footprint:959; sc2-techtree c2c90e9b56",
    ),
    (
        "Extractor",
        [1.5, 1.6772, 1.6875],
        "liberty UnitData:8149, Footprint:959; sc2-techtree c2c90e9b56",
    ),
    (
        "Supply Depot",
        [1.25, 1.1181, 1.25],
        "liberty UnitData:5663, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Missile Turret",
        [0.75, 1.1181, 1.125],
        "liberty UnitData:6033, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Tech Lab",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:6642, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Reactor",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:6579, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Pylon",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:4335, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Photon Cannon",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:11052, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Shield Battery",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:1612, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Dark Shrine",
        [1.5, 1.1181, 1.5],
        "liberty UnitData:9510, Footprint:1950; sc2-techtree c2c90e9b56",
    ),
    (
        "Spine Crawler",
        [0.875, 1.1181, 1.125],
        "liberty UnitData:8615, Footprint:5369; sc2-techtree c2c90e9b56",
    ),
    (
        "Spore Crawler",
        [0.875, 0.559, 0.875],
        "liberty UnitData:8749, Footprint:162; sc2-techtree c2c90e9b56",
    ),
    (
        "Sensor Tower",
        [0.75, 0.7072, 0.75],
        "liberty UnitData:9452, Footprint:1106; sc2-techtree c2c90e9b56",
    ),
    (
        "Spire",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:9011, Footprint:2073; sc2-techtree c2c90e9b56",
    ),
    (
        "Greater Spire",
        [1.0, 1.1181, 1.125],
        "liberty UnitData:9082, Footprint:1238; sc2-techtree c2c90e9b56",
    ),
];

/// The engine rule pins the data (plan `2026-11-18-structure-radius` §2.2, plan
/// `2026-11-18-contour-footprints` D-10): every row's live radius `E` is
/// `max(Radius, ⌈16 · Shape.Radius⌉ ÷ 16)`, every structure's authored radius is its `E`, and `E`
/// covers its authored footprint shape (its farthest vertex is within `E` of the centre), so no
/// unsourced value can be authored and a unit touching any point of the shape is within reach.
#[test]
fn structure_radii_follow_the_engine_rule() -> TestResult {
    for (analogue, [radius, shape, engine], source) in &SC2_STRUCTURE_ENGINE_RADIUS {
        let rule = radius.max((shape * 16.0).ceil() / 16.0);
        assert!(
            (rule - engine).abs() < 0.001,
            "{analogue}: max({radius}, ceil(16 * {shape}) / 16) = {rule}, live {engine} ({source})"
        );
    }
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut checked = 0;
    for archetype in catalogs.units.archetypes() {
        let sim_types::ShapeKind::BuildingDome { shape, .. } = archetype.shape else {
            continue;
        };
        let name = analogue(archetype);
        let (_, [_, _, engine], source) = SC2_STRUCTURE_ENGINE_RADIUS
            .iter()
            .find(|(row, ..)| *row == name)
            .ok_or_else(|| format!("{} ({name}): no engine radius row", archetype.slug))?;
        let authored = f(archetype.collision_radius);
        assert!(
            (authored - engine).abs() < 0.001,
            "{} ({name}): radius {authored}, expected the engine radius {engine} ({source})",
            archetype.slug
        );
        let quarters = |q: u16| f32::from(q) / 4.0;
        let box_q = shape
            .box_extents()
            .ok_or("every structure's shape is a box")?;
        let (hw, hh, c) = (quarters(box_q.0), quarters(box_q.1), quarters(box_q.2));
        let farthest = (hw * hw + (hh - c) * (hh - c)).max((hw - c) * (hw - c) + hh * hh);
        assert!(
            engine * engine >= farthest,
            "{} ({name}): engine radius {engine} does not cover the shape ({hw}, {hh}, {c})",
            archetype.slug
        );
        checked += 1;
    }
    assert_eq!(checked, SC2_STRUCTURE_ENGINE_RADIUS.len());
    Ok(())
}

/// B-134, B-138: every structure's footprint cells and 5.0.14 `Shape`, `[w, h, half-width,
/// half-height, chamfer]` in cells and world units: the analogue's `Footprint` in liberty
/// `UnitData` and its `Shape` `Offsets` in `FootprintData.xml` at `5444e12f13` (plan
/// `2026-11-18-contour-footprints` §2.3). The Greater Spire's footprint inherits its 2×2 cells from
/// `Footprint2x2` (Footprint:1226). Rows name analogues (charter §3.3).
const SC2_FOOTPRINT_SHAPE: [(&str, [f32; 5], &str); 49] = [
    (
        "Command Center",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:10379 Footprint5x5Contour; Footprint:2016 Offsets",
    ),
    (
        "Orbital Command",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:10525 Footprint5x5Contour; Footprint:2016 Offsets",
    ),
    (
        "Planetary Fortress",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:10663 Footprint5x5Contour; Footprint:2016 Offsets",
    ),
    (
        "Nexus",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:4186 Footprint5x5Contour; Footprint:2016 Offsets",
    ),
    (
        "Hatchery",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:8210 Footprint6x5DropOffCreepSourceContour; Footprint:2262 Offsets",
    ),
    (
        "Lair",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:8292 Footprint6x5DropOffCreepSourceContour; Footprint:2262 Offsets",
    ),
    (
        "Hive",
        [5.0, 5.0, 2.5, 2.5, 1.5],
        "liberty UnitData:8372 Footprint6x5DropOffCreepSourceContour; Footprint:2262 Offsets",
    ),
    (
        "Barracks",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:5840 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Engineering Bay",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:5972 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Bunker",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:6156 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Factory",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:6240 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Ghost Academy",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9775 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Starport",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:6373 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Armory",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:6508 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Fusion Core",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:10917 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Gateway",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:4396 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Forge",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:4538 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Cybernetics Core",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:10819 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Twilight Council",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:4606 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Stargate",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9642 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Robotics Facility",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9571 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Templar Archives",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:4669 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Robotics Bay",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9839 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Fleet Beacon",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9710 Footprint3x3Contour; Footprint:1936 Offsets",
    ),
    (
        "Spawning Pool",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:8888 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Evolution Chamber",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:8446 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Roach Warren",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:11767 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Baneling Nest",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9968 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Hydralisk Den",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:8951 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Lurker Den",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:678 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Infestation Pit",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:2079 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Nydus Network",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9901 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Ultralisk Cavern",
        [3.0, 3.0, 1.5, 1.5, 0.5],
        "liberty UnitData:9217 Footprint3x3CreepContour; Footprint:2206 Offsets",
    ),
    (
        "Refinery",
        [3.0, 3.0, 1.5, 1.5, 0.75],
        "liberty UnitData:5778 FootprintGeyserRoundedBuilt; Footprint:960 Offsets",
    ),
    (
        "Assimilator",
        [3.0, 3.0, 1.5, 1.5, 0.75],
        "liberty UnitData:4124 FootprintGeyserRoundedBuilt; Footprint:960 Offsets",
    ),
    (
        "Extractor",
        [3.0, 3.0, 1.5, 1.5, 0.75],
        "liberty UnitData:8151 FootprintGeyserRoundedBuilt; Footprint:960 Offsets",
    ),
    (
        "Supply Depot",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:5665 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Missile Turret",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:6035 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Tech Lab",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:6644 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Reactor",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:6581 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Pylon",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:4337 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Photon Cannon",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:11054 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Shield Battery",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:1614 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Dark Shrine",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:9512 Footprint2x2Contour; Footprint:1951 Offsets",
    ),
    (
        "Spine Crawler",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:8617 Footprint2x2ZergSpineCrawler; Footprint:5370 Offsets",
    ),
    (
        "Spore Crawler",
        [2.0, 2.0, 0.5, 0.5, 0.25],
        "liberty UnitData:8751 Footprint2x2Contour2; Footprint:163 Offsets",
    ),
    (
        "Sensor Tower",
        [1.0, 1.0, 0.5, 0.5, 0.0],
        "liberty UnitData:9454 Footprint1x1; Footprint:1107 Offsets",
    ),
    (
        "Spire",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:9013 Footprint2x2CreepContour; Footprint:2074 Offsets",
    ),
    (
        "Greater Spire",
        [2.0, 2.0, 1.0, 1.0, 0.5],
        "liberty UnitData:9084 Footprint2x2IgnoreCreepContour; Footprint:1239 Offsets",
    ),
];

#[test]
fn b134_footprint_shapes_follow_starcraft() -> TestResult {
    check_table("footprint shape", true, &SC2_FOOTPRINT_SHAPE, &[], |a| {
        let sim_types::ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
            shape,
        } = a.shape
        else {
            return [f32::NAN; 5];
        };
        let quarters = |q: u16| f32::from(q) / 4.0;
        let Some(box_q) = shape.box_extents() else {
            return [f32::NAN; 5];
        };
        [
            f32::from(footprint_w),
            f32::from(footprint_h),
            quarters(box_q.0),
            quarters(box_q.1),
            quarters(box_q.2),
        ]
    })
}

/// The placement flags each structure's 5.0.14 placement footprint `Check` refuses beyond the
/// shared defaults (plan `2026-11-19-footprint-flags` §2.3, D-2): the town halls' `NearResources`
/// (`Footprint5x5DropOff` Footprint:1838, `Footprint6x5DropOffCreepSource` Footprint:2213) and the
/// crawlers' `Fogged` (`Footprint2x2Creep2` Footprint:1217). Every other structure refuses none.
const SC2_PLACEMENT_EXCLUDES: [(&str, sim_types::PlacementFlag, &str); 9] = [
    (
        "Command Center",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:10380",
    ),
    (
        "Orbital Command",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:10526",
    ),
    (
        "Planetary Fortress",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:10664",
    ),
    (
        "Nexus",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:4187",
    ),
    (
        "Hatchery",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:8211",
    ),
    (
        "Lair",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:8293",
    ),
    (
        "Hive",
        sim_types::PlacementFlag::NearResources,
        "liberty UnitData:8373",
    ),
    (
        "Spine Crawler",
        sim_types::PlacementFlag::Fogged,
        "liberty UnitData:8618",
    ),
    (
        "Spore Crawler",
        sim_types::PlacementFlag::Fogged,
        "liberty UnitData:8752",
    ),
];

#[test]
fn the_shipped_flag_sets() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut structures = 0;
    let mut flagged = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes() {
        if !is_structure(archetype) {
            assert!(
                archetype.placement_excludes.is_empty(),
                "{} is mobile",
                archetype.slug
            );
            continue;
        }
        structures += 1;
        let name = analogue(archetype);
        let expected = SC2_PLACEMENT_EXCLUDES
            .iter()
            .find(|(analogue, ..)| *analogue == name)
            .map_or(sim_types::PlacementFlags::empty(), |(_, flag, _)| {
                flag.bit()
            });
        assert_eq!(archetype.placement_excludes, expected, "{name}");
        if !expected.is_empty() {
            flagged.insert(name.to_owned());
        }
    }
    assert_eq!(structures, 49, "every structure row is checked");
    assert_eq!(flagged.len(), SC2_PLACEMENT_EXCLUDES.len(), "{flagged:?}");
}

/// Plan `2026-11-20-resource-footprints` WP2 (D-1, §2.3): the shipped resource footprints are
/// 5.0.14's. A mineral field (`FootprintMineralsRounded`, liberty Footprint:1061-1096) takes its
/// anchor cell and the cell east of it, its shape is the ±1 × ±0.5 box (Footprint:1094), its
/// `NearResources` ring is the 8 × 7 rows less their corners (52 cells, the field's two
/// included), and it leaves the map when empty (`RemoveWhenEmpty`, liberty BehaviorData:749). A
/// geyser (`FootprintGeyserRounded`, Footprint:1535-1576 with void Footprint:3-21) takes the 3×3
/// centred on its anchor, its shape is the void layer's 16 vertices, its ring is the void-rounded
/// 9 × 9 (69 cells), and it stays when empty (liberty BehaviorData:801-807).
/// Plan `2026-11-22-harvest-timing` §2.3: each resource row's harvest is 5.0.14's, converted at
/// 25 ticks per Faster second (t Normal s = round(t / 1.4 × 25) ticks, halves up). A mineral
/// field (`MineralFieldMinerals`, liberty BehaviorData:743-756): `HarvestTime 2.786` (:747) =
/// 49.75 → 50 ticks, `HarvestAmount 5` (:748), `ReturnDelay 0.5` (:750) = 8.93 → 9 ticks; no
/// later layer changes them. Gas through an extractor (`HarvestableVespeneGeyserGas` and its
/// Protoss and Zerg twins, liberty BehaviorData:817-852): `HarvestTime 1.981` = 35.375 → 35
/// ticks, `HarvestAmount 4`, no `ReturnDelay` (0).
#[test]
fn the_shipped_resource_harvests_are_5_0_14() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let rules = catalogs.units.placement_rules();
    let mineral = rules
        .harvest(sim_types::ResourceKind::Mineral)
        .ok_or("a mineral harvest")?;
    assert_eq!(
        (
            mineral.time_ticks,
            mineral.amount,
            mineral.return_delay_ticks
        ),
        (50, 5, 9)
    );
    let gas = rules
        .harvest(sim_types::ResourceKind::Gas)
        .ok_or("a gas harvest")?;
    assert_eq!(
        (gas.time_ticks, gas.amount, gas.return_delay_ticks),
        (35, 4, 0)
    );
    Ok(())
}

/// Plan `2026-11-23-harvest-handoff` §2.2, §2.4 (**verify**: an inference, B-162, Q0100): the
/// next worker in a resource's line starts whole engine steps after the take, one step for it to
/// start plus one if the gatherer passes through a `WaitToReturn` stage, where the field is
/// released. Read in game loops (16 a Normal second, 22.4 a Faster second), Liquipedia's
/// saturated figures are whole loops: three workers on a mineral field earn 143 a minute, 5 a load,
/// a period of 46.99 loops (47) against a `HarvestTime 2.786` that the engine ends on loop 45 (a
/// gap of 2); a gold field's 200, 7 a load, gives 47.04 loops, the same 47; a geyser's 163, 4 a
/// load, gives 32.98 loops (33) against `HarvestTime 1.981` ending on loop 32 (a gap of 1). The
/// mineral row has a `ReturnDelay` (liberty BehaviorData:750) and so a `WaitToReturn` stage, 1 + 1
/// = 2 steps; gas has none (:817-852), 1 step. One loop is 1.12 ticks, so 2 loops are 2 ticks and
/// 1 loop 1 tick (S-5, whole-tick rounding).
#[test]
fn the_shipped_handoffs_follow_the_stage_reading() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let rules = catalogs.units.placement_rules();
    let mineral = rules
        .harvest(sim_types::ResourceKind::Mineral)
        .ok_or("a mineral harvest")?;
    let gas = rules
        .harvest(sim_types::ResourceKind::Gas)
        .ok_or("a gas harvest")?;
    assert_eq!((mineral.handoff_ticks, gas.handoff_ticks), (2, 1));
    Ok(())
}

#[test]
fn the_shipped_resource_footprints_are_5_0_14() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let rules = catalogs.units.placement_rules();
    let mineral = rules
        .resource_footprints()
        .get(sim_types::ResourceKind::Mineral)
        .ok_or("a mineral footprint")?;
    assert_eq!(mineral.cells, (0, 0, 2, 1));
    assert_eq!(
        mineral.shape,
        sim_types::FootprintShape::Box {
            half_width_q: 4,
            half_height_q: 2,
            chamfer_q: 0
        }
    );
    assert!(mineral.remove_when_empty);
    let mineral_reach = rules
        .reach(sim_types::ResourceKind::Mineral)
        .ok_or("a mineral reach")?;
    assert_eq!(mineral_reach.cells.len(), 52);
    let gas = rules
        .resource_footprints()
        .get(sim_types::ResourceKind::Gas)
        .ok_or("a gas footprint")?;
    assert_eq!(gas.cells, (-1, -1, 3, 3));
    let quarters: [(i16, i16); 16] = [
        (-2, -4),
        (-3, -5),
        (-5, -3),
        (-4, -2),
        (-4, 2),
        (-5, 3),
        (-3, 5),
        (-2, 4),
        (2, 4),
        (3, 5),
        (5, 3),
        (4, 2),
        (4, -2),
        (5, -3),
        (3, -5),
        (2, -4),
    ];
    let sim_types::FootprintShape::Outline(outline) = gas.shape else {
        return Err("the geyser's shape is an outline".into());
    };
    assert_eq!(outline.vertices(), quarters.as_slice());
    assert!(!gas.remove_when_empty);
    let gas_reach = rules
        .reach(sim_types::ResourceKind::Gas)
        .ok_or("a gas reach")?;
    assert_eq!(gas_reach.cells.len(), 69);
    Ok(())
}

/// Plan `2026-11-23-harvest-occupancy` D-4: every harvester acquires as the 5.0.14 harvest
/// ability does. `CAbilHarvest`'s default (core AbilData:353-380), which `SCVHarvest`,
/// `ProbeHarvest` and `DroneHarvest` inherit unchanged (liberty AbilData:1094-1099, 2241-2243):
/// `AcquireRadius 10` (:361, game units = cells = world units) and `ResourceAcquire Minerals 1`
/// (:368), with no `ResourceAcquire` for Vespene.
#[test]
fn the_shipped_harvesters_acquire_as_5_0_14() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut harvesters = 0;
    for archetype in catalogs.units.archetypes() {
        let Some(spec) = archetype.capabilities.harvester else {
            continue;
        };
        harvesters += 1;
        let name = analogue(archetype);
        assert_eq!(spec.acquire_radius, SimScalar::from_num(10), "{name}");
        assert_eq!(
            spec.acquires,
            sim_types::ResourceKindSet::of(&[sim_types::ResourceKind::Mineral]),
            "{name}"
        );
    }
    assert_eq!(harvesters, 3, "the three races' workers");
}

/// Plan `2026-11-23-production-queue` D-7: every town hall auto-rallies as 5.0.14's
/// `CommandStructureAutoRally` (voidmulti EffectData:2507-2537), which `CCCreateSet`,
/// `NexusCreateSet` and `HatcheryCreateSet` run (voidmulti EffectData:166-169, 696-699, 713-716):
/// a search of `HarvestableResource` within `Radius 6.5` (game units = cells = world units),
/// sorted by `TSFarthestDistance`, `MaxCount 1`, validated by `Minerals`. The Orbital Command,
/// Planetary Fortress, Lair and Hive keep their base hall's create set; no other structure has it.
#[test]
fn the_shipped_town_halls_auto_rally_as_5_0_14() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut halls: Vec<&str> = Vec::new();
    for archetype in catalogs.units.archetypes() {
        let Some(spec) = archetype.capabilities.auto_rally else {
            continue;
        };
        let name = analogue(archetype);
        halls.push(name);
        assert_eq!(spec.radius, SimScalar::from_num(6.5), "{name}");
        assert_eq!(
            spec.kinds,
            sim_types::ResourceKindSet::of(&[sim_types::ResourceKind::Mineral]),
            "{name}"
        );
    }
    halls.sort_unstable();
    assert_eq!(
        halls,
        [
            "Command Center",
            "Hatchery",
            "Hive",
            "Lair",
            "Nexus",
            "Orbital Command",
            "Planetary Fortress"
        ]
    );
}

/// Structure energy regeneration (plan `2026-11-23-regeneration` D-2, B-168): every structure
/// with energy regenerates `EnergyRegenRate` 0.5625 per Normal second, 0.5625 × 1.4 ÷ 25 =
/// 0.0315 per tick. Orbital Command `liberty UnitData:10513–10515`; Nexus
/// `liberty UnitData:4239–4240` (`voidmulti UnitData:1094–1096` sets its maximum and start, not
/// the rate); Shield Battery `liberty UnitData:1591–1593`. The three are the only structures
/// with energy, and every other structure regenerates none.
#[test]
fn structures_with_energy_regenerate_as_5_0_14() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let mut with_energy = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes().filter(|a| is_structure(a)) {
        let rate = f(archetype.energy_regen_per_tick);
        if archetype.max_energy > SimScalar::ZERO {
            assert!(
                (rate - 0.0315).abs() < 0.001,
                "{} ({}): energy regeneration {rate}, StarCraft II 0.0315 per tick",
                archetype.slug,
                analogue(archetype)
            );
            with_energy.insert(analogue(archetype).to_owned());
        } else {
            assert_eq!(rate, 0.0, "{}: no energy, no rate", archetype.slug);
        }
    }
    assert_eq!(
        with_energy.iter().map(String::as_str).collect::<Vec<_>>(),
        ["Nexus", "Orbital Command", "Shield Battery"]
    );
}

/// B-164 (plan `2026-11-25-rallies` WP2, §2.3): every producer's rally ability is 5.0.14's.
/// - Every structure that trains units authors one slot used by every unit and taking every
///   target (`CAbilRally` with one empty `InfoArray` entry: core AbilData:671-701; `Rally`,
///   `RallyCommand`, `RallyNexus`: liberty AbilData:859-884), except the Hatchery, Lair and
///   Hive analogues.
/// - Those carry `RallyHatchery` (liberty AbilData:885-897; UnitData:8246-8260, 8329-8341,
///   8408-8418): `InfoArray` 0 for non-workers that refuses resources
///   (`NotResourcesOrEnemyTargetType`, liberty ValidatorData:667-669, 694-703) and `InfoArray`
///   1 for workers that a right click on the ground does not set (`SetOnGround 0`), both
///   excluding the Queen (`NotQueen`, liberty ValidatorData:1598-1601). Their auto-rally fills
///   slot 1 (voidmulti EffectData:2507-2537).
/// - Every rally holds up to four waypoints (`TeamLiquid` 2011, plan S10; a community figure,
///   B-215).
#[test]
fn the_shipped_rallies_are_5_0_14s() -> TestResult {
    use sim_types::RallyUsers;
    let catalogs = sim_fixtures::shipped_catalogs();
    let queen = catalogs
        .units
        .archetypes()
        .find(|a| !is_structure(a) && analogue(a).starts_with("Queen"))
        .ok_or("no Queen analogue")?
        .slug
        .clone();
    let mut hatcheries = 0;
    for archetype in catalogs
        .units
        .archetypes()
        .filter(|a| is_structure(a) && !a.trains_units.is_empty())
    {
        let slug = &archetype.slug;
        let rally = archetype
            .rally
            .as_ref()
            .ok_or(format!("{slug}: no rally"))?;
        assert_eq!(rally.max_waypoints, 4, "{slug}");
        assert!(
            rally.slots.iter().all(|slot| !slot.label.is_empty()),
            "{slug}"
        );
        let auto_slot = archetype.capabilities.auto_rally.map(|auto| auto.slot);
        if matches!(analogue(archetype), "Hatchery" | "Lair" | "Hive") {
            hatcheries += 1;
            assert_eq!(rally.slots.len(), 2, "{slug}");
            let (army, worker) = (&rally.slots[0], &rally.slots[1]);
            assert_eq!(army.used_by, RallyUsers::NonWorkers, "{slug}");
            assert!(!army.on_resources && army.smart_on_ground, "{slug}");
            assert_eq!(worker.used_by, RallyUsers::Workers, "{slug}");
            assert!(worker.on_resources && !worker.smart_on_ground, "{slug}");
            for slot in [army, worker] {
                assert_eq!(
                    slot.excludes,
                    std::slice::from_ref(&queen),
                    "{slug}: NotQueen"
                );
            }
            assert_eq!(auto_slot, Some(1), "{slug}: the worker rally");
        } else {
            assert_eq!(rally.slots.len(), 1, "{slug}");
            let slot = &rally.slots[0];
            assert_eq!(slot.used_by, RallyUsers::All, "{slug}");
            assert!(slot.excludes.is_empty(), "{slug}");
            assert!(slot.on_resources && slot.smart_on_ground, "{slug}");
            assert!(auto_slot.is_none_or(|slot| slot == 0), "{slug}");
        }
    }
    assert_eq!(hatcheries, 3, "the Hatchery, Lair and Hive analogues");
    for archetype in catalogs
        .units
        .archetypes()
        .filter(|a| a.trains_units.is_empty())
    {
        assert!(archetype.rally.is_none(), "{}: no rally", archetype.slug);
    }
    Ok(())
}

/// V-19 (plan `2026-11-27-launch-and-spell-visibility` §2.1 S4, S5, D-1): of every shipped spell
/// damage and missile node, only Psionic Storm's periodic pulse shows nothing of its caster
/// (`PsiStormDamage` `Visibility Hidden`, `liberty` EffectData:2099); every other one keeps the
/// engine default, a snapshot (`core` EffectData:62, :102), and none is visible.
#[test]
fn shipped_spell_visibility_follows_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let spells = catalogs.units.spells();
    let mut nodes = 0;
    for index in 0..spells.node_count() {
        let id = sim_types::EffectId(u16::try_from(index)?);
        let label = spells.label(id).ok_or("a label")?;
        let (sim_core::catalog::EffectNode::Damage { visibility, .. }
        | sim_core::catalog::EffectNode::LaunchMissile { visibility, .. }) =
            spells.node(id).ok_or("a node")?.kind
        else {
            continue;
        };
        nodes += 1;
        let expected = if label == "psionic_storm_damage" {
            AttackVisibility::Hidden
        } else {
            AttackVisibility::Snapshot
        };
        assert_eq!(visibility, expected, "{label}");
    }
    assert_eq!(
        nodes, 6,
        "the storm's two damages, Abduct's tongue, Fungal's missile and two damages"
    );
    Ok(())
}

/// One constant missile row of M-8: analogue, weapon block, top speed per tick, source, and the
/// open bug a row whose shipped speed is not the 5.0.14 mover's records (its value is then the
/// shipped one, so the table never passes a mismatch silently).
type ConstantMissileRow = (
    &'static str,
    &'static str,
    f32,
    &'static str,
    Option<&'static str>,
);

/// M-8 (plan `2026-11-28-missile-movers-b52` S12, S14): the constant speed of every shipped unit
/// missile weapon without movers, from the 5.0.14 mover its launch flies (the launch's `Movers`,
/// else its ammo unit's `Mover`; `MaxSpeed × 0.056` per tick, 3200 accelerations instant).
/// `MissileDefault` (`core` MoverData:13-25) and its children fly 18.75, 1.05 per tick.
const SC2_UNIT_MISSILE_SPEEDS: [ConstantMissileRow; 15] = [
    ("Roach", "weapon", 1.05, "AcidSalivaLM ammo AcidSalivaWeapon, liberty MoverData:670-679", None),
    ("Marauder", "weapon", 1.12, "PunisherGrenadesLM Movers PunisherGrenadesWeapon, liberty MoverData:662-669 (MaxSpeed 20)", None),
    ("Stalker", "weapon", 1.05, "ParticleDisruptors ammo StalkerWeapon, a MissileDefault child (liberty MoverData:601)", None),
    ("Stalker", "air_weapon", 1.05, "ParticleDisruptors ammo StalkerWeapon, a MissileDefault child (liberty MoverData:601)", None),
    ("Adept", "weapon", 1.05, "AdeptLM Movers AdeptWeapon, a MissileDefault child (void MoverData:370)", None),
    ("High Templar", "weapon", 1.05, "HighTemplarWeaponLM Movers HighTemplarWeaponMover, a MissileDefault child (balancemulti EffectData:1404-1409, MoverData:47)", None),
    ("Ravager", "weapon", 1.05, "RavagerWeaponLM ammo RavagerWeaponMissile, Mover MissileDefault (liberty UnitData:1124-1128)", None),
    ("Hydralisk", "weapon", 2.24, "NeedleSpinesLaunchMissile ammo NeedleSpinesWeapon, liberty MoverData:602-609 (Speed 40)", None),
    ("Hydralisk", "air_weapon", 2.24, "NeedleSpinesLaunchMissile ammo NeedleSpinesWeapon, liberty MoverData:602-609 (Speed 40)", None),
    ("Mutalisk", "weapon", 1.05, "GlaiveWurm ammo GlaiveWurmWeapon (the default ##id##Weapon, core EffectData:104), Mover MissileDefault (liberty UnitData:9271-9275)", None),
    ("Mutalisk", "air_weapon", 1.05, "GlaiveWurm ammo GlaiveWurmWeapon (the default ##id##Weapon, core EffectData:104), Mover MissileDefault (liberty UnitData:9271-9275)", None),
    ("Corruptor", "air_weapon", 1.05, "ParasiteSporeLaunchMissile ammo ParasiteSporeWeapon, Mover MissileDefault (liberty UnitData:9296-9300)", None),
    ("Queen", "weapon", 2.24, "AcidSpinesLM Movers AcidSpinesWeapon, liberty MoverData:610-619 (Speed 40)", None),
    ("Queen", "ground_weapon", 2.24, "AcidSpinesLM Movers AcidSpinesWeapon, liberty MoverData:610-619 (Speed 40)", None),
    ("Queen", "air_weapon", 2.24, "TalonsLM ammo TalonsMissileWeapon, swarm MoverData:335-344 (Speed 40)", None),
];

/// M-8: the structures' constant missiles, as [`SC2_UNIT_MISSILE_SPEEDS`].
const SC2_STRUCTURE_MISSILE_SPEEDS: [ConstantMissileRow; 3] = [
    ("Photon Cannon", "ground_weapon", 1.008, "PhotonCannonLM ammo PhotonCannonWeapon, liberty MoverData:407-416 (MaxSpeed 18)", None),
    ("Photon Cannon", "air_weapon", 1.008, "PhotonCannonLM ammo PhotonCannonWeapon, liberty MoverData:407-416 (MaxSpeed 18)", None),
    ("Spore Crawler", "air_weapon", 1.05, "SporeCrawler ammo SporeCrawlerWeapon (the default ##id##Weapon), Mover MissileDefault (liberty UnitData:8868-8872)", None),
];

/// M-8: every shipped missile weapon without movers flies the constant of its 5.0.14 mover, or
/// its row names the open bug that records the difference; every row names a shipped weapon.
#[test]
fn constant_missile_speeds_follow_starcraft() -> TestResult {
    for (structures, rows) in [
        (false, &SC2_UNIT_MISSILE_SPEEDS[..]),
        (true, &SC2_STRUCTURE_MISSILE_SPEEDS[..]),
    ] {
        let weapon_rows: Vec<WeaponRow> = rows
            .iter()
            .map(|&(unit, block, speed, source, _)| (unit, block, speed, source))
            .collect();
        check_weapons(
            "constant missile speed",
            structures,
            &weapon_rows,
            &[],
            |w| w.projectile_speed.is_some() && w.missile_movers.is_empty(),
            |w| w.projectile_speed.map_or(0.0, f),
        )?;
    }
    let design = include_str!("../../../DESIGN.md");
    for (unit, block, _, _, bug) in SC2_UNIT_MISSILE_SPEEDS
        .iter()
        .chain(&SC2_STRUCTURE_MISSILE_SPEEDS)
    {
        if let Some(bug) = bug {
            assert!(
                design.contains(&format!("| {bug} |")),
                "{unit} {block}: {bug} is a row of DESIGN.md §4"
            );
        }
    }
    Ok(())
}

/// Where a 5.0.14 missile phase ends, world units (`Outro` read by its sign, plan
/// `2026-11-28-missile-movers-b52` D-2): after a path flown (a positive `Outro`'s midpoint), or
/// once a path is left to the target (a negative `Outro`'s magnitude).
#[derive(Debug, Clone, Copy)]
enum End {
    /// A path flown since the release.
    Flown(f32),
    /// A path left to the impact.
    Remaining(f32),
}

/// One speed phase of a 5.0.14 missile mover in world units and ticks (`Speed`, `Acceleration`,
/// `MaxSpeed`, `Outro`), converted as the catalog's speeds and accelerations are: a Normal speed
/// `v` is `0.056 v` per tick, a Normal acceleration `a` is `0.003136 a` per tick squared (plan
/// `2026-11-27-weapon-delivery` §2).
type PhaseRow = (Option<f32>, f32, f32, Option<End>);

/// One mover of a launch effect's `Movers`: its `IfRangeLTE` bound (`None` on the last, which
/// admits any distance) and its phases.
type MoverRow = (Option<f32>, &'static [PhaseRow]);

/// W-18 (plan `2026-11-27-weapon-delivery` D-2): every shipped weapon with missile movers, by
/// analogue and block, with the 5.0.14 movers of its launch effect.
const SC2_MISSILE_MOVERS: [(&str, &str, &[MoverRow], f32, &str); 7] = [
    (
        "Phoenix",
        "air_weapon",
        &[
            // MissileDefault (core MoverData:13-25): Acceleration 3200 reaches MaxSpeed 18.75
            // within a tenth of a tick, so a constant 1.05.
            (Some(1.0), &[(Some(1.05), 0.0, 1.05, None)]),
            // IonCannonsWeapon{Left,Right} (liberty MoverData:446-484): Throw Speed 6 MaxSpeed 6
            // to Outro 1,1.5 (phases 0 and 1 merged, midpoint 1.25), then Guidance Acceleration
            // 60 to MaxSpeed 40; its IfRangeLTE 6 is the last band, so unbounded.
            (
                None,
                &[
                    (Some(0.336), 0.0, 0.336, Some(End::Flown(1.25))),
                    (None, 0.18816, 2.24, None),
                ],
            ),
        ],
        2.24,
        "IonCannonsLMLeft, IonCannonsLMRight, liberty EffectData:1972-1981 (S5, S7, S8)",
    ),
    (
        "Spine Crawler",
        "ground_weapon",
        &[
            // SpineCrawlerTentacleExtendShort (liberty MoverData:243-265): Throw Speed 1,
            // Acceleration 10, MaxSpeed 20, Outro 0.2,0.4 (midpoint 0.3); Guidance Acceleration
            // 140, MaxSpeed 200.
            (
                Some(3.0),
                &[
                    (Some(0.056), 0.03136, 1.12, Some(End::Flown(0.3))),
                    (None, 0.43904, 11.2, None),
                ],
            ),
            // SpineCrawlerTentacleExtendLong (:266-278): Guidance Speed 12, Acceleration 140,
            // MaxSpeed 200; its IfRangeLTE 7 is the weapon's range and the last band.
            (None, &[(Some(0.672), 0.43904, 11.2, None)]),
        ],
        11.2,
        "ImpalerTentacleLM, liberty EffectData:2151-2162 (S10, S11)",
    ),
    (
        "Viking",
        "air_weapon",
        &[
            // VikingFighterMissile (liberty MoverData:531-567), the ammo unit VikingFighterWeapon's
            // mover (UnitData:6901-6905): two Throw phases at Speed 2 merged (rule 2), switched at
            // phase 1's Outro 0.625,0.75 (midpoint 0.6875); then Guidance from Speed 2 accelerating
            // 56 to MaxSpeed 28, phase 3 carrying the acceleration (S11), so phases 2 and 3 merge.
            (
                None,
                &[
                    (Some(0.112), 0.0, 0.112, Some(End::Flown(0.6875))),
                    (Some(0.112), 0.175_616, 1.568, None),
                ],
            ),
        ],
        1.568,
        "LanzerTorpedoesLM, liberty EffectData:1707-1712 (plan 2026-11-28-missile-movers-b52 S2, S3)",
    ),
    (
        "Liberator",
        "air_weapon",
        &[
            // LiberatorMissile (void MoverData:384-422), the ammo unit LiberatorDamageMissile's mover
            // (UnitData:3411-3415): Throw reaching MaxSpeed 8 at Acceleration 3200 (instant, rule 4)
            // to Outro 2.5,3.5 (midpoint 3); then Guidance at MaxSpeed 16, phase 2 carrying it on and
            // phase 1's Outro -2.5,-1.5 changing no speed (rule 2).
            (
                None,
                &[
                    (Some(0.448), 0.0, 0.448, Some(End::Flown(3.0))),
                    (Some(0.896), 0.0, 0.896, None),
                ],
            ),
        ],
        0.896,
        "LiberatorDamageMissileLM, void EffectData:2979-2984 (plan 2026-11-28-missile-movers-b52 S5, S6)",
    ),
    (
        "Banshee",
        "ground_weapon",
        // BacklashRocketsLMWeapon (liberty MoverData:485-510): Guidance from Speed 10 accelerating 20
        // to MaxSpeed 50; phases 1 and 2 set no speed value and carry phase 0's on (rule 2).
        &[(None, &[(Some(0.56), 0.06272, 2.8, None)])],
        2.8,
        "BacklashRocketsLM, liberty EffectData:2558-2563 (plan 2026-11-28-missile-movers-b52 S8)",
    ),
    (
        "Missile Turret",
        "air_weapon",
        &[
            // LongboltMissileWeapon (liberty MoverData:589-600), the ammo unit's own mover
            // (UnitData:11605-11608): Guidance at Speed 10, MaxSpeed 10 until 4 from the target
            // (Outro -4), then the "kill phase" accelerating 200 to MaxSpeed 20.
            (
                None,
                &[
                    (Some(0.56), 0.0, 0.56, Some(End::Remaining(4.0))),
                    (None, 0.6272, 1.12, None),
                ],
            ),
        ],
        1.12,
        "LongboltMissileLM, liberty EffectData:1789-1794 (plan 2026-11-28-missile-movers-b52 S9, S10)",
    ),
    (
        "Brood Lord",
        "ground_weapon",
        &[
            // BroodLordWeaponRight (liberty MoverData:44-69; Left, :620-644, its mirror): Throw from
            // Speed 5 accelerating 3 to MaxSpeed 8 until Outro 1.5,4.5 (midpoint 3); then Guidance
            // accelerating 20 to MaxSpeed 40 with no Speed, carrying the speed in. Both bands are
            // IfRangeLTE 500, so one unbounded mover.
            (
                None,
                &[
                    (Some(0.28), 0.009_408, 0.448, Some(End::Flown(3.0))),
                    (None, 0.062_72, 2.24, None),
                ],
            ),
        ],
        2.24,
        "BroodlingEscortLaunchA, liberty EffectData:2664-2672 (plan 2026-11-28-broodlings S9, S19, D-2)",
    ),
];

/// W-18: the movers and top speed of every shipped weapon that has movers follow 5.0.14, and
/// every row names a shipped weapon.
#[test]
fn missile_movers_follow_starcraft() -> TestResult {
    let catalogs = sim_fixtures::shipped_catalogs();
    let close = |got: SimScalar, want: f32| (f(got) - want).abs() < 0.001;
    let mut seen = std::collections::BTreeSet::new();
    for archetype in catalogs.units.archetypes() {
        for (block, weapon) in weapon_blocks(archetype) {
            let Some(weapon) = weapon.filter(|weapon| !weapon.missile_movers.is_empty()) else {
                continue;
            };
            let name = analogue(archetype);
            if block == "weapon" && archetype.ground_weapon.as_ref() == Some(weapon) {
                continue;
            }
            let (_, _, movers, top, source) = SC2_MISSILE_MOVERS
                .iter()
                .find(|(unit, row_block, ..)| *unit == name && *row_block == block)
                .ok_or_else(|| format!("{} ({name}) {block}: no movers row", archetype.slug))?;
            let speed = weapon
                .projectile_speed
                .ok_or("a weapon with movers is a missile")?;
            assert!(close(speed, *top), "{name} {block}: top speed ({source})");
            assert_eq!(
                weapon.missile_movers.len(),
                movers.len(),
                "{name} {block} ({source})"
            );
            for (index, (mover, (bound, phases))) in
                weapon.missile_movers.iter().zip(movers.iter()).enumerate()
            {
                assert_eq!(
                    mover.up_to_range.map(f),
                    *bound,
                    "{name} {block} mover {index} bound ({source})"
                );
                assert_eq!(mover.phases.len(), phases.len(), "{name} mover {index}");
                for (phase, &(start, acceleration, max, ends)) in mover.phases.iter().zip(*phases) {
                    let same = |got: Option<SimScalar>, want: Option<f32>| match (got, want) {
                        (Some(got), Some(want)) => close(got, want),
                        (None, None) => true,
                        _ => false,
                    };
                    let same_end = match (phase.ends, ends) {
                        (Some(PhaseEnd::Flown(got)), Some(End::Flown(want)))
                        | (Some(PhaseEnd::Remaining(got)), Some(End::Remaining(want))) => {
                            close(got, want)
                        }
                        (None, None) => true,
                        _ => false,
                    };
                    assert!(
                        same(phase.speed, start)
                            && close(phase.acceleration, acceleration)
                            && close(phase.max_speed, max)
                            && same_end,
                        "{name} {block} mover {index}: {phase:?} ({source})"
                    );
                }
            }
            seen.insert(name.to_owned());
        }
    }
    for (unit, ..) in &SC2_MISSILE_MOVERS {
        assert!(
            seen.contains(*unit),
            "movers row {unit} has no shipped weapon"
        );
    }
    Ok(())
}

/// H-1 (plan `2026-11-28-carrier-interceptors` D-1, B-228): the Carrier's separation radius is the
/// Carrier's `SeparationRadius` 1.25 (`liberty` UnitData:3492), as its collision radius is its
/// `Radius` (the radius table). Found by analogue, never by slug (charter §3.3).
#[test]
fn the_carrier_separation_radius_follows_starcraft() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let carriers: Vec<_> = catalogs
        .units
        .archetypes()
        .filter(|archetype| analogue(archetype) == "Carrier")
        .collect();
    assert_eq!(carriers.len(), 1, "one archetype has the Carrier analogue");
    for carrier in carriers {
        assert!(
            (f(carrier.separation_radius) - 1.25).abs() < 0.001,
            "{}: separation radius {}, StarCraft II 1.25 (liberty UnitData:3492)",
            carrier.slug,
            f(carrier.separation_radius)
        );
    }
}
