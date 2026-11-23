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
         harvest_time_ticks: 50, harvest_amount: 5, return_delay_ticks: 0),{}",
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
    std::fs::write(
        &path,
        text.replacen("impact: Attacker", "impact: Target", 1),
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
            if !world.entities.get(&baneling).is_some_and(|b| b.is_alive()) {
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
