//! Public angle conversion handles the signed raw minimum without abs overflow.
//! Exact diagonal/axis results preserve the existing binary-angle convention.
use sim_types::{SimAngle, SimScalar};
#[test]
fn signed_minimum_diagonal_and_axes_remain_total() {
    let min = SimScalar::from_raw(i64::MIN);
    let max = SimScalar::from_raw(i64::MAX);
    assert_eq!(SimAngle::atan2(min, min), SimAngle::from_degrees(225));
    assert_eq!(SimAngle::atan2(min, max), SimAngle::from_degrees(315));
    assert_eq!(SimAngle::atan2(max, min), SimAngle::from_degrees(135));
    assert_eq!(SimAngle::atan2(min, SimScalar::ZERO), SimAngle::DEG_270);
    assert_eq!(SimAngle::atan2(SimScalar::ZERO, min), SimAngle::DEG_180);
}
