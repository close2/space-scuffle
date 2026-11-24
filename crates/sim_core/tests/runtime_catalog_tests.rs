//! External rule loading, injected construction and mutation-safe catalog indexing.
//!
//! Tests read real authored assets, modify complete temporary bundles, and reload
//! them in the same test process. This proves file edits affect gameplay without
//! recompilation while typed failures cannot silently select embedded defaults.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs, UnitCatalog};
use sim_core::scenario::{build_world_with_catalogs, PlayerSetup, StartLoadoutCatalog};
use sim_core::{SimCoreError, SimWorld, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell, MapFile};
use sim_types::{PlayerId, ShapeKind, SimScalar, SimVec2, UnitTypeId};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;
/// The rule files a bundle holds; `factions.ron` is last so the indices of the others are stable.
const FILES: [&str; 8] = [
    "units.ron",
    "structures.ron",
    "upgrades.ron",
    "movement_rules.ron",
    "statuses.ron",
    "ability_effects.ron",
    "placement_rules.ron",
    "factions.ron",
];

fn catalog_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog")
}

struct Bundle {
    directory: PathBuf,
}

impl Bundle {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        let directory = std::env::temp_dir().join(format!(
            "space-scuffle-catalog-{}-{label}",
            std::process::id()
        ));
        std::fs::create_dir(&directory)?;
        let result = Self { directory };
        result.restore()?;
        Ok(result)
    }

    fn restore(&self) -> Result<(), std::io::Error> {
        for file in FILES {
            std::fs::copy(catalog_directory().join(file), self.directory.join(file))?;
        }
        Ok(())
    }

    fn load(&self) -> Result<RuntimeCatalogs, CatalogLoadError> {
        RuntimeCatalogs::load_from_directory(&self.directory)
    }

    fn finish(self) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(self.directory)
    }
}

/// Replaces a single complete field value in the actual authored source while
/// respecting quoted commas and nested RON collections. Tests check the parsed result.
fn replace_first_field(source: &str, field: &str, replacement: &str) -> String {
    let marker = format!("{field}:");
    let start = source.find(&marker).expect("fixture has requested field") + marker.len();
    let mut depth = 0_u32;
    let mut quoted = false;
    let mut escaped = false;
    let end = source[start..]
        .char_indices()
        .find_map(|(offset, character)| {
            if quoted {
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quoted = false;
                }
            } else {
                match character {
                    '"' => quoted = true,
                    '(' | '[' | '{' => depth += 1,
                    // A field value ends at its comma, or at the close of the struct holding
                    // it when it is the last field and written without a trailing comma.
                    ',' | ')' | ']' | '}' if depth == 0 => return Some(start + offset),
                    ')' | ']' | '}' => depth -= 1,
                    _ => {}
                }
            }
            None
        })
        .expect("fixture field has a terminating comma or closing bracket");
    format!("{} {replacement}{}", &source[..start], &source[end..])
}

fn duplicate_definitions(source: &str, field: &str, modify: impl FnOnce(&str) -> String) -> String {
    let marker = source
        .find(&format!("{field}:"))
        .expect("root collection exists");
    let start = marker + source[marker..].find('[').expect("root sequence exists") + 1;
    let end = source.rfind(']').expect("root sequence closes last");
    let definitions = &source[start..end];
    format!(
        "{}{}{}{}",
        &source[..start],
        definitions,
        modify(definitions),
        &source[end..]
    )
}

fn empty_map() -> Result<GameMap, sim_map::MapError> {
    GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))
}

#[test]
fn same_binary_reload_uses_edited_external_vitals_and_upgrades() -> TestResult {
    let bundle = Bundle::new("reload")?;
    let original = bundle.load()?;
    let unit_text = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    std::fs::write(
        bundle.directory.join(FILES[0]),
        replace_first_field(&unit_text, "max_hp", "4321.0"),
    )?;
    let upgrade_text = std::fs::read_to_string(bundle.directory.join(FILES[2]))?;
    std::fs::write(
        bundle.directory.join(FILES[2]),
        replace_first_field(&upgrade_text, "research_ticks", "4321"),
    )?;
    let modified = bundle.load()?;
    let changed = original
        .units
        .archetypes()
        .find(|archetype| {
            modified
                .units
                .get(archetype.unit_type_id)
                .is_ok_and(|loaded| loaded.max_hp != archetype.max_hp)
        })
        .expect("edited source must change one loaded archetype");
    let unit_type = changed.unit_type_id;
    let old_hp = changed.max_hp;
    let upgrade = modified
        .upgrades
        .upgrades()
        .find(|definition| {
            original
                .upgrades
                .get_by_id(definition.upgrade_id)
                .is_some_and(|old| old.research_ticks != definition.research_ticks)
        })
        .expect("edited source must change one loaded upgrade");
    let upgrade_id = upgrade.upgrade_id;
    assert_eq!(upgrade.research_ticks, 4321);
    let mut old_world =
        SimWorld::with_catalogs(empty_map()?, 7, original.units, original.upgrades)?;
    let mut new_world =
        SimWorld::with_catalogs(empty_map()?, 7, modified.units, modified.upgrades)?;
    let old_entity = old_world.spawn_unit(unit_type, PlayerId(0), SimVec2::ZERO)?;
    let new_entity = new_world.spawn_unit(unit_type, PlayerId(0), SimVec2::ZERO)?;
    assert_eq!(old_world.entities[&old_entity].hp, old_hp);
    assert_eq!(
        new_world.entities[&new_entity].hp,
        SimScalar::from_i32(4321)
    );
    assert_eq!(
        new_world
            .upgrade_catalog
            .get_by_id(upgrade_id)
            .expect("injected upgrade exists")
            .research_ticks,
        4321
    );
    assert_ne!(old_world.compute_checksum(), new_world.compute_checksum());
    bundle.finish()?;
    Ok(())
}

#[test]
fn injected_scenario_resolves_loadout_against_supplied_catalog() -> TestResult {
    let mut catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    let loadouts = StartLoadoutCatalog::load(catalog_directory().join("start_loadouts.ron"))?;
    let loadout = loadouts
        .loadouts
        .first()
        .expect("real loadouts catalog is nonempty");
    let entry = loadout
        .units
        .iter()
        .find(|entry| entry.count > 0)
        .expect("loadout has mobile units");
    let unit_type = catalogs
        .units
        .id_by_slug(&entry.slug)
        .expect("authored loadout resolves");
    catalogs.units.get_mut(unit_type)?.max_hp = SimScalar::from_i32(4321);
    let doc = MapFile::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/default_arena.scmap.ron"),
    )?
    .decode()?;
    let players: Vec<_> = doc
        .start_locations
        .iter()
        .map(|start| PlayerSetup {
            player_slot: start.player_slot,
            loadout_slug: loadout.slug.clone(),
        })
        .collect();
    let a = build_world_with_catalogs(
        &doc,
        &loadouts,
        &players,
        77,
        catalogs.units.clone(),
        catalogs.upgrades.clone(),
    )?;
    let b = build_world_with_catalogs(
        &doc,
        &loadouts,
        &players,
        77,
        catalogs.units,
        catalogs.upgrades,
    )?;
    let spawned: Vec<_> = a
        .entities
        .values()
        .filter(|entity| entity.unit_type_id == unit_type)
        .collect();
    assert!(
        !spawned.is_empty(),
        "actual scenario must spawn the selected loadout unit"
    );
    assert!(spawned
        .iter()
        .all(|entity| entity.hp == SimScalar::from_i32(4321)));
    assert_eq!(a.entities, b.entities);
    assert_eq!(a.compute_checksum(), b.compute_checksum());
    assert_eq!(a.last_checksum, a.compute_checksum());
    Ok(())
}

#[test]
fn missing_and_malformed_selected_files_fail_with_exact_path() -> TestResult {
    let bundle = Bundle::new("failures")?;
    for file in FILES {
        let path = bundle.directory.join(file);
        std::fs::remove_file(&path)?;
        assert!(
            matches!(bundle.load(), Err(CatalogLoadError::Io { path: failed, source }) if failed == path && source.kind() == std::io::ErrorKind::NotFound)
        );
        bundle.restore()?;
        std::fs::write(&path, "(malformed:")?;
        assert!(
            matches!(bundle.load(), Err(CatalogLoadError::Invalid { path: failed, source: SimCoreError::RonDeserializationError(_) }) if failed == path)
        );
        bundle.restore()?;
    }
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-19-footprint-flags` WP1 (charter §3.3, C6): `placement_rules.ron` is read at run
/// time, so an edited ring changes the loaded reach without recompiling.
#[test]
fn an_edited_placement_rule_changes_the_loaded_reach() -> TestResult {
    let bundle = Bundle::new("placement-rules")?;
    let shipped = bundle.load()?;
    let reach = |catalogs: &RuntimeCatalogs| {
        catalogs
            .units
            .placement_rules()
            .reach(sim_types::ResourceKind::Mineral)
            .map(|reach| reach.cells.len())
    };
    assert_eq!(reach(&shipped), Some(52));
    let path = bundle.directory.join(FILES[6]);
    let text = std::fs::read_to_string(&path)?;
    let start = text.find("(kind: Mineral,").ok_or("the mineral ring")?;
    let removal = start
        + text[start..]
            .find("remove_when_empty: ")
            .ok_or("its removal flag")?;
    let end = removal + text[removal..].find("),").ok_or("its end")? + "),".len();
    let edited = format!(
        "{}(kind: Mineral, applies: [NearResources], anchor: (0, 0), rows: [\"x\"], \
         shape: Box(half_width: 0.5, half_height: 0.5, chamfer: 0.0), \
         remove_when_empty: true, \
         harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 0, handoff_ticks: 0),{}",
        &text[..start],
        &text[end..]
    );
    std::fs::write(&path, edited)?;
    assert_eq!(reach(&bundle.load()?), Some(1));
    std::fs::write(&path, text.replace("anchor: (3, 3)", "anchor: (0, 0)"))?;
    assert!(matches!(
        bundle.load(),
        Err(CatalogLoadError::Invalid { path: failed, source: SimCoreError::CatalogInvalidValue { .. } }) if failed == path
    ));
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-20-resource-footprints` WP2 (charter §3.3, C6): the resource footprints are read
/// at run time, so an edited shape or removal flag changes the loaded table without recompiling.
#[test]
fn an_edited_resource_footprint_changes_the_loaded_table() -> TestResult {
    let bundle = Bundle::new("resource-footprints")?;
    let footprint = |catalogs: &RuntimeCatalogs| {
        catalogs
            .units
            .placement_rules()
            .resource_footprints()
            .get(sim_types::ResourceKind::Mineral)
            .copied()
    };
    let shipped = footprint(&bundle.load()?).ok_or("the shipped mineral footprint")?;
    assert_eq!(shipped.cells, (0, 0, 2, 1));
    assert!(shipped.remove_when_empty);
    let path = bundle.directory.join(FILES[6]);
    let text = std::fs::read_to_string(&path)?;
    let narrower = text.replace(
        "shape: Box(half_width: 1.0, half_height: 0.5, chamfer: 0.0), remove_when_empty: true",
        "shape: Box(half_width: 0.75, half_height: 0.5, chamfer: 0.25), remove_when_empty: false",
    );
    assert_ne!(narrower, text, "the shipped mineral shape is the 2 x 1 box");
    std::fs::write(&path, narrower)?;
    let edited = footprint(&bundle.load()?).ok_or("the edited mineral footprint")?;
    assert_eq!(
        edited.shape,
        sim_types::FootprintShape::Box {
            half_width_q: 3,
            half_height_q: 2,
            chamfer_q: 1
        }
    );
    assert!(!edited.remove_when_empty);
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-22-harvest-timing` WP1 (charter §3.3, C6): a resource's harvest time and amount
/// are read from `placement_rules.ron` at run time, so an edited mineral row changes how long a
/// worker gathers and what it carries without recompiling.
#[test]
fn an_edited_resource_harvest_changes_the_gather() -> TestResult {
    let bundle = Bundle::new("resource-harvest")?;
    let shipped = bundle.load()?;
    let shipped_harvest = *shipped
        .units
        .placement_rules()
        .harvest(sim_types::ResourceKind::Mineral)
        .ok_or("the shipped mineral harvest")?;
    let path = bundle.directory.join(FILES[6]);
    let text = std::fs::read_to_string(&path)?;
    let row = text.find("(kind: Mineral,").ok_or("the mineral row")?;
    let (time, amount) = (
        shipped_harvest.time_ticks + 13,
        shipped_harvest.amount.saturating_sub(2).max(1),
    );
    let tail = replace_first_field(&text[row..], "harvest_time_ticks", &time.to_string());
    let tail = replace_first_field(&tail, "harvest_amount", &amount.to_string());
    std::fs::write(&path, format!("{}{tail}", &text[..row]))?;
    let edited = bundle.load()?;
    bundle.finish()?;
    let harvest = *edited
        .units
        .placement_rules()
        .harvest(sim_types::ResourceKind::Mineral)
        .ok_or("the edited mineral harvest")?;
    assert_eq!((harvest.time_ticks, harvest.amount), (time, amount));
    assert_ne!(harvest, shipped_harvest);

    let side = 32_u32;
    let cells = vec![MapCell::default_ground(CliffLevel(0)); (side * side) as usize];
    let mineral = sim_map::ResourceNode {
        pos: SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(8.5)),
        kind: sim_types::ResourceKind::Mineral,
        remaining: 1_000,
        cell: (16, 8),
        placed: None,
    };
    let map = GameMap::from_cells(side, side, cells, vec![mineral], Vec::new())?;
    let mut world = SimWorld::with_catalogs(map, 7, edited.units, edited.upgrades)?;
    let worker_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.harvester.is_some())
        .ok_or("a harvester")?
        .unit_type_id;
    let hall_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.is_resource_dropoff && arch.requires_structures.is_empty())
        .ok_or("a depot")?
        .unit_type_id;
    world.spawn_structure(hall_type, PlayerId(0), 4, 6)?;
    let worker = world.spawn_unit(
        worker_type,
        PlayerId(0),
        SimVec2::new(SimScalar::from_num(12.5), SimScalar::from_num(8.5)),
    )?;
    let step = |world: &mut SimWorld, commands: Vec<sim_types::PlayerCommand>| {
        let tick = world.current_tick;
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands,
            state_checksum: None,
        });
    };
    step(
        &mut world,
        vec![sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![worker],
            order: sim_types::UnitOrder::Harvest {
                target: sim_types::ResourceNodeId(0),
            },
            queue: false,
        }],
    );
    let mut started = None;
    for _ in 0..600 {
        step(&mut world, Vec::new());
        let entity = &world.entities[&worker];
        if started.is_none() && entity.task.gather_ticks > 0 {
            started = Some(world.current_tick);
        }
        if let Some(cargo) = entity.cargo {
            let started = started.ok_or("the gather started before the cargo")?;
            assert_eq!(world.current_tick - started, u32::from(time));
            assert_eq!(cargo.amount, amount);
            assert_eq!(world.map.resources()[0].remaining, 1_000 - amount);
            return Ok(());
        }
    }
    Err("the worker never gathered".into())
}

/// Plan `2026-11-22-harvest-timing` D-3: the worker's harvester capability names no harvest
/// numbers (they are the resource's), so a units catalog still naming the old per-worker fields
/// is refused, not silently ignored.
#[test]
fn a_harvester_naming_a_harvest_field_is_refused() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    assert!(units.contains("harvester: Some((acquire_radius:"));
    for field in ["harvest_capacity: 5", "harvest_rate_ticks: 50"] {
        let edited = units.replacen(
            "harvester: Some((acquire_radius:",
            &format!("harvester: Some(({field}, acquire_radius:"),
            1,
        );
        let result = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &edited);
        let name = field.split(':').next().ok_or("a field name")?;
        assert!(
            matches!(&result, Err(SimCoreError::RonDeserializationError(message)) if message.contains(name)),
            "{field}: {result:?}"
        );
    }
    Ok(())
}

/// Plan `2026-11-23-harvest-occupancy` D-4: both acquire fields are required, so a harvester row
/// without one fails to load with the deserialization error naming it.
#[test]
fn a_harvester_without_its_acquire_fields_is_refused() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let row = units
        .find("harvester: Some((acquire_radius:")
        .ok_or("a harvester row")?;
    let end = row + units[row..].find("))").ok_or("the row's end")? + 2;
    for (missing, replacement) in [
        ("acquire_radius", "harvester: Some((acquires: [Mineral]))"),
        ("acquires", "harvester: Some((acquire_radius: 10.0))"),
    ] {
        let edited = format!("{}{replacement}{}", &units[..row], &units[end..]);
        let result = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &edited);
        assert!(
            matches!(&result, Err(SimCoreError::RonDeserializationError(message)) if message.contains(missing)),
            "{missing}: {result:?}"
        );
    }
    Ok(())
}

/// Sets every harvester row's `acquire_radius` in `units` to `radius`.
fn with_acquire_radius(units: &str, radius: &str) -> String {
    let marker = "harvester: Some((acquire_radius: ";
    let mut out = String::with_capacity(units.len());
    let mut rest = units;
    while let Some(at) = rest.find(marker) {
        let value = at + marker.len();
        let end = value
            + rest[value..]
                .find(',')
                .expect("the radius ends with a comma");
        out.push_str(&rest[..value]);
        out.push_str(radius);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// What a worker does when its only field runs dry: the first step's event, if it ended its
/// order, or the node it moved on to. One field of one load beside the hall and a second field
/// about 6 world units from where the worker stands when it finds the first empty.
fn exhaustion_outcome(
    catalogs: RuntimeCatalogs,
) -> Result<
    Result<sim_types::ResourceNodeId, sim_core::economy::TaskEventKind>,
    Box<dyn std::error::Error>,
> {
    let amount = catalogs
        .units
        .placement_rules()
        .harvest(sim_types::ResourceKind::Mineral)
        .ok_or("the mineral harvest")?
        .amount;
    let field = |cell: (u32, u32), remaining: u32| sim_map::ResourceNode {
        pos: SimVec2::new(
            SimScalar::from_num(cell.0) + SimScalar::HALF,
            SimScalar::from_num(cell.1) + SimScalar::HALF,
        ),
        kind: sim_types::ResourceKind::Mineral,
        remaining,
        cell,
        placed: None,
    };
    let side = 32_u32;
    let cells = vec![MapCell::default_ground(CliffLevel(0)); (side * side) as usize];
    let map = GameMap::from_cells(
        side,
        side,
        cells,
        vec![field((14, 8), amount), field((14, 13), 1_000)],
        Vec::new(),
    )?;
    let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
    let worker_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.harvester.is_some())
        .ok_or("a harvester")?
        .unit_type_id;
    let hall_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.is_resource_dropoff && arch.requires_structures.is_empty())
        .ok_or("a depot")?
        .unit_type_id;
    world.spawn_structure(hall_type, PlayerId(0), 4, 6)?;
    let worker = world.spawn_unit(
        worker_type,
        PlayerId(0),
        SimVec2::new(SimScalar::from_num(11.5), SimScalar::from_num(8.5)),
    )?;
    let mut commands = vec![sim_types::PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![worker],
        order: sim_types::UnitOrder::Harvest {
            target: sim_types::ResourceNodeId(0),
        },
        queue: false,
    }];
    for _ in 0..1_000 {
        let tick = world.current_tick;
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        if let Some(event) = world.task_events().first() {
            return Ok(Err(event.kind.clone()));
        }
        if let Some(sim_types::UnitOrder::Harvest { target }) =
            world.entities[&worker].order_queue.front()
        {
            if target.0 != 0 {
                return Ok(Ok(*target));
            }
        }
    }
    Err("the worker neither moved on nor ended its order".into())
}

/// The take ticks of the showcase's near geyser (10, 1) in a 3,000-tick window after 750 ticks of
/// warm-up, with three workers of slot 0 on it and `catalogs` loaded.
fn near_geyser_takes(catalogs: RuntimeCatalogs) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/maps/showcase_canyon.scmap.ron");
    let doc = MapFile::from_ron_str(&std::fs::read_to_string(path)?)?.decode()?;
    let start = doc
        .start_locations
        .iter()
        .find(|s| s.player_slot == 0)
        .ok_or("slot 0")?
        .cell;
    let mut world =
        SimWorld::with_catalogs(doc.to_game_map()?, 7, catalogs.units, catalogs.upgrades)?;
    let find = |test: &dyn Fn(&sim_core::UnitArchetype) -> bool| {
        world
            .catalog
            .archetypes()
            .find(|arch| test(arch))
            .map(|arch| arch.unit_type_id)
    };
    let worker_type = find(&|arch| arch.capabilities.harvester.is_some()).ok_or("a harvester")?;
    let hall_type = find(&|arch| arch.is_resource_dropoff && arch.requires_structures.is_empty())
        .ok_or("a depot")?;
    let extractor_type = find(&|arch| arch.capabilities.is_gas_extractor).ok_or("an extractor")?;
    world.spawn_structure(hall_type, PlayerId(0), start.0, start.1)?;
    world.spawn_structure(extractor_type, PlayerId(0), 9, 0)?;
    let geyser = (0_u32..)
        .zip(world.map.resources())
        .find(|(_, n)| n.kind == sim_types::ResourceKind::Gas && n.cell == (10, 1))
        .map(|(i, _)| sim_types::ResourceNodeId(i))
        .ok_or("the near geyser")?;
    let at = SimVec2::new(
        SimScalar::from_num(start.0 + 6),
        SimScalar::from_num(start.1 + 6),
    );
    let crew = (0..3)
        .map(|_| world.spawn_unit(worker_type, PlayerId(0), at))
        .collect::<Result<Vec<_>, _>>()?;
    let mut commands = vec![sim_types::PlayerCommand {
        player_id: PlayerId(0),
        entities: crew,
        order: sim_types::UnitOrder::Harvest { target: geyser },
        queue: false,
    }];
    let mut last = world.map.resources()[geyser.0 as usize].remaining;
    let mut takes = Vec::new();
    for tick in 0..3_750 {
        let now = world.current_tick;
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(now),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        let remaining = world.map.resources()[geyser.0 as usize].remaining;
        if remaining < last && tick >= 750 {
            takes.push(world.current_tick);
        }
        last = remaining;
    }
    Ok(takes)
}

/// Plan `2026-11-23-harvest-handoff` WP2 (charter §3.3, C6): each resource's hand-off is read from
/// `placement_rules.ron` at run time. A copy of the shipped catalog with `handoff_ticks: 0` on
/// both rows loads in the same binary, and three workers on the showcase's near geyser are then
/// taken every gas `time_ticks`, `amount × 1,500 / time` a minute, where the shipped catalog
/// takes it every `time_ticks + handoff_ticks`.
#[test]
fn an_edited_handoff_changes_the_saturated_period() -> TestResult {
    let bundle = Bundle::new("handoff")?;
    let shipped = bundle.load()?;
    let gas = *shipped
        .units
        .placement_rules()
        .harvest(sim_types::ResourceKind::Gas)
        .ok_or("the shipped gas harvest")?;
    assert!(gas.handoff_ticks > 0, "the shipped gas row hands off");
    let path = bundle.directory.join(FILES[6]);
    let text = std::fs::read_to_string(&path)?;
    let mut edited = String::new();
    let mut rest = text.as_str();
    let mut rows = 0;
    while let Some(row) = rest.find("(kind: ") {
        let next = rest[row + 1..]
            .find("(kind: ")
            .map_or(rest.len(), |at| row + 1 + at);
        edited.push_str(&rest[..row]);
        edited.push_str(&replace_first_field(&rest[row..next], "handoff_ticks", "0"));
        rest = &rest[next..];
        rows += 1;
    }
    edited.push_str(rest);
    assert_eq!(rows, 2, "one row per resource kind");
    std::fs::write(&path, edited)?;
    let zero = bundle.load()?;
    bundle.finish()?;
    for kind in [
        sim_types::ResourceKind::Mineral,
        sim_types::ResourceKind::Gas,
    ] {
        let harvest = zero
            .units
            .placement_rules()
            .harvest(kind)
            .ok_or("the edited harvest")?;
        assert_eq!(harvest.handoff_ticks, 0, "{kind:?}");
    }
    let periods = |takes: &[u32]| {
        takes
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect::<std::collections::BTreeSet<_>>()
    };
    let with = near_geyser_takes(shipped)?;
    let without = near_geyser_takes(zero)?;
    assert!(with.len() > 50 && without.len() > 50);
    assert_eq!(
        periods(&with),
        [u32::from(gas.time_ticks) + u32::from(gas.handoff_ticks)].into()
    );
    assert_eq!(periods(&without), [u32::from(gas.time_ticks)].into());
    Ok(())
}

/// Plan `2026-11-23-harvest-occupancy` D-4 (charter §3.3, C6): the acquire radius is read from
/// `units.ron` at run time. With the shipped radius an exhausted field's worker moves on to the
/// second field; the same binary with a radius below the distance to it ends the order instead.
#[test]
fn an_edited_acquire_radius_changes_the_next_field() -> TestResult {
    let bundle = Bundle::new("acquire-radius")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    std::fs::write(&path, with_acquire_radius(&text, "2.0"))?;
    let edited = bundle.load()?;
    bundle.finish()?;
    let radius = |catalogs: &RuntimeCatalogs| {
        catalogs
            .units
            .archetypes()
            .find_map(|arch| arch.capabilities.harvester)
            .map(|spec| spec.acquire_radius)
    };
    assert_eq!(radius(&edited), Some(SimScalar::from_num(2)));
    assert_ne!(radius(&shipped), radius(&edited));
    assert_eq!(
        exhaustion_outcome(shipped)?,
        Ok(sim_types::ResourceNodeId(1))
    );
    assert_eq!(
        exhaustion_outcome(edited)?,
        Err(sim_core::economy::TaskEventKind::ResourceExhausted)
    );
    Ok(())
}

#[test]
fn authored_identity_conflicts_and_narrowing_errors_are_typed() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let structures = std::fs::read_to_string(catalog_directory().join(FILES[1]))?;
    let base = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &units)?;
    let first = base.archetypes().next().expect("unit catalog nonempty");
    let duplicated = duplicate_definitions(&units, "units", str::to_owned);
    assert!(matches!(
        UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &duplicated),
        Err(SimCoreError::CatalogDuplicateId { .. })
    ));
    let duplicate_slug = duplicate_definitions(&units, "units", |definitions| {
        let changed_id = replace_first_field(definitions, "unit_type_id", &u16::MAX.to_string());
        replace_first_field(
            &changed_id,
            "slug",
            &format!("{:?}", first.slug.to_ascii_uppercase()),
        )
    });
    assert!(matches!(
        UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &duplicate_slug),
        Err(SimCoreError::CatalogDuplicateSlug { .. })
    ));
    let cross_id = replace_first_field(
        &structures,
        "structure_type_id",
        &first.unit_type_id.0.to_string(),
    );
    assert!(matches!(
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &units, &cross_id),
        Err(SimCoreError::CatalogDuplicateId { .. })
    ));
    let cross_slug = replace_first_field(
        &structures,
        "slug",
        &format!("{:?}", first.slug.to_ascii_uppercase()),
    );
    assert!(matches!(
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &units, &cross_slug),
        Err(SimCoreError::CatalogDuplicateSlug { .. })
    ));
    for (field, value) in [
        ("structure_type_id", "65536"),
        ("footprint", "(0, 5)"),
        ("footprint", "(5, 256)"),
        ("slug", "\"  \""),
    ] {
        let invalid = replace_first_field(&structures, field, value);
        assert!(
            matches!(UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &units, &invalid), Err(SimCoreError::CatalogInvalidValue { field: reported, .. }) if reported == field)
        );
    }
    let invalid_slug = replace_first_field(&units, "slug", "\"\"");
    assert!(matches!(
        UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &invalid_slug),
        Err(SimCoreError::CatalogInvalidValue { .. })
    ));
    Ok(())
}

#[test]
fn invalid_numbers_fail_before_fixed_point_conversion_with_file_context() -> TestResult {
    let bundle = Bundle::new("numbers")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    for (field, value) in [
        ("max_hp", "NaN"),
        ("max_speed", "inf"),
        ("max_hp", "2147483648.0"),
        ("max_hp", "-2147483904.0"),
        ("base_damage", "NaN"),
        ("fraction", "inf"),
        ("hp_cost", "NaN"),
        ("turn_rate_deg_per_tick", "NaN"),
        ("firing_arc_deg", "inf"),
        ("projectile_speed", "Some(inf)"),
    ] {
        std::fs::write(
            bundle.directory.join(FILES[0]),
            replace_first_field(&units, field, value),
        )?;
        match bundle.load() {
            Err(CatalogLoadError::Invalid {
                path,
                source:
                    SimCoreError::CatalogInvalidValue {
                        field: reported, ..
                    },
            }) => {
                assert_eq!(path, bundle.directory.join(FILES[0]));
                assert!(
                    reported.ends_with(field),
                    "expected {field}, got {reported}"
                );
            }
            other => {
                return Err(
                    format!("expected typed invalid numeric field {field}, got {other:?}").into(),
                )
            }
        }
    }
    for (field, value) in [
        ("energy_cost_per_second", "NaN"),
        ("sight_radius_override", "Some(inf)"),
    ] {
        // These optional mode fields are absent from the current source. Insert
        // them into a real authored mode while preserving the complete bundle.
        let changed = replace_first_field(
            &units,
            "mode_name",
            &format!("\"fixture-mode\", {field}: {value}"),
        );
        std::fs::write(bundle.directory.join(FILES[0]), changed)?;
        assert!(matches!(bundle.load(), Err(CatalogLoadError::Invalid {
            source: SimCoreError::CatalogInvalidValue { field: reported, .. }, ..
        }) if reported.ends_with(field)));
    }
    bundle.restore()?;
    let structures = std::fs::read_to_string(bundle.directory.join(FILES[1]))?;
    std::fs::write(
        bundle.directory.join(FILES[1]),
        replace_first_field(&structures, "max_hp", "2147483648.0"),
    )?;
    assert!(
        matches!(bundle.load(), Err(CatalogLoadError::Invalid { path, source: SimCoreError::CatalogInvalidValue { .. } }) if path == bundle.directory.join(FILES[1]))
    );
    bundle.finish()?;
    Ok(())
}

#[test]
fn structure_additions_are_atomic_on_a_late_identity_error() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let structures = std::fs::read_to_string(catalog_directory().join(FILES[1]))?;
    let mut catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &units)?;
    let original = catalog.clone();
    let duplicated = duplicate_definitions(&structures, "structures", str::to_owned);
    assert!(matches!(
        catalog.load_structures_from_ron_str(&duplicated),
        Err(SimCoreError::CatalogDuplicateId { .. })
    ));
    assert!(catalog.archetypes().eq(original.archetypes()));
    for archetype in original.archetypes() {
        assert_eq!(catalog.get_by_slug(&archetype.slug), Some(archetype));
    }
    Ok(())
}

#[test]
fn upgrade_duplicates_fail_and_sparse_ids_do_not_require_dense_storage() -> TestResult {
    let source = std::fs::read_to_string(catalog_directory().join(FILES[2]))?;
    let duplicated = duplicate_definitions(&source, "upgrades", str::to_owned);
    assert!(matches!(
        UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &duplicated),
        Err(SimCoreError::CatalogDuplicateId {
            kind: "upgrade",
            ..
        })
    ));
    let duplicated_slug = duplicate_definitions(&source, "upgrades", |definitions| {
        replace_first_field(definitions, "upgrade_id", &u32::MAX.to_string())
    });
    assert!(matches!(
        UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &duplicated_slug),
        Err(SimCoreError::CatalogDuplicateSlug {
            kind: "upgrade",
            ..
        })
    ));
    let sparse = replace_first_field(&source, "upgrade_id", &u32::MAX.to_string());
    let catalog = UpgradeCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &sparse)?;
    assert!(catalog.get_by_id(u32::MAX).is_some());
    assert!(catalog
        .upgrades()
        .map(|upgrade| upgrade.upgrade_id)
        .is_sorted());
    Ok(())
}

#[test]
fn slug_index_tracks_mutations_clones_and_storage_identity() -> TestResult {
    let loaded = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    let template = loaded
        .units
        .archetypes()
        .next()
        .expect("catalog contains an archetype")
        .clone();
    let mut catalog = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), "(units: [])")?;
    for (id, slug) in [
        (3, "fixture-gamma"),
        (2, "fixture-beta"),
        (1, "fixture-alpha"),
    ] {
        let mut definition = template.clone();
        definition.unit_type_id = UnitTypeId(id);
        definition.slug = slug.to_owned();
        catalog.register(definition);
    }
    let cold_clone = catalog.clone();
    assert_eq!(catalog.id_by_slug("FIXTURE-BETA"), Some(UnitTypeId(2)));
    assert_eq!(cold_clone.id_by_slug("fixture-alpha"), Some(UnitTypeId(1)));
    assert!(catalog.get_by_slug("absent-fixture").is_none());
    let warm_clone = catalog.clone();
    assert_eq!(warm_clone.id_by_slug("fixture-gamma"), Some(UnitTypeId(3)));
    catalog.get_mut(UnitTypeId(2))?.slug = "fixture-renamed".to_owned();
    assert!(catalog.get_by_slug("fixture-beta").is_none());
    assert_eq!(catalog.id_by_slug("FIXTURE-RENAMED"), Some(UnitTypeId(2)));
    assert_eq!(warm_clone.id_by_slug("fixture-beta"), Some(UnitTypeId(2)));
    catalog.get_mut(UnitTypeId(2))?.unit_type_id = UnitTypeId(99);
    assert_eq!(
        catalog
            .get_by_slug("fixture-renamed")
            .expect("index uses storage slot")
            .unit_type_id,
        UnitTypeId(99)
    );
    let mut replacement = template.clone();
    replacement.unit_type_id = UnitTypeId(2);
    replacement.slug = "fixture-replacement".to_owned();
    catalog.register(replacement);
    assert!(catalog.get_by_slug("fixture-renamed").is_none());
    assert_eq!(
        catalog.id_by_slug("fixture-replacement"),
        Some(UnitTypeId(2))
    );
    catalog.get_mut(UnitTypeId(3))?.slug = "FIXTURE-ALPHA".to_owned();
    assert_eq!(
        catalog.id_by_slug("fixture-alpha"),
        Some(UnitTypeId(1)),
        "legacy duplicate slugs resolve by lowest storage slot"
    );
    assert_eq!(
        catalog
            .archetypes()
            .map(|a| a.unit_type_id)
            .collect::<Vec<_>>(),
        [UnitTypeId(1), UnitTypeId(2), UnitTypeId(3)]
    );
    Ok(())
}

/// The shipped structures with every authored `scan_range` line removed, then `scan_range`
/// written after the `sight_radius` of each definition for which `scan_of(armed)` gives a value
/// (`armed`: the definition has a ground or an air weapon).
fn structures_with_scan(
    scan_of: impl Fn(bool) -> Option<&'static str>,
) -> Result<String, std::io::Error> {
    const OPENING: &str = "\n        (\n            structure_type_id";
    let shipped = std::fs::read_to_string(catalog_directory().join(FILES[1]))?;
    let mut stripped = String::new();
    for line in shipped
        .lines()
        .filter(|line| !line.trim_start().starts_with("scan_range:"))
    {
        stripped.push_str(line);
        stripped.push('\n');
    }
    let mut definitions = stripped.split(OPENING);
    let mut source = definitions.next().unwrap_or_default().to_owned();
    for definition in definitions {
        source.push_str(OPENING);
        let armed = definition.contains("_weapon: Some(");
        match scan_of(armed) {
            Some(value) => {
                let sight = definition
                    .find("sight_radius:")
                    .expect("every structure has a sight radius");
                let line_end = sight + definition[sight..].find('\n').expect("the line ends") + 1;
                source.push_str(&definition[..line_end]);
                source.push_str("            scan_range: ");
                source.push_str(value);
                source.push_str(",\n");
                source.push_str(&definition[line_end..]);
            }
            None => source.push_str(definition),
        }
    }
    Ok(source)
}

/// B-12 (combat alignment WP14 Amendment 7): an armed structure scans to its authored
/// `scan_range` (the reference of audit §11.3), not to its sight radius.
#[test]
fn an_armed_structure_scans_to_its_authored_range() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let structures = structures_with_scan(|armed| armed.then_some("Some(4.25)"))?;
    let catalog =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &units, &structures)?;
    let armed: Vec<_> = catalog
        .archetypes()
        .filter(|archetype| {
            matches!(archetype.shape, ShapeKind::BuildingDome { .. })
                && (archetype.ground_weapon.is_some() || archetype.air_weapon.is_some())
        })
        .collect();
    assert!(
        !armed.is_empty(),
        "the shipped catalog has armed structures"
    );
    for archetype in armed {
        assert_eq!(
            archetype.scan_range,
            SimScalar::from_num(4.25),
            "{} scans to its authored range",
            archetype.slug
        );
    }
    Ok(())
}

/// B-12: `scan_range` is required on a structure with a weapon and refused on one without.
#[test]
fn a_structure_scan_range_is_required_exactly_when_it_is_armed() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let armed_without = structures_with_scan(|_| None)?;
    let unarmed_with =
        structures_with_scan(|armed| Some(if armed { "Some(7.0)" } else { "Some(5.0)" }))?;
    for structures in [armed_without, unarmed_with] {
        let result = UnitCatalog::from_ron_str_bundle(
            &sim_fixtures::shipped_factions(),
            &units,
            &structures,
        );
        assert!(
            matches!(&result, Err(SimCoreError::CatalogInvalidValue { field, .. }) if field == "scan_range"),
            "{:?}",
            result.as_ref().err()
        );
    }
    Ok(())
}

/// Plan `2026-11-10-spellcaster-mechanics` D-3: the spell tables are read at run time. Raising
/// the first damage node of `ability_effects.ron` (the storm's initial damage) in a copied bundle
/// changes what a storm deals, with no recompilation.
#[test]
fn an_edited_effect_value_changes_a_storm_without_recompiling() -> TestResult {
    let bundle = Bundle::new("spell")?;
    let effects = std::fs::read_to_string(bundle.directory.join(FILES[5]))?;
    std::fs::write(
        bundle.directory.join(FILES[5]),
        replace_first_field(&effects, "amount", "25.0"),
    )?;
    let mut losses = Vec::new();
    for catalogs in [
        RuntimeCatalogs::load_from_directory(catalog_directory())?,
        bundle.load()?,
    ] {
        let mut world =
            SimWorld::with_catalogs(empty_map()?, 7, catalogs.units, catalogs.upgrades)?;
        world.research_upgrade(PlayerId(0), "psionic_storm")?;
        let (caster_kind, index) = world
            .catalog
            .archetypes()
            .find_map(|archetype| {
                let index = archetype.abilities.iter().position(|ability| {
                    matches!(&ability.effect, sim_types::AbilityEffectSpec::Effect(key) if key == "psionic_storm")
                })?;
                Some((archetype.unit_type_id, u8::try_from(index).ok()?))
            })
            .ok_or("a shipped storm caster")?;
        let victim_kind = world
            .catalog
            .archetypes()
            .find(|archetype| {
                archetype.max_hp > SimScalar::from_i32(30)
                    && archetype.max_shields == SimScalar::ZERO
                    && archetype.max_speed > SimScalar::ZERO
            })
            .ok_or("a unit without shields")?
            .unit_type_id;
        let caster = world.spawn_unit(caster_kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        world.entities.get_mut(&caster).ok_or("caster")?.energy = SimScalar::from_i32(200);
        let victim = world.spawn_unit(victim_kind, PlayerId(1), SimVec2::from_i32(15, 10))?;
        let before = world.entities[&victim].hp;
        assert!(world.execute_ability(caster, index, Some(SimVec2::from_i32(15, 10)), None));
        losses.push(before - world.entities[&victim].hp);
    }
    assert_eq!(
        losses,
        [SimScalar::from_i32(10), SimScalar::from_i32(25)],
        "the shipped initial damage, then the edited one"
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-10-spellcaster-mechanics` D-3, WP3: shortening Abduct's flight in a copied
/// `ability_effects.ron` shortens the pull, with no recompilation.
#[test]
fn an_edited_flight_changes_an_abduct_without_recompiling() -> TestResult {
    let bundle = Bundle::new("abduct")?;
    let effects = std::fs::read_to_string(bundle.directory.join(FILES[5]))?;
    std::fs::write(
        bundle.directory.join(FILES[5]),
        replace_first_field(&effects, "flight_ticks", "6"),
    )?;
    let mut flights = Vec::new();
    for catalogs in [
        RuntimeCatalogs::load_from_directory(catalog_directory())?,
        bundle.load()?,
    ] {
        let mut world =
            SimWorld::with_catalogs(empty_map()?, 7, catalogs.units, catalogs.upgrades)?;
        let (caster_kind, index) = world
            .catalog
            .archetypes()
            .find_map(|archetype| {
                let index = archetype.abilities.iter().position(|ability| {
                    matches!(&ability.effect, sim_types::AbilityEffectSpec::Effect(key) if key == "abduct")
                })?;
                Some((archetype.unit_type_id, u8::try_from(index).ok()?))
            })
            .ok_or("a shipped Abduct caster")?;
        let victim_kind = world
            .catalog
            .archetypes()
            .find(|archetype| {
                archetype.max_hp > SimScalar::from_i32(30)
                    && archetype.max_speed > SimScalar::ZERO
                    && archetype.weapon.is_none()
                    && !matches!(archetype.shape, ShapeKind::BuildingDome { .. })
            })
            .ok_or("an unarmed mobile unit")?
            .unit_type_id;
        let caster = world.spawn_unit(caster_kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        world.entities.get_mut(&caster).ok_or("caster")?.energy = SimScalar::from_i32(200);
        let victim = world.spawn_unit(victim_kind, PlayerId(0), SimVec2::from_i32(15, 10))?;
        assert!(world.execute_ability(caster, index, None, Some(victim)));
        let mut flying = 0;
        for _ in 0..60 {
            world.step(&sim_types::TickBundle {
                tick: sim_types::Tick(world.current_tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            if world.entities[&victim].displacement.is_some() {
                flying += 1;
            }
        }
        flights.push(flying);
    }
    // The tick a flight starts the unit is still at its start; it lands on the last tick.
    assert_eq!(flights, [18, 6], "the shipped flight, then the edited one");
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-10-spellcaster-mechanics` D-3, WP4: a weaker slow in a copied `statuses.ron`
/// changes what Fungal Growth does to a unit's speed, with no recompilation.
#[test]
fn an_edited_status_changes_a_fungal_slow_without_recompiling() -> TestResult {
    let bundle = Bundle::new("fungal")?;
    let statuses = std::fs::read_to_string(bundle.directory.join(FILES[4]))?;
    assert_eq!(
        statuses.matches("move_speed_factor:").count(),
        2,
        "Fungal Growth's slow, first in the file, and the concussive slow change speed"
    );
    std::fs::write(
        bundle.directory.join(FILES[4]),
        replace_first_field(&statuses, "move_speed_factor", "0.5"),
    )?;
    let mut factors = Vec::new();
    for catalogs in [
        RuntimeCatalogs::load_from_directory(catalog_directory())?,
        bundle.load()?,
    ] {
        let mut world =
            SimWorld::with_catalogs(empty_map()?, 7, catalogs.units, catalogs.upgrades)?;
        let (caster_kind, index) = world
            .catalog
            .archetypes()
            .find_map(|archetype| {
                let index = archetype.abilities.iter().position(|ability| {
                    matches!(&ability.effect, sim_types::AbilityEffectSpec::Effect(key) if key == "fungal_growth")
                })?;
                Some((archetype.unit_type_id, u8::try_from(index).ok()?))
            })
            .ok_or("a shipped Fungal Growth caster")?;
        let victim_kind = world
            .catalog
            .archetypes()
            .find(|archetype| {
                archetype.max_hp > SimScalar::from_i32(30)
                    && archetype.max_speed > SimScalar::ZERO
                    && !matches!(archetype.shape, ShapeKind::BuildingDome { .. })
            })
            .ok_or("a mobile unit")?
            .unit_type_id;
        let caster = world.spawn_unit(caster_kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        world.entities.get_mut(&caster).ok_or("caster")?.energy = SimScalar::from_i32(200);
        let victim = world.spawn_unit(victim_kind, PlayerId(1), SimVec2::from_i32(15, 10))?;
        assert!(world.execute_ability(caster, index, Some(SimVec2::from_i32(15, 10)), None));
        for _ in 0..12 {
            world.step(&sim_types::TickBundle {
                tick: sim_types::Tick(world.current_tick),
                commands: Vec::new(),
                state_checksum: None,
            });
        }
        factors.push(world.entities[&victim].status_modifiers().move_speed_factor);
    }
    assert_eq!(
        factors,
        [SimScalar::from_num(0.25), SimScalar::from_num(0.5)],
        "the shipped slow, then the edited one"
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-10-spellcaster-mechanics` D-3, WP5: a smaller reduction in a copied
/// `statuses.ron` changes what Blinding Cloud leaves of a ranged weapon's range, with no
/// recompilation.
#[test]
fn an_edited_status_changes_a_blinding_cloud_range_without_recompiling() -> TestResult {
    let bundle = Bundle::new("blinding")?;
    let statuses = std::fs::read_to_string(bundle.directory.join(FILES[4]))?;
    assert_eq!(
        statuses.matches("ranged_weapon_range_delta:").count(),
        1,
        "the cloud's unit status is the one that reduces ranged weapons only"
    );
    std::fs::write(
        bundle.directory.join(FILES[4]),
        replace_first_field(&statuses, "ranged_weapon_range_delta", "-2.0"),
    )?;
    let mut ranges = Vec::new();
    for catalogs in [
        RuntimeCatalogs::load_from_directory(catalog_directory())?,
        bundle.load()?,
    ] {
        let mut world =
            SimWorld::with_catalogs(empty_map()?, 7, catalogs.units, catalogs.upgrades)?;
        let (caster_kind, index) = world
            .catalog
            .archetypes()
            .find_map(|archetype| {
                let index = archetype.abilities.iter().position(|ability| {
                    matches!(&ability.effect, sim_types::AbilityEffectSpec::Effect(key) if key == "blinding_cloud")
                })?;
                Some((archetype.unit_type_id, u8::try_from(index).ok()?))
            })
            .ok_or("a shipped Blinding Cloud caster")?;
        // A mobile ground unit whose only weapon is ranged with range 5.
        let shooter = world
            .catalog
            .archetypes()
            .find(|archetype| {
                archetype.mover_type == sim_types::MoverType::Ground
                    && archetype.max_speed > SimScalar::ZERO
                    && archetype.weapon.as_ref().is_some_and(|weapon| {
                        !weapon.melee && weapon.max_range == SimScalar::from_i32(5)
                    })
            })
            .ok_or("a ranged ground unit")?
            .clone();
        let weapon = shooter.weapon.clone().ok_or("armed")?;
        let caster = world.spawn_unit(caster_kind, PlayerId(0), SimVec2::from_i32(10, 10))?;
        world.entities.get_mut(&caster).ok_or("caster")?.energy = SimScalar::from_i32(200);
        let victim =
            world.spawn_unit(shooter.unit_type_id, PlayerId(1), SimVec2::from_i32(15, 10))?;
        assert!(world.execute_ability(caster, index, Some(SimVec2::from_i32(15, 10)), None));
        let entity = world.entities[&victim].clone();
        ranges.push(world.entity_weapon_range(&entity, &weapon, &shooter.slug));
    }
    assert_eq!(
        ranges,
        [
            SimScalar::ONE.saturating_div(SimScalar::from_i32(10)),
            SimScalar::from_i32(3)
        ],
        "the shipped reduction to the floor, then the edited one"
    );
    bundle.finish()?;
    Ok(())
}

/// The showcase's world built from the catalogs of `bundle`, and the rallies of its structures.
fn showcase_rallies(
    catalogs: RuntimeCatalogs,
) -> Result<Vec<Option<sim_types::RallyTarget>>, Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let manifest = sim_core::scenario::ScenarioManifest::from_ron_str(&std::fs::read_to_string(
        assets.join("scenarios/showcase.ron"),
    )?)?;
    let document = MapFile::load(&assets.join(&manifest.map))?.decode()?;
    let loadouts = StartLoadoutCatalog::load(catalog_directory().join("start_loadouts.ron"))?;
    let world = build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        manifest.seed,
        catalogs.units,
        catalogs.upgrades,
    )?;
    Ok(world
        .entities
        .values()
        .filter(|entity| entity.is_immobile)
        .map(|entity| entity.rally)
        .collect())
}

/// Plan `2026-11-23-production-queue` D-8 (charter §3.3, C6): the town hall's auto-rally is read
/// from `structures.ron` at run time. The shipped rows rally the showcase's halls; the same
/// binary with every `auto_rally` removed, or with a radius below every field's reach, gives
/// none.
#[test]
fn an_edited_auto_rally_changes_the_halls_rally() -> TestResult {
    let bundle = Bundle::new("auto-rally")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[1]);
    let text = std::fs::read_to_string(&path)?;
    let without: String = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("auto_rally:"))
        .flat_map(|line| [line, "\n"])
        .collect();
    std::fs::write(&path, &without)?;
    let removed = bundle.load()?;
    std::fs::write(
        &path,
        text.replace("radius: 6.5, kinds", "radius: 0.5, kinds"),
    )?;
    let short = bundle.load()?;
    bundle.finish()?;
    let rallied = showcase_rallies(shipped)?;
    assert_eq!(rallied.len(), 2, "the showcase's two halls");
    assert!(rallied.iter().all(Option::is_some), "{rallied:?}");
    for edited in [removed, short] {
        let rallies = showcase_rallies(edited)?;
        assert_eq!(rallies.len(), rallied.len());
        assert!(rallies.iter().all(Option::is_none), "{rallies:?}");
    }
    Ok(())
}

/// Plan `2026-11-23-regeneration` P-7 (charter §3.3, C6): a structure's energy regeneration is
/// read at run time, so a catalog copy with the first authored rate doubled doubles that
/// structure's gain per tick in the same binary.
#[test]
fn an_edited_structure_energy_rate_changes_the_regeneration() -> TestResult {
    let bundle = Bundle::new("structure-energy")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[1]);
    let text = std::fs::read_to_string(&path)?;
    std::fs::write(
        &path,
        replace_first_field(&text, "energy_regen_per_tick", "0.063"),
    )?;
    let edited = bundle.load()?;
    let structure = shipped
        .units
        .archetypes()
        .find(|archetype| {
            edited
                .units
                .get(archetype.unit_type_id)
                .is_ok_and(|loaded| loaded.energy_regen_per_tick != archetype.energy_regen_per_tick)
        })
        .expect("the edit must change one loaded structure");
    let unit_type = structure.unit_type_id;
    let gain = |catalogs: RuntimeCatalogs| -> Result<SimScalar, Box<dyn std::error::Error>> {
        let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
        let id = world.spawn_structure(unit_type, PlayerId(0), 4, 4)?;
        world.entities.get_mut(&id).ok_or("structure")?.energy = SimScalar::ZERO;
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(0),
            commands: Vec::new(),
            state_checksum: None,
        });
        Ok(world.entities[&id].energy)
    };
    let shipped_gain = gain(shipped)?;
    assert!(shipped_gain > SimScalar::ZERO);
    assert_eq!(gain(edited)?, shipped_gain * SimScalar::from_num(2));
    bundle.finish()?;
    Ok(())
}

/// `structures` with the first definition `pick` accepts authoring `attributes` (a RON list):
/// any authored `attributes` line of that definition is removed and the list written before its
/// `max_hp`. Returns the edited text and that definition's slug, read from the file (C5).
fn with_structure_attributes(
    structures: &str,
    pick: impl Fn(&str) -> bool,
    attributes: &str,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    const OPENING: &str = "\n        (\n            structure_type_id";
    let mut definitions = structures.split(OPENING);
    let mut source = definitions.next().unwrap_or_default().to_owned();
    let mut chosen = None;
    for definition in definitions {
        source.push_str(OPENING);
        if chosen.is_some() || !pick(definition) {
            source.push_str(definition);
            continue;
        }
        let slug = definition
            .split("slug: \"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .ok_or("every definition has a slug")?;
        chosen = Some(slug.to_owned());
        for line in definition.split_inclusive('\n') {
            let trimmed = line.trim_start();
            if trimmed.starts_with("attributes:") {
                continue;
            }
            if trimmed.starts_with("max_hp:") {
                source.push_str("            attributes: ");
                source.push_str(attributes);
                source.push_str(",\n");
            }
            source.push_str(line);
        }
    }
    Ok((source, chosen.ok_or("no definition was picked")?))
}

/// Whether a structure definition authors shields.
fn is_shielded(definition: &str) -> bool {
    definition
        .split("max_shields:")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .is_some_and(|value| value > 0.0)
}

/// Plan `2026-11-23-structure-attributes` P-3 (B-88, charter §3.3, C6): a structure's attributes
/// are read as written, in the same binary. A shielded structure given `[Armored, Mechanical,
/// Structure]` loads with exactly those (shields add no Psionic), and an unshielded one given
/// `[Armored, Light, Structure]` takes a shipped weapon's `Light` bonus (found by property).
#[test]
fn an_edited_structure_attribute_list_is_read_as_written() -> TestResult {
    use sim_types::{Attribute, AttributeFlags};
    let bundle = Bundle::new("structure-attributes")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[1]);
    let text = std::fs::read_to_string(&path)?;
    let (text, shielded) =
        with_structure_attributes(&text, is_shielded, "[Armored, Mechanical, Structure]")?;
    let (text, unshielded) = with_structure_attributes(
        &text,
        |definition| !is_shielded(definition),
        "[Armored, Light, Structure]",
    )?;
    std::fs::write(&path, text)?;
    let edited = bundle.load()?;
    let archetype =
        |catalogs: &RuntimeCatalogs, slug: &str| -> Result<_, Box<dyn std::error::Error>> {
            Ok(catalogs
                .units
                .get(
                    catalogs
                        .units
                        .id_by_slug(slug)
                        .ok_or("the edited structure loads")?,
                )?
                .clone())
        };
    assert_eq!(
        archetype(&edited, &shielded)?.attributes,
        AttributeFlags::ARMORED | AttributeFlags::MECHANICAL | AttributeFlags::STRUCTURE,
        "{shielded}: shields add no attribute"
    );
    let light = archetype(&edited, &unshielded)?;
    assert_eq!(
        light.attributes,
        AttributeFlags::ARMORED | AttributeFlags::LIGHT | AttributeFlags::STRUCTURE
    );
    let before = archetype(&shipped, &unshielded)?;
    assert!(!before.attributes.contains(AttributeFlags::LIGHT));
    let weapon = edited
        .units
        .archetypes()
        .flat_map(|a| [&a.ground_weapon, &a.air_weapon, &a.weapon])
        .flatten()
        .find(|weapon| {
            weapon.structure_strike.is_none()
                && weapon.attribute_bonuses.get(Attribute::Light) > SimScalar::ZERO
        })
        .expect("a shipped weapon has a Light bonus");
    let damage = |target: &sim_core::catalog::UnitArchetype| {
        sim_core::calculate_hit_damage(weapon, target, SimScalar::ZERO, target.max_hp).life_damage
    };
    assert_eq!(
        damage(&light),
        damage(&before) + weapon.attribute_bonuses.get(Attribute::Light),
        "{unshielded} takes {}'s Light bonus",
        weapon.name
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-23-splash-centres` P-7 (B-83): where the Baneling analogue's burst is centred is
/// read from `units.ron` at run time. Its burst, found by property (a suicide weapon with an
/// area), is fired at an enemy beside it, with one enemy behind the Baneling (inside the reach of
/// the Baneling, outside that of the target) and one beyond the target (the other way round). The
/// shipped catalog strikes the one behind and spares the one beyond; the same binary with the
/// row's `impact: Attacker` edited to `impact: Target` does the opposite.
#[test]
fn an_edited_splash_impact_moves_the_burst() -> TestResult {
    let bundle = Bundle::new("splash-impact")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    assert!(
        text.contains("impact: Attacker"),
        "the shipped catalog centres an area on its attacker"
    );
    // The other-level strike belongs to an attacker-centred area only (plan
    // `2026-11-23-splash-cliff-levels` D-3), so the edit drops it with the centre.
    std::fs::write(
        &path,
        text.replacen("impact: Attacker", "impact: Target", 1)
            .replacen("strike_target_on_other_level: true,", "", 1),
    )?;
    let edited = bundle.load()?;
    let struck = |catalogs: RuntimeCatalogs| -> Result<(bool, bool), Box<dyn std::error::Error>> {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
        sim_fixtures::without_random_delays(&mut world.catalog);
        let (burst, reach) = world
            .catalog
            .archetypes()
            .find_map(|arch| {
                arch.weapon
                    .as_ref()
                    .filter(|weapon| weapon.suicide_on_attack)
                    .and_then(|weapon| weapon.splash.as_ref())
                    .map(|splash| (arch.unit_type_id, splash.reach()))
            })
            .ok_or("a suicide weapon with an area")?;
        let infantry = world
            .catalog
            .archetypes()
            .find(|arch| {
                arch.unit_type_id != burst
                    && arch.mover_type == sim_types::MoverType::Ground
                    && arch.max_speed > SimScalar::ZERO
                    && matches!(arch.shape, ShapeKind::UnitSphere)
                    && arch.collision_radius <= SimScalar::HALF
            })
            .map(|arch| (arch.unit_type_id, arch.collision_radius))
            .ok_or("small infantry")?;
        let arch = world.catalog.get_mut(burst)?;
        arch.max_hp = SimScalar::from_num(100_000);
        let own = arch.collision_radius;
        let (kind, radius) = infantry;
        let at = |x: SimScalar| SimVec2::new(x, SimScalar::from_num(16));
        let centre = SimScalar::from_num(16);
        let tenth = SimScalar::from_num(0.1);
        let baneling = world.spawn_unit(burst, PlayerId(0), at(centre))?;
        let target_x = centre + own + radius + tenth;
        let mut spawn = |x: SimScalar| -> Result<sim_types::EntityId, Box<dyn std::error::Error>> {
            let id = world.spawn_unit(kind, PlayerId(1), at(x))?;
            world
                .entities
                .get_mut(&id)
                .ok_or("spawned")?
                .issue_order(sim_types::UnitOrder::HoldPosition, false)?;
            Ok(id)
        };
        let target = spawn(target_x)?;
        // Behind: edge `reach − 0.1` from the Baneling's centre, beyond `reach` from the target's.
        let behind = spawn(centre - (reach + radius - tenth))?;
        // Beyond: edge `reach − 0.1` from the target's centre, beyond `reach` from the Baneling's.
        let beyond = spawn(target_x + reach + radius - tenth)?;
        let life = |world: &SimWorld, id| world.entities.get(&id).map_or(SimScalar::ZERO, |e| e.hp);
        let full = life(&world, behind);
        world
            .entities
            .get_mut(&baneling)
            .ok_or("spawned")?
            .issue_order(sim_types::UnitOrder::attack(target), false)?;
        for tick in 0..80 {
            world.step(&sim_types::TickBundle {
                tick: sim_types::Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            if !world
                .entities
                .get(&baneling)
                .is_some_and(sim_core::Entity::is_alive)
            {
                break;
            }
        }
        Ok((life(&world, behind) < full, life(&world, beyond) < full))
    };
    assert_eq!(struck(shipped)?, (true, false), "centred on the Baneling");
    assert_eq!(struck(edited)?, (false, true), "centred on its target");
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-23-splash-centres` P-7 (B-84): the sieged cannon's target radius condition is
/// read from `units.ron` at run time. The cannon, found by property (a mode weapon with a
/// conditional circular area), fires at the first shipped structure the condition exempts; the
/// shipped catalog publishes no `SplashVisual`, and the same binary with the condition removed
/// from the row publishes one.
#[test]
fn an_edited_target_radius_condition_opens_the_sieged_area() -> TestResult {
    let bundle = Bundle::new("splash-condition")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let condition = "target_radius_below: Some(1.25),";
    assert!(
        text.contains(condition),
        "the shipped catalog has the condition"
    );
    std::fs::write(&path, text.replacen(condition, "", 1))?;
    let edited = bundle.load()?;
    // The sieged cannon of `catalogs` by property, and the first structure its shipped
    // condition exempts.
    let shipped_cannon = shipped
        .units
        .archetypes()
        .flat_map(|arch| &arch.modes)
        .filter_map(|mode| {
            mode.ground_weapon_override
                .as_ref()
                .and_then(Option::as_ref)
        })
        .find(|weapon| {
            weapon
                .splash
                .as_ref()
                .is_some_and(|splash| splash.target_radius_below.is_some())
        })
        .cloned()
        .ok_or("a conditional mode weapon")?;
    let limit = shipped_cannon
        .splash
        .as_ref()
        .and_then(|splash| splash.target_radius_below)
        .ok_or("conditional")?;
    let large = shipped
        .units
        .archetypes()
        .find(|arch| {
            matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && !arch.capabilities.is_gas_extractor
                && arch.collision_radius >= limit
        })
        .map(|arch| (arch.unit_type_id, arch.collision_radius))
        .ok_or("a large structure")?;
    let visuals = |catalogs: RuntimeCatalogs| -> Result<usize, Box<dyn std::error::Error>> {
        let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
        let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
        sim_fixtures::without_random_delays(&mut world.catalog);
        // The same mode weapon as `catalogs` authors it, made to fire from where it stands.
        let (owner_kind, mut cannon) = world
            .catalog
            .archetypes()
            .find_map(|arch| {
                arch.modes
                    .iter()
                    .filter_map(|mode| {
                        mode.ground_weapon_override
                            .as_ref()
                            .and_then(Option::as_ref)
                    })
                    .find(|weapon| weapon.name == shipped_cannon.name)
                    .map(|weapon| (arch.unit_type_id, weapon.clone()))
            })
            .ok_or("the cannon")?;
        cannon.min_range = SimScalar::ZERO;
        cannon.firing_arc = sim_types::SimAngle::from_degrees(360);
        cannon.acquire_arc = sim_types::SimAngle::from_degrees(360);
        cannon.has_turret = false;
        let arch = world.catalog.get_mut(owner_kind)?;
        arch.weapon = Some(cannon.clone());
        arch.ground_weapon = Some(cannon);
        let (kind, radius) = large;
        let structure = world.spawn_structure(kind, PlayerId(1), 12, 12)?;
        let at = world.entities[&structure].position;
        let gunner = world.spawn_unit(
            owner_kind,
            PlayerId(0),
            SimVec2::new(at.x - radius - SimScalar::from_num(4), at.y),
        )?;
        world
            .entities
            .get_mut(&gunner)
            .ok_or("spawned")?
            .issue_order(sim_types::UnitOrder::attack(structure), false)?;
        let full = world.entities[&structure].hp + world.entities[&structure].shields;
        for tick in 0..120 {
            let snapshot = world.step(&sim_types::TickBundle {
                tick: sim_types::Tick(tick),
                commands: Vec::new(),
                state_checksum: None,
            });
            let now = world.entities[&structure].hp + world.entities[&structure].shields;
            if now < full {
                return Ok(snapshot
                    .combat_events
                    .iter()
                    .filter(|event| {
                        matches!(event, sim_types::CombatEventVisual::SplashVisual { .. })
                    })
                    .count());
            }
        }
        Err("the cannon never struck".into())
    };
    assert_eq!(visuals(shipped)?, 0, "the shipped condition: no area");
    assert_eq!(visuals(edited)?, 1, "the condition removed: the area opens");
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-23-anchored-commands` D-1: a mode's `orders` is required, as its
/// `leave_cast_ticks` is, so a units catalog with a mode written without it fails to load with
/// the deserialization error naming the field instead of guessing a stance's orders.
#[test]
fn a_mode_without_its_orders_is_refused() -> TestResult {
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let marker = "orders: (stop:";
    let mut checked = 0;
    for (at, _) in units.match_indices(marker) {
        let end = at + units[at..].find("),").ok_or("the orders' end")? + 2;
        let edited = format!("{}{}", &units[..at], &units[end..]);
        let result = UnitCatalog::from_ron_str(&sim_fixtures::shipped_factions(), &edited);
        assert!(
            matches!(&result, Err(SimCoreError::RonDeserializationError(message)) if message.contains("orders")),
            "{result:?}"
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "every shipped mode authors its orders: {checked}"
    );
    Ok(())
}

/// Plan `2026-11-23-anchored-commands` P-3, the simulation half: a stance's plain orders are
/// read from `units.ron` at run time. Granting Hold Position to every mode without it, and
/// taking Stop from every mode with Hold Position, flips what `UnitArchetype::stance_orders`
/// and a settled unit's `Entity::settled_stance_orders` answer, with no rebuild.
#[test]
fn edited_stance_orders_change_the_loaded_rule() -> TestResult {
    use sim_types::{AbilityEffectSpec, StanceOrders};
    let bundle = Bundle::new("stance-orders")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let stop_only = "orders: (stop: true, hold_position: false, attack: Attack)";
    let both = "orders: (stop: true, hold_position: true, attack: Attack)";
    assert!(
        text.contains(stop_only) && text.contains(both),
        "both shipped sets"
    );
    let edited_text = text
        .replace(
            both,
            "orders: (stop: false, hold_position: true, attack: Attack)",
        )
        .replace(stop_only, both);
    std::fs::write(&path, edited_text)?;
    let edited = bundle.load()?;
    let mut flipped = 0;
    for archetype in shipped.units.archetypes() {
        let loaded = edited.units.get(archetype.unit_type_id)?;
        assert_eq!(loaded.stance_orders(None), archetype.stance_orders(None));
        for (mode, after) in archetype.modes.iter().zip(&loaded.modes) {
            let before = archetype.stance_orders(Some(mode));
            let expected = if before == StanceOrders::ALL {
                StanceOrders {
                    stop: false,
                    hold_position: true,
                    attack: sim_types::AttackCommand::Attack,
                }
            } else if before
                == (StanceOrders {
                    stop: true,
                    hold_position: false,
                    attack: sim_types::AttackCommand::Attack,
                })
            {
                StanceOrders::ALL
            } else {
                before
            };
            assert_eq!(
                loaded.stance_orders(Some(after)),
                expected,
                "{}",
                mode.mode_name
            );
            if expected != before {
                flipped += 1;
                let ability = loaded
                    .abilities
                    .iter()
                    .position(|ability| {
                        matches!(&ability.effect,
                            AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == after.mode_name)
                    })
                    .ok_or("the mode's switch ability")?;
                let map = GameMap::new(24, 24, MapCell::default_ground(CliffLevel(0)))?;
                let mut world =
                    SimWorld::with_catalogs(map, 7, edited.units.clone(), edited.upgrades.clone())?;
                let unit = world.spawn_unit(
                    archetype.unit_type_id,
                    PlayerId(0),
                    SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(12)),
                )?;
                assert!(world.execute_ability(unit, u8::try_from(ability)?, None, None));
                let entity = &world.entities[&unit];
                assert_eq!(entity.settled_stance_orders(loaded), expected);
            }
        }
    }
    assert!(flipped >= 2, "both shipped sets flipped: {flipped}");
    bundle.finish()?;
    Ok(())
}

/// A 32-cell world from `catalogs` with the plateau: cells with `x >= 16` at level 1, the rest
/// level 0; no random weapon delays.
fn cliff_world(catalogs: RuntimeCatalogs) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..32 {
        for x in 16..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
    sim_fixtures::without_random_delays(&mut world.catalog);
    Ok(world)
}

/// The first small shipped mobile ground unit other than `except`, with its collision radius.
fn small_infantry(
    world: &SimWorld,
    except: UnitTypeId,
) -> Result<(UnitTypeId, SimScalar), Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.unit_type_id != except
                && arch.mover_type == sim_types::MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && matches!(arch.shape, ShapeKind::UnitSphere)
                && arch.collision_radius <= SimScalar::HALF
        })
        .map(|arch| (arch.unit_type_id, arch.collision_radius))
        .ok_or("small infantry")?)
}

/// Spawns `kind` for player 1 at `(x, 16)` holding position.
fn holding(
    world: &mut SimWorld,
    kind: UnitTypeId,
    x: SimScalar,
) -> Result<sim_types::EntityId, Box<dyn std::error::Error>> {
    let id = world.spawn_unit(kind, PlayerId(1), SimVec2::new(x, SimScalar::from_num(16)))?;
    world
        .entities
        .get_mut(&id)
        .ok_or("spawned")?
        .issue_order(sim_types::UnitOrder::HoldPosition, false)?;
    Ok(id)
}

/// Orders `attacker` onto `target` and plays up to 120 ticks, until any of `watched` loses life.
/// Returns which of `watched` lost life.
fn struck_by(
    world: &mut SimWorld,
    attacker: sim_types::EntityId,
    target: sim_types::EntityId,
    watched: &[sim_types::EntityId],
) -> Result<Vec<bool>, Box<dyn std::error::Error>> {
    let life = |world: &SimWorld, id| world.entities.get(&id).map_or(SimScalar::ZERO, |e| e.hp);
    let full: Vec<_> = watched.iter().map(|&id| life(world, id)).collect();
    world
        .entities
        .get_mut(&attacker)
        .ok_or("spawned")?
        .issue_order(sim_types::UnitOrder::attack(target), false)?;
    for tick in 0..120 {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        if watched
            .iter()
            .zip(&full)
            .any(|(&id, &before)| life(world, id) < before)
        {
            break;
        }
    }
    Ok(watched
        .iter()
        .zip(&full)
        .map(|(&id, &before)| life(world, id) < before)
        .collect())
}

/// Plan `2026-11-23-splash-cliff-levels` P-7 (B-170): the cliff rule is read from `units.ron` at
/// run time. The sieged cannon (a mode weapon with a conditional circular area), fired from the
/// plateau at infantry below it, spares infantry on the plateau inside its first zone with the
/// shipped catalog, and strikes it in the same binary with the row edited to `cliff: Any`. The
/// Baneling analogue's burst (an attacker-centred area) from the plateau strikes its target below
/// with the shipped catalog, and leaves it untouched with the row's
/// `strike_target_on_other_level: true` removed.
#[test]
fn an_edited_cliff_rule_changes_who_an_area_strikes() -> TestResult {
    let bundle = Bundle::new("splash-cliff")?;
    let shipped = bundle.load()?;
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let condition = "target_radius_below: Some(1.25),";
    let fallback = "strike_target_on_other_level: true,";
    assert!(
        text.contains(condition) && text.contains(fallback),
        "the shipped rows"
    );
    std::fs::write(
        &path,
        text.replacen(condition, &format!("{condition} cliff: Any,"), 1),
    )?;
    let any_level_cannon = bundle.load()?;
    std::fs::write(&path, text.replacen(fallback, "", 1))?;
    let no_fallback = bundle.load()?;
    bundle.restore()?;
    let shipped_again = bundle.load()?;

    let cannon = |catalogs: RuntimeCatalogs| -> Result<Vec<bool>, Box<dyn std::error::Error>> {
        let mut world = cliff_world(catalogs)?;
        let (owner_kind, mut weapon) = world
            .catalog
            .archetypes()
            .find_map(|arch| {
                arch.modes
                    .iter()
                    .filter_map(|mode| {
                        mode.ground_weapon_override
                            .as_ref()
                            .and_then(Option::as_ref)
                    })
                    .find(|weapon| {
                        weapon
                            .splash
                            .as_ref()
                            .is_some_and(|splash| splash.target_radius_below.is_some())
                    })
                    .map(|weapon| (arch.unit_type_id, weapon.clone()))
            })
            .ok_or("a conditional mode weapon")?;
        weapon.min_range = SimScalar::ZERO;
        weapon.firing_arc = sim_types::SimAngle::from_degrees(360);
        weapon.acquire_arc = sim_types::SimAngle::from_degrees(360);
        weapon.has_turret = false;
        let arch = world.catalog.get_mut(owner_kind)?;
        arch.weapon = Some(weapon.clone());
        arch.ground_weapon = Some(weapon);
        let (kind, _) = small_infantry(&world, owner_kind)?;
        let gunner = world.spawn_unit(
            owner_kind,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(22), SimScalar::from_num(16)),
        )?;
        let target = holding(&mut world, kind, SimScalar::from_num(15.5))?;
        let beside = holding(&mut world, kind, SimScalar::from_num(16.3))?;
        struck_by(&mut world, gunner, target, &[target, beside])
    };
    assert_eq!(
        cannon(shipped)?,
        vec![true, false],
        "shipped: kept to its level"
    );
    assert_eq!(
        cannon(any_level_cannon)?,
        vec![true, true],
        "edited: every level"
    );

    let burst = |catalogs: RuntimeCatalogs| -> Result<bool, Box<dyn std::error::Error>> {
        let mut world = cliff_world(catalogs)?;
        let burst = world
            .catalog
            .archetypes()
            .find(|arch| {
                arch.weapon.as_ref().is_some_and(|weapon| {
                    weapon
                        .splash
                        .as_ref()
                        .is_some_and(|splash| splash.impact == sim_types::SplashImpact::Attacker)
                })
            })
            .map(|arch| (arch.unit_type_id, arch.collision_radius))
            .ok_or("an attacker-centred area")?;
        let (burst_kind, own) = burst;
        let (kind, radius) = small_infantry(&world, burst_kind)?;
        let origin = SimScalar::from_num(16) + own;
        let baneling = world.spawn_unit(
            burst_kind,
            PlayerId(0),
            SimVec2::new(origin, SimScalar::from_num(16)),
        )?;
        let target_x = origin - own - radius - SimScalar::from_num(0.1);
        let target = holding(&mut world, kind, target_x)?;
        Ok(struck_by(&mut world, baneling, target, &[target])?[0])
    };
    assert!(burst(shipped_again)?, "shipped: struck across the cliff");
    assert!(
        !burst(no_fallback)?,
        "edited: the target on another level untouched"
    );
    bundle.finish()?;
    Ok(())
}

/// `source` with the first `field` after the first `anchor` given `replacement` (see
/// [`replace_first_field`]).
fn replace_field_after(source: &str, anchor: &str, field: &str, replacement: &str) -> String {
    let at = source.find(anchor).expect("fixture has the anchor");
    format!(
        "{}{}",
        &source[..at],
        replace_first_field(&source[at..], field, replacement)
    )
}

/// A world on `catalogs` with nothing in it but what the caller spawns.
fn bare_world(catalogs: &RuntimeCatalogs) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    Ok(SimWorld::with_catalogs(
        map,
        11,
        catalogs.units.clone(),
        catalogs.upgrades.clone(),
    )?)
}

/// Steps `world` one tick with `commands`.
fn step_with(world: &mut SimWorld, commands: Vec<sim_types::PlayerCommand>) {
    world.step(&sim_types::TickBundle {
        tick: sim_types::Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

/// Whether a queued `order` from player 0 reaches `unit` of `world` (it ends the tick at the
/// back of the unit's queue).
fn queued_order_reaches(
    world: &mut SimWorld,
    unit: sim_types::EntityId,
    order: &sim_types::UnitOrder,
) -> bool {
    step_with(
        world,
        vec![sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![unit],
            order: order.clone(),
            queue: true,
        }],
    );
    world.entities[&unit].order_queue.back() == Some(order)
}

/// Plan `2026-11-23-stance-checked-orders` P-8, the simulation half (B-171, B-172, C6): with
/// every weapon of the armed structures removed from `structures.ron` at run time, they lose
/// Stop in the default stance's rule and in the simulation's admission, with no rebuild.
#[test]
fn an_armed_structure_disarmed_in_its_file_loses_stop() -> TestResult {
    let bundle = Bundle::new("disarmed-structures")?;
    let shipped = bundle.load()?;
    let armed: Vec<_> = shipped
        .units
        .archetypes()
        .filter(|arch| {
            matches!(arch.shape, ShapeKind::BuildingDome { .. }) && arch.stance_armed(None)
        })
        .map(|arch| (arch.unit_type_id, arch.slug.clone()))
        .collect();
    assert!(
        !armed.is_empty(),
        "the shipped catalog has an armed structure"
    );
    let path = bundle.directory.join(FILES[1]);
    let mut text = std::fs::read_to_string(&path)?;
    for (_, slug) in &armed {
        let anchor = format!("slug: \"{slug}\"");
        text = replace_field_after(&text, &anchor, "ground_weapon", "None");
        text = replace_field_after(&text, &anchor, "air_weapon", "None");
        // A structure without a weapon has no acquisition range either.
        text = replace_field_after(&text, &anchor, "scan_range", "None");
    }
    std::fs::write(&path, text)?;
    let edited = bundle.load()?;
    for (id, slug) in &armed {
        let before = shipped.units.get(*id)?;
        let after = edited.units.get(*id)?;
        assert!(before.stance_orders(None).stop, "{slug}: shipped with Stop");
        assert!(!after.stance_armed(None), "{slug}: disarmed");
        assert_eq!(
            after.stance_orders(None),
            sim_types::StanceOrders::NONE,
            "{slug}"
        );
        for (catalogs, takes) in [(&shipped, true), (&edited, false)] {
            let mut world = bare_world(catalogs)?;
            let unit = world.spawn_structure(*id, PlayerId(0), 10, 10)?;
            assert_eq!(
                queued_order_reaches(&mut world, unit, &sim_types::UnitOrder::Stop),
                takes,
                "{slug}: the simulation's Stop"
            );
        }
    }
    bundle.finish()?;
    Ok(())
}

/// The mode switch ability of `arch` for its mode `mode`.
fn switch_of(
    arch: &sim_core::UnitArchetype,
    mode: usize,
) -> Result<u8, Box<dyn std::error::Error>> {
    let name = &arch.modes[mode].mode_name;
    let ability = arch
        .abilities
        .iter()
        .position(|ability| {
            matches!(&ability.effect,
                sim_types::AbilityEffectSpec::SwitchMode { mode_name } if mode_name == name)
        })
        .ok_or("the mode's switch ability")?;
    Ok(u8::try_from(ability)?)
}

/// `arch`'s unit spawned in `world` and settled in its mode `mode`.
fn settled_in(
    world: &mut SimWorld,
    arch: &sim_core::UnitArchetype,
    mode: usize,
) -> Result<sim_types::EntityId, Box<dyn std::error::Error>> {
    let unit = world.spawn_unit(
        arch.unit_type_id,
        PlayerId(0),
        SimVec2::new(SimScalar::from_num(12), SimScalar::from_num(12)),
    )?;
    assert!(world.execute_ability(unit, switch_of(arch, mode)?, None, None));
    for _ in 0..400 {
        if world.entities[&unit].channeling_remaining_ticks == 0 {
            return Ok(unit);
        }
        step_with(world, Vec::new());
    }
    Err("the switch ends within 400 ticks".into())
}

/// Plan `2026-11-23-stance-checked-orders` P-8, the simulation half (B-172, C6): an anchoring
/// mode authored with Hold Position at run time lets the simulation hold a unit settled in it.
#[test]
fn an_anchoring_mode_given_hold_position_in_its_file_holds() -> TestResult {
    let bundle = Bundle::new("hold-anchored")?;
    let shipped = bundle.load()?;
    let (arch, mode) = shipped
        .units
        .archetypes()
        .find_map(|arch| {
            let mode = arch
                .modes
                .iter()
                .position(|mode| mode.is_immobile && !mode.orders.hold_position)?;
            Some((arch.clone(), mode))
        })
        .ok_or("an anchoring mode without Hold Position")?;
    let anchor = format!("mode_name: \"{}\"", arch.modes[mode].mode_name);
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let stop = if arch.modes[mode].orders.stop {
        "true"
    } else {
        "false"
    };
    std::fs::write(
        &path,
        replace_field_after(
            &text,
            &anchor,
            "orders",
            &format!(
                "(stop: {stop}, hold_position: true, attack: {:?})",
                arch.modes[mode].orders.attack
            ),
        ),
    )?;
    let edited = bundle.load()?;
    for (catalogs, holds) in [(&shipped, false), (&edited, true)] {
        let loaded = catalogs.units.get(arch.unit_type_id)?;
        let mut world = bare_world(catalogs)?;
        let unit = settled_in(&mut world, loaded, mode)?;
        step_with(
            &mut world,
            vec![sim_types::PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![unit],
                order: sim_types::UnitOrder::HoldPosition,
                queue: false,
            }],
        );
        assert_eq!(
            world.entities[&unit].is_holding_position, holds,
            "edited {holds}"
        );
    }
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-23-stance-checked-orders` P-8, the simulation half (B-173, C6): a mode whose
/// replacement ground weapon is removed at run time leaves a unit armed only in that mode
/// unarmed there, by `stance_armed` and by the settled unit.
#[test]
fn a_mode_without_its_replacement_weapon_in_its_file_is_unarmed() -> TestResult {
    let bundle = Bundle::new("unarmed-mode")?;
    let shipped = bundle.load()?;
    let (arch, mode) = shipped
        .units
        .archetypes()
        .find_map(|arch| {
            let mode = arch
                .modes
                .iter()
                .position(|mode| matches!(mode.ground_weapon_override, Some(Some(_))))?;
            (!arch.stance_armed(None)).then(|| (arch.clone(), mode))
        })
        .ok_or("an archetype armed only in a mode")?;
    let anchor = format!("mode_name: \"{}\"", arch.modes[mode].mode_name);
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    std::fs::write(
        &path,
        replace_field_after(&text, &anchor, "ground_weapon_override", "None"),
    )?;
    let edited = bundle.load()?;
    for (catalogs, armed) in [(&shipped, true), (&edited, false)] {
        let loaded = catalogs.units.get(arch.unit_type_id)?;
        assert_eq!(loaded.stance_armed(Some(&loaded.modes[mode])), armed);
        let mut world = bare_world(catalogs)?;
        let unit = settled_in(&mut world, loaded, mode)?;
        assert_eq!(world.entities[&unit].settled_armed(loaded), armed);
    }
    bundle.finish()?;
    Ok(())
}

/// Ticks from the release of `shooter_type`'s first attack to its first missile, attacking a held
/// `target_type` 4 world units away, in a world built from `catalogs`.
fn first_missile_after_release(
    catalogs: RuntimeCatalogs,
    shooter_type: UnitTypeId,
    target_type: UnitTypeId,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut world = SimWorld::with_catalogs(empty_map()?, 7, catalogs.units, catalogs.upgrades)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), SimVec2::from_i32(5, 8))?;
    let target = world.spawn_unit(target_type, PlayerId(1), SimVec2::from_i32(9, 8))?;
    let mut commands = vec![
        sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![shooter],
            order: sim_types::UnitOrder::attack(target),
            queue: false,
        },
        sim_types::PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![target],
            order: sim_types::UnitOrder::HoldPosition,
            queue: false,
        },
    ];
    let mut release = None;
    for tick in 0..60_u32 {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        let index = usize::try_from(tick)?;
        let entity = world.entities.get(&shooter).ok_or("the shooter survives")?;
        if release.is_none()
            && matches!(entity.weapon_phase, sim_core::WeaponPhase::Backswing { .. })
        {
            release = Some(index);
        }
        if world
            .projectiles
            .iter()
            .any(|projectile| projectile.attacker_id == Some(shooter))
        {
            let release = release.ok_or("a missile before any release")?;
            return Ok(index - release);
        }
    }
    Err("no missile in 60 ticks".into())
}

/// Plan `2026-11-23-strike-delays` P-8: setting the shipped first-strike delay to 0 in a copied
/// `units.ron` launches the first missile on the release tick again, with no recompilation.
#[test]
fn an_edited_first_strike_delay_moves_the_first_missile_without_recompiling() -> TestResult {
    let shipped = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    let shooter = shipped
        .units
        .archetypes()
        .find(|archetype| {
            [
                &archetype.ground_weapon,
                &archetype.air_weapon,
                &archetype.weapon,
            ]
            .into_iter()
            .flatten()
            .any(|weapon| weapon.first_strike_delay_ticks > 0 && weapon.projectile_speed.is_some())
        })
        .ok_or("a shipped missile weapon delays its first strike")?;
    let (shooter_type, faction) = (shooter.unit_type_id, shooter.faction);
    let target_type = shipped
        .units
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == sim_types::MoverType::Air
                && archetype.faction != faction
                && archetype.max_hp >= SimScalar::from_i32(100)
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("an air unit of another faction")?
        .unit_type_id;

    let bundle = Bundle::new("strike-delay")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    std::fs::write(
        bundle.directory.join(FILES[0]),
        replace_first_field(&units, "first_strike_delay_ticks", "0"),
    )?;
    let edited = bundle.load()?;
    assert!(
        edited
            .units
            .archetypes()
            .flat_map(|archetype| [
                &archetype.ground_weapon,
                &archetype.air_weapon,
                &archetype.weapon
            ])
            .flatten()
            .all(|weapon| weapon.first_strike_delay_ticks == 0),
        "premise: the edited catalog delays no first strike"
    );
    let after = [
        first_missile_after_release(shipped, shooter_type, target_type)?,
        first_missile_after_release(edited, shooter_type, target_type)?,
    ];
    assert_eq!(
        after,
        [1, 0],
        "the shipped missile leaves a tick after the release, the edited one on it"
    );
    bundle.finish()?;
    Ok(())
}

/// Whether a weapon fires while its unit moves under an order that is no attack (plan
/// `2026-11-23-turret-wind-up` D-2): `Moving` and outside attack orders.
fn fires_while_moving(weapon: &sim_types::WeaponSpec) -> bool {
    weapon.allowed_movement == sim_types::AllowedMovement::Moving
        && weapon.fires_outside_attack_order
}

/// The life and shields an air enemy holding beside its path loses while a unit of `shooter_type`
/// moves past it for 25 ticks, in `catalogs`.
fn damage_dealt_in_passing(
    catalogs: RuntimeCatalogs,
    shooter_type: UnitTypeId,
    enemy_type: UnitTypeId,
) -> Result<SimScalar, Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(map, 7, catalogs.units, catalogs.upgrades)?;
    let shooter = world.spawn_unit(shooter_type, PlayerId(0), SimVec2::from_i32(8, 8))?;
    let enemy = world.spawn_unit(enemy_type, PlayerId(1), SimVec2::from_i32(10, 6))?;
    let unit = world.entities.get_mut(&shooter).ok_or("shooter")?;
    unit.heading = sim_types::SimAngle::DEG_90;
    unit.turret_angle = sim_types::SimAngle::DEG_90;
    let life = |world: &SimWorld| {
        world
            .entities
            .get(&enemy)
            .map_or(SimScalar::ZERO, |e| e.hp + e.shields)
    };
    let before = life(&world);
    let mut commands = vec![
        sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![shooter],
            order: sim_types::UnitOrder::Move {
                target: SimVec2::from_i32(8, 28),
            },
            queue: false,
        },
        sim_types::PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![enemy],
            order: sim_types::UnitOrder::HoldPosition,
            queue: false,
        },
    ];
    for tick in 0..25_u32 {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
    }
    Ok(before - life(&world))
}

/// Plan `2026-11-23-turret-wind-up` P-7: clearing the shipped weapon's `allowed_movement: Moving`
/// and `fires_outside_attack_order: true` in a copied `units.ron` makes its unit hold fire while
/// it moves past an enemy, in the same binary.
#[test]
fn an_edited_movement_rule_makes_a_unit_hold_fire_while_moving_without_recompiling() -> TestResult {
    let shipped = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    let shooter = shipped
        .units
        .archetypes()
        .find(|archetype| {
            archetype.mover_type == sim_types::MoverType::Air
                && archetype
                    .air_weapon
                    .as_ref()
                    .is_some_and(fires_while_moving)
        })
        .ok_or("a shipped air weapon fires while moving outside attack orders")?;
    let (shooter_type, faction) = (shooter.unit_type_id, shooter.faction);
    let enemy_type = shipped
        .units
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == sim_types::MoverType::Air
                && archetype.faction != faction
                && archetype.max_hp >= SimScalar::from_i32(100)
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .ok_or("an air unit of another faction")?
        .unit_type_id;

    let bundle = Bundle::new("weapon-movement")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    let rules = "allowed_movement: Moving,\n                fires_outside_attack_order: true,\n";
    assert!(
        units.contains(rules),
        "premise: the shipped rules are written together"
    );
    std::fs::write(
        bundle.directory.join(FILES[0]),
        units.replacen(rules, "", 1),
    )?;
    let edited = bundle.load()?;
    let edited_weapon = edited
        .units
        .get(shooter_type)?
        .air_weapon
        .clone()
        .ok_or("the edited shooter keeps its air weapon")?;
    assert_eq!(
        (
            edited_weapon.allowed_movement,
            edited_weapon.fires_outside_attack_order
        ),
        (sim_types::AllowedMovement::Stopped, false),
        "premise: the edited weapon reads the engine defaults"
    );
    let shipped_damage = damage_dealt_in_passing(shipped, shooter_type, enemy_type)?;
    let edited_damage = damage_dealt_in_passing(edited, shooter_type, enemy_type)?;
    assert!(
        shipped_damage > SimScalar::ZERO,
        "the shipped unit fires while it moves past the enemy"
    );
    assert_eq!(
        edited_damage,
        SimScalar::ZERO,
        "the edited unit holds fire while it moves"
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-24-attack-orders` P-8, the data half (B-174, C6): the Lurker analogue's A
/// command is read from its authored `default_stance_orders` at run time. With the line removed
/// from `units.ron` it derives Scan Move, as every other unarmed mobile unit, with no rebuild.
#[test]
fn removing_an_authored_default_stance_derives_scan_move() -> TestResult {
    use sim_types::AttackCommand;
    let bundle = Bundle::new("authored-default-stance")?;
    let shipped = bundle.load()?;
    let authored: Vec<_> = shipped
        .units
        .archetypes()
        .filter(|arch| arch.default_stance_orders.is_some())
        .map(|arch| (arch.unit_type_id, arch.slug.clone()))
        .collect();
    assert_eq!(
        authored.len(),
        1,
        "one archetype authors its default stance"
    );
    let (id, slug) = &authored[0];
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let start = text
        .find("default_stance_orders: Some(")
        .ok_or("the authored line")?;
    let end = start + text[start..].find("),\n").ok_or("the line's end")? + 3;
    std::fs::write(&path, format!("{}{}", &text[..start], &text[end..]))?;
    let edited = bundle.load()?;
    let before = shipped.units.get(*id)?;
    let after = edited.units.get(*id)?;
    assert!(
        !before.stance_armed(None),
        "{slug}: unarmed in its default stance"
    );
    assert_eq!(
        before.stance_orders(None).attack,
        AttackCommand::Attack,
        "{slug}"
    );
    assert_eq!(after.default_stance_orders, None, "{slug}");
    assert_eq!(
        after.stance_orders(None),
        sim_types::StanceOrders {
            attack: AttackCommand::ScanMove,
            ..before.stance_orders(None)
        },
        "{slug}: derived"
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-24-attack-orders` P-8, the data half (B-186, C6): an unarmed mobile unit given
/// `default_stance_orders` with no A command in `units.ron` at run time loses its Scan Move.
#[test]
fn an_unarmed_walker_authored_without_an_a_command_loses_scan_move() -> TestResult {
    use sim_types::AttackCommand;
    let bundle = Bundle::new("no-scan-move")?;
    let shipped = bundle.load()?;
    let walker = shipped
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && !arch.stance_armed(None)
                && arch.default_stance_orders.is_none()
        })
        .ok_or("an unarmed mobile unit that derives its default stance")?
        .clone();
    assert_eq!(
        walker.stance_orders(None).attack,
        AttackCommand::ScanMove,
        "{}",
        walker.slug
    );
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let anchor = format!("slug: \"{}\",", walker.slug);
    let authored = sim_types::StanceOrders {
        attack: AttackCommand::None,
        ..walker.stance_orders(None)
    };
    let line = format!(
        "{anchor}\n            default_stance_orders: Some((stop: {}, hold_position: {}, attack: None)),",
        authored.stop, authored.hold_position
    );
    assert_eq!(text.matches(&anchor).count(), 1, "one row");
    std::fs::write(&path, text.replace(&anchor, &line))?;
    let edited = bundle.load()?;
    assert_eq!(
        edited.units.get(walker.unit_type_id)?.stance_orders(None),
        authored,
        "{}",
        walker.slug
    );
    bundle.finish()?;
    Ok(())
}

/// Plan `2026-11-24-attack-orders` D-1, P-8 (validation): Scan Move and Hold Position are both
/// commands of the `move` ability (`core` AbilData:555-557), so a stance authored with Scan Move
/// and without Hold Position fails to load with the typed error, for a mode and for an authored
/// default stance alike.
#[test]
fn a_stance_with_scan_move_and_no_hold_position_is_refused() -> TestResult {
    use sim_core::catalog::CatalogError;
    let units = std::fs::read_to_string(catalog_directory().join(FILES[0]))?;
    let factions = sim_fixtures::shipped_factions();
    let mode = "orders: (stop: true, hold_position: false, attack: Attack)";
    assert_eq!(units.matches(mode).count(), 1, "the shipped anchoring set");
    let authored = "default_stance_orders: Some((stop: true, hold_position: true, attack: Attack))";
    assert_eq!(
        units.matches(authored).count(),
        1,
        "the shipped default set"
    );
    for (shipped, edited, stance) in [
        (
            mode,
            "orders: (stop: true, hold_position: false, attack: ScanMove)",
            "modes.",
        ),
        (
            authored,
            "default_stance_orders: Some((stop: true, hold_position: false, attack: ScanMove))",
            "default_stance_orders",
        ),
    ] {
        let result = UnitCatalog::from_ron_str(&factions, &units.replace(shipped, edited));
        assert!(
            matches!(
                &result,
                Err(SimCoreError::SpellCatalog(CatalogError::ScanMoveWithoutHoldPosition {
                    stance: found,
                    ..
                })) if found.starts_with(stance)
            ),
            "{result:?}"
        );
    }
    // Scan Move with Hold Position loads.
    let held = units.replace(
        authored,
        "default_stance_orders: Some((stop: true, hold_position: true, attack: ScanMove))",
    );
    assert!(UnitCatalog::from_ron_str(&factions, &held).is_ok());
    Ok(())
}

/// Plan `2026-11-24-attack-orders` P-8, the simulation half (B-174, C6): an anchoring mode
/// authored with no A command at run time refuses an `AttackTarget` to a unit settled in it,
/// while the shipped mode takes it, with no rebuild.
#[test]
fn an_anchoring_mode_authored_without_attack_refuses_an_attack() -> TestResult {
    use sim_types::{AttackCommand, UnitOrder};
    let bundle = Bundle::new("anchored-no-attack")?;
    let shipped = bundle.load()?;
    let (arch, mode) = shipped
        .units
        .archetypes()
        .find_map(|arch| {
            let mode = arch.modes.iter().position(|mode| {
                mode.is_immobile
                    && !mode.orders.hold_position
                    && mode.orders.attack == AttackCommand::Attack
                    && arch
                        .stance_weapon_against(Some(mode), sim_types::MoverType::Ground)
                        .is_some()
            })?;
            Some((arch.clone(), mode))
        })
        .ok_or("an armed anchoring mode with Attack and no Hold Position")?;
    let target_kind = shipped
        .units
        .archetypes()
        .find(|other| {
            !other.is_immobile_by_default()
                && other.mover_type == sim_types::MoverType::Ground
                && other.modes.is_empty()
        })
        .ok_or("a ground walker")?
        .unit_type_id;
    let anchor = format!("mode_name: \"{}\"", arch.modes[mode].mode_name);
    let path = bundle.directory.join(FILES[0]);
    let text = std::fs::read_to_string(&path)?;
    let orders = arch.modes[mode].orders;
    std::fs::write(
        &path,
        replace_field_after(
            &text,
            &anchor,
            "orders",
            &format!(
                "(stop: {}, hold_position: {}, attack: None)",
                orders.stop, orders.hold_position
            ),
        ),
    )?;
    let edited = bundle.load()?;
    for (catalogs, takes) in [(&shipped, true), (&edited, false)] {
        let loaded = catalogs.units.get(arch.unit_type_id)?;
        let mut world = bare_world(catalogs)?;
        let unit = settled_in(&mut world, loaded, mode)?;
        let enemy = world.spawn_unit(
            target_kind,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(16), SimScalar::from_num(12)),
        )?;
        assert_eq!(
            queued_order_reaches(&mut world, unit, &UnitOrder::attack(enemy)),
            takes,
            "edited {}",
            !takes
        );
    }
    bundle.finish()?;
    Ok(())
}

/// The shipped archetype carrying a splash missile, and that weapon.
fn splash_missile_archetype(
    catalogs: &RuntimeCatalogs,
) -> Option<(sim_core::UnitArchetype, sim_core::WeaponSpec)> {
    catalogs.units.archetypes().find_map(|arch| {
        [&arch.weapon, &arch.ground_weapon, &arch.air_weapon]
            .into_iter()
            .flatten()
            .find(|w| w.splash.is_some() && w.projectile_speed.is_some())
            .map(|w| (arch.clone(), w.clone()))
    })
}

/// Fires `catalogs`' splash missile at an unarmed target, kills the target in flight, and
/// returns whether a bystander beside the target was struck by the area at the point.
fn lost_splash_strikes(catalogs: RuntimeCatalogs) -> Result<bool, Box<dyn std::error::Error>> {
    let (shooter_arch, weapon) =
        splash_missile_archetype(&catalogs).ok_or("a shipped weapon is a splash missile")?;
    let plane = if shooter_arch.air_weapon.as_ref() == Some(&weapon) {
        sim_types::MoverType::Air
    } else {
        sim_types::MoverType::Ground
    };
    let mut target_arch = catalogs
        .units
        .archetypes()
        .find(|arch| {
            arch.mover_type == plane
                && arch.max_speed > SimScalar::ZERO
                && weapon.target_flags.intersects(arch.attributes)
        })
        .cloned()
        .ok_or("an archetype on the weapon's plane")?;
    let mut units = catalogs.units;
    let dummy = UnitTypeId(
        units
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    target_arch.unit_type_id = dummy;
    "scratch_unarmed_target".clone_into(&mut target_arch.slug);
    target_arch.weapon = None;
    target_arch.ground_weapon = None;
    target_arch.air_weapon = None;
    units.register(target_arch);
    let mut world = SimWorld::with_catalogs(
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?,
        41,
        units,
        catalogs.upgrades,
    )?;
    let shooter = world.spawn_unit(
        shooter_arch.unit_type_id,
        PlayerId(0),
        SimVec2::from_i32(13, 16),
    )?;
    let target = world.spawn_unit(dummy, PlayerId(1), SimVec2::from_i32(16, 16))?;
    let bystander = world.spawn_unit(
        dummy,
        PlayerId(1),
        SimVec2::new(SimScalar::from_i32(16), SimScalar::from_num(16.5)),
    )?;
    let vitals = |world: &SimWorld| world.entities.get(&bystander).map(|b| (b.hp, b.shields));
    let full = vitals(&world);
    world
        .entities
        .get_mut(&shooter)
        .ok_or("spawned")?
        .issue_order(sim_types::UnitOrder::attack(target), false)
        .map_err(|error| format!("{error:?}"))?;
    let step = |world: &mut SimWorld, tick: u32| {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
    };
    let mut tick = 1;
    while world.projectiles.is_empty() {
        assert!(tick < 100, "the missile launched within 100 ticks");
        step(&mut world, tick);
        tick += 1;
    }
    world.entities.get_mut(&target).ok_or("in flight")?.hp = SimScalar::ZERO;
    // Stop once the missile is spent: later the shooter acquires the bystander itself.
    for later in tick..tick + 200 {
        step(&mut world, later);
        if world.projectiles.is_empty() {
            break;
        }
    }
    Ok(vitals(&world) != full)
}

/// P-3 (plan `2026-11-23-vision-and-detection` D-1): authoring `target_loss_behavior: Fizzle`
/// on the shipped splash missile restores the vanishing missile, with no area at the point, in
/// the same binary.
#[test]
fn an_authored_fizzle_drops_a_lost_missiles_area() -> TestResult {
    let bundle = Bundle::new("fizzle")?;
    let shipped = bundle.load()?;
    let (arch, _) = splash_missile_archetype(&shipped).ok_or("a splash missile is shipped")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    let row = units
        .find(&format!("slug: \"{}\"", arch.slug))
        .ok_or("the archetype's row")?;
    let field = row
        + units[row..]
            .find("projectile_speed: Some(")
            .ok_or("the row's missile speed")?;
    let edited_text = format!(
        "{}target_loss_behavior: Fizzle,\n                {}",
        &units[..field],
        &units[field..]
    );
    std::fs::write(bundle.directory.join(FILES[0]), edited_text)?;
    let edited = bundle.load()?;
    let (_, edited_weapon) =
        splash_missile_archetype(&edited).ok_or("the edited missile is still shipped")?;
    assert_eq!(
        edited_weapon.target_loss_behavior,
        sim_types::ProjectileTargetLossBehavior::Fizzle,
        "premise: the edit reached the splash missile"
    );
    assert!(
        lost_splash_strikes(shipped)?,
        "the shipped missile lands on the point and its area strikes"
    );
    assert!(
        !lost_splash_strikes(edited)?,
        "the authored fizzle vanishes without an area"
    );
    bundle.finish()?;
    Ok(())
}

/// P-7 (plan `2026-11-23-vision-and-detection` D-2, §2.4): exactly five shipped archetypes
/// detect (the Observer, Raven, Missile Turret, Photon Cannon and Spore Crawler analogues), each
/// with 5.0.14's `Detector11` radius, which equals its sight radius.
#[test]
fn detection_radius_follows_starcraft() -> TestResult {
    let catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    let detectors: Vec<_> = catalogs
        .units
        .archetypes()
        .filter(|arch| arch.detection_radius.is_some())
        .collect();
    assert_eq!(detectors.len(), 5, "five detectors are shipped");
    for arch in detectors {
        assert!(arch.is_detector());
        assert_eq!(arch.detection_radius, Some(SimScalar::from_i32(11)));
        assert_eq!(arch.detection_radius, Some(arch.sight_radius));
    }
    Ok(())
}

/// Whether a mobile detector of player 0 standing 8 from a cloaked enemy lets its player attack
/// that enemy, with `catalogs`.
fn detector_reveals_at_eight(
    catalogs: RuntimeCatalogs,
    detector: UnitTypeId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let enemy_type = catalogs
        .units
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
        })
        .map(|arch| arch.unit_type_id)
        .ok_or("a mobile ground archetype")?;
    let mut world = SimWorld::with_catalogs(
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?,
        5,
        catalogs.units,
        catalogs.upgrades,
    )?;
    world.spawn_unit(detector, PlayerId(0), SimVec2::from_i32(10, 16))?;
    let enemy = world.spawn_unit(enemy_type, PlayerId(1), SimVec2::from_i32(18, 16))?;
    world.entities.get_mut(&enemy).ok_or("spawned")?.is_cloaked = true;
    world.step(&sim_types::TickBundle {
        tick: sim_types::Tick(0),
        commands: Vec::new(),
        state_checksum: None,
    });
    Ok(world.is_entity_available_for_combat(PlayerId(0), enemy))
}

/// Zero recompilation for B-56: a catalog copy that shortens a mobile detector's detection
/// radius to 6 loses a cloaked enemy at 8 that the shipped radius 11 reveals, while its sight
/// still covers the enemy's cell.
#[test]
fn an_edited_detection_radius_loses_a_cloaked_enemy() -> TestResult {
    let bundle = Bundle::new("detection")?;
    let shipped = bundle.load()?;
    let detector = shipped
        .units
        .archetypes()
        .find(|arch| arch.is_detector() && arch.max_speed > SimScalar::ZERO)
        .cloned()
        .ok_or("a mobile detector is shipped")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    let row = units
        .find(&format!("slug: \"{}\"", detector.slug))
        .ok_or("the detector's row")?;
    let edited_text = format!(
        "{}{}",
        &units[..row],
        replace_first_field(&units[row..], "detection_radius", "Some(6.0)")
    );
    std::fs::write(bundle.directory.join(FILES[0]), edited_text)?;
    let edited = bundle.load()?;
    assert_eq!(
        edited.units.get(detector.unit_type_id)?.detection_radius,
        Some(SimScalar::from_i32(6)),
        "premise: the edit reached the detector"
    );
    assert!(detector_reveals_at_eight(shipped, detector.unit_type_id)?);
    assert!(!detector_reveals_at_eight(edited, detector.unit_type_id)?);
    bundle.finish()?;
    Ok(())
}

/// Whether a holding ground unit of player 1 that hits air ever targets an air unit of
/// `attacker` striking it from over a plateau it cannot see, within 100 ticks, in `catalogs`.
fn victim_answers_from_below(
    catalogs: RuntimeCatalogs,
    attacker: UnitTypeId,
) -> Result<bool, Box<dyn std::error::Error>> {
    let victim_arch = catalogs
        .units
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !arch.is_detector()
                && arch
                    .weapon_against(sim_types::MoverType::Air, None)
                    .is_some_and(|w| w.max_range >= SimScalar::from_i32(4))
        })
        .cloned()
        .ok_or("a ground unit that hits air")?;
    let attacker_arch = catalogs.units.get(attacker)?.clone();
    let reach = attacker_arch
        .weapon_against(sim_types::MoverType::Ground, None)
        .ok_or("the attacker hits ground")?
        .max_range
        .min(
            victim_arch
                .weapon_against(sim_types::MoverType::Air, None)
                .ok_or("the victim hits air")?
                .max_range,
        )
        + attacker_arch.collision_radius
        + victim_arch.collision_radius
        - SimScalar::from_num(0.25);
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..32 {
        for x in 16..32 {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(1);
        }
    }
    let mut world = SimWorld::with_catalogs(map, 9, catalogs.units, catalogs.upgrades)?;
    let over = SimVec2::new(SimScalar::from_num(16.5), SimScalar::from_num(16.5));
    let shooter = world.spawn_unit(attacker, PlayerId(0), over)?;
    let victim = world.spawn_unit(
        victim_arch.unit_type_id,
        PlayerId(1),
        SimVec2::new(over.x - reach, over.y),
    )?;
    let mut commands = vec![
        sim_types::PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![shooter],
            order: sim_types::UnitOrder::attack(victim),
            queue: false,
        },
        sim_types::PlayerCommand {
            player_id: PlayerId(1),
            entities: vec![victim],
            order: sim_types::UnitOrder::HoldPosition,
            queue: false,
        },
    ];
    for tick in 0..100_u32 {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        if world
            .entities
            .get(&victim)
            .is_some_and(|v| v.current_target == Some(shooter))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// P-16 (plan `2026-11-23-vision-and-detection` D-4): a catalog copy that makes an air unit's
/// weapons `Snapshot` keeps it hidden from the ground unit it strikes from over a plateau, which
/// the shipped `Visible` weapons reveal, in the same binary.
#[test]
fn an_edited_attack_visibility_hides_an_air_attacker() -> TestResult {
    let bundle = Bundle::new("attack-visibility")?;
    let shipped = bundle.load()?;
    let attacker = shipped
        .units
        .archetypes()
        .find(|arch| {
            arch.mover_type == sim_types::MoverType::Air
                && arch.max_speed > SimScalar::ZERO
                && arch
                    .weapon_against(sim_types::MoverType::Ground, None)
                    .is_some_and(|w| {
                        w.splash.is_none()
                            && matches!(
                                w.attack_visibility,
                                sim_types::AttackVisibility::Visible { .. }
                            )
                    })
        })
        .cloned()
        .ok_or("a shipped air unit with a Visible weapon against ground")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    let row = units
        .find(&format!("slug: \"{}\"", attacker.slug))
        .ok_or("the attacker's row")?;
    let end = units[row + 1..]
        .find("slug: \"")
        .map_or(units.len(), |next| row + 1 + next);
    let edited_row = units[row..end].replace(
        "attack_visibility: Visible(reveal_ticks: 54)",
        "attack_visibility: Snapshot",
    );
    std::fs::write(
        bundle.directory.join(FILES[0]),
        format!("{}{edited_row}{}", &units[..row], &units[end..]),
    )?;
    let edited = bundle.load()?;
    assert_eq!(
        edited
            .units
            .get(attacker.unit_type_id)?
            .weapon_against(sim_types::MoverType::Ground, None)
            .map(|w| w.attack_visibility),
        Some(sim_types::AttackVisibility::Snapshot),
        "premise: the edit reached the weapon"
    );
    assert!(victim_answers_from_below(shipped, attacker.unit_type_id)?);
    assert!(!victim_answers_from_below(edited, attacker.unit_type_id)?);
    bundle.finish()?;
    Ok(())
}

/// The first shipped mobile ground archetype whose ground weapon is a missile without an area,
/// and that weapon.
fn ground_missile_archetype(
    catalogs: &RuntimeCatalogs,
) -> Option<(sim_core::UnitArchetype, sim_core::WeaponSpec)> {
    catalogs
        .units
        .archetypes()
        .filter(|arch| {
            arch.mover_type == sim_types::MoverType::Ground && arch.max_speed > SimScalar::ZERO
        })
        .find_map(|arch| {
            arch.weapon_against(sim_types::MoverType::Ground, None)
                .filter(|w| w.projectile_speed.is_some() && w.splash.is_none())
                .map(|w| (arch.clone(), w.clone()))
        })
}

/// Authors `missile_validation: <flags>` on the ground missile of
/// [`ground_missile_archetype`] in `bundle`'s `units.ron`, before the row's first missile speed.
fn author_missile_validation(bundle: &Bundle, flags: &str) -> TestResult {
    let shipped = bundle.load()?;
    let (arch, _) = ground_missile_archetype(&shipped).ok_or("a ground missile is shipped")?;
    let units = std::fs::read_to_string(bundle.directory.join(FILES[0]))?;
    let row = units
        .find(&format!("slug: \"{}\"", arch.slug))
        .ok_or("the archetype's row")?;
    let field = row
        + units[row..]
            .find("projectile_speed: Some(")
            .ok_or("the row's missile speed")?;
    let edited = format!(
        "{}missile_validation: {flags},\n                {}",
        &units[..field],
        &units[field..]
    );
    std::fs::write(bundle.directory.join(FILES[0]), edited)?;
    Ok(())
}

/// Fires `catalogs`' ground missile at an unarmed unit that Blinks (a scratch `BlinkTeleport`
/// ability, none being shipped) while the missile flies; returns whether the unit was struck.
fn blinker_struck(catalogs: RuntimeCatalogs) -> Result<bool, Box<dyn std::error::Error>> {
    let (shooter_arch, _) =
        ground_missile_archetype(&catalogs).ok_or("a shipped weapon is a ground missile")?;
    let mut units = catalogs.units;
    let mut blinker = shooter_arch.clone();
    blinker.unit_type_id = UnitTypeId(
        units
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    "scratch_blinker".clone_into(&mut blinker.slug);
    blinker.weapon = None;
    blinker.ground_weapon = None;
    blinker.air_weapon = None;
    blinker.abilities = vec![sim_types::AbilitySpec {
        name: "scratch blink".into(),
        target_kind: sim_types::AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cast_range: SimScalar::from_i32(30),
        cooldown_ticks: 0,
        effect: sim_types::AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_i32(30),
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }];
    let blinker_type = blinker.unit_type_id;
    units.register(blinker);
    let mut world = SimWorld::with_catalogs(
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?,
        0xB23,
        units,
        catalogs.upgrades,
    )?;
    let shooter = world.spawn_unit(
        shooter_arch.unit_type_id,
        PlayerId(0),
        SimVec2::from_i32(12, 16),
    )?;
    let target = world.spawn_unit(blinker_type, PlayerId(1), SimVec2::from_i32(16, 16))?;
    let vitals = |world: &SimWorld| world.entities.get(&target).map(|t| (t.hp, t.shields));
    let full = vitals(&world);
    world
        .entities
        .get_mut(&shooter)
        .ok_or("spawned")?
        .issue_order(sim_types::UnitOrder::attack(target), false)
        .map_err(|error| format!("{error:?}"))?;
    let step = |world: &mut SimWorld, tick: u32| {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
    };
    let mut tick = 1;
    while world.projectiles.is_empty() {
        assert!(tick < 100, "the missile launched within 100 ticks");
        step(&mut world, tick);
        tick += 1;
    }
    assert!(
        world.execute_ability(target, 0, Some(SimVec2::from_i32(16, 19)), None),
        "the blink is admitted"
    );
    for later in tick..tick + 200 {
        step(&mut world, later);
        if world.projectiles.is_empty() {
            break;
        }
    }
    Ok(vitals(&world) != full)
}

/// P-2 (plan `2026-11-24-weapon-validation` D-1, D-4): authoring `missile_validation: (weapon:
/// true, teleport: false)` on the shipped ground missile makes it follow a Blink and strike, in
/// the same binary; the shipped default drops the blinker.
#[test]
fn an_authored_teleport_validation_off_follows_the_blink() -> TestResult {
    let bundle = Bundle::new("teleport-validation")?;
    let shipped = bundle.load()?;
    author_missile_validation(&bundle, "(weapon: true, teleport: false)")?;
    let edited = bundle.load()?;
    let (_, edited_weapon) =
        ground_missile_archetype(&edited).ok_or("the edited missile is still shipped")?;
    assert_eq!(
        edited_weapon.missile_validation,
        sim_types::MissileValidation {
            weapon: true,
            teleport: false,
        },
        "premise: the edit reached the ground missile"
    );
    assert!(
        !blinker_struck(shipped)?,
        "the shipped missile drops a target that blinks"
    );
    assert!(
        blinker_struck(edited)?,
        "the authored flag follows the blink and strikes"
    );
    bundle.finish()?;
    Ok(())
}

/// Fires `catalogs`' ground missile at an unarmed unit that cloaks, undetected, right after the
/// launch; returns whether the missile still homes on it one tick later.
fn missile_keeps_a_cloaked_target(
    catalogs: RuntimeCatalogs,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (shooter_arch, _) =
        ground_missile_archetype(&catalogs).ok_or("a shipped weapon is a ground missile")?;
    let mut units = catalogs.units;
    let mut dummy = shooter_arch.clone();
    dummy.unit_type_id = UnitTypeId(
        units
            .archetypes()
            .map(|a| a.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    );
    "scratch_unarmed_target".clone_into(&mut dummy.slug);
    dummy.weapon = None;
    dummy.ground_weapon = None;
    dummy.air_weapon = None;
    let dummy_type = dummy.unit_type_id;
    units.register(dummy);
    let mut world = SimWorld::with_catalogs(
        GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?,
        0xB33,
        units,
        catalogs.upgrades,
    )?;
    let shooter = world.spawn_unit(
        shooter_arch.unit_type_id,
        PlayerId(0),
        SimVec2::from_i32(10, 16),
    )?;
    let target = world.spawn_unit(dummy_type, PlayerId(1), SimVec2::from_i32(16, 16))?;
    world
        .entities
        .get_mut(&shooter)
        .ok_or("spawned")?
        .issue_order(sim_types::UnitOrder::attack(target), false)
        .map_err(|error| format!("{error:?}"))?;
    let step = |world: &mut SimWorld, tick: u32| {
        world.step(&sim_types::TickBundle {
            tick: sim_types::Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
    };
    let mut tick = 1;
    while world.projectiles.is_empty() {
        assert!(tick < 100, "the missile launched within 100 ticks");
        step(&mut world, tick);
        tick += 1;
    }
    world
        .entities
        .get_mut(&target)
        .ok_or("in flight")?
        .is_cloaked = true;
    step(&mut world, tick);
    Ok(world
        .projectiles
        .first()
        .is_some_and(|missile| missile.target_id == Some(target)))
}

/// P-4 (plan `2026-11-24-weapon-validation` D-1, D-4): authoring `missile_validation: (weapon:
/// false, teleport: true)`, the Brood Lord's flags, on the shipped ground missile makes it keep
/// homing on a target that cloaks undetected, in the same binary; the shipped default drops it.
#[test]
fn an_authored_weapon_validation_off_follows_a_cloaking_target() -> TestResult {
    let bundle = Bundle::new("weapon-validation")?;
    let shipped = bundle.load()?;
    author_missile_validation(&bundle, "(weapon: false, teleport: true)")?;
    let edited = bundle.load()?;
    let (_, edited_weapon) =
        ground_missile_archetype(&edited).ok_or("the edited missile is still shipped")?;
    assert!(
        !edited_weapon.missile_validation.weapon,
        "premise: the edit reached the ground missile"
    );
    assert!(
        !missile_keeps_a_cloaked_target(shipped)?,
        "the shipped missile drops a target that cloaks"
    );
    assert!(
        missile_keeps_a_cloaked_target(edited)?,
        "the authored flag keeps homing on it"
    );
    bundle.finish()?;
    Ok(())
}
