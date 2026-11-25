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
//! Floats appear only in this test's arithmetic on catalog values (charter §3.1).

use sim_core::catalog::{ContactRules, MovementPipeline, RuntimeCatalogs};
use sim_core::movement::solver::{contact_shares, ContactParty, PairShares};
use sim_core::SimWorld;
use sim_fixtures::matchup::{BattleRun, Matrix, MATRIX_RON};
use sim_types::{EntityId, MoverType, PlayerId, SimScalar, SimVec2};
use std::error::Error;

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
fn with_crowd_iterations(
    crowd_iterations: u16,
) -> Result<RuntimeCatalogs, Box<dyn Error>> {
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
fn battle_26(
    catalogs: RuntimeCatalogs,
    ticks: u32,
) -> Result<Vec<(u16, u64)>, Box<dyn Error>> {
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
