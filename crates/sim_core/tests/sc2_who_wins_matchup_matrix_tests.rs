//! Data-driven "who wins" matchup matrix, played through `PlayerCommand` on several seeds.
//!
//! # Responsibility
//! Plays every battle in `fixtures/matchup_matrix.ron` and checks the recorded outcome. The
//! battles are the `StarCraft` II: Legacy of the Void (Patch 5.0.14) benchmarks the catalog is
//! tuned against: duels, splash, breakpoints, mass swarms and equal-supply fights.
//!
//! # How it fits
//! The fixture holds what a designer would change: who fights, where, under which orders, and
//! how many survive. The mechanics of setting a battle up and playing it live in
//! `sim_fixtures::matchup`, shared with the battle trace tool, so rebalancing or adding a
//! matchup is a data edit and the trace replays exactly the battles checked here. Damage formulas are tested exactly elsewhere; this suite
//! asks whether whole battles still come out as the game intends.
//!
//! # Invariants
//! - Units are spawned, then ordered through `TickBundle` commands, exactly as a player would.
//!   The one fixture a command cannot express is a hazard, because no shipped unit casts one.
//! - The simulation draws no randomness in these battles, so a seed alone would replay the
//!   same fight. Every seed after the first therefore also nudges each spawn point, which
//!   shows whether an outcome is a property of the units or of one exact formation.
//! - An outcome must hold on every seed.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use sim_fixtures::matchup::{Battle, BattleRun, Expected, Jitter, Matrix, MATRIX_RON};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Threads playing battles, modest because the suite runs on shared machines.
const WORKERS: usize = 4;

/// What actually happened in one playthrough.
#[derive(Debug, Clone, Copy)]
struct Outcome {
    blue: usize,
    red: usize,
    blue_lead_hp: Option<f32>,
    red_lead_hp: Option<f32>,
}

fn lead_hp(world: &sim_core::SimWorld, units: &[sim_types::EntityId]) -> Option<f32> {
    let lead = world.entities.get(units.first()?)?;
    lead.is_alive().then(|| lead.hp.to_f32())
}

/// Plays `battle` to resolution or to its tick limit.
fn play(battle: &Battle, seed: u64, spawn_jitter: f32) -> Outcome {
    let mut run = BattleRun::new(battle, sim_fixtures::shipped_catalogs(), seed, spawn_jitter)
        .unwrap_or_else(|error| panic!("{}: setup: {error}", battle.name));
    while !run.is_finished() {
        let diagnostics = run.step();
        assert!(
            diagnostics.is_empty(),
            "{}: tick {}: {diagnostics:?}",
            battle.name,
            run.ticks_played()
        );
    }
    Outcome {
        blue: sim_fixtures::matchup::living(&run.world, &run.blue),
        red: sim_fixtures::matchup::living(&run.world, &run.red),
        blue_lead_hp: lead_hp(&run.world, &run.blue),
        red_lead_hp: lead_hp(&run.world, &run.red),
    }
}

fn within(range: Option<(f32, f32)>, value: Option<f32>) -> bool {
    match (range, value) {
        (Some((low, high)), Some(value)) => (low..=high).contains(&value),
        _ => true,
    }
}

fn holds(expected: &Expected, outcome: &Outcome) -> bool {
    (expected.blue.0..=expected.blue.1).contains(&outcome.blue)
        && (expected.red.0..=expected.red.1).contains(&outcome.red)
        && within(expected.blue_lead_hp, outcome.blue_lead_hp)
        && within(expected.red_lead_hp, outcome.red_lead_hp)
}

/// Plays `battle` on every seed, recording each outcome that breaks the fixture.
fn check_battle(matrix: &Matrix, battle: &Battle, failures: &mut Vec<String>) {
    for (index, &seed) in matrix.seeds.iter().enumerate() {
        let jitter = if index == 0 { 0.0 } else { matrix.spawn_jitter };
        let outcome = play(battle, seed, jitter);
        if !holds(&battle.expect, &outcome) {
            failures.push(format!(
                "{} seed {seed}: {outcome:?}, expected {:?}. Claim: {}",
                battle.name, battle.expect, battle.claim
            ));
        }
    }
}

#[test]
fn every_battle_keeps_its_recorded_outcome_on_every_seed() {
    let matrix: Matrix = Matrix::from_ron_str(MATRIX_RON).expect("matrix fixture parses");
    assert!(matrix.seeds.len() >= 3, "several seeds are required");

    // PERFORMANCE RATIONALE:
    // Why: Fifty battles on four seeds, some with over a hundred units, take minutes in series.
    // What: Worker threads claim whole battles from a shared counter.
    // Trade-off: Failures are gathered per battle and sorted, in exchange for a suite that
    // runs at the speed of the separate tests it replaced.
    let next_battle = AtomicUsize::new(0);
    let matrix_ref = &matrix;
    let next_ref = &next_battle;
    let mut failures: Vec<String> = thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|_| {
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    while let Some(battle) = matrix_ref
                        .battles
                        .get(next_ref.fetch_add(1, Ordering::Relaxed))
                    {
                        check_battle(matrix_ref, battle, &mut failures);
                    }
                    failures
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("matrix worker must not panic"))
            .collect()
    });
    failures.sort();
    let report = failures.join("\n");
    assert!(report.is_empty(), "matchup outcomes changed:\n{report}");
}

/// Names are how a failure is reported and how a rebaseline is explained.
#[test]
fn battles_are_named_uniquely_and_state_their_claim() {
    let matrix: Matrix = Matrix::from_ron_str(MATRIX_RON).expect("matrix fixture parses");
    let mut names: Vec<_> = matrix.battles.iter().map(|b| b.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), matrix.battles.len());
    assert!(matrix.battles.iter().all(|b| !b.claim.is_empty()));
}

/// A seed that changes nothing would make "several seeds" an empty claim.
#[test]
fn later_seeds_really_replay_a_different_formation() {
    let matrix: Matrix = Matrix::from_ron_str(MATRIX_RON).expect("matrix fixture parses");
    let mut first = Jitter::new(matrix.seeds[1], matrix.spawn_jitter);
    let mut second = Jitter::new(matrix.seeds[2], matrix.spawn_jitter);
    assert!(first.next_nudge().abs() <= matrix.spawn_jitter);
    assert!((first.next_nudge() - second.next_nudge()).abs() > f32::EPSILON);
}
