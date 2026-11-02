//! Checked arithmetic shared by local steering, collision and atomic movement proposals.
//! Wide intermediates preserve representable geometry; no helper mutates world state.
//! Domain admission validates identities/rates once; data-dependent operations still
//! return explicit errors when their required final values cannot be represented.

use sim_types::{PlayerId, SimScalar, SimVec2};

/// Invalid movement input or a required result outside deterministic Q32.32 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MovementMathError {
    /// Collision pairs require distinct physical identities.
    #[error("collision pair repeats the same entity identity")]
    DuplicateEntity,
    /// A supplied distance/radius is negative or inconsistent with exact geometry.
    #[error("distance/radius is invalid or inconsistent with actor positions")]
    InvalidDistance,
    /// A required final value exceeds its numeric representation.
    #[error("movement arithmetic cannot represent the required value")]
    Unrepresentable,
    /// An owner's modifier slot does not exist.
    #[error("movement owner {0:?} has no player modifier slot")]
    InvalidOwner(PlayerId),
    /// Effective or authored base movement speed is negative.
    #[error("movement speed must be nonnegative")]
    NegativeSpeed,
    /// An ordered buff multiplier is negative.
    #[error("movement buff index {index} has a negative speed multiplier")]
    NegativeMultiplier {
        /// Index in the stored ordered buff sequence.
        index: usize,
    },
}

pub(crate) fn distance(a: SimVec2, b: SimVec2) -> Result<SimScalar, MovementMathError> {
    super::checked_distance(a, b).ok_or(MovementMathError::Unrepresentable)
}

/// Normalize a wide difference before narrowing its components. Even differences
/// spanning signed MIN/MAX can have a representable unit direction.
pub(crate) fn direction(a: SimVec2, b: SimVec2) -> Result<SimVec2, MovementMathError> {
    let x = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let y = i128::from(a.y.raw()) - i128::from(b.y.raw());
    let norm = raw_norm(a, b);
    if norm <= u128::from(SimScalar::EPSILON.raw().unsigned_abs()) {
        return Ok(SimVec2::ZERO);
    }
    let divisor = i128::try_from(norm).map_err(|_| MovementMathError::Unrepresentable)?;
    let component = |v: i128| {
        i64::try_from((v << 32) / divisor)
            .map(SimScalar::from_raw)
            .map_err(|_| MovementMathError::Unrepresentable)
    };
    Ok(SimVec2::new(component(x)?, component(y)?))
}

/// Dot/cross against a raw wide position difference without narrowing that
/// difference. Steering's unit normal bounds products below 2^98; checked sums
/// also keep this helper explicit if a future caller supplies an arbitrary vector.
pub(crate) fn difference_product(
    vector: SimVec2,
    end: SimVec2,
    start: SimVec2,
    cross: bool,
) -> Result<i128, MovementMathError> {
    let x = i128::from(end.x.raw()) - i128::from(start.x.raw());
    let y = i128::from(end.y.raw()) - i128::from(start.y.raw());
    let vx = i128::from(vector.x.raw());
    let vy = i128::from(vector.y.raw());
    let value = if cross {
        (vx * y).checked_sub(vy * x)
    } else {
        (vx * x).checked_add(vy * y)
    };
    value.ok_or(MovementMathError::Unrepresentable)
}

pub(crate) fn checked_vec_add(a: SimVec2, b: SimVec2) -> Result<SimVec2, MovementMathError> {
    Ok(SimVec2::new(
        a.x.checked_add(b.x)
            .ok_or(MovementMathError::Unrepresentable)?,
        a.y.checked_add(b.y)
            .ok_or(MovementMathError::Unrepresentable)?,
    ))
}
pub(crate) fn checked_vec_sub(a: SimVec2, b: SimVec2) -> Result<SimVec2, MovementMathError> {
    Ok(SimVec2::new(
        a.x.checked_sub(b.x)
            .ok_or(MovementMathError::Unrepresentable)?,
        a.y.checked_sub(b.y)
            .ok_or(MovementMathError::Unrepresentable)?,
    ))
}
pub(crate) fn checked_vec_neg(a: SimVec2) -> Result<SimVec2, MovementMathError> {
    checked_vec_sub(SimVec2::ZERO, a)
}
pub(crate) fn checked_vec_scale(a: SimVec2, s: SimScalar) -> Result<SimVec2, MovementMathError> {
    Ok(SimVec2::new(
        a.x.checked_mul(s)
            .ok_or(MovementMathError::Unrepresentable)?,
        a.y.checked_mul(s)
            .ok_or(MovementMathError::Unrepresentable)?,
    ))
}
pub(crate) fn checked_vec_div(a: SimVec2, s: SimScalar) -> Result<SimVec2, MovementMathError> {
    Ok(SimVec2::new(
        a.x.checked_div(s)
            .ok_or(MovementMathError::Unrepresentable)?,
        a.y.checked_div(s)
            .ok_or(MovementMathError::Unrepresentable)?,
    ))
}
pub(crate) fn checked_normalize(a: SimVec2) -> Result<SimVec2, MovementMathError> {
    direction(a, SimVec2::ZERO)
}
pub(crate) fn checked_dot(a: SimVec2, b: SimVec2) -> Result<SimScalar, MovementMathError> {
    let x = i128::from(a.x.raw()) * i128::from(b.x.raw());
    let y = i128::from(a.y.raw()) * i128::from(b.y.raw());
    let raw = x.checked_add(y).ok_or(MovementMathError::Unrepresentable)? >> 32;
    i64::try_from(raw)
        .map(SimScalar::from_raw)
        .map_err(|_| MovementMathError::Unrepresentable)
}

// Each raw coordinate difference is at most u64::MAX. Two squared differences
// therefore occupy at most129 bits, represented as (high limb, low limb).
pub(crate) fn squared_distance(a: SimVec2, b: SimVec2) -> (u128, u128) {
    let x = (i128::from(a.x.raw()) - i128::from(b.x.raw())).unsigned_abs();
    let y = (i128::from(a.y.raw()) - i128::from(b.y.raw())).unsigned_abs();
    let (low, carry) = (x * x).overflowing_add(y * y);
    (u128::from(carry), low)
}

pub(crate) fn within_distance(a: SimVec2, b: SimVec2, threshold: i128) -> bool {
    let Ok(n) = u128::try_from(threshold) else {
        return false;
    };
    // Thresholds are sums of at most two nonnegative signed raw radii and a
    // positive fixed margin: fewer than65 bits. Larger thresholds cover every
    // representable planar position pair without needing a square.
    if n >= (1_u128 << 65) {
        return true;
    }
    squared_distance(a, b) <= wide_square(n)
}

// n<2^65, so hi<=1 and every individual limb product fits u128. The
// carry explicitly preserves the129th/130th bits instead of wrapping the sum.
fn wide_square(n: u128) -> (u128, u128) {
    let hi = n >> 64;
    let lo = n & u128::from(u64::MAX);
    let mid = 2 * hi * lo;
    let (low, carry) = (lo * lo).overflowing_add(mid << 64);
    (hi * hi + (mid >> 64) + u128::from(carry), low)
}

fn raw_norm(a: SimVec2, b: SimVec2) -> u128 {
    let square = squared_distance(a, b);
    if square.0 == 0 {
        return super::integer_sqrt(square.1);
    }
    // PERFORMANCE RATIONALE:
    // Why: Ordinary map coordinates fit a 128-bit squared sum; an unconditional
    // two-limb binary search would add 64 comparisons to every normalization.
    // What: Reuse the existing integer root above and reserve the two-limb
    // search for extreme differences spanning the full signed coordinate domain.
    // Trade-off: A second, bounded root path retains full-domain correctness
    // without imposing its cost on ordinary geometry. Its root is in [2^64,2^65).
    let mut low = 1_u128 << 64;
    let mut high = 1_u128 << 65;
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if wide_square(mid) <= square {
            low = mid;
        } else {
            high = mid;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signed_extreme_diagonal_normal_is_representable() -> Result<(), MovementMathError> {
        let min = SimScalar::from_raw(i64::MIN);
        let max = SimScalar::from_raw(i64::MAX);
        let a = SimVec2::new(max, max);
        let b = SimVec2::new(min, min);
        let n = direction(a, b)?;
        assert!(n.x > SimScalar::ZERO && n.x < SimScalar::ONE);
        assert_eq!(n.x, n.y);
        assert_eq!(direction(b, a), Ok(-n));
        assert_eq!(
            direction(
                SimVec2::new(max, SimScalar::ZERO),
                SimVec2::new(min, SimScalar::ZERO)
            ),
            Ok(SimVec2::X_AXIS)
        );
        Ok(())
    }
}
