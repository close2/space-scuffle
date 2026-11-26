//! The drawn terrain never rises inside walkable ground: invariant W1 of the mesher, triangle by
//! triangle (plan `docs/plans/2026-11-30-terrain-and-air-indicators.md` §5.1, T-B).
//!
//! # Responsibility
//! The simulation's boundary between two plateaus is the cell edge, and both sides are walkable.
//! W1 states what "the drawn cliff coincides with that boundary" means for every feature at once:
//! inside the footprint of a walkable cell (a non-abyss plateau or a ramp), no terrain triangle
//! rises above the cell's simulation surface, except a ramp's guard rail within `RAIL_WIDTH` of
//! its side edge and `RAIL_HEIGHT` above the surface. These tests mesh whole maps with the
//! production mesher and check every triangle near every walkable cell.
//!
//! # Method
//! For walkable cell `(x, y)` with surface `S`, every triangle near it is clipped to the open
//! column `Q = (x + τ, x + 1 − τ) × (y + τ, y + 1 − τ)` ([`terrain_mesh::clip_to_column`]). A
//! clipped vertex higher than `S` there by more than `ε` is a violation. A violation whose clipped
//! part lies in the nook of a concave cliff corner (a dual cell whose lone low corner, or saddle
//! low corner, is this cell) is a **corner** violation (B-245); any other is a **profile**
//! violation (B-243).
//!
//! # Tolerances, derived from `f32` rounding
//! Mesh coordinates lie below `M` in magnitude and altitudes below `H_max`; a clipped vertex is
//! one `a + t (b − a)` per plane, a few roundings. So `τ = 16·ulp(M)` and `ε = 16·ulp(H_max)`,
//! with `ulp(v) = 2^(⌊log₂ v⌋ − 23)`, both computed from the meshed map. The tests assert the
//! premise that every vertex lies within `M`.
//!
//! # Inputs
//! `showcase_canyon` and `default_arena` through the real `MapFile` loader, the goldens' seeded
//! synthetic map, and the ramp-geometry fixture (`support/terrain_mesh.rs`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/terrain_mesh.rs"]
mod terrain_mesh;

use bevy::prelude::{Vec2, Vec3};
use client::terrain::mesher::{
    cliff_level_height, dual_corner_levels, surface_class, SurfaceClass, ABYSS_WALL_BOTTOM_Y,
    RAIL_HEIGHT,
};
use sim_map::grid::{GameMap, RampProfile};
use sim_types::SimScalar;
use terrain_mesh::{clip_to_column, footprint_margins, MapMesh};

/// Rounding steps one clipped vertex may carry, as a multiple of the unit in the last place.
const ROUNDING_ULPS: f32 = 16.0;

/// The unit in the last place of `f32` values of magnitude up to `v`: `2^(⌊log₂ v⌋ − 23)`.
fn ulp(v: f32) -> f32 {
    let exponent = v.abs().log2().floor() as i32;
    2.0_f32.powi(exponent - f32::MANTISSA_DIGITS as i32 + 1)
}

/// The derived tolerances of one meshed map.
#[derive(Debug, Clone, Copy)]
struct Tolerances {
    /// Horizontal inset of the open column, world units.
    tau: f32,
    /// Height allowance above the surface, world units.
    epsilon: f32,
}

impl Tolerances {
    /// `τ` and `ε` of `mesh` over `map`, after asserting their premise: every coordinate lies
    /// within `M` and every altitude within `H_max`.
    fn of(map: &GameMap, mesh: &MapMesh) -> Self {
        let max_level = (0..map.height())
            .flat_map(|y| (0..map.width()).map(move |x| (x, y)))
            .filter_map(|(x, y)| map.cell(x, y).ok())
            .map(|cell| i32::from(cell.cliff_level.0))
            .max()
            .unwrap_or(0);
        let h_max = ABYSS_WALL_BOTTOM_Y
            .abs()
            .max(cliff_level_height(max_level) + RAIL_HEIGHT);
        let extent = mesh
            .tris
            .iter()
            .flatten()
            .map(|p| p.x.abs().max(p.z.abs()))
            .fold(0.0_f32, f32::max);
        let m = extent.max(map.width().max(map.height()) as f32);
        for p in mesh.tris.iter().flatten() {
            assert!(
                p.x.abs() <= m && p.z.abs() <= m && p.y.abs() <= h_max,
                "premise: vertex {p} lies within M = {m} and H_max = {h_max}"
            );
        }
        Self {
            tau: ROUNDING_ULPS * ulp(m),
            epsilon: ROUNDING_ULPS * ulp(h_max),
        }
    }
}

/// The simulation surface of a walkable cell.
#[derive(Debug, Clone, Copy)]
enum Surface {
    /// A plateau at this altitude.
    Flat(f32),
    /// A ramp: its corner altitudes `[bl, br, tr, tl]`, a plane over the cell.
    Ramp(RampProfile, [f32; 4]),
}

impl Surface {
    /// The surface of cell `(x, y)`, or `None` for the void.
    fn of(map: &GameMap, x: u32, y: u32) -> Option<Self> {
        match surface_class(map, x as i32, y as i32) {
            SurfaceClass::Void => None,
            SurfaceClass::Plateau(level) => Some(Self::Flat(cliff_level_height(level))),
            SurfaceClass::Ramp(profile) => {
                let at = |fx: SimScalar, fy: SimScalar| profile.height_at(fx, fy).to_f32();
                let (zero, one) = (SimScalar::ZERO, SimScalar::ONE);
                Some(Self::Ramp(
                    profile,
                    [at(zero, zero), at(one, zero), at(one, one), at(zero, one)],
                ))
            }
        }
    }

    /// Altitude at cell-local `(fx, fz)`; the ramp's plane interpolated from its corners.
    fn at(&self, fx: f32, fz: f32) -> f32 {
        match *self {
            Self::Flat(h) => h,
            Self::Ramp(_, [bl, br, tr, tl]) => {
                let bottom = bl + (br - bl) * fx;
                let top = tl + (tr - tl) * fx;
                bottom + (top - bottom) * fz
            }
        }
    }
}

/// Which bug a violation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// In the nook of a concave cliff corner (B-245).
    Corner,
    /// Anywhere else (B-243).
    Profile,
}

/// One triangle that rises inside a walkable cell.
#[derive(Debug, Clone)]
struct Violation {
    cell: (u32, u32),
    class: Class,
    /// Largest height above the surface, world units.
    height: f32,
    /// Largest horizontal depth into the cell of a violating vertex, world units.
    depth: f32,
    triangle: [Vec3; 3],
}

/// The centres `(cx, cy)` of the dual cells in which cell `(x, y)` is a lone low corner or a
/// saddle's low corner at some threshold: its level is below both its edge-adjacent corners'.
fn nook_corners(map: &GameMap, x: u32, y: u32) -> Vec<Vec2> {
    // Cell (x, y) is corner k of the dual cell centred on its corner (di, dj).
    let duals = [(x + 1, y + 1, 0), (x, y + 1, 1), (x, y, 2), (x + 1, y, 3)];
    duals
        .into_iter()
        .filter(|&(di, dj, k)| {
            let (levels, _) = dual_corner_levels(map, di, dj);
            let (own, a, b) = (levels[k], levels[(k + 1) % 4], levels[(k + 3) % 4]);
            own < a.min(b)
        })
        .map(|(di, dj, _)| Vec2::new(di as f32, dj as f32))
        .collect()
}

/// Every violation of W1 in `map`'s mesh.
fn violations(map: &GameMap) -> Vec<Violation> {
    let mesh = MapMesh::build(map);
    let tol = Tolerances::of(map, &mesh);
    let mut found = Vec::new();
    for y in 0..map.height() {
        for x in 0..map.width() {
            let Some(surface) = Surface::of(map, x, y) else {
                continue;
            };
            let nooks = nook_corners(map, x, y);
            for t in mesh.near(x, y) {
                if let Some(v) = check_triangle(t, (x, y), surface, &nooks, tol) {
                    found.push(v);
                }
            }
        }
    }
    found
}

/// Checks triangle `t` against walkable cell `cell` with surface `surface`.
fn check_triangle(
    t: &[Vec3; 3],
    cell: (u32, u32),
    surface: Surface,
    nooks: &[Vec2],
    tol: Tolerances,
) -> Option<Violation> {
    let origin = Vec2::new(cell.0 as f32, cell.1 as f32);
    let clip = clip_to_column(
        t,
        origin + Vec2::splat(tol.tau),
        origin + Vec2::splat(1.0 - tol.tau),
    );
    // A ramp's guard rail may stand up to RAIL_HEIGHT above the surface within its margins.
    let rail = match surface {
        Surface::Ramp(profile, _) => Some(footprint_margins(profile.dir, tol.tau)),
        Surface::Flat(_) => None,
    };
    let mut worst: Option<(f32, f32)> = None;
    let mut all_in_nook = true;
    for p in clip.points() {
        let (fx, fz) = (p.x - origin.x, p.z - origin.y);
        let mut allowance = tol.epsilon;
        if let Some((mx, mz)) = rail {
            let in_margin = fx < mx || fx > 1.0 - mx || fz < mz || fz > 1.0 - mz;
            if in_margin {
                allowance += RAIL_HEIGHT;
            }
        }
        let above = p.y - surface.at(fx, fz);
        if above <= allowance {
            continue;
        }
        let depth = fx.min(1.0 - fx).min(fz).min(1.0 - fz);
        let in_nook = nooks
            .iter()
            .any(|c| (p.x - c.x).abs() + (p.z - c.y).abs() <= 0.5 + tol.tau);
        all_in_nook &= in_nook;
        let (h, d) = worst.unwrap_or((f32::NEG_INFINITY, 0.0));
        worst = Some((h.max(above), d.max(depth)));
    }
    worst.map(|(height, depth)| Violation {
        cell,
        class: if all_in_nook {
            Class::Corner
        } else {
            Class::Profile
        },
        height,
        depth,
        triangle: *t,
    })
}

/// Fails with a report of the violations `classes` select.
fn assert_no_violations(name: &str, map: &GameMap, classes: &[Class]) {
    let all = violations(map);
    let selected: Vec<&Violation> = all.iter().filter(|v| classes.contains(&v.class)).collect();
    if selected.is_empty() {
        println!(
            "{name}: no {classes:?} violation of W1 ({} of other classes)",
            all.len()
        );
        return;
    }
    let cells: std::collections::BTreeSet<(u32, u32)> = selected.iter().map(|v| v.cell).collect();
    let height = selected.iter().map(|v| v.height).fold(0.0_f32, f32::max);
    let depth = selected.iter().map(|v| v.depth).fold(0.0_f32, f32::max);
    let lines: Vec<String> = selected
        .iter()
        .take(10)
        .map(|v| {
            format!(
                "cell {:?} {:?}: {:.4} above the surface, {:.4} deep, triangle {:?}",
                v.cell, v.class, v.height, v.depth, v.triangle
            )
        })
        .collect();
    panic!(
        "{name}: {} triangles in {} walkable cells rise above the surface ({classes:?}); \
         largest {height:.4} above, {depth:.4} deep; e.g.:\n{}",
        selected.len(),
        cells.len(),
        lines.join("\n")
    );
}

// --- T-B1: no profile violation (B-243) --------------------------------------------------------

#[test]
fn no_profile_rises_inside_walkable_ground_showcase_canyon() {
    assert_no_violations(
        "showcase_canyon",
        &terrain_mesh::load_shipped_map("showcase_canyon.scmap.ron"),
        &[Class::Profile],
    );
}

#[test]
fn no_profile_rises_inside_walkable_ground_default_arena() {
    assert_no_violations(
        "default_arena",
        &terrain_mesh::load_shipped_map("default_arena.scmap.ron"),
        &[Class::Profile],
    );
}

#[test]
fn no_profile_rises_inside_walkable_ground_synthetic() {
    assert_no_violations(
        "synthetic",
        &terrain_mesh::synthetic_map(),
        &[Class::Profile],
    );
}

#[test]
fn no_profile_rises_inside_walkable_ground_ramp_fixture() {
    assert_no_violations(
        "ramp_fixture",
        &terrain_mesh::fixture_map(),
        &[Class::Profile],
    );
}

// --- T-B2: no violation of either class (B-243 and B-245) --------------------------------------

#[test]
fn no_terrain_rises_inside_walkable_ground_showcase_canyon() {
    assert_no_violations(
        "showcase_canyon",
        &terrain_mesh::load_shipped_map("showcase_canyon.scmap.ron"),
        &[Class::Profile, Class::Corner],
    );
}

#[test]
fn no_terrain_rises_inside_walkable_ground_default_arena() {
    assert_no_violations(
        "default_arena",
        &terrain_mesh::load_shipped_map("default_arena.scmap.ron"),
        &[Class::Profile, Class::Corner],
    );
}

#[test]
fn no_terrain_rises_inside_walkable_ground_synthetic() {
    assert_no_violations(
        "synthetic",
        &terrain_mesh::synthetic_map(),
        &[Class::Profile, Class::Corner],
    );
}

#[test]
fn no_terrain_rises_inside_walkable_ground_ramp_fixture() {
    assert_no_violations(
        "ramp_fixture",
        &terrain_mesh::fixture_map(),
        &[Class::Profile, Class::Corner],
    );
}
