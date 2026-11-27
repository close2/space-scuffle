//! The charter's checkable claims, run against this repository.
//!
//! # Responsibility
//! One `#[test]` per check, named after its ID (`c01_...`, `c02_...`); C1, C15 to C20 and C24
//! judge the documentation (the charter's own statements and its guides, `DESIGN.md`, the plan
//! index, notebook paths, `README.md`) rather than code. Each runs the check from
//! the `workspace_checks` library on the repository root and fails with every finding, one
//! `path:line: message` per line, so a single run shows all the work to do.
//!
//! # How it fits
//! `cargo test --workspace` runs these with every other test, which is how CI's `check-and-test`
//! job enforces them. The library's unit tests prove each check can fail (positive controls on
//! planted violations); these tests prove the repository passes. Check C9's real enforcement is
//! clippy itself, reading the `clippy.toml` files this test requires.
//!
//! # Invariants
//! - A check that cannot read its input fails the test with that error, never passes silently.
//! - The C7 allowlist below is a ratchet: counts are exact, so removing floats from a listed file
//!   requires lowering its entry, and adding floats anywhere requires a reviewed entry.

use std::error::Error;
use workspace_checks::{
    cargo_metadata, charter_graph, charter_paths_exist, ci_runs_the_gate, design_doc,
    engine_free_simulation, expect_messages_state_invariants, float_ratchet, function_length_limit,
    host_math_in_smog, in_process_render_devices, module_docs, no_catalog_identities_in_code,
    no_discarded_results_in_simulation, no_embedded_assets, no_mutable_statics, no_work_markers,
    notebook_references, performance_rationale_format, plan_index, readme, render, repo_root,
    seeded_draws_name_a_fixed_width, simulation_clippy_config, test_subjects, tracked_files,
    typed_library_errors, Finding, FloatAllowance, SIMULATION_CRATES,
};

/// Fails with every finding of `check`, or passes when there are none.
fn assert_clean(check: &str, findings: &[Finding]) {
    assert!(
        findings.is_empty(),
        "{check}: {} finding(s)\n{}",
        findings.len(),
        render(findings)
    );
}

/// Files of the simulation crates whose production code may name `f32`/`f64`, recorded at
/// Phase 9 WP0. Every entry is a boundary where floats leave or enter the simulation; none of
/// them stores a float in authoritative state.
const FLOAT_ALLOWLIST: &[FloatAllowance] = &[
    FloatAllowance {
        path: "crates/sim_types/src/math.rs",
        count: 12,
        reason: "boundary conversions: RON/serde float input to `SimScalar`, and `to_f32` \
                 outputs for snapshots and the renderer",
    },
    FloatAllowance {
        path: "crates/sim_types/src/snapshot.rs",
        count: 33,
        reason: "presentation: `RenderSnapshot` is the documented fixed-point to GPU boundary, \
                 including a foundation's damage-look ratio (B-210)",
    },
    FloatAllowance {
        path: "crates/sim_types/src/player_view.rs",
        count: 1,
        reason: "presentation: a rally's point in `PlayerView`, the viewer's record for the HUD \
                 (plan `2026-11-23-production-queue` D-10)",
    },
    FloatAllowance {
        path: "crates/sim_types/src/presentation.rs",
        count: 6,
        reason: "presentation: line-splash, area and spell-missile geometry carried by \
                 `RenderSnapshot` for the effect system",
    },
    FloatAllowance {
        path: "crates/sim_map/src/grid.rs",
        count: 2,
        reason: "presentation: terrain altitude as `f32` for rendering interpolation",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog.rs",
        count: 86,
        reason: "authoring: catalog RON fields are decimal and converted to fixed-point at load \
                 (the two detection radii: plan `2026-11-23-vision-and-detection` D-2; a missile \
                 mover's bound and phases: plan `2026-11-27-weapon-delivery` D-2; a hangar's \
                 leash: plan `2026-11-28-carrier-interceptors` D-2; a phase end's distance, \
                 `PhaseEndDef`: plan `2026-11-28-missile-movers-b52` D-2; an impact spawn's \
                 search range, `ImpactSpawnDef`: plan `2026-11-28-broodlings` D-4; a death \
                 spawn's search range, `DeathSpawnSpecDef`: plan \
                 `2026-12-01-death-spawns-and-hangar-commands` D-1)",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog/strike_validation.rs",
        count: 2,
        reason: "authoring: the range checks of an impact spawn's and a death spawn's decimal \
                 search ranges before their fixed-point conversion (plan `2026-11-28-broodlings` \
                 D-4; plan `2026-12-01-death-spawns-and-hangar-commands` D-1)",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog/validation.rs",
        count: 53,
        reason: "authoring: range checks on catalog decimals before their fixed-point conversion \
                 (the detection radius: plan `2026-11-23-vision-and-detection` D-2; missile \
                 movers' bounds and phase ends: plans `2026-11-27-weapon-delivery` D-4 and \
                 `2026-11-28-missile-movers-b52` D-4)",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog/placement_rules.rs",
        count: 9,
        reason: "authoring: resource footprint shapes in `placement_rules.ron` are decimal world \
                 units, checked as multiples of 1/4 and converted to quarter cells at load \
                 (plan `2026-11-20-resource-footprints` D-1)",
    },
    FloatAllowance {
        path: "crates/sim_core/src/combat_resolution.rs",
        count: 4,
        reason: "a decimal constant converted once with `SimScalar::from_num`, and the \
                 presentation-only flight time of a projectile visual event and zone radii of \
                 a splash visual event",
    },
    FloatAllowance {
        path: "crates/sim_core/src/world.rs",
        count: 1,
        reason: "presentation: the altitude at which snapshot output draws air units",
    },
    FloatAllowance {
        path: "crates/sim_core/src/world/presentation.rs",
        count: 1,
        reason: "presentation: the barrier fraction written into `RenderSnapshot`",
    },
    FloatAllowance {
        path: "crates/sim_core/src/world/snapshot.rs",
        count: 8,
        reason: "presentation: progress and hit-flash ratios written into `RenderSnapshot`, a \
                 foundation's damage-look ratio (B-210), and the placement and share conversions \
                 a live record and an attack snapshot share (plan 2026-11-24-attacker-snapshot \
                 D-4)",
    },
];

#[test]
fn c01_charter_graph_equals_the_cargo_graph() -> Result<(), Box<dyn Error>> {
    let metadata = cargo_metadata(true)?;
    assert_clean("C1", &charter_graph(&repo_root(), &metadata)?);
    Ok(())
}

#[test]
fn c02_simulation_crates_have_no_engine_gpu_window_or_audio_dependency(
) -> Result<(), Box<dyn Error>> {
    let metadata = cargo_metadata(false)?;
    let findings = engine_free_simulation(&metadata, &repo_root(), &SIMULATION_CRATES)?;
    assert_clean("C2", &findings);
    Ok(())
}

#[test]
fn c03_no_unfinished_work_markers() -> Result<(), Box<dyn Error>> {
    assert_clean("C3", &no_work_markers(&repo_root())?);
    Ok(())
}

#[test]
fn c04_every_rust_file_begins_with_module_docs() -> Result<(), Box<dyn Error>> {
    assert_clean("C4", &module_docs(&repo_root())?);
    Ok(())
}

#[test]
fn c05_production_code_names_no_catalog_identity() -> Result<(), Box<dyn Error>> {
    assert_clean("C5", &no_catalog_identities_in_code(&repo_root())?);
    Ok(())
}

#[test]
fn c06_production_code_embeds_no_shipped_data() -> Result<(), Box<dyn Error>> {
    assert_clean("C6", &no_embedded_assets(&repo_root())?);
    Ok(())
}

#[test]
fn c07_floats_in_simulation_crates_only_at_listed_boundaries() -> Result<(), Box<dyn Error>> {
    assert_clean("C7", &float_ratchet(&repo_root(), FLOAT_ALLOWLIST)?);
    Ok(())
}

#[test]
fn c08_no_production_function_length_suppression_and_one_threshold() -> Result<(), Box<dyn Error>> {
    assert_clean("C8", &function_length_limit(&repo_root())?);
    Ok(())
}

#[test]
fn c09_simulation_clippy_config_bans_hashed_collections_and_clocks() -> Result<(), Box<dyn Error>> {
    assert_clean("C9", &simulation_clippy_config(&repo_root())?);
    Ok(())
}

#[test]
fn c10_expect_messages_state_an_invariant() -> Result<(), Box<dyn Error>> {
    assert_clean("C10", &expect_messages_state_invariants(&repo_root())?);
    Ok(())
}

#[test]
fn c11_performance_rationale_blocks_give_why_what_and_trade_off() -> Result<(), Box<dyn Error>> {
    assert_clean("C11", &performance_rationale_format(&repo_root())?);
    Ok(())
}

#[test]
fn c12_simulation_crates_discard_no_results() -> Result<(), Box<dyn Error>> {
    assert_clean("C12", &no_discarded_results_in_simulation(&repo_root())?);
    Ok(())
}

#[test]
fn c13_no_mutable_statics() -> Result<(), Box<dyn Error>> {
    assert_clean("C13", &no_mutable_statics(&repo_root())?);
    Ok(())
}

#[test]
fn c14_library_crates_return_typed_errors() -> Result<(), Box<dyn Error>> {
    assert_clean("C14", &typed_library_errors(&repo_root())?);
    Ok(())
}

#[test]
fn c15_charter_test_subjects_exist_and_every_crate_has_tests() -> Result<(), Box<dyn Error>> {
    assert_clean("C15", &test_subjects(&repo_root())?);
    Ok(())
}

#[test]
fn c16_ci_runs_every_gate_command() -> Result<(), Box<dyn Error>> {
    assert_clean("C16", &ci_runs_the_gate(&repo_root())?);
    Ok(())
}

#[test]
fn c17_design_doc_links_evidence_and_tick_rate_match_the_repository() -> Result<(), Box<dyn Error>>
{
    let root = repo_root();
    let rate = u32::from(sim_types::TICKS_PER_SECOND);
    assert_clean("C17", &design_doc(&root, &tracked_files(&root)?, rate)?);
    Ok(())
}

#[test]
fn c18_every_plan_has_one_index_row_with_a_known_status() -> Result<(), Box<dyn Error>> {
    assert_clean("C18", &plan_index(&repo_root())?);
    Ok(())
}

#[test]
fn c19_no_file_urls_and_lost_tmp_evidence_is_marked() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    assert_clean("C19", &notebook_references(&root, &tracked_files(&root)?)?);
    Ok(())
}

#[test]
fn c20_paths_named_in_the_charter_exist() -> Result<(), Box<dyn Error>> {
    let root = repo_root();
    assert_clean("C20", &charter_paths_exist(&root, &tracked_files(&root)?)?);
    Ok(())
}

#[test]
fn c21_no_test_builds_a_render_device_in_its_own_process() -> Result<(), Box<dyn Error>> {
    assert_clean("C21", &in_process_render_devices(&repo_root())?);
    Ok(())
}

#[test]
fn c23_smog_code_calls_no_host_transcendental_function() -> Result<(), Box<dyn Error>> {
    assert_clean("C23", &host_math_in_smog(&repo_root())?);
    Ok(())
}

#[test]
fn c24_readme_exists_its_links_resolve_and_it_shows_the_published_showcase(
) -> Result<(), Box<dyn Error>> {
    assert_clean("C24", &readme(&repo_root())?);
    Ok(())
}

#[test]
fn c25_seeded_draws_in_the_simulation_crates_name_a_fixed_width() -> Result<(), Box<dyn Error>> {
    assert_clean("C25", &seeded_draws_name_a_fixed_width(&repo_root())?);
    Ok(())
}
