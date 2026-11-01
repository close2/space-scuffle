//! High-Ground Fog of War, Line-of-Sight Raycasting, and Watchtowers.
//!
//! This module implements StarCraft II-parity Fog of War visibility.
//! It enforces high-ground line-of-sight occlusion, low-ground spotter mechanics,
//! tall grass / smoke vent blockers, and neutral capturable watchtowers.
//!
//! Ground sight follows surface height. On flat terrain that reduces to comparing cliff levels.
//! On a ramp the observer's and target's heights vary continuously, so a unit at the foot of a
//! ramp cannot see its top, and one cresting it can see the plateau ahead.
//!
//! # Invariants
//! Sight radii above [`MAX_SIGHT_RADIUS_CELLS`] are rejected when maps and catalogs load, so
//! the clamp in [`PlayerVisionMap::apply_vision_source`] never shortens authored data.

use crate::grid::{CellFlags, GameMap};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, VisibilityState};
use std::sync::OnceLock;

// PERFORMANCE RATIONALE:
// Why: In a 400-unit battle, computing dynamic raycasts per unit every tick (25 Hz)
// generates over 100,000 ray queries per second, leading to high CPU usage and heap churn.
// What:
// 1. We precompute circular Bresenham ray offset tables indexed by integer sight radius, up to
//    `MAX_SIGHT_RADIUS_CELLS`. The table is built once, on first use.
// 2. Each PlayerVisionMap uses flat pre-allocated Vec<u16> sight counters and Vec<u8> state buffers.
// 3. Incrementing/decrementing sight counters avoids any heap allocations during simulation ticks.
// Trade-off: A static table of a few hundred kilobytes and a per-player map of three flat buffers,
// in exchange for deterministic, cache-friendly O(1) tile updates and zero per-tick heap allocations.

/// Largest sight radius in whole cells that vision can honour.
///
/// Long-range structures author sight above the common unit range, so this must stay at or above
/// the largest shipped value. Larger authored values are rejected at load, never shortened.
pub const MAX_SIGHT_RADIUS_CELLS: u32 = 32;
const MAX_PRECOMPUTED_RADIUS: usize = MAX_SIGHT_RADIUS_CELLS as usize;

/// Highest surface a ground observer can see above its own, in world units: half a cliff.
///
/// A plateau one cliff up stays hidden from level ground, while a ramp reveals itself and the
/// ground beyond as the observer climbs past its midpoint.
const MAX_UPHILL_SIGHT: SimScalar = SimScalar::from_raw(3 << 31);

/// Precomputed Bresenham rays for all integer radii up to `MAX_PRECOMPUTED_RADIUS`.
struct RayTable {
    /// Indexed by radius `R` (0..=`MAX_PRECOMPUTED_RADIUS`), each contains a list of rays.
    /// Each ray is a list of relative `(dx, dy)` cell offsets from origin.
    rays_by_radius: Vec<Vec<Vec<(i8, i8)>>>,
}

impl RayTable {
    fn new() -> Self {
        let mut rays_by_radius = Vec::with_capacity(MAX_PRECOMPUTED_RADIUS + 1);
        rays_by_radius.push(Vec::new()); // R = 0

        for r in 1..=MAX_PRECOMPUTED_RADIUS {
            let r_i32 = r as i32;
            let max_r_sq = r_i32 * r_i32;
            let mut rays = Vec::new();

            // Perimeter points along the square [-r, r] x [-r, r]
            for x in -r_i32..=r_i32 {
                for y in -r_i32..=r_i32 {
                    if x.abs() == r_i32 || y.abs() == r_i32 {
                        let ray = Self::generate_bresenham_ray(x, y, max_r_sq);
                        if !ray.is_empty() {
                            rays.push(ray);
                        }
                    }
                }
            }

            rays_by_radius.push(rays);
        }

        Self { rays_by_radius }
    }

    fn generate_bresenham_ray(target_x: i32, target_y: i32, max_r_sq: i32) -> Vec<(i8, i8)> {
        let mut points = Vec::new();
        let dx = target_x.abs();
        let dy = target_y.abs();
        let sx = if target_x >= 0 { 1 } else { -1 };
        let sy = if target_y >= 0 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = 0;
        let mut y = 0;

        loop {
            if x * x + y * y <= max_r_sq {
                points.push((x as i8, y as i8));
            } else {
                break;
            }

            if x == target_x && y == target_y {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }

        points
    }
}

static GLOBAL_RAY_TABLE: OnceLock<RayTable> = OnceLock::new();

fn get_ray_table() -> &'static RayTable {
    GLOBAL_RAY_TABLE.get_or_init(RayTable::new)
}

/// Vision source query parameters.
#[derive(Debug, Clone, Copy)]
pub struct VisionSource {
    /// World position of the observing unit.
    pub pos: SimVec2,
    /// Locomotion classification (Ground vs Air vs CliffWalker).
    pub mover_type: MoverType,
    /// Maximum sight radius in world units.
    pub sight_radius: SimScalar,
    /// Whether this observer is an active detector (reveals cloaked / burrowed units).
    pub is_detector: bool,
}

/// Authoritative Fog of War map for a single player.
#[derive(Debug, Clone)]
pub struct PlayerVisionMap {
    width: u32,
    height: u32,
    /// Current visibility state per cell.
    states: Vec<VisibilityState>,
    /// Number of active allied vision sources currently seeing each cell.
    visible_counter: Vec<u16>,
    /// Number of active allied detectors currently revealing each cell.
    detector_counter: Vec<u16>,
    /// Generation counter to prevent multiple increments per single vision source.
    visited_in_source: Vec<u32>,
    /// Monotonically increasing generation ID for per-source deduplication.
    current_source_gen: u32,
}

impl PlayerVisionMap {
    /// Creates a new Fog of War visibility map initialized to `VisibilityState::Unexplored`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width as usize) * (height as usize);
        Self {
            width,
            height,
            states: vec![VisibilityState::Unexplored; size],
            visible_counter: vec![0; size],
            detector_counter: vec![0; size],
            visited_in_source: vec![0; size],
            current_source_gen: 0,
        }
    }

    /// Sets all cells on the map to `VisibilityState::Fogged` (competitive SC2 melee map terrain knowledge).
    pub fn set_all_fogged(&mut self) {
        self.states.fill(VisibilityState::Fogged);
    }

    /// Width of the visibility map in 1x1 cells.
    #[must_use]
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the visibility map in 1x1 cells.
    #[must_use]
    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Resets active tick sight counters before re-evaluating allied units.
    pub fn begin_tick_update(&mut self) {
        self.visible_counter.fill(0);
        self.detector_counter.fill(0);
    }

    /// Evaluates line-of-sight for a single vision source and marks visible cells.
    ///
    /// Enforces:
    /// - High-ground occlusion for ground observers.
    /// - Height-based ramp visibility for ground observers: a surface is visible up to half a
    ///   cliff above the observer's own surface height.
    /// - LoS blocker smoke-vent occlusion.
    /// - Air observer elevation bypassing cliff occlusion.
    pub fn apply_vision_source(&mut self, map: &GameMap, source: &VisionSource) {
        let Ok((center_x, center_y)) = map.world_to_cell(source.pos) else {
            return;
        };
        let Ok(center_cell) = map.cell(center_x, center_y) else {
            return;
        };

        let source_cliff = center_cell.cliff_level;
        let source_on_ramp = center_cell.ramp.is_some();
        let sight_ceiling = map.terrain_altitude_scalar(source.pos) + MAX_UPHILL_SIGHT;
        let is_air = matches!(source.mover_type, MoverType::Air | MoverType::CliffWalker);
        let source_in_blocker = center_cell.flags.contains(CellFlags::LOS_BLOCKER);

        let r_int = (source.sight_radius.saturating_ceil().saturating_to_u32() as usize)
            .min(MAX_PRECOMPUTED_RADIUS);
        let r_sq = source.sight_radius * source.sight_radius;

        self.current_source_gen = self.current_source_gen.wrapping_add(1);
        if self.current_source_gen == 0 {
            self.visited_in_source.fill(0);
            self.current_source_gen = 1;
        }

        // Always reveal observer's own cell
        let center_idx = (center_y as usize) * (self.width as usize) + (center_x as usize);
        self.visited_in_source[center_idx] = self.current_source_gen;
        self.visible_counter[center_idx] = self.visible_counter[center_idx].saturating_add(1);
        if source.is_detector {
            self.detector_counter[center_idx] = self.detector_counter[center_idx].saturating_add(1);
        }

        if r_int == 0 {
            return;
        }

        let table = get_ray_table();
        let rays = &table.rays_by_radius[r_int];

        for ray in rays {
            let mut in_blocker = source_in_blocker;

            for &(dx, dy) in ray {
                if dx == 0 && dy == 0 {
                    continue;
                }

                let tx = center_x as i32 + dx as i32;
                let ty = center_y as i32 + dy as i32;

                if !map.in_bounds(tx, ty) {
                    break;
                }

                let dist_sq = (SimScalar::from_num(dx) * SimScalar::from_num(dx))
                    + (SimScalar::from_num(dy) * SimScalar::from_num(dy));
                if dist_sq > r_sq {
                    break;
                }

                let target_cell = match map.cell(tx as u32, ty as u32) {
                    Ok(c) => c,
                    Err(_) => break,
                };

                // High-ground check. Between flat cells this is a cliff level comparison;
                // heights are only computed when the observer or target stands on a ramp.
                if !is_air {
                    let occluded = if target_cell.ramp.is_some() {
                        let surface = map
                            .cell_to_world_center(tx as u32, ty as u32)
                            .map(|center| map.terrain_altitude_scalar(center));
                        surface.map_or(true, |height| height > sight_ceiling)
                    } else if source_on_ramp {
                        target_cell.cliff_level.height_scalar() > sight_ceiling
                    } else {
                        target_cell.cliff_level > source_cliff
                    };
                    if occluded {
                        break;
                    }
                }

                // Line-of-sight blocker check:
                // Smoke vents / tall grass block ground vision across them.
                // Units outside cannot see inside beyond 1 cell; units inside can see out.
                let target_is_blocker = target_cell.flags.contains(CellFlags::LOS_BLOCKER);
                if !is_air && !in_blocker && target_is_blocker {
                    // Entering blocker from outside: reveal edge cell, then terminate ray
                    let t_idx = (ty as usize) * (self.width as usize) + (tx as usize);
                    if self.visited_in_source[t_idx] != self.current_source_gen {
                        self.visited_in_source[t_idx] = self.current_source_gen;
                        self.visible_counter[t_idx] = self.visible_counter[t_idx].saturating_add(1);
                        if source.is_detector {
                            self.detector_counter[t_idx] =
                                self.detector_counter[t_idx].saturating_add(1);
                        }
                    }
                    break;
                }

                in_blocker = target_is_blocker;

                // Normal visible cell
                let t_idx = (ty as usize) * (self.width as usize) + (tx as usize);
                if self.visited_in_source[t_idx] != self.current_source_gen {
                    self.visited_in_source[t_idx] = self.current_source_gen;
                    self.visible_counter[t_idx] = self.visible_counter[t_idx].saturating_add(1);
                    if source.is_detector {
                        self.detector_counter[t_idx] =
                            self.detector_counter[t_idx].saturating_add(1);
                    }
                }
            }
        }
    }

    /// Evaluates vision for all neutral watchtowers controlled by `player`.
    pub fn apply_watchtowers(&mut self, map: &GameMap, player: PlayerId) {
        for tower in map.watchtowers() {
            if tower.controlling_player == Some(player) {
                let source = VisionSource {
                    pos: tower.pos,
                    mover_type: MoverType::Air, // Elevated observation platform overlooks cliffs
                    sight_radius: tower.reveal_radius,
                    is_detector: false,
                };
                self.apply_vision_source(map, &source);
            }
        }
    }

    /// Finalizes the tick vision update, transitioning visibility states.
    ///
    /// Cells with `visible_counter > 0` become `VisibilityState::Visible`.
    /// Cells previously `Visible` that drop to 0 transition to `VisibilityState::Fogged`.
    /// Cells previously `Unexplored` that were not seen remain `Unexplored`.
    pub fn end_tick_update(&mut self) {
        for (i, &count) in self.visible_counter.iter().enumerate() {
            if count > 0 {
                self.states[i] = VisibilityState::Visible;
            } else if self.states[i] == VisibilityState::Visible {
                self.states[i] = VisibilityState::Fogged;
            }
        }
    }

    /// Returns the current visibility state of cell `(cell_x, cell_y)`.
    #[must_use]
    pub fn visibility_at(&self, cell_x: u32, cell_y: u32) -> VisibilityState {
        if cell_x >= self.width || cell_y >= self.height {
            return VisibilityState::Unexplored;
        }
        let idx = (cell_y as usize) * (self.width as usize) + (cell_x as usize);
        self.states[idx]
    }

    /// Checks whether cell `(cell_x, cell_y)` is currently covered by detection.
    #[must_use]
    pub fn is_detected(&self, cell_x: u32, cell_y: u32) -> bool {
        if cell_x >= self.width || cell_y >= self.height {
            return false;
        }
        let idx = (cell_y as usize) * (self.width as usize) + (cell_x as usize);
        self.detector_counter[idx] > 0
    }

    /// Returns an immutable slice of all visibility states.
    #[must_use]
    pub fn states(&self) -> &[VisibilityState] {
        &self.states
    }

    /// Returns an immutable slice of active vision source counters.
    #[must_use]
    pub fn visible_counter(&self) -> &[u16] {
        &self.visible_counter
    }

    /// Returns an immutable slice of active detector counters.
    #[must_use]
    pub fn detector_counter(&self) -> &[u16] {
        &self.detector_counter
    }
}
