//! A whole match, played from shipped data to its end through player commands alone.
//!
//! # Responsibility
//! Proves the game loop closes. From the real skirmish scenario one side gathers, raises a
//! building, trains an army and destroys the other, and the match ends with a defeated player
//! and a winner. A second game plays the tech path: gas, an extractor, a prerequisite module
//! and research, ending in a stimmed unit that could not stim before.
//!
//! # How it fits
//! This is plan step 1.9, the exit test of Phase 5 on the simulation side. The opponent has no
//! AI, because there is no bot: its workers stand idle and defend themselves when attacked.
//!
//! # Invariants
//! - Scenario, map, loadouts and catalogs are the shipped files, embedded at compile time.
//! - After setup the world changes only through `TickBundle` commands. The script reads the
//!   world to decide what to do next, as a player reads the screen, and writes nothing.
//! - Every command the script issues must be accepted, and no tick may raise a diagnostic.

use sim_core::economy::{GameCommandError, GameCommandOutcome, TaskEventKind};
use sim_core::production::ProductionItem;
use sim_core::scenario::{build_world_with_catalogs, ScenarioManifest, StartLoadoutCatalog};
use sim_core::{SimWorld, UnitCatalog, UpgradeCatalog};
use sim_map::MapFile;
use sim_types::{
    EntityId, MatchResult, PlayerCommand, PlayerId, RenderSnapshot, ResourceKind, ResourceNodeId,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const SCENARIO: &str = include_str!("../../../assets/scenarios/skirmish.ron");
const MAP: &str = include_str!("../../../assets/maps/showcase_canyon.scmap.ron");
const LOADOUTS: &str = include_str!("../../../assets/catalog/start_loadouts.ron");
const UNITS: &str = include_str!("../../../assets/catalog/units.ron");
const STRUCTURES: &str = include_str!("../../../assets/catalog/structures.ron");
const UPGRADES: &str = include_str!("../../../assets/catalog/upgrades.ron");

const HOME: PlayerId = PlayerId(0);
const AWAY: PlayerId = PlayerId(1);
const ARMY_SIZE: usize = 6;
const TICK_LIMIT: u32 = 12_000;

/// Minimum-corner cells of the home player's buildings, on open plateau clear of its nodes.
const TRAINING_SITE: (u32, u32) = (2, 2);
const MODULE_SITE: (u32, u32) = (2, 7);
/// A 3x3 extractor here covers the gas node at (14, 6).
const EXTRACTOR_SITE: (u32, u32) = (13, 5);

fn skirmish(seed: u64) -> Result<SimWorld, Box<dyn Error>> {
    let manifest = ScenarioManifest::from_ron_str(SCENARIO)?;
    let document = MapFile::from_ron_str(MAP)?.decode()?;
    let loadouts = StartLoadoutCatalog::from_ron_str(LOADOUTS)?;
    Ok(build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        seed,
        UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?,
        UpgradeCatalog::from_ron_str(UPGRADES)?,
    )?)
}

fn type_of(world: &SimWorld, slug: &str) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world.catalog.id_by_slug(slug).ok_or(slug.to_owned())?)
}

fn command(entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: HOME,
        entities,
        order,
        queue: false,
    }
}

fn build_order(unit_type: UnitTypeId, (cell_x, cell_y): (u32, u32)) -> UnitOrder {
    UnitOrder::Build {
        structure_type: unit_type,
        cell_x,
        cell_y,
    }
}

/// What the home player has decided, in the order it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    OpenMining,
    RaiseTraining,
    RaiseExtractor,
    MineGas,
    RaiseModule,
    Research,
    TrainArmy,
    March,
    Assault,
}

/// The home player's script: reads the world, and answers with commands.
struct Script {
    stage: Stage,
    /// Whether to take the gas and research path before training.
    tech: bool,
    army_size: usize,
    workers: Vec<EntityId>,
    training: UnitTypeId,
    module: UnitTypeId,
    extractor: UnitTypeId,
    soldier: UnitTypeId,
    stim_upgrade: u32,
}

impl Script {
    fn new(world: &SimWorld, tech: bool) -> Result<Self, Box<dyn Error>> {
        let upgrade = world
            .upgrade_catalog
            .get_by_slug("overclock_stim")
            .ok_or("shipped upgrade")?;
        Ok(Self {
            stage: Stage::OpenMining,
            tech,
            army_size: if tech { 1 } else { ARMY_SIZE },
            workers: world
                .entities
                .values()
                .filter(|e| e.owner == HOME && !e.is_immobile)
                .map(|e| e.id)
                .collect(),
            training: type_of(world, "assembly_bay_dome")?,
            module: type_of(world, "tech_module_pod")?,
            extractor: type_of(world, "plasma_siphon_dome")?,
            soldier: type_of(world, "pulse_bot")?,
            stim_upgrade: upgrade.upgrade_id,
        })
    }

    fn own_building(world: &SimWorld, unit_type: UnitTypeId) -> Option<EntityId> {
        world
            .entities
            .values()
            .find(|e| e.owner == HOME && e.unit_type_id == unit_type)
            .map(|e| e.id)
    }

    fn finished(world: &SimWorld, unit_type: UnitTypeId) -> Option<EntityId> {
        Self::own_building(world, unit_type).filter(|id| world.entities[id].is_completed)
    }

    fn afford(world: &SimWorld, unit_type: UnitTypeId) -> bool {
        world.catalog.get(unit_type).is_ok_and(|arch| {
            let bank = &world.player_resources[0];
            bank.minerals >= arch.mineral_cost && bank.gas >= arch.gas_cost
        })
    }

    fn home_nodes(world: &SimWorld, kind: ResourceKind) -> Vec<ResourceNodeId> {
        (0_u32..)
            .zip(world.map.resources())
            .filter(|(_, node)| node.kind == kind && node.cell.0 < 32 && node.cell.1 < 32)
            .map(|(index, _)| ResourceNodeId(index))
            .collect()
    }

    fn soldiers(&self, world: &SimWorld) -> Vec<EntityId> {
        world
            .entities
            .values()
            .filter(|e| e.owner == HOME && e.unit_type_id == self.soldier)
            .map(|e| e.id)
            .collect()
    }

    fn enemy_hall(world: &SimWorld) -> Option<(EntityId, SimVec2)> {
        world
            .entities
            .values()
            .find(|e| e.owner == AWAY && world.map.structures().contains_key(&e.id))
            .map(|e| (e.id, e.position))
    }

    fn research_done(world: &SimWorld) -> bool {
        world.player_upgrades[0]
            .completed_upgrade_slugs
            .contains("overclock_stim")
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one player's build order, read top to bottom"
    )]
    fn commands(&mut self, world: &SimWorld) -> Vec<PlayerCommand> {
        match self.stage {
            Stage::OpenMining => {
                let nodes = Self::home_nodes(world, ResourceKind::Mineral);
                self.stage = Stage::RaiseTraining;
                self.workers
                    .iter()
                    .zip(&nodes)
                    .map(|(worker, node)| {
                        command(vec![*worker], UnitOrder::Harvest { target: *node })
                    })
                    .collect()
            }
            Stage::RaiseTraining => {
                if Self::own_building(world, self.training).is_some() {
                    return Vec::new();
                }
                if !Self::afford(world, self.training) {
                    return Vec::new();
                }
                self.stage = if self.tech {
                    Stage::RaiseExtractor
                } else {
                    Stage::TrainArmy
                };
                vec![
                    command(
                        vec![self.workers[0]],
                        build_order(self.training, TRAINING_SITE),
                    ),
                    PlayerCommand {
                        queue: true,
                        ..command(
                            vec![self.workers[0]],
                            UnitOrder::Harvest {
                                target: Self::home_nodes(world, ResourceKind::Mineral)[0],
                            },
                        )
                    },
                ]
            }
            Stage::RaiseExtractor => {
                if Self::finished(world, self.training).is_none()
                    || !Self::afford(world, self.extractor)
                {
                    return Vec::new();
                }
                self.stage = Stage::MineGas;
                vec![command(
                    vec![self.workers[0]],
                    build_order(self.extractor, EXTRACTOR_SITE),
                )]
            }
            Stage::MineGas => {
                if Self::finished(world, self.extractor).is_none() {
                    return Vec::new();
                }
                self.stage = Stage::RaiseModule;
                let gas = Self::home_nodes(world, ResourceKind::Gas)
                    .into_iter()
                    .find(|node| world.map.resources()[node.0 as usize].cell == (14, 6));
                gas.map_or_else(Vec::new, |target| {
                    vec![command(
                        self.workers[..3].to_vec(),
                        UnitOrder::Harvest { target },
                    )]
                })
            }
            Stage::RaiseModule => {
                if !Self::afford(world, self.module) {
                    return Vec::new();
                }
                self.stage = Stage::Research;
                vec![
                    command(vec![self.workers[3]], build_order(self.module, MODULE_SITE)),
                    PlayerCommand {
                        queue: true,
                        ..command(
                            vec![self.workers[3]],
                            UnitOrder::Harvest {
                                target: Self::home_nodes(world, ResourceKind::Mineral)[3],
                            },
                        )
                    },
                ]
            }
            Stage::Research => {
                let barracks = Self::finished(world, self.training);
                if self.soldiers(world).is_empty() {
                    // Train the proof subject first: the queue is first in, first out.
                    let idle = barracks
                        .filter(|id| world.entities[id].production_queue.is_empty())
                        .filter(|_| Self::afford(world, self.soldier));
                    return idle.map_or_else(Vec::new, |id| {
                        vec![command(
                            vec![id],
                            UnitOrder::Produce {
                                unit_type: self.soldier,
                            },
                        )]
                    });
                }
                let (Some(lab), Some(_)) = (
                    Self::finished(world, self.training),
                    Self::finished(world, self.module),
                ) else {
                    return Vec::new();
                };
                let cost = world.upgrade_catalog.get_by_id(self.stim_upgrade);
                let bank = &world.player_resources[0];
                if !cost.is_some_and(|c| bank.minerals >= c.mineral_cost && bank.gas >= c.gas_cost)
                {
                    return Vec::new();
                }
                self.stage = Stage::TrainArmy;
                vec![command(
                    vec![lab],
                    UnitOrder::Research {
                        upgrade: self.stim_upgrade,
                    },
                )]
            }
            Stage::TrainArmy => {
                let soldiers = self.soldiers(world).len();
                let Some(barracks) = Self::finished(world, self.training) else {
                    return Vec::new();
                };
                let queued = world.entities[&barracks]
                    .production_queue
                    .iter()
                    .filter(|job| matches!(job.item, ProductionItem::Unit(_)))
                    .count();
                if soldiers >= self.army_size {
                    if !self.tech {
                        self.stage = Stage::March;
                    } else if Self::research_done(world) {
                        self.stage = Stage::Assault;
                    }
                    return Vec::new();
                }
                if soldiers + queued < self.army_size
                    && queued < 2
                    && Self::afford(world, self.soldier)
                {
                    return vec![command(
                        vec![barracks],
                        UnitOrder::Produce {
                            unit_type: self.soldier,
                        },
                    )];
                }
                Vec::new()
            }
            Stage::March => {
                self.stage = Stage::Assault;
                let Some((_, hall)) = Self::enemy_hall(world) else {
                    return Vec::new();
                };
                let target = SimVec2::new(hall.x - SimScalar::from_num(6), hall.y);
                vec![command(
                    self.soldiers(world),
                    UnitOrder::AttackMove { target },
                )]
            }
            Stage::Assault if self.tech => {
                vec![command(self.soldiers(world), UnitOrder::UseStim)]
            }
            Stage::Assault => {
                let Some((hall_id, hall)) = Self::enemy_hall(world) else {
                    return Vec::new();
                };
                let soldiers = self.soldiers(world);
                let near = soldiers
                    .iter()
                    .any(|id| world.entities[id].position.distance(hall) < SimScalar::from_num(9));
                let on_target = soldiers.iter().all(|id| {
                    matches!(
                        world.entities[id].order_queue.front(),
                        Some(UnitOrder::AttackTarget { target, .. }) if *target == hall_id
                    )
                });
                // Clear the defenders first: a focused army does not answer them.
                let defended = world
                    .entities
                    .values()
                    .any(|e| e.owner == AWAY && !e.is_immobile);
                if near && !on_target && !defended {
                    vec![command(soldiers, UnitOrder::attack(hall_id))]
                } else {
                    Vec::new()
                }
            }
        }
    }
}

/// Everything a finished game leaves behind for comparison.
struct Played {
    world: SimWorld,
    last: RenderSnapshot,
    checksums: Vec<u64>,
}

/// Plays one tick of the script and holds the world to the rules of this test.
fn advance(
    world: &mut SimWorld,
    script: &mut Script,
    last: &mut RenderSnapshot,
) -> Result<(), String> {
    if world.current_tick >= TICK_LIMIT {
        return Err(format!(
            "stalled at {:?} with {:?}",
            script.stage, world.player_resources[0]
        ));
    }
    let bundle = TickBundle {
        tick: Tick(world.current_tick),
        commands: script.commands(world),
        state_checksum: None,
    };
    world.step_into(&bundle, last);
    if !world.diagnostics().is_empty() {
        return Err(format!(
            "tick {}: {:?}",
            world.current_tick,
            world.diagnostics()
        ));
    }
    for outcome in world.game_command_outcomes() {
        outcome
            .result
            .as_ref()
            .map_err(|e| format!("{outcome:?}: {e}"))?;
    }
    for outcome in world.production_outcomes() {
        outcome
            .result
            .as_ref()
            .map_err(|e| format!("{outcome:?}: {e}"))?;
    }
    for outcome in world.movement_outcomes() {
        outcome
            .result
            .as_ref()
            .map_err(|e| format!("{outcome:?}: {e}"))?;
    }
    for event in world.task_events() {
        if !matches!(event.kind, TaskEventKind::StructureCompleted { .. }) {
            return Err(format!("{event:?}"));
        }
    }
    Ok(())
}

fn play(seed: u64) -> Result<Played, String> {
    let mut world = skirmish(seed).map_err(|e| e.to_string())?;
    let mut script = Script::new(&world, false).map_err(|e| e.to_string())?;
    let mut last = RenderSnapshot::default();
    let mut checksums = Vec::new();
    while !world.match_state.result.is_over() {
        advance(&mut world, &mut script, &mut last)?;
        if world.current_tick.is_multiple_of(250) {
            checksums.push(world.last_checksum());
        }
    }
    checksums.push(world.last_checksum());
    Ok(Played {
        world,
        last,
        checksums,
    })
}

/// Plays each seed twice, in parallel, since one match takes half a minute in a debug build.
fn play_pairs(seeds: &[u64]) -> Result<Vec<(Played, Played)>, Box<dyn Error>> {
    let pairs = std::thread::scope(|scope| {
        let runs: Vec<_> = seeds
            .iter()
            .map(|&seed| {
                (
                    scope.spawn(move || play(seed)),
                    scope.spawn(move || play(seed)),
                )
            })
            .collect();
        runs.into_iter()
            .map(|(first, second)| {
                let join = |run: std::thread::ScopedJoinHandle<'_, Result<Played, String>>| {
                    run.join().map_err(|_| "match thread panicked".to_owned())?
                };
                Ok::<_, String>((join(first)?, join(second)?))
            })
            .collect::<Result<Vec<_>, String>>()
    })?;
    Ok(pairs)
}

#[test]
fn one_side_gathers_builds_trains_and_destroys_the_other() -> Result<(), Box<dyn Error>> {
    let manifest = ScenarioManifest::from_ron_str(SCENARIO)?;
    let pairs = play_pairs(&[manifest.seed, 0xC0FF_EE00])?;
    for (first, second) in &pairs {
        assert_eq!(
            first.checksums, second.checksums,
            "same seed and commands must give the same match"
        );
        let world = &first.world;
        let MatchResult::Victory { winner, tick } = world.match_state.result else {
            return Err(format!("no victory: {:?}", world.match_state.result).into());
        };
        assert_eq!(winner, HOME);
        assert_eq!(tick.0, world.current_tick);
        assert!(world.match_state.is_defeated(AWAY));
        assert!(!world.match_state.is_defeated(HOME));

        assert_eq!(first.last.match_result, world.match_state.result);
        assert!(first.last.players[1].defeated);
        assert!(!first.last.players[0].defeated);
        assert!(!world
            .entities
            .values()
            .any(|e| e.owner == AWAY && world.map.structures().contains_key(&e.id)));
        world.validate_supply_ledger()?;
    }
    assert_ne!(
        pairs[0].0.world.last_checksum(),
        pairs[1].0.world.last_checksum(),
        "the seed is part of the state"
    );
    let (mut ended, _) = pairs.into_iter().next().ok_or("two seeds were played")?;
    refuses_commands_after_the_end(&mut ended)
}

fn refuses_commands_after_the_end(played: &mut Played) -> Result<(), Box<dyn Error>> {
    let world = &mut played.world;
    let worker = world
        .entities
        .values()
        .find(|e| e.owner == HOME && !e.is_immobile)
        .map(|e| e.id)
        .ok_or("home unit")?;
    let tick = world.current_tick;
    let result = world.match_state.result;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands: vec![command(vec![worker], UnitOrder::HoldPosition)],
        state_checksum: None,
    });
    assert!(matches!(
        world.game_command_outcomes(),
        [GameCommandOutcome {
            result: Err(GameCommandError::MatchOver),
            ..
        }]
    ));
    assert_ne!(
        world.entities[&worker].order_queue.front(),
        Some(&UnitOrder::HoldPosition)
    );
    assert_eq!(world.current_tick, tick + 1, "time still passes");
    assert_eq!(world.match_state.result, result);
    Ok(())
}

#[test]
fn stim_is_locked_until_researched_through_the_whole_tech_path() -> Result<(), Box<dyn Error>> {
    let seed = ScenarioManifest::from_ron_str(SCENARIO)?.seed;
    let mut world = skirmish(seed)?;
    let mut script = Script::new(&world, true)?;
    let mut last = RenderSnapshot::default();
    let soldier = script.soldier;
    let stimmed = |world: &SimWorld| {
        world
            .entities
            .values()
            .any(|e| e.unit_type_id == soldier && !e.active_buffs.is_empty())
    };

    // Before research finishes, the same order on the same unit does nothing.
    let mut refused = false;
    while !stimmed(&world) {
        advance(&mut world, &mut script, &mut last)?;
        let trained = script.soldiers(&world);
        if !refused && !trained.is_empty() {
            refused = true;
            world.step(&TickBundle {
                tick: Tick(world.current_tick),
                commands: vec![command(trained, UnitOrder::UseStim)],
                state_checksum: None,
            });
            assert!(!stimmed(&world), "stim must be locked before research");
        }
    }
    assert!(refused, "the unit existed before the research ended");
    assert!(world.player_upgrades[0]
        .completed_upgrade_slugs
        .contains("overclock_stim"));
    Ok(())
}
