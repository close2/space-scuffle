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
//!   `docs/plans/2026-11-03-close-phase-6-7-8-exits.md`, decision D-4). It declares the display
//!   path the harness chose for it (`kwin-virtual` under the headless `KWin`,
//!   `xvfb-software-present` under Xvfb; plan `docs/plans/2026-11-17-gpu-test-readback.md`,
//!   WP3) and checks that the report records it and the display variables that prove it
//!   (`WP5b`).
//! - `the_400_match_lasts_the_measured_window_with_combat_inside_it` steps the `400` scenario in
//!   the simulation alone for the measured window and checks that the match does not end and that
//!   the combat window fits inside it (plan `docs/plans/2026-11-11-scenario-outer-rings.md`,
//!   Amendment 2).
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
use sim_types::{MatchResult, ShapeKind, Tick, TickBundle};
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

/// The two armies fight for the whole measured window: stepped by the simulation alone, as the
/// client steps it, the `400` scenario's match is still in progress after `measure_ticks`, and
/// its first damage leaves room for the whole combat window inside the measured one (plan
/// Amendment 2: sent to each other's start, one army won and destroyed the other hall).
#[test]
fn the_400_match_lasts_the_measured_window_with_combat_inside_it() -> Result<(), Box<dyn Error>> {
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let mut world =
        sim_core::scenario::load_scenario_battle(&root, &fixture::scenario_path("400"))?;
    let health = |world: &sim_core::SimWorld| -> i64 {
        world.entities.values().map(|entity| entity.hp.raw()).sum()
    };
    let mut last = health(&world);
    let mut first_damage = None;
    for tick in 1..=spec.measure_ticks {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        let now = health(&world);
        if first_damage.is_none() && now < last {
            first_damage = Some(tick);
        }
        last = now;
        assert!(
            matches!(world.match_state.result, MatchResult::InProgress),
            "the match ended at tick {tick}: {:?}",
            world.match_state.result
        );
    }
    let first_damage = first_damage.ok_or("no unit took damage in the measured window")?;
    assert!(
        first_damage + spec.combat_window_ticks <= spec.measure_ticks,
        "combat from tick {first_damage} leaves no room for the {}-tick combat window",
        spec.combat_window_ticks
    );
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
    let units =
        sim_core::catalog::RuntimeCatalogs::load_from_directory(root.join("catalog"))?.units;
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

/// The effect mix (plan `2026-11-09-effect-visualization` WP14) draws every kind of look: a shot
/// for weapons with a travel or impact, every distinct death look, area, tether and missile
/// abilities, and every `Status` and `Mode` effect of the vocabulary.
#[test]
fn the_mix_sources_cover_every_kind_of_look() -> Result<(), Box<dyn Error>> {
    use client::effects::catalog::schema::EventKind;
    let spec = spec()?;
    let work = tempfile::tempdir()?;
    let root = fixture::build_fixture_root(&spec, &assets(), work.path())?;
    let effects = client::effects::catalog::EffectCatalog::load(
        &root.join(client::effects::catalog::EFFECTS_CATALOG_PATH),
        &root,
    )?;
    let units =
        sim_core::catalog::RuntimeCatalogs::load_from_directory(root.join("catalog"))?.units;
    let tables = client::effects::catalog::UnitEffectTables::build(&units, &effects)?;
    let mix = client_diagnostics::frame_budget::effect_mix::mix_sources(&units, &tables, &effects);
    assert!(!mix.shots.is_empty(), "weapons name travels and impacts");
    assert!(mix
        .shots
        .iter()
        .all(|shot| tables.travel(shot.source).is_some() || tables.impact(shot.source).is_some()));
    let mut deaths: Vec<_> = units
        .archetypes()
        .filter_map(|archetype| tables.death(archetype.unit_type_id))
        .collect();
    deaths.sort_unstable();
    deaths.dedup();
    assert_eq!(mix.deaths.len(), deaths.len(), "one unit per death look");
    for (kind, list) in [
        ("area", &mix.areas),
        ("tether", &mix.links),
        ("missile", &mix.missiles),
    ] {
        assert!(!list.is_empty(), "the shipped units name a {kind} look");
    }
    let held_kinds = effects
        .effects
        .iter()
        .filter(|effect| matches!(effect.event, EventKind::Status | EventKind::Mode))
        .count();
    assert_eq!(
        mix.held.len(),
        held_kinds,
        "every unit look is held in turn"
    );
    assert_ne!(held_kinds, 0);
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

/// The `--display-path` that names the harness's display of a child, by its ledger name
/// (`gpu_run::GpuCommand::display_name`): the headless `KWin` is `kwin-virtual`, Xvfb with
/// radv's software present or with lavapipe is `xvfb-software-present`, and the test process's
/// own display (no `xvfb-run`) is `desktop`.
fn declared_display_path(display: &str) -> DisplayPath {
    match display {
        "kwin" => DisplayPath::KwinVirtual,
        "xvfb-sw" | "xvfb" => DisplayPath::XvfbSoftwarePresent,
        "host" => DisplayPath::Desktop,
        other => panic!("the smoke run needs a windowed display, not {other:?}"),
    }
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
    let declared = declared_display_path(command.display_name());
    command
        .args(["--display-path", declared.name()])
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
        // The process CPU clock is read at every `First`: each row's share is positive, so the
        // clock rose strictly across the window (plan `2026-11-19-retained-effect-instances`
        // D-14).
        assert!(
            frame.process_cpu_us > 0,
            "frame {}: no process CPU time recorded",
            frame.frame
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
        summary.process_cpu_ms.p50,
        summary.process_cpu_ms.mean,
    ] {
        assert!(value.is_finite() && value >= 0.0, "{summary:?}");
    }
    assert!(summary.interval_ms.p50 > 0.0, "{summary:?}");
    assert!(summary.process_cpu_ms.mean > 0.0, "{summary:?}");
    assert_ne!(report.environment.adapter, "");
    assert_eq!(report.environment.display_path, declared);
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
    let wayland = recorded("WAYLAND_DISPLAY");
    match declared {
        // `KWin` sets its socket for the session; Xvfb has none.
        DisplayPath::KwinVirtual => assert!(
            wayland.as_deref().is_some_and(|value| value != "unset"),
            "WAYLAND_DISPLAY is recorded: {wayland:?}"
        ),
        DisplayPath::XvfbSoftwarePresent | DisplayPath::Desktop => {
            assert_eq!(wayland.as_deref(), Some("unset"));
        }
    }
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

// --- The interleaved series (plan `2026-11-19-retained-effect-instances` D-14) ----------------

mod series {
    use client_diagnostics::frame_budget::series::{
        balanced_order, median, pair_statistics, verdict, RunFigures, RunRecord, SeriesReport,
        Slot, Verdict, CPU_SHARE_BAR, RECORDED_B_P50_MS,
    };

    /// A successful run of `arm` in `pair` at `load`, with a p50 of `p50` ms (p99 twice it) and
    /// `cpu` ms of process CPU per frame.
    fn run(arm: &str, pair: u32, load: f64, p50: f64, cpu: f64) -> RunRecord {
        RunRecord {
            arm: arm.to_owned(),
            pair,
            load_start: load,
            load_end: load,
            outcome: Ok(RunFigures {
                frames: 4000,
                interval_p50: p50,
                interval_p99: p50 * 2.0,
                update_p50: p50 * 0.8,
                cpu_mean: cpu,
                cpu_p50: cpu,
            }),
        }
    }

    #[test]
    fn the_order_is_balanced_b_first_then_reversed() {
        let order = balanced_order(3, 4);
        let arms: Vec<usize> = order.iter().map(|slot| slot.arm).collect();
        assert_eq!(arms, [0, 1, 2, 2, 1, 0, 0, 1, 2, 2, 1, 0]);
        assert_eq!(order[3], Slot { arm: 2, pair: 1 });
        for arm in 0..3 {
            assert_eq!(order.iter().filter(|slot| slot.arm == arm).count(), 4);
        }
        assert_eq!(balanced_order(3, 0).len(), 0);
    }

    #[test]
    fn the_median_takes_the_middle_or_the_mean_of_the_middle_two() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
    }

    #[test]
    fn a_pair_is_quiet_only_when_both_runs_stayed_at_or_below_sixteen() {
        let mut runs = vec![
            run("B", 0, 16.0, 10.0, 20.0),
            run("P", 0, 12.0, 10.4, 20.3),
            run("B", 1, 10.0, 10.0, 20.0),
            run("P", 1, 10.0, 10.4, 20.3),
        ];
        runs[3].load_end = 16.5;
        let pairs = pair_statistics(&runs, "B", "P");
        assert_eq!(pairs.len(), 2);
        assert!(pairs[0].quiet, "a load of exactly 16 is quiet");
        assert!(!pairs[1].quiet, "a load above 16 at the end is not");
        assert!((pairs[0].p50_ratio - 1.04).abs() < 1e-9);
        assert!((pairs[0].cpu_diff - 0.3).abs() < 1e-9);
    }

    #[test]
    fn a_failed_run_leaves_its_pair_out() {
        let mut runs = vec![
            run("B", 0, 5.0, 10.0, 20.0),
            run("P", 0, 5.0, 10.4, 20.3),
            run("B", 1, 5.0, 10.0, 20.0),
            run("P", 1, 5.0, 10.4, 20.3),
        ];
        runs[2].outcome = Err("crashed".into());
        let pairs = pair_statistics(&runs, "B", "P");
        assert_eq!(pairs.iter().map(|pair| pair.pair).collect::<Vec<_>>(), [0]);
    }

    #[test]
    fn four_quiet_pairs_give_the_wall_verdict_on_the_median_ratios() {
        let mut runs = Vec::new();
        for (pair, ratio) in [1.02, 1.04, 1.08, 1.03].into_iter().enumerate() {
            let pair = u32::try_from(pair).expect("a small index");
            runs.push(run("B", pair, 8.0, 10.0, 20.0));
            runs.push(run("P", pair, 8.0, 10.0 * ratio, 25.0));
        }
        let pairs = pair_statistics(&runs, "B", "P");
        let Verdict::Wall {
            quiet_pairs,
            median_p50_ratio,
            met,
            ..
        } = verdict(&pairs, &[10.0; 4])
        else {
            panic!("four quiet pairs take the wall route");
        };
        assert_eq!(quiet_pairs, 4);
        assert!((median_p50_ratio - 1.035).abs() < 1e-9);
        assert!(met, "1.035 is within +5 %, whatever the CPU says");
        runs[7] = run("P", 3, 8.0, 11.0, 25.0);
        runs[5] = run("P", 2, 8.0, 11.0, 25.0);
        let pairs = pair_statistics(&runs, "B", "P");
        assert!(matches!(
            verdict(&pairs, &[10.0; 4]),
            Verdict::Wall { met: false, .. }
        ));
    }

    #[test]
    fn too_few_quiet_pairs_give_the_cpu_verdict_against_the_recorded_floor() {
        let mut runs = Vec::new();
        for pair in 0..6 {
            runs.push(run("B", pair, 40.0, 9.0, 20.0));
            runs.push(run("P", pair, 40.0, 30.0, 20.5));
        }
        let pairs = pair_statistics(&runs, "B", "P");
        let Verdict::Cpu {
            pairs: judged,
            quiet_pairs,
            median_cpu_diff,
            bar,
            met,
        } = verdict(&pairs, &[9.0; 6])
        else {
            panic!("six loaded pairs take the CPU route");
        };
        assert_eq!((judged, quiet_pairs), (6, 0));
        assert!((median_cpu_diff - 0.5).abs() < 1e-9);
        assert!(
            (bar - CPU_SHARE_BAR * RECORDED_B_P50_MS).abs() < 1e-9,
            "a B p50 below the recorded 11.86 ms uses the recorded figure"
        );
        assert!(met, "0.5 ms is within 5 % of 11.86 ms");
        let higher = verdict(&pairs, &[20.0; 6]);
        assert!(
            matches!(higher, Verdict::Cpu { bar, .. } if (bar - 1.0).abs() < 1e-9),
            "a higher session B p50 raises the bar: {higher:?}"
        );
    }

    #[test]
    fn too_few_pairs_for_either_route_are_insufficient() {
        let runs: Vec<RunRecord> = (0..5)
            .flat_map(|pair| {
                [
                    run("B", pair, 40.0, 10.0, 20.0),
                    run("P", pair, 40.0, 10.0, 20.0),
                ]
            })
            .collect();
        assert_eq!(
            verdict(&pair_statistics(&runs, "B", "P"), &[10.0; 5]),
            Verdict::Insufficient {
                pairs: 5,
                quiet_pairs: 0
            }
        );
    }

    #[test]
    fn a_series_report_judges_every_arm_against_the_base() {
        let mut report = SeriesReport {
            arms: vec!["B".into(), "S".into(), "P".into()],
            pairs_requested: 1,
            extra: Vec::new(),
            runs: vec![
                run("B", 0, 5.0, 10.0, 20.0),
                run("S", 0, 5.0, 10.2, 20.1),
                run("P", 0, 5.0, 10.4, 20.3),
            ],
            base_p50_median: None,
            comparisons: Vec::new(),
        };
        report.judge();
        assert_eq!(report.base_p50_median, Some(10.0));
        assert_eq!(
            report
                .comparisons
                .iter()
                .map(|comparison| comparison.arm.as_str())
                .collect::<Vec<_>>(),
            ["S", "P"]
        );
        assert!(report
            .comparisons
            .iter()
            .all(|comparison| comparison.pairs.len() == 1));
    }
}
