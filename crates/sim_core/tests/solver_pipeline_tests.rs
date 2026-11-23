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
//! - WP4's ring rule (Amendments 7 and 8): in the ring families attackers aim for free gaps and
//!   the rest stand with their orders: no corridor give-up, no standing jitter.
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

/// T4 and T7 of §6.5 on 20 development seeds of the gap and cross families. Judged at WP5
/// (plan Amendment 4, 3): WP3 to WP5 are judged together, and with WP3 alone it meets T4 on
/// these seeds but not T7 (gap: 1.91 % of moving near pair-ticks above 5 %, evidence §6.5).
/// The thresholds are unchanged.
#[test]
#[ignore = "judged at WP5 (plan 2026-11-20-contact-solver Amendment 4, 3; evidence §12): still meets T4 on these 20 seeds, not T7 (gap 1.91 % of moving near pair-ticks above 5 %); reported with the acceptance run, thresholds unchanged"]
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

/// WP4's ring rule (plan Amendments 7 and 8) on 20 development seeds of the ring families: an
/// attacker whose bearing point is taken walks to the nearest free gap, and one with no free angle
/// stands with its order. So no corridor attacker gives its target up (E1), nobody left without
/// room jitters (M3 ≤ 0.5, T2), and the corridor's queue does not oscillate (T1). Red under WP3,
/// where the queue gave up (68 give-ups on these seeds) and jostled.
#[test]
fn ring_attackers_aim_for_gaps_and_the_rest_stand() -> TestResult {
    let options = solver_options()?;
    let corridor = twenty(Family::Corridor, &options)?;
    assert_eq!(corridor.get("giveups"), 0.0, "corridor E1");
    assert!(
        corridor.get("M1") <= 1.0,
        "corridor T1 {}",
        corridor.get("M1")
    );
    for family in [Family::Arc, Family::Corridor] {
        let a = twenty(family, &options)?;
        assert!(a.get("M3") <= 0.5, "{family:?} M3 {}", a.get("M3"));
    }
    Ok(())
}

/// WP5 (plan §5.4, §7): two identical units placed point-symmetrically, each attacking the other,
/// reach range on the same tick and start firing on the same tick. Under the solver pipeline the
/// pursuit reads its target's tick-start position, so the unit processed second does not see
/// where the first went this tick. Red when the pursuit reads the live target.
#[test]
fn a_mirrored_pair_reaches_range_and_fires_on_the_same_tick() -> TestResult {
    let mut world = solver_world(0x5EED_0005, true)?;
    let unit = kind(&world)?;
    let blue = world.spawn_unit(unit, PlayerId(0), at(20.0, 32.0))?;
    let red = world.spawn_unit(unit, PlayerId(1), at(44.0, 32.0))?;
    let attack = |player: u8, unit: EntityId, target: EntityId| PlayerCommand {
        player_id: PlayerId(player),
        entities: vec![unit],
        order: UnitOrder::AttackTarget {
            target,
            origin: sim_types::AttackOrigin::Explicit,
        },
        queue: false,
    };
    step(&mut world, vec![attack(0, blue, red), attack(1, red, blue)]);
    let (mut stopped, mut fired) = ([None; 2], [None; 2]);
    for _ in 0..400 {
        step(&mut world, vec![]);
        let tick = world.current_tick;
        for (slot, id) in [blue, red].into_iter().enumerate() {
            let Some(entity) = world.entities.get(&id) else {
                continue;
            };
            if stopped[slot].is_none() && entity.velocity == SimVec2::ZERO {
                stopped[slot] = Some(tick);
            }
            if fired[slot].is_none() && entity.current_anim == sim_types::AnimState::Attack {
                fired[slot] = Some(tick);
            }
        }
        if fired.iter().all(Option::is_some) {
            break;
        }
    }
    assert!(fired[0].is_some(), "the pair never fired: {fired:?}");
    assert_eq!(stopped[0], stopped[1], "reached range on different ticks");
    assert_eq!(fired[0], fired[1], "fired on different ticks");
    Ok(())
}

/// P8 (plan Amendment 11): the ring interval cache is exact. Ring scenes of the arc and corridor
/// families play to the same positions and the same checksum on every tick with the cache on and
/// off.
#[test]
fn the_ring_interval_cache_changes_nothing() -> TestResult {
    use sim_fixtures::harness::corpus::generate;
    use sim_fixtures::harness::{build_world, opening_commands};
    let options = solver_options()?;
    for (family, seed) in [
        (Family::Arc, 1),
        (Family::Arc, 5),
        (Family::Arc, 563),
        (Family::Corridor, 4),
    ] {
        let scene = generate(family, seed, false);
        let mut runs = Vec::new();
        for cached in [true, false] {
            let (mut world, ids) = build_world(&scene, &options)?;
            world.set_ring_interval_cache(cached);
            let mut commands = opening_commands(&scene, &ids);
            let mut trace = Vec::new();
            for _ in 0..scene.ticks.min(600) {
                step(&mut world, std::mem::take(&mut commands));
                trace.push(world.last_checksum());
                trace.extend(ids.iter().filter_map(|id| {
                    world.entities.get(id).map(|e| {
                        e.position.x.raw().unsigned_abs() ^ e.position.y.raw().unsigned_abs() << 1
                    })
                }));
            }
            runs.push(trace);
        }
        assert!(
            runs[0] == runs[1],
            "{family:?} seed {seed} differs with the cache off"
        );
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

/// The arrival rule (plan §3.10, Amendment 3 item 4, B-158): in the mixed-priority gap scenes of development seeds 12,
/// 15 and 16, lower-priority walkers reached the clump of higher-priority arrivals at the shared
/// point, could not push them (C-2) and touched only arrivals beyond the bounded witness reach,
/// so they gave their `Move` up. Under the group-arrival chain each of them arrives.
#[test]
fn mixed_priority_walkers_all_arrive_at_the_shared_point() -> TestResult {
    let options = solver_options()?;
    for seed in [12, 15, 16] {
        let csv = run_range(Family::Gap, (seed, 1), false, 1, &options)?;
        let a = aggregate(Family::Gap.name(), &parse_csv(&csv));
        assert_eq!(a.get("giveups"), 0.0, "seed {seed}: give-ups");
        assert_eq!(a.get("deadlocks"), 0.0, "seed {seed}: deadlocks");
        assert_eq!(a.get("complete_scen"), 100.0, "seed {seed}: complete");
    }
    Ok(())
}

/// The arrival rule (plan §3.10): the arrival chain runs only through arrivals. A walker that gave its `Move` up in front
/// of a closed way (here a line of friends on Hold Position across a corridor) is no witness, so a
/// second walker to the same point that touches it keeps its order instead of ending it short of
/// the way through (the failure of the unbounded chain measured in §5.3 of the plan).
#[test]
fn a_give_up_is_no_witness_of_the_arrival_chain() -> TestResult {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    for x in 0..64 {
        for y in (0..64).filter(|y| !(30..33).contains(y)) {
            map.cell_mut(x, y)?.flags = CellFlags::empty();
        }
    }
    let mut world = sim_fixtures::world(map, 0x6A7E);
    let rules = world.catalog.movement_rules();
    let contact = ContactRules {
        pipeline: MovementPipeline::Solver,
        ..rules.contact()
    };
    world
        .catalog
        .set_movement_rules(rules.with_contact(contact)?);
    let unit = kind(&world)?;
    let r = f64::from(world.catalog.get(unit)?.collision_radius.to_f32());
    let mut holders = Vec::new();
    // Holders edge to edge across the whole opening, so no walker squeezes past them.
    let mut y = 30.0 + r;
    loop {
        holders.push(world.spawn_unit(unit, PlayerId(0), at(30.5, y))?);
        if y + r >= 33.0 {
            break;
        }
        y = (y + 2.0 * r - 0.01).min(33.0 - r);
    }
    let first = world.spawn_unit(unit, PlayerId(0), at(20.0, 31.5))?;
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: holders,
            order: UnitOrder::HoldPosition,
            queue: false,
        }],
    );
    step(&mut world, vec![move_to(first, 45.0, 31.5)]);
    let mut gave_up = false;
    for _ in 0..600 {
        step(&mut world, vec![]);
        if world.entities[&first].order_queue.is_empty() {
            gave_up = true;
            break;
        }
    }
    assert!(gave_up, "the first walker gives its blocked move up");
    assert_eq!(world.entities[&first].movement.arrived_at, None);
    let second = world.spawn_unit(unit, PlayerId(0), at(22.0, 31.5))?;
    step(&mut world, vec![move_to(second, 45.0, 31.5)]);
    for tick in 0..60 {
        step(&mut world, vec![]);
        assert!(
            !world.entities[&second].order_queue.is_empty(),
            "tick {tick}: the second walker ended its move on a give-up"
        );
    }
    assert!(gap(&world, first, second) <= 0.15, "the walkers touch");
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

/// A unit heading as a unit vector, `f64` for measurement only.
type Heading = (f64, f64);
/// A turn between two headings: the tick, its sign and its size in degrees.
type Turn = (u32, f64, f64);

/// Amendment 3, item 5: the kept `battlefield_bubble_trajectories_are_smooth_without_micro_jitter`
/// played under the solver pipeline. Its zig-zag was traced to the parked-corner aim (L24)
/// reading a body beyond the legacy reach, which the side rule's wider neighbour list had let in;
/// with every rule but the side rule reading the near reach, the march has no zig-zag.
#[test]
fn the_canyon_march_has_no_zig_zag_under_the_solver() -> TestResult {
    use sim_core::scenario::{PlayerSetup, StartLoadoutCatalog};
    use sim_map::MapFile;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let doc = MapFile::load(&dir.join("../../assets/maps/showcase_canyon.scmap.ron"))?.decode()?;
    let loadouts = StartLoadoutCatalog::load(dir.join("../../assets/catalog/start_loadouts.ron"))?;
    let players: Vec<PlayerSetup> = (0..2)
        .map(|player_slot| PlayerSetup {
            player_slot,
            loadout_slug: "battle_automaton".to_string(),
        })
        .collect();
    let mut world = sim_fixtures::scenario_world(&doc, &loadouts, &players, 1_592_639_215)?;
    let rules = world.catalog.movement_rules();
    let contact = ContactRules {
        pipeline: MovementPipeline::Solver,
        ..rules.contact()
    };
    world
        .catalog
        .set_movement_rules(rules.with_contact(contact)?);
    let mut marchers = Vec::new();
    let mut fastest = SimScalar::ZERO;
    for (&id, entity) in &world.entities {
        let arch = world.catalog.get(entity.unit_type_id)?;
        let armed =
            arch.ground_weapon.is_some() || arch.air_weapon.is_some() || arch.weapon.is_some();
        if entity.owner == PlayerId(0)
            && !entity.is_immobile
            && arch.capabilities.harvester.is_none()
            && armed
        {
            marchers.push(id);
            fastest = fastest.max(arch.max_speed);
        }
    }
    // The kept test's window: 16.2 world units at the fastest marcher's speed.
    let window = SimScalar::from_num(16.2)
        .checked_div(fastest)
        .ok_or("a mobile marcher")?
        .saturating_ceil()
        .saturating_to_u32();
    let target = at(52.5, 52.5);
    step(
        &mut world,
        vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: marchers.clone(),
            order: UnitOrder::AttackMove { target },
            queue: false,
        }],
    );
    // Per unit: the last heading and the last turn (tick, sign, degrees), as the kept test counts.
    let mut last: std::collections::BTreeMap<EntityId, (Heading, Option<Turn>)> =
        std::collections::BTreeMap::new();
    let mut zig_zags = Vec::new();
    for tick in 1..=window {
        step(&mut world, vec![]);
        for &id in &marchers {
            let Some(entity) = world.entities.get(&id) else {
                continue;
            };
            let (vx, vy) = (
                f64::from(entity.velocity.x.to_f32()),
                f64::from(entity.velocity.y.to_f32()),
            );
            let speed = vx.hypot(vy);
            if speed < 0.01 {
                continue;
            }
            let heading = (vx / speed, vy / speed);
            let turn = last.get(&id).map(|&((px, py), _)| {
                let angle = (heading.0 * px + heading.1 * py)
                    .clamp(-1.0, 1.0)
                    .acos()
                    .to_degrees();
                let cross = px * heading.1 - py * heading.0;
                let sign = if cross > 0.02 {
                    1.0
                } else if cross < -0.02 {
                    -1.0
                } else {
                    0.0
                };
                (tick, sign, angle)
            });
            if let (Some((_, sign, angle)), Some((_, Some((prev_tick, prev_sign, prev_angle))))) =
                (turn, last.get(&id))
            {
                if *prev_tick == tick - 1
                    && tick > 3
                    && prev_sign * sign < 0.0
                    && angle > 8.0
                    && *prev_angle > 8.0
                {
                    zig_zags.push((id, tick));
                }
            }
            let kept = turn.or_else(|| last.get(&id).and_then(|entry| entry.1));
            last.insert(id, (heading, kept));
        }
    }
    assert_eq!(
        zig_zags,
        [],
        "velocity zig-zags (> 8° each way on consecutive ticks)"
    );
    assert_eq!(world.diagnostics(), []);
    Ok(())
}
