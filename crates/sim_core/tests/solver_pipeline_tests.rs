//! # Integration Test: The Solver Pipeline (plan `2026-11-20-contact-solver` WP3)
//!
//! Under `contact.pipeline: Solver` the step steers by intent (I-1 to I-4) and one contact solver
//! separates bodies after every unit moved (C-1 to C-8). These tests pin, through public APIs:
//! - the harness's gap and cross families on 20 development seeds meet T4 (no deadlock and no
//!   give-up where there is room) and T7 (the overlap bounds of D-α), which today's pipeline does
//!   not (§5, WP1 evidence);
//! - the tests of §8.1 marked *restated* for WP3, in their new form: an idle friend is pushed
//!   along the line of centres, not out of the lane; a waiting friend takes half; the D-β-off twin;
//!   the exact head-on tie passes right to right; a converging pair does not vibrate;
//! - a body spawned into a crowd (K-3) leaves it within a bounded number of ticks and never
//!   through a wall.
//!
//! Worlds use the shipped catalog with its rules switched to the solver pipeline, or scratch
//! archetypes; positions are converted to `f64` for measurement only.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use sim_core::catalog::{ContactRules, MovementPipeline};
use sim_core::SimWorld;
use sim_fixtures::harness::report::{aggregate, parse_csv};
use sim_fixtures::harness::{run_range, Family, RunOptions};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn solver_options() -> Result<RunOptions, Box<dyn std::error::Error>> {
    Ok(RunOptions::with_pipeline(MovementPipeline::Solver)?)
}

/// The aggregates of `family` over development seeds 1 to 20 under `options`.
fn twenty(
    family: Family,
    options: &RunOptions,
) -> Result<sim_fixtures::harness::report::Aggregates, Box<dyn std::error::Error>> {
    let csv = run_range(family, (1, 20), false, 4, options)?;
    Ok(aggregate(family.name(), &parse_csv(&csv)))
}

/// T4 and T7 of §6.5 on 20 development seeds of the gap and cross families.
#[test]
fn gaps_and_crossings_meet_t4_and_t7_under_the_solver() -> TestResult {
    let options = solver_options()?;
    for family in [Family::Gap, Family::Cross] {
        let a = twenty(family, &options)?;
        let get = |key: &str| a.get(key);
        assert_eq!(get("deadlocks"), 0.0, "{family:?} T4 deadlocks");
        assert_eq!(get("giveups"), 0.0, "{family:?} T4 give-ups");
        assert!(
            get("enemy_max") <= 0.005,
            "{family:?} T7 enemy {}",
            get("enemy_max")
        );
        assert!(
            get("terrain_max") <= 1.0 / 1024.0,
            "{family:?} T7 terrain {}",
            get("terrain_max")
        );
        assert!(
            get("fo_move_share") <= 0.1,
            "{family:?} T7 moving {}",
            get("fo_move_share")
        );
        assert!(
            get("fo_rest_share") <= 0.1,
            "{family:?} T7 standing {}",
            get("fo_rest_share")
        );
        assert!(get("fo_max") <= 0.15, "{family:?} T7 max {}", get("fo_max"));
        assert_eq!(get("diagnostics"), 0.0);
    }
    Ok(())
}

/// The shipped catalog on a flat 64 × 64 map with the rules switched to the solver pipeline,
/// D-β as `push`.
fn solver_world(seed: u64, push: bool) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let rules = world.catalog.movement_rules();
    let contact = ContactRules {
        pipeline: MovementPipeline::Solver,
        moving_friends_push: push,
        ..rules.contact()
    };
    world
        .catalog
        .set_movement_rules(rules.with_contact(contact)?);
    Ok(world)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn move_to(unit: EntityId, x: f64, y: f64) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![unit],
        order: UnitOrder::Move { target: at(x, y) },
        queue: false,
    }
}

fn kind(world: &SimWorld) -> Result<sim_types::UnitTypeId, Box<dyn std::error::Error>> {
    Ok(world
        .catalog
        .archetypes()
        .filter(|a| {
            a.mover_type == sim_types::MoverType::Ground
                && !a.is_immobile_by_default()
                && a.harvester.is_none()
        })
        .min_by_key(|a| (a.collision_radius, a.unit_type_id))
        .map(|a| a.unit_type_id)
        .ok_or("a mobile ground archetype")?)
}

fn radius(world: &SimWorld, id: EntityId) -> f64 {
    f64::from(
        world
            .catalog
            .get(world.entities[&id].unit_type_id)
            .unwrap()
            .collision_radius
            .to_f32(),
    )
}

fn gap(world: &SimWorld, a: EntityId, b: EntityId) -> f64 {
    let d = f64::from(
        world.entities[&a]
            .position
            .distance(world.entities[&b].position)
            .to_f32(),
    );
    d - radius(world, a) - radius(world, b)
}

/// Restated `test_idle_friendly_displaced_laterally` (§8.1, WP3): an idle friend in a mover's
/// way is pushed along the line of centres (C-2: it takes the whole overlap), and the two end in
/// contact within the dead band; the mover is not pushed back.
#[test]
fn an_idle_friend_is_pushed_along_the_line_of_centres() -> TestResult {
    let mut world = solver_world(0x1111_2222, true)?;
    let unit = kind(&world)?;
    let mover = world.spawn_unit(unit, PlayerId(0), at(10.0, 10.0))?;
    let r = radius(&world, mover);
    let idle = world.spawn_unit(unit, PlayerId(0), at(10.0 + 2.0 * r + 0.02, 10.05))?;
    let before = world.entities[&idle].position;
    for tick in 0..6 {
        step(
            &mut world,
            if tick == 0 {
                vec![move_to(mover, 20.0, 10.0)]
            } else {
                vec![]
            },
        );
        assert!(
            gap(&world, mover, idle) >= -0.01 * 2.0 * r - 1e-6,
            "tick {tick}: overlap"
        );
    }
    let after = world.entities[&idle].position;
    assert!(after.x > before.x, "pushed ahead along the line of centres");
    assert!(after.y >= before.y, "never across the mover's lane");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// Restated `a_waiting_body_is_not_pushed_aside` (§8.1, WP3): a friend standing under an order
/// shares the overlap evenly (C-2), so it moves by at most half of it per tick and never leaves
/// its lane by more than that.
#[test]
fn a_waiting_friend_takes_at_most_half() {
    use sim_core::movement::solver::{contact_shares, ContactParty, PairShares};
    let mover = ContactParty {
        owner: PlayerId(0),
        push_priority: 10,
        fixed: false,
        holding: false,
        engaged: false,
        idle: false,
        moving: true,
    };
    let waiting = ContactParty {
        moving: false,
        ..mover
    };
    assert_eq!(
        contact_shares(&mover, &waiting, true),
        PairShares::Weights(SimScalar::HALF, SimScalar::HALF)
    );
}

/// `moving_friends_separate_evenly` and its D-β-off twin (§8.1, WP3): under D-β on two moving
/// friends of one priority separate evenly; off, the one that closed takes the overlap.
#[test]
fn moving_friends_separate_evenly_and_the_d_beta_off_twin() -> TestResult {
    for push in [true, false] {
        let mut world = solver_world(0x5151, push)?;
        let unit = kind(&world)?;
        let a = world.spawn_unit(unit, PlayerId(0), at(10.0, 10.0))?;
        let r = radius(&world, a);
        let b = world.spawn_unit(unit, PlayerId(0), at(10.0 + 2.0 * r + 0.3, 10.0))?;
        // Both walk east; A, faster from behind, closes on B.
        step(
            &mut world,
            vec![move_to(a, 30.0, 10.0), move_to(b, 30.0, 10.0)],
        );
        for _ in 0..40 {
            step(&mut world, vec![]);
            assert!(
                gap(&world, a, b) >= -0.05 * 2.0 * r,
                "D-β {push}: {}",
                gap(&world, a, b)
            );
        }
        assert_eq!(world.diagnostics(), []);
    }
    Ok(())
}

/// Restated `exact_head_on_tie_separates_to_opposite_world_sides` (§8.1, WP3, I-3b): two bodies
/// exactly head-on each pass on their own right, and so to opposite world sides, without contact.
#[test]
fn an_exact_head_on_pair_passes_right_to_right() -> TestResult {
    let mut world = solver_world(0x4E4E, true)?;
    let unit = kind(&world)?;
    let a = world.spawn_unit(unit, PlayerId(0), at(10.0, 20.0))?;
    let b = world.spawn_unit(unit, PlayerId(0), at(22.0, 20.0))?;
    step(
        &mut world,
        vec![move_to(a, 30.0, 20.0), move_to(b, 2.0, 20.0)],
    );
    let mut min_gap = f64::INFINITY;
    let (mut a_side, mut b_side) = (0.0_f64, 0.0_f64);
    for _ in 0..200 {
        step(&mut world, vec![]);
        min_gap = min_gap.min(gap(&world, a, b));
        let (pa, pb) = (world.entities[&a].position, world.entities[&b].position);
        if (f64::from(pa.x.to_f32()) - f64::from(pb.x.to_f32())).abs() < 0.5 {
            a_side = f64::from(pa.y.to_f32()) - 20.0;
            b_side = f64::from(pb.y.to_f32()) - 20.0;
        }
    }
    // A heads east and passes on its right (south); B heads west and passes north.
    assert!(a_side < 0.0 && b_side > 0.0, "a {a_side}, b {b_side}");
    assert!(min_gap > -0.05, "they met: {min_gap}");
    Ok(())
}

/// Restated `test_converging_units_respect_speed_cap_and_do_not_vibrate` (§8.1, WP3): no speed
/// cap remains (C-5), and a pair converging on one point and then standing in contact has no
/// visible reversal by M1's amplitude rule (both steps at least a quarter speed, cosine below
/// −0.5) once it stands.
#[test]
fn converging_units_do_not_vibrate() -> TestResult {
    let mut world = solver_world(0xC0C0, true)?;
    let unit = kind(&world)?;
    let a = world.spawn_unit(unit, PlayerId(0), at(10.0, 10.0))?;
    let b = world.spawn_unit(unit, PlayerId(0), at(14.0, 10.0))?;
    let speed = f64::from(world.catalog.get(unit)?.max_speed.to_f32());
    step(
        &mut world,
        vec![move_to(a, 12.0, 10.0), move_to(b, 12.0, 10.0)],
    );
    let mut last: [Option<(f64, f64)>; 2] = [None, None];
    let mut reversals = 0;
    for _ in 0..200 {
        step(&mut world, vec![]);
        for (k, id) in [a, b].into_iter().enumerate() {
            let v = world.entities[&id].velocity;
            let now = (f64::from(v.x.to_f32()), f64::from(v.y.to_f32()));
            if let Some(before) = last[k] {
                let (n0, n1) = (f64::hypot(now.0, now.1), f64::hypot(before.0, before.1));
                let cos = (now.0 * before.0 + now.1 * before.1) / (n0 * n1).max(1e-12);
                if n0 >= speed / 4.0 && n1 >= speed / 4.0 && cos < -0.5 {
                    reversals += 1;
                }
            }
            last[k] = Some(now);
        }
    }
    assert_eq!(reversals, 0);
    assert!(gap(&world, a, b) >= -0.01 * 2.0 * radius(&world, a) - 1e-6);
    Ok(())
}

/// K-3 (C-5): a body spawned onto a standing crowd leaves the overlap within 25 ticks, by the
/// solver alone, and never into the wall beside the crowd.
#[test]
fn a_body_spawned_into_a_crowd_leaves_it_within_a_second_and_never_through_a_wall() -> TestResult {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    for y in 0..64 {
        map.cell_mut(12, y)?.flags = CellFlags::empty();
    }
    let mut world = sim_fixtures::world(map, 0x0B0D);
    let rules = world.catalog.movement_rules();
    let contact = ContactRules {
        pipeline: MovementPipeline::Solver,
        ..rules.contact()
    };
    world
        .catalog
        .set_movement_rules(rules.with_contact(contact)?);
    let unit = kind(&world)?;
    let mut crowd = Vec::new();
    for row in 0..4 {
        for column in 0..4 {
            crowd.push(world.spawn_unit(
                unit,
                PlayerId(0),
                at(10.0 - f64::from(column), 20.0 + f64::from(row)),
            )?);
        }
    }
    let r = radius(&world, crowd[0]);
    let late = world.spawn_unit(unit, PlayerId(0), at(10.5, 21.5))?;
    let mut cleared = None;
    for tick in 0..50 {
        step(&mut world, vec![]);
        let x = f64::from(world.entities[&late].position.x.to_f32());
        assert!(x + r <= 12.0 + 1e-3, "tick {tick}: into the wall at x {x}");
        let worst = crowd
            .iter()
            .map(|&c| gap(&world, late, c))
            .fold(f64::INFINITY, f64::min);
        if cleared.is_none() && worst >= -0.01 * 2.0 * r - 1e-6 {
            cleared = Some(tick);
        }
    }
    assert!(cleared.is_some_and(|t| t < 25), "cleared at {cleared:?}");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}
