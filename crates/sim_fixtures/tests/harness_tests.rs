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
fn still_scene() -> Result<(sim_core::SimWorld, ScenarioMeter, EntityId), Box<dyn Error>> {
    let scene = generate(Family::Cross, 3, false);
    let (world, ids) = build_world(&scene, &RunOptions::default())?;
    let meter = ScenarioMeter::new(&world, &scene, &ids);
    Ok((world, meter, ids[0]))
}

fn place(world: &mut sim_core::SimWorld, id: EntityId, x: f64, y: f64) -> TestResult {
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

/// D-S (plan Amendment 11): an attacker under its order that makes no progress for ten seconds
/// while its ring is empty counts as stuck with room, from the 250th tick on; an attacker
/// without an order never counts.
#[test]
fn an_attacker_held_short_of_an_empty_ring_is_stuck_with_room() -> TestResult {
    let scene = generate(Family::Arc, 3, false);
    let (mut world, ids) = build_world(&scene, &RunOptions::default())?;
    let mut idle = ScenarioMeter::new(&world, &scene, &ids);
    for _ in 0..260 {
        idle.observe(&world);
    }
    assert_eq!(idle.finish().stuck_with_room, 0);
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: sim_fixtures::harness::opening_commands(&scene, &ids),
        state_checksum: None,
    });
    // Held where it stands: the world is not stepped again, so no attacker moves.
    let mut meter = ScenarioMeter::new(&world, &scene, &ids);
    for _ in 0..260 {
        meter.observe(&world);
    }
    let attackers = u32::try_from(ids.len() - 1)?;
    // Progress is set on the first tick observed (tick 1), so ticks 251 to 260 count: ten per
    // attacker.
    assert_eq!(meter.finish().stuck_with_room, attackers * 10);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Plan `2026-11-27-gated-movement` WP0a: the eight new families, their known answers, the
// meters those answers read, and the permutation mode. The known answers are stated by the
// plan (§5.2) before any `Gated` code; these tests pin that each meter counts what its answer
// names, on tracks written by hand, and that a scene that breaks the answer is judged broken.
// ---------------------------------------------------------------------------------------------

use sim_fixtures::harness::answers::{judge, KnownAnswer, LaneUse};
use sim_fixtures::harness::corpus::{CueOrder, Lattice};
use sim_fixtures::harness::metrics::known::{KnownMeter, KnownRow};
use sim_fixtures::harness::metrics::ScenarioRow;
use sim_fixtures::harness::{permutation_range, PermutationRow};
use sim_fixtures::trace::contact_checks::moved_by_others;
use sim_types::{EntityId, UnitOrder};

/// A world point from sixty-fourths, in world units as `f64`.
fn lattice_point(at: (Lattice, Lattice)) -> (f64, f64) {
    let world = |v: Lattice| f64::from(i32::try_from(v).unwrap_or(i32::MAX)) / 64.0;
    (world(at.0), world(at.1))
}

/// A scene, its world, its units' ids in spawn order, and the known-answer meter on it.
type KnownScene = (
    sim_fixtures::harness::corpus::Scenario,
    sim_core::SimWorld,
    Vec<EntityId>,
    KnownMeter,
);

/// The scene of `family` and `seed` built into a world, with the known-answer meter on it.
fn known_scene(family: Family, seed: u64) -> Result<KnownScene, Box<dyn Error>> {
    let scene = generate(family, seed, false);
    let (world, ids) = build_world(&scene, &RunOptions::default())?;
    let meter = KnownMeter::new(&world, &scene, &ids).ok_or("a new family has a known answer")?;
    Ok((scene, world, ids, meter))
}

/// The spawn indices whose role satisfies `pick`.
fn spawns_where(
    scene: &sim_fixtures::harness::corpus::Scenario,
    pick: impl Fn(&Role) -> bool,
) -> Vec<usize> {
    scene
        .spawns
        .iter()
        .enumerate()
        .filter(|(_, spawn)| pick(&spawn.role))
        .map(|(index, _)| index)
        .collect()
}

/// W0-T1: every new family is a pure function of family and seed on the 1/64 lattice, and a
/// relabelled scene differs from it only in its spawn order.
#[test]
fn new_families_are_pure_functions_of_family_and_seed() {
    for family in Family::NEW {
        for seed in 1..=20 {
            let scene = generate(family, seed, false);
            assert_eq!(scene, generate(family, seed, false), "{family:?} {seed}");
            assert!(scene.known.is_some(), "{family:?} {seed}: no known answer");
            assert!(scene.ticks > 0 && !scene.spawns.is_empty(), "{family:?}");
            for spawn in &scene.spawns {
                for value in [spawn.at.0, spawn.at.1] {
                    assert_eq!((value << 26) >> 26, value);
                    assert!(value > 0, "{family:?} {seed}: {spawn:?}");
                }
                let (x, y) = lattice_point(spawn.at);
                let (w, h) = scene.map.size;
                assert!(x < f64::from(w) && y < f64::from(h), "{family:?} {seed}");
            }
            let relabelled = generate(family, seed, true);
            let mut a = scene.spawns.clone();
            let mut b = relabelled.spawns.clone();
            a.sort_by_key(|s| (s.at, s.owner, s.archetype));
            b.sort_by_key(|s| (s.at, s.owner, s.archetype));
            assert_eq!(a, b, "{family:?} {seed}");
            assert_eq!(
                (&scene.map, &scene.archetypes, &scene.script, scene.known),
                (
                    &relabelled.map,
                    &relabelled.archetypes,
                    &relabelled.script,
                    relabelled.known
                )
            );
        }
        assert_ne!(generate(family, 1, false), generate(family, 2, false));
        assert_eq!(Family::parse(family.name()), Some(family));
    }
}

/// W0-T2: a holder displaced by others counts (its unit-tick and its displacement), a holder
/// that only turns does not, and a walker's displacement is never a holder's.
#[test]
fn held_moved_counts_a_displaced_holder_and_not_a_turning_one() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::HoldLine, 3)?;
    let holder = ids[*spawns_where(&scene, |r| *r == Role::Hold)
        .first()
        .ok_or("a holder")?];
    let walker = ids[*spawns_where(&scene, |r| matches!(r, Role::GroupMove { .. }))
        .first()
        .ok_or("a walker")?];
    let entity = world.entities.get_mut(&holder).ok_or("unit")?;
    entity.order_queue.push_back(UnitOrder::HoldPosition);
    meter.observe(&world);
    // The holder turns in place.
    let entity = world.entities.get_mut(&holder).ok_or("unit")?;
    entity.heading = sim_types::SimAngle::from_raw(16_384);
    world.current_tick += 1;
    meter.observe(&world);
    // The walker moves a whole unit: not a holder.
    let start = world.entities[&walker].position;
    let (wx, wy) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    place(&mut world, walker, wx + 1.0, wy)?;
    meter.observe(&world);
    // The holder is displaced by 0.25.
    let start = world.entities[&holder].position;
    let (hx, hy) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    place(&mut world, holder, hx, hy + 0.25)?;
    meter.observe(&world);
    let row = meter.finish();
    assert_eq!(row.held_moved_ticks, 1, "{row:?}");
    assert!((row.held_moved_max - 0.25).abs() < 1e-6, "{row:?}");
    let base = ScenarioRow {
        done: scene.expected,
        expected: scene.expected,
        ..Default::default()
    };
    let verdict = judge(scene.known.ok_or("known")?, &base, &row);
    assert!(
        !verdict.hard && verdict.failed.contains(&"held_moved"),
        "{verdict:?}"
    );
    Ok(())
}

/// W0-T3: an enemy that proposed no motion and is displaced counts; the same enemy walking under
/// an order of its own does not.
#[test]
fn enemy_moved_by_others_ignores_the_enemys_own_step() -> TestResult {
    let mut seed = 1;
    let (scene, mut world, ids, mut meter) = loop {
        let candidate = known_scene(Family::EnemyPress, seed)?;
        if matches!(
            candidate.0.known,
            Some(KnownAnswer::EnemyPress {
                stance: sim_fixtures::harness::answers::Stance::Idle
            })
        ) {
            break candidate;
        }
        seed += 1;
    };
    let enemy = ids[*spawns_where(&scene, |r| *r == Role::Stand)
        .iter()
        .find(|&&i| scene.spawns[i].owner == 1)
        .ok_or("an idle enemy")?];
    meter.observe(&world);
    let start = world.entities[&enemy].position;
    let (x, y) = (f64::from(start.x.to_f32()), f64::from(start.y.to_f32()));
    place(&mut world, enemy, x - 0.125, y)?;
    meter.observe(&world);
    // Under a move of its own the next step is its own.
    let entity = world.entities.get_mut(&enemy).ok_or("unit")?;
    entity.order_queue.push_back(UnitOrder::Move {
        target: SimVec2::new(SimScalar::from_num(x + 5.0), SimScalar::from_num(y)),
    });
    meter.observe(&world);
    place(&mut world, enemy, x, y)?;
    meter.observe(&world);
    let row = meter.finish();
    assert_eq!(row.enemy_moved_ticks, 1, "{row:?}");
    assert!((row.enemy_moved_max - 0.125).abs() < 1e-6, "{row:?}");
    let verdict = judge(scene.known.ok_or("known")?, &ScenarioRow::default(), &row);
    assert!(
        !verdict.hard && verdict.failed.contains(&"enemy_moved"),
        "{verdict:?}"
    );
    // The pure rule: a unit standing still at the tick start with no proposed step counts all
    // its displacement; one that proposed a step, or was walking, counts none.
    assert!((moved_by_others(true, None, (0.3, 0.4)) - 0.5).abs() < 1e-12);
    assert!((moved_by_others(true, Some((0.0, 0.0)), (0.3, 0.4)) - 0.5).abs() < 1e-12);
    assert!(moved_by_others(true, Some((0.1, 0.0)), (0.3, 0.4)).abs() < 1e-12);
    assert!(moved_by_others(false, None, (0.3, 0.4)).abs() < 1e-12);
    Ok(())
}

/// W0-T4: a walker whose centre crosses between the two bodies has passed the narrow gap, and
/// the scene fails its answer; one that goes round has not.
#[test]
fn the_wedge_answer_fails_a_pass_through_the_narrow_gap() -> TestResult {
    for through in [true, false] {
        let (scene, mut world, ids, mut meter) = known_scene(Family::Wedge, 5)?;
        let Some(KnownAnswer::Wedge { bodies, .. }) = scene.known else {
            return Err("a wedge scene names its bodies".into());
        };
        let walker = ids[*spawns_where(&scene, |r| matches!(r, Role::OwnMove { .. }))
            .first()
            .ok_or("the walker")?];
        let (a, b) = (lattice_point(bodies[0]), lattice_point(bodies[1]));
        let middle = (f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1));
        // Round: well clear of both bodies' outer sides.
        let detour = if through {
            0.0
        } else {
            (a.1 - b.1).abs() + 6.0
        };
        meter.observe(&world);
        for x in [
            middle.0 - 2.0,
            middle.0 - 0.5,
            middle.0 + 0.5,
            middle.0 + 2.0,
        ] {
            place(&mut world, walker, x, middle.1 + detour)?;
            meter.observe(&world);
        }
        let row = meter.finish();
        assert_eq!(row.wedge_passed, u32::from(through), "{row:?}");
        let base = ScenarioRow {
            done: 1,
            expected: 1,
            ..Default::default()
        };
        let verdict = judge(scene.known.ok_or("known")?, &base, &row);
        assert_eq!(verdict.holds, !through, "{verdict:?}");
        assert_eq!(verdict.failed.contains(&"wedge_passed"), through);
    }
    Ok(())
}

/// W0-T5: the stutter lag is counted from the tick whose bundle carried the order: a unit whose
/// first step along the order shows two observations after that bundle has lag 2, and the hard
/// check (lag 1) fails.
#[test]
fn stutter_lag_is_read_from_the_order_tick() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::Stutter, 2)?;
    let cue = scene
        .script
        .iter()
        .find(|cue| matches!(cue.order, CueOrder::MoveBy { .. }))
        .copied()
        .ok_or("a move cue")?;
    let CueOrder::MoveBy { offset } = cue.order else {
        return Err("a move cue".into());
    };
    let attackers: Vec<EntityId> = spawns_where(&scene, |r| *r == Role::Attack)
        .into_iter()
        .map(|i| ids[i])
        .collect();
    let direction = lattice_point(offset);
    let length = direction.0.hypot(direction.1);
    let unit = (direction.0 / length, direction.1 / length);
    // Observations 1 ..= cue.tick are before the cue; the bundle of tick `cue.tick` is played by
    // the step observed as observation `cue.tick + 1`.
    for _ in 0..cue.tick {
        world.current_tick += 1;
        meter.observe(&world);
    }
    world.current_tick += 1;
    meter.observe(&world);
    for id in &attackers {
        let at = world.entities[id].position;
        let (x, y) = (f64::from(at.x.to_f32()), f64::from(at.y.to_f32()));
        let entity = world.entities.get_mut(id).ok_or("unit")?;
        entity.position = SimVec2::new(
            SimScalar::from_num(x + 0.1 * unit.0),
            SimScalar::from_num(y + 0.1 * unit.1),
        );
    }
    world.current_tick += 1;
    meter.observe(&world);
    let row = meter.finish();
    assert_eq!(row.stutter_lag_max, 2, "{row:?}");
    let verdict = judge(scene.known.ok_or("known")?, &ScenarioRow::default(), &row);
    assert!(
        !verdict.hard && verdict.failed.contains(&"stutter_lag"),
        "{verdict:?}"
    );
    Ok(())
}

/// W0-T6: the fill time of surround-moving runs from the first tick an attacker stands in its
/// band of the target, not from the scene's start.
#[test]
fn surround_fill_time_starts_at_first_contact() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::SurroundMoving, 4)?;
    let Some(KnownAnswer::SurroundMoving { capacity, band }) = scene.known else {
        return Err("a surround scene names its capacity".into());
    };
    let target = ids[*spawns_where(&scene, |r| matches!(r, Role::Circle { .. }))
        .first()
        .ok_or("the target")?];
    let attackers: Vec<EntityId> = spawns_where(&scene, |r| *r == Role::Attack)
        .into_iter()
        .map(|i| ids[i])
        .collect();
    let wanted = attackers.len().min(usize::try_from(capacity)?);
    let centre = world.entities[&target].position;
    let (cx, cy) = (f64::from(centre.x.to_f32()), f64::from(centre.y.to_f32()));
    let ring = lattice_point((band, 0)).0 - 0.01;
    for _ in 0..10 {
        world.current_tick += 1;
        meter.observe(&world);
    }
    // Tick 11: the first attacker reaches its band; 20 ticks later all `wanted` stand in it.
    for (index, id) in attackers.iter().enumerate().take(wanted) {
        let angle = std::f64::consts::TAU * f64::from(u32::try_from(index)?)
            / f64::from(u32::try_from(wanted)?);
        let entity = world.entities.get_mut(id).ok_or("unit")?;
        entity.position = SimVec2::new(
            SimScalar::from_num(cx + ring * angle.cos()),
            SimScalar::from_num(cy + ring * angle.sin()),
        );
        if index == 0 {
            world.current_tick += 1;
            meter.observe(&world);
            for _ in 0..19 {
                world.current_tick += 1;
                meter.observe(&world);
            }
        }
    }
    world.current_tick += 1;
    meter.observe(&world);
    let row = meter.finish();
    assert_eq!(row.first_contact, Some(11), "{row:?}");
    assert_eq!(row.fill_time, Some(20), "{row:?}");
    assert_eq!(row.fill_max, u32::try_from(wanted)?);
    Ok(())
}

/// W0-T7: the lanes answer holds only when each gap passes at least a quarter of the group.
#[test]
fn lanes_answer_requires_both_gaps() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::Lanes, 6)?;
    let Some(KnownAnswer::Lanes { wall, gaps, .. }) = scene.known else {
        return Err("a lanes scene names its gaps".into());
    };
    let walkers: Vec<EntityId> = spawns_where(&scene, |r| matches!(r, Role::GroupMove { .. }))
        .into_iter()
        .map(|i| ids[i])
        .collect();
    meter.observe(&world);
    let x_before = f64::from(wall) - 1.0;
    let x_after = f64::from(wall) + 2.0;
    let lane_y = |lane: usize| f64::from(gaps[lane].0) + 1.5;
    // Everyone through the first gap.
    for &id in &walkers {
        place(&mut world, id, x_before, lane_y(0))?;
    }
    meter.observe(&world);
    for &id in &walkers {
        place(&mut world, id, x_after, lane_y(0))?;
    }
    meter.observe(&world);
    let row = meter.finish();
    let group = u32::try_from(walkers.len())?;
    assert_eq!(row.lanes, LaneUse([group, 0]), "{row:?}");
    let verdict = judge(scene.known.ok_or("known")?, &ScenarioRow::default(), &row);
    assert!(verdict.failed.contains(&"lanes"), "{verdict:?}");
    // Half through each gap uses both.
    let both = KnownRow {
        lanes: LaneUse([group / 2, group - group / 2]),
        ..row
    };
    let verdict = judge(scene.known.ok_or("known")?, &ScenarioRow::default(), &both);
    assert!(!verdict.failed.contains(&"lanes"), "{verdict:?}");
    Ok(())
}

/// The deep-queue answer reads friend overlap per rank: an overlap between two units of the
/// second rank counts in rank 1's columns and in no other.
#[test]
fn deep_queue_overlap_is_reported_per_rank() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::DeepQueue, 7)?;
    let Some(KnownAnswer::DeepQueue { ranks, .. }) = scene.known else {
        return Err("a deep-queue scene names its ranks".into());
    };
    let attackers = spawns_where(&scene, |r| *r == Role::Attack);
    // The second rank: the second-highest spawn x among the attackers.
    let mut columns: Vec<Lattice> = attackers.iter().map(|&i| scene.spawns[i].at.0).collect();
    columns.sort_unstable();
    columns.dedup();
    let second = columns[columns.len() - 2];
    let pair: Vec<usize> = attackers
        .iter()
        .copied()
        .filter(|&i| scene.spawns[i].at.0 == second)
        .take(2)
        .collect();
    if pair.len() < 2 {
        return Err("the scene's second rank holds two units".into());
    }
    meter.observe(&world);
    let (a, b) = (
        lattice_point(scene.spawns[pair[0]].at),
        lattice_point(scene.spawns[pair[1]].at),
    );
    // Move the second unit onto the first, half-way: a deep overlap.
    let mid = (f64::midpoint(a.0, b.0), f64::midpoint(a.1, b.1));
    place(&mut world, ids[pair[1]], mid.0, mid.1)?;
    meter.observe(&world);
    let row = meter.finish();
    assert_eq!(row.rank_fo_max.len(), usize::try_from(ranks)?);
    assert!(row.rank_fo_max[1] > 0.2, "{row:?}");
    for (rank, max) in row.rank_fo_max.iter().enumerate() {
        if rank != 1 {
            assert!(*max <= 0.0, "rank {rank}: {row:?}");
        }
    }
    Ok(())
}

/// The hallway answer counts a pair that changes the side it passes on.
#[test]
fn hallway_side_changes_count_a_pair_that_swaps_sides() -> TestResult {
    let (scene, mut world, ids, mut meter) = known_scene(Family::Hallway, 1)?;
    let east = ids[*spawns_where(
        &scene,
        |r| matches!(r, Role::OwnMove { goal } if goal.0 > 32 * 64),
    )
    .first()
    .ok_or("an east walker")?];
    let west = ids[*spawns_where(
        &scene,
        |r| matches!(r, Role::OwnMove { goal } if goal.0 < 32 * 64),
    )
    .first()
    .ok_or("a west walker")?];
    meter.observe(&world);
    // Head to head at x = 32, the east walker first above, then below, then above the other.
    for (dx, side) in [(0.2, 0.3), (0.1, -0.3), (0.0, 0.3), (-0.1, 0.3)] {
        place(&mut world, east, 32.0 + dx, 8.0 + side)?;
        place(&mut world, west, 32.0 - dx, 8.0)?;
        meter.observe(&world);
    }
    let row = meter.finish();
    assert_eq!(row.side_changes, 2, "{row:?}");
    let verdict = judge(scene.known.ok_or("known")?, &ScenarioRow::default(), &row);
    assert!(verdict.failed.contains(&"side_changes"), "{verdict:?}");
    Ok(())
}

/// W0-T11: five seeds of every new family play without a simulation diagnostic and give the
/// same CSV whatever the number of threads; their rows carry the known-answer columns.
#[test]
fn five_seeds_of_each_new_family_play_without_a_diagnostic_and_repeat() -> TestResult {
    for family in Family::NEW {
        let first = run_range(family, (1, 5), false, 2, &RunOptions::default())?;
        let again = run_range(family, (1, 5), false, 3, &RunOptions::default())?;
        assert_eq!(first, again, "{family:?}");
        let rows = parse_csv(&first);
        assert_eq!(rows.len(), 5, "{family:?}");
        for row in rows {
            assert_eq!(
                row.get("diagnostics").map(String::as_str),
                Some("0"),
                "{family:?} {row:?}"
            );
            assert!(
                row.contains_key("answer") && row.contains_key("hard"),
                "{row:?}"
            );
        }
    }
    Ok(())
}

/// The old families' CSV keeps its header: the new columns belong to the new families only, so
/// a run of an old family is byte-identical to one written before them.
#[test]
fn old_families_keep_their_csv_columns() -> TestResult {
    let text = run_range(Family::Cross, (1, 1), false, 1, &RunOptions::default())?;
    let header = text.lines().next().ok_or("a header")?;
    assert_eq!(header, ScenarioRow::HEADER);
    Ok(())
}

/// The permutation mode: at offset 0 the two plays are the same play, so no tick differs; a row
/// names the family, seed and offset and classifies what it found.
#[test]
fn the_permutation_mode_finds_nothing_at_offset_zero() -> TestResult {
    let rows: Vec<PermutationRow> =
        permutation_range(Family::Arc, (1, 2), 0, 2, &RunOptions::default())?;
    assert_eq!(rows.len(), 2);
    for row in &rows {
        assert_eq!(
            (row.offset, row.first_difference, row.differing_unit_ticks),
            (0, None, 0)
        );
        assert_eq!(row.class(), "identical");
    }
    let text = sim_fixtures::harness::permutation_csv(&rows);
    assert!(text.starts_with(PermutationRow::HEADER), "{text}");
    assert_eq!(parse_csv(&text).len(), 2);
    Ok(())
}
