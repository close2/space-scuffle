//! Golden cross-platform replays: per-checkpoint `SimWorld` checksums.
//!
//! # Responsibility
//! Replays each embedded fixture through the public tick API on the shipped default arena
//! and compares the full-state checksum at every checkpoint with the recorded list:
//! - `fixtures/golden_replay.ron`: a long, broad battle (stim, structures, hold, stop).
//! - `fixtures/golden_replay_orders.ron`: a short script that pins two phase orders of
//!   `SimWorld::step_into` the broad battle does not: vision before target acquisition, and
//!   route upkeep before the arrival decision (Phase 9 §9 gap 4). Swapping either pair makes
//!   it diverge at a named checkpoint.
//!
//! # How it fits
//! This is the charter's cross-platform determinism claim made executable. CI runs this
//! same test file on Linux, Windows, macOS ARM64 and `wasm32-wasip1`, in debug and release,
//! so a divergence between targets or build profiles fails with the first differing tick.
//!
//! # Invariants
//! - Fixtures and map are embedded at compile time: the test has no filesystem access,
//!   which is what lets it run under wasmtime with no preopened directories.
//! - Orders go through `PlayerCommand` only; no entity state is injected.
//! - Every replay must contain a real fight, so a script that stops exercising combat fails
//!   rather than passing with a trivially stable checksum.
//! - The recorded schema must equal `CHECKSUM_SCHEMA_VERSION`, so a schema bump cannot pass
//!   without the fixtures being deliberately re-recorded.
//! - The phase-order fixture must keep containing the situation each order decides, observed
//!   from the replay itself, so an edit to the script cannot quietly drop it.

use serde::Deserialize;
use sim_core::checksum::CHECKSUM_SCHEMA_VERSION;
use sim_core::{Entity, SimWorld};
use sim_map::MapFile;
use sim_types::{
    EntityId, PlayerCommand, PlayerId, PlayerSlot, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
    VisibilityState,
};
use std::collections::BTreeMap;
use std::error::Error;

const FIXTURE: &str = include_str!("fixtures/golden_replay.ron");
const ORDERS_FIXTURE: &str = include_str!("fixtures/golden_replay_orders.ron");
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

/// Watches a replay tick by tick, through the public world state only.
trait ReplayObserver {
    /// Called with the world as it stands before each tick is stepped.
    fn before_step(&mut self, world: &SimWorld) -> Result<(), Box<dyn Error>>;
    /// Called with the world after each tick, once its diagnostics are known to be empty.
    fn after_step(&mut self, world: &SimWorld) -> Result<(), Box<dyn Error>>;
}

/// Observer for replays that only compare checksums.
struct ChecksumsOnly;

impl ReplayObserver for ChecksumsOnly {
    fn before_step(&mut self, _world: &SimWorld) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn after_step(&mut self, _world: &SimWorld) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

/// One entity as it stood before a tick, as far as the phase-order evidence needs it.
#[derive(Debug, Clone, Copy)]
struct Glimpse {
    /// Target the entity was engaging.
    target: Option<EntityId>,
    /// Bit `n` is set when player slot `n` had the entity's cell `Visible`.
    seen_by: u8,
    /// Position in world units.
    position: SimVec2,
    /// Goal of the `Move` order at the front of its queue, in world units.
    move_goal: Option<SimVec2>,
    /// Number of waypoints left in its stored route.
    route_len: usize,
}

impl Glimpse {
    fn of(world: &SimWorld, entity: &Entity) -> Result<Self, Box<dyn Error>> {
        let (x, y) = world.map.world_to_cell(entity.position)?;
        let mut seen_by = 0_u8;
        for slot in PlayerSlot::ALL {
            if world.vision_maps[slot].visibility_at(x, y) == VisibilityState::Visible {
                seen_by |= 1 << slot.index();
            }
        }
        Ok(Self {
            target: entity.current_target,
            seen_by,
            position: entity.position,
            move_goal: move_goal(entity),
            route_len: entity.current_funnel_waypoints.len(),
        })
    }

    fn seen_by(self, player: PlayerId) -> bool {
        player
            .slot()
            .is_some_and(|slot| self.seen_by & (1 << slot.index()) != 0)
    }
}

/// Goal of the `Move` order at the front of `entity`'s queue, in world units.
fn move_goal(entity: &Entity) -> Option<SimVec2> {
    match entity.order_queue.front() {
        Some(UnitOrder::Move { target }) => Some(*target),
        _ => None,
    }
}

/// Evidence, read from the replay itself, that the phase-order fixture still contains the
/// situations its two pinned orders decide.
///
/// - **Reveal acquisitions.** A unit that holds a new target after a tick, where that target's
///   cell was not `Visible` to the unit's owner before the tick, can only have acquired it
///   because this tick's vision update ran first. With acquisition before vision the unit
///   acquires one tick later, when the snapshot taken before that tick already shows the
///   target, so nothing is counted: this check is itself order-specific.
/// - **Mid-route repaths.** A unit still walking the same `Move` order whose stored route was
///   empty before the tick (dropped after a stall in a crowd) and holds a new route after it,
///   while still more than [`MID_ROUTE_DISTANCE`] from its goal. Route upkeep running before
///   the arrival decision is what re-routes such a unit; in the other order the arrival
///   decision sees the empty route as a last leg and may end the order beside a settled
///   friend. This check only proves the situation occurs; that the fixture diverges when the
///   order is swapped is proved by the mutation runs recorded with WP5.
#[derive(Debug, Default)]
struct PhaseOrderEvidence {
    before: BTreeMap<EntityId, Glimpse>,
    /// Acquisitions of a target its owner first saw during the same tick.
    reveal_acquisitions: u32,
    /// Units re-routed, mid-order and away from their goal, after their route was dropped.
    mid_route_repaths: u32,
}

/// Distance from its goal, in world units, beyond which a re-routed unit counts as mid-route.
const MID_ROUTE_DISTANCE: i32 = 2;

impl ReplayObserver for PhaseOrderEvidence {
    fn before_step(&mut self, world: &SimWorld) -> Result<(), Box<dyn Error>> {
        self.before.clear();
        for (id, entity) in &world.entities {
            self.before.insert(*id, Glimpse::of(world, entity)?);
        }
        Ok(())
    }

    fn after_step(&mut self, world: &SimWorld) -> Result<(), Box<dyn Error>> {
        for (id, entity) in &world.entities {
            let Some(was) = self.before.get(id) else {
                continue;
            };
            if let Some(target) = entity.current_target {
                let newly_hidden_target = was.target != Some(target)
                    && self
                        .before
                        .get(&target)
                        .is_some_and(|glimpse| !glimpse.seen_by(entity.owner));
                if newly_hidden_target {
                    self.reveal_acquisitions += 1;
                }
            }
            if let (Some(goal), Some(still)) = (was.move_goal, move_goal(entity)) {
                let rerouted = goal == still
                    && was.route_len == 0
                    && !entity.current_funnel_waypoints.is_empty()
                    && was.position.distance(goal) > SimScalar::from_num(MID_ROUTE_DISTANCE);
                if rerouted {
                    self.mid_route_repaths += 1;
                }
            }
        }
        Ok(())
    }
}

/// Runs the script, returning the checksum recorded at each checkpoint.
fn replay_checksums(
    replay: &GoldenReplay,
    observer: &mut dyn ReplayObserver,
) -> Result<(Vec<u64>, SimWorld), Box<dyn Error>> {
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
        observer.before_step(&world)?;
        world.step(&TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        });
        let diagnostics = world.drain_diagnostics().collect::<Vec<_>>();
        if !diagnostics.is_empty() {
            return Err(format!("tick {tick} diagnostics: {diagnostics:?}").into());
        }
        observer.after_step(&world)?;
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

/// Replays `source` and asserts every invariant shared by all fixtures: the schema, each
/// checkpoint's checksum, the checkpoint count, and a real fight. `name` labels the failure
/// messages. Returns the final world for fixture-specific checks.
fn assert_reproduces(
    name: &str,
    source: &str,
    observer: &mut dyn ReplayObserver,
) -> Result<SimWorld, Box<dyn Error>> {
    let replay: GoldenReplay = ron::from_str(source)?;
    assert_eq!(
        replay.checksum_schema, CHECKSUM_SCHEMA_VERSION,
        "{name}: checksum schema changed: re-record the fixture in the commit that explains why"
    );

    let (actual, world) = replay_checksums(&replay, observer)?;
    let first_difference = actual
        .iter()
        .zip(&replay.checksums)
        .position(|(left, right)| left != right)
        .or_else(|| {
            (actual.len() != replay.checksums.len())
                .then(|| actual.len().min(replay.checksums.len()))
        });
    let interval = u64::from(replay.checkpoint_interval);
    let last_tick_of_first_difference = first_difference
        .map(|index| u64::try_from(index + 1).map(|checkpoint| checkpoint * interval))
        .transpose()?;
    assert!(
        actual == replay.checksums,
        "{name} diverged at checkpoint {first_difference:?} (ticks up to \
         {last_tick_of_first_difference:?}, interval {interval}).\n\
         A deliberate simulation change must explain itself in the commit that replaces \
         the list below. An unexplained change, or one on a single target, is a determinism \
         defect.\n{}",
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
        "{name} must contain real combat: all {survivors} entities survived"
    );
    Ok(world)
}

#[test]
fn golden_replay_reproduces_every_checkpoint() -> Result<(), Box<dyn Error>> {
    assert_reproduces("golden_replay", FIXTURE, &mut ChecksumsOnly)?;
    Ok(())
}

#[test]
fn golden_replay_orders_reproduces_every_checkpoint() -> Result<(), Box<dyn Error>> {
    let mut evidence = PhaseOrderEvidence::default();
    assert_reproduces("golden_replay_orders", ORDERS_FIXTURE, &mut evidence)?;
    assert!(
        evidence.reveal_acquisitions > 0,
        "golden_replay_orders must contain a same-tick reveal and acquisition: no unit acquired \
         a target its owner could not see before that tick"
    );
    assert!(
        evidence.mid_route_repaths > 0,
        "golden_replay_orders must contain a mid-route repath: no unit walking a Move order \
         had its dropped route searched again away from its goal"
    );
    Ok(())
}

#[test]
fn golden_replay_is_stable_within_one_process() -> Result<(), Box<dyn Error>> {
    for source in [FIXTURE, ORDERS_FIXTURE] {
        let mut replay: GoldenReplay = ron::from_str(source)?;
        replay.total_ticks = replay.checkpoint_interval * 4;
        let (first, _) = replay_checksums(&replay, &mut ChecksumsOnly)?;
        let (second, _) = replay_checksums(&replay, &mut ChecksumsOnly)?;
        assert_eq!(first, second, "replaying the same script must not diverge");
    }
    Ok(())
}
