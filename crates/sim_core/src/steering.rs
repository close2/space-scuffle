//! # Local Steering, Contact Rules and Spatial Hash Grid
//!
//! Owns the pure, per-pair and per-unit rules of local movement:
//! 1. One continuous steering function that bends a unit's heading around nearby bodies.
//! 2. Removal of the part of a step that would drive a unit into a body that will not yield, for
//!    all such bodies at once, so a step into a corner between two of them is removed whole.
//! 3. Explicit arrival: a unit stops against a body that already settled on its destination.
//! 4. Positional contact correction, where only the side that may move does.
//! 5. A spatial hash grid for neighbourhood queries.
//!
//! ## How it fits
//! [`crate::movement`] gathers each unit's neighbours from a tick-start grid and feeds them to
//! these functions; nothing here reads the world, so every rule can be tested on plain values.
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math (`SimScalar`, `SimVec2`).
//! 2. Neighbourhood queries return strictly sorted candidates for deterministic iteration.
//! 3. A body that may not move receives exactly zero correction, whoever touches it.
//! 4. Facing units decide sides the same way, each to its own right, so they always separate.
//! 5. A unit never steers into a gap narrower than itself between two bodies that will not yield:
//!    the pair is one wall, rounded at its nearer end (plan `2026-11-09-friendly-blocking` D-1).

use crate::entity::Entity;
use crate::movement::math::{
    checked_dot, checked_normalize, checked_vec_add, checked_vec_div, checked_vec_neg,
    checked_vec_scale, checked_vec_sub, difference_product, direction, distance, squared_distance,
    within_distance,
};
pub use crate::movement::tuning::PUSH_RELAXATION;
use crate::movement::tuning::{
    ARRIVAL_TOUCH_MARGIN, ARRIVAL_WITNESS_REACH, COMOVING_MIN_COSINE, EMERGENCY_SEPARATION,
    FIXED_BODY_TANGENT_WEIGHT, FLOCK_SHELL, FLOCK_WEIGHT_OVERLAPPING, FLOCK_WEIGHT_TOUCHING,
    OBSTACLE_WEIGHT, STANDING_DRIFT, STEERING_CORRIDOR_MARGIN, STEERING_LOOKAHEAD,
};
use sim_types::{EntityId, PlayerId, SimScalar, SimVec2};

// PERFORMANCE RATIONALE:
// Why: Naive O(N^2) pairwise distance checks for 400 units perform 160,000 checks per tick.
// What: Partition 2D world space into a flat spatial hash grid with 2.0x2.0 cell sizing, and
// clear only the buckets used, so an empty map costs nothing however large it is.
// Trade-off: Memory pre-allocated once during world initialization; neighborhood queries
// run in O(1) time per entity, reducing pairwise checks to O(N).

/// Default cell size in world units for the spatial hash grid.
pub const SPATIAL_CELL_SIZE: usize = 2;

/// Default grid width in 1x1 cells.
pub const GRID_WIDTH: usize = 128;

/// Default grid height in 1x1 cells.
pub const GRID_HEIGHT: usize = 128;

/// Spatial bucket index of a raw Q32.32 coordinate, clamped into `0..cells`.
#[inline]
pub(crate) fn bucket_coordinate(raw: i64, cell_size: usize, cells: usize) -> usize {
    let whole = usize::try_from(raw >> 32).unwrap_or(0);
    (whole / cell_size).min(cells.saturating_sub(1))
}

/// 2D flat spatial hash grid accelerating entity neighborhood queries.
#[derive(Debug, Clone)]
pub struct SpatialHashGrid {
    cell_size: usize,
    width_cells: usize,
    height_cells: usize,
    buckets: Vec<Vec<EntityId>>,
    occupied: Vec<usize>,
}

impl Default for SpatialHashGrid {
    fn default() -> Self {
        Self::new(GRID_WIDTH, GRID_HEIGHT, SPATIAL_CELL_SIZE)
    }
}

impl SpatialHashGrid {
    /// Constructs a spatial hash grid for a map of dimensions `map_width x map_height`.
    #[must_use]
    pub fn new(map_width: usize, map_height: usize, cell_size: usize) -> Self {
        let cs = cell_size.max(1);
        let width_cells = map_width.div_ceil(cs);
        let height_cells = map_height.div_ceil(cs);
        let total_buckets = width_cells.saturating_mul(height_cells).max(1);

        Self {
            cell_size: cs,
            width_cells,
            height_cells,
            buckets: vec![Vec::with_capacity(8); total_buckets],
            occupied: Vec::new(),
        }
    }

    /// Clears all entity references from the spatial hash grid.
    pub fn clear(&mut self) {
        for &index in &self.occupied {
            self.buckets[index].clear();
        }
        self.occupied.clear();
    }

    /// Inserts an entity into the grid bucket corresponding to its world position.
    pub fn insert(&mut self, id: EntityId, pos: SimVec2) {
        let cx = bucket_coordinate(pos.x.raw(), self.cell_size, self.width_cells);
        let cy = bucket_coordinate(pos.y.raw(), self.cell_size, self.height_cells);
        let idx = cy * self.width_cells + cx;
        if let Some(bucket) = self.buckets.get_mut(idx) {
            if bucket.is_empty() {
                self.occupied.push(idx);
            }
            bucket.push(id);
        }
    }

    /// Queries all entities located in buckets overlapping a circle centered at `pos` with `radius`.
    /// Results are populated into `out` and sorted deterministically.
    pub fn query_radius(&self, pos: SimVec2, radius: SimScalar, out: &mut Vec<EntityId>) {
        self.query_radius_unordered(pos, radius, out);
        // Guarantee deterministic candidate ordering
        out.sort_unstable();
    }

    /// Like [`Self::query_radius`], in grid order rather than sorted by ID.
    ///
    /// The order is a pure function of the inserted positions, so this stays deterministic.
    // PERFORMANCE RATIONALE:
    // Why: Sorting neighbours was about a quarter of target acquisition in a 400-unit battle.
    // What: Callers that rank candidates by a total order get them unsorted.
    // Trade-off: Those callers must not depend on visiting order.
    pub fn query_radius_unordered(&self, pos: SimVec2, radius: SimScalar, out: &mut Vec<EntityId>) {
        out.clear();
        let reach = radius.raw().max(1 << 32);
        let low = |value: SimScalar, cells| {
            bucket_coordinate(value.raw().saturating_sub(reach), self.cell_size, cells)
        };
        let high = |value: SimScalar, cells| {
            bucket_coordinate(value.raw().saturating_add(reach), self.cell_size, cells)
        };
        for cy in low(pos.y, self.height_cells)..=high(pos.y, self.height_cells) {
            for cx in low(pos.x, self.width_cells)..=high(pos.x, self.width_cells) {
                let idx = cy * self.width_cells + cx;
                if let Some(bucket) = self.buckets.get(idx) {
                    out.extend_from_slice(bucket);
                }
            }
        }
    }
}

/// Computes a normalized tangent around a blocked firing line in planar world units.
///
/// # Errors
/// Returns a typed arithmetic error if required geometry cannot be represented.
pub fn compute_concave_arc_steering(
    unit_pos: SimVec2,
    target_pos: SimVec2,
    blocked_by_allies: bool,
    steer_left: bool,
) -> Result<SimVec2, PushGeometryError> {
    if !blocked_by_allies {
        return Ok(SimVec2::ZERO);
    }
    let toward = direction(target_pos, unit_pos)?;
    Ok(if steer_left {
        SimVec2::new(-toward.y, toward.x)
    } else {
        SimVec2::new(toward.y, -toward.x)
    })
}

/// A nearby body, as local steering, contact and arrival see it.
///
/// Values are captured at the start of the movement phase, so every unit in a tick reacts to the
/// same world regardless of the order units are processed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteeringObstacle {
    /// Entity simulation identifier.
    pub id: EntityId,
    /// 2D planar position in continuous world space.
    pub pos: SimVec2,
    /// Collision radius in world units.
    pub radius: SimScalar,
    /// Displacement this body actually achieved last tick, in world units per tick.
    pub velocity: SimVec2,
    /// Owning player.
    pub owner: PlayerId,
    /// Authored priority for friendly contact.
    pub push_priority: u8,
    /// Whether this body holds its ground: on hold, immobile or anchored in combat.
    pub anchored: bool,
    /// Whether this body has nothing to do: no order, `Idle` or `Stop`. A body standing still
    /// under an order is waiting for room, not idle, and is not pushed aside.
    pub idle: bool,
    /// Destination this body has finished moving to, while it stays there.
    pub settled_on: Option<SimVec2>,
}

impl SteeringObstacle {
    /// A freely yielding body of player 0 that has not settled anywhere.
    #[must_use]
    pub fn new(id: EntityId, pos: SimVec2, radius: SimScalar, velocity: SimVec2) -> Self {
        Self {
            id,
            pos,
            radius,
            velocity,
            owner: PlayerId(0),
            push_priority: 0,
            anchored: false,
            idle: true,
            settled_on: None,
        }
    }

    /// Whether this body steps aside for a unit of `owner` and `push_priority` walking into it:
    /// a friendly body that is idle, standing still, not anchored, and of no higher priority (plan
    /// `2026-11-09-friendly-blocking` D-4). A body waiting under an order does not.
    #[must_use]
    pub fn yields_to(&self, owner: PlayerId, push_priority: u8) -> bool {
        self.owner == owner
            && !self.anchored
            && self.idle
            && within_distance(
                self.velocity,
                SimVec2::ZERO,
                i128::from(STANDING_DRIFT.raw()),
            )
            && push_priority >= self.push_priority
    }

    /// Whether this body stands: it holds its ground (anchored), or it moved by at most
    /// [`STANDING_DRIFT`] last tick, as a body waiting in a crowd does while contact nudges it.
    #[must_use]
    pub fn is_standing(&self) -> bool {
        self.anchored
            || within_distance(
                self.velocity,
                SimVec2::ZERO,
                i128::from(STANDING_DRIFT.raw()),
            )
    }
}

/// The nearest relevant blocker on one side of a unit's path.
#[derive(Clone, Copy, Debug)]
struct SideBlocker {
    square: (u128, u128),
    pos: SimVec2,
    radius: SimScalar,
    lateral: SimScalar,
    co_moving: bool,
    /// Whether this body can be part of a wall: it stands still and will not step aside for the
    /// steered unit ([`SteeringObstacle::yields_to`]).
    walls: bool,
}

/// Whether `velocity` runs broadly along the unit-length `heading`.
fn is_co_moving(heading: SimVec2, velocity: SimVec2) -> Result<bool, PushGeometryError> {
    if velocity == SimVec2::ZERO {
        return Ok(false);
    }
    Ok(checked_dot(heading, checked_normalize(velocity)?)? >= COMOVING_MIN_COSINE)
}

/// Sideways weight of `blocker`, zero at its reach and rising continuously as it nears.
fn side_weight(blocker: &SideBlocker, unit_pos: SimVec2, unit_radius: SimScalar) -> SimScalar {
    let combined = unit_radius.saturating_add(blocker.radius);
    let Ok(dist) = distance(unit_pos, blocker.pos) else {
        return SimScalar::ZERO;
    };
    let (margin, touching, overlapping) = if blocker.co_moving {
        (FLOCK_SHELL, FLOCK_WEIGHT_TOUCHING, FLOCK_WEIGHT_OVERLAPPING)
    } else {
        (STEERING_LOOKAHEAD, OBSTACLE_WEIGHT, OBSTACLE_WEIGHT)
    };
    let reach = combined + margin;
    if dist >= reach {
        return SimScalar::ZERO;
    }
    let emergency = combined * EMERGENCY_SEPARATION;
    let radial = if dist < emergency {
        overlapping
    } else if dist < combined {
        let span = combined - emergency;
        let share = if span > SimScalar::ZERO {
            ((combined - dist) / span).min(SimScalar::ONE)
        } else {
            SimScalar::ONE
        };
        touching + (overlapping - touching) * share
    } else {
        touching * ((reach - dist) / margin).min(SimScalar::ONE)
    };

    let half_width = combined + STEERING_CORRIDOR_MARGIN;
    let intrusion = (half_width - blocker.lateral).max(SimScalar::ZERO);
    let lateral = if half_width > SimScalar::ZERO {
        (intrusion / half_width).min(SimScalar::ONE)
    } else {
        SimScalar::ONE
    };
    radial * lateral
}

/// Computes the unit-length local travel direction toward `goal_pos`.
///
/// This is the only steering rule: following a route, closing on a destination and chasing a
/// target all use it, so a unit's path has no seam where one mode hands over to another. Bodies
/// beside the path are ignored, a flockmate ahead is followed rather than dodged, a flockmate
/// alongside nudges gently, and anything else in the way is rounded. Positions and positive radii
/// are planar world units. `ignored_entity` is the unit's own target, which it must not avoid.
///
/// A blocker dead ahead counts as lying to the left, so the unit passes on its right. Two units
/// meeting head-on therefore turn to opposite world sides, and they separate.
///
/// This form knows no owner for the unit, so every body is a separate obstacle. The movement
/// phase steers with [`steer_body`], which also joins two bodies that will not yield into one
/// wall when the unit cannot pass between them.
///
/// # Errors
/// Rejects nonpositive radii or unrepresentable geometry without mutation.
pub fn compute_unified_local_steering(
    unit_id: EntityId,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> Result<SimVec2, PushGeometryError> {
    steer(
        Steered {
            id: unit_id,
            pos: unit_pos,
            radius: unit_radius,
            mover: None,
        },
        goal_pos,
        ignored_entity,
        obstacles,
    )
}

/// [`compute_unified_local_steering`] for a unit of known owner and push priority.
///
/// Two standing bodies that will not step aside for `unit` ([`SteeringObstacle::yields_to`]), the
/// nearest on each side of its path, whose edges are closer than the unit's diameter plus
/// [`STEERING_CORRIDOR_MARGIN`], are one wall, with every further body that closes up to them:
/// the unit cannot pass between them, so it heads for the outside of the wall's nearer end
/// ([`round_wall`]) and never toward a gap (plan
/// `2026-11-09-friendly-blocking` D-1.1, bug B-103).
///
/// # Errors
/// Rejects nonpositive radii or unrepresentable geometry without mutation.
pub(crate) fn steer_body(
    unit: ContactBody,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> Result<SimVec2, PushGeometryError> {
    steer(
        Steered {
            id: unit.id,
            pos: unit.pos,
            radius: unit.radius,
            mover: Some((unit.owner, unit.push_priority)),
        },
        goal_pos,
        ignored_entity,
        obstacles,
    )
}

/// The unit a heading is computed for. `mover` is its owner and push priority when known, which
/// decides which bodies step aside for it.
#[derive(Clone, Copy)]
struct Steered {
    id: EntityId,
    pos: SimVec2,
    radius: SimScalar,
    mover: Option<(PlayerId, u8)>,
}

/// Whether `obs` can be part of a wall for a unit of owner and push priority `mover`: it holds
/// its ground (anchored: holding, immobile or fighting, whatever contact nudges it by) or stands,
/// moved by no more than contact nudges it ([`SteeringObstacle::is_standing`]), and will not step
/// aside. A moving body leaves the way by itself, and steering
/// spaces the unit from it like any flockmate; counting it would make the wall, and so the side the
/// unit rounds it on, change from tick to tick as it moves. Without a known mover nothing is a
/// wall.
fn is_wall_body(obs: &SteeringObstacle, mover: Option<(PlayerId, u8)>) -> bool {
    obs.is_standing() && mover.is_some_and(|(owner, priority)| !obs.yields_to(owner, priority))
}

/// Whether two side blockers that can be part of a wall leave too little room for a unit of
/// `unit_radius` to pass between them, edge to edge.
fn forms_wall(
    left: &SideBlocker,
    right: &SideBlocker,
    unit_radius: SimScalar,
) -> Result<bool, PushGeometryError> {
    let gap = distance(left.pos, right.pos)? - left.radius - right.radius;
    Ok(gap < unit_radius + unit_radius + STEERING_CORRIDOR_MARGIN)
}

/// One end of a wall: the body at it and how far its outer edge lies from the unit's path line
/// on that side, in raw fixed-point units.
#[derive(Clone, Copy)]
struct WallEnd {
    pos: SimVec2,
    radius: SimScalar,
    outer: i128,
}

/// The outermost body of the wall that starts at `first` on the side `sign` (`1` left of
/// `goal_dir`, `-1` right), among `obstacles`.
///
/// The wall continues from a body to any further-out body that can be part of a wall
/// ([`is_wall_body`]) and whose edge lies closer to it than the unit's diameter plus [`STEERING_CORRIDOR_MARGIN`]: the
/// unit could not pass between those two either. Only `obstacles` are known, the unit's
/// neighbours, so a wall that runs on beyond them ends at the last one seen; the unit learns more
/// as it walks.
// PERFORMANCE RATIONALE:
// Why: A wall is a chain of bodies; following it needs the pairwise gaps of the unit's neighbours.
// What: Rescan the neighbour slice once per link, keeping only the current end, with no buffer.
// Trade-off: O(k^2) in the neighbour count k (a handful of bodies within about one body width),
// in exchange for no allocation in the movement phase.
fn wall_end(
    unit: &Steered,
    goal_dir: SimVec2,
    first: &SideBlocker,
    sign: i128,
    obstacles: &[SteeringObstacle],
) -> Result<WallEnd, PushGeometryError> {
    let outer_of = |pos: SimVec2, radius: SimScalar| -> Result<i128, PushGeometryError> {
        let lateral = difference_product(goal_dir, pos, unit.pos, true)? >> 32;
        Ok(lateral * sign + i128::from(radius.raw()))
    };
    let mut end = WallEnd {
        pos: first.pos,
        radius: first.radius,
        outer: outer_of(first.pos, first.radius)?,
    };
    let passable = unit.radius + unit.radius + STEERING_CORRIDOR_MARGIN;
    for _ in 0..obstacles.len() {
        let mut next: Option<WallEnd> = None;
        for obs in obstacles {
            if obs.id == unit.id || obs.pos == end.pos || !is_wall_body(obs, unit.mover) {
                continue;
            }
            let outer = outer_of(obs.pos, obs.radius)?;
            if outer <= end.outer || next.is_some_and(|best| best.outer >= outer) {
                continue;
            }
            let reach = i128::from(end.radius.raw())
                + i128::from(obs.radius.raw())
                + i128::from(passable.raw());
            if within_distance(end.pos, obs.pos, reach) {
                next = Some(WallEnd {
                    pos: obs.pos,
                    radius: obs.radius,
                    outer,
                });
            }
        }
        match next {
            Some(further) => end = further,
            None => break,
        }
    }
    Ok(end)
}

/// The heading past a wall of bodies the unit cannot pass between: toward the point just
/// outside its nearer end.
///
/// Each side's end is the outermost body of the wall on that side ([`wall_end`]); the nearer end
/// is the one whose outer edge lies nearer the unit's path line, and on a tie the left one. The
/// aim lies beside that body, perpendicular to `goal_dir`, at the body's radius plus the unit's
/// plus [`STEERING_CORRIDOR_MARGIN`], so the heading never points into a gap of the wall.
///
/// A unit rounds an end only when the point beside it lies within [`STEERING_LOOKAHEAD`] of the
/// unit: every body that could stand there is then among the unit's neighbours, so the end is
/// known to be one. Otherwise the unit heads straight on: the wall leaves it no admissible step,
/// and it waits (D-1.3). An end chosen from further away changes with every body that comes into
/// or leaves the unit's view, and the unit vibrates; walking round a crowd on a kept side is the
/// surround walk's work (D-2), which remembers the side.
fn round_wall(
    unit: &Steered,
    goal_dir: SimVec2,
    left: &SideBlocker,
    right: &SideBlocker,
    obstacles: &[SteeringObstacle],
) -> Result<SimVec2, PushGeometryError> {
    let tangent_left = SimVec2::new(-goal_dir.y, goal_dir.x);
    let left_end = wall_end(unit, goal_dir, left, 1, obstacles)?;
    let right_end = wall_end(unit, goal_dir, right, -1, obstacles)?;
    let (end, outward) = if left_end.outer <= right_end.outer {
        (left_end, tangent_left)
    } else {
        (right_end, checked_vec_neg(tangent_left)?)
    };
    let clearance = end.radius + unit.radius + STEERING_CORRIDOR_MARGIN;
    let aim = checked_vec_add(end.pos, checked_vec_scale(outward, clearance)?)?;
    if !within_distance(unit.pos, aim, i128::from(STEERING_LOOKAHEAD.raw())) {
        // The way past the end lies beyond what the unit knows of the wall: another body may
        // close it. The unit heads straight into the wall, which leaves no admissible step.
        return Ok(goal_dir);
    }
    direction(aim, unit.pos)
}

/// The bodies in a unit's path, nearest first on each side.
#[derive(Default)]
struct SideBlockers {
    /// Nearest relevant blocker on each side, `[left, right]`, which steering deflects from.
    nearest: [Option<SideBlocker>; 2],
    /// Nearest blocker on each side that can be part of a wall ([`is_wall_body`]).
    walls: [Option<SideBlocker>; 2],
}

/// Finds the nearest blockers on each side of the path from `unit` along `goal_dir`.
fn side_blockers(
    unit: &Steered,
    goal_dir: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> Result<SideBlockers, PushGeometryError> {
    let mut sides = SideBlockers::default();
    for obs in obstacles {
        if obs.id == unit.id || Some(obs.id) == ignored_entity {
            continue;
        }
        if obs.radius <= SimScalar::ZERO {
            return Err(PushGeometryError::InvalidDistance);
        }
        let combined = i128::from(unit.radius.raw()) + i128::from(obs.radius.raw());
        let reach = combined + i128::from(STEERING_LOOKAHEAD.raw());
        if obs.pos == unit.pos || !within_distance(unit.pos, obs.pos, reach) {
            continue;
        }
        let ahead = difference_product(goal_dir, obs.pos, unit.pos, false)?;
        if ahead <= 0 {
            continue;
        }
        let cross = difference_product(goal_dir, obs.pos, unit.pos, true)?;
        let half_width = combined + i128::from(STEERING_CORRIDOR_MARGIN.raw());
        let lateral_raw = cross.unsigned_abs() >> 32;
        if lateral_raw >= half_width.unsigned_abs() {
            continue;
        }
        let side = usize::from(cross < 0);
        let square = squared_distance(unit.pos, obs.pos);
        let co_moving = is_co_moving(goal_dir, obs.velocity)?;
        let candidate = SideBlocker {
            square,
            pos: obs.pos,
            radius: obs.radius,
            lateral: SimScalar::from_raw(i64::try_from(lateral_raw).unwrap_or(i64::MAX)),
            co_moving,
            walls: is_wall_body(obs, unit.mover),
        };
        if candidate.walls && sides.walls[side].is_none_or(|best| square < best.square) {
            sides.walls[side] = Some(candidate);
        }
        if co_moving {
            let is_column_leader = ahead.unsigned_abs() >> 32 > lateral_raw;
            let shell = combined + i128::from(FLOCK_SHELL.raw());
            if is_column_leader || !within_distance(unit.pos, obs.pos, shell) {
                continue;
            }
        }
        if sides.nearest[side].is_none_or(|best| square < best.square) {
            sides.nearest[side] = Some(candidate);
        }
    }
    Ok(sides)
}

/// The side a blocked unit walks round the crowd in its way to `goal_pos` on: `true` for the
/// left of its way, `false` for the right (plan `2026-11-09-friendly-blocking` D-2.4).
///
/// It is the side of the nearer end of the wall in front of the unit, as [`round_wall`] judges
/// it; with a standing body that will not yield on one side only, the other side; and on a tie,
/// or with none, the parity of the unit's ID (even walks left). `ignored_entity` is the unit's
/// target, which is no part of the crowd.
///
/// # Errors
/// Rejects nonpositive radii or unrepresentable geometry.
pub(crate) fn surround_side(
    unit: ContactBody,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> Result<bool, PushGeometryError> {
    let steered = Steered {
        id: unit.id,
        pos: unit.pos,
        radius: unit.radius,
        mover: Some((unit.owner, unit.push_priority)),
    };
    let parity = unit.id.0.is_multiple_of(2);
    let goal_dir = direction(goal_pos, unit.pos)?;
    if goal_dir == SimVec2::ZERO {
        return Ok(parity);
    }
    let sides = side_blockers(&steered, goal_dir, ignored_entity, obstacles)?;
    Ok(match &sides.walls {
        [Some(left), Some(right)] => {
            let left_end = wall_end(&steered, goal_dir, left, 1, obstacles)?.outer;
            let right_end = wall_end(&steered, goal_dir, right, -1, obstacles)?.outer;
            if left_end == right_end {
                parity
            } else {
                left_end < right_end
            }
        }
        [Some(_), None] => false,
        [None, Some(_)] => true,
        [None, None] => parity,
    })
}

/// The steering rule shared by [`compute_unified_local_steering`] and [`steer_body`].
fn steer(
    unit: Steered,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> Result<SimVec2, PushGeometryError> {
    if unit.radius <= SimScalar::ZERO {
        return Err(PushGeometryError::InvalidDistance);
    }
    let goal_dir = direction(goal_pos, unit.pos)?;
    let sides = side_blockers(&unit, goal_dir, ignored_entity, obstacles)?;
    if goal_dir == SimVec2::ZERO {
        return Ok(SimVec2::ZERO);
    }
    if let [Some(left), Some(right)] = &sides.walls {
        if forms_wall(left, right, unit.radius)? {
            return round_wall(&unit, goal_dir, left, right, obstacles);
        }
    }
    let weight_of = |blocker: Option<SideBlocker>| {
        blocker.map_or(SimScalar::ZERO, |b| side_weight(&b, unit.pos, unit.radius))
    };
    let [nearest_left, nearest_right] = sides.nearest;
    let net_weight = weight_of(nearest_right) - weight_of(nearest_left);
    if net_weight == SimScalar::ZERO {
        return Ok(goal_dir);
    }
    let tangent_left = SimVec2::new(-goal_dir.y, goal_dir.x);
    checked_normalize(checked_vec_add(
        goal_dir,
        checked_vec_scale(tangent_left, net_weight)?,
    )?)
}

/// The unit a contact rule is applied for: who it is and which of its neighbours give way to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContactBody {
    pub id: EntityId,
    pub owner: PlayerId,
    pub push_priority: u8,
    pub pos: SimVec2,
    pub radius: SimScalar,
}

/// Removes from `velocity` whatever would drive the unit into a body that will not yield.
///
/// Contact is a constraint, not a force to out-push: a unit presses into nothing that holds its
/// ground, so what remains of its step slides it around the body. An idle friendly of no higher
/// priority does not count, because the mover's contact pushes it aside. A body receding at least
/// as fast as the unit approaches is no obstacle either, which lets columns march closed up.
///
/// The constraint holds for all such bodies at once: the per-body removal is repeated, in the
/// slice's order, until a pass removes nothing, and at most as many passes as there are bodies
/// within reach. Two bodies at an angle then both take their share, so a step into the corner
/// between them is removed whole instead of leaving a residue that contact would undo (plan
/// `2026-11-09-friendly-blocking` D-1.2, bug B-103).
pub(crate) fn remove_blocked_velocity(
    unit: ContactBody,
    mut velocity: SimVec2,
    obstacles: &[SteeringObstacle],
) -> Result<SimVec2, PushGeometryError> {
    let step = velocity.x.raw().unsigned_abs() + velocity.y.raw().unsigned_abs();
    let mut passes = 0;
    loop {
        let (projected, within_reach) = remove_blocked_once(unit, velocity, step, obstacles)?;
        passes += 1;
        if projected == velocity || passes >= within_reach {
            return Ok(projected);
        }
        velocity = projected;
    }
}

/// One pass of [`remove_blocked_velocity`] with reach `step` (raw Manhattan length of the
/// original step): the projected velocity and how many non-yielding bodies lie within reach.
fn remove_blocked_once(
    unit: ContactBody,
    mut velocity: SimVec2,
    step: u64,
    obstacles: &[SteeringObstacle],
) -> Result<(SimVec2, usize), PushGeometryError> {
    let mut within_reach = 0;
    for obs in obstacles {
        if obs.id == unit.id {
            continue;
        }
        if obs.yields_to(unit.owner, unit.push_priority) {
            continue;
        }
        let reach = i128::from(unit.radius.raw()) + i128::from(obs.radius.raw()) + i128::from(step);
        if obs.pos == unit.pos || !within_distance(unit.pos, obs.pos, reach) {
            continue;
        }
        within_reach += 1;
        let toward = direction(obs.pos, unit.pos)?;
        let approach = checked_dot(velocity, toward)?;
        if approach <= SimScalar::ZERO {
            continue;
        }
        // The room left before touching, plus however far the body moves out of the way.
        let gap = (distance(unit.pos, obs.pos)? - (unit.radius + obs.radius)).max(SimScalar::ZERO);
        let room = gap + checked_dot(obs.velocity, toward)?.max(SimScalar::ZERO);
        if room >= approach {
            continue;
        }
        velocity = checked_vec_sub(velocity, checked_vec_scale(toward, approach - room)?)?;
    }
    Ok((velocity, within_reach))
}

/// Checks whether a settled body stops a unit at `unit_pos` heading for `target_pos`.
/// This query alone does not establish world terrain connectivity.
///
/// # Errors
/// Rejects nonpositive radii; widened threshold comparisons never narrow squares.
pub fn check_cluster_arrival(
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    target_pos: SimVec2,
    obstacles: &[SteeringObstacle],
) -> Result<bool, PushGeometryError> {
    check_cluster_arrival_except(None, unit_pos, unit_radius, target_pos, obstacles)
}

/// Checks arrival while excluding the querying actor's identity.
/// This query alone does not establish world terrain connectivity.
///
/// # Errors
/// Rejects nonpositive radii before reporting a settled-neighbor result.
pub fn check_cluster_arrival_except(
    excluded: Option<EntityId>,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    target_pos: SimVec2,
    obstacles: &[SteeringObstacle],
) -> Result<bool, PushGeometryError> {
    check_cluster_arrival_with(
        excluded,
        unit_pos,
        unit_radius,
        target_pos,
        obstacles,
        |_| true,
    )
}

/// Checks arrival with an additional immutable world-policy witness.
///
/// A body stops the unit only if it finished moving to this exact destination: identity, not
/// stillness. Squadmates that have not started, strangers and buildings all stand still without
/// having arrived anywhere, so none of them can end an order. Positions and radii are world units.
/// `witness_allowed` is called only for settled, geometrically eligible, nonexcluded neighbors
/// until the first accepted witness. Every nonexcluded radius is validated even after success.
/// World callers supply pure read-only predicates, so skipping later callbacks hides no effects.
pub(crate) fn check_cluster_arrival_with(
    excluded: Option<EntityId>,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    target_pos: SimVec2,
    obstacles: &[SteeringObstacle],
    mut witness_allowed: impl FnMut(&SteeringObstacle) -> bool,
) -> Result<bool, PushGeometryError> {
    if unit_radius <= SimScalar::ZERO {
        return Err(PushGeometryError::InvalidDistance);
    }
    let mut arrived = false;
    for obs in obstacles {
        if Some(obs.id) == excluded {
            continue;
        }
        if obs.radius <= SimScalar::ZERO {
            return Err(PushGeometryError::InvalidDistance);
        }
        // PERFORMANCE RATIONALE:
        // Why: Ground witnesses may traverse many map cells; later witnesses
        // cannot change an already established arrival result.
        // What: Skip geometry/policy after success, but retain every radius check.
        // Trade-off: Still scan the obstacle slice so malformed state is reported;
        // no additional map queries or scratch allocation after the first witness.
        if arrived || obs.settled_on != Some(target_pos) {
            continue;
        }
        let combined = i128::from(unit_radius.raw()) + i128::from(obs.radius.raw());
        arrived = within_distance(
            obs.pos,
            target_pos,
            combined + i128::from(ARRIVAL_WITNESS_REACH.raw()),
        ) && within_distance(
            unit_pos,
            obs.pos,
            combined + i128::from(ARRIVAL_TOUCH_MARGIN.raw()),
        ) && witness_allowed(obs);
    }
    Ok(arrived)
}

/// Parameters for an entity participating in push collision resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PushEntityParams {
    /// Simulation entity handle.
    pub id: EntityId,
    /// 2D planar position.
    pub pos: SimVec2,
    /// 2D linear velocity.
    pub vel: SimVec2,
    /// Owning player.
    pub owner: PlayerId,
    /// Priority mass used for push resolution.
    pub push_priority: u8,
    /// Whether friendly units may not push this entity right now, including while it fights.
    pub cannot_be_pushed: bool,
    /// Whether nothing may move this entity: Hold Position, immobile or unpushable.
    pub is_holding: bool,
    /// Whether this entity has nothing to do: no order, `Idle` or `Stop`. Only an idle, standing
    /// friendly is pushed aside by a moving one; one standing under an order separates evenly.
    pub idle: bool,
}

impl PushEntityParams {
    /// Creates push parameters for an entity.
    #[must_use]
    pub fn new(
        id: EntityId,
        pos: SimVec2,
        vel: SimVec2,
        owner: PlayerId,
        push_priority: u8,
        cannot_be_pushed: bool,
    ) -> Self {
        Self {
            id,
            pos,
            vel,
            owner,
            push_priority,
            cannot_be_pushed,
            is_holding: cannot_be_pushed,
            idle: true,
        }
    }
}

/// Computes dynamic-body displacement with checked fixed-point arithmetic.
/// Inputs are planar world positions/velocities and radii in world units.
/// `distance` must equal [`crate::movement::checked_distance`] for these positions.
///
/// # Errors
/// Returns invalid-distance or unrepresentable-arithmetic evidence without mutation.
pub fn compute_push_displacement(
    a: PushEntityParams,
    b: PushEntityParams,
    distance: SimScalar,
    combined_radius: SimScalar,
) -> Result<(SimVec2, SimVec2), PushGeometryError> {
    validate_push_distance(a, b, distance, combined_radius)?;
    checked_dynamic_push(a, b, distance, combined_radius)
}

/// Sideways direction, perpendicular to `move_dir`, that carries a body out of the mover's lane.
///
/// `offset` points from the mover to that body. A body exactly on the mover's line has no side,
/// so `flip_on_tie` picks one; everywhere else geometry decides, which keeps the direction
/// continuous so nothing is ever pushed back across the lane.
fn lane_exit(
    move_dir: SimVec2,
    offset: SimVec2,
    flip_on_tie: bool,
) -> Result<SimVec2, PushGeometryError> {
    let perp = SimVec2::new(-move_dir.y, move_dir.x);
    let side = checked_dot(offset, perp)?;
    if side < SimScalar::ZERO || (side == SimScalar::ZERO && flip_on_tie) {
        checked_vec_neg(perp)
    } else {
        Ok(perp)
    }
}

/// Correction for `mover` sliding around a friendly body that may not move.
fn slide_around_fixed(
    mover: PushEntityParams,
    normal: SimVec2,
    offset_from_fixed: SimVec2,
    raw_overlap: SimScalar,
) -> Result<SimVec2, PushGeometryError> {
    if !motion_flags(mover.vel).1 {
        return checked_vec_scale(normal, raw_overlap);
    }
    let move_dir = checked_normalize(mover.vel)?;
    let tangent = lane_exit(move_dir, offset_from_fixed, false)?;
    let slide = checked_normalize(checked_vec_add(
        checked_vec_scale(normal, SimScalar::HALF)?,
        checked_vec_scale(tangent, FIXED_BODY_TANGENT_WEIGHT)?,
    )?)?;
    checked_vec_scale(slide, raw_overlap)
}

/// Resolves contact between two bodies. The rules, in order (the push table of plan
/// `2026-11-09-friendly-blocking` D-4):
/// 1. Bodies that nothing may move stay put, against friend and foe alike.
/// 2. A body that may not be pushed right now stays put and the other takes the whole correction.
/// 3. Enemies separate evenly along the line of centres.
/// 4. A moving friendly walks through an idle, standing friendly of no higher priority, which
///    steps aside.
/// 5. A moving friendly gives way to an idle, standing friendly of higher priority.
/// 6. Everything else, a body waiting under an order included, separates evenly along the line
///    of centres, with no sideways shove.
fn checked_dynamic_push(
    a: PushEntityParams,
    b: PushEntityParams,
    distance: SimScalar,
    combined_radius: SimScalar,
) -> Result<(SimVec2, SimVec2), PushGeometryError> {
    let raw_overlap = combined_radius
        .checked_sub(distance)
        .ok_or(PushGeometryError::Unrepresentable)?;
    if raw_overlap <= SimScalar::ZERO {
        return Ok((SimVec2::ZERO, SimVec2::ZERO));
    }
    let delta = checked_vec_sub(a.pos, b.pos)?;
    let separation = if distance > SimScalar::EPSILON {
        checked_vec_div(delta, distance)?
    } else {
        SimVec2::X_AXIS
    };
    let opposite = checked_vec_neg(separation)?;
    let friendly = a.owner == b.owner;
    let a_anchored = a.is_holding || (friendly && a.cannot_be_pushed);
    let b_anchored = b.is_holding || (friendly && b.cannot_be_pushed);
    match (a.is_holding, b.is_holding) {
        (true, true) => return Ok((SimVec2::ZERO, SimVec2::ZERO)),
        (true, false) if !friendly => {
            return Ok((SimVec2::ZERO, checked_vec_scale(opposite, raw_overlap)?));
        }
        (false, true) if !friendly => {
            return Ok((checked_vec_scale(separation, raw_overlap)?, SimVec2::ZERO));
        }
        _ => {}
    }
    if !friendly {
        let correction = checked_vec_scale(separation, raw_overlap * SimScalar::HALF)?;
        return Ok((correction, checked_vec_neg(correction)?));
    }
    if b_anchored && !a_anchored {
        let correction = slide_around_fixed(a, separation, delta, raw_overlap)?;
        return Ok((correction, SimVec2::ZERO));
    }
    if a_anchored && !b_anchored {
        let correction = slide_around_fixed(b, opposite, checked_vec_neg(delta)?, raw_overlap)?;
        return Ok((SimVec2::ZERO, correction));
    }
    let overlap = raw_overlap
        .checked_mul(PUSH_RELAXATION)
        .ok_or(PushGeometryError::Unrepresentable)?;
    if a_anchored && b_anchored {
        // Two units fighting shoulder to shoulder ease apart. Removing the whole overlap at once
        // throws each out of its firing spot, and in a packed ring that never settles.
        let correction = checked_vec_scale(separation, overlap * SimScalar::HALF)?;
        return Ok((correction, checked_vec_neg(correction)?));
    }
    let (a_moving, b_moving) = (motion_flags(a.vel).0, motion_flags(b.vel).0);
    if a_moving && !b_moving && b.idle {
        if a.push_priority < b.push_priority {
            return Ok((checked_vec_scale(separation, overlap)?, SimVec2::ZERO));
        }
        let exit = lane_exit(
            checked_normalize(a.vel)?,
            checked_vec_neg(delta)?,
            a.id < b.id,
        )?;
        return Ok((SimVec2::ZERO, checked_vec_scale(exit, overlap)?));
    }
    if b_moving && !a_moving && a.idle {
        if b.push_priority < a.push_priority {
            return Ok((SimVec2::ZERO, checked_vec_scale(opposite, overlap)?));
        }
        let exit = lane_exit(checked_normalize(b.vel)?, delta, a.id < b.id)?;
        return Ok((checked_vec_scale(exit, overlap)?, SimVec2::ZERO));
    }
    let correction = checked_vec_scale(separation, overlap * SimScalar::HALF)?;
    Ok((correction, checked_vec_neg(correction)?))
}

// Only classify squared magnitude against zero/epsilon; no physical squared
// value needs narrowing. An overflowing u128 sum is necessarily above both
// thresholds, so that branch is an exact predicate rather than numeric saturation.
fn motion_flags(v: SimVec2) -> (bool, bool) {
    let x = i128::from(v.x.raw()).unsigned_abs();
    let y = i128::from(v.y.raw()).unsigned_abs();
    let Some(square) = (x * x).checked_add(y * y) else {
        return (true, true);
    };
    (
        square >= 1_u128 << 32,
        square >= (u128::from(SimScalar::EPSILON.raw().unsigned_abs()) + 1) << 32,
    )
}

/// Atomically applies checked pair correction, honoring actual immobile/unpushable flags.
/// Callers must establish compatible layers and valid resulting terrain/domain
/// placement; this entity-only API cannot consult registered map foundations.
/// If either actor is carried, returns Ok(()) with both actors unchanged, without
/// evaluating geometry: there is no world-space pair to resolve.
///
/// # Errors
/// Returns typed geometry errors before changing either actor's position.
pub fn resolve_push_interaction(
    moving_unit: &mut Entity,
    other_unit: &mut Entity,
    distance: SimScalar,
    combined_radius: SimScalar,
) -> Result<(), PushGeometryError> {
    // A valid passenger is absent from collision space; no geometry is evaluated.
    if moving_unit.is_loaded_in_transport() || other_unit.is_loaded_in_transport() {
        return Ok(());
    }
    let a = PushEntityParams {
        id: moving_unit.id,
        pos: moving_unit.position,
        vel: moving_unit.velocity,
        owner: moving_unit.owner,
        push_priority: 10,
        cannot_be_pushed: moving_unit.cannot_be_pushed(),
        is_holding: moving_unit.cannot_be_pushed(),
        idle: moving_unit.is_idle(),
    };
    let b = PushEntityParams {
        id: other_unit.id,
        pos: other_unit.position,
        vel: other_unit.velocity,
        owner: other_unit.owner,
        push_priority: 10,
        cannot_be_pushed: other_unit.cannot_be_pushed(),
        is_holding: other_unit.cannot_be_pushed(),
        idle: other_unit.is_idle(),
    };
    let mobility = |e: &Entity| {
        if e.is_immobile || e.unpushable {
            CollisionMobility::Fixed
        } else {
            CollisionMobility::Dynamic
        }
    };
    let (d_a, d_b) = compute_push_displacement_with_mobility(
        a,
        b,
        distance,
        combined_radius,
        mobility(moving_unit),
        mobility(other_unit),
    )?;
    let next_a = checked_vec_add(moving_unit.position, d_a)?;
    let next_b = checked_vec_add(other_unit.position, d_b)?;
    moving_unit.position = next_a;
    other_unit.position = next_b;
    Ok(())
}

/// Positional correction weight for an actual body, separate from friendly priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionMobility {
    /// Unit correction weight; existing priority rules apply between dynamic bodies.
    Dynamic,
    /// Zero correction weight, including registered foundations and immobile bodies.
    Fixed,
}

/// Compatibility name for the shared checked movement arithmetic error.
pub use crate::movement::MovementMathError as PushGeometryError;

/// Computes weighted same-layer circle correction in fixed-point world units.
/// Fixed bodies always receive zero correction, before enemy and priority rules.
/// `distance` must equal the integer-root Euclidean distance between the two positions;
/// `combined_radius` must be nonnegative and actor IDs distinct. Different layers
/// must be filtered by callers. At coincident fixed/dynamic centers, lower ID uses
/// +X normal and higher ID -X, preserving physical direction on argument reversal.
/// The dynamic/dynamic compatibility branch retains its original +X tie convention.
///
/// # Errors
/// Rejects inconsistent/negative inputs and unrepresentable distance, correction or
/// dynamic velocity arithmetic. No entity or world state is mutated by this query.
pub fn compute_push_displacement_with_mobility(
    a: PushEntityParams,
    b: PushEntityParams,
    distance: SimScalar,
    combined_radius: SimScalar,
    mobility_a: CollisionMobility,
    mobility_b: CollisionMobility,
) -> Result<(SimVec2, SimVec2), PushGeometryError> {
    validate_push_distance(a, b, distance, combined_radius)?;
    if distance >= combined_radius
        || (mobility_a == CollisionMobility::Fixed && mobility_b == CollisionMobility::Fixed)
    {
        return Ok((SimVec2::ZERO, SimVec2::ZERO));
    }
    if mobility_a == CollisionMobility::Dynamic && mobility_b == CollisionMobility::Dynamic {
        return checked_dynamic_push(a, b, distance, combined_radius);
    }
    let normal = if distance > SimScalar::EPSILON {
        SimVec2::new(
            a.pos
                .x
                .checked_sub(b.pos.x)
                .and_then(|v| v.checked_div(distance))
                .ok_or(PushGeometryError::Unrepresentable)?,
            a.pos
                .y
                .checked_sub(b.pos.y)
                .and_then(|v| v.checked_div(distance))
                .ok_or(PushGeometryError::Unrepresentable)?,
        )
    } else if a.id < b.id {
        SimVec2::X_AXIS
    } else {
        -SimVec2::X_AXIS
    };
    let overlap = combined_radius
        .checked_sub(distance)
        .ok_or(PushGeometryError::Unrepresentable)?;
    let correction = SimVec2::new(
        normal
            .x
            .checked_mul(overlap)
            .ok_or(PushGeometryError::Unrepresentable)?,
        normal
            .y
            .checked_mul(overlap)
            .ok_or(PushGeometryError::Unrepresentable)?,
    );
    Ok(if mobility_a == CollisionMobility::Fixed {
        (SimVec2::ZERO, -correction)
    } else {
        (correction, SimVec2::ZERO)
    })
}

fn validate_push_distance(
    a: PushEntityParams,
    b: PushEntityParams,
    distance: SimScalar,
    combined_radius: SimScalar,
) -> Result<(), PushGeometryError> {
    if a.id == b.id {
        return Err(PushGeometryError::DuplicateEntity);
    }
    if distance < SimScalar::ZERO || combined_radius < SimScalar::ZERO {
        return Err(PushGeometryError::InvalidDistance);
    }
    let actual = crate::movement::checked_distance(a.pos, b.pos)
        .ok_or(PushGeometryError::Unrepresentable)?;
    if distance != actual {
        return Err(PushGeometryError::InvalidDistance);
    }
    Ok(())
}

#[cfg(test)]
mod weighted_tests {
    use super::*;
    fn body(id: u32, x: i32, y: i32) -> PushEntityParams {
        PushEntityParams {
            id: EntityId(id),
            pos: SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y)),
            vel: SimVec2::ZERO,
            owner: PlayerId(0),
            push_priority: 0,
            cannot_be_pushed: false,
            is_holding: false,
            idle: true,
        }
    }
    #[test]
    fn fixed_normal_uses_wide_geometry_and_full_dynamic_weight() -> Result<(), PushGeometryError> {
        let a = body(1, 30_000, 40_000);
        let b = body(2, 0, 0);
        let d = SimScalar::from_num(50_000);
        let radius = SimScalar::from_num(50_010);
        let (da, db) = compute_push_displacement_with_mobility(
            a,
            b,
            d,
            radius,
            CollisionMobility::Fixed,
            CollisionMobility::Dynamic,
        )?;
        assert_eq!(da, SimVec2::ZERO);
        assert!((db.x - SimScalar::from_num(-6)).abs() < SimScalar::from_raw(16));
        assert!((db.y - SimScalar::from_num(-8)).abs() < SimScalar::from_raw(16));
        let reversed = compute_push_displacement_with_mobility(
            b,
            a,
            d,
            radius,
            CollisionMobility::Dynamic,
            CollisionMobility::Fixed,
        )?;
        assert_eq!(reversed, (db, da));
        assert_eq!(
            compute_push_displacement_with_mobility(
                a,
                b,
                d,
                radius,
                CollisionMobility::Fixed,
                CollisionMobility::Fixed
            )?,
            (SimVec2::ZERO, SimVec2::ZERO)
        );
        assert_eq!(
            compute_push_displacement_with_mobility(
                a,
                b,
                d,
                d,
                CollisionMobility::Fixed,
                CollisionMobility::Dynamic
            )?,
            (SimVec2::ZERO, SimVec2::ZERO)
        );
        Ok(())
    }
    #[test]
    fn invalid_supplied_distance_is_explicit_and_coincident_fixed_body_stays_fixed(
    ) -> Result<(), PushGeometryError> {
        let a = body(1, 0, 0);
        let b = body(2, 0, 0);
        assert_eq!(
            compute_push_displacement_with_mobility(
                a,
                b,
                SimScalar::ONE,
                SimScalar::ONE,
                CollisionMobility::Fixed,
                CollisionMobility::Dynamic
            ),
            Err(PushGeometryError::InvalidDistance)
        );
        assert_eq!(
            compute_push_displacement_with_mobility(
                a,
                b,
                -SimScalar::ONE,
                SimScalar::ONE,
                CollisionMobility::Fixed,
                CollisionMobility::Dynamic
            ),
            Err(PushGeometryError::InvalidDistance)
        );
        assert_eq!(
            compute_push_displacement_with_mobility(
                a,
                b,
                SimScalar::ZERO,
                SimScalar::ONE,
                CollisionMobility::Fixed,
                CollisionMobility::Dynamic
            )?,
            (SimVec2::ZERO, -SimVec2::X_AXIS)
        );
        Ok(())
    }
}

#[cfg(test)]
mod wall_tests {
    use super::*;

    /// 0.375 world units.
    const RADIUS: SimScalar = SimScalar::from_raw(3 << 29);

    /// A unit of player 0, priority 10, radius 0.375, at the origin.
    fn mover() -> ContactBody {
        ContactBody {
            id: EntityId(1),
            owner: PlayerId(0),
            push_priority: 10,
            pos: SimVec2::ZERO,
            radius: RADIUS,
        }
    }

    /// A friendly body of radius 0.375 at (`x`, `y`) that holds its ground.
    fn anchored(id: u32, x: f32, y: f32) -> SteeringObstacle {
        SteeringObstacle {
            anchored: true,
            push_priority: 10,
            ..SteeringObstacle::new(
                EntityId(id),
                SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y)),
                RADIUS,
                SimVec2::ZERO,
            )
        }
    }

    #[test]
    fn a_gap_narrower_than_the_unit_is_a_wall_and_a_wider_one_is_not(
    ) -> Result<(), PushGeometryError> {
        let goal = SimVec2::new(SimScalar::from_num(10), SimScalar::ZERO);
        // Edges 0.70 apart: below the unit's 0.75 diameter plus 0.05, so one wall. The unit
        // stands in front of its middle, far from either end, and heads straight into it.
        let narrow = [anchored(2, 0.6, 0.70), anchored(3, 0.6, -0.75)];
        assert_eq!(steer_body(mover(), goal, None, &narrow)?, SimVec2::X_AXIS);
        // Edges 0.81 apart (0.80 sits on the boundary, where fixed-point rounding decides):
        // passable, so each body is rounded on its own and the unequal
        // offsets deflect the unit.
        let wide = [anchored(2, 0.6, 0.775), anchored(3, 0.6, -0.785)];
        assert_ne!(steer_body(mover(), goal, None, &wide)?, SimVec2::X_AXIS);
        // Without a known mover nothing is a wall.
        let unknown = compute_unified_local_steering(
            EntityId(1),
            SimVec2::ZERO,
            RADIUS,
            goal,
            None,
            &narrow,
        )?;
        assert_ne!(unknown, SimVec2::X_AXIS);
        Ok(())
    }

    #[test]
    fn a_unit_at_the_end_of_a_wall_rounds_it_on_the_outside() -> Result<(), PushGeometryError> {
        let goal = SimVec2::new(SimScalar::from_num(10), SimScalar::ZERO);
        // The unit stands almost beside the wall's left end, which a short step clears.
        let wall = [
            anchored(2, 0.45, -0.05),
            anchored(3, 0.45, -0.8),
            anchored(4, 0.45, 0.70),
        ];
        let heading = steer_body(mover(), goal, None, &wall[..2])?;
        assert!(
            heading.y > SimScalar::ZERO,
            "rounds the left end: {heading:?}"
        );
        // A third body closing the left end makes the right end the nearer one, out of reach,
        // so the unit heads straight on and waits.
        assert_eq!(steer_body(mover(), goal, None, &wall)?, SimVec2::X_AXIS);
        Ok(())
    }

    #[test]
    fn a_step_into_a_corner_between_two_bodies_is_removed_whole() -> Result<(), PushGeometryError> {
        // Two bodies touching the unit at right angles: a step into the corner is all removed.
        let corner = [anchored(2, 0.75, 0.0), anchored(3, 0.0, 0.75)];
        let step = SimVec2::new(SimScalar::from_num(0.1), SimScalar::from_num(0.1));
        assert_eq!(
            remove_blocked_velocity(mover(), step, &corner)?,
            SimVec2::ZERO
        );
        // A wedge of 60 degrees: what remains approaches neither body.
        let (c, h) = (0.75 * 0.866_025_4, 0.75 * 0.5);
        let wedge = [anchored(2, c, h), anchored(3, c, -h)];
        let forward = SimVec2::new(SimScalar::from_num(0.1), SimScalar::from_num(0.01));
        let left = remove_blocked_velocity(mover(), forward, &wedge)?;
        for body in &wedge {
            let toward = direction(body.pos, SimVec2::ZERO)?;
            assert!(
                checked_dot(left, toward)? <= SimScalar::from_raw(1 << 8),
                "{left:?}"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod cluster_arrival_tests {
    use super::*;
    use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, RampDir};

    /// A body that finished moving to `goal` and now stands at `pos`.
    fn witness(goal: SimVec2, id: u32, pos: SimVec2) -> SteeringObstacle {
        let mut body = SteeringObstacle::new(EntityId(id), pos, SimScalar::HALF, SimVec2::ZERO);
        body.settled_on = Some(goal);
        body
    }

    #[test]
    fn arrival_policy_only_visits_eligible_witnesses_until_success() -> Result<(), PushGeometryError>
    {
        let origin = SimVec2::from_i32(4, 4);
        let goal = origin + SimVec2::X_AXIS;
        let mut excluded = witness(goal, 1, origin);
        excluded.radius = SimScalar::ZERO;
        // Stillness is not arrival: this body stands on the goal without having settled there.
        let mut moving = witness(goal, 2, goal);
        moving.settled_on = None;
        let far = witness(goal, 3, SimVec2::from_i32(30, 30));
        let candidates = [
            excluded,
            moving,
            far,
            witness(goal, 4, goal),
            witness(goal, 5, goal),
            witness(goal, 6, goal),
        ];
        let mut visited = Vec::new();
        assert!(check_cluster_arrival_with(
            Some(EntityId(1)),
            origin,
            SimScalar::HALF,
            goal,
            &candidates,
            |obs| {
                visited.push(obs.id);
                obs.id == EntityId(5)
            },
        )?);
        assert_eq!(visited, [EntityId(4), EntityId(5)]);
        assert!(!check_cluster_arrival_with(
            Some(EntityId(1)),
            origin,
            SimScalar::HALF,
            goal,
            &candidates,
            |_| false,
        )?);
        // The public compatibility query deliberately remains geometry-only.
        assert!(check_cluster_arrival_except(
            Some(EntityId(1)),
            origin,
            SimScalar::HALF,
            goal,
            &candidates,
        )?);
        Ok(())
    }

    #[test]
    fn later_malformed_radius_survives_success_and_moving_neighbor_short_circuit() {
        let origin = SimVec2::from_i32(4, 4);
        let goal = origin + SimVec2::X_AXIS;
        let mut invalid = witness(goal, 3, goal);
        invalid.radius = SimScalar::ZERO;
        invalid.settled_on = None;
        let mut count = 0;
        assert_eq!(
            check_cluster_arrival_with(
                Some(EntityId(1)),
                origin,
                SimScalar::HALF,
                goal,
                &[witness(goal, 2, goal), invalid],
                |_| {
                    count += 1;
                    true
                },
            ),
            Err(PushGeometryError::InvalidDistance)
        );
        assert_eq!(count, 1);
        assert_eq!(
            check_cluster_arrival_with(None, origin, SimScalar::ZERO, goal, &[], |_| {
                count += 1;
                true
            },),
            Err(PushGeometryError::InvalidDistance)
        );
        assert_eq!(count, 1);
    }

    #[test]
    fn reversed_candidates_preserve_errors_and_valid_ramp_witnesses(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let center =
            |x, y| SimVec2::from_i32(x, y) + SimVec2::new(SimScalar::HALF, SimScalar::HALF);
        for y in 5..8 {
            let cell = map.cell_mut(7, y)?;
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
            cell.ramp = Some(RampDir::North);
        }
        map.cell_mut(7, 8)?.cliff_level = CliffLevel(1);
        let origin = center(7, 4);
        let goal = center(7, 6);
        let valid = witness(goal, 2, center(7, 5));
        let side = witness(goal, 3, center(8, 5));
        let allowed = |w: &SteeringObstacle| {
            map.ground_segment_is_connected(origin, w.pos, |_, _| true)
                && map.ground_segment_is_connected(w.pos, goal, |_, _| true)
        };
        assert!(allowed(&valid));
        assert!(!allowed(&side));
        // Radius1 admits both differently distant candidates geometrically.
        for candidates in [[side, valid], [valid, side]] {
            assert!(check_cluster_arrival_with(
                None,
                origin,
                SimScalar::ONE,
                goal,
                &candidates,
                allowed
            )?);
        }
        let mut invalid = side;
        invalid.radius = SimScalar::ZERO;
        for candidates in [[invalid, valid], [valid, invalid]] {
            assert_eq!(
                check_cluster_arrival_with(
                    None,
                    origin,
                    SimScalar::ONE,
                    goal,
                    &candidates,
                    allowed
                ),
                Err(PushGeometryError::InvalidDistance)
            );
        }
        Ok(())
    }

    #[test]
    fn terrain_witness_rejects_diagonal_cliffs_and_malformed_ramps(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
        let center =
            |x, y| SimVec2::from_i32(x, y) + SimVec2::new(SimScalar::HALF, SimScalar::HALF);
        let origin = center(4, 4);
        let goal = center(5, 5);
        // Larger generic radius admits the diagonal geometrically; the two
        // independent cardinal routes must still agree on terrain connectivity.
        let mut obs = witness(goal, 2, goal);
        obs.radius = SimScalar::ONE;
        map.cell_mut(5, 4)?.cliff_level = CliffLevel(1);
        assert!(check_cluster_arrival(origin, SimScalar::ONE, goal, &[obs])?);
        assert!(!check_cluster_arrival_with(
            None,
            origin,
            SimScalar::ONE,
            goal,
            &[obs],
            |w| {
                map.ground_segment_is_connected(origin, w.pos, |_, _| true)
                    && map.ground_segment_is_connected(w.pos, goal, |_, _| true)
            }
        )?);
        map.cell_mut(5, 4)?.cliff_level = CliffLevel(0);
        assert!(check_cluster_arrival_with(
            None,
            origin,
            SimScalar::ONE,
            goal,
            &[obs],
            |w| {
                map.ground_segment_is_connected(origin, w.pos, |_, _| true)
                    && map.ground_segment_is_connected(w.pos, goal, |_, _| true)
            }
        )?);
        let cell = map.cell_mut(5, 5)?;
        cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        cell.ramp = Some(RampDir::NorthEast);
        assert!(!check_cluster_arrival_with(
            None,
            origin,
            SimScalar::ONE,
            goal,
            &[obs],
            |w| {
                map.ground_segment_is_connected(origin, w.pos, |_, _| true)
                    && map.ground_segment_is_connected(w.pos, goal, |_, _| true)
            }
        )?);
        Ok(())
    }
}
