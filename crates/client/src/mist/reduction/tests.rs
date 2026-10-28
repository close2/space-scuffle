//! CPU reducer boundary regressions; these do not certify a volumetric density field.

#![allow(clippy::float_cmp)] // Exact representable values and immutable reference clocks are part of this contract.

use super::*;

fn sample(x: f32, birth: f64, distance: f32) -> TrailSample {
    TrailSample {
        position: Vec3::new(x, 0.0, 0.0),
        birth,
        distance,
    }
}

fn run(samples: &[TrailSample], clock: f64) -> Result<ReducedWake, ReductionError> {
    reduce_samples(
        ReductionInput {
            clock,
            samples,
            capacity: MAX_SOURCE_SAMPLES,
        },
        1.0,
        &ReductionPolicy::for_lifetime(1.35)?,
        &mut ReductionScratch::default(),
    )
}

#[test]
fn empty_and_singleton_validate_clock_and_rounded_age() -> Result<(), ReductionError> {
    assert_eq!(run(&[], 0.0)?.control_count, 0);
    let one = run(&[sample(2.0, 0.5, 4.0)], 1.0)?;
    assert_eq!(one.control_count, 1);
    assert_eq!(one.controls[0].age, 0.5);
    for clock in [-1.0, f64::INFINITY, f64::NAN] {
        assert!(matches!(
            run(&[], clock),
            Err(ReductionError::Input("clock"))
        ));
    }
    assert!(matches!(
        run(&[sample(0.0, 0.0, 0.0)], f64::MAX),
        Err(ReductionError::Representation { .. })
    ));
    // This age is finite in f32, but rounding exceeds the authored seconds tolerance.
    assert!(matches!(
        run(&[sample(0.0, 0.0, 0.0)], 16_777_217.0),
        Err(ReductionError::Representation {
            field: "rounded endpoint tolerance",
            ..
        })
    ));
    Ok(())
}

#[test]
fn malformed_samples_are_rejected_before_deduplication() {
    let cases = [
        (sample(f32::NAN, 0.0, 0.0), "position"),
        (sample(0.0, -0.1, 0.0), "birth"),
        (sample(0.0, f64::INFINITY, 0.0), "birth"),
        (sample(0.0, 2.0, 0.0), "birth"),
        (sample(0.0, 0.0, -1.0), "path distance"),
        (sample(0.0, 0.0, f32::INFINITY), "path distance"),
    ];
    for (value, field) in cases {
        assert_eq!(
            run(&[value], 1.0),
            Err(ReductionError::Sample { index: 0, field })
        );
    }
    assert_eq!(
        run(&[sample(0.0, 0.8, 0.0), sample(1.0, 0.4, 1.0)], 1.0),
        Err(ReductionError::Sample {
            index: 1,
            field: "decreasing birth"
        })
    );
    assert_eq!(
        run(&[sample(0.0, 0.0, 1.0), sample(1.0, 0.5, 0.0)], 1.0),
        Err(ReductionError::Sample {
            index: 1,
            field: "decreasing path"
        })
    );
}

#[test]
fn straight_affine_attributes_reduce_to_endpoints() -> Result<(), ReductionError> {
    let source: Vec<_> = (0_u16..33)
        .map(|i| {
            sample(
                f32::from(i) / 16.0,
                f64::from(i) / 32.0,
                f32::from(i) / 16.0,
            )
        })
        .collect();
    let result = run(&source, 1.25)?;
    assert_eq!(result.control_count, 2);
    assert_eq!(result.source_count, 33);
    assert_eq!(result.source_arc, 2.0);
    assert_eq!(result.reduced_arc, 2.0);
    assert_eq!(result.errors, ReductionErrors::default());
    assert_eq!(result.controls[0].source.before, 0);
    assert_eq!(result.controls[1].source.before, 32);
    Ok(())
}

#[test]
fn only_all_attribute_duplicates_disappear() -> Result<(), ReductionError> {
    let a = sample(0.0, 0.0, 0.0);
    let result = run(&[a, a, sample(0.0, 0.5, 0.0), sample(0.0, 1.0, 1.0)], 1.0)?;
    assert_eq!(result.original_count, 4);
    assert_eq!(result.source_count, 3);
    assert_eq!(result.control_count, 3); // Path is not affine in birth over all three.
    assert!(result.segments[..2]
        .iter()
        .all(|s| s.kind == SegmentKind::ZeroMeasure));
    assert_eq!(result.relative_arc_deficit, None);
    Ok(())
}

#[test]
fn positive_arc_cannot_collapse_to_zero_chord() -> Result<(), ReductionError> {
    let result = run(
        &[
            sample(0.0, 0.0, 0.0),
            sample(0.01, 0.5, 0.01),
            sample(0.0, 1.0, 0.02),
        ],
        1.0,
    )?;
    assert_eq!(result.control_count, 3);
    assert!(result.segments[..2]
        .iter()
        .all(|s| s.kind == SegmentKind::Spatial));
    assert_eq!(result.source_arc, result.reduced_arc);
    Ok(())
}

#[test]
fn near_closed_counterexample_records_mass_risk_without_false_rejection(
) -> Result<(), ReductionError> {
    let source = [
        sample(0.0, 0.0, 0.0),
        sample(0.04, 40.0 / 79.0, 0.04),
        sample(0.001, 1.0, 0.079),
    ];
    let result = run(&source, 1.0)?;
    assert_eq!(result.control_count, 2);
    assert!(result.errors.maximum_ratio() <= 1.0);
    assert!(result.relative_arc_deficit.is_some_and(|loss| loss > 0.98));
    // Passing control accuracy expressly does not make this a passing field comparison.
    Ok(())
}

#[test]
fn clipping_preserves_cut_provenance_and_original_path_coordinate() -> Result<(), ReductionError> {
    let result = run(
        &[
            sample(0.0, 0.0, 10.0),
            sample(3.0, 0.3, 13.0),
            sample(10.0, 1.0, 20.0),
        ],
        1.2,
    )?;
    assert!(result.clipped);
    assert_eq!(result.original_arc, 10.0);
    assert!(result.source_arc <= 8.0);
    let oldest = result.controls[0];
    assert!((oldest.position.x - 2.0).abs() < 0.000_001);
    assert!((oldest.path_distance - 12.0).abs() < 0.000_001);
    assert_eq!(oldest.source.before, 0);
    assert_eq!(oldest.source.after, 1);
    assert!((oldest.source.fraction - 2.0 / 3.0).abs() < 0.000_001);
    assert!((oldest.birth - 0.2).abs() < 0.000_001);
    assert_eq!(result.clock, 1.2);
    Ok(())
}

#[test]
fn exact_sample_support_cut_uses_original_identity() -> Result<(), ReductionError> {
    let result = run(
        &[
            sample(0.0, 0.0, 0.0),
            sample(2.0, 0.2, 2.0),
            sample(10.0, 1.0, 10.0),
        ],
        1.0,
    )?;
    assert_eq!(
        result.controls[0].source,
        SourceLocation {
            before: 1,
            after: 1,
            fraction: 0.0
        }
    );
    assert_eq!(result.source_arc, 8.0);
    Ok(())
}

#[test]
fn tiny_positive_chords_with_infinite_reciprocal_are_rejected() {
    let tiny = f32::from_bits(1);
    assert!(matches!(
        run(&[sample(0.0, 0.0, 0.0), sample(tiny, 1.0, tiny)], 1.0),
        Err(ReductionError::Representation {
            field: "positive representable chord and reciprocal",
            ..
        })
    ));
}

#[test]
fn budget_failure_does_not_drop_violating_points() {
    let source: Vec<_> = (0_u16..18)
        .map(|i| TrailSample {
            position: Vec3::new(f32::from(i) / 8.0, if i % 2 == 0 { 0.0 } else { 0.25 }, 0.0),
            birth: f64::from(i) / 18.0,
            distance: f32::from(i),
        })
        .collect();
    assert!(matches!(
        run(&source, 1.0),
        Err(ReductionError::ControlBudgetExceeded {
            selected_count: 17,
            source_count: 18,
            ..
        })
    ));
}

#[test]
fn equal_error_ties_choose_earlier_original_index() -> Result<(), ReductionError> {
    let source = [
        sample(0.0, 0.0, 0.0),
        TrailSample {
            position: Vec3::new(1.0, 1.0, 0.0),
            birth: 0.25,
            distance: 1.0,
        },
        sample(2.0, 0.5, 2.0),
        TrailSample {
            position: Vec3::new(3.0, -1.0, 0.0),
            birth: 0.75,
            distance: 3.0,
        },
        sample(4.0, 1.0, 4.0),
    ];
    let mut policy = ReductionPolicy::for_lifetime(1.35)?;
    policy.maximum_controls = 3;
    let mut scratch = ReductionScratch::default();
    let error = reduce_samples(
        ReductionInput {
            clock: 1.0,
            samples: &source,
            capacity: 5,
        },
        1.0,
        &policy,
        &mut scratch,
    );
    assert!(matches!(
        error,
        Err(ReductionError::ControlBudgetExceeded { .. })
    ));
    assert_eq!(&scratch.selected[..3], &[0, 1, 4]);
    Ok(())
}

#[test]
fn caller_capacity_and_policy_limits_are_checked_for_empty_input() -> Result<(), ReductionError> {
    let mut policy = ReductionPolicy::for_lifetime(1.35)?;
    for capacity in [0, 129] {
        assert_eq!(
            reduce_samples(
                ReductionInput {
                    clock: 0.0,
                    samples: &[],
                    capacity
                },
                1.0,
                &policy,
                &mut ReductionScratch::default()
            ),
            Err(ReductionError::Input("source capacity"))
        );
    }
    policy.maximum_controls = 18;
    assert_eq!(
        policy.validate(),
        Err(ReductionError::Policy("maximum_controls"))
    );
    assert!(ReductionPolicy::for_lifetime(f32::NAN).is_err());
    Ok(())
}

#[test]
fn inclusive_position_tolerance_and_one_ulp_excess_are_distinct() -> Result<(), ReductionError> {
    let policy = ReductionPolicy {
        position_error_radii: 0.125,
        ..ReductionPolicy::for_lifetime(1.35)?
    };
    for (height, count) in [(0.125_f32, 2), (f32::from_bits(0.125_f32.to_bits() + 1), 3)] {
        let source = [
            sample(0.0, 0.0, 0.0),
            TrailSample {
                position: Vec3::new(1.0, height, 0.0),
                birth: 0.5,
                distance: 1.0,
            },
            sample(2.0, 1.0, 2.0),
        ];
        let result = reduce_samples(
            ReductionInput {
                clock: 1.0,
                samples: &source,
                capacity: 3,
            },
            1.0,
            &policy,
            &mut ReductionScratch::default(),
        )?;
        assert_eq!(result.control_count, count);
        assert!(result.errors.maximum_ratio() <= 1.0);
    }
    Ok(())
}

#[test]
fn age_is_subtracted_before_large_absolute_times_are_rounded() -> Result<(), ReductionError> {
    let clock = 1_099_511_627_776.0_f64;
    let result = run(
        &[
            sample(0.0, clock - 0.75, 0.0),
            sample(1.0, clock - 0.25, 1.0),
        ],
        clock,
    )?;
    assert_eq!(result.controls[0].age, 0.75);
    assert_eq!(result.controls[1].age, 0.25);
    assert_eq!(result.clock, clock);
    Ok(())
}

#[test]
fn rounded_support_endpoint_cannot_claim_exact_reference() -> Result<(), ReductionError> {
    let source = [
        sample(16_777_216.0, 0.0, 0.0),
        sample(16_777_220.0, 1.0, 4.0),
    ];
    let policy = ReductionPolicy {
        support_radii: 3.0,
        ..ReductionPolicy::for_lifetime(1.35)?
    };
    let error = reduce_samples(
        ReductionInput {
            clock: 1.0,
            samples: &source,
            capacity: 2,
        },
        1.0,
        &policy,
        &mut ReductionScratch::default(),
    );
    assert!(matches!(
        error,
        Err(ReductionError::Representation {
            field: "rounded endpoint tolerance",
            ..
        })
    ));
    Ok(())
}

#[test]
fn real_capacity_eviction_retains_original_phase_and_matches_raw_adapter(
) -> Result<(), Box<dyn std::error::Error>> {
    let style = super::super::MistStyle {
        samples_per_emitter: 4,
        ..Default::default()
    };
    let mut history = EmitterHistory::new(&style)?;
    history.advance(Vec3::ZERO, 1.0, 0.0, &style)?;
    for i in 1_u16..20 {
        history.advance(Vec3::new(f32::from(i) * 0.1, 0.0, 0.0), 1.0, 0.04, &style)?;
    }
    assert_eq!(history.len(), 4);
    let source: Vec<_> = (0..history.len())
        .filter_map(|i| history.sample(i))
        .collect();
    assert!(source[0].distance > 0.0);
    assert!(source[0].birth > 0.0);
    let policy = ReductionPolicy::for_lifetime(style.lifetime_seconds)?;
    let actual = reduce_history(&history, 1.0, &policy, &mut ReductionScratch::default())?;
    let raw = reduce_samples(
        ReductionInput {
            clock: history.clock(),
            samples: &source,
            capacity: history.capacity(),
        },
        1.0,
        &policy,
        &mut ReductionScratch::default(),
    )?;
    assert_eq!(actual, raw);
    assert_eq!(actual.controls[0].path_distance, source[0].distance);
    Ok(())
}

#[test]
fn every_clipped_sweep_stays_inside_declared_support() -> Result<(), ReductionError> {
    for index in 1_u16..100 {
        let step = f32::from(index) / 7.0;
        let source = [
            sample(0.0, 0.0, 0.0),
            sample(step, 0.25, step),
            sample(step * 2.0, 0.5, step * 2.0),
            sample(step * 4.0, 1.0, step * 4.0),
        ];
        let result = run(&source, 1.0)?;
        assert!(
            result.source_arc <= 8.0,
            "index={index}, arc={}",
            result.source_arc
        );
        assert!(result.errors.maximum_ratio() <= 1.0);
    }
    Ok(())
}

#[test]
fn reconstructed_cut_overflow_retains_ideal_reference_in_typed_error() -> Result<(), ReductionError>
{
    let policy = ReductionPolicy::for_lifetime(1.35)?;
    let mut scratch = ReductionScratch::default();
    let mut failure = None;
    for i in 20_u16..400 {
        let scale = f32::from(i);
        let source = [
            sample(0.0, 0.0, 0.0),
            TrailSample {
                position: Vec3::new(scale / 7.0, scale / 11.0, 0.0),
                birth: 0.25,
                distance: scale,
            },
            TrailSample {
                position: Vec3::new(scale / 3.0, scale / 13.0, 0.0),
                birth: 0.5,
                distance: scale * 2.0,
            },
            TrailSample {
                position: Vec3::new(scale / 2.0, scale / 17.0, 0.0),
                birth: 1.0,
                distance: scale * 3.0,
            },
        ];
        let result = reduce_samples(
            ReductionInput {
                clock: 1.0,
                samples: &source,
                capacity: 4,
            },
            1.0,
            &policy,
            &mut scratch,
        );
        if matches!(result, Err(ReductionError::SupportCut { .. })) {
            failure = Some(result);
            break;
        }
    }
    let Some(Err(ReductionError::SupportCut {
        support,
        reconstructed,
        boundary: cut,
        position,
        birth,
        path,
    })) = failure
    else {
        return Err(ReductionError::Input(
            "regression must expose the known unrepresentable support cut",
        ));
    };
    assert_eq!(support, 8.0);
    assert!(reconstructed > support);
    assert_eq!(cut.after, cut.before + 1);
    assert_eq!(cut, scratch.source[0].source);
    assert_eq!(position, scratch.source[0].position.to_array());
    assert_eq!(birth, scratch.source[0].birth);
    assert_eq!(path, scratch.source[0].path);
    Ok(())
}

#[test]
fn zero_arc_cut_ties_retain_earliest_boundary_age_and_phase() -> Result<(), ReductionError> {
    let result = run(
        &[
            sample(0.0, 0.0, 0.0),
            sample(2.0, 0.1, 2.0),
            sample(2.0, 0.5, 3.0),
            sample(10.0, 1.0, 11.0),
        ],
        1.0,
    )?;
    assert_eq!(result.source_count, 3);
    assert_eq!(result.controls[0].source.before, 1);
    assert_eq!(result.controls[0].birth, 0.1);
    assert_eq!(result.controls[0].path_distance, 2.0);
    assert_eq!(result.segments[0].kind, SegmentKind::ZeroMeasure);
    Ok(())
}

#[test]
fn adjacent_support_values_keep_the_direct_cut_fraction() -> Result<(), ReductionError> {
    let source = [
        sample(0.0, 0.0, 0.0),
        sample(2.0, 0.25, 2.0),
        sample(10.0, 1.0, 10.0),
    ];
    for support in [
        f64::from_bits(8.0_f64.to_bits() - 1),
        8.0,
        f64::from_bits(8.0_f64.to_bits() + 1),
    ] {
        let policy = ReductionPolicy {
            support_radii: support,
            ..ReductionPolicy::for_lifetime(1.35)?
        };
        let mut scratch = ReductionScratch::default();
        let result = reduce_samples(
            ReductionInput {
                clock: 1.0,
                samples: &source,
                capacity: 3,
            },
            1.0,
            &policy,
            &mut scratch,
        )?;
        let cut = 10.0 - support;
        let first = result.controls[0].source;
        if cut == 2.0 {
            assert_eq!(
                first,
                SourceLocation {
                    before: 1,
                    after: 1,
                    fraction: 0.0
                }
            );
        } else if cut > 2.0 {
            assert_eq!(first.fraction, (cut - 2.0) / 8.0);
            assert_eq!(first.before, 1);
        } else {
            assert_eq!(first.fraction, cut / 2.0);
            assert_eq!(first.before, 0);
        }
        assert!(result.source_arc <= support);
    }
    Ok(())
}

#[test]
fn duplicate_cut_names_the_actual_original_outgoing_edge() -> Result<(), ReductionError> {
    let point = sample(0.0, 0.0, 0.0);
    let result = run(&[point, point, sample(10.0, 1.0, 10.0)], 1.0)?;
    assert_eq!(result.controls[0].source.before, 1);
    assert_eq!(result.controls[0].source.after, 2);
    assert_eq!(result.controls[0].source.fraction, 0.2);
    Ok(())
}

#[test]
fn rejected_passes_count_actual_work_and_next_input_clears_it() -> Result<(), ReductionError> {
    let source = [
        sample(0.0, 0.0, 0.0),
        TrailSample {
            position: Vec3::Y,
            birth: 0.5,
            distance: 1.0,
        },
        sample(2.0, 1.0, 2.0),
    ];
    let policy = ReductionPolicy {
        maximum_controls: 2,
        ..ReductionPolicy::for_lifetime(1.35)?
    };
    let mut scratch = ReductionScratch::default();
    let result = reduce_samples(
        ReductionInput {
            clock: 1.0,
            samples: &source,
            capacity: 3,
        },
        1.0,
        &policy,
        &mut scratch,
    );
    assert!(matches!(
        result,
        Err(ReductionError::ControlBudgetExceeded { .. })
    ));
    assert_eq!(scratch.source_evaluations(), 3);
    assert!(reduce_samples(
        ReductionInput {
            clock: -1.0,
            samples: &[],
            capacity: 3
        },
        1.0,
        &policy,
        &mut scratch
    )
    .is_err());
    assert_eq!(scratch.source_evaluations(), 0);
    Ok(())
}

#[test]
fn positive_support_cannot_disappear_in_cut_subtraction() -> Result<(), ReductionError> {
    let policy = ReductionPolicy::for_lifetime(1.35)?;
    for (end, radius) in [(1.0e20, 1.0), (1.0, f32::from_bits(67 << 23))] {
        let source = [sample(0.0, 0.0, 0.0), sample(end, 1.0, 1.0)];
        let result = reduce_samples(
            ReductionInput {
                clock: 1.0,
                samples: &source,
                capacity: 2,
            },
            radius,
            &policy,
            &mut ReductionScratch::default(),
        );
        assert!(matches!(
            result,
            Err(ReductionError::Representation {
                field: "positive support cut progress",
                ..
            })
        ));
    }
    Ok(())
}

#[test]
fn forced_budget_failure_reports_only_its_own_interval_errors() -> Result<(), ReductionError> {
    let source = [
        sample(0.0, 0.0, 0.0),
        TrailSample {
            position: Vec3::new(1.0, 0.04, 0.0),
            birth: 1.0 / 6.0,
            distance: 1.0,
        },
        sample(2.0, 1.0 / 3.0, 2.0),
        sample(3.0, 0.5, 3.0),
        sample(3.01, 0.75, 3.01),
        sample(3.0, 1.0, 3.02),
    ];
    let policy = ReductionPolicy {
        maximum_controls: 3,
        ..ReductionPolicy::for_lifetime(1.35)?
    };
    let error = reduce_samples(
        ReductionInput {
            clock: 1.0,
            samples: &source,
            capacity: 6,
        },
        1.0,
        &policy,
        &mut ReductionScratch::default(),
    );
    let Err(ReductionError::ControlBudgetExceeded {
        start, end, errors, ..
    }) = error
    else {
        return Err(ReductionError::Input(
            "expected forced closed interval budget rejection",
        ));
    };
    assert_eq!((start, end), (3, 5));
    assert!(errors.position > 0.009 && errors.position < 0.011);
    assert!(errors.position < 0.04); // The earlier interval's larger error is unrelated.
    Ok(())
}

#[test]
fn singleton_failed_normalized_score_is_counted() -> Result<(), ReductionError> {
    let policy = ReductionPolicy {
        age_error_seconds: f64::from_bits(1),
        ..ReductionPolicy::for_lifetime(1.35)?
    };
    let source = [sample(0.0, 0.1, 0.0)];
    let mut scratch = ReductionScratch::default();
    let result = reduce_samples(
        ReductionInput {
            clock: 1.0,
            samples: &source,
            capacity: 1,
        },
        1.0,
        &policy,
        &mut scratch,
    );
    assert!(matches!(
        result,
        Err(ReductionError::Arithmetic("normalized candidate error"))
    ));
    assert_eq!(scratch.source_evaluations(), 1);
    Ok(())
}
