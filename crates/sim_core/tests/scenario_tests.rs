//! # Scenario Builder Integration Tests
//!
//! Verifies end-to-end scenario construction, map document conversion, start loadout
//! parsing, deterministic entity placement, and comprehensive error paths.
//!
//! ## Subsystem Invariants Tested
//! 1. **Complete Entity Spawning:** Two-player setup spawns exactly 2 dome town halls and 12 welder drones.
//! 2. **Exact Positioning:** Town hall center matches the start location footprint center.
//! 3. **Placement Validity:** Units are placed strictly on ground-pathable, non-ramp, non-abyss cells within Chebyshev distance 1..=3.
//! 4. **Deterministic Checksums:** Stepping two identically initialized worlds for 200 ticks yields bit-for-bit identical checksums.
//! 5. **Error Coverage:** Every typed [`ScenarioError`] variant is explicitly verified with negative test cases.
//! 6. **Catalog Integrity:** Authoritative starting loadouts file on disk resolves fully against the starter unit roster.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::cast_possible_wrap
)]

use std::path::{Path, PathBuf};

use sim_core::scenario::{
    load_scenario_world, ring_perimeter, PlayerSetup, ScenarioError, ScenarioManifest,
    StartLoadoutCatalog,
};
use sim_map::validation::validate_map;
use sim_map::{
    CellFlags, CliffLevel, MapCellSpec, MapDocument, MapFile, MapMetadata, ResourceKind,
    TerrainKind,
};
use sim_types::{PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle};

fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

fn resolve_path(rel: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join(rel)
}

/// Helper to build a minimal valid 32x32 document with two start locations.
fn base_valid_doc() -> MapDocument {
    let width = 32;
    let height = 32;
    let total = (width * height) as usize;

    let cells = vec![
        MapCellSpec {
            level: CliffLevel(0),
            ramp: None,
            terrain: TerrainKind::Grass,
            flags: CellFlags::BUILDABLE | CellFlags::GROUND_PATHABLE,
        };
        total
    ];

    let mut doc = MapDocument {
        metadata: MapMetadata {
            title: "Test Map".to_string(),
            author: "Tester".to_string(),
            description: "Minimal test map".to_string(),
            theme: "orbital_foundry".to_string(),
        },
        width,
        height,
        cells,
        resources: Vec::new(),
        watchtowers: Vec::new(),
        start_locations: Vec::new(),
    };

    let start_0_id = doc.next_entity_id();
    doc.start_locations.push(sim_map::MapStartLocation {
        id: start_0_id,
        player_slot: 0,
        cell: (4, 4),
        footprint: (5, 5),
    });

    let start_1_id = doc.next_entity_id();
    doc.start_locations.push(sim_map::MapStartLocation {
        id: start_1_id,
        player_slot: 1,
        cell: (22, 22),
        footprint: (5, 5),
    });

    let report = validate_map(&doc);
    assert!(
        report.is_playable(),
        "base_valid_doc must be playable: {:?}",
        report.errors
    );
    doc
}

#[test]
fn showcase_two_players_spawns_expected_entities() {
    let map_path = resolve_path("../../assets/maps/showcase_canyon.scmap.ron");
    let file = MapFile::load(&map_path).expect("failed to load showcase_canyon.scmap.ron");
    let doc = file
        .decode()
        .expect("failed to decode showcase_canyon.scmap.ron");

    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");

    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "standard_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "standard_automaton".to_string(),
        },
    ];

    let world = sim_fixtures::scenario_world(&doc, &loadouts, &players, 42)
        .expect("build_world should succeed");

    // Exactly 2 structures + 12 units = 14 entities.
    assert_eq!(world.entities.len(), 14);

    let mut structures = Vec::new();
    let mut units = Vec::new();
    for entity in world.entities.values() {
        let arch = world
            .catalog
            .get(entity.unit_type_id)
            .expect("archetype exists");
        if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
            structures.push(entity);
        } else {
            units.push(entity);
        }
    }

    assert_eq!(structures.len(), 2);
    assert_eq!(units.len(), 12);

    // Verify Player 0 town hall center: cell (6, 6) + 2.5 = (8.5, 8.5)
    let p0_th = structures
        .iter()
        .find(|e| e.owner == PlayerId(0))
        .expect("Player 0 town hall must exist");
    assert_eq!(
        p0_th.position,
        SimVec2::new(SimScalar::from_num(8.5), SimScalar::from_num(8.5))
    );

    // Verify Player 1 town hall center: cell (53, 53) + 2.5 = (55.5, 55.5)
    let p1_th = structures
        .iter()
        .find(|e| e.owner == PlayerId(1))
        .expect("Player 1 town hall must exist");
    assert_eq!(
        p1_th.position,
        SimVec2::new(SimScalar::from_num(55.5), SimScalar::from_num(55.5))
    );

    // Verify all units are on valid pathable, non-ramp, non-abyss cells
    // and are within Chebyshev distance 1..=3 from their player's town hall footprint.
    for entity in &units {
        let ux = entity.position.x.0.to_num::<u32>();
        let uy = entity.position.y.0.to_num::<u32>();
        let cell = world.map.cell(ux, uy).expect("unit cell in bounds");

        assert!(
            cell.flags.contains(CellFlags::GROUND_PATHABLE),
            "unit cell must be GROUND_PATHABLE"
        );
        assert_ne!(
            cell.terrain_kind,
            TerrainKind::Abyss,
            "unit cell must not be Abyss"
        );
        assert!(cell.ramp.is_none(), "unit cell must not be a ramp");
        assert!(
            !cell.flags.contains(CellFlags::RAMP),
            "unit cell must not have RAMP flag"
        );

        let (fp_min_x, fp_min_y) = if entity.owner == PlayerId(0) {
            (6, 6)
        } else {
            (53, 53)
        };
        let fp_max_x = fp_min_x + 4;
        let fp_max_y = fp_min_y + 4;

        let ix = ux as i32;
        let iy = uy as i32;
        let dx = if ix < fp_min_x {
            fp_min_x - ix
        } else if ix > fp_max_x {
            ix - fp_max_x
        } else {
            0
        };
        let dy = if iy < fp_min_y {
            fp_min_y - iy
        } else if iy > fp_max_y {
            iy - fp_max_y
        } else {
            0
        };
        let chebyshev = dx.max(dy);
        assert!(
            (1..=3).contains(&chebyshev),
            "unit at ({ix}, {iy}) has chebyshev distance {chebyshev} which is outside 1..=3"
        );
    }
}

#[test]
fn build_world_is_deterministic() {
    let map_path = resolve_path("../../assets/maps/showcase_canyon.scmap.ron");
    let file = MapFile::load(&map_path).expect("failed to load showcase_canyon.scmap.ron");
    let doc = file
        .decode()
        .expect("failed to decode showcase_canyon.scmap.ron");

    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");

    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "standard_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "standard_automaton".to_string(),
        },
    ];

    let seed = 0x5CA1_AB1E;
    let mut world1 =
        sim_fixtures::scenario_world(&doc, &loadouts, &players, seed).expect("build world 1");
    let mut world2 =
        sim_fixtures::scenario_world(&doc, &loadouts, &players, seed).expect("build world 2");

    assert_eq!(
        world1.compute_checksum(),
        world2.compute_checksum(),
        "initial checksums must match"
    );

    for tick in 1..=200 {
        let bundle = empty_bundle(tick);
        world1.step(&bundle);
        world2.step(&bundle);

        assert_eq!(
            world1.compute_checksum(),
            world2.compute_checksum(),
            "checksums diverged at tick {tick}"
        );
    }
}

#[test]
fn error_invalid_map() {
    let mut doc = base_valid_doc();
    // Invalidate map by clearing start locations (requires at least MIN_START_LOCATIONS = 2).
    doc.start_locations.clear();

    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");
    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "standard_automaton".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::InvalidMap(ref errors)) if !errors.is_empty()),
        "expected non-empty InvalidMap error"
    );
}

#[test]
fn error_missing_start_location() {
    let doc = base_valid_doc();
    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");

    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "standard_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 7, // Slot 7 does not exist on doc
            loadout_slug: "standard_automaton".to_string(),
        },
    ];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::MissingStartLocation { slot: 7 })),
        "expected MissingStartLocation for slot 7"
    );
}

#[test]
fn error_unknown_loadout() {
    let doc = base_valid_doc();
    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");

    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "non_existent_faction".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::UnknownLoadout(ref slug)) if slug == "non_existent_faction"),
        "expected UnknownLoadout for non_existent_faction"
    );
}

#[test]
fn error_unknown_slug() {
    let doc = base_valid_doc();
    let raw_ron = r#"
    (
        loadouts: [
            (
                slug: "custom_loadout",
                display_name: "Custom",
                town_hall: "ghost_building_slug",
                units: [],
            ),
        ],
    )
    "#;
    let loadouts = StartLoadoutCatalog::from_ron_str(raw_ron).expect("valid RON");
    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "custom_loadout".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::UnknownSlug(ref slug)) if slug == "ghost_building_slug"),
        "expected UnknownSlug for ghost_building_slug"
    );
}

#[test]
fn error_town_hall_not_a_dome() {
    let doc = base_valid_doc();
    // Point town_hall to "welder_drone", which is a mobile unit (ShapeKind::UnitSphere), not a dome.
    let raw_ron = r#"
    (
        loadouts: [
            (
                slug: "unit_as_hall",
                display_name: "Unit as Hall",
                town_hall: "welder_drone",
                units: [],
            ),
        ],
    )
    "#;
    let loadouts = StartLoadoutCatalog::from_ron_str(raw_ron).expect("valid RON");
    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "unit_as_hall".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::TownHallNotADome(ref slug)) if slug == "welder_drone"),
        "expected TownHallNotADome for welder_drone"
    );
}

#[test]
fn error_town_hall_does_not_fit() {
    let mut doc = base_valid_doc();
    // Reduce footprint of slot 0 to 3x3, which is smaller than 5x5 foundry_core_dome.
    doc.start_locations[0].footprint = (3, 3);

    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");
    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "standard_automaton".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::TownHallDoesNotFit { slot: 0 })),
        "expected TownHallDoesNotFit for slot 0"
    );
}

#[test]
fn error_not_enough_spawn_cells() {
    let doc = base_valid_doc();
    // A 5x5 footprint at distances 1..=3 has exactly (24 + 32 + 40) = 96 available perimeter cells.
    // Requesting 100 units exceeds the available spawn ring capacity.
    let raw_ron = r#"
    (
        loadouts: [
            (
                slug: "swarms_overflow",
                display_name: "Too Many Units",
                town_hall: "foundry_core_dome",
                units: [
                    (slug: "welder_drone", count: 100),
                ],
            ),
        ],
    )
    "#;
    let loadouts = StartLoadoutCatalog::from_ron_str(raw_ron).expect("valid RON");
    let players = vec![PlayerSetup {
        player_slot: 0,
        loadout_slug: "swarms_overflow".to_string(),
    }];

    let res = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1);
    assert!(
        matches!(res, Err(ScenarioError::NotEnoughSpawnCells { slot: 0 })),
        "expected NotEnoughSpawnCells for slot 0"
    );
}

#[test]
fn error_ron_syntax() {
    let raw_invalid_ron = "( loadouts: [ broken ron syntax ... ";
    let res = StartLoadoutCatalog::from_ron_str(raw_invalid_ron);
    assert!(
        matches!(res, Err(ScenarioError::Ron(_))),
        "expected Ron error"
    );
}

#[test]
fn error_io_on_missing_file() {
    let missing_path = PathBuf::from("assets/catalog/non_existent_loadouts_file_12345.ron");
    let res = StartLoadoutCatalog::load(&missing_path);
    assert!(
        matches!(res, Err(ScenarioError::Io(ref err)) if err.kind() == std::io::ErrorKind::NotFound),
        "expected NotFound Io error"
    );
}

#[test]
fn real_loadout_file_resolves() {
    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadout_catalog =
        StartLoadoutCatalog::load(&loadouts_path).expect("start_loadouts.ron must be valid");

    assert_ne!(loadout_catalog.loadouts.len(), 0);

    let unit_catalog = sim_fixtures::shipped_catalogs().units;

    for loadout in &loadout_catalog.loadouts {
        // Town hall slug must resolve in the unit catalog.
        let th_arch = unit_catalog
            .get_by_slug(&loadout.town_hall)
            .expect("town hall slug must resolve in catalog");
        assert!(
            matches!(th_arch.shape, ShapeKind::BuildingDome { .. }),
            "town hall '{}' must have BuildingDome shape",
            loadout.town_hall
        );

        // Every starting unit slug must resolve in the unit catalog.
        for unit_entry in &loadout.units {
            assert!(
                unit_entry.count > 0,
                "unit count for '{}' must be positive",
                unit_entry.slug
            );
            assert!(
                unit_catalog.get_by_slug(&unit_entry.slug).is_some(),
                "unit slug '{}' must resolve in catalog",
                unit_entry.slug
            );
        }
    }
}

#[test]
fn ring_perimeter_order_and_len() {
    let x0 = 5;
    let x1 = 11;
    let y0 = 5;
    let y1 = 11;

    let cells: Vec<(i32, i32)> = ring_perimeter(x0, x1, y0, y1).collect();

    // Exactly 2 * (x1 - x0 + y1 - y0) = 2 * (6 + 6) = 24
    assert_eq!(cells.len(), 24);

    // Order verification:
    // South edge: (5..=11, 5) -> 7 cells
    for (i, x) in (x0..=x1).enumerate() {
        assert_eq!(cells[i], (x, y0));
    }
    // East edge: (11, 6..=11) -> 6 cells
    for (i, y) in ((y0 + 1)..=y1).enumerate() {
        assert_eq!(cells[7 + i], (x1, y));
    }
    // North edge: (10 down to 5, 11) -> 6 cells
    for (i, x) in (x0..x1).rev().enumerate() {
        assert_eq!(cells[13 + i], (x, y1));
    }
    // West edge: (5, 10 down to 6) -> 5 cells
    for (i, y) in ((y0 + 1)..y1).rev().enumerate() {
        assert_eq!(cells[19 + i], (x0, y));
    }

    // All coordinates must be unique
    let mut unique = std::collections::BTreeSet::new();
    for c in &cells {
        assert!(unique.insert(*c), "duplicate coordinate in ring: {c:?}");
    }
    assert_eq!(unique.len(), 24);

    // Minimal 1x1 perimeter test (surrounding a single cell at (1, 1))
    let mini_cells: Vec<(i32, i32)> = ring_perimeter(0, 2, 0, 2).collect();
    assert_eq!(mini_cells.len(), 8);
}

#[test]
fn units_never_spawn_on_other_cliff_level() {
    let mut doc = base_valid_doc();

    // Set the upper area (y >= 4) to CliffLevel(1).
    // The southern row band (y < 4) remains CliffLevel(0).
    // Start 0 at (4, 4) with footprint (5, 5) is at CliffLevel(1),
    // and its south edge at distance 1 is at y = 3 (CliffLevel(0)).
    for y in 4u32..32 {
        for x in 0u32..32 {
            let idx = (y * 32 + x) as usize;
            doc.cells[idx].level = CliffLevel(1);
        }
    }

    let report = validate_map(&doc);
    assert!(
        report.is_playable(),
        "modified cliff map must be playable: {:?}",
        report.errors
    );

    let loadouts_path = resolve_path("../../assets/catalog/start_loadouts.ron");
    let loadouts =
        StartLoadoutCatalog::load(&loadouts_path).expect("failed to load start loadouts");

    let players = vec![
        PlayerSetup {
            player_slot: 0,
            loadout_slug: "standard_automaton".to_string(),
        },
        PlayerSetup {
            player_slot: 1,
            loadout_slug: "standard_automaton".to_string(),
        },
    ];

    let world = sim_fixtures::scenario_world(&doc, &loadouts, &players, 123)
        .expect("build_world must succeed");

    let p0_units: Vec<_> = world
        .entities
        .values()
        .filter(|e| {
            e.owner == PlayerId(0)
                && !matches!(
                    world.catalog.get(e.unit_type_id).unwrap().shape,
                    ShapeKind::BuildingDome { .. }
                )
        })
        .collect();

    assert_eq!(p0_units.len(), 6, "all 6 welder drones must be spawned");

    for u in &p0_units {
        let ux = u.position.x.0.to_num::<u32>();
        let uy = u.position.y.0.to_num::<u32>();
        let cell = world.map.cell(ux, uy).expect("unit cell in bounds");
        assert_eq!(
            cell.cliff_level,
            CliffLevel(1),
            "unit at ({ux}, {uy}) spawned on level {:?}, expected level 1",
            cell.cliff_level
        );
        // Explicitly assert that no units spawned on the lower cliff (y < 4)
        assert!(
            uy >= 4,
            "unit spawned at y = {uy} which is on the lower cliff"
        );
    }
}

fn assets_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

#[test]
fn the_skirmish_scenario_is_a_playable_start_from_real_files() {
    let world = load_scenario_world(&assets_root(), Path::new("scenarios/skirmish.ron"))
        .expect("shipped skirmish scenario builds");
    assert_eq!(world.current_tick, 0);
    assert_eq!(world.match_state.participants.count_ones(), 2);
    for slot in [0_u8, 1] {
        let owned: Vec<_> = world
            .entities
            .values()
            .filter(|e| e.owner == PlayerId(slot))
            .collect();
        let archetypes: Vec<_> = owned
            .iter()
            .map(|e| world.catalog.get(e.unit_type_id).expect("shipped identity"))
            .collect();
        assert_eq!(
            archetypes.iter().filter(|a| a.is_resource_dropoff).count(),
            1,
            "one depot"
        );
        assert_eq!(
            archetypes
                .iter()
                .filter(|a| a.capabilities.harvester.is_some() && a.capabilities.can_construct)
                .count(),
            6,
            "six workers who can also build"
        );
        let bank = world.player_resources[usize::from(slot)];
        assert_eq!((bank.minerals, bank.gas), (50, 0));
        assert_eq!((bank.supply_used, bank.supply_max), (6, 15));

        let home = owned
            .iter()
            .find(|e| e.is_immobile)
            .expect("depot")
            .position;
        let nearby = |kind| {
            world
                .map
                .resources()
                .iter()
                .filter(|n| n.kind == kind && n.pos.distance(home) < SimScalar::from_num(12))
                .count()
        };
        assert_eq!(nearby(ResourceKind::Mineral), 8);
        assert_eq!(nearby(ResourceKind::Gas), 2);
    }
    assert_eq!(world.diagnostics(), []);
}

#[test]
fn a_loadout_without_a_bank_keeps_the_default() {
    let loadouts = StartLoadoutCatalog::load(assets_root().join("catalog/start_loadouts.ron"))
        .expect("shipped loadouts");
    assert_eq!(
        loadouts
            .get("battle_automaton")
            .expect("shipped loadout")
            .starting_resources,
        None
    );
}

#[test]
fn an_unknown_scenario_version_is_refused() {
    let text = r#"(format_version: 2, map: "m", seed: 1, players: [])"#;
    assert!(matches!(
        ScenarioManifest::from_ron_str(text),
        Err(ScenarioError::UnsupportedVersion(2))
    ));
}

#[test]
fn a_missing_scenario_file_is_a_typed_failure() {
    assert!(matches!(
        load_scenario_world(&assets_root(), Path::new("scenarios/absent.ron")),
        Err(ScenarioError::Io(_))
    ));
}
