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
//! And E-1 of plan `2026-11-24-solver-retarget-and-shoving` (B-193: enemies never push each
//! other; each body of an enemy pair takes the share of the gap its own displacement closed):
//! - a crowd of melee attackers pressing from one side never moves the standing enemy it fights
//!   (E-T1), mirrored and relabelled alike;
//! - a unit on `Move` inside a closed ring of standing enemies is trapped, and no ring body moves
//!   (E-T2);
//! - two advancing lines that meet head-on share their contact evenly, the scene staying
//!   point-symmetric, and a line that stops is not pushed by one still walking
//!   (E-T3).
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
        contact_shares(&base, &enemy, true, true),
        PairShares::Weights(half, half)
    );
    assert_eq!(
        contact_shares(&base, &idle, true, false),
        PairShares::Weights(zero, one)
    );
    assert_eq!(
        contact_shares(&base, &holding, true, false),
        PairShares::Weights(one, zero)
    );
    assert_eq!(
        contact_shares(&base, &base, false, false),
        PairShares::ByApproach
    );
    assert_eq!(
        contact_shares(&base, &enemy, true, false),
        PairShares::EnemyByClosing
    );
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
/// `2026-11-24-solver-retarget-and-shoving` §1.2). At the other scratch archetypes' 0.125 the
/// attackers stop 0.1 short of the disc and nothing presses the target (As built, WP1).
const ATTACKER_SPEED: f64 = 0.1875;
/// The scene's centre, world units.
const CENTRE: (f64, f64) = (32.0, 32.0);

const ATTACKER: u16 = 1;
const FIGHTER: u16 = 2;
const DUMMY: u16 = 3;
const WALKER: u16 = 4;
const POST: u16 = 5;

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

/// The scratch catalog: a melee attacker (top speed [`ATTACKER_SPEED`]; the others 0.125), an
/// armed standing fighter (range 3), an unarmed immobile dummy (it cannot flee its attacker), an unarmed walker, and an unarmed post, all mortal only in name (a million hit points).
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
        .ron("e1_attacker", ATTACKER, &faction),
        kind(LARGE, Some("3.0")).ron("e1_fighter", FIGHTER, &faction),
        ScratchArchetype {
            speed: "0",
            ..kind(SMALL, None)
        }
        .ron("e1_dummy", DUMMY, &faction),
        kind(SMALL, None).ron("e1_walker", WALKER, &faction),
        kind(LARGE, None).ron("e1_post", POST, &faction),
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

fn position(world: &SimWorld, id: EntityId) -> Option<SimVec2> {
    world.entities.get(&id).map(|e| e.position)
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

/// The E-T1 scene: a standing fighter, an unkillable dummy it attacks in reach on the far side,
/// and `N = 2C` melee attackers in three ranks attack-moving into it; the orders' tick is
/// stepped. Returns the world, the fighter, and the ticks the check runs: twice the time the rear
/// rank needs to reach the fighter's ring.
struct PressScene {
    world: SimWorld,
    fighter: EntityId,
    ticks: u32,
}

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

fn at_place(layout: Layout, p: (f64, f64)) -> SimVec2 {
    let (x, y) = place(layout, p);
    at(x, y)
}

/// E-T1 (plan `2026-11-24-solver-retarget-and-shoving` §7 WP1): a standing, armed, not-holding
/// fighter attacking a target in reach is never moved by the melee crowd that presses into it
/// from one side, twice the ring's capacity in three ranks; mirrored and relabelled alike. Its
/// D-SHOVE is 0 because its position never changes.
#[test]
fn a_pressing_crowd_does_not_move_a_standing_enemy() -> TestResult {
    for layout in LAYOUTS {
        let mut scene = press_scene(solver_rules()?, layout)?;
        let start = position(&scene.world, scene.fighter).ok_or("the fighter")?;
        let mut touched = 0_u32;
        for tick in 0..scene.ticks {
            step(&mut scene.world, vec![]);
            let now = position(&scene.world, scene.fighter).ok_or("the fighter")?;
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
    }
    Ok(())
}

/// E-T2: a walker on `Move` from the centre of a closed ring of eight standing (idle, unarmed)
/// enemy posts, whose gaps are narrower than its diameter, does not get out, and no post moves.
#[test]
fn a_ringed_mover_is_trapped() -> TestResult {
    let (r, big) = (world_units(SMALL), world_units(LARGE));
    // Eight posts on a circle of radius D: neighbouring discs do not touch (chord ≥ 2R) and the
    // gap between them, chord − 2R, is less than the walker's diameter 2r.
    let posts_on = 8_u32;
    let half_angle = std::f64::consts::PI / f64::from(posts_on);
    let chord = 2.0 * big + r / 2.0;
    let radius = chord / (2.0 * half_angle.sin());
    assert!(chord - 2.0 * big < 2.0 * r && radius > big + r);
    for layout in LAYOUTS {
        let mut world = scratch_world(solver_rules()?, 0x0E1_0002)?;
        let mut posts = Vec::new();
        let angles: Vec<f64> = (0..posts_on)
            .map(|k| (2.0 * f64::from(k) + 1.0) * half_angle)
            .collect();
        let mut spawn_order: Vec<usize> = (0..angles.len()).collect();
        if layout.relabelled {
            spawn_order.reverse();
        }
        for index in spawn_order {
            let a = angles[index];
            let p = (CENTRE.0 + radius * a.cos(), CENTRE.1 + radius * a.sin());
            posts.push(world.spawn_unit(UnitTypeId(POST), RED, at_place(layout, p))?);
        }
        let walker = world.spawn_unit(UnitTypeId(WALKER), BLUE, at_place(layout, CENTRE))?;
        let starts: Vec<SimVec2> = posts
            .iter()
            .map(|&id| position(&world, id).ok_or("a post"))
            .collect::<Result<_, _>>()?;
        let goal = place(layout, (CENTRE.0 + 10.0, CENTRE.1));
        step(
            &mut world,
            vec![order(
                BLUE,
                &[walker],
                UnitOrder::Move {
                    target: at(goal.0, goal.1),
                },
            )],
        );
        for tick in 0..300 {
            step(&mut world, vec![]);
            for (&post, &start) in posts.iter().zip(&starts) {
                assert_eq!(
                    position(&world, post),
                    Some(start),
                    "{layout:?}: post {post:?} moved at tick {tick}"
                );
            }
            let at_walker = xy(position(&world, walker).ok_or("the walker")?);
            assert!(
                dist(at_walker, CENTRE) < radius,
                "{layout:?}: the walker left the ring at tick {tick}: {at_walker:?}"
            );
        }
    }
    Ok(())
}

/// How far, world units, a red walker of E-T3 may stand from its blue twin's reflection. Not
/// zero: the scene is not point-symmetric in its last bits even before the lines meet (the walk
/// from rest already differs by a few millionths), so the even share is pinned to well within
/// the standing drift instead (As built, WP1).
const SYMMETRY_TOLERANCE: f64 = 1.0 / 1024.0;

/// The row of the E-T3 corridor: the only walkable cells, `y` in `[31, 32)`.
const CORRIDOR_ROW: u32 = 31;

/// The point reflection of the E-T3 scene, exact in Q32.32: through (32, 31.5), the corridor's
/// centre.
fn reflect(v: SimVec2) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(64) - v.x, SimScalar::from_num(63) - v.y)
}

/// The E-T3 world, blue's walkers and red's.
type Lines = (SimWorld, Vec<EntityId>, Vec<EntityId>);

/// The E-T3 lines: three walkers a side in single file along a corridor one cell wide (they
/// cannot pass each other), one world unit apart, red's the point reflection of blue's. Blue
/// walks east and red west when `red_walks`; otherwise red stands, near enough that blue reaches
/// it at once.
fn line_scene(red_walks: bool) -> Result<Lines, Box<dyn std::error::Error>> {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    for y in (0..64).filter(|&y| y != CORRIDOR_ROW) {
        for x in 0..64 {
            map.cell_mut(x, y)?.flags = sim_map::CellFlags::empty();
        }
    }
    let mut world = sim_fixtures::try_world(map, 0x0E1_0003)?;
    world.catalog = scratch_catalog(solver_rules()?)?;
    let row = f64::from(CORRIDOR_ROW) + 0.5;
    let gap = if red_walks { 3.0 } else { 1.0 };
    let mut blue = Vec::new();
    let mut red = Vec::new();
    for k in 0..3 {
        let p = at(CENTRE.0 - gap - f64::from(k), row);
        blue.push(world.spawn_unit(UnitTypeId(WALKER), BLUE, p)?);
    }
    for &twin in &blue {
        let p = reflect(position(&world, twin).ok_or("blue")?);
        red.push(world.spawn_unit(UnitTypeId(WALKER), RED, p)?);
    }
    let mut commands = vec![order(
        BLUE,
        &blue,
        UnitOrder::Move {
            target: at(CENTRE.0 + 12.0, row),
        },
    )];
    if red_walks {
        commands.push(order(
            RED,
            &red,
            UnitOrder::Move {
                target: reflect(at(CENTRE.0 + 12.0, row)),
            },
        ));
    }
    step(&mut world, commands);
    Ok((world, blue, red))
}

/// E-T3: two lines of equal speed that meet head-on in a corridor share their contact evenly:
/// the scene stays the point reflection of itself, each red walker within [`SYMMETRY_TOLERANCE`]
/// of its blue twin's reflection. A line that stops is not pushed by one still walking: the standing red line
/// keeps every position while blue presses into it.
#[test]
fn two_advancing_lines_share_by_their_approach() -> TestResult {
    let (mut world, blue, red) = line_scene(true)?;
    let mut met = false;
    for tick in 0..150 {
        step(&mut world, vec![]);
        for (&b, &r) in blue.iter().zip(&red) {
            let (pb, pr) = (
                position(&world, b).ok_or("blue")?,
                position(&world, r).ok_or("red")?,
            );
            let off = dist(xy(reflect(pb)), xy(pr));
            assert!(
                off <= SYMMETRY_TOLERANCE,
                "tick {tick}: walkers {b:?} and {r:?} {off} off the reflection"
            );
            met |= dist(xy(pb), xy(pr)) < 2.0 * world_units(SMALL) + 0.01;
        }
    }
    assert!(met, "the lines met");
    let (mut world, blue, red) = line_scene(false)?;
    let starts: Vec<SimVec2> = red
        .iter()
        .map(|&id| position(&world, id).ok_or("red"))
        .collect::<Result<_, _>>()?;
    let mut pressed = false;
    for tick in 0..150 {
        step(&mut world, vec![]);
        for (&id, &start) in red.iter().zip(&starts) {
            assert_eq!(
                position(&world, id),
                Some(start),
                "red {id:?} moved at tick {tick}"
            );
        }
        pressed |= blue.iter().zip(&red).any(|(&b, &r)| {
            position(&world, b)
                .zip(position(&world, r))
                .is_some_and(|(pb, pr)| dist(xy(pb), xy(pr)) < 2.0 * world_units(SMALL) + 0.01)
        });
    }
    assert!(pressed, "blue reached the standing line");
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

/// `rules` with enemies splitting their overlap evenly, as before E-1, and G-6′ off (plan
/// `2026-11-24-solver-retarget-and-shoving` §4.2), so the base's behaviour.
fn even_split(rules: MovementRules) -> Result<MovementRules, Box<dyn std::error::Error>> {
    Ok(rules.with_contact(ContactRules {
        enemies_push_each_other: true,
        retarget_from_full_ring: false,
        ..rules.contact()
    })?)
}

/// The E-T1 scene (plain layout) under `rules`: the per-tick checksums' digest.
fn press_digest(rules: MovementRules) -> Result<u64, Box<dyn std::error::Error>> {
    let mut scene = press_scene(rules, LAYOUTS[0])?;
    let mut checksums = vec![scene.world.compute_checksum()];
    for _ in 0..scene.ticks {
        step(&mut scene.world, vec![]);
        checksums.push(scene.world.compute_checksum());
    }
    Ok(digest(&checksums))
}

/// Battle 28 of the matchup matrix (zerglings against immortals, the battle B-193 was measured
/// in) on its first seed of record under `rules`: the per-tick checksums' digest.
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

/// E-T4: with `contact.enemies_push_each_other` on (and G-6′ off), the simulation is the base's
/// (main at `708576ac` with WP0's diagnostics, before E-1), bit for bit, on the E-T1 scene and on
/// battle 28's first seed of record. E-1 adds no state; G-6′'s memory field moved the schema to
/// 47, so the digests are pinned at 47. Proof that they are the base's: a scratch build of this
/// tree that leaves `ring_retarget` unhashed and holds the schema at 46 gives exactly the base's
/// digests of the same two runs, `0xa381a7c5b8982232` and `0x6b074574a51eff4b` (evidence
/// `docs/evidence/2026-11-24-solver-retarget-and-shoving.md`).
#[test]
fn the_enemy_push_switch_on_is_bit_identical() -> TestResult {
    let press = press_digest(even_split(solver_rules()?)?)?;
    let battle = battle_digest(even_split(sim_fixtures::harness::shipped_rules())?)?;
    if std::env::var_os("E1_PRINT_DIGESTS").is_some() {
        println!("press digest {press:#018x}, battle digest {battle:#018x}");
    }
    assert_eq!(press, PRESS_DIGEST_EVEN_SPLIT, "the E-T1 scene, switch on");
    assert_eq!(battle, BATTLE_DIGEST_EVEN_SPLIT, "battle 28, switch on");
    Ok(())
}

/// The digest of the E-T1 scene with the base's behaviour (schema 47).
const PRESS_DIGEST_EVEN_SPLIT: u64 = 0xe27d_9266_6aa9_7f72;
/// The digest of battle 28's first seed of record with the base's behaviour (schema 47).
const BATTLE_DIGEST_EVEN_SPLIT: u64 = 0x601d_5cda_1b45_3251;
