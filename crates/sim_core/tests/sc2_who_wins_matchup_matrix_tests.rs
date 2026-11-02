//! Data-driven "who wins" matchup matrix, played through `PlayerCommand` on several seeds.
//!
//! # Responsibility
//! Plays every battle in `fixtures/matchup_matrix.ron` and checks the recorded outcome. The
//! battles are the `StarCraft` II: Legacy of the Void (Patch 5.0.14) benchmarks the catalog is
//! tuned against: duels, splash, breakpoints, mass swarms and equal-supply fights.
//!
//! # How it fits
//! The fixture holds what a designer would change: who fights, where, under which orders, and
//! how many survive. This runner holds only the mechanics of playing it, so rebalancing or
//! adding a matchup is a data edit. Damage formulas are tested exactly elsewhere; this suite
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

use serde::Deserialize;
use sim_core::{ActiveHazardZone, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

const FIXTURE: &str = include_str!("fixtures/matchup_matrix.ron");
/// Threads playing battles, modest because the suite runs on shared machines.
const WORKERS: usize = 4;
const BLUE: PlayerId = PlayerId(0);
const RED: PlayerId = PlayerId(1);

/// The whole matrix.
#[derive(Debug, Deserialize)]
struct Matrix {
    /// World seeds; the first plays the authored positions unchanged.
    seeds: Vec<u64>,
    /// Largest spawn nudge per axis on later seeds, in world units.
    spawn_jitter: f32,
    battles: Vec<Battle>,
}

/// One battle and its recorded result.
#[derive(Debug, Deserialize)]
struct Battle {
    name: String,
    /// What the battle is meant to demonstrate.
    claim: String,
    map_cells: u32,
    /// Ticks allowed before the battle is judged where it stands.
    ticks: u32,
    blue: Side,
    red: Side,
    #[serde(default)]
    hazard: Option<HazardFixture>,
    expect: Expected,
}

/// One player's forces and instructions.
#[derive(Debug, Deserialize)]
struct Side {
    squads: Vec<Squad>,
    #[serde(default)]
    order: SideOrder,
    /// Upgrades complete before anything spawns.
    #[serde(default)]
    research: Vec<String>,
    /// Issues `UseStim` to the whole side on the first tick.
    #[serde(default)]
    stim: bool,
}

/// A rectangular group of identical units.
#[derive(Debug, Deserialize)]
struct Squad {
    unit: String,
    count: u32,
    columns: u32,
    origin: (f32, f32),
    spacing: f32,
}

/// The side's standing order, issued once on the first tick.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
enum SideOrder {
    #[default]
    None,
    AttackMove(f32, f32),
    /// Focus fire on the enemy side's first unit.
    AttackFirstEnemy,
    HoldPosition,
}

/// An area hazard already on the map, owned by the blue player.
#[derive(Debug, Deserialize)]
struct HazardFixture {
    center: (f32, f32),
    radius: i32,
    remaining_ticks: u16,
    interval_ticks: u16,
    damage_per_interval: i32,
}

/// The recorded result of a battle.
#[derive(Debug, Deserialize)]
struct Expected {
    /// Inclusive survivor count range.
    blue: (usize, usize),
    /// Inclusive survivor count range.
    red: (usize, usize),
    /// Inclusive hull range of blue's first unit, when it survives.
    #[serde(default)]
    blue_lead_hp: Option<(f32, f32)>,
    /// Inclusive hull range of red's first unit, when it survives.
    #[serde(default)]
    red_lead_hp: Option<(f32, f32)>,
}

/// What actually happened in one playthrough.
#[derive(Debug, Clone, Copy)]
struct Outcome {
    blue: usize,
    red: usize,
    blue_lead_hp: Option<f32>,
    red_lead_hp: Option<f32>,
}

/// Deterministic spawn nudges for one seed, drawn from a small xorshift generator.
struct Jitter {
    state: u64,
    scale: f32,
}

impl Jitter {
    fn new(seed: u64, scale: f32) -> Self {
        Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            scale,
        }
    }

    /// Next nudge in `-scale..=scale` world units.
    #[expect(
        clippy::cast_precision_loss,
        reason = "spawn nudges are test scenery; any 16 bits of spread are enough"
    )]
    fn next(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let unit = (self.state >> 48) as f32 / f32::from(u16::MAX);
        (unit * 2.0 - 1.0) * self.scale
    }
}

fn point(x: f32, y: f32) -> SimVec2 {
    SimVec2::new(SimScalar::from_f32_lossy(x), SimScalar::from_f32_lossy(y))
}

fn spawn_side(
    world: &mut SimWorld,
    owner: PlayerId,
    side: &Side,
    jitter: &mut Jitter,
) -> Vec<EntityId> {
    for upgrade in &side.research {
        world
            .research_upgrade(owner, upgrade)
            .unwrap_or_else(|error| panic!("research {upgrade}: {error}"));
    }
    let mut units = Vec::new();
    for squad in &side.squads {
        let unit_type = world
            .catalog
            .id_by_slug(&squad.unit)
            .unwrap_or_else(|| panic!("catalog lacks {}", squad.unit));
        for index in 0..squad.count {
            let (row, column) = (index / squad.columns, index % squad.columns);
            #[expect(
                clippy::cast_precision_loss,
                reason = "grid indices are far below f32's integer range"
            )]
            let position = point(
                squad.origin.0 + column as f32 * squad.spacing + jitter.next(),
                squad.origin.1 + row as f32 * squad.spacing + jitter.next(),
            );
            let unit = world
                .spawn_unit(unit_type, owner, position)
                .expect("matrix spawn fits the arena");
            units.push(unit);
        }
    }
    units
}

fn side_commands(
    owner: PlayerId,
    side: &Side,
    units: &[EntityId],
    enemies: &[EntityId],
) -> Vec<PlayerCommand> {
    let command = |order| PlayerCommand {
        player_id: owner,
        entities: units.to_vec(),
        order,
        queue: false,
    };
    let order = match side.order {
        SideOrder::None => None,
        SideOrder::AttackMove(x, y) => Some(UnitOrder::AttackMove {
            target: point(x, y),
        }),
        SideOrder::AttackFirstEnemy => Some(UnitOrder::attack(
            *enemies.first().expect("an enemy to focus"),
        )),
        SideOrder::HoldPosition => Some(UnitOrder::HoldPosition),
    };
    let mut commands: Vec<_> = order.into_iter().map(command).collect();
    if side.stim {
        commands.push(command(UnitOrder::UseStim));
    }
    commands
}

fn living(world: &SimWorld, units: &[EntityId]) -> usize {
    units
        .iter()
        .filter(|id| {
            world
                .entities
                .get(id)
                .is_some_and(sim_core::Entity::is_alive)
        })
        .count()
}

fn lead_hp(world: &SimWorld, units: &[EntityId]) -> Option<f32> {
    let lead = world.entities.get(units.first()?)?;
    lead.is_alive().then(|| lead.hp.to_f32())
}

/// Plays `battle` to resolution or to its tick limit.
fn play(battle: &Battle, seed: u64, spawn_jitter: f32) -> Outcome {
    let map = GameMap::new(
        battle.map_cells,
        battle.map_cells,
        MapCell::default_ground(CliffLevel(0)),
    )
    .expect("invariant: matrix arenas have a valid size");
    let mut world = SimWorld::new(map, seed);
    let mut jitter = Jitter::new(seed, spawn_jitter);
    let blue = spawn_side(&mut world, BLUE, &battle.blue, &mut jitter);
    let red = spawn_side(&mut world, RED, &battle.red, &mut jitter);

    // Rule fixture: no shipped unit casts a hazard, so the storm is placed directly.
    if let Some(hazard) = &battle.hazard {
        world.active_hazards.push(ActiveHazardZone {
            id: 1,
            owner: BLUE,
            center: point(hazard.center.0, hazard.center.1),
            radius: SimScalar::from_num(hazard.radius),
            delay_ticks: 0,
            remaining_ticks: hazard.remaining_ticks,
            interval_ticks: hazard.interval_ticks,
            ticks_until_next_interval: 1,
            damage_per_interval: SimScalar::from_num(hazard.damage_per_interval),
            targets_friendly: false,
        });
    }

    let mut commands = side_commands(BLUE, &battle.blue, &blue, &red);
    commands.extend(side_commands(RED, &battle.red, &red, &blue));
    let two_sided = !blue.is_empty() && !red.is_empty();
    let mut tick = 0;
    while tick < battle.ticks {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        tick += 1;
        let diagnostics: Vec<_> = world.drain_diagnostics().collect();
        assert!(
            diagnostics.is_empty(),
            "{}: tick {tick}: {diagnostics:?}",
            battle.name
        );
        if two_sided && (living(&world, &blue) == 0 || living(&world, &red) == 0) {
            break;
        }
    }
    Outcome {
        blue: living(&world, &blue),
        red: living(&world, &red),
        blue_lead_hp: lead_hp(&world, &blue),
        red_lead_hp: lead_hp(&world, &red),
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
    let matrix: Matrix = ron::from_str(FIXTURE).expect("matrix fixture parses");
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
    let matrix: Matrix = ron::from_str(FIXTURE).expect("matrix fixture parses");
    let mut names: Vec<_> = matrix.battles.iter().map(|b| b.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), matrix.battles.len());
    assert!(matrix.battles.iter().all(|b| !b.claim.is_empty()));
}

/// A seed that changes nothing would make "several seeds" an empty claim.
#[test]
fn later_seeds_really_replay_a_different_formation() {
    let matrix: Matrix = ron::from_str(FIXTURE).expect("matrix fixture parses");
    let mut first = Jitter::new(matrix.seeds[1], matrix.spawn_jitter);
    let mut second = Jitter::new(matrix.seeds[2], matrix.spawn_jitter);
    assert!(first.next().abs() <= matrix.spawn_jitter);
    assert!((first.next() - second.next()).abs() > f32::EPSILON);
}
