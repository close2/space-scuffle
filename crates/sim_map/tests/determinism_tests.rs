//! Determinism and performance validation tests for sim_map.
//!
//! Validates:
//! 1. Bit-for-bit cross-platform determinism across 1,000 queries using xxHash64.
//! 2. Performance quality gates for 100 simultaneous path queries and 400 visibility updates.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::items_after_statements,
    clippy::doc_markdown
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

#[path = "support/radii.rs"]
mod radii;
use radii::{LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{PathRequest, Pathfinder, PlayerVisionMap, VisionSource};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
use std::time::Instant;
use xxhash_rust::xxh64::Xxh64;

#[test]
fn test_cross_platform_determinism_checksum() {
    let arena = arena::default_arena();
    let mut pathfinder = Pathfinder::new(&arena);
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    let mut rng = ChaCha8Rng::seed_from_u64(0x5ca1_ab1e_cafe_babe);
    let mut hasher = Xxh64::new(0);

    vision.begin_tick_update();

    for i in 0..1000 {
        let sx = rng.gen_range::<i32, _>(2..62);
        let sy = rng.gen_range::<i32, _>(2..62);
        let gx = rng.gen_range::<i32, _>(2..62);
        let gy = rng.gen_range::<i32, _>(2..62);

        let radius = match rng.gen_range::<i32, _>(0..3) {
            0 => SMALL_RADIUS,
            1 => MEDIUM_RADIUS,
            _ => LARGE_RADIUS,
        };

        let request = PathRequest {
            start: SimVec2::new(
                SimScalar::from_num(sx) + SimScalar::HALF,
                SimScalar::from_num(sy) + SimScalar::HALF,
            ),
            goal: SimVec2::new(
                SimScalar::from_num(gx) + SimScalar::HALF,
                SimScalar::from_num(gy) + SimScalar::HALF,
            ),
            radius,
        };

        if let Ok(res) = pathfinder.find_path(&arena, &request) {
            hasher.update(&(res.waypoints.len() as u64).to_le_bytes());
            for wp in &res.waypoints {
                hasher.update(&wp.x.raw().to_le_bytes());
                hasher.update(&wp.y.raw().to_le_bytes());
            }
            hasher.update(&res.total_length.raw().to_le_bytes());
        }

        let vx = rng.gen_range::<i32, _>(2..62);
        let vy = rng.gen_range::<i32, _>(2..62);
        let r = rng.gen_range::<i32, _>(4..14);
        let mover = if rng.gen_bool(0.25) {
            MoverType::Air
        } else {
            MoverType::Ground
        };

        let source = VisionSource {
            pos: SimVec2::new(
                SimScalar::from_num(vx) + SimScalar::HALF,
                SimScalar::from_num(vy) + SimScalar::HALF,
            ),
            mover_type: mover,
            sight_radius: SimScalar::from_num(r),
            detection_radius: if rng.gen_bool(0.1) {
                SimScalar::from_num(r)
            } else {
                SimScalar::ZERO
            },
        };

        vision.apply_vision_source(&arena, &source);

        if (i + 1) % 100 == 0 {
            vision.apply_watchtowers(&arena, PlayerId(1));
            vision.end_tick_update();

            for state in vision.states() {
                let code: u8 = match state {
                    sim_types::VisibilityState::Unexplored => 0,
                    sim_types::VisibilityState::Fogged => 1,
                    sim_types::VisibilityState::Visible => 2,
                };
                hasher.update(&[code]);
            }

            vision.begin_tick_update();
        }
    }

    let final_checksum = hasher.digest();
    eprintln!("sim_map reference checksum: 0x{:016x}", final_checksum);

    // Fixed-seed reference after the switch to the navigation mesh (sc2 pathfinding WP9): the
    // hashed routes are the mesh's, with arcs emitted on their circles (Amendment 2), and goals
    // where the tier's disc does not fit are refused. Before: 0x69cd_c3bf_29fa_dbd8 (the grid).
    // Re-recorded under Amendment 4: the switch commit's 0xa324_e36c_647d_586e was recorded
    // before the last edits of that commit, and its own tree already produced this value.
    // Re-recorded for plan 2026-11-23-vision-and-detection WP3 (B-57; was 0x672c_257d_0fca_cc1b):
    // a ground ray no longer marks the first cell of a sight blocker it enters from outside, so
    // the hashed states of the default arena's blocker cells (x 15 to 17, y 15 to 17) changed.
    const EXPECTED_CHECKSUM: u64 = 0x0875_3f04_b63d_f2ee;
    assert_eq!(
        final_checksum, EXPECTED_CHECKSUM,
        "Checksum diverged from reference"
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "wall-clock timing measures the pathfinder and vision code; it never enters simulation state"
)]
fn test_performance_quality_gate() {
    let arena = arena::default_arena();
    let mut pathfinder = Pathfinder::new(&arena);
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    // Quality Gate 1: 100 path queries in cluttered map
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let start_time = Instant::now();
    let mut successful_queries = 0;

    for _ in 0..100 {
        let sx = rng.gen_range::<i32, _>(2..62);
        let sy = rng.gen_range::<i32, _>(2..62);
        let gx = rng.gen_range::<i32, _>(2..62);
        let gy = rng.gen_range::<i32, _>(2..62);

        let req = PathRequest {
            start: SimVec2::new(
                SimScalar::from_num(sx) + SimScalar::HALF,
                SimScalar::from_num(sy) + SimScalar::HALF,
            ),
            goal: SimVec2::new(
                SimScalar::from_num(gx) + SimScalar::HALF,
                SimScalar::from_num(gy) + SimScalar::HALF,
            ),
            radius: MEDIUM_RADIUS,
        };

        if pathfinder.find_path(&arena, &req).is_ok() {
            successful_queries += 1;
        }
    }
    let path_duration = start_time.elapsed();
    eprintln!(
        "100 path queries took {:?} ({} successful)",
        path_duration, successful_queries
    );
    let max_path_allowed = if cfg!(debug_assertions) { 250 } else { 100 };
    assert!(
        path_duration.as_millis() < max_path_allowed,
        "100 path queries took too long: {:?}",
        path_duration
    );

    // Quality Gate 2: 400 active units visibility update
    let vis_start = Instant::now();
    vision.begin_tick_update();
    for _ in 0..400 {
        let vx = rng.gen_range::<i32, _>(2..62);
        let vy = rng.gen_range::<i32, _>(2..62);
        let source = VisionSource {
            pos: SimVec2::new(
                SimScalar::from_num(vx) + SimScalar::HALF,
                SimScalar::from_num(vy) + SimScalar::HALF,
            ),
            mover_type: MoverType::Ground,
            sight_radius: SimScalar::from_num(10),
            detection_radius: SimScalar::ZERO,
        };
        vision.apply_vision_source(&arena, &source);
    }
    vision.end_tick_update();
    let vis_duration = vis_start.elapsed();
    eprintln!("400-unit vision update took {:?}", vis_duration);
    let max_vis_allowed = if cfg!(debug_assertions) { 250 } else { 100 };
    assert!(
        vis_duration.as_millis() < max_vis_allowed,
        "400-unit vision update took too long: {:?}",
        vis_duration
    );
}

/// Plan `2026-11-18-contour-footprints` WP3: 1,000 seeded routes on the default arena among eight
/// 5.0.14 contour structures, hashed by raw bits; pinned when the contour cut landed and
/// replayed on every target. Every draw is a fixed-width integer range.
///
/// Re-recorded for plan `2026-11-19-funnel-fallbacks` WP3 (D-2, the zero-room retry, with
/// Amendments 1 and 2: runs only where needed, arcs bisected to a thirty-second of a turn; was
/// `0xc0d5_fbfd_292f_37a4`, which the tree reproduces with D-2 switched off, D-3 alone leaving it
/// unchanged). Two of the 1,000 requests change, both class E zero-room pinches at r 0.5 that fell
/// back to the portal midpoints and are now legal: (8.75, 4.0) -> (27.5, 59.0), 95.67 -> 63.41
/// long, and (8.75, 6.25) -> (8.75, 49.5), 115.55 -> 63.63. The new value is the same in native
/// debug and release and in `wasm32-wasip1` release and debug.
///
/// Re-recorded for plan `2026-11-20-tastar-budget` WP2 (D-1, TA*'s channel-geodesic bound; was
/// `0x7ac0_f0b9_f8b4_2057`, which the measurement tree reproduces with D-1 switched off). One of
/// the 1,000 requests changes, shorter: (61.75, 19.5) -> (2.0, ·), r 0.5, 62.090 -> 61.497 (the
/// anytime budget cut its better channel before; D-2 alone finds it too). The new value is the
/// same in native debug and release and in `wasm32-wasip1` release and debug.
#[test]
fn contour_route_checksum() {
    let mut map = arena::default_arena();
    let contours: [((u32, u32), u8, u16, u16); 8] = [
        ((8, 8), 5, 10, 6),
        ((14, 8), 3, 6, 2),
        ((17, 11), 3, 6, 3),
        ((20, 8), 2, 4, 2),
        ((22, 10), 2, 4, 2),
        ((40, 12), 5, 10, 6),
        ((45, 17), 3, 6, 2),
        ((12, 20), 2, 2, 1),
    ];
    for (index, (cell, size, half, chamfer)) in (0_u32..).zip(contours) {
        let footprint = sim_map::StructureFootprint {
            width: size,
            height: size,
            shape: sim_types::FootprintShape::Box {
                half_width_q: half,
                half_height_q: half,
                chamfer_q: chamfer,
            },
        };
        map.place_structure(sim_types::EntityId(500 + index), cell, footprint)
            .expect("invariant: the seeded sites are open ground");
    }
    let mut pathfinder = Pathfinder::new(&map);
    let mut rng = ChaCha8Rng::seed_from_u64(0xc047_0e5e_ed00_0001);
    let mut hasher = Xxh64::new(0);
    let quarter = SimScalar::from_raw(1 << 30);
    for _ in 0..1000 {
        let point = |rng: &mut ChaCha8Rng| {
            SimVec2::new(
                SimScalar::from_num(rng.gen_range::<i32, _>(2..62))
                    + quarter * rng.gen_range::<i32, _>(0..4),
                SimScalar::from_num(rng.gen_range::<i32, _>(2..62))
                    + quarter * rng.gen_range::<i32, _>(0..4),
            )
        };
        let (start, goal) = (point(&mut rng), point(&mut rng));
        let radius = SimScalar::from_raw(rng.gen_range::<i64, _>(5..17) << 28);
        match pathfinder.find_path(
            &map,
            &PathRequest {
                start,
                goal,
                radius,
            },
        ) {
            Ok(res) => {
                hasher.update(&(res.waypoints.len() as u64).to_le_bytes());
                for wp in &res.waypoints {
                    hasher.update(&wp.x.raw().to_le_bytes());
                    hasher.update(&wp.y.raw().to_le_bytes());
                }
                hasher.update(&res.total_length.raw().to_le_bytes());
            }
            Err(_) => hasher.update(&[0]),
        }
    }
    let checksum = hasher.digest();
    eprintln!("contour route checksum: 0x{checksum:016x}");
    const CONTOUR_ROUTE_CHECKSUM: u64 = 0xec9a_71b8_655a_01c2;
    assert_eq!(checksum, CONTOUR_ROUTE_CHECKSUM, "contour routes diverged");
}
