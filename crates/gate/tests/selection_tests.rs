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
    select, select_change, Change, ClippyScope, CompleteCause, FileVersions, GateError, Inputs,
    Reason, Selection, SourceIndex, SuiteCatalog, TargetId, TestClass, Tier, Workspace,
    CATALOG_PATH, LOCKFILE_PATH,
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

    /// The selection of `tier` for `paths`, with both versions of the files in `contents`.
    fn select_change<const N: usize>(
        &self,
        tier: Tier,
        paths: &[&str],
        contents: [(&str, FileVersions); N],
    ) -> Result<Selection, GateError> {
        for path in paths {
            assert!(root().join(path).exists(), "{path} must be a real file");
        }
        let inputs = Inputs {
            catalog: &self.catalog,
            workspace: &self.workspace,
            sources: &self.sources,
        };
        let change = Change {
            paths: paths.iter().map(ToString::to_string).collect(),
            contents: contents
                .into_iter()
                .map(|(path, versions)| (path.to_string(), versions))
                .collect(),
        };
        select_change(tier, inputs, &change)
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
        21,
        "18 client GPU binaries and 3 diagnostics"
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
    assert_eq!(count(&selection, TestClass::Gpu), 21);
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
    assert_eq!(count(&selection, TestClass::Gpu), 21);
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
    let selection = repo.select(Tier::Package, &["Cargo.toml"])?;
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

/// The real text of `path` in this checkout.
fn real_text(path: &str) -> Result<String, GateError> {
    let full = root().join(path);
    std::fs::read_to_string(&full).map_err(|source| GateError::Io { path: full, source })
}

/// `text` with `from` (which must occur exactly once) replaced by `to`.
fn edited(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?} must occur once");
    text.replacen(from, to, 1)
}

/// The catalog's selection when its base version was `before` and its version now is the
/// real file.
fn catalog_edit(repo: &Repo, before: Option<String>) -> Result<Selection, GateError> {
    let after = Some(real_text(CATALOG_PATH)?);
    repo.select_change(
        Tier::Package,
        &[CATALOG_PATH],
        [(CATALOG_PATH, FileVersions { before, after })],
    )
}

/// The crates of the selected targets.
fn crates(selection: &Selection) -> BTreeSet<String> {
    selection.crates()
}

/// The crates a catalog edit selects whatever it changes: the gate's own and the charter checks.
fn gate_and_checks() -> BTreeSet<String> {
    BTreeSet::from(["gate".to_string(), "workspace_checks".to_string()])
}

#[test]
fn a_new_downstream_reference_in_the_catalog_selects_that_target_not_the_complete_suite(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let before = edited(
        &real_text(CATALOG_PATH)?,
        "            // The client proof of cast admission (`would_accept_cast_order`, `cast_states`).\n            \"client/battle_cast_tests\",\n",
        "",
    );
    let selection = catalog_edit(&repo, Some(before))?;
    assert!(selection.complete.is_none(), "{:?}", selection.complete);
    let reasons = &selection.targets[&id("client/battle_cast_tests")].reasons;
    assert!(reasons.contains(&Reason::Catalog(
        "downstream `sim_core` gained it".to_string()
    )));
    let mut expected = gate_and_checks();
    expected.insert("client".to_string());
    assert_eq!(crates(&selection), expected);
    assert!(!has(&selection, "client/lib"));
    assert!(has(&selection, "gate/suite_catalog_tests"));
    Ok(())
}

#[test]
fn a_comment_only_catalog_edit_selects_only_the_gate_crate_and_the_charter_checks(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let before = edited(
        &real_text(CATALOG_PATH)?,
        "        // The effect blocks (plan 2026-11-09-effect-visualization WP4): one child, eight shots.\n",
        "",
    );
    let selection = catalog_edit(&repo, Some(before))?;
    assert!(selection.complete.is_none());
    assert_eq!(crates(&selection), gate_and_checks());
    assert!(selection.commands.is_empty());
    Ok(())
}

#[test]
fn a_test_threads_change_selects_only_that_target_and_the_gate_crate() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let before = edited(
        &real_text(CATALOG_PATH)?,
        "\"client/editor_app_tests\": (class: Gpu, test_threads: Some(2)),",
        "\"client/editor_app_tests\": (class: Gpu, test_threads: Some(3)),",
    );
    let selection = catalog_edit(&repo, Some(before))?;
    assert!(selection.complete.is_none());
    let client: Vec<&TargetId> = selection
        .targets
        .keys()
        .filter(|id| id.package == "client")
        .collect();
    assert_eq!(client, [&id("client/editor_app_tests")]);
    let path = &selection.paths[0];
    assert!(
        path.covered_by
            .iter()
            .any(|line| line.contains("client/editor_app_tests")
                && line.contains("test_threads 3")
                && line.contains("test_threads 2")),
        "{path:?}"
    );
    Ok(())
}

#[test]
fn a_rule_edit_selects_the_old_and_new_rule_targets() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let before = edited(
        &real_text(CATALOG_PATH)?,
        "                \"client/battle_result_window_tests\",\n",
        "",
    );
    let selection = catalog_edit(&repo, Some(before))?;
    assert!(selection.complete.is_none());
    for wanted in [
        "client/battle_hud_window_tests",
        "client/battle_result_window_tests",
        "client/concealment_gpu_tests",
    ] {
        assert!(has(&selection, wanted), "{wanted} must be selected");
    }
    assert!(!has(&selection, "client/lib"));
    Ok(())
}

#[test]
fn a_global_or_docs_catalog_edit_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let text = real_text(CATALOG_PATH)?;
    for before in [
        edited(&text, "        \".gitignore\",\n", ""),
        edited(&text, "        \"rust-toolchain*\",\n", ""),
    ] {
        let selection = catalog_edit(&repo, Some(before))?;
        assert!(
            matches!(&selection.complete, Some(CompleteCause::Content { why, .. }) if why.contains("scope")),
            "{:?}",
            selection.complete
        );
        assert_eq!(
            selection.targets.len(),
            repo.workspace.test_targets().count()
        );
    }
    Ok(())
}

#[test]
fn an_unreadable_base_catalog_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    for before in [None, Some("SuiteCatalog(".to_string())] {
        let selection = catalog_edit(&repo, before)?;
        assert!(
            matches!(&selection.complete, Some(CompleteCause::Content { why, .. }) if why == "base catalog unreadable"),
            "{:?}",
            selection.complete
        );
    }
    Ok(())
}

#[test]
fn a_ci_workflow_change_runs_the_charter_checks_only() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &[".github/workflows/ci.yml"])?;
    assert!(selection.complete.is_none());
    assert_eq!(
        crates(&selection),
        BTreeSet::from(["workspace_checks".to_string()])
    );
    assert!(selection.commands.is_empty());
    Ok(())
}

#[test]
fn a_gate_source_change_still_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/gate/src/select.rs"])?;
    assert!(matches!(
        selection.complete,
        Some(CompleteCause::Global { .. })
    ));
    Ok(())
}

#[test]
fn a_gate_test_change_selects_only_the_gate_crate() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let selection = repo.select(Tier::Package, &["crates/gate/tests/selection_tests.rs"])?;
    assert!(selection.complete.is_none());
    assert_eq!(crates(&selection), gate_and_checks());
    Ok(())
}

#[test]
fn the_path_only_form_still_selects_the_complete_suite_for_the_catalog_and_the_lockfile(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    for path in [CATALOG_PATH, LOCKFILE_PATH, "crates/sim_core/Cargo.toml"] {
        let selection = repo.select(Tier::Package, &[path])?;
        assert!(selection.complete.is_some(), "{path}");
        assert_eq!(
            selection.targets.len(),
            repo.workspace.test_targets().count()
        );
    }
    Ok(())
}

/// The real `Cargo.lock` and `crates/sim_core/Cargo.toml`, and their versions without the
/// dev-dependency `proptest` of `sim_core`.
struct ProptestEdit {
    lock: String,
    lock_before: String,
    manifest: String,
    manifest_before: String,
}

const SIM_CORE_MANIFEST: &str = "crates/sim_core/Cargo.toml";

impl ProptestEdit {
    fn new() -> Result<Self, GateError> {
        let lock = real_text(LOCKFILE_PATH)?;
        let lock_before = edited(
            &lock,
            "name = \"sim_core\"\nversion = \"0.1.0\"\ndependencies = [\n \"proptest\",\n",
            "name = \"sim_core\"\nversion = \"0.1.0\"\ndependencies = [\n",
        );
        let manifest = real_text(SIM_CORE_MANIFEST)?;
        let manifest_before = edited(&manifest, "proptest.workspace = true\n", "");
        Ok(Self {
            lock,
            lock_before,
            manifest,
            manifest_before,
        })
    }

    fn lock_versions(&self) -> FileVersions {
        FileVersions {
            before: Some(self.lock_before.clone()),
            after: Some(self.lock.clone()),
        }
    }

    fn manifest_versions(&self) -> FileVersions {
        FileVersions {
            before: Some(self.manifest_before.clone()),
            after: Some(self.manifest.clone()),
        }
    }
}

/// The complete-suite cause's text, or a note that the selection is narrower.
fn cause(selection: &Selection) -> String {
    selection
        .complete
        .as_ref()
        .map_or_else(|| "narrower".to_string(), ToString::to_string)
}

#[test]
fn a_dev_dependency_only_change_selects_the_member_crate_not_the_complete_suite(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let edit = ProptestEdit::new()?;
    let selection = repo.select_change(
        Tier::Package,
        &[LOCKFILE_PATH, SIM_CORE_MANIFEST],
        [
            (LOCKFILE_PATH, edit.lock_versions()),
            (SIM_CORE_MANIFEST, edit.manifest_versions()),
        ],
    )?;
    assert!(selection.complete.is_none(), "{}", cause(&selection));
    assert!(has(&selection, "sim_core/lib"));
    assert!(has(
        &selection,
        "sim_core/sc2_who_wins_matchup_matrix_tests"
    ));
    assert!(selection.commands.contains_key("golden-replay-release"));
    assert_eq!(
        crates(&selection),
        BTreeSet::from(["sim_core".to_string(), "workspace_checks".to_string()])
    );
    Ok(())
}

#[test]
fn a_dev_only_change_selects_no_downstream_contract() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let edit = ProptestEdit::new()?;
    let selection = repo.select_change(
        Tier::Package,
        &[SIM_CORE_MANIFEST],
        [(SIM_CORE_MANIFEST, edit.manifest_versions())],
    )?;
    assert!(selection.complete.is_none(), "{}", cause(&selection));
    for unwanted in [
        "sim_fixtures/lib",
        "client/skirmish_match_tests",
        "asset_baker/runtime_catalog_tests",
    ] {
        assert!(!has(&selection, unwanted), "{unwanted} is downstream");
    }
    assert!(selection.changed_crates.contains("sim_core"));
    Ok(())
}

#[test]
fn a_runtime_dependency_change_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let manifest = real_text(SIM_CORE_MANIFEST)?;
    let before = edited(&manifest, "ron.workspace = true\n", "");
    let selection = repo.select_change(
        Tier::Package,
        &[SIM_CORE_MANIFEST],
        [(
            SIM_CORE_MANIFEST,
            FileVersions {
                before: Some(before),
                after: Some(manifest),
            },
        )],
    )?;
    assert_eq!(
        cause(&selection),
        "crates/sim_core/Cargo.toml: [dependencies] changed"
    );
    assert_eq!(
        selection.targets.len(),
        repo.workspace.test_targets().count()
    );
    Ok(())
}

#[test]
fn an_external_version_change_selects_the_complete_suite() -> Result<(), GateError> {
    let repo = Repo::load()?;
    let lock = real_text(LOCKFILE_PATH)?;
    let before = edited(
        &lock,
        "name = \"toml_edit\"\nversion = \"0.22.27\"\n",
        "name = \"toml_edit\"\nversion = \"0.22.26\"\n",
    );
    let selection = repo.select_change(
        Tier::Package,
        &[LOCKFILE_PATH],
        [(
            LOCKFILE_PATH,
            FileVersions {
                before: Some(before),
                after: Some(lock),
            },
        )],
    )?;
    assert!(
        cause(&selection).starts_with("Cargo.lock: external package toml_edit 0.22.2"),
        "{}",
        cause(&selection)
    );
    Ok(())
}

#[test]
fn a_member_lock_change_without_a_dev_only_manifest_change_selects_the_complete_suite(
) -> Result<(), GateError> {
    let repo = Repo::load()?;
    let edit = ProptestEdit::new()?;
    let selection = repo.select_change(
        Tier::Package,
        &[LOCKFILE_PATH],
        [(LOCKFILE_PATH, edit.lock_versions())],
    )?;
    assert_eq!(
        cause(&selection),
        "Cargo.lock: sim_core dependencies changed without a dev-only manifest change"
    );
    Ok(())
}
