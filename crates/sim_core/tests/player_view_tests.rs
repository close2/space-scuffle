//! `PlayerView`, `preview_site` and knowledge-based admission, on the real skirmish world.
//!
//! # Responsibility
//! Proves what a player's interface is told, and that it is the truth the simulation acts on.
//! The view must hold no enemy data and change nothing. Every option it offers must match what
//! the real command does. Admission must answer from what the player knows, so a refusal is
//! never a remote detector, and the truth must arrive with the unit.
//!
//! # How it fits
//! Plan WP1. This is the headless half of fog: the client's HUD reads this record and never the
//! world, so these tests stand in for it.
//!
//! # Invariants
//! - Scenario, map, loadouts and catalogs are the shipped files, embedded at compile time.
//! - Behaviour is driven through `PlayerCommand`. Setup may set money, place entities or draw
//!   down nodes, as other behavioural tests do, and never during what a test is measuring.
//! - Slugs are read from data here. Simulation source names none of them.

use sim_core::economy::{GameCommandError, GameCommandReceipt, TaskEventKind};
use sim_core::production::construction::BuildRejection;
use sim_core::production::ProductionAdmissionError;
use sim_core::scenario::{build_world_with_catalogs, ScenarioManifest, StartLoadoutCatalog};
use sim_core::{SimWorld, UnitArchetype, UnitCatalog, UpgradeCatalog};
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, PlacementFailureReason, ResourceNode};
use sim_types::{
    CommandOptionView, CommandSubject, EntityId, OptionLock, PlayerCommand, PlayerId, PlayerSlot,
    PlayerView, QueuedItem, ResourceKind, ResourceNodeId, ShapeKind, SimScalar, SimVec2,
    SiteRefusal, Tick, TickBundle, UnitOrder, UnitTypeId, VisibilityState,
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
/// More money than any test spends, so that money never decides an outcome.
const RICH: u32 = 1_000_000;
/// Minimum-corner cells of the home player's buildings, on open plateau clear of its nodes.
const TRAINING_SITE: (u32, u32) = (2, 2);
const MODULE_SITE: (u32, u32) = (2, 7);
const WALK_LIMIT: u32 = 6_000;

type TestResult = Result<(), Box<dyn Error>>;

fn skirmish() -> Result<SimWorld, Box<dyn Error>> {
    let manifest = ScenarioManifest::from_ron_str(SCENARIO)?;
    let document = MapFile::from_ron_str(MAP)?.decode()?;
    let loadouts = StartLoadoutCatalog::from_ron_str(LOADOUTS)?;
    Ok(build_world_with_catalogs(
        &document,
        &loadouts,
        &manifest.players,
        3,
        UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?,
        UpgradeCatalog::from_ron_str(UPGRADES)?,
    )?)
}

fn type_of(world: &SimWorld, slug: &str) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world.catalog.id_by_slug(slug).ok_or(slug.to_owned())?)
}

fn command(player: PlayerId, entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities,
        order,
        queue: false,
    }
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let bundle = TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    };
    world.step(&bundle);
}

fn idle(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        step(world, Vec::new());
    }
}

/// Steps until `done`, failing on any diagnostic and at the limit.
fn until(
    world: &mut SimWorld,
    limit: u32,
    done: impl Fn(&SimWorld) -> bool,
) -> Result<(), Box<dyn Error>> {
    for _ in 0..limit {
        if done(world) {
            return Ok(());
        }
        step(world, Vec::new());
        if !world.diagnostics().is_empty() {
            return Err(format!("{:?}", world.diagnostics()).into());
        }
    }
    Err(format!("not done after {limit} ticks").into())
}

/// Issues one game command and returns what admission said.
fn admit(
    world: &mut SimWorld,
    player: PlayerId,
    entities: Vec<EntityId>,
    order: UnitOrder,
) -> Result<Result<GameCommandReceipt, GameCommandError>, Box<dyn Error>> {
    step(world, vec![command(player, entities, order)]);
    let outcome = world.game_command_outcomes().first();
    Ok(outcome.ok_or("the command left no outcome")?.result.clone())
}

fn view_of(world: &SimWorld, viewer: PlayerId) -> PlayerView {
    let mut view = PlayerView::default();
    world.write_player_view(viewer, &mut view);
    view
}

fn builders(world: &SimWorld, owner: PlayerId) -> Vec<EntityId> {
    world
        .entities
        .values()
        .filter(|entity| entity.owner == owner)
        .filter(|entity| {
            world
                .catalog
                .get(entity.unit_type_id)
                .is_ok_and(|arch| arch.capabilities.can_construct)
        })
        .map(|entity| entity.id)
        .collect()
}

/// The first of `owner`'s structures, with the minimum corner of its footprint.
fn base_of(world: &SimWorld, owner: PlayerId) -> Result<(EntityId, (u32, u32)), Box<dyn Error>> {
    let found = world
        .map
        .structures()
        .iter()
        .find(|(id, _)| world.entities[id].owner == owner)
        .map(|(id, placed)| (*id, placed.top_left));
    Ok(found.ok_or("the player has no structure")?)
}

fn nodes_of_side(world: &SimWorld, home: bool, kind: ResourceKind) -> Vec<ResourceNodeId> {
    (0_u32..)
        .zip(world.map.resources())
        .filter(|(_, node)| node.kind == kind && (node.cell.0 < 32) == home)
        .map(|(index, _)| ResourceNodeId(index))
        .collect()
}

fn sees_node(world: &SimWorld, player: PlayerId, node: ResourceNodeId) -> bool {
    let (x, y) = world.map.resources()[node.0 as usize].cell;
    world
        .vision_maps
        .get(player)
        .is_some_and(|vision| vision.visibility_at(x, y) == VisibilityState::Visible)
}

/// Exhausts every node on the far side, so a worker there finds nothing to move on to.
fn exhaust_far_side(world: &mut SimWorld) {
    for node in world.map.resources_mut() {
        if node.cell.0 >= 32 {
            node.remaining = 0;
        }
    }
}

/// How far along the shipped tech path a stage of the agreement test is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Stage {
    Opening,
    TierLadder,
    FoundationDown,
    TrainingDone,
    ModuleDone,
    Researching,
    Researched,
}

fn finished(world: &SimWorld, unit_type: UnitTypeId) -> bool {
    world
        .entities
        .values()
        .any(|e| e.owner == HOME && e.unit_type_id == unit_type && e.is_completed)
}

fn first_of(world: &SimWorld, unit_type: UnitTypeId) -> Result<EntityId, Box<dyn Error>> {
    let found = world
        .entities
        .values()
        .find(|e| e.owner == HOME && e.unit_type_id == unit_type);
    Ok(found.ok_or("no such entity")?.id)
}

fn build_at(unit_type: UnitTypeId, (cell_x, cell_y): (u32, u32)) -> UnitOrder {
    UnitOrder::Build {
        structure_type: unit_type,
        cell_x,
        cell_y,
    }
}

/// The first cell of the home side where `viewer` could put `structure`.
fn open_site(world: &SimWorld, structure: UnitTypeId) -> Result<(u32, u32), Box<dyn Error>> {
    let found = (0..32)
        .flat_map(|y| (0..32).map(move |x| (x, y)))
        .find(|&(x, y)| world.preview_site(HOME, structure, x, y).is_ok());
    Ok(found.ok_or("no open site")?)
}

/// A fresh world at `stage`, reached by replaying the same commands every time.
fn world_at(stage: Stage) -> Result<SimWorld, Box<dyn Error>> {
    let mut world = skirmish()?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    world.player_resources[PlayerSlot::ALL[0]].gas = RICH;
    idle(&mut world, 1);
    if stage == Stage::Opening {
        return Ok(world);
    }
    if stage == Stage::TierLadder {
        // Setup: the structure that researches tiers is placed directly, because the tier rule
        // needs only the structure and not the long road to it.
        let lab = type_of(&world, "mechanics_lab_dome")?;
        let (x, y) = open_site(&world, lab)?;
        world.spawn_structure(lab, HOME, x, y)?;
        idle(&mut world, 1);
        return Ok(world);
    }
    let training = type_of(&world, "assembly_bay_dome")?;
    let module = type_of(&world, "tech_module_pod")?;
    let crew = builders(&world, HOME);
    let order = build_at(training, TRAINING_SITE);
    step(&mut world, vec![command(HOME, vec![crew[0]], order)]);
    until(&mut world, WALK_LIMIT, |w| first_of(w, training).is_ok())?;
    if stage == Stage::FoundationDown {
        return Ok(world);
    }
    until(&mut world, WALK_LIMIT, |w| finished(w, training))?;
    if stage == Stage::TrainingDone {
        return Ok(world);
    }
    step(
        &mut world,
        vec![command(HOME, vec![crew[1]], build_at(module, MODULE_SITE))],
    );
    until(&mut world, WALK_LIMIT, |w| finished(w, module))?;
    if stage == Stage::ModuleDone {
        return Ok(world);
    }
    let lab = first_of(&world, training)?;
    let upgrade = world
        .upgrade_catalog
        .get_by_slug("overclock_stim")
        .ok_or("shipped upgrade")?
        .upgrade_id;
    step(
        &mut world,
        vec![command(HOME, vec![lab], UnitOrder::Research { upgrade })],
    );
    if stage == Stage::Researching {
        return Ok(world);
    }
    until(&mut world, WALK_LIMIT, |w| {
        w.player_upgrades[PlayerSlot::ALL[0]]
            .completed_upgrade_slugs
            .contains("overclock_stim")
    })?;
    Ok(world)
}

/// What a real command did, in the view's terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Accepted,
    OutOfSupply,
    Locked(OptionLock),
}

fn production_verdict(error: &ProductionAdmissionError) -> Result<Verdict, String> {
    match error {
        ProductionAdmissionError::Supply { .. } => Ok(Verdict::OutOfSupply),
        ProductionAdmissionError::MissingPrerequisite(missing) => {
            Ok(Verdict::Locked(OptionLock::MissingStructure(*missing)))
        }
        ProductionAdmissionError::UpgradeAlreadyResearched(_) => {
            Ok(Verdict::Locked(OptionLock::Researched))
        }
        ProductionAdmissionError::UpgradeAlreadyQueued(_) => {
            Ok(Verdict::Locked(OptionLock::Researching))
        }
        ProductionAdmissionError::UpgradeTierLocked(_) => {
            Ok(Verdict::Locked(OptionLock::TierLocked))
        }
        other => Err(format!("no view of {other:?}")),
    }
}

/// Everything in the catalog that makes `option` possible, as selected entities.
fn recipients_of(world: &SimWorld, option: &CommandOptionView) -> Vec<EntityId> {
    let slug_of = |id: UnitTypeId| world.catalog.get(id).map_or("", |a| a.slug.as_str());
    let able = |arch: &UnitArchetype| match option.subject {
        CommandSubject::Train(product) => arch.trains_units.iter().any(|s| s == slug_of(product)),
        CommandSubject::Research(upgrade) => world
            .upgrade_catalog
            .get_by_id(upgrade)
            .is_some_and(|def| arch.researches_upgrades.contains(&def.slug)),
        CommandSubject::Raise(structure) => world
            .catalog
            .get(structure)
            .is_ok_and(|structure| arch.can_raise(structure)),
    };
    world
        .entities
        .values()
        .filter(|e| e.owner == HOME && e.is_completed)
        .filter(|e| world.catalog.get(e.unit_type_id).is_ok_and(able))
        .map(|e| e.id)
        .collect()
}

fn order_for(world: &SimWorld, subject: CommandSubject) -> Result<UnitOrder, Box<dyn Error>> {
    Ok(match subject {
        CommandSubject::Train(unit_type) => UnitOrder::Produce { unit_type },
        CommandSubject::Research(upgrade) => UnitOrder::Research { upgrade },
        CommandSubject::Raise(structure) => build_at(structure, open_site(world, structure)?),
    })
}

/// Issues every option of the view as its real command and compares the results. Returns what
/// was seen, so the caller can prove the test was not vacuous.
fn check_agreement(mut world: SimWorld) -> Result<Vec<Verdict>, Box<dyn Error>> {
    let view = view_of(&world, HOME);
    let mut commands = Vec::with_capacity(view.options.len());
    for option in &view.options {
        commands.push(command(
            HOME,
            recipients_of(&world, option),
            order_for(&world, option.subject)?,
        ));
    }
    step(&mut world, commands);
    let mut verdicts = Vec::with_capacity(view.options.len());
    for (index, option) in view.options.iter().enumerate() {
        let production = world
            .production_outcomes()
            .iter()
            .find(|outcome| outcome.command_index == index);
        let game = world
            .game_command_outcomes()
            .iter()
            .find(|outcome| outcome.command_index == index);
        let verdict = match (production, game) {
            (Some(outcome), _) => match &outcome.result {
                Ok(_) => Verdict::Accepted,
                Err(error) => production_verdict(error)?,
            },
            (None, Some(outcome)) => match &outcome.result {
                Ok(_) => Verdict::Accepted,
                Err(GameCommandError::Build(BuildRejection::MissingPrerequisite(missing))) => {
                    Verdict::Locked(OptionLock::MissingStructure(*missing))
                }
                Err(other) => return Err(format!("{option:?} refused: {other:?}").into()),
            },
            (None, None) => return Err(format!("{option:?} left no outcome").into()),
        };
        match option.lock {
            Some(lock) => assert_eq!(verdict, Verdict::Locked(lock), "{option:?}"),
            None => assert!(
                matches!(verdict, Verdict::Accepted | Verdict::OutOfSupply),
                "{option:?} was refused: {verdict:?}"
            ),
        }
        verdicts.push(verdict);
    }
    Ok(verdicts)
}

#[test]
fn the_view_changes_nothing() -> TestResult {
    let mut world = skirmish()?;
    let crew = builders(&world, HOME);
    let target = nodes_of_side(&world, true, ResourceKind::Mineral)[0];
    step(
        &mut world,
        vec![command(HOME, crew, UnitOrder::Harvest { target })],
    );
    idle(&mut world, 60);
    let before = (world.compute_checksum(), world.next_entity_id);
    let mut view = PlayerView::default();
    for slot in 0..=8 {
        world.write_player_view(PlayerId(slot), &mut view);
        let _ = world.preview_site(PlayerId(slot), UnitTypeId(1), 2, 2);
    }
    assert_eq!((world.compute_checksum(), world.next_entity_id), before);
    Ok(())
}

#[test]
fn a_viewer_outside_the_slots_gets_an_empty_view() -> TestResult {
    let world = skirmish()?;
    let stranger = PlayerId(8);
    let mut view = view_of(&world, HOME);
    world.write_player_view(stranger, &mut view);
    let expected = PlayerView {
        tick: Tick(world.current_tick),
        viewer: stranger,
        ..PlayerView::default()
    };
    assert_eq!(view, expected);
    let structure = type_of(&world, "assembly_bay_dome")?;
    assert_eq!(
        world.preview_site(stranger, structure, 2, 2),
        Err(SiteRefusal::InvalidRequest)
    );
    Ok(())
}

#[test]
fn a_warmed_view_keeps_its_buffers() -> TestResult {
    type Marks = [(usize, usize); 8];
    fn marks(view: &PlayerView) -> Marks {
        [
            (
                view.visibility.capacity(),
                view.visibility.as_ptr() as usize,
            ),
            (view.queues.capacity(), view.queues.as_ptr() as usize),
            (view.jobs.capacity(), view.jobs.as_ptr() as usize),
            (view.carriers.capacity(), view.carriers.as_ptr() as usize),
            (view.laden.capacity(), view.laden.as_ptr() as usize),
            (view.options.capacity(), view.options.as_ptr() as usize),
            (
                view.completed_types.capacity(),
                view.completed_types.as_ptr() as usize,
            ),
            (
                view.visible_nodes.capacity(),
                view.visible_nodes.as_ptr() as usize,
            ),
        ]
    }

    let mut world = skirmish()?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    let crew = builders(&world, HOME);
    let target = nodes_of_side(&world, true, ResourceKind::Mineral)[0];
    let (base, _) = base_of(&world, HOME)?;
    let worker = world.entities[&crew[0]].unit_type_id;
    step(
        &mut world,
        vec![
            command(HOME, crew, UnitOrder::Harvest { target }),
            command(HOME, vec![base], UnitOrder::Produce { unit_type: worker }),
        ],
    );
    let mut view = PlayerView::default();
    // Warm through a whole mining trip, with every worker laden at some point.
    for _ in 0..600 {
        step(&mut world, Vec::new());
        world.write_player_view(HOME, &mut view);
    }
    let warmed = marks(&view);
    for _ in 0..500 {
        step(&mut world, Vec::new());
        world.write_player_view(HOME, &mut view);
        assert_eq!(marks(&view), warmed, "tick {}", world.current_tick);
    }
    Ok(())
}

#[test]
fn no_enemy_data_reaches_the_view() -> TestResult {
    let mut world = skirmish()?;
    world.player_resources[PlayerSlot::ALL[1]].minerals = RICH;
    let enemy_crew = builders(&world, AWAY);
    let (enemy_base, _) = base_of(&world, AWAY)?;
    let worker = world.entities[&enemy_crew[0]].unit_type_id;
    let target = nodes_of_side(&world, false, ResourceKind::Mineral)[0];
    let carrier_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.transport_capacity > 0)
        .ok_or("no carrier in the catalog")?
        .unit_type_id;
    let hall = world.entities[&enemy_base].position;
    let carrier = world.spawn_unit(carrier_type, AWAY, hall)?;
    step(
        &mut world,
        vec![
            command(AWAY, enemy_crew, UnitOrder::Harvest { target }),
            command(
                AWAY,
                vec![enemy_base],
                UnitOrder::Produce { unit_type: worker },
            ),
        ],
    );
    assert!(view_of(&world, AWAY).queue_of(enemy_base).is_some());
    assert!(view_of(&world, HOME).queue_of(enemy_base).is_none());
    until(&mut world, WALK_LIMIT, |w| {
        w.entities
            .values()
            .any(|e| e.owner == AWAY && e.cargo.is_some())
    })?;

    let enemy = view_of(&world, AWAY);
    assert_ne!(enemy.laden.len(), 0);
    assert_eq!(enemy.carriers[0].entity, carrier);

    let home = view_of(&world, HOME);
    assert!(home.queues.iter().all(|q| q.entity != enemy_base));
    assert!(home.laden.iter().all(|id| world.entities[id].owner == HOME));
    assert_eq!(home.carriers.len(), 0);
    assert_eq!(
        home.status.minerals,
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        "the status is the viewer's own"
    );
    assert_eq!(
        home.visibility,
        world.vision_maps[PlayerSlot::ALL[0]].states()
    );
    Ok(())
}

#[test]
fn the_view_reports_the_viewers_own_queue_cargo_and_carrier() -> TestResult {
    let mut world = skirmish()?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    let crew = builders(&world, HOME);
    let (base, _) = base_of(&world, HOME)?;
    let worker_type = world.entities[&crew[0]].unit_type_id;
    let target = nodes_of_side(&world, true, ResourceKind::Mineral)[0];
    let carrier_type = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.transport_capacity > 0)
        .ok_or("no carrier in the catalog")?;
    let capacity = carrier_type.capabilities.transport_capacity;
    let carrier_type = carrier_type.unit_type_id;
    let passenger = crew[1];
    let carrier = world.spawn_unit(carrier_type, HOME, world.entities[&passenger].position)?;
    step(
        &mut world,
        vec![
            command(HOME, vec![crew[0]], UnitOrder::Harvest { target }),
            command(HOME, vec![passenger], UnitOrder::Load { carrier }),
            command(
                HOME,
                vec![base],
                UnitOrder::Produce {
                    unit_type: worker_type,
                },
            ),
            command(
                HOME,
                vec![base],
                UnitOrder::Produce {
                    unit_type: worker_type,
                },
            ),
        ],
    );
    let view = view_of(&world, HOME);
    let queue = view.queue_of(base).ok_or("the base is training")?;
    assert_eq!(queue.job_count, 2);
    let jobs = view.jobs_of(queue);
    assert_eq!(jobs[0].item, QueuedItem::Unit(worker_type));
    assert!(jobs[0].remaining_ticks <= jobs[0].total_ticks);
    assert_eq!(jobs[1].remaining_ticks, jobs[1].total_ticks);

    until(&mut world, WALK_LIMIT, |w| {
        w.entities[&passenger].is_loaded_in_transport() && w.entities[&crew[0]].cargo.is_some()
    })?;
    let view = view_of(&world, HOME);
    assert_eq!(view.laden, vec![crew[0]]);
    assert_eq!(view.carriers.len(), 1);
    assert_eq!(view.carriers[0].entity, carrier);
    assert_eq!(view.carriers[0].capacity, capacity);
    assert!(view.carriers[0].slots_used > 0);
    Ok(())
}

#[test]
fn a_dead_entity_awaiting_reap_is_absent() -> TestResult {
    let mut world = skirmish()?;
    let crew = builders(&world, HOME);
    let worker = world.entities[&crew[0]].unit_type_id;
    for id in &crew {
        world.entities.get_mut(id).ok_or("listed above")?.hp = SimScalar::ZERO;
    }
    let view = view_of(&world, HOME);
    assert!(!view.completed_types.contains(&worker));
    assert!(view
        .options
        .iter()
        .all(|o| !matches!(o.subject, CommandSubject::Raise(_))));
    Ok(())
}

#[test]
fn the_view_and_admission_agree() -> TestResult {
    let stages = [
        Stage::Opening,
        Stage::TierLadder,
        Stage::FoundationDown,
        Stage::TrainingDone,
        Stage::ModuleDone,
        Stage::Researching,
        Stage::Researched,
    ];
    let runs: Vec<Result<Vec<Verdict>, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = stages
            .iter()
            .map(|&stage| {
                scope.spawn(move || {
                    world_at(stage)
                        .and_then(check_agreement)
                        .map_err(|error| format!("{stage:?}: {error}"))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("stage panicked".to_owned()))
            })
            .collect()
    });
    let mut seen = Vec::new();
    for run in runs {
        seen.extend(run?);
    }
    // Every kind of answer occurred, so the comparison above was not vacuous.
    let saw = |test: fn(&Verdict) -> bool| seen.iter().any(test);
    assert!(saw(|v| *v == Verdict::Accepted));
    assert!(saw(|v| matches!(
        v,
        Verdict::Locked(OptionLock::MissingStructure(_))
    )));
    assert!(saw(|v| *v == Verdict::Locked(OptionLock::TierLocked)));
    assert!(saw(|v| *v == Verdict::Locked(OptionLock::Researching)));
    assert!(saw(|v| *v == Verdict::Locked(OptionLock::Researched)));
    Ok(())
}

#[test]
fn the_view_tracks_the_tech_path() -> TestResult {
    let world = world_at(Stage::Researched)?;
    let view = view_of(&world, HOME);
    assert!(view
        .options
        .windows(2)
        .all(|pair| pair[0].subject < pair[1].subject));
    assert!(view
        .completed_types
        .windows(2)
        .all(|pair| pair[0] < pair[1]));
    let soldier = type_of(&world, "pulse_bot")?;
    let train = view
        .options
        .iter()
        .find(|o| o.subject == CommandSubject::Train(soldier))
        .ok_or("the finished training structure offers its soldier")?;
    let arch = world.catalog.get(soldier)?;
    assert_eq!(
        (train.minerals, train.gas, train.supply),
        (arch.mineral_cost, arch.gas_cost, arch.supply_cost)
    );
    // The soldier is not on the field yet, so its overclock is not a thing this player owns.
    assert!(!view.overclock_ready.contains(&soldier));
    Ok(())
}

#[test]
fn overclock_is_ready_only_once_researched_and_owned() -> TestResult {
    let mut early = world_at(Stage::Opening)?;
    let soldier = type_of(&early, "pulse_bot")?;
    let (base, _) = base_of(&early, HOME)?;
    let beside_base = early.entities[&base].position;
    early.spawn_unit(soldier, HOME, beside_base)?;
    assert!(!view_of(&early, HOME).overclock_ready.contains(&soldier));

    let mut late = world_at(Stage::Researched)?;
    late.spawn_unit(soldier, HOME, beside_base)?;
    assert_eq!(view_of(&late, HOME).overclock_ready, vec![soldier]);
    Ok(())
}

#[test]
fn a_foundation_blocks_its_site_but_is_no_prerequisite() -> TestResult {
    let world = world_at(Stage::FoundationDown)?;
    let training = type_of(&world, "assembly_bay_dome")?;
    assert_eq!(
        world.preview_site(HOME, training, TRAINING_SITE.0, TRAINING_SITE.1),
        Err(SiteRefusal::KnownStructure)
    );
    let view = view_of(&world, HOME);
    assert!(!view.completed_types.contains(&training));
    let soldier = CommandSubject::Train(type_of(&world, "pulse_bot")?);
    assert!(view.options.iter().all(|o| o.subject != soldier));
    let plant = type_of(&world, "machining_plant_dome")?;
    let raise = view
        .options
        .iter()
        .find(|o| o.subject == CommandSubject::Raise(plant))
        .ok_or("workers can raise the plant")?;
    assert_eq!(raise.lock, Some(OptionLock::MissingStructure(training)));
    Ok(())
}

#[test]
fn a_hidden_node_reports_nothing_until_seen() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 2);
    let far = nodes_of_side(&world, false, ResourceKind::Mineral)[0];
    let near = nodes_of_side(&world, true, ResourceKind::Mineral)[0];
    let view = view_of(&world, HOME);
    assert!(view.visible_nodes.iter().any(|node| node.id == near));
    assert!(view.visible_nodes.iter().all(|node| node.id != far));

    world.map.resources_mut()[far.0 as usize].remaining = 321;
    let scout = world.entities[&builders(&world, HOME)[0]].unit_type_id;
    let spot = world.map.resources()[far.0 as usize].pos;
    world.spawn_unit(scout, HOME, spot)?;
    idle(&mut world, 2);
    let view = view_of(&world, HOME);
    let seen = view.visible_nodes.iter().find(|node| node.id == far);
    assert_eq!(seen.ok_or("the scout stands on it")?.remaining, 321);
    assert!(view
        .visible_nodes
        .windows(2)
        .all(|pair| pair[0].id < pair[1].id));
    Ok(())
}

#[test]
fn preview_ignores_a_structure_the_player_has_never_seen() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    let (_, (x, y)) = base_of(&world, AWAY)?;
    let structure = type_of(&world, "assembly_bay_dome")?;
    assert!(world.map.can_place_building(x, y, 3, 3).is_err());
    assert_eq!(world.preview_site(HOME, structure, x, y), Ok(()));
    Ok(())
}

/// Shows `HOME` the enemy base with a scout, then removes the scout. Returns the base's corner.
fn glimpse_enemy_base(world: &mut SimWorld) -> Result<(u32, u32), Box<dyn Error>> {
    let (base, corner) = base_of(world, AWAY)?;
    let scout_type = world.entities[&builders(world, HOME)[0]].unit_type_id;
    let spot = world.entities[&base].position;
    let spot = SimVec2::new(spot.x, spot.y - SimScalar::from_num(4));
    let scout = world.spawn_unit(scout_type, HOME, spot)?;
    idle(world, 3);
    let (x, y) = corner;
    let index = y as usize * world.map.width() as usize + x as usize;
    assert!(
        world.known_blocked_cells[PlayerSlot::ALL[0]][index],
        "the scout saw it"
    );
    world.entities.get_mut(&scout).ok_or("just spawned")?.hp = SimScalar::ZERO;
    idle(world, 3);
    assert_eq!(
        world.vision_maps[PlayerSlot::ALL[0]].visibility_at(x, y),
        VisibilityState::Fogged
    );
    Ok(corner)
}

#[test]
fn preview_refuses_a_structure_seen_once_even_in_fog() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    let (x, y) = glimpse_enemy_base(&mut world)?;
    let structure = type_of(&world, "assembly_bay_dome")?;
    assert_eq!(
        world.preview_site(HOME, structure, x, y),
        Err(SiteRefusal::KnownStructure)
    );
    Ok(())
}

#[test]
fn a_structure_gone_in_fog_stays_remembered_until_seen_empty() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    let (x, y) = glimpse_enemy_base(&mut world)?;
    // A second enemy structure keeps the match alive once the first falls out of sight.
    let structure = type_of(&world, "assembly_bay_dome")?;
    world.spawn_structure(structure, AWAY, 40, 56)?;
    let (base, _) = base_of(&world, AWAY)?;
    world.entities.get_mut(&base).ok_or("listed above")?.hp = SimScalar::ZERO;
    idle(&mut world, 3);
    assert!(world.map.can_place_building(x, y, 3, 3).is_ok());
    assert_eq!(
        world.preview_site(HOME, structure, x, y),
        Err(SiteRefusal::KnownStructure)
    );
    let crew = builders(&world, HOME);
    let order = build_at(structure, (x, y));
    assert_eq!(
        admit(&mut world, HOME, crew, order)?,
        Err(GameCommandError::Build(BuildRejection::RememberedStructure))
    );

    // Seeing the cell empty clears the memory, and with it the refusal.
    let scout_type = world.entities[&builders(&world, HOME)[0]].unit_type_id;
    let spot = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
    world.spawn_unit(scout_type, HOME, spot)?;
    idle(&mut world, 3);
    assert_eq!(world.preview_site(HOME, structure, x, y), Ok(()));
    Ok(())
}

#[test]
fn preview_refuses_the_players_own_structure_on_tick_zero() -> TestResult {
    let world = skirmish()?;
    assert_eq!(world.current_tick, 0);
    let (_, (x, y)) = base_of(&world, HOME)?;
    let structure = type_of(&world, "assembly_bay_dome")?;
    assert_eq!(
        world.preview_site(HOME, structure, x, y),
        Err(SiteRefusal::KnownStructure)
    );
    Ok(())
}

#[test]
fn preview_judges_public_terrain_and_nodes_at_any_range() -> TestResult {
    let world = skirmish()?;
    let structure = type_of(&world, "assembly_bay_dome")?;
    let extractor = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.is_gas_extractor)
        .ok_or("no extractor in the catalog")?
        .unit_type_id;
    let soldier = type_of(&world, "pulse_bot")?;
    let width = world.map.width();

    assert_eq!(
        world.preview_site(HOME, structure, width, 0),
        Err(SiteRefusal::OutOfBounds)
    );
    assert_eq!(
        world.preview_site(HOME, soldier, 2, 2),
        Err(SiteRefusal::InvalidRequest)
    );
    assert_eq!(
        world.preview_site(HOME, UnitTypeId(u16::MAX), 2, 2),
        Err(SiteRefusal::InvalidRequest)
    );
    let far = nodes_of_side(&world, false, ResourceKind::Mineral)[0];
    let (x, y) = world.map.resources()[far.0 as usize].cell;
    assert_eq!(
        world.preview_site(HOME, structure, x, y),
        Err(SiteRefusal::CoversResource)
    );
    assert_eq!(
        world.preview_site(HOME, extractor, x, y),
        Err(SiteRefusal::ExtractorNeedsGas)
    );
    assert_eq!(
        world.preview_site(HOME, extractor, TRAINING_SITE.0, TRAINING_SITE.1),
        Err(SiteRefusal::ExtractorNeedsGas)
    );
    assert!(world.preview_site(HOME, extractor, 13, 5).is_ok());
    Ok(())
}

#[test]
fn a_build_on_a_hidden_structure_is_accepted_and_abandoned_on_arrival() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    let (_, site) = base_of(&world, AWAY)?;
    let structure = type_of(&world, "assembly_bay_dome")?;
    let builder = builders(&world, HOME)[0];
    let structures_before = world.map.structures().len();
    // The enemy's workers would otherwise answer an intruder before it reached the site.
    for enemy in world.entities.values_mut() {
        if enemy.owner == AWAY && !enemy.is_immobile {
            enemy.hp = SimScalar::ZERO;
        }
    }

    let order = build_at(structure, site);
    assert_eq!(
        admit(&mut world, HOME, vec![builder], order)?,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );

    let mut abandoned = None;
    for _ in 0..WALK_LIMIT {
        step(&mut world, Vec::new());
        assert!(world.diagnostics().is_empty(), "{:?}", world.diagnostics());
        if let Some(event) = world.task_events().iter().find(|e| e.entity == builder) {
            abandoned = Some(event.kind.clone());
            break;
        }
    }
    assert!(
        matches!(
            abandoned,
            Some(TaskEventKind::BuildAbandoned(BuildRejection::Blocked(
                PlacementFailureReason::OccupiedByStructure(_)
            )))
        ),
        "{abandoned:?}"
    );
    assert_eq!(
        world.player_resources[PlayerSlot::ALL[0]].minerals,
        RICH,
        "nothing was paid"
    );
    assert_eq!(world.map.structures().len(), structures_before);
    assert!(!matches!(
        world.entities[&builder].order_queue.front(),
        Some(UnitOrder::Build { .. })
    ));
    Ok(())
}

#[test]
fn an_exhausted_hidden_node_takes_the_order_and_ends_it_on_arrival() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    exhaust_far_side(&mut world);
    let target = nodes_of_side(&world, false, ResourceKind::Mineral)[0];
    let worker = builders(&world, HOME)[0];
    assert!(!sees_node(&world, HOME, target));

    assert_eq!(
        admit(
            &mut world,
            HOME,
            vec![worker],
            UnitOrder::Harvest { target }
        )?,
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    );

    // The news waits for the worker: nothing is said while the node is out of sight.
    let mut ended = false;
    for _ in 0..WALK_LIMIT {
        step(&mut world, Vec::new());
        assert!(world.diagnostics().is_empty(), "{:?}", world.diagnostics());
        let event = world.task_events().iter().find(|e| e.entity == worker);
        if let Some(event) = event {
            assert_eq!(event.kind, TaskEventKind::ResourceExhausted);
            assert!(
                sees_node(&world, HOME, target),
                "reported before it was seen"
            );
            ended = true;
            break;
        }
        assert_eq!(
            world.entities[&worker].order_queue.front(),
            Some(&UnitOrder::Harvest { target })
        );
    }
    assert!(ended, "the worker never reached the node");
    Ok(())
}

#[test]
fn an_exhausted_visible_node_is_refused() -> TestResult {
    let mut world = skirmish()?;
    idle(&mut world, 3);
    let target = nodes_of_side(&world, true, ResourceKind::Mineral)[0];
    assert!(sees_node(&world, HOME, target));
    world.map.resources_mut()[target.0 as usize].remaining = 0;
    let worker = builders(&world, HOME)[0];
    assert_eq!(
        admit(
            &mut world,
            HOME,
            vec![worker],
            UnitOrder::Harvest { target }
        )?,
        Err(GameCommandError::ResourceExhausted(target))
    );
    Ok(())
}

#[test]
fn can_raise_matches_admission_for_every_catalog_pair() -> TestResult {
    const SIDE: u32 = 160;
    const GAS_CELL: u32 = 150;
    let gas_at = SimScalar::from_num(GAS_CELL) + SimScalar::HALF;
    let map = GameMap::from_cells(
        SIDE,
        SIDE,
        vec![MapCell::default_ground(CliffLevel(0)); (SIDE * SIDE) as usize],
        vec![ResourceNode {
            pos: SimVec2::new(gas_at, gas_at),
            kind: ResourceKind::Gas,
            remaining: 100,
            cell: (GAS_CELL, GAS_CELL),
        }],
        Vec::new(),
    )?;
    let mut world = SimWorld::new(map, 5);
    world.catalog = UnitCatalog::from_ron_str_bundle(UNITS, STRUCTURES)?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = RICH;
    world.player_resources[PlayerSlot::ALL[0]].gas = RICH;
    let ids: Vec<UnitTypeId> = world.catalog.archetypes().map(|a| a.unit_type_id).collect();

    // Prerequisites are another rule's business: lift them so only the pair decides.
    for &id in &ids {
        world.catalog.get_mut(id)?.requires_structures.clear();
    }
    let mut members = Vec::with_capacity(ids.len());
    let mut next_site = 0;
    for &id in &ids {
        let shape = world.catalog.get(id)?.shape;
        let member = if matches!(shape, ShapeKind::BuildingDome { .. }) {
            let (x, y) = (next_site % 12 * 8, next_site / 12 * 8 + 16);
            next_site += 1;
            world.spawn_structure(id, HOME, x, y)?
        } else {
            let spot = SimScalar::from_num(4);
            world.spawn_unit(id, HOME, SimVec2::new(spot, spot))?
        };
        members.push((id, member));
    }

    let mut commands = Vec::new();
    let mut pairs = Vec::new();
    for &structure in &ids {
        let arch = world.catalog.get(structure)?;
        let site = if arch.capabilities.is_gas_extractor {
            (GAS_CELL, GAS_CELL)
        } else {
            (100, 2)
        };
        for &(member_type, member) in &members {
            pairs.push((member_type, structure));
            commands.push(command(HOME, vec![member], build_at(structure, site)));
        }
    }
    step(&mut world, commands);

    let mut raised = 0;
    for outcome in world.game_command_outcomes() {
        let (member, structure) = pairs[outcome.command_index];
        let (member, structure) = (world.catalog.get(member)?, world.catalog.get(structure)?);
        let expected = member.can_raise(structure);
        match &outcome.result {
            Ok(_) => {
                assert!(expected, "{} raised {}", member.slug, structure.slug);
                raised += 1;
            }
            Err(
                GameCommandError::NoCapableBuilder
                | GameCommandError::Build(BuildRejection::NotAStructure(_)),
            ) => assert!(!expected, "{} refused {}", member.slug, structure.slug),
            Err(other) => return Err(format!("{member:?} {other:?}").into()),
        }
    }
    assert_eq!(world.game_command_outcomes().len(), pairs.len());
    assert!(raised > 0);
    Ok(())
}
