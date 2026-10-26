//! Integration tests verifying mathematical identities and algebraic invariants.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_types::SimScalar;

#[test]
fn test_exhaustive_math_identities() {
    let mut rng = ChaCha8Rng::seed_from_u64(0xCAFE_BABE_1234_5678);

    for _ in 0..10_000 {
        let raw_a = rng.gen_range(-1_000_000i64..1_000_000i64) << 16;
        let raw_b = rng.gen_range(-1_000_000i64..1_000_000i64) << 16;
        let raw_c = rng.gen_range(-1_000_000i64..1_000_000i64) << 16;

        let a = SimScalar::from_raw(raw_a);
        let b = SimScalar::from_raw(raw_b);
        let c = SimScalar::from_raw(raw_c);

        // 1. Commutativity of Addition
        assert_eq!(
            a.saturating_add(b),
            b.saturating_add(a),
            "addition commutativity violated for ({a}, {b})"
        );

        // 2. Commutativity of Multiplication
        assert_eq!(
            a.saturating_mul(b),
            b.saturating_mul(a),
            "multiplication commutativity violated for ({a}, {b})"
        );

        // 3. Distributivity: a * (b + c) ≈ a * b + a * c within rounding epsilon
        if let (Some(b_plus_c), Some(ab), Some(ac)) =
            (b.checked_add(c), a.checked_mul(b), a.checked_mul(c))
        {
            if let (Some(lhs), Some(rhs)) = (a.checked_mul(b_plus_c), ab.checked_add(ac)) {
                let diff = (lhs - rhs).abs();
                // Fixed-point multiplication truncates the lowest 32 fractional bits,
                // so distribution is exact within a few multiples of EPSILON.
                assert!(
                    diff <= SimScalar::EPSILON * 4,
                    "distributivity violated: {lhs} vs {rhs} (diff {diff})"
                );
            }
        }

        // 4. Monotonicity of Square Root: a <= b => sqrt(a) <= sqrt(b)
        let non_neg_a = a.abs();
        let non_neg_b = b.abs();
        let (min_val, max_val) = if non_neg_a <= non_neg_b {
            (non_neg_a, non_neg_b)
        } else {
            (non_neg_b, non_neg_a)
        };

        let sqrt_min = min_val
            .sqrt()
            .expect("invariant: absolute value is non-negative");
        let sqrt_max = max_val
            .sqrt()
            .expect("invariant: absolute value is non-negative");
        assert!(
            sqrt_min <= sqrt_max,
            "sqrt monotonicity violated: sqrt({min_val}) = {sqrt_min} > sqrt({max_val}) = {sqrt_max}"
        );
    }
}
