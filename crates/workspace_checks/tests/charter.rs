//! The charter's checkable claims, run against this repository.
//!
//! # Responsibility
//! One `#[test]` per check, named after its ID (`c02_...`, `c03_...`); C18 and C19 judge the
//! documentation (plan index, notebook paths) rather than code. Each runs the check from
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
    cargo_metadata, engine_free_simulation, float_ratchet, module_docs, no_mutable_statics,
    no_work_markers, notebook_references, plan_index, render, repo_root, simulation_clippy_config,
    tracked_files, typed_library_errors, Finding, FloatAllowance, SIMULATION_CRATES,
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
        path: "crates/sim_types/src/enums.rs",
        count: 1,
        reason: "presentation: `Faction::primary_color` returns linear RGBA for the renderer",
    },
    FloatAllowance {
        path: "crates/sim_types/src/math.rs",
        count: 12,
        reason: "boundary conversions: RON/serde float input to `SimScalar`, and `to_f32` \
                 outputs for snapshots and the renderer",
    },
    FloatAllowance {
        path: "crates/sim_types/src/snapshot.rs",
        count: 28,
        reason: "presentation: `RenderSnapshot` is the documented fixed-point to GPU boundary",
    },
    FloatAllowance {
        path: "crates/sim_map/src/grid.rs",
        count: 2,
        reason: "presentation: terrain altitude as `f32` for rendering interpolation",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog.rs",
        count: 46,
        reason: "authoring: catalog RON fields are decimal and converted to fixed-point at load",
    },
    FloatAllowance {
        path: "crates/sim_core/src/catalog/validation.rs",
        count: 20,
        reason: "authoring: range checks on catalog decimals before their fixed-point conversion",
    },
    FloatAllowance {
        path: "crates/sim_core/src/combat_resolution.rs",
        count: 2,
        reason: "a decimal constant converted once with `SimScalar::from_num`, and the \
                 presentation-only flight time of a projectile visual event",
    },
    FloatAllowance {
        path: "crates/sim_core/src/world.rs",
        count: 3,
        reason: "presentation: progress and hit-flash ratios written into `RenderSnapshot` \
                 (moves to `world/snapshot.rs` in Phase 9 WP1)",
    },
];

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
fn c07_floats_in_simulation_crates_only_at_listed_boundaries() -> Result<(), Box<dyn Error>> {
    assert_clean("C7", &float_ratchet(&repo_root(), FLOAT_ALLOWLIST)?);
    Ok(())
}

#[test]
fn c09_simulation_clippy_config_bans_hashed_collections_and_clocks() -> Result<(), Box<dyn Error>> {
    assert_clean("C9", &simulation_clippy_config(&repo_root())?);
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
