//! Checked pair accumulation and final crowd correction preparation.
//! A pair contribution commits to both accumulators together; final pose preparation
//! never changes an actor until all numeric and terrain decisions have completed.
use super::math::{
    checked_normalize, checked_vec_add, checked_vec_scale, checked_vec_sub, within_distance,
};
use super::MovementMathError;
use sim_types::{EntityId, SimScalar, SimVec2};
use std::collections::BTreeMap;

pub(crate) fn accumulate_pair(
    pushes: &mut BTreeMap<EntityId, SimVec2>,
    a: EntityId,
    b: EntityId,
    da: SimVec2,
    db: SimVec2,
) -> Result<(), MovementMathError> {
    let previous_a = pushes.get(&a).copied().unwrap_or(SimVec2::ZERO);
    let previous_b = pushes.get(&b).copied().unwrap_or(SimVec2::ZERO);
    let next_a = checked_vec_add(previous_a, da)?;
    let next_b = checked_vec_add(previous_b, db)?;
    if da != SimVec2::ZERO {
        pushes.insert(a, next_a);
    }
    if db != SimVec2::ZERO {
        pushes.insert(b, next_b);
    }
    Ok(())
}

pub(crate) fn soft_position(
    position: SimVec2,
    start: SimVec2,
    push: SimVec2,
    speed: SimScalar,
) -> Result<SimVec2, MovementMathError> {
    if speed < SimScalar::ZERO {
        return Err(MovementMathError::NegativeSpeed);
    }
    if speed == SimScalar::ZERO {
        return Ok(position);
    }
    let max_push = speed
        .checked_mul(SimScalar::from_f32_lossy(0.65))
        .ok_or(MovementMathError::Unrepresentable)?
        .max(SimScalar::from_f32_lossy(0.015));
    let push = if within_distance(push, SimVec2::ZERO, i128::from(max_push.raw())) {
        push
    } else {
        checked_vec_scale(checked_normalize(push)?, max_push)?
    };
    let proposed = checked_vec_add(position, push)?;
    let delta = checked_vec_sub(proposed, start)?;
    if within_distance(delta, SimVec2::ZERO, i128::from(speed.raw())) {
        Ok(proposed)
    } else {
        checked_vec_add(start, checked_vec_scale(checked_normalize(delta)?, speed)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_pair_keeps_both_accumulators_and_prior_contributions() {
        let mut pushes = BTreeMap::from([
            (EntityId(1), SimVec2::X_AXIS),
            (
                EntityId(2),
                SimVec2::new(SimScalar::from_raw(i64::MAX), SimScalar::ZERO),
            ),
        ]);
        let before = pushes.clone();
        assert_eq!(
            accumulate_pair(
                &mut pushes,
                EntityId(1),
                EntityId(2),
                SimVec2::X_AXIS,
                SimVec2::X_AXIS
            ),
            Err(MovementMathError::Unrepresentable)
        );
        assert_eq!(pushes, before);
    }
}
