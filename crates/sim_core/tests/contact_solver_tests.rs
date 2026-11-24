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
//! Floats appear only in this test's arithmetic on catalog values (charter §3.1).

use sim_core::catalog::MovementPipeline;
use sim_core::movement::solver::{contact_shares, ContactParty, PairShares};
use sim_types::{MoverType, PlayerId, SimScalar};

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
