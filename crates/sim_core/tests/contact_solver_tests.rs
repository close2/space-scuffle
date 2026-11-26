//! # Integration Test: The Contact Solver's Data and Shares (plan `2026-11-20-contact-solver` WP2)
//!
//! Pins, through public APIs, what the solver pipeline will rest on:
//! - the iteration count in the shipped `movement_rules.ron` is at least C-3's bound
//!   `⌈ln(δ_move / ρ) / ln(5/6)⌉`, `ρ` recomputed from the shipped catalog as the largest
//!   `(s_a + s_b) / (r_a + r_b)` over pairs of mobile ground archetypes, `s` the top speed times the
//!   archetype's largest speed multiplier (Overclock, attack dash), so a faster unit or a smaller
//!   radius in the catalog fails this test until the data follows;
//! - `contact_shares` answers C-2's table for the public cases;
//! - the shipped file runs the legacy pipeline until the plan's switch-over (WP6);
//! - the determinism probe of plan `2026-11-27-gated-movement` (`WP0a`, W0-T9 and W0-T10):
//!   rotating the processing order by offset 0 is the shipped world, and the offset never
//!   enters the checksum;
//! - the crowd extension C-3′ of plan `2026-11-26-friend-crowd-overlap` ships switched off
//!   (`crowd_iterations` equal to `iterations`, owner question Q0118), and a catalog that switches
//!   it on reaches the solver of a running world: a dense battle then runs ticks past K, and with
//!   it off none, every world equal to the shipped one (the solver-level properties P-1 to P-7
//!   are unit tests of `movement::solver`, whose solver is crate-private).
//!
//! And the contact layer C′ of the `Gated` pipeline (plan `2026-11-27-gated-movement` WP1, §3.1),
//! played in worlds whose catalog selects `Gated`:
//! - where no C′ rule applies (one owner, nobody attacking, no wall contact), `Gated` plays the
//!   reference arm R (`Solver` with C-3′ at cap 151) bit for bit (C1-T1);
//! - an attacking unit is never nudged by a friend (C1-T2, C′-1), and a holding line is not moved
//!   by a crowd pressing through it (C1-T3);
//! - E-2 of plan `2026-11-25-enemy-contact-and-ring-retarget` (B-193: enemies block, never
//!   push; every step a body takes in the tick closes on an enemy by at most its share of the gap
//!   between them), ported without its switch: a crowd of melee attackers pressing from one side
//!   never moves the standing enemy it fights (E2-T1), mirrored and relabelled alike; a walker
//!   stops at a standing enemy's surface (E2-T2), and one whose path clips an enemy slides past
//!   it (E2-T3), the enemy unmoved in both; two enemies walking head-on meet at contact (E2-T4);
//!   an idle body pushed by a friend into a standing enemy stops at its surface (E2-T5);
//! - the `gated` section's cap ships at 151, at least C-3′'s derivation.
//!
//! The C′ scenes use scratch archetypes (C5) and derive their sizes from the radii and the ring
//! capacity `C = ⌊π / asin(r / (R + r + range))⌋` of plan `2026-11-20-contact-solver` §4.
//! Floats appear only in this test's arithmetic on catalog values and positions (charter §3.1).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use sim_core::catalog::{ContactRules, MovementPipeline, MovementRules, RuntimeCatalogs};
use sim_core::movement::solver::{contact_shares, ContactParty, PairShares, ShareRules};
use sim_core::{SimWorld, UnitCatalog};
use sim_fixtures::harness::corpus::{Lattice, ScratchArchetype, LATTICE};
use sim_fixtures::matchup::{BattleRun, Matrix, MATRIX_RON};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackOrigin, EntityId, MoverType, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

/// The battle the probe tests play: battle 28, a 110-unit melee crowd round a ring.
const PROBE_BATTLE: &str = "28_100_lings_vs_10_immortals";

/// Ticks of battle 28 the probe tests play.
const PROBE_TICKS: u32 = 150;

/// Battle 28 on its first seed of record, with the processing order rotated by `offset` when
/// one is given and the probe left untouched otherwise.
fn probe_run(offset: Option<u32>) -> Result<BattleRun, Box<dyn Error>> {
    let matrix = Matrix::from_ron_str(MATRIX_RON)?;
    let battle = matrix.battle(PROBE_BATTLE)?;
    let seed = *matrix
        .seeds
        .first()
        .ok_or("the matrix has seeds of record")?;
    let mut run = BattleRun::new(
        battle,
        sim_fixtures::shipped_catalogs(),
        seed,
        matrix.jitter_for(seed),
    )?;
    if let Some(offset) = offset {
        run.world.set_movement_order_rotation(offset);
    }
    Ok(run)
}

/// Every living body's id and position, in id order.
fn positions(world: &SimWorld) -> Vec<(EntityId, SimVec2)> {
    world
        .entities
        .iter()
        .filter(|(_, entity)| entity.is_alive())
        .map(|(&id, entity)| (id, entity.position))
        .collect()
}

/// W0-T9: battle 28's first 150 ticks with the rotation set to 0 give, tick by tick, the
/// checksums of the world no probe touched.
#[test]
fn order_rotation_zero_is_the_identity() -> Result<(), Box<dyn Error>> {
    let mut shipped = probe_run(None)?;
    let mut probed = probe_run(Some(0))?;
    assert_eq!(probed.world.movement_order_rotation(), 0);
    for tick in 0..PROBE_TICKS {
        assert!(shipped.step().is_empty() && probed.step().is_empty());
        assert_eq!(
            shipped.world.compute_checksum(),
            probed.world.compute_checksum(),
            "tick {tick}"
        );
    }
    Ok(())
}

/// W0-T10: the same world under two offsets gives equal checksums on every tick on which its
/// bodies stand at the same positions (the offset is unhashed); and the probe is live: the
/// rotated world is played with its offset.
#[test]
fn order_rotation_never_enters_the_checksum() -> Result<(), Box<dyn Error>> {
    let mut plain = probe_run(Some(0))?;
    let mut rotated = probe_run(Some(37))?;
    assert_eq!(rotated.world.movement_order_rotation(), 37);
    assert_eq!(
        plain.world.compute_checksum(),
        rotated.world.compute_checksum(),
        "before the first tick"
    );
    let mut equal_ticks = 0;
    for _ in 0..PROBE_TICKS {
        assert!(plain.step().is_empty() && rotated.step().is_empty());
        if positions(&plain.world) == positions(&rotated.world) {
            equal_ticks += 1;
            assert_eq!(
                plain.world.compute_checksum(),
                rotated.world.compute_checksum(),
                "tick {}",
                plain.ticks_played()
            );
        }
    }
    assert!(equal_ticks > 0, "no tick had equal positions to compare");
    Ok(())
}

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
        contact_shares(
            &base,
            &enemy,
            ShareRules {
                moving_friends_push: true,
                gated: false
            }
        ),
        PairShares::Weights(half, half)
    );
    assert_eq!(
        contact_shares(
            &base,
            &idle,
            ShareRules {
                moving_friends_push: true,
                gated: false
            }
        ),
        PairShares::Weights(zero, one)
    );
    assert_eq!(
        contact_shares(
            &base,
            &holding,
            ShareRules {
                moving_friends_push: true,
                gated: false
            }
        ),
        PairShares::Weights(one, zero)
    );
    assert_eq!(
        contact_shares(
            &base,
            &base,
            ShareRules {
                moving_friends_push: false,
                gated: false
            }
        ),
        PairShares::ByApproach
    );
}

/// WP1 (plan `2026-11-26-friend-crowd-overlap`): C-3′ ships latent, its cap equal to C-3's K.
#[test]
fn the_crowd_extension_ships_switched_off() {
    let rules = sim_fixtures::shipped_catalogs()
        .units
        .movement_rules()
        .contact();
    assert_eq!(rules.crowd_iterations, u16::from(rules.iterations));
    assert!(!rules.crowd_extension_on());
}

/// The shipped catalogs with C-3′'s cap set to `crowd_iterations`.
fn with_crowd_iterations(crowd_iterations: u16) -> Result<RuntimeCatalogs, Box<dyn Error>> {
    let mut catalogs = sim_fixtures::shipped_catalogs();
    let rules = catalogs.units.movement_rules();
    let contact = ContactRules {
        crowd_iterations,
        ..rules.contact()
    };
    catalogs
        .units
        .set_movement_rules(rules.with_contact(contact)?);
    Ok(catalogs)
}

/// Plays the first `ticks` ticks of battle 26 (100 zergling analogues against 6 colossus
/// analogues, seed 15) with `catalogs`; returns each tick's solver iterations and checksum.
fn battle_26(catalogs: RuntimeCatalogs, ticks: u32) -> Result<Vec<(u16, u64)>, Box<dyn Error>> {
    let matrix = Matrix::from_ron_str(MATRIX_RON)?;
    let battle = matrix.battle("26_100_lings_vs_6_colossi")?;
    let mut run = BattleRun::new(battle, catalogs, 15, matrix.jitter_for(15))?;
    let mut out = Vec::new();
    while !run.is_finished() && run.ticks_played() < ticks {
        assert!(run.step().is_empty(), "no diagnostics");
        out.push((run.world.contact_iterations(), run.world.compute_checksum()));
    }
    Ok(out)
}

/// WP1 (plan `2026-11-26-friend-crowd-overlap`): the catalog's `crowd_iterations` reaches the
/// solver. With C-3′ switched off in a test catalog, battle 26's first 150 ticks never run past
/// K and every tick's checksum equals the shipped catalog's; switched on (cap 151, P-1's value
/// on this catalog), its crowd runs ticks past K, and only ticks that did so or followed one
/// differ.
#[test]
fn the_catalogs_crowd_cap_reaches_the_solver() -> Result<(), Box<dyn Error>> {
    let k = sim_fixtures::shipped_catalogs()
        .units
        .movement_rules()
        .contact()
        .iterations;
    let shipped = battle_26(sim_fixtures::shipped_catalogs(), 150)?;
    let off = battle_26(with_crowd_iterations(u16::from(k))?, 150)?;
    assert_eq!(off, shipped);
    assert!(off
        .iter()
        .all(|&(iterations, _)| iterations <= u16::from(k)));
    let on = battle_26(with_crowd_iterations(151)?, 150)?;
    let first_extended = on
        .iter()
        .position(|&(iterations, _)| iterations > u16::from(k))
        .ok_or("battle 26's crowd runs a tick past K")?;
    assert!(on.iter().all(|&(iterations, _)| iterations <= 151));
    assert_eq!(on[..first_extended], shipped[..first_extended]);
    assert_ne!(on[first_extended].1, shipped[first_extended].1);
    Ok(())
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
/// Q32.32 for each of the solver's applications in a tick (under `Gated` at most the `gated`
/// cap's 151 averaged iterations, two summed passes and application zero), rounded up to 160.
const ROUNDING: f64 = 160.0 / 4_294_967_296.0;

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

/// The scratch catalog: a melee attacker (top speed [`ATTACKER_SPEED`], a melee weapon), an armed standing
/// fighter (range 3), an unarmed immobile dummy (it cannot flee its attacker), an unarmed walker
/// (0.125), an unarmed post, an unarmed runner (twice the walker's speed) and an unarmed block of
/// radius one (as wide as a corridor of two cells), all mortal only in name (a million hit
/// points).
fn scratch_catalog(rules: MovementRules) -> Result<UnitCatalog, Box<dyn Error>> {
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
        // The attacker's weapon is melee, so D-δ may nudge it under `Solver` (C1-T2's control).
        ScratchArchetype {
            speed: "0.1875",
            ..kind(SMALL, Some("0.1"))
        }
        .ron("c1_attacker", ATTACKER, &faction)
        .replace(
            "bonus_upgrade_per_level: 0.0))",
            "bonus_upgrade_per_level: 0.0, melee: true))",
        ),
        kind(LARGE, Some("3.0")).ron("c1_fighter", FIGHTER, &faction),
        ScratchArchetype {
            speed: "0",
            ..kind(SMALL, None)
        }
        .ron("c1_dummy", DUMMY, &faction),
        kind(SMALL, None).ron("c1_walker", WALKER, &faction),
        kind(LARGE, None).ron("c1_post", POST, &faction),
        ScratchArchetype {
            speed: "0.25",
            ..kind(SMALL, None)
        }
        .ron("c1_runner", RUNNER, &faction),
        kind(LATTICE, None).ron("c1_block", BLOCK, &faction),
    ];
    let mut catalog =
        UnitCatalog::from_ron_str(&factions, &format!("(units: [{}])", entries.join(",")))?;
    catalog.set_movement_rules(rules);
    Ok(catalog)
}

/// The shipped movement rules on the pipeline `pipeline`.
fn rules_on(pipeline: MovementPipeline) -> Result<MovementRules, Box<dyn Error>> {
    let rules = sim_fixtures::harness::shipped_rules();
    Ok(rules.with_contact(ContactRules {
        pipeline,
        ..rules.contact()
    })?)
}

/// The shipped movement rules on the `Gated` pipeline (the shipped `gated` section).
fn gated_rules() -> Result<MovementRules, Box<dyn Error>> {
    rules_on(MovementPipeline::Gated)
}

/// The reference arm R of plan `2026-11-27-gated-movement` §6.1: `Solver` with C-3′ on at the
/// shipped `gated` section's cap.
fn reference_rules() -> Result<MovementRules, Box<dyn Error>> {
    let rules = rules_on(MovementPipeline::Solver)?;
    let cap = rules
        .contact()
        .gated
        .ok_or("the shipped file carries a gated section")?
        .crowd_iterations;
    Ok(rules.with_contact(ContactRules {
        crowd_iterations: cap,
        ..rules.contact()
    })?)
}

/// An empty flat 64 × 64 world on the scratch catalog.
fn scratch_world(rules: MovementRules, seed: u64) -> Result<SimWorld, Box<dyn Error>> {
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

fn position(world: &SimWorld, id: EntityId) -> Result<SimVec2, Box<dyn Error>> {
    Ok(world.entities.get(&id).ok_or("a unit")?.position)
}

/// The C1-T1 scene under `rules`: 64 friendly walkers and runners in an 8 × 8 block, packed at
/// contact, given one `Move` to the block's far side; the per-tick checksums, contact iterations
/// and terrain transfers of 160 ticks.
fn converging_crowd(rules: MovementRules) -> Result<Vec<(u64, u16, u32)>, Box<dyn Error>> {
    let mut world = scratch_world(rules, 0x0C1_0001)?;
    let pitch = 2.0 * world_units(SMALL) + 1.0 / 64.0;
    let mut crowd = Vec::new();
    for row in 0..8 {
        for column in 0..8 {
            let kind = if (row + column) % 3 == 0 {
                RUNNER
            } else {
                WALKER
            };
            let p = at(
                CENTRE.0 - 6.0 + pitch * f64::from(column),
                CENTRE.1 - 3.0 + pitch * f64::from(row),
            );
            crowd.push(world.spawn_unit(UnitTypeId(kind), BLUE, p)?);
        }
    }
    step(
        &mut world,
        vec![order(
            BLUE,
            &crowd,
            UnitOrder::Move {
                target: at(CENTRE.0 + 4.0, CENTRE.1),
            },
        )],
    );
    let mut out = Vec::new();
    for _ in 0..160 {
        step(&mut world, vec![]);
        assert!(world.drain_diagnostics().next().is_none());
        out.push((
            world.compute_checksum(),
            world.contact_iterations(),
            world.movement_probes().terrain_transfers,
        ));
    }
    Ok(out)
}

/// C1-T1 (plan `2026-11-27-gated-movement` WP1, F-4): an enemy-free, attack-free crowd that never
/// touches a wall plays bit-identically under `Gated` and under R (`Solver` with C-3′ at the same
/// cap), tick by tick; no terrain transfer happens, and R's crowd runs ticks past K, so the
/// equality covers the extension too.
#[test]
fn gated_is_the_reference_where_no_c_prime_rule_applies() -> TestResult {
    let gated = converging_crowd(gated_rules()?)?;
    let reference = converging_crowd(reference_rules()?)?;
    let k = gated_rules()?.contact().iterations;
    assert!(
        reference
            .iter()
            .any(|&(_, iterations, _)| iterations > u16::from(k)),
        "the crowd never ran past K"
    );
    assert!(gated.iter().all(|&(_, _, transfers)| transfers == 0));
    for (tick, (g, r)) in gated.iter().zip(&reference).enumerate() {
        assert_eq!(g.0, r.0, "tick {tick}");
    }
    Ok(())
}

/// The C1-T2 scene under `rules`: a melee attacker attacking an immobile enemy dummy from in its
/// band (west of it), and a friendly walker that starts touching it on its north and walks due
/// south through the attacker's position, along the tangent of the attacker's ring. Returns the attacker's largest displacement from
/// where it stood once the order settled (tick 5) over the next 75 ticks, world units.
fn pressed_attacker(rules: MovementRules) -> Result<f64, Box<dyn Error>> {
    let mut world = scratch_world(rules, 0x0C1_0002)?;
    let r = world_units(SMALL);
    let dummy = world.spawn_unit(UnitTypeId(DUMMY), RED, at(CENTRE.0, CENTRE.1))?;
    let x = CENTRE.0 - 2.0 * r - MELEE / 2.0;
    let attacker = world.spawn_unit(UnitTypeId(ATTACKER), BLUE, at(x, CENTRE.1))?;
    let walker = world.spawn_unit(UnitTypeId(WALKER), BLUE, at(x, CENTRE.1 + 2.0 * r))?;
    step(
        &mut world,
        vec![
            order(
                BLUE,
                &[attacker],
                UnitOrder::AttackTarget {
                    target: dummy,
                    origin: AttackOrigin::Explicit,
                },
            ),
            order(
                BLUE,
                &[walker],
                UnitOrder::Move {
                    target: at(x, CENTRE.1 - 8.0),
                },
            ),
        ],
    );
    for _ in 0..5 {
        step(&mut world, vec![]);
    }
    let settled = position(&world, attacker)?;
    let mut worst = 0.0_f64;
    for _ in 0..75 {
        step(&mut world, vec![]);
        worst = worst.max(exact_distance(position(&world, attacker)?, settled));
    }
    Ok(worst)
}

/// C1-T2 (C′-1, Q0121 (a)): under `Gated` a melee unit attacking in its band is never moved by a
/// friend walking into it along its ring, though D-δ is on in the shipped data; under `Solver`
/// the same press nudges it along the ring (the scene does press).
#[test]
fn an_attacking_unit_is_never_nudged_under_gated() -> TestResult {
    let gated = pressed_attacker(gated_rules()?)?;
    assert!(gated == 0.0, "the attacker moved by {gated} under Gated");
    let solver = pressed_attacker(rules_on(MovementPipeline::Solver)?)?;
    assert!(solver > 0.0, "the walker never pressed the attacker");
    Ok(())
}

/// C1-T3 (H-1 in miniature): six walkers on Hold Position in a line across the field, and
/// eighteen walkers and runners given one `Move` through it from the west. No holder moves.
#[test]
fn a_holding_line_is_not_moved_by_a_pressing_crowd_under_gated() -> TestResult {
    let mut world = scratch_world(gated_rules()?, 0x0C1_0003)?;
    let pitch = 2.0 * world_units(SMALL) + 0.05;
    let mut holders = Vec::new();
    for k in 0..6 {
        let y = CENTRE.1 + pitch * (f64::from(k) - 2.5);
        holders.push(world.spawn_unit(UnitTypeId(WALKER), BLUE, at(CENTRE.0, y))?);
    }
    let mut crowd = Vec::new();
    for k in 0..18 {
        let (row, column) = (k / 3, k % 3);
        let kind = if k % 2 == 0 { RUNNER } else { WALKER };
        let p = at(
            CENTRE.0 - 4.0 - pitch * f64::from(column),
            CENTRE.1 + pitch * (f64::from(row) - 2.5),
        );
        crowd.push(world.spawn_unit(UnitTypeId(kind), BLUE, p)?);
    }
    step(
        &mut world,
        vec![
            order(BLUE, &holders, UnitOrder::HoldPosition),
            order(
                BLUE,
                &crowd,
                UnitOrder::Move {
                    target: at(CENTRE.0 + 8.0, CENTRE.1),
                },
            ),
        ],
    );
    let start: Vec<SimVec2> = holders
        .iter()
        .map(|&id| position(&world, id))
        .collect::<Result<_, _>>()?;
    let mut touched = 0_u32;
    for tick in 0..200 {
        step(&mut world, vec![]);
        for (&id, &from) in holders.iter().zip(&start) {
            assert_eq!(position(&world, id)?, from, "holder {id:?} at tick {tick}");
        }
        touched += u32::from(crowd.iter().any(|&id| {
            position(&world, id).is_ok_and(|p| {
                start
                    .iter()
                    .any(|&h| exact_distance(p, h) <= 2.0 * world_units(SMALL) + 1.0 / 64.0)
            })
        }));
    }
    assert!(
        touched > 10,
        "the crowd touched the line on {touched} ticks"
    );
    Ok(())
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
fn press_scene(rules: MovementRules, layout: Layout) -> Result<PressScene, Box<dyn Error>> {
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

/// E2-T1 (plan `2026-11-25-enemy-contact-and-ring-retarget` §8 WP2), under `Gated`: a standing,
/// armed, not-holding fighter attacking a target in reach is never moved by the melee crowd that
/// presses into it from one side, twice the ring's capacity in three ranks, over twice the rear
/// rank's arrival time; mirrored and relabelled alike. Its D-SHOVE-E is therefore 0, and the meter
/// confirms it.
#[test]
fn a_pressing_crowd_does_not_move_a_standing_enemy() -> TestResult {
    use sim_fixtures::trace::contact_checks::ContactMeter;
    for layout in LAYOUTS {
        let mut scene = press_scene(gated_rules()?, layout)?;
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

/// A 64 × 64 world on the scratch catalog (`Gated`) whose only walkable cells are the `rows` rows
/// from the corridor row up.
fn corridor_world(seed: u64, rows: u32) -> Result<SimWorld, Box<dyn Error>> {
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))?;
    let walkable = CORRIDOR_ROW..CORRIDOR_ROW + rows;
    for y in (0..64).filter(|y| !walkable.contains(y)) {
        for x in 0..64 {
            map.cell_mut(x, y)?.flags = sim_map::CellFlags::empty();
        }
    }
    let mut world = sim_fixtures::try_world(map, seed)?;
    world.catalog = scratch_catalog(gated_rules()?)?;
    Ok(world)
}

/// The corridor's centre line, world units.
fn row() -> f64 {
    f64::from(CORRIDOR_ROW) + 0.5
}

/// E2-T2, under `Gated`: a walker on `Move` down a corridor one cell wide, straight at a standing
/// (idle, unarmed) enemy post in its way, never moves the post, never overlaps it by more than
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

/// E2-T3, under `Gated`: a walker on `Move` due east that starts touching a standing enemy post
/// on its south-east (its centre a third of the radius sum above the post's, so its path clips the
/// post's disc and its first step already closes on it) ends beyond the post, having kept the
/// tangential part of its steps, and the post never moves.
#[test]
fn a_grazing_walker_slides_along_an_enemy() -> TestResult {
    let mut world = scratch_world(gated_rules()?, 0x0E2_0003)?;
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
/// world and both units.
fn head_on(
    red_kind: u16,
    gap: f64,
    mirrored: bool,
) -> Result<(SimWorld, EntityId, EntityId), Box<dyn Error>> {
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

/// E2-T4, under `Gated`: two enemies walking head-on down a corridor meet at contact (overlap
/// within [`ROUNDING`] on every tick, touching at the end), at equal speeds and at 2:1; at equal
/// speeds they cover equal distances, and the mirrored scene (sides swapped) gives the same
/// distances.
#[test]
fn two_walkers_meet_at_contact() -> TestResult {
    let sum = 2.0 * world_units(SMALL);
    let gap = 3.0;
    let run = |red_kind, mirrored| -> Result<(f64, f64), Box<dyn Error>> {
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

/// E2-T5, under `Gated`: a walker on `Move` pushes an idle friend of its own width ahead of it
/// into a standing enemy post. The friend ends touching the post (overlap within [`ROUNDING`] on
/// every tick) and the post never moves. Two scenes: a corridor two cells wide, as wide as the
/// bodies (neither can pass or step aside), and an open field. Returns, per scene, the largest
/// overlap of the two friends as a share of their radius sum.
///
/// The enemy-contact plan's E2-T5 also expected the friends' overlap to stay within D-α's moving
/// bound. Without the gate it does not (that plan's Amendment 1, item 8: the walker keeps walking
/// into the friend E-2 holds), and plan `2026-11-27-gated-movement` registers that residual as
/// RC-1's, reported at WP1 and judged at WP2 (`I2-T8`, and F-1's extension as WP2's amendment
/// candidate S-7). So the friend overlap is printed here, not asserted.
#[test]
fn a_body_pressed_by_friends_stops_at_an_enemy() -> TestResult {
    let corridor = press_through_a_friend(Some(2))?;
    let open = press_through_a_friend(None)?;
    println!("E2-T5 friend overlap (reported, RC-1): corridor {corridor:.4}, open {open:.4}");
    Ok(())
}

/// E2-T5's scene in a corridor of `rows` cells, or in the open field when `None`; returns the
/// largest friend overlap share.
fn press_through_a_friend(rows: Option<u32>) -> Result<f64, Box<dyn Error>> {
    let mut world = match rows {
        Some(rows) => corridor_world(0x0E2_0005, rows)?,
        None => scratch_world(gated_rules()?, 0x0E2_0005)?,
    };
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
    let mut worst = 0.0_f64;
    for tick in 0..150 {
        step(&mut world, vec![]);
        assert_eq!(
            position(&world, post)?,
            start,
            "{rows:?}: the post moved at tick {tick}"
        );
        let gap = exact_distance(position(&world, idle)?, start) - (big + block);
        assert!(gap >= -ROUNDING, "{rows:?}: tick {tick}: overlap {}", -gap);
        let friends = exact_distance(position(&world, idle)?, position(&world, walker)?);
        worst = worst.max((2.0 * block - friends) / (2.0 * block));
    }
    let gap = exact_distance(position(&world, idle)?, start) - (big + block);
    assert!(
        gap <= 1.0 / 64.0,
        "{rows:?}: the idle friend stopped {gap} short of the post"
    );
    Ok(worst)
}

/// The `gated` section ships with C-3′'s cap at 151 (plan `2026-11-27-gated-movement` §3.4), and
/// the shipped pipeline stays `Solver` while `Gated` is latent.
#[test]
fn the_gated_section_ships_with_the_derived_cap_and_stays_latent() {
    let rules = sim_fixtures::shipped_catalogs()
        .units
        .movement_rules()
        .contact();
    assert_eq!(rules.pipeline, MovementPipeline::Solver);
    assert_eq!(rules.gated.map(|g| g.crowd_iterations), Some(151));
    assert_eq!(rules.solver_crowd_iterations(), rules.crowd_iterations);
}
