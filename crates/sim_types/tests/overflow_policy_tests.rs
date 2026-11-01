//! Behavioural tests for the explicit `SimScalar` overflow policy and exact vector length.
//!
//! # Responsibility
//! Pins the arithmetic contract that lockstep depends on: every operator saturates in every
//! build profile, and `length`, `distance` and `normalize` are computed exactly rather than
//! through a lossy Q32.32 square.
//!
//! # How it fits
//! CI runs this file in both debug and release. A panicking or wrapping operator would make
//! peers on different profiles diverge, which is the defect recorded as F-1. Small-vector
//! normalisation is F-3.
//!
//! # Invariants
//! Assertions name exact raw values wherever the correct answer is exactly representable.

use proptest::prelude::*;
use sim_types::{MathError, SimScalar, SimVec2};

fn scalar(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn vector(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(scalar(x), scalar(y))
}

fn exact_norm_raw(x: i64, y: i64) -> u128 {
    let square = |raw: i64| u128::from(raw.unsigned_abs()) * u128::from(raw.unsigned_abs());
    (square(x) + square(y)).isqrt()
}

#[test]
fn operators_saturate_at_both_bounds() {
    assert_eq!(SimScalar::MAX + SimScalar::ONE, SimScalar::MAX);
    assert_eq!(SimScalar::MIN - SimScalar::ONE, SimScalar::MIN);
    assert_eq!(SimScalar::MAX * scalar(2.0), SimScalar::MAX);
    assert_eq!(SimScalar::MAX * scalar(-2.0), SimScalar::MIN);
    assert_eq!(SimScalar::MIN * 2, SimScalar::MIN);
    assert_eq!(2 * SimScalar::MAX, SimScalar::MAX);
    assert_eq!(SimScalar::MAX / SimScalar::EPSILON, SimScalar::MAX);
    assert_eq!(SimScalar::MIN / -SimScalar::ONE, SimScalar::MAX);
    assert_eq!(-SimScalar::MIN, SimScalar::MAX);

    let mut accumulator = SimScalar::MAX;
    accumulator += SimScalar::ONE;
    assert_eq!(accumulator, SimScalar::MAX);
    accumulator = SimScalar::MIN;
    accumulator -= SimScalar::ONE;
    assert_eq!(accumulator, SimScalar::MIN);
    accumulator = SimScalar::MAX;
    accumulator *= scalar(4.0);
    assert_eq!(accumulator, SimScalar::MAX);
}

#[test]
fn division_by_zero_saturates_by_dividend_sign() {
    assert_eq!(SimScalar::ONE / SimScalar::ZERO, SimScalar::MAX);
    assert_eq!(-SimScalar::ONE / SimScalar::ZERO, SimScalar::MIN);
    assert_eq!(SimScalar::ZERO / SimScalar::ZERO, SimScalar::ZERO);
    assert_eq!(SimScalar::ONE / 0, SimScalar::MAX);
    assert_eq!(
        SimScalar::ONE.saturating_div(SimScalar::ZERO),
        SimScalar::MAX
    );
    assert_eq!(SimScalar::ONE.checked_div(SimScalar::ZERO), None);
}

#[test]
fn numeric_conversions_saturate_instead_of_wrapping() {
    assert_eq!(SimScalar::from_num(u32::MAX), SimScalar::MAX);
    assert_eq!(SimScalar::from_num(i64::MIN), SimScalar::MIN);
    assert_eq!(SimScalar::from_num(1e300_f64), SimScalar::MAX);
    assert_eq!(SimScalar::from_f32_lossy(-1e30), SimScalar::MIN);
}

#[test]
fn r10_length_of_large_vector_is_exact() {
    let length = vector(32768.0, 32768.0).length();
    let expected = exact_norm_raw(1 << 47, 1 << 47);
    assert_eq!(u128::from(length.raw().unsigned_abs()), expected);
    assert!((length.to_f32() - 46_340.95).abs() < 0.01);
}

#[test]
fn length_never_reports_zero_for_a_nonzero_vector() {
    for raw in [1_i64, -1, 2, 92_681, -92_681, i64::MAX, i64::MIN] {
        let length = SimVec2::new(SimScalar::from_raw(raw), SimScalar::ZERO).length();
        assert!(
            length > SimScalar::ZERO,
            "raw {raw} reported length {length:?}"
        );
    }
    assert_eq!(SimVec2::ZERO.length(), SimScalar::ZERO);
}

#[test]
fn extreme_length_and_distance_saturate_at_max() {
    let corner = SimVec2::new(SimScalar::MIN, SimScalar::MIN);
    assert_eq!(corner.length(), SimScalar::MAX);
    let opposite = SimVec2::new(SimScalar::MAX, SimScalar::MAX);
    assert_eq!(corner.distance(opposite), SimScalar::MAX);
}

#[test]
fn r11_normalize_small_vector_is_unit() {
    let raw = SimScalar::from_raw(92_681);
    let unit = SimVec2::new(raw, SimScalar::ZERO).normalize();
    assert_eq!(unit, Ok(SimVec2::X_AXIS));
}

#[test]
fn normalize_only_rejects_the_zero_vector() {
    assert_eq!(SimVec2::ZERO.normalize(), Err(MathError::ZeroLengthVector));
    assert_eq!(SimVec2::ZERO.normalize_or_zero(), SimVec2::ZERO);

    let smallest = SimVec2::new(SimScalar::EPSILON, SimScalar::EPSILON);
    let unit = smallest.normalize_or_zero();
    let error = (unit.length() - SimScalar::ONE).abs();
    assert!(error < scalar(1e-8), "length off by {error:?}");
    assert_eq!(smallest.normalize(), Ok(unit));
}

proptest! {
    /// Exact raw-square length is the floor of the true Euclidean norm, for any input.
    #[test]
    fn length_is_floor_of_exact_norm(x in any::<i64>(), y in any::<i64>()) {
        let length = SimVec2::new(SimScalar::from_raw(x), SimScalar::from_raw(y)).length();
        let expected = exact_norm_raw(x, y).min(u128::from(i64::MAX.unsigned_abs()));
        prop_assert_eq!(u128::from(length.raw().unsigned_abs()), expected);
    }

    /// Every nonzero vector normalises to unit length, whatever its magnitude.
    #[test]
    fn normalize_returns_unit_length(x in any::<i64>(), y in any::<i64>()) {
        prop_assume!(x != 0 || y != 0);
        let vector = SimVec2::new(SimScalar::from_raw(x), SimScalar::from_raw(y));
        let unit = vector.normalize();
        prop_assert!(unit.is_ok());
        let length = vector.normalize_or_zero().length();
        prop_assert!((length - SimScalar::ONE).abs() < scalar(1e-8));
    }

    /// Normalising keeps direction: the cross product with the input stays negligible.
    #[test]
    fn normalize_preserves_direction(x in -1_000_000.0..1_000_000.0_f64, y in -1_000_000.0..1_000_000.0_f64) {
        let input = vector(x, y);
        prop_assume!(input.length() > scalar(1e-3));
        let unit = input.normalize_or_zero();
        prop_assert!(unit.dot(input) >= SimScalar::ZERO);
        let aligned = (unit.cross_2d(input) / input.length()).abs();
        prop_assert!(aligned < scalar(1e-6));
    }
}
