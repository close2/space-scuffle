//! Behaviour of the `frame_budget` tool through its public API and its real binary.
//!
//! # Responsibility
//! - `fixture_root_scenarios_spawn_exactly_their_units` builds the measurement's assets root from
//!   the real spec and the real assets, then loads every scenario through the production loader
//!   (`client::runtime_battle::BattleDefinition`) and counts the units the production placement
//!   spawns. This also proves that the generated map passes validation and that every army fits
//!   its spawn rings.
//! - `frame_budget_smoke_run_writes_a_valid_report` runs the debug `frame_budget` binary for
//!   [`SMOKE_FRAMES`] measured frames of the 400-unit scenario on the test GPU, under Vulkan
//!   validation, through the client's GPU harness (`crates/client/tests/support/gpu_run.rs`), and
//!   checks the report it writes. It proves the measurement works; its numbers mean nothing (plan
//!   `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`, decision D-4). It declares
//!   `--display-path xvfb-software-present`, the harness's path, and checks that the report
//!   records it (`WP5b`).
//! - `a_contradicting_display_path_is_refused_before_anything_is_written` runs the binary with a
//!   declared path its environment contradicts. It exits before any renderer exists, so it needs
//!   neither the GPU harness nor the lock.
//!
//! # Invariants
//! - No test edits the repository's `assets/`: every root is built in a temporary directory.
//! - The GPU run goes through the harness, so it takes the shared lock on `radv` and is checked
//!   for the adapter and for validation findings (charter §7, check C21).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "test assertions report failures by panicking"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "../../client/tests/support/gpu_run.rs"]
mod gpu_run;

use client_diagnostics::frame_budget::display::DisplayPath;
use client_diagnostics::frame_budget::{fixture, report::FrameBudgetReport, FrameBudgetSpec};
use sim_types::ShapeKind;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Frames the smoke run measures, and warm-up frames before them.
///
/// The plan asked for 30 measured frames after the spec's 180 warm-up frames. On `lavapipe` a
/// 400-unit frame of the debug build takes about 2 s on the 16-core development server (measured
/// 2026-11-03: 180 warm-up frames in 368 s), and CI's runner has 4 cores, so the run would exceed
/// the harness's 300 s limit. Following the plan's rule, the frames are lowered and the unit count
/// is kept. Readiness (pipelines compiled, sprites loaded) is still awaited before frame 0.
const SMOKE_FRAMES: u32 = 10;
const SMOKE_WARMUP_FRAMES: u32 = 0;

/// The repository's real assets root.
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// The real measurement spec the binary reads by default.
fn spec() -> Result<FrameBudgetSpec, Box<dyn Error>> {
    Ok(FrameBudgetSpec::load(&FrameBudgetSpec::default_path())?)
}

#[test]
fn fixture_root_scenarios_spawn_exactly_their_units() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let players = spec.players.len();
    let mut totals = BTreeMap::new();
    for scenario in &spec.scenarios {
        let definition = client::runtime_battle::BattleDefinition::load(
            &root,
            &fixture::scenario_path(&scenario.name),
        )?;
        let world = definition.instantiate()?;
        let mut units: BTreeMap<u8, u32> = BTreeMap::new();
        let mut halls: BTreeMap<u8, u32> = BTreeMap::new();
        for entity in world.entities.values() {
            let archetype = world.catalog.get(entity.unit_type_id)?;
            let counts = match archetype.shape {
                ShapeKind::UnitSphere => &mut units,
                ShapeKind::BuildingDome { .. } => &mut halls,
            };
            *counts.entry(entity.owner.0).or_default() += 1;
        }
        assert_eq!(
            halls.len(),
            players,
            "scenario {}: one town hall per player",
            scenario.name
        );
        assert!(halls.values().all(|&count| count == 1));
        for player in &spec.players {
            assert_eq!(
                units.get(&player.slot).copied().unwrap_or(0),
                scenario.units_per_player,
                "scenario {}: units of player {}",
                scenario.name,
                player.slot
            );
        }
        totals.insert(scenario.name.clone(), units.values().sum::<u32>());
    }
    let expected: BTreeMap<String, u32> = [("0", 0), ("200", 200), ("400", 400)]
        .into_iter()
        .map(|(name, count)| (name.to_owned(), count))
        .collect();
    assert_eq!(totals, expected);
    Ok(())
}

/// The client loads the effect vocabulary and its texture sets at startup and exits if they
/// are missing, so the fixture root must hold them (the effect plan's `WP6c` found it did not).
#[test]
fn fixture_root_loads_the_effect_vocabulary() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let catalog = client::effects::catalog::EffectCatalog::load(
        &root.join(client::effects::catalog::EFFECTS_CATALOG_PATH),
        &root,
    )?;
    assert!(
        !catalog.effects.is_empty(),
        "the shipped vocabulary has effects"
    );
    Ok(())
}

/// The stress mode streams every weapon whose presentation block names a splash effect, with
/// the zones the simulation publishes.
#[test]
fn the_stress_sources_are_the_catalogs_splash_weapons() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let effects = client::effects::catalog::EffectCatalog::load(
        &root.join(client::effects::catalog::EFFECTS_CATALOG_PATH),
        &root,
    )?;
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = client::effects::catalog::UnitEffectTables::build(&units, &effects)?;
    let sources =
        client_diagnostics::frame_budget::effect_stress::stress_sources(&units, &tables, &effects);
    let mut named: Vec<&str> = sources
        .iter()
        .map(|source| source.effect.as_str())
        .collect();
    named.sort_unstable();
    named.dedup();
    let mut chosen: Vec<&str> = units
        .archetypes()
        .flat_map(|archetype| archetype.effects.weapons.iter())
        .filter_map(|weapon| weapon.splash.as_deref())
        .collect();
    chosen.sort_unstable();
    chosen.dedup();
    assert!(!chosen.is_empty(), "the shipped units name splash effects");
    assert_eq!(named, chosen, "every chosen splash effect is streamed");
    assert!(sources
        .iter()
        .all(|source| source.zones[2] > 0.0 && source.zones[0] <= source.zones[2]));
    Ok(())
}

#[test]
fn the_generated_map_passes_validation_without_warnings() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let document = sim_map::MapFile::load(&root.join(&spec.map.file))?.decode()?;
    let report = sim_map::validate_map(&document);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        (document.width, document.height),
        (spec.map.width, spec.map.height)
    );
    Ok(())
}

#[test]
fn a_work_directory_inside_the_real_assets_is_refused_before_anything_is_written(
) -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let inside = assets().join("frame_budget_must_not_exist");
    let result = fixture::build_fixture_root(&spec, &assets(), &inside);
    assert!(
        matches!(
            result,
            Err(fixture::FixtureError::WorkDirInsideAssets { .. })
        ),
        "{result:?}"
    );
    assert!(!inside.exists());
    Ok(())
}

#[test]
fn rebuilding_a_root_replaces_only_a_root_the_tool_made() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    std::fs::write(root.join("stale.txt"), "left by an earlier run")?;
    let again = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    assert_eq!(again, root);
    assert!(!root.join("stale.txt").exists());

    let foreign = tempfile::tempdir()?;
    std::fs::create_dir(foreign.path().join(fixture::ROOT_DIRECTORY))?;
    std::fs::write(
        foreign
            .path()
            .join(fixture::ROOT_DIRECTORY)
            .join("mine.txt"),
        "",
    )?;
    let result = fixture::build_fixture_root(&spec, &assets(), foreign.path());
    assert!(
        matches!(result, Err(fixture::FixtureError::ForeignRoot { .. })),
        "{result:?}"
    );
    assert!(foreign
        .path()
        .join(fixture::ROOT_DIRECTORY)
        .join("mine.txt")
        .exists());
    Ok(())
}

#[test]
fn frame_budget_smoke_run_writes_a_valid_report() -> Result<(), Box<dyn Error>> {
    if gpu_run::gpu_tests_opted_out() {
        return Ok(());
    }
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let out = work.path().join("report.ron");
    let mut command = gpu_run::virtual_gpu_command_on_screen(
        Path::new(env!("CARGO_BIN_EXE_frame_budget")),
        Some("1280x720x24"),
    );
    // The harness removes `WAYLAND_DISPLAY` from every renderer child and runs it under Xvfb
    // (`GpuCommand::renderer`), which is exactly what this path declares.
    command
        .args(["--display-path", "xvfb-software-present"])
        .arg("--work-dir")
        .arg(work.path())
        .args(["--scenario", "400"])
        .args(["--warmup-frames", &SMOKE_WARMUP_FRAMES.to_string()])
        .args(["--measure-frames", &SMOKE_FRAMES.to_string()])
        .arg("--out")
        .arg(&out);
    let run = command.assert_passes_validation("frame_budget smoke run, scenario 400");
    eprintln!(
        "frame_budget smoke run took {:.1} s",
        run.run_time.as_secs_f64()
    );

    let report = FrameBudgetReport::load(&out)?;
    assert_eq!(report.frames.len(), SMOKE_FRAMES as usize);
    assert_eq!(report.frames[0].live_units, 400);
    assert_eq!(report.expected_units, 400);
    for frame in &report.frames {
        assert!(
            f64::from(frame.units_in_view)
                >= spec.min_in_view_fraction * f64::from(frame.live_units),
            "frame {}: {} of {} units in view",
            frame.frame,
            frame.units_in_view,
            frame.live_units
        );
    }
    let summary = &report.summary.all;
    assert_eq!(summary.frames, SMOKE_FRAMES as usize);
    assert_eq!(report.warmup_frames, SMOKE_WARMUP_FRAMES);
    for value in [
        summary.interval_ms.p50,
        summary.interval_ms.p95,
        summary.interval_ms.p99,
        summary.interval_ms.max,
        summary.update_ms.p50,
        summary.update_ms.p95,
        summary.simulation_ms.p50,
        summary.simulation_ms.p95,
    ] {
        assert!(value.is_finite() && value >= 0.0, "{summary:?}");
    }
    assert!(summary.interval_ms.p50 > 0.0, "{summary:?}");
    assert_ne!(report.environment.adapter, "");
    assert_eq!(
        report.environment.display_path,
        DisplayPath::XvfbSoftwarePresent
    );
    let recorded = |name: &str| {
        report
            .environment
            .graphics_environment
            .iter()
            .find(|(variable, _)| variable == name)
            .map(|(_, value)| value.clone())
    };
    let display = recorded("DISPLAY");
    assert!(
        display.as_deref().is_some_and(|value| value != "unset"),
        "DISPLAY is recorded: {display:?}"
    );
    assert_eq!(recorded("WAYLAND_DISPLAY").as_deref(), Some("unset"));
    assert_eq!(report.gpu_passes, None);
    Ok(())
}

#[test]
fn a_contradicting_display_path_is_refused_before_anything_is_written() -> Result<(), Box<dyn Error>>
{
    let work = tempfile::tempdir()?;
    // No renderer is ever built: the tool checks the declared path before anything else, so
    // this child runs without the GPU harness.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_frame_budget"))
        .args(["--display-path", "kwin-virtual"])
        .arg("--work-dir")
        .arg(work.path())
        .args(["--scenario", "0"])
        .arg("--out")
        .arg(work.path().join("report.ron"))
        .env_remove("WAYLAND_DISPLAY")
        .env("DISPLAY", ":frame-budget-test-dummy")
        .env_remove("MESA_VK_WSI_DEBUG")
        .output()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("WAYLAND_DISPLAY"), "{stderr}");
    assert!(stderr.contains("frame_budget failed"), "{stderr}");
    let left: Vec<_> = std::fs::read_dir(work.path())?.collect();
    assert!(left.is_empty(), "the work directory holds {left:?}");
    Ok(())
}
