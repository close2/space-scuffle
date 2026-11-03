//! Ensures exact scalar persistence retains compatibility with the real numeric catalogs.
//!
//! This public-API boundary test reads the shipped files without embedding or rewriting
//! them, so serialization changes cannot silently require a catalog migration.

use sim_core::catalog::UnitCatalog;

#[test]
fn checked_in_numeric_catalogs_load() -> Result<(), Box<dyn std::error::Error>> {
    let catalog_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
    let units = std::fs::read_to_string(catalog_dir.join("units.ron"))?;
    let structures = std::fs::read_to_string(catalog_dir.join("structures.ron"))?;
    let catalog =
        UnitCatalog::from_ron_str_bundle(&sim_fixtures::shipped_factions(), &units, &structures)?;
    assert!(catalog.archetypes().next().is_some());
    Ok(())
}
