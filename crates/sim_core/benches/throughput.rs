//! Release-mode throughput benchmark for the authoritative simulation tick.
//!
//! # Responsibility
//! Measures how many complete `SimWorld::step` calls per second the real tick pipeline
//! sustains for the scenarios in `throughput_scenarios.ron`, and prints one Markdown
//! table row per scenario so results can be pasted into plans and compared over time.
//!
//! # How it fits
//! The charter claims a throughput target that nothing else in the repository measures.
//! This target is the measurement. It deliberately drives the public API only: real
//! catalog, real map, real `PlayerCommand`s, with no phase skipped.
//!
//! # Invariants
//! - Wall-clock time is reported and never asserted, so a shared or slow host cannot
//!   fail a build. The binary fails only if a scenario cannot be constructed.
//! - The reported figure is the median of the configured repeats, which is less
//!   sensitive to other work on the host than the mean.
//! - The unit identity is read from the data file; no archetype is named in Rust.
//!
//! Run with `cargo bench -p sim_core --bench throughput`.

use serde::Deserialize;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::error::Error;
use std::time::Instant;

/// Scenario table embedded at compile time so the benchmark has no runtime path.
const SCENARIO_TABLE: &str = include_str!("throughput_scenarios.ron");

/// Units per formation row when placing an army in its starting corner.
const FORMATION_COLUMNS: u32 = 20;

/// Distance in cells from a map edge to the first unit of an army.
const FORMATION_MARGIN: u32 = 5;

/// Distance in cells from a map edge to the attack-move destination.
const ORDER_MARGIN: u32 = 8;

/// Complete benchmark configuration decoded from `throughput_scenarios.ron`.
#[derive(Debug, Deserialize)]
struct BenchmarkConfig {
    /// Catalog slug of the unit spawned for both armies.
    unit_slug: String,
    /// World RNG seed.
    seed: u64,
    /// Simulation ticks measured per repeat.
    ticks: u32,
    /// Independent repeats per scenario; the median is reported.
    repeats: usize,
    /// Map and army sizes to measure, in table order.
    scenarios: Vec<Scenario>,
}

/// One row of the throughput table.
#[derive(Debug, Clone, Copy, Deserialize)]
struct Scenario {
    /// Side length of the flat square map in cells.
    map_cells: u32,
    /// Total units across both players.
    units: u32,
}

fn scalar_from_cells(cells: u32) -> Result<SimScalar, Box<dyn Error>> {
    Ok(SimScalar::from_i32(i32::try_from(cells)?))
}

fn position(x: u32, y: u32) -> Result<SimVec2, Box<dyn Error>> {
    Ok(SimVec2::new(scalar_from_cells(x)?, scalar_from_cells(y)?))
}

fn step_with(world: &mut SimWorld, tick: &mut u32, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(*tick),
        commands,
        state_checksum: None,
    });
    *tick += 1;
}

fn attack_move(player: u8, entities: Vec<EntityId>, target: SimVec2) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(player),
        entities,
        order: UnitOrder::AttackMove { target },
        queue: false,
    }
}

/// Builds a battle-ready world, returning it with the next tick to execute.
fn build_world(
    config: &BenchmarkConfig,
    scenario: Scenario,
) -> Result<(SimWorld, u32), Box<dyn Error>> {
    let map = GameMap::new(
        scenario.map_cells,
        scenario.map_cells,
        MapCell::default_ground(CliffLevel(0)),
    )?;
    let mut world = SimWorld::new(map, config.seed);
    let unit_type = world
        .catalog
        .id_by_slug(&config.unit_slug)
        .ok_or("scenario unit_slug is not in the catalog")?;
    let far_edge = scenario.map_cells - 1 - FORMATION_MARGIN;
    let (mut blue, mut red) = (Vec::new(), Vec::new());
    for index in 0..scenario.units {
        let column = index % FORMATION_COLUMNS;
        let row = index / FORMATION_COLUMNS;
        if index % 2 == 0 {
            let spawn = position(FORMATION_MARGIN + column, FORMATION_MARGIN + row)?;
            blue.push(world.spawn_unit(unit_type, PlayerId(0), spawn)?);
        } else {
            let spawn = position(far_edge - column, far_edge - row)?;
            red.push(world.spawn_unit(unit_type, PlayerId(1), spawn)?);
        }
    }
    let far_target = scenario.map_cells - ORDER_MARGIN;
    let mut tick = 0;
    step_with(
        &mut world,
        &mut tick,
        vec![attack_move(0, blue, position(far_target, far_target)?)],
    );
    step_with(
        &mut world,
        &mut tick,
        vec![attack_move(1, red, position(ORDER_MARGIN, ORDER_MARGIN)?)],
    );
    Ok((world, tick))
}

/// Measures one repeat, returning ticks per second.
fn measure(config: &BenchmarkConfig, scenario: Scenario) -> Result<f64, Box<dyn Error>> {
    let (mut world, mut tick) = build_world(config, scenario)?;
    let started = Instant::now();
    for _ in 0..config.ticks {
        step_with(&mut world, &mut tick, Vec::new());
    }
    Ok(f64::from(config.ticks) / started.elapsed().as_secs_f64())
}

fn main() -> Result<(), Box<dyn Error>> {
    let config: BenchmarkConfig = ron::from_str(SCENARIO_TABLE)?;
    println!("| Map | Units | Ticks/s (median of {}) |", config.repeats);
    println!("|---|---:|---:|");
    for scenario in &config.scenarios {
        let mut samples = Vec::with_capacity(config.repeats);
        for _ in 0..config.repeats {
            samples.push(measure(&config, *scenario)?);
        }
        samples.sort_by(f64::total_cmp);
        let median = samples
            .get(samples.len() / 2)
            .ok_or("scenario table must request at least one repeat")?;
        println!(
            "| {0}x{0} | {1} | {median:.0} |",
            scenario.map_cells, scenario.units
        );
    }
    Ok(())
}
