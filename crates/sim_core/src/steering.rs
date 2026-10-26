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
//! 3. Zero heap allocations during hot per-tick movement passes.

use crate::entity::Entity;
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

/// Computes tangential steering force to wrap around allied firing lines and form a concave firing arc.
#[must_use]
pub fn compute_concave_arc_steering(
    unit_pos: SimVec2,
    target_pos: SimVec2,
    blocked_by_allies: bool,
    steer_left: bool,
) -> SimVec2 {
    if !blocked_by_allies {
        return SimVec2::ZERO;
    }
    let to_target = target_pos - unit_pos;
    let perp = if steer_left {
        SimVec2::new(-to_target.y, to_target.x)
    } else {
        SimVec2::new(to_target.y, -to_target.x)
    };
    perp.normalize_or_zero()
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

/// Computes unified local steering towards `goal_pos`, fanning out and wrapping around blocking entities.
///
/// Returns the normalized travel direction vector.
#[must_use]
pub fn compute_unified_local_steering(
    unit_id: EntityId,
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    goal_pos: SimVec2,
    ignored_entity: Option<EntityId>,
    obstacles: &[SteeringObstacle],
) -> SimVec2 {
    let to_goal = goal_pos - unit_pos;
    let goal_dist = to_goal.length();
    if goal_dist == SimScalar::ZERO {
        return SimVec2::ZERO;
    }
    let goal_dir = to_goal / goal_dist;

    let mut closest_blocker: Option<(SimScalar, SimVec2, EntityId)> = None;
    let forward_lookahead = SimScalar::from_f32_lossy(0.35);

    for obs in obstacles {
        if obs.id == unit_id || Some(obs.id) == ignored_entity {
            continue;
        }

        let to_obs = obs.pos - unit_pos;
        let obs_dist = to_obs.length();
        let max_block_dist = unit_radius + obs.radius + forward_lookahead;

        if obs_dist > max_block_dist || obs_dist == SimScalar::ZERO {
            continue;
        }

        // Must be in forward hemisphere relative to goal direction
        if to_obs.dot(goal_dir) <= SimScalar::ZERO {
            continue;
        }

        if let Some((best_dist, _, _)) = closest_blocker {
            if obs_dist < best_dist {
                closest_blocker = Some((obs_dist, obs.pos, obs.id));
            }
        } else {
            closest_blocker = Some((obs_dist, obs.pos, obs.id));
        }
    }

    if let Some((_, blocker_pos, blocker_id)) = closest_blocker {
        let to_blocker = blocker_pos - unit_pos;
        // 2D cross product: goal_dir.x * to_blocker.y - goal_dir.y * to_blocker.x
        let cross = goal_dir.x * to_blocker.y - goal_dir.y * to_blocker.x;
        let deadzone = SimScalar::from_f32_lossy(0.05);
        let steer_left = if cross.abs() > deadzone {
            cross < SimScalar::ZERO // blocker is on right -> steer left
        } else {
            unit_id.0 < blocker_id.0 // deterministic tie-breaker based on persistent entity IDs
        };

        let tangent = if steer_left {
            SimVec2::new(-goal_dir.y, goal_dir.x)
        } else {
            SimVec2::new(goal_dir.y, -goal_dir.x)
        };

        (goal_dir + tangent * SimScalar::from_f32_lossy(0.85)).normalize_or_zero()
    } else {
        goal_dir
    }
}

/// Checks if a moving unit has settled into an arrival cluster adjacent to units already at the destination.
#[must_use]
pub fn check_cluster_arrival(
    unit_pos: SimVec2,
    unit_radius: SimScalar,
    target_pos: SimVec2,
    obstacles: &[SteeringObstacle],
) -> bool {
    let arrival_cluster_radius = SimScalar::from_f32_lossy(1.5);
    let touch_margin = SimScalar::from_f32_lossy(0.15);

    for obs in obstacles {
        if obs.velocity == SimVec2::ZERO {
            let combined_radius = unit_radius + obs.radius;
            if obs.pos.distance(target_pos) <= combined_radius + arrival_cluster_radius
                && unit_pos.distance(obs.pos) <= combined_radius + touch_margin
            {
                return true;
            }
        }
    }
    false
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

/// Computes position displacement deltas for two colliding entities.
#[must_use]
pub fn compute_push_displacement(
    a: PushEntityParams,
    b: PushEntityParams,
    distance: SimScalar,
    combined_radius: SimScalar,
) -> (SimVec2, SimVec2) {
    let raw_overlap = combined_radius - distance;
    if raw_overlap <= SimScalar::ZERO {
        return (SimVec2::ZERO, SimVec2::ZERO);
    }
    // Rule 1: Enemies never yield (hard reciprocal repulsion)
    if a.owner != b.owner {
        let sep_dir = if distance > SimScalar::EPSILON {
            (a.pos - b.pos).normalize_or_zero()
        } else {
            SimVec2::X_AXIS
        };
        let half_overlap = raw_overlap * SimScalar::HALF;
        return (sep_dir * half_overlap, -sep_dir * half_overlap);
    }

    let a_immobile = a.cannot_be_pushed || a.is_holding;
    let b_immobile = b.cannot_be_pushed || b.is_holding;

    // Rule 2: Friendly units on Hold Position / immobile act as immovable walls
    if b_immobile && !a_immobile {
        let push_back = if distance > SimScalar::EPSILON {
            (a.pos - b.pos).normalize_or_zero()
        } else {
            SimVec2::X_AXIS
        };
        let disp_a = if a.vel.length_sq() > SimScalar::EPSILON {
            let move_dir = a.vel.normalize_or_zero();
            let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
            let delta = a.pos - b.pos;
            if delta.dot(perp) < SimScalar::ZERO {
                perp = -perp;
            }
            (push_back * SimScalar::from_f32_lossy(0.5) + perp * SimScalar::from_f32_lossy(0.85))
                .normalize_or_zero()
                * raw_overlap
        } else {
            push_back * raw_overlap
        };
        return (disp_a, SimVec2::ZERO);
    }

    if a_immobile && !b_immobile {
        let push_back = if distance > SimScalar::EPSILON {
            (b.pos - a.pos).normalize_or_zero()
        } else {
            -SimVec2::X_AXIS
        };
        let disp_b = if b.vel.length_sq() > SimScalar::EPSILON {
            let move_dir = b.vel.normalize_or_zero();
            let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
            let delta = b.pos - a.pos;
            if delta.dot(perp) < SimScalar::ZERO {
                perp = -perp;
            }
            (push_back * SimScalar::from_f32_lossy(0.5) + perp * SimScalar::from_f32_lossy(0.85))
                .normalize_or_zero()
                * raw_overlap
        } else {
            push_back * raw_overlap
        };
        return (SimVec2::ZERO, disp_b);
    }

    if a_immobile && b_immobile {
        let sep_dir = if distance > SimScalar::EPSILON {
            (a.pos - b.pos).normalize_or_zero()
        } else {
            SimVec2::X_AXIS
        };
        let half_overlap = raw_overlap * SimScalar::HALF;
        return (sep_dir * half_overlap, -sep_dir * half_overlap);
    }

    let overlap = raw_overlap * PUSH_RELAXATION;

    // Rule 3: Idle friendly units yield sideways perpendicular to moving unit's velocity
    if b.vel.length_sq() == SimScalar::ZERO
        && a.vel.length_sq() > SimScalar::ZERO
        && a.push_priority >= b.push_priority
    {
        let move_dir = a.vel.normalize_or_zero();
        let mut perp = if move_dir.length_sq() > SimScalar::EPSILON {
            SimVec2::new(-move_dir.y, move_dir.x)
        } else {
            SimVec2::Y_AXIS
        };
        let delta = b.pos - a.pos;
        let perp_dot = delta.dot(perp);
        if perp_dot.abs() < SimScalar::from_f32_lossy(0.05) {
            if a.id.0 < b.id.0 {
                perp = -perp;
            }
        } else if perp_dot < SimScalar::ZERO {
            perp = -perp;
        }
        return (SimVec2::ZERO, perp * overlap);
    }

    if a.vel.length_sq() == SimScalar::ZERO
        && b.vel.length_sq() > SimScalar::ZERO
        && b.push_priority >= a.push_priority
    {
        let move_dir = b.vel.normalize_or_zero();
        let mut perp = if move_dir.length_sq() > SimScalar::EPSILON {
            SimVec2::new(-move_dir.y, move_dir.x)
        } else {
            SimVec2::Y_AXIS
        };
        let delta = a.pos - b.pos;
        let perp_dot = delta.dot(perp);
        if perp_dot.abs() < SimScalar::from_f32_lossy(0.05) {
            if a.id.0 < b.id.0 {
                perp = -perp;
            }
        } else if perp_dot < SimScalar::ZERO {
            perp = -perp;
        }
        return (perp * overlap, SimVec2::ZERO);
    }

    // Rule 4: Both units moving; reciprocal repulsion with cone-wedge lateral deflection
    let sep_dir = if distance > SimScalar::EPSILON {
        (a.pos - b.pos).normalize_or_zero()
    } else {
        SimVec2::X_AXIS
    };
    let half_overlap = overlap * SimScalar::HALF;

    let move_dir = if a.vel.length_sq() > SimScalar::EPSILON {
        a.vel.normalize_or_zero()
    } else if b.vel.length_sq() > SimScalar::EPSILON {
        b.vel.normalize_or_zero()
    } else {
        SimVec2::ZERO
    };

    if move_dir.length_sq() > SimScalar::EPSILON
        && sep_dir.dot(move_dir).abs() > SimScalar::from_f32_lossy(0.5)
    {
        let mut perp = SimVec2::new(-move_dir.y, move_dir.x);
        let delta = a.pos - b.pos;
        let perp_dot = delta.dot(perp);
        if perp_dot.abs() < SimScalar::from_f32_lossy(0.05) {
            if a.id.0 < b.id.0 {
                perp = -perp;
            }
        } else if perp_dot < SimScalar::ZERO {
            perp = -perp;
        }
        let wedge_dir_a = (sep_dir + perp * SimScalar::from_f32_lossy(0.25)).normalize_or_zero();
        let wedge_dir_b = (-sep_dir - perp * SimScalar::from_f32_lossy(0.25)).normalize_or_zero();
        (wedge_dir_a * half_overlap, wedge_dir_b * half_overlap)
    } else {
        (sep_dir * half_overlap, -sep_dir * half_overlap)
    }
}

/// Resolves push priority and circle-circle collision displacement between two entities.
pub fn resolve_push_interaction(
    moving_unit: &mut Entity,
    other_unit: &mut Entity,
    distance: SimScalar,
    combined_radius: SimScalar,
) {
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
    let (d_a, d_b) = compute_push_displacement(a, b, distance, combined_radius);
    moving_unit.position += d_a;
    other_unit.position += d_b;
}
