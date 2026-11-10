//! Integration tests of terrain lanes and the lane-restricted route search (plan
//! `docs/plans/2026-11-10-group-routing.md`, WP1 and WP2: tests H-1 to H-6).
//!
//! # Responsibility
//! Checks through the public API that the lane cuts form a forest rooted at the map border on
//! both shipped maps and on synthetic nests (H-2), that lane words tell the two sides of an
//! obstacle apart and are blind to the route's shape within a side (H-1), that splitting a leg
//! never changes a word and that degenerate legs are decided by the plan's perturbation rule
//! (H-3), that a route's word equals the word of its channel's portal-midpoint polyline (H-3b),
//! that the restricted search returns exactly the unrestricted route when asked for that route's
//! own lane (H-4), that it finds the other side of an obstacle and fails cleanly where a lane is
//! too narrow or names no route (H-5), and pins a digest of restricted routes (H-6).
//!
//! # How it fits
//! WP1 and WP2 are latent: nothing in the simulation reads lanes yet, so these tests are their
//! only users until the group rule of WP4.
//!
//! # Invariants
//! - Corpora are pure functions of the seeds below and the shipped map bytes, which the digest
//!   test embeds so that it also runs under `wasm32-wasip1`:
//!   `cargo test -p sim_map --test lane_tests --target wasm32-wasip1 -- digest`.
//! - Points of the split corpus lie on a 1/256-cell grid and split parameters are sixteenths, so
//!   every split point lies exactly on its leg.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::panic,
    reason = "test maps are at most 64 cells a side, so every index fits every integer type; a \
              broken lane law must fail the test loudly"
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sim_map::{
    CellFlags, CliffLevel, GameMap, LaneCut, LaneWord, MapError, MapFile, PathRequest,
    PathRequestWithVision, PathResult, Pathfinder,
};
use sim_types::{SimScalar, SimVec2};
use xxhash_rust::xxh64::Xxh64;

/// Body radii of H-4: 0.375 and 0.5 world units.
const RADII: [SimScalar; 2] = [SimScalar::from_raw(3 << 29), SimScalar::from_raw(1 << 31)];
/// Seed of the route corpora.
const ROUTE_SEED: u64 = 0x1a7e_0000_0000_0001;
/// Seed of the leg-splitting corpus.
const SPLIT_SEED: u64 = 0x1a7e_0000_0000_0002;
/// Seed of the digest corpus.
const DIGEST_SEED: u64 = 0x1a7e_0000_0000_0003;
/// Route requests per map and radius in the H-3b and H-4 corpora.
const ROUTES_PER_MAP: usize = 150;

/// A point in whole and half cells: `(2x, 2y)` halves.
fn half(x2: i32, y2: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_raw(i64::from(x2) << 31),
        SimScalar::from_raw(i64::from(y2) << 31),
    )
}

fn request(start: SimVec2, goal: SimVec2, radius: SimScalar) -> PathRequestWithVision<'static> {
    PathRequest {
        start,
        goal,
        radius,
    }
    .into()
}

fn flat_map(side: u32) -> GameMap {
    GameMap::new(side, side, sim_map::MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a positive side is a valid map size")
}

fn block(map: &mut GameMap, xs: std::ops::Range<u32>, ys: std::ops::Range<u32>) {
    for y in ys {
        for x in xs.clone() {
            map.cell_mut(x, y)
                .expect("invariant: blocked cells lie on the test map")
                .flags = CellFlags::empty();
        }
    }
}

/// A 24x24 map with one walled island, cells `x in 9..15, y in 8..16`, between a start at
/// (12, 3) and a goal at (12, 21). The west passage is 9 cells wide; the east passage is
/// `east_gap` cells wide.
fn island_map(east_gap: u32) -> GameMap {
    let mut map = flat_map(24);
    block(&mut map, 9..15, 8..16);
    block(&mut map, 15 + east_gap..24, 8..16);
    map
}

const ISLAND_START: (i32, i32) = (24, 6);
const ISLAND_GOAL: (i32, i32) = (24, 42);

fn shipped(text: &str) -> GameMap {
    MapFile::from_ron_str(text)
        .and_then(|file| file.decode())
        .and_then(|document| document.to_game_map())
        .expect("invariant: the shipped maps decode")
}

fn canyon() -> GameMap {
    shipped(include_str!(
        "../../../assets/maps/showcase_canyon.scmap.ron"
    ))
}

fn arena_map() -> GameMap {
    shipped(include_str!("../../../assets/maps/default_arena.scmap.ron"))
}

/// A random standable cell centre.
fn standable(map: &GameMap, rng: &mut ChaCha8Rng) -> SimVec2 {
    loop {
        let (x, y) = (
            rng.gen_range(0..map.width()),
            rng.gen_range(0..map.height()),
        );
        if map.terrain_step_is_connected((x, y), (x, y)) {
            return half(2 * x as i32 + 1, 2 * y as i32 + 1);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// H-2: the cut forest.
// ---------------------------------------------------------------------------------------------

/// Checks every cut against the terrain directly, sharing no code with `LaneCuts::build`: the
/// cut's column is free between its ends, a wall lies at its bottom, the obstacle it ends on is
/// strictly lower, and following `below` reaches the border.
fn check_forest(map: &GameMap, cuts: &[LaneCut]) -> Result<(), String> {
    for cut in cuts {
        if cut.bottom >= cut.top {
            return Err(format!("{cut:?}: bottom not below top"));
        }
        let x = cut.x as u32;
        for y in cut.bottom as u32..cut.top as u32 {
            if !map.terrain_step_is_connected((x, y), (x, y)) {
                return Err(format!(
                    "{cut:?}: cell ({x}, {y}) under the cut is unusable"
                ));
            }
            if y + 1 < cut.top as u32 && !map.terrain_step_is_connected((x, y), (x, y + 1)) {
                return Err(format!("{cut:?}: a wall crosses the cut above row {y}"));
            }
        }
        let bottom = cut.bottom as u32;
        if bottom > 0 && map.terrain_step_is_connected((x, bottom - 1), (x, bottom)) {
            return Err(format!("{cut:?}: no wall at the cut's bottom"));
        }
        let mut current = *cut;
        for _ in 0..=cuts.len() {
            let Some(below) = current.below else {
                break;
            };
            let next = cuts
                .iter()
                .find(|c| c.obstacle == below)
                .ok_or_else(|| format!("{current:?}: ends on an obstacle without a cut"))?;
            if next.top > current.bottom {
                return Err(format!("{current:?} ends on {next:?}, which is not lower"));
            }
            current = *next;
        }
        if current.below.is_some() {
            return Err(format!("{cut:?}: the forest does not reach the border"));
        }
    }
    let mut obstacles: Vec<u32> = cuts.iter().map(|c| c.obstacle).collect();
    obstacles.sort_unstable();
    if obstacles != (0..cuts.len() as u32).collect::<Vec<_>>() {
        return Err(format!("obstacle indices are not 0..n: {obstacles:?}"));
    }
    Ok(())
}

/// Records each shipped map's cut count and `K`. The canyon's central plateau is its one
/// obstacle; the default arena's plateau and blockers all touch the border, so it has none and
/// every route on it has the empty word.
#[test]
fn shipped_maps_cut_forests_reach_the_border_and_record_their_size() {
    for (name, map, cut_count, bound) in [
        ("showcase_canyon", canyon(), 1, 1),
        ("default_arena", arena_map(), 0, 0),
    ] {
        let pathfinder = Pathfinder::new(&map);
        let cuts = pathfinder.lane_cuts().cuts();
        check_forest(&map, cuts).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(
            (pathfinder.lane_cut_count(), pathfinder.lane_edge_bound()),
            (cut_count, bound),
            "{name}: cuts {cuts:?}"
        );
    }
}

/// A 32x32 map with nested obstacles: a U open to the north holding an island, above an
/// obstacle under the U's west arm; a closed ring whose pocket holds another island; and a bay
/// on the west border, which is part of the outside.
fn nested_map() -> GameMap {
    let mut map = flat_map(32);
    block(&mut map, 8..24, 8..9); // U: bottom bar
    block(&mut map, 8..9, 8..20); // U: west arm
    block(&mut map, 23..24, 8..20); // U: east arm
    block(&mut map, 14..18, 14..16); // island inside the U
    block(&mut map, 6..12, 3..5); // obstacle under the U's west arm
    block(&mut map, 0..3, 26..28); // bay touching the west border: not an obstacle
    block(&mut map, 25..31, 22..23); // ring: bottom
    block(&mut map, 25..31, 29..30); // ring: top
    block(&mut map, 25..26, 22..30); // ring: west
    block(&mut map, 30..31, 22..30); // ring: east
    block(&mut map, 27..29, 25..27); // island in the ring's pocket, which no route reaches
    map
}

#[test]
fn nested_obstacles_hang_their_cuts_on_the_obstacle_below() {
    let map = nested_map();
    let pathfinder = Pathfinder::new(&map);
    let cuts = pathfinder.lane_cuts().cuts();
    check_forest(&map, cuts).unwrap_or_else(|error| panic!("{error}"));
    let at = |x: i32, top: i32| {
        *cuts
            .iter()
            .find(|c| (c.x, c.top) == (x, top))
            .unwrap_or_else(|| panic!("no cut at ({x}, {top}) in {cuts:?}"))
    };
    let (low, u, island) = (at(6, 3), at(8, 8), at(14, 14));
    let (ring, pocket) = (at(25, 22), at(27, 25));
    assert_eq!(cuts.len(), 5, "{cuts:?}");
    assert_eq!((low.bottom, low.below), (0, None));
    assert_eq!((u.bottom, u.below), (5, Some(low.obstacle)));
    assert_eq!((island.bottom, island.below), (9, Some(u.obstacle)));
    assert_eq!((ring.bottom, ring.below), (0, None));
    assert_eq!((pocket.bottom, pocket.below), (23, Some(ring.obstacle)));
}

#[test]
fn structures_do_not_change_the_cuts() {
    let mut map = island_map(9);
    let before = Pathfinder::new(&map).lane_cuts().clone();
    map.place_structure(sim_types::EntityId(900), 3, 3, 3, 3)
        .expect("invariant: the open corner admits a 3x3 structure");
    let mut pathfinder = Pathfinder::new(&map);
    assert_eq!(pathfinder.lane_cuts(), &before);
    map.remove_structure(sim_types::EntityId(900))
        .expect("invariant: the structure was placed");
    pathfinder.recompute_clearance(&map);
    assert_eq!(pathfinder.lane_cuts(), &before);
}

// ---------------------------------------------------------------------------------------------
// H-1: words tell the sides of an obstacle apart.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_two_sides_of_an_island_have_different_words() {
    let map = island_map(9);
    let pathfinder = Pathfinder::new(&map);
    assert_eq!(pathfinder.lane_cut_count(), 1);
    let (start, goal) = (
        half(ISLAND_START.0, ISLAND_START.1),
        half(ISLAND_GOAL.0, ISLAND_GOAL.1),
    );
    let west = pathfinder.lane_word(&[start, half(8, 24), goal]);
    let west_wide = pathfinder.lane_word(&[start, half(4, 10), half(2, 24), half(10, 38), goal]);
    let east = pathfinder.lane_word(&[start, half(40, 24), goal]);
    let east_wide = pathfinder.lane_word(&[start, half(44, 12), half(40, 40), goal]);
    assert_ne!(west, east);
    assert_eq!(west, west_wide, "two routes on the west side share a word");
    assert_eq!(east, east_wide, "two routes on the east side share a word");
    // Up the west side and back down it: the crossings cancel.
    let there_and_back = pathfinder.lane_word(&[start, half(8, 24), goal, half(8, 24), start]);
    assert_eq!(there_and_back, LaneWord::empty());
    // Once round the island is not the empty word.
    let round = pathfinder.lane_word(&[start, half(8, 24), goal, half(40, 24), start]);
    assert_eq!(round.len(), 1);
}

#[test]
fn the_canyon_routes_west_and_east_of_the_plateau_have_different_words() {
    let map = canyon();
    let document = arena::load_document(arena::SHOWCASE_CANYON_FILE);
    let spawn = document
        .start_locations
        .iter()
        .find(|s| s.player_slot == 1)
        .expect("invariant: the canyon has a player 1 start");
    let (fx, fy) = (u32::from(spawn.footprint.0), u32::from(spawn.footprint.1));
    let start = half(
        (2 * spawn.cell.0 + fx) as i32,
        (2 * spawn.cell.1 + fy) as i32,
    );
    let goal = half(11, 11);
    let mut pathfinder = Pathfinder::new(&map);
    let mut via = |point: SimVec2| {
        let radius = RADII[0];
        let first = pathfinder
            .find_path(
                &map,
                &PathRequest {
                    start,
                    goal: point,
                    radius,
                },
            )
            .expect("invariant: the lane point is reachable from the spawn");
        let second = pathfinder
            .find_path(
                &map,
                &PathRequest {
                    start: point,
                    goal,
                    radius,
                },
            )
            .expect("invariant: the goal is reachable from the lane point");
        let mut points = first.waypoints;
        points.extend_from_slice(&second.waypoints[1..]);
        pathfinder.lane_word(&points)
    };
    let (west, east) = (via(half(51, 64)), via(half(77, 64)));
    assert_ne!(west, east, "west {west:?}, east {east:?}");
}

// ---------------------------------------------------------------------------------------------
// H-3: splitting and degenerate legs.
// ---------------------------------------------------------------------------------------------

/// A random point on the 1/256-cell grid inside a `side`-cell map.
fn grid_point(rng: &mut ChaCha8Rng, side: u32) -> SimVec2 {
    let coordinate =
        |rng: &mut ChaCha8Rng| SimScalar::from_raw(i64::from(rng.gen_range(0..side * 256)) << 24);
    SimVec2::new(coordinate(rng), coordinate(rng))
}

#[test]
fn splitting_a_leg_never_changes_its_word() {
    for (map, has_cuts) in [(canyon(), true), (arena_map(), false), (nested_map(), true)] {
        let pathfinder = Pathfinder::new(&map);
        let mut rng = ChaCha8Rng::seed_from_u64(SPLIT_SEED);
        let mut crossing_legs = 0;
        for _ in 0..2_000 {
            let (p, q) = (
                grid_point(&mut rng, map.width()),
                grid_point(&mut rng, map.width()),
            );
            let whole = pathfinder.lane_word(&[p, q]);
            crossing_legs += usize::from(!whole.is_empty());
            let sixteenths = rng.gen_range(1..16);
            let split = p + (q - p) * SimScalar::from_raw(sixteenths << 28);
            assert_eq!(
                pathfinder.lane_word(&[p, split, q]),
                whole,
                "{p:?} -> {q:?} split at {sixteenths}/16"
            );
        }
        assert_eq!(
            crossing_legs > 20,
            has_cuts,
            "crossing legs: {crossing_legs}"
        );
    }
}

/// The letters of a word that crosses nothing.
const NO_LETTERS: Vec<i32> = Vec::new();

#[test]
fn degenerate_legs_on_a_shipped_cut_follow_the_perturbation_rule() {
    let map = canyon();
    let pathfinder = Pathfinder::new(&map);
    let cut = pathfinder.lane_cuts().cuts()[0];
    let letter = cut.obstacle as i32 + 1;
    let (x2, top2, bottom2) = (2 * cut.x, 2 * cut.top, 2 * cut.bottom);
    let mid2 = top2.midpoint(bottom2);
    let word = |points: &[SimVec2]| pathfinder.lane_word(points).letters().to_vec();
    // Through ζ: level is on the cut (its top is inclusive), rising passes above, falling below.
    assert_eq!(
        word(&[half(x2 - 2, top2), half(x2 + 2, top2)]),
        vec![letter]
    );
    assert_eq!(
        word(&[half(x2 - 2, top2 - 2), half(x2 + 2, top2 + 2)]),
        NO_LETTERS
    );
    assert_eq!(
        word(&[half(x2 - 2, top2 + 2), half(x2 + 2, top2 - 2)]),
        vec![letter]
    );
    // Along x = ζ.x: on the cut's left all the way, so no crossing.
    assert_eq!(
        word(&[half(x2, bottom2 + 1), half(x2, top2 - 1)]),
        NO_LETTERS
    );
    // Ending exactly on the cut's column counts as left: the next leg makes the crossing.
    assert_eq!(word(&[half(x2 - 2, mid2), half(x2, mid2)]), NO_LETTERS);
    assert_eq!(
        word(&[half(x2 - 2, mid2), half(x2, mid2), half(x2 + 2, mid2)]),
        vec![letter]
    );
    assert_eq!(word(&[half(x2 + 2, mid2), half(x2, mid2)]), vec![-letter]);
    // Through the bottom end: rising crosses, level does not (the span is open there).
    assert_eq!(
        word(&[half(x2 - 2, bottom2 - 2), half(x2 + 2, bottom2 + 2)]),
        vec![letter]
    );
    assert_eq!(
        word(&[half(x2 - 2, bottom2), half(x2 + 2, bottom2)]),
        NO_LETTERS
    );
}

// ---------------------------------------------------------------------------------------------
// H-3b and H-4: routes, channels and the restricted search on the shipped maps.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_routes_word_equals_its_channels_midpoint_word() {
    for (map, has_cuts) in [(canyon(), true), (arena_map(), false), (nested_map(), true)] {
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = ChaCha8Rng::seed_from_u64(ROUTE_SEED);
        let mut crossing = 0;
        for radius in RADII {
            for _ in 0..ROUTES_PER_MAP {
                let (start, goal) = (standable(&map, &mut rng), standable(&map, &mut rng));
                let Ok(route) =
                    pathfinder.find_path_with_vision(&map, &request(start, goal, radius))
                else {
                    continue;
                };
                let channel = pathfinder.last_channel_midpoints(start, goal);
                let word = pathfinder.lane_word(&route.waypoints);
                assert_eq!(
                    word,
                    pathfinder.lane_word(&channel),
                    "{start:?} -> {goal:?}"
                );
                crossing += usize::from(!word.is_empty());
            }
        }
        assert_eq!(crossing > 10, has_cuts, "routes crossing cuts: {crossing}");
    }
}

#[test]
fn restricted_to_its_own_lane_the_search_returns_the_unrestricted_route() {
    for map in [canyon(), arena_map(), nested_map()] {
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = ChaCha8Rng::seed_from_u64(ROUTE_SEED);
        let mut compared = 0;
        for radius in RADII {
            for _ in 0..ROUTES_PER_MAP {
                let (start, goal) = (standable(&map, &mut rng), standable(&map, &mut rng));
                let request = request(start, goal, radius);
                let free = pathfinder.find_path_with_vision(&map, &request);
                let lane = free.as_ref().map_or_else(
                    |_| LaneWord::empty(),
                    |r| pathfinder.lane_word(&r.waypoints),
                );
                let restricted = pathfinder.find_path_in_lane(&map, &request, &lane);
                if free.is_ok() {
                    compared += 1;
                    assert_eq!(restricted, free, "{start:?} -> {goal:?} radius {radius}");
                } else if let Err(MapError::InvalidStart(_) | MapError::InvalidGoal(_)) = free {
                    assert_eq!(restricted, free, "endpoint errors are the search's");
                }
            }
        }
        assert!(compared > 200, "{compared}");
    }
}

// ---------------------------------------------------------------------------------------------
// H-5: the other side, and lanes that admit no route.
// ---------------------------------------------------------------------------------------------

fn legal(
    pathfinder: &mut Pathfinder,
    map: &GameMap,
    request: &PathRequestWithVision<'_>,
    route: &PathResult,
) -> bool {
    route
        .waypoints
        .windows(2)
        .all(|pair| pathfinder.segment_is_passable(map, request, pair[0], pair[1]))
}

#[test]
fn the_restricted_search_finds_the_other_side_of_an_island() {
    let map = island_map(9);
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (
        half(ISLAND_START.0, ISLAND_START.1),
        half(ISLAND_GOAL.0, ISLAND_GOAL.1),
    );
    let west = pathfinder.lane_word(&[start, half(8, 24), goal]);
    let east = pathfinder.lane_word(&[start, half(40, 24), goal]);
    for radius in RADII {
        let request = request(start, goal, radius);
        let free = pathfinder
            .find_path_with_vision(&map, &request)
            .expect("invariant: the island map is connected");
        let own = pathfinder.lane_word(&free.waypoints);
        let other = if own == west { &east } else { &west };
        assert!(own == west || own == east, "{own:?}");
        let routed = pathfinder
            .find_path_in_lane(&map, &request, other)
            .expect("invariant: both passages admit the disc");
        assert_eq!(&pathfinder.lane_word(&routed.waypoints), other);
        assert!(legal(&mut pathfinder, &map, &request, &routed));
        assert!(routed.total_length >= free.total_length);
    }
}

#[test]
fn a_unit_too_big_for_the_lane_fails_cleanly() {
    // The east passage is one cell wide: a disc of radius 0.5 fits it only exactly, 0.75 never.
    let map = island_map(1);
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (
        half(ISLAND_START.0, ISLAND_START.1),
        half(ISLAND_GOAL.0, ISLAND_GOAL.1),
    );
    let east = pathfinder.lane_word(&[start, half(31, 24), goal]);
    let big = request(start, goal, SimScalar::from_raw(3 << 30));
    assert_eq!(
        pathfinder.find_path_in_lane(&map, &big, &east),
        Err(MapError::NoPathFound { start, goal })
    );
    // The same unit still has its west route, and a small one has the east lane.
    let free = pathfinder
        .find_path_with_vision(&map, &big)
        .expect("invariant: the west passage is wide");
    assert_ne!(pathfinder.lane_word(&free.waypoints), east);
    let small = request(start, goal, SimScalar::from_raw(1 << 30));
    let narrow = pathfinder
        .find_path_in_lane(&map, &small, &east)
        .expect("invariant: a disc of radius 0.25 fits the one-cell passage");
    assert_eq!(pathfinder.lane_word(&narrow.waypoints), east);
}

#[test]
fn a_word_of_an_unreachable_or_unknown_obstacle_finds_no_route() {
    let map = nested_map();
    let mut pathfinder = Pathfinder::new(&map);
    let (start, goal) = (half(4, 56), half(40, 56));
    let request = request(start, goal, RADII[0]);
    let unknown = LaneWord::from_letters(&[99]);
    assert_eq!(
        pathfinder.find_path_in_lane(&map, &request, &unknown),
        Err(MapError::NoPathFound { start, goal })
    );
    // The island in the ring's pocket: its cut lies inside the closed pocket, so no route from
    // outside the ring crosses it.
    let pocket = pathfinder
        .lane_cuts()
        .cuts()
        .iter()
        .find(|c| (c.x, c.top) == (27, 25))
        .map(|c| c.obstacle as i32 + 1)
        .expect("invariant: the nested map has the pocket island");
    assert_eq!(
        pathfinder.find_path_in_lane(&map, &request, &LaneWord::from_letters(&[pocket])),
        Err(MapError::NoPathFound { start, goal })
    );
}

// ---------------------------------------------------------------------------------------------
// H-6: pinned digest of restricted routes.
// ---------------------------------------------------------------------------------------------

/// Digest of the restricted routes of [`digest_of_restricted_routes_is_pinned`]. It must be
/// identical on every target, including `wasm32-wasip1`.
const RESTRICTED_ROUTES_DIGEST: u64 = 0x971c_de38_d460_d2c2;

fn eat_result(hasher: &mut Xxh64, result: &Result<PathResult, MapError>) {
    match result {
        Ok(route) => {
            hasher.update(&[1]);
            for point in &route.waypoints {
                hasher.update(&point.x.raw().to_le_bytes());
                hasher.update(&point.y.raw().to_le_bytes());
            }
            hasher.update(&route.total_length.raw().to_le_bytes());
        }
        Err(error) => {
            hasher.update(&[0]);
            hasher.update(format!("{error:?}").as_bytes());
        }
    }
}

#[test]
fn digest_of_restricted_routes_is_pinned() {
    let mut hasher = Xxh64::new(0);
    let mut found = 0;
    for map in [canyon(), arena_map()] {
        let mut pathfinder = Pathfinder::new(&map);
        let mut rng = ChaCha8Rng::seed_from_u64(DIGEST_SEED);
        // The lanes of every cut's two sides: pass the cut's column just left or just right.
        let mut lanes = vec![LaneWord::empty()];
        for cut in pathfinder.lane_cuts().cuts() {
            let letter = cut.obstacle as i32 + 1;
            lanes.extend([
                LaneWord::from_letters(&[letter]),
                LaneWord::from_letters(&[-letter]),
            ]);
        }
        for radius in RADII {
            for _ in 0..60 {
                let (start, goal) = (standable(&map, &mut rng), standable(&map, &mut rng));
                let request = request(start, goal, radius);
                for lane in &lanes {
                    let result = pathfinder.find_path_in_lane(&map, &request, lane);
                    if let Ok(route) = &result {
                        found += 1;
                        assert_eq!(&pathfinder.lane_word(&route.waypoints), lane);
                    }
                    eat_result(&mut hasher, &result);
                }
            }
        }
    }
    assert!(found > 100, "{found}");
    assert_eq!(
        hasher.digest(),
        RESTRICTED_ROUTES_DIGEST,
        "digest {:#x}",
        hasher.digest()
    );
}
