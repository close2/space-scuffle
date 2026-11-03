//! The baker reads its unit catalog from disk at run time.
//!
//! # Responsibility
//! Proves charter §3.3's "adding a unit needs zero recompilation" for the asset baker: a unit
//! written into a copy of the shipped catalog *after* the baker binary was built is discovered by
//! the library and synthesised by that same binary.
//!
//! # How it fits
//! `sim_core/tests/runtime_catalog_tests.rs` proves the same for the simulation's loader. This
//! test covers the baker, which used to compile the catalog in.
//!
//! # Invariants
//! - The new unit is derived from the shipped data (a copy of its first unit under a fresh slug
//!   and ID), so no unit identifier is written in this file.
//! - The binary under test is the one Cargo built before the test ran; the test never builds.

use asset_baker::{default_unit_archetypes, load_catalogs, resolve_assets_root};
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shipped asset bundle.
fn shipped_assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// A unit added to a copy of the shipped catalog.
struct AddedUnit {
    slug: String,
    display_name: String,
}

/// Copies the shipped rule catalogs into `root/catalog` and appends a copy of the first unit of
/// `units.ron` under a fresh slug, ID and display name.
fn bundle_with_added_unit(root: &Path) -> Result<AddedUnit, Box<dyn Error>> {
    let catalog = root.join("catalog");
    fs::create_dir_all(&catalog)?;
    for file in ["factions.ron", "structures.ron", "upgrades.ron"] {
        fs::copy(
            shipped_assets().join("catalog").join(file),
            catalog.join(file),
        )?;
    }
    let units = fs::read_to_string(shipped_assets().join("catalog/units.ron"))?;

    // The first unit entry runs from the first `(` line under `units: [` to the next one.
    let entry_open = "\n        (\n";
    let list = units.find("units: [").ok_or("units.ron has a units list")?;
    let first = list + units[list..].find(entry_open).ok_or("a first unit")? + 1;
    let second = first + 1 + units[first + 1..].find(entry_open).ok_or("a second unit")? + 1;
    let template = &units[first..second];

    let field = |name: &str| -> Result<String, Box<dyn Error>> {
        let start = template
            .find(&format!("{name}: "))
            .ok_or(format!("field {name}"))?;
        let line = &template[start..];
        let end = line.find(",\n").ok_or(format!("end of field {name}"))?;
        Ok(line[..end].to_string())
    };
    let used: BTreeSet<u16> = sim_fixtures::shipped_catalogs()
        .units
        .archetypes()
        .map(|a| a.unit_type_id.0)
        .collect();
    let id = (1..128u16)
        .find(|id| !used.contains(id))
        .ok_or("a free unit type ID")?;
    let template_slug = field("slug")?;
    let slug = format!(
        "{}_runtime_copy",
        template_slug
            .trim_start_matches("slug: \"")
            .trim_end_matches('"')
    );
    let display_name = format!("Runtime Copy {id}");
    let added = template
        .replacen(&template_slug, &format!("slug: \"{slug}\""), 1)
        .replacen(&field("unit_type_id")?, &format!("unit_type_id: {id}"), 1)
        .replacen(
            &field("display_name")?,
            &format!("display_name: \"{display_name}\""),
            1,
        );
    let mut edited = units.clone();
    edited.insert_str(second, &added);
    fs::write(catalog.join("units.ron"), edited)?;
    Ok(AddedUnit { slug, display_name })
}

#[test]
fn the_library_discovers_a_unit_added_to_the_catalog_files() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let assets = dir.path().join("assets");
    let added = bundle_with_added_unit(&assets)?;

    let root = resolve_assets_root(Some(&assets))?;
    let catalogs = load_catalogs(&root)?;
    assert!(default_unit_archetypes(&catalogs.units).contains(&added.slug));
    assert_eq!(
        catalogs
            .units
            .get_by_slug(&added.slug)
            .map(|a| a.display_name.as_str()),
        Some(added.display_name.as_str())
    );
    let style = asset_baker::MediaStyle::load(&shipped_assets())?;
    let prompt = asset_baker::build_action_prompt(
        &catalogs.units,
        &style,
        &added.slug,
        sim_types::enums::AnimState::Idle,
    )?;
    assert!(prompt.text.contains(&added.display_name) || prompt.authored);

    let shipped = sim_fixtures::shipped_catalogs().units;
    assert!(!default_unit_archetypes(&shipped).contains(&added.slug));
    Ok(())
}

#[test]
fn the_built_binary_bakes_a_unit_added_after_it_was_compiled() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let assets = dir.path().join("assets");
    let added = bundle_with_added_unit(&assets)?;
    let output_dir = dir.path().join("fixtures");
    let bake = |bundle: &Path| {
        Command::new(env!("CARGO_BIN_EXE_asset_baker"))
            .arg("--assets")
            .arg(bundle)
            .args(["procedural", "--size", "8", "--archetype", &added.slug])
            .arg("--output-dir")
            .arg(&output_dir)
            .output()
    };

    let refused = bake(&shipped_assets())?;
    assert!(
        !refused.status.success(),
        "the shipped catalog has no unit {:?}",
        added.slug
    );
    assert!(!output_dir.join(format!("{}.png", added.slug)).exists());

    let baked = bake(&assets)?;
    assert!(
        baked.status.success(),
        "{}",
        String::from_utf8_lossy(&baked.stderr)
    );
    assert!(output_dir.join(format!("{}.png", added.slug)).is_file());
    assert!(output_dir
        .join(format!("{}.flipbook.ron", added.slug))
        .is_file());
    Ok(())
}
