//! # Integration Test: the wall contact query (plan `2026-11-11-contact-and-missiles` WP2)
//!
//! `Pathfinder::wall_contact` returns the nearest point of the mesh edges that block a request
//! within the request's radius of a point, with its exact squared distance and the edge's
//! open-side normal (plan D-5). Movement builds the non-deepening contact rule of bug B-113 on
//! it. These tests pin the geometry on small synthetic maps (a straight wall, a convex and a
//! concave corner, a structure, a ramp, the map border, a centre on the wall, no wall in reach)
//! and check the flood against the exhaustive reference over every blocking edge on both
//! shipped maps.

#![allow(clippy::expect_used)]

use rand::{Rng, SeedableRng};
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, MapFile, PathRequestWithVision, Pathfinder, RampDir,
    WallContact,
};
use sim_types::{EntityId, SimScalar, SimVec2};

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

/// The physical request of a body at `p`: true terrain, every structure, no vision.
fn request(p: SimVec2, radius: SimScalar) -> PathRequestWithVision<'static> {
    PathRequestWithVision {
        start: p,
        goal: p,
        radius,
        vision: None,
        block_unexplored_ramps: false,
        known_structures: None,
    }
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

/// The contact of a disc of `radius` at `p`, checked against the exhaustive reference.
fn contact(map: &GameMap, p: SimVec2, radius: f64) -> Option<WallContact> {
    let mut pathfinder = Pathfinder::new(map);
    let request = request(p, s(radius));
    let found = pathfinder.wall_contact(map, &request, p);
    let reference = pathfinder.wall_contact_exhaustive(map, &request, p);
    assert_eq!(
        found.map(|c| c.distance_sq),
        reference.map(|c| c.distance_sq),
        "flood and reference at {p:?}"
    );
    found
}

/// The squared distance between two world points in raw Q32.32 units squared.
fn raw_distance_sq(a: SimVec2, b: SimVec2) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

#[test]
fn a_straight_wall_is_touched_square_to_it() {
    let map = walled(20, |x, _| x >= 10);
    let p = at(9.6, 5.5);
    let found = contact(&map, p, 0.5).expect("0.4 from the wall");
    assert_eq!(found.point, at(10.0, 5.5));
    assert_eq!(found.distance_sq, raw_distance_sq(p, at(10.0, 5.5)));
    assert_eq!(found.normal, at(-1.0, 0.0), "the open side is west");
    assert!(
        contact(&map, p, 0.375).is_none(),
        "0.4 is beyond a radius of 0.375"
    );
}

#[test]
fn at_a_convex_corner_the_nearest_point_is_the_vertex() {
    let map = walled(24, |x, y| (10..14).contains(&x) && (10..14).contains(&y));
    let p = at(9.7, 9.6);
    let found = contact(&map, p, 0.6).expect("0.5 from the corner");
    assert_eq!(found.point, at(10.0, 10.0));
    assert_eq!(found.distance_sq, raw_distance_sq(p, at(10.0, 10.0)));
    assert!(contact(&map, p, 0.45).is_none());
}

#[test]
fn at_a_concave_corner_the_nearer_of_two_walls_wins() {
    let map = walled(20, |x, y| x >= 10 || y >= 10);
    let p = at(9.8, 9.5);
    let found = contact(&map, p, 0.75).expect("0.2 from the east wall, 0.5 from the north");
    assert_eq!(found.point, at(10.0, 9.5));
    assert_eq!(found.normal, at(-1.0, 0.0));
    let p = at(9.5, 9.7);
    let found = contact(&map, p, 0.75).expect("0.3 from the north wall");
    assert_eq!(found.point, at(9.5, 10.0));
    assert_eq!(found.normal, at(0.0, -1.0));
}

#[test]
fn a_structures_edges_are_walls() {
    let mut map = walled(20, |_, _| false);
    map.place_structure(EntityId(1), 10, 10, 2, 2)
        .expect("a free 2 × 2 site");
    let mut pathfinder = Pathfinder::new(&map);
    pathfinder.recompute_clearance(&map);
    let p = at(9.7, 11.0);
    let request = request(p, s(0.5));
    let found = pathfinder
        .wall_contact(&map, &request, p)
        .expect("0.3 from the structure");
    assert_eq!(found.point, at(10.0, 11.0));
    assert_eq!(
        Some(found.distance_sq),
        pathfinder
            .wall_contact_exhaustive(&map, &request, p)
            .map(|c| c.distance_sq)
    );
}

#[test]
fn ramp_connected_level_edges_are_not_walls() {
    // Low ground below y = 14, high ground above; a cliff at y = 14 but for a ramp at x 12..15.
    let mut map = walled(30, |x, y| y == 14 && !(12..15).contains(&x));
    for y in 15..30 {
        for x in 0..30 {
            map.cell_mut(x, y).expect("inside").cliff_level = CliffLevel(2);
        }
    }
    for x in 12..15 {
        let cell = map.cell_mut(x, 14).expect("inside");
        cell.ramp = Some(RampDir::North);
        cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
    }
    assert!(
        contact(&map, at(13.5, 13.8), 0.5).is_none(),
        "the ramp's foot"
    );
    assert!(contact(&map, at(13.5, 14.5), 0.5).is_none(), "the ramp");
    assert!(
        contact(&map, at(13.5, 15.2), 0.5).is_none(),
        "the ramp's top"
    );
    let found = contact(&map, at(11.5, 13.7), 0.5).expect("below the cliff beside the ramp");
    assert_eq!(found.point, at(11.5, 14.0));
}

#[test]
fn the_map_border_is_a_wall() {
    let map = walled(16, |_, _| false);
    let found = contact(&map, at(0.3, 5.5), 0.5).expect("0.3 from the west border");
    assert_eq!(found.point, at(0.0, 5.5));
    assert_eq!(found.normal, at(1.0, 0.0));
}

#[test]
fn a_centre_on_the_wall_gets_the_open_side_normal() {
    let map = walled(16, |_, _| false);
    let found = contact(&map, at(0.0, 5.5), 0.5).expect("on the west border");
    assert_eq!(found.distance_sq, 0);
    assert_eq!(found.point, at(0.0, 5.5));
    assert_eq!(found.normal, at(1.0, 0.0), "into the map");
    let found = contact(&map, at(7.5, 0.0), 0.5).expect("on the south border");
    assert_eq!(found.normal, at(0.0, 1.0));
}

#[test]
fn no_wall_within_the_radius_gives_none() {
    let map = walled(20, |x, _| x >= 10);
    assert!(contact(&map, at(5.5, 5.5), 1.5).is_none());
    assert!(
        contact(&map, at(12.5, 5.5), 0.5).is_none(),
        "inside the cliff"
    );
}

/// Loads a shipped map.
fn shipped(name: &str) -> GameMap {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/maps")
        .join(name);
    MapFile::load(&path)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("a shipped map")
}

/// On both shipped maps, at 2,000 seeded points each with radii from 0.25 to 1.5, the flood's
/// distance equals the exhaustive reference's, and its point lies at that distance.
#[test]
fn the_flood_equals_the_exhaustive_reference_on_both_shipped_maps() {
    for name in ["default_arena.scmap.ron", "showcase_canyon.scmap.ron"] {
        let map = shipped(name);
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0x00C0_47AC);
        let (width, height) = (f64::from(map.width()), f64::from(map.height()));
        let mut touching = 0;
        for _ in 0..2_000 {
            let p = at(rng.gen_range(0.0..width), rng.gen_range(0.0..height));
            let radius = s(rng.gen_range(0.25..1.5));
            let request = request(p, radius);
            let found = pathfinder.wall_contact(&map, &request, p);
            let reference = pathfinder.wall_contact_exhaustive(&map, &request, p);
            assert_eq!(
                found.map(|c| c.distance_sq),
                reference.map(|c| c.distance_sq),
                "{name} at {p:?}, radius {radius:?}"
            );
            if let Some(found) = found {
                touching += 1;
                let radius_raw = i128::from(radius.raw());
                assert!(found.distance_sq <= radius_raw * radius_raw);
                let error = (raw_distance_sq(p, found.point) - found.distance_sq).abs();
                // The rounded point lies within a few raw units of the exact one.
                assert!(
                    error <= 4 * i128::from(radius.raw()) + 4,
                    "{name}: the point lies at the distance"
                );
            }
        }
        assert!(touching >= 100, "{name}: {touching} points touch a wall");
    }
}
