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
//! matchup is a data edit and the trace replays exactly the battles checked here. The judgement
//! itself (`Expected::holds` on an `Outcome`) lives there too, so the tool's whole-matrix mode
//! reports exactly the failures this suite would. Damage formulas are tested exactly
//! elsewhere; this suite asks whether whole battles still come out as the game intends.
//!
//! # Invariants
//! - Units are spawned, then ordered through `TickBundle` commands, exactly as a player would.
//!   Scripted casts are commands on their own ticks; the one setting a command cannot express
//!   is a squad's starting energy.
//! - The simulation draws no randomness in these battles, so a seed alone would replay the
//!   same fight. Every seed after the first therefore also nudges each spawn point, which
//!   shows whether an outcome is a property of the units or of one exact formation.
//! - An outcome must hold on every seed, unless the battle names a known bug of `DESIGN.md`'s
//!   register (`known_bug`, plan `2026-11-20-contact-solver` Amendment 19, Q0103): it is then
//!   reported as an expected failure while its claim breaks, and fails once the claim holds on
//!   every seed, so the marker cannot outlive its bug.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use sim_fixtures::matchup::{Battle, BattleRun, Jitter, Matrix, Outcome, Verdict, MATRIX_RON};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

/// Threads playing battles, modest because the suite runs on shared machines.
const WORKERS: usize = 4;

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
    Outcome::of(&run)
}

/// Plays `battle` on every seed and judges it ([`Battle::verdict`]); each outcome that breaks
/// the claim is described.
fn check_battle(matrix: &Matrix, battle: &Battle) -> (Verdict, Vec<String>) {
    let mut results = Vec::with_capacity(matrix.seeds.len());
    let mut broken = Vec::new();
    for (index, &seed) in matrix.seeds.iter().enumerate() {
        let jitter = if index == 0 { 0.0 } else { matrix.spawn_jitter };
        let outcome = play(battle, seed, jitter);
        let held = battle.expect.holds(&outcome);
        if !held {
            broken.push(format!(
                "{} seed {seed}: {outcome:?}, expected {:?}. Claim: {}",
                battle.name, battle.expect, battle.claim
            ));
        }
        results.push((seed, held));
    }
    (battle.verdict(&results), broken)
}

/// The report lines of one battle's verdict: failures (a broken claim without a known bug, or a
/// stale marker) and expected failures, which are listed apart and do not fail the suite.
fn report_lines(
    battle: &Battle,
    verdict: &Verdict,
    broken: Vec<String>,
) -> (Vec<String>, Vec<String>) {
    match verdict {
        Verdict::Holds => (Vec::new(), Vec::new()),
        Verdict::Broken(_) => (broken, Vec::new()),
        Verdict::ExpectedFailure { bug, .. } => (
            Vec::new(),
            broken
                .into_iter()
                .map(|line| format!("[{bug}] {line}"))
                .collect(),
        ),
        Verdict::StaleKnownBug(bug) => (
            vec![format!(
                "{} holds its claim on every seed, yet is marked with known bug {bug}: close the \
                 bug and remove the marker",
                battle.name
            )],
            Vec::new(),
        ),
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
    let (mut failures, mut expected): (Vec<String>, Vec<String>) = thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|_| {
                scope.spawn(move || {
                    let (mut failures, mut expected) = (Vec::new(), Vec::new());
                    while let Some(battle) = matrix_ref
                        .battles
                        .get(next_ref.fetch_add(1, Ordering::Relaxed))
                    {
                        let (verdict, broken) = check_battle(matrix_ref, battle);
                        let (failed, known) = report_lines(battle, &verdict, broken);
                        failures.extend(failed);
                        expected.extend(known);
                    }
                    (failures, expected)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("matrix worker must not panic"))
            .fold((Vec::new(), Vec::new()), |(mut f, mut e), (wf, we)| {
                f.extend(wf);
                e.extend(we);
                (f, e)
            })
    });
    failures.sort();
    expected.sort();
    if !expected.is_empty() {
        println!(
            "expected failures (known bugs, not counted):\n{}",
            expected.join("\n")
        );
    }
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

/// A known-bug marker names an open row of `DESIGN.md`'s bug register, so it is tracked there
/// and goes when the row is fixed (plan `2026-11-20-contact-solver` Amendment 19).
#[test]
fn every_known_bug_marker_names_an_open_bug_of_the_register() {
    let matrix: Matrix = Matrix::from_ron_str(MATRIX_RON).expect("matrix fixture parses");
    let design = include_str!("../../../DESIGN.md");
    for battle in &matrix.battles {
        let Some(bug) = battle.known_bug.as_deref() else {
            continue;
        };
        let row = design
            .lines()
            .find(|line| line.starts_with(&format!("| {bug} |")))
            .unwrap_or_else(|| panic!("{}: {bug} is not in DESIGN.md's register", battle.name));
        assert!(
            !row.contains("**fixed**"),
            "{}: {bug} is marked fixed; remove the marker",
            battle.name
        );
    }
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
