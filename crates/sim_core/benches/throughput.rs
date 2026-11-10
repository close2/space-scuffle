//! Release-mode throughput benchmark for the authoritative simulation tick.
//!
//! # Responsibility
//! Measures how many complete `SimWorld::step` calls per second the real tick pipeline
//! sustains for the scenarios in `throughput_scenarios.ron`, and prints one Markdown
//! table row per scenario so results can be pasted into plans and compared over time.
//!
//! # How it fits
//! Charter §3.1 requires every scenario to run at least in real time, and `DESIGN.md` §5
//! publishes the measured figures. This target is the measurement. It deliberately drives the
//! public API only: real catalog, real map, real `PlayerCommand`s, with no phase skipped. CI's
//! `throughput` job runs it with `--floor 25` and appends the table to the job summary.
//!
//! # Invariants
//! - Without `--floor`, wall-clock time is reported and never asserted. With
//!   `--floor <ticks per second>`, the binary prints the whole table first and then exits
//!   non-zero naming every scenario whose median is below the floor. The floor CI uses is the
//!   simulation's own tick rate (`sim_types::TICKS_PER_SECOND`): below it a match cannot run in
//!   real time, which no host speed excuses, and it leaves a wide margin for slow runners.
//! - The binary also fails if a scenario cannot be constructed or an argument is not understood.
//! - The reported figure is the median of the configured repeats, which is less
//!   sensitive to other work on the host than the mean.
//! - The unit identity is read from the data file; no archetype is named in Rust.
//! - The rule catalogs are loaded from disk at run time, as the game loads them: from
//!   `$SPACE_SCUFFLE_ASSETS/catalog` when that is set, else from the repository's `assets/catalog`.
//!
//! Run with `cargo bench -p sim_core --bench throughput [-- --floor <ticks per second>]`.

use serde::Deserialize;
use sim_core::catalog::RuntimeCatalogs;
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell, MapFile};
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::error::Error;
use std::path::PathBuf;
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
#[derive(Debug, Clone, Deserialize)]
struct Scenario {
    /// Side length of the flat square map in cells; ignored when `map_file` is set.
    map_cells: u32,
    /// Total units across both players.
    units: u32,
    /// A shipped map under `assets/maps/` to fight on instead of a flat square: each army
    /// starts packed around its start location and attack-moves to the other's (sc2
    /// pathfinding WP8, the terrain scenario).
    #[serde(default)]
    map_file: Option<String>,
}

impl Scenario {
    fn label(&self) -> String {
        self.map_file
            .clone()
            .unwrap_or_else(|| format!("{0}x{0}", self.map_cells))
    }
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

/// The catalog directory of the asset bundle: `$SPACE_SCUFFLE_ASSETS/catalog`, else the
/// repository's `assets/catalog`.
fn catalog_directory() -> PathBuf {
    std::env::var_os("SPACE_SCUFFLE_ASSETS").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog"),
        |assets| PathBuf::from(assets).join("catalog"),
    )
}

/// Builds a battle-ready world with `catalogs`, returning it with the next tick to execute.
fn build_world(
    config: &BenchmarkConfig,
    catalogs: &RuntimeCatalogs,
    scenario: &Scenario,
) -> Result<(SimWorld, u32), Box<dyn Error>> {
    if let Some(file) = &scenario.map_file {
        return build_terrain_world(config, catalogs, file, scenario.units);
    }
    let map = GameMap::new(
        scenario.map_cells,
        scenario.map_cells,
        MapCell::default_ground(CliffLevel(0)),
    )?;
    let mut world = SimWorld::with_catalogs(
        map,
        config.seed,
        catalogs.units.clone(),
        catalogs.upgrades.clone(),
    );
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

/// The cells of `map` around `centre` in rings of growing Chebyshev radius, in a fixed order,
/// keeping those a ground unit may stand on.
fn standable_cells_around(map: &GameMap, centre: (u32, u32), count: usize) -> Vec<(u32, u32)> {
    let mut cells = Vec::with_capacity(count);
    let side = i64::from(map.width().max(map.height()));
    for ring in 0..side {
        for dy in -ring..=ring {
            for dx in -ring..=ring {
                if dx.abs().max(dy.abs()) != ring || cells.len() >= count {
                    continue;
                }
                let (Ok(x), Ok(y)) = (
                    u32::try_from(i64::from(centre.0) + dx),
                    u32::try_from(i64::from(centre.1) + dy),
                ) else {
                    continue;
                };
                let free = map
                    .cell(x, y)
                    .is_ok_and(|cell| cell.occupying_structure.is_none());
                if free && map.terrain_step_is_connected((x, y), (x, y)) {
                    cells.push((x, y));
                }
            }
        }
    }
    cells
}

/// Builds a world on the shipped map `file` with `units` split between its first two start
/// locations, each army attack-moving to the other's start.
fn build_terrain_world(
    config: &BenchmarkConfig,
    catalogs: &RuntimeCatalogs,
    file: &str,
    units: u32,
) -> Result<(SimWorld, u32), Box<dyn Error>> {
    let path = catalog_directory().join("../maps").join(file);
    let document = MapFile::load(&path)?.decode()?;
    let [first, second, ..] = document.start_locations.as_slice() else {
        return Err(format!("{file} needs two start locations").into());
    };
    let (home, away) = (first.cell, second.cell);
    let map = document.to_game_map()?;
    let per_side = usize::try_from(units / 2)?;
    let blue_cells = standable_cells_around(&map, home, per_side);
    let red_cells = standable_cells_around(&map, away, per_side);
    let mut world = SimWorld::with_catalogs(
        map,
        config.seed,
        catalogs.units.clone(),
        catalogs.upgrades.clone(),
    );
    let unit_type = world
        .catalog
        .id_by_slug(&config.unit_slug)
        .ok_or("scenario unit_slug is not in the catalog")?;
    let centre = |(x, y): (u32, u32)| {
        Ok::<_, Box<dyn Error>>(SimVec2::new(
            scalar_from_cells(x)? + SimScalar::HALF,
            scalar_from_cells(y)? + SimScalar::HALF,
        ))
    };
    let (mut blue, mut red) = (Vec::new(), Vec::new());
    for cell in blue_cells {
        blue.push(world.spawn_unit(unit_type, PlayerId(0), centre(cell)?)?);
    }
    for cell in red_cells {
        red.push(world.spawn_unit(unit_type, PlayerId(1), centre(cell)?)?);
    }
    let mut tick = 0;
    step_with(
        &mut world,
        &mut tick,
        vec![attack_move(0, blue, centre(away)?)],
    );
    step_with(
        &mut world,
        &mut tick,
        vec![attack_move(1, red, centre(home)?)],
    );
    Ok((world, tick))
}

/// Measures one repeat, returning ticks per second.
#[expect(
    clippy::disallowed_methods,
    reason = "the benchmark times the simulation from outside; the clock never enters simulation state"
)]
fn measure(
    config: &BenchmarkConfig,
    catalogs: &RuntimeCatalogs,
    scenario: &Scenario,
) -> Result<f64, Box<dyn Error>> {
    let (mut world, mut tick) = build_world(config, catalogs, scenario)?;
    let started = Instant::now();
    for _ in 0..config.ticks {
        step_with(&mut world, &mut tick, Vec::new());
    }
    Ok(f64::from(config.ticks) / started.elapsed().as_secs_f64())
}

/// The minimum median ticks per second from `--floor <n>`, if given.
///
/// Cargo passes `--bench` to every benchmark binary; it is accepted and ignored. Any other
/// argument is an error, so a mistyped floor can never pass as no floor.
fn floor_from_args(args: impl IntoIterator<Item = String>) -> Result<Option<f64>, Box<dyn Error>> {
    let mut floor = None;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bench" => {}
            "--floor" => {
                let value = args
                    .next()
                    .ok_or("--floor needs a number of ticks per second")?;
                let ticks: u32 = value.parse().map_err(|_| {
                    format!("--floor {value}: not a whole number of ticks per second")
                })?;
                floor = Some(f64::from(ticks));
            }
            other => return Err(format!("unknown argument `{other}`; expected --floor <n>").into()),
        }
    }
    Ok(floor)
}

fn main() -> Result<(), Box<dyn Error>> {
    let floor = floor_from_args(std::env::args().skip(1))?;
    let config: BenchmarkConfig = ron::from_str(SCENARIO_TABLE)?;
    let catalogs = RuntimeCatalogs::load_from_directory(catalog_directory())?;
    println!("| Map | Units | Ticks/s (median of {}) |", config.repeats);
    println!("|---|---:|---:|");
    let mut below = Vec::new();
    for scenario in &config.scenarios {
        let mut samples = Vec::with_capacity(config.repeats);
        for _ in 0..config.repeats {
            samples.push(measure(&config, &catalogs, scenario)?);
        }
        samples.sort_by(f64::total_cmp);
        let median = samples
            .get(samples.len() / 2)
            .ok_or("scenario table must request at least one repeat")?;
        println!(
            "| {} | {} | {median:.0} |",
            scenario.label(),
            scenario.units
        );
        if floor.is_some_and(|floor| *median < floor) {
            below.push(format!(
                "{} with {} units at {median:.0} ticks/s",
                scenario.label(),
                scenario.units
            ));
        }
    }
    match floor {
        None => Ok(()),
        Some(floor) if below.is_empty() => {
            println!("\nEvery scenario meets the floor of {floor:.0} ticks/s.");
            Ok(())
        }
        Some(floor) => Err(format!(
            "below the floor of {floor:.0} ticks/s (charter §3.1): {}",
            below.join("; ")
        )
        .into()),
    }
}
