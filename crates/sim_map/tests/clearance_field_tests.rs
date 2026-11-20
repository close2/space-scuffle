//! Clearance must be a symmetric property of the terrain and of a unit's real size.
//!
//! # Responsibility
//! Pins that a unit's ability to stand or pass somewhere depends on the free space around it,
//! not on which side of it a wall happens to lie, and that zero-width diagonal gaps are closed.
//!
//! # How it fits
//! Every ground path is built on this field, so a directional bias here shows up as large units
//! that refuse to approach walls on two sides of the map only.
//!
//! # Invariants
//! Symmetry tests compare a map with its own mirror image, so they need no hand-computed
//! expected values and cannot share a bias with the implementation.

#[path = "support/radii.rs"]
mod radii;
use radii::{LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, MapError, PathRequest, Pathfinder};
use sim_types::{EntityId, SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const SIDE: u32 = 24;

fn open_map() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(GameMap::new(
        SIDE,
        SIDE,
        MapCell::default_ground(CliffLevel(1)),
    )?)
}

fn block(map: &mut GameMap, x: u32, y: u32) -> sim_map::Result<()> {
    map.cell_mut(x, y)?.flags = CellFlags::empty();
    Ok(())
}

fn centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn route_exists(map: &GameMap, from: (u32, u32), to: (u32, u32), radius: SimScalar) -> bool {
    Pathfinder::new(map)
        .find_path(
            map,
            &PathRequest {
                start: centre(from.0, from.1),
                goal: centre(to.0, to.1),
                radius,
            },
        )
        .is_ok()
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// The route of a disc of `radius` between two world points (the mesh refuses a
/// goal where the whole disc does not fit, plan Amendment 2).
fn route_between(
    map: &GameMap,
    start: SimVec2,
    goal: SimVec2,
    radius: SimScalar,
) -> sim_map::Result<sim_map::PathResult> {
    Pathfinder::new(map).find_path(
        map,
        &PathRequest {
            start,
            goal,
            radius,
        },
    )
}

/// Mirrors a blocked-cell list through the map centre on both axes.
fn mirrored(cells: &[(u32, u32)]) -> Vec<(u32, u32)> {
    cells
        .iter()
        .map(|&(x, y)| (SIDE - 1 - x, SIDE - 1 - y))
        .collect()
}

fn map_with(cells: &[(u32, u32)]) -> Result<GameMap, Box<dyn std::error::Error>> {
    let mut map = open_map()?;
    for &(x, y) in cells {
        block(&mut map, x, y)?;
    }
    Ok(map)
}

fn wall_samples() -> Vec<Vec<(u32, u32)>> {
    let vertical: Vec<_> = (0..SIDE).map(|y| (12, y)).collect();
    let horizontal: Vec<_> = (0..SIDE).map(|x| (x, 9)).collect();
    let pillar = vec![(7, 7), (8, 7), (7, 8), (8, 8)];
    let scattered = vec![(3, 4), (15, 6), (9, 18), (20, 20), (6, 13)];
    vec![vertical, horizontal, pillar, scattered]
}

#[test]
fn m8_clearance_is_invariant_under_map_rotation() -> TestResult {
    for cells in wall_samples() {
        let forward = Pathfinder::new(&map_with(&cells)?);
        let rotated = Pathfinder::new(&map_with(&mirrored(&cells))?);
        for y in 0..SIDE {
            for x in 0..SIDE {
                assert_eq!(
                    forward.clearance_at(x, y),
                    rotated.clearance_at(SIDE - 1 - x, SIDE - 1 - y),
                    "clearance differs between ({x}, {y}) and its rotation for {cells:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn m8_large_unit_can_stand_beside_a_wall_on_any_side() -> TestResult {
    // A vertical wall at x = 12: the cells either side of it are mirror images.
    let map = map_with(&(0..SIDE).map(|y| (12, y)).collect::<Vec<_>>())?;
    let large = LARGE_RADIUS;
    assert!(
        route_exists(&map, (5, 5), (10, 5), large),
        "west of wall, one cell clear"
    );
    assert!(
        route_exists(&map, (18, 5), (14, 5), large),
        "east of wall, one cell clear"
    );
    // The wall occupies 12 <= x <= 13: a disc of radius 1 touches it at x = 11 and x = 14.
    assert!(
        route_between(&map, at(5.5, 5.5), at(11.0, 5.5), large).is_ok(),
        "touching the wall from the west"
    );
    assert!(
        route_between(&map, at(18.5, 5.5), at(14.0, 5.5), large).is_ok(),
        "touching the wall from the east"
    );
    // A goal the disc overlaps is refused on both sides alike.
    for (start, goal) in [((5.5, 5.5), (11.5, 5.5)), ((18.5, 5.5), (13.5, 5.5))] {
        assert!(
            matches!(
                route_between(&map, at(start.0, start.1), at(goal.0, goal.1), large),
                Err(MapError::InvalidGoal(_))
            ),
            "{goal:?} overlaps the wall"
        );
    }
    Ok(())
}

#[test]
fn m8_large_unit_can_reach_every_map_edge() -> TestResult {
    let map = open_map()?;
    let large = LARGE_RADIUS;
    // The disc touches the map border with its centre one radius inside it.
    let far = f64::from(SIDE) - 1.0;
    for goal in [(1.0, 12.5), (far, 12.5), (12.5, 1.0), (12.5, far)] {
        assert!(
            route_between(&map, at(12.5, 12.5), at(goal.0, goal.1), large).is_ok(),
            "edge point {goal:?}"
        );
    }
    Ok(())
}

#[test]
fn m8_units_fit_through_gaps_as_wide_as_their_real_diameter() -> TestResult {
    let gap = |width: u32| -> Result<GameMap, Box<dyn std::error::Error>> {
        let cells: Vec<_> = (0..SIDE)
            .filter(|&y| !(10..10 + width).contains(&y))
            .map(|y| (12, y))
            .collect();
        map_with(&cells)
    };
    let crossing = |map: &GameMap, radius| route_exists(map, (6, 11), (18, 11), radius);

    let one = gap(1)?;
    assert!(crossing(&one, SMALL_RADIUS), "0.75 fits a 1-cell gap");
    assert!(
        !crossing(&one, MEDIUM_RADIUS),
        "1.25 does not fit a 1-cell gap"
    );
    assert!(
        !crossing(&one, LARGE_RADIUS),
        "2.0 does not fit a 1-cell gap"
    );

    let two = gap(2)?;
    assert!(crossing(&two, MEDIUM_RADIUS), "1.25 fits a 2-cell gap");
    assert!(crossing(&two, LARGE_RADIUS), "2.0 fits a 2-cell gap");
    Ok(())
}

#[test]
fn m8_known_structures_block_symmetrically() -> TestResult {
    let mut map = open_map()?;
    map.place_structure(
        EntityId(1),
        (11, 4),
        sim_map::StructureFootprint::square(2, 16),
    )?;
    // The structure occupies 11 <= x <= 13: a disc of radius 1 touches it at x = 10 and 14.
    let reach = |x: f64| route_between(&map, at(x, 1.5), at(x, 8.5), LARGE_RADIUS).is_ok();
    assert!(reach(10.0), "west face of the structure");
    assert!(reach(14.0), "east face of the structure");
    Ok(())
}

#[test]
fn m8_cliff_edges_count_as_walls() -> TestResult {
    let mut map = open_map()?;
    for y in 12..SIDE {
        for x in 0..SIDE {
            map.cell_mut(x, y)?.cliff_level = CliffLevel(2);
        }
    }
    let pathfinder = Pathfinder::new(&map);
    assert!(pathfinder.clearance_at(5, 11) < pathfinder.clearance_at(5, 8));
    assert_eq!(
        pathfinder.clearance_at(5, 11),
        pathfinder.clearance_at(5, 12)
    );
    Ok(())
}

#[test]
fn f7_no_tier_passes_between_structures_touching_at_a_corner() -> TestResult {
    // Two 2x2 buildings touch only at the vertex (12, 12). Terrain staircases extend them along
    // the main diagonal into a complete wall separating north-west from south-east.
    let mut map = open_map()?;
    for i in 0..10 {
        block(&mut map, i, i)?;
        block(&mut map, i + 1, i)?;
    }
    for i in 14..SIDE {
        block(&mut map, i, i)?;
        if i + 1 < SIDE {
            block(&mut map, i + 1, i)?;
        }
    }
    map.place_structure(
        EntityId(1),
        (10, 10),
        sim_map::StructureFootprint::square(2, 2),
    )?;
    map.place_structure(
        EntityId(2),
        (12, 12),
        sim_map::StructureFootprint::square(2, 2),
    )?;
    for radius in [SMALL_RADIUS, MEDIUM_RADIUS, LARGE_RADIUS] {
        let request = PathRequest {
            start: centre(3, 18),
            goal: centre(18, 3),
            radius,
        };
        assert!(
            matches!(
                Pathfinder::new(&map).find_path(&map, &request),
                Err(MapError::NoPathFound { .. })
            ),
            "a corner contact has no width for {radius:?}"
        );
    }
    Ok(())
}

#[test]
fn f7_small_units_keep_passing_real_one_cell_gaps() -> TestResult {
    let cells: Vec<_> = (0..SIDE).filter(|&y| y != 12).map(|y| (12, y)).collect();
    let map = map_with(&cells)?;
    assert!(route_exists(&map, (6, 6), (18, 18), SMALL_RADIUS));
    Ok(())
}

#[test]
fn f7_diagonal_segment_between_touching_structures_is_rejected() -> TestResult {
    let mut map = open_map()?;
    map.place_structure(
        EntityId(1),
        (5, 6),
        sim_map::StructureFootprint::square(1, 1),
    )?;
    map.place_structure(
        EntityId(2),
        (6, 5),
        sim_map::StructureFootprint::square(1, 1),
    )?;
    let mut pathfinder = Pathfinder::new(&map);
    let request = PathRequest {
        start: centre(5, 5),
        goal: centre(6, 6),
        radius: SMALL_RADIUS,
    };
    assert!(!pathfinder.segment_is_passable(&map, &request.into(), request.start, request.goal));
    Ok(())
}

/// The §2.3 contours as `(cells, half extents and chamfer in quarter cells)`, and seeded
/// placements of them on an open map: `(cell, footprint)` pairs that fit.
fn seeded_layout(seed: u64, square: bool) -> Vec<((u32, u32), sim_map::StructureFootprint)> {
    const SHAPES: [(u8, u16); 5] = [(5, 6), (3, 2), (3, 3), (2, 2), (1, 0)];
    let mut state = seed;
    let mut next = |bound: u64| {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % bound
    };
    let Ok(mut probe) = open_map() else {
        return Vec::new();
    };
    let mut layout = Vec::new();
    for attempt in 0..40 {
        let (size, chamfer) = SHAPES[usize::try_from(next(5)).unwrap_or(0)];
        let footprint = if square {
            sim_map::StructureFootprint::square(size, size)
        } else {
            sim_map::StructureFootprint {
                width: size,
                height: size,
                shape: sim_types::FootprintShape::Box {
                    half_width_q: 2 * u16::from(size),
                    half_height_q: 2 * u16::from(size),
                    chamfer_q: chamfer,
                },
            }
        };
        let cell = (
            u32::try_from(next(u64::from(SIDE - 6))).unwrap_or(0) + 1,
            u32::try_from(next(u64::from(SIDE - 6))).unwrap_or(0) + 1,
        );
        if probe
            .place_structure(EntityId(attempt), cell, footprint)
            .is_ok()
        {
            layout.push((cell, footprint));
        }
    }
    layout
}

/// D-6: a structure whose shape is the full box of its cells marks the clearance field exactly
/// as the cells did; the reference is the same map with those cells as unwalkable terrain,
/// which marks every lattice point of the closed cells.
#[test]
fn square_shapes_keep_todays_clearance() -> TestResult {
    for seed in 0..8 {
        let layout = seeded_layout(seed, true);
        let (mut placed, mut reference) = (open_map()?, open_map()?);
        for (index, &(cell, footprint)) in layout.iter().enumerate() {
            placed.place_structure(EntityId(u32::try_from(index)?), cell, footprint)?;
            for y in cell.1..cell.1 + u32::from(footprint.height) {
                for x in cell.0..cell.0 + u32::from(footprint.width) {
                    block(&mut reference, x, y)?;
                }
            }
        }
        let (a, b) = (Pathfinder::new(&placed), Pathfinder::new(&reference));
        for y in 0..SIDE {
            for x in 0..SIDE {
                assert_eq!(
                    a.clearance_at(x, y),
                    b.clearance_at(x, y),
                    "seed {seed}, cell ({x}, {y})"
                );
            }
        }
    }
    Ok(())
}

/// Exact Euclidean distance, in world units, from the half-cell lattice point `(hx, hy)`
/// (world `(hx / 2, hy / 2)`) to the closed shape of `placed`.
fn distance_to_shape(placed: &sim_map::PlacedStructure, (hx, hy): (i64, i64)) -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "test coordinates are below 2^10"
    )]
    let p = (hx as f64 / 2.0, hy as f64 / 2.0);
    let footprint = placed.footprint();
    let vertices = footprint.contour();
    let count = vertices.len();
    let world = |v: sim_map::LatticePoint| (f64::from(v.x) / 4.0, f64::from(v.y) / 4.0);
    let inside = placed.contains_open_scaled(2 * hx, 2 * hy, 1);
    if inside {
        return 0.0;
    }
    (0..count)
        .map(|i| {
            let (u, v) = (world(vertices[i]), world(vertices[(i + 1) % count]));
            let (dx, dy) = (v.0 - u.0, v.1 - u.1);
            let t = (((p.0 - u.0) * dx + (p.1 - u.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
            ((p.0 - u.0 - t * dx).powi(2) + (p.1 - u.1 - t * dy).powi(2)).sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

/// D-6: with contours the field never claims more room than there is. On an open map, a cell's
/// clearance (eighths of a world unit, the best of its nine half-cell sample points) is at most
/// the true distance from those points to the map border and every shape.
#[test]
fn a_contours_clearance_never_exceeds_the_true_clearance() -> TestResult {
    let mut cut_cells = 0;
    for seed in 0..8 {
        let layout = seeded_layout(seed, false);
        let mut map = open_map()?;
        for (index, &(cell, footprint)) in layout.iter().enumerate() {
            map.place_structure(EntityId(u32::try_from(index)?), cell, footprint)?;
        }
        let pathfinder = Pathfinder::new(&map);
        let side = f64::from(SIDE);
        for y in 0..SIDE {
            for x in 0..SIDE {
                let mut best = 0.0_f64;
                for oy in 0..=2 {
                    for ox in 0..=2 {
                        let p = (
                            f64::from(x) + f64::from(ox) / 2.0,
                            f64::from(y) + f64::from(oy) / 2.0,
                        );
                        let border = p.0.min(p.1).min(side - p.0).min(side - p.1);
                        let shapes = map
                            .structures()
                            .values()
                            .map(|placed| {
                                distance_to_shape(
                                    placed,
                                    (
                                        2 * i64::from(x) + i64::from(ox),
                                        2 * i64::from(y) + i64::from(oy),
                                    ),
                                )
                            })
                            .fold(f64::INFINITY, f64::min);
                        best = best.max(border.min(shapes));
                    }
                }
                let truth = (8.0 * best + 1e-9).floor();
                let field = f64::from(pathfinder.clearance_at(x, y));
                assert!(
                    field <= truth,
                    "seed {seed}, cell ({x}, {y}): {field} > {truth}"
                );
                if field < truth {
                    cut_cells += 1;
                }
            }
        }
    }
    // The bound is not vacuous: the contours leave cells whose field underestimates.
    assert!(cut_cells > 0);
    Ok(())
}
