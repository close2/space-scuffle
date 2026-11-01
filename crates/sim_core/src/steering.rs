//! # Local Steering, Spatial Hash Grid & Push Priority
//!
//! Implements multi-layer local steering:
//! 1. Macro NavMesh funnel waypoint tracking.
//! 2. Spatial Hash Grid for O(1) neighborhood collision queries.
//! 3. Boid separation and concave combat firing arc formation.
//! 4. Friendly push-priority yielding vs. HoldPosition hard blocking.
//!
//! ## Invariants Maintained
//! 1. All calculations use deterministic fixed-point math (`SimScalar`, `SimVec2`).
//! 2. Neighborhood queries return strictly sorted candidates for deterministic iteration.
//! 3. Reused grid/output capacity avoids repeated allocation after workload warmup;
//!    bucket growth and existing world route/collision scratch allocations remain measurable.

use crate::entity::Entity;
use crate::movement::math::{
    checked_dot, checked_normalize, checked_vec_add, checked_vec_div, checked_vec_neg,
    checked_vec_scale, checked_vec_sub, difference_product, direction, distance, squared_distance,
    within_distance,
};
use sim_types::{EntityId, SimScalar, SimVec2};

// PERFORMANCE RATIONALE:
// Why: Naive O(N^2) pairwise distance checks for 400 units perform 160,000 checks per tick.
// What: Partition 2D world space into a flat spatial hash grid with 2.0x2.0 cell sizing.
// Trade-off: Memory pre-allocated once during world initialization; neighborhood queries
// run in O(1) time per entity, reducing pairwise checks to O(N).

/// Default cell size in world units for the spatial hash grid.
pub const SPATIAL_CELL_SIZE: usize = 2;

/// Default grid width in 1x1 cells.
pub const GRID_WIDTH: usize = 128;

/// Default grid height in 1x1 cells.
pub const GRID_HEIGHT: usize = 128;

/// 2D flat spatial hash grid accelerating entity neighborhood queries.
#[derive(Debug, Clone)]
pub struct SpatialHashGrid {
    cell_size: usize,
    width_cells: usize,
    height_cells: usize,
    buckets: Vec<Vec<EntityId>>,
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
        }
    }

    /// Clears all entity references from the spatial hash grid.
    pub fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
    }

    /// Inserts an entity into the grid bucket corresponding to its world position.
    pub fn insert(&mut self, id: EntityId, pos: SimVec2) {
        let x_int = pos.x.raw() >> 32;
        let y_int = pos.y.raw() >> 32;

        let cx = (x_int.max(0) as usize / self.cell_size).min(self.width_cells.saturating_sub(1));
        let cy = (y_int.max(0) as usize / self.cell_size).min(self.height_cells.saturating_sub(1));

        let idx = cy * self.width_cells + cx;
        if idx < self.buckets.len() {
            self.buckets[idx].push(id);
        }
    }

    /// Queries all entities located in buckets overlapping a circle centered at `pos` with `radius`.
    /// Results are populated into `out` and sorted deterministically.
    pub fn query_radius(&self, pos: SimVec2, radius: SimScalar, out: &mut Vec<EntityId>) {
        out.clear();
        let r_ceil = (radius.raw() >> 32).max(1) as usize;
        let x_int = (pos.x.raw() >> 32).max(0) as usize;
        let y_int = (pos.y.raw() >> 32).max(0) as usize;

        let min_x =
            (x_int.saturating_sub(r_ceil) / self.cell_size).min(self.width_cells.saturating_sub(1));
        let max_x = ((x_int + r_ceil) / self.cell_size).min(self.width_cells.saturating_sub(1));
        let min_y = (y_int.saturating_sub(r_ceil) / self.cell_size)
            .min(self.height_cells.saturating_sub(1));
        let max_y = ((y_int + r_ceil) / self.cell_size).min(self.height_cells.saturating_sub(1));

        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                let idx = cy * self.width_cells + cx;
                if let Some(bucket) = self.buckets.get(idx) {
                    out.extend_from_slice(bucket);
                }
            }
        }

        // Guarantee deterministic candidate ordering
        out.sort_unstable();
        out.dedup();
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

/// Parameters of a candidate obstacle entity for local avoidance and fan-out steering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SteeringObstacle {
    /// Entity simulation identifier.
    pub id: EntityId,
    /// 2D planar position in continuous world space.
    pub pos: SimVec2,
    /// Collision radius.
    pub radius: SimScalar,
    /// Current linear velocity.
    pub velocity: SimVec2,
}

#[derive(Clone, Copy, Debug)]
struct BlockerCandidate {
    square: (u128, u128),
    pos: SimVec2,
    radius: SimScalar,
    velocity: SimVec2,
    lat: SimScalar,
}

fn calc_steering_weight(
    blocker: &BlockerCandidate,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    goal_dir: SimVec2,
    is_corridor: bool,
) -> SimScalar {
    let obs_forward =
        difference_product(goal_dir, blocker.velocity, SimVec2::ZERO, false).unwrap_or(0);
    let obs_fwd_scalar = SimScalar::from_raw((obs_forward >> 32) as i64);
    let is_co_moving = is_corridor
        && blocker.velocity != SimVec2::ZERO
        && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15);

    let combined = unit_radius.saturating_add(blocker.radius);
    let Ok(dist) = distance(unit_pos, blocker.pos) else {
        return SimScalar::ZERO;
    };
    let lookahead_margin = if is_co_moving {
        SimScalar::from_f32_lossy(0.08)
    } else {
        SimScalar::from_f32_lossy(0.35)
    };
    let threshold = combined + lookahead_margin;
    if dist >= threshold {
        return SimScalar::ZERO;
    }
    let sep_thresh = combined;
    let emergency_min = combined * SimScalar::from_f32_lossy(0.85);

    let corridor_half_width = combined + SimScalar::from_f32_lossy(0.05);
    let lat_pen = (corridor_half_width - blocker.lat).max(SimScalar::ZERO);
    let lat_t = if corridor_half_width > SimScalar::ZERO {
        (lat_pen / corridor_half_width).min(SimScalar::ONE)
    } else {
        SimScalar::ONE
    };

    let (w_touch, w_max) = if is_co_moving {
        (
            SimScalar::from_f32_lossy(0.25),
            SimScalar::from_f32_lossy(0.35),
        )
    } else {
        (
            SimScalar::from_f32_lossy(0.85),
            SimScalar::from_f32_lossy(0.85),
        )
    };

    let radial_w = if dist < emergency_min {
        w_max
    } else if dist < sep_thresh {
        let pen = (sep_thresh - dist).max(SimScalar::ZERO);
        let range = sep_thresh - emergency_min;
        let frac = if range > SimScalar::ZERO {
            (pen / range).min(SimScalar::ONE)
        } else {
            SimScalar::ONE
        };
        w_touch + (w_max - w_touch) * frac
    } else {
        let pen = (threshold - dist).max(SimScalar::ZERO);
        let range = threshold - sep_thresh;
        let frac = if range > SimScalar::ZERO {
            (pen / range).min(SimScalar::ONE)
        } else {
            SimScalar::ZERO
        };
        w_touch * frac
    };

    let effective_lat_t = if !is_corridor && dist < sep_thresh {
        SimScalar::from_f32_lossy(0.75) + SimScalar::from_f32_lossy(0.25) * lat_t
    } else {
        lat_t
    };

    radial_w * effective_lat_t
}

/// Computes normalized local travel direction with deterministic nearest-blocker steering.
/// Positions and positive radii are planar world units. Input obstacle order resolves
/// equal-distance ties; world callers supply stable EntityId order.
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
    if unit_radius <= SimScalar::ZERO {
        return Err(PushGeometryError::InvalidDistance);
    }
    let goal_dir = direction(goal_pos, unit_pos)?;
    let mut closest_blocker: Option<((u128, u128), SimVec2, EntityId)> = None;
    let lookahead = SimScalar::from_f32_lossy(0.35);
    for obs in obstacles {
        if obs.id == unit_id || Some(obs.id) == ignored_entity {
            continue;
        }
        if obs.radius <= SimScalar::ZERO {
            return Err(PushGeometryError::InvalidDistance);
        }
        let threshold = i128::from(unit_radius.raw())
            + i128::from(obs.radius.raw())
            + i128::from(lookahead.raw());
        if obs.pos == unit_pos || !within_distance(unit_pos, obs.pos, threshold) {
            continue;
        }
        if difference_product(goal_dir, obs.pos, unit_pos, false)? <= 0 {
            continue;
        }
        let square = squared_distance(unit_pos, obs.pos);
        if closest_blocker.is_none()
            || closest_blocker
                .as_ref()
                .is_some_and(|(best, _, _)| square < *best)
        {
            closest_blocker = Some((square, obs.pos, obs.id));
        }
    }
    if goal_dir == SimVec2::ZERO {
        return Ok(SimVec2::ZERO);
    }
    if let Some((_, pos, id)) = closest_blocker {
        let cross = difference_product(goal_dir, pos, unit_pos, true)?;
        let deadzone = (SimScalar::from_f32_lossy(0.05).raw() as u128) << 32;
        let left = if cross.unsigned_abs() > deadzone {
            cross < 0
        } else {
            unit_id.0 < id.0
        };
        let tangent = if left {
            SimVec2::new(-goal_dir.y, goal_dir.x)
        } else {
            SimVec2::new(goal_dir.y, -goal_dir.x)
        };
        checked_normalize(checked_vec_add(
            goal_dir,
            checked_vec_scale(tangent, SimScalar::from_f32_lossy(0.85))?,
        )?)
    } else {
        Ok(goal_dir)
    }
}

/// Computes normalized local travel direction with corridor-awareness.
/// When `is_corridor` is true, units following intermediate corridor waypoints
/// treat front squadmates as column leaders to march smoothly without lateral jitter.
///
/// # Errors
/// Rejects nonpositive radii or unrepresentable geometry without mutation.
pub fn compute_unified_local_steering_ext(
    unit_id: EntityId,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
    is_corridor: bool,
) -> Result<SimVec2, PushGeometryError> {
    if !is_corridor {
        return compute_unified_local_steering(
            unit_id,
            unit_pos,
            unit_radius,
            goal_pos,
            ignored_entity,
            obstacles,
        );
    }
    if unit_radius <= SimScalar::ZERO {
        return Err(PushGeometryError::InvalidDistance);
    }
    let goal_dir = direction(goal_pos, unit_pos)?;
    if goal_dir == SimVec2::ZERO {
        return Ok(SimVec2::ZERO);
    }

    let mut closest_left: Option<BlockerCandidate> = None;
    let mut closest_right: Option<BlockerCandidate> = None;
    let lookahead = SimScalar::from_f32_lossy(0.35);

    for obs in obstacles {
        if obs.id == unit_id || Some(obs.id) == ignored_entity {
            continue;
        }
        if obs.radius <= SimScalar::ZERO {
            return Err(PushGeometryError::InvalidDistance);
        }
        let combined = i128::from(unit_radius.raw()) + i128::from(obs.radius.raw());
        let threshold = combined + i128::from(lookahead.raw());
        if obs.pos == unit_pos || !within_distance(unit_pos, obs.pos, threshold) {
            continue;
        }
        let proj = difference_product(goal_dir, obs.pos, unit_pos, false)?;
        if proj <= 0 {
            continue;
        }
        let cross = difference_product(goal_dir, obs.pos, unit_pos, true)?;
        let corridor_half_width = combined + i128::from(SimScalar::from_f32_lossy(0.05).raw());
        if (cross.unsigned_abs() >> 32) >= corridor_half_width as u128 {
            continue;
        }
        let obs_forward =
            difference_product(goal_dir, obs.velocity, SimVec2::ZERO, false).unwrap_or(0);
        let obs_fwd_scalar = SimScalar::from_raw((obs_forward >> 32) as i64);
        let lat_raw = (cross.unsigned_abs() >> 32) as i64;
        let proj_raw = (proj >> 32) as i64;

        if is_corridor
            && obs.velocity != SimVec2::ZERO
            && obs_fwd_scalar >= -SimScalar::from_f32_lossy(0.15)
        {
            let is_column_leader = proj_raw > lat_raw;
            let boid_threshold = combined + i128::from(SimScalar::from_f32_lossy(0.08).raw());
            let is_outside_boid_shell = !within_distance(unit_pos, obs.pos, boid_threshold);
            if is_column_leader || is_outside_boid_shell {
                continue;
            }
        }

        let is_left_obstacle = if cross != 0 {
            cross > 0
        } else {
            unit_id.0 >= obs.id.0
        };

        let square = squared_distance(unit_pos, obs.pos);
        let lat = SimScalar::from_raw((cross.unsigned_abs() >> 32) as i64);
        let candidate = BlockerCandidate {
            square,
            pos: obs.pos,
            radius: obs.radius,
            velocity: obs.velocity,
            lat,
        };

        if is_left_obstacle {
            if closest_left.is_none()
                || closest_left
                    .as_ref()
                    .is_some_and(|best| square < best.square)
            {
                closest_left = Some(candidate);
            }
        } else if closest_right.is_none()
            || closest_right
                .as_ref()
                .is_some_and(|best| square < best.square)
        {
            closest_right = Some(candidate);
        }
    }

    let weight_left = closest_left.as_ref().map_or(SimScalar::ZERO, |b| {
        calc_steering_weight(b, unit_pos, unit_radius, goal_dir, is_corridor)
    });
    let weight_right = closest_right.as_ref().map_or(SimScalar::ZERO, |b| {
        calc_steering_weight(b, unit_pos, unit_radius, goal_dir, is_corridor)
    });

    let net_weight = weight_right - weight_left;

    if net_weight == SimScalar::ZERO {
        return Ok(goal_dir);
    }

    let tangent_left = SimVec2::new(-goal_dir.y, goal_dir.x);
    let lateral_offset = checked_vec_scale(tangent_left, net_weight)?;
    checked_normalize(checked_vec_add(goal_dir, lateral_offset)?)
}

/// Checks geometric settled-neighbor arrival in world units, without self-exclusion.
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

/// Checks geometric arrival while excluding the querying actor's identity.
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

/// Checks geometric arrival with an additional immutable world-policy witness.
/// Positions/radii are world units. `witness_allowed` is called only for settled,
/// geometrically eligible, nonexcluded neighbors until the first accepted witness.
/// Every nonexcluded radius is validated even after success; invalid radii return
/// the same error as the geometry-only public queries. World callers supply pure
/// read-only predicates, so skipping later callbacks cannot hide side effects.
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
        if arrived || obs.velocity != SimVec2::ZERO {
            continue;
        }
        let combined = i128::from(unit_radius.raw()) + i128::from(obs.radius.raw());
        arrived = within_distance(
            obs.pos,
            target_pos,
            combined + i128::from(SimScalar::from_f32_lossy(1.5).raw()),
        ) && within_distance(
            unit_pos,
            obs.pos,
            combined + i128::from(SimScalar::from_f32_lossy(0.15).raw()),
        ) && witness_allowed(obs);
    }
    Ok(arrived)
}

// PERFORMANCE RATIONALE:
// Why: RTS armies gridlock in narrow chokes if friendly units act as rigid bodies.
// What: Moving friendly units gently displace idle friendly units perpendicular to the path.
// HoldPosition units and enemy units act as unyielding hard obstacles.

/// Relaxation coefficient for Baumgarte push overlap resolution (0.35 resolves overlaps smoothly over ~3 ticks).
pub const PUSH_RELAXATION: SimScalar = SimScalar::from_raw(1_503_238_554);

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
    pub owner: sim_types::PlayerId,
    /// Priority mass used for push resolution.
    pub push_priority: u8,
    /// Whether the entity cannot be pushed aside by friendly units (HoldPosition, is_immobile, unpushable).
    pub cannot_be_pushed: bool,
    /// Backward-compatibility alias for `cannot_be_pushed`.
    pub is_holding: bool,
}

impl PushEntityParams {
    /// Creates push parameters for an entity.
    #[must_use]
    pub fn new(
        id: EntityId,
        pos: SimVec2,
        vel: SimVec2,
        owner: sim_types::PlayerId,
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
        }
    }
}

/// Computes dynamic-body displacement with checked fixed-point arithmetic.
/// Inputs are planar world positions/velocities and radii in world units.
/// `distance` must equal [`crate::movement::checked_distance`] for these positions.
/// Priority/hold branches retain their meaning; widened normalization can change
/// final raw bits compared with the former squared-Q32 normalization.
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
    if a.owner != b.owner {
        let correction = checked_vec_scale(separation, raw_overlap * SimScalar::HALF)?;
        return Ok((correction, checked_vec_neg(correction)?));
    }
    let a_immobile = a.cannot_be_pushed || a.is_holding;
    let b_immobile = b.cannot_be_pushed || b.is_holding;
    let (a_nonzero, a_moving) = motion_flags(a.vel);
    let (b_nonzero, b_moving) = motion_flags(b.vel);
    if b_immobile && !a_immobile {
        let normal = if distance > SimScalar::EPSILON {
            separation
        } else {
            SimVec2::X_AXIS
        };
        let direction = if a_moving {
            let move_dir = checked_normalize(a.vel)?;
            let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
            if checked_dot(delta, perp)? < SimScalar::ZERO {
                perp = checked_vec_neg(perp)?;
            }
            checked_normalize(checked_vec_add(
                checked_vec_scale(normal, SimScalar::HALF)?,
                checked_vec_scale(perp, SimScalar::from_f32_lossy(0.85))?,
            )?)?
        } else {
            normal
        };
        return Ok((checked_vec_scale(direction, raw_overlap)?, SimVec2::ZERO));
    }
    if a_immobile && !b_immobile {
        let normal = checked_vec_neg(separation)?;
        let direction = if b_moving {
            let move_dir = checked_normalize(b.vel)?;
            let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
            if checked_dot(checked_vec_neg(delta)?, perp)? < SimScalar::ZERO {
                perp = checked_vec_neg(perp)?;
            }
            checked_normalize(checked_vec_add(
                checked_vec_scale(normal, SimScalar::HALF)?,
                checked_vec_scale(perp, SimScalar::from_f32_lossy(0.85))?,
            )?)?
        } else {
            normal
        };
        return Ok((SimVec2::ZERO, checked_vec_scale(direction, raw_overlap)?));
    }
    if a_immobile && b_immobile {
        let correction = checked_vec_scale(separation, raw_overlap * SimScalar::HALF)?;
        return Ok((correction, checked_vec_neg(correction)?));
    }
    let overlap = raw_overlap
        .checked_mul(PUSH_RELAXATION)
        .ok_or(PushGeometryError::Unrepresentable)?;
    if !b_nonzero && a_nonzero && a.push_priority >= b.push_priority {
        let move_dir = checked_normalize(a.vel)?;
        let mut perp = if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON {
            SimVec2::new(-move_dir.y, move_dir.x)
        } else {
            SimVec2::Y_AXIS
        };
        let dot = checked_dot(checked_vec_neg(delta)?, perp)?;
        if dot.raw().unsigned_abs() < SimScalar::from_f32_lossy(0.05).raw() as u64 {
            if a.id.0 < b.id.0 {
                perp = checked_vec_neg(perp)?;
            }
        } else if dot < SimScalar::ZERO {
            perp = checked_vec_neg(perp)?;
        }
        return Ok((SimVec2::ZERO, checked_vec_scale(perp, overlap)?));
    }
    if !a_nonzero && b_nonzero && b.push_priority >= a.push_priority {
        let move_dir = checked_normalize(b.vel)?;
        let mut perp = if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON {
            SimVec2::new(-move_dir.y, move_dir.x)
        } else {
            SimVec2::Y_AXIS
        };
        let dot = checked_dot(delta, perp)?;
        if dot.raw().unsigned_abs() < SimScalar::from_f32_lossy(0.05).raw() as u64 {
            if a.id.0 < b.id.0 {
                perp = checked_vec_neg(perp)?;
            }
        } else if dot < SimScalar::ZERO {
            perp = checked_vec_neg(perp)?;
        }
        return Ok((checked_vec_scale(perp, overlap)?, SimVec2::ZERO));
    }
    let half = overlap * SimScalar::HALF;
    let move_dir = if a_moving {
        checked_normalize(a.vel)?
    } else if b_moving {
        checked_normalize(b.vel)?
    } else {
        SimVec2::ZERO
    };
    if checked_dot(move_dir, move_dir)? > SimScalar::EPSILON
        && checked_dot(separation, move_dir)?.raw().unsigned_abs() > SimScalar::HALF.raw() as u64
    {
        let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
        let dot = checked_dot(delta, perp)?;
        if dot.raw().unsigned_abs() < SimScalar::from_f32_lossy(0.05).raw() as u64 {
            if a.id.0 < b.id.0 {
                perp = checked_vec_neg(perp)?;
            }
        } else if dot < SimScalar::ZERO {
            perp = checked_vec_neg(perp)?;
        }
        let wedge_dir_a = checked_normalize(checked_vec_add(
            separation,
            checked_vec_scale(perp, SimScalar::from_f32_lossy(0.25))?,
        )?)?;
        let wedge_dir_b = checked_normalize(checked_vec_add(
            checked_vec_neg(separation)?,
            checked_vec_scale(checked_vec_neg(perp)?, SimScalar::from_f32_lossy(0.25))?,
        )?)?;
        return Ok((
            checked_vec_scale(wedge_dir_a, half)?,
            checked_vec_scale(wedge_dir_b, half)?,
        ));
    }
    let correction = checked_vec_scale(separation, half)?;
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
        square >= (SimScalar::EPSILON.raw() as u128 + 1) << 32,
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
    };
    let b = PushEntityParams {
        id: other_unit.id,
        pos: other_unit.position,
        vel: other_unit.velocity,
        owner: other_unit.owner,
        push_priority: 10,
        cannot_be_pushed: other_unit.cannot_be_pushed(),
        is_holding: other_unit.cannot_be_pushed(),
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
            owner: sim_types::PlayerId(0),
            push_priority: 0,
            cannot_be_pushed: false,
            is_holding: false,
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
mod cluster_arrival_tests {
    use super::*;
    use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, RampDir};

    fn witness(id: u32, pos: SimVec2) -> SteeringObstacle {
        SteeringObstacle {
            id: EntityId(id),
            pos,
            radius: SimScalar::HALF,
            velocity: SimVec2::ZERO,
        }
    }

    #[test]
    fn arrival_policy_only_visits_eligible_witnesses_until_success() -> Result<(), PushGeometryError>
    {
        let origin = SimVec2::from_i32(4, 4);
        let goal = origin + SimVec2::X_AXIS;
        let mut excluded = witness(1, origin);
        excluded.radius = SimScalar::ZERO;
        let mut moving = witness(2, goal);
        moving.velocity = SimVec2::X_AXIS;
        let far = witness(3, SimVec2::from_i32(30, 30));
        let candidates = [
            excluded,
            moving,
            far,
            witness(4, goal),
            witness(5, goal),
            witness(6, goal),
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
        let mut invalid = witness(3, goal);
        invalid.radius = SimScalar::ZERO;
        invalid.velocity = SimVec2::X_AXIS;
        let mut count = 0;
        assert_eq!(
            check_cluster_arrival_with(
                Some(EntityId(1)),
                origin,
                SimScalar::HALF,
                goal,
                &[witness(2, goal), invalid],
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
        let valid = witness(2, center(7, 5));
        let side = witness(3, center(8, 5));
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
        let mut obs = witness(2, goal);
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
