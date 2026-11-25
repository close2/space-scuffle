//! Clash mode of the trace tool: the throughput benchmark's 400-unit battle, played with the
//! battle meter (plan `2026-11-27-gated-movement` §5.2, `WP0a`).
//!
//! # Responsibility
//! [`run_clash`] builds the world of the benchmark's flat 64 × 64 scenario of 400 units
//! (`crates/sim_core/benches/throughput.rs`): a flat map, the named unit split evenly between two
//! players in opposite corners in rows of twenty, five cells from the edges, blue ordered on the
//! first tick and red on the second to attack-move to the far corner, eight cells in. It then
//! plays [`CLASH_TICKS`] more ticks under the [`ContactMeter`] on each selected seed and writes
//! `<out>/clash_<unit>.csv`: per seed the survivors, the contact checks and the columns of plan
//! `2026-11-27-gated-movement` (H-1, M1, M1-hi, the cost probes), so the friend-overlap checks
//! F-1 and F-2 can be read on the two clashes the plan names.
//!
//! # How it fits
//! `matchup_trace --clash <unit> --out <dir> [--seeds 4]` runs it. The unit is an argument,
//! never a name in Rust (charter §3.3); the seed of record is the benchmark's, [`CLASH_SEED`],
//! and `--seeds n` adds the first `n` positive integers after it, as the matrix mode does.
//!
//! # Invariants
//! - A row is a function of the unit, the catalog and the seed; rows are in seed order.
//! - The setup is the benchmark's, step for step, so the measured ticks are the ones it times.

use super::cli::{create_dir, load_catalogs, write, RunError, TraceArgs};
use super::contact_checks::{ContactChecks, ContactMeter, MovementChecks};
use super::matrix::{checks_cells, movement_cells, CSV_HEADER, SPLIT_CSV_HEADER};
use sim_core::catalog::RuntimeCatalogs;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::path::PathBuf;

/// The benchmark's world seed, the clashes' seed of record.
pub const CLASH_SEED: u64 = 7;

/// Ticks measured after the two setup ticks, as the benchmark measures them.
pub const CLASH_TICKS: u32 = 300;

/// The benchmark's units, map side in cells, units per formation row, and margins (cells) of the
/// formation and of the attack-move target from the map's edges.
const UNITS: u32 = 400;
const MAP_CELLS: u32 = 64;
const FORMATION_COLUMNS: u32 = 20;
const FORMATION_MARGIN: u32 = 5;
const ORDER_MARGIN: u32 = 8;

/// One clash played on one seed.
#[derive(Debug, Clone, PartialEq)]
pub struct ClashRow {
    /// The unit's catalog slug.
    pub unit: String,
    /// The world seed.
    pub seed: u64,
    /// Ticks played, the two setup ticks included.
    pub ticks: u32,
    /// Living units per side at the end.
    pub alive: [u32; 2],
    /// The meter's contact checks over the measured ticks.
    pub checks: ContactChecks,
    /// The meter's columns of plan `2026-11-27-gated-movement` over the measured ticks.
    pub movement: MovementChecks,
}

impl ClashRow {
    /// The CSV header: the clash's own columns, then the matrix's check columns.
    #[must_use]
    pub fn header() -> String {
        let checks = CSV_HEADER
            .get(SPLIT_CSV_HEADER.len() + 1..)
            .unwrap_or_default();
        format!("unit,seed,of_record,ticks,blue_alive,red_alive,{checks}")
    }

    /// The row as one CSV line, without a line end.
    #[must_use]
    pub fn csv_line(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{}",
            self.unit,
            self.seed,
            self.seed == CLASH_SEED,
            self.ticks,
            self.alive[0],
            self.alive[1],
            checks_cells(&self.checks),
            movement_cells(&self.movement)
        )
    }
}

/// The cell `(x, y)` as a world point.
fn cell_point(x: u32, y: u32) -> Result<SimVec2, RunError> {
    let scalar = |v: u32| {
        i32::try_from(v)
            .map(SimScalar::from_i32)
            .map_err(|_| RunError::Usage(format!("cell {v} out of range")))
    };
    Ok(SimVec2::new(scalar(x)?, scalar(y)?))
}

/// Steps `world` one tick with `commands`.
fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

/// The benchmark's world for `unit` on `seed`, after its two setup ticks.
fn clash_world(catalogs: &RuntimeCatalogs, unit: &str, seed: u64) -> Result<SimWorld, RunError> {
    let world_error = |e: &dyn std::fmt::Display| RunError::Usage(format!("clash world: {e}"));
    let map = GameMap::new(MAP_CELLS, MAP_CELLS, MapCell::default_ground(CliffLevel(0)))
        .map_err(|e| world_error(&e))?;
    let mut world =
        SimWorld::with_catalogs(map, seed, catalogs.units.clone(), catalogs.upgrades.clone())
            .map_err(|e| world_error(&e))?;
    let kind = world
        .catalog
        .id_by_slug(unit)
        .ok_or_else(|| RunError::Usage(format!("--clash: no unit `{unit}` in the catalog")))?;
    let far_edge = MAP_CELLS - 1 - FORMATION_MARGIN;
    let (mut blue, mut red): (Vec<EntityId>, Vec<EntityId>) = (Vec::new(), Vec::new());
    for index in 0..UNITS {
        let (column, row) = (index % FORMATION_COLUMNS, index / FORMATION_COLUMNS);
        let (player, at, side) = if index % 2 == 0 {
            let at = cell_point(FORMATION_MARGIN + column, FORMATION_MARGIN + row)?;
            (PlayerId(0), at, &mut blue)
        } else {
            (
                PlayerId(1),
                cell_point(far_edge - column, far_edge - row)?,
                &mut red,
            )
        };
        side.push(
            world
                .spawn_unit(kind, player, at)
                .map_err(|e| world_error(&e))?,
        );
    }
    let far_target = MAP_CELLS - ORDER_MARGIN;
    for (player, entities, target) in [
        (0, blue, cell_point(far_target, far_target)?),
        (1, red, cell_point(ORDER_MARGIN, ORDER_MARGIN)?),
    ] {
        step(
            &mut world,
            vec![PlayerCommand {
                player_id: PlayerId(player),
                entities,
                order: UnitOrder::AttackMove { target },
                queue: false,
            }],
        );
    }
    Ok(world)
}

/// Plays the clash of `unit` on `seed` with `catalogs`.
///
/// # Errors
/// [`RunError::Usage`] for an unknown unit or a world that cannot be built, and
/// [`RunError::Diagnostics`] when the simulation raises one.
pub fn play_clash(catalogs: &RuntimeCatalogs, unit: &str, seed: u64) -> Result<ClashRow, RunError> {
    let mut world = clash_world(catalogs, unit, seed)?;
    let mut meter = ContactMeter::new();
    meter.observe(&world);
    for _ in 0..CLASH_TICKS {
        step(&mut world, Vec::new());
        let diagnostics: Vec<_> = world.drain_diagnostics().collect();
        if !diagnostics.is_empty() {
            return Err(RunError::Diagnostics {
                seed,
                tick: world.current_tick,
                diagnostics: format!("{diagnostics:?}"),
            });
        }
        meter.observe(&world);
    }
    let alive = |owner: u8| {
        let count = world
            .entities
            .values()
            .filter(|e| e.is_alive() && e.owner == PlayerId(owner))
            .count();
        u32::try_from(count).unwrap_or(u32::MAX)
    };
    Ok(ClashRow {
        unit: unit.to_owned(),
        seed,
        ticks: world.current_tick,
        alive: [alive(0), alive(1)],
        checks: meter.total(),
        movement: meter.movement(),
    })
}

/// Plays the clash `args` names on every selected seed and writes `<out>/clash_<unit>.csv`;
/// returns the file and the rows.
///
/// # Errors
/// [`RunError::Usage`] without `--clash`; every error of [`play_clash`], and of writing the file.
pub fn run_clash(args: &TraceArgs) -> Result<(PathBuf, Vec<ClashRow>), RunError> {
    let unit = args
        .clash
        .as_deref()
        .ok_or_else(|| RunError::Usage("clash mode needs --clash <unit>".into()))?;
    let catalogs = load_catalogs(args)?;
    let rows = args
        .seed_list(&[CLASH_SEED])
        .into_iter()
        .map(|seed| play_clash(&catalogs, unit, seed))
        .collect::<Result<Vec<_>, _>>()?;
    let mut text = format!("{}\n", ClashRow::header());
    for row in &rows {
        text.push_str(&row.csv_line());
        text.push('\n');
    }
    create_dir(&args.out)?;
    let path = args.out.join(format!("clash_{unit}.csv"));
    write(&path, &text)?;
    Ok((path, rows))
}
