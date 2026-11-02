//! Verifies the sparse-owner capture input through the real map and scenario loaders.

use client::runtime_battle::BattleDefinition;
use client_diagnostics::palette_fixture;
use sim_types::PlayerId;

#[test]
fn sparse_native_fixture_roundtrips_all_nonownership_data_and_rejects_reuse(
) -> Result<(), Box<dyn std::error::Error>> {
    let temporary = tempfile::tempdir()?;
    let output = temporary.path().join("fresh");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let fixture = palette_fixture::prepare(&root, &output)?;
    let loaded = BattleDefinition::load(&root, &fixture.scenario)?;
    assert_eq!(
        loaded
            .scenario
            .players
            .iter()
            .map(|p| p.player_slot)
            .collect::<Vec<_>>(),
        vec![2, 7]
    );
    let world = loaded.instantiate()?;
    assert!(world
        .entities
        .values()
        .all(|actor| [PlayerId(2), PlayerId(7)].contains(&actor.owner)));
    assert!(palette_fixture::prepare(&root, &output).is_err());
    Ok(())
}
