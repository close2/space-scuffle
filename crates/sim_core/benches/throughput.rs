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
//! - The unit identity is read from the data file; no archetype is named in Rust. A spell
//!   scenario names its casters and abilities in the data file too, and the measured ticks
//!   include its casts, issued as `PlayerCommand`s (plan `2026-11-10-spellcaster-mechanics`
//!   WP6).
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
    /// Spellcasters added to the first army on a flat map, each load casting on its own
    /// cadence at the second army (plan `2026-11-10-spellcaster-mechanics` WP6).
    #[serde(default)]
    spells: Vec<SpellLoad>,
    /// Ticks stepped, casts included, before the measured ticks, so a spell load is in its
    /// steady state when the clock starts.
    #[serde(default)]
    warm_ticks: u32,
}

/// One kind of spellcaster in a spell scenario: `casters` units of `caster_slug` for the
/// first player, which casts the point-targeted `ability` by name once every `every_ticks`
/// ticks, its casters taking turns, at the second army's centroid offset on a 5 × 5 grid of
/// `spread` cells.
#[derive(Debug, Clone, Deserialize)]
struct SpellLoad {
    /// Catalog slug of the caster.
    caster_slug: String,
    /// The ability's catalog name.
    ability: String,
    /// The research the ability needs, if any, granted to the first player.
    #[serde(default)]
    research: Option<String>,
    /// Number of casters.
    casters: u32,
    /// Start energy of every caster, overriding the catalog's (a scenario setting, as the
    /// matchup fixture's `Squad::energy` is).
    energy: u32,
    /// Ticks between two casts of this load.
    every_ticks: u32,
    /// Cells between neighbouring target points.
    spread: u32,
}

impl Scenario {
    fn label(&self) -> String {
        let map = self
            .map_file
            .clone()
            .unwrap_or_else(|| format!("{0}x{0}", self.map_cells));
        if self.spells.is_empty() {
            map
        } else {
            let casters: u32 = self.spells.iter().map(|load| load.casters).sum();
            format!("{map}, {casters} casters")
        }
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

/// Builds a battle-ready world with `catalogs`, returning it with the next tick to execute and
/// the second army.
fn build_world(
    config: &BenchmarkConfig,
    catalogs: &RuntimeCatalogs,
    scenario: &Scenario,
) -> Result<(SimWorld, u32, Vec<EntityId>), Box<dyn Error>> {
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
        vec![attack_move(
            1,
            red.clone(),
            position(ORDER_MARGIN, ORDER_MARGIN)?,
        )],
    );
    Ok((world, tick, red))
}

/// A spell scenario's mean live areas and statuses per measured tick.
type SpellLoadMeans = (f64, f64);

/// Rows below the first army's formation where spellcasters stand, from row 1.
const CASTER_ROWS: u32 = FORMATION_MARGIN - 1;

/// One spell load as it runs: its casters, the ability's index and whose turn it is.
struct LoadState {
    casters: Vec<EntityId>,
    ability_index: u8,
    every_ticks: u32,
    spread: i32,
    turn: usize,
    casts: i32,
}

/// Issues a spell scenario's casts and samples its live areas and statuses.
struct SpellDriver {
    loads: Vec<LoadState>,
    /// The second army, whose centroid the casts aim at.
    targets: Vec<EntityId>,
    /// Sums over the sampled ticks: live areas, live statuses, samples.
    sums: (usize, usize, u32),
}

impl SpellDriver {
    /// Spawns every load's casters for the first player below its formation, sets their
    /// energy and grants the research the abilities need.
    fn new(
        world: &mut SimWorld,
        scenario: &Scenario,
        targets: Vec<EntityId>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut loads = Vec::with_capacity(scenario.spells.len());
        let mut placed = 0;
        for load in &scenario.spells {
            if let Some(research) = &load.research {
                world.research_upgrade(PlayerId(0), research)?;
            }
            let kind = world
                .catalog
                .id_by_slug(&load.caster_slug)
                .ok_or("a spell load's caster_slug is not in the catalog")?;
            let ability_index = world
                .catalog
                .get(kind)?
                .abilities
                .iter()
                .position(|ability| ability.name == load.ability)
                .ok_or_else(|| format!("{} has no ability {}", load.caster_slug, load.ability))?;
            let mut casters = Vec::with_capacity(usize::try_from(load.casters)?);
            for _ in 0..load.casters {
                let column = placed % FORMATION_COLUMNS;
                let row = 1 + (placed / FORMATION_COLUMNS) % CASTER_ROWS;
                let spawn = position(FORMATION_MARGIN + column, row)?;
                let id = world.spawn_unit(kind, PlayerId(0), spawn)?;
                world
                    .entities
                    .get_mut(&id)
                    .ok_or("a spawned caster")?
                    .energy = scalar_from_cells(load.energy)?;
                casters.push(id);
                placed += 1;
            }
            loads.push(LoadState {
                casters,
                ability_index: u8::try_from(ability_index)?,
                every_ticks: load.every_ticks.max(1),
                spread: i32::try_from(load.spread)?,
                turn: 0,
                casts: 0,
            });
        }
        Ok(Self {
            loads,
            targets,
            sums: (0, 0, 0),
        })
    }

    /// The casts due on `tick`: for each load whose cadence falls on it, its next living
    /// caster casts at the second army's centroid plus the next offset of a 5 × 5 grid.
    fn commands(&mut self, world: &SimWorld, tick: u32) -> Vec<PlayerCommand> {
        let mut commands = Vec::new();
        let Some(centre) = centroid(world, &self.targets) else {
            return commands;
        };
        for load in &mut self.loads {
            if !tick.is_multiple_of(load.every_ticks) {
                continue;
            }
            let count = load.casters.len();
            let living = (0..count)
                .map(|step| (load.turn + step) % count)
                .find(|&index| world.entities.contains_key(&load.casters[index]));
            let Some(index) = living else {
                continue;
            };
            load.turn = index + 1;
            let offset = |slot: i32| SimScalar::from_i32((slot % 5 - 2) * load.spread);
            let target = SimVec2::new(
                centre.x.saturating_add(offset(load.casts)),
                centre.y.saturating_add(offset(load.casts / 5)),
            );
            load.casts += 1;
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![load.casters[index]],
                order: UnitOrder::CastAbility {
                    ability_index: load.ability_index,
                    target_pos: Some(target),
                    target_entity: None,
                },
                queue: false,
            });
        }
        commands
    }

    /// Adds this tick's live areas and statuses to the sums.
    fn sample(&mut self, world: &SimWorld) {
        let effects = world.effects();
        self.sums.0 += effects.persistents().len();
        self.sums.1 += effects.statuses().len();
        self.sums.2 += 1;
    }

    /// Mean live areas and statuses per sampled tick.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a reported mean of counts far below 2^52"
    )]
    fn means(&self) -> SpellLoadMeans {
        let samples = f64::from(self.sums.2.max(1));
        (self.sums.0 as f64 / samples, self.sums.1 as f64 / samples)
    }
}

/// The mean position of the living `units`, if any lives.
fn centroid(world: &SimWorld, units: &[EntityId]) -> Option<SimVec2> {
    let (mut x, mut y, mut count) = (SimScalar::ZERO, SimScalar::ZERO, 0_i32);
    for entity in units.iter().filter_map(|id| world.entities.get(id)) {
        x = x.saturating_add(entity.position.x);
        y = y.saturating_add(entity.position.y);
        count += 1;
    }
    (count > 0).then(|| {
        let count = SimScalar::from_i32(count);
        SimVec2::new(x.saturating_div(count), y.saturating_div(count))
    })
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
) -> Result<(SimWorld, u32, Vec<EntityId>), Box<dyn Error>> {
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
        vec![attack_move(1, red.clone(), centre(home)?)],
    );
    Ok((world, tick, red))
}

/// Measures one repeat, returning ticks per second and, for a spell scenario, the mean live
/// areas and statuses per measured tick.
#[expect(
    clippy::disallowed_methods,
    reason = "the benchmark times the simulation from outside; the clock never enters simulation state"
)]
fn measure(
    config: &BenchmarkConfig,
    catalogs: &RuntimeCatalogs,
    scenario: &Scenario,
) -> Result<(f64, Option<SpellLoadMeans>), Box<dyn Error>> {
    let (mut world, mut tick, red) = build_world(config, catalogs, scenario)?;
    if scenario.spells.is_empty() {
        let started = Instant::now();
        for _ in 0..config.ticks {
            step_with(&mut world, &mut tick, Vec::new());
        }
        return Ok((
            f64::from(config.ticks) / started.elapsed().as_secs_f64(),
            None,
        ));
    }
    if scenario.map_file.is_some() {
        return Err("a spell scenario needs a flat map".into());
    }
    let mut driver = SpellDriver::new(&mut world, scenario, red)?;
    for _ in 0..scenario.warm_ticks {
        let commands = driver.commands(&world, tick);
        step_with(&mut world, &mut tick, commands);
    }
    let started = Instant::now();
    for _ in 0..config.ticks {
        let commands = driver.commands(&world, tick);
        step_with(&mut world, &mut tick, commands);
        driver.sample(&world);
    }
    let rate = f64::from(config.ticks) / started.elapsed().as_secs_f64();
    Ok((rate, Some(driver.means())))
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
    let mut loads = Vec::new();
    for scenario in &config.scenarios {
        let mut samples = Vec::with_capacity(config.repeats);
        for _ in 0..config.repeats {
            let (rate, load) = measure(&config, &catalogs, scenario)?;
            samples.push(rate);
            if let Some((areas, statuses)) = load {
                loads.push(format!(
                    "{}: {areas:.1} live areas and {statuses:.1} statuses per measured tick",
                    scenario.label()
                ));
            }
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
    // The spell load of each repeat; the simulation is deterministic, so the repeats agree and
    // one line remains. Printed so a reader can check what a spell row measured.
    loads.dedup();
    for load in &loads {
        println!("\n{load}");
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
