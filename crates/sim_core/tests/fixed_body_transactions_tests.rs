//! Checked collision-pair API behavior and atomic entity-only correction.
//! Fixtures use catalog capabilities; registered foundation invariants are covered
//! by `movement_layers_tests` through the actual world/map lifecycle.

use sim_core::movement::checked_distance;
use sim_core::steering::{
    compute_push_displacement, compute_push_displacement_with_mobility, resolve_push_interaction,
    CollisionMobility, PushEntityParams, PushGeometryError,
};
use sim_core::{Entity, UnitCatalog};
use sim_types::{EntityId, PlayerId, ShapeKind, SimScalar, SimVec2};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}
fn params(id: u32, pos: SimVec2) -> PushEntityParams {
    PushEntityParams {
        id: EntityId(id),
        pos,
        vel: SimVec2::ZERO,
        owner: PlayerId(0),
        push_priority: 10,
        cannot_be_pushed: false,
        is_holding: false,
    }
}
fn entities() -> Result<(Entity, Entity), Box<dyn std::error::Error>> {
    let catalog = UnitCatalog::new_starter_roster();
    let archetype = catalog
        .archetypes()
        .find(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }) && !a.is_immobile_by_default())
        .ok_or("mobile archetype")?;
    Ok((
        Entity::from_archetype(EntityId(1), PlayerId(0), point(8, 8), archetype),
        Entity::from_archetype(EntityId(2), PlayerId(1), point(9, 8), archetype),
    ))
}
#[test]
fn checked_dynamic_branches_accept_large_representable_normal_geometry() -> TestResult {
    for branch in 0..7 {
        let mut a = params(1, point(30_000, 40_000));
        let mut b = params(2, SimVec2::ZERO);
        match branch {
            0 => b.owner = PlayerId(1),
            1 => {
                b.is_holding = true;
                a.vel = point(30_000, 40_000);
            }
            2 => {
                a.is_holding = true;
                b.vel = point(30_000, 40_000);
            }
            3 => {
                a.cannot_be_pushed = true;
                b.cannot_be_pushed = true;
            }
            4 => a.vel = point(30_000, 40_000),
            5 => b.vel = point(30_000, 40_000),
            _ => {
                a.vel = point(30_000, 40_000);
                b.vel = point(30_000, 40_000);
            }
        }
        let distance = checked_distance(a.pos, b.pos).ok_or("distance")?;
        let result = compute_push_displacement(a, b, distance, SimScalar::from_num(50_010))?;
        assert_ne!(result, (SimVec2::ZERO, SimVec2::ZERO), "branch {branch}");
    }
    Ok(())
}
#[test]
fn quantized_normals_and_extreme_overlap_return_checked_results() -> TestResult {
    for raw in [
        1,
        2,
        3,
        255,
        65_536,
        SimScalar::EPSILON.raw(),
        SimScalar::EPSILON.raw() + 1,
    ] {
        let a = params(
            1,
            SimVec2::new(SimScalar::from_raw(raw), SimScalar::from_raw(raw)),
        );
        let mut b = params(2, SimVec2::ZERO);
        b.owner = PlayerId(1);
        let distance = checked_distance(a.pos, b.pos).ok_or("distance")?;
        let result = compute_push_displacement(a, b, distance, SimScalar::ONE)?;
        assert_eq!(result.0, -result.1);
    }
    let a = params(1, SimVec2::ZERO);
    let mut b = params(2, SimVec2::ZERO);
    b.owner = PlayerId(1);
    let result = compute_push_displacement(a, b, SimScalar::ZERO, SimScalar::from_raw(i64::MAX))?;
    assert_eq!(result.0, -result.1);
    assert_eq!(
        compute_push_displacement(a, b, SimScalar::ONE, SimScalar::ONE),
        Err(PushGeometryError::InvalidDistance)
    );
    Ok(())
}
#[test]
fn entity_wrapper_prepares_both_positions_before_mutating_either() -> TestResult {
    let (mut a, mut b) = entities()?;
    a.position = SimVec2::new(SimScalar::from_raw(i64::MAX), SimScalar::ZERO);
    b.position = SimVec2::new(SimScalar::from_raw(i64::MAX - 1), SimScalar::ZERO);
    let before = (a.clone(), b.clone());
    assert_eq!(
        resolve_push_interaction(&mut a, &mut b, SimScalar::from_raw(1), SimScalar::ONE),
        Err(PushGeometryError::Unrepresentable)
    );
    assert_eq!((a, b), before);
    Ok(())
}
#[test]
fn entity_wrapper_honors_immobile_and_unpushable_flags_for_enemy_pairs() -> TestResult {
    for fixed_a in [true, false] {
        let (mut a, mut b) = entities()?;
        a.is_immobile = fixed_a;
        b.unpushable = !fixed_a;
        let before = (a.position, b.position);
        resolve_push_interaction(&mut a, &mut b, SimScalar::ONE, SimScalar::from_num(2))?;
        if fixed_a {
            assert_eq!(a.position, before.0);
            assert_ne!(b.position, before.1);
        } else {
            assert_eq!(b.position, before.1);
            assert_ne!(a.position, before.0);
        }
        a.unpushable = true;
        b.is_immobile = true;
        let before = (a.clone(), b.clone());
        let distance = checked_distance(a.position, b.position).ok_or("distance")?;
        resolve_push_interaction(&mut a, &mut b, distance, SimScalar::from_num(4))?;
        assert_eq!((a, b), before);
    }
    Ok(())
}
#[test]
fn weighted_dynamic_api_uses_the_same_checked_priority_core() -> TestResult {
    let mut a = params(1, point(5, 5));
    let b = params(2, point(6, 5));
    a.vel = point(1, 0);
    let expected = compute_push_displacement(a, b, SimScalar::ONE, SimScalar::from_num(2))?;
    assert_eq!(
        compute_push_displacement_with_mobility(
            a,
            b,
            SimScalar::ONE,
            SimScalar::from_num(2),
            CollisionMobility::Dynamic,
            CollisionMobility::Dynamic
        )?,
        expected
    );
    Ok(())
}

#[test]
fn moving_pair_is_radial_and_idle_first_tie_uses_original_id_order() -> TestResult {
    let mut a = params(1, point(6, 5));
    let mut b = params(2, point(5, 5));
    a.vel = point(1, 0);
    b.vel = point(1, 0);
    let (da, db) = compute_push_displacement(a, b, SimScalar::ONE, SimScalar::from_num(2))?;
    assert_eq!(da.y, SimScalar::ZERO, "moving units get no sideways wedge");
    assert_eq!(db.y, SimScalar::ZERO, "moving units get no sideways wedge");
    assert_eq!(da.x, -db.x);
    a.vel = SimVec2::ZERO;
    let (da, db) = compute_push_displacement(a, b, SimScalar::ONE, SimScalar::from_num(2))?;
    assert!(
        da.y < SimScalar::ZERO,
        "idle first actor retains a.id < b.id lateral tie convention"
    );
    assert_eq!(db, SimVec2::ZERO);
    Ok(())
}

#[test]
fn coincident_fixed_pair_is_physically_stable_under_argument_and_id_reversal() -> TestResult {
    for (fixed_id, dynamic_id) in [(1, 2), (2, 1)] {
        let fixed = params(fixed_id, point(5, 5));
        let dynamic = params(dynamic_id, point(5, 5));
        let (df, dd) = compute_push_displacement_with_mobility(
            fixed,
            dynamic,
            SimScalar::ZERO,
            SimScalar::ONE,
            CollisionMobility::Fixed,
            CollisionMobility::Dynamic,
        )?;
        let reversed = compute_push_displacement_with_mobility(
            dynamic,
            fixed,
            SimScalar::ZERO,
            SimScalar::ONE,
            CollisionMobility::Dynamic,
            CollisionMobility::Fixed,
        )?;
        assert_eq!(df, SimVec2::ZERO);
        assert_eq!(reversed, (dd, df));
        assert_eq!(
            dd,
            if fixed_id < dynamic_id {
                -SimVec2::X_AXIS
            } else {
                SimVec2::X_AXIS
            }
        );
    }
    let a = params(1, point(5, 5));
    let mut b = params(2, point(5, 5));
    b.owner = PlayerId(1);
    let direct = compute_push_displacement(a, b, SimScalar::ZERO, SimScalar::ONE)?;
    let reversed = compute_push_displacement(b, a, SimScalar::ZERO, SimScalar::ONE)?;
    assert_eq!(direct, reversed, "legacy D/D uses argument-ordered +X tie");
    assert_eq!(
        compute_push_displacement_with_mobility(
            a,
            a,
            SimScalar::ZERO,
            SimScalar::ONE,
            CollisionMobility::Fixed,
            CollisionMobility::Dynamic
        ),
        Err(PushGeometryError::DuplicateEntity)
    );
    Ok(())
}
