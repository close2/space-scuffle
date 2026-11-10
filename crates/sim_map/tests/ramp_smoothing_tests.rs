//! Legality of routes on shipped maps after ramp-aware corridor smoothing.
//!
//! # Responsibility
//! Smoothing is what remains when funnel output fails validation, which happens around ramps.
//! This test walks long routes across the real maps, in both directions and at every unit size,
//! and rejects any waypoint pair the movement layer would refuse.
//!
//! # How it fits
//! Complements `ramp_traversal_tests`, which pins individual ramp entrances. The fixed-seed
//! reference in `determinism_tests` separately pins that smoothing output does not drift.
//!
//! # Invariants
//! Routes that do not exist are skipped: connectivity is covered by map validation.

#[path = "support/radii.rs"]
mod radii;
use radii::{LARGE_RADIUS, MEDIUM_RADIUS, SMALL_RADIUS};

use sim_map::{MapFile, PathRequest, Pathfinder};
use sim_types::{SimScalar, SimVec2};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn centre(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

#[test]
fn f9_real_map_routes_stay_legal_across_ramps() -> TestResult {
    let mut checked = 0;
    for filename in ["default_arena.scmap.ron", "showcase_canyon.scmap.ron"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps")
            .join(filename);
        let map = MapFile::load(&path)?.decode()?.to_game_map()?;
        let mut pathfinder = Pathfinder::new(&map);
        let endpoints = [(4, 4), (58, 4), (4, 58), (58, 58), (32, 10), (32, 50)];
        for &start in &endpoints {
            for &goal in &endpoints {
                for radius in [SMALL_RADIUS, MEDIUM_RADIUS, LARGE_RADIUS] {
                    let request = PathRequest {
                        start: centre(start.0, start.1),
                        goal: centre(goal.0, goal.1),
                        radius,
                    };
                    let Ok(route) = pathfinder.find_path(&map, &request) else {
                        continue;
                    };
                    checked += 1;
                    for pair in route.waypoints.windows(2) {
                        assert!(
                            pathfinder.segment_is_passable(&map, &request.into(), pair[0], pair[1]),
                            "{filename}: illegal segment {pair:?} for {radius:?}"
                        );
                    }
                }
            }
        }
    }
    assert!(checked > 100, "only {checked} routes existed");
    Ok(())
}
