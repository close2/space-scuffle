//! Factions, weapon classes and armour classes are catalog data, not Rust types (9.3, F-13).
//!
//! # Responsibility
//! Proves through the public loading API that a faction is defined only by
//! `assets/catalog/factions.ron`: a bundle with a fourth faction loads with no code change and its
//! builder raises its own structure, while a unit, structure or upgrade naming an unknown faction
//! or class is rejected with a typed error that names the referring definition.
//!
//! # How it fits
//! Each test copies the shipped catalog directory into a private temporary directory, edits the
//! copy as an author would, and loads it with [`RuntimeCatalogs::load_from_directory`], the same
//! entry point the client and the asset baker use.
//!
//! # Invariants
//! - The shipped files are never modified; every edit happens on a temporary copy.
//! - Slugs are read from the data; no test names a faction or class in Rust beyond the planted
//!   test-only identities.

use sim_core::catalog::{CatalogLoadError, RuntimeCatalogs};
use sim_core::economy::GameCommandReceipt;
use sim_core::{SimCoreError, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    PlayerCommand, PlayerId, PlayerSlot, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
    UnitTypeId,
};
use std::error::Error;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn Error>>;

const FILES: [&str; 4] = [
    "factions.ron",
    "units.ron",
    "structures.ron",
    "upgrades.ron",
];
const ME: PlayerId = PlayerId(0);

/// A private copy of the shipped catalog directory, removed by [`Bundle::finish`].
struct Bundle {
    directory: PathBuf,
}

impl Bundle {
    fn new(label: &str) -> Result<Self, std::io::Error> {
        let directory = std::env::temp_dir().join(format!(
            "space-scuffle-factions-{}-{label}",
            std::process::id()
        ));
        if directory.exists() {
            std::fs::remove_dir_all(&directory)?;
        }
        std::fs::create_dir(&directory)?;
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        for file in FILES {
            std::fs::copy(shipped.join(file), directory.join(file))?;
        }
        Ok(Self { directory })
    }

    fn read(&self, file: &str) -> Result<String, std::io::Error> {
        std::fs::read_to_string(self.directory.join(file))
    }

    fn write(&self, file: &str, text: &str) -> Result<(), std::io::Error> {
        std::fs::write(self.directory.join(file), text)
    }

    fn load(&self) -> Result<RuntimeCatalogs, CatalogLoadError> {
        RuntimeCatalogs::load_from_directory(&self.directory)
    }

    fn finish(self) -> Result<(), std::io::Error> {
        std::fs::remove_dir_all(self.directory)
    }
}

/// Index of the `)` closing the `(` at `open`.
fn closing(source: &str, open: usize) -> Result<usize, String> {
    let mut depth = 0_u32;
    let mut quoted = false;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Ok(open + offset);
                }
            }
            _ => {}
        }
    }
    Err(format!("unbalanced definition at byte {open}"))
}

/// The complete `( ... )` definition whose own `slug` field is `slug`.
fn definition<'a>(source: &'a str, slug: &str) -> Result<&'a str, String> {
    let marker = source
        .find(&format!("slug: \"{slug}\""))
        .ok_or_else(|| format!("no definition with slug {slug}"))?;
    let mut depth = 0_i32;
    let mut open = None;
    for (index, character) in source[..marker].char_indices().rev() {
        match character {
            ')' | ']' => depth += 1,
            '(' | '[' if depth == 0 => {
                open = Some(index);
                break;
            }
            '(' | '[' => depth -= 1,
            _ => {}
        }
    }
    let open = open.ok_or_else(|| format!("definition of {slug} has no opening parenthesis"))?;
    Ok(&source[open..=closing(source, open)?])
}

/// Replaces the value of the first top-level `field:` of `definition` with `value`.
fn with_field(definition: &str, field: &str, value: &str) -> Result<String, String> {
    let marker = format!("\n            {field}:");
    let start = definition
        .find(&marker)
        .ok_or_else(|| format!("definition has no top-level {field}"))?
        + marker.len();
    let end = start
        + definition[start..]
            .find(",\n")
            .ok_or_else(|| format!("{field} has no terminating comma"))?;
    Ok(format!(
        "{} {value}{}",
        &definition[..start],
        &definition[end..]
    ))
}

/// `source` with `addition` appended to the root list `list:`, which must close last.
fn append_to_list(source: &str, list: &str, addition: &str) -> Result<String, String> {
    let start = source
        .find(&format!("{list}:"))
        .ok_or_else(|| format!("no list {list}"))?;
    let open = start
        + source[start..]
            .find('[')
            .ok_or_else(|| format!("{list} is not a list"))?;
    let close = closing(source, open)?;
    Ok(format!(
        "{}{addition},\n{}",
        &source[..close],
        &source[close..]
    ))
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
}

/// Slug of the first catalog definition in `source` whose fields include `needle`.
fn first_slug_with<'a>(source: &'a str, needle: &str) -> Result<&'a str, String> {
    let at = source
        .find(needle)
        .ok_or_else(|| format!("no definition contains {needle}"))?;
    let slug_at = source[..at]
        .rfind("slug: \"")
        .ok_or("definition has no slug before the field")?
        + "slug: \"".len();
    let end = slug_at + source[slug_at..].find('"').ok_or("unterminated slug")?;
    Ok(&source[slug_at..end])
}

/// Red run (a) of WP12: a fourth faction is data only, and its builder raises its own structure.
#[test]
fn a_fourth_faction_added_in_data_builds_its_own_structure() -> TestResult {
    let bundle = Bundle::new("fourth")?;
    let factions = append_to_list(
        &bundle.read("factions.ron")?,
        "factions",
        r#"(
            faction_id: 3,
            slug: "test_fourth_faction",
            display_name: "Test Fourth Faction",
            accent_srgb: (200, 200, 40),
            lore: "Exists only in this test's temporary catalog.",
        )"#,
    )?;
    bundle.write("factions.ron", &factions)?;

    let units = bundle.read("units.ron")?;
    let builder_slug = first_slug_with(&units, "can_construct: true")?.to_owned();
    let builder = definition(&units, &builder_slug)?;
    let builder = with_field(builder, "slug", "\"test_fourth_builder\"")?;
    let builder = with_field(&builder, "unit_type_id", "200")?;
    let builder = with_field(&builder, "faction", "\"test_fourth_faction\"")?;
    let builder = with_field(&builder, "display_name", "\"Test Fourth Builder\"")?;
    bundle.write("units.ron", &append_to_list(&units, "units", &builder)?)?;

    let structures = bundle.read("structures.ron")?;
    let shipped_dome = first_slug_with(&structures, "requires_structures: [],")?.to_owned();
    let dome = definition(&structures, &shipped_dome)?;
    let dome = with_field(dome, "structure_type_id", "201")?;
    let dome = with_field(&dome, "slug", "\"test_fourth_dome\"")?;
    let dome = with_field(&dome, "faction", "\"test_fourth_faction\"")?;
    let dome = with_field(&dome, "display_name", "\"Test Fourth Dome\"")?;
    bundle.write(
        "structures.ron",
        &append_to_list(&structures, "structures", &dome)?,
    )?;

    let catalogs = bundle.load()?;
    let builder = catalogs
        .units
        .get_by_slug("test_fourth_builder")
        .ok_or("fourth builder loaded")?;
    let dome = catalogs
        .units
        .get_by_slug("test_fourth_dome")
        .ok_or("fourth dome loaded")?;
    let shipped = catalogs
        .units
        .get_by_slug(&shipped_dome)
        .ok_or("shipped dome loaded")?;
    assert!(builder.can_raise(dome));
    assert!(
        !builder.can_raise(shipped),
        "a builder raises only its own faction's structures"
    );
    let (builder, dome): (UnitTypeId, UnitTypeId) = (builder.unit_type_id, dome.unit_type_id);

    let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(map, 11, catalogs.units, catalogs.upgrades);
    let worker = world.spawn_unit(builder, ME, at(12, 12))?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 1000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 1000;
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: ME,
            entities: vec![worker],
            order: UnitOrder::Build {
                structure_type: dome,
                cell_x: 20,
                cell_y: 12,
            },
            queue: false,
        }],
    );
    assert!(matches!(
        world.game_command_outcomes()[0].result,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    ));
    let laid = (0..600).any(|_| {
        step(&mut world, Vec::new());
        world.entities.values().any(|e| e.unit_type_id == dome)
    });
    assert!(laid, "the fourth faction's builder lays its own foundation");
    bundle.finish()?;
    Ok(())
}

/// `text` with the value of the first field starting `needle` (for example `faction: "`)
/// replaced by `value`, and the slug of the definition holding it.
fn edit_first(text: &str, needle: &str, value: &str) -> Result<(String, String), String> {
    let slug = first_slug_with(text, needle)?.to_owned();
    let at = text.find(needle).ok_or("needle vanished")?;
    let start = at + text[at..].find(':').ok_or("field has no colon")? + 1;
    let end = start
        + text[start..]
            .find(",\n")
            .ok_or("field has no terminating comma")?;
    Ok((format!("{} {value}{}", &text[..start], &text[end..]), slug))
}

/// Loads `bundle` after editing `file` with [`edit_first`]; returns the failing file, the typed
/// error and the slug of the edited definition.
fn rejected(
    label: &str,
    file: &str,
    needle: &str,
    value: &str,
) -> Result<(PathBuf, SimCoreError, String), Box<dyn Error>> {
    let bundle = Bundle::new(label)?;
    let (edited, slug) = edit_first(&bundle.read(file)?, needle, value)?;
    bundle.write(file, &edited)?;
    let outcome = bundle.load();
    bundle.finish()?;
    match outcome {
        Err(CatalogLoadError::Invalid { path, source }) => Ok((path, source, slug)),
        Err(other) => Err(format!("expected an invalid catalog, got {other}").into()),
        Ok(_) => Err(format!("{label}: the edited catalog must not load").into()),
    }
}

/// Red run (c) of WP12: an unknown armour class is rejected and the error names the unit.
#[test]
fn a_unit_naming_an_unknown_armor_class_is_rejected_naming_the_unit() -> TestResult {
    let (path, error, unit) = rejected(
        "armor",
        "units.ron",
        "armor_class: Some(",
        "Some(\"no_such_armor\")",
    )?;
    assert!(path.ends_with("units.ron"), "{}", path.display());
    assert_eq!(
        error,
        SimCoreError::UnknownArmorClass {
            definition: unit,
            slug: "no_such_armor".to_owned()
        }
    );
    Ok(())
}

#[test]
fn unknown_factions_are_rejected_in_the_file_and_definition_that_name_them() -> TestResult {
    for (file, label) in [
        ("units.ron", "unit-faction"),
        ("structures.ron", "structure-faction"),
        ("upgrades.ron", "upgrade-faction"),
    ] {
        let (path, error, definition) =
            rejected(label, file, "faction: \"", "\"no_such_faction\"")?;
        assert!(path.ends_with(file), "{}", path.display());
        assert_eq!(
            error,
            SimCoreError::UnknownFaction {
                definition,
                slug: "no_such_faction".to_owned()
            },
            "{file}"
        );
    }
    Ok(())
}

#[test]
fn unknown_weapon_classes_are_rejected_on_weapons_and_upgrade_effects() -> TestResult {
    for (file, label, needle, value) in [
        (
            "units.ron",
            "unit-weapon",
            "weapon_class: Some(",
            "Some(\"no_such_weapons\")",
        ),
        (
            "upgrades.ron",
            "upgrade-weapon",
            "weapon_class: \"",
            "\"no_such_weapons\"",
        ),
    ] {
        let (path, error, definition) = rejected(label, file, needle, value)?;
        assert!(path.ends_with(file), "{}", path.display());
        assert_eq!(
            error,
            SimCoreError::UnknownWeaponClass {
                definition,
                slug: "no_such_weapons".to_owned()
            },
            "{file}"
        );
    }
    let (path, error, upgrade) = rejected(
        "upgrade-armor",
        "upgrades.ron",
        "armor_class: \"",
        "\"no_such_armor\"",
    )?;
    assert!(path.ends_with("upgrades.ron"), "{}", path.display());
    assert_eq!(
        error,
        SimCoreError::UnknownArmorClass {
            definition: upgrade,
            slug: "no_such_armor".to_owned()
        }
    );
    Ok(())
}

#[test]
fn an_inconsistent_or_missing_factions_file_fails_with_its_own_path() -> TestResult {
    let bundle = Bundle::new("class-faction")?;
    let text = bundle.read("factions.ron")?;
    let class = "slug: \"directorate_infantry_weapons\", faction: \"";
    if !text.contains(class) {
        return Err("factions.ron no longer has the edited class".into());
    }
    bundle.write(
        "factions.ron",
        &text.replacen(class, &format!("{class}no_such_faction_"), 1),
    )?;
    let outcome = bundle.load();
    bundle.finish()?;
    let Err(CatalogLoadError::Invalid { path, source }) = outcome else {
        return Err(format!("expected an invalid factions.ron, got {outcome:?}").into());
    };
    assert!(path.ends_with("factions.ron"), "{}", path.display());
    assert!(
        matches!(&source, SimCoreError::UnknownFaction { definition, slug }
            if definition == "directorate_infantry_weapons" && slug.starts_with("no_such_faction_")),
        "{source}"
    );

    let bundle = Bundle::new("missing")?;
    std::fs::remove_file(bundle.directory.join("factions.ron"))?;
    let outcome = bundle.load();
    bundle.finish()?;
    assert!(
        matches!(&outcome, Err(CatalogLoadError::Io { path, .. }) if path.ends_with("factions.ron")),
        "{outcome:?}"
    );
    Ok(())
}

#[test]
fn the_shipped_catalog_resolves_every_reference_and_keeps_the_identities() -> TestResult {
    let bundle = Bundle::new("shipped")?;
    let catalogs = bundle.load()?;
    bundle.finish()?;
    let factions = catalogs.units.factions();
    for archetype in catalogs.units.archetypes() {
        assert!(
            factions.faction(archetype.faction).is_some(),
            "{}",
            archetype.slug
        );
        if let Some(class) = archetype.armor_class {
            let class = factions.armor_class(class).ok_or("armor class resolves")?;
            assert_eq!(class.faction, archetype.faction, "{}", archetype.slug);
        }
        for weapon in [
            &archetype.ground_weapon,
            &archetype.air_weapon,
            &archetype.weapon,
        ]
        .into_iter()
        .flatten()
        {
            if let Some(class) = weapon.weapon_class {
                assert!(factions.weapon_class(class).is_some(), "{}", archetype.slug);
            }
        }
    }
    for upgrade in catalogs.upgrades.upgrades() {
        assert!(
            factions.faction(upgrade.faction).is_some(),
            "{}",
            upgrade.slug
        );
    }
    Ok(())
}
