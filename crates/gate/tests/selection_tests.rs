//! Behaviour of the selection on the real catalog and workspace, through the public API.
//!
//! # Responsibility
//! Pins the tier semantics of blueprint §3.3 to concrete changes of this repository: a simulation
//! change, an `asset_baker` change, a headless client change, a renderer change, a shader change,
//! a support-file change, a docs-only change and a workspace-wide change. Each test names the
//! targets that must and must not be selected, so a catalog edit that silently drops a contract
//! fails here.
//!
//! # How it fits
//! `suite_catalog_tests.rs` (check C22) proves the catalog is consistent with the repository;
//! this file proves it selects what the blueprint promises. Both read the checkout they were
//! built from and start no test themselves.
//!
//! # Invariants
//! - The paths used are real files of the repository; the tests check that before selecting.

use gate::{
    select, ClippyScope, CompleteCause, GateError, Inputs, Reason, Selection, SourceIndex,
    SuiteCatalog, TargetId, TestClass, Tier, Workspace,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The repository root, two levels above this crate.
fn root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .map_or_else(|| manifest.to_path_buf(), Path::to_path_buf)
}

/// The real catalog, workspace and sources.
struct Repo {
    catalog: SuiteCatalog,
    workspace: Workspace,
    sources: SourceIndex,
}

impl Repo {
    fn load() -> Result<Self, GateError> {
        let root = root();
        let workspace = Workspace::load(&root)?;
        Ok(Self {
            catalog: SuiteCatalog::load(&root)?,
            sources: SourceIndex::build(&root, &workspace)?,
            workspace,
        })
    }

    /// The selection of `tier` for `paths`, which must exist in the checkout.
    fn select(&self, tier: Tier, paths: &[&str]) -> Result<Selection, GateError> {
        for path in paths {
            assert!(root().join(path).exists(), "{path} must be a real file");
        }
        let inputs = Inputs {
            catalog: &self.catalog,
            workspace: &self.workspace,
            sources: &self.sources,
        };
        let changed = paths.iter().map(ToString::to_string).collect();
        select(tier, inputs, &changed)
    }
}

/// The target named `crate/name` as written in the catalog.
fn id(text: &str) -> TargetId {
    match text.split_once('/') {
        Some((package, "lib")) => TargetId::lib(package),
        Some((package, name)) => TargetId::test(package, name),
        None => TargetId::lib(text),
    }
}

fn has(selection: &Selection, target: &str) -> bool {
    selection.targets.contains_key(&id(target))
}

fn count(selection: &Selection, class: TestClass) -> usize {
    selection
        .targets
        .values()
        .filter(|s| s.class == class)
        .count()
}

#[test]
fn a_simulation_change_runs_its_crate_contracts_and_goldens_but_no_client_renderer(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/sim_core/src/lib.rs"])?;
    assert!(selection.complete.is_none());
    for wanted in [
        "sim_core/lib",
        "sim_core/golden_replay_tests",
        "sim_core/sc2_who_wins_matchup_matrix_tests",
        "sim_fixtures/lib",
        "asset_baker/runtime_catalog_tests",
        "client/skirmish_match_tests",
        "client/battle_result_tests",
        "workspace_checks/charter",
    ] {
        assert!(has(&selection, wanted), "{wanted} must be selected");
    }
    for unwanted in [
        "client/lib",
        "client/gpu_validation_tests",
        "sim_map/lib",
        "client/sync_allocation_count_tests",
    ] {
        assert!(
            !has(&selection, unwanted),
            "{unwanted} must not be selected"
        );
    }
    for command in [
        "golden-replay-release",
        "golden-replay-wasm32-wasip1",
        "check-wasm32-unknown-unknown",
    ] {
        assert!(
            selection.commands.contains_key(command),
            "{command} must run"
        );
    }
    assert_eq!(selection.clippy, ClippyScope::Workspace);
    Ok(())
}

#[test]
fn an_asset_baker_change_runs_its_crate_and_the_client_media_contracts() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/asset_baker/src/manifest.rs"])?;
    for wanted in [
        "asset_baker/lib",
        "asset_baker/procedural_tests",
        "client/runtime_media_tests",
        "client/lab_scene_tests",
        "client_diagnostics/palette_fixture_tests",
    ] {
        assert!(has(&selection, wanted), "{wanted} must be selected");
    }
    assert!(!has(&selection, "sim_core/lib"));
    assert!(!has(&selection, "client/lib"));
    assert!(
        selection.commands.is_empty(),
        "no golden replay for media code"
    );
    Ok(())
}

#[test]
fn a_headless_client_change_runs_the_battle_drivers_but_no_gpu_binary() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/client/src/input_policy.rs"])?;
    assert!(has(&selection, "client/lib"));
    assert!(has(&selection, "client/battle_input_tests"));
    assert!(
        has(&selection, "client_diagnostics/runtime_capture_tests"),
        "downstream fast"
    );
    assert_eq!(count(&selection, TestClass::Gpu), 0, "{selection:#?}");
    assert_eq!(count(&selection, TestClass::Valgrind), 0);
    Ok(())
}

#[test]
fn a_renderer_change_runs_every_client_gpu_binary_and_the_frame_budget() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/client/src/bubble_shader.rs"])?;
    assert!(has(&selection, "client/gpu_validation_tests"));
    assert!(has(&selection, "client/terrain_tests"));
    assert!(
        has(&selection, "client_diagnostics/frame_budget_tests"),
        "by trigger"
    );
    assert_eq!(
        count(&selection, TestClass::Gpu),
        19,
        "16 client GPU binaries and 3 diagnostics"
    );
    assert!(!has(&selection, "client/sync_allocation_count_tests"));
    Ok(())
}

#[test]
fn a_live_sync_change_runs_valgrind_and_the_skirmish() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/client/src/presentation.rs"])?;
    assert!(has(&selection, "client/sync_allocation_count_tests"));
    assert!(has(&selection, "client/skirmish_match_tests"));
    assert!(has(&selection, "client/runtime_battle_tests"));
    assert!(!has(&selection, "client/gpu_validation_tests"));
    Ok(())
}

#[test]
fn a_gpu_support_file_selects_every_target_that_loads_it() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/client/tests/support/gpu_run.rs"])?;
    assert_eq!(count(&selection, TestClass::Gpu), 19);
    let harness = repo.select(
        Tier::Package,
        &["crates/client/tests/support/battle_harness.rs"],
    )?;
    assert!(
        has(&harness, "client/skirmish_match_tests"),
        "a gpu target that loads it"
    );
    assert!(
        !has(&harness, "client/gpu_validation_tests"),
        "a gpu target that does not"
    );
    Ok(())
}

#[test]
fn a_shader_change_runs_the_gpu_binaries_without_any_crate_change() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["assets/shaders/bubble_dome.wgsl"])?;
    assert!(selection.changed_crates.is_empty());
    assert_eq!(count(&selection, TestClass::Gpu), 19);
    assert!(has(&selection, "client/headless_tests"));
    assert!(!has(&selection, "client/lib"));
    Ok(())
}

#[test]
fn a_catalog_change_runs_the_simulation_and_the_goldens() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["assets/catalog/units.ron"])?;
    assert!(has(&selection, "sim_core/full_match_tests"));
    assert!(has(&selection, "sim_fixtures/fixture_tests"));
    assert!(has(&selection, "client/battle_card_tests"));
    assert!(selection.commands.contains_key("golden-replay-release"));
    Ok(())
}

#[test]
fn docs_only_runs_the_charter_checks_alone() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(
        Tier::Package,
        &["docs/plans/2026-11-04-tiered-test-gates.md", "AGENTS.md"],
    )?;
    assert!(selection.docs_only);
    assert_eq!(selection.clippy, ClippyScope::Skip);
    let crates: BTreeSet<String> = selection.crates();
    assert_eq!(crates, BTreeSet::from(["workspace_checks".to_string()]));
    Ok(())
}

#[test]
fn a_workspace_input_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["Cargo.lock"])?;
    assert!(matches!(
        selection.complete,
        Some(CompleteCause::Global { .. })
    ));
    assert_eq!(
        selection.targets.len(),
        repo.workspace.test_targets().count()
    );
    Ok(())
}

#[test]
fn the_sim_fixtures_contract_selects_its_callers_without_a_list() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/sim_fixtures/src/lib.rs"])?;
    assert!(selection.complete.is_none());
    let callers = Reason::Callers("sim_fixtures".to_string());
    for wanted in [
        "sim_core/golden_replay_tests",
        "client/lib",
        "asset_baker/veo_tests",
    ] {
        let reasons = selection.targets.get(&id(wanted)).map(|s| &s.reasons);
        assert!(
            reasons.is_some_and(|r| r.contains(&callers)),
            "{wanted} must be selected as a caller: {reasons:?}"
        );
    }
    assert!(!has(&selection, "client/gpu_validation_tests"));
    Ok(())
}

#[test]
fn quick_keeps_only_the_changed_crates_fast_targets() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Quick, &["crates/sim_core/src/lib.rs"])?;
    assert!(has(&selection, "sim_core/lib"));
    assert!(
        !has(&selection, "sim_core/sc2_who_wins_matchup_matrix_tests"),
        "slow"
    );
    assert!(
        !has(&selection, "client/skirmish_match_tests"),
        "downstream"
    );
    assert!(selection.commands.is_empty());
    assert_eq!(
        selection.clippy,
        ClippyScope::Packages(BTreeSet::from(["sim_core".to_string()]))
    );
    Ok(())
}
