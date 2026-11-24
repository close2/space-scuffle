//! Integration tests for multi-cell ramp elevation profile and altitude interpolation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/arena.rs"]
mod arena;

use sim_map::{CliffLevel, CliffSpan, GameMap, MapCell, RampDir, RampProfile, CLIFF_LEVEL_HEIGHT};
use sim_types::{SimScalar, SimVec2};

#[test]
fn multi_cell_north_ramp_is_monotonic() {
    let map = arena::default_arena();
    let x = SimScalar::from_num(31.5);

    let mut prev_alt = SimScalar::ZERO;
    let mut y = SimScalar::from_num(29.5);
    let step = SimScalar::from_num(0.25);
    let end = SimScalar::from_num(34.5);

    let mut first = true;
    while y <= end {
        let alt = map.terrain_altitude_scalar(SimVec2::new(x, y));
        if !first {
            assert!(
                alt >= prev_alt,
                "altitude must be non-decreasing along ascending north ramp: y = {:?}, alt = {:?}, prev = {:?}",
                y, alt, prev_alt
            );
        }
        first = false;
        prev_alt = alt;
        y += step;
    }

    // Exact SimScalar assertions at key positions
    let alt_30 = map.terrain_altitude_scalar(SimVec2::new(x, SimScalar::from_num(30.0)));
    assert_eq!(alt_30, SimScalar::from_num(3.0));

    let alt_32 = map.terrain_altitude_scalar(SimVec2::new(x, SimScalar::from_num(32.0)));
    assert_eq!(alt_32, SimScalar::from_num(4.5));

    let alt_34 = map.terrain_altitude_scalar(SimVec2::new(x, SimScalar::from_num(34.0)));
    assert_eq!(alt_34, SimScalar::from_num(6.0));
}

#[test]
fn ramp_profile_indices() {
    let map = arena::default_arena();
    for (expected_idx, y) in (30..=33).enumerate() {
        let profile = map.ramp_profile(31, y).expect("cell (31, y) is a ramp");
        assert_eq!(
            profile,
            RampProfile {
                dir: RampDir::North,
                low_level: CliffLevel(1),
                index: expected_idx as u8,
                run_len: 4,
            }
        );
    }
}

#[test]
fn cardinal_north_ramp_run_progress() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // 3-long north ramp at x=5, y in 5..=7 ascending North into y=8 (level 2)
    for y in 5..=7 {
        let cell = &mut map.cells_mut()[(y * 16 + 5) as usize];
        cell.ramp = Some(RampDir::North);
        cell.cliff_level = CliffLevel(1);
    }
    let cell_8 = &mut map.cells_mut()[(8 * 16 + 5) as usize];
    cell_8.cliff_level = CliffLevel(2);

    let profile_start = map.ramp_profile(5, 5).expect("ramp profile");
    assert_eq!(profile_start.index, 0);
    assert_eq!(profile_start.run_len, 3);
    assert_eq!(
        profile_start.progress(SimScalar::HALF, SimScalar::ZERO),
        SimScalar::ZERO
    );

    let profile_end = map.ramp_profile(5, 7).expect("ramp profile");
    assert_eq!(profile_end.index, 2);
    assert_eq!(profile_end.run_len, 3);
    assert_eq!(
        profile_end.progress(SimScalar::HALF, SimScalar::ONE),
        SimScalar::ONE
    );
}

#[test]
fn cardinal_south_ramp_run_progress() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // 3-long south ramp at x=5, y in 5..=7 ascending South into y=4 (level 2)
    for y in 5..=7 {
        let cell = &mut map.cells_mut()[(y * 16 + 5) as usize];
        cell.ramp = Some(RampDir::South);
        cell.cliff_level = CliffLevel(1);
    }
    let cell_4 = &mut map.cells_mut()[(4 * 16 + 5) as usize];
    cell_4.cliff_level = CliffLevel(2);

    let profile_start = map.ramp_profile(5, 7).expect("ramp profile");
    assert_eq!(profile_start.index, 0);
    assert_eq!(profile_start.run_len, 3);
    assert_eq!(
        profile_start.progress(SimScalar::HALF, SimScalar::ONE),
        SimScalar::ZERO
    );

    let profile_end = map.ramp_profile(5, 5).expect("ramp profile");
    assert_eq!(profile_end.index, 2);
    assert_eq!(profile_end.run_len, 3);
    assert_eq!(
        profile_end.progress(SimScalar::HALF, SimScalar::ZERO),
        SimScalar::ONE
    );
}

#[test]
fn cardinal_east_ramp_run_progress() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // 3-long east ramp at y=5, x in 5..=7 ascending East into x=8 (level 2)
    for x in 5..=7 {
        let cell = &mut map.cells_mut()[(5 * 16 + x) as usize];
        cell.ramp = Some(RampDir::East);
        cell.cliff_level = CliffLevel(1);
    }
    let cell_8 = &mut map.cells_mut()[(5 * 16 + 8) as usize];
    cell_8.cliff_level = CliffLevel(2);

    let profile_start = map.ramp_profile(5, 5).expect("ramp profile");
    assert_eq!(profile_start.index, 0);
    assert_eq!(profile_start.run_len, 3);
    assert_eq!(
        profile_start.progress(SimScalar::ZERO, SimScalar::HALF),
        SimScalar::ZERO
    );

    let profile_end = map.ramp_profile(7, 5).expect("ramp profile");
    assert_eq!(profile_end.index, 2);
    assert_eq!(profile_end.run_len, 3);
    assert_eq!(
        profile_end.progress(SimScalar::ONE, SimScalar::HALF),
        SimScalar::ONE
    );
}

#[test]
fn cardinal_west_ramp_run_progress() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    // 3-long west ramp at y=5, x in 5..=7 ascending West into x=4 (level 2)
    for x in 5..=7 {
        let cell = &mut map.cells_mut()[(5 * 16 + x) as usize];
        cell.ramp = Some(RampDir::West);
        cell.cliff_level = CliffLevel(1);
    }
    let cell_4 = &mut map.cells_mut()[(5 * 16 + 4) as usize];
    cell_4.cliff_level = CliffLevel(2);

    let profile_start = map.ramp_profile(7, 5).expect("ramp profile");
    assert_eq!(profile_start.index, 0);
    assert_eq!(profile_start.run_len, 3);
    assert_eq!(
        profile_start.progress(SimScalar::ONE, SimScalar::HALF),
        SimScalar::ZERO
    );

    let profile_end = map.ramp_profile(5, 5).expect("ramp profile");
    assert_eq!(profile_end.index, 2);
    assert_eq!(profile_end.run_len, 3);
    assert_eq!(
        profile_end.progress(SimScalar::ZERO, SimScalar::HALF),
        SimScalar::ONE
    );
}

#[test]
fn diagonal_ramp_keeps_legacy_single_cell_behaviour() {
    let mut map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(1))).unwrap();
    {
        let cell = &mut map.cells_mut()[(5 * 16 + 5) as usize];
        cell.ramp = Some(RampDir::NorthEast);
        cell.cliff_level = CliffLevel(1);
    }

    let profile = map.ramp_profile(5, 5).expect("ramp profile");
    assert_eq!(
        profile,
        RampProfile {
            dir: RampDir::NorthEast,
            low_level: CliffLevel(1),
            index: 0,
            run_len: 1,
        }
    );

    let fx = SimScalar::from_num(0.5);
    let fy = SimScalar::from_num(0.5);
    let expected_prog = (fx + fy) / SimScalar::from_num(2);
    assert_eq!(profile.progress(fx, fy), expected_prog);
    assert_eq!(
        profile.height_at(fx, fy),
        CliffLevel(1).height_scalar() + expected_prog * SimScalar::from_num(CLIFF_LEVEL_HEIGHT)
    );
}

/// Plan `2026-11-24-owner-answers-cliff-and-vision` P-1 (owner answer Q0099 1(c)): a position
/// on a ramp spans both of the ramp's landings, everywhere on every cell of the run, for each
/// cardinal direction and runs of one and three cells; the cell past the run's top spans the
/// upper level only, the cell before its foot the lower one only.
#[test]
fn cliff_span_at_covers_both_landings_on_every_ramp_cell() {
    let low = CliffLevel(1);
    let high = CliffLevel(2);
    // Sub-cell positions: the centre, and points near the low and high edges and the sides.
    let inside = [
        (0.5, 0.5),
        (0.001, 0.5),
        (0.999, 0.5),
        (0.5, 0.001),
        (0.5, 0.999),
    ];
    for dir in [RampDir::North, RampDir::South, RampDir::East, RampDir::West] {
        let (dx, dy): (i32, i32) = match dir {
            RampDir::North => (0, 1),
            RampDir::South => (0, -1),
            RampDir::East => (1, 0),
            _ => (-1, 0),
        };
        for run in [1_i32, 3] {
            let mut map = GameMap::new(16, 16, MapCell::default_ground(low)).unwrap();
            let foot = (7_i32, 7_i32);
            let cell_of = |step: i32| (foot.0 + dx * step, foot.1 + dy * step);
            // The upper landing: every cell at or past the run's top along the ascent.
            for y in 0..16_i32 {
                for x in 0..16_i32 {
                    let along = (x - foot.0) * dx + (y - foot.1) * dy;
                    if along >= run {
                        map.cell_mut(x as u32, y as u32).unwrap().cliff_level = high;
                    }
                }
            }
            for step in 0..run {
                let (x, y) = cell_of(step);
                let cell = map.cell_mut(x as u32, y as u32).unwrap();
                cell.ramp = Some(dir);
                cell.cliff_level = low;
            }
            for step in 0..run {
                let (x, y) = cell_of(step);
                assert!(
                    map.ramp_profile(x as u32, y as u32).is_some(),
                    "a ramp cell"
                );
                for (fx, fy) in inside {
                    let pos = SimVec2::new(
                        SimScalar::from_num(f64::from(x) + fx),
                        SimScalar::from_num(f64::from(y) + fy),
                    );
                    assert_eq!(
                        map.cliff_span_at(pos),
                        CliffSpan { low, high },
                        "{dir:?} run {run} step {step} at ({fx}, {fy})"
                    );
                }
            }
            let centre = |(x, y): (i32, i32)| {
                SimVec2::new(
                    SimScalar::from_num(f64::from(x) + 0.5),
                    SimScalar::from_num(f64::from(y) + 0.5),
                )
            };
            assert_eq!(
                map.cliff_span_at(centre(cell_of(run))),
                CliffSpan::single(high),
                "{dir:?} top"
            );
            assert_eq!(
                map.cliff_span_at(centre(cell_of(-1))),
                CliffSpan::single(low),
                "{dir:?} foot"
            );
        }
    }
}
