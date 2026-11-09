//! Reference-value tests for the non-time catalog audit (plan `2026-11-05-sc2-deviation-fixes`
//! package 9, B-47).
//!
//! # Responsibility
//! Holds the `StarCraft` II 5.0.14 values of every non-time field the audit corrected, one table
//! per field kind, and checks the shipped catalog against them through its public API.
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

/// The archetype's weapons by block name; `weapon` is the ground fallback (a structure's is a
/// copy of its ground weapon, so structures skip it).
fn weapon_blocks(archetype: &UnitArchetype) -> [(&'static str, Option<&WeaponSpec>); 3] {
    [
        ("weapon", archetype.weapon.as_ref()),
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
const SC2_UNIT_ATTRIBUTES: [(&str, [f32; 1], &str); 41] = [
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
        "Carrier / Mothership",
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
const SC2_UNIT_DAMAGE: [WeaponRow; 41] = [
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
        &["Phoenix", "Carrier / Mothership", "Brood Lord"],
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
        &["Phoenix", "Carrier / Mothership"],
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
/// Carrier / Mothership composite and the saucer without one analogue are class E (evidence §10.1).
const SC2_UNIT_VITALS: [(&str, [f32; 3], &str); 40] = [
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
];

#[test]
fn b47_unit_vitals_follow_starcraft() -> TestResult {
    check_table(
        "life, shields, armour",
        false,
        &SC2_UNIT_VITALS,
        &["Phoenix", "Carrier / Mothership"],
        |a| [f(a.max_hp), f(a.max_shields), f(a.life_armor)],
    )
}

/// Unit sight (`CUnit` `Sight`).
const SC2_UNIT_SIGHT: [(&str, [f32; 1], &str); 41] = [
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
    ("Carrier / Mothership", [12.0], "liberty UnitData:3507"),
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
];

#[test]
fn b47_unit_sight_follows_starcraft() -> TestResult {
    check_table("sight", false, &SC2_UNIT_SIGHT, &["Phoenix"], |a| {
        [f(a.sight_radius)]
    })
}

/// Maximum range of every armed unit's weapon (`CWeapon` `Range`; `core` WeaponData:12 is the class
/// default 5).
const SC2_UNIT_RANGE: [WeaponRow; 43] = [
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
];

#[test]
fn b47_unit_weapon_ranges_follow_starcraft() -> TestResult {
    check_weapons(
        "range",
        false,
        &SC2_UNIT_RANGE,
        &["Phoenix", "Carrier / Mothership"],
        |_| true,
        |w| f(w.max_range),
    )
}

/// Unit collision radius (`CUnit` `Radius`; `core` UnitData:41 is the class default 0.5).
const SC2_UNIT_RADIUS: [(&str, [f32; 1], &str); 40] = [
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
];

#[test]
fn b47_unit_radii_follow_starcraft() -> TestResult {
    check_table(
        "radius",
        false,
        &SC2_UNIT_RADIUS,
        &["Phoenix", "Carrier / Mothership"],
        |a| [f(a.collision_radius)],
    )
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

/// Where a weapon sets no `Arc`: no `CWeapon` default sets it (`core` WeaponData:3-31), so the
/// engine's 0 applies (catalog-time audit §10.3).
const ARC_UNSET: &str = "Arc not set: engine 0 (core WeaponData:3-31)";

/// Acquisition arc of every unit weapon whose mount matches the game: `Arc` ÷ 2 of the weapon,
/// in binary angle units (65 536 a turn; 32 768 is 180°, every direction). Plan
/// `2026-11-05-sc2-deviation-fixes` WP10, B-29; weapons per catalog-time audit §10.3. The row
/// with block `siege_mode` is the Siege Tank's Siege Mode cannon, checked through its mode.
const SC2_UNIT_ACQUIRE_ARC: [WeaponRow; 41] = [
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
        32_768.0,
        "a turret with an open arc here; the game's hull weapon, Arc 5.625, is B-28",
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
        "Carrier / Mothership",
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

/// The analogues whose weapon mount differs from the game (catalog-time audit §10.5, B-28): their
/// arcs belong to the mount and change with it.
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
        for mode in &archetype.modes {
            if let Some(Some(weapon)) = &mode.ground_weapon_override {
                let got = f32::from(weapon.acquire_arc.raw());
                assert!(
                    (got - want).abs() < 0.001,
                    "{} {}: acquisition arc {got}, StarCraft II {want} ({source})",
                    archetype.slug,
                    mode.mode_name
                );
                checked += 1;
            }
        }
    }
    assert_eq!(
        checked, 1,
        "one weapon override: the Siege Tank's Siege Mode"
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
const SC2_WEAPON_MOUNTS: [MountRow; 4] = [
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
