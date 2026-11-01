//! Property tests for fixed-point arithmetic and binary angle wrapping.
//!
//! # Responsibility
//! Checks algebraic laws across the whole input domain, including the extreme values that the
//! hand-picked unit tests in `math.rs` do not reach.
//!
//! # How it fits
//! Every simulation crate builds on these types, so a law broken here surfaces elsewhere as an
//! opaque checksum divergence.
//!
//! # Invariants
//! Where an expected value can be derived independently, it is computed in wide integers rather
//! than through the operator under test.

use proptest::prelude::*;
use sim_types::{SimAngle, SimScalar};

fn raw(value: i64) -> SimScalar {
    SimScalar::from_raw(value)
}

fn saturate_raw(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

proptest! {
    #[test]
    fn addition_matches_wide_integer_model(a in any::<i64>(), b in any::<i64>()) {
        let expected = saturate_raw(i128::from(a) + i128::from(b));
        prop_assert_eq!((raw(a) + raw(b)).raw(), expected);
    }

    #[test]
    fn subtraction_matches_wide_integer_model(a in any::<i64>(), b in any::<i64>()) {
        let expected = saturate_raw(i128::from(a) - i128::from(b));
        prop_assert_eq!((raw(a) - raw(b)).raw(), expected);
    }

    #[test]
    fn multiplication_matches_wide_integer_model(a in any::<i64>(), b in any::<i64>()) {
        // Q32.32 product: truncate the Q64.64 result towards negative infinity.
        let expected = saturate_raw((i128::from(a) * i128::from(b)) >> 32);
        prop_assert_eq!((raw(a) * raw(b)).raw(), expected);
    }

    #[test]
    fn addition_and_multiplication_commute(a in any::<i64>(), b in any::<i64>()) {
        prop_assert_eq!(raw(a) + raw(b), raw(b) + raw(a));
        prop_assert_eq!(raw(a) * raw(b), raw(b) * raw(a));
    }

    #[test]
    fn checked_add_agrees_with_saturation(a in any::<i64>(), b in any::<i64>()) {
        let saturated = raw(a) + raw(b);
        match raw(a).checked_add(raw(b)) {
            Some(exact) => prop_assert_eq!(exact, saturated),
            None => prop_assert!(saturated == SimScalar::MAX || saturated == SimScalar::MIN),
        }
    }

    #[test]
    fn negation_is_an_involution_except_at_min(a in (i64::MIN + 1)..=i64::MAX) {
        prop_assert_eq!(-(-raw(a)), raw(a));
    }

    #[test]
    fn division_never_panics_and_inverts_small_products(
        a in -1_000_000_i32..1_000_000,
        b in 1_i32..1_000,
    ) {
        let product = SimScalar::from_i32(a) * SimScalar::from_i32(b);
        prop_assert_eq!(product / SimScalar::from_i32(b), SimScalar::from_i32(a));
    }

    #[test]
    fn division_is_total(a in any::<i64>(), b in any::<i64>()) {
        let quotient = raw(a) / raw(b);
        if b == 0 {
            let expected = match a.cmp(&0) {
                std::cmp::Ordering::Greater => SimScalar::MAX,
                std::cmp::Ordering::Less => SimScalar::MIN,
                std::cmp::Ordering::Equal => SimScalar::ZERO,
            };
            prop_assert_eq!(quotient, expected);
        }
    }

    #[test]
    fn sqrt_is_the_floor_root(a in 0_i64..=i64::MAX) {
        let root = i128::from(raw(a).sqrt().map_err(|error| TestCaseError::fail(error.to_string()))?.raw());
        let target = i128::from(a) << 32;
        prop_assert!(root * root <= target);
        prop_assert!((root + 1) * (root + 1) > target);
    }

    #[test]
    fn angle_addition_wraps_modulo_full_turn(a in any::<u16>(), b in any::<u16>()) {
        let expected = (u32::from(a) + u32::from(b)) % 65_536;
        prop_assert_eq!(u32::from((SimAngle(a) + SimAngle(b)).raw()), expected);
    }

    #[test]
    fn angle_subtraction_undoes_addition(a in any::<u16>(), b in any::<u16>()) {
        prop_assert_eq!(SimAngle(a) + SimAngle(b) - SimAngle(b), SimAngle(a));
    }

    #[test]
    fn angle_diff_is_shortest_and_reaches_target(a in any::<u16>(), b in any::<u16>()) {
        let difference = SimAngle(a).diff(SimAngle(b));
        prop_assert_eq!(SimAngle(a.wrapping_add_signed(difference)), SimAngle(b));
    }

    #[test]
    fn degrees_wrap_with_euclidean_modulo(degrees in -100_000_i32..100_000) {
        prop_assert_eq!(
            SimAngle::from_degrees(degrees),
            SimAngle::from_degrees(degrees.rem_euclid(360))
        );
    }

    #[test]
    fn rotate_towards_never_overshoots(a in any::<u16>(), b in any::<u16>(), step in any::<u16>()) {
        let start = SimAngle(a);
        let target = SimAngle(b);
        let stepped = start.rotate_towards(target, step);
        let before = start.diff(target).unsigned_abs();
        let after = stepped.diff(target).unsigned_abs();
        prop_assert!(after <= before);
    }

    #[test]
    fn sine_and_cosine_stay_on_the_unit_circle(a in any::<u16>()) {
        let angle = SimAngle(a);
        let squared = angle.sin() * angle.sin() + angle.cos() * angle.cos();
        prop_assert!((squared - SimScalar::ONE).abs() < SimScalar::from_num(0.001));
    }

    #[test]
    fn atan2_recovers_the_rotation_of_the_x_axis(a in any::<u16>()) {
        let angle = SimAngle(a);
        let recovered = SimAngle::atan2(angle.sin(), angle.cos());
        prop_assert!(recovered.diff(angle).unsigned_abs() <= 4);
    }
}
