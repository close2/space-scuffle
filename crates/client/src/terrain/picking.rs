//! Pure Amanatides-Woo 2D DDA raycaster for terrain surface picking.
//!
//! # Architectural Responsibilities
//! - **Exact Surface Raycasting:** Clips rays to the map AABB and traces cell-by-cell
//!   in 2D XZ order, testing plateau and ramp plane equations within each visited cell.
//! - **Front-to-Back Deterministic Selection:** Selects the topmost surface hit in front-to-back
//!   order, ensuring plateau lips occlude lower terrain behind them.
//! - **Abyss & Wall Ignoring:** Clicks into abyss chasms or against vertical cliff faces
//!   do not register as surface hits (cells are picked by their walkable surface tops).

use bevy::prelude::*;
use sim_map::grid::{GameMap, RampDir, TerrainKind, CLIFF_LEVEL_HEIGHT};
use sim_types::SimScalar;

use super::mesher::ABYSS_WALL_BOTTOM_Y;

/// Authoritative terrain ray intersection result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainHit {
    /// Coordinates `(x, y)` of the hit simulation cell.
    pub cell: (u32, u32),
    /// Exact 3D world position of the hit point.
    pub point: Vec3,
}

#[inline]
fn is_inside_cell_1d(val: f32, min_val: f32, max_val: f32, step: i32) -> bool {
    match step.cmp(&0) {
        std::cmp::Ordering::Greater => val >= min_val - 1e-4 && val < max_val - 1e-5,
        std::cmp::Ordering::Less => val > min_val + 1e-5 && val <= max_val + 1e-4,
        std::cmp::Ordering::Equal => val >= min_val - 1e-4 && val <= max_val + 1e-4,
    }
}

/// Traces a 3D ray through the terrain map and returns the first hit surface cell.
///
/// Returns `None` if the ray misses all cell surfaces (e.g. pointing into the void).
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
#[expect(
    clippy::too_many_lines,
    reason = "a single ordered sequence that shares its local state"
)]
pub fn pick_terrain(map: &GameMap, origin: Vec3, dir: Vec3) -> Option<TerrainHit> {
    if dir.length_squared() < 1e-8 {
        return None;
    }
    let norm_dir = dir.normalize();

    let width = map.width() as f32;
    let height = map.height() as f32;

    let aabb_min = Vec3::new(0.0, ABYSS_WALL_BOTTOM_Y, 0.0);
    let aabb_max = Vec3::new(width, 12.0, height);

    // 1. Ray - AABB slab intersection
    let mut t_near = -f32::INFINITY;
    let mut t_far = f32::INFINITY;

    for i in 0..3 {
        let (o, d, min_val, max_val) = match i {
            0 => (origin.x, norm_dir.x, aabb_min.x, aabb_max.x),
            1 => (origin.y, norm_dir.y, aabb_min.y, aabb_max.y),
            _ => (origin.z, norm_dir.z, aabb_min.z, aabb_max.z),
        };

        if d.abs() < 1e-8 {
            if o < min_val || o > max_val {
                return None;
            }
        } else {
            let t1 = (min_val - o) / d;
            let t2 = (max_val - o) / d;
            let (t_min, t_max) = if t1 < t2 { (t1, t2) } else { (t2, t1) };
            t_near = t_near.max(t_min);
            t_far = t_far.min(t_max);
            if t_near > t_far || t_far < 0.0 {
                return None;
            }
        }
    }

    let t_start = t_near.max(0.0);
    let p_start = origin + norm_dir * (t_start + 1e-5);

    let max_x = i32::try_from(map.width().saturating_sub(1)).unwrap_or(0);
    let max_y = i32::try_from(map.height().saturating_sub(1)).unwrap_or(0);
    let mut cx = (p_start.x.floor() as i32).clamp(0, max_x);
    let mut cy = (p_start.z.floor() as i32).clamp(0, max_y);

    let step_x = if norm_dir.x > 0.0 {
        1
    } else if norm_dir.x < 0.0 {
        -1
    } else {
        0
    };
    let step_z = if norm_dir.z > 0.0 {
        1
    } else if norm_dir.z < 0.0 {
        -1
    } else {
        0
    };

    let max_steps = (map.width() + map.height()) as usize * 3;

    for _ in 0..max_steps {
        let map_w = i32::try_from(map.width()).unwrap_or(i32::MAX);
        let map_h = i32::try_from(map.height()).unwrap_or(i32::MAX);
        if cx < 0 || cy < 0 || cx >= map_w || cy >= map_h {
            break;
        }

        let Ok(cell_x) = u32::try_from(cx) else {
            break;
        };
        let Ok(cell_y) = u32::try_from(cy) else {
            break;
        };

        let cell_min_x = cx as f32;
        let cell_max_x = (cx + 1) as f32;
        let cell_min_z = cy as f32;
        let cell_max_z = (cy + 1) as f32;

        let (t_entry_x, t_exit_x) = if norm_dir.x == 0.0 {
            (-f32::INFINITY, f32::INFINITY)
        } else {
            let t1 = (cell_min_x - origin.x) / norm_dir.x;
            let t2 = (cell_max_x - origin.x) / norm_dir.x;
            if t1 < t2 {
                (t1, t2)
            } else {
                (t2, t1)
            }
        };

        let (t_entry_z, t_exit_z) = if norm_dir.z == 0.0 {
            (-f32::INFINITY, f32::INFINITY)
        } else {
            let t1 = (cell_min_z - origin.z) / norm_dir.z;
            let t2 = (cell_max_z - origin.z) / norm_dir.z;
            if t1 < t2 {
                (t1, t2)
            } else {
                (t2, t1)
            }
        };

        let t_cell_entry = t_entry_x.max(t_entry_z).max(0.0);
        let t_cell_exit = t_exit_x.min(t_exit_z);

        if let Ok(cell) = map.cell(cell_x, cell_y) {
            if cell.terrain_kind != TerrainKind::Abyss {
                if let Some(prof) = map.ramp_profile(cell_x, cell_y) {
                    let h00 = prof.height_at(SimScalar::ZERO, SimScalar::ZERO).to_f32();
                    let h10 = prof.height_at(SimScalar::ONE, SimScalar::ZERO).to_f32();
                    let h01 = prof.height_at(SimScalar::ZERO, SimScalar::ONE).to_f32();

                    let (denom, num) = match prof.dir {
                        RampDir::North | RampDir::South => {
                            let slope_z = h01 - h00;
                            (
                                norm_dir.y - norm_dir.z * slope_z,
                                h00 + (origin.z - cell_min_z) * slope_z - origin.y,
                            )
                        }
                        RampDir::East | RampDir::West => {
                            let slope_x = h10 - h00;
                            (
                                norm_dir.y - norm_dir.x * slope_x,
                                h00 + (origin.x - cell_min_x) * slope_x - origin.y,
                            )
                        }
                        _ => (0.0, 0.0),
                    };

                    if denom.abs() > 1e-7 {
                        let t_hit = num / denom;
                        if t_hit >= t_cell_entry - 1e-4
                            && t_hit <= t_cell_exit + 1e-4
                            && t_hit >= 0.0
                        {
                            let hit_point = origin + norm_dir * t_hit;
                            let in_x =
                                is_inside_cell_1d(hit_point.x, cell_min_x, cell_max_x, step_x);
                            let in_z =
                                is_inside_cell_1d(hit_point.z, cell_min_z, cell_max_z, step_z);

                            if in_x && in_z {
                                return Some(TerrainHit {
                                    cell: (cell_x, cell_y),
                                    point: hit_point,
                                });
                            }
                        }
                    }
                } else {
                    let plateau_y = f32::from(cell.cliff_level.0) * CLIFF_LEVEL_HEIGHT as f32;
                    if norm_dir.y.abs() > 1e-7 {
                        let t_hit = (plateau_y - origin.y) / norm_dir.y;
                        if t_hit >= t_cell_entry - 1e-4
                            && t_hit <= t_cell_exit + 1e-4
                            && t_hit >= 0.0
                        {
                            let hit_point = origin + norm_dir * t_hit;
                            let in_x =
                                is_inside_cell_1d(hit_point.x, cell_min_x, cell_max_x, step_x);
                            let in_z =
                                is_inside_cell_1d(hit_point.z, cell_min_z, cell_max_z, step_z);

                            if in_x && in_z {
                                return Some(TerrainHit {
                                    cell: (cell_x, cell_y),
                                    point: hit_point,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Advance DDA
        if t_exit_x < t_exit_z {
            if step_x == 0 {
                break;
            }
            cx += step_x;
        } else {
            if step_z == 0 {
                break;
            }
            cy += step_z;
        }
    }

    None
}
