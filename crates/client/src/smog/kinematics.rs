//! 10-Bubble arbitrary 3D kinematics, elastic boundary reflection, and sphere collision resolution.
//!
//! # Architectural Responsibilities
//! This module owns the continuous 3D motion of the 10 mobile bubbles within the bounded arena.
//! Every bubble moves with arbitrary 3D velocity $\mathbf{v}_i$, reflecting elastically off all 6
//! boundary walls and resolving elastic collisions upon sphere-sphere contact.
//!
//! # Invariants Maintained
//! - Exactly [`BOUNCING_BUBBLE_COUNT`] (10) bubbles are simulated and contained.
//! - Strict containment: every bubble center $\mathbf{p}_i$ satisfies
//!   $\Omega_{\min} + R_i \le \mathbf{p}_i \le \Omega_{\max} - R_i$ at all times.
//! - Heading angle $\theta_i = \operatorname{atan2}(v_{i,z}, v_{i,x})$ remains synchronized with horizontal velocity.
//! - Pairwise elastic collision resolution conserves momentum and kinetic energy without penetration.

use super::SmogError;
use bevy::prelude::*;

/// Number of bouncing unit bubbles simulated concurrently in the 3D smog arena.
pub const BOUNCING_BUBBLE_COUNT: usize = 10;

const SPATIAL_GRID_DIM: usize = 16;
const SPATIAL_CELL_COUNT: usize = SPATIAL_GRID_DIM * SPATIAL_GRID_DIM;

/// Maximum number of bouncing smog bubbles supported without heap allocation in the spatial grid.
pub const MAX_BOUNCING_BUBBLES: usize = 512;

/// 3D Axis-Aligned Bounding Box delimiting the bubble bounce arena.
#[derive(Debug, Clone, Copy, PartialEq, Reflect)]
pub struct SmogArenaBounds {
    /// Minimum corner coordinate in world units.
    pub min: Vec3,
    /// Maximum corner coordinate in world units.
    pub max: Vec3,
}

impl Default for SmogArenaBounds {
    fn default() -> Self {
        Self {
            min: Vec3::new(-4.8, 0.0, -3.0),
            max: Vec3::new(4.8, 2.8, 3.0),
        }
    }
}

impl SmogArenaBounds {
    /// Creates a new arena bounds box, verifying that minimum coordinates are strictly less than maximums.
    ///
    /// # Errors
    /// Returns [`SmogError::InvalidBounds`] if any coordinate has `min >= max` or is non-finite.
    pub fn new(min: Vec3, max: Vec3) -> Result<Self, SmogError> {
        let bounds = Self { min, max };
        bounds.validate()?;
        Ok(bounds)
    }

    /// Validates that bounds are finite and offer at least 1.5 world units of internal space on each axis.
    ///
    /// # Errors
    /// Returns [`SmogError::InvalidBounds`] if validation fails.
    pub fn validate(&self) -> Result<(), SmogError> {
        if !self.min.is_finite()
            || !self.max.is_finite()
            || self.min.x >= self.max.x - 1.5
            || self.min.y >= self.max.y - 1.5
            || self.min.z >= self.max.z - 1.5
        {
            return Err(SmogError::InvalidBounds {
                min: self.min,
                max: self.max,
            });
        }
        Ok(())
    }

    /// Checks if a sphere with center `center` and radius `radius` is strictly contained within the bounds.
    #[must_use]
    pub fn contains_sphere(&self, center: Vec3, radius: f32) -> bool {
        center.x >= self.min.x + radius
            && center.x <= self.max.x - radius
            && center.y >= self.min.y + radius
            && center.y <= self.max.y - radius
            && center.z >= self.min.z + radius
            && center.z <= self.max.z - radius
    }
}

/// ECS component attached to each of the 10 mobile bouncing bubbles.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct BouncingSmogBubble {
    /// Unique index in `0..BOUNCING_BUBBLE_COUNT`.
    pub index: usize,
    /// Current 3D velocity vector in world units per second.
    pub velocity: Vec3,
    /// Bounding sphere collision radius in world units.
    pub radius: f32,
    /// Cumulative count of elastic wall reflections.
    pub wall_bounces: u64,
    /// Cumulative count of pairwise sphere-sphere collisions.
    pub peer_bounces: u64,
}

/// Transient kinematics state representation of a single bouncing bubble.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BouncingBubbleState {
    /// Unique bubble index in `0..BOUNCING_BUBBLE_COUNT`.
    pub index: usize,
    /// 3D position in world units.
    pub position: Vec3,
    /// 3D velocity vector in world units per second.
    pub velocity: Vec3,
    /// Sphere radius in world units.
    pub radius: f32,
    /// Heading orientation angle in radians.
    pub heading_angle_rad: f32,
    /// Cumulative wall reflections.
    pub wall_bounces: u64,
    /// Cumulative peer sphere collisions.
    pub peer_bounces: u64,
}

/// Deterministically generates `count` bouncing bubble states within `bounds`.
///
/// For `count <= 20`, bubble radii are in `[0.46, 0.64]` and speeds in `[2.4, 4.1]`.
/// For larger crowds (up to 400), bubble radii scale down to `[0.22, 0.30]` so entities
/// distribute across a 3D lattice without continuous overlap jamming.
///
/// # Errors
/// Returns [`SmogError::InvalidBounds`] if `bounds` is invalid.
/// Curated harmonious bioluminescent base palette for the 10 showcase bouncing bubbles.
pub const SHOWCASE_BUBBLE_PALETTE: [Vec3; BOUNCING_BUBBLE_COUNT] = [
    Vec3::new(0.98, 0.82, 0.22), // 0: Solar Gold
    Vec3::new(0.18, 0.88, 0.96), // 1: Electric Cyan
    Vec3::new(0.20, 0.92, 0.52), // 2: Bioluminescent Emerald
    Vec3::new(0.72, 0.36, 0.95), // 3: Royal Amethyst
    Vec3::new(0.96, 0.32, 0.58), // 4: Coral Magenta
    Vec3::new(0.24, 0.58, 0.98), // 5: Cerulean Azure
    Vec3::new(0.98, 0.55, 0.18), // 6: Amber Tangerine
    Vec3::new(0.28, 0.88, 0.75), // 7: Jade Mint
    Vec3::new(0.95, 0.26, 0.38), // 8: Crimson Rose
    Vec3::new(0.60, 0.52, 0.96), // 9: Pearlescent Violet
];

/// Returns the curated or deterministic bioluminescent base color for a bubble by index.
#[must_use]
pub fn bubble_team_color(index: usize) -> Vec3 {
    if index < SHOWCASE_BUBBLE_PALETTE.len() {
        SHOWCASE_BUBBLE_PALETTE[index]
    } else {
        let hue_shift = (index as f32 * 0.137) % 1.0;
        let tau = std::f32::consts::TAU;
        Vec3::new(
            (0.25 + 0.65 * (hue_shift * tau).sin().abs()).clamp(0.05, 0.95),
            (0.30 + 0.60 * ((hue_shift + 0.33) * tau).sin().abs()).clamp(0.05, 0.95),
            (0.35 + 0.60 * ((hue_shift + 0.67) * tau).sin().abs()).clamp(0.05, 0.95),
        )
    }
}

/// Generates an arbitrary number of well-separated bouncing bubble states on the horizontal map plane.
///
/// For `count <= 10`, uses hand-curated well-distributed positions across the $(X, Z)$ ground plane.
/// For larger crowds up to 400, distributes bubbles across a 2D map-plane lattice with deterministic
/// jitter to ensure non-overlapping starting configurations. All bubbles have $v_y = 0.0$ and
/// $p_y = \text{bounds.min.y} + R_i$.
///
/// # Errors
/// Returns [`SmogError::InvalidBounds`] if `bounds` is invalid.
pub fn generate_bouncing_bubbles(
    count: usize,
    bounds: &SmogArenaBounds,
) -> Result<Vec<BouncingBubbleState>, SmogError> {
    bounds.validate()?;
    if count == 0 {
        return Ok(Vec::new());
    }

    if count <= BOUNCING_BUBBLE_COUNT {
        let initial_10_xz = [
            (Vec2::new(-3.6, -1.6), Vec2::new(3.0, 0.5)),
            (Vec2::new(-1.8, -2.2), Vec2::new(0.4, 3.0)),
            (Vec2::new(-3.6, 1.2), Vec2::new(2.7, -1.6)),
            (Vec2::new(-2.2, 2.1), Vec2::new(2.8, -0.4)),
            (Vec2::new(-0.2, -2.1), Vec2::new(0.3, 3.1)),
            (Vec2::new(0.4, 1.9), Vec2::new(-0.4, -2.9)),
            (Vec2::new(3.6, -1.5), Vec2::new(-2.9, 0.4)),
            (Vec2::new(1.9, -2.2), Vec2::new(-0.5, 3.0)),
            (Vec2::new(3.6, 1.4), Vec2::new(-2.8, -1.4)),
            (Vec2::new(2.2, 2.1), Vec2::new(-2.7, -0.5)),
        ];

        let mut out = Vec::with_capacity(count);
        for (i, &(raw_pos_xz, vel_xz)) in initial_10_xz.iter().enumerate().take(count) {
            let radius = 0.46 + (i as f32) * 0.02;
            let pos = Vec3::new(
                raw_pos_xz
                    .x
                    .clamp(bounds.min.x + radius, bounds.max.x - radius),
                bounds.min.y + radius,
                raw_pos_xz
                    .y
                    .clamp(bounds.min.z + radius, bounds.max.z - radius),
            );
            let vel = Vec3::new(vel_xz.x, 0.0, vel_xz.y);
            let heading_angle_rad = vel.z.atan2(vel.x);
            out.push(BouncingBubbleState {
                index: i,
                position: pos,
                velocity: vel,
                radius,
                heading_angle_rad,
                wall_bounces: 0,
                peer_bounces: 0,
            });
        }
        return Ok(out);
    }

    // For larger crowds (up to 400), arrange on a 2D map-plane lattice with deterministic jitter
    let radius = if count <= 20 {
        0.42
    } else if count <= 100 {
        0.30
    } else {
        0.24
    };

    let domain_x = (bounds.max.x - bounds.min.x - radius * 2.0).max(1.0);
    let domain_z = (bounds.max.z - bounds.min.z - radius * 2.0).max(1.0);

    let aspect = domain_x / domain_z;
    let nx = ((count as f32 * aspect).sqrt().round() as usize).max(1);
    let nz = count.div_ceil(nx);
    let cell_size_x = domain_x / nx as f32;
    let cell_size_z = domain_z / nz as f32;

    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let ix = i % nx;
        let iz = i / nx;

        let fx = (ix as f32 + 0.5) / (nx as f32);
        let fz = (iz as f32 + 0.5) / (nz as f32).max(1.0);

        // Deterministic offset to break perfect grid alignment
        let jitter_x = ((i as f32 * std::f32::consts::GOLDEN_RATIO).sin() * 0.25) * cell_size_x;
        let jitter_z = ((i as f32 * std::f32::consts::PI).sin() * 0.25) * cell_size_z;

        let r_i = radius + ((i % 5) as f32) * 0.01;
        let pos = Vec3::new(
            (bounds.min.x + r_i + fx * domain_x + jitter_x)
                .clamp(bounds.min.x + r_i, bounds.max.x - r_i),
            bounds.min.y + r_i,
            (bounds.min.z + r_i + fz * domain_z + jitter_z)
                .clamp(bounds.min.z + r_i, bounds.max.z - r_i),
        );

        let speed = 2.2 + ((i % 7) as f32) * 0.25;
        let phi = (i as f32) * 2.399_963_1; // golden angle
        let vel = Vec3::new(phi.cos() * speed, 0.0, phi.sin() * speed);
        let heading_angle_rad = vel.z.atan2(vel.x);

        out.push(BouncingBubbleState {
            index: i,
            position: pos,
            velocity: vel,
            radius: r_i,
            heading_angle_rad,
            wall_bounces: 0,
            peer_bounces: 0,
        });
    }

    Ok(out)
}

/// Generates 10 well-separated initial bouncing bubble states inside `bounds`.
///
/// Every bubble is given a distinct radius $R_i \in [0.46, 0.64]$ and arbitrary 3D velocity
/// with speeds in $[2.4, 4.1]$ world units per second and varied directional orientations.
///
/// # Errors
/// Returns [`SmogError::InvalidBounds`] if `bounds` is invalid.
pub fn initial_bouncing_bubbles(
    bounds: &SmogArenaBounds,
) -> Result<[BouncingBubbleState; BOUNCING_BUBBLE_COUNT], SmogError> {
    let list = generate_bouncing_bubbles(BOUNCING_BUBBLE_COUNT, bounds)?;
    let mut states = [BouncingBubbleState {
        index: 0,
        position: Vec3::ZERO,
        velocity: Vec3::ZERO,
        radius: 0.5,
        heading_angle_rad: 0.0,
        wall_bounces: 0,
        peer_bounces: 0,
    }; BOUNCING_BUBBLE_COUNT];
    states.copy_from_slice(&list[..BOUNCING_BUBBLE_COUNT]);
    Ok(states)
}

/// Steps kinematics for all bouncing bubbles: position integration, 6-face elastic wall reflections,
/// pairwise sphere-sphere elastic collision resolution, and heading angle synchronization.
///
/// Returns `Ok((wall_bounces, peer_bounces))` with the counts of events that occurred during this step.
///
/// # Errors
/// Returns [`SmogError::InvalidDeltaTime`] if `dt < 0.0` or non-finite.
/// Returns [`SmogError::InvalidBounds`] if `bounds` is invalid.
pub fn step_bouncing_kinematics(
    states: &mut [BouncingBubbleState],
    bounds: &SmogArenaBounds,
    dt: f32,
) -> Result<(u64, u64), SmogError> {
    if !dt.is_finite() || dt < 0.0 {
        return Err(SmogError::InvalidDeltaTime(dt));
    }
    bounds.validate()?;

    if dt == 0.0 || states.is_empty() {
        return Ok((0, 0));
    }

    let mut step_wall_bounces = 0u64;
    let mut step_peer_bounces = 0u64;

    // 1. Position Integration & Elastic Wall Reflection (All 6 faces)
    for state in states.iter_mut() {
        if state.velocity.y.abs() < 1e-4 {
            state.velocity.y = 0.0;
            state.position.y = bounds.min.y + state.radius;
        } else {
            state.position.y += state.velocity.y * dt;
        }
        state.position.x += state.velocity.x * dt;
        state.position.z += state.velocity.z * dt;

        // Check and reflect across X, Y, Z boundaries
        for axis in 0..3 {
            let p = state.position[axis];
            let v = state.velocity[axis];
            let min_bound = bounds.min[axis] + state.radius;
            let max_bound = bounds.max[axis] - state.radius;

            if p < min_bound && v < 0.0 {
                state.position[axis] = min_bound + (min_bound - p);
                state.velocity[axis] = -v;
                state.wall_bounces += 1;
                step_wall_bounces += 1;
            } else if p > max_bound && v > 0.0 {
                state.position[axis] = max_bound - (p - max_bound);
                state.velocity[axis] = -v;
                state.wall_bounces += 1;
                step_wall_bounces += 1;
            }

            // Guaranteed numerical clamp
            state.position[axis] = state.position[axis].clamp(min_bound, max_bound);
        }
    }

    // 2. Spatial Hash Grid Pairwise Elastic Sphere-Sphere Impulse Resolution
    // PERFORMANCE RATIONALE:
    // Why: In a 400-unit simulation, naive O(N^2) pairwise collision tests perform 79,800 tests/frame.
    // What: We partition the XZ arena plane into a 16x16 spatial hash grid using stack-allocated flat arrays.
    // Trade-off: Uses 512 + 256 words of stack space in exchange for O(N) local 3x3 neighborhood queries
    // and zero heap allocations during the kinematics tick.
    let n = states.len();
    if n <= MAX_BOUNCING_BUBBLES {
        let mut head = [-1i32; SPATIAL_CELL_COUNT];
        let mut next = [-1i32; MAX_BOUNCING_BUBBLES];

        let extent_x = (bounds.max.x - bounds.min.x).max(1e-4);
        let extent_z = (bounds.max.z - bounds.min.z).max(1e-4);

        for (idx, state) in states.iter().enumerate() {
            let u_x = ((state.position.x - bounds.min.x) / extent_x).clamp(0.0, 0.9999);
            let u_z = ((state.position.z - bounds.min.z) / extent_z).clamp(0.0, 0.9999);
            let cx = (u_x * SPATIAL_GRID_DIM as f32) as usize;
            let cz = (u_z * SPATIAL_GRID_DIM as f32) as usize;
            let cell = cz * SPATIAL_GRID_DIM + cx;
            next[idx] = head[cell];
            head[cell] = idx as i32;
        }

        for i in 0..n {
            let u_x = ((states[i].position.x - bounds.min.x) / extent_x).clamp(0.0, 0.9999);
            let u_z = ((states[i].position.z - bounds.min.z) / extent_z).clamp(0.0, 0.9999);
            let cx = (u_x * SPATIAL_GRID_DIM as f32) as i32;
            let cz = (u_z * SPATIAL_GRID_DIM as f32) as i32;

            for dz in -1..=1 {
                let nz = cz + dz;
                if nz < 0 || nz >= SPATIAL_GRID_DIM as i32 {
                    continue;
                }
                for dx in -1..=1 {
                    let nx = cx + dx;
                    if nx < 0 || nx >= SPATIAL_GRID_DIM as i32 {
                        continue;
                    }
                    let cell = (nz as usize) * SPATIAL_GRID_DIM + (nx as usize);
                    let mut curr = head[cell];
                    while curr >= 0 {
                        let j = curr as usize;
                        curr = next[j];
                        if j <= i {
                            continue;
                        }

                        let pos_i = states[i].position;
                        let pos_j = states[j].position;
                        let r_i = states[i].radius;
                        let r_j = states[j].radius;

                        let is_planar =
                            states[i].velocity.y.abs() < 1e-4 && states[j].velocity.y.abs() < 1e-4;
                        let delta = if is_planar {
                            Vec3::new(pos_j.x - pos_i.x, 0.0, pos_j.z - pos_i.z)
                        } else {
                            pos_j - pos_i
                        };
                        let dist_sq = delta.length_squared();
                        let min_dist = r_i + r_j;

                        if dist_sq < min_dist * min_dist && dist_sq > 1e-8 {
                            let dist = dist_sq.sqrt();
                            let normal = delta / dist;
                            let overlap = min_dist - dist;

                            let m_i = r_i.powi(3);
                            let m_j = r_j.powi(3);
                            let total_m = m_i + m_j;

                            states[i].position -= normal * (overlap * (m_j / total_m));
                            states[j].position += normal * (overlap * (m_i / total_m));

                            if is_planar {
                                states[i].position.y = bounds.min.y + states[i].radius;
                                states[j].position.y = bounds.min.y + states[j].radius;
                            }

                            for axis in 0..3 {
                                states[i].position[axis] = states[i].position[axis].clamp(
                                    bounds.min[axis] + states[i].radius,
                                    bounds.max[axis] - states[i].radius,
                                );
                                states[j].position[axis] = states[j].position[axis].clamp(
                                    bounds.min[axis] + states[j].radius,
                                    bounds.max[axis] - states[j].radius,
                                );
                            }

                            let vel_i = states[i].velocity;
                            let vel_j = states[j].velocity;
                            let v_rel = (vel_j - vel_i).dot(normal);

                            if v_rel < 0.0 {
                                let impulse = -2.0 * v_rel / (1.0 / m_i + 1.0 / m_j);
                                states[i].velocity -= normal * (impulse / m_i);
                                states[j].velocity += normal * (impulse / m_j);
                                if is_planar {
                                    states[i].velocity.y = 0.0;
                                    states[j].velocity.y = 0.0;
                                }
                                states[i].peer_bounces += 1;
                                states[j].peer_bounces += 1;
                                step_peer_bounces += 1;
                            }
                        }
                    }
                }
            }
        }
    } else {
        // Fallback for n > MAX_BOUNCING_BUBBLES
        for i in 0..n {
            for j in (i + 1)..n {
                let pos_i = states[i].position;
                let pos_j = states[j].position;
                let r_i = states[i].radius;
                let r_j = states[j].radius;

                let is_planar =
                    states[i].velocity.y.abs() < 1e-4 && states[j].velocity.y.abs() < 1e-4;
                let delta = if is_planar {
                    Vec3::new(pos_j.x - pos_i.x, 0.0, pos_j.z - pos_i.z)
                } else {
                    pos_j - pos_i
                };
                let dist_sq = delta.length_squared();
                let min_dist = r_i + r_j;

                if dist_sq < min_dist * min_dist && dist_sq > 1e-8 {
                    let dist = dist_sq.sqrt();
                    let normal = delta / dist;
                    let overlap = min_dist - dist;

                    let m_i = r_i.powi(3);
                    let m_j = r_j.powi(3);
                    let total_m = m_i + m_j;

                    states[i].position -= normal * (overlap * (m_j / total_m));
                    states[j].position += normal * (overlap * (m_i / total_m));

                    if is_planar {
                        states[i].position.y = bounds.min.y + states[i].radius;
                        states[j].position.y = bounds.min.y + states[j].radius;
                    }

                    for axis in 0..3 {
                        states[i].position[axis] = states[i].position[axis].clamp(
                            bounds.min[axis] + states[i].radius,
                            bounds.max[axis] - states[i].radius,
                        );
                        states[j].position[axis] = states[j].position[axis].clamp(
                            bounds.min[axis] + states[j].radius,
                            bounds.max[axis] - states[j].radius,
                        );
                    }

                    let vel_i = states[i].velocity;
                    let vel_j = states[j].velocity;
                    let v_rel = (vel_j - vel_i).dot(normal);

                    if v_rel < 0.0 {
                        let impulse = -2.0 * v_rel / (1.0 / m_i + 1.0 / m_j);
                        states[i].velocity -= normal * (impulse / m_i);
                        states[j].velocity += normal * (impulse / m_j);
                        if is_planar {
                            states[i].velocity.y = 0.0;
                            states[j].velocity.y = 0.0;
                        }
                        states[i].peer_bounces += 1;
                        states[j].peer_bounces += 1;
                    }
                }
            }
        }
    }

    // 3. Heading Angle Synchronization
    for state in states.iter_mut() {
        let speed_xz =
            (state.velocity.x * state.velocity.x + state.velocity.z * state.velocity.z).sqrt();
        if speed_xz > 1e-4 {
            state.heading_angle_rad = state.velocity.z.atan2(state.velocity.x);
        }
    }

    Ok((step_wall_bounces, step_peer_bounces))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_bouncing_bubbles_invariants() -> Result<(), SmogError> {
        let bounds = SmogArenaBounds::default();
        let bubbles = initial_bouncing_bubbles(&bounds)?;

        assert_eq!(bubbles.len(), BOUNCING_BUBBLE_COUNT);

        for (i, b) in bubbles.iter().enumerate() {
            assert_eq!(b.index, i);
            assert!(
                bounds.contains_sphere(b.position, b.radius),
                "Bubble {} at {:?} (radius {}) must be inside bounds",
                i,
                b.position,
                b.radius
            );
            assert!(
                (0.45..=0.65).contains(&b.radius),
                "Radius {} out of expected range",
                b.radius
            );
            let speed = b.velocity.length();
            assert!(
                (2.0..=5.0).contains(&speed),
                "Speed {speed} out of expected range"
            );
            assert!(b.heading_angle_rad.is_finite());
        }

        // Pairwise non-overlap
        for i in 0..bubbles.len() {
            for j in (i + 1)..bubbles.len() {
                let dist = (bubbles[i].position - bubbles[j].position).length();
                let min_dist = bubbles[i].radius + bubbles[j].radius;
                assert!(
                    dist >= min_dist,
                    "Initial bubbles {i} and {j} must not overlap (dist={dist}, min={min_dist})"
                );
            }
        }

        Ok(())
    }

    #[test]
    fn test_elastic_wall_bounces_all_six_faces() -> Result<(), SmogError> {
        let bounds = SmogArenaBounds::new(Vec3::new(-4.0, 0.0, -3.0), Vec3::new(4.0, 4.0, 3.0))?;
        let radius = 0.5;

        // Test each face: -X, +X, -Y, +Y, -Z, +Z
        let test_cases = [
            (Vec3::new(-3.4, 2.0, 0.0), Vec3::new(-2.0, 0.0, 0.0), 0, 1.0),
            (Vec3::new(3.4, 2.0, 0.0), Vec3::new(2.0, 0.0, 0.0), 0, -1.0),
            (Vec3::new(0.0, 0.6, 0.0), Vec3::new(0.0, -2.0, 0.0), 1, 1.0),
            (Vec3::new(0.0, 3.4, 0.0), Vec3::new(0.0, 2.0, 0.0), 1, -1.0),
            (Vec3::new(0.0, 2.0, -2.4), Vec3::new(0.0, 0.0, -2.0), 2, 1.0),
            (Vec3::new(0.0, 2.0, 2.4), Vec3::new(0.0, 0.0, 2.0), 2, -1.0),
        ];

        for (initial_pos, initial_vel, axis, expected_sign) in test_cases {
            let mut state = [BouncingBubbleState {
                index: 0,
                position: initial_pos,
                velocity: initial_vel,
                radius,
                heading_angle_rad: 0.0,
                wall_bounces: 0,
                peer_bounces: 0,
            }];

            let (wall_bounces, _) = step_bouncing_kinematics(&mut state, &bounds, 0.1)?;
            assert_eq!(wall_bounces, 1, "Should reflect off wall on axis {axis}");
            assert_eq!(state[0].wall_bounces, 1);
            let v_after = state[0].velocity[axis];
            assert!(
                v_after * expected_sign > 0.0,
                "Velocity on axis {axis} should have reversed direction to sign {expected_sign}"
            );
            assert!(
                bounds.contains_sphere(state[0].position, state[0].radius),
                "Position after bounce must stay inside bounds"
            );
        }

        Ok(())
    }

    #[test]
    fn test_sphere_sphere_elastic_collision() -> Result<(), SmogError> {
        let bounds = SmogArenaBounds::default();
        let r = 0.5;
        let mut states = [
            BouncingBubbleState {
                index: 0,
                position: Vec3::new(-0.4, 1.5, 0.0),
                velocity: Vec3::new(2.0, 0.0, 0.0),
                radius: r,
                heading_angle_rad: 0.0,
                wall_bounces: 0,
                peer_bounces: 0,
            },
            BouncingBubbleState {
                index: 1,
                position: Vec3::new(0.4, 1.5, 0.0),
                velocity: Vec3::new(-2.0, 0.0, 0.0),
                radius: r,
                heading_angle_rad: std::f32::consts::PI,
                wall_bounces: 0,
                peer_bounces: 0,
            },
        ];

        let (_, peer_bounces) = step_bouncing_kinematics(&mut states, &bounds, 0.01)?;
        assert_eq!(peer_bounces, 1, "Pairwise collision must be detected");
        assert_eq!(states[0].peer_bounces, 1);
        assert_eq!(states[1].peer_bounces, 1);

        // Velocities should have reversed along X
        assert!(states[0].velocity.x < 0.0, "Bubble 0 should rebound to -X");
        assert!(states[1].velocity.x > 0.0, "Bubble 1 should rebound to +X");

        // Distance after collision should be >= 2*r
        let dist = (states[1].position - states[0].position).length();
        let min_sep = 2.0 * r;
        assert!(
            dist >= min_sep - 1e-4,
            "Bubbles must be separated after resolution (dist={dist}, min={min_sep})"
        );

        Ok(())
    }

    #[test]
    fn test_multi_hundred_step_containment() -> Result<(), SmogError> {
        let bounds = SmogArenaBounds::default();
        let mut bubbles = initial_bouncing_bubbles(&bounds)?;

        let dt = 1.0 / 60.0;
        let mut total_wall_bounces = 0u64;
        let mut total_peer_bounces = 0u64;

        for step in 0..500 {
            let (wb, pb) = step_bouncing_kinematics(&mut bubbles, &bounds, dt)?;
            total_wall_bounces += wb;
            total_peer_bounces += pb;

            for (i, b) in bubbles.iter().enumerate() {
                assert!(
                    bounds.contains_sphere(b.position, b.radius),
                    "Step {}: Bubble {} at {:?} escaped bounds",
                    step,
                    i,
                    b.position
                );
                assert!(b.velocity.is_finite(), "Velocity must remain finite");
                assert!(b.position.is_finite(), "Position must remain finite");
            }
        }

        assert!(
            total_wall_bounces > 10,
            "Over 500 steps, multiple wall bounces should occur (got {total_wall_bounces})"
        );
        assert!(
            total_peer_bounces > 0,
            "Over 500 steps, peer bubble collisions should occur (got {total_peer_bounces})"
        );

        Ok(())
    }

    #[test]
    fn test_four_hundred_bubbles_spatial_grid_containment() -> Result<(), SmogError> {
        let bounds = SmogArenaBounds::default();
        let mut bubbles = generate_bouncing_bubbles(400, &bounds)?;
        assert_eq!(bubbles.len(), 400);

        let dt = 1.0 / 60.0;
        for _ in 0..60 {
            step_bouncing_kinematics(&mut bubbles, &bounds, dt)?;
        }

        for (i, b) in bubbles.iter().enumerate() {
            assert!(
                bounds.contains_sphere(b.position, b.radius),
                "Bubble {i} at {:?} with radius {} must stay inside bounds",
                b.position,
                b.radius
            );
        }
        Ok(())
    }
}
