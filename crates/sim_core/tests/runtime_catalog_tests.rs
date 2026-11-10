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
const FILES: [&str; 5] = [
    "units.ron",
    "structures.ron",
    "upgrades.ron",
    "movement_rules.ron",
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
    let mut old_world = SimWorld::with_catalogs(empty_map()?, 7, original.units, original.upgrades);
    let mut new_world = SimWorld::with_catalogs(empty_map()?, 7, modified.units, modified.upgrades);
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
