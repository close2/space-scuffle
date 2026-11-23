//! Behaviour of the movement harness of plan `2026-11-20-contact-solver` (WP1).
//!
//! # Responsibility
//! Pins what the harness's verdicts rest on: the generator is a pure function of `(family, seed)`
//! on the 1/64 lattice and a relabelled scene differs only in its spawn order; the metrics count
//! what §6.3 defines on synthetic tracks a test writes into a world by hand (M1's amplitude and
//! cosine floors, M2's and M3's waste); the gap family's potential falls along the way through
//! the gap; the sign test and the Wilson interval give known values; and five seeds of every
//! family play without a simulation diagnostic and give the same CSV twice.
//!
//! # How it fits
//! The harness only reads the simulation, so these tests drive its meter with positions set
//! directly on a world's entities, and its runner with real scenes.
//!
//! # Invariants
//! - Expected values are worked out by hand from the definitions of §6.3, not copied from a run.

use sim_fixtures::harness::corpus::{generate, Answer, Role, RADII, SPEEDS};
use sim_fixtures::harness::metrics::{reverses, ScenarioMeter};
use sim_fixtures::harness::report::{aggregate, parse_csv, sign_test, verdicts, wilson, Run};
use sim_fixtures::harness::{build_world, run_range, Family, RunOptions};
use sim_types::{SimScalar, SimVec2, Tick, TickBundle};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

const SCENES: [Family; 5] = [
    Family::Gap,
    Family::Arc,
    Family::Cross,
    Family::Corridor,
    Family::Corner,
];

#[test]
fn the_generator_is_a_pure_function_of_family_and_seed_on_the_lattice() {
    for family in SCENES {
        for seed in 1..=40 {
            let scene = generate(family, seed, false);
            assert_eq!(scene, generate(family, seed, false), "{family:?} {seed}");
            for arch in &scene.archetypes {
                assert!(
                    RADII.contains(&arch.radius) || arch.speed == "0.0" || family == Family::Gap,
                    "{family:?} {seed}: radius {}",
                    arch.radius
                );
                assert!(SPEEDS.contains(&arch.speed) || arch.speed == "0.0");
            }
            for spawn in &scene.spawns {
                // A lattice coordinate converts to Q32.32 exactly: shifting back loses nothing.
                for value in [spawn.at.0, spawn.at.1] {
                    assert_eq!((value << 26) >> 26, value);
                    assert!(value > 0, "{family:?} {seed}: {spawn:?}");
                }
            }
            assert!(
                scene.ticks > 0 && !scene.spawns.is_empty(),
                "{family:?} {seed}"
            );
        }
        assert_ne!(generate(family, 1, false), generate(family, 2, false));
    }
}

#[test]
fn a_relabelled_scene_differs_only_in_spawn_order() {
    for family in SCENES {
        let mut permuted = 0;
        for seed in 1..=20 {
            let original = generate(family, seed, false);
            let relabelled = generate(family, seed, true);
            assert!(relabelled.relabel);
            let mut a = original.spawns.clone();
            let mut b = relabelled.spawns.clone();
            permuted += usize::from(a != b);
            a.sort_by_key(|s| (s.at, s.owner, s.archetype));
            b.sort_by_key(|s| (s.at, s.owner, s.archetype));
            assert_eq!(a, b, "{family:?} {seed}");
            assert_eq!(original.params, relabelled.params);
            assert_eq!(original.map, relabelled.map);
            assert_eq!(original.archetypes, relabelled.archetypes);
            assert_eq!(
                (original.ticks, original.expected),
                (relabelled.ticks, relabelled.expected)
            );
        }
        assert!(permuted >= 18, "{family:?}: only {permuted} of 20 permuted");
    }
}

#[test]
fn m1_counts_a_reversal_only_above_a_quarter_speed_and_below_cosine_minus_one_half() {
    let v = 0.2;
    // Straight back, both steps a quarter of the speed: counts.
    assert!(reverses((0.05, 0.0), (-0.05, 0.0), v / 4.0, -0.5));
    // Both steps just short of a quarter speed: does not.
    assert!(!reverses((0.049, 0.0), (-0.049, 0.0), v / 4.0, -0.5));
    // One long, one short: does not.
    assert!(!reverses((0.2, 0.0), (-0.04, 0.0), v / 4.0, -0.5));
    // Cosine −0.6 counts; cosine −0.4 does not.
    let angle = (-0.6_f64).acos();
    assert!(reverses(
        (0.1, 0.0),
        (0.1 * angle.cos(), 0.1 * angle.sin()),
        v / 4.0,
        -0.5
    ));
    let angle = (-0.4_f64).acos();
    assert!(!reverses(
        (0.1, 0.0),
        (0.1 * angle.cos(), 0.1 * angle.sin()),
        v / 4.0,
        -0.5
    ));
}

/// A cross scene's world, with the meter on it, and the first walker's id; nobody has an order.
fn still_scene() -> Result<(sim_core::SimWorld, ScenarioMeter, sim_types::EntityId), Box<dyn Error>>
{
    let scene = generate(Family::Cross, 3, false);
    let (world, ids) = build_world(&scene, &RunOptions::default())?;
    let meter = ScenarioMeter::new(&world, &scene, &ids);
    Ok((world, meter, ids[0]))
}

fn place(world: &mut sim_core::SimWorld, id: sim_types::EntityId, x: f64, y: f64) -> TestResult {
    let entity = world.entities.get_mut(&id).ok_or("unit")?;
    entity.position = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
    world.current_tick += 1;
    Ok(())
}

#[test]
fn the_meter_counts_reversals_and_waste_on_a_synthetic_track() -> TestResult {
    let (mut world, mut meter, id) = still_scene()?;
    let start = world.entities[&id].position;
    let (x0, y0) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    // One second forth and back along x by 0.1 a tick (the scene's speed is 0.1): 12 steps out,
    // 13 back. Every turn is one reversal of steps of a full speed.
    let mut x = x0;
    for tick in 0..25 {
        x += if tick < 12 { 0.1 } else { -0.1 };
        place(&mut world, id, x, y0)?;
        meter.observe(&world);
    }
    let row = meter.finish();
    assert_eq!(row.rev_amp, 1, "{row:?}");
    assert_eq!(row.rev_amp_hi, 1);
    assert_eq!(row.moving_ticks, 25);
    // E4 (plan Amendment 7): the unit has no order, so its reversal and its moving ticks are all
    // idle ones.
    assert_eq!(row.rev_amp_idle, 1);
    assert_eq!(row.moving_ticks_idle, 25);
    // M2: 25 steps of 0.1 walked, net displacement 0.1: 2.4 wasted of 2.5.
    assert!((row.path - 2.5).abs() < 1e-6, "{}", row.path);
    assert!((row.wasted - 2.4).abs() < 1e-6, "{}", row.wasted);
    Ok(())
}

#[test]
fn standing_waste_counts_only_the_last_250_ticks() -> TestResult {
    let scene = generate(Family::Cross, 3, false);
    let (mut world, ids) = build_world(&scene, &RunOptions::default())?;
    let mut meter = ScenarioMeter::new(&world, &scene, &ids);
    let id = ids[0];
    let start = world.entities[&id].position;
    let (x0, y0) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    let ticks = scene.ticks;
    for tick in 0..ticks {
        // Jitters by 0.01 back and forth every tick: 0.01 of path a tick, no net displacement over
        // an even number of ticks.
        let x = if tick % 2 == 0 { x0 + 0.01 } else { x0 };
        place(&mut world, id, x, y0)?;
        meter.observe(&world);
    }
    let row = meter.finish();
    // The tail starts after tick `ticks − 250`; 250 jitter steps of 0.01, net 0 or 0.01.
    assert!(
        row.standing_wasted > 2.48 && row.standing_wasted <= 2.5 + 1e-6,
        "{}",
        row.standing_wasted
    );
    Ok(())
}

#[test]
fn the_gap_potential_falls_along_the_straight_path_through_the_gap() -> TestResult {
    let scene = generate(Family::Gap, 4, false);
    let Answer::Wall { opening, .. } = scene.answer else {
        return Err("a gap scene has a wall".into());
    };
    let Role::GroupMove { goal } = scene.spawns[0].role else {
        return Err("gap walkers share a move".into());
    };
    let (world, ids) = build_world(&scene, &RunOptions::default())?;
    let meter = ScenarioMeter::new(&world, &scene, &ids);
    let start = scene.spawns[0].at;
    let lattice = |v: i64| f64::from(i32::try_from(v).unwrap_or(i32::MAX)) / 64.0;
    let points = [start, opening, goal];
    let mut previous = f64::INFINITY;
    for leg in points.windows(2) {
        for step in 0..=20 {
            let t = f64::from(step) / 20.0;
            let at = (
                lattice(leg[0].0) + t * (lattice(leg[1].0) - lattice(leg[0].0)),
                lattice(leg[0].1) + t * (lattice(leg[1].1) - lattice(leg[0].1)),
            );
            let phi = meter.potential_of(0, at);
            assert!(phi <= previous + 1e-9, "{phi} after {previous} at {at:?}");
            previous = phi;
        }
    }
    assert!(previous < 1e-9, "Φ at the goal is {previous}");
    Ok(())
}

#[test]
fn the_sign_test_and_wilson_interval_give_known_values() {
    let rows = |values: &[u32]| {
        parse_csv(&format!(
            "rev_amp\n{}",
            values
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        ))
    };
    // Ten positive differences, none negative: p = 2 · 2^-10.
    let (p, pos, neg) = sign_test(&rows(&[5; 10]), &rows(&[1; 10]), "rev_amp");
    assert_eq!((pos, neg), (10, 0));
    assert!((p - 2.0 / 1024.0).abs() < 1e-12, "{p}");
    // Five each way and ties: p = 1.
    let a = rows(&[1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 3]);
    let b = rows(&[0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 3]);
    assert_eq!(sign_test(&a, &b, "rev_amp"), (1.0, 5, 5));
    // Six to two: p = 2 · (1 + 8 + 28) / 256 = 0.2890625.
    let a = rows(&[1, 1, 1, 1, 1, 1, 0, 0]);
    let b = rows(&[0, 0, 0, 0, 0, 0, 1, 1]);
    assert!((sign_test(&a, &b, "rev_amp").0 - 0.289_062_5).abs() < 1e-12);
    // Wilson, 5 of 10 at 95 %: 0.5 [0.2366, 0.7634].
    let (share, low, high) = wilson(5, 10);
    assert!((share - 0.5).abs() < 1e-12);
    assert!(
        (low - 0.236_6).abs() < 1e-4 && (high - 0.763_4).abs() < 1e-4,
        "{low} {high}"
    );
}

#[test]
fn five_seeds_of_every_family_play_without_diagnostics_and_repeat_exactly() -> TestResult {
    for family in [SCENES.as_slice(), &[Family::Mirror]].concat() {
        let first = run_range(family, (1, 5), false, 2, &RunOptions::default())?;
        let again = run_range(family, (1, 5), false, 3, &RunOptions::default())?;
        assert_eq!(first, again, "{family:?}");
        for row in parse_csv(&first) {
            assert_eq!(
                row.get("diagnostics").map(String::as_str),
                Some("0"),
                "{family:?} {row:?}"
            );
        }
        if family != Family::Mirror {
            let relabelled = run_range(family, (1, 5), true, 2, &RunOptions::default())?;
            assert_eq!(parse_csv(&relabelled).len(), 5);
        }
    }
    Ok(())
}

#[test]
fn a_harness_world_steps_like_any_world() -> TestResult {
    let scene = generate(Family::Arc, 2, false);
    let (mut world, ids) = build_world(&scene, &RunOptions::default())?;
    assert_eq!(ids.len(), scene.spawns.len());
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: Vec::new(),
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// E1 and E4 of plan Amendment 7 on hand-written rows: M1 splits into ticks under an order and
/// idle ticks, and in the ring families a single give-up fails T4 (the pillar lives and a route
/// reaches range in every such scene).
#[test]
fn ring_give_ups_fail_t4_and_m1_splits_by_order() {
    let header = "family,seed,expected,in_range,units,giveups,rev_amp,moving_ticks,rev_amp_idle,\
                  moving_ticks_idle";
    let rows = |giveups: u32| {
        parse_csv(&format!(
            "{header}\narc,1,4,4,4,{giveups},6,3000,4,1000\narc,2,4,4,4,0,0,1000,0,0\n"
        ))
    };
    let split = aggregate("arc", &rows(0));
    // Under an order: 2 reversals in 3,000 moving ticks; idle: 4 in 1,000.
    assert!((split.get("M1_order") - 2.0 / 3.0).abs() < 1e-9);
    assert!((split.get("M1_idle") - 4.0).abs() < 1e-9);
    let run = |label: &str, giveups: u32| {
        let mut run = Run {
            label: label.to_owned(),
            ..Run::default()
        };
        run.scenes.insert("arc".to_owned(), rows(giveups));
        run
    };
    let held = verdicts(&[run("today", 0), run("new", 0)]);
    assert!(held.contains("| T4 | arc | holds |"), "{held}");
    let failed = verdicts(&[run("today", 0), run("new", 1)]);
    assert!(failed.contains("| T4 | arc | **fails** |"), "{failed}");
}
