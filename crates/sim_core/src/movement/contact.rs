//! Disc contact with terrain: a walking ground body's disc never goes deeper into a wall (plan
//! `2026-11-11-contact-and-missiles` D-1 to D-3, bug B-113).
//!
//! # Responsibility
//! Owns [`settle_on_terrain`], the one rule that decides where a ground body's walking move, by
//! locomotion or by a push, may end. `StarCraft` II collides a unit's disc with terrain and
//! structures (its `InnerRadius`); our centre-only terrain test let a push press a disc up to its
//! whole radius into a wall, where its route's arc points round the corner it was pressed against
//! lay behind it (B-113). Here a move may not reduce the body's clearance (its distance to the
//! nearest wall, capped at its terrain radius). A move that would is projected out along the
//! contact normal, so the body slides along the wall or round its corner instead of stopping.
//!
//! # How it fits
//! Walking positions are written in two places, and both call [`settle_on_terrain`] on a
//! position their centre rules already admitted: locomotion before `MotionProposal::commit`
//! (every steering mode), and the collision phase's `apply_correction` (friendly and hostile
//! pushes). The wall geometry is `sim_map`'s `Pathfinder::wall_contact` (D-5) under the body's
//! physical request: true terrain, every structure, no vision.
//!
//! # Invariants
//! - Non-deepening: an admitted end's clearance is at least the start's minus
//!   [`WALL_CONTACT_TOLERANCE`]. A body clear of walls never overlaps one; a body that already
//!   overlaps (a spawn, a placement) may leave or slide, never sink, and is never thrown out.
//! - A projection never moves a body further than the move it replaces, and its end passes the
//!   centre rules from the move's start and from the tick's start.
//! - Exact arithmetic: distances come from exact squared distances by integer square root;
//!   every vector operation is checked, and an overflow is an error the caller reports.
//! - Air bodies are untouched: terrain does not reach them.

use super::math::{
    checked_normalize, checked_vec_add, checked_vec_scale, checked_vec_sub, squared_distance,
    MovementMathError,
};
use super::tuning::{WALL_CONTACT_PASSES, WALL_CONTACT_TOLERANCE};
use super::{MotionPolicy, MovementLayer};
use sim_map::{PathRequestWithVision, Pathfinder, SimMap, WallContact};
use sim_types::{SimScalar, SimVec2};

/// The physical request of a body of `radius` at `point`: true terrain and every structure, no
/// vision, as the segment predicate of movement uses.
fn contact_request(point: SimVec2, radius: SimScalar) -> PathRequestWithVision<'static> {
    PathRequestWithVision {
        start: point,
        goal: point,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
}

/// The clearance of a disc of `radius` at `point`: its distance to the nearest wall in world
/// units, `radius` when no wall lies within it, with the contact found.
pub(crate) fn clearance(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    radius: SimScalar,
    point: SimVec2,
) -> (SimScalar, Option<WallContact>) {
    match pathfinder.wall_contact(map, &contact_request(point, radius), point) {
        None => (radius, None),
        Some(contact) => {
            let root = contact.distance_sq.max(0).unsigned_abs().isqrt();
            // The contact lies within `radius`, so its distance fits a raw scalar.
            let raw = i64::try_from(root).unwrap_or(i64::MAX);
            (SimScalar::from_raw(raw).min(radius), Some(contact))
        }
    }
}

/// Moves `point` out of the wall `contact` to `target` from it along the contact normal, up to
/// [`WALL_CONTACT_PASSES`] times while another wall is still nearer than `target` (a concave
/// corner). `None` when the passes do not converge.
///
/// # Errors
/// An unrepresentable vector operation.
pub(crate) fn project_out(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    radius: SimScalar,
    target: SimScalar,
    mut point: SimVec2,
    mut contact: WallContact,
) -> Result<Option<SimVec2>, MovementMathError> {
    let floor = target.saturating_sub(WALL_CONTACT_TOLERANCE);
    for _ in 0..WALL_CONTACT_PASSES {
        let away = checked_vec_sub(point, contact.point)?;
        let normal = if away == SimVec2::ZERO {
            contact.normal
        } else {
            checked_normalize(away)?
        };
        point = checked_vec_add(contact.point, checked_vec_scale(normal, target)?)?;
        match clearance(map, pathfinder, radius, point) {
            (now, _) if now >= floor => return Ok(Some(point)),
            (_, Some(next)) => contact = next,
            (_, None) => return Ok(Some(point)),
        }
    }
    Ok(None)
}

/// Where a walking move of a ground body from `from` to `to` ends (plan D-1 to D-3), given that
/// the callers' centre rules already admitted `to`; `None` when the body stays at `from`.
///
/// `tick_start` is where the body stood when the tick began: every end must pass the centre
/// rule from there too, so a tick's whole travel stays walkable. In order:
/// 1. an air body, a still body, or an end whose clearance is at least the start's (less the
///    tolerance) is admitted as it is;
/// 2. otherwise the end is projected out of the wall to the start's clearance (sliding);
/// 3. otherwise the axis slides from `from` towards `to` are tried with the same test.
///
/// # Errors
/// An unrepresentable vector operation; the caller reports it and refuses the move.
pub(crate) fn settle_on_terrain(
    map: &SimMap,
    pathfinder: &mut Pathfinder,
    policy: MotionPolicy,
    tick_start: SimVec2,
    from: SimVec2,
    to: SimVec2,
) -> Result<Option<SimVec2>, MovementMathError> {
    if policy.layer != MovementLayer::Ground || to == from {
        return Ok(Some(to));
    }
    let radius = policy.terrain_radius();
    let (allowed, _) = clearance(map, pathfinder, radius, from);
    let floor = allowed.saturating_sub(WALL_CONTACT_TOLERANCE);
    let (reached, contact) = clearance(map, pathfinder, radius, to);
    if reached >= floor {
        return Ok(Some(to));
    }
    let walkable = |end: SimVec2| {
        policy.permits_segment(map, from, end) && policy.permits_segment(map, tick_start, end)
    };
    if let Some(contact) = contact {
        if let Some(end) = project_out(map, pathfinder, radius, allowed, to, contact)? {
            if squared_distance(from, end) <= squared_distance(from, to) && walkable(end) {
                return Ok(Some(end));
            }
        }
    }
    let along_x = SimVec2::new(to.x, from.y);
    let along_y = SimVec2::new(from.x, to.y);
    let x_dominant = to.x.raw().abs_diff(from.x.raw()) >= to.y.raw().abs_diff(from.y.raw());
    let slides = if x_dominant {
        [along_x, along_y]
    } else {
        [along_y, along_x]
    };
    for end in slides {
        if end != from && walkable(end) && clearance(map, pathfinder, radius, end).0 >= floor {
            return Ok(Some(end));
        }
    }
    Ok(None)
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};

    fn s(value: f64) -> SimScalar {
        SimScalar::from_num(value)
    }

    fn at(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(s(x), s(y))
    }

    /// A flat level-1 map of `size` × `size` with the cells for which `wall` holds made cliffs.
    fn walled(size: u32, wall: impl Fn(u32, u32) -> bool) -> GameMap {
        let mut map = GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
            .expect("invariant: a valid size");
        for y in 0..size {
            for x in 0..size {
                if wall(x, y) {
                    map.cell_mut(x, y).expect("inside").flags = CellFlags::UNPASSABLE_CLIFF;
                }
            }
        }
        map
    }

    fn ground(radius: f64) -> MotionPolicy {
        MotionPolicy {
            layer: MovementLayer::Ground,
            radius: s(radius),
            burrowed: false,
        }
    }

    fn close(a: SimVec2, b: SimVec2) -> bool {
        a.distance(b) <= s(1e-4)
    }

    /// T-B8: a disc pressed into a concave corner is projected out of both walls, one pass each.
    #[test]
    fn a_concave_corner_converges_out_of_both_walls() {
        let map = walled(20, |x, y| x >= 10 || y >= 10);
        let mut pathfinder = Pathfinder::new(&map);
        let (from, to) = (at(9.5, 9.5), at(9.8, 9.8));
        let end = settle_on_terrain(&map, &mut pathfinder, ground(0.375), from, from, to)
            .expect("representable")
            .expect("projected");
        assert!(close(end, at(9.625, 9.625)), "{end:?}");
    }

    /// T-B8: a centre on the wall is projected along the edge's open-side normal.
    #[test]
    fn a_centre_on_the_wall_uses_the_open_side_normal() {
        // The wall is west of x = 10, so a centre at x = 10 stands on open ground on the wall.
        let map = walled(20, |x, _| x < 10);
        let mut pathfinder = Pathfinder::new(&map);
        let (from, to) = (at(10.5, 5.0), at(10.0, 5.2));
        let end = settle_on_terrain(&map, &mut pathfinder, ground(0.375), from, from, to)
            .expect("representable")
            .expect("projected");
        assert!(close(end, at(10.375, 5.2)), "{end:?}");
    }

    /// T-B8: an unrepresentable projection is an error, which the callers report and refuse.
    #[test]
    fn an_overflowing_projection_is_an_error() {
        let map = walled(20, |x, _| x >= 10);
        let mut pathfinder = Pathfinder::new(&map);
        let contact = WallContact {
            point: SimVec2::new(SimScalar::MIN, SimScalar::MIN),
            distance_sq: 0,
            normal: at(1.0, 0.0),
        };
        let projected = project_out(
            &map,
            &mut pathfinder,
            s(0.375),
            s(0.375),
            at(9.9, 5.0),
            contact,
        );
        assert!(projected.is_err());
    }

    /// A seeded uniform draw in `[low, high)` (splitmix64).
    fn draw(state: &mut u64, low: f64, high: f64) -> f64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[expect(clippy::cast_precision_loss, reason = "a test draw")]
        let unit = (z >> 11) as f64 / (1_u64 << 53) as f64;
        low + (high - low) * unit
    }

    /// T-B7 (property): on seeded maps of random wall blocks, seeded moves of up to one world
    /// unit that pass the centre rule settle to an end that is no deeper than the start and no
    /// further from it than the move's end.
    #[test]
    fn a_settled_move_never_deepens_nor_lengthens() {
        let mut rng = 0xC0_47AC_u64;
        let mut projected = 0;
        for _ in 0..12 {
            let blocks: Vec<(f64, f64, f64, f64)> = (0..6)
                .map(|_| {
                    let (x, y) = (draw(&mut rng, 2.0, 20.0), draw(&mut rng, 2.0, 20.0));
                    (
                        x,
                        y,
                        x + draw(&mut rng, 1.0, 5.0),
                        y + draw(&mut rng, 1.0, 5.0),
                    )
                })
                .collect();
            let map = walled(24, |x, y| {
                let (x, y) = (f64::from(x), f64::from(y));
                blocks
                    .iter()
                    .any(|&(x0, y0, x1, y1)| x >= x0.floor() && x < x1 && y >= y0.floor() && y < y1)
            });
            let mut pathfinder = Pathfinder::new(&map);
            for _ in 0..200 {
                let policy = ground(draw(&mut rng, 0.25, 1.0));
                let from = at(draw(&mut rng, 1.0, 23.0), draw(&mut rng, 1.0, 23.0));
                let to = at(
                    f64::from(from.x.to_f32()) + draw(&mut rng, -1.0, 1.0),
                    f64::from(from.y.to_f32()) + draw(&mut rng, -1.0, 1.0),
                );
                if !policy.contains(&map, from)
                    || !policy.permits_segment(&map, from, from)
                    || !policy.permits_segment(&map, from, to)
                {
                    continue;
                }
                let radius = policy.terrain_radius();
                let (before, _) = clearance(&map, &mut pathfinder, radius, from);
                let settled = settle_on_terrain(&map, &mut pathfinder, policy, from, from, to)
                    .expect("representable");
                let Some(end) = settled else {
                    continue;
                };
                let (after, _) = clearance(&map, &mut pathfinder, radius, end);
                assert!(
                    after >= before - WALL_CONTACT_TOLERANCE,
                    "{from:?} to {to:?} settled at {end:?}: {before:?} to {after:?}"
                );
                assert!(squared_distance(from, end) <= squared_distance(from, to));
                projected += usize::from(end != to);
            }
        }
        assert!(projected >= 20, "{projected} moves were projected");
    }
}
