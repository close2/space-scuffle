//! Golden cross-platform replay: per-checkpoint `SimWorld` checksums.
//!
//! # Responsibility
//! Replays `fixtures/golden_replay.ron` through the public tick API on the shipped default
//! arena and compares the full-state checksum at every checkpoint with the recorded list.
//!
//! # How it fits
//! This is the charter's cross-platform determinism claim made executable. CI runs this
//! same test on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, in debug and release, so
//! a divergence between targets or build profiles fails with the first differing tick.
//!
//! # Invariants
//! - The fixture and map are embedded at compile time: the test has no filesystem access,
//!   which is what lets it run under wasmtime with no preopened directories.
//! - Orders go through `PlayerCommand` only; no entity state is injected.
//! - The replay must contain a real fight, so a script that stops exercising combat fails
//!   rather than passing with a trivially stable checksum.
//! - The recorded schema must equal `CHECKSUM_SCHEMA_VERSION`, so a schema bump cannot pass
//!   without the fixture being deliberately re-recorded.

use serde::Deserialize;
use sim_core::checksum::CHECKSUM_SCHEMA_VERSION;
use sim_core::SimWorld;
use sim_map::MapFile;
use sim_types::{
    EntityId, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
};
use std::collections::BTreeMap;
use std::error::Error;

const FIXTURE: &str = include_str!("fixtures/golden_replay.ron");
const MAP: &str = include_str!("../../../assets/maps/default_arena.scmap.ron");

/// Decoded replay script and its recorded checksums.
#[derive(Debug, Deserialize)]
struct GoldenReplay {
    checksum_schema: u32,
    seed: u64,
    total_ticks: u32,
    checkpoint_interval: u32,
    squads: Vec<Squad>,
    research: Vec<CompletedResearch>,
    structures: Vec<StructureSpawn>,
    orders: Vec<ScriptedOrder>,
    checksums: Vec<u64>,
}

/// A named rectangular group of identical units.
#[derive(Debug, Deserialize)]
struct Squad {
    name: String,
    player: u8,
    slug: String,
    count: u32,
    origin: (i32, i32),
    columns: u32,
}

/// One upgrade completed before the replay starts.
#[derive(Debug, Deserialize)]
struct CompletedResearch {
    player: u8,
    upgrade: String,
}

/// One structure placed before the replay starts.
#[derive(Debug, Deserialize)]
struct StructureSpawn {
    player: u8,
    slug: String,
    cell: (u32, u32),
}

/// Orders issued to whole squads at a fixed tick.
#[derive(Debug, Deserialize)]
struct ScriptedOrder {
    tick: u32,
    squads: Vec<String>,
    order: ScriptedOrderKind,
}

/// The subset of `UnitOrder` the replay exercises, in whole world units.
#[derive(Debug, Clone, Copy, Deserialize)]
enum ScriptedOrderKind {
    Move(i32, i32),
    AttackMove(i32, i32),
    HoldPosition,
    Stop,
    UseStim,
}

impl ScriptedOrderKind {
    fn to_unit_order(self) -> UnitOrder {
        let point = |x, y| SimVec2::from_i32(x, y);
        match self {
            Self::Move(x, y) => UnitOrder::Move {
                target: point(x, y),
            },
            Self::AttackMove(x, y) => UnitOrder::AttackMove {
                target: point(x, y),
            },
            Self::HoldPosition => UnitOrder::HoldPosition,
            Self::Stop => UnitOrder::Stop,
            Self::UseStim => UnitOrder::UseStim,
        }
    }
}

/// Owning player and unit identities of each named squad.
type SquadRoster = BTreeMap<String, (u8, Vec<EntityId>)>;

/// Spawns the script's forces and returns the unit identities of each named squad.
fn populate(world: &mut SimWorld, replay: &GoldenReplay) -> Result<SquadRoster, Box<dyn Error>> {
    let half = SimScalar::HALF;
    let mut squads = BTreeMap::new();
    for squad in &replay.squads {
        let unit_type = world
            .catalog
            .id_by_slug(&squad.slug)
            .ok_or_else(|| format!("unknown fixture unit {}", squad.slug))?;
        let mut members = Vec::new();
        for index in 0..squad.count {
            let column = i32::try_from(index % squad.columns)?;
            let row = i32::try_from(index / squad.columns)?;
            let spawn = SimVec2::from_i32(squad.origin.0 + column, squad.origin.1 + row);
            let spawn = SimVec2::new(spawn.x + half, spawn.y + half);
            members.push(world.spawn_unit(unit_type, PlayerId(squad.player), spawn)?);
        }
        squads.insert(squad.name.clone(), (squad.player, members));
    }
    for research in &replay.research {
        world.research_upgrade(PlayerId(research.player), &research.upgrade)?;
    }
    for structure in &replay.structures {
        let structure_type = world
            .catalog
            .id_by_slug(&structure.slug)
            .ok_or_else(|| format!("unknown fixture structure {}", structure.slug))?;
        world.spawn_structure(
            structure_type,
            PlayerId(structure.player),
            structure.cell.0,
            structure.cell.1,
        )?;
    }
    Ok(squads)
}

/// Runs the script, returning the checksum recorded at each checkpoint.
fn replay_checksums(replay: &GoldenReplay) -> Result<(Vec<u64>, SimWorld), Box<dyn Error>> {
    let map = MapFile::from_ron_str(MAP)?.decode()?.to_game_map()?;
    let mut world = sim_fixtures::world(map, replay.seed);
    let squads = populate(&mut world, replay)?;

    let mut checksums = Vec::new();
    for tick in 0..replay.total_ticks {
        let mut commands = Vec::new();
        for scripted in replay.orders.iter().filter(|order| order.tick == tick) {
            for name in &scripted.squads {
                let (player, members) = squads
                    .get(name)
                    .ok_or_else(|| format!("order names unknown squad {name}"))?;
                commands.push(PlayerCommand {
                    player_id: PlayerId(*player),
                    entities: members.clone(),
                    order: scripted.order.to_unit_order(),
                    queue: false,
                });
            }
        }
        world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
        let diagnostics = world.drain_diagnostics().collect::<Vec<_>>();
        if !diagnostics.is_empty() {
            return Err(format!("tick {tick} diagnostics: {diagnostics:?}").into());
        }
        if (tick + 1) % replay.checkpoint_interval == 0 {
            checksums.push(world.last_checksum());
        }
    }
    Ok((checksums, world))
}

fn render_checksum_list(checksums: &[u64]) -> String {
    let lines: Vec<String> = checksums
        .iter()
        .map(|checksum| format!("        {checksum:#018x},"))
        .collect();
    format!("    checksums: [\n{}\n    ],", lines.join("\n"))
}

#[test]
fn golden_replay_reproduces_every_checkpoint() -> Result<(), Box<dyn Error>> {
    let replay: GoldenReplay = ron::from_str(FIXTURE)?;
    assert_eq!(
        replay.checksum_schema, CHECKSUM_SCHEMA_VERSION,
        "checksum schema changed: re-record the fixture in the commit that explains why"
    );

    let (actual, world) = replay_checksums(&replay)?;
    let first_difference = actual
        .iter()
        .zip(&replay.checksums)
        .position(|(left, right)| left != right);
    assert!(
        actual == replay.checksums,
        "golden replay diverged at checkpoint {first_difference:?} (interval {}).\n\
         A deliberate simulation change must explain itself in the commit that replaces \
         the list below. An unexplained change, or one on a single target, is a determinism \
         defect.\n{}",
        replay.checkpoint_interval,
        render_checksum_list(&actual),
    );

    let expected_checkpoints = replay.total_ticks / replay.checkpoint_interval;
    assert_eq!(actual.len(), usize::try_from(expected_checkpoints)?);

    // A replay in which nothing dies would stay green while proving little about combat.
    let spawned: u32 = replay.squads.iter().map(|squad| squad.count).sum();
    let structures = u32::try_from(replay.structures.len())?;
    let survivors = u32::try_from(world.entities.len())?;
    assert!(
        survivors < spawned + structures,
        "fixture must contain real combat: all {survivors} entities survived"
    );
    Ok(())
}

#[test]
fn golden_replay_is_stable_within_one_process() -> Result<(), Box<dyn Error>> {
    let mut replay: GoldenReplay = ron::from_str(FIXTURE)?;
    replay.total_ticks = replay.checkpoint_interval * 4;
    let (first, _) = replay_checksums(&replay)?;
    let (second, _) = replay_checksums(&replay)?;
    assert_eq!(first, second, "replaying the same script must not diverge");
    Ok(())
}
