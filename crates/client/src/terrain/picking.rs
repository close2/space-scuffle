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
use sim_map::grid::{CliffLevel, GameMap, RampDir, RampProfile, TerrainKind, CLIFF_LEVEL_HEIGHT};
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
/// `origin` is in world space (simulation X to Bevy X, simulation Y to Bevy Z, altitude to Bevy
/// Y); `dir` need not be normalised. The ray is clipped to the map's bounding box, then
/// `traverse_cells` walks the cells it crosses in front-to-back order.
///
/// Returns `None` if the ray misses all cell surfaces (e.g. pointing into the void).
#[must_use]
pub fn pick_terrain(map: &GameMap, origin: Vec3, dir: Vec3) -> Option<TerrainHit> {
    if dir.length_squared() < 1e-8 {
        return None;
    }
    let norm_dir = dir.normalize();
    let t_start = clip_to_map_bounds(map, origin, norm_dir)?;
    traverse_cells(map, origin, norm_dir, t_start)
}

/// Ray-AABB slab test against the map's bounding box, from the abyss floor to 12 world units
/// high. Returns the ray parameter where tracing starts (never negative), or `None` if the ray
/// misses the box or the box lies behind the origin.
#[expect(
    clippy::cast_precision_loss,
    reason = "map dimensions stay far below the range of f32"
)]
fn clip_to_map_bounds(map: &GameMap, origin: Vec3, norm_dir: Vec3) -> Option<f32> {
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

    Some(t_near.max(0.0))
}

/// Sign of a ray direction component as a DDA cell step: `1`, `-1` or `0`.
fn dda_step(component: f32) -> i32 {
    if component > 0.0 {
        1
    } else if component < 0.0 {
        -1
    } else {
        0
    }
}

/// A normalised ray and its DDA steps along world X and Z.
#[derive(Debug, Clone, Copy)]
struct PickRay {
    origin: Vec3,
    dir: Vec3,
    step_x: i32,
    step_z: i32,
}

/// The cell under test and the ray's parameter interval inside its XZ footprint.
#[derive(Debug, Clone, Copy)]
struct CellSpan {
    cell: (u32, u32),
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
    /// Ray parameter where the ray enters the footprint (never negative).
    t_entry: f32,
    /// Ray parameter where the ray leaves the footprint.
    t_exit: f32,
}

/// Parameter interval `(entry, exit)` of the ray inside the slab `[min_v, max_v]` on one axis;
/// unbounded when the ray is parallel to the slab.
fn slab_span(origin: f32, dir: f32, min_v: f32, max_v: f32) -> (f32, f32) {
    if dir == 0.0 {
        (-f32::INFINITY, f32::INFINITY)
    } else {
        let t1 = (min_v - origin) / dir;
        let t2 = (max_v - origin) / dir;
        if t1 < t2 {
            (t1, t2)
        } else {
            (t2, t1)
        }
    }
}

/// Amanatides-Woo DDA over the map's cells, starting at ray parameter `t_start`. Each visited,
/// non-abyss cell is tested against its ramp plane ([`intersect_ramp`]) or, if it is not a ramp,
/// its plateau plane ([`intersect_floor`]); the first hit is returned.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
fn traverse_cells(map: &GameMap, origin: Vec3, norm_dir: Vec3, t_start: f32) -> Option<TerrainHit> {
    let p_start = origin + norm_dir * (t_start + 1e-5);

    let max_x = i32::try_from(map.width().saturating_sub(1)).unwrap_or(0);
    let max_y = i32::try_from(map.height().saturating_sub(1)).unwrap_or(0);
    let mut cx = (p_start.x.floor() as i32).clamp(0, max_x);
    let mut cy = (p_start.z.floor() as i32).clamp(0, max_y);

    let ray = PickRay {
        origin,
        dir: norm_dir,
        step_x: dda_step(norm_dir.x),
        step_z: dda_step(norm_dir.z),
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

        let min_x = cx as f32;
        let max_x = (cx + 1) as f32;
        let min_z = cy as f32;
        let max_z = (cy + 1) as f32;

        let (t_entry_x, t_exit_x) = slab_span(origin.x, norm_dir.x, min_x, max_x);
        let (t_entry_z, t_exit_z) = slab_span(origin.z, norm_dir.z, min_z, max_z);

        let span = CellSpan {
            cell: (cell_x, cell_y),
            min_x,
            max_x,
            min_z,
            max_z,
            t_entry: t_entry_x.max(t_entry_z).max(0.0),
            t_exit: t_exit_x.min(t_exit_z),
        };

        if let Ok(cell) = map.cell(cell_x, cell_y) {
            if cell.terrain_kind != TerrainKind::Abyss {
                let hit = match map.ramp_profile(cell_x, cell_y) {
                    Some(prof) => intersect_ramp(&ray, &span, prof),
                    None => intersect_floor(&ray, &span, cell.cliff_level),
                };
                if hit.is_some() {
                    return hit;
                }
            }
        }

        // Advance DDA
        if t_exit_x < t_exit_z {
            if ray.step_x == 0 {
                break;
            }
            cx += ray.step_x;
        } else {
            if ray.step_z == 0 {
                break;
            }
            cy += ray.step_z;
        }
    }

    None
}

/// Intersects the ray with the sloped plane of a straight ramp cell. Diagonal ramps have no plane
/// here and never hit.
fn intersect_ramp(ray: &PickRay, span: &CellSpan, prof: RampProfile) -> Option<TerrainHit> {
    let (origin, norm_dir) = (ray.origin, ray.dir);
    let h00 = prof.height_at(SimScalar::ZERO, SimScalar::ZERO).to_f32();
    let h10 = prof.height_at(SimScalar::ONE, SimScalar::ZERO).to_f32();
    let h01 = prof.height_at(SimScalar::ZERO, SimScalar::ONE).to_f32();

    let (denom, num) = match prof.dir {
        RampDir::North | RampDir::South => {
            let slope_z = h01 - h00;
            (
                norm_dir.y - norm_dir.z * slope_z,
                h00 + (origin.z - span.min_z) * slope_z - origin.y,
            )
        }
        RampDir::East | RampDir::West => {
            let slope_x = h10 - h00;
            (
                norm_dir.y - norm_dir.x * slope_x,
                h00 + (origin.x - span.min_x) * slope_x - origin.y,
            )
        }
        _ => (0.0, 0.0),
    };

    if denom.abs() > 1e-7 {
        accept_hit(ray, span, num / denom)
    } else {
        None
    }
}

/// Intersects the ray with the flat plateau of a cell at cliff level `level`.
#[expect(
    clippy::cast_precision_loss,
    reason = "the cliff level height is a small integer"
)]
fn intersect_floor(ray: &PickRay, span: &CellSpan, level: CliffLevel) -> Option<TerrainHit> {
    let plateau_y = f32::from(level.0) * CLIFF_LEVEL_HEIGHT as f32;
    if ray.dir.y.abs() > 1e-7 {
        accept_hit(ray, span, (plateau_y - ray.origin.y) / ray.dir.y)
    } else {
        None
    }
}

/// Accepts the plane hit at ray parameter `t_hit` if it lies inside the cell's ray interval (with
/// a `1e-4` tolerance), in front of the origin, and inside the cell's XZ footprint under the
/// half-open rule of [`is_inside_cell_1d`].
fn accept_hit(ray: &PickRay, span: &CellSpan, t_hit: f32) -> Option<TerrainHit> {
    if t_hit >= span.t_entry - 1e-4 && t_hit <= span.t_exit + 1e-4 && t_hit >= 0.0 {
        let hit_point = ray.origin + ray.dir * t_hit;
        let in_x = is_inside_cell_1d(hit_point.x, span.min_x, span.max_x, ray.step_x);
        let in_z = is_inside_cell_1d(hit_point.z, span.min_z, span.max_z, ray.step_z);
        if in_x && in_z {
            return Some(TerrainHit {
                cell: span.cell,
                point: hit_point,
            });
        }
    }
    None
}
