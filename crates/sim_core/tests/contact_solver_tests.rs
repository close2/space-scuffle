//! # Integration Test: The Contact Solver's Data and Shares (plan `2026-11-20-contact-solver` WP2)
//!
//! Pins, through public APIs, what the solver pipeline will rest on:
//! - the iteration count in the shipped `movement_rules.ron` is at least C-3's bound
//!   `⌈ln(δ_move / ρ) / ln(5/6)⌉`, `ρ` recomputed from the shipped catalog as the largest
//!   `(s_a + s_b) / (r_a + r_b)` over pairs of mobile ground archetypes, `s` the top speed times the
//!   archetype's largest speed multiplier (Overclock, attack dash), so a faster unit or a smaller
//!   radius in the catalog fails this test until the data follows;
//! - `contact_shares` answers C-2's table for the public cases;
//! - the shipped file runs the legacy pipeline until the plan's switch-over (WP6).
//!
//! And E-2 of plan `2026-11-25-enemy-contact-and-ring-retarget` (B-193: enemies block, never
//! push; every step a body takes in the tick closes on an enemy by at most its share of the gap
//! between them):
//! - a crowd of melee attackers pressing from one side never moves the standing enemy it fights
//!   (E2-T1), mirrored and relabelled alike;
//! - a walker stops at a standing enemy's surface (E2-T2), and one whose path clips an enemy
//!   slides past it (E2-T3), the enemy unmoved in both;
//! - two enemies walking head-on meet at contact, sharing the gap by their speeds (E2-T4);
//! - an idle body pushed by a friend into a standing enemy stops at its surface (E2-T5);
//! - with `contact.enemies_block` off the simulation is WP1's, bit for bit (E2-T8).
//!
//! The scenes use scratch archetypes (C5) and derive their sizes from the radii and the ring
//! capacity `C = ⌊π / asin(r / (R + r + range))⌋` of plan `2026-11-20-contact-solver` §4. Floats
//! appear only in this test's arithmetic on catalog values and positions (charter §3.1).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use sim_core::catalog::{ContactRules, MovementPipeline, MovementRules};
use sim_core::movement::solver::{contact_shares, ContactParty, PairShares};
use sim_core::{SimWorld, UnitCatalog};
use sim_fixtures::harness::corpus::{Lattice, ScratchArchetype, LATTICE};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn the_shipped_iteration_count_meets_the_catalogs_bound() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let rules = catalogs.units.movement_rules().contact();
    let ground: Vec<(f64, f64, String)> = catalogs
        .units
        .archetypes()
        .filter(|a| a.mover_type == MoverType::Ground && !a.is_immobile_by_default())
        .map(|a| {
            let multiplier = [
                a.overclock.map(|o| o.speed_multiplier),
                a.capabilities.attack_dash.map(|d| d.speed_multiplier),
            ]
            .into_iter()
            .flatten()
            .map(|m| f64::from(m.to_f32()))
            .fold(1.0_f64, f64::max);
            (
                f64::from(a.max_speed.to_f32()) * multiplier,
                f64::from(a.collision_radius.to_f32()),
                a.slug.clone(),
            )
        })
        .collect();
    let mut worst = (0.0_f64, String::new());
    for (i, a) in ground.iter().enumerate() {
        for b in &ground[i..] {
            let rho = (a.0 + b.0) / (a.1 + b.1);
            if rho > worst.0 {
                worst = (rho, format!("{} / {}", a.2, b.2));
            }
        }
    }
    let moving = f64::from(rules.moving_overlap_permille) / 1000.0;
    let bound = ((moving / worst.0).ln() / (5.0_f64 / 6.0).ln()).ceil();
    assert!(
        f64::from(rules.iterations) >= bound,
        "iterations {} below the bound {bound} of ρ {:.3} ({})",
        rules.iterations,
        worst.0,
        worst.1
    );
}

/// WP6: the shipped rules run the solver pipeline, with D-β (Q0092) and D-δ (Q0095) on.
#[test]
fn the_shipped_rules_run_the_solver_pipeline_since_the_switch_over() {
    let rules = sim_fixtures::shipped_catalogs()
        .units
        .movement_rules()
        .contact();
    assert_eq!(rules.pipeline, MovementPipeline::Solver);
    assert!(rules.moving_friends_push, "D-β is on by default (Q0092)");
    assert!(
        rules.nudge_engaged_along_ring,
        "D-δ is on by default (Q0095)"
    );
}

#[test]
fn contact_shares_answer_the_push_table() {
    let base = ContactParty {
        owner: PlayerId(0),
        push_priority: 10,
        fixed: false,
        holding: false,
        engaged: false,
        idle: false,
        moving: true,
        nudge: None,
    };
    let (zero, half, one) = (SimScalar::ZERO, SimScalar::HALF, SimScalar::ONE);
    let enemy = ContactParty {
        owner: PlayerId(1),
        ..base
    };
    let idle = ContactParty {
        idle: true,
        moving: false,
        ..base
    };
    let holding = ContactParty {
        holding: true,
        moving: false,
        ..base
    };
    assert_eq!(
        contact_shares(&base, &enemy, true),
        PairShares::Weights(half, half)
    );
    assert_eq!(
        contact_shares(&base, &idle, true),
        PairShares::Weights(zero, one)
    );
    assert_eq!(
        contact_shares(&base, &holding, true),
        PairShares::Weights(one, zero)
    );
    assert_eq!(contact_shares(&base, &base, false), PairShares::ByApproach);
}

const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The melee attackers' and walkers' radius, sixty-fourths (0.375).
const SMALL: Lattice = 24;
/// The standing bodies' radius, sixty-fourths (0.75).
const LARGE: Lattice = 48;
/// The attackers' weapon range, world units: melee.
const MELEE: f64 = 0.1;
/// The melee attackers' top speed, world units per tick: a step longer than the melee gap
/// (`MELEE`), as the shipped melee chasers' (0.165 for the fastest), so a committed pursuit step
/// can overshoot into the target's disc, B-193's first way in (plan
/// `2026-11-24-solver-retarget-and-shoving` §1.2).
const ATTACKER_SPEED: f64 = 0.1875;
/// The scene's centre, world units.
const CENTRE: (f64, f64) = (32.0, 32.0);

const ATTACKER: u16 = 1;
const FIGHTER: u16 = 2;
const DUMMY: u16 = 3;
const WALKER: u16 = 4;
const POST: u16 = 5;
/// A walker twice as fast as [`WALKER`].
const RUNNER: u16 = 6;
/// An unarmed walker of radius one, as wide as a corridor of two cells.
const BLOCK: u16 = 7;

/// How far, world units, two bodies may still overlap where E-2 allows none: one raw unit of
/// Q32.32 for each of the solver's applications in a tick (at most the shipped 21 averaged
/// iterations, two summed passes and application zero), rounded up to 32.
const ROUNDING: f64 = 32.0 / 4_294_967_296.0;

fn world_units(lattice: Lattice) -> f64 {
    lattice as f64 / LATTICE as f64
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn xy(v: SimVec2) -> (f64, f64) {
    (f64::from(v.x.to_f32()), f64::from(v.y.to_f32()))
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

/// The exact centre distance of two positions, world units, from the raw Q32.32 values.
fn exact_distance(a: SimVec2, b: SimVec2) -> f64 {
    let dx = (a.x.raw() - b.x.raw()) as f64 / 4_294_967_296.0;
    let dy = (a.y.raw() - b.y.raw()) as f64 / 4_294_967_296.0;
    dx.hypot(dy)
}

/// The scratch catalog: a melee attacker (top speed [`ATTACKER_SPEED`]), an armed standing
/// fighter (range 3), an unarmed immobile dummy (it cannot flee its attacker), an unarmed walker
/// (0.125), an unarmed post, an unarmed runner (twice the walker's speed) and an unarmed block of
/// radius one (as wide as a corridor of two cells), all
/// mortal only in name (a million hit points).
fn scratch_catalog(rules: MovementRules) -> Result<UnitCatalog, Box<dyn std::error::Error>> {
    let factions = sim_fixtures::shipped_factions();
    let faction = factions
        .factions()
        .next()
        .map(|f| f.slug.clone())
        .ok_or("a faction")?;
    let kind = |radius, range| ScratchArchetype {
        radius,
        speed: "0.125",
        range,
        priority: 10,
    };
    let entries = [
        ScratchArchetype {
            speed: "0.1875",
            ..kind(SMALL, Some("0.1"))
        }
        .ron("e2_attacker", ATTACKER, &faction),
        kind(LARGE, Some("3.0")).ron("e2_fighter", FIGHTER, &faction),
        ScratchArchetype {
            speed: "0",
            ..kind(SMALL, None)
        }
        .ron("e2_dummy", DUMMY, &faction),
        kind(SMALL, None).ron("e2_walker", WALKER, &faction),
        kind(LARGE, None).ron("e2_post", POST, &faction),
        ScratchArchetype {
            speed: "0.25",
            ..kind(SMALL, None)
        }
        .ron("e2_runner", RUNNER, &faction),
        kind(LATTICE, None).ron("e2_block", BLOCK, &faction),
    ];
    let mut catalog =
        UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", entries.join(",")))?;
    catalog.set_movement_rules(rules);
    Ok(catalog)
}

/// The shipped movement rules on the solver pipeline.
fn solver_rules() -> Result<MovementRules, Box<dyn std::error::Error>> {
    let rules = sim_fixtures::harness::shipped_rules();
    Ok(rules.with_contact(ContactRules {
        pipeline: MovementPipeline::Solver,
        ..rules.contact()
    })?)
}

/// An empty flat 64 × 64 world on the scratch catalog.
fn scratch_world(rules: MovementRules, seed: u64) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::try_world(map, seed)?;
    world.catalog = scratch_catalog(rules)?;
    Ok(world)
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

fn order(player: PlayerId, units: &[EntityId], order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities: units.to_vec(),
        order,
        queue: false,
    }
}

fn position(world: &SimWorld, id: EntityId) -> Result<SimVec2, Box<dyn std::error::Error>> {
    Ok(world.entities.get(&id).ok_or("a unit")?.position)
}

/// How a scene is laid out.
#[derive(Clone, Copy, Debug)]
struct Layout {
    /// Reflected through the scene's centre (attackers from the east).
    mirrored: bool,
    /// Units spawned in reverse order, so their ids run the other way.
    relabelled: bool,
}

const LAYOUTS: [Layout; 3] = [
    Layout {
        mirrored: false,
        relabelled: false,
    },
    Layout {
        mirrored: true,
        relabelled: false,
    },
    Layout {
        mirrored: false,
        relabelled: true,
    },
];

/// `p` reflected through the scene's centre when mirrored.
fn place(layout: Layout, p: (f64, f64)) -> (f64, f64) {
    if layout.mirrored {
        (2.0 * CENTRE.0 - p.0, 2.0 * CENTRE.1 - p.1)
    } else {
        p
    }
}

fn at_place(layout: Layout, p: (f64, f64)) -> SimVec2 {
    let (x, y) = place(layout, p);
    at(x, y)
}

/// The E2-T1 scene: the world, the standing fighter, and the ticks the check runs.
struct PressScene {
    world: SimWorld,
    fighter: EntityId,
    ticks: u32,
}

/// A standing fighter, an unkillable dummy it attacks in reach on the far side, and `N = 2C`
/// melee attackers in three ranks attack-moving into it; the orders' tick is stepped. The check
/// runs twice the time the rear rank needs to reach the fighter's ring.
fn press_scene(
    rules: MovementRules,
    layout: Layout,
) -> Result<PressScene, Box<dyn std::error::Error>> {
    let (r, big) = (world_units(SMALL), world_units(LARGE));
    let band = big + r + MELEE;
    let capacity = (std::f64::consts::PI / (r / band).asin()).floor() as usize;
    let count = 2 * capacity;
    let mut world = scratch_world(rules, 0x0E1_0001)?;
    let fighter = world.spawn_unit(UnitTypeId(FIGHTER), RED, at_place(layout, CENTRE))?;
    let dummy = world.spawn_unit(
        UnitTypeId(DUMMY),
        BLUE,
        at_place(layout, (CENTRE.0 + 2.5, CENTRE.1)),
    )?;
    let spacing = 2.0 * r + 0.05;
    let ranks = 3;
    let per_rank = count.div_ceil(ranks);
    let front = CENTRE.0 - 6.0;
    let mut places: Vec<(f64, f64)> = (0..count)
        .map(|index| {
            let (rank, file) = (index / per_rank, index % per_rank);
            let y = CENTRE.1 + spacing * (file as f64 - (per_rank as f64 - 1.0) / 2.0);
            place(layout, (front - spacing * rank as f64, y))
        })
        .collect();
    if layout.relabelled {
        places.reverse();
    }
    let mut attackers = Vec::with_capacity(count);
    for (x, y) in places {
        attackers.push(world.spawn_unit(UnitTypeId(ATTACKER), BLUE, at(x, y))?);
    }
    let goal = place(layout, (CENTRE.0 + 10.0, CENTRE.1));
    step(
        &mut world,
        vec![
            order(
                RED,
                &[fighter],
                UnitOrder::AttackTarget {
                    target: dummy,
                    origin: AttackOrigin::Explicit,
                },
            ),
            order(
                BLUE,
                &attackers,
                UnitOrder::AttackMove {
                    target: at(goal.0, goal.1),
                },
            ),
        ],
    );
    let depth = spacing * (ranks as f64 - 1.0);
    let travel = (CENTRE.0 - front) + depth - band;
    Ok(PressScene {
        world,
        fighter,
        ticks: (2.0 * travel / ATTACKER_SPEED).ceil() as u32,
    })
}

/// E2-T1 (plan `2026-11-25-enemy-contact-and-ring-retarget` §8 WP2): a standing, armed,
/// not-holding fighter attacking a target in reach is never moved by the melee crowd that presses
/// into it from one side, twice the ring's capacity in three ranks, over twice the rear rank's
/// arrival time; mirrored and relabelled alike. Its D-SHOVE-E is therefore 0, and the meter
/// confirms it.
#[test]
fn a_pressing_crowd_does_not_move_a_standing_enemy() -> TestResult {
    use sim_fixtures::trace::contact_checks::ContactMeter;
    for layout in LAYOUTS {
        let mut scene = press_scene(solver_rules()?, layout)?;
        let start = position(&scene.world, scene.fighter)?;
        let mut meter = ContactMeter::new();
        meter.observe(&scene.world);
        let mut touched = 0_u32;
        for tick in 0..scene.ticks {
            step(&mut scene.world, vec![]);
            meter.observe(&scene.world);
            let now = position(&scene.world, scene.fighter)?;
            assert_eq!(
                now,
                start,
                "{layout:?}: the fighter moved by {:.4} at tick {tick}",
                dist(xy(now), xy(start))
            );
            let band = world_units(SMALL) + world_units(LARGE) + MELEE + 1.0 / 64.0;
            touched += u32::from(scene.world.entities.values().any(|e| {
                e.owner == BLUE
                    && e.unit_type_id == UnitTypeId(ATTACKER)
                    && dist(xy(e.position), xy(start)) <= band
            }));
        }
        assert!(
            touched > scene.ticks / 4,
            "{layout:?}: the crowd reached the fighter on {touched} ticks"
        );
        let shove = meter.total().sides[1].shove_e;
        assert!(shove.abs() < 1e-12, "{layout:?}: D-SHOVE-E {shove}");
    }
    Ok(())
}

/// The row of the corridor scenes: the only walkable cells, `y` in `[31, 32)`.
const CORRIDOR_ROW: u32 = 31;

/// A 64 × 64 world on the scratch catalog whose only walkable cells are the `rows` rows from the
/// corridor row up.
fn corridor_world(seed: u64, rows: u32) -> Result<SimWorld, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let walkable = CORRIDOR_ROW..CORRIDOR_ROW + rows;
    for y in (0..64).filter(|y| !walkable.contains(y)) {
        for x in 0..64 {
            map.cell_mut(x, y)?.flags = sim_map::CellFlags::empty();
        }
    }
    let mut world = sim_fixtures::try_world(map, seed)?;
    world.catalog = scratch_catalog(solver_rules()?)?;
    Ok(world)
}

/// The corridor's centre line, world units.
fn row() -> f64 {
    f64::from(CORRIDOR_ROW) + 0.5
}

/// E2-T2: a walker on `Move` down a corridor one cell wide, straight at a standing (idle,
/// unarmed) enemy post in its way, never moves the post, never overlaps it by more than
/// [`ROUNDING`], and ends touching it (within the standing drift, 1/64).
#[test]
fn a_walker_stops_at_a_standing_enemy() -> TestResult {
    let mut world = corridor_world(0x0E2_0002, 1)?;
    let post = world.spawn_unit(UnitTypeId(POST), RED, at(CENTRE.0, row()))?;
    let walker = world.spawn_unit(UnitTypeId(WALKER), BLUE, at(CENTRE.0 - 4.0, row()))?;
    let start = position(&world, post)?;
    let sum = world_units(SMALL) + world_units(LARGE);
    step(
        &mut world,
        vec![order(
            BLUE,
            &[walker],
            UnitOrder::Move {
                target: at(CENTRE.0 + 12.0, row()),
            },
        )],
    );
    for tick in 0..100 {
        step(&mut world, vec![]);
        assert_eq!(
            position(&world, post)?,
            start,
            "the post moved at tick {tick}"
        );
        let gap = exact_distance(position(&world, walker)?, start) - sum;
        assert!(gap >= -ROUNDING, "tick {tick}: overlap {}", -gap);
    }
    let gap = exact_distance(position(&world, walker)?, start) - sum;
    assert!(
        gap <= 1.0 / 64.0,
        "the walker stopped {gap} short of the post"
    );
    Ok(())
}

/// E2-T3: a walker on `Move` due east that starts touching a standing enemy post on its
/// south-east (its centre a third of the radius sum above the post's, so its path clips the
/// post's disc and its first step already closes on it) ends beyond the post, having kept the
/// tangential part of its steps, and the post never moves.
#[test]
fn a_grazing_walker_slides_along_an_enemy() -> TestResult {
    let mut world = scratch_world(solver_rules()?, 0x0E2_0003)?;
    let sum = world_units(SMALL) + world_units(LARGE);
    let post = world.spawn_unit(UnitTypeId(POST), RED, at(CENTRE.0, CENTRE.1))?;
    let lane = CENTRE.1 + sum / 3.0;
    let west = (sum * sum - (sum / 3.0) * (sum / 3.0)).sqrt();
    let walker = world.spawn_unit(UnitTypeId(WALKER), BLUE, at(CENTRE.0 - west, lane))?;
    let start = position(&world, post)?;
    step(
        &mut world,
        vec![order(
            BLUE,
            &[walker],
            UnitOrder::Move {
                target: at(CENTRE.0 + 6.0, lane),
            },
        )],
    );
    for tick in 0..100 {
        step(&mut world, vec![]);
        assert_eq!(
            position(&world, post)?,
            start,
            "the post moved at tick {tick}"
        );
        let gap = exact_distance(position(&world, walker)?, start) - sum;
        assert!(gap >= -ROUNDING, "tick {tick}: overlap {}", -gap);
    }
    let end = xy(position(&world, walker)?);
    assert!(
        end.0 > CENTRE.0 + sum,
        "the walker did not get past: {end:?}"
    );
    Ok(())
}

/// The E2-T4 scene: a blue walker and a red mover of kind `red_kind` in a corridor one cell
/// wide, `gap` world units of free corridor between their discs, walking head-on. Returns the
/// world, both units and their start positions.
fn head_on(
    red_kind: u16,
    gap: f64,
    mirrored: bool,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
    let mut world = corridor_world(0x0E2_0004, 1)?;
    let r = world_units(SMALL);
    let half = r + gap / 2.0;
    let (west, east) = (at(CENTRE.0 - half, row()), at(CENTRE.0 + half, row()));
    let (blue_at, red_at) = if mirrored { (east, west) } else { (west, east) };
    let blue = world.spawn_unit(UnitTypeId(WALKER), BLUE, blue_at)?;
    let red = world.spawn_unit(UnitTypeId(red_kind), RED, red_at)?;
    let toward = |from: SimVec2, to: SimVec2| {
        let (f, t) = (xy(from), xy(to));
        at(f.0 + 12.0 * (t.0 - f.0).signum(), row())
    };
    step(
        &mut world,
        vec![
            order(
                BLUE,
                &[blue],
                UnitOrder::Move {
                    target: toward(blue_at, red_at),
                },
            ),
            order(
                RED,
                &[red],
                UnitOrder::Move {
                    target: toward(red_at, blue_at),
                },
            ),
        ],
    );
    Ok((world, blue, red))
}

/// E2-T4: two enemies walking head-on down a corridor meet at contact (overlap within
/// [`ROUNDING`] on every tick, touching at the end), at equal speeds and at 2:1; at equal speeds
/// they cover equal distances, and the mirrored scene (sides swapped) gives the same distances.
#[test]
fn two_walkers_meet_at_contact() -> TestResult {
    let sum = 2.0 * world_units(SMALL);
    let gap = 3.0;
    let run = |red_kind, mirrored| -> Result<(f64, f64), Box<dyn std::error::Error>> {
        let (mut world, blue, red) = head_on(red_kind, gap, mirrored)?;
        let (b0, r0) = (position(&world, blue)?, position(&world, red)?);
        for tick in 0..120 {
            step(&mut world, vec![]);
            let d = exact_distance(position(&world, blue)?, position(&world, red)?);
            assert!(d - sum >= -ROUNDING, "tick {tick}: overlap {}", sum - d);
        }
        let (b, r) = (position(&world, blue)?, position(&world, red)?);
        let d = exact_distance(b, r);
        assert!(d - sum <= 1.0 / 64.0, "they stopped {} apart", d - sum);
        Ok((exact_distance(b, b0), exact_distance(r, r0)))
    };
    let (blue, red) = run(WALKER, false)?;
    assert!(
        (blue - red).abs() <= 1.0 / 1024.0,
        "equal speeds: {blue} and {red}"
    );
    let (blue_m, red_m) = run(WALKER, true)?;
    assert!(
        (blue_m - blue).abs() <= 1.0 / 1024.0 && (red_m - red).abs() <= 1.0 / 1024.0,
        "mirrored: {blue_m}, {red_m} against {blue}, {red}"
    );
    // At 2:1 they meet at contact too; how the gap is shared is pinned in the solver's
    // `two_walkers_share_the_gap_by_their_walks`, since steering slows the slower walker first.
    run(RUNNER, false)?;
    Ok(())
}

/// E2-T5: in a corridor two cells wide, a walker as wide as the corridor on `Move` pushes an idle
/// friend of the same width ahead of it (neither can pass or step aside) into a standing enemy
/// post: the friend stops at the post's surface (overlap within [`ROUNDING`] on every tick,
/// touching at the end), the post never moves, and the friends' overlap at the end is within
/// D-α's moving bound.
#[test]
#[ignore = "fails under the plan's design: the friend stops at the post, but the walker keeps \
            walking into it (C-2 gives a moving body no share against an idle friend) up to 90 % \
            overlap; evidence 2026-11-25-enemy-contact-and-ring-retarget §5"]
fn a_body_pressed_by_friends_stops_at_an_enemy() -> TestResult {
    let mut world = corridor_world(0x0E2_0005, 2)?;
    let (big, block) = (world_units(LARGE), 1.0);
    let centre = f64::from(CORRIDOR_ROW) + 1.0;
    let post = world.spawn_unit(UnitTypeId(POST), RED, at(CENTRE.0, centre))?;
    let idle = world.spawn_unit(
        UnitTypeId(BLOCK),
        BLUE,
        at(CENTRE.0 - big - block - 1.0, centre),
    )?;
    let walker = world.spawn_unit(
        UnitTypeId(BLOCK),
        BLUE,
        at(CENTRE.0 - big - 3.0 * block - 2.0, centre),
    )?;
    let start = position(&world, post)?;
    step(
        &mut world,
        vec![order(
            BLUE,
            &[walker],
            UnitOrder::Move {
                target: at(CENTRE.0 + 12.0, centre),
            },
        )],
    );
    for tick in 0..150 {
        step(&mut world, vec![]);
        assert_eq!(
            position(&world, post)?,
            start,
            "the post moved at tick {tick}"
        );
        let gap = exact_distance(position(&world, idle)?, start) - (big + block);
        assert!(gap >= -ROUNDING, "tick {tick}: overlap {}", -gap);
    }
    let gap = exact_distance(position(&world, idle)?, start) - (big + block);
    assert!(
        gap <= 1.0 / 64.0,
        "the idle friend stopped {gap} short of the post"
    );
    let friends = exact_distance(position(&world, idle)?, position(&world, walker)?);
    let moving = f64::from(solver_rules()?.contact().moving_overlap_permille) / 1000.0;
    let overlap = (2.0 * block - friends) / (2.0 * block);
    assert!(overlap <= moving, "friend overlap {overlap}");
    Ok(())
}

/// FNV-1a over a list of per-tick checksums: one number for a whole run.
fn digest(checksums: &[u64]) -> u64 {
    checksums
        .iter()
        .fold(0xcbf2_9ce4_8422_2325, |hash, &value| {
            value.to_le_bytes().iter().fold(hash, |h, &byte| {
                (h ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
            })
        })
}

/// The E2-T1 scene (plain layout) under `rules`: the per-tick checksums' digest.
fn press_digest(rules: MovementRules) -> Result<u64, Box<dyn std::error::Error>> {
    let mut scene = press_scene(rules, LAYOUTS[0])?;
    let mut checksums = vec![scene.world.compute_checksum()];
    for _ in 0..scene.ticks {
        step(&mut scene.world, vec![]);
        checksums.push(scene.world.compute_checksum());
    }
    Ok(digest(&checksums))
}

/// Battle 28 of the matchup matrix (zerglings against immortals, where B-193 was measured) on its
/// first seed of record under `rules`: the per-tick checksums' digest.
fn battle_digest(rules: MovementRules) -> Result<u64, Box<dyn std::error::Error>> {
    use sim_fixtures::matchup::{BattleRun, Matrix, MATRIX_RON};
    let matrix = Matrix::from_ron_str(MATRIX_RON)?;
    let battle = matrix.battle("28_100_lings_vs_10_immortals")?;
    let seed = *matrix.seeds.first().ok_or("a seed of record")?;
    let mut catalogs = sim_fixtures::shipped_catalogs();
    catalogs.units.set_movement_rules(rules);
    let mut run = BattleRun::new(battle, catalogs, seed, matrix.jitter_for(seed))?;
    let mut checksums = vec![run.world.compute_checksum()];
    while !run.is_finished() {
        assert!(run.step().is_empty(), "no diagnostic");
        checksums.push(run.world.compute_checksum());
    }
    Ok(digest(&checksums))
}

/// Prints the two digests of E2-T8 under the shipped rules (run by hand with `--ignored` and
/// `--nocapture` on the tree whose digests E2-T8 pins).
#[test]
#[ignore = "prints digests, run by hand"]
fn print_the_block_digests() -> TestResult {
    let press = press_digest(solver_rules()?)?;
    let battle = battle_digest(sim_fixtures::harness::shipped_rules())?;
    println!("press digest {press:#018x}, battle digest {battle:#018x}");
    Ok(())
}

/// `rules` with E-2 off: new enemy contact split evenly, as on WP1's tree.
fn without_block(rules: MovementRules) -> Result<MovementRules, Box<dyn std::error::Error>> {
    Ok(rules.with_contact(ContactRules {
        enemies_block: false,
        ..rules.contact()
    })?)
}

/// E2-T8: with `contact.enemies_block` off, the simulation is WP1's (the solver with K-1, before
/// E-2), bit for bit, on the E2-T1 scene and on battle 28's first seed of record: their per-tick
/// checksum digests equal WP1's, pinned below. E-2 adds no hashed state.
#[test]
fn the_block_switch_off_is_bit_identical() -> TestResult {
    let press = press_digest(without_block(solver_rules()?)?)?;
    let battle = battle_digest(without_block(sim_fixtures::harness::shipped_rules())?)?;
    assert_eq!(
        press, PRESS_DIGEST_WP1,
        "the E2-T1 scene, switch off: {press:#018x}"
    );
    assert_eq!(
        battle, BATTLE_DIGEST_WP1,
        "battle 28, switch off: {battle:#018x}"
    );
    Ok(())
}

/// WP1's digest of the E2-T1 scene (`print_the_block_digests` on WP1's commit `1ccccb00` merged
/// with `main` `802df056`, schema 47).
const PRESS_DIGEST_WP1: u64 = 0x7e1c_ffb0_addd_e4de;
/// WP1's digest of battle 28's first seed of record.
const BATTLE_DIGEST_WP1: u64 = 0x3957_40dd_8944_647d;
