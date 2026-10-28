//! Real-catalog CPU reduction/presentation regressions; no media or GPU is substituted.
//!
//! Focused cases check the trace-reuse optimization through actual worlds, clocks,
//! timelines and histories. The separately recorded full matrix remains mandatory.

use client::mist::reduction_admission::{AdmissionCorpus, MotionFamily};
use sim_types::MoverType;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}
fn corpus() -> Result<AdmissionCorpus, client::mist::reduction_admission::AdmissionError> {
    AdmissionCorpus::load(&root(), Path::new("captures/mist_reduction.ron"))
}

fn case_index(
    corpus: &AdmissionCorpus,
    mover: MoverType,
    family: &MotionFamily,
) -> Result<usize, Box<dyn std::error::Error>> {
    corpus
        .cases()
        .iter()
        .position(|case| case.mover == mover && &case.family == family)
        .ok_or_else(|| {
            format!("actual catalog lacks requested {mover:?} {family:?} fixture").into()
        })
}

#[test]
fn actual_inventory_derives_complete_capability_matrix() -> TestResult {
    let corpus = corpus()?;
    let catalogs = sim_core::catalog::RuntimeCatalogs::load_from_directory(root().join("catalog"))?;
    let mobile: Vec<_> = catalogs
        .units
        .archetypes()
        .filter(|a| !a.is_immobile_by_default())
        .collect();
    let weapons = mobile
        .iter()
        .filter(|a| a.weapon.is_some() || a.ground_weapon.is_some() || a.air_weapon.is_some())
        .count();
    let boosts = mobile.iter().filter(|a| a.overclock.is_some()).count();
    let mut movers = Vec::new();
    for arch in &mobile {
        if !movers.contains(&arch.mover_type) {
            movers.push(arch.mover_type);
        }
    }
    let expected = mobile.len() * 15 + weapons + boosts + movers.len() * 3;
    assert_eq!(corpus.cases().len(), expected);
    assert_eq!(
        corpus.unsupported().len(),
        mobile.len() * 2 - weapons - boosts
    );
    assert_eq!(corpus.window_count()?, u64::try_from(expected)? * 24);
    eprintln!(
        "actual mobiles={}, families={expected}, unsupported={}, windows={}",
        mobile.len(),
        corpus.unsupported().len(),
        corpus.window_count()?
    );
    for arch in mobile {
        assert_eq!(
            corpus
                .cases()
                .iter()
                .filter(
                    |case| case.unit_type == arch.unit_type_id && case.family == MotionFamily::Ramp
                )
                .count(),
            1
        );
    }
    Ok(())
}

#[test]
fn direct_world_and_saved_outputs_match_both_layers_at_zero_one_and_multiple_ticks() -> TestResult {
    let corpus = corpus()?;
    let mut observed_zero = false;
    let mut observed_one = false;
    let mut observed_many = false;
    for (mover, speed, rate, phase) in [(MoverType::Ground, 1, 144, -1), (MoverType::Air, 4, 30, 1)]
    {
        let index = case_index(&corpus, mover, &MotionFamily::Straight)?;
        let (summary, frames) = corpus.verify_window(index, speed, rate, phase)?;
        assert!(summary.displayed_travel > 0.0);
        assert!(summary.moving_histories > 0);
        assert!(summary.frames <= summary.frame_bound);
        observed_zero |= frames.iter().any(|frame| frame.ticks == 0);
        observed_one |= frames.iter().any(|frame| frame.ticks == 1);
        observed_many |= frames.iter().any(|frame| frame.ticks > 1);
    }
    assert!(observed_zero && observed_one && observed_many);
    Ok(())
}

#[test]
fn actual_virtual_time_clamp_and_battle_debt_are_distinct() -> TestResult {
    let corpus = corpus()?;
    let index = case_index(&corpus, MoverType::Ground, &MotionFamily::Hitch)?;
    let (summary, frames) = corpus.verify_window(index, 4, 30, -1)?;
    let hitch = frames
        .iter()
        .find(|frame| frame.real_ns > frame.virtual_ns)
        .ok_or("actual raw hitch was not clamped by TimePlugin")?;
    assert_eq!(hitch.requested_ns, hitch.real_ns);
    assert!(hitch.virtual_ns > 0);
    assert!(hitch.discarded_ns > 0);
    assert!(summary.discarded_ns > 0);
    assert_eq!(hitch.ticks, 5);
    assert!(hitch.debt_ns > 0);
    Ok(())
}

#[test]
fn pause_zero_speed_and_scene_return_preserve_real_aging_and_publication() -> TestResult {
    let corpus = corpus()?;
    for family in [
        MotionFamily::BattlePause,
        MotionFamily::ZeroSpeed,
        MotionFamily::TabReturn,
        MotionFamily::Disable,
    ] {
        let index = case_index(&corpus, MoverType::Ground, &family)?;
        let (summary, frames) = corpus.verify_window(index, 2, 60, 1)?;
        assert!(summary.displayed_travel > 0.0);
        if family != MotionFamily::Disable {
            let paused: Vec<_> = frames.iter().filter(|frame| frame.suspended).collect();
            assert_ne!(paused.len(), 0);
            assert!(paused
                .iter()
                .all(|frame| frame.ticks == 0 && frame.virtual_ns > 0));
            assert!(paused
                .windows(2)
                .all(|pair| pair[1].history_clock > pair[0].history_clock));
            assert!(paused
                .windows(2)
                .all(|pair| pair[1].current_tick == pair[0].current_tick));
        }
        if matches!(family, MotionFamily::TabReturn | MotionFamily::Disable) {
            let returned = frames
                .iter()
                .find(|frame| frame.lifecycle_reset)
                .ok_or("actual lifecycle reset not observed")?;
            assert_eq!(returned.samples.len(), 0);
        }
    }
    Ok(())
}

#[test]
fn generic_real_blink_transaction_replays_on_both_layers() -> TestResult {
    let corpus = corpus()?;
    for mover in [MoverType::Ground, MoverType::Air] {
        let index = case_index(&corpus, mover, &MotionFamily::Blink { offset_sign: 1 })?;
        let (summary, frames) = corpus.verify_window(index, 1, 144, -1)?;
        assert!(summary.displayed_travel > 0.0);
        assert!(
            summary
                .blink_transaction
                .as_ref()
                .is_some_and(|transaction| transaction.admitted),
            "actual transaction={:?}; failures={:?}",
            summary.blink_transaction,
            summary.failures
        );
        let transaction = summary
            .blink_transaction
            .as_ref()
            .ok_or("actual cast transaction absent")?;
        assert_eq!(transaction.actual_position, transaction.destination);
        assert_eq!(transaction.no_cast_position, transaction.from);
        assert_eq!(transaction.no_cast_bundle.commands.len(), 1);
        assert_eq!(
            transaction.no_cast_bundle.commands[0].order,
            sim_types::UnitOrder::Stop
        );
        assert!(summary.completed_horizon);
        eprintln!("actual {mover:?} isolated transaction={transaction:?}");
        if let Some(bridge) = summary.failures.iter().find(|failure| {
            failure.category == client::mist::reduction_admission::FailureCategory::Discontinuity
        }) {
            let frame = frames
                .get(usize::try_from(bridge.frame)?)
                .ok_or("first bridge frame missing")?;
            assert!(frame.blink_transition_tick.is_some());
            assert!(frame.newly_emitted_samples > 0 && frame.retained_previous_samples > 0);
            assert!(!frame.discontinuity_predicate);
            eprintln!("actual {mover:?} first bridge frame={frame:?}");
        }
        assert!(!summary.failures.iter().any(|failure| failure.category
            == client::mist::reduction_admission::FailureCategory::Producer));
        assert_ne!(frames.len(), 0);
        // A reported interpolation bridge remains a real refusal, never a test failure hidden as success.
        eprintln!(
            "{mover:?} actual generic Blink admission failures={:?}",
            summary.failures
        );
    }
    Ok(())
}

#[test]
fn actual_capabilities_and_loaded_ramp_produce_measured_coverage() -> TestResult {
    let corpus = corpus()?;
    let spec: client::mist::reduction_admission::AdmissionSpec = ron::from_str(
        &std::fs::read_to_string(root().join("captures/mist_reduction.ron"))?,
    )?;
    let map = sim_map::MapFile::load(&root().join(&spec.ramp_map))?
        .decode()?
        .to_game_map()?;
    let start = sim_types::SimVec2::new(spec.ramp_route[0][0], spec.ramp_route[0][1]);
    let end = sim_types::SimVec2::new(spec.ramp_route[1][0], spec.ramp_route[1][1]);
    assert!(map.terrain_altitude_scalar(end) > map.terrain_altitude_scalar(start));
    let (start_x, y) = map.world_to_cell(start)?;
    let (end_x, end_y) = map.world_to_cell(end)?;
    assert_eq!(y, end_y);
    assert!((start_x..=end_x).any(|x| map
        .cell(x, y)
        .is_ok_and(|cell| cell.flags.contains(sim_map::CellFlags::RAMP))));
    let mut indices = Vec::new();
    for mover in [MoverType::Ground, MoverType::Air] {
        for family in [
            MotionFamily::Ramp,
            MotionFamily::Collision,
            MotionFamily::Pursuit,
        ] {
            indices.push(case_index(&corpus, mover, &family)?);
        }
    }
    indices.push(
        corpus
            .cases()
            .iter()
            .position(|case| case.family == MotionFamily::Boost)
            .ok_or("actual catalog lacks authored boost fixture")?,
    );
    for index in indices {
        let (summary, frames) = corpus.verify_window(index, 1, 30, 1)?;
        eprintln!(
            "actual path first={:?}, last={:?}, minmax_x={:?}",
            frames.first().and_then(|frame| frame.position),
            frames.last().and_then(|frame| frame.position),
            frames.iter().filter_map(|frame| frame.position).fold(
                [f32::INFINITY, f32::NEG_INFINITY],
                |range, position| [range[0].min(position[0]), range[1].max(position[0])]
            )
        );
        let coverage = summary.coverage.as_ref().ok_or("actual coverage absent")?;
        eprintln!(
            "actual coverage {:?} {:?}: {:?}, height={:?}, boost={}, failures={:?}",
            summary.case.mover,
            summary.case.family,
            coverage,
            summary.ground_range,
            summary.boosted_frames,
            summary.failures
        );
        assert!(summary.completed_horizon);
        assert!(summary.displayed_travel > 0.0);
        assert!(coverage.subject_alive);
        match summary.case.family {
            MotionFamily::Ramp => assert!(summary
                .ground_range
                .is_some_and(|range| range[1] > range[0])),
            MotionFamily::Boost => assert!(summary.boosted_frames > 0),
            MotionFamily::Collision => assert!(coverage.same_layer_near_approach),
            MotionFamily::Pursuit => {
                assert!(coverage.pursuit_order_observed);
                assert_eq!(coverage.target_alive, Some(true));
            }
            _ => return Err("unexpected capability test case".into()),
        }
    }
    Ok(())
}
