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

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use sim_map::{GameMap, PathRequest, Pathfinder, PlayerVisionMap, UnitClearance, VisionSource};
use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
use std::time::Instant;
use xxhash_rust::xxh64::Xxh64;

#[test]
fn test_cross_platform_determinism_checksum() {
    let arena = GameMap::default_arena();
    let mut pathfinder = Pathfinder::new(&arena);
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    let mut rng = ChaCha8Rng::seed_from_u64(0x5ca1_ab1e_cafe_babe);
    let mut hasher = Xxh64::new(0);

    vision.begin_tick_update();

    for i in 0..1000 {
        let sx = rng.gen_range(2..62);
        let sy = rng.gen_range(2..62);
        let gx = rng.gen_range(2..62);
        let gy = rng.gen_range(2..62);

        let clearance = match rng.gen_range(0..3) {
            0 => UnitClearance::Small,
            1 => UnitClearance::Medium,
            _ => UnitClearance::Large,
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
            clearance,
        };

        if let Ok(res) = pathfinder.find_path(&arena, &request) {
            hasher.update(&(res.waypoints.len() as u64).to_le_bytes());
            for wp in &res.waypoints {
                hasher.update(&wp.x.raw().to_le_bytes());
                hasher.update(&wp.y.raw().to_le_bytes());
            }
            hasher.update(&res.total_length.raw().to_le_bytes());
        }

        let vx = rng.gen_range(2..62);
        let vy = rng.gen_range(2..62);
        let r = rng.gen_range(4..14);
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
            is_detector: rng.gen_bool(0.1),
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

    // Baseline reference digest computed with fixed seed
    const EXPECTED_CHECKSUM: u64 = 0xc62a_9a2f_f0f2_e14f;
    assert_eq!(
        final_checksum, EXPECTED_CHECKSUM,
        "Checksum diverged from reference"
    );
}

#[test]
fn test_performance_quality_gate() {
    let arena = GameMap::default_arena();
    let mut pathfinder = Pathfinder::new(&arena);
    let mut vision = PlayerVisionMap::new(arena.width(), arena.height());

    // Quality Gate 1: 100 path queries in cluttered map
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let start_time = Instant::now();
    let mut successful_queries = 0;

    for _ in 0..100 {
        let sx = rng.gen_range(2..62);
        let sy = rng.gen_range(2..62);
        let gx = rng.gen_range(2..62);
        let gy = rng.gen_range(2..62);

        let req = PathRequest {
            start: SimVec2::new(
                SimScalar::from_num(sx) + SimScalar::HALF,
                SimScalar::from_num(sy) + SimScalar::HALF,
            ),
            goal: SimVec2::new(
                SimScalar::from_num(gx) + SimScalar::HALF,
                SimScalar::from_num(gy) + SimScalar::HALF,
            ),
            clearance: UnitClearance::Medium,
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
        let vx = rng.gen_range(2..62);
        let vy = rng.gen_range(2..62);
        let source = VisionSource {
            pos: SimVec2::new(
                SimScalar::from_num(vx) + SimScalar::HALF,
                SimScalar::from_num(vy) + SimScalar::HALF,
            ),
            mover_type: MoverType::Ground,
            sight_radius: SimScalar::from_num(10),
            is_detector: false,
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
