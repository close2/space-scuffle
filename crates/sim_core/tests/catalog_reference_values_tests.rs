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

/// Costs the audit corrected: `(analogue, [minerals, gas], source)` (`CUnit` `CostResource`). The other
/// unit and structure costs match the game's build cost (evidence §10.1: a Zerg structure's data cost
/// includes the Drone; morphs and the Zergling pair are B-86).
const SC2_CORRECTED_COSTS: [(&str, [f32; 2], &str); 4] = [
    (
        "Armory",
        [150.0, 50.0],
        "liberty UnitData:6502; voidmulti UnitData:2100",
    ),
    ("Dark Shrine", [150.0, 150.0], "void UnitData:1914, :1915"),
    (
        "Liberator",
        [150.0, 125.0],
        "void UnitData:3305; voidmulti UnitData:468",
    ),
    (
        "Ultralisk",
        [275.0, 200.0],
        "voidmulti UnitData:1940; liberty UnitData:8063",
    ),
];

/// Research cost of every upgrade: `(upgrade slug, [minerals, gas], source)`, the research ability's
/// `InfoArray` `Resource` entries.
const SC2_RESEARCH_COSTS: [(&str, [f32; 2], &str); 82] = [
    (
        "infantry_weapons_1",
        [100.0, 100.0],
        "liberty AbilData:1700 (EngineeringBayResearch Research3)",
    ),
    (
        "infantry_weapons_2",
        [150.0, 150.0],
        "voidmulti AbilData:1811 (EngineeringBayResearch Research4)",
    ),
    (
        "infantry_weapons_3",
        [200.0, 200.0],
        "voidmulti AbilData:1815 (EngineeringBayResearch Research5)",
    ),
    (
        "infantry_armor_1",
        [100.0, 100.0],
        "liberty AbilData:1722 (EngineeringBayResearch Research7)",
    ),
    (
        "infantry_armor_2",
        [150.0, 150.0],
        "voidmulti AbilData:1819 (EngineeringBayResearch Research8)",
    ),
    (
        "infantry_armor_3",
        [200.0, 200.0],
        "voidmulti AbilData:1823 (EngineeringBayResearch Research9)",
    ),
    (
        "vehicle_weapons_1",
        [100.0, 100.0],
        "liberty AbilData:1875 (ArmoryResearch Research6)",
    ),
    (
        "vehicle_weapons_2",
        [175.0, 175.0],
        "liberty AbilData:1880 (ArmoryResearch Research7)",
    ),
    (
        "vehicle_weapons_3",
        [250.0, 250.0],
        "liberty AbilData:1885 (ArmoryResearch Research8)",
    ),
    (
        "ship_weapons_1",
        [100.0, 100.0],
        "liberty AbilData:1905 (ArmoryResearch Research12)",
    ),
    (
        "ship_weapons_2",
        [175.0, 175.0],
        "liberty AbilData:1910 (ArmoryResearch Research13)",
    ),
    (
        "ship_weapons_3",
        [250.0, 250.0],
        "liberty AbilData:1915 (ArmoryResearch Research14)",
    ),
    (
        "vehicle_ship_plating_1",
        [100.0, 100.0],
        "voidmulti AbilData:26 (ArmoryResearch Research15)",
    ),
    (
        "vehicle_ship_plating_2",
        [175.0, 175.0],
        "voidmulti AbilData:31 (ArmoryResearch Research16)",
    ),
    (
        "vehicle_ship_plating_3",
        [250.0, 250.0],
        "voidmulti AbilData:36 (ArmoryResearch Research17)",
    ),
    (
        "combat_shield",
        [100.0, 100.0],
        "swarm AbilData:984 (BarracksTechLabResearch Research2)",
    ),
    (
        "overclock_stim",
        [100.0, 100.0],
        "swarm AbilData:980 (BarracksTechLabResearch Research1)",
    ),
    (
        "concussive_shells",
        [50.0, 50.0],
        "swarm AbilData:988 (BarracksTechLabResearch Research3)",
    ),
    (
        "infernal_preigniter",
        [100.0, 100.0],
        "voidmulti AbilData:319 (FactoryTechLabResearch Research2)",
    ),
    (
        "drilling_claws",
        [75.0, 75.0],
        "voidmulti AbilData:323 (FactoryTechLabResearch Research5)",
    ),
    (
        "smart_servos",
        [100.0, 100.0],
        "void AbilData:653 (FactoryTechLabResearch Research7)",
    ),
    (
        "personal_cloaking",
        [150.0, 150.0],
        "liberty AbilData:1843 (GhostAcademyResearch Research1)",
    ),
    (
        "cloaking_field",
        [100.0, 100.0],
        "voidmulti AbilData:967 (StarportTechLabResearch Research1)",
    ),
    (
        "neosteel_armor",
        [150.0, 150.0],
        "liberty AbilData:1695 (EngineeringBayResearch Research2)",
    ),
    (
        "hi_sec_auto_tracking",
        [100.0, 100.0],
        "liberty AbilData:1690 (EngineeringBayResearch Research1)",
    ),
    (
        "advanced_ballistics",
        [150.0, 150.0],
        "liberty AbilData:3369 (FusionCoreResearch Research2)",
    ),
    (
        "ground_weapons_1",
        [100.0, 100.0],
        "liberty AbilData:2102 (ForgeResearch Research1)",
    ),
    (
        "ground_weapons_2",
        [150.0, 150.0],
        "swarm AbilData:502 (ForgeResearch Research2)",
    ),
    (
        "ground_weapons_3",
        [200.0, 200.0],
        "swarm AbilData:506 (ForgeResearch Research3)",
    ),
    (
        "ground_armor_1",
        [100.0, 100.0],
        "liberty AbilData:2117 (ForgeResearch Research4)",
    ),
    (
        "ground_armor_2",
        [150.0, 150.0],
        "swarm AbilData:510 (ForgeResearch Research5)",
    ),
    (
        "ground_armor_3",
        [200.0, 200.0],
        "swarm AbilData:514 (ForgeResearch Research6)",
    ),
    (
        "plasma_shields_1",
        [150.0, 150.0],
        "swarm AbilData:518 (ForgeResearch Research7)",
    ),
    (
        "plasma_shields_2",
        [200.0, 200.0],
        "voidmulti AbilData:404 (ForgeResearch Research8)",
    ),
    (
        "plasma_shields_3",
        [250.0, 250.0],
        "voidmulti AbilData:408 (ForgeResearch Research9)",
    ),
    (
        "air_weapons_1",
        [100.0, 100.0],
        "liberty AbilData:3379 (CyberneticsCoreResearch Research1)",
    ),
    (
        "air_weapons_2",
        [175.0, 175.0],
        "liberty AbilData:3384 (CyberneticsCoreResearch Research2)",
    ),
    (
        "air_weapons_3",
        [250.0, 250.0],
        "liberty AbilData:3389 (CyberneticsCoreResearch Research3)",
    ),
    (
        "air_armor_1",
        [100.0, 100.0],
        "voidmulti AbilData:301 (CyberneticsCoreResearch Research4)",
    ),
    (
        "air_armor_2",
        [175.0, 175.0],
        "voidmulti AbilData:305 (CyberneticsCoreResearch Research5)",
    ),
    (
        "air_armor_3",
        [250.0, 250.0],
        "voidmulti AbilData:309 (CyberneticsCoreResearch Research6)",
    ),
    (
        "warp_gate_tech",
        [50.0, 50.0],
        "liberty AbilData:3409 (CyberneticsCoreResearch Research7)",
    ),
    (
        "charge",
        [100.0, 100.0],
        "voidmulti AbilData:1075 (TwilightCouncilResearch Research1)",
    ),
    (
        "phase_blink",
        [150.0, 150.0],
        "liberty AbilData:3434 (TwilightCouncilResearch Research2)",
    ),
    (
        "resonating_glaives",
        [100.0, 100.0],
        "void AbilData:834 (TwilightCouncilResearch Research3)",
    ),
    (
        "shadow_stride",
        [100.0, 100.0],
        "voidmulti AbilData:1319 (DarkShrineResearch Research1)",
    ),
    (
        "psionic_storm",
        [200.0, 200.0],
        "liberty AbilData:2184 (TemplarArchivesResearch Research5)",
    ),
    (
        "extended_thermal_lance",
        [150.0, 150.0],
        "voidmulti AbilData:865 (RoboticsBayResearch Research6)",
    ),
    (
        "gravitic_boosters",
        [100.0, 100.0],
        "liberty AbilData:2151 (RoboticsBayResearch Research2)",
    ),
    (
        "gravitic_drive",
        [100.0, 100.0],
        "liberty AbilData:2158 (RoboticsBayResearch Research3)",
    ),
    (
        "anion_pulse_crystals",
        [150.0, 150.0],
        "swarm AbilData:1050 (FleetBeaconResearch Research3)",
    ),
    (
        "flux_vanes",
        [100.0, 100.0],
        "voidmulti AbilData:381 (FleetBeaconResearch Research5)",
    ),
    (
        "melee_attacks_1",
        [100.0, 100.0],
        "liberty AbilData:2247 (evolutionchamberresearch Research1)",
    ),
    (
        "melee_attacks_2",
        [150.0, 150.0],
        "liberty AbilData:2252 (evolutionchamberresearch Research2)",
    ),
    (
        "melee_attacks_3",
        [200.0, 200.0],
        "liberty AbilData:2257 (evolutionchamberresearch Research3)",
    ),
    (
        "missile_attacks_1",
        [100.0, 100.0],
        "liberty AbilData:2277 (evolutionchamberresearch Research7)",
    ),
    (
        "missile_attacks_2",
        [150.0, 150.0],
        "liberty AbilData:2282 (evolutionchamberresearch Research8)",
    ),
    (
        "missile_attacks_3",
        [200.0, 200.0],
        "liberty AbilData:2287 (evolutionchamberresearch Research9)",
    ),
    (
        "ground_carapace_1",
        [150.0, 150.0],
        "liberty AbilData:2262 (evolutionchamberresearch Research4)",
    ),
    (
        "ground_carapace_2",
        [200.0, 200.0],
        "voidmulti AbilData:1680 (evolutionchamberresearch Research5)",
    ),
    (
        "ground_carapace_3",
        [250.0, 250.0],
        "voidmulti AbilData:1684 (evolutionchamberresearch Research6)",
    ),
    (
        "flyer_attacks_1",
        [100.0, 100.0],
        "liberty AbilData:2430 (SpireResearch Research1)",
    ),
    (
        "flyer_attacks_2",
        [175.0, 175.0],
        "liberty AbilData:2437 (SpireResearch Research2)",
    ),
    (
        "flyer_attacks_3",
        [250.0, 250.0],
        "liberty AbilData:2444 (SpireResearch Research3)",
    ),
    (
        "flyer_carapace_1",
        [100.0, 100.0],
        "voidmulti AbilData:1797 (SpireResearch Research4)",
    ),
    (
        "flyer_carapace_2",
        [175.0, 175.0],
        "voidmulti AbilData:1801 (SpireResearch Research5)",
    ),
    (
        "flyer_carapace_3",
        [250.0, 250.0],
        "voidmulti AbilData:1805 (SpireResearch Research6)",
    ),
    (
        "metabolic_boost",
        [100.0, 100.0],
        "liberty AbilData:2410 (SpawningPoolResearch Research2)",
    ),
    (
        "adrenal_glands",
        [200.0, 200.0],
        "liberty AbilData:2403 (SpawningPoolResearch Research1)",
    ),
    (
        "centrifugal_hooks",
        [100.0, 100.0],
        "voidmulti AbilData:1772 (BanelingNestResearch Research1)",
    ),
    (
        "glial_reconstitution",
        [100.0, 100.0],
        "liberty AbilData:901 (RoachWarrenResearch Research2)",
    ),
    (
        "tunneling_claws",
        [100.0, 100.0],
        "voidmulti AbilData:859 (RoachWarrenResearch Research3)",
    ),
    (
        "muscular_augments",
        [100.0, 100.0],
        "voidmulti AbilData:536 (HydraliskDenResearch Research2)",
    ),
    (
        "grooved_spines",
        [75.0, 75.0],
        "voidmulti AbilData:529 (HydraliskDenResearch Research1)",
    ),
    (
        "nanomuscular_swell",
        [100.0, 100.0],
        "voidmulti AbilData:543 (HydraliskDenResearch Research3)",
    ),
    (
        "adaptive_talons",
        [100.0, 100.0],
        "voidmulti AbilData:1348 (LurkerDenResearch Research1)",
    ),
    (
        "seismic_spines",
        [150.0, 150.0],
        "voidmulti AbilData:1355 (LurkerDenResearch Research2)",
    ),
    (
        "pneumatized_carapace",
        [100.0, 100.0],
        "swarm AbilData:1284 (LairResearch Research2)",
    ),
    (
        "chitinous_plating",
        [150.0, 150.0],
        "liberty AbilData:1065 (UltraliskCavernResearch Research3)",
    ),
    (
        "anabolic_synthesis",
        [150.0, 150.0],
        "voidmulti AbilData:1105 (UltraliskCavernResearch Research1)",
    ),
    (
        "microbial_shroud",
        [150.0, 150.0],
        "voidmulti AbilData:599 (InfestationPitResearch Research6)",
    ),
    (
        "neural_parasite",
        [150.0, 150.0],
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
            [minerals as u32, gas as u32],
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
            [*minerals as u32, *gas as u32],
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
